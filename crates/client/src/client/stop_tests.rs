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
    let marker = client
        .helper_config()
        .input_socket
        .with_extension("stopped");

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
    person.link.socket_mut().path = person.helper_config().socket_path.clone();
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
    person.link.socket_mut().path = person.helper_config().socket_path.clone();
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
    let marker = person
        .helper_config()
        .input_socket
        .with_extension("stopped");
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
async fn a_stop_on_a_vm_desktop_is_kept_here_and_said_not_to_have_reached_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut person = crate::test_support::vm_client(dir.path(), Some("main"));
    person.actor = Actor::Viewer;
    let reply = person.hold(true).await.unwrap();
    assert_eq!(reply["held"], true);
    assert_eq!(reply["kept"], true);
    assert_eq!(reply["delivered"], false);
    assert!(
        reply["warnings"][0]
            .as_str()
            .unwrap()
            .starts_with("Atlas cannot reach work now: "),
        "{reply}"
    );
    assert!(person.controller.stop_noted());
    assert!(person.stop_marked());
}

#[tokio::test]
async fn a_resume_on_a_vm_desktop_is_refused_and_its_stop_stays() {
    let dir = tempfile::tempdir().unwrap();
    let mut person = crate::test_support::vm_client(dir.path(), None);
    person.actor = Actor::Viewer;
    person.controller.note_stop(true).unwrap();
    let refused = person.hold(false).await.unwrap_err();
    assert_eq!(refused.error, person.unreachable().error);
    assert!(person.controller.stop_noted());
}

#[tokio::test]
async fn a_resume_whose_reply_is_late_keeps_a_newer_stop_whose_caller_died_before_telling_anyone() {
    let (dir, client, listener, _session) = native_client();
    let mut person = client;
    person.actor = Actor::Viewer;
    person.controller.note_stop(true).unwrap();
    let mut resuming = Box::pin(person.hold(false));
    let mut hello = dispatched(&mut resuming, &listener, "hello").await;
    answer(&mut hello, json!({"ok": true, "capabilities": ["hold"]})).await;
    let mut hold = dispatched(&mut resuming, &listener, "hold").await;

    // While the reply is late, another process notes a newer Stop and dies
    // before it reaches the helper or anything else.
    atlas_control::Controller::at(atlas_core::test_support::UID, &dir.path().join("state"))
        .note_stop(true)
        .unwrap();
    answer(&mut hold, json!({"ok": true, "held": false, "kept": true})).await;

    let resumed = resuming.await.unwrap();
    assert_eq!(
        resumed["held"], true,
        "the newer Stop is said to hold: {resumed}"
    );
    assert!(
        resumed["warnings"]
            .to_string()
            .contains("still holds agents"),
        "{resumed}"
    );
    assert!(person.controller.stop_noted(), "the newer Stop stays");
    let mut agent = person.clone();
    agent.actor = Actor::Agent;
    assert_eq!(agent.require_going().await.unwrap_err().code, "stopped");
    drop(dir);
}

#[tokio::test]
async fn a_resume_that_finds_the_stop_already_lifted_says_agents_are_free_without_a_warning() {
    let (dir, client, listener, _session) = native_client();
    let mut person = client;
    person.actor = Actor::Viewer;
    person.controller.note_stop(true).unwrap();
    let mut resuming = Box::pin(person.hold(false));
    let mut hello = dispatched(&mut resuming, &listener, "hello").await;
    answer(&mut hello, json!({"ok": true, "capabilities": ["hold"]})).await;
    let mut hold = dispatched(&mut resuming, &listener, "hold").await;

    // Another Resume lifts the same Stop while this one's reply is late.
    atlas_control::Controller::at(atlas_core::test_support::UID, &dir.path().join("state"))
        .note_stop(false)
        .unwrap();
    answer(&mut hold, json!({"ok": true, "held": false, "kept": true})).await;

    let resumed = resuming.await.unwrap();
    assert_eq!(resumed["held"], false, "{resumed}");
    assert!(
        !resumed["warnings"]
            .to_string()
            .contains("still holds agents"),
        "{resumed}"
    );
    assert!(!person.controller.stop_noted());
    drop(dir);
}

#[tokio::test]
async fn a_resume_without_the_input_service_that_finds_a_newer_stop_says_agents_stay_stopped() {
    let (dir, client, listener, _session) = native_client();
    let mut person = client;
    person.actor = Actor::Viewer;
    person.controller.note_stop(true).unwrap();
    let mut resuming = Box::pin(person.hold(false));
    let hello = dispatched(&mut resuming, &listener, "hello").await;

    // A newer Stop is noted, then the input service goes without answering.
    atlas_control::Controller::at(atlas_core::test_support::UID, &dir.path().join("state"))
        .note_stop(true)
        .unwrap();
    drop(hello);

    let resumed = resuming.await.unwrap();
    assert_eq!(resumed["held"], true, "{resumed}");
    let warnings = resumed["warnings"].to_string();
    assert!(!warnings.contains("free to go on"), "{warnings}");
    assert!(warnings.contains("stopped here all the same"), "{warnings}");
    assert!(warnings.contains("still holds agents"), "{warnings}");
    assert!(person.controller.stop_noted());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vm_whose_guest_has_not_answered_since_it_started_is_said_starting_and_never_held_by_a_stop()
 {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = crate::test_support::vm_through_keeper(dir.path());
    // Started, and its guest still booting: no hello reaches it yet.
    *keeper.away.lock().unwrap() = true;

    let starting = client.require_going().await.unwrap_err();
    keeper.runner.set_power("stopped");
    let unknown = client.require_going().await.unwrap_err();
    *keeper.away.lock().unwrap() = false;
    *keeper.stop.lock().unwrap() = "stopped";
    let held = client.require_going().await.unwrap_err();

    assert_eq!(starting.code, Code::NotReady, "{starting:?}");
    assert_eq!(starting.error, "work is starting; try again in a moment.");
    assert!(!starting.uncertain, "certain: nothing was sent");
    assert_eq!(
        starting.details["next"],
        "Run it again in a moment; nothing was sent."
    );
    assert_eq!(unknown.code, Code::Stopped, "{unknown:?}");
    assert!(
        unknown.error.starts_with("work's own Stop cannot be read"),
        "{unknown:?}"
    );
    assert_eq!(held.code, Code::Stopped, "{held:?}");
    assert!(held.error.starts_with("work holds agents"), "{held:?}");
}
