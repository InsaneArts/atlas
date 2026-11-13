//! What a cleanup needs of the copies into the helper: how far each of a
//! task's plans got, read without control, and undoing one under the
//! control the cleanup holds. Every plan is looked at by its own ID only;
//! nothing here lists the helper's store or the plans beside the shared
//! command for anything else.
use super::{model::*, reply::WorkerReply, *};
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
    pub(crate) fn started(phase: &Lenient<Phase>) -> Option<Self> {
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
    match client.desktop().kind() {
        Kind::User(login) => crate::user::environment::plan_states_in(client, login, ids).await,
    }
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
    match client.desktop().kind() {
        Kind::User(login) => {
            crate::user::environment::undo_plan_in(client, login, ownership, id).await
        }
    }
}

/// What a rollback `reply` that reached rolledBack left: nothing, or the
/// files the helper changed since and the folders it could not remove. One
/// rolled back before says the same as one rolled back now.
pub(crate) fn rolled_back(reply: WorkerReply) -> Undone {
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
