//! The files the log keeps once it starts over: [`PREVIOUS`], always the
//! newest of them, and the older ones in [`HISTORY`], numbered in the order
//! they were kept. They are kept for [`KEEP_FOR`], and within [`KEEP_BYTES`]
//! with the current file; the oldest go first.
use super::{FILE, PREVIOUS, ROTATE_AT};
use atlas_core::error::Code;
use std::{
    fs::{self, File, Metadata},
    io,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

/// The folder, beside the log, that keeps its older files.
pub(crate) const HISTORY: &str = "metadata.history";
/// How long a kept file stays after its last row was written.
const KEEP_FOR: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// The most the log's files take on disk together.
pub(super) const KEEP_BYTES: u64 = 64 * 1024 * 1024;
/// The most entries of [`HISTORY`] looked at. The log keeps about
/// [`KEEP_BYTES`] over a megabyte of them, so this is never reached by its
/// own files.
const LISTED: usize = 1024;

/// What a file was when it was read: a file that changes, or another file
/// under its name, reads as another revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Revision {
    pub(super) device: u64,
    pub(super) inode: u64,
    pub(super) length: u64,
    modified: i64,
    modified_nanos: i64,
}

impl Revision {
    pub(super) fn of(file: &File) -> io::Result<Self> {
        Ok(Self::from(&file.metadata()?))
    }

    /// Whether it is the same file as `other`, whatever either holds.
    pub(super) fn same_file(&self, other: &Self) -> bool {
        (self.device, self.inode) == (other.device, other.inode)
    }
}

impl From<&Metadata> for Revision {
    fn from(metadata: &Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            length: metadata.len(),
            modified: metadata.mtime(),
            modified_nanos: metadata.mtime_nsec(),
        }
    }
}

/// The kept files in [`HISTORY`], as one listing found them.
pub(super) struct History {
    /// Newest first, by their number.
    pub(super) files: Vec<PathBuf>,
    /// Whether every entry was looked at: false past [`LISTED`] entries.
    pub(super) whole: bool,
}

/// The kept files in [`HISTORY`]. A missing folder keeps none; one that is
/// not a folder of this account's own is refused.
///
/// # Errors
///
/// Returns the I/O error of listing it, or one for a folder that is a link
/// or another account's.
pub(super) fn history(directory: &Path) -> io::Result<History> {
    let folder = directory.join(HISTORY);
    match fs::symlink_metadata(&folder) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(History {
                files: Vec::new(),
                whole: true,
            });
        }
        Err(error) => return Err(error),
        Ok(_) if !atlas_core::files::own_folder(&folder) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "The activity history is not a folder of this account's own",
            ));
        }
        Ok(_) => {}
    }
    let mut numbered = Vec::new();
    let mut entries = fs::read_dir(&folder)?;
    for entry in entries.by_ref().take(LISTED) {
        let entry = entry?;
        if let Some(number) = entry.file_name().to_str().and_then(number) {
            numbered.push((number, entry.path()));
        }
    }
    let whole = entries.next().is_none();
    numbered.sort_unstable_by_key(|(number, _)| std::cmp::Reverse(*number));
    Ok(History {
        files: numbered.into_iter().map(|(_, path)| path).collect(),
        whole,
    })
}

/// The number a kept file's name gives: decimal digits, then `.jsonl`.
fn number(name: &str) -> Option<u64> {
    let digits = name.strip_suffix(".jsonl")?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// Start the log in `directory` over, under its writers' lock: the previous
/// file goes to [`HISTORY`] under the next number, the current file becomes
/// the previous one, and the history is pruned. Keeping and pruning are
/// best effort: when keeping fails, the previous file is replaced, as it
/// always was before the log kept more.
///
/// The previous file is linked into the history before the current one is
/// renamed over it, so a reader always finds a previous file, and the rows
/// it held are under one name or the other throughout.
///
/// # Errors
///
/// Returns the I/O error of renaming the current file.
pub(super) fn rotate(directory: &Path) -> io::Result<()> {
    // Best effort: the row is still appended, and a file not kept is
    // replaced, as before.
    let _kept = keep_previous(directory);
    fs::rename(directory.join(FILE), directory.join(PREVIOUS))?;
    // Best effort: what is not pruned now is pruned when the log next
    // starts over, and every reader keeps its own bound meanwhile.
    let _pruned = prune(directory, SystemTime::now());
    Ok(())
}

/// Link the previous file into [`HISTORY`] under the next number, unless it
/// is there already, as after a rotation cut short between its steps.
fn keep_previous(directory: &Path) -> io::Result<()> {
    let previous = directory.join(PREVIOUS);
    let metadata = match fs::symlink_metadata(&previous) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        other => other?,
    };
    if !metadata.is_file() {
        return Ok(());
    }
    let folder = directory.join(HISTORY);
    atlas_core::files::private_folder(&folder, Code::State).map_err(io::Error::other)?;
    let kept = history(directory)?.files;
    let revision = Revision::from(&metadata);
    let mut next = 1;
    for (index, path) in kept.iter().enumerate() {
        let Ok(found) = fs::symlink_metadata(path) else {
            continue;
        };
        if Revision::from(&found).same_file(&revision) {
            return Ok(());
        }
        if index == 0 {
            next = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(number)
                .map_or(1, |newest| newest.saturating_add(1));
        }
    }
    fs::hard_link(&previous, folder.join(format!("{next}.jsonl")))
}

/// Remove the kept files last written longer than [`KEEP_FOR`] before `now`,
/// then, oldest first, those past [`KEEP_BYTES`] with the newer ones and the
/// current and previous files, the current one counted as full: it grows to
/// [`ROTATE_AT`] and a row before the log starts over again. The current and
/// previous files are never removed, nor is anything in the history but its
/// numbered plain files.
fn prune(directory: &Path, now: SystemTime) -> io::Result<()> {
    let length =
        |name: &str| fs::symlink_metadata(directory.join(name)).map_or(0, |found| found.len());
    let mut total = length(FILE).max(ROTATE_AT).saturating_add(length(PREVIOUS));
    // Once one file is past the bound, so is every older one, however small.
    let mut full = false;
    for path in history(directory)?.files {
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let stale = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > KEEP_FOR);
        if !stale && !full {
            total = total.saturating_add(metadata.len());
            full = total > KEEP_BYTES;
        }
        if stale || full {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "retained_tests.rs"]
mod tests;
