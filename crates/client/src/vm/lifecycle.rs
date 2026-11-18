//! What the station keeps of each VM's lifecycle, in `Station/lifecycle/`:
//! the person's requests and grants, a record per VM, its name's
//! reservation, and the account's uninstall barrier. All of it is outside
//! each VM's own folder and its station folder, which a removal deletes,
//! and in Atlas's own folder, which nothing hands to a desktop or reads for
//! an agent. Every file is this account's alone, written whole and renamed
//! into place, or made only where none is.
//!
//! The station's locks are taken in one order: the uninstall barrier
//! ([`barrier`]: its record's lock, then the live lock), then the VM's
//! station-operation lock ([`super::StationHold`]), then the VM's
//! exclusion (its controller's lock, which an update's handover holds past
//! the barrier alone, and a start when needed takes last, tried and never
//! waited for), then its keeper's. The settings lock stands apart: a settings write holds it for that one
//! write, waits for nothing under it and gives up in seconds, so it may be
//! taken under any of them without a cycle. So may the lock that admits one
//! removal at a time from a settings file, held only while that removal is
//! counted and recorded.
pub mod barrier;
pub mod grant;
mod names;
mod provenance;
pub mod record;

pub use names::Reservation;
pub use provenance::{Provenance, preflight_station_key};

use atlas_core::{
    error::{Code, Error, Result},
    files::{Lock, LockFile},
};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

/// The most of one lifecycle file read.
const LONGEST: u64 = 256 * 1024;

/// How long a removal waits to be admitted while another is being.
const ADMITTING: Duration = Duration::from_secs(5);

/// The station's lifecycle folder in one account's home.
#[derive(Clone, Debug)]
pub struct Lifecycle {
    /// The account's home, whose VMs the lifecycle is of.
    home: PathBuf,
    folder: PathBuf,
}

impl Lifecycle {
    /// This account's.
    ///
    /// # Errors
    ///
    /// Fails when this account's home cannot be found.
    pub fn own() -> Result<Self> {
        Ok(Self::under(&atlas_core::config::home()?))
    }

    /// The lifecycle folder in the home `home`.
    #[must_use]
    pub fn under(home: &Path) -> Self {
        Self {
            home: home.to_owned(),
            folder: crate::station::Station::under(home).lifecycle_path(),
        }
    }

    /// The home of the account whose lifecycle it is.
    pub(in crate::vm) fn home(&self) -> &Path {
        &self.home
    }

    /// The VMs this lifecycle is of: its account's, kept in its home.
    pub(in crate::vm) fn vms(&self) -> super::Vms {
        super::Vms::under(&self.home, nix::unistd::getuid().as_raw())
    }

    /// Whether anything was ever kept here: a read that finds nothing need
    /// make nothing.
    ///
    /// # Errors
    ///
    /// Fails when the folder cannot be looked at: only one that is not
    /// there is taken for none.
    pub(in crate::vm) fn kept_any(&self) -> Result<bool> {
        match std::fs::symlink_metadata(&self.folder) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(Error::new(
                Code::State,
                "This Mac's VM lifecycle records cannot be read.",
            )
            .with_source(error)),
        }
    }

    /// The lock that admits one removal at a time from the settings file
    /// `settings`, by its canonical path: held while a removal is counted
    /// against what the settings would have left, with every other removal
    /// already under way, and recorded as under way itself. It is taken
    /// under a VM's station-operation lock and waits for nothing under it,
    /// giving up in seconds, as the settings lock does.
    ///
    /// # Errors
    ///
    /// Refuses while another removal is being admitted that long; fails
    /// when the lock cannot be reached.
    pub(in crate::vm) fn admitting_removal(&self, settings: &Path) -> Result<LockFile> {
        let digest = Sha256::digest(settings.as_os_str().as_encoded_bytes());
        let name = format!(
            ".removing-{}.lock",
            atlas_core::name::hex(digest.get(..16).unwrap_or(&digest))
        );
        let lock = atlas_core::files::open_lock(&self.folder()?.join(name))?;
        atlas_core::files::lock_within(&lock, Lock::Exclusive, ADMITTING).map_err(|_| {
            Error::new(
                Code::Busy,
                "Another VM's removal from these settings is being admitted; try again.",
            )
        })?;
        Ok(lock)
    }

    /// The folder, made private when missing and checked private either
    /// way, as is the station's own that holds it.
    fn folder(&self) -> Result<&Path> {
        if let Some(station) = self.folder.parent() {
            atlas_core::files::private_folder(station, Code::State)?;
        }
        atlas_core::files::private_folder(&self.folder, Code::State)?;
        Ok(&self.folder)
    }

    /// The private folder `name` in it.
    fn inner(&self, name: &str) -> Result<PathBuf> {
        let folder = self.folder()?.join(name);
        atlas_core::files::private_folder(&folder, Code::State)?;
        Ok(folder)
    }
}

/// The record at `path`; none when nothing is there. `what` names it in a
/// refusal.
fn read<T: DeserializeOwned>(path: &Path, what: &str) -> Result<Option<T>> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(Error::writing(path, error)),
        Ok(_) => {}
    }
    let unreadable = || Error::new(Code::State, format!("{what} cannot be read."));
    let bytes = atlas_core::files::read_own(path, LONGEST).ok_or_else(unreadable)?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| unreadable().with_source(error))
}

/// Keep `value` at `path`, in the lifecycle's own folder: written whole
/// beside it, renamed, and its folder written through, so it never reads as
/// missing once kept.
fn keep<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    atlas_core::files::write_durable(path, &serde_json::to_vec(value)?, 0o600)
}

/// Make `value` at `path` only while nothing is there: whether it was made.
fn create<T: Serialize>(path: &Path, value: &T) -> Result<bool> {
    let bytes = serde_json::to_vec(value)?;
    let mut made = true;
    atlas_core::files::write_beside(
        path,
        0o600,
        |file| Ok(std::io::Write::write_all(file, &bytes)?),
        |temporary| match std::fs::hard_link(temporary, path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                made = false;
                Ok(())
            }
            Err(error) => Err(Error::writing(path, error)),
        },
    )?;
    if made {
        sync_parent(path)?;
    }
    Ok(made)
}

/// Remove the file at `path`; one already gone is fine.
fn remove(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => sync_parent(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Error::writing(path, error)),
    }
}

/// Make what was renamed or removed beside `path` reach the disk.
fn sync_parent(path: &Path) -> Result<()> {
    match path.parent() {
        Some(folder) => super::sync(folder),
        None => Ok(()),
    }
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
