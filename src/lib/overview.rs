//! One summary of what is happening in the helper, for the viewer and the
//! menu bar: a mode, a one-sentence reason, recent agent activity and the
//! capability facts a person needs at a glance.
mod activity;
mod sessions;

pub(crate) use activity::shown;
pub(crate) use sessions::{
    SessionFacts, SessionReading, add_host_sessions, add_sessions_for, first_command_running,
};

use crate::{
    activity_log::{self, Op, Row},
    client::Client,
    control::Controller,
    error::{Code, Error},
    single_flight::Shared,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::time::Instant;

/// Activity this recent counts as an agent acting now.
const ACTIVE_NANOS: u128 = 5_000_000_000;

/// How long reading the status or a frame may take: less than the app
/// waits (8 seconds), so it hears why rather than giving up.
pub(crate) const READ_DEADLINE: Duration = Duration::from_secs(5);
/// How many recent activity rows a status carries.
pub(crate) const ROWS: usize = 12;

/// A read of the helper that took longer than a status waits.
pub(crate) fn late() -> Error {
    Error::new(Code::Transport, "The helper did not answer in time")
}

/// How long one look at the logins is reused by every status read in this
/// process.
const REUSE: Duration = Duration::from_secs(30);
/// How long one look at why the helper is silent is reused. It asks no
/// directory service, and the app's logged-out mode waits on it.
const ABSENCE_REUSE: Duration = Duration::from_secs(5);

/// The logins Atlas can close, and whether the helper's login is stale.
static LOGINS: Shared<(Vec<u32>, bool)> = Shared::new(REUSE);
/// Why the silent helper is down.
static ABSENCE: Shared<Option<crate::session::Absence>> = Shared::new(ABSENCE_REUSE);

/// The doctor report for the helper as it answers now, with why a silent
/// helper is down. It waits on the helper until `until` at most.
pub(crate) async fn report(client: &Client, until: Instant) -> Value {
    let uid = client.config.helper_uid;
    let reply = tokio::time::timeout_at(until, client.status())
        .await
        .unwrap_or_else(|_| json!(late()));

    // Why a silent helper is down, asked off the runtime (and only then).
    let silent = crate::session::silent(&reply);
    let absence = if silent {
        ABSENCE
            .get(uid, until, move || crate::session::absence(uid))
            .await
            .flatten()
    } else {
        None
    };

    let mut status = crate::capability::doctor(reply, uid);
    if let Some(absence) = &absence {
        absence.explain(&mut status);
    }

    // Logins Atlas can close: deleted accounts' logins, and the helper's own
    // when its desktop is up but Atlas there stays silent. Unknown is none.
    // Staleness is looked at only for a silent helper, so it may show up to
    // one look late.
    let (leftovers, stale) = LOGINS
        .get(uid, until, move || logins(uid, silent))
        .await
        .unwrap_or_default();
    status["leftoverLogins"] = json!(leftovers);
    status["helperStale"] = json!(silent && stale);

    // A Stop kept on the person's side counts as one the helper holds.
    if client.controller.stop_noted() && status["session"]["inputService"].is_object() {
        status["session"]["inputService"]["held"] = json!(true);
    }

    status
}

/// The logins of deleted accounts, and, when `silent`, whether helper
/// `uid`'s own login is stale.
fn logins(uid: u32, silent: bool) -> (Vec<u32>, bool) {
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
}

/// The controller's mutations of unknown outcome, and how many are pending
/// in all. An unreadable marker is unknown, so it counts as unresolved.
pub(crate) fn pending(controller: &Controller) -> (Pending, usize) {
    let (unresolved, total, commands) = controller
        .pending_snapshot_within(Duration::from_millis(50))
        .map_or((1, 1, 0), |snapshot| {
            (snapshot.unresolved.len(), snapshot.total, snapshot.commands)
        });
    let pending = Pending {
        unresolved,
        in_flight: total.saturating_sub(unresolved),
        commands,
    };

    (pending, total)
}

/// The helper's status and overview as an agent app shows them, read
/// without a viewer: no viewer of this process holds control.
pub(crate) async fn read(client: &Client) -> Value {
    let mut status = report(client, Instant::now() + READ_DEADLINE).await;
    let (pending, _) = pending(&client.controller);
    let activity = client
        .output_dir()
        .map(|directory| activity_log::recent(directory, ROWS))
        .unwrap_or_default();

    status["controlActive"] = json!(false);
    let now = crate::client::timestamp();
    status["overview"] = overview(&status, false, pending, &activity, now);
    add_host_sessions(&mut status, client, now).await;
    status
}

/// Unknown-outcome mutations: those nothing will settle, and those a live
/// process is still carrying out, commands agents started among them.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Pending {
    pub(crate) unresolved: usize,
    pub(crate) in_flight: usize,
    /// How many of those in flight are commands agents started with `run`.
    pub(crate) commands: usize,
}

/// What is happening in the helper, as the overview names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum Mode {
    /// An agent is acting, or work it started is in progress.
    Agent,
    /// Nothing is happening.
    Observing,
    /// The person has control.
    Controlling,
    /// An outcome is unknown until the person recovers.
    Recovering,
    /// The person stopped agents.
    Stopped,
    /// The helper answers but agents can't use it yet.
    Blocked,
    /// The helper is logged out.
    LoggedOut,
    /// The helper does not answer, or another account answered for it.
    Unavailable,
}

impl Mode {
    /// The state every agent app shows for this mode, and its label: an
    /// agent working, nothing happening, the person in control, something
    /// only the person or recovery can settle, or a helper agents can't use
    /// yet.
    fn kind(self, recoverable: bool) -> (&'static str, &'static str) {
        match self {
            Self::Agent => ("working", "Working"),
            Self::Observing => ("idle", "Idle"),
            Self::Controlling => ("control", "You have control"),
            Self::Recovering => ("needsYou", "Needs you"),
            Self::Stopped => ("stopped", "Stopped"),
            Self::Blocked if recoverable => ("needsYou", "Needs you"),
            Self::Blocked | Self::LoggedOut | Self::Unavailable => ("notReady", "Not ready"),
        }
    }
}

/// The mode and reason for a doctor report plus local controller facts.
/// `lease_active` means some viewer of this server holds control.
pub(crate) fn overview(
    report: &Value,
    lease_active: bool,
    pending: Pending,
    activity: &[Row],
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
        .find(|row| row.actor() != "viewer" && !row.is(&Op::Restart))
        .and_then(|row| row.age(now))
        .is_some_and(|age| age < ACTIVE_NANOS);

    // A silent helper in the app's words: the panel and viewer offer buttons,
    // never commands.
    let (mode, reason) = if !reachable {
        match report["absence"].as_str() {
            Some("loggedOut") => (
                Mode::LoggedOut,
                "The helper is logged out, so agents can't use it".to_owned(),
            ),
            Some("missing") => (
                Mode::Unavailable,
                "The helper account no longer exists. Open Getting Started to set up a new one."
                    .to_owned(),
            ),
            _ if session["code"] == "identity" => (
                Mode::Unavailable,
                "Another account answered for the helper. Open Getting Started to check it."
                    .to_owned(),
            ),
            _ => (
                Mode::Unavailable,
                "Atlas isn't answering in the helper. Open Getting Started if this lasts."
                    .to_owned(),
            ),
        }
    } else if pending.unresolved > 0 {
        // A stop that cut an action short leaves its outcome unknown: say
        // both, as recovery comes first and the stop stays after it.
        let reason = if report["ready"] != true && !recoverable {
            first_step.as_str()
        } else if session["inputService"]["held"] == true {
            "You stopped the agent mid-action, so its outcome is unknown. Recover, then resume."
        } else if pending.commands > 0 {
            "An action's outcome is unknown; recovery waits for the running command to end"
        } else {
            "An action's outcome is unknown; recover before taking control"
        };
        (Mode::Recovering, reason.to_owned())
    } else if session["inputService"]["held"] == true && !lease_active && report["ready"] == true {
        // The person's stop holds whatever else is true, until they take
        // control themselves: agents wait.
        (
            Mode::Stopped,
            "You stopped the agent. It waits until you resume it.".to_owned(),
        )
    } else if pending.in_flight > 0 && !lease_active {
        // Starting or upgrading input makes readiness briefly false; work in
        // progress is not a reason to offer recovery.
        (Mode::Agent, "Work is in progress in the helper".to_owned())
    } else if recoverable {
        (
            Mode::Blocked,
            "Mouse and keyboard stopped in the helper. Recover starts them again.".to_owned(),
        )
    } else if report["ready"] != true {
        (Mode::Blocked, first_step)
    } else if lease_active && pending.commands > 0 {
        (
            Mode::Controlling,
            "You have control. A command an agent started is still running".to_owned(),
        )
    } else if lease_active {
        (
            Mode::Controlling,
            "You have control. The agent waits".to_owned(),
        )
    } else if session["inputService"]["controlActive"] == true || recent || pending.in_flight > 0 {
        (Mode::Agent, "An agent is acting in the helper".to_owned())
    } else {
        (
            Mode::Observing,
            "Nothing is happening in the helper".to_owned(),
        )
    };

    let (state, label) = mode.kind(recoverable);

    // What the agent is doing, or did last, in the words every surface
    // shows; otherwise the reason. A headline is a status, not a sentence,
    // so it ends without a full stop.
    let newest = activity
        .iter()
        .find(|row| row.actor() != "viewer")
        .map(crate::doing::says);
    let headline = match (mode, newest) {
        (Mode::Agent, Some(newest)) => crate::doing::capitalized(&newest),
        (Mode::Observing, Some(newest)) => format!("Last: {newest}"),
        _ => reason.trim_end_matches('.').to_owned(),
    };

    json!({
        "mode": mode,
        "state": state,
        "label": label,
        "headline": headline,
        "reason": reason,
        "recoverable": mode == Mode::Blocked && recoverable,
        // Recovery waits for a command an agent started to end, so it helps
        // only once none runs.
        "recoveryHelps": pending.commands == 0
            && ((mode == Mode::Recovering && (report["ready"] == true || recoverable))
                || (mode == Mode::Blocked && recoverable)),
        "account": if reachable { &session["user"]["name"] } else { &report["account"] },
        "display": serde::Deserialize::deserialize(&session["display"])
            .ok()
            .filter(crate::session::Display::valid)
            .map(|_: crate::session::Display| &session["display"]),
        "pending": pending.unresolved,
        "inFlight": pending.in_flight,
        "commands": pending.commands,
        "nativeDialogs": report["capabilities"]["nativeDialogs"]["state"],
        // Background jobs that may still run, as the helper's daemon says.
        // Records the daemon cannot read are counted apart: their jobs may
        // still run, so the person's Stop stays offered.
        "jobs": {
            "running": session["daemon"]["jobs"]["running"].as_u64().unwrap_or(0),
            "launching": session["daemon"]["jobs"]["launching"].as_u64().unwrap_or(0),
            "unresolved": session["daemon"]["jobs"]["unresolved"].as_u64().unwrap_or(0),
        },
        "activity": activity.iter().map(|row| shown(row, now)).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests;
