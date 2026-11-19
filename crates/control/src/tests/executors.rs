//! Who carries out each pending change: recovery settles the desktop's own
//! services' changes and leaves every other executor's for its answer.
use crate::{
    executors::{self, Executor},
    *,
};
use atlas_core::files::token;

#[test]
fn recovery_leaves_a_change_another_executor_carries_out_for_its_answer() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let own = controller.quarantine().unwrap();
    let theirs = token();
    let vms = controller.quarantine_by(&theirs, Executor::Vm).unwrap();
    let own_generation = own.generation().to_owned();
    drop(own);
    drop(vms);

    controller.clear().unwrap();

    assert_eq!(controller.pending().unwrap(), std::slice::from_ref(&theirs));
    assert!(!controller.pending().unwrap().contains(&own_generation));
    // Its executor's answer settles it.
    controller.clear_if(&theirs).unwrap();
    assert!(controller.pending().unwrap().is_empty());
}

#[test]
fn a_change_the_desktops_own_services_carry_out_is_settled_by_recovery_as_before() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let generation = token();
    drop(
        controller
            .quarantine_by(&generation, Executor::Input)
            .unwrap(),
    );

    controller.clear().unwrap();

    assert!(controller.pending().unwrap().is_empty());
}

#[test]
fn an_executor_record_that_cannot_be_read_settles_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let generation = token();
    drop(
        controller
            .quarantine_by(&generation, Executor::Guest)
            .unwrap(),
    );
    std::fs::write(&controller.executors, b"{").unwrap();

    assert!(controller.clear().is_err());
    assert_eq!(controller.pending().unwrap(), [generation]);
}

#[test]
fn a_marker_that_cannot_be_read_beside_another_executors_change_settles_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    drop(controller.quarantine_by(&token(), Executor::Guest).unwrap());
    std::fs::write(&controller.marker, b"[").unwrap();

    assert!(controller.clear().is_err());
    assert!(controller.marker.exists());
    assert!(!executors::read(&controller.executors).unwrap().is_empty());
}

#[test]
fn a_damaged_executor_record_goes_once_nothing_is_pending_so_recovery_works_again() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let theirs = token();
    drop(controller.quarantine_by(&theirs, Executor::Vm).unwrap());
    std::fs::write(&controller.executors, b"{").unwrap();
    assert!(controller.clear().is_err());

    // Its executor answered: nothing is pending any more.
    controller.clear_if(&theirs).unwrap();
    drop(controller.quarantine().unwrap());

    controller.clear().unwrap();
    assert!(controller.pending().unwrap().is_empty());
}

#[test]
fn settling_the_last_change_again_keeps_the_markers_removal() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let generation = token();
    drop(controller.quarantine_as(&generation).unwrap());
    controller.clear_if(&generation).unwrap();

    // Already gone, as after a removal a crash cut short: it is synced
    // again, and a sync that fails says so.
    crate::faults::fail_marker_sync();
    assert!(controller.clear_if(&generation).is_err());
    controller.clear_if(&generation).unwrap();
    assert!(!controller.marker.exists());
}

#[test]
fn settling_or_recording_beside_a_marker_that_cannot_be_read_keeps_another_executors_change() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let theirs = token();
    drop(controller.quarantine_by(&theirs, Executor::Vm).unwrap());
    std::fs::write(&controller.marker, b"[").unwrap();

    controller.clear_if(&token()).unwrap();
    drop(controller.quarantine_by(&token(), Executor::Guest).unwrap());

    let named = executors::read(&controller.executors).unwrap();
    assert_eq!(named.get(&theirs), Some(&Executor::Vm));
    assert!(controller.clear().is_err());
}

#[test]
fn a_damaged_executor_record_with_nothing_pending_never_blocks_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    controller.make_directory().unwrap();
    std::fs::write(&controller.executors, b"{").unwrap();

    controller.clear().unwrap();

    assert!(!controller.executors.exists());
}

#[test]
fn a_change_that_would_make_the_record_too_large_to_read_is_refused_and_changes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    controller.make_directory().unwrap();
    // As many other executors' changes as fit, nearly at the record's size.
    let mut named = std::collections::BTreeMap::new();
    while serde_json::to_vec(&named).unwrap().len() < 64 * 1024 - 40 {
        named.insert(token(), Executor::Station);
    }
    let pending: Vec<&String> = named.keys().collect();
    std::fs::write(&controller.marker, serde_json::to_vec(&pending).unwrap()).unwrap();
    std::fs::write(&controller.executors, serde_json::to_vec(&named).unwrap()).unwrap();
    let before = std::fs::read(&controller.executors).unwrap();

    let refused = controller
        .quarantine_by(&token(), Executor::Station)
        .unwrap_err();

    assert_eq!(refused.code, "busy");
    assert_eq!(std::fs::read(&controller.executors).unwrap(), before);
}

#[test]
fn a_damaged_executor_record_is_cleared_before_a_new_change_is_taken_with_nothing_pending() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    controller.make_directory().unwrap();
    std::fs::write(&controller.executors, b"{").unwrap();

    drop(controller.quarantine().unwrap());

    assert!(!controller.executors.exists());
    controller.clear().unwrap();
    assert!(controller.pending().unwrap().is_empty());
}

#[test]
fn a_command_another_executor_carries_out_runs_beside_others_and_waits_for_its_answer() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let theirs = token();
    let command = controller
        .quarantine_command_by(&theirs, Executor::Guest)
        .unwrap();

    assert!(controller.commands().contains(&theirs));
    let beside = Controller::at(505, directory.path())
        .acquire_beside_commands()
        .unwrap();
    drop(beside);
    controller.abandon_command(command);
    controller.clear().unwrap();
    assert_eq!(controller.pending().unwrap(), std::slice::from_ref(&theirs));
}

#[test]
fn the_pending_changes_are_read_without_waiting_and_never_from_a_marker_that_cannot_be_read() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    assert!(
        controller
            .pending_within(std::time::Duration::ZERO)
            .unwrap()
            .is_empty()
    );
    let generation = token();
    drop(
        controller
            .quarantine_by(&generation, Executor::Guest)
            .unwrap(),
    );

    assert_eq!(
        controller
            .pending_within(std::time::Duration::ZERO)
            .unwrap(),
        std::slice::from_ref(&generation)
    );
    let held = controller.lock_marker().unwrap();
    let busy = controller
        .pending_within(std::time::Duration::ZERO)
        .unwrap_err();
    assert_eq!(busy.code, "busy");
    drop(held);
    std::fs::write(&controller.marker, b"[").unwrap();
    assert!(
        controller
            .pending_within(std::time::Duration::ZERO)
            .is_err()
    );
}

#[test]
fn the_pending_changes_are_read_under_the_lock_even_with_no_marker() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let held = controller.lock_marker().unwrap();

    let busy = controller
        .pending_within(std::time::Duration::ZERO)
        .unwrap_err();

    assert!(!controller.marker.exists());
    assert_eq!(busy.code, "busy");
    drop(held);
    assert!(
        controller
            .pending_within(std::time::Duration::ZERO)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_clearing_whose_folder_was_not_synced_is_never_read_as_settled_until_it_is() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let generation = token();
    drop(
        controller
            .quarantine_by(&generation, Executor::Guest)
            .unwrap(),
    );
    // Unlinked, its folder not synced: a crash could bring it back.
    crate::faults::fail_marker_sync();
    assert!(controller.clear_if(&generation).is_err());
    assert!(!controller.marker.exists());

    crate::faults::fail_marker_sync();
    let unsynced = controller.pending_within(std::time::Duration::ZERO);
    crate::faults::fail_marker_sync();
    let unsynced_by = controller.pending_by(std::time::Duration::ZERO);

    assert!(unsynced.is_err(), "nothing is said settled");
    assert!(unsynced_by.is_err(), "nor is anything said settled by whom");
    assert!(
        controller
            .pending_by(std::time::Duration::ZERO)
            .unwrap()
            .is_empty(),
        "once synced, by whom too"
    );
    assert!(
        controller
            .pending_within(std::time::Duration::ZERO)
            .unwrap()
            .is_empty(),
        "once synced"
    );
}

#[test]
fn every_pending_change_is_read_with_who_carries_it_out_held_or_not() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let wait = std::time::Duration::ZERO;
    assert!(controller.pending_by(wait).unwrap().is_empty());
    let (guest, vms, station, own) = (token(), token(), token(), token());
    let holding = controller.quarantine_by(&guest, Executor::Guest).unwrap();
    drop(controller.quarantine_by(&vms, Executor::Vm).unwrap());
    drop(
        controller
            .quarantine_by(&station, Executor::Station)
            .unwrap(),
    );
    drop(controller.quarantine_as(&own).unwrap());

    let mut read = controller.pending_by(wait).unwrap();
    read.sort_by(|one, other| one.0.cmp(&other.0));
    let mut expected = vec![
        (guest, Some(Executor::Guest)),
        (vms, Some(Executor::Vm)),
        (station, Some(Executor::Station)),
        (own, None),
    ];
    expected.sort_by(|one, other| one.0.cmp(&other.0));
    assert_eq!(read, expected);
    drop(holding);

    let busy = controller.lock_marker().unwrap();
    assert_eq!(controller.pending_by(wait).unwrap_err().code, "busy");
    drop(busy);
    std::fs::write(&controller.executors, b"{").unwrap();
    assert!(controller.pending_by(wait).is_err());
    std::fs::write(&controller.marker, b"[").unwrap();
    assert!(controller.pending_by(wait).is_err());
}

#[test]
fn an_executors_changes_no_live_process_holds_are_read_apart_from_held_ones_and_others() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let wait = std::time::Duration::ZERO;
    let (left, held, vms, own) = (token(), token(), token(), token());
    drop(controller.quarantine_by(&left, Executor::Guest).unwrap());
    let holding = controller
        .quarantine_command_by(&held, Executor::Guest)
        .unwrap();
    drop(controller.quarantine_by(&vms, Executor::Vm).unwrap());
    drop(controller.quarantine_as(&own).unwrap());

    assert_eq!(
        controller.unheld_by(Executor::Guest, wait).unwrap(),
        std::slice::from_ref(&left)
    );
    drop(holding);
    let mut unheld = controller.unheld_by(Executor::Guest, wait).unwrap();
    unheld.sort();
    let mut expected = vec![left, held];
    expected.sort();
    assert_eq!(unheld, expected);
    assert_eq!(
        controller.unheld_by(Executor::Vm, wait).unwrap(),
        std::slice::from_ref(&vms)
    );

    let busy = controller.lock_marker().unwrap();
    assert_eq!(
        controller
            .unheld_by(Executor::Guest, wait)
            .unwrap_err()
            .code,
        "busy"
    );
    drop(busy);
    std::fs::write(&controller.executors, b"{").unwrap();
    assert!(controller.unheld_by(Executor::Guest, wait).is_err());
    std::fs::write(&controller.marker, b"[").unwrap();
    assert!(controller.unheld_by(Executor::Guest, wait).is_err());
}

#[test]
fn a_clearing_that_must_not_wait_is_refused_busy_while_the_marker_is_held_and_settles_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let generation = token();
    drop(
        controller
            .quarantine_by(&generation, Executor::Guest)
            .unwrap(),
    );
    let held = controller.lock_marker().unwrap();

    let busy = controller
        .clear_if_within(&generation, std::time::Duration::ZERO)
        .unwrap_err();
    drop(held);

    assert_eq!(busy.code, "busy");
    assert_eq!(
        controller.pending().unwrap(),
        std::slice::from_ref(&generation)
    );
    controller
        .clear_if_within(&generation, std::time::Duration::ZERO)
        .unwrap();
    assert!(controller.pending().unwrap().is_empty());
}

#[test]
fn a_clearing_that_must_know_the_marker_settles_nothing_beside_one_that_cannot_be_read() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let theirs = token();
    drop(controller.quarantine_by(&theirs, Executor::Vm).unwrap());
    std::fs::write(controller.marker_path(), b"{").unwrap();

    let refused = controller
        .clear_known_within(&theirs, std::time::Duration::from_secs(1))
        .unwrap_err();

    assert_eq!(refused.code, Code::State, "{refused:?}");
    assert_eq!(std::fs::read(controller.marker_path()).unwrap(), b"{");
    std::fs::write(
        controller.marker_path(),
        serde_json::to_vec(&[&theirs]).unwrap(),
    )
    .unwrap();
    controller
        .clear_known_within(&theirs, std::time::Duration::from_secs(1))
        .unwrap();
    assert!(controller.pending().unwrap().is_empty());
}

#[test]
fn a_claim_waits_for_the_markers_lock_no_longer_than_it_is_given() {
    use fs2::FileExt;
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    std::fs::create_dir_all(controller.marker_lock_path().parent().unwrap()).unwrap();
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(controller.marker_lock_path())
        .unwrap();
    lock.lock_exclusive().unwrap();
    let began = std::time::Instant::now();

    let refused = controller
        .quarantine_by_within(
            &token(),
            Executor::Vm,
            std::time::Duration::from_millis(200),
        )
        .unwrap_err();

    assert_eq!(refused.code, Code::Busy, "{refused:?}");
    assert!(began.elapsed() < std::time::Duration::from_secs(2));
    FileExt::unlock(&lock).unwrap();
    assert!(controller.pending().unwrap().is_empty(), "nothing recorded");
}

#[test]
fn what_keeps_control_from_being_taken_is_every_pending_change_whoever_carries_it_out() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let wait = std::time::Duration::from_millis(50);
    assert!(controller.unresolved_within(wait).unwrap().is_empty());
    let vms = token();
    drop(controller.quarantine_by(&vms, Executor::Vm).unwrap());
    let own = controller.quarantine().unwrap();

    let unresolved = controller.unresolved_within(wait).unwrap();

    assert_eq!(
        unresolved,
        [
            Unresolved {
                generation: vms.clone(),
                executor: Some(Executor::Vm),
                holder: Liveness::Gone,
            },
            Unresolved {
                generation: own.generation().to_owned(),
                executor: None,
                holder: Liveness::Live,
            },
        ]
    );
    assert!(controller.require_known_beside(Beside::Commands).is_err());
    drop(own);
}

#[test]
fn commands_agents_started_that_still_run_keep_nothing_from_being_taken_alone() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let wait = std::time::Duration::from_millis(50);
    let command = controller
        .quarantine_command_by(&token(), Executor::Guest)
        .unwrap();

    assert!(controller.unresolved_within(wait).unwrap().is_empty());
    assert!(controller.require_known_beside(Beside::Commands).is_ok());

    let theirs = token();
    drop(
        controller
            .quarantine_by(&theirs, Executor::Station)
            .unwrap(),
    );
    let unresolved = controller.unresolved_within(wait).unwrap();
    assert_eq!(
        unresolved,
        [Unresolved {
            generation: theirs,
            executor: Some(Executor::Station),
            holder: Liveness::Gone,
        }],
        "only the change beside the command"
    );
    controller.abandon_command(command);
}

#[test]
fn what_keeps_control_from_being_taken_is_never_read_as_nothing_from_records_that_cannot_be_read() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let wait = std::time::Duration::from_millis(50);
    drop(controller.quarantine_by(&token(), Executor::Vm).unwrap());
    std::fs::write(&controller.executors, b"{").unwrap();
    assert!(controller.unresolved_within(wait).is_err());

    std::fs::write(&controller.marker, b"[").unwrap();
    assert!(controller.unresolved_within(wait).is_err());

    std::fs::write(&controller.marker, b"[]").unwrap();
    assert!(
        controller.unresolved_within(wait).is_err(),
        "a marker naming nothing is an unknown outcome"
    );
}

#[test]
fn a_change_a_live_process_makes_is_live_until_it_dies_then_gone_or_unknown_by_its_record() {
    const TEST: &str = "tests::executors::a_change_a_live_process_makes_is_live_until_it_dies_then_gone_or_unknown_by_its_record";
    if let Some(directory) = std::env::var_os("ATLAS_CONTROL_TEST_CLAIMER") {
        let controller = Controller::at(505, std::path::Path::new(&directory));
        let _claim = controller.quarantine_by(&token(), Executor::Guest).unwrap();
        atlas_core::test_support::message("claimed");
        atlas_core::test_support::permitted("never");
        return;
    }
    if !atlas_core::test_support::isolated(TEST) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let wait = std::time::Duration::from_millis(50);
    let mut child = atlas_core::test_support::TestChild::spawn(
        atlas_core::test_support::rerun(TEST)
            .arg("--nocapture")
            .env("ATLAS_CONTROL_TEST_CLAIMER", directory.path()),
    );
    child.expect("claimed");

    let making = controller.unresolved_within(wait).unwrap();
    child.process.kill().unwrap();
    child.wait();
    let left = controller.unresolved_within(wait).unwrap();

    assert_eq!(making.len(), 1, "{making:?}");
    assert_eq!(making[0].holder, Liveness::Live, "{making:?}");
    assert_eq!(making[0].executor, Some(Executor::Guest));
    assert_eq!(left.len(), 1, "{left:?}");
    assert_eq!(
        left[0].holder,
        Liveness::Gone,
        "the claim no process holds: {left:?}"
    );
    assert_eq!(left[0].generation, making[0].generation);

    // A malformed executor record beside the claim it left: whether a
    // process still carries it out cannot be told, and recovery still
    // keeps it.
    let claim = std::fs::read_dir(directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.to_string_lossy()
                .ends_with(&format!(".{}.live", left[0].generation))
        })
        .unwrap();
    std::fs::write(claim.with_extension("executor.live"), b"{").unwrap();
    let unknown = controller.unresolved_within(wait).unwrap();
    assert_eq!(unknown[0].holder, Liveness::Unknown, "{unknown:?}");
    let _ = controller.clear();
    assert_eq!(controller.pending().unwrap(), [left[0].generation.clone()]);
}

#[test]
fn an_executor_whose_process_cannot_be_read_is_unknown_never_live_or_gone() {
    use std::io::Write;
    let mut file = tempfile::tempfile().unwrap();
    // This process stands for one that reused the executor's PID.
    let executor = crate::executor::Executor {
        pid: nix::unistd::getpid().as_raw(),
        boot: "boot-session".into(),
        start: [300, 400],
    };
    file.write_all(&serde_json::to_vec(&executor).unwrap())
        .unwrap();
    let boot = || Ok("boot-session".to_owned());
    let liveness = |seen: atlas_core::error::Result<Option<crate::executor::Seen>>| {
        crate::executor::executor_liveness_with(&file, boot, |_| seen)
    };
    let seen = |start, exited| Ok(Some(crate::executor::Seen { start, exited }));

    assert_eq!(liveness(seen([300, 400], false)), Liveness::Live);
    assert_eq!(
        liveness(seen([300, 401], false)),
        Liveness::Gone,
        "another process"
    );
    assert_eq!(
        liveness(seen([300, 400], true)),
        Liveness::Gone,
        "exited, not reaped"
    );
    assert_eq!(liveness(Ok(None)), Liveness::Gone, "no such process");
    assert_eq!(
        liveness(Err(atlas_core::error::Error::new(
            atlas_core::error::Code::State,
            "unreadable process"
        ))),
        Liveness::Unknown
    );
    assert!(
        crate::executor::executor_running_with(&file, boot, |_| Err(
            atlas_core::error::Error::new(atlas_core::error::Code::State, "unreadable process")
        ),),
        "recovery still holds what cannot be told"
    );
}

#[test]
fn a_change_whose_executor_exited_but_is_not_reaped_yet_is_gone() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let wait = std::time::Duration::from_millis(50);
    let mut executor = atlas_core::spawn::spawn(
        std::process::Command::new("/bin/sh")
            .args(["-c", "read line"])
            .stdin(std::process::Stdio::piped()),
    )
    .unwrap();
    let pid = i32::try_from(executor.id()).unwrap();
    let change = token();
    let claim = controller.quarantine_by(&change, Executor::Guest).unwrap();
    let claims = ExecutorClaims::new(std::slice::from_ref(&claim)).unwrap();
    record_executor(nix::unistd::Pid::from_raw(pid)).unwrap();
    drop(claims);
    // Its caller ends: the claim is kept for the executor that still runs.
    drop(claim);
    let running = controller.unresolved_within(wait).unwrap();

    // The executor exits; nothing reaps it before the check.
    drop(executor.stdin.take());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !atlas_core::process::info(pid)
        .unwrap()
        .is_some_and(|info| info.zombie)
    {
        assert!(std::time::Instant::now() < deadline, "never exited");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let exited = controller.unresolved_within(wait).unwrap();
    executor.wait().unwrap();

    assert_eq!(running.len(), 1, "{running:?}");
    assert_eq!(running[0].holder, Liveness::Live, "{running:?}");
    assert_eq!(exited.len(), 1, "{exited:?}");
    assert_eq!(exited[0].generation, change);
    assert_eq!(exited[0].holder, Liveness::Gone, "{exited:?}");
}
