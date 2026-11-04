//! A command's result as a person reads it. Each view takes the same JSON a
//! program gets, so the two never disagree; anything a view does not know
//! falls back to a short generic summary, never to raw JSON.
mod act;
mod activity;
mod cleanup;
mod jobs;
mod manage;
mod observe;

use super::{style, text};
use atlas_core::error::advice as hints;
use atlas_core::error::{Code, Error};
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
    /// A listing of elements; of their values and state alone when `values`.
    Elements {
        values: bool,
    },
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
    /// The menu bar asked to start: said only when it already ran, as one
    /// that starts tells the person itself.
    Menubar,
    /// The viewer window opened.
    Monitor,
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
        View::Activity => checked(value, activity::activity),
        View::Config => Rendered::out(observe::config(value)),
        View::Capture => checked(value, observe::capture),
        View::Apps => checked(value, observe::apps),
        View::Windows => checked(value, observe::windows),
        View::Elements { values } => checked(value, |value| observe::elements(value, *values)),
        View::Readiness => Rendered::out(observe::readiness(value)),
        View::Action(done) => act::action(done, value),
        View::Type => act::typed(value),
        View::Run => act::run(value),
        View::Open(what) => act::opened(what, value),
        View::Wait { gone } => checked(value, |value| act::wait(value, *gone)),
        View::Environment => act::environment(value),
        View::Plans => checked(value, act::plans),
        View::Discover => checked(value, act::discover),
        View::SetupPreview => checked(value, act::setup_preview),
        View::Close => checked(value, act::close),
        View::Cleanup => cleanup::cleanup(value),
        View::Job => checked(value, jobs::job),
        View::Jobs => checked(value, jobs::jobs),
        View::JobLog => jobs::log(value),
        View::Collect => jobs::collect(value),
        View::InstallApp => checked(value, cleanup::install_app),
        View::Restart => checked(value, act::restart),
        View::Logout => checked(value, act::logout),
        View::StartInput => checked(value, act::start_input),
        View::Install => checked(value, act::install),
        View::Agents => Rendered::out(manage::agents(value)),
        View::AgentsChange => manage::agents_change(value),
        View::AgentsProject => checked(value, manage::agents_project),
        View::Switch => checked(value, manage::switch),
        View::Update => manage::update(value),
        View::Uninstall => checked(value, manage::uninstall),
        View::Login => checked(value, manage::login),
        View::Menubar => checked(value, manage::menubar),
        View::Monitor => checked(value, |_| manage::monitor()),
        View::Quiet => failed(value).unwrap_or_default(),
        View::Generic => checked(value, generic),
    }
}

/// The failure card when `value` failed, otherwise what `show` makes of it.
fn checked(value: &Value, show: impl FnOnce(&Value) -> String) -> Rendered {
    failed(value).unwrap_or_else(|| Rendered::out(show(value)))
}

/// `out` on standard output whatever happened, beside a failure's card.
fn beside_failure(out: String, value: &Value) -> Rendered {
    Rendered {
        out,
        err: failed(value).map(|failure| failure.err).unwrap_or_default(),
    }
}

/// The items of a list a reply gives; none when it gives something else.
fn items(list: &Value) -> impl Iterator<Item = &Value> {
    list.as_array().into_iter().flatten()
}

/// The strings of a list a reply gives.
fn strings(list: &Value) -> impl Iterator<Item = &str> {
    items(list).filter_map(Value::as_str)
}

/// The strings of a list a reply gives, safe to print.
fn printable_strings(list: &Value) -> Vec<String> {
    strings(list)
        .map(|item| text::printable(item).into_owned())
        .collect()
}

/// Each `(label, fact)` on a line, labels dimmed and padded to `width`.
fn labelled(rows: &[(&str, String)], width: usize) -> Vec<String> {
    rows.iter()
        .map(|(label, shown)| format!("{}  {shown}", style::dim(format!("{label:<width$}"))))
        .collect()
}

/// `steps`, numbered from 1 and safe to print.
fn numbered(steps: &Value) -> Vec<String> {
    strings(steps)
        .enumerate()
        .map(|(index, step)| format!("  {}. {}", index + 1, text::printable(step)))
        .collect()
}

/// The shared sanitized process list; callers keep their own indentation and streams.
fn remaining_processes(processes: &Value) -> Option<String> {
    let listed: Vec<String> = items(processes)
        .map(|process| {
            format!(
                "{} ({})",
                text::printable(process["name"].as_str().unwrap_or("a process")),
                process["pid"]
            )
        })
        .collect();
    (!listed.is_empty()).then(|| format!("still running, never signalled: {}", listed.join(", ")))
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
    let left_by: Vec<String> = strings(&details["causedBy"]).map(str::to_owned).collect();
    let named = [
        (
            "Held by",
            details["heldBy"]["name"].as_str().map(str::to_owned),
        ),
        (
            "Last used by",
            details["lastUsedBy"]["name"].as_str().map(str::to_owned),
        ),
        ("Left by", Some(text::listed(&left_by))),
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
    items(candidates)
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
pub(crate) fn card(message: &str, code: Option<Code>, uncertain: bool) -> String {
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

/// [`card`] for a failed `result`: keep its supplied next step, or advise
/// on a focus refusal by what was in the way and what was sent first.
pub(super) fn advised_card(message: &str, result: &Value) -> String {
    if let Some(next) = result["next"].as_str() {
        return card_with(message, Some(&text::printable(next)));
    }

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
