use crate::test_support::client_in;
use atlas_control::Controller;
use atlas_core::digest::file_sha256;
use std::fs;

#[test]
fn an_interrupted_setup_can_repair_its_probe_without_settling_pending_work() {
    if !atlas_core::test_support::isolated(
        "client::install::tests::an_interrupted_setup_can_repair_its_probe_without_settling_pending_work",
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
