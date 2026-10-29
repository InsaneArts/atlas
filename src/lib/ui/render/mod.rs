//! A command's result as a person reads it. Each view takes the same JSON a
//! program gets, so the two never disagree; anything a view does not know
//! falls back to a short generic summary, never to raw JSON.
mod act;
mod activity;
mod cleanup;
mod jobs;
mod manage;
mod observe;

use super::{hints, style, text};
use crate::error::{Code, Error};
use serde_json::Value;

/// Which command produced a result, with what the result alone cannot say.
#[derive(Clone, Debug)]
pub enum View {
    Doctor {
        verbose: bool,
    },
    LoginTiming,
    Status,
    Activity,
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
    /// The copies into the helper `environment list` found.
    Plans,
    Discover,
    SetupPreview,
    Close,
    /// A task's cleanup: its preview or a run.
    Cleanup,
    /// An app installed into the helper for a task.
    InstallApp,
    /// A background job started or stopped.
    Job,
    /// A task's background jobs.
    Jobs,
    /// The end of a job's output.
    JobLog,
    /// The copies a collection kept.
    Collect,
    /// What was opened.
    Open(String),
    Wait {
        gone: bool,
    },
    Restart,
    Logout,
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
        View::LoginTiming => Rendered::out(observe::login_timing(value)),
        View::Status => observe::status(value),
        View::Activity => failed(value).unwrap_or_else(|| Rendered::out(activity::activity(value))),
        View::Config => Rendered::out(observe::config(value)),
        View::Capture => failed(value).unwrap_or_else(|| Rendered::out(observe::capture(value))),
        View::Apps => failed(value).unwrap_or_else(|| Rendered::out(observe::apps(value))),
        View::Windows => failed(value).unwrap_or_else(|| Rendered::out(observe::windows(value))),
        View::Elements => failed(value).unwrap_or_else(|| Rendered::out(observe::elements(value))),
        View::Readiness => Rendered::out(observe::readiness(value)),
        View::Action(done) => act::action(done, value),
        View::Type => act::typed(value),
        View::Run => act::run(value),
        View::Open(what) => {
            // The copies made for it are said either way: a failed launch
            // leaves them where close removes them.
            let copied = if value["copied"]
                .as_array()
                .is_some_and(|copied| !copied.is_empty())
            {
                format!(
                    "\n  {}",
                    style::dim(format!(
                        "copied first to {}, where the helper can read it",
                        crate::handoff::ROOT
                    ))
                )
            } else {
                String::new()
            };

            match failed(value) {
                Some(failure) => Rendered::err(format!("{}{copied}", failure.err)),
                None => Rendered::out(format!(
                    "{} Opened {}{copied}",
                    style::ok(),
                    text::printable(what)
                )),
            }
        }
        View::Wait { gone } => {
            failed(value).unwrap_or_else(|| Rendered::out(act::wait(value, *gone)))
        }
        View::Environment => act::environment(value),
        View::Plans => failed(value).unwrap_or_else(|| Rendered::out(act::plans(value))),
        View::Discover => failed(value).unwrap_or_else(|| Rendered::out(act::discover(value))),
        View::SetupPreview => {
            failed(value).unwrap_or_else(|| Rendered::out(act::setup_preview(value)))
        }
        View::Close => failed(value).unwrap_or_else(|| Rendered::out(act::close(value))),
        View::Cleanup => cleanup::cleanup(value),
        View::Job => jobs::job(value),
        View::Jobs => jobs::jobs(value),
        View::JobLog => jobs::log(value),
        View::Collect => jobs::collect(value),
        View::InstallApp => {
            failed(value).unwrap_or_else(|| Rendered::out(cleanup::install_app(value)))
        }
        View::Restart => failed(value).unwrap_or_else(|| Rendered::out(act::restart(value))),
        View::Logout => failed(value).unwrap_or_else(|| Rendered::out(act::logout(value))),
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

    let shown = advised_card(message, value);
    let shown = with_sessions(shown, value);
    Some(Rendered::err(with_candidates(shown, &value["candidates"])))
}

/// An error as a person reads it.
pub fn error(error: &Error) -> String {
    if error.code == Code::Cancelled {
        return format!("{} {}", style::unknown(), error.error);
    }

    let shown = advised_card(&error.error, &serde_json::json!(error));
    let shown = with_sessions(shown, &Value::Object(error.details.clone()));
    let candidates = error.details.get("candidates").unwrap_or(&Value::Null);
    with_candidates(shown, candidates)
}

/// A card followed by the agent session a refusal names, when it names
/// one: who holds the helper, who last used the app in front, or whose an
/// unknown action is.
fn with_sessions(card: String, details: &Value) -> String {
    let named = [
        (
            "Held by",
            details["heldBy"]["name"].as_str().map(str::to_owned),
        ),
        (
            "Last used by",
            details["lastUsedBy"]["name"].as_str().map(str::to_owned),
        ),
        (
            "Left by",
            details["causedBy"].as_array().map(|names| {
                let names: Vec<String> = names
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect();
                text::listed(&names)
            }),
        ),
    ];

    let mut lines = vec![card];
    lines.extend(
        named
            .into_iter()
            .filter_map(|(said, name)| Some((said, name.filter(|name| !name.is_empty())?)))
            .map(|(said, name)| format!("  {said} {}.", text::printable(&name))),
    );
    lines.join("\n")
}

/// A card followed by the elements a refused name may have meant, one a
/// line: `· "Format" (menu bar item) at 356, 11`.
fn with_candidates(card: String, candidates: &Value) -> String {
    let mut lines = vec![card];
    lines.extend(candidate_lines(candidates));
    lines.join("\n")
}

/// Up to five elements a name may have meant, each on a line of its own.
fn candidate_lines(candidates: &Value) -> Vec<String> {
    candidates
        .as_array()
        .into_iter()
        .flatten()
        .take(5)
        .map(|candidate| {
            let mut line = format!(
                "  · \"{}\"",
                text::printable(candidate["name"].as_str().unwrap_or(""))
            );
            if let Some(role) = candidate["role"].as_str() {
                line.push_str(&format!(" ({})", role_word(role)));
            }
            if let Some((x, y)) = point(&candidate["at"]) {
                line.push_str(&format!(" at {}, {}", text::number(x), text::number(y)));
            }
            line
        })
        .collect()
}

/// A point a reply gives as `[x, y]`.
fn point(at: &Value) -> Option<(f64, f64)> {
    Some((at[0].as_f64()?, at[1].as_f64()?))
}

/// `✗ what went wrong` and, when the message does not already say, `→ what
/// to do`. Messages can carry text from the helper, so each line is made
/// safe to print.
pub fn card(message: &str, code: Option<Code>, uncertain: bool) -> String {
    let fix = if uncertain {
        Some(hints::UNCERTAIN)
    } else if message.to_lowercase().contains("run atlas ") {
        // The message already says what to run.
        None
    } else {
        code.and_then(hints::fix)
    };

    card_with(message, fix)
}

/// [`card`] for a failed `result`: a focus refusal says what to do as JSON
/// output says it, by what was in the way and what was sent first.
pub(super) fn advised_card(message: &str, result: &Value) -> String {
    match hints::focus_advice(result) {
        Some(advice) => card_with(message, Some(&advice)),
        None => card(
            message,
            result["code"].as_str().and_then(Code::parse),
            result["uncertain"] == true,
        ),
    }
}

/// `✗ message`, then `→ fix` when there is one.
fn card_with(message: &str, fix: Option<&str>) -> String {
    let message = message
        .trim()
        .lines()
        .map(text::printable)
        .collect::<Vec<_>>()
        .join("\n");
    let mut lines = vec![format!("{} {message}", style::fail())];
    if let Some(fix) = fix {
        lines.push(format!("  {} {fix}", style::dim("→")));
    }

    lines.join("\n")
}

/// What an accessibility element is called: its first non-empty name,
/// title, description or identifier.
fn element_name(element: &Value) -> Option<&str> {
    ["name", "title", "description", "identifier"]
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
