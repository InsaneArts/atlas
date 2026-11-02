//! Ordered batch execution, focus budget and failure settlement.
use super::{Engine, Gate, worker_failed};
use crate::native::focus::{self, Need};
use atlas_core::{
    actions::{Action, Expect},
    error::{Code, Error, Result},
    screen::geometry::Bounds,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

/// What the input loop needs of the event engine; tests stand in for it.
pub(super) trait Perform {
    /// Monotonic time used to charge the request's shared focus budget.
    fn now(&self) -> Instant {
        Instant::now()
    }

    /// Refuse unless `expect` holds for `need` now, before anything is
    /// posted, taking at most `budget` to tell; once it does, the role of
    /// the field it names, when it names one.
    async fn require(
        &mut self,
        expect: &Expect,
        need: Need,
        budget: Duration,
        prepare: bool,
    ) -> Result<Option<&'static str>>;
    /// Carry out one action through posted events or a Dock AXPress.
    async fn perform(&mut self, action: Action) -> Result<()>;
    /// Events posted or AX presses acknowledged so far.
    fn posted(&self) -> u64;
    fn held_input(&self) -> bool;
    /// Let go of every key and button still held.
    fn release(&mut self) -> Result<()>;
}

impl Perform for Engine {
    /// The reads block, so they run on a blocking thread; the engine stays
    /// locked by the caller meanwhile, so nothing posts in between.
    async fn require(
        &mut self,
        expect: &Expect,
        need: Need,
        budget: Duration,
        prepare: bool,
    ) -> Result<Option<&'static str>> {
        let display = Bounds::display(self.epoch.width, self.epoch.height);
        let deadline = Instant::now() + budget;
        let expect = expect.clone();
        self.check()?;
        let guard = self.preparation_guard();
        checked_preparation(
            move || {
                if prepare {
                    focus::prepare(&expect, need, &display, deadline, guard)
                } else {
                    guard()?;
                    let result = focus::check(&expect, need, &display, deadline);
                    guard()?;
                    result
                }
            },
            || {
                self.check().and_then(|()| {
                    crate::native::ax::check_application(self.target)?;
                    drop(self.grant.as_ref().ok_or_else(worker_failed)?.authorize()?);
                    Ok(())
                })
            },
        )
        .await
    }

    async fn perform(&mut self, action: Action) -> Result<()> {
        Engine::perform(self, action).await
    }

    fn posted(&self) -> u64 {
        self.posted
    }

    fn held_input(&self) -> bool {
        Engine::held_input(self)
    }

    fn release(&mut self) -> Result<()> {
        Engine::release(self)
    }
}

pub(super) async fn preparation_worker<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|cause| worker_failed().with_source(cause))?
}

pub(super) async fn checked_preparation<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
    after: impl FnOnce() -> Result<()>,
) -> Result<T> {
    let result = preparation_worker(work).await;
    let after = after();
    match result {
        Err(error) if error.uncertain || matches!(error.code, Code::Stopped | Code::Cancelled) => {
            Err(error)
        }
        result => {
            after?;
            result
        }
    }
}

/// Carry out `actions` in order on `engine`, which the caller holds
/// locked, each checked against `expect` just before it starts: a key
/// after the batch's first against the app alone, since an earlier key
/// (tab, return, cmd+w) may move the window or field on purpose. On the
/// first failure, revoke `gate`, release what is held and return the
/// [`Failure`]. Once all are done, the role of the field `expect` names, when
/// every check of it found the same.
pub(super) async fn run(
    engine: &mut impl Perform,
    gate: &Gate,
    actions: Vec<Action>,
    expect: Option<&Expect>,
) -> std::result::Result<Option<&'static str>, Box<Failure>> {
    let mut budget = focus::PREPARE_BUDGET;
    let later = expect.map(Expect::app_only);
    let mut pressed = false;
    let mut roles = BTreeSet::new();

    for (performed, action) in actions.into_iter().enumerate() {
        let before = engine.posted();
        let key = match action {
            Action::Key { .. } => true,
            Action::Pointer { .. }
            | Action::Move { .. }
            | Action::Click { .. }
            | Action::Drag { .. }
            | Action::Scroll { .. }
            | Action::Type { .. }
            | Action::Wait { .. } => false,
        };
        let continuation = key && pressed;
        let expected = if continuation { later.as_ref() } else { expect };
        pressed |= key;
        let checked = match expected {
            Some(expected) => {
                let prepare = !continuation && !engine.held_input();
                require_within(engine, expected, action.target(), &mut budget, prepare).await
            }
            None => Ok(None),
        };
        let step = match checked {
            Ok(role) => {
                roles.extend(role);
                engine.perform(action).await
            }
            Err(error) => Err(error),
        };

        if let Err(error) = step {
            // Only a poisoned gate fails this, and that refuses every later grant.
            let _ = gate.revoke();
            return Err(Box::new(failed(engine, error, performed, before)));
        }
    }

    let mut roles = roles.into_iter();
    Ok(match (roles.next(), roles.next()) {
        (Some(role), None) => Some(role),
        _ => None,
    })
}

/// [`Perform::require`], spending what it takes of `budget`: the time one
/// request may spend checking in all.
pub(super) async fn require_within(
    engine: &mut impl Perform,
    expect: &Expect,
    need: Need,
    budget: &mut Duration,
    prepare: bool,
) -> Result<Option<&'static str>> {
    let started = engine.now();
    let outcome = if need == Need::Nothing {
        Ok(None)
    } else {
        let available = if prepare {
            *budget
        } else {
            (*budget).min(focus::CHECK_BUDGET)
        };
        engine.require(expect, need, available, prepare).await
    };
    *budget = budget.saturating_sub(engine.now().saturating_duration_since(started));
    outcome
}

/// How a batch ended early: the failure, the actions finished before it,
/// whether the failing action had posted anything, and how releasing what
/// was held went.
#[derive(Debug)]
pub(super) struct Failure {
    error: Error,
    performed: usize,
    partial: bool,
    cleanup: Result<()>,
}

impl Failure {
    /// The reply that says so, and whether the outcome is unknown.
    pub(super) fn reply(&self) -> Value {
        let mut reply = json!(self.error);
        reply["performed"] = json!(self.performed);
        reply["partial"] = json!(self.partial);

        // A viewer's cancel is the person's doing, watching what landed; a stop
        // (the helper hidden) or any other break mid-action leaves that unknown.
        let unknown = self.partial && self.error.code != Code::Cancelled;
        reply["uncertain"] = json!(self.cleanup.is_err() || unknown || self.error.uncertain);

        if let Err(cleanup) = &self.cleanup {
            reply["cleanupError"] = json!(cleanup.error);
        }

        reply
    }
}

/// Settle an action that failed with `error` once `performed` actions had
/// finished and `before` events had been posted, releasing what is held.
pub(super) fn failed(
    engine: &mut impl Perform,
    error: Error,
    performed: usize,
    before: u64,
) -> Failure {
    // An action that posted some of its events before failing (a few typed
    // characters, one click of two) had part of its effect, and nobody can
    // say which part. A refused one posted nothing.
    let partial = engine.posted() > before;
    Failure {
        error,
        performed,
        partial,
        cleanup: engine.release(),
    }
}
