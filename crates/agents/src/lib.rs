//! `atlas agents`: connect coding agents to Atlas. The
//! plugin ships inside Atlas.app, matching this CLI. Claude Code and Codex
//! use a local marketplace; Pi references the package, and opencode loads
//! a symlinked plugin. Each gets the Atlas skill, MCP and a session brief.
use atlas_client::{
    client::Client,
    host_session::{Asker, Recovery, brief},
};
use atlas_core::{
    config::Config,
    desktop::Kind,
    error::{Code, Error, Result},
    host::{Host, HostSession},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub mod claude_app;
mod cursor;
pub mod guide;
mod opencode;
mod pi;
mod process;
mod project;

pub use cursor::hook;
use process::{exec, exec_in, run};
pub use process::{stop_running, stopping};
use project::taught;
pub use project::{forget, project};

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
    Error::new(Code::Agents, message)
}

/// Catalog membership and settings discovery are one declaration; each
/// agent's name and display name are its [`Host`]'s.
macro_rules! agents {
    ($($agent:ident => $restart:expr),+ $(,)?) => {
        /// The agents Atlas can connect, and their command-line tools.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Agent { $($agent),+ }

        impl Agent {
            /// Every agent, in the order they are listed.
            pub const ALL: [Agent; [$(Agent::$agent),+].len()] = [$(Agent::$agent),+];

            pub fn metadata(self) -> Metadata {
                match self { $(Self::$agent => Metadata {
                    name: Host::$agent.as_str(),
                    display: Host::$agent.label(),
                    restart: $restart,
                }),+ }
            }
        }
    };
}

agents! {
    Claude => None,
    Codex => None,
    Opencode => Some("Restart opencode for this change to take effect."),
    Pi => None,
    Cursor => Some("Reload the Cursor window or start a new Cursor Agent session for this change to take effect."),
}

impl Agent {
    pub(crate) fn name(self) -> &'static str {
        self.metadata().name
    }

    /// The name a person knows it by.
    pub fn display_name(self) -> &'static str {
        self.metadata().display
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
            Agent::Pi | Agent::Opencode | Agent::Cursor => vec![],
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
            Some(Source::Folder(folder)) if atlas_folder(folder) => match self {
                Agent::Codex => steps.push(argv(&["plugin", "marketplace", "remove", NAME])),
                Agent::Claude | Agent::Pi | Agent::Opencode | Agent::Cursor => {}
            },
            Some(Source::Folder(folder)) => {
                return Err(self.another(&folder.display().to_string()));
            }
            Some(Source::Elsewhere(place)) => return Err(self.another(place)),
            Some(Source::Absent) => {}
            None => match self {
                Agent::Claude => return Err(self.unreadable()),
                Agent::Codex | Agent::Pi | Agent::Opencode | Agent::Cursor => {}
            },
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
            Agent::Pi | Agent::Opencode | Agent::Cursor => vec![],
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
            Agent::Pi | Agent::Opencode | Agent::Cursor => vec![],
        }
    }

    /// Whether the tool is here: its version, or why it could not be asked.
    fn presence(self) -> Presence {
        match self {
            Self::Cursor => cursor::presence(),
            Self::Claude | Self::Codex | Self::Pi | Self::Opencode => process::presence(self),
        }
    }

    /// Cursor removal operates on owned files even when its executable is
    /// absent, broken or hangs. Never probe the host before that cleanup.
    fn presence_for_change(self, remove: bool) -> Presence {
        match (self, remove) {
            (Self::Cursor, true) => Presence::Installed("Cursor registration".into()),
            (Self::Claude | Self::Codex | Self::Pi | Self::Opencode, _) | (Self::Cursor, false) => {
                self.presence()
            }
        }
    }

    /// Whether a removal of every agent includes this one: Cursor's only
    /// when Atlas recorded registering it.
    fn removable(self) -> bool {
        match self {
            Self::Cursor => cursor::recorded(),
            Self::Claude | Self::Codex | Self::Pi | Self::Opencode => true,
        }
    }

    /// Whether this agent uses the Atlas plugin from this Atlas's own
    /// marketplace (unknown when its lists cannot be read); an error when it
    /// does not answer.
    fn connected(self) -> Result<Option<bool>> {
        match self {
            Self::Cursor => return cursor::connected().map(Some),
            Self::Pi => {}
            Self::Claude | Self::Codex | Self::Opencode => match self.installed()? {
                Some(true) => {}
                other => return Ok(other),
            },
        }

        Ok(self.source()?.map(|source| match source {
            Source::Folder(folder) => this_atlas(&folder),
            Source::Absent | Source::Elsewhere(_) => false,
        }))
    }

    /// Whether the Atlas plugin is installed in this agent, from whichever
    /// marketplace (unknown when its list cannot be read); an error when it
    /// does not answer.
    fn installed(self) -> Result<Option<bool>> {
        // Codex's whole list asks its remote marketplaces too, which can
        // take longer than ASK; Atlas's own marketplace answers at once.
        let list: &[&str] = match self {
            Self::Claude => &["plugin", "list", "--json"],
            Self::Codex => &["plugin", "list", "--marketplace", NAME, "--json"],
            Self::Opencode => return opencode::installed().map(Some),
            Self::Pi => return Ok(self.source()?.map(|source| source != Source::Absent)),
            Self::Cursor => return Ok(None),
        };

        let output = match exec(self, list, ASK) {
            Ok(output) => output,
            Err(error) if error.code == Code::Agents => return Err(error),
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
            Agent::Pi | Agent::Opencode | Agent::Cursor => return None,
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
    fn source(self) -> Result<Option<Source>> {
        let recorded = match self {
            Self::Opencode => return opencode::source().map(Some),
            Self::Pi => return pi::source().map(Some),
            Self::Codex => codex_home().and_then(|home| codex_recorded(&home)),
            Self::Claude | Self::Cursor => None,
        };

        if let Some(found @ (Source::Folder(_) | Source::Elsewhere(_))) = &recorded {
            return Ok(Some(found.clone()));
        }

        match exec(self, &["plugin", "marketplace", "list", "--json"], ASK) {
            Ok(output) => Ok(self.source_in(&output.stdout).or(recorded)),
            Err(error) if error.code == Code::Agents => Err(error),
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
            Agent::Pi | Agent::Opencode | Agent::Cursor => return None,
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
            Agent::Pi | Agent::Opencode | Agent::Cursor => return None,
        };

        Some(match (kind, place) {
            (Some("directory" | "local"), Some(folder)) => Source::Folder(PathBuf::from(folder)),
            (_, place) => Source::Elsewhere(place.unwrap_or("somewhere else").to_owned()),
        })
    }
}

pub struct Metadata {
    name: &'static str,
    display: &'static str,
    pub restart: Option<&'static str>,
}

/// The folder Codex keeps its settings in, as its tool finds it with
/// Atlas's environment.
fn codex_home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".codex")))
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
pub fn same_place(a: &Path, b: &Path) -> bool {
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

/// Whether `folder` is a marketplace of Atlas's own: this Atlas's, or one
/// whose plugin manifest gives Atlas's homepage, such as another Atlas.app's
/// or a checkout's.
fn atlas_folder(folder: &Path) -> bool {
    marketplace().is_ok_and(|ours| same_place(folder, &ours)) || atlas_plugin(&folder.join(NAME))
}

/// Whether `plugin` is Atlas's plugin: a manifest of it gives Atlas's homepage.
fn atlas_plugin(plugin: &Path) -> bool {
    [".claude-plugin/plugin.json", ".codex-plugin/plugin.json"]
        .iter()
        .filter_map(|manifest| fs::read(plugin.join(manifest)).ok())
        .filter_map(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .any(|manifest| manifest["name"] == NAME && manifest["homepage"] == HOMEPAGE)
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
fn connection_lock() -> Result<atlas_core::files::LockFile> {
    connection_lock_in(&std::env::temp_dir())
}

/// The connection lock in `directory`, also used by the Claude app's writer.
pub(crate) fn connection_lock_in(directory: &Path) -> Result<atlas_core::files::LockFile> {
    use fs2::FileExt;
    let path = directory.join(format!("atlas-agents-{}.lock", nix::unistd::Uid::current()));
    let file = atlas_core::files::open_lock_file(&path, atlas_core::files::LockOpen::WriteOnly)
        .map_err(|error| problem(format!("Could not open the agent connection lock: {error}")))?;
    file.try_lock_exclusive()
        .map_err(|error| problem(format!("Another agent connection is running: {error}")))?;
    Ok(atlas_core::files::LockFile(file))
}

/// Whether an agent's tool is on this Mac.
enum Presence {
    Installed(String),
    /// Installed, but it did not answer in time.
    Silent(String),
    Missing,
}

/// What was found of one agent, kept typed for what decides on it.
struct Probe {
    presence: Presence,
    /// Whether Atlas is connected, or why that could not be told; none when
    /// it was not asked.
    connection: Option<Result<Option<bool>>>,
    /// What the connection's word rests on, when it is not the agent's own.
    verification: Option<&'static str>,
}

impl Probe {
    /// `agent`'s row for `status`.
    fn row(&self, agent: Agent) -> Value {
        let mut row = self.presence.row(agent);
        match &self.connection {
            Some(Ok(connected)) => row["connected"] = json!(connected),
            Some(Err(error)) => row["error"] = json!(error.error),
            None => {}
        }
        if let Some(verification) = self.verification {
            row["verification"] = json!(verification);
        }
        row
    }

    /// Whether the agent is here and known not to be connected.
    fn unconnected(&self) -> bool {
        self.presence.here() && matches!(self.connection, Some(Ok(Some(false))))
    }
}

impl Presence {
    fn here(&self) -> bool {
        match self {
            Self::Installed(_) | Self::Silent(_) => true,
            Self::Missing => false,
        }
    }

    /// `agent`'s row for `status`, as far as its tool says.
    fn row(&self, agent: Agent) -> Value {
        let name = agent.name();
        match self {
            Self::Installed(version) => {
                json!({"agent": name, "installed": true, "version": version})
            }
            Self::Silent(error) => json!({"agent": name, "installed": true, "error": error}),
            Self::Missing => json!({"agent": name, "installed": false}),
        }
    }
}

/// The plugin marketplace shipped with this CLI: inside Atlas.app, or
/// in a development checkout its `agents/` folder.
pub(crate) fn marketplace() -> Result<PathBuf> {
    if let Ok(app) = atlas_core::app_bundle::installed_app() {
        let bundled = app.join("Contents/Resources/agents");
        if bundled.is_dir() {
            return Ok(bundled);
        }
    }

    // Release builds ship only inside Atlas.app; the build machine's checkout
    // means nothing on another Mac.
    #[cfg(debug_assertions)]
    {
        let checkout = atlas_core::app_bundle::checkout().join("agents");
        if checkout.is_dir() {
            return Ok(checkout);
        }
    }

    Err(problem("This Atlas carries no agent plugin; reinstall it"))
}

/// Each agent on this Mac and whether Atlas is connected to it.
pub fn status() -> Value {
    let mut agents: Vec<Value> = probes()
        .into_iter()
        .map(|probed| probed.map_or(Value::Null, |(agent, probe)| probe.row(agent)))
        .collect();

    agents.push(crate::claude_app::status());
    json!({"ok": true, "agents": agents, "plugin": marketplace().ok(), "projects": taught()})
}

/// What could still be connected here.
pub struct Unconnected {
    pub agents: Vec<Agent>,
    /// The Claude app.
    pub app: bool,
}

/// The agents here, and the Claude app, known not to be connected.
pub fn unconnected() -> Unconnected {
    Unconnected {
        agents: probes()
            .into_iter()
            .flatten()
            .filter(|(_, probe)| probe.unconnected())
            .map(|(agent, _)| agent)
            .collect(),
        app: crate::claude_app::unconnected(),
    }
}

/// Every agent looked at side by side; none for a look that panicked.
fn probes() -> Vec<Option<(Agent, Probe)>> {
    std::thread::scope(|scope| {
        Agent::ALL
            .map(|agent| scope.spawn(move || (agent, probe(agent))))
            .into_iter()
            .map(|probe| probe.join().ok())
            .collect()
    })
}

/// Whether `agent`'s tool is here, and whether Atlas is connected to it.
fn probe(agent: Agent) -> Probe {
    match agent {
        Agent::Cursor => cursor::probe(),
        Agent::Claude | Agent::Codex | Agent::Pi | Agent::Opencode => {
            let presence = agent.presence();
            let connection = match presence {
                Presence::Installed(_) => Some(agent.connected()),
                Presence::Silent(_) | Presence::Missing => None,
            };
            Probe {
                presence,
                connection,
                verification: None,
            }
        }
    }
}

/// Whether a change connects Atlas or disconnects it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Connect,
    Disconnect,
}

/// What a change covers beside the agents it names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Covering {
    /// Every installed agent, whichever are named.
    pub every_agent: bool,
    /// The Claude app.
    pub app: bool,
}

/// Make `change` for the chosen agents, or every installed one. Only a
/// marketplace named atlas that is Atlas's own is replaced or removed; an
/// agent whose is someone else's is refused when connecting and left alone
/// when disconnecting.
///
/// # Errors
///
/// Returns host-discovery, configuration or installation failures. Earlier hosts may
/// already have completed their requested change.
pub fn change(chosen: &[Agent], change: Change) -> Result<Value> {
    // The Claude app comes along when no agent was named: it has no
    // command line of its own to choose it by.
    let covering = Covering {
        every_agent: chosen.is_empty(),
        app: chosen.is_empty() && crate::claude_app::installed(),
    };
    change_these(chosen, covering, change).map(|changed| changed.report)
}

/// What a change did: whether all of it worked, and its report.
pub struct Changed {
    pub ok: bool,
    pub report: Value,
}

/// [`change`] for exactly `chosen` and what `covering` adds.
///
/// # Errors
///
/// Returns a selected host or app integration failure. Changes completed for earlier
/// selections are not rolled back automatically.
pub fn change_these(chosen: &[Agent], covering: Covering, change: Change) -> Result<Changed> {
    let remove = match change {
        Change::Connect => false,
        Change::Disconnect => true,
    };
    let Covering { every_agent, app } = covering;
    let marketplace = if remove { None } else { Some(marketplace()?) };

    let present: Vec<(Agent, Presence)> = if every_agent {
        Agent::ALL
            .into_iter()
            .map(|agent| (agent, agent.presence_for_change(remove)))
            .filter(|(agent, presence)| presence.here() && (!remove || agent.removable()))
            .collect()
    } else {
        chosen
            .iter()
            .map(|agent| (*agent, agent.presence_for_change(remove)))
            .collect()
    };

    if present.is_empty() && !app {
        return Err(problem(format!(
            "None of {} or the Claude app is installed",
            Agent::ALL.map(Agent::display_name).join(", ")
        )));
    }

    let mut results: Vec<Changed> = present
        .into_iter()
        .map(|(agent, presence)| change_agent(agent, &presence, marketplace.as_deref()))
        .collect();

    if app {
        results.push(crate::claude_app::change(!remove));
    }

    let ok = results.iter().all(|result| result.ok);
    let agents: Vec<Value> = results.into_iter().map(|result| result.report).collect();
    Ok(Changed {
        ok,
        report: json!({"ok": ok, "agents": agents}),
    })
}

/// `agent`'s part of a change: connected to `marketplace`, or with none
/// disconnected.
fn change_agent(agent: Agent, presence: &Presence, marketplace: Option<&Path>) -> Changed {
    let name = agent.name();
    let refused = |error: &str| Changed {
        ok: false,
        report: json!({"agent": name, "ok": false, "error": error}),
    };
    match presence {
        Presence::Installed(_) => {}
        Presence::Silent(error) => return refused(error),
        Presence::Missing => return refused("not installed"),
    }

    let outcome = match agent {
        Agent::Cursor => cursor::change(marketplace),
        Agent::Pi => pi::change(marketplace),
        Agent::Opencode => opencode::change(marketplace),
        Agent::Claude | Agent::Codex => {
            // A marketplace owned by someone else is never changed.
            let source = agent.source();
            match marketplace {
                Some(path) => connect_agent(agent, path, source),
                None => disconnect_agent(agent, source),
            }
        }
    };

    let removal = match outcome {
        Ok(removal) => removal,
        Err(error) => return refused(&error.error),
    };
    let mut report = json!({
        "agent": name,
        "ok": true,
        "connected": marketplace.is_some(),
        "changed": removal == Removal::Changed,
    });

    if agent.metadata().restart.is_some() && removal == Removal::Changed {
        report["restart"] = json!(true);
    }

    if let Removal::Kept(place) = removal {
        report["kept"] = json!(place);
    }

    Changed { ok: true, report }
}

/// Connect `agent` to `marketplace`, given where its marketplace named
/// atlas comes from (`source`, unknown when unreadable): the steps stop at
/// a failure.
fn connect_agent(
    agent: Agent,
    marketplace: &Path,
    source: Result<Option<Source>>,
) -> Result<Removal> {
    let steps = agent
        .connecting(marketplace, source.as_ref().ok().and_then(Option::as_ref))
        .map_err(|refused| match source {
            Err(cause) => refused.with_source(cause),
            Ok(_) => refused,
        })?;

    for argv in steps {
        run(agent, &argv)?;
    }
    Ok(Removal::Changed)
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
fn disconnect_agent(agent: Agent, source: Result<Option<Source>>) -> Result<Removal> {
    match source? {
        Some(Source::Folder(folder)) if atlas_folder(&folder) => {}
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
fn removed(agent: Agent, failures: Vec<Error>, installed: Result<Option<bool>>) -> Result<()> {
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

/// After an update, bring each connected agent's copy of the plugin up to
/// date (agents cache plugins when installing). Never fails the update.
pub fn refresh() -> Value {
    let refreshed: Vec<Value> = Agent::ALL
        .into_iter()
        .filter(|agent| match agent {
            Agent::Cursor => cursor::recorded(),
            Agent::Claude | Agent::Codex | Agent::Pi | Agent::Opencode => {
                matches!(agent.connected(), Ok(Some(true)))
            }
        })
        .map(|agent| {
            let failed = match agent {
                Agent::Cursor => marketplace()
                    .and_then(|source| cursor::change(Some(&source)))
                    .err(),
                Agent::Claude | Agent::Codex | Agent::Pi | Agent::Opencode => agent
                    .update()
                    .iter()
                    .find_map(|argv| run(agent, argv).err()),
            };
            json!({"agent": agent.name(), "ok": failed.is_none(), "error": failed.map(|e| e.error)})
        })
        .collect();

    json!({"agents": refreshed})
}

/// One line for an agent's session start: local, bounded, never failing.
/// For host `session`, it says what that session left in the helper, and
/// lets it recover an unknown action that is all its own.
pub async fn brief(config_path: &Path, session: Option<&HostSession>) -> String {
    let version = atlas_core::VERSION;

    let Ok(config) = Config::load(config_path) else {
        return "Atlas is installed but not set up. If the person wants GUI testing, \
                ask them to open Atlas to finish setup."
            .to_owned();
    };

    let Ok(client) = Client::new(config.clone()) else {
        return format!("Atlas {version}: run atlas doctor before using it.");
    };

    if let Some(session) = session {
        let _ = atlas_client::host_session::store::touch(&client, session);
    }

    let causes = client.controller.pending_causes().unwrap_or_default();
    if let Some(recovery) = Recovery::of(&causes, Asker::Agent(session.map(HostSession::key))) {
        return format!("Atlas {version}: {}", recovery.step());
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

    let ready = match config.desktop().kind() {
        Kind::User(login) => atlas_client::user::capability::doctor(status, login)["ready"] == true,
    };
    if ready {
        let mut line = format!(
            "Atlas {version}: ready. After any change a person could see, check it in the \
             hidden helper with the atlas skill before saying it works."
        );
        if let Some(left) = session.and_then(|session| brief::leftovers(&client, session)) {
            line.push(' ');
            line.push_str(&left);
        }
        line
    } else {
        format!("Atlas {version}: not ready; run atlas doctor and tell the person what it says.")
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
