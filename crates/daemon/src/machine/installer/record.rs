//! What the installer keeps of each transaction, root-owned, written before
//! each step and after it: what recovery settles from.
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
    fs_move::FileName,
};
use nix::{
    fcntl::AtFlags,
    sys::stat::{SFlag, fstatat},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs::File, path::Path};

/// The longest record read.
const LONGEST: u64 = 256 * 1024;

/// A folder as it was seen: its device and inode, which no rename changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Identity {
    pub(super) device: u64,
    pub(super) inode: u64,
    /// Whether it is a folder.
    pub(super) folder: bool,
}

impl Identity {
    /// Whatever is at `name` in `folder`, never through a link; none when
    /// there is nothing there. A file or a link has an identity too, which
    /// no recorded folder's matches.
    pub(super) fn of(folder: &File, name: &FileName) -> Result<Option<Self>> {
        match fstatat(folder, name.as_c_str(), AtFlags::AT_SYMLINK_NOFOLLOW) {
            Ok(stat) => Ok(Some(Self {
                device: u64::try_from(stat.st_dev).unwrap_or_default(),
                inode: stat.st_ino,
                folder: SFlag::from_bits_truncate(stat.st_mode) & SFlag::S_IFMT == SFlag::S_IFDIR,
            })),
            Err(nix::errno::Errno::ENOENT) => Ok(None),
            Err(error) => {
                Err(Error::new(Code::Io, "A bundle could not be looked at.").with_source(error))
            }
        }
    }
}

/// Where a transaction is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Phase {
    /// The new bundle is being copied in from the daemon's staged one.
    Copying,
    /// It is copied in and its signature verified.
    Verified,
    /// It is about to be swapped with the installed bundle.
    Swapping,
    /// It is swapped into place; its place not yet checked.
    PlacedUnverified,
    /// It is in place and checked; the previous bundle kept.
    Placed,
    /// It is being released: from here it is never rolled back.
    Releasing,
    /// Done: the previous bundle is gone.
    Released,
    /// The previous bundle is being put back.
    RollingBack,
    /// The previous bundle is back; its daemon still to be started again.
    Restarting,
    /// The previous bundle is back and its daemon started again.
    RolledBack,
    /// Stopped before anything installed changed.
    Aborted,
    /// Something it did not expect was found; nothing more is done until
    /// someone looks.
    Pending,
}

impl Phase {
    /// Whether it is over.
    pub(super) fn ended(self) -> bool {
        matches!(self, Self::Released | Self::RolledBack | Self::Aborted)
    }
}

/// One request, as it was asked and what it came to: a request ID is bound
/// to its operation and arguments, and answered again from here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Asked {
    pub(super) request: Token,
    pub(super) op: String,
    /// Its arguments, as one string compared whole.
    pub(super) args: String,
    /// The transaction it is about, once there is one.
    pub(super) transaction: Option<Token>,
    /// Its reply; none while it is being done.
    pub(super) reply: Option<Value>,
}

impl Asked {
    /// The record at `file`; none when there is none.
    pub(super) fn read(file: &Path) -> Result<Option<Self>> {
        read(file)
    }

    /// Keep it at `file`, whole or not at all.
    pub(super) fn write(&self, file: &Path) -> Result<()> {
        write(file, self)
    }
}

/// One update of the guest's Atlas.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Transaction {
    /// The daemon's handover ID.
    pub(super) id: Token,
    /// The account whose machine job asked for it.
    pub(super) uid: u32,
    /// The digest of the snapshot it installs.
    pub(super) manifest: String,
    pub(super) phase: Phase,
    /// Why it is pending, when it is.
    pub(super) why: Option<String>,
    /// The installed bundle when it was taken in.
    pub(super) installed: Identity,
    /// The new bundle, once copied in.
    pub(super) new: Option<Identity>,
    /// The new bundle's `atlas`, as its code directory hash in hex.
    pub(super) cdhash: Option<String>,
    /// When it was placed, in milliseconds past the epoch.
    pub(super) placed_at: Option<u64>,
}

impl Transaction {
    /// The record at `file`; none when there is none.
    pub(super) fn read(file: &Path) -> Result<Option<Self>> {
        read(file)
    }

    /// Keep it at `file`, whole or not at all.
    pub(super) fn write(&self, file: &Path) -> Result<()> {
        write(file, self)
    }
}

/// The record at `file`, root's own; none when there is none.
fn read<T: serde::de::DeserializeOwned>(file: &Path) -> Result<Option<T>> {
    if !file.exists() {
        return Ok(None);
    }
    let bytes = atlas_core::files::read_own(file, LONGEST)
        .ok_or_else(|| Error::new(Code::State, "An installer record cannot be read."))?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| Error::new(Code::State, "An installer record is damaged."))
}

/// Keep `record` at `file`: written whole beside it, renamed into place, and
/// its folder written through, so a crash leaves the old record or the new.
fn write(file: &Path, record: &impl Serialize) -> Result<()> {
    atlas_core::files::write_atomic(file, &serde_json::to_vec(record)?, 0o600)?;
    let folder = file
        .parent()
        .ok_or_else(|| Error::new(Code::State, "An installer record has no folder."))?;
    File::open(folder)?.sync_all()?;
    Ok(())
}
