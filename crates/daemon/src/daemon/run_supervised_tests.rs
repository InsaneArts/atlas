//! Commands on a machine desktop: run under a supervisor, their program's
//! end told apart from the supervisor's, and what they leave alive only
//! while the lease is.
use super::*;
use tokio::sync::{mpsc, oneshot};

/// A lease under this test binary as supervisor, and what ends it.
fn lease() -> (Lease, oneshot::Sender<()>) {
    let (end, over) = oneshot::channel::<()>();
    let lease = Lease {
        supervisor: Supervisor::of_these_tests(),
        over: Box::pin(async move {
            let _ = over.await;
        }),
    };
    (lease, end)
}

/// Run `argv` under `lease` until it replies, with its events.
async fn supervised(
    argv: &[&str],
    timeout: Duration,
    cancel: impl Future<Output = Cancel>,
    lease: Lease,
) -> (Vec<Value>, Value) {
    let argv: Vec<String> = argv.iter().map(|word| (*word).to_owned()).collect();
    let (sender, mut receiver) = mpsc::channel(EVENTS);
    let collected = async {
        let mut events = Vec::new();
        while let Some(event) = receiver.recv().await {
            events.push(event);
        }
        events
    };
    let command = run_at(
        &argv,
        home(),
        timeout,
        sender,
        cancel,
        &|| None,
        Some(lease),
    );
    let (reply, events) = tokio::time::timeout(Duration::from_secs(30), async {
        tokio::join!(command, collected)
    })
    .await
    .expect("the command replied");
    (events, reply)
}

fn output(events: &[Value]) -> String {
    let bytes: Vec<u8> = events
        .iter()
        .filter(|event| event["event"] == "output")
        .flat_map(|event| STANDARD.decode(event["data"].as_str().unwrap()).unwrap())
        .collect();
    String::from_utf8(bytes).unwrap()
}

/// Whether process `pid` is gone, or only waits to be reaped, within `within`.
async fn gone_within(pid: i32, within: Duration) -> bool {
    let started = Instant::now();
    loop {
        if atlas_core::process::info(pid)
            .unwrap()
            .is_none_or(|info| info.zombie)
        {
            return true;
        }
        if started.elapsed() >= within {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The PID a command printed first.
fn printed(events: &[Value]) -> i32 {
    output(events)
        .lines()
        .next()
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

#[tokio::test]
async fn a_supervised_command_replies_with_its_program_s_exit_and_merged_output() {
    let (lease, _end) = lease();

    let (events, reply) = supervised(
        &["/bin/sh", "-c", "echo out; echo err >&2; exit 3"],
        Duration::from_secs(10),
        std::future::pending(),
        lease,
    )
    .await;

    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(reply["exitCode"], 3, "{reply}");
    assert_eq!(output(&events), "out\nerr\n");
    assert!(reply.get("outputUnfinished").is_none(), "{reply}");
    let started = events
        .iter()
        .find(|event| event["event"] == "started")
        .unwrap();
    assert!(started["pid"].as_i64().unwrap() > 0);
}

#[tokio::test]
async fn a_supervised_command_ended_by_a_signal_replies_with_that_signal() {
    let (lease, _end) = lease();

    let (_events, reply) = supervised(
        &["/usr/bin/perl", "-e", "kill 'KILL', $$"],
        Duration::from_secs(10),
        std::future::pending(),
        lease,
    )
    .await;

    assert_eq!(reply["signal"], "SIGKILL", "{reply}");
    assert_eq!(reply["exitCode"], Value::Null, "{reply}");
}

#[tokio::test]
async fn a_supervised_command_times_out_as_any_command_does() {
    let (lease, _end) = lease();

    let (_events, reply) = supervised(
        &["/bin/sleep", "30"],
        Duration::from_millis(300),
        std::future::pending(),
        lease,
    )
    .await;

    assert_eq!(reply["timedOut"], true, "{reply}");
    assert_eq!(reply["signal"], "SIGTERM", "{reply}");
}

#[tokio::test]
async fn a_timed_out_command_that_left_its_group_and_ignores_sigterm_still_ends() {
    let (lease, _end) = lease();

    let (events, reply) = supervised(
        &[
            "/usr/bin/perl",
            "-e",
            "use POSIX; $| = 1; POSIX::setsid(); $SIG{TERM} = 'IGNORE'; print \"$$\\n\"; sleep 30",
        ],
        Duration::from_millis(500),
        std::future::pending(),
        lease,
    )
    .await;

    assert_eq!(reply["timedOut"], true, "{reply}");
    assert!(gone_within(printed(&events), Duration::from_secs(1)).await);
}

#[tokio::test]
async fn a_cancelled_command_ends_what_left_its_group() {
    let (lease, _end) = lease();
    let cancel = async {
        tokio::time::sleep(Duration::from_millis(500)).await;
        Cancel::Halted
    };

    let (events, reply) = supervised(
        &[
            "/usr/bin/perl",
            "-e",
            "use POSIX; $| = 1; POSIX::setsid(); print \"$$\\n\"; sleep 30",
        ],
        Duration::from_secs(10),
        cancel,
        lease,
    )
    .await;

    assert_eq!(reply["code"], "stopped", "{reply}");
    assert!(gone_within(printed(&events), Duration::from_secs(1)).await);
}

#[tokio::test]
async fn what_a_command_left_lives_on_after_its_reply_until_the_lease_is_over() {
    let (lease, end) = lease();

    let started = Instant::now();
    let (events, reply) = supervised(
        &["/bin/sh", "-c", "/bin/sleep 30 >/dev/null 2>&1 & echo $!"],
        Duration::from_secs(10),
        std::future::pending(),
        lease,
    )
    .await;

    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the reply waited"
    );
    assert_eq!(reply["exitCode"], 0, "{reply}");
    assert!(reply.get("outputUnfinished").is_none(), "{reply}");
    let left = printed(&events);
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert!(
        !gone_within(left, Duration::ZERO).await,
        "renewed past a lease"
    );
    end.send(()).unwrap();
    assert!(gone_within(left, Duration::from_secs(1)).await);
}

#[tokio::test]
async fn a_command_dropped_mid_run_keeps_running_until_the_lease_is_over() {
    let (lease, end) = lease();
    let folder = tempfile::tempdir().unwrap();
    let said = folder.path().join("pid");
    let script = format!("echo $$ > {}; exec /bin/sleep 30", said.display());
    let argv = ["/bin/sh".to_owned(), "-c".to_owned(), script];
    let (sender, _receiver) = mpsc::channel(EVENTS);

    let running = tokio::spawn(async move {
        run_at(
            &argv,
            home(),
            Duration::from_secs(30),
            sender,
            std::future::pending(),
            &|| None,
            Some(lease),
        )
        .await
    });
    let pid = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(pid) = std::fs::read_to_string(&said)
                && let Ok(pid) = pid.trim().parse::<i32>()
            {
                return pid;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    running.abort();
    let _ = running.await;

    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert!(
        !gone_within(pid, Duration::ZERO).await,
        "renewed past a lease"
    );
    end.send(()).unwrap();
    assert!(gone_within(pid, Duration::from_secs(1)).await);
}

#[tokio::test]
async fn a_supervisor_killed_from_outside_leaves_the_command_s_end_unknown() {
    let (lease, _end) = lease();
    let (sender, mut receiver) = mpsc::channel(EVENTS);
    let argv = ["/bin/sleep".to_owned(), "30".to_owned()];
    let command = run_at(
        &argv,
        home(),
        Duration::from_secs(30),
        sender,
        std::future::pending(),
        &|| None,
        Some(lease),
    );
    let killing = async {
        let started = receiver.recv().await.unwrap();
        let supervisor = Pid::from_raw(i32::try_from(started["pid"].as_i64().unwrap()).unwrap());
        tokio::time::sleep(Duration::from_millis(300)).await;
        nix::sys::signal::kill(supervisor, Signal::SIGKILL).unwrap();
        supervisor
    };

    let (reply, supervisor) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(command, killing)
    })
    .await
    .unwrap();

    assert_eq!(reply["ok"], false, "{reply}");
    assert_eq!(reply["code"], "wait-failed", "{reply}");
    assert_eq!(reply["uncertain"], true, "{reply}");
    signal_group(supervisor, Signal::SIGKILL);
}

#[tokio::test]
async fn a_stop_that_ends_the_lease_and_cancels_at_once_is_replied_to_as_the_stop() {
    let (stopping, stopped) = tokio::sync::watch::channel(false);
    let lease = Lease {
        supervisor: Supervisor::of_these_tests(),
        over: Box::pin({
            let mut stopped = stopped.clone();
            async move {
                let _ = stopped.wait_for(|stopped| *stopped).await;
            }
        }),
    };
    let cancel = {
        let mut stopped = stopped.clone();
        async move {
            let _ = stopped.wait_for(|stopped| *stopped).await;
            Cancel::Halted
        }
    };
    let stop = async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        stopping.send_replace(true);
    };

    let ((_events, reply), ()) = tokio::join!(
        supervised(
            &["/bin/sleep", "30"],
            Duration::from_secs(10),
            cancel,
            lease
        ),
        stop
    );

    assert_eq!(reply["code"], "stopped", "{reply}");
}

#[tokio::test]
async fn a_stop_that_comes_while_a_timed_out_command_is_being_ended_is_replied_to_as_the_stop() {
    let (lease, _end) = lease();
    let (stopping, stopped) = tokio::sync::watch::channel(false);
    let cancel = {
        let mut stopped = stopped.clone();
        async move {
            let _ = stopped.wait_for(|stopped| *stopped).await;
            Cancel::Halted
        }
    };
    // The program ends at the timeout's SIGTERM, and says so; what it left
    // ignores SIGTERM, so ending it takes until the supervisor's grace.
    let stop = async {
        tokio::time::sleep(Duration::from_millis(900)).await;
        stopping.send_replace(true);
    };

    let ((_events, reply), ()) = tokio::join!(
        supervised(
            &[
                "/bin/sh",
                "-c",
                "(trap '' TERM; exec /bin/sleep 30) & exec /bin/sleep 30",
            ],
            Duration::from_millis(300),
            cancel,
            lease,
        ),
        stop
    );

    assert_eq!(reply["code"], "stopped", "{reply}");
}

#[tokio::test]
async fn a_timed_out_supervised_program_is_asked_to_end_once() {
    let (lease, _end) = lease();

    // It counts each SIGTERM, and after the first takes half a second to
    // end cleanly, as a program with work to save would.
    let (events, reply) = supervised(
        &[
            "/usr/bin/perl",
            "-e",
            "$| = 1; my $n = 0; $SIG{TERM} = sub { $n++ }; sleep 1 while !$n; \
             select(undef, undef, undef, 0.5); print \"terms $n\\n\"; exit 0",
        ],
        Duration::from_millis(300),
        std::future::pending(),
        lease,
    )
    .await;

    assert_eq!(output(&events), "terms 1\n");
    assert_eq!(reply["timedOut"], true, "{reply}");
    assert_eq!(reply["exitCode"], 0, "{reply}");
}
