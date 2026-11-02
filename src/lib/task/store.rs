//! Each task's record: one file in the private output folder's `tasks`
//! folder, apart from the session records, so that a build that rewrites a
//! session record without knowing what a task made can never drop a claim.
//! Every write goes through to the disk before the command goes on: a
//! claim is written before the thing it names is made.
use super::{
    Task,
    record::{Owned, Record, State, VERSION},
};
use crate::{
    error::{Code, Error, Result},
    files::Durability,
    host::Key,
    record_folder::{Cached, LISTED, RecordFolder, Records},
};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

/// The records [`Store::recent`] read last.
static RECENT: Mutex<Option<Cached<Record>>> = Mutex::new(None);

/// How task records are kept: through to the disk, and waited for as
/// briefly as session records are, so a claim never holds `open` up long.
static TASKS: Records = Records {
    folder: "tasks",
    noun: "task",
    lock_wait: Duration::from_millis(250),
    durability: Durability::Disk,
    bytes: 256 * 1024,
};

/// The task records of one output folder.
pub(crate) struct Store {
    folder: RecordFolder<Key>,
    output: std::path::PathBuf,
}

/// A record file, as read.
enum Read {
    Missing,
    /// There, but not a record this build may rewrite: unreadable, or
    /// written by a later build.
    Kept,
    Record(Box<Record>),
}

impl Store {
    pub(crate) fn at(output: &Path) -> Self {
        Self {
            folder: RecordFolder::at(output, &TASKS),
            output: output.to_owned(),
        }
    }

    /// One task's record, when it is there and reads.
    pub(crate) fn load(&self, key: &Key) -> Option<Record> {
        match self.read(key) {
            Read::Record(record) => Some(*record),
            Read::Missing | Read::Kept => None,
        }
    }

    /// The records of `session`'s tasks, of the newest [`LISTED`] records
    /// in the folder, oldest first; nothing they name is looked at.
    pub(crate) fn of_session(&self, session: &Key) -> Vec<Record> {
        let mut records: Vec<Record> = self
            .all()
            .into_iter()
            .filter(|record| record.session == *session)
            .collect();
        records.sort_by_key(|record| record.started);
        records
    }

    /// Discover cleanup work beyond the ordinary display prefix. Keep the
    /// existing opaque-record policy; strict claim validation belongs to
    /// settlement. Physical read/scan failures cannot prove an empty scope.
    pub(crate) fn cleanup_tasks(&self, session: &Key) -> Result<Vec<Record>> {
        let mut records = self.folder.newest_complete(|key, bytes| {
            // As in load, unsupported JSON/version/identity stays untouched
            // and cannot supply a claim. Acknowledgement checks it separately.
            serde_json::from_slice::<Record>(bytes)
                .ok()
                .filter(|record| {
                    record.key == *key && record.version <= VERSION && record.session == *session
                })
        })?;
        records.sort_by_key(|record| record.started);
        Ok(records)
    }

    /// Change `task`'s record under the lock, making it when new, and write
    /// it through to the disk. A record this build cannot read or did not
    /// write the version of is never replaced: the change is refused.
    pub(crate) fn update(
        &self,
        task: &Task,
        now: u64,
        change: impl FnOnce(&mut Record) -> Result<()>,
    ) -> Result<()> {
        self.folder.make()?;
        let _lock = self.folder.lock()?;

        let (mut record, made) = match self.read(task.key()) {
            Read::Record(record) => (*record, false),
            Read::Missing => (Record::new(task, now), true),
            Read::Kept => {
                return Err(Error::new(
                    Code::State,
                    "This task's record cannot be read by this version of Atlas, so it was left as it is",
                ));
            }
        };
        change(&mut record)?;
        record.updated = now;
        self.write(&record)?;

        if made {
            self.prune(task.key());
        }
        Ok(())
    }

    /// Note what became of each of `owned` in every record that lists it,
    /// under the lock: whoever undid it, the thing itself changed.
    pub(crate) fn settle_everywhere(&self, owned: &[Owned], state: State, now: u64) -> Result<()> {
        if owned.is_empty() || !self.folder.exists() {
            return Ok(());
        }
        let _lock = self.folder.lock()?;
        for mut record in self.all() {
            let mut listed = false;
            for owned in owned {
                listed |= record.settle(owned, state, now);
            }
            if listed {
                record.updated = now;
                self.write(&record)?;
            }
        }
        Ok(())
    }

    /// Settle cleanup claims only when a complete uncached read can prove the
    /// resulting state. Ordinary attribution keeps its best-effort path above.
    pub(crate) fn settle_everywhere_checked(
        &self,
        owned: &Owned,
        state: State,
        now: u64,
    ) -> Result<()> {
        self.settle_checked(owned, state, now, None)
    }

    pub(crate) fn settle_in_scope_checked(
        &self,
        session: &Key,
        task: Option<&Key>,
        owned: &Owned,
        state: State,
        now: u64,
    ) -> Result<()> {
        self.settle_checked(owned, state, now, Some((session, task)))
    }

    fn settle_checked(
        &self,
        owned: &Owned,
        state: State,
        now: u64,
        scope: Option<(&Key, Option<&Key>)>,
    ) -> Result<()> {
        let in_scope = |record: &Record| {
            scope.is_none_or(|(session, task)| {
                record.session == *session && task.is_none_or(|task| record.key == *task)
            })
        };
        let matches = |record: &Record| {
            in_scope(record) && record.claims().any(|(claimed, _)| claimed.same(owned))
        };
        self.folder.rewrite_complete(
            cleanup_record,
            |record, modified| matches(record).then_some(std::cmp::Reverse(modified)),
            |record| {
                if !matches(record) || !record.settle(owned, state, now) {
                    return Err(incomplete_settlement());
                }
                record.updated = now;
                self.write(record)
            },
            |record| {
                if in_scope(record)
                    && record
                        .claims()
                        .any(|(claimed, found)| claimed.same(owned) && found != state)
                {
                    return Err(incomplete_settlement());
                }
                Ok(())
            },
            incomplete_settlement,
        )
    }

    /// Remove the records [`super::prune::finished`] says may go. The
    /// record `keep` stays.
    fn prune(&self, keep: &Key) {
        for record in self.all() {
            if record.key != *keep && super::prune::finished(&self.output, &record) {
                self.folder.remove(&record.key);
            }
        }
    }

    /// [`Store::all`], as last read: read again only once a record was
    /// written (every write renames one into the folder, which changes it),
    /// so readers that come often, such as the viewer's session list and
    /// session pruning, do not read every record each time.
    pub(crate) fn recent(&self) -> Arc<[Record]> {
        self.folder.cached(&RECENT, Duration::MAX, || self.all())
    }

    /// The records among the newest [`LISTED`] in the folder.
    pub(crate) fn all(&self) -> Vec<Record> {
        self.folder
            .listed(LISTED)
            .iter()
            .filter_map(|key| self.load(key))
            .collect()
    }

    fn read(&self, key: &Key) -> Read {
        if !self.folder.holds(key) {
            return Read::Missing;
        }
        let record = self
            .folder
            .read_json::<Record>(key)
            .filter(|record| record.key == *key && record.version <= VERSION);
        match record {
            Some(record) => Read::Record(Box::new(record)),
            None => Read::Kept,
        }
    }

    /// Write `record` whole in its place, through to the disk.
    fn write(&self, record: &Record) -> Result<()> {
        self.folder.write_json(&record.key, record)
    }
}

fn cleanup_record(key: &Key, bytes: &[u8]) -> Result<Record> {
    let record: Record = serde_json::from_slice(bytes)
        .map_err(|cause| incomplete_settlement().with_source(cause))?;
    // An omitted future or malformed ownership entry cannot prove absence.
    if record.key != *key
        || record.version > VERSION
        || record.claims().count() != record.owned.len()
    {
        return Err(incomplete_settlement());
    }
    Ok(record)
}

fn incomplete_settlement() -> Error {
    Error::new(
        Code::State,
        "Task records could not be read completely to confirm cleanup",
    )
}
