//! How long a login of the helper takes, step by step. Atlas notes markers
//! in the unified log as a login goes (`com.insanearts.atlas`, category
//! `Login`): the switch, the helper's first-login opener and its daemon.
//! Each is public and names user IDs only, never an account's name or a
//! password. `atlas doctor --login-timing` reads them back beside macOS's
//! own milestones for the same login: loginwindow's, and those of the
//! UserAccountUpdater macOS runs at a login to bring the account up to date.
use crate::error::{Error, Result};
use serde_json::{Value, json};
use std::{collections::BTreeSet, process::Command, sync::OnceLock};

/// The unified log subsystem Atlas's markers go to.
pub const SUBSYSTEM: &str = "com.insanearts.atlas";
/// The category of the login markers.
pub const CATEGORY: &str = "Login";

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

/// The last login of the past two hours, step by step, from the unified
/// log.
pub fn timing() -> Result<Value> {
    let output = Command::new("/usr/bin/log")
        .args([
            "show",
            "--last",
            LOOK_BACK,
            "--style",
            "ndjson",
            "--predicate",
        ])
        .arg(PREDICATE)
        .stdin(std::process::Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(Error::new(
            "io",
            format!(
                "macOS could not read its log: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
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
    } else if value["processImagePath"]
        .as_str()?
        .ends_with("/UserAccountUpdater")
    {
        Source::Updater
    } else if value["processImagePath"]
        .as_str()?
        .ends_with("/loginwindow")
    {
        Source::LoginWindow
    } else {
        return None;
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
    let (from, to) = (from?, to?);
    Some(((to.at - from.at) * 10.0).round() / 10.0)
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
                "after": ((event.at - password.at) * 10.0).round() / 10.0,
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
mod tests {
    use super::*;

    fn at(seconds: f64, source: Source, pid: u64, message: &str) -> Event {
        Event {
            at: 1_000.0 + seconds,
            source,
            pid,
            message: message.to_owned(),
        }
    }

    #[test]
    fn a_log_line_is_read_with_its_time_zone() {
        let line = r#"{"subsystem":"com.apple.loginwindow.logging","processImagePath":"\/System\/Library\/CoreServices\/loginwindow.app\/Contents\/MacOS\/loginwindow","processID":31095,"timestamp":"2026-10-23 20:50:04.250000+0400","eventMessage":"-[SessionStateMonitor setLoginState:] | ************* login state: LoginComplete"}"#;
        let read = event(line).unwrap();
        assert_eq!(read.source, Source::LoginWindow);
        assert_eq!(read.pid, 31095);
        // 2026-10-23 16:50:04.25 UTC.
        assert!((read.at - 1_792_774_204.25).abs() < 1e-6, "{}", read.at);

        let west = timestamp("2026-10-23 20:50:04.250000-0400").unwrap();
        assert!((west - read.at - 8.0 * 3600.0).abs() < 1e-6);
        assert_eq!(timestamp("yesterday"), None);

        let atlas = r#"{"subsystem":"com.insanearts.atlas","processImagePath":"\/Applications\/Atlas.app\/Contents\/MacOS\/atlas","timestamp":"2026-10-23 20:50:05.000000+0400","eventMessage":"opener.start"}"#;
        assert_eq!(event(atlas).unwrap().source, Source::Atlas);
        assert!(event("not json").is_none());
    }

    #[test]
    fn the_last_login_is_told_step_by_step() {
        use Source::{Atlas, LoginWindow, Updater};

        let events = [
            at(-600.0, LoginWindow, 1, "login state: ReturnFromAuthCopy"),
            at(
                0.0,
                LoginWindow,
                7,
                "-[SessionStateMonitor setLoginState:] | ************* login state: ReturnFromAuthCopy",
            ),
            at(0.1, LoginWindow, 7, "login state: CreateAndSwitch"),
            at(0.5, Updater, 40, "UAU Session: -(-)->27.0.1, UID: 503"),
            at(
                0.6,
                Updater,
                40,
                "UAU: Plugin com.apple.dock.DockUpdater loaded.",
            ),
            at(
                0.6,
                Updater,
                40,
                "UAU: Plugin com.apple.dock.SpacesUpdater loaded.",
            ),
            at(
                0.6,
                Updater,
                40,
                "UAU: Plugin com.apple.Safari.UserAccountUpdater opts out of this session.",
            ),
            at(
                0.7,
                Updater,
                40,
                "UAU Plugin com.apple.dock.DockUpdater Root phase completed in 0.000015 seconds.",
            ),
            at(
                0.8,
                Updater,
                40,
                "UAU Plugin com.apple.dock.DockUpdater User phase completed in 0.040805 seconds.",
            ),
            at(
                0.9,
                Updater,
                40,
                "UAU Plugin com.apple.dock.SpacesUpdater Root phase completed in 0.300000 seconds.",
            ),
            at(
                3.0,
                LoginWindow,
                7,
                "-[SessionStateMonitor setLoginState:] | ************* login state: LoginComplete",
            ),
            at(
                4.0,
                Atlas,
                50,
                "opener.desktop waited=3.2s setupAssistant=false",
            ),
            at(6.5, Atlas, 50, "opener.window waited=2.5s"),
        ];
        let summary = summarize(&events);

        assert_eq!(summary["found"], true);
        assert_eq!(summary["steps"]["passwordToUpdater"], 0.5);
        assert_eq!(
            summary["steps"]["updater"],
            Value::Null,
            "an update with a plugin left unfinished never ended"
        );
        assert_eq!(summary["steps"]["setupAssistant"], false);
        assert_eq!(summary["steps"]["loginCompleteToDesktop"], 1.0);
        assert_eq!(summary["steps"]["desktopToAtlas"], 2.5);
        assert_eq!(summary["steps"]["passwordToDesktop"], 4.0);
        assert_eq!(
            summary["slowestPlugin"],
            json!({"plugin": "com.apple.dock.SpacesUpdater", "phase": "Root", "seconds": 0.3})
        );
        assert_eq!(
            summary["unfinished"],
            json!(["com.apple.dock.SpacesUpdater"]),
            "its user phase never finished"
        );
        assert_eq!(summary["markers"][1]["after"], 6.5);
    }

    #[test]
    fn no_login_is_said_plainly_and_missing_steps_are_unknown() {
        use Source::LoginWindow;

        assert_eq!(summarize(&[])["found"], false);

        let unlock = [at(0.0, LoginWindow, 7, "login state: ReturnFromAuthCopy")];
        assert_eq!(
            summarize(&unlock)["found"],
            false,
            "a screen unlocked is not a login"
        );

        let hung = [
            at(0.0, LoginWindow, 7, "login state: ReturnFromAuthCopy"),
            at(0.1, LoginWindow, 7, "login state: CreateAndSwitch"),
            at(0.2, LoginWindow, 7, "login state: UAULaunched"),
        ];
        let summary = summarize(&hung);
        assert_eq!(summary["found"], true);
        assert_eq!(summary["steps"]["passwordToDesktop"], Value::Null);
        assert_eq!(summary["unfinished"], json!([]));
    }

    #[test]
    fn the_login_after_atlas_locked_the_screen_is_the_one_told() {
        use Source::{Atlas, LoginWindow};

        let events = [
            at(0.0, Atlas, 1, "switch.locked helper=503"),
            at(5.0, LoginWindow, 7, "login state: ReturnFromAuthCopy"),
            at(5.1, LoginWindow, 7, "login state: CreateAndSwitch"),
            at(90.0, LoginWindow, 8, "login state: ReturnFromAuthCopy"),
            at(90.1, LoginWindow, 8, "login state: CreateAndSwitch"),
            at(
                95.0,
                Atlas,
                2,
                "opener.desktop waited=1.0s setupAssistant=true",
            ),
        ];
        let summary = summarize(&events);
        assert_eq!(summary["steps"]["passwordToDesktop"], 90.0);
        assert_eq!(summary["steps"]["setupAssistant"], true);
    }
}
