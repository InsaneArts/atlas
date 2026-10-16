//! A command's result as a person reads it. Each view takes the same JSON a
//! program gets, so the two never disagree; anything a view does not know
//! falls back to a short generic summary, never to raw JSON.
mod act;
mod manage;
mod observe;

use super::{hints, style, text};
use crate::error::Error;
use serde_json::Value;

/// Which command produced a result, with what the result alone cannot say.
#[derive(Clone, Debug)]
pub enum View {
    Doctor {
        verbose: bool,
    },
    Status,
    Config,
    Capture,
    Apps,
    Windows,
    Elements,
    Readiness,
    /// An input action, said as done: "Clicked at 10, 20".
    Action(String),
    Type,
    Run,
    Environment,
    Discover,
    SetupPreview,
    Close,
    /// What was opened.
    Open(String),
    Wait {
        gone: bool,
    },
    Restart,
    StartInput,
    Install,
    Agents,
    AgentsChange,
    AgentsProject,
    Switch,
    Update,
    Uninstall,
    Login,
    /// A flow that already told the person everything as it went.
    Quiet,
    Generic,
}

/// What to print: the result on standard output, notes on standard error.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Rendered {
    pub out: String,
    pub err: String,
}

impl Rendered {
    fn out(text: impl Into<String>) -> Self {
        Self {
            out: text.into(),
            err: String::new(),
        }
    }
    fn err(text: impl Into<String>) -> Self {
        Self {
            out: String::new(),
            err: text.into(),
        }
    }
}

pub fn render(view: &View, value: &Value) -> Rendered {
    match view {
        View::Doctor { verbose } => Rendered::out(observe::doctor(value, *verbose)),
        View::Status => observe::status(value),
        View::Config => Rendered::out(observe::config(value)),
        View::Capture => failed(value).unwrap_or_else(|| Rendered::out(observe::capture(value))),
        View::Apps => failed(value).unwrap_or_else(|| Rendered::out(observe::apps(value))),
        View::Windows => failed(value).unwrap_or_else(|| Rendered::out(observe::windows(value))),
        View::Elements => failed(value).unwrap_or_else(|| Rendered::out(observe::elements(value))),
        View::Readiness => Rendered::out(observe::readiness(value)),
        View::Action(done) => act::action(done, value),
        View::Type => act::typed(value),
        View::Run => act::run(value),
        View::Open(what) => failed(value).unwrap_or_else(|| {
            let mut out = format!("{} Opened {}", style::ok(), text::printable(what));
            if value["copied"]
                .as_array()
                .is_some_and(|copied| !copied.is_empty())
            {
                out.push_str(&format!(
                    "\n  {}",
                    style::dim(format!(
                        "copied first to {}, where the helper can read it",
                        crate::handoff::ROOT
                    ))
                ));
            }
            Rendered::out(out)
        }),
        View::Wait { gone } => {
            failed(value).unwrap_or_else(|| Rendered::out(act::wait(value, *gone)))
        }
        View::Environment => act::environment(value),
        View::Discover => failed(value).unwrap_or_else(|| Rendered::out(act::discover(value))),
        View::SetupPreview => {
            failed(value).unwrap_or_else(|| Rendered::out(act::setup_preview(value)))
        }
        View::Close => failed(value).unwrap_or_else(|| Rendered::out(act::close(value))),
        View::Restart => failed(value).unwrap_or_else(|| Rendered::out(act::restart(value))),
        View::StartInput => failed(value).unwrap_or_else(|| Rendered::out(act::start_input(value))),
        View::Install => failed(value).unwrap_or_else(|| Rendered::out(act::install(value))),
        View::Agents => Rendered::out(manage::agents(value)),
        View::AgentsChange => manage::agents_change(value),
        View::AgentsProject => {
            failed(value).unwrap_or_else(|| Rendered::out(manage::agents_project(value)))
        }
        View::Switch => failed(value).unwrap_or_else(|| Rendered::out(manage::switch(value))),
        View::Update => manage::update(value),
        View::Uninstall => failed(value).unwrap_or_else(|| Rendered::out(manage::uninstall(value))),
        View::Login => failed(value).unwrap_or_else(|| Rendered::out(manage::login(value))),
        View::Quiet => failed(value).unwrap_or_default(),
        View::Generic => failed(value).unwrap_or_else(|| Rendered::out(generic(value))),
    }
}

/// A failed result (`ok: false`) as an error card on standard error.
fn failed(value: &Value) -> Option<Rendered> {
    if value["ok"] != false {
        return None;
    }
    let message = value["error"]
        .as_str()
        .or_else(|| value["failure"]["error"].as_str())
        .unwrap_or("It did not finish. Run it again with --json to see why.");
    Some(Rendered::err(card(
        message,
        value["code"].as_str(),
        value["uncertain"] == true,
    )))
}

/// An error as a person reads it.
pub fn error(error: &Error) -> String {
    if error.code == "cancelled" {
        return format!("{} {}", style::unknown(), error.error);
    }
    card(&error.error, Some(error.code), error.uncertain)
}

/// `✗ what went wrong` and, when the message does not already say, `→ what
/// to do`. Messages can carry text from the helper, so each line is made
/// safe to print.
pub fn card(message: &str, code: Option<&str>, uncertain: bool) -> String {
    let message = message
        .trim()
        .lines()
        .map(text::printable)
        .collect::<Vec<_>>()
        .join("\n");
    let mut lines = vec![format!("{} {message}", style::fail())];
    let fix = if uncertain {
        Some(
            "It may or may not have happened, so don't repeat it. Take a screenshot, then run atlas restart.",
        )
    } else if message.to_lowercase().contains("run atlas ") {
        // The message already says what to run.
        None
    } else {
        code.and_then(hints::fix)
    };
    if let Some(fix) = fix {
        lines.push(format!("  {} {fix}", style::dim("→")));
    }
    lines.join("\n")
}

/// What an accessibility element is called: its first non-empty title,
/// description or identifier.
fn element_name(element: &Value) -> Option<&str> {
    ["title", "description", "identifier"]
        .iter()
        .filter_map(|key| element[*key].as_str())
        .find(|name| !name.is_empty())
}

/// An accessibility role in words: `AXButton` as `button`.
fn role_word(role: &str) -> String {
    let word = role.strip_prefix("AX").unwrap_or(role);
    let mut out = String::new();
    for (index, character) in word.chars().enumerate() {
        if character.is_uppercase() && index > 0 {
            out.push(' ');
        }
        out.extend(character.to_lowercase());
    }
    text::printable(&out).into_owned()
}

/// The top-level facts of a result, for commands without a view.
fn generic(value: &Value) -> String {
    let mut lines = vec![format!("{} Done", style::ok())];
    if let Some(object) = value.as_object() {
        let facts: Vec<(String, String)> = object
            .iter()
            .filter(|(key, _)| key.as_str() != "ok")
            .filter_map(|(key, value)| {
                let shown = match value {
                    Value::String(text) => text::printable(text).into_owned(),
                    Value::Bool(fact) => if *fact { "yes" } else { "no" }.to_owned(),
                    Value::Number(number) => number.to_string(),
                    _ => return None,
                };
                Some((words(key), shown))
            })
            .collect();
        let width = facts.iter().map(|(key, _)| key.len()).max().unwrap_or(0);
        for (key, shown) in facts {
            lines.push(format!("  {key:width$}  {shown}"));
        }
    }
    lines.push(style::dim("  --json shows everything."));
    lines.join("\n")
}

/// `planId` as `Plan id`.
fn words(key: &str) -> String {
    let mut out = String::new();
    for (index, character) in key.chars().enumerate() {
        if index == 0 {
            out.extend(character.to_uppercase());
        } else if character.is_uppercase() {
            out.push(' ');
            out.extend(character.to_lowercase());
        } else {
            out.push(character);
        }
    }
    out
}

#[cfg(test)]
mod tests;
