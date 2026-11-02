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
    digest,
    error::{Code, Error, Result},
    files::{Durability, LockOpen, open_lock_file, write_atomic, write_private},
    helper_info::capture_time,
    host::Key,
    protocol::{Crop, Region},
    record_folder::{RecordFolder, Records},
    screen::{
        frame::{Frame, Size},
        windows::HitWindow,
    },
    session::{Display, Epoch},
    time::timestamp,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const RECORD: &str = "last-captures.json";
const LOCK: &str = "last-captures.lock";
const VERSION: u64 = 1;
static WINDOWS: Records = Records {
    folder: "last-windows",
    noun: "window observation",
    lock_wait: Duration::from_millis(250),
    durability: Durability::Kernel,
    bytes: RECORD_LIMIT,
};
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
    pub(crate) path: PathBuf,
    /// The pixels are identical to the last capture of the same target.
    pub(crate) unchanged: bool,
    /// A new file was saved.
    pub(crate) written: bool,
}

#[derive(Deserialize, Serialize)]
struct Captures<T> {
    version: u64,
    targets: T,
}

/// The last capture of one target.
#[derive(Debug, Deserialize, Serialize)]
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

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Observation {
    #[serde(deserialize_with = "Option::deserialize")]
    pub(crate) windows: Option<Vec<HitWindow>>,
    pub(crate) epoch: Value,
}

pub(crate) fn remember_windows(
    output: &Path,
    key: &Key,
    windows: Option<&[HitWindow]>,
    epoch: &Epoch,
) -> Result<()> {
    let folder = RecordFolder::at(output, &WINDOWS);
    let mut observation = Observation {
        windows: windows.map(<[_]>::to_vec),
        epoch: recorded_epoch(epoch),
    };
    let mut bytes = serde_json::to_vec(&observation)?;
    let oversized = bytes.len() as u64 > RECORD_LIMIT;
    if oversized {
        observation.windows = None;
        bytes = serde_json::to_vec(&observation)?;
    }
    folder.make()?;
    let _lock = folder.lock()?;
    folder.write(key, &bytes)?;
    if oversized {
        return Err(Error::new(
            Code::Io,
            "Too many window regions to remember; look again",
        ));
    }
    Ok(())
}

pub(crate) fn remembered_windows(output: &Path, key: &Key) -> Result<Option<Observation>> {
    let folder = RecordFolder::at(output, &WINDOWS);
    match fs::symlink_metadata(folder.directory()) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    if !folder.exists() {
        return Err(Error::new(
            Code::Windows,
            "Cannot read the window observations folder; look again",
        ));
    }
    match fs::symlink_metadata(folder.path(key)) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    folder.read_json(key).map(Some).ok_or_else(|| {
        Error::new(
            Code::Windows,
            "Cannot read the last window observation; look again",
        )
    })
}

pub(crate) fn forget_windows(output: &Path, key: &Key) {
    RecordFolder::at(output, &WINDOWS).remove(key);
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
    targets.insert(key.to_owned(), entry);
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
fn locked(output: &Path) -> Option<crate::files::LockFile> {
    let lock = open_lock_file(&output.join(LOCK), LockOpen::WriteNonblocking).ok()?;
    if !lock.metadata().ok()?.is_file() {
        return None;
    }
    let deadline = Instant::now() + Duration::from_millis(50);
    while let Err(error) = FileExt::try_lock_exclusive(&lock) {
        if error.kind() != std::io::ErrorKind::WouldBlock || Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    Some(crate::files::LockFile(lock))
}

/// The display epoch a capture was taken in, as the record keeps it. It
/// stays raw JSON: this guard matches only exactly what Atlas writes, never
/// an array, an integer dimension or anything a typed read would accept.
pub(crate) fn recorded_epoch(epoch: &Epoch) -> Value {
    let Epoch {
        uid,
        capture_pid,
        display: Display {
            width,
            height,
            scale,
        },
        security_session_id,
        display_id,
        input_pid,
    } = *epoch;
    json!({
        "uid": uid,
        "securitySessionId": security_session_id,
        "displayId": display_id,
        "width": width,
        "height": height,
        "scale": scale,
        "capturePid": capture_pid,
        "inputPid": input_pid,
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
fn read(output: &Path) -> BTreeMap<String, Entry> {
    let read = || {
        let file = crate::files::open_plain(&output.join(RECORD))?;
        let bytes = crate::files::read_limited(file, RECORD_LIMIT)?;
        let record: Captures<BTreeMap<String, Entry>> = serde_json::from_slice(&bytes).ok()?;
        (record.version == VERSION).then_some(record.targets)
    };
    read().unwrap_or_default()
}

/// Write the newest targets within the record's count and byte limits.
fn write(output: &Path, targets: BTreeMap<String, Entry>) -> Result<()> {
    let mut entries: Vec<_> = targets.into_iter().collect();
    entries.sort_by_key(|(_, entry)| std::cmp::Reverse(entry.at.parse::<u128>().unwrap_or(0)));
    entries.truncate(TARGETS);
    loop {
        let targets: BTreeMap<_, _> = entries.iter().map(|(key, entry)| (key, entry)).collect();
        let bytes = serde_json::to_vec(&Captures {
            version: VERSION,
            targets,
        })?;
        if bytes.len() as u64 <= RECORD_LIMIT {
            return write_atomic(&output.join(RECORD), &bytes, 0o600);
        }
        entries.pop();
    }
}

#[cfg(test)]
mod tests;
