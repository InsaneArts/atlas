use super::*;

#[test]
fn an_executor_test_worker() {
    let Some(directory) = super::super::directory() else {
        return;
    };
    executor_ready(&directory, || {
        fs::remove_file(directory.join("opener")).unwrap();
        fs::write(directory.join("changed"), b"").unwrap();
    })
    .unwrap();
}

#[test]
fn an_executor_reports_readiness_before_its_caller_is_killed() {
    interrupted_executor(
        "setup::job::tests::a_job_installation_test_caller",
        "install",
    );
}

#[test]
fn a_surviving_executor_keeps_recovery_excluded_until_its_observed_exit() {
    interrupted_executor(
        "onboarding::tests::an_account_setup_test_caller",
        "interactive",
    );
}

#[test]
fn a_lost_readiness_channel_fails_and_reaps_its_fixture() {
    let directory = tempfile::tempdir().unwrap();
    let mut fixture = Fixture::start(directory.path(), WORKER, "").unwrap();
    fixture.connect().unwrap();
    let identity = fixture.executor.as_ref().unwrap().identity;
    fixture
        .channel
        .as_ref()
        .unwrap()
        .shutdown(Shutdown::Both)
        .unwrap();
    assert!(fixture.ready().is_err());
    drop(fixture);
    assert_eq!(crate::process::info(identity.pid).unwrap(), None);
    assert!(!directory.path().join("changed").exists());
}

#[test]
fn a_failed_assertion_still_releases_and_reaps_its_fixture() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("opener"), b"temporary opener").unwrap();
    let mut fixture = Fixture::start(directory.path(), WORKER, "").unwrap();
    fixture.ready().unwrap();
    let identity = fixture.executor.as_ref().unwrap().identity;
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _fixture = fixture;
        panic!("the fixture's assertion failed");
    }));
    assert!(failed.is_err());
    assert_eq!(crate::process::info(identity.pid).unwrap(), None);
}

#[test]
fn an_abandoned_continuation_cannot_leave_a_test_child() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("opener"), b"temporary opener").unwrap();
    let mut fixture = Fixture::start(directory.path(), WORKER, "").unwrap();
    fixture.ready().unwrap();
    let identity = fixture.executor.as_ref().unwrap().identity;
    drop(fixture);
    assert_eq!(crate::process::info(identity.pid).unwrap(), None);
    assert!(directory.path().join("changed").exists());
}

#[test]
fn a_lost_exit_message_cannot_confirm_settlement() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("opener"), b"temporary opener").unwrap();
    let mut fixture = Fixture::start(directory.path(), WORKER, "").unwrap();
    fixture.ready().unwrap();
    let identity = fixture.executor.as_ref().unwrap().identity;
    fixture
        .channel
        .as_ref()
        .unwrap()
        .shutdown(Shutdown::Both)
        .unwrap();
    assert!(fixture.finish().is_err());
    drop(fixture);
    assert_eq!(crate::process::info(identity.pid).unwrap(), None);
}

#[test]
fn a_failed_fixture_reaps_both_its_worker_and_its_requester() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("opener"), b"temporary opener").unwrap();
    let mut fixture = Fixture::spawn(
        directory.path(),
        rerun(WORKER)
            .env("ATLAS_MUTATION_TEST_DIRECTORY", directory.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()),
    )
    .unwrap();
    fixture.request(&mut Command::new("/bin/cat")).unwrap();
    fixture.ready().unwrap();
    let worker = fixture.executor.as_ref().unwrap().identity;
    let requester = fixture.requester.as_ref().unwrap().id() as i32;
    drop(fixture);
    assert_eq!(crate::process::info(worker.pid).unwrap(), None);
    assert_eq!(crate::process::info(requester).unwrap(), None);
}

#[test]
fn a_fixture_reaps_its_direct_worker_before_confirming_completion() {
    use std::os::fd::OwnedFd;

    let (channel, worker) = UnixStream::pair().unwrap();
    channel.set_read_timeout(Some(LIMIT)).unwrap();
    channel.set_write_timeout(Some(LIMIT)).unwrap();
    let input: OwnedFd = worker.try_clone().unwrap().into();
    let output: OwnedFd = worker.into();
    let caller = Command::new("/bin/sh")
        .args([
            "-c",
            "IFS= read -r message && [ \"$message\" = continue ] && printf 'exited\\n'",
        ])
        .stdin(Stdio::from(input))
        .stdout(Stdio::from(output))
        .spawn()
        .unwrap();
    let pid = caller.id() as i32;
    let identity = Instance::new(pid, crate::process::start(pid).unwrap());
    let mut fixture = Fixture {
        caller,
        requester: None,
        listener: None,
        channel: Some(channel),
        executor: Some(Executor {
            identity,
            exit: Some(
                watch(
                    pid as usize,
                    EventFilter::EVFILT_PROC,
                    FilterFlag::NOTE_EXIT,
                )
                .unwrap(),
            ),
        }),
    };
    fixture.finish().unwrap();
    fixture.wait().unwrap();
    assert_eq!(crate::process::info(pid).unwrap(), None);
}

#[test]
fn a_child_exiting_during_watch_registration_is_still_reaped() -> io::Result<()> {
    for _ in 0..256 {
        let mut fixture = Fixture {
            caller: Command::new("/usr/bin/true").spawn()?,
            requester: None,
            listener: None,
            channel: None,
            executor: None,
        };
        let pid = fixture.caller.id() as i32;
        fixture.wait()?;
        assert_eq!(crate::process::info(pid).map_err(io::Error::other)?, None);
    }
    Ok(())
}

#[test]
fn an_exit_watch_requires_the_owned_process_to_end_and_be_reaped() {
    let caller = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let mut fixture = Fixture {
        caller,
        requester: None,
        listener: None,
        channel: None,
        executor: None,
    };
    let pid = fixture.caller.id() as i32;
    let identity = Instance::new(pid, crate::process::start(pid).unwrap());
    fixture.executor = Some(Executor {
        identity,
        exit: Some(
            watch(
                pid as usize,
                EventFilter::EVFILT_PROC,
                FilterFlag::NOTE_EXIT,
            )
            .unwrap(),
        ),
    });
    assert_eq!(
        observed(
            fixture.executor.as_ref().unwrap().exit.as_ref().unwrap(),
            Duration::ZERO
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::TimedOut
    );
    drop(fixture.caller.stdin.take());
    fixture.executor.as_mut().unwrap().wait().unwrap();
    let started = Instant::now();
    loop {
        let exited = crate::process::info(pid).unwrap().unwrap();
        assert_eq!(exited.instance, identity);
        if exited.zombie {
            break;
        }
        assert!(
            started.elapsed() < LIMIT,
            "executor zombie watchdog expired"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        fixture
            .executor
            .as_mut()
            .unwrap()
            .finish(Duration::from_millis(50))
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut,
        "an observed exit cannot finish while its identity remains unreaped"
    );
    assert!(fixture.caller.wait().unwrap().success());
    fixture.executor.as_mut().unwrap().finish(LIMIT).unwrap();
    assert_eq!(crate::process::info(identity.pid).unwrap(), None);
}
