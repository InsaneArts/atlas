//! `atlas activity` as a person reads it: one session's work in a few lines,
//! or one line for each session. Words only; keys only with `--json`.
use super::{style, text};
use crate::host_session::names::minute;
use serde_json::Value;

/// One line for each task the session named, with what it left.
fn tasks(tasks: &Value) -> Vec<String> {
    tasks
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|task| {
            let name = text::printable(task["name"].as_str()?).into_owned();
            let left = &task["leftovers"];
            let count = |field: &str, one: &str, many: &str| {
                left[field]
                    .as_u64()
                    .filter(|count| *count > 0)
                    .map(|count| match count {
                        1 => format!("1 {one}"),
                        count => format!("{count} {many}"),
                    })
            };
            let parts: Vec<String> = [
                count("copies", "open copy", "open copies"),
                count("plans", "environment plan", "environment plans"),
            ]
            .into_iter()
            .flatten()
            .collect();
            let left = if parts.is_empty() {
                "nothing left".to_owned()
            } else {
                format!("left {}", parts.join(" and "))
            };
            Some(format!("  Task {name}: {left}"))
        })
        .collect()
}

/// What a host session did, or one line for each session.
pub fn activity(value: &Value) -> String {
    match value["sessions"].as_array() {
        Some(sessions) => everyone(sessions),
        None => one(value),
    }
}

/// One session: who it is, what it did by verb, what it left and its last
/// action.
fn one(value: &Value) -> String {
    let session = &value["session"];
    let mut heading =
        text::printable(session["name"].as_str().unwrap_or("This session")).into_owned();
    let since = session["started"].as_u64().and_then(minute);
    match (session["this"] == true, since) {
        (true, Some(since)) => heading.push_str(&format!(" — this session, since {since}")),
        (true, None) => heading.push_str(" — this session"),
        (false, Some(since)) => heading.push_str(&format!(" — since {since}")),
        (false, None) => {}
    }

    let mut lines = vec![style::bold(heading)];
    let counts = counted(&value["counts"]);
    lines.push(format!(
        "  {}",
        if counts.is_empty() {
            "Nothing done in the helper yet".to_owned()
        } else {
            counts.join(" · ")
        }
    ));

    let copies: Vec<&str> = value["copies"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|copy| copy["app"].as_str().unwrap_or("an app"))
        .collect();
    if !copies.is_empty() {
        lines.push(format!(
            "  Open copies: {}",
            text::printable(&copies.join(", "))
        ));
    }

    let plans: Vec<String> = value["plans"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|plan| {
            let id = plan["planId"].as_str()?;
            let phase = phase_word(plan["phase"].as_str()?);
            Some(format!("{}… {phase}", id.get(..4)?))
        })
        .collect();
    if !plans.is_empty() {
        lines.push(format!("  Environment plans: {}", plans.join(", ")));
    }

    lines.extend(tasks(&value["tasks"]));

    if let Some(unknown) = value["uncertain"].as_u64().filter(|count| *count > 0) {
        let count = text::count(
            usize::try_from(unknown).unwrap_or(usize::MAX),
            "unknown action",
        );
        let whose = if session["this"] == true {
            " of this session"
        } else {
            ""
        };
        // The step to recover comes only with the reply's own, which is given
        // only when every unknown action is this session's.
        let step = if value["next"].is_string() {
            format!(
                ": take a screenshot, then run {}",
                style::cmd("atlas restart")
            )
        } else {
            String::new()
        };
        lines.push(format!("  {} {count}{whose}{step}", style::warn()));
    }

    if let Some(last) = value["recent"].as_array().and_then(|recent| recent.first()) {
        lines.push(format!("  Last: {}", said(last)));
    }

    let previous = &value["previous"];
    if let Some(name) = previous["name"].as_str() {
        lines.push(style::dim(format!(
            "  The session before it, {}, left work in the helper",
            text::printable(name)
        )));
    }

    lines.join("\n")
}

/// One line for each session: its name, how long ago it last acted, and
/// what it left when it left anything.
fn everyone(sessions: &[Value]) -> String {
    if sessions.is_empty() {
        return format!("{} No agent session is recorded here", style::unknown());
    }

    let now = crate::host_session::now_ms();
    sessions
        .iter()
        .map(|session| {
            let mut line =
                text::printable(session["name"].as_str().unwrap_or("A session")).into_owned();
            if let Some(seen) = session["lastSeen"].as_u64() {
                line.push_str(&format!(", {} ago", age(now.saturating_sub(seen) / 1000)));
            }
            if session["holdsHelper"] == true {
                line.push_str(", has control now");
            }
            let left = &session["leftovers"];
            let parts: Vec<String> = [
                ("copies", "open copy", "open copies"),
                ("plans", "environment plan", "environment plans"),
                ("uncertain", "unknown action", "unknown actions"),
            ]
            .into_iter()
            .filter_map(|(key, one, many)| {
                let count = left[key].as_u64().filter(|count| *count > 0)?;
                Some(if count == 1 {
                    format!("1 {one}")
                } else {
                    format!("{count} {many}")
                })
            })
            .collect();
            if !parts.is_empty() {
                line.push_str(&style::dim(format!(" — left {}", text::listed(&parts))));
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A recent action as said: "took a screenshot, 40 s ago".
fn said(row: &Value) -> String {
    let says = text::printable(row["says"].as_str().unwrap_or("did something")).into_owned();
    match row["ageSeconds"].as_u64() {
        Some(seconds) => format!("{says}, {} ago", age(seconds)),
        None => says,
    }
}

/// Seconds as people say an age: "40 s", "5 min", "3 h", "2 days".
fn age(seconds: u64) -> String {
    match seconds {
        0..60 => format!("{seconds} s"),
        60..3600 => format!("{} min", seconds / 60),
        3600..86_400 => format!("{} h", seconds / 3600),
        _ => text::count(
            usize::try_from(seconds / 86_400).unwrap_or(usize::MAX),
            "day",
        ),
    }
}

/// A session's counts by verb, in words: "4 clicks", "2 typed".
fn counted(counts: &Value) -> Vec<String> {
    counts
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(verb, count)| Some((verb.as_str(), count.as_u64()?)))
        .map(|(verb, count)| {
            let (one, many) = verb_words(verb);
            if count == 1 {
                format!("1 {one}")
            } else {
                format!("{count} {many}")
            }
        })
        .collect()
}

/// A verb's words for one and for more.
fn verb_words(verb: &str) -> (&str, String) {
    match verb {
        "type" => ("typed", "typed".to_owned()),
        "key" => ("key press", "key presses".to_owned()),
        "open-link" => ("link opened", "links opened".to_owned()),
        "environment" => ("environment change", "environment changes".to_owned()),
        verb => (verb, format!("{verb}s")),
    }
}

/// How far a plan got, in words.
fn phase_word(phase: &str) -> &str {
    match phase {
        "applying" => "applying",
        "applied" => "applied",
        "rollingBack" => "rolling back",
        "rolledBack" => "rolled back",
        _ => "started",
    }
}
