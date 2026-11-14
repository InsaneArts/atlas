//! The supervisors a machine guest's daemon started, kept on the guest's
//! own disk before any of their programs may start, so a daemon that starts
//! after one that was lost knows what it left. The file is vouched for by
//! its commitment in the Keychain ([`super::commitment`]): one deleted or
//! rolled back reads as unknown, never as none.
use super::{
    commitment::{Commitment, Judged, Revision},
    record_file::{self, number},
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
    fn add(&self, supervised: Supervised) -> Result<()> {
        Processes::add(self, supervised)
    }

    fn retire(&self, retired: &[Supervised]) -> Result<()> {
        Processes::retire(self, retired)
    }
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
