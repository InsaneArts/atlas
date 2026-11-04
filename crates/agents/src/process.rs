//! Agent tool discovery, output collection and process-group lifetime.
use super::{ASK, Agent, CHANGE, Presence, problem};
use atlas_core::error::{Code, Result};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Output, Stdio},
    sync::{Arc, Condvar, Mutex, MutexGuard},
    time::{Duration, Instant},
};

/// The agent tools Atlas runs: their process groups, the leaders of those
/// whose end could not be confirmed, and how far a stop of Atlas has got.
struct Tools {
    groups: Vec<i32>,
    /// Kept, never reaped elsewhere, so their groups stay safe to signal:
    /// a group stays listed while its leader is held here.
    held: Vec<Child>,
    /// Atlas is being stopped: no tool starts from now on.
    stopping: bool,
    /// The stop's grace is over: each supervisor kills its tool now.
    ending: bool,
}

impl Tools {
    /// Whether a supervisor still waits for a tool: a listed group whose
    /// leader is not held.
    fn supervised(&self) -> bool {
        self.groups.iter().any(|group| {
            !self
                .held
                .iter()
                .any(|child| i32::try_from(child.id()).ok() == Some(*group))
        })
    }
}

/// The tools Atlas runs, and the signal that one left the list or was held.
struct Registry {
    tools: Mutex<Tools>,
    changed: Condvar,
}

impl Registry {
    const fn new() -> Self {
        Self {
            tools: Mutex::new(Tools {
                groups: Vec::new(),
                held: Vec::new(),
                stopping: false,
                ending: false,
            }),
            changed: Condvar::new(),
        }
    }

    /// The tools once no supervisor waits for any, or after `limit`.
    fn settled(&self, limit: Duration) -> MutexGuard<'_, Tools> {
        self.changed
            .wait_timeout_while(locked(self), limit, |tools| tools.supervised())
            .map_or_else(|poisoned| poisoned.into_inner().0, |(tools, _)| tools)
    }
}

static RUNNING: Registry = Registry::new();

/// The grace a tool has to end on request when Atlas is stopped.
const GRACE: Duration = Duration::from_millis(300);
/// How long a stop of Atlas waits, after killing its tools, for each to be
/// confirmed ended.
const REAPED: Duration = Duration::from_secs(2);

/// A successful version probe proves presence; a timed-out tool is installed but silent.
pub(super) fn presence(agent: Agent) -> Presence {
    match exec(agent, &["--version"], ASK) {
        Ok(output) if output.status.success() => {
            Presence::Installed(String::from_utf8_lossy(&output.stdout).trim().to_owned())
        }
        Err(error) if error.code == Code::Agents => Presence::Silent(error.error),
        _ => Presence::Missing,
    }
}

/// The tools in `tools`, a lock that never stays poisoned (nothing panics
/// while holding it).
fn locked(registry: &Registry) -> MutexGuard<'_, Tools> {
    registry
        .tools
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Stop every agent tool Atlas is running (Atlas itself is being stopped,
/// and ends once this returns). No tool starts from now on, so none
/// outlives Atlas.
pub fn stop_running() {
    stop_all(&RUNNING, GRACE, REAPED);
}

/// Stop every tool in `registry`. Each group is asked to end, and after
/// `grace` killed outright; each supervisor then kills its tool through the
/// tool's own handle, which reaches a tool that left its group, and reaps
/// it, and each held leader is killed and reaped the same way. Returns once
/// every tool is confirmed ended, or `bound` after the kill at most; a
/// leader not yet reaped stays held.
fn stop_all(registry: &Registry, grace: Duration, bound: Duration) {
    let groups = halt(registry);
    if groups.is_empty() {
        return;
    }
    let deadline = Instant::now() + grace + bound;

    signal_groups(&groups, nix::sys::signal::Signal::SIGTERM);
    let mut tools = registry.settled(grace);
    tools.ending = true;
    let groups = tools.groups.clone();
    drop(tools);
    signal_groups(&groups, nix::sys::signal::Signal::SIGKILL);

    let held = std::mem::take(&mut registry.settled(bound).held);
    let unresolved = reap_held(registry, held, deadline);
    locked(registry).held.extend(unresolved);
}

/// Send `signal` to each of `groups`, as far as it reaches: a supervisor or
/// the held-leader reaping confirms each end.
fn signal_groups(groups: &[i32], signal: nix::sys::signal::Signal) {
    for group in groups {
        let _ = nix::sys::signal::killpg(nix::unistd::Pid::from_raw(*group), signal);
    }
}

/// Kill each of `held` through its own handle and reap it by `deadline`,
/// unlisting each group whose leader is reaped (or reaped elsewhere, so
/// its process ID may be another's). Gives back those still unreaped.
fn reap_held(registry: &Registry, mut held: Vec<Child>, deadline: Instant) -> Vec<Child> {
    for child in &mut held {
        // One that cannot be killed is seen unreaped below, and kept.
        let _ = child.kill();
    }

    loop {
        let mut ended = Vec::new();
        held.retain_mut(|child| match child.try_wait() {
            Ok(Some(_)) => {
                ended.extend(i32::try_from(child.id()).ok());
                false
            }
            Err(error) if lost(&error) => {
                ended.extend(i32::try_from(child.id()).ok());
                false
            }
            Ok(None) | Err(_) => true,
        });
        locked(registry)
            .groups
            .retain(|group| !ended.contains(group));
        if held.is_empty() || Instant::now() >= deadline {
            return held;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Whether Atlas is being stopped: whatever runs now stops at its next step.
pub fn stopping() -> bool {
    locked(&RUNNING).stopping
}

/// Mark `registry` stopping, so no tool starts from now on, and give the
/// process groups running now.
fn halt(registry: &Registry) -> Vec<i32> {
    let mut tools = locked(registry);
    tools.stopping = true;

    tools.groups.clone()
}

/// Start `command` and list its process group in `tools`, under one lock:
/// a stop then reaches every tool started before it, and none starts after.
fn start(tools: &'static Registry, command: &mut Command) -> Result<Tool> {
    let mut list = locked(tools);
    if list.stopping {
        return Err(problem("Atlas is stopping"));
    }

    let child = atlas_core::spawn::spawn(command)?;
    let group = i32::try_from(child.id()).ok();
    if let Some(group) = group {
        list.groups.push(group);
    }

    Ok(Tool {
        leader: child,
        running: Running { tools, group },
        #[cfg(test)]
        faults: tests::Faults::default(),
    })
}

/// A tool Atlas started: the process leading its group and the group's
/// listing, owned together.
struct Tool {
    leader: Child,
    running: Running,
    /// Failures a test makes the leader's waits and signals report.
    #[cfg(test)]
    faults: tests::Faults,
}

impl Tool {
    /// Whether the leader has exited, without waiting.
    fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        #[cfg(test)]
        if let Some(error) = self.faults.try_wait() {
            return Err(error);
        }
        self.leader.try_wait()
    }

    /// Wait for the leader to exit, and reap it.
    fn wait(&mut self) -> std::io::Result<ExitStatus> {
        #[cfg(test)]
        if let Some(error) = self.faults.wait() {
            return Err(error);
        }
        self.leader.wait()
    }

    /// Kill everything in the leader's process group, then the leader through
    /// its own handle, as a leader that left its group is no longer in it.
    /// Neither finding nothing to signal is a failure: the leader is
    /// unreaped, so its process ID is still its own, and it has exited.
    fn kill(&mut self) -> std::io::Result<()> {
        #[cfg(test)]
        if let Some(error) = self.faults.signal() {
            return Err(error);
        }
        let group = nix::unistd::Pid::from_raw(
            i32::try_from(self.leader.id()).map_err(std::io::Error::other)?,
        );
        match nix::sys::signal::killpg(group, nix::sys::signal::Signal::SIGKILL) {
            Ok(()) | Err(nix::errno::Errno::ESRCH) => {}
            Err(errno) => return Err(errno.into()),
        }
        match self.leader.kill() {
            Err(error) if error.raw_os_error() == Some(nix::errno::Errno::ESRCH as i32) => Ok(()),
            killed => killed,
        }
    }

    /// The leader's exit status once it exits, or None at `deadline` or once
    /// a stop of Atlas says to kill it ([`Tool::ending`]); a wait that is
    /// interrupted is asked again.
    fn exited_by(&mut self, deadline: Instant) -> std::io::Result<Option<ExitStatus>> {
        loop {
            match self.try_wait() {
                Ok(Some(status)) => return Ok(Some(status)),
                Ok(None) if Instant::now() >= deadline || self.ending() => return Ok(None),
                Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        }
    }

    /// Whether a stop of Atlas has given up waiting for tools to end on
    /// request, so this one is to be killed now.
    fn ending(&self) -> bool {
        locked(self.running.tools).ending
    }

    /// Kill the tool (see [`Tool::kill`]) and reap its leader. The group is
    /// unlisted once the leader is reaped, or once something else reaped it:
    /// its process ID may then be another's, so it is never signalled again.
    /// When a signal or the reap fails otherwise, the leader stays unreaped,
    /// so its group stays listed for a stop to reach and the leader is held
    /// in the list.
    fn stop(mut self) -> std::io::Result<()> {
        if let Err(error) = self.kill() {
            self.hold();
            return Err(error);
        }

        loop {
            match self.wait() {
                Ok(_) => return Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) if lost(&error) => return Err(error),
                Err(error) => {
                    self.hold();
                    return Err(error);
                }
            }
        }
    }

    /// Keep the leader in the list, its group listed with it.
    fn hold(self) {
        let Self {
            leader,
            mut running,
            ..
        } = self;
        locked(running.tools).held.push(leader);
        // Dropped with no group, it leaves the group listed.
        running.group = None;
    }
}

/// `what` happened to a tool and then its end could not be confirmed, for
/// `cause`: whatever it was doing may have happened.
fn unknown_end(what: &str, cause: std::io::Error) -> atlas_core::error::Error {
    problem(format!("{what}; whether it stopped is unknown"))
        .with_source(cause)
        .uncertain()
}

/// Whether a wait failed because the process is no child of Atlas's (any
/// more): something else reaped it, so its identity is lost.
fn lost(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(nix::errno::Errno::ECHILD as i32)
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
    let tool = start(&RUNNING, &mut command)?;

    supervise(agent, tool, limit)
}

/// A process group Atlas started, listed while it is waited for, so
/// stop_running reaches it.
struct Running {
    tools: &'static Registry,
    group: Option<i32>,
}

impl Drop for Running {
    /// Unlist the group, unless its leader is held, and tell a waiting stop.
    fn drop(&mut self) {
        if let Some(group) = self.group {
            locked(self.tools)
                .groups
                .retain(|running| *running != group);
        }
        self.tools.changed.notify_all();
    }
}

/// The tool's executable, and the PATH to run it with. Opened from the
/// Finder or at login, Atlas has launchd's PATH, not the shell's, so the
/// places the tools install to are searched as well, and last the shell
/// is asked for its PATH. The tool gets every place searched, as a tool
/// installed with npm needs node beside it.
fn executable(agent: Agent) -> (PathBuf, std::ffi::OsString) {
    match agent {
        Agent::Cursor => {
            let places = install_places().into_iter().chain(path_places());
            return (
                cursor_executable().unwrap_or_else(|| PathBuf::from("cursor-agent")),
                std::env::join_paths(places).unwrap_or_default(),
            );
        }
        Agent::Claude | Agent::Codex | Agent::Opencode | Agent::Pi => {}
    }
    let name = agent.name();
    let mut places = path_places();
    let mut found = find(name, &places);

    if found.is_none() {
        places.extend(install_places());
        found = find(name, &places);
    }

    if found.is_none() {
        places.extend(shell_path().iter().cloned());
        found = find(name, &places);
    }

    let path = std::env::join_paths(&places)
        .unwrap_or_else(|_| std::env::var_os("PATH").unwrap_or_default());
    (found.unwrap_or_else(|| PathBuf::from(name)), path)
}

/// The places this process's own PATH names.
fn path_places() -> Vec<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect()
}

/// Cursor's agent is separate from its editor launcher. The generic name
/// `agent` is accepted only when its resolved path belongs to cursor-agent.
pub(super) fn cursor_executable() -> Option<PathBuf> {
    let mut places = path_places();
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
        let Ok(mut tool) = start(&RUNNING, &mut command) else {
            return Vec::new();
        };

        let output = collect(tool.leader.stdout.take());
        match tool.exited_by(Instant::now() + Duration::from_secs(3)) {
            Ok(Some(_)) => {}
            // Something else reaped it: dropping the tool unlists it without
            // a signal.
            Err(error) if lost(&error) => {}
            // The PATH is only a hint, so this asks for nothing more; a shell
            // whose end cannot be confirmed stays listed and held.
            Ok(None) | Err(_) => {
                let _ = tool.stop();
            }
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

/// Wait for `tool` within `limit`, collecting its output. A tool that does
/// not answer in time, that cannot be waited for, or that a stop of Atlas
/// gave up waiting for, is stopped; one whose
/// end cannot be confirmed is an uncertain failure that says so.
fn supervise(agent: Agent, mut tool: Tool, limit: Duration) -> Result<Output> {
    let stdout = collect(tool.leader.stdout.take());
    let stderr = collect(tool.leader.stderr.take());
    let untracked = format!("Atlas lost track of {}", agent.display_name());

    let status = match tool.exited_by(Instant::now() + limit) {
        Ok(Some(status)) => status,
        Ok(None) => {
            let why = if tool.ending() {
                "Atlas is stopping".to_owned()
            } else {
                format!(
                    "{} did not answer within {} seconds",
                    agent.display_name(),
                    limit.as_secs()
                )
            };
            return Err(match tool.stop() {
                Ok(()) => problem(why),
                Err(cleanup) => unknown_end(&why, cleanup),
            });
        }
        // Dropping the tool unlists it without a signal.
        Err(error) if lost(&error) => return Err(unknown_end(&untracked, error)),
        Err(error) => {
            return Err(match tool.stop() {
                Ok(()) => problem(format!("{untracked} and stopped it"))
                    .with_source(error)
                    .uncertain(),
                Err(cleanup) => unknown_end(
                    &untracked,
                    std::io::Error::other(format!("{error}; stopping it: {cleanup}")),
                ),
            });
        }
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
        atlas_core::name::first_line(&output.stderr, 300).unwrap_or_else(|| "no output".to_owned())
    )))
}

#[cfg(test)]
mod tests;
