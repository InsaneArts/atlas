use super::*;
use crate::{
    native::capture::{banded_png, sample_png},
    protocol::Region,
};
use std::{sync::Arc, time::Instant};

fn window_key(name: &str) -> Key {
    Key::of(crate::host_session::Host::Named, name)
}

fn window_record(output: &Path, name: &str) -> PathBuf {
    RecordFolder::at(output, &WINDOWS).path(&window_key(name))
}

fn remember_windows(output: &Path, name: &str, windows: &[HitWindow], epoch: &Epoch) -> Result<()> {
    super::remember_windows(output, &window_key(name), Some(windows), epoch)
}

fn remembered_windows(output: &Path, name: &str) -> Option<Observation> {
    super::remembered_windows(output, &window_key(name)).unwrap()
}

fn epoch() -> Epoch {
    Epoch {
        uid: 502,
        capture_pid: 123,
        display: Display {
            width: 2560.0,
            height: 1440.0,
            scale: 1.0,
        },
        security_session_id: 42,
        display_id: 7,
        input_pid: 456,
    }
}

fn frame(png: Vec<u8>) -> Frame {
    let (width, height) = crate::screen::png::png_dimensions(&png).unwrap();
    Frame {
        png: Arc::from(png),
        width,
        height,
        epoch: epoch(),
        at: Instant::now(),
    }
}

fn screen() -> Frame {
    frame(sample_png(64, 36))
}

/// The screenshot files in `output`, oldest first.
fn files(output: &Path) -> Vec<String> {
    let mut names: Vec<String> = crate::helper_info::captures(output, 1000)
        .into_iter()
        .map(|(_, name)| name)
        .collect();
    names.reverse();
    names
}

fn record(output: &Path) -> Value {
    serde_json::from_slice(&fs::read(output.join(RECORD)).unwrap()).unwrap()
}

#[test]
fn an_identical_capture_is_reported_unchanged_with_the_earlier_file() {
    let dir = tempfile::tempdir().unwrap();
    let first = keep(dir.path(), "screen 1456", &screen(), None, false).unwrap();
    let second = keep(dir.path(), "screen 1456", &screen(), None, false).unwrap();

    assert!(first.written && !first.unchanged);
    assert_eq!(
        second,
        Kept {
            path: first.path.clone(),
            unchanged: true,
            written: false,
        }
    );
    assert_eq!(files(dir.path()).len(), 1, "no second copy");
}

#[test]
fn a_capture_differing_only_in_the_menu_bar_is_a_new_file() {
    let dir = tempfile::tempdir().unwrap();
    let before = frame(banded_png(64, 36, 0));
    let ticked = frame(banded_png(64, 36, 24));
    let first = keep(dir.path(), "screen 1456", &before, None, false).unwrap();
    let second = keep(dir.path(), "screen 1456", &ticked, None, false).unwrap();

    assert!(second.written && !second.unchanged);
    assert_ne!(second.path, first.path);
    assert_eq!(fs::read(&second.path).unwrap(), &ticked.png[..]);
}

#[test]
fn a_changed_capture_is_saved_and_becomes_the_one_compared_against() {
    let dir = tempfile::tempdir().unwrap();
    let (one, two) = (frame(banded_png(64, 36, 0)), frame(banded_png(64, 36, 24)));
    keep(dir.path(), "screen 1456", &one, None, false).unwrap();
    let changed = keep(dir.path(), "screen 1456", &two, None, false).unwrap();
    let again = keep(dir.path(), "screen 1456", &two, None, false).unwrap();
    let back = keep(dir.path(), "screen 1456", &one, None, false).unwrap();

    assert!(again.unchanged && !again.written);
    assert_eq!(again.path, changed.path);
    assert!(!back.unchanged, "only the latest capture is compared");
    assert_eq!(files(dir.path()).len(), 3);
}

#[test]
fn captures_of_different_targets_or_sizes_are_compared_apart() {
    let region = Crop::Region(Region {
        x: 10,
        y: 20,
        width: 300,
        height: 200,
    });
    let keys = [
        key(Size::Standard, None),
        key(Size::Longest(1456), None),
        key(Size::Full, None),
        key(Size::Full, Some(&Crop::App { pid: 42 })),
        key(Size::Full, Some(&Crop::App { pid: 43 })),
        key(Size::Standard, Some(&Crop::App { pid: 42 })),
        key(Size::Longest(400), Some(&region)),
    ];
    assert_eq!(
        keys,
        [
            "screen standard",
            "screen 1456",
            "screen full",
            "app:42 full",
            "app:43 full",
            "app:42 standard",
            "region:10,20,300,200 400",
        ]
    );

    let dir = tempfile::tempdir().unwrap();
    let firsts: Vec<Kept> = keys
        .iter()
        .map(|key| keep(dir.path(), key, &screen(), None, false).unwrap())
        .collect();
    assert!(firsts.iter().all(|kept| !kept.unchanged && kept.written));

    for (key, first) in keys.iter().zip(&firsts) {
        let again = keep(dir.path(), key, &screen(), None, false).unwrap();
        assert_eq!(again.path, first.path, "{key}");
        assert!(again.unchanged, "{key}");
    }
}

#[test]
fn identical_crops_of_different_parts_of_the_display_are_new_files() {
    let at = |x| Region {
        x,
        y: 20,
        width: 300,
        height: 200,
    };
    let (here, moved) = (at(10), at(110));
    let dir = tempfile::tempdir().unwrap();
    let first = keep(dir.path(), "app:42 full", &screen(), Some(&here), false).unwrap();
    let second = keep(dir.path(), "app:42 full", &screen(), Some(&moved), false).unwrap();
    let again = keep(dir.path(), "app:42 full", &screen(), Some(&moved), false).unwrap();

    assert!(second.written && !second.unchanged);
    assert_ne!(second.path, first.path);
    assert!(again.unchanged && !again.written);
    assert_eq!(again.path, second.path);
}

#[test]
fn a_new_display_epoch_never_matches_an_earlier_capture() {
    let changes: [fn(&mut Epoch); 8] = [
        |epoch| epoch.uid += 1,
        |epoch| epoch.capture_pid += 1,
        |epoch| epoch.input_pid += 1,
        |epoch| epoch.security_session_id += 1,
        |epoch| epoch.display_id += 1,
        |epoch| epoch.display.width = 1920.0,
        |epoch| epoch.display.height = 1080.0,
        |epoch| epoch.display.scale = 2.0,
    ];

    for change in changes {
        let dir = tempfile::tempdir().unwrap();
        keep(dir.path(), "screen 1456", &screen(), None, false).unwrap();
        let mut moved = screen();
        change(&mut moved.epoch);

        let kept = keep(dir.path(), "screen 1456", &moved, None, false).unwrap();
        assert!(kept.written && !kept.unchanged, "{:?}", moved.epoch);
    }
}

#[test]
fn an_earlier_capture_that_is_gone_altered_or_a_link_counts_as_changed() {
    let gone = |path: &Path| fs::remove_file(path).unwrap();
    let altered = |path: &Path| fs::write(path, sample_png(36, 64)).unwrap();
    let linked = |path: &Path| {
        let copy = path.with_file_name("copy.png");
        fs::rename(path, &copy).unwrap();
        std::os::unix::fs::symlink(&copy, path).unwrap();
    };
    let tamper: [&dyn Fn(&Path); 3] = [&gone, &altered, &linked];

    for tamper in tamper {
        let dir = tempfile::tempdir().unwrap();
        let first = keep(dir.path(), "screen 1456", &screen(), None, false).unwrap();
        tamper(&first.path);

        let second = keep(dir.path(), "screen 1456", &screen(), None, false).unwrap();
        assert!(second.written && !second.unchanged);
        assert_ne!(second.path, first.path);

        let third = keep(dir.path(), "screen 1456", &screen(), None, false).unwrap();
        assert_eq!(third.path, second.path, "the new file is compared next");
    }
}

#[test]
fn an_earlier_capture_near_its_pruning_age_counts_as_changed() {
    let dir = tempfile::tempdir().unwrap();
    let first = keep(dir.path(), "screen 1456", &screen(), None, false).unwrap();
    let aged = std::time::SystemTime::now() - FRESH - Duration::from_secs(60);
    fs::File::options()
        .write(true)
        .open(&first.path)
        .unwrap()
        .set_modified(aged)
        .unwrap();

    let second = keep(dir.path(), "screen 1456", &screen(), None, false).unwrap();
    assert!(second.written && !second.unchanged);
    assert_ne!(second.path, first.path);
}

#[test]
fn asking_for_a_new_file_saves_one_even_when_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let first = keep(dir.path(), "screen 1456", &screen(), None, false).unwrap();
    let asked = keep(dir.path(), "screen 1456", &screen(), None, true).unwrap();
    let next = keep(dir.path(), "screen 1456", &screen(), None, false).unwrap();

    assert!(asked.written && asked.unchanged);
    assert_ne!(asked.path, first.path);
    assert_eq!(
        fs::read(&asked.path).unwrap(),
        fs::read(&first.path).unwrap()
    );
    assert_eq!(next.path, asked.path, "the record points at the new file");
    assert_eq!(files(dir.path()).len(), 2);
}

#[test]
fn a_damaged_capture_record_is_ignored_and_replaced() {
    let shot = screen();
    let hash = format!("{:x}", Sha256::digest(&shot.png));
    let pointing_at = |file: &str| {
        json!({"version": 1, "targets": {"screen 1456": {
            "sha256": hash, "file": file, "width": shot.width, "height": shot.height,
            "epoch": recorded_epoch(&shot.epoch), "at": "1",
        }}})
        .to_string()
    };
    let mut oversized = pointing_at("1-capture.png");
    oversized.push_str(&" ".repeat(RECORD_LIMIT as usize));
    let mut wrong_version: Value = serde_json::from_str(&pointing_at("1-capture.png")).unwrap();
    wrong_version["version"] = json!(2);

    let damaged = [
        "not json {".to_owned(),
        wrong_version.to_string(),
        pointing_at("../x"),
        pointing_at("x.png"),
        oversized,
    ];
    for damaged in damaged {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("output");
        fs::create_dir(&dir).unwrap();
        // Each earlier file a damaged record names holds the same pixels.
        for name in ["1-capture.png", "x.png"] {
            fs::write(dir.join(name), &shot.png[..]).unwrap();
        }
        fs::write(root.path().join("x"), &shot.png[..]).unwrap();
        fs::write(dir.join(RECORD), &damaged).unwrap();

        let kept = keep(&dir, "screen 1456", &shot, None, false).unwrap();
        assert!(kept.written && !kept.unchanged, "{:.40}", damaged);

        let replaced = record(&dir);
        assert_eq!(replaced["version"], 1);
        assert_eq!(
            Path::new(replaced["targets"]["screen 1456"]["file"].as_str().unwrap()),
            kept.path.strip_prefix(&dir).unwrap()
        );
    }

    // The undamaged record does match.
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("1-capture.png"), &shot.png[..]).unwrap();
    fs::write(dir.path().join(RECORD), pointing_at("1-capture.png")).unwrap();
    let kept = keep(dir.path(), "screen 1456", &shot, None, false).unwrap();
    assert!(kept.unchanged && !kept.written);
}

#[test]
fn the_capture_record_keeps_only_the_newest_targets() {
    let dir = tempfile::tempdir().unwrap();
    for index in 0..TARGETS + 8 {
        keep(
            dir.path(),
            &format!("app:{index} full"),
            &screen(),
            None,
            false,
        )
        .unwrap();
    }

    let kept = record(dir.path());
    let targets = kept["targets"].as_object().unwrap();
    assert_eq!(targets.len(), TARGETS);
    assert!((8..TARGETS + 8).all(|index| targets.contains_key(&format!("app:{index} full"))));
}

#[test]
fn capture_targets_are_read_as_entries_and_written_at_the_file_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let kept = keep(dir.path(), "screen full", &screen(), None, false).unwrap();
    let targets: BTreeMap<String, Entry> = read(dir.path());
    assert_eq!(
        targets["screen full"].file,
        kept.path.file_name().unwrap().to_str().unwrap()
    );
    write(dir.path(), targets).unwrap();
    assert_eq!(
        read(dir.path())["screen full"].file,
        record(dir.path())["targets"]["screen full"]["file"]
    );
}

#[test]
fn window_memory_uses_kernel_durability_outside_the_screenshot_directory() {
    assert!(WINDOWS.durability == Durability::Kernel);
    let dir = tempfile::tempdir().unwrap();
    remember_windows(dir.path(), "one", &[], &epoch()).unwrap();
    let entries: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].is_dir());
    assert!(remembered_windows(dir.path(), "one").is_some());
}

#[test]
fn window_memory_replaces_an_existing_observation_while_the_capture_lock_is_busy() {
    let dir = tempfile::tempdir().unwrap();
    let window = HitWindow {
        start: Some([123, 456]),
        number: 42,
        pid: 812,
        layer: 0,
        bounds: crate::screen::windows::Bounds::display(1000.0, 800.0),
    };
    remember_windows(dir.path(), "one", &[window], &epoch()).unwrap();
    let _lock = locked(dir.path()).unwrap();
    remember_windows(dir.path(), "one", &[], &epoch()).unwrap();
    assert!(
        remembered_windows(dir.path(), "one")
            .unwrap()
            .windows
            .unwrap()
            .is_empty()
    );
}

#[test]
fn window_memory_reports_denied_replacements_and_invalidation() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    remember_windows(dir.path(), "one", &[], &epoch()).unwrap();
    let path = window_record(dir.path(), "one");
    fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o500)).unwrap();
    let denied = remember_windows(dir.path(), "one", &[], &epoch());
    fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
    assert!(denied.is_err());
    assert!(path.is_file());

    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(remember_windows(dir.path(), "one", &[], &epoch()).is_err());
}

#[test]
fn malformed_window_memory_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    RecordFolder::<Key>::at(dir.path(), &WINDOWS)
        .make()
        .unwrap();
    for entry in [
        json!(false),
        json!(1),
        json!("bad"),
        json!([]),
        json!({}),
        json!({"epoch": {}}),
        json!({"windows": false, "epoch": {}}),
    ] {
        fs::write(window_record(dir.path(), "one"), entry.to_string()).unwrap();
        assert!(super::remembered_windows(dir.path(), &window_key("one")).is_err());
    }
}

#[test]
fn window_memory_is_separate_from_capture_retention() {
    let dir = tempfile::tempdir().unwrap();
    let kept = keep(dir.path(), "screen full", &screen(), None, false).unwrap();
    for index in 0..TARGETS + 1 {
        remember_windows(dir.path(), &index.to_string(), &[], &epoch()).unwrap();
    }
    assert!(remembered_windows(dir.path(), "0").is_some());
    assert_eq!(
        remembered_windows(dir.path(), &TARGETS.to_string()).map(|observation| observation.windows),
        Some(Some(vec![]))
    );
    keep(dir.path(), "screen full", &screen(), None, false).unwrap();
    assert_eq!(
        remembered_windows(dir.path(), &TARGETS.to_string()).map(|observation| observation.windows),
        Some(Some(vec![]))
    );
    assert_eq!(record(dir.path())["targets"].as_object().unwrap().len(), 1);
    assert!(kept.path.exists());
    let large = HitWindow {
        start: Some([123, 456]),
        number: 42,
        pid: 812,
        layer: 0,
        bounds: crate::screen::windows::Bounds::display(1000.0, 800.0),
    };
    remember_windows(dir.path(), "large", &[], &epoch()).unwrap();
    assert!(remember_windows(dir.path(), "large", &vec![large; 1024], &epoch()).is_err());
    assert!(
        remembered_windows(dir.path(), "large")
            .unwrap()
            .windows
            .is_none()
    );
    assert!(fs::metadata(dir.path().join(RECORD)).unwrap().len() <= RECORD_LIMIT);
}

#[test]
fn the_capture_record_is_never_listed_or_pruned_as_a_screenshot() {
    let dir = tempfile::tempdir().unwrap();
    let kept = keep(dir.path(), "screen 1456", &screen(), None, false).unwrap();
    let old = std::time::SystemTime::now() - Duration::from_secs(30 * 24 * 60 * 60);
    for name in [RECORD, LOCK] {
        fs::File::options()
            .write(true)
            .open(dir.path().join(name))
            .unwrap()
            .set_modified(old)
            .unwrap();
    }
    fs::File::options()
        .write(true)
        .open(&kept.path)
        .unwrap()
        .set_modified(old)
        .unwrap();

    assert_eq!(files(dir.path()).len(), 1, "only the screenshot is listed");
    crate::client::prune_captures(dir.path());

    assert!(!kept.path.exists(), "the old screenshot is pruned");
    assert!(dir.path().join(RECORD).exists());
    assert!(dir.path().join(LOCK).exists());
}

#[test]
fn two_agents_keeping_captures_at_once_leave_a_valid_record() {
    let dir = tempfile::tempdir().unwrap();
    let shots = [frame(banded_png(64, 36, 0)), frame(banded_png(64, 36, 24))];

    std::thread::scope(|scope| {
        for agent in 0..4 {
            let (dir, shots) = (dir.path(), &shots);
            scope.spawn(move || {
                for turn in 0..10 {
                    let key = if turn % 2 == 0 {
                        "screen 1456"
                    } else {
                        "screen full"
                    };
                    let kept = keep(dir, key, &shots[(agent + turn) % 2], None, false).unwrap();
                    assert!(kept.path.is_file());
                }
            });
        }
    });

    let kept = record(dir.path());
    assert_eq!(kept["version"], 1);
    for key in ["screen 1456", "screen full"] {
        let file = kept["targets"][key]["file"].as_str().unwrap();
        assert!(dir.path().join(file).is_file(), "{key}");
    }
}
