use super::*;

#[test]
fn signal_timeout_and_exit_are_distinct_completed_outcomes() {
    for reply in [
        json!({"ok":true,"exitCode":0,"timedOut":false,"signal":null}),
        json!({"ok":true,"exitCode":null,"timedOut":false,"signal":"SIGTERM"}),
        json!({"ok":true,"exitCode":null,"timedOut":true,"signal":"SIGKILL"}),
    ] {
        Mutation::Run.validate(&reply).unwrap();
    }

    for reply in [
        json!({"ok":true}),
        json!({"ok":true,"exitCode":null,"timedOut":false,"signal":15}),
    ] {
        assert!(Mutation::Run.validate(&reply).unwrap_err().uncertain);
    }

    assert!(
        Mutation::Input {
            count: 1,
            expect: None
        }
        .validate(&json!({"ok":true,"performed":0}))
        .unwrap_err()
        .uncertain
    );
}

/// The client's own transport reads this daemon's replies.
#[tokio::test]
async fn the_client_transport_understands_the_daemon() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("d.sock");
    atlas_daemon::test_support::serve(tokio::net::UnixListener::bind(&socket).unwrap());

    let transport = atlas_core::transport::Transport {
        path: socket,
        timeout: std::time::Duration::from_secs(10),
        peer_uid: nix::unistd::getuid().as_raw(),
    };

    let reply = transport
        .call(&json!({"op": "run", "argv": ["/bin/sh", "-c", "printf ok; exit 4"], "timeoutMs": 3000}))
        .await
        .unwrap();
    assert_eq!(
        atlas_core::transport::run_output(&reply, 64).unwrap(),
        b"ok"
    );
    assert_eq!(
        (&reply["exitCode"], &reply["timedOut"]),
        (&json!(4), &json!(false))
    );
    Mutation::Run.validate(&reply).unwrap();

    let hello = transport.call(&json!({"op": "hello"})).await.unwrap();
    assert_eq!(hello["daemon"]["pid"], std::process::id());
}
