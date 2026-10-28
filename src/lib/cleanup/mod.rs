//! `atlas cleanup`: undo what one session's tasks made in the helper, in an
//! order that keeps every step safe, with a receipt on disk that a later
//! run finishes from. A session or task key only chooses which records are
//! looked at: every removal checks the thing itself, its label, that
//! nothing runs from it, and that it is still what Atlas made. Apps opened
//! by name are listed, never quit.
mod live;
pub(crate) mod plan;
pub(crate) mod receipt;
pub(crate) mod run;
#[cfg(test)]
mod tests;

use crate::{
    client::{Actor, Client},
    control::Token,
    error::{Error, Result},
    handoff,
    host_session::{
        Key,
        names::{name_of, names},
        now_ms,
        record::Record as SessionRecord,
        store::Store as Sessions,
    },
    task::{
        self, Owned, State, Task,
        record::{Adoption, CleanupState, Record as TaskRecord},
        store::Store as Tasks,
    },
};
use plan::{Step, Undoable, merge};
use receipt::{Entry, Receipt, Receipts, StepState};
use run::{Hands, Shown, folders, running_from};
use serde_json::{Value, json};
use std::path::Path;

/// What one cleanup covers: one session's work, or one task's of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Scope {
    /// The session whose work it is.
    pub session: Key,
    /// The one task it covers; none for every task of the session.
    pub task: Option<Key>,
    /// Whether the work is another session's than the one cleaning up.
    pub adopting: bool,
    /// Whether the session was named by its key, so a key that names
    /// nothing is said, never taken for nothing to clean up.
    pub named: bool,
}

impl Scope {
    /// What a command of `client` covers: the session `session` names, or
    /// its own; the task `task` names, or the one ATLAS_TASK names for its
    /// own session, or every task.
    pub fn of(client: &Client, session: Option<&Key>, task: Option<&Key>) -> Result<Self> {
        let own = client.host_session.as_ref().map(|host| host.key());
        let session = session.or(own).cloned().ok_or_else(|| {
            Error::new(
                "usage",
                "Atlas cannot tell which session this is, so it has nothing to clean up. Set \
                 ATLAS_SESSION=NAME before the task's first command, or pass --session KEY \
                 (atlas activity --all lists sessions).",
            )
        })?;
        let own_task = client
            .task
            .as_ref()
            .filter(|own| own.name().is_some() && *own.session() == session)
            .map(|own| own.key().clone());
        Ok(Self {
            adopting: own != Some(&session),
            named: own != Some(&session) || task.is_some(),
            task: task.cloned().or(own_task),
            session,
        })
    }

    /// The person's cleanup of `session`'s work, through Atlas.app.
    pub fn for_person(session: &Key) -> Self {
        Self {
            session: session.clone(),
            task: None,
            adopting: true,
            named: true,
        }
    }
}

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
    reply["active"] = json!(
        Sessions::at(output)
            .read(session)
            .as_ref()
            .is_some_and(alive)
    );
    Ok(reply)
}

/// What a scope's records say: the session's record, its tasks' records,
/// and the work in scope.
pub(crate) struct Loaded {
    pub record: Option<SessionRecord>,
    pub tasks: Vec<TaskRecord>,
    pub work: Vec<task::Work>,
}

impl Loaded {
    /// The records of `scope`'s session in `output`; a key that names no
    /// record is said.
    pub fn of(output: &Path, scope: &Scope) -> Result<Self> {
        let record = Sessions::at(output).read(&scope.session);
        let tasks = Tasks::at(output).of_session(&scope.session);
        let nothing = || {
            Error::new(
                "input",
                "No session or task with that key is recorded here; atlas activity --all lists them",
            )
        };
        if scope.named && record.is_none() && tasks.is_empty() {
            return Err(nothing());
        }
        if let Some(task) = &scope.task
            && *task != scope.session
            && !tasks.iter().any(|record| record.key == *task)
        {
            return Err(nothing());
        }

        let work = task::work(record.as_ref(), &tasks)
            .into_iter()
            .filter(|work| scope.task.as_ref().is_none_or(|task| work.task == *task))
            .collect();
        Ok(Self {
            record,
            tasks,
            work,
        })
    }

    /// What a cleanup undoes: everything in scope that may still be in the
    /// helper, with when it was claimed.
    pub fn undoable(&self) -> Vec<Undoable> {
        self.work
            .iter()
            .filter(|work| matches!(work.state, State::Intent | State::Made))
            .map(|work| Undoable {
                owned: work.owned.clone(),
                claimed: self.claimed(work),
            })
            .collect()
    }

    /// When `work` was claimed: as its task's record says, else as the
    /// session's record noted its plan.
    fn claimed(&self, work: &task::Work) -> u64 {
        self.tasks
            .iter()
            .find(|record| record.key == work.task)
            .and_then(|record| record.claimed_at(&work.owned))
            .or_else(|| match &work.owned {
                Owned::Plan { id } => self.record.as_ref().and_then(|record| {
                    record
                        .plans
                        .iter()
                        .find(|plan| plan.plan_id == *id)
                        .map(|plan| plan.at)
                }),
                _ => None,
            })
            .unwrap_or(0)
    }

    /// The apps the scope's tasks opened by name: listed, never quit.
    pub fn left(&self) -> Vec<Value> {
        let mut apps: Vec<&str> = Vec::new();
        for work in &self.work {
            if let Owned::Launch { app } = &work.owned
                && work.state == State::Made
                && !apps.iter().any(|known| crate::text::same_text(known, app))
            {
                apps.push(app);
            }
        }
        apps.into_iter()
            .map(|app| json!({"app": app, "why": "opened by name"}))
            .collect()
    }

    /// The newest receipt of `scope` that has not finished, as its tasks'
    /// records list it.
    pub fn unfinished(&self, receipts: &Receipts, scope: &Scope) -> Option<Receipt> {
        self.tasks
            .iter()
            .flat_map(|record| record.cleanups.iter())
            .filter(|note| note.state != CleanupState::Complete)
            .filter_map(|note| receipts.load(&note.id))
            .filter(|receipt| {
                receipt.session == scope.session
                    && receipt.task == scope.task
                    && receipt.state != CleanupState::Complete
            })
            .max_by_key(|receipt| receipt.updated)
    }

    /// The tasks whose records a cleanup of `scope` notes itself in: each
    /// task its work belongs to, else the task it names or the session's
    /// default task.
    fn covered(&self, scope: &Scope) -> Vec<Task> {
        let mut keys: Vec<&Key> = Vec::new();
        for work in &self.work {
            if !keys.contains(&&work.task) {
                keys.push(&work.task);
            }
        }
        if keys.is_empty() {
            keys.push(scope.task.as_ref().unwrap_or(&scope.session));
        }
        keys.into_iter()
            .filter_map(
                |key| match self.tasks.iter().find(|record| record.key == *key) {
                    Some(record) => Some(Task::recorded(record)),
                    None if *key == scope.session => Task::of(&scope.session, None).ok(),
                    None => None,
                },
            )
            .collect()
    }
}

/// Whether the session of `record` may still be working: it acted within
/// the last minutes, or the host process it names still runs.
fn alive(record: &SessionRecord) -> bool {
    record.active(now_ms()) || record.host_process.is_some_and(|process| process.running())
}

/// Refuse an agent's cleanup of another session's work while that session
/// may still be working.
fn may_adopt(record: Option<&SessionRecord>) -> Result<()> {
    let Some(record) = record.filter(|record| alive(record)) else {
        return Ok(());
    };
    let minutes = now_ms().saturating_sub(record.last_seen) / 60_000;
    let ago = match minutes {
        0 => "less than a minute ago".to_owned(),
        1 => "1 minute ago".to_owned(),
        minutes => format!("{minutes} minutes ago"),
    };
    Err(Error::new(
        "input",
        format!("That session acted {ago} and may still be working; ask the person."),
    ))
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
    let receipts = Receipts::at(output);
    let receipt = loaded.unfinished(&receipts, scope);
    let mut steps = receipt
        .as_ref()
        .map(|receipt| receipt.steps.clone())
        .unwrap_or_default();
    merge(&mut steps, &loaded.undoable());

    let to_run: Vec<&Entry> = steps
        .iter()
        .filter(|entry| !entry.state.finished())
        .collect();
    let apps = if to_run
        .iter()
        .any(|entry| matches!(entry.step, Step::Quit { .. } | Step::RemoveCopy { .. }))
    {
        Some(hands.apps().await?)
    } else {
        None
    };
    let plans: Vec<Token> = to_run
        .iter()
        .filter_map(|entry| match &entry.step {
            Step::UndoPlan { plan } => Some(plan.clone()),
            _ => None,
        })
        .collect();
    let states = hands.plan_states(&plans).await?;
    let bundles: Vec<plan::Bundle> = to_run
        .iter()
        .flat_map(|entry| match &entry.step {
            Step::Quit { bundles } => bundles.clone(),
            Step::RemoveInstall { install, .. } => vec![plan::Bundle::Install(install.clone())],
            _ => Vec::new(),
        })
        .collect();
    let installs = run::installs_answer(hands, &bundles).await?;

    let shown: Vec<Value> = steps
        .iter()
        .map(|entry| {
            let mut value = described(hands, entry);
            if entry.state != StepState::Pending {
                said_state(&mut value, entry);
            }
            if entry.state.finished() {
                return value;
            }
            match &entry.step {
                Step::Quit { bundles } => {
                    let mut running = apps
                        .as_ref()
                        .map(|apps| running_from(apps, &folders(hands, bundles, &installs)))
                        .unwrap_or_default();
                    for process in run::running_from_installs(&installs) {
                        if !running.iter().any(|known| known.pid == process.pid) {
                            running.push(process);
                        }
                    }
                    value["apps"] = running
                        .iter()
                        .map(|process| json!({"app": process.name, "pid": process.pid}))
                        .collect();
                }
                Step::RemoveInstall { install, .. } => {
                    if let Some(found) = run::install_in(&installs, install) {
                        value["path"] = found["path"].clone();
                        value["present"] = found["present"].clone();
                        value["unchanged"] = found["unchanged"].clone();
                        value["installPhase"] = found["phase"].clone();
                    }
                }
                Step::UndoPlan { plan } => {
                    if let Some(state) = plans
                        .iter()
                        .position(|id| id == plan)
                        .and_then(|place| states.get(place))
                    {
                        value["planState"] = json!(state.phase);
                        value["targets"] = json!(state.targets);
                    }
                }
                Step::RemoveCopy { copy } => {
                    value["present"] = json!(handoff::copy_exists(hands.copies(), copy));
                }
            }
            value
        })
        .collect();

    Ok(json!({
        "ok": true,
        "preview": true,
        "session": session_named(output, &scope.session),
        "task": task_named(loaded, scope),
        "steps": shown,
        "left": loaded.left(),
        "receipt": receipt.map(|receipt| receipt.id),
    }))
}

async fn apply_scope(client: &Client, scope: &Scope) -> Result<Value> {
    let output = client.output_dir()?;
    if scope.adopting && client.actor == Actor::Agent {
        may_adopt(Sessions::at(output).read(&scope.session).as_ref())?;
    }
    // Held to the end, as close holds it, so no other open, close or
    // cleanup interleaves with one.
    let ownership = client.controller.acquire(false)?;
    let loaded = Loaded::of(output, scope)?;
    let adopter = scope
        .adopting
        .then(|| client.host_session.as_ref().map(|host| host.key().clone()));
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

/// Clean up `scope`'s work: finish its unfinished receipt, or start one,
/// with a step for everything in scope no step covers yet, then run the
/// steps not yet done or kept. `adopter` is who takes another session's
/// work over: its key, none for the person, and nothing at all when the
/// session cleans up its own. The caller holds control.
pub(crate) async fn apply(
    hands: &impl Hands,
    output: &Path,
    scope: &Scope,
    loaded: &Loaded,
    actor: Actor,
    adopter: Option<Option<Key>>,
) -> Result<Value> {
    let receipts = Receipts::at(output);
    let mut receipt = loaded.unfinished(&receipts, scope).unwrap_or_else(|| {
        Receipt::new(
            scope.session.clone(),
            scope.task.clone(),
            actor.as_str(),
            None,
        )
    });
    merge(&mut receipt.steps, &loaded.undoable());
    if receipt.steps.is_empty() {
        return Ok(json!({
            "ok": true,
            "preview": false,
            "state": "complete",
            "session": session_named(output, &scope.session),
            "task": task_named(loaded, scope),
            "steps": [],
            "left": loaded.left(),
            "receipt": null,
        }));
    }
    actor.as_str().clone_into(&mut receipt.actor);
    if let Some(adopter) = &adopter {
        receipt.adopted_by.clone_from(adopter);
    }

    receipts.save(&mut receipt)?;
    receipts.prune(&receipt.id);
    let covered = loaded.covered(scope);
    let adoption = adopter.map(|key| Adoption { key, at: now_ms() });
    note(output, &covered, &receipt, adoption.as_ref())?;

    let tasks = Tasks::at(output);
    let sessions = Sessions::at(output);
    let shown = run::run(
        hands,
        &mut receipt,
        &scope.session,
        |receipt| receipts.save(receipt),
        |step, state| settle(&tasks, &sessions, &covered, step, state),
    )
    .await?;
    receipts.save(&mut receipt)?;
    let _ = note(output, &covered, &receipt, None);

    Ok(applied(hands, output, scope, loaded, &receipt, &shown))
}

/// Note `receipt`, as it stands, in each of `covered`'s records, and who
/// adopted their work when someone did.
fn note(
    output: &Path,
    covered: &[Task],
    receipt: &Receipt,
    adoption: Option<&Adoption>,
) -> Result<()> {
    let tasks = Tasks::at(output);
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
/// too. A record that cannot be written changes nothing: the receipt says
/// what happened.
fn settle(tasks: &Tasks, sessions: &Sessions, covered: &[Task], step: &Step, state: StepState) {
    let Some(owned) = step.owned() else {
        return;
    };
    let now = now_ms();
    match state {
        StepState::Done => {
            let _ = tasks.settle_everywhere(std::slice::from_ref(&owned), State::Gone, now);
        }
        StepState::Kept => {
            for task in covered {
                let _ = tasks.update(task, now, |record| {
                    record.settle(&owned, State::Kept, now);
                    Ok(())
                });
            }
        }
        _ => return,
    }
    if let Owned::Plan { id } = &owned {
        let _ = sessions.forget_plan(id);
    }
}

/// The reply to a cleanup that ran: `ok` exactly when it is complete, kept
/// steps included, and `uncertain` exactly when it is uncertain.
fn applied(
    hands: &impl Hands,
    output: &Path,
    scope: &Scope,
    loaded: &Loaded,
    receipt: &Receipt,
    shown: &Shown,
) -> Value {
    let steps: Vec<Value> = receipt
        .steps
        .iter()
        .map(|entry| {
            let mut value = described(hands, entry);
            said_state(&mut value, entry);
            if let Some(Value::Object(more)) = shown.by_step.get(&entry.n) {
                for (key, more) in more {
                    value[key] = more.clone();
                }
            }
            value
        })
        .collect();
    let state = receipt.state;
    let mut reply = json!({
        "ok": state == CleanupState::Complete,
        "preview": false,
        "state": state,
        "receipt": receipt.id,
        "session": session_named(output, &scope.session),
        "task": task_named(loaded, scope),
        "steps": steps,
        "left": loaded.left(),
    });
    if state != CleanupState::Complete {
        reply["code"] = json!("cleanup");
        reply["uncertain"] = json!(state == CleanupState::Uncertain);
        reply["error"] = json!(unfinished(receipt));
    }
    reply
}

/// Why a cleanup is not finished, in one sentence with what to do.
fn unfinished(receipt: &Receipt) -> String {
    let failed = receipt
        .steps
        .iter()
        .filter(|entry| entry.state == StepState::Failed)
        .count();
    match receipt.state {
        CleanupState::Uncertain => "Cleanup stopped: Atlas does not know whether its last step \
             finished. Recover first (look, then atlas restart if the action was yours), then \
             run atlas cleanup --apply again."
            .to_owned(),
        CleanupState::Blocked => "Cleanup stopped: the person stopped agents. Ask them before \
             running atlas cleanup --apply again."
            .to_owned(),
        _ if failed == 0 => "Cleanup is not finished. Run atlas cleanup --apply again, or tell \
             the person."
            .to_owned(),
        _ => format!(
            "Cleanup is not finished: {failed} step{} failed. Run atlas cleanup --apply again, \
             or tell the person.",
            if failed == 1 { "" } else { "s" }
        ),
    }
}

/// A step as replies show it: its number, what it does and what to.
fn described(hands: &impl Hands, entry: &Entry) -> Value {
    let mut value = json!({"n": entry.n});
    match &entry.step {
        Step::Quit { .. } => value["do"] = json!("quit"),
        Step::UndoPlan { plan } => {
            value["do"] = json!("undo-plan");
            value["planId"] = json!(plan);
        }
        Step::RemoveInstall { install, app } => {
            value["do"] = json!("remove-install");
            value["installId"] = json!(install);
            value["app"] = json!(app);
        }
        Step::RemoveCopy { copy } => {
            value["do"] = json!("remove-copy");
            value["path"] = json!(handoff::copy_folder(hands.copies(), copy));
            value["app"] = json!(handoff::copy_app(hands.copies(), copy));
        }
    }
    value
}

/// Add how far `entry` got to its `value`.
fn said_state(value: &mut Value, entry: &Entry) {
    value["state"] = json!(entry.state);
    if let Some(reason) = entry.reason {
        value["reason"] = json!(reason.as_str());
    }
    if entry.retry {
        value["retry"] = json!(true);
    }
    if entry.already_gone {
        value["alreadyGone"] = json!(true);
    }
}

/// The session a reply is about: its key and the name every surface gives
/// it.
fn session_named(output: &Path, key: &Key) -> Value {
    let records = Sessions::at(output).all();
    let host = records
        .iter()
        .find(|record| record.key == *key)
        .map(|record| record.host);
    json!({"key": key, "name": name_of(&names(&records), key, host)})
}

/// The one task a reply is about, with the name it was given, when the
/// cleanup covers one task only.
fn task_named(loaded: &Loaded, scope: &Scope) -> Value {
    let Some(task) = &scope.task else {
        return Value::Null;
    };
    let name = loaded
        .tasks
        .iter()
        .find(|record| record.key == *task)
        .and_then(|record| record.name.clone());
    json!({"key": task, "name": name})
}
