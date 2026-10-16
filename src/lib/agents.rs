//! `atlas agents`: connect Claude Code and Codex to Atlas. The
//! plugin (a skill that teaches the agent, a setup skill, and a session-start
//! status line) ships inside Atlas.app, so it always matches this CLI;
//! connecting registers that folder as a local marketplace in each agent.
use crate::{
    client::Client,
    config::Config,
    error::{Error, Result},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// The marketplace and plugin name in both agents.
const NAME: &str = "atlas";
/// The plugin as both agents name it: `NAME@NAME`, the plugin in its marketplace.
const PLUGIN: &str = "atlas@atlas";

/// How long a question to an agent's tool may take, and a change to it.
const ASK: Duration = Duration::from_secs(10);
const CHANGE: Duration = Duration::from_secs(120);

fn problem(message: impl Into<String>) -> Error {
    Error::new("agents", message)
}

/// The agents Atlas can connect, and their command-line tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    pub const ALL: [Agent; 2] = [Agent::Claude, Agent::Codex];

    pub fn name(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
        }
    }

    /// The name a person knows it by.
    pub fn display_name(self) -> &'static str {
        match self {
            Agent::Claude => "Claude Code",
            Agent::Codex => "Codex",
        }
    }

    /// The agent `name` (as in `status`), if it is one.
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|agent| agent.name() == name)
    }

    /// The argv that connect the plugin at `marketplace`.
    fn connect(self, marketplace: &Path) -> Vec<Vec<String>> {
        let path = marketplace.to_string_lossy().into_owned();
        let argv = |items: &[&str]| items.iter().map(|item| (*item).to_owned()).collect();
        match self {
            Agent::Claude => vec![
                argv(&["plugin", "marketplace", "add", &path, "--scope", "user"]),
                argv(&["plugin", "install", PLUGIN, "--scope", "user"]),
            ],
            Agent::Codex => vec![
                argv(&["plugin", "marketplace", "add", &path]),
                argv(&["plugin", "add", PLUGIN]),
            ],
        }
    }

    /// The argv that disconnect it.
    fn disconnect(self) -> Vec<Vec<String>> {
        let argv = |items: &[&str]| items.iter().map(|item| (*item).to_owned()).collect();
        match self {
            Agent::Claude => vec![
                argv(&["plugin", "uninstall", PLUGIN, "--scope", "user"]),
                argv(&["plugin", "marketplace", "remove", NAME]),
            ],
            Agent::Codex => vec![
                argv(&["plugin", "remove", PLUGIN]),
                argv(&["plugin", "marketplace", "remove", NAME]),
            ],
        }
    }

    /// Whether the tool is here: its version, or why it could not be asked.
    fn presence(self) -> Presence {
        match exec(self, &["--version"], ASK) {
            Ok(output) if output.status.success() => {
                Presence::Installed(String::from_utf8_lossy(&output.stdout).trim().to_owned())
            }
            // Installed, but it did not answer in time.
            Err(error) if error.code == "agents" => Presence::Silent(error.error),
            _ => Presence::Missing,
        }
    }

    /// Whether the Atlas plugin is installed in this agent (unknown when
    /// its list cannot be read); an error when it does not answer.
    fn connected(self) -> std::result::Result<Option<bool>, String> {
        let output = match exec(self, &["plugin", "list", "--json"], ASK) {
            Ok(output) => output,
            Err(error) if error.code == "agents" => return Err(error.error),
            Err(_) => return Ok(None),
        };
        Ok(self.listed(&output.stdout))
    }

    /// Whether the plugin list this agent printed names the Atlas plugin.
    fn listed(self, stdout: &[u8]) -> Option<bool> {
        let list: Value = serde_json::from_slice(stdout).ok()?;
        let plugins = match self {
            Agent::Claude => list.as_array(),
            Agent::Codex => list["installed"].as_array(),
        }?;
        Some(plugins.iter().any(|plugin| {
            let id = plugin["id"].as_str().or(plugin["pluginId"].as_str());
            id == Some(PLUGIN) || plugin["name"] == NAME
        }))
    }
}

/// Whether an agent's tool is on this Mac.
enum Presence {
    Installed(String),
    /// Installed, but it did not answer in time.
    Silent(String),
    Missing,
}

/// The plugin marketplace shipped with this CLI: inside Atlas.app, or
/// in a development checkout its `agents/` folder.
pub fn marketplace() -> Result<PathBuf> {
    if let Ok(app) = crate::update::installed_app() {
        let bundled = app.join("Contents/Resources/agents");
        if bundled.is_dir() {
            return Ok(bundled);
        }
    }
    // Release builds ship only inside Atlas.app; the build machine's checkout
    // means nothing on another Mac.
    #[cfg(debug_assertions)]
    {
        let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("agents");
        if checkout.is_dir() {
            return Ok(checkout);
        }
    }
    Err(problem("This Atlas carries no agent plugin; reinstall it"))
}

/// The process groups of agent tools running now.
static RUNNING: Mutex<Vec<i32>> = Mutex::new(Vec::new());

/// Stop every agent tool Atlas is running (Atlas itself is being stopped).
pub fn stop_running() {
    let groups = RUNNING
        .lock()
        .map(|groups| groups.clone())
        .unwrap_or_default();
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

/// Run an agent's tool with no input and stop it after `limit`. Output is
/// collected as it arrives, so a background process the tool leaves holding
/// its output open cannot keep Atlas waiting.
fn exec(agent: Agent, argv: &[impl AsRef<std::ffi::OsStr>], limit: Duration) -> Result<Output> {
    use std::os::unix::process::CommandExt;
    // Its own process group, so a timeout stops whatever it started too.
    let (executable, path) = executable(agent);
    let child = Command::new(executable)
        .args(argv)
        .env("PATH", path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()?;
    let _running = Running::register(&child);
    supervise(agent, child, limit)
}

/// A process group Atlas started, listed in RUNNING while it is waited
/// for, so stop_running reaches it.
struct Running(Option<i32>);

impl Running {
    fn register(child: &std::process::Child) -> Self {
        let group = i32::try_from(child.id()).ok();
        if let (Some(group), Ok(mut running)) = (group, RUNNING.lock()) {
            running.push(group);
        }
        Self(group)
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if let (Some(group), Ok(mut running)) = (self.0, RUNNING.lock()) {
            running.retain(|running| *running != group);
        }
    }
}

/// The tool's executable, and the PATH to run it with. Opened from the
/// Finder or at login, Atlas has launchd's PATH, not the shell's, so the
/// places the tools install to are searched as well, and last the shell
/// is asked for its PATH. The tool gets every place searched, as a tool
/// installed with npm needs node beside it.
fn executable(agent: Agent) -> (PathBuf, std::ffi::OsString) {
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
/// at most; nothing when the shell does not answer.
fn shell_path() -> &'static [PathBuf] {
    static SHELL_PATH: std::sync::OnceLock<Vec<PathBuf>> = std::sync::OnceLock::new();
    SHELL_PATH.get_or_init(|| {
        use std::os::unix::process::CommandExt;
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_owned());
        // An interactive shell may print other things: the answer is marked.
        let script = "printf '\\n<atlas-path>%s</atlas-path>\\n' \"$PATH\"";
        let Ok(mut child) = Command::new(shell)
            .args(["-ilc", script])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
        else {
            return Vec::new();
        };
        let _running = Running::register(&child);
        let Some(mut stdout) = child.stdout.take() else {
            return Vec::new();
        };
        let collected = Arc::new(Mutex::new(Vec::new()));
        let (finished, ended) = std::sync::mpsc::channel();
        let sink = Arc::clone(&collected);
        std::thread::spawn(move || {
            let mut chunk = [0u8; 8192];
            while let Ok(read @ 1..) = stdout.read(&mut chunk) {
                if let Ok(mut sink) = sink.lock() {
                    let room = (1_usize << 20).saturating_sub(sink.len()).min(read);
                    sink.extend_from_slice(&chunk[..room]);
                }
            }
            let _ = finished.send(());
        });
        let deadline = Instant::now() + Duration::from_secs(3);
        while child.try_wait().ok().flatten().is_none() {
            if Instant::now() >= deadline {
                if let Ok(group) = i32::try_from(child.id()) {
                    let _ = nix::sys::signal::killpg(
                        nix::unistd::Pid::from_raw(group),
                        nix::sys::signal::Signal::SIGKILL,
                    );
                }
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        // Normally the output ends with the shell. A background job it left
        // holding the pipe open cannot keep Atlas waiting: a moment more,
        // then what arrived.
        let _ = ended.recv_timeout(Duration::from_millis(500));
        let text = collected
            .lock()
            .map(|text| text.clone())
            .unwrap_or_default();
        marked_path(&String::from_utf8_lossy(&text))
    })
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

/// Wait for `child` within `limit`, collecting its output.
fn supervise(agent: Agent, mut child: std::process::Child, limit: Duration) -> Result<Output> {
    // Output is collected as it arrives, up to a limit (the rest is read
    // and dropped, so the tool never blocks on a full pipe).
    const KEEP: usize = 1 << 20;
    let collect = |stream: Option<Box<dyn Read + Send>>| {
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
    };
    let stdout = collect(
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>),
    );
    let stderr = collect(
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>),
    );
    let deadline = Instant::now() + limit;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            if let Ok(group) = i32::try_from(child.id()) {
                let _ = nix::sys::signal::killpg(
                    nix::unistd::Pid::from_raw(group),
                    nix::sys::signal::Signal::SIGKILL,
                );
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err(problem(format!(
                "{} did not answer within {} seconds",
                agent.display_name(),
                limit.as_secs()
            )));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    // Normally the output ends with the tool; a moment more, then keep
    // what arrived.
    let take = |(collected, ended): (Arc<Mutex<Vec<u8>>>, std::sync::mpsc::Receiver<()>)| {
        let _ = ended.recv_timeout(Duration::from_millis(500));
        collected
            .lock()
            .map(|bytes| bytes.clone())
            .unwrap_or_default()
    };
    Ok(Output {
        status,
        stdout: take(stdout),
        stderr: take(stderr),
    })
}

fn run(agent: Agent, argv: &[String]) -> Result<()> {
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

/// Each agent on this Mac and whether Atlas is connected to it.
pub fn status() -> Value {
    let agents: Vec<Value> = std::thread::scope(|scope| {
        let probes: Vec<_> = Agent::ALL
            .map(|agent| scope.spawn(move || probe(agent)))
            .into_iter()
            .collect();
        probes
            .into_iter()
            .map(|probe| probe.join().unwrap_or(Value::Null))
            .collect()
    });
    json!({"ok": true, "agents": agents, "plugin": marketplace().ok()})
}

fn probe(agent: Agent) -> Value {
    match agent.presence() {
        Presence::Installed(version) => match agent.connected() {
            Ok(connected) => json!({
                "agent": agent.name(),
                "installed": true,
                "version": version,
                "connected": connected,
            }),
            Err(error) => json!({
                "agent": agent.name(),
                "installed": true,
                "version": version,
                "error": error,
            }),
        },
        Presence::Silent(error) => json!({
            "agent": agent.name(),
            "installed": true,
            "error": error,
        }),
        Presence::Missing => json!({"agent": agent.name(), "installed": false}),
    }
}

/// Connect (or `remove` from) the chosen agents, or every installed one.
pub fn change(chosen: &[Agent], remove: bool) -> Result<Value> {
    let marketplace = if remove { None } else { Some(marketplace()?) };
    let present: Vec<(Agent, Presence)> = if chosen.is_empty() {
        Agent::ALL
            .into_iter()
            .map(|agent| (agent, agent.presence()))
            .filter(|(_, presence)| !matches!(presence, Presence::Missing))
            .collect()
    } else {
        chosen
            .iter()
            .map(|agent| (*agent, agent.presence()))
            .collect()
    };
    if present.is_empty() {
        return Err(problem("Neither Claude Code nor Codex is installed"));
    }
    let mut results = Vec::new();
    for (agent, presence) in present {
        match presence {
            Presence::Installed(_) => {}
            Presence::Silent(error) => {
                results.push(json!({"agent": agent.name(), "ok": false, "error": error}));
                continue;
            }
            Presence::Missing => {
                results.push(json!({"agent": agent.name(), "ok": false, "error": "not installed"}));
                continue;
            }
        }
        let outcome = match &marketplace {
            // Connecting stops at a failure.
            Some(path) => agent
                .connect(path)
                .iter()
                .map(|argv| run(agent, argv))
                .find(Result::is_err)
                .unwrap_or(Ok(())),
            // Removing runs every step, tolerating parts already gone, then
            // checks the plugin is gone.
            None => {
                let failures = agent
                    .disconnect()
                    .iter()
                    .filter_map(|argv| run(agent, argv).err())
                    .collect();
                removed(agent, failures, agent.connected())
            }
        };
        results.push(match outcome {
            Ok(()) => json!({"agent": agent.name(), "ok": true, "connected": !remove}),
            Err(error) => json!({"agent": agent.name(), "ok": false, "error": error.error}),
        });
    }
    let ok = results.iter().all(|result| result["ok"] == true);
    Ok(json!({"ok": ok, "agents": results}))
}

/// Whether removing from `agent` worked, from the steps that failed and
/// whether the agent still lists the plugin afterwards. A failed step on a
/// plugin that is gone was a part already absent; when the list cannot be
/// read, a failed step is reported.
fn removed(
    agent: Agent,
    failures: Vec<Error>,
    connected: std::result::Result<Option<bool>, String>,
) -> Result<()> {
    let first = failures.into_iter().next();
    match connected {
        Ok(Some(false)) => Ok(()),
        Ok(Some(true)) => Err(first.unwrap_or_else(|| {
            problem(format!(
                "{} still lists the Atlas plugin",
                agent.display_name()
            ))
        })),
        Ok(None) | Err(_) => first.map_or(Ok(()), Err),
    }
}

/// The section `atlas agents project` keeps in a project's AGENTS.md: when
/// to test in the helper, and how. The README shows the same text.
pub const PROJECT_SECTION: &str = include_str!("agent_project.md");
const SECTION_START: &str = "<!-- atlas:start -->";
const SECTION_END: &str = "<!-- atlas:end -->";

/// Add the Atlas section to a project's AGENTS.md, or bring it up to date,
/// and to its CLAUDE.md when that is a separate file that does not already
/// include AGENTS.md. Nothing else in either file changes, and a file is
/// replaced whole or not at all.
pub fn project(dir: &Path) -> Result<Value> {
    if !dir.is_dir() {
        return Err(problem(format!("{} is not a folder", dir.display())));
    }
    let agents = dir.join("AGENTS.md");
    let claude = dir.join("CLAUDE.md");
    let mut plans = vec![(agents.clone(), read_instructions(&agents)?)];
    if let Some(text) = read_instructions(&claude)? {
        let same = match (claude.canonicalize(), agents.canonicalize()) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        };
        if !same && !text.contains("@AGENTS.md") {
            plans.push((claude, Some(text)));
        }
    }
    // Check every file before changing any.
    let mut changes = Vec::new();
    for (path, before) in plans {
        let before = before.unwrap_or_default();
        let after = with_section(&before).ok_or_else(|| {
            problem(format!(
                "{} has an Atlas marker without its pair. Remove the lines \
                 {SECTION_START} and {SECTION_END}, then run this again",
                path.display()
            ))
        })?;
        changes.push((path, before, after));
    }
    let mut files = Vec::new();
    for (path, before, after) in changes {
        let change = if before == after {
            "unchanged"
        } else if before.contains(SECTION_START) {
            "updated"
        } else {
            "added"
        };
        if before != after {
            replace_file(&path, after.as_bytes())
                .map_err(|error| problem(format!("Could not write {}: {error}", path.display())))?;
        }
        files.push(json!({"path": path, "change": change}));
    }
    Ok(json!({"ok": true, "files": files}))
}

/// An instruction file's text, or None when there is none.
fn read_instructions(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(problem(format!(
            "Could not read {}: {error}",
            path.display()
        ))),
    }
}

/// `text` with the Atlas section in place of an earlier one, or appended;
/// None when the markers are not exactly one pair in order.
fn with_section(text: &str) -> Option<String> {
    let block = format!("{SECTION_START}\n{PROJECT_SECTION}{SECTION_END}\n");
    match (
        text.matches(SECTION_START).count(),
        text.matches(SECTION_END).count(),
    ) {
        (0, 0) => {}
        (1, 1) => {
            let start = text.find(SECTION_START)?;
            let end = text.find(SECTION_END).filter(|&end| end > start)?;
            let rest = &text[end + SECTION_END.len()..];
            return Some(format!(
                "{}{block}{}",
                &text[..start],
                rest.strip_prefix('\n').unwrap_or(rest)
            ));
        }
        _ => return None,
    }
    let gap = match text {
        "" => "",
        _ if text.ends_with("\n\n") => "",
        _ if text.ends_with('\n') => "\n",
        _ => "\n\n",
    };
    Some(format!("{text}{gap}{block}"))
}

/// Replace a file through a temporary file beside it, so a failure leaves
/// the old text whole. A link is followed and its target replaced; the
/// file keeps its permissions.
fn replace_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt, os::unix::fs::PermissionsExt};
    let target = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mode = std::fs::metadata(&target).map_or(0o644, |m| m.permissions().mode() & 0o777);
    let temporary = target.with_file_name(format!(
        ".{}.atlas-{}",
        target
            .file_name()
            .map_or("instructions".into(), |n| n.to_string_lossy()),
        crate::control::token()
    ));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temporary)?;
        // The mode given at creation is reduced by the umask.
        file.set_permissions(std::fs::Permissions::from_mode(mode))?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, &target)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

/// After an update, bring each connected agent's copy of the plugin up to
/// date (agents cache plugins when installing). Never fails the update.
pub fn refresh() -> Value {
    let refreshed: Vec<Value> = Agent::ALL
        .into_iter()
        .filter(|agent| agent.connected() == Ok(Some(true)))
        .map(|agent| {
            let steps: Vec<Vec<String>> = match agent {
                Agent::Claude => vec![
                    vec![
                        "plugin".into(),
                        "marketplace".into(),
                        "update".into(),
                        NAME.into(),
                    ],
                    vec!["plugin".into(), "update".into(), PLUGIN.into()],
                ],
                Agent::Codex => vec![vec!["plugin".into(), "add".into(), PLUGIN.into()]],
            };
            let failed = steps.iter().find_map(|argv| run(agent, argv).err());
            json!({"agent": agent.name(), "ok": failed.is_none(), "error": failed.map(|e| e.error)})
        })
        .collect();
    json!({"agents": refreshed})
}

/// One line for an agent's session start: local, bounded, never failing.
pub async fn brief(config_path: &Path) -> String {
    let version = crate::VERSION;
    let Ok(config) = Config::load(config_path) else {
        return "Atlas is installed but not set up. If the person wants GUI testing, \
                offer the atlas setup skill (the person opens Atlas)."
            .to_owned();
    };
    let Ok(client) = Client::new(config.clone()) else {
        return format!("Atlas {version}: run atlas doctor before using it.");
    };
    if client
        .controller
        .pending()
        .is_ok_and(|pending| !pending.is_empty())
    {
        return format!(
            "Atlas {version}: an action's outcome is unknown; run atlas restart first."
        );
    }
    let Ok(status) = tokio::time::timeout(Duration::from_millis(1500), client.status()).await
    else {
        return format!("Atlas {version}: the helper is not answering; run atlas doctor.");
    };
    let ready = crate::capability::doctor(status, config.helper_uid)["ready"] == true;
    if ready {
        format!(
            "Atlas {version}: ready. After any change a person could see, check it in the \
             hidden helper with the atlas skill before saying it works."
        )
    } else {
        format!("Atlas {version}: not ready; run atlas doctor and tell the person what it says.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tool_is_found_where_it_can_run() {
        let dir = tempfile::tempdir().unwrap();
        let (first, second) = (dir.path().join("first"), dir.path().join("second"));
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        fs::write(first.join("claude"), b"").unwrap();
        let places = vec![dir.path().join("missing"), first.clone(), second.clone()];
        assert_eq!(find("claude", &places), None, "not runnable yet");
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(second.join("claude"), fs::Permissions::from_mode(0o755)).ok();
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

    #[test]
    fn the_project_section_is_added_once_and_kept_current() {
        let block = format!("{SECTION_START}\n{PROJECT_SECTION}{SECTION_END}\n");
        assert_eq!(with_section("").unwrap(), block);
        assert_eq!(
            with_section("# Rules").unwrap(),
            format!("# Rules\n\n{block}")
        );
        assert_eq!(
            with_section("# Rules\n").unwrap(),
            format!("# Rules\n\n{block}")
        );
        let once = with_section("# Rules\n\nKeep this.\n").unwrap();
        assert_eq!(
            with_section(&once).unwrap(),
            once,
            "a second run changes nothing"
        );
        let old = format!("# Rules\n{SECTION_START}\nold words\n{SECTION_END}\nAfter.\n");
        assert_eq!(
            with_section(&old).unwrap(),
            format!("# Rules\n{block}After.\n")
        );
        // An unpaired or reversed marker is refused, never guessed at.
        for broken in [
            format!("{SECTION_START}\nKeep this.\n"),
            format!("Keep this.\n{SECTION_END}\n"),
            format!("{SECTION_END}\n{SECTION_START}\n"),
            format!("{once}{SECTION_START}\n"),
        ] {
            assert_eq!(with_section(&broken), None, "{broken}");
        }
    }

    #[test]
    fn a_broken_marker_leaves_every_file_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("AGENTS.md"), "# Rules\n").unwrap();
        let broken = format!("# Claude\n{SECTION_START}\nKeep this.\n");
        std::fs::write(dir.path().join("CLAUDE.md"), &broken).unwrap();
        assert!(project(dir.path()).is_err());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("AGENTS.md")).unwrap(),
            "# Rules\n"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap(),
            broken
        );
    }

    #[test]
    fn a_project_gets_the_section_in_agents_and_a_separate_claude_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "# Claude\n").unwrap();
        let result = project(dir.path()).unwrap();
        let changes: Vec<&str> = result["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|file| file["change"].as_str().unwrap())
            .collect();
        assert_eq!(changes, ["added", "added"]);
        for name in ["AGENTS.md", "CLAUDE.md"] {
            let text = std::fs::read_to_string(dir.path().join(name)).unwrap();
            assert!(text.contains(PROJECT_SECTION), "{name}");
        }
        assert_eq!(
            project(dir.path()).unwrap()["files"][0]["change"],
            "unchanged"
        );

        // A CLAUDE.md that already includes AGENTS.md is left alone.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "@AGENTS.md\n").unwrap();
        assert_eq!(
            project(dir.path()).unwrap()["files"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap(),
            "@AGENTS.md\n"
        );
    }

    #[test]
    fn a_replaced_file_keeps_its_exact_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("AGENTS.md");
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o664)).unwrap();
        replace_file(&path, b"new").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o664);
    }

    #[test]
    fn the_plugin_is_found_in_each_agents_list() {
        assert_eq!(PLUGIN, format!("{NAME}@{NAME}"));
        let claude = br#"[{"id":"other@x"},{"id":"atlas@atlas","version":"1"}]"#;
        assert_eq!(Agent::Claude.listed(claude), Some(true));
        assert_eq!(Agent::Claude.listed(br#"[{"id":"other@x"}]"#), Some(false));
        let codex = br#"{"installed":[{"pluginId":"atlas@atlas"}]}"#;
        assert_eq!(Agent::Codex.listed(codex), Some(true));
        assert_eq!(Agent::Codex.listed(br#"{"installed":[]}"#), Some(false));
        assert_eq!(Agent::Codex.listed(claude), None, "not Codex's shape");
        assert_eq!(Agent::Claude.listed(b"not json"), None);
    }

    /// Removing reports success only when the plugin is gone, or when every
    /// step worked and the list cannot be read.
    #[test]
    fn removing_checks_the_plugin_is_gone() {
        let failed = || vec![problem("claude plugin uninstall failed: not found")];
        assert!(removed(Agent::Claude, failed(), Ok(Some(false))).is_ok());
        assert!(removed(Agent::Claude, Vec::new(), Ok(Some(false))).is_ok());
        let error = removed(Agent::Claude, failed(), Ok(Some(true))).unwrap_err();
        assert!(error.error.contains("uninstall failed"));
        let error = removed(Agent::Codex, Vec::new(), Ok(Some(true))).unwrap_err();
        assert_eq!(error.error, "Codex still lists the Atlas plugin");
        assert!(removed(Agent::Claude, Vec::new(), Ok(None)).is_ok());
        assert!(removed(Agent::Claude, failed(), Ok(None)).is_err());
        assert!(removed(Agent::Claude, failed(), Err("silent".into())).is_err());
    }

    #[test]
    fn the_readme_shows_the_project_section() {
        let readme = include_str!("../../README.md");
        assert!(readme.contains(PROJECT_SECTION.trim_end()));
    }

    #[test]
    fn connecting_registers_the_local_marketplace_then_installs() {
        let path = Path::new("/Applications/Atlas.app/Contents/Resources/agents");
        assert_eq!(
            Agent::Claude.connect(path),
            [
                vec![
                    "plugin",
                    "marketplace",
                    "add",
                    path.to_str().unwrap(),
                    "--scope",
                    "user"
                ],
                vec!["plugin", "install", "atlas@atlas", "--scope", "user"],
            ]
        );
        assert_eq!(
            Agent::Codex.connect(path),
            [
                vec!["plugin", "marketplace", "add", path.to_str().unwrap()],
                vec!["plugin", "add", "atlas@atlas"],
            ]
        );
        assert_eq!(
            Agent::Codex.disconnect()[0],
            ["plugin", "remove", "atlas@atlas"]
        );
        assert_eq!(
            Agent::Claude.disconnect()[1],
            ["plugin", "marketplace", "remove", "atlas"]
        );
    }

    /// The plugin's manifests describe this version, and its skill and
    /// command reference are exactly what this CLI generates.
    #[test]
    fn the_plugin_matches_this_cli() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("agents");
        for manifest in [".claude-plugin/plugin.json", ".codex-plugin/plugin.json"] {
            let text = std::fs::read_to_string(root.join("atlas").join(manifest)).unwrap();
            let value: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(value["name"], NAME, "{manifest}");
            assert_eq!(value["version"], crate::VERSION, "{manifest}");
        }
        for marketplace in [
            ".claude-plugin/marketplace.json",
            ".agents/plugins/marketplace.json",
        ] {
            let text = std::fs::read_to_string(root.join(marketplace)).unwrap();
            let value: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(value["name"], NAME, "{marketplace}");
        }
        let skill = std::fs::read_to_string(root.join("atlas/skills/atlas/SKILL.md")).unwrap();
        assert_eq!(
            skill,
            crate::agent::skill(),
            "regenerate with: atlas agent-guide --skill"
        );
    }

    /// Every error code the CLI can report is explained to the agent.
    #[test]
    fn every_error_code_is_in_the_troubleshooting_reference() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let reference = std::fs::read_to_string(
            root.join("agents/atlas/skills/atlas/references/troubleshooting.md"),
        )
        .unwrap();
        let mut sources = Vec::new();
        let mut directories = vec![root.join("src")];
        while let Some(directory) = directories.pop() {
            for entry in std::fs::read_dir(directory).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    directories.push(path);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    sources.push(std::fs::read_to_string(path).unwrap());
                }
            }
        }
        let mut missing = Vec::new();
        for source in &sources {
            for start in ["Error::new(", "failure("] {
                for (index, _) in source.match_indices(start) {
                    let rest = source[index + start.len()..].trim_start();
                    let Some(code) = rest
                        .strip_prefix('"')
                        .and_then(|rest| rest.split('"').next())
                    else {
                        continue;
                    };
                    let documented = format!("| `{code}` |");
                    if !code.is_empty()
                        && code.chars().all(|c| c.is_ascii_lowercase() || c == '-')
                        && !reference.contains(&documented)
                        && !missing.contains(&code.to_owned())
                    {
                        missing.push(code.to_owned());
                    }
                }
            }
        }
        // Codes reached through each module's own problem() helper.
        for code in ["agents", "init", "setup", "uninstall", "update"] {
            if !reference.contains(&format!("| `{code}` |")) && !missing.contains(&code.to_owned())
            {
                missing.push(code.to_owned());
            }
        }
        assert!(missing.is_empty(), "undocumented error codes: {missing:?}");
    }
}
