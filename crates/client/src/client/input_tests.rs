use super::*;
use crate::{
    client::tests::{
        another, answer, dispatched, native_client, received, recovery_settles_the_abandoned_claim,
        stoppable, textedit,
    },
    test_support::client_in,
};
use atlas_core::session::{HOLD, SHUTDOWN};
use nix::sys::signal::Signal;

#[test]
fn a_required_check_is_refused_by_an_older_service() {
    let older = json!({"ok": true, "capabilities": [SHUTDOWN, HOLD]});
    let current = json!({"ok": true, "capabilities": [SHUTDOWN, HOLD, FOCUS_APP, FOCUS_DOCK]});
    let undocked = json!({"ok": true, "capabilities": [SHUTDOWN, HOLD, FOCUS_APP]});
    assert_eq!(
        expectation(
            &NativeObservation::decode(&undocked),
            Some(textedit().unwrap())
        )
        .unwrap_err()
        .code,
        "startup"
    );
    let unanswered = json!({"ok": false, "code": "transport"});
    let pid = textedit().unwrap();

    assert_eq!(
        expectation(&NativeObservation::decode(&older), Some(pid.clone()))
            .unwrap_err()
            .code,
        "startup"
    );
    assert_eq!(
        expectation(&NativeObservation::decode(&current), Some(pid.clone())).unwrap(),
        Some(pid.clone())
    );
    // A service that did not answer is refused as unavailable later.
    assert_eq!(
        expectation(&NativeObservation::decode(&unanswered), Some(pid.clone())).unwrap(),
        Some(pid.clone())
    );
    assert_eq!(
        expectation(&NativeObservation::decode(&older), None).unwrap(),
        None
    );
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

#[tokio::test]
async fn a_stop_before_native_acquisition_starts_nothing() {
    let (_dir, mut client, listener, session) = native_client();
    let interrupt = stoppable(&mut client);
    let _ownership = client.controller.acquire(false).unwrap();
    interrupt.ask_for_test(Signal::SIGTERM);

    let actions = json!([{"type": "key", "key": "a"}]);
    let error = client
        .input_in_session(
            actions::Batch::decode(&actions).unwrap(),
            None,
            None,
            None,
            &session,
        )
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
async fn malformed_uncertainty_in_an_acquisition_keeps_its_original_claim() {
    for ok in [false, true] {
        for flag in atlas_core::test_support::invalid_uncertainty_flags() {
            let (_dir, client, listener, session) = native_client();
            let mut caller = Box::pin(client.acquire_input("owner", &session.epoch));
            let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
            let original = client.controller.pending().unwrap();
            assert_eq!(original.len(), 1);
            answer(
                &mut acquire,
                json!({"ok": ok, "code": "busy", "error": "refused", "uncertain": flag}),
            )
            .await;
            let result = caller.await;
            assert_eq!(client.controller.pending().unwrap(), original);
            let error = result.unwrap_err();
            assert_eq!(error.code, Code::Protocol);
            assert!(error.uncertain);
            recovery_settles_the_abandoned_claim(&client);
        }
    }
}

#[tokio::test]
async fn a_successful_but_uncertain_acquisition_keeps_its_original_claim() {
    let (_dir, client, listener, session) = native_client();
    let mut caller = Box::pin(client.acquire_input("owner", &session.epoch));
    let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
    let original = client.controller.pending().unwrap();
    assert_eq!(original.len(), 1);
    answer(&mut acquire, json!({"ok": true, "uncertain": true})).await;
    let result = caller.await;
    assert_eq!(client.controller.pending().unwrap(), original);
    let error = result.unwrap_err();
    assert_eq!(error.code, Code::Uncertain);
    assert_eq!(error.error, "Input service reported an uncertain outcome");
    assert!(error.uncertain);
    recovery_settles_the_abandoned_claim(&client);
}

#[tokio::test]
async fn a_busy_input_service_says_who_holds_control() {
    for (holder, expected) in [
        (json!("person"), Some(Holder::Person)),
        (json!("agent"), Some(Holder::Agent)),
        (json!("someone"), None),
        (Value::Null, None),
    ] {
        for uncertain in [false, true] {
            let (_dir, client, listener, session) = native_client();
            let mut caller = Box::pin(client.acquire_input("owner", &session.epoch));
            let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
            let reply = json!({"ok": false, "code": "busy", "error": "refused", "holder": holder,
                "uncertain": uncertain, "reason": "app", "unread": "slow", "found": {"pid": 812},
                "performed": 2, "partial": true, "id": "job", "task": "task", "state": "running",
                "secret": "unrelated-refusal-secret-4b72"});
            answer(&mut acquire, reply).await;
            let error = caller.await.unwrap_err();
            assert_eq!(error.code, "busy");
            assert_eq!(error.holder, expected, "{holder}");
            let mut kept =
                json!({"ok": false, "code": "busy", "error": "refused", "uncertain": uncertain});
            if let Some(expected) = expected {
                kept["holder"] = json!(expected);
            }
            assert_eq!(serde_json::to_value(error).unwrap(), kept);
            assert_eq!(client.controller.pending().unwrap().is_empty(), !uncertain);
        }
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
        expectation(&NativeObservation::decode(native), Some(expect.clone()))
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

#[tokio::test]
async fn an_input_change_stays_pending_only_while_its_outcome_is_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = client_in(dir.path());
    client.controller = atlas_control::Controller::at(60001, &dir.path().join("state"));

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

    for (index, code) in [Code::Transport, Code::Uncertain].into_iter().enumerate() {
        let unknown = Error::new(code, "The mutation was not acknowledged").uncertain();
        assert!(
            client
                .pending_until_known(async { Err::<(), _>(unknown) })
                .await
                .is_err()
        );
        assert_eq!(client.controller.unresolved().unwrap().len(), index + 1);
    }
}

#[tokio::test]
async fn input_goes_on_beside_a_running_command() {
    if !atlas_core::test_support::isolated(
        "client::input::tests::input_goes_on_beside_a_running_command",
    ) {
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
async fn a_named_click_sends_its_window_and_keeps_a_native_dock_refusal() {
    let (_dir, client, listener, session) = native_client();
    let mut caller = Box::pin(async {
        let _ownership = client.controller.acquire(false)?;
        let actions = json!([{"type":"click","x":20,"y":80}]);
        let expect = Expect::new(812)?.in_window(42)?;
        client
            .input_in_session(
                actions::Batch::decode(&actions)?,
                None,
                None,
                Some(expect),
                &session,
            )
            .await
    });
    let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
    answer(&mut acquire, json!({"ok":true})).await;
    let (mut input, request) = received(&mut caller, &listener, "input").await;
    assert_eq!(request["expect"], json!({"pid":812,"window":42}));
    assert_eq!(request["actions"][0]["type"], "click");
    let refused = json!({"ok":false,"code":"focus-changed","reason":"point","found":{"app":"Dock","pid":2},"error":"The Dock is at that point","performed":0,"partial":false,"uncertain":false});
    answer(&mut input, refused.clone()).await;
    let mut release = dispatched(&mut caller, &listener, "release").await;
    answer(&mut release, json!({"ok":true})).await;
    let reply = caller.await.unwrap();
    for field in [
        "code",
        "reason",
        "found",
        "performed",
        "partial",
        "uncertain",
    ] {
        assert_eq!(reply[field], refused[field]);
    }
    assert!(client.controller.pending().unwrap().is_empty());
}

#[test]
fn preparation_fits_before_the_client_reply_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let client = crate::test_support::client_in(dir.path());
    let budget = atlas_native::native::focus::prepare_budget();
    assert_eq!(budget, std::time::Duration::from_millis(1250));
    assert!(budget < client.input_transport().timeout);
    assert_eq!(
        client.input_transport().timeout,
        std::time::Duration::from_secs(5)
    );
}
