//! What vouches for a recovery store kept in files: its outcomes, its Stop,
//! its process list. A guest command can delete, roll back or forge any
//! file of the account's, and a record's own check cannot show that a
//! record was left out. So each store keeps a manifest of every record,
//! and its revision and digest are committed here, in the Keychain:
//!
//! 1. the next revision is committed before any file of it is written;
//! 2. the files are written;
//! 3. the next revision is made current.
//!
//! Each store's commitment is created when the guest pairs, so a missing
//! one is never taken for a store that was never written. What is found on
//! disk is then the current revision, the next one being written, or
//! unknown: missing, emptied, older or forged.
use super::vault::{Vault, damaged};
use atlas_core::error::{Code, Error, Result};
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, PoisonError};

const VERSION: u32 = 1;

/// The recovery stores a guest keeps.
pub const STORES: [&str; 4] = ["outcomes", "stop", "processes", "updates"];

/// One change to a commitment at a time in this process, whatever adapter
/// makes it.
static CHANGING: Mutex<()> = Mutex::new(());

/// One revision of a store's manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revision {
    pub number: u64,
    /// The manifest's SHA-256, in lowercase hexadecimal; empty only for the
    /// empty store, revision zero.
    pub digest: String,
}

impl Revision {
    /// The store with nothing written yet.
    fn empty() -> Self {
        Self {
            number: 0,
            digest: String::new(),
        }
    }

    fn valid(&self) -> bool {
        if self.number == 0 {
            return self.digest.is_empty();
        }
        self.digest.len() == 64
            && self
                .digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Kept {
    version: u32,
    current: Revision,
    next: Option<Revision>,
}

impl Kept {
    fn valid(&self) -> bool {
        self.version == VERSION
            && self.current.valid()
            && self.next.as_ref().is_none_or(|next| {
                next.valid() && self.current.number.checked_add(1) == Some(next.number)
            })
    }
}

/// How a store found on disk stands against its commitment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Judged {
    /// It is the current revision.
    Current,
    /// It is the next revision, committed but not made current.
    Next,
    /// Anything else: nothing in it can be trusted, and it is never
    /// started again empty.
    Unknown,
}

/// Create the commitment of the empty store `store` in `vault`, unless it
/// has one.
///
/// # Errors
///
/// Fails when the commitment cannot be read or kept.
pub fn create(vault: &impl Vault, store: &str) -> Result<()> {
    let commitment = Commitment {
        vault,
        name: name(store),
    };
    let _one = CHANGING.lock().unwrap_or_else(PoisonError::into_inner);
    if commitment.kept()?.is_some() {
        return Ok(());
    }
    commitment.keep(Revision::empty(), None)
}

fn name(store: &str) -> String {
    format!("store {store}")
}

/// The commitment of the store `name` in `vault`.
#[derive(Debug)]
pub struct Commitment<V> {
    vault: V,
    name: String,
}

impl<V: Vault> Commitment<V> {
    pub fn new(vault: V, store: &str) -> Self {
        Self {
            vault,
            name: name(store),
        }
    }

    fn kept(&self) -> Result<Option<Kept>> {
        let Some(bytes) = self.vault.read(&self.name)? else {
            return Ok(None);
        };
        let kept: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| damaged(&self.name))?;
        // Every member is there: a pending revision lost is damage, never none.
        if kept.get("next").is_none() {
            return Err(damaged(&self.name));
        }
        let kept: Kept = serde_json::from_value(kept).map_err(|_| damaged(&self.name))?;
        if !kept.valid() {
            return Err(damaged(&self.name));
        }
        Ok(Some(kept))
    }

    /// The commitment, which must exist once the guest paired.
    fn required(&self) -> Result<Kept> {
        self.kept()?.ok_or_else(|| damaged(&self.name))
    }

    fn keep(&self, current: Revision, next: Option<Revision>) -> Result<()> {
        let kept = Kept {
            version: VERSION,
            current,
            next,
        };
        if !kept.valid() {
            return Err(out_of_turn());
        }
        self.vault.write(&self.name, &serde_json::to_vec(&kept)?)
    }

    /// Commit to `next` before any file of it is written. It must follow
    /// the current revision; the same next again changes nothing.
    ///
    /// # Errors
    ///
    /// Refuses a revision out of turn, or while another next is pending:
    /// recovery settles or abandons that one first. Fails when there is no
    /// commitment to change.
    pub fn begin(&self, next: Revision) -> Result<()> {
        let _one = CHANGING.lock().unwrap_or_else(PoisonError::into_inner);
        let kept = self.required()?;
        match &kept.next {
            Some(pending) if *pending == next => Ok(()),
            Some(_) => Err(out_of_turn()),
            None if kept.current.number.checked_add(1) == Some(next.number) => {
                self.keep(kept.current, Some(next))
            }
            None => Err(out_of_turn()),
        }
    }

    /// Commit to `next` in place of whatever is pending, with no moment
    /// between in which the current revision stands alone: what recovery
    /// judges is the current one, this next one, or unknown.
    ///
    /// # Errors
    ///
    /// Refuses a revision that does not follow the current one.
    pub fn begin_over(&self, next: Revision) -> Result<()> {
        let _one = CHANGING.lock().unwrap_or_else(PoisonError::into_inner);
        let kept = self.required()?;
        if kept.current.number.checked_add(1) != Some(next.number) {
            return Err(out_of_turn());
        }
        self.keep(kept.current, Some(next))
    }

    /// Make the pending next revision current, once all of it is written.
    ///
    /// # Errors
    ///
    /// Fails when none is pending, or it cannot be kept.
    pub fn settle(&self) -> Result<()> {
        let _one = CHANGING.lock().unwrap_or_else(PoisonError::into_inner);
        let next = self.required()?.next.ok_or_else(out_of_turn)?;
        self.keep(next, None)
    }

    /// Give up the pending next revision: recovery found the current one
    /// still on disk.
    ///
    /// # Errors
    ///
    /// Fails when it cannot be kept.
    pub fn abandon(&self) -> Result<()> {
        let _one = CHANGING.lock().unwrap_or_else(PoisonError::into_inner);
        let kept = self.required()?;
        if kept.next.is_none() {
            return Ok(());
        }
        self.keep(kept.current, None)
    }

    /// The current revision's number, and whether a next one is pending.
    ///
    /// # Errors
    ///
    /// Fails when there is no commitment, or it cannot be read.
    pub fn state(&self) -> Result<(u64, bool)> {
        let kept = self.required()?;
        Ok((kept.current.number, kept.next.is_some()))
    }

    /// How `found`, the manifest on disk if there is one, stands.
    ///
    /// # Errors
    ///
    /// Fails when the commitment cannot be read.
    pub fn judge(&self, found: Option<&Revision>) -> Result<Judged> {
        let Some(kept) = self.kept()? else {
            return Ok(Judged::Unknown);
        };
        let empty = kept.current == Revision::empty();
        Ok(match found {
            Some(found) if *found == kept.current => Judged::Current,
            Some(found) if kept.next.as_ref() == Some(found) => Judged::Next,
            None if empty => Judged::Current,
            Some(_) | None => Judged::Unknown,
        })
    }
}

fn out_of_turn() -> Error {
    Error::new(
        Code::State,
        "A store's revision came out of turn with its commitment.",
    )
}

#[cfg(test)]
#[path = "commitment_tests.rs"]
mod tests;
