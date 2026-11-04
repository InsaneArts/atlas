use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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
