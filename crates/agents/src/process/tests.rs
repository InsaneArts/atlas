use super::*;

#[test]
fn group_signals_hold_the_reap_lock_and_never_reuse_a_retired_snapshot() {
    let registry = Registry::new();
    locked(&registry).groups.push(12345);
    let mut posted = Vec::new();
    let mut post = |group: &i32, _| {
        assert!(matches!(
            registry.tools.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ));
        posted.push(*group);
    };
    signal_groups_using(
        &locked(&registry),
        nix::sys::signal::Signal::SIGTERM,
        &mut post,
    );
    locked(&registry).groups.clear();
    signal_groups_using(
        &locked(&registry),
        nix::sys::signal::Signal::SIGKILL,
        &mut post,
    );
    assert_eq!(
        posted,
        vec![12345],
        "the retired numeric identity is never signalled again"
    );
}

#[test]
fn reaping_a_tool_retires_its_group_before_the_tool_object_is_dropped() {
    use std::os::unix::process::CommandExt;
    static TOOLS: Registry = Registry::new();
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "exit 0"]).process_group(0);
    let mut tool = start(&TOOLS, &mut command).unwrap();
    let due = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = tool.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < due, "the isolated tool never exited");
        std::thread::yield_now();
    }
    assert!(tool.running.group.is_none());
    assert!(locked(&TOOLS).groups.is_empty());
}

#[test]
fn pis_writer_uses_the_same_absolute_config_home_as_its_reader() {
    let home = Path::new("/private/tmp/atlas-pi-config");
    for verb in ["install", "remove"] {
        let command = command(
            Agent::Pi,
            &[verb, "/bundle/atlas", "--no-approve"],
            Some(home),
        );
        assert_eq!(command.get_current_dir(), Some(home));
        let configured = command
            .get_envs()
            .find(|(name, _)| *name == "PI_CODING_AGENT_DIR");
        assert_eq!(
            configured.map(|(_, value)| value),
            Some(Some(home.as_os_str()))
        );
    }
    assert!(
        command(Agent::Pi, &["--version"], None)
            .get_envs()
            .all(|(name, _)| name != "PI_CODING_AGENT_DIR"),
        "discovery preserves the caller's configuration"
    );
}

#[test]
fn a_generic_agent_is_cursor_only_in_its_resolved_install_tree() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let unrelated = directory.path().join("cursor-agent");
    fs::write(&unrelated, "unrelated").unwrap();
    let launcher = directory.path().join("agent");
    symlink(&unrelated, &launcher).unwrap();
    assert!(!cursor_agent_path(&launcher));
    fs::remove_file(&launcher).unwrap();
    fs::remove_file(&unrelated).unwrap();
    let installed = directory
        .path()
        .join("cursor-agent/versions/test/cursor-agent");
    fs::create_dir_all(installed.parent().unwrap()).unwrap();
    fs::write(&installed, "Cursor Agent").unwrap();
    symlink(&installed, &launcher).unwrap();
    assert!(cursor_agent_path(&launcher));
}

#[test]
fn a_tool_is_found_where_it_can_run() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let (first, second) = (dir.path().join("first"), dir.path().join("second"));
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    fs::write(first.join("claude"), b"").unwrap();
    let places = vec![dir.path().join("missing"), first.clone(), second.clone()];
    assert_eq!(find("claude", &places), None, "not runnable yet");

    fs::write(second.join("claude"), b"").unwrap();
    fs::set_permissions(second.join("claude"), fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(find("claude", &places), Some(second.join("claude")));

    fs::set_permissions(first.join("claude"), fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        find("claude", &places),
        Some(first.join("claude")),
        "first place wins"
    );

    fs::create_dir_all(second.join("codex")).unwrap();
    assert_eq!(find("codex", &places), None, "a folder is not a tool");
}

#[test]
fn the_places_tools_install_to_are_searched() {
    let places = install_places();
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    assert_eq!(
        places[0],
        home.join(".local/bin"),
        "Claude Code's own installer first"
    );
    assert!(places.contains(&PathBuf::from("/opt/homebrew/bin")));
    assert!(places.contains(&PathBuf::from("/usr/local/bin")));
}

#[test]
fn the_shells_path_is_read_between_its_markers() {
    let noisy = "Last login: today\n<atlas-path>/opt/homebrew/bin:/usr/bin</atlas-path>\nbye\n";
    assert_eq!(
        marked_path(noisy),
        vec![
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/bin")
        ]
    );
    assert!(marked_path("nothing here").is_empty());
}

/// Once stopping, no tool starts, and every tool started before is in
/// the groups the stop reaches.
#[test]
fn no_tool_starts_once_atlas_is_stopping() {
    use std::os::unix::process::CommandExt;

    static TOOLS: Registry = Registry::new();

    let mut command = Command::new("/bin/sleep");
    command.arg("30").process_group(0);
    let tool = start(&TOOLS, &mut command).unwrap();
    let group = i32::try_from(tool.leader.id()).unwrap();

    assert_eq!(halt(&TOOLS), vec![group]);
    let refused = start(&TOOLS, &mut command).err().unwrap();
    assert_eq!(refused.error, "Atlas is stopping");

    tool.stop().unwrap();
    assert!(
        locked(&TOOLS).groups.is_empty(),
        "a finished tool leaves the list"
    );
}

/// A fixture owns only its isolated group; its watchdog also runs if an assertion panics.
struct ProcessFixture {
    group: nix::unistd::Pid,
    cancel: std::sync::mpsc::Sender<()>,
    watchdog_fired: Arc<std::sync::atomic::AtomicBool>,
    watchdog: Option<std::thread::JoinHandle<()>>,
    worker: Option<std::thread::JoinHandle<()>>,
    result: std::sync::mpsc::Receiver<Result<Output>>,
}

impl ProcessFixture {
    fn start(tools: &'static Registry, script: &str, limit: Duration) -> Self {
        Self::led(tools, script, limit, Faults::default())
    }

    /// The fixture with its tool's waits and signals failing by `faults`.
    fn led(tools: &'static Registry, script: &str, limit: Duration, faults: Faults) -> Self {
        use std::os::unix::process::CommandExt;

        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", script])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        let mut tool = start(tools, &mut command).unwrap();
        tool.faults = faults;
        Self::supervising(tool, limit)
    }

    fn supervising(tool: Tool, limit: Duration) -> Self {
        let group = nix::unistd::Pid::from_raw(i32::try_from(tool.leader.id()).unwrap());
        let (cancel, cancelled) = std::sync::mpsc::channel();
        let watchdog_fired = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let fired = Arc::clone(&watchdog_fired);
        let watchdog = std::thread::spawn(move || {
            if cancelled.recv_timeout(Duration::from_secs(5)).is_err() {
                fired.store(true, std::sync::atomic::Ordering::SeqCst);
                let _ = nix::sys::signal::killpg(group, nix::sys::signal::Signal::SIGKILL);
            }
        });
        let (finished, result) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _ = finished.send(supervise(Agent::Claude, tool, limit));
        });

        Self {
            group,
            cancel,
            watchdog_fired,
            watchdog: Some(watchdog),
            worker: Some(worker),
            result,
        }
    }

    fn answer(&self) -> Result<Output> {
        self.result.recv_timeout(Duration::from_secs(4)).unwrap()
    }
}

impl Drop for ProcessFixture {
    fn drop(&mut self) {
        let _ = nix::sys::signal::killpg(self.group, nix::sys::signal::Signal::SIGKILL);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        let _ = self.cancel.send(());
        if let Some(watchdog) = self.watchdog.take() {
            let _ = watchdog.join();
        }
    }
}

#[test]
fn a_timed_out_tool_is_killed_reaped_and_unregistered() {
    static TOOLS: Registry = Registry::new();
    let fixture = ProcessFixture::start(&TOOLS, "exec /bin/sleep 30", Duration::from_millis(100));
    let error = fixture.answer().unwrap_err();
    assert_eq!(error.code, "agents");
    assert_eq!(error.error, "Claude Code did not answer within 0 seconds");
    assert_eq!(
        nix::sys::wait::waitpid(fixture.group, Some(nix::sys::wait::WaitPidFlag::WNOHANG)),
        Err(nix::errno::Errno::ECHILD),
        "the leader was reaped"
    );
    assert_eq!(
        nix::sys::signal::killpg(fixture.group, None),
        Err(nix::errno::Errno::ESRCH),
        "the isolated process group is gone"
    );
    assert!(locked(&TOOLS).groups.is_empty());
}

#[test]
fn an_exited_tool_returns_while_its_descendant_holds_both_pipes() {
    static TOOLS: Registry = Registry::new();
    let fixture = ProcessFixture::start(
        &TOOLS,
        "/bin/sleep 30 & printf 'out\n%s\n' \"$!\"; printf 'err\n' >&2",
        Duration::from_secs(2),
    );
    let output = fixture.answer().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stderr, b"err\n");
    let text = String::from_utf8(output.stdout).unwrap();
    let descendant = text.strip_prefix("out\n").unwrap().trim().parse().unwrap();
    let descendant = nix::unistd::Pid::from_raw(descendant);
    assert!(nix::sys::signal::kill(descendant, None).is_ok());
    assert_eq!(
        nix::sys::wait::waitpid(fixture.group, Some(nix::sys::wait::WaitPidFlag::WNOHANG)),
        Err(nix::errno::Errno::ECHILD),
        "the leader exited and was reaped while its descendant still owns the pipes"
    );
    assert!(locked(&TOOLS).groups.is_empty());
}

#[test]
fn stopping_atlas_kills_a_resistant_descendant_after_its_leader_exits_on_term() {
    use std::{io::BufRead, os::unix::net::UnixListener, os::unix::process::CommandExt};

    let tools = fresh_tools();
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("ready");
    let ready = UnixListener::bind(&socket).unwrap();
    ready.set_nonblocking(true).unwrap();
    let mut command = Command::new("/usr/bin/perl");
    command
        .args([
            "-e",
            r#"
use IO::Socket::UNIX;
pipe(my $ready, my $notify) or die;
my $child = fork;
defined($child) or die;
if ($child == 0) {
    close $ready;
    $SIG{TERM} = 'IGNORE';
    print {$notify} "ready\n";
    close $notify;
    while (1) { sleep 30 }
}
close $notify;
<$ready> eq "ready\n" or die;
close $ready;
$SIG{TERM} = sub { exit 0 };
my $socket = IO::Socket::UNIX->new(Type => SOCK_STREAM, Peer => $ARGV[0]) or die;
print {$socket} "$child\n";
close $socket;
while (1) { sleep 30 }
"#,
        ])
        .arg(&socket)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let tool = start(tools, &mut command).unwrap();
    let fixture = ProcessFixture::supervising(tool, Duration::from_secs(30));

    // The child acknowledges its TERM disposition before the leader publishes
    // readiness. The outer watchdog owns the whole group on every failure path.
    let deadline = Instant::now() + Duration::from_secs(4);
    let stream = loop {
        match ready.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "the tool did not become ready");
                std::thread::yield_now();
            }
            Err(error) => panic!("the readiness connection failed: {error}"),
        }
    };
    // macOS accepts with the listener's nonblocking mode; the read below
    // must instead wait for readiness within its timeout.
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let mut said = String::new();
    std::io::BufReader::new(stream)
        .read_line(&mut said)
        .unwrap();
    let descendant = said.trim().parse().unwrap();
    let descendant = atlas_core::process::info(descendant)
        .unwrap()
        .expect("the ready descendant is running");
    assert_eq!(descendant.pgid, fixture.group.as_raw());

    stop_all(tools, Duration::from_millis(300), Duration::from_secs(2));
    // SIGKILL posting and kernel teardown are asynchronous. A stopping error
    // can bypass output collection, so also bound the process completion wait.
    let _answer = fixture.answer();
    let deadline = Instant::now() + Duration::from_secs(1);
    while !descendant.instance.ended() && Instant::now() < deadline {
        std::thread::yield_now();
    }

    assert!(
        descendant.instance.ended(),
        "shutdown escalation must end the TERM-resistant descendant"
    );
    assert!(
        !fixture
            .watchdog_fired
            .load(std::sync::atomic::Ordering::SeqCst),
        "the watchdog must not rescue shutdown escalation"
    );
    assert!(locked(tools).groups.is_empty());
    assert!(locked(tools).held.is_empty());
    assert_eq!(
        nix::sys::wait::waitpid(fixture.group, Some(nix::sys::wait::WaitPidFlag::WNOHANG)),
        Err(nix::errno::Errno::ECHILD),
        "the leader was reaped after escalation"
    );
}

#[test]
fn both_output_streams_are_drained_after_the_retained_prefix_is_full() {
    static TOOLS: Registry = Registry::new();
    let fixture = ProcessFixture::start(
        &TOOLS,
        "/usr/bin/head -c 1114112 /dev/zero; /usr/bin/head -c 1114112 /dev/zero >&2",
        Duration::from_secs(2),
    );
    let output = fixture.answer().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, vec![0; KEEP]);
    assert_eq!(output.stderr, vec![0; KEEP]);
    assert!(locked(&TOOLS).groups.is_empty());
}

/// A list of tools of a test's own.
fn fresh_tools() -> &'static Registry {
    Box::leak(Box::new(Registry::new()))
}

type Errno = nix::errno::Errno;

fn os_error(errno: Errno) -> std::io::Error {
    std::io::Error::from_raw_os_error(errno as i32)
}

/// What a test makes a real tool's waits and signals report: failures, in
/// order, before the real answers, and a cleanup wait held until the test
/// releases it.
#[derive(Default)]
pub(super) struct Faults {
    try_waits: std::collections::VecDeque<std::io::Error>,
    waits: std::collections::VecDeque<std::io::Error>,
    signal: Option<Errno>,
    signalled: Arc<std::sync::atomic::AtomicUsize>,
    held: Option<(std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>)>,
}

impl Faults {
    /// The failure the next `try_wait` reports instead of asking, if any.
    pub(super) fn try_wait(&mut self) -> Option<std::io::Error> {
        self.try_waits.pop_front()
    }

    /// The failure the next `wait` reports instead of reaping, if any, once
    /// a held wait is released.
    pub(super) fn wait(&mut self) -> Option<std::io::Error> {
        if let Some((reached, release)) = self.held.take() {
            let _ = reached.send(());
            let _ = release.recv();
        }
        self.waits.pop_front()
    }

    /// The failure a kill reports instead of signalling, if any; every kill
    /// is counted.
    pub(super) fn signal(&mut self) -> Option<std::io::Error> {
        self.signalled
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.signal.map(os_error)
    }

    fn try_wait_fails(mut self, errno: Errno) -> Self {
        self.try_waits.push_back(os_error(errno));
        self
    }

    fn wait_fails(mut self, errno: Errno) -> Self {
        self.waits.push_back(os_error(errno));
        self
    }

    fn signal_fails(mut self, errno: Errno) -> Self {
        self.signal = Some(errno);
        self
    }

    fn counting(mut self, signalled: &Arc<std::sync::atomic::AtomicUsize>) -> Self {
        self.signalled = Arc::clone(signalled);
        self
    }

    fn holding(
        mut self,
        reached: std::sync::mpsc::Sender<()>,
        release: std::sync::mpsc::Receiver<()>,
    ) -> Self {
        self.held = Some((reached, release));
        self
    }
}

/// Stop and reap what a test left of its isolated group.
fn reap(group: nix::unistd::Pid) {
    let _ = nix::sys::signal::killpg(group, nix::sys::signal::Signal::SIGKILL);
    let _ = nix::sys::wait::waitpid(group, None);
}

#[test]
fn a_child_wait_error_retains_supervision_until_cleanup_is_confirmed() {
    let tools = fresh_tools();
    let (reached, cleanup) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let fixture = ProcessFixture::led(
        tools,
        "exec /bin/sleep 30",
        Duration::from_secs(2),
        Faults::default()
            .try_wait_fails(Errno::EINVAL)
            .holding(reached, released),
    );

    cleanup
        .recv_timeout(Duration::from_secs(4))
        .expect("cleanup runs before the tool is let go");
    assert_eq!(
        locked(tools).groups,
        vec![fixture.group.as_raw()],
        "a stop still reaches the tool while it is cleaned up"
    );
    assert!(
        fixture.result.try_recv().is_err(),
        "nothing is answered before cleanup is confirmed"
    );

    release.send(()).unwrap();
    let error = fixture.answer().unwrap_err();
    assert_eq!(error.code, "agents");
    assert_eq!(
        error.error,
        "Atlas lost track of Claude Code and stopped it"
    );
    assert!(error.uncertain, "whether its change happened is unknown");
    assert!(locked(tools).groups.is_empty());
    assert!(locked(tools).held.is_empty());
    assert_eq!(
        nix::sys::wait::waitpid(fixture.group, Some(nix::sys::wait::WaitPidFlag::WNOHANG)),
        Err(Errno::ECHILD),
        "the leader was reaped"
    );
}

#[test]
fn a_tool_whose_cleanup_fails_stays_listed_and_held() {
    let timed_out = "Claude Code did not answer within 0 seconds; whether it stopped is unknown";
    type Fault = fn(Faults) -> Faults;
    let cases: [(&str, Duration, Fault, &str); 3] = [
        (
            "signal",
            Duration::from_millis(100),
            |leader| leader.signal_fails(Errno::EPERM),
            timed_out,
        ),
        (
            "cleanup wait",
            Duration::from_millis(100),
            |leader| leader.wait_fails(Errno::EINVAL),
            timed_out,
        ),
        (
            "wait then signal",
            Duration::from_secs(2),
            |leader| {
                leader
                    .try_wait_fails(Errno::EINVAL)
                    .signal_fails(Errno::EPERM)
            },
            "Atlas lost track of Claude Code; whether it stopped is unknown",
        ),
    ];

    for (name, limit, fault, said) in cases {
        let tools = fresh_tools();
        let fixture =
            ProcessFixture::led(tools, "exec /bin/sleep 30", limit, fault(Faults::default()));
        let error = fixture.answer().unwrap_err();
        assert_eq!(error.error, said, "{name}");
        assert!(error.uncertain, "{name}");
        assert_eq!(
            locked(tools).groups,
            vec![fixture.group.as_raw()],
            "{name}: a stop still reaches it"
        );
        assert_eq!(locked(tools).held.len(), 1, "{name}: its leader is kept");
        let group = fixture.group;
        drop(fixture);
        reap(group);
    }
}

#[test]
fn a_leader_whose_identity_is_lost_is_never_signalled_again() {
    let tools = fresh_tools();
    let signalled = Arc::default();
    let fixture = ProcessFixture::led(
        tools,
        "exec /bin/sleep 30",
        Duration::from_secs(2),
        Faults::default()
            .try_wait_fails(Errno::ECHILD)
            .counting(&signalled),
    );
    let error = fixture.answer().unwrap_err();
    assert_eq!(
        error.error,
        "Atlas lost track of Claude Code; whether it stopped is unknown"
    );
    assert!(error.uncertain);
    assert_eq!(
        signalled.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "its process ID may be another's"
    );
    assert!(locked(tools).groups.is_empty() && locked(tools).held.is_empty());
    let group = fixture.group;
    drop(fixture);
    reap(group);

    let tools = fresh_tools();
    let fixture = ProcessFixture::led(
        tools,
        "exec /bin/sleep 30",
        Duration::from_millis(100),
        Faults::default().wait_fails(Errno::ECHILD),
    );
    let error = fixture.answer().unwrap_err();
    assert_eq!(
        error.error,
        "Claude Code did not answer within 0 seconds; whether it stopped is unknown"
    );
    assert!(error.uncertain);
    assert!(
        locked(tools).groups.is_empty() && locked(tools).held.is_empty(),
        "a final reap that lost the leader leaves nothing to signal"
    );
    let group = fixture.group;
    drop(fixture);
    reap(group);
}

#[test]
fn an_interrupted_wait_is_asked_again() {
    let tools = fresh_tools();
    let fixture = ProcessFixture::led(
        tools,
        "printf done",
        Duration::from_secs(2),
        Faults::default().try_wait_fails(Errno::EINTR),
    );
    let output = fixture.answer().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"done");
    assert!(locked(tools).groups.is_empty());

    let tools = fresh_tools();
    let fixture = ProcessFixture::led(
        tools,
        "exec /bin/sleep 30",
        Duration::from_millis(100),
        Faults::default().wait_fails(Errno::EINTR),
    );
    let error = fixture.answer().unwrap_err();
    assert_eq!(error.error, "Claude Code did not answer within 0 seconds");
    assert!(!error.uncertain, "the tool was stopped and reaped");
    assert!(locked(tools).groups.is_empty() && locked(tools).held.is_empty());
}

/// A real tool that left its own process group for another, with an outer
/// watchdog that kills it by its process ID unless the test says it was
/// answered.
struct Strayed {
    pid: nix::unistd::Pid,
    anchor: Child,
    cancel: Option<std::sync::mpsc::Sender<()>>,
    watchdog: Option<std::thread::JoinHandle<()>>,
}

impl Strayed {
    /// Start a tool in `tools` that moves into another group, and wait for
    /// it to say it has; the tool is the test's to supervise.
    fn start(tools: &'static Registry) -> (Self, Tool) {
        use std::{io::BufRead, os::unix::process::CommandExt};

        // A group of the test's own for the tool to move into.
        let mut anchor = Command::new("/bin/sleep");
        anchor.arg("30").process_group(0);
        let anchor = atlas_core::spawn::spawn(&mut anchor).unwrap();
        let elsewhere = nix::unistd::Pid::from_raw(i32::try_from(anchor.id()).unwrap());

        let mut command = Command::new("/usr/bin/perl");
        command
            .args([
                "-e",
                "setpgrp(0, $ENV{ELSEWHERE}) or die; $| = 1; print \"left\\n\"; sleep 30",
            ])
            .env_clear()
            .env("ELSEWHERE", elsewhere.as_raw().to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0);
        let mut tool = start(tools, &mut command).unwrap();
        let pid = nix::unistd::Pid::from_raw(i32::try_from(tool.leader.id()).unwrap());

        let (cancel, cancelled) = std::sync::mpsc::channel::<()>();
        let watchdog = std::thread::spawn(move || {
            if cancelled.recv_timeout(Duration::from_secs(5)).is_err() {
                let _ = nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGKILL);
            }
        });
        let strayed = Self {
            pid,
            anchor,
            cancel: Some(cancel),
            watchdog: Some(watchdog),
        };

        let mut said = String::new();
        let mut stdout = std::io::BufReader::new(tool.leader.stdout.take().unwrap());
        stdout.read_line(&mut said).unwrap();
        assert_eq!(said, "left\n", "the tool says once it has left its group");
        assert_eq!(nix::unistd::getpgid(Some(pid)), Ok(elsewhere));
        assert_eq!(
            nix::sys::signal::killpg(pid, None),
            Err(Errno::ESRCH),
            "its own group is empty while it runs"
        );

        (strayed, tool)
    }

    /// The tool's supervisor answered, so the watchdog is not needed.
    fn answered(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(());
        }
    }
}

impl Drop for Strayed {
    fn drop(&mut self) {
        drop(self.cancel.take());
        if let Some(watchdog) = self.watchdog.take() {
            let _ = watchdog.join();
        }
        let _ = self.anchor.kill();
        let _ = self.anchor.wait();
    }
}

#[test]
fn a_tool_that_leaves_its_process_group_is_still_stopped_at_its_deadline() {
    let tools = fresh_tools();
    let (mut strayed, tool) = Strayed::start(tools);
    let pid = strayed.pid;
    let (finished, result) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let _ = finished.send(supervise(Agent::Claude, tool, Duration::from_millis(100)));
    });
    let answer = result.recv_timeout(Duration::from_secs(4));
    if answer.is_ok() {
        strayed.answered();
    }
    // Without an answer the watchdog stops the tool now, so the worker ends.
    drop(strayed);
    worker.join().unwrap();

    let error = answer
        .expect("cleanup does not wait on a tool it never stopped")
        .unwrap_err();
    assert_eq!(error.error, "Claude Code did not answer within 0 seconds");
    assert!(!error.uncertain, "the tool was stopped and reaped");
    assert_eq!(
        nix::sys::wait::waitpid(pid, Some(nix::sys::wait::WaitPidFlag::WNOHANG)),
        Err(Errno::ECHILD),
        "the tool was reaped"
    );
    assert!(locked(tools).groups.is_empty() && locked(tools).held.is_empty());
}

#[test]
fn stopping_atlas_stops_and_reaps_a_tool_that_left_its_process_group() {
    let tools = fresh_tools();
    let (mut strayed, tool) = Strayed::start(tools);
    let pid = strayed.pid;
    let (finished, result) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let _ = finished.send(supervise(Agent::Claude, tool, Duration::from_secs(30)));
    });

    stop_all(tools, Duration::from_millis(100), Duration::from_secs(2));
    let left = locked(tools).groups.clone();
    let alive = nix::sys::signal::kill(pid, None).is_ok();
    let answer = result.recv_timeout(Duration::from_secs(4));
    if answer.is_ok() {
        strayed.answered();
    }
    drop(strayed);
    worker.join().unwrap();

    assert!(
        left.is_empty(),
        "the stop returns once its tools are reaped"
    );
    assert!(!alive, "the tool does not outlive the stop");
    let error = answer.unwrap().unwrap_err();
    assert_eq!(error.error, "Atlas is stopping");
    assert!(!error.uncertain, "the tool was stopped and reaped");
    assert_eq!(
        nix::sys::wait::waitpid(pid, Some(nix::sys::wait::WaitPidFlag::WNOHANG)),
        Err(Errno::ECHILD),
        "the tool was reaped"
    );
}

#[test]
fn stopping_atlas_kills_and_reaps_a_held_tool_that_left_its_process_group() {
    let tools = fresh_tools();
    let (mut strayed, mut tool) = Strayed::start(tools);
    let pid = strayed.pid;
    tool.faults = Faults::default().signal_fails(Errno::EPERM);
    let error = supervise(Agent::Claude, tool, Duration::from_millis(100)).unwrap_err();
    assert!(error.uncertain, "its cleanup failed, so it is held");
    assert_eq!(locked(tools).held.len(), 1);

    stop_all(tools, Duration::from_millis(100), Duration::from_secs(2));
    let alive = nix::sys::signal::kill(pid, None).is_ok();
    let left = {
        let tools = locked(tools);
        (tools.groups.clone(), tools.held.len())
    };
    if !alive {
        strayed.answered();
    }
    drop(strayed);

    assert!(!alive, "the held tool does not outlive the stop");
    assert_eq!(left, (Vec::new(), 0), "it was reaped and unlisted");
}
