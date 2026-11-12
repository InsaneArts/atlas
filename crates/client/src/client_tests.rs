//! The client's tests that cross its parts: commands run beside each other,
//! restarts and log-outs, and the rows they write. Each part's own tests sit
//! beside it and share the stand-ins here.
use super::*;
use super::{capture::capture_name, record::PersonInput};
use crate::{
    activity_log::{self, Failure, Op},
    test_support::client_in,
};
use atlas_control::{Ownership, SharedLease, interrupt::Interrupt};
use atlas_core::{
    actions::{self, Expect},
    error::Holder,
    protocol::{Crop, Region, SessionPin},
    screen::windows::HitWindow,
    session::{Display, Epoch, FOCUS_APP, FOCUS_DOCK, FOCUS_WINDOW, HOLD, SHUTDOWN},
};
use fs2::FileExt;
use nix::sys::signal::Signal;
use std::{
    fs::{self},
    time::Instant,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
};

/// A native socket and controller confined to a short temporary path.
pub(super) fn native_client() -> (tempfile::TempDir, Client, UnixListener, Session) {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let mut client = client_in(dir.path());
    client.controller = Controller::at(atlas_core::test_support::UID, &dir.path().join("state"));
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
pub(super) async fn dispatched<T: std::fmt::Debug>(
    caller: &mut (impl Future<Output = Result<T>> + Unpin),
    listener: &UnixListener,
    operation: &str,
) -> BufReader<UnixStream> {
    received(caller, listener, operation).await.0
}

/// [`dispatched`], with the request the stand-in received.
pub(super) async fn received<T: std::fmt::Debug>(
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

pub(super) async fn answer(peer: &mut BufReader<UnixStream>, reply: Value) {
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
    let batch = actions::Batch::decode(&actions)?;
    batch.check(&session.epoch.display)?;
    client
        .input_in_session(batch, None, None, None, session)
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
        .input_in_session(
            actions::Batch::decode(&actions)?,
            None,
            None,
            expect,
            session,
        )
        .await
}

pub(super) fn textedit() -> Option<Expect> {
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
async fn named_typing_pins_the_resolved_instance_across_every_native_batch() {
    use crate::{
        agent::{Expected, Target, type_text},
        test_support::Helper,
    };
    use atlas_core::test_support::UID;

    let pid = i64::from(i32::MAX);
    let start = [123, 456];
    for (target, window, windowless) in [
        (Some(Target::Name("Editor".into())), None, false),
        (Some(Target::Pid(pid)), None, false),
        (Some(Target::Name("Editor".into())), None, true),
        (None, Some(42), false),
        (Some(Target::Name("Editor".into())), Some(42), false),
        (Some(Target::Pid(pid)), Some(42), false),
    ] {
        let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let (client, helper) = Helper::client(dir.path(), UID);
        let listener = UnixListener::bind(&client.config.input_socket).unwrap();
        let expected = Expected::new(target, window, None).unwrap().unwrap();
        let text = "x".repeat(150);
        let mut caller = Box::pin(helper.serve(
            type_text(&client, &text, Some(&expected)),
            |request| {
                if request["op"] == "apps" {
                    Some(json!({"ok":true,"starts":{pid.to_string():start},"apps":[{"pid":pid,"name":"Editor","active":true}]}))
                } else {
                    assert_eq!(request["op"], "windows");
                    let mut snapshot = json!({"ok":true,"starts":{pid.to_string():start},"windows":[{
                    "number":42,"pid":pid,"owner":"Editor","title":null,"layer":0,"alpha":1.0,
                    "bounds":{"x":0,"y":0,"width":100,"height":100}
                }]});
                    if windowless { snapshot["windows"] = json!([]); }
                    Some(snapshot)
                }
            },
        ));
        let mut status = verified_status(&[SHUTDOWN, HOLD, FOCUS_APP, FOCUS_DOCK, FOCUS_WINDOW]);
        status["inputService"]["session"]["uid"] = json!(UID);
        let native = &status["inputService"];
        let mut hello = dispatched(&mut caller, &listener, "hello").await;
        answer(&mut hello, native.clone()).await;
        for _ in 0..3 {
            let mut hello = dispatched(&mut caller, &listener, "hello").await;
            answer(&mut hello, native.clone()).await;
            let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
            answer(&mut acquire, json!({"ok":true})).await;
            let (mut input, request) = received(&mut caller, &listener, "input").await;
            let mut pin = json!({"pid":pid,"start":start});
            if let Some(window) = window {
                pin["window"] = json!(window);
            }
            assert_eq!(request["expect"], pin);
            let decoded = Expect::from_request(&request["expect"]).unwrap().unwrap();
            assert_eq!(decoded.app_only().instance(), decoded.instance());
            answer(
                &mut input,
                json!({"ok":true,"performed":request["actions"].as_array().unwrap().len(),"expect":request["expect"]}),
            )
            .await;
            let mut release = dispatched(&mut caller, &listener, "release").await;
            answer(&mut release, json!({"ok":true})).await;
        }
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), caller)
                .await
                .unwrap()
                .unwrap(),
            json!({"ok":true,"typed":150,"batches":3})
        );
        assert!(client.controller.pending().unwrap().is_empty());
    }
}

#[tokio::test]
async fn named_clicks_carry_the_helper_identity_even_when_the_controller_cannot_read_the_process() {
    use crate::{
        agent::{Target, click_element},
        test_support::{Helper, completed},
    };
    use atlas_core::{actions::Button, screen::select::ElementFilter, test_support::UID};
    let pid = i64::from(i32::MAX);
    let start = [123, 456];
    for target in [Target::Name("Editor".into()), Target::Pid(pid)] {
        let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let (client, helper) = Helper::client(dir.path(), UID);
        let listener = UnixListener::bind(&client.config.input_socket).unwrap();
        let filter = ElementFilter {
            name: Some("Save".into()),
            ..ElementFilter::default()
        };
        let mut caller = Box::pin(helper.serve(
            click_element(&client, &target, &filter, Button::Left, 1, &[]),
            |request| {
                if request["op"] == "apps" {
                    return Some(json!({"ok":true,"apps":[{"pid":pid,"name":"Editor"}],"starts":{pid.to_string():start}}));
                }
                if request["op"] == "windows" {
                    return Some(json!({"ok":true,"starts":{pid.to_string():if request["dock"] == true { [124,456] } else { start }},"windows":[{
                        "number":42,"pid":pid,"owner":"Editor","title":null,"layer":0,"alpha":1.0,
                        "bounds":{"x":10,"y":20,"width":80,"height":60}}]}));
                }
                let output = match request["argv"][1].as_str() {
                    Some("elements-info") => json!({"ok":true,"truncated":false,"elements":[{
                        "index":0,"depth":0,"role":"AXButton","title":"Save","enabled":true,
                        "frame":{"x":20,"y":30,"width":20,"height":20},
                        "window":{"x":10,"y":20,"width":80,"height":60}}]}),
                    other => panic!("unexpected helper read: {other:?}"),
                };
                Some(completed(&output.to_string()))
            },
        ));
        let mut status = verified_status(&[SHUTDOWN, HOLD, FOCUS_APP, FOCUS_DOCK, FOCUS_WINDOW]);
        status["inputService"]["session"]["uid"] = json!(UID);
        let native = &status["inputService"];
        for _ in 0..3 {
            let mut hello = dispatched(&mut caller, &listener, "hello").await;
            answer(&mut hello, native.clone()).await;
        }
        let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
        answer(&mut acquire, json!({"ok":true})).await;
        let (mut input, request) = received(&mut caller, &listener, "input").await;
        assert_eq!(
            request["expect"],
            json!({"pid":pid,"start":start,"window":42})
        );
        assert_eq!(request["actions"].as_array().unwrap().len(), 1);
        answer(
            &mut input,
            json!({"ok":true,"performed":1,"expect":request["expect"]}),
        )
        .await;
        let mut release = dispatched(&mut caller, &listener, "release").await;
        answer(&mut release, json!({"ok":true})).await;
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), caller)
                .await
                .unwrap()
                .unwrap()["ok"],
            true
        );
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

#[tokio::test]
async fn verification_reuses_decoded_capability_facts_without_another_helper_request() {
    use base64::{Engine, engine::general_purpose::STANDARD};

    for (offering, valid_proof) in [
        (None, true),
        (Some(SESSION_PIN), true),
        (Some("missing"), true),
        (None, false),
        (Some("missing"), false),
    ] {
        let (_dir, mut client, listener, _) = native_client();
        client.config.helper_uid = 7001;
        client.input_peer_uid = Some(nix::unistd::getuid().as_raw());
        let mut status = verified_status(&[SHUTDOWN, "future-input"]);
        status["daemon"]["capabilities"] = json!([SESSION_PIN, "future-daemon", null, 1]);
        if !valid_proof {
            status["sessionProof"]["onConsole"] = json!(true);
        }
        let mut caller = Box::pin(async {
            match offering {
                Some(capability) => client.verify_offering(capability, "Older daemon").await,
                None => client.verify().await,
            }
        });
        let mut hello = dispatched(&mut caller, &listener, "hello").await;
        answer(&mut hello, status.clone()).await;
        let mut native = dispatched(&mut caller, &listener, "hello").await;
        answer(&mut native, status["inputService"].clone()).await;
        let (mut proof, request) = received(&mut caller, &listener, "run").await;
        assert_eq!(
            request["argv"],
            json!([client.config.identity_probe, "session-info"])
        );
        assert_eq!(request["timeoutMs"], 3000);
        answer(&mut proof, json!({"ok": true, "exitCode": 0, "timedOut": false,
            "events": [{"event": "output", "data": STANDARD.encode(status["sessionProof"].to_string())}]})).await;
        let outcome = tokio::time::timeout(Duration::from_secs(2), caller)
            .await
            .unwrap();
        if !valid_proof {
            assert_eq!(outcome.unwrap_err().code, Code::OnConsole);
        } else if offering == Some("missing") {
            let error = outcome.unwrap_err();
            assert_eq!(error.code, Code::Daemon);
            assert_eq!(error.error, "Older daemon");
        } else {
            let verified = outcome.unwrap();
            assert!(verified.session().input);
            assert!(verified.session().capture);
            assert_eq!(
                verified.pin().unwrap(),
                SessionPin {
                    security_session_id: 42,
                    display_id: 7
                }
            );
            assert!(verified.observation.has_capability("future-daemon"));
            assert!(verified.observation.native.has_capability("future-input"));
            assert!(!verified.observation.native.has_capability("missing"));
        }
        assert!(
            matches!(listener.into_std().unwrap().accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
    }
}

/// Send one key under `check` given `status`, and whether the input
/// service's socket was connected to at all.
///
/// The helper account must differ from the one running the test, so
/// the stand-in socket (served by the test's own account) fails the
/// peer check of any connection: a connection is the evidence that
/// something would have been sent, and none is the evidence that
/// nothing was.
async fn connected(status: &Value, check: Option<Expect>) -> (Result<Value>, bool) {
    let (_dir, mut client, listener, _) = native_client();
    client.config.helper_uid = 7001;
    let actions = json!([{"type": "key", "key": "a"}]);
    let _ownership = client.controller.acquire(false).unwrap();
    let (outcome, accepted) = tokio::join!(
        client.input_with_status(status, actions::Batch::decode(&actions), None, None, check),
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
    let (outcome, sent) = connected(&older, textedit()).await;
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
    for (status, check) in [(&older, None), (&current, textedit())] {
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
    let (outcome, sent) = connected(&undocked, textedit()).await;
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

    let (_, sent) = connected(&undocked, None).await;
    assert!(sent);
}

/// `acquire` once the lock is free: a child another test is starting
/// may hold the lock's file a moment past its release, until it runs
/// its own program. Any other refusal is the answer.
fn once_free(acquire: impl Fn() -> Result<Ownership>) -> Result<Ownership> {
    let held = |error: &Error| {
        error.code == Code::Busy
            && error.holder == Some(atlas_core::error::Holder::Agent)
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

pub(super) fn recovery_settles_the_abandoned_claim(client: &Client) {
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
    if !atlas_core::test_support::isolated(
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
pub(super) const GRACE: Duration = Duration::from_millis(500);

/// Well within [`GRACE`].
pub(super) const SOON: Duration = Duration::from_millis(50);

/// `client`'s controller stopped by a stop of its own, which the test
/// asks.
pub(super) fn stoppable(client: &mut Client) -> Interrupt {
    let interrupt = Interrupt::new_for_test(GRACE);
    client.controller = client.controller.clone().interrupted_by(interrupt.clone());
    interrupt
}

/// `client` with a controller of its own on the same state, as another
/// agent's command has.
pub(super) fn another(client: &Client, dir: &Path) -> Client {
    let mut other = client.clone();
    other.controller = Controller::at(atlas_core::test_support::UID, &dir.join("state"));
    other
}

#[tokio::test]
async fn a_stop_while_acquisition_is_unanswered_waits_for_it_then_posts_nothing() {
    let (_dir, mut client, listener, session) = native_client();
    let interrupt = stoppable(&mut client);
    let mut caller = Box::pin(standalone_input(&client, &session));
    let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
    interrupt.ask_for_test(Signal::SIGTERM);
    answer(&mut acquire, json!({"ok": true})).await;

    let mut release = dispatched(&mut caller, &listener, "release").await;
    answer(&mut release, json!({"ok": true})).await;
    let error = caller.await.unwrap_err();
    assert_eq!(error.code, "interrupted");
    assert!(!error.uncertain, "nothing was posted");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn a_signalled_input_settles_only_after_its_batch_and_release_are_known() {
    if !atlas_core::test_support::isolated(
        "client::tests::a_signalled_input_settles_only_after_its_batch_and_release_are_known",
    ) {
        return;
    }
    for signal in [Signal::SIGTERM, Signal::SIGHUP, Signal::SIGINT] {
        for (batch_reply, release_reply) in [
            (
                Some(json!({"ok": true, "performed": 2})),
                Some(json!({"ok": true})),
            ),
            (Some(json!({"ok": true, "performed": 2})), None),
            (
                Some(json!({"ok": false, "uncertain": true, "error": "unknown"})),
                Some(json!({"ok": true})),
            ),
            (None, None),
        ] {
            tokio::time::timeout(Duration::from_secs(4), async {
                let (dir, mut client, listener, session) = native_client();
                let interrupt = stoppable(&mut client);
                let other = another(&client, dir.path());
                let mut caller = Box::pin(standalone_input(&client, &session));
                let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
                answer(&mut acquire, json!({"ok": true})).await;
                let mut input = dispatched(&mut caller, &listener, "input").await;
                let generation = other.controller.pending().unwrap();
                assert_eq!(generation.len(), 1);

                let known_batch = batch_reply
                    .as_ref()
                    .is_some_and(|reply| reply["ok"] == true);
                let known_release = release_reply.is_some();
                let mut asked = tokio::time::Instant::now();
                if !known_batch || known_release {
                    interrupt.ask_for_test(signal);
                }
                let outcome = if let Some(reply) = batch_reply {
                    answer(&mut input, reply).await;
                    let mut release = dispatched(&mut caller, &listener, "release").await;
                    assert_eq!(other.controller.pending().unwrap(), generation);
                    for recovery in [false, true] {
                        assert_eq!(
                            other.controller.acquire(recovery).unwrap_err().code,
                            Code::Busy
                        );
                    }
                    if known_batch && !known_release {
                        asked = tokio::time::Instant::now();
                        interrupt.ask_for_test(signal);
                    }
                    if let Some(reply) = release_reply {
                        answer(&mut release, reply).await;
                    } else {
                        assert!(tokio::time::timeout(SOON, caller.as_mut()).await.is_err());
                        assert_eq!(other.controller.pending().unwrap(), generation);
                    }
                    tokio::time::timeout(GRACE * 4, caller).await.unwrap()
                } else {
                    let outcome = tokio::time::timeout(GRACE * 4, caller).await.unwrap();
                    let mut line = String::new();
                    assert_eq!(input.read_line(&mut line).await.unwrap(), 0);
                    let (release, _) = listener.accept().await.unwrap();
                    BufReader::new(release).read_line(&mut line).await.unwrap();
                    assert_eq!(
                        serde_json::from_str::<Value>(&line).unwrap()["op"],
                        "release"
                    );
                    outcome
                };

                if known_batch && known_release {
                    assert_eq!(outcome.unwrap()["ok"], true);
                    assert!(other.controller.pending().unwrap().is_empty());
                    assert!(other.controller.acquire(false).is_ok());
                } else {
                    if !known_release {
                        assert!(asked.elapsed() >= GRACE, "cut at the end of the grace");
                        let error = outcome.unwrap_err();
                        assert_eq!(error.code, Code::Interrupted);
                        assert!(error.uncertain);
                    } else {
                        assert!(outcome.as_ref().map_or_else(
                            |error| error.uncertain,
                            |reply| reply["uncertain"] == true,
                        ));
                    }
                    assert_eq!(other.controller.pending().unwrap(), generation);
                    for observer in [other, another(&client, dir.path())] {
                        assert_eq!(
                            observer.controller.acquire(false).unwrap_err().code,
                            Code::Uncertain
                        );
                        assert_eq!(
                            observer
                                .controller
                                .acquire_beside_commands()
                                .unwrap_err()
                                .code,
                            Code::Uncertain
                        );
                    }
                    recovery_settles_the_abandoned_claim(&another(&client, dir.path()));
                }
            })
            .await
            .expect("the caller and fake peer must finish within four seconds");
        }
    }
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
    tokio::time::pause();
    interrupt.ask_for_test(Signal::SIGTERM);
    tokio::time::advance(SOON).await;

    answer(&mut release, json!({"ok": true})).await;
    assert_eq!(caller.await.unwrap()["ok"], true);
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn an_acquisition_still_unanswered_after_the_grace_stays_pending() {
    if !atlas_core::test_support::isolated(
        "client::tests::an_acquisition_still_unanswered_after_the_grace_stays_pending",
    ) {
        return;
    }
    let (dir, mut client, listener, session) = native_client();
    let interrupt = stoppable(&mut client);
    let mut caller = Box::pin(standalone_input(&client, &session));
    let _acquire = dispatched(&mut caller, &listener, "acquire").await;
    interrupt.ask_for_test(Signal::SIGTERM);

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
    let lease: SharedLease = Arc::new(tokio::sync::Mutex::new(atlas_control::Lease::new(
        Duration::from_secs(10),
    )));
    let token = lease.lock().await.acquire(&acting.controller).unwrap();
    let authorizing = lease.lock().await;
    let run = || run_of(&["true"], 1000);
    let mut caller = Box::pin(acting.mutate(run(), Some((&lease, &token)), None, None));
    assert!(
        tokio::time::timeout(SOON, &mut caller).await.is_err(),
        "it waits for the lease"
    );
    interrupt.ask_for_test(Signal::SIGTERM);
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
pub(super) fn running(dir: &Path, mut client: Client) -> (Client, UnixListener, Interrupt) {
    client.transport.path = dir.join("helper.sock");
    let helper = UnixListener::bind(&client.transport.path).unwrap();
    let interrupt = stoppable(&mut client);
    (client.doing(Doing::new("run")), helper, interrupt)
}

#[tokio::test]
async fn a_restart_stopped_before_its_request_settles_only_its_own_claim() {
    let (_dir, mut client, _input, _session) = native_client();
    let interrupt = stoppable(&mut client);
    let earlier = client.controller.quarantine().unwrap();
    let unknown = vec![earlier.generation().to_owned()];
    drop(earlier);
    let claim = client.controller.quarantine().unwrap();
    interrupt.ask_for_test(Signal::SIGTERM);

    let outcome = client.mutate(Mutation::Restart, None, None, None).await;
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
            client.controller = client.controller.waiting_with_spent_allowance(wait);
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
async fn a_successful_but_uncertain_release_keeps_its_original_claim() {
    let (_dir, client, listener, session) = native_client();
    let mut caller = Box::pin(standalone_input(&client, &session));
    let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
    answer(&mut acquire, json!({"ok": true})).await;
    let mut input = dispatched(&mut caller, &listener, "input").await;
    let original = client.controller.pending().unwrap();
    assert_eq!(original.len(), 1);
    answer(&mut input, json!({"ok": true, "performed": 2})).await;
    let mut release = dispatched(&mut caller, &listener, "release").await;
    answer(&mut release, json!({"ok": true, "uncertain": true})).await;
    let result = caller.await;
    assert_eq!(client.controller.pending().unwrap(), original);
    let error = result.unwrap_err();
    assert_eq!(error.code, Code::Uncertain);
    assert!(error.uncertain);
    recovery_settles_the_abandoned_claim(&client);
}

/// A positive native Stop acknowledgement followed by a controlled job reply.
async fn stop_with_job_reply(reply: Value) -> (tempfile::TempDir, Client, Value) {
    let (dir, mut client, input, _session) = native_client();
    client.actor = Actor::Viewer;
    // As in use, commands go to the daemon's socket.
    client.transport.path = client.config.socket_path.clone();
    let daemon = UnixListener::bind(&client.config.socket_path).unwrap();
    let mut caller = Box::pin(client.hold(true));
    let mut hello = dispatched(&mut caller, &input, "hello").await;
    assert!(client.controller.stop_noted());
    answer(&mut hello, json!({"ok": true, "capabilities": ["hold"]})).await;
    let (mut hold, request) = received(&mut caller, &input, "hold").await;
    assert_eq!(request, json!({"op": "hold", "on": true}));
    answer(&mut hold, json!({"ok": true, "held": true, "kept": true})).await;
    let (mut jobs, request) = received(&mut caller, &daemon, "job").await;
    assert_eq!(
        request,
        json!({"op": "job", "action": "stop-all", "reason": "person"})
    );
    answer(&mut jobs, reply).await;
    let result = caller.await.unwrap();
    assert!(client.controller.stop_noted());
    assert_eq!(result["ok"], true);
    assert_eq!(result["held"], true);
    assert_eq!(result["kept"], true);
    assert!(result.get("uncertain").is_none());
    (dir, client, result)
}

#[tokio::test]
async fn an_unconfirmed_stop_all_reply_never_fabricates_an_empty_job_summary() {
    let wrong_variant = json!({"ok": true, "action": "show", "jobs": []});
    assert!(matches!(
        serde_json::from_value::<atlas_core::protocol::JobReply>(wrong_variant.clone()).unwrap(),
        atlas_core::protocol::JobReply::Show(_)
    ));
    for mut reply in [
        wrong_variant,
        json!({"ok": true}),
        json!({"ok": true, "action": "future-stop", "code": "bad-request"}),
        json!({"ok": true, "action": "stop-all", "unresolved": false}),
        json!({"ok": true, "action": "stop-all", "refused": null}),
        json!({"ok": true, "action": "stop-all", "stopping": -1}),
        json!({"ok": true, "action": "stop-all", "uncertain": true}),
    ] {
        reply["extension"] = json!("private-stop-all-reply-canary");
        let (_dir, _client, result) = stop_with_job_reply(reply).await;
        assert!(result.get("jobs").is_none(), "{result}");
        assert_eq!(
            result["warnings"],
            json!(["Atlas could not confirm that every background job was stopped"])
        );
        assert!(!result.to_string().contains("private-stop-all-reply-canary"));
    }
}

#[tokio::test]
async fn a_stop_all_the_helper_could_not_complete_says_so_rather_than_unreachable() {
    for reply in [
        json!({"ok": false, "code": "job", "reason": "unobserved", "uncertain": true,
            "error": "Atlas could not read every background job record; a job may still be running."}),
        json!({"ok": false, "code": "job", "uncertain": false,
            "error": "The helper could not answer about its jobs; restart the daemon to recover"}),
    ] {
        let (_dir, _client, result) = stop_with_job_reply(reply.clone()).await;
        assert!(result.get("jobs").is_none(), "{result}");
        assert_eq!(
            result["warnings"],
            json!(["Atlas could not confirm that every background job was stopped"]),
            "{reply}"
        );
    }
}

#[tokio::test]
async fn confirmed_stop_all_replies_keep_legacy_defaults_and_partial_job_warnings() {
    for (reply, expected, warning) in [
        (
            json!({"ok": true, "action": "stop-all"}),
            Some(
                json!({"stopping": 0, "ended": 0, "refused": [], "unknown": [], "unresolved": []}),
            ),
            None,
        ),
        (
            json!({"ok": true, "action": "stop-all", "stopping": 1, "uncertain": false, "extension": "private-stop-all-reply-canary"}),
            Some(
                json!({"stopping": 1, "ended": 0, "refused": [], "unknown": [], "unresolved": []}),
            ),
            None,
        ),
        (
            json!({"ok": true, "action": "stop-all", "stopping": 1, "ended": 2,
                "refused": [{"id": "000000000001", "reason": "identity", "extension": "private-stop-all-reply-canary"}],
                "unknown": [{"id": "000000000002"}], "unresolved": [{"id": "000000000003"}]}),
            Some(json!({"stopping": 1, "ended": 2,
                "refused": [{"id": "000000000001", "reason": "identity"}],
                "unknown": [{"id": "000000000002"}], "unresolved": [{"id": "000000000003"}]})),
            Some(
                "The helper did not stop 3 background jobs, as it could not tell they are still Atlas's; atlas job list --all shows them",
            ),
        ),
        (
            json!({"ok": false, "code": "bad-request", "error": "private-stop-all-reply-canary"}),
            None,
            None,
        ),
        (
            json!({"ok": false, "code": "bad-request", "error": "private-stop-all-reply-canary", "uncertain": true}),
            None,
            Some("Atlas could not reach the helper to end its background jobs"),
        ),
    ] {
        let (_dir, _client, result) = stop_with_job_reply(reply).await;
        assert_eq!(result.get("jobs"), expected.as_ref());
        assert_eq!(
            result.get("warnings").cloned(),
            warning.map(|warning| json!([warning]))
        );
        assert!(!result.to_string().contains("private-stop-all-reply-canary"));
    }
}

/// The rows `client` wrote to its activity log, oldest first.
pub(super) fn rows(client: &Client) -> Vec<Value> {
    fs::read_to_string(client.output_dir().unwrap().join(activity_log::FILE))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

pub(super) fn labelled(client: &Client) -> Client {
    let mut labelled = client.clone().doing(Doing::new("click"));
    labelled.host_session =
        crate::host_session::HostSession::new(crate::host_session::Host::Claude, "d9031471-5e2a");
    labelled
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

#[tokio::test]
async fn a_stop_holds_on_the_persons_side_while_the_helpers_input_is_down() {
    let dir = tempfile::tempdir().unwrap();
    let mut person = client_in(dir.path());
    person.controller = atlas_control::Controller::at(60001, &dir.path().join("state"));
    person.actor = Actor::Viewer;
    let mut agent = client_in(dir.path());
    agent.controller = atlas_control::Controller::at(60001, &dir.path().join("state"));

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
        .mutate(run_of(&["true"], 1000), None, None, None)
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
async fn a_local_stop_cancels_a_running_command_and_refuses_its_next_dispatch() {
    let (dir, mut client, input, _session) = native_client();
    drop(input);
    client.transport.path = dir.path().join("helper.sock");
    let helper = UnixListener::bind(&client.transport.path).unwrap();
    let mut person = client.clone();
    person.actor = Actor::Viewer;
    // The person's Stop finds no daemon keeping jobs to end.
    person.transport.path = person.config.socket_path.clone();
    let acting = client.doing(Doing::new("run"));
    let run = || run_of(&["true"], 1000);
    let ownership = acting.controller.acquire(false).unwrap();
    let mut caller = Box::pin(acting.mutate(run(), None, None, None));
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

    let error = acting.mutate(run(), None, None, None).await.unwrap_err();
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
pub(super) async fn waits<T: std::fmt::Debug>(
    caller: &mut (impl Future<Output = Result<T>> + Unpin),
) {
    assert!(
        tokio::time::timeout(SOON, caller).await.is_err(),
        "it still waits for its reply"
    );
}

/// A command an agent started with `run`, which the stand-in leaves
/// unanswered until the test answers it.
pub(super) fn command() -> Mutation<'static> {
    run_of(&["sleep", "30"], 60_000)
}

/// A run of `argv` the helper stops after `millis` milliseconds.
pub(super) fn run_of(argv: &[&str], millis: u64) -> Mutation<'static> {
    Mutation::Run {
        argv: argv.iter().map(|&argument| argument.to_owned()).collect(),
        timeout: Duration::from_millis(millis),
        pin: None,
    }
}

#[tokio::test]
async fn a_run_refused_before_it_is_sent_settles_under_control() {
    if !atlas_core::test_support::isolated(
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
    let lease: SharedLease = Arc::new(tokio::sync::Mutex::new(atlas_control::Lease::new(
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
    interrupt.ask_for_test(Signal::SIGTERM);
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
    if !atlas_core::test_support::isolated(
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
async fn a_lost_or_uncertain_run_reply_is_unknown_before_its_row_is_written() {
    if !atlas_core::test_support::isolated(
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
async fn a_local_stop_succeeds_and_cancels_a_running_command_when_native_hold_fails() {
    let (dir, mut client, input, _session) = native_client();
    client.transport.path = dir.path().join("helper.sock");
    let helper = UnixListener::bind(&client.transport.path).unwrap();
    let mut person = client.clone();
    person.actor = Actor::Viewer;
    // The person's Stop finds no daemon keeping jobs to end.
    person.transport.path = person.config.socket_path.clone();
    let acting = client.doing(Doing::new("run"));
    let ownership = acting.controller.acquire(false).unwrap();
    let mut caller = Box::pin(acting.mutate(run_of(&["true"], 1000), None, None, None));
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

/// A viewer's client for `dir`, gathering the person's input as a viewer
/// does.
/// A viewer's client in `dir` whose person's input gathers a `span`.
pub(super) fn persons_client(dir: &Path, span: Duration) -> Client {
    let mut client = client_in(dir);
    client.actor = Actor::Viewer;
    client.person_input = Some(PersonInput::with_span(span));
    client
}

#[test]
fn a_failed_rollover_keeps_both_spans_in_their_original_order() {
    let dir = tempfile::tempdir().unwrap();
    let client = persons_client(dir.path(), Duration::ZERO);
    let output = client.output_dir().unwrap();
    fs::create_dir_all(output).unwrap();
    let blocked = output.join(activity_log::FILE);
    fs::create_dir(&blocked).unwrap();
    let input =
        |count: usize| json!({"op": "input", "actions": vec![json!({"type": "click"}); count]});
    client.record_for_test(&input(1), &json!({"ok": true}));
    assert!(
        client
            .record_blocking(&input(2), &json!({"ok": true}), Duration::ZERO)
            .is_err()
    );
    fs::remove_dir(blocked).unwrap();
    client.person_input.as_ref().unwrap().flush().unwrap();
    let text = fs::read_to_string(output.join(activity_log::FILE)).unwrap();
    let counts = text
        .lines()
        .map(|line| {
            serde_json::from_str::<Value>(line).unwrap()["actionCount"]
                .as_u64()
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(counts, [1, 2]);
}

#[test]
fn a_command_terminated_by_a_signal_is_recorded_as_an_exit_failure() {
    let dir = tempfile::tempdir().unwrap();
    let client = client_in(dir.path()).doing(Doing::new("run"));
    let request = json!({"op": "run", "argv": ["tool"]});
    let reply = json!({"ok": true, "exitCode": null, "timedOut": false, "signal": "SIGTERM"});
    run_of(&["tool"], 1000).validate(&reply).unwrap();
    client.record_start_blocking(&request).unwrap();
    client
        .record_blocking(&request, &reply, Duration::ZERO)
        .unwrap();

    let rows = crate::activity_log::recent(client.output_dir().unwrap(), 5);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].ok, Some(false));
    assert_eq!(rows[0].code, Some(Failure::Exit));
}

pub(super) fn display() -> Display {
    Display {
        width: 1280.0,
        height: 720.0,
        scale: 2.0,
    }
}

pub(super) fn region(x: i32, y: i32, width: i32, height: i32) -> Region {
    Region {
        x,
        y,
        width,
        height,
    }
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
    client.controller = atlas_control::Controller::at(60001, &dir.path().join("state"));

    let status = crate::overview::read(&client).await.value;
    assert_eq!(status["controlActive"], false);
    assert!(status.get("consoleName").is_none());
    assert!(status.get("idleTime").is_none());
    assert_eq!(status["overview"]["state"], "notReady", "no helper answers");
    assert_eq!(status["overview"]["label"], "Not ready");
    assert!(
        !status["overview"]["headline"]
            .as_str()
            .unwrap()
            .ends_with('.')
    );
}

#[tokio::test]
async fn an_app_capture_records_its_identity_but_never_caches_window_titles() {
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;
    use base64::{Engine, engine::general_purpose::STANDARD};
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let client = client.doing(Doing::new("screenshot"));
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend(10_u32.to_be_bytes());
    png.extend(10_u32.to_be_bytes());
    png.resize(33, 0);
    let saved = helper
        .serve(
            client.capture(Size::Full, Some(&Crop::App { pid: 812 }), false),
            |request| {
                assert_eq!(request["op"], "screenshot");
                Some(
                    json!({"ok": true, "png": STANDARD.encode(&png), "width": 10, "height": 10,
                "region": {"x": 0, "y": 0, "width": 10, "height": 10},
                "app": {"pid": 812, "name": "TextEdit", "bundleId": "com.apple.TextEdit"},
                "overlapping": [{"title": "PRIVATE DOCUMENT TITLE"}]}),
                )
            },
        )
        .await
        .unwrap();
    assert_eq!(saved["app"]["bundleId"], "com.apple.TextEdit");
    let row = activity_log::recent(client.output_dir().unwrap(), 1).remove(0);
    assert_eq!(row.app.as_deref(), Some("TextEdit"));
    assert_eq!(row.bundle_id.as_deref(), Some("com.apple.TextEdit"));
    assert_eq!(row.label, None);
    for entry in fs::read_dir(client.output_dir().unwrap()).unwrap() {
        let path = entry.unwrap().path();
        if matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("json" | "jsonl")
        ) {
            assert!(
                !fs::read_to_string(path)
                    .unwrap()
                    .contains("PRIVATE DOCUMENT TITLE")
            );
        }
    }
}

#[tokio::test]
async fn a_lost_preparation_reply_stays_pending_after_acknowledged_release() {
    let (dir, client, listener, session) = native_client();
    let expect = Expect::new(812).unwrap().in_window(42).unwrap();
    let mut caller = Box::pin(checked_input(&client, &session, Some(expect)));
    let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
    answer(&mut acquire, json!({"ok":true})).await;
    let (input, request) = received(&mut caller, &listener, "input").await;
    assert_eq!(request["expect"], json!({"pid":812,"window":42}));
    drop(input);
    // The service is asked what the input came to, and does not know.
    let (mut asked, question) = received(&mut caller, &listener, "outcome").await;
    assert_eq!(question["change"], request["change"]);
    answer(&mut asked, json!({"ok": true, "outcome": "unknown"})).await;
    let mut release = dispatched(&mut caller, &listener, "release").await;
    answer(&mut release, json!({"ok":true})).await;
    assert!(caller.await.unwrap_err().uncertain);
    let other = another(&client, dir.path());
    assert_eq!(other.controller.pending().unwrap().len(), 1);
    assert_eq!(
        other.controller.acquire(false).unwrap_err().code,
        Code::Uncertain
    );
}

/// Input whose reply is lost, driven until the service has been asked what
/// it came to: the caller, and the question's connection.
async fn input_with_a_lost_reply<'a>(
    client: &'a Client,
    listener: &UnixListener,
    session: &'a Session,
) -> (
    std::pin::Pin<Box<impl Future<Output = Result<Value>> + 'a>>,
    BufReader<UnixStream>,
) {
    let mut caller = Box::pin(checked_input(client, session, None));
    let mut acquire = dispatched(&mut caller, listener, "acquire").await;
    answer(&mut acquire, json!({"ok":true})).await;
    let (input, request) = received(&mut caller, listener, "input").await;
    drop(input);
    let (asked, question) = received(&mut caller, listener, "outcome").await;
    assert_eq!(question["change"], request["change"]);
    assert_eq!(question.as_object().unwrap().len(), 2, "{question}");
    (caller, asked)
}

#[tokio::test]
async fn a_lost_reply_the_service_remembers_settles_its_input() {
    let (_dir, client, listener, session) = native_client();
    let (mut caller, mut asked) = input_with_a_lost_reply(&client, &listener, &session).await;
    answer(
        &mut asked,
        json!({"ok": true, "outcome": "done", "reply": {"ok": true, "performed": 1}}),
    )
    .await;
    let mut release = dispatched(&mut caller, &listener, "release").await;
    answer(&mut release, json!({"ok":true})).await;

    let reply = caller.await.unwrap();
    assert_eq!(reply["performed"], 1);
    assert!(reply["elapsedMs"].is_number());
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn a_remembered_reply_proves_no_more_than_one_that_arrived() {
    // It ran, and the service itself cannot say how far.
    let cut = json!({"ok": false, "code": "input", "error": "cut", "uncertain": true});
    // It does not confirm the one key that was asked for.
    let unconfirmed = json!({"ok": true, "performed": 0});
    for (remembered, said) in [
        (cut, "cut"),
        (unconfirmed, "Helper did not confirm operation completion"),
    ] {
        let (dir, client, listener, session) = native_client();
        let (mut caller, mut asked) = input_with_a_lost_reply(&client, &listener, &session).await;
        answer(
            &mut asked,
            json!({"ok": true, "outcome": "done", "reply": remembered}),
        )
        .await;
        let mut release = dispatched(&mut caller, &listener, "release").await;
        answer(&mut release, json!({"ok":true})).await;

        // What is said is the remembered reply's own, never the loss's.
        let (uncertain, words) = match caller.await {
            Ok(reply) => (
                reply["uncertain"] == true,
                reply["error"]
                    .as_str()
                    .expect("a remembered refusal says why")
                    .to_owned(),
            ),
            Err(error) => (error.uncertain, error.error),
        };
        assert!(uncertain, "{said}");
        assert_eq!(words, said);
        let other = another(&client, dir.path());
        assert_eq!(other.controller.pending().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn input_still_under_way_in_the_service_stays_pending() {
    let (dir, client, listener, session) = native_client();
    let (mut caller, mut asked) = input_with_a_lost_reply(&client, &listener, &session).await;
    answer(&mut asked, json!({"ok": true, "outcome": "running"})).await;
    let mut release = dispatched(&mut caller, &listener, "release").await;
    answer(&mut release, json!({"ok":true})).await;

    assert!(caller.await.unwrap_err().uncertain);
    let other = another(&client, dir.path());
    assert_eq!(other.controller.pending().unwrap().len(), 1);
}

#[tokio::test]
async fn checked_window_input_echoes_the_exact_target_without_an_extra_request() {
    let (_dir, client, listener, session) = native_client();
    let expect = Expect::new(812)
        .unwrap()
        .in_window(42)
        .unwrap()
        .in_field("Email")
        .unwrap();
    let mut caller = Box::pin(checked_input(&client, &session, Some(expect.clone())));
    let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
    answer(&mut acquire, json!({"ok":true})).await;
    let (mut input, request) = received(&mut caller, &listener, "input").await;
    assert_eq!(request["expect"], json!(expect));
    let reply = json!({"ok":true,"performed":1,"expect":expect,"field":{"role":"AXTextField"}});
    answer(&mut input, reply.clone()).await;
    let mut release = dispatched(&mut caller, &listener, "release").await;
    answer(&mut release, json!({"ok":true})).await;
    let mut actual = caller.await.unwrap();
    assert!(actual["elapsedMs"].is_number());
    actual.as_object_mut().unwrap().remove("elapsedMs");
    assert_eq!(actual, reply);
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn a_focus_write_with_an_unknown_outcome_keeps_its_claim_after_cleanup() {
    let (dir, client, listener, session) = native_client();
    let mut caller = Box::pin(checked_input(&client, &session, textedit()));
    let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
    answer(&mut acquire, json!({"ok":true})).await;
    let mut input = dispatched(&mut caller, &listener, "input").await;
    answer(&mut input, json!({"ok":false,"code":"uncertain","error":"The app did not acknowledge the focus request","performed":0,"partial":false,"uncertain":true})).await;
    let mut release = dispatched(&mut caller, &listener, "release").await;
    answer(&mut release, json!({"ok":true})).await;
    let reply = caller.await.unwrap();
    assert_eq!(reply["performed"], 0);
    assert_eq!(reply["uncertain"], true);
    assert_eq!(
        another(&client, dir.path())
            .controller
            .pending()
            .unwrap()
            .len(),
        1
    );
}

impl Client {
    pub(crate) fn remember_windows(&self, windows: &[HitWindow], epoch: &Epoch) {
        if let Some(session) = &self.host_session
            && let Ok(output) = self.output_dir()
        {
            crate::last_capture::remember_windows(output, session.key(), Some(windows), epoch)
                .unwrap();
        }
    }

    pub(crate) fn remembered_windows(&self) -> Option<crate::last_capture::Observation> {
        crate::last_capture::remembered_windows(
            &self.config.output_dir,
            self.host_session.as_ref()?.key(),
        )
        .unwrap()
    }
}

#[test]
fn independent_proofs_require_the_exact_current_challenge_before_session_validation() {
    let proof = json!({"ok":true,"uid":502,"parentPID":123,"onConsole":false});
    let reply = json!({"ok":true,"challenge":"fresh","proof":proof});
    assert_eq!(Client::challenged_proof(&reply, "fresh").unwrap(), proof);
    for bad in [
        json!({"ok":true,"challenge":"old","proof":proof}),
        json!({"ok":true,"proof":proof}),
        json!({"ok":false,"challenge":"fresh","proof":proof}),
        json!({"ok":true,"challenge":"fresh","proof":"not a proof"}),
    ] {
        assert_eq!(
            Client::challenged_proof(&bad, "fresh").unwrap_err().code,
            Code::Identity
        );
    }
}

#[tokio::test]
async fn independent_proofs_use_fresh_challenges_and_older_helpers_keep_the_original_probe() {
    let directory = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(directory.path());
    client.transport.peer_uid = nix::unistd::getuid().as_raw();
    std::fs::create_dir_all(client.transport.path.parent().unwrap()).unwrap();
    let listener = UnixListener::bind(&client.transport.path).unwrap();
    let source = json!({"ok":true,"uid":502,"parentPID":123,"read":true});
    let proof = source.clone();
    let server = tokio::spawn(async move {
        let mut nonces = Vec::new();
        for round in 0..3 {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = BufReader::new(socket);
            let mut line = String::new();
            socket.read_line(&mut line).await.unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            let reply = if round < 2 {
                assert_eq!(request["op"], "session-proof");
                let nonce = request["challenge"].as_str().unwrap().to_owned();
                assert!(atlas_core::protocol::valid_challenge(&nonce));
                nonces.push(nonce.clone());
                json!({"ok":true,"challenge":nonce,"proof":proof})
            } else {
                assert_eq!(request["op"], "run");
                assert_eq!(request["argv"][1], "session-info");
                crate::test_support::completed(&proof.to_string())
            };
            socket
                .get_mut()
                .write_all(format!("{reply}\n").as_bytes())
                .await
                .unwrap();
        }
        assert_ne!(
            nonces[0], nonces[1],
            "each proof needs its own unpredictable challenge"
        );
    });
    let capability = json!({"capabilities":[atlas_core::protocol::SESSION_PROOF]});
    for daemon in [&capability, &capability, &json!({"capabilities":[]})] {
        assert_eq!(client.session_proof(daemon).await.unwrap(), source);
    }
    server.await.unwrap();
}

#[tokio::test]
async fn identity_reads_input_and_challenged_proof_together_but_waits_for_both() {
    for mismatch in [false, true] {
        let directory = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let mut client = crate::test_support::client(directory.path());
        client.transport.peer_uid = nix::unistd::getuid().as_raw();
        client.input_peer_uid = Some(nix::unistd::getuid().as_raw());
        std::fs::create_dir_all(client.transport.path.parent().unwrap()).unwrap();
        let daemon = UnixListener::bind(&client.transport.path).unwrap();
        let input = UnixListener::bind(&client.config.input_socket).unwrap();
        let mut status = verified_status(&[]);
        status["daemon"]["capabilities"] = json!([atlas_core::protocol::SESSION_PROOF]);
        let mut caller = Box::pin(client.identity());
        let mut hello = dispatched(&mut caller, &daemon, "hello").await;
        answer(&mut hello, status.clone()).await;

        // Neither service replies until both requests have arrived. Sequential
        // probing would deadlock here; there is no timing-based speed assertion.
        let mut input_hello = dispatched(&mut caller, &input, "hello").await;
        let (mut proof, request) = received(&mut caller, &daemon, "session-proof").await;
        let mut native = status["sessionProof"].clone();
        if mismatch {
            native["displayID"] = json!(8);
        }
        answer(
            &mut proof,
            json!({"ok":true,"challenge":request["challenge"],"proof":native}),
        )
        .await;
        tokio::select! {
            premature = &mut caller => panic!("identity returned without input: {premature:?}"),
            _ = tokio::task::yield_now() => {},
        }
        answer(&mut input_hello, status["inputService"].clone()).await;
        let result = caller.await;
        if mismatch {
            let session = result.unwrap();
            assert_eq!(session.epoch.display_id, 8);
            assert!(
                !session.input,
                "mismatched input proof cannot authorize input"
            );
        } else {
            let session = result.unwrap();
            assert_eq!(session.epoch.display_id, 7);
            assert!(session.input);
        }
    }
}

#[tokio::test]
async fn an_unverified_daemon_never_starts_either_independent_read() {
    for (uid, on_console) in [(7002, false), (7001, true)] {
        let directory = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let mut client = crate::test_support::client(directory.path());
        client.transport.peer_uid = nix::unistd::getuid().as_raw();
        client.input_peer_uid = Some(nix::unistd::getuid().as_raw());
        std::fs::create_dir_all(client.transport.path.parent().unwrap()).unwrap();
        let daemon = UnixListener::bind(&client.transport.path).unwrap();
        let input = UnixListener::bind(&client.config.input_socket).unwrap();
        let mut status = verified_status(&[]);
        status["user"]["uid"] = json!(uid);
        status["session"]["onConsole"] = json!(on_console);
        status["daemon"]["capabilities"] = json!([atlas_core::protocol::SESSION_PROOF]);
        let mut caller = Box::pin(client.identity());
        let mut hello = dispatched(&mut caller, &daemon, "hello").await;
        answer(&mut hello, status).await;
        assert!(caller.await.is_err());
        for listener in [&daemon, &input] {
            assert!(
                tokio::time::timeout(Duration::from_millis(10), listener.accept())
                    .await
                    .is_err(),
                "no native probe may be sent before the daemon gate passes"
            );
        }
    }
}

/// The input service's refusal to ask the person, as it answers it: with
/// the message the person would see.
fn refusal_to_ask(canary: &str) -> Value {
    let key = json!({"kind": "site", "id": "slack.com", "browser": "com.google.Chrome"});
    json!({
        "ok": false,
        "code": "waiting-for-person",
        "error": "slack.com is signed in; Atlas asked the person before sending and they haven't answered.",
        "ask": {
            "account": "slack.com",
            "accountKey": key,
            "what": "send-key",
            "batch": "b1",
            "bound": {"app": "com.google.Chrome", "site": "app.slack.com"},
            "action": "a1",
            "bind": {"accountKey": key, "action": "a1", "value": "d1"},
            "message": {"preview": canary, "digest": "d1", "length": canary.len()},
            "once": true,
        },
    })
}

/// A stand-in input service that grants and releases, and answers each input with the next of `replies`,
/// until it has released after the second input.
fn answering_input(
    listener: UnixListener,
    replies: Vec<Value>,
) -> tokio::task::JoinHandle<Vec<Value>> {
    tokio::spawn(async move {
        let mut replies = replies.into_iter();
        let mut inputs = Vec::new();
        while let Ok(Ok((peer, _))) =
            tokio::time::timeout(Duration::from_secs(15), listener.accept()).await
        {
            let mut peer = BufReader::new(peer);
            let mut line = String::new();
            peer.read_line(&mut line).await.unwrap();
            // A picture for the person's question, asked of the daemon's
            // socket this stand-in also is, isn't served.
            let Ok(request) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let released = request["op"] == "release";
            let reply = match request["op"].as_str() {
                Some("input") => {
                    inputs.push(request);
                    replies
                        .next()
                        .unwrap_or(json!({"ok": true, "performed": 1}))
                }
                _ => json!({"ok": true}),
            };
            answer(&mut peer, reply).await;
            if released && inputs.len() >= 2 {
                break;
            }
        }
        inputs
    })
}

#[tokio::test]
async fn a_refusal_to_ask_through_the_real_input_path_waits_and_tells_the_agent_nothing_private() {
    const CANARY: &str = "the release notes canary 9f2e";
    let (_dir, client, listener, session) = native_client();
    let asks = client.controller.asks();
    let (person, seen) = std::sync::mpsc::channel();
    let client = Client {
        controller: client
            .controller
            .clone()
            .waiting(Duration::from_millis(150)),
        // Read the person's stored card after publication, before the wait
        // can end and take its preview down, even if the deadline passed.
        ask_shown: Some(Arc::new(move |id| {
            let ask = asks
                .open(atlas_core::time::now_ms())
                .into_iter()
                .find(|ask| ask.id == id)
                .expect("the published question is shown");
            person
                .send(ask.question["message"]["preview"].clone())
                .unwrap();
        })),
        ..client
    };
    let service = answering_input(listener, vec![refusal_to_ask(CANARY)]);
    let outcome = crate::asking::after_asking(&client, async || {
        let _ownership = client.controller.acquire(false)?;
        let actions = json!([{"type": "key", "key": "return"}]);
        client
            .input_in_session(
                actions::Batch::decode(&actions)?,
                None,
                None,
                None,
                &session,
            )
            .await
    })
    .await;
    service.abort();

    let shown = match &outcome {
        Ok(reply) => reply.to_string(),
        Err(error) => serde_json::to_string(error).unwrap(),
    };
    assert!(shown.contains("waiting-for-person"), "{shown}");
    assert!(
        !shown.contains(CANARY),
        "the message never reaches the agent: {shown}"
    );
    assert!(
        !shown.contains("app.slack.com"),
        "nor where it lands: {shown}"
    );
    assert_eq!(
        seen.try_recv().unwrap(),
        CANARY,
        "the person sees the message while the command waits"
    );
    let asks = client.controller.asks().open(atlas_core::time::now_ms());
    assert_eq!(asks.len(), 1, "the question stays up for a rerun");
    assert!(
        asks[0].question["message"]["preview"].is_null(),
        "its message doesn't outlive the wait"
    );
}

/// Run a request of a key and Return through the real input path, the
/// input service answering it with `reply`, while the person answers what
/// is asked with `answer`.
async fn stopped_partway(
    reply: Value,
    answer: Option<atlas_control::asks::Answer>,
) -> (Result<Value>, usize, Client, tempfile::TempDir) {
    let (dir, client, listener, session) = native_client();
    let client = Client {
        controller: client.controller.clone().waiting(Duration::from_secs(5)),
        ..client
    };
    let service = answering_input(listener, vec![reply]);
    let asks = client.controller.asks();
    let person = std::thread::spawn(move || {
        let Some(answer) = answer else {
            return;
        };
        for _ in 0..500 {
            if let Some(ask) = asks.open(atlas_core::time::now_ms()).into_iter().next() {
                asks.answer(&ask.id, answer, atlas_core::time::now_ms())
                    .unwrap();
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    let mut attempts = 0;
    let outcome = crate::asking::after_asking(&client, async || {
        attempts += 1;
        let _ownership = client.controller.acquire(false)?;
        let actions = json!([{"type": "key", "key": "a"}, {"type": "key", "key": "return"}]);
        client
            .input_in_session(
                actions::Batch::decode(&actions)?,
                None,
                None,
                None,
                &session,
            )
            .await
    })
    .await;
    person.join().unwrap();
    service.abort();
    (outcome, attempts, client, dir)
}

fn shown(outcome: &Result<Value>) -> String {
    match outcome {
        Ok(reply) => reply.to_string(),
        Err(error) => serde_json::to_string(error).unwrap(),
    }
}

#[tokio::test]
async fn a_request_stopped_before_a_later_action_asks_about_it_alone_and_never_runs_again() {
    let mut stopped = refusal_to_ask("the stopped message");
    stopped["performed"] = json!(1);
    stopped["partial"] = json!(false);
    stopped["uncertain"] = json!(false);
    let (outcome, attempts, client, _dir) =
        stopped_partway(stopped, Some(atlas_control::asks::Answer::Once)).await;
    assert_eq!(attempts, 1, "never resubmitted");
    let shown = shown(&outcome);
    assert!(shown.contains("\"performed\":1"), "{shown}");
    assert!(
        shown.contains("send only the actions after the first 1"),
        "{shown}"
    );
    assert!(!shown.contains("the stopped message"), "{shown}");
    let asks = client.controller.asks();
    let now = atlas_core::time::now_ms();
    assert!(asks.open(now).is_empty(), "the question came down");
    let taken = asks
        .take_permits("terminal", None, &["a1".to_owned()], now)
        .unwrap();
    assert_eq!(taken.len(), 1, "a permit for the stopped action");
    assert_eq!(taken[0].value.as_deref(), Some("d1"), "and its message");
}

#[tokio::test]
async fn a_stop_inside_an_action_that_sent_part_of_itself_asks_nothing_and_says_to_look() {
    let mut inside = refusal_to_ask("partial");
    inside["performed"] = json!(0);
    inside["partial"] = json!(true);
    inside["uncertain"] = json!(true);
    let (outcome, attempts, client, _dir) = stopped_partway(inside, None).await;
    assert_eq!(attempts, 1);
    let shown = shown(&outcome);
    assert!(shown.contains("Look at the field"), "{shown}");
    let asks = client.controller.asks();
    let now = atlas_core::time::now_ms();
    assert!(asks.open(now).is_empty(), "no question");
    assert!(
        asks.take_permits("terminal", None, &["a1".to_owned()], now)
            .unwrap()
            .is_empty(),
        "no permit"
    );
}

#[tokio::test]
async fn an_allow_after_the_observation_freshness_limit_still_runs_through_a_fresh_look() {
    let (_dir, client, listener, session) = native_client();
    let client = Client {
        controller: client.controller.clone().waiting(Duration::from_secs(30)),
        ..client
    };
    let service = answering_input(listener, vec![refusal_to_ask("fresh")]);
    let asks = client.controller.asks();
    let (first_tx, first_rx) = std::sync::mpsc::channel::<Instant>();
    // The person answers once the first look is past its freshness limit.
    let person = std::thread::spawn(move || {
        let first = first_rx.recv().unwrap();
        let ask = loop {
            if let Some(ask) = asks.open(atlas_core::time::now_ms()).into_iter().next() {
                break ask;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        std::thread::sleep(
            (first + Duration::from_millis(8_200)).saturating_duration_since(Instant::now()),
        );
        asks.answer(
            &ask.id,
            atlas_control::asks::Answer::Task,
            atlas_core::time::now_ms(),
        )
        .unwrap();
    });
    let mut looks = Vec::new();
    let reply = crate::asking::after_asking(&client, async || {
        // Each attempt takes control and looks at its target itself.
        let _ownership = client.controller.acquire(false)?;
        let looked = Instant::now();
        if looks.is_empty() {
            first_tx.send(looked).unwrap();
        }
        looks.push(looked);
        let actions = json!([{"type": "key", "key": "return"}]);
        client
            .input_in_session(
                actions::Batch::decode(&actions)?,
                Some((&session.epoch, looked)),
                None,
                None,
                &session,
            )
            .await
    })
    .await;
    person.join().unwrap();
    let inputs = service.await.unwrap();
    assert_eq!(reply.unwrap()["ok"], true, "the allowed attempt ran");
    assert_eq!(inputs.len(), 2, "and reached the input service");
    assert!(
        looks[1].duration_since(looks[0]) >= Duration::from_secs(8),
        "the first look was past its limit when the person answered"
    );
}
