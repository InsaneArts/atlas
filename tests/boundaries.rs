use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixListener,
    sync::Mutex,
};
use understudy::{
    actions,
    client::{Client, png_dimensions, validate_completion},
    config::Config,
    control::{ActionPermit, Controller, Lease},
    session::{Display, require_hidden},
    transport::Transport,
};

fn status() -> Value {
    json!({"ok": true, "user": {"uid": 502}, "daemon": {"pid": 123},
        "session": {"managerName": "Aqua", "onConsole": false},
        "display": {"width": 2560, "height": 1440, "scale": 1},
        "permissions": {"screenCapture": true, "accessibility": true},
        "sessionProof": {"ok": true,"uid":502,"hasGraphicAccess":true,"parentPID":123,"onConsole":false,
        "securitySessionID":100384,"displayID":28,"width":2560,"height":1440}})
}
fn display() -> Display {
    Display {
        width: 2560.0,
        height: 1440.0,
        scale: 1.0,
    }
}

#[test]
fn graphical_identity_needs_independent_proof() {
    assert!(require_hidden(&status(), 502).is_ok());
    for path in [
        "onConsole",
        "hasGraphicAccess",
        "securitySessionID",
        "displayID",
        "uid",
        "width",
    ] {
        let mut state = status();
        state["sessionProof"].as_object_mut().unwrap().remove(path);
        assert!(require_hidden(&state, 502).is_err(), "{path}");
    }
    let mut state = status();
    state["session"]["onConsole"] = json!(true);
    assert!(require_hidden(&state, 502).is_err());
    let mut state = status();
    state["sessionProof"]["hasGraphicAccess"] = json!(false);
    assert!(require_hidden(&state, 502).is_err());
}

#[test]
fn identities_change_when_security_session_or_display_changes() {
    let original = require_hidden(&status(), 502).unwrap().epoch;
    for field in ["securitySessionID", "displayID"] {
        let mut state = status();
        state["sessionProof"][field] = json!(99);
        assert_ne!(original, require_hidden(&state, 502).unwrap().epoch);
    }
}

#[test]
fn unsafe_actions_and_whole_batches_are_rejected() {
    for action in [
        json!({"type":"click","x":true,"y":10}),
        json!({"type":"move","x":-1,"y":10}),
        json!({"type":"move","x":1e99,"y":10}),
        json!({"type":"scroll","x":1,"y":2,"dy":1.5}),
        json!({"type":"click","x":1,"y":2,"count":0}),
        json!({"type":"key","key":"cmd+invented"}),
        json!({"type":"wait","ms":251}),
        json!({"type":"type","text":"x".repeat(257)}),
        json!({"type":"click","x":1,"y":2,"modifiers":["unknown"]}),
        json!({"type":"move","x":1,"y":2,"secret":true}),
    ] {
        assert!(actions::validate(&json!([action]), &display()).is_err());
    }
    assert!(
        actions::validate(&json!(vec![json!({"type":"wait","ms":250}); 9]), &display()).is_err()
    );
    assert!(
        actions::validate(
            &json!([{ "type":"key", "key":"return" }, { "type":"run" }]),
            &display()
        )
        .is_err()
    );
}

#[test]
fn unicode_and_supported_shortcuts_validate() {
    for key in ["cmd+[", "cmd+shift+/", "cmd+a", "ctrl+alt+forwarddelete"] {
        assert!(actions::valid_key(key));
    }
    assert!(
        actions::validate(
            &json!([{ "type":"type", "text":"ქართული 日本語 🧑🏽‍💻 é" }]),
            &display()
        )
        .is_ok()
    );
}

#[test]
fn signal_timeout_and_exit_are_distinct_completed_outcomes() {
    let request = json!({"op":"run"});
    for reply in [
        json!({"ok":true,"exitCode":0,"timedOut":false,"signal":null}),
        json!({"ok":true,"exitCode":null,"timedOut":false,"signal":"SIGTERM"}),
        json!({"ok":true,"exitCode":null,"timedOut":true,"signal":"SIGKILL"}),
    ] {
        validate_completion(&request, &reply).unwrap();
    }
    for reply in [
        json!({"ok":true}),
        json!({"ok":true,"exitCode":null,"timedOut":false,"signal":15}),
    ] {
        assert!(validate_completion(&request, &reply).unwrap_err().uncertain);
    }
    assert!(
        validate_completion(
            &json!({"op":"input","actions":[{}]}),
            &json!({"ok":true,"performed":0})
        )
        .unwrap_err()
        .uncertain
    );
}

#[test]
fn config_paths_are_relative_and_unknown_fields_fail() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    let mut value = json!({"helper_uid":502,"socket_path":"sock","identity_probe":"probe","input_socket":"input","output_dir":"out"});
    std::fs::write(&path, value.to_string()).unwrap();
    let config = Config::load(&path).unwrap();
    assert_eq!(
        config.socket_path,
        std::fs::canonicalize(dir.path()).unwrap().join("sock")
    );
    value["unexpected"] = json!(true);
    std::fs::write(&path, value.to_string()).unwrap();
    assert!(Config::load(&path).is_err());
}

#[test]
fn library_construction_cannot_target_the_caller() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config {
        helper_uid: unsafe { libc::getuid() },
        socket_path: dir.path().join("sock"),
        identity_probe: dir.path().join("probe"),
        input_socket: dir.path().join("input"),
        output_dir: dir.path().join("out"),
    };
    assert!(Client::new(config).is_err());
    assert!(!dir.path().join("out").exists());
}

#[test]
fn symlink_aliases_share_the_same_lock_and_pending_marker() {
    let dir = tempfile::tempdir().unwrap();
    let alias = dir.path().join("alias");
    std::os::unix::fs::symlink(dir.path(), &alias).unwrap();
    let first = Controller::at(502, &dir.path().join("sock"), dir.path()).unwrap();
    let second = Controller::at(502, &alias.join("sock"), dir.path()).unwrap();
    let held = first.acquire(false).unwrap();
    assert_eq!(second.acquire(false).unwrap_err().code, "busy");
    first.quarantine().unwrap();
    drop(held);
    assert_eq!(second.acquire(false).unwrap_err().code, "uncertain");
    let recovery = second.acquire(true).unwrap();
    second.clear().unwrap();
    drop(recovery);
    first.acquire(false).unwrap();
}

#[tokio::test]
async fn lease_expiry_keeps_flock_until_the_action_settles() {
    let dir = tempfile::tempdir().unwrap();
    let controller = Controller::at(502, &dir.path().join("socket"), dir.path()).unwrap();
    let lease = Arc::new(Mutex::new(Lease::new(Duration::from_millis(10))));
    let owner = lease.lock().await.acquire(&controller).unwrap();
    let permit = ActionPermit::begin(lease.clone(), &owner).await.unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(lease.lock().await.require(&owner).is_err());
    assert_eq!(controller.acquire(false).unwrap_err().code, "busy");
    drop(permit);
    assert_eq!(controller.acquire(false).unwrap_err().code, "busy");
    let retiring = lease.lock().await.begin_release(None).unwrap();
    assert_eq!(retiring.as_deref(), Some(owner.as_str()));
    lease.lock().await.end_release();
    controller.acquire(false).unwrap();
}

#[test]
fn cleanup_completion_cannot_clear_unknown_input_delivery() {
    let dir = tempfile::tempdir().unwrap();
    let controller = Controller::at(502, &dir.path().join("socket"), dir.path()).unwrap();
    let input = controller.quarantine().unwrap();
    let cleanup = controller.quarantine().unwrap();
    controller.clear_if(&cleanup).unwrap();
    assert_eq!(controller.require_known().unwrap_err().code, "uncertain");
    controller.clear_if(&input).unwrap();
    controller.require_known().unwrap();
    let older = controller.quarantine().unwrap();
    let newer = controller.quarantine().unwrap();
    controller.clear_if(&older).unwrap();
    assert!(controller.require_known().is_err());
    controller.clear_if(&newer).unwrap();
    controller.require_known().unwrap();
}

#[test]
fn async_transport_futures_do_not_embed_large_receive_buffers() {
    let transport = Transport {
        path: "/tmp/unused-understudy-test".into(),
        timeout: Duration::from_secs(1),
    };
    let request = json!({"op":"hello"});
    let future = transport.call(&request);
    assert!(
        std::mem::size_of_val(&future) < 8192,
        "A large receive buffer on the async stack crashed the debug HTTP server"
    );
}

#[tokio::test]
async fn old_owner_cannot_release_a_new_lease() {
    let dir = tempfile::tempdir().unwrap();
    let controller = Controller::at(502, &dir.path().join("socket"), dir.path()).unwrap();
    let mut lease = Lease::default();
    let old = lease.acquire(&controller).unwrap();
    lease.release();
    let new = lease.acquire(&controller).unwrap();
    if lease.require(&old).is_ok() {
        lease.release();
    }
    lease.require(&new).unwrap();
}

#[test]
fn delayed_acquire_failure_cannot_release_a_replacement_owner() {
    let dir = tempfile::tempdir().unwrap();
    let controller = Controller::at(502, &dir.path().join("socket"), dir.path()).unwrap();
    let mut lease = Lease::default();
    let old = lease.acquire(&controller).unwrap();
    assert_eq!(lease.begin_release(Some(&old)).unwrap(), Some(old.clone()));
    lease.end_release();
    let current = lease.acquire(&controller).unwrap();

    lease.abandon_acquire(&old);
    lease.require(&current).unwrap();
    assert_eq!(controller.acquire(false).unwrap_err().code, "busy");

    assert_eq!(
        lease.begin_release(Some(&current)).unwrap(),
        Some(current.clone())
    );
    lease.abandon_acquire(&current);
    assert_eq!(controller.acquire(false).unwrap_err().code, "busy");
    lease.end_release();
    controller.acquire(false).unwrap();
}

async fn exchange(
    chunks: Vec<Vec<u8>>,
    timeout: Duration,
    pause: Duration,
) -> understudy::error::Result<Value> {
    let dir = tempfile::Builder::new()
        .prefix("ust-")
        .tempdir_in("/tmp")
        .unwrap();
    let socket = dir.path().join("s");
    let listener = UnixListener::bind(&socket).unwrap();
    let worker = tokio::spawn(async move {
        let (mut peer, _) = listener.accept().await.unwrap();
        let mut request = [0; 1024];
        let _ = peer.read(&mut request).await;
        for chunk in chunks {
            tokio::time::sleep(pause).await;
            if peer.write_all(&chunk).await.is_err() {
                break;
            }
        }
    });
    let result = Transport {
        path: socket,
        timeout,
    }
    .call(&json!({"op":"input","actions":[]}))
    .await;
    worker.abort();
    result
}

#[tokio::test]
async fn fragmented_json_lines_are_reassembled_once() {
    let reply = exchange(
        vec![
            b"{\"event\":\"started\",\"pid\":2}\n{\"ok\":".to_vec(),
            b"true}\n".to_vec(),
        ],
        Duration::from_secs(1),
        Duration::ZERO,
    )
    .await
    .unwrap();
    assert_eq!(reply["events"][0]["pid"], 2);
}

#[tokio::test]
async fn malformed_replies_remain_uncertain() {
    for bytes in [
        b"null\n".to_vec(),
        b"{\"ok\":1}\n".to_vec(),
        b"{\"ok\":true,\"x\":1e10000}\n".to_vec(),
        b"[".repeat(2000),
    ] {
        assert!(
            exchange(vec![bytes], Duration::from_secs(1), Duration::ZERO)
                .await
                .unwrap_err()
                .uncertain
        );
    }
}

#[tokio::test]
async fn a_drip_feed_cannot_extend_the_wall_clock_deadline() {
    let started = std::time::Instant::now();
    assert!(
        exchange(
            vec![b" ".to_vec(); 100],
            Duration::from_millis(80),
            Duration::from_millis(10)
        )
        .await
        .is_err()
    );
    assert!(started.elapsed() < Duration::from_millis(300));
}

#[test]
fn capture_rejects_non_png_and_extreme_dimensions() {
    assert!(png_dimensions(b"not a PNG").is_err());
    let mut header = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    header.extend_from_slice(&u32::MAX.to_be_bytes());
    header.extend_from_slice(&900u32.to_be_bytes());
    header.extend_from_slice(&[0; 9]);
    assert!(png_dimensions(&header).is_err());
}
