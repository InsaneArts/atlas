//! The person's Stop, as a machine guest keeps it on its own disk: saved
//! before its work is ended, kept across the daemon's restarts and the
//! guest's, and lifted only by the person's Resume, which only the station
//! sends. The file is vouched for by its commitment in the Keychain
//! ([`super::commitment`]), so a Stop deleted or rolled back by a guest
//! command reads as unknown, never as no Stop: unknown holds as Stop does.
use super::{
    commitment::{Commitment, Judged, Revision},
    record_file::{self, number},
    vault::Vault,
};
use atlas_core::{
    digest::bytes_sha256,
    error::{Code, Error, Result},
    files::write_atomic,
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
};

/// The store's name among the guest's recovery stores.
pub const STORE: &str = "stop";

/// The longest the record may be.
const LIMIT: u64 = 4096;

const VERSION: u32 = 1;

/// Whether agents are stopped on this desktop.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum Kept {
    /// No Stop: agents may act.
    Clear,
    /// Stopped, and why: the person's Stop, or the station lost.
    Stopped { why: Why },
}

/// Why agents were stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Why {
    /// The person stopped agents, through the station.
    Person,
    /// The station was not heard from in time.
    Lapsed,
    /// The control connection was lost.
    ControlLost,
    /// The desktop's pairing was revoked.
    Revoked,
    /// A daemon started after one that may have left work running.
    Restarted,
    /// The daemon restarted to take Screen Recording, which only a new
    /// process gets: saved before any agent is admitted again, so the
    /// person, at the VM to grant it, resumes agents.
    Recording,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u32,
    number: u64,
    kept: Kept,
}

/// The guest's saved Stop in `path`, vouched for by `vault`.
pub struct Stop<V> {
    path: PathBuf,
    commitment: Commitment<V>,
    writing: Mutex<()>,
}

impl<V: Vault> Stop<V> {
    pub fn new(path: impl Into<PathBuf>, vault: V) -> Self {
        Self {
            path: path.into(),
            commitment: Commitment::new(vault, STORE),
            writing: Mutex::new(()),
        }
    }

    /// The record on disk and its number, once its commitment vouches for
    /// it; none when nothing was ever saved.
    fn trusted(&self) -> Result<(u64, Kept)> {
        let found = record_file::read_regular(&self.path, LIMIT).map_err(|_| unknown())?;
        let revision = found.as_ref().map(|bytes| Revision {
            number: number(bytes),
            digest: bytes_sha256(bytes),
        });
        let judged = self.commitment.judge(revision.as_ref())?;
        if judged == Judged::Unknown {
            return Err(unknown());
        }
        // A Stop committed but never written: it may have been told, so
        // it is never given up into what came before. Unknown holds.
        if judged == Judged::Current && self.commitment.state()?.1 {
            return Err(unknown());
        }
        let trusted = match found {
            Some(bytes) => {
                let record: Record = serde_json::from_slice(&bytes).map_err(|_| unknown())?;
                if record.version != VERSION {
                    return Err(unknown());
                }
                (record.number, record.kept)
            }
            None => (0, Kept::Clear),
        };
        match judged {
            Judged::Next => {
                sync_folder(&self.path)?;
                self.commitment.settle()?;
            }
            Judged::Current | Judged::Unknown => self.commitment.abandon()?,
        }
        Ok(trusted)
    }

    /// Whether agents are stopped here.
    ///
    /// # Errors
    ///
    /// Fails when the Stop cannot be trusted: it is then unknown, which
    /// holds as Stop does.
    pub fn kept(&self) -> Result<Kept> {
        let _one = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        self.trusted().map(|(_, kept)| kept)
    }

    /// Keep what `next` makes of the state kept now, judged and written
    /// under one lock: committed first, then written and kept for good, then
    /// made current. None, or the same state again, changes nothing.
    ///
    /// # Errors
    ///
    /// Fails when the Stop cannot be trusted or kept.
    fn keep(&self, next: impl FnOnce(&Kept) -> Option<Kept>) -> Result<()> {
        let _one = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        let (number, before) = self.trusted()?;
        let Some(kept) = next(&before).filter(|kept| *kept != before) else {
            return Ok(());
        };
        let number = number
            .checked_add(1)
            .ok_or_else(|| Error::new(Code::State, "The Stop cannot be kept again."))?;
        let bytes = serde_json::to_vec(&Record {
            version: VERSION,
            number,
            kept,
        })?;
        self.commitment.begin(Revision {
            number,
            digest: bytes_sha256(&bytes),
        })?;
        write_atomic(&self.path, &bytes, 0o600)?;
        sync_folder(&self.path)?;
        self.commitment.settle()
    }

    /// Save a Stop for `why`, before anything is ended for it. A Stop saved
    /// already stays, with its first reason.
    ///
    /// # Errors
    ///
    /// Fails when it cannot be saved.
    pub fn stop(&self, why: Why) -> Result<()> {
        match self.keep(|before| match before {
            Kept::Stopped { .. } => None,
            Kept::Clear => Some(Kept::Stopped { why }),
        }) {
            // Unknown: a Stop is written over whatever is there, so a Stop
            // never stays unknown for good.
            Err(error) if error.code == Code::Stopped => self.overwrite(why),
            kept => kept,
        }
    }

    /// Write a Stop for `why` over a record that cannot be trusted: the next
    /// revision after the commitment's, in place of any pending.
    fn overwrite(&self, why: Why) -> Result<()> {
        let _one = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        let (number, _) = self.commitment.state()?;
        let number = number
            .checked_add(1)
            .ok_or_else(|| Error::new(Code::State, "The Stop cannot be kept again."))?;
        let bytes = serde_json::to_vec(&Record {
            version: VERSION,
            number,
            kept: Kept::Stopped { why },
        })?;
        // In place of what is pending: never trusting what came before.
        self.commitment.begin_over(Revision {
            number,
            digest: bytes_sha256(&bytes),
        })?;
        write_atomic(&self.path, &bytes, 0o600)?;
        sync_folder(&self.path)?;
        self.commitment.settle()
    }

    /// The person's Resume, from the station: agents may act again.
    ///
    /// # Errors
    ///
    /// Refuses while the Stop is unknown: only a known Stop is lifted.
    pub fn resume(&self) -> Result<()> {
        self.keep(|_| Some(Kept::Clear))
    }
}

/// Keep what changed beside `path`, its name included, for good.
fn sync_folder(path: &Path) -> Result<()> {
    record_file::sync_folder(path)
        .map_err(|error| Error::new(Code::Io, "The Stop cannot be kept.").with_source(error))
}

fn unknown() -> Error {
    Error::new(
        Code::Stopped,
        "Whether agents are stopped on this desktop cannot be told, so they are.",
    )
}

#[cfg(test)]
#[path = "stop_tests.rs"]
mod tests;
