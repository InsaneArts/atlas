//! Ordered batch execution, referenced elements found again and acted on,
//! focus budget and failure settlement.
use super::{Engine, Gate, worker_failed};
use crate::native::{
    ax::{self, Acted, Reached, Resolved},
    focus::{self, Need},
};
use atlas_core::{
    actions::{Action, ElementAction, Expect},
    error::{Code, Error, Result},
    screen::{
        elements::{Reference, stale},
        geometry::Bounds,
    },
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
    /// Find the element `reference` names again, before anything is checked
    /// or posted: where to click it now and what that click must reach.
    async fn resolve(&mut self, reference: Reference) -> Result<Resolved>;
    /// Refuse unless the element `resolved` names is still the one to click,
    /// once its app was brought forward, asking nothing to move or focus.
    async fn confirm(&mut self, resolved: Resolved) -> Result<()>;
    /// Carry out `action` on the element `reference` names through
    /// Accessibility alone, in window `window` when the batch names one:
    /// found again and proven just before the request that acts, with
    /// nothing posted and nothing asked to take focus.
    async fn act(
        &mut self,
        reference: Reference,
        action: ElementAction,
        window: Option<i64>,
    ) -> Result<Acted>;
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

    /// The app is read again on a blocking thread under the same guards as
    /// a check; the engine stays locked meanwhile. A batch named for an app
    /// finds references only in it, and one that was not keeps the found
    /// app as its target through its last post.
    async fn resolve(&mut self, reference: Reference) -> Result<Resolved> {
        self.check()?;
        let looks = self.looks.clone();
        let epoch = self.epoch.clone();
        let named = self.target;
        let guard = self.preparation_guard();
        let resolved = checked_preparation(
            move || {
                guard()?;
                ax::resolve(&looks, reference, &epoch, named)
            },
            || {
                self.check().and_then(|()| {
                    drop(self.grant.as_ref().ok_or_else(worker_failed)?.authorize()?);
                    Ok(())
                })
            },
        )
        .await?;
        let found = resolved
            .expect
            .instance()
            .ok_or_else(|| stale("That look names no process"))?;
        self.target = Some(found);
        Ok(resolved)
    }

    /// Read on a blocking thread under the same guards as a check, as
    /// [`Perform::resolve`] is.
    async fn confirm(&mut self, resolved: Resolved) -> Result<()> {
        self.check()?;
        let epoch = self.epoch.clone();
        let guard = self.preparation_guard();
        checked_preparation(
            move || {
                guard()?;
                ax::confirm(&resolved, &epoch)
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

    /// The whole action runs on one blocking thread, which makes and keeps
    /// every object it asks or acts on, with the engine locked meanwhile.
    /// Its guard reads the gate anew at each step and holds no lock across
    /// a request, so a cancellation never waits for one. The found app is
    /// the batch's target through its last post, as for a referenced click.
    async fn act(
        &mut self,
        reference: Reference,
        action: ElementAction,
        window: Option<i64>,
    ) -> Result<Acted> {
        self.check()?;
        let looks = self.looks.clone();
        let epoch = self.epoch.clone();
        let named = self.target;
        let guard = self.preparation_guard();
        // A worker lost after the request that acts leaves its outcome
        // unknown, as `preparation_worker` says.
        let acted = preparation_worker(move || {
            ax::act_on(&looks, reference, &action, &epoch, named, window, &guard)
        })
        .await?;
        if acted.changed {
            self.posted += 1;
        }
        self.target = Some(acted.instance);
        Ok(acted)
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

/// What a batch came to: the role of the field its text went into (see
/// [`typed_into`]), or for a batch that types nothing, the field its checks
/// named, when every check of it found the same; what its action on an element through
/// Accessibility came to, when it had one, and the element its first
/// referenced click or action on an element reached, as it was met then.
pub(super) struct Done {
    pub(super) field: Option<&'static str>,
    pub(super) acted: Option<Acted>,
    pub(super) reached: Option<Reached>,
}

impl Done {
    /// How many of the batch's `count` actions were sent: all but an action
    /// on an element that already read as asked, for which nothing was.
    pub(super) fn performed(&self, count: usize) -> usize {
        count - usize::from(self.acted.as_ref().is_some_and(|acted| !acted.changed))
    }
}

/// Carry out `actions` in order on `engine`, which the caller holds
/// locked, each checked against `expect` just before it starts: a key
/// after the batch's first against the app alone, since an earlier key
/// (tab, return, cmd+w) may move the window or field on purpose, and a
/// referenced click against what its element, found again first, must
/// reach. An action on an element through Accessibility finds and proves
/// its own element, in the app and window `expect` names. On the first
/// failure, revoke `gate`, release what is held and return the
/// [`Failure`].
pub(super) async fn run(
    engine: &mut impl Perform,
    gate: &Gate,
    actions: Vec<Action>,
    expect: Option<&Expect>,
) -> std::result::Result<Done, Box<Failure>> {
    let mut budget = focus::PREPARE_BUDGET;
    let later = expect.map(Expect::app_only);
    let mut pressed = false;
    let mut roles = BTreeSet::new();
    // The role each typing action's check found, in turn.
    let mut typed = Vec::new();
    let mut acted = None;
    let mut reached = None;

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
            | Action::Wait { .. }
            | Action::Reference { .. }
            | Action::Press { .. }
            | Action::SetValue { .. }
            | Action::Check { .. }
            | Action::Uncheck { .. }
            | Action::Pick { .. }
            | Action::Expand { .. }
            | Action::Collapse { .. }
            | Action::ScrollIntoView { .. } => false,
        };
        let types = matches!(action, Action::Type { .. });
        let continuation = key && pressed;
        let expected = if continuation { later.as_ref() } else { expect };
        pressed |= key;
        let prepare = !continuation && !engine.held_input();

        let outcome = match action.element_action() {
            Some((reference, element)) => engine
                .act(reference, element, expected.and_then(Expect::window))
                .await
                .map(|done| {
                    let met = done.reached.clone();
                    acted = Some(done);
                    (None, met)
                }),
            None => step(engine, action, expected, prepare, &mut budget).await,
        };
        match outcome {
            Ok((role, met)) => {
                if types {
                    typed.push(role);
                }
                roles.extend(role);
                if reached.is_none() {
                    reached = met;
                }
            }
            Err(error) => {
                // Only a poisoned gate fails this, and that refuses every later grant.
                let _ = gate.revoke();
                return Err(Box::new(failed(engine, error, performed, before)));
            }
        }
    }

    let field = if typed.is_empty() {
        let mut roles = roles.into_iter();
        match (roles.next(), roles.next()) {
            (Some(role), None) => Some(role),
            _ => None,
        }
    } else {
        typed_into(&typed)
    };
    Ok(Done {
        field,
        acted,
        reached,
    })
}

/// The field a batch's text went into, from the role each of its typing
/// actions' checks found: a password field when any found one, and
/// otherwise the role every one of them found alike. None when any found
/// none, or they disagree: some text may then have gone into a password
/// field.
fn typed_into(typed: &[Option<&'static str>]) -> Option<&'static str> {
    if typed.contains(&Some(focus::SECURE)) {
        return Some(focus::SECURE);
    }
    let first = *typed.first()?;
    typed.iter().all(|role| *role == first).then_some(first)?
}

/// One action of a batch, checked against `expected` (when it names an app)
/// and carried out. A referenced click is found again first, checked
/// against what its own element must reach, then confirmed to be that very
/// element once more and its point checked again, reading only, just before
/// it is posted. Once done, the role of the field the check named, when it
/// named one, and for a referenced click, its element as it was found.
async fn step(
    engine: &mut impl Perform,
    action: Action,
    expected: Option<&Expect>,
    prepare: bool,
    budget: &mut Duration,
) -> Result<(Option<&'static str>, Option<Reached>)> {
    let (reference, button, count, modifiers) = match action {
        Action::Reference {
            reference,
            button,
            count,
            modifiers,
        } => (reference, button, count, modifiers),
        action => {
            let role = match expected {
                Some(expected) => {
                    require_within(engine, expected, action.target(), budget, prepare).await?
                }
                None => None,
            };
            engine.perform(action).await?;
            return Ok((role, None));
        }
    };

    let resolved = engine.resolve(reference).await?;
    // A batch that names a window clicks a reference only in it.
    if let Some(window) = expected.and_then(Expect::window)
        && resolved.expect.window() != Some(window)
    {
        return Err(stale("That element is not in the window named"));
    }
    let click = Action::Click {
        x: resolved.x,
        y: resolved.y,
        button,
        count,
        modifiers,
    };
    let expect = resolved.expect.clone();
    let reached = resolved.reached.clone();
    let role = require_within(engine, &expect, click.target(), budget, prepare).await?;
    engine.confirm(resolved).await?;
    // Whatever came in front meanwhile is seen by a check that only reads.
    require_within(engine, &expect, click.target(), budget, false).await?;
    engine.perform(click).await?;
    Ok((role, Some(reached)))
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
