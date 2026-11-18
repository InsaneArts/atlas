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
pub(in crate::vm::keeper) struct Serving {
    pub(in crate::vm::keeper) fixture: Fixture,
    pub(in crate::vm::keeper) socket: std::path::PathBuf,
    capability: String,
    _folder: tempfile::TempDir,
    _tasks: Vec<tokio::task::JoinHandle<()>>,
}

pub(in crate::vm::keeper) async fn serving(admitted: bool) -> Serving {
    serving_with(admitted, false).await
}

/// [`serving`], its guest an older one that acknowledges nothing when `plain`.
pub(in crate::vm::keeper) async fn serving_with(admitted: bool, plain: bool) -> Serving {
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
pub(in crate::vm::keeper) async fn asked(socket: &Path, request: &Value) -> Vec<Value> {
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
        &json!({"op": "read", "request": {"op": "outcome", "change": "ab"}, "deadline_ms": 5000}),
    )
    .await;

    assert_eq!(
        answered,
        [json!({"ok": true, "echo": {"op": "outcome", "change": "ab"}})]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_read_pinned_to_a_generation_is_carried_only_in_that_generation() {
    let serving = serving(true).await;
    let live = serving.fixture.keeper.shared.state().generation.unwrap().0;
    let read = |generation: Value| {
        json!({"op": "read", "request": {"op": "outcome"}, "generation": generation,
            "deadline_ms": 5000})
    };

    let carried = asked(&serving.socket, &read(json!(live))).await;
    let stale = asked(&serving.socket, &read(json!(live + 1))).await;
    let earlier = asked(&serving.socket, &read(json!(live - 1))).await;
    let malformed = asked(&serving.socket, &read(json!("live"))).await;

    assert_eq!(carried, [json!({"ok": true, "echo": {"op": "outcome"}})]);
    for refused in [&stale, &earlier] {
        assert_eq!(refused.len(), 1, "{refused:?}");
        assert_eq!(refused[0]["code"], "session-changed", "{refused:?}");
    }
    assert_eq!(malformed[0]["code"], "bad-request", "{malformed:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_read_pinned_to_a_generation_that_ended_is_refused_in_the_next() {
    let serving = serving(true).await;
    let keeper = &serving.fixture.keeper;
    let first = keeper.shared.state().generation.unwrap().0;
    serving.fixture.guest.cut();
    until("a new generation", || {
        keeper
            .shared
            .live()
            .is_some_and(|session| session.control.generation().0 != first)
    })
    .await;

    let refused = asked(
        &serving.socket,
        &json!({"op": "read", "request": {"op": "outcome"}, "generation": first}),
    )
    .await;

    assert_eq!(refused[0]["code"], "session-changed", "{refused:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_observation_is_asked_on_the_keepers_socket() {
    let serving = serving(true).await;
    serving.fixture.guest.proving();

    let observed = asked(
        &serving.socket,
        &json!({"op": "observe", "challenge": "on-the-socket", "deadline_ms": 5000}),
    )
    .await;

    assert_eq!(observed.len(), 1, "{observed:?}");
    assert_eq!(observed[0]["op"], "observe", "{observed:?}");
    assert_eq!(
        observed[0]["status"]["sessionProof"]["challenge"],
        "on-the-socket"
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
pub(in crate::vm::keeper) fn client_of(
    serving: &Serving,
    directory: &Path,
) -> crate::client::Client {
    client_within(serving, directory, Duration::from_secs(10))
}

/// [`client_of`], each exchange with the keeper within `timeout`.
fn client_within(serving: &Serving, directory: &Path, timeout: Duration) -> crate::client::Client {
    let mut client = crate::test_support::vm_client(directory, None);
    client.controller = serving.fixture.keeper.stop.clone();
    client.link = crate::link::Link::Keeper(
        crate::link::Keeper::at(
            Arc::new(client.selected.clone()),
            serving.fixture.keeper.vm.clone(),
            serving.socket.clone(),
            timeout,
            Arc::new(Trusting),
        )
        .with_pairing(serving.fixture.kept.clone()),
    );
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
    let unproven = client.identity().await.unwrap_err();
    assert_eq!(unproven.error, client.unreachable().error, "{unproven:?}");
    assert_eq!(
        client
            .link
            .call(&json!({"op": "apps"}))
            .await
            .unwrap_err()
            .error,
        client.unreachable().error,
        "a read the keeper does not carry"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_status_proven_through_its_keeper_with_everything_holding_is_ready() {
    let serving = serving(true).await;
    serving.fixture.guest.proving();
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path());
    let generation = serving.fixture.keeper.shared.state().generation.unwrap();

    let status = client.status().await;

    assert_eq!(status["ok"], true, "{status}");
    assert_eq!(status["proven"], true, "{status}");
    assert_eq!(status["ready"], true, "{status}");
    assert!(status.get("notReady").is_none(), "{status}");
    assert_eq!(status["station"]["unresolved"], json!([]), "{status}");
    let proof = &status["proof"];
    assert_eq!(proof["generation"], json!(generation));
    assert_eq!(proof["capture"], true);
    assert_eq!(proof["input"], true, "{status}");
    assert_eq!(proof["epoch"]["capturePid"], 123);
    assert_eq!(proof["epoch"]["inputPid"], 456);
    assert_eq!(proof["epoch"]["scale"], 2.0);
    assert_eq!(status["guest"]["stop"]["state"], "clear");
    assert_eq!(status["runner"]["window"], "closed");
    let doctor = crate::capability::Doctor::vm(status.clone(), client.unreachable());
    assert!(doctor.diagnosis.ready);
    assert_eq!(doctor.vm, Some(crate::capability::VmMode::Ready));
    assert!(crate::capability::vm_first_step(&status).is_none());
    assert_eq!(
        client.identity().await.unwrap_err().error,
        client.unreachable().error,
        "only a command this build runs on a VM proves it"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stalled_input_lane_leaves_a_vm_proven_with_input_unavailable_in_time() {
    let serving = serving(true).await;
    let saying = serving.fixture.guest.proving();
    saying.input_stalls.store(true, SeqCst);
    let directory = tempfile::tempdir().unwrap();
    let timeout = Duration::from_secs(3);
    let client = client_within(&serving, directory.path(), timeout);
    let started = std::time::Instant::now();

    let status = client.status().await;

    assert!(
        started.elapsed() < timeout,
        "{:?} for the hello and the observation",
        started.elapsed()
    );
    assert_eq!(status["ok"], true, "{status}");
    assert_eq!(status["proven"], true, "{status}");
    assert_eq!(status["proof"]["input"], false);
    assert_eq!(
        status["guest"]["inputService"]["available"], false,
        "{status}"
    );
    assert_eq!(
        status["runner"]["window"], "closed",
        "the runner still asked"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_overview_of_a_vm_says_its_proofs_first_step_after_its_stop_and_handover() {
    let serving = serving(true).await;
    serving.fixture.guest.proving();
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path());
    let reason = |status: Value, handover: Option<String>| {
        let doctor =
            crate::capability::Doctor::vm(status, client.unreachable()).handing_over(handover);
        crate::overview::overview(&doctor, false, crate::overview::Pending::default(), &[], 0).value
    };
    let window = |said: Value| {
        *serving.fixture.guest.runner.lock().unwrap() = json!({"ok": true, "op": "state", "power": "running", "confirmed": true,
                "window": said});
    };

    window(json!("open"));
    let open = reason(client.status().await, None);
    window(json!(null));
    let unknown = client.status().await;
    let handing = reason(
        unknown.clone(),
        Some("Atlas is handing the VM over.".to_owned()),
    );
    let unknown = reason(unknown, None);
    window(json!("closed"));
    *serving.fixture.guest.stop.lock().unwrap() = "stopped";
    let stopped = reason(client.status().await, None);

    assert_eq!(open["mode"], "unavailable");
    assert!(
        open["reason"].as_str().unwrap().contains("Close it"),
        "{open}"
    );
    assert!(
        unknown["reason"]
            .as_str()
            .unwrap()
            .contains("Restart the VM"),
        "{unknown}"
    );
    assert_eq!(handing["reason"], "Atlas is handing the VM over.");
    assert_eq!(stopped["mode"], "stopped", "{stopped}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_statuses_in_a_row_prove_one_epoch_each_with_a_challenge_of_its_own() {
    let serving = serving(true).await;
    let saying = serving.fixture.guest.proving();
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path());

    let first = client.status().await;
    let second = client.status().await;

    assert_eq!(first["proven"], true, "{first}");
    assert_eq!(second["proven"], true, "{second}");
    assert_eq!(first["proof"]["epoch"], second["proof"]["epoch"]);
    let challenges = saying.challenges.lock().unwrap().clone();
    assert_eq!(challenges.len(), 2, "{challenges:?}");
    assert_ne!(challenges[0], challenges[1]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_proof_that_answers_another_challenge_is_refused_and_doctor_says_so_first() {
    let serving = serving(true).await;
    let saying = serving.fixture.guest.proving();
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path());
    let earlier = client.status().await;
    assert_eq!(earlier["proven"], true, "{earlier}");
    let replayed = saying.challenges.lock().unwrap()[0].clone();
    *saying.replaying.lock().unwrap() = Some(replayed);

    let status = client.status().await;

    assert_eq!(status["ok"], true, "{status}");
    assert_eq!(status["proven"], false, "{status}");
    assert_eq!(status["ready"], false);
    assert_eq!(status["proof"]["code"], "identity", "{status}");
    let refused = status["proof"]["error"].as_str().unwrap().to_owned();
    assert!(refused.contains("did not match its challenge"), "{refused}");
    assert_eq!(status["guest"]["stop"]["state"], "clear", "still said");
    let first = crate::capability::vm_first_step(&status).unwrap();
    assert_eq!(first.error, refused);
    let doctor = crate::capability::Doctor::vm(status, client.unreachable());
    assert!(matches!(
        doctor.diagnosis.first_step,
        atlas_core::session::Guidance::Advice(ref step) if *step == refused
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vm_with_no_input_hello_is_proven_with_input_false_and_its_advice_first() {
    let serving = serving(true).await;
    let saying = serving.fixture.guest.proving();
    *saying.input.lock().unwrap() =
        Some(json!({"ok": false, "code": "transport", "error": "The lane ended."}));
    *saying.hello.lock().unwrap() = Some(|hello| {
        hello["permissions"]["accessibility"] = json!(false);
        hello["input"] = json!({"standing": "gone",
            "failure": {"code": "permission", "error": "No Accessibility."},
            "held": "Allow Atlas in Privacy & Security, Accessibility, in the VM's window: agents wait for it."});
    });
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path());

    let status = client.status().await;

    assert_eq!(status["proven"], true, "{status}");
    assert_eq!(status["proof"]["capture"], true);
    assert_eq!(status["proof"]["input"], false);
    assert_eq!(status["proof"]["epoch"]["inputPid"], 0);
    assert_eq!(status["guest"]["inputService"]["available"], false);
    let first = crate::capability::vm_first_step(&status).unwrap();
    assert!(
        first
            .error
            .starts_with("Allow Atlas in Privacy & Security, Accessibility"),
        "{}",
        first.error
    );
    assert_eq!(
        first.details["next"],
        "Ask the person to allow Atlas in Privacy & Security, Accessibility, in work's window."
    );
    let shown = serde_json::to_string(&first).unwrap();
    assert!(!shown.contains("helper"), "{shown}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vm_whose_input_runs_another_build_of_the_same_version_is_said_to_be_mid_update() {
    let serving = serving(true).await;
    let saying = serving.fixture.guest.proving();
    *saying.hello.lock().unwrap() = Some(|hello| hello["daemon"]["build"] = json!("b2"));
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path());

    let status = client.status().await;

    assert_eq!(status["proven"], true, "{status}");
    assert_eq!(
        status["guest"]["daemon"]["version"],
        status["guest"]["inputService"]["version"]
    );
    assert_eq!(status["proof"]["input"], false, "{status}");
    assert_eq!(status["proof"]["inputWhy"], crate::vm::proof::MID_UPDATE);
    assert_eq!(status["notReady"], crate::vm::proof::MID_UPDATE);
    let first = crate::capability::vm_first_step(&status).unwrap();
    assert_eq!(first.code, Code::Update);
    assert!(first.error.contains("Restart the VM"), "{}", first.error);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vm_open_to_more_than_atlas_is_refused_in_its_guests_words() {
    let serving = serving(true).await;
    let saying = serving.fixture.guest.proving();
    *saying.hello.lock().unwrap() = Some(|hello| {
        hello["remoteLogin"] = json!("on");
        hello["agents"]["held"] = json!("Remote Login is on in the guest.");
    });
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path());

    let status = client.status().await;

    assert_eq!(status["proven"], false, "{status}");
    assert_eq!(status["proof"]["code"], "platform", "{status}");
    assert!(
        status["proof"]["error"]
            .as_str()
            .unwrap()
            .starts_with("Remote Login is on in the guest.")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vm_whose_runner_says_nothing_of_its_window_is_refused_with_a_restart_advised() {
    let serving = serving(true).await;
    serving.fixture.guest.proving();
    *serving.fixture.guest.runner.lock().unwrap() =
        json!({"ok": true, "op": "state", "power": "running", "confirmed": true});
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path());

    let status = client.status().await;

    assert_eq!(status["proven"], false, "{status}");
    assert_eq!(status["proof"]["code"], "on-console", "{status}");
    assert!(
        status["proof"]["error"]
            .as_str()
            .unwrap()
            .contains("Restart the VM")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vm_whose_keeper_cannot_observe_it_says_why_beside_its_guests_hello() {
    let serving = serving_with(true, true).await;
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path());

    let status = client.status().await;

    assert_eq!(status["ok"], true, "{status}");
    assert_eq!(status["proven"], false);
    assert_eq!(status["proof"]["code"], "update", "{status}");
    assert_eq!(status["guest"]["stop"]["state"], "clear", "{status}");
    assert!(status["runner"].is_null());
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

/// Keep `client`'s VM's lifecycle record in `state`, in its test lifecycle.
fn lifecycle_record(client: &crate::client::Client, state: crate::vm::lifecycle::record::State) {
    use crate::vm::lifecycle::{Provenance, Reservation, record::Record};
    let lifecycle = client.lifecycle.as_ref().unwrap();
    let reservation = Reservation {
        name: "work".to_owned(),
        id: client.desktop().id(),
        named: true,
        resumed: false,
    };
    let mut record = Record::adding(
        &reservation,
        "/Users/someone/.config/atlas.toml".into(),
        Provenance {
            program: "/Applications/Atlas.app/Contents/MacOS/atlas".into(),
            requirement: "identifier \"com.insanearts.atlas\"".to_owned(),
        },
    );
    record.state = state;
    lifecycle.keep_record(&record).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vm_being_added_refuses_every_command_but_the_persons_stop_until_the_same_client_sees_it_added()
 {
    use crate::{client::Actor, vm::lifecycle::record::State};
    let serving = serving(true).await;
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path());
    lifecycle_record(&client, State::Adding);

    let status = client.status().await;
    let mut viewer = client.clone();
    viewer.actor = Actor::Viewer;
    let refused = [
        client.reachable().unwrap_err(),
        client.link.call(&json!({"op": "hello"})).await.unwrap_err(),
        viewer.resume_vm(&serving.capability).await.unwrap_err(),
    ];
    let stopped = client.hold(true).await.unwrap();
    lifecycle_record(&client, State::Created);
    let added = client.status().await;

    assert_eq!(status["ok"], false, "{status}");
    assert_eq!(status["error"], "work is still being added.");
    for refused in refused {
        assert_eq!(refused.error, "work is still being added.");
    }
    assert_eq!(stopped["kept"], true, "{stopped}");
    assert_eq!(stopped["delivered"], true, "{stopped}");
    assert_eq!(added["ok"], true, "the same client sees it added: {added}");
    assert!(
        client.controller.stop_noted(),
        "the Stop holds once it is added"
    );
    assert!(client.controller.stop_revision().unwrap().is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vm_whose_lifecycle_record_cannot_be_read_is_refused_as_uncertain_and_still_stopped() {
    let serving = serving(true).await;
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path());
    lifecycle_record(&client, crate::vm::lifecycle::record::State::Adding);
    let path = client
        .lifecycle
        .as_ref()
        .unwrap()
        .record_at(&client.desktop().id());
    std::fs::write(&path, br#"{"id":"0123456789abcdef01234567","schema":9}"#).unwrap();

    let status = client.status().await;
    let refused = client.reachable().unwrap_err();
    let stopped = client.hold(true).await.unwrap();

    assert_eq!(status["ok"], false, "{status}");
    for error in [status["error"].as_str().unwrap(), refused.error.as_str()] {
        assert!(error.contains("cannot be told"), "{error}");
    }
    assert_eq!(stopped["delivered"], true, "{stopped}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lifecycle_record_out_of_reach_for_a_while_is_read_again_by_the_same_client_unchanged() {
    use std::os::unix::fs::PermissionsExt;
    let serving = serving(true).await;
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path());
    lifecycle_record(&client, crate::vm::lifecycle::record::State::Created);
    assert_eq!(client.status().await["ok"], true);
    let folder = client
        .lifecycle
        .as_ref()
        .unwrap()
        .record_at(&client.desktop().id())
        .parent()
        .unwrap()
        .to_owned();

    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o600)).unwrap();
    let out_of_reach = client.status().await;
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
    let back = client.status().await;

    assert_eq!(out_of_reach["ok"], false, "{out_of_reach}");
    assert!(
        out_of_reach["error"]
            .as_str()
            .unwrap()
            .contains("cannot be told")
    );
    assert_eq!(back["ok"], true, "{back}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vm_without_screen_recording_asks_for_it_in_its_own_window_never_the_helpers() {
    let serving = serving(true).await;
    let saying = serving.fixture.guest.proving();
    *saying.hello.lock().unwrap() = Some(|hello| {
        hello["permissions"]["screenCapture"] = json!(false);
    });
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path());

    let status = client.status().await;

    assert_eq!(status["proven"], true, "{status}");
    let first = crate::capability::vm_first_step(&status).unwrap();
    assert_eq!(first.code, Code::Permission);
    assert_eq!(
        first.details["next"],
        "Ask the person to allow Atlas in Privacy & Security, Screen & System Audio Recording, \
         in work's window."
    );
    let shown = serde_json::to_string(&first).unwrap();
    assert!(!shown.contains("helper"), "{shown}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_ready_vm_is_ready_to_doctor_and_the_overview_and_nothing_unchecked_is_said_to_pass() {
    let serving = serving(true).await;
    serving.fixture.guest.proving();
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path());

    let status = client.status().await;
    let report = crate::capability::vm_report(status.clone(), client.unreachable());
    let reading = crate::overview::read(&client).await;

    assert_eq!(status["ready"], true, "{status}");
    assert_eq!(report["ready"], true, "{report}");
    for check in report["checks"].as_array().unwrap() {
        assert_eq!(check["ok"], true, "{report}");
    }
    for unchecked in ["nativeDialogs", "installedApps", "maintenance"] {
        assert_eq!(
            report["capabilities"][unchecked]["state"], "notChecked",
            "{unchecked}: {report}"
        );
        assert!(report["capabilities"][unchecked]["reason"].is_string());
    }
    assert!(!report["capabilities"].to_string().contains("passed"));
    let summary = crate::capability::summary(report);
    assert_eq!(summary["ready"], true, "{summary}");
    assert!(
        summary["warnings"][0]
            .as_str()
            .unwrap()
            .starts_with("Native Save/Open is not checked on work"),
        "{summary}"
    );
    assert!(reading.ready, "{}", reading.value);
    let overview = &reading.value["overview"];
    assert_eq!(overview["mode"], "observing", "{overview}");
    assert_eq!(overview["state"], "idle");
    assert_eq!(overview["reason"], "Nothing is happening in work");
    assert_eq!(overview["nativeDialogs"], "notChecked");
    assert!(
        !overview.to_string().to_lowercase().contains("helper"),
        "{overview}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_ready_vm_with_a_change_atlas_vm_left_unanswered_is_not_ready_in_words_naming_it() {
    let serving = serving(true).await;
    serving.fixture.guest.proving();
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path());
    let start = atlas_core::files::Token::new();
    // A start of the VM whose command ended before Atlas VM answered it.
    drop(
        client
            .controller
            .quarantine_by(start.as_str(), atlas_control::executors::Executor::Vm)
            .unwrap(),
    );
    let words = "Atlas VM has not said yet what a change it was making to work came to, so \
                 agents wait until it does.";

    let status = client.status().await;
    let report = crate::capability::vm_report(status.clone(), client.unreachable());
    let reading = crate::overview::read(&client).await;

    assert_eq!(status["proven"], true, "{status}");
    assert_eq!(status["ready"], false, "{status}");
    assert_eq!(status["notReady"], words);
    assert_eq!(
        status["station"]["unresolved"],
        json!([{"generation": start.as_str(), "executor": "vm", "holder": "gone"}])
    );
    assert_eq!(report["ready"], false);
    assert_eq!(report["nextSteps"], json!([words]), "{report}");
    assert!(!reading.ready);
    assert_eq!(reading.value["overview"]["mode"], "unavailable");
    let reason = reading.value["overview"]["reason"].as_str().unwrap();
    assert!(reason.starts_with(words), "{reason}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_command_an_agent_started_that_still_runs_keeps_a_vm_ready_until_it_is_left_unanswered() {
    let serving = serving(true).await;
    serving.fixture.guest.proving();
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path());
    let command = atlas_core::files::Token::new();
    let running = client
        .controller
        .quarantine_command_by(command.as_str(), atlas_control::executors::Executor::Guest)
        .unwrap();

    let beside = client.status().await;
    client.controller.abandon_command(running);
    let left = client.status().await;

    assert_eq!(beside["ready"], true, "{beside}");
    assert_eq!(beside["station"]["pending"], json!([command.as_str()]));
    assert_eq!(beside["station"]["unresolved"], json!([]));
    assert_eq!(left["ready"], false, "{left}");
    assert_eq!(
        left["notReady"],
        "An action on work may not have finished; Atlas settles it on its own once work \
         answers. If it stays, run atlas doctor --desktop work."
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fresh_vm_is_not_ready_with_screen_recording_first_in_its_guests_words() {
    let serving = serving(true).await;
    let saying = serving.fixture.guest.proving();
    *saying.hello.lock().unwrap() = Some(|hello| {
        hello["permissions"]["screenCapture"] = json!(false);
        hello["permissions"]["accessibility"] = json!(false);
        hello["capture"] = json!({"held": "Allow Atlas in Privacy & Security, Screen & System \
            Audio Recording, in the VM's window: agents' pictures wait for it."});
        hello["input"]["held"] = json!(
            "Allow Atlas in Privacy & Security, Accessibility, in the VM's window: agents wait \
             for it."
        );
    });
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path());

    let status = client.status().await;
    let report = crate::capability::vm_report(status.clone(), client.unreachable());
    let summary = crate::capability::summary(report.clone());

    assert_eq!(status["proven"], true, "{status}");
    assert_eq!(status["ready"], false);
    assert_eq!(
        status["notReady"],
        "Allow Atlas in Privacy & Security, Screen & System Audio Recording, in the VM's window: \
         agents' pictures wait for it."
    );
    assert_eq!(report["nextSteps"][0], status["notReady"], "{report}");
    assert_eq!(report["canObserve"], false);
    for unchecked in ["nativeDialogs", "installedApps", "maintenance"] {
        assert_eq!(
            summary["capabilities"][unchecked]["state"], "notChecked",
            "{summary}"
        );
    }
    assert!(
        summary.get("guest").is_none(),
        "the status stays verbose: {summary}"
    );
    assert!(summary.get("helper").is_none(), "{summary}");
    assert!(
        !summary.to_string().to_lowercase().contains("helper"),
        "{summary}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_change_another_command_is_making_leaves_a_vm_ready_until_it_ends_without_an_answer() {
    let serving = serving(true).await;
    serving.fixture.guest.proving();
    let directory = tempfile::tempdir().unwrap();
    let client = client_of(&serving, directory.path());
    let change = atlas_core::files::Token::new();
    let making = client
        .controller
        .quarantine_by(change.as_str(), atlas_control::executors::Executor::Guest)
        .unwrap();

    let during = client.status().await;
    let report = crate::capability::vm_report(during.clone(), client.unreachable());
    let reading = crate::overview::read(&client).await;
    // Its command ends without settling it, as one killed would.
    drop(making);
    let after = client.status().await;

    assert_eq!(during["ready"], true, "{during}");
    assert_eq!(
        during["station"]["unresolved"],
        json!([{"generation": change.as_str(), "executor": "guest", "holder": "live"}])
    );
    assert!(
        report["warnings"]
            .as_array()
            .unwrap()
            .contains(&json!("A change to work is still being made.")),
        "{report}"
    );
    assert!(reading.ready);
    assert_eq!(
        reading.value["overview"]["mode"], "agent",
        "{}",
        reading.value
    );
    assert_eq!(
        reading.value["overview"]["reason"],
        "An agent is acting in work"
    );
    assert_eq!(after["ready"], false, "{after}");
    assert_eq!(
        after["notReady"],
        "An action on work may not have finished; Atlas settles it on its own once work \
         answers. If it stays, run atlas doctor --desktop work."
    );
}

/// The claim file of the change `generation`, found under `folder`.
fn claim_under(folder: &Path, generation: &str) -> std::path::PathBuf {
    let suffix = format!(".{generation}.live");
    let mut folders = vec![folder.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else if path.to_string_lossy().ends_with(&suffix) {
                return path;
            }
        }
    }
    panic!("no claim of {generation} under {}", folder.display());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_change_whose_command_ended_beside_a_record_that_cannot_be_read_keeps_a_vm_not_ready() {
    let serving = serving(true).await;
    serving.fixture.guest.proving();
    let directory = tempfile::tempdir().unwrap();
    let mut client = client_of(&serving, directory.path());
    // The station's records in a folder of the test's own, to find the claim in.
    client.controller =
        atlas_control::Controller::of_at(&client.desktop(), &directory.path().join("station"));
    let change = atlas_core::files::Token::new();
    let making = client
        .controller
        .quarantine_by(change.as_str(), atlas_control::executors::Executor::Guest)
        .unwrap();
    let claim = claim_under(directory.path(), change.as_str());
    std::fs::write(claim.with_extension("executor.live"), b"{").unwrap();
    // Its command ends: the claim is kept, as what holds it cannot be told.
    drop(making);

    let status = client.status().await;

    assert!(claim.exists(), "recovery would still keep it");
    assert_eq!(
        status["station"]["unresolved"],
        json!([{"generation": change.as_str(), "executor": "guest", "holder": "unknown"}])
    );
    assert_eq!(status["ready"], false, "{status}");
    assert_eq!(
        status["notReady"],
        "Whether a change to work is still being made cannot be told, so agents wait."
    );
}

/// An observation in the generation after the keeper's hello, whose guest
/// is of a build that serves no pictures.
fn reconnected_older(answer: &mut Value) {
    answer["generation"] = json!(answer["generation"].as_u64().unwrap() + 1);
    let older: Vec<&str> = atlas_session::compatibility::MACHINE
        .into_iter()
        .filter(|name| *name != atlas_session::compatibility::PICTURES)
        .collect();
    answer["declared"] = json!({"protocol": 1, "capabilities": older, "version": "0.9.0"});
}

/// Doctor's check that the VM's Atlas serves this build, of `status`.
fn served(status: &Value, client: &crate::client::Client) -> Value {
    crate::capability::vm_report(status.clone(), client.unreachable())["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["name"] == "Atlas in it serves this build")
        .unwrap()
        .clone()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vm_that_reconnected_to_an_older_build_between_its_hello_and_proof_is_never_ready() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = crate::test_support::vm_through_keeper(dir.path());

    // Proved in the next generation, while the hello read first said
    // everything.
    *keeper.observing.lock().unwrap() = Some(reconnected_older);
    let older = client.status().await;
    // The same, its proof refused there: its window is open.
    *keeper.observing.lock().unwrap() = Some(|answer| {
        reconnected_older(answer);
        answer["runner"]["window"] = json!("open");
    });
    let refused = client.status().await;
    // An observation that does not say what its generation declared.
    *keeper.observing.lock().unwrap() = Some(|answer| {
        reconnected_older(answer);
        answer.as_object_mut().unwrap().remove("declared");
    });
    let unsaid = client.status().await;
    *keeper.observing.lock().unwrap() = Some(|answer| {
        reconnected_older(answer);
        answer.as_object_mut().unwrap().remove("declared");
        answer["runner"]["window"] = json!("open");
    });
    let unsaid_refused = client.status().await;

    assert_eq!(older["proven"], true, "{older}");
    assert_eq!(older["ready"], false, "{older}");
    assert!(
        older["notReady"]
            .as_str()
            .unwrap()
            .contains("(pictures-v1)"),
        "{older}"
    );
    assert_eq!(refused["proven"], false, "{refused}");
    let check = served(&refused, &client);
    assert_eq!(check["ok"], false, "the observation's own: {check}");
    assert!(
        check["nextStep"]
            .as_str()
            .unwrap()
            .contains("(pictures-v1)"),
        "{check}"
    );
    assert_eq!(unsaid["ready"], false, "{unsaid}");
    assert_eq!(
        unsaid["notReady"],
        "work's session keeper is from an older build; run atlas update."
    );
    assert_eq!(
        served(&unsaid, &client)["ok"],
        Value::Null,
        "never the hello's"
    );
    assert_eq!(unsaid_refused["proven"], false, "{unsaid_refused}");
    assert_eq!(served(&unsaid_refused, &client)["ok"], Value::Null);
}
