//! Explicit, frozen file snapshots for an existing hidden login.
//! This is a resumable transaction, not a watcher or an account clone.
pub mod discovery;
mod files;
mod installs;
mod list;
mod model;

pub(crate) use model::Phase;
pub mod profile;
mod requests;
mod runner;
mod state;
#[cfg(test)]
mod tests;
mod undo;
mod worker;

use crate::{
    client::Client,
    config::parse_toml,
    control::Ownership,
    error::{Error, Result},
    task::{self, Claims},
    transport::{OutputError, run_output},
};
use files::*;
use fs2::FileExt;
pub use list::list;
use model::*;
pub(crate) use requests::{install_app, installs, remove_app};
pub use runner::NO_PROFILE;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt},
    path::{Path, PathBuf},
    time::Duration,
};
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
    let manifest = fs::canonicalize(manifest)
        .map_err(|error| Error::new("io", format!("{}: {error}", manifest.display())))?;
    let selection: Manifest = parse_toml(&read_limited(&manifest, MAX_MANIFEST)?, &manifest)?;
    freeze(client, &manifest, selection.files, apply_now).await
}

pub(super) async fn freeze(
    client: &Client,
    manifest: &Path,
    selections: Vec<Selection>,
    apply_now: bool,
) -> Result<Value> {
    if selections.is_empty() || selections.len() > MAX_SELECTIONS {
        return Err(problem("Select 1–32 files"));
    }

    unique_targets(selections.iter().map(|file| file.target.as_path()))?;
    client.identity().await?;

    let helper = helper_name(client.config.helper_uid)?;
    freeze_as(client, &helper, manifest, selections, apply_now).await
}

/// [`freeze`] for the helper account `helper`, once the helper is verified.
async fn freeze_as(
    client: &Client,
    helper: &str,
    manifest: &Path,
    selections: Vec<Selection>,
    apply_now: bool,
) -> Result<Value> {
    prune(client, helper).await;

    let id = crate::control::token();
    let stage = stage_path(client, &id)?;
    // Held while the preview is made, so no other command sweeps the stage
    // as a preview cut short before its plan is written.
    let lock = create_locked_stage(&stage, helper)?;

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
            allow_helper(&source, helper, false)?;

            files.push(Payload {
                source,
                target: file.target,
                sha256: digest(&bytes),
                executable: file.executable,
            });
        }

        let mut result = dispatch(
            client,
            helper,
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

        let plan = result
            .as_object_mut()
            .and_then(|result| result.remove("plan"))
            .unwrap_or(Value::Null);
        let plan: Plan = serde_json::from_value(plan)
            .map_err(|_| problem("Invalid environment record or schema"))?;
        if plan.id != id || plan.uid != client.config.helper_uid {
            return Err(problem("Preview returned a different plan identity"));
        }

        atomic_write(
            &stage.join("plan.json"),
            &serde_json::to_vec_pretty(&plan)?,
            0o600,
        )?;
        claim_plan(client, &id);

        result["retainedPlan"] = json!(stage);
        atomic_write(
            &stage.join("last-result.json"),
            &serde_json::to_vec_pretty(&result)?,
            0o600,
        )?;

        Ok(result)
    }
    .await;

    let result = previewed(result, &id, &stage);
    // The apply takes the plan's lock itself.
    drop(lock);
    match result {
        Ok(result) if apply_now && result["ok"] == true => apply_frozen(client, helper, &id).await,
        result => result,
    }
}

/// A preview's result, with its stage kept only when it holds a plan. A
/// preview reads and changes nothing, so a stage it never wrote a plan into
/// (a refusal or an error on the way) can never be applied, and is removed
/// with its copies of the selected files.
fn previewed(result: Result<Value>, id: &str, stage: &Path) -> Result<Value> {
    if !planned(stage) {
        discard(stage);
    }
    retained(result, id, stage)
}

/// Whether `stage` holds a frozen plan, or may: one that cannot be looked
/// for counts as there, so it is never removed for want of a plan.
fn planned(stage: &Path) -> bool {
    stage.join("plan.json").try_exists().unwrap_or(true)
}

/// Apply or resume the same immutable snapshot. Original source files are not read.
pub async fn apply(client: &Client, plan_id: &str) -> Result<Value> {
    let helper = helper_name(client.config.helper_uid)?;
    tidy(client);
    apply_frozen(client, &helper, plan_id).await
}

async fn apply_frozen(client: &Client, helper: &str, plan_id: &str) -> Result<Value> {
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

    // Claimed again for a plan another task previewed, before the helper
    // is asked.
    claim_plan(client, plan_id);
    let result = dispatch(client, helper, &stage, Request::Apply { plan }).await;
    // Discarded while still locked, so nothing else can start on it.
    if settled(&result, "applied") {
        discard(&stage);
    }

    drop(lock);
    operation(retained(result, plan_id, &stage), "apply")
}

/// Note that the command's task made plan `plan_id`. A claim that cannot
/// be written changes nothing: the transaction list still finds the plan.
fn claim_plan(client: &Client, plan_id: &str) {
    if let Some(id) = crate::control::Token::parse(plan_id) {
        let _ = Claims::of(client).claim(&task::Owned::Plan { id }, task::State::Made);
    }
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

/// Whether a rollback's working folder can go. An undo that finished needs
/// no plan. A folder the rollback made itself holds no plan either (only its
/// lock), so it goes once no request in it is left unconfirmed, whatever
/// the helper answered: an undo that stopped part way resumes from the
/// helper's own records.
fn rolled_back(result: &Result<Value>, apply: bool, created: bool, stage: &Path) -> bool {
    (apply && settled(result, "rolledBack")) || (created && !requests(stage))
}

/// Say which step a result answers, so it is read in the right direction.
fn operation(mut result: Result<Value>, name: &str) -> Result<Value> {
    if let Ok(value) = &mut result {
        value["operation"] = json!(name);
    }
    result
}

/// What a request folder that may change the helper is named with: one
/// left behind holds a step whose outcome is not known yet.
const CHANGE: &str = "request-";
/// What a read-only request folder is named with: one left behind (its
/// command killed mid-read) holds nothing to recover.
const READ: &str = "read-";

/// Whether `stage` holds a retained request: a step whose outcome is not
/// known yet, kept as evidence until it is. A folder or entry that cannot
/// be read may hold one, so it counts as holding one.
fn requests(stage: &Path) -> bool {
    let Ok(entries) = fs::read_dir(stage) else {
        return true;
    };

    entries.into_iter().any(|entry| {
        entry.map_or(true, |entry| {
            entry.file_name().to_string_lossy().starts_with(CHANGE)
        })
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

/// What a folder being removed is first renamed to, so no command ever sees
/// it half removed; one left behind holds nothing anyone needs.
const DISCARDED: &str = ".environment-discarded-";

/// What a stage is made under before it is locked and moved into place, so
/// no command sees it unlocked. A sweep never looks at it.
const STAGING: &str = ".environment-staging-";

/// Remove a frozen plan the helper no longer needs, called under its lock:
/// it is moved to a discarded name first, so its own name is gone at once
/// and nothing is left there half removed. Where it cannot be moved it is
/// removed where it is.
fn discard(stage: &Path) {
    let discarded = stage
        .parent()
        .map(|parent| parent.join(format!("{DISCARDED}{}", crate::control::token())));
    match discarded {
        Some(discarded) if fs::rename(stage, &discarded).is_ok() => {
            let _ = fs::remove_dir_all(discarded);
        }
        _ => {
            let _ = fs::remove_dir_all(stage);
        }
    }
}

/// Remove what a [`discard`] cut short left under a discarded name: Atlas's
/// own real folders, which no command uses once renamed.
fn clear_discarded(shared: &Path) {
    let Ok(entries) = fs::read_dir(shared) else {
        return;
    };

    for entry in entries.flatten() {
        let ours = entry.file_name().to_string_lossy().starts_with(DISCARDED)
            && fs::symlink_metadata(entry.path()).is_ok_and(|metadata| {
                metadata.file_type().is_dir() && metadata.uid() == nix::unistd::getuid().as_raw()
            });
        if ours {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// How long a frozen plan is kept before the helper is asked whether it is
/// still needed.
const UNUSED: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// How old a plan folder with no plan and no request whose outcome is
/// unknown must be before it is taken for a preview cut short rather than
/// one still being made. A preview holds its folder's lock while it runs,
/// and none takes this long.
const STARTING: Duration = Duration::from_secs(10 * 60);

/// Remove what earlier environment commands left behind that nothing can
/// use: [`prune_runs`], [`sweep`] and [`clear_discarded`].
fn tidy(client: &Client) {
    if let Some(shared) = client.config.identity_probe.parent() {
        prune_runs(shared);
        sweep(shared);
        clear_discarded(shared);
    }
}

/// Remove previews cut short, a killed command's: plan folders older than
/// [`STARTING`] with no plan and no request whose outcome is unknown, whose
/// lock nothing holds. They hold only copies of the selected files, which
/// nothing can apply.
fn sweep(shared: &Path) {
    for (_, stage) in stale(shared, STARTING) {
        // Looked at first, so a folder that is kept is never touched, then
        // again under the folder's own lock, which a preview being made holds.
        if !abandoned(&stage) {
            continue;
        }
        let Ok(lock) = lock_stage(&stage) else {
            continue;
        };
        if abandoned(&stage) {
            discard(&stage);
        }

        drop(lock);
    }
}

/// [`tidy`], then remove frozen plans older than UNUSED that the helper
/// never started (it answers that nothing was copied with their ID). Plans
/// applied or undone completely were removed when that finished; one with a
/// retained request (a step whose outcome is unknown), one the helper
/// started, or one it cannot answer for, stays. A plan removed is settled
/// gone wherever a task claimed it.
async fn prune(client: &Client, helper: &str) {
    let Some(shared) = client.config.identity_probe.parent() else {
        return;
    };

    tidy(client);

    let mut removed = Vec::new();
    for (id, stage) in stale(shared, UNUSED) {
        // Everything below happens under the plan's own lock; a plan in use
        // is skipped.
        let Ok(lock) = lock_stage(&stage) else {
            continue;
        };

        if !prunable(&stage, &id, client.config.helper_uid) {
            continue;
        }

        // A read-only question: has the helper ever started this plan?
        let answer = dispatch(
            client,
            helper,
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
            if fs::symlink_metadata(&stage).is_err()
                && let Some(id) = crate::control::Token::parse(&id)
            {
                removed.push(task::Owned::Plan { id });
            }
        }

        drop(lock);
    }
    task::settle_gone(client, &removed);
}

/// Remove the folders of commands run older than UNUSED. A command kept
/// its folder only when it was stopped or its reply was lost; no command
/// runs longer than ten minutes, so a week-old folder is no longer read.
fn prune_runs(shared: &Path) {
    for stage in stale_runs(shared, UNUSED) {
        discard(&stage);
    }
}

/// The frozen plans beside the shared command older than `age`, with their IDs.
fn stale(shared: &Path, age: Duration) -> Vec<(String, PathBuf)> {
    older(shared, "environment-plan-", age)
}

/// The command folders beside the shared command older than `age`.
fn stale_runs(shared: &Path, age: Duration) -> Vec<PathBuf> {
    older(shared, "environment-run-", age)
        .into_iter()
        .map(|(_, stage)| stage)
        .collect()
}

/// Atlas's folders beside the shared command named with `prefix` and older
/// than `age`, with their IDs.
fn older(shared: &Path, prefix: &str, age: Duration) -> Vec<(String, PathBuf)> {
    leftovers(shared)
        .into_iter()
        .filter_map(|stage| {
            let name = stage.file_name()?.to_string_lossy().into_owned();
            let id = name.strip_prefix(prefix)?.to_owned();
            let elapsed = fs::symlink_metadata(&stage)
                .ok()?
                .modified()
                .ok()?
                .elapsed()
                .ok()?;

            (elapsed > age).then_some((id, stage))
        })
        .collect()
}

/// Whether `stage` is a preview cut short: copies of the selected files but
/// no plan, so nothing can apply it, and no request whose outcome is
/// unknown, so the helper has nothing to recover from it.
fn abandoned(stage: &Path) -> bool {
    !planned(stage) && !requests(stage)
}

/// Whether the helper may be asked about the plan in `stage`: a readable
/// plan for this ID and helper, with no request whose outcome is unknown.
fn prunable(stage: &Path, id: &str, uid: u32) -> bool {
    let ours = read_limited(&stage.join("plan.json"), MAX_RECORD)
        .ok()
        .and_then(|bytes| decode::<Plan>(&bytes).ok())
        .is_some_and(|plan| plan.id == id && plan.uid == uid);

    ours && !requests(stage)
}

/// Execute a command with the installed helper-local profile, stopping it
/// after `timeout`. The reply is the actual process result, including its
/// output, status, signal and timeout.
pub async fn run(client: &Client, argv: Vec<String>, timeout: Duration) -> Result<Value> {
    runner::validate_argv(&argv)?;

    let id = crate::control::token();
    let stage = stage_path(client, &id)?.with_file_name(format!("environment-run-{id}"));
    let helper = helper_name(client.config.helper_uid)?;
    tidy(client);
    create_stage(&stage, &helper)?;

    let request = Request::Run { argv };
    let mut result = dispatch_within(client, &helper, &stage, request, timeout).await;
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

/// Name what `stage` still holds: a frozen plan, or only requests whose
/// outcome is unknown (kept as evidence), or nothing once it is gone.
fn retained(mut result: Result<Value>, id: &str, stage: &Path) -> Result<Value> {
    let plan = planned(stage);
    let kept = stage.exists();

    match &mut result {
        Ok(value) => {
            value["retainedPlan"] = if plan { json!(stage) } else { Value::Null };
            if kept && !plan {
                value["retainedRequest"] = json!(stage);
            }
            value["planId"] = json!(id);
        }
        Err(error) if plan => error.error.push_str(&format!(
            " Frozen plan {id} retained at {}.",
            stage.display()
        )),
        Err(error) if kept => error
            .error
            .push_str(&format!(" Retained request: {}.", stage.display())),
        Err(_) => {}
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

/// Make a private folder the helper may read, or nothing.
fn create_stage(stage: &Path, helper: &str) -> Result<()> {
    fs::DirBuilder::new()
        .mode(0o700)
        .create(stage)
        .map_err(|error| Error::writing(stage, error))?;

    let shared = remove_inherited_acl(stage).and_then(|()| allow_helper(stage, helper, true));
    if shared.is_err() {
        let _ = fs::remove_dir(stage);
    }

    shared
}

/// Make a private folder the helper may read at `stage`, holding its lock.
/// It is made and locked under a name no sweep looks at, then moved into
/// place, so no other command ever finds it unlocked.
fn create_locked_stage(stage: &Path, helper: &str) -> Result<fs::File> {
    create_locked_stage_with(stage, helper, |_| {})
}

/// [`create_locked_stage`], calling `made` with the folder once it is made
/// and before it is locked.
fn create_locked_stage_with(
    stage: &Path,
    helper: &str,
    made: impl FnOnce(&Path),
) -> Result<fs::File> {
    let parent = stage
        .parent()
        .ok_or_else(|| problem("Missing shared directory"))?;
    let staging = parent.join(format!("{STAGING}{}", crate::control::token()));
    create_stage(&staging, helper)?;
    made(&staging);

    let placed = lock_stage(&staging).and_then(|lock| {
        fs::rename(&staging, stage)
            .map(|()| lock)
            .map_err(|error| Error::writing(stage, error))
    });
    if placed.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }

    placed
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

    // A listing looks at the lock for an instant, and a process being
    // spawned elsewhere briefly holds every open file of its parent: a lock
    // held that briefly is waited out; a command using the plan holds it
    // far longer.
    let until = std::time::Instant::now() + STAGE_PATIENCE;
    while lock.try_lock_exclusive().is_err() {
        if std::time::Instant::now() >= until {
            return Err(problem("This environment plan is already running"));
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    Ok(lock)
}

/// How long a plan's lock that looks held is tried again.
const STAGE_PATIENCE: Duration = Duration::from_millis(250);

/// [`dispatch_within`] the usual time a helper command is given.
async fn dispatch(client: &Client, helper: &str, stage: &Path, request: Request) -> Result<Value> {
    dispatch_within(client, helper, stage, request, crate::client::RUN_TIMEOUT).await
}

/// [`dispatch`] under control the caller already holds, as a cleanup or an
/// install does from start to end: taking it again from the same process
/// would answer busy.
async fn dispatch_owned(
    client: &Client,
    ownership: &Ownership,
    helper: &str,
    stage: &Path,
    request: Request,
) -> Result<Value> {
    dispatch_as(
        client,
        Some(ownership),
        helper,
        stage,
        request,
        crate::client::RUN_TIMEOUT,
    )
    .await
}

/// Each invocation gets immutable arguments and a fresh session pin. Mutating
/// requests and their binary remain available whenever the reply is uncertain;
/// a request that was never sent leaves nothing behind. A request that only
/// reads takes no control and leaves nothing pending. The worker is stopped
/// after `timeout`.
async fn dispatch_within(
    client: &Client,
    helper: &str,
    stage: &Path,
    request: Request,
    timeout: Duration,
) -> Result<Value> {
    dispatch_as(client, None, helper, stage, request, timeout).await
}

/// [`dispatch_within`], a change running under `held` when the caller
/// already holds control, and under control taken here otherwise.
async fn dispatch_as(
    client: &Client,
    held: Option<&Ownership>,
    helper: &str,
    stage: &Path,
    request: Request,
    timeout: Duration,
) -> Result<Value> {
    let mutating = request.changes_helper();
    let raw_output = matches!(&request, Request::Run { .. });
    // A preview only reads in the helper: like any other inspection it
    // takes no control and records nothing pending, so a lost reply leaves
    // no recovery behind. A change reads the helper under control, so a
    // wait for it never leaves the packet expecting a helper that has since
    // restarted.
    let taken = (mutating && held.is_none())
        .then(|| client.controller.acquire(false))
        .transpose()?;
    let ownership = held.filter(|_| mutating).or(taken.as_ref());
    let packet = Packet {
        version: 1,
        expected: Identity::from_epoch(&client.identity().await?.epoch),
        request,
    };

    let kind = if mutating { CHANGE } else { READ };
    let name = format!("{kind}{}", crate::control::token());
    let invocation = stage.join(&name);
    create_stage(&invocation, helper)?;

    let argv = match stage_request(&invocation, helper, &packet, mutating) {
        Ok(argv) => argv,
        Err(error) => {
            let _ = fs::remove_dir_all(&invocation);
            return Err(error);
        }
    };

    let result = match ownership {
        None => client
            .inspect_within(argv, timeout)
            .await
            .and_then(|reply| decode_reply(&reply, false, &name)),
        Some(ownership) => change(client, ownership, argv, timeout, raw_output, &name).await,
    };

    if !delivery_uncertain(&result) {
        // A complete child reply means its immutable request is no longer in use.
        let _ = fs::remove_dir_all(&invocation);
    }

    result
}

/// Run the worker request `name` that changes the helper, under control the
/// caller holds. It stays pending until a reply shows what it did; `raw_output`
/// passes a command's own output through.
async fn change(
    client: &Client,
    ownership: &Ownership,
    argv: Vec<String>,
    timeout: Duration,
    raw_output: bool,
    name: &str,
) -> Result<Value> {
    // A process exit does not acknowledge the environment mutation.
    let generation = client.controller.quarantine()?;
    let result = match client.run_owned_within(ownership, argv, timeout).await {
        Ok(reply) if reply["ok"] == false => Err(crate::transport::refusal(&reply)),
        Ok(reply) if raw_output => refused(reply, name),
        Ok(reply) => decode_reply(&reply, true, name),
        Err(error) => Err(error),
    };

    if !delivery_uncertain(&result) {
        client.controller.clear_if(&generation)?;
    }

    result
}

/// Write one request (its packet and, for a loose build, the executable that
/// runs it) into `invocation`: the argv that runs it in the helper.
fn stage_request(
    invocation: &Path,
    helper: &str,
    packet: &Packet,
    mutating: bool,
) -> Result<Vec<String>> {
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
        allow_helper(&executable, helper, true)?;
        executable
    };

    let path = invocation.join("packet.json");
    atomic_write(&path, &serde_json::to_vec(packet)?, 0o600)?;
    allow_helper(&path, helper, false)?;

    let mut argv = vec![
        executable.to_string_lossy().into_owned(),
        "environment-worker".into(),
        path.to_string_lossy().into_owned(),
    ];

    if mutating {
        argv.push("--apply".into());
    }

    Ok(argv)
}

fn decode_reply(reply: &Value, mutating: bool, request: &str) -> Result<Value> {
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

    // The worker of `request` refuses before it changes anything (another
    // operation is running, the plan changed, nothing was copied with that
    // ID): there is nothing to resume. Any other failed mutation may have
    // stopped part way.
    let refused = reply["exitCode"] == WORKER_REFUSED && value["refusal"] == request;
    if mutating && value["ok"] == false && !refused {
        value["resumeRequired"] = json!(true);
    }

    Ok(value)
}
