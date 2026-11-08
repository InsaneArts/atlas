//! Element references through the public library: what a reference that
//! names nothing gets back.
use atlas_client::{agent, client::Client};
use atlas_control::Controller;
use atlas_core::{actions::Button, config::Config, screen::elements::LOOK_AGAIN};
use serde_json::json;

#[tokio::test]
async fn an_element_action_on_a_ref_that_names_nothing_is_refused_with_nothing_sent() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = Client::new(Config {
        helper_uid: nix::unistd::getuid().as_raw() + 1,
        socket_path: dir.path().join("sock"),
        identity_probe: dir.path().join("probe"),
        input_socket: dir.path().join("input"),
        output_dir: dir.path().join("out"),
        activity: Default::default(),
    })
    .unwrap();
    client.controller = Controller::at(502, &dir.path().join("state"));

    for action in [
        json!({"type": "press", "ref": "Save"}),
        json!({"type": "pick", "ref": "8123abc.12", "option": "Large"}),
    ] {
        let error = agent::element(&client, None, &action).await.unwrap_err();
        assert_eq!(error.code, "elements");
        assert!(!error.uncertain);
        assert_eq!(error.details["next"], LOOK_AGAIN);
        assert_eq!(error.details["performed"], 0);
    }
    assert!(client.controller.pending().unwrap().is_empty());
    assert!(!dir.path().join("out").exists(), "nothing was written");
}

#[tokio::test]
async fn a_reference_that_names_no_element_is_refused_with_nothing_sent() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = Client::new(Config {
        helper_uid: nix::unistd::getuid().as_raw() + 1,
        socket_path: dir.path().join("sock"),
        identity_probe: dir.path().join("probe"),
        input_socket: dir.path().join("input"),
        output_dir: dir.path().join("out"),
        activity: Default::default(),
    })
    .unwrap();
    client.controller = Controller::at(502, &dir.path().join("state"));

    let error = agent::click_reference(&client, None, "Save", Button::Left, 1, &[])
        .await
        .unwrap_err();
    assert_eq!(error.code, "elements");
    assert!(!error.uncertain);
    assert_eq!(error.details["next"], LOOK_AGAIN);
    assert_eq!(error.details["performed"], 0);
    assert!(client.controller.pending().unwrap().is_empty());
    assert!(!dir.path().join("out").exists(), "nothing was written");
}
