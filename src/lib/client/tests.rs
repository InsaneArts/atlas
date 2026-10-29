//! The client's tests: captures and their crops, input and its checks,
//! stops, commands run beside each other, restarts and log-outs, and the
//! activity rows they write.
use super::*;
use super::{
    apps::app_list,
    capture::{Cropped, capture_name, cropped, meets, placement, screenshot_request},
    input::expectation,
    record::{PERSON_INPUT_SPAN, PersonInput, worked},
    service::process_exists,
    session::gone_within,
};
use crate::{
    actions::{self, Expect},
    activity_log::{self, Failure, Op},
    control::{Ownership, SharedLease},
    daemon::protocol::{Crop, Region},
    digest::file_sha256,
    error::Holder,
    interrupt::Interrupt,
    session::{Display, Epoch, FOCUS_APP, FOCUS_DOCK, FOCUS_FIELD, FOCUS_WINDOW, HOLD, SHUTDOWN},
    test_support::client_in,
};
use fs2::FileExt;
use nix::sys::signal::Signal;
use std::{fs::OpenOptions, os::unix::fs::OpenOptionsExt, time::Instant};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
};

#[test]
fn pruning_removes_only_old_capture_files() {
    let dir = tempfile::tempdir().unwrap();
    let old = std::time::SystemTime::now() - Duration::from_secs(8 * 24 * 60 * 60);
    let kept = [
        "notes-capture.png",
        "-capture.png",
        "340282366920938463463374607431768211456-capture.png",
        ".1-capture.png.partial",
    ];
    for name in kept.into_iter().chain(["1-capture.png"]) {
        let file = fs::File::create(dir.path().join(name)).unwrap();
        file.set_times(fs::FileTimes::new().set_modified(old))
            .unwrap();
    }
    fs::write(dir.path().join("2-capture.png"), b"recent").unwrap();
    std::os::unix::fs::symlink("notes-capture.png", dir.path().join("3-capture.png")).unwrap();
    fs::create_dir(dir.path().join("4-capture.png")).unwrap();
    fs::File::open(dir.path().join("4-capture.png"))
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(old))
        .unwrap();

    prune_captures(dir.path());

    assert!(!dir.path().join("1-capture.png").exists());
    for name in kept
        .into_iter()
        .chain(["2-capture.png", "3-capture.png", "4-capture.png"])
    {
        assert!(
            fs::symlink_metadata(dir.path().join(name)).is_ok(),
            "{name}"
        );
    }
}

/// A native socket and controller confined to a short temporary path.
fn native_client() -> (tempfile::TempDir, Client, UnixListener, Session) {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let mut client = client_in(dir.path());
    client.controller = Controller::at(60001, &dir.path().join("state"));
    client.config.helper_uid = nix::unistd::getuid().as_raw();
    client.transport = client.input_transport();
    let listener = UnixListener::bind(&client.config.input_socket).unwrap();
    let session = Session {
        epoch: Epoch {
            uid: client.config.helper_uid,
            capture_pid: 123,
            display: Display {
                width: 100.0,
                height: 100.0,
                scale: 1.0,
            },
            security_session_id: 42,
            display_id: 7,
            input_pid: 456,
        },
        capture: true,
        input: true,
    };
    (dir, client, listener, session)
}

/// Drive the caller until the stand-in has received a complete request,
/// leaving its reply under the test's control.
async fn dispatched<T: std::fmt::Debug>(
    caller: &mut (impl Future<Output = Result<T>> + Unpin),
    listener: &UnixListener,
    operation: &str,
) -> BufReader<UnixStream> {
    received(caller, listener, operation).await.0
}

/// [`dispatched`], with the request the stand-in received.
async fn received<T: std::fmt::Debug>(
    caller: &mut (impl Future<Output = Result<T>> + Unpin),
    listener: &UnixListener,
    operation: &str,
) -> (BufReader<UnixStream>, Value) {
    tokio::select! {
        outcome = caller => panic!("caller ended before {operation}: {outcome:?}"),
        peer = tokio::time::timeout(Duration::from_secs(2), async {
            let (peer, _) = listener.accept().await.unwrap();
            let mut peer = BufReader::new(peer);
            let mut line = String::new();
            peer.read_line(&mut line).await.unwrap();
            let request = serde_json::from_str::<Value>(&line).unwrap();
            assert_eq!(request["op"], operation);
            (peer, request)
        }) => peer.unwrap(),
    }
}

async fn answer(peer: &mut BufReader<UnixStream>, reply: Value) {
    peer.get_mut()
        .write_all(format!("{reply}\n").as_bytes())
        .await
        .unwrap();
}

/// Exercise the transaction after session verification, with no native
/// events: the stand-in acknowledges a press and move without a release.
async fn standalone_input(client: &Client, session: &Session) -> Result<Value> {
    let _ownership = client.controller.acquire(false)?;
    let actions = json!([
        {"type": "pointer", "phase": "down", "x": 1, "y": 1},
        {"type": "pointer", "phase": "move", "x": 2, "y": 2},
    ]);
    actions::validate(&actions, &session.epoch.display)?;
    client
        .input_in_session(&actions, None, None, None, session)
        .await
}

/// One key press, checked against `expect` when given, after session
/// verification.
async fn checked_input(
    client: &Client,
    session: &Session,
    expect: Option<Expect>,
) -> Result<Value> {
    let _ownership = client.controller.acquire(false)?;
    let actions = json!([{"type": "key", "key": "a"}]);
    client
        .input_in_session(&actions, None, None, expect, session)
        .await
}

fn textedit() -> Option<Expect> {
    Some(Expect::new(812).unwrap())
}

#[tokio::test]
async fn the_expectation_is_sent_only_when_asked() {
    for (expect, sent) in [(None, Value::Null), (textedit(), json!({"pid": 812}))] {
        let (_dir, client, listener, session) = native_client();
        let mut caller = Box::pin(checked_input(&client, &session, expect));
        let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
        answer(&mut acquire, json!({"ok": true})).await;
        let (mut input, request) = received(&mut caller, &listener, "input").await;
        assert_eq!(request.get("expect").cloned().unwrap_or(Value::Null), sent);
        let mut reply = json!({"ok": true, "performed": 1});
        reply["expect"] = sent;
        answer(&mut input, reply).await;
        let mut release = dispatched(&mut caller, &listener, "release").await;
        answer(&mut release, json!({"ok": true})).await;
        assert_eq!(caller.await.unwrap()["ok"], true);
        assert!(client.controller.pending().unwrap().is_empty());
    }
}

#[tokio::test]
async fn a_checked_reply_without_its_echo_is_uncertain() {
    let (_dir, client, listener, session) = native_client();
    let mut caller = Box::pin(checked_input(&client, &session, textedit()));
    let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
    answer(&mut acquire, json!({"ok": true})).await;
    let mut input = dispatched(&mut caller, &listener, "input").await;
    // A service that ignored the expectation posted the key unchecked.
    answer(&mut input, json!({"ok": true, "performed": 1})).await;
    let mut release = dispatched(&mut caller, &listener, "release").await;
    answer(&mut release, json!({"ok": true})).await;
    let error = caller.await.unwrap_err();
    assert_eq!(error.code, "protocol");
    assert!(error.uncertain);
    recovery_settles_the_abandoned_claim(&client);
}

#[tokio::test]
async fn a_focus_refusal_releases_and_leaves_no_marker() {
    let (_dir, client, listener, session) = native_client();
    let mut caller = Box::pin(checked_input(&client, &session, textedit()));
    let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
    answer(&mut acquire, json!({"ok": true})).await;
    let mut input = dispatched(&mut caller, &listener, "input").await;
    let refusal = json!({"ok": false, "code": "focus-changed", "reason": "app",
        "found": {"app": "Calculator", "pid": 900}, "error": "Calculator has keyboard focus",
        "performed": 0, "partial": false, "uncertain": false});
    answer(&mut input, refusal.clone()).await;
    let mut release = dispatched(&mut caller, &listener, "release").await;
    answer(&mut release, json!({"ok": true})).await;
    let reply = caller.await.unwrap();
    for field in ["ok", "code", "reason", "found", "performed", "uncertain"] {
        assert_eq!(reply[field], refusal[field], "{field}");
    }
    assert!(client.controller.pending().unwrap().is_empty());
    assert!(client.controller.acquire(false).is_ok());
}

/// A verified status of helper account 7001 whose input service lists
/// `capabilities`: enough for input to go ahead without asking again.
fn verified_status(capabilities: &[&str]) -> Value {
    let proof = json!({"ok": true, "uid": 7001, "hasGraphicAccess": true, "parentPID": 123,
        "onConsole": false, "securitySessionID": 42, "displayID": 7, "width": 100,
        "height": 100});
    json!({
        "ok": true,
        "user": {"uid": 7001},
        "daemon": {"pid": 123},
        "session": {"managerName": "Aqua", "onConsole": false},
        "display": {"width": 100, "height": 100, "scale": 1},
        "permissions": {"screenCapture": true, "accessibility": true},
        "sessionProof": proof,
        "inputService": {"ok": true, "protocolVersion": 1, "pid": 456, "ready": true,
            "accessibility": true, "session": proof, "capabilities": capabilities},
    })
}

/// Send one key under `check` given `status`, and whether the input
/// service's socket was connected to at all.
///
/// The helper account must differ from the one running the test, so
/// the stand-in socket (served by the test's own account) fails the
/// peer check of any connection: a connection is the evidence that
/// something would have been sent, and none is the evidence that
/// nothing was.
async fn connected(status: &Value, check: Option<Check>) -> (Result<Value>, bool) {
    let (_dir, mut client, listener, _) = native_client();
    client.config.helper_uid = 7001;
    let actions = json!([{"type": "key", "key": "a"}]);
    let _ownership = client.controller.acquire(false).unwrap();
    let (outcome, accepted) = tokio::join!(
        client.input_with_status(status, &actions, None, None, check),
        tokio::time::timeout(Duration::from_millis(500), listener.accept()),
    );
    assert!(
        client.controller.pending().unwrap().is_empty(),
        "no marker is left"
    );
    (outcome, accepted.is_ok())
}

#[tokio::test]
async fn an_older_input_service_is_refused_before_anything_is_sent() {
    let older = verified_status(&[SHUTDOWN, HOLD]);
    let (outcome, sent) = connected(&older, textedit().map(Check::Required)).await;
    let refused = outcome.unwrap_err();
    assert_eq!(refused.code, "startup");
    assert!(!refused.uncertain);
    assert!(
        refused.error.contains("atlas start-input"),
        "{}",
        refused.error
    );
    assert!(!sent, "neither acquire nor input reached the service");

    // The same status goes on to the service when nothing needs the
    // check, or when the service can make it.
    let current = verified_status(&[SHUTDOWN, HOLD, FOCUS_APP, FOCUS_DOCK]);
    for (status, check) in [
        (&older, None),
        (&older, textedit().map(Check::Offered)),
        (&current, textedit().map(Check::Required)),
    ] {
        let (_, sent) = connected(status, check.clone()).await;
        assert!(sent, "{check:?}");
    }
}

/// A service from before the Dock counted as covering checks the app
/// but lets a click on a Dock icon through: it is given no input that
/// needs the check.
#[tokio::test]
async fn an_input_service_without_the_dock_check_receives_no_checked_input() {
    let undocked = verified_status(&[SHUTDOWN, HOLD, FOCUS_APP]);
    let (outcome, sent) = connected(&undocked, textedit().map(Check::Required)).await;
    let refused = outcome.unwrap_err();
    assert_eq!(refused.code, "startup");
    assert!(!refused.uncertain);
    assert!(refused.error.contains("older Atlas"), "{}", refused.error);
    assert!(
        refused.error.contains("atlas start-input"),
        "{}",
        refused.error
    );
    assert!(!sent, "neither acquire nor input reached the service");

    // A named click is judged against the Dock before it is sent, and
    // input that needs no check goes on as before.
    for check in [None, textedit().map(Check::Offered)] {
        let (_, sent) = connected(&undocked, check.clone()).await;
        assert!(sent, "{check:?}");
    }
}

#[test]
fn a_named_click_is_checked_only_by_a_service_that_can() {
    let older = json!({"ok": true, "capabilities": [SHUTDOWN, HOLD]});
    let current = json!({"ok": true, "capabilities": [SHUTDOWN, HOLD, FOCUS_APP, FOCUS_DOCK]});
    let undocked = json!({"ok": true, "capabilities": [SHUTDOWN, HOLD, FOCUS_APP]});
    assert_eq!(
        expectation(&undocked, Some(Check::Required(textedit().unwrap())))
            .unwrap_err()
            .code,
        "startup"
    );
    let unanswered = json!({"ok": false, "code": "transport"});
    let pid = textedit().unwrap();

    assert_eq!(
        expectation(&older, Some(Check::Offered(pid.clone()))).unwrap(),
        None
    );
    assert_eq!(
        expectation(&unanswered, Some(Check::Offered(pid.clone()))).unwrap(),
        None
    );
    assert_eq!(
        expectation(&current, Some(Check::Offered(pid.clone()))).unwrap(),
        Some(pid.clone())
    );

    assert_eq!(
        expectation(&older, Some(Check::Required(pid.clone())))
            .unwrap_err()
            .code,
        "startup"
    );
    assert_eq!(
        expectation(&current, Some(Check::Required(pid.clone()))).unwrap(),
        Some(pid.clone())
    );
    // A service that did not answer is refused as unavailable later.
    assert_eq!(
        expectation(&unanswered, Some(Check::Required(pid.clone()))).unwrap(),
        Some(pid.clone())
    );
    assert_eq!(expectation(&older, None).unwrap(), None);
}

#[tokio::test]
async fn a_focus_refusal_keeps_its_code_on_every_native_path() {
    let (_dir, client, listener, _) = native_client();
    let mut caller = Box::pin(client.native_control("input", "owner"));
    let mut request = dispatched(&mut caller, &listener, "input").await;
    answer(
        &mut request,
        json!({"ok": false, "code": "focus-changed", "error": "Calculator has keyboard focus"}),
    )
    .await;
    let error = caller.await.unwrap_err();
    assert_eq!(error.code, "focus-changed");
    assert!(!error.uncertain);
}

#[test]
fn a_checked_input_is_confirmed_only_with_its_expectation_echoed() {
    let unchecked = json!({"op": "input", "actions": [{}, {}]});
    let mut checked = unchecked.clone();
    checked["expect"] = json!({"pid": 812});

    for (request, reply, confirmed) in [
        (&unchecked, json!({"ok": true, "performed": 2}), true),
        (&unchecked, json!({"ok": true, "performed": 1}), false),
        (
            &checked,
            json!({"ok": true, "performed": 2, "expect": {"pid": 812}}),
            true,
        ),
        (&checked, json!({"ok": true, "performed": 2}), false),
        (
            &checked,
            json!({"ok": true, "performed": 2, "expect": {"pid": 9}}),
            false,
        ),
        (
            &checked,
            json!({"ok": false, "code": "focus-changed"}),
            true,
        ),
    ] {
        let outcome = validate_completion(request, &reply);
        assert_eq!(outcome.is_ok(), confirmed, "{request} {reply}");
        if let Err(error) = outcome {
            assert_eq!(error.code, "protocol");
            assert!(error.uncertain);
        }
    }
}

/// `acquire` once the lock is free: a child another test is starting
/// may hold the lock's file a moment past its release, until it runs
/// its own program. Any other refusal is the answer.
fn once_free(acquire: impl Fn() -> Result<Ownership>) -> Result<Ownership> {
    let held = |error: &Error| {
        error.code == Code::Busy
            && error.holder == Some(crate::error::Holder::Agent)
            && !error.details.contains_key("running")
    };
    for _ in 0..50 {
        match acquire() {
            Err(error) if held(&error) => std::thread::sleep(Duration::from_millis(20)),
            taken => return taken,
        }
    }
    acquire()
}

fn recovery_settles_the_abandoned_claim(client: &Client) {
    assert_eq!(client.controller.pending().unwrap().len(), 1);
    assert_eq!(client.controller.unresolved().unwrap().len(), 1);
    assert_eq!(
        once_free(|| client.controller.acquire(false))
            .unwrap_err()
            .code,
        "uncertain"
    );
    let ownership = once_free(|| client.controller.acquire(true)).unwrap();
    client.controller.clear().unwrap();
    drop(ownership);
    assert!(client.controller.pending().unwrap().is_empty());
    assert!(once_free(|| client.controller.acquire(false)).is_ok());
}

#[tokio::test]
async fn a_release_interrupted_after_dispatch_stays_pending_until_recovery() {
    let (_dir, client, listener, session) = native_client();
    let mut caller = Box::pin(standalone_input(&client, &session));
    let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
    answer(&mut acquire, json!({"ok": true})).await;
    let mut input = dispatched(&mut caller, &listener, "input").await;
    let generation = client.controller.pending().unwrap();
    assert_eq!(generation.len(), 1);
    answer(&mut input, json!({"ok": true, "performed": 2})).await;
    let _release = dispatched(&mut caller, &listener, "release").await;
    assert_eq!(client.controller.pending().unwrap(), generation);
    assert_eq!(client.controller.acquire(true).unwrap_err().code, "busy");
    drop(caller);
    recovery_settles_the_abandoned_claim(&client);
}

#[tokio::test]
async fn a_successful_release_settles_the_standalone_input_claim() {
    let (_dir, client, listener, session) = native_client();
    let mut caller = Box::pin(standalone_input(&client, &session));
    let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
    answer(&mut acquire, json!({"ok": true})).await;
    let mut input = dispatched(&mut caller, &listener, "input").await;
    let generation = client.controller.pending().unwrap();
    assert_eq!(generation.len(), 1);
    answer(&mut input, json!({"ok": true, "performed": 2})).await;
    let mut release = dispatched(&mut caller, &listener, "release").await;
    assert_eq!(client.controller.pending().unwrap(), generation);
    assert_eq!(client.controller.acquire(true).unwrap_err().code, "busy");
    answer(&mut release, json!({"ok": true})).await;
    assert_eq!(caller.await.unwrap()["ok"], true);
    assert!(client.controller.pending().unwrap().is_empty());
    assert!(client.controller.acquire(false).is_ok());
}

#[tokio::test]
async fn standalone_input_with_an_uncertain_outcome_stays_pending_until_recovery() {
    // Recovery takes the controller lock, which a spawn on another test
    // thread can hold in its child until exec.
    if !crate::test_support::isolated(
        "client::tests::standalone_input_with_an_uncertain_outcome_stays_pending_until_recovery",
    ) {
        return;
    }
    for (input_reply, release_reply) in [
        (json!({"ok": true, "performed": 2}), None),
        (
            json!({"ok": false, "uncertain": true, "error": "unknown"}),
            Some(json!({"ok": true})),
        ),
        (
            json!({"ok": true, "performed": 2}),
            Some(json!({"ok": false, "uncertain": true, "error": "unknown"})),
        ),
    ] {
        let (_dir, client, listener, session) = native_client();
        let mut caller = Box::pin(standalone_input(&client, &session));
        let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
        answer(&mut acquire, json!({"ok": true})).await;
        let mut input = dispatched(&mut caller, &listener, "input").await;
        answer(&mut input, input_reply).await;
        let mut release = dispatched(&mut caller, &listener, "release").await;
        assert_eq!(client.controller.pending().unwrap().len(), 1);
        if let Some(reply) = release_reply {
            answer(&mut release, reply).await;
        }
        drop(release);
        let outcome = caller.await;
        assert!(
            outcome
                .as_ref()
                .map_or_else(|error| error.uncertain, |reply| reply["uncertain"] == true)
        );
        recovery_settles_the_abandoned_claim(&client);
    }
}

/// A grace the tests wait out.
const GRACE: Duration = Duration::from_millis(500);

/// Well within [`GRACE`].
const SOON: Duration = Duration::from_millis(50);

/// `client`'s controller stopped by a stop of its own, which the test
/// asks.
fn stoppable(client: &mut Client) -> Interrupt {
    let interrupt = Interrupt::new(GRACE);
    client.controller = client.controller.clone().interrupted_by(interrupt.clone());
    interrupt
}

/// `client` with a controller of its own on the same state, as another
/// agent's command has.
fn another(client: &Client, dir: &Path) -> Client {
    let mut other = client.clone();
    other.controller = Controller::at(60001, &dir.join("state"));
    other
}

#[tokio::test]
async fn a_stop_before_native_acquisition_starts_nothing() {
    let (_dir, mut client, listener, session) = native_client();
    let interrupt = stoppable(&mut client);
    let _ownership = client.controller.acquire(false).unwrap();
    interrupt.ask(Signal::SIGTERM);

    let actions = json!([{"type": "key", "key": "a"}]);
    let error = client
        .input_in_session(&actions, None, None, None, &session)
        .await
        .unwrap_err();
    assert_eq!(error.code, "interrupted");
    assert!(!error.uncertain);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err(),
        "the input service heard nothing"
    );
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn a_stop_while_acquisition_is_unanswered_waits_for_it_then_posts_nothing() {
    let (_dir, mut client, listener, session) = native_client();
    let interrupt = stoppable(&mut client);
    let mut caller = Box::pin(standalone_input(&client, &session));
    let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
    interrupt.ask(Signal::SIGTERM);
    answer(&mut acquire, json!({"ok": true})).await;

    let mut release = dispatched(&mut caller, &listener, "release").await;
    answer(&mut release, json!({"ok": true})).await;
    let error = caller.await.unwrap_err();
    assert_eq!(error.code, "interrupted");
    assert!(!error.uncertain, "nothing was posted");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn a_stop_while_input_is_under_way_lets_the_batch_finish_and_settles_it() {
    let (_dir, mut client, listener, session) = native_client();
    let interrupt = stoppable(&mut client);
    let mut caller = Box::pin(standalone_input(&client, &session));
    let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
    answer(&mut acquire, json!({"ok": true})).await;
    let mut input = dispatched(&mut caller, &listener, "input").await;
    interrupt.ask(Signal::SIGTERM);
    answer(&mut input, json!({"ok": true, "performed": 2})).await;

    let mut release = dispatched(&mut caller, &listener, "release").await;
    answer(&mut release, json!({"ok": true})).await;
    assert_eq!(caller.await.unwrap()["ok"], true);
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn input_still_unanswered_after_the_grace_stays_pending_for_every_agent() {
    // Recovery takes the controller lock, which a spawn on another test
    // thread can hold in its child until exec.
    if !crate::test_support::isolated(
        "client::tests::input_still_unanswered_after_the_grace_stays_pending_for_every_agent",
    ) {
        return;
    }
    let (dir, mut client, listener, session) = native_client();
    let interrupt = stoppable(&mut client);
    let mut caller = Box::pin(standalone_input(&client, &session));
    let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
    answer(&mut acquire, json!({"ok": true})).await;
    let mut input = dispatched(&mut caller, &listener, "input").await;
    interrupt.ask(Signal::SIGTERM);

    // Past the grace, its release is still sent whole, but not waited
    // for.
    let error = tokio::time::timeout(GRACE * 4, caller)
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, "interrupted");
    assert!(error.uncertain);
    let mut line = String::new();
    assert_eq!(
        input.read_line(&mut line).await.unwrap(),
        0,
        "the input service sees the batch cut off"
    );
    let (release, _) = listener.accept().await.unwrap();
    BufReader::new(release).read_line(&mut line).await.unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&line).unwrap()["op"],
        "release"
    );
    recovery_settles_the_abandoned_claim(&another(&client, dir.path()));
}

#[tokio::test]
async fn a_stop_while_release_is_unanswered_waits_for_it() {
    let (_dir, mut client, listener, session) = native_client();
    let interrupt = stoppable(&mut client);
    let mut caller = Box::pin(standalone_input(&client, &session));
    let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
    answer(&mut acquire, json!({"ok": true})).await;
    let mut input = dispatched(&mut caller, &listener, "input").await;
    answer(&mut input, json!({"ok": true, "performed": 2})).await;
    let mut release = dispatched(&mut caller, &listener, "release").await;
    interrupt.ask(Signal::SIGTERM);
    tokio::time::sleep(SOON).await;

    answer(&mut release, json!({"ok": true})).await;
    assert_eq!(caller.await.unwrap()["ok"], true);
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn a_release_still_unanswered_after_the_grace_stays_pending() {
    if !crate::test_support::isolated(
        "client::tests::a_release_still_unanswered_after_the_grace_stays_pending",
    ) {
        return;
    }
    let (dir, mut client, listener, session) = native_client();
    let interrupt = stoppable(&mut client);
    let mut caller = Box::pin(standalone_input(&client, &session));
    let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
    answer(&mut acquire, json!({"ok": true})).await;
    let mut input = dispatched(&mut caller, &listener, "input").await;
    answer(&mut input, json!({"ok": true, "performed": 2})).await;
    let _release = dispatched(&mut caller, &listener, "release").await;
    interrupt.ask(Signal::SIGTERM);
    let asked = Instant::now();

    let error = tokio::time::timeout(GRACE * 4, caller)
        .await
        .unwrap()
        .unwrap_err();
    assert!(asked.elapsed() >= GRACE, "cut at the end of the grace");
    assert_eq!(error.code, "interrupted");
    assert!(error.uncertain);
    recovery_settles_the_abandoned_claim(&another(&client, dir.path()));
}

#[tokio::test]
async fn an_acquisition_still_unanswered_after_the_grace_stays_pending() {
    if !crate::test_support::isolated(
        "client::tests::an_acquisition_still_unanswered_after_the_grace_stays_pending",
    ) {
        return;
    }
    let (dir, mut client, listener, session) = native_client();
    let interrupt = stoppable(&mut client);
    let mut caller = Box::pin(standalone_input(&client, &session));
    let _acquire = dispatched(&mut caller, &listener, "acquire").await;
    interrupt.ask(Signal::SIGTERM);

    // The release that follows an unknown acquisition is cut at once,
    // so it stays unknown too.
    let error = tokio::time::timeout(GRACE * 4, caller)
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, "interrupted");
    assert!(error.uncertain);
    let other = another(&client, dir.path());
    assert!(!other.controller.pending().unwrap().is_empty());
    assert_eq!(
        other.controller.acquire(false).unwrap_err().code,
        "uncertain"
    );
    let recovery = other.controller.acquire(true).unwrap();
    other.controller.clear().unwrap();
    drop(recovery);
    assert!(other.controller.acquire(false).is_ok());
}

#[tokio::test]
async fn a_stop_while_a_request_waits_to_be_sent_sends_nothing_and_leaves_nothing() {
    let (dir, client, input, _session) = native_client();
    drop(input);
    let (acting, helper, interrupt) = running(dir.path(), client);
    let lease: SharedLease = Arc::new(tokio::sync::Mutex::new(crate::control::Lease::new(
        Duration::from_secs(10),
    )));
    let token = lease.lock().await.acquire(&acting.controller).unwrap();
    let authorizing = lease.lock().await;
    let run = json!({"op": "run", "argv": ["true"], "timeoutMs": 1000});
    let mut caller = Box::pin(acting.mutate(run, Some((&lease, &token)), None, None));
    assert!(
        tokio::time::timeout(SOON, &mut caller).await.is_err(),
        "it waits for the lease"
    );
    interrupt.ask(Signal::SIGTERM);
    drop(authorizing);

    let error = caller.await.unwrap_err();
    assert_eq!(error.code, "interrupted");
    assert!(!error.uncertain, "nothing was sent");
    let (peer, _) = helper.accept().await.unwrap();
    let mut line = String::new();
    assert_eq!(
        BufReader::new(peer).read_line(&mut line).await.unwrap(),
        0,
        "the helper got no request"
    );
    assert!(acting.controller.pending().unwrap().is_empty());
}

/// An agent's `run` against a helper stand-in kept in `dir`, its
/// listener, and the stop its controller heeds.
fn running(dir: &Path, mut client: Client) -> (Client, UnixListener, Interrupt) {
    client.transport.path = dir.join("helper.sock");
    let helper = UnixListener::bind(&client.transport.path).unwrap();
    let interrupt = stoppable(&mut client);
    (client.doing(Doing::new("run")), helper, interrupt)
}

#[tokio::test]
async fn a_run_that_ends_within_the_grace_is_settled() {
    let (dir, client, input, _session) = native_client();
    drop(input);
    let (acting, helper, interrupt) = running(dir.path(), client);
    let _ownership = acting.controller.acquire(false).unwrap();
    let run = json!({"op": "run", "argv": ["true"], "timeoutMs": 1000});
    let mut caller = Box::pin(acting.mutate(run, None, None, None));
    let mut peer = dispatched(&mut caller, &helper, "run").await;
    interrupt.ask(Signal::SIGTERM);
    tokio::time::sleep(SOON).await;

    answer(
        &mut peer,
        json!({"ok": true, "exitCode": 0, "timedOut": false}),
    )
    .await;
    assert_eq!(caller.await.unwrap()["exitCode"], 0);
    assert!(acting.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn a_run_still_going_after_the_grace_is_cut_and_stays_pending() {
    if !crate::test_support::isolated(
        "client::tests::a_run_still_going_after_the_grace_is_cut_and_stays_pending",
    ) {
        return;
    }
    let (dir, client, input, _session) = native_client();
    drop(input);
    let (acting, helper, interrupt) = running(dir.path(), client);
    let ownership = acting.controller.acquire(false).unwrap();
    let run = json!({"op": "run", "argv": ["sleep", "30"], "timeoutMs": 60_000});
    let mut caller = Box::pin(acting.mutate(run, None, None, None));
    let mut peer = dispatched(&mut caller, &helper, "run").await;
    interrupt.ask(Signal::SIGTERM);

    let error = tokio::time::timeout(GRACE * 5, caller)
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, "interrupted");
    assert!(error.uncertain);
    let mut line = String::new();
    assert_eq!(
        peer.read_line(&mut line).await.unwrap(),
        0,
        "the helper sees the connection close, which ends the command"
    );
    let log = fs::read_to_string(acting.output_dir().unwrap().join(activity_log::FILE)).unwrap();
    let ended: Value = serde_json::from_str(log.lines().last().unwrap()).unwrap();
    assert_eq!(ended["ok"], false);
    assert_eq!(ended["code"], "uncertain", "never recorded as not done");
    drop(ownership);
    recovery_settles_the_abandoned_claim(&another(&acting, dir.path()));
}

#[tokio::test]
async fn a_restart_stopped_before_its_request_settles_only_its_own_claim() {
    let (_dir, mut client, _input, _session) = native_client();
    let interrupt = stoppable(&mut client);
    let earlier = client.controller.quarantine().unwrap();
    let unknown = vec![earlier.generation().to_owned()];
    drop(earlier);
    let claim = client.controller.quarantine().unwrap();
    interrupt.ask(Signal::SIGTERM);

    let outcome = client
        .mutate(json!({"op": "restart"}), None, None, None)
        .await;
    let error = client.settle_restart(&claim, outcome).unwrap_err();
    assert_eq!(error.code, "interrupted");
    assert!(!error.uncertain, "the restart was never sent");
    drop(claim);
    assert_eq!(client.controller.pending().unwrap(), unknown);
}

#[tokio::test]
async fn a_grant_another_command_left_is_waited_out_within_the_wait() {
    let (_dir, mut client, listener, session) = native_client();
    client.controller = client.controller.waiting(Duration::from_secs(2));
    let mut caller = Box::pin(standalone_input(&client, &session));
    for _ in 0..2 {
        let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
        answer(
            &mut acquire,
            json!({"ok": false, "code": "busy", "holder": "agent", "error": "held"}),
        )
        .await;
    }
    let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
    answer(&mut acquire, json!({"ok": true})).await;
    let mut input = dispatched(&mut caller, &listener, "input").await;
    answer(&mut input, json!({"ok": true, "performed": 2})).await;
    let mut release = dispatched(&mut caller, &listener, "release").await;
    answer(&mut release, json!({"ok": true})).await;
    assert_eq!(caller.await.unwrap()["ok"], true);
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn a_grant_the_person_holds_is_never_waited_for() {
    let (_dir, mut client, listener, session) = native_client();
    client.controller = client.controller.waiting(Duration::from_secs(2));
    let mut caller = Box::pin(standalone_input(&client, &session));
    let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
    answer(
        &mut acquire,
        json!({"ok": false, "code": "busy", "holder": "person", "error": "held"}),
    )
    .await;
    let error = tokio::time::timeout(Duration::from_secs(1), caller)
        .await
        .expect("answered after one try")
        .unwrap_err();
    assert_eq!(error.code, "busy");
    assert_eq!(error.holder, Some(Holder::Person));
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn a_controller_that_does_not_wait_tries_the_grant_once() {
    for wait in [None, Some(Duration::from_millis(1))] {
        let (_dir, mut client, listener, session) = native_client();
        if let Some(wait) = wait {
            // A wait already spent on the lock leaves none for the grant.
            client.controller = client.controller.waiting(wait);
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let mut caller = Box::pin(standalone_input(&client, &session));
        let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
        answer(
            &mut acquire,
            json!({"ok": false, "code": "busy", "holder": "agent", "error": "held"}),
        )
        .await;
        let error = tokio::time::timeout(Duration::from_secs(1), caller)
            .await
            .expect("answered after one try")
            .unwrap_err();
        assert_eq!(error.code, "busy");
        assert_eq!(error.holder, Some(Holder::Agent));
        assert!(client.controller.pending().unwrap().is_empty());
    }
}

#[tokio::test]
async fn a_grant_refusal_that_may_have_happened_or_names_no_agent_is_never_retried() {
    for reply in [
        json!({"ok": false, "code": "busy", "holder": "agent", "uncertain": true, "error": "?"}),
        json!({"ok": false, "code": "busy", "error": "held"}),
        json!({"ok": false, "code": "lease-lost", "error": "lost"}),
    ] {
        let (_dir, mut client, listener, session) = native_client();
        client.controller = client.controller.waiting(Duration::from_secs(2));
        let mut caller = Box::pin(standalone_input(&client, &session));
        let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
        answer(&mut acquire, reply.clone()).await;
        if reply["uncertain"] == true {
            // The grant may be held: it is released, not tried again.
            let mut release = dispatched(&mut caller, &listener, "release").await;
            answer(&mut release, json!({"ok": true})).await;
        }
        let error = tokio::time::timeout(Duration::from_secs(1), caller)
            .await
            .expect("answered after one try")
            .unwrap_err();
        assert_eq!(error.code.as_str(), reply["code"], "{reply}");
    }
}

#[tokio::test]
async fn a_saved_stop_ends_the_wait_for_a_grant_before_another_try() {
    let (_dir, mut client, listener, session) = native_client();
    client.controller = client.controller.waiting(Duration::from_secs(2));
    let mut caller = Box::pin(standalone_input(&client, &session));
    let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
    answer(
        &mut acquire,
        json!({"ok": false, "code": "busy", "holder": "agent", "error": "held"}),
    )
    .await;
    // The person stops agents; the service has not heard of it.
    client.controller.note_stop(true).unwrap();
    let error = tokio::time::timeout(Duration::from_secs(1), caller)
        .await
        .expect("no other try")
        .unwrap_err();
    assert_eq!(error.code, "stopped");
    assert!(!error.uncertain);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err(),
        "no acquire or input after the Stop"
    );
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn an_input_whose_native_release_is_definitely_refused_stays_pending_until_recovery() {
    let (_dir, client, listener, session) = native_client();
    let mut caller = Box::pin(standalone_input(&client, &session));
    let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
    answer(&mut acquire, json!({"ok": true})).await;
    let mut input = dispatched(&mut caller, &listener, "input").await;
    answer(&mut input, json!({"ok": true, "performed": 2})).await;
    let mut release = dispatched(&mut caller, &listener, "release").await;
    let generation = client.controller.pending().unwrap();
    assert_eq!(generation.len(), 1);
    answer(
        &mut release,
        json!({"ok": false, "code": "lease-lost", "error": "refused"}),
    )
    .await;
    let error = caller.await.unwrap_err();
    assert_eq!(error.code, "uncertain");
    assert!(error.uncertain);
    assert_eq!(client.controller.pending().unwrap(), generation);
    recovery_settles_the_abandoned_claim(&client);
}

#[tokio::test]
async fn releasing_after_a_lost_acquisition_reply_keeps_the_acquisition_pending() {
    let (_dir, client, listener, session) = native_client();
    let mut caller = Box::pin(standalone_input(&client, &session));
    let acquire = dispatched(&mut caller, &listener, "acquire").await;
    drop(acquire);
    let mut release = dispatched(&mut caller, &listener, "release").await;
    assert_eq!(client.controller.pending().unwrap().len(), 2);
    assert_eq!(client.controller.unresolved().unwrap().len(), 1);
    assert_eq!(client.controller.acquire(true).unwrap_err().code, "busy");
    answer(&mut release, json!({"ok": true})).await;
    assert!(caller.await.unwrap_err().uncertain);
    recovery_settles_the_abandoned_claim(&client);
}

#[tokio::test]
async fn an_acquisition_interrupted_after_dispatch_stays_pending_until_recovery() {
    let (_dir, client, listener, session) = native_client();
    let mut caller = Box::pin(client.acquire_input("owner", &session.epoch));
    let _acquire = dispatched(&mut caller, &listener, "acquire").await;
    assert_eq!(client.controller.pending().unwrap().len(), 1);
    drop(caller);
    recovery_settles_the_abandoned_claim(&client);
}

#[tokio::test]
async fn an_acquisition_settles_only_an_acknowledged_outcome() {
    for reply in [
        json!({"ok": true}),
        json!({"ok": false, "code": "busy", "error": "refused"}),
        json!({"ok": false, "uncertain": true, "error": "unknown"}),
    ] {
        let (_dir, client, listener, session) = native_client();
        let mut caller = Box::pin(client.acquire_input("owner", &session.epoch));
        let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
        assert_eq!(client.controller.pending().unwrap().len(), 1);
        answer(&mut acquire, reply.clone()).await;
        assert_eq!(caller.await.is_ok(), reply["ok"] == true);
        if reply["uncertain"] == true {
            recovery_settles_the_abandoned_claim(&client);
        } else {
            assert!(client.controller.pending().unwrap().is_empty());
            assert!(client.controller.acquire(false).is_ok());
        }
    }
}

#[tokio::test]
async fn a_busy_input_service_says_who_holds_control() {
    for (holder, expected) in [
        (json!("person"), Some(Holder::Person)),
        (json!("agent"), Some(Holder::Agent)),
        (json!("someone"), None),
        (Value::Null, None),
    ] {
        let (_dir, client, listener, session) = native_client();
        let mut caller = Box::pin(client.acquire_input("owner", &session.epoch));
        let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
        let reply = json!({"ok": false, "code": "busy", "error": "refused", "holder": holder});
        answer(&mut acquire, reply).await;
        let error = caller.await.unwrap_err();
        assert_eq!(error.code, "busy");
        assert_eq!(error.holder, expected, "{holder}");
    }
}

/// A status reply sufficient for repinning preflight. It deliberately
/// lacks the independent proof required by later session verification.
fn repinnable_status(session: &Session) -> Value {
    json!({"ok": true, "inputService": {
        "ok": true, "protocolVersion": 1, "accessibility": true,
        "enforcedEpoch": session.epoch.input_epoch(),
    }})
}

#[tokio::test]
async fn missing_input_with_permissions_granted_reports_a_startup_failure() {
    let (_dir, client, listener, session) = native_client();
    let status = json!({
        "ok": true,
        "permissions": {"screenCapture": true, "accessibility": true},
        "inputService": {"ok": false, "accessibility": true},
    });
    let mut caller = Box::pin(client.repin(session));
    let mut hello = dispatched(&mut caller, &listener, "hello").await;
    answer(&mut hello, status).await;
    let error = caller.await.unwrap_err();
    assert_eq!(error.code, "startup");
    assert!(!error.uncertain);
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn repinning_with_accessibility_revoked_reports_a_permission_failure() {
    for source in ["permissions", "inputService"] {
        let (_dir, client, listener, session) = native_client();
        let mut status = repinnable_status(&session);
        status[source]["accessibility"] = json!(false);
        let mut caller = Box::pin(client.repin(session));
        let mut hello = dispatched(&mut caller, &listener, "hello").await;
        answer(&mut hello, status).await;
        assert_eq!(caller.await.unwrap_err().code, "permission", "{source}");
        assert!(client.controller.pending().unwrap().is_empty());
    }
}

#[tokio::test]
async fn a_display_reset_interrupted_after_dispatch_stays_pending_until_recovery() {
    let (_dir, client, listener, session) = native_client();
    let status = repinnable_status(&session);
    let mut caller = Box::pin(client.repin(session));
    let mut hello = dispatched(&mut caller, &listener, "hello").await;
    answer(&mut hello, status).await;
    let _reset = dispatched(&mut caller, &listener, "reset").await;
    assert_eq!(client.controller.pending().unwrap().len(), 1);
    drop(caller);
    recovery_settles_the_abandoned_claim(&client);
}

#[tokio::test]
async fn an_acknowledged_display_reset_is_settled_before_session_verification() {
    let (_dir, client, listener, session) = native_client();
    let status = repinnable_status(&session);
    let mut caller = Box::pin(client.repin(session));
    let mut hello = dispatched(&mut caller, &listener, "hello").await;
    answer(&mut hello, status.clone()).await;
    let mut reset = dispatched(&mut caller, &listener, "reset").await;
    assert_eq!(client.controller.pending().unwrap().len(), 1);
    answer(&mut reset, json!({"ok": true, "released": true})).await;
    let mut verifying = dispatched(&mut caller, &listener, "hello").await;
    assert!(client.controller.pending().unwrap().is_empty());
    answer(&mut verifying, status).await;
    assert_eq!(caller.await.unwrap_err().code, "identity");
    assert!(client.controller.pending().unwrap().is_empty());
    assert!(client.controller.acquire(false).is_ok());
}

#[test]
fn an_interrupted_setup_can_repair_its_probe_without_settling_pending_work() {
    if !crate::test_support::isolated(
        "client::tests::an_interrupted_setup_can_repair_its_probe_without_settling_pending_work",
    ) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut client = client_in(dir.path());
    client.controller = Controller::at(60001, &dir.path().join("state"));
    let previous = client.controller.quarantine().unwrap();
    let generation = previous.generation().to_owned();
    drop(previous);
    assert!(!client.config.identity_probe.exists());
    assert!(client.install().is_err());
    let competing = client.controller.acquire(true).unwrap();
    assert!(client.bootstrap_probe().is_err());
    assert!(!client.config.identity_probe.exists());
    drop(competing);
    client.bootstrap_probe().unwrap();
    assert!(client.config.identity_probe.is_file());
    assert_eq!(
        file_sha256(&client.config.identity_probe).unwrap(),
        file_sha256(&std::env::current_exe().unwrap()).unwrap()
    );
    assert_eq!(
        client.controller.pending().unwrap(),
        std::slice::from_ref(&generation)
    );
    assert!(client.controller.acquire(false).is_err());
    fs::write(&client.config.identity_probe, b"interrupted copy").unwrap();
    client.bootstrap_probe().unwrap();
    assert_eq!(
        file_sha256(&client.config.identity_probe).unwrap(),
        file_sha256(&std::env::current_exe().unwrap()).unwrap()
    );
    assert_eq!(client.controller.pending().unwrap(), [generation]);
    let owned = client.controller.acquire(true).unwrap();
    client.controller.clear().unwrap();
    drop(owned);
    assert!(client.controller.acquire(false).is_ok());
}

#[test]
fn a_successful_recovery_allows_the_next_normal_acquisition() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = client_in(dir.path());
    client.controller = Controller::at(60001, &dir.path().join("state"));
    let ownership = client.controller.acquire(true).unwrap();
    drop(client.controller.quarantine().unwrap());
    let claim = client.controller.quarantine().unwrap();
    // The verified recovery clears abandoned work while retaining its
    // own live claim, which the restart success path must then settle.
    client.controller.clear().unwrap();
    client
        .settle_restart(&claim, Ok(json!({"ok": true, "restarted": true})))
        .unwrap();
    drop(claim);
    drop(ownership);
    assert!(client.controller.pending().unwrap().is_empty());
    assert!(client.controller.acquire(false).is_ok());

    let other = client.controller.quarantine().unwrap();
    let claim = client.controller.quarantine().unwrap();
    client.controller.clear().unwrap();
    client
        .settle_restart(&claim, Ok(json!({"ok": true, "restarted": true})))
        .unwrap();
    drop(claim);
    assert_eq!(client.controller.pending().unwrap(), [other.generation()]);
    assert!(client.controller.acquire(false).is_err());
}

/// The rows `client` wrote to its activity log, oldest first.
fn rows(client: &Client) -> Vec<Value> {
    fs::read_to_string(client.output_dir().unwrap().join(activity_log::FILE))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn labelled(client: &Client) -> Client {
    let mut labelled = client.clone().doing(Doing::new("click"));
    labelled.host_session =
        crate::host_session::HostSession::new(crate::host_session::Host::Claude, "d9031471-5e2a");
    labelled
}

#[test]
fn an_agent_row_carries_its_host_session_key() {
    let dir = tempfile::tempdir().unwrap();
    let client = labelled(&client_in(dir.path()));
    let key = client.host_session.as_ref().unwrap().key().clone();
    client.record_for_test(
        &json!({"op": "input", "actions": [{}]}),
        &json!({"ok": true}),
    );
    assert_eq!(rows(&client)[0]["session"], key.as_str());

    let mut viewer = client.clone();
    viewer.actor = Actor::Viewer;
    viewer.record_for_test(
        &json!({"op": "input", "actions": [{}]}),
        &json!({"ok": true}),
    );
    assert!(
        rows(&client)[1].get("session").is_none(),
        "the person's never does"
    );
}

#[test]
fn a_default_task_adds_nothing_to_a_row_and_a_named_one_adds_only_its_key() {
    let dir = tempfile::tempdir().unwrap();
    let session = labelled(&client_in(dir.path()));
    let key = session.host_session.as_ref().unwrap().key().clone();
    let mut default = session.clone();
    default.task = Some(Task::of(&key, None).unwrap());
    let mut named = session.clone();
    named.task = Some(Task::of(&key, Some("settings-sheet")).unwrap());
    let request = json!({"op": "input", "actions": [{}]});
    for client in [&session, &default, &named] {
        client.record_for_test(&request, &json!({"ok": true}));
    }

    let [plain, by_default, by_name] = <[Value; 3]>::try_from(rows(&session)).unwrap();
    let fields = |row: &Value| row.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(fields(&by_default), fields(&plain));
    let task = named.task.as_ref().unwrap().key();
    assert_eq!(by_name["task"], task.as_str());
    assert_ne!(task, &key);
    let line = fs::read_to_string(session.output_dir().unwrap().join(activity_log::FILE)).unwrap();
    assert!(!line.contains("settings-sheet"), "never the name");
}

#[test]
fn a_row_with_no_host_session_is_as_before() {
    let dir = tempfile::tempdir().unwrap();
    let client = labelled(&client_in(dir.path()));
    let mut plain = client.clone();
    plain.host_session = None;
    let request = json!({"op": "input", "actions": [{}]});
    client.record_for_test(&request, &json!({"ok": true}));
    plain.record_for_test(&request, &json!({"ok": true}));

    let [mut labelled, unlabelled] = <[Value; 2]>::try_from(rows(&client)).unwrap();
    labelled.as_object_mut().unwrap().remove("session");
    let fields = |row: &Value| row.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(fields(&labelled), fields(&unlabelled));
    assert_eq!(
        fields(&unlabelled),
        [
            "actionCount",
            "actor",
            "at",
            "command",
            "elapsedMs",
            "ok",
            "operation",
            "verb"
        ]
    );
}

#[test]
fn every_kind_of_row_is_written_byte_for_byte_as_the_log_keeps_it() {
    let dir = tempfile::tempdir().unwrap();
    let agent = labelled(&client_in(dir.path()));
    let key = agent.host_session.as_ref().unwrap().key().clone();
    let mut tasked = agent
        .clone()
        .doing(Doing::new("type").app(Some("TextEdit")));
    tasked.task = Some(Task::of(&key, Some("settings-sheet")).unwrap());
    let typed = json!({"op": "input", "actions": [{}, {}]});
    tasked.record_start_blocking(&typed).unwrap();
    tasked
        .record_blocking(&typed, &json!({"ok": true}), Duration::from_millis(5))
        .unwrap();
    let ran = agent
        .clone()
        .doing(Doing::new("open").app(Some("Calculator")));
    let run = json!({"op": "run", "argv": ["open"]});
    ran.record_blocking(&run, &json!({"ok": true, "exitCode": 2}), Duration::ZERO)
        .unwrap();
    ran.record_blocking(&run, &json!({"ok": true, "timedOut": true}), Duration::ZERO)
        .unwrap();
    let refused = agent.clone().doing(Doing::new("click"));
    refused
        .record_blocking(
            &typed,
            &json!({"ok": false, "code": "focus-changed", "performed": 1, "reason": "window"}),
            Duration::ZERO,
        )
        .unwrap();
    agent
        .clone()
        .doing(Doing::new("collect"))
        .record_blocking(
            &json!({"op": "collect"}),
            &json!({"ok": true, "count": 2, "bytes": 30}),
            Duration::ZERO,
        )
        .unwrap();
    agent
        .clone()
        .doing(Doing::new("screenshot"))
        .record_blocking(
            &json!({"op": "screenshot"}),
            &json!({"ok": true, "capture": "1761660429000123456-capture.png"}),
            Duration::ZERO,
        )
        .unwrap();
    let mut viewer = client_in(dir.path());
    viewer.actor = Actor::Viewer;
    viewer
        .record_blocking(
            &json!({"op": "restart"}),
            &json!({"ok": true}),
            Duration::ZERO,
        )
        .unwrap();

    let log = fs::read_to_string(agent.output_dir().unwrap().join(activity_log::FILE)).unwrap();
    let lines: Vec<String> = log
        .lines()
        .map(|line| {
            let row: Value = serde_json::from_str(line).unwrap();
            let mut line = line.replace(row["at"].as_str().unwrap(), "AT");
            if let Some(command) = row["command"].as_str() {
                line = line.replace(command, "COMMAND");
            }
            line.replace(key.as_str(), "SESSION")
                .replace(tasked.task.as_ref().unwrap().key().as_str(), "TASK")
        })
        .collect();
    assert_eq!(
        lines,
        [
            r#"{"actionCount":2,"actor":"agent","app":"TextEdit","at":"AT","command":"COMMAND","elapsedMs":0.0,"ok":null,"operation":"input","session":"SESSION","started":true,"task":"TASK","verb":"type"}"#,
            r#"{"actionCount":2,"actor":"agent","app":"TextEdit","at":"AT","command":"COMMAND","elapsedMs":5.0,"ok":true,"operation":"input","session":"SESSION","task":"TASK","verb":"type"}"#,
            r#"{"actionCount":null,"actor":"agent","app":"Calculator","at":"AT","code":"exit","command":"COMMAND","elapsedMs":0.0,"ok":false,"operation":"run","session":"SESSION","verb":"open"}"#,
            r#"{"actionCount":null,"actor":"agent","app":"Calculator","at":"AT","code":"timeout","command":"COMMAND","elapsedMs":0.0,"ok":false,"operation":"run","session":"SESSION","verb":"open"}"#,
            r#"{"actionCount":2,"actor":"agent","at":"AT","code":"focus-changed","command":"COMMAND","elapsedMs":0.0,"ok":false,"operation":"input","partial":true,"reason":"window","session":"SESSION","verb":"click"}"#,
            r#"{"actionCount":null,"actor":"agent","at":"AT","bytes":30,"command":"COMMAND","count":2,"elapsedMs":0.0,"ok":true,"operation":"collect","session":"SESSION","verb":"collect"}"#,
            r#"{"actionCount":null,"actor":"agent","at":"AT","capture":"1761660429000123456-capture.png","command":"COMMAND","elapsedMs":0.0,"ok":true,"operation":"screenshot","session":"SESSION","verb":"screenshot"}"#,
            r#"{"actionCount":null,"actor":"viewer","at":"AT","elapsedMs":0.0,"ok":true,"operation":"restart"}"#,
        ]
    );
}

#[test]
fn a_screenshot_row_names_its_capture_file_and_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    let client = labelled(&client_in(dir.path()));
    let name = "1761660429000123456-capture.png";
    let saved = json!({"ok": true, "path": format!("/Users/me/runs/{name}"), "unchanged": true});
    assert_eq!(
        capture_name(&saved),
        Some(name),
        "an unchanged capture names the earlier file"
    );

    let request = json!({"op": "screenshot"});
    for capture in [
        json!(name),
        json!("/Users/me/runs/1-capture.png"),
        json!("notes.png"),
    ] {
        client.record_for_test(&request, &json!({"ok": true, "capture": capture}));
    }
    let rows = rows(&client);
    assert_eq!(rows[0]["capture"], name);
    assert!(rows[1].get("capture").is_none() && rows[2].get("capture").is_none());
    assert!(!rows.iter().any(|row| row.to_string().contains("/Users")));
}

#[test]
fn a_refusal_for_another_app_in_front_says_whether_some_was_sent() {
    let dir = tempfile::tempdir().unwrap();
    let client = labelled(&client_in(dir.path()));
    let request = json!({"op": "input", "actions": [{}, {}]});
    for reply in [
        json!({"ok": false, "code": "focus-changed", "performed": 0, "partial": false}),
        json!({"ok": false, "code": "focus-changed", "performed": 1, "partial": false}),
        json!({"ok": false, "code": "focus-changed", "performed": 0, "partial": true}),
        json!({"ok": false, "code": "busy"}),
    ] {
        client.record_for_test(&request, &reply);
    }
    let partial: Vec<Value> = rows(&client)
        .iter()
        .map(|row| row["partial"].clone())
        .collect();
    assert_eq!(
        partial,
        [json!(false), json!(true), json!(true), Value::Null]
    );
}

#[test]
fn a_focus_refusals_row_keeps_why_it_was_refused_in_the_services_own_words_only() {
    let dir = tempfile::tempdir().unwrap();
    let client = labelled(&client_in(dir.path()));
    let request = json!({"op": "input", "actions": [{}]});
    for reason in [
        json!("app"),
        json!("point"),
        json!("unreadable"),
        json!("window"),
        json!("sheet"),
        json!("field"),
        json!("disabled"),
        json!("Calculator has keyboard focus"),
        Value::Null,
    ] {
        client.record_for_test(
            &request,
            &json!({"ok": false, "code": "focus-changed", "reason": reason, "performed": 0}),
        );
    }
    client.record_for_test(
        &request,
        &json!({"ok": false, "code": "busy", "reason": "app"}),
    );
    let reasons: Vec<Value> = rows(&client)
        .iter()
        .map(|row| row["reason"].clone())
        .collect();
    assert_eq!(
        reasons,
        [
            json!("app"),
            json!("point"),
            json!("unreadable"),
            json!("window"),
            json!("sheet"),
            json!("field"),
            json!("disabled"),
            Value::Null,
            Value::Null,
            Value::Null,
        ]
    );
}

#[test]
fn an_expectation_is_sent_only_to_a_service_that_names_the_capability() {
    let app = Expect::new(812).unwrap();
    let window = app.clone().in_window(4120).unwrap();
    let field = app.clone().in_field("Email").unwrap();
    let both = window.clone().in_field("Email").unwrap();
    let service = |extra: &[&str]| {
        let mut capabilities = vec![SHUTDOWN, HOLD, FOCUS_APP, FOCUS_DOCK];
        capabilities.extend_from_slice(extra);
        json!({"ok": true, "capabilities": capabilities})
    };
    let sent = |native: &Value, expect: &Expect| {
        expectation(native, Some(Check::Required(expect.clone())))
            .map(|sent| sent == Some(expect.clone()))
    };

    let app_only = service(&[]);
    assert!(sent(&app_only, &app).unwrap());
    for refused in [&window, &field, &both] {
        assert_eq!(
            sent(&app_only, refused).unwrap_err().code,
            "startup",
            "{refused:?}"
        );
    }

    let windows = service(&[FOCUS_WINDOW]);
    assert!(sent(&windows, &window).unwrap());
    assert_eq!(sent(&windows, &field).unwrap_err().code, "startup");
    assert_eq!(sent(&windows, &both).unwrap_err().code, "startup");

    let all = service(&[FOCUS_WINDOW, FOCUS_FIELD]);
    for expect in [&app, &window, &field, &both] {
        assert!(sent(&all, expect).unwrap(), "{expect:?}");
    }
}

/// A login is gone only once it is seen gone twice in a row and its
/// daemon stopped answering; a look in between that sees it again
/// starts the count over.
#[tokio::test]
async fn a_login_is_gone_only_when_seen_gone_twice_and_silent() {
    use std::{cell::RefCell, collections::VecDeque};

    let pause = Duration::from_millis(1);
    let limit = Duration::from_secs(5);
    let run = |looks: &[bool], hellos: &[bool]| {
        let looks = RefCell::new(looks.iter().copied().collect::<VecDeque<_>>());
        let hellos = RefCell::new(hellos.iter().copied().collect::<VecDeque<_>>());
        let seen = RefCell::new(0);
        async move {
            let gone = gone_within(
                || {
                    *seen.borrow_mut() += 1;
                    let look = looks.borrow_mut().pop_front().unwrap_or(false);
                    async move { look }
                },
                || {
                    let hello = hellos.borrow_mut().pop_front().unwrap_or(false);
                    async move { hello }
                },
                limit,
                pause,
            )
            .await;
            (gone, seen.into_inner())
        }
    };

    assert_eq!(run(&[true, true, true], &[]).await, (true, 3));
    assert_eq!(
        run(&[true, false, true, true, true], &[]).await,
        (true, 5),
        "a login seen again starts the count over"
    );
    assert_eq!(
        run(&[true, true, true, true, true], &[true, true]).await,
        (true, 5),
        "waits for the daemon to go quiet, looking all the while"
    );
    assert_eq!(
        run(&[true, true, false, true, true, true], &[true, false]).await,
        (true, 6),
        "a login back after the second look starts the count over"
    );
    assert_eq!(
        run(&[true, true, false, true, true, true], &[false, false]).await,
        (true, 6),
        "a login back while the daemon was asked starts the count over"
    );

    // Back after the second look, then its daemon goes quiet: still
    // logged in, never gone.
    let back = Duration::from_millis(50);
    let looks = std::cell::Cell::new(0);
    let hellos = std::cell::Cell::new(0);
    let returned = gone_within(
        || {
            looks.set(looks.get() + 1);
            let look = looks.get() <= 2;
            async move { look }
        },
        || {
            hellos.set(hellos.get() + 1);
            let hello = hellos.get() == 1;
            async move { hello }
        },
        back,
        pause,
    )
    .await;
    assert!(!returned, "a login that came back is not gone");

    // It logs in again while the daemon is still being asked, and the
    // daemon then says nothing: still logged in.
    let again = std::cell::Cell::new(false);
    let returned = gone_within(
        || {
            let look = !again.get();
            async move { look }
        },
        || {
            again.set(true);
            async move {
                tokio::time::sleep(pause).await;
                false
            }
        },
        back,
        pause,
    )
    .await;
    assert!(!returned, "back while its daemon was asked");

    let short = Duration::from_millis(50);
    let stays = gone_within(|| async { false }, || async { false }, short, pause).await;
    assert!(!stays, "a login that stays is not gone");
    let answering = gone_within(|| async { true }, || async { true }, short, pause).await;
    assert!(!answering, "a daemon that keeps answering is not gone");
}

#[tokio::test]
async fn an_input_change_stays_pending_only_while_its_outcome_is_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = client_in(dir.path());
    client.controller = crate::control::Controller::at(60001, &dir.path().join("state"));

    assert_eq!(
        client.pending_until_known(async { Ok(1) }).await.unwrap(),
        1
    );

    let refused = Error::new(Code::Native, "refused");
    assert!(
        client
            .pending_until_known(async { Err::<(), _>(refused) })
            .await
            .is_err()
    );
    assert!(
        client.controller.pending().unwrap().is_empty(),
        "known outcomes clear"
    );

    let lost = Error::new(Code::Transport, "reply lost").uncertain();
    assert!(
        client
            .pending_until_known(async { Err::<(), _>(lost) })
            .await
            .is_err()
    );
    assert_eq!(
        client.controller.unresolved().unwrap().len(),
        1,
        "an unknown one stays"
    );
}

#[test]
fn process_existence_ignores_ownership() {
    assert!(process_exists(std::process::id()));
    assert!(process_exists(1), "launchd exists though owned by root");
    assert!(!process_exists(u32::MAX));
}

#[tokio::test]
async fn only_the_person_resumes_agents() {
    let dir = tempfile::tempdir().unwrap();
    let client = client_in(dir.path());
    assert_eq!(client.hold(false).await.unwrap_err().code, "stopped");
}

#[tokio::test]
async fn a_stop_holds_on_the_persons_side_while_the_helpers_input_is_down() {
    let dir = tempfile::tempdir().unwrap();
    let mut person = client_in(dir.path());
    person.controller = crate::control::Controller::at(60001, &dir.path().join("state"));
    person.actor = Actor::Viewer;
    let mut agent = client_in(dir.path());
    agent.controller = crate::control::Controller::at(60001, &dir.path().join("state"));

    let stopped = person.hold(true).await.unwrap();
    assert_eq!(stopped["held"], true);
    assert!(
        stopped["warnings"][0]
            .as_str()
            .unwrap()
            .contains("isn't running")
    );
    assert_eq!(agent.require_going().await.unwrap_err().code, "stopped");
    assert!(
        tokio::time::timeout(Duration::from_secs(2), agent.stopped_while_running())
            .await
            .is_ok()
    );
    let acting = agent.clone().doing(Doing::new("run"));
    let refused = acting
        .mutate(json!({"op": "run", "argv": ["true"]}), None, None, None)
        .await
        .unwrap_err();
    assert_eq!(refused.code, "stopped");
    assert!(!refused.uncertain, "nothing started");

    let marker = person.config.input_socket.with_extension("stopped");
    fs::write(&marker, b"").unwrap();
    let refused = person.hold(false).await.unwrap_err();
    assert_eq!(refused.code, "stopped");
    assert!(person.controller.stop_noted());
    assert!(marker.exists());

    fs::remove_file(marker).unwrap();
    person.hold(false).await.unwrap();
    assert!(agent.require_going().await.is_ok(), "resumed");
}

#[tokio::test]
async fn a_running_command_hears_of_a_stop_from_its_saved_marker() {
    let dir = tempfile::tempdir().unwrap();
    let client = client_in(dir.path());
    let marker = client.config.input_socket.with_extension("stopped");

    let waiting = tokio::time::timeout(Duration::from_millis(500), client.stopped_while_running());
    assert!(waiting.await.is_err(), "no stop, no end");

    std::fs::write(&marker, b"").unwrap();
    let heard = tokio::time::timeout(Duration::from_secs(2), client.stopped_while_running());
    assert!(heard.await.is_ok(), "the stop is heard within a moment");
}

#[tokio::test]
async fn a_local_stop_cancels_a_running_command_and_refuses_its_next_dispatch() {
    let (dir, mut client, input, _session) = native_client();
    drop(input);
    client.transport.path = dir.path().join("helper.sock");
    let helper = UnixListener::bind(&client.transport.path).unwrap();
    let mut person = client.clone();
    person.actor = Actor::Viewer;
    let acting = client.doing(Doing::new("run"));
    let run = json!({"op": "run", "argv": ["true"], "timeoutMs": 1000});
    let ownership = acting.controller.acquire(false).unwrap();
    let mut caller = Box::pin(acting.mutate(run.clone(), None, None, None));
    let mut peer = dispatched(&mut caller, &helper, "run").await;
    let pending = acting.controller.pending().unwrap();
    assert_eq!(pending.len(), 1);

    let stopped = person.hold(true).await.unwrap();
    assert_eq!(stopped["ok"], true);
    assert_eq!(stopped["held"], true);
    assert!(
        !acting
            .config
            .input_socket
            .with_extension("stopped")
            .exists()
    );
    let error = tokio::time::timeout(Duration::from_secs(2), caller)
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, "stopped");
    assert!(error.uncertain);
    assert_eq!(acting.controller.pending().unwrap(), pending);
    assert_eq!(acting.controller.acquire(true).unwrap_err().code, "busy");
    let mut line = String::new();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), peer.read_line(&mut line))
            .await
            .unwrap()
            .unwrap(),
        0,
        "the helper sees the cancelled connection"
    );

    let error = acting.mutate(run, None, None, None).await.unwrap_err();
    assert_eq!(error.code, "stopped");
    assert!(!error.uncertain, "the next command never starts");
    assert!(
        tokio::time::timeout(Duration::from_millis(50), helper.accept())
            .await
            .is_err()
    );
    drop(ownership);
    recovery_settles_the_abandoned_claim(&acting);
}

/// Drive `caller` for a moment past sending its request, as it waits
/// for the reply.
async fn waits<T: std::fmt::Debug>(caller: &mut (impl Future<Output = Result<T>> + Unpin)) {
    assert!(
        tokio::time::timeout(SOON, caller).await.is_err(),
        "it still waits for its reply"
    );
}

/// A command an agent started with `run`, which the stand-in leaves
/// unanswered until the test answers it.
fn command() -> Value {
    json!({"op": "run", "argv": ["sleep", "30"], "timeoutMs": 60_000})
}

#[tokio::test]
async fn a_run_lets_go_of_control_once_its_request_is_sent() {
    if !crate::test_support::isolated(
        "client::tests::a_run_lets_go_of_control_once_its_request_is_sent",
    ) {
        return;
    }
    let (dir, client, input, _session) = native_client();
    drop(input);
    let (acting, helper, _interrupt) = running(dir.path(), client);
    let other = another(&acting, dir.path());
    let ownership = acting.controller.acquire_beside_commands().unwrap();
    let mut caller = Box::pin(acting.mutate(command(), None, None, Some(ownership)));
    let mut peer = dispatched(&mut caller, &helper, "run").await;
    waits(&mut caller).await;

    drop(other.controller.acquire_beside_commands().unwrap());
    drop(other.controller.acquire_for_person().unwrap());
    let refused = other.controller.acquire(true).unwrap_err();
    assert_eq!(refused.code, "busy");
    assert_eq!(refused.holder, Some(Holder::Agent));
    assert_eq!(other.controller.acquire(false).unwrap_err().code, "busy");
    assert!(other.controller.pending_causes().unwrap()[0].command);

    answer(
        &mut peer,
        json!({"ok": true, "exitCode": 0, "timedOut": false}),
    )
    .await;
    assert_eq!(caller.await.unwrap()["exitCode"], 0);
    assert!(other.controller.pending().unwrap().is_empty());
    assert!(other.controller.acquire(false).is_ok());
}

#[tokio::test]
async fn a_run_refused_before_it_is_sent_settles_under_control() {
    if !crate::test_support::isolated(
        "client::tests::a_run_refused_before_it_is_sent_settles_under_control",
    ) {
        return;
    }
    let (dir, client, input, _session) = native_client();
    drop(input);
    let (acting, helper, interrupt) = running(dir.path(), client);
    let other = another(&acting, dir.path());
    // A lease of a helper of its own holds the request back before it
    // is sent, while the run keeps its control.
    let lease: SharedLease = Arc::new(tokio::sync::Mutex::new(crate::control::Lease::new(
        Duration::from_secs(10),
    )));
    let elsewhere = Controller::at(60001, &dir.path().join("elsewhere"));
    let token = lease.lock().await.acquire(&elsewhere).unwrap();
    let authorizing = lease.lock().await;
    let ownership = acting.controller.acquire_beside_commands().unwrap();
    let mut caller =
        Box::pin(acting.mutate(command(), Some((&lease, &token)), None, Some(ownership)));
    assert!(tokio::time::timeout(SOON, &mut caller).await.is_err());
    assert_eq!(
        other.controller.acquire_beside_commands().unwrap_err().code,
        "busy",
        "control is still held"
    );
    interrupt.ask(Signal::SIGTERM);
    drop(authorizing);

    let error = caller.await.unwrap_err();
    assert_eq!(error.code, "interrupted");
    assert!(!error.uncertain, "nothing was sent");
    let (peer, _) = helper.accept().await.unwrap();
    let mut line = String::new();
    assert_eq!(
        BufReader::new(peer).read_line(&mut line).await.unwrap(),
        0,
        "the helper got no request"
    );
    assert!(other.controller.pending().unwrap().is_empty());
    assert!(other.controller.acquire(false).is_ok());
}

#[tokio::test]
async fn a_stopped_run_stays_pending_beside_other_agents() {
    if !crate::test_support::isolated(
        "client::tests::a_stopped_run_stays_pending_beside_other_agents",
    ) {
        return;
    }
    let (dir, client, input, _session) = native_client();
    drop(input);
    let (acting, helper, _interrupt) = running(dir.path(), client);
    let other = another(&acting, dir.path());
    let mut person = acting.clone();
    person.actor = Actor::Viewer;
    let ownership = acting.controller.acquire_beside_commands().unwrap();
    let mut caller = Box::pin(acting.mutate(command(), None, None, Some(ownership)));
    let mut peer = dispatched(&mut caller, &helper, "run").await;
    waits(&mut caller).await;
    let pending = other.controller.pending().unwrap();

    person.hold(true).await.unwrap();
    let error = tokio::time::timeout(Duration::from_secs(2), caller)
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, "stopped");
    assert!(error.uncertain);
    let mut line = String::new();
    assert_eq!(
        peer.read_line(&mut line).await.unwrap(),
        0,
        "the helper sees the connection close, which ends the command"
    );
    assert_eq!(other.controller.pending().unwrap(), pending);
    assert_eq!(other.controller.unresolved().unwrap(), pending);
    assert_eq!(
        other.controller.acquire_beside_commands().unwrap_err().code,
        "uncertain"
    );
    recovery_settles_the_abandoned_claim(&other);
}

#[tokio::test]
async fn a_lost_run_reply_blocks_every_agent() {
    if !crate::test_support::isolated("client::tests::a_lost_run_reply_blocks_every_agent") {
        return;
    }
    let (dir, client, input, _session) = native_client();
    drop(input);
    let (acting, helper, _interrupt) = running(dir.path(), client);
    let other = another(&acting, dir.path());
    let ownership = acting.controller.acquire_beside_commands().unwrap();
    let mut caller = Box::pin(acting.mutate(command(), None, None, Some(ownership)));
    let peer = dispatched(&mut caller, &helper, "run").await;
    waits(&mut caller).await;

    drop(peer);
    let error = caller.await.unwrap_err();
    assert!(error.uncertain, "{error:?}");
    for refused in [
        other.controller.acquire_beside_commands(),
        other.controller.acquire_for_person(),
        other.controller.acquire(false),
    ] {
        assert_eq!(refused.unwrap_err().code, "uncertain");
    }
    recovery_settles_the_abandoned_claim(&other);
}

#[tokio::test]
async fn a_lost_or_uncertain_run_reply_is_unknown_before_its_row_is_written() {
    if !crate::test_support::isolated(
        "client::tests::a_lost_or_uncertain_run_reply_is_unknown_before_its_row_is_written",
    ) {
        return;
    }
    for lost in [true, false] {
        lost_before_its_row(lost).await;
    }
}

/// A run that let go of control ends with its reply `lost`, or with
/// one saying its outcome is unknown, while another command writes its
/// row: everyone hears it is unknown before its own row is written.
async fn lost_before_its_row(lost: bool) {
    let (dir, client, input, _session) = native_client();
    drop(input);
    let (acting, helper, _interrupt) = running(dir.path(), client);
    let other = another(&acting, dir.path());
    let output = acting.output_dir().unwrap().to_owned();
    let ownership = acting.controller.acquire_beside_commands().unwrap();
    // Writing a row blocks its thread, so the run has one of its own.
    let caller = std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(acting.mutate(command(), None, None, Some(ownership)))
    });
    let (peer, _) = tokio::time::timeout(Duration::from_secs(2), helper.accept())
        .await
        .unwrap()
        .unwrap();
    let mut peer = BufReader::new(peer);
    let mut line = String::new();
    peer.read_line(&mut line).await.unwrap();
    let started = Instant::now();
    while !fs::read_to_string(output.join(activity_log::FILE)).is_ok_and(|log| !log.is_empty()) {
        assert!(started.elapsed() < Duration::from_secs(2), "it began");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let activity = fs::File::create(output.join(activity_log::LOCK)).unwrap();
    FileExt::lock_exclusive(&activity).unwrap();
    if lost {
        drop(peer);
    } else {
        answer(
            &mut peer,
            json!({"ok": false, "code": "restart", "error": "The helper stopped", "uncertain": true}),
        )
        .await;
    }
    let started = Instant::now();
    loop {
        match other.controller.acquire_beside_commands() {
            Err(error) if error.code == Code::Uncertain => break,
            _ => {
                assert!(started.elapsed() < Duration::from_secs(2), "never unknown");
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    }
    assert!(!caller.is_finished(), "its row still waits");
    FileExt::unlock(&activity).unwrap();

    match caller.join().unwrap() {
        Err(error) => assert!(lost && error.uncertain, "{error:?}"),
        Ok(reply) => assert!(!lost && reply["uncertain"] == true, "{reply}"),
    }
    assert_eq!(rows(&other).pop().unwrap()["ok"], false);
    recovery_settles_the_abandoned_claim(&other);
}

#[tokio::test]
async fn a_run_that_let_go_and_is_cut_after_the_grace_stays_pending_for_everyone() {
    if !crate::test_support::isolated(
        "client::tests::a_run_that_let_go_and_is_cut_after_the_grace_stays_pending_for_everyone",
    ) {
        return;
    }
    let (dir, client, input, _session) = native_client();
    drop(input);
    let (acting, helper, interrupt) = running(dir.path(), client);
    let other = another(&acting, dir.path());
    let ownership = acting.controller.acquire_beside_commands().unwrap();
    let mut caller = Box::pin(acting.mutate(command(), None, None, Some(ownership)));
    let mut peer = dispatched(&mut caller, &helper, "run").await;
    waits(&mut caller).await;
    interrupt.ask(Signal::SIGTERM);

    let error = tokio::time::timeout(GRACE * 5, caller)
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, "interrupted");
    assert!(error.uncertain);
    let mut line = String::new();
    assert_eq!(
        peer.read_line(&mut line).await.unwrap(),
        0,
        "the helper sees the connection close, which ends the command"
    );
    for refused in [
        other.controller.acquire_beside_commands(),
        other.controller.acquire_for_person(),
    ] {
        assert_eq!(refused.unwrap_err().code, "uncertain");
    }
    recovery_settles_the_abandoned_claim(&other);
}

#[tokio::test]
async fn two_runs_go_on_beside_each_other() {
    if !crate::test_support::isolated("client::tests::two_runs_go_on_beside_each_other") {
        return;
    }
    let (dir, client, input, _session) = native_client();
    drop(input);
    let (acting, helper, _interrupt) = running(dir.path(), client);
    let second = another(&acting, dir.path());
    let other = another(&acting, dir.path());

    let first_control = acting.controller.acquire_beside_commands().unwrap();
    let mut first = Box::pin(acting.mutate(command(), None, None, Some(first_control)));
    let mut first_peer = dispatched(&mut first, &helper, "run").await;
    waits(&mut first).await;
    let second_control = second.controller.acquire_beside_commands().unwrap();
    let mut beside = Box::pin(second.mutate(command(), None, None, Some(second_control)));
    let mut second_peer = dispatched(&mut beside, &helper, "run").await;
    waits(&mut beside).await;

    let causes = other.controller.pending_causes().unwrap();
    assert_eq!(causes.len(), 2);
    assert!(causes.iter().all(|cause| cause.command));
    drop(other.controller.acquire_beside_commands().unwrap());
    assert_eq!(other.controller.acquire(true).unwrap_err().code, "busy");

    let ended = json!({"ok": true, "exitCode": 0, "timedOut": false});
    answer(&mut second_peer, ended.clone()).await;
    beside.await.unwrap();
    assert_eq!(other.controller.pending().unwrap().len(), 1);
    answer(&mut first_peer, ended).await;
    first.await.unwrap();
    assert!(other.controller.pending().unwrap().is_empty());
    assert!(other.controller.acquire(false).is_ok());
}

#[tokio::test]
async fn input_goes_on_beside_a_running_command() {
    if !crate::test_support::isolated("client::tests::input_goes_on_beside_a_running_command") {
        return;
    }
    let (dir, client, input, _session) = native_client();
    drop(input);
    let running = another(&client, dir.path());

    let command = running.controller.quarantine_command().unwrap();
    let actions = json!([{"type": "key", "key": "a"}]);
    let beside = client.input(&actions, None, None).await.unwrap_err();
    assert!(
        ![Code::Busy, Code::Uncertain].contains(&beside.code),
        "only the helper refuses it: {beside:?}"
    );
    running.controller.clear_if(&command).unwrap();
    drop(command);

    let change = running.controller.quarantine().unwrap();
    let refused = client.input(&actions, None, None).await.unwrap_err();
    assert_eq!(refused.code, "uncertain", "an unmarked change still blocks");
    running.controller.clear_if(&change).unwrap();
}

#[tokio::test]
async fn other_commands_keep_control_for_their_whole_run() {
    let (dir, client, input, _session) = native_client();
    drop(input);
    let (acting, helper, _interrupt) = running(dir.path(), client);
    let other = another(&acting, dir.path());
    let ownership = acting.controller.acquire_beside_commands().unwrap();
    let mut caller = Box::pin(acting.mutate(command(), None, None, None));
    let mut peer = dispatched(&mut caller, &helper, "run").await;
    waits(&mut caller).await;

    assert_eq!(
        other.controller.acquire_beside_commands().unwrap_err().code,
        "busy",
        "the caller keeps control"
    );
    assert!(!other.controller.pending_causes().unwrap()[0].command);

    answer(
        &mut peer,
        json!({"ok": true, "exitCode": 0, "timedOut": false}),
    )
    .await;
    caller.await.unwrap();
    drop(ownership);
    assert!(other.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn a_local_stop_succeeds_and_cancels_a_running_command_when_native_hold_fails() {
    let (dir, mut client, input, _session) = native_client();
    client.transport.path = dir.path().join("helper.sock");
    let helper = UnixListener::bind(&client.transport.path).unwrap();
    let mut person = client.clone();
    person.actor = Actor::Viewer;
    let acting = client.doing(Doing::new("run"));
    let ownership = acting.controller.acquire(false).unwrap();
    let mut caller = Box::pin(acting.mutate(
        json!({"op": "run", "argv": ["true"], "timeoutMs": 1000}),
        None,
        None,
        None,
    ));
    let mut peer = dispatched(&mut caller, &helper, "run").await;
    let pending = acting.controller.pending().unwrap();
    assert_eq!(pending.len(), 1);

    let mut stopping = Box::pin(person.hold(true));
    let mut hello = dispatched(&mut stopping, &input, "hello").await;
    answer(&mut hello, json!({"ok": true, "capabilities": ["hold"]})).await;
    let mut hold = dispatched(&mut stopping, &input, "hold").await;
    answer(
        &mut hold,
        json!({"ok": false, "error": "Cleanup failed", "uncertain": true}),
    )
    .await;
    assert_eq!(
        stopping.await.unwrap(),
        json!({
            "ok": true,
            "held": true,
            "kept": true,
            "uncertain": true,
            "warnings": ["The helper's input service may or may not have been told. Agents are stopped here all the same."],
        })
    );
    assert!(person.controller.stop_noted());
    let error = tokio::time::timeout(Duration::from_secs(2), caller)
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, "stopped");
    assert!(error.uncertain);
    assert_eq!(acting.controller.pending().unwrap(), pending);
    assert_eq!(acting.controller.acquire(true).unwrap_err().code, "busy");
    let mut line = String::new();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), peer.read_line(&mut line))
            .await
            .unwrap()
            .unwrap(),
        0,
        "the helper sees the cancelled connection"
    );
    drop(ownership);
    recovery_settles_the_abandoned_claim(&acting);
}

#[tokio::test]
async fn a_stop_returns_ok_with_the_native_problem_when_native_hold_fails() {
    let (_dir, mut person, input, _session) = native_client();
    person.actor = Actor::Viewer;
    let mut stopping = Box::pin(person.hold(true));
    let mut hello = dispatched(&mut stopping, &input, "hello").await;
    answer(&mut hello, json!({"ok": true, "capabilities": ["hold"]})).await;
    let mut hold = dispatched(&mut stopping, &input, "hold").await;
    answer(
        &mut hold,
        json!({"ok": false, "error": "Cleanup failed", "uncertain": false}),
    )
    .await;

    assert_eq!(
        stopping.await.unwrap(),
        json!({
            "ok": true,
            "held": true,
            "kept": true,
            "warnings": ["The helper's input service reported a problem while stopping: Cleanup failed. Agents are stopped here all the same."],
        })
    );
    assert!(person.controller.stop_noted());
}

#[tokio::test]
async fn a_stop_returns_ok_with_uncertainty_and_a_warning_when_native_hold_is_uncertain() {
    let (_dir, mut person, input, _session) = native_client();
    person.actor = Actor::Viewer;
    let mut stopping = Box::pin(person.hold(true));
    let mut hello = dispatched(&mut stopping, &input, "hello").await;
    answer(&mut hello, json!({"ok": true, "capabilities": ["hold"]})).await;
    let mut hold = dispatched(&mut stopping, &input, "hold").await;
    answer(
        &mut hold,
        json!({"ok": false, "error": "Cleanup failed", "uncertain": true}),
    )
    .await;

    assert_eq!(
        stopping.await.unwrap(),
        json!({
            "ok": true,
            "held": true,
            "kept": true,
            "uncertain": true,
            "warnings": ["The helper's input service may or may not have been told. Agents are stopped here all the same."],
        })
    );
    assert!(person.controller.stop_noted());
}

#[tokio::test]
async fn resume_keeps_the_local_stop_until_the_helpers_stop_can_be_cleared() {
    let (_dir, client, listener, _session) = native_client();
    let mut person = client;
    person.actor = Actor::Viewer;
    let marker = person.config.input_socket.with_extension("stopped");
    let mut stopping = Box::pin(person.hold(true));
    let mut hello = dispatched(&mut stopping, &listener, "hello").await;
    answer(&mut hello, json!({"ok": true, "capabilities": ["hold"]})).await;
    let mut hold = dispatched(&mut stopping, &listener, "hold").await;
    fs::write(&marker, b"").unwrap();
    answer(&mut hold, json!({"ok": true, "held": true, "kept": true})).await;
    assert_eq!(stopping.await.unwrap()["held"], true);
    drop(listener);

    let error = person.hold(false).await.unwrap_err();
    assert_eq!(error.code, "stopped");
    assert!(
        error
            .error
            .contains("Recover input before agents can resume")
    );
    assert!(person.controller.stop_noted());
    assert!(marker.exists());
    let mut agent = person.clone();
    agent.actor = Actor::Agent;
    assert_eq!(agent.require_going().await.unwrap_err().code, "stopped");

    fs::remove_file(marker).unwrap();
    assert_eq!(person.hold(false).await.unwrap()["held"], false);
    assert!(!person.controller.stop_noted());
}

#[tokio::test]
async fn resume_clears_the_local_stop_only_after_native_acknowledgment() {
    for acknowledged in [false, true] {
        let (_dir, client, listener, _session) = native_client();
        let mut person = client;
        person.actor = Actor::Viewer;
        person.controller.note_stop(true).unwrap();
        let mut resuming = Box::pin(person.hold(false));
        let mut hello = dispatched(&mut resuming, &listener, "hello").await;
        assert!(person.controller.stop_noted());
        answer(&mut hello, json!({"ok": true, "capabilities": ["hold"]})).await;
        let mut hold = dispatched(&mut resuming, &listener, "hold").await;
        assert!(person.controller.stop_noted());
        answer(
            &mut hold,
            json!({"ok": acknowledged, "code": "native", "error": "refused", "held": false}),
        )
        .await;
        assert_eq!(resuming.await.is_ok(), acknowledged);
        assert_eq!(person.controller.stop_noted(), !acknowledged);
    }
}

#[tokio::test]
async fn a_stopped_agents_command_is_refused_before_it_starts_and_a_read_is_not() {
    let dir = tempfile::tempdir().unwrap();
    let client = client_in(dir.path());
    std::fs::write(client.config.input_socket.with_extension("stopped"), b"").unwrap();
    let run = json!({"op": "run", "argv": ["true"], "timeoutMs": 1000});

    let acting = client.clone().doing(Doing::new("run"));
    let refused = acting
        .mutate(run.clone(), None, None, None)
        .await
        .unwrap_err();
    assert_eq!(refused.code, "stopped");
    assert!(!refused.uncertain, "nothing started");

    let reading = client.mutate(run, None, None, None).await.unwrap_err();
    assert_ne!(
        reading.code, "stopped",
        "a read goes on (and finds no helper here)"
    );
}

#[test]
fn input_names_the_app_it_landed_in_and_a_command_its_start() {
    let dir = tempfile::tempdir().unwrap();
    let client = client_in(dir.path()).doing(Doing::new("type"));
    let typed = json!({"op": "input", "actions": [{"type": "type", "text": "x"}]});
    client
        .record_blocking(
            &typed,
            &json!({"ok": true, "app": "TextEdit"}),
            Duration::ZERO,
        )
        .unwrap();
    let rows = crate::activity_log::recent(client.output_dir().unwrap(), 5);
    assert_eq!(crate::doing::says(&rows[0]), "typed in TextEdit");

    let client = client.doing(Doing::new("open").app(Some("Calculator")));
    let run = json!({"op": "run", "argv": ["open", "-a", "Calculator"]});
    client.record_start_blocking(&run).unwrap();
    let rows = crate::activity_log::recent(client.output_dir().unwrap(), 5);
    assert_eq!(crate::doing::says(&rows[0]), "opening Calculator…");
    client
        .record_blocking(&run, &json!({"ok": true, "exitCode": 0}), Duration::ZERO)
        .unwrap();
    let rows = crate::activity_log::recent(client.output_dir().unwrap(), 5);
    assert_eq!(
        crate::doing::says(&rows[0]),
        "opened Calculator",
        "the end stands for the start"
    );
    assert_eq!(rows.len(), 2, "one row per command");
}

#[test]
fn input_names_the_bundle_of_the_app_it_landed_in_as_the_helper_saw_it() {
    let dir = tempfile::tempdir().unwrap();
    let client = client_in(dir.path()).doing(Doing::new("click"));
    let clicked = json!({"op": "input", "actions": [{"type": "click", "x": 1, "y": 2}]});
    let landed = |reply: Value| {
        client
            .record_blocking(&clicked, &reply, Duration::ZERO)
            .unwrap();
        activity_log::recent(client.output_dir().unwrap(), 1).remove(0)
    };

    let row = landed(json!({"ok": true, "app": "TextEdit", "bundleId": "com.apple.TextEdit"}));
    assert_eq!(row.app.as_deref(), Some("TextEdit"));
    assert_eq!(row.bundle_id.as_deref(), Some("com.apple.TextEdit"));

    for odd in [
        json!("/Applications/TextEdit.app"),
        json!("com.apple TextEdit"),
        json!(7),
    ] {
        let row = landed(json!({"ok": true, "app": "TextEdit", "bundleId": odd}));
        assert_eq!(row.bundle_id, None, "{odd}");
    }
    let row = landed(json!({"ok": true, "bundleId": "com.apple.TextEdit"}));
    assert_eq!(
        (row.app, row.bundle_id),
        (None, None),
        "an identifier for no app named"
    );
}

#[test]
fn typed_text_names_its_field_only_by_the_role_the_helper_found_it_of() {
    let dir = tempfile::tempdir().unwrap();
    let client =
        client_in(dir.path()).doing(Doing::new("type").app(Some("Notes")).field(Some("Email")));
    let typed = json!({"op": "input", "actions": [{"type": "type", "text": "sam@example.com"}]});
    let landed = |reply: Value| {
        client
            .record_blocking(&typed, &reply, Duration::ZERO)
            .unwrap();
        activity_log::recent(client.output_dir().unwrap(), 1).remove(0)
    };

    let row = landed(json!({"ok": true, "app": "Notes", "field": {"role": "AXTextField"}}));
    assert_eq!(row.label.as_deref(), Some("Email"));
    assert_eq!(crate::doing::says(&row), "typed into Email in Notes");

    for unchecked in [
        json!({"ok": true, "app": "Notes"}),
        json!({"ok": true, "app": "Notes", "field": {"role": "AXTextArea"}}),
        json!({"ok": true, "app": "Notes", "field": {"role": 7}}),
        json!({"ok": true, "app": "Notes", "field": "AXTextField"}),
        json!({"ok": false, "code": "focus-changed", "reason": "field",
            "field": {"role": "AXTextField"}}),
    ] {
        let row = landed(unchecked.clone());
        assert_eq!(row.label, None, "{unchecked}");
    }
    let written =
        fs::read_to_string(client.output_dir().unwrap().join(activity_log::FILE)).unwrap();
    assert!(
        !written.contains("sam@example.com"),
        "typed text is never kept"
    );
}

#[test]
fn a_bundle_id_seen_for_another_app_than_the_one_named_is_not_kept() {
    let dir = tempfile::tempdir().unwrap();
    let typed = json!({"op": "input", "actions": [{"type": "type", "text": "x"}]});
    let landed = |named: &str, seen: &str| {
        let client = client_in(dir.path()).doing(Doing::new("type").app(Some(named)));
        let reply = json!({"ok": true, "app": seen, "bundleId": "com.apple.calculator"});
        client
            .record_blocking(&typed, &reply, Duration::ZERO)
            .unwrap();
        activity_log::recent(client.output_dir().unwrap(), 1).remove(0)
    };

    let other = landed("TextEdit", "Calculator");
    assert_eq!(other.app.as_deref(), Some("TextEdit"));
    assert_eq!(other.bundle_id, None);
    let same = landed("calculator", "Calculator");
    assert_eq!(same.bundle_id.as_deref(), Some("com.apple.calculator"));
}

#[test]
fn an_install_row_keeps_the_installed_apps_bundle_id() {
    use base64::{Engine, engine::general_purpose::STANDARD};

    let dir = tempfile::tempdir().unwrap();
    let client =
        client_in(dir.path()).doing(Doing::new("install").app(Some("/tmp/Secret/Probe.app")));
    let run = json!({"op": "run", "argv": ["atlas", "environment-worker"]});
    let answered = |answer: Value| {
        let output = STANDARD.encode(answer.to_string());
        let reply = json!({"ok": true, "exitCode": 0, "timedOut": false, "signal": null,
            "events": [{"event": "output", "data": output}]});
        client
            .record_blocking(&run, &reply, Duration::ZERO)
            .unwrap();
        activity_log::recent(client.output_dir().unwrap(), 1).remove(0)
    };
    let installed = |app: &str, id: Value| {
        json!({"ok": true, "installed": true, "installId": "ID", "app": app,
            "path": "/Users/helper/Applications/Probe.app", "bundleId": id,
            "version": "1.0 (1)", "registered": true})
    };

    let row = answered(installed("Probe", json!("test.atlas.install-probe")));
    assert_eq!(crate::doing::says(&row), "installed Probe");
    assert_eq!(row.bundle_id.as_deref(), Some("test.atlas.install-probe"));
    let text =
        std::fs::read_to_string(client.output_dir().unwrap().join(activity_log::FILE)).unwrap();
    assert!(
        !text.contains("/Users/helper") && !text.contains("Secret"),
        "{text}"
    );

    assert_eq!(
        answered(installed("Other", json!("test.other"))).bundle_id,
        None
    );
    assert_eq!(answered(installed("Probe", json!(null))).bundle_id, None);
    assert_eq!(answered(installed("Probe", json!("a b"))).bundle_id, None);
    let refused = json!({"ok": false, "installId": "ID", "phase": "held", "error": "x"});
    assert_eq!(answered(refused).bundle_id, None);
}

/// A viewer's client for `dir`, gathering the person's input as a viewer
/// does.
/// A viewer's client in `dir` whose person's input gathers a `span`.
fn persons_client(dir: &Path, span: Duration) -> Client {
    let mut client = client_in(dir);
    client.actor = Actor::Viewer;
    client.person_input = Some(PersonInput::with_span(span));
    client
}

/// A span no test outlasts, so gathering never ends while a test runs.
const LONG_SPAN: Duration = Duration::from_secs(3600);
/// A span short enough to wait out in a test.
const SHORT_SPAN: Duration = Duration::from_millis(200);

#[test]
fn the_persons_input_through_a_viewer_is_one_row_a_span_counting_every_action() {
    let dir = tempfile::tempdir().unwrap();
    let client = persons_client(dir.path(), LONG_SPAN);
    let clicked = json!({"op": "input", "actions": [{"type": "click", "x": 412, "y": 96}, {"type": "type", "text": "hunter2"}]});
    for _ in 0..30 {
        client
            .record_blocking(
                &clicked,
                &json!({"ok": true, "performed": 2}),
                Duration::ZERO,
            )
            .unwrap();
    }
    let output = client.output_dir().unwrap().to_path_buf();
    assert!(
        activity_log::recent(&output, 5).is_empty(),
        "gathered until its span ends"
    );

    let refused = json!({"ok": false, "code": "busy"});
    client
        .record_blocking(&clicked, &refused, Duration::ZERO)
        .unwrap();
    let rows = activity_log::recent(&output, 5);
    assert_eq!(
        rows.iter()
            .map(|row| (row.failed(), row.action_count))
            .collect::<Vec<_>>(),
        [(true, Some(2)), (false, Some(60))],
        "a failure is written at once, after the input before it"
    );

    client
        .record_blocking(&clicked, &json!({"ok": true}), Duration::ZERO)
        .unwrap();
    client.person_input.as_ref().unwrap().flush().unwrap();
    let text = std::fs::read_to_string(output.join(activity_log::FILE)).unwrap();
    assert_eq!(text.lines().count(), 3);
    // Digits can turn up in a row's times by chance, so coordinates are
    // looked for as fields, and the typed text as words.
    assert!(!text.contains("hunter2"), "no typed text");
    for line in text.lines() {
        let row: Value = serde_json::from_str(line).unwrap();
        for field in ["x", "y", "text", "actions"] {
            assert!(row.get(field).is_none(), "{field} in {line}");
        }
    }

    // An agent's input, and the person's elsewhere, are written at once.
    let agent = client_in(dir.path()).doing(Doing::new("click"));
    agent
        .record_blocking(&clicked, &json!({"ok": true}), Duration::ZERO)
        .unwrap();
    let mut unshared = persons_client(dir.path(), LONG_SPAN);
    unshared.person_input = None;
    unshared
        .record_blocking(&clicked, &json!({"ok": true}), Duration::ZERO)
        .unwrap();
    let text = std::fs::read_to_string(output.join(activity_log::FILE)).unwrap();
    assert_eq!(text.lines().count(), 5);
}

#[test]
fn the_persons_input_gathers_five_seconds_a_row_unless_made_with_another_span() {
    assert_eq!(PersonInput::default().span(), PERSON_INPUT_SPAN);
    assert_eq!(PERSON_INPUT_SPAN, Duration::from_secs(5));
    assert_eq!(PersonInput::with_span(SHORT_SPAN).span(), SHORT_SPAN);
}

#[tokio::test]
async fn the_persons_gathered_input_is_written_once_its_span_ends_with_nothing_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let client = persons_client(dir.path(), SHORT_SPAN);
    let output = client.output_dir().unwrap().to_path_buf();
    let clicked = json!({"op": "input", "actions": [{"type": "click", "x": 1, "y": 2}]});
    for _ in 0..3 {
        client
            .record(&clicked, &json!({"ok": true}), Duration::ZERO)
            .await
            .unwrap();
    }
    assert!(activity_log::recent(&output, 5).is_empty());

    tokio::time::sleep(SHORT_SPAN).await;
    let mut rows = Vec::new();
    for _ in 0..300 {
        rows = activity_log::recent(&output, 5);
        if !rows.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].action_count, Some(3));
    assert_eq!(rows[0].actor(), "viewer");
}

#[test]
fn a_command_that_failed_or_timed_out_is_recorded_as_failed() {
    assert!(worked(&json!({"ok": true})));
    assert!(worked(
        &json!({"ok": true, "exitCode": 0, "timedOut": false})
    ));
    assert!(!worked(&json!({"ok": true, "exitCode": 1})));
    assert!(!worked(
        &json!({"ok": true, "exitCode": 0, "timedOut": true})
    ));
    assert!(!worked(&json!({"ok": false})));
}

#[test]
fn a_command_terminated_by_a_signal_is_recorded_as_an_exit_failure() {
    let dir = tempfile::tempdir().unwrap();
    let client = client_in(dir.path()).doing(Doing::new("run"));
    let request = json!({"op": "run", "argv": ["tool"]});
    let reply = json!({"ok": true, "exitCode": null, "timedOut": false, "signal": "SIGTERM"});
    validate_completion(&request, &reply).unwrap();
    client.record_start_blocking(&request).unwrap();
    client
        .record_blocking(&request, &reply, Duration::ZERO)
        .unwrap();

    let rows = crate::activity_log::recent(client.output_dir().unwrap(), 5);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].ok, Some(false));
    assert_eq!(rows[0].code, Some(Failure::Exit));
}

fn display() -> Display {
    Display {
        width: 1280.0,
        height: 720.0,
        scale: 2.0,
    }
}

fn region(x: i32, y: i32, width: i32, height: i32) -> Region {
    Region {
        x,
        y,
        width,
        height,
    }
}

#[test]
fn a_crop_maps_pixels_to_points_from_its_origin() {
    let shown = region(300, 200, 800, 600);
    let ((x, y), scale) = placement(&display(), 400, Some(&shown));

    assert_eq!(((x, y), scale), ((300.0, 200.0), 2.0));
    assert_eq!((x + 10.0 * scale, y + 20.0 * scale), (320.0, 240.0));
    assert_eq!(
        placement(&display(), 1600, Some(&shown)),
        ((300.0, 200.0), 0.5)
    );
}

#[test]
fn a_full_screenshot_starts_at_the_corner_with_todays_scale() {
    assert_eq!(placement(&display(), 2560, None), ((0.0, 0.0), 0.5));
    assert_eq!(placement(&display(), 640, None), ((0.0, 0.0), 2.0));
    assert_eq!(
        cropped(&json!({"ok": true}), None, &display()).unwrap(),
        None
    );
}

#[test]
fn an_older_helper_that_ignores_the_crop_is_refused() {
    let whole = json!({"ok": true, "png": "", "width": 2560, "height": 1440, "scale": 2.0, "offDisplayWindows": []});

    for crop in [Crop::App { pid: 42 }, Crop::Region(region(0, 0, 400, 300))] {
        let refused = cropped(&whole, Some(&crop), &display()).unwrap_err();
        assert_eq!(refused.code, "daemon", "{crop:?}");
        assert!(refused.error.contains("atlas restart"), "{crop:?}");
    }

    let mut null = whole.clone();
    null["region"] = Value::Null;
    assert_eq!(
        cropped(&null, Some(&Crop::App { pid: 42 }), &display())
            .unwrap_err()
            .code,
        "daemon"
    );
}

#[test]
fn a_crop_other_than_the_one_asked_for_is_refused() {
    let reply = |region: Value| json!({"ok": true, "region": region});
    let asked = Crop::Region(region(100, 100, 400, 300));

    let fits = reply(json!({"x": 100, "y": 100, "width": 400, "height": 300}));
    assert_eq!(
        cropped(&fits, Some(&asked), &display()).unwrap(),
        Some(Cropped {
            region: region(100, 100, 400, 300),
            app: Value::Null,
            overlapping: Value::Null,
        })
    );
    let clamped = reply(json!({"x": 100, "y": 100, "width": 100, "height": 50}));
    assert!(cropped(&clamped, Some(&asked), &display()).is_ok());

    for sent in [
        json!({"x": 90, "y": 100, "width": 400, "height": 300}),
        json!({"x": 100, "y": 100, "width": 401, "height": 300}),
        json!({"x": -1, "y": 0, "width": 10, "height": 10}),
        json!({"x": 1200, "y": 0, "width": 100, "height": 10}),
        json!({"x": 0, "y": 0, "width": 0, "height": 10}),
        json!({"x": 100.5, "y": 100, "width": 10, "height": 10}),
        json!({"x": 100, "y": 100}),
        json!("100,100,400,300"),
    ] {
        let refused = cropped(&reply(sent.clone()), Some(&asked), &display()).unwrap_err();
        assert_eq!(refused.code, "protocol", "{sent}");
    }

    let app = Crop::App { pid: 42 };
    let shown = |pid: i64, overlapping: Value| {
        json!({
            "ok": true,
            "region": {"x": 0, "y": 0, "width": 10, "height": 10},
            "app": {"pid": pid, "name": "TextEdit"},
            "overlapping": overlapping,
        })
    };
    let found = cropped(&shown(42, json!([])), Some(&app), &display())
        .unwrap()
        .unwrap();
    assert_eq!(found.app, json!({"pid": 42, "name": "TextEdit"}));
    assert_eq!(found.overlapping, json!([]));
    for (pid, overlapping) in [(43, json!([])), (42, Value::Null), (42, json!({}))] {
        let refused = cropped(&shown(pid, overlapping), Some(&app), &display()).unwrap_err();
        assert_eq!(refused.code, "protocol", "{pid}");
    }
}

#[test]
fn only_the_standard_size_asks_for_one_pixel_per_point() {
    assert_eq!(
        screenshot_request(Size::Standard),
        json!({"op": "screenshot", "maxDimension": SCREENSHOT_SIDE, "perPoint": true})
    );
    assert_eq!(
        screenshot_request(Size::Longest(SCREENSHOT_SIDE)),
        json!({"op": "screenshot", "maxDimension": SCREENSHOT_SIDE})
    );
    assert_eq!(
        screenshot_request(Size::Longest(800)),
        json!({"op": "screenshot", "maxDimension": 800})
    );
    assert_eq!(screenshot_request(Size::Full), json!({"op": "screenshot"}));
}

#[test]
fn a_crop_nobody_asked_for_is_refused() {
    let reply = json!({"ok": true, "region": {"x": 0, "y": 0, "width": 10, "height": 10}});
    let refused = cropped(&reply, None, &display()).unwrap_err();

    assert_eq!(
        (refused.code, refused.error.as_str()),
        (
            Code::Protocol,
            "The helper sent a crop that was not asked for"
        )
    );
}

#[test]
fn a_region_must_meet_the_display() {
    assert!(meets(&region(-50, -50, 200, 200), &display()));
    assert!(meets(&region(1279, 719, 10, 10), &display()));
    assert!(!meets(&region(5000, 5000, 10, 10), &display()));
    assert!(!meets(&region(1280, 0, 10, 10), &display()));
    assert!(!meets(&region(-10, 0, 10, 10), &display()));
}

#[tokio::test]
async fn a_screenshot_is_recorded_as_the_agent_looking_and_a_failed_one_as_failed() {
    let dir = tempfile::tempdir().unwrap();
    let client = client_in(dir.path()).doing(Doing::new("screenshot"));
    assert!(
        client.capture(Size::Standard, None, false).await.is_err(),
        "no helper answers"
    );
    let region = Crop::Region(Region {
        x: 0,
        y: 0,
        width: 400,
        height: 300,
    });
    assert!(
        client
            .capture(Size::Longest(400), Some(&region), true)
            .await
            .is_err()
    );

    let rows = crate::activity_log::recent(client.output_dir().unwrap(), 5);
    assert_eq!(
        (&rows[0].operation, rows[0].ok, rows[0].actor()),
        (&Some(Op::Screenshot), Some(false), "agent")
    );
    assert_eq!(crate::doing::says(&rows[0]), "couldn't take a screenshot");
    let log = fs::read_to_string(client.output_dir().unwrap().join(activity_log::FILE)).unwrap();
    let logged: Vec<Value> = log
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(logged.len(), 2);
    for row in &logged {
        assert_eq!(
            (&row["operation"], &row["ok"], &row["request"]),
            (&json!("screenshot"), &json!(false), &Value::Null),
            "a crop is logged as a plain screenshot"
        );
    }
    assert!(!log.contains("region"), "{log}");
}

#[tokio::test]
async fn a_status_read_without_a_viewer_says_no_viewer_has_control() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = client_in(dir.path());
    client.controller = crate::control::Controller::at(60001, &dir.path().join("state"));

    let status = crate::overview::read(&client).await;
    assert_eq!(status["controlActive"], false);
    assert_eq!(status["overview"]["state"], "notReady", "no helper answers");
    assert_eq!(status["overview"]["label"], "Not ready");
    assert!(
        !status["overview"]["headline"]
            .as_str()
            .unwrap()
            .ends_with('.')
    );
}

#[test]
fn a_failure_keeps_its_fixed_code_and_reads_as_plain_words_but_never_its_message() {
    let dir = tempfile::tempdir().unwrap();
    let click = json!({"op": "input", "actions": [{"type": "click"}]});
    let refused = json!({"ok": false, "code": "elements", "error": "No element is named secret"});
    let odd = json!({"ok": false, "code": "Not A Code: secret"});
    let failed = json!({"ok": true, "exitCode": 2});

    // One client per action: rows of one command read as its newest step.
    for reply in [&refused, &odd, &failed] {
        client_in(dir.path())
            .doing(Doing::new("click").app(Some("Opie")))
            .record_blocking(&click, reply, Duration::ZERO)
            .unwrap();
    }

    let client = client_in(dir.path());
    let directory = client.output_dir().unwrap();
    let log = std::fs::read_to_string(directory.join(activity_log::FILE)).unwrap();
    assert!(!log.contains("secret"), "{log}");
    let rows = crate::activity_log::recent(directory, 5);
    let reasons = rows
        .iter()
        .map(|row| json!(crate::overview::shown(row, 0).reason))
        .collect::<Vec<_>>();
    assert_eq!(
        reasons,
        [
            json!("The command failed"),
            Value::Null,
            json!("Nothing on screen matched")
        ]
    );
}

#[test]
fn recorded_rows_name_the_actor_and_what_was_done_but_never_typed_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = client_in(dir.path()).doing(Doing::new("type").app(Some("TextEdit")));
    let typed = json!({"op": "input", "actions": [{"type": "type", "text": "secret"}]});
    let ok = json!({"ok": true});

    client
        .record_blocking(&typed, &ok, Duration::from_millis(5))
        .unwrap();
    client.doing = None;
    client.actor = Actor::Viewer;
    client
        .record_blocking(&json!({"op": "restart"}), &ok, Duration::ZERO)
        .unwrap();

    let directory = client.output_dir().unwrap();
    let rows = crate::activity_log::recent(directory, 5);
    assert_eq!(
        (rows[0].actor(), &rows[0].operation),
        ("viewer", &Some(Op::Restart))
    );
    assert_eq!((rows[1].actor(), rows[1].action_count), ("agent", Some(1)));
    assert_eq!(
        (rows[1].verb.as_deref(), rows[1].app.as_deref()),
        (Some("type"), Some("TextEdit"))
    );
    assert!(rows[0].verb.is_none(), "a row says only what it knows");

    let log = fs::read_to_string(directory.join(activity_log::FILE)).unwrap();
    assert!(!log.contains("secret"), "no typed text is recorded");
}

#[test]
fn recording_refuses_non_files_without_waiting_for_a_reader() {
    const WORKER: &str = "ATLAS_TEST_METADATA_DIRECTORY";
    if let Some(directory) = std::env::var_os(WORKER) {
        let client = client_in(Path::new(&directory));
        assert!(client.record_start_blocking(&json!({"op": "run"})).is_err());
        return;
    }

    let mut refused = Vec::new();
    for name in [activity_log::LOCK, activity_log::FILE] {
        for kind in [
            "fifo",
            "fifo with reader",
            "directory",
            "link",
            "large link",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let client = client_in(dir.path());
            let output = client.output_dir().unwrap();
            let path = output.join(name);
            let target = output.join("personal-file");
            let contents = if kind == "large link" {
                vec![b'x'; 1024 * 1024 + 1]
            } else {
                b"kept".to_vec()
            };
            fs::write(&target, &contents).unwrap();
            match kind {
                "fifo" | "fifo with reader" => nix::unistd::mkfifo(
                    &path,
                    nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
                )
                .unwrap(),
                "directory" => fs::create_dir(&path).unwrap(),
                "link" | "large link" => std::os::unix::fs::symlink(&target, &path).unwrap(),
                _ => unreachable!(),
            }
            let _reader = (kind == "fifo with reader").then(|| {
                OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NONBLOCK)
                    .open(&path)
                    .unwrap()
            });

            // The synchronous open cannot be interrupted by a Tokio timeout.
            // Confine it to a child we always reap, even on the old blocking path.
            let mut child = crate::test_support::rerun(
                "client::tests::recording_refuses_non_files_without_waiting_for_a_reader",
            )
            .env(WORKER, dir.path())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
            let deadline = Instant::now() + Duration::from_secs(2);
            let success = loop {
                if let Some(status) = child.try_wait().unwrap() {
                    break status.success();
                }
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    child.wait().unwrap();
                    break false;
                }
                std::thread::sleep(Duration::from_millis(10));
            };
            assert_eq!(fs::read(&target).unwrap(), contents);
            refused.push((name, kind, success));
        }
    }
    assert!(
        refused.iter().all(|(_, _, success)| *success),
        "{refused:?}"
    );
}

#[test]
fn recording_appends_privately_and_rotates_a_full_log() {
    let dir = tempfile::tempdir().unwrap();
    let client = client_in(dir.path()).doing(Doing::new("run"));
    let request = json!({"op": "run"});
    client.record_start_blocking(&request).unwrap();
    client
        .record_blocking(&request, &json!({"ok": true}), Duration::ZERO)
        .unwrap();
    let output = client.output_dir().unwrap();
    let path = output.join(activity_log::FILE);
    assert_eq!(fs::read_to_string(&path).unwrap().lines().count(), 2);
    for name in [activity_log::LOCK, activity_log::FILE] {
        assert_eq!(
            fs::metadata(output.join(name)).unwrap().mode() & 0o777,
            0o600
        );
    }

    let full = vec![b'x'; 1024 * 1024 + 1];
    fs::write(&path, &full).unwrap();
    client.record_start_blocking(&request).unwrap();
    assert_eq!(fs::read(output.join(activity_log::PREVIOUS)).unwrap(), full);
    let row: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(row["operation"], "run");
    assert_eq!(row["started"], true);
}

#[test]
fn a_refused_app_list_keeps_its_code() {
    let busy = json!({"ok": false, "code": "busy", "uncertain": false,
        "error": "The helper is serving too many connections"});
    let failed = json!({"ok": false, "code": "apps", "uncertain": false,
        "error": "Running applications could not be listed"});

    for (reply, code) in [(busy, Code::Busy), (failed, Code::Apps)] {
        let error = app_list(reply).unwrap_err();
        assert_eq!((error.code, error.uncertain), (code, false));
    }
}

#[test]
fn a_reply_without_an_app_array_is_never_an_empty_list() {
    let malformed = [
        json!({"ok": true}),
        json!({"ok": true, "apps": null}),
        json!({"ok": true, "apps": {}}),
        json!({"ok": true, "apps": [{"name": "TextEdit"}]}),
        json!({"ok": true, "apps": [{"pid": 0}]}),
        json!({"ok": true, "apps": [{"pid": -1}]}),
        json!({"ok": true, "apps": [{"pid": "5"}]}),
        json!({"ok": true, "apps": [{"pid": 2_147_483_648_u64}]}),
        json!({"ok": true, "apps": [{"pid": 5, "name": 3}]}),
        json!({"ok": true, "apps": [{"pid": 5, "active": "yes"}]}),
        json!({"ok": true, "apps": ["TextEdit"]}),
        json!({"apps": []}),
    ];

    for reply in malformed {
        let error = app_list(reply.clone()).unwrap_err();
        assert_eq!(
            (error.code, error.uncertain),
            (Code::Protocol, false),
            "{reply}"
        );
    }
}

#[test]
fn a_truly_empty_list_is_an_empty_list() {
    let empty = json!({"ok": true, "apps": [], "elapsedMs": 1.5});
    assert_eq!(app_list(empty.clone()).unwrap(), empty);

    let listed = json!({"ok": true, "apps": [{"pid": 812, "name": null, "bundleId": null,
        "executable": "/x", "active": true, "hidden": false, "policy": "regular"}]});
    assert_eq!(app_list(listed.clone()).unwrap(), listed);
}

#[tokio::test]
async fn an_app_list_read_is_never_uncertain() {
    use crate::test_support::{Helper, UID};

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);

    // The stand-in takes the request and hangs up without a reply.
    let error = helper.serve(client.apps(), |_| None).await.unwrap_err();

    assert_eq!((error.code, error.uncertain), (Code::Protocol, false));
}
