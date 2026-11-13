//! Explicit, frozen file snapshots for an existing hidden login.
//! This is a resumable transaction, not a watcher or an account clone.
pub mod discovery;
pub(crate) mod files;
pub(crate) mod installs;
pub(crate) mod list;
pub(crate) mod model;

pub use model::Phase;
pub(crate) mod one_off;
pub mod profile;
pub(crate) mod reply;
pub(crate) mod runner;
pub(crate) mod stage;
mod state;
pub use stage::WORKER_REFUSED;
#[cfg(test)]
#[path = "environment_tests.rs"]
pub(crate) mod tests;
pub(crate) mod undo;
pub(crate) mod worker;

use crate::{client::Client, task};
use atlas_control::Ownership;
use atlas_core::{
    account::helper_search_path,
    config::parse_toml,
    desktop::Kind,
    error::{Code, Error, Result},
    transport::{OutputError, run_output},
};
use files::*;
pub use installs::HELD_DAYS;
pub use list::list;
use model::*;
pub(crate) use one_off::{install_app, install_states, installed_app, placed_app, remove_app};
pub(crate) use reply::{InstallState, KeptReason, Outcome, RemovalReply, RunningFrom, SideEffect};
pub(crate) use runner::NO_PROFILE;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::Duration,
};
#[cfg(test)]
pub(crate) use undo::PlanPhase;
pub(crate) use undo::{PlanState, Undone, plan_states, undo_plan};
pub use worker::worker;

pub(crate) const MAX_FILE: u64 = 128 * 1024 * 1024;
pub(crate) const MAX_RECORD: u64 = 4 * 1024 * 1024;
/// The largest manifest or profile read.
const MAX_MANIFEST: u64 = 16 * 1024;
/// How many files one plan may copy.
pub(crate) const MAX_SELECTIONS: usize = 32;
pub(crate) const STATE: &str = ".config/atlas/provisioning";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    files: Vec<Selection>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Selection {
    pub(crate) source: PathBuf,
    pub(crate) target: PathBuf,
    #[serde(default)]
    pub(crate) executable: bool,
}

pub(crate) fn problem(message: impl Into<String>) -> Error {
    Error::new(Code::Environment, message)
}

/// Identity and search-path variables the helper runner owns.
pub(super) const RESERVED_VARIABLES: &[&str] = &[
    "HOME", "USER", "LOGNAME", "SHELL", "PATH", "UID", "EUID", "TMPDIR",
];

/// A portable variable name: ASCII letters, digits and `_`, not leading digit.
pub(super) fn variable_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(c) if c.is_ascii_alphabetic() || c == b'_')
        && name.len() <= 128
        && bytes.all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

/// A name a profile may select and the runner may export.
pub(super) fn selectable_variable(name: &str) -> bool {
    variable_name(name) && !RESERVED_VARIABLES.contains(&name)
}

/// Freeze the selected bytes, then preview or apply the resulting plan.
///
/// # Errors
///
/// Returns manifest, identity, staging or worker failures. Apply may leave a retained
/// transaction; preview does not authorize later effects.
pub async fn sync(client: &Client, manifest: &Path, apply_now: bool) -> Result<Value> {
    let manifest = fs::canonicalize(manifest)
        .map_err(|error| Error::new(Code::Io, format!("{}: {error}", manifest.display())))?;
    let selection: Manifest = parse_toml(&read_limited(&manifest, MAX_MANIFEST)?, &manifest)?;
    match client.desktop().kind() {
        Kind::User(login) => {
            crate::user::environment::stage::freeze(
                client,
                login,
                &manifest,
                selection.files,
                apply_now,
            )
            .await
        }
    }
}

/// Apply or resume the same immutable snapshot. Original source files are not read.
///
/// # Errors
///
/// Rejects an unavailable or incompatible plan, changed identity or unsafe destinations.
/// Unacknowledged worker effects retain their transaction and pending ownership.
pub async fn apply(client: &Client, plan_id: &str) -> Result<Value> {
    match client.desktop().kind() {
        Kind::User(login) => crate::user::environment::apply_in(client, login, plan_id).await,
    }
}

/// Preview rollback by default; restore only this transaction's unchanged files.
///
/// # Errors
///
/// Returns transaction, identity, ownership or worker failures. Changed helper files may be
/// kept, and unknown restoration stays pending.
pub async fn rollback(client: &Client, transaction_id: &str, apply: bool) -> Result<Value> {
    match client.desktop().kind() {
        Kind::User(login) => {
            crate::user::environment::rollback_in(client, login, transaction_id, apply).await
        }
    }
}

/// Execute a command with the installed helper-local profile, stopping it
/// after `timeout`. The reply is the actual process result, including its
/// output, status, signal and timeout.
///
/// # Errors
///
/// Returns argument, environment-admission, ownership or command-completion failures. A
/// failed child can be represented in its reply; unknown execution stays pending.
pub async fn run(client: &Client, argv: Vec<String>, timeout: Duration) -> Result<Value> {
    runner::validate_argv(&argv)?;
    match client.desktop().kind() {
        Kind::User(login) => crate::user::environment::run_in(client, login, argv, timeout).await,
    }
}
