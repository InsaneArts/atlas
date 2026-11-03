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
        stopping: false,
    });

    let mut command = Command::new("/bin/sleep");
    command.arg("30").process_group(0);
    let (mut child, running) = start(&TOOLS, &mut command).unwrap();
    let group = i32::try_from(child.id()).unwrap();

    assert_eq!(halt(&TOOLS), vec![group]);
    let refused = start(&TOOLS, &mut command).err().unwrap();
    assert_eq!(refused.error, "Atlas is stopping");

    kill_group(&mut child);
    drop(running);
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
        let (child, running) = start(tools, &mut command).unwrap();
        let group = nix::unistd::Pid::from_raw(i32::try_from(child.id()).unwrap());
        let (cancel, cancelled) = std::sync::mpsc::channel();
        let watchdog = std::thread::spawn(move || {
            if cancelled.recv_timeout(Duration::from_secs(5)).is_err() {
                let _ = nix::sys::signal::killpg(group, nix::sys::signal::Signal::SIGKILL);
            }
        });
        let (finished, result) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let answer = supervise(Agent::Claude, child, limit);
            drop(running);
            let _ = finished.send(answer);
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
