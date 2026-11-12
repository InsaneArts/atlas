use atlas_client::client::Client;
use atlas_control::Controller;
use atlas_core::screen::{frame::Size, png::png_dimensions};
use atlas_core::{
    actions,
    config::Config,
    desktop::{Desktop, Hidden},
    error::Code,
    session::{Display, InputEpoch, Observation, diagnosis, require_hidden},
    transport::Transport,
};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixListener,
};

fn status() -> Value {
    json!({"ok": true, "user": {"uid": 502}, "daemon": {"pid": 123},
        "session": {"managerName": "Aqua", "onConsole": false},
        "display": {"width": 2560, "height": 1440, "scale": 1},
        "permissions": {"screenCapture": true, "accessibility": true},
        "sessionProof": {"ok": true,"uid":502,"hasGraphicAccess":true,"parentPID":123,"onConsole":false,
        "securitySessionID":100384,"displayID":28,"width":2560,"height":1440}})
}

/// Validates a whole input batch on `display` without sending any of it.
fn validate(batch: &Value, display: &Display) -> atlas_core::error::Result<()> {
    actions::Batch::decode(batch)?.check(display)
}

fn display() -> Display {
    Display {
        width: 2560.0,
        height: 1440.0,
        scale: 1.0,
    }
}

#[test]
fn a_changed_display_or_process_invalidates_the_original_epoch() {
    let original = require_hidden(&ready_status(), 502).unwrap().epoch;
    for (path, value) in [
        ("/daemon/pid", json!(124)),
        ("/inputService/pid", json!(78)),
        ("/sessionProof/securitySessionID", json!(99)),
        ("/sessionProof/displayID", json!(99)),
        ("/display/width", json!(1440)),
        ("/display/scale", json!(2)),
    ] {
        let mut state = ready_status();
        *state.pointer_mut(path).unwrap() = value;
        state["sessionProof"]["parentPID"] = state["daemon"]["pid"].clone();
        state["sessionProof"]["width"] = state["display"]["width"].clone();
        let changed = require_hidden(&state, 502).unwrap().epoch;
        assert_ne!(original, changed, "{path}");
    }
}

#[test]
fn hidden_session_proofs_keep_their_field_and_range_requirements() {
    for (path, invalid) in [
        ("/user/uid", json!(503)),
        ("/daemon/pid", json!(u64::from(u32::MAX) + 1)),
        ("/session/managerName", json!("Background")),
        ("/session/onConsole", json!(true)),
        ("/display/width", json!(0)),
        ("/display/height", json!(32769)),
        ("/display/scale", json!(9)),
        ("/sessionProof/ok", json!(false)),
        ("/sessionProof/uid", json!(501)),
        ("/sessionProof/parentPID", json!(124)),
        ("/sessionProof/onConsole", json!(true)),
        ("/sessionProof/hasGraphicAccess", json!(false)),
        ("/sessionProof/securitySessionID", json!(u32::MAX)),
        ("/sessionProof/displayID", json!(u64::from(u32::MAX) + 1)),
        ("/sessionProof/width", json!(1e99)),
        ("/sessionProof/height", json!(-1)),
    ] {
        for value in [
            None,
            Some(Value::Null),
            Some(json!("unknown")),
            Some(invalid),
        ] {
            let mut state = ready_status();
            match value {
                Some(value) => *state.pointer_mut(path).unwrap() = value,
                None => {
                    let (parent, key) = path.rsplit_once('/').unwrap();
                    state
                        .pointer_mut(parent)
                        .unwrap()
                        .as_object_mut()
                        .unwrap()
                        .remove(key);
                }
            }
            let error = require_hidden(&state, 502).unwrap_err();
            let console =
                state["session"]["onConsole"] == true || state["sessionProof"]["onConsole"] == true;
            assert_eq!(
                error.code,
                if console {
                    Code::OnConsole
                } else {
                    Code::Identity
                },
                "{path}: {state}"
            );
        }
    }
}

#[test]
fn input_proof_refusals_preserve_the_capture_session() {
    let valid = ready_status()["inputService"]["session"].clone();
    assert!(InputEpoch::from_proof(&valid, 77).is_ok());
    for (field, invalid) in [
        ("ok", json!(false)),
        ("uid", json!(500)),
        ("securitySessionID", json!(u32::MAX)),
        ("displayID", json!(u64::from(u32::MAX) + 1)),
        ("width", json!(0)),
        ("height", json!(32769)),
        ("onConsole", json!(true)),
        ("hasGraphicAccess", json!(false)),
    ] {
        for value in [
            None,
            Some(Value::Null),
            Some(json!("unknown")),
            Some(invalid),
        ] {
            let mut proof = valid.clone();
            match value {
                Some(value) => proof[field] = value,
                None => {
                    proof.as_object_mut().unwrap().remove(field);
                }
            }
            let error = InputEpoch::from_proof(&proof, 77).unwrap_err();
            assert_eq!(error.code, Code::Identity);
            assert_eq!(
                error.error,
                "A hidden graphical session with valid geometry is required"
            );
            let mut state = ready_status();
            state["inputService"]["session"] = proof;
            let session = require_hidden(&state, 502).unwrap();
            assert!(session.capture);
            assert!(!session.input, "{field}: {state}");
        }
    }
    assert!(InputEpoch::from_proof(&valid, 0).is_err());
}

/// Whether `status` shows input service `pid` idle in the hidden helper.
fn settled(status: &Value, pid: u32) -> bool {
    let observation = Observation::decode(status);
    observation
        .require_hidden(502)
        .is_ok_and(|session| observation.settled_in(&session, pid))
}

#[test]
fn only_idle_empty_input_settles_recovery() {
    let mut ready = ready_status();
    ready["inputService"]["controlActive"] = json!(false);
    ready["inputService"]["heldInput"] = json!(false);
    assert!(settled(&ready, 77));
    for field in ["controlActive", "heldInput"] {
        for value in [
            None,
            Some(Value::Null),
            Some(json!(true)),
            Some(json!(0)),
            Some(json!("false")),
        ] {
            let mut state = ready.clone();
            match value {
                Some(value) => state["inputService"][field] = value,
                None => {
                    state["inputService"].as_object_mut().unwrap().remove(field);
                }
            }
            assert!(!settled(&state, 77), "{field}: {state}");
        }
    }
}

#[test]
fn proof_refusals_keep_their_precedence_and_words() {
    for (code, fallback) in [
        ("blocked", "This shell may not connect to the helper"),
        ("busy", "The helper is serving too many requests; try again"),
        ("identity", "The helper socket belongs to another account"),
    ] {
        let mut state = ready_status();
        state["ok"] = json!(false);
        state["code"] = json!(code);
        state["user"]["uid"] = json!(503);
        state["session"]["onConsole"] = json!(true);
        for (cause, expected) in [
            (Value::Null, fallback),
            (json!("Original cause"), "Original cause"),
        ] {
            state["error"] = cause;
            let error = require_hidden(&state, 502).unwrap_err();
            assert_eq!(error.code.as_str(), code);
            assert_eq!(error.error, expected);
            assert!(!error.uncertain);
        }
        assert_eq!(
            require_hidden(&state, 500).unwrap_err().error,
            "Target must be a distinct helper account with UID >= 501"
        );
    }
    let mut state = ready_status();
    state["user"]["uid"] = json!(503);
    state["sessionProof"]["onConsole"] = json!(true);
    assert_eq!(
        require_hidden(&state, 502).unwrap_err().error,
        "A different account answered than the configured helper. Run atlas doctor."
    );
    state["user"]["uid"] = Value::Null;
    assert_eq!(
        require_hidden(&state, 502).unwrap_err().code,
        Code::OnConsole
    );
}

#[test]
fn compatibility_decoders_keep_legacy_absence_and_unknown_extensions() {
    let mut state = status();
    state["display"] = json!([2560, 1440, 1]);
    state["future"] = json!({"nested": [true, "extension"]});
    let session = require_hidden(&state, 502).unwrap();
    assert!(session.capture);
    assert!(!session.input);
    assert_eq!(session.epoch.input_pid, 0);

    let mut state = ready_status();
    state["inputService"]["session"]["future"] = json!(true);
    assert!(require_hidden(&state, 502).unwrap().input);
    state["inputService"]["protocolVersion"] = json!(1.0);
    assert!(!require_hidden(&state, 502).unwrap().input);
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
        json!({"type":"click","x":1,"y":2,"modifiers":vec!["cmd"; 6]}),
        json!({"type":"move","x":1,"y":2,"secret":true}),
    ] {
        // A malformed action fails to parse; a well-formed one out of
        // bounds is refused as input.
        let refused = validate(&json!([action]), &display()).unwrap_err();
        assert!(
            matches!(refused.code, Code::Input | Code::InvalidJson),
            "{action}: {}",
            refused.code
        );
    }

    assert_eq!(
        validate(&json!(vec![json!({"type":"wait","ms":250}); 9]), &display())
            .unwrap_err()
            .code,
        "input"
    );
    assert_eq!(
        validate(
            &json!([{ "type":"key", "key":"return" }, { "type":"run" }]),
            &display()
        )
        .unwrap_err()
        .code,
        "invalid-json"
    );
}

#[test]
fn the_whole_batch_is_deserialized_before_semantics_count_or_budget_are_checked() {
    let malformed = json!({"type":"move","y":1});
    let mut too_many = vec![json!({"type":"wait","ms":0}); 32];
    too_many.push(malformed.clone());
    let mut too_slow = vec![json!({"type":"wait","ms":250}); 9];
    too_slow.push(malformed.clone());

    for batch in [
        json!([{"type":"move","x":-1,"y":1}, malformed]),
        json!(too_many),
        json!(too_slow),
    ] {
        let error = validate(&batch, &display()).unwrap_err();
        assert_eq!(error.code, "invalid-json");
        assert_eq!(error.error, "missing field `x`");
    }
}

#[test]
fn an_invalid_tail_names_the_action_and_keeps_field_error_precedence() {
    for (x, count, problem) in [
        (-1, 0, "its point is off the display"),
        (1, 0, "a click is 1 to 5 clicks"),
        (1, 1, "a modifier is one of cmd, ctrl, alt, shift and fn"),
    ] {
        let batch = json!([
            {"type":"click","x":1,"y":1},
            {"type":"click","x":x,"y":1,"count":count,"modifiers":["hyper"]},
        ]);
        let error = validate(&batch, &display()).unwrap_err();
        assert_eq!(error.code, "input");
        assert_eq!(
            error.error,
            format!("Action 2 was refused and nothing was sent: {problem}")
        );
    }
}

#[test]
fn action_count_and_total_budget_keep_their_exact_boundaries() {
    for count in [0, 33] {
        let error = validate(
            &json!(vec![json!({"type":"move","x":1,"y":1}); count]),
            &display(),
        )
        .unwrap_err();
        assert_eq!(error.code, "input");
        assert_eq!(error.error, "Send between 1 and 32 actions");
    }
    validate(
        &json!(vec![json!({"type":"move","x":1,"y":1}); 32]),
        &display(),
    )
    .unwrap();

    let mut batch = vec![json!({"type":"wait","ms":250}); 8];
    validate(&json!(batch), &display()).unwrap();
    batch.push(json!({"type":"key","key":"return"}));
    let error = validate(&json!(batch), &display()).unwrap_err();
    assert_eq!(error.code, "input");
    assert_eq!(
        error.error,
        "The batch would take longer than two seconds; send it in parts"
    );
}

#[test]
fn unicode_and_supported_shortcuts_validate() {
    for key in ["cmd+[", "cmd+shift+/", "cmd+a", "ctrl+alt+forwarddelete"] {
        assert!(actions::valid_key(key));
    }

    assert!(
        validate(
            &json!([{ "type":"type", "text":"ქართული 日本語 🧑🏽‍💻 e\u{301}" }]),
            &display()
        )
        .is_ok()
    );
}

#[test]
fn config_paths_are_relative_and_unknown_fields_fail() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let text = "helper_uid = 502\nsocket_path = \"sock\"\nidentity_probe = \"probe\"\n\
                input_socket = \"input\"\noutput_dir = \"out\"\n";
    std::fs::write(&path, text).unwrap();
    let config = Config::load(&path).unwrap();
    assert_eq!(
        config.socket_path,
        std::fs::canonicalize(dir.path()).unwrap().join("sock")
    );

    std::fs::write(&path, format!("{text}unexpected = true\n")).unwrap();
    let error = Config::load(&path).unwrap_err();
    assert_eq!(
        error.code, "config-invalid",
        "an existing file that does not load"
    );
    assert!(error.error.contains("unexpected"), "{}", error.error);

    std::fs::write(&path, "helper_uid = \"502\"\n").unwrap();
    assert!(
        Config::load(&path)
            .unwrap_err()
            .error
            .contains("config.toml")
    );
}

#[test]
fn shipped_example_configuration_parses_and_round_trips() {
    let example = atlas_core::app_bundle::checkout().join("examples/config.toml");
    let config = Config::load(&example).unwrap();
    assert_eq!(config.helper_uid, 502);

    let dir = tempfile::tempdir().unwrap();
    let saved = dir.path().join("saved.toml");
    std::fs::write(&saved, config.to_toml().unwrap()).unwrap();
    assert_eq!(Config::load(&saved).unwrap(), config);
}

#[test]
fn configuration_lookup_distinguishes_absence_from_broken_links() {
    use std::{fs, io::ErrorKind, os::unix::fs::symlink};

    let dir = tempfile::tempdir().unwrap();
    for missing in ["missing.toml", "missing/settings.toml"] {
        assert_eq!(
            Config::load(&dir.path().join(missing)).unwrap_err().code,
            "config"
        );
    }
    let valid = dir.path().join("valid.toml");
    let expected = config(&fs::canonicalize(dir.path()).unwrap());
    fs::write(&valid, expected.to_toml().unwrap()).unwrap();
    assert_eq!(Config::load(&valid).unwrap(), expected);

    let dangling = dir.path().join("dangling.toml");
    symlink(dir.path().join("missing.toml"), &dangling).unwrap();
    assert!(
        fs::symlink_metadata(&dangling)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let looping = dir.path().join("loop");
    symlink(&looping, &looping).unwrap();
    assert_eq!(fs::read_link(&looping).unwrap(), looping);
    let beneath_loop = looping.join("settings.toml");
    assert_ne!(
        fs::symlink_metadata(&beneath_loop).unwrap_err().kind(),
        ErrorKind::NotFound
    );

    for broken in [dangling, beneath_loop] {
        let diagnostic = fs::canonicalize(&broken).unwrap_err().to_string();
        let error = Config::load(&broken).unwrap_err();
        assert_eq!(error.code, "config-invalid", "{}", broken.display());
        assert!(
            error.error.contains(&broken.display().to_string()),
            "{}",
            error.error
        );
        assert!(error.error.contains(&diagnostic), "{}", error.error);
    }
}

#[test]
fn library_construction_cannot_target_the_caller() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config {
        helper_uid: nix::unistd::getuid().as_raw(),
        socket_path: dir.path().join("sock"),
        identity_probe: dir.path().join("probe"),
        input_socket: dir.path().join("input"),
        output_dir: dir.path().join("out"),
        activity: Default::default(),
    };

    assert_eq!(
        Client::new(config).err().map(|error| error.code),
        Some(Code::Identity)
    );
    assert!(!dir.path().join("out").exists());
}

#[test]
fn controllers_for_one_helper_share_the_lock_and_pending_marker() {
    let dir = tempfile::tempdir().unwrap();
    let first = Controller::at(502, dir.path());
    let second = Controller::at(502, dir.path());
    let held = first.acquire(false).unwrap();

    assert_eq!(second.acquire(false).unwrap_err().code, "busy");

    drop(first.quarantine().unwrap());
    drop(held);
    assert_eq!(second.acquire(false).unwrap_err().code, "uncertain");

    let recovery = second.acquire(true).unwrap();
    second.clear().unwrap();
    drop(recovery);
    first.acquire(false).unwrap();
}

#[test]
fn cleanup_completion_cannot_clear_unknown_input_delivery() {
    let dir = tempfile::tempdir().unwrap();
    let controller = Controller::at(502, dir.path());
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
        path: "/tmp/unused-atlas-test".into(),
        timeout: Duration::from_secs(1),
        peer_uid: 502,
    };
    let request = json!({"op":"hello"});
    let future = transport.call(&request);
    assert!(
        std::mem::size_of_val(&future) < 8192,
        "A large receive buffer on the async stack crashed the debug HTTP server"
    );
}

async fn exchange(
    chunks: Vec<Vec<u8>>,
    timeout: Duration,
    pause: Duration,
) -> atlas_core::error::Result<Value> {
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
        peer_uid: current_uid(),
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

#[tokio::test(start_paused = true)]
async fn a_drip_feed_cannot_extend_the_wall_clock_deadline() {
    use std::task::Poll;
    use tokio::{
        io::AsyncBufReadExt,
        sync::{mpsc, oneshot},
        task::JoinSet,
    };

    let (finished, watching) = std::sync::mpsc::channel();
    let watchdog = std::thread::spawn(move || {
        if watching.recv_timeout(Duration::from_secs(20))
            == Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        {
            std::process::abort();
        }
    });
    let dir = tempfile::Builder::new()
        .prefix("ust-")
        .tempdir_in("/tmp")
        .unwrap();
    let socket = dir.path().join("s");
    let listener = UnixListener::bind(&socket).unwrap();
    let mut workers = JoinSet::<()>::new();
    // Keep paused Tokio time from auto-advancing while the real socket reactor
    // establishes readiness. Only this test's explicit advances spend the budget.
    workers.spawn(async {
        loop {
            tokio::task::yield_now().await;
        }
    });
    let (dispatched, heard) = oneshot::channel();
    let (permit, mut allowed) = mpsc::channel(1);
    let (written, mut received) = mpsc::channel(1);
    workers.spawn(async move {
        let (peer, _) = listener.accept().await.unwrap();
        let mut peer = tokio::io::BufReader::new(peer);
        let mut request = String::new();
        assert!(peer.read_line(&mut request).await.unwrap() > 0);
        assert_eq!(
            serde_json::from_str::<Value>(&request).unwrap(),
            json!({"op":"input","actions":[]})
        );
        dispatched.send(()).unwrap();
        while let Some(number) = allowed.recv().await {
            peer.get_mut().write_all(b" ").await.unwrap();
            written.send(number).await.unwrap();
        }
    });
    let transport = Transport {
        path: socket,
        timeout: Duration::from_millis(80),
        peer_uid: current_uid(),
    };
    let request = json!({"op":"input","actions":[]});
    let mut call = Box::pin(transport.call(&request));
    tokio::select! {
        result = &mut call => panic!("the peer has not answered: {result:?}"),
        ready = heard => ready.unwrap(),
    }
    for number in 1..=3 {
        tokio::time::advance(Duration::from_millis(20)).await;
        permit.send(number).await.unwrap();
        tokio::select! {
            result = &mut call => panic!("expired before the total deadline: {result:?}"),
            sent = received.recv() => assert_eq!(sent, Some(number)),
        }
        assert!(
            std::future::poll_fn(|cx| Poll::Ready(call.as_mut().poll(cx)))
                .await
                .is_pending()
        );
    }
    tokio::time::advance(Duration::from_millis(19)).await;
    assert!(
        std::future::poll_fn(|cx| Poll::Ready(call.as_mut().poll(cx)))
            .await
            .is_pending()
    );
    tokio::time::advance(Duration::from_millis(1)).await;
    let result = std::future::poll_fn(|cx| Poll::Ready(call.as_mut().poll(cx))).await;
    let Poll::Ready(Err(error)) = result else {
        panic!("drips extended the total deadline: {result:?}");
    };
    assert_eq!(error.code, atlas_core::error::Code::Transport);
    assert_eq!(error.error, "Helper exceeded the request deadline");
    assert!(error.uncertain, "the complete request was dispatched");
    // The peer was still held open: no EOF or server timeout supplied the failure.
    drop(permit);
    workers.abort_all();
    while let Some(outcome) = workers.join_next().await {
        if let Err(error) = outcome {
            assert!(error.is_cancelled());
        }
    }
    finished.send(()).unwrap();
    watchdog.join().unwrap();
}

#[test]
fn capture_rejects_non_png_and_extreme_dimensions() {
    assert_eq!(png_dimensions(b"not a PNG").unwrap_err().code, "capture");

    let mut header = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    header.extend_from_slice(&u32::MAX.to_be_bytes());
    header.extend_from_slice(&900u32.to_be_bytes());
    header.extend_from_slice(&[0; 9]);
    assert_eq!(png_dimensions(&header).unwrap_err().code, "capture");
}

#[tokio::test]
async fn a_session_changed_screenshot_refusal_reaches_the_caller_with_that_code() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use tokio::io::{AsyncBufReadExt, BufReader};

    let dir = tempfile::Builder::new()
        .prefix("ust-")
        .tempdir_in("/tmp")
        .unwrap();
    let mut client = Client::new(config(dir.path())).unwrap();
    client.transport.peer_uid = current_uid();
    let listener = UnixListener::bind(&client.config.socket_path).unwrap();
    let mut hello = status();
    hello["user"]["uid"] = json!(client.config.helper_uid);
    hello["sessionProof"]["uid"] = json!(client.config.helper_uid);
    let proof = STANDARD.encode(serde_json::to_vec(&hello["sessionProof"]).unwrap());
    let message = "Session changed during capture; observe again";
    let server = tokio::spawn(async move {
        for (op, reply) in [
            ("hello", hello),
            (
                "run",
                json!({"ok": true, "exitCode": 0, "timedOut": false,
                    "events": [{"event": "output", "data": proof}]}),
            ),
            (
                "screenshot",
                json!({"ok": false, "code": "session-changed", "error": message}),
            ),
        ] {
            let (peer, _) = listener.accept().await.unwrap();
            let mut peer = BufReader::new(peer);
            let mut request = String::new();
            peer.read_line(&mut request).await.unwrap();
            assert_eq!(serde_json::from_str::<Value>(&request).unwrap()["op"], op);
            let mut bytes = serde_json::to_vec(&reply).unwrap();
            bytes.push(b'\n');
            peer.get_mut().write_all(&bytes).await.unwrap();
        }
    });

    let error = client.capture(Size::Full, None, false).await.unwrap_err();
    assert_eq!(error.code, "session-changed");
    assert_eq!(error.error, message);
    assert!(!error.uncertain);
    server.await.unwrap();
}

#[tokio::test]
async fn an_older_helper_that_sends_the_whole_display_for_a_crop_is_refused_before_saving() {
    use atlas_core::protocol::{Crop, Region};
    use base64::{Engine, engine::general_purpose::STANDARD};
    use tokio::io::{AsyncBufReadExt, BufReader};

    let dir = tempfile::Builder::new()
        .prefix("ust-")
        .tempdir_in("/tmp")
        .unwrap();
    let mut client = Client::new(config(dir.path())).unwrap();
    client.transport.peer_uid = current_uid();
    let listener = UnixListener::bind(&client.config.socket_path).unwrap();
    let mut hello = status();
    hello["user"]["uid"] = json!(client.config.helper_uid);
    hello["sessionProof"]["uid"] = json!(client.config.helper_uid);
    let proof = STANDARD.encode(serde_json::to_vec(&hello["sessionProof"]).unwrap());
    let server = tokio::spawn(async move {
        let mut asked = Value::Null;
        for (op, reply) in [
            ("hello", hello),
            (
                "run",
                json!({"ok": true, "exitCode": 0, "timedOut": false,
                    "events": [{"event": "output", "data": proof}]}),
            ),
            (
                "screenshot",
                json!({"ok": true, "png": "", "width": 2560, "height": 1440,
                    "scale": 1.0, "offDisplayWindows": []}),
            ),
        ] {
            let (peer, _) = listener.accept().await.unwrap();
            let mut peer = BufReader::new(peer);
            let mut request = String::new();
            peer.read_line(&mut request).await.unwrap();
            let request: Value = serde_json::from_str(&request).unwrap();
            assert_eq!(request["op"], op);
            asked = request;
            let mut bytes = serde_json::to_vec(&reply).unwrap();
            bytes.push(b'\n');
            peer.get_mut().write_all(&bytes).await.unwrap();
        }
        asked
    });

    let region = Crop::Region(Region {
        x: 0,
        y: 0,
        width: 400,
        height: 300,
    });
    let error = client
        .capture(Size::Full, Some(&region), false)
        .await
        .unwrap_err();
    assert_eq!(error.code, "daemon");
    assert!(error.error.contains("atlas restart"), "{}", error.error);
    assert_eq!(
        server.await.unwrap()["region"],
        json!({"x": 0, "y": 0, "width": 400, "height": 300})
    );

    let saved = std::fs::read_dir(&client.config.output_dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "png"))
                .count()
        })
        .unwrap_or(0);
    assert_eq!(saved, 0, "nothing was saved");
}

fn current_uid() -> u32 {
    nix::unistd::getuid().as_raw()
}

#[tokio::test]
async fn transport_refuses_a_socket_served_by_another_account_before_writing() {
    let dir = tempfile::Builder::new()
        .prefix("ust-")
        .tempdir_in("/tmp")
        .unwrap();
    let socket = dir.path().join("s");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = tokio::spawn(async move {
        let (mut peer, _) = listener.accept().await.unwrap();
        let mut received = Vec::new();
        peer.read_to_end(&mut received).await.unwrap();
        received
    });

    let error = Transport {
        path: socket,
        timeout: Duration::from_secs(1),
        peer_uid: current_uid() + 1,
    }
    .call(&json!({"op":"input","actions":[]}))
    .await
    .unwrap_err();
    assert_eq!(error.code, "identity");
    assert!(!error.uncertain, "nothing was dispatched");
    assert!(server.await.unwrap().is_empty());
}

fn config(dir: &std::path::Path) -> Config {
    Config {
        helper_uid: current_uid() + 1,
        socket_path: dir.join("sock"),
        identity_probe: dir.join("probe"),
        input_socket: dir.join("input"),
        output_dir: dir.join("out"),
        activity: Default::default(),
    }
}

#[test]
fn constructing_a_client_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    Client::new(config(dir.path())).unwrap();
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);

    let state = dir.path().join("state");
    Controller::at(502, &state);
    assert!(!state.exists());
}

#[test]
fn output_directory_is_created_private_and_never_repermissioned() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let client = Client::new(config(dir.path())).unwrap();
    let output = client.output_dir().unwrap().to_owned();
    let mode = |path: &std::path::Path| std::fs::metadata(path).unwrap().permissions().mode();
    assert_eq!(mode(&output) & 0o777, 0o700);

    std::fs::set_permissions(&output, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(client.output_dir().unwrap_err().code, "config");
    assert_eq!(mode(&output) & 0o777, 0o755);

    let mut nested = config(dir.path());
    nested.output_dir = dir.path().join("artifacts/runs");

    let output = Client::new(nested)
        .unwrap()
        .output_dir()
        .unwrap()
        .to_owned();
    assert_eq!(mode(&output) & 0o777, 0o700);
    assert_eq!(mode(output.parent().unwrap()) & 0o777, 0o700);
}

fn ready_status() -> Value {
    let mut state = status();
    state["inputService"] = json!({"ok": true, "protocolVersion": 1, "ready": true,
        "accessibility": true, "pid": 77, "version": env!("CARGO_PKG_VERSION"),
        "session": {"ok": true, "uid": 502, "securitySessionID": 100384, "displayID": 28,
        "width": 2560, "height": 1440, "onConsole": false, "hasGraphicAccess": true}});

    state
}

fn check(report: &Value, name: &str) -> Value {
    report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["name"] == name)
        .unwrap()["ok"]
        .clone()
}

#[test]
fn doctor_reports_unobservable_facts_as_unknown_not_missing() {
    let report = diagnosis(
        json!({"ok": false, "code": "transport", "error": "Connection refused"}),
        502,
    )
    .value;
    assert_eq!(report["ready"], false);
    assert_eq!(check(&report, "Helper running"), false);

    for name in [
        "Helper in the background",
        "Screen Recording allowed",
        "Accessibility allowed",
        "Input service ready",
    ] {
        assert_eq!(check(&report, name), Value::Null, "{name}");
    }

    assert_eq!(report["nextSteps"].as_array().unwrap().len(), 1);
}

#[test]
fn doctor_ready_never_implies_native_dialogs() {
    let report = diagnosis(ready_status(), 502).value;
    assert_eq!(report["ready"], true, "{report}");
    assert_eq!(report["capabilities"]["input"], true);
    assert_eq!(report["capabilities"]["nativeDialogs"]["state"], "unknown");
}

#[test]
fn doctor_reads_input_permission_from_the_input_service() {
    let mut state = ready_status();
    state["inputService"]["accessibility"] = json!(false);
    let report = diagnosis(state, 502).value;
    assert_eq!(check(&report, "Accessibility allowed"), false);
    assert_eq!(report["ready"], false);

    let mut state = ready_status();
    state["permissions"]["accessibility"] = json!(false);
    assert_eq!(diagnosis(state, 502).value["ready"], true);
}

#[test]
fn doctor_distinguishes_on_console_from_wrong_account() {
    let guidance = |state: Value| {
        diagnosis(state, 502).value["nextSteps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|step| step.as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
            .join(" ")
    };

    let mut console = ready_status();
    console["session"]["onConsole"] = json!(true);
    assert!(guidance(console).contains("Switch back to your own account"));

    let mut other = ready_status();
    other["user"]["uid"] = json!(503);
    assert!(guidance(other).contains("different account"));

    // Hidden, but its session could not be verified: switching back is no help.
    let mut unproven = ready_status();
    unproven["sessionProof"]["ok"] = json!(false);
    let advice = guidance(unproven);
    assert!(advice.contains("atlas restart"), "{advice}");
    assert!(!advice.contains("Switch back"), "{advice}");
}

#[test]
fn doctor_reports_an_unqueried_input_service_as_unknown() {
    let mut console = status();
    console["session"]["onConsole"] = json!(true);
    let report = diagnosis(console, 502).value;
    assert_eq!(check(&report, "Helper in the background"), false);
    assert_eq!(check(&report, "Accessibility allowed"), Value::Null);
    assert_eq!(check(&report, "Input service ready"), Value::Null);
}

#[test]
fn exclusion_is_per_helper_account() {
    let dir = tempfile::tempdir().unwrap();
    let first = Controller::at(502, dir.path());
    let moved = Controller::at(502, dir.path());
    let other = Controller::at(503, dir.path());
    let held = first.acquire(false).unwrap();

    assert_eq!(moved.acquire(false).unwrap_err().code, "busy");
    drop(other.acquire(false).unwrap());
    drop(first.quarantine().unwrap());
    drop(held);
    assert_eq!(moved.acquire(false).unwrap_err().code, "uncertain");
}

#[test]
fn doctor_warns_when_the_input_service_build_is_not_the_installed_one() {
    let warnings = |service: Value, installed: Value| {
        let mut state = ready_status();
        state["inputService"]["build"] = service;
        state["installedBuild"] = installed;
        diagnosis(state, 502).value["warnings"]
            .as_array()
            .unwrap()
            .len()
    };

    assert_eq!(warnings(json!("abc"), json!("abc")), 0);
    assert_eq!(warnings(json!("abc"), json!("def")), 1);
    assert_eq!(warnings(Value::Null, json!("def")), 1, "pre-build services");
    assert_eq!(warnings(Value::Null, Value::Null), 1);
}

/// Whether `status` shows input service `pid` current in the hidden login
/// of user ID 502.
fn input_current(status: &Value, pid: u32) -> bool {
    Desktop::from(Hidden { uid: 502 }).input_current(&Observation::decode(status), pid)
}

#[test]
fn a_replacement_counts_only_when_ready_current_idle_and_expected() {
    let current = || {
        let mut state = ready_status();
        state["inputService"]["build"] = json!("b");
        state["inputService"]["controlActive"] = json!(false);
        state["inputService"]["heldInput"] = json!(false);
        state["installedBuild"] = json!("b");
        state
    };

    assert!(input_current(&current(), 77));
    assert!(!input_current(&current(), 78), "another PID");

    for (field, value) in [
        ("heldInput", json!(true)),
        ("controlActive", json!(true)),
        ("build", json!("old")),
        ("accessibility", json!(false)),
    ] {
        let mut state = current();
        state["inputService"][field] = value;
        assert!(!input_current(&state, 77), "{field}");
    }

    let mut state = current();
    state["sessionProof"]["onConsole"] = json!(true);
    assert!(!input_current(&state, 77), "visible helper");
}

#[test]
fn an_unreadable_installed_build_is_retried_not_cached() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let client = Client::new(config(dir.path())).unwrap();
    assert_eq!(client.installed_build(), None);

    let probe = dir.path().join("probe");
    std::fs::write(&probe, b"abc").unwrap();
    std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o000)).unwrap();
    assert_eq!(client.installed_build(), None);

    std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        client.installed_build().as_deref(),
        Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
}

#[test]
fn cloned_clients_share_builds_and_notice_a_replaced_executable() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let dir = tempfile::tempdir().unwrap();
    let client = Client::new(config(dir.path())).unwrap();
    let clone = client.clone();
    let probe = dir.path().join("probe");
    std::fs::write(&probe, b"abc").unwrap();
    let original = std::fs::metadata(&probe).unwrap();
    let first = client.installed_build().unwrap();

    std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o000)).unwrap();
    assert_eq!(clone.installed_build(), Some(first.clone()), "shared cache");

    let replacement = dir.path().join("replacement");
    std::fs::write(&replacement, b"def").unwrap();
    std::fs::File::open(&replacement)
        .unwrap()
        .set_modified(original.modified().unwrap())
        .unwrap();
    let second = format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(b"def"));
    assert_ne!(first, second);
    std::fs::rename(&replacement, &probe).unwrap();
    let replaced = std::fs::metadata(&probe).unwrap();
    assert_ne!(original.ino(), replaced.ino());
    assert_eq!(original.size(), replaced.size());
    assert_eq!(original.modified().unwrap(), replaced.modified().unwrap());

    assert_eq!(clone.installed_build(), Some(second.clone()));
    assert_eq!(client.installed_build(), Some(second));
}

#[test]
fn a_settled_service_need_not_be_current_but_a_current_one_must_be_settled() {
    let mut state = ready_status();
    state["inputService"]["controlActive"] = json!(false);
    state["inputService"]["heldInput"] = json!(false);
    state["inputService"]["build"] = json!("old");
    state["installedBuild"] = json!("new");
    assert!(settled(&state, 77));
    assert!(!input_current(&state, 77));

    state["inputService"]["heldInput"] = json!(true);
    assert!(!settled(&state, 77));

    state["inputService"]["heldInput"] = json!(false);
    assert!(!settled(&state, 76), "another PID");
}

#[test]
fn pending_generations_are_listed_until_settled() {
    let dir = tempfile::tempdir().unwrap();
    let controller = Controller::at(502, dir.path());
    assert!(controller.pending().unwrap().is_empty());

    let first = controller.quarantine().unwrap();
    let second = controller.quarantine().unwrap();
    assert_eq!(
        controller.pending().unwrap(),
        vec![first.to_string(), second.to_string()]
    );

    controller.clear_if(&first).unwrap();
    assert_eq!(controller.pending().unwrap(), vec![second.to_string()]);
}

#[test]
fn only_generations_nothing_will_settle_are_unresolved() {
    let dir = tempfile::tempdir().unwrap();
    let controller = Controller::at(502, dir.path());
    // Another controller instance stands in for another process: claims are
    // file locks, so it sees exactly what a separate process would.
    let observer = Controller::at(502, dir.path());
    let working = controller.quarantine().unwrap();

    assert_eq!(observer.pending().unwrap().len(), 1);
    assert!(
        observer.unresolved().unwrap().is_empty(),
        "work in progress is not an unknown outcome"
    );

    let abandoned = controller.quarantine().unwrap();
    let lost = abandoned.to_string();
    drop(abandoned);
    assert_eq!(
        observer.unresolved().unwrap(),
        vec![lost.clone()],
        "an uncertain failure leaves an entry nothing will settle"
    );
    assert_eq!(
        controller.quarantine_as(&working).unwrap_err().code,
        "busy",
        "a generation is claimed once"
    );

    controller.clear_if(&working).unwrap();
    drop(working);

    // A crashed writer leaves its claim file behind, unlocked.
    let crashed = atlas_core::files::token();
    let claim = controller.quarantine_as(&crashed).unwrap();
    let find = |suffix: &str| {
        std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.to_string_lossy().ends_with(suffix))
            .unwrap()
    };
    let live = find(&format!(".{crashed}.live"));
    std::fs::hard_link(&live, dir.path().join("kept")).unwrap();
    drop(claim);
    std::fs::rename(dir.path().join("kept"), &live).unwrap();

    // An entry this version did not write is never taken as in progress.
    let marker = find(".uncertain.json");
    let mut entries: Vec<String> =
        serde_json::from_slice(&std::fs::read(&marker).unwrap()).unwrap();
    entries.push("1234.legacy".into());
    std::fs::write(&marker, serde_json::to_vec(&entries).unwrap()).unwrap();

    assert_eq!(
        observer.unresolved().unwrap(),
        vec![lost, crashed.clone(), "1234.legacy".to_owned()]
    );
    assert_eq!(
        controller.quarantine_as("not/a token").unwrap_err().code,
        "state"
    );

    // Settling one generation removes its abandoned claim file too.
    controller.clear_if(&crashed).unwrap();
    assert!(!live.exists(), "settling removes an abandoned claim file");

    // A writer that crashed before publishing leaves an unlisted claim file;
    // recovery sweeps it but never a claim a live mutation holds.
    let name = marker
        .file_name()
        .unwrap()
        .to_string_lossy()
        .replace(".uncertain.json", "");
    let orphan = dir
        .path()
        .join(format!("{name}.{}.live", atlas_core::files::token()));
    std::fs::write(&orphan, b"").unwrap();

    let held = controller.quarantine().unwrap();
    controller.clear().unwrap();
    assert_eq!(controller.pending().unwrap(), vec![held.to_string()]);
    assert!(
        !orphan.exists(),
        "recovery sweeps unlisted abandoned claims"
    );
    assert!(
        find(&format!(".{}.live", &*held)).exists(),
        "a held claim is never swept"
    );

    // A claim whose file was replaced never removes the replacement.
    let path = find(&format!(".{}.live", &*held));
    std::fs::remove_file(&path).unwrap();
    let replacement = controller.quarantine_as(&held).unwrap();
    drop(held);
    assert!(path.exists(), "the replacement claim survives");
    assert!(observer.unresolved().unwrap().is_empty());

    drop(replacement);
    assert!(!path.exists());
}

/// The usual reasons the helper cannot be used keep codes agents act on.
#[test]
fn unusable_helpers_say_why_with_their_own_codes() {
    let code = |state: Value| require_hidden(&state, 502).unwrap_err().code;

    let mut console = status();
    console["session"]["onConsole"] = json!(true);
    assert_eq!(code(console), "on-console");

    let other = json!({"ok": false, "code": "identity", "error": "Socket owned by 503"});
    assert_eq!(code(other), "identity");

    let silent = json!({"ok": false, "code": "transport", "error": "No answer"});
    assert_eq!(code(silent), "transport");

    let mut wrong = status();
    wrong["user"]["uid"] = json!(503);
    assert_eq!(code(wrong), "identity");
}
