//! `understudy agents`: connect Claude Code and Codex to Understudy. The
//! plugin (a skill that teaches the agent, a setup skill, and a session-start
//! status line) ships inside Understudy.app, so it always matches this CLI;
//! connecting registers that folder as a local marketplace in each agent.
use crate::{
    client::Client,
    config::Config,
    error::{Error, Result},
};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

/// The marketplace and plugin name in both agents.
const NAME: &str = "understudy";

fn problem(message: impl Into<String>) -> Error {
    Error::new("agents", message)
}

/// The agents Understudy can connect, and their command-line tools.
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

    /// The argv that connect the plugin at `marketplace`.
    fn connect(self, marketplace: &Path) -> Vec<Vec<String>> {
        let path = marketplace.to_string_lossy().into_owned();
        let plugin = format!("{NAME}@{NAME}");
        let argv = |items: &[&str]| items.iter().map(|item| (*item).to_owned()).collect();
        match self {
            Agent::Claude => vec![
                argv(&["plugin", "marketplace", "add", &path, "--scope", "user"]),
                argv(&["plugin", "install", &plugin, "--scope", "user"]),
            ],
            Agent::Codex => vec![
                argv(&["plugin", "marketplace", "add", &path]),
                argv(&["plugin", "add", &plugin]),
            ],
        }
    }

    /// The argv that disconnect it.
    fn disconnect(self) -> Vec<Vec<String>> {
        let plugin = format!("{NAME}@{NAME}");
        let argv = |items: &[&str]| items.iter().map(|item| (*item).to_owned()).collect();
        match self {
            Agent::Claude => vec![
                argv(&["plugin", "uninstall", &plugin, "--scope", "user"]),
                argv(&["plugin", "marketplace", "remove", NAME]),
            ],
            Agent::Codex => vec![
                argv(&["plugin", "remove", &plugin]),
                argv(&["plugin", "marketplace", "remove", NAME]),
            ],
        }
    }

    /// The tool's version, if it is installed.
    fn version(self) -> Option<String> {
        let output = Command::new(self.name()).arg("--version").output().ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    /// Whether the Understudy plugin is installed in this agent.
    fn connected(self) -> Option<bool> {
        let output = Command::new(self.name())
            .args(["plugin", "list", "--json"])
            .output()
            .ok()?;
        let list: Value = serde_json::from_slice(&output.stdout).ok()?;
        let plugins = match self {
            Agent::Claude => list.as_array().cloned(),
            Agent::Codex => list["installed"].as_array().cloned(),
        }?;
        Some(plugins.iter().any(|plugin| {
            let id = plugin["id"].as_str().or(plugin["pluginId"].as_str());
            id == Some(&format!("{NAME}@{NAME}")) || plugin["name"] == NAME
        }))
    }
}

/// The plugin marketplace shipped with this CLI: inside Understudy.app, or
/// in a development checkout its `agents/` folder.
pub fn marketplace() -> Result<PathBuf> {
    if let Ok(app) = crate::update::installed_app() {
        let bundled = app.join("Contents/Resources/agents");
        if bundled.is_dir() {
            return Ok(bundled);
        }
    }
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("agents");
    checkout
        .is_dir()
        .then_some(checkout)
        .ok_or_else(|| problem("This Understudy carries no agent plugin; reinstall it"))
}

fn run(agent: Agent, argv: &[String]) -> Result<()> {
    let output = Command::new(agent.name()).args(argv).output()?;
    if output.status.success() {
        return Ok(());
    }
    let text = String::from_utf8_lossy(&output.stderr);
    Err(problem(format!(
        "{} {} failed: {}",
        agent.name(),
        argv.join(" "),
        text.lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("no output")
    )))
}

/// Each agent on this Mac and whether Understudy is connected to it.
pub fn status() -> Value {
    let agents: Vec<Value> = Agent::ALL
        .iter()
        .map(|agent| match agent.version() {
            Some(version) => json!({
                "agent": agent.name(),
                "installed": true,
                "version": version,
                "connected": agent.connected(),
            }),
            None => json!({"agent": agent.name(), "installed": false}),
        })
        .collect();
    json!({"ok": true, "agents": agents, "plugin": marketplace().ok()})
}

/// Connect (or `remove` from) the chosen agents, or every installed one.
pub fn change(chosen: &[Agent], remove: bool) -> Result<Value> {
    let marketplace = if remove { None } else { Some(marketplace()?) };
    let chosen: Vec<Agent> = if chosen.is_empty() {
        Agent::ALL
            .into_iter()
            .filter(|agent| agent.version().is_some())
            .collect()
    } else {
        chosen.to_vec()
    };
    if chosen.is_empty() {
        return Err(problem("Neither Claude Code nor Codex is installed"));
    }
    let mut results = Vec::new();
    for agent in chosen {
        if agent.version().is_none() {
            results.push(json!({"agent": agent.name(), "ok": false, "error": "not installed"}));
            continue;
        }
        let steps = match &marketplace {
            Some(path) => agent.connect(path),
            None => agent.disconnect(),
        };
        // Removing tolerates parts already gone; connecting stops at a failure.
        let outcome = steps
            .iter()
            .map(|argv| run(agent, argv))
            .find(|step| step.is_err() && !remove)
            .unwrap_or(Ok(()));
        results.push(match outcome {
            Ok(()) => json!({"agent": agent.name(), "ok": true, "connected": !remove}),
            Err(error) => json!({"agent": agent.name(), "ok": false, "error": error.error}),
        });
    }
    let ok = results.iter().all(|result| result["ok"] == true);
    Ok(json!({"ok": ok, "agents": results}))
}

/// After an update, bring each connected agent's copy of the plugin up to
/// date (agents cache plugins when installing). Never fails the update.
pub fn refresh() -> Value {
    let refreshed: Vec<Value> = Agent::ALL
        .into_iter()
        .filter(|agent| agent.connected() == Some(true))
        .map(|agent| {
            let plugin = format!("{NAME}@{NAME}");
            let steps: Vec<Vec<String>> = match agent {
                Agent::Claude => vec![
                    vec![
                        "plugin".into(),
                        "marketplace".into(),
                        "update".into(),
                        NAME.into(),
                    ],
                    vec!["plugin".into(), "update".into(), plugin],
                ],
                Agent::Codex => vec![vec!["plugin".into(), "add".into(), plugin]],
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
        return "Understudy is installed but not set up. If the person wants GUI testing, \
                offer the understudy setup skill (understudy init)."
            .to_owned();
    };
    let Ok(client) = Client::new(config.clone()) else {
        return format!("Understudy {version}: run understudy doctor before using it.");
    };
    if client
        .controller
        .pending()
        .is_ok_and(|pending| !pending.is_empty())
    {
        return format!(
            "Understudy {version}: an action's outcome is unknown; run understudy restart first."
        );
    }
    let Ok(status) = tokio::time::timeout(Duration::from_millis(1500), client.status()).await
    else {
        return format!(
            "Understudy {version}: the helper is not answering; run understudy doctor."
        );
    };
    let ready = crate::capability::doctor(status, config.helper_uid)["ready"] == true;
    if ready {
        format!("Understudy {version}: ready. Use the understudy skill for GUI work.")
    } else {
        format!(
            "Understudy {version}: not ready; run understudy doctor and tell the person what it says."
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connecting_registers_the_local_marketplace_then_installs() {
        let path = Path::new("/Applications/Understudy.app/Contents/Resources/agents");
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
                vec![
                    "plugin",
                    "install",
                    "understudy@understudy",
                    "--scope",
                    "user"
                ],
            ]
        );
        assert_eq!(
            Agent::Codex.connect(path),
            [
                vec!["plugin", "marketplace", "add", path.to_str().unwrap()],
                vec!["plugin", "add", "understudy@understudy"],
            ]
        );
        assert_eq!(
            Agent::Codex.disconnect()[0],
            ["plugin", "remove", "understudy@understudy"]
        );
        assert_eq!(
            Agent::Claude.disconnect()[1],
            ["plugin", "marketplace", "remove", "understudy"]
        );
    }

    /// The plugin's manifests describe this version, and its skill and
    /// command reference are exactly what this CLI generates.
    #[test]
    fn the_plugin_matches_this_cli() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("agents");
        for manifest in [".claude-plugin/plugin.json", ".codex-plugin/plugin.json"] {
            let text = std::fs::read_to_string(root.join("understudy").join(manifest)).unwrap();
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
        let skill =
            std::fs::read_to_string(root.join("understudy/skills/understudy/SKILL.md")).unwrap();
        assert_eq!(
            skill,
            crate::agent::skill(),
            "regenerate with: understudy agent-guide --skill"
        );
    }

    /// Every error code the CLI can report is explained to the agent.
    #[test]
    fn every_error_code_is_in_the_troubleshooting_reference() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let reference = std::fs::read_to_string(
            root.join("agents/understudy/skills/understudy/references/troubleshooting.md"),
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
