//! Commands on a machine desktop: run under a supervisor, their program's
//! end told apart from the supervisor's, and what they leave alive only
//! while the lease is.
use super::*;
use crate::machine::processes::Supervised;
use std::sync::Mutex;
use tokio::sync::{mpsc, oneshot};

/// Supervisors kept in memory, as they came and went: a store a test can
/// hold up, or make fail.
#[derive(Default)]
struct Remembering {
    kept: Mutex<Vec<Supervised>>,
    /// The next add waits for this to be let go.
    held: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
    failing: bool,
    /// Forgetting fails.
    keeping: bool,
}

impl Remembering {
    fn kept(&self) -> Vec<Supervised> {
        self.kept.lock().unwrap().clone()
    }
}

impl Remembered for Remembering {
    fn kept(&self) -> atlas_core::error::Result<Vec<Supervised>> {
        Ok(self.kept())
    }

    fn add(&self, supervised: Supervised) -> atlas_core::error::Result<()> {
        let held = self.held.lock().unwrap().take();
        if let Some(held) = held {
            let _ = held.recv();
        }
        if self.failing {
            return Err(atlas_core::error::Error::new(Code::Io, "The store failed"));
        }
        self.kept.lock().unwrap().push(supervised);
        Ok(())
    }

    fn retire(&self, retired: &[Supervised]) -> atlas_core::error::Result<()> {
        if self.keeping {
            return Err(atlas_core::error::Error::new(Code::Io, "The store failed"));
        }
        self.kept
            .lock()
            .unwrap()
            .retain(|kept| !retired.contains(kept));
        Ok(())
    }
}

/// Where the supervisors these tests start leave their proofs.
fn proofs() -> PathBuf {
    static PROOFS: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    PROOFS
        .get_or_init(|| tempfile::tempdir().unwrap())
        .path()
        .to_owned()
}

/// A lease under this test binary as supervisor, kept in memory, that ends
/// once `over` does.
fn lease_until(over: impl Future<Output = ()> + Send + 'static) -> Lease {
    kept_in(over, Arc::default(), Box::new(|| None))
}

/// A lease as [`lease_until`] gives, kept in `kept` and admitted while
/// `admitted` says so.
fn kept_in(
    over: impl Future<Output = ()> + Send + 'static,
    kept: Arc<Remembering>,
    admitted: Box<dyn Fn() -> Option<Value> + Send + Sync>,
) -> Lease {
    Lease {
        supervisor: Supervisor::of_these_tests(),
        proofs: proofs(),
        generation: Generation(1),
        over: Box::pin(over),
        admitted,
        kept,
        keepers: Arc::default(),
        stops: None,
        judged: None,
        job: None,
    }
}

/// A lease under this test binary as supervisor, and what ends it.
fn lease() -> (Lease, oneshot::Sender<()>) {
    let (end, over) = oneshot::channel::<()>();
    let lease = lease_until(async move {
        let _ = over.await;
    });
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
    let lease = lease_until({
        let mut stopped = stopped.clone();
        async move {
            let _ = stopped.wait_for(|stopped| *stopped).await;
        }
    });
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

/// Whether `check` holds within `within`, looking every 20 ms.
async fn within(within: Duration, check: impl Fn() -> bool) -> bool {
    let started = Instant::now();
    loop {
        if check() {
            return true;
        }
        if started.elapsed() >= within {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A command that leaves a mark when it runs, in a folder of its own.
fn marking() -> (tempfile::TempDir, PathBuf, Vec<String>) {
    let folder = tempfile::tempdir().unwrap();
    let mark = folder.path().join("ran");
    let argv = vec!["/usr/bin/touch".to_owned(), mark.display().to_string()];
    (folder, mark, argv)
}

#[tokio::test]
async fn a_supervised_program_starts_only_once_its_supervisor_is_kept() {
    let (letting, held) = std::sync::mpsc::channel();
    let kept = Arc::new(Remembering {
        held: Mutex::new(Some(held)),
        ..Remembering::default()
    });
    let (_folder, mark, argv) = marking();
    let lease = kept_in(std::future::pending(), kept.clone(), Box::new(|| None));
    let (sender, _receiver) = mpsc::channel(EVENTS);
    let running = tokio::spawn(async move {
        run_at(
            &argv,
            home(),
            Duration::from_secs(10),
            sender,
            std::future::pending(),
            &|| None,
            Some(lease),
        )
        .await
    });

    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!mark.exists(), "it ran before it was kept");
    letting.send(()).unwrap();
    let reply = running.await.unwrap();

    assert_eq!(reply["exitCode"], 0, "{reply}");
    assert!(mark.exists());
}

#[tokio::test]
async fn a_supervised_command_whose_supervisor_cannot_be_kept_is_refused_and_never_runs() {
    let kept = Arc::new(Remembering {
        failing: true,
        ..Remembering::default()
    });
    let (_folder, mark, argv) = marking();
    let argv: Vec<&str> = argv.iter().map(String::as_str).collect();

    let (_events, reply) = supervised(
        &argv,
        Duration::from_secs(10),
        std::future::pending(),
        kept_in(std::future::pending(), kept, Box::new(|| None)),
    )
    .await;

    assert_eq!(reply["ok"], false, "{reply}");
    assert_eq!(reply["uncertain"], false, "{reply}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!mark.exists());
}

#[tokio::test]
async fn a_supervised_command_no_longer_admitted_once_kept_is_refused_and_never_runs() {
    let (letting, held) = std::sync::mpsc::channel();
    let kept = Arc::new(Remembering {
        held: Mutex::new(Some(held)),
        ..Remembering::default()
    });
    let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let admitted = {
        let stopped = stopped.clone();
        Box::new(move || {
            stopped
                .load(std::sync::atomic::Ordering::SeqCst)
                .then(|| failure(Code::Stopped, "Agents are stopped on this desktop"))
        })
    };
    let (_folder, mark, argv) = marking();
    let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
    // Agents are stopped while the store holds it up.
    let stop = async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        stopped.store(true, std::sync::atomic::Ordering::SeqCst);
        letting.send(()).unwrap();
    };

    let ((_events, reply), ()) = tokio::join!(
        supervised(
            &argv,
            Duration::from_secs(10),
            std::future::pending(),
            kept_in(std::future::pending(), kept, admitted),
        ),
        stop
    );

    assert_eq!(reply["code"], "stopped", "{reply}");
    assert_eq!(reply["uncertain"], false, "{reply}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!mark.exists());
}

#[tokio::test]
async fn a_supervisor_whose_work_ended_by_itself_is_forgotten() {
    let kept = Arc::new(Remembering::default());

    let (_events, reply) = supervised(
        &["/bin/echo", "done"],
        Duration::from_secs(10),
        std::future::pending(),
        kept_in(std::future::pending(), kept.clone(), Box::new(|| None)),
    )
    .await;

    assert_eq!(reply["exitCode"], 0, "{reply}");
    assert!(within(Duration::from_secs(3), || kept.kept().is_empty()).await);
}

#[tokio::test]
async fn a_supervisor_whose_work_ended_because_its_lease_was_over_is_forgotten_once_proved() {
    let kept = Arc::new(Remembering::default());
    let (end, over) = oneshot::channel::<()>();
    let lease = kept_in(
        async move {
            let _ = over.await;
        },
        kept.clone(),
        Box::new(|| None),
    );
    let (events, _reply) = supervised(
        &["/bin/sh", "-c", "/bin/sleep 30 >/dev/null 2>&1 & echo $!"],
        Duration::from_secs(10),
        std::future::pending(),
        lease,
    )
    .await;
    let left = printed(&events);

    end.send(()).unwrap();

    assert!(gone_within(left, Duration::from_secs(2)).await);
    assert!(within(Duration::from_secs(3), || kept.kept().is_empty()).await);
}

#[tokio::test]
async fn a_supervisor_killed_from_outside_leaves_its_work_unknown() {
    let kept = Arc::new(Remembering::default());
    let lease = kept_in(std::future::pending(), kept.clone(), Box::new(|| None));
    let keepers = lease.keepers.clone();
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
        tokio::time::sleep(Duration::from_millis(500)).await;
        nix::sys::signal::kill(supervisor, Signal::SIGKILL).unwrap();
        supervisor
    };

    let (_reply, supervisor) = tokio::join!(command, killing);

    assert!(within(Duration::from_secs(3), || keepers.uncertain()).await);
    assert_eq!(kept.kept().len(), 1);
    signal_group(supervisor, Signal::SIGKILL);
}

#[tokio::test]
async fn a_command_cancelled_while_its_supervisor_is_being_kept_never_starts() {
    let (letting, held) = std::sync::mpsc::channel();
    let kept = Arc::new(Remembering {
        held: Mutex::new(Some(held)),
        ..Remembering::default()
    });
    let (_folder, mark, argv) = marking();
    let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
    let cancel = async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        Cancel::Left
    };
    // Kept only after the cancel, inside the supervisor's grace.
    let release = async {
        tokio::time::sleep(Duration::from_millis(600)).await;
        letting.send(()).unwrap();
    };

    let ((_events, reply), ()) = tokio::join!(
        supervised(
            &argv,
            Duration::from_secs(10),
            cancel,
            kept_in(std::future::pending(), kept, Box::new(|| None)),
        ),
        release
    );

    assert_eq!(reply["code"], "transport", "{reply}");
    assert_eq!(reply["uncertain"], false, "{reply}");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!mark.exists(), "it started after it was cancelled");
}

#[tokio::test]
async fn a_supervisor_whose_record_cannot_be_forgotten_leaves_its_work_unknown() {
    let kept = Arc::new(Remembering {
        keeping: true,
        ..Remembering::default()
    });
    let lease = kept_in(std::future::pending(), kept, Box::new(|| None));
    let keepers = lease.keepers.clone();

    let (_events, reply) = supervised(
        &["/bin/echo", "done"],
        Duration::from_secs(10),
        std::future::pending(),
        lease,
    )
    .await;

    assert_eq!(reply["exitCode"], 0, "{reply}");
    assert!(within(Duration::from_secs(3), || keepers.uncertain()).await);
}
