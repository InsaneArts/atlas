//! Agent tool discovery, output collection and process-group lifetime.
use super::{Agent, CHANGE, problem};
use crate::error::Result;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// The agent tools Atlas runs: their process groups, and whether Atlas is
/// being stopped, after which none starts.
struct Tools {
    groups: Vec<i32>,
    stopping: bool,
}

static RUNNING: Mutex<Tools> = Mutex::new(Tools {
    groups: Vec::new(),
    stopping: false,
});

/// The tools in `tools`, a lock that never stays poisoned (nothing panics
/// while holding it).
fn locked(tools: &Mutex<Tools>) -> std::sync::MutexGuard<'_, Tools> {
    tools
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Stop every agent tool Atlas is running (Atlas itself is being stopped,
/// and ends once this returns). No tool starts from now on, so none
/// outlives Atlas.
pub fn stop_running() {
    let groups = halt(&RUNNING);
    if groups.is_empty() {
        return;
    }

    let signal = |signal| {
        for group in &groups {
            let _ = nix::sys::signal::killpg(nix::unistd::Pid::from_raw(*group), signal);
        }
    };

    signal(nix::sys::signal::Signal::SIGTERM);
    // A tool that ignores the request is stopped outright.
    std::thread::sleep(Duration::from_millis(300));
    signal(nix::sys::signal::Signal::SIGKILL);
}

/// Whether Atlas is being stopped: whatever runs now stops at its next step.
pub fn stopping() -> bool {
    locked(&RUNNING).stopping
}

/// Mark `tools` stopping, so no tool starts from now on, and give the
/// process groups running now.
fn halt(tools: &Mutex<Tools>) -> Vec<i32> {
    let mut tools = locked(tools);
    tools.stopping = true;

    tools.groups.clone()
}

/// Start `command` and list its process group in `tools`, under one lock:
/// a stop then reaches every tool started before it, and none starts after.
fn start(tools: &'static Mutex<Tools>, command: &mut Command) -> Result<(Child, Running)> {
    let mut list = locked(tools);
    if list.stopping {
        return Err(problem("Atlas is stopping"));
    }

    let child = command.spawn()?;
    let group = i32::try_from(child.id()).ok();
    if let Some(group) = group {
        list.groups.push(group);
    }

    Ok((child, Running { tools, group }))
}

/// Run an agent's tool with no input and stop it after `limit`. Output is
/// collected as it arrives, so a background process the tool leaves holding
/// its output open cannot keep Atlas waiting.
pub(super) fn exec(
    agent: Agent,
    argv: &[impl AsRef<std::ffi::OsStr>],
    limit: Duration,
) -> Result<Output> {
    exec_in(agent, argv, limit, None)
}

/// Run a tool in a particular config folder, retaining timeout and cancellation handling.
pub(super) fn exec_in(
    agent: Agent,
    argv: &[impl AsRef<std::ffi::OsStr>],
    limit: Duration,
    cwd: Option<&Path>,
) -> Result<Output> {
    use std::os::unix::process::CommandExt;

    let (executable, path) = executable(agent);

    // Its own process group, so a timeout stops whatever it started too.
    let mut command = Command::new(executable);
    command
        .args(argv)
        .env("PATH", path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let (child, _running) = start(&RUNNING, &mut command)?;

    supervise(agent, child, limit)
}

/// A process group Atlas started, listed while it is waited for, so
/// stop_running reaches it.
struct Running {
    tools: &'static Mutex<Tools>,
    group: Option<i32>,
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(group) = self.group {
            locked(self.tools)
                .groups
                .retain(|running| *running != group);
        }
    }
}

/// The tool's executable, and the PATH to run it with. Opened from the
/// Finder or at login, Atlas has launchd's PATH, not the shell's, so the
/// places the tools install to are searched as well, and last the shell
/// is asked for its PATH. The tool gets every place searched, as a tool
/// installed with npm needs node beside it.
fn executable(agent: Agent) -> (PathBuf, std::ffi::OsString) {
    if agent == Agent::Cursor {
        let path = std::env::join_paths(install_places().into_iter().chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )))
        .unwrap_or_default();
        return (
            cursor_executable().unwrap_or_else(|| PathBuf::from("cursor-agent")),
            path,
        );
    }
    let name = agent.name();
    let original = std::env::var_os("PATH").unwrap_or_default();
    let mut places: Vec<PathBuf> = std::env::split_paths(&original).collect();
    let mut found = find(name, &places);

    if found.is_none() {
        places.extend(install_places());
        found = find(name, &places);
    }

    if found.is_none() {
        places.extend(shell_path().iter().cloned());
        found = find(name, &places);
    }

    let path = std::env::join_paths(&places).unwrap_or(original);
    (found.unwrap_or_else(|| PathBuf::from(name)), path)
}

/// Cursor's agent is separate from its editor launcher. The generic name
/// `agent` is accepted only when its resolved path belongs to cursor-agent.
pub(super) fn cursor_executable() -> Option<PathBuf> {
    let mut places: Vec<_> =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
    places.extend(install_places());
    places.extend(shell_path().iter().cloned());
    find("cursor-agent", &places).or_else(|| {
        places
            .iter()
            .filter_map(|place| find("agent", std::slice::from_ref(place)))
            .find(|path| cursor_agent_path(path))
    })
}

fn cursor_agent_path(path: &Path) -> bool {
    std::fs::canonicalize(path).ok().is_some_and(|resolved| {
        resolved
            .file_name()
            .is_some_and(|name| name == "cursor-agent")
            && resolved.parent().is_some_and(|parent| {
                parent
                    .components()
                    .any(|component| component.as_os_str() == "cursor-agent")
            })
    })
}

/// `name` as a runnable file in the first of `places` that has it.
fn find(name: &str, places: &[PathBuf]) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    places
        .iter()
        .map(|place| place.join(name))
        .find(|candidate| {
            fs::metadata(candidate)
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        })
}

/// Where the tools install to: Claude Code's own installer, Homebrew, and
/// the global bin of npm and the Node version managers.
fn install_places() -> Vec<PathBuf> {
    let mut places = Vec::new();

    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        for relative in [
            ".local/bin",
            ".claude/local",
            ".opencode/bin",
            ".npm-global/bin",
            ".volta/bin",
            ".bun/bin",
            "Library/pnpm",
            ".local/share/pnpm",
        ] {
            places.push(home.join(relative));
        }

        // Every Node that nvm or fnm manages, newest first.
        for (root, tail) in [
            (".nvm/versions/node", "bin"),
            (".local/share/fnm/node-versions", "installation/bin"),
        ] {
            let mut versions: Vec<PathBuf> = fs::read_dir(home.join(root))
                .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
                .unwrap_or_default();
            versions.sort();
            places.extend(versions.into_iter().rev().map(|version| version.join(tail)));
        }
    }

    places.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    places
}

/// The PATH the person's shell sets up, asked once and for a few seconds
/// at most; nothing when the shell does not answer. Never asked from a
/// terminal: Atlas then has the shell's PATH already, and an interactive
/// shell started there in a process group of its own would be stopped
/// for reading the terminal (SIGTTIN) until the time ran out.
fn shell_path() -> &'static [PathBuf] {
    static SHELL_PATH: std::sync::OnceLock<Vec<PathBuf>> = std::sync::OnceLock::new();

    SHELL_PATH.get_or_init(|| {
        use std::os::unix::process::CommandExt;

        if from_terminal() {
            return Vec::new();
        }

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_owned());
        // An interactive shell may print other things: the answer is marked.
        let script = "printf '\\n<atlas-path>%s</atlas-path>\\n' \"$PATH\"";
        let mut command = Command::new(shell);
        command
            .args(["-ilc", script])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0);
        let Ok((mut child, _running)) = start(&RUNNING, &mut command) else {
            return Vec::new();
        };

        let Some(stdout) = child.stdout.take() else {
            return Vec::new();
        };

        let output = collect(Some(stdout));
        let deadline = Instant::now() + Duration::from_secs(3);

        while child.try_wait().ok().flatten().is_none() {
            if Instant::now() >= deadline {
                kill_group(&mut child);
                break;
            }

            std::thread::sleep(Duration::from_millis(20));
        }

        // A background job the shell left holding the pipe open cannot keep
        // Atlas waiting.
        marked_path(&String::from_utf8_lossy(&take(output)))
    })
}

/// Whether Atlas has a controlling terminal: it was started from a shell
/// (its output may still go elsewhere). Only opened, never read.
fn from_terminal() -> bool {
    fs::File::open("/dev/tty").is_ok()
}

/// The PATH between the markers in a shell's output, split into places.
fn marked_path(output: &str) -> Vec<PathBuf> {
    output
        .split("<atlas-path>")
        .nth(1)
        .and_then(|rest| rest.split("</atlas-path>").next())
        .map(|path| std::env::split_paths(path).collect())
        .unwrap_or_default()
}

/// The most of a stream that is kept; the rest is read and dropped, so the
/// writer never blocks on a full pipe.
const KEEP: usize = 1 << 20;

/// A stream being collected, and the signal that it ended.
type Collected = (Arc<Mutex<Vec<u8>>>, std::sync::mpsc::Receiver<()>);

/// Collect `stream` on a thread of its own as it arrives, up to KEEP bytes.
fn collect(stream: Option<impl Read + Send + 'static>) -> Collected {
    let collected = Arc::new(Mutex::new(Vec::new()));
    let (finished, ended) = std::sync::mpsc::channel();

    if let Some(mut stream) = stream {
        let sink = Arc::clone(&collected);
        std::thread::spawn(move || {
            let mut chunk = [0u8; 8192];
            while let Ok(read @ 1..) = stream.read(&mut chunk) {
                if let Ok(mut sink) = sink.lock() {
                    let room = KEEP.saturating_sub(sink.len()).min(read);
                    sink.extend_from_slice(&chunk[..room]);
                }
            }

            let _ = finished.send(());
        });
    }

    (collected, ended)
}

/// What a collected stream gave. Normally it ends with its process; a
/// moment more, then what arrived.
fn take((collected, ended): Collected) -> Vec<u8> {
    let _ = ended.recv_timeout(Duration::from_millis(500));
    collected
        .lock()
        .map(|bytes| bytes.clone())
        .unwrap_or_default()
}

/// Stop `child` and everything in its process group, and reap it.
fn kill_group(child: &mut std::process::Child) {
    if let Ok(group) = i32::try_from(child.id()) {
        let _ = nix::sys::signal::killpg(
            nix::unistd::Pid::from_raw(group),
            nix::sys::signal::Signal::SIGKILL,
        );
    }

    let _ = child.kill();
    let _ = child.wait();
}

/// Wait for `child` within `limit`, collecting its output.
fn supervise(agent: Agent, mut child: std::process::Child, limit: Duration) -> Result<Output> {
    let stdout = collect(child.stdout.take());
    let stderr = collect(child.stderr.take());
    let deadline = Instant::now() + limit;

    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }

        if Instant::now() >= deadline {
            kill_group(&mut child);
            return Err(problem(format!(
                "{} did not answer within {} seconds",
                agent.display_name(),
                limit.as_secs()
            )));
        }

        std::thread::sleep(Duration::from_millis(20));
    };

    Ok(Output {
        status,
        stdout: take(stdout),
        stderr: take(stderr),
    })
}

/// Run one change with an agent's tool; a failure says the first line the
/// tool wrote to its error output.
pub(super) fn run(agent: Agent, argv: &[String]) -> Result<()> {
    let output = exec(agent, argv, CHANGE)?;
    if output.status.success() {
        return Ok(());
    }

    Err(problem(format!(
        "{} {} failed: {}",
        agent.name(),
        argv.join(" "),
        crate::ui::text::first_line(&output.stderr, 300).unwrap_or_else(|| "no output".to_owned())
    )))
}

#[cfg(test)]
mod tests {
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
        let fixture =
            ProcessFixture::start(&TOOLS, "exec /bin/sleep 30", Duration::from_millis(100));
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
}
