//! The account's uninstall barrier, `.uninstalling`, and its lock, the
//! first in the station's order. Uninstall takes the lock alone and
//! records its change, with the VMs the person allowed it to remove, before
//! it looks at what to remove; every add and removal shares the lock and
//! refuses while the record says uninstall's VM phase is under way. Once
//! that phase commits, or the person calls the uninstall off, the barrier
//! blocks nothing, so a shared cleanup cut short later never leaves a
//! reinstall or a repair refused. Beside it, the live lock in Atlas's caches,
//! which the shared cleanup never deletes: every add and removal shares it,
//! and uninstall holds it alone from its VM phase until it ends, so none
//! starts while what Atlas shares goes; it holds nothing once the process
//! is gone.
use super::{Lifecycle, keep, read};
use atlas_core::{
    desktop::DesktopId,
    error::{Code, Error, Result},
    files::{Lock, LockFile, Token},
};
use serde::{Deserialize, Serialize};
use std::{os::unix::fs::DirBuilderExt, path::PathBuf, time::Duration};

/// The barrier's record, in the lifecycle folder.
const RECORD: &str = ".uninstalling";
/// Its lock, beside it.
const LOCK: &str = ".uninstalling.lock";
/// The live lock, in Atlas's caches beside the controller's locks: an
/// uninstall holds it alone from its VM phase to its end, so no add or
/// removal starts while it removes what Atlas shares, and nothing of it
/// outlives the process.
const LIVE: &str = "vm-lifecycle.lock";
/// Atlas's caches, in the account's home.
const CACHES: &str = "Library/Caches/Atlas";
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
    /// Called off before every VM was gone: the VMs removed stay removed,
    /// the others as they were, and the barrier no longer blocks anything.
    Cancelled,
}

/// The barrier as kept: the uninstall's change, its phase, and the VMs the
/// person allowed it to remove.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Barrier {
    pub change: Token,
    pub phase: Phase,
    #[serde(default)]
    pub vms: Vec<DesktopId>,
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

/// An add or removal let past the barrier: both locks shared, held for its
/// whole life.
#[derive(Debug)]
pub struct Admitted {
    _lock: LockFile,
    _live: LockFile,
    home: PathBuf,
}

impl sealed::Sealed for Admitted {
    fn home(&self) -> &std::path::Path {
        &self.home
    }
}
impl Passed for Admitted {}

/// Uninstall's own hold of the barrier: both locks held alone, its change
/// and the VMs it may remove recorded.
#[derive(Debug)]
pub struct Uninstalling {
    _lock: LockFile,
    live: LockFile,
    change: Token,
    vms: Vec<DesktopId>,
    record: PathBuf,
    home: PathBuf,
}

impl sealed::Sealed for Uninstalling {
    fn home(&self) -> &std::path::Path {
        &self.home
    }
}
impl Passed for Uninstalling {}

/// An uninstall whose VM phase committed, still keeping every add and
/// removal out while it removes what Atlas shares: the live lock, held
/// alone until this is dropped or the process ends. Its file is among
/// Atlas's caches, whose locks the shared cleanup never deletes, so the
/// lock it holds is the one every later add or removal opens.
#[derive(Debug)]
pub struct Closed {
    _live: LockFile,
}

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

    /// The VMs the person allowed it to remove.
    #[must_use]
    pub(in crate::vm) fn vms(&self) -> &[DesktopId] {
        &self.vms
    }

    /// Note every VM gone: from now the barrier's record blocks nothing,
    /// and every add and removal stays out only while what this gives is
    /// held.
    ///
    /// # Errors
    ///
    /// Fails when the record cannot be written; the VM phase then stays
    /// under way.
    pub fn commit(self) -> Result<Closed> {
        keep(
            &self.record,
            &Barrier {
                change: self.change,
                phase: Phase::Committed,
                vms: self.vms,
            },
        )?;
        Ok(Closed { _live: self.live })
    }

    /// Call the uninstall off, nothing more removed: from now the barrier
    /// blocks nothing, and another uninstall is the person's to allow anew.
    ///
    /// # Errors
    ///
    /// Fails when the record cannot be written; the VM phase then stays
    /// under way.
    pub(in crate::vm) fn call_off(self) -> Result<()> {
        keep(
            &self.record,
            &Barrier {
                change: self.change,
                phase: Phase::Cancelled,
                vms: self.vms,
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

    /// The live lock, in Atlas's caches, taken `lock` within the patience;
    /// none while it is held otherwise.
    fn live_lock(&self, lock: Lock) -> Result<Option<LockFile>> {
        let caches = self.home.join(CACHES);
        // Made private when missing; one there already is the controller's,
        // which keeps its locks in it too, and need only be this account's
        // own real folder.
        if !caches.exists() {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&caches)
                .map_err(|error| Error::writing(&caches, error))?;
        }
        if !atlas_core::files::own_folder(&caches) {
            return Err(Error::new(
                Code::State,
                format!("{} must be a folder you own, not a link.", caches.display()),
            ));
        }
        let file = atlas_core::files::open_lock(&caches.join(LIVE))?;
        Ok(atlas_core::files::lock_within(&file, lock, PATIENCE)
            .ok()
            .map(|()| file))
    }

    /// Both of uninstall's locks held alone.
    fn exclusive(&self) -> Result<(LockFile, LockFile)> {
        let Some(lock) = self.barrier_lock(Lock::Exclusive)? else {
            return Err(busy());
        };
        let Some(live) = self.live_lock(Lock::Exclusive)? else {
            return Err(busy());
        };
        Ok((lock, live))
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

    /// Let an add or removal past the barrier, holding both its locks
    /// shared for as long as the proof lives.
    ///
    /// # Errors
    ///
    /// Refuses while an uninstall holds either or its VM phase is under
    /// way; fails when the barrier cannot be read.
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
        let Some(live) = self.live_lock(Lock::Shared)? else {
            return Err(uninstalling());
        };
        Ok(Admitted {
            _lock: lock,
            _live: live,
            home: self.home.clone(),
        })
    }

    /// Hold the barrier alone for a new uninstall, `change`, allowed to
    /// remove the VMs `vms`, recorded before anything else is looked at.
    ///
    /// # Errors
    ///
    /// Refuses while an add, a removal or another uninstall holds the
    /// barrier, and while an uninstall's VM phase is under way; fails when
    /// the record cannot be read or written.
    pub fn uninstalling(&self, change: &Token, vms: &[DesktopId]) -> Result<Uninstalling> {
        let (lock, live) = self.exclusive()?;
        if self
            .barrier()?
            .is_some_and(|barrier| barrier.phase == Phase::Vms)
        {
            return Err(Error::new(
                Code::Busy,
                "Another uninstall is under way; run atlas uninstall again to finish it.",
            ));
        }
        let record = self.folder()?.join(RECORD);
        keep(
            &record,
            &Barrier {
                change: change.clone(),
                phase: Phase::Vms,
                vms: vms.to_vec(),
            },
        )?;
        Ok(Uninstalling {
            _lock: lock,
            live,
            change: change.clone(),
            vms: vms.to_vec(),
            record,
            home: self.home.clone(),
        })
    }

    /// Hold the barrier alone to go on with the uninstall `change`, as the
    /// VMs it recorded, only while its VM phase is still under way:
    /// nothing is written.
    ///
    /// # Errors
    ///
    /// Refuses while an add, a removal or another uninstall holds the
    /// barrier, and once that uninstall was called off, committed or
    /// replaced; fails when the record cannot be read.
    pub(in crate::vm) fn resuming(&self, change: &Token) -> Result<Uninstalling> {
        let (lock, live) = self.exclusive()?;
        match self.barrier()? {
            Some(barrier) if barrier.phase == Phase::Vms && barrier.change == *change => {
                Ok(Uninstalling {
                    _lock: lock,
                    live,
                    change: barrier.change,
                    vms: barrier.vms,
                    record: self.folder()?.join(RECORD),
                    home: self.home.clone(),
                })
            }
            _ => Err(Error::new(
                Code::State,
                "The uninstall was called off or ended meanwhile, so nothing more is removed. \
                 Run atlas uninstall again to start another.",
            )),
        }
    }

    /// Call off the uninstall `change` while its VM phase is under way,
    /// under the barrier held alone: whether it was under way.
    ///
    /// # Errors
    ///
    /// Refuses while an add, a removal or an uninstall holds the barrier;
    /// fails when the record cannot be read or written.
    pub(in crate::vm) fn call_off(&self, change: &Token) -> Result<bool> {
        let Some(_lock) = self.barrier_lock(Lock::Exclusive)? else {
            return Err(busy());
        };
        match self.barrier()? {
            Some(barrier) if barrier.phase == Phase::Vms && barrier.change == *change => {
                keep(
                    &self.folder()?.join(RECORD),
                    &Barrier {
                        phase: Phase::Cancelled,
                        ..barrier
                    },
                )?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

fn busy() -> Error {
    Error::new(
        Code::Busy,
        "A VM is being added, removed or uninstalled; try again once it is done.",
    )
}

fn uninstalling() -> Error {
    Error::new(
        Code::Busy,
        "Atlas is being uninstalled, so no VM is added or removed. Run atlas uninstall again to \
         finish it, or atlas uninstall --cancel to call it off.",
    )
}

#[cfg(test)]
#[path = "barrier_tests.rs"]
mod tests;
