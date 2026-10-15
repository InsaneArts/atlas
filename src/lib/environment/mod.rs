//! Explicit, frozen file snapshots for an existing hidden login.
//! This is a resumable transaction, not a watcher or an account clone.
pub mod discovery;
mod files;
mod model;
pub mod profile;
mod runner;
mod state;
#[cfg(test)]
mod tests;
mod worker;

use crate::{
    client::Client,
    config::parse_toml,
    error::{Error, Result},
    transport::{OutputError, run_output},
};
use files::*;
use fs2::FileExt;
use model::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt},
    path::{Path, PathBuf},
};
pub use worker::worker;

const MAX_FILE: u64 = 128 * 1024 * 1024;
const MAX_RECORD: u64 = 4 * 1024 * 1024;
const STATE: &str = ".config/atlas/provisioning";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    files: Vec<Selection>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Selection {
    pub source: PathBuf,
    pub target: PathBuf,
    #[serde(default)]
    pub executable: bool,
}

fn problem(message: impl Into<String>) -> Error {
    Error::new("environment", message)
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
    let manifest = fs::canonicalize(manifest)?;
    let selection: Manifest = parse_toml(&read_limited(&manifest, 16384)?, &manifest)?;
    freeze(client, &manifest, selection.files, apply_now).await
}

pub(super) async fn freeze(
    client: &Client,
    manifest: &Path,
    selections: Vec<Selection>,
    apply_now: bool,
) -> Result<Value> {
    client.identity().await?;
    prune(client).await;
    if selections.is_empty() || selections.len() > 32 {
        return Err(problem("Select 1–32 files"));
    }
    unique_targets(selections.iter().map(|file| file.target.as_path()))?;
    let id = crate::control::token();
    let stage = stage_path(client, &id)?;
    create_stage(&stage, client.config.helper_uid)?;
    let result = async {
        let mut files = Vec::new();
        let mut total = 0;
        for (index, file) in selections.into_iter().enumerate() {
            let source = if file.source.is_absolute() {
                file.source
            } else {
                manifest
                    .parent()
                    .ok_or_else(|| problem("Missing manifest parent"))?
                    .join(file.source)
            };
            let source = fs::canonicalize(source)?;
            let bytes = read_limited(&source, MAX_FILE)?;
            total += bytes.len() as u64;
            if total > MAX_FILE {
                return Err(problem("Selected files exceed the aggregate 128 MiB limit"));
            }
            let source = stage.join(format!("payload-{index}"));
            atomic_write(&source, &bytes, 0o600)?;
            allow_helper(&source, client.config.helper_uid, false)?;
            files.push(Payload {
                source,
                target: file.target,
                sha256: digest(&bytes),
                executable: file.executable,
            });
        }
        let mut result = dispatch(
            client,
            &stage,
            Request::Preview {
                id: id.clone(),
                files,
            },
        )
        .await?;
        if result["ok"] == false && result["plan"].is_null() {
            return Ok(result);
        }
        let plan: Plan = decode(&serde_json::to_vec(&result["plan"])?)?;
        if plan.id != id || plan.uid != client.config.helper_uid {
            return Err(problem("Preview returned a different plan identity"));
        }
        atomic_write(
            &stage.join("plan.json"),
            &serde_json::to_vec_pretty(&plan)?,
            0o600,
        )?;
        result
            .as_object_mut()
            .ok_or_else(|| problem("Invalid preview result"))?
            .remove("plan");
        result["retainedPlan"] = json!(stage);
        atomic_write(
            &stage.join("last-result.json"),
            &serde_json::to_vec_pretty(&result)?,
            0o600,
        )?;
        if apply_now && result["ok"] == true {
            apply(client, &id).await
        } else {
            Ok(result)
        }
    }
    .await;
    retained(result, &id, &stage)
}

/// Apply or resume the same immutable snapshot. Original source files are not read.
pub async fn apply(client: &Client, plan_id: &str) -> Result<Value> {
    let stage = stage_path(client, plan_id)?;
    if directory(&stage, false)?.is_none() {
        return Err(problem(format!(
            "There is no plan {plan_id} to apply: it was applied completely, or never made. \
             To undo an applied plan, run atlas environment rollback {plan_id} --apply."
        )));
    }
    let lock = lock_stage(&stage)?;
    let plan: Plan = decode(&read_limited(&stage.join("plan.json"), MAX_RECORD)?)?;
    if plan.id != plan_id || plan.uid != client.config.helper_uid {
        return Err(problem(
            "Frozen plan belongs to a different helper or plan ID",
        ));
    }
    let result = dispatch(client, &stage, Request::Apply { plan }).await;
    // Discarded while still locked, so nothing else can start on it.
    if settled(&result, "applied") {
        discard(&stage);
    }
    drop(lock);
    operation(retained(result, plan_id, &stage), "apply")
}

/// Preview rollback by default; restore only this transaction's unchanged files.
pub async fn rollback(client: &Client, transaction_id: &str, apply: bool) -> Result<Value> {
    let stage = stage_path(client, transaction_id)?;
    let created = directory(&stage, false)?.is_none();
    if created {
        create_stage(&stage, client.config.helper_uid)?;
    }
    let lock = lock_stage(&stage)?;
    let result = dispatch(
        client,
        &stage,
        Request::Rollback {
            id: transaction_id.into(),
            apply,
        },
    )
    .await;
    // An undo that finished needs no plan. A preview changes nothing, so it
    // only removes a working folder it made itself and left empty of
    // unconfirmed requests.
    if (apply && settled(&result, "rolledBack")) || (created && !apply && !requests(&stage)) {
        discard(&stage);
    }
    drop(lock);
    operation(
        retained(result, transaction_id, &stage),
        if apply { "rollback" } else { "rollbackPreview" },
    )
}

/// Say which step a result answers, so it is read in the right direction.
fn operation(mut result: Result<Value>, name: &str) -> Result<Value> {
    if let Ok(value) = &mut result {
        value["operation"] = json!(name);
    }
    result
}

/// Whether `stage` holds a retained request: a step whose outcome is not
/// known yet, kept as evidence until it is.
fn requests(stage: &Path) -> bool {
    fs::read_dir(stage).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().starts_with("request-"))
    })
}

/// Whether the helper confirmed the transaction reached `phase` with nothing
/// left to resume, so its frozen plan (copies of the selected files) is no
/// longer needed: rollback works from the helper's own records.
fn settled(result: &Result<Value>, phase: &str) -> bool {
    result.as_ref().is_ok_and(|value| {
        value["ok"] == true && value["phase"] == phase && value["resumeRequired"] == false
    })
}

/// Atlas's own plan and request folders beside the shared command: named
/// with a valid ID, real folders owned by this account. Anything else there
/// is left alone.
pub fn leftovers(shared: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(shared) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let id = name
                .strip_prefix("environment-plan-")
                .or_else(|| name.strip_prefix("environment-run-"));
            id.is_some_and(|id| validate_id(id).is_ok())
                && fs::symlink_metadata(entry.path()).is_ok_and(|metadata| {
                    metadata.file_type().is_dir()
                        && metadata.uid() == nix::unistd::getuid().as_raw()
                })
        })
        .map(|entry| entry.path())
        .collect()
}

/// Remove a frozen plan the helper no longer needs.
fn discard(stage: &Path) {
    let _ = fs::remove_dir_all(stage);
}

/// How long a frozen plan is kept before the helper is asked whether it is
/// still needed.
const UNUSED: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 60 * 60);

/// Remove frozen plans older than UNUSED that the helper no longer needs:
/// ones it never applied, and ones applied or undone completely. A plan with
/// a retained request (a step whose outcome is unknown), one the helper must
/// still resume, or one it cannot answer for, stays.
async fn prune(client: &Client) {
    let Some(shared) = client.config.identity_probe.parent() else {
        return;
    };
    let old: Vec<(String, PathBuf)> = leftovers(shared)
        .into_iter()
        .filter_map(|stage| {
            let name = stage.file_name()?.to_string_lossy().into_owned();
            let id = name.strip_prefix("environment-plan-")?.to_owned();
            let age = fs::symlink_metadata(&stage)
                .ok()?
                .modified()
                .ok()?
                .elapsed()
                .ok()?;
            (age > UNUSED).then_some((id, stage))
        })
        .collect();
    for (id, stage) in old {
        // Everything below happens under the plan's own lock; a plan in use
        // is skipped.
        let Ok(lock) = lock_stage(&stage) else {
            continue;
        };
        let ours = read_limited(&stage.join("plan.json"), MAX_RECORD)
            .ok()
            .and_then(|bytes| decode::<Plan>(&bytes).ok())
            .is_some_and(|plan| plan.id == id && plan.uid == client.config.helper_uid);
        if !ours || requests(&stage) {
            continue;
        }
        // A read-only question: has the helper ever started this plan?
        let answer = dispatch(
            client,
            &stage,
            Request::Rollback {
                id: id.clone(),
                apply: false,
            },
        )
        .await;
        let never = answer
            .as_ref()
            .is_ok_and(|answer| answer["ok"] == false && answer["error"] == worker::MISSING);
        if never && !requests(&stage) {
            discard(&stage);
        }
        drop(lock);
    }
}

/// Execute a command with the installed helper-local profile. The reply is the
/// actual process result, including its output, status, signal and timeout.
pub async fn run(client: &Client, argv: Vec<String>) -> Result<Value> {
    runner::validate_argv(&argv)?;
    let id = crate::control::token();
    let stage = stage_path(client, &id)?.with_file_name(format!("environment-run-{id}"));
    create_stage(&stage, client.config.helper_uid)?;
    let mut result = dispatch(client, &stage, Request::Run { argv }).await;
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

/// The exit status of a worker that refused before running anything; its
/// error is its only output.
pub const WORKER_REFUSED: u8 = 125;

/// A run whose worker refused (no profile, a changed session) as that error;
/// any other reply is the command's own. A refusal names its private request
/// directory, which the command it would have run is never told, so even a
/// command that exits 125 with an error of the same shape stays the
/// command's result.
fn refused(reply: Value, request: &str) -> Result<Value> {
    if reply["exitCode"] != WORKER_REFUSED {
        return Ok(reply);
    }
    let error = run_output(&reply, 65536)
        .ok()
        .and_then(|output| serde_json::from_slice::<Value>(&output).ok())
        .filter(|value| {
            value["ok"] == false && value["error"].is_string() && value["refusal"] == request
        });
    match error {
        // The worker's own code is kept, so a changed session still reads as one.
        Some(error) => Err(Error::new(
            error["code"]
                .as_str()
                .and_then(crate::error::code)
                .unwrap_or("environment"),
            error["error"]
                .as_str()
                .unwrap_or("The helper environment refused"),
        )),
        None => Ok(reply),
    }
}

fn delivery_uncertain(result: &Result<Value>) -> bool {
    match result {
        Err(error) => error.uncertain,
        Ok(value) => value["timedOut"] == true || !value["signal"].is_null(),
    }
}

fn retained(mut result: Result<Value>, id: &str, stage: &Path) -> Result<Value> {
    let kept = stage.exists();
    match &mut result {
        Ok(value) => {
            value["retainedPlan"] = if kept { json!(stage) } else { Value::Null };
            value["planId"] = json!(id);
            value["transactionId"] = json!(id);
        }
        Err(error) => error.error.push_str(&format!(
            " Frozen plan {id} retained at {}.",
            stage.display()
        )),
    }
    result
}

fn stage_path(client: &Client, id: &str) -> Result<PathBuf> {
    validate_id(id)?;
    let parent = client
        .config
        .identity_probe
        .parent()
        .ok_or_else(|| problem("Missing shared directory"))?;
    let directory =
        directory(parent, false)?.ok_or_else(|| problem("Shared staging directory is missing"))?;
    installation_parent(&directory)?;
    if has_acl(parent)? {
        return Err(problem("Shared staging parent must not have an ACL"));
    }
    Ok(parent.join(format!("environment-plan-{id}")))
}

fn create_stage(stage: &Path, uid: u32) -> Result<()> {
    fs::DirBuilder::new().mode(0o700).create(stage)?;
    remove_inherited_acl(stage)?;
    allow_helper(stage, uid, true)
}

fn lock_stage(stage: &Path) -> Result<fs::File> {
    let directory = directory(stage, false)?.ok_or_else(|| problem("Frozen plan is missing"))?;
    installation_parent(&directory)?;
    if directory.metadata()?.mode() & 0o7777 != 0o700 {
        return Err(problem("Frozen plan directory is not private"));
    }
    let lock = open_regular(&stage.join("controller.lock"), true)?
        .ok_or_else(|| problem("Plan lock is missing"))?;
    validate_private(&lock, false)?;
    lock.try_lock_exclusive()
        .map_err(|_| problem("This environment plan is already running"))?;
    Ok(lock)
}

/// Each invocation gets immutable arguments and a fresh session pin. Mutating
/// requests and their binary remain available whenever the reply is uncertain.
async fn dispatch(client: &Client, stage: &Path, request: Request) -> Result<Value> {
    let mutating = matches!(
        &request,
        Request::Apply { .. } | Request::Rollback { apply: true, .. } | Request::Run { .. }
    );
    let raw_output = matches!(&request, Request::Run { .. });
    let invocation = stage.join(format!("request-{}", crate::control::token()));
    create_stage(&invocation, client.config.helper_uid)?;
    // Atlas.app's executable is sealed with its bundle, so a copy would
    // fail its signature: the helper runs the app's own executable. A loose
    // build is copied, so its request keeps the binary it was made with.
    let source = fs::canonicalize(std::env::current_exe()?)?;
    let executable = if crate::update::app_of(&source).is_some()
        && crate::update::reachable_by_others(&source)
    {
        source
    } else {
        let executable = invocation.join("atlas");
        atomic_write(&executable, &read_limited(&source, MAX_FILE)?, 0o700)?;
        allow_helper(&executable, client.config.helper_uid, true)?;
        executable
    };
    let expected = Identity::from_epoch(&client.identity().await?.epoch);
    let packet = Packet {
        version: 1,
        expected,
        request,
    };
    let path = invocation.join("packet.json");
    atomic_write(&path, &serde_json::to_vec(&packet)?, 0o600)?;
    allow_helper(&path, client.config.helper_uid, false)?;
    let mut argv = vec![
        executable.to_string_lossy().into_owned(),
        "environment-worker".into(),
        path.to_string_lossy().into_owned(),
    ];
    if mutating {
        argv.push("--apply".into());
    }
    let request = invocation
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let result = match client.run(argv).await {
        Ok(reply) if raw_output => refused(reply, &request),
        Ok(reply) => decode_reply(&reply, mutating),
        Err(error) => Err(error),
    };
    if !delivery_uncertain(&result) {
        // A complete child reply means its immutable request is no longer in use.
        let _ = fs::remove_dir_all(&invocation);
    }
    result
}

fn decode_reply(reply: &Value, mutating: bool) -> Result<Value> {
    let invalid = |message: &str| {
        let error = problem(message);
        if mutating { error.uncertain() } else { error }
    };
    if reply["timedOut"] == true || !reply["signal"].is_null() {
        return Err(invalid(
            "Environment worker was interrupted; retain the plan and inspect or resume its transaction",
        ));
    }
    let output = run_output(reply, MAX_RECORD as usize).map_err(|error| match error {
        OutputError::Malformed => invalid("Malformed environment output"),
        OutputError::Oversized => invalid("Environment output exceeded its limit"),
    })?;
    let mut value: Value = serde_json::from_slice(&output)
        .map_err(|_| invalid("Environment worker did not return a complete result"))?;
    if !value.is_object()
        || !value["ok"].is_boolean()
        || (reply["exitCode"] != 0 && value["ok"] != false)
    {
        return Err(invalid("Environment worker returned an invalid result"));
    }
    if mutating && value["ok"] == false {
        value["resumeRequired"] = json!(true);
    }
    Ok(value)
}
