//! Notes beside the lock naming a command: the one that holds control, and
//! each one waiting for it. A note is a label: control is the lock alone.
//! Each names its process with its start time, so a note a killed command
//! left behind is told apart from a live one.
use crate::{
    error::Result,
    files::{Durability, write_private_with},
    host_session::{Host, HostId, Key, Label, Process, now_ms},
};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The most a note may hold before it is treated as missing.
const LIMIT: u64 = 4096;

/// A command that holds or waits for control.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    /// Its host session, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<Key>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<Host>,
    /// Its session's project folder's name, never its path, so a session
    /// on its first command is named as its record will name it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// A named session's own label, which names it; none for a host's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub named: Option<HostId>,
    #[serde(flatten)]
    pub process: Process,
    /// When it took control, or began to wait, in milliseconds since 1970.
    pub since: u64,
}

impl Note {
    /// A note for this process now, labelled `label`; None when macOS does
    /// not describe this process.
    pub(super) fn now(label: Option<&Label>) -> Option<Self> {
        Some(Self {
            session: label.map(|label| label.key.clone()),
            host: label.map(|label| label.host),
            folder: label.and_then(|label| label.folder.clone()),
            named: label.and_then(|label| label.named.clone()),
            process: Process::current()?,
            since: now_ms(),
        })
    }

    /// Whether the command it names still runs.
    pub fn live(&self) -> bool {
        self.process.running()
    }

    /// The note at `path`, when it is a plain file of this account that
    /// parses.
    pub(super) fn read(path: &Path) -> Option<Self> {
        serde_json::from_slice(&crate::files::read_own(path, LIMIT)?).ok()
    }

    /// Write this note at `path`, private to this account.
    pub(super) fn write(&self, path: &Path) -> Result<()> {
        write_private_with(path, &serde_json::to_vec(self)?, Durability::Kernel)
    }
}
