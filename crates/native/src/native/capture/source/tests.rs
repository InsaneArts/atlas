use super::*;
use std::{
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::Duration,
};

fn stopped() -> Result<()> {
    Ok(())
}

fn never_stops() -> Result<()> {
    Err(Error::new(
        Code::Capture,
        "Native stream stop is still pending",
    ))
}

#[test]
fn one_generation_owns_the_source_until_its_stop_is_confirmed() {
    let source = Source::new(stopped);
    let first = source.admit(CaptureBackend::CoreGraphics).unwrap();
    assert_eq!(first.generation(), 1);
    assert_eq!(source.lifecycle(), Lifecycle::Active);
    assert_eq!(
        source
            .admit(CaptureBackend::ScreenCaptureKit)
            .unwrap_err()
            .code,
        Code::Busy
    );
    assert_eq!(source.drain(1), Drain::Held(Lifecycle::Active));
    source.stop(first, Some(9)).unwrap();
    assert_eq!(source.lifecycle(), Lifecycle::Stopped);
    assert_eq!(
        source.drain(1),
        Drain::Drained {
            stop_request: Some(9)
        }
    );
    let second = source.admit(CaptureBackend::ScreenCaptureKit).unwrap();
    assert_eq!(second.generation(), 2, "generations never repeat");
    source.stop(second, None).unwrap();
    assert_eq!(source.drain(2), Drain::Drained { stop_request: None });
    assert_eq!(source.drain(3), Drain::Unknown);
}

#[test]
fn an_unconfirmed_native_stop_quarantines_every_backend_until_it_is_confirmed() {
    static CONFIRMS: AtomicBool = AtomicBool::new(false);
    static ASKED: AtomicUsize = AtomicUsize::new(0);
    fn eventually() -> Result<()> {
        ASKED.fetch_add(1, Ordering::SeqCst);
        if CONFIRMS.load(Ordering::SeqCst) {
            Ok(())
        } else {
            never_stops()
        }
    }
    let source = Source::new(eventually);
    let owner = source.admit(CaptureBackend::ScreenCaptureKit).unwrap();
    assert!(source.stop(owner, Some(4)).is_err());
    assert_eq!(source.lifecycle(), Lifecycle::Quarantined);
    assert_eq!(source.drain(1), Drain::Held(Lifecycle::Quarantined));
    for backend in [
        CaptureBackend::CoreGraphics,
        CaptureBackend::ScreenCaptureKit,
    ] {
        assert_eq!(source.admit(backend).unwrap_err().code, Code::Busy);
    }
    assert_eq!(
        source
            .one_shot(|| panic!("no capture while quarantined"))
            .unwrap_err()
            .code,
        Code::Busy
    );
    assert_eq!(ASKED.load(Ordering::SeqCst), 4, "each admission asks again");

    CONFIRMS.store(true, Ordering::SeqCst);
    let next = source.admit(CaptureBackend::CoreGraphics).unwrap();
    assert_eq!(next.generation(), 2);
    assert_eq!(
        source.drain(1),
        Drain::Drained { stop_request: None },
        "a later confirmation never claims the earlier request's stop"
    );
    source.stop(next, None).unwrap();
    assert_eq!(source.one_shot(|| 7).unwrap(), 7);
}

#[test]
fn a_core_graphics_owner_never_asks_the_native_stream_to_stop() {
    let source = Source::new(|| panic!("no native source to stop"));
    let owner = source.admit(CaptureBackend::CoreGraphics).unwrap();
    source.stop(owner, None).unwrap();
    assert!(source.one_shot(|| ()).is_ok());
}

#[test]
fn only_the_newest_sixteen_drained_generations_are_remembered() {
    let source = Source::new(stopped);
    for _ in 0..(TOMBSTONES + 1) {
        let owner = source.admit(CaptureBackend::CoreGraphics).unwrap();
        source.stop(owner, None).unwrap();
    }
    assert_eq!(source.drain(1), Drain::Unknown, "pruned");
    assert_eq!(source.drain(2), Drain::Drained { stop_request: None });
}

#[test]
fn no_state_change_can_begin_while_a_one_shot_capture_runs() {
    use std::sync::{Arc, mpsc};
    let source = Arc::new(Source::new(stopped));
    let owner = source.admit(CaptureBackend::ScreenCaptureKit).unwrap();
    let (started, capturing) = mpsc::channel();
    let (finish, finished) = mpsc::channel::<()>();
    let shooting = source.clone();
    let shot = std::thread::spawn(move || {
        shooting
            .one_shot(|| {
                started.send(()).unwrap();
                finished.recv().unwrap();
            })
            .unwrap()
    });
    capturing.recv().unwrap();
    assert!(
        source.fence.try_write().is_err(),
        "a native stop or quarantine retry waits for the capture that passed the check"
    );
    assert!(
        source.state.try_lock().is_ok(),
        "the state stays free for reads"
    );
    finish.send(()).unwrap();
    shot.join().unwrap();
    source.stop(owner, None).unwrap();
    assert_eq!(source.lifecycle(), Lifecycle::Stopped);
}

/// A long enough wait for work between captures in tests: never reached
/// unless a capture is held.
const WAIT: Duration = Duration::from_secs(30);

#[test]
fn nothing_runs_between_captures_while_a_frame_or_one_shot_capture_runs() {
    use std::sync::{Arc, mpsc};
    let source = Arc::new(Source::new(stopped));
    let owner = source.admit(CaptureBackend::CoreGraphics).unwrap();
    for one_shot in [false, true] {
        let (started, capturing) = mpsc::channel();
        let (finish, finished) = mpsc::channel::<()>();
        let shooting = source.clone();
        let shot = std::thread::spawn(move || {
            let capture = || {
                started.send(()).unwrap();
                finished.recv().unwrap();
            };
            if one_shot {
                shooting.one_shot(capture).unwrap();
            } else {
                shooting.frame(capture).unwrap();
            }
        });
        capturing.recv().unwrap();
        assert!(
            source.fence.try_write().is_err(),
            "the capture holds the fence"
        );
        assert_eq!(
            source.between_captures(Duration::from_millis(20), || {
                panic!("nothing runs under a capture")
            }),
            None,
            "a capture still running when the wait ends defers the work"
        );
        finish.send(()).unwrap();
        shot.join().unwrap();
        assert_eq!(source.between_captures(Duration::ZERO, || 1), Some(1));
    }
    source.stop(owner, None).unwrap();
}

#[test]
fn work_between_captures_runs_once_the_running_capture_ends() {
    use std::sync::{Arc, mpsc};
    let source = Arc::new(Source::new(stopped));
    let (started, capturing) = mpsc::channel();
    let (finish, finished) = mpsc::channel::<()>();
    let ended = Arc::new(AtomicBool::new(false));
    let shooting = source.clone();
    let shot_ended = ended.clone();
    let shot = std::thread::spawn(move || {
        shooting
            .frame(|| {
                started.send(()).unwrap();
                finished.recv().unwrap();
                shot_ended.store(true, Ordering::SeqCst);
            })
            .unwrap();
    });
    capturing.recv().unwrap();
    let waiting = source.clone();
    let quiet_saw =
        std::thread::spawn(move || waiting.between_captures(WAIT, || ended.load(Ordering::SeqCst)));
    finish.send(()).unwrap();
    shot.join().unwrap();
    assert_eq!(quiet_saw.join().unwrap(), Some(true));
}

#[test]
fn no_capture_begins_while_work_between_captures_runs() {
    use std::sync::{Arc, mpsc};
    let source = Arc::new(Source::new(stopped));
    let (started, quieting) = mpsc::channel();
    let (finish, finished) = mpsc::channel::<()>();
    let waiting = source.clone();
    let quiet = std::thread::spawn(move || {
        waiting.between_captures(WAIT, || {
            started.send(()).unwrap();
            finished.recv().unwrap();
        })
    });
    quieting.recv().unwrap();
    assert!(
        source.fence.try_read().is_err(),
        "a frame or one-shot capture waits for the work to end"
    );
    finish.send(()).unwrap();
    assert_eq!(quiet.join().unwrap(), Some(()));
    assert_eq!(source.frame(|| 7).unwrap(), 7);
}

#[test]
fn nothing_runs_between_captures_while_a_native_stream_is_live_or_unconfirmed() {
    let source = Source::new(never_stops);
    let owner = source.admit(CaptureBackend::ScreenCaptureKit).unwrap();
    assert_eq!(
        source.between_captures(WAIT, || panic!("a live native stream would be cut")),
        None
    );
    assert!(source.stop(owner, None).is_err());
    assert_eq!(source.lifecycle(), Lifecycle::Quarantined);
    assert_eq!(
        source.between_captures(WAIT, || panic!(
            "an unconfirmed native stream may still run"
        )),
        None
    );
}

#[test]
fn work_runs_between_the_frames_of_a_core_graphics_stream_and_with_none_open() {
    let source = Source::new(stopped);
    assert_eq!(source.between_captures(WAIT, || 1), Some(1));
    let owner = source.admit(CaptureBackend::CoreGraphics).unwrap();
    assert_eq!(source.between_captures(WAIT, || 2), Some(2));
    source.stop(owner, None).unwrap();
    let native = source.admit(CaptureBackend::ScreenCaptureKit).unwrap();
    source.stop(native, None).unwrap();
    assert_eq!(source.between_captures(WAIT, || 3), Some(3));
}

#[test]
fn no_capture_begins_until_a_process_told_to_end_is_gone() {
    let source = Source::new(stopped);
    let owner = source.admit(CaptureBackend::CoreGraphics).unwrap();
    source.until_gone(Instance::current().unwrap());
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
    let pid = i32::try_from(std::process::id()).unwrap();
    source.until_gone(Instance::new(pid, [1, 0]));
    assert_eq!(
        source.frame(|| 1).unwrap(),
        1,
        "a process gone lets captures run"
    );
    assert_eq!(source.one_shot(|| 2).unwrap(), 2);
    source.stop(owner, None).unwrap();
}
