//! A stop asked of a command by a signal. Nothing new starts once it is
//! asked. What is under way (a pending change, or a step that reports how
//! far it got) may finish within a grace, after which it is cut and left
//! as it is: pending when unacknowledged.
use crate::error::{Error, Result};
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
            .with("waited", crate::control::seconds(waited))),
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
    Error::new("interrupted", format!("Stopped by {name}{how}")).with("signal", name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::Controller;
    use serde_json::{Value, json};
    use std::{
        fs,
        path::{Path, PathBuf},
        process::{Child, Stdio},
    };

    /// A grace no test waits out.
    const LONG: Duration = Duration::from_secs(10);

    /// A grace a test waits out.
    const SHORT: Duration = Duration::from_millis(200);

    /// How soon "at once" is, with room for a busy machine.
    const PROMPTLY: Duration = Duration::from_millis(500);

    #[tokio::test]
    async fn a_stop_with_nothing_under_way_settles_at_once() {
        let interrupt = Interrupt::new(LONG);
        interrupt.ask(Signal::SIGTERM);

        let stopped = tokio::time::timeout(PROMPTLY, interrupt.settled())
            .await
            .unwrap();
        assert_eq!(
            stopped,
            Stopped {
                signal: Signal::SIGTERM,
                cut: false
            }
        );
    }

    #[tokio::test]
    async fn a_stop_waits_for_the_change_under_way_then_settles() {
        let interrupt = Interrupt::new(LONG);
        let claim = interrupt.claim();
        interrupt.ask(Signal::SIGTERM);
        let asked = Instant::now();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            drop(claim);
        });

        let stopped = tokio::time::timeout(PROMPTLY * 2, interrupt.settled())
            .await
            .unwrap();
        assert!(asked.elapsed() >= Duration::from_millis(100));
        assert!(!stopped.cut, "the change finished");
    }

    #[tokio::test]
    async fn a_change_still_under_way_after_the_grace_is_cut() {
        let interrupt = Interrupt::new(SHORT);
        let _claim = interrupt.claim();
        interrupt.ask(Signal::SIGTERM);
        let asked = Instant::now();

        assert_eq!(interrupt.cut().await, Signal::SIGTERM);
        let cut = asked.elapsed();
        assert!(cut >= SHORT && cut < SHORT + PROMPTLY, "{cut:?}");

        let stopped = interrupt.settled().await;
        let settled = asked.elapsed();
        assert!(
            settled >= SHORT + SETTLE && settled < SHORT + SETTLE + PROMPTLY,
            "{settled:?}"
        );
        assert!(stopped.cut, "its outcome is unknown");
    }

    #[tokio::test]
    async fn a_second_signal_cuts_at_once() {
        let interrupt = Interrupt::new(LONG);
        let _claim = interrupt.claim();
        interrupt.ask(Signal::SIGTERM);
        interrupt.ask(Signal::SIGINT);

        let (signal, stopped) = tokio::time::timeout(PROMPTLY, async {
            (interrupt.cut().await, interrupt.settled().await)
        })
        .await
        .unwrap();
        assert_eq!(signal, Signal::SIGTERM, "the first signal names the stop");
        assert!(stopped.cut);
    }

    #[tokio::test]
    async fn a_step_cut_short_ends_uncertain_once_its_claim_is_gone() {
        let interrupt = Interrupt::new(SHORT);
        let step = interrupt.step();
        interrupt.ask(Signal::SIGTERM);
        let error = interrupt.cut_short(interrupt.cut().await);
        assert!(error.uncertain);
        drop(step);

        let stopped = tokio::time::timeout(PROMPTLY, interrupt.settled())
            .await
            .unwrap();
        assert!(stopped.cut, "what was cut stays unknown");
        assert!(stopped.error().uncertain);
    }

    #[tokio::test]
    async fn a_request_being_sent_is_never_dropped_half_sent() {
        let interrupt = Interrupt::new(LONG);
        let sending = interrupt.sending();
        interrupt.ask(Signal::SIGTERM);
        interrupt.ask(Signal::SIGTERM);

        let early = tokio::time::timeout(Duration::from_millis(100), interrupt.settled()).await;
        assert!(early.is_err(), "not even a second signal drops it");
        drop(sending);
        let stopped = tokio::time::timeout(PROMPTLY, interrupt.settled())
            .await
            .unwrap();
        assert!(!stopped.cut, "a request not all sent changed nothing");
    }

    #[test]
    fn nothing_new_starts_once_a_stop_was_asked() {
        let interrupt = Interrupt::new(LONG);
        assert!(interrupt.check().is_ok());
        assert!(interrupt.check_waiting(Duration::from_secs(2)).is_ok());
        assert_eq!(interrupt.asked(), None);

        interrupt.ask(Signal::SIGTERM);
        let refused = interrupt.check().unwrap_err();
        assert_eq!(refused.code, "interrupted");
        assert!(!refused.uncertain);
        assert_eq!(
            refused.error,
            "Stopped by SIGTERM; nothing more was started."
        );
        assert_eq!(refused.details["signal"], "SIGTERM");

        let waiting = interrupt
            .check_waiting(Duration::from_millis(2400))
            .unwrap_err();
        assert_eq!(waiting.code, "interrupted");
        assert!(!waiting.uncertain);
        assert_eq!(waiting.details["waited"], 2);
        assert!(waiting.error.contains("nothing was done"), "{waiting}");
        assert_eq!(interrupt.asked(), Some(Signal::SIGTERM));
    }

    #[test]
    fn a_stopped_command_ends_with_its_signals_status() {
        for (signal, status) in [
            (Signal::SIGTERM, 143),
            (Signal::SIGHUP, 129),
            (Signal::SIGINT, 130),
        ] {
            for cut in [false, true] {
                let stopped = Stopped { signal, cut };
                assert_eq!(stopped.status(), status);
                let error = stopped.error();
                assert_eq!(error.code, "interrupted");
                assert_eq!(error.uncertain, cut, "{error}");
                assert_eq!(error.details["signal"], signal.as_str());
            }
        }
        assert_eq!(
            cut_error(Signal::SIGTERM).error,
            "Stopped by SIGTERM while a change was under way, so whether it happened is unknown."
        );
    }

    /// The helper account the stand-in commands keep their control state for.
    const UID: u32 = 7002;

    /// What the stand-in command does, by name.
    const SCENARIO: &str = "ATLAS_INTERRUPT_TEST_SCENARIO";
    /// The folder the stand-in keeps its state and `ready` in.
    const DIRECTORY: &str = "ATLAS_INTERRUPT_TEST_DIRECTORY";
    /// The stand-in's grace, in milliseconds.
    const GRACE_MS: &str = "ATLAS_INTERRUPT_TEST_GRACE_MS";

    /// A stand-in for an Atlas command, run in a process of its own.
    #[derive(Clone, Copy, Debug)]
    enum Scenario {
        /// Waits on a read, with nothing under way.
        Idle,
        /// Holds a change for `hold` from the signal on, settles it, then
        /// waits on a read.
        Change { hold: Duration },
        /// Waits for control another process holds.
        Waiting,
        /// Holds a change while blocked where it never checks.
        Blocked,
        /// Blocked where it never checks, with nothing under way, as a
        /// read of its standard input is.
        Reading,
    }

    impl Scenario {
        fn name(self) -> String {
            match self {
                Self::Idle => "idle".to_owned(),
                Self::Change { hold } => format!("change-{}", hold.as_millis()),
                Self::Waiting => "waiting".to_owned(),
                Self::Blocked => "blocked".to_owned(),
                Self::Reading => "reading".to_owned(),
            }
        }

        fn parse(name: &str) -> Self {
            match name {
                "idle" => Self::Idle,
                "waiting" => Self::Waiting,
                "blocked" => Self::Blocked,
                "reading" => Self::Reading,
                change => Self::Change {
                    hold: Duration::from_millis(
                        change.strip_prefix("change-").unwrap().parse().unwrap(),
                    ),
                },
            }
        }

        /// Carry it out in `directory`, under `interrupt`, saying `ready`
        /// once a signal finds it where it is meant to.
        async fn run(self, directory: &Path, interrupt: &Interrupt) -> Result<()> {
            let ready = directory.join("ready");
            let controller =
                Controller::at(UID, &directory.join("state")).interrupted_by(interrupt.clone());
            match self {
                Self::Idle => {
                    fs::write(&ready, b"")?;
                    tokio::time::sleep(LONG).await;
                }
                Self::Change { hold } => {
                    let _ownership = controller.acquire(false)?;
                    let change = controller.quarantine()?;
                    fs::write(&ready, b"")?;
                    // Timed from the signal's arrival, not the sender's.
                    while interrupt.asked().is_none() {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    tokio::time::sleep(hold).await;
                    controller.clear_if(&change)?;
                    drop(change);
                    tokio::time::sleep(LONG).await;
                }
                Self::Waiting => {
                    let waiting = controller.waiting(LONG).on_wait(move |event| {
                        if event == crate::control::WaitEvent::Began {
                            let _ = fs::write(&ready, b"");
                        }
                    });
                    waiting.acquire(false)?;
                }
                Self::Blocked => {
                    let _ownership = controller.acquire(false)?;
                    let _change = controller.quarantine()?;
                    fs::write(&ready, b"")?;
                    std::thread::sleep(LONG * 2);
                }
                Self::Reading => {
                    fs::write(&ready, b"")?;
                    std::thread::sleep(LONG * 2);
                }
            }
            Ok(())
        }
    }

    /// When this process is a stand-in command: it listens and runs the
    /// way `atlas` does, prints its failure on stderr and exits with the
    /// signal's status once one came.
    fn command_if_stand_in() {
        let Ok(scenario) = std::env::var(SCENARIO) else {
            return;
        };
        let directory = PathBuf::from(std::env::var_os(DIRECTORY).unwrap());
        let grace = Duration::from_millis(std::env::var(GRACE_MS).unwrap().parse().unwrap());
        let interrupt = Interrupt::new(grace);

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let result = runtime.block_on(async {
            interrupt.listen(|stopped| eprintln!("{}", json!(stopped.error())));
            interrupt
                .unless_stopped(Scenario::parse(&scenario).run(&directory, &interrupt))
                .await
        });
        interrupt.ended();
        if let Err(error) = result {
            eprintln!("{}", json!(error));
        }
        drop(runtime);
        std::process::exit(interrupt.asked().map_or(0, |signal| status(signal).into()));
    }

    /// How a stand-in ended: its status, its failure and how long after
    /// the signal.
    struct Ended {
        status: Option<i32>,
        failure: Value,
        after: Duration,
    }

    /// Start `scenario` as the test `test` run again, in `directory`,
    /// giving it `grace`.
    fn stand_in(test: &str, scenario: Scenario, directory: &Path, grace: Duration) -> Child {
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test, "--nocapture"])
            .env(SCENARIO, scenario.name())
            .env(DIRECTORY, directory)
            .env(GRACE_MS, grace.as_millis().to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    }

    /// Run `scenario` in a process of its own, send it `signal` once it is
    /// ready, and wait for it to end.
    fn signalled(
        test: &str,
        scenario: Scenario,
        directory: &Path,
        grace: Duration,
        signal: Signal,
    ) -> Ended {
        let mut child = stand_in(test, scenario, directory, grace);
        let ready = directory.join("ready");
        let started = std::time::Instant::now();
        while !ready.exists() {
            assert!(started.elapsed() < LONG, "the stand-in never got ready");
            std::thread::sleep(Duration::from_millis(10));
        }

        let pid = nix::unistd::Pid::from_raw(i32::try_from(child.id()).unwrap());
        nix::sys::signal::kill(pid, signal).unwrap();
        let signalled = std::time::Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if signalled.elapsed() > LONG * 2 {
                let _ = child.kill();
                panic!("the stand-in did not end");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let after = signalled.elapsed();

        let mut stderr = String::new();
        std::io::Read::read_to_string(&mut child.stderr.take().unwrap(), &mut stderr).unwrap();
        let failure = stderr
            .lines()
            .rev()
            .find_map(|line| serde_json::from_str::<Value>(line).ok())
            .unwrap_or(Value::Null);

        Ended {
            status: status.code(),
            failure,
            after,
        }
    }

    /// The control state the stand-ins keep in `directory`.
    fn controller(directory: &Path) -> Controller {
        Controller::at(UID, &directory.join("state"))
    }

    #[test]
    fn a_terminated_command_with_nothing_under_way_exits_at_once_with_its_status() {
        const TEST: &str = "interrupt::tests::a_terminated_command_with_nothing_under_way_exits_at_once_with_its_status";
        command_if_stand_in();
        if !crate::transaction_test::isolated(TEST) {
            return;
        }
        let directory = tempfile::tempdir().unwrap();

        let ended = signalled(
            TEST,
            Scenario::Idle,
            directory.path(),
            LONG,
            Signal::SIGTERM,
        );
        assert_eq!(ended.status, Some(143));
        assert!(ended.after < PROMPTLY * 4, "{:?}", ended.after);
        assert_eq!(ended.failure["code"], "interrupted");
        assert_eq!(ended.failure["uncertain"], false);
        assert_eq!(ended.failure["signal"], "SIGTERM");
    }

    #[test]
    fn a_terminated_command_lets_its_change_settle_then_exits() {
        const TEST: &str =
            "interrupt::tests::a_terminated_command_lets_its_change_settle_then_exits";
        command_if_stand_in();
        if !crate::transaction_test::isolated(TEST) {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let hold = Duration::from_millis(500);

        let ended = signalled(
            TEST,
            Scenario::Change { hold },
            directory.path(),
            LONG,
            Signal::SIGTERM,
        );
        assert_eq!(ended.status, Some(143));
        assert!(
            ended.after >= hold && ended.after < hold + PROMPTLY * 4,
            "{:?}",
            ended.after
        );
        assert_eq!(ended.failure["code"], "interrupted");
        assert_eq!(ended.failure["uncertain"], false);

        let controller = controller(directory.path());
        assert!(controller.pending().unwrap().is_empty());
        assert!(controller.acquire(false).is_ok(), "the lock is free");
    }

    #[test]
    fn a_change_a_signal_cuts_stays_pending_for_every_agent() {
        const TEST: &str = "interrupt::tests::a_change_a_signal_cuts_stays_pending_for_every_agent";
        command_if_stand_in();
        if !crate::transaction_test::isolated(TEST) {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let grace = Duration::from_millis(300);

        let ended = signalled(
            TEST,
            Scenario::Change { hold: LONG },
            directory.path(),
            grace,
            Signal::SIGTERM,
        );
        assert_eq!(ended.status, Some(143));
        assert!(
            ended.after >= grace + SETTLE && ended.after < grace + SETTLE + PROMPTLY * 4,
            "{:?}",
            ended.after
        );
        assert_eq!(ended.failure["code"], "interrupted");
        assert_eq!(ended.failure["uncertain"], true);

        let controller = controller(directory.path());
        assert_eq!(controller.pending().unwrap().len(), 1);
        assert_eq!(controller.unresolved().unwrap().len(), 1);
        assert_eq!(controller.acquire(false).unwrap_err().code, "uncertain");
        let recovery = controller.acquire(true).unwrap();
        controller.clear().unwrap();
        drop(recovery);
        assert!(controller.acquire(false).is_ok());
    }

    #[test]
    fn a_signal_while_waiting_for_control_ends_the_wait_with_nothing_held() {
        const TEST: &str =
            "interrupt::tests::a_signal_while_waiting_for_control_ends_the_wait_with_nothing_held";
        command_if_stand_in();
        if !crate::transaction_test::isolated(TEST) {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let controller = controller(directory.path());
        let held = controller.acquire(false).unwrap();

        let ended = signalled(
            TEST,
            Scenario::Waiting,
            directory.path(),
            LONG,
            Signal::SIGTERM,
        );
        assert_eq!(ended.status, Some(143));
        assert!(ended.after < PROMPTLY * 4, "{:?}", ended.after);
        assert_eq!(ended.failure["code"], "interrupted");
        assert_eq!(ended.failure["uncertain"], false);
        assert!(ended.failure["waited"].is_u64(), "{}", ended.failure);
        assert!(controller.pending().unwrap().is_empty());
        drop(held);
    }

    #[test]
    fn a_hangup_and_an_interrupt_stop_it_the_same_way() {
        const TEST: &str = "interrupt::tests::a_hangup_and_an_interrupt_stop_it_the_same_way";
        command_if_stand_in();
        if !crate::transaction_test::isolated(TEST) {
            return;
        }
        let hold = Duration::from_millis(300);

        for (signal, status) in [(Signal::SIGHUP, 129), (Signal::SIGINT, 130)] {
            let directory = tempfile::tempdir().unwrap();
            let ended = signalled(
                TEST,
                Scenario::Change { hold },
                directory.path(),
                LONG,
                signal,
            );
            assert_eq!(ended.status, Some(status), "{signal}");
            assert!(
                ended.after >= hold && ended.after < hold + PROMPTLY * 4,
                "{:?}",
                ended.after
            );
            assert_eq!(ended.failure["signal"], signal.as_str());
            assert_eq!(ended.failure["uncertain"], false);
            assert!(controller(directory.path()).pending().unwrap().is_empty());
        }
    }

    #[test]
    fn a_command_that_never_reaches_a_check_is_ended_by_its_last_resort() {
        const TEST: &str =
            "interrupt::tests::a_command_that_never_reaches_a_check_is_ended_by_its_last_resort";
        command_if_stand_in();
        if !crate::transaction_test::isolated(TEST) {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let grace = Duration::from_millis(300);

        let ended = signalled(
            TEST,
            Scenario::Blocked,
            directory.path(),
            grace,
            Signal::SIGTERM,
        );
        let last_resort = grace + SETTLE + LAST_RESORT;
        assert_eq!(ended.status, Some(143));
        assert!(
            ended.after >= last_resort && ended.after < last_resort + PROMPTLY * 4,
            "{:?}",
            ended.after
        );
        assert_eq!(ended.failure["code"], "interrupted");
        assert_eq!(ended.failure["uncertain"], true);
        assert_eq!(controller(directory.path()).pending().unwrap().len(), 1);
    }

    #[test]
    fn a_blocked_command_with_nothing_under_way_ends_promptly() {
        const TEST: &str =
            "interrupt::tests::a_blocked_command_with_nothing_under_way_ends_promptly";
        command_if_stand_in();
        if !crate::transaction_test::isolated(TEST) {
            return;
        }
        let directory = tempfile::tempdir().unwrap();

        let ended = signalled(
            TEST,
            Scenario::Reading,
            directory.path(),
            LONG,
            Signal::SIGTERM,
        );
        assert_eq!(ended.status, Some(143));
        assert!(
            ended.after >= LAST_RESORT && ended.after < LAST_RESORT + PROMPTLY * 4,
            "{:?}",
            ended.after
        );
        assert_eq!(ended.failure["code"], "interrupted");
        assert_eq!(ended.failure["uncertain"], false);
    }
}
