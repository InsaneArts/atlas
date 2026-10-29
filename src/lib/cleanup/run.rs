//! Running a receipt's steps in order. Each step reads only the thing it
//! names, checks that thing's own state first and then acts, so a step cut
//! short is simply run again: what had finished counts as done, and what
//! had not is tried again with every proof. What each step came to is
//! kept in the receipt itself. Nothing here lists the copy root or the
//! helper's stores.
use super::{
    plan::{Bundle, Step},
    receipt::{CameTo, Reason, Receipt, SideEffect, StepState},
    targets::{installs_of, targets},
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
use std::path::Path;

/// How waiting for a quit step's processes ended.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Quitted {
    Gone,
    /// Some were still running when the wait ran out, as said.
    TimedOut(String),
}

/// What a cleanup needs of the helper; tests stand in for it.
pub(crate) trait Hands {
    /// Where `open`'s copy folders are.
    fn copies(&self) -> &Path;
    /// The helper's running apps, from a list it gave.
    async fn apps(&self) -> Result<Value>;
    /// How far each of `ids` got, read without control.
    async fn plan_states(&self, ids: &[Token]) -> Result<Vec<PlanState>>;
    /// How each of `ids` stands in the helper, read without control: where
    /// it is while its folder is the one Atlas placed, whether it is
    /// unchanged, and what runs from it.
    async fn installs(&self, ids: &[Token]) -> Result<Value>;
    /// Signal `processes`, each only while it runs from the folder it was
    /// proven to, and wait until no app in a fresh list runs from any of
    /// `folders`, nor any process from `installs`.
    async fn quit(
        &self,
        processes: &[Process],
        folders: &[String],
        installs: &[Token],
    ) -> Result<Quitted>;
    /// Undo one copy into the helper, under the control the cleanup holds.
    async fn undo_plan(&self, plan: &Token) -> Result<Undone>;
    /// Remove one install, when it is unchanged and nothing runs from it,
    /// under the control the cleanup holds.
    async fn remove_install(&self, install: &Token) -> Result<Value>;
    /// Whether the person has stopped agents.
    fn stopped(&self) -> bool;
    /// Whether a signal asked this command to stop.
    fn interrupted(&self) -> bool;
}

/// Run `receipt`'s steps that are not done or kept, in order, for a
/// cleanup of `session`'s work, calling `save` before each step starts and
/// after it ends, and `settled` with each step that removed, undid or kept
/// its thing, which says whether its task records took it. A Stop between
/// steps blocks the rest; a signal leaves them pending; a step whose
/// outcome is not known ends the run.
pub(crate) async fn run(
    hands: &impl Hands,
    receipt: &mut Receipt,
    session: &Key,
    mut save: impl FnMut(&mut Receipt) -> Result<()>,
    mut settled: impl FnMut(&Step, StepState) -> bool,
) -> Result<()> {
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
        entry.came_to = outcome.came_to;
        save(receipt)?;
        if outcome.state.finished() {
            receipt.steps[place].settled = settled(&step, outcome.state);
            save(receipt)?;
        }
        if outcome.state == StepState::Uncertain {
            break;
        }
    }
    Ok(())
}

/// What one step came to.
struct Outcome {
    state: StepState,
    reason: Option<Reason>,
    already_gone: bool,
    came_to: CameTo,
}

impl Outcome {
    fn done() -> Self {
        Self {
            state: StepState::Done,
            reason: None,
            already_gone: false,
            came_to: CameTo::default(),
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

    /// `self`, having come to `came_to`.
    fn with(self, came_to: CameTo) -> Self {
        Self { came_to, ..self }
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
            Self::failed(reason).with(CameTo::failed(&error.error))
        }
    }
}

async fn act(hands: &impl Hands, step: &Step, session: &Key) -> Outcome {
    match step {
        Step::Quit { bundles } => quit(hands, bundles, session).await,
        Step::UndoPlan { plan } => match hands.undo_plan(plan).await {
            Ok(Undone::AlreadyGone) => Outcome::gone(),
            Ok(Undone::Discarded | Undone::RolledBack) => Outcome::done(),
            Ok(Undone::Kept {
                targets,
                directories,
            }) => Outcome::kept(Reason::KeptTargets).with(CameTo {
                targets,
                retained_directories: directories,
                ..CameTo::default()
            }),
            Ok(Undone::Unfinished(said)) => {
                Outcome::failed(Reason::Helper).with(CameTo::failed(&said))
            }
            Err(error) => Outcome::erred(&error, Reason::Helper),
        },
        Step::RemoveInstall { install, .. } => remove_install(hands, install).await,
        Step::RemoveCopy { copy } => remove_copy(hands, copy, session).await,
    }
}

/// What the helper says of the installs among `bundles`; nothing to ask
/// when there are none.
pub(crate) async fn installs_answer(hands: &impl Hands, bundles: &[Bundle]) -> Result<Value> {
    let ids = installs_of(bundles);
    if ids.is_empty() {
        return Ok(json!({"ok": true, "installs": []}));
    }
    hands.installs(&ids).await
}

async fn quit(hands: &impl Hands, bundles: &[Bundle], session: &Key) -> Outcome {
    let installs = match installs_answer(hands, bundles).await {
        Ok(installs) => installs,
        Err(error) => return Outcome::erred(&error, Reason::Helper),
    };
    let apps = match hands.apps().await {
        Ok(apps) => apps,
        Err(error) => return Outcome::erred(&error, Reason::Helper),
    };
    let targets = targets(hands.copies(), session, bundles, &installs, &apps);
    let left = CameTo {
        untraced: targets.untraced.clone(),
        ..CameTo::default()
    };
    if targets.processes.is_empty() {
        return Outcome::done().with(left);
    }
    match hands
        .quit(&targets.processes, &targets.folders, &installs_of(bundles))
        .await
    {
        Ok(Quitted::Gone) => Outcome::done().with(left),
        Ok(Quitted::TimedOut(still)) => Outcome::failed(Reason::Timeout).with(CameTo {
            error: Some(still),
            ..left
        }),
        Err(error) => Outcome::erred(&error, Reason::Helper),
    }
}

async fn remove_install(hands: &impl Hands, install: &Token) -> Outcome {
    let reply = match hands.remove_install(install).await {
        Ok(reply) => reply,
        Err(error) => return Outcome::erred(&error, Reason::Helper),
    };
    let came_to = CameTo {
        side_effects: side_effects(&reply),
        held: reply["heldPath"].as_str().map(str::to_owned),
        ..CameTo::default()
    };
    if reply["ok"] != true {
        let said = reply["error"]
            .as_str()
            .unwrap_or("The helper did not finish moving the app aside");
        return Outcome::failed(Reason::Helper).with(CameTo::failed(said));
    }
    let outcome = match (reply["outcome"].as_str(), reply["reason"].as_str()) {
        (Some("held"), _) if came_to.held.is_some() => Outcome::done(),
        (Some("alreadyGone"), _) => Outcome::gone(),
        (Some("kept"), Some("replaced")) => Outcome::kept(Reason::Replaced),
        (Some("kept"), Some("other-volume")) => Outcome::kept(Reason::OtherVolume),
        (Some("kept"), Some("unreadable")) => Outcome::kept(Reason::Unreadable),
        (Some("kept"), _) => Outcome::kept(Reason::Changed),
        (Some("inUse"), _) => Outcome::failed(Reason::InUse),
        _ => Outcome::failed(Reason::Helper),
    };
    outcome.with(came_to)
}

/// The files an installed app keeps outside its bundle that a removal's
/// `reply` says exist now.
fn side_effects(reply: &Value) -> Vec<SideEffect> {
    reply["sideEffects"]["kept"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|effect| {
            Some(SideEffect {
                path: effect["path"].as_str()?.to_owned(),
                existed_before: effect["existedBefore"] == true,
            })
        })
        .collect()
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
        CopyRemoval::Failed(error) => Outcome::failed(Reason::Io).with(CameTo::failed(&error)),
    }
}
