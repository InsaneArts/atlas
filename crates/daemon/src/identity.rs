//! An account as Atlas knows it: its user ID, its name and its GeneratedUID,
//! which macOS never gives to another account, and how the account now
//! stands against the one on record. Setup reads and records these; what
//! removes or logs out an account carries them from its first look to its
//! last step.
use serde::{Deserialize, Serialize};

/// One account, as directory services name it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Identity {
    pub uid: u32,
    pub name: String,
    pub generated_uid: String,
}

/// What is on file for one helper user ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnFile {
    /// Nothing: no helper has been chosen for this user ID.
    Nothing,
    /// The helper's identity, as recorded.
    Recorded(Identity),
    /// A record that cannot be read: never taken for nothing.
    Unreadable,
}

/// How the account with a helper's user ID stands against its record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Standing {
    /// Nothing is recorded, and the account can be read: it may be recorded.
    Unrecorded(Identity),
    /// It is the account recorded.
    Same(Identity),
    /// It is another account than the one recorded: only an explicit new
    /// choice of helper replaces the record.
    Replaced,
    /// The account or its record cannot be read.
    Unknown,
}
