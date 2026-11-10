//! The order in which commands waiting for control take it. Each waiting
//! command keeps a note in the waiting folder, and tries the lock only once
//! no older waiter is left before it. The notes decide the order only: the
//! lock alone decides who has control, and a note that is missing,
//! unreadable or stale never stops anyone from trying.
use super::{CONTROL_WAIT_MAX, notes::Note};
use atlas_core::{
    files::{own_folder, token},
    host::Label,
    process::State,
    time::now_ms,
};
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

/// A note older than the longest wait, and a little more, is no longer a
/// command waiting.
const STALE: Duration = CONTROL_WAIT_MAX.saturating_add(Duration::from_secs(5));
/// A waiter whose note has not been touched for this long is not trying,
/// so others go on without it.
const QUIET: Duration = Duration::from_secs(2);
/// The most entries of the waiting folder looked at: far more than could
/// ever wait at once, so every waiter is among them, while a folder filled
/// with litter costs a bounded look.
const ENTRIES: usize = 4096;
/// Hex digits of a waiting note's name.
const NAME_LEN: usize = 16;

/// One command's place among those waiting for control, noted until it is
/// dropped: once the command has control, or has given up waiting.
pub(super) struct Turn {
    folder: PathBuf,
    /// The note's file name, telling apart waiters that began together.
    name: String,
    /// When the wait began, in milliseconds since 1970.
    since: u64,
    /// Whether the note was written, and so must be removed.
    written: bool,
    /// The note, kept open to mark it as still trying.
    note: Option<File>,
}

impl Turn {
    /// Take a place in line in `folder`, noted as `label`'s. A note that
    /// cannot be written leaves the place unnoted, never refused.
    pub(super) fn join(folder: &Path, label: Option<&Label>) -> Self {
        let mut name = token();
        name.truncate(NAME_LEN);
        name.push_str(".json");
        let note = Note::now(label);
        let since = note.as_ref().map_or_else(now_ms, |note| note.since);
        Self::write(folder, name, since, note)
    }

    #[cfg(test)]
    pub(super) fn join_at(folder: &Path, label: Option<&Label>, since: u64, name: &str) -> Self {
        let note = Note::now(label).map(|note| Note { since, ..note });
        Self::write(folder, format!("{name}.json"), since, note)
    }

    fn write(folder: &Path, name: String, since: u64, note: Option<Note>) -> Self {
        let path = folder.join(&name);
        let written =
            note.is_some_and(|note| make_folder(folder).is_ok() && note.write(&path).is_ok());
        let kept = written
            .then(|| {
                OpenOptions::new()
                    .write(true)
                    .custom_flags(libc::O_NOFOLLOW)
                    .open(&path)
                    .ok()
            })
            .flatten();

        Self {
            folder: folder.to_owned(),
            name,
            since,
            written,
            note: kept,
        }
    }

    /// Whether no older waiter is left in line before this one, marking
    /// this one as still trying.
    pub(super) fn first(&self) -> bool {
        if let Some(note) = &self.note {
            let _ = note.set_modified(SystemTime::now());
        }

        !waiters(&self.folder)
            .iter()
            .any(|(name, since)| name != &self.name && (*since, name) < (self.since, &self.name))
    }
}

impl Drop for Turn {
    fn drop(&mut self) {
        if self.written {
            let _ = fs::remove_file(self.folder.join(&self.name));
        }
    }
}

/// Whether any command is waiting in line in `folder`: a newcomer then
/// waits its turn instead of trying first.
pub(super) fn queued(folder: &Path) -> bool {
    !waiters(folder).is_empty()
}

/// The commands waiting in `folder`, each note's name and when it began
/// waiting, among all the notes there. A note whose command has ended is
/// removed. One whose command is suspended, too old, not touched for a
/// while, or whose process macOS would not describe, is passed over.
fn waiters(folder: &Path) -> Vec<(String, u64)> {
    read_notes(folder, true)
        .0
        .into_iter()
        .map(|(name, note)| (name, note.since))
        .collect()
}

/// The notes of the commands waiting in `folder` now, as [`waiters`]
/// counts them, for apps to show who waits. Reading changes nothing.
pub(super) fn waiting(folder: &Path) -> Vec<Note> {
    read_notes(folder, false)
        .0
        .into_iter()
        .map(|(_, note)| note)
        .collect()
}

/// Display candidates without process inspection or cleanup.
pub(super) fn candidates(folder: &Path) -> Vec<Note> {
    candidates_at(folder, SystemTime::now())
}

pub(super) fn candidates_at(folder: &Path, now: SystemTime) -> Vec<Note> {
    if !own_folder(folder) {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let oldest = atlas_core::time::ms(
        now.duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default(),
    )
    .saturating_sub(atlas_core::time::ms(STALE));
    let mut notes: Vec<_> = entries
        .flatten()
        .take(ENTRIES)
        .filter_map(|entry| {
            note_name(&entry)?;
            let path = entry.path();
            let note = Note::read(&path)?;
            let touched = fs::symlink_metadata(&path)
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| now.duration_since(modified).ok())
                .is_none_or(|age| age < QUIET);
            (note.since >= oldest && touched).then_some(note)
        })
        .take(super::NOTES_READ)
        .collect();
    notes.sort_by_key(|note| note.since);
    notes
}

/// Waiting notes for a service change, preserving failed or truncated reads.
pub(super) fn checked_waiting(folder: &Path) -> Option<Vec<Note>> {
    let (notes, complete) = read_notes(folder, false);
    complete.then(|| notes.into_iter().map(|(_, note)| note).collect())
}

/// Read waiting notes and whether every entry was accounted for, tidying only
/// for lock admission. Display and service-change reads leave files untouched.
fn read_notes(folder: &Path, tidy: bool) -> (Vec<(String, Note)>, bool) {
    match fs::symlink_metadata(folder) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return (Vec::new(), true);
        }
        Err(_) => return (Vec::new(), false),
        Ok(_) => {}
    }
    if !own_folder(folder) {
        return (Vec::new(), false);
    }
    let Ok(entries) = fs::read_dir(folder) else {
        return (Vec::new(), false);
    };
    collect_notes(entries, tidy)
}

fn collect_notes(
    mut entries: impl Iterator<Item = std::io::Result<fs::DirEntry>>,
    tidy: bool,
) -> (Vec<(String, Note)>, bool) {
    let now = SystemTime::now();
    let oldest = now_ms().saturating_sub(atlas_core::time::ms(STALE));
    let mut notes = Vec::new();
    let mut complete = true;
    for _ in 0..ENTRIES {
        let Some(entry) = entries.next() else {
            return (notes, complete);
        };
        let Ok(entry) = entry else {
            complete = false;
            continue;
        };
        let Some(name) = note_name(&entry) else {
            continue;
        };
        let path = entry.path();
        let Some(note) = Note::read(&path) else {
            complete = false;
            continue;
        };
        let state = note.process.state();
        if state == State::Gone {
            if tidy {
                let _ = fs::remove_file(&path);
            }
            continue;
        }
        if note.since < oldest {
            continue;
        }
        let modified = fs::symlink_metadata(&path).and_then(|metadata| metadata.modified());
        let touched = match modified {
            Ok(modified) => now.duration_since(modified).map_or(true, |age| age < QUIET),
            Err(_) => {
                complete = false;
                true
            }
        };
        if !touched {
            continue;
        }
        match state {
            State::Running => {}
            State::Gone | State::Stopped => continue,
            State::Unknown => {
                complete = false;
                continue;
            }
        }
        notes.push((name, note));
    }
    // Do not read an extra entry just to establish the end of a full budget.
    (notes, false)
}

/// The name of the waiting note `entry` is, unless it is anything else.
fn note_name(entry: &fs::DirEntry) -> Option<String> {
    let name = entry.file_name().into_string().ok()?;
    atlas_core::name::lower_hex(name.strip_suffix(".json")?, NAME_LEN).then_some(name)
}

/// Make the waiting folder, private to this account, unless it is there.
fn make_folder(folder: &Path) -> std::io::Result<()> {
    match fs::DirBuilder::new().mode(0o700).create(folder) {
        Err(error) if error.kind() != std::io::ErrorKind::AlreadyExists => return Err(error),
        _ => {}
    }

    if own_folder(folder) {
        Ok(())
    } else {
        Err(std::io::Error::other(
            "the waiting folder is not a folder of this account",
        ))
    }
}

#[cfg(test)]
#[path = "turns_tests.rs"]
mod tests;
