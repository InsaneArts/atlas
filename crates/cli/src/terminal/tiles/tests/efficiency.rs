//! Opt-in synthetic production-renderer measurements. No helper connection,
//! desktop capture, keyboard event, terminal graphics or input injection.
use super::super::{Tile, Tiles};
use crate::terminal::graphics::Layout;
use atlas_core::buffer::Buffer;
use image::{ImageBuffer, Rgb};
use serde_json::json;
use std::{io, process::Command, time::Instant};

type Picture = ImageBuffer<Rgb<u8>, Buffer>;

fn setting(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .map(|value| value.parse().expect("a whole-number benchmark setting"))
        .unwrap_or(default)
}

#[allow(
    unsafe_code,
    reason = "FFI: read only this benchmark thread's CPU clock"
)]
fn thread_cpu_seconds() -> f64 {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: clock_gettime writes a valid timespec to this live stack value;
    // CLOCK_THREAD_CPUTIME_ID observes only the calling benchmark thread.
    let result = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut time) };
    assert_eq!(result, 0, "the thread CPU clock must be available");
    time.tv_sec as f64 + time.tv_nsec as f64 / 1_000_000_000.0
}

fn rss_bytes() -> Option<u64> {
    let pid = std::process::id().to_string();
    let output = Command::new("/bin/ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    std::str::from_utf8(&output.stdout)
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?
        .checked_mul(1024)
}

fn retained_bytes(renderer: &Tiles) -> usize {
    renderer.previous.capacity()
        + renderer.pixels.capacity()
        + renderer.tiles.capacity() * std::mem::size_of::<Tile>()
        + renderer
            .source
            .as_ref()
            .map_or(0, |(_, image)| image.as_raw().capacity())
}

fn mutate(image: &mut Picture, workload: &str, iteration: usize) {
    match workload {
        "idle" => {}
        "edit" | "bottom-edit" => {
            let top = if workload == "bottom-edit" {
                image.height() - 24
            } else {
                24
            };
            for y in top..top + 18 {
                for x in 24..36 {
                    image.put_pixel(x, y, image::Rgb([iteration as u8, 110, 210]));
                }
            }
        }
        "scroll" => {
            let row_bytes = image.width() as usize * 3;
            let pixels: &mut [u8] = image.as_mut();
            pixels.rotate_left(row_bytes * 4);
        }
        "noise" => {
            let pixels: &mut [u8] = image.as_mut();
            for pixel in pixels {
                *pixel ^= 1;
            }
        }
        _ => panic!("unknown synthetic workload"),
    }
}

fn percentile(values: &[f64], percent: usize) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[(sorted.len() - 1) * percent / 100]
}

#[test]
#[ignore = "Explicit CPU/memory/byte qualification of synthetic RGB frames; no desktop or terminal"]
fn synthetic_renderer_efficiency_benchmark() {
    let width = setting("ATLAS_EFFICIENCY_WIDTH", 2560);
    let height = setting("ATLAS_EFFICIENCY_HEIGHT", width * 13 / 20);
    let frames = setting("ATLAS_EFFICIENCY_FRAMES", 240);
    let warmup = 10;
    let patches = setting("ATLAS_EFFICIENCY_PATCHES", 1) != 0;
    let workload = std::env::var("ATLAS_EFFICIENCY_WORKLOAD").unwrap_or_else(|_| "idle".into());
    assert!((320..=4096).contains(&width) && (64..=4096).contains(&height));
    assert!((30..=10_000).contains(&frames));
    assert!(["idle", "edit", "bottom-edit", "scroll", "noise"].contains(&workload.as_str()));
    let initial_rss = rss_bytes();
    let mut random = 19_u32;
    let pixels = Buffer::zeroed(width * height * 3).unwrap();
    let mut image = Picture::from_raw(width as u32, height as u32, pixels).unwrap();
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = Rgb(if workload == "noise" {
            std::array::from_fn(|_| {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                random as u8
            })
        } else if x % 12 < 2 && y % 20 < 12 {
            [30, 35, 40]
        } else {
            [235, 237, 240]
        })
    }
    let geometry = Layout {
        columns: 120,
        rows: 40,
    };
    let terminal = (120, 40, 1280, 832);
    let mut renderer = Tiles::default();
    renderer.set_patches(patches);
    let mut output = io::sink();
    let started = Instant::now();
    let initial_bytes = renderer
        .draw_rgb(
            &mut output,
            image.as_raw(),
            image.dimensions(),
            geometry,
            terminal,
        )
        .unwrap();
    let initial_ms = started.elapsed().as_secs_f64() * 1000.0;
    for iteration in 0..warmup {
        mutate(&mut image, &workload, iteration);
        renderer
            .draw_rgb(
                &mut output,
                image.as_raw(),
                image.dimensions(),
                geometry,
                terminal,
            )
            .unwrap();
    }
    let mut wall_ms = vec![0.0; frames];
    let mut cpu_ms = vec![0.0; frames];
    let mut encoded_bytes = vec![0_usize; frames];
    let warm_rss = rss_bytes();
    let warm_retained = retained_bytes(&renderer);
    let mut memory = Vec::with_capacity(7);
    memory.push(
        json!({"after_frames":0,"rss_bytes":warm_rss,"renderer_retained_bytes":warm_retained}),
    );
    let every = frames.div_ceil(6);
    let began = Instant::now();
    for iteration in 0..frames {
        // Source generation is outside the renderer's CPU/wall timing. Real
        // capture produces these bytes elsewhere and has its own cost.
        mutate(&mut image, &workload, iteration + warmup);
        let cpu_before = thread_cpu_seconds();
        let started = Instant::now();
        encoded_bytes[iteration] = renderer
            .draw_rgb(
                &mut output,
                image.as_raw(),
                image.dimensions(),
                geometry,
                terminal,
            )
            .unwrap();
        wall_ms[iteration] = started.elapsed().as_secs_f64() * 1000.0;
        cpu_ms[iteration] = (thread_cpu_seconds() - cpu_before) * 1000.0;
        if workload == "idle" {
            assert_eq!(encoded_bytes[iteration], 0);
        }
        if (iteration + 1) % every == 0 || iteration + 1 == frames {
            memory.push(json!({"after_frames":iteration+1,"rss_bytes":rss_bytes(),"renderer_retained_bytes":retained_bytes(&renderer)}));
        }
    }
    let elapsed = began.elapsed().as_secs_f64();
    let wall_sum = wall_ms.iter().sum::<f64>();
    let cpu_sum = cpu_ms.iter().sum::<f64>();
    let byte_sum = encoded_bytes.iter().map(|n| *n as u64).sum::<u64>();
    let cpu_mean = cpu_sum / frames as f64;
    let final_retained = retained_bytes(&renderer);
    println!(
        "{}",
        json!({
            "benchmark":"terminal-renderer-efficiency-v2","width":width,"height":height,
            "source_storage":"Buffer: private anonymous mapping above 128 KiB, fixed heap below",
            "patches":patches,"workload":workload,"warmup_frames":warmup,"measured_frames":frames,
            "initial_render_ms":initial_ms,"initial_wire_bytes":initial_bytes,
            "wall_ms_mean":wall_sum / frames as f64,"wall_ms_p50":percentile(&wall_ms,50),"wall_ms_p95":percentile(&wall_ms,95),
            "thread_cpu_ms_mean":cpu_mean,"thread_cpu_ms_p50":percentile(&cpu_ms,50),"thread_cpu_ms_p95":percentile(&cpu_ms,95),
            "renderer_cpu_only_capacity_fps":1000.0 / cpu_mean,
            "renderer_wall_only_capacity_fps":1000.0 * frames as f64 / wall_sum,
            "one_core_percent_at_10hz":cpu_mean,"one_core_percent_at_30hz":cpu_mean * 3.0,
            "wire_bytes_per_update":byte_sum as f64 / frames as f64,"wire_bytes_per_second_at_30hz":byte_sum as f64 / frames as f64 * 30.0,
            "source_rgb_bytes":image.as_raw().len(),"source_rgb_capacity_bytes":image.as_raw().capacity(),
            "initial_process_rss_bytes":initial_rss,"warm_renderer_retained_bytes":warm_retained,"final_renderer_retained_bytes":final_retained,
            "renderer_retained_growth_bytes":final_retained as i64 - warm_retained as i64,
            "memory_checkpoints":memory,"unpaced_loop_elapsed_seconds":elapsed,
            "limitations":"Only production RGB renderer to an output sink; excludes capture, proofs, source generation, viewport copies, real PTY writes and terminal decode/GPU. CPU duty is projected for a requested cadence, not measured viewer FPS. RSS includes the test process/allocator; retained capacities exclude transient codec buffers and terminal-owned images."
        })
    );
}
