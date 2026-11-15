//! Updates of the guest's own Atlas, as the guest daemon keeps them: each
//! recorded before the root installer is asked anything, kept across the
//! daemon's restarts (the update's own among them) and read by every build,
//! and settled only by what the installer says the transaction came to.
//! The file is vouched for by its commitment in the Keychain
//! ([`super::commitment`]), so an update record deleted or rolled back by a
//! guest command reads as unknown, never as none: unknown holds updates up,
//! as an unresolved one does.
mod gate;
mod guest;
mod installer;
mod reconcile;

pub use gate::{Admission, Gate};
pub(crate) use guest::Guest;
pub(crate) use reconcile::System;
pub use reconcile::{Health, Reconciler, outcome};

/// How many published updates are kept besides the rest: the newest.
const SETTLED: usize = 8;

use super::{
    commitment::{Commitment, Judged, Revision},
    record_file::{self, number},
    vault::Vault,
};
use atlas_core::{
    digest::bytes_sha256,
    error::{Code, Error, Result},
    files::{Token, write_atomic},
    stage::Snapshot,
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
};

/// The store's name among the guest's recovery stores.
pub const STORE: &str = "updates";

/// The longest the record may be: one update's manifest, and the rest.
const LIMIT: u64 = atlas_core::stage::MOST_MANIFEST as u64 + 64 * 1024;

const VERSION: u32 = 1;

/// How far an update got.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    /// Recorded, the installer asked or about to be: not settled.
    Asked,
    /// The installer released it: the new build is Atlas here now.
    Released,
    /// Refused before anything began, or by the installer.
    Refused,
    /// Placed, then put back: the previous build is Atlas here again.
    RolledBack,
}

impl Phase {
    /// Whether it is settled for good.
    #[must_use]
    pub fn settled(self) -> bool {
        self != Self::Asked
    }
}

/// One update, as kept.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Update {
    /// The change the station asked it as.
    pub change: Token,
    /// The bundle staged in the guest, and its manifest as staged; the
    /// manifest goes once the update is settled.
    pub source: PathBuf,
    pub manifest: Option<Snapshot>,
    /// The installer's transaction, and the requests asked of it.
    pub transaction: Token,
    pub install: Token,
    pub release: Token,
    /// What input and capture were when it was asked, so the new build is
    /// released only no worse.
    pub input: Health,
    pub capture: Health,
    pub phase: Phase,
    /// What the installer said, once settled.
    pub why: Option<String>,
    /// Whether what it came to was told to its change: until it is, the
    /// update is never pruned.
    #[serde(default)]
    pub published: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u32,
    number: u64,
    updates: Vec<Update>,
}

/// The guest's updates in `path`, vouched for by `vault`.
pub struct Updates<V> {
    path: PathBuf,
    commitment: Commitment<V>,
    writing: Mutex<()>,
}

impl<V: Vault> Updates<V> {
    pub fn new(path: impl Into<PathBuf>, vault: V) -> Self {
        Self {
            path: path.into(),
            commitment: Commitment::new(vault, STORE),
            writing: Mutex::new(()),
        }
    }

    /// The record on disk and its number, once its commitment vouches for
    /// it; none kept when nothing was ever saved.
    fn trusted(&self) -> Result<(u64, Vec<Update>)> {
        let found = record_file::read_regular(&self.path, LIMIT).map_err(|_| unknown())?;
        let revision = found.as_ref().map(|bytes| Revision {
            number: number(bytes),
            digest: bytes_sha256(bytes),
        });
        let judged = self.commitment.judge(revision.as_ref())?;
        if judged == Judged::Unknown {
            return Err(unknown());
        }
        // One committed but never written may have been asked of the
        // installer: never given up into what came before.
        if judged == Judged::Current && self.commitment.state()?.1 {
            return Err(unknown());
        }
        let trusted = match found {
            Some(bytes) => {
                let record: Record = serde_json::from_slice(&bytes).map_err(|_| unknown())?;
                if record.version != VERSION {
                    return Err(unknown());
                }
                (record.number, record.updates)
            }
            None => (0, Vec::new()),
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

    /// Every update kept.
    ///
    /// # Errors
    ///
    /// Fails when the record cannot be trusted.
    pub fn all(&self) -> Result<Vec<Update>> {
        let _one = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        self.trusted().map(|(_, updates)| updates)
    }

    /// The update of `change`, when one is kept.
    ///
    /// # Errors
    ///
    /// Fails when the record cannot be trusted.
    pub fn of(&self, change: &Token) -> Result<Option<Update>> {
        Ok(self
            .all()?
            .into_iter()
            .find(|update| &update.change == change))
    }

    /// Whether any update is unresolved: true when that cannot be told.
    pub fn unresolved(&self) -> bool {
        self.all().map_or(true, |updates| {
            updates.iter().any(|update| !update.phase.settled())
        })
    }

    /// Keep what `next` makes of the updates kept now, judged and written
    /// under one lock: committed first, then written and kept for good, then
    /// made current. None changes nothing.
    ///
    /// # Errors
    ///
    /// Fails when the record cannot be trusted or kept.
    pub fn keep(&self, next: impl FnOnce(&[Update]) -> Option<Vec<Update>>) -> Result<()> {
        let _one = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        let (number, before) = self.trusted()?;
        let Some(updates) = next(&before).filter(|updates| *updates != before) else {
            return Ok(());
        };
        let number = number
            .checked_add(1)
            .ok_or_else(|| Error::new(Code::State, "The updates cannot be kept again."))?;
        let bytes = serde_json::to_vec(&Record {
            version: VERSION,
            number,
            updates,
        })?;
        // Never one it could not read back.
        if bytes.len() as u64 > LIMIT {
            return Err(Error::new(Code::Busy, "Too many updates are kept."));
        }
        self.commitment.begin(Revision {
            number,
            digest: bytes_sha256(&bytes),
        })?;
        write_atomic(&self.path, &bytes, 0o600)?;
        sync_folder(&self.path)?;
        self.commitment.settle()
    }

    /// Mark the settled update of `change` as told to its change.
    ///
    /// # Errors
    ///
    /// Fails when the record cannot be trusted or kept.
    pub fn published(&self, change: &Token) -> Result<()> {
        self.keep(|before| {
            let mut updates = before.to_vec();
            let update = updates.iter_mut().find(|update| {
                &update.change == change && update.phase.settled() && !update.published
            })?;
            update.published = true;
            Some(updates)
        })
    }

    /// Settle the update of `change` as `phase`, for `why`; its manifest
    /// goes with it. One settled already stays as it was.
    ///
    /// # Errors
    ///
    /// Fails when the record cannot be trusted or kept.
    pub fn settle(&self, change: &Token, phase: Phase, why: Option<String>) -> Result<()> {
        self.keep(|before| {
            let mut updates = before.to_vec();
            let update = updates
                .iter_mut()
                .find(|update| &update.change == change && !update.phase.settled())?;
            update.phase = phase;
            update.why = why;
            update.manifest = None;
            Some(updates)
        })
    }
}

/// `updates` with the oldest published ones gone, past the newest few:
/// every one not yet told to its change stays.
#[must_use]
pub fn pruned(mut updates: Vec<Update>) -> Vec<Update> {
    let mut published = updates.iter().filter(|update| update.published).count();
    updates.retain(|update| {
        if update.published && published > SETTLED {
            published -= 1;
            return false;
        }
        true
    });
    updates
}

/// Keep what changed beside `path`, its name included, for good.
fn sync_folder(path: &Path) -> Result<()> {
    record_file::sync_folder(path)
        .map_err(|error| Error::new(Code::Io, "The updates cannot be kept.").with_source(error))
}

fn unknown() -> Error {
    Error::new(
        Code::Uncertain,
        "This guest's updates cannot be told, so none is taken.",
    )
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;
