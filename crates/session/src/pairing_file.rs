//! The station's pairing record for one desktop, as a private file in a
//! private folder: read and changed only under the desktop's lock, which
//! every station process takes, replaced in one rename, and synced with
//! every folder that holds it, so a power cut leaves the record before or
//! after, never lost.
use crate::pairing::{Enrolment, StationRecords};
use atlas_core::error::{Code, Error, Result};
use std::{
    fs::{self, File},
    os::unix::fs::{DirBuilderExt, MetadataExt},
    path::{Path, PathBuf},
};

/// The longest record read.
const LIMIT: u64 = 16 * 1024;

/// How long a change waits for another station process to let go of the
/// record.
const PATIENCE: std::time::Duration = std::time::Duration::from_secs(2);

/// A desktop's pairing record, `<desktops>/<desktop>/pairing.json`.
#[derive(Clone, Debug)]
pub struct PairingFile {
    /// The station's desktops folder: the one ancestor trusted, made and
    /// checked private like the desktop's own.
    desktops: PathBuf,
    folder: PathBuf,
    path: PathBuf,
}

impl PairingFile {
    /// The record of `desktop` in the station's `desktops` folder.
    pub fn of(desktops: impl Into<PathBuf>, desktop: &atlas_core::desktop::DesktopId) -> Self {
        let desktops = desktops.into();
        let folder = desktops.join(desktop.as_str());
        let path = folder.join("pairing.json");
        Self {
            desktops,
            folder,
            path,
        }
    }

    fn refused(&self, why: &str) -> Error {
        Error::new(
            Code::Config,
            format!(
                "The pairing record {} {why}; the desktop is refused until it is paired again.",
                self.path.display()
            ),
        )
    }

    /// Both folders, made private when missing and checked private either
    /// way: a link at either is refused.
    fn make_folders(&self) -> Result<()> {
        for folder in [&self.desktops, &self.folder] {
            match fs::DirBuilder::new().mode(0o700).create(folder) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(Error::writing(folder, error)),
            }
        }
        self.check_folders()
    }

    fn check_folders(&self) -> Result<()> {
        atlas_core::files::private_folder(&self.desktops, Code::Config)?;
        atlas_core::files::private_folder(&self.folder, Code::Config)
    }

    /// Make the record and every folder that holds it durable: each folder
    /// is synced into its parent every time, so one made by a change that
    /// was cut short, or a rename whose sync failed, is durable before the
    /// record is trusted.
    fn sync_all(&self) -> Result<()> {
        let above = self.desktops.parent().unwrap_or(Path::new("."));
        for folder in [above, &self.desktops, &self.folder] {
            sync(folder)?;
        }
        Ok(())
    }

    /// The desktop's lock, which every station process takes to read or
    /// change the record, waited for no longer than [`PATIENCE`].
    fn lock(&self) -> Result<atlas_core::files::LockFile> {
        let lock = atlas_core::files::open_lock(&self.folder.join("pairing.lock"))?;
        atlas_core::files::lock_within(&lock, atlas_core::files::Lock::Exclusive, PATIENCE)
            .map_err(|_| {
                Error::new(
                    Code::Busy,
                    "Another Atlas is changing the desktop's pairing; try again.",
                )
            })?;
        Ok(lock)
    }

    /// What is kept now, under the lock.
    fn current(&self) -> Result<Option<Enrolment>> {
        let metadata = match fs::symlink_metadata(&self.path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(Error::writing(&self.path, error)),
            Ok(metadata) => metadata,
        };
        if metadata.mode() & 0o077 != 0 {
            return Err(self.refused("may be changed by others"));
        }
        let bytes = atlas_core::files::read_own(&self.path, LIMIT)
            .ok_or_else(|| self.refused("is not a private file of this account's"))?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| self.refused("is damaged or from a later version"))
    }
}

fn sync(folder: &Path) -> Result<()> {
    File::open(folder)
        .and_then(|folder| folder.sync_all())
        .map_err(|error| Error::writing(folder, error))
}

impl StationRecords for PairingFile {
    /// What is kept, made durable first: a record is trusted only once a
    /// power cut can no longer take it back.
    fn read(&self) -> Result<Option<Enrolment>> {
        match fs::symlink_metadata(&self.folder) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(Error::writing(&self.folder, error)),
            Ok(_) => {}
        }
        self.check_folders()?;
        let _locked = self.lock()?;
        let kept = self.current()?;
        if kept.is_some() {
            self.sync_all()?;
        }
        Ok(kept)
    }

    fn replace(&self, expected: Option<&Enrolment>, record: &Enrolment) -> Result<()> {
        self.make_folders()?;
        let _locked = self.lock()?;
        if self.current()?.as_ref() != expected {
            return Err(Error::new(
                Code::Busy,
                "The desktop's pairing changed meanwhile; try again.",
            ));
        }
        atlas_core::files::write_atomic(&self.path, &serde_json::to_vec_pretty(record)?, 0o600)?;
        self.sync_all()
    }
}

#[cfg(test)]
#[path = "pairing_file_tests.rs"]
mod tests;
