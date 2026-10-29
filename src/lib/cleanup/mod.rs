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
    task::{
        Owned, State, Task,
        record::{Adoption, Record as TaskRecord},
        store::Store as Tasks,
    },
};
use plan::{Bundle, Step, merge_with};
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
    merge_with(&mut steps, &loaded.undoable(), &loaded.others());
    if steps.is_empty() {
        let running = running_named(hands, loaded, None).await;
        return Ok(reply::nothing(
            output,
            scope,
            loaded,
            true,
            running.as_ref(),
        ));
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
    let mut jobs = Vec::new();
    for entry in &to_run {
        if let Step::StopJob { job, .. } = &entry.step {
            let state = match hands.job(job).await {
                Ok(shown) => shown["state"].clone(),
                Err(_) => json!("unknown"),
            };
            jobs.push((job.clone(), state));
        }
    }

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
                Step::CheckOutputs { outputs } => {
                    let found: Vec<_> = outputs.iter().map(|output| hands.intact(output)).collect();
                    value["intact"] = json!(found.iter().all(crate::collect::Intact::whole));
                }
                Step::StopJob { job, .. } => {
                    if let Some((_, state)) = jobs.iter().find(|(known, _)| known == job) {
                        value["jobState"] = state.clone();
                    }
                }
            }
            value
        })
        .collect();

    let running = running_named(hands, loaded, looks_at_apps.then_some(&apps)).await;
    let mut left = reply::left(loaded, receipt.as_ref(), running.as_ref());
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
    reconcile(&receipts, &tasks, &sessions, loaded);

    let mut receipt = loaded
        .unfinished(scope)
        .unwrap_or_else(|| Receipt::new(scope.session.clone(), scope.task.clone(), actor));
    merge_with(&mut receipt.steps, &loaded.undoable(), &loaded.others());
    if receipt.steps.is_empty() {
        let running = running_named(hands, loaded, None).await;
        return Ok(reply::nothing(
            output,
            scope,
            loaded,
            false,
            running.as_ref(),
        ));
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

    let own_scope = (scope.session.clone(), scope.task.clone());
    run::run(
        hands,
        &mut receipt,
        &scope.session,
        |receipt| receipts.save(receipt),
        |step, state| settle(&tasks, &sessions, &own_scope, step, state),
    )
    .await?;
    receipts.save(&mut receipt)?;
    let _ = note(&tasks, &covered, &receipt, None);

    let running = running_named(hands, loaded, None).await;
    Ok(reply::applied(
        hands.copies(),
        output,
        scope,
        loaded,
        &receipt,
        running.as_ref(),
    ))
}

/// The helper's app list, read only when the scope's tasks opened an app by
/// name, so only those still running are left to the agent: `listed` when
/// one was just read, else a fresh one. None when there is none to read or
/// it cannot be read, and each such app is then left, as it may still run.
async fn running_named(
    hands: &impl Hands,
    loaded: &Loaded,
    listed: Option<&Value>,
) -> Option<Value> {
    if loaded.opened_by_name().is_empty() {
        return None;
    }
    match listed {
        Some(listed) => Some(listed.clone()),
        None => hands.apps().await.ok().filter(|apps| apps["ok"] == true),
    }
}

/// Settle every step of the session's receipts that finished but whose
/// settling was not acknowledged, as after a crash between the two, each
/// in the records of its own receipt's scope: what a receipt says was
/// moved aside, removed, undone or kept is never planned again, and its
/// claims say so too.
fn reconcile(receipts: &Receipts, tasks: &Tasks, sessions: &Sessions, loaded: &Loaded) {
    for receipt in &loaded.receipts {
        let mut receipt = receipt.clone();
        let mut changed = false;
        let scope = (receipt.session.clone(), receipt.task.clone());
        for entry in receipt
            .steps
            .iter_mut()
            .filter(|entry| entry.state.finished() && !entry.settled)
        {
            entry.settled = settle(tasks, sessions, &scope, &entry.step, entry.state);
            changed |= entry.settled;
        }
        if changed {
            let _ = receipts.save(&mut receipt);
        }
    }
}

/// Note what a step that ended `state` did to its thing, in the records of
/// the receipt's own scope (`scope`: its session, and its one task when it
/// covers one): gone wherever any record claims it, once moved aside,
/// removed or undone; kept, in each record of that scope that claims it,
/// once left on purpose. A plan undone leaves the session records too.
/// Whether every record that must say so does, read back after writing:
/// a step not acknowledged is settled again before anything is planned.
fn settle(
    tasks: &Tasks,
    sessions: &Sessions,
    scope: &(Key, Option<Key>),
    step: &Step,
    state: StepState,
) -> bool {
    let Some(owned) = step.owned() else {
        return true;
    };
    let now = now_ms();
    let (session, task) = scope;
    let claiming = |records: Vec<TaskRecord>| -> Vec<TaskRecord> {
        records
            .into_iter()
            .filter(|record| task.as_ref().is_none_or(|task| record.key == *task))
            .filter(|record| record.claims().any(|(claimed, _)| claimed.same(&owned)))
            .collect()
    };
    let state_of = |record: &TaskRecord| {
        record
            .claims()
            .find(|(claimed, _)| claimed.same(&owned))
            .map(|(_, state)| state)
    };

    let acknowledged = match state {
        StepState::Done => {
            tasks
                .settle_everywhere(std::slice::from_ref(&owned), State::Gone, now)
                .is_ok()
                && tasks
                    .all()
                    .iter()
                    .all(|record| !matches!(state_of(record), Some(State::Intent | State::Made)))
        }
        StepState::Kept => {
            for record in claiming(tasks.of_session(session)) {
                let _ = tasks.update(&Task::recorded(&record), now, |record| {
                    record.settle(&owned, State::Kept, now);
                    Ok(())
                });
            }
            claiming(tasks.of_session(session))
                .iter()
                .all(|record| state_of(record) == Some(State::Kept))
        }
        _ => return false,
    };
    let forgotten = match &owned {
        Owned::Plan { id } => sessions.forget_plan(id).is_ok(),
        _ => true,
    };
    acknowledged && forgotten
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
