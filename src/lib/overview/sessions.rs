//! The host sessions a status lists beside the helper: those live now, by
//! what the records, the controller's notes and the recent activity say,
//! each named and shown in the state it is in.
use super::pending;
use crate::{
    activity_log,
    client::Client,
    control::Controller,
    host_session::{
        Key,
        liveness::{Lifetime, Snapshot},
        names::{name_of, names_with_left},
        record::Seen,
        store::Store,
    },
    session_view::{self, Scene, Who},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

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
    service_blocker_with(client, recovery, now, Snapshot::polled)
}

pub(crate) fn service_blocker_with(
    client: &Client,
    recovery: bool,
    now: u128,
    sample: impl FnOnce(&std::path::Path, Vec<crate::process::Instance>) -> Snapshot,
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
    let reading = SessionReading {
        seen: seen.into(),
        rows,
        causes,
        holder,
        waiting,
        running: client.controller.running_candidates(),
        processes: Snapshot::default(),
    }
    .sampled_for_service_change(output, sample);
    if reading
        .holder
        .iter()
        .chain(&reading.running)
        .any(|note| reading.processes.state(note.process) == crate::process::State::Unknown)
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
        return Some(format!("{name} is active in the helper."));
    }
    reading
        .live(facts)
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
            // An unknown or budget-limited host sample cannot establish idle.
            // Display relevance may omit it, but recent work still defers replacement.
            if recovering_own {
                return None;
            }
            let seen = reading.seen.iter().find(|seen| {
                let record = &seen.record;
                Some(record.key.as_str()) != own
                    && record.ended.is_none()
                    && now_ms.saturating_sub(record.last_seen) <= crate::time::ms(SERVICE_IDLE)
                    && record.host_process.is_some_and(|process| {
                        reading.processes.state(process) == crate::process::State::Unknown
                    })
            })?;
            let names = names_with_left(reading.seen.iter().map(|seen| &seen.record), [], []);
            let name = name_of(&names, &seen.record.key, Some(seen.record.host));
            Some(format!("{name} is active in the helper."))
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
    add_sessions_for(status, client, now, None).await;
}

pub(crate) async fn add_sessions_for(
    status: &mut Value,
    client: &Client,
    now: u128,
    own: Option<&Key>,
) {
    let client = client.clone();
    let facts = SessionFacts::of(status, now);
    let has_own = own.is_some();
    let own = own.cloned();
    let (listed, own_entry) = tokio::task::spawn_blocking(move || {
        let Some(reading) = SessionReading::collect(&client) else {
            return (HostSessions::default(), None);
        };
        let own_entry = own.and_then(|key| {
            reading
                .entries(facts)
                .into_iter()
                .find(|entry| entry["key"].as_str() == Some(key.as_str()))
        });
        (reading.summary(facts), own_entry)
    })
    .await
    .unwrap_or_default();
    status["overview"]["hostSessions"] = json!(listed.sessions);
    status["overview"]["moreSessions"] = json!(listed.more);
    status["overview"]["sessionsShown"] = json!(listed.shown);
    status["overview"]["holder"] = json!(listed.holder);
    if has_own && let Some(overview) = status["overview"].as_object_mut() {
        overview.insert("ownSession".into(), own_entry.unwrap_or(Value::Null));
    }
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
    /// How many more relevant sessions there are than are listed.
    pub(super) more: usize,
    /// Whether two or more sessions remain relevant.
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

fn summarize(listed: Vec<Listed>) -> HostSessions {
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

/// The records and bounded note candidates for one frozen reply.
pub(crate) struct SessionReading {
    pub(crate) seen: Arc<[Seen]>,
    rows: Vec<crate::activity_log::Row>,
    causes: Vec<crate::control::Cause>,
    holder: Option<crate::control::Note>,
    waiting: Vec<crate::control::Note>,
    running: Vec<crate::control::Note>,
    pub(crate) processes: Snapshot,
}

impl SessionReading {
    pub(crate) fn collect(client: &Client) -> Option<Self> {
        Self::collect_with(client, Snapshot::polled)
    }

    pub(super) fn collect_with(
        client: &Client,
        sample: impl FnOnce(&std::path::Path, Vec<crate::process::Instance>) -> Snapshot,
    ) -> Option<Self> {
        let output = client.output_dir().ok()?;
        let seen = Store::at(output).recent();
        let rows = activity_log::recent(output, SESSION_ROWS);
        let causes = client
            .controller
            .pending_causes_within(std::time::Duration::from_millis(50))
            .unwrap_or_default();
        let holder = client.controller.holder_note();
        let waiting = client.controller.waiting_candidates();
        let running = client.controller.running_candidates();
        Some(
            Self {
                seen,
                rows,
                causes,
                holder,
                waiting,
                running,
                processes: Snapshot::default(),
            }
            .sampled(output, sample),
        )
    }

    fn sample(
        mut self,
        output: &std::path::Path,
        sample: impl FnOnce(&std::path::Path, Vec<crate::process::Instance>) -> Snapshot,
    ) -> Self {
        let instances = self
            .holder
            .iter()
            .chain(&self.waiting)
            .chain(&self.running)
            .map(|note| note.process)
            .chain(self.seen.iter().filter_map(|seen| seen.record.host_process))
            .collect();
        self.processes = sample(output, instances);
        self
    }

    fn sampled_for_service_change(
        self,
        output: &std::path::Path,
        sample: impl FnOnce(&std::path::Path, Vec<crate::process::Instance>) -> Snapshot,
    ) -> Self {
        let mut reading = self.sample(output, sample);
        let remains = |note: &crate::control::Note| {
            reading.processes.state(note.process) != crate::process::State::Gone
        };
        reading.holder = reading.holder.filter(remains);
        reading.waiting.retain(remains);
        reading.running.retain(remains);
        reading
    }

    fn sampled(
        mut self,
        output: &std::path::Path,
        sample: impl FnOnce(&std::path::Path, Vec<crate::process::Instance>) -> Snapshot,
    ) -> Self {
        self = self.sample(output, sample);
        self.holder = self
            .holder
            .filter(|note| self.processes.running(note.process));
        self.waiting
            .retain(|note| self.processes.state(note.process) == crate::process::State::Running);
        self.running
            .retain(|note| self.processes.running(note.process));
        self
    }

    fn scene(&self, facts: SessionFacts) -> Scene<'_> {
        Scene {
            rows: &self.rows,
            processes: &self.processes,
            holder: self.holder.as_ref(),
            waiting: &self.waiting,
            running: &self.running,
            causes: &self.causes,
            pending: facts.pending,
            agent_holds: facts.agent_holds,
            now: facts.now,
        }
    }

    pub(crate) fn work(&self, key: &Key, facts: SessionFacts) -> bool {
        self.scene(facts).work(&Who::Host(key.clone()))
    }

    pub(super) fn summary(&self, facts: SessionFacts) -> HostSessions {
        summarize(self.live(facts))
    }

    pub(crate) fn entries(&self, facts: SessionFacts) -> Vec<Value> {
        self.live(facts)
            .into_iter()
            .map(Listed::into_entry)
            .collect()
    }
}

impl SessionReading {
    fn live(&self, facts: SessionFacts) -> Vec<Listed> {
        let seen = &self.seen;
        let causes = &self.causes;
        let holder = &self.holder;
        let waiting = &self.waiting;
        let running = &self.running;
        let scene = self.scene(facts);
        let now_ms = u64::try_from(facts.now / 1_000_000).unwrap_or(u64::MAX);
        let mut unknown: BTreeMap<&Key, usize> = BTreeMap::new();
        for cause in causes.iter().filter(|cause| !cause.running) {
            if let Some(session) = &cause.session {
                *unknown.entry(session).or_default() += 1;
            }
        }
        // A session whose record has not been written is named by its command's
        // note meanwhile, by the same rule as a record.
        let first = unrecorded(holder.as_ref(), waiting, running, seen);
        let named = names_with_left(
            seen.iter().map(|seen| &seen.record),
            [],
            first.iter().copied(),
        );

        let mut listed: Vec<Listed> = seen
            .iter()
            .filter_map(|seen| {
                let record = &seen.record;
                let uncertain = unknown.get(&record.key).copied().unwrap_or(0);
                let (shown, lifetime) = scene.project(record);
                shown.relevant().then(|| Listed {
                    entry: host_session(seen, &named, uncertain, lifetime),
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
    let notes = holder.into_iter().chain(waiting).chain(running);
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
    SessionReading::collect(client).is_some_and(|reading| {
        !unrecorded(
            reading.holder.as_ref(),
            &reading.waiting,
            &reading.running,
            seen,
        )
        .is_empty()
    })
}

/// A record's frozen lifetime and activity metadata, before its shown
/// work and recovery state are added.
fn host_session(
    seen: &Seen,
    names: &BTreeMap<Key, String>,
    uncertain: usize,
    lifetime: Lifetime,
) -> Value {
    let record = &seen.record;

    json!({
        "key": record.key,
        "host": record.host,
        "hostLabel": record.host.label(),
        "name": name_of(names, &record.key, Some(record.host)),
        "folder": record.folder,
        "hostSession": record.host_session,
        "started": record.started,
        "lastSeen": record.last_seen,
        "ended": lifetime.ended,
        "active": lifetime.active,
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
