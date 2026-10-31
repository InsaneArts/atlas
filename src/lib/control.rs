//! Who may act on the helper, and what is still unsettled: one controller at
//! a time ([`Controller`], a file lock per helper account), a marker of every
//! mutation whose outcome is unknown until verified recovery clears it, and
//! the short-lived [`Lease`] a viewer holds while a person has control.
//! A command an agent starts with `run` lets go of the lock once it is sent
//! and is marked as one, so other agents' actions go on beside it while
//! recovery still waits for it.
//! Beside the lock and the marker it keeps labels for people and agents:
//! which host session holds control, which waits, and which claimed each
//! pending change. Nothing here decides anything by them.
mod busy;
mod by_generation;
mod commands;
mod executor;
#[cfg(test)]
mod faults;
mod lease;
mod marker;
mod notes;
#[cfg(test)]
mod tests;
mod turns;
mod wait;

pub(crate) use busy::seconds;
pub(crate) use executor::{ExecutorClaims, record_executor};
pub use lease::{ActionPermit, Lease, SharedLease};
pub use notes::Note;
pub(crate) use wait::poll;
pub use wait::{WaitEvent, WaitHook};

use busy::busy_command;
use by_generation::ByGeneration;
use executor::claim_executor_running;
use marker::{Unsettled, pending};

use crate::{
    config::home,
    error::{Code, Error, Result},
    files::{open_lock, probe_write},
    host_session::{HostSession, Key, Label},
    interrupt::{Hold, Interrupt},
};
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

/// How long an agent's command that changes the helper waits for another
/// Atlas command to let go of it, unless told otherwise: long enough for the
/// usual command ahead, short enough that the wait and a default command
/// stay within an agent tool's two-minute limit.
pub const CONTROL_WAIT: Duration = Duration::from_secs(30);

/// The longest a command may be told to wait for control: as long as the
/// longest run.
pub const CONTROL_WAIT_MAX: Duration = Duration::from_secs(600);

/// The most commands agents started with `run` that may run at once, so
/// the helper keeps connections free for reads and other actions.
pub const RUNNING_COMMANDS_MAX: usize = 8;

/// What an acquisition may go on beside.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Beside {
    /// Nothing pending at all, running or not.
    Nothing,
    /// Commands agents started with `run` that are still running. Every
    /// other pending outcome still refuses.
    Commands,
}

/// What control is taken for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Purpose {
    /// Recovery, setup and removal: refused while any pending change is
    /// still being carried out, so never beside a running command.
    Recovery,
    /// A change, going on beside what it names.
    Change(Beside),
}

/// A mutation still being carried out. For its lifetime it holds an
/// exclusive lock on a file named for its generation, so any process can tell
/// work in progress from an outcome nothing will settle. Its marker entry
/// outlives it until the outcome is acknowledged.
#[must_use = "dropping it at once reports the mutation as abandoned"]
#[derive(Debug)]
pub struct Pending {
    generation: String,
    live: PathBuf,
    /// The claim file, exclusively locked for as long as it is open.
    lock: File,
    /// Keeps a signal's stop waiting until the claim file is let go.
    _hold: Hold,
}

impl std::ops::Deref for Pending {
    type Target = str;
    fn deref(&self) -> &str {
        self.generation()
    }
}

impl Pending {
    /// The generation this mutation is recorded under.
    pub fn generation(&self) -> &str {
        &self.generation
    }

    /// Whether the claim path still names the file this claim locked.
    fn names_own_file(&self) -> bool {
        let (named, held) = (fs::symlink_metadata(&self.live), self.lock.metadata());
        matches!((named, held), (Ok(a), Ok(b)) if (a.dev(), a.ino()) == (b.dev(), b.ino()))
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        // Only an unlocked claim file is ever removed by others, and a
        // replacement can only appear once this one is gone, so a path that
        // still names the locked file stays ours until it is removed here.
        // The lock goes with the descriptor.
        if self.names_own_file() && !claim_executor_running(&self.lock, &self.live) {
            remove_claim(&self.live);
        }
    }
}

fn remove_claim(live: &Path) {
    match fs::remove_file(live) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return,
        _ => {}
    }
    #[cfg(test)]
    assert!(!faults::claim_removal_stops(), "interrupted cleanup");
    let _ = fs::remove_file(live.with_extension("executor.live"));
}

/// Exclusive control of one helper, held until dropped.
#[derive(Debug)]
pub struct Ownership {
    _lock: File,
    /// The note that the person holds this ownership through a viewer, gone
    /// with it.
    person: Option<PathBuf>,
    /// The note naming the host session that holds it, gone before the lock
    /// is, so the lock is never free while a note still names its holder.
    holder: Option<PathBuf>,
}

impl Drop for Ownership {
    fn drop(&mut self) {
        for note in [&self.holder, &self.person].into_iter().flatten() {
            let _ = fs::remove_file(note);
        }
        let _ = FileExt::unlock(&self._lock);
    }
}

/// One pending change, as far as people and agents are told of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cause {
    pub generation: String,
    /// Whether a live process is still carrying it out.
    pub running: bool,
    /// Whether it is a command an agent started with `run`, still running
    /// under its own claim: work under way, not an unknown outcome.
    pub command: bool,
    /// The host session that claimed it, when one is recorded.
    pub session: Option<Key>,
}

/// Controller exclusion and pending-mutation state for one helper account,
/// keyed by helper UID so a changed socket path cannot admit a second
/// controller or hide a pending mutation.
#[derive(Clone)]
pub struct Controller {
    directory: PathBuf,
    lock: PathBuf,
    marker: PathBuf,
    marker_lock: PathBuf,
    /// Present while the person holds control through a viewer, so whoever
    /// finds the controller busy is told to wait for them.
    person: PathBuf,
    /// Present while the person has stopped agents: kept on the person's
    /// side, so a Stop holds even while the helper's input service is down.
    stopped: PathBuf,
    name: String,
    /// How long [`Controller::acquire`] may keep trying, in all, while
    /// another controller has the lock, before answering busy. Zero
    /// answers at once.
    wait: Duration,
    /// When that wait runs out, shared by every acquire and every clone.
    until: Option<Instant>,
    /// Whether a Stop ends the wait and refuses control taken after one.
    heeds_stop: bool,
    /// Told when [`Controller::acquire`] starts waiting and when it stops.
    on_wait: Option<WaitHook>,
    /// The stop a signal asks: it refuses control from then on, and waits
    /// for the changes this controller has under way.
    interrupt: Interrupt,
    /// The host session this controller's changes are claimed for, so one
    /// of unknown outcome can be shown to the session that made it. A label
    /// only: what blocks and what recovery clears never depend on it.
    label: Option<Label>,
    /// Which host session claimed each pending generation, beside the marker.
    causes: PathBuf,
    /// Which pending generations are commands agents started with `run`,
    /// beside the marker.
    commands: PathBuf,
    /// Who started each of those commands, beside the marker.
    running: PathBuf,
    /// Which host session holds control right now, beside the lock.
    holder: PathBuf,
    /// The commands waiting for control, one note each, in the order they
    /// take it.
    waiting: PathBuf,
}

impl Controller {
    pub fn new(uid: u32) -> Result<Self> {
        Ok(Self::at(uid, &home()?.join("Library/Caches/Atlas")))
    }

    /// Locating state is read-only; the directory appears with the first lock.
    pub fn at(uid: u32, directory: &Path) -> Self {
        let digest = format!("{:x}", Sha256::digest(format!("helper:{uid}").as_bytes()));
        let name = &digest[..24];
        Self {
            directory: directory.to_owned(),
            lock: directory.join(format!("{name}.lock")),
            marker: directory.join(format!("{name}.uncertain.json")),
            marker_lock: directory.join(format!("{name}.state.lock")),
            person: directory.join(format!("{name}.person")),
            stopped: directory.join(format!("{name}.stopped")),
            name: name.to_owned(),
            wait: Duration::ZERO,
            until: None,
            heeds_stop: false,
            on_wait: None,
            interrupt: Interrupt::process().clone(),
            label: None,
            causes: directory.join(format!("{name}.causes.json")),
            commands: directory.join(format!("{name}.commands.json")),
            running: directory.join(format!("{name}.running.json")),
            holder: directory.join(format!("{name}.holder.json")),
            waiting: directory.join(format!("{name}.waiting")),
        }
    }

    /// This controller, trying for up to `wait` to take control. The wait
    /// is one allowance, starting now, for every acquire of this controller
    /// and its clones: each spends what it waits, and the next gets what is
    /// left. It blocks the calling thread, so only a command-line process
    /// whose command is its only work may be given one; a server (viewer,
    /// MCP, watch) never is.
    #[must_use]
    pub fn waiting(mut self, wait: Duration) -> Self {
        self.wait = wait;
        self.until = Some(Instant::now() + wait);
        self
    }

    /// How long, in all, [`Controller::acquire`] may wait for another
    /// controller.
    pub fn wait(&self) -> Duration {
        self.wait
    }

    /// What is left of [`Controller::wait`].
    pub fn remaining(&self) -> Duration {
        self.until.map_or(Duration::ZERO, |until| {
            until.saturating_duration_since(Instant::now())
        })
    }

    /// This controller, for a command the person's Stop refuses: once
    /// agents are stopped, a wait ends with `stopped`, and control the
    /// lock gives after waiting is refused too. With no wait, the Stop is
    /// the caller's to check.
    #[must_use]
    pub fn heeding_stop(mut self) -> Self {
        self.heeds_stop = true;
        self
    }

    /// This controller, telling `hook` when a wait for another controller
    /// begins and when it ends. A try that takes control at once, or is
    /// refused at once, waits for nothing and tells nothing. A process that
    /// exits while waiting (`std::process::exit`, a signal) never tells
    /// [`WaitEvent::Ended`].
    #[must_use]
    pub fn on_wait(mut self, hook: impl Fn(WaitEvent) + Send + Sync + 'static) -> Self {
        self.on_wait = Some(Arc::new(hook));
        self
    }

    /// This controller, claiming its changes and holding control for host
    /// `session`, as labels only.
    #[must_use]
    pub fn labelled(mut self, session: Option<&HostSession>) -> Self {
        self.label = session.map(HostSession::label);
        self
    }

    /// This controller, stopped by `interrupt` instead of this process's
    /// signals.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn interrupted_by(mut self, interrupt: Interrupt) -> Self {
        self.interrupt = interrupt;
        self
    }

    /// The stop a signal asks of this controller's command.
    pub(crate) fn interrupt(&self) -> &Interrupt {
        &self.interrupt
    }

    /// Whether this process may keep its locks and claims here at all. An
    /// agent's sandbox may forbid writing, and the error then says so.
    pub fn writable(&self) -> Result<()> {
        if fs::symlink_metadata(&self.directory).is_ok() {
            real_directory(&self.directory)?;
        }
        probe_write(&self.directory)
    }

    /// The state folder, made if missing, and refused when it is a link:
    /// locks and records kept through a link would be another folder's once
    /// the link is changed or removed, and a second controller could take a
    /// lock of its own.
    fn make_directory(&self) -> Result<()> {
        fs::create_dir_all(&self.directory)
            .map_err(|error| Error::writing(&self.directory, error))?;
        real_directory(&self.directory)
    }

    /// Take control. Recovery is refused while any pending change is still
    /// being carried out; anything else while any change is pending at all,
    /// a command an agent started included.
    pub fn acquire(&self, recovery: bool) -> Result<Ownership> {
        self.acquire_for(if recovery {
            Purpose::Recovery
        } else {
            Purpose::Change(Beside::Nothing)
        })
    }

    /// Take control for service replacement, refusing a stalled marker writer
    /// even when interruption of other sessions is permitted.
    pub(crate) fn acquire_for_service_change(
        &self,
        recovery: bool,
        marker_wait: Duration,
    ) -> Result<Ownership> {
        let lock = self.take_lock()?;
        let _marker = self.lock_marker_within(marker_wait)?;
        if recovery {
            self.read_nothing_running()?;
        } else {
            Self::require_unsettled_beside(
                self.read_unsettled().unwrap_or(Unsettled::Unknown),
                Beside::Nothing,
            )?;
        }
        Ok(self.owning(lock))
    }

    /// [`Controller::acquire`] for an agent's own action, going on beside
    /// commands still running under their own claims.
    pub fn acquire_beside_commands(&self) -> Result<Ownership> {
        self.acquire_for(Purpose::Change(Beside::Commands))
    }

    /// [`Controller::acquire`] for `purpose`.
    fn acquire_for(&self, purpose: Purpose) -> Result<Ownership> {
        let lock = self.take_lock()?;
        match purpose {
            Purpose::Recovery => self.require_nothing_running()?,
            Purpose::Change(beside) => self.require_known_beside(beside)?,
        }
        Ok(self.owning(lock))
    }

    /// [`Controller::acquire`] for a cleanup, and whether an earlier
    /// outcome is unknown. While one is, a cleanup still stops background
    /// jobs and checks collected copies, which recover nothing and change no
    /// app, and leaves every other step to after recovery; it holds control
    /// as any change does, so recovery cannot begin while it runs, and its
    /// own changes are claimed as any change's are. A command still running,
    /// the person's control and a Stop refuse it as they refuse any change.
    pub(crate) fn acquire_for_cleanup(&self) -> Result<(Ownership, bool)> {
        let lock = self.take_lock()?;
        let unknown = match self.require_known_beside(Beside::Nothing) {
            Ok(()) => false,
            Err(error) if error.code == Code::Uncertain => true,
            Err(error) => return Err(error),
        };
        Ok((self.owning(lock), unknown))
    }

    /// The controller lock, taken as [`Controller::take`] takes it.
    fn take_lock(&self) -> Result<File> {
        // Nothing new starts once a signal asked this command to stop.
        self.interrupt.check()?;
        self.make_directory()?;

        let lock = open_lock(&self.lock)?;
        self.take(&lock)?;

        // Nobody held it, so a note left by a viewer that ended is stale.
        let _ = fs::remove_file(&self.person);
        Ok(lock)
    }

    /// Ownership of `lock`, held, with the note naming its holder.
    fn owning(&self, lock: File) -> Ownership {
        Ownership {
            _lock: lock,
            person: None,
            holder: self.note_holder(),
        }
    }

    /// Name this controller's host session beside the lock it now holds, or
    /// clear the note an earlier holder left when it has none: a label for
    /// whoever finds the helper busy. The note's path, when one was written.
    fn note_holder(&self) -> Option<PathBuf> {
        let Some(label) = &self.label else {
            let _ = fs::remove_file(&self.holder);
            return None;
        };

        Note::now(Some(label))
            .and_then(|note| note.write(&self.holder).ok())
            .map(|()| self.holder.clone())
    }

    /// The holder note, as written, when there is one that parses. Whether
    /// its command still runs is the reader's to check.
    pub fn holder_note(&self) -> Option<Note> {
        Note::read(&self.holder)
    }

    /// Bounded display candidates; the reply's frozen batch checks liveness.
    pub(crate) fn waiting_candidates(&self) -> Vec<Note> {
        turns::candidates(&self.waiting)
    }

    pub(crate) fn running_candidates(&self) -> Vec<Note> {
        let marked = self.commands();
        let mut notes: Vec<_> = by_generation::read::<ByGeneration<Note>>(&self.running)
            .into_iter()
            .filter(|(generation, _)| self.running_command(&marked, generation))
            .map(|(_, note)| note)
            .take(crate::host_session::liveness::BUDGET)
            .collect();
        notes.sort_by_key(|note| note.since);
        notes
    }

    /// The notes of the commands waiting for control now, oldest first:
    /// for apps to show who waits, never to decide who goes next.
    pub fn waiting_notes(&self) -> Vec<Note> {
        let mut notes = turns::waiting(&self.waiting);
        notes.sort_by_key(|note| note.since);
        notes
    }

    /// Waiting notes only when the bounded read accounted for every entry.
    /// An incomplete read cannot establish that service replacement is idle.
    pub(crate) fn checked_waiting_notes(&self) -> Option<Vec<Note>> {
        turns::checked_waiting(&self.waiting)
    }

    /// The notes of the commands agents started with `run` that still run,
    /// oldest first: such a command holds no control while it runs, so
    /// only its note names a session on its first command. For apps to
    /// show who acts, never to decide who may.
    pub fn running_notes(&self) -> Vec<Note> {
        let marked = self.commands();
        let mut notes: Vec<Note> = by_generation::read::<ByGeneration<Note>>(&self.running)
            .into_iter()
            .filter(|(generation, note)| marked.contains(generation) && note.live())
            .map(|(_, note)| note)
            .collect();
        notes.sort_by_key(|note| note.since);
        notes
    }

    /// The notes of the commands agents started with `run` whose change is
    /// still pending, live or not: a command killed on its session's first
    /// command leaves no record, and only its note names that session, as
    /// its record would have. For naming whose an unknown outcome is, never
    /// to decide anything.
    pub fn pending_notes(&self) -> Vec<Note> {
        let Ok(pending) = self.pending() else {
            return Vec::new();
        };
        by_generation::read::<ByGeneration<Note>>(&self.running)
            .into_iter()
            .filter(|(generation, _)| pending.contains(generation))
            .map(|(_, note)| note)
            .collect()
    }

    /// Where the holder note is kept, for tests that stand in for another
    /// command's.
    #[cfg(test)]
    pub(crate) fn holder_path(&self) -> &Path {
        &self.holder
    }

    /// Whether the person stopped agents, as noted on their side.
    pub fn stop_noted(&self) -> bool {
        self.stopped.exists()
    }

    /// Note that the person stopped agents (`on`), or let them go on.
    pub fn note_stop(&self, on: bool) -> Result<()> {
        if fs::symlink_metadata(&self.directory).is_ok() {
            real_directory(&self.directory)?;
        }
        if on {
            self.make_directory()?;
            fs::write(&self.stopped, b"").map_err(|error| Error::writing(&self.directory, error))
        } else {
            match fs::remove_file(&self.stopped) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    Err(Error::writing(&self.directory, error))
                }
                _ => Ok(()),
            }
        }
    }

    /// [`Controller::acquire`] for the person's control through a viewer:
    /// while it is held, others hear that the person has control. A command
    /// an agent started keeps running beside it, as it does beside other
    /// agents' actions.
    pub fn acquire_for_person(&self) -> Result<Ownership> {
        let mut ownership = self.acquire_beside_commands()?;
        fs::write(&self.person, b"").map_err(|error| Error::writing(&self.directory, error))?;
        ownership.person = Some(self.person.clone());
        Ok(ownership)
    }

    /// Refuse while any change is pending, a command still running
    /// included.
    pub fn require_known(&self) -> Result<()> {
        self.require_known_beside(Beside::Nothing)
    }

    /// Refuse while an outcome is unknown. A command still running is not
    /// unknown: beside commands it is let through; otherwise it answers
    /// `busy`, held by an agent, so nobody is told to recover it.
    pub(crate) fn require_known_beside(&self, beside: Beside) -> Result<()> {
        if !self.marker.try_exists()? {
            return Ok(());
        }

        // A marker that cannot be read is an unknown outcome.
        Self::require_unsettled_beside(self.unsettled().unwrap_or(Unsettled::Unknown), beside)
    }

    /// Apply the acquisition rules to state read under the marker lock.
    fn require_unsettled_beside(unsettled: Unsettled, beside: Beside) -> Result<()> {
        match (unsettled, beside) {
            (Unsettled::Nothing, _) | (Unsettled::Commands, Beside::Commands) => Ok(()),
            (Unsettled::Commands, Beside::Nothing) => {
                Err(busy_command(Purpose::Change(Beside::Nothing)))
            }
            (Unsettled::Unknown, _) => Err(Error::new(
                Code::Uncertain,
                "The last action's outcome is unknown; Atlas waits until it is recovered.",
            )),
        }
    }

    /// Refuse recovery while a pending change is still being carried out:
    /// a command an agent started, or an executor that outlived its caller.
    /// Recovery never settles either.
    fn require_nothing_running(&self) -> Result<()> {
        let _lock = self.lock_marker()?;
        self.read_nothing_running()
    }

    /// Check running claims while the caller holds the marker lock.
    fn read_nothing_running(&self) -> Result<()> {
        let pending = pending(&self.marker)?;
        let marked = self.commands();
        if pending
            .iter()
            .any(|generation| self.running_command(&marked, generation))
        {
            return Err(busy_command(Purpose::Recovery));
        }
        if pending
            .iter()
            .any(|generation| self.in_progress(generation))
        {
            return Err(Error::new(
                Code::Busy,
                "An Atlas executor is still carrying out an action. Wait for it to end before recovering.",
            ));
        }

        Ok(())
    }
}

/// Refuse `directory` unless it is a real folder of this account, not a
/// link to one.
fn real_directory(directory: &Path) -> Result<()> {
    let metadata =
        fs::symlink_metadata(directory).map_err(|error| Error::writing(directory, error))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != nix::unistd::getuid().as_raw()
    {
        return Err(Error::new(
            Code::State,
            format!(
                "{} is a link or not a folder of this account. Remove it, then try again.",
                directory.display()
            ),
        ));
    }

    Ok(())
}
