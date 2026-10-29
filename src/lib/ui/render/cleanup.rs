//! Views of what a task installs and cleans up: an install, a cleanup's
//! preview, the steps a run took, and what was kept on purpose apart from
//! what failed.
use super::{Rendered, failed, style, text};
use serde_json::Value;

/// A cleanup's preview or run: one line per step, then the apps opened by
/// name it leaves to the agent. A run that did not finish says why on
/// standard error, its steps still on standard output.
pub fn cleanup(value: &Value) -> Rendered {
    let steps = value["steps"].as_array().map_or(&[][..], Vec::as_slice);
    let preview = value["preview"] == true;
    let mut lines = Vec::new();

    if steps.is_empty() {
        lines.push(format!("{} Nothing to clean up in the helper", style::ok()));
    } else {
        if preview {
            lines.push(format!(
                "Cleaning up would take these steps ({} does it):",
                style::cmd("atlas cleanup --apply")
            ));
        } else if value["ok"] == true {
            lines.push(format!("{} Cleaned up", style::ok()));
        }
        lines.extend(steps.iter().map(step));
    }

    let left: Vec<String> = value["left"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|left| left["app"].as_str())
        .map(|app| text::printable(app).into_owned())
        .collect();
    if !left.is_empty() {
        lines.push(style::dim(format!(
            "Opened by name, so not quit: {}. Close them with atlas close --app NAME.",
            left.join(", ")
        )));
    }

    let out = lines.join("\n");
    match failed(value) {
        Some(failure) => Rendered {
            out,
            err: failure.err,
        },
        None => Rendered::out(out),
    }
}

/// An app installed into the helper: where it went, and what removes it.
pub fn install_app(value: &Value) -> String {
    let app = text::printable(value["app"].as_str().unwrap_or("the app")).into_owned();
    let mut line = format!("{} Installed {app}", style::ok());
    if let Some(path) = value["path"].as_str() {
        line.push_str(&format!(" at {}", text::printable(path)));
    }
    line.push_str(&format!(
        "\n  {}",
        style::dim("atlas cleanup --apply removes it once the task is done, unless it changed")
    ));
    line
}

/// One step: its number, what it does and to what, and how far it got.
fn step(step: &Value) -> String {
    let n = step["n"].as_u64().unwrap_or(0);
    let what = match step["do"].as_str() {
        Some("quit") => {
            let apps: Vec<String> = step["apps"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|app| {
                    format!(
                        "{} ({})",
                        text::printable(app["app"].as_str().unwrap_or("an app")),
                        app["pid"]
                    )
                })
                .collect();
            if apps.is_empty() {
                "Quit what runs from this task's copies".to_owned()
            } else {
                format!("Quit {}", apps.join(", "))
            }
        }
        Some("undo-plan") => {
            let targets: Vec<String> = step["targets"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|target| text::printable(target).into_owned())
                .collect();
            let id = step["planId"].as_str().unwrap_or("?");
            if targets.is_empty() {
                format!("Roll back copy {id} into the helper")
            } else {
                format!(
                    "Roll back copy {id} into the helper ({})",
                    targets.join(", ")
                )
            }
        }
        Some("remove-install") => format!(
            "Move {} from Applications to the helper's Trash",
            text::printable(step["app"].as_str().unwrap_or("the app"))
        ),
        Some("remove-copy") => {
            let path = text::printable(step["path"].as_str().unwrap_or("?")).into_owned();
            match step["app"].as_str() {
                Some(app) => format!("Remove the copy at {path} ({})", text::printable(app)),
                None => format!("Remove the copy at {path}"),
            }
        }
        other => other.unwrap_or("step").to_owned(),
    };

    let state = step["state"].as_str();
    let reason = step["reason"].as_str().map(reason);
    let mark = match state {
        Some("done") => style::ok(),
        Some("kept") => style::warn(),
        Some("failed" | "blocked") => style::fail(),
        Some("uncertain") => style::unknown(),
        _ => " ".to_owned(),
    };
    let said = match (state, reason) {
        (Some("done"), _) if step["alreadyGone"] == true => " · already gone".to_owned(),
        (Some("done"), _) if step["trashed"].is_string() => format!(
            " · in the Trash as {}",
            text::printable(step["trashed"].as_str().unwrap_or_default())
        ),
        (Some("kept"), Some(reason)) => format!(" · kept: {reason}"),
        (Some("failed"), Some(reason)) if step["retry"] == true => {
            format!(" · couldn't: {reason}; try again")
        }
        (Some("failed"), Some(reason)) => format!(" · couldn't: {reason}"),
        (Some("blocked"), _) => " · not run: agents are stopped".to_owned(),
        (Some("uncertain"), _) => " · outcome unknown".to_owned(),
        _ => String::new(),
    };
    format!("{mark} {n}. {what}{}", style::dim(said))
}

/// Why a step was kept or failed, in words.
fn reason(reason: &str) -> &str {
    match reason {
        "changed" => "changed since it was installed",
        "replaced" => "something else is there now",
        "another-session" => "another session made it",
        "label-unreadable" => "Atlas cannot tell whose it is",
        "kept-targets" => "the helper changed some of its files since",
        "other-volume" => "it could not be moved to the Trash without copying",
        "unreadable" => "Atlas cannot read its record of it",
        "timeout" => "it did not quit in time",
        "in-use" => "still in use",
        "io" => "the file system refused",
        "helper" => "the helper could not do it",
        "needs-step" => "a step before it is not done",
        "stopped" => "agents are stopped",
        other => other,
    }
}
