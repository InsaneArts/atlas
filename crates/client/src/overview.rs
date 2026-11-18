//! One summary of what is happening in the helper, for the viewer and the
//! menu bar: a mode, a one-sentence reason, recent agent activity and the
//! capability facts a person needs at a glance.
mod activity;
mod sessions;

use crate::host_session::reading::SessionFacts;
pub use activity::{Called, shown, shown_to_person};
pub(crate) use activity::{listed, reason};
pub use sessions::{add_host_sessions, add_sessions_for};

use crate::{
    activity_log::{self, Op, Row},
    client::Client,
    console_user::Named,
};
use atlas_control::Controller;
use atlas_core::{
    desktop::Kind,
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
pub const READ_DEADLINE: Duration = Duration::from_secs(5);
/// How many recent activity rows a status carries.
pub const ROWS: usize = 12;

/// A read of `client`'s desktop that took longer than a status waits,
/// said of that desktop: the helper, or a VM by its name.
pub fn late(client: &Client) -> Error {
    Error::new(
        Code::Transport,
        format!("{} did not answer in time", client.subject()),
    )
}

/// How long the account named at the person's screen is reused.
const CONSOLE_NAME_REUSE: Duration = Duration::from_secs(15);

/// The account macOS names at the person's screen.
static CONSOLE_NAME: Shared<Named> = Shared::new(CONSOLE_NAME_REUSE);

/// The doctor report for the helper as it answers now, with why a silent
/// helper is down. It waits on the helper until `until` at most. `idle` says
/// whether the look at the person's idle time may restart it: only the
/// viewer service's may.
pub async fn report(
    client: &Client,
    until: Instant,
    idle: crate::idle_time::Look,
) -> crate::capability::Doctor {
    let mut doctor = helper_report(client, until, idle).await;
    note_stop(client, &mut doctor);
    doctor
}

/// [`report`] as the helper answers, without the Stop kept on the person's
/// side, which [`note_stop`] adds: a report shared between reads is noted
/// afresh for each.
pub async fn helper_report(
    client: &Client,
    until: Instant,
    idle: crate::idle_time::Look,
) -> crate::capability::Doctor {
    // A VM's maintenance, read from the station's own record before its
    // status is asked, outlives a status that comes too late.
    let maintenance = match client.desktop().kind() {
        Kind::Vm(_) => Some(client.maintenance().said(&client.selected.name)),
        Kind::User(_) => None,
    };
    let reply = tokio::time::timeout_at(until, client.status())
        .await
        .unwrap_or_else(|_| {
            let mut late = json!(late(client));
            if let Some(said) = maintenance.filter(|said| said["state"] != "none") {
                late["station"] = json!({"maintenance": said});
            }
            late
        });
    let mut doctor = match client.desktop().kind() {
        Kind::User(login) => crate::user::overview::user_report(login, reply, until, idle).await,
        Kind::Vm(_) => crate::capability::Doctor::vm(reply, client.unreachable())
            .handing_over(client.handover_unfinished()),
    };
    let status = &mut doctor.diagnosis.value;

    status["consoleName"] = json!(
        CONSOLE_NAME
            .get(nix::unistd::getuid().as_raw(), until, Named::now)
            .await
            .unwrap_or(Named::Unknown)
    );

    doctor
}

/// A Stop kept on the person's side counts as one the helper holds.
pub fn note_stop(client: &Client, doctor: &mut crate::capability::Doctor) {
    let status = &mut doctor.diagnosis.value;
    if client.controller.stop_noted() && status["session"]["inputService"].is_object() {
        status["session"]["inputService"]["held"] = json!(true);
        doctor.diagnosis.observation.native.held = true;
    }
}

/// The controller's mutations of unknown outcome, and how many are pending
/// in all. An unreadable marker is unknown, so it counts as unresolved.
pub fn pending(controller: &Controller) -> (Pending, usize) {
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
pub async fn read(client: &Client) -> Reading {
    let doctor = report(
        client,
        Instant::now() + READ_DEADLINE,
        crate::idle_time::Look::Only,
    )
    .await;
    let (pending, _) = pending(&client.controller);
    let activity = client
        .output_dir()
        .map(|directory| activity_log::recent(directory, ROWS))
        .unwrap_or_default();

    let now = atlas_core::time::timestamp();
    let overview = overview(&doctor, false, pending, &activity, now);
    let mut reading = Reading::new(doctor, overview, false, now);
    if let Some(fields) = reading.value.as_object_mut() {
        fields.remove("consoleName");
        fields.remove("idleTime");
    }
    add_host_sessions(&mut reading, client, now).await;
    reading
}

/// Unknown-outcome mutations: those nothing will settle, and those a live
/// process is still carrying out, commands agents started among them.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pending {
    pub(crate) unresolved: usize,
    pub(crate) in_flight: usize,
    /// How many of those in flight are commands agents started with `run`.
    pub(crate) commands: usize,
}

/// What is happening in the helper, as the overview names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Mode {
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
    fn kind(self, recoverable: bool) -> (State, &'static str) {
        match self {
            Self::Agent => (State::Working, "Working"),
            Self::Observing => (State::Idle, "Idle"),
            Self::Controlling => (State::Control, "You have control"),
            Self::Recovering => (State::NeedsYou, "Needs you"),
            Self::Stopped => (State::Stopped, "Stopped"),
            Self::Blocked if recoverable => (State::NeedsYou, "Needs you"),
            Self::Blocked | Self::LoggedOut | Self::Unavailable => (State::NotReady, "Not ready"),
        }
    }
}

/// The small public state vocabulary; future viewer states remain extensions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Working,
    Idle,
    Control,
    NeedsYou,
    Stopped,
    NotReady,
    Extension(String),
    Empty,
    Missing,
}

impl State {
    pub(crate) fn decode(value: &Value) -> Self {
        match value.as_str() {
            Some("working") => Self::Working,
            Some("idle") => Self::Idle,
            Some("control") => Self::Control,
            Some("needsYou") => Self::NeedsYou,
            Some("stopped") => Self::Stopped,
            Some("notReady") => Self::NotReady,
            Some("") => Self::Empty,
            Some(state) => Self::Extension(state.to_owned()),
            None => Self::Missing,
        }
    }

    pub fn is_string(&self) -> bool {
        !matches!(self, Self::Missing)
    }
}

impl Serialize for State {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Working => "working",
            Self::Idle => "idle",
            Self::Control => "control",
            Self::NeedsYou => "needsYou",
            Self::Stopped => "stopped",
            Self::NotReady => "notReady",
            Self::Extension(state) => state,
            Self::Empty | Self::Missing => "",
        })
    }
}

pub struct Overview {
    mode: Mode,
    state: State,
    pending: Pending,
    agent_holds: bool,
    pub(crate) value: Value,
}

/// A typed local read or one decoded viewer response, with its opaque projection.
pub struct Reading {
    pub value: Value,
    pub ready: bool,
    pub state: State,
    pub(crate) facts: SessionFacts,
    pub sessions: Vec<(String, Value)>,
}

impl Reading {
    pub fn new(
        doctor: crate::capability::Doctor,
        overview: Overview,
        active: bool,
        now: u128,
    ) -> Self {
        let facts = SessionFacts::of(&overview, now);
        let ready = doctor.diagnosis.ready;
        let mut value = doctor.diagnosis.value;
        value["controlActive"] = json!(active);
        value["overview"] = overview.value;
        Self {
            value,
            ready,
            state: overview.state,
            facts,
            sessions: Vec::new(),
        }
    }

    /// Viewer HTTP ingress. Unknown fields and malformed presentation are retained.
    pub fn decode(value: Value) -> Self {
        let overview = &value["overview"];
        let state = State::decode(&overview["state"]);
        let facts = SessionFacts::decode(&value, atlas_core::time::timestamp());
        let sessions = overview["hostSessions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entry| Some((entry["key"].as_str()?.to_owned(), entry.clone())))
            .collect();
        Self {
            ready: value["ready"] == true,
            value,
            state,
            facts,
            sessions,
        }
    }
}

/// The mode and reason for a doctor report plus local controller facts.
/// `lease_active` means some viewer of this server holds control.
pub fn overview(
    report: &crate::capability::Doctor,
    lease_active: bool,
    pending: Pending,
    activity: &[Row],
    now: u128,
) -> Overview {
    let diagnosis = &report.diagnosis;
    let observation = &diagnosis.observation;
    let session = &diagnosis.value["session"];
    let reachable = observation.ok;
    // A VM's activity names it, as its status does.
    let called = match (&report.vm, diagnosis.value["desktop"]["name"].as_str()) {
        (None, _) => activity::Called::Helper,
        (Some(_), Some(name)) => activity::Called::Vm(name),
        (Some(_), None) => activity::Called::UnnamedVm,
    };

    // Recover fixes it: the panel offers the button beside the reason.
    let recoverable = !diagnosis.ready
        && matches!(
            diagnosis.first_step,
            atlas_core::session::Guidance::StartInput
        );

    // The person's own viewer input and explicit recovery are not an agent.
    let recent = activity
        .iter()
        .find(|row| row.actor() != "viewer" && !row.is(&Op::Restart))
        .and_then(|row| row.age(now))
        .is_some_and(|age| age < ACTIVE_NANOS);

    let (mode, reason) = mode(report, lease_active, pending, recent);

    let (state, label) = mode.kind(recoverable);

    // What the agent is doing, or did last, in the words every surface
    // shows; otherwise the reason. A headline is a status, not a sentence,
    // so it ends without a full stop.
    let newest = activity
        .iter()
        .find(|row| row.actor() != "viewer")
        .map(|row| crate::doing::says_in(row, called));
    let headline = match (mode, newest) {
        (Mode::Agent, Some(newest)) => crate::doing::capitalized(&newest),
        (Mode::Observing, Some(newest)) => format!("Last: {newest}"),
        _ => reason.trim_end_matches('.').to_owned(),
    };

    // Background jobs that may still run, as the helper's daemon says.
    // Records the daemon cannot read are counted apart: their jobs may
    // still run, so the person's Stop stays offered. A count that could not
    // read every record says so: more jobs than it counts may run.
    let jobs = match &report.vm {
        Some(_) => vm_jobs(&diagnosis.value["work"]),
        None => helper_jobs(&session["daemon"]["jobs"]),
    };
    let mut value = json!({
        "mode": mode,
        "state": state,
        "label": label,
        "headline": headline,
        "reason": reason,
        "recoverable": mode == Mode::Blocked && recoverable,
        // Recovery waits for a command an agent started to end, so it helps
        // only once none runs.
        "recoveryHelps": pending.commands == 0
            && ((mode == Mode::Recovering && (diagnosis.ready || recoverable))
                || (mode == Mode::Blocked && recoverable)),
        "account": if reachable { &session["user"]["name"] } else { &diagnosis.value["account"] },
        "display": observation.display_valid().then_some(&session["display"]),
        "pending": pending.unresolved,
        "inFlight": pending.in_flight,
        "commands": pending.commands,
        "nativeDialogs": report.native,
        "jobs": jobs,
        // Whether the helper's apps can be heard, as its daemon last said:
        // read with the status, never asked for on its own.
        "sound": session["sound"].is_object().then_some(&session["sound"]),
        "activity": activity
            .iter()
            .map(|row| shown(row, now, called))
            .collect::<Vec<_>>(),
    });
    // A VM's guest says its own Stop, the work it carries, and whether any
    // it started may still run: the person's Stop stays offered for that
    // work while the VM is otherwise not ready.
    value["workMayRun"] = json!(vm_work_may_run(report));
    if report.vm.is_some() {
        let status = &diagnosis.value;
        value["guest"] = json!({"stop": status["stop"], "work": status["work"],
            "leftovers": status["leftovers"]});
    }
    Overview {
        mode,
        state,
        pending,
        agent_holds: observation.native.control_active == Some(true),
        value,
    }
}

/// The helper's background jobs as its daemon counts them, `counted`.
fn helper_jobs(counted: &Value) -> Value {
    let mut jobs = json!({
        "running": counted["running"].as_u64().unwrap_or(0),
        "launching": counted["launching"].as_u64().unwrap_or(0),
        "unresolved": counted["unresolved"].as_u64().unwrap_or(0),
    });
    if counted["incomplete"] == true {
        jobs["incomplete"] = json!(true);
    }
    jobs
}

/// Whether work an agent started on the VM `report` is of may still run
/// while its keeper reaches it and its agents are not stopped already,
/// ready or not: its guest says it runs, or cannot say. Never for Second
/// Login, whose jobs keep Stop offered by their own count.
fn vm_work_may_run(report: &crate::capability::Doctor) -> bool {
    let status = &report.diagnosis.value;
    let may_run = |said: &Value| matches!(said.as_str(), Some("running" | "unknown"));
    matches!(
        report.vm,
        Some(
            crate::capability::VmMode::Maintained(_)
                | crate::capability::VmMode::Unavailable(_)
                | crate::capability::VmMode::Ready
        )
    ) && status["ok"] == true
        && status["keeper"]["live"] == true
        && (may_run(&status["leftovers"]) || may_run(&status["work"]["state"]))
}

/// A VM's background jobs as its guest's `work` counts them: running,
/// starting and being ended alike. A count it cannot tell may hide jobs
/// still running, as an unread record does.
fn vm_jobs(work: &Value) -> Value {
    match work["jobs"].as_u64() {
        Some(running) => json!({"running": running, "launching": 0, "unresolved": 0}),
        None => json!({"running": 0, "launching": 0, "unresolved": 0, "incomplete": true}),
    }
}

/// The mode of the VM `name`, ready for agents, and why, named for it: an
/// outcome its station read as unknown since, an agent acting there now
/// (`acting`) or work in progress, else nothing happening.
fn ready_vm(name: &str, pending: Pending, acting: bool) -> (Mode, String) {
    if pending.unresolved > 0 {
        (
            Mode::Recovering,
            format!("An action's outcome on {name} is unknown; agents wait until it is recovered"),
        )
    } else if acting || pending.in_flight > 0 {
        (Mode::Agent, format!("An agent is acting in {name}"))
    } else {
        (Mode::Observing, format!("Nothing is happening in {name}"))
    }
}

fn mode(
    report: &crate::capability::Doctor,
    lease_active: bool,
    pending: Pending,
    recent: bool,
) -> (Mode, String) {
    // A VM: the person's maintenance first, which no Resume ends, then its
    // guest's own Stop, then whether it is ready.
    match &report.vm {
        Some(crate::capability::VmMode::Maintained(why)) => {
            return (Mode::Unavailable, why.clone());
        }
        Some(crate::capability::VmMode::Stopped) => {
            return (
                Mode::Stopped,
                "You stopped the agent. It waits until you resume it.".to_owned(),
            );
        }
        Some(crate::capability::VmMode::Ready) => {
            let name = report.diagnosis.value["desktop"]["name"]
                .as_str()
                .unwrap_or("the VM");
            let acting = report.diagnosis.observation.native.control_active == Some(true);
            return ready_vm(name, pending, recent || acting);
        }
        Some(crate::capability::VmMode::Unavailable(why)) => {
            return (Mode::Unavailable, why.clone());
        }
        None => {}
    }
    let diagnosis = &report.diagnosis;
    let observation = &diagnosis.observation;
    let reachable = observation.ok;
    let first_step = diagnosis.first_step.text();
    let recoverable = !diagnosis.ready
        && matches!(
            diagnosis.first_step,
            atlas_core::session::Guidance::StartInput
        );
    // A silent helper in the app's words: the panel and viewer offer buttons,
    // never commands.
    if !reachable {
        match &report.absence {
            Some(atlas_core::session::Absence::LoggedOut(_)) => (
                Mode::LoggedOut,
                "The helper is logged out, so agents can't use it".to_owned(),
            ),
            Some(atlas_core::session::Absence::Unreachable(why)) => {
                (Mode::Unavailable, why.clone())
            }
            Some(atlas_core::session::Absence::Missing) => (
                Mode::Unavailable,
                "The helper account no longer exists. Open Getting Started to set up a new one."
                    .to_owned(),
            ),
            _ if observation.code.as_deref() == Some("identity") => (
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
        let reason = if !diagnosis.ready && !recoverable {
            first_step
        } else if observation.native.held {
            "You stopped the agent mid-action, so its outcome is unknown. Recover, then resume."
        } else if pending.commands > 0 {
            "An action's outcome is unknown; recovery waits for the running command to end"
        } else {
            "An action's outcome is unknown; recover before taking control"
        };
        (Mode::Recovering, reason.to_owned())
    } else if observation.native.held && !lease_active && diagnosis.ready {
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
    } else if !diagnosis.ready {
        (Mode::Blocked, first_step.to_owned())
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
    } else if observation.native.control_active == Some(true) || recent || pending.in_flight > 0 {
        (Mode::Agent, "An agent is acting in the helper".to_owned())
    } else {
        (
            Mode::Observing,
            "Nothing is happening in the helper".to_owned(),
        )
    }
}

#[cfg(test)]
#[path = "overview_tests.rs"]
mod tests;
