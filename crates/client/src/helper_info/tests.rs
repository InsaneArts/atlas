use super::*;
use atlas_core::screen::geometry::Bounds;

fn listed(apps: &Value, windows: &[Window]) -> Result<Vec<Value>> {
    open_apps(
        &AppList::decode(json!({"ok": true, "apps": apps})).unwrap(),
        windows,
    )
}

fn window(number: i64, pid: i64, title: String, frame: [f64; 4]) -> Window {
    let [x, y, width, height] = frame;
    Window {
        number,
        pid,
        owner: None,
        title: Some(title),
        layer: 0,
        alpha: 1.0,
        bounds: Bounds {
            x,
            y,
            width,
            height,
        },
    }
}

#[test]
fn a_frame_is_a_jpeg_within_the_budget_named_by_its_content() {
    let png = atlas_native::native::capture::sample_png(2560, 1440);
    let first = frame(&png).unwrap();
    assert_eq!(first["mimeType"], "image/jpeg");
    assert_eq!(
        (first["width"].clone(), first["height"].clone()),
        (json!(1024), json!(576))
    );
    assert!(first["data"].as_str().unwrap().len() <= FRAME_BUDGET);
    assert_eq!(first["id"].as_str().unwrap().len(), 16);
    assert_eq!(
        frame(&png).unwrap()["id"],
        first["id"],
        "the same screen, the same id"
    );
    assert_eq!(base64_length(4), 8);
}

#[test]
fn apps_list_front_to_back_with_their_windows_and_bundles() {
    let apps = json!([
        {"name": "Finder", "pid": 1, "policy": "regular", "executable": "/System/Library/CoreServices/Finder.app/Contents/MacOS/Finder"},
        {"name": "loginwindow", "pid": 2, "policy": "accessory", "executable": "/x"},
        {"name": "Opie", "pid": 3, "policy": "regular", "active": true, "executable": "/Applications/Opie.app/Contents/MacOS/Opie"},
        {"name": "Notes", "pid": 4, "policy": "regular", "executable": "/System/Applications/Notes.app/Contents/MacOS/Notes"},
    ]);
    let windows = [
        window(11, 3, "Starter".into(), [0.0, 25.0, 1080.0, 590.0]),
        window(12, 4, "QA checklist".into(), [40.0, 60.0, 900.0, 640.0]),
        window(13, 3, "Ring".into(), [700.0, 80.0, 520.0, 860.0]),
    ];

    let listed = listed(&apps, &windows).unwrap();
    let order = listed
        .iter()
        .map(|app| app["name"].clone())
        .collect::<Vec<_>>();
    assert_eq!(order, [json!("Opie"), json!("Notes"), json!("Finder")]);
    assert_eq!(listed[0]["active"], true);
    assert_eq!(listed[0]["bundle"], "/Applications/Opie.app");
    assert_eq!(
        listed[0]["windows"][1],
        json!({"id": 13, "title": "Ring", "x": 700.0, "y": 80.0, "width": 520.0, "height": 860.0, "order": 2})
    );
    assert_eq!(
        listed[2]["windows"],
        json!([]),
        "Finder, without a window, last"
    );
}

#[test]
fn repeated_app_process_ids_are_refused_before_joining_windows() {
    let app = json!({"name": "Notes", "pid": 4, "policy": "regular"});
    let apps = json!([app, app]);
    let windows = [window(1, 4, "Notes".into(), [0.0; 4])];
    assert_eq!(listed(&apps, &windows).unwrap_err().code, "apps");
}

#[test]
fn app_lists_bound_total_entries_and_text_before_copying_them() {
    let apps = json!([{"name": "Notes", "pid": 4, "policy": "regular"}]);
    let windows = vec![window(1, 4, "x".repeat(FRAME_LIMIT / 2), [0.0; 4]); 2];
    assert_eq!(listed(&apps, &windows).unwrap_err().code, "apps");
    let windows = vec![window(1, 4, String::new(), [0.0; 4]); FRAME_LIMIT / 128 + 1];
    assert_eq!(listed(&apps, &windows).unwrap_err().code, "apps");
    let apps = json!([{"pid": 4, "name": "x".repeat(FRAME_LIMIT), "policy": "regular"}]);
    assert_eq!(listed(&apps, &[]).unwrap_err().code, "apps");
}

#[test]
fn only_files_an_agents_capture_wrote_are_screenshots() {
    assert_eq!(
        capture_time("1700000000000000000-capture.png"),
        Some(1_700_000_000_000_000_000)
    );
    for name in [
        "-capture.png",
        "../1-capture.png",
        "1-capture.png.partial",
        "a1-capture.png",
        crate::activity_log::FILE,
    ] {
        assert_eq!(capture_time(name), None, "{name}");
    }

    let dir = tempfile::tempdir().unwrap();
    let second = 1_000_000_000_u128;
    let named = |at: u128| format!("{}-capture.png", at * second);
    std::fs::write(dir.path().join(named(10)), b"x").unwrap();
    std::fs::write(dir.path().join(named(40)), b"x").unwrap();
    std::fs::write(dir.path().join(crate::activity_log::FILE), b"x").unwrap();
    std::os::unix::fs::symlink("/etc/hosts", dir.path().join(named(50))).unwrap();

    let shots = captures(dir.path(), 30);
    assert_eq!(
        shots,
        [(40 * second, named(40)), (10 * second, named(10))],
        "no links, no other files, newest first"
    );
    assert_eq!(captures(dir.path(), 1).len(), 1);
}

#[test]
fn a_screenshot_name_outside_the_capture_pattern_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let png = atlas_native::native::capture::sample_png(64, 40);
    std::fs::write(dir.path().join("5-capture.png"), &png).unwrap();
    std::fs::write(dir.path().join("u.toml"), b"secret").unwrap();
    std::fs::write(dir.path().join("5-capture.png.partial"), &png).unwrap();

    assert_eq!(read_capture(dir.path(), "5-capture.png").unwrap(), png);
    assert_eq!(capture_size(dir.path(), "5-capture.png"), Some((64, 40)));
    for name in [
        "u.toml",
        "../u.toml",
        "5-capture.png.partial",
        "/etc/hosts",
        "x5-capture.png",
        "6-capture.png",
    ] {
        assert_eq!(
            read_capture(dir.path(), name).unwrap_err().code,
            "bad-request",
            "{name}"
        );
        assert_eq!(capture_size(dir.path(), name), None, "{name}");
    }
}

#[test]
fn a_linked_screenshot_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let png = atlas_native::native::capture::sample_png(64, 40);
    std::fs::write(dir.path().join("5-capture.png"), &png).unwrap();
    std::os::unix::fs::symlink(
        dir.path().join("5-capture.png"),
        dir.path().join("6-capture.png"),
    )
    .unwrap();

    assert_eq!(
        read_capture(dir.path(), "6-capture.png").unwrap_err().code,
        "bad-request"
    );
    assert_eq!(capture_size(dir.path(), "6-capture.png"), None);
}

#[test]
fn a_screenshot_that_is_not_a_plain_file_is_refused_without_waiting() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("7-capture.png")).unwrap();
    nix::unistd::mkfifo(
        &dir.path().join("8-capture.png"),
        nix::sys::stat::Mode::S_IRWXU,
    )
    .unwrap();

    for name in ["7-capture.png", "8-capture.png"] {
        assert_eq!(
            read_capture(dir.path(), name).unwrap_err().code,
            "bad-request",
            "{name}"
        );
    }
    assert!(captures(dir.path(), 30).is_empty());
}

#[test]
fn icons_come_only_from_apps_installed_where_apps_go() {
    assert!(installed(Path::new("/Applications/Opie.app")));
    assert!(installed(Path::new("/System/Applications/Notes.app")));
    assert!(installed(Path::new(
        "/System/Library/CoreServices/Finder.app"
    )));
    assert!(!installed(Path::new("/System/Library/Evil.app")));
    assert!(!installed(Path::new("/Users/Shared/Evil.app")));
    assert!(!installed(Path::new("/tmp/Evil.app")));
    assert!(!installed(Path::new("/Applications/../tmp/Evil.app")));
    assert!(!installed(Path::new("Applications/Opie.app")));

    let nested = json!([{"name": "Helper", "pid": 1, "policy": "regular",
        "executable": "/Applications/Big.app/Contents/Frameworks/Helper.app/Contents/MacOS/Helper"}]);
    assert_eq!(
        listed(&nested, &[]).unwrap()[0]["bundle"],
        "/Applications/Big.app/Contents/Frameworks/Helper.app"
    );
}
