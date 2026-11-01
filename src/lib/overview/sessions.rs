//! The host sessions a status lists beside the helper: those live now, by
//! what the records, the controller's notes and the recent activity say,
//! each named and shown in the state it is in.
use super::pending;
use crate::host_session::reading::{SessionFacts, SessionReading, unrecorded};
use crate::{
    client::Client,
    control::Controller,
    host_session::{
        Key,
        liveness::Lifetime,
        names::{name_of, names_with_left},
        record::Seen,
    },
    session_view::{self, Who},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The most host sessions a status lists.
const SESSIONS: usize = 8;

/// The host sessions an app shows beside the helper's status, in its
/// overview: those seen in the last hour, holding the helper, or with an
/// unknown outcome, and the agents' commands no host named while they act;
/// at most eight, those needing the person first, then the one acting,
/// those waiting and the idle, newest first within each, with how many
/// more there are. Each carries the host's own id, for the app to know its
/// own session: never for an agent's reply. Beside them, whether the apps
/// speak of sessions at all and which one has the helper. The records and
/// notes are read off the runtime.
pub(crate) async fn add_host_sessions(status: &mut super::Reading, client: &Client, now: u128) {
    add_sessions_for(status, client, now, None).await;
}

pub(crate) async fn add_sessions_for(
    status: &mut super::Reading,
    client: &Client,
    now: u128,
    own: Option<&Key>,
) {
    let client = client.clone();
    let facts = SessionFacts {
        now,
        ..status.facts
    };
    let has_own = own.is_some();
    let own = own.cloned();
    let (listed, own_entry) = tokio::task::spawn_blocking(move || {
        let Some(reading) = SessionReading::collect(&client) else {
            return (HostSessions::default(), None);
        };
        let own_entry = own.and_then(|key| {
            reading
                .live(facts)
                .into_iter()
                .find(|entry| entry.key == key.as_str())
                .map(Listed::into_entry)
        });
        (reading.summary(facts), own_entry)
    })
    .await
    .unwrap_or_default();
    status.sessions = listed.sessions;
    let value = &mut status.value;
    value["overview"]["hostSessions"] = json!(
        status
            .sessions
            .iter()
            .map(|(_, entry)| entry)
            .collect::<Vec<_>>()
    );
    value["overview"]["moreSessions"] = json!(listed.more);
    value["overview"]["sessionsShown"] = json!(listed.shown);
    value["overview"]["holder"] = json!(listed.holder);
    if has_own && let Some(overview) = value["overview"].as_object_mut() {
        overview.insert("ownSession".into(), own_entry.unwrap_or(Value::Null));
    }
}

impl SessionFacts {
    /// The facts `status` gives, with its overview, at `now`.
    pub(crate) fn of(overview: &super::Overview, now: u128) -> Self {
        Self {
            pending: overview.pending.unresolved > 0,
            agent_holds: overview.mode == super::Mode::Agent && overview.agent_holds,
            now,
        }
    }

    /// Compatibility facts decoded once at viewer HTTP ingress.
    pub(crate) fn decode(status: &Value, now: u128) -> Self {
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
pub(crate) struct HostSessions {
    pub(crate) sessions: Vec<(String, Value)>,
    /// How many more relevant sessions there are than are listed.
    pub(crate) more: usize,
    /// Whether two or more sessions remain relevant.
    pub(crate) shown: bool,
    /// The key of the one with the helper; empty for the commands no host
    /// named.
    pub(crate) holder: Option<String>,
}

/// One session [`add_host_sessions`] may list: what it is, when it was
/// last seen, and how it is shown.
pub(crate) struct Listed {
    pub(crate) key: String,
    pub(crate) name: String,
    pub(crate) ended: Option<u64>,
    entry: Value,
    pub(crate) last_seen: u64,
    pub(crate) shown: session_view::Shown,
    pub(crate) running_command: bool,
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
        .map(|listed| listed.key.clone());
    let more = listed.len().saturating_sub(SESSIONS);
    let sessions = listed
        .into_iter()
        .take(SESSIONS)
        .map(|listed| (listed.key.clone(), listed.into_entry()))
        .collect();

    HostSessions {
        sessions,
        more,
        shown,
        holder,
    }
}

impl SessionReading {
    pub(crate) fn summary(&self, facts: SessionFacts) -> HostSessions {
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
    pub(crate) fn live(&self, facts: SessionFacts) -> Vec<Listed> {
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
                    key: record.key.to_string(),
                    name: name_of(&named, &record.key, Some(record.host)),
                    ended: lifetime.ended,
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
                key: key.to_string(),
                name: name_of(&named, key, note.host),
                ended: None,
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
                key: String::new(),
                name: session_view::TERMINAL.to_owned(),
                ended: None,
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
