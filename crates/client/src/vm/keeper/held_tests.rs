//! The person's maintenance through a real keeper on its socket, its guest
//! a stand-in: every agent's request refused in its words while it is kept,
//! what maintenance and recovery need still served, and a hold that answers
//! only once every agent's lane it cancelled is gone.
use super::*;
use crate::{
    client::Client,
    link::Link,
    test_support::{StandInRunner, under_maintenance},
    vm::{
        keeper::{
            change::tests::{live, proven},
            clients::{
                Grant, bind, serve,
                tests::{Admitting, Serving, asked, serving},
            },
            picture::tests::{begun, picture, rest, session},
            tests::{Saying, next, until},
        },
        lifecycle::record::{Ended, Phase},
        maintenance::{Maintain, maintained_by},
        runner::socket_in,
    },
};
use atlas_session::credit::BULK;
use std::sync::{Arc, Mutex, mpsc};

/// Keep the person's maintenance of the keeper's VM at `phase`: its ID.
fn maintained(serving: &Serving, phase: Phase) -> Token {
    let keeper = &serving.fixture.keeper;
    under_maintenance(keeper.lifecycle.home(), &keeper.vm, phase)
}

/// End the maintenance kept for the keeper's VM, as Atlas VM confirmed its
/// window takes no input.
fn ended(serving: &Serving) {
    let keeper = &serving.fixture.keeper;
    let mut record = keeper.lifecycle.record(&keeper.vm).unwrap().unwrap();
    let maintenance = record.maintenance.as_mut().unwrap();
    maintenance.phase = Phase::Closing;
    maintenance.ended = Some(Ended {
        by: "run-1".to_owned(),
        close: Token::new(),
        earlier: None,
        earlier_gone: None,
    });
    keeper.lifecycle.keep_record(&record).unwrap();
}

/// What the keeper answers `request` with, its one line.
async fn one(serving: &Serving, request: &Value) -> Value {
    asked(&serving.socket, request).await.remove(0)
}

/// Whether `answer` is the refusal of an agent while the VM `work` is
/// under maintenance, as the person holds it.
fn held_on(answer: &Value) -> bool {
    answer["ok"] == false
        && answer["sent"] == false
        && answer["uncertain"] == false
        && answer["code"] == "busy"
        && answer["holder"] == "person"
        && answer["error"]
            .as_str()
            .is_some_and(|said| said.starts_with("work is under maintenance"))
}

/// Every agent's request the keeper carries, in the generation `generation`.
fn agents_requests(generation: u64) -> Vec<(&'static str, Value)> {
    let change = Token::new();
    vec![
        (
            "a job's log",
            json!({"op": "read", "request": {"op": "job", "action": "log", "id": "0123456789ab"}}),
        ),
        (
            "a list of apps",
            json!({"op": "read", "request": {"op": "apps", "session": session()},
                "generation": generation}),
        ),
        (
            "an installed read",
            json!({"op": "read", "request": {"op": "installed", "names": ["Firefox"],
                "session": session()}, "generation": generation}),
        ),
        (
            "a native read",
            json!({"op": "read", "request": {"op": "native", "action": "files",
                "directory": "/private/tmp/atlas-canary.Ab12Cd", "session": session()},
                "generation": generation}),
        ),
        ("a proof", json!({"op": "observe", "challenge": "c1"})),
        (
            "a run",
            json!({"op": "change", "lane": "change", "change": change,
                "generation": generation, "request": {"op": "run", "change": change}}),
        ),
        (
            "a stage",
            json!({"op": "stage", "change": change, "generation": generation,
                "request": {"op": "stage", "change": change}}),
        ),
        (
            "a screenshot",
            picture(
                json!({"op": "screenshot", "session": session()}),
                generation,
            ),
        ),
        (
            "a file's read",
            picture(
                json!({"op": "read", "path": "a.bin", "offset": 0, "length": 4,
                    "maxSize": 4, "session": session()}),
                generation,
            ),
        ),
        (
            "elements",
            json!({"op": "read", "lane": "input", "generation": generation,
                "request": {"op": "elements", "epoch": {}, "expect": {}}}),
        ),
    ]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_agents_request_is_refused_in_the_maintenance_words_while_its_exceptions_are_served()
{
    let serving = serving(true).await;
    let generation = live(&serving);
    let change = maintained(&serving, Phase::Open);

    for (what, request) in agents_requests(generation) {
        let answer = one(&serving, &request).await;
        assert!(held_on(&answer), "{what}: {answer}");
        assert!(!answer.to_string().contains("helper"), "{what}: {answer}");
    }
    let outcome = Token::new();
    for (what, request) in [
        ("the keeper's hello", json!({"op": "hello"})),
        (
            "the guest's hello",
            json!({"op": "read", "request": {"op": "hello"}}),
        ),
        (
            "a change's outcome",
            json!({"op": "read", "request": {"op": "outcome", "change": outcome}}),
        ),
        (
            "leftovers",
            json!({"op": "read", "request": {"op": "leftovers"}}),
        ),
        (
            "a settlement",
            json!({"op": "settle", "changes": [Token::new()]}),
        ),
        (
            "a recovery",
            json!({"op": "recover", "generation": generation}),
        ),
        ("the person's Stop", json!({"op": "stop"})),
        ("the hold", json!({"op": "hold", "change": change})),
    ] {
        let answer = one(&serving, &request).await;
        assert!(!held_on(&answer), "{what}: {answer}");
        assert_eq!(answer["ok"], true, "{what}: {answer}");
    }
    let release = one(&serving, &json!({"op": "release", "change": change})).await;
    assert_eq!(release["code"], "state", "not ended: {release}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_maintenance_ending_or_a_record_that_cannot_be_read_holds_agents_too() {
    let serving = serving(true).await;
    let generation = live(&serving);
    maintained(&serving, Phase::Closing);
    let request = &agents_requests(generation)[0].1;

    let ending = one(&serving, request).await;
    let keeper = &serving.fixture.keeper;
    std::fs::write(keeper.lifecycle.record_at(&keeper.vm), b"{not a record").unwrap();
    let unreadable = one(&serving, request).await;
    let proof = one(&serving, &json!({"op": "observe", "challenge": "c1"})).await;

    assert_eq!(ending["code"], "busy", "{ending}");
    assert!(
        ending["error"]
            .as_str()
            .unwrap()
            .starts_with("work's maintenance is ending"),
        "{ending}"
    );
    for refused in [&unreadable, &proof] {
        assert_eq!(refused["code"], "state", "{refused}");
        assert_eq!(
            refused["error"],
            "Whether this VM is under maintenance cannot be read, so agents wait.",
            "{refused}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hold_names_the_maintenance_kept_and_with_no_lane_open_is_answered_at_once() {
    let serving = serving(true).await;
    let change = maintained(&serving, Phase::Holding);

    let other = one(&serving, &json!({"op": "hold", "change": Token::new()})).await;
    let unnamed = one(&serving, &json!({"op": "hold"})).await;
    let held = one(&serving, &json!({"op": "hold", "change": change})).await;
    let again = one(&serving, &json!({"op": "hold", "change": change})).await;

    assert_eq!(other["code"], "state", "{other}");
    assert_eq!(unnamed["code"], "bad-request", "{unnamed}");
    assert_eq!(held["ok"], true, "{held}");
    assert_eq!(held["change"], json!(change));
    assert_eq!(held["cancelled"], json!({}));
    assert_eq!(again["ok"], true, "asked again: {again}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hold_with_no_live_generation_is_answered_at_once() {
    let fixture = crate::vm::keeper::tests::fixture();
    let keeper = &fixture.keeper;
    let change = under_maintenance(keeper.lifecycle.home(), &keeper.vm, Phase::Holding);

    let held = tokio::time::timeout(
        Duration::from_secs(5),
        hold(keeper, &json!({"change": change}), Duration::from_secs(30)),
    )
    .await
    .expect("answered");

    assert!(keeper.shared.live().is_none());
    assert_eq!(held["ok"], true, "{held}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_collection_paused_mid_file_is_cancelled_with_its_credit_back_before_the_hold_answers() {
    let (serving, _saying, _client, _directory) = proven().await;
    let bytes: Vec<u8> = (0..1_536 * 1024).map(|at| (at % 251) as u8).collect();
    let standin = serving.fixture.guest.standin.clone();
    standin.pictures.set(Arc::new(move |_| Some(bytes.clone())));
    let read = json!({"op": "read", "path": "big.bin", "offset": 0, "length": 1_572_864,
        "maxSize": 1_572_864, "session": session()});
    let (mut paused, first) = begun(&serving, &picture(read.clone(), live(&serving))).await;
    let credit = standin.bulk_credit().unwrap();
    until("part of the file on its way", || {
        credit.left() < BULK as usize
    })
    .await;
    let change = maintained(&serving, Phase::Holding);

    let held = one(&serving, &json!({"op": "hold", "change": change})).await;

    assert_eq!(first["ok"], true, "{first}");
    assert_eq!(held["ok"], true, "{held}");
    assert_eq!(held["cancelled"], json!({"picture": 1}), "{held}");
    assert_eq!(
        serving.fixture.keeper.lanes.open_now(),
        0,
        "every lane gone"
    );
    let left = rest(&mut paused, Duration::from_secs(10)).await;
    assert!(left.len() < 1_572_864, "{} bytes came", left.len());
    until("the guest's pump ended", || !standin.pumped().is_empty()).await;
    assert!(!standin.pumped()[0].whole, "cancelled part way");
    until("every credit back", || credit.left() == BULK as usize).await;
    let again = one(&serving, &picture(read, live(&serving))).await;
    assert!(held_on(&again), "{again}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_keeper_started_again_between_holding_and_opening_admits_no_lane_and_holds_at_once() {
    let (serving, _saying, _client, _directory) = proven().await;
    let bytes: Vec<u8> = (0..1_536 * 1024).map(|at| (at % 251) as u8).collect();
    let standin = serving.fixture.guest.standin.clone();
    standin.pictures.set(Arc::new(move |_| Some(bytes.clone())));
    let read = json!({"op": "read", "path": "big.bin", "offset": 0, "length": 1_572_864,
        "maxSize": 1_572_864, "session": session()});
    let (mut paused, _) = begun(&serving, &picture(read, live(&serving))).await;
    let change = maintained(&serving, Phase::Holding);
    // The keeper ends before it is asked to hold: its generation goes with
    // it, and launchd starts the next.
    serving.fixture.keeper.shared.live().unwrap().retire();
    let started = next(&serving.fixture.keeper);
    at_start(&started).await;

    let left = rest(&mut paused, Duration::from_secs(10)).await;
    let lane = started.lanes.admit(Work::Picture).err();
    let refused = admitted(&started, Work::Read).await.err().unwrap();
    let held = hold(&started, &json!({"change": change}), Duration::from_secs(5)).await;

    assert!(left.len() < 1_572_864, "{} bytes came", left.len());
    until("the guest's pump ended", || !standin.pumped().is_empty()).await;
    assert!(!standin.pumped()[0].whole, "ended with its generation");
    assert_eq!(
        lane.and_then(|hold| hold.change),
        Some(change),
        "held from its start"
    );
    assert!(held_on(&refused), "{refused}");
    assert_eq!(held["ok"], true, "{held}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_change_carried_at_a_hold_is_cancelled_and_said_lost_for_its_outcome_to_tell() {
    let (serving, saying, _client, _directory) = proven().await;
    // The guest takes the job's start and stays silent.
    *saying.otherwise.lock().unwrap() = Some(Arc::new(|_, _| Value::Null));
    let change = Token::new();
    let start = json!({"op": "change", "lane": "change", "change": change,
        "generation": live(&serving), "deadline_ms": 20_000,
        "request": {"op": "job", "action": "start", "change": change}});
    let socket = serving.socket.clone();
    let carrying = tokio::spawn(async move { asked(&socket, &start).await });
    until("the start's lane open", || {
        serving.fixture.keeper.lanes.open_now() == 1
    })
    .await;
    let kept = maintained(&serving, Phase::Holding);

    let held = one(&serving, &json!({"op": "hold", "change": kept})).await;
    let said = carrying.await.unwrap();

    assert_eq!(held["cancelled"], json!({"change": 1}), "{held}");
    let last = said.last().unwrap();
    assert_eq!(last["uncertain"], true, "{said:?}");
    assert!(
        last["error"]
            .as_str()
            .unwrap()
            .starts_with("work is under maintenance"),
        "{said:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_release_lets_agents_in_again_only_once_the_maintenance_is_recorded_ended() {
    let serving = serving(true).await;
    let generation = live(&serving);
    let change = maintained(&serving, Phase::Holding);
    let apps = &agents_requests(generation)[1].1;
    assert_eq!(
        one(&serving, &json!({"op": "hold", "change": change})).await["ok"],
        true
    );

    let open = one(&serving, &json!({"op": "release", "change": change})).await;
    ended(&serving);
    let other = one(&serving, &json!({"op": "release", "change": Token::new()})).await;
    let released = one(&serving, &json!({"op": "release", "change": change})).await;
    let still = one(&serving, apps).await;
    let keeper = &serving.fixture.keeper;
    let mut record = keeper.lifecycle.record(&keeper.vm).unwrap().unwrap();
    record.maintenance = None;
    keeper.lifecycle.keep_record(&record).unwrap();
    let admitted = one(&serving, apps).await;

    assert_eq!(open["code"], "state", "{open}");
    assert_eq!(other["code"], "state", "another maintenance: {other}");
    assert_eq!(released["ok"], true, "{released}");
    assert_eq!(still["code"], "busy", "the record still keeps it: {still}");
    assert!(!held_on(&admitted), "{admitted}");
    assert!(keeper.lanes.held().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lanes_held_for_a_maintenance_whose_record_went_stay_held_until_released() {
    let serving = serving(true).await;
    let generation = live(&serving);
    let change = maintained(&serving, Phase::Holding);
    assert_eq!(
        one(&serving, &json!({"op": "hold", "change": change})).await["ok"],
        true
    );
    let keeper = &serving.fixture.keeper;
    let mut record = keeper.lifecycle.record(&keeper.vm).unwrap().unwrap();
    record.maintenance = None;
    keeper.lifecycle.keep_record(&record).unwrap();

    let held = one(&serving, &agents_requests(generation)[1].1).await;
    let resume = one(
        &serving,
        &json!({"op": "resume", "capability": serving.capability,
            "revision": keeper.stop.stop_revision().unwrap()}),
    )
    .await;
    let released = one(&serving, &json!({"op": "release", "change": change})).await;

    assert!(held_on(&held), "{held}");
    assert_eq!(resume["code"], "busy", "{resume}");
    assert_eq!(released["ok"], true, "{released}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_persons_resume_is_refused_while_a_vm_is_under_maintenance_and_its_stop_stays() {
    let serving = serving(true).await;
    let keeper = &serving.fixture.keeper;
    keeper.stop.note_stop(true).unwrap();
    maintained(&serving, Phase::Open);
    let revision = json!(keeper.stop.stop_revision().unwrap());

    let resumed = one(
        &serving,
        &json!({"op": "resume", "capability": serving.capability, "revision": revision}),
    )
    .await;

    assert_eq!(resumed["code"], "busy", "{resumed}");
    assert_eq!(resumed["holder"], "person", "{resumed}");
    assert_eq!(
        resumed["error"],
        "work is under maintenance, so agents stay stopped. End it with atlas desktop maintain \
         work --end, then resume."
    );
    assert!(keeper.stop.stop_noted(), "the Stop stays");
    assert_eq!(
        serving
            .fixture
            .guest
            .standin
            .resumes
            .load(std::sync::atomic::Ordering::SeqCst),
        0,
        "the guest heard nothing"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_persons_resume_is_refused_while_the_windows_input_off_is_unanswered() {
    let serving = serving(true).await;
    let keeper = &serving.fixture.keeper;
    keeper.stop.note_stop(true).unwrap();
    maintained(&serving, Phase::Closing);
    let revision = json!(keeper.stop.stop_revision().unwrap());

    let resumed = one(
        &serving,
        &json!({"op": "resume", "capability": serving.capability, "revision": revision}),
    )
    .await;

    assert_eq!(resumed["code"], "busy", "{resumed}");
    assert!(
        resumed["error"]
            .as_str()
            .unwrap()
            .starts_with("Atlas VM has not confirmed that work's window takes no input"),
        "{resumed}"
    );
    assert!(keeper.stop.stop_noted(), "the Stop stays");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_resume_paused_after_its_check_holds_a_hold_off_until_it_ends_and_one_after_is_refused() {
    let serving = serving(true).await;
    let keeper = serving.fixture.keeper.clone();
    keeper.stop.note_stop(true).unwrap();
    until("the Stop applied", || keeper.applied().revision.is_some()).await;
    let revision = json!(keeper.stop.stop_revision().unwrap());
    // The guest carries the Resume out a while after it is told: the
    // keeper's check is behind it, its lift ahead.
    let (reached, reaching) = mpsc::channel::<()>();
    let reached = Mutex::new(reached);
    serving.fixture.guest.standin.on_lift.set(move || {
        let _ = reached.lock().unwrap().send(());
        std::thread::sleep(Duration::from_millis(1500));
    });
    let resume = json!({"op": "resume", "capability": serving.capability, "revision": revision});
    let socket = serving.socket.clone();
    let resuming = tokio::spawn(async move { asked(&socket, &resume).await.remove(0) });
    tokio::task::spawn_blocking(move || reaching.recv_timeout(Duration::from_secs(30)))
        .await
        .unwrap()
        .expect("the Resume reached the guest");
    let change = maintained(&serving, Phase::Holding);
    let socket = serving.socket.clone();
    let holding =
        tokio::spawn(async move { asked(&socket, &json!({"op": "hold", "change": change})).await });

    tokio::time::sleep(Duration::from_millis(300)).await;
    let waited = !holding.is_finished();
    let resumed = resuming.await.unwrap();
    let held = holding.await.unwrap().remove(0);
    let after = one(
        &serving,
        &json!({"op": "resume", "capability": serving.capability,
            "revision": keeper.stop.stop_revision().unwrap()}),
    )
    .await;

    assert!(waited, "the hold waited for the Resume's turn");
    assert_eq!(resumed["outcome"], "resumed", "{resumed}");
    assert_eq!(resumed["lifted"], "cleared", "{resumed}");
    assert_eq!(held["ok"], true, "{held}");
    assert_eq!(after["code"], "busy", "{after}");
    assert!(
        after["error"]
            .as_str()
            .unwrap()
            .starts_with("work is under maintenance"),
        "{after}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_client_is_refused_before_its_keeper_is_asked_and_the_persons_resume_too() {
    let (serving, _saying, client, _directory) = proven().await;
    let keeper = &serving.fixture.keeper;
    keeper.stop.note_stop(true).unwrap();
    // The station's own record, which the client reads, keeps it.
    let home = client.lifecycle.as_ref().unwrap().home().to_owned();
    under_maintenance(&home, &keeper.vm, Phase::Open);
    let asked_before = serving
        .fixture
        .guest
        .standin
        .resumes
        .load(std::sync::atomic::Ordering::SeqCst);

    let verified = client.verify_vm().await.err().unwrap();
    let looked = client.verify_vm_looking().await.err().unwrap();
    let recovered = client.recover_helper().await.err().unwrap();
    let mut viewer = client.clone();
    viewer.actor = crate::client::Actor::Viewer;
    let resumed = viewer.resume_vm(&serving.capability).await.err().unwrap();

    for refused in [&verified, &looked, &recovered] {
        assert_eq!(refused.code, Code::Busy, "{refused:?}");
        assert!(
            refused.error.starts_with("work is under maintenance"),
            "{refused:?}"
        );
    }
    assert_eq!(resumed.code, Code::Busy, "{resumed:?}");
    assert!(resumed.error.contains("then resume"), "{resumed:?}");
    assert!(keeper.stop.stop_noted(), "the Stop stays");
    assert_eq!(
        serving
            .fixture
            .guest
            .standin
            .resumes
            .load(std::sync::atomic::Ordering::SeqCst),
        asked_before
    );
}

/// Ask the keeper of `serving` for an observation, in a task of its own.
fn observing(serving: &Serving) -> tokio::task::JoinHandle<Vec<Value>> {
    let socket = serving.socket.clone();
    tokio::spawn(async move {
        asked(
            &socket,
            &json!({"op": "observe", "challenge": "c1", "deadline_ms": 20_000}),
        )
        .await
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_observation_whose_lanes_are_still_opening_at_a_hold_never_asks_a_proof_after_it() {
    let (serving, saying, _client, _directory) = proven().await;
    let guest = &serving.fixture.guest;
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    *guest.lanes_gate.lock().unwrap() = Some(gate.clone());
    let observation = observing(&serving);
    until("its lanes opening", || {
        guest
            .lanes_waiting
            .load(std::sync::atomic::Ordering::SeqCst)
            > 0
    })
    .await;
    let change = maintained(&serving, Phase::Holding);

    let held = tokio::time::timeout(
        Duration::from_secs(5),
        one(&serving, &json!({"op": "hold", "change": change})),
    )
    .await
    .expect("answered while the lanes were still opening");
    let observed = observation.await.unwrap().remove(0);
    gate.add_permits(16);
    tokio::time::sleep(Duration::from_millis(300)).await;

    assert_eq!(held["ok"], true, "{held}");
    assert_eq!(held["cancelled"], json!({"observe": 1}), "{held}");
    assert!(held_on(&observed), "{observed}");
    assert!(
        saying.challenges.lock().unwrap().is_empty(),
        "no proof reached the guest"
    );
    assert_eq!(serving.fixture.keeper.lanes.open_now(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_observation_paused_on_an_open_lane_is_cancelled_at_a_hold_and_asks_nothing_more() {
    let (serving, saying, _client, _directory) = proven().await;
    // The guest's input service opens its lane and stays silent.
    saying
        .input_stalls
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let observation = observing(&serving);
    until("its proof asked and its input hello paused", || {
        !saying.challenges.lock().unwrap().is_empty()
            && serving.fixture.keeper.lanes.open_now() == 1
    })
    .await;
    let proofs = saying.challenges.lock().unwrap().len();
    let change = maintained(&serving, Phase::Holding);

    let asked_at = tokio::time::Instant::now();
    let held = one(&serving, &json!({"op": "hold", "change": change})).await;
    let took = asked_at.elapsed();
    let observed = observation.await.unwrap().remove(0);
    tokio::time::sleep(Duration::from_millis(300)).await;

    assert_eq!(held["cancelled"], json!({"observe": 1}), "{held}");
    assert!(took < Duration::from_secs(5), "waited {took:?}");
    assert!(held_on(&observed), "{observed}");
    assert_eq!(saying.challenges.lock().unwrap().len(), proofs);
    assert_eq!(serving.fixture.keeper.lanes.open_now(), 0);
}

/// Make `client`, an agent's client of `serving`, the station the person's
/// maintenance is begun from: the keeper's own lifecycle records, the VM
/// added there, its guest saying no work agents started runs there, and
/// Atlas VM stood in for beside them, the VM running.
fn station_of(serving: &Serving, saying: &Saying, client: &mut Client) -> StandInRunner {
    let keeper = &serving.fixture.keeper;
    under_maintenance(keeper.lifecycle.home(), &keeper.vm, Phase::Holding);
    let mut record = keeper.lifecycle.record(&keeper.vm).unwrap().unwrap();
    record.maintenance = None;
    keeper.lifecycle.keep_record(&record).unwrap();
    client.lifecycle = Some(keeper.lifecycle.clone());
    if let Link::Keeper(link) = &client.link {
        client.link = Link::Keeper(link.clone().with_lifecycle(client.lifecycle.clone()));
    }
    *saying.otherwise.lock().unwrap() = Some(Arc::new(|_, request| {
        if request["op"] == "leftovers" {
            json!({"ok": true, "op": "leftovers", "leftovers": "none"})
        } else {
            json!({"ok": true, "echo": request})
        }
    }));
    StandInRunner::at(&socket_in(keeper.lifecycle.home(), &keeper.vm), "running")
}

/// The person's maintenance of `client`'s VM begun, every grant allowed.
async fn begin(client: &Client) -> atlas_core::error::Result<Value> {
    let client = client.clone();
    tokio::task::spawn_blocking(move || {
        maintained_by(
            &client,
            Maintain::default(),
            Duration::from_secs(10),
            &|_| Ok(Token::new()),
            &|_| {},
        )
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_collection_paused_mid_file_across_a_begin_is_cancelled_before_atlas_vm_is_asked_for_the_window()
 {
    let (serving, saying, mut client, _directory) = proven().await;
    let runner = station_of(&serving, &saying, &mut client);
    let bytes: Vec<u8> = (0..1_536 * 1024).map(|at| (at % 251) as u8).collect();
    let standin = serving.fixture.guest.standin.clone();
    standin.pictures.set(Arc::new(move |_| Some(bytes.clone())));
    let read = json!({"op": "read", "path": "big.bin", "offset": 0, "length": 1_572_864,
        "maxSize": 1_572_864, "session": session()});
    let (mut paused, first) = begun(&serving, &picture(read, live(&serving))).await;
    let credit = standin.bulk_credit().unwrap();
    until("part of the file on its way", || {
        credit.left() < BULK as usize
    })
    .await;
    let keeper = serving.fixture.keeper.clone();
    let at_window = Arc::new(Mutex::new(None));
    runner.on_window({
        let (keeper, seen) = (keeper.clone(), at_window.clone());
        move || *seen.lock().unwrap() = Some(keeper.lanes.open_now())
    });

    let begun = begin(&client).await.unwrap();

    assert_eq!(first["ok"], true, "{first}");
    assert_eq!(begun["maintenance"], "on", "{begun}");
    assert_eq!(runner.asked_for("window").len(), 1);
    assert_eq!(
        *at_window.lock().unwrap(),
        Some(0),
        "every lane gone before the window was asked"
    );
    let left = rest(&mut paused, Duration::from_secs(10)).await;
    assert!(left.len() < 1_572_864, "{} bytes came", left.len());
    until("every credit back", || credit.left() == BULK as usize).await;
    assert!(
        keeper.lanes.held().is_some(),
        "agents held while it is open"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_begin_kept_at_holding_goes_on_through_its_keeper_started_again_which_admits_nothing_before_the_window_is_asked()
 {
    let (serving, saying, mut client, directory) = proven().await;
    let runner = station_of(&serving, &saying, &mut client);
    let bytes: Vec<u8> = (0..1_536 * 1024).map(|at| (at % 251) as u8).collect();
    let standin = serving.fixture.guest.standin.clone();
    standin.pictures.set(Arc::new(move |_| Some(bytes.clone())));
    let read = json!({"op": "read", "path": "big.bin", "offset": 0, "length": 1_572_864,
        "maxSize": 1_572_864, "session": session()});
    let (mut paused, _) = begun(&serving, &picture(read, live(&serving))).await;
    // A begin kept its maintenance at holding, and the keeper ended before
    // it was asked to hold: its generation goes with it, launchd starts the
    // next on a socket of its own, and the begin is asked again.
    let change = maintained(&serving, Phase::Holding);
    serving.fixture.keeper.shared.live().unwrap().retire();
    let started = next(&serving.fixture.keeper);
    at_start(&started).await;
    let socket = directory.path().join("next.sock");
    let listener = bind(&socket).unwrap();
    let answering = tokio::spawn(serve(
        listener,
        started.clone(),
        Grant::new(serving.capability.clone()),
        Arc::new(Admitting(true)),
    ));
    let Link::Keeper(link) = &client.link else {
        panic!("a keeper's client");
    };
    client.link = Link::Keeper(link.clone().at_socket(socket));
    let at_window = Arc::new(Mutex::new(None));
    runner.on_window({
        let (started, seen) = (started.clone(), at_window.clone());
        move || *seen.lock().unwrap() = Some(started.lanes.admit(Work::Read).is_err())
    });

    let begun = begin(&client).await.unwrap();
    answering.abort();

    let left = rest(&mut paused, Duration::from_secs(10)).await;
    assert!(left.len() < 1_572_864, "{} bytes came", left.len());
    assert_eq!(begun["change"], json!(change), "{begun}");
    assert_eq!(
        *at_window.lock().unwrap(),
        Some(true),
        "held from its start, through the window's opening"
    );
    let kept = serving
        .fixture
        .keeper
        .lifecycle
        .record(&serving.fixture.keeper.vm)
        .unwrap()
        .unwrap()
        .maintenance
        .unwrap();
    assert_eq!(kept.phase, Phase::Open);
}

/// Forget the maintenance in the keeper's VM's record, as the station does
/// once it ended.
fn forgotten(serving: &Serving) {
    let keeper = &serving.fixture.keeper;
    let mut record = keeper.lifecycle.record(&keeper.vm).unwrap().unwrap();
    record.maintenance = None;
    keeper.lifecycle.keep_record(&record).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_keeper_started_after_its_release_was_answered_and_before_the_record_went_lets_agents_in_once_it_goes()
 {
    let serving = serving(true).await;
    let change = maintained(&serving, Phase::Holding);
    assert_eq!(
        one(&serving, &json!({"op": "hold", "change": change})).await["ok"],
        true
    );
    ended(&serving);
    let released = one(&serving, &json!({"op": "release", "change": change})).await;
    // The keeper ends here, and launchd starts the next, which reads the
    // record still keeping the maintenance ended.
    let started = next(&serving.fixture.keeper);
    at_start(&started).await;

    let while_kept = admitted(&started, Work::Read).await.err();
    forgotten(&serving);
    let once_gone = admitted(&started, Work::Read).await.is_ok();

    assert_eq!(released["ok"], true, "{released}");
    assert!(while_kept.is_some(), "held while the record keeps it");
    assert!(once_gone, "no release needed");
    assert!(started.lanes.held().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_keeper_that_read_the_record_before_its_socket_was_bound_lets_agents_in_once_it_goes() {
    let serving = serving(true).await;
    maintained(&serving, Phase::Open);
    let started = next(&serving.fixture.keeper);
    at_start(&started).await;
    let held_from_start = started.lanes.held();
    // The record goes before the keeper listens: no release ever reaches it.
    forgotten(&serving);
    let folder = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let socket = folder.path().join("next.sock");
    let listener = bind(&socket).unwrap();
    let answering = tokio::spawn(serve(
        listener,
        started.clone(),
        Grant::new(serving.capability.clone()),
        Arc::new(Admitting(true)),
    ));

    let apps = asked(&socket, &json!({"op": "read", "request": {"op": "copies"}})).await;
    answering.abort();

    assert!(
        held_from_start.is_some_and(|hold| !hold.asked),
        "held from the record"
    );
    assert!(!held_on(&apps[0]), "{apps:?}");
    assert!(started.lanes.held().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hold_the_station_asked_for_outlives_the_records_removal_until_it_is_released() {
    let serving = serving(true).await;
    let change = maintained(&serving, Phase::Holding);
    let started = next(&serving.fixture.keeper);
    at_start(&started).await;
    let held = hold(&started, &json!({"change": change}), Duration::from_secs(5)).await;
    forgotten(&serving);

    let after_removal = admitted(&started, Work::Read).await.err();
    let released = release(&started, &json!({"change": change})).await;
    let after_release = admitted(&started, Work::Read).await.is_ok();

    assert_eq!(held["ok"], true, "{held}");
    assert!(
        after_removal.as_ref().is_some_and(held_on),
        "{after_removal:?}"
    );
    assert_eq!(released["ok"], true, "{released}");
    assert!(after_release);
}
