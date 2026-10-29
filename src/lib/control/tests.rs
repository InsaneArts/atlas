//! The controller's tests: the lock and waiting for it, the marker and the
//! claims, executor records and the viewer's lease. Commands agents started
//! with `run` and the labels beside the lock and the marker have their own.
mod commands;
mod notes;

use super::*;
use super::{
    busy::{busy_agent, busy_person},
    executor::{
        EXECUTOR_WRITING, Executor, executor_boot, executor_boot_with, executor_running,
        executor_running_with, executor_start, publish_executor_with, record_executor_with,
    },
    wait::poll_with,
};
use crate::{error::Holder, files::token};
use std::io::{Seek, SeekFrom, Write};
use tokio::sync::Mutex;

/// What agents and people read gives the default wait the code uses.
#[test]
fn the_documented_default_wait_is_the_one_used() {
    let said = format!("up to {} s", CONTROL_WAIT.as_secs());
    for (file, text) in [
        (
            "limits.md",
            include_str!("../../../agents/atlas/skills/atlas/references/limits.md"),
        ),
        (
            "troubleshooting.md",
            include_str!("../../../agents/atlas/skills/atlas/references/troubleshooting.md"),
        ),
        ("docs/cli.md", include_str!("../../../docs/cli.md")),
        (
            "docs/troubleshooting.md",
            include_str!("../../../docs/troubleshooting.md"),
        ),
        (
            "docs/architecture.md",
            include_str!("../../../docs/architecture.md"),
        ),
    ] {
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(text.contains(&said), "{file} does not say {said}");
    }
}

fn pending_counts(controller: &Controller) -> (usize, usize, usize) {
    let (pending, total) = crate::overview::pending(controller);
    (total, pending.unresolved, pending.in_flight)
}

#[test]
fn reporting_pending_claims_never_settles_them_or_admits_control() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let observer = Controller::at(505, directory.path());
    let report = |expected| {
        let before = fs::read(&controller.marker).ok();
        assert_eq!(pending_counts(&observer), expected);
        assert_eq!(fs::read(&controller.marker).ok(), before);
    };

    report((0, 0, 0));
    let abandoned = controller.quarantine().unwrap();
    let generation = abandoned.to_string();
    report((1, 0, 1));
    assert!(observer.unresolved().unwrap().is_empty());
    assert_eq!(observer.acquire(false).unwrap_err().code, "uncertain");

    drop(abandoned);
    report((1, 1, 0));
    assert_eq!(observer.unresolved().unwrap(), vec![generation.clone()]);
    assert_eq!(observer.acquire(false).unwrap_err().code, "uncertain");

    let working = controller.quarantine().unwrap();
    report((2, 1, 1));
    assert_eq!(observer.unresolved().unwrap(), vec![generation.clone()]);
    controller.clear_if(&working).unwrap();
    drop(working);
    report((1, 1, 0));
    assert_eq!(observer.pending().unwrap(), vec![generation]);
    assert_eq!(observer.acquire(false).unwrap_err().code, "uncertain");

    controller.clear().unwrap();
    report((0, 0, 0));
    assert!(observer.acquire(false).is_ok());
}

#[test]
fn reporting_empty_legacy_and_malformed_markers_preserves_their_contents() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    for (contents, generations) in [
        ("[]", vec![]),
        ("not JSON", vec!["legacy-unknown"]),
        (
            r#"["1234.legacy","5678.legacy"]"#,
            vec!["1234.legacy", "5678.legacy"],
        ),
    ] {
        fs::write(&controller.marker, contents).unwrap();
        assert_eq!(
            pending_counts(&controller),
            (generations.len(), generations.len(), 0)
        );
        assert_eq!(controller.pending().unwrap(), generations);
        assert_eq!(controller.unresolved().unwrap(), generations);
        assert_eq!(fs::read_to_string(&controller.marker).unwrap(), contents);
        assert_eq!(controller.acquire(false).unwrap_err().code, "uncertain");
    }
}

#[test]
fn unreadable_markers_and_unopenable_locks_are_reported_as_unresolved() {
    for marker_fails in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, directory.path());
        let blocked = if marker_fails {
            &controller.marker
        } else {
            &controller.marker_lock
        };
        fs::create_dir(blocked).unwrap();
        assert!(controller.pending().is_err());
        assert!(controller.unresolved().is_err());
        assert_eq!(pending_counts(&controller), (1, 1, 0));
        assert!(blocked.is_dir());
    }
}

#[test]
fn dropping_ownership_releases_control_while_a_duplicate_descriptor_stays_open() {
    if !crate::test_support::isolated(
        "control::tests::dropping_ownership_releases_control_while_a_duplicate_descriptor_stays_open",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let ownership = controller.acquire_for_person().unwrap();
    let duplicate = ownership._lock.try_clone().unwrap();
    assert_eq!(controller.acquire(false).unwrap_err().code, "busy");

    drop(ownership);

    assert!(!controller.person.exists());
    let _next = controller.acquire(false).unwrap();
    assert!(duplicate.metadata().is_ok());
}

#[test]
fn a_privileged_executor_is_identified_and_keeps_recovery_excluded() {
    let dir = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, dir.path());
    let pending = controller.quarantine().unwrap();
    let claims = ExecutorClaims::new(std::slice::from_ref(&pending)).unwrap();
    // launchd is a stable, root-owned process. Observe it only: this test
    // never dispatches privileged work or changes a real helper's state.
    let pid = nix::unistd::Pid::from_raw(1);
    record_executor(pid).unwrap();
    let live = pending.live.clone();
    let record = live.with_extension("executor.live");
    let executor: Executor = serde_json::from_slice(&fs::read(&record).unwrap()).unwrap();
    assert_eq!(executor.pid, 1);
    assert_eq!(executor.boot, executor_boot().unwrap());
    assert_eq!(executor.start, executor_start(1).unwrap());
    assert!(executor.start[0] > 0);
    drop(claims);
    drop(pending);
    assert!(live.exists(), "the executor outlives its caller's claim");
    assert!(executor_running(&File::open(record).unwrap()));
    assert_eq!(pending_counts(&controller), (1, 0, 1));
    assert_eq!(
        controller.acquire(true).unwrap_err().code,
        "busy",
        "recovery stays excluded"
    );
}

#[test]
fn an_exited_executor_has_no_start_identity() {
    let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
    let pid = child.id() as i32;
    assert!(child.wait().unwrap().success());
    assert!(executor_start(pid).is_err());
}

#[test]
fn executor_claims_persist_the_pid_boot_and_process_start_before_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, dir.path());
    let pending = controller.quarantine().unwrap();
    let _claims = ExecutorClaims::new(std::slice::from_ref(&pending)).unwrap();
    let pid = nix::unistd::getpid();
    record_executor_with(
        pid,
        || Ok("boot-session".into()),
        |observed| {
            assert_eq!(observed, pid.as_raw());
            Ok([300, 400])
        },
    )
    .unwrap();
    assert!(pending.names_own_file());
    let executor: Executor =
        serde_json::from_slice(&fs::read(pending.live.with_extension("executor.live")).unwrap())
            .unwrap();
    assert_eq!(executor.pid, pid.as_raw());
    assert_eq!(executor.boot, "boot-session");
    assert_eq!(executor.start, [300, 400]);
}

#[test]
fn a_reader_of_the_claim_file_sees_a_running_executor() {
    let dir = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, dir.path());
    let pending = controller.quarantine().unwrap();
    let _claims = ExecutorClaims::new(std::slice::from_ref(&pending)).unwrap();
    record_executor_with(
        nix::unistd::getpid(),
        || Ok("boot-session".into()),
        |_| Ok([300, 400]),
    )
    .unwrap();
    assert!(executor_running_with(
        &File::open(&pending.live).unwrap(),
        || Ok("boot-session".into()),
        |_| Ok([300, 400]),
    ));
}

#[test]
fn a_reused_executor_pid_is_not_alive_on_a_different_boot_or_start_time() {
    let mut file = tempfile::tempfile().unwrap();
    let executor = Executor {
        pid: nix::unistd::getpid().as_raw(),
        boot: "boot-session".into(),
        start: [300, 400],
    };
    file.write_all(&serde_json::to_vec(&executor).unwrap())
        .unwrap();
    assert!(executor_running_with(
        &file,
        || Ok(executor.boot.clone()),
        |pid| {
            assert_eq!(pid, executor.pid);
            Ok(executor.start)
        }
    ));
    for changed in ["boot", "start"] {
        let mut boot = executor.boot.clone();
        let mut start = executor.start;
        match changed {
            "boot" => boot.push('2'),
            _ => start[1] += 1,
        }
        assert!(
            !executor_running_with(
                &file,
                || Ok(boot),
                |pid| {
                    assert_eq!(pid, executor.pid);
                    Ok(start)
                }
            ),
            "different {changed}"
        );
    }
    assert!(
        executor_running_with(
            &file,
            || Err(Error::new("state", "unreadable boot")),
            |_| Err(Error::new("state", "unreadable process")),
        ),
        "unreadable observations do not release a live executor"
    );
    assert!(executor_running_with(
        &file,
        || Err(Error::new("state", "unreadable boot")),
        |_| Ok([0, 0]),
    ));
    assert!(executor_running_with(
        &file,
        || Ok("different boot".into()),
        |_| Err(Error::new("state", "unreadable process")),
    ));
}

#[test]
fn a_clock_change_does_not_release_a_surviving_executor() {
    let dir = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, dir.path());
    let pending = controller.quarantine().unwrap();
    let _claims = ExecutorClaims::new(std::slice::from_ref(&pending)).unwrap();
    let boottime = std::cell::Cell::new(100);
    let read = |name: &std::ffi::CStr| match name.to_bytes() {
        b"kern.bootsessionuuid" => Ok("same-boot-session".into()),
        b"kern.boottime" => Ok(boottime.get().to_string()),
        _ => panic!("unexpected sysctl"),
    };
    record_executor_with(
        nix::unistd::getpid(),
        || executor_boot_with(read),
        |_| Ok([300, 400]),
    )
    .unwrap();
    boottime.set(200);
    assert!(executor_running_with(
        &File::open(pending.live.with_extension("executor.live")).unwrap(),
        || executor_boot_with(read),
        |_| Ok([300, 400]),
    ));
}

#[test]
fn unreadable_and_legacy_executor_records_do_not_release_a_live_process() {
    let mut file = tempfile::tempfile().unwrap();
    for record in [nix::unistd::getpid().to_string(), "{".into(), "-1".into()] {
        file.set_len(0).unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(record.as_bytes()).unwrap();
        assert!(executor_running(&file));
    }
}

#[test]
fn an_exited_executor_releases_its_claim_when_its_identity_is_unavailable() {
    let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
    let pid = child.id() as i32;
    assert!(child.wait().unwrap().success());
    let mut file = tempfile::tempfile().unwrap();
    let executor = Executor {
        pid,
        boot: "boot-session".into(),
        start: [300, 400],
    };
    file.write_all(&serde_json::to_vec(&executor).unwrap())
        .unwrap();
    assert!(!executor_running_with(
        &file,
        || Err(Error::new("state", "unreadable boot")),
        |_| Err(Error::new("state", "process no longer exists")),
    ));
}

#[test]
fn a_partial_executor_record_does_not_block_recovery_after_its_executor_and_boot_are_gone() {
    if !crate::test_support::isolated(
        "control::tests::a_partial_executor_record_does_not_block_recovery_after_its_executor_and_boot_are_gone",
    ) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, dir.path());
    let pending = controller.quarantine().unwrap();
    let generation = pending.to_string();
    let live = pending.live.clone();
    let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
    let pid = child.id() as i32;
    assert!(child.wait().unwrap().success());
    let record = serde_json::to_vec(&Executor {
        pid,
        boot: "previous-boot-session".into(),
        start: [300, 400],
    })
    .unwrap();
    publish_executor_with(&live, &record, |file, record| file.write_all(record)).unwrap();
    assert!(
        publish_executor_with(&live, &record, |file, record| {
            file.write_all(&record[..record.len() / 2])?;
            file.sync_all()?;
            Err(std::io::Error::other("interrupted write"))
        })
        .is_err()
    );
    FileExt::unlock(&pending.lock).unwrap();

    assert_eq!(controller.pending().unwrap(), vec![generation]);
    assert!(live.exists());
    let leftover = live.with_extension(format!("{EXECUTOR_WRITING}{}", token()));
    fs::write(&leftover, &record[..record.len() / 2]).unwrap();
    let _ownership = controller.acquire(true).unwrap();
    controller.clear().unwrap();
    assert!(controller.pending().unwrap().is_empty());
    assert!(!live.exists());
    assert!(!live.with_extension("executor.live").exists());
    assert!(!leftover.exists(), "a killed writer's record is swept");
}

#[test]
fn interrupted_claim_cleanup_leaves_recovery_unblocked_despite_a_malformed_record() {
    let dir = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, dir.path());
    let pending = controller.quarantine().unwrap();
    let generation = pending.to_string();
    let live = pending.live.clone();
    let sidecar = live.with_extension("executor.live");
    let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
    let pid = child.id() as i32;
    assert!(child.wait().unwrap().success());
    let record = serde_json::to_vec(&Executor {
        pid,
        boot: "previous-boot-session".into(),
        start: [300, 400],
    })
    .unwrap();
    fs::write(&live, b"{").unwrap();
    fs::write(&sidecar, &record).unwrap();

    faults::interrupt_claim_removal();
    assert!(std::panic::catch_unwind(|| drop(pending)).is_err());

    assert!(!live.exists());
    assert_eq!(fs::read(&sidecar).unwrap(), record);
    assert_eq!(
        controller.pending().unwrap(),
        std::slice::from_ref(&generation)
    );
    assert_eq!(controller.unresolved().unwrap(), [generation]);
    let ownership = controller.acquire(true).unwrap();
    controller.clear().unwrap();
    assert!(controller.pending().unwrap().is_empty());
    assert!(sidecar.exists());
    drop(ownership);
    assert!(controller.acquire(false).is_ok());
}

#[test]
fn a_partial_executor_record_keeps_a_possibly_live_executor_pending() {
    if !crate::test_support::isolated(
        "control::tests::a_partial_executor_record_keeps_a_possibly_live_executor_pending",
    ) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, dir.path());
    let pending = controller.quarantine().unwrap();
    let generation = pending.to_string();
    let live = pending.live.clone();
    let record = nix::unistd::getpid().to_string().into_bytes();
    fs::write(&live, &record).unwrap();
    let next = serde_json::to_vec(&Executor {
        pid: nix::unistd::getpid().as_raw(),
        boot: "boot-session".into(),
        start: [300, 400],
    })
    .unwrap();
    assert!(
        publish_executor_with(&live, &next, |file, record| {
            file.write_all(&record[..record.len() / 2])?;
            file.sync_all()?;
            Err(std::io::Error::other("interrupted write"))
        })
        .is_err()
    );
    assert!(pending.names_own_file());
    drop(pending);

    assert_eq!(controller.acquire(true).unwrap_err().code, "busy");
    controller.clear().unwrap();
    assert_eq!(controller.pending().unwrap(), vec![generation]);
    assert!(live.exists());
    assert_eq!(fs::read(live).unwrap(), record);
}

/// Checking that the lock can be kept creates its directory, leaves no
/// file behind and never touches the lock; a place that cannot be
/// written says so.
#[test]
fn writable_leaves_the_directory_as_it_found_it() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, &directory.path().join("Atlas"));
    controller.writable().unwrap();

    assert_eq!(fs::read_dir(&controller.directory).unwrap().count(), 0);
    controller.acquire(true).unwrap();

    let unwritable = Controller::at(505, Path::new("/dev/null/Atlas"));
    assert_eq!(unwritable.writable().unwrap_err().code, "io");
}

#[test]
fn a_state_folder_that_is_a_link_is_refused_before_any_lock_or_record() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("elsewhere");
    fs::create_dir(&target).unwrap();
    let link = dir.path().join("Atlas");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let controller = Controller::at(505, &link);

    assert_eq!(controller.acquire(true).unwrap_err().code, "state");
    assert_eq!(controller.quarantine().unwrap_err().code, "state");
    assert_eq!(controller.pending().unwrap_err().code, "state");
    assert_eq!(controller.writable().unwrap_err().code, "state");
    assert_eq!(controller.note_stop(false).unwrap_err().code, "state");
    assert_eq!(controller.note_stop(true).unwrap_err().code, "state");
    assert_eq!(
        fs::read_dir(&target).unwrap().count(),
        0,
        "nothing kept through the link"
    );
}

#[test]
fn recovery_never_settles_a_mutation_still_being_carried_out() {
    let dir = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, &dir.path().join("Atlas"));
    let lost = controller.quarantine().unwrap();
    drop(lost);
    let running = controller.quarantine().unwrap();

    assert_eq!(controller.acquire(true).unwrap_err().code, "busy");

    // Recovery from another process, while the run goes on.
    Controller::at(505, &dir.path().join("Atlas"))
        .clear()
        .unwrap();
    assert_eq!(
        controller.pending().unwrap(),
        [running.generation().to_owned()]
    );
    assert!(controller.require_known().is_err());

    controller.clear_if(&running).unwrap();
    drop(running);
    assert!(controller.pending().unwrap().is_empty());
}

/// While the person holds control through a viewer, an agent finding
/// the controller busy is told to wait for them; once they let go, or
/// when the note outlived a viewer that ended, nobody is.
#[test]
fn an_agent_finding_the_person_in_control_is_told_to_wait_for_them() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, &directory.path().join("Atlas"));

    let person = controller.acquire_for_person().unwrap();
    let refused = controller.acquire(false).err().unwrap();
    assert_eq!(refused.code, "busy");
    assert_eq!(refused.error, crate::error::PERSON_HAS_CONTROL);
    assert_eq!(refused.holder, Some(Holder::Person));
    assert!(refused.details.get("waited").is_none(), "never waited");
    drop(person);
    assert!(
        !controller.person.exists(),
        "the note goes with the control"
    );

    // A child another test is starting may hold the lock's file a moment
    // past its release, until it runs its own program.
    let agent = (0..50)
        .find_map(|_| {
            controller.acquire(false).ok().or_else(|| {
                std::thread::sleep(Duration::from_millis(20));
                None
            })
        })
        .expect("the lock comes free");
    let refused = controller.acquire(false).err().unwrap();
    assert_ne!(
        refused.error,
        crate::error::PERSON_HAS_CONTROL,
        "another agent is not the person"
    );
    assert_eq!(refused.holder, Some(Holder::Agent));
    drop(agent);

    fs::write(&controller.person, b"").unwrap();
    drop(controller.acquire(false).unwrap());
    assert!(!controller.person.exists(), "a stale note is cleared");
}

/// `controller` given `wait`, and what its hook hears, in order.
fn watched(
    controller: &Controller,
    wait: Duration,
) -> (Controller, std::sync::mpsc::Receiver<WaitEvent>) {
    let (tell, events) = std::sync::mpsc::channel();
    let watched = controller.clone().waiting(wait).on_wait(move |event| {
        let _ = tell.send(event);
    });
    (watched, events)
}

/// The next thing a watched wait tells, within five seconds.
fn told(events: &std::sync::mpsc::Receiver<WaitEvent>) -> WaitEvent {
    events.recv_timeout(Duration::from_secs(5)).unwrap()
}

/// Acquire on another thread, answering with when it returned.
fn acquiring(
    controller: Controller,
    recovery: bool,
) -> std::thread::JoinHandle<(Result<()>, Instant)> {
    std::thread::spawn(move || {
        let outcome = controller.acquire(recovery).map(drop);
        (outcome, Instant::now())
    })
}

/// Wait for `path` to appear, for at most five seconds.
fn appears(path: &Path) {
    let started = Instant::now();
    while !path.exists() {
        assert!(started.elapsed() < Duration::from_secs(5), "{path:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Stand in for another Atlas command, as the test `test` run again in
/// a process of its own: it takes control of the helper kept in
/// `directory`, marks a change and says so in `held`.
fn holder(test: &str, directory: &Path, hold: Duration) -> std::process::Child {
    crate::test_support::rerun(test)
        .arg("--nocapture")
        .env("ATLAS_CONTROL_TEST_HOLDER", directory)
        .env("ATLAS_CONTROL_TEST_HOLD_MS", hold.as_millis().to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap()
}

/// The other command [`holder`] starts, when this process is it: it
/// keeps control and its change for the time it was given, settles the
/// change, says so in `released` and exits still in control, so the
/// lock goes only with the process.
fn hold_if_holder() {
    let Some(directory) = std::env::var_os("ATLAS_CONTROL_TEST_HOLDER") else {
        return;
    };
    let directory = PathBuf::from(directory);
    let hold = std::env::var("ATLAS_CONTROL_TEST_HOLD_MS")
        .unwrap()
        .parse()
        .unwrap();
    let controller = Controller::at(505, &directory.join("state"));
    let ownership = controller.acquire(false).unwrap();
    let change = controller.quarantine().unwrap();
    fs::write(directory.join("held"), b"").unwrap();
    std::thread::sleep(Duration::from_millis(hold));
    controller.clear_if(&change).unwrap();
    drop(change);
    fs::write(directory.join("released"), b"").unwrap();
    std::mem::forget(ownership);
    std::process::exit(0);
}

#[test]
fn waiting_for_control_takes_the_lock_once_the_holder_lets_go() {
    if !crate::test_support::isolated(
        "control::tests::waiting_for_control_takes_the_lock_once_the_holder_lets_go",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let held = controller.acquire(false).unwrap();
    let (waiter, events) = watched(&controller, Duration::from_secs(5));
    let waiting = acquiring(waiter, false);

    assert_eq!(told(&events), WaitEvent::Began);
    std::thread::sleep(Duration::from_millis(300));
    let released = Instant::now();
    drop(held);
    let (outcome, taken) = waiting.join().unwrap();
    outcome.unwrap();
    assert!(taken >= released);
    assert!(taken - released < Duration::from_millis(500));
    assert_eq!(told(&events), WaitEvent::Ended);
}

#[test]
fn two_processes_that_change_the_helper_both_succeed_one_after_the_other() {
    const TEST: &str =
        "control::tests::two_processes_that_change_the_helper_both_succeed_one_after_the_other";
    hold_if_holder();
    if !crate::test_support::isolated(TEST) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, &directory.path().join("state"));
    let mut child = holder(TEST, directory.path(), Duration::from_millis(400));
    appears(&directory.path().join("held"));
    assert_eq!(controller.acquire(false).unwrap_err().code, "busy");

    let ownership = controller
        .clone()
        .waiting(Duration::from_secs(5))
        .acquire(false)
        .unwrap();
    assert!(
        directory.path().join("released").exists(),
        "control came only after the first command let go"
    );
    let change = controller.quarantine().unwrap();
    controller.clear_if(&change).unwrap();
    drop(change);
    drop(ownership);

    assert!(child.wait().unwrap().success());
    assert!(controller.pending().unwrap().is_empty());
}

#[test]
fn waiting_gives_up_with_busy_after_its_wait_and_says_how_long() {
    if !crate::test_support::isolated(
        "control::tests::waiting_gives_up_with_busy_after_its_wait_and_says_how_long",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let _held = controller.acquire(false).unwrap();
    let (waiter, events) = watched(&controller, Duration::from_secs(1));

    let started = Instant::now();
    let refused = waiter.acquire(false).unwrap_err();
    let waited = started.elapsed();
    assert!(waited >= Duration::from_secs(1), "{waited:?}");
    assert!(waited < Duration::from_millis(1500), "{waited:?}");
    assert_eq!(refused.code, "busy");
    assert_eq!(refused.holder, Some(Holder::Agent));
    assert!(
        refused.error.contains("all of the 1 seconds"),
        "{}",
        refused.error
    );
    assert_eq!(refused.details["waited"], 1);
    assert_eq!(refused.details["next"], crate::ui::hints::WAITED);
    assert_eq!(told(&events), WaitEvent::Began);
    assert_eq!(told(&events), WaitEvent::Ended);
}

/// A wait that used all of its time ends a little short of it, after
/// what the command did first: it says as many seconds as its message.
#[test]
fn a_full_wait_says_the_seconds_its_message_does() {
    let refused = busy_agent(Some(Duration::from_millis(29_600)), Duration::from_secs(30));
    assert_eq!(refused.details["waited"], 30);
    assert!(
        refused.error.contains("all of the 30 seconds"),
        "{}",
        refused.error
    );

    assert_eq!(
        busy_person(Some(Duration::from_millis(1_400))).details["waited"],
        1
    );
    assert_eq!(seconds(Duration::from_millis(499)), 0);
    assert_eq!(seconds(Duration::from_millis(500)), 1);
    assert_eq!(seconds(Duration::MAX), u64::MAX);
}

#[test]
fn one_wait_is_shared_by_every_acquire_of_a_command() {
    if !crate::test_support::isolated(
        "control::tests::one_wait_is_shared_by_every_acquire_of_a_command",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let _held = controller.acquire(false).unwrap();
    let waiter = controller.clone().waiting(Duration::from_secs(1));

    assert_eq!(waiter.acquire(false).unwrap_err().code, "busy");
    assert_eq!(waiter.remaining(), Duration::ZERO);
    for again in [waiter.clone(), waiter] {
        let started = Instant::now();
        let refused = again.acquire(false).unwrap_err();
        assert!(started.elapsed() < Duration::from_millis(50));
        assert_eq!(refused.details["waited"], 0);
        assert_eq!(refused.details["next"], crate::ui::hints::WAITED);
    }
}

#[test]
fn a_wait_of_zero_answers_busy_at_once() {
    if !crate::test_support::isolated("control::tests::a_wait_of_zero_answers_busy_at_once") {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let _held = controller.acquire(false).unwrap();
    let (waiter, events) = watched(&controller, Duration::ZERO);

    let started = Instant::now();
    let refused = waiter.acquire(false).unwrap_err();
    assert!(started.elapsed() < Duration::from_millis(50));
    assert_eq!(refused.code, "busy");
    assert_eq!(refused.holder, Some(Holder::Agent));
    assert_eq!(
        refused.error,
        "Another Atlas controller is active: wait for it to finish, or release it in the viewer."
    );
    assert!(
        refused.details.is_empty(),
        "the reply says nothing of a wait"
    );
    assert!(events.try_recv().is_err(), "nothing waited for");
}

#[test]
fn the_person_in_control_is_never_waited_for() {
    if !crate::test_support::isolated("control::tests::the_person_in_control_is_never_waited_for") {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let _person = controller.acquire_for_person().unwrap();
    let (waiter, events) = watched(&controller, Duration::from_secs(5));

    let started = Instant::now();
    let refused = waiter.acquire(false).unwrap_err();
    assert!(started.elapsed() < Duration::from_millis(50));
    assert_eq!(refused.code, "busy");
    assert_eq!(refused.error, crate::error::PERSON_HAS_CONTROL);
    assert_eq!(refused.holder, Some(Holder::Person));
    assert_eq!(refused.details["waited"], 0);
    assert!(events.try_recv().is_err(), "nothing waited for");
}

#[test]
fn the_person_taking_control_during_a_wait_ends_it_at_once() {
    if !crate::test_support::isolated(
        "control::tests::the_person_taking_control_during_a_wait_ends_it_at_once",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let _held = controller.acquire(false).unwrap();
    let (waiter, events) = watched(&controller, Duration::from_secs(5));
    let waiting = std::thread::spawn(move || {
        let refused = waiter.acquire(false).unwrap_err();
        (refused, Instant::now())
    });

    assert_eq!(told(&events), WaitEvent::Began);
    std::thread::sleep(Duration::from_millis(1100));
    let noted = Instant::now();
    fs::write(&controller.person, b"").unwrap();
    let (refused, answered) = waiting.join().unwrap();
    assert!(answered - noted < Duration::from_millis(300));
    assert_eq!(refused.code, "busy");
    assert_eq!(refused.holder, Some(Holder::Person));
    assert_eq!(refused.details["waited"], 1, "seen partway through");
    assert_eq!(told(&events), WaitEvent::Ended);
}

#[test]
fn a_stop_noted_before_or_during_a_wait_ends_it_at_once() {
    if !crate::test_support::isolated(
        "control::tests::a_stop_noted_before_or_during_a_wait_ends_it_at_once",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let held = controller.acquire(false).unwrap();
    let (waiter, events) = watched(&controller, Duration::from_secs(5));
    let waiter = waiter.heeding_stop();

    controller.note_stop(true).unwrap();
    let started = Instant::now();
    assert_eq!(waiter.acquire(false).unwrap_err().code, "stopped");
    assert!(started.elapsed() < Duration::from_millis(50));
    assert!(events.try_recv().is_err(), "nothing waited for");
    let refused = controller
        .clone()
        .heeding_stop()
        .acquire(false)
        .unwrap_err();
    assert_eq!(refused.code, "busy", "no wait, today's answer");
    controller.note_stop(false).unwrap();

    let waiting = std::thread::spawn(move || {
        let refused = waiter.acquire(false).unwrap_err();
        (refused, Instant::now())
    });
    assert_eq!(told(&events), WaitEvent::Began);
    let noted = Instant::now();
    controller.note_stop(true).unwrap();
    let (refused, answered) = waiting.join().unwrap();
    assert!(answered - noted < Duration::from_millis(300));
    assert_eq!(refused.code, "stopped");
    assert_eq!(refused.error, crate::error::STOPPED);
    assert_eq!(told(&events), WaitEvent::Ended);

    // What a Stop does not refuse, such as recovery, keeps waiting for the lock.
    let (recovery, events) = watched(&controller, Duration::from_secs(5));
    let waiting = acquiring(recovery, true);
    assert_eq!(told(&events), WaitEvent::Began);
    drop(held);
    waiting.join().unwrap().0.unwrap();
}

#[test]
fn a_stop_noted_as_the_holder_lets_go_refuses_the_control_a_wait_takes() {
    if !crate::test_support::isolated(
        "control::tests::a_stop_noted_as_the_holder_lets_go_refuses_the_control_a_wait_takes",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let held = controller.acquire(false).unwrap();
    let (began, waits) = std::sync::mpsc::channel();
    let (go, gone) = std::sync::mpsc::channel::<()>();
    let gone = std::sync::Mutex::new(gone);
    // The waiter's next try comes only once the Stop is noted and the
    // lock let go, so that try takes it.
    let waiter = controller
        .clone()
        .waiting(Duration::from_secs(5))
        .heeding_stop()
        .on_wait(move |event| {
            if event == WaitEvent::Began {
                began.send(()).unwrap();
                gone.lock().unwrap().recv().unwrap();
            }
        });
    let waiting = acquiring(waiter, false);

    waits.recv_timeout(Duration::from_secs(5)).unwrap();
    controller.note_stop(true).unwrap();
    drop(held);
    go.send(()).unwrap();
    assert_eq!(waiting.join().unwrap().0.unwrap_err().code, "stopped");

    controller.note_stop(false).unwrap();
    assert!(
        controller.acquire(false).is_ok(),
        "the refused lock was let go"
    );
}

#[test]
fn a_holder_killed_while_others_wait_hands_control_on_at_once() {
    const TEST: &str = "control::tests::a_holder_killed_while_others_wait_hands_control_on_at_once";
    hold_if_holder();
    if !crate::test_support::isolated(TEST) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, &directory.path().join("state"));
    let mut child = holder(TEST, directory.path(), Duration::from_secs(30));
    appears(&directory.path().join("held"));

    let (waiter, events) = watched(&controller, Duration::from_secs(10));
    let waiting = acquiring(waiter, false);
    assert_eq!(told(&events), WaitEvent::Began);
    let killed = Instant::now();
    child.kill().unwrap();
    let (outcome, answered) = waiting.join().unwrap();
    assert!(answered - killed < Duration::from_millis(500));
    assert!(!child.wait().unwrap().success());
    assert_eq!(
        outcome.unwrap_err().code,
        "uncertain",
        "the killed command's change is unknown"
    );

    let ownership = controller.acquire(true).unwrap();
    controller.clear().unwrap();
    assert!(controller.pending().unwrap().is_empty());
    drop(ownership);
    assert!(controller.acquire(false).is_ok());
}

#[test]
fn recovery_waits_for_the_lock_but_never_for_an_executor_still_running() {
    if !crate::test_support::isolated(
        "control::tests::recovery_waits_for_the_lock_but_never_for_an_executor_still_running",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let held = controller.acquire(false).unwrap();
    let running = controller.quarantine().unwrap();
    let (recovery, events) = watched(&controller, Duration::from_secs(2));
    let waiting = std::thread::spawn(move || {
        let refused = recovery.acquire(true).unwrap_err();
        (refused, Instant::now())
    });

    assert_eq!(told(&events), WaitEvent::Began);
    std::thread::sleep(Duration::from_millis(300));
    let released = Instant::now();
    drop(held);
    let (refused, answered) = waiting.join().unwrap();
    assert!(answered >= released);
    assert_eq!(refused.code, "busy");
    assert!(
        refused.error.contains("executor is still carrying out"),
        "{}",
        refused.error
    );

    controller.clear_if(&running).unwrap();
    drop(running);
    let started = Instant::now();
    drop(
        controller
            .clone()
            .waiting(Duration::from_secs(2))
            .acquire(true)
            .unwrap(),
    );
    assert!(started.elapsed() < Duration::from_millis(50));
}

#[test]
fn a_wait_tells_its_hook_when_it_begins_and_when_it_ends() {
    if !crate::test_support::isolated(
        "control::tests::a_wait_tells_its_hook_when_it_begins_and_when_it_ends",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let (waiter, events) = watched(&controller, Duration::from_secs(5));
    drop(waiter.acquire(false).unwrap());
    assert!(
        events.try_recv().is_err(),
        "taken at once, nothing waited for"
    );

    // Taken after a wait.
    let held = controller.acquire(false).unwrap();
    let waiting = acquiring(waiter, false);
    assert_eq!(told(&events), WaitEvent::Began);
    assert!(events.try_recv().is_err(), "still waiting");
    drop(held);
    let (outcome, _) = waiting.join().unwrap();
    outcome.unwrap();
    assert_eq!(told(&events), WaitEvent::Ended);

    // Refused once the wait ran out.
    let _held = controller.acquire(false).unwrap();
    let (waiter, events) = watched(&controller, Duration::from_millis(300));
    assert_eq!(waiter.acquire(false).unwrap_err().code, "busy");
    assert_eq!(told(&events), WaitEvent::Began);
    assert_eq!(told(&events), WaitEvent::Ended);
    assert!(events.try_recv().is_err(), "told once each");
}

#[test]
fn a_poll_is_about_a_tenth_of_a_second_and_never_past_the_wait() {
    for jitter in 75..=125 {
        assert_eq!(
            poll_with(Duration::from_secs(1), jitter),
            Duration::from_millis(jitter)
        );
    }
    assert_eq!(
        poll_with(Duration::from_millis(30), 100),
        Duration::from_millis(30)
    );
    for _ in 0..100 {
        let slept = poll(Duration::from_secs(1));
        assert!(
            (Duration::from_millis(75)..=Duration::from_millis(125)).contains(&slept),
            "{slept:?}"
        );
    }
}

/// A permit dropped outside any runtime, while the lease is locked
/// elsewhere, waits for it instead of panicking.
#[test]
fn a_permit_settles_without_a_runtime() {
    let lease: SharedLease = Arc::new(Mutex::new(Lease::new(Duration::from_secs(60))));
    let token = token();

    {
        let mut state = lease.blocking_lock();
        state.owner = Some(token.clone());
        state.expires = Instant::now() + Duration::from_secs(60);
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let permit = runtime
        .block_on(ActionPermit::begin(lease.clone(), &token))
        .unwrap();

    drop(runtime);
    let held = lease.clone().blocking_lock_owned();
    let holder = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        drop(held);
    });

    drop(permit);
    holder.join().unwrap();
    assert!(!lease.blocking_lock().inflight);
}

#[test]
fn beginning_a_release_clears_the_retired_token_too() {
    let mut lease = Lease::new(Duration::from_secs(60));
    lease.owner = Some("a".repeat(48));
    lease.expires = Instant::now() + Duration::from_secs(60);
    lease.retired = Some("b".repeat(48));

    assert_eq!(lease.begin_release(None).unwrap(), Some("a".repeat(48)));
    assert_eq!((lease.owner.clone(), lease.retired.clone()), (None, None));

    lease.retired = Some("b".repeat(48));
    assert_eq!(lease.begin_release(None).unwrap(), Some("b".repeat(48)));
    assert_eq!(lease.retired, None);
}
