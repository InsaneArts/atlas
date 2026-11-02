//! How long a login of the helper takes, step by step. Atlas notes markers
//! in the unified log as a login goes (`com.insanearts.atlas`, category
//! `Login`): the switch, the helper's first-login opener and its daemon.
//! Each is public and names user IDs only, never an account's name or a
//! password. `atlas doctor --login-timing` reads them back beside macOS's
//! own milestones for the same login: loginwindow's, and those of the
//! UserAccountUpdater macOS runs at a login to bring the account up to date.
use crate::error::{Code, Error, Result};
use serde_json::{Value, json};
use std::{collections::BTreeSet, process::Command, sync::OnceLock, time::Duration};

/// The unified log subsystem Atlas's markers go to.
const SUBSYSTEM: &str = "com.insanearts.atlas";
/// The category of the login markers.
const CATEGORY: &str = "Login";

/// Note `marker` in the unified log, where macOS keeps it with its own
/// messages of the same moment.
pub fn mark(marker: &str) {
    // Tests never write to this Mac's log.
    if cfg!(test) {
        return;
    }

    static LOG: OnceLock<oslog::OsLog> = OnceLock::new();
    LOG.get_or_init(|| oslog::OsLog::new(SUBSYSTEM, CATEGORY))
        .default(marker);
}

/// Seconds as a marker writes them: `12.3s`.
pub fn seconds(duration: std::time::Duration) -> String {
    format!("{:.1}s", duration.as_secs_f64())
}

/// How far back `atlas doctor --login-timing` looks.
const LOOK_BACK: &str = "2h";

/// What `log show` is asked for: Atlas's markers, loginwindow's milestones
/// and the account updater's lines.
const PREDICATE: &str = "(subsystem == \"com.insanearts.atlas\" AND category == \"Login\") OR \
     (process == \"loginwindow\" AND category == \"KeyMilestone\") OR \
     process == \"UserAccountUpdater\"";

/// Maximum retained bytes in each OS-log query stream, not a reply-size budget.
const OUTPUT_LIMIT: usize = 8 * 1024 * 1024;
/// The command's execution deadline; owned pipe draining and cleanup follow it.
const QUERY_LIMIT: Duration = Duration::from_secs(30);

/// The last login of the past two hours, step by step, from the unified
/// log.
///
/// # Errors
///
/// Returns an I/O failure if the query cannot start or complete within thirty seconds,
/// either stream exceeds eight MiB, pipe reading or child cleanup fails, or the tool
/// exits unsuccessfully. No partial timeline or raw system-log error text is returned.
/// This read-only query does not change helper input or acknowledge pending work.
pub fn timing() -> Result<Value> {
    let mut command = Command::new("/usr/bin/log");
    command
        .args([
            "show",
            "--last",
            LOOK_BACK,
            "--style",
            "ndjson",
            "--predicate",
        ])
        .arg(PREDICATE);
    extract(&mut command, QUERY_LIMIT, OUTPUT_LIMIT)
}

/// Decode only a complete successful query. The command seam keeps fixtures out
/// of the system log and permits exercising the actual owned process/pipe path.
fn extract(command: &mut Command, limit: Duration, cap: usize) -> Result<Value> {
    let failed = || Error::new(Code::Io, "macOS could not read its log");
    let output = crate::bounded::output_with_limit(command, limit, cap)
        .map_err(|cause| failed().with_source(cause))?;
    if !output.status.success() {
        // The exit status is actionable; arbitrary OS-log stderr is private content,
        // not a diagnostic. Retain the status without copying either output stream.
        return Err(failed().with_source(std::io::Error::other(format!(
            "log show exited with {}",
            output.status
        ))));
    }

    let events: Vec<Event> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(event)
        .collect();

    Ok(summarize(&events))
}

/// Who wrote a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    Atlas,
    LoginWindow,
    Updater,
}

/// One line of the unified log.
#[derive(Debug, Clone, PartialEq)]
struct Event {
    /// Seconds since 1970.
    at: f64,
    source: Source,
    pid: u64,
    message: String,
}

/// One `log show --style ndjson` line as an [`Event`], when it is one.
fn event(line: &str) -> Option<Event> {
    let value: Value = serde_json::from_str(line).ok()?;
    let source = if value["subsystem"] == SUBSYSTEM {
        Source::Atlas
    } else {
        match value["processImagePath"].as_str()? {
            image if image.ends_with("/UserAccountUpdater") => Source::Updater,
            image if image.ends_with("/loginwindow") => Source::LoginWindow,
            _ => return None,
        }
    };

    Some(Event {
        at: timestamp(value["timestamp"].as_str()?)?,
        source,
        pid: value["processID"].as_u64().unwrap_or(0),
        message: value["eventMessage"].as_str()?.to_owned(),
    })
}

/// `2026-10-23 20:50:04.343478+0400` as seconds since 1970.
fn timestamp(text: &str) -> Option<f64> {
    let (date, rest) = text.split_once(' ')?;
    let mut parts = date.split('-').map(str::parse::<i64>);
    let (year, month, day) = (
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    );

    let zone_at = rest.rfind(['+', '-'])?;
    let (clock, zone) = rest.split_at(zone_at);
    let mut clock = clock.split(':');
    let hours: f64 = clock.next()?.parse().ok()?;
    let minutes: f64 = clock.next()?.parse().ok()?;
    let seconds: f64 = clock.next()?.parse().ok()?;

    let sign = if zone.starts_with('-') { -1.0 } else { 1.0 };
    let zone = zone.get(1..)?;
    let offset = sign
        * (zone.get(..2)?.parse::<f64>().ok()? * 3600.0
            + zone.get(2..4)?.parse::<f64>().ok()? * 60.0);

    let days = days_from_civil(year, month, day) as f64;
    Some(days * 86_400.0 + hours * 3600.0 + minutes * 60.0 + seconds - offset)
}

/// Days from 1970-01-01 to a date of the proleptic Gregorian calendar.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let of_era_days = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    era * 146_097 + of_era_days - 719_468
}

/// The first event after `from` that `wanted` accepts.
fn first_after(events: &[Event], from: f64, wanted: impl Fn(&Event) -> bool) -> Option<&Event> {
    events
        .iter()
        .filter(|event| event.at >= from)
        .find(|event| wanted(event))
}

/// Seconds from one event to another, when both are known.
fn between(from: Option<&Event>, to: Option<&Event>) -> Option<f64> {
    Some(tenths(to?.at - from?.at))
}

/// `seconds` to a tenth of a second.
fn tenths(seconds: f64) -> f64 {
    (seconds * 10.0).round() / 10.0
}

/// The plugin, its phase and its seconds in `UAU Plugin X Root phase
/// completed in 0.000005 seconds.`.
fn completed(message: &str) -> Option<(&str, &str, f64)> {
    let rest = message.strip_prefix("UAU Plugin ")?;
    let (plugin, rest) = rest.split_once(' ')?;
    let (phase, rest) = rest.split_once(" phase completed in ")?;
    let seconds = rest.strip_suffix(" seconds.")?.parse().ok()?;

    Some((plugin, phase, seconds))
}

/// The plugin in `UAU: Plugin X loaded.`.
fn loaded(message: &str) -> Option<&str> {
    message
        .strip_prefix("UAU: Plugin ")?
        .strip_suffix(" loaded.")
}

/// Where the login to tell starts: the password accepted for a new login
/// (loginwindow goes on to create its session, which unlocking a screen
/// does not). The first one after Atlas last locked the screen for the
/// helper, else the last one.
fn last_login(events: &[Event]) -> Option<&Event> {
    let logins: Vec<&Event> = events
        .iter()
        .filter(|event| {
            event.source == Source::LoginWindow
                && event.message.contains("login state: ReturnFromAuthCopy")
                && first_after(events, event.at, |later| {
                    later.source == Source::LoginWindow
                        && later.pid == event.pid
                        && later.message.contains("login state: CreateAndSwitch")
                })
                .is_some()
        })
        .collect();
    let locked = events
        .iter()
        .rev()
        .find(|event| event.source == Source::Atlas && event.message.starts_with("switch.locked"));

    match locked {
        Some(locked) => logins
            .iter()
            .find(|login| login.at >= locked.at)
            .or(logins.last())
            .copied(),
        None => logins.last().copied(),
    }
}

/// The last login in `events`, from the password to Atlas's window: how
/// long each step took, the account updater's slowest plugin, and any of
/// its plugins that never finished.
fn summarize(events: &[Event]) -> Value {
    let Some(password) = last_login(events) else {
        return json!({"ok": true, "found": false});
    };

    let complete = first_after(events, password.at, |event| {
        event.source == Source::LoginWindow
            && event.pid == password.pid
            && event.message.contains("login state: LoginComplete")
    });
    let updater = first_after(events, password.at, |event| event.source == Source::Updater);
    let lines: Vec<&Event> = updater.map_or_else(Vec::new, |first| {
        events
            .iter()
            .filter(|event| event.source == Source::Updater && event.pid == first.pid)
            .collect()
    });
    let finished = lines
        .iter()
        .rev()
        .find(|event| completed(&event.message).is_some())
        .copied();

    let slowest = lines
        .iter()
        .filter_map(|event| completed(&event.message))
        .max_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(plugin, phase, seconds)| {
            json!({"plugin": plugin, "phase": phase, "seconds": seconds})
        });

    let started: BTreeSet<&str> = lines
        .iter()
        .filter_map(|event| loaded(&event.message))
        .collect();
    let done = |wanted: &str| -> BTreeSet<&str> {
        lines
            .iter()
            .filter_map(|event| completed(&event.message))
            .filter(|(_, phase, _)| *phase == wanted)
            .map(|(plugin, _, _)| plugin)
            .collect()
    };
    let (root, user) = (done("Root"), done("User"));
    let unfinished: Vec<&str> = started
        .iter()
        .filter(|plugin| !root.contains(*plugin) || !user.contains(*plugin))
        .copied()
        .collect();

    let marker = |name: &str| {
        first_after(events, password.at, |event| {
            event.source == Source::Atlas && event.message.starts_with(name)
        })
    };
    let desktop = marker("opener.desktop");
    let window = marker("opener.window");
    let assistant = desktop.map(|event| event.message.contains("setupAssistant=true"));
    let markers: Vec<Value> = events
        .iter()
        .filter(|event| event.source == Source::Atlas && event.at >= password.at)
        .map(|event| {
            json!({
                "after": tenths(event.at - password.at),
                "marker": event.message,
            })
        })
        .collect();

    json!({
        "ok": true,
        "found": true,
        "steps": {
            "passwordToUpdater": between(Some(password), updater),
            // An update with a plugin that never finished never ended.
            "updater": if unfinished.is_empty() { between(updater, finished) } else { None },
            "setupAssistant": assistant,
            "loginCompleteToDesktop": between(complete, desktop),
            "desktopToAtlas": between(desktop, window),
            "passwordToDesktop": between(Some(password), desktop),
        },
        "slowestPlugin": slowest,
        "unfinished": unfinished,
        "markers": markers,
    })
}

#[cfg(test)]
mod extraction_tests;

#[cfg(test)]
mod tests;
