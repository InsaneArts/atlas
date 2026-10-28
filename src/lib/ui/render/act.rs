//! Views of what was done in the helper: input, typing, commands, opening,
//! waiting and recovery.
use super::{Rendered, card, element_name, failed, style, text, with_sessions};
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
        let message = format!(
            "Typed {} before it stopped: {}",
            text::count(characters, "character"),
            value["error"]
                .as_str()
                .unwrap_or("the helper did not confirm the rest")
        );
        let code = value["code"].as_str().unwrap_or("input");

        let shown = card(&message, Some(code), value["uncertain"] == true);
        return Rendered::err(with_sessions(shown, value));
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
            "{} The output could not be decoded; --json shows it as sent.",
            style::warn()
        ));
    }

    if value["outputTruncated"] == true {
        notes.push(format!(
            "{} The output was cut at 1 MB; write longer output to a file and read parts of it.",
            style::warn()
        ));
    }

    if value["outputUnfinished"] == true {
        notes.push(format!(
            "{} Something the command started still held its output; the end may be missing.",
            style::warn()
        ));
    }

    let took = value["durationMs"]
        .as_f64()
        .map(|ms| format!(" · {}", text::duration(ms)))
        .unwrap_or_default();
    let signal = &value["signal"];

    if value["timedOut"] == true {
        notes.push(format!(
            "{} Timed out and was stopped{took}; --timeout gives it longer",
            style::fail()
        ));
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

pub fn logout(_: &Value) -> String {
    format!(
        "{} The helper is logged out  {}",
        style::ok(),
        style::dim("atlas switch to log in to it again")
    )
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

    let id = value["planId"].as_str().unwrap_or("ID");
    let next = |what: &str, command: String| {
        format!("  {} {what}: {}", style::dim("→"), style::cmd(command))
    };

    // Stopped part way, whatever the reason: say how to finish.
    // A refused or previewed undo is an error like any other, not a stopped copy.
    let undo = value["operation"] == "rollback" || value["operation"] == "rollbackPreview";
    let partial = value["operation"] != "rollbackPreview"
        && (value["resumeRequired"] == true
            || (value["error"].is_string()
                && (value["phase"] == "applying" || value["phase"] == "rollingBack")))
        && (!undo || value["phase"] == "rollingBack");
    if partial {
        let undoing = undo || value["phase"] == "rollingBack" || value["phase"] == "rolledBack";
        let reason = value["error"]
            .as_str()
            .map(|error| format!(": {error}"))
            .unwrap_or_default();

        // A copy that can no longer finish is finished by undoing it.
        let (what, command) = if undoing {
            (
                "Undoing stopped part way",
                format!("atlas environment rollback {id} --apply"),
            )
        } else if value["rollbackRequired"] == true {
            (
                "Copying stopped part way",
                format!("atlas environment rollback {id} --apply"),
            )
        } else {
            (
                "Copying stopped part way",
                format!("atlas environment apply {id}"),
            )
        };

        return Rendered::err(format!(
            "{}\n{}",
            card(&format!("{what}{reason}"), None, false),
            next("To finish", command)
        ));
    }

    if value["error"].is_string() {
        return failed(value).unwrap_or_default();
    }

    let word = |operation: &str| match operation {
        "create" => "add",
        "update" => "update",
        "permissions" => "fix permissions",
        "unchanged" => "already there",
        _ => "conflict",
    };
    let line = |what: &str, target: &str| format!("  {:<16} ~/{}", what, text::printable(target));
    let listed = |file: &Value| {
        line(
            word(file["operation"].as_str().unwrap_or("")),
            file["target"].as_str().unwrap_or("?"),
        )
    };
    let targets = |key: &str| -> Vec<String> {
        value[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(|target| format!("  ~/{}", text::printable(target)))
            .collect()
    };
    let count = |n: usize| text::count(n, "file");
    let mut lines = Vec::new();

    if value["rolledBack"] == true {
        lines.push(format!("{} Undone in the helper", style::ok()));
        lines.extend(targets("completedTargets"));

        let kept = targets("keptTargets");
        if !kept.is_empty() {
            lines.push(format!(
                "Kept {} the helper changed since the copy:",
                count(kept.len())
            ));
            lines.extend(kept);
        }
    } else if value["preview"] == true {
        let pending = targets("pendingTargets");
        let kept = targets("keptTargets");

        if pending.is_empty() {
            lines.push(format!(
                "{} No files change back in the helper",
                style::ok()
            ));
        } else {
            lines.push(format!(
                "Undoing restores or removes {} in the helper:",
                count(pending.len())
            ));
            lines.extend(pending);
        }

        if !kept.is_empty() {
            lines.push(format!(
                "It keeps {} the helper changed since the copy:",
                count(kept.len())
            ));
            lines.extend(kept);
        }

        lines.push(next(
            "To undo",
            format!("atlas environment rollback {id} --apply"),
        ));
    } else if value["applied"] == true {
        lines.push(format!("{} Copied into the helper", style::ok()));
        lines.extend(files.iter().map(listed));
        lines.push(next(
            "To undo",
            format!("atlas environment rollback {id} --apply"),
        ));
    } else {
        let conflicts: Vec<String> = files
            .iter()
            .filter(|file| file["operation"] == "conflict")
            .map(|file| line("in the way", file["target"].as_str().unwrap_or("?")))
            .collect();
        if !conflicts.is_empty() {
            let mut err = vec![card(
                &format!(
                    "Nothing was copied: {} in the helper already {} or changed there since Atlas copied {}.",
                    count(conflicts.len()),
                    if conflicts.len() == 1 {
                        "exists"
                    } else {
                        "exist"
                    },
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
            lines.extend(files.iter().map(listed));
            lines.push(next("To copy", format!("atlas environment apply {id}")));
        }
    }

    Rendered::out(lines.join("\n"))
}

/// An app closed in the helper, and whether its copy went with it.
pub fn close(value: &Value) -> String {
    // `close --all` names every app it quit.
    if let Some(apps) = value["apps"].as_array() {
        let names: Vec<String> = apps
            .iter()
            .filter_map(Value::as_str)
            .map(|name| text::printable(name).into_owned())
            .collect();
        let mut line = if names.is_empty() {
            format!("{} No apps were open", style::ok())
        } else {
            format!("{} Closed {}", style::ok(), names.join(", "))
        };

        if value["removed"]
            .as_array()
            .is_some_and(|removed| !removed.is_empty())
        {
            line.push_str(&style::dim(" · removed Atlas's copies"));
        }

        if let Some(skipped) = value["skipped"]
            .as_array()
            .filter(|skipped| !skipped.is_empty())
        {
            let skipped: Vec<String> = skipped
                .iter()
                .filter_map(Value::as_str)
                .map(|name| text::printable(name).into_owned())
                .collect();

            line.push_str(&format!(
                "\n{} Left {}: it changed before it could be quit",
                style::warn(),
                skipped.join(", ")
            ));
        }

        line.push_str(&copies_left(value));
        return line;
    }

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
        line.push_str(&style::dim(" · removed Atlas's copy of it"));
    }

    line.push_str(&copies_left(value));
    line
}

/// What a close left of Atlas's copies: those another session made, and
/// those that could not be removed.
fn copies_left(value: &Value) -> String {
    let copies = |count: u64| match count {
        1 => "1 copy".to_owned(),
        count => format!("{count} copies"),
    };
    let mut said = String::new();
    if let Some(others) = value["keptOthers"].as_u64().filter(|count| *count > 0) {
        said.push_str(&style::dim(format!(
            " · kept {} another session made",
            copies(others)
        )));
    }
    if let Some(failed) = value["failed"]
        .as_array()
        .filter(|failed| !failed.is_empty())
    {
        said.push_str(&format!(
            "\n{} Could not remove {}; run the same close again",
            style::warn(),
            copies(u64::try_from(failed.len()).unwrap_or(u64::MAX))
        ));
    }
    said
}

/// What `environment discover` found, here and in the helper.
pub fn discover(value: &Value) -> String {
    let mut rows: Vec<Vec<String>> = Vec::new();

    for item in value["selections"].as_array().into_iter().flatten() {
        let name = text::printable(item["name"].as_str().unwrap_or("?")).into_owned();
        let here = item["host"]
            .as_str()
            .map(|path| text::tilde(std::path::Path::new(path)))
            .unwrap_or_else(|| "not found".to_owned());

        let helper = match item["kind"].as_str() {
            Some("tool") => item["helperCandidates"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|candidate| candidate["access"]["executable"] == true)
                .and_then(|candidate| candidate["path"].as_str())
                .map(|path| text::printable(path).into_owned())
                .unwrap_or_else(|| "not found".to_owned()),
            _ => "not checked".to_owned(),
        };

        let found = item["host"].is_string() && helper != "not found";
        let mark = if found { style::ok() } else { style::unknown() };
        rows.push(vec![format!("{mark} {name}"), here, helper]);
    }

    for variable in value["environment"].as_array().into_iter().flatten() {
        let yes = |key: &str| {
            if variable[key] == true {
                "set"
            } else {
                "not set"
            }
        };

        let mark = if variable["hostDefined"] == true && variable["helperInheritedDefined"] == true
        {
            style::ok()
        } else {
            style::unknown()
        };

        rows.push(vec![
            format!(
                "{mark} ${}",
                text::printable(variable["name"].as_str().unwrap_or("?"))
            ),
            yes("hostDefined").to_owned(),
            yes("helperInheritedDefined").to_owned(),
        ]);
    }

    for reference in value["references"].as_array().into_iter().flatten() {
        let access = |side: &Value| {
            match (side["readable"] == true, side["writable"] == true) {
                (true, true) => "read and write",
                (true, false) => "read only",
                _ => "no access",
            }
            .to_owned()
        };

        let mark = if reference["helper"]["readable"] == true {
            style::ok()
        } else {
            style::unknown()
        };

        rows.push(vec![
            format!(
                "{mark} {}",
                text::tilde(std::path::Path::new(
                    reference["path"].as_str().unwrap_or("?")
                ))
            ),
            access(&reference["host"]),
            access(&reference["helper"]),
        ]);
    }

    if rows.is_empty() {
        return "Nothing to check: name --tool, --app, --env or --reference.".to_owned();
    }

    crate::ui::table::render(&["", "HERE", "IN THE HELPER"], &rows)
}

/// `setup` without --apply: whether anything needs doing.
pub fn setup_preview(value: &Value) -> String {
    let steps: Vec<&str> = value["nextSteps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if steps.is_empty() {
        return format!("{} The helper's setup is current", style::ok());
    }

    let mut lines = vec![format!(
        "{} The helper's setup needs attention:",
        style::unknown()
    )];

    for (number, step) in steps.iter().enumerate() {
        lines.push(format!("  {}. {}", number + 1, text::printable(step)));
    }

    lines.join("\n")
}
