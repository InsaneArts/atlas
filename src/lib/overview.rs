//! One summary of what is happening in the helper, for the viewer and the
//! menu bar: a mode, a one-sentence reason, recent agent activity and the
//! capability facts a person needs at a glance.
use crate::{client::Client, control::Controller, error::Error};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
    time::Duration,
};
use tokio::time::Instant;

/// Activity this recent counts as an agent acting now.
const ACTIVE_NANOS: u128 = 5_000_000_000;
/// How much of the end of the activity log is read at least, and as much
/// more as a row takes at most for each row asked for past that.
const TAIL: u64 = 64 * 1024;
const ROW_BYTES: u64 = 512;

/// How long reading the status or a frame may take: less than the app
/// waits (8 seconds), so it hears why rather than giving up.
pub const READ_DEADLINE: Duration = Duration::from_secs(5);
/// How many recent activity rows a status carries.
pub const ROWS: usize = 12;

/// A read of the helper that took longer than a status waits.
pub(crate) fn late() -> Error {
    Error::new("transport", "The helper did not answer in time")
}

/// The doctor report for the helper as it answers now, with why a silent
/// helper is down. It waits on the helper until `until` at most.
pub async fn report(client: &Client, until: Instant) -> Value {
    let uid = client.config.helper_uid;
    let reply = tokio::time::timeout_at(until, client.status())
        .await
        .unwrap_or_else(|_| json!(late()));

    // Why a silent helper is down, asked off the runtime (and only then).
    let absence = if crate::session::silent(&reply) {
        let asking = tokio::task::spawn_blocking(move || crate::session::absence(uid));
        tokio::time::timeout_at(until, asking)
            .await
            .ok()
            .and_then(Result::ok)
            .flatten()
    } else {
        None
    };

    let silent = crate::session::silent(&reply);
    let mut status = crate::capability::doctor(reply, uid);
    if let Some(absence) = &absence {
        absence.explain(&mut status);
    }

    // Logins Atlas can close: deleted accounts' logins, and the helper's own
    // when its desktop is up but Atlas there stays silent. Unknown is none.
    let looking = tokio::task::spawn_blocking(move || {
        let leftovers = crate::session::sessions(Some(uid))
            .map(|sessions| {
                sessions
                    .leftover_uids(crate::session::Kind::Orphan)
                    .into_iter()
                    .chain(sessions.leftover_uids(crate::session::Kind::HelperLeftover))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        (
            leftovers,
            silent && crate::session::stale(uid) == Some(true),
        )
    });
    let (leftovers, stale) = tokio::time::timeout_at(until, looking)
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
    status["leftoverLogins"] = json!(leftovers);
    status["helperStale"] = json!(stale);

    // A Stop kept on the person's side counts as one the helper holds.
    if client.controller.stop_noted() && status["session"]["inputService"].is_object() {
        status["session"]["inputService"]["held"] = json!(true);
    }

    status
}

/// The controller's mutations of unknown outcome, and how many are pending
/// in all. An unreadable marker is unknown, so it counts as unresolved.
pub fn pending(controller: &Controller) -> (Pending, usize) {
    let total = controller.pending().map_or(1, |pending| pending.len());
    let unresolved = controller.unresolved().map_or(1, |pending| pending.len());
    let pending = Pending {
        unresolved,
        in_flight: total.saturating_sub(unresolved),
    };

    (pending, total)
}

/// The helper's status and overview as an agent app shows them, read
/// without a viewer: no viewer of this process holds control.
pub async fn read(client: &Client) -> Value {
    let mut status = report(client, Instant::now() + READ_DEADLINE).await;
    let (pending, _) = pending(&client.controller);
    let activity = client
        .output_dir()
        .map(|directory| activity(directory, ROWS))
        .unwrap_or_default();

    status["controlActive"] = json!(false);
    status["overview"] = overview(
        &status,
        false,
        pending,
        &activity,
        crate::client::timestamp(),
    );
    status
}

/// One recorded operation per command, newest first; no payloads are ever
/// recorded. A command that acted in several steps (typing in batches,
/// opening then waiting) shows as its newest step.
pub fn activity(directory: &Path, limit: usize) -> Vec<Value> {
    if limit == 0 {
        return Vec::new();
    }

    // Never through a link, and never waiting on a pipe put in its place.
    let Some(mut file) = crate::helper_info::open_plain(&directory.join("metadata.jsonl")) else {
        return Vec::new();
    };

    let tail = TAIL.max(ROW_BYTES.saturating_mul(limit as u64));
    let length = file.metadata().map(|m| m.len()).unwrap_or(0);
    let mut bytes = Vec::new();
    if file
        .seek(SeekFrom::Start(length.saturating_sub(tail)))
        .is_err()
        || file.take(tail).read_to_end(&mut bytes).is_err()
    {
        return Vec::new();
    }

    // The tail may start inside a character; only that first, partial line
    // is lost.
    let mut rows: Vec<Value> = Vec::new();
    for row in String::from_utf8_lossy(&bytes)
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|row| row["operation"].is_string())
    {
        let step = row["command"].is_string()
            && rows
                .last()
                .is_some_and(|newer| newer["command"] == row["command"]);
        if !step {
            rows.push(row);
        }
        if rows.len() == limit {
            break;
        }
    }

    rows
}

/// A logged row's id: the same on every read, and never another row's.
///
/// The log keeps no sequence number, and a row's place in it moves: it is
/// read from its tail, and it starts over once it grows past a megabyte.
/// What a row holds never changes once written, though, and no two rows
/// hold the same: each carries the nanosecond it was written at (taken
/// under the log's exclusive lock, so writers take turns) and, for an
/// agent's command, that command's random token. So the id is a digest of
/// the row itself. A command's newest step stands for it, so the row shown
/// for a command under way is another one, with another id, once it ends.
pub fn row_id(row: &Value) -> String {
    let digest = Sha256::digest(row.to_string().as_bytes());
    digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Nanoseconds since an activity row was recorded, if it parses.
fn age(row: &Value, now: u128) -> Option<u128> {
    row["at"]
        .as_str()?
        .parse::<u128>()
        .ok()
        .map(|at| now.saturating_sub(at))
}

/// Unknown-outcome mutations: those nothing will settle, and those a live
/// process is still carrying out.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pending {
    pub unresolved: usize,
    pub in_flight: usize,
}

/// The state every agent app shows for a mode, and its label: an agent
/// working, nothing happening, the person in control, something only the
/// person or recovery can settle, or a helper agents can't use yet.
fn kind(mode: &str, recoverable: bool) -> (&'static str, &'static str) {
    match mode {
        "agent" => ("working", "Working"),
        "observing" => ("idle", "Idle"),
        "controlling" => ("control", "You have control"),
        "recovering" => ("needsYou", "Needs you"),
        "stopped" => ("stopped", "Stopped"),
        "blocked" if recoverable => ("needsYou", "Needs you"),
        _ => ("notReady", "Not ready"),
    }
}

/// The mode and reason for a doctor report plus local controller facts.
/// `lease_active` means some viewer of this server holds control.
pub fn overview(
    report: &Value,
    lease_active: bool,
    pending: Pending,
    activity: &[Value],
    now: u128,
) -> Value {
    let session = &report["session"];
    let reachable = session["ok"] == true;
    let first_step = report["nextSteps"][0]
        .as_str()
        .unwrap_or("The helper is not ready")
        .to_owned();

    // Recover fixes it: the panel offers the button beside the reason.
    let recoverable = report["ready"] != true && first_step == crate::session::START_INPUT;

    // The person's own viewer input and explicit recovery are not an agent.
    let recent = activity
        .iter()
        .find(|row| row["actor"] != "viewer" && row["operation"] != "restart")
        .and_then(|row| age(row, now))
        .is_some_and(|age| age < ACTIVE_NANOS);

    // A silent helper in the app's words: the panel and viewer offer buttons,
    // never commands.
    let (mode, reason) = if !reachable {
        match report["absence"].as_str() {
            Some("loggedOut") => (
                "loggedOut",
                "The helper is logged out, so agents can't use it".to_owned(),
            ),
            Some("missing") => (
                "unavailable",
                "The helper account no longer exists. Open Getting Started to set up a new one."
                    .to_owned(),
            ),
            _ if session["code"] == "identity" => (
                "unavailable",
                "Another account answered for the helper. Open Getting Started to check it."
                    .to_owned(),
            ),
            _ => (
                "unavailable",
                "Atlas isn't answering in the helper. Open Getting Started if this lasts."
                    .to_owned(),
            ),
        }
    } else if pending.unresolved > 0 {
        // A stop that cut an action short leaves its outcome unknown: say
        // both, as recovery comes first and the stop stays after it.
        let reason = if session["inputService"]["held"] == true {
            "You stopped the agent mid-action, so its outcome is unknown. Recover, then resume."
        } else {
            "An action's outcome is unknown; recover before taking control"
        };
        ("recovering", reason.to_owned())
    } else if session["inputService"]["held"] == true && !lease_active {
        // The person's stop holds whatever else is true, until they take
        // control themselves: agents wait.
        (
            "stopped",
            "You stopped the agent. It waits until you resume it.".to_owned(),
        )
    } else if pending.in_flight > 0 && !lease_active {
        // Starting or upgrading input makes readiness briefly false; work in
        // progress is not a reason to offer recovery.
        ("agent", "Work is in progress in the helper".to_owned())
    } else if recoverable {
        (
            "blocked",
            "Mouse and keyboard stopped in the helper. Recover starts them again.".to_owned(),
        )
    } else if report["ready"] != true {
        ("blocked", first_step)
    } else if lease_active {
        (
            "controlling",
            "You have control. The agent waits".to_owned(),
        )
    } else if session["inputService"]["controlActive"] == true || recent || pending.in_flight > 0 {
        ("agent", "An agent is acting in the helper".to_owned())
    } else {
        ("observing", "Nothing is happening in the helper".to_owned())
    };

    let (state, label) = kind(mode, recoverable);

    // What the agent is doing, or did last, in the words every surface
    // shows; otherwise the reason. A headline is a status, not a sentence,
    // so it ends without a full stop.
    let newest = activity
        .iter()
        .find(|row| row["actor"] != "viewer")
        .map(crate::doing::says);
    let headline = match (state, newest) {
        ("working", Some(newest)) => crate::doing::capitalized(&newest),
        ("idle", Some(newest)) => format!("Last: {newest}"),
        _ => reason.trim_end_matches('.').to_owned(),
    };

    json!({
        "mode": mode,
        "state": state,
        "label": label,
        "headline": headline,
        "reason": reason,
        "recoverable": mode == "blocked" && recoverable,
        "account": if reachable { &session["user"]["name"] } else { &report["account"] },
        "display": serde::Deserialize::deserialize(&session["display"])
            .ok()
            .filter(crate::session::Display::valid)
            .map(|_: crate::session::Display| &session["display"]),
        "pending": pending.unresolved,
        "inFlight": pending.in_flight,
        "nativeDialogs": report["capabilities"]["nativeDialogs"]["state"],
        "activity": activity.iter().map(|row| shown(row, now)).collect::<Vec<_>>(),
    })
}

/// Why a failed action failed, in plain words, from the fixed code its row
/// keeps; nil for an action that worked or a code with no plain reason.
fn reason(row: &Value) -> Option<&'static str> {
    if row["ok"] != false {
        return None;
    }
    Some(match row["code"].as_str()? {
        "busy" => "Something else had the mouse and keyboard",
        "stopped" => "Agents were stopped",
        "elements" => "Nothing on screen matched",
        "windows" => "The window wasn't there",
        "input" => "The mouse and keyboard weren't ready",
        "capture" => "The screen couldn't be read",
        "permission" => match row["operation"].as_str() {
            Some("screenshot") => "The helper can't record the screen",
            Some("input") => "The helper can't use the mouse and keyboard",
            _ => "The helper doesn't have permission",
        },
        "session-changed" => "The helper's screen changed meanwhile",
        "lease-lost" => "Control ended before it finished",
        "cancelled" => "It was cancelled",
        "uncertain" => "It may not have finished",
        "timeout" => "It took too long",
        "exit" => "The command failed",
        "transport" | "identity" | "native" | "daemon" | "on-console" => {
            "Atlas couldn't reach the helper"
        }
        _ => return None,
    })
}

/// A logged row as every app shows it: what it did in words, who did it
/// and how long ago, at `now`.
pub fn shown(row: &Value, now: u128) -> Value {
    json!({
        // The row's own id, the same on every read, so an app can keep a
        // line's identity as newer ones arrive.
        "id": row_id(row),
        "operation": row["operation"],
        // A command under way is not a failure: apps that read only `ok`
        // show it as going well.
        "ok": row["ok"] == true || row["started"] == true,
        "actionCount": row["actionCount"],
        "started": row["started"] == true,
        "verb": row["verb"],
        "app": row["app"],
        "label": row["label"],
        "says": crate::doing::says(row),
        "reason": reason(row),
        "actor": row["actor"].as_str().unwrap_or("agent"),
        "ageSeconds": age(row, now)
            .map(|nanos| u64::try_from(nanos / 1_000_000_000).unwrap_or(u64::MAX)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const NOW: u128 = 100_000_000_000;

    #[test]
    fn every_screenshot_refusal_has_a_plain_reason_in_the_activity_list() {
        for (code, words) in [
            ("capture", "The screen couldn't be read"),
            ("identity", "Atlas couldn't reach the helper"),
            ("on-console", "Atlas couldn't reach the helper"),
            ("permission", "The helper can't record the screen"),
            ("session-changed", "The helper's screen changed meanwhile"),
        ] {
            let row = json!({"operation": "screenshot", "ok": false, "code": code});
            assert_eq!(shown(&row, NOW)["reason"], words, "{code}");
        }
        for (operation, words) in [
            ("input", "The helper can't use the mouse and keyboard"),
            ("run", "The helper doesn't have permission"),
        ] {
            let row = json!({"operation": operation, "ok": false, "code": "permission"});
            assert_eq!(shown(&row, NOW)["reason"], words, "{operation}");
        }
    }

    #[test]
    fn the_activity_log_is_read_only_as_a_plain_file_never_a_link_or_a_pipe() {
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = dir.path().join("elsewhere.jsonl");
        fs::write(&elsewhere, row(1).to_string() + "\n").unwrap();
        std::os::unix::fs::symlink(&elsewhere, dir.path().join("metadata.jsonl")).unwrap();
        assert!(activity(dir.path(), 5).is_empty(), "a link");

        fs::remove_file(dir.path().join("metadata.jsonl")).unwrap();
        nix::unistd::mkfifo(
            &dir.path().join("metadata.jsonl"),
            nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
        )
        .unwrap();
        // A pipe would hold a blocking open forever; this returns at once.
        assert!(activity(dir.path(), 5).is_empty(), "a pipe");
    }

    fn report(reachable: bool, ready: bool) -> Value {
        json!({
            "ready": ready,
            "nextSteps": ["Run atlas start-input"],
            "capabilities": {"nativeDialogs": {"state": "failed"}},
            "session": {"ok": reachable, "user": {"name": "atlastest"},
                "inputService": {"controlActive": false}},
        })
    }

    fn row(seconds_ago: u128) -> Value {
        json!({"at": (NOW - seconds_ago * 1_000_000_000).to_string(),
            "operation": "input", "ok": true, "actionCount": 1})
    }

    fn unresolved(count: usize) -> Pending {
        Pending {
            unresolved: count,
            in_flight: 0,
        }
    }

    fn mode(report: &Value, lease: bool, pending: usize, activity: &[Value]) -> Value {
        overview(report, lease, unresolved(pending), activity, NOW)["mode"].clone()
    }

    #[test]
    fn a_not_ready_helper_shows_only_valid_display_geometry() {
        let mut report = report(true, false);
        for display in [
            json!({"width": 1e100, "height": 1, "scale": 1}),
            json!({"width": 1, "height": 32769, "scale": 1}),
            json!({"width": 0, "height": 1, "scale": 1}),
            json!({"width": 1, "height": 1, "scale": 9}),
            json!({"width": f64::INFINITY, "height": 1, "scale": 1}),
        ] {
            report["session"]["display"] = display;
            let shown = overview(&report, false, unresolved(0), &[], NOW);
            assert_eq!(shown["state"], "notReady");
            assert!(shown["display"].is_null());
        }
        let display = json!({"width": 32768, "height": 1, "scale": 8});
        report["session"]["display"] = display.clone();
        assert_eq!(
            overview(&report, false, unresolved(0), &[], NOW)["display"],
            display
        );
    }

    #[test]
    fn every_mode_has_one_of_the_states_agent_apps_show() {
        let ready = report(true, true);
        let state = |report: &Value, lease: bool, pending: usize, activity: &[Value]| {
            let summary = overview(report, lease, unresolved(pending), activity, NOW);
            (summary["state"].clone(), summary["label"].clone())
        };

        assert_eq!(
            state(&ready, false, 0, &[row(2)]),
            (json!("working"), json!("Working"))
        );
        assert_eq!(state(&ready, false, 0, &[]), (json!("idle"), json!("Idle")));
        assert_eq!(
            state(&ready, true, 0, &[]),
            (json!("control"), json!("You have control"))
        );
        assert_eq!(
            state(&ready, false, 1, &[]),
            (json!("needsYou"), json!("Needs you"))
        );

        let mut stopped = report(true, false);
        stopped["nextSteps"] = json!([crate::session::START_INPUT]);
        assert_eq!(
            state(&stopped, false, 0, &[]).0,
            "needsYou",
            "Recover fixes it"
        );
        assert_eq!(state(&report(true, false), false, 0, &[]).0, "notReady");
        assert_eq!(state(&report(false, false), false, 0, &[]).0, "notReady");
    }

    #[test]
    fn the_headline_is_the_newest_thing_an_agent_did() {
        let ready = report(true, true);
        let mut save = row(2);
        save["verb"] = json!("click");
        save["app"] = json!("TextEdit");
        save["label"] = json!("Save");
        let mut own = row(1);
        own["actor"] = json!("viewer");

        let summary = overview(&ready, false, unresolved(0), &[own, save], NOW);
        assert_eq!(summary["headline"], "Clicked Save in TextEdit");
        assert_eq!(summary["activity"][1]["says"], "clicked Save in TextEdit");
        assert_eq!(summary["activity"][0]["says"], "sent 1 action");

        let quiet = overview(&ready, false, unresolved(0), &[], NOW);
        assert_eq!(quiet["headline"], quiet["reason"], "nothing done yet");

        let unsure = overview(&ready, false, unresolved(1), &[row(2)], NOW);
        assert_eq!(
            unsure["headline"], unsure["reason"],
            "why it needs the person"
        );
    }

    #[test]
    fn a_stop_shows_until_resumed_but_an_unknown_outcome_comes_first() {
        let mut held = report(true, true);
        held["session"]["inputService"]["held"] = json!(true);

        let summary = overview(&held, false, unresolved(0), &[row(1)], NOW);
        assert_eq!(
            (&summary["mode"], &summary["state"], &summary["label"]),
            (&json!("stopped"), &json!("stopped"), &json!("Stopped"))
        );
        assert_eq!(
            summary["headline"],
            "You stopped the agent. It waits until you resume it"
        );
        assert_eq!(
            mode(&held, true, 0, &[]),
            "controlling",
            "the person may take over"
        );
        assert_eq!(mode(&held, false, 1, &[]), "recovering");
    }

    #[test]
    fn an_idle_headline_says_what_was_done_last() {
        let mut save = row(60);
        save["verb"] = json!("click");
        save["label"] = json!("Save");

        let summary = overview(&report(true, true), false, unresolved(0), &[save], NOW);
        assert_eq!(summary["state"], "idle");
        assert_eq!(summary["headline"], "Last: clicked Save");
    }

    #[test]
    fn a_command_of_several_steps_is_one_row() {
        let dir = tempfile::tempdir().unwrap();
        let rows = [
            json!({"operation": "run", "ok": true, "verb": "open", "command": "a"}),
            json!({"operation": "input", "ok": true, "verb": "type", "command": "b"}),
            json!({"operation": "input", "ok": false, "verb": "type", "command": "b"}),
            json!({"operation": "input", "ok": true}),
            json!({"operation": "input", "ok": true}),
        ];
        let log = rows
            .iter()
            .map(|row| format!("{row}\n"))
            .collect::<String>();
        fs::write(dir.path().join("metadata.jsonl"), log).unwrap();

        let shown = activity(dir.path(), 12);
        let says = shown.iter().map(crate::doing::says).collect::<Vec<_>>();
        assert_eq!(
            says,
            [
                "used the mouse and keyboard",
                "used the mouse and keyboard",
                "couldn't type",
                "opened a file"
            ],
            "the newest step stands for its command; rows of no command stay apart"
        );
        assert_eq!(activity(dir.path(), 2).len(), 2);
    }

    #[test]
    fn input_that_recover_restarts_is_offered_as_recover_not_a_command() {
        let mut stopped = report(true, false);
        stopped["nextSteps"] = json!([crate::session::START_INPUT]);
        let summary = overview(&stopped, false, unresolved(0), &[], NOW);
        assert_eq!(summary["mode"], "blocked");
        assert_eq!(summary["recoverable"], true);
        assert!(!summary["reason"].as_str().unwrap().contains("atlas "));

        let mut settings = report(true, false);
        settings["nextSteps"] = json!(["Turn on Accessibility for Atlas in the helper first."]);
        let summary = overview(&settings, false, unresolved(0), &[], NOW);
        assert_eq!(summary["recoverable"], false);
        assert_eq!(summary["reason"], settings["nextSteps"][0]);
    }

    #[test]
    fn modes_follow_reachability_uncertainty_readiness_and_control() {
        let ready = report(true, true);
        assert_eq!(mode(&report(false, false), false, 0, &[]), "unavailable");
        assert_eq!(
            mode(&ready, true, 1, &[]),
            "recovering",
            "uncertainty first"
        );
        assert_eq!(mode(&report(true, false), false, 0, &[]), "blocked");
        assert_eq!(mode(&ready, true, 0, &[]), "controlling");
        assert_eq!(mode(&ready, false, 0, &[row(2)]), "agent");
        assert_eq!(mode(&ready, false, 0, &[row(30)]), "observing");

        let mut own = row(1);
        own["actor"] = json!("viewer");
        let mut restart = row(1);
        restart["operation"] = json!("restart");

        assert_eq!(
            mode(&ready, false, 0, &[own.clone(), restart, row(30)]),
            "observing",
            "the person's input and recovery are not an agent"
        );
        assert_eq!(mode(&ready, false, 0, &[own, row(2)]), "agent");

        let mut native = report(true, true);
        native["session"]["inputService"]["controlActive"] = json!(true);
        assert_eq!(mode(&native, false, 0, &[]), "agent");

        let working = Pending {
            unresolved: 0,
            in_flight: 1,
        };

        assert_eq!(
            overview(&ready, false, working, &[], NOW)["mode"],
            "agent",
            "work in progress is not an unknown outcome"
        );
        assert_eq!(
            overview(&ready, true, working, &[], NOW)["mode"],
            "controlling"
        );
        assert_eq!(
            overview(&report(true, false), false, working, &[], NOW)["mode"],
            "agent",
            "an input start or upgrade in progress is not a blocked helper"
        );

        let blocked = overview(&report(true, false), false, unresolved(0), &[], NOW);
        assert_eq!(blocked["reason"], "Run atlas start-input");
        assert_eq!(
            overview(&ready, false, unresolved(0), &[row(30)], NOW)["activity"][0]["ageSeconds"],
            30
        );
    }

    #[test]
    fn a_silent_helper_is_explained_without_commands() {
        let mut silent = report(false, false);
        silent["absence"] = json!("loggedOut");
        silent["account"] = json!("atlas");
        let logged_out = overview(&silent, false, unresolved(1), &[], NOW);
        assert_eq!(logged_out["mode"], "loggedOut", "before any recovery");
        assert_eq!(logged_out["account"], "atlas");

        silent["absence"] = json!("missing");
        assert_eq!(mode(&silent, false, 0, &[]), "unavailable");

        for absence in [json!("missing"), Value::Null] {
            silent["absence"] = absence;
            let reason = overview(&silent, false, unresolved(0), &[], NOW)["reason"].clone();
            assert!(!reason.as_str().unwrap().contains("atlas "), "{reason}");
        }
    }

    #[test]
    fn activity_reads_the_newest_rows_from_the_log_tail() {
        let dir = tempfile::tempdir().unwrap();
        assert!(activity(dir.path(), 5).is_empty());

        let rows: String = (0..30)
            .map(|i| format!("{{\"at\":\"{i}\",\"operation\":\"input\",\"ok\":true}}\n"))
            .chain(["not json\n".to_owned()])
            .collect();
        fs::write(dir.path().join("metadata.jsonl"), rows).unwrap();
        let newest = activity(dir.path(), 3);
        assert_eq!(newest.len(), 3);
        assert_eq!(newest[0]["at"], "29");
        assert_eq!(newest[2]["at"], "27");
    }

    #[test]
    fn a_tail_that_starts_inside_a_character_still_reads() {
        let dir = tempfile::tempdir().unwrap();
        let mut text = format!(
            "{{\"operation\":\"old\",\"pad\":\"{}\"}}\n{{\"at\":\"1\",\"operation\":\"input\"}}\n",
            "é".repeat(40_000)
        );
        let cut = |text: &str| text.as_bytes()[text.len() - TAIL as usize];
        if cut(&text) & 0xC0 != 0x80 {
            text.push('\n');
        }

        assert_eq!(cut(&text) & 0xC0, 0x80, "the tail starts inside é");
        fs::write(dir.path().join("metadata.jsonl"), text).unwrap();
        let rows = activity(dir.path(), 5);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["at"], "1");
    }
}
