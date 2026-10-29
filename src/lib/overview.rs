//! One summary of what is happening in the helper, for the viewer and the
//! menu bar: a mode, a one-sentence reason, recent agent activity and the
//! capability facts a person needs at a glance.
use crate::{
    client::Client,
    control::Controller,
    error::Error,
    host_session::{
        Key,
        names::{name_of, names_with},
        record::Seen,
        store::Store,
    },
    session_view::{self, Scene, Who},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Read, Seek, SeekFrom},
    path::Path,
    sync::{Mutex, MutexGuard, PoisonError},
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
/// The most host sessions a status lists.
const SESSIONS: usize = 8;
/// How many recent activity rows each session's state and latest action
/// are read from.
const SESSION_ROWS: usize = 64;

/// A read of the helper that took longer than a status waits.
pub(crate) fn late() -> Error {
    Error::new("transport", "The helper did not answer in time")
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
pub async fn report(client: &Client, until: Instant) -> Value {
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

/// One look at directory services and the process list, shared by every
/// status read in this process: at most one runs at a time, and what it
/// found is reused for a while. A look that hangs keeps one thread at most.
struct Shared<T> {
    fresh_for: Duration,
    kept: Mutex<Kept<T>>,
}

struct Kept<T> {
    /// The helper user ID the last look was for, what it found and when.
    last: Option<(u32, T, Instant)>,
    /// Whether a look runs now.
    running: bool,
}

impl<T: Clone + Send + 'static> Shared<T> {
    const fn new(fresh_for: Duration) -> Self {
        Self {
            fresh_for,
            kept: Mutex::new(Kept {
                last: None,
                running: false,
            }),
        }
    }

    /// What `look` finds for helper `uid`. While the last look's result is
    /// fresh, or another look runs, that result comes at once; otherwise a
    /// new look runs off the runtime and is waited for until `until`. A look
    /// given up on still ends and keeps what it found. `None` when nothing
    /// is known for `uid` in time, or only a result too old to trust.
    async fn get(
        &'static self,
        uid: u32,
        until: Instant,
        look: impl FnOnce() -> T + Send + 'static,
    ) -> Option<T> {
        let known = {
            let mut kept = self.lock();
            let last = kept.last.as_ref().filter(|(key, ..)| *key == uid);
            let fresh = last.is_some_and(|(.., taken)| taken.elapsed() < self.fresh_for);
            let known = last.map(|(_, found, taken)| (found.clone(), *taken));
            if fresh || kept.running {
                return self.trusted(known);
            }
            kept.running = true;
            known
        };

        // The look ends as not running however it ends, even never started.
        let ending = Ending(self);
        let looking = tokio::task::spawn_blocking(move || {
            let found = look();
            ending.0.lock().last = Some((uid, found.clone(), Instant::now()));
            drop(ending);
            found
        });

        match tokio::time::timeout_at(until, looking).await {
            Ok(Ok(found)) => Some(found),
            _ => self.trusted(known),
        }
    }
}

impl<T> Shared<T> {
    fn lock(&self) -> MutexGuard<'_, Kept<T>> {
        self.kept.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// What a look found, unless it was taken more than twice as long ago
    /// as a result is reused for.
    fn trusted(&self, known: Option<(T, Instant)>) -> Option<T> {
        known
            .filter(|(_, taken)| taken.elapsed() < self.fresh_for * 2)
            .map(|(found, _)| found)
    }
}

/// Marks a look as not running when dropped.
struct Ending<T: 'static>(&'static Shared<T>);

impl<T> Drop for Ending<T> {
    fn drop(&mut self) {
        self.0.lock().running = false;
    }
}

/// The controller's mutations of unknown outcome, and how many are pending
/// in all. An unreadable marker is unknown, so it counts as unresolved.
pub fn pending(controller: &Controller) -> (Pending, usize) {
    let (unresolved, total, commands) =
        controller.pending_snapshot().map_or((1, 1, 0), |snapshot| {
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
pub async fn read(client: &Client) -> Value {
    let mut status = report(client, Instant::now() + READ_DEADLINE).await;
    let (pending, _) = pending(&client.controller);
    let activity = client
        .output_dir()
        .map(|directory| activity(directory, ROWS))
        .unwrap_or_default();

    status["controlActive"] = json!(false);
    let now = crate::client::timestamp();
    status["overview"] = overview(&status, false, pending, &activity, now);
    add_host_sessions(&mut status, client, now).await;
    status
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
pub async fn add_host_sessions(status: &mut Value, client: &Client, now: u128) {
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
    pending: bool,
    /// Whether the input service says an agent, not the person, holds
    /// control.
    agent_holds: bool,
    /// Now, in nanoseconds since 1970.
    now: u128,
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
struct HostSessions {
    sessions: Vec<Value>,
    /// How many more live sessions there are than are listed.
    more: usize,
    /// Whether two or more are live.
    shown: bool,
    /// The key of the one with the helper; empty for the commands no host
    /// named.
    holder: Option<String>,
}

/// One session [`add_host_sessions`] may list: what it is, when it was
/// last seen, and how it is shown.
struct Listed {
    entry: Value,
    last_seen: u64,
    shown: session_view::Shown,
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
fn host_sessions(client: &Client, facts: SessionFacts) -> HostSessions {
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
    let rows = activity(output, SESSION_ROWS);
    let causes = client.controller.pending_causes().unwrap_or_default();
    let holder = client.controller.holder_note();
    let waiting = client.controller.waiting_notes();
    let scene = Scene {
        rows: &rows,
        holder: holder.as_ref(),
        waiting: &waiting,
        causes: &causes,
        pending: facts.pending,
        agent_holds: facts.agent_holds,
        now: facts.now,
    };

    let now_ms = u64::try_from(facts.now / 1_000_000).unwrap_or(u64::MAX);
    let live_for = u64::try_from(session_view::LIVE_FOR.as_millis()).unwrap_or(u64::MAX);
    let mut unknown: BTreeMap<&Key, usize> = BTreeMap::new();
    for cause in causes.iter().filter(|cause| !cause.running) {
        if let Some(session) = &cause.session {
            *unknown.entry(session).or_default() += 1;
        }
    }
    // A session on its first command has no record until that command
    // ends: its note names it meanwhile, by the same rule as a record.
    let first = unrecorded(holder.as_ref(), &waiting, &seen);
    let named = names_with(seen.iter().map(|seen| &seen.record), first.iter().copied());

    let mut listed: Vec<Listed> = seen
        .iter()
        .filter_map(|seen| {
            let record = &seen.record;
            let uncertain = unknown.get(&record.key).copied().unwrap_or(0);
            let shown = scene.shown(&Who::Host(record.key.clone()), record.ended_at().is_some());
            let wanted = now_ms.saturating_sub(record.last_seen) <= live_for
                || shown.holds_helper
                || shown.waiting_for.is_some()
                || uncertain > 0;
            (wanted && shown.live).then(|| Listed {
                entry: host_session(seen, &named, uncertain, &shown, now_ms),
                last_seen: record.last_seen,
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
        });
    }
    if let Some((started, last_seen)) = scene.terminal() {
        listed.push(Listed {
            entry: terminal(started, last_seen, now_ms),
            last_seen,
            shown: scene.shown(&Who::Terminal, false),
        });
    }
    listed.sort_by_key(|listed| (listed.shown.state, std::cmp::Reverse(listed.last_seen)));
    listed
}

/// The live notes, the holder's first, naming a session that has no
/// record among `seen`: each such session once.
fn unrecorded<'a>(
    holder: Option<&'a crate::control::Note>,
    waiting: &'a [crate::control::Note],
    seen: &[Seen],
) -> Vec<&'a crate::control::Note> {
    let notes = holder.filter(|note| note.live()).into_iter().chain(waiting);
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

/// Whether a live holder or waiting note names a session with no record
/// yet among `seen`: a session on its first command.
pub(crate) fn first_command_running(client: &Client, seen: &[Seen]) -> bool {
    let holder = client.controller.holder_note();
    let waiting = client.controller.waiting_notes();
    !unrecorded(holder.as_ref(), &waiting, seen).is_empty()
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

/// A host session whose first command holds or waits for the helper and
/// so has no record yet, from its note: named by its host and folder, and
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
    let active = now.saturating_sub(last_seen)
        <= u64::try_from(crate::host_session::record::ACTIVE_FOR.as_millis()).unwrap_or(u64::MAX);
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
/// process is still carrying out, commands agents started among them.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pending {
    pub unresolved: usize,
    pub in_flight: usize,
    /// How many of those in flight are commands agents started with `run`.
    pub commands: usize,
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
        let reason = if report["ready"] != true && !recoverable {
            first_step.as_str()
        } else if session["inputService"]["held"] == true {
            "You stopped the agent mid-action, so its outcome is unknown. Recover, then resume."
        } else if pending.commands > 0 {
            "An action's outcome is unknown; recovery waits for the running command to end"
        } else {
            "An action's outcome is unknown; recover before taking control"
        };
        ("recovering", reason.to_owned())
    } else if session["inputService"]["held"] == true && !lease_active && report["ready"] == true {
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
    } else if lease_active && pending.commands > 0 {
        (
            "controlling",
            "You have control. A command an agent started is still running".to_owned(),
        )
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
        // Recovery waits for a command an agent started to end, so it helps
        // only once none runs.
        "recoveryHelps": pending.commands == 0
            && ((mode == "recovering" && (report["ready"] == true || recoverable))
                || (mode == "blocked" && recoverable)),
        "account": if reachable { &session["user"]["name"] } else { &report["account"] },
        "display": serde::Deserialize::deserialize(&session["display"])
            .ok()
            .filter(crate::session::Display::valid)
            .map(|_: crate::session::Display| &session["display"]),
        "pending": pending.unresolved,
        "inFlight": pending.in_flight,
        "commands": pending.commands,
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
        "no-window" => "The app had no window on screen",
        "input" => "The mouse and keyboard weren't ready",
        "capture" => "The screen couldn't be read",
        "permission" => match row["operation"].as_str() {
            Some("screenshot") => "The helper can't record the screen",
            Some("input") => "The helper can't use the mouse and keyboard",
            _ => "The helper doesn't have permission",
        },
        "session-changed" => "The helper's screen changed meanwhile",
        "focus-changed" if row["reason"] == "unreadable" => {
            "Atlas couldn't tell which app was in front"
        }
        "focus-changed" if row["reason"] == "window" => "Another window was in front",
        "focus-changed" if row["reason"] == "sheet" => "A sheet was open over the window",
        "focus-changed" if row["reason"] == "field" => "Another field had focus",
        "focus-changed" if row["reason"] == "disabled" => "That field is disabled",
        "focus-changed" => "Another app was in front",
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
        // The host session's key and the screenshot's file name, as the row
        // keeps them; apps name the session from their own list.
        "session": row["session"],
        "capture": row["capture"],
        // Refused before anything was sent, as when another app was in
        // front: not a problem, and nothing to recover.
        "nothingSent": row["code"] == "focus-changed",
        // Such a refusal after part of its text was typed.
        "partial": row["partial"] == true,
        // Its outcome is unknown until the person recovers.
        "uncertain": row["code"] == "uncertain",
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

    /// The facts of a status with `pending` whether an outcome is unknown,
    /// at `now`.
    fn facts(pending: bool, now: u128) -> SessionFacts {
        SessionFacts {
            pending,
            agent_holds: false,
            now,
        }
    }
    use std::fs;

    const NOW: u128 = 100_000_000_000;

    /// How long a look is reused in these tests.
    const TEST_REUSE: Duration = Duration::from_secs(1);

    fn shared<T: Clone + Send + 'static>() -> &'static Shared<T> {
        Box::leak(Box::new(Shared::new(TEST_REUSE)))
    }

    /// Make the last look `by` older than it is; so soon after the Mac
    /// started that no such time exists, wait that long instead.
    fn age<T>(shared: &Shared<T>, by: Duration) {
        let mut kept = shared.lock();
        let Some((.., taken)) = &mut kept.last else {
            return;
        };
        match taken.checked_sub(by) {
            Some(older) => *taken = older,
            None => {
                drop(kept);
                std::thread::sleep(by);
            }
        }
    }

    /// A look that counts how often it ran and gives that count.
    fn counting(
        runs: &std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) -> impl FnOnce() -> usize + Send + 'static {
        let runs = std::sync::Arc::clone(runs);
        move || runs.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
    }

    fn later() -> Instant {
        Instant::now() + Duration::from_secs(5)
    }

    #[test]
    fn every_screenshot_refusal_has_a_plain_reason_in_the_activity_list() {
        for (code, words) in [
            ("capture", "The screen couldn't be read"),
            ("identity", "Atlas couldn't reach the helper"),
            ("on-console", "Atlas couldn't reach the helper"),
            ("permission", "The helper can't record the screen"),
            ("session-changed", "The helper's screen changed meanwhile"),
            ("no-window", "The app had no window on screen"),
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
    fn activity_rows_carry_their_session_and_capture() {
        let row = json!({"at": "99000000000", "operation": "screenshot", "ok": true,
            "actor": "agent", "verb": "screenshot", "command": "0123456789ab",
            "session": "3f9a1c07b2e4", "capture": "1761660429000123456-capture.png"});
        let shown = shown(&row, NOW);
        assert_eq!(shown["session"], "3f9a1c07b2e4");
        assert_eq!(shown["capture"], "1761660429000123456-capture.png");

        let plain = super::shown(
            &json!({"at": "99000000000", "operation": "input", "ok": true}),
            NOW,
        );
        assert_eq!(plain["session"], Value::Null);
        assert_eq!(plain["capture"], Value::Null);
    }

    #[test]
    fn shown_rows_never_carry_a_task() {
        let row = json!({"at": "99000000000", "operation": "input", "ok": true,
            "actor": "agent", "verb": "click", "session": "3f9a1c07b2e4",
            "task": "5be0d1a2c3f4"});
        let shown = shown(&row, NOW);

        assert!(shown.get("task").is_none());
        assert!(!shown.to_string().contains("5be0d1a2c3f4"));
    }

    #[test]
    fn the_status_lists_recent_host_sessions_and_who_holds_the_helper() {
        use crate::host_session::{
            Host, HostSession,
            store::{Kind, note},
        };

        let directory = tempfile::tempdir().unwrap();
        let client = crate::transaction_test::client(directory.path());
        let session = |id: String| HostSession::new(Host::Codex, &id).unwrap();
        let worked = |session: &HostSession| {
            let doing = crate::doing::Doing::new("click");
            note(
                &client,
                session,
                Kind::Other,
                Some(&doing),
                &Ok(json!({"ok": true})),
            )
            .unwrap();
        };
        let sessions: Vec<HostSession> = (0..10).map(|n| session(format!("worker-{n}"))).collect();
        for each in &sessions {
            worked(each);
            std::thread::sleep(Duration::from_millis(2));
        }

        // One long idle, with an unknown outcome of its own.
        let idle = session("idle".into());
        worked(&idle);
        let path = client
            .output_dir()
            .unwrap()
            .join(format!("sessions/{}.json", idle.key()));
        let mut record: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        record["lastSeen"] = json!(1_000);
        fs::write(&path, record.to_string()).unwrap();
        drop(
            client
                .controller
                .clone()
                .labelled(Some(&idle))
                .quarantine()
                .unwrap(),
        );
        let holding = client
            .controller
            .clone()
            .labelled(Some(&sessions[9]))
            .acquire(true)
            .unwrap();

        let listed = host_sessions(&client, facts(true, crate::client::timestamp()));
        assert_eq!(listed.sessions.len(), 8);
        assert_eq!(listed.more, 3);
        assert!(listed.shown, "ten live sessions");
        assert_eq!(listed.holder.as_deref(), Some(sessions[9].key().as_str()));
        let sessions_listed = &listed.sessions;
        assert_eq!(
            sessions_listed[0]["key"],
            idle.key().as_str(),
            "the one needing the person first, however long idle"
        );
        assert_eq!(sessions_listed[0]["state"], "needsYou");
        assert_eq!(sessions_listed[0]["live"], true);
        let first = &sessions_listed[1];
        assert_eq!(
            first["key"],
            sessions[9].key().as_str(),
            "then the one acting"
        );
        assert_eq!(first["state"], "acting");
        assert_eq!(first["holdsHelper"], true);
        assert_eq!(sessions_listed[2]["holdsHelper"], false);
        assert_eq!(
            sessions_listed[2]["key"],
            sessions[8].key().as_str(),
            "then newest first"
        );
        assert_eq!(first["host"], "codex");
        assert_eq!(first["hostLabel"], "Codex");
        assert_eq!(first["hostSession"], "worker-9");
        assert_eq!(first["active"], true);
        assert_eq!(first["uncertain"], 0);
        assert_eq!(first["leftovers"], json!({"copies": 0, "plans": 0}));
        assert_eq!(first["waitingFor"], Value::Null);
        assert!(first["name"].as_str().unwrap().starts_with("Codex"));
        drop(holding);

        let listed = host_sessions(
            &client,
            facts(true, crate::client::timestamp() + 2 * 3_600_000_000_000),
        );
        assert_eq!(
            listed.sessions.len(),
            1,
            "only the one with an unknown outcome, hours later"
        );
        assert!(!listed.shown, "one live session speaks of none");
        assert_eq!(listed.holder, None);
        let only = &listed.sessions[0];
        assert_eq!(only["key"], idle.key().as_str());
        assert_eq!(only["uncertain"], 1);
        assert_eq!(only["active"], false);
    }

    #[test]
    fn the_agents_commands_no_host_named_are_listed_as_terminal_beside_the_hosts() {
        use crate::host_session::{Host, HostSession, store::note};

        let directory = tempfile::tempdir().unwrap();
        let client = crate::transaction_test::client(directory.path());
        let output = client.output_dir().unwrap().to_path_buf();
        let session = HostSession::new(Host::Claude, "left").unwrap();
        note(
            &client,
            &session,
            crate::host_session::store::Kind::Other,
            Some(&crate::doing::Doing::new("click")),
            &Ok(json!({"ok": true})),
        )
        .unwrap();
        let now = crate::client::timestamp();
        let rows = [
            json!({"at": (now - 1_000_000_000).to_string(), "operation": "input", "ok": true,
                "actor": "agent", "verb": "click", "app": "Notes", "command": "000000000001"}),
            json!({"at": (now - 2_000_000_000).to_string(), "operation": "input", "ok": true,
                "actor": "agent", "verb": "type", "app": "Opie", "command": "000000000002",
                "session": session.key()}),
        ];
        let log: String = rows.iter().rev().map(|row| format!("{row}\n")).collect();
        fs::write(output.join("metadata.jsonl"), log).unwrap();

        let listed = host_sessions(&client, facts(false, now));
        assert!(listed.shown, "a host's session and Terminal make two");
        let names: Vec<&str> = listed
            .sessions
            .iter()
            .map(|entry| entry["name"].as_str().unwrap())
            .collect();
        assert!(
            names[0].starts_with("Claude Code"),
            "its record was seen last"
        );
        assert_eq!(names[1], "Terminal");
        let terminal = &listed.sessions[1];
        assert_eq!(terminal["key"], "");
        assert_eq!(terminal["host"], "none");
        assert_eq!(terminal["state"], "idle");
        assert_eq!(terminal["latest"]["says"], "clicked in Notes");
        assert!(terminal.get("hostSession").is_none());
        assert_eq!(listed.sessions[0]["latest"]["says"], "typed in Opie");
    }

    #[test]
    fn a_session_on_its_first_command_is_listed_from_its_note() {
        use crate::host_session::{Host, HostSession};

        let directory = tempfile::tempdir().unwrap();
        let client = crate::transaction_test::client(directory.path());
        let session = HostSession::new(Host::Codex, "first").unwrap();
        let holding = client
            .controller
            .clone()
            .labelled(Some(&session))
            .acquire(true)
            .unwrap();

        let listed = host_sessions(&client, facts(false, crate::client::timestamp()));
        assert_eq!(listed.sessions.len(), 1);
        let entry = &listed.sessions[0];
        assert_eq!(entry["key"], session.key().as_str());
        assert_eq!(entry["name"], "Codex");
        assert_eq!(entry["state"], "acting");
        assert!(entry.get("hostSession").is_none());
        assert_eq!(listed.holder.as_deref(), Some(session.key().as_str()));
        drop(holding);

        let listed = host_sessions(&client, facts(false, crate::client::timestamp()));
        assert!(listed.sessions.is_empty(), "gone with its note");
    }

    #[test]
    fn a_session_on_its_first_command_is_named_by_its_host_and_folder_as_its_record_will_be() {
        use crate::host_session::{Host, HostSession, store::note};

        let directory = tempfile::tempdir().unwrap();
        let client = crate::transaction_test::client(directory.path());
        let first = HostSession::new(Host::Codex, "first")
            .unwrap()
            .in_folder(Some("left"));
        let holding = client
            .controller
            .clone()
            .labelled(Some(&first))
            .acquire(true)
            .unwrap();

        let listed = host_sessions(&client, facts(false, crate::client::timestamp()));
        let entry = &listed.sessions[0];
        assert_eq!(entry["name"], "Codex · left");
        assert_eq!(entry["folder"], "left");

        // Another session of that host in that folder: both gain their
        // start times, as two records would.
        let other = HostSession::new(Host::Codex, "other")
            .unwrap()
            .in_folder(Some("left"));
        note(
            &client,
            &other,
            crate::host_session::store::Kind::Other,
            Some(&crate::doing::Doing::new("click")),
            &Ok(json!({"ok": true})),
        )
        .unwrap();
        let listed = host_sessions(&client, facts(false, crate::client::timestamp()));
        let names: BTreeMap<&str, &str> = listed
            .sessions
            .iter()
            .map(|entry| {
                (
                    entry["key"].as_str().unwrap(),
                    entry["name"].as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(names.len(), 2);
        for key in [first.key(), other.key()] {
            assert!(
                names[key.as_str()].starts_with("Codex · left · "),
                "{names:?}"
            );
        }
        assert_ne!(names[first.key().as_str()], names[other.key().as_str()]);
        drop(holding);

        let named = HostSession::new(Host::Named, "release")
            .unwrap()
            .in_folder(Some("left"));
        let holding = client
            .controller
            .clone()
            .labelled(Some(&named))
            .acquire(true)
            .unwrap();
        let listed = host_sessions(&client, facts(false, crate::client::timestamp()));
        let entry = listed
            .sessions
            .iter()
            .find(|entry| entry["key"] == named.key().as_str())
            .unwrap();
        assert_eq!(
            entry["name"], "release · left",
            "a named session by its label"
        );
        drop(holding);
    }

    #[test]
    fn a_first_commands_name_stays_once_its_record_is_written() {
        use crate::host_session::{Host, HostSession, Process, store::note};

        let directory = tempfile::tempdir().unwrap();
        let client = crate::transaction_test::client(directory.path());
        let worked = |session: &HostSession| {
            note(
                &client,
                session,
                crate::host_session::store::Kind::Other,
                Some(&crate::doing::Doing::new("click")),
                &Ok(json!({"ok": true})),
            )
            .unwrap();
        };
        let other = HostSession::new(Host::Codex, "other")
            .unwrap()
            .in_folder(Some("left"));
        worked(&other);
        // This test process stands for the first command: its notes and
        // its record both start when it did.
        let first = HostSession::new(Host::Codex, "first")
            .unwrap()
            .in_folder(Some("left"))
            .begun(Process::current().map(|process| process.started_ms()));
        let holding = client
            .controller
            .clone()
            .labelled(Some(&first))
            .acquire(true)
            .unwrap();
        let name = |listed: &HostSessions| {
            listed
                .sessions
                .iter()
                .find(|entry| entry["key"] == first.key().as_str())
                .map(|entry| (entry["name"].clone(), entry["started"].clone()))
                .unwrap()
        };

        let before = name(&host_sessions(
            &client,
            facts(false, crate::client::timestamp()),
        ));
        assert!(before.0.as_str().unwrap().starts_with("Codex · left · "));
        worked(&first);
        let after = name(&host_sessions(
            &client,
            facts(false, crate::client::timestamp()),
        ));
        assert_eq!(after, before);
        drop(holding);
    }

    #[test]
    fn a_session_at_work_has_never_ended_whatever_its_record_says() {
        use crate::host_session::{
            Host, HostSession, Process,
            store::{Kind, note, touch},
        };

        let directory = tempfile::tempdir().unwrap();
        let client = crate::transaction_test::client(directory.path());
        let first = HostSession::new(Host::Claude, "resumed")
            .unwrap()
            .run_by(Some(Process::new(i32::MAX, [1, 0])));
        note(
            &client,
            &first,
            Kind::Other,
            Some(&crate::doing::Doing::new("click")),
            &Ok(json!({"ok": true})),
        )
        .unwrap();
        let listed = host_sessions(&client, facts(false, crate::client::timestamp()));
        assert!(listed.sessions.is_empty(), "the process it names is gone");

        // Resumed in a new process, the one this command runs in: its
        // command holds the helper before anything is recorded.
        let parent = nix::unistd::getppid().as_raw();
        let host = crate::host_session::ancestry::process(parent).unwrap();
        let resumed = HostSession::new(Host::Claude, "resumed")
            .unwrap()
            .run_by(Some(Process::new(parent, host.start)));
        let holding = client
            .controller
            .clone()
            .labelled(Some(&resumed))
            .acquire(true)
            .unwrap();
        let listed = host_sessions(&client, facts(false, crate::client::timestamp()));
        let entry = &listed.sessions[0];
        assert_eq!(entry["state"], "acting");
        assert_eq!(
            (&entry["live"], &entry["ended"], &entry["active"]),
            (&json!(true), &Value::Null, &json!(true))
        );
        drop(holding);

        // Any command of it refreshes its record, a read too.
        touch(&client, &resumed).unwrap();
        let listed = host_sessions(&client, facts(false, crate::client::timestamp()));
        let entry = &listed.sessions[0];
        assert_eq!(entry["state"], "idle");
        assert_eq!(
            (&entry["live"], &entry["ended"], &entry["active"]),
            (&json!(true), &Value::Null, &json!(true))
        );
    }

    #[test]
    fn an_ended_session_waits_for_no_recovery() {
        use crate::host_session::{Host, HostSession, store::note};

        let directory = tempfile::tempdir().unwrap();
        let client = crate::transaction_test::client(directory.path());
        let session = HostSession::new(Host::Codex, "done").unwrap();
        note(
            &client,
            &session,
            crate::host_session::store::Kind::Other,
            Some(&crate::doing::Doing::new("click")),
            &Ok(json!({"ok": true})),
        )
        .unwrap();
        let output = client.output_dir().unwrap().to_path_buf();
        let now = crate::client::timestamp();
        let row = json!({"at": (now - 1_000_000_000).to_string(), "operation": "input",
            "ok": true, "actor": "agent", "verb": "click", "session": session.key()});
        fs::write(output.join("metadata.jsonl"), format!("{row}\n")).unwrap();

        let listed = host_sessions(&client, facts(true, now));
        assert_eq!(listed.sessions[0]["waitingFor"], "recovery");

        let path = output.join(format!("sessions/{}.json", session.key()));
        let mut record: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        record["ended"] = record["lastSeen"].clone();
        // Renamed into place, as every record is written, so status reads
        // see the folder changed.
        let partial = path.with_extension("json.partial");
        fs::write(&partial, record.to_string()).unwrap();
        fs::rename(&partial, &path).unwrap();
        let listed = host_sessions(&client, facts(true, now));
        assert!(listed.sessions.is_empty(), "{:?}", listed.sessions);
    }

    #[test]
    fn ended_sessions_are_neither_listed_nor_counted_as_more() {
        use crate::host_session::{Host, HostSession, store::note};

        let directory = tempfile::tempdir().unwrap();
        let client = crate::transaction_test::client(directory.path());
        let sessions: Vec<HostSession> = (0..12)
            .map(|n| HostSession::new(Host::Codex, &format!("worker-{n}")).unwrap())
            .collect();
        for session in &sessions {
            note(
                &client,
                session,
                crate::host_session::store::Kind::Other,
                Some(&crate::doing::Doing::new("click")),
                &Ok(json!({"ok": true})),
            )
            .unwrap();
        }
        let folder = client.output_dir().unwrap().join("sessions");
        for session in &sessions[2..] {
            let path = folder.join(format!("{}.json", session.key()));
            let mut record: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            record["ended"] = record["lastSeen"].clone();
            fs::write(&path, record.to_string()).unwrap();
        }

        let listed = host_sessions(&client, facts(false, crate::client::timestamp()));
        assert_eq!(listed.sessions.len(), 2, "ten ended");
        assert_eq!(listed.more, 0);
        assert!(listed.sessions.iter().all(|entry| entry["live"] == true));
    }

    #[test]
    fn an_overview_with_no_records_has_no_sessions() {
        let directory = tempfile::tempdir().unwrap();
        let client = crate::transaction_test::client(directory.path());
        let listed = host_sessions(&client, facts(false, crate::client::timestamp()));
        assert!(listed.sessions.is_empty() && !listed.shown);
        assert_eq!((listed.more, listed.holder), (0, None));
    }

    #[test]
    fn a_focus_refusal_has_a_plain_reason() {
        let row = json!({"operation": "input", "ok": false, "code": "focus-changed",
            "verb": "type", "app": "TextEdit"});
        let shown = shown(&row, NOW);
        assert_eq!(shown["reason"], "Another app was in front");
        assert_eq!(shown["says"], "didn't type in TextEdit");
    }

    #[test]
    fn a_focus_refusal_atlas_could_not_read_says_so_rather_than_blaming_another_app() {
        let reason = |why: &str| {
            let row = json!({"operation": "input", "ok": false, "code": "focus-changed",
                "verb": "click", "reason": why});
            shown(&row, NOW)["reason"].clone()
        };
        assert_eq!(
            reason("unreadable"),
            "Atlas couldn't tell which app was in front"
        );
        for other in ["app", "point"] {
            assert_eq!(reason(other), "Another app was in front", "{other}");
        }
        let unreadable = json!({"operation": "input", "ok": false, "code": "focus-changed",
            "reason": "unreadable"});
        assert_eq!(shown(&unreadable, NOW)["nothingSent"], true);
    }

    #[test]
    fn a_focus_refusal_sent_nothing_and_is_not_uncertain() {
        let refused = shown(
            &json!({"operation": "input", "ok": false, "code": "focus-changed",
                "verb": "type", "app": "Notes", "partial": true}),
            NOW,
        );
        assert_eq!(refused["ok"], false, "older apps still show it as not done");
        assert_eq!(refused["nothingSent"], true);
        assert_eq!(refused["partial"], true);
        assert_eq!(refused["uncertain"], false);
        assert_eq!(refused["says"], "stopped typing in Notes");

        let unknown = shown(
            &json!({"operation": "input", "ok": false, "code": "uncertain", "verb": "click"}),
            NOW,
        );
        assert_eq!(unknown["nothingSent"], false);
        assert_eq!(unknown["partial"], false);
        assert_eq!(unknown["uncertain"], true);
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
            ..Pending::default()
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
    fn input_failure_after_a_stop_offers_recovery_and_keeps_agents_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, dir.path());
        controller.note_stop(true).unwrap();
        let mut held = report(true, true);
        held["session"]["inputService"]["held"] = json!(true);
        assert_eq!(mode(&held, false, 0, &[]), "stopped");

        held["ready"] = json!(false);
        held["nextSteps"] = json!([crate::session::START_INPUT]);
        let summary = overview(&held, false, pending(&controller).0, &[], NOW);
        assert_eq!(summary["mode"], "blocked");
        assert_eq!(summary["state"], "needsYou");
        assert_eq!(summary["recoveryHelps"], true);
        assert!(controller.stop_noted());
        assert_eq!(held["session"]["inputService"]["held"], true);

        held["ready"] = json!(true);
        let recovered = overview(&held, false, pending(&controller).0, &[], NOW);
        assert_eq!(recovered["mode"], "stopped");
        assert_eq!(recovered["recoveryHelps"], false);
        assert!(controller.stop_noted());
        assert_eq!(held["session"]["inputService"]["held"], true);

        controller.note_stop(false).unwrap();
        held["session"]["inputService"]["held"] = json!(false);
        assert_eq!(mode(&held, false, 0, &[]), "observing");
    }

    #[test]
    fn a_permission_failure_after_a_stop_shows_what_must_be_allowed_before_resuming() {
        let dir = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, dir.path());
        controller.note_stop(true).unwrap();
        let mut held = report(true, true);
        held["session"]["inputService"]["held"] = json!(true);
        assert_eq!(mode(&held, false, 0, &[]), "stopped");

        held["ready"] = json!(false);
        held["nextSteps"] = json!(["Turn on Accessibility for Atlas in the helper first."]);
        let summary = overview(&held, false, pending(&controller).0, &[], NOW);
        assert_eq!(summary["mode"], "blocked");
        assert_eq!(summary["state"], "notReady");
        assert_eq!(summary["reason"], held["nextSteps"][0]);
        assert_eq!(summary["recoveryHelps"], false);
        assert!(controller.stop_noted());
        assert_eq!(held["session"]["inputService"]["held"], true);

        held["ready"] = json!(true);
        assert_eq!(mode(&held, false, 0, &[]), "stopped");
    }

    #[test]
    fn an_unknown_outcome_keeps_its_claim_while_recovery_waits_for_a_prerequisite() {
        let dir = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, dir.path());
        drop(controller.quarantine().unwrap());
        let claims = controller.pending().unwrap();
        let mut report = report(true, true);
        let summary = overview(&report, false, pending(&controller).0, &[], NOW);
        assert_eq!(summary["mode"], "recovering");
        assert_eq!(summary["recoveryHelps"], true);

        for step in [
            "Turn on Accessibility for Atlas in the helper first.",
            "Allow Screen Recording for Atlas in the helper first.",
            "Switch back to your own account without logging the helper out.",
        ] {
            report["ready"] = json!(false);
            report["nextSteps"] = json!([step]);
            let summary = overview(&report, false, pending(&controller).0, &[], NOW);
            assert_eq!(summary["mode"], "recovering");
            assert_eq!(summary["state"], "needsYou");
            assert_eq!(summary["pending"], 1);
            assert_eq!(summary["reason"], step);
            assert_eq!(summary["recoveryHelps"], false);
            assert_eq!(controller.pending().unwrap(), claims);
        }

        report["nextSteps"] = json!([crate::session::START_INPUT]);
        let summary = overview(&report, false, pending(&controller).0, &[], NOW);
        assert_eq!(summary["mode"], "recovering");
        assert_eq!(summary["recoveryHelps"], true);
        assert_eq!(controller.pending().unwrap(), claims);

        report["ready"] = json!(true);
        assert_eq!(mode(&report, false, 1, &[]), "recovering");
        controller.clear().unwrap();
        assert_eq!(
            overview(&report, false, pending(&controller).0, &[], NOW)["mode"],
            "observing"
        );
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
            in_flight: 1,
            ..Pending::default()
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
    fn recovery_says_it_waits_for_a_running_command() {
        let ready = report(true, true);
        let waiting = Pending {
            unresolved: 1,
            in_flight: 1,
            commands: 1,
        };

        let shown = overview(&ready, false, waiting, &[], NOW);
        assert_eq!(shown["mode"], "recovering");
        assert_eq!(shown["commands"], 1);
        assert_eq!(shown["recoveryHelps"], false, "recovery waits for it");
        assert_eq!(
            overview(&ready, false, unresolved(1), &[], NOW)["recoveryHelps"],
            true
        );
        assert_eq!(
            shown["reason"],
            "An action's outcome is unknown; recovery waits for the running command to end"
        );
        assert_eq!(
            overview(&ready, false, unresolved(1), &[], NOW)["reason"],
            "An action's outcome is unknown; recover before taking control",
            "without a running command recovery can go ahead"
        );
    }

    #[test]
    fn control_says_a_command_is_still_running() {
        let ready = report(true, true);
        let running = Pending {
            in_flight: 1,
            commands: 1,
            ..Pending::default()
        };
        let input = Pending {
            in_flight: 1,
            ..Pending::default()
        };

        let shown = overview(&ready, true, running, &[], NOW);
        assert_eq!(shown["mode"], "controlling");
        assert_eq!(
            shown["reason"],
            "You have control. A command an agent started is still running"
        );
        assert_eq!(
            overview(&ready, true, input, &[], NOW)["reason"],
            "You have control. The agent waits",
            "the person's own input under way is no command"
        );
        assert_eq!(
            overview(&ready, false, running, &[], NOW)["reason"],
            "Work is in progress in the helper"
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

    #[tokio::test]
    async fn a_look_that_hangs_runs_once_and_no_status_read_waits_past_its_deadline() {
        use std::sync::{Arc, atomic::AtomicUsize, atomic::Ordering};

        let shared = shared::<usize>();
        let runs = Arc::new(AtomicUsize::new(0));
        let (release, gate) = std::sync::mpsc::channel::<()>();
        let gate = Arc::new(Mutex::new(gate));
        let look = || {
            let gate = Arc::clone(&gate);
            let count = counting(&runs);
            move || {
                let run = count();
                let _ = gate.lock().unwrap().recv();
                run
            }
        };

        for _ in 0..50 {
            let until = Instant::now() + Duration::from_millis(20);
            assert_eq!(shared.get(503, until, look()).await, None);
            assert!(
                Instant::now() < until + Duration::from_millis(500),
                "the read ended at its deadline"
            );
        }
        assert_eq!(runs.load(Ordering::SeqCst), 1, "only one look ran");

        drop(release);
        while shared.lock().running {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(
            shared.get(503, later(), look()).await,
            Some(1),
            "the look given up on kept what it found"
        );
        assert_eq!(runs.load(Ordering::SeqCst), 1);

        age(shared, TEST_REUSE + Duration::from_millis(100));
        assert_eq!(shared.get(503, later(), look()).await, Some(2));
        assert_eq!(runs.load(Ordering::SeqCst), 2, "an old look is taken again");
    }

    #[tokio::test]
    async fn a_fresh_look_is_reused_and_an_old_one_taken_again() {
        let shared = shared::<usize>();
        let runs = std::sync::Arc::default();

        assert_eq!(shared.get(503, later(), counting(&runs)).await, Some(1));
        age(shared, TEST_REUSE - Duration::from_millis(500));
        assert_eq!(
            shared.get(503, later(), counting(&runs)).await,
            Some(1),
            "a fresh look is reused"
        );

        age(shared, Duration::from_millis(600));
        assert_eq!(
            shared.get(503, later(), counting(&runs)).await,
            Some(2),
            "an old look is taken again"
        );
    }

    #[tokio::test]
    async fn a_look_is_never_reused_for_another_helper() {
        let shared = shared::<usize>();
        let runs = std::sync::Arc::default();

        assert_eq!(shared.get(503, later(), counting(&runs)).await, Some(1));
        assert_eq!(shared.get(504, later(), counting(&runs)).await, Some(2));
        assert_eq!(shared.get(503, later(), counting(&runs)).await, Some(3));
    }
    /// A look that waits until `gate` is released or dropped, then gives
    /// `found`.
    fn held(
        gate: &std::sync::Arc<Mutex<std::sync::mpsc::Receiver<()>>>,
        found: usize,
    ) -> impl FnOnce() -> usize + Send + 'static {
        let gate = std::sync::Arc::clone(gate);
        move || {
            let _ = gate.lock().unwrap().recv();
            found
        }
    }

    #[tokio::test]
    async fn a_look_somewhat_old_is_given_while_a_new_one_runs() {
        let shared = shared::<usize>();
        assert_eq!(shared.get(503, later(), || 1).await, Some(1));
        age(shared, TEST_REUSE + Duration::from_millis(100));

        let (release, gate) = std::sync::mpsc::channel::<()>();
        let gate = std::sync::Arc::new(Mutex::new(gate));
        let soon = Instant::now() + Duration::from_millis(20);
        assert_eq!(
            shared.get(503, soon, held(&gate, 2)).await,
            Some(1),
            "the caller whose look ran late gets the last result"
        );
        assert_eq!(
            shared.get(503, later(), || 3).await,
            Some(1),
            "another caller gets it while the look runs"
        );
        drop(release);
    }

    #[tokio::test]
    async fn a_look_too_old_to_trust_is_unknown_while_a_new_one_runs() {
        let shared = shared::<usize>();
        assert_eq!(shared.get(503, later(), || 1).await, Some(1));
        age(shared, TEST_REUSE * 2 + Duration::from_millis(100));

        let (release, gate) = std::sync::mpsc::channel::<()>();
        let gate = std::sync::Arc::new(Mutex::new(gate));
        let soon = Instant::now() + Duration::from_millis(20);
        assert_eq!(
            shared.get(503, soon, held(&gate, 2)).await,
            None,
            "the caller whose look ran late does not get the old result"
        );
        assert_eq!(
            shared.get(503, later(), || 3).await,
            None,
            "nor does another caller while the look runs"
        );
        drop(release);
    }

    #[test]
    fn why_the_helper_is_silent_is_looked_at_again_within_seconds() {
        assert!(ABSENCE.fresh_for <= Duration::from_secs(5));
        assert!(ABSENCE.fresh_for < LOGINS.fresh_for);
    }
}
