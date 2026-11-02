use super::*;

#[test]
fn a_token_is_live_only_before_its_deadline() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let start = Instant::now();
    let ttl = Duration::from_secs(10);
    for elapsed in [
        ttl - Duration::from_nanos(1),
        ttl,
        ttl + Duration::from_nanos(1),
    ] {
        let mut lease = Lease::new(ttl);
        let owner = lease.acquire_at(&controller, start, || start).unwrap();
        let now = start + elapsed;
        assert_eq!(lease.expires, start + ttl);
        assert_eq!(lease.require_at(&owner, now).is_ok(), elapsed < ttl);
        assert_eq!(lease.owner.as_ref(), (elapsed < ttl).then_some(&owner));
        assert_eq!(lease.retired.as_ref(), (elapsed >= ttl).then_some(&owner));
        assert_eq!(lease.cleaning, elapsed >= ttl);
        assert!(!lease.inflight);
        assert!(lease.active_at(now));
        assert_eq!(controller.acquire(false).unwrap_err().code, "busy");
        assert_eq!(lease.begin_release_at(None, now).unwrap(), Some(owner));
        assert!(lease.retired.is_none());
        lease.end_release_at(now);
        assert!(!lease.active_at(now));
        assert!(controller.acquire(false).is_ok());
    }
}

#[test]
fn acquisition_grants_a_full_ttl_after_waiting_for_the_controller() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let start = Instant::now();
    let ttl = Duration::from_secs(10);
    let completed = start + ttl * 2;
    let mut lease = Lease::new(ttl);
    let owner = lease
        .acquire_at(&controller, start, || {
            assert_eq!(controller.acquire(false).unwrap_err().code, "busy");
            completed
        })
        .unwrap();

    assert_eq!(lease.expires, completed + ttl);
    lease.require_at(&owner, completed).unwrap();
    lease
        .require_at(&owner, completed + ttl - Duration::from_nanos(1))
        .unwrap();
    assert_eq!(
        lease.require_at(&owner, completed + ttl).unwrap_err().code,
        "lease-lost"
    );
}

#[test]
fn renewal_grants_a_full_ttl_after_successful_validation() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let start = Instant::now();
    let ttl = Duration::from_secs(10);
    let mut lease = Lease::new(ttl);
    let owner = lease.acquire_at(&controller, start, || start).unwrap();
    let validated = start + ttl - Duration::from_nanos(1);
    let completed = start + ttl * 2;

    lease.renew_at(&owner, validated, || completed).unwrap();

    assert_eq!(lease.expires, completed + ttl);
    lease
        .require_at(&owner, completed + ttl - Duration::from_nanos(1))
        .unwrap();
    assert_eq!(
        lease
            .renew_at(&owner, completed + ttl, || panic!(
                "an expired token cannot start another ttl"
            ))
            .unwrap_err()
            .code,
        "lease-lost"
    );
    assert_eq!(lease.expires, completed + ttl);
}

#[test]
fn renewal_uses_the_supplied_instant_without_reviving_an_expired_token() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let start = Instant::now();
    let ttl = Duration::from_secs(10);
    let mut lease = Lease::new(ttl);
    let owner = lease.acquire_at(&controller, start, || start).unwrap();
    let renewed = start + ttl - Duration::from_nanos(1);
    lease.renew_at(&owner, renewed, || renewed).unwrap();
    assert_eq!(lease.expires, renewed + ttl);
    let wrong = "x".repeat(owner.len());
    for token in [wrong.as_str(), "", &owner[..owner.len() - 1]] {
        let error = lease.require_at(token, renewed).unwrap_err();
        assert_eq!(error.code, "lease-lost");
        assert_eq!(
            error.error,
            "Control was released or expired. Take control again."
        );
        assert_eq!(
            lease
                .renew_at(token, renewed, || panic!("an invalid token cannot renew"))
                .unwrap_err()
                .code,
            "lease-lost"
        );
        assert_eq!(lease.expires, renewed + ttl);
    }
    lease.require_at(&owner, renewed).unwrap();
    let error = lease
        .renew_at(&owner, renewed + ttl, || renewed + ttl)
        .unwrap_err();
    assert_eq!(error.code, "lease-lost");
    assert_eq!(lease.expires, renewed + ttl);
}

#[test]
fn an_expired_token_cannot_renew_while_its_cleanup_retains_control() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let start = Instant::now();
    let ttl = Duration::from_secs(10);
    let mut lease = Lease::new(ttl);
    let owner = lease.acquire_at(&controller, start, || start).unwrap();
    let deadline = start + ttl;
    lease.expire_at(deadline);
    assert!(lease.expired());
    assert!(lease.descriptor.is_some());
    assert!(!lease.inflight);
    assert!(lease.cleaning);
    assert_eq!(
        lease
            .renew_at(&owner, deadline, || deadline)
            .unwrap_err()
            .code,
        "lease-lost"
    );
    assert_eq!(
        lease
            .acquire_at(&controller, deadline, || panic!(
                "a busy lease cannot be acquired"
            ))
            .unwrap_err()
            .code,
        "busy"
    );
    assert_eq!(controller.acquire(false).unwrap_err().code, "busy");
    assert_eq!(
        lease.begin_release_at(Some(&owner), deadline).unwrap(),
        Some(owner.clone())
    );
    assert!(!lease.expired());
    assert!(lease.cleaning);
    assert!(lease.active_at(deadline));
    assert_eq!(controller.acquire(false).unwrap_err().code, "busy");
    lease.end_release_at(deadline);
    assert!(lease.descriptor.is_none());
    assert!(!lease.cleaning);
    assert!(controller.acquire(false).is_ok());
}

#[tokio::test]
async fn finishing_cleanup_does_not_release_an_input_still_in_flight() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let lease = Arc::new(Mutex::new(Lease::default()));
    let start = Instant::now() + Duration::from_secs(3600);
    let owner = lease
        .lock()
        .await
        .acquire_at(&controller, start, || start)
        .unwrap();
    let permit = ActionPermit::begin(lease.clone(), &owner).await.unwrap();
    let mut state = lease.lock().await;
    let deadline = state.expires;
    assert_eq!(
        state.begin_release_at(Some(&owner), deadline).unwrap(),
        Some(owner)
    );
    assert!(state.inflight && state.cleaning && state.descriptor.is_some());
    state.end_release_at(deadline);
    assert!(state.inflight && !state.cleaning && state.descriptor.is_some());
    assert_eq!(controller.acquire(false).unwrap_err().code, "busy");
    drop(state);
    drop(permit);
    let state = lease.lock().await;
    assert!(!state.inflight && !state.cleaning && state.descriptor.is_none());
    assert!(controller.acquire(false).is_ok());
}

#[tokio::test]
async fn finishing_input_does_not_release_cleanup_still_in_progress() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let lease = Arc::new(Mutex::new(Lease::default()));
    let start = Instant::now() + Duration::from_secs(3600);
    let owner = lease
        .lock()
        .await
        .acquire_at(&controller, start, || start)
        .unwrap();
    let permit = ActionPermit::begin(lease.clone(), &owner).await.unwrap();
    let deadline = lease.lock().await.expires;
    lease.lock().await.expire_at(deadline);
    assert_eq!(
        lease
            .lock()
            .await
            .require_at(&owner, deadline)
            .unwrap_err()
            .code,
        "lease-lost"
    );
    assert_eq!(controller.acquire(false).unwrap_err().code, "busy");
    drop(permit);
    let mut state = lease.lock().await;
    assert!(!state.inflight && state.cleaning && state.descriptor.is_some());
    assert_eq!(controller.acquire(false).unwrap_err().code, "busy");
    assert_eq!(state.begin_release_at(None, deadline).unwrap(), Some(owner));
    state.end_release_at(deadline);
    assert!(!state.inflight && !state.cleaning && state.descriptor.is_none());
    assert!(controller.acquire(false).is_ok());
}

/// What the viewer does with an acquire the helper refused for certain.
fn end_refused_acquire(lease: &mut Lease, token: &str, now: Instant) {
    if lease.require_at(token, now).is_ok() {
        lease.release_at(now);
    }
}

#[test]
fn a_refused_acquire_never_ends_a_later_owner_or_unfinished_work() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let now = Instant::now();
    let mut lease = Lease::default();
    let old = lease.acquire_at(&controller, now, || now).unwrap();
    lease.release_at(now);
    let owner = lease.acquire_at(&controller, now, || now).unwrap();
    end_refused_acquire(&mut lease, &old, now);
    lease.require_at(&owner, now).unwrap();
    assert_eq!(
        lease.begin_release_at(Some(&owner), now).unwrap(),
        Some(owner.clone())
    );
    end_refused_acquire(&mut lease, &owner, now);
    assert!(lease.cleaning && lease.descriptor.is_some());
    assert_eq!(controller.acquire(false).unwrap_err().code, "busy");
    lease.end_release_at(now);
    let owner = lease.acquire_at(&controller, now, || now).unwrap();
    lease.inflight = true;
    end_refused_acquire(&mut lease, &owner, now);
    assert!(lease.active_at(now), "input in flight keeps the lock");
    assert_eq!(controller.acquire(false).unwrap_err().code, "busy");
    lease.inflight = false;
    assert!(!lease.active_at(now));
    assert!(controller.acquire(false).is_ok());
}

#[test]
fn a_refused_acquire_that_expired_leaves_cleanup_in_charge() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let mut lease = Lease::default();
    let owner = lease
        .acquire_at(&controller, Instant::now(), Instant::now)
        .unwrap();
    let deadline = lease.expires;
    end_refused_acquire(&mut lease, &owner, deadline);
    assert_eq!(lease.retired.as_ref(), Some(&owner));
    assert!(lease.cleaning && lease.descriptor.is_some());
    assert_eq!(controller.acquire(false).unwrap_err().code, "busy");
    assert_eq!(
        lease.begin_release_at(Some(&owner), deadline).unwrap(),
        Some(owner)
    );
    lease.end_release_at(deadline);
    assert!(controller.acquire(false).is_ok());
}

#[test]
fn beginning_a_release_clears_the_retired_token_too() {
    let now = Instant::now();
    let mut lease = Lease::new(Duration::from_secs(60));
    lease.owner = Some("a".repeat(48));
    lease.expires = now + Duration::from_secs(60);
    lease.retired = Some("b".repeat(48));
    assert_eq!(
        lease.begin_release_at(None, now).unwrap(),
        Some("a".repeat(48))
    );
    assert_eq!((lease.owner.clone(), lease.retired.clone()), (None, None));
    lease.retired = Some("b".repeat(48));
    assert_eq!(
        lease.begin_release_at(None, now).unwrap(),
        Some("b".repeat(48))
    );
    assert_eq!(lease.retired, None);
}

#[test]
fn a_stale_release_never_ends_a_different_owner() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let now = Instant::now();
    let mut lease = Lease::default();
    let old = lease.acquire_at(&controller, now, || now).unwrap();
    lease.begin_release_at(None, now).unwrap();
    lease.end_release_at(now);
    assert_eq!(lease.begin_release_at(Some(&old), now).unwrap(), None);
    let owner = lease.acquire_at(&controller, now, || now).unwrap();
    let error = lease.begin_release_at(Some(&old), now).unwrap_err();
    assert_eq!(error.code, "lease-lost");
    assert_eq!(error.error, "This token cannot release another controller");
    lease.require_at(&owner, now).unwrap();
    assert!(!lease.cleaning);
    assert_eq!(controller.acquire(false).unwrap_err().code, "busy");
}

#[tokio::test]
async fn a_permit_requires_a_live_token_and_excludes_another_input() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let lease = Arc::new(Mutex::new(Lease::default()));
    let start = Instant::now() + Duration::from_secs(3600);
    let owner = lease
        .lock()
        .await
        .acquire_at(&controller, start, || start)
        .unwrap();
    assert_eq!(
        ActionPermit::begin(lease.clone(), "wrong")
            .await
            .err()
            .unwrap()
            .code,
        "lease-lost"
    );
    let permit = ActionPermit::begin(lease.clone(), &owner).await.unwrap();
    let error = ActionPermit::begin(lease.clone(), &owner)
        .await
        .err()
        .unwrap();
    assert_eq!(error.code, "busy");
    assert_eq!(error.error, "Wait for the current input to finish");
    drop(permit);
    assert!(!lease.lock().await.inflight);
    assert!(lease.lock().await.active_at(start));
    lease.lock().await.expire_for_test();
    assert_eq!(
        ActionPermit::begin(lease.clone(), &owner)
            .await
            .err()
            .unwrap()
            .code,
        "lease-lost"
    );
    assert!(!lease.lock().await.inflight);
    assert_eq!(controller.acquire(false).unwrap_err().code, "busy");
}

fn with_watchdog(scenario: impl FnOnce() + Send + 'static) {
    let (finished, completion) = std::sync::mpsc::channel();
    // Keep blocking work and its unwind cleanup off the waiting test thread.
    // Detaching on timeout lets the test fail even if a Drop or join stalls.
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(scenario));
        let _ = finished.send(result);
    });
    if let Err(panic) = completion
        .recv_timeout(Duration::from_secs(30))
        .expect("permit scenario did not complete")
    {
        std::panic::resume_unwind(panic);
    }
}

#[test]
fn a_contended_permit_settles_in_a_runtime_task() {
    with_watchdog(|| {
        let directory = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, directory.path());
        let lease = Arc::new(Mutex::new(Lease::default()));
        let start = Instant::now() + Duration::from_secs(3600);
        let owner = lease
            .blocking_lock()
            .acquire_at(&controller, start, || start)
            .unwrap();
        let (parked, waiting) = std::sync::mpsc::channel();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .on_thread_park(move || {
                let _ = parked.send(());
            })
            .build()
            .unwrap();
        let permit = runtime
            .block_on(ActionPermit::begin(lease.clone(), &owner))
            .unwrap();
        let mut held = lease.blocking_lock();
        held.release_at(start);
        {
            let _entered = runtime.enter();
            drop(permit);
        }
        assert!(held.inflight && held.descriptor.is_some());
        assert_eq!(controller.acquire(false).unwrap_err().code, "busy");
        assert!(waiting.try_recv().is_err());
        let (finish, finished) = tokio::sync::oneshot::channel();
        let worker = std::thread::spawn(move || runtime.block_on(finished).unwrap());
        // A current-thread runtime parks only after its spawned cleanup has
        // queued for the held mutex. The next guard follows that cleanup.
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(held);
        let state = lease.blocking_lock();
        assert!(!state.inflight && state.descriptor.is_none());
        assert!(controller.acquire(false).is_ok());
        finish.send(()).unwrap();
        worker.join().unwrap();
    });
}

thread_local! {
    static BLOCKING_DROP: std::cell::RefCell<Option<std::sync::mpsc::Sender<()>>> = const {
        std::cell::RefCell::new(None)
    };
}

pub(super) fn blocking_drop_started() {
    BLOCKING_DROP.with(|signal| {
        if let Some(signal) = signal.take() {
            signal.send(()).unwrap();
        }
    });
}

#[test]
fn a_permit_settles_without_a_runtime() {
    with_watchdog(|| {
        let directory = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, directory.path());
        let lease = Arc::new(Mutex::new(Lease::default()));
        let start = Instant::now() + Duration::from_secs(3600);
        let owner = lease
            .blocking_lock()
            .acquire_at(&controller, start, || start)
            .unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let permit = runtime
            .block_on(ActionPermit::begin(lease.clone(), &owner))
            .unwrap();
        drop(runtime);
        let (acquired, locked) = std::sync::mpsc::channel();
        let (proceed, continued) = std::sync::mpsc::channel();
        let holder = std::thread::spawn({
            let lease = lease.clone();
            move || {
                let mut held = lease.blocking_lock();
                held.release_at(start);
                acquired.send(()).unwrap();
                continued.recv_timeout(Duration::from_secs(5)).unwrap();
            }
        });
        locked.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(controller.acquire(false).unwrap_err().code, "busy");
        let (started, dropping_started) = std::sync::mpsc::channel();
        let (finished, completion) = std::sync::mpsc::channel();
        let dropping = std::thread::spawn(move || {
            BLOCKING_DROP.with(|signal| *signal.borrow_mut() = Some(started));
            drop(permit);
            finished.send(()).unwrap();
        });
        dropping_started
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert!(completion.try_recv().is_err());
        proceed.send(()).unwrap();
        completion.recv_timeout(Duration::from_secs(5)).unwrap();
        dropping.join().unwrap();
        holder.join().unwrap();
        let state = lease.blocking_lock();
        assert!(!state.inflight && state.descriptor.is_none());
        assert!(controller.acquire(false).is_ok());
    });
}
