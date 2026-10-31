//! The marker of every change whose outcome is unknown, and the claim each
//! one is carried out under: recording a change before it starts, reading
//! what is pending, and settling it once its outcome is acknowledged or
//! recovered. The marker is replaced whole, under its own lock.
use super::{
    Cause, Controller, Pending, RUNNING_COMMANDS_MAX,
    busy::busy_running,
    by_generation::{self, ByGeneration},
    commands,
    executor::{EXECUTOR_WRITING, claim_executor_running},
    notes::Note,
    remove_claim,
};
use crate::{
    error::{Code, Error, Result},
    files::{Held, Lock, lock_within, open_lock, token},
    host_session::Key,
};
use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

/// How a pending change is recorded beside the marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    /// A change carried out under control until its outcome is known.
    Change,
    /// A command an agent started with `run`, which others act beside.
    Command,
}

/// What is pending, as one read under the marker lock finds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Unsettled {
    /// Nothing: there is no marker.
    Nothing,
    /// Only commands agents started with `run`, each still running under
    /// its own claim.
    Commands,
    /// An outcome nothing may settle yet, or one not known to be a command.
    Unknown,
}

impl Controller {
    /// What is pending, from one read under the marker lock. A marker with
    /// no entries is an unknown outcome, as an unreadable one is.
    pub(super) fn unsettled(&self) -> Result<Unsettled> {
        let _lock = self.lock_marker()?;
        if !self.marker.try_exists()? {
            return Ok(Unsettled::Nothing);
        }

        let pending = pending(&self.marker)?;
        let marked = self.commands();
        let running = |generation: &String| self.running_command(&marked, generation);
        Ok(if !pending.is_empty() && pending.iter().all(running) {
            Unsettled::Commands
        } else {
            Unsettled::Unknown
        })
    }

    pub(super) fn lock_marker(&self) -> Result<File> {
        self.make_directory()?;
        let file = open_lock(&self.marker_lock)?;
        FileExt::lock_exclusive(&file)?;

        Ok(file)
    }

    fn lock_marker_within(&self, wait: Duration) -> Result<File> {
        self.make_directory()?;
        let file = open_lock(&self.marker_lock)?;
        lock_within(&file, Lock::Exclusive, wait).map_err(|Held| {
            Error::new(
                Code::Busy,
                "Another Atlas command is updating pending actions",
            )
        })?;
        Ok(file)
    }

    /// Every pending generation, whether a live process is still carrying
    /// it out, and the host session that claimed it when one is recorded;
    /// from one locked read of the marker, or none when there is no marker.
    pub fn pending_causes(&self) -> Result<Vec<Cause>> {
        if !self.marker.try_exists()? {
            return Ok(Vec::new());
        }

        let _lock = self.lock_marker()?;
        self.read_causes()
    }

    /// Read one pending snapshot, waiting at most `wait` for its lock.
    /// An absent marker creates neither its directory nor its lock.
    pub fn pending_causes_within(&self, wait: Duration) -> Result<Vec<Cause>> {
        if !self.marker.try_exists()? {
            return Ok(Vec::new());
        }
        let _lock = self.lock_marker_within(wait)?;
        self.read_causes()
    }

    fn read_causes(&self) -> Result<Vec<Cause>> {
        let mut claimed: ByGeneration<Key> = by_generation::read(&self.causes);
        let marked = self.commands();
        Ok(pending(&self.marker)?
            .into_iter()
            .map(|generation| Cause {
                running: self.in_progress(&generation),
                command: self.running_command(&marked, &generation),
                session: claimed.remove(&generation),
                generation,
            })
            .collect())
    }

    /// Generations of mutations whose outcome is still unknown.
    pub fn pending(&self) -> Result<Vec<String>> {
        let _lock = self.lock_marker()?;
        pending(&self.marker)
    }

    /// Generations no mutation in progress will settle: their writer
    /// dropped or lost its claim (an uncertain failure or a crash), or the
    /// entry is not one of ours. For display only; every pending generation
    /// still blocks control until verified recovery.
    pub fn unresolved(&self) -> Result<Vec<String>> {
        self.pending_snapshot().map(|snapshot| snapshot.unresolved)
    }

    /// Unresolved generations, the total and the commands still running,
    /// from one locked marker read.
    pub(crate) fn pending_snapshot(&self) -> Result<Snapshot> {
        let _lock = self.lock_marker()?;
        self.read_snapshot()
    }

    pub(crate) fn pending_snapshot_within(&self, wait: Duration) -> Result<Snapshot> {
        let _lock = self.lock_marker_within(wait)?;
        self.read_snapshot()
    }

    fn read_snapshot(&self) -> Result<Snapshot> {
        let pending = pending(&self.marker)?;
        let marked = self.commands();
        let total = pending.len();
        let commands = pending
            .iter()
            .filter(|generation| self.running_command(&marked, generation))
            .count();
        let unresolved = pending
            .into_iter()
            .filter(|generation| !self.in_progress(generation))
            .collect();

        Ok(Snapshot {
            unresolved,
            total,
            commands,
        })
    }

    /// The claim file for a generation; only plain tokens name one.
    fn live(&self, generation: &str) -> Option<PathBuf> {
        (!generation.is_empty() && generation.bytes().all(|b| b.is_ascii_hexdigit())).then(|| {
            self.directory
                .join(format!("{}.{generation}.live", self.name))
        })
    }

    /// Whether a live process holds this generation's claim file locked: a
    /// plain lock test, never an executor's record, so a claim kept only by
    /// an executor that outlived its caller is not held.
    fn held(&self, generation: &str) -> bool {
        self.open_claim(generation).is_some_and(|(_, file)| {
            FileExt::try_lock_shared(&file)
                .is_err_and(|error| error.kind() == std::io::ErrorKind::WouldBlock)
        })
    }

    /// Whether `generation` is a command an agent started with `run` that
    /// still runs: `marked` as one, and its claim held by a live process.
    pub(super) fn running_command(&self, marked: &[String], generation: &str) -> bool {
        marked.iter().any(|entry| entry == generation) && self.held(generation)
    }

    /// The claim file for `generation`, and where it is, when there is one
    /// that opens without following a link.
    fn open_claim(&self, generation: &str) -> Option<(PathBuf, File)> {
        let live = self.live(generation)?;
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&live)
            .ok()?;
        Some((live, file))
    }

    /// The generations marked as commands agents started with `run`, as
    /// the list beside the marker names them; whether each still runs is
    /// the reader's to check.
    pub(super) fn commands(&self) -> Vec<String> {
        commands::read(&self.commands)
    }

    /// Whether some live mutation, in any process, still holds the claim.
    pub(super) fn in_progress(&self, generation: &str) -> bool {
        self.open_claim(generation).is_some_and(|(live, file)| {
            FileExt::try_lock_shared(&file).is_err() || claim_executor_running(&file, &live)
        })
    }

    /// Record a pending mutation this process is carrying out.
    pub fn quarantine(&self) -> Result<Pending> {
        self.quarantine_as(&token())
    }

    /// Record a command an agent started with `run`: its claim, its marker
    /// entry and its mark as a running command, so other agents may act
    /// beside it while it runs and recovery waits for it.
    pub(crate) fn quarantine_command(&self) -> Result<Pending> {
        self.quarantine_marked(&token(), Mark::Command)
    }

    /// Record a pending mutation under a generation the caller chose, so it
    /// can persist its own claim on that generation first. The claim is
    /// taken before the marker is written, so a marked generation is never
    /// briefly mistaken for an abandoned one. It is never refused after a
    /// signal: what settles or cleans up a change under way still records.
    pub fn quarantine_as(&self, generation: &str) -> Result<Pending> {
        self.quarantine_marked(generation, Mark::Change)
    }

    /// [`Controller::quarantine_as`], recorded as `mark` says.
    fn quarantine_marked(&self, generation: &str, mark: Mark) -> Result<Pending> {
        let hold = self.interrupt.claim();
        let live = self
            .live(generation)
            .ok_or_else(|| Error::new(Code::State, "Pending generations are hexadecimal tokens"))?;

        self.make_directory()?;
        let file = open_lock(&live)?;
        FileExt::try_lock_exclusive(&file)
            .map_err(|_| Error::new(Code::Busy, "That generation is already being carried out"))?;

        let claim = Pending {
            generation: generation.to_owned(),
            live,
            lock: file,
            _hold: hold,
        };

        // Claim files are only unlinked under the marker lock or by their
        // holder, so if the path still names the locked file here, it will
        // until the claim is dropped.
        let _lock = self.lock_marker()?;
        if !claim.names_own_file() {
            return Err(Error::new(
                Code::Busy,
                "That generation's claim changed while it was taken",
            ));
        }

        let mut pending = pending(&self.marker)?;
        if !pending.iter().any(|entry| entry == generation) {
            pending.push(generation.to_owned());
        }

        // Marked before the marker names it: a mark the marker does not
        // name means nothing, and a command that cannot be marked never
        // starts. So many already run that another is refused.
        if mark == Mark::Command {
            let marked = self.commands();
            let running = pending
                .iter()
                .filter(|entry| self.running_command(&marked, entry))
                .count();
            if running >= RUNNING_COMMANDS_MAX {
                return Err(busy_running(format!(
                    "{RUNNING_COMMANDS_MAX} commands agents started are already running in the helper. Wait for one to end."
                )));
            }
            commands::mark(&self.commands, generation, &pending)?;
        }
        replace_private(&self.marker, &serde_json::to_vec(&pending)?)?;
        if let Some(label) = &self.label {
            // Whose change it is, for display only; a lost note loses a label.
            let _ = by_generation::note(&self.causes, generation, &label.key, &pending);
            if mark == Mark::Command
                && let Some(note) = Note::now(Some(label))
            {
                let _ = by_generation::note(&self.running, generation, &note, &pending);
            }
        }
        Ok(claim)
    }

    /// Leave a command an agent started with `run` whose outcome became
    /// unknown as any other unknown outcome: its mark goes, then its claim,
    /// while its marker entry stays for recovery, and its note with it, to
    /// name its session. It is never settled here.
    pub(crate) fn abandon_command(&self, claim: Pending) {
        if let Ok(_lock) = self.lock_marker() {
            let kept: Vec<String> = pending(&self.marker)
                .unwrap_or_default()
                .into_iter()
                .filter(|entry| entry != claim.generation())
                .collect();
            let _ = commands::keep(&self.commands, &kept);
        }
        drop(claim);
    }

    /// Settle one acknowledged mutation; others stay pending.
    pub fn clear_if(&self, generation: &str) -> Result<()> {
        let _lock = self.lock_marker()?;

        if let Some(live) = self.live(generation)
            && !self.in_progress(generation)
        {
            remove_claim(&live);
        }

        let mut pending = pending(&self.marker)?;
        pending.retain(|entry| entry != generation);
        let _ = by_generation::keep::<Key>(&self.causes, &pending);
        let _ = commands::keep(&self.commands, &pending);
        let _ = by_generation::keep::<Note>(&self.running, &pending);

        if pending.is_empty() {
            remove_marker(&self.marker)
        } else {
            replace_private(&self.marker, &serde_json::to_vec(&pending)?)
        }
    }

    /// Verified recovery settles every pending mutation no live claim holds
    /// and removes every claim file no live mutation holds, listed or not. A
    /// mutation still being carried out (an approved root run among them)
    /// stays pending: only its own end settles it.
    pub fn clear(&self) -> Result<()> {
        let _lock = self.lock_marker()?;
        let running: Vec<String> = pending(&self.marker)?
            .into_iter()
            .filter(|generation| self.in_progress(generation))
            .collect();
        let prefix = format!("{}.", self.name);
        let replacing = format!(".{prefix}");

        for entry in fs::read_dir(&self.directory)?.flatten() {
            let name = entry.file_name();
            // Only a writer holding the marker lock has a replacement under
            // way, so one found now was left by a writer that was killed.
            if name
                .to_str()
                .is_some_and(|name| name.starts_with(&replacing) && name.ends_with(REPLACING))
            {
                let _ = fs::remove_file(entry.path());
                continue;
            }
            let generation = name
                .to_str()
                .and_then(|name| name.strip_prefix(&prefix)?.strip_suffix(".live"));
            if let Some(generation) = generation
                && self.live(generation).is_some()
                && !self.in_progress(generation)
            {
                remove_claim(&entry.path());
                continue;
            }
            // An executor record still unnamed when its claim is no longer
            // held was left by a writer that was killed.
            let writing = name.to_str().and_then(|name| {
                let (generation, _) = name
                    .strip_prefix(&prefix)?
                    .split_once(&format!(".{EXECUTOR_WRITING}"))?;
                Some(generation)
            });
            if let Some(generation) = writing
                && self.live(generation).is_some()
                && !self.in_progress(generation)
            {
                let _ = fs::remove_file(entry.path());
            }
        }
        let _ = by_generation::keep::<Key>(&self.causes, &running);
        let _ = commands::keep(&self.commands, &running);
        let _ = by_generation::keep::<Note>(&self.running, &running);

        if running.is_empty() {
            remove_marker(&self.marker)
        } else {
            replace_private(&self.marker, &serde_json::to_vec(&running)?)
        }
    }
}

/// The end of the name a replacement is written under before it is renamed
/// over the file it replaces.
pub(super) const REPLACING: &str = ".replacing";

/// Replace the file at `path` with `bytes`, private to this account, so a
/// reader only ever sees the old file or the new one whole, and a writer
/// killed partway leaves the old one: the new one is written beside it,
/// synced, renamed over it, and the folder synced. Only one writer at a
/// time may replace a file, as the marker lock keeps them.
pub(super) fn replace_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let (Some(directory), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(Error::new(
            Code::State,
            "A record needs a folder and a name",
        ));
    };
    let temporary = directory.join(format!(
        ".{}.{}{REPLACING}",
        name.to_string_lossy(),
        token()
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temporary)
        .map_err(|error| Error::writing(&temporary, error))?;

    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    #[cfg(test)]
    if super::faults::replacing_stops(&name.to_string_lossy()) {
        // As a writer killed here would, it leaves the replacement unnamed.
        return Err(Error::new(Code::State, "interrupted before the rename"));
    }
    let replaced = written
        .and_then(|()| fs::rename(&temporary, path))
        .and_then(|()| File::open(directory)?.sync_all());
    if replaced.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    replaced.map_err(|error| Error::writing(path, error))
}

/// The pending changes, as one locked read of the marker counts them.
#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    /// Generations no mutation in progress will settle.
    pub unresolved: Vec<String>,
    /// How many are pending in all.
    pub total: usize,
    /// How many are commands agents started with `run`, still running.
    pub commands: usize,
}

/// Pending generations in a marker; an unreadable one stays pending as unknown.
pub(super) fn pending(marker: &Path) -> Result<Vec<String>> {
    match fs::read_to_string(marker) {
        Ok(text) => {
            Ok(serde_json::from_str(&text).unwrap_or_else(|_| vec!["legacy-unknown".into()]))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

fn remove_marker(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => Ok(other?),
    }
}
