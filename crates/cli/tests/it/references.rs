//! Element references through the public library: what a reference is,
//! how a referenced click is checked with its batch, and what a reference
//! that names nothing gets back.
use atlas_client::{agent, client::Client};
use atlas_control::Controller;
use atlas_core::{
    actions::{Batch, Button},
    config::Config,
    screen::elements::{LOOK_AGAIN, Look, Reference},
    session::Display,
};
use serde_json::json;

#[test]
fn a_reference_is_a_look_name_and_an_element_index_written_one_way() {
    let reference = Reference::parse("8123abc12").unwrap();
    assert_eq!(reference.look, Look::parse("8123abc").unwrap());
    assert_eq!(reference.index, 12);
    assert_eq!(reference.to_string(), "8123abc12");
    for text in [
        "8123abc",
        "8123abc012",
        "8123abc.e12",
        "8123ABC12",
        "12",
        "kfp12",
    ] {
        assert!(Reference::parse(text).is_none(), "{text}");
    }
}

#[test]
fn a_referenced_click_is_validated_with_its_whole_batch_before_anything_is_sent() {
    let display = Display {
        width: 2560.0,
        height: 1440.0,
        scale: 1.0,
    };
    let validate = |batch: serde_json::Value| Batch::decode(&batch)?.check(&display);

    validate(json!([{"type": "reference", "ref": "8123abc12"}, {"type": "key", "key": "return"}]))
        .unwrap();
    let refused =
        validate(json!([{"type": "reference", "ref": "8123abc12", "count": 6}])).unwrap_err();
    assert_eq!(refused.code, "input");
    assert_eq!(
        refused.error,
        "Action 1 was refused and nothing was sent: a click is 1 to 5 clicks"
    );
    let malformed = validate(json!([{"type": "reference", "ref": "Save"}])).unwrap_err();
    assert_eq!(malformed.code, "invalid-json");
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
