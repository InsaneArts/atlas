//! The account's uninstall barrier, `.uninstalling`, and its lock, the
//! first in the station's order. Uninstall takes the lock alone and
//! records its change before it looks at what to remove; every add and
//! removal shares the lock and refuses while the record says uninstall's VM
//! phase is under way. Once that phase commits the barrier blocks nothing,
//! so a shared cleanup cut short later never leaves a reinstall or a repair
//! refused.
use super::{Lifecycle, keep, read};
use atlas_core::{
    error::{Code, Error, Result},
    files::{Lock, LockFile, Token},
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};

/// The barrier's record, in the lifecycle folder.
const RECORD: &str = ".uninstalling";
/// Its lock, beside it.
const LOCK: &str = ".uninstalling.lock";
/// How long the lock is tried for before the work is refused as busy.
const PATIENCE: Duration = Duration::from_secs(2);

/// How far an uninstall got.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    /// Its VMs are being removed: no add or removal starts.
    Vms,
    /// Every VM is gone: the barrier no longer blocks anything.
    Committed,
}

/// The barrier as kept: the uninstall's change and its phase.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Barrier {
    pub change: Token,
    pub phase: Phase,
}

mod sealed {
    pub trait Sealed {
        /// The home of the account whose barrier it guards.
        fn home(&self) -> &std::path::Path;
    }
}

/// Proof that the uninstall barrier was passed, and is held while the
/// proof lives: what a VM's station-operation lock is taken under.
pub trait Passed: sealed::Sealed {}

/// An add or removal let past the barrier: the lock shared, held for its
/// whole life.
#[derive(Debug)]
pub struct Admitted {
    _lock: LockFile,
    home: PathBuf,
}

impl sealed::Sealed for Admitted {
    fn home(&self) -> &std::path::Path {
        &self.home
    }
}
impl Passed for Admitted {}

/// Uninstall's own hold of the barrier: the lock held alone, its change
/// recorded.
#[derive(Debug)]
pub struct Uninstalling {
    _lock: LockFile,
    change: Token,
    record: PathBuf,
    home: PathBuf,
}

impl sealed::Sealed for Uninstalling {
    fn home(&self) -> &std::path::Path {
        &self.home
    }
}
impl Passed for Uninstalling {}

/// Whether `passed` guards the barrier of the account whose home is
/// `home`.
pub(crate) fn guards(passed: &impl Passed, home: &std::path::Path) -> bool {
    sealed::Sealed::home(passed) == home
}

impl Uninstalling {
    /// The uninstall's change, which a cut-short one is resumed as.
    #[must_use]
    pub fn change(&self) -> &Token {
        &self.change
    }

    /// Note every VM gone: from now the barrier blocks nothing.
    ///
    /// # Errors
    ///
    /// Fails when the record cannot be written; the VM phase then stays
    /// under way.
    pub fn commit(self) -> Result<()> {
        keep(
            &self.record,
            &Barrier {
                change: self.change.clone(),
                phase: Phase::Committed,
            },
        )
    }
}

impl Lifecycle {
    fn barrier_lock(&self, lock: Lock) -> Result<Option<LockFile>> {
        let file = atlas_core::files::open_lock(&self.folder()?.join(LOCK))?;
        Ok(atlas_core::files::lock_within(&file, lock, PATIENCE)
            .ok()
            .map(|()| file))
    }

    /// The barrier as recorded; none when no uninstall began.
    ///
    /// # Errors
    ///
    /// Fails when it cannot be read: a barrier that is there is never taken
    /// for absent.
    pub fn barrier(&self) -> Result<Option<Barrier>> {
        read(&self.folder()?.join(RECORD), "The uninstall barrier")
    }

    /// Let an add or removal past the barrier, holding it shared for as
    /// long as the proof lives.
    ///
    /// # Errors
    ///
    /// Refuses while an uninstall holds it or its VM phase is under way;
    /// fails when the barrier cannot be read.
    pub fn admit(&self) -> Result<Admitted> {
        let Some(lock) = self.barrier_lock(Lock::Shared)? else {
            return Err(uninstalling());
        };
        if self
            .barrier()?
            .is_some_and(|barrier| barrier.phase == Phase::Vms)
        {
            return Err(uninstalling());
        }
        Ok(Admitted {
            _lock: lock,
            home: self.home.clone(),
        })
    }

    /// Hold the barrier alone for the uninstall `change`, recorded before
    /// anything else is looked at, or go on with that very uninstall.
    ///
    /// # Errors
    ///
    /// Refuses while an add or removal holds the barrier, and while another
    /// uninstall's VM phase is under way; fails when the record cannot be
    /// read or written.
    pub fn uninstalling(&self, change: &Token) -> Result<Uninstalling> {
        let Some(lock) = self.barrier_lock(Lock::Exclusive)? else {
            return Err(Error::new(
                Code::Busy,
                "A VM is being added or removed; try again once it is done.",
            ));
        };
        let record = self.folder()?.join(RECORD);
        match self.barrier()? {
            Some(barrier) if barrier.phase == Phase::Vms && barrier.change != *change => {
                return Err(Error::new(
                    Code::Busy,
                    "Another uninstall is under way; run atlas uninstall again to finish it.",
                ));
            }
            Some(barrier) if barrier.phase == Phase::Vms => {}
            _ => keep(
                &record,
                &Barrier {
                    change: change.clone(),
                    phase: Phase::Vms,
                },
            )?,
        }
        Ok(Uninstalling {
            _lock: lock,
            change: change.clone(),
            record,
            home: self.home.clone(),
        })
    }
}

fn uninstalling() -> Error {
    Error::new(
        Code::Busy,
        "Atlas is being uninstalled, so no VM is added or removed. Run atlas uninstall again to \
         finish it.",
    )
}

#[cfg(test)]
#[path = "barrier_tests.rs"]
mod tests;
