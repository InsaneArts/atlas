//! The environment on a VM desktop. Its guest keeps its own files, so a
//! plan's payloads are frozen here, in the VM's records, and staged into
//! the guest once, as a copy only the plan's discard removes; the plan
//! names them by where the guest placed them, for the guest's own account
//! and home. Every request is carried out by the guest's own Atlas as its
//! environment worker, handed the request as a change on a lane of its
//! own, claimed before it goes: one that may change the guest under the
//! control its command holds, one that only reads beside other agents.
//!
//! What the guest's own records say is trusted for its own bookkeeping
//! alone: whether a plan was applied, and so whether it may be forgotten,
//! rests on this Mac's own record of an apply that may have reached the
//! guest, kept beside the plan before the apply is said, marked once the
//! guest acknowledges it, and gone only when the guest says for certain
//! that it changed nothing, or once an undo is acknowledged. A change whose
//! outcome is unknown keeps a marker beside its plan while its claim is
//! pending; while one stands, the plan is never discarded, pruned or
//! undone, and the same frozen plan, sent again, resumes it. An app is
//! installed, taken away and looked at the same way, as a change the
//! guest's worker carries out, and this Mac keeps its own record of an
//! install that may have reached the guest.
mod frozen;
mod installs;
mod undo;

pub(in crate::vm) use frozen::Payloads;
pub(crate) use installs::{install_app_in, install_states_in, remove_app_in};
pub(crate) use undo::{plan_states_in, undo_plan_in};

use crate::{
    client::{Client, Mutation, RUN_TIMEOUT, Verified},
    environment::{
        MAX_FILE, MAX_SELECTIONS, NO_PROFILE, Selection, WORKER_REFUSED,
        files::{digest, read_limited, unique_targets},
        list::{Frozen as Listed, labelled},
        model::{Identity, Packet, Payload, Phase, Plan, Request, validate_id},
        one_off, problem,
        profile::{Profile, SCOPE},
        reply::{Transactions, WorkerReply},
        runner::ENVIRONMENT_TARGET,
        stage::{operation, settled},
    },
    task,
};
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
    protocol::MAX_REQUEST,
};
use atlas_session::compatibility::ENVIRONMENT;
use frozen::{Copy, Frozen};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

/// Room in a request for what the keeper and the guest say around it: the
/// lane, the change and its generation, and the name the guest gives the
/// worker's refusal.
const ENVELOPE: usize = 1024;

/// How long a plan with no plan frozen in it, and no change whose outcome
/// is unknown, must be left before it is taken for a preview cut short. A
/// preview holds its folder's lock while it runs, and none takes this long.
const STARTING: Duration = Duration::from_secs(10 * 60);

/// How long a frozen plan is kept before the guest is asked whether it is
/// still needed.
const UNUSED: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// The most frozen plans the guest is asked about by ID.
const ASKED: usize = 200;

/// How long the guest's worker is given to apply a plan or undo one,
/// copying up to 128 MiB into a VM: longer than a read's [`RUN_TIMEOUT`],
/// within the longest a command may run.
const CHANGING: Duration = Duration::from_secs(120);
const _: () = assert!(CHANGING.as_secs() <= crate::client::RUN_TIMEOUT_MAX.as_secs());

/// The words of a guest without an environment worker of its own.
fn older(vm: &str) -> String {
    format!("{vm} is running an older Atlas that has no environment worker")
}

/// The VM `client` acts on, proved now, its guest's worker offered.
pub(crate) async fn verified(client: &Client) -> Result<Verified> {
    client.verify_vm_offering(ENVIRONMENT, older).await
}

/// One file a plan copies: where its bytes come from, where it goes, and
/// whether it runs.
struct Chosen {
    source: Source,
    target: PathBuf,
    executable: bool,
}

/// Where a chosen file's bytes come from.
enum Source {
    /// A file on this Mac, as given.
    Path(PathBuf),
    /// Bytes Atlas made, such as a profile's variables.
    Made(Vec<u8>),
}

/// The refusal of a profile, or a discovery, naming paths on this Mac for
/// the VM `client` acts on to use as they are.
pub(crate) fn references_refused(client: &Client) -> Error {
    problem(format!(
        "{} keeps its own files, so a profile can't point it at a path on this Mac; copy them \
         instead as [[files]] entries.",
        client.selected.name
    ))
}

/// [`crate::environment::sync`] on a VM: `selections`, read from the
/// manifest at `manifest`, frozen here and previewed by the guest, then
/// applied when `apply_now`.
pub(crate) async fn sync_in(
    client: &Client,
    manifest: &Path,
    selections: Vec<Selection>,
    apply_now: bool,
) -> Result<Value> {
    let chosen = selections
        .into_iter()
        .map(|file| {
            Ok(Chosen {
                source: Source::Path(source_path(manifest, &file.source)?),
                target: file.target,
                executable: file.executable,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    freeze(client, chosen, apply_now).await
}

/// [`crate::environment::profile::sync`] on a VM: `profile`, read from
/// `path`, its files frozen beside `variables`, the values it selects, as
/// the environment `environment run` runs with, its references refused
/// already. Its readiness is not checked: Atlas looks for no tools, apps or
/// variables in a VM.
pub(crate) async fn profile_in(
    client: &Client,
    path: &Path,
    profile: Profile,
    variables: Vec<u8>,
    apply: bool,
) -> Result<Value> {
    let mut chosen = profile
        .files
        .into_iter()
        .map(|file| {
            Ok(Chosen {
                source: Source::Path(source_path(path, &file.source)?),
                target: file.target,
                executable: file.executable,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    chosen.push(Chosen {
        source: Source::Made(variables),
        target: ENVIRONMENT_TARGET.into(),
        executable: false,
    });
    let mut result = freeze(client, chosen, apply).await?;
    result["profileVersion"] = json!(2);
    result["environmentNames"] = json!(profile.environment);
    result["readinessBeforeApply"] = json!({"checked": false});
    result["environmentScope"] = json!(SCOPE);
    Ok(result)
}

/// Where a selection's `source` is: as given when absolute, else beside the
/// manifest or profile at `manifest`.
fn source_path(manifest: &Path, source: &Path) -> Result<PathBuf> {
    if source.is_absolute() {
        return Ok(source.to_path_buf());
    }
    Ok(manifest
        .parent()
        .ok_or_else(|| problem("Missing manifest parent"))?
        .join(source))
}

/// Freeze `chosen` here and have the guest preview the plan of it, then
/// apply it when `apply_now` and it previewed whole. Nothing is asked of
/// the guest before every file given is refused that holds the person's
/// sign-ins or secrets, or is in Atlas's own folder.
async fn freeze(client: &Client, chosen: Vec<Chosen>, apply_now: bool) -> Result<Value> {
    if chosen.is_empty() || chosen.len() > MAX_SELECTIONS {
        return Err(problem("Select 1–32 files"));
    }
    unique_targets(chosen.iter().map(|file| file.target.as_path()))?;
    let given: Vec<&Path> = chosen
        .iter()
        .filter_map(|file| match &file.source {
            Source::Path(path) => Some(path.as_path()),
            Source::Made(_) => None,
        })
        .collect();
    crate::credential_files::refuse(given.iter().copied())?;
    for path in given {
        atlas_core::own_folder::refuse(path)?;
    }

    let verified = verified(client).await?;
    prune_vm(client, &verified).await;
    let result = preview(client, &verified, chosen).await?;
    match result["planId"].as_str() {
        Some(id) if apply_now && WorkerReply::read(&result).ok == Some(true) => {
            let id = id.to_owned();
            apply_verified(client, &verified, &id).await
        }
        _ => Ok(result),
    }
}

/// The guest's preview of a new plan of `chosen`, frozen here under its
/// lock: a preview that froze no plan goes with what it made.
async fn preview(client: &Client, verified: &Verified, chosen: Vec<Chosen>) -> Result<Value> {
    let id = Token::new();
    let frozen = Frozen::of(client.output_dir()?, id.as_str())?;
    let lock = frozen.create()?;
    let result = previewing(client, verified, &frozen, chosen).await;
    if !frozen.planned() {
        discard_vm(client, verified, &frozen).await;
    }
    drop(lock);
    retained(result, &frozen)
}

async fn previewing(
    client: &Client,
    verified: &Verified,
    frozen: &Frozen,
    chosen: Vec<Chosen>,
) -> Result<Value> {
    let mut files = Vec::new();
    let mut total = 0;
    for (index, file) in chosen.into_iter().enumerate() {
        let bytes = match file.source {
            Source::Made(bytes) => bytes,
            Source::Path(given) => read_source(&given)?,
        };
        total += bytes.len() as u64;
        if total > MAX_FILE {
            return Err(problem("Selected files exceed the aggregate 128 MiB limit"));
        }
        frozen.keep_payload(index, &bytes)?;
        files.push((index, file.target, digest(&bytes), file.executable));
    }

    let copy = Token::new();
    frozen.keep_copy(&copy)?;
    let placed =
        crate::vm::copies::stage_frozen(client, &frozen.payloads(), &copy, verified).await?;
    let files = files
        .into_iter()
        .map(|(index, target, sha256, executable)| Payload {
            source: placed.join(frozen::payload(index)),
            target,
            sha256,
            executable,
        })
        .collect();
    let id = frozen.id().as_str().to_owned();
    let request = Request::Preview {
        id: id.clone(),
        files,
    };
    let mut result = send(client, verified, request, (&Token::new(), RUN_TIMEOUT)).await?;
    if WorkerReply::read(&result).ok == Some(false) && result["plan"].is_null() {
        return Ok(result);
    }

    let plan = result
        .as_object_mut()
        .and_then(|result| result.remove("plan"))
        .unwrap_or(Value::Null);
    let plan: Plan = serde_json::from_value(plan)
        .map_err(|cause| problem("Invalid environment record or schema").with_source(cause))?;
    let uid = machine(client, verified)?.epoch.uid;
    if plan.id != id || plan.uid != uid {
        return Err(problem("Preview returned a different plan identity"));
    }
    // Its apply is one request: a plan too large for one is never frozen.
    let (apply, _) = packet_of(client, verified, Request::Apply { plan: plan.clone() })?;
    if !fits(&apply, verified, &Token::new())? {
        return Err(too_large(client));
    }

    frozen.keep_plan(&plan)?;
    claim_plan(client, frozen.id());
    result["retainedPlan"] = json!(frozen.folder());
    frozen.keep_result(&result)?;
    Ok(result)
}

/// The bytes of the file given at `given`, read once more as given and
/// where it leads: a link changed, or a file made, since the first check
/// counts too.
fn read_source(given: &Path) -> Result<Vec<u8>> {
    let source = fs::canonicalize(given)?;
    crate::credential_files::refuse([given, source.as_path()])?;
    atlas_core::own_folder::refuse(&source)?;
    read_limited(&source, MAX_FILE)
}

/// [`crate::environment::apply`] on a VM: the plan frozen here as `plan_id`
/// sent whole to its guest, or sent again to resume it.
pub(crate) async fn apply_in(client: &Client, plan_id: &str) -> Result<Value> {
    validate_id(plan_id)?;
    let verified = verified(client).await?;
    apply_verified(client, &verified, plan_id).await
}

/// [`apply_in`] on the VM as `verified` proved it.
pub(in crate::vm) async fn apply_verified(
    client: &Client,
    verified: &Verified,
    plan_id: &str,
) -> Result<Value> {
    tidy_vm(client);
    let frozen = Frozen::of(client.output_dir()?, plan_id)?;
    if !frozen.exists() || (frozen.applied() && !frozen.has_payloads()) {
        return Err(problem(format!(
            "There is no plan {plan_id} to apply: it was applied completely, or never made. \
             To undo an applied plan, run atlas environment rollback {plan_id} --apply."
        )));
    }
    let lock = frozen.lock()?;
    if frozen.marked() {
        return Err(unfinished(client));
    }
    let uid = machine(client, verified)?.epoch.uid;
    let plan = frozen
        .plan()
        .filter(|plan| plan.uid == uid)
        .ok_or_else(|| problem("Frozen plan belongs to a different desktop or plan ID"))?;

    let ownership = client.controller.acquire(false)?;
    claim_plan(client, frozen.id());
    // Kept before its change is claimed or said, whatever then becomes of
    // this command: the plan is never forgotten for a guest's word that it
    // has no such transaction once an apply may have reached it. One that
    // cannot be kept sends nothing.
    let fresh = !frozen.applied();
    if fresh {
        frozen.keep_applied(false)?;
    }
    let change = Token::new();
    if let Err(error) = frozen.mark(&change) {
        if fresh {
            let _ = frozen.forget_applied();
        }
        return Err(error);
    }
    let mut result = send(
        client,
        verified,
        Request::Apply { plan },
        (&change, CHANGING),
    )
    .await;
    settle_marker(&frozen, &change, &result);
    match &mut result {
        Ok(answer) if answer["phase"].is_string() => {
            if let Err(error) = frozen.keep_applied(true) {
                answer["warnings"] = json!([format!(
                    "The apply was acknowledged, but Atlas could not mark its own record of it \
                     so; it is kept as one that may have applied: {error}"
                )]);
            }
        }
        // Nothing was applied, for certain: what this apply kept goes.
        Ok(answer) if fresh && changed_nothing(answer) => {
            let _ = frozen.forget_applied();
        }
        Err(error) if fresh && !error.uncertain => {
            let _ = frozen.forget_applied();
        }
        Ok(_) | Err(_) => {}
    }
    if settled(&result, Phase::Applied) {
        shed(client, verified, &frozen).await;
    }
    drop(ownership);
    drop(lock);
    operation(retained(result, &frozen), "apply")
}

/// [`crate::environment::rollback`] on a VM: what undoing `id` would do, or
/// its undo with `apply`, after which its plan goes. A guest that says it
/// has no such transaction never makes this Mac forget an apply that may
/// have reached it.
pub(crate) async fn rollback_in(client: &Client, id: &str, apply: bool) -> Result<Value> {
    validate_id(id)?;
    let verified = verified(client).await?;
    tidy_vm(client);
    let frozen = Frozen::of(client.output_dir()?, id)?;
    let (lock, created) = frozen.lock_or_create()?;
    if apply && frozen.marked() {
        return Err(unfinished(client));
    }

    let ownership = apply
        .then(|| client.controller.acquire(false))
        .transpose()?;
    let change = Token::new();
    if apply {
        frozen.mark(&change)?;
    }
    let request = Request::Rollback {
        id: id.to_owned(),
        apply,
    };
    let within = if apply { CHANGING } else { RUN_TIMEOUT };
    let result = send(client, &verified, request, (&change, within)).await;
    if apply {
        settle_marker(&frozen, &change, &result);
    }
    if apply && settled(&result, Phase::RolledBack) {
        discard_vm(client, &verified, &frozen).await;
    } else if created && !frozen.marked() {
        // Made only to hold its lock: it keeps nothing else.
        frozen.discard();
    }
    drop(ownership);
    drop(lock);
    operation(
        retained(result, &frozen),
        if apply { "rollback" } else { "rollbackPreview" },
    )
}

/// [`crate::environment::list()`] on a VM: the plans frozen here and the
/// guest's own transactions, labelled. While the guest, or this Mac, holds
/// agents, the plans here are listed with the guest's word unknown.
pub(crate) async fn list_in(client: &Client, all: bool) -> Result<Value> {
    tidy_vm(client);
    let here: Vec<Listed> = Frozen::all(client.output_dir()?)
        .iter()
        .filter_map(Frozen::listed)
        .collect();
    let there = match verified(client).await {
        Ok(verified) => transactions(client, &verified, all, &here).await,
        Err(error) => Err(error),
    };
    match there {
        Ok(there) => labelled(client, &here, &there, all),
        Err(held) if held.code == Code::Stopped => {
            let unknown: Vec<Listed> = here.into_iter().map(Listed::unknown).collect();
            let none = Transactions {
                ok: true,
                transactions: Vec::new(),
                hidden: 0,
                more: 0,
                unreadable: 0,
            };
            let mut listed = labelled(client, &unknown, &none, all)?;
            listed["transactions"] = json!("unknown");
            listed["why"] = json!(held.error);
            Ok(listed)
        }
        Err(error) => Err(error),
    }
}

/// The guest's own list of its transactions, with those rolled back when
/// `all`, and whatever it has of the newest plans in `here`.
async fn transactions(
    client: &Client,
    verified: &Verified,
    all: bool,
    here: &[Listed],
) -> Result<Transactions> {
    let mut newest: Vec<&Listed> = here.iter().collect();
    newest.sort_by_key(|plan| std::cmp::Reverse(plan.updated));
    let ids = newest
        .into_iter()
        .take(ASKED)
        .map(|plan| plan.id.clone())
        .collect();
    ask(client, verified, all, ids).await
}

/// The guest's own list of its transactions, with those rolled back when
/// `all`, and whatever it has of each of `ids` whatever its phase and age.
async fn ask(
    client: &Client,
    verified: &Verified,
    all: bool,
    ids: Vec<String>,
) -> Result<Transactions> {
    let malformed = "The VM's list of its copies was malformed";
    let answer = send(
        client,
        verified,
        Request::Transactions { all, ids },
        (&Token::new(), RUN_TIMEOUT),
    )
    .await?;
    let answer = one_off::read(answer, "transactions", malformed)?;
    serde_json::from_value(answer)
        .map_err(|cause| Error::new(Code::Protocol, malformed).with_source(cause))
}

/// [`crate::environment::run`] on a VM, once its arguments are valid: the
/// command run by the guest's worker with the profile it keeps, stopped
/// after `timeout`, under control held while it runs. Its reply is the
/// command's own.
pub(crate) async fn run_in(client: &Client, argv: Vec<String>, timeout: Duration) -> Result<Value> {
    let verified = verified(client).await?;
    let ownership = client.controller.acquire(false)?;
    let ran = send(
        client,
        &verified,
        Request::Run { argv },
        (&Token::new(), timeout),
    )
    .await;
    drop(ownership);
    ran.map_err(|error| without_profile(client, error))
}

/// `error`, named for the VM `client` acts on when it is the refusal of a
/// command because no profile is installed there: its code kept, which
/// callers fall back on.
fn without_profile(client: &Client, error: Error) -> Error {
    if error.code != NO_PROFILE {
        return error;
    }
    Error::new(
        NO_PROFILE,
        format!(
            "No environment profile is installed in {}. Use atlas run instead, or run atlas \
             environment profile to set one up.",
            client.selected.name
        ),
    )
}

/// The guest's answer to `request`, the change `change`, its worker stopped
/// after `timeout`, in the generation of `verified`: the worker's own answer
/// for a request of its records, or a command's own reply. One the guest
/// refused is its refusal; one whose outcome is unknown names its change,
/// pending until the guest answers it.
async fn send(
    client: &Client,
    verified: &Verified,
    request: Request,
    (change, timeout): (&Token, Duration),
) -> Result<Value> {
    let generation = client.generation_offering(verified, ENVIRONMENT, older)?;
    let (packet, apply) = packet_of(client, verified, request)?;
    let mutation = Mutation::Environment {
        packet: &packet,
        apply,
        timeout,
        pin: verified.pin()?,
        change,
    };
    if !fits(&packet, verified, change)? {
        return Err(too_large(client));
    }
    let named = |error: Error| {
        if error.uncertain {
            error.with("pendingChange", json!(change))
        } else {
            error
        }
    };
    let reply = client
        .mutate_vm(&mutation, generation, None)
        .await
        .map_err(named)?;
    if reply["ok"] != true {
        let mut refused = atlas_core::transport::refusal(&reply);
        if let Some(next) = reply.get("next") {
            refused = refused.with("next", next.clone());
        }
        return Err(named(refused));
    }
    if packet["request"]["operation"] == "run" {
        return Ok(reply);
    }
    Ok(answer(&reply, apply))
}

/// The packet `request` is handed to the worker of `verified`'s guest in,
/// and whether it may change the guest.
fn packet_of(client: &Client, verified: &Verified, request: Request) -> Result<(Value, bool)> {
    let machine = machine(client, verified)?;
    let apply = request.changes_helper();
    let packet = Packet {
        version: 1,
        expected: Identity::from_machine(&machine.epoch),
        request,
        refusal: None,
    };
    Ok((serde_json::to_value(packet)?, apply))
}

/// Whether `packet`, sent as the change `change` to `verified`'s guest,
/// fits in one request with what is said around it.
fn fits(packet: &Value, verified: &Verified, change: &Token) -> Result<bool> {
    let mutation = Mutation::Environment {
        packet,
        apply: true,
        timeout: crate::client::RUN_TIMEOUT_MAX,
        pin: verified.pin()?,
        change,
    };
    let mut request = mutation.request();
    request["change"] = json!(change);
    Ok(serde_json::to_vec(&request)?.len() + ENVELOPE <= MAX_REQUEST)
}

/// The refusal of a plan too large to send the VM `client` acts on.
fn too_large(client: &Client) -> Error {
    problem(format!(
        "This plan is too large to send to {} in one request; select fewer files and try again",
        client.selected.name
    ))
}

/// The worker's answer in the guest's `reply`, as an answer from the
/// helper's worker reads: a failed change that was not the worker's own
/// refusal (which changes nothing) may have stopped part way, so it says
/// it must be resumed.
fn answer(reply: &Value, mutating: bool) -> Value {
    let mut answer = reply["result"].clone();
    let refused = reply["exitCode"] == WORKER_REFUSED && answer["refusal"].is_string();
    if mutating && answer["ok"] == false && !refused {
        answer["resumeRequired"] = json!(true);
    }
    answer
}

/// Whether `answer`, the worker's to an apply, is its own refusal, which
/// changes nothing: a failure naming no transaction, and none to resume.
fn changed_nothing(answer: &Value) -> bool {
    answer["ok"] == false && answer.get("phase").is_none() && answer.get("resumeRequired").is_none()
}

/// The VM's session as `verified` proved it.
fn machine<'a>(client: &Client, verified: &'a Verified) -> Result<&'a crate::client::Pinned> {
    verified.machine().ok_or_else(|| client.unreachable())
}

/// The refusal of a step on a plan one of whose changes has an outcome not
/// known yet.
fn unfinished(client: &Client) -> Error {
    let name = &client.selected.name;
    problem(format!(
        "A change to this plan has an outcome Atlas does not know yet. It stays pending until \
         {name} answers what it came to; atlas --desktop {name} status shows it."
    ))
}

/// Drop `frozen`'s marker of `change` once `result` says what it came to:
/// kept while it is unknown, so its claim, pending, is its plan's too.
fn settle_marker(frozen: &Frozen, change: &Token, result: &Result<Value>) {
    if !result.as_ref().is_err_and(|error| error.uncertain) {
        frozen.unmark(change);
    }
}

/// Note that the command's task made the plan `id`. A claim that cannot be
/// written changes nothing: the plan is still listed.
fn claim_plan(client: &Client, id: &Token) {
    let _ =
        task::Claims::of(client).claim(&task::Owned::Plan { id: id.clone() }, task::State::Made);
}

/// Name what `frozen` still holds: a frozen plan, or only a change whose
/// outcome is unknown, or nothing once it is gone.
fn retained(mut result: Result<Value>, frozen: &Frozen) -> Result<Value> {
    let kept = frozen.exists();
    let plan = kept && frozen.planned();
    let (id, folder) = (frozen.id().as_str(), frozen.folder());
    match &mut result {
        Ok(value) => {
            value["retainedPlan"] = if plan { json!(folder) } else { Value::Null };
            if kept && !plan {
                value["retainedRequest"] = json!(folder);
            }
            value["planId"] = json!(id);
        }
        Err(error) if plan => error.error.push_str(&format!(
            " Frozen plan {id} retained at {}.",
            folder.display()
        )),
        Err(error) if kept => error
            .error
            .push_str(&format!(" Retained request: {}.", folder.display())),
        Err(_) => {}
    }
    result
}

/// Drop the markers of changes whose claims are no longer pending, each
/// plan looked at under its own lock (one in use is skipped), and what a
/// discard cut short left. Nothing is dropped while the pending changes
/// cannot be read.
fn tidy_vm(client: &Client) {
    let Ok(records) = client.output_dir() else {
        return;
    };
    let pending = || client.controller.pending_within(Duration::from_millis(500));
    let Ok(before) = pending() else {
        return;
    };
    for frozen in Frozen::all(records) {
        let Some(markers) = frozen.markers() else {
            continue;
        };
        if markers
            .iter()
            .all(|change| before.iter().any(|claim| claim == change.as_str()))
        {
            continue;
        }
        let Ok(lock) = frozen.try_lock() else {
            continue;
        };
        // Read again under the lock: a change marked and claimed since is
        // its command's own until it lets go of the plan.
        if let (Some(markers), Ok(pending)) = (frozen.markers(), pending()) {
            for change in markers {
                if !pending.iter().any(|claim| claim == change.as_str()) {
                    frozen.unmark(&change);
                }
            }
        }
        drop(lock);
    }
    frozen::clear_discarded(records);
}

/// [`tidy_vm`], then remove what earlier commands left that nothing can
/// use: previews cut short, and frozen plans older than [`UNUSED`] the
/// guest never started. A plan with a change whose outcome is unknown, or
/// one an apply may have reached, stays. A plan removed is settled gone wherever
/// a task claimed it.
async fn prune_vm(client: &Client, verified: &Verified) {
    tidy_vm(client);
    let Ok(records) = client.output_dir() else {
        return;
    };
    let mut removed = Vec::new();
    for frozen in Frozen::all(records) {
        let Some(age) = frozen.age() else {
            continue;
        };
        let planned = frozen.planned();
        if age <= if planned { UNUSED } else { STARTING } {
            continue;
        }
        let Ok(lock) = frozen.try_lock() else {
            continue;
        };
        if frozen.marked() || frozen.applied() {
            continue;
        }
        if planned {
            // A read-only question: has the guest ever started this plan?
            let request = Request::Rollback {
                id: frozen.id().as_str().to_owned(),
                apply: false,
            };
            let answer = send(client, verified, request, (&Token::new(), RUN_TIMEOUT)).await;
            if !answer.is_ok_and(|answer| WorkerReply::read(&answer).never_started()) {
                continue;
            }
        }
        if discard_vm(client, verified, &frozen).await {
            removed.push(task::Owned::Plan {
                id: frozen.id().clone(),
            });
        }
        drop(lock);
    }
    task::settle_gone(client, &removed);
}

/// Discard `frozen`, held under its lock: its guest copy first, then its
/// folder. Whether it is gone: one with a change whose outcome is unknown,
/// or whose copy's removal is not known to have happened, stays.
async fn discard_vm(client: &Client, verified: &Verified, frozen: &Frozen) -> bool {
    !frozen.marked() && shed(client, verified, frozen).await && frozen.discard()
}

/// Remove `frozen`'s guest copy and its payloads, which an applied plan no
/// longer needs: whether both are gone.
async fn shed(client: &Client, verified: &Verified, frozen: &Frozen) -> bool {
    let copy = match frozen.copy() {
        Copy::None => return frozen.forget_copy().is_ok(),
        Copy::Unreadable => return false,
        Copy::Kept(copy) => copy,
    };
    if !removed(client, verified, &copy).await {
        return false;
    }
    let owners = match client.output_dir() {
        Ok(records) => crate::vm::copies::Owners::in_records(records),
        Err(_) => return false,
    };
    owners.forget(&copy).is_ok() && frozen.forget_copy().is_ok()
}

/// Whether the guest's copy `copy` is gone: removed now, as a change of its
/// own while it is still the folder the guest lists, or already gone.
async fn removed(client: &Client, verified: &Verified, copy: &Token) -> bool {
    let listed = match crate::vm::copies::listed(client, copy).await {
        Ok(Some(listed)) => listed,
        Ok(None) => return true,
        Err(_) => return false,
    };
    let Some(node) = listed.node.as_deref().filter(|_| listed.state == "placed") else {
        return false;
    };
    match crate::vm::copies::remove(client, copy, node, verified).await {
        Ok(reply) if reply["ok"] == true => {
            matches!(reply["outcome"].as_str(), Some("removed" | "alreadyGone"))
        }
        Ok(_) | Err(_) => false,
    }
}
