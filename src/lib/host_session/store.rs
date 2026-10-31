//! Each host session's record: one file in the private output folder's
//! `sessions` folder, saying when the session was seen, how many of each
//! action it did, and the screenshots, copies and environment plans it
//! left, by Atlas's own names only. Losing a record loses only labels: the
//! copies, plans and pending changes themselves stay and keep their own
//! cleanup. Recording is best effort and never holds a command up: when
//! another command is writing the records, this one's goes unrecorded.
use super::{
    Host, HostSession, Instance, Key, now_ms,
    record::{Leftovers, Record, Seen},
};
use crate::{
    client::Client,
    control::{Cause, Controller},
    doing::Doing,
    environment::Phase,
    error::Result,
    files::{Durability, Token},
    handoff,
    helper_info::Capture,
    record_folder::{Cached, LISTED, RecordFolder, Records},
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

/// The most records kept; those with nothing left in the helper go first.
pub const MAX_RECORDS: usize = 64;
/// How long a record with nothing left in the helper is kept after its
/// session's last recorded action.
pub const IDLE_FOR: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// The most folders looked at, from the working directory up, for the
/// project a session works in.
const FOLDER_DEPTH: usize = 32;
/// How long the copy folders counted for status reads are trusted: they
/// can go without any record changing.
const COPIES_FRESH: Duration = Duration::from_secs(30);

/// What a command was, as far as its session's record cares: nothing of
/// its arguments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Screenshot,
    Open,
    Close,
    /// An environment request: one that `changes` the helper (applies or
    /// rolls back a plan), or one that only previews.
    Environment {
        changes: bool,
    },
    Other,
}

impl Kind {
    /// Whether a command of this kind, doing `doing`, is recorded at all:
    /// one that acts in the helper, or asks about an environment plan.
    pub fn recorded(self, doing: Option<&Doing>) -> bool {
        doing.is_some() || matches!(self, Self::Environment { .. })
    }
}

/// The records status reads share, read again only once one was written,
/// or once the copy folders they counted may have changed.
static RECENT: Mutex<Option<Cached<Seen>>> = Mutex::new(None);

/// The records of one output folder.
pub struct Store {
    folder: RecordFolder<Key>,
    /// Where `open`'s copy folders are.
    copies: PathBuf,
}

/// How session records are kept: written as far as the kernel, as every
/// recorded command writes one, and waited for at most briefly, so
/// recording never holds a command up.
static SESSIONS: Records = Records {
    folder: "sessions",
    noun: "session",
    lock_wait: Duration::from_millis(250),
    durability: Durability::Kernel,
    bytes: 64 * 1024,
};

impl Store {
    pub fn at(output: &Path) -> Self {
        Self {
            folder: RecordFolder::at(output, &SESSIONS),
            copies: PathBuf::from(handoff::ROOT),
        }
    }

    /// This store, with copy folders under `root` instead.
    #[cfg(test)]
    pub(crate) fn copying_into(mut self, root: &Path) -> Self {
        self.copies = root.to_owned();
        self
    }

    /// Where `open`'s copy folders are.
    pub(crate) fn copies(&self) -> &Path {
        &self.copies
    }

    /// One record, when it is there and reads.
    pub(crate) fn read(&self, key: &Key) -> Option<Record> {
        self.folder
            .read_json::<Record>(key)
            .filter(|record| record.key == *key)
    }

    /// The newest [`MAX_RECORDS`] records, newest `lastSeen` first.
    pub(crate) fn all(&self) -> Vec<Record> {
        self.listed(MAX_RECORDS)
    }

    /// The newest `limit` records by when each was written, of the first
    /// [`LISTED`] entries in the folder, newest `lastSeen` first.
    fn listed(&self, limit: usize) -> Vec<Record> {
        self.scanned(limit).0
    }

    /// The bounded read, retaining failed record reads and listing uncertainty.
    fn scanned(&self, limit: usize) -> (Vec<Record>, bool) {
        let listing = self.folder.listing(limit);
        let mut complete = listing.complete;
        let mut records = Vec::new();
        for key in listing.keys {
            match self.read(&key) {
                Some(record) => records.push(record),
                None => complete = false,
            }
        }
        records.sort_by_key(|record| std::cmp::Reverse(record.last_seen));
        (records, complete)
    }

    /// An uncached session read for service replacement. A missing folder is
    /// empty; a failed or truncated read cannot establish that sessions are idle.
    pub(crate) fn checked_recent(&self) -> Option<Vec<Seen>> {
        let (records, complete) = self.scanned(MAX_RECORDS);
        complete.then(|| self.seen(records))
    }

    fn seen(&self, records: Vec<Record>) -> Vec<Seen> {
        records
            .into_iter()
            .map(|record| Seen {
                copies_left: self.copies_left(&record).len(),
                record,
            })
            .collect()
    }

    /// [`Store::all`] with each record's copy folders still there counted,
    /// shared by every status read in this process: a poll costs one look
    /// at the folder when no session acted.
    pub(crate) fn recent(&self) -> Arc<[Seen]> {
        self.recent_in(&RECENT, Self::all)
    }

    /// [`Store::recent`], kept in `kept` and read with `read`.
    fn recent_in(
        &self,
        kept: &Mutex<Option<Cached<Seen>>>,
        read: impl FnOnce(&Self) -> Vec<Record>,
    ) -> Arc<[Seen]> {
        self.folder
            .cached(kept, COPIES_FRESH, || self.seen(read(self)))
    }

    /// Change `session`'s record under the store's lock, making it when
    /// new, and write it whole. A new Claude record follows the one before
    /// it in the same host process, which then ends, and makes room among the
    /// others; `controller` says which pending changes are whose.
    pub(crate) fn update(
        &self,
        session: &HostSession,
        now: u64,
        controller: &Controller,
        change: impl FnOnce(&mut Record),
    ) -> Result<()> {
        let causes = controller.pending_causes_within(Duration::ZERO)?;
        self.folder.make()?;
        let _lock = self.folder.lock()?;

        let existing = self.read(session.key());
        let made = existing.is_none();
        let mut record = match existing {
            Some(record) => record,
            None => self.started_record(session, session.process(), now)?,
        };

        change(&mut record);
        self.write(&record)?;

        if made {
            self.prune(&record.key, &causes);
        }
        Ok(())
    }

    /// Attach before dispatch with zero-wait locks. Pending attribution is
    /// read before the record lock; a failed snapshot creates and prunes nothing.
    pub(crate) fn touch(
        &self,
        session: &HostSession,
        ancestor: Option<Instance>,
        now: u64,
        controller: &Controller,
    ) -> Result<()> {
        if let Some(mut record) = self.read(session.key())
            && !record.touched(session, ancestor, now)
        {
            return Ok(());
        }
        let causes = controller.pending_causes_within(Duration::ZERO)?;
        self.folder.make()?;
        let _lock = self.folder.lock_within(Duration::ZERO)?;
        if let Some(mut record) = self.read(session.key()) {
            if record.touched(session, ancestor, now) {
                self.write(&record)?;
            }
        } else {
            let record = self.started_record(session, ancestor, now)?;
            self.write(&record)?;
            self.prune(&record.key, &causes);
        }
        Ok(())
    }

    /// Start once, using the command's original start and preserving a
    /// predecessor in the same host process before writing the new record.
    fn started_record(
        &self,
        session: &HostSession,
        process: Option<Instance>,
        now: u64,
    ) -> Result<Record> {
        let mut record = Record::new(session, now, session.folder().map(str::to_owned));
        record.host_process = process;
        record.started = session.began().map_or(now, |began| began.min(now));
        if let Some(mut before) = self.before_clear(&record) {
            record.previous = Some(before.key.clone());
            before.ended = Some(now);
            self.write(&before)?;
        }
        Ok(record)
    }

    /// Forget plan `plan_id`, which a command rolled back, in every record
    /// that lists it: the plan is gone, whoever rolled it back.
    pub(crate) fn forget_plan(&self, plan_id: &Token) -> Result<()> {
        if !self.folder.exists() {
            return Ok(());
        }
        let _lock = self.folder.lock()?;
        for mut record in self.listed(LISTED) {
            if record.forget_plan(plan_id) {
                self.write(&record)?;
            }
        }
        Ok(())
    }

    /// What `record` left in the helper: its copy folders still there, its
    /// applied plans and the pending changes `causes` say it claimed.
    pub(crate) fn leftovers(&self, record: &Record, causes: &[Cause]) -> Leftovers {
        let own = causes
            .iter()
            .filter(|cause| cause.session.as_ref() == Some(&record.key));
        let running = own.clone().filter(|cause| cause.running).count();

        Leftovers {
            copies: self.copies_left(record),
            plans: record.plans.len(),
            uncertain: own.count() - running,
            running,
        }
    }

    /// The tokens of `record`'s copy folders still there.
    fn copies_left(&self, record: &Record) -> Vec<Token> {
        record
            .copies
            .iter()
            .filter(|token| handoff::copy_exists(&self.copies, token))
            .cloned()
            .collect()
    }

    /// The Claude session `record` followed through `/clear`: the newest
    /// Claude record naming that process that has not ended.
    fn before_clear(&self, record: &Record) -> Option<Record> {
        if record.host != Host::Claude {
            return None;
        }
        let process = record.host_process?;
        self.all().into_iter().find(|other| {
            other.host == Host::Claude
                && other.key != record.key
                && other.host_process == Some(process)
                && other.ended.is_none()
        })
    }

    /// Remove records idle longer than [`IDLE_FOR`] with nothing left in
    /// the helper, then the oldest while more than [`MAX_RECORDS`] remain,
    /// those with something left last. The record `keep` stays.
    fn prune(&self, keep: &Key, causes: &[Cause]) {
        let now = now_ms();
        let idle = crate::time::ms(IDLE_FOR);
        // A session whose tasks still hold something, an unfinished cleanup
        // among them, has something left too.
        let held = self
            .folder
            .directory()
            .parent()
            .map(crate::task::prune::sessions_held)
            .unwrap_or_default();
        let mut records: Vec<(Record, bool)> = self
            .listed(LISTED)
            .into_iter()
            .map(|record| {
                let empty =
                    self.leftovers(&record, causes).is_empty() && !held.contains(&record.key);
                (record, empty)
            })
            .collect();

        records.retain(|(record, empty)| {
            let stale =
                *empty && now.saturating_sub(record.last_seen) > idle && record.key != *keep;
            if stale {
                self.folder.remove(&record.key);
            }
            !stale
        });

        records.sort_by_key(|(record, _)| record.last_seen);
        while records.len() > MAX_RECORDS {
            let oldest = records
                .iter()
                .position(|(record, empty)| *empty && record.key != *keep)
                .or_else(|| records.iter().position(|(record, _)| record.key != *keep));
            let Some(oldest) = oldest else {
                break;
            };
            let (record, _) = records.remove(oldest);
            self.folder.remove(&record.key);
        }
    }

    /// Write `record` whole in its place.
    fn write(&self, record: &Record) -> Result<()> {
        self.folder.write_json(&record.key, record)
    }
}

/// Record what one command did for its session: when it acted, its verb
/// unless it was refused, and the screenshot, copies and plans its reply
/// names. Nothing of its arguments, text or output is kept. A command that
/// acts nowhere in the helper records nothing.
pub fn note(
    client: &Client,
    session: &HostSession,
    kind: Kind,
    doing: Option<&Doing>,
    result: &Result<Value>,
) -> Result<()> {
    if !kind.recorded(doing) {
        return Ok(());
    }
    let verb = doing.map_or("environment", Doing::verb);
    let verb = (!refused(result)).then_some(verb);
    let store = Store::at(client.output_dir()?);
    let reply = result.as_ref().ok();
    let now = now_ms();

    store.update(session, now, &client.controller, |record| {
        record.seen(session, now, verb);
        if let Some(reply) = reply {
            keep_what_was_left(record, kind, reply, now);
        }
    })
}

/// Note any command before dispatch, including a first read, without counting
/// an action. Only a verified ancestor may attach or replace lifetime metadata;
/// an existing root must be confirmed gone before replacement. Best effort,
/// with zero-wait locks.
pub fn touch(client: &Client, session: &HostSession) -> Result<()> {
    Store::at(client.output_dir()?).touch(session, session.ancestor(), now_ms(), &client.controller)
}

/// Forget, in every session's record, the plan a successful rollback in
/// `result` undid, whoever ran it: the person at a terminal, another
/// session or this one. It decides nothing about the rollback itself.
pub fn forget_rolled_back(client: &Client, kind: Kind, result: &Result<Value>) -> Result<()> {
    match rolled_back(kind, result).and_then(plan) {
        Some(plan) => Store::at(client.output_dir()?).forget_plan(&plan),
        None => Ok(()),
    }
}

/// The reply of a command of `kind` whose `result` says it rolled a plan
/// back completely, when it did.
pub(crate) fn rolled_back(kind: Kind, result: &Result<Value>) -> Option<&Value> {
    let reply = result.as_ref().ok()?;
    let rolled_back = kind == Kind::Environment { changes: true }
        && reply["ok"] == true
        && reply["preview"] != true
        && phase(reply) == Some(Phase::RolledBack);
    rolled_back.then_some(reply)
}

/// Note in `record` what `reply`, to a command of `kind`, says it left:
/// the screenshot it saved, the copies it made or removed, and the plan it
/// applied. A preview only looks, so it names no plan as this session's.
fn keep_what_was_left(record: &mut Record, kind: Kind, reply: &Value, now: u64) {
    match kind {
        Kind::Screenshot if reply["ok"] == true => {
            if let Some(capture) = capture_named(&reply["path"]) {
                record.took(capture);
            }
        }
        Kind::Open => record.copied(tokens(&reply["copied"], "to")),
        Kind::Close if reply["ok"] == true => {
            let gone: Vec<Token> = tokens(&reply["removed"], "").collect();
            record.removed(&gone);
        }
        Kind::Environment { changes: true } if reply["preview"] != true => {
            if let (Some(plan), Some(phase)) = (plan(reply), phase(reply)) {
                record.planned(plan, phase, now);
            }
        }
        _ => {}
    }
}

/// Whether `result` refused the command before it did anything: the helper
/// was another's, agents were stopped, or an earlier outcome is unknown.
fn refused(result: &Result<Value>) -> bool {
    let (code, uncertain) = match result {
        Err(error) => (error.code.as_str(), error.uncertain),
        Ok(value) if value["ok"] == false => (
            value["code"].as_str().unwrap_or_default(),
            value["uncertain"] == true,
        ),
        Ok(_) => return false,
    };
    !uncertain && matches!(code, "busy" | "stopped" | "uncertain")
}

/// The plan a reply names, when it is one of Atlas's own.
pub(crate) fn plan(reply: &Value) -> Option<Token> {
    reply["planId"].as_str().and_then(Token::parse)
}

/// How far a reply says its plan got.
fn phase(reply: &Value) -> Option<Phase> {
    Phase::deserialize(&reply["phase"]).ok()
}

/// The capture at `path`, when it is one of Atlas's own.
fn capture_named(path: &Value) -> Option<Capture> {
    Path::new(path.as_str()?)
        .file_name()?
        .to_str()
        .and_then(Capture::parse)
}

/// The copy folder tokens of the paths `listed` names, each one's `field`
/// (or the entry itself when `field` is empty).
fn tokens<'a>(listed: &'a Value, field: &'a str) -> impl Iterator<Item = Token> + 'a {
    listed
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(move |entry| {
            let path = if field.is_empty() {
                entry
            } else {
                &entry[field]
            };
            handoff::token_of(path.as_str()?)
        })
}

/// The project a command works in: the name of the nearest folder from
/// `directory` up that holds a `.git` entry, else of `directory` itself.
/// None for the home folder and `/`, which name no project.
pub(crate) fn project_folder(directory: &Path) -> Option<String> {
    let project = directory
        .ancestors()
        .take(FOLDER_DEPTH)
        .find(|folder| fs::symlink_metadata(folder.join(".git")).is_ok())
        .unwrap_or(directory);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if home.as_deref() == Some(project) {
        return None;
    }

    project
        .file_name()
        .and_then(|name| name.to_str())
        .map(crate::doing::bounded)
        .filter(|name| !name.is_empty())
}

#[cfg(test)]
mod tests;
