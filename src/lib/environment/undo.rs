//! What a cleanup needs of the copies into the helper: how far each of a
//! task's plans got, read without control, and undoing one under the
//! control the cleanup holds. Every plan is looked at by its own ID only;
//! nothing here lists the helper's store or the plans beside the shared
//! command for anything else.
use super::{list::frozen, model::*, *};
use crate::control::Token;

/// How far a plan got, as a cleanup reads it before it undoes anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PlanPhase {
    /// Frozen here; the helper never started it.
    Previewed,
    Applying,
    Applied,
    RollingBack,
    RolledBack,
    /// A command holds it now.
    Running,
    /// A request of it has an outcome nobody knows, or the helper named a
    /// phase this build does not know: never taken for finished.
    Unknown,
    /// Nothing of it, here or in the helper.
    Gone,
}

/// How far a plan got, and what it copies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlanState {
    pub phase: PlanPhase,
    /// The targets it copies into the helper, relative to its home.
    pub targets: Vec<String>,
}

/// How far each of `ids` got: the helper's word for those it started, and
/// the frozen plans beside the shared command for those it never did,
/// asked in as many reads as the helper's limit needs. Reads only: it
/// takes no control and records nothing.
pub(crate) async fn plan_states(client: &Client, ids: &[Token]) -> Result<Vec<PlanState>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let helper = helper_name(client.config.helper_uid)?;
    let shared = client
        .config
        .identity_probe
        .parent()
        .ok_or_else(|| problem("Missing shared directory"))?;
    let here = frozen(shared, client.config.helper_uid);
    let mut transactions: Vec<Value> = Vec::new();
    for batch in ids.chunks(list::ASKED) {
        let asked: Vec<String> = batch.iter().map(|id| id.as_str().to_owned()).collect();
        let there = list::ask(client, &helper, true, asked).await?;
        transactions.extend(
            there["transactions"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
        );
    }

    Ok(ids
        .iter()
        .map(|id| {
            let plan = here.iter().find(|plan| plan.id() == id.as_str());
            let running = plan.is_some_and(list::Frozen::running);
            let started = transactions
                .iter()
                .find(|transaction| transaction["planId"] == id.as_str());
            match (started, plan) {
                (Some(transaction), _) => PlanState {
                    phase: if running {
                        PlanPhase::Running
                    } else {
                        phase_of(&transaction["phase"])
                    },
                    targets: targets(&transaction["files"]),
                },
                (None, Some(plan)) => PlanState {
                    phase: if running {
                        PlanPhase::Running
                    } else if plan.request() {
                        PlanPhase::Unknown
                    } else {
                        PlanPhase::Previewed
                    },
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

/// A helper phase as a cleanup reads it; one it does not know reads as
/// unknown, never as finished.
fn phase_of(phase: &Value) -> PlanPhase {
    match phase.as_str() {
        Some("applying") => PlanPhase::Applying,
        Some("applied") => PlanPhase::Applied,
        Some("rollingBack") => PlanPhase::RollingBack,
        Some("rolledBack") => PlanPhase::RolledBack,
        _ => PlanPhase::Unknown,
    }
}

/// The targets a helper's file report names.
fn targets(files: &Value) -> Vec<String> {
    files
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|file| file["target"].as_str().map(str::to_owned))
        .collect()
}

/// What undoing one plan for a cleanup came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Undone {
    /// Nothing of it was there, here or in the helper.
    AlreadyGone,
    /// The helper never started it; its frozen plan here was discarded.
    Discarded,
    /// Rolled back completely, now or before.
    RolledBack,
    /// Rolled back, leaving the files the helper changed since, and the
    /// folders it could not remove, as they are.
    Kept {
        targets: Vec<String>,
        directories: usize,
    },
    /// The helper could not finish it, and said why; trying again may.
    Unfinished(String),
}

/// Undo plan `id` under the control the cleanup holds: ask the helper,
/// without changing anything, how far it got, then roll it back when it
/// was started and not yet undone, or discard its frozen plan when it was
/// never started. Only this one plan is looked at. An error whose outcome
/// is not known comes back uncertain, its request kept as for any change.
pub(crate) async fn undo_plan(
    client: &Client,
    ownership: &Ownership,
    id: &Token,
) -> Result<Undone> {
    let helper = helper_name(client.config.helper_uid)?;
    let stage = stage_path(client, id.as_str())?;
    let created = directory(&stage, false)?.is_none();
    let lock = if created {
        create_locked_stage(&stage, &helper)?
    } else {
        lock_stage(&stage)?
    };

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
    let preview = dispatch_owned(client, ownership, helper, stage, ask(false)).await?;

    if preview["ok"] == false && preview["error"] == worker::MISSING {
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
    if preview["ok"] != true {
        return Ok(Undone::Unfinished(said(&preview)));
    }
    let reply = if preview["phase"] == "rolledBack" {
        preview
    } else {
        let reply = dispatch_owned(client, ownership, helper, stage, ask(true)).await?;
        if !settled(&Ok(reply.clone()), "rolledBack") {
            return Ok(Undone::Unfinished(said(&reply)));
        }
        reply
    };
    if !requests(stage) && !discarded(stage) {
        return Ok(Undone::Unfinished(UNREMOVED.into()));
    }
    Ok(rolled_back(&reply))
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

/// What a rollback `reply` that reached rolledBack left: nothing, or the
/// files the helper changed since and the folders it could not remove. One
/// rolled back before says the same as one rolled back now.
fn rolled_back(reply: &Value) -> Undone {
    let kept: Vec<String> = reply["keptTargets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|target| target.as_str().map(str::to_owned))
        .collect();
    let directories = reply["retainedDirectories"].as_array().map_or(0, Vec::len);
    if kept.is_empty() && directories == 0 {
        Undone::RolledBack
    } else {
        Undone::Kept {
            targets: kept,
            directories,
        }
    }
}

/// What the helper said of a request it did not finish.
fn said(reply: &Value) -> String {
    reply["error"]
        .as_str()
        .unwrap_or("The helper did not finish undoing this copy")
        .to_owned()
}
