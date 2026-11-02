//! A stop asked of a command by a signal. Nothing new starts once it is
//! asked. What is under way (a pending change, or a step that reports how
//! far it got) may finish within a grace, after which it is cut and left
//! as it is: pending when unacknowledged.
use crate::error::{Code, Error, Result};
use nix::sys::signal::Signal;
use std::{
    future::Future,
    ops::ControlFlow,
    panic::AssertUnwindSafe,
    sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError},
    time::Duration,
};
use tokio::{
    signal::unix::{self, SignalKind},
    sync::Notify,
    time::Instant,
};

/// How long the step under way may still take once a signal arrives.
const GRACE: Duration = Duration::from_secs(3);

/// After the cut, how long a step that was cut has to record that it was.
const SETTLE: Duration = Duration::from_secs(1);

/// Once the stop has settled, how long the command has to end on its own
/// before the process exits however far it got.
const LAST_RESORT: Duration = Duration::from_millis(500);

/// A stop that signals may ask of a command, shared by its clones.
#[derive(Clone, Debug)]
pub struct Interrupt(Arc<Shared>);

/// What every clone of an [`Interrupt`] shares.
#[derive(Debug)]
struct Shared {
    state: Mutex<State>,
    /// Told whenever the state changes.
    changed: Notify,
    /// How long the step under way may still take once a stop is asked.
    grace: Duration,
}

/// Whether a stop was asked, and what it still waits for.
#[derive(Debug, Default)]
struct State {
    asked: Option<Asked>,
    /// Pending changes this process holds.
    claims: usize,
    /// Other steps that report how far they got before stopping.
    steps: usize,
    /// Requests being sent, which a stop never drops half sent.
    sending: usize,
    /// A step under way was cut, so its outcome is unknown.
    cut: bool,
    /// The command has ended, and what is left is saying how.
    ended: bool,
}

/// A stop that was asked.
#[derive(Clone, Copy, Debug)]
struct Asked {
    /// The first signal.
    signal: Signal,
    /// When the step under way is cut: the end of the grace, or sooner if
    /// a second signal came.
    cut: Instant,
    /// A second signal came, so nothing under way is waited for.
    forced: bool,
}

/// What a [`Hold`] keeps a stop waiting for.
#[derive(Clone, Copy, Debug)]
enum Held {
    /// A pending change.
    Claim,
    /// A step that stops at its own next check and reports how far it got.
    Step,
    /// A request being sent.
    Sending,
}

/// Work under way that a stop waits for, until dropped.
#[must_use = "a stop waits for the work only while it is held"]
#[derive(Debug)]
pub(crate) struct Hold {
    interrupt: Interrupt,
    held: Held,
}

/// How a command a signal stopped ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stopped {
    signal: Signal,
    /// A change was cut or still under way, so its outcome is unknown.
    cut: bool,
}

/// The signals that ask for a stop.
struct Signals {
    terminate: unix::Signal,
    hangup: unix::Signal,
    interrupt: unix::Signal,
}

impl Interrupt {
    /// A stop of its own, giving the step under way `grace` once asked.
    pub(crate) fn new(grace: Duration) -> Self {
        Self(Arc::new(Shared {
            state: Mutex::default(),
            changed: Notify::new(),
            grace,
        }))
    }

    /// The stop this process's signals reach once it [`Interrupt::listen`]s,
    /// with the usual grace. A process that never listens is never stopped.
    pub fn process() -> &'static Self {
        static PROCESS: OnceLock<Interrupt> = OnceLock::new();
        PROCESS.get_or_init(|| Self::new(GRACE))
    }

    /// Turn SIGTERM, SIGHUP and SIGINT into this stop. Call once, inside a
    /// runtime that lasts until the process exits. The first signal asks
    /// it, and a second cuts what is under way at once. Once the stop has
    /// settled, a command that has still not ended (it is blocked where it
    /// never checks, or in saying how it ended) is ended here: `report`
    /// says how, unless the command already got that far, and the process
    /// exits with the signal's status. Signals that cannot be handled keep
    /// their default action.
    pub fn listen(&self, report: impl FnOnce(&Stopped) + Send + 'static) {
        let Some(mut signals) = Signals::register() else {
            return;
        };

        let interrupt = self.clone();
        tokio::spawn(async move {
            let Some(signal) = signals.next().await else {
                return;
            };
            interrupt.ask(signal);

            loop {
                tokio::select! {
                    _ = interrupt.settled() => break,
                    Some(signal) = signals.next() => interrupt.ask(signal),
                }
            }

            // A command that can end does so at once from here.
            tokio::time::sleep(LAST_RESORT).await;
            interrupt.give_up(report);
        });
    }

    /// Ask for a stop, as a signal does: the first names it, and a later
    /// one cuts what is under way at once.
    pub(crate) fn ask(&self, signal: Signal) {
        self.change(|state| {
            let now = Instant::now();
            match &mut state.asked {
                None => {
                    state.asked = Some(Asked {
                        signal,
                        cut: now + self.0.grace,
                        forced: false,
                    });
                }
                Some(asked) => {
                    asked.cut = asked.cut.min(now);
                    asked.forced = true;
                }
            }
        });
    }

    /// The signal that asked for a stop, once one did.
    pub fn asked(&self) -> Option<Signal> {
        self.state().asked.map(|asked| asked.signal)
    }

    /// The command has ended; what is left is saying how. A stop asked from
    /// now on never reports again, and still exits with the signal's
    /// status if saying so does not finish.
    pub fn ended(&self) {
        self.change(|state| state.ended = true);
    }

    /// Refuse to start a step once a stop was asked.
    pub(crate) fn check(&self) -> Result<()> {
        match self.asked() {
            None => Ok(()),
            Some(signal) => Err(interrupted(signal, "; nothing more was started.")),
        }
    }

    /// [`Interrupt::check`] while waiting for control, saying it had waited
    /// `waited`.
    pub(crate) fn check_waiting(&self, waited: Duration) -> Result<()> {
        match self.asked() {
            None => Ok(()),
            Some(signal) => Err(interrupted(
                signal,
                " while waiting for another Atlas command to finish; nothing was done.",
            )
            .with("waited", serde_json::json!(crate::control::seconds(waited)))),
        }
    }

    /// A pending change under way; [`crate::control::Pending`] holds one.
    pub(crate) fn claim(&self) -> Hold {
        self.hold(Held::Claim)
    }

    /// A step that stops at its own next [`Interrupt::check`] and reports
    /// how far it got: a stop waits for it as for a change.
    pub(crate) fn step(&self) -> Hold {
        self.hold(Held::Step)
    }

    /// A request being sent. A stop waits until it is all sent or has
    /// failed, even after a second signal: a request half sent changed
    /// nothing, and is settled as never sent, so it must not be dropped
    /// with its claim. Sending is bounded by its transport.
    pub(crate) fn sending(&self) -> Hold {
        self.hold(Held::Sending)
    }

    /// Once a stop was asked, the end of its grace (at once after a second
    /// signal), with the signal that asked: a step still under way is cut
    /// there, with [`Interrupt::cut_short`].
    pub(crate) async fn cut(&self) -> Signal {
        self.when(|state| match state.asked {
            Some(asked) if Instant::now() >= asked.cut => ControlFlow::Break(asked.signal),
            asked => ControlFlow::Continue(asked.map(|asked| asked.cut)),
        })
        .await
    }

    /// The failure of a step [`Interrupt::cut`] cut short, remembered so the
    /// command ends uncertain however it ends.
    pub(crate) fn cut_short(&self, signal: Signal) -> Error {
        self.change(|state| state.cut = true);
        cut_error(signal)
    }

    /// `command`'s own result, unless a stop settles first. Then `command`
    /// is dropped where it waits, so its control is let go and a change
    /// still under way stays pending, and the stop's failure is the result.
    /// A result ready at the same moment wins.
    ///
    /// # Errors
    ///
    /// Propagates the command's error or returns the settled interruption error. Cancellation
    /// does not prove that in-flight effects were acknowledged; their owners retain cleanup and
    /// recovery.
    pub async fn unless_stopped<T>(&self, command: impl Future<Output = Result<T>>) -> Result<T> {
        tokio::select! {
            biased;
            result = command => result,
            stopped = self.settled() => Err(stopped.error()),
        }
    }

    /// How the command ended, once a stop was asked and nothing is being
    /// sent, and then nothing holds it any more, or a second signal came,
    /// or the grace and the time to settle a cut have both passed.
    async fn settled(&self) -> Stopped {
        self.when(|state| {
            let Some(asked) = state.asked else {
                return ControlFlow::Continue(None);
            };
            if state.sending > 0 {
                return ControlFlow::Continue(None);
            }

            let settled = asked.cut + SETTLE;
            if asked.forced || state.claims + state.steps == 0 || Instant::now() >= settled {
                ControlFlow::Break(state.stopped(asked.signal))
            } else {
                ControlFlow::Continue(Some(settled))
            }
        })
        .await
    }

    /// End the process with the signal's status, `report` saying how the
    /// command ended unless it got as far as saying so itself. A report
    /// that panics still exits.
    fn give_up(&self, report: impl FnOnce(&Stopped)) {
        let (stopped, ended) = {
            let state = self.state();
            let Some(asked) = state.asked else {
                return;
            };
            (state.stopped(asked.signal), state.ended)
        };

        if !ended {
            let _ = std::panic::catch_unwind(AssertUnwindSafe(|| report(&stopped)));
        }
        std::process::exit(stopped.status().into());
    }

    /// Wait until `ready` answers, looking again whenever the state
    /// changes and at the moment it names, if any.
    async fn when<T>(&self, ready: impl Fn(&State) -> ControlFlow<T, Option<Instant>>) -> T {
        loop {
            // Made before the state is read, so a change in between still
            // wakes it.
            let changed = self.0.changed.notified();
            let until = match ready(&self.state()) {
                ControlFlow::Break(answer) => return answer,
                ControlFlow::Continue(until) => until,
            };

            match until {
                Some(until) => {
                    tokio::select! {
                        () = changed => {}
                        () = tokio::time::sleep_until(until) => {}
                    }
                }
                None => changed.await,
            }
        }
    }

    fn hold(&self, held: Held) -> Hold {
        self.change(|state| *state.count(held) += 1);
        Hold {
            interrupt: self.clone(),
            held,
        }
    }

    /// Change the state, then tell everyone waiting on it.
    fn change(&self, change: impl FnOnce(&mut State)) {
        change(&mut self.state());
        self.0.changed.notify_waiters();
    }

    /// The state; a panic elsewhere while it was held leaves it usable.
    fn state(&self) -> MutexGuard<'_, State> {
        self.0.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl State {
    /// How many of `held` are under way.
    fn count(&mut self, held: Held) -> &mut usize {
        match held {
            Held::Claim => &mut self.claims,
            Held::Step => &mut self.steps,
            Held::Sending => &mut self.sending,
        }
    }

    /// How a command stopped by `signal` ends now: cut once a step was cut
    /// or while a change is still under way.
    fn stopped(&self, signal: Signal) -> Stopped {
        Stopped {
            signal,
            cut: self.cut || self.claims > 0,
        }
    }
}

impl Drop for Hold {
    fn drop(&mut self) {
        let held = self.held;
        self.interrupt.change(|state| *state.count(held) -= 1);
    }
}

impl Stopped {
    /// The failure a stopped command reports.
    pub fn error(&self) -> Error {
        if self.cut {
            cut_error(self.signal)
        } else {
            interrupted(
                self.signal,
                " before it finished. What it had done was confirmed, so nothing is left unknown.",
            )
        }
    }

    /// The status the command exits with.
    pub fn status(&self) -> u8 {
        status(self.signal)
    }
}

impl Signals {
    /// The three signals, each handled from now on; none if any cannot be.
    fn register() -> Option<Self> {
        let handle = |kind| unix::signal(kind).ok();
        Some(Self {
            terminate: handle(SignalKind::terminate())?,
            hangup: handle(SignalKind::hangup())?,
            interrupt: handle(SignalKind::interrupt())?,
        })
    }

    /// The next of them to arrive.
    async fn next(&mut self) -> Option<Signal> {
        tokio::select! {
            Some(()) = self.terminate.recv() => Some(Signal::SIGTERM),
            Some(()) = self.hangup.recv() => Some(Signal::SIGHUP),
            Some(()) = self.interrupt.recv() => Some(Signal::SIGINT),
            else => None,
        }
    }
}

/// 128 plus the signal's number, as a shell reports a command `signal`
/// stopped.
pub fn status(signal: Signal) -> u8 {
    u8::try_from(signal as i32).map_or(u8::MAX, |number| 128u8.saturating_add(number))
}

/// The failure of a step still under way when a stop by `signal` cut it:
/// whether it happened is unknown.
fn cut_error(signal: Signal) -> Error {
    interrupted(
        signal,
        " while a change was under way, so whether it happened is unknown.",
    )
    .uncertain()
}

/// A failure because `signal` stopped the command, `how` saying what it
/// left.
fn interrupted(signal: Signal, how: &str) -> Error {
    let name = signal.as_str();
    Error::new(Code::Interrupted, format!("Stopped by {name}{how}"))
        .with("signal", serde_json::json!(name))
}

#[cfg(test)]
mod tests;
