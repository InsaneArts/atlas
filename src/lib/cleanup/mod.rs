//! `atlas cleanup`: undo what one session's tasks made in the helper, in an
//! order that keeps every step safe, with a receipt on disk that a later
//! run finishes from. A session or task key only chooses which records are
//! looked at: every removal checks the thing itself, its label, that
//! nothing runs from it, and that it is still what Atlas made, and moves
//! it aside and checks it again there before it deletes it. Apps opened by
//! name are listed, never quit.
mod live;
pub(crate) mod plan;
pub(crate) mod receipt;
mod reply;
pub(crate) mod run;
pub(crate) mod scope;
mod targets;
#[cfg(test)]
mod tests;

use crate::{
    client::{Actor, Client},
    control::Token,
    environment::PlanState,
    error::Result,
    handoff,
    host_session::{Key, now_ms, store::Store as Sessions},
    task::{Owned, State, Task, record::Adoption, store::Store as Tasks},
};
use plan::{Bundle, Step, merge};
use receipt::{Entry, Receipt, Receipts, StepState};
use reply::{described, said_state};
use run::Hands;
use scope::{Adopter, Loaded, Scope, may_adopt};
use serde_json::{Value, json};
use std::path::Path;
use targets::{install_in, targets};

/// See what a session's tasks left in the helper (`apply` false: read
/// only, without control), or clean it up. For an agent, another session's
/// work is cleaned up only once that session has ended.
pub async fn cleanup(
    client: &Client,
    session: Option<&Key>,
    task: Option<&Key>,
    apply: bool,
) -> Result<Value> {
    let scope = Scope::of(client, session, task)?;
    if apply {
        apply_scope(client, &scope).await
    } else {
        preview_scope(client, &scope).await
    }
}

/// The person's cleanup of `session`'s work through Atlas.app: the
/// preview, with whether that session may still be working, or the
/// cleanup itself, which passes every proof an agent's does and is not
/// held back by the session looking alive.
pub async fn for_person(client: &Client, session: &Key, apply: bool) -> Result<Value> {
    let scope = Scope::for_person(session);
    if apply {
        return apply_scope(client, &scope).await;
    }
    let mut reply = preview_scope(client, &scope).await?;
    let output = client.output_dir()?;
    let loaded = Loaded::of(output, &scope)?;
    reply["active"] = json!(may_adopt(loaded.record.as_ref(), &loaded.tasks).is_err());
    Ok(reply)
}

async fn preview_scope(client: &Client, scope: &Scope) -> Result<Value> {
    let output = client.output_dir()?;
    let loaded = Loaded::of(output, scope)?;
    preview(&live::Live::reading(client), output, scope, &loaded).await
}

/// The preview of `scope`'s cleanup: the steps a run would take now, those
/// an unfinished receipt already took with how far each got, and what each
/// would act on. It reads only, takes no control and writes nothing.
pub(crate) async fn preview(
    hands: &impl Hands,
    output: &Path,
    scope: &Scope,
    loaded: &Loaded,
) -> Result<Value> {
    let receipt = loaded.unfinished(scope);
    let mut steps = receipt
        .as_ref()
        .map(|receipt| receipt.steps.clone())
        .unwrap_or_default();
    merge(&mut steps, &loaded.undoable());
    if steps.is_empty() {
        return Ok(reply::nothing(output, scope, loaded, true));
    }

    let to_run: Vec<&Entry> = steps
        .iter()
        .filter(|entry| !entry.state.finished())
        .collect();
    let looks_at_apps = to_run
        .iter()
        .any(|entry| matches!(entry.step, Step::Quit { .. } | Step::RemoveCopy { .. }));
    let apps = if looks_at_apps {
        hands.apps().await?
    } else {
        json!({"ok": true, "apps": []})
    };
    let plans: Vec<Token> = to_run
        .iter()
        .filter_map(|entry| match &entry.step {
            Step::UndoPlan { plan } => Some(plan.clone()),
            _ => None,
        })
        .collect();
    let states = hands.plan_states(&plans).await?;
    let bundles: Vec<Bundle> = to_run
        .iter()
        .flat_map(|entry| match &entry.step {
            Step::Quit { bundles } => bundles.clone(),
            Step::RemoveInstall { install, .. } => vec![Bundle::Install(install.clone())],
            _ => Vec::new(),
        })
        .collect();
    let installs = run::installs_answer(hands, &bundles).await?;

    let mut untraced: Vec<String> = Vec::new();
    let shown: Vec<Value> = steps
        .iter()
        .map(|entry| {
            let mut value = described(hands.copies(), entry);
            if entry.state != StepState::Pending {
                said_state(&mut value, entry);
            }
            if entry.state.finished() {
                return value;
            }
            match &entry.step {
                Step::Quit { bundles } => {
                    let found = targets(hands.copies(), &scope.session, bundles, &installs, &apps);
                    untraced.extend(found.untraced);
                    value["apps"] = found
                        .processes
                        .iter()
                        .map(|process| json!({"app": process.name, "pid": process.pid}))
                        .collect();
                }
                Step::RemoveInstall { install, .. } => {
                    if let Some(found) = install_in(&installs, install) {
                        value["installPhase"] = found["phase"].clone();
                        value["present"] = json!(found["verified"] == true);
                        value["unchanged"] = found["unchanged"].clone();
                        if !found["path"].is_null() {
                            value["path"] = found["path"].clone();
                        }
                    }
                }
                Step::UndoPlan { plan } => {
                    let state = plans
                        .iter()
                        .position(|id| id == plan)
                        .and_then(|place| states.get(place));
                    if let Some(PlanState { phase, targets }) = state {
                        value["planState"] = json!(phase);
                        value["targets"] = json!(targets);
                    }
                }
                Step::RemoveCopy { copy } => {
                    value["present"] = json!(handoff::copy_exists(hands.copies(), copy));
                }
            }
            value
        })
        .collect();

    let mut left = reply::left(loaded, receipt.as_ref());
    for app in untraced {
        if !left.iter().any(|known| known["app"] == app.as_str()) {
            left.push(json!({"app": app, "why": "runs from a copy macOS moved it to"}));
        }
    }
    Ok(json!({
        "ok": true,
        "preview": true,
        "session": reply::session_named(output, &scope.session),
        "task": reply::task_named(loaded, scope),
        "steps": shown,
        "left": left,
        "receipt": receipt.map(|receipt| receipt.id),
    }))
}

async fn apply_scope(client: &Client, scope: &Scope) -> Result<Value> {
    let output = client.output_dir()?;
    let agent_adopts = scope.adopting && client.actor == Actor::Agent;
    if agent_adopts {
        let loaded = Loaded::of(output, scope)?;
        may_adopt(loaded.record.as_ref(), &loaded.tasks)?;
    }
    // Held to the end, as close holds it, so no other open, close or
    // cleanup interleaves with one.
    let ownership = client.controller.acquire(false)?;
    // The session may have acted while this one waited for control: it is
    // judged again, on what its records say now.
    let loaded = Loaded::of(output, scope)?;
    if agent_adopts {
        may_adopt(loaded.record.as_ref(), &loaded.tasks)?;
    }
    let adopter = scope.adopting.then(|| match &client.host_session {
        Some(host) if client.actor == Actor::Agent => Adopter::Session(host.key().clone()),
        _ => Adopter::Person,
    });
    apply(
        &live::Live::acting(client, &ownership),
        output,
        scope,
        &loaded,
        client.actor,
        adopter,
    )
    .await
}

/// Clean up `scope`'s work: settle what earlier cleanups finished, then
/// finish the scope's unfinished receipt, or start one, with a step for
/// everything in scope no step covers yet, and run the steps not yet done
/// or kept. `adopter` is who takes another session's work over; none when
/// the session cleans up its own. The caller holds control.
pub(crate) async fn apply(
    hands: &impl Hands,
    output: &Path,
    scope: &Scope,
    loaded: &Loaded,
    actor: Actor,
    adopter: Option<Adopter>,
) -> Result<Value> {
    let receipts = Receipts::at(output);
    let tasks = Tasks::at(output);
    let sessions = Sessions::at(output);
    let covered = loaded.covered(scope);
    reconcile(&receipts, &tasks, &sessions, &covered, loaded);

    let mut receipt = loaded
        .unfinished(scope)
        .unwrap_or_else(|| Receipt::new(scope.session.clone(), scope.task.clone(), actor));
    merge(&mut receipt.steps, &loaded.undoable());
    if receipt.steps.is_empty() {
        return Ok(reply::nothing(output, scope, loaded, false));
    }
    receipt.actor = actor;
    if let Some(adopter) = &adopter {
        receipt.adopted_by = adopter.key();
    }

    receipts.save(&mut receipt)?;
    receipts.prune(&receipt.id, |session| {
        sessions.read(session).is_some() || !tasks.of_session(session).is_empty()
    });
    let adoption = adopter.map(|adopter| Adoption {
        key: adopter.key(),
        at: now_ms(),
    });
    note(&tasks, &covered, &receipt, adoption.as_ref())?;

    run::run(
        hands,
        &mut receipt,
        &scope.session,
        |receipt| receipts.save(receipt),
        |step, state| settle(&tasks, &sessions, &covered, step, state),
    )
    .await?;
    receipts.save(&mut receipt)?;
    let _ = note(&tasks, &covered, &receipt, None);

    Ok(reply::applied(
        hands.copies(),
        output,
        scope,
        loaded,
        &receipt,
    ))
}

/// Settle, in the task records, every step of the session's receipts that
/// finished but whose settling did not take, as after a crash between the
/// two: what a receipt says was removed, undone or kept is never planned
/// again, and its claim says so too.
fn reconcile(
    receipts: &Receipts,
    tasks: &Tasks,
    sessions: &Sessions,
    covered: &[Task],
    loaded: &Loaded,
) {
    for receipt in &loaded.receipts {
        let mut receipt = receipt.clone();
        let mut changed = false;
        for entry in receipt
            .steps
            .iter_mut()
            .filter(|entry| entry.state.finished() && !entry.settled)
        {
            entry.settled = settle(tasks, sessions, covered, &entry.step, entry.state);
            changed |= entry.settled;
        }
        if changed {
            let _ = receipts.save(&mut receipt);
        }
    }
}

/// Note `receipt`, as it stands, in each of `covered`'s records, and who
/// adopted their work when someone did.
fn note(
    tasks: &Tasks,
    covered: &[Task],
    receipt: &Receipt,
    adoption: Option<&Adoption>,
) -> Result<()> {
    let now = now_ms();
    for task in covered {
        tasks.update(task, now, |record| {
            record.noted_cleanup(&receipt.id, receipt.state, now);
            if let Some(adoption) = adoption {
                record.adopted_by = Some(adoption.clone());
            }
            Ok(())
        })?;
    }
    Ok(())
}

/// Note what a step that ended `state` did to its thing: gone wherever a
/// record claims it, once removed or undone; kept, in the covered tasks'
/// records, once left on purpose. A plan undone leaves the session records
/// too. Whether every record that must be told was told: one that was not
/// is told again before anything is planned again.
fn settle(
    tasks: &Tasks,
    sessions: &Sessions,
    covered: &[Task],
    step: &Step,
    state: StepState,
) -> bool {
    let Some(owned) = step.owned() else {
        return true;
    };
    let now = now_ms();
    let told = match state {
        StepState::Done => tasks
            .settle_everywhere(std::slice::from_ref(&owned), State::Gone, now)
            .is_ok(),
        StepState::Kept => covered.iter().all(|task| {
            tasks
                .update(task, now, |record| {
                    record.settle(&owned, State::Kept, now);
                    Ok(())
                })
                .is_ok()
        }),
        _ => return false,
    };
    let forgotten = match &owned {
        Owned::Plan { id } => sessions.forget_plan(id).is_ok(),
        _ => true,
    };
    told && forgotten
}
