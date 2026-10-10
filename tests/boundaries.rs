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
    session::{Display, diagnose, input_current, input_settled, require_hidden},
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
        json!({"type":"click","x":1,"y":2,"modifiers":vec!["cmd"; 6]}),
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
    assert_eq!(error.code, "config");
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
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/config.toml");
    let config = Config::load(&example).unwrap();
    assert_eq!(config.helper_uid, 502);
    let dir = tempfile::tempdir().unwrap();
    let saved = dir.path().join("saved.toml");
    std::fs::write(&saved, config.to_toml().unwrap()).unwrap();
    assert_eq!(Config::load(&saved).unwrap(), config);
}

#[test]
fn a_missing_configuration_points_at_a_legacy_json_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("understudy.local.toml");
    assert!(
        !Config::load(&path)
            .unwrap_err()
            .error
            .contains("no longer read")
    );
    std::fs::write(dir.path().join("understudy.local.json"), "{}").unwrap();
    assert!(
        Config::load(&path)
            .unwrap_err()
            .error
            .contains("no longer read")
    );

    // A TOML file beside its legacy JSON twin is refused until the twin is gone.
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/config.toml");
    std::fs::copy(example, &path).unwrap();
    assert!(Config::load(&path).unwrap_err().error.contains("Both"));
    std::fs::remove_file(dir.path().join("understudy.local.json")).unwrap();
    Config::load(&path).unwrap();
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

#[tokio::test]
async fn lease_expiry_keeps_flock_until_the_action_settles() {
    let dir = tempfile::tempdir().unwrap();
    let controller = Controller::at(502, dir.path());
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
        path: "/tmp/unused-understudy-test".into(),
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

#[tokio::test]
async fn old_owner_cannot_release_a_new_lease() {
    let dir = tempfile::tempdir().unwrap();
    let controller = Controller::at(502, dir.path());
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
    let controller = Controller::at(502, dir.path());
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

fn current_uid() -> u32 {
    // SAFETY: getuid has no pointer arguments or preconditions.
    unsafe { libc::getuid() }
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
    let report = diagnose(
        json!({"ok": false, "code": "transport", "error": "Connection refused"}),
        502,
    );
    assert_eq!(report["ready"], false);
    assert_eq!(check(&report, "Helper connection"), false);
    for name in [
        "Hidden desktop",
        "Screen recording",
        "Input permission",
        "Native input service",
    ] {
        assert_eq!(check(&report, name), Value::Null, "{name}");
    }
    assert_eq!(report["nextSteps"].as_array().unwrap().len(), 1);
}

#[test]
fn doctor_ready_never_implies_native_dialogs() {
    let report = diagnose(ready_status(), 502);
    assert_eq!(report["ready"], true, "{report}");
    assert_eq!(report["capabilities"]["input"], true);
    assert_eq!(report["capabilities"]["nativeDialogs"]["state"], "unknown");
}

#[test]
fn doctor_reads_input_permission_from_the_input_service() {
    let mut state = ready_status();
    state["inputService"]["accessibility"] = json!(false);
    let report = diagnose(state, 502);
    assert_eq!(check(&report, "Input permission"), false);
    assert_eq!(report["ready"], false);
    let mut state = ready_status();
    state["permissions"]["accessibility"] = json!(false);
    assert_eq!(diagnose(state, 502)["ready"], true);
}

#[test]
fn doctor_distinguishes_on_console_from_wrong_account() {
    let guidance = |state: Value| {
        diagnose(state, 502)["nextSteps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|step| step.as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut console = ready_status();
    console["session"]["onConsole"] = json!(true);
    assert!(guidance(console).contains("on the console"));
    let mut other = ready_status();
    other["user"]["uid"] = json!(503);
    assert!(guidance(other).contains("different account"));
}

#[test]
fn doctor_reports_an_unqueried_input_service_as_unknown() {
    let mut console = status();
    console["session"]["onConsole"] = json!(true);
    let report = diagnose(console, 502);
    assert_eq!(check(&report, "Hidden desktop"), false);
    assert_eq!(check(&report, "Input permission"), Value::Null);
    assert_eq!(check(&report, "Native input service"), Value::Null);
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
        diagnose(state, 502)["warnings"].as_array().unwrap().len()
    };
    assert_eq!(warnings(json!("abc"), json!("abc")), 0);
    assert_eq!(warnings(json!("abc"), json!("def")), 1);
    assert_eq!(warnings(Value::Null, json!("def")), 1, "pre-build services");
    assert_eq!(warnings(Value::Null, Value::Null), 1);
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
    assert!(input_current(&current(), 502, 77));
    assert!(!input_current(&current(), 502, 78), "another PID");
    for (field, value) in [
        ("heldInput", json!(true)),
        ("controlActive", json!(true)),
        ("build", json!("old")),
        ("accessibility", json!(false)),
    ] {
        let mut state = current();
        state["inputService"][field] = value;
        assert!(!input_current(&state, 502, 77), "{field}");
    }
    let mut state = current();
    state["sessionProof"]["onConsole"] = json!(true);
    assert!(!input_current(&state, 502, 77), "visible helper");
}

#[test]
fn an_unreadable_installed_build_is_retried_not_cached() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let client = Client::new(config(dir.path())).unwrap();
    assert_eq!(client.installed_build(), None);
    std::fs::write(dir.path().join("probe"), b"abc").unwrap();
    let probe = dir.path().join("probe");
    std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o000)).unwrap();
    assert_eq!(client.installed_build(), None);
    std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        client.installed_build().as_deref(),
        Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
}

#[test]
fn a_settled_service_need_not_be_current_but_a_current_one_must_be_settled() {
    let mut state = ready_status();
    state["inputService"]["controlActive"] = json!(false);
    state["inputService"]["heldInput"] = json!(false);
    state["inputService"]["build"] = json!("old");
    state["installedBuild"] = json!("new");
    assert!(input_settled(&state, 502, 77));
    assert!(!input_current(&state, 502, 77));
    state["inputService"]["heldInput"] = json!(true);
    assert!(!input_settled(&state, 502, 77));
    state["inputService"]["heldInput"] = json!(false);
    assert!(!input_settled(&state, 502, 76), "another PID");
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
    let crashed = understudy::control::token();
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
        .join(format!("{name}.{}.live", understudy::control::token()));
    std::fs::write(&orphan, b"").unwrap();
    let held = controller.quarantine().unwrap();
    controller.clear().unwrap();
    assert!(controller.pending().unwrap().is_empty());
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

#[test]
fn a_stale_release_is_a_no_op_but_never_ends_another_lease() {
    let dir = tempfile::tempdir().unwrap();
    let controller = Controller::at(502, dir.path());
    let mut lease = Lease::default();
    let old = lease.acquire(&controller).unwrap();
    // Revoked elsewhere (the menu bar); the tab's own release follows.
    assert_eq!(lease.begin_release(None).unwrap(), Some(old.clone()));
    lease.end_release();
    assert_eq!(lease.begin_release(Some(&old)).unwrap(), None);
    let current = lease.acquire(&controller).unwrap();
    assert_eq!(
        lease.begin_release(Some(&old)).unwrap_err().code,
        "lease-lost"
    );
    lease.require(&current).unwrap();
}
