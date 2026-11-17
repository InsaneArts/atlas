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

    assert!(unsynced.is_err(), "nothing is said settled");
    assert!(
        controller
            .pending_within(std::time::Duration::ZERO)
            .unwrap()
            .is_empty(),
        "once synced"
    );
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
