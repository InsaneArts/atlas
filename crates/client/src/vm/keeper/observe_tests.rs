use super::*;
use crate::vm::keeper::tests::{Fixture, fixture, until};
use std::sync::atomic::Ordering::SeqCst;

/// A keeper live with its guest, an older guest declaring no proof when
/// `plain`.
async fn live(plain: bool) -> (Fixture, tokio::task::JoinHandle<()>) {
    let fixture = fixture();
    fixture.guest.standin.plain.store(plain, SeqCst);
    let keeper = fixture.keeper.clone();
    let running = tokio::spawn(async move { keeper.run().await });
    until("live", || fixture.keeper.shared.live().is_some()).await;
    (fixture, running)
}

const DEADLINE: Duration = Duration::from_secs(10);

fn lanes(fixture: &Fixture) -> usize {
    fixture.guest.held_lanes.lock().unwrap().len()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_observation_carries_the_commands_own_challenge_in_one_generation() {
    let (fixture, _running) = live(false).await;
    let saying = fixture.guest.proving();
    let generation = fixture.keeper.shared.state().generation.unwrap();

    let observed = observed(&fixture.keeper, Some("first-challenge"), DEADLINE).await;

    assert_eq!(observed["ok"], true, "{observed}");
    assert_eq!(observed["generation"], json!(generation));
    assert_eq!(observed["peer"], json!(fixture.guest.standin.keys.public()));
    assert_eq!(observed["station"], json!(fixture.keeper.own.public()));
    let status = &observed["status"];
    assert_eq!(status["hello"]["machine"]["mode"], "machine", "{observed}");
    assert_eq!(status["inputService"]["pid"], 456, "{observed}");
    assert_eq!(status["sessionProof"]["challenge"], "first-challenge");
    assert_eq!(*saying.challenges.lock().unwrap(), ["first-challenge"]);
    assert_eq!(observed["runner"]["power"], "running");
    assert_eq!(observed["runner"]["window"], "closed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_challenge_that_is_no_challenge_is_refused_before_any_lane_opens() {
    let (fixture, _running) = live(false).await;
    let saying = fixture.guest.proving();
    let opened = lanes(&fixture);
    let long = "a".repeat(129);

    for challenge in [None, Some(""), Some("two words"), Some(long.as_str())] {
        let refused = observed(&fixture.keeper, challenge, DEADLINE).await;

        assert_eq!(refused["code"], "bad-request", "{challenge:?}: {refused}");
    }
    assert_eq!(lanes(&fixture), opened, "a lane was opened");
    assert!(saying.challenges.lock().unwrap().is_empty());
    assert_eq!(fixture.guest.runner_asked.load(SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_that_does_not_say_what_it_is_proved_by_is_told_to_update() {
    let (fixture, _running) = live(true).await;
    fixture.guest.proving();
    let opened = lanes(&fixture);

    let refused = observed(&fixture.keeper, Some("challenge"), DEADLINE).await;

    assert_eq!(refused["code"], "update", "{refused}");
    assert_eq!(refused["error"], "Update Atlas in the VM.");
    assert_eq!(lanes(&fixture), opened);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_generation_lost_while_it_is_observed_is_refused() {
    let (fixture, _running) = live(false).await;
    fixture.guest.proving();
    let told = Arc::new(tokio::sync::Notify::new());
    *fixture.guest.runner_held.lock().unwrap() = Some((told.clone(), true));
    let generation = fixture.keeper.shared.state().generation;

    let observing = tokio::spawn({
        let keeper = fixture.keeper.clone();
        async move { observed(&keeper, Some("challenge"), DEADLINE).await }
    });
    until("the runner asked", || {
        fixture.guest.runner_asked.load(SeqCst) == 1
    })
    .await;
    until("the generation lost", || {
        fixture
            .keeper
            .shared
            .live()
            .is_none_or(|session| Some(session.control.generation()) != generation)
    })
    .await;
    told.notify_one();
    let refused = observing.await.unwrap();

    assert_eq!(refused["code"], "transport", "{refused}");
    assert!(
        refused["error"].as_str().unwrap().contains("ended"),
        "{refused}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn no_more_than_four_observations_run_at_once() {
    let (fixture, _running) = live(false).await;
    fixture.guest.proving();
    let told = Arc::new(tokio::sync::Notify::new());
    *fixture.guest.runner_held.lock().unwrap() = Some((told.clone(), false));

    let running: Vec<_> = (0..4)
        .map(|_| {
            let keeper = fixture.keeper.clone();
            tokio::spawn(async move { observed(&keeper, Some("challenge"), DEADLINE).await })
        })
        .collect();
    until("four at the runner", || {
        fixture.guest.runner_asked.load(SeqCst) == 4
    })
    .await;
    let busy = observed(&fixture.keeper, Some("challenge"), DEADLINE).await;
    *fixture.guest.runner_held.lock().unwrap() = None;
    told.notify_waiters();

    assert_eq!(busy["code"], "busy", "{busy}");
    for observing in running {
        let observed = observing.await.unwrap();
        assert_eq!(observed["ok"], true, "{observed}");
    }
    let after = observed(&fixture.keeper, Some("challenge"), DEADLINE).await;
    assert_eq!(after["ok"], true, "room again: {after}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_input_service_that_never_says_hello_is_observed_as_unavailable() {
    let (fixture, _running) = live(false).await;
    let saying = fixture.guest.proving();
    *saying.input.lock().unwrap() =
        Some(json!({"ok": false, "code": "platform", "error": "The input service is not running"}));

    let observed = observed(&fixture.keeper, Some("challenge"), DEADLINE).await;

    assert_eq!(observed["ok"], true, "{observed}");
    assert_eq!(
        observed["status"]["inputService"],
        json!({"available": false, "error": "The input service is not running"})
    );
    assert_eq!(observed["status"]["sessionProof"]["ok"], true);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_runner_that_cannot_be_asked_is_observed_as_such() {
    let (fixture, _running) = live(false).await;
    fixture.guest.proving();
    *fixture.guest.runner.lock().unwrap() = json!("not a reply");

    let observed = observed(&fixture.keeper, Some("challenge"), DEADLINE).await;

    assert_eq!(observed["ok"], true, "{observed}");
    assert_eq!(observed["runner"]["ok"], false, "{observed}");
    assert!(observed["runner"]["window"].is_null());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_runner_that_never_answers_is_given_up_at_the_deadline_its_thread_with_it() {
    let (fixture, _running) = live(false).await;
    fixture.guest.proving();
    let folder = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let socket = folder.path().join("control");
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    // It takes the request and says nothing, until the station hangs up.
    let silent = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut heard = Vec::new();
        let _ = std::io::Read::read_to_end(&mut stream, &mut heard);
        std::time::Instant::now()
    });
    *fixture.guest.runner_socket.lock().unwrap() = Some(socket);
    let deadline = Duration::from_secs(2);
    let started = std::time::Instant::now();

    let observed = observed(&fixture.keeper, Some("challenge"), deadline).await;
    let answered = started.elapsed();
    let hung_up = silent.join().unwrap().duration_since(started);

    assert_eq!(observed["ok"], true, "{observed}");
    assert_eq!(observed["runner"]["ok"], false, "{observed}");
    assert!(
        observed["runner"]["error"]
            .as_str()
            .unwrap()
            .contains("in time"),
        "{observed}"
    );
    let bound = share(deadline, RUNNER_BY) + Duration::from_secs(1);
    assert!(answered < bound, "answered after {answered:?}");
    assert!(hung_up < bound, "its socket was held {hung_up:?}");
}
