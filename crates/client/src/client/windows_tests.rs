use super::*;
use crate::test_support::Helper;
use atlas_core::test_support::UID;
use atlas_core::{
    protocol::SESSION_PIN,
    screen::windows::HitWindow,
    session::{Display, Epoch, Observation, Session},
};
use tokio::{io::AsyncReadExt, net::UnixListener};

#[test]
fn an_older_daemon_tells_the_person_to_restart_before_listing_windows() {
    let error =
        window_list(json!({"ok": false, "code": "bad-request", "error": "unknown op 'windows'"}))
            .unwrap_err();
    assert_eq!(error.code, Code::Daemon);
    assert!(error.error.contains("older Atlas"));
    assert!(error.error.contains("atlas restart"));
    assert!(!error.uncertain);
}

#[test]
fn a_window_reply_keeps_refusals_and_never_reads_malformed_data_as_an_empty_list() {
    let error = window_list(json!({"ok": false, "code": "session-changed", "error": "look again"}))
        .unwrap_err();
    assert_eq!(error.code, Code::SessionChanged);
    for reply in [
        json!({}),
        json!({"ok": true}),
        json!({"ok": true, "windows": [null]}),
    ] {
        assert_eq!(window_list(reply).unwrap_err().code, Code::Windows);
    }
    assert!(
        window_list(json!({"ok": true, "windows": [], "starts": {}}))
            .unwrap()
            .windows
            .is_empty()
    );
}

#[tokio::test]
async fn a_window_read_keeps_its_deadline_and_never_leaves_an_uncertain_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let (mut client, helper) = Helper::client(directory.path(), UID);
    let verified = helper
        .serve(client.verify(), |_| panic!("only identity is read"))
        .await
        .unwrap();
    client.transport.path = directory.path().join("waiting.sock");
    let listener = UnixListener::bind(&client.transport.path).unwrap();
    let waiting = async {
        let (mut peer, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        loop {
            let byte = peer.read_u8().await.unwrap();
            if byte == b'\n' {
                break;
            }
            request.push(byte);
        }
        let request: serde_json::Value = serde_json::from_slice(&request).unwrap();
        assert_eq!(request["op"], "windows");
        assert_eq!(request["session"], json!(verified.pin().unwrap()));
        let mut byte = [0];
        assert_eq!(
            peer.read(&mut byte).await.unwrap(),
            0,
            "the deadline closes the read"
        );
    };
    let reading = client.windows_in(Some(&verified), false, Duration::from_millis(20));
    let (result, ()) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(reading, waiting)
    })
    .await
    .unwrap();
    let error = result.unwrap_err();
    assert_eq!(error.code, Code::Transport);
    assert!(!error.uncertain);
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn a_missing_window_endpoint_preserves_memory_without_starting_the_look() {
    use crate::host_session::{Host, HostSession};
    use atlas_core::screen::windows::Bounds;
    use std::cell::Cell;

    let directory = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(directory.path());
    client.host_session = Some(HostSession::new(Host::Named, "one").unwrap());
    let verified = Verified {
        session: Session {
            epoch: Epoch {
                uid: client.config.helper_uid,
                capture_pid: 123,
                input_pid: 456,
                security_session_id: 42,
                display_id: 7,
                display: Display {
                    width: 100.0,
                    height: 100.0,
                    scale: 1.0,
                },
            },
            capture: true,
            input: true,
        },
        observation: Observation::decode(&json!({"daemon": {"capabilities": [SESSION_PIN]}})),
    };
    client.remember_windows(
        &[HitWindow {
            number: 42,
            pid: 812,
            start: Some([123, 456]),
            layer: 0,
            bounds: Bounds::display(100.0, 100.0),
        }],
        &verified.session().epoch,
    );
    let looked = Cell::new(false);
    let result = tokio::time::timeout(
        Duration::from_secs(1),
        client.look(&verified, async {
            looked.set(true);
            Ok(())
        }),
    )
    .await
    .expect("the missing endpoint is refused promptly");
    assert!(result.is_err());
    assert!(!looked.get());
    assert_eq!(
        client.remembered_windows().unwrap().windows.unwrap()[0].number,
        42
    );
}
