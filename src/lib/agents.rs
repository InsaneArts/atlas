//! `atlas agents`: connect coding agents to Atlas. The
//! plugin ships inside Atlas.app, matching this CLI. Claude Code and Codex
//! use a local marketplace; Pi references the package, and opencode loads
//! a symlinked plugin. Each gets the Atlas skill, MCP and a session brief.
use crate::{
    client::Client,
    config::Config,
    control::Cause,
    error::{Error, Result},
    host_session::{HostSession, brief, owns_every_unknown},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

mod cursor;
mod opencode;
mod pi;
mod process;
mod project;

pub use cursor::hook;
use process::{exec, exec_in, run};
pub use process::{stop_running, stopping};
pub use project::{PROJECT_SECTION, forget, project, taught};

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
    Cursor,
}

impl Agent {
    pub const ALL: [Agent; 5] = [
        Agent::Claude,
        Agent::Codex,
        Agent::Opencode,
        Agent::Pi,
        Agent::Cursor,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
            Agent::Pi => "pi",
            Agent::Opencode => "opencode",
            Agent::Cursor => "cursor",
        }
    }

    /// The name a person knows it by.
    pub fn display_name(self) -> &'static str {
        match self {
            Agent::Claude => "Claude Code",
            Agent::Codex => "Codex",
            Agent::Pi => "Pi",
            Agent::Opencode => "opencode",
            Agent::Cursor => "Cursor",
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
        if matches!(self, Agent::Pi | Agent::Opencode | Agent::Cursor) {
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
            Agent::Cursor => return cursor::home().ok(),
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
        if self == Agent::Cursor {
            return cursor::presence();
        }
        match exec(self, &["--version"], ASK) {
            Ok(output) if output.status.success() => {
                Presence::Installed(String::from_utf8_lossy(&output.stdout).trim().to_owned())
            }
            // Installed, but it did not answer in time.
            Err(error) if error.code == "agents" => Presence::Silent(error.error),
            _ => Presence::Missing,
        }
    }

    /// Cursor removal operates on owned files even when its executable is
    /// absent, broken or hangs. Never probe the host before that cleanup.
    fn presence_for_change(self, remove: bool) -> Presence {
        if self == Agent::Cursor && remove {
            Presence::Installed("Cursor registration".into())
        } else {
            self.presence()
        }
    }

    /// Whether this agent uses the Atlas plugin from this Atlas's own
    /// marketplace (unknown when its lists cannot be read); an error when it
    /// does not answer.
    fn connected(self) -> std::result::Result<Option<bool>, String> {
        if self == Agent::Cursor {
            return cursor::connected().map(Some).map_err(|e| e.error);
        }
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
            Agent::Pi | Agent::Opencode | Agent::Cursor => return Ok(None),
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
            Agent::Pi | Agent::Opencode | Agent::Cursor => None,
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
    connection_lock_in(&std::env::temp_dir())
}

/// The connection lock in `directory`, also used by the Claude app's writer.
pub(crate) fn connection_lock_in(directory: &Path) -> Result<fs::File> {
    use fs2::FileExt;
    use std::os::unix::fs::OpenOptionsExt;
    let path = directory.join(format!("atlas-agents-{}.lock", nix::unistd::Uid::current()));
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
    if agent == Agent::Cursor {
        return cursor::status();
    }
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
            .map(|agent| (agent, agent.presence_for_change(remove)))
            .filter(|(agent, presence)| {
                !matches!(presence, Presence::Missing)
                    && (!remove || *agent != Agent::Cursor || cursor::recorded())
            })
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

    let mut results = Vec::new();

    for (agent, presence) in present {
        match presence {
            Presence::Installed(_) => {}
            Presence::Silent(error) => {
                results.push(json!({"agent": agent.name(), "ok": false, "error": error}));
                continue;
            }
            Presence::Missing if remove && agent == Agent::Cursor => {}
            Presence::Missing => {
                results.push(json!({"agent": agent.name(), "ok": false, "error": "not installed"}));
                continue;
            }
        }

        let outcome = if agent == Agent::Cursor {
            cursor::change(marketplace.as_deref())
        } else if agent == Agent::Pi {
            pi::change(marketplace.as_deref())
        } else if agent == Agent::Opencode {
            opencode::change(marketplace.as_deref())
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

        if matches!(agent, Agent::Opencode | Agent::Cursor)
            && matches!(outcome, Ok(Removal::Changed))
        {
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

/// After an update, bring each connected agent's copy of the plugin up to
/// date (agents cache plugins when installing). Never fails the update.
pub fn refresh() -> Value {
    let refreshed: Vec<Value> = Agent::ALL
        .into_iter()
        .filter(|agent| {
            if *agent == Agent::Cursor {
                cursor::recorded()
            } else {
                agent.connected() == Ok(Some(true))
            }
        })
        .map(|agent| {
            let failed = if agent == Agent::Cursor {
                marketplace()
                    .and_then(|source| cursor::change(Some(&source)))
                    .err()
            } else {
                agent
                    .update()
                    .iter()
                    .find_map(|argv| run(agent, argv).err())
            };
            json!({"agent": agent.name(), "ok": failed.is_none(), "error": failed.map(|e| e.error)})
        })
        .collect();

    json!({"agents": refreshed})
}

/// The session brief while an earlier action's outcome is unknown: it may
/// be another agent's, so the person hears of it before any restart.
const UNKNOWN_OUTCOME: &str = "an earlier action's outcome is unknown. Take a screenshot and tell \
the person before running atlas restart.";

/// The session brief while every action of unknown outcome is this
/// session's own, none still being carried out.
const OWN_UNKNOWN_OUTCOME: &str =
    "an earlier action of this session is unknown. Take a screenshot, then run atlas restart.";

/// What a session start says while actions of unknown outcome, `causes`,
/// are pending: the session may recover them itself only when every one
/// is its own and none is still being carried out.
fn unknown_outcome(causes: &[Cause], session: Option<&HostSession>) -> &'static str {
    if session.is_some_and(|session| owns_every_unknown(causes, session.key())) {
        OWN_UNKNOWN_OUTCOME
    } else {
        UNKNOWN_OUTCOME
    }
}

/// One line for an agent's session start: local, bounded, never failing.
/// For host `session`, it says what that session left in the helper, and
/// lets it recover an unknown action that is all its own.
pub async fn brief(config_path: &Path, session: Option<&HostSession>) -> String {
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
        let causes = client.controller.pending_causes().unwrap_or_default();
        return format!("Atlas {version}: {}", unknown_outcome(&causes, session));
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
mod tests {
    use super::*;

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

        for manifest in [
            ".claude-plugin/plugin.json",
            ".codex-plugin/plugin.json",
            ".cursor-plugin/plugin.json",
        ] {
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

    /// The plugin's manifests describe this version, and its skill and
    /// command reference are exactly what this CLI generates.
    #[test]
    fn the_plugin_matches_this_cli() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("agents");

        for manifest in [
            ".claude-plugin/plugin.json",
            ".codex-plugin/plugin.json",
            ".cursor-plugin/plugin.json",
        ] {
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

    #[test]
    fn the_brief_lets_a_session_recover_its_own_unknown_action() {
        use crate::host_session::Host;
        let own = HostSession::new(Host::Claude, "own").unwrap();
        let other = HostSession::new(Host::Codex, "other").unwrap();
        let cause = |session: &HostSession, running: bool| Cause {
            generation: crate::control::token(),
            running,
            session: Some(session.key().clone()),
        };

        let mine = [cause(&own, false)];
        assert_eq!(unknown_outcome(&mine, Some(&own)), OWN_UNKNOWN_OUTCOME);
        assert_eq!(unknown_outcome(&mine, Some(&other)), UNKNOWN_OUTCOME);
        assert_eq!(unknown_outcome(&mine, None), UNKNOWN_OUTCOME);
        assert_eq!(
            unknown_outcome(&[cause(&own, true)], Some(&own)),
            UNKNOWN_OUTCOME
        );
        let both = [cause(&own, false), cause(&other, false)];
        assert_eq!(unknown_outcome(&both, Some(&own)), UNKNOWN_OUTCOME);
    }

    #[test]
    fn a_session_with_an_unknown_outcome_is_told_to_ask_before_recovering() {
        assert!(UNKNOWN_OUTCOME.contains("Take a screenshot"));
        assert!(UNKNOWN_OUTCOME.contains("tell the person before running atlas restart"));
        assert!(!UNKNOWN_OUTCOME.contains("restart first"));
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
