//! How far Atlas VM's install in a VM's guest got, as Atlas VM keeps it in
//! the VM's folder, `Bootstrap.json`: read only, never written here.
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
};
use serde::Deserialize;
use std::path::Path;

/// The most of the record read.
const LONGEST: u64 = 16 * 1024;

/// The phases an install goes through, in Atlas VM's order, as the person
/// reads each once done.
const PHASES: [&str; 8] = [
    "Atlas packed for the guest",
    "sent to the guest",
    "installed in the guest",
    "its services started",
    "a second administrator made",
    "the guest's account is no longer an administrator",
    "Remote Login being turned off",
    "closed",
];

/// The phase an install's close begins with, as far as an install goes
/// before the station's word.
const CLOSING: u8 = 6;

/// An install's progress.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub(super) struct Progress {
    /// The install change it belongs to.
    pub(super) change: Token,
    /// The last phase done, by its place in Atlas VM's order.
    #[serde(default)]
    pub(super) done: Option<u8>,
    /// Whether Atlas VM took the station's word that its guest paired.
    #[serde(default)]
    pub(super) paired: bool,
    /// Why its last try stopped short.
    #[serde(default)]
    pub(super) failure: Option<String>,
    /// Why it was refused for good.
    #[serde(default)]
    pub(super) refused: Option<String>,
}

impl Progress {
    /// Whether it reached its close.
    #[must_use]
    pub(super) fn closing(&self) -> bool {
        self.done.is_some_and(|done| done >= CLOSING)
    }

    /// Its last phase done, as the person reads it.
    #[must_use]
    pub(super) fn said(&self) -> Option<&'static str> {
        self.done
            .and_then(|done| PHASES.get(usize::from(done)))
            .copied()
    }

    /// The progress kept at `path`; none when there is none yet.
    ///
    /// # Errors
    ///
    /// Fails when it is there but cannot be read: it is never taken for
    /// absent.
    pub(super) fn read(path: &Path) -> Result<Option<Self>> {
        match std::fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(Error::writing(path, error)),
            Ok(_) => {}
        }
        let unreadable = || Error::new(Code::State, "The VM's install progress cannot be read.");
        let bytes = atlas_core::files::read_own(path, LONGEST).ok_or_else(unreadable)?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| unreadable().with_source(error))
    }
}
