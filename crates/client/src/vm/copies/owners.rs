//! Who each of a VM guest's copies was made for, kept on this Mac in the
//! VM's own records folder: one record per copy, named by its token, written
//! before its stage is said and kept until its removal is confirmed. A copy
//! is removed only for the host session its record names; one with no
//! record, or one whose record cannot be read, is never taken as anyone's.
//!
//! The records folder is Atlas's own, inside the very folder no file an
//! agent names may be in: the records are kept as Atlas keeps every record
//! of its own (a private folder, a private file written whole and synced,
//! then renamed into place; read only when it is a plain file of this
//! account's), never through the guard for files an agent names, which
//! refuses that folder by design.
use crate::host_session::Key;
use atlas_core::{
    error::{Code, Result},
    files::{self, Token},
};
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

/// The folder of the records, in a VM's records folder.
const FOLDER: &str = "copies";

/// The longest record read.
const LONGEST: u64 = 4096;

/// Who one copy was made for: the host session of the command that staged
/// it, and its task, or none for a command without one; the frozen plan
/// whose payloads it holds, when it holds a plan's, which only that plan's
/// discard removes; and the digest of the native check's fixture it holds,
/// when it holds that, which only a check of another fixture replaces.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Owner {
    pub(crate) copy: Token,
    pub(crate) session: Option<Key>,
    pub(crate) task: Option<Key>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) plan: Option<Token>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) fixture: Option<String>,
}

impl Owner {
    /// Whether a command of host session `session`, or none, may take the
    /// copy for its own.
    pub(crate) fn names(&self, session: Option<&Key>) -> bool {
        self.session.as_ref() == session
    }
}

/// What the record of one copy says.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Owned {
    By(Owner),
    /// No record: nobody is known to have made it.
    Unknown,
    /// A record there that cannot be read, or names another copy.
    Unreadable,
}

/// The records of a VM's copies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Owners(PathBuf);

impl Owners {
    /// The records kept in the VM's records folder `records`.
    pub(crate) fn in_records(records: &Path) -> Self {
        Self(records.join(FOLDER))
    }

    fn name(copy: &Token) -> OsString {
        OsString::from(format!("{}.json", copy.as_str()))
    }

    /// Keep `owner`, written whole to the disk in a private folder of this
    /// account's, before anything of its copy is said.
    ///
    /// # Errors
    ///
    /// Fails when the folder or the record cannot be made.
    pub(crate) fn keep(&self, owner: &Owner) -> Result<()> {
        files::private_folder(&self.0, Code::State)?;
        let bytes = serde_json::to_vec(owner)?;
        files::write_atomic(&self.0.join(Self::name(&owner.copy)), &bytes, 0o600)
    }

    /// What the record of `copy` says.
    pub(crate) fn of(&self, copy: &Token) -> Owned {
        let path = self.0.join(Self::name(copy));
        match std::fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Owned::Unknown,
            Err(_) => return Owned::Unreadable,
            Ok(_) => {}
        }
        match files::read_own(&path, LONGEST)
            .and_then(|bytes| serde_json::from_slice::<Owner>(&bytes).ok())
        {
            Some(owner) if owner.copy == *copy => Owned::By(owner),
            _ => Owned::Unreadable,
        }
    }

    /// Forget the record of `copy`, once the copy is known gone.
    ///
    /// # Errors
    ///
    /// Fails when the record cannot be removed.
    pub(crate) fn forget(&self, copy: &Token) -> Result<()> {
        let path = self.0.join(Self::name(copy));
        match std::fs::remove_file(&path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                Err(atlas_core::error::Error::writing(&path, error))
            }
            Ok(()) | Err(_) => Ok(()),
        }
    }
}

#[cfg(test)]
#[path = "owners_tests.rs"]
mod tests;
