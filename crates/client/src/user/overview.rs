//! The user desktop's part of the overview: its doctor report with what
//! this Mac can tell of it, and the looks it reuses.
use atlas_core::single_flight::Shared;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::time::Instant;

/// How long one look at the logins is reused by every status read in this
/// process.
const REUSE: Duration = Duration::from_secs(30);

/// How long one look at why the helper is silent is reused. It asks no
/// directory service, and the app's logged-out mode waits on it.
const ABSENCE_REUSE: Duration = Duration::from_secs(5);

/// How long one read of launchd's log is reused: reading it takes a moment,
/// and what it says changes over hours.
const RESTARTS_REUSE: Duration = Duration::from_secs(10 * 60);

/// How long one look at the idle time apps read is reused: the person's
/// apps lock only after minutes of it stopped.
const IDLE_TIME_REUSE: Duration = Duration::from_secs(15);

/// The logins Atlas can close, and whether the helper's login is stale.
static LOGINS: Shared<(Vec<u32>, bool)> = Shared::new(REUSE);

/// The Apple jobs restarting in the helper, from launchd's log.
static RESTARTS: Shared<Option<Vec<crate::console_agents::Restart>>> = Shared::new(RESTARTS_REUSE);

/// Why the silent helper is down.
static ABSENCE: Shared<Option<atlas_core::session::Absence>> = Shared::new(ABSENCE_REUSE);

/// Whether the idle time apps read has stopped in this login.
static IDLE_TIME: Shared<crate::idle_time::IdleTime> = Shared::new(IDLE_TIME_REUSE);

/// The doctor report for the second login `login` from its status `reply`,
/// with what this Mac can tell of it: why it is silent, the logins Atlas
/// can close, whether an Apple agent keeps restarting in it, and whether
/// switching to and from it has stopped the idle time the person's apps
/// read, which `idle` says may be restarted.
pub(crate) async fn user_report(
    login: &atlas_core::user::User,
    reply: Value,
    until: Instant,
    idle: crate::idle_time::Look,
) -> crate::capability::Doctor {
    let uid = login.uid;
    // Why a silent helper is down, asked off the runtime (and only then).
    let silent = atlas_core::session::silent(&reply);
    let absence = if silent {
        ABSENCE
            .get(uid, until, move || atlas_core::user::logins::absence(uid))
            .await
            .flatten()
    } else {
        None
    };

    let answered = crate::console_agents::answered(&reply);
    let mut doctor = crate::user::capability::doctor_report(reply, login);
    let status = &mut doctor.diagnosis.value;
    if let Some(absence) = &absence {
        absence.explain(status);
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
    // Whether a macOS agent that slows the Mac keeps restarting in the
    // helper, from launchd's log; looked at only when the helper itself
    // answered, and at most once in ten minutes.
    let restarts = if !answered {
        None
    } else {
        RESTARTS
            .get(uid, until, move || {
                crate::user::console_agents::read_restarting(uid)
            })
            .await
            .flatten()
    };
    status["restartingService"] = json!(crate::console_agents::Restarting::of(restarts.as_deref()));

    // Whether the idle time the person's apps read has stopped while they
    // type, as a switch to or from the second login can leave it, restarted
    // when `idle` allows. This process runs in the person's login.
    let idle_time = IDLE_TIME
        .get(uid, until, move || crate::idle_time::IdleTime::now(idle))
        .await
        .unwrap_or(crate::idle_time::IdleTime::Unknown);
    status["idleTime"] = json!(idle_time);

    doctor.absence = absence;
    doctor
}

/// The logins of deleted accounts, and, when `silent`, whether helper
/// `uid`'s own login is stale.
fn logins(uid: u32, silent: bool) -> (Vec<u32>, bool) {
    let leftovers = atlas_core::user::logins::sessions(Some(uid))
        .map(|sessions| {
            sessions
                .leftover_uids(atlas_core::user::logins::Kind::Orphan)
                .into_iter()
                .chain(sessions.leftover_uids(atlas_core::user::logins::Kind::HelperLeftover))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    (
        leftovers,
        silent && atlas_core::user::logins::stale(uid) == Some(true),
    )
}

#[cfg(test)]
#[path = "overview_tests.rs"]
mod tests;
