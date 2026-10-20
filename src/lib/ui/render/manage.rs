//! Views of looking after Atlas itself: agents, switching, updates,
//! removal and the menu bar's login item.
use super::{Rendered, card, style, text};
use crate::ui::table;
use serde_json::Value;

fn agent_name(id: &str) -> &str {
    match crate::agents::Agent::named(id) {
        Some(agent) => agent.display_name(),
        None => id,
    }
}

pub fn agents(value: &Value) -> String {
    let agents: Vec<&Value> = value["agents"].as_array().into_iter().flatten().collect();
    let rows: Vec<Vec<String>> = agents
        .iter()
        .map(|agent| {
            let state = if agent["installed"] != true {
                format!("{} not installed", style::unknown())
            } else if agent["error"].is_string() {
                format!("{} not answering", style::warn())
            } else if agent["connected"] == true {
                format!("{} connected", style::ok())
            } else {
                format!("{} not connected", style::unknown())
            };

            let version = agent["version"]
                .as_str()
                .and_then(|version| version.split_whitespace().find(|word| word.contains('.')))
                .unwrap_or("");

            vec![
                agent_name(agent["agent"].as_str().unwrap_or("?")).to_owned(),
                version.to_owned(),
                state,
            ]
        })
        .collect();

    let mut lines = vec![table::render(&["AGENT", "VERSION", "ATLAS"], &rows)];

    if agents.iter().any(|agent| {
        agent["installed"] == true && agent["connected"] != true && agent["error"].is_null()
    }) {
        lines.push(format!(
            "\nTo connect: {}",
            style::cmd("atlas agents install")
        ));
    }

    lines.join("\n")
}

/// Which instruction files now carry the Atlas section.
pub fn agents_project(value: &Value) -> String {
    let mut lines = Vec::new();
    let mut changed = false;

    for file in value["files"].as_array().into_iter().flatten() {
        let path = file["path"].as_str().unwrap_or("?");
        let name = std::path::Path::new(path)
            .file_name()
            .map_or(path.into(), |name| name.to_string_lossy());

        let line = match file["change"].as_str() {
            Some("added") => format!("Added Atlas to {name}"),
            Some("updated") => format!("Updated Atlas in {name}"),
            _ => format!("{name} already has Atlas"),
        };

        changed |= file["change"] != "unchanged";
        lines.push(format!("{} {}", style::ok(), text::printable(&line)));
    }

    if changed {
        lines.push(style::dim(
            "Agents working here will now test changes in the helper.",
        ));
    }

    lines.join("\n")
}

pub fn agents_change(value: &Value) -> Rendered {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut connected = false;

    for agent in value["agents"].as_array().into_iter().flatten() {
        let name = agent_name(agent["agent"].as_str().unwrap_or("?"));

        if agent["ok"] == true {
            connected |= agent["connected"] == true;
            let state = if agent["connected"] == true {
                "connected"
            } else {
                "disconnected"
            };
            out.push(format!("{} {name} {state}", style::ok()));
        } else {
            err.push(card(
                &format!("{name}: {}", agent["error"].as_str().unwrap_or("failed")),
                Some("agents"),
                false,
            ));
        }
    }

    if connected {
        out.push(style::dim("New sessions know how to use Atlas."));
        out.push(format!(
            "In each project, {} tells its agents to test with it.",
            style::cmd("atlas agents project")
        ));
    }

    Rendered {
        out: out.join("\n"),
        err: err.join("\n"),
    }
}

pub fn switch(value: &Value) -> String {
    if let Some(accounts) = value["accounts"].as_array() {
        if accounts.is_empty() {
            return "No other account to switch to.".to_owned();
        }

        let rows: Vec<Vec<String>> = accounts
            .iter()
            .map(|account| {
                vec![
                    text::printable(account["realName"].as_str().unwrap_or("")).into_owned(),
                    text::printable(account["name"].as_str().unwrap_or("?")).into_owned(),
                    if account["helper"] == true {
                        "helper"
                    } else {
                        ""
                    }
                    .to_owned(),
                ]
            })
            .collect();

        return table::render(&["NAME", "ACCOUNT", ""], &rows);
    }

    match value["choose"].as_str() {
        Some(name) => format!(
            "{} Screen locked. Click Switch User and choose {}.",
            style::ok(),
            text::printable(name)
        ),
        None => format!("{} Screen locked.", style::ok()),
    }
}

/// `Atlas-0.3.0-macos.zip` as `0.3.0`.
fn version(name: &str) -> &str {
    name.strip_prefix("Atlas-")
        .and_then(|rest| rest.strip_suffix("-macos.zip"))
        .unwrap_or(name)
}

pub fn update(value: &Value) -> Rendered {
    if value["updated"] == false {
        let installed = value["version"].as_str().unwrap_or("");
        let available = version(value["available"].as_str().unwrap_or(installed));
        let out = if available.is_empty() || available == installed {
            format!("{} Atlas {installed} is up to date", style::ok())
        } else {
            format!(
                "{} Atlas {installed} is installed; {available} is not newer. --force installs it anyway.",
                style::ok()
            )
        };

        return Rendered {
            out,
            err: String::new(),
        };
    }

    let helper = &value["helper"];

    if value["ok"] != true {
        let step = match helper["failedStep"].as_str() {
            Some("install") => "installing the shared command",
            Some("restart") => "restarting the helper",
            Some("start-input") => "starting input",
            Some("setup --apply") => "checking the setup",
            _ => "moving the helper onto it",
        };

        let reason = helper["failure"]["error"]
            .as_str()
            .or_else(|| helper["error"].as_str())
            .or_else(|| value["error"].as_str())
            .unwrap_or("it did not finish");

        return Rendered {
            out: String::new(),
            err: format!(
                "{}\n  {} Fix that, then run atlas update again to finish.",
                card(
                    &format!("The app was updated, but {step} failed: {reason}"),
                    None,
                    false,
                ),
                style::dim("→")
            ),
        };
    }

    let to = version(value["to"].as_str().unwrap_or(""));

    // Nothing was set up yet, so there was no helper to move.
    if helper["next"] == "atlas init" {
        return Rendered {
            out: format!(
                "{} Updated Atlas to {to}\n  Run {} to set up the helper.",
                style::ok(),
                style::cmd("atlas init")
            ),
            err: String::new(),
        };
    }

    let mut lines = vec![if value["resumed"] == true {
        format!("{} Finished the earlier update", style::ok())
    } else {
        format!("{} Updated Atlas to {to}", style::ok())
    }];

    let refreshed = helper["agents"]["refreshed"]
        .as_array()
        .map(Vec::len)
        .unwrap_or(0);
    let mut facts = vec!["the helper runs it now".to_owned()];

    if refreshed > 0 {
        facts.push("agent plugins refreshed".to_owned());
    }

    lines.push(format!("  {}", style::dim(facts.join(" · "))));

    Rendered {
        out: lines.join("\n"),
        err: String::new(),
    }
}

pub fn uninstall(value: &Value) -> String {
    let mut lines = vec![format!("{} Atlas is removed", style::ok())];

    if let (Some(name), Some(command)) = (
        value["kept"]["helperAccount"].as_str(),
        value["kept"]["remove"].as_str(),
    ) {
        lines.push(format!(
            "  The helper account {} is still here. To delete it too: {}",
            text::printable(name),
            style::cmd(command)
        ));
    }

    lines.join("\n")
}

pub fn login(value: &Value) -> String {
    if value["available"] == false {
        return format!(
            "{} Open at Login needs Atlas.app in /Applications",
            style::unknown()
        );
    }

    if value["enabled"] == true {
        format!("{} The menu bar opens at login", style::ok())
    } else {
        format!("{} The menu bar does not open at login", style::unknown())
    }
}
