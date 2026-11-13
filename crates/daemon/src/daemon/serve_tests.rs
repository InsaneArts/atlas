use super::*;
use crate::test_support::child::Processes;
use crate::test_support::{session, synthetic};
use atlas_core::protocol::{Crop, JobRequest, Region};
use atlas_core::screen::windows::{Bounds, Dock, Window};
use std::os::fd::AsRawFd;
use tokio::io::{AsyncBufReadExt, BufReader};

#[tokio::test]
async fn an_abandoned_read_does_not_degrade_the_service() {
    let daemon = daemon(false);
    let slots = Arc::new(Semaphore::new(1));
    for request in [b"{\n".as_slice(), b"{\"op\":\"windows\"}\n"] {
        let (mut client, server) = UnixStream::pair().unwrap();
        client.write_all(request).await.unwrap();
        nix::sys::socket::shutdown(server.as_raw_fd(), nix::sys::socket::Shutdown::Write).unwrap();
        let (phase, observed) = watch::channel(ConnectionPhase::Refusal);
        let mut tasks = JoinSet::new();
        let slot = slots.clone().acquire_owned().await.unwrap();
        let daemon = daemon.clone();
        let task = tasks.spawn(async move {
            let _slot = slot;
            handle(server, daemon, true, phase).await
        });
        let mut connections = Connections::from([(task.id(), observed)]);
        let outcome = tasks.join_next_with_id().await.unwrap();
        assert!(matches!(outcome, Ok((_, Err(_)))));
        let phase = *connections.values().next().unwrap().borrow();
        assert!(matches!(phase, ConnectionPhase::Refusal) == (request == b"{\n"));
        connection_outcome(outcome, &mut connections, false).unwrap();
        assert!(connections.is_empty());
        assert_eq!(slots.available_permits(), 1);
    }
    let reply = exchange(daemon, r#"{"op":"screenshot"}"#).await;
    assert_eq!(reply[0]["ok"], true);
}

#[tokio::test]
async fn a_handler_panic_is_observed() {
    let mut daemon = Daemon::new(
        "Aqua".into(),
        synthetic,
        || panic!("private handler cause"),
        session,
    );
    daemon.screen.windows = fake_windows;
    let (mut client, server) = UnixStream::pair().unwrap();
    client.write_all(b"{\"op\":\"windows\"}\n").await.unwrap();
    let (phase, observed) = watch::channel(ConnectionPhase::Refusal);
    let mut tasks = JoinSet::new();
    let task = tasks.spawn(handle(server, Arc::new(daemon), true, phase));
    let mut connections = Connections::from([(task.id(), observed)]);
    let outcome = tasks.join_next_with_id().await.unwrap();
    assert!(outcome.as_ref().unwrap_err().is_panic());
    let error = connection_outcome(outcome, &mut connections, false).unwrap_err();
    assert_eq!(error.code, Code::Job);
    assert!(error.uncertain);
    assert!(connections.is_empty());
    for diagnostic in [json!(error).to_string(), format!("{error:?}")] {
        assert!(!diagnostic.contains("private handler cause"));
        assert!(diagnostic.len() < 256);
    }
}

#[tokio::test]
async fn a_lost_mutation_reply_keeps_recovery_pending() {
    let daemon = daemon(false);
    let (mut client, server) = UnixStream::pair().unwrap();
    client.write_all(b"{\"op\":\"restart\"}\n").await.unwrap();
    nix::sys::socket::shutdown(server.as_raw_fd(), nix::sys::socket::Shutdown::Write).unwrap();
    let (phase, observed) = watch::channel(ConnectionPhase::Refusal);
    let mut tasks = JoinSet::new();
    let task = tasks.spawn(handle(server, daemon.clone(), true, phase));
    let mut connections = Connections::from([(task.id(), observed)]);
    let error = connection_outcome(
        tasks.join_next_with_id().await.unwrap(),
        &mut connections,
        false,
    )
    .unwrap_err();
    assert_eq!(error.code, Code::Transport);
    assert!(error.uncertain);
    assert_eq!(
        error.error,
        "Daemon restart reply was lost; outcome unknown"
    );
    // The failed reply did not suppress the requested effect or supply
    // an acknowledgement on the caller's behalf.
    let notified = daemon.restarting.notified();
    tokio::pin!(notified);
    std::future::poll_fn(|cx| {
        assert!(std::future::Future::poll(notified.as_mut(), cx).is_ready());
        std::task::Poll::Ready(())
    })
    .await;
}

/// A name a change may give itself.
const CHANGE: &str = "0123456789abcdef0123456789abcdef0123456789abcdef";

/// What `daemon` says the change named [`CHANGE`] came to.
async fn outcome(daemon: &Arc<Daemon>) -> Value {
    let request = format!(r#"{{"op":"outcome","change":"{CHANGE}"}}"#);
    exchange(daemon.clone(), &request).await.remove(0)
}

#[tokio::test]
async fn a_change_that_named_itself_is_remembered_when_its_reply_is_lost() {
    let daemon = daemon(false);
    assert_eq!(
        outcome(&daemon).await,
        json!({"ok": true, "outcome": "unknown"})
    );

    let (mut client, server) = UnixStream::pair().unwrap();
    client
        .write_all(format!("{{\"op\":\"restart\",\"change\":\"{CHANGE}\"}}\n").as_bytes())
        .await
        .unwrap();
    nix::sys::socket::shutdown(server.as_raw_fd(), nix::sys::socket::Shutdown::Write).unwrap();
    let phase = watch::channel(ConnectionPhase::Refusal).0;
    let lost = handle(server, daemon.clone(), true, phase).await;
    assert!(lost.is_err(), "its reply could not be written");

    assert_eq!(
        outcome(&daemon).await,
        json!({"ok": true, "outcome": "done", "reply": {"ok": true, "op": "restart"}})
    );
}

#[tokio::test]
async fn a_restart_cut_before_it_happened_is_not_remembered_as_done() {
    let daemon = daemon(false);
    // The client reads nothing, and the kernel takes no more for it: the
    // reply can only wait.
    let (client, server) = std::os::unix::net::UnixStream::pair().unwrap();
    server.set_nonblocking(true).unwrap();
    let mut filled = 0;
    let full = loop {
        match std::io::Write::write(&mut &server, &[0; 65536]) {
            Ok(written) => filled += written,
            Err(error) => break error,
        }
    };
    assert!(filled > 0);
    assert_eq!(full.kind(), std::io::ErrorKind::WouldBlock);
    let (_reader, mut writer) = UnixStream::from_std(server).unwrap().into_split();
    let change = atlas_core::files::Token::parse(CHANGE).unwrap();
    let remembering = daemon.outcomes.begin(change);
    let restarted = AtomicBool::new(false);
    {
        let acknowledging = acknowledge(&mut writer, "restart", remembering, || {
            restarted.store(true, Ordering::SeqCst);
        });
        tokio::pin!(acknowledging);
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(
            acknowledging.as_mut().poll(&mut context).is_pending(),
            "it waits to write its reply"
        );
        // Cut there, as a handler that is stopped is.
    }

    assert!(!restarted.load(Ordering::SeqCst));
    assert_eq!(
        outcome(&daemon).await,
        json!({"ok": true, "outcome": "unknown"})
    );
    drop(client);
}

#[tokio::test]
async fn a_change_asked_for_twice_is_carried_out_once() {
    let daemon = daemon(false);
    let request =
        format!(r#"{{"op":"run","argv":["/usr/bin/true"],"timeoutMs":60000,"change":"{CHANGE}"}}"#);
    let first = exchange(daemon.clone(), &request).await;
    assert_eq!(first.last().unwrap()["exitCode"], 0);

    let second = exchange(daemon.clone(), &request).await;
    assert_eq!(
        second,
        [json!({
            "ok": false,
            "code": "bad-request",
            "error": "The helper was already asked for this change; ask for its outcome",
            "uncertain": false,
        })]
    );
    assert_eq!(
        outcome(&daemon).await,
        json!({"ok": true, "outcome": "done", "reply": first.last().unwrap()})
    );
}

#[tokio::test]
async fn a_command_under_way_is_running_until_the_daemon_knows_what_it_came_to() {
    let daemon = daemon(false);
    let (mut client, server) = UnixStream::pair().unwrap();
    let phase = watch::channel(ConnectionPhase::Refusal).0;
    let served = tokio::spawn(handle(server, daemon.clone(), true, phase));
    let request = format!(
        r#"{{"op":"run","argv":["/usr/bin/tail","-f","/dev/null"],"timeoutMs":60000,"change":"{CHANGE}"}}"#
    );
    client
        .write_all(format!("{request}\n").as_bytes())
        .await
        .unwrap();
    let mut lines = BufReader::new(client).lines();
    let started: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!(started["event"], "started");
    assert_eq!(
        outcome(&daemon).await,
        json!({"ok": true, "outcome": "running"})
    );

    // The client leaves, which ends the command: the daemon remembers
    // that it cannot say how far it got.
    drop(lines);
    assert!(served.await.unwrap().is_err());
    let remembered = outcome(&daemon).await;
    assert_eq!(remembered["outcome"], "done");
    assert_eq!(remembered["reply"]["ok"], false);
    assert_eq!(remembered["reply"]["uncertain"], true);
}

#[tokio::test]
async fn a_change_refused_while_the_helper_is_on_screen_is_remembered_as_refused() {
    let daemon = daemon(true);
    let request = format!(r#"{{"op":"logout","change":"{CHANGE}"}}"#);
    let refusal = exchange(daemon.clone(), &request).await.remove(0);
    assert_eq!(refusal["code"], "on-console");

    assert_eq!(
        outcome(&daemon).await,
        json!({"ok": true, "outcome": "done", "reply": refusal})
    );
}

#[tokio::test]
async fn a_read_that_names_a_change_is_not_remembered_and_may_be_asked_again() {
    let daemon = daemon(false);
    let request = format!(r#"{{"op":"windows","change":"{CHANGE}"}}"#);
    for _ in 0..2 {
        assert_eq!(exchange(daemon.clone(), &request).await[0]["ok"], true);
    }

    assert_eq!(
        outcome(&daemon).await,
        json!({"ok": true, "outcome": "unknown"})
    );
}

#[tokio::test]
async fn only_the_controller_is_told_what_a_change_came_to() {
    let request = format!(r#"{{"op":"outcome","change":"{CHANGE}"}}"#);
    let refusal = exchange_as(daemon(false), &request, false).await.remove(0);
    assert_eq!(refusal["code"], "permission");
}

#[tokio::test]
async fn a_lost_run_reply_does_not_refuse_the_next_command() {
    let daemon = daemon(false);
    let previous = daemon
        .runs
        .lock()
        .await
        .spawn(async { Err(Error::new(Code::Transport, "private abandoned transport")) });
    while !previous.is_finished() {
        tokio::task::yield_now().await;
    }

    let replies = exchange(
        daemon.clone(),
        r#"{"op":"run","argv":["/usr/bin/true"],"timeoutMs":60000}"#,
    )
    .await;
    assert_eq!(replies.last().unwrap()["ok"], true);
    assert_eq!(replies.last().unwrap()["exitCode"], 0);
    let error = drain_runs(&daemon, tokio::time::Instant::now() + STOP)
        .await
        .unwrap_err();
    assert_eq!(error.code, Code::Transport);
    assert!(error.uncertain);
    assert!(daemon.runs.lock().await.is_empty());
}

#[tokio::test]
async fn a_queued_command_rechecks_admission_before_it_starts() {
    use std::sync::atomic::{AtomicU8, Ordering};
    static STATE: AtomicU8 = AtomicU8::new(0);

    tokio::time::timeout(Duration::from_secs(8), async {
        for state in 1..=3 {
            STATE.store(0, Ordering::SeqCst);
            let daemon = Arc::new(Daemon::new(
                "Aqua".into(),
                synthetic,
                || STATE.load(Ordering::SeqCst) == 1,
                || {
                    let mut proof = session()?;
                    match STATE.load(Ordering::SeqCst) {
                        2 => proof["securitySessionID"] = json!(43),
                        3 => proof["displayID"] = json!(8),
                        _ => {}
                    }
                    Ok(proof)
                },
            ));
            let folder = tempfile::tempdir().unwrap();
            let effect = folder.path().join("effect");
            let request = json!({
                "op": "run",
                "argv": ["/usr/bin/touch", effect],
                "session": {"securitySessionId": 42, "displayId": 7},
            });
            let (mut client, server) = UnixStream::pair().unwrap();
            let (phase, mut observed) = watch::channel(ConnectionPhase::Refusal);
            let runs = daemon.runs.lock().await;
            let served = tokio::spawn(handle(server, daemon.clone(), true, phase));
            client
                .write_all(format!("{request}\n").as_bytes())
                .await
                .unwrap();
            observed
                .wait_for(|phase| matches!(phase, ConnectionPhase::Mutation("run")))
                .await
                .unwrap();
            STATE.store(state, Ordering::SeqCst);
            drop(runs);

            let mut bytes = Vec::new();
            client.read_to_end(&mut bytes).await.unwrap();
            served.await.unwrap().unwrap();
            let refusal = if state == 1 {
                failure(
                    Code::OnConsole,
                    "The helper is on the screen; switch back to your own account first",
                )
            } else {
                failure(
                    Code::SessionChanged,
                    "The helper changed since Atlas checked it; look again",
                )
            };
            assert_eq!(bytes, format!("{refusal}\n").as_bytes());
            assert!(matches!(*observed.borrow(), ConnectionPhase::Refusal));
            assert!(!effect.exists(), "nothing ran before refusal");
            drain_runs(&daemon, tokio::time::Instant::now() + STOP)
                .await
                .unwrap();
        }
    })
    .await
    .expect("queued admission did not finish");
}

#[tokio::test]
async fn a_failed_run_event_writer_still_reaps_the_command() {
    let mut processes = Processes::default();
    tokio::time::timeout(Duration::from_secs(8), async {
        let daemon = daemon(false);
        let (mut client, server) = UnixStream::pair().unwrap();
        let fault = nix::unistd::dup(&server).unwrap();
        let (phase, observed) = watch::channel(ConnectionPhase::Refusal);
        let mut tasks = JoinSet::new();
        let task = tasks.spawn(handle(server, daemon, true, phase));
        let mut connections = Connections::from([(task.id(), observed)]);
        client
            .write_all(b"{\"op\":\"run\",\"argv\":[\"/bin/sh\",\"-c\",\"while :; do printf streamed; done\"],\"timeoutMs\":60000}\n")
            .await
            .unwrap();
        let mut lines = BufReader::new(client).lines();
        let started: Value =
            serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        processes.started(&started);
        let group = nix::unistd::Pid::from_raw(started["pid"].as_i64().unwrap() as i32);
        // Leave the client connected: only a failed streamed write can
        // abandon the producer and end this otherwise unbounded command.
        nix::sys::socket::shutdown(fault.as_raw_fd(), nix::sys::socket::Shutdown::Write).unwrap();
        let outcome = tasks.join_next_with_id().await.unwrap();
        let error = connection_outcome(outcome, &mut connections, false).unwrap_err();
        assert_eq!(error.code, Code::Transport);
        assert!(error.uncertain);
        assert!(nix::sys::signal::killpg(group, None).is_err());
    })
    .await
    .expect("streamed writer cleanup did not finish");
}

#[tokio::test]
async fn aborting_a_run_handler_keeps_its_command_registered_until_reaped() {
    let mut processes = Processes::default();
    tokio::time::timeout(Duration::from_secs(8), async {
        let daemon = daemon(false);
        let (mut client, server) = UnixStream::pair().unwrap();
        let (phase, observed) = watch::channel(ConnectionPhase::Refusal);
        let mut tasks = JoinSet::new();
        let task = tasks.spawn(handle(server, daemon.clone(), true, phase));
        let abort = task.clone();
        let mut connections = Connections::from([(task.id(), observed)]);
        client
            .write_all(b"{\"op\":\"run\",\"argv\":[\"/bin/sleep\",\"30\"],\"timeoutMs\":60000}\n")
            .await
            .unwrap();
        let mut lines = BufReader::new(client).lines();
        let started: Value =
            serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        processes.started(&started);
        let group = nix::unistd::Pid::from_raw(started["pid"].as_i64().unwrap() as i32);
        abort.abort();
        let outcome = tasks.join_next_with_id().await.unwrap();
        assert!(outcome.as_ref().unwrap_err().is_cancelled());
        connection_outcome(outcome, &mut connections, true).unwrap();
        assert!(!daemon.runs.lock().await.is_empty());
        let error = drain_runs(&daemon, tokio::time::Instant::now() + STOP)
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::Transport);
        assert!(error.uncertain);
        assert!(daemon.runs.lock().await.is_empty());
        assert!(nix::sys::signal::killpg(group, None).is_err());
    })
    .await
    .expect("aborted handler cleanup did not finish");
}

#[tokio::test]
async fn cancelling_a_handler_joins_its_blocked_event_writer() {
    let mut processes = Processes::default();
    tokio::time::timeout(Duration::from_secs(8), async {
        let daemon = daemon(false);
        let (mut client, server) = UnixStream::pair().unwrap();
        nix::sys::socket::setsockopt(&server, nix::sys::socket::sockopt::SndBuf, &4096).unwrap();
        let (phase, observed) = watch::channel(ConnectionPhase::Refusal);
        let mut tasks = JoinSet::new();
        let task = tasks.spawn(handle(server, daemon.clone(), true, phase));
        let mut connections = Connections::from([(task.id(), observed)]);
        client
            .write_all(b"{\"op\":\"run\",\"argv\":[\"/usr/bin/head\",\"-c\",\"65536\",\"/dev/zero\"],\"timeoutMs\":60000}\n")
            .await
            .unwrap();
        let mut reader = BufReader::new(client);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let started: Value = serde_json::from_str(&line).unwrap();
        processes.started(&started);
        let group = nix::unistd::Pid::from_raw(started["pid"].as_i64().unwrap() as i32);
        let mut first = [0];
        reader.read_exact(&mut first).await.unwrap();
        assert_eq!(first, [b'{']);
        // The child has settled, while its output exceeds the socket's
        // buffer. Keep the peer connected without draining that output.
        while nix::sys::signal::killpg(group, None).is_ok() {
            tokio::task::yield_now().await;
        }
        assert!(!task.is_finished());
        tokio::time::pause();
        let cancelled = tokio::time::Instant::now();
        task.abort();
        connection_outcome(
            tasks.join_next_with_id().await.unwrap(),
            &mut connections,
            true,
        )
        .unwrap();
        let error = drain_runs(&daemon, tokio::time::Instant::now() + STOP)
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::Transport);
        assert!(error.uncertain);
        assert_eq!(tokio::time::Instant::now(), cancelled);
        assert!(daemon.runs.lock().await.is_empty());
        drop(reader);
    })
    .await
    .expect("blocked event writer cleanup did not finish");
}

#[tokio::test(start_paused = true)]
async fn shutdown_classifies_cancelled_handlers_and_joins_owned_work() {
    let daemon = daemon(false);
    let mut tasks = JoinSet::new();
    let mut connections = Connections::new();
    let slots = Arc::new(Semaphore::new(1));
    let slot = slots.clone().acquire_owned().await.unwrap();
    let (phase, observed) = watch::channel(ConnectionPhase::Mutation("run"));
    let (left, mut gone) = oneshot::channel::<()>();
    let idle = tasks.spawn(async move {
        let _slot = slot;
        let _phase = phase;
        let _left = left;
        std::future::pending::<Result<()>>().await
    });
    connections.insert(idle.id(), observed);
    let (resume, barrier) = oneshot::channel::<()>();
    let run = daemon.runs.lock().await.spawn(async move {
        barrier.await.unwrap();
        Ok(())
    });
    let deadline = tokio::time::Instant::now() + STOP;
    let error = tokio::time::timeout(STOP * 2, async {
        drain_connections(&mut tasks, &mut connections, deadline).await?;
        drain_runs(&daemon, deadline).await
    })
    .await
    .expect("shutdown exceeded its watchdog")
    .unwrap_err();
    assert_eq!(tokio::time::Instant::now(), deadline);
    assert_eq!(error.code, Code::Daemon);
    assert!(error.uncertain);
    assert_eq!(
        error.error,
        "Daemon commands did not finish before shutdown; outcome unknown"
    );
    assert!(matches!(
        gone.try_recv(),
        Err(oneshot::error::TryRecvError::Closed)
    ));
    assert!(idle.is_finished());
    assert!(tasks.is_empty());
    assert!(connections.is_empty());
    assert_eq!(slots.available_permits(), 1);
    assert!(!run.is_finished());
    assert_eq!(daemon.runs.lock().await.len(), 1);
    resume.send(()).unwrap();
    drain_runs(&daemon, tokio::time::Instant::now() + STOP)
        .await
        .unwrap();
    assert!(run.is_finished());
}

#[tokio::test(start_paused = true)]
async fn shutdown_joins_owned_work_that_finishes_before_the_deadline() {
    let daemon = daemon(false);
    let mut tasks = JoinSet::new();
    let mut connections = Connections::new();
    let (phase, observed) = watch::channel(ConnectionPhase::Mutation("run"));
    let task = tasks.spawn(async move {
        let _phase = phase;
        tokio::time::sleep(STOP / 4).await;
        Ok(())
    });
    connections.insert(task.id(), observed);
    let run = daemon.runs.lock().await.spawn(async {
        tokio::time::sleep(STOP / 2).await;
        Ok(())
    });
    let started = tokio::time::Instant::now();
    let deadline = started + STOP;
    tokio::time::timeout(STOP * 2, async {
        drain_connections(&mut tasks, &mut connections, deadline).await?;
        drain_runs(&daemon, deadline).await
    })
    .await
    .expect("shutdown exceeded its watchdog")
    .unwrap();
    assert_eq!(tokio::time::Instant::now(), started + STOP / 2);
    assert!(task.is_finished());
    assert!(run.is_finished());
    assert!(daemon.runs.lock().await.is_empty());
    assert!(connections.is_empty());
}

#[tokio::test]
async fn event_cost_counts_the_emitted_json_lines_and_delimiters() {
    let fixtures: Vec<Value> = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/reply-cost/run.json"
    ))
    .unwrap();
    for fixture in fixtures {
        let mut reply = fixture["input"].clone();
        let events = reply.as_object_mut().unwrap().remove("events").unwrap();
        let (mut client, mut server) = UnixStream::pair().unwrap();
        let send = async move {
            for event in events.as_array().unwrap() {
                write(&mut server, event).await.unwrap();
            }
            write(&mut server, &reply).await.unwrap();
        };
        let receive = async {
            let mut bytes = Vec::new();
            client.read_to_end(&mut bytes).await.unwrap();
            bytes
        };
        let ((), bytes) = tokio::join!(send, receive);
        let baseline = &fixture["eventCost"];
        assert_eq!(
            bytes.len(),
            baseline["bytes"].as_u64().unwrap() as usize,
            "{}",
            baseline["id"]
        );
        let serialized = String::from_utf8(bytes).unwrap();
        assert_eq!(
            serialized,
            baseline["serialized"].as_str().unwrap(),
            "{}",
            baseline["id"]
        );
        assert_eq!(
            serialized.lines().count() - 1,
            baseline["rows"].as_u64().unwrap() as usize,
            "{}",
            baseline["id"]
        );
    }
}

/// The run loop mode is passed by name; it is the default mode.
#[test]
#[allow(
    unsafe_code,
    reason = "FFI: the framework's constants are extern statics"
)]
fn the_run_loop_mode_is_the_default_mode() {
    // SAFETY: reading an immutable framework constant in a test.
    let mode = unsafe { objc2_core_foundation::kCFRunLoopDefaultMode }.unwrap();
    assert_eq!(mode.to_string(), "kCFRunLoopDefaultMode");
}

fn window(pid: i64, owner: &str, layer: i64, bounds: [f64; 4]) -> Window {
    Window {
        number: pid * 10 + layer,
        pid,
        owner: Some(owner.into()),
        title: Some(format!("{owner} window")),
        layer,
        alpha: 1.0,
        system: None,
        bounds: Bounds {
            x: bounds[0],
            y: bounds[1],
            width: bounds[2],
            height: bounds[3],
        },
    }
}

/// A 64 × 36 point display: an alert of System Events in front of a
/// TextEdit window, and a Finder window behind both.
fn fake_windows() -> Result<Vec<Window>> {
    Ok(vec![
        window(300, "System Events", 8, [20.0, 10.0, 10.0, 8.0]),
        window(200, "TextEdit", 0, [10.0, 5.0, 30.0, 20.0]),
        window(100, "Finder", 0, [0.0, 0.0, 64.0, 36.0]),
    ])
}

fn fake_bundle_id(pid: i64) -> Option<String> {
    (pid == 200).then(|| "com.apple.TextEdit".to_owned())
}

fn daemon(on_console: bool) -> Arc<Daemon> {
    let mut daemon = Daemon::new(
        "Aqua".into(),
        synthetic,
        if on_console { || true } else { || false },
        session,
    );
    daemon.screen.windows = fake_windows;
    daemon.screen.bundle_id = fake_bundle_id;
    daemon.screen.dock = |_| Dock::Icons(Vec::new());
    Arc::new(daemon)
}

/// Send one request line over a socket pair and collect every reply line.
async fn exchange(daemon: Arc<Daemon>, request: &str) -> Vec<Value> {
    exchange_as(daemon, request, true).await
}

pub(in crate::daemon) async fn exchange_with_jobs(jobs: Arc<Jobs>, request: &str) -> Vec<Value> {
    let mut daemon = daemon(false);
    Arc::get_mut(&mut daemon).unwrap().jobs = Some(jobs);
    exchange(daemon, request).await
}

/// [`exchange`] from the controller, or from the helper's own account.
async fn exchange_as(daemon: Arc<Daemon>, request: &str, controller: bool) -> Vec<Value> {
    let (mut client, server) = UnixStream::pair().unwrap();
    let served = tokio::spawn(handle(
        server,
        daemon,
        controller,
        watch::channel(ConnectionPhase::Refusal).0,
    ));

    client
        .write_all(format!("{request}\n").as_bytes())
        .await
        .unwrap();

    let mut lines = BufReader::new(client).lines();
    let mut replies = Vec::new();

    while let Some(line) = lines.next_line().await.unwrap() {
        let value: Value = serde_json::from_str(&line).unwrap();
        let last = value.get("ok").is_some();
        replies.push(value);
        if last {
            break;
        }
    }

    served.await.unwrap().unwrap();
    replies
}

/// Anything in the helper's own account is answered hello and refused
/// everything else, so nothing changed.
#[tokio::test]
async fn the_helpers_own_processes_may_only_say_hello() {
    let hello = exchange_as(daemon(false), r#"{"op":"hello"}"#, false).await;
    assert_eq!(hello[0]["op"], "hello");

    for request in [
        r#"{"op":"run","argv":["/bin/echo","hi"]}"#,
        r#"{"op":"windows"}"#,
        r#"{"op":"session-proof","challenge":"fresh"}"#,
        r#"{"op":"screenshot"}"#,
        r#"{"op":"apps"}"#,
        r#"{"op":"restart"}"#,
        r#"{"op":"logout","force":true}"#,
        r#"{"op":"job","action":"show"}"#,
        r#"{"op":"job","action":"stop-all","reason":"person"}"#,
        r#"{"op":"read","path":".zshrc","maxSize":1024}"#,
    ] {
        let reply = &exchange_as(daemon(false), request, false).await[0];
        assert_eq!(reply["code"], "permission", "{request}");
        assert_eq!(reply["uncertain"], false, "{request}");
    }
}

/// A read pinned to the session Atlas verified is served only in that
/// session: one that logged in again, or moved to another display, is
/// refused before anything runs.
#[tokio::test]
async fn a_read_pinned_to_another_session_is_refused_before_it_runs() {
    let pinned = |session: &str| {
        [
            format!(r#"{{"op":"apps","session":{session}}}"#),
            format!(r#"{{"op":"windows","session":{session}}}"#),
            format!(r#"{{"op":"run","argv":["/bin/echo","hi"],"session":{session}}}"#),
        ]
    };

    for request in pinned(r#"{"securitySessionId":42,"displayId":7}"#) {
        let reply = exchange(daemon(false), &request).await;
        assert_ne!(reply[0]["code"], "session-changed", "{request}");
        assert!(
            reply.iter().any(|line| line["ok"] == true),
            "{request}: {reply:?}"
        );
    }

    for session in [
        r#"{"securitySessionId":43,"displayId":7}"#,
        r#"{"securitySessionId":42,"displayId":8}"#,
    ] {
        for request in pinned(session) {
            let reply = exchange(daemon(false), &request).await;
            assert_eq!(reply.len(), 1, "nothing ran: {reply:?}");
            assert_eq!(reply[0]["code"], "session-changed", "{request}");
            assert_eq!(reply[0]["uncertain"], false);
        }
    }
}

#[test]
fn a_window_read_is_refused_while_the_helper_is_on_screen() {
    let request = Request::Windows {
        dock: true,
        pin: None,
    };
    assert!(request.policy().allowed(false).is_none());
    assert_eq!(
        request.policy().allowed(true).unwrap()["code"],
        "on-console"
    );
}

#[test]
fn a_window_read_is_refused_in_another_session() {
    for (security_session_id, display_id) in [(43, 7), (42, 8)] {
        let request = Request::Windows {
            dock: false,
            pin: Some(protocol::SessionPin {
                security_session_id,
                display_id,
            }),
        };
        let refusal = request.policy().check(|| false, session).unwrap();
        assert_eq!(refusal["code"], "session-changed");
        assert_eq!(refusal["uncertain"], false);
    }
}

#[tokio::test]
async fn a_window_list_is_read_in_the_daemon_without_running_a_process() {
    for dock in [false, true] {
        let request = json!({"op": "windows", "dock": dock}).to_string();
        let replies =
            tokio::time::timeout(Duration::from_secs(5), exchange(daemon(false), &request))
                .await
                .unwrap();
        assert_eq!(replies.len(), 1, "no process events");
        let reply = &replies[0];
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(reply["windows"], json!(fake_windows().unwrap()));
        assert!(reply["starts"].is_object());
        assert_eq!(reply.get("dock").is_some(), dock);
        assert!(reply.get("exitCode").is_none());
    }
}

#[tokio::test]
async fn requests_are_answered_on_the_wire() {
    let hello = exchange(daemon(false), r#"{"op":"hello"}"#).await;
    assert_eq!(hello[0]["op"], "hello");
    assert_eq!(hello[0]["session"]["managerName"], "Aqua");

    let refused = exchange(daemon(false), r#"{"op":"nope"}"#).await;
    assert_eq!(refused[0]["error"], "unknown op 'nope'");

    let ran = exchange(
        daemon(false),
        r#"{"op":"run","argv":["/bin/echo","hi"],"timeoutMs":3000}"#,
    )
    .await;
    assert_eq!(ran[0]["event"], "started");
    assert_eq!(ran[1], json!({"event": "output", "data": "aGkK"}));
    assert_eq!(ran[2]["exitCode"], 0);

    let shot = exchange(daemon(false), r#"{"op":"screenshot","maxDimension":64}"#).await;
    assert_eq!(
        (&shot[0]["width"], &shot[0]["height"]),
        (&json!(64), &json!(36))
    );
    assert!(shot[0]["offDisplayWindows"].is_array());

    let apps = exchange(daemon(false), r#"{"op":"apps"}"#).await;
    assert!(
        apps[0]["apps"]
            .as_array()
            .is_some_and(|apps| !apps.is_empty())
    );
}

#[tokio::test]
async fn nothing_acts_while_the_helper_is_on_screen() {
    for request in [
        r#"{"op":"run","argv":["/bin/echo","hi"]}"#,
        r#"{"op":"windows"}"#,
        r#"{"op":"session-proof","challenge":"fresh"}"#,
        r#"{"op":"screenshot"}"#,
        r#"{"op":"screenshot","pid":200}"#,
        r#"{"op":"screenshot","region":{"x":0,"y":0,"width":10,"height":10}}"#,
    ] {
        let reply = exchange(daemon(true), request).await;
        assert_eq!(reply.len(), 1, "{request}");
        assert_eq!(reply[0]["code"], "on-console", "{request}");
    }

    let hello = exchange(daemon(true), r#"{"op":"hello"}"#).await;
    assert_eq!(hello[0]["ok"], true, "facts stay available");
}

#[test]
fn nothing_is_captured_if_the_helper_comes_on_screen_while_the_worker_waits() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    static SHOWN: AtomicBool = AtomicBool::new(false);
    static CHECKED: Notify = Notify::const_new();
    static CAPTURES: AtomicUsize = AtomicUsize::new(0);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    let (release, waiting) = std::sync::mpsc::channel();
    let blocker = runtime.spawn_blocking(move || {
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let daemon = Arc::new(Daemon::new(
        "Aqua".into(),
        |display, area, limit| {
            CAPTURES.fetch_add(1, Ordering::SeqCst);
            synthetic(display, area, limit)
        },
        || {
            let shown = SHOWN.load(Ordering::SeqCst);
            CHECKED.notify_one();
            shown
        },
        session,
    ));

    runtime.block_on(async {
        let request = tokio::spawn(exchange(daemon, r#"{"op":"screenshot"}"#));
        tokio::time::timeout(Duration::from_secs(5), CHECKED.notified())
            .await
            .unwrap();
        SHOWN.store(true, Ordering::SeqCst);
        release.send(()).unwrap();
        blocker.await.unwrap();

        let reply = request.await.unwrap();
        assert_eq!(CAPTURES.load(Ordering::SeqCst), 0);
        assert_eq!(
            reply,
            vec![
                (Request::Screenshot {
                    max_dimension: None,
                    per_point: false,
                    crop: None
                })
                .policy()
                .allowed(true)
                .unwrap()
            ]
        );
    });
}

#[tokio::test]
async fn a_capture_is_refused_when_the_display_epoch_changes() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static READS: AtomicUsize = AtomicUsize::new(0);
    static CHANGE_AT: AtomicUsize = AtomicUsize::new(1);
    static CAPTURES: AtomicUsize = AtomicUsize::new(0);

    for (change_at, request) in [1, 2].into_iter().flat_map(|change_at| {
        [
            r#"{"op":"screenshot"}"#,
            r#"{"op":"screenshot","pid":200}"#,
            r#"{"op":"screenshot","region":{"x":0,"y":0,"width":10,"height":10}}"#,
        ]
        .map(|request| (change_at, request))
    }) {
        READS.store(0, Ordering::SeqCst);
        CHANGE_AT.store(change_at, Ordering::SeqCst);
        CAPTURES.store(0, Ordering::SeqCst);
        let mut daemon = Daemon::new(
            "Aqua".into(),
            |display, area, limit| {
                CAPTURES.fetch_add(1, Ordering::SeqCst);
                synthetic(display, area, limit)
            },
            || false,
            || {
                let mut proof = session()?;
                if READS.fetch_add(1, Ordering::SeqCst) >= CHANGE_AT.load(Ordering::SeqCst) {
                    proof["displayID"] = json!(8);
                }
                Ok(proof)
            },
        );
        daemon.screen.windows = fake_windows;
        daemon.screen.dock = |_| Dock::Icons(Vec::new());

        let reply = exchange(Arc::new(daemon), request).await;
        assert_eq!(CAPTURES.load(Ordering::SeqCst), change_at - 1, "{request}");
        assert_eq!(
            reply,
            vec![failure(
                Code::SessionChanged,
                "The helper changed since Atlas checked it; look again"
            )]
        );
    }
}

#[tokio::test]
async fn pixels_are_discarded_if_the_helper_comes_on_screen_during_capture() {
    use std::sync::atomic::{AtomicBool, Ordering};

    static SHOWN: AtomicBool = AtomicBool::new(false);
    let daemon = Arc::new(Daemon::new(
        "Aqua".into(),
        |display, area, limit| {
            SHOWN.store(true, Ordering::SeqCst);
            synthetic(display, area, limit)
        },
        || SHOWN.load(Ordering::SeqCst),
        session,
    ));

    let reply = exchange(daemon, r#"{"op":"screenshot"}"#).await;
    assert_eq!(
        reply,
        vec![
            (Request::Screenshot {
                max_dimension: None,
                per_point: false,
                crop: None
            })
            .policy()
            .allowed(true)
            .unwrap()
        ]
    );
}

/// The reply shapes clients and the live checks read.
#[tokio::test]
async fn replies_keep_their_shapes() {
    let keys = |value: &Value| {
        let mut keys: Vec<_> = value.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    };

    let hello = &exchange(daemon(false), r#"{"op":"hello"}"#).await[0];
    assert_eq!(
        keys(hello),
        [
            "consoleAgent",
            "daemon",
            "display",
            "ok",
            "op",
            "permissions",
            "session",
            "sound",
            "user"
        ]
    );
    assert_eq!(
        keys(&hello["daemon"]),
        [
            "capabilities",
            "pictureStats",
            "pid",
            "protocol",
            "resources",
            "version"
        ]
    );
    assert_eq!(
        hello["consoleAgent"],
        json!({
            "state": "off",
            "pauses": 0,
            "misses": 0,
            "rapportd": {"paused": false, "pauses": 0},
        })
    );
    assert_eq!(
        keys(&hello["daemon"]["pictureStats"]),
        ["coreGraphics", "lastCoreGraphics", "streamed"]
    );
    assert_eq!(keys(&hello["display"]), ["height", "scale", "width"]);
    assert_eq!(keys(&hello["user"]), ["home", "name", "uid"]);

    let shot = &exchange(daemon(false), r#"{"op":"screenshot"}"#).await[0];
    assert_eq!(
        keys(shot),
        ["height", "offDisplayWindows", "ok", "png", "scale", "width"]
    );

    let region = &exchange(
        daemon(false),
        r#"{"op":"screenshot","region":{"x":0,"y":0,"width":10,"height":10}}"#,
    )
    .await[0];
    assert_eq!(
        keys(region),
        [
            "height",
            "offDisplayWindows",
            "ok",
            "png",
            "region",
            "scale",
            "width"
        ]
    );
    assert_eq!(keys(&region["region"]), ["height", "width", "x", "y"]);

    let app = &exchange(daemon(false), r#"{"op":"screenshot","pid":200}"#).await[0];
    assert_eq!(
        keys(app),
        [
            "app",
            "height",
            "offDisplayWindows",
            "ok",
            "overlapping",
            "png",
            "region",
            "scale",
            "width"
        ]
    );
    assert_eq!(keys(&app["app"]), ["bundleId", "name", "pid"]);
    assert_eq!(app["app"]["bundleId"], "com.apple.TextEdit");
    assert_eq!(
        keys(&app["overlapping"][0]),
        ["bounds", "owner", "pid", "title"]
    );

    let ran = exchange(daemon(false), r#"{"op":"run","argv":["/bin/echo"]}"#).await;
    assert_eq!(keys(&ran[0]), ["event", "pid"]);
    assert_eq!(
        keys(ran.last().unwrap()),
        ["durationMs", "exitCode", "ok", "signal", "timedOut"]
    );

    let refusal = &exchange(daemon(false), r#"{"op":"run","argv":[]}"#).await[0];
    assert_eq!(keys(refusal), ["code", "error", "ok", "uncertain"]);

    let apps = &exchange(daemon(false), r#"{"op":"apps"}"#).await[0];
    assert_eq!(
        keys(&apps["apps"][0]),
        [
            "active",
            "bundleId",
            "executable",
            "hidden",
            "launchedAt",
            "name",
            "pid",
            "policy"
        ]
    );
}

/// The area each capture was asked for, kept in `areas`.
type Areas = std::sync::Mutex<Vec<Option<CGRect>>>;

fn recorded(areas: &Areas, area: Option<CGRect>) -> Result<capture::Shot> {
    areas.lock().unwrap().push(area);
    synthetic(7, area, None)
}

fn with_windows(capture: Capture, windows: fn() -> Result<Vec<Window>>) -> Arc<Daemon> {
    let mut daemon = Daemon::new("Aqua".into(), capture, || false, session);
    daemon.screen.windows = windows;
    daemon.screen.bundle_id = fake_bundle_id;
    daemon.screen.dock = |_| Dock::Icons(Vec::new());
    Arc::new(daemon)
}

#[tokio::test]
async fn anonymous_captures_skip_routing_but_app_crops_still_read_the_dock() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static LISTS: AtomicUsize = AtomicUsize::new(0);
    static DOCKS: AtomicUsize = AtomicUsize::new(0);
    let mut daemon = Daemon::new("Aqua".into(), synthetic, || false, session);
    daemon.screen.windows = || {
        LISTS.fetch_add(1, Ordering::SeqCst);
        fake_windows()
    };
    daemon.screen.dock = |_| {
        DOCKS.fetch_add(1, Ordering::SeqCst);
        Dock::Unread
    };
    for crop in [
        None,
        Some(Crop::Region(Region {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        })),
        Some(Crop::App { pid: 200 }),
    ] {
        LISTS.store(0, Ordering::SeqCst);
        DOCKS.store(0, Ordering::SeqCst);
        let reply = tokio::time::timeout(
            Duration::from_secs(5),
            screenshot(&daemon.screen, daemon.source.clone(), None, false, crop),
        )
        .await
        .unwrap();
        assert_eq!(reply["ok"], true);
        assert!(reply.get("windows").is_none());
        assert_eq!(
            LISTS.load(Ordering::SeqCst),
            1,
            "only the existing window report is read"
        );
        assert_eq!(
            DOCKS.load(Ordering::SeqCst),
            usize::from(matches!(crop, Some(Crop::App { .. })))
        );
    }
}

fn sides(area: CGRect) -> (f64, f64, f64, f64) {
    (
        area.origin.x,
        area.origin.y,
        area.size.width,
        area.size.height,
    )
}

#[tokio::test]
async fn an_app_screenshot_covers_its_windows_and_names_what_is_in_front() {
    static AREAS: Areas = std::sync::Mutex::new(Vec::new());
    let daemon = with_windows(|_, area, _| recorded(&AREAS, area), fake_windows);

    let reply = &exchange(daemon, r#"{"op":"screenshot","pid":200}"#).await[0];

    let areas = AREAS.lock().unwrap().clone();
    assert_eq!(areas.len(), 1);
    assert_eq!(sides(areas[0].unwrap()), (2.0, 0.0, 46.0, 33.0));
    assert_eq!(
        reply["region"],
        json!({"x": 2, "y": 0, "width": 46, "height": 33})
    );
    assert_eq!(
        reply["app"],
        json!({"pid": 200, "name": "TextEdit", "bundleId": "com.apple.TextEdit"})
    );
    assert_eq!(
        reply["overlapping"],
        json!([{
            "owner": "System Events",
            "pid": 300,
            "title": "System Events window",
            "bounds": {"x": 20.0, "y": 10.0, "width": 10.0, "height": 8.0},
        }])
    );
}

#[tokio::test]
async fn an_app_without_a_window_on_screen_is_refused_without_capturing() {
    static AREAS: Areas = std::sync::Mutex::new(Vec::new());
    let capture: Capture = |_, area, _| recorded(&AREAS, area);
    let unavailable = || Err(Error::new(Code::Windows, "The window list is unavailable"));

    let reply = exchange(
        with_windows(capture, fake_windows),
        r#"{"op":"screenshot","pid":4242}"#,
    )
    .await;
    assert_eq!(
        reply,
        vec![failure(
            Code::NoWindow,
            "Process 4242 has no window on screen; open one, or take a full screenshot"
        )]
    );

    let reply = exchange(
        with_windows(capture, unavailable),
        r#"{"op":"screenshot","pid":200}"#,
    )
    .await;
    assert_eq!(reply[0]["code"], "windows");
    assert!(AREAS.lock().unwrap().is_empty(), "nothing was captured");

    // A full screenshot does without the window list.
    let reply = &exchange(with_windows(capture, unavailable), r#"{"op":"screenshot"}"#).await[0];
    assert_eq!(
        (&reply["ok"], &reply["offDisplayWindows"]),
        (&json!(true), &json!([]))
    );
    assert_eq!(*AREAS.lock().unwrap(), [None]);
}

#[tokio::test]
async fn a_region_screenshot_is_cut_to_the_display() {
    static AREAS: Areas = std::sync::Mutex::new(Vec::new());
    let capture: Capture = |_, area, _| recorded(&AREAS, area);

    let reply = &exchange(
        with_windows(capture, fake_windows),
        r#"{"op":"screenshot","region":{"x":-10,"y":30,"width":100,"height":100}}"#,
    )
    .await[0];
    assert_eq!(
        reply["region"],
        json!({"x": 0, "y": 30, "width": 64, "height": 6})
    );
    assert!(reply.get("app").is_none() && reply.get("overlapping").is_none());

    let reply = exchange(
        with_windows(capture, fake_windows),
        r#"{"op":"screenshot","region":{"x":64,"y":0,"width":10,"height":10}}"#,
    )
    .await;
    assert_eq!(
        reply,
        vec![failure(
            Code::BadRequest,
            "\"region\" lies outside the display"
        )]
    );

    let areas = AREAS.lock().unwrap().clone();
    assert_eq!(areas.len(), 1, "the region outside was not captured");
    assert_eq!(sides(areas[0].unwrap()), (0.0, 30.0, 64.0, 6.0));
}

#[tokio::test]
async fn a_picture_per_point_is_limited_to_the_points_it_shows() {
    static LIMITS: std::sync::Mutex<Vec<Option<u32>>> = std::sync::Mutex::new(Vec::new());
    let capture: Capture = |display, area, limit| {
        LIMITS.lock().unwrap().push(limit);
        synthetic(display, area, limit)
    };

    for request in [
        r#"{"op":"screenshot","maxDimension":1456,"perPoint":true}"#,
        r#"{"op":"screenshot","maxDimension":40,"perPoint":true}"#,
        r#"{"op":"screenshot","perPoint":true}"#,
        r#"{"op":"screenshot","maxDimension":1456,"perPoint":true,"region":{"x":0,"y":0,"width":12,"height":20}}"#,
        r#"{"op":"screenshot","maxDimension":1456,"perPoint":true,"region":{"x":-10,"y":30,"width":100,"height":100}}"#,
        r#"{"op":"screenshot","maxDimension":1456,"perPoint":true,"pid":200}"#,
        r#"{"op":"screenshot","maxDimension":1456,"region":{"x":0,"y":0,"width":12,"height":20}}"#,
        r#"{"op":"screenshot","maxDimension":1456}"#,
        r#"{"op":"screenshot"}"#,
    ] {
        let reply = &exchange(with_windows(capture, fake_windows), request).await[0];
        assert_eq!(reply["ok"], true, "{request}");
    }

    assert_eq!(
        *LIMITS.lock().unwrap(),
        [
            Some(64),
            Some(40),
            Some(64),
            Some(20),
            Some(64),
            Some(46),
            Some(1456),
            Some(1456),
            None
        ]
    );
}

#[tokio::test]
async fn restart_answers_then_asks_the_daemon_to_exit() {
    let daemon = daemon(false);
    let notified = daemon.clone();
    let waiting = tokio::spawn(async move { notified.restarting.notified().await });

    tokio::task::yield_now().await;
    let reply = exchange(daemon, r#"{"op":"restart"}"#).await;
    assert_eq!(reply[0], json!({"ok": true, "op": "restart"}));

    tokio::time::timeout(Duration::from_secs(2), waiting)
        .await
        .unwrap()
        .unwrap();
}

/// A client that hangs up ends its quiet command; so does a stopping
/// daemon.
#[tokio::test]
async fn quiet_commands_end_when_the_client_or_daemon_leaves() {
    for leave in ["client", "daemon"] {
        let daemon = daemon(false);
        let (mut client, server) = UnixStream::pair().unwrap();
        let served = tokio::spawn(handle(
            server,
            daemon.clone(),
            true,
            watch::channel(ConnectionPhase::Refusal).0,
        ));

        client
            .write_all(b"{\"op\":\"run\",\"argv\":[\"/bin/sleep\",\"30\"],\"timeoutMs\":60000}\n")
            .await
            .unwrap();

        let mut lines = BufReader::new(client).lines();
        let started: Value =
            serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        let group = started["pid"].as_i64().unwrap() as i32;

        match leave {
            "client" => drop(lines),
            _ => {
                daemon.stopping.send_replace(true);

                // The client still listens: it hears that the command
                // was cut short, with an outcome nobody can know.
                let reply: Value =
                    serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
                assert_eq!(
                    (&reply["code"], &reply["uncertain"]),
                    (&json!("restart"), &json!(true))
                );
            }
        }

        tokio::time::timeout(Duration::from_secs(8), served)
            .await
            .expect(leave)
            .unwrap()
            .ok();

        let group = nix::unistd::Pid::from_raw(group);
        assert!(nix::sys::signal::killpg(group, None).is_err(), "{leave}");
    }
}

/// A running command stops when the helper comes on screen, and the
/// client hears that its outcome is unknown.
#[tokio::test]
async fn a_command_stops_when_the_helper_comes_on_screen() {
    static SHOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    let daemon = Arc::new(Daemon::new(
        "Aqua".into(),
        synthetic,
        || SHOWN.load(std::sync::atomic::Ordering::SeqCst),
        session,
    ));
    let (mut client, server) = UnixStream::pair().unwrap();
    let served = tokio::spawn(handle(
        server,
        daemon,
        true,
        watch::channel(ConnectionPhase::Refusal).0,
    ));

    client
        .write_all(b"{\"op\":\"run\",\"argv\":[\"/bin/sleep\",\"30\"],\"timeoutMs\":60000}\n")
        .await
        .unwrap();

    let mut lines = BufReader::new(client).lines();
    let started: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    let group = started["pid"].as_i64().unwrap() as i32;

    SHOWN.store(true, std::sync::atomic::Ordering::SeqCst);

    let line = tokio::time::timeout(Duration::from_secs(8), lines.next_line())
        .await
        .expect("the command was stopped")
        .unwrap()
        .unwrap();
    let reply: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(
        (&reply["code"], &reply["uncertain"]),
        (&json!("on-console"), &json!(true))
    );

    served.await.unwrap().unwrap();

    let group = nix::unistd::Pid::from_raw(group);
    assert!(nix::sys::signal::killpg(group, None).is_err());
}

/// A daemon that keeps its jobs in a folder of its own.
fn with_jobs(on_console: bool) -> (tempfile::TempDir, Arc<Daemon>) {
    let folder = tempfile::tempdir().unwrap();
    let mut daemon = Daemon::new(
        "Aqua".into(),
        synthetic,
        if on_console { || true } else { || false },
        session,
    );
    daemon.jobs = Some(Arc::new(
        Jobs::open(
            folder.path(),
            std::env::current_exe().unwrap(),
            nix::unistd::getuid().as_raw(),
        )
        .unwrap(),
    ));
    (folder, Arc::new(daemon))
}

#[tokio::test]
async fn the_daemon_closes_to_new_commands_only_off_the_screen_with_none_running() {
    assert!(!daemon(true).close_commands().await, "closed on the screen");

    let daemon = daemon(false);
    let running = daemon
        .runs
        .lock()
        .await
        .spawn(std::future::pending::<Result<()>>());
    assert!(
        !daemon.close_commands().await,
        "closed with a command running"
    );
    running.abort();
    while daemon.runs.lock().await.join_next().await.is_some() {}
    assert!(daemon.close_commands().await);
}

#[tokio::test]
async fn a_command_asked_for_once_the_daemon_closed_runs_nothing() {
    let folder = tempfile::tempdir().unwrap();
    let made = folder.path().join("made");
    let daemon = daemon(false);
    assert!(daemon.close_commands().await);

    let request = json!({
        "op": "run",
        "argv": ["/usr/bin/touch", made],
        "timeoutMs": 60000,
    });
    let replies = exchange(daemon, &request.to_string()).await;
    assert_eq!(replies.len(), 1, "{replies:?}");
    assert_eq!(replies[0]["code"], "busy");
    assert_eq!(replies[0]["uncertain"], false);
    assert!(!made.exists(), "the command ran");
}

#[tokio::test]
async fn daemon_shutdown_observes_the_supervisor_outcome() {
    for stopping in [false, true] {
        let complete = tokio::spawn(async { Ok(()) });
        assert_eq!(
            supervisor_outcome(complete.await, stopping).is_ok(),
            stopping
        );

        let failed = tokio::spawn(async {
            Err(Jobs::unavailable().with_source(std::io::Error::from_raw_os_error(libc::EIO)))
        });
        let error = supervisor_outcome(failed.await, stopping).unwrap_err();
        assert_eq!(error.code, Code::Job);
        assert_eq!(
            std::error::Error::source(&error)
                .unwrap()
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .raw_os_error(),
            Some(libc::EIO)
        );

        let panicked = tokio::spawn(async { panic!("private supervisor cause") });
        let error = supervisor_outcome(panicked.await, stopping).unwrap_err();
        assert_eq!(error.code, Code::Job);
        assert!(
            std::error::Error::source(&error)
                .unwrap()
                .downcast_ref::<tokio::task::JoinError>()
                .unwrap()
                .is_panic()
        );
        assert!(
            !json!(error)
                .to_string()
                .contains("private supervisor cause")
        );
    }
}

#[tokio::test(start_paused = true)]
async fn shutdown_bounds_a_supervisor_that_does_not_finish() -> Result<()> {
    let task = tokio::spawn(std::future::pending::<Result<()>>());
    let aborted = task.abort_handle();
    let started = tokio::time::Instant::now();
    let outcome = drain_supervision(task, started + STOP).await;
    let Err(error) = outcome else {
        panic!("unfinished supervision must remain unknown");
    };
    assert_eq!(tokio::time::Instant::now(), started + STOP);
    assert_eq!(error.code, Code::Job);
    assert!(error.uncertain);
    tokio::task::yield_now().await;
    assert!(aborted.is_finished());

    let complete = tokio::spawn(async { Ok(()) });
    tokio::task::yield_now().await;
    drain_supervision(complete, started).await?;
    Ok(())
}

#[tokio::test]
async fn unavailable_jobs_refuse_hello_instead_of_reporting_zero() {
    let (_folder, daemon) = with_jobs(false);
    daemon.jobs.as_ref().unwrap().fail_supervision();
    let hello = daemon.hello();
    assert_eq!(hello["ok"], false);
    assert_eq!(hello["code"], "job");
    assert!(hello.get("daemon").is_none());
    let reply = daemon
        .jobs
        .as_ref()
        .unwrap()
        .serve(JobRequest::Show { id: None })
        .await;
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["code"], "job");
    assert!(reply.get("jobs").is_none());
}

#[tokio::test]
async fn recovered_job_history_reports_a_complete_idle_census() {
    let (folder, daemon) = with_jobs(false);
    let now = atlas_core::time::now_ms();
    for number in 0..129 {
        let id = format!("{number:012x}");
        let record = json!({
            "version": 1, "id": id, "name": "history", "task": null, "session": null,
            "uid": nix::unistd::getuid().as_raw(), "boot": "fixture-boot", "pid": 4312,
            "start": [1761650000_u64, 123456], "pgid": 4312, "state": "ended",
            "startedAt": now, "endsAt": now, "adopted": number == 0, "seen": [],
            "stopping": null, "ended": {"how": "exited", "at": now,
                "signal": null, "exitCode": null, "exitSignal": null, "left": []},
            "logResets": 0
        });
        std::fs::write(
            folder.path().join(format!("jobs/{id}.json")),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        std::fs::write(folder.path().join(format!("jobs/{id}.log")), b"").unwrap();
    }
    let jobs = daemon.jobs.as_ref().unwrap();
    let listed = jobs.show(None);
    assert_eq!(listed["ok"], true);
    assert_eq!(listed["jobs"].as_array().unwrap().len(), 129);
    jobs.watch_once(None).unwrap();
    let hello = daemon.hello();
    assert_eq!(hello["ok"], true);
    assert_eq!(
        hello["daemon"]["jobs"],
        json!({"running": 0, "launching": 0, "unresolved": 0})
    );
    assert_eq!(jobs.show(None)["jobs"].as_array().unwrap().len(), 64);
}

#[tokio::test]
async fn an_unlistable_jobs_folder_still_answers_hello_and_says_its_jobs_are_incomplete() {
    use std::os::unix::fs::PermissionsExt;
    let (folder, daemon) = with_jobs(false);
    let jobs = daemon.jobs.clone().unwrap();
    let records = folder.path().join("jobs");
    std::fs::set_permissions(&records, std::fs::Permissions::from_mode(0o300)).unwrap();
    jobs.watch_once(None).unwrap();
    let hello = daemon.hello();
    std::fs::set_permissions(&records, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(hello["ok"], true, "{hello}");
    assert_eq!(
        hello["daemon"]["capabilities"],
        json!([
            "session-pin",
            "app-parts",
            "read",
            "rgb-stream-v1",
            "rgb-stream-4096",
            "session-proof-v1",
            "capture-sck-experimental",
            "rgb-stream-stop-v1",
            "helper-sound",
            "outcomes-v1",
            "jobs"
        ])
    );
    assert_eq!(
        hello["daemon"]["jobs"],
        json!({"running": 0, "launching": 0, "unresolved": 0, "incomplete": true})
    );

    jobs.watch_once(None).unwrap();
    assert_eq!(
        daemon.hello()["daemon"]["jobs"],
        json!({"running": 0, "launching": 0, "unresolved": 0})
    );
}

/// Hello waits for the jobs' lock while a slow look at them holds it. The
/// runtime here has one thread, so another request is answered before the
/// lock is let go only if hello waits off it; the outer thread lets go
/// either way.
#[test]
fn a_blocked_hello_lookup_leaves_other_daemon_requests_responsive() {
    let (_folder, daemon) = with_jobs(false);
    let held = daemon.jobs.as_ref().unwrap().held();
    let (answered, responsive) = std::sync::mpsc::channel();
    let serving = daemon.clone();
    let runtime = std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                let (mut client, server) = UnixStream::pair().unwrap();
                let (phase, mut observed) = watch::channel(ConnectionPhase::Refusal);
                let hello = tokio::spawn(handle(server, serving.clone(), true, phase));
                client.write_all(b"{\"op\":\"hello\"}\n").await.unwrap();
                observed
                    .wait_for(|phase| matches!(phase, ConnectionPhase::Read("hello")))
                    .await
                    .unwrap();
                let windows = exchange(serving, r#"{"op":"windows"}"#).await;
                let _ = answered.send(windows);
                let mut reply = String::new();
                BufReader::new(client).read_line(&mut reply).await.unwrap();
                hello.await.unwrap().unwrap();
                serde_json::from_str::<Value>(&reply).unwrap()
            })
    });
    let windows = responsive.recv_timeout(Duration::from_secs(10));
    drop(held);
    let hello = runtime.join().unwrap();
    let windows = windows.expect("the runtime stalled while hello waited for the jobs");
    assert_eq!(windows[0]["ok"], true);
    assert_eq!(hello["op"], "hello");
    assert_eq!(
        hello["daemon"]["capabilities"],
        json!([
            "session-pin",
            "app-parts",
            "read",
            "rgb-stream-v1",
            "rgb-stream-4096",
            "session-proof-v1",
            "capture-sck-experimental",
            "rgb-stream-stop-v1",
            "helper-sound",
            "outcomes-v1",
            "jobs"
        ])
    );
    assert_eq!(
        hello["daemon"]["jobs"],
        json!({"running": 0, "launching": 0, "unresolved": 0})
    );
}

#[tokio::test]
async fn the_daemon_offers_jobs_and_reads() {
    let hello = &exchange(daemon(false), r#"{"op":"hello"}"#).await[0];
    assert_eq!(
        hello["daemon"]["capabilities"],
        json!([
            "session-pin",
            "app-parts",
            "read",
            "rgb-stream-v1",
            "rgb-stream-4096",
            "session-proof-v1",
            "capture-sck-experimental",
            "rgb-stream-stop-v1",
            "helper-sound",
            "outcomes-v1",
        ]),
        "no jobs without their folder"
    );
    let refused = &exchange(daemon(false), r#"{"op":"job","action":"show"}"#).await[0];
    assert_eq!(refused["code"], "daemon");

    let (_folder, daemon) = with_jobs(false);
    let hello = &exchange(daemon.clone(), r#"{"op":"hello"}"#).await[0];
    assert_eq!(
        hello["daemon"]["capabilities"],
        json!([
            "session-pin",
            "app-parts",
            "read",
            "rgb-stream-v1",
            "rgb-stream-4096",
            "session-proof-v1",
            "capture-sck-experimental",
            "rgb-stream-stop-v1",
            "helper-sound",
            "outcomes-v1",
            "jobs"
        ])
    );
    assert_eq!(
        hello["daemon"]["jobs"],
        json!({"running": 0, "launching": 0, "unresolved": 0})
    );
    let shown = &exchange(daemon, r#"{"op":"job","action":"show"}"#).await[0];
    assert_eq!(shown["jobs"], json!([]));
}

#[tokio::test]
async fn jobs_and_reads_are_refused_while_the_helper_is_on_screen() {
    let (_folder, daemon) = with_jobs(true);
    for request in [
        r#"{"op":"job","action":"start","argv":["/bin/sleep","9"],"lifetimeMs":5000,"uid":502}"#,
        r#"{"op":"read","path":".zshrc","maxSize":1024}"#,
    ] {
        let reply = exchange(daemon.clone(), request).await;
        assert_eq!(reply.len(), 1, "{request}");
        assert_eq!(reply[0]["code"], "on-console", "{request}");
    }
    for request in [
        r#"{"op":"job","action":"show"}"#,
        r#"{"op":"job","action":"stop-all","reason":"on-console"}"#,
        r#"{"op":"job","action":"log","id":"7c2e91a04b3f","bytes":10}"#,
    ] {
        let reply = &exchange(daemon.clone(), request).await[0];
        assert_ne!(
            reply["code"], "on-console",
            "{request}: ending things goes on"
        );
    }
}

#[tokio::test]
async fn a_job_start_for_another_account_is_refused() {
    let (_folder, daemon) = with_jobs(false);
    let reply = &exchange(
        daemon,
        r#"{"op":"job","action":"start","argv":["/bin/sleep","9"],"lifetimeMs":5000,"uid":1}"#,
    )
    .await[0];
    assert_eq!(reply["code"], "identity");
    assert_eq!(reply["uncertain"], false);
}

#[test]
fn nothing_is_read_if_the_helper_comes_on_screen_while_the_worker_waits() {
    use std::sync::atomic::{AtomicBool, Ordering};

    static SHOWN: AtomicBool = AtomicBool::new(false);
    static CHECKED: Notify = Notify::const_new();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    let (release, waiting) = std::sync::mpsc::channel();
    let blocker = runtime.spawn_blocking(move || {
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let daemon = Arc::new(Daemon::new(
        "Aqua".into(),
        synthetic,
        || {
            let shown = SHOWN.load(Ordering::SeqCst);
            CHECKED.notify_one();
            shown
        },
        session,
    ));

    runtime.block_on(async {
        let request = tokio::spawn(exchange(
            daemon,
            r#"{"op":"read","path":".zshrc","maxSize":1048576}"#,
        ));
        tokio::time::timeout(Duration::from_secs(5), CHECKED.notified())
            .await
            .unwrap();
        SHOWN.store(true, Ordering::SeqCst);
        release.send(()).unwrap();
        blocker.await.unwrap();

        let reply = request.await.unwrap();
        assert_eq!(reply.len(), 1, "no part was sent: {reply:?}");
        assert_eq!(reply[0]["code"], "on-console");
        assert_eq!(reply[0]["uncertain"], false);
    });
}

#[tokio::test]
async fn parts_waiting_to_be_delivered_are_dropped_once_the_session_changes() {
    let (events, queue) = mpsc::channel(8);
    for n in 0..3 {
        events
            .send(json!({"event": "data", "data": n.to_string()}))
            .await
            .unwrap();
    }
    drop(events);
    let checks = std::sync::atomic::AtomicUsize::new(0);
    let gate = || {
        if checks.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            Ok(())
        } else {
            Err(failure(
                Code::SessionChanged,
                "The helper changed since Atlas checked it; look again",
            ))
        }
    };
    let (written, cut) = deliver(queue, Vec::new(), gate).await.unwrap();
    let lines = String::from_utf8(written).unwrap();
    assert_eq!(
        lines.lines().count(),
        1,
        "only the part before the change: {lines}"
    );
    assert_eq!(cut.unwrap()["code"], "session-changed");
}

#[tokio::test]
async fn no_screenshot_is_taken_while_a_live_source_is_quarantined() {
    use atlas_native::native::capture::source::Source;
    let mut daemon = Daemon::new(
        "Aqua".into(),
        |_, _, _| -> Result<capture::Shot> { panic!("no capture while quarantined") },
        || false,
        session,
    );
    daemon.screen.windows = fake_windows;
    daemon.screen.dock = |_| Dock::Icons(Vec::new());
    daemon.source = Arc::new(Source::new(|| {
        Err(Error::new(
            Code::Capture,
            "Native stream stop is still pending",
        ))
    }));
    let owner = daemon
        .source
        .admit(atlas_core::protocol::CaptureBackend::ScreenCaptureKit)
        .unwrap();
    assert!(daemon.source.stop(owner, None).is_err());
    let reply = &exchange(Arc::new(daemon), r#"{"op":"screenshot"}"#).await[0];
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["code"], "busy");
}

#[tokio::test]
async fn a_sound_request_answers_what_the_mute_last_reported() {
    for line in [r#"{"op":"sound"}"#, r#"{"op":"sound","muted":true}"#] {
        let reply = &exchange(daemon(false), line).await[0];
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(reply["op"], "sound");
        assert_eq!(reply["helperSound"], "unavailable");
        assert!(reply["reason"].is_string(), "{reply}");
    }
}
