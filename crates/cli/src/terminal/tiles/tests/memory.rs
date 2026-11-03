use super::*;

fn layout() -> Layout {
    Layout {
        columns: 120,
        rows: 40,
    }
}
fn terminal() -> (u16, u16, u16, u16) {
    (120, 40, 1280, 832)
}
fn noise(side: u32) -> Vec<u8> {
    let mut seed = 19_u32;
    (0..side as usize * side as usize * 3)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as u8
        })
        .collect()
}

#[test]
fn clearing_the_picture_releases_all_renderer_owned_pixel_and_wire_caches() {
    let mut renderer = Tiles::default();
    renderer.set_patches(true);
    renderer
        .draw_rgb(
            &mut io::sink(),
            &noise(1024),
            (1024, 1024),
            layout(),
            terminal(),
        )
        .unwrap();
    renderer.source = Some(([0; 32], RgbImage::new(64, 64)));
    renderer.pixels = vec![0; 128 * 128 * 3];
    assert!(renderer.previous.capacity() > 0);
    renderer.clear(&mut io::sink()).unwrap();
    assert!(renderer.source.is_none());
    assert_eq!(renderer.previous.capacity(), 0);
    assert_eq!(renderer.pixels.capacity(), 0);
    assert_eq!(renderer.tiles.capacity(), 0);
}

#[test]
fn a_small_picture_does_not_retain_the_previous_large_rgb_or_transport_allocation() {
    let mut renderer = Tiles::default();
    renderer.set_patches(true);
    renderer
        .draw_rgb(
            &mut io::sink(),
            &noise(1024),
            (1024, 1024),
            layout(),
            terminal(),
        )
        .unwrap();
    let small = vec![235; 320 * 208 * 3];
    renderer
        .draw_rgb(&mut io::sink(), &small, (320, 208), layout(), terminal())
        .unwrap();
    assert_eq!(renderer.previous, small);
    assert_eq!(renderer.previous.capacity(), small.len());
    assert_eq!(
        renderer
            .draw_rgb(&mut io::sink(), &small, (320, 208), layout(), terminal())
            .unwrap(),
        0
    );
}

#[test]
fn repeated_large_uploads_retain_only_the_current_rgb_comparison_baseline() {
    let mut renderer = Tiles::default();
    renderer.set_patches(false);
    let mut rgb = noise(1024);
    for _ in 0..3 {
        rgb[0] ^= 1;
        renderer
            .draw_rgb(&mut io::sink(), &rgb, (1024, 1024), layout(), terminal())
            .unwrap();
        assert_eq!(renderer.previous.len(), rgb.len());
        assert_eq!(renderer.previous.capacity(), rgb.len());
        assert!(renderer.timing.bytes > 4 * 1024 * 1024);
    }
}

#[test]
fn raw_frames_release_a_compatibility_png_cache() {
    let mut renderer = Tiles {
        source: Some(([0; 32], RgbImage::new(512, 512))),
        ..Tiles::default()
    };
    renderer
        .draw_rgb(
            &mut io::sink(),
            &vec![0; 320 * 208 * 3],
            (320, 208),
            layout(),
            terminal(),
        )
        .unwrap();
    assert!(renderer.source.is_none());
}

#[test]
fn a_failed_stream_flush_does_not_commit_pixels_or_leave_a_delta_baseline_for_retry() {
    struct FlushFailure;
    impl Write for FlushFailure {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("flush failed"))
        }
    }
    let mut renderer = Tiles::default();
    renderer.set_patches(true);
    let old = vec![0; 320 * 208 * 3];
    renderer
        .draw_rgb(&mut io::sink(), &old, (320, 208), layout(), terminal())
        .unwrap();
    let mut next = old.clone();
    next[10] = 255;
    assert!(
        renderer
            .draw_rgb(&mut FlushFailure, &next, (320, 208), layout(), terminal())
            .is_err()
    );
    assert_eq!(renderer.previous, old);
    assert!(renderer.geometry.is_none());
    let mut wire = Vec::new();
    renderer
        .draw_rgb(&mut wire, &next, (320, 208), layout(), terminal())
        .unwrap();
    assert!(
        std::str::from_utf8(&wire).unwrap().contains("a=t,"),
        "retry must upload a whole image"
    );
    assert_eq!(renderer.previous, next);
}
