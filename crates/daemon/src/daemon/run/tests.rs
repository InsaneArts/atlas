use super::*;
use crate::test_support::child::{Child, Processes, RUN};
use tokio::sync::mpsc;

#[test]
fn commands_search_where_readiness_looks() {
    let path = search_path(Some(Path::new("/Users/helper")));
    assert!(
        path.starts_with("/Users/helper/.local/bin:/Users/helper/.cargo/bin:/opt/homebrew/bin:"),
        "{path}"
    );
    assert!(path.ends_with(":/usr/bin:/bin:/usr/sbin:/sbin"), "{path}");
    assert_eq!(search_path(None), PATH, "a daemon without a home");
}

#[tokio::test]
async fn a_command_runs_at_home_in_a_utf8_locale() {
    let (events, _, _processes) = execute(
        &["/bin/sh", "-c", "pwd -P; echo \"$LANG\"; echo \"$PATH\""],
        5000,
    )
    .await;
    let output = output(&events);
    let mut lines = output.lines();
    let home = std::fs::canonicalize(std::env::var("HOME").unwrap()).unwrap();
    assert_eq!(lines.next(), home.to_str());

    let lang = std::env::var("LANG")
        .ok()
        .filter(|lang| !lang.is_empty())
        .unwrap_or_else(|| LANG.to_owned());
    assert_eq!(lines.next(), Some(lang.as_str()), "{output}");
    assert!(
        lines
            .next()
            .is_some_and(|path| path.contains("/.local/bin:")),
        "{output}"
    );
}

/// Every event `receiver` gets until the run drops its sender, taken as
/// they come so a long output never fills the channel.
async fn gather(mut receiver: mpsc::Receiver<Value>) -> (Vec<Value>, Processes) {
    let mut processes = Processes::default();
    let mut events = Vec::new();
    while let Some(event) = receiver.recv().await {
        processes.started(&event);
        events.push(event);
    }
    (events, processes)
}

async fn execute(argv: &[&str], timeout_ms: u64) -> (Vec<Value>, Value, Processes) {
    let argv: Vec<String> = argv.iter().map(|a| (*a).to_owned()).collect();
    let (sender, receiver) = mpsc::channel(EVENTS);
    let collected = gather(receiver);

    let command = run(
        &argv,
        Duration::from_millis(timeout_ms),
        sender,
        std::future::pending::<Cancel>(),
    );

    let (reply, (events, processes)) = tokio::time::timeout(Duration::from_secs(30), async {
        tokio::join!(command, collected)
    })
    .await
    .unwrap();
    (events, reply, processes)
}

fn output(events: &[Value]) -> String {
    let bytes: Vec<u8> = events
        .iter()
        .filter(|event| event["event"] == "output")
        .flat_map(|event| STANDARD.decode(event["data"].as_str().unwrap()).unwrap())
        .collect();

    String::from_utf8(bytes).unwrap()
}

fn group_alive(group: i64) -> bool {
    signal_group(Pid::from_raw(group as i32), None)
}

async fn poll_pending(mut command: std::pin::Pin<&mut impl Future<Output = Value>>) {
    std::future::poll_fn(|context| {
        assert!(command.as_mut().poll(context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
}

// Keep runtime time fixed while the real kernel delivers exit readiness.
// Observe reaping independently of pipe collection and final delivery.
async fn observe_exit(
    mut command: std::pin::Pin<&mut impl Future<Output = Value>>,
    leader: atlas_core::process::Instance,
) {
    let until = Instant::now() + Duration::from_secs(20);
    std::future::poll_fn(|context| {
        assert!(Instant::now() < until, "leader was not reaped");
        assert!(command.as_mut().poll(context).is_pending());
        if atlas_core::process::info(leader.pid)
            .unwrap()
            .is_none_or(|info| info.instance != leader)
        {
            return std::task::Poll::Ready(());
        }
        // Prevent paused time from auto-advancing before kernel exit.
        context.waker().wake_by_ref();
        std::task::Poll::Pending
    })
    .await;
}

#[tokio::test]
async fn each_stop_cause_preserves_its_reply_and_escalation() {
    for (reason, timeout_first) in [
        (None, true),
        (Some(Cancel::Left), false),
        (Some(Cancel::Stopping), false),
        (Some(Cancel::OnConsole), false),
        (Some(Cancel::Left), true),
        (Some(Cancel::Stopping), true),
        (Some(Cancel::OnConsole), true),
    ] {
        let mut child = Child::new(RUN);
        let argv = child.argv("stubborn");
        let (sender, mut receiver) = mpsc::channel(EVENTS);
        let observer = sender.clone();
        let (cancel, cancelled) = tokio::sync::oneshot::channel();
        let command = run(&argv, Duration::from_secs(60), sender, async {
            cancelled.await.unwrap()
        });
        tokio::pin!(command);
        let ready = async {
            let leader = child.started(&mut receiver, observer).await;
            child.message("ready").await;
            child.permit("start");
            let descendant = child.message("descendant").await;
            (leader, descendant)
        };
        let (leader, descendant) = tokio::select! {
            reply = &mut command => panic!("command ended before readiness: {reply}"),
            processes = ready => processes,
        };
        let mut cancel = Some(cancel);
        if !timeout_first {
            cancel.take().unwrap().send(reason.unwrap()).unwrap();
        } else {
            tokio::time::pause();
            tokio::time::advance(Duration::from_secs(60)).await;
            tokio::time::resume();
        }
        poll_pending(command.as_mut()).await;
        tokio::time::pause();
        {
            let terms = async {
                child.message("term").await;
                child.message("term").await;
            };
            tokio::pin!(terms);
            let until = Instant::now() + Duration::from_secs(20);
            std::future::poll_fn(|context| {
                assert!(Instant::now() < until, "SIGTERM was not observed");
                let result = terms.as_mut().poll(context);
                // The kernel must deliver both reports before grace advances.
                if result.is_pending() {
                    context.waker().wake_by_ref();
                }
                result
            })
            .await;
        }
        for process in [leader, descendant] {
            let info = atlas_core::process::info(process.pid).unwrap().unwrap();
            assert_eq!(info.instance, process);
            assert!(
                !info.zombie,
                "SIGTERM leaves this fixture alive for escalation"
            );
        }
        if timeout_first && let Some(reason) = reason {
            cancel.take().unwrap().send(reason).unwrap();
            poll_pending(command.as_mut()).await;
        }
        tokio::time::advance(GRACE).await;
        tokio::time::resume();
        let reply = tokio::time::timeout(Duration::from_secs(20), &mut command)
            .await
            .unwrap();
        match reason {
            None => {
                assert_eq!(reply["ok"], true);
                assert_eq!(reply["exitCode"], Value::Null);
                assert_eq!(reply["signal"], "SIGKILL");
                assert_eq!(reply["timedOut"], true);
                assert_eq!(reply.get("outputUnfinished"), None);
            }
            Some(Cancel::Left) => {
                assert_eq!(reply, interrupted(Code::Transport, "The client went away"))
            }
            Some(Cancel::Stopping) => assert_eq!(
                reply,
                interrupted(Code::Restart, "The helper stopped while the command ran")
            ),
            Some(Cancel::OnConsole) => assert_eq!(
                reply,
                interrupted(
                    Code::OnConsole,
                    "The helper came on screen, so the command was stopped"
                )
            ),
        }
        assert!(!group_alive(i64::from(leader.pid)));
        assert!(
            atlas_core::process::info(leader.pid).unwrap().is_none(),
            "leader reaped"
        );
        assert_eq!(
            receiver.recv().await.unwrap(),
            json!({"event": "started", "pid": leader.pid})
        );
        assert!(receiver.try_recv().is_err());
    }
}

#[tokio::test]
async fn a_full_output_queue_is_not_a_cancelled_command() {
    let mut child = Child::new(RUN);
    let argv = child.argv("output");
    let (sender, mut receiver) = mpsc::channel(1);
    let observer = sender.clone();
    let command = run(
        &argv,
        Duration::from_secs(60),
        sender,
        std::future::pending::<Cancel>(),
    );
    tokio::pin!(command);
    let ready = async {
        let leader = child.started(&mut receiver, observer).await;
        child.message("ready").await;
        child.permit("start");
        child.permit("output");
        child.message("output").await;
        leader
    };
    let leader = tokio::select! {
        reply = &mut command => panic!("command ended before readiness: {reply}"),
        leader = ready => leader,
    };
    poll_pending(command.as_mut()).await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_millis(50)).await;
    poll_pending(command.as_mut()).await;
    tokio::time::resume();
    assert!(
        atlas_core::process::info(leader.pid)
            .unwrap()
            .is_some_and(|info| !info.zombie)
    );
    child.permit("exit");
    child.message("exiting").await;
    tokio::time::pause();
    observe_exit(command.as_mut(), leader).await;
    tokio::time::advance(FINISH).await;
    tokio::time::resume();
    let reply = tokio::time::timeout(Duration::from_secs(20), command)
        .await
        .unwrap();
    assert_eq!(reply["ok"], true);
    assert_eq!(reply["exitCode"], 0);
    assert_eq!(reply["timedOut"], false);
    assert_eq!(reply["outputTruncated"], true);
    assert!(atlas_core::process::info(leader.pid).unwrap().is_none());
    assert_eq!(receiver.recv().await.unwrap()["event"], "started");
    assert!(receiver.try_recv().is_err());
}

async fn time_out(child: &mut Child, mode: &str, capacity: usize) -> (Vec<Value>, Value) {
    let argv = child.argv(mode);
    let (sender, mut receiver) = mpsc::channel(capacity);
    let observer = sender.clone();
    let command = run(
        &argv,
        Duration::from_secs(60),
        sender,
        std::future::pending::<Cancel>(),
    );
    tokio::pin!(command);
    let ready = async {
        child.started(&mut receiver, observer).await;
        child.message("ready").await;
        child.permit("start");
        if matches!(mode, "group" | "term") {
            child.message("descendant").await;
        } else if mode == "flood" {
            child.message("output").await;
        }
    };
    tokio::select! {
        reply = &mut command => panic!("command ended before readiness: {reply}"),
        () = ready => {}
    }
    // The real child is ready. Advance only the command's deadline, then
    // resume real time before polling supervision and its kernel waits.
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(60)).await;
    tokio::time::resume();
    let reply = tokio::time::timeout(Duration::from_secs(20), command)
        .await
        .unwrap();
    let mut events = Vec::new();
    while let Some(event) = receiver.recv().await {
        events.push(event);
    }
    (events, reply)
}

#[tokio::test]
async fn a_command_streams_merged_output_then_its_exit() {
    let (events, reply, _processes) =
        execute(&["/bin/sh", "-c", "echo out; echo err >&2; exit 3"], 5000).await;
    assert_eq!(events[0]["event"], "started");
    assert!(events[0]["pid"].as_u64().unwrap() > 1);
    assert_eq!(output(&events), "out\nerr\n");
    assert_eq!(reply["ok"], true);
    assert_eq!(reply["exitCode"], 3);
    assert_eq!(reply["signal"], Value::Null);
    assert_eq!(reply["timedOut"], false);

    let (events, reply, _processes) = execute(&["echo", "on", "path"], 5000).await;
    assert_eq!(
        (output(&events).as_str(), &reply["exitCode"]),
        ("on path\n", &json!(0))
    );
}

#[tokio::test]
async fn a_timeout_ends_the_whole_group_with_sigterm() {
    let mut child = Child::new(RUN);
    let (events, reply) = time_out(&mut child, "term", EVENTS).await;

    assert_eq!(reply["timedOut"], true);
    // A shell that handles the signal first exits 143 instead of dying by it.
    assert!(reply["signal"] == "SIGTERM" || reply["exitCode"] == 143);

    let group = events[0]["pid"].as_i64().unwrap();
    assert!(!group_alive(group), "no member of the group survives");

    let (_, reply, _processes) = execute(&["/bin/sh", "-c", "kill -TERM $$"], 5000).await;
    assert_eq!(
        (&reply["signal"], &reply["timedOut"]),
        (&json!("SIGTERM"), &json!(false))
    );
}

/// The leader dies on SIGTERM; a descendant that ignores it is killed too.
#[tokio::test]
async fn a_timeout_kills_descendants_that_ignore_sigterm() {
    let mut child = Child::new(RUN);
    let (events, reply) = time_out(&mut child, "group", EVENTS).await;
    assert_eq!(reply["timedOut"], true);
    let group = events[0]["pid"].as_i64().unwrap();
    assert!(
        !group_alive(group),
        "the descendant was killed before the reply"
    );
}

#[tokio::test]
async fn a_child_exit_does_not_imply_its_pipe_closed() {
    let mut child = Child::new(RUN);
    let argv = child.argv("pipe");
    let (sender, mut receiver) = mpsc::channel(EVENTS);
    let observer = sender.clone();
    let command = run(
        &argv,
        Duration::from_secs(60),
        sender,
        std::future::pending::<Cancel>(),
    );
    tokio::pin!(command);
    let ready = async {
        let leader = child.started(&mut receiver, observer).await;
        child.message("ready").await;
        child.permit("start");
        let descendant = child.message("descendant").await;
        (leader, descendant)
    };
    let (leader, descendant) = tokio::select! {
        reply = &mut command => panic!("command ended before readiness: {reply}"),
        processes = ready => processes,
    };
    tokio::time::pause();
    child.permit("exit");
    observe_exit(command.as_mut(), leader).await;
    child.message("exiting").await;
    tokio::time::advance(LINGER).await;
    tokio::time::resume();
    let reply = tokio::time::timeout(Duration::from_secs(20), command)
        .await
        .unwrap();
    assert_eq!(reply["exitCode"], 0);
    assert_eq!(reply["timedOut"], false);
    assert_eq!(reply.get("outputTruncated"), None);
    assert_eq!(
        reply["outputUnfinished"], true,
        "the descendant still holds the output pipe"
    );
    let info = atlas_core::process::info(descendant.pid).unwrap().unwrap();
    assert_eq!(info.instance, descendant);
    assert!(!info.zombie, "a background process outlives the run");
    let mut events = Vec::new();
    while let Some(event) = receiver.recv().await {
        events.push(event);
    }
    assert!(output(&events).is_empty());
    child.permit_descendant("close");
    child.message("closed").await;
    assert!(
        atlas_core::process::info(descendant.pid)
            .unwrap()
            .is_some_and(|info| !info.zombie),
        "closing the output pipe and exiting have separate permits"
    );
    child.permit_descendant("exit");
}

#[tokio::test]
async fn cancellation_after_exit_leaves_the_descendant_running() {
    for reason in [Cancel::Left, Cancel::Stopping, Cancel::OnConsole] {
        let mut child = Child::new(RUN);
        let argv = child.argv("pipe");
        let (sender, mut receiver) = mpsc::channel(EVENTS);
        let observer = sender.clone();
        let (cancel, cancelled) = tokio::sync::oneshot::channel();
        let command = run(&argv, Duration::from_secs(60), sender, async {
            cancelled.await.unwrap()
        });
        tokio::pin!(command);
        let ready = async {
            let leader = child.started(&mut receiver, observer).await;
            child.message("ready").await;
            child.permit("start");
            let descendant = child.message("descendant").await;
            (leader, descendant)
        };
        let (leader, descendant) = tokio::select! {
            reply = &mut command => panic!("command ended before readiness: {reply}"),
            processes = ready => processes,
        };
        tokio::time::pause();
        child.permit("exit");
        observe_exit(command.as_mut(), leader).await;
        child.message("exiting").await;
        cancel.send(reason).unwrap();
        poll_pending(command.as_mut()).await;
        tokio::time::advance(LINGER).await;
        tokio::time::resume();
        let reply = tokio::time::timeout(Duration::from_secs(20), command)
            .await
            .unwrap();
        let expected = match reason {
            Cancel::Left => interrupted(Code::Transport, "The client went away"),
            Cancel::Stopping => {
                interrupted(Code::Restart, "The helper stopped while the command ran")
            }
            Cancel::OnConsole => interrupted(
                Code::OnConsole,
                "The helper came on screen, so the command was stopped",
            ),
        };
        assert_eq!(reply, expected);
        let info = atlas_core::process::info(descendant.pid).unwrap().unwrap();
        assert_eq!(info.instance, descendant);
        assert!(!info.zombie);
        // A signal handler would report `term` before accepting `close`.
        child.permit_descendant("close");
        child.message("closed").await;
        child.permit_descendant("exit");
    }
}

#[tokio::test]
async fn a_wait_failure_keeps_the_outcome_uncertain_and_the_descendant_alive() {
    use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};

    let mut child = Child::new(RUN);
    let argv = child.argv("pipe");
    let (sender, mut receiver) = mpsc::channel(EVENTS);
    let observer = sender.clone();
    let command = run(
        &argv,
        Duration::from_secs(60),
        sender,
        std::future::pending::<Cancel>(),
    );
    tokio::pin!(command);
    let ready = async {
        let leader = child.started(&mut receiver, observer).await;
        child.message("ready").await;
        child.permit("start");
        let descendant = child.message("descendant").await;
        (leader, descendant)
    };
    let (leader, descendant) = tokio::select! {
        reply = &mut command => panic!("command ended before readiness: {reply}"),
        processes = ready => processes,
    };
    // Reap the owned leader while supervision is unpolled, so its real
    // wait returns ECHILD without a shipping fault hook.
    child.permit("exit");
    child.message("exiting").await;
    let until = Instant::now() + Duration::from_secs(20);
    let status = loop {
        assert!(Instant::now() < until, "leader did not exit");
        match waitpid(Pid::from_raw(leader.pid), Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) | Err(nix::errno::Errno::EINTR) => {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            status => break status.unwrap(),
        }
    };
    assert_eq!(status, WaitStatus::Exited(Pid::from_raw(leader.pid), 0));
    tokio::time::pause();
    poll_pending(command.as_mut()).await;
    tokio::time::advance(LINGER).await;
    tokio::time::resume();
    let reply = tokio::time::timeout(Duration::from_secs(20), command)
        .await
        .unwrap();
    assert_eq!(
        reply,
        interrupted(
            Code::WaitFailed,
            "The command's exit status could not be read"
        )
    );
    let info = atlas_core::process::info(descendant.pid).unwrap().unwrap();
    assert_eq!(info.instance, descendant);
    assert!(!info.zombie);
    child.permit_descendant("close");
    child.message("closed").await;
    child.permit_descendant("exit");
}

/// A client that stops reading cannot stall supervision or the timeout.
#[tokio::test]
async fn a_client_that_never_reads_cannot_stall_the_timeout() {
    let mut child = Child::new(RUN);
    let (_, reply) = time_out(&mut child, "flood", 1).await;

    assert_eq!(
        (&reply["timedOut"], &reply["outputTruncated"]),
        (&json!(true), &json!(true))
    );
}

#[tokio::test]
async fn a_departed_receiver_leaves_no_owned_group_running() {
    let mut child = Child::new(RUN);
    let argv = child.argv("flood");
    let (sender, mut receiver) = mpsc::channel(1);
    let observer = sender.clone();
    let command = run(
        &argv,
        Duration::from_secs(30),
        sender,
        std::future::pending::<Cancel>(),
    );
    let control = async {
        child.started(&mut receiver, observer).await;
        let leader = child.message("ready").await;
        drop(receiver);
        child.permit("start");
        leader
    };
    let (gone, leader) = tokio::time::timeout(Duration::from_secs(20), async {
        tokio::join!(command, control)
    })
    .await
    .unwrap();

    assert_eq!(
        gone["code"], "transport",
        "a departed client ends the command"
    );
    assert!(
        !group_alive(i64::from(leader.pid)),
        "the owned group ended before the reply"
    );
}

#[tokio::test]
async fn a_readiness_timeout_leaves_no_foreground_child_running() {
    let mut child = Child::new(RUN);
    let argv = child.argv("unready");
    let (sender, mut receiver) = mpsc::channel(1);
    let observer = sender.clone();
    let leader = {
        let command = run(
            &argv,
            Duration::from_secs(60),
            sender,
            std::future::pending::<Cancel>(),
        );
        tokio::pin!(command);
        let control = async {
            let leader = child.started(&mut receiver, observer).await;
            assert!(
                tokio::time::timeout(Duration::ZERO, child.message("ready"))
                    .await
                    .is_err()
            );
            leader
        };
        tokio::time::timeout(Duration::from_secs(20), async {
            tokio::select! {
                reply = &mut command => panic!("command ended before readiness: {reply}"),
                leader = control => leader,
            }
        })
        .await
        .unwrap()
    };
    drop(child);
    assert!(atlas_core::process::info(leader.pid).unwrap().is_none());
}

/// Cancelling ends a command that prints nothing, and its descendants.
#[tokio::test]
async fn cancelling_ends_a_quiet_command_and_its_group() {
    let mut child = Child::new(RUN);
    let (sender, mut receiver) = mpsc::channel(EVENTS);
    let observer = sender.clone();
    let argv = child.argv("group");
    let cancel = async {
        child.started(&mut receiver, observer).await;
        child.message("ready").await;
        child.permit("start");
        child.message("descendant").await;
        Cancel::Left
    };
    let reply = tokio::time::timeout(
        Duration::from_secs(20),
        run(&argv, Duration::from_secs(60), sender, cancel),
    )
    .await
    .unwrap();

    assert_eq!(reply["code"], "transport");
    assert_eq!(reply["uncertain"], true, "the command had started");

    let group = receiver.recv().await.unwrap()["pid"].as_i64().unwrap();
    assert!(!group_alive(group), "nothing the command started survives");
    assert!(
        receiver.try_recv().is_err(),
        "fixture messages never enter output"
    );
    child.message("term").await;
    child.message("term").await;
}

/// A daemon that stops mid-command says so, and that the outcome is
/// unknown.
#[tokio::test]
async fn a_stopping_daemon_leaves_the_outcome_unknown() {
    let mut child = Child::new(RUN);
    let (sender, mut receiver) = mpsc::channel(EVENTS);
    let observer = sender.clone();
    let argv = child.argv("quiet");
    let cancel = async {
        child.started(&mut receiver, observer).await;
        child.message("ready").await;
        child.permit("start");
        Cancel::Stopping
    };

    let reply = tokio::time::timeout(
        Duration::from_secs(20),
        run(&argv, Duration::from_secs(60), sender, cancel),
    )
    .await
    .unwrap();
    assert_eq!(
        reply,
        interrupted(Code::Restart, "The helper stopped while the command ran")
    );
    assert_eq!(reply["uncertain"], true);
}

#[tokio::test]
async fn final_output_waits_for_capacity_before_delivery() {
    let (sender, mut receiver) = mpsc::channel(1);
    sender.try_send(json!({"event": "started"})).unwrap();
    let mut buffered = Output::default();
    buffered.push(b"onetwo");
    {
        let finish = buffered.finish(&sender);
        tokio::pin!(finish);
        std::future::poll_fn(|context| {
            assert!(finish.as_mut().poll(context).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        assert_eq!(receiver.recv().await.unwrap()["event"], "started");
        assert!(finish.await.is_ok());
    }
    assert_eq!(output(&[receiver.try_recv().unwrap()]), "onetwo");
    assert!(!buffered.truncated);
    assert!(buffered.pending.is_empty());
    assert_eq!(buffered.forwarded, 6);
    assert_eq!(buffered.sent, 1);
}

#[tokio::test]
async fn an_exhausted_event_budget_drops_output_without_reporting_a_disconnect() {
    let (sender, mut receiver) = mpsc::channel(1);
    let mut buffered = Output::default();
    for _ in 0..MAX_EVENTS {
        buffered.push(b"x");
        assert!(buffered.flush(&sender).is_ok());
        assert_eq!(output(&[receiver.recv().await.unwrap()]), "x");
    }
    buffered.push(b"still draining");
    assert!(buffered.flush(&sender).is_ok());
    assert!(buffered.truncated);
    assert!(buffered.pending.is_empty());
    assert_eq!(buffered.sent, MAX_EVENTS);
    assert_eq!(buffered.forwarded, MAX_EVENTS);
    assert!(buffered.finish(&sender).await.is_ok());
    assert!(receiver.try_recv().is_err());
}

/// The last output waits for a slow client instead of being dropped, and
/// output that cannot be delivered is reported as truncated.
#[tokio::test]
async fn the_last_output_reaches_a_slow_client_or_is_reported() {
    let mut child = Child::new(RUN);
    let argv = child.argv("output");
    // One slot: `started` fills it until the controller permits reads.
    let (sender, mut receiver) = mpsc::channel(1);
    let observer = sender.clone();
    let (permit, reads) = tokio::sync::oneshot::channel();
    let slow = async move {
        let mut receiver: mpsc::Receiver<Value> = reads.await.unwrap();
        let mut events = Vec::new();
        while let Some(event) = receiver.recv().await {
            events.push(event);
        }
        events
    };
    let control = async {
        child.started(&mut receiver, observer).await;
        child.message("ready").await;
        child.permit("start");
        child.permit("output");
        child.message("output").await;
        child.permit("exit");
        child.message("exiting").await;
        permit.send(receiver).unwrap();
    };

    let command = run(
        &argv,
        Duration::from_secs(60),
        sender,
        std::future::pending::<Cancel>(),
    );
    let (reply, events, ()) = tokio::time::timeout(Duration::from_secs(20), async {
        tokio::join!(command, slow, control)
    })
    .await
    .unwrap();

    assert_eq!(output(&events), "onetwo");
    assert_eq!(reply.get("outputTruncated"), None);

    let mut child = Child::new(RUN);
    let argv = child.argv("output");
    let (sender, mut never) = mpsc::channel(1);
    let observer = sender.clone();
    let command = run(
        &argv,
        Duration::from_secs(60),
        sender,
        std::future::pending::<Cancel>(),
    );
    let control = async {
        child.started(&mut never, observer).await;
        child.message("ready").await;
        child.permit("start");
        child.permit("output");
        child.message("output").await;
        child.permit("exit");
        child.message("exiting").await;
    };
    let (reply, ()) = tokio::time::timeout(Duration::from_secs(20), async {
        tokio::join!(command, control)
    })
    .await
    .unwrap();
    assert_eq!(reply["outputTruncated"], true);
}

/// Output written a little at a time stays within the client's event
/// limit and arrives whole.
#[tokio::test]
async fn trickled_output_is_coalesced_into_few_events() {
    let (events, reply, _processes) = execute(
        &[
            "/bin/sh",
            "-c",
            "i=0; while [ $i -lt 2000 ]; do printf x; i=$((i+1)); done",
        ],
        20000,
    )
    .await;
    assert_eq!(reply["exitCode"], 0);
    assert_eq!(reply.get("outputTruncated"), None);
    assert_eq!(output(&events), "x".repeat(2000));
    assert!(events.len() < 100, "{} events", events.len());
}

#[tokio::test]
async fn missing_commands_and_huge_output_are_reported() {
    let (events, reply, _processes) = execute(&["/nonexistent/binary"], 3000).await;
    assert!(events.is_empty());
    assert_eq!(
        reply,
        failure(Code::SpawnFailed, "/nonexistent/binary: no such program")
    );

    let (_, folder, _processes) = execute(&["/tmp"], 3000).await;
    assert_eq!(
        folder,
        failure(Code::SpawnFailed, "/tmp: is a folder, not a program")
    );

    let plain = tempfile::NamedTempFile::new().unwrap();
    let plain_path = plain.path().to_string_lossy().into_owned();
    let (_, not_executable, _processes) = execute(&[&plain_path], 3000).await;
    assert_eq!(
        not_executable,
        failure(
            Code::SpawnFailed,
            format!("{plain_path}: is not executable (chmod +x it)")
        )
    );

    let (_, reply, _processes) = execute(&["no-such-command-atlas"], 3000).await;
    assert_eq!(reply["code"], "spawn-failed");

    let (events, reply, _processes) = execute(&[], 3000).await;
    assert!(events.is_empty());
    assert_eq!(reply, failure(Code::SpawnFailed, "No command to run"));

    let (events, reply, _processes) =
        execute(&["/bin/sh", "-c", "head -c 17000000 /dev/zero"], 20000).await;
    assert_eq!(reply["outputTruncated"], true);
    assert_eq!(reply["ok"], true);
    assert_eq!(reply["exitCode"], 0);
    assert_eq!(reply["timedOut"], false);

    let forwarded: usize = events
        .iter()
        .filter(|event| event["event"] == "output")
        .map(|event| {
            STANDARD
                .decode(event["data"].as_str().unwrap())
                .unwrap()
                .len()
        })
        .sum();
    assert!(forwarded <= MAX_OUTPUT);
}

/// A relative path with a slash starts at the helper's home, for the
/// checks and the command alike, however the daemon itself was started.
#[tokio::test]
async fn a_relative_program_starts_at_home() {
    let home = tempfile::tempdir().unwrap();
    let bin = home.path().join("bin");
    std::fs::create_dir(&bin).unwrap();

    let script = bin.join("build.sh");
    std::fs::write(&script, "#!/bin/sh\necho \"built $1\"\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(home.path().join("notes"), "plain\n").unwrap();

    let execute = |argv: &[&str]| {
        let argv: Vec<String> = argv.iter().map(|a| (*a).to_owned()).collect();
        let home = home.path().to_owned();
        async move {
            let (sender, receiver) = mpsc::channel(EVENTS);
            let collected = gather(receiver);

            let command = run_at(
                &argv,
                Some(home),
                Duration::from_secs(5),
                sender,
                std::future::pending::<Cancel>(),
            );

            let (reply, (events, processes)) =
                tokio::time::timeout(Duration::from_secs(20), async {
                    tokio::join!(command, collected)
                })
                .await
                .unwrap();
            (events, reply, processes)
        }
    };

    let (events, reply, _processes) = execute(&["./bin/build.sh", "now"]).await;
    assert_eq!(reply["exitCode"], 0, "{reply}");
    assert_eq!(output(&events), "built now\n");

    let (events, reply, _processes) = execute(&["bin/build.sh"]).await;
    assert_eq!(reply["exitCode"], 0, "{reply}");
    assert_eq!(output(&events), "built \n");

    let (_, folder, _processes) = execute(&["./bin"]).await;
    assert_eq!(
        folder,
        failure(Code::SpawnFailed, "./bin: is a folder, not a program")
    );

    let (_, plain, _processes) = execute(&["./notes"]).await;
    assert_eq!(
        plain,
        failure(
            Code::SpawnFailed,
            "./notes: is not executable (chmod +x it)"
        )
    );

    let (_, missing, _processes) = execute(&["./missing"]).await;
    assert_eq!(
        missing,
        failure(Code::SpawnFailed, "./missing: no such program")
    );
}

/// A helper that comes on screen stops the command, and its outcome is
/// unknown.
#[tokio::test]
async fn coming_on_screen_stops_the_command() {
    let mut child = Child::new(RUN);
    let (sender, mut receiver) = mpsc::channel(EVENTS);
    let observer = sender.clone();
    let argv = child.argv("quiet");
    let cancel = async {
        child.started(&mut receiver, observer).await;
        child.message("ready").await;
        child.permit("start");
        Cancel::OnConsole
    };

    let reply = tokio::time::timeout(
        Duration::from_secs(20),
        run(&argv, Duration::from_secs(60), sender, cancel),
    )
    .await
    .unwrap();
    assert_eq!(
        (&reply["code"], &reply["uncertain"]),
        (&json!("on-console"), &json!(true))
    );

    let group = receiver.recv().await.unwrap()["pid"].as_i64().unwrap();
    assert!(!group_alive(group), "the command was ended");
}

#[test]
fn signals_have_conventional_names() {
    assert_eq!(signal_name(Signal::SIGTERM as i32), "SIGTERM");
    assert_eq!(signal_name(Signal::SIGKILL as i32), "SIGKILL");
    assert_eq!(signal_name(Signal::SIGXCPU as i32), "SIGXCPU");
    assert_eq!(signal_name(Signal::SIGXFSZ as i32), "SIGXFSZ");
    assert_eq!(signal_name(Signal::SIGSYS as i32), "SIGSYS");
    assert_eq!(signal_name(Signal::SIGINFO as i32), "SIGINFO");
    assert_eq!(signal_name(64), "SIG64");
}
