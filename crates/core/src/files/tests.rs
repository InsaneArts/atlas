use super::{Held, Lock, lock_retrying, lock_within, open_lock};
use std::time::{Duration, Instant};

#[test]
#[allow(
    unsafe_code,
    reason = "The isolated fork child only calls async-signal-safe syscalls"
)]
fn a_lock_is_released_even_when_a_forked_child_inherits_it() {
    use nix::{
        sys::{
            signal::{Signal, kill},
            wait::{WaitPidFlag, WaitStatus, waitpid},
        },
        unistd::{ForkResult, alarm, fork, pause},
    };

    if !crate::test_support::isolated(
        "files::tests::a_lock_is_released_even_when_a_forked_child_inherits_it",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("lock");
    let held = open_lock(&path).unwrap();
    lock_within(&held, Lock::Exclusive, Duration::ZERO).unwrap();
    let next = open_lock(&path).unwrap();
    // SAFETY: The child only calls async-signal-safe alarm and pause;
    // it expires after five seconds even if the parent's cleanup fails.
    let child = match unsafe { fork() }.unwrap() {
        ForkResult::Parent { child } => child,
        ForkResult::Child => {
            alarm::set(5);
            loop {
                pause();
            }
        }
    };

    let refused = lock_within(&next, Lock::Exclusive, Duration::ZERO);
    drop(held);
    let released = lock_within(&next, Lock::Exclusive, Duration::ZERO);

    let killed = kill(child, Signal::SIGKILL);
    let deadline = Instant::now() + Duration::from_secs(2);
    let reaped = loop {
        match waitpid(child, Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(5));
            }
            status => break status,
        }
    };
    killed.unwrap();
    assert_eq!(
        reaped.unwrap(),
        WaitStatus::Signaled(child, Signal::SIGKILL, false)
    );
    assert!(matches!(refused, Err(Held(_))));
    assert!(
        released.is_ok(),
        "the inherited descriptor kept the lock: {released:?}"
    );
}

#[test]
fn a_held_lock_is_refused_once_its_patience_runs_out() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lock");
    let (first, second) = (open_lock(&path).unwrap(), open_lock(&path).unwrap());

    lock_within(&first, Lock::Shared, Duration::ZERO).unwrap();
    lock_within(&second, Lock::Shared, Duration::ZERO).expect("readers share it");
    fs2::FileExt::unlock(&second.0).unwrap();

    let base = Instant::now();
    let deadline = base + Duration::from_millis(30);
    for offset in [0, 29, 30, 31] {
        let first_now = base + Duration::from_millis(offset);
        let mut now = first_now;
        let mut observed = Vec::new();
        let refused = lock_retrying(&second, Lock::Exclusive, deadline, |until| {
            assert_eq!(until, deadline, "all retries share the supplied deadline");
            observed.push(now);
            let retry = now < until;
            now = until;
            retry
        });
        assert!(matches!(refused, Err(Held(_))));
        let expected = if first_now < deadline {
            vec![first_now, deadline]
        } else {
            vec![first_now]
        };
        assert_eq!(observed, expected, "virtual offset {offset}ms");
    }

    fs2::FileExt::unlock(&first.0).unwrap();
    lock_within(&second, Lock::Exclusive, Duration::ZERO).expect("a free lock is taken at once");
}
