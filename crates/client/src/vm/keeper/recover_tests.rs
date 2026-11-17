use super::*;
use crate::{
    client::Client,
    doing::Doing,
    vm::keeper::{
        clients::tests::{Serving, asked, client_of, serving},
        tests::{Saying, until},
    },
};
use atlas_control::Controller;
use std::sync::{Arc, atomic::Ordering::SeqCst};

/// A keeper serving a guest that proves itself, its saying, and an agent's
/// client of it.
async fn proven() -> (Serving, Arc<Saying>, Client, tempfile::TempDir) {
    let serving = serving(true).await;
    let saying = serving.fixture.guest.proving();
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path()).doing(Doing::new("run"));
    (serving, saying, client, directory)
}

fn stop(serving: &Serving) -> Controller {
    serving.fixture.keeper.stop.clone()
}

/// The changes the station settled with the guest.
fn settled(serving: &Serving) -> Vec<Token> {
    serving
        .fixture
        .guest
        .standin
        .settled
        .lock()
        .unwrap()
        .clone()
}

/// The generation live now.
fn live(serving: &Serving) -> u64 {
    serving.fixture.keeper.shared.state().generation.unwrap().0
}

/// Cut the guest's control connection and wait for the keeper's next
/// generation.
async fn restarted(serving: &Serving) {
    let first = live(serving);
    serving.fixture.guest.cut();
    until("a new generation", || {
        serving
            .fixture
            .keeper
            .shared
            .live()
            .is_some_and(|session| session.control.generation().0 != first)
    })
    .await;
}

/// A change of the guest's the station claims, its command killed before
/// it heard the answer: its claim held by no one, its guest's record kept
/// as `came_to` says (none while it is carried out).
fn left_by_a_killed_command(serving: &Serving, came_to: Option<Value>) -> Token {
    let change = Token::new();
    let guest = &serving.fixture.guest.standin;
    guest.ask(&change);
    if let Some(reply) = came_to {
        guest.done(&change, &reply);
    }
    drop(
        stop(serving)
            .quarantine_command_by(change.as_str(), Executor::Guest)
            .unwrap(),
    );
    change
}

fn certain() -> Value {
    json!({"ok": true, "exitCode": 0, "timedOut": false, "signal": null, "truncated": false})
}

fn uncertain() -> Value {
    json!({"ok": false, "code": "stopped", "error": "Agents were stopped while it ran",
        "uncertain": true})
}

fn ended() -> Value {
    json!({"ok": false, "code": "uncertain", "error": "Its work is proven ended",
        "uncertain": true, "ended": true})
}

/// What the keeper's recovery answers in the live generation.
async fn recover(serving: &Serving) -> Value {
    let answered = asked(
        &serving.socket,
        &json!({"op": "recover", "generation": live(serving)}),
    )
    .await;
    answered[0].clone()
}

fn hi() -> Vec<String> {
    vec!["/bin/echo".to_owned(), "hi".to_owned()]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_killed_commands_change_is_settled_at_the_next_hello_and_the_next_command_runs() {
    let (serving, saying, client, _directory) = proven().await;
    let change = left_by_a_killed_command(&serving, Some(certain()));

    let held = client.run_beside(hi(), Duration::from_secs(20)).await;
    assert!(held.is_err(), "held while its outcome is unknown: {held:?}");
    assert!(saying.runs.lock().unwrap().is_empty());
    restarted(&serving).await;
    until("its claim cleared", || {
        client.controller.pending().unwrap().is_empty()
    })
    .await;
    until("settled with the guest", || {
        settled(&serving).contains(&change)
    })
    .await;

    let reply = client
        .run_beside(hi(), Duration::from_secs(20))
        .await
        .unwrap();
    assert_eq!(reply["exitCode"], 0, "{reply}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_change_whose_outcome_is_unknown_is_kept_until_its_guest_proves_its_work_ended() {
    let (serving, _saying, client, _directory) = proven().await;
    let change = left_by_a_killed_command(&serving, None);

    let unknown = recover(&serving).await;
    serving.fixture.guest.standin.done(&change, &uncertain());
    let cut_short = recover(&serving).await;
    serving.fixture.guest.standin.done(&change, &ended());
    let proven = recover(&serving).await;

    assert_eq!(unknown["kept"], json!([change]), "{unknown}");
    assert_eq!(cut_short["kept"], json!([change]), "never as it stands");
    assert_eq!(proven["cleared"], json!([change]), "{proven}");
    assert!(client.controller.pending().unwrap().is_empty());
    until("settled with the guest", || {
        settled(&serving).contains(&change)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_that_proves_nothing_ended_leaves_an_ended_answer_pending() {
    let (serving, _saying, client, _directory) = proven().await;
    serving.fixture.guest.standin.unended.store(true, SeqCst);
    restarted(&serving).await;
    let change = left_by_a_killed_command(&serving, Some(ended()));

    let recovered = recover(&serving).await;

    assert_eq!(recovered["kept"], json!([change]), "{recovered}");
    assert_eq!(client.controller.pending().unwrap(), [change.as_str()]);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(!settled(&serving).contains(&change));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_change_whose_command_still_holds_its_claim_is_never_cleared() {
    let (serving, _saying, client, _directory) = proven().await;
    let change = Token::new();
    let guest = &serving.fixture.guest.standin;
    guest.ask(&change);
    guest.done(&change, &certain());
    let _held = stop(&serving)
        .quarantine_command_by(change.as_str(), Executor::Guest)
        .unwrap();

    let recovered = recover(&serving).await;
    restarted(&serving).await;
    tokio::time::sleep(Duration::from_millis(1500)).await;

    assert_eq!(recovered["cleared"], json!([]), "{recovered}");
    assert_eq!(
        recovered["kept"],
        json!([]),
        "a held one is not asked after"
    );
    assert_eq!(client.controller.pending().unwrap(), [change.as_str()]);
    assert!(!settled(&serving).contains(&change));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stop_mid_command_with_its_cleanup_unproven_stays_pending_through_recovery() {
    let (serving, _saying, client, _directory) = proven().await;
    let guest = serving.fixture.guest.standin.clone();
    guest.holding.store(true, SeqCst);
    let running = tokio::spawn({
        let client = client.clone();
        async move { client.run_beside(hi(), Duration::from_secs(20)).await }
    });
    until("asked for", || guest.changes.lock().unwrap().len() == 1).await;
    let change = guest.changes.lock().unwrap().keys().next().unwrap().clone();
    client.controller.note_stop(true).unwrap();
    let stopped = running.await.unwrap().unwrap_err();
    until("its lane cancelled", || {
        guest
            .changes
            .lock()
            .unwrap()
            .get(&change)
            .is_some_and(Option::is_some)
    })
    .await;

    let recovered = recover(&serving).await;
    restarted(&serving).await;
    tokio::time::sleep(Duration::from_millis(1500)).await;

    assert!(stopped.uncertain, "{stopped:?}");
    assert_eq!(recovered["kept"], json!([change]), "{recovered}");
    assert_eq!(client.controller.pending().unwrap(), [change.as_str()]);
    assert!(!settled(&serving).contains(&change));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_persons_recover_on_a_vm_clears_what_its_guest_answered_under_its_saved_stop_and_says_the_rest_kept()
 {
    let serving = serving(true).await;
    serving.fixture.guest.proving();
    *serving.fixture.guest.stop.lock().unwrap() = "stopped";
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path());
    let done = left_by_a_killed_command(&serving, Some(certain()));
    let unknown = left_by_a_killed_command(&serving, None);
    let vms = Token::new();
    drop(
        stop(&serving)
            .quarantine_by(vms.as_str(), Executor::Vm)
            .unwrap(),
    );

    let recovered = client.recover_helper().await.unwrap();

    assert_eq!(recovered["op"], "recover", "{recovered}");
    assert_eq!(recovered["cleared"], json!([done]));
    let mut kept: Vec<String> = serde_json::from_value(recovered["kept"].clone()).unwrap();
    kept.sort();
    let mut expected = vec![unknown.as_str().to_owned(), vms.as_str().to_owned()];
    expected.sort();
    assert_eq!(kept, expected, "Atlas VM's claim is never touched");
    until("settled with the guest", || {
        settled(&serving).contains(&done)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_marker_held_once_the_outcome_is_asked_keeps_the_claim_and_leaves_nothing_waiting() {
    let (serving, _saying, client, _directory) = proven().await;
    // Unknown to the stand-in's records: its outcome read is the test's.
    let change = Token::new();
    drop(
        stop(&serving)
            .quarantine_command_by(change.as_str(), Executor::Guest)
            .unwrap(),
    );
    let lock = stop(&serving).marker_lock_path().to_owned();
    let held: Arc<std::sync::Mutex<Option<std::fs::File>>> = Arc::default();
    let (holding, once) = (held.clone(), std::sync::atomic::AtomicBool::new(true));
    *serving.fixture.guest.answering.lock().unwrap() = Some(Arc::new(move |_, request| {
        if request["op"] == "outcome" {
            if !once.swap(false, SeqCst) {
                return Outcome::Done { reply: certain() }.answer();
            }
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&lock)
                .unwrap();
            fs2::FileExt::lock_exclusive(&file).unwrap();
            *holding.lock().unwrap() = Some(file);
            let done = Outcome::Done { reply: certain() };
            return done.answer();
        }
        json!({"ok": true, "echo": request})
    }));

    let started = std::time::Instant::now();
    let recovered = recover(&serving).await;
    let took = started.elapsed();

    assert!(took < Duration::from_secs(5), "answered in {took:?}");
    assert_eq!(recovered["cleared"], json!([]), "{recovered}");
    assert_eq!(recovered["kept"], json!([change]), "{recovered}");
    assert!(
        serving.fixture.keeper.recovering.try_lock().is_ok(),
        "the recovery ended"
    );
    // A clearing still waiting would take the lock the moment it goes.
    held.lock().unwrap().take();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(client.controller.pending().unwrap(), [change.as_str()]);
    let again = recover(&serving).await;
    assert_eq!(again["cleared"], json!([change]), "the next round: {again}");
    assert!(client.controller.pending().unwrap().is_empty());
}
