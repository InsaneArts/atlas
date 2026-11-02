//! The labels kept beside the lock and the marker: which host session
//! claimed each pending change, which holds control and which waits, and
//! the order waiters take control in.
use super::{acquiring, told, watched};
use crate::{
    control::*,
    error::Holder,
    files::token,
    host_session::{Host, HostSession, Instance},
};
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
    let kept: ByGeneration<Key> = by_generation::read(&controller.causes);
    assert_eq!(kept.len(), 2, "the settled one's cause is gone too");

    controller.clear().unwrap();
    let kept: ByGeneration<Key> = by_generation::read(&controller.causes);
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
/// kept in `directory`, reports readiness, and waits to be killed.
fn claimer(test: &str, directory: &Path) -> crate::test_support::TestChild {
    crate::test_support::TestChild::spawn(
        crate::test_support::rerun(test)
            .arg("--nocapture")
            .env("ATLAS_CONTROL_TEST_CLAIMER", directory),
    )
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
    crate::test_support::message("ready");
    crate::test_support::permitted("exit");
    std::process::exit(0);
}

#[test]
fn a_killed_holders_cause_survives_it() {
    const TEST: &str = "control::tests::notes::a_killed_holders_cause_survives_it";
    claim_if_claimer();
    if !crate::test_support::isolated(TEST) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let mut child = claimer(TEST, directory.path());
    child.expect("ready");
    let controller = Controller::at(505, &directory.path().join("state"));
    let note = controller.holder_note().unwrap();
    assert_eq!(
        note.session,
        Some(session("right").key().clone()),
        "it holds control"
    );

    child.process.kill().unwrap();
    assert!(!child.wait().success());
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
    if !crate::test_support::isolated(
        "control::tests::notes::six_waiters_take_the_lock_in_the_order_they_began",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let held = controller.acquire(false).unwrap();
    let since = crate::time::now_ms();
    let (send, acquired) = std::sync::mpsc::channel();
    let waiters: Vec<_> = (0..6)
        .map(|index| {
            let (waiter, notice) = watched(
                &labelled(directory.path(), &format!("waiter-{index}")),
                Duration::from_secs(60),
            );
            let (release, permit) = std::sync::mpsc::channel();
            let send = send.clone();
            let waiting = std::thread::spawn(move || {
                let lock = crate::files::open_lock(&waiter.lock).unwrap();
                waiter
                    .wait_for_lock(&lock, Instant::now(), || {
                        turns::Turn::join_at(
                            &waiter.waiting,
                            waiter.label.as_ref(),
                            since + index,
                            &format!("{index:016x}"),
                        )
                    })
                    .unwrap();
                let ownership = waiter.owning(lock);
                send.send(index).unwrap();
                permit.recv_timeout(Duration::from_secs(30)).unwrap();
                drop(ownership);
            });
            assert_eq!(told(&notice), WaitEvent::Began);
            // Began precedes joining: observe the actual note before the next waiter starts.
            within(Duration::from_secs(30), || {
                waiting_notes(&controller).len() == index as usize + 1
            });
            (waiting, release)
        })
        .collect();
    let notes = waiting_notes(&controller);
    assert_eq!(notes.len(), 6);
    assert!(
        notes
            .iter()
            .all(|note| note.session.is_some() && note.live())
    );
    assert!(
        acquired.try_recv().is_err(),
        "the holder excludes every successor"
    );
    drop(held);
    for (index, (waiting, release)) in waiters.into_iter().enumerate() {
        assert_eq!(
            acquired.recv_timeout(Duration::from_secs(30)).unwrap(),
            index as u64
        );
        assert_eq!(controller.acquire(false).unwrap_err().code, "busy");
        assert!(
            acquired.try_recv().is_err(),
            "only one waiter owns the lock"
        );
        release.send(()).unwrap();
        waiting.join().unwrap();
    }
    assert!(
        waiting_notes(&controller).is_empty(),
        "each note goes with its wait"
    );
}

#[test]
fn a_dead_or_stale_waiters_note_is_skipped() {
    if !crate::test_support::isolated(
        "control::tests::notes::a_dead_or_stale_waiters_note_is_skipped",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let this = Instance::current().unwrap();
    let now = crate::time::now_ms();
    let dead = waiter_note(
        &controller,
        Instance::new(i32::MAX, [1, 0]),
        now - 1000,
        Duration::ZERO,
    );
    let old = waiter_note(&controller, this, now - 606_000, Duration::ZERO);
    let quiet = waiter_note(&controller, this, now - 1000, Duration::from_secs(3));
    let held = controller.acquire(false).unwrap();
    let (waiter, events) = watched(&controller, Duration::from_secs(60));
    let waiting = acquiring(waiter, false);
    assert_eq!(told(&events), WaitEvent::Began);
    assert!(!waiting.is_finished());
    drop(held);
    assert_eq!(told(&events), WaitEvent::Ended);
    waiting.join().unwrap().unwrap();
    assert!(!dead.exists(), "a dead waiter's note is removed");
    assert!(
        old.exists() && quiet.exists(),
        "a live one's is only passed over"
    );
    let (waiter, events) = watched(&controller, Duration::from_secs(60));
    drop(waiter.acquire(false).unwrap());
    assert!(events.try_recv().is_err(), "nobody is in line");
}

#[test]
fn a_waiter_with_no_note_still_gets_the_lock() {
    if !crate::test_support::isolated(
        "control::tests::notes::a_waiter_with_no_note_still_gets_the_lock",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let controller = labelled(directory.path(), "right");
    let held = controller.acquire(false).unwrap();
    fs::write(&controller.waiting, b"not a folder").unwrap();
    let (waiter, events) = watched(&controller, Duration::from_secs(60));
    let waiting = acquiring(waiter, false);
    assert_eq!(told(&events), WaitEvent::Began);
    assert!(!waiting.is_finished());
    drop(held);
    assert_eq!(told(&events), WaitEvent::Ended);
    waiting.join().unwrap().unwrap();
    assert_eq!(fs::read(&controller.waiting).unwrap(), b"not a folder");
}

#[test]
fn a_newcomer_waits_its_turn_but_the_person_and_a_stop_end_every_wait() {
    if !crate::test_support::isolated(
        "control::tests::notes::a_newcomer_waits_its_turn_but_the_person_and_a_stop_end_every_wait",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let this = Instance::current().unwrap();
    let ahead = waiter_note(
        &controller,
        this,
        crate::time::now_ms() - 1000,
        Duration::ZERO,
    );
    for stop in [false, true] {
        let (waiter, events) = watched(&controller, Duration::from_secs(60));
        let waiting = acquiring(waiter.heeding_stop(), false);
        assert_eq!(told(&events), WaitEvent::Began);
        assert!(
            !waiting.is_finished(),
            "an older waiter goes first even with the lock free"
        );
        drop(controller.acquire(false).unwrap());
        if stop {
            controller.note_stop(true).unwrap();
        } else {
            fs::write(&controller.person, b"").unwrap();
        }
        assert_eq!(told(&events), WaitEvent::Ended);
        let refused = waiting.join().unwrap().unwrap_err();
        if stop {
            assert_eq!(refused.code, "stopped");
        } else {
            assert_eq!(refused.holder, Some(Holder::Person));
            fs::remove_file(&controller.person).unwrap();
        }
    }
    fs::remove_file(ahead).unwrap();
}

#[test]
fn a_bounded_pending_read_returns_when_the_marker_lock_is_held() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(crate::test_support::UID, directory.path());
    drop(controller.quarantine().unwrap());
    let held = controller.lock_marker().unwrap();
    let (send, result) = std::sync::mpsc::channel();
    let reader = controller.clone();
    let reading = std::thread::spawn(move || {
        send.send(reader.pending_causes_within(Duration::from_millis(10)))
            .unwrap();
    });
    let refused = result
        .recv_timeout(Duration::from_secs(30))
        .unwrap()
        .unwrap_err();
    assert_eq!(refused.code, "busy");
    assert_eq!(
        refused.error,
        "Atlas could not check pending activity in time."
    );
    reading.join().unwrap();
    drop(held);
    assert_eq!(
        controller
            .pending_causes_within(Duration::ZERO)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn a_missing_marker_read_creates_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("absent");
    let controller = Controller::at(crate::test_support::UID, &path);
    assert!(
        controller
            .pending_causes_within(Duration::ZERO)
            .unwrap()
            .is_empty()
    );
    assert!(!path.exists());
}

#[test]
fn a_bounded_pending_read_preserves_claim_classification() {
    let directory = tempfile::tempdir().unwrap();
    let controller =
        Controller::at(crate::test_support::UID, directory.path()).labelled(Some(&session("own")));
    let claim = controller.quarantine_command().unwrap();
    let causes = controller.pending_causes_within(Duration::ZERO).unwrap();
    assert_eq!(causes, controller.pending_causes().unwrap());
    assert!(causes[0].running && causes[0].command);
    drop(claim);
    assert!(!controller.pending_causes_within(Duration::ZERO).unwrap()[0].running);
}

#[test]
fn entry_attachment_does_not_wait_for_a_held_marker_lock_or_lose_pending_attribution() {
    let directory = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(directory.path());
    let pid = nix::unistd::getppid().as_raw();
    let process = Instance::new(pid, crate::host::ancestry::process(pid).unwrap().start);
    let own = session("owner")
        .run_by(Some(process))
        .in_folder(Some("parent"));
    client.controller = client.controller.labelled(Some(&own));
    let claim = client.controller.quarantine().unwrap();
    let generation = claim.generation().to_owned();
    drop(claim);
    let lock = client.controller.lock_marker().unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    let caller = client.clone();
    let worker = std::thread::spawn(move || {
        let result = crate::host_session::store::touch(&caller, &own);
        send.send(result.map_err(|error| error.code)).unwrap();
    });
    let result = receive.recv_timeout(Duration::from_secs(30));
    drop(lock);
    worker.join().unwrap();
    assert_eq!(result.unwrap().unwrap_err(), "busy");
    assert!(
        crate::host_session::store::Store::at(client.output_dir().unwrap())
            .all()
            .is_empty()
    );
    let causes = client.controller.pending_causes().unwrap();
    assert_eq!(causes[0].generation, generation);
    assert_eq!(causes[0].session, Some(session("owner").key().clone()));
}

#[test]
fn display_note_candidates_never_sample_processes() {
    let directory = tempfile::tempdir().unwrap();
    let controller =
        Controller::at(crate::test_support::UID, directory.path()).labelled(Some(&session("own")));
    let hold = controller.acquire(true).unwrap();
    let command = controller.quarantine_command().unwrap();
    let ((holder, waiting, running), count) = crate::process::measure_states(|| {
        (
            controller.holder_note(),
            controller.waiting_candidates(),
            controller.running_candidates(),
        )
    });
    assert_eq!(count, 0);
    assert!(holder.is_some());
    assert!(waiting.is_empty());
    assert_eq!(running.len(), 1);
    drop(command);
    drop(hold);
}

#[test]
fn waiting_display_candidates_keep_their_file_and_count_bounds() {
    use std::time::SystemTime;

    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(crate::test_support::UID, directory.path());
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&controller.waiting)
        .unwrap();
    let mut note = Note::now(Some(&session("own").label())).unwrap();
    let now_ms = 1_761_660_429_000;
    let now = SystemTime::UNIX_EPOCH + Duration::from_millis(now_ms);
    note.since = now_ms;
    for index in 0..200 {
        note.process = Instance::new(i32::MAX - index, [1, 0]);
        let path = controller.waiting.join(format!("{index:016x}.json"));
        note.write(&path).unwrap();
        fs::File::open(path)
            .unwrap()
            .set_modified(now - Duration::from_secs(1))
            .unwrap();
    }
    fs::write(controller.waiting.join("not-a-note.json"), b"{}").unwrap();
    let (candidates, count) =
        crate::process::measure_states(|| turns::candidates_at(&controller.waiting, now));
    assert_eq!(count, 0);
    assert_eq!(candidates.len(), crate::host_session::liveness::BUDGET);
    let (_, reads) = crate::process::measure_states(|| {
        let candidates = turns::candidates_at(&controller.waiting, now);
        let processes = crate::host_session::liveness::Snapshot::sample(
            candidates.iter().map(|note| note.process),
            |instance| instance.state(),
        );
        let scene = crate::session_view::Scene {
            rows: &[],
            processes: &processes,
            holder: None,
            waiting: &candidates,
            running: &[],
            causes: &[],
            pending: false,
            agent_holds: false,
            now: u128::from(now_ms) * 1_000_000,
        };
        for _ in 0..200 {
            let shown = scene.shown(
                &crate::session_view::Who::Host(session("own").key().clone()),
                false,
            );
            assert_eq!(shown.state, crate::session_view::SessionState::Idle);
        }
    });
    assert_eq!(reads, crate::host_session::liveness::BUDGET);
}
