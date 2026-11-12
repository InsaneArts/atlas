//! The user desktop's side of environments: plans and requests staged in
//! folders beside the shared Atlas command on this disk, which the user's
//! account may read by an ACL, and the steps that hand them to its worker.
//! What every kind shares (the model, the worker, replies, installs) is in
//! [`crate::environment`].
pub mod stage;
#[cfg(test)]
#[path = "environment_tests.rs"]
mod tests;

use crate::{
    client::{Client, Run},
    environment::{
        files::*,
        list::Frozen,
        model::*,
        profile::Profile,
        reply::{InstallState, ShownFile, TransactionSummary, Transactions, WorkerReply},
        runner::ENVIRONMENT_TARGET,
        stage::*,
        undo::PlanPhase,
        *,
    },
    task::{self, Claims},
};
use atlas_control::Ownership;
use atlas_core::{
    error::{Code, Error, Result},
    files::{LockFile, Token},
};
use fs2::FileExt;
use serde_json::{Value, json};
use stage::*;
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

/// [`rollback`] in the second login `login`.
pub(crate) async fn rollback_in(
    client: &Client,
    login: &atlas_core::user::User,
    transaction_id: &str,
    apply: bool,
) -> Result<Value> {
    let stage = stage_path(client, transaction_id)?;
    let helper = helper_name(login.uid)?;
    tidy(client);
    let (lock, created) = open_stage(&stage, &helper)?;
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

/// [`run`] in the second login `login`, once its arguments are valid.
pub(crate) async fn run_in(
    client: &Client,
    login: &atlas_core::user::User,
    argv: Vec<String>,
    timeout: Duration,
) -> Result<Value> {
    let stage = request_stage(client)?;
    let helper = helper_name(login.uid)?;
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

/// The helper's short name, as an ACL entry names it. Resolved once per
/// operation and passed to every allow_helper of it.
pub(crate) fn helper_name(uid: u32) -> Result<String> {
    let identity = atlas_core::bounded::output(
        Command::new("/usr/bin/id").args(["-un", &uid.to_string()]),
        atlas_core::bounded::LOOKUP,
    )?;
    let name =
        String::from_utf8(identity.stdout).map_err(|_| problem("Invalid helper short name"))?;
    let name = name.trim();
    if !identity.status.success() || !acl_name(name) {
        return Err(problem("Could not resolve helper UID to an ACL identity"));
    }

    Ok(name.to_owned())
}

fn acl_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
}

/// Let the helper named `helper` (from helper_name) read `path`.
fn allow_helper(path: &Path, helper: &str, executable: bool) -> Result<()> {
    if !acl_name(helper) {
        return Err(problem("Could not resolve helper UID to an ACL identity"));
    }

    let rights = if executable {
        "read,execute,readattr,readsecurity"
    } else {
        "read,readattr,readsecurity"
    };

    let output = atlas_core::bounded::output(
        Command::new("/bin/chmod")
            .args(["+a", &format!("user:{helper} allow {rights}")])
            .arg(path),
        atlas_core::bounded::FILE,
    )?;
    if !output.status.success() {
        let reason = String::from_utf8_lossy(&output.stderr);
        let reason = reason.trim();
        return Err(problem(if reason.is_empty() {
            "Could not grant the helper access to staged files".to_owned()
        } else {
            format!("Could not grant the helper access to staged files: {reason}")
        }));
    }

    Ok(())
}

/// Send `request` to the helper account `helper` through a request folder
/// of its own, under `held` when it changes the helper. A change may copy
/// a large app, so it is given as long as any command; a read, a command's
/// usual time.
pub(crate) async fn send(
    client: &Client,
    helper: &str,
    held: Option<&Ownership>,
    request: Request,
) -> Result<Value> {
    let stage = request_stage(client)?;
    create_stage(&stage, helper)?;

    let timeout = if request.changes_helper() {
        crate::client::RUN_TIMEOUT_MAX
    } else {
        crate::client::RUN_TIMEOUT
    };
    let result = dispatch_as(client, held, helper, &stage, request, timeout).await;
    if !delivery_uncertain(&result) && !requests(&stage) {
        discard(&stage);
    }
    result
}

/// [`install_states`] of the installs `unique` in the second login `login`.
pub(crate) async fn install_states_in(
    client: &Client,
    login: &atlas_core::user::User,
    unique: &[String],
) -> Result<Vec<InstallState>> {
    let helper = helper_name(login.uid)?;
    let mut listed = Vec::new();
    for batch in unique.chunks(crate::environment::installs::ASKED) {
        let request = Request::Installs {
            ids: batch.to_vec(),
        };
        let answer = crate::environment::one_off::read(
            send(client, &helper, None, request).await?,
            "installs",
            "The helper's word on its installs was malformed",
        )?;
        listed.extend(
            answer["installs"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|install| serde_json::from_value(install.clone()).ok()),
        );
    }
    Ok(listed)
}

/// What [`list`](crate::environment::list()) lists of the second login `login`, whose account ACLs name
/// `helper`: the frozen plans here, and the helper's own transactions.
pub(crate) async fn list_as(
    client: &Client,
    login: &atlas_core::user::User,
    helper: &str,
    all: bool,
) -> Result<(Vec<Frozen>, Transactions)> {
    let here = frozen(shared_folder(client)?, login.uid);
    let there = transactions(client, helper, all, &here).await?;
    Ok((here, there))
}

/// The most frozen plans the helper is asked about by ID.
const ASKED: usize = 200;

/// The helper's own list of its transactions, with those rolled back when
/// `all`, and whatever it has of the newest frozen plans in `here`; asked
/// through a request folder of its own that goes once the answer is in.
async fn transactions(
    client: &Client,
    helper: &str,
    all: bool,
    here: &[Frozen],
) -> Result<Transactions> {
    let mut newest: Vec<&Frozen> = here.iter().collect();
    newest.sort_by_key(|plan| std::cmp::Reverse(plan.updated));
    let ids = newest
        .into_iter()
        .take(ASKED)
        .map(|plan| plan.id.clone())
        .collect();
    ask(client, helper, all, ids).await
}

/// The helper's own list of its transactions, with those rolled back when
/// `all`, and whatever it has of each of `ids` whatever its phase and age.
async fn ask(client: &Client, helper: &str, all: bool, ids: Vec<String>) -> Result<Transactions> {
    if ids.len() > ASKED {
        return Err(problem("Too many plans were asked about"));
    }
    let malformed = "The helper's list of its copies was malformed";
    let answer = send(client, helper, None, Request::Transactions { all, ids }).await?;
    let answer = crate::environment::one_off::read(answer, "transactions", malformed)?;
    serde_json::from_value(answer)
        .map_err(|cause| Error::new(Code::Protocol, malformed).with_source(cause))
}

/// The plan folders beside the shared command in `shared` that hold a plan
/// for helper `uid` or a request whose outcome is unknown.
fn frozen(shared: &Path, uid: u32) -> Vec<Frozen> {
    leftovers(shared)
        .into_iter()
        .filter_map(|stage| {
            let name = stage.file_name()?.to_str()?.to_owned();
            let id = name.strip_prefix("environment-plan-")?.to_owned();
            let files = kept_plan(&stage, &id, uid)
                .map(|plan| file_report(&plan).iter().map(ShownFile::from).collect());
            let request = requests(&stage);
            if files.is_none() && !request {
                return None;
            }
            Some(Frozen::new(
                id,
                files,
                request,
                held(&stage),
                changed_ms(&stage).unwrap_or(0),
            ))
        })
        .collect()
}

/// Whether a command holds the lock of the plan folder `stage` right now.
/// It is looked at with a shared lock let go of at once; a command that
/// takes the lock in that instant waits it out.
fn held(stage: &Path) -> bool {
    match open_regular(&stage.join("controller.lock"), false) {
        Ok(Some(lock)) => FileExt::try_lock_shared(&LockFile(lock).0).is_err(),
        Ok(None) | Err(_) => false,
    }
}

/// [`plan_states`] of the plans `ids` in the second login `login`.
pub(crate) async fn plan_states_in(
    client: &Client,
    login: &atlas_core::user::User,
    ids: &[Token],
) -> Result<Vec<PlanState>> {
    let helper = helper_name(login.uid)?;
    let here = frozen(shared_folder(client)?, login.uid);
    let mut transactions: Vec<TransactionSummary> = Vec::new();
    for batch in ids.chunks(ASKED) {
        let asked: Vec<String> = batch.iter().map(|id| id.as_str().to_owned()).collect();
        let there: Transactions = ask(client, &helper, true, asked).await?;
        if !there.all_read() {
            return Err(problem(
                "Atlas could not read every record of the helper's copies, so it cannot tell \
                 what undoing one would remove",
            ));
        }
        transactions.extend(there.known().cloned());
    }

    Ok(ids
        .iter()
        .map(|id| {
            let plan = here.iter().find(|plan| plan.id == id.as_str());
            let started = transactions
                .iter()
                .find(|transaction| transaction.plan_id == id.as_str());
            match (started, plan) {
                (Some(transaction), _) => PlanState {
                    phase: if plan.is_some_and(|plan| plan.running) {
                        PlanPhase::Running
                    } else {
                        PlanPhase::started(&transaction.phase).unwrap_or(PlanPhase::Unknown)
                    },
                    targets: transaction
                        .files
                        .iter()
                        .map(|file| file.target.to_string_lossy().into_owned())
                        .collect(),
                },
                (None, Some(plan)) => PlanState {
                    phase: plan.state(),
                    targets: plan.targets(),
                },
                (None, None) => PlanState {
                    phase: PlanPhase::Gone,
                    targets: Vec::new(),
                },
            }
        })
        .collect())
}

/// [`undo_plan`] of plan `id` in the second login `login`.
pub(crate) async fn undo_plan_in(
    client: &Client,
    login: &atlas_core::user::User,
    ownership: &Ownership,
    id: &Token,
) -> Result<Undone> {
    let helper = helper_name(login.uid)?;
    let stage = stage_path(client, id.as_str())?;
    let (lock, created) = open_stage(&stage, &helper)?;

    let undone = undo_locked(client, ownership, &helper, &stage, id, created).await;
    // A folder made only to ask goes, unless it keeps a request whose
    // outcome is not known.
    if created && fs::symlink_metadata(&stage).is_ok() && !requests(&stage) {
        discard(&stage);
    }
    drop(lock);
    undone
}

async fn undo_locked(
    client: &Client,
    ownership: &Ownership,
    helper: &str,
    stage: &Path,
    id: &Token,
    created: bool,
) -> Result<Undone> {
    let ask = |apply| Request::Rollback {
        id: id.as_str().to_owned(),
        apply,
    };
    let preview = dispatch_as(
        client,
        Some(ownership),
        helper,
        stage,
        ask(false),
        crate::client::RUN_TIMEOUT,
    )
    .await?;
    let preview = WorkerReply::read(&preview);

    if preview.never_started() {
        if requests(stage) {
            return Ok(Undone::Unfinished(
                "A step of this copy into the helper has an outcome Atlas does not know".into(),
            ));
        }
        let frozen = !created && planned(stage);
        if !discarded(stage) {
            return Ok(Undone::Unfinished(UNREMOVED.into()));
        }
        return Ok(if frozen {
            Undone::Discarded
        } else {
            Undone::AlreadyGone
        });
    }
    if preview.ok != Some(true) {
        return Ok(Undone::Unfinished(preview.error));
    }
    let reply = match preview.phase {
        Some(Phase::RolledBack) => preview,
        Some(Phase::Applying | Phase::Applied | Phase::RollingBack) => {
            let reply = dispatch_as(
                client,
                Some(ownership),
                helper,
                stage,
                ask(true),
                crate::client::RUN_TIMEOUT,
            )
            .await?;
            let reply = WorkerReply::read(&reply);
            if !reply.settled(Phase::RolledBack) {
                return Ok(Undone::Unfinished(reply.error));
            }
            reply
        }
        None => {
            return Ok(Undone::Unfinished(
                "The helper reported an unknown transaction phase; no rollback was sent".into(),
            ));
        }
    };
    if !requests(stage) && !discarded(stage) {
        return Ok(Undone::Unfinished(UNREMOVED.into()));
    }
    Ok(crate::environment::undo::rolled_back(reply))
}

/// Why undoing a plan is not finished when what Atlas kept for it would
/// not go.
const UNREMOVED: &str = "Atlas could not remove what it kept for this copy into the helper";

/// Discard `stage`, and whether it is gone: a folder still there is never
/// taken for removed.
fn discarded(stage: &Path) -> bool {
    discard(stage);
    fs::symlink_metadata(stage).is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
}

/// What the helper offers of `profile`'s selections, before anything is
/// copied: a reference it cannot use as selected is refused.
async fn readiness(client: &Client, profile: &Profile) -> Result<Value> {
    let references = profile
        .references
        .iter()
        .map(|reference| reference.path.clone())
        .collect::<Vec<_>>();
    if profile.tools.is_empty()
        && profile.applications.is_empty()
        && profile.environment.is_empty()
        && references.is_empty()
    {
        return Ok(
            json!({"ok":true,"readOnly":true,"selections":[],"environment":[],"references":[]}),
        );
    }

    let discovery = discovery::inspect(
        client,
        &profile.tools,
        &profile.applications,
        &profile.environment,
        &references,
    )
    .await?;
    for (reference, observed) in profile.references.iter().zip(&discovery.references) {
        if !observed.helper.permits_reference(reference.writable) {
            return Err(problem(format!(
                "Selected reference is not accessible to the helper with the requested permissions: {}",
                reference.path.display()
            )));
        }
    }

    Ok(discovery.report())
}

/// [`sync`](crate::environment::profile::sync) of `profile`, read from `path`, whose selected values are
/// `bytes`, in the second login `login`.
pub(crate) async fn sync_in(
    client: &Client,
    login: &atlas_core::user::User,
    path: PathBuf,
    profile: Profile,
    bytes: Vec<u8>,
    apply: bool,
) -> Result<Value> {
    let readiness = readiness(client, &profile).await?;

    // Owned from the moment it is made, so it goes even when this future is
    // dropped part way.
    let temporary = tempfile::Builder::new()
        .prefix(&format!(
            "environment-profile-{}",
            atlas_core::files::token()
        ))
        .rand_bytes(0)
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(fs::canonicalize(client.output_dir()?)?)?;
    let generated = temporary.path().join("environment.json");

    let result: Result<Value> = async {
        crate::environment::files::remove_inherited_acl(temporary.path())?;
        crate::environment::files::atomic_write(&generated, &bytes, 0o600)?;

        let mut selections = profile.files;
        selections.push(Selection {
            source: generated,
            target: ENVIRONMENT_TARGET.into(),
            executable: false,
        });

        let mut result = freeze(client, login, &path, selections, apply).await?;
        result["profileVersion"] = json!(2);
        result["environmentNames"] = json!(profile.environment);
        result["readinessBeforeApply"] = readiness;
        result["environmentScope"] = json!(
            "Selected exported values are frozen privately and used by environment run. \
             Existing GUI apps and interactive shell startup files are not modified. \
             Application logins remain separate."
        );

        Ok(result)
    }
    .await;

    // freeze has copied the generated bytes into its own retained plan before it
    // can dispatch an apply. Removing this source cannot invalidate recovery.
    let folder = temporary.path().to_owned();
    let cleanup = temporary.close();

    match result {
        Ok(mut value) => {
            if cleanup.is_err() {
                value["cleanupWarning"] = json!(format!(
                    "The operation result is known, but a private generated source remains at {}",
                    folder.display()
                ));
            }
            Ok(value)
        }
        Err(mut error) => {
            if cleanup.is_err() {
                error.error.push_str(&format!(
                    " Private generated source cleanup is incomplete at {}.",
                    folder.display()
                ));
            }
            Err(error)
        }
    }
}
