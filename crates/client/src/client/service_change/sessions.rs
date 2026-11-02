use super::{Client, MARKER_CHECK};
use crate::{
    activity_log,
    host_session::{
        liveness::Snapshot,
        names::{name_of, names_with_left},
        reading::{SESSION_ROWS, SessionFacts, SessionReading, unrecorded},
        store::Store,
    },
    session_view,
};
use atlas_core::process::State;

/// How recently another session used Atlas to defer service replacement.
const SERVICE_IDLE: std::time::Duration = std::time::Duration::from_secs(2 * 60);

/// Who would be interrupted by a service replacement, from checked session
/// records and the full recent activity window. The caller's own session
/// is left out for recent activity. Its waiting and running commands still defer
/// replacement; the service change's own holder note does not. Recovering the
/// caller's unknown outcome ignores recent activity and waiters, which cannot
/// act until recovery finishes.
pub(crate) fn service_blocker(client: &Client, recovery: bool, now: u128) -> Option<String> {
    service_blocker_with(client, recovery, now, Snapshot::polled)
}

pub(crate) fn service_blocker_with(
    client: &Client,
    recovery: bool,
    now: u128,
    sample: impl FnOnce(&std::path::Path, Vec<atlas_core::process::Instance>) -> Snapshot,
) -> Option<String> {
    let Ok(output) = client.output_dir() else {
        return Some("Atlas could not check which sessions are active.".into());
    };
    let Some(seen) = Store::at(output).checked_recent() else {
        return Some(
            "Atlas could not read every session record; another session may still be active."
                .into(),
        );
    };
    // Unlike a display read, replacement checks the lock even with no marker:
    // a writer may be paused just before recording its first pending change.
    let causes = match client
        .controller
        .pending_causes_for_service_change(MARKER_CHECK)
    {
        Ok(causes) => causes,
        Err(error) if error.code == atlas_core::error::Code::Busy => return Some(error.error),
        Err(error) => {
            return Some(format!(
                "Atlas could not check pending activity: {}",
                error.error
            ));
        }
    };
    let own = client
        .host_session
        .as_ref()
        .map(|session| session.key().as_str());
    let pending = causes.iter().any(|cause| !cause.running);
    let recovering_own = recovery
        && pending
        && causes
            .iter()
            .all(|cause| cause.running || cause.session.as_ref().map(|key| key.as_str()) == own);
    let waiting = if recovering_own {
        Vec::new()
    } else if let Some(waiting) = client.controller.checked_waiting_notes() {
        waiting
    } else {
        return Some(
            "Atlas could not read every waiting command; a command may still be active.".into(),
        );
    };
    let holder = client.controller.holder_note().filter(|note| {
        !(note.process.is_this_one() && note.session.as_ref().map(|key| key.as_str()) == own)
    });
    let facts = SessionFacts {
        pending,
        agent_holds: false,
        now,
    };
    let since = milliseconds(now).saturating_sub(atlas_core::time::ms(SERVICE_IDLE));
    let Some(activity) = activity_log::checked_since(output, since) else {
        return Some(
            "Atlas could not read every activity row; another session may still be active.".into(),
        );
    };
    let rows = activity_log::recent(output, SESSION_ROWS);
    let mut reading = SessionReading {
        seen: seen.into(),
        rows,
        causes,
        holder,
        waiting,
        running: client.controller.running_candidates(),
        processes: Snapshot::default(),
    }
    .sample(output, sample);
    let remains = |note: &atlas_control::Note| reading.processes.state(note.process) != State::Gone;
    reading.holder = reading.holder.filter(remains);
    reading.waiting.retain(remains);
    reading.running.retain(remains);
    blocker(&reading, facts, own, recovering_own, &activity)
}

/// Select from the single frozen read; unknown process samples stay busy even
/// when the display would omit them as irrelevant.
fn blocker(
    reading: &SessionReading,
    facts: SessionFacts,
    own: Option<&str>,
    recovering_own: bool,
    activity: &[activity_log::Row],
) -> Option<String> {
    let now_ms = milliseconds(facts.now);
    let recent =
        |last_seen: u64| now_ms.saturating_sub(last_seen) <= atlas_core::time::ms(SERVICE_IDLE);
    let active = |name: &str| format!("{name} is active in the helper.");

    if reading
        .holder
        .iter()
        .chain(&reading.running)
        .any(|note| reading.processes.state(note.process) == State::Unknown)
    {
        return Some(
            "Atlas could not check whether a command is still active in the helper.".into(),
        );
    }
    if let Some(note) = reading.waiting.first() {
        let first = unrecorded(None, &reading.waiting, &[], &reading.seen);
        let names = names_with_left(
            reading.seen.iter().map(|seen| &seen.record),
            [],
            first.iter().copied(),
        );
        let name = note.session.as_ref().map_or_else(
            || session_view::TERMINAL.to_owned(),
            |key| name_of(&names, key, note.host),
        );
        return Some(active(&name));
    }

    let busy = reading.live(facts).into_iter().find(|listed| {
        let key = (!listed.key.is_empty()).then_some(listed.key.as_str());
        listed.shown.holds_helper
            || listed.running_command
            || (!recovering_own && key != own && listed.ended.is_none() && recent(listed.last_seen))
    });
    if let Some(listed) = busy {
        let name = &listed.name;
        return Some(if listed.running_command {
            format!("{name} has a command still running in the helper.")
        } else {
            active(name)
        });
    }
    if recovering_own {
        return None;
    }

    // An unknown or budget-limited host sample cannot establish idle.
    // Display relevance may omit it, but recent work still defers replacement.
    let unsampled = reading.seen.iter().find(|seen| {
        let record = &seen.record;
        Some(record.key.as_str()) != own
            && record.ended.is_none()
            && recent(record.last_seen)
            && record
                .host_process
                .is_some_and(|process| reading.processes.state(process) == State::Unknown)
    });
    if let Some(seen) = unsampled {
        let names = names_with_left(reading.seen.iter().map(|seen| &seen.record), [], []);
        let name = name_of(&names, &seen.record.key, Some(seen.record.host));
        return Some(active(&name));
    }

    // The display omits unnamed rows older than the first named row.
    // Service replacement must still respect their recent activity.
    (own.is_some()
        && activity
            .iter()
            .any(|row| row.session.is_none() && row.actor() != "viewer"))
    .then(|| active(session_view::TERMINAL))
}

/// `nanoseconds` since 1970 in milliseconds, as session records keep time.
fn milliseconds(nanoseconds: u128) -> u64 {
    u64::try_from(nanoseconds / 1_000_000).unwrap_or(u64::MAX)
}
