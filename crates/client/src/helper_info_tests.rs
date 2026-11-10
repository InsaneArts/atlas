use super::*;

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
