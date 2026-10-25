//! `atlas agents`: connect coding agents to Atlas. The
//! plugin ships inside Atlas.app, matching this CLI. Claude Code and Codex
//! use a local marketplace; Pi references the package, and opencode loads
//! a symlinked plugin. Each gets the Atlas skill, MCP and a session brief.
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
    process::{Child, Command, Output, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

mod opencode;
mod pi;

/// The marketplace and plugin name in the marketplace-based agents.
const NAME: &str = "atlas";
/// The plugin as both agents name it: `NAME@NAME`, the plugin in its marketplace.
const PLUGIN: &str = "atlas@atlas";
/// The homepage Atlas's plugin manifests give, which marks a marketplace
/// folder as Atlas's own wherever it is.
const HOMEPAGE: &str = "https://github.com/InsaneArts/atlas";

/// How long a question to an agent's tool may take.
const ASK: Duration = Duration::from_secs(10);
/// How long a change to an agent's tool may take.
const CHANGE: Duration = Duration::from_secs(120);

fn problem(message: impl Into<String>) -> Error {
    Error::new("agents", message)
}

/// The agents Atlas can connect, and their command-line tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    Claude,
    Codex,
    Opencode,
    Pi,
}

impl Agent {
    pub const ALL: [Agent; 4] = [Agent::Claude, Agent::Codex, Agent::Opencode, Agent::Pi];

    pub fn name(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
            Agent::Pi => "pi",
            Agent::Opencode => "opencode",
        }
    }

    /// The name a person knows it by.
    pub fn display_name(self) -> &'static str {
        match self {
            Agent::Claude => "Claude Code",
            Agent::Codex => "Codex",
            Agent::Pi => "Pi",
            Agent::Opencode => "opencode",
        }
    }

    /// The agent `name` (as in `status`), if it is one.
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|agent| agent.name() == name)
    }

    /// The argv that connect the plugin at `marketplace`.
    fn connect(self, marketplace: &Path) -> Vec<Vec<String>> {
        let path = marketplace.to_string_lossy().into_owned();

        match self {
            Agent::Claude => vec![
                argv(&["plugin", "marketplace", "add", &path, "--scope", "user"]),
                argv(&["plugin", "install", PLUGIN, "--scope", "user"]),
            ],
            Agent::Codex => vec![
                argv(&["plugin", "marketplace", "add", &path]),
                argv(&["plugin", "add", PLUGIN]),
            ],
            Agent::Pi | Agent::Opencode => vec![],
        }
    }

    /// The argv that connect it to `marketplace`, given where its
    /// marketplace named atlas comes from (`source`, unknown when `None`).
    /// One from another
    /// folder of Atlas's own gives way first (Codex refuses to add over it;
    /// Claude Code replaces it itself); anyone else's is never touched.
    /// Claude Code would replace one it cannot tell is Atlas's too, so it is
    /// not connected while its marketplaces cannot be read.
    fn connecting(self, marketplace: &Path, source: Option<&Source>) -> Result<Vec<Vec<String>>> {
        let mut steps = Vec::new();

        match source {
            Some(Source::Folder(folder)) if same_place(folder, marketplace) => {}
            Some(Source::Folder(folder)) if self.ours(folder) => {
                if self == Agent::Codex {
                    steps.push(argv(&["plugin", "marketplace", "remove", NAME]));
                }
            }
            Some(Source::Folder(folder)) => {
                return Err(self.another(&folder.display().to_string()));
            }
            Some(Source::Elsewhere(place)) => return Err(self.another(place)),
            Some(Source::Absent) => {}
            None if self == Agent::Claude => return Err(self.unreadable()),
            None => {}
        }

        steps.extend(self.connect(marketplace));
        Ok(steps)
    }

    /// Its marketplace named atlas is someone else's, from `place`.
    fn another(self, place: &str) -> Error {
        problem(format!(
            "{} has another marketplace named atlas, from {place}. Remove it with {} plugin \
             marketplace remove atlas, then connect again",
            self.display_name(),
            self.name()
        ))
    }

    /// Its marketplaces could not be read.
    fn unreadable(self) -> Error {
        problem(format!(
            "{}'s marketplaces could not be read",
            self.display_name()
        ))
    }

    /// Whether `folder`, where its marketplace named atlas comes from, is
    /// Atlas's own: a folder of Atlas's, or one since removed whose plugin
    /// this agent keeps a copy of that is Atlas's.
    fn ours(self, folder: &Path) -> bool {
        atlas_folder(folder) || (matches!(folder.try_exists(), Ok(false)) && self.keeps_atlas())
    }

    /// Whether this agent's copy of the plugin named atlas (agents copy a
    /// plugin when installing it) is Atlas's.
    fn keeps_atlas(self) -> bool {
        if matches!(self, Agent::Pi | Agent::Opencode) {
            return false;
        }
        let Some(home) = self.home() else {
            return false;
        };

        fs::read_dir(home.join("plugins/cache").join(NAME).join(NAME))
            .into_iter()
            .flatten()
            .flatten()
            .any(|version| atlas_plugin(&version.path()))
    }

    /// The folder this agent keeps its settings in, as its tool finds it
    /// with Atlas's environment.
    fn home(self) -> Option<PathBuf> {
        let (variable, folder) = match self {
            Agent::Claude => ("CLAUDE_CONFIG_DIR", ".claude"),
            Agent::Codex => ("CODEX_HOME", ".codex"),
            Agent::Pi => return pi::home(),
            Agent::Opencode => return opencode::home(),
        };

        std::env::var_os(variable)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(folder)))
    }

    /// The argv that disconnect it.
    fn disconnect(self) -> Vec<Vec<String>> {
        match self {
            Agent::Claude => vec![
                argv(&["plugin", "uninstall", PLUGIN, "--scope", "user"]),
                argv(&["plugin", "marketplace", "remove", NAME]),
            ],
            Agent::Codex => vec![
                argv(&["plugin", "remove", PLUGIN]),
                argv(&["plugin", "marketplace", "remove", NAME]),
            ],
            Agent::Pi | Agent::Opencode => vec![],
        }
    }

    /// The argv that bring its copy of the plugin up to date.
    fn update(self) -> Vec<Vec<String>> {
        match self {
            Agent::Claude => vec![
                argv(&["plugin", "marketplace", "update", NAME]),
                argv(&["plugin", "update", PLUGIN]),
            ],
            Agent::Codex => vec![argv(&["plugin", "add", PLUGIN])],
            Agent::Pi | Agent::Opencode => vec![],
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

    /// Whether this agent uses the Atlas plugin from this Atlas's own
    /// marketplace (unknown when its lists cannot be read); an error when it
    /// does not answer.
    fn connected(self) -> std::result::Result<Option<bool>, String> {
        if self != Agent::Pi {
            match self.installed()? {
                Some(true) => {}
                other => return Ok(other),
            }
        }

        Ok(self.source()?.map(|source| match source {
            Source::Folder(folder) => this_atlas(&folder),
            Source::Absent | Source::Elsewhere(_) => false,
        }))
    }

    /// Whether the Atlas plugin is installed in this agent, from whichever
    /// marketplace (unknown when its list cannot be read); an error when it
    /// does not answer.
    fn installed(self) -> std::result::Result<Option<bool>, String> {
        if self == Agent::Opencode {
            return opencode::installed().map(Some).map_err(|e| e.error);
        }
        if self == Agent::Pi {
            return self
                .source()
                .map(|source| source.map(|s| s != Source::Absent));
        }
        // Codex's whole list asks its remote marketplaces too, which can
        // take longer than ASK; Atlas's own marketplace answers at once.
        let list: &[&str] = match self {
            Agent::Claude => &["plugin", "list", "--json"],
            Agent::Codex => &["plugin", "list", "--marketplace", NAME, "--json"],
            Agent::Pi | Agent::Opencode => return Ok(None),
        };

        let output = match exec(self, list, ASK) {
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
            Agent::Pi | Agent::Opencode => return None,
        }?;

        Some(plugins.iter().any(|plugin| {
            let id = plugin["id"].as_str().or(plugin["pluginId"].as_str());
            id == Some(PLUGIN) || plugin["name"] == NAME
        }))
    }

    /// Where its marketplace named atlas comes from (unknown when its list
    /// cannot be read); an error when it does not answer. Codex's whole list
    /// fails while any of its local marketplace folders is missing, so its
    /// settings, where adding a marketplace records it, are read first.
    fn source(self) -> std::result::Result<Option<Source>, String> {
        if self == Agent::Opencode {
            return opencode::source().map(Some).map_err(|e| e.error);
        }
        if self == Agent::Pi {
            return pi::source().map(Some).map_err(|e| e.error);
        }
        let recorded = match self {
            Agent::Claude => None,
            Agent::Codex => self.home().and_then(|home| codex_recorded(&home)),
            Agent::Pi | Agent::Opencode => None,
        };

        if let Some(found @ (Source::Folder(_) | Source::Elsewhere(_))) = &recorded {
            return Ok(Some(found.clone()));
        }

        match exec(self, &["plugin", "marketplace", "list", "--json"], ASK) {
            Ok(output) => Ok(self.source_in(&output.stdout).or(recorded)),
            Err(error) if error.code == "agents" => Err(error.error),
            Err(_) => Ok(recorded),
        }
    }

    /// Where the marketplace named atlas comes from, in the marketplace list
    /// this agent printed.
    fn source_in(self, stdout: &[u8]) -> Option<Source> {
        let list: Value = serde_json::from_slice(stdout).ok()?;
        let marketplaces = match self {
            Agent::Claude => list.as_array(),
            Agent::Codex => list["marketplaces"].as_array(),
            Agent::Pi | Agent::Opencode => return None,
        }?;

        let Some(found) = marketplaces.iter().find(|entry| entry["name"] == NAME) else {
            return Some(Source::Absent);
        };

        // Claude Code: {"source": "directory", "path": …}, or a repository
        // or URL. Codex: {"marketplaceSource": {"sourceType": "local", "source": …}}.
        let (kind, place) = match self {
            Agent::Claude => (
                found["source"].as_str(),
                ["path", "repo", "url"]
                    .into_iter()
                    .find_map(|key| found[key].as_str()),
            ),
            Agent::Codex => (
                found["marketplaceSource"]["sourceType"].as_str(),
                found["marketplaceSource"]["source"].as_str(),
            ),
            Agent::Pi | Agent::Opencode => return None,
        };

        Some(match (kind, place) {
            (Some("directory" | "local"), Some(folder)) => Source::Folder(PathBuf::from(folder)),
            (_, place) => Source::Elsewhere(place.unwrap_or("somewhere else").to_owned()),
        })
    }
}

/// Where an agent's marketplace named atlas comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Source {
    /// It has none.
    Absent,
    /// A folder on this Mac.
    Folder(PathBuf),
    /// Somewhere else, such as a Git repository.
    Elsewhere(String),
}

/// Whether `a` and `b` are the same file or folder, links resolved when
/// they exist.
pub(crate) fn same_place(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Whether `folder` is this Atlas's marketplace, or (when this Atlas
/// carries none) a marketplace of Atlas's own.
fn this_atlas(folder: &Path) -> bool {
    match marketplace() {
        Ok(ours) => same_place(folder, &ours),
        Err(_) => atlas_folder(folder),
    }
}

/// Whether `folder` is a marketplace of Atlas's own: this Atlas's, one
/// inside an app that is Atlas (or inside any app, once removed), or one
/// whose plugin manifest gives Atlas's homepage, such as a checkout's.
fn atlas_folder(folder: &Path) -> bool {
    if marketplace().is_ok_and(|ours| same_place(folder, &ours)) {
        return true;
    }

    let app = folder
        .ancestors()
        .nth(3)
        .filter(|app| app.extension().is_some_and(|extension| extension == "app"))
        .filter(|_| folder.ends_with("Contents/Resources/agents"));
    if let Some(app) = app
        && (matches!(folder.try_exists(), Ok(false)) || atlas_bundle(app))
    {
        return true;
    }

    atlas_plugin(&folder.join(NAME))
}

/// Whether `plugin` is Atlas's plugin: a manifest of it gives Atlas's homepage.
fn atlas_plugin(plugin: &Path) -> bool {
    [".claude-plugin/plugin.json", ".codex-plugin/plugin.json"]
        .iter()
        .filter_map(|manifest| fs::read(plugin.join(manifest)).ok())
        .filter_map(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .any(|manifest| manifest["name"] == NAME && manifest["homepage"] == HOMEPAGE)
}

/// Whether the app bundle at `app` is Atlas, by its bundle identifier.
fn atlas_bundle(app: &Path) -> bool {
    let Ok(info) = fs::read_to_string(app.join("Contents/Info.plist")) else {
        return false;
    };

    info.split("<key>CFBundleIdentifier</key>")
        .nth(1)
        .and_then(|rest| rest.trim_start().strip_prefix("<string>"))
        .and_then(|rest| rest.split("</string>").next())
        == Some(crate::setup::IDENTIFIER)
}

/// Where Codex's marketplace named atlas comes from, as the settings in
/// `home` record it: None when they cannot be read.
fn codex_recorded(home: &Path) -> Option<Source> {
    match fs::read_to_string(home.join("config.toml")) {
        Ok(text) => codex_recorded_in(&text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(Source::Absent),
        Err(_) => None,
    }
}

/// Where the marketplace named atlas comes from, in Codex's settings:
/// `[marketplaces.atlas]` with `source_type` and `source`.
fn codex_recorded_in(text: &str) -> Option<Source> {
    let settings: toml::Table = text.parse().ok()?;
    let Some(found) = settings
        .get("marketplaces")
        .and_then(|marketplaces| marketplaces.get(NAME))
    else {
        return Some(Source::Absent);
    };

    let kind = found.get("source_type").and_then(toml::Value::as_str);
    let place = found.get("source").and_then(toml::Value::as_str);

    Some(match (kind, place) {
        (Some("local"), Some(folder)) => Source::Folder(PathBuf::from(folder)),
        (_, place) => Source::Elsewhere(place.unwrap_or("somewhere else").to_owned()),
    })
}

/// One argv of owned words.
fn argv(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

/// Serialize Atlas's multi-step local registrations without taking the host's own settings lock.
fn connection_lock() -> Result<fs::File> {
    use fs2::FileExt;
    use std::os::unix::fs::OpenOptionsExt;
    let path =
        std::env::temp_dir().join(format!("atlas-agents-{}.lock", nix::unistd::Uid::current()));
    let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| problem(format!("Could not open the agent connection lock: {error}")))?;
    file.try_lock_exclusive()
        .map_err(|error| problem(format!("Another agent connection is running: {error}")))?;
    Ok(file)
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
fn exec(agent: Agent, argv: &[impl AsRef<std::ffi::OsStr>], limit: Duration) -> Result<Output> {
    exec_in(agent, argv, limit, None)
}

/// Run a tool in a particular config folder, retaining timeout and cancellation handling.
fn exec_in(
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

    let mut agents = agents;
    agents.push(crate::claude_app::status());
    json!({"ok": true, "agents": agents, "plugin": marketplace().ok(), "projects": taught()})
}

/// One agent's row for `status`: whether its tool is here, and whether
/// Atlas is connected to it.
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
/// Only a marketplace named atlas that is Atlas's own is replaced or
/// removed; an agent whose is someone else's is refused when connecting and
/// left alone when removing.
pub fn change(chosen: &[Agent], remove: bool) -> Result<Value> {
    // The Claude app comes along when no agent was named: it has no
    // command line of its own to choose it by.
    let app = chosen.is_empty() && crate::claude_app::installed();
    let all = chosen.is_empty();
    change_these(chosen, all, app, remove)
}

/// [`change`] for exactly `chosen` (every installed agent when `all`) and
/// the Claude app when `app`.
pub fn change_these(chosen: &[Agent], all: bool, app: bool, remove: bool) -> Result<Value> {
    let marketplace = if remove { None } else { Some(marketplace()?) };

    let present: Vec<(Agent, Presence)> = if all {
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

    if present.is_empty() && !app {
        return Err(problem(format!(
            "None of {} or the Claude app is installed",
            Agent::ALL.map(Agent::display_name).join(", ")
        )));
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

        let outcome = if agent == Agent::Pi {
            pi::change(marketplace.as_deref())
        } else {
            // Where its marketplace named atlas comes from now decides what may
            // change: never one that is someone else's.
            let source = agent.source();

            match &marketplace {
                Some(path) => connect_agent(agent, path, &source),
                None => disconnect_agent(agent, source),
            }
        };

        let mut result = match &outcome {
            Ok(removal) => json!({
                "agent": agent.name(),
                "ok": true,
                "connected": !remove,
                "changed": *removal == Removal::Changed,
            }),
            Err(error) => json!({"agent": agent.name(), "ok": false, "error": error.error}),
        };

        if agent == Agent::Opencode && matches!(outcome, Ok(Removal::Changed)) {
            result["restart"] = json!(true);
        }

        if let Ok(Removal::Kept(place)) = outcome {
            result["kept"] = json!(place);
        }

        results.push(result);
    }

    if app {
        results.push(crate::claude_app::change(!remove));
    }

    let ok = results.iter().all(|result| result["ok"] == true);
    Ok(json!({"ok": ok, "agents": results}))
}

/// Connect `agent` to `marketplace`, given where its marketplace named
/// atlas comes from (`source`, unknown when unreadable): the steps stop at
/// a failure.
fn connect_agent(
    agent: Agent,
    marketplace: &Path,
    source: &std::result::Result<Option<Source>, String>,
) -> Result<Removal> {
    if agent == Agent::Opencode {
        return opencode::change(Some(marketplace));
    }
    let steps = agent.connecting(marketplace, source.as_ref().ok().and_then(Option::as_ref))?;

    steps
        .iter()
        .map(|argv| run(agent, argv))
        .find(Result::is_err)
        .unwrap_or(Ok(()))
        .map(|()| Removal::Changed)
}

/// What removing the plugin from an agent did.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Removal {
    Changed,
    /// It had no marketplace named atlas.
    Nothing,
    /// Its marketplace named atlas is someone else's, from this place.
    Kept(String),
}

/// Disconnect `agent` when its marketplace named atlas (from `source`) is
/// Atlas's own: every step runs, tolerating parts already gone, then the
/// plugin must be gone.
fn disconnect_agent(
    agent: Agent,
    source: std::result::Result<Option<Source>, String>,
) -> Result<Removal> {
    if agent == Agent::Opencode {
        return opencode::change(None);
    }
    match source.map_err(problem)? {
        Some(Source::Folder(folder)) if agent.ours(&folder) => {}
        Some(Source::Folder(folder)) => return Ok(Removal::Kept(folder.display().to_string())),
        Some(Source::Elsewhere(place)) => return Ok(Removal::Kept(place)),
        Some(Source::Absent) => return Ok(Removal::Nothing),
        None => return Err(agent.unreadable()),
    }

    let failures = agent
        .disconnect()
        .iter()
        .filter_map(|argv| run(agent, argv).err())
        .collect();

    removed(agent, failures, agent.installed()).map(|()| Removal::Changed)
}

/// Whether removing from `agent` worked, from the steps that failed and
/// whether the agent still lists the plugin afterwards (`installed`). A
/// failed step on a plugin that is gone was a part already absent; when the
/// list cannot be read, a failed step is reported.
fn removed(
    agent: Agent,
    failures: Vec<Error>,
    installed: std::result::Result<Option<bool>, String>,
) -> Result<()> {
    let first = failures.into_iter().next();

    match installed {
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

    if let Some(text) = read_instructions(&claude)?
        && !same_place(&claude, &agents)
        && !text.contains("@AGENTS.md")
    {
        plans.push((claude, Some(text)));
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

    // Kept so Atlas's Settings can list the project and forget it later;
    // the files are what agents read, so a list that can't be saved never
    // undoes them. Tests never write the person's own list.
    #[cfg(not(test))]
    if let Ok(record) = taught_record() {
        let _ = remember(&record, dir, true);
    }

    Ok(json!({"ok": true, "files": files}))
}

/// Take the Atlas section out of a project's AGENTS.md and CLAUDE.md, and
/// forget the project. Nothing else in either file changes; a file left
/// with nothing in it was only ever the section, and goes.
pub fn forget(dir: &Path) -> Result<Value> {
    forget_in(dir, &taught_record()?)
}

/// [`forget`], with the list of taught projects at `record`.
fn forget_in(dir: &Path, record: &Path) -> Result<Value> {
    if !dir.is_dir() {
        return Err(problem(format!("{} is not a folder", dir.display())));
    }

    let agents = dir.join("AGENTS.md");
    let claude = dir.join("CLAUDE.md");
    // A file another name here links to stays, even when only the section
    // was in it, so that link keeps working.
    let aliased = same_place(&claude, &agents);
    let mut changes = Vec::new();
    for path in [agents.clone(), claude.clone()] {
        // CLAUDE.md that is AGENTS.md under another name is changed once.
        if path == claude && same_place(&claude, &agents) {
            continue;
        }
        let Some(before) = read_instructions(&path)? else {
            continue;
        };
        let after = without_section(&before).ok_or_else(|| {
            problem(format!(
                "{} has an Atlas marker without its pair. Remove the lines \
                 {SECTION_START} and {SECTION_END} yourself",
                path.display()
            ))
        })?;
        changes.push((path, before, after));
    }

    let mut files = Vec::new();
    for (path, before, after) in changes {
        // A link stays: its target is cleared as teaching wrote through it.
        let linked = std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_symlink());
        let change = if before == after {
            "absent"
        } else if after.trim().is_empty() && !linked && !aliased {
            std::fs::remove_file(&path).map_err(|error| {
                problem(format!("Could not remove {}: {error}", path.display()))
            })?;
            "removed"
        } else {
            replace_file(&path, after.as_bytes())
                .map_err(|error| problem(format!("Could not write {}: {error}", path.display())))?;
            "removed"
        };
        files.push(json!({"path": path, "change": change}));
    }

    // The files are what agents read: once they're changed, a list that
    // can't be saved is said, never reported as the forgetting failing.
    let mut report = json!({"ok": true, "files": files});
    if let Err(error) = remember(record, dir, false) {
        report["warnings"] = json!([format!(
            "Atlas's list of projects wasn't updated: {}",
            error.error
        )]);
    }
    Ok(report)
}

/// The projects Atlas taught, each with whether its AGENTS.md still has
/// the section.
pub fn taught() -> Vec<Value> {
    let Ok(record) = taught_record() else {
        return Vec::new();
    };
    taught_in(&record)
        .into_iter()
        .map(|dir| {
            let still = has_section(&dir.join("AGENTS.md"));
            json!({"path": dir, "taught": still})
        })
        .collect()
}

/// Whether an instruction file has the Atlas section: read only as a plain
/// file, never waiting on a pipe, and no more of it than an instruction file
/// takes.
fn has_section(path: &Path) -> bool {
    use std::io::Read;

    let Some(file) = crate::helper_info::open_plain(path) else {
        return false;
    };
    let mut text = String::new();
    file.take(INSTRUCTIONS_LIMIT)
        .read_to_string(&mut text)
        .is_ok_and(|_| text.contains(SECTION_START))
}

/// 1 MiB: more than any project's AGENTS.md, so reading one never runs on.
const INSTRUCTIONS_LIMIT: u64 = 1 << 20;

/// Where the taught projects are listed: one JSON list of folders.
fn taught_record() -> Result<PathBuf> {
    Ok(crate::config::home()?.join("Library/Application Support/Atlas/projects.json"))
}

/// The folders `record` lists; none when it is missing or unreadable.
fn taught_in(record: &Path) -> Vec<PathBuf> {
    std::fs::read(record)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<PathBuf>>(&bytes).ok())
        .unwrap_or_default()
}

/// Add `dir` to the list (`keep`) or take it off, by its full path. One
/// change at a time: a lock beside the list is held from reading it to
/// replacing it, so two at once never lose an entry.
fn remember(record: &Path, dir: &Path, keep: bool) -> Result<()> {
    use fs2::FileExt;
    use std::os::unix::fs::OpenOptionsExt;

    let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    if let Some(parent) = record.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| problem(format!("Could not write {}: {error}", parent.display())))?;
    }
    let lock_path = record.with_extension("lock");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&lock_path)
        .map_err(|error| problem(format!("Could not write {}: {error}", lock_path.display())))?;
    lock.lock_exclusive()
        .map_err(|error| problem(format!("Could not lock {}: {error}", lock_path.display())))?;

    let mut listed = taught_in(record);
    listed.retain(|known| known != &dir);
    if keep {
        listed.push(dir);
    }
    replace_file(record, &serde_json::to_vec_pretty(&listed)?)
        .map_err(|error| problem(format!("Could not write {}: {error}", record.display())))
}

/// `text` without the Atlas section and the blank line put before it;
/// unchanged when there is none, and None when the markers are not exactly
/// one pair in order.
fn without_section(text: &str) -> Option<String> {
    match (
        text.matches(SECTION_START).count(),
        text.matches(SECTION_END).count(),
    ) {
        (0, 0) => Some(text.to_owned()),
        (1, 1) => {
            let start = text.find(SECTION_START)?;
            let end = text.find(SECTION_END).filter(|&end| end > start)?;
            // Only what adding put there goes: the line ending after the
            // section, and the blank line before it.
            let mut before = &text[..start];
            if before.ends_with("\n\n") {
                before = &before[..before.len() - 1];
            }
            let rest = &text[end + SECTION_END.len()..];
            let after = rest.strip_prefix('\n').unwrap_or(rest);
            Some(format!("{before}{after}"))
        }
        _ => None,
    }
}

/// An instruction file's text, or None when there is none.
fn read_instructions(path: &Path) -> Result<Option<String>> {
    let target = file_target(path)
        .map_err(|error| problem(format!("Could not read {}: {error}", path.display())))?;
    match std::fs::read_to_string(target) {
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

    let target = file_target(path)?;
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

/// Resolve an existing file, refusing links whose targets cannot be resolved.
fn file_target(path: &Path) -> std::io::Result<PathBuf> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => path.canonicalize().map_err(|error| {
            std::io::Error::new(
                error.kind(),
                format!("Could not resolve {}: {error}", path.display()),
            )
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(path.to_path_buf()),
        Err(error) => Err(error),
    }
}

/// After an update, bring each connected agent's copy of the plugin up to
/// date (agents cache plugins when installing). Never fails the update.
pub fn refresh() -> Value {
    let refreshed: Vec<Value> = Agent::ALL
        .into_iter()
        .filter(|agent| agent.connected() == Ok(Some(true)))
        .map(|agent| {
            let failed = agent
                .update()
                .iter()
                .find_map(|argv| run(agent, argv).err());
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
                ask them to open Atlas to finish setup."
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

    if status["inputService"]["held"] == true {
        return format!(
            "Atlas {version}: the person stopped agents in the helper; ask them before using it."
        );
    }

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
    fn an_unresolved_instruction_link_leaves_every_file_unchanged() {
        for (name, other) in [("AGENTS.md", "CLAUDE.md"), ("CLAUDE.md", "AGENTS.md")] {
            for target in ["missing.md", name] {
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join(name);
                let other = dir.path().join(other);
                let before = with_section("# Rules\n").unwrap();
                std::fs::write(&other, &before).unwrap();
                std::os::unix::fs::symlink(target, &path).unwrap();
                let record = dir.path().join("projects.json");

                let error = project(dir.path()).unwrap_err();
                assert!(error.error.contains(&path.display().to_string()));
                assert!(error.error.contains("Could not resolve"));
                assert_eq!(std::fs::read_link(&path).unwrap(), Path::new(target));
                assert_eq!(std::fs::read_to_string(&other).unwrap(), before);

                let error = forget_in(dir.path(), &record).unwrap_err();
                assert!(error.error.contains(&path.display().to_string()));
                assert_eq!(std::fs::read_link(&path).unwrap(), Path::new(target));
                assert_eq!(std::fs::read_to_string(&other).unwrap(), before);
                assert!(!record.exists());
                assert!(!dir.path().join("missing.md").exists());
            }
        }
    }

    #[test]
    fn forgetting_takes_back_only_what_teaching_added() {
        for text in ["A", "A\n", "A\n\n", "# Opie\n\nBuild with make.\n"] {
            let taught = with_section(text).unwrap();
            let forgotten = without_section(&taught).unwrap();
            assert!(
                forgotten == text || forgotten == format!("{}\n", text.trim_end_matches('\n')),
                "{text:?} came back as {forgotten:?}"
            );
        }
        let moved = format!("X\n{SECTION_START}\nbody\n{SECTION_END}\nY");
        assert_eq!(
            without_section(&moved).unwrap(),
            "X\nY",
            "no blank line added"
        );
    }

    #[test]
    fn forgetting_a_folder_that_isnt_there_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("opei");
        let error = forget_in(&missing, &dir.path().join("projects.json")).unwrap_err();
        assert!(error.error.contains("is not a folder"), "{}", error.error);
    }

    #[test]
    fn forgetting_keeps_a_linked_agents_file_and_clears_its_target() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("opie");
        std::fs::create_dir_all(&project).unwrap();
        let shared = dir.path().join("shared.md");
        std::fs::write(&shared, with_section("").unwrap()).unwrap();
        std::os::unix::fs::symlink(&shared, project.join("AGENTS.md")).unwrap();
        std::os::unix::fs::symlink("AGENTS.md", project.join("CLAUDE.md")).unwrap();

        let report = forget_in(&project, &dir.path().join("projects.json")).unwrap();

        assert!(project.join("AGENTS.md").is_symlink(), "the link stays");
        assert!(project.join("CLAUDE.md").is_symlink(), "the alias stays");
        assert_eq!(std::fs::read_to_string(&shared).unwrap(), "");
        assert_eq!(
            report["files"].as_array().unwrap().len(),
            1,
            "the alias once"
        );
    }

    #[test]
    fn teaching_and_forgetting_at_once_lose_no_listing() {
        let dir = tempfile::tempdir().unwrap();
        let record = dir.path().join("projects.json");
        let folders = (0..16)
            .map(|index| {
                let folder = dir.path().join(format!("p{index}"));
                std::fs::create_dir_all(&folder).unwrap();
                folder
            })
            .collect::<Vec<_>>();

        std::thread::scope(|scope| {
            for folder in &folders {
                let record = &record;
                scope.spawn(move || remember(record, folder, true).unwrap());
            }
        });

        assert_eq!(taught_in(&record).len(), folders.len());
    }

    #[test]
    fn an_unresolved_project_list_link_is_refused_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let record = dir.path().join("projects.json");
        let target = Path::new("missing.json");
        std::os::unix::fs::symlink(target, &record).unwrap();

        for keep in [true, false] {
            let error = remember(&record, dir.path(), keep).unwrap_err();
            assert!(error.error.contains(&record.display().to_string()));
            assert!(error.error.contains("Could not resolve"));
            assert_eq!(std::fs::read_link(&record).unwrap(), target);
            assert!(!dir.path().join(target).exists());
        }
    }

    #[test]
    fn forgetting_a_project_takes_out_only_the_section_and_its_listing() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("opie");
        let other = dir.path().join("landing");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        let record = dir.path().join("projects.json");
        std::fs::write(project.join("AGENTS.md"), "# Opie\n\nBuild with make.\n").unwrap();
        std::fs::write(project.join("CLAUDE.md"), "Only ours.").unwrap();
        super::project(&project).unwrap();
        std::fs::write(project.join("CLAUDE.md"), with_section("").unwrap()).unwrap();
        remember(&record, &project, true).unwrap();
        remember(&record, &other, true).unwrap();

        let report = forget_in(&project, &record).unwrap();

        assert_eq!(
            std::fs::read_to_string(project.join("AGENTS.md")).unwrap(),
            "# Opie\n\nBuild with make.\n"
        );
        assert!(!project.join("CLAUDE.md").exists(), "only ever the section");
        assert_eq!(report["files"][0]["change"], "removed");
        assert_eq!(taught_in(&record), [other.canonicalize().unwrap()]);
        assert_eq!(
            forget_in(&project, &record).unwrap()["files"][0]["change"],
            "absent",
            "forgetting twice changes nothing"
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

    #[test]
    fn the_marketplace_source_is_read_from_each_agents_list() {
        let claude = br#"[{"name":"other","source":"github","repo":"o/r"},
            {"name":"atlas","source":"directory","path":"/a/agents","installLocation":"/a/agents"}]"#;
        assert_eq!(
            Agent::Claude.source_in(claude),
            Some(Source::Folder("/a/agents".into()))
        );
        let repository = br#"[{"name":"atlas","source":"github","repo":"someone/atlas"}]"#;
        assert_eq!(
            Agent::Claude.source_in(repository),
            Some(Source::Elsewhere("someone/atlas".into()))
        );
        assert_eq!(Agent::Claude.source_in(b"[]"), Some(Source::Absent));

        let codex = br#"{"marketplaces":[{"name":"atlas","root":"/a/agents",
            "marketplaceSource":{"sourceType":"local","source":"/a/agents"}}]}"#;
        assert_eq!(
            Agent::Codex.source_in(codex),
            Some(Source::Folder("/a/agents".into()))
        );
        let git = br#"{"marketplaces":[{"name":"atlas","root":"/cache",
            "marketplaceSource":{"sourceType":"git","source":"https://example.com/atlas.git"}}]}"#;
        assert_eq!(
            Agent::Codex.source_in(git),
            Some(Source::Elsewhere("https://example.com/atlas.git".into()))
        );
        assert_eq!(
            Agent::Codex.source_in(br#"{"marketplaces":[]}"#),
            Some(Source::Absent)
        );
        assert_eq!(Agent::Codex.source_in(claude), None, "not Codex's shape");
        assert_eq!(Agent::Claude.source_in(b"not json"), None);
    }

    #[test]
    fn only_atlas_own_folders_count_as_its_marketplace() {
        let dir = tempfile::tempdir().unwrap();
        let checkout = dir.path().join("checkout/agents");
        let manifest = checkout.join("atlas/.claude-plugin");
        fs::create_dir_all(&manifest).unwrap();
        fs::write(
            manifest.join("plugin.json"),
            format!(r#"{{"name":"atlas","homepage":"{HOMEPAGE}"}}"#),
        )
        .unwrap();
        assert!(atlas_folder(&checkout), "a checkout's, by its homepage");

        let someone = dir.path().join("someone/agents");
        let manifest = someone.join("atlas/.claude-plugin");
        fs::create_dir_all(&manifest).unwrap();
        fs::write(
            manifest.join("plugin.json"),
            r#"{"name":"atlas","homepage":"https://example.com"}"#,
        )
        .unwrap();
        assert!(!atlas_folder(&someone));
        assert!(!atlas_folder(&dir.path().join("missing")));

        assert!(atlas_folder(Path::new(
            "/Volumes/Old/Atlas.app/Contents/Resources/agents"
        )));
    }

    #[test]
    fn the_manifests_give_the_homepage_that_marks_atlas_own() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("agents");
        assert!(atlas_folder(&root));

        for manifest in [".claude-plugin/plugin.json", ".codex-plugin/plugin.json"] {
            let text = std::fs::read_to_string(root.join("atlas").join(manifest)).unwrap();
            let value: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(value["homepage"], HOMEPAGE, "{manifest}");
        }
    }

    /// Codex refuses hooks/hooks.json for its Claude Code `modules`, so its
    /// manifest names hooks.json, which holds the same hooks without them.
    #[test]
    fn codex_reads_the_same_hooks_without_claude_code_modules() {
        let plugin = Path::new(env!("CARGO_MANIFEST_DIR")).join("agents/atlas");
        let read = |name: &str| -> Value {
            serde_json::from_str(&std::fs::read_to_string(plugin.join(name)).unwrap()).unwrap()
        };

        let mut claude = read("hooks/hooks.json");
        assert!(claude["modules"].is_array());
        claude.as_object_mut().unwrap().remove("modules");
        assert_eq!(read("hooks.json"), claude);
        assert_eq!(read(".codex-plugin/plugin.json")["hooks"], "./hooks.json");
    }

    /// Codex will not add over a marketplace of the same name, so another
    /// of Atlas's own goes first; anyone else's stops the connection.
    #[test]
    fn connecting_replaces_only_a_marketplace_of_atlas_own() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Atlas.app/Contents/Resources/agents");
        fs::create_dir_all(&path).unwrap();
        let old = Source::Folder("/Volumes/Old/Atlas.app/Contents/Resources/agents".into());
        let remove = argv(&["plugin", "marketplace", "remove", "atlas"]);

        for agent in [Agent::Claude, Agent::Codex] {
            for source in [Some(&Source::Absent), Some(&Source::Folder(path.clone()))] {
                assert_eq!(
                    agent.connecting(&path, source).unwrap(),
                    agent.connect(&path),
                    "{agent:?} {source:?}"
                );
            }

            // There, so not judged by the agent's own copy of the plugin.
            fs::create_dir_all(dir.path().join("someone")).unwrap();
            let someone = Source::Folder(dir.path().join("someone"));
            let error = agent.connecting(&path, Some(&someone)).unwrap_err();
            assert!(error.error.contains("another marketplace named atlas"));
            let elsewhere = Source::Elsewhere("someone/atlas".into());
            assert!(agent.connecting(&path, Some(&elsewhere)).is_err());
        }

        let mut codex = vec![remove];
        codex.extend(Agent::Codex.connect(&path));
        assert_eq!(Agent::Codex.connecting(&path, Some(&old)).unwrap(), codex);
        assert_eq!(
            Agent::Claude.connecting(&path, Some(&old)).unwrap(),
            Agent::Claude.connect(&path),
            "Claude Code replaces it itself"
        );
    }

    /// Claude Code replaces a marketplace named atlas without asking, so an
    /// unknown one stops it; Codex refuses to add over another itself.
    #[test]
    fn claude_code_is_not_connected_over_an_unknown_marketplace() {
        let path = Path::new("/Applications/Atlas.app/Contents/Resources/agents");
        let error = Agent::Claude.connecting(path, None).unwrap_err();
        assert_eq!(error.error, "Claude Code's marketplaces could not be read");
        assert_eq!(
            Agent::Claude
                .connecting(path, Some(&Source::Absent))
                .unwrap(),
            Agent::Claude.connect(path),
            "none at all is no reason to stop"
        );
        assert_eq!(
            Agent::Codex.connecting(path, None).unwrap(),
            Agent::Codex.connect(path)
        );
    }

    /// Removing leaves an agent alone unless its marketplace named atlas is
    /// Atlas's own, and says whose it left (these cases run no tool).
    #[test]
    fn removing_never_touches_someone_elses_marketplace() {
        let dir = tempfile::tempdir().unwrap();
        let someone = Some(Source::Folder(dir.path().to_owned()));
        assert_eq!(
            disconnect_agent(Agent::Claude, Ok(someone)).unwrap(),
            Removal::Kept(dir.path().display().to_string())
        );
        let elsewhere = Some(Source::Elsewhere("someone/atlas".into()));
        assert_eq!(
            disconnect_agent(Agent::Codex, Ok(elsewhere)).unwrap(),
            Removal::Kept("someone/atlas".into())
        );
        assert_eq!(
            disconnect_agent(Agent::Codex, Ok(Some(Source::Absent))).unwrap(),
            Removal::Nothing
        );
        assert!(disconnect_agent(Agent::Claude, Ok(None)).is_err());
        assert!(disconnect_agent(Agent::Claude, Err("silent".into())).is_err());
    }

    /// Codex records where each marketplace comes from in its settings,
    /// which stay readable while any marketplace folder is missing.
    #[test]
    fn codex_settings_say_where_its_atlas_marketplace_comes_from() {
        let local = "model = \"x\"\n\n[marketplaces.atlas]\nsource_type = \"local\"\n\
                     source = \"/a/agents\"\n\n[marketplaces.other]\nsource_type = \"local\"\n\
                     source = \"/gone\"\n";
        assert_eq!(
            codex_recorded_in(local),
            Some(Source::Folder("/a/agents".into()))
        );
        let git =
            "[marketplaces.atlas]\nsource_type = \"git\"\nsource = \"https://example.com/a.git\"\n";
        assert_eq!(
            codex_recorded_in(git),
            Some(Source::Elsewhere("https://example.com/a.git".into()))
        );
        assert_eq!(
            codex_recorded_in("[marketplaces.other]\nsource = \"/x\"\n"),
            Some(Source::Absent)
        );
        assert_eq!(codex_recorded_in(""), Some(Source::Absent));
        assert_eq!(codex_recorded_in("not = [toml"), None);

        let dir = tempfile::tempdir().unwrap();
        assert_eq!(codex_recorded(dir.path()), Some(Source::Absent), "no file");
        fs::write(dir.path().join("config.toml"), local).unwrap();
        assert_eq!(
            codex_recorded(dir.path()),
            Some(Source::Folder("/a/agents".into()))
        );
    }

    /// A marketplace folder since removed is Atlas's when it was inside an
    /// app, or when the agent's own copy of the plugin is Atlas's.
    #[test]
    fn a_removed_folder_is_atlas_own_by_its_shape_or_the_agents_copy() {
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("Atlas 2.app/Contents/Resources/agents");
        assert!(atlas_folder(&gone), "any app's, once removed");
        assert!(!atlas_folder(&dir.path().join("checkout/agents")));

        // An app that is there counts only when it is Atlas.
        let other = dir.path().join("Other.app");
        fs::create_dir_all(other.join("Contents/Resources/agents")).unwrap();
        fs::write(
            other.join("Contents/Info.plist"),
            "<dict>\n\t<key>CFBundleIdentifier</key>\n\t<string>com.example.other</string>\n</dict>",
        )
        .unwrap();
        assert!(!atlas_folder(&other.join("Contents/Resources/agents")));
        fs::write(
            other.join("Contents/Info.plist"),
            "<dict>\n\t<key>CFBundleIdentifier</key>\n\t<string>com.insanearts.atlas</string>\n</dict>",
        )
        .unwrap();
        assert!(atlas_folder(&other.join("Contents/Resources/agents")));

        // The copy an agent keeps when installing the plugin.
        let copy = dir.path().join("cache/1.0/.codex-plugin");
        fs::create_dir_all(&copy).unwrap();
        assert!(!atlas_plugin(&dir.path().join("cache/1.0")));
        fs::write(
            copy.join("plugin.json"),
            format!(r#"{{"name":"atlas","homepage":"{HOMEPAGE}"}}"#),
        )
        .unwrap();
        assert!(atlas_plugin(&dir.path().join("cache/1.0")));
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
