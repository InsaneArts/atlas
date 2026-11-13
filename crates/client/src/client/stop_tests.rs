use super::*;
use crate::{
    client::tests::{answer, dispatched, native_client},
    test_support::client_in,
};
use std::fs;

#[tokio::test]
async fn a_successful_but_uncertain_resume_keeps_the_local_stop() {
    let (_dir, mut client, listener, _session) = native_client();
    client.actor = Actor::Viewer;
    client.controller.note_stop(true).unwrap();
    let mut caller = Box::pin(client.hold(false));
    let mut hello = dispatched(&mut caller, &listener, "hello").await;
    answer(&mut hello, json!({"ok": true, "capabilities": ["hold"]})).await;
    let mut hold = dispatched(&mut caller, &listener, "hold").await;
    answer(
        &mut hold,
        json!({"ok": true, "held": false, "kept": true, "uncertain": true}),
    )
    .await;
    let result = caller.await;
    assert!(client.controller.stop_noted());
    let error = result.unwrap_err();
    assert_eq!(error.code, Code::Uncertain);
    assert!(error.uncertain);
}

#[tokio::test]
async fn only_the_person_resumes_agents() {
    let dir = tempfile::tempdir().unwrap();
    let client = client_in(dir.path());
    assert_eq!(client.hold(false).await.unwrap_err().code, "stopped");
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
async fn a_stop_returns_ok_with_the_native_problem_when_native_hold_fails() {
    let (_dir, mut person, input, _session) = native_client();
    person.actor = Actor::Viewer;
    // No daemon runs, so none keeps jobs to end.
    person.transport.path = person.config.socket_path.clone();
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
    // No daemon runs, so none keeps jobs to end.
    person.transport.path = person.config.socket_path.clone();
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
