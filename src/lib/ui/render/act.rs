//! Views of what was done in the helper: input, typing, commands, opening,
//! waiting and recovery.
use super::{Rendered, card, element_name, failed, style, text};
use serde_json::Value;

/// An input action, said as done; a failure says how far it got.
pub fn action(done: &str, value: &Value) -> Rendered {
    if value["ok"] == false
        && let Some(performed) = value["performed"].as_u64()
        && performed > 0
    {
        let mut rendered = failed(value).unwrap_or_default();
        rendered.err.push_str(&format!(
            "\n  {}",
            style::dim(format!(
                "{} finished first.",
                text::count(performed as usize, "action")
            ))
        ));
        return rendered;
    }
    if let Some(failure) = failed(value) {
        return failure;
    }
    let element = &value["element"];
    let done = match (element["role"].as_str(), element_name(element)) {
        (Some(role), Some(name)) => format!(
            "{done} \"{}\" ({})",
            text::printable(name),
            super::role_word(role)
        ),
        (Some(role), None) => format!("{done} the {}", super::role_word(role)),
        _ => done.to_owned(),
    };
    Rendered::out(format!("{} {done}", style::ok()))
}

pub fn typed(value: &Value) -> Rendered {
    let characters = value["typed"].as_u64().unwrap_or(0) as usize;
    if value["ok"] == false {
        let batches = value["batches"].as_u64().unwrap_or(0);
        let confirmed = value["confirmedBatches"].as_u64().unwrap_or(0);
        let message = format!(
            "Typed {confirmed} of {batches} parts: {}",
            value["error"]
                .as_str()
                .unwrap_or("the helper did not confirm the rest")
        );
        return Rendered::err(card(&message, Some("input"), value["uncertain"] == true));
    }
    Rendered::out(format!(
        "{} Typed {}",
        style::ok(),
        text::count(characters, "character")
    ))
}

/// The command's own output, as it printed it; a note only when it failed.
pub fn run(value: &Value) -> Rendered {
    if let Some(failure) = failed(value) {
        return failure;
    }
    let mut rendered = Rendered::out(value["output"].as_str().unwrap_or("").to_owned());
    let mut notes = Vec::new();
    if value["output"].is_null() && value["events"].is_array() {
        notes.push(format!(
            "{} The output is over 1 MB; --json has all of it.",
            style::warn()
        ));
    }
    let took = value["durationMs"]
        .as_f64()
        .map(|ms| format!(" · {}", text::duration(ms)))
        .unwrap_or_default();
    let signal = &value["signal"];
    if value["timedOut"] == true {
        notes.push(format!("{} Timed out and was stopped{took}", style::fail()));
    } else if !signal.is_null() {
        let name = signal
            .as_str()
            .map_or_else(|| signal.to_string(), str::to_owned);
        notes.push(format!("{} Stopped by signal {name}{took}", style::fail()));
    } else if let Some(code) = value["exitCode"].as_i64().filter(|code| *code != 0) {
        notes.push(format!("{} Exited with {code}{took}", style::fail()));
    }
    rendered.err = notes.join("\n");
    rendered
}

pub fn wait(value: &Value, gone: bool) -> String {
    let after = value["waitedMs"]
        .as_f64()
        .map(|ms| format!(" after {}", text::duration(ms)))
        .unwrap_or_default();
    if gone {
        return format!("{} The window closed{after}", style::ok());
    }
    let window = value["windows"].get(0);
    let name = window
        .map(|window| {
            let owner = window["owner"].as_str().unwrap_or("A window");
            match window["title"].as_str().filter(|title| !title.is_empty()) {
                Some(title) => format!("{} \"{}\"", text::printable(owner), text::printable(title)),
                None => text::printable(owner).into_owned(),
            }
        })
        .unwrap_or_else(|| "The window".to_owned());
    format!("{} {name} appeared{after}", style::ok())
}

pub fn restart(value: &Value) -> String {
    let mut line = format!("{} The helper restarted and is ready", style::ok());
    if value["inputReplaced"] == true {
        line.push_str(&format!("  {}", style::dim("input service updated")));
    }
    line
}

pub fn start_input(value: &Value) -> String {
    let how = if value["alreadyRunning"] == true {
        "was already running"
    } else if value["recovered"] == true {
        "was reset and is ready"
    } else if value["replaced"] == true || value["upgraded"] == true {
        "was updated and is ready"
    } else {
        "started"
    };
    format!("{} The input service {how}", style::ok())
}

pub fn install(value: &Value) -> String {
    let path = value["path"].as_str().map(text::tilde).unwrap_or_default();
    if value["alreadyInstalled"] == true {
        format!(
            "{} The shared command is current  {}",
            style::ok(),
            style::dim(path)
        )
    } else {
        format!(
            "{} Installed the shared command  {}",
            style::ok(),
            style::dim(path)
        )
    }
}

/// An environment preview, copy or undo: what changes in the helper's home,
/// and the one command that comes next.
pub fn environment(value: &Value) -> Rendered {
    let Some(files) = value["files"].as_array() else {
        return failed(value).unwrap_or_else(|| Rendered::out(super::generic(value)));
    };
    if value["error"].is_string() {
        return failed(value).unwrap_or_default();
    }
    let id = value["transactionId"].as_str().unwrap_or("ID");
    let word = |operation: &str| match operation {
        "create" => "add",
        "update" => "update",
        "permissions" => "fix permissions",
        "unchanged" => "already there",
        _ => "conflict",
    };
    let line = |what: &str, target: &str| format!("  {:<16} ~/{}", what, text::printable(target));
    let targets = |key: &str| -> Vec<String> {
        value[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(|target| format!("  ~/{}", text::printable(target)))
            .collect()
    };
    let next = |what: &str, command: String| {
        format!("  {} {what}: {}", style::dim("→"), style::cmd(command))
    };
    let count = |n: usize| text::count(n, "file");
    let mut lines = Vec::new();
    if value["rolledBack"] == true {
        lines.push(format!("{} Undone in the helper", style::ok()));
        lines.extend(targets("completedTargets"));
    } else if value["preview"] == true {
        let pending = targets("pendingTargets");
        if pending.is_empty() {
            lines.push(format!("{} Nothing to undo", style::ok()));
        } else {
            lines.push(format!(
                "Undoing restores or removes {} in the helper:",
                count(pending.len())
            ));
            lines.extend(pending);
            lines.push(next(
                "To undo",
                format!("understudy environment rollback {id} --apply"),
            ));
        }
    } else if value["applied"] == true {
        if value["resumeRequired"] == true {
            let left = value["pendingTargets"].as_array().map_or(0, Vec::len);
            return Rendered::err(format!(
                "{}\n{}",
                card(
                    &format!("Copying stopped with {} left.", count(left)),
                    None,
                    false
                ),
                next("To finish", format!("understudy environment apply {id}"))
            ));
        }
        lines.push(format!("{} Copied into the helper", style::ok()));
        for file in files {
            let operation = file["operation"].as_str().unwrap_or("");
            lines.push(line(
                word(operation),
                file["target"].as_str().unwrap_or("?"),
            ));
        }
        lines.push(next(
            "To undo",
            format!("understudy environment rollback {id} --apply"),
        ));
    } else {
        let conflicts: Vec<String> = files
            .iter()
            .filter(|file| file["operation"] == "conflict")
            .map(|file| line("changed there", file["target"].as_str().unwrap_or("?")))
            .collect();
        if !conflicts.is_empty() {
            let mut err = vec![card(
                &format!(
                    "{} changed in the helper since Understudy last copied {}, so nothing was copied.",
                    count(conflicts.len()),
                    if conflicts.len() == 1 { "it" } else { "them" }
                ),
                None,
                false,
            )];
            err.extend(conflicts);
            return Rendered::err(err.join("\n"));
        }
        if files.iter().all(|file| file["operation"] == "unchanged") {
            lines.push(format!(
                "{} The helper already has these files",
                style::ok()
            ));
        } else {
            lines.push(format!(
                "Would copy into the helper ({}):",
                count(files.len())
            ));
            for file in files {
                let operation = file["operation"].as_str().unwrap_or("");
                lines.push(line(
                    word(operation),
                    file["target"].as_str().unwrap_or("?"),
                ));
            }
            lines.push(next(
                "To copy",
                format!("understudy environment apply {id}"),
            ));
        }
    }
    Rendered::out(lines.join("\n"))
}

/// An app closed in the helper, and whether its copy went with it.
pub fn close(value: &Value) -> String {
    let name = text::printable(value["app"].as_str().unwrap_or("The app")).into_owned();
    let mut line = if value["closed"] == true {
        format!("{} Closed {name}", style::ok())
    } else {
        format!("{} {name} was not running", style::ok())
    };
    if value["removed"]
        .as_array()
        .is_some_and(|removed| !removed.is_empty())
    {
        line.push_str(&style::dim(" · removed Understudy's copy of it"));
    }
    line
}
