//! Verify the plugin copy a marketplace host actually loads.
use super::{Agent, Removal, Source, argv, connection_lock, exec, problem, run, same_place};
use crate::error::Result;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

type Payload = BTreeMap<PathBuf, (Vec<u8>, u32)>;

struct Installed {
    path: PathBuf,
    enabled: Option<bool>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Install,
    Refresh,
}

pub(super) fn connect(
    agent: Agent,
    marketplace: &Path,
    source: &std::result::Result<Option<Source>, String>,
    mode: Mode,
) -> Result<Removal> {
    let _lock = connection_lock()?;
    let steps = agent.connecting(marketplace, source.as_ref().ok().and_then(Option::as_ref))?;
    let expected = payload(&marketplace.join(super::NAME))?;
    let same_source =
        matches!(source, Ok(Some(Source::Folder(folder))) if same_place(folder, marketplace));
    // Codex cannot list a marketplace until it has been registered at a readable path.
    let before = if agent == Agent::Codex && !same_source {
        None
    } else {
        installed(agent)?
    };
    if mode == Mode::Refresh && agent == Agent::Claude && before.is_none() {
        return Ok(Removal::Nothing);
    }
    let current = same_source
        && matches_payload(before.as_ref().map(|entry| entry.path.as_path()), &expected)?;
    if current && (mode == Mode::Refresh || enabled(agent, before.as_ref())?) {
        return Ok(Removal::Nothing);
    }

    let user_enabled = if mode == Mode::Refresh && agent == Agent::Claude {
        user_enablement()?
    } else {
        None
    };
    if !current || agent != Agent::Claude {
        // Claude's install and update both keep an existing copy at the same version.
        if agent == Agent::Claude && before.is_some() {
            run(
                agent,
                &argv(&["plugin", "uninstall", super::PLUGIN, "--scope", "user"]),
            )?;
        }
        for step in steps {
            run(agent, &step)?;
        }
        if agent == Agent::Claude && user_enabled == Some(false) {
            run(
                agent,
                &argv(&["plugin", "disable", super::PLUGIN, "--scope", "user"]),
            )?;
        }
    }
    if agent == Agent::Claude && (mode == Mode::Install || user_enabled == Some(true)) {
        if user_enablement()? != Some(true) {
            run(
                agent,
                &argv(&["plugin", "enable", super::PLUGIN, "--scope", "user"]),
            )?;
        }
    } else if agent == Agent::Claude && user_enabled.is_none() {
        let path = agent
            .home()
            .ok_or_else(|| unreadable(agent))?
            .join("settings.json");
        let mut settings: Value = serde_json::from_slice(&fs::read(&path)?)?;
        if let Some(plugins) = settings
            .get_mut("enabledPlugins")
            .and_then(Value::as_object_mut)
        {
            plugins.remove(super::PLUGIN);
        }
        fs::write(path, serde_json::to_vec_pretty(&settings)?)?;
    }

    let source = agent.source().map_err(problem)?;
    let after = installed(agent)?;
    if !matches!(source, Some(Source::Folder(folder)) if same_place(&folder, marketplace))
        || !matches_payload(after.as_ref().map(|entry| entry.path.as_path()), &expected)?
    {
        return Err(problem(format!(
            "{} did not load the bundled Atlas plugin after installing",
            agent.display_name()
        )));
    }
    if mode == Mode::Refresh && agent == Agent::Claude && user_enablement()? != user_enabled {
        return Err(problem(
            "Claude Code did not keep the Atlas plugin's user setting after refreshing",
        ));
    }
    if mode == Mode::Install && !enabled(agent, after.as_ref())? {
        return Err(problem(format!(
            "{} still reports the Atlas plugin disabled after installing; check its configuration overrides",
            agent.display_name()
        )));
    }
    Ok(Removal::Changed)
}

fn enabled(agent: Agent, installed: Option<&Installed>) -> Result<bool> {
    if agent == Agent::Claude {
        return Ok(user_enablement()? == Some(true));
    }
    if let Some(enabled) = installed.and_then(|entry| entry.enabled) {
        return Ok(enabled);
    }
    let home = agent.home().ok_or_else(|| unreadable(agent))?;
    let text = match fs::read_to_string(home.join("config.toml")) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(error.into()),
    };
    let settings: toml::Table = text.parse().map_err(|_| unreadable(agent))?;
    Ok(settings
        .get("plugins")
        .and_then(|plugins| plugins.get(super::PLUGIN))
        .and_then(|plugin| plugin.get("enabled"))
        .and_then(toml::Value::as_bool)
        .unwrap_or(true))
}

fn user_enablement() -> Result<Option<bool>> {
    let home = Agent::Claude
        .home()
        .ok_or_else(|| unreadable(Agent::Claude))?;
    let bytes = match fs::read(home.join("settings.json")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let settings: Value = serde_json::from_slice(&bytes)?;
    settings["enabledPlugins"]
        .get(super::PLUGIN)
        .map(|value| value.as_bool().ok_or_else(|| unreadable(Agent::Claude)))
        .transpose()
}

fn installed(agent: Agent) -> Result<Option<Installed>> {
    let command = match agent {
        Agent::Claude => argv(&["plugin", "list", "--json"]),
        Agent::Codex => argv(&["plugin", "list", "--marketplace", super::NAME, "--json"]),
        _ => return Err(problem("This host does not cache marketplace plugins")),
    };
    let output = exec(agent, &command, super::ASK)?;
    if !output.status.success() {
        return Err(unreadable(agent));
    }
    let list: Value = serde_json::from_slice(&output.stdout).map_err(|_| unreadable(agent))?;
    installed_in(agent, &list)
}

fn unreadable(agent: Agent) -> crate::error::Error {
    problem(format!(
        "{}'s installed plugins could not be read",
        agent.display_name()
    ))
}

fn installed_in(agent: Agent, list: &Value) -> Result<Option<Installed>> {
    let entries = match agent {
        Agent::Claude => list.as_array(),
        Agent::Codex => list["installed"].as_array(),
        _ => None,
    }
    .ok_or_else(|| unreadable(agent))?;
    let Some(entry) = entries.iter().find(|entry| {
        let id = entry["id"].as_str().or(entry["pluginId"].as_str());
        id == Some(super::PLUGIN) && (agent != Agent::Claude || entry["scope"] == "user")
    }) else {
        return Ok(None);
    };

    let path = if agent == Agent::Claude {
        entry["readFromFolder"]
            .as_str()
            .or_else(|| entry["installPath"].as_str())
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .ok_or_else(|| unreadable(agent))?
    } else {
        let version = entry["version"]
            .as_str()
            .filter(|version| {
                !version.is_empty() && *version != "." && *version != ".." && !version.contains('/')
            })
            .ok_or_else(|| unreadable(agent))?;
        let home = agent.home().ok_or_else(|| unreadable(agent))?;
        home.join("plugins/cache")
            .join(super::NAME)
            .join(super::NAME)
            .join(version)
    };
    Ok(Some(Installed {
        path,
        enabled: entry["enabled"].as_bool(),
    }))
}

fn matches_payload(installed: Option<&Path>, expected: &Payload) -> Result<bool> {
    let Some(installed) = installed else {
        return Ok(false);
    };
    match fs::symlink_metadata(installed) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
        Ok(_) => Ok(payload(installed)? == *expected),
    }
}

/// Compare contents and executable bits, including removed files and empty folders.
fn payload(root: &Path) -> Result<Payload> {
    let mut entries = BTreeMap::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        if entries.len() >= 1024 {
            return Err(problem("The Atlas plugin contains too many files"));
        }
        let path = root.join(&relative);
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            entries.insert(relative.clone(), (Vec::new(), 0));
            for entry in fs::read_dir(&path)? {
                let entry = entry?;
                if entry.file_name() != "tests" {
                    pending.push(relative.join(entry.file_name()));
                }
            }
        } else if metadata.is_file() && metadata.len() <= 2 << 20 {
            entries.insert(
                relative,
                (
                    Sha256::digest(fs::read(&path)?).to_vec(),
                    metadata.permissions().mode() & 0o111,
                ),
            );
        } else {
            return Err(problem(format!(
                "Unsupported Atlas plugin file: {}",
                path.display()
            )));
        }
    }
    Ok(entries)
}

#[cfg(test)]
mod tests;
