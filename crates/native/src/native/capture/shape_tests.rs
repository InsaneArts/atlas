use super::*;
use objc2_core_foundation::{CGPoint, CGSize};

fn rect(x: f64, y: f64, width: f64, height: f64) -> CGRect {
    CGRect::new(CGPoint::new(x, y), CGSize::new(width, height))
}

#[test]
fn the_whole_picture_is_fitted_to_the_limit_and_never_enlarged() {
    assert_eq!(
        Shape::whole((2560, 1440), Some(1600)),
        Shape {
            area: None,
            width: 1600,
            height: 900
        }
    );
    assert!(Shape::whole((2560, 1440), None).is_whole((2560, 1440)));
    assert!(Shape::whole((1280, 800), Some(1600)).is_whole((1280, 800)));
}

#[test]
fn an_area_is_read_in_the_pictures_own_pixels_from_the_displays_corner() {
    let shape = Shape::of(
        rect(-1412.0, 150.0, 300.5, 200.0),
        rect(-1512.0, 100.0, 1512.0, 982.0),
        (3024, 1964),
        None,
    )
    .unwrap();
    assert_eq!(
        shape.area,
        Some(Area {
            x: 200,
            y: 100,
            width: 601,
            height: 400
        })
    );
    assert_eq!((shape.width, shape.height), (601, 400));
    assert!(!shape.is_whole((3024, 1964)));
}

#[test]
fn an_area_is_fitted_to_the_limit_and_cut_at_the_displays_edge() {
    let display = rect(0.0, 0.0, 2560.0, 1440.0);
    let shape = Shape::of(
        rect(2000.0, 1000.0, 1000.0, 1000.0),
        display,
        (2560, 1440),
        Some(200),
    )
    .unwrap();
    assert_eq!(
        shape.area,
        Some(Area {
            x: 2000,
            y: 1000,
            width: 560,
            height: 440
        })
    );
    assert_eq!((shape.width, shape.height), (200, 157));
}

#[test]
fn an_area_without_a_whole_pixel_or_outside_the_display_gives_nothing() {
    let display = rect(0.0, 0.0, 2560.0, 1440.0);
    assert_eq!(
        Shape::of(rect(10.0, 10.0, 0.2, 50.0), display, (2560, 1440), None),
        None
    );
    assert_eq!(
        Shape::of(rect(3000.0, 10.0, 50.0, 50.0), display, (2560, 1440), None),
        None
    );
    assert_eq!(
        Shape::of(
            rect(0.0, 0.0, 10.0, 10.0),
            rect(0.0, 0.0, 0.0, 0.0),
            (2560, 1440),
            None
        ),
        None
    );
}

#[test]
fn a_picture_from_the_stream_is_the_size_a_core_graphics_capture_of_it_would_be() {
    // CoreGraphics captures the display, or an area of it, at its own
    // pixels; the caller's limit then fits it. The stream's shape must
    // come out the same.
    for (pixels, bounds) in [
        ((2560, 1440), rect(0.0, 0.0, 2560.0, 1440.0)),
        ((3024, 1964), rect(-1512.0, 100.0, 1512.0, 982.0)),
    ] {
        let scale = pixels.0 as f64 / bounds.size.width;
        for limit in [None, Some(1456), Some(320)] {
            let whole = Shape::whole(pixels, limit);
            assert_eq!((whole.width, whole.height), fit(pixels.0, pixels.1, limit));
            let area = rect(
                bounds.origin.x + 300.0,
                bounds.origin.y + 100.0,
                400.0,
                300.0,
            );
            let cut = Shape::of(area, bounds, pixels, limit).unwrap();
            let captured = (
                (area.size.width * scale) as usize,
                (area.size.height * scale) as usize,
            );
            assert_eq!((cut.width, cut.height), fit(captured.0, captured.1, limit));
        }
    }
}
