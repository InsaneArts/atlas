use super::super::bitmap;
use super::*;
use objc2_core_graphics::CGBitmapContextCreateImage;

const INVALID: &str = "invalid layout";

/// The original scalar conversion, kept as an independent reference for
/// exact pixels and an explicit performance baseline.
fn reference_pack(bytes: &[u8], width: usize, height: usize, stride: usize) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(width * height * 3);
    for line in bytes[..stride * height].chunks_exact(stride) {
        for pixel in line[..width * 4].chunks_exact(4) {
            rgb.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
        }
    }
    rgb
}

fn color_bands(width: usize, height: usize) -> objc2_core_foundation::CFRetained<CGImage> {
    let context = bitmap(width, height).unwrap();
    for (y, red, green, blue) in [(0.0, 0.0, 0.0, 1.0), (height as f64 / 2.0, 1.0, 0.0, 0.0)] {
        CGContext::set_rgb_fill_color(Some(&context), red, green, blue, 1.0);
        CGContext::fill_rect(
            Some(&context),
            CGRect::new(
                CGPoint::new(0.0, y),
                CGSize::new(width as f64, height as f64 / 2.0),
            ),
        );
    }
    CGBitmapContextCreateImage(Some(&context)).unwrap()
}

#[test]
fn raw_pixels_match_png_color_and_top_to_bottom_orientation() {
    let image = color_bands(3, 4);
    let shot = from_image(&image, None, 2.0).unwrap();
    let png = super::super::png(&image).unwrap();
    let decoded = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
        .unwrap()
        .to_rgb8();
    assert_eq!(shot.rgb, decoded.into_raw());
    assert_eq!((shot.width, shot.height, shot.scale), (3, 4, 2.0));
    assert_eq!(&shot.rgb[..3], &[255, 0, 0], "top is red");
    assert_eq!(
        &shot.rgb[shot.rgb.len() - 3..],
        &[0, 0, 255],
        "bottom is blue"
    );
}

#[test]
fn caller_owned_large_bitmaps_keep_exact_rows_across_padded_target_sizes() {
    for (width, height) in [(257, 130), (1025, 514)] {
        let image = color_bands(width, height);
        let shot = from_image(&image, None, 2.0).unwrap();
        assert_eq!(shot.rgb.len(), width * height * 3);
        assert_eq!((shot.width, shot.height), (width as u32, height as u32));
        for (y, row) in shot.rgb.chunks_exact(width * 3).enumerate() {
            let expected = if y < height / 2 {
                [255, 0, 0]
            } else {
                [0, 0, 255]
            };
            assert!(row.chunks_exact(3).all(|pixel| pixel == expected));
        }
    }
}

#[test]
fn raw_scaling_matches_the_existing_bitmap_scaler_and_keeps_backing_scale() {
    let image = color_bands(12, 8);
    let shot = from_image(&image, Some(6), 2.0).unwrap();
    let scaled = super::super::scaled(&image, 6, 4).unwrap();
    let encoded = super::super::png(&scaled).unwrap();
    let decoded = image::load_from_memory_with_format(&encoded, image::ImageFormat::Png)
        .unwrap()
        .to_rgb8();
    assert_eq!((shot.width, shot.height, shot.scale), (6, 4, 2.0));
    assert_eq!(shot.rgb, decoded.into_raw());
    let original = from_image(&image, Some(24), 2.0).unwrap();
    assert_eq!((original.width, original.height), (12, 8), "never enlarged");
}

#[test]
fn raw_packing_skips_padding_and_preserves_row_and_channel_order() {
    let bgra = [
        3, 2, 1, 255, 6, 5, 4, 255, 99, 99, 99, 99, 9, 8, 7, 255, 12, 11, 10, 255, 88, 88, 88, 88,
    ];
    assert_eq!(
        pack(&bgra, 2, 2, 12, INVALID).unwrap(),
        (1..=12).collect::<Vec<_>>()
    );
    assert!(pack(&bgra, 2, 2, 7, INVALID).is_err(), "short stride");
    assert!(
        pack(&bgra[..23], 2, 2, 12, INVALID).is_err(),
        "truncated rows"
    );
    assert!(pack(&[], usize::MAX, 2, 12, INVALID).is_err(), "overflow");
    assert!(pack(&[], 0, 0, 0, INVALID).is_err(), "empty bitmap");
}

#[test]
fn vectorizable_packing_matches_reference_across_short_rows_and_padding() {
    for width in 1..=129 {
        for height in [1, 2, 7] {
            for padding in [0, 1, 15, 64] {
                let stride = width * 4 + padding;
                let bytes: Vec<_> = (0..stride * height + 13)
                    .map(|index| (index * 37 + index / 131) as u8)
                    .collect();
                assert_eq!(
                    pack(&bytes, width, height, stride, INVALID).unwrap(),
                    reference_pack(&bytes, width, height, stride),
                    "{width}x{height}, padding {padding}"
                );
            }
        }
    }
}

#[test]
#[ignore = "run in release mode; synthetic packing only, no display capture"]
fn benchmark_rgb_packing_against_original_conversion() {
    use std::{hint::black_box, time::Instant};
    for (width, height) in [(960, 624), (1920, 1248), (3200, 2080), (4096, 2560)] {
        let stride = (width * 4_usize).next_multiple_of(64) + 64;
        let bytes: Vec<_> = (0..stride * height)
            .map(|index| (index * 37 + index / 131) as u8)
            .collect();
        assert_eq!(
            pack(&bytes, width, height, stride, INVALID).unwrap(),
            reference_pack(&bytes, width, height, stride)
        );
        let measure = |optimized| {
            let started = Instant::now();
            for _ in 0..10 {
                if optimized {
                    black_box(pack(black_box(&bytes), width, height, stride, INVALID).unwrap());
                } else {
                    black_box(reference_pack(black_box(&bytes), width, height, stride));
                }
            }
            started.elapsed().as_secs_f64() * 100.0
        };
        let mut original = Vec::new();
        let mut optimized = Vec::new();
        for round in 0..15 {
            // Alternate order to reduce cache, thermal and load bias.
            if round % 2 == 0 {
                original.push(measure(false));
                optimized.push(measure(true));
            } else {
                optimized.push(measure(true));
                original.push(measure(false));
            }
        }
        original.sort_by(f64::total_cmp);
        optimized.sort_by(f64::total_cmp);
        eprintln!(
            "{width}x{height}: original={:.3}ms optimized={:.3}ms ratio={:.2}x",
            original[7],
            optimized[7],
            original[7] / optimized[7]
        );
    }
}

#[test]
#[ignore = "in-memory encoding comparison; no display capture"]
fn benchmark_raw_bitmap_against_png_encoding() {
    let image = color_bands(1600, 1040);
    let started = std::time::Instant::now();
    let mut raw_bytes = 0;
    for _ in 0..30 {
        raw_bytes += std::hint::black_box(from_image(&image, Some(960), 2.0).unwrap())
            .rgb
            .len();
    }
    let raw_time = started.elapsed();
    let started = std::time::Instant::now();
    let mut png_bytes = 0;
    for _ in 0..30 {
        let small = super::super::scaled(&image, 960, 624).unwrap();
        png_bytes += std::hint::black_box(super::super::png(&small).unwrap()).len();
    }
    eprintln!(
        "30 synthetic frames: raw={raw_time:?}/{raw_bytes}B, PNG={:?}/{png_bytes}B",
        started.elapsed()
    );
}
