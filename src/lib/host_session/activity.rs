//! `atlas activity`: what a host session did in the helper, or every
//! session's, read from the activity log, the session records, the copy
//! folders and the marker. It changes nothing and never takes control, so
//! it works while another command or the person has control, and after a
//! Stop. Only the caller's own session is told its host's id, or to recover.
use super::{
    Asker, Host, HostSession, Key, Recovery,
    names::{name_of, names},
    record::{Leftovers, Record},
    store::Store,
};
use crate::{
    activity_log::{self, Row},
    client::Client,
    control::Cause,
    error::{Code, Error, Result},
    files::Token,
    handoff,
    task::Owned,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

/// How many rows of the activity log are looked through for a session's.
const ROWS: usize = 256;
/// How many of a session's screenshots are listed.
const SCREENSHOTS: usize = 5;

/// Whose activity a list shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope<'a> {
    /// The caller's own session's, or everyone's when it has none.
    Own,
    /// Every session's.
    Everyone,
    /// One session's, by its key.
    Session(&'a Key),
}

/// What a host session did, or every session's, as `scope` says: read
/// from the activity log, the session records, the copy folders and the
/// marker; nothing is changed. At most `limit` recent actions are listed.
pub fn list(
    client: &Client,
    own: Option<&HostSession>,
    scope: Scope,
    limit: usize,
) -> Result<Value> {
    let output = client.output_dir()?;
    let store = Store::at(output);
    let records = store.all();
    let reading = Reading {
        output,
        names: names(&records),
        causes: client.controller.pending_causes().unwrap_or_default(),
        holder: client
            .controller
            .holder_note()
            .filter(|note| note.live())
            .and_then(|note| note.session),
        rows: activity_log::recent(output, ROWS),
        now: crate::client::timestamp(),
        store: &store,
        records: &records,
        tasks: crate::task::store::Store::at(output).all(),
    };

    if let Scope::Session(key) = scope {
        let record = reading.record(key).ok_or_else(|| {
            Error::new(
                Code::Input,
                format!("No session {key} is recorded here; atlas activity --all lists them"),
            )
        })?;
        let own = own.filter(|own| own.key() == key);
        return Ok(reading.one(&Subject::of(record, own), limit));
    }

    match own {
        Some(own) if scope == Scope::Own => Ok(reading.one(&reading.subject(own), limit)),
        _ => Ok(reading.everyone(own, limit)),
    }
}

/// The session a reply is about: its record when it has one yet, and the
/// caller's own session when it is that one.
struct Subject<'a> {
    key: &'a Key,
    host: Host,
    record: Option<&'a Record>,
    own: Option<&'a HostSession>,
}

impl<'a> Subject<'a> {
    fn of(record: &'a Record, own: Option<&'a HostSession>) -> Self {
        Self {
            key: &record.key,
            host: record.host,
            record: Some(record),
            own,
        }
    }
}

/// Everything one reply is read from.
struct Reading<'a> {
    output: &'a Path,
    store: &'a Store,
    records: &'a [Record],
    names: BTreeMap<Key, String>,
    causes: Vec<Cause>,
    /// The session holding the helper, when its command still runs.
    holder: Option<Key>,
    rows: Vec<Row>,
    /// Every task record, for what each session's tasks left.
    tasks: Vec<crate::task::record::Record>,
    now: u128,
}

impl Reading<'_> {
    fn record(&self, key: &Key) -> Option<&Record> {
        self.records.iter().find(|record| record.key == *key)
    }

    /// The subject `own` is: its record, or itself before it has one.
    fn subject<'s>(&'s self, own: &'s HostSession) -> Subject<'s> {
        match self.record(own.key()) {
            Some(record) => Subject::of(record, Some(own)),
            None => Subject {
                key: own.key(),
                host: own.host(),
                record: None,
                own: Some(own),
            },
        }
    }

    fn name(&self, key: &Key, host: Host) -> String {
        name_of(&self.names, key, Some(host))
    }

    fn leftovers(&self, record: &Record) -> Leftovers {
        self.store.leftovers(record, &self.causes)
    }

    /// The apps `key`'s tasks installed into the helper that may still be
    /// there, by name, never by path.
    fn installs(&self, key: &Key) -> Vec<String> {
        crate::task::record::installs_of(&self.tasks, key)
    }

    /// The background jobs `key`'s tasks started, as their records last
    /// noted them: by ID, name and state, never their arguments.
    fn jobs(&self, key: &Key) -> Vec<Value> {
        self.tasks
            .iter()
            .filter(|task| task.session == *key)
            .flat_map(|task| &task.jobs)
            .map(|job| json!({"id": job.id, "name": job.name, "state": job.state}))
            .collect()
    }

    /// The collections `key`'s tasks made: each receipt, and how many files
    /// and bytes it kept, never their names.
    fn outputs(&self, key: &Key) -> Vec<Value> {
        self.tasks
            .iter()
            .filter(|task| task.session == *key)
            .flat_map(|task| &task.outputs)
            .map(|output| {
                json!({"receipt": output.receipt, "count": output.files, "bytes": output.bytes})
            })
            .collect()
    }

    /// How far the newest unfinished cleanup of `key`'s work got, when one
    /// has not finished.
    fn cleanup(&self, key: &Key) -> Option<crate::task::record::CleanupState> {
        crate::task::record::unfinished_cleanup_of(&self.tasks, key)
    }

    /// The session object a reply names: who it is and when it acted, and
    /// the host's own id only to the session itself.
    fn session(&self, subject: &Subject) -> Value {
        let record = subject.record;
        let mut session = json!({
            "key": subject.key,
            "host": subject.host,
            "name": self.name(subject.key, subject.host),
            "started": record.map(|record| record.started),
            "lastSeen": record.map(|record| record.last_seen),
            "ended": record.and_then(Record::ended_at),
            "this": subject.own.is_some(),
        });
        if let Some(own) = subject.own {
            session["hostSession"] = json!(own.id());
        }
        session
    }

    /// One session's reply: its counts, recent actions, screenshots, copies,
    /// plans and unknown actions, and the session before it when that one
    /// left something. It says to recover only to the session itself, when
    /// it may.
    fn one(&self, subject: &Subject, limit: usize) -> Value {
        let leftovers = subject.record.map(|record| self.leftovers(record));
        let uncertain = self
            .causes
            .iter()
            .filter(|cause| cause.session.as_ref() == Some(subject.key) && !cause.running)
            .count();
        let recent: Vec<Value> = self
            .rows
            .iter()
            .filter(|row| row.session.as_deref() == Some(subject.key.as_str()))
            .take(limit)
            .map(|row| self.row(row))
            .collect();

        let mut reply = json!({
            "ok": true,
            "session": self.session(subject),
            "counts": subject.record.map(|record| json!(record.counts)).unwrap_or_else(|| json!({})),
            "recent": recent,
            "screenshots": subject.record.map(|record| self.screenshots(record)).unwrap_or_default(),
            "copies": leftovers
                .as_ref()
                .map(|leftovers| self.copies(&leftovers.copies))
                .unwrap_or_default(),
            "plans": subject.record.map(plans).unwrap_or_default(),
            "uncertain": uncertain,
            "tasks": subject.record.map(|record| self.tasks(record)).unwrap_or_default(),
        });
        if let Some(previous) = subject.record.and_then(|record| self.previous(record)) {
            reply["previous"] = previous;
        }
        let installs = self.installs(subject.key);
        if !installs.is_empty() {
            reply["installs"] = installs.iter().map(|app| json!({"app": app})).collect();
        }
        let jobs = self.jobs(subject.key);
        if !jobs.is_empty() {
            reply["jobs"] = json!(jobs);
        }
        let outputs = self.outputs(subject.key);
        if !outputs.is_empty() {
            reply["outputs"] = json!(outputs);
        }
        if let Some(state) = self.cleanup(subject.key) {
            reply["cleanup"] = json!(state);
        }
        // Only the session itself may recover, and only when every
        // unknown action is its own and none is still being carried out.
        if subject.own.is_some()
            && let Some(recovery) = Recovery::of(&self.causes, Asker::Agent(Some(subject.key)))
            && recovery.own()
        {
            reply["next"] = json!(recovery.step());
        }
        reply
    }

    /// Every session's reply: each one's counts and leftovers, never their
    /// screenshots' paths, copies, plan ids or host ids, and everyone's
    /// recent actions.
    fn everyone(&self, own: Option<&HostSession>, limit: usize) -> Value {
        let sessions: Vec<Value> = self
            .records
            .iter()
            .map(|record| {
                let leftovers = self.leftovers(record);
                let mut left = json!({
                    "copies": leftovers.copies.len(),
                    "plans": leftovers.plans,
                    "uncertain": leftovers.uncertain,
                });
                let installs = self.installs(&record.key).len();
                if installs > 0 {
                    left["installs"] = json!(installs);
                }
                if let Some(state) = self.cleanup(&record.key) {
                    left["cleanup"] = json!(state);
                }
                json!({
                    "key": record.key,
                    "host": record.host,
                    "name": self.name(&record.key, record.host),
                    "started": record.started,
                    "lastSeen": record.last_seen,
                    "ended": record.ended_at(),
                    "active": record.active(super::now_ms()),
                    "holdsHelper": self.holder.as_ref() == Some(&record.key),
                    "screenshots": record.screenshots.len(),
                    "leftovers": left,
                })
            })
            .collect();
        let recent: Vec<Value> = self
            .rows
            .iter()
            .take(limit)
            .map(|row| {
                let mut shown = self.row(row);
                shown["session"] = row
                    .session
                    .as_deref()
                    .and_then(Key::parse)
                    .map_or(Value::Null, |key| json!(name_of(&self.names, &key, None)));
                shown
            })
            .collect();

        json!({
            "ok": true,
            "session": own.map(|own| self.session(&self.subject(own))),
            "sessions": sessions,
            "recent": recent,
        })
    }

    /// One recent action, in words: what it did, whether it worked and how
    /// long ago, why not when it failed and the screenshot it took.
    fn row(&self, row: &Row) -> Value {
        let shown = crate::overview::shown(row, self.now);
        let mut said = json!({
            "says": shown.says,
            "ok": shown.ok,
            "ageSeconds": shown.age_seconds,
            "app": shown.app,
            "label": shown.label,
            "bundleId": shown.bundle_id,
            "keys": shown.keys(),
        });
        if !shown.ok
            && let Some(reason) = shown.reason
        {
            said["reason"] = json!(reason);
        }
        if let Some(capture) = &row.capture {
            said["capture"] = json!(capture);
        }
        said
    }

    /// The newest of `record`'s screenshots still in the output folder, as
    /// full paths.
    fn screenshots(&self, record: &Record) -> Vec<String> {
        record
            .screenshots
            .iter()
            .map(|name| self.output.join(name.as_str()))
            .filter(|path| std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file()))
            .take(SCREENSHOTS)
            .map(|path| path.to_string_lossy().into_owned())
            .collect()
    }

    /// Each copy folder `tokens` name, by the app it is for.
    fn copies(&self, tokens: &[Token]) -> Vec<Value> {
        tokens
            .iter()
            .map(|token| json!({"app": handoff::copy_app(self.store.copies(), token)}))
            .collect()
    }

    /// The tasks the session of `record` named with ATLAS_TASK, each with
    /// what it left: its copy folders still there, and its plans the
    /// session's record still lists as applied.
    fn tasks(&self, record: &Record) -> Vec<Value> {
        let mut tasks: Vec<&crate::task::record::Record> = self
            .tasks
            .iter()
            .filter(|task| task.session == record.key && task.name.is_some())
            .collect();
        tasks.sort_by_key(|task| task.started);
        tasks
            .into_iter()
            .map(|task| {
                let (mut copies, mut plans, mut installs) = (0, 0, 0);
                for (owned, state) in task.claims() {
                    let live = state.live();
                    match owned {
                        Owned::Copy { id }
                            if live && handoff::copy_exists(self.store.copies(), &id) =>
                        {
                            copies += 1;
                        }
                        Owned::Plan { id }
                            if live && record.plans.iter().any(|plan| plan.plan_id == id) =>
                        {
                            plans += 1;
                        }
                        Owned::Install { .. } if live => installs += 1,
                        _ => {}
                    }
                }
                let mut left = json!({"copies": copies, "plans": plans});
                if installs > 0 {
                    left["installs"] = json!(installs);
                }
                json!({"key": task.key, "name": task.name, "leftovers": left})
            })
            .collect()
    }

    /// The session `record` followed in its host process, when that one
    /// still left something in the helper.
    fn previous(&self, record: &Record) -> Option<Value> {
        let before = self.record(record.previous.as_ref()?)?;
        let leftovers = self.leftovers(before);
        (!leftovers.is_empty()).then(|| {
            json!({
                "key": before.key,
                "name": self.name(&before.key, before.host),
                "copies": leftovers.copies.len(),
                "plans": leftovers.plans,
                "uncertain": leftovers.uncertain,
            })
        })
    }
}

/// A record's started plans, by id and how far each got.
fn plans(record: &Record) -> Vec<Value> {
    record
        .plans
        .iter()
        .map(|plan| json!({"planId": plan.plan_id, "phase": plan.phase}))
        .collect()
}

#[cfg(test)]
mod tests;
