use super::*;

#[test]
fn sizes_fit_the_longest_side_and_keep_their_aspect() {
    assert_eq!(fit(2560, 1440, Some(320)), (320, 180));
    assert_eq!(fit(1440, 2560, Some(320)), (180, 320));
    assert_eq!(fit(2560, 1440, None), (2560, 1440));
    assert_eq!(fit(800, 600, Some(1600)), (800, 600), "never enlarged");
    assert_eq!(fit(5000, 1, Some(100)), (100, 1), "never a zero side");
}

#[test]
fn a_picture_per_point_holds_one_pixel_per_point_up_to_the_limit() {
    let sized = |pixels: (usize, usize), points: (f64, f64)| {
        fit(
            pixels.0,
            pixels.1,
            per_point(Some(1456), points.0.max(points.1)),
        )
    };

    assert_eq!(sized((2560, 1440), (2560.0, 1440.0)), (1456, 819));
    assert_eq!(sized((3024, 1964), (1512.0, 982.0)), (1456, 946));
    assert_eq!(sized((1280, 800), (1280.0, 800.0)), (1280, 800));
    assert_eq!(sized((2560, 1600), (1280.0, 800.0)), (1280, 800));
    assert_eq!(sized((602, 504), (602.0, 504.0)), (602, 504));
    assert_eq!(sized((1204, 1008), (602.0, 504.0)), (602, 504));
    assert_eq!(sized((400, 1200), (200.0, 600.0)), (200, 600));
    assert_eq!(sized((4000, 2000), (2000.0, 1000.0)), (1456, 728));
}

#[test]
fn the_per_point_limit_keeps_the_lower_bound_and_rounds_points_up() {
    assert_eq!(per_point(Some(1456), 602.0), Some(602));
    assert_eq!(per_point(Some(400), 602.0), Some(400));
    assert_eq!(per_point(None, 1512.0), Some(1512));
    assert_eq!(per_point(Some(1456), 600.5), Some(601));
    assert_eq!(per_point(Some(1456), 0.0), Some(1));
}

#[test]
fn the_bitmap_layout_keeps_its_raw_value() {
    assert_eq!(BGRA, 2 | (2 << 12));
}

#[test]
fn a_bitmap_scales_and_encodes_as_png() {
    assert!(srgb().is_some(), "the sRGB name resolves");

    // A real image from a bitmap context stands in for a display capture.
    let context = bitmap(64, 36).unwrap();
    let image = CGBitmapContextCreateImage(Some(&context)).unwrap();
    let small = scaled(&image, 32, 18).unwrap();
    let encoded = png(&small).unwrap();
    let (width, height) = atlas_core::screen::png::png_dimensions(&encoded).unwrap();

    assert_eq!((width, height), (32, 18));
}

#[test]
fn the_same_pixels_encode_to_the_same_png() {
    let plain = || png(&banded(640, 360, 24)).unwrap();
    assert_eq!(plain(), plain());
    assert_ne!(plain(), sample_png(640, 360), "different pixels differ");

    let shrunk = || png(&scaled(&banded(640, 360, 24), 320, 180).unwrap()).unwrap();
    assert_eq!(shrunk(), shrunk());
}

#[test]
fn a_png_becomes_a_smaller_jpeg_of_the_same_aspect() {
    let context = bitmap(640, 360).unwrap();
    let image = CGBitmapContextCreateImage(Some(&context)).unwrap();
    let png = png(&image).unwrap();

    let (small, width, height) = jpeg(&png, 320, 0.6).unwrap();
    assert_eq!((width, height), (320, 180));
    assert_eq!(&small[..3], &[0xFF, 0xD8, 0xFF], "a JPEG's start of image");

    let (_, width, _) = jpeg(&png, 4096, 0.6).unwrap();
    assert_eq!(width, 640, "never enlarged");
    assert_eq!(
        jpeg(b"not a picture", 320, 0.6).unwrap_err().code,
        "capture"
    );
}
