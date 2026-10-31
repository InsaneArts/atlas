//! The host sessions a status lists beside the helper: those live now, by
//! what the records, the controller's notes and the recent activity say,
//! each named and shown in the state it is in.
use super::pending;
use crate::{
    activity_log,
    client::Client,
    control::{Cause, Controller, Note},
    host_session::{
        Key,
        names::{name_of, names_with},
        record::Seen,
        store::Store,
    },
    session_view::{self, Scene, Who},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

/// The most host sessions a status lists.
const SESSIONS: usize = 8;
/// How many recent activity rows each session's state and latest action
/// are read from.
const SESSION_ROWS: usize = 64;
/// How recently another session used Atlas to defer service replacement.
const SERVICE_IDLE: std::time::Duration = std::time::Duration::from_secs(2 * 60);
/// How long the local service-change read waits for a marker writer.
const MARKER_CHECK: std::time::Duration = std::time::Duration::from_millis(250);

/// Who would be interrupted by a service replacement, from checked session
/// records and the full recent activity window. The caller's own session
/// is left out for recent activity. Its waiting and running commands still defer
/// replacement; the service change's own holder note does not. Recovering the
/// caller's unknown outcome ignores recent activity and waiters, which cannot
/// act until recovery finishes.
pub(crate) fn service_blocker(client: &Client, recovery: bool, now: u128) -> Option<String> {
    let Ok(output) = client.output_dir() else {
        return Some("Atlas could not check which sessions are active.".into());
    };
    let Some(seen) = Store::at(output).checked_recent() else {
        return Some(
            "Atlas could not read every session record; another session may still be active."
                .into(),
        );
    };
    let Some(waiting) = client.controller.checked_waiting_notes() else {
        return Some(
            "Atlas could not read every waiting command; a command may still be active.".into(),
        );
    };
    let causes = match client.controller.pending_causes_within(MARKER_CHECK) {
        Ok(causes) => causes,
        Err(error) if error.code == crate::error::Code::Busy => return Some(error.error),
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
    if recovery
        && causes
            .iter()
            .any(|cause| !cause.running && cause.session.as_ref().map(|key| key.as_str()) != own)
    {
        return Some(
            "Another session has an unknown outcome; that session must recover it.".into(),
        );
    }
    let recovering_own = recovery && pending;
    if !recovering_own && let Some(note) = waiting.first() {
        let first = unrecorded(None, &waiting, &[], &seen);
        let names = names_with(seen.iter().map(|seen| &seen.record), first.iter().copied());
        let name = note.session.as_ref().map_or_else(
            || session_view::TERMINAL.to_owned(),
            |key| name_of(&names, key, note.host),
        );
        return Some(format!("{name} is active in the helper."));
    }
    let now_ms = u64::try_from(now / 1_000_000).unwrap_or(u64::MAX);
    let holder = client.controller.holder_note().filter(|note| {
        !(note.process.is_this_one() && note.session.as_ref().map(|key| key.as_str()) == own)
    });
    let facts = SessionFacts {
        pending,
        agent_holds: false,
        now,
    };
    let Some(activity) =
        activity_log::checked_since(output, now_ms.saturating_sub(crate::time::ms(SERVICE_IDLE)))
    else {
        return Some(
            "Atlas could not read every activity row; another session may still be active.".into(),
        );
    };
    let rows = activity_log::recent(output, SESSION_ROWS);
    live_from(
        client,
        facts,
        &rows,
        &seen,
        &causes,
        holder.as_ref(),
        &waiting,
    )
    .into_iter()
    .find(|listed| {
        let key = listed.entry["key"].as_str().filter(|key| !key.is_empty());
        listed.shown.holds_helper
            || listed.running_command
            || (!recovering_own
                && key != own
                && listed.entry["ended"].is_null()
                && now_ms.saturating_sub(listed.last_seen) <= crate::time::ms(SERVICE_IDLE))
    })
    .map(|listed| {
        let name = listed.entry["name"].as_str().unwrap_or("Another session");
        if listed.running_command {
            return format!("{name} has a command still running in the helper.");
        }
        format!("{name} is active in the helper.")
    })
    .or_else(|| {
        // The display omits unnamed rows older than the first named row.
        // Service replacement must still respect their recent activity.
        (!recovering_own
            && own.is_some()
            && activity
                .iter()
                .any(|row| row.session.is_none() && row.actor() != "viewer"))
        .then(|| format!("{} is active in the helper.", session_view::TERMINAL))
    })
}

/// The host sessions an app shows beside the helper's status, in its
/// overview: those seen in the last hour, holding the helper, or with an
/// unknown outcome, and the agents' commands no host named while they act;
/// at most eight, those needing the person first, then the one acting,
/// those waiting and the idle, newest first within each, with how many
/// more there are. Each carries the host's own id, for the app to know its
/// own session: never for an agent's reply. Beside them, whether the apps
/// speak of sessions at all and which one has the helper. The records and
/// notes are read off the runtime.
pub(crate) async fn add_host_sessions(status: &mut Value, client: &Client, now: u128) {
    let client = client.clone();
    let facts = SessionFacts::of(status, now);
    let listed = tokio::task::spawn_blocking(move || host_sessions(&client, facts))
        .await
        .unwrap_or_default();
    status["overview"]["hostSessions"] = json!(listed.sessions);
    status["overview"]["moreSessions"] = json!(listed.more);
    status["overview"]["sessionsShown"] = json!(listed.shown);
    status["overview"]["holder"] = json!(listed.holder);
}

/// Every live host session, as [`add_host_sessions`] would list them with
/// no limit, by `facts`: the viewer's session menu lists them all, and a
/// pane finds its own among them. Read off the runtime.
pub(crate) async fn live_host_sessions(client: &Client, facts: SessionFacts) -> Vec<Value> {
    let client = client.clone();
    tokio::task::spawn_blocking(move || {
        live(&client, facts)
            .into_iter()
            .map(Listed::into_entry)
            .collect()
    })
    .await
    .unwrap_or_default()
}

/// What a status says that the sessions' states depend on.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SessionFacts {
    /// Whether an outcome is unknown.
    pub(super) pending: bool,
    /// Whether the input service says an agent, not the person, holds
    /// control.
    pub(super) agent_holds: bool,
    /// Now, in nanoseconds since 1970.
    pub(super) now: u128,
}

impl SessionFacts {
    /// The facts `status` gives, with its overview, at `now`.
    pub(crate) fn of(status: &Value, now: u128) -> Self {
        let overview = &status["overview"];
        Self {
            pending: overview["pending"].as_u64().unwrap_or(0) > 0,
            agent_holds: overview["mode"] == "agent"
                && status["session"]["inputService"]["controlActive"] == true,
            now,
        }
    }

    /// The facts the controller's own files give at `now`, with no status
    /// read: whether an outcome is unknown, and no word of who holds
    /// control but its notes.
    pub(crate) fn local(controller: &Controller, now: u128) -> Self {
        Self {
            pending: pending(controller).0.unresolved > 0,
            agent_holds: false,
            now,
        }
    }
}

/// The sessions [`add_host_sessions`] lists.
#[derive(Default)]
pub(super) struct HostSessions {
    pub(super) sessions: Vec<Value>,
    /// How many more live sessions there are than are listed.
    pub(super) more: usize,
    /// Whether two or more are live.
    pub(super) shown: bool,
    /// The key of the one with the helper; empty for the commands no host
    /// named.
    pub(super) holder: Option<String>,
}

/// One session [`add_host_sessions`] may list: what it is, when it was
/// last seen, and how it is shown.
struct Listed {
    entry: Value,
    last_seen: u64,
    shown: session_view::Shown,
    running_command: bool,
}

impl Listed {
    /// Its entry with how it is shown.
    fn into_entry(self) -> Value {
        let mut entry = self.entry;
        if let (Some(entry), Value::Object(shown)) = (entry.as_object_mut(), json!(self.shown)) {
            entry.extend(shown);
        }
        entry
    }
}

/// The sessions [`add_host_sessions`] lists, by `facts`.
pub(super) fn host_sessions(client: &Client, facts: SessionFacts) -> HostSessions {
    let listed = live(client, facts);
    let shown = session_view::sessions_shown(listed.iter().map(|listed| &listed.shown));
    let holder = listed
        .iter()
        .find(|listed| listed.shown.holds_helper)
        .and_then(|listed| listed.entry["key"].as_str())
        .map(str::to_owned);
    let more = listed.len().saturating_sub(SESSIONS);
    let sessions = listed
        .into_iter()
        .take(SESSIONS)
        .map(Listed::into_entry)
        .collect();

    HostSessions {
        sessions,
        more,
        shown,
        holder,
    }
}

/// Every live session by `facts`: those seen in the last hour, holding or
/// waiting for the helper, or with an unknown outcome; those on their
/// first command, from their notes; and the agents' commands no host
/// named. Those needing the person first, then the one acting, those
/// waiting and the idle, newest first within each. Ended ones are left
/// out: the viewer's history lists them.
fn live(client: &Client, facts: SessionFacts) -> Vec<Listed> {
    let Ok(output) = client.output_dir() else {
        return Vec::new();
    };
    let seen = Store::at(output).recent();
    live_with(client, facts, output, &seen)
}

fn live_with(client: &Client, facts: SessionFacts, output: &Path, seen: &[Seen]) -> Vec<Listed> {
    let rows = activity_log::recent(output, SESSION_ROWS);
    let causes = client.controller.pending_causes().unwrap_or_default();
    let holder = client.controller.holder_note();
    let waiting = client.controller.waiting_notes();
    live_from(
        client,
        facts,
        &rows,
        seen,
        &causes,
        holder.as_ref(),
        &waiting,
    )
}

fn live_from(
    client: &Client,
    facts: SessionFacts,
    rows: &[activity_log::Row],
    seen: &[Seen],
    causes: &[Cause],
    holder: Option<&Note>,
    waiting: &[Note],
) -> Vec<Listed> {
    let running = client.controller.running_notes();
    let scene = Scene {
        rows,
        holder,
        waiting,
        causes,
        pending: facts.pending,
        agent_holds: facts.agent_holds,
        now: facts.now,
    };

    let now_ms = u64::try_from(facts.now / 1_000_000).unwrap_or(u64::MAX);
    let live_for = crate::time::ms(session_view::LIVE_FOR);
    let mut unknown: BTreeMap<&Key, usize> = BTreeMap::new();
    for cause in causes.iter().filter(|cause| !cause.running) {
        if let Some(session) = &cause.session {
            *unknown.entry(session).or_default() += 1;
        }
    }
    // A session whose record has not been written is named by its command's
    // note meanwhile, by the same rule as a record.
    let first = unrecorded(holder, waiting, &running, seen);
    let named = names_with(seen.iter().map(|seen| &seen.record), first.iter().copied());

    let mut listed: Vec<Listed> = seen
        .iter()
        .filter_map(|seen| {
            let record = &seen.record;
            let uncertain = unknown.get(&record.key).copied().unwrap_or(0);
            let shown = scene.shown(&Who::Host(record.key.clone()), record.ended_at().is_some());
            let wanted = now_ms.saturating_sub(record.last_seen) <= live_for
                || shown.at_work()
                || uncertain > 0;
            (wanted && shown.live).then(|| Listed {
                entry: host_session(seen, &named, uncertain, &shown, now_ms),
                last_seen: record.last_seen,
                running_command: causes
                    .iter()
                    .any(|cause| cause.command && cause.session.as_ref() == Some(&record.key)),
                shown,
            })
        })
        .collect();
    for note in first {
        let Some(key) = &note.session else { continue };
        listed.push(Listed {
            entry: first_command(key, note, &named),
            last_seen: note.since,
            shown: scene.shown(&Who::Host(key.clone()), false),
            running_command: causes
                .iter()
                .any(|cause| cause.command && cause.session.as_ref() == Some(key)),
        });
    }
    if let Some((started, last_seen)) = scene.terminal() {
        listed.push(Listed {
            entry: terminal(started, last_seen, now_ms),
            last_seen,
            shown: scene.shown(&Who::Terminal, false),
            running_command: causes
                .iter()
                .any(|cause| cause.command && cause.session.is_none()),
        });
    }
    listed.sort_by_key(|listed| (listed.shown.state, std::cmp::Reverse(listed.last_seen)));
    listed
}

/// The live notes, the holder's first, then the waiting commands' and the
/// running commands', naming a session that has no record among `seen`:
/// each such session once.
fn unrecorded<'a>(
    holder: Option<&'a crate::control::Note>,
    waiting: &'a [crate::control::Note],
    running: &'a [crate::control::Note],
    seen: &[Seen],
) -> Vec<&'a crate::control::Note> {
    let notes = holder
        .filter(|note| note.live())
        .into_iter()
        .chain(waiting)
        .chain(running);
    let mut found: Vec<&crate::control::Note> = Vec::new();
    for note in notes {
        let Some(key) = &note.session else { continue };
        let known = seen.iter().any(|seen| seen.record.key == *key)
            || found.iter().any(|kept| kept.session.as_ref() == Some(key));
        if !known {
            found.push(note);
        }
    }
    found
}

/// Whether a live holder, waiting or running note names a session with no
/// record yet among `seen`: a session on its first command.
pub(crate) fn first_command_running(client: &Client, seen: &[Seen]) -> bool {
    let holder = client.controller.holder_note();
    let waiting = client.controller.waiting_notes();
    let running = client.controller.running_notes();
    !unrecorded(holder.as_ref(), &waiting, &running, seen).is_empty()
}

/// One host session as [`add_host_sessions`] lists it, before the rest of
/// how it is `shown`: one at work has neither ended nor gone quiet,
/// whatever its record says.
fn host_session(
    seen: &Seen,
    names: &BTreeMap<Key, String>,
    uncertain: usize,
    shown: &session_view::Shown,
    now: u64,
) -> Value {
    let record = &seen.record;
    let at_work = shown.at_work();
    json!({
        "key": record.key,
        "host": record.host,
        "hostLabel": record.host.label(),
        "name": name_of(names, &record.key, Some(record.host)),
        "folder": record.folder,
        "hostSession": record.host_session,
        "started": record.started,
        "lastSeen": record.last_seen,
        "ended": record.ended_at().filter(|_| !at_work),
        "active": at_work || record.active(now),
        "uncertain": uncertain,
        "leftovers": {"copies": seen.copies_left, "plans": record.plans.len()},
    })
}

/// A host session whose first command holds or waits for the helper, or
/// runs as a command started with `run`, and so has no record yet, from
/// its note: named by its host and folder, and
/// started when its command did, as its record will say.
fn first_command(key: &Key, note: &crate::control::Note, names: &BTreeMap<Key, String>) -> Value {
    json!({
        "key": key,
        "host": note.host,
        "hostLabel": note.host.map(crate::host_session::Host::label),
        "name": name_of(names, key, note.host),
        "folder": note.folder,
        "started": note.process.started_ms(),
        "lastSeen": note.since,
        "ended": null,
        "active": true,
        "uncertain": 0,
        "leftovers": {"copies": 0, "plans": 0},
    })
}

/// The agents' commands no host named, listed as one session with no
/// record: from `started` to `last_seen`.
fn terminal(started: u64, last_seen: u64, now: u64) -> Value {
    let active =
        now.saturating_sub(last_seen) <= crate::time::ms(crate::host_session::record::ACTIVE_FOR);
    json!({
        "key": "",
        "host": "none",
        "hostLabel": session_view::TERMINAL,
        "name": session_view::TERMINAL,
        "folder": null,
        "started": started,
        "lastSeen": last_seen,
        "ended": null,
        "active": active,
        "uncertain": 0,
        "leftovers": {"copies": 0, "plans": 0},
    })
}
