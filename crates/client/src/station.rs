//! What the station keeps of each VM desktop on this Mac, apart from any
//! folder a person may move or clear: a private folder per VM in Atlas's
//! own folder, holding its pairing record, and the Keychain items that hold
//! its enrolment and the station's key.
pub mod enrolment;
pub mod pairing;

use crate::station_key::StationKey;
use atlas_core::{
    desktop::{DesktopId, RecordKey},
    error::{Code, Result},
};
use atlas_session::{keys::Keypair, pairing_file::PairingFile};
use std::path::{Path, PathBuf};

/// Where the station's folders are in a home.
const FOLDER: &str = "Library/Application Support/Atlas/Station";

/// The station's folders in one account's home.
#[derive(Clone, Debug)]
pub struct Station {
    root: PathBuf,
}

impl Station {
    /// This account's.
    ///
    /// # Errors
    ///
    /// Fails when this account's home cannot be found.
    pub fn own() -> Result<Self> {
        Ok(Self::under(&atlas_core::config::home()?))
    }

    /// The station's folders in the home `home`.
    #[must_use]
    pub fn under(home: &Path) -> Self {
        Self {
            root: home.join(FOLDER),
        }
    }

    /// The station's own folder, made private when missing and checked
    /// private either way.
    fn root(&self) -> Result<&Path> {
        atlas_core::files::private_folder(&self.root, Code::State)?;
        Ok(&self.root)
    }

    /// The private folder the station keeps `vm`'s state in, `vm-<id>`: made
    /// 0700 when missing, and refused unless it is a folder of this account's
    /// that no one else may read or change.
    ///
    /// # Errors
    ///
    /// Fails when either folder cannot be made, or is not private.
    pub fn folder(&self, vm: &DesktopId) -> Result<PathBuf> {
        self.root()?;
        let folder = self.path_of(vm);
        atlas_core::files::private_folder(&folder, Code::State)?;
        Ok(folder)
    }

    /// Where `vm`'s private folder is, made or not, for what only reads it
    /// or removes it.
    #[must_use]
    pub fn path_of(&self, vm: &DesktopId) -> PathBuf {
        self.root.join(RecordKey::vm(vm).as_str())
    }

    /// Where the station keeps each VM's lifecycle, made or not: beside
    /// the VMs' own folders, never in one, since a removal deletes those.
    #[must_use]
    pub fn lifecycle_path(&self) -> PathBuf {
        self.root.join("lifecycle")
    }

    /// `vm`'s pairing record, in its private folder.
    ///
    /// # Errors
    ///
    /// As [`Self::folder`].
    pub fn pairing(&self, vm: &DesktopId) -> Result<PairingFile> {
        Ok(PairingFile::at(self.folder(vm)?))
    }

    /// The lock every station process takes to make the station's key.
    ///
    /// # Errors
    ///
    /// As [`Self::folder`].
    pub fn key_lock(&self) -> Result<PathBuf> {
        Ok(self.root()?.join(".key.lock"))
    }

    /// The station's key, made once.
    ///
    /// # Errors
    ///
    /// Fails as [`StationKey::own`] does, or when the station's folder is
    /// not private.
    pub fn key(&self) -> Result<Keypair> {
        StationKey::default().own(&self.key_lock()?)
    }
}

#[cfg(test)]
#[path = "station_tests.rs"]
mod tests;
