//! Which host sessions the apps show, and in what state: who has the
//! helper, who waits for it and whose action may not have finished. One
//! rule for the menu bar panel, the viewer, the chat apps' view and the
//! agents' panes, so they never disagree. A session is a label for the
//! person: nothing here is read by any check of who may act.
use crate::{
    activity_log::{Op, Row},
    control::{Cause, Note},
    environment::Phase,
    helper_info::Capture,
    host_session::{
        Host, Key,
        names::{name_of, names},
        record::Record,
        store::IDLE_FOR,
    },
    task::{
        Owned, State as Claimed,
        record::{CleanupState, Record as TaskRecord},
    },
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

/// How long after its last action a session stays live.
pub(crate) const LIVE_FOR: Duration = Duration::from_secs(60 * 60);
/// How recently a session acted for it to be waiting, with every other,
/// for the person to recover.
const RECOVERY_WAIT: Duration = Duration::from_secs(2 * 60);
/// How recently an agent's row under way began for its session to stand
/// for the one with the helper, when no holder note names one.
const UNDER_WAY: Duration = Duration::from_secs(5);
/// The name of the commands no host named.
pub(crate) const TERMINAL: &str = "Terminal";
/// The most sessions the viewer's history lists.
const HISTORY: usize = 64;
/// The most screenshots listed for one session in the history.
const HISTORY_SHOTS: usize = 12;

/// A session's state, in the order the apps list sessions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum State {
    /// An action of its own may not have finished: the person recovers.
    NeedsYou,
    /// It has the helper, or a command it started with `run` still runs.
    Acting,
    /// It waits for the helper, or for the person to recover.
    Waiting,
    Idle,
    Ended,
}

/// What a waiting session waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum WaitingFor {
    /// Another command has the helper.
    Helper,
    /// An outcome is unknown, so every session waits for the person.
    Recovery,
}

/// The session a state is read for: a recorded host session, or the
/// agents' commands no host named, shown as [`TERMINAL`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Who {
    Host(Key),
    Terminal,
}

impl Who {
    /// Whether a row, note or cause naming `session` is this one's.
    fn names(&self, session: Option<&Key>) -> bool {
        match self {
            Self::Host(key) => session == Some(key),
            Self::Terminal => session.is_none(),
        }
    }

    /// Whether `row` is this one's: a host's by its key, Terminal's when
    /// it names no session at all. A row naming something that is not a
    /// key is nobody's.
    fn wrote(&self, row: &Row) -> bool {
        match (self, row.session.as_deref()) {
            (Self::Terminal, None) => true,
            (Self::Host(key), session) => session == Some(key.as_str()),
            _ => false,
        }
    }
}

/// The facts every session's state is read from, as one status read
/// found them.
pub(crate) struct Scene<'a> {
    /// The newest activity rows, newest first, as the log keeps them.
    pub rows: &'a [Row],
    /// The holder note, whether or not its command still runs.
    pub holder: Option<&'a Note>,
    /// The notes of the commands waiting for control now.
    pub waiting: &'a [Note],
    /// The pending changes and who claimed each.
    pub causes: &'a [Cause],
    /// Whether the status says an outcome is unknown.
    pub pending: bool,
    /// Whether the input service says an agent, not the person, holds
    /// control: with no holder note naming a session, the commands no host
    /// named hold it.
    pub agent_holds: bool,
    /// Now, in nanoseconds since 1970.
    pub now: u128,
}

/// What the apps are told of one session beyond its record.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Shown {
    /// Whether it counts among the sessions at work: not ended, or ended
    /// with an action of its own whose outcome is unknown.
    pub live: bool,
    pub state: State,
    pub holds_helper: bool,
    pub waiting_for: Option<WaitingFor>,
    /// How long it has waited for the helper, in seconds.
    pub waiting_seconds: Option<u64>,
    /// Its newest row, as the apps word it.
    pub latest: Option<Value>,
    /// The id of its row whose outcome is unknown, while it needs the
    /// person.
    pub uncertain_row: Option<String>,
    /// What that row did: "click", "type" and so on.
    pub uncertain_verb: Option<String>,
}

impl Shown {
    /// Whether it acts or waits: a command of it holds or waits for the
    /// helper, or it acted just now and waits for recovery. Either way it
    /// has neither ended nor gone quiet, whatever its record says.
    pub(crate) fn at_work(&self) -> bool {
        matches!(self.state, State::Acting | State::Waiting)
    }
}

impl Scene<'_> {
    /// How `who` is shown, `ended` being whether its record says it ended.
    /// An ended session waits for nothing; a command of it that holds or
    /// waits for the helper shows it has not ended after all.
    pub(crate) fn shown(&self, who: &Who, ended: bool) -> Shown {
        let holds = self.holder().as_ref() == Some(who);
        let newest = self.rows_of(who).next();
        // Only a change a host session claimed names it: one no host
        // claimed needs the person, but blames nobody.
        let needs_you = self.pending
            && matches!(who, Who::Host(key) if self
                .causes
                .iter()
                .any(|cause| !cause.running && cause.session.as_ref() == Some(key)));
        // A command it started with `run` that still runs is its work
        // under way, whoever has control meanwhile.
        let running = self
            .causes
            .iter()
            .any(|cause| cause.command && who.names(cause.session.as_ref()));
        let note = self
            .waiting
            .iter()
            .find(|note| who.names(note.session.as_ref()));
        let recently = newest
            .and_then(|row| row.age(self.now))
            .is_some_and(|age| age < RECOVERY_WAIT.as_nanos());

        let (state, waiting_for) = if needs_you {
            (State::NeedsYou, None)
        } else if holds || running {
            (State::Acting, None)
        } else if self.pending && (note.is_some() || (recently && !ended)) {
            (State::Waiting, Some(WaitingFor::Recovery))
        } else if note.is_some() {
            (State::Waiting, Some(WaitingFor::Helper))
        } else if ended {
            (State::Ended, None)
        } else {
            (State::Idle, None)
        };

        let waiting_seconds = note
            .filter(|_| waiting_for == Some(WaitingFor::Helper))
            .map(|note| {
                let now = u64::try_from(self.now / 1_000_000).unwrap_or(u64::MAX);
                now.saturating_sub(note.since) / 1000
            });
        let uncertain = needs_you.then(|| self.cause_row(who)).flatten();

        Shown {
            live: state != State::Ended,
            state,
            holds_helper: holds,
            waiting_for,
            waiting_seconds,
            latest: newest.map(|row| self.latest(row)),
            uncertain_row: uncertain.map(|row| row.id.clone()),
            uncertain_verb: uncertain.and_then(|row| row.verb.clone()),
        }
    }

    /// When the agents' commands no host named first and last acted within
    /// [`LIVE_FOR`], in milliseconds since 1970, or now while they hold the
    /// helper: only then is [`Who::Terminal`] listed. Rows older than the
    /// oldest one naming a session are left out, as written before Atlas
    /// named sessions at all.
    pub(crate) fn terminal(&self) -> Option<(u64, u64)> {
        let named_since = self
            .rows
            .iter()
            .rev()
            .find(|row| row.session.is_some())
            .and_then(|row| row.age(self.now));
        let mut rows = self.rows_of(&Who::Terminal).filter(|row| {
            row.age(self.now).is_some_and(|age| {
                age <= LIVE_FOR.as_nanos() && named_since.is_none_or(|since| age < since)
            })
        });
        match rows.next() {
            Some(newest) => {
                let oldest = rows.last().unwrap_or(newest);
                Some((oldest.at_ms()?, newest.at_ms()?))
            }
            None => {
                let now = u64::try_from(self.now / 1_000_000).ok()?;
                (self.holder() == Some(Who::Terminal)).then_some((now, now))
            }
        }
    }

    /// The session that has the helper: the one the holder note names
    /// while its command runs; else the commands no host named while the
    /// input service says an agent holds it; else the one whose row under
    /// way began within [`UNDER_WAY`]. While an outcome is unknown no row
    /// ever stands for a holder: a command killed mid-step leaves its row
    /// under way, and only recovery runs until the outcome is settled.
    fn holder(&self) -> Option<Who> {
        if let Some(key) = self
            .holder
            .filter(|note| note.live())
            .and_then(|note| note.session.as_ref())
        {
            return Some(Who::Host(key.clone()));
        }
        if self.agent_holds {
            return Some(Who::Terminal);
        }
        if self.pending {
            return None;
        }

        let newest = self.rows.iter().find(|row| row.actor() != "viewer")?;
        let begun = newest.under_way()
            && newest
                .age(self.now)
                .is_some_and(|age| age < UNDER_WAY.as_nanos());
        if !begun {
            return None;
        }
        match newest.session.as_deref() {
            None => Some(Who::Terminal),
            Some(session) => Key::parse(session).map(Who::Host),
        }
    }

    /// The row of `who` whose outcome is unknown: the oldest of its newest
    /// unknown rows, passing over the looks that need no control, so a
    /// later command refused because of it is not taken for it.
    fn cause_row<'s>(&'s self, who: &'s Who) -> Option<&'s Row> {
        let mut cause = None;
        for row in self.rows_of(who) {
            if unknown(row) {
                cause = Some(row);
            } else if !looks(row) {
                break;
            }
        }
        cause
    }

    /// The agents' rows of `who`, newest first; never the person's own.
    fn rows_of<'s>(&'s self, who: &'s Who) -> impl Iterator<Item = &'s Row> + 's {
        self.rows
            .iter()
            .filter(move |row| row.actor() != "viewer" && who.wrote(row))
    }

    /// A row as a session's latest action: what it did in words and how
    /// long ago.
    fn latest(&self, row: &Row) -> Value {
        let shown = crate::overview::shown(row, self.now);
        json!({
            "says": shown.says,
            "verb": shown.verb,
            "ok": shown.ok,
            "started": shown.started,
            "nothingSent": shown.nothing_sent,
            "ageSeconds": shown.age_seconds,
        })
    }
}

/// One session as the viewer's history lists it, live or ended: who it
/// is, when it worked and what it made. No host id, path, plan id or copy
/// token is in it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Past {
    pub key: Key,
    pub host: Host,
    pub host_label: &'static str,
    pub name: String,
    pub folder: Option<String>,
    pub started_at: u64,
    pub last_seen_at: u64,
    /// When it ended, as its record or its host's process says; none for
    /// one that only went quiet.
    pub ended_at: Option<u64>,
    /// [`State::Ended`] once ended or quiet for [`LIVE_FOR`], else
    /// [`State::Idle`]: the status says how a live one is doing.
    pub state: State,
    /// How many actions it did.
    pub actions: u64,
    pub apps: Vec<PastApp>,
    pub copies: Vec<PastCopy>,
    pub plans: Vec<PastPlan>,
    /// The apps its tasks installed into the helper that may still be
    /// there, by name.
    pub installs: Vec<String>,
    /// How far its newest unfinished cleanup got; none when every cleanup
    /// of its work finished, or none ran.
    pub cleanup: Option<CleanupState>,
    /// Its newest screenshots still kept, by file name.
    pub screenshots: Vec<Capture>,
}

/// An app a session opened, and whether from a copy Atlas made for it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct PastApp {
    pub name: String,
    pub copied: bool,
}

/// A copy Atlas made for a session that is still there, by its app.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct PastCopy {
    pub app: Option<String>,
}

/// An environment plan a session applied, as far as it got.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PastPlan {
    pub phase: Phase,
    pub made_at: u64,
    pub can_roll_back: bool,
}

/// What the viewer's history is read from.
pub(crate) struct Records<'a> {
    /// Every record kept, newest last seen first.
    pub records: &'a [Record],
    /// The newest activity rows, newest first.
    pub rows: &'a [Row],
    /// The screenshots still kept, by file name.
    pub captures: &'a BTreeSet<String>,
    /// Each session's copies still there, by the app each is for when
    /// that can be told.
    pub copies: &'a BTreeMap<Key, Vec<Option<String>>>,
    /// Every task record kept, for what each session's tasks installed and
    /// whether their cleanup finished.
    pub tasks: &'a [TaskRecord],
    /// Now, in milliseconds since 1970.
    pub now: u64,
}

impl Records<'_> {
    /// The sessions last seen within the days records are kept for, newest
    /// first, at most [`HISTORY`].
    pub(crate) fn history(&self) -> Vec<Past> {
        let kept = crate::time::ms(IDLE_FOR);
        let live_for = crate::time::ms(LIVE_FOR);
        let named = names(self.records);

        self.records
            .iter()
            .filter(|record| self.now.saturating_sub(record.last_seen) <= kept)
            .take(HISTORY)
            .map(|record| {
                let ended_at = record.ended_at();
                let quiet = self.now.saturating_sub(record.last_seen) > live_for;
                let copies: Vec<PastCopy> = self
                    .copies
                    .get(&record.key)
                    .into_iter()
                    .flatten()
                    .map(|app| PastCopy { app: app.clone() })
                    .collect();
                Past {
                    key: record.key.clone(),
                    host: record.host,
                    host_label: record.host.label(),
                    name: name_of(&named, &record.key, Some(record.host)),
                    folder: record.folder.clone(),
                    started_at: record.started,
                    last_seen_at: record.last_seen,
                    ended_at,
                    state: if ended_at.is_some() || quiet {
                        State::Ended
                    } else {
                        State::Idle
                    },
                    actions: record.counts.values().sum(),
                    apps: self.apps(record, &copies),
                    copies,
                    plans: record
                        .plans
                        .iter()
                        .map(|plan| PastPlan {
                            phase: plan.phase,
                            made_at: plan.at,
                            can_roll_back: matches!(plan.phase, Phase::Applied | Phase::Applying),
                        })
                        .collect(),
                    installs: self.installs(&record.key),
                    cleanup: self.cleanup(&record.key),
                    screenshots: record
                        .screenshots
                        .iter()
                        .filter(|capture| self.captures.contains(capture.as_str()))
                        .take(HISTORY_SHOTS)
                        .cloned()
                        .collect(),
                }
            })
            .collect()
    }

    /// The apps `key`'s tasks installed that may still be there, by name.
    fn installs(&self, key: &Key) -> Vec<String> {
        self.tasks
            .iter()
            .filter(|task| task.session == *key)
            .flat_map(TaskRecord::claims)
            .filter_map(|(owned, state)| match owned {
                Owned::Install { app, .. } if matches!(state, Claimed::Intent | Claimed::Made) => {
                    Some(app)
                }
                _ => None,
            })
            .collect()
    }

    /// How far the newest unfinished cleanup of `key`'s work got.
    fn cleanup(&self, key: &Key) -> Option<CleanupState> {
        self.tasks
            .iter()
            .filter(|task| task.session == *key)
            .filter_map(TaskRecord::unfinished_cleanup)
            .max_by_key(|note| note.updated)
            .map(|note| note.state)
    }

    /// The apps `record`'s session opened, as its rows still say, newest
    /// first and each once.
    fn apps(&self, record: &Record, copies: &[PastCopy]) -> Vec<PastApp> {
        let mut apps: Vec<PastApp> = Vec::new();
        let opened = self
            .rows
            .iter()
            .filter(|row| row.session.as_deref() == Some(record.key.as_str()))
            .filter(|row| row.verb.as_deref() == Some("open") && row.worked())
            .filter_map(|row| row.app.as_deref());
        for name in opened {
            if apps.iter().all(|app| app.name != name) {
                apps.push(PastApp {
                    name: name.to_owned(),
                    copied: copies.iter().any(|copy| copy.app.as_deref() == Some(name)),
                });
            }
        }
        apps
    }
}

/// Whether the apps speak of sessions: two or more are live.
pub(crate) fn sessions_shown<'a>(shown: impl IntoIterator<Item = &'a Shown>) -> bool {
    shown.into_iter().filter(|shown| shown.live).count() >= 2
}

/// Whether `row`'s outcome may be unknown: refused as uncertain, or a
/// step whose command never said how it ended. Rows flag only the first
/// as `uncertain`; a session needing the person names either.
fn unknown(row: &Row) -> bool {
    row.coded("uncertain") || row.under_way()
}

/// Whether `row` only looked at the helper, which takes no control: a
/// screenshot or a list of its apps.
fn looks(row: &Row) -> bool {
    matches!(row.operation, Some(Op::Screenshot | Op::Apps))
}

#[cfg(test)]
mod tests;
