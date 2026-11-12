//! What a cleanup needs of the copies into the helper: how far each of a
//! task's plans got, read without control, and undoing one under the
//! control the cleanup holds. Every plan is looked at by its own ID only;
//! nothing here lists the helper's store or the plans beside the shared
//! command for anything else.
use super::{
    list::frozen,
    model::*,
    reply::{TransactionSummary, Transactions, WorkerReply},
    *,
};
use atlas_core::{files::Token, lenient::Lenient};

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

impl PlanPhase {
    /// How far a plan the helper started got, as its transaction's phase
    /// says; none for a phase this build does not know.
    pub(super) fn started(phase: &Lenient<Phase>) -> Option<Self> {
        match phase {
            Lenient::Known(Phase::Applying) => Some(Self::Applying),
            Lenient::Known(Phase::Applied) => Some(Self::Applied),
            Lenient::Known(Phase::RollingBack) => Some(Self::RollingBack),
            Lenient::Known(Phase::RolledBack) => Some(Self::RolledBack),
            Lenient::Later(_) => None,
        }
    }
}

/// How far a plan got, and what it copies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlanState {
    pub(crate) phase: PlanPhase,
    /// The targets it copies into the helper, relative to its home.
    pub(crate) targets: Vec<String>,
}

/// How far each of `ids` got: the helper's word for those it started, and
/// the frozen plans beside the shared command for those it never did,
/// asked in as many reads as the helper's limit needs. Reads only: it
/// takes no control and records nothing.
///
/// # Errors
///
/// Fails when the helper's list cannot be read, or when it holds a record
/// that the helper or this build cannot read: that record may be any of
/// `ids`, so none of them is taken for gone or never started.
pub(crate) async fn plan_states(client: &Client, ids: &[Token]) -> Result<Vec<PlanState>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let helper = helper_name(client.desktop().hidden().uid)?;
    let here = frozen(shared_folder(client)?, client.desktop().hidden().uid);
    let mut transactions: Vec<TransactionSummary> = Vec::new();
    for batch in ids.chunks(list::ASKED) {
        let asked: Vec<String> = batch.iter().map(|id| id.as_str().to_owned()).collect();
        let there: Transactions = list::ask(client, &helper, true, asked).await?;
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
    let helper = helper_name(client.desktop().hidden().uid)?;
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

pub(super) async fn undo_locked(
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
    Ok(rolled_back(reply))
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
fn rolled_back(reply: WorkerReply) -> Undone {
    if reply.kept_targets.is_empty() && reply.retained_directories == 0 {
        Undone::RolledBack
    } else {
        Undone::Kept {
            targets: reply.kept_targets,
            directories: reply.retained_directories,
        }
    }
}

#[cfg(test)]
#[path = "undo_tests.rs"]
mod tests;
