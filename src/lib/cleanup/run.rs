//! Running a receipt's steps in order. Each step reads only the thing it
//! names, checks that thing's own state first and then acts, so a step cut
//! short is simply run again: what had finished counts as done, and what
//! had not is tried again with every proof. Nothing here lists the copy
//! root or the helper's stores.
use super::{
    plan::{Bundle, Step},
    receipt::{Reason, Receipt, StepState},
};
use crate::{
    app_selection::Process,
    control::Token,
    environment::{PlanState, Undone},
    error::{Error, Result},
    handoff::{self, CopyRemoval, Running},
    host_session::Key,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

/// What a cleanup needs of the helper; tests stand in for it.
pub(crate) trait Hands {
    /// Where `open`'s copy folders are.
    fn copies(&self) -> &Path;
    /// The helper's running apps, from a list it gave.
    async fn apps(&self) -> Result<Value>;
    /// How far each of `ids` got, read without control.
    async fn plan_states(&self, ids: &[Token]) -> Result<Vec<PlanState>>;
    /// Signal `processes`, which run from `folders`, and wait until no app
    /// in a fresh list runs from any of them.
    async fn quit(&self, processes: &[Process], folders: &[String]) -> Result<()>;
    /// Undo one copy into the helper, under the control the cleanup holds.
    async fn undo_plan(&self, plan: &Token) -> Result<Undone>;
    /// Whether the person has stopped agents.
    fn stopped(&self) -> bool;
    /// Whether a signal asked this command to stop.
    fn interrupted(&self) -> bool;
}

/// What one run's steps showed, beside what the receipt keeps: the files a
/// kept copy into the helper left, by step number.
#[derive(Default)]
pub(crate) struct Shown {
    pub by_step: BTreeMap<u32, Value>,
}

/// Run `receipt`'s steps that are not done or kept, in order, for a
/// cleanup of `session`'s work, calling `save` before each step starts and
/// after it ends, and `settled` with each step that removed, undid or kept
/// its thing. A Stop between steps blocks the rest; a signal leaves them
/// pending; a step whose outcome is not known ends the run.
pub(crate) async fn run(
    hands: &impl Hands,
    receipt: &mut Receipt,
    session: &Key,
    mut save: impl FnMut(&mut Receipt) -> Result<()>,
    mut settled: impl FnMut(&Step, StepState),
) -> Result<Shown> {
    let mut shown = Shown::default();
    for place in 0..receipt.steps.len() {
        if receipt.steps[place].state.finished() {
            continue;
        }
        if hands.interrupted() {
            break;
        }
        if hands.stopped() {
            for entry in receipt.steps[place..]
                .iter_mut()
                .filter(|entry| !entry.state.finished())
            {
                entry.set(StepState::Blocked, Some(Reason::Stopped));
            }
            save(receipt)?;
            break;
        }

        let waiting = receipt.steps[place]
            .after
            .iter()
            .any(|n| receipt.step(*n).is_none_or(|entry| !entry.state.finished()));
        if waiting {
            receipt.steps[place].set(StepState::Failed, Some(Reason::NeedsStep));
            save(receipt)?;
            continue;
        }

        receipt.steps[place].set(StepState::Started, None);
        save(receipt)?;
        let step = receipt.steps[place].step.clone();
        let outcome = act(hands, &step, session).await;
        let entry = &mut receipt.steps[place];
        entry.set(outcome.state, outcome.reason);
        entry.already_gone = outcome.already_gone;
        if let Some(value) = outcome.shown {
            shown.by_step.insert(entry.n, value);
        }
        save(receipt)?;
        if matches!(outcome.state, StepState::Done | StepState::Kept) {
            settled(&step, outcome.state);
        }
        if outcome.state == StepState::Uncertain {
            break;
        }
    }
    Ok(shown)
}

/// What one step came to.
struct Outcome {
    state: StepState,
    reason: Option<Reason>,
    already_gone: bool,
    shown: Option<Value>,
}

impl Outcome {
    fn done() -> Self {
        Self {
            state: StepState::Done,
            reason: None,
            already_gone: false,
            shown: None,
        }
    }

    fn gone() -> Self {
        Self {
            already_gone: true,
            ..Self::done()
        }
    }

    fn kept(reason: Reason) -> Self {
        Self {
            state: StepState::Kept,
            reason: Some(reason),
            ..Self::done()
        }
    }

    fn failed(reason: Reason) -> Self {
        Self {
            state: StepState::Failed,
            reason: Some(reason),
            ..Self::done()
        }
    }

    /// A step that could not be carried out because of `error`: uncertain
    /// when its outcome is not known, otherwise a failure for `reason`.
    fn erred(error: &Error, reason: Reason) -> Self {
        if error.uncertain {
            Self {
                state: StepState::Uncertain,
                reason: None,
                ..Self::done()
            }
        } else {
            Self {
                shown: Some(json!({"error": error.error})),
                ..Self::failed(reason)
            }
        }
    }
}

async fn act(hands: &impl Hands, step: &Step, session: &Key) -> Outcome {
    match step {
        Step::Quit { bundles } => quit(hands, bundles).await,
        Step::UndoPlan { plan } => match hands.undo_plan(plan).await {
            Ok(Undone::AlreadyGone) => Outcome::gone(),
            Ok(Undone::Discarded | Undone::RolledBack) => Outcome::done(),
            Ok(Undone::Kept {
                targets,
                directories,
            }) => Outcome {
                shown: Some(json!({"targets": targets, "retainedDirectories": directories})),
                ..Outcome::kept(Reason::KeptTargets)
            },
            Ok(Undone::Unfinished(said)) => Outcome {
                shown: Some(json!({"error": said})),
                ..Outcome::failed(Reason::Helper)
            },
            Err(error) => Outcome::erred(&error, Reason::Helper),
        },
        Step::RemoveCopy { copy } => remove_copy(hands, copy, session).await,
    }
}

/// The folders `bundles` name, as paths processes run from.
pub(crate) fn folders(hands: &impl Hands, bundles: &[Bundle]) -> Vec<String> {
    bundles
        .iter()
        .map(|bundle| match bundle {
            Bundle::Copy(token) => handoff::copy_folder(hands.copies(), token)
                .to_string_lossy()
                .into_owned(),
        })
        .collect()
}

/// Whether `executable` lies inside one of `folders`.
pub(crate) fn inside(executable: &str, folders: &[String]) -> bool {
    !executable.is_empty()
        && folders.iter().any(|folder| {
            Path::new(executable).starts_with(folder) && executable.len() > folder.len()
        })
}

/// The app processes in `apps` (a list the helper gave) that run from one
/// of `folders`, by where they run from, never by name.
pub(crate) fn running_from(apps: &Value, folders: &[String]) -> Vec<Process> {
    apps["apps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|app| {
            let executable = app["executable"].as_str()?;
            inside(executable, folders).then(|| Process {
                pid: app["pid"].as_u64().unwrap_or(0),
                executable: executable.to_owned(),
                name: app["name"].as_str().unwrap_or("").to_owned(),
                protected: false,
            })
        })
        .filter(|process| process.pid > 0)
        .collect()
}

async fn quit(hands: &impl Hands, bundles: &[Bundle]) -> Outcome {
    let folders = folders(hands, bundles);
    let apps = match hands.apps().await {
        Ok(apps) => apps,
        Err(error) => return Outcome::erred(&error, Reason::Helper),
    };
    let processes = running_from(&apps, &folders);
    if processes.is_empty() {
        return Outcome::done();
    }
    match hands.quit(&processes, &folders).await {
        Ok(()) => Outcome::done(),
        Err(error) if error.code == "wait-failed" && !error.uncertain => Outcome {
            shown: Some(json!({"error": error.error})),
            ..Outcome::failed(Reason::Timeout)
        },
        Err(error) => Outcome::erred(&error, Reason::Helper),
    }
}

async fn remove_copy(hands: &impl Hands, copy: &Token, session: &Key) -> Outcome {
    let apps = match hands.apps().await {
        Ok(apps) => apps,
        Err(error) => return Outcome::erred(&error, Reason::Helper),
    };
    let running = Running::listed(&apps);
    match handoff::remove_copy(hands.copies(), copy, session, &running) {
        CopyRemoval::AlreadyGone => Outcome::gone(),
        CopyRemoval::Removed => Outcome::done(),
        CopyRemoval::AnotherSession => Outcome::kept(Reason::AnotherSession),
        CopyRemoval::Unreadable => Outcome::kept(Reason::LabelUnreadable),
        CopyRemoval::InUse => Outcome::failed(Reason::InUse),
        CopyRemoval::Failed(error) => Outcome {
            shown: Some(json!({"error": error})),
            ..Outcome::failed(Reason::Io)
        },
    }
}
