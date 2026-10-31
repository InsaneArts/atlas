//! The latest screenshot of each target (the whole screen, an app's windows
//! or a region, at one size), so a capture with the same pixels can point at
//! that file instead of saving the same picture again, and the latest
//! pointer hit regions observed by each host session.
//!
//! The record (`last-captures.json`, beside the screenshots) says where
//! to look: an earlier file counts as the same picture only once its own
//! bytes on disk hash to the new capture's. A lost, damaged or stale record
//! can therefore only cost a new file, never claim a match that is not one.
use crate::{
    client::{Frame, Size, timestamp},
    config::write_atomic,
    daemon::protocol::{Crop, Region},
    digest,
    error::{Code, Error, Result},
    files::write_private,
    helper_info::capture_time,
    native::windows::HitWindow,
    session::Epoch,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Read,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

const RECORD: &str = "last-captures.json";
const LOCK: &str = "last-captures.lock";
const VERSION: u64 = 1;
/// How many targets the record remembers, newest first.
const TARGETS: usize = 32;
/// A record larger than this is not read.
const RECORD_LIMIT: u64 = 64 * 1024;
/// Screenshots are pruned after a week: an earlier one older than this is
/// never pointed at, so a reply never names a file about to be removed.
const FRESH: Duration = Duration::from_secs(6 * 24 * 60 * 60);

/// Where a capture's picture is, and whether it repeats the last one.
#[derive(Debug, PartialEq)]
pub(crate) struct Kept {
    pub path: PathBuf,
    /// The pixels are identical to the last capture of the same target.
    pub unchanged: bool,
    /// A new file was saved.
    pub written: bool,
}

/// The last capture of one target.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct Entry {
    sha256: String,
    file: String,
    width: u32,
    height: u32,
    /// The part of the display a crop showed, as the helper returned it.
    #[serde(default)]
    region: Option<Region>,
    epoch: Value,
    at: String,
}

pub(crate) fn remember_windows(output: &Path, key: &str, windows: &[HitWindow]) -> Result<()> {
    let _lock = locked(output)
        .ok_or_else(|| Error::new(Code::State, "The capture record is unavailable"))?;
    let mut targets = read(output);
    targets.insert(
        format!("session:{key}"),
        json!({"windows": windows, "at": timestamp().to_string()}),
    );
    write(output, targets)
}

pub(crate) fn remembered_windows(output: &Path, key: &str) -> Option<Vec<HitWindow>> {
    serde_json::from_value(read(output).remove(&format!("session:{key}"))?["windows"].take()).ok()
}

/// What a capture shows and at what size: captures compare only with the
/// last one of the same key. An app is named by its process, so a relaunched
/// app never matches its earlier windows.
pub(crate) fn key(size: Size, crop: Option<&Crop>) -> String {
    let target = match crop {
        None => "screen".to_owned(),
        Some(Crop::App { pid }) => format!("app:{pid}"),
        Some(Crop::Region(region)) => format!(
            "region:{},{},{},{}",
            region.x, region.y, region.width, region.height
        ),
    };
    let size = match size {
        Size::Standard => "standard".to_owned(),
        Size::Longest(side) => side.to_string(),
        Size::Full => "full".to_owned(),
    };

    format!("{target} {size}")
}

/// Save `frame` in `output`, or, when its pixels match the last capture of
/// `key` (same display epoch, same size, the same part of the display
/// `region` and that file still holding exactly those bytes), name the
/// earlier file instead. `region` is the part a crop showed: an app's crop
/// follows its windows, so a moved window is a new picture even when the
/// pixels match. `new_file` saves a new file either way; the record then
/// points at it.
pub(crate) fn keep(
    output: &Path,
    key: &str,
    frame: &Frame,
    region: Option<&Region>,
    new_file: bool,
) -> Result<Kept> {
    let hash = format!("{:x}", Sha256::digest(&frame.png));

    let Some(_lock) = locked(output) else {
        return Ok(Kept {
            path: save(output, frame)?.0,
            unchanged: false,
            written: true,
        });
    };

    let mut targets = read(output);
    let epoch = recorded_epoch(&frame.epoch);
    let earlier = targets
        .get(key)
        .and_then(|entry| serde_json::from_value::<Entry>(entry.clone()).ok())
        .filter(|entry| {
            entry.sha256 == hash
                && entry.epoch == epoch
                && (entry.width, entry.height) == (frame.width, frame.height)
                && entry.region.as_ref() == region
        })
        .and_then(|entry| verified(output, &entry.file, &hash));

    if let Some(path) = &earlier
        && !new_file
    {
        return Ok(Kept {
            path: path.clone(),
            unchanged: true,
            written: false,
        });
    }

    let (path, file) = save(output, frame)?;
    let entry = Entry {
        sha256: hash,
        file,
        width: frame.width,
        height: frame.height,
        region: region.copied(),
        epoch,
        at: timestamp().to_string(),
    };
    targets.insert(key.to_owned(), serde_json::to_value(entry)?);
    // A record that cannot be written only costs the next match.
    let _ = write(output, targets);

    Ok(Kept {
        path,
        unchanged: earlier.is_some(),
        written: true,
    })
}

/// `frame` as a new screenshot file, with its path and name.
fn save(output: &Path, frame: &Frame) -> Result<(PathBuf, String)> {
    let file = format!("{}-capture.png", timestamp());
    let path = output.join(&file);
    write_private(&path, &frame.png)?;

    Ok((path, file))
}

/// The record's lock, held until dropped; `None` when it cannot be taken.
fn locked(output: &Path) -> Option<fs::File> {
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(output.join(LOCK))
        .ok()?;
    if !lock.metadata().ok()?.is_file() {
        return None;
    }
    FileExt::try_lock_exclusive(&lock).ok()?;

    Some(lock)
}

/// The display epoch a capture was taken in, as the record keeps it.
fn recorded_epoch(epoch: &Epoch) -> Value {
    json!({
        "uid": epoch.uid,
        "securitySessionId": epoch.security_session_id,
        "displayId": epoch.display_id,
        "width": epoch.display.width,
        "height": epoch.display.height,
        "scale": epoch.display.scale,
        "capturePid": epoch.capture_pid,
        "inputPid": epoch.input_pid,
    })
}

/// The earlier capture `file`, if it is a screenshot file in `output`,
/// young enough to outlive pruning for a while, and still exactly the
/// picture hashed as `hash`.
fn verified(output: &Path, file: &str, hash: &str) -> Option<PathBuf> {
    capture_time(file)?;
    let path = output.join(file);
    let metadata = fs::symlink_metadata(&path).ok()?;
    let age = metadata.modified().ok()?.elapsed().ok()?;

    (metadata.is_file() && age < FRESH && digest::file_sha256(&path)? == hash).then_some(path)
}

/// A missing, oversized, unreadable or unknown record counts as empty.
fn read(output: &Path) -> BTreeMap<String, Value> {
    let Some(file) = crate::files::open_plain(&output.join(RECORD)) else {
        return BTreeMap::new();
    };
    let mut bytes = Vec::new();
    if file.take(RECORD_LIMIT + 1).read_to_end(&mut bytes).is_err()
        || bytes.len() as u64 > RECORD_LIMIT
    {
        return BTreeMap::new();
    }

    let Ok(mut record) = serde_json::from_slice::<Value>(&bytes) else {
        return BTreeMap::new();
    };
    if record["version"].as_u64() != Some(VERSION) {
        return BTreeMap::new();
    }
    serde_json::from_value(record["targets"].take()).unwrap_or_default()
}

/// Write the newest targets within the record's count and byte limits.
fn write(output: &Path, targets: BTreeMap<String, Value>) -> Result<()> {
    let mut entries: Vec<_> = targets.into_iter().collect();
    entries.sort_by_key(|(_, entry)| {
        std::cmp::Reverse(
            entry["at"]
                .as_str()
                .and_then(|at| at.parse::<u128>().ok())
                .unwrap_or(0),
        )
    });
    entries.truncate(TARGETS);
    loop {
        let targets: BTreeMap<_, _> = entries.iter().map(|(key, entry)| (key, entry)).collect();
        let bytes = serde_json::to_vec(&json!({"version": VERSION, "targets": targets}))?;
        if bytes.len() as u64 <= RECORD_LIMIT {
            return write_atomic(&output.join(RECORD), &bytes, 0o600);
        }
        entries.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        daemon::protocol::Region,
        native::capture::{banded_png, sample_png},
        session::Display,
    };
    use std::{sync::Arc, time::Instant};

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
        let (width, height) = crate::client::png_dimensions(&png).unwrap();
        Frame {
            png: Arc::from(png),
            width,
            height,
            display: epoch().display,
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
            moved.display = moved.epoch.display.clone();

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
    fn window_memory_shares_the_capture_lock_and_retention_without_waiting() {
        let dir = tempfile::tempdir().unwrap();
        let kept = keep(dir.path(), "screen full", &screen(), None, false).unwrap();
        for index in 0..TARGETS + 1 {
            remember_windows(dir.path(), &index.to_string(), &[]).unwrap();
        }
        assert!(remembered_windows(dir.path(), "0").is_none());
        assert_eq!(
            remembered_windows(dir.path(), &TARGETS.to_string()),
            Some(vec![])
        );
        keep(dir.path(), "screen full", &screen(), None, false).unwrap();
        assert_eq!(
            remembered_windows(dir.path(), &TARGETS.to_string()),
            Some(vec![])
        );
        assert_eq!(
            record(dir.path())["targets"].as_object().unwrap().len(),
            TARGETS
        );
        assert!(kept.path.exists());
        let lock = locked(dir.path()).unwrap();
        assert!(remember_windows(dir.path(), "locked", &[]).is_err());
        drop(lock);
        let large = HitWindow {
            number: 42,
            pid: 812,
            layer: 0,
            bounds: crate::native::windows::Bounds::display(1000.0, 800.0),
        };
        remember_windows(dir.path(), "large", &vec![large; 1024]).unwrap();
        assert!(remembered_windows(dir.path(), "large").is_none());
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
}
