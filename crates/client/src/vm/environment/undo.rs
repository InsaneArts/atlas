//! What a cleanup needs of a VM's plans: how far each got, read without
//! control, and undoing one under the control the cleanup holds. The
//! guest's word on its own transactions says how far it got; whether a plan
//! may have been applied, and so whether it may be forgotten, rests on this
//! Mac's own record beside it: a plan the guest no longer knows that an
//! apply may have reached is never discarded, and its cleanup says so.
use super::{
    CHANGING, Frozen, Listed, ask, discard_vm, settle_marker, tidy_vm, unfinished, verified,
};
use crate::{
    client::{Client, RUN_TIMEOUT, Verified},
    environment::{
        model::{Phase, Request},
        problem,
        reply::{TransactionSummary, WorkerReply},
        undo::{PlanState, Undone, rolled_back, states_of},
    },
};
use atlas_core::{error::Result, files::Token};

/// [`crate::environment::undo::plan_states`] on a VM: the guest's word on
/// the plans it started, asked in as many reads as its limit needs, and the
/// plans frozen here for those it never did.
///
/// # Errors
///
/// Fails when the VM cannot be proved or its list cannot be read, or when
/// that list holds a record the guest or this build cannot read: that
/// record may be any of `ids`, so none of them is taken for gone or never
/// started.
pub(crate) async fn plan_states_in(client: &Client, ids: &[Token]) -> Result<Vec<PlanState>> {
    let verified = verified(client).await?;
    tidy_vm(client);
    let records = client.output_dir()?;
    let here: Vec<Listed> = ids
        .iter()
        .filter_map(|id| Frozen::of(records, id.as_str()).ok()?.listed())
        .collect();
    let mut transactions: Vec<TransactionSummary> = Vec::new();
    for batch in ids.chunks(super::ASKED) {
        let asked = batch.iter().map(|id| id.as_str().to_owned()).collect();
        let there = ask(client, &verified, true, asked).await?;
        if !there.all_read() {
            return Err(problem(format!(
                "Atlas could not read every record of {}'s copies, so it cannot tell what \
                 undoing one would remove",
                client.selected.name
            )));
        }
        transactions.extend(there.known().cloned());
    }
    Ok(states_of(ids, &here, &transactions))
}

/// [`crate::environment::undo::undo_plan`] of plan `id` on a VM, under the
/// control the cleanup holds: the guest asked, without changing anything,
/// how far it got, then the plan rolled back when it was started and not
/// yet undone, or discarded with its guest copy when it was never started.
/// One the guest never started that an apply may have reached is kept.
///
/// # Errors
///
/// Returns the failure to prove the VM, to lock the plan here or to ask the
/// guest; one whose outcome is not known comes back uncertain, its change
/// kept pending and marked beside the plan.
pub(crate) async fn undo_plan_in(client: &Client, id: &Token) -> Result<Undone> {
    let verified = verified(client).await?;
    tidy_vm(client);
    let frozen = Frozen::of(client.output_dir()?, id.as_str())?;
    let (lock, created) = frozen.lock_or_create()?;
    let undone = undo_locked(client, &verified, &frozen, created).await;
    // A folder made only to ask goes, unless it marks a change whose
    // outcome is not known.
    if created && frozen.exists() && !frozen.marked() {
        frozen.discard();
    }
    drop(lock);
    undone
}

async fn undo_locked(
    client: &Client,
    verified: &Verified,
    frozen: &Frozen,
    created: bool,
) -> Result<Undone> {
    if frozen.marked() {
        return Ok(Undone::Unfinished(unfinished(client).error));
    }
    let ask = |apply| Request::Rollback {
        id: frozen.id().as_str().to_owned(),
        apply,
    };
    let preview = super::send(client, verified, ask(false), (&Token::new(), RUN_TIMEOUT)).await?;
    let preview = WorkerReply::read(&preview);

    if preview.never_started() {
        if frozen.applied() {
            return Ok(Undone::Forgotten(forgotten(client)));
        }
        let planned = !created && frozen.planned();
        if !discard_vm(client, verified, frozen).await {
            return Ok(Undone::Unfinished(unremoved(client)));
        }
        return Ok(if planned {
            Undone::Discarded
        } else {
            Undone::AlreadyGone
        });
    }
    if preview.ok != Some(true) {
        return Ok(Undone::Unfinished(preview.error));
    }
    let reply = match preview.phase {
        // A plan rolled back is still undone again: a rollback cut short
        // after its phase was saved leaves it active in the guest, which only
        // an applying rollback lets go of.
        Some(Phase::Applying | Phase::Applied | Phase::RollingBack | Phase::RolledBack) => {
            let change = Token::new();
            frozen.mark(&change)?;
            let result = super::send(client, verified, ask(true), (&change, CHANGING)).await;
            settle_marker(frozen, &change, &result);
            let reply = WorkerReply::read(&result?);
            if !reply.settled(Phase::RolledBack) {
                return Ok(Undone::Unfinished(reply.error));
            }
            reply
        }
        None => {
            return Ok(Undone::Unfinished(format!(
                "{} reported an unknown transaction phase; no rollback was sent",
                client.selected.name
            )));
        }
    };
    if !discard_vm(client, verified, frozen).await {
        return Ok(Undone::Unfinished(unremoved(client)));
    }
    Ok(rolled_back(reply))
}

/// Why a plan is kept when the VM `client` acts on no longer knows it,
/// though an apply of it may have reached it.
fn forgotten(client: &Client) -> String {
    let name = &client.selected.name;
    format!(
        "{name} has no record of this copy, though it may have applied it, so Atlas undid \
         nothing and keeps its plan until {name} is removed"
    )
}

/// Why undoing a plan is not finished when what Atlas kept for it, here or
/// in the VM `client` acts on, would not go.
fn unremoved(client: &Client) -> String {
    format!(
        "Atlas could not remove what it kept for this copy into {}",
        client.selected.name
    )
}
