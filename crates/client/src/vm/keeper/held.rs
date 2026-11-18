//! The person's maintenance, as the keeper holds it: every request passes
//! here, so no agent's path goes around it. The keeper reads the VM's
//! lifecycle record at its start and again at every request; while a
//! maintenance is kept, ending or cannot be read, it refuses every agent's
//! request on every lane, in the maintenance's words. What maintenance and
//! recovery need is served all the same: hellos, a change's outcome, the
//! guest's word on its leftover work, the station's own settlement and
//! recovery, the person's Stop, and the maintenance's own `hold` and
//! `release`.
//!
//! `hold`, naming the maintenance kept, is taken under the keeper's turn: no
//! lane is admitted from then, every open one is cancelled, and it is
//! answered only once each ended with its credit back. `release` lets agents
//! in again only once the record says the maintenance ended, or keeps none.
//! A hold asked so stays until its release, should the record go first; one
//! the keeper took from the record alone goes with it.
use super::{
    Keeper,
    change::unsent,
    lanes::{Hold, Ticket, Work},
};
use crate::vm::maintenance::{self, Kept};
use atlas_core::{
    error::{Code, Error},
    files::Token,
};
use serde_json::{Value, json};
use std::time::Duration;

/// The keeper's capability to hold agents for the person's maintenance.
pub(super) const CAPABILITY: &str = "hold-v1";

/// What a VM is called in the keeper's words when its record does not say:
/// within a sentence, and at its start.
const UNNAMED: &str = "this VM";
const UNNAMED_FIRST: &str = "This VM";

/// Whether the read `request` is one the keeper carries while agents are
/// held: a hello, a change's outcome, or the guest's word on its leftover
/// work.
pub(crate) fn served_while_held(request: &Value) -> bool {
    matches!(
        request["op"].as_str(),
        Some("hello" | "outcome" | "leftovers")
    )
}

/// What the VM's lifecycle record says of its maintenance now, and its
/// name there.
async fn kept(keeper: &Keeper) -> (Kept, Option<String>) {
    let (lifecycle, vm) = (keeper.lifecycle.clone(), keeper.vm.clone());
    tokio::task::spawn_blocking(move || Kept::read(Some(&lifecycle), &vm))
        .await
        .unwrap_or((Kept::Unreadable, None))
}

/// Hold every lane now when the record keeps a maintenance, or cannot be
/// read: what a keeper does at its start, so none is admitted for as long
/// as the record keeps it. Taken from the record alone, the hold lets go
/// once the record keeps none, released or not: a keeper started while a
/// release was being finished is never left holding.
pub(super) async fn at_start(keeper: &Keeper) {
    let (kept, name) = kept(keeper).await;
    if kept != Kept::None {
        keeper.lanes.hold(Hold {
            change: kept
                .maintenance()
                .map(|maintenance| maintenance.change.clone()),
            name,
            asked: false,
        });
    }
}

/// Why an agent's lane stops, or is never admitted, while a hold keeps
/// every lane: in the words of the maintenance it names.
pub(super) fn held_now(keeper: &Keeper) -> Error {
    let name = keeper.lanes.held().and_then(|hold| hold.name);
    maintenance::on(name.as_deref().unwrap_or(UNNAMED_FIRST))
}

/// [`held_now`], said as a refusal certain that nothing more of its request
/// goes to the guest.
pub(super) fn unsent_now(keeper: &Keeper) -> Value {
    unsent(held_now(keeper))
}

/// A ticket for an agent's lane carrying `work`, read afresh against the
/// person's maintenance; the refusal to say in its place while it holds the
/// VM, certain that nothing of the request was sent.
pub(super) async fn admitted(keeper: &Keeper, work: Work) -> Result<Ticket<'_>, Value> {
    let (kept, name) = kept(keeper).await;
    if kept == Kept::None {
        keeper.lanes.settled();
    }
    if let Some(held) = kept.held(name.as_deref().unwrap_or(UNNAMED)) {
        return Err(unsent(held));
    }
    keeper
        .lanes
        .admit(work)
        .map_err(|_| unsent(held_now(keeper)))
}

/// The refusal of the person's Resume while their maintenance holds the
/// VM, read afresh under the keeper's turn: none while nothing holds it.
pub(super) async fn resume_refused(keeper: &Keeper) -> Option<Value> {
    let (kept, name) = kept(keeper).await;
    if kept == Kept::None {
        keeper.lanes.settled();
    }
    // Lanes the station held for a maintenance whose record went before its
    // release refuse it as one that is on.
    let refused = kept
        .resume_refused(name.as_deref().unwrap_or(UNNAMED))
        .or_else(|| {
            let name = name.or(keeper.lanes.held()?.name);
            Some(maintenance::resume_on(
                name.as_deref().unwrap_or(UNNAMED_FIRST),
            ))
        })?;
    Some(said(&refused))
}

/// Hold every agent's lane for the maintenance `request` names, under the
/// keeper's turn: none admitted from then, every open one cancelled, and
/// answered once each ended with its credit back, within `deadline`.
pub(super) async fn hold(keeper: &Keeper, request: &Value, deadline: Duration) -> Value {
    let Some(change) = change_of(request) else {
        return said(&Error::new(
            Code::BadRequest,
            "A hold names the maintenance it is for.",
        ));
    };
    let _turn = keeper.turn.lock().await;
    let (kept, name) = kept(keeper).await;
    if kept.maintenance().is_none_or(|kept| kept.change != change) {
        return said(&Error::new(
            Code::State,
            "No maintenance of this VM is kept under that change, so agents are not held for it.",
        ));
    }
    let told = keeper.lanes.hold(Hold {
        change: Some(change.clone()),
        name,
        asked: true,
    });
    if tokio::time::timeout(deadline, keeper.lanes.drained())
        .await
        .is_err()
    {
        return said(&Error::new(
            Code::NotReady,
            "Not every agent's lane on this VM ended in time; agents stay held.",
        ));
    }
    json!({"ok": true, "op": "hold", "change": change, "cancelled": told})
}

/// Let agents' lanes be admitted again once the maintenance `request`
/// names is recorded ended, or none is kept, under the keeper's turn.
pub(super) async fn release(keeper: &Keeper, request: &Value) -> Value {
    let Some(change) = change_of(request) else {
        return said(&Error::new(
            Code::BadRequest,
            "A release names the maintenance it is for.",
        ));
    };
    let _turn = keeper.turn.lock().await;
    let (kept, _) = kept(keeper).await;
    let ended = match &kept {
        Kept::None => true,
        Kept::On(kept) | Kept::Ending(kept) => kept.change == change && kept.ended.is_some(),
        Kept::Unreadable => false,
    };
    if !ended {
        return said(&Error::new(
            Code::State,
            "This VM's maintenance is not recorded ended, so agents stay held.",
        ));
    }
    keeper.lanes.release();
    json!({"ok": true, "op": "release", "change": change})
}

fn change_of(request: &Value) -> Option<Token> {
    Token::parse(request["change"].as_str()?)
}

fn said(error: &Error) -> Value {
    serde_json::to_value(error).unwrap_or_default()
}

#[cfg(test)]
#[path = "held_tests.rs"]
mod tests;
