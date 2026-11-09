use super::*;
use crate::test_support::Helper;
use atlas_core::test_support::UID;
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
