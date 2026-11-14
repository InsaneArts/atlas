use super::*;
use crate::clock::Manual;
use std::sync::{
    Barrier, OnceLock,
    atomic::{AtomicUsize, Ordering::SeqCst},
};

/// What a hook heard, in order.
type Heard = Arc<Mutex<Vec<(Generation, Retired)>>>;

/// An authority on a clock the test moves, and what its hook heard.
fn held() -> (Authority, Arc<Manual>, Heard) {
    let clock = Arc::new(Manual::default());
    let heard = Arc::new(Mutex::new(Vec::new()));
    let hook = {
        let heard = heard.clone();
        Box::new(move |generation, cause| heard.lock().unwrap().push((generation, cause))) as Hook
    };
    (
        Authority::new(Generation(7), clock.clone(), hook),
        clock,
        heard,
    )
}

#[test]
fn effects_run_while_heartbeats_keep_coming() {
    let (authority, clock, _) = held();

    for _ in 0..10 {
        clock.advance(Duration::from_secs(1));
        authority.heard();
        authority.check().unwrap();
    }
}

#[test]
fn a_lease_that_lapses_retires_the_generation_and_refuses_what_comes_next() {
    let (authority, clock, heard) = held();

    clock.advance(LEASE + Duration::from_millis(1));
    let refused = authority.check().unwrap_err();
    authority.heard();

    assert_eq!(refused.code, Code::Stopped);
    assert!(
        authority.check().is_err(),
        "a heartbeat after the lapse brought it back"
    );
    assert_eq!(*heard.lock().unwrap(), [(Generation(7), Retired::Lapsed)]);
}

#[test]
fn work_queued_before_a_retirement_is_refused_though_the_clock_says_fresh() {
    let (authority, _clock, _) = held();
    let queued = authority.clone();
    authority.check().unwrap();

    authority.retire(Retired::Replaced);

    assert_eq!(queued.check().unwrap_err().code, Code::SessionChanged);
}

#[test]
fn each_way_a_generation_ends_retires_it_and_is_told_once() {
    for cause in [
        Retired::Replaced,
        Retired::ControlLost,
        Retired::Lapsed,
        Retired::Revoked,
    ] {
        let (authority, _clock, heard) = held();

        assert!(authority.retire(cause));
        assert!(
            !authority.retire(Retired::Replaced),
            "a second cause counted"
        );

        assert!(authority.check().is_err());
        assert_eq!(authority.retired(), Some(cause));
        assert_eq!(*heard.lock().unwrap(), [(Generation(7), cause)]);
    }
}

#[test]
fn a_hook_that_retires_again_is_not_heard_twice() {
    let clock = Arc::new(Manual::default());
    let calls = Arc::new(AtomicUsize::new(0));
    let slot: Arc<OnceLock<Authority>> = Arc::new(OnceLock::new());
    let hook = {
        let (calls, slot) = (calls.clone(), slot.clone());
        Box::new(move |_, _| {
            calls.fetch_add(1, SeqCst);
            if let Some(authority) = slot.get() {
                authority.retire(Retired::Revoked);
            }
        }) as Hook
    };
    let authority = Authority::new(Generation(1), clock, hook);
    let _ = slot.set(authority.clone());

    authority.retire(Retired::Lapsed);

    assert_eq!(calls.load(SeqCst), 1);
    assert_eq!(authority.retired(), Some(Retired::Lapsed));
}

#[test]
fn a_heartbeat_that_comes_after_the_lease_lapsed_retires_rather_than_renews() {
    let (authority, clock, heard) = held();

    clock.advance(LEASE + Duration::from_secs(1));
    authority.heard();

    assert_eq!(authority.check().unwrap_err().code, Code::Stopped);
    assert_eq!(*heard.lock().unwrap(), [(Generation(7), Retired::Lapsed)]);
}

#[test]
fn a_clock_that_cannot_be_read_never_grants_anything() {
    // Failing from the start.
    let clock = Arc::new(Manual::default());
    clock.set(None);
    let authority = Authority::new(Generation(1), clock.clone(), Box::new(|_, _| {}));
    assert_eq!(authority.retired(), Some(Retired::Lapsed));

    // Failing at a heartbeat, then at a check.
    for at_heartbeat in [true, false] {
        let (authority, clock, _) = held();
        clock.set(None);
        if at_heartbeat {
            authority.heard();
        }
        assert!(authority.check().is_err());
        clock.set(Some(Duration::ZERO));
        assert!(
            authority.check().is_err(),
            "a clock that came back revived it"
        );
    }
}

#[test]
fn a_clock_that_goes_backward_counts_as_a_lapse() {
    let (authority, clock, _) = held();
    clock.advance(Duration::from_secs(3));
    authority.heard();

    clock.set(Some(Duration::from_secs(1)));

    assert!(authority.check().is_err());
}

#[test]
fn a_clock_that_goes_back_after_a_check_counts_as_a_lapse_though_past_the_heartbeat() {
    for renewing in [false, true] {
        let (authority, clock, _) = held();
        clock.advance(Duration::from_secs(4));
        authority.check().unwrap();

        clock.set(Some(Duration::from_secs(1)));
        if renewing {
            authority.heard();
        }

        assert!(authority.check().is_err(), "renewing: {renewing}");
    }
}

#[test]
fn a_heartbeat_that_comes_first_counts_for_the_check_after_it() {
    let (authority, clock, _) = held();

    clock.advance(Duration::from_millis(4900));
    authority.heard();
    clock.advance(Duration::from_millis(200));

    authority.check().unwrap();
}

/// A clock that, once `armed`, records its read in `order` and then waits
/// at `barrier` twice: a check is held in the middle of deciding.
struct Paused {
    barrier: Barrier,
    armed: std::sync::atomic::AtomicBool,
    order: Arc<Mutex<Vec<&'static str>>>,
}

impl Clock for Paused {
    fn now(&self) -> Option<Duration> {
        if self.armed.swap(false, SeqCst) {
            self.barrier.wait();
            self.barrier.wait();
            self.order.lock().unwrap().push("check decided");
        }
        Some(Duration::ZERO)
    }
}

#[test]
fn a_retirement_waits_for_a_check_deciding_and_every_check_after_it_is_refused() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let clock = Arc::new(Paused {
        barrier: Barrier::new(2),
        armed: std::sync::atomic::AtomicBool::new(false),
        order: order.clone(),
    });
    let hook = {
        let order = order.clone();
        Box::new(move |_, _| order.lock().unwrap().push("retired")) as Hook
    };
    let authority = Authority::new(Generation(2), clock.clone(), hook);
    clock.armed.store(true, SeqCst);
    let checking = {
        let authority = authority.clone();
        std::thread::spawn(move || authority.check())
    };
    clock.barrier.wait();
    // The check is deciding, holding the lock retiring takes.
    assert!(
        authority.shared.state.try_lock().is_err(),
        "the check decides outside the lock"
    );
    let started = Arc::new(Barrier::new(2));
    let retiring = {
        let (authority, started) = (authority.clone(), started.clone());
        std::thread::spawn(move || {
            started.wait();
            authority.retire(Retired::Replaced)
        })
    };
    started.wait();
    clock.barrier.wait();

    assert!(checking.join().unwrap().is_ok());
    assert!(retiring.join().unwrap());
    assert_eq!(*order.lock().unwrap(), ["check decided", "retired"]);
    assert!(authority.check().is_err());
}

#[test]
fn a_registration_under_way_is_there_for_a_retirement_that_comes_during_it() {
    let registered = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let hook = {
        let (registered, seen) = (registered.clone(), seen.clone());
        Box::new(move |_, _| *seen.lock().unwrap() = registered.lock().unwrap().clone()) as Hook
    };
    let authority = Authority::new(Generation(4), Arc::new(Manual::default()), hook);
    let inside = Arc::new(Barrier::new(2));
    let registering = {
        let (authority, registered, inside) =
            (authority.clone(), registered.clone(), inside.clone());
        std::thread::spawn(move || {
            authority.while_live(|| {
                inside.wait();
                inside.wait();
                registered.lock().unwrap().push(1);
                Ok(())
            })
        })
    };
    inside.wait();
    // Registering, before it inserts anything, holds the lock retiring
    // takes: a retirement can only come before or after it.
    assert!(
        authority.shared.state.try_lock().is_err(),
        "registration runs outside the lock"
    );
    let retiring = {
        let authority = authority.clone();
        std::thread::spawn(move || authority.retire(Retired::ControlLost))
    };
    inside.wait();

    registering.join().unwrap().unwrap();
    retiring.join().unwrap();
    assert_eq!(
        *seen.lock().unwrap(),
        [1],
        "the retirement missed what was registered"
    );
}

#[test]
fn what_was_registered_while_live_is_there_for_the_retirement_and_nothing_after() {
    let registered = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let hook = {
        let (registered, seen) = (registered.clone(), seen.clone());
        Box::new(move |_, _| *seen.lock().unwrap() = registered.lock().unwrap().clone()) as Hook
    };
    let authority = Authority::new(Generation(4), Arc::new(Manual::default()), hook);

    authority
        .while_live(|| {
            registered.lock().unwrap().push(1);
            Ok(())
        })
        .unwrap();
    authority.retire(Retired::ControlLost);
    let after = authority.while_live(|| {
        registered.lock().unwrap().push(2);
        Ok(())
    });

    assert!(after.is_err());
    assert_eq!(*seen.lock().unwrap(), [1]);
    assert_eq!(*registered.lock().unwrap(), [1]);
}

#[test]
fn a_heartbeat_is_taken_under_the_lock_a_check_decides_under() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let clock = Arc::new(Paused {
        barrier: Barrier::new(2),
        armed: std::sync::atomic::AtomicBool::new(false),
        order,
    });
    let authority = Authority::new(Generation(3), clock.clone(), Box::new(|_, _| {}));
    clock.armed.store(true, SeqCst);
    let hearing = {
        let authority = authority.clone();
        std::thread::spawn(move || authority.heard())
    };
    clock.barrier.wait();

    assert!(
        authority.shared.state.try_lock().is_err(),
        "a heartbeat is renewed outside the lock"
    );

    clock.barrier.wait();
    hearing.join().unwrap();
    authority.check().unwrap();
}

#[tokio::test]
async fn work_waiting_on_a_generation_hears_it_retire_for_any_cause() {
    let (replaced, _, _) = held();
    let waiting = tokio::spawn({
        let replaced = replaced.clone();
        async move { replaced.ended().await }
    });
    tokio::task::yield_now().await;
    assert!(!waiting.is_finished());
    replaced.retire(Retired::Replaced);
    tokio::time::timeout(std::time::Duration::from_secs(5), waiting)
        .await
        .unwrap()
        .unwrap();

    // A lapse found by a check, as the watchdog finds it.
    let (lapsed, clock, _) = held();
    clock.advance(LEASE * 2);
    assert!(lapsed.check().is_err());
    tokio::time::timeout(std::time::Duration::from_secs(5), lapsed.ended())
        .await
        .unwrap();
}
