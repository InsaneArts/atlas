use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use image::ImageEncoder;
use std::{collections::HashMap, io::Read};

fn picture() -> RgbImage {
    RgbImage::from_fn(384, 256, |x, y| {
        image::Rgb([(x % 251) as u8, (y % 251) as u8, ((x + y) % 251) as u8])
    })
}

fn encoded(image: &RgbImage, compression: image::codecs::png::CompressionType) -> Vec<u8> {
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new_with_quality(
        &mut bytes,
        compression,
        image::codecs::png::FilterType::Adaptive,
    )
    .write_image(
        &image.clone().into_raw(),
        image.width(),
        image.height(),
        image::ExtendedColorType::Rgb8,
    )
    .unwrap();
    bytes
}

struct Upload {
    keys: HashMap<String, String>,
    pixels: Vec<u8>,
}

impl Upload {
    fn number(&self, key: &str) -> u32 {
        self.keys[key].parse().unwrap()
    }
}

/// Independently decode complete Kitty transmissions, including continuation
/// chunks, instead of trusting the renderer's cached source or scratch pixels.
fn uploads(wire: &[u8]) -> Vec<Upload> {
    let wire = std::str::from_utf8(wire).unwrap();
    let mut uploads = Vec::new();
    let mut keys = HashMap::new();
    let mut payload = String::new();
    for command in wire.split("\x1b_G").skip(1) {
        let command = command.split_once("\x1b\\").unwrap().0;
        let Some((control, data)) = command.split_once(';') else {
            continue;
        };
        let chunk = control
            .split(',')
            .map(|value| {
                let (key, value) = value.split_once('=').unwrap();
                (key.to_string(), value.to_string())
            })
            .collect::<HashMap<_, _>>();
        if chunk.contains_key("s") {
            keys = chunk.clone();
            payload.clear();
        } else if keys.get("a").map(String::as_str) == Some("f") {
            assert_eq!(chunk.get("a").map(String::as_str), Some("f"));
            assert_eq!(chunk.get("q").map(String::as_str), Some("2"));
        }
        payload.push_str(data);
        if chunk.get("m").map(String::as_str) == Some("0") {
            let compressed = STANDARD.decode(&payload).unwrap();
            let mut pixels = Vec::new();
            flate2::read::ZlibDecoder::new(compressed.as_slice())
                .read_to_end(&mut pixels)
                .unwrap();
            uploads.push(Upload {
                keys: keys.clone(),
                pixels,
            });
        }
    }
    uploads
}

#[derive(Default)]
struct Scene {
    images: HashMap<u32, RgbImage>,
    current: Option<u32>,
}

impl Scene {
    fn apply(&mut self, wire: &[u8]) {
        for upload in uploads(wire) {
            let image = upload.number("i");
            let (width, height) = (upload.number("s"), upload.number("v"));
            assert_eq!(upload.pixels.len(), width as usize * height as usize * 3);
            match upload.keys["a"].as_str() {
                "t" => {
                    self.images.insert(
                        image,
                        RgbImage::from_raw(width, height, upload.pixels).unwrap(),
                    );
                }
                "f" => {
                    assert_eq!(
                        upload.number("r"),
                        1,
                        "Edits must never allocate animation frames"
                    );
                    assert_eq!(upload.number("X"), 1, "Opaque RGB edits overwrite pixels");
                    let (x, y) = (upload.number("x"), upload.number("y"));
                    let destination = self
                        .images
                        .get_mut(&image)
                        .expect("the original image canvas");
                    assert!(x + width <= destination.width() && y + height <= destination.height());
                    for row in 0..height {
                        for column in 0..width {
                            let index = (row as usize * width as usize + column as usize) * 3;
                            destination.put_pixel(
                                x + column,
                                y + row,
                                image::Rgb(upload.pixels[index..index + 3].try_into().unwrap()),
                            );
                        }
                    }
                }
                other => panic!("Unexpected upload action {other}"),
            }
        }
        let wire = std::str::from_utf8(wire).unwrap();
        for command in wire.split("\x1b_G").skip(1) {
            let command = command.split_once("\x1b\\").unwrap().0;
            if command.starts_with("a=p,") {
                self.current = command
                    .split(',')
                    .find_map(|part| part.strip_prefix("i="))
                    .map(|id| id.parse().unwrap());
            }
        }
    }

    fn picture(&self) -> &RgbImage {
        &self.images[&self.current.unwrap()]
    }
}

fn layout() -> Layout {
    Layout {
        columns: 48,
        rows: 16,
    }
}
fn terminal() -> (u16, u16, u16, u16) {
    (48, 16, 192, 128)
}

#[test]
fn fallback_places_one_complete_native_image_and_swaps_before_deleting() {
    let mut image = picture();
    let mut renderer = Tiles::default();
    let mut scene = Scene::default();
    for iteration in 0..2 {
        image.get_pixel_mut(10, 10)[1] = iteration;
        let mut wire = Vec::new();
        renderer
            .draw_rgb(
                &mut wire,
                image.as_raw(),
                image.dimensions(),
                layout(),
                terminal(),
            )
            .unwrap();
        let transfers = uploads(&wire);
        assert_eq!(transfers.len(), 1);
        assert_eq!(transfers[0].keys["a"], "t");
        assert_eq!(
            transfers[0].pixels,
            *image.as_raw(),
            "Capture pixels must not be resampled to the logical pixel report"
        );
        let text = std::str::from_utf8(&wire).unwrap();
        assert_eq!(text.matches("a=p,").count(), 1);
        assert!(!text.contains("a=f,"));
        assert_eq!(text.matches("\x1b[?2026h").count(), 1);
        assert_eq!(text.matches("\x1b[?2026l").count(), 1);
        if iteration != 0 {
            assert!(text.find("a=p,").unwrap() < text.find("a=d,").unwrap());
        }
        scene.apply(&wire);
        assert_eq!(scene.picture(), &image);
    }
}

#[test]
fn root_edits_reconstruct_the_exact_scene_without_placements_or_new_frames() {
    let mut image = RgbImage::from_fn(389, 259, |x, y| {
        image::Rgb([(x % 251) as u8, (y % 251) as u8, 50])
    });
    let mut renderer = Tiles::default();
    renderer.set_patches(true);
    let mut scene = Scene::default();
    let mut first = Vec::new();
    renderer
        .draw_rgb(
            &mut first,
            image.as_raw(),
            image.dimensions(),
            layout(),
            terminal(),
        )
        .unwrap();
    scene.apply(&first);
    let canvas = scene.current;
    for iteration in 0..3 {
        image.put_pixel(130, 132, image::Rgb([iteration, 5, 20]));
        image.put_pixel(388, 258, image::Rgb([5, iteration, 7]));
        let mut wire = Vec::new();
        renderer
            .draw_rgb(
                &mut wire,
                image.as_raw(),
                image.dimensions(),
                layout(),
                terminal(),
            )
            .unwrap();
        let transfers = uploads(&wire);
        assert_eq!(transfers.len(), 2);
        assert!(transfers.iter().all(|upload| upload.keys["a"] == "f"));
        assert_eq!(
            (transfers[0].number("x"), transfers[0].number("y")),
            (130, 132)
        );
        assert_eq!((transfers[1].number("s"), transfers[1].number("v")), (1, 1));
        let text = std::str::from_utf8(&wire).unwrap();
        assert!(!text.contains("a=t,") && !text.contains("a=p,") && !text.contains("a=d,"));
        scene.apply(&wire);
        assert_eq!(scene.current, canvas);
        assert_eq!(
            scene.images.len(),
            1,
            "Updates must never accumulate image or animation frame storage"
        );
        assert_eq!(scene.picture(), &image);
    }
}

#[test]
fn large_root_edits_repeat_the_animation_action_on_every_continuation_chunk() {
    let mut image = RgbImage::from_pixel(384, 256, image::Rgb([0, 0, 0]));
    let mut renderer = Tiles::default();
    renderer.set_patches(true);
    let mut scene = Scene::default();
    let mut first = Vec::new();
    renderer
        .draw_rgb(
            &mut first,
            image.as_raw(),
            image.dimensions(),
            layout(),
            terminal(),
        )
        .unwrap();
    scene.apply(&first);
    let mut value = 19_u32;
    for y in 128..256 {
        for x in 128..256 {
            let mut channels = [0; 3];
            for channel in &mut channels {
                value ^= value << 13;
                value ^= value >> 17;
                value ^= value << 5;
                *channel = value as u8;
            }
            image.put_pixel(x, y, image::Rgb(channels));
        }
    }
    let mut wire = Vec::new();
    renderer
        .draw_rgb(
            &mut wire,
            image.as_raw(),
            image.dimensions(),
            layout(),
            terminal(),
        )
        .unwrap();
    assert!(
        std::str::from_utf8(&wire)
            .unwrap()
            .matches("a=f,q=2,m=")
            .count()
            > 1
    );
    scene.apply(&wire);
    assert_eq!(scene.picture(), &image);
}

#[test]
fn geometry_changes_after_a_patch_upload_a_fresh_whole_image() {
    let mut image = picture();
    let mut renderer = Tiles::default();
    renderer.set_patches(true);
    let mut scene = Scene::default();
    let mut wire = Vec::new();
    renderer
        .draw_rgb(
            &mut wire,
            image.as_raw(),
            image.dimensions(),
            layout(),
            terminal(),
        )
        .unwrap();
    scene.apply(&wire);
    let original = scene.current;
    image.put_pixel(10, 10, image::Rgb([1, 2, 3]));
    wire.clear();
    renderer
        .draw_rgb(
            &mut wire,
            image.as_raw(),
            image.dimensions(),
            layout(),
            terminal(),
        )
        .unwrap();
    scene.apply(&wire);
    wire.clear();
    renderer
        .draw_rgb(
            &mut wire,
            image.as_raw(),
            image.dimensions(),
            Layout {
                columns: 47,
                rows: 16,
            },
            terminal(),
        )
        .unwrap();
    assert_eq!(uploads(&wire).len(), 1);
    assert_eq!(uploads(&wire)[0].keys["a"], "t");
    scene.apply(&wire);
    assert_ne!(scene.current, original);
    assert_eq!(scene.picture(), &image);
}

#[test]
fn identical_raw_frames_and_different_png_encodings_upload_nothing() {
    let image = picture();
    let fast = encoded(&image, image::codecs::png::CompressionType::Fast);
    let best = encoded(&image, image::codecs::png::CompressionType::Best);
    assert_ne!(fast, best);
    for patches in [false, true] {
        let mut renderer = Tiles::default();
        renderer.set_patches(patches);
        renderer
            .draw(&mut Vec::new(), &fast, layout(), terminal())
            .unwrap();
        let mut out = Vec::new();
        assert_eq!(
            renderer
                .draw_rgb(
                    &mut out,
                    image.as_raw(),
                    image.dimensions(),
                    layout(),
                    terminal()
                )
                .unwrap(),
            0
        );
        assert_eq!(
            renderer
                .draw(&mut out, &best, layout(), terminal())
                .unwrap(),
            0
        );
        assert!(out.is_empty());
        assert_eq!(renderer.previous, *image.as_raw());
    }
}

#[test]
fn raw_rgb_and_png_produce_identical_native_canvas_bytes() {
    let image = picture();
    let png = encoded(&image, image::codecs::png::CompressionType::Fast);
    for patches in [false, true] {
        let mut png_renderer = Tiles::default();
        let mut raw_renderer = Tiles::default();
        png_renderer.set_patches(patches);
        raw_renderer.set_patches(patches);
        let mut png_wire = Vec::new();
        let mut rgb_wire = Vec::new();
        png_renderer
            .draw(&mut png_wire, &png, layout(), terminal())
            .unwrap();
        raw_renderer
            .draw_rgb(
                &mut rgb_wire,
                image.as_raw(),
                image.dimensions(),
                layout(),
                terminal(),
            )
            .unwrap();
        assert_eq!(png_wire, rgb_wire);
        assert!(raw_renderer.source.is_none());
    }
}

#[test]
fn invalid_raw_frames_never_mutate_the_previous_canvas() {
    let mut renderer = Tiles::default();
    renderer
        .draw_rgb(&mut Vec::new(), &[10; 12], (2, 2), layout(), terminal())
        .unwrap();
    let previous = renderer.previous.to_vec();
    let geometry = renderer.geometry;
    for (bytes, size) in [
        (&[][..], (0, 0)),
        (&[0; 12][..], (2, 0)),
        (&[0; 11][..], (2, 2)),
        (&[0; 13][..], (2, 2)),
        (&[][..], (MAX_SIDE + 1, 1)),
        (&[][..], (1, MAX_SIDE + 1)),
        (&[][..], (u32::MAX, u32::MAX)),
    ] {
        let mut out = Vec::new();
        assert!(
            renderer
                .draw_rgb(&mut out, bytes, size, layout(), terminal())
                .is_err()
        );
        assert!(out.is_empty());
        assert_eq!(renderer.previous, previous);
        assert_eq!(renderer.geometry, geometry);
    }
}

#[test]
fn changing_most_pixels_replaces_the_canvas_instead_of_sending_many_patches() {
    let mut image = picture();
    let mut renderer = Tiles::default();
    renderer.set_patches(true);
    renderer
        .draw_rgb(
            &mut Vec::new(),
            image.as_raw(),
            image.dimensions(),
            layout(),
            terminal(),
        )
        .unwrap();
    for pixel in image.pixels_mut() {
        pixel[0] = pixel[0].wrapping_add(1);
    }
    let mut out = Vec::new();
    renderer
        .draw_rgb(
            &mut out,
            image.as_raw(),
            image.dimensions(),
            layout(),
            terminal(),
        )
        .unwrap();
    let uploads = uploads(&out);
    assert_eq!(uploads.len(), 1);
    assert_eq!(uploads[0].keys["a"], "t");
}

#[test]
fn changed_channel_bounds_include_whole_pixels_and_preserve_the_surrounding_scene() {
    let mut image = picture();
    let mut renderer = Tiles::default();
    renderer.set_patches(true);
    let mut scene = Scene::default();
    let mut wire = Vec::new();
    renderer
        .draw_rgb(
            &mut wire,
            image.as_raw(),
            image.dimensions(),
            layout(),
            terminal(),
        )
        .unwrap();
    scene.apply(&wire);
    image.get_pixel_mut(5, 9)[1] ^= 255;
    image.get_pixel_mut(92, 67)[2] ^= 255;
    wire.clear();
    renderer
        .draw_rgb(
            &mut wire,
            image.as_raw(),
            image.dimensions(),
            layout(),
            terminal(),
        )
        .unwrap();
    let patches = uploads(&wire);
    assert_eq!(patches.len(), 1);
    assert_eq!((patches[0].number("x"), patches[0].number("y")), (5, 9));
    assert_eq!((patches[0].number("s"), patches[0].number("v")), (88, 59));
    scene.apply(&wire);
    assert_eq!(scene.picture(), &image);
    wire.clear();
    assert_eq!(
        renderer
            .draw_rgb(
                &mut wire,
                image.as_raw(),
                image.dimensions(),
                layout(),
                terminal()
            )
            .unwrap(),
        0
    );
    assert!(
        wire.is_empty(),
        "The exact cropped baseline must match after the edit"
    );
}

#[test]
#[ignore = "Synthetic renderer timing only; no capture or terminal presentation latency"]
fn native_resolution_rendering_benchmark() {
    use std::time::Instant;
    let noise = std::env::var_os("ATLAS_TERMINAL_BENCH_NOISE").is_some();
    let mut seed = 19_u32;
    let mut image = RgbImage::from_fn(2560, 1664, |x, y| {
        image::Rgb(if noise {
            std::array::from_fn(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                seed as u8
            })
        } else if x % 12 < 2 && y % 20 < 12 {
            [30, 35, 40]
        } else {
            [235, 237, 240]
        })
    });
    let mut renderer = Tiles::default();
    renderer.set_patches(true);
    let started = Instant::now();
    let initial = renderer
        .draw_rgb(
            &mut std::io::sink(),
            image.as_raw(),
            image.dimensions(),
            layout(),
            terminal(),
        )
        .unwrap();
    let initial_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut changed_ms = Vec::new();
    let mut unchanged_ms = Vec::new();
    let mut changed_bytes = 0;
    for iteration in 0..10 {
        for y in 10..26 {
            for x in 10..26 {
                image.put_pixel(x, y, image::Rgb([iteration, 110, 210]));
            }
        }
        let started = Instant::now();
        changed_bytes = renderer
            .draw_rgb(
                &mut std::io::sink(),
                image.as_raw(),
                image.dimensions(),
                layout(),
                terminal(),
            )
            .unwrap();
        changed_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        let started = Instant::now();
        assert_eq!(
            renderer
                .draw_rgb(
                    &mut std::io::sink(),
                    image.as_raw(),
                    image.dimensions(),
                    layout(),
                    terminal()
                )
                .unwrap(),
            0
        );
        unchanged_ms.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    changed_ms.sort_by(f64::total_cmp);
    unchanged_ms.sort_by(f64::total_cmp);
    eprintln!(
        "whole canvas2560x1664: initial_ms={initial_ms:.2} initial_bytes={initial} change_median_ms={:.3} change_bytes={changed_bytes} unchanged_median_ms={:.3}",
        changed_ms[5], unchanged_ms[5]
    );
}

mod efficiency;
mod memory;
