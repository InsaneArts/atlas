//! A cleanup's replies, built from its receipt: each step by its number,
//! what it does and to what, how far it got and what it came to, and what
//! the cleanup leaves to the agent.
use super::{
    plan::Step,
    receipt::{Entry, Receipt, StepState},
    scope::{Loaded, Scope},
};
use crate::{
    handoff,
    host_session::{
        Key,
        names::{name_of, names},
        store::Store as Sessions,
    },
    task::record::CleanupState,
};
use serde_json::{Value, json};
use std::path::Path;

/// A step as replies show it: its number, what it does and what to.
pub(super) fn described(copies: &Path, entry: &Entry) -> Value {
    let mut value = json!({"n": entry.n});
    match &entry.step {
        Step::CheckOutputs { outputs } => {
            value["do"] = json!("check-outputs");
            value["receipts"] = outputs.iter().map(|output| json!(output.receipt)).collect();
        }
        Step::StopJob { job, name, .. } => {
            value["do"] = json!("stop-job");
            value["id"] = json!(job);
            value["name"] = json!(name);
        }
        Step::FindJobs => value["do"] = json!("find-jobs"),
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
            value["path"] = json!(handoff::copy_folder(copies, copy));
            value["app"] = json!(handoff::copy_app(copies, copy));
        }
    }
    value
}

/// Add how far `entry` got, and what it came to, to its `value`.
pub(super) fn said_state(value: &mut Value, entry: &Entry) {
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
    let came_to = &entry.came_to;
    if !came_to.targets.is_empty() || came_to.retained_directories > 0 {
        value["targets"] = json!(came_to.targets);
        value["retainedDirectories"] = json!(came_to.retained_directories);
    }
    if !came_to.side_effects.is_empty() {
        value["sideEffects"] = json!({"kept": came_to.side_effects});
    }
    if let Some(held) = &came_to.held {
        value["held"] = json!(held);
    }
    if let Some(error) = &came_to.error {
        value["error"] = json!(error);
    }
    if !came_to.not_kept.is_empty() {
        value["notKept"] = json!(came_to.not_kept);
    }
    if !came_to.left.is_empty() {
        value["left"] = json!(came_to.left);
    }
}

/// What a cleanup leaves to the agent: the apps the scope's tasks opened
/// by name that still run by the helper's app list `running`, and the apps
/// the receipt's quit steps found running from a copy macOS moved them to
/// that Atlas cannot trace.
pub(super) fn left(
    loaded: &Loaded,
    receipt: Option<&Receipt>,
    running: Option<&Value>,
) -> Vec<Value> {
    let mut left = loaded.left(running);
    let untraced = receipt
        .into_iter()
        .flat_map(|receipt| &receipt.steps)
        .flat_map(|entry| &entry.came_to.untraced);
    for app in untraced {
        if !left.iter().any(|known| known["app"] == app.as_str()) {
            left.push(json!({"app": app, "why": "runs from a copy macOS moved it to"}));
        }
    }
    left
}

/// The reply to a cleanup that ran: `ok` exactly when it is complete, kept
/// steps included, and `uncertain` exactly when it is uncertain.
pub(super) fn applied(
    copies: &Path,
    output: &Path,
    scope: &Scope,
    loaded: &Loaded,
    receipt: &Receipt,
    running: Option<&Value>,
) -> Value {
    let steps: Vec<Value> = receipt
        .steps
        .iter()
        .map(|entry| {
            let mut value = described(copies, entry);
            said_state(&mut value, entry);
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
        "left": left(loaded, Some(receipt), running),
    });
    if state != CleanupState::Complete {
        reply["code"] = json!("cleanup");
        reply["uncertain"] = json!(state == CleanupState::Uncertain);
        reply["error"] = json!(unfinished(receipt));
    }
    reply
}

/// The reply when there is nothing to clean up, with the apps opened by
/// name that still run by the helper's app list `running`.
pub(super) fn nothing(
    output: &Path,
    scope: &Scope,
    loaded: &Loaded,
    preview: bool,
    running: Option<&Value>,
) -> Value {
    json!({
        "ok": true,
        "preview": preview,
        "state": "complete",
        "session": session_named(output, &scope.session),
        "task": task_named(loaded, scope),
        "steps": [],
        "left": loaded.left(running),
        "receipt": null,
    })
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
        CleanupState::Blocked
            if receipt.steps.iter().any(|entry| {
                entry.reason == Some(super::receipt::Reason::OutputNotKept)
                    && entry.state == StepState::Blocked
            }) =>
        {
            "Cleanup is not finished: a collected copy is missing or changed, so the files it \
             came from were kept. Collect them again with atlas collect, then run atlas cleanup \
             --apply again."
                .to_owned()
        }
        CleanupState::Incomplete
            if receipt.steps.iter().any(|entry| {
                entry.reason == Some(super::receipt::Reason::Uncertain)
                    && entry.state == StepState::Blocked
            }) =>
        {
            "Cleanup is not finished: an earlier action's outcome is unknown, so only background \
             jobs were stopped and collected copies checked. Recover first (look, then atlas \
             restart if the action was yours), then run atlas cleanup --apply again."
                .to_owned()
        }
        CleanupState::Blocked => "Cleanup stopped: the person stopped agents, or the helper \
             could not be reached to stop a background job. Ask the person before running atlas \
             cleanup --apply again."
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

/// The session a reply is about: its key and the name every surface gives
/// it.
pub(super) fn session_named(output: &Path, key: &Key) -> Value {
    let records = Sessions::at(output).all();
    let host = records
        .iter()
        .find(|record| record.key == *key)
        .map(|record| record.host);
    json!({"key": key, "name": name_of(&names(&records), key, host)})
}

/// The one task a reply is about, with the name it was given, when the
/// cleanup covers one task only.
pub(super) fn task_named(loaded: &Loaded, scope: &Scope) -> Value {
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
