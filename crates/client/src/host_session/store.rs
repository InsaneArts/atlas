//! Each host session's record: one file in the private output folder's
//! `sessions` folder, saying when the session was seen, how many of each
//! action it did, and the screenshots, copies and environment plans it
//! left, by Atlas's own names only. Losing a record loses only labels: the
//! copies, plans and pending changes themselves stay and keep their own
//! cleanup. Recording is best effort and never holds a command up: when
//! another command is writing the records, this one's goes unrecorded.
use super::{
    Host, HostSession, Instance, Key,
    record::{Leftovers, Record, Seen, VERSION},
};
use crate::{
    client::Client, doing::Doing, environment::Phase, handoff::Copies, helper_info::Capture,
    task::State,
};
use atlas_control::{Cause, Controller};
use atlas_core::{
    error::{Code, Error, Result},
    files::{Durability, Token},
    record_folder::{Cached, LISTED, RecordFolder, Records},
    time::now_ms,
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    fs,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

/// The most records kept; those with nothing left in the helper go first.
pub(crate) const MAX_RECORDS: usize = 64;
/// How long a record with nothing left in the helper is kept after its
/// session's last recorded action.
pub(crate) const IDLE_FOR: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// The verb a record counts for environment work, which may need no helper
/// command of its own: a preview keeps no activity row.
pub(crate) const ENVIRONMENT_VERB: &str = "environment";

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
        match self {
            Self::Environment { .. } => true,
            Self::Screenshot | Self::Open | Self::Close | Self::Other => doing.is_some(),
        }
    }
}

/// Reply facts retained for `dispatch::note_outcome` to share across host and
/// task settlement. These are labels, never mutation acknowledgements.
pub struct ReplyFacts {
    kind: Kind,
    refusal: bool,
    left: Left,
}

/// Attribution observed in one reply, separate from command admission.
/// A plan's global settlement is present only for a successful rollback.
enum Left {
    None,
    Capture(Capture),
    Copied(Vec<Token>),
    Removed(Vec<Token>),
    Plan {
        id: Token,
        phase: Phase,
        settlement: Option<State>,
    },
}

impl ReplyFacts {
    /// Decode only Atlas names and settlement facts, never private reply text.
    /// Copies are known by `copies`, those of the desktop the reply came from.
    pub fn new(kind: Kind, result: &Result<Value>, copies: &Copies) -> Self {
        let refused_code = |code: &str| matches!(code, "busy" | "stopped" | "uncertain");
        let reply = match result {
            Err(error) => {
                return Self {
                    kind,
                    refusal: !error.uncertain && refused_code(error.code.as_str()),
                    left: Left::None,
                };
            }
            Ok(reply) => reply,
        };
        let success = reply["ok"] == true;
        let left = match kind {
            Kind::Screenshot if success => {
                capture_named(&reply["path"]).map_or(Left::None, Left::Capture)
            }
            Kind::Open => Left::Copied(tokens(&reply["copied"], "to", copies).collect()),
            Kind::Close if success => {
                Left::Removed(tokens(&reply["removed"], "", copies).collect())
            }
            Kind::Environment { changes: true } if reply["preview"] != true => {
                let plan = reply["planId"]
                    .as_str()
                    .and_then(Token::parse)
                    .zip(Phase::deserialize(&reply["phase"]).ok());
                plan.map_or(Left::None, |(id, phase)| {
                    let kept =
                        |field: &str| reply[field].as_array().is_some_and(|kept| !kept.is_empty());
                    let state = if kept("keptTargets") || kept("retainedDirectories") {
                        State::Kept
                    } else {
                        State::Gone
                    };
                    Left::Plan {
                        id,
                        phase,
                        settlement: (success && phase == Phase::RolledBack).then_some(state),
                    }
                })
            }
            Kind::Screenshot | Kind::Close | Kind::Environment { .. } | Kind::Other => Left::None,
        };
        Self {
            kind,
            refusal: reply["ok"] == false
                && reply["uncertain"] != true
                && reply["code"].as_str().is_some_and(refused_code),
            left,
        }
    }

    pub(crate) fn rolled_back(&self) -> Option<(&Token, State)> {
        match &self.left {
            Left::Plan { id, settlement, .. } => settlement.map(|state| (id, state)),
            Left::None | Left::Capture(_) | Left::Copied(_) | Left::Removed(_) => None,
        }
    }
}

/// The records status reads share, read again only once one was written,
/// or once the copy folders they counted may have changed.
static RECENT: Mutex<Option<Cached<Seen>>> = Mutex::new(None);

/// The records of one output folder.
pub struct Store {
    folder: RecordFolder<Key>,
    /// The copies `open` made, which its records name.
    copies: Copies,
}

/// A record file, including bytes this build must preserve.
pub(crate) enum Read {
    Missing,
    Kept,
    Record(Box<Record>),
}

impl Read {
    pub(crate) fn as_ref(&self) -> Option<&Record> {
        match self {
            Self::Record(record) => Some(record),
            Self::Missing | Self::Kept => None,
        }
    }
}

fn preserved_record() -> Error {
    Error::new(
        Code::State,
        "This session's record cannot be read by this version of Atlas, so it was left as it is",
    )
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
            copies: Copies::in_shared_folder(),
        }
    }

    /// This store, with copy folders under `root` instead.
    #[cfg(test)]
    pub(crate) fn copying_into(mut self, root: &Path) -> Self {
        self.copies = Copies::in_folder(root);
        self
    }

    /// The copies `open` made, which its records name.
    pub fn copies(&self) -> &Copies {
        &self.copies
    }

    /// One record, when it is there and reads.
    pub fn read(&self, key: &Key) -> Option<Record> {
        match self.read_file(key) {
            Read::Record(record) => Some(*record),
            Read::Missing | Read::Kept => None,
        }
    }

    pub(crate) fn read_file(&self, key: &Key) -> Read {
        match self.folder.read_json::<Record>(key) {
            Some(record) if record.key == *key && record.version <= VERSION => {
                Read::Record(Box::new(record))
            }
            Some(_) => Read::Kept,
            None => match fs::symlink_metadata(self.folder.path(key)) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Read::Missing,
                _ => Read::Kept,
            },
        }
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
    pub fn recent(&self) -> Arc<[Seen]> {
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
    ///
    /// # Errors
    ///
    /// Returns the failure to take the lock or to read or write the record.
    pub fn update(
        &self,
        session: &HostSession,
        now: u64,
        controller: &Controller,
        change: impl FnOnce(&mut Record),
    ) -> Result<()> {
        let causes = controller.pending_causes_within(Duration::ZERO)?;
        self.folder.make()?;
        let _lock = self.folder.lock()?;

        let (mut record, made) = match self.read_file(session.key()) {
            Read::Record(record) => (*record, false),
            Read::Missing => (self.started_record(session, session.process(), now)?, true),
            Read::Kept => return Err(preserved_record()),
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
        if let Read::Record(mut record) = self.read_file(session.key())
            && !record.touched(session, ancestor, now)
        {
            return Ok(());
        }
        let causes = controller.pending_causes_within(Duration::ZERO)?;
        self.folder.make()?;
        let _lock = self.folder.lock_within(Duration::ZERO)?;
        match self.read_file(session.key()) {
            Read::Record(mut record) => {
                if record.touched(session, ancestor, now) {
                    self.write(&record)?;
                }
            }
            Read::Missing => {
                let record = self.started_record(session, ancestor, now)?;
                self.write(&record)?;
                self.prune(&record.key, &causes);
            }
            Read::Kept => return Err(preserved_record()),
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

    /// Cleanup acknowledgement requires a complete supported scan, successful
    /// writes and a complete read-back under the same session-store lock.
    pub(crate) fn forget_plan_checked(&self, plan_id: &Token) -> Result<()> {
        let names_plan = |record: &Record| record.plans.iter().any(|plan| plan.plan_id == *plan_id);
        self.folder.rewrite_complete(
            cleanup_record,
            |record, modified| {
                names_plan(record).then_some((
                    std::cmp::Reverse(record.last_seen),
                    std::cmp::Reverse(modified),
                ))
            },
            |record| {
                if !record.forget_plan(plan_id) {
                    return Err(incomplete_cleanup());
                }
                self.write(record)
            },
            |record| {
                if names_plan(record) {
                    return Err(Error::new(
                        Code::State,
                        "Session records still name the plan after cleanup",
                    ));
                }
                Ok(())
            },
            incomplete_cleanup,
        )
    }

    /// What `record` left in the helper: its copy folders still there, its
    /// applied plans and the pending changes `causes` say it claimed.
    pub fn leftovers(&self, record: &Record, causes: &[Cause]) -> Leftovers {
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
            .filter(|token| self.copies.exists(token))
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
        self.prune_at(keep, causes, now_ms());
    }

    fn prune_at(&self, keep: &Key, causes: &[Cause], now: u64) {
        let idle = atlas_core::time::ms(IDLE_FOR);
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
                self.remove(&record.key);
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
            self.remove(&record.key);
        }
    }

    fn remove(&self, key: &Key) {
        self.folder.remove(key);
        if let Some(output) = self.folder.directory().parent() {
            crate::last_capture::forget_windows(output, key);
        }
    }

    /// Write `record` whole in its place.
    fn write(&self, record: &Record) -> Result<()> {
        self.folder.write_json(&record.key, record)
    }
}

fn cleanup_record(key: &Key, bytes: &[u8]) -> Result<Record> {
    let record: Record =
        serde_json::from_slice(bytes).map_err(|cause| incomplete_cleanup().with_source(cause))?;
    if record.key != *key || record.version > VERSION {
        return Err(incomplete_cleanup());
    }
    Ok(record)
}

fn incomplete_cleanup() -> Error {
    Error::new(
        Code::State,
        "Session records could not be read completely to confirm cleanup",
    )
}

/// Construct typed reply facts for session-record test fixtures.
///
/// # Errors
///
/// Returns the failure to update the session's record.
#[cfg(any(test, feature = "test-support"))]
pub fn note(
    client: &Client,
    session: &HostSession,
    kind: Kind,
    doing: Option<&Doing>,
    result: &Result<Value>,
) -> Result<()> {
    let copies = Copies::of(&client.desktop());
    note_facts(
        client,
        session,
        doing,
        &ReplyFacts::new(kind, result, &copies),
    )
}

/// Record the facts decoded by `dispatch::note_outcome`.
///
/// # Errors
///
/// Returns record-lock, decoding or write failures while recording attribution. These facts
/// do not acknowledge helper mutations.
pub fn note_facts(
    client: &Client,
    session: &HostSession,
    doing: Option<&Doing>,
    facts: &ReplyFacts,
) -> Result<()> {
    note_at(client, session, doing, facts, now_ms)
}

fn note_at(
    client: &Client,
    session: &HostSession,
    doing: Option<&Doing>,
    facts: &ReplyFacts,
    now: impl FnOnce() -> u64,
) -> Result<()> {
    if !facts.kind.recorded(doing) {
        return Ok(());
    }
    let verb = doing.map_or(ENVIRONMENT_VERB, Doing::verb);
    let verb = (!facts.refusal).then_some(verb);
    let app = doing.and_then(Doing::app_name);
    let store = Store::at(client.output_dir()?);
    let now = now();

    let logged = crate::client::logged_at(session.key());
    store.update(session, now, &client.controller, |record| {
        record.seen(session, now, verb, app);
        if let Some(logged) = logged {
            record.logged = record.logged.max(Some(logged));
        }
        match &facts.left {
            Left::None => {}
            Left::Capture(capture) => record.took(capture.clone()),
            Left::Copied(copied) => record.copied(copied.iter().cloned()),
            Left::Removed(removed) => record.removed(removed),
            Left::Plan { id, phase, .. } => record.planned(id.clone(), *phase, now),
        }
    })
}

/// Note any command before dispatch, including a first read, without counting
/// an action. Only a verified ancestor may attach or replace lifetime metadata;
/// an existing root must be confirmed gone before replacement. Best effort,
/// with zero-wait locks.
///
/// # Errors
///
/// Returns private-storage, record-lock or write failures. An unreadable existing session
/// record is left intact.
pub fn touch(client: &Client, session: &HostSession) -> Result<()> {
    Store::at(client.output_dir()?).touch(session, session.ancestor(), now_ms(), &client.controller)
}

/// Forget, in every session's record, the plan a successful rollback in
/// `result` undid, whoever ran it: the person at a terminal, another
/// session or this one. It decides nothing about the rollback itself.
#[cfg(test)]
fn forget_rolled_back(client: &Client, kind: Kind, result: &Result<Value>) -> Result<()> {
    let copies = Copies::of(&client.desktop());
    forget_rolled_back_facts(client, &ReplyFacts::new(kind, result, &copies))
}

/// Forget the rollback decoded by `dispatch::note_outcome`.
///
/// # Errors
///
/// Returns record-access or settlement-write failures. Only a confirmed rollback supplies
/// global settlement facts.
pub fn forget_rolled_back_facts(client: &Client, facts: &ReplyFacts) -> Result<()> {
    match facts.rolled_back() {
        Some((plan, _)) => Store::at(client.output_dir()?).forget_plan(plan),
        None => Ok(()),
    }
}

/// The capture at `path`, when it is one of Atlas's own.
fn capture_named(path: &Value) -> Option<Capture> {
    Path::new(path.as_str()?)
        .file_name()?
        .to_str()
        .and_then(Capture::parse)
}

/// The copy folder tokens of the paths `listed` names under `copies`, each
/// one's `field` (or the entry itself when `field` is empty).
fn tokens<'a>(
    listed: &'a Value,
    field: &'a str,
    copies: &'a Copies,
) -> impl Iterator<Item = Token> + 'a {
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
            copies.token_of(path.as_str()?)
        })
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
