//! The labels kept beside the lock and the marker: which host session
//! claimed each pending change, which holds control and which waits, and
//! the order waiters take control in.
use super::*;
use crate::host_session::{Host, HostSession, Instance};
use std::os::unix::fs::DirBuilderExt;

fn session(id: &str) -> HostSession {
    HostSession::new(Host::Codex, id).unwrap()
}

fn labelled(directory: &Path, id: &str) -> Controller {
    Controller::at(505, directory).labelled(Some(&session(id)))
}

fn causes_of(controller: &Controller) -> Vec<(String, Option<Key>)> {
    controller
        .pending_causes()
        .unwrap()
        .into_iter()
        .map(|cause| (cause.generation, cause.session))
        .collect()
}

#[test]
fn a_claimed_change_records_which_session_claimed_it() {
    let directory = tempfile::tempdir().unwrap();
    let controller = labelled(directory.path(), "right");
    let claim = controller.quarantine().unwrap();

    let causes = controller.pending_causes().unwrap();
    assert_eq!(causes.len(), 1);
    assert_eq!(causes[0].generation, claim.generation());
    assert_eq!(causes[0].session, Some(session("right").key().clone()));
    assert!(causes[0].running);

    drop(claim);
    assert!(!controller.pending_causes().unwrap()[0].running);
    let observer = Controller::at(505, directory.path());
    assert_eq!(
        causes_of(&observer),
        causes_of(&controller),
        "anyone can read it"
    );
}

#[test]
fn the_marker_is_unchanged_by_causes() {
    let (plain, labelled_folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let unlabelled = Controller::at(505, plain.path());
    let with_label = labelled(labelled_folder.path(), "right");
    let generation = token();

    drop(unlabelled.quarantine_as(&generation).unwrap());
    drop(with_label.quarantine_as(&generation).unwrap());

    let read = |controller: &Controller| {
        serde_json::from_slice::<Vec<String>>(&fs::read(&controller.marker).unwrap()).unwrap()
    };
    assert_eq!(read(&unlabelled), std::slice::from_ref(&generation));
    assert_eq!(read(&with_label), [generation]);
    assert_eq!(
        fs::read(&unlabelled.marker).unwrap(),
        fs::read(&with_label.marker).unwrap()
    );
}

#[test]
fn a_settled_change_forgets_its_cause_and_recovery_forgets_all_but_running_ones() {
    let directory = tempfile::tempdir().unwrap();
    let controller = labelled(directory.path(), "right");
    let abandoned = controller.quarantine().unwrap().generation().to_owned();
    let running = controller.quarantine().unwrap();
    let settled = controller.quarantine().unwrap();

    controller.clear_if(&settled).unwrap();
    drop(settled);
    let generations: Vec<String> = causes_of(&controller).into_iter().map(|(g, _)| g).collect();
    assert_eq!(
        generations,
        [abandoned.clone(), running.generation().to_owned()]
    );
    let kept = causes::read(&controller.causes);
    assert_eq!(kept.len(), 2, "the settled one's cause is gone too");

    controller.clear().unwrap();
    let kept = causes::read(&controller.causes);
    assert_eq!(
        kept.keys().cloned().collect::<Vec<_>>(),
        [running.generation().to_owned()]
    );

    controller.clear_if(&running).unwrap();
    drop(running);
    assert!(!controller.causes.exists(), "nothing pending, nothing kept");
}

#[test]
fn an_unlabelled_controller_writes_no_causes() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    drop(controller.quarantine().unwrap());

    assert!(!controller.causes.exists());
    assert_eq!(causes_of(&controller)[0].1, None);
}

#[test]
fn a_cause_that_cannot_be_written_never_fails_the_claim() {
    let directory = tempfile::tempdir().unwrap();
    let controller = labelled(directory.path(), "right");
    fs::create_dir(&controller.causes).unwrap();

    let claim = controller.quarantine().unwrap();
    assert_eq!(
        controller.pending().unwrap(),
        [claim.generation().to_owned()]
    );
    assert_eq!(causes_of(&controller)[0].1, None);
    controller.clear_if(&claim).unwrap();
    drop(claim);
    assert!(controller.pending().unwrap().is_empty());
}

/// Stand in for another agent's command, as the test `test` run again in a
/// process of its own: it claims a change for session `right` in the state
/// kept in `directory`, says so in `claimed`, and waits to be killed.
fn claimer(test: &str, directory: &Path) -> std::process::Child {
    std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env("ATLAS_CONTROL_TEST_CLAIMER", directory)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap()
}

/// The command [`claimer`] starts, when this process is it.
fn claim_if_claimer() {
    let Some(directory) = std::env::var_os("ATLAS_CONTROL_TEST_CLAIMER") else {
        return;
    };
    let directory = PathBuf::from(directory);
    let controller = labelled(&directory.join("state"), "right");
    let _ownership = controller.acquire(false).unwrap();
    let _claim = controller.quarantine().unwrap();
    fs::write(directory.join("claimed"), b"").unwrap();
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Wait for `path` to appear, for at most five seconds.
fn appears(path: &Path) {
    let started = Instant::now();
    while !path.exists() {
        assert!(started.elapsed() < Duration::from_secs(5), "{path:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_killed_holders_cause_survives_it() {
    const TEST: &str = "control::notes_tests::a_killed_holders_cause_survives_it";
    claim_if_claimer();
    if !crate::transaction_test::isolated(TEST) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let mut child = claimer(TEST, directory.path());
    appears(&directory.path().join("claimed"));
    let controller = Controller::at(505, &directory.path().join("state"));
    let note = controller.holder_note().unwrap();
    assert_eq!(
        note.session,
        Some(session("right").key().clone()),
        "it holds control"
    );

    child.kill().unwrap();
    child.wait().unwrap();
    let causes = controller.pending_causes().unwrap();
    assert_eq!(causes.len(), 1);
    assert_eq!(causes[0].session, Some(session("right").key().clone()));
    assert!(!causes[0].running);
    assert!(
        !controller.holder_note().unwrap().live(),
        "its note names a process gone"
    );
    assert_eq!(
        controller.acquire(false).unwrap_err().code,
        "uncertain",
        "it still blocks"
    );
}

#[test]
fn taking_control_writes_the_holder_note_and_letting_go_removes_it() {
    let directory = tempfile::tempdir().unwrap();
    let controller = labelled(directory.path(), "right");
    assert_eq!(controller.holder_note(), None);

    let ownership = controller.acquire(false).unwrap();
    let note = controller.holder_note().unwrap();
    assert_eq!(note.session, Some(session("right").key().clone()));
    assert_eq!(note.host, Some(Host::Codex));
    assert!(note.live() && note.process.is_this_one());
    assert!(note.since > 0);
    drop(ownership);
    assert!(!controller.holder.exists());
}

#[test]
fn the_person_taking_control_removes_a_stale_note() {
    let directory = tempfile::tempdir().unwrap();
    let person = Controller::at(505, directory.path());
    // A killed command's note: its lock went with its process.
    let killed = Note {
        session: Some(session("right").key().clone()),
        host: Some(Host::Codex),
        folder: None,
        named: None,
        process: Instance::new(i32::MAX, [1, 0]),
        since: 1,
    };
    fs::create_dir_all(directory.path()).unwrap();
    killed.write(&person.holder).unwrap();
    assert!(person.holder_note().is_some_and(|note| !note.live()));

    let ownership = person.acquire_for_person().unwrap();
    assert!(!person.holder.exists());
    drop(ownership);
}

#[test]
fn a_holder_note_that_cannot_be_written_never_fails_control() {
    let directory = tempfile::tempdir().unwrap();
    let controller = labelled(directory.path(), "right");
    fs::create_dir_all(&controller.holder).unwrap();

    let ownership = controller.acquire(false).unwrap();
    assert_eq!(controller.holder_note(), None);
    drop(ownership);
    assert!(
        controller.holder.is_dir(),
        "what was there is left as it was"
    );
}

#[test]
fn a_refused_acquire_leaves_no_note() {
    let directory = tempfile::tempdir().unwrap();
    let controller = labelled(directory.path(), "right");
    drop(controller.quarantine().unwrap());

    assert_eq!(controller.acquire(false).unwrap_err().code, "uncertain");
    assert!(!controller.holder.exists());
}

/// A waiting note in `controller`'s waiting folder for `process`, begun
/// `since`, touched `touched` ago.
fn waiter_note(
    controller: &Controller,
    process: Instance,
    since: u64,
    touched: Duration,
) -> PathBuf {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&controller.waiting)
        .unwrap();
    let mut name = token();
    name.truncate(16);
    let path = controller.waiting.join(format!("{name}.json"));
    let note = Note {
        session: None,
        host: None,
        folder: None,
        named: None,
        process,
        since,
    };
    fs::write(&path, serde_json::to_vec(&note).unwrap()).unwrap();
    let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_modified(std::time::SystemTime::now() - touched)
        .unwrap();
    path
}

/// Wait until `done`, for at most `limit`.
fn within(limit: Duration, done: impl Fn() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(started.elapsed() < limit, "not done within {limit:?}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The notes in `controller`'s waiting folder.
fn waiting_notes(controller: &Controller) -> Vec<Note> {
    fs::read_dir(&controller.waiting)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| Note::read(&entry.path()))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn six_waiters_take_the_lock_in_the_order_they_began() {
    if !crate::transaction_test::isolated(
        "control::notes_tests::six_waiters_take_the_lock_in_the_order_they_began",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let held = controller.acquire(false).unwrap();
    let order = Arc::new(std::sync::Mutex::new(Vec::new()));

    // Each waiter starts only once the one before it is in line, and in a
    // later millisecond, so the order they began in is certain.
    let waiters: Vec<_> = (0..6)
        .map(|index| {
            let (waiter, order) = (
                labelled(directory.path(), &format!("waiter-{index}"))
                    .waiting(Duration::from_secs(60)),
                Arc::clone(&order),
            );
            let began = crate::host_session::now_ms();
            let waiting = std::thread::spawn(move || {
                let ownership = waiter.acquire(false).unwrap();
                order.lock().unwrap().push(index);
                std::thread::sleep(Duration::from_millis(40));
                drop(ownership);
            });
            within(Duration::from_secs(30), || {
                waiting_notes(&controller).len() == index + 1
            });
            within(Duration::from_secs(5), || {
                crate::host_session::now_ms() > began
            });
            waiting
        })
        .collect();

    let notes = waiting_notes(&controller);
    assert_eq!(notes.len(), 6, "each waiter keeps a note");
    assert!(
        notes
            .iter()
            .all(|note| note.session.is_some() && note.live())
    );
    drop(held);
    for waiting in waiters {
        waiting.join().unwrap();
    }
    assert_eq!(*order.lock().unwrap(), [0, 1, 2, 3, 4, 5]);
    assert!(
        waiting_notes(&controller).is_empty(),
        "each note goes with its wait"
    );
}

#[test]
fn a_dead_or_stale_waiters_note_is_skipped() {
    if !crate::transaction_test::isolated(
        "control::notes_tests::a_dead_or_stale_waiters_note_is_skipped",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let this = Instance::current().unwrap();
    let now = crate::host_session::now_ms();
    let dead = waiter_note(
        &controller,
        Instance::new(i32::MAX, [1, 0]),
        now - 1000,
        Duration::ZERO,
    );
    let old = waiter_note(&controller, this, now - 606_000, Duration::ZERO);
    let quiet = waiter_note(&controller, this, now - 1000, Duration::from_secs(3));

    let held = controller.acquire(false).unwrap();
    let waiting = std::thread::spawn({
        let waiter = controller.clone().waiting(Duration::from_secs(5));
        move || waiter.acquire(false).map(drop)
    });
    std::thread::sleep(Duration::from_millis(300));
    let released = Instant::now();
    drop(held);
    waiting.join().unwrap().unwrap();
    assert!(
        released.elapsed() < Duration::from_secs(3),
        "no passed-over note held it up"
    );
    assert!(!dead.exists(), "a dead waiter's note is removed");
    assert!(
        old.exists() && quiet.exists(),
        "a live one's is only passed over"
    );

    let started = Instant::now();
    drop(
        controller
            .clone()
            .waiting(Duration::from_secs(5))
            .acquire(false)
            .unwrap(),
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "nobody is in line"
    );
}

#[test]
fn a_waiter_with_no_note_still_gets_the_lock() {
    if !crate::transaction_test::isolated(
        "control::notes_tests::a_waiter_with_no_note_still_gets_the_lock",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let held = controller.acquire(false).unwrap();
    fs::write(&controller.waiting, b"not a folder").unwrap();

    let waiting = std::thread::spawn({
        let waiter = labelled(directory.path(), "right").waiting(Duration::from_secs(5));
        move || waiter.acquire(false).map(drop)
    });
    std::thread::sleep(Duration::from_millis(300));
    drop(held);
    waiting.join().unwrap().unwrap();
    assert_eq!(fs::read(&controller.waiting).unwrap(), b"not a folder");
}

#[test]
fn a_newcomer_waits_its_turn_but_the_person_and_a_stop_end_every_wait() {
    if !crate::transaction_test::isolated(
        "control::notes_tests::a_newcomer_waits_its_turn_but_the_person_and_a_stop_end_every_wait",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let this = Instance::current().unwrap();
    let now = crate::host_session::now_ms();
    let ahead = waiter_note(&controller, this, now - 1000, Duration::ZERO);

    // The lock is free, but an older waiter is in line: a newcomer with a
    // wait lets it go first, and one with none answers as before.
    let refused = controller
        .clone()
        .waiting(Duration::from_millis(300))
        .acquire(false)
        .unwrap_err();
    assert_eq!(refused.code, "busy");
    drop(controller.acquire(false).unwrap());

    fs::write(&controller.person, b"").unwrap();
    let started = Instant::now();
    let refused = controller
        .clone()
        .waiting(Duration::from_secs(5))
        .acquire(false)
        .unwrap_err();
    assert_eq!(refused.holder, Some(Holder::Person));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the person is never waited for"
    );
    fs::remove_file(&controller.person).unwrap();

    controller.note_stop(true).unwrap();
    let started = Instant::now();
    let refused = controller
        .clone()
        .waiting(Duration::from_secs(5))
        .heeding_stop()
        .acquire(false)
        .unwrap_err();
    assert_eq!(refused.code, "stopped");
    assert!(started.elapsed() < Duration::from_secs(2));
    fs::remove_file(ahead).unwrap();
}
