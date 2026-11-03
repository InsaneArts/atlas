use super::*;

#[test]
fn fit_preserves_the_whole_source_and_changes_at_its_limits_are_noops() {
    let view = Viewport::default();
    let projection = view.projection((3024, 1964));
    assert_eq!(view.label(), "Fit");
    assert_eq!(projection.origin, (0, 0));
    assert_eq!(projection.size, (3024, 1964));
    assert!(projection.is_full());
    for change in [
        Change::Fit,
        Change::ZoomOut,
        Change::PanLeft,
        Change::PanRight,
        Change::PanUp,
        Change::PanDown,
    ] {
        assert_eq!(view.changed(change), view);
    }
}

#[test]
fn zoom_levels_have_explicit_relative_fit_labels_and_round_trip_to_fit() {
    let mut view = Viewport::default();
    for (label, factor) in [("150%", 1.5), ("200%", 2.0), ("300%", 3.0), ("400%", 4.0)] {
        let previous = view.projection((1200, 800)).generation;
        view = view.changed(Change::ZoomIn);
        assert_eq!(view.label(), label);
        assert_eq!(view.factor(), factor);
        assert_eq!(view.projection((1200, 800)).generation, previous + 1);
    }
    assert_eq!(view.changed(Change::ZoomIn), view);
    for label in ["300%", "200%", "150%", "Fit"] {
        view = view.changed(Change::ZoomOut);
        assert_eq!(view.label(), label);
    }
    let projection = view.projection((1200, 800));
    assert!(projection.is_full());
    assert_eq!(projection.generation, 8);
}

#[test]
fn panning_moves_a_quarter_of_the_viewport_and_clamps_at_every_display_edge() {
    let mut view = Viewport::default()
        .changed(Change::ZoomIn)
        .changed(Change::ZoomIn);
    assert_eq!(view.projection((1000, 800)).origin, (250, 200));
    view = view.changed(Change::PanRight).changed(Change::PanDown);
    assert_eq!(view.projection((1000, 800)).origin, (375, 300));
    for _ in 0..10 {
        view = view.changed(Change::PanRight).changed(Change::PanDown);
    }
    assert_eq!(view.projection((1000, 800)).origin, (500, 400));
    assert_eq!(view.changed(Change::PanRight), view);
    assert_eq!(view.changed(Change::PanDown), view);
    for _ in 0..10 {
        view = view.changed(Change::PanLeft).changed(Change::PanUp);
    }
    assert_eq!(view.projection((1000, 800)).origin, (0, 0));
    assert_eq!(view.changed(Change::PanLeft), view);
    assert_eq!(view.changed(Change::PanUp), view);
    assert!(view.changed(Change::Fit).projection((1000, 800)).is_full());
}

#[test]
fn fractional_and_tiny_source_geometry_always_produce_bounded_nonzero_crops() {
    for source in [
        (1, 1),
        (1, 9),
        (9, 1),
        (3, 5),
        (1001, 667),
        (u32::MAX, u32::MAX),
    ] {
        let mut view = Viewport::default();
        for _ in 0..5 {
            for _ in 0..16 {
                view = view.changed(Change::PanLeft).changed(Change::PanUp);
            }
            let top = view.projection(source);
            assert_eq!(top.origin, (0, 0));
            assert!(top.size.0 > 0 && top.size.1 > 0);
            assert!(top.valid());
            for _ in 0..16 {
                view = view.changed(Change::PanRight).changed(Change::PanDown);
            }
            let bottom = view.projection(source);
            assert_eq!(bottom.origin.0 + bottom.size.0, source.0);
            assert_eq!(bottom.origin.1 + bottom.size.1, source.1);
            assert!(bottom.valid());
            view = view.changed(Change::ZoomIn);
        }
    }
}

#[test]
fn cropping_copies_exact_rgb_rows_and_never_resamples_them() {
    let source = (5, 4);
    let mut rgb = Vec::new();
    for y in 0..source.1 {
        for x in 0..source.0 {
            rgb.extend_from_slice(&[x as u8, y as u8, (x + y) as u8]);
        }
    }
    let view = Viewport::default()
        .changed(Change::ZoomIn)
        .changed(Change::ZoomIn);
    let projection = view.projection(source);
    assert_eq!(projection.origin, (1, 1));
    assert_eq!(projection.size, (3, 2));
    assert_eq!(
        projection.crop_rgb(&rgb).unwrap(),
        vec![1, 1, 2, 2, 1, 3, 3, 1, 4, 1, 2, 3, 2, 2, 4, 3, 2, 5]
    );
    assert_eq!(
        Viewport::default()
            .projection(source)
            .crop_rgb(&rgb)
            .unwrap(),
        rgb
    );
}

#[test]
fn pointer_mapping_uses_the_actual_crop_and_full_helper_display_points() {
    let projection = Viewport::default()
        .changed(Change::ZoomIn)
        .changed(Change::ZoomIn)
        .changed(Change::PanRight)
        .projection((1000, 800));
    let layout = Layout {
        columns: 50,
        rows: 20,
    };
    assert_eq!(projection.origin, (375, 200));
    assert_eq!(
        projection.point(layout, 0, 0, (2000.0, 1600.0)),
        Some((760.0, 420.0))
    );
    assert_eq!(
        projection.point(layout, 49, 19, (2000.0, 1600.0)),
        Some((1740.0, 1180.0))
    );
    assert_eq!(projection.point(layout, 50, 0, (2000.0, 1600.0)), None);
    assert_eq!(projection.point(layout, 0, 20, (2000.0, 1600.0)), None);
    for display in [
        (f64::NAN, 100.0),
        (100.0, f64::INFINITY),
        (0.0, 100.0),
        (100.0, -1.0),
    ] {
        assert_eq!(projection.point(layout, 0, 0, display), None);
    }
}

#[test]
fn rounded_integer_crops_are_the_authority_for_fractional_zoom_mapping() {
    let projection = Viewport::default()
        .changed(Change::ZoomIn)
        .changed(Change::PanRight)
        .projection((1001, 667));
    let layout = Layout {
        columns: 67,
        rows: 45,
    };
    let point = projection.point(layout, 0, 0, (1001.0, 667.0)).unwrap();
    let expected = (
        f64::from(projection.origin.0) + f64::from(projection.size.0) / 134.0,
        f64::from(projection.origin.1) + f64::from(projection.size.1) / 90.0,
    );
    assert!((point.0 - expected.0).abs() < 1e-10);
    assert!((point.1 - expected.1).abs() < 1e-10);
}

#[test]
fn invalid_dimensions_or_rgb_lengths_are_refused_before_allocation() {
    let valid = Viewport::default().projection((2, 2));
    for bytes in [&[0; 11][..], &[0; 13][..]] {
        assert!(valid.crop_rgb(bytes).is_err());
    }
    let overflowing = Projection {
        origin: (u32::MAX, 0),
        size: (1, 1),
        source: (u32::MAX, 1),
        generation: 0,
    };
    for projection in [
        Viewport::default().projection((0, 0)),
        Viewport::default().projection((1, 0)),
        Viewport::default().projection((u32::MAX, u32::MAX)),
        overflowing,
    ] {
        assert!(projection.crop_rgb(&[]).is_err());
        if !projection.valid() {
            assert!(!projection.is_full());
            assert_eq!(
                projection.point(
                    Layout {
                        columns: 2,
                        rows: 2
                    },
                    0,
                    0,
                    (100.0, 100.0)
                ),
                None
            );
        }
    }
}

#[test]
fn source_geometry_changes_the_projection_without_changing_the_view_generation() {
    let view = Viewport::default().changed(Change::ZoomIn);
    let first = view.projection((1001, 667));
    let second = view.projection((3024, 1964));
    assert_ne!(first.size, second.size);
    assert_eq!(first.generation, second.generation);
}

#[test]
fn generation_exhaustion_cannot_reuse_an_old_generation_for_a_new_view() {
    let view = Viewport {
        generation: u64::MAX,
        ..Viewport::default()
    };
    assert_eq!(view.changed(Change::ZoomIn), view);
}
