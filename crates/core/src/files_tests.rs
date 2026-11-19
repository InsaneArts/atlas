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

#[test]
fn a_durable_write_replaces_the_file_whole_with_its_mode_and_syncs_its_folder() {
    use std::os::unix::fs::PermissionsExt;
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("record.json");
    std::fs::write(&path, b"before").unwrap();

    super::write_durable(&path, b"after", 0o600).unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), b"after");
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
    let left: Vec<_> = std::fs::read_dir(folder.path()).unwrap().collect();
    assert_eq!(left.len(), 1, "no replacement left beside it");
}

#[test]
fn a_durable_write_into_a_missing_folder_is_refused() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("missing").join("record.json");

    assert!(super::write_durable(&path, b"kept", 0o600).is_err());
    assert!(!path.exists());
}

/// Run `then` with `folder` writable and searchable but not readable, so
/// names can be made and removed in it while the folder itself cannot be
/// opened to be synced; its mode put back after.
fn unsyncable<T>(folder: &std::path::Path, then: impl FnOnce() -> T) -> T {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o300)).unwrap();
    let done = then();
    std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700)).unwrap();
    done
}

#[test]
fn a_durable_write_whose_folder_cannot_be_synced_fails_though_the_file_may_be_in_place() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("record.json");

    let written = unsyncable(folder.path(), || {
        super::write_durable(&path, b"kept", 0o600)
    });

    let failure = written.unwrap_err();
    assert!(failure.error.contains("Sync the folder of"), "{failure:?}");
    assert_eq!(std::fs::read(&path).unwrap(), b"kept");
}

#[test]
fn a_durable_removal_takes_the_file_away_and_syncs_its_folder() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("record.json");
    std::fs::write(&path, b"kept").unwrap();

    super::remove_durable(&path).unwrap();

    assert!(!path.exists());
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0);
    super::remove_durable(&path).expect("a file already gone counts as removed");
}

#[test]
fn a_durable_removal_whose_folder_cannot_be_synced_fails_and_succeeds_once_asked_again() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("record.json");
    std::fs::write(&path, b"kept").unwrap();

    let removed = unsyncable(folder.path(), || super::remove_durable(&path));

    let failure = removed.unwrap_err();
    assert!(failure.error.contains("Sync the folder of"), "{failure:?}");
    assert!(!path.exists(), "removed, its removal not yet made durable");
    super::remove_durable(&path).expect("synced once its folder can be opened");
}

#[test]
fn a_durable_removal_takes_away_a_link_never_what_it_names_and_never_a_folder() {
    let folder = tempfile::tempdir().unwrap();
    let target = folder.path().join("target.json");
    std::fs::write(&target, b"kept").unwrap();
    let link = folder.path().join("record.json");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let inner = folder.path().join("inner");
    std::fs::create_dir(&inner).unwrap();

    super::remove_durable(&link).unwrap();
    let refused = super::remove_durable(&inner);

    assert!(std::fs::symlink_metadata(&link).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"kept");
    assert!(refused.is_err());
    assert!(inner.is_dir());
}

#[test]
fn a_durable_removal_the_system_does_not_permit_says_it_was_a_removal_and_why() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("record.json");
    std::fs::write(&path, b"kept").unwrap();
    let flag = |flag: &str| {
        crate::spawn::status(
            std::process::Command::new("/usr/bin/chflags")
                .arg(flag)
                .arg(&path),
        )
        .unwrap()
        .success()
    };
    assert!(flag("uchg"), "the file made immutable");

    let removed = super::remove_durable(&path);
    let kept = path.exists();
    assert!(flag("nouchg"), "the file made mutable again");

    let failure = removed.unwrap_err();
    assert_eq!(failure.code, crate::error::Code::Io, "{failure:?}");
    assert!(
        failure
            .error
            .starts_with(&format!("Remove {}: ", path.display())),
        "{failure:?}"
    );
    assert!(failure.error.contains("not permitted"), "{failure:?}");
    assert!(!failure.error.contains("sandbox"), "{failure:?}");
    assert!(
        std::error::Error::source(&failure).is_some(),
        "its cause kept"
    );
    assert!(kept, "an immutable file stays");
}
