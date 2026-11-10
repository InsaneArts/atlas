use super::*;
use crate::client::tests::{display, region};

#[test]
fn pruning_removes_only_old_capture_files() {
    let dir = tempfile::tempdir().unwrap();
    let old = std::time::SystemTime::now() - Duration::from_secs(8 * 24 * 60 * 60);
    let kept = [
        "notes-capture.png",
        "-capture.png",
        "340282366920938463463374607431768211456-capture.png",
        ".1-capture.png.partial",
    ];
    for name in kept.into_iter().chain(["1-capture.png"]) {
        let file = fs::File::create(dir.path().join(name)).unwrap();
        file.set_times(fs::FileTimes::new().set_modified(old))
            .unwrap();
    }
    fs::write(dir.path().join("2-capture.png"), b"recent").unwrap();
    std::os::unix::fs::symlink("notes-capture.png", dir.path().join("3-capture.png")).unwrap();
    fs::create_dir(dir.path().join("4-capture.png")).unwrap();
    fs::File::open(dir.path().join("4-capture.png"))
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(old))
        .unwrap();

    prune_captures(dir.path());

    assert!(!dir.path().join("1-capture.png").exists());
    for name in kept
        .into_iter()
        .chain(["2-capture.png", "3-capture.png", "4-capture.png"])
    {
        assert!(
            fs::symlink_metadata(dir.path().join(name)).is_ok(),
            "{name}"
        );
    }
}

#[test]
fn a_crop_maps_pixels_to_points_from_its_origin() {
    let shown = region(300, 200, 800, 600);
    let ((x, y), scale) = placement(&display(), 400, Some(&shown));

    assert_eq!(((x, y), scale), ((300.0, 200.0), 2.0));
    assert_eq!((x + 10.0 * scale, y + 20.0 * scale), (320.0, 240.0));
    assert_eq!(
        placement(&display(), 1600, Some(&shown)),
        ((300.0, 200.0), 0.5)
    );
}

#[test]
fn a_full_screenshot_starts_at_the_corner_with_todays_scale() {
    assert_eq!(placement(&display(), 2560, None), ((0.0, 0.0), 0.5));
    assert_eq!(placement(&display(), 640, None), ((0.0, 0.0), 2.0));
    assert_eq!(
        cropped(&json!({"ok": true}), None, &display()).unwrap(),
        None
    );
}

#[test]
fn an_older_helper_that_ignores_the_crop_is_refused() {
    let whole = json!({"ok": true, "png": "", "width": 2560, "height": 1440, "scale": 2.0, "offDisplayWindows": []});

    for crop in [Crop::App { pid: 42 }, Crop::Region(region(0, 0, 400, 300))] {
        let refused = cropped(&whole, Some(&crop), &display()).unwrap_err();
        assert_eq!(refused.code, "daemon", "{crop:?}");
        assert!(refused.error.contains("atlas restart"), "{crop:?}");
    }

    let mut null = whole.clone();
    null["region"] = Value::Null;
    assert_eq!(
        cropped(&null, Some(&Crop::App { pid: 42 }), &display())
            .unwrap_err()
            .code,
        "daemon"
    );
}

#[test]
fn a_crop_other_than_the_one_asked_for_is_refused() {
    let reply = |region: Value| json!({"ok": true, "region": region});
    let asked = Crop::Region(region(100, 100, 400, 300));

    let fits = reply(json!({"x": 100, "y": 100, "width": 400, "height": 300}));
    assert_eq!(
        cropped(&fits, Some(&asked), &display()).unwrap(),
        Some(Cropped {
            region: region(100, 100, 400, 300),
            app: Value::Null,
            overlapping: Value::Null,
        })
    );
    let clamped = reply(json!({"x": 100, "y": 100, "width": 100, "height": 50}));
    assert!(cropped(&clamped, Some(&asked), &display()).is_ok());

    for sent in [
        json!({"x": 90, "y": 100, "width": 400, "height": 300}),
        json!({"x": 100, "y": 100, "width": 401, "height": 300}),
        json!({"x": -1, "y": 0, "width": 10, "height": 10}),
        json!({"x": 1200, "y": 0, "width": 100, "height": 10}),
        json!({"x": 0, "y": 0, "width": 0, "height": 10}),
        json!({"x": 100.5, "y": 100, "width": 10, "height": 10}),
        json!({"x": 100, "y": 100}),
        json!("100,100,400,300"),
    ] {
        let refused = cropped(&reply(sent.clone()), Some(&asked), &display()).unwrap_err();
        assert_eq!(refused.code, "protocol", "{sent}");
    }

    let app = Crop::App { pid: 42 };
    let shown = |pid: i64, overlapping: Value| {
        json!({
            "ok": true,
            "region": {"x": 0, "y": 0, "width": 10, "height": 10},
            "app": {"pid": pid, "name": "TextEdit"},
            "overlapping": overlapping,
        })
    };
    let found = cropped(&shown(42, json!([])), Some(&app), &display())
        .unwrap()
        .unwrap();
    assert_eq!(found.app, json!({"pid": 42, "name": "TextEdit"}));
    assert_eq!(found.overlapping, json!([]));
    for (pid, overlapping) in [(43, json!([])), (42, Value::Null), (42, json!({}))] {
        let refused = cropped(&shown(pid, overlapping), Some(&app), &display()).unwrap_err();
        assert_eq!(refused.code, "protocol", "{pid}");
    }
}

#[test]
fn only_the_standard_size_asks_for_one_pixel_per_point() {
    assert_eq!(
        screenshot_request(Size::Standard),
        json!({"op": "screenshot", "maxDimension": atlas_core::screen::frame::SCREENSHOT_SIDE, "perPoint": true})
    );
    assert_eq!(
        screenshot_request(Size::Longest(atlas_core::screen::frame::SCREENSHOT_SIDE)),
        json!({"op": "screenshot", "maxDimension": atlas_core::screen::frame::SCREENSHOT_SIDE})
    );
    assert_eq!(
        screenshot_request(Size::Longest(800)),
        json!({"op": "screenshot", "maxDimension": 800})
    );
    assert_eq!(screenshot_request(Size::Full), json!({"op": "screenshot"}));
}

#[test]
fn a_crop_nobody_asked_for_is_refused() {
    let reply = json!({"ok": true, "region": {"x": 0, "y": 0, "width": 10, "height": 10}});
    let refused = cropped(&reply, None, &display()).unwrap_err();

    assert_eq!(
        (refused.code, refused.error.as_str()),
        (
            Code::Protocol,
            "The helper sent a crop that was not asked for"
        )
    );
}

#[test]
fn a_region_must_meet_the_display() {
    assert!(meets(&region(-50, -50, 200, 200), &display()));
    assert!(meets(&region(1279, 719, 10, 10), &display()));
    assert!(!meets(&region(5000, 5000, 10, 10), &display()));
    assert!(!meets(&region(1280, 0, 10, 10), &display()));
    assert!(!meets(&region(-10, 0, 10, 10), &display()));
}
