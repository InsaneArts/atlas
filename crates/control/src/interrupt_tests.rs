use super::*;
use crate::Controller;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// A grace no test waits out.
const LONG: Duration = Duration::from_secs(60);

/// A grace advanced by the paused-time tests.
const SHORT: Duration = Duration::from_millis(200);

fn pending(future: std::pin::Pin<&mut impl Future>) {
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(future.poll(&mut context).is_pending());
}

fn ready<F: Future>(future: std::pin::Pin<&mut F>) -> F::Output {
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    match future.poll(&mut context) {
        std::task::Poll::Ready(output) => output,
        std::task::Poll::Pending => panic!("the future must be ready without advancing time"),
    }
}

#[tokio::test(start_paused = true)]
async fn a_stop_with_nothing_under_way_settles_at_once() {
    let interrupt = Interrupt::new(LONG);
    interrupt.ask(Signal::SIGTERM);

    let stopped = ready(std::pin::pin!(interrupt.settled()));
    assert_eq!(
        stopped,
        Stopped {
            signal: Signal::SIGTERM,
            cut: false
        }
    );
}

#[tokio::test(start_paused = true)]
async fn a_stop_waits_for_the_change_under_way_then_settles() {
    let interrupt = Interrupt::new(LONG);
    let claim = interrupt.claim();
    interrupt.ask(Signal::SIGTERM);
    let (release, permit) = tokio::sync::oneshot::channel();
    let worker = tokio::spawn(async move {
        permit.await.unwrap();
        drop(claim);
    });
    let mut settlement = std::pin::pin!(interrupt.settled());
    pending(settlement.as_mut());
    release.send(()).unwrap();
    worker.await.unwrap();
    let stopped = ready(settlement.as_mut());
    assert!(!stopped.cut, "the change finished");
}

#[tokio::test(start_paused = true)]
async fn a_change_still_under_way_after_the_grace_is_cut() {
    let interrupt = Interrupt::new(SHORT);
    let _claim = interrupt.claim();
    interrupt.ask(Signal::SIGTERM);
    let mut cut = std::pin::pin!(interrupt.cut());
    let mut settlement = std::pin::pin!(interrupt.settled());
    pending(cut.as_mut());
    pending(settlement.as_mut());
    let tick = Duration::from_nanos(1);
    tokio::time::advance(SHORT - tick).await;
    pending(cut.as_mut());
    pending(settlement.as_mut());
    tokio::time::advance(tick).await;
    assert_eq!(ready(cut.as_mut()), Signal::SIGTERM);
    pending(settlement.as_mut());
    tokio::time::advance(SETTLE - tick).await;
    pending(settlement.as_mut());
    tokio::time::advance(tick).await;
    assert!(ready(settlement.as_mut()).cut, "its outcome is unknown");
}

#[tokio::test(start_paused = true)]
async fn a_second_signal_cuts_at_once() {
    let interrupt = Interrupt::new(LONG);
    let _claim = interrupt.claim();
    interrupt.ask(Signal::SIGTERM);
    let mut cut = std::pin::pin!(interrupt.cut());
    let mut settlement = std::pin::pin!(interrupt.settled());
    pending(cut.as_mut());
    pending(settlement.as_mut());
    interrupt.ask(Signal::SIGINT);
    assert_eq!(
        ready(cut.as_mut()),
        Signal::SIGTERM,
        "the first signal names the stop"
    );
    assert!(ready(settlement.as_mut()).cut);
}

#[tokio::test(start_paused = true)]
async fn a_step_cut_short_ends_uncertain_once_its_claim_is_gone() {
    let interrupt = Interrupt::new(SHORT);
    let step = interrupt.step();
    interrupt.ask(Signal::SIGTERM);
    let mut cut = std::pin::pin!(interrupt.cut());
    pending(cut.as_mut());
    tokio::time::advance(SHORT).await;
    let error = interrupt.cut_short(ready(cut.as_mut()));
    assert!(error.uncertain);
    drop(step);

    let stopped = ready(std::pin::pin!(interrupt.settled()));
    assert!(stopped.cut, "what was cut stays unknown");
    assert!(stopped.error().uncertain);
}

#[tokio::test(start_paused = true)]
async fn a_request_being_sent_is_never_dropped_half_sent() {
    let interrupt = Interrupt::new(LONG);
    let sending = interrupt.sending();
    interrupt.ask(Signal::SIGTERM);
    interrupt.ask(Signal::SIGTERM);
    let mut settlement = std::pin::pin!(interrupt.settled());
    pending(settlement.as_mut());
    tokio::time::advance(LONG + SETTLE).await;
    pending(settlement.as_mut());
    drop(sending);
    assert!(
        !ready(settlement.as_mut()).cut,
        "a request not all sent changed nothing"
    );
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
/// The folder the stand-in keeps its controller state in.
const DIRECTORY: &str = "ATLAS_INTERRUPT_TEST_DIRECTORY";
/// The stand-in's grace, in milliseconds.
const GRACE_MS: &str = "ATLAS_INTERRUPT_TEST_GRACE_MS";

/// A stand-in for an Atlas command, run in a process of its own.
#[derive(Clone, Copy, Debug)]
enum Scenario {
    /// Waits on a read, with nothing under way.
    Idle,
    /// Holds a change until permitted to settle after observing a signal.
    Change,
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
            Self::Change => "change".to_owned(),
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
            "change" => Self::Change,
            _ => panic!("unknown test scenario"),
        }
    }

    /// Carry it out in `directory`, under `interrupt`, saying `ready`
    /// once a signal finds it where it is meant to.
    async fn run(self, directory: &Path, interrupt: &Interrupt) -> Result<()> {
        let controller =
            Controller::at(UID, &directory.join("state")).interrupted_by(interrupt.clone());
        match self {
            Self::Idle => {
                atlas_core::test_support::message("ready");
                std::future::pending::<()>().await;
            }
            Self::Change => {
                let _ownership = controller.acquire(false)?;
                let change = controller.quarantine()?;
                atlas_core::test_support::message("ready");
                interrupt
                    .when(|state| match state.asked {
                        Some(_) => ControlFlow::Break(()),
                        None => ControlFlow::Continue(None),
                    })
                    .await;
                atlas_core::test_support::message("signal-observed");
                let (released, permit) = tokio::sync::oneshot::channel();
                std::thread::spawn(move || {
                    atlas_core::test_support::permitted("continue");
                    let _ = released.send(());
                });
                permit.await.unwrap();
                controller.clear_if(&change)?;
                drop(change);
                atlas_core::test_support::message("released");
                std::future::pending::<()>().await;
            }
            Self::Waiting => {
                let waiting = controller.waiting(LONG).on_wait(move |event| {
                    if event == crate::WaitEvent::Began {
                        atlas_core::test_support::message("ready");
                    }
                });
                waiting.acquire(false)?;
            }
            Self::Blocked => {
                let _ownership = controller.acquire(false)?;
                let _change = controller.quarantine()?;
                atlas_core::test_support::message("ready");
                atlas_core::test_support::permitted("continue");
            }
            Self::Reading => {
                atlas_core::test_support::message("ready");
                atlas_core::test_support::permitted("continue");
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
        interrupt.listen(|stopped| {
            eprintln!("{}", json!(stopped.error()));
            atlas_core::test_support::message("exit");
        });
        interrupt
            .unless_stopped(Scenario::parse(&scenario).run(&directory, &interrupt))
            .await
    });
    interrupt.ended();
    if let Err(error) = result {
        eprintln!("{}", json!(error));
    }
    drop(runtime);
    atlas_core::test_support::message("exit");
    std::process::exit(interrupt.asked().map_or(0, |signal| status(signal).into()));
}

/// The status and public failure of the reaped command.
struct Ended {
    status: Option<i32>,
    failure: Value,
}

fn stand_in(
    test: &str,
    scenario: Scenario,
    directory: &Path,
    grace: Duration,
) -> atlas_core::test_support::TestChild {
    atlas_core::test_support::TestChild::spawn(
        atlas_core::test_support::rerun(test)
            .arg("--nocapture")
            .env(SCENARIO, scenario.name())
            .env(DIRECTORY, directory)
            .env(GRACE_MS, grace.as_millis().to_string()),
    )
}

/// Send the real signal only after the child has acknowledged readiness.
fn signal(child: &atlas_core::test_support::TestChild, signal: Signal) {
    let pid = nix::unistd::Pid::from_raw(i32::try_from(child.process.id()).unwrap());
    nix::sys::signal::kill(pid, signal).unwrap();
}

fn ended(child: &mut atlas_core::test_support::TestChild) -> Ended {
    child.expect("exit");
    let status = child.wait();
    let mut stderr = String::new();
    std::io::Read::read_to_string(&mut child.process.stderr.take().unwrap(), &mut stderr).unwrap();
    let failure = stderr
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<Value>(line).ok())
        .unwrap_or(Value::Null);
    Ended {
        status: status.code(),
        failure,
    }
}

fn signalled(
    test: &str,
    scenario: Scenario,
    directory: &Path,
    grace: Duration,
    sent: Signal,
) -> Ended {
    let mut child = stand_in(test, scenario, directory, grace);
    child.expect("ready");
    signal(&child, sent);
    if matches!(scenario, Scenario::Change) {
        child.expect("signal-observed");
        let controller = controller(directory);
        assert_eq!(controller.acquire(false).unwrap_err().code, "busy");
        assert_eq!(controller.pending().unwrap().len(), 1);
        child.permit("continue");
        child.expect("released");
    }
    ended(&mut child)
}

/// The control state the stand-ins keep in `directory`.
fn controller(directory: &Path) -> Controller {
    Controller::at(UID, &directory.join("state"))
}

#[test]
fn a_terminated_command_with_nothing_under_way_exits_at_once_with_its_status() {
    const TEST: &str = "interrupt::tests::a_terminated_command_with_nothing_under_way_exits_at_once_with_its_status";
    command_if_stand_in();
    if !atlas_core::test_support::isolated(TEST) {
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
    assert_eq!(ended.failure["code"], "interrupted");
    assert_eq!(ended.failure["uncertain"], false);
    assert_eq!(ended.failure["signal"], "SIGTERM");
}

#[test]
fn a_terminated_command_lets_its_change_settle_then_exits() {
    const TEST: &str = "interrupt::tests::a_terminated_command_lets_its_change_settle_then_exits";
    command_if_stand_in();
    if !atlas_core::test_support::isolated(TEST) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();

    let ended = signalled(
        TEST,
        Scenario::Change,
        directory.path(),
        LONG,
        Signal::SIGTERM,
    );
    assert_eq!(ended.status, Some(143));
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
    if !atlas_core::test_support::isolated(TEST) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let mut child = stand_in(TEST, Scenario::Change, directory.path(), LONG);
    child.expect("ready");
    let controller = controller(directory.path());
    let pending = controller.pending().unwrap();
    assert_eq!(pending.len(), 1);
    signal(&child, Signal::SIGTERM);
    child.expect("signal-observed");
    assert_eq!(controller.acquire(false).unwrap_err().code, "busy");
    assert_eq!(controller.pending().unwrap(), pending);
    signal(&child, Signal::SIGINT);
    let ended = ended(&mut child);
    assert_eq!(ended.status, Some(143));
    assert_eq!(ended.failure["code"], "interrupted");
    assert_eq!(ended.failure["uncertain"], true);

    assert_eq!(controller.pending().unwrap(), pending);
    assert_eq!(controller.unresolved().unwrap(), pending);
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
    if !atlas_core::test_support::isolated(TEST) {
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
    if !atlas_core::test_support::isolated(TEST) {
        return;
    }

    for (signal, status) in [(Signal::SIGHUP, 129), (Signal::SIGINT, 130)] {
        let directory = tempfile::tempdir().unwrap();
        let ended = signalled(TEST, Scenario::Change, directory.path(), LONG, signal);
        assert_eq!(ended.status, Some(status), "{signal}");
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
    if !atlas_core::test_support::isolated(TEST) {
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
    assert_eq!(ended.status, Some(143));
    assert_eq!(ended.failure["code"], "interrupted");
    assert_eq!(ended.failure["uncertain"], true);
    assert_eq!(controller(directory.path()).pending().unwrap().len(), 1);
}

#[test]
fn a_blocked_command_with_nothing_under_way_ends_promptly() {
    const TEST: &str = "interrupt::tests::a_blocked_command_with_nothing_under_way_ends_promptly";
    command_if_stand_in();
    if !atlas_core::test_support::isolated(TEST) {
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
    assert_eq!(ended.failure["code"], "interrupted");
    assert_eq!(ended.failure["uncertain"], false);
}
