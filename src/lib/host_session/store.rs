//! Each host session's record: one file in the private output folder's
//! `sessions` folder, saying when the session was seen, how many of each
//! action it did, and the screenshots, copies and environment plans it
//! left, by Atlas's own names only. Losing a record loses only labels: the
//! copies, plans and pending changes themselves stay and keep their own
//! cleanup. Recording is best effort and never holds a command up: when
//! another command is writing the records, this one's goes unrecorded.
use super::{
    HostSession, Key, now_ms,
    record::{Leftovers, Record, Seen},
};
use crate::{
    client::Client,
    control::{Cause, Controller, Durability, Token, write_private_with},
    doing::Doing,
    environment::Phase,
    error::{Error, Result},
    handoff,
    helper_info::{Capture, own_folder, read_own},
};
use fs2::FileExt;
use serde::Deserialize;
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant, SystemTime},
};

/// The most records kept; those with nothing left in the helper go first.
pub const MAX_RECORDS: usize = 64;
/// How long a record with nothing left in the helper is kept after its
/// session's last recorded action.
pub const IDLE_FOR: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// The most a record may hold before it is treated as missing.
const RECORD_BYTES: u64 = 64 * 1024;
/// The most entries of the `sessions` folder looked at.
const LISTED: usize = 256;
/// The most folders looked at, from the working directory up, for the
/// project a session works in.
const FOLDER_DEPTH: usize = 32;
/// How long a command waits for another to finish writing the records
/// before it leaves its own unrecorded.
const LOCK_WAIT: Duration = Duration::from_millis(250);
/// How often it tries the records' lock meanwhile.
const LOCK_POLL: Duration = Duration::from_millis(10);
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

/// The records last read for status reads, by the folder they came from
/// and its modification time (every write renames a record into it).
struct Snapshot {
    directory: PathBuf,
    modified: SystemTime,
    taken: Instant,
    seen: Arc<[Seen]>,
}

/// The records status reads share, read again only once one was written,
/// or once the copy folders they counted may have changed.
static RECENT: Mutex<Option<Snapshot>> = Mutex::new(None);

/// The records of one output folder.
pub struct Store {
    directory: PathBuf,
    /// Where `open`'s copy folders are.
    copies: PathBuf,
}

impl Store {
    pub fn at(output: &Path) -> Self {
        Self {
            directory: output.join("sessions"),
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
        own_folder(&self.directory)
            .then(|| read_record(&self.path(key)))
            .flatten()
            .filter(|record| record.key == *key)
    }

    /// The newest [`MAX_RECORDS`] records, newest `lastSeen` first.
    pub(crate) fn all(&self) -> Vec<Record> {
        self.listed(MAX_RECORDS)
    }

    /// The newest `limit` records by when each was written, of the first
    /// [`LISTED`] entries of the folder, newest `lastSeen` first.
    fn listed(&self, limit: usize) -> Vec<Record> {
        if !own_folder(&self.directory) {
            return Vec::new();
        }
        let Ok(entries) = fs::read_dir(&self.directory) else {
            return Vec::new();
        };

        let mut named: Vec<(SystemTime, PathBuf, Key)> = entries
            .flatten()
            .take(LISTED)
            .filter_map(|entry| {
                let key = entry
                    .file_name()
                    .to_str()?
                    .strip_suffix(".json")
                    .and_then(Key::parse)?;
                let modified = entry.metadata().ok()?.modified().ok()?;
                Some((modified, entry.path(), key))
            })
            .collect();
        named.sort_by_key(|(modified, ..)| std::cmp::Reverse(*modified));

        let mut records: Vec<Record> = named
            .into_iter()
            .take(limit)
            .filter_map(|(_, path, key)| read_record(&path).filter(|record| record.key == key))
            .collect();
        records.sort_by_key(|record| std::cmp::Reverse(record.last_seen));
        records
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
        kept: &Mutex<Option<Snapshot>>,
        read: impl FnOnce(&Self) -> Vec<Record>,
    ) -> Arc<[Seen]> {
        let Some(modified) = fs::symlink_metadata(&self.directory)
            .ok()
            .filter(|metadata| metadata.is_dir())
            .and_then(|metadata| metadata.modified().ok())
        else {
            return Arc::from([]);
        };

        let mut kept = kept.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(snapshot) = kept.as_ref()
            && snapshot.directory == self.directory
            && snapshot.modified == modified
            && snapshot.taken.elapsed() < COPIES_FRESH
        {
            return Arc::clone(&snapshot.seen);
        }

        let seen: Arc<[Seen]> = read(self)
            .into_iter()
            .map(|record| Seen {
                copies_left: self.copies_left(&record).len(),
                record,
            })
            .collect();
        *kept = Some(Snapshot {
            directory: self.directory.clone(),
            modified,
            taken: Instant::now(),
            seen: Arc::clone(&seen),
        });
        seen
    }

    /// Change `session`'s record under the store's lock, making it when
    /// new, and write it whole. A new record follows the one before it in
    /// the same host process, which then ends, and makes room among the
    /// others; `controller` says which pending changes are whose.
    pub(crate) fn update(
        &self,
        session: &HostSession,
        now: u64,
        controller: &Controller,
        change: impl FnOnce(&mut Record),
    ) -> Result<()> {
        self.make_folder()?;
        let _lock = self.lock()?;

        let existing = self.read(session.key());
        let made = existing.is_none();
        let mut record = existing.unwrap_or_else(|| {
            let folder = std::env::current_dir()
                .ok()
                .and_then(|directory| project_folder(&directory));
            Record::new(session, now, folder)
        });

        if made && let Some(mut before) = self.before_clear(&record) {
            record.previous = Some(before.key.clone());
            before.ended = Some(now);
            self.write(&before)?;
        }

        change(&mut record);
        self.write(&record)?;

        if made {
            let causes = controller.pending_causes().unwrap_or_default();
            self.prune(&record.key, &causes);
        }
        Ok(())
    }

    /// Forget plan `plan_id`, which a command rolled back, in every record
    /// that lists it: the plan is gone, whoever rolled it back.
    pub(crate) fn forget_plan(&self, plan_id: &Token) -> Result<()> {
        if !own_folder(&self.directory) {
            return Ok(());
        }
        let _lock = self.lock()?;
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

    /// The session `record` followed in the same host process: the newest
    /// other record naming that process that has not ended.
    fn before_clear(&self, record: &Record) -> Option<Record> {
        let process = record.host_process?;
        self.all().into_iter().find(|other| {
            other.key != record.key && other.host_process == Some(process) && other.ended.is_none()
        })
    }

    /// Remove records idle longer than [`IDLE_FOR`] with nothing left in
    /// the helper, then the oldest while more than [`MAX_RECORDS`] remain,
    /// those with something left last. The record `keep` stays.
    fn prune(&self, keep: &Key, causes: &[Cause]) {
        let now = now_ms();
        let idle = u64::try_from(IDLE_FOR.as_millis()).unwrap_or(u64::MAX);
        let mut records: Vec<(Record, bool)> = self
            .listed(LISTED)
            .into_iter()
            .map(|record| {
                let empty = self.leftovers(&record, causes).is_empty();
                (record, empty)
            })
            .collect();

        records.retain(|(record, empty)| {
            let stale =
                *empty && now.saturating_sub(record.last_seen) > idle && record.key != *keep;
            if stale {
                let _ = fs::remove_file(self.path(&record.key));
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
            let _ = fs::remove_file(self.path(&record.key));
        }
    }

    fn path(&self, key: &Key) -> PathBuf {
        self.directory.join(format!("{key}.json"))
    }

    /// Write `record` whole beside its place, then rename it there, so a
    /// reader never sees half of one.
    fn write(&self, record: &Record) -> Result<()> {
        let path = self.path(&record.key);
        let partial = path.with_extension("json.partial");
        write_private_with(&partial, &serde_json::to_vec(record)?, Durability::Kernel)?;
        fs::rename(&partial, &path).map_err(|error| Error::writing(&path, error))
    }

    /// The store's lock, held while records are changed and never by a
    /// reader. A command waits at most [`LOCK_WAIT`] for another to let go
    /// of it, then is refused, so recording never holds a command up.
    fn lock(&self) -> Result<File> {
        let path = self.directory.join(".lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&path)
            .map_err(|error| Error::writing(&path, error))?;
        if !lock.metadata()?.is_file() {
            return Err(Error::new(
                "state",
                "The session records' lock is not a plain file",
            ));
        }

        let until = Instant::now() + LOCK_WAIT;
        while FileExt::try_lock_exclusive(&lock).is_err() {
            if Instant::now() >= until {
                return Err(Error::new(
                    "busy",
                    "Another Atlas command is writing the session records",
                ));
            }
            std::thread::sleep(LOCK_POLL);
        }
        Ok(lock)
    }

    /// Make the `sessions` folder, private to this account, refusing a link
    /// or another account's folder.
    fn make_folder(&self) -> Result<()> {
        match fs::DirBuilder::new().mode(0o700).create(&self.directory) {
            Err(error) if error.kind() != std::io::ErrorKind::AlreadyExists => {
                return Err(Error::writing(&self.directory, error));
            }
            _ => {}
        }

        if own_folder(&self.directory) {
            Ok(())
        } else {
            Err(Error::new(
                "state",
                format!(
                    "{} is a link or not a folder of this account",
                    self.directory.display()
                ),
            ))
        }
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

/// Forget, in every session's record, the plan a successful rollback in
/// `result` undid, whoever ran it: the person at a terminal, another
/// session or this one. It decides nothing about the rollback itself.
pub fn forget_rolled_back(client: &Client, kind: Kind, result: &Result<Value>) -> Result<()> {
    let Ok(reply) = result else {
        return Ok(());
    };
    let rolled_back = kind == Kind::Environment { changes: true }
        && reply["ok"] == true
        && reply["preview"] != true
        && phase(reply) == Some(Phase::RolledBack);
    match plan(reply).filter(|_| rolled_back) {
        Some(plan) => Store::at(client.output_dir()?).forget_plan(&plan),
        None => Ok(()),
    }
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
        Err(error) => (error.code, error.uncertain),
        Ok(value) if value["ok"] == false => (
            value["code"].as_str().unwrap_or_default(),
            value["uncertain"] == true,
        ),
        Ok(_) => return false,
    };
    !uncertain && matches!(code, "busy" | "stopped" | "uncertain")
}

/// The plan a reply names, when it is one of Atlas's own.
fn plan(reply: &Value) -> Option<Token> {
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

/// The record at `path`, when it is a plain file of this account no larger
/// than [`RECORD_BYTES`] that holds only what Atlas writes.
fn read_record(path: &Path) -> Option<Record> {
    serde_json::from_slice(&read_own(path, RECORD_BYTES)?).ok()
}

#[cfg(test)]
mod tests;
