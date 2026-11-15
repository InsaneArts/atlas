use super::*;

fn controller() -> (tempfile::TempDir, Controller) {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(atlas_core::test_support::UID, directory.path());
    (directory, controller)
}

/// Another process's controller over the same state.
fn also(directory: &tempfile::TempDir) -> Controller {
    Controller::at(atlas_core::test_support::UID, directory.path())
}

#[test]
fn each_stop_notes_a_revision_never_noted_before() {
    let (_directory, controller) = controller();
    assert_eq!(controller.stop_revision().unwrap(), None);

    controller.note_stop(true).unwrap();
    let first = controller.stop_revision().unwrap().unwrap();
    controller.note_stop(true).unwrap();
    let second = controller.stop_revision().unwrap().unwrap();

    assert_ne!(first, second);
    assert!(controller.stop_noted());
}

#[test]
fn a_resume_lifts_the_stop_it_read() {
    let (_directory, controller) = controller();
    controller.note_stop(true).unwrap();
    let lifting = controller.stop_revision().unwrap();

    assert_eq!(
        controller.lift_stop(lifting.as_ref()).unwrap(),
        Resumed::Cleared
    );
    assert!(!controller.stop_noted());
}

#[test]
fn a_resume_whose_reply_is_late_never_lifts_a_newer_stop_whose_caller_died() {
    let (directory, controller) = controller();
    controller.note_stop(true).unwrap();
    // The Resume reads what it lifts, then waits for the desktop's reply.
    let lifting = controller.stop_revision().unwrap();
    // Another process's Stop is noted and its caller dies before telling
    // the desktop anything.
    also(&directory).note_stop(true).unwrap();

    // The late reply arrives.
    assert_eq!(
        controller.lift_stop(lifting.as_ref()).unwrap(),
        Resumed::StopHeld
    );
    assert!(controller.stop_noted(), "the newer Stop stays");
}

#[test]
fn a_resume_that_read_no_stop_reports_one_arriving_since_as_still_held() {
    let (directory, controller) = controller();
    let lifting = controller.stop_revision().unwrap();
    assert_eq!(lifting, None);
    also(&directory).note_stop(true).unwrap();

    assert_eq!(
        controller.lift_stop(lifting.as_ref()).unwrap(),
        Resumed::StopHeld
    );
    assert!(controller.stop_noted());
}

#[test]
fn a_second_resume_finds_the_stop_already_gone() {
    let (directory, controller) = controller();
    controller.note_stop(true).unwrap();
    let lifting = controller.stop_revision().unwrap();
    // Two Resumes capture the same revision; the first clears it.
    assert_eq!(
        controller.lift_stop(lifting.as_ref()).unwrap(),
        Resumed::Cleared
    );

    assert_eq!(
        also(&directory).lift_stop(lifting.as_ref()).unwrap(),
        Resumed::AlreadyClear
    );
    assert!(!controller.stop_noted());
}

#[test]
fn an_empty_marker_an_older_build_wrote_is_a_stop_a_resume_can_lift() {
    let (_directory, controller) = controller();
    controller.note_stop(true).unwrap();
    fs::write(&controller.stopped, b"").unwrap();

    let lifting = controller.stop_revision().unwrap();
    assert_eq!(
        lifting,
        Some(StopRevision {
            marker: Some(String::new()),
            pending: BTreeSet::new(),
        })
    );
    assert!(controller.stop_noted());
    assert_eq!(
        controller.lift_stop(lifting.as_ref()).unwrap(),
        Resumed::Cleared
    );
    assert!(!controller.stop_noted());
}

#[test]
fn a_stop_after_an_older_builds_empty_marker_is_kept_from_a_resume_that_read_it() {
    let (_directory, controller) = controller();
    controller.note_stop(true).unwrap();
    fs::write(&controller.stopped, b"").unwrap();
    let lifting = controller.stop_revision().unwrap();
    controller.note_stop(true).unwrap();

    assert_eq!(
        controller.lift_stop(lifting.as_ref()).unwrap(),
        Resumed::StopHeld
    );
    assert!(controller.stop_noted());
}

#[test]
fn a_stop_that_cannot_take_the_lock_keeps_a_witness_a_resume_cannot_miss() {
    let (directory, controller) = controller();
    controller.note_stop(true).unwrap();
    let lifting = controller.stop_revision().unwrap();

    // A Stop finds the lock held and keeps a witness instead; its caller
    // dies before telling the desktop.
    let held = controller.lock_stop().unwrap();
    also(&directory).note_stop(true).unwrap();
    assert!(
        controller.pending_names().next().is_some(),
        "a witness was kept"
    );
    drop(held);

    // The earlier Resume's late reply must not clear the witnessed Stop.
    assert_eq!(
        controller.lift_stop(lifting.as_ref()).unwrap(),
        Resumed::StopHeld
    );
    assert!(controller.stop_noted(), "the witnessed Stop stays");

    // A Stop that then takes the lock folds the witness into the marker.
    controller.note_stop(true).unwrap();
    assert!(
        controller.pending_names().next().is_none(),
        "witness folded in"
    );
    assert!(controller.stop_noted());
}

#[test]
fn folding_a_witness_keeps_it_when_the_new_marker_cannot_be_published() {
    let (directory, controller) = controller();
    // A witness of a Stop that could not take the lock.
    let held = controller.lock_stop().unwrap();
    also(&directory).note_stop(true).unwrap();
    drop(held);
    assert!(
        controller.pending_names().next().is_some(),
        "a witness is kept"
    );

    // The marker cannot be written (its path is a folder): the fold fails,
    // and the witness it would have folded in stays, never an unstopped gap.
    fs::create_dir(&controller.stopped).unwrap();
    assert!(controller.note_stop(true).is_err());
    assert!(
        controller.pending_names().next().is_some(),
        "the witness is not removed before the marker is published"
    );
    assert!(controller.stop_noted(), "still stopped throughout");

    // With the marker writable again, the fold publishes it and clears the
    // witness.
    fs::remove_dir(&controller.stopped).unwrap();
    controller.note_stop(true).unwrap();
    assert!(
        controller.pending_names().next().is_none(),
        "witness folded in"
    );
    assert!(controller.stop_noted());
}
