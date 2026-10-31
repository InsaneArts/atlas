use super::*;
use crate::{
    host_session::{Host, HostSession, store::Store},
    overview::service_blocker,
};
use serde_json::json;

fn session(id: &str) -> HostSession {
    HostSession::new(Host::Named, id).unwrap()
}

fn seen(client: &Client, session: &HostSession, at: u64, ended: bool) {
    Store::at(client.output_dir().unwrap())
        .update(session, at, &client.controller, |record| {
            record.last_seen = at;
            record.ended = ended.then_some(at);
        })
        .unwrap();
}

#[test]
fn service_changes_defer_for_two_minutes_but_not_for_the_callers_own_session() {
    let directory = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(directory.path());
    let own = session("updater");
    let other = session("worker");
    let now = crate::host_session::now_ms();
    seen(&client, &own, now, false);
    client.host_session = Some(own);
    assert!(service_blocker(&client, false, u128::from(now) * 1_000_000).is_none());

    seen(&client, &other, now, false);
    assert_eq!(
        service_blocker(&client, false, u128::from(now + 120_000) * 1_000_000).as_deref(),
        Some("worker is active in the helper.")
    );
    assert!(service_blocker(&client, false, u128::from(now + 120_001) * 1_000_000).is_none());
    seen(&client, &other, now, true);
    assert!(service_blocker(&client, false, u128::from(now) * 1_000_000).is_none());
}

#[test]
fn service_changes_find_active_sessions_beyond_the_eight_shown_in_status() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let now = crate::host_session::now_ms();
    for index in 0..10 {
        let host = session(&format!("idle-{index}"));
        seen(&client, &host, now - 180_000, false);
        let abandoned = client
            .controller
            .clone()
            .labelled(Some(&host))
            .quarantine()
            .unwrap();
        drop(abandoned);
    }
    seen(&client, &session("worker"), now, false);
    assert_eq!(
        service_blocker(&client, false, u128::from(now) * 1_000_000).as_deref(),
        Some("worker is active in the helper.")
    );
}

#[test]
fn service_changes_keep_recent_sessions_whose_process_state_is_unknown() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let host = session("worker");
    let now = crate::host_session::now_ms();
    let process = crate::process::Instance::new(0, [1, 0]);
    assert_eq!(process.state(), crate::process::State::Unknown);
    Store::at(client.output_dir().unwrap())
        .update(&host, now, &client.controller, |record| {
            record.host_process = Some(process);
        })
        .unwrap();
    assert_eq!(
        service_blocker(&client, false, u128::from(now) * 1_000_000).as_deref(),
        Some("worker is active in the helper.")
    );
    assert!(service_blocker(&client, false, u128::from(now + 120_001) * 1_000_000).is_none());
    seen(&client, &host, now, true);
    assert!(service_blocker(&client, false, u128::from(now) * 1_000_000).is_none());
}

#[test]
fn service_changes_find_terminal_activity_before_many_newer_actions() {
    use crate::activity_log::{Op, Row, append};

    let directory = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(directory.path());
    let own = session("updater");
    let now = crate::host_session::now_ms();
    client.host_session = Some(own.clone());
    for index in 0..=65 {
        append(
            client.output_dir().unwrap(),
            &Row {
                at: Some((u128::from(now + index) * 1_000_000).to_string()),
                operation: Some(Op::Screenshot),
                ok: Some(true),
                session: (index > 0).then(|| own.key().to_string()),
                ..Default::default()
            },
        )
        .unwrap();
    }
    assert_eq!(
        service_blocker(&client, false, u128::from(now + 120_000) * 1_000_000).as_deref(),
        Some("Terminal is active in the helper.")
    );
    assert!(service_blocker(&client, false, u128::from(now + 120_001) * 1_000_000).is_none());
}

#[test]
fn service_changes_refuse_incomplete_activity_reads() {
    use crate::activity_log::{FILE, PREVIOUS};
    use std::{fs, os::unix::fs::symlink};

    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let output = client.output_dir().unwrap();
    let now = super::super::timestamp();
    assert!(service_blocker(&client, false, now).is_none());
    for name in [FILE, PREVIOUS] {
        let path = output.join(name);
        for contents in [
            "broken JSON\n".to_owned(),
            json!({"operation": "screenshot"}).to_string() + "\n",
            " ".repeat(4 * 1024 * 1024 + 1),
        ] {
            fs::write(&path, contents).unwrap();
            assert!(
                service_blocker(&client, false, now)
                    .unwrap()
                    .contains("could not read every activity row")
            );
        }
        fs::remove_file(&path).unwrap();
        symlink(&path, &path).unwrap();
        assert!(
            service_blocker(&client, false, now)
                .unwrap()
                .contains("could not read every activity row")
        );
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(service_blocker(&client, false, now).is_some());
        fs::remove_dir(&path).unwrap();
        nix::unistd::mkfifo(&path, nix::sys::stat::Mode::S_IRUSR).unwrap();
        assert!(service_blocker(&client, false, now).is_some());
        fs::remove_file(&path).unwrap();
    }
    assert!(service_blocker(&client, false, now).is_none());
}

#[test]
fn rotated_activity_must_cover_the_service_idle_window() {
    use crate::activity_log::{FILE, PREVIOUS};
    use std::fs;

    let directory = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(directory.path());
    let own = session("updater");
    client.host_session = Some(own.clone());
    let now = crate::host_session::now_ms();
    let row = |at| {
        format!(
            "{}\n",
            json!({"operation": "screenshot", "at": (u128::from(at) * 1_000_000).to_string(),
                "session": own.key()})
        )
    };
    let output = client.output_dir().unwrap();
    fs::write(output.join(FILE), row(now)).unwrap();
    fs::write(output.join(PREVIOUS), row(now - 60_000)).unwrap();
    assert!(
        service_blocker(&client, false, u128::from(now) * 1_000_000)
            .unwrap()
            .contains("could not read every activity row")
    );
    fs::write(output.join(PREVIOUS), row(now - 120_000)).unwrap();
    assert!(service_blocker(&client, false, u128::from(now) * 1_000_000).is_none());
}

#[tokio::test]
async fn a_terminal_can_update_after_its_completed_screenshot() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let now = crate::host_session::now_ms();
    crate::activity_log::append(
        client.output_dir().unwrap(),
        &crate::activity_log::Row {
            at: Some((u128::from(now) * 1_000_000).to_string()),
            operation: Some(crate::activity_log::Op::Screenshot),
            ok: Some(true),
            actor: Some("agent".into()),
            verb: Some("screenshot".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let ownership = tokio::time::timeout(Duration::from_secs(1), client.service_change(false))
        .await
        .unwrap()
        .unwrap();
    drop(ownership);

    let mut named = client.clone();
    named.host_session = Some(session("updater"));
    assert_eq!(
        service_blocker(&named, false, u128::from(now) * 1_000_000).as_deref(),
        Some("Terminal is active in the helper.")
    );
    seen(&client, &session("worker"), now, false);
    assert_eq!(
        service_blocker(&client, false, u128::from(now) * 1_000_000).as_deref(),
        Some("worker is active in the helper.")
    );
}

#[tokio::test]
async fn recovery_can_take_control_after_the_callers_screenshot_with_an_unknown_outcome() {
    for own in [None, Some(session("updater"))] {
        let directory = tempfile::tempdir().unwrap();
        let mut client = crate::test_support::client(directory.path());
        client.controller = client.controller.labelled(own.as_ref());
        let abandoned = client.controller.quarantine().unwrap();
        let generation = abandoned.generation().to_owned();
        drop(abandoned);
        let now = crate::host_session::now_ms();
        if let Some(own) = &own {
            seen(&client, own, now, false);
        }
        client.host_session = own;
        crate::activity_log::append(
            client.output_dir().unwrap(),
            &crate::activity_log::Row {
                at: Some((u128::from(now) * 1_000_000).to_string()),
                operation: Some(crate::activity_log::Op::Screenshot),
                ok: Some(true),
                actor: Some("agent".into()),
                verb: Some("screenshot".into()),
                session: client
                    .host_session
                    .as_ref()
                    .map(|own| own.key().to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        let listed = crate::overview::live_host_sessions(
            &client,
            crate::overview::SessionFacts::local(&client.controller, u128::from(now) * 1_000_000),
        )
        .await;
        assert_eq!(listed.len(), 1);
        if client.host_session.is_some() {
            assert_eq!(listed[0]["state"], "needsYou");
        } else {
            assert_eq!(listed[0]["waitingFor"], "recovery");
        }
        assert!(service_blocker(&client, false, u128::from(now) * 1_000_000).is_none());
        assert_eq!(
            client.service_change(false).await.unwrap_err().code,
            Code::Uncertain
        );
        let ownership = tokio::time::timeout(Duration::from_secs(1), client.service_change(true))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(client.controller.pending().unwrap(), [generation]);
        assert_eq!(
            client.controller.acquire(true).unwrap_err().code,
            Code::Busy
        );
        drop(ownership);
    }
}

#[tokio::test]
async fn only_recovery_of_the_callers_unknown_outcome_can_pass_recent_work_and_waiters() {
    for own in [None, Some(session("recovering"))] {
        for waiter in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let mut client = crate::test_support::client(directory.path());
            client.host_session = own.clone();
            client.controller = client.controller.labelled(own.as_ref());
            let other = session("worker");
            seen(&client, &other, crate::host_session::now_ms(), false);
            if waiter {
                let holder = client
                    .controller
                    .clone()
                    .labelled(Some(&other))
                    .acquire(true)
                    .unwrap();
                crate::test_support::waiting_note(&directory.path().join("state"), Some(&other));
                drop(holder);
                assert_eq!(client.controller.checked_waiting_notes().unwrap().len(), 1);
            }
            let abandoned = client.controller.quarantine().unwrap();
            let generation = abandoned.generation().to_owned();
            drop(abandoned);
            assert!(!client.controller.pending_causes().unwrap()[0].running);

            let update = tokio::time::timeout(Duration::from_secs(1), client.service_change(false))
                .await
                .unwrap()
                .unwrap_err();
            assert_eq!(update.code, Code::Busy);
            assert_eq!(update.error, "worker is active in the helper.");
            let ownership =
                tokio::time::timeout(Duration::from_secs(1), client.service_change(true))
                    .await
                    .unwrap()
                    .unwrap();
            assert_eq!(client.controller.pending().unwrap(), [generation]);
            assert_eq!(
                client.controller.acquire(true).unwrap_err().code,
                Code::Busy
            );
            if waiter {
                assert_eq!(client.controller.checked_waiting_notes().unwrap().len(), 1);
            }
            drop(ownership);
        }
    }
}

#[tokio::test]
async fn recovery_replaces_stale_input_during_restart_despite_recent_work_and_waiters() {
    use serde_json::Value;
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::UnixListener,
    };

    for (own, other_owner, through_app) in [
        (None, false, false),
        (Some(session("recovering")), false, false),
        (None, true, false),
        (None, true, true),
    ] {
        for waiter in [false, true] {
            if other_owner && waiter {
                continue;
            }
            let directory = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
            let mut client = crate::test_support::client(directory.path());
            client.host_session = own.clone();
            client.controller = client.controller.labelled(own.as_ref());
            client.transport.peer_uid = nix::unistd::getuid().as_raw();
            client.input_peer_uid = Some(client.transport.peer_uid);
            std::fs::write(&client.config.identity_probe, b"installed build").unwrap();
            let build = client.installed_build().unwrap();
            let other = session("worker");
            let at = crate::host_session::now_ms() - if other_owner { 120_001 } else { 0 };
            seen(&client, &other, at, false);
            if waiter {
                let holder = client
                    .controller
                    .clone()
                    .labelled(Some(&other))
                    .acquire(true)
                    .unwrap();
                crate::test_support::waiting_note(&directory.path().join("state"), Some(&other));
                drop(holder);
            }
            let controller = client.controller.clone().labelled(if other_owner {
                Some(&other)
            } else {
                own.as_ref()
            });
            drop(controller.quarantine().unwrap());
            let pending = client.controller.pending().unwrap();
            assert_eq!(pending.len(), 1);

            std::fs::create_dir_all(client.transport.path.parent().unwrap()).unwrap();
            let daemon = UnixListener::bind(&client.transport.path).unwrap();
            let mut input = UnixListener::bind(&client.config.input_socket).unwrap();
            let previous_pid = i32::MAX as u32;
            assert!(!super::super::service::process_exists(previous_pid));
            let mut capture_pid = 123;
            let mut replaced = false;
            let mut changes = Vec::new();
            let serve = async {
                loop {
                    let (native, (peer, _)) = tokio::select! {
                        peer = daemon.accept() => (false, peer.unwrap()),
                        peer = input.accept() => (true, peer.unwrap()),
                    };
                    let mut peer = BufReader::new(peer);
                    let mut line = String::new();
                    peer.read_line(&mut line).await.unwrap();
                    let request: Value = serde_json::from_str(&line).unwrap();
                    let proof = json!({
                        "ok": true, "uid": client.config.helper_uid,
                        "hasGraphicAccess": true, "onConsole": false,
                        "parentPID": capture_pid, "securitySessionID": 42,
                        "displayID": 7, "width": 100, "height": 100,
                    });
                    let reply = match (native, request["op"].as_str().unwrap()) {
                        (false, "hello") => json!({
                            "ok": true, "user": {"uid": client.config.helper_uid},
                            "daemon": {"pid": capture_pid},
                            "session": {"managerName": "Aqua", "onConsole": false},
                            "display": {"width": 100, "height": 100, "scale": 1},
                        }),
                        (true, "hello") => json!({
                            "ok": true, "pid": if replaced { 456 } else { previous_pid },
                            "protocolVersion": 1, "accessibility": true,
                            "ready": capture_pid == 123 || replaced,
                            "session": proof, "controlActive": false, "heldInput": false,
                            "build": if replaced { build.as_str() } else { "old build" },
                            "capabilities": [crate::session::SHUTDOWN],
                        }),
                        (false, "job") => {
                            json!({"ok": true, "op": "job", "action": "show", "jobs": []})
                        }
                        (false, "run") if request["argv"][1] == "session-info" => {
                            crate::test_support::completed(&proof.to_string())
                        }
                        (true, "reset") => {
                            assert_eq!(request["epoch"]["processId"], previous_pid);
                            changes.push("reset");
                            json!({"ok": true, "released": true})
                        }
                        (false, "restart") => {
                            assert_eq!(
                                client.controller.acquire(true).unwrap_err().code,
                                Code::Busy
                            );
                            assert!(pending.iter().all(|claim| {
                                client.controller.pending().unwrap().contains(claim)
                            }));
                            changes.push("restart");
                            capture_pid = 124;
                            json!({"ok": true})
                        }
                        (true, "shutdown") => {
                            assert_eq!(request["pid"], previous_pid);
                            assert_eq!(
                                client.controller.acquire(true).unwrap_err().code,
                                Code::Busy
                            );
                            changes.push("shutdown");
                            std::fs::remove_file(&client.config.input_socket).unwrap();
                            json!({"ok": true})
                        }
                        (false, "run") => {
                            assert_eq!(request["argv"][1], "-c");
                            assert_eq!(
                                client.controller.acquire(true).unwrap_err().code,
                                Code::Busy
                            );
                            changes.push("spawn");
                            input = UnixListener::bind(&client.config.input_socket).unwrap();
                            replaced = true;
                            crate::test_support::completed("456\n")
                        }
                        _ => panic!("unexpected request: {request}"),
                    };
                    peer.get_mut()
                        .write_all(format!("{reply}\n").as_bytes())
                        .await
                        .unwrap();
                }
            };
            let outcome = tokio::time::timeout(Duration::from_secs(5), async {
                tokio::select! {
                    outcome = async {
                        if through_app {
                            Ok(crate::viewer::recover_from_app(client.clone()).await)
                        } else {
                            client.restart().await
                        }
                    } => outcome,
                    _ = serve => unreachable!(),
                }
            })
            .await
            .unwrap()
            .unwrap();
            assert_eq!(outcome["ok"], true);
            assert_eq!(outcome["inputReplaced"], true);
            assert_eq!(outcome["session"]["daemon"]["pid"], 124);
            assert!(crate::session::input_current(
                &outcome["session"],
                client.config.helper_uid,
                456,
            ));
            assert_eq!(changes, ["reset", "restart", "shutdown", "spawn"]);
            assert!(client.controller.pending().unwrap().is_empty());
            assert!(client.controller.holder_note().is_none());
            if waiter {
                assert_eq!(client.controller.checked_waiting_notes().unwrap().len(), 1);
            }
        }
    }
}

#[tokio::test]
async fn an_ordinary_input_upgrade_still_refuses_recent_work_with_the_callers_unknown_outcome() {
    let directory = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(directory.path());
    client.host_session = Some(session("recovering"));
    client.controller = client.controller.labelled(client.host_session.as_ref());
    drop(client.controller.quarantine().unwrap());
    let pending = client.controller.pending().unwrap();
    seen(
        &client,
        &session("worker"),
        crate::host_session::now_ms(),
        false,
    );
    let _ownership = client.controller.acquire(true).unwrap();

    let error = tokio::time::timeout(Duration::from_secs(1), client.upgrade_input(&json!({})))
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, Code::Busy);
    assert_eq!(error.error, "worker is active in the helper.");
    assert!(!error.uncertain);
    assert_eq!(client.controller.pending().unwrap(), pending);
}

#[tokio::test]
async fn recovery_of_the_callers_unknown_outcome_still_refuses_a_running_command() {
    let directory = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(directory.path());
    client.host_session = Some(session("recovering"));
    client.controller = client.controller.labelled(client.host_session.as_ref());
    drop(client.controller.quarantine().unwrap());
    let other = client.controller.clone().labelled(Some(&session("worker")));
    let command = other.quarantine_command().unwrap();
    let pending = client.controller.pending().unwrap();

    let error = tokio::time::timeout(Duration::from_secs(1), client.service_change(true))
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, Code::Busy);
    assert_eq!(
        error.error,
        "worker has a command still running in the helper."
    );
    assert_eq!(client.controller.pending().unwrap(), pending);
    other.clear_if(&command).unwrap();
}

#[tokio::test]
async fn recovery_of_another_sessions_unknown_outcome_uses_the_ordinary_idle_guard() {
    for own in [None, Some(session("recovering"))] {
        for pending_session in [None, Some(session("worker"))] {
            if own.is_none() && pending_session.is_none() {
                continue;
            }
            let directory = tempfile::tempdir().unwrap();
            let mut client = crate::test_support::client(directory.path());
            client.host_session = own.clone();
            client.controller = client.controller.labelled(own.as_ref());
            drop(
                client
                    .controller
                    .clone()
                    .labelled(pending_session.as_ref())
                    .quarantine()
                    .unwrap(),
            );
            let pending = client.controller.pending().unwrap();
            let worker = session("worker");
            let now = crate::host_session::now_ms();
            seen(&client, &worker, now, false);
            let error = tokio::time::timeout(Duration::from_secs(1), client.service_change(true))
                .await
                .unwrap()
                .unwrap_err();
            assert_eq!(error.code, Code::Busy);
            assert_eq!(error.error, "worker is active in the helper.");
            assert_eq!(client.controller.pending().unwrap(), pending);
            assert!(client.controller.holder_note().is_none());

            seen(&client, &worker, now - 120_001, false);
            let ownership =
                tokio::time::timeout(Duration::from_secs(1), client.service_change(true))
                    .await
                    .unwrap()
                    .unwrap();
            assert_eq!(client.controller.pending().unwrap(), pending);
            drop(ownership);
            assert!(client.controller.holder_note().is_none());
        }
    }
}

#[tokio::test]
async fn recovery_refuses_when_pending_state_cannot_be_read() {
    let directory = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(directory.path());
    client.host_session = Some(session("recovering"));
    client.controller = client.controller.labelled(client.host_session.as_ref());
    drop(client.controller.quarantine().unwrap());
    let marker = std::fs::read_dir(directory.path().join("state"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.to_string_lossy().ends_with(".uncertain.json"))
        .unwrap();
    std::fs::remove_file(&marker).unwrap();
    std::fs::create_dir(&marker).unwrap();

    let error = tokio::time::timeout(Duration::from_secs(1), client.service_change(true))
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, Code::Busy);
    assert!(error.error.contains("could not check pending activity"));
    assert!(marker.is_dir());
    assert!(client.controller.holder_note().is_none());
}

#[tokio::test]
async fn a_named_updater_defers_for_terminal_work_before_its_own_screenshot() {
    use crate::activity_log::{Op, Row};

    let directory = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(directory.path());
    let own = session("updater");
    let now = crate::host_session::now_ms();
    let terminal_at = now - 60_000;
    let screenshot_at = now - 30_000;
    seen(&client, &own, screenshot_at, false);
    client.controller = client.controller.labelled(Some(&own));
    client.host_session = Some(own.clone());
    for (at, operation, verb, session) in [
        (terminal_at, Op::Input, "click", None),
        (
            screenshot_at,
            Op::Screenshot,
            "screenshot",
            Some(own.key().as_str().to_owned()),
        ),
    ] {
        crate::activity_log::append(
            client.output_dir().unwrap(),
            &Row {
                at: Some((u128::from(at) * 1_000_000).to_string()),
                operation: Some(operation),
                ok: Some(true),
                actor: Some("agent".into()),
                verb: Some(verb.into()),
                session,
                ..Default::default()
            },
        )
        .unwrap();
    }
    let listed = crate::overview::live_host_sessions(
        &client,
        crate::overview::SessionFacts::local(&client.controller, u128::from(now) * 1_000_000),
    )
    .await;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["key"], own.key().as_str());

    let error = tokio::time::timeout(Duration::from_secs(1), client.service_change(false))
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, Code::Busy);
    assert_eq!(error.error, "Terminal is active in the helper.");
    assert!(client.controller.holder_note().is_none());
    assert_eq!(
        service_blocker(
            &client,
            false,
            u128::from(terminal_at + 120_000) * 1_000_000
        )
        .as_deref(),
        Some("Terminal is active in the helper.")
    );
    assert!(
        service_blocker(
            &client,
            false,
            u128::from(terminal_at + 120_001) * 1_000_000
        )
        .is_none()
    );
}

#[test]
fn a_holder_on_its_first_command_defers_service_changes_until_it_lets_go() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let host = session("worker");
    let holder = client
        .controller
        .clone()
        .labelled(Some(&host))
        .acquire(false)
        .unwrap();
    assert_eq!(
        service_blocker(&client, false, super::super::timestamp()).as_deref(),
        Some("worker is active in the helper.")
    );
    drop(holder);
    assert!(service_blocker(&client, false, super::super::timestamp()).is_none());
}

#[test]
fn a_running_command_defers_service_changes_even_when_its_session_went_quiet() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let host = session("worker");
    let now = crate::host_session::now_ms();
    seen(&client, &host, now - 7_200_000, false);
    let controller = client.controller.clone().labelled(Some(&host));
    let command = controller.quarantine_command().unwrap();
    assert_eq!(
        service_blocker(&client, false, u128::from(now) * 1_000_000).as_deref(),
        Some("worker has a command still running in the helper.")
    );
    controller.clear_if(&command).unwrap();
    drop(command);
    assert!(service_blocker(&client, false, u128::from(now) * 1_000_000).is_none());
}

#[test]
fn an_unnamed_running_command_is_named_terminal_without_an_activity_row() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let command = client.controller.quarantine_command().unwrap();
    assert_eq!(
        service_blocker(&client, false, super::super::timestamp()).as_deref(),
        Some("Terminal has a command still running in the helper.")
    );
    client.controller.clear_if(&command).unwrap();
}

#[tokio::test]
async fn an_unnamed_waiter_without_activity_defers_named_and_unnamed_updaters() {
    let directory = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(directory.path());
    client.host_session = Some(session("updater"));
    client.controller = client.controller.labelled(client.host_session.as_ref());
    let ownership = client.service_change(false).await.unwrap();
    drop(client.controller.quarantine().unwrap());
    crate::test_support::waiting_note(&directory.path().join("state"), None);
    assert!(crate::activity_log::recent(client.output_dir().unwrap(), 1).is_empty());
    for host_session in [client.host_session.clone(), None] {
        client.host_session = host_session;
        client.controller = client.controller.labelled(client.host_session.as_ref());
        let error = tokio::time::timeout(Duration::from_secs(1), client.require_service_idle())
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(error.code, Code::Busy);
        assert_eq!(error.error, "Terminal is active in the helper.");
        assert_eq!(
            client.controller.acquire(false).unwrap_err().code,
            Code::Busy
        );
    }
    drop(ownership);
}

#[tokio::test]
async fn incomplete_waiter_reads_refuse_even_with_no_session_records() {
    use std::{
        fs,
        os::unix::fs::{PermissionsExt, symlink},
    };

    let directory = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(directory.path());
    client.host_session = Some(session("updater"));
    client.controller = client.controller.labelled(client.host_session.as_ref());
    let ownership = client.service_change(false).await.unwrap();
    let holder = fs::read_dir(directory.path().join("state"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.to_string_lossy().ends_with(".holder.json"))
        .unwrap();
    let waiting = holder.with_file_name(
        holder
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .replace(".holder.json", ".waiting"),
    );
    let refused = async || {
        let error = tokio::time::timeout(Duration::from_secs(1), client.require_service_idle())
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(error.code, Code::Busy);
        assert!(error.error.contains("could not read every waiting command"));
        assert!(
            client
                .controller
                .holder_note()
                .unwrap()
                .process
                .is_this_one()
        );
    };
    fs::create_dir(&waiting).unwrap();
    fs::set_permissions(&waiting, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read_dir(&waiting).is_err() {
        refused().await;
    }
    fs::set_permissions(&waiting, fs::Permissions::from_mode(0o700)).unwrap();
    fs::remove_dir(&waiting).unwrap();
    symlink(&waiting, &waiting).unwrap();
    refused().await;
    fs::remove_file(&waiting).unwrap();
    fs::create_dir(&waiting).unwrap();
    let note = waiting.join("0000000000000001.json");
    for bytes in [b"broken JSON".as_slice(), &[b' '; 4097]] {
        fs::write(&note, bytes).unwrap();
        refused().await;
    }
    fs::remove_file(&note).unwrap();
    symlink(&note, &note).unwrap();
    refused().await;
    fs::remove_file(&note).unwrap();
    for index in 0..4096 {
        fs::write(waiting.join(format!("unrelated-{index}")), "").unwrap();
    }
    refused().await;
    fs::remove_dir_all(&waiting).unwrap();
    client.require_service_idle().await.unwrap();
    drop(ownership);
}

#[tokio::test]
async fn a_locked_activity_marker_refuses_within_a_deadline_even_without_pending_changes() {
    use fs2::FileExt;
    use std::fs;

    for pending in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let client = crate::test_support::client(directory.path());
        client.require_service_idle().await.unwrap();
        if pending {
            drop(client.controller.quarantine().unwrap());
        }
        let marker_lock = fs::read_dir(directory.path().join("state"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.to_string_lossy().ends_with(".state.lock"))
            .unwrap();
        let lock = crate::files::open_lock(&marker_lock).unwrap();
        FileExt::try_lock_exclusive(&lock).unwrap();
        let result =
            tokio::time::timeout(Duration::from_secs(2), client.require_service_idle()).await;
        // Release before asserting so even an unbounded-reader regression
        // cannot leave runtime shutdown waiting for its blocking task.
        drop(lock);
        let error = result
            .expect("activity checks must stop waiting for the marker lock")
            .unwrap_err();
        assert_eq!(error.code, Code::Busy);
        assert!(error.error.contains("pending activity"));
        assert!(error.error.contains("in time"));
        assert_eq!(
            client.controller.pending().unwrap().len(),
            usize::from(pending)
        );
    }
}

#[tokio::test]
async fn allowing_disruption_still_bounds_the_marker_lock_during_controller_acquisition() {
    use fs2::FileExt;
    use std::fs;

    for recovery in [true, false] {
        for pending in [true, false] {
            let directory = tempfile::tempdir().unwrap();
            let mut client = crate::test_support::client(directory.path());
            client.require_service_idle().await.unwrap();
            client.allow_disrupt = true;
            client.host_session = Some(session("updater"));
            client.controller = client.controller.labelled(client.host_session.as_ref());
            if pending {
                drop(client.controller.quarantine().unwrap());
            }
            let marker_lock = fs::read_dir(directory.path().join("state"))
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .find(|path| path.to_string_lossy().ends_with(".state.lock"))
                .unwrap();
            let lock = crate::files::open_lock(&marker_lock).unwrap();
            FileExt::try_lock_exclusive(&lock).unwrap();
            let waiting = client.clone();
            let mut change = tokio::task::spawn_blocking(move || {
                tokio::runtime::Handle::current().block_on(waiting.service_change(recovery))
            });
            let result = tokio::time::timeout(Duration::from_secs(2), &mut change).await;
            // Release before asserting so a blocking acquisition regression
            // cannot leave runtime shutdown waiting for its task.
            drop(lock);
            let error = result
                .expect("service changes must stop waiting for the marker lock")
                .unwrap()
                .unwrap_err();
            assert_eq!(error.code, Code::Busy);
            assert!(error.error.contains("pending activity"));
            assert!(error.error.contains("in time"));
            assert!(client.controller.holder_note().is_none());
            assert_eq!(
                client.controller.pending().unwrap().len(),
                usize::from(pending)
            );
            // A refusal releases controller exclusion as well as the marker lock.
            drop(client.controller.acquire(true).unwrap());
        }
    }
}

#[tokio::test]
async fn a_missing_sessions_folder_is_idle_but_an_unreadable_scan_refuses() {
    use std::{
        fs,
        os::unix::fs::{PermissionsExt, symlink},
    };

    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let sessions = client.output_dir().unwrap().join("sessions");
    client.require_service_idle().await.unwrap();
    fs::create_dir(&sessions).unwrap();
    fs::set_permissions(&sessions, fs::Permissions::from_mode(0o000)).unwrap();
    let unreadable = fs::read_dir(&sessions).is_err();
    let result = client.require_service_idle().await;
    fs::set_permissions(&sessions, fs::Permissions::from_mode(0o700)).unwrap();
    // Root can read mode-000 directories; the malformed-path checks below
    // exercise refusals under that UID too.
    if unreadable {
        assert_eq!(result.unwrap_err().code, Code::Busy);
    }
    let host = session("worker");
    let record = sessions.join(format!("{}.json", host.key()));
    let idle = crate::host_session::record::Record::new(&host, 0, None);
    fs::write(&record, serde_json::to_vec(&idle).unwrap()).unwrap();
    assert_eq!(Store::at(client.output_dir().unwrap()).recent().len(), 1);
    for unreadable in [false, true] {
        if unreadable {
            symlink(&record, &record).unwrap();
        } else {
            // Overwriting leaves the directory unchanged: the cached healthy
            // record cannot establish that this later read is complete.
            fs::write(&record, "broken JSON").unwrap();
        }
        let error = tokio::time::timeout(Duration::from_secs(1), client.service_change(false))
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(error.code, Code::Busy);
        assert!(error.error.contains("could not read every session record"));
        assert!(client.controller.holder_note().is_none());
        fs::remove_file(&record).unwrap();
    }
    fs::remove_dir(&sessions).unwrap();
    // A loop is inaccessible rather than absent, regardless of test UID.
    symlink(&sessions, &sessions).unwrap();
    let error = client.require_service_idle().await.unwrap_err();
    assert_eq!(error.code, Code::Busy);
    assert!(error.error.contains("could not read every session record"));
    fs::remove_file(&sessions).unwrap();
    client.require_service_idle().await.unwrap();
}

#[tokio::test]
async fn truncated_session_records_and_exhausted_directory_scans_refuse() {
    use std::fs;

    for crowded in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let client = crate::test_support::client(directory.path());
        let sessions = client.output_dir().unwrap().join("sessions");
        fs::create_dir(&sessions).unwrap();
        if crowded {
            for index in 0..crate::record_folder::LISTED {
                fs::write(sessions.join(format!("unrelated-{index}")), "").unwrap();
            }
        } else {
            for index in 0..=crate::host_session::store::MAX_RECORDS {
                let host = session(&format!("worker-{index}"));
                let record = crate::host_session::record::Record::new(&host, 0, None);
                fs::write(
                    sessions.join(format!("{}.json", host.key())),
                    serde_json::to_vec(&record).unwrap(),
                )
                .unwrap();
            }
        }
        let error = tokio::time::timeout(Duration::from_secs(1), client.require_service_idle())
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(error.code, Code::Busy);
        assert!(error.error.contains("could not read every session record"));
    }
}

#[tokio::test]
async fn recovery_names_a_running_command_and_leaves_it_pending() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let command = client.controller.quarantine_command().unwrap();
    let error = client.restart().await.unwrap_err();
    assert_eq!(error.code, Code::Busy);
    assert_eq!(error.holder, Some(crate::error::Holder::Agent));
    assert_eq!(
        error.error,
        "Terminal has a command still running in the helper."
    );
    assert_eq!(client.controller.pending().unwrap(), [command.generation()]);
    client.controller.clear_if(&command).unwrap();
}

#[tokio::test]
async fn a_restart_refuses_recent_work_before_contacting_the_helper() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    seen(
        &client,
        &session("worker"),
        crate::host_session::now_ms(),
        false,
    );
    let error = tokio::time::timeout(Duration::from_secs(1), client.restart())
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, Code::Busy);
    assert_eq!(error.error, "worker is active in the helper.");
    assert!(
        error.details["next"]
            .as_str()
            .unwrap()
            .contains("--allow-disrupt")
    );
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn allowing_disruption_preserves_controller_exclusion_and_pending_changes() {
    let directory = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(directory.path());
    client.allow_disrupt = true;
    seen(
        &client,
        &session("worker"),
        crate::host_session::now_ms(),
        false,
    );
    let ownership = client.service_change(false).await.unwrap();
    assert_eq!(
        client.service_change(false).await.unwrap_err().code,
        Code::Busy
    );
    drop(ownership);

    let command = client.controller.quarantine_command().unwrap();
    assert_eq!(
        client.service_change(true).await.unwrap_err().code,
        Code::Busy
    );
    assert_eq!(client.controller.pending().unwrap(), [command.generation()]);
    client.controller.clear_if(&command).unwrap();
    drop(command);

    let abandoned = client.controller.quarantine().unwrap();
    let generation = abandoned.generation().to_owned();
    drop(abandoned);
    assert_eq!(
        client.service_change(false).await.unwrap_err().code,
        Code::Uncertain
    );
    assert_eq!(client.controller.pending().unwrap(), [generation]);
}

#[tokio::test]
async fn the_callers_own_session_can_update_after_its_running_command_ends() {
    let directory = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(directory.path());
    let host = session("updater");
    seen(&client, &host, crate::host_session::now_ms(), false);
    client.controller = client.controller.clone().labelled(Some(&host));
    client.host_session = Some(host);
    let command = client.controller.quarantine_command().unwrap();
    let error = client.require_service_idle().await.unwrap_err();
    assert_eq!(
        error.error,
        "updater has a command still running in the helper."
    );
    client.controller.clear_if(&command).unwrap();
    drop(command);
    let ownership = tokio::time::timeout(Duration::from_secs(1), client.service_change(false))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        client.controller.holder_note().unwrap().session,
        client.host_session.as_ref().map(|host| host.key().clone())
    );
    drop(ownership);
}

#[tokio::test]
async fn input_replacement_and_setup_repairs_refuse_recent_work_before_dispatch() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    seen(
        &client,
        &session("worker"),
        crate::host_session::now_ms(),
        false,
    );
    let ownership = client.controller.acquire(true).unwrap();
    let config = directory.path().join("missing.toml");
    let checked = async {
        assert_eq!(
            client.upgrade_input(&json!({})).await.unwrap_err().error,
            "worker is active in the helper."
        );
        assert_eq!(
            crate::setup::migrate::apply(&config, &client, &ownership)
                .await
                .unwrap_err()
                .code,
            Code::Busy
        );
        assert_eq!(
            crate::setup::migrate::apply_fresh(&config, &client, &ownership)
                .await
                .unwrap_err()
                .code,
            Code::Busy
        );
        assert_eq!(
            crate::setup::migrate::rollback(&config, &client, &ownership)
                .await
                .unwrap_err()
                .code,
            Code::Busy
        );
    };
    tokio::time::timeout(Duration::from_secs(1), checked)
        .await
        .unwrap();
    assert!(client.controller.pending().unwrap().is_empty());
}

fn job(state: &str) -> Shown {
    serde_json::from_value(json!({
        "id": "7c2e91a04b3f", "name": "web", "task": null, "session": null,
        "uid": 7001, "boot": "B", "pid": 4312, "start": [1761650000, 123456],
        "pgid": 4312, "state": state, "startedAt": 10, "endsAt": 20, "logResets": 0,
    }))
    .unwrap()
}

#[test]
fn background_jobs_block_service_changes_until_they_have_ended() {
    for state in ["starting", "launched", "running", "stopping"] {
        assert_eq!(
            job_blocker(&[job(state)]).as_deref(),
            Some("Background job web is active in the helper."),
            "{state}"
        );
    }
    assert!(job_blocker(&[]).is_none());
    assert!(job_blocker(&[job("ended")]).is_none());
    assert!(job_blocker(&[Shown::Later(json!({"state": "unknown"}))]).is_some());
}

#[tokio::test(start_paused = true)]
async fn checking_jobs_has_a_deadline_and_does_not_treat_a_blocked_read_as_idle() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let error = client
        .require_jobs_idle(std::future::pending())
        .await
        .unwrap_err();
    assert_eq!(error.code, Code::Busy);
    assert!(error.error.contains("in time"));
    let error = client
        .require_jobs_idle(async { Err(Error::new(Code::Blocked, "This shell may not connect")) })
        .await
        .unwrap_err();
    assert_eq!(error.code, Code::Busy);
    assert!(error.error.contains("This shell may not connect"));
    let error = client
        .require_jobs_idle(async {
            Err(Error::new(
                Code::Job,
                "Atlas could not read every background job record; a job may still be running.",
            ))
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, Code::Busy);
    assert!(error.error.contains("may still be running"));
    client
        .require_jobs_idle(async { Err(Error::new(Code::Daemon, crate::jobs::OLDER)) })
        .await
        .unwrap();
}

#[tokio::test]
async fn only_an_absent_daemon_directory_exempts_a_first_setup_from_checking_jobs() {
    use std::{fs, os::unix::fs::symlink};

    let directory = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(directory.path());
    // First setup has already bootstrapped the shared executable.
    fs::write(&client.config.identity_probe, "installed executable").unwrap();
    let daemon = client.transport.path.parent().unwrap();
    assert!(!daemon.exists());
    drop(client.service_change(false).await.unwrap());

    fs::create_dir(daemon).unwrap();
    fs::create_dir(daemon.join("jobs")).unwrap();
    fs::write(daemon.join("jobs/7c2e91a04b3f.json"), "unreadable job").unwrap();
    let error = tokio::time::timeout(Duration::from_secs(1), client.service_change(false))
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, Code::Busy);
    assert!(error.error.contains("could not check background jobs"));
    assert!(client.controller.holder_note().is_none());

    // A dangling directory link also cannot prove that setup is fresh.
    fs::remove_dir_all(daemon).unwrap();
    symlink(directory.path().join("missing"), daemon).unwrap();
    assert_eq!(
        client.require_service_idle().await.unwrap_err().code,
        Code::Busy
    );
    fs::remove_file(daemon).unwrap();
    fs::create_dir(daemon).unwrap();
    client.allow_disrupt = true;
    drop(client.service_change(false).await.unwrap());
}

#[tokio::test]
async fn losing_the_socket_during_a_job_read_never_establishes_idle() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    std::fs::create_dir(client.transport.path.parent().unwrap()).unwrap();
    // Only the endpoint's disappearance matters; no server needs to run.
    std::fs::write(&client.transport.path, "endpoint").unwrap();
    let error = client
        .require_jobs_idle(async {
            std::fs::remove_file(&client.transport.path).unwrap();
            Err(Error::new(Code::Io, "The daemon shut down during the read"))
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, Code::Busy);
    assert!(error.error.contains("The daemon shut down during the read"));
}
