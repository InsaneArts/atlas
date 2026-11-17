//! The supervisors a machine guest's daemon started, kept on the guest's
//! own disk before any of their programs may start, so a daemon that starts
//! after one that was lost knows what it left. The file is vouched for by
//! its commitment in the Keychain ([`super::commitment`]): one deleted or
//! rolled back reads as unknown, never as none.
use super::{
    commitment::{Commitment, Judged, Revision},
    record_file::{self, number},
    supervise::{DONE, ENDED, proof},
    vault::Vault,
};
use atlas_core::{
    digest::bytes_sha256,
    error::{Code, Error, Result},
    files::write_atomic,
    process::Instance,
};
use atlas_session::generation::Generation;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
    time::Duration,
};

/// The store's name among the guest's recovery stores.
pub const STORE: &str = "processes";

/// The most supervisors kept at once.
pub const MOST: usize = 256;

/// The longest the record may be.
const LIMIT: u64 = 64 * 1024;

const VERSION: u32 = 1;

/// One supervisor, as it was started.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Supervised {
    /// The boot it started in.
    pub boot: String,
    /// The generation it was started for.
    pub generation: Generation,
    pub supervisor: Instance,
    /// The change it was started for, kept before its program may start:
    /// what tells whether the work of a change whose outcome is unknown
    /// may still run. None for a supervisor kept by an earlier build.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<atlas_core::files::Token>,
    /// The background job it runs, when it runs one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<JobRecord>,
}

/// What a background job a supervisor runs is, as kept beside it: never its
/// arguments, which may carry secrets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobRecord {
    pub id: atlas_core::protocol::JobId,
    pub name: String,
    pub task: Option<atlas_core::host::Key>,
    pub session: Option<atlas_core::host::Key>,
    pub started_at: u64,
    pub ends_at: u64,
    /// The change that started it.
    pub change: atlas_core::files::Token,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u32,
    number: u64,
    work: Vec<Supervised>,
}

/// What a machine's commands run under: how a supervisor is started, where
/// it leaves its proof, and where it is kept before its program may start.
pub struct Supervising {
    pub supervisor: super::supervise::invocation::Supervisor,
    pub proofs: PathBuf,
    pub kept: std::sync::Arc<dyn Remembered>,
}

/// Where a machine's supervisors are kept on its disk.
pub trait Remembered: Send + Sync {
    /// Every supervisor kept now.
    ///
    /// # Errors
    ///
    /// Fails when the record cannot be trusted.
    fn kept(&self) -> Result<Vec<Supervised>>;

    /// Keep `supervised` before its program may start.
    ///
    /// # Errors
    ///
    /// Busy when too many are kept; fails when it cannot be kept.
    fn add(&self, supervised: Supervised) -> Result<()>;

    /// Forget `retired`, each exactly as it was kept.
    ///
    /// # Errors
    ///
    /// Fails when the record cannot be trusted or kept.
    fn retire(&self, retired: &[Supervised]) -> Result<()>;
}

/// The guest's supervisors in `path`, vouched for by `vault`.
pub struct Processes<V> {
    path: PathBuf,
    commitment: Commitment<V>,
    writing: Mutex<()>,
}

impl<V: Vault> Processes<V> {
    pub fn new(path: impl Into<PathBuf>, vault: V) -> Self {
        Self {
            path: path.into(),
            commitment: Commitment::new(vault, STORE),
            writing: Mutex::new(()),
        }
    }

    /// The record on disk and its number, once its commitment vouches for
    /// it; none when nothing was ever kept.
    fn trusted(&self) -> Result<(u64, Vec<Supervised>)> {
        let found = record_file::read_regular(&self.path, LIMIT).map_err(|_| unknown())?;
        let revision = found.as_ref().map(|bytes| Revision {
            number: number(bytes),
            digest: bytes_sha256(bytes),
        });
        let judged = self.commitment.judge(revision.as_ref())?;
        if judged == Judged::Unknown {
            return Err(unknown());
        }
        // Committed but never written: a supervisor may have been told to
        // start under it, so it is never given up into what came before.
        if judged == Judged::Current && self.commitment.state()?.1 {
            return Err(unknown());
        }
        let trusted = match found {
            Some(bytes) => {
                let record: Record = serde_json::from_slice(&bytes).map_err(|_| unknown())?;
                if record.version != VERSION || record.work.len() > MOST {
                    return Err(unknown());
                }
                (record.number, record.work)
            }
            None => (0, Vec::new()),
        };
        if judged == Judged::Next {
            sync_folder(&self.path)?;
            self.commitment.settle()?;
        }
        Ok(trusted)
    }

    /// Every supervisor kept now.
    ///
    /// # Errors
    ///
    /// Fails when the record cannot be trusted: what was left is then
    /// unknown.
    pub fn kept(&self) -> Result<Vec<Supervised>> {
        let _one = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        self.trusted().map(|(_, work)| work)
    }

    /// Keep what `next` makes of the supervisors kept now, read fresh and
    /// written under one lock: committed first, then written and kept for
    /// good, then made current. The same list again changes nothing.
    fn keep(&self, next: impl FnOnce(&[Supervised]) -> Result<Vec<Supervised>>) -> Result<()> {
        let _one = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        let (number, before) = self.trusted()?;
        let work = next(&before)?;
        if work == before {
            return Ok(());
        }
        let number = number
            .checked_add(1)
            .ok_or_else(|| Error::new(Code::State, "The supervisors cannot be kept again."))?;
        let bytes = serde_json::to_vec(&Record {
            version: VERSION,
            number,
            work,
        })?;
        self.commitment.begin(Revision {
            number,
            digest: bytes_sha256(&bytes),
        })?;
        write_atomic(&self.path, &bytes, 0o600)?;
        sync_folder(&self.path)?;
        self.commitment.settle()
    }

    /// Keep `supervised` before its program may start.
    ///
    /// # Errors
    ///
    /// Busy when [`MOST`] are kept already; fails when it cannot be kept.
    pub fn add(&self, supervised: Supervised) -> Result<()> {
        self.keep(|before| {
            if before.contains(&supervised) {
                return Ok(before.to_vec());
            }
            if before.len() >= MOST {
                return Err(Error::new(
                    Code::Busy,
                    "Too much is running on this desktop to start more",
                ));
            }
            let mut work = before.to_vec();
            work.push(supervised);
            Ok(work)
        })
    }

    /// Forget `retired`, each exactly as it was kept; any other stays.
    ///
    /// # Errors
    ///
    /// Fails when the record cannot be trusted or kept.
    pub fn retire(&self, retired: &[Supervised]) -> Result<()> {
        self.keep(|before| {
            Ok(before
                .iter()
                .filter(|supervised| !retired.contains(supervised))
                .cloned()
                .collect())
        })
    }
}

impl<V: Vault> Remembered for Processes<V> {
    fn kept(&self) -> Result<Vec<Supervised>> {
        Processes::kept(self)
    }

    fn add(&self, supervised: Supervised) -> Result<()> {
        Processes::add(self, supervised)
    }

    fn retire(&self, retired: &[Supervised]) -> Result<()> {
        Processes::retire(self, retired)
    }
}

/// How long recovery at a start waits for a supervisor an earlier daemon
/// left to end the work it knew, its lifeline closed with that daemon.
pub const LEFT_ENDING: Duration = Duration::from_secs(5);

/// Recover what earlier daemons left in `kept`: retire every supervisor
/// shown to be done with, by one rule, and say whether anything is left
/// that is not. It only reads processes and never signals one: a PID read
/// then signalled could be another process by then.
///
/// - one a keeper of this daemon `owned` is its keeper's, and skipped;
/// - one from another boot ended with that boot;
/// - one gone that left its proof of `DONE` or `ENDED` in `proofs` ended
///   everything it knew;
/// - one still `running`, waited for up to `wait`, or gone with no such
///   proof, is not known to be done with, and stays.
///
/// Only with `retiring` (under a saved Stop) is anything forgotten.
/// Whether everything is done with: false when the record cannot be trusted,
/// something stays, or forgetting failed.
pub fn recover(
    kept: &dyn Remembered,
    proofs: &Path,
    owned: impl Fn(&Instance) -> bool,
    running: impl Fn(&Instance) -> bool,
    wait: Duration,
    retiring: bool,
) -> bool {
    let Ok(work) = kept.kept() else {
        return false;
    };
    let Ok(boot) = atlas_core::process::boot() else {
        return false;
    };
    let left: Vec<Supervised> = work
        .into_iter()
        .filter(|supervised| !owned(&supervised.supervisor))
        .collect();
    let until = std::time::Instant::now() + wait;
    while left
        .iter()
        .any(|supervised| supervised.boot == boot && running(&supervised.supervisor))
        && std::time::Instant::now() < until
    {
        std::thread::sleep(Duration::from_millis(100));
    }
    let (done, staying): (Vec<Supervised>, Vec<Supervised>) = left
        .into_iter()
        .partition(|supervised| done_with(supervised, &boot, proofs, &running));
    if !retiring {
        return done.is_empty() && staying.is_empty();
    }
    if !done.is_empty() {
        if kept.retire(&done).is_err() {
            return false;
        }
        for supervised in &done {
            let _ = proof::forget(proofs, &supervised.supervisor);
        }
    }
    staying.is_empty()
}

/// How many supervisors in `kept` that no keeper of this daemon `owned` are
/// not known to be done with, by [`recover`]'s rule, without waiting or
/// forgetting any: none when the record or this boot cannot be told.
pub(crate) fn unresolved(
    kept: &dyn Remembered,
    proofs: &Path,
    owned: impl Fn(&Instance) -> bool,
    running: impl Fn(&Instance) -> bool,
) -> Option<usize> {
    let work = kept.kept().ok()?;
    let boot = atlas_core::process::boot().ok()?;
    Some(
        work.iter()
            .filter(|supervised| !owned(&supervised.supervisor))
            .filter(|supervised| !done_with(supervised, &boot, proofs, &running))
            .count(),
    )
}

/// Whether `supervised` is done with: from another boot than `boot`, or
/// gone with its proof of `DONE` or `ENDED` left in `proofs`.
pub(crate) fn done_with(
    supervised: &Supervised,
    boot: &str,
    proofs: &Path,
    running: impl Fn(&Instance) -> bool,
) -> bool {
    supervised.boot != boot
        || (!running(&supervised.supervisor)
            && proof::left(proofs, &supervised.supervisor)
                .is_some_and(|code| code == DONE || code == ENDED))
}

/// Keep what changed beside `path`, its name included, for good.
fn sync_folder(path: &Path) -> Result<()> {
    record_file::sync_folder(path)
        .map_err(|error| Error::new(Code::Io, "The supervisors cannot be kept.").with_source(error))
}

fn unknown() -> Error {
    Error::new(
        Code::Stopped,
        "What earlier work was left on this desktop cannot be told, so agents are stopped.",
    )
}

#[cfg(test)]
#[path = "processes_tests.rs"]
mod tests;
