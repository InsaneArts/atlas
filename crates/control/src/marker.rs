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
    executors::{self, Executor},
    notes::Note,
    remove_claim,
};
use atlas_core::{
    context::context,
    error::{Code, Error, Result},
    files::{Held, Lock, LockFile, lock_within, open_lock, token},
    host::Key,
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
        self.read_unsettled()
    }

    /// Read unsettled state while the caller holds the marker lock.
    pub(super) fn read_unsettled(&self) -> Result<Unsettled> {
        if !self.marker_exists()? {
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

    fn marker_exists(&self) -> Result<bool> {
        self.marker
            .try_exists()
            .map_err(context("Inspect pending marker", &self.marker))
    }

    pub(super) fn lock_marker(&self) -> Result<LockFile> {
        self.make_directory()?;
        let file = open_lock(&self.marker_lock)?;
        FileExt::lock_exclusive(&file.0)
            .map_err(context("Lock pending marker", &self.marker_lock))?;

        Ok(file)
    }

    /// Take the marker lock with bounded patience, including when no marker exists.
    pub(super) fn lock_marker_within(&self, wait: Duration) -> Result<LockFile> {
        self.make_directory()?;
        let file = open_lock(&self.marker_lock)?;
        lock_within(&file, Lock::Exclusive, wait).map_err(|Held(cause)| {
            Error::new(
                Code::Busy,
                "Atlas could not check pending activity in time.",
            )
            .with_source(cause)
        })?;
        Ok(file)
    }

    /// Every pending generation, whether a live process is still carrying
    /// it out, and the host session that claimed it when one is recorded;
    /// from one locked read of the marker, or none when there is no marker.
    ///
    /// # Errors
    ///
    /// Returns lock, storage or decoding failures rather than treating unreadable pending
    /// ownership as empty.
    pub fn pending_causes(&self) -> Result<Vec<Cause>> {
        if !self.marker_exists()? {
            return Ok(Vec::new());
        }

        let _lock = self.lock_marker()?;
        self.read_causes()
    }

    /// Read one pending snapshot, waiting at most `wait` for its lock.
    /// An absent marker creates neither its directory nor its lock.
    ///
    /// # Errors
    ///
    /// Returns lock, storage or decoding failures rather than treating unreadable pending
    /// ownership as empty.
    pub fn pending_causes_within(&self, wait: Duration) -> Result<Vec<Cause>> {
        if !self.marker_exists()? {
            return Ok(Vec::new());
        }
        let _lock = self.lock_marker_within(wait)?;
        self.read_causes()
    }

    /// Service replacement must check for a writer even before a marker exists.
    ///
    /// # Errors
    ///
    /// Returns a marker lock still held after `wait`, and storage or decoding failures rather
    /// than treating unreadable pending ownership as empty.
    pub fn pending_causes_for_service_change(&self, wait: Duration) -> Result<Vec<Cause>> {
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
    ///
    /// # Errors
    ///
    /// Returns failures reading pending mutation records; absence is distinct from an
    /// unreadable record.
    pub fn pending(&self) -> Result<Vec<String>> {
        let _lock = self.lock_marker()?;
        pending(&self.marker)
    }

    /// Generations no mutation in progress will settle: their writer
    /// dropped or lost its claim (an uncertain failure or a crash), or the
    /// entry is not one of ours. For display only; every pending generation
    /// still blocks control until verified recovery.
    ///
    /// # Errors
    ///
    /// Returns failures reading unresolved mutation ownership. A live claim is not an
    /// acknowledgement.
    #[cfg(any(test, feature = "test-support"))]
    pub fn unresolved(&self) -> Result<Vec<String>> {
        self.pending_snapshot().map(|snapshot| snapshot.unresolved)
    }

    /// Unresolved generations, the total and the commands still running,
    /// from one locked marker read.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn pending_snapshot(&self) -> Result<Snapshot> {
        let _lock = self.lock_marker()?;
        self.read_snapshot()
    }

    /// Unresolved generations, the total and the commands still running,
    /// from one locked marker read, waiting at most `wait` for its lock.
    ///
    /// # Errors
    ///
    /// Returns a marker lock still held after `wait` and failures reading the marker. Callers
    /// never treat a failed read as nothing pending.
    pub fn pending_snapshot_within(&self, wait: Duration) -> Result<Snapshot> {
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
            FileExt::try_lock_shared(&file.0)
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
    fn open_claim(&self, generation: &str) -> Option<(PathBuf, LockFile)> {
        let live = self.live(generation)?;
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&live)
            .ok()?;
        Some((live, LockFile(file)))
    }

    /// The generations marked as commands agents started with `run`, as
    /// the list beside the marker names them; whether each still runs is
    /// the reader's to check.
    pub(super) fn commands(&self) -> Vec<String> {
        commands::read(&self.commands)
    }

    /// Whether a process still carries out `generation`'s claim, proven
    /// either way or unknown: unlike [`Controller::in_progress`], which takes
    /// what cannot be told as held, so recovery never clears it.
    fn liveness(&self, generation: &str) -> Liveness {
        let Some(live) = self.live(generation) else {
            return Liveness::Unknown;
        };
        let file = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&live)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Liveness::Gone,
            Err(_) => return Liveness::Unknown,
        };
        match FileExt::try_lock_shared(&file) {
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Liveness::Live,
            Err(_) => Liveness::Unknown,
            Ok(()) => super::executor::claim_executor(&file, &live),
        }
    }

    /// Whether some live mutation, in any process, still holds the claim.
    pub(super) fn in_progress(&self, generation: &str) -> bool {
        self.open_claim(generation).is_some_and(|(live, file)| {
            FileExt::try_lock_shared(&file.0).is_err() || claim_executor_running(&file.0, &live)
        })
    }

    /// Record a pending mutation this process is carrying out.
    ///
    /// # Errors
    ///
    /// Returns lock or durable-marker failures before the caller can treat a mutation as
    /// registered.
    pub fn quarantine(&self) -> Result<Pending> {
        self.quarantine_as(&token())
    }

    /// Record a command an agent started with `run`: its claim, its marker
    /// entry and its mark as a running command, so other agents may act
    /// beside it while it runs and recovery waits for it.
    ///
    /// # Errors
    ///
    /// Returns the failure to write the claim or its marker.
    pub fn quarantine_command(&self) -> Result<Pending> {
        self.quarantine_marked(&token(), Mark::Command, None)
    }

    /// Record a pending mutation under a generation the caller chose, so it
    /// can persist its own claim on that generation first. The claim is
    /// taken before the marker is written, so a marked generation is never
    /// briefly mistaken for an abandoned one. It is never refused after a
    /// signal: what settles or cleans up a change under way still records.
    ///
    /// # Errors
    ///
    /// Rejects invalid generation names and returns lock or marker-write failures. Registration
    /// alone never acknowledges an effect.
    pub fn quarantine_as(&self, generation: &str) -> Result<Pending> {
        self.quarantine_marked(generation, Mark::Change, None)
    }

    /// [`Controller::quarantine_as`], for a change `executor` carries out:
    /// one another executor carries out is named so before the marker names
    /// it, and recovery leaves it for that executor's answer.
    ///
    /// # Errors
    ///
    /// As [`Controller::quarantine_as`], and when its executor cannot be
    /// recorded.
    pub fn quarantine_by(&self, generation: &str, executor: Executor) -> Result<Pending> {
        let named = (!executor.local()).then_some(executor);
        self.quarantine_marked(generation, Mark::Change, named)
    }

    /// [`Controller::quarantine_by`], waiting at most `wait` for the marker
    /// lock: for a claimer that holds other locks and must not queue behind
    /// a stalled marker writer.
    ///
    /// # Errors
    ///
    /// Busy when the marker lock is held past `wait`, with nothing recorded;
    /// else as [`Controller::quarantine_by`].
    pub fn quarantine_by_within(
        &self,
        generation: &str,
        executor: Executor,
        wait: Duration,
    ) -> Result<Pending> {
        let named = (!executor.local()).then_some(executor);
        self.quarantine_marked_within(generation, Mark::Change, named, Some(wait))
    }

    /// [`Controller::quarantine_command`] under a generation the caller
    /// chose, for a command `executor` carries out: named so, as
    /// [`Controller::quarantine_by`] names a change.
    ///
    /// # Errors
    ///
    /// As [`Controller::quarantine_command`], and when its executor cannot
    /// be recorded.
    pub fn quarantine_command_by(&self, generation: &str, executor: Executor) -> Result<Pending> {
        let named = (!executor.local()).then_some(executor);
        self.quarantine_marked(generation, Mark::Command, named)
    }

    /// Every pending generation, from one read of the marker under its lock,
    /// waiting at most `wait` for it: for a reader that must never queue
    /// behind control, and looks again later instead. What it read is synced
    /// to disk before it is said, so no change is taken as settled on a
    /// clearing a crash could still undo.
    ///
    /// # Errors
    ///
    /// Busy when the lock is held past `wait`, marker or not; refuses a
    /// marker that cannot be read, which may name any change; fails when what
    /// was read cannot be kept for good, and on other storage failures.
    pub fn pending_within(&self, wait: Duration) -> Result<Vec<String>> {
        let _lock = self.lock_marker_within(wait)?;
        let pending = pending(&self.marker)?;
        if pending.iter().any(|entry| entry == UNREADABLE) {
            return Err(Error::new(
                Code::State,
                "The pending changes cannot be read.",
            ));
        }
        // A clearing whose folder sync failed left what was read unsynced.
        #[cfg(test)]
        if super::faults::marker_sync_fails() {
            return Err(std::io::Error::other("the folder could not be synced").into());
        }
        File::open(&self.directory)
            .and_then(|folder| folder.sync_all())
            .map_err(context("Sync pending marker folder", &self.directory))?;
        Ok(pending)
    }

    /// Every pending change `executor` carries out whose claim no live
    /// process holds: those whose holder ended without settling them, which
    /// only that executor's answer settles. From one read of the marker and
    /// its executors under the marker lock, waiting at most `wait` for it,
    /// as [`Controller::pending_within`] reads.
    ///
    /// # Errors
    ///
    /// As [`Controller::pending_within`], and when who carries out the
    /// pending changes cannot be read.
    pub fn unheld_by(&self, executor: Executor, wait: Duration) -> Result<Vec<String>> {
        let _lock = self.lock_marker_within(wait)?;
        let pending = pending(&self.marker)?;
        if pending.iter().any(|entry| entry == UNREADABLE) {
            return Err(Error::new(
                Code::State,
                "The pending changes cannot be read.",
            ));
        }
        let executors = executors::read(&self.executors)?;
        Ok(pending
            .into_iter()
            .filter(|generation| executors.get(generation) == Some(&executor))
            .filter(|generation| !self.in_progress(generation))
            .collect())
    }

    /// Every pending change that keeps control from being taken now, as
    /// taking it judges them: none when nothing is pending or only commands
    /// agents started still run; otherwise every pending change but those
    /// commands, whoever carries it out. From one read of the marker and its
    /// executors under the marker lock, waiting at most `wait` for it, as
    /// [`Controller::pending_within`] reads.
    ///
    /// # Errors
    ///
    /// Busy when the lock is held past `wait`; refuses a marker that cannot
    /// be read or names nothing, an unknown outcome to taking control, and
    /// who carries out the pending changes when that cannot be read.
    pub fn unresolved_within(&self, wait: Duration) -> Result<Vec<Unresolved>> {
        let _lock = self.lock_marker_within(wait)?;
        if matches!(
            self.read_unsettled()?,
            Unsettled::Nothing | Unsettled::Commands
        ) {
            return Ok(Vec::new());
        }
        let pending = pending(&self.marker)?;
        if pending.is_empty() || pending.iter().any(|entry| entry == UNREADABLE) {
            return Err(Error::new(
                Code::State,
                "The pending changes cannot be read.",
            ));
        }
        let executors = executors::read(&self.executors)?;
        let marked = self.commands();
        Ok(pending
            .into_iter()
            .filter(|generation| !self.running_command(&marked, generation))
            .map(|generation| Unresolved {
                executor: executors.get(&generation).copied(),
                holder: self.liveness(&generation),
                generation,
            })
            .collect())
    }

    /// [`Controller::quarantine_as`], recorded as `mark` says, its executor
    /// named when it is not the desktop's own services.
    fn quarantine_marked(
        &self,
        generation: &str,
        mark: Mark,
        executor: Option<Executor>,
    ) -> Result<Pending> {
        self.quarantine_marked_within(generation, mark, executor, None)
    }

    /// [`Controller::quarantine_marked`], the marker lock waited for at most
    /// `wait` when one is given.
    fn quarantine_marked_within(
        &self,
        generation: &str,
        mark: Mark,
        executor: Option<Executor>,
        wait: Option<Duration>,
    ) -> Result<Pending> {
        let hold = self.interrupt.claim();
        let live = self
            .live(generation)
            .ok_or_else(|| Error::new(Code::State, "Pending generations are hexadecimal tokens"))?;

        self.make_directory()?;
        let file = open_lock(&live)?;
        FileExt::try_lock_exclusive(&file.0).map_err(|cause| {
            Error::new(Code::Busy, "That generation is already being carried out")
                .with_source(cause)
        })?;

        let claim = Pending {
            generation: generation.to_owned(),
            live,
            lock: file,
            _hold: hold,
        };

        // Claim files are only unlinked under the marker lock or by their
        // holder, so if the path still names the locked file here, it will
        // until the claim is dropped.
        let _lock = match wait {
            Some(wait) => self.lock_marker_within(wait)?,
            None => self.lock_marker()?,
        };
        if !claim.names_own_file() {
            return Err(Error::new(
                Code::Busy,
                "That generation's claim changed while it was taken",
            ));
        }

        let mut pending = pending(&self.marker)?;
        // Nothing pending: a damaged executor record names nobody's change,
        // and is cleared before a new one is taken.
        if pending.is_empty() && executors::read(&self.executors).is_err() {
            executors::forget(&self.executors)?;
        }
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
        // Named before the marker names it, like a command's mark: a change
        // whose executor cannot be named is never recorded.
        if let Some(executor) = executor {
            executors::name(&self.executors, generation, executor, &pending)?;
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
    pub fn abandon_command(&self, claim: Pending) {
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
    ///
    /// # Errors
    ///
    /// Returns lock, read or write failures while settling only the named acknowledged
    /// generation. Other pending generations remain.
    pub fn clear_if(&self, generation: &str) -> Result<()> {
        let _lock = self.lock_marker()?;
        self.clear_locked(generation)
    }

    /// [`Controller::clear_if`], waiting at most `wait` for the marker
    /// lock: for a settler that must never queue behind control, and tries
    /// again later instead.
    ///
    /// # Errors
    ///
    /// Busy when the lock is held past `wait`, with nothing settled; else
    /// as [`Controller::clear_if`].
    pub fn clear_if_within(&self, generation: &str, wait: Duration) -> Result<()> {
        let _lock = self.lock_marker_within(wait)?;
        self.clear_locked(generation)
    }

    /// [`Controller::clear_if_within`], refusing a marker that cannot be
    /// read whole: one that may name any change is never taken as cleared
    /// of this one.
    ///
    /// # Errors
    ///
    /// Refuses a marker that cannot be read, with nothing settled; else as
    /// [`Controller::clear_if_within`].
    pub fn clear_known_within(&self, generation: &str, wait: Duration) -> Result<()> {
        let _lock = self.lock_marker_within(wait)?;
        if pending(&self.marker)?
            .iter()
            .any(|entry| entry == UNREADABLE)
        {
            return Err(Error::new(
                Code::State,
                "The pending changes cannot be read; nothing is settled.",
            ));
        }
        self.clear_locked(generation)
    }

    /// Settle `generation` while the caller holds the marker lock.
    fn clear_locked(&self, generation: &str) -> Result<()> {
        if let Some(live) = self.live(generation)
            && !self.in_progress(generation)
        {
            remove_claim(&live);
        }

        let mut pending = pending(&self.marker)?;
        pending.retain(|entry| entry != generation);
        self.keep_pending(&pending)
    }

    /// Verified recovery settles every pending mutation no live claim holds
    /// and removes every claim file no live mutation holds, listed or not. A
    /// mutation still being carried out (an approved root run among them)
    /// stays pending: only its own end settles it.
    ///
    /// # Errors
    ///
    /// Returns lock, enumeration or storage failures during verified recovery. Live mutations
    /// remain pending even when other records are settled.
    pub fn clear(&self) -> Result<()> {
        let _lock = self.lock_marker()?;
        // A change another executor carries out waits for its answer.
        let listed = pending(&self.marker)?;
        let executors = match executors::read(&self.executors) {
            Ok(executors) => executors,
            // Nothing pending: a damaged record names nobody's change.
            Err(_) if listed.is_empty() => {
                executors::forget(&self.executors)?;
                ByGeneration::new()
            }
            Err(error) => return Err(error),
        };
        // A marker that cannot be read may name one of those: nothing is
        // settled while another executor holds a change.
        if listed.iter().any(|entry| entry == UNREADABLE)
            && executors.values().any(|executor| !executor.local())
        {
            return Err(Error::new(
                Code::State,
                "The pending changes cannot be read while another executor holds one; nothing is settled.",
            ));
        }
        let running: Vec<String> = listed
            .into_iter()
            .filter(|generation| {
                self.in_progress(generation)
                    || executors
                        .get(generation)
                        .is_some_and(|executor| !executor.local())
            })
            .collect();
        let prefix = format!("{}.", self.name);
        let replacing = format!(".{prefix}");
        let stale =
            |generation: &str| self.live(generation).is_some() && !self.in_progress(generation);

        for entry in fs::read_dir(&self.directory)?.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            // Only a writer holding the marker lock has a replacement under
            // way, so one found now was left by a writer that was killed.
            if name.starts_with(&replacing) && name.ends_with(REPLACING) {
                let _ = fs::remove_file(entry.path());
                continue;
            }
            let Some(name) = name.strip_prefix(&prefix) else {
                continue;
            };
            if name.strip_suffix(".live").is_some_and(stale) {
                remove_claim(&entry.path());
                continue;
            }
            // An executor record still unnamed when its claim is no longer
            // held was left by a writer that was killed.
            if name
                .split_once(&format!(".{EXECUTOR_WRITING}"))
                .is_some_and(|(generation, _)| stale(generation))
            {
                let _ = fs::remove_file(entry.path());
            }
        }
        self.keep_pending(&running)
    }

    /// Under the marker lock, prune ancillary records before publishing settlement.
    /// Lost labels are harmless; lost command marks conservatively become unknown.
    fn keep_pending(&self, pending: &[String]) -> Result<()> {
        // Who carries out a change decides what recovery may settle, so a
        // record that cannot be kept in step fails the settlement.
        executors::keep(&self.executors, pending)?;
        let _ = by_generation::keep::<Key>(&self.causes, pending);
        let _ = commands::keep(&self.commands, pending);
        let _ = by_generation::keep::<Note>(&self.running, pending);
        if pending.is_empty() {
            remove_marker(&self.marker)
        } else {
            replace_private(&self.marker, &serde_json::to_vec(pending)?)
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
pub struct Snapshot {
    /// Generations no mutation in progress will settle.
    pub unresolved: Vec<String>,
    /// How many are pending in all.
    pub total: usize,
    /// How many are commands agents started with `run`, still running.
    pub commands: usize,
}

/// A pending change that keeps control from being taken, as
/// [`Controller::unresolved_within`] reads it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Unresolved {
    pub generation: String,
    /// Who carries it out, when it is not the desktop's own services.
    pub executor: Option<Executor>,
    /// Whether a process still carries it out.
    pub holder: Liveness,
}

/// Whether a process still carries out a claimed change, as far as can be
/// told.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Liveness {
    /// Proven: its claim's lock is held, or the executor its record names
    /// is read and is that very process.
    Live,
    /// Proven gone: no claim, or neither a lock nor an executor that runs.
    Gone,
    /// Cannot be told: the claim, its record or the process it names cannot
    /// be read.
    Unknown,
}

/// What a marker that cannot be read names: an unknown outcome, pending.
pub(super) const UNREADABLE: &str = "legacy-unknown";

/// Pending generations in a marker; an unreadable one stays pending as unknown.
pub(super) fn pending(marker: &Path) -> Result<Vec<String>> {
    match fs::read_to_string(marker) {
        Ok(text) => Ok(serde_json::from_str(&text).unwrap_or_else(|_| vec![UNREADABLE.into()])),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(context("Read pending marker", marker)(error)),
    }
}

/// Remove the marker, its last change settled, for good: its folder is
/// synced, so a crash never brings a settled change back, nor leaves the
/// station unsure whether it settled one it told a desktop it had.
fn remove_marker(path: &Path) -> Result<()> {
    // Gone already, its removal may not have been kept: kept again.
    match fs::remove_file(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        other => other?,
    }
    #[cfg(test)]
    if super::faults::marker_sync_fails() {
        return Err(std::io::Error::other("the folder could not be synced").into());
    }
    if let Some(folder) = path.parent() {
        File::open(folder)?.sync_all()?;
    }
    Ok(())
}
