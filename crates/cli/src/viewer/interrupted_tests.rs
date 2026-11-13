use super::*;
use crate::viewer::http;
use atlas_client::{
    sign_ins::{self, Listed},
    test_support::Helper,
};
use atlas_core::test_support::UID;
use std::path::Path;

fn github() -> PrivateHold {
    PrivateHold {
        site: "github.com".to_owned(),
        kind: HoldKind::SignIn,
        browser: Some("com.google.Chrome".to_owned()),
    }
}

/// The app's viewer, its accounts kept in `dir`, with `hold` left by an
/// earlier service that died holding it, and the helper standing in, its
/// input `settled` or not.
fn after_a_killed_sign_in(dir: &Path, hold: &PrivateHold, settled: bool) -> (Arc<Viewer>, Helper) {
    let (client, helper) = Helper::client(dir, UID);
    let helper = helper.serving_input(&client, &[]).input_settled(settled);
    client.controller.interrupt_for_test(hold);
    let viewer = http::configured(client, "127.0.0.1:1".to_owned(), None);
    let (lines, _) = tokio::sync::mpsc::unbounded_channel();
    *unpoisoned(&viewer.app) = Some(lines);
    *unpoisoned(&viewer.accounts_home) = Some(dir.join("home"));
    std::fs::create_dir_all(dir.join("home")).unwrap();
    (viewer, helper)
}

/// The body the app posts for the sign-in its status shows, `done` or not.
fn answer(viewer: &Viewer, done: bool) -> Value {
    json!({"done": done, "id": viewer.interrupted_for_status()["id"]})
}

/// What the app's answer to the sign-in it is shown gets, `done` or not,
/// with the helper standing in.
async fn settle(viewer: &Viewer, helper: &Helper, done: bool) -> Result<Value> {
    let body = answer(viewer, done);
    helper
        .serve(viewer.sign_in_interrupted(&body), |_| None)
        .await
}

#[tokio::test]
async fn the_apps_service_takes_over_an_interrupted_sign_in_and_tells_the_app() {
    let dir = tempfile::tempdir().unwrap();
    let (viewer, _helper) = after_a_killed_sign_in(dir.path(), &github(), true);

    viewer.adopt_interrupted().unwrap();

    let mut shown = viewer.interrupted_for_status();
    assert!(shown["id"].is_string());
    shown.as_object_mut().unwrap().remove("id");
    let mut expected = json!(github());
    expected["canFinish"] = json!(true);
    assert_eq!(shown, expected);
    let controller = &viewer.client().controller;
    assert_eq!(controller.screen_visible().unwrap_err().code, Code::Busy);
    assert_eq!(controller.acquire(false).unwrap_err().code, Code::Busy);
}

#[tokio::test]
async fn done_records_the_account_and_lets_agents_back() {
    let dir = tempfile::tempdir().unwrap();
    let (viewer, helper) = after_a_killed_sign_in(dir.path(), &github(), true);

    let reply = settle(&viewer, &helper, true).await.unwrap();

    assert_eq!(reply["added"], true);
    let Listed::Accounts(accounts) = sign_ins::listed_in(&dir.path().join("home")) else {
        panic!("unread");
    };
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].id, "github.com");
    assert_eq!(viewer.interrupted_for_status(), Value::Null);
    let controller = &viewer.client().controller;
    assert!(controller.screen_visible().is_ok());
    assert!(controller.acquire(false).is_ok());
}

#[tokio::test]
async fn cancel_adds_nothing_and_ends_the_hold() {
    let dir = tempfile::tempdir().unwrap();
    let (viewer, helper) = after_a_killed_sign_in(dir.path(), &github(), true);

    let reply = settle(&viewer, &helper, false).await.unwrap();

    assert!(reply.get("added").is_none());
    assert_eq!(
        sign_ins::listed_in(&dir.path().join("home")),
        Listed::Accounts(Vec::new())
    );
    assert!(viewer.client().controller.screen_visible().is_ok());
}

#[tokio::test]
async fn nothing_ends_while_the_helper_still_holds_the_sign_ins_input() {
    let dir = tempfile::tempdir().unwrap();
    let (viewer, helper) = after_a_killed_sign_in(dir.path(), &github(), false);

    let refused = settle(&viewer, &helper, false).await.unwrap_err();

    assert_eq!(refused.code, Code::Busy);
    assert!(viewer.client().controller.screen_visible().is_err());
    assert!(unpoisoned(&viewer.interrupted).is_some(), "still held");
}

#[tokio::test]
async fn done_is_refused_for_a_sign_in_it_could_not_record_and_cancel_still_works() {
    let dir = tempfile::tempdir().unwrap();
    let older = PrivateHold {
        browser: None,
        ..github()
    };
    let (viewer, helper) = after_a_killed_sign_in(dir.path(), &older, true);

    assert_eq!(viewer.interrupted_for_status()["canFinish"], false);
    let refused = settle(&viewer, &helper, true).await.unwrap_err();

    assert_eq!(refused.code, Code::Input);
    assert!(viewer.client().controller.screen_visible().is_err());
    settle(&viewer, &helper, false).await.unwrap();
    assert!(viewer.client().controller.screen_visible().is_ok());
}

#[tokio::test]
async fn a_hold_nothing_of_which_can_be_read_offers_cancel_alone() {
    let dir = tempfile::tempdir().unwrap();
    let (viewer, helper) = after_a_killed_sign_in(dir.path(), &github(), true);
    let controller = viewer.client().controller.clone();
    std::fs::write(controller.private_path_for_test(), b"{not a hold").unwrap();

    assert_eq!(viewer.interrupted_for_status()["canFinish"], false);
    assert!(viewer.interrupted_for_status().get("site").is_none());
    let refused = settle(&viewer, &helper, true).await.unwrap_err();
    assert_eq!(refused.code, Code::Input);
    settle(&viewer, &helper, false).await.unwrap();
    assert!(controller.screen_visible().is_ok());
}

#[tokio::test]
async fn a_viewer_without_the_app_reports_the_hold_and_leaves_it_to_the_app() {
    let dir = tempfile::tempdir().unwrap();
    let client = atlas_client::test_support::client_in(dir.path());
    client.controller.interrupt_for_test(&github());
    let viewer = http::configured(client, "127.0.0.1:1".to_owned(), None);

    assert_eq!(viewer.interrupted_for_status()["site"], "github.com");
    assert!(unpoisoned(&viewer.interrupted).is_none());
    assert_eq!(
        viewer
            .sign_in_interrupted(&json!({"done": false, "id": "any"}))
            .await
            .unwrap_err()
            .code,
        Code::State
    );
}

#[tokio::test]
async fn an_input_service_that_does_not_answer_is_started_again_and_the_hold_kept_if_that_fails() {
    let dir = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(dir.path(), UID);
    client.controller.interrupt_for_test(&github());
    let viewer = http::configured(client, "127.0.0.1:1".to_owned(), None);
    let (lines, _) = tokio::sync::mpsc::unbounded_channel();
    *unpoisoned(&viewer.app) = Some(lines);

    assert!(settle(&viewer, &helper, false).await.is_err());

    assert!(viewer.client().controller.screen_visible().is_err());
    assert!(unpoisoned(&viewer.interrupted).is_some(), "still held");
}

#[tokio::test]
async fn an_answer_to_another_sign_in_than_the_one_held_ends_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (viewer, helper) = after_a_killed_sign_in(dir.path(), &github(), true);
    viewer.adopt_interrupted().unwrap();
    let earlier = json!({"done": false, "id": "an-earlier-one"});

    let refused = helper
        .serve(viewer.sign_in_interrupted(&earlier), |_| None)
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::State);
    assert!(viewer.client().controller.screen_visible().is_err());
    assert!(unpoisoned(&viewer.interrupted).is_some());
}

#[tokio::test]
async fn a_new_sign_in_waits_until_the_interrupted_one_is_finished_or_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(dir.path(), UID);
    let helper = helper.serving_input(&client, &[]);
    client.controller.interrupt_for_test(&github());
    // Not the app's: nothing takes the interrupted sign-in over.
    let viewer = http::configured(client, "127.0.0.1:1".to_owned(), None);

    let refused = helper
        .serve(
            viewer.sign_in_start(
                HoldKind::SignIn,
                &json!({"site": "gitlab.com", "browser": "com.google.Chrome"}),
            ),
            |_| Some(json!({"ok": true})),
        )
        .await
        .unwrap_err();

    assert_eq!(
        refused.error,
        "Finish or cancel the sign-in that was interrupted first"
    );
    assert_eq!(
        viewer.client().controller.interrupted_hold(),
        Some(atlas_control::Interrupted {
            hold: Some(github())
        })
    );
}

#[tokio::test]
async fn nothing_ends_until_the_helper_says_each_of_control_input_and_its_record_settled() {
    for state in [
        [Some(false), None, Some(false)],
        [Some(false), Some(false), Some(true)],
        [Some(false), Some(true), Some(false)],
        [None, Some(false), Some(false)],
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(dir.path(), UID);
        let helper = helper.serving_input(&client, &[]).input_state(state);
        client.controller.interrupt_for_test(&github());
        let viewer = http::configured(client, "127.0.0.1:1".to_owned(), None);
        let (lines, _) = tokio::sync::mpsc::unbounded_channel();
        *unpoisoned(&viewer.app) = Some(lines);

        assert!(settle(&viewer, &helper, false).await.is_err(), "{state:?}");

        assert!(
            viewer.client().controller.screen_visible().is_err(),
            "{state:?}"
        );
        assert!(unpoisoned(&viewer.interrupted).is_some(), "{state:?}");
    }
}

#[tokio::test]
async fn two_answers_at_once_end_the_sign_in_once() {
    let dir = tempfile::tempdir().unwrap();
    let (viewer, helper) = after_a_killed_sign_in(dir.path(), &github(), true);
    let (done, cancel) = (answer(&viewer, true), answer(&viewer, false));

    let (first, second) = helper
        .serve(
            async {
                tokio::join!(
                    viewer.sign_in_interrupted(&done),
                    viewer.sign_in_interrupted(&cancel)
                )
            },
            |_| None,
        )
        .await;

    assert_eq!(first.unwrap()["added"], true);
    assert_eq!(second.unwrap_err().code, Code::State);
    assert!(viewer.client().controller.screen_visible().is_ok());
}

#[tokio::test]
async fn a_second_answer_waits_while_the_first_recovers_the_helper() {
    let dir = tempfile::tempdir().unwrap();
    // Not settled: each answer goes on to recover the helper's input.
    let (viewer, helper) = after_a_killed_sign_in(dir.path(), &github(), false);
    let (began, go_on) = (
        Arc::new(tokio::sync::Notify::new()),
        Arc::new(tokio::sync::Semaphore::new(0)),
    );
    *unpoisoned(&viewer.recovery_pause.set) = Some((began.clone(), go_on.clone()));
    let (first_body, second_body) = (answer(&viewer, false), answer(&viewer, false));

    let (first, second) = helper
        .serve(
            async {
                let first = viewer.sign_in_interrupted(&first_body);
                let second = async {
                    began.notified().await;
                    let second = viewer.sign_in_interrupted(&second_body);
                    tokio::pin!(second);
                    tokio::select! {
                        biased;
                        _ = &mut second => panic!("it did not wait for the first"),
                        () = std::future::ready(()) => {}
                    }
                    assert!(
                        viewer.resolving.try_lock().is_err(),
                        "the first still recovers"
                    );
                    let other = viewer.client().controller.clone();
                    assert_eq!(
                        other.acquire(false).unwrap_err().code,
                        Code::Busy,
                        "the exclusion is held through the recovery"
                    );
                    go_on.add_permits(2);
                    second.await
                };
                tokio::join!(first, second)
            },
            |_| None,
        )
        .await;

    // The stand-in never settles: each keeps the sign-in held.
    assert!(first.is_err());
    assert!(second.is_err());
    assert!(viewer.client().controller.screen_visible().is_err());
    assert!(unpoisoned(&viewer.interrupted).is_some());
}

#[tokio::test]
async fn the_helper_stays_while_a_sign_in_taken_over_waits_for_the_person() {
    let dir = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(dir.path()).unwrap();
    let settings = base.join("u.toml");
    std::fs::write(&settings, atlas_core::test_support::SETTINGS).unwrap();
    let mut client = Client::new(atlas_core::config::Config::load(&settings).unwrap()).unwrap();
    client.controller = atlas_control::Controller::at(60001, &base.join("state"));
    client.controller.interrupt_for_test(&github());
    let (_, viewer) = http::bind(client, 0, Some(&settings)).await.unwrap();
    let (lines, _) = tokio::sync::mpsc::unbounded_channel();
    *unpoisoned(&viewer.app) = Some(lines);
    viewer.adopt_interrupted().unwrap();

    std::fs::write(
        &settings,
        atlas_core::test_support::SETTINGS.replace("60001", "60002"),
    )
    .unwrap();
    viewer.follow_settings().await;

    assert_eq!(viewer.client().config.helper_uid, 60001, "unchanged");
}
