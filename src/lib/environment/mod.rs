//! Explicit, frozen file snapshots for an existing hidden login.
//! This is a resumable transaction, not a watcher or an account clone.
pub mod discovery;
mod files;
mod installs;
mod list;
mod model;

pub(crate) use model::Phase;
mod one_off;
pub mod profile;
mod reply;
mod runner;
mod stage;
mod state;
pub use stage::WORKER_REFUSED;
pub(crate) use stage::leftovers;
use stage::*;
#[cfg(test)]
mod tests;
mod undo;
mod worker;

use crate::{
    client::{Client, Run},
    config::parse_toml,
    control::Ownership,
    error::{Code, Error, Result},
    files::LockFile,
    task::{self, Claims},
    transport::{OutputError, run_output},
};
use files::*;
use fs2::FileExt;
pub(crate) use installs::HELD_DAYS;
pub use list::list;
use model::*;
pub(crate) use one_off::{install_app, install_states, installed_app, remove_app};
pub(crate) use reply::{InstallState, KeptReason, Outcome, Removal, RunningFrom, SideEffect};
pub(crate) use runner::NO_PROFILE;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt},
    path::{Path, PathBuf},
    time::Duration,
};
#[cfg(test)]
pub(crate) use undo::PlanPhase;
pub(crate) use undo::{PlanState, Undone, plan_states, undo_plan};
pub use worker::worker;

const MAX_FILE: u64 = 128 * 1024 * 1024;
const MAX_RECORD: u64 = 4 * 1024 * 1024;
/// The largest manifest or profile read.
const MAX_MANIFEST: u64 = 16 * 1024;
/// How many files one plan may copy.
const MAX_SELECTIONS: usize = 32;
const STATE: &str = ".config/atlas/provisioning";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    files: Vec<Selection>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    source: PathBuf,
    target: PathBuf,
    #[serde(default)]
    executable: bool,
}

fn problem(message: impl Into<String>) -> Error {
    Error::new(Code::Environment, message)
}

/// Identity and search-path variables the helper runner owns.
pub(super) const RESERVED_VARIABLES: &[&str] = &[
    "HOME", "USER", "LOGNAME", "SHELL", "PATH", "UID", "EUID", "TMPDIR",
];

/// Where tools installed for every account live, in search order.
pub(crate) const SHARED_BIN: &[&str] = &[
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "/usr/bin",
    "/bin",
    "/usr/sbin",
    "/sbin",
];

/// Where a command runs from inside the helper whose home is `home`: the
/// helper's own tools first, then the shared places.
pub(crate) fn helper_search_path(home: &Path) -> Vec<PathBuf> {
    let mut places = vec![home.join(".local/bin"), home.join(".cargo/bin")];
    places.extend(SHARED_BIN.iter().map(PathBuf::from));
    places
}

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
pub async fn sync(client: &Client, manifest: &Path, apply_now: bool) -> Result<Value> {
    let manifest = fs::canonicalize(manifest)
        .map_err(|error| Error::new(Code::Io, format!("{}: {error}", manifest.display())))?;
    let selection: Manifest = parse_toml(&read_limited(&manifest, MAX_MANIFEST)?, &manifest)?;
    freeze(client, &manifest, selection.files, apply_now).await
}

/// Apply or resume the same immutable snapshot. Original source files are not read.
pub async fn apply(client: &Client, plan_id: &str) -> Result<Value> {
    let helper = helper_name(client.config.helper_uid)?;
    tidy(client);
    apply_frozen(client, &helper, plan_id).await
}

/// Preview rollback by default; restore only this transaction's unchanged files.
pub async fn rollback(client: &Client, transaction_id: &str, apply: bool) -> Result<Value> {
    let stage = stage_path(client, transaction_id)?;
    let helper = helper_name(client.config.helper_uid)?;
    tidy(client);
    let created = directory(&stage, false)?.is_none();
    let lock = if created {
        create_locked_stage(&stage, &helper)?
    } else {
        lock_stage(&stage)?
    };
    let result = dispatch(
        client,
        &helper,
        &stage,
        Request::Rollback {
            id: transaction_id.into(),
            apply,
        },
    )
    .await;
    if rolled_back(&result, apply, created, &stage) {
        discard(&stage);
    }

    drop(lock);
    operation(
        retained(result, transaction_id, &stage),
        if apply { "rollback" } else { "rollbackPreview" },
    )
}

/// Execute a command with the installed helper-local profile, stopping it
/// after `timeout`. The reply is the actual process result, including its
/// output, status, signal and timeout.
pub async fn run(client: &Client, argv: Vec<String>, timeout: Duration) -> Result<Value> {
    runner::validate_argv(&argv)?;

    let id = crate::files::token();
    let stage = stage_path(client, &id)?.with_file_name(format!("environment-run-{id}"));
    let helper = helper_name(client.config.helper_uid)?;
    tidy(client);
    create_stage(&stage, &helper)?;

    let request = Request::Run { argv };
    let mut result = dispatch_as(client, None, &helper, &stage, request, timeout).await;
    if delivery_uncertain(&result) {
        match &mut result {
            Ok(value) => value["retainedRequest"] = json!(stage),
            Err(error) => error
                .error
                .push_str(&format!(" Retained request: {}.", stage.display())),
        }
    } else {
        let _ = fs::remove_dir_all(&stage);
    }

    result
}
