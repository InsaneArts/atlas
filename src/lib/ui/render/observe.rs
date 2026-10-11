//! Views of what the helper is and shows: doctor, status, configuration,
//! screenshots, apps, windows, elements and readiness.
use super::{Rendered, failed, style, text};
use crate::ui::table;
use serde_json::Value;

fn helper_name(value: &Value) -> Option<&str> {
    value["helper"]["name"]
        .as_str()
        .or_else(|| value["session"]["user"]["name"].as_str())
        .or_else(|| value["user"]["name"].as_str())
}

fn display(value: &Value) -> Option<String> {
    let display = if value["display"].is_object() {
        &value["display"]
    } else {
        &value["session"]["display"]
    };
    Some(text::dimensions(
        display["width"].as_f64()?,
        display["height"].as_f64()?,
    ))
}

/// Ready in one line; otherwise each check and the numbered fixes. Verbose
/// adds the facts behind them.
pub fn doctor(value: &Value, verbose: bool) -> String {
    let facts: Vec<String> = [
        helper_name(value).map(|name| format!("helper {}", text::printable(name))),
        display(value),
    ]
    .into_iter()
    .flatten()
    .collect();
    let mut lines = Vec::new();
    if value["ready"] == true {
        let detail = if facts.is_empty() {
            String::new()
        } else {
            format!("  {}", style::dim(facts.join(" · ")))
        };
        lines.push(format!("{} Atlas is ready{detail}", style::ok()));
    } else {
        lines.push(format!("{} Atlas is not ready", style::fail()));
        lines.push(String::new());
        for check in value["checks"].as_array().into_iter().flatten() {
            let name = check["name"].as_str().unwrap_or("Check");
            lines.push(format!("  {} {name}", style::mark(check["ok"].as_bool())));
        }
        let steps: Vec<&str> = value["nextSteps"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if !steps.is_empty() {
            lines.push(String::new());
            lines.push("To fix:".to_owned());
            for (number, step) in steps.iter().enumerate() {
                lines.push(format!("  {}. {step}", number + 1));
            }
        }
    }
    for warning in value["warnings"].as_array().into_iter().flatten() {
        if let Some(warning) = warning.as_str() {
            lines.push(format!("{} {warning}", style::warn()));
        }
    }
    let native = &value["capabilities"]["nativeDialogs"];
    if native["state"] == "failed" {
        lines.push(format!(
            "{} Native Save and Open dialogs failed their last check. atlas verify-native checks again.",
            style::warn()
        ));
    }
    if verbose {
        lines.push(String::new());
        lines.extend(details(value, native));
    }
    lines.join("\n")
}

/// The facts behind a doctor report, for `doctor --verbose`.
fn details(value: &Value, native: &Value) -> Vec<String> {
    let session = &value["session"];
    let fact = |value: &Value| match value {
        Value::Null => "unknown".to_owned(),
        Value::String(text) => text::printable(text).into_owned(),
        other => other.to_string(),
    };
    let rows = [
        (
            "Helper",
            format!(
                "{} (uid {})",
                fact(&session["user"]["name"]),
                fact(&session["user"]["uid"])
            ),
        ),
        (
            "Display",
            display(value).unwrap_or_else(|| "unknown".into()),
        ),
        (
            "Helper app",
            format!(
                "pid {} · version {}",
                fact(&session["daemon"]["pid"]),
                fact(&session["daemon"]["version"])
            ),
        ),
        (
            "Input service",
            format!(
                "pid {} · {}",
                fact(&session["inputService"]["pid"]),
                if session["inputService"]["ready"] == true {
                    "ready"
                } else {
                    "not ready"
                }
            ),
        ),
        (
            "Save/Open",
            format!("{} · {}", fact(&native["state"]), fact(&native["reason"])),
        ),
    ];
    let mut lines: Vec<String> = rows
        .iter()
        .map(|(label, shown)| format!("{}  {shown}", style::dim(format!("{label:<13}"))))
        .collect();
    for check in value["checks"].as_array().into_iter().flatten() {
        if let Some(step) = check["nextStep"].as_str() {
            lines.push(format!(
                "{}  {}",
                style::dim(format!("{:<13}", check["name"].as_str().unwrap_or(""))),
                step
            ));
        }
    }
    lines.push(style::dim("--json --verbose has everything."));
    lines
}

/// Who the helper is, where it is, and what it may do.
pub fn status(value: &Value) -> Rendered {
    if value["ok"] != true {
        return failed(&serde_json::json!({
            "ok": false,
            "code": value["code"],
            "error": value["error"].as_str().unwrap_or("The helper did not answer."),
        }))
        .unwrap_or_default();
    }
    let name = helper_name(value).unwrap_or("helper");
    let place = match value["session"]["onConsole"].as_bool() {
        Some(true) => "on screen",
        Some(false) => "in the background",
        None => "unknown",
    };
    let allowed = |key: &str| style::mark(value["permissions"][key].as_bool());
    let input = &value["inputService"];
    let input = if input["ready"] == true {
        "ready".to_owned()
    } else if input["ok"] == true {
        "running, not ready".to_owned()
    } else {
        "not running".to_owned()
    };
    let rows = [
        ("Helper", format!("{} · {place}", text::printable(name))),
        (
            "Display",
            display(value).unwrap_or_else(|| "unknown".into()),
        ),
        (
            "Allowed",
            format!(
                "Screen Recording {}  Accessibility {}",
                allowed("screenCapture"),
                allowed("accessibility")
            ),
        ),
        ("Input", input),
        (
            "Version",
            value["daemon"]["version"]
                .as_str()
                .unwrap_or("unknown")
                .to_owned(),
        ),
    ];
    Rendered::out(
        rows.iter()
            .map(|(label, shown)| format!("{}  {shown}", style::dim(format!("{label:<8}"))))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// The resolved settings, labelled in words.
pub fn config(value: &Value) -> String {
    let config = &value["config"];
    let rows = [
        ("Settings", path(&value["path"])),
        ("Helper user ID", config["helper_uid"].to_string()),
        ("Screenshots", path(&config["output_dir"])),
        ("Shared command", path(&config["identity_probe"])),
        ("Helper socket", path(&config["socket_path"])),
        ("Input socket", path(&config["input_socket"])),
    ];
    rows.iter()
        .map(|(label, shown)| format!("{}  {shown}", style::dim(format!("{label:<14}"))))
        .collect::<Vec<_>>()
        .join("\n")
}

fn path(value: &Value) -> String {
    value
        .as_str()
        .map(text::tilde)
        .unwrap_or_else(|| "—".into())
}

pub fn capture(value: &Value) -> String {
    let mut lines = vec![format!(
        "{} Screenshot saved  {}",
        style::ok(),
        path(&value["path"])
    )];
    let mut facts = Vec::new();
    if let (Some(width), Some(height)) = (value["width"].as_f64(), value["height"].as_f64()) {
        facts.push(format!("{} pixels", text::dimensions(width, height)));
    }
    if let Some(ratio) = value["pointsPerPixel"].as_f64() {
        facts.push(if ratio == 1.0 {
            "1 point per pixel".to_owned()
        } else {
            format!("{} points per pixel", text::number(ratio))
        });
    }
    if !facts.is_empty() {
        lines.push(format!("  {}", style::dim(facts.join(" · "))));
    }
    lines.join("\n")
}

/// The helper's apps a person would see in the Dock.
pub fn apps(value: &Value) -> String {
    let apps: Vec<&Value> = value["apps"].as_array().into_iter().flatten().collect();
    let shown: Vec<&&Value> = apps
        .iter()
        .filter(|app| app["policy"] == "regular")
        .collect();
    let mut lines = Vec::new();
    if shown.is_empty() {
        lines.push("No apps are open in the helper.".to_owned());
    } else {
        let rows: Vec<Vec<String>> = shown
            .iter()
            .map(|app| {
                let state = if app["active"] == true {
                    "frontmost"
                } else if app["hidden"] == true {
                    "hidden"
                } else {
                    ""
                };
                vec![
                    text::printable(app["name"].as_str().unwrap_or("?")).into_owned(),
                    app["pid"].to_string(),
                    state.to_owned(),
                ]
            })
            .collect();
        lines.push(table::render(&["APP", "PID", "STATE"], &rows));
    }
    let background = apps.len() - shown.len();
    if background > 0 {
        let what = if background == 1 {
            "1 background process".to_owned()
        } else {
            format!("{background} background processes")
        };
        lines.push(style::dim(format!("{what} not shown; --json lists them.")));
    }
    lines.join("\n")
}

pub fn windows(value: &Value) -> String {
    let windows: Vec<&Value> = value["windows"].as_array().into_iter().flatten().collect();
    if windows.is_empty() {
        return "No windows. --all includes panels and menus.".to_owned();
    }
    let rows: Vec<Vec<String>> = windows
        .iter()
        .map(|window| {
            let bounds = &window["bounds"];
            let number = |key: &str| text::number(bounds[key].as_f64().unwrap_or(0.0));
            vec![
                text::clip(
                    &text::printable(window["owner"].as_str().unwrap_or("?")),
                    24,
                ),
                text::clip(&text::printable(window["title"].as_str().unwrap_or("")), 40),
                window["pid"].to_string(),
                format!("{}, {}", number("x"), number("y")),
                format!("{} × {}", number("width"), number("height")),
            ]
        })
        .collect();
    format!(
        "{}\n{}",
        table::render(&["APP", "TITLE", "PID", "POSITION", "SIZE"], &rows),
        style::dim(format!(
            "{}, front to back",
            text::count(windows.len(), "window")
        ))
    )
}

pub fn elements(value: &Value) -> String {
    let elements: Vec<&Value> = value["elements"].as_array().into_iter().flatten().collect();
    let matching = value["matching"].as_u64().unwrap_or(elements.len() as u64);
    let scanned = value["scanned"].as_u64().unwrap_or(matching);
    let mut lines = vec![if matching == scanned {
        format!(
            "Process {} · {}",
            value["pid"],
            text::count(scanned as usize, "element")
        )
    } else {
        format!(
            "Process {} · {matching} of {scanned} elements match",
            value["pid"]
        )
    }];
    if !elements.is_empty() {
        let rows: Vec<Vec<String>> = elements
            .iter()
            .map(|element| {
                let name = super::element_name(element).unwrap_or("");
                let frame = &element["frame"];
                let at = match (frame["x"].as_f64(), frame["y"].as_f64()) {
                    (Some(x), Some(y)) => format!("{}, {}", text::number(x), text::number(y)),
                    _ => String::new(),
                };
                vec![
                    element["index"].to_string(),
                    text::clip(&text::printable(element["role"].as_str().unwrap_or("")), 24),
                    text::clip(&text::printable(name), 32),
                    text::clip(
                        &text::printable(element["value"].as_str().unwrap_or("")),
                        24,
                    ),
                    at,
                ]
            })
            .collect();
        lines.push(String::new());
        lines.push(table::render(&["#", "ROLE", "NAME", "VALUE", "AT"], &rows));
    }
    if (elements.len() as u64) < matching {
        lines.push(style::dim(format!(
            "Showing {} of {matching}; --max shows more.",
            elements.len()
        )));
    }
    if value["scanIncomplete"] == true {
        lines.push(format!(
            "{} The scan stopped early, so some elements may be missing.",
            style::warn()
        ));
    }
    lines.join("\n")
}

pub fn readiness(value: &Value) -> String {
    let mut lines = Vec::new();
    for item in value["items"].as_array().into_iter().flatten() {
        let status = item["status"].as_str().unwrap_or("unknown");
        let mark = match status {
            "ready" | "installed" => style::ok(),
            "missing" | "failing" => style::fail(),
            _ => style::unknown(),
        };
        let detail = item["detail"].as_str().unwrap_or("");
        let detail = if detail.is_empty() {
            status.to_owned()
        } else {
            format!("{status} · {}", text::printable(detail))
        };
        lines.push(format!(
            "{mark} {}  {}",
            text::printable(item["name"].as_str().unwrap_or("?")),
            style::dim(detail)
        ));
    }
    let unchecked: Vec<&str> = value["notVerified"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if !unchecked.is_empty() {
        lines.push(style::dim(format!(
            "Not checked: {}.",
            unchecked.join(", ")
        )));
    }
    lines.join("\n")
}
