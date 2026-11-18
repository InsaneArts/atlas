use super::*;
use crate::{
    client::{Client, Mutation},
    doing::Doing,
    vm::keeper::{
        clients::tests::{Serving, asked, client_of, serving, serving_with},
        tests::{Saying, ran_hi, until},
    },
};
use atlas_control::executors::Executor;
use std::sync::{Arc, atomic::Ordering::SeqCst};

/// A keeper serving a guest that proves itself, its saying, and an agent's
/// client of it.
pub(in crate::vm::keeper) async fn proven() -> (Serving, Arc<Saying>, Client, tempfile::TempDir) {
    let serving = serving(true).await;
    let saying = serving.fixture.guest.proving();
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path()).doing(Doing::new("run"));
    (serving, saying, client, directory)
}

/// Every lane the guest admitted, in order.
pub(in crate::vm::keeper) fn admitted(serving: &Serving) -> Vec<atlas_session::lane::Header> {
    serving
        .fixture
        .guest
        .held_lanes
        .lock()
        .unwrap()
        .iter()
        .flat_map(|dialer| dialer.admitted.lock().unwrap().clone())
        .collect()
}

/// The changes the station settled with the guest.
pub(in crate::vm::keeper) fn settled(serving: &Serving) -> Vec<Token> {
    serving
        .fixture
        .guest
        .standin
        .settled
        .lock()
        .unwrap()
        .clone()
}

/// What the guest keeps of `change`: none when it keeps nothing of it,
/// then none inside while it is carried out.
pub(in crate::vm::keeper) fn record(serving: &Serving, change: &Token) -> Option<Option<Value>> {
    serving
        .fixture
        .guest
        .standin
        .changes
        .lock()
        .unwrap()
        .get(change)
        .cloned()
}

/// The one change the guest keeps, once it was asked for one.
pub(super) async fn asked_for(serving: &Serving) -> Token {
    let changes = || {
        serving
            .fixture
            .guest
            .standin
            .changes
            .lock()
            .unwrap()
            .clone()
    };
    until("asked for", || changes().len() == 1).await;
    changes().into_keys().next().unwrap()
}

/// The one change the guest ran.
fn only_change(saying: &Saying) -> Token {
    let runs = saying.runs.lock().unwrap();
    assert_eq!(runs.len(), 1, "{runs:?}");
    Token::parse(runs[0]["change"].as_str().unwrap()).unwrap()
}

fn hi() -> Vec<String> {
    vec!["/bin/echo".to_owned(), "hi".to_owned()]
}

/// The generation live now.
pub(in crate::vm::keeper) fn live(serving: &Serving) -> u64 {
    serving.fixture.keeper.shared.state().generation.unwrap().0
}

/// Cut the guest's control connection and wait for the keeper's next
/// generation.
pub(in crate::vm::keeper) async fn restarted(serving: &Serving) {
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_command_on_a_vm_goes_on_its_change_lane_in_the_proved_generation_and_is_settled_after_its_claim()
 {
    let (serving, saying, client, _directory) = proven().await;
    let generation = live(&serving);
    let controller = client.controller.clone();
    let pending_when_settled = Arc::new(std::sync::Mutex::new(None));
    let seen = pending_when_settled.clone();
    serving.fixture.guest.standin.on_settled.set(move |_| {
        *seen.lock().unwrap() = Some(controller.pending().unwrap());
    });

    let reply = client
        .run_beside(hi(), Duration::from_secs(20))
        .await
        .unwrap();

    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(reply["exitCode"], 0);
    let shown = atlas_core::transport::with_output(reply);
    assert_eq!(shown["output"], "hi\n");
    let change = only_change(&saying);
    assert!(admitted(&serving).iter().any(|header| {
        header.kind
            == LaneKind::Change {
                change: change.clone(),
            }
            && header.generation.0 == generation
    }));
    let request = saying.runs.lock().unwrap()[0].clone();
    assert_eq!(request["session"]["securitySessionId"], 42, "{request}");
    assert!(client.controller.pending().unwrap().is_empty());
    until("settled with the guest", || {
        settled(&serving).contains(&change)
    })
    .await;
    assert_eq!(
        *pending_when_settled.lock().unwrap(),
        Some(Vec::new()),
        "its claim cleared before the guest was told"
    );
    assert_eq!(record(&serving, &change), None, "the guest forgot it");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_command_runs_on_a_vm_whose_screen_recording_is_not_allowed_yet() {
    let (_serving, saying, client, _directory) = proven().await;
    *saying.hello.lock().unwrap() = Some(|hello| {
        hello["permissions"]["screenCapture"] = json!(false);
    });

    let reply = client
        .run_beside(hi(), Duration::from_secs(20))
        .await
        .unwrap();

    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(atlas_core::transport::with_output(reply)["output"], "hi\n");
    assert_eq!(saying.runs.lock().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lost_reply_is_recovered_from_what_the_guest_kept_with_what_it_printed() {
    let (serving, saying, client, _directory) = proven().await;
    serving.fixture.guest.standin.dropping.store(1, SeqCst);

    let reply = client
        .run_beside(hi(), Duration::from_secs(20))
        .await
        .unwrap();

    assert_eq!(reply["exitCode"], 0, "{reply}");
    assert_eq!(reply["truncated"], false);
    assert_eq!(atlas_core::transport::with_output(reply)["output"], "hi\n");
    let change = only_change(&saying);
    assert!(client.controller.pending().unwrap().is_empty());
    until("settled with the guest", || {
        settled(&serving).contains(&change)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_recovered_reply_keeps_its_cut_and_one_kept_without_its_output_says_it_is_unavailable() {
    let (serving, saying, client, _directory) = proven().await;
    let guest = &serving.fixture.guest.standin;
    let mut cut = ran_hi();
    cut["truncated"] = json!(true);
    *saying.ran.lock().unwrap() = Some(cut);
    guest.dropping.store(1, SeqCst);

    let truncated = client
        .run_beside(hi(), Duration::from_secs(20))
        .await
        .unwrap();
    *saying.ran.lock().unwrap() = Some(json!({"ok": true, "exitCode": 0, "timedOut": false,
        "shortened": true}));
    guest.dropping.store(1, SeqCst);
    let unavailable = client
        .run_beside(hi(), Duration::from_secs(20))
        .await
        .unwrap();

    assert_eq!(truncated["truncated"], true, "{truncated}");
    assert_eq!(truncated["outputTruncated"], true, "{truncated}");
    let shown = atlas_core::transport::with_output(truncated);
    assert_eq!(
        shown["outputTruncated"], true,
        "kept once its output is read"
    );
    let shown = atlas_core::transport::with_output(unavailable);
    assert_eq!(shown["output"], "unavailable", "{shown}");
    assert_eq!(shown["exitCode"], 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_change_carried_through_a_restart_is_kept_while_claimed_then_answered_and_settled() {
    let (serving, _saying, client, _directory) = proven().await;
    let guest = serving.fixture.guest.standin.clone();
    guest.holding.store(true, SeqCst);
    let running = tokio::spawn({
        let client = client.clone();
        async move { client.run_beside(hi(), Duration::from_secs(20)).await }
    });
    let change = asked_for(&serving).await;

    restarted(&serving).await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(!settled(&serving).contains(&change));
    assert_eq!(client.controller.pending().unwrap(), [change.as_str()]);
    guest.holding.store(false, SeqCst);
    let reply = running.await.unwrap().unwrap();

    assert_eq!(reply["exitCode"], 0, "{reply}");
    assert!(client.controller.pending().unwrap().is_empty());
    until("settled in the new generation", || {
        settled(&serving).contains(&change)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stop_mid_command_cancels_its_lane_and_leaves_its_change_pending() {
    let (serving, _saying, client, _directory) = proven().await;
    let guest = serving.fixture.guest.standin.clone();
    guest.holding.store(true, SeqCst);
    let running = tokio::spawn({
        let client = client.clone();
        async move { client.run_beside(hi(), Duration::from_secs(20)).await }
    });
    let change = asked_for(&serving).await;

    client.controller.note_stop(true).unwrap();
    let stopped = running.await.unwrap().unwrap_err();

    assert_eq!(stopped.code, Code::Stopped, "{stopped:?}");
    assert!(stopped.uncertain);
    assert_eq!(
        stopped.error,
        "The person stopped agents on work. Ask them before doing anything more there."
    );
    let next = stopped.details["next"].as_str().unwrap();
    assert!(
        next.contains("stays pending until work answers what it came to")
            && next.contains("atlas --desktop work status")
            && !next.contains("restart")
            && !next.contains("screenshot"),
        "{next}"
    );
    until("its lane cancelled", || {
        record(&serving, &change).flatten().is_some()
    })
    .await;
    assert_eq!(
        record(&serving, &change).flatten().unwrap()["code"],
        "cancelled"
    );
    assert_eq!(client.controller.pending().unwrap(), [change.as_str()]);
    assert_eq!(
        client.status().await["station"]["pending"],
        json!([change]),
        "status says it is pending"
    );
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(!settled(&serving).contains(&change), "still claimed");
    until("the guest stopped", || guest.stops.load(SeqCst) > 0).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_change_for_a_generation_that_ended_is_refused_unsaid_and_its_claim_cleared() {
    let (serving, saying, client, _directory) = proven().await;
    let stale = live(&serving);
    restarted(&serving).await;
    let mutation = Mutation::Run {
        argv: hi(),
        timeout: Duration::from_secs(20),
        pin: None,
    };

    let refused = client.mutate_vm(mutation, stale, None).await.unwrap();

    assert_eq!(refused["code"], "session-changed", "{refused}");
    assert_eq!(refused["sent"], false);
    assert!(client.controller.pending().unwrap().is_empty(), "cleared");
    assert!(saying.runs.lock().unwrap().is_empty(), "nothing said");
    assert!(
        admitted(&serving)
            .iter()
            .all(|header| !matches!(header.kind, LaneKind::Change { .. }))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_change_is_carried_only_as_asked_and_refused_unsaid_otherwise() {
    let serving = serving(true).await;
    let saying = serving.fixture.guest.proving();
    let generation = live(&serving);
    let change = Token::new();
    let change_of = |lane: &str, generation: Value, named: &Token| {
        json!({"op": "change", "lane": lane, "change": change, "generation": generation,
            "request": {"op": "run", "argv": ["/bin/echo", "hi"], "change": named}})
    };

    let carried = asked(
        &serving.socket,
        &change_of("change", json!(generation), &change),
    )
    .await;
    for refused in [
        change_of("stage", json!(generation), &change),
        change_of("change", json!("live"), &change),
        change_of("change", json!(generation), &Token::new()),
        json!({"op": "change", "lane": "change", "generation": generation}),
    ] {
        let answered = asked(&serving.socket, &refused).await;
        assert_eq!(answered.len(), 1, "{refused}");
        assert_eq!(answered[0]["code"], "bad-request", "{refused}");
        assert_eq!(answered[0]["sent"], false, "{refused}");
    }

    assert_eq!(carried, [ran_hi()]);
    assert_eq!(saying.runs.lock().unwrap().len(), 1, "only the one asked");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_that_keeps_no_change_records_is_told_nothing_and_asked_to_update() {
    let serving = serving_with(true, true).await;
    let change = Token::new();

    let answered = asked(
        &serving.socket,
        &json!({"op": "change", "lane": "change", "change": change,
            "generation": live(&serving),
            "request": {"op": "run", "argv": ["/bin/echo"], "change": change}}),
    )
    .await;

    assert_eq!(answered[0]["code"], "update", "{answered:?}");
    assert_eq!(answered[0]["sent"], false);
    assert!(
        admitted(&serving)
            .iter()
            .all(|header| !matches!(header.kind, LaneKind::Change { .. }))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_client_that_hangs_up_before_its_final_reply_has_its_changes_lane_cancelled() {
    let serving = serving(true).await;
    serving.fixture.guest.proving();
    let guest = serving.fixture.guest.standin.clone();
    guest.holding.store(true, SeqCst);
    let change = Token::new();
    let request = json!({"op": "change", "lane": "change", "change": change,
        "generation": live(&serving),
        "request": {"op": "run", "argv": ["/bin/echo", "hi"], "change": change}});
    let mut stream = UnixStream::connect(&serving.socket).await.unwrap();
    let mut line = serde_json::to_vec(&request).unwrap();
    line.push(b'\n');
    stream.write_all(&line).await.unwrap();
    until("asked for", || record(&serving, &change).is_some()).await;

    drop(stream);

    until("its lane cancelled", || {
        record(&serving, &change).flatten().is_some()
    })
    .await;
    assert_eq!(
        record(&serving, &change).flatten().unwrap()["code"],
        "cancelled"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_carried_change_is_settled_once_the_station_holds_no_claim_on_it() {
    let serving = serving(true).await;
    serving.fixture.guest.proving();
    let stop = serving.fixture.keeper.stop.clone();
    let change = Token::new();
    let claim = stop
        .quarantine_by(change.as_str(), Executor::Guest)
        .unwrap();

    let answered = asked(
        &serving.socket,
        &json!({"op": "change", "lane": "change", "change": change,
            "generation": live(&serving),
            "request": {"op": "run", "argv": ["/bin/echo", "hi"], "change": change}}),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let kept_while_claimed = !settled(&serving).contains(&change);
    stop.clear_if(&claim).unwrap();

    assert_eq!(answered, [ran_hi()]);
    assert!(kept_while_claimed);
    until("settled once unclaimed", || {
        settled(&serving).contains(&change)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hello_settles_only_the_changes_the_station_holds_no_claim_on_and_none_while_its_marker_cannot_be_read()
 {
    let serving = serving(true).await;
    let guest = serving.fixture.guest.standin.clone();
    let stop = serving.fixture.keeper.stop.clone();
    let (claimed, unclaimed, unread) = (Token::new(), Token::new(), Token::new());
    for change in [&claimed, &unclaimed] {
        guest.ask(change);
        guest.done(change, &json!({"ok": true}));
    }
    let _claim = stop
        .quarantine_by(claimed.as_str(), Executor::Guest)
        .unwrap();

    restarted(&serving).await;
    until("the unclaimed one settled", || {
        settled(&serving).contains(&unclaimed)
    })
    .await;
    assert!(!settled(&serving).contains(&claimed));
    guest.ask(&unread);
    guest.done(&unread, &json!({"ok": true}));
    std::fs::write(stop.marker_path(), b"[").unwrap();
    restarted(&serving).await;
    tokio::time::sleep(Duration::from_millis(1500)).await;

    assert!(!settled(&serving).contains(&unread));
    assert!(!settled(&serving).contains(&claimed));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_settlement_names_only_what_the_station_holds_no_claim_on() {
    let serving = serving(true).await;
    let stop = serving.fixture.keeper.stop.clone();
    let (claimed, free) = (Token::new(), Token::new());
    let _claim = stop
        .quarantine_by(claimed.as_str(), Executor::Guest)
        .unwrap();

    let answered = asked(
        &serving.socket,
        &json!({"op": "settle", "changes": [claimed, free]}),
    )
    .await;
    let empty = asked(&serving.socket, &json!({"op": "settle", "changes": []})).await;

    assert_eq!(answered[0]["settled"], json!([free]), "{answered:?}");
    assert_eq!(empty[0]["code"], "bad-request");
    until("said to the guest", || settled(&serving).contains(&free)).await;
    assert!(!settled(&serving).contains(&claimed));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_read_outside_what_the_keeper_carries_is_refused_before_any_lane_opens() {
    let (serving, _saying, client, _directory) = proven().await;
    let keeper = &serving.fixture.keeper;
    until("the guest's report read", || {
        keeper.shared.state().report.is_some()
    })
    .await;
    let before = admitted(&serving).len();

    let refused = asked(
        &serving.socket,
        &json!({"op": "read", "request": {"op": "apps"}}),
    )
    .await;
    let unsent = client
        .link
        .call(&json!({"op": "screenshot"}))
        .await
        .unwrap_err();
    let hello = client.link.call(&json!({"op": "hello"})).await.unwrap();

    assert_eq!(refused[0]["code"], "denied", "{refused:?}");
    assert_eq!(unsent.error, client.unreachable().error);
    assert_eq!(hello["op"], "hello", "{hello}");
    assert_eq!(admitted(&serving).len(), before + 1, "only the hello");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_command_not_built_for_a_vm_is_refused_in_one_wording_before_anything_is_asked() {
    let (serving, saying, client, _directory) = proven().await;
    let keeper = &serving.fixture.keeper;
    until("the guest's report read", || {
        keeper.shared.state().report.is_some()
    })
    .await;
    let before = admitted(&serving).len();
    let one_wording = client.unreachable().error;
    let in_an_app = crate::agent::Expected::new(
        Some(crate::agent::Target::Name("Finder".into())),
        None,
        None,
    )
    .unwrap()
    .unwrap();

    let refused = [
        client.start_input().await.unwrap_err(),
        client.identity().await.unwrap_err(),
        client.apps().await.unwrap_err(),
        crate::agent::list(&client, &crate::agent::WindowFilter::default())
            .await
            .unwrap_err(),
        crate::agent::act(
            &client,
            Some(&in_an_app),
            &crate::agent::click(10.0, 10.0, atlas_core::actions::Button::Left, 1, &[]),
        )
        .await
        .unwrap_err(),
        crate::agent::type_text(&client, "hi", Some(&in_an_app))
            .await
            .unwrap_err(),
        crate::agent::click_reference(
            &client,
            None,
            "8123abc.12",
            atlas_core::actions::Button::Left,
            1,
            &[],
        )
        .await
        .unwrap_err(),
        client.restart().await.unwrap_err(),
    ];

    for refused in refused {
        assert_eq!(refused.error, one_wording, "{refused:?}");
        assert!(!refused.uncertain, "{refused:?}");
    }
    assert_eq!(admitted(&serving).len(), before, "no lane opened");
    assert!(saying.challenges.lock().unwrap().is_empty(), "never proved");
    assert!(client.controller.pending().unwrap().is_empty());
}

/// What the keeper answers on its socket to `said`, sent as it is, its
/// writing ended once it is sent.
async fn answered_to(socket: &std::path::Path, said: &[u8]) -> Vec<Value> {
    let mut stream = UnixStream::connect(socket).await.unwrap();
    stream.write_all(said).await.unwrap();
    stream.shutdown().await.unwrap();
    let mut answered = String::new();
    tokio::time::timeout(
        Duration::from_secs(30),
        tokio::io::AsyncReadExt::read_to_string(&mut stream, &mut answered),
    )
    .await
    .expect("the keeper answered")
    .unwrap();
    answered
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_change_not_said_whole_with_its_newline_is_refused_and_nothing_is_opened() {
    let serving = serving(true).await;
    let saying = serving.fixture.guest.proving();
    let change = Token::new();
    let whole = serde_json::to_vec(&json!({"op": "change", "lane": "change",
        "change": change, "generation": live(&serving),
        "request": {"op": "run", "argv": ["/bin/echo", "hi"], "change": change}}))
    .unwrap();

    let unended = answered_to(&serving.socket, &whole).await;
    let cut = answered_to(&serving.socket, &whole[..whole.len() / 2]).await;

    for refused in [&unended, &cut] {
        assert_eq!(refused.len(), 1, "{refused:?}");
        assert_eq!(refused[0]["code"], "bad-request", "{refused:?}");
        assert_eq!(refused[0]["sent"], false, "{refused:?}");
    }
    assert!(saying.runs.lock().unwrap().is_empty(), "nothing ran");
    assert_eq!(
        record(&serving, &change),
        None,
        "nothing asked of the guest"
    );
    assert!(
        admitted(&serving)
            .iter()
            .all(|header| !matches!(header.kind, LaneKind::Change { .. }))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_command_printing_megabytes_in_many_events_is_passed_on_whole() {
    let serving = serving(true).await;
    let saying = serving.fixture.guest.proving();
    let chunk = "A".repeat(5464);
    // Past 4 MiB in all as it goes, each line well within its bound.
    let mut lines: Vec<Value> = (0..800)
        .map(|_| json!({"event": "output", "data": chunk}))
        .collect();
    lines.push(ran_hi());
    *saying.ran.lock().unwrap() = Some(json!(lines));
    let change = Token::new();

    let answered = asked(
        &serving.socket,
        &json!({"op": "change", "lane": "change", "change": change,
            "generation": live(&serving),
            "request": {"op": "run", "argv": ["/bin/echo", "hi"], "change": change}}),
    )
    .await;

    assert_eq!(answered.len(), 801);
    assert_eq!(answered[800], ran_hi());
    let mut reply = answered[800].clone();
    reply["events"] = json!(answered[..800]);
    let printed = atlas_core::transport::run_output(&reply, 8 << 20).unwrap();
    assert_eq!(printed.len(), 800 * 4098);
}

/// What `relay` makes of a guest that says `said` and ends its lane: how it
/// ended, and what reached the client.
async fn relayed(said: Vec<u8>) -> (Carrying, Vec<u8>) {
    let (keeper, mut guest) = tokio::io::duplex(64 * 1024);
    let saying = tokio::spawn(async move {
        let mut request = [0; 1];
        while tokio::io::AsyncReadExt::read(&mut guest, &mut request)
            .await
            .is_ok_and(|read| read > 0 && request[0] != b'\n')
        {}
        let _ = guest.write_all(&said).await;
    });
    let mut client = Vec::new();
    let between = std::sync::atomic::AtomicBool::new(true);
    let carried = relay(keeper, &json!({"op": "run"}), &mut client, false, &between).await;
    saying.abort();
    (carried, client)
}

#[tokio::test]
async fn a_reply_line_cut_short_or_too_long_is_never_passed_on_and_leaves_the_change_uncertain() {
    let event = b"{\"event\":\"output\",\"data\":\"aGkK\"}\n".to_vec();
    let mut cut = event.clone();
    cut.extend_from_slice(b"{\"ok\":true,\"exit");
    let mut long = event.clone();
    long.extend_from_slice(b"{\"event\":\"output\",\"data\":\"");
    long.extend(std::iter::repeat_n(b'A', 4 << 20));

    for said in [cut, long] {
        let (carried, client) = relayed(said).await;

        assert!(matches!(carried, Carrying::Lost(_)));
        assert_eq!(client, event, "only the whole line before it");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn what_a_hello_could_not_settle_while_the_marker_was_busy_is_settled_on_a_later_tick() {
    let serving = serving(true).await;
    let guest = serving.fixture.guest.standin.clone();
    let stop = serving.fixture.keeper.stop.clone();
    let unclaimed = Token::new();
    guest.ask(&unclaimed);
    guest.done(&unclaimed, &json!({"ok": true}));
    // A marker there, its lock held by another process's control.
    let _other = stop
        .quarantine_by(Token::new().as_str(), Executor::Guest)
        .unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(stop.marker_lock_path())
        .unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();

    restarted(&serving).await;
    let generation = live(&serving);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let kept_while_busy = !settled(&serving).contains(&unclaimed);
    fs2::FileExt::unlock(&lock).unwrap();

    assert!(kept_while_busy);
    until("settled on a later tick", || {
        settled(&serving).contains(&unclaimed)
    })
    .await;
    assert_eq!(live(&serving), generation, "with no reconnect");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_client_that_stops_reading_is_left_with_its_connections_end_never_a_cut_line_and_a_word_after_it()
 {
    let serving = serving(true).await;
    let saying = serving.fixture.guest.proving();
    let chunk = "A".repeat(5464);
    let mut lines: Vec<Value> = (0..800)
        .map(|_| json!({"event": "output", "data": chunk}))
        .collect();
    lines.push(ran_hi());
    *saying.ran.lock().unwrap() = Some(json!(lines));
    let change = Token::new();
    let request = json!({"op": "change", "lane": "change", "change": change,
        "generation": live(&serving), "deadline_ms": 1000,
        "request": {"op": "run", "argv": ["/bin/echo", "hi"], "change": change}});
    let mut stream = UnixStream::connect(&serving.socket).await.unwrap();
    let mut line = serde_json::to_vec(&request).unwrap();
    line.push(b'\n');
    stream.write_all(&line).await.unwrap();

    // Read nothing past the change's deadline and the keeper's last word.
    tokio::time::sleep(Duration::from_secs(4)).await;
    let mut said = Vec::new();
    tokio::time::timeout(
        Duration::from_secs(10),
        tokio::io::AsyncReadExt::read_to_end(&mut stream, &mut said),
    )
    .await
    .expect("the keeper let the connection go")
    .unwrap();

    let text = String::from_utf8(said).unwrap();
    let (whole, cut) = text.rsplit_once('\n').unwrap_or(("", &text));
    let lines: Vec<Value> = whole
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(lines.len() < 801, "it was cut through the deadline");
    if cut.is_empty() {
        let last = lines.last().unwrap();
        assert!(last["event"] == "output" || last["sent"] == true, "{last}");
    } else {
        assert!(
            lines.iter().all(|line| line["event"] == "output"),
            "nothing said after a line cut short"
        );
    }
}

/// How a test makes the guest's hello say its Stop.
type StopSaid = fn(&mut Value);

fn stopped_by_the_person(hello: &mut Value) {
    hello["stop"] = json!({"state": "stopped", "why": "person"});
}

fn stopped_by_a_lost_session(hello: &mut Value) {
    hello["stop"] = json!({"state": "stopped", "why": "control-lost"});
}

fn stopped_to_take_screen_recording(hello: &mut Value) {
    hello["stop"] = json!({"state": "stopped", "why": "recording"});
}

fn stop_not_known(hello: &mut Value) {
    hello["stop"] = json!({"state": "unknown"});
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vm_holding_agents_says_why_in_its_own_words_never_the_helpers() {
    let (serving, saying, client, _directory) = proven().await;
    let cases: [(StopSaid, &str); 4] = [
        (stopped_by_the_person, "The person stopped agents on work."),
        (
            stopped_by_a_lost_session,
            "work lost its session with this Mac, so it holds agents until the person resumes them",
        ),
        (
            stopped_to_take_screen_recording,
            "work's Atlas restarted to take Screen Recording, so it holds agents until the person \
             resumes them",
        ),
        (stop_not_known, "work's own Stop cannot be read"),
    ];

    for (stop, said) in cases {
        *saying.hello.lock().unwrap() = Some(stop);
        let held = [
            client
                .run_beside(hi(), Duration::from_secs(20))
                .await
                .unwrap_err(),
            client.require_going().await.unwrap_err(),
        ];
        for held in held {
            assert_eq!(held.code, Code::Stopped, "{held:?}");
            assert!(held.error.starts_with(said), "{held:?}");
            assert!(!held.error.contains("helper"), "{held:?}");
        }
    }
    *saying.hello.lock().unwrap() = None;
    client.controller.note_stop(true).unwrap();
    let station = client.require_going().await.unwrap_err();
    assert_eq!(
        station.error,
        "The person stopped agents on work. Ask them before doing anything more there."
    );
    assert!(saying.runs.lock().unwrap().is_empty());
    assert!(
        admitted(&serving)
            .iter()
            .all(|header| !matches!(header.kind, LaneKind::Change { .. }))
    );
}
