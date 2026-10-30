//! The order in which commands waiting for control take it. Each waiting
//! command keeps a note in the waiting folder, and tries the lock only once
//! no older waiter is left before it. The notes decide the order only: the
//! lock alone decides who has control, and a note that is missing,
//! unreadable or stale never stops anyone from trying.
use super::{CONTROL_WAIT_MAX, notes::Note};
use crate::{
    files::{own_folder, token},
    host_session::{Label, now_ms},
    process::State,
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
    notes(folder, true)
        .into_iter()
        .map(|(name, note)| (name, note.since))
        .collect()
}

/// The notes of the commands waiting in `folder` now, as [`waiters`]
/// counts them, for apps to show who waits. Reading changes nothing.
pub(super) fn waiting(folder: &Path) -> Vec<Note> {
    notes(folder, false)
        .into_iter()
        .map(|(_, note)| note)
        .collect()
}

/// Each waiting command's note in `folder`, by its file name, passing over
/// those [`waiters`] passes over; a note whose command has ended is removed
/// when `tidy`.
fn notes(folder: &Path, tidy: bool) -> Vec<(String, Note)> {
    if !own_folder(folder) {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let now = SystemTime::now();
    let oldest = now_ms().saturating_sub(crate::time::ms(STALE));

    entries
        .flatten()
        .take(ENTRIES)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let stem = name.strip_suffix(".json")?;
            if !crate::name::lower_hex(stem, NAME_LEN) {
                return None;
            }
            let path = entry.path();
            let note = Note::read(&path)?;
            match note.process.state() {
                State::Running => {}
                State::Gone => {
                    if tidy {
                        let _ = fs::remove_file(&path);
                    }
                    return None;
                }
                State::Stopped | State::Unknown => return None,
            }
            let touched = fs::symlink_metadata(&path)
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| now.duration_since(modified).ok())
                .is_none_or(|age| age < QUIET);

            (note.since >= oldest && touched).then_some((name, note))
        })
        .collect()
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
mod tests {
    use super::*;
    use crate::host_session::Instance;

    /// A waiting note in `folder` for `process`, begun `since`.
    fn noted(folder: &Path, process: Instance, since: u64) -> PathBuf {
        make_folder(folder).unwrap();
        let mut name = crate::files::token();
        name.truncate(NAME_LEN);
        let path = folder.join(format!("{name}.json"));
        let note = Note {
            session: None,
            host: None,
            folder: None,
            named: None,
            process,
            since,
        };
        note.write(&path).unwrap();
        path
    }

    #[test]
    fn a_written_note_goes_with_its_turn_even_when_it_could_not_be_kept_open() {
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().join("waiting");
        let turn = Turn::join(&folder, None);
        let path = folder.join(&turn.name);
        assert!(turn.written && path.exists());

        let mut unopened = turn;
        unopened.note = None;
        drop(unopened);
        assert!(!path.exists());
    }

    #[test]
    fn every_waiter_counts_however_many_there_are_and_whatever_else_is_there() {
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().join("waiting");
        let this = Instance::current().unwrap();
        let now = now_ms();
        let turn = Turn::join(&folder, None);

        for n in 0..100 {
            fs::write(folder.join(format!("litter-{n}.json")), b"{}").unwrap();
            fs::write(folder.join(format!("{n:016x}.json")), b"not a note").unwrap();
        }
        for n in 0..70 {
            noted(&folder, this, now + 1000 + n);
        }
        assert!(turn.first(), "seventy newer waiters wait behind it");

        let older = noted(&folder, this, now - 1000);
        assert!(
            !turn.first(),
            "one older waiter, wherever it is listed, goes first"
        );
        fs::remove_file(older).unwrap();
        assert!(turn.first());
    }

    #[test]
    fn a_suspended_waiter_is_passed_over_at_once_and_kept() {
        use nix::sys::signal::{Signal, kill};

        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().join("waiting");
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let pid = i32::try_from(child.id()).unwrap();
        let start = crate::host_session::ancestry::process(pid).unwrap().start;
        let suspended = Instance::new(pid, start);
        let path = noted(&folder, suspended, now_ms() - 1000);
        assert!(queued(&folder), "a running waiter is in line");

        kill(nix::unistd::Pid::from_raw(pid), Signal::SIGSTOP).unwrap();
        let started = std::time::Instant::now();
        while suspended.state() != State::Stopped {
            assert!(started.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!queued(&folder), "a suspended one is not, at once");
        assert!(path.exists(), "it is not gone, so its note stays");

        let _ = kill(nix::unistd::Pid::from_raw(pid), Signal::SIGCONT);
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(!queued(&folder));
        assert!(!path.exists(), "an ended waiter's note is removed");
    }

    #[test]
    fn a_waiter_macos_will_not_describe_is_skipped_but_never_removed() {
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().join("waiting");
        // launchd is another account's process: macOS will not say more.
        let other = Instance::new(1, [1, 0]);
        assert_eq!(other.state(), State::Unknown);
        let path = noted(&folder, other, now_ms() - 1000);

        assert!(!queued(&folder));
        assert!(path.exists());
    }

    #[test]
    fn stale_waiting_notes_are_ignored_and_reading_them_changes_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().join("waiting");
        let this = Instance::current().unwrap();
        let now = now_ms();
        let live = noted(&folder, this, now - 1000);
        let stale = noted(
            &folder,
            this,
            now - u64::try_from(STALE.as_millis()).unwrap() - 1000,
        );
        let gone = noted(&folder, Instance::new(i32::MAX, [1, 0]), now - 1000);

        let notes = waiting(&folder);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].since, now - 1000);
        assert!(
            live.exists() && stale.exists() && gone.exists(),
            "a read for the apps removes nothing"
        );
        assert_eq!(waiters(&folder).len(), 1);
        assert!(!gone.exists(), "a waiter's own look tidies up");
    }
}
