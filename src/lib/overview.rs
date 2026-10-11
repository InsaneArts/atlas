//! One summary of what is happening in the helper, for the viewer and the
//! menu bar: a mode, a one-sentence reason, recent agent activity and the
//! capability facts a person needs at a glance.
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

/// Activity this recent counts as an agent acting now.
const ACTIVE_NANOS: u128 = 5_000_000_000;
/// How much of the end of the activity log is read.
const TAIL: u64 = 64 * 1024;

/// One recorded operation, newest first; no payloads are ever recorded.
pub fn activity(directory: &Path, limit: usize) -> Vec<Value> {
    let Ok(mut file) = fs::File::open(directory.join("metadata.jsonl")) else {
        return Vec::new();
    };
    let length = file.metadata().map(|m| m.len()).unwrap_or(0);
    let mut text = String::new();
    if file
        .seek(SeekFrom::Start(length.saturating_sub(TAIL)))
        .is_err()
        || file.take(TAIL).read_to_string(&mut text).is_err()
    {
        return Vec::new();
    }
    text.lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|row| row["operation"].is_string())
        .take(limit)
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
    // The person's own viewer input and explicit recovery are not an agent.
    let recent = activity
        .iter()
        .find(|row| row["actor"] != "viewer" && row["operation"] != "restart")
        .and_then(|row| age(row, now))
        .is_some_and(|age| age < ACTIVE_NANOS);
    let (mode, reason) = if !reachable {
        ("unavailable", first_step)
    } else if pending.unresolved > 0 {
        (
            "recovering",
            "An action's outcome is unknown; recover before taking control".to_owned(),
        )
    } else if pending.in_flight > 0 && !lease_active {
        // Starting or upgrading input makes readiness briefly false; work in
        // progress is not a reason to offer recovery.
        ("agent", "Work is in progress in the helper".to_owned())
    } else if report["ready"] != true {
        ("blocked", first_step)
    } else if lease_active {
        (
            "controlling",
            "A viewer has mouse and keyboard control".to_owned(),
        )
    } else if session["inputService"]["controlActive"] == true || recent || pending.in_flight > 0 {
        ("agent", "An agent is acting in the helper".to_owned())
    } else {
        ("observing", "Watching the helper desktop".to_owned())
    };
    json!({
        "mode": mode,
        "reason": reason,
        "account": session["user"]["name"],
        "display": session["display"],
        "pending": pending.unresolved,
        "inFlight": pending.in_flight,
        "nativeDialogs": report["capabilities"]["nativeDialogs"]["state"],
        "activity": activity
            .iter()
            .map(|row| json!({
                "operation": row["operation"],
                "ok": row["ok"],
                "actionCount": row["actionCount"],
                "actor": row["actor"].as_str().unwrap_or("agent"),
                "ageSeconds": age(row, now).map(|nanos| (nanos / 1_000_000_000) as u64),
            }))
            .collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u128 = 100_000_000_000;

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
}
