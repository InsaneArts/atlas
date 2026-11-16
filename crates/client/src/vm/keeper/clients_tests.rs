use super::*;
use crate::vm::keeper::tests::{Fixture, fixture, next, until};
use std::sync::atomic::Ordering::SeqCst;

/// A client admitted or not, whoever it is.
struct Admitting(bool);

impl Admit for Admitting {
    fn admits(&self, _: BorrowedFd<'_>) -> bool {
        self.0
    }
}

/// A keeper live with its guest and answering clients on a socket of its
/// own: its fixture, its socket and its Resume capability.
struct Serving {
    fixture: Fixture,
    socket: std::path::PathBuf,
    capability: String,
    _folder: tempfile::TempDir,
    _tasks: Vec<tokio::task::JoinHandle<()>>,
}

async fn serving(admitted: bool) -> Serving {
    serving_with(admitted, false).await
}

/// [`serving`], its guest an older one that acknowledges nothing when `plain`.
async fn serving_with(admitted: bool, plain: bool) -> Serving {
    let fixture = fixture();
    fixture.guest.standin.plain.store(plain, SeqCst);
    let folder = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let socket = folder.path().join("keeper.sock");
    let capability = capability(folder.path()).unwrap();
    let listener = bind(&socket).unwrap();
    let keeper = fixture.keeper.clone();
    let running = tokio::spawn(async move { keeper.run().await });
    let answering = tokio::spawn(serve(
        listener,
        fixture.keeper.clone(),
        Grant::new(capability.clone()),
        Arc::new(Admitting(admitted)),
    ));
    until("live", || fixture.keeper.shared.live().is_some()).await;
    Serving {
        fixture,
        socket,
        capability,
        _folder: folder,
        _tasks: vec![running, answering],
    }
}

/// Every line the keeper answers `request` with, until it closes.
async fn asked(socket: &Path, request: &Value) -> Vec<Value> {
    let mut stream = UnixStream::connect(socket).await.unwrap();
    let mut line = serde_json::to_vec(request).unwrap();
    line.push(b'\n');
    stream.write_all(&line).await.unwrap();
    let mut answered = String::new();
    tokio::time::timeout(
        Duration::from_secs(30),
        stream.read_to_string(&mut answered),
    )
    .await
    .expect("the keeper answered")
    .unwrap();
    answered
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn only_a_client_of_this_account_whose_running_code_is_signed_as_atlas_is_admitted() {
    assert!(judge_client(Some(501), 501, true));
    assert!(!judge_client(Some(502), 501, true), "another account");
    assert!(!judge_client(Some(501), 501, false), "not signed as Atlas");
    assert!(
        !judge_client(None, 501, true),
        "an account that cannot be told"
    );
}

#[test]
fn the_resume_capability_is_admitted_only_whole() {
    let grant = Grant::new("ab12".to_owned());

    assert!(grant.admits(Some("ab12")));
    for given in [
        None,
        Some(""),
        Some("ab1"),
        Some("ab123"),
        Some("AB12"),
        Some("viewer"),
    ] {
        assert!(!grant.admits(given), "{given:?}");
    }
}

#[test]
fn the_resume_capability_is_random_and_kept_private_in_the_station_folder() {
    use std::os::unix::fs::PermissionsExt;
    let folder = tempfile::tempdir().unwrap();

    let first = capability(folder.path()).unwrap();
    let again = capability(folder.path()).unwrap();

    assert_eq!(first.len(), 64);
    assert_ne!(first, again);
    let kept = folder.path().join(RESUME_KEY);
    assert_eq!(std::fs::read_to_string(&kept).unwrap(), again);
    assert_eq!(
        std::fs::metadata(&kept).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[tokio::test]
async fn a_socket_left_by_a_keeper_that_ended_is_replaced_and_anything_else_refused() {
    let folder = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let socket = folder.path().join("keeper.sock");
    drop(std::os::unix::net::UnixListener::bind(&socket).unwrap());

    assert!(bind(&socket).is_ok());
    std::fs::remove_file(&socket).unwrap();
    std::fs::write(&socket, b"").unwrap();
    assert_eq!(bind(&socket).unwrap_err().code, Code::State);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hello_says_the_keeper_its_guest_and_the_live_generation() {
    let serving = serving(true).await;

    let hello = asked(&serving.socket, &json!({"op": "hello"})).await;

    let hello = &hello[0];
    assert_eq!(hello["ok"], true, "{hello}");
    assert_eq!(hello["keeper"]["protocol"], PROTOCOL);
    assert_eq!(hello["keeper"]["capabilities"], json!(CAPABILITIES));
    assert_eq!(hello["live"], true);
    assert!(hello["generation"].is_u64());
    assert!(
        hello["guest"]["capabilities"]
            .as_array()
            .unwrap()
            .contains(&json!(atlas_session::compatibility::ACKNOWLEDGED))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stop_and_a_resume_are_answered_by_their_own_numbers() {
    let serving = serving(true).await;

    let stop = asked(&serving.socket, &json!({"op": "stop"})).await;
    let resume = asked(
        &serving.socket,
        &json!({"op": "resume", "capability": serving.capability, "revision": null}),
    )
    .await;

    assert_eq!(
        stop,
        [json!({"ok": true, "said": true, "seq": 1, "outcome": "stopped"})]
    );
    assert_eq!(
        resume,
        [
            json!({"ok": true, "said": true, "seq": 2, "outcome": "resumed", "lifted": "already-clear"})
        ]
    );
    assert_eq!(serving.fixture.guest.standin.resumes.load(SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_resume_without_the_capability_is_refused_whatever_the_client_claims() {
    let serving = serving(true).await;

    for request in [
        json!({"op": "resume"}),
        json!({"op": "resume", "actor": "viewer"}),
        json!({"op": "resume", "actor": "person", "capability": "0".repeat(64)}),
        json!({"op": "resume", "capability": serving.capability.to_uppercase()}),
    ] {
        let answered = asked(&serving.socket, &request).await;

        assert_eq!(answered.len(), 1, "{request}");
        assert_eq!(answered[0]["ok"], false, "{request}");
        assert_eq!(answered[0]["code"], "denied", "{request}");
    }
    assert_eq!(serving.fixture.guest.standin.resumes.load(SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_read_is_carried_on_a_lane_of_the_live_generation() {
    let serving = serving(true).await;

    let answered = asked(
        &serving.socket,
        &json!({"op": "read", "request": {"op": "elements", "app": "Notes"}, "deadline_ms": 5000}),
    )
    .await;

    assert_eq!(
        answered,
        [json!({"ok": true, "echo": {"op": "elements", "app": "Notes"}})]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn anything_else_is_refused_with_what_the_keeper_serves() {
    let serving = serving(true).await;

    for request in [
        json!({"op": "run", "argv": ["/usr/bin/true"]}),
        json!({"op": "read"}),
        json!({}),
    ] {
        let answered = asked(&serving.socket, &request).await;

        assert_eq!(answered[0]["code"], "bad-request", "{request}");
        assert_eq!(
            answered[0]["capabilities"],
            json!(CAPABILITIES),
            "{request}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_client_not_admitted_is_answered_nothing_and_nothing_is_said() {
    let serving = serving(false).await;

    for request in [json!({"op": "hello"}), json!({"op": "stop"})] {
        assert!(
            asked(&serving.socket, &request).await.is_empty(),
            "{request}"
        );
    }
    assert_eq!(serving.fixture.guest.standin.stops.load(SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_resume_naming_no_stop_it_read_is_refused() {
    let serving = serving(true).await;

    let answered = asked(
        &serving.socket,
        &json!({"op": "resume", "capability": serving.capability}),
    )
    .await;

    assert_eq!(answered[0]["code"], "bad-request", "{answered:?}");
    assert_eq!(serving.fixture.guest.standin.resumes.load(SeqCst), 0);
}

/// The station's Stop as a Resume reads it before it asks.
fn read_stop(serving: &Serving) -> Value {
    json!(serving.fixture.keeper.stop.stop_revision().unwrap())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_resumed_guest_has_the_stop_lifted_by_the_keeper_and_a_new_generation_is_never_stopped_again()
 {
    let serving = serving(true).await;
    serving.fixture.keeper.stop.note_stop(true).unwrap();
    let revision = read_stop(&serving);

    let resumed = asked(
        &serving.socket,
        &json!({"op": "resume", "capability": serving.capability, "revision": revision}),
    )
    .await;

    assert_eq!(resumed[0]["outcome"], "resumed", "{resumed:?}");
    assert_eq!(resumed[0]["lifted"], "cleared", "{resumed:?}");
    assert!(!serving.fixture.keeper.stop.stop_noted());
    let stops = serving.fixture.guest.standin.stops.load(SeqCst);
    serving.fixture.guest.cut();
    let first = serving.fixture.keeper.shared.state().generation;
    until("a new generation", || {
        serving
            .fixture
            .keeper
            .shared
            .live()
            .is_some_and(|session| Some(session.control.generation()) != first)
    })
    .await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(serving.fixture.guest.standin.stops.load(SeqCst), stops);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_resume_of_a_stop_no_longer_the_one_kept_is_refused_before_the_guest_hears_it() {
    let serving = serving(true).await;
    serving.fixture.keeper.stop.note_stop(true).unwrap();
    let revision = read_stop(&serving);
    serving.fixture.keeper.stop.note_stop(true).unwrap();

    let resumed = asked(
        &serving.socket,
        &json!({"op": "resume", "capability": serving.capability, "revision": revision}),
    )
    .await;

    assert_eq!(resumed[0]["ok"], false, "{resumed:?}");
    assert_eq!(resumed[0]["code"], "stopped", "{resumed:?}");
    assert_eq!(serving.fixture.guest.standin.resumes.load(SeqCst), 0);
    assert!(serving.fixture.keeper.stop.stop_noted());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stop_given_while_a_resume_is_carried_out_is_said_again_in_the_same_generation() {
    let serving = serving(true).await;
    let keeper = &serving.fixture.keeper;
    keeper.stop.note_stop(true).unwrap();
    until("the Stop applied", || keeper.applied().revision.is_some()).await;
    let revision = read_stop(&serving);
    let generation = keeper.shared.state().generation;
    let stops = serving.fixture.guest.standin.stops.load(SeqCst);
    // Its caller dies before it reaches anyone: only the marker says it.
    let stop = keeper.stop.clone();
    serving.fixture.guest.standin.on_lift.set(move || {
        stop.note_stop(true).unwrap();
    });

    let resumed = asked(
        &serving.socket,
        &json!({"op": "resume", "capability": serving.capability, "revision": revision}),
    )
    .await;

    assert_eq!(resumed[0]["outcome"], "resumed", "{resumed:?}");
    assert_eq!(resumed[0]["lifted"], "stop-held", "{resumed:?}");
    until("the newer Stop said again", || {
        serving.fixture.guest.standin.stops.load(SeqCst) > stops
    })
    .await;
    assert_eq!(keeper.shared.state().generation, generation, "reconnected");
    assert!(keeper.stop.stop_noted());
}

/// A keeper trusted whoever listens, for a client of a test's keeper.
struct Trusting;

impl crate::link::Trust for Trusting {
    fn trusts(
        &self,
        _: &atlas_core::desktop::DesktopId,
        _: &Path,
        _: &std::os::unix::net::UnixStream,
    ) -> Result<()> {
        Ok(())
    }
}

/// A client of the VM `serving` keeps, reaching it through its socket, its
/// Stop the keeper's own.
fn client_of(serving: &Serving, directory: &Path) -> crate::client::Client {
    let mut client = crate::test_support::vm_client(directory, None);
    client.controller = serving.fixture.keeper.stop.clone();
    client.link = crate::link::Link::Keeper(crate::link::Keeper::at(
        Arc::new(client.selected.clone()),
        serving.fixture.keeper.vm.clone(),
        serving.socket.clone(),
        Duration::from_secs(10),
        Arc::new(Trusting),
    ));
    client
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_status_through_its_keeper_says_both_hellos_and_is_never_proven() {
    let serving = serving(true).await;
    *serving.fixture.guest.stop.lock().unwrap() = "stopped";
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path());

    let status = client.status().await;

    assert_eq!(status["ok"], true, "{status}");
    assert_eq!(status["proven"], false);
    assert_eq!(status["keeper"]["live"], true);
    assert_eq!(status["guest"]["stop"]["state"], "stopped");
    assert_eq!(status["station"]["stop"]["state"], "clear");
    let doctor = crate::capability::Doctor::vm(status, client.unreachable());
    assert!(!doctor.diagnosis.ready);
    let shown =
        crate::overview::overview(&doctor, false, crate::overview::Pending::default(), &[], 0)
            .value;
    assert_eq!(shown["mode"], "stopped");
    for refused in [
        client.identity().await.unwrap_err(),
        client.link.call(&json!({"op": "hello"})).await.unwrap_err(),
    ] {
        assert_eq!(refused.error, client.unreachable().error);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stop_on_a_vm_is_delivered_only_once_its_guest_applied_it() {
    let serving = serving(true).await;
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path());

    serving.fixture.guest.standin.unkept.store(1, SeqCst);
    let unkept = client.hold(true).await.unwrap();
    let applied = client.hold(true).await.unwrap();

    assert_eq!(unkept["kept"], true);
    assert_eq!(unkept["delivered"], false, "{unkept}");
    assert!(unkept["warnings"][0].is_string(), "{unkept}");
    assert_eq!(applied["delivered"], true, "{applied}");
    assert!(applied["seq"].is_u64(), "{applied}");
    assert_eq!(applied["outcome"], "stopped");
    assert!(client.controller.stop_noted());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_resume_on_a_vm_is_the_viewers_alone_with_the_capability_and_lifts_the_stop_it_read() {
    use crate::client::Actor;
    let serving = serving(true).await;
    let directory = tempfile::tempdir().unwrap();
    let mut client = client_of(&serving, directory.path());
    client.hold(true).await.unwrap();

    // The command line's own Resume, whoever it says it is.
    assert!(client.hold(false).await.is_err());
    assert!(client.resume_vm(&serving.capability).await.is_err());
    client.actor = Actor::Viewer;
    assert!(client.hold(false).await.is_err());
    let refused = client.resume_vm(&"0".repeat(64)).await.unwrap_err();
    assert!(
        refused.error.contains("Only the person"),
        "{}",
        refused.error
    );
    assert_eq!(serving.fixture.guest.standin.resumes.load(SeqCst), 0);
    assert!(client.controller.stop_noted());

    let resumed = client.resume_vm(&serving.capability).await.unwrap();

    assert_eq!(resumed["held"], false, "{resumed}");
    assert_eq!(resumed["lifted"], "cleared");
    assert!(!client.controller.stop_noted());
    assert_eq!(serving.fixture.guest.standin.resumes.load(SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_resume_answered_after_its_deadline_has_the_stop_said_again_in_the_same_generation() {
    let serving = serving(true).await;
    let keeper = &serving.fixture.keeper;
    keeper.stop.note_stop(true).unwrap();
    until("the Stop applied", || keeper.applied().revision.is_some()).await;
    let revision = read_stop(&serving);
    let generation = keeper.shared.state().generation;
    let stops = serving.fixture.guest.standin.stops.load(SeqCst);
    // The guest carries the Resume out after its caller stopped waiting.
    serving
        .fixture
        .guest
        .standin
        .on_lift
        .set(|| std::thread::sleep(Duration::from_millis(1500)));

    let resumed = asked(
        &serving.socket,
        &json!({"op": "resume", "capability": serving.capability, "revision": revision,
            "deadline_ms": 200}),
    )
    .await;

    assert_eq!(resumed[0]["outcome"], "unconfirmed", "{resumed:?}");
    assert!(resumed[0].get("lifted").is_none(), "{resumed:?}");
    until("the Stop said again", || {
        serving.fixture.guest.standin.stops.load(SeqCst) > stops
    })
    .await;
    assert_eq!(serving.fixture.guest.standin.resumes.load(SeqCst), 1);
    assert!(keeper.stop.stop_noted(), "the Stop it never lifted stays");
    assert_eq!(keeper.shared.state().generation, generation, "reconnected");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_that_never_acknowledges_is_stopped_again_after_a_resume_it_cannot_confirm() {
    let serving = serving_with(true, true).await;
    let keeper = &serving.fixture.keeper;
    keeper.stop.note_stop(true).unwrap();
    until("the unit Stop said", || {
        serving.fixture.guest.standin.stops.load(SeqCst) == 1
    })
    .await;
    let revision = read_stop(&serving);

    let resumed = asked(
        &serving.socket,
        &json!({"op": "resume", "capability": serving.capability, "revision": revision}),
    )
    .await;

    assert_eq!(resumed[0]["seq"], Value::Null, "{resumed:?}");
    assert_eq!(resumed[0]["outcome"], "unconfirmed", "{resumed:?}");
    until("stopped again", || {
        serving.fixture.guest.standin.stops.load(SeqCst) == 2
    })
    .await;
    assert!(keeper.stop.stop_noted());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_handover_keeps_a_stop_for_the_update_says_it_and_ends_the_generation_and_the_keeper() {
    let serving = serving(true).await;
    let keeper = serving.fixture.keeper.clone();
    let change = Token::new();
    // Work whose outcome is uncertain, as the station keeps it.
    let uncertain = br#"{"uncertain": true}"#;
    std::fs::write(keeper.stop.marker_path(), uncertain).unwrap();

    let handed = asked(
        &serving.socket,
        &json!({"op": "handover", "change": change}),
    )
    .await;

    let handed = &handed[0];
    assert_eq!(handed["ok"], true, "{handed}");
    assert_eq!(handed["op"], "handover");
    assert_eq!(handed["change"], json!(change));
    assert_eq!(handed["said"], true);
    assert_eq!(handed["seq"], 1);
    assert_eq!(handed["outcome"], "stopped");
    assert_eq!(handed["noted"], true);
    assert_eq!(handed["instance"], json!(keeper.instance));
    let kept = keeper.stop.stop_revision().unwrap().unwrap();
    assert_eq!(kept.why(), StopWhy::Update);
    tokio::time::timeout(Duration::from_secs(5), keeper.ended.notified())
        .await
        .expect("the keeper ends");
    assert!(keeper.shared.live().is_none());
    assert_eq!(keeper.hello()["live"], false);
    // It dials no more: the session is the next keeper's.
    let dials = serving.fixture.guest.controls().len();
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(serving.fixture.guest.controls().len(), dials);
    assert_eq!(
        std::fs::read(keeper.stop.marker_path()).unwrap(),
        uncertain,
        "kept uncertain"
    );
    assert_eq!(serving.fixture.guest.standin.resumes.load(SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_handover_naming_no_change_is_refused_and_nothing_is_kept_or_said() {
    let serving = serving(true).await;

    for request in [
        json!({"op": "handover"}),
        json!({"op": "handover", "change": "not-a-change"}),
    ] {
        let refused = asked(&serving.socket, &request).await;

        assert_eq!(refused[0]["code"], "bad-request", "{request}");
    }
    assert!(!serving.fixture.keeper.stop.stop_noted());
    assert_eq!(serving.fixture.guest.standin.stops.load(SeqCst), 0);
    assert!(serving.fixture.keeper.shared.live().is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stop_given_during_a_handover_holds_after_it_and_nothing_resumes_agents() {
    let serving = serving(true).await;
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path());
    let keeper = serving.fixture.keeper.clone();
    asked(
        &serving.socket,
        &json!({"op": "handover", "change": Token::new()}),
    )
    .await;

    // The person stops agents before the next keeper serves.
    let stopped = client.hold(true).await.unwrap();
    assert_eq!(stopped["kept"], true, "{stopped}");
    assert_eq!(stopped["delivered"], false);
    let given = keeper.stop.stop_revision().unwrap().unwrap();
    assert_eq!(given.why(), StopWhy::Person);
    let next = next(&keeper);
    let running = tokio::spawn({
        let next = next.clone();
        async move { next.run().await }
    });
    until("the next keeper says it", || {
        serving.fixture.guest.standin.stops.load(SeqCst) >= 2
    })
    .await;
    tokio::time::sleep(Duration::from_secs(3)).await;

    assert_eq!(keeper.stop.stop_revision().unwrap(), Some(given));
    assert_eq!(serving.fixture.guest.standin.resumes.load(SeqCst), 0);
    assert!(next.shared.live().is_some());
    running.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_status_says_its_agents_were_stopped_for_an_update() {
    let serving = serving(true).await;
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path());
    serving
        .fixture
        .keeper
        .stop
        .note_stop_unless_noted(StopWhy::Update)
        .unwrap();

    let status = client.status().await;

    assert_eq!(status["station"]["stop"]["state"], "stopped", "{status}");
    assert_eq!(status["station"]["stop"]["why"], "update");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_persons_stop_before_the_update_keeps_its_reason_and_revision_through_the_handover() {
    let serving = serving(true).await;
    let keeper = serving.fixture.keeper.clone();
    keeper.stop.note_stop(true).unwrap();
    let given = keeper.stop.stop_revision().unwrap();

    let handed = asked(
        &serving.socket,
        &json!({"op": "handover", "change": Token::new()}),
    )
    .await;

    assert_eq!(handed[0]["ok"], true, "{}", handed[0]);
    assert_eq!(handed[0]["noted"], false);
    assert_eq!(handed[0]["said"], true, "the Stop is said all the same");
    let kept = keeper.stop.stop_revision().unwrap();
    assert_eq!(kept, given);
    assert_eq!(kept.unwrap().why(), StopWhy::Person);
}
