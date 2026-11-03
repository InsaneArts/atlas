use super::*;

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

    static TOOLS: Mutex<Tools> = Mutex::new(Tools {
        groups: Vec::new(),
        held: Vec::new(),
        stopping: false,
    });

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
    watchdog: Option<std::thread::JoinHandle<()>>,
    worker: Option<std::thread::JoinHandle<()>>,
    result: std::sync::mpsc::Receiver<Result<Output>>,
}

impl ProcessFixture {
    fn start(tools: &'static Mutex<Tools>, script: &str, limit: Duration) -> Self {
        Self::led(tools, script, limit, |child| child)
    }

    /// The fixture with its leader seen through `lead`.
    fn led<L: Leader + 'static>(
        tools: &'static Mutex<Tools>,
        script: &str,
        limit: Duration,
        lead: impl FnOnce(Child) -> L,
    ) -> Self {
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
        let Tool { leader, running } = start(tools, &mut command).unwrap();
        let group = nix::unistd::Pid::from_raw(i32::try_from(leader.id()).unwrap());
        let tool = Tool {
            leader: lead(leader),
            running,
        };
        let (cancel, cancelled) = std::sync::mpsc::channel();
        let watchdog = std::thread::spawn(move || {
            if cancelled.recv_timeout(Duration::from_secs(5)).is_err() {
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
    static TOOLS: Mutex<Tools> = Mutex::new(Tools {
        groups: Vec::new(),
        held: Vec::new(),
        stopping: false,
    });
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
    static TOOLS: Mutex<Tools> = Mutex::new(Tools {
        groups: Vec::new(),
        held: Vec::new(),
        stopping: false,
    });
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
fn both_output_streams_are_drained_after_the_retained_prefix_is_full() {
    static TOOLS: Mutex<Tools> = Mutex::new(Tools {
        groups: Vec::new(),
        held: Vec::new(),
        stopping: false,
    });
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
fn fresh_tools() -> &'static Mutex<Tools> {
    Box::leak(Box::new(Mutex::new(Tools {
        groups: Vec::new(),
        held: Vec::new(),
        stopping: false,
    })))
}

type Errno = nix::errno::Errno;

fn os_error(errno: Errno) -> std::io::Error {
    std::io::Error::from_raw_os_error(errno as i32)
}

/// A real leader whose waits and group signal fail as a test says, and whose
/// cleanup wait can be held until the test releases it.
struct Faulty {
    child: Child,
    try_waits: std::collections::VecDeque<std::io::Error>,
    waits: std::collections::VecDeque<std::io::Error>,
    signal: Option<Errno>,
    signalled: Arc<std::sync::atomic::AtomicUsize>,
    held: Option<(std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>)>,
}

impl Faulty {
    fn new(child: Child) -> Self {
        Self {
            child,
            try_waits: Default::default(),
            waits: Default::default(),
            signal: None,
            signalled: Default::default(),
            held: None,
        }
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

impl Leader for Faulty {
    fn pipes(&mut self) -> (Option<ChildStdout>, Option<ChildStderr>) {
        self.child.pipes()
    }

    fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        match self.try_waits.pop_front() {
            Some(error) => Err(error),
            None => Leader::try_wait(&mut self.child),
        }
    }

    fn wait(&mut self) -> std::io::Result<ExitStatus> {
        if let Some((reached, release)) = self.held.take() {
            let _ = reached.send(());
            let _ = release.recv();
        }
        match self.waits.pop_front() {
            Some(error) => Err(error),
            None => Leader::wait(&mut self.child),
        }
    }

    fn kill_group(&mut self) -> nix::Result<()> {
        self.signalled
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        match self.signal {
            Some(errno) => Err(errno),
            None => self.child.kill_group(),
        }
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
        |child| {
            Faulty::new(child)
                .try_wait_fails(Errno::EINVAL)
                .holding(reached, released)
        },
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
    type Fault = fn(Faulty) -> Faulty;
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
        let fixture = ProcessFixture::led(tools, "exec /bin/sleep 30", limit, |child| {
            fault(Faulty::new(child))
        });
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
        |child| {
            Faulty::new(child)
                .try_wait_fails(Errno::ECHILD)
                .counting(&signalled)
        },
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
        |child| Faulty::new(child).wait_fails(Errno::ECHILD),
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
    let fixture = ProcessFixture::led(tools, "printf done", Duration::from_secs(2), |child| {
        Faulty::new(child).try_wait_fails(Errno::EINTR)
    });
    let output = fixture.answer().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"done");
    assert!(locked(tools).groups.is_empty());

    let tools = fresh_tools();
    let fixture = ProcessFixture::led(
        tools,
        "exec /bin/sleep 30",
        Duration::from_millis(100),
        |child| Faulty::new(child).wait_fails(Errno::EINTR),
    );
    let error = fixture.answer().unwrap_err();
    assert_eq!(error.error, "Claude Code did not answer within 0 seconds");
    assert!(!error.uncertain, "the tool was stopped and reaped");
    assert!(locked(tools).groups.is_empty() && locked(tools).held.is_empty());
}
