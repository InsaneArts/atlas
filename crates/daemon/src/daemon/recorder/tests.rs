use super::*;
use atlas_core::{
    error::{Code, Error},
    protocol::CaptureBackend,
};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
};

const HELPER: u32 = 503;
const PERSON: u32 = 501;

fn replayd(pid: i32, uid: u32, footprint: u64) -> (Process, Usage) {
    (
        Process {
            instance: Instance::new(pid, [1_793_900_000, 250]),
            uid,
            real_uid: uid,
            path: PathBuf::from(RECORDER),
        },
        Usage {
            footprint,
            peak_footprint: footprint,
            resident: footprint / 4,
        },
    )
}

thread_local! {
    /// What `find` sees: the recorder the kill leaves in its place, once
    /// it has been killed.
    static FOUND: RefCell<Option<(Process, Usage)>> = const { RefCell::new(None) };
    static REPLACEMENT: RefCell<Option<(Process, Usage)>> = const { RefCell::new(None) };
    /// What `describe` says, when it is not what `find` saw.
    static DESCRIBED: RefCell<Option<Option<Process>>> = const { RefCell::new(None) };
    static KILLED: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) };
    static KILL_FAILS: Cell<bool> = const { Cell::new(false) };
    static EXITS: Cell<bool> = const { Cell::new(true) };
}

fn found() -> Option<(Process, Usage)> {
    FOUND.with_borrow(Clone::clone)
}

fn described(pid: i32) -> Option<Process> {
    DESCRIBED
        .with_borrow(Clone::clone)
        .unwrap_or_else(|| found().map(|(process, _)| process))
        .filter(|process| process.instance.pid == pid)
}

fn killed(pid: i32) -> nix::Result<()> {
    if KILL_FAILS.get() {
        return Err(Errno::EPERM);
    }
    KILLED.with_borrow_mut(|killed| killed.push(pid));
    FOUND.set(REPLACEMENT.take());
    Ok(())
}

fn fake() -> System {
    System {
        find: found,
        describe: described,
        kill: killed,
        exits: |_| EXITS.get(),
    }
}

fn kills() -> Vec<i32> {
    KILLED.with_borrow(Clone::clone)
}

fn source() -> Source {
    Source::new(|| Ok(()))
}

#[test]
fn a_recorder_past_the_limit_is_restarted_and_the_reading_shows_the_new_one() {
    FOUND.set(Some(replayd(700, HELPER, LIMIT + 1)));
    REPLACEMENT.set(Some(replayd(701, HELPER, 13 << 20)));
    let report = Recorder::new(fake(), Some(HELPER))
        .report(&source())
        .unwrap();
    assert_eq!(kills(), [700]);
    assert_eq!(report.resets, 1);
    assert!(report.last_reset_at.is_some());
    assert_eq!(report.reset_failures, 0);
    assert_eq!(report.usage.map(|usage| usage.footprint), Some(13 << 20));
}

#[test]
fn a_recorder_at_or_under_the_limit_is_left_alone() {
    for footprint in [LIMIT, LIMIT / 2, 0] {
        FOUND.set(Some(replayd(700, HELPER, footprint)));
        let report = Recorder::new(fake(), Some(HELPER))
            .report(&source())
            .unwrap();
        assert_eq!(kills(), [] as [i32; 0], "{footprint}");
        assert_eq!(report.resets, 0);
        assert_eq!(report.last_reset_at, None);
        assert_eq!(report.usage.map(|usage| usage.footprint), Some(footprint));
    }
}

#[test]
fn no_recorder_running_reports_nothing_and_ends_nothing() {
    FOUND.set(None);
    assert_eq!(Recorder::new(fake(), Some(HELPER)).report(&source()), None);
    assert_eq!(kills(), [] as [i32; 0]);
}

#[test]
fn a_restart_waits_for_a_capture_that_ends_in_time_and_none_begins_under_it() {
    static CAPTURING: AtomicBool = AtomicBool::new(false);
    static KILLED_DURING_CAPTURE: AtomicUsize = AtomicUsize::new(0);
    fn kill_noting_captures(pid: i32) -> nix::Result<()> {
        if CAPTURING.load(Ordering::SeqCst) {
            KILLED_DURING_CAPTURE.fetch_add(1, Ordering::SeqCst);
        }
        killed(pid)
    }
    FOUND.set(Some(replayd(700, HELPER, 6 << 30)));
    let source = Arc::new(source());
    let owner = source.admit(CaptureBackend::CoreGraphics).unwrap();
    let (started, capturing) = mpsc::channel();
    let (finish, finished) = mpsc::channel::<()>();
    let streaming = source.clone();
    let frame = std::thread::spawn(move || {
        streaming
            .frame(|| {
                CAPTURING.store(true, Ordering::SeqCst);
                started.send(()).unwrap();
                finished.recv().unwrap();
                CAPTURING.store(false, Ordering::SeqCst);
            })
            .unwrap();
    });
    capturing.recv().unwrap();
    let ending = std::thread::spawn(move || finish.send(()).unwrap());
    let report = Recorder::new(
        System {
            kill: kill_noting_captures,
            ..fake()
        },
        Some(HELPER),
    )
    .report(&source)
    .unwrap();
    ending.join().unwrap();
    frame.join().unwrap();
    assert_eq!(kills(), [700]);
    assert_eq!(KILLED_DURING_CAPTURE.load(Ordering::SeqCst), 0);
    assert_eq!(report.resets, 1);
    // The stream stays open, and its next frame runs once the restart ends.
    assert_eq!(source.frame(|| 1).unwrap(), 1);
    source.stop(owner, None).unwrap();
}

#[test]
fn a_capture_still_in_flight_puts_the_restart_off_to_the_next_reading() {
    FOUND.set(Some(replayd(700, HELPER, 6 << 30)));
    let source = Arc::new(source());
    let owner = source.admit(CaptureBackend::CoreGraphics).unwrap();
    let (started, capturing) = mpsc::channel();
    let (finish, finished) = mpsc::channel::<()>();
    let streaming = source.clone();
    let frame = std::thread::spawn(move || {
        streaming
            .frame(|| {
                started.send(()).unwrap();
                finished.recv().unwrap();
            })
            .unwrap();
    });
    capturing.recv().unwrap();
    let recorder = Recorder::new(fake(), Some(HELPER));
    let start = Instant::now();
    let held = recorder.report_at(&source, start).unwrap();
    assert_eq!(kills(), [] as [i32; 0], "nothing is ended under a capture");
    assert_eq!((held.resets, held.reset_failures), (0, 1));
    finish.send(()).unwrap();
    frame.join().unwrap();
    let next = recorder.report_at(&source, start + READING).unwrap();
    assert_eq!(kills(), [700]);
    assert_eq!((next.resets, next.reset_failures), (1, 0));
    assert_eq!(source.frame(|| 1).unwrap(), 1, "the stream stays open");
    source.stop(owner, None).unwrap();
}

#[test]
fn a_live_native_stream_is_never_cut_and_the_missed_restart_is_counted() {
    FOUND.set(Some(replayd(700, HELPER, 6 << 30)));
    let source = source();
    let owner = source.admit(CaptureBackend::ScreenCaptureKit).unwrap();
    let recorder = Recorder::new(fake(), Some(HELPER));
    let start = Instant::now();
    let first = recorder.report_at(&source, start).unwrap();
    let second = recorder.report_at(&source, start + READING).unwrap();
    assert_eq!(kills(), [] as [i32; 0]);
    assert_eq!((first.reset_failures, second.reset_failures), (1, 2));
    assert_eq!(second.resets, 0);
    source.stop(owner, None).unwrap();
    let third = recorder.report_at(&source, start + READING * 2).unwrap();
    assert_eq!(kills(), [700]);
    assert_eq!((third.resets, third.reset_failures), (1, 0));
}

#[test]
fn a_refused_signal_or_a_recorder_that_stays_is_counted_and_tried_again_next_minute() {
    FOUND.set(Some(replayd(700, HELPER, 6 << 30)));
    let source = source();
    let recorder = Recorder::new(fake(), Some(HELPER));
    let start = Instant::now();
    KILL_FAILS.set(true);
    assert_eq!(
        recorder.report_at(&source, start).unwrap().reset_failures,
        1
    );
    assert_eq!(
        recorder
            .report_at(&source, start + Duration::from_secs(30))
            .unwrap()
            .reset_failures,
        1,
        "nothing is tried again within the minute"
    );
    KILL_FAILS.set(false);
    EXITS.set(false);
    REPLACEMENT.set(found());
    let stayed = recorder.report_at(&source, start + READING).unwrap();
    assert_eq!((stayed.resets, stayed.reset_failures), (0, 2));
    EXITS.set(true);
    REPLACEMENT.set(Some(replayd(701, HELPER, 13 << 20)));
    let restarted = recorder.report_at(&source, start + READING * 2).unwrap();
    assert_eq!((restarted.resets, restarted.reset_failures), (1, 0));
}

#[test]
fn the_identity_check_refuses_a_changed_pid_uid_path_or_start_time() {
    let (measured, _) = replayd(700, HELPER, 6 << 30);
    assert!(may_restart(&measured, Some(&measured), HELPER));
    let changed = [
        Process {
            instance: Instance::new(701, measured.instance.start),
            ..measured.clone()
        },
        Process {
            instance: Instance::new(700, [1_793_900_000, 251]),
            ..measured.clone()
        },
        Process {
            uid: PERSON,
            ..measured.clone()
        },
        Process {
            real_uid: PERSON,
            ..measured.clone()
        },
        Process {
            path: PathBuf::from("/usr/libexec/replayd2"),
            ..measured.clone()
        },
    ];
    for now in &changed {
        assert!(!may_restart(&measured, Some(now), HELPER), "{now:?}");
    }
    assert!(!may_restart(&measured, None, HELPER), "it has exited");
}

#[test]
fn a_recorder_that_changed_before_the_signal_is_never_signalled() {
    FOUND.set(Some(replayd(700, HELPER, 6 << 30)));
    let (mut reused, _) = replayd(700, HELPER, 0);
    reused.instance = Instance::new(700, [1_793_900_001, 0]);
    reused.path = PathBuf::from("/bin/sleep");
    DESCRIBED.set(Some(Some(reused)));
    let report = Recorder::new(fake(), Some(HELPER))
        .report(&source())
        .unwrap();
    assert_eq!(kills(), [] as [i32; 0]);
    assert_eq!((report.resets, report.reset_failures), (0, 0));
}

#[test]
fn another_accounts_recorder_is_never_signalled() {
    let (persons, _) = replayd(600, PERSON, 6 << 30);
    assert!(!may_restart(&persons, Some(&persons), HELPER));
    let setuid = Process {
        real_uid: PERSON,
        ..replayd(600, HELPER, 0).0
    };
    assert!(!may_restart(&setuid, Some(&setuid), HELPER));

    FOUND.set(Some(replayd(600, PERSON, 6 << 30)));
    Recorder::new(fake(), Some(HELPER)).report(&source());
    assert_eq!(kills(), [] as [i32; 0], "the person's recorder stays");

    FOUND.set(Some(replayd(700, HELPER, 6 << 30)));
    let report = Recorder::new(fake(), None).report(&source()).unwrap();
    assert_eq!(
        kills(),
        [] as [i32; 0],
        "a daemon not the helper's only reads"
    );
    assert_eq!((report.resets, report.reset_failures), (0, 0));
}

#[test]
fn no_signal_ever_reaches_a_process_group_every_process_or_launchd() {
    for pid in [-1, 0, 1] {
        assert_eq!(sigkill(pid), Err(Errno::EINVAL));
        let (measured, _) = replayd(pid, HELPER, 6 << 30);
        assert!(!may_restart(&measured, Some(&measured), HELPER));
    }
}

#[test]
fn the_screen_recorder_is_read_once_a_minute_however_often_status_is_asked() {
    static READS: AtomicUsize = AtomicUsize::new(0);
    let recorder = Recorder::new(
        System {
            find: || {
                READS.fetch_add(1, Ordering::SeqCst);
                Some(replayd(700, HELPER, 1))
            },
            ..fake()
        },
        Some(HELPER),
    );
    let source = source();
    let start = Instant::now();
    for seconds in [0, 1, 30, 59] {
        let at = start + Duration::from_secs(seconds);
        assert!(recorder.report_at(&source, at).is_some());
    }
    assert_eq!(READS.load(Ordering::SeqCst), 1);
    assert!(recorder.report_at(&source, start + READING).is_some());
    assert_eq!(READS.load(Ordering::SeqCst), 2);
}

#[test]
fn statuses_asked_together_share_one_reading() {
    static READS: AtomicUsize = AtomicUsize::new(0);
    static GATE: Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>> = Mutex::new(None);
    let (entered, entering) = mpsc::channel();
    let (release, released) = mpsc::channel();
    *GATE.lock().unwrap() = Some((entered, released));
    let recorder = Recorder::new(
        System {
            find: || {
                READS.fetch_add(1, Ordering::SeqCst);
                if let Some((entered, released)) = GATE.lock().unwrap().take() {
                    entered.send(()).unwrap();
                    released.recv().unwrap();
                }
                Some(replayd(700, HELPER, 1))
            },
            ..fake()
        },
        Some(HELPER),
    );
    let source = source();
    let now = Instant::now();
    std::thread::scope(|scope| {
        let first = scope.spawn(|| recorder.report_at(&source, now));
        entering.recv().unwrap();
        let others: Vec<_> = (0..7)
            .map(|_| scope.spawn(|| recorder.report_at(&source, now)))
            .collect();
        release.send(()).unwrap();
        assert!(first.join().unwrap().is_some());
        for other in others {
            assert!(other.join().unwrap().is_some());
        }
    });
    assert_eq!(READS.load(Ordering::SeqCst), 1);
}

#[test]
fn a_failed_capture_source_stop_does_not_reach_the_recorder() {
    // A quarantined native source may still hold a stream, so it counts as
    // live until its stop is confirmed.
    fn never() -> atlas_core::error::Result<()> {
        Err(Error::new(Code::Capture, "still stopping"))
    }
    FOUND.set(Some(replayd(700, HELPER, 6 << 30)));
    let source = Source::new(never);
    let owner = source.admit(CaptureBackend::ScreenCaptureKit).unwrap();
    assert!(source.stop(owner, None).is_err());
    let report = Recorder::new(fake(), Some(HELPER)).report(&source).unwrap();
    assert_eq!(kills(), [] as [i32; 0]);
    assert_eq!(report.reset_failures, 1);
}

#[test]
fn a_recorder_that_has_not_exited_after_the_signal_keeps_every_capture_out() {
    // This test process stands in for a recorder that does not exit; the
    // fake signal never reaches it.
    let (mut lingering, usage) = replayd(0, HELPER, 6 << 30);
    lingering.instance = Instance::current().unwrap();
    FOUND.set(Some((lingering.clone(), usage)));
    REPLACEMENT.set(Some((lingering, usage)));
    EXITS.set(false);
    let source = source();
    let owner = source.admit(CaptureBackend::CoreGraphics).unwrap();
    let report = Recorder::new(fake(), Some(HELPER)).report(&source).unwrap();
    assert_eq!(kills(), [std::process::id() as i32]);
    assert_eq!((report.resets, report.reset_failures), (0, 1));
    assert_eq!(
        source
            .frame(|| panic!("no frame while it may exit"))
            .unwrap_err()
            .code,
        Code::Busy
    );
    assert_eq!(
        source
            .one_shot(|| panic!("no screenshot while it may exit"))
            .unwrap_err()
            .code,
        Code::Busy
    );
    source.stop(owner, None).unwrap();
}

#[test]
fn a_healthy_replacement_found_before_the_signal_clears_the_failures() {
    /// What `find` saw, after which it sees the replacement, if one is set.
    fn found_then_replaced() -> Option<(Process, Usage)> {
        let seen = found();
        if let Some(next) = REPLACEMENT.take() {
            FOUND.set(Some(next));
        }
        seen
    }
    FOUND.set(Some(replayd(700, HELPER, 6 << 30)));
    KILL_FAILS.set(true);
    let source = source();
    let recorder = Recorder::new(
        System {
            find: found_then_replaced,
            ..fake()
        },
        Some(HELPER),
    );
    let start = Instant::now();
    let failing = (0..3)
        .map(|minute| recorder.report_at(&source, start + READING * minute))
        .last()
        .flatten()
        .unwrap();
    assert_eq!(failing.reset_failures, 3);

    // The measured recorder exits and launchd starts a fresh one before the
    // signal is sent.
    DESCRIBED.set(Some(None));
    REPLACEMENT.set(Some(replayd(701, HELPER, 13 << 20)));
    let report = recorder.report_at(&source, start + READING * 3).unwrap();
    assert_eq!(kills(), [] as [i32; 0]);
    assert_eq!((report.resets, report.reset_failures), (0, 0));
    assert_eq!(report.usage.map(|usage| usage.footprint), Some(13 << 20));
}
