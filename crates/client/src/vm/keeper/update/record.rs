//! The record of an update of a VM's guest to its keeper's build, kept in
//! the VM's station folder: the build it brings the guest to, and where it
//! stands. Each change it asks is named here, durably, before it is
//! claimed, and each answer is kept here, durably, before its claim is
//! cleared; so a keeper that ends at any moment leaves the next one every
//! change to reconcile and everything the guest already said.
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;

/// Its name in the station folder.
pub(super) const RECORD: &str = "guest-update.json";

/// The longest record read: an outcome is the guest's final reply, which
/// is short.
const LONGEST: u64 = 64 * 1024;

/// One update of the guest, as kept.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Record {
    /// The build it brings the guest to: the keeper's that began it.
    pub(super) target: Target,
    pub(super) phase: Phase,
    /// The copies updates of other builds staged and left, each removed
    /// once agents resume, kept here until its removal is answered.
    #[serde(default)]
    pub(super) earlier: Vec<Leftover>,
}

/// A copy an earlier update staged, by its stage, and its removal once one
/// was named.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Leftover {
    pub(super) stage: Token,
    pub(super) removal: Option<Removal>,
}

/// A copy's removal: the folder node it is asked for, and its change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Removal {
    pub(super) node: String,
    pub(super) remove: Token,
}

/// A build of Atlas, by its executable's SHA-256 and its version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Target {
    pub(super) build: String,
    pub(super) version: String,
}

/// Where the guest's copy of the bundle was placed, and the manifest digest
/// it was staged with, which the update names as the one to install.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Staged {
    pub(super) path: String,
    pub(super) digest: String,
}

/// Where an update stands. Each phase names every change it has asked so
/// far; the one it waits on is [`Phase::open`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Phase {
    /// Kept before anything is claimed or asked.
    Intended,
    /// The stage of the keeper's own bundle, claimed and perhaps asked,
    /// bound to the manifest digest of the snapshot it is said with once
    /// one was taken: it is never said under its name with another.
    Staging {
        stage: Token,
        #[serde(default)]
        digest: Option<String>,
    },
    /// The guest placed the bundle; nothing more asked yet.
    Staged { stage: Token, staged: Staged },
    /// The update, claimed and perhaps asked.
    Asked {
        stage: Token,
        staged: Staged,
        update: Token,
    },
    /// What the update came to, or why it was never asked; the staged copy
    /// stays until agents resume.
    Settled {
        stage: Token,
        update: Option<Token>,
        outcome: Value,
    },
    /// The staged copy's removal, claimed and perhaps asked, of the folder
    /// `node` names.
    Cleaning {
        stage: Token,
        update: Option<Token>,
        outcome: Value,
        node: String,
        remove: Token,
    },
    /// Finished: what it came to.
    Done { issued: Vec<Token>, outcome: Value },
}

impl Phase {
    /// Every change it has asked, or may have.
    pub(super) fn issued(&self) -> Vec<Token> {
        match self {
            Self::Intended => Vec::new(),
            Self::Staging { stage, .. } | Self::Staged { stage, .. } => vec![stage.clone()],
            Self::Asked { stage, update, .. } => vec![stage.clone(), update.clone()],
            Self::Settled { stage, update, .. } => std::iter::once(stage.clone())
                .chain(update.clone())
                .collect(),
            Self::Cleaning {
                stage,
                update,
                remove,
                ..
            } => std::iter::once(stage.clone())
                .chain(update.clone())
                .chain(std::iter::once(remove.clone()))
                .collect(),
            Self::Done { issued, .. } => issued.clone(),
        }
    }

    /// The change it waits on the guest's answer for; none between them.
    pub(super) fn open(&self) -> Option<&Token> {
        match self {
            Self::Staging { stage, .. } => Some(stage),
            Self::Asked { update, .. } => Some(update),
            Self::Cleaning { remove, .. } => Some(remove),
            Self::Intended | Self::Staged { .. } | Self::Settled { .. } | Self::Done { .. } => None,
        }
    }

    /// Its name, as it is kept and as a status says it.
    const fn name(&self) -> &'static str {
        match self {
            Self::Intended => "intended",
            Self::Staging { .. } => "staging",
            Self::Staged { .. } => "staged",
            Self::Asked { .. } => "asked",
            Self::Settled { .. } => "settled",
            Self::Cleaning { .. } => "cleaning",
            Self::Done { .. } => "done",
        }
    }

    /// What it came to, once it is settled.
    pub(super) fn outcome(&self) -> Option<&Value> {
        match self {
            Self::Settled { outcome, .. }
            | Self::Cleaning { outcome, .. }
            | Self::Done { outcome, .. } => Some(outcome),
            Self::Intended | Self::Staging { .. } | Self::Staged { .. } | Self::Asked { .. } => {
                None
            }
        }
    }
}

impl Record {
    /// Every change it names, its earlier updates' left copies included:
    /// those only its owner settles.
    pub(super) fn issued(&self) -> Vec<Token> {
        let earlier = self.earlier.iter().flat_map(|left| {
            std::iter::once(left.stage.clone())
                .chain(left.removal.as_ref().map(|removal| removal.remove.clone()))
        });
        self.phase.issued().into_iter().chain(earlier).collect()
    }

    /// What it leaves for the update begun in its place to remove: its
    /// copy, once it staged one and no longer waits on the guest, with its
    /// removal once one was named, and what it was left itself.
    pub(super) fn left(self) -> Vec<Leftover> {
        let own = match self.phase {
            Phase::Settled { stage, .. } => Some(Leftover {
                stage,
                removal: None,
            }),
            Phase::Cleaning {
                stage,
                node,
                remove,
                ..
            } => Some(Leftover {
                stage,
                removal: Some(Removal { node, remove }),
            }),
            Phase::Intended
            | Phase::Staging { .. }
            | Phase::Staged { .. }
            | Phase::Asked { .. }
            | Phase::Done { .. } => None,
        };
        self.earlier.into_iter().chain(own).collect()
    }

    /// What a status says of it: its phase, the build it brings the guest
    /// to, what it came to once settled, and each copy an earlier update
    /// left, by its stage and its removal once one is named. No node and no
    /// staged path: only what the person reads. A settlement not known to
    /// last, `lasting` false, is said as `settling`, its outcome unsaid: it
    /// may yet be lost.
    pub(super) fn said(&self, lasting: bool) -> Value {
        let earlier: Vec<Value> = self
            .earlier
            .iter()
            .map(|left| {
                json!({"stage": left.stage,
                    "removal": left.removal.as_ref().map(|removal| &removal.remove)})
            })
            .collect();
        let outcome = self.phase.outcome().filter(|_| lasting);
        let phase = match (self.phase.outcome(), outcome) {
            (Some(_), None) => "settling",
            _ => self.phase.name(),
        };
        let mut said = json!({"phase": phase, "target": self.target, "earlier": earlier});
        if let Some(outcome) = outcome {
            said["outcome"] = outcome.clone();
        }
        said
    }

    /// The update kept in the station folder `folder`, if any.
    ///
    /// # Errors
    ///
    /// Fails when one is there but cannot be read: never taken for none.
    pub(super) fn kept(folder: &Path) -> Result<Option<Self>> {
        let path = folder.join(RECORD);
        if std::fs::symlink_metadata(&path)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
        {
            return Ok(None);
        }
        let damaged = || {
            Error::new(
                Code::State,
                "The record of an update of the VM's Atlas cannot be read.",
            )
        };
        let bytes = atlas_core::files::read_own(&path, LONGEST).ok_or_else(damaged)?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| damaged())
    }

    /// Keep it in `folder`, its folder synced, before anything it names is
    /// claimed or cleared.
    ///
    /// # Errors
    ///
    /// Fails when it cannot be written, or its folder synced: it may then be
    /// in place, or not, and nothing that relies on it is done.
    pub(super) fn keep(&self, folder: &Path) -> Result<()> {
        #[cfg(test)]
        writes::wait(folder);
        atlas_core::files::write_durable(&folder.join(RECORD), &serde_json::to_vec(self)?, 0o600)
    }

    /// Forget it from `folder`, its folder synced.
    ///
    /// # Errors
    ///
    /// Fails when it cannot be removed, or its folder synced.
    pub(super) fn forget(folder: &Path) -> Result<()> {
        atlas_core::files::remove_durable(&folder.join(RECORD))
    }
}

/// Where a test holds a keeper's record writes, as a disk slow to finish
/// one would: each waits, off the runtime, until it is let go.
#[cfg(test)]
pub(super) mod writes {
    use std::{
        collections::BTreeMap,
        path::{Path, PathBuf},
        sync::{Condvar, Mutex, PoisonError},
    };

    /// Each folder whose writes are held: how many more go first, and how
    /// many wait now.
    static HELD: Mutex<BTreeMap<PathBuf, (usize, usize)>> = Mutex::new(BTreeMap::new());
    static LET_GO: Condvar = Condvar::new();

    /// Hold every write of a record in the station folder `folder` after
    /// the next `first`.
    pub(in crate::vm::keeper) fn hold_after(folder: &Path, first: usize) {
        HELD.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(folder.to_owned(), (first, 0));
    }

    /// Let the writes in `folder` go.
    pub(in crate::vm::keeper) fn release(folder: &Path) {
        HELD.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(folder);
        LET_GO.notify_all();
    }

    /// Whether a write in `folder` waits now.
    pub(in crate::vm::keeper) fn waiting(folder: &Path) -> bool {
        HELD.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(folder)
            .is_some_and(|(_, waiting)| *waiting > 0)
    }

    pub(super) fn wait(folder: &Path) {
        let mut held = HELD.lock().unwrap_or_else(PoisonError::into_inner);
        match held.get_mut(folder) {
            None => return,
            Some((first, _)) if *first > 0 => {
                *first -= 1;
                return;
            }
            Some((_, waiting)) => *waiting += 1,
        }
        while held.contains_key(folder) {
            held = LET_GO.wait(held).unwrap_or_else(PoisonError::into_inner);
        }
    }
}

#[cfg(test)]
#[path = "record_tests.rs"]
mod tests;
