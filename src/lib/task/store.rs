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
    control::write_private,
    error::{Error, Result},
    helper_info::{own_folder, read_own},
    host_session::{Key, store::Store as Sessions},
};
use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// The most a record may hold before it is treated as unreadable.
const RECORD_BYTES: u64 = 256 * 1024;
/// The most entries of the `tasks` folder looked at.
const LISTED: usize = 256;
/// How long a command waits for another to finish writing the records.
const LOCK_WAIT: Duration = Duration::from_secs(1);
/// How often it tries the records' lock meanwhile.
const LOCK_POLL: Duration = Duration::from_millis(10);

/// The task records of one output folder.
pub(crate) struct Store {
    directory: PathBuf,
    output: PathBuf,
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
    pub fn at(output: &Path) -> Self {
        Self {
            directory: output.join("tasks"),
            output: output.to_owned(),
        }
    }

    /// One task's record, when it is there and reads.
    pub fn load(&self, key: &Key) -> Option<Record> {
        match self.read(key) {
            Read::Record(record) => Some(*record),
            Read::Missing | Read::Kept => None,
        }
    }

    /// The records of `session`'s tasks, from the first [`LISTED`] entries
    /// of the folder, oldest first; nothing they name is looked at.
    pub fn of_session(&self, session: &Key) -> Vec<Record> {
        let mut records: Vec<Record> = self
            .listed()
            .into_iter()
            .filter(|record| record.session == *session)
            .collect();
        records.sort_by_key(|record| record.started);
        records
    }

    /// Change `task`'s record under the lock, making it when new, and write
    /// it through to the disk. A record this build cannot read or did not
    /// write the version of is never replaced: the change is refused.
    pub fn update(
        &self,
        task: &Task,
        now: u64,
        change: impl FnOnce(&mut Record) -> Result<()>,
    ) -> Result<()> {
        self.make_folder()?;
        let _lock = self.lock()?;

        let (mut record, made) = match self.read(task.key()) {
            Read::Record(record) => (*record, false),
            Read::Missing => (Record::new(task, now), true),
            Read::Kept => {
                return Err(Error::new(
                    "state",
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
    pub fn settle_everywhere(&self, owned: &[Owned], state: State, now: u64) -> Result<()> {
        if !own_folder(&self.directory) {
            return Ok(());
        }
        let _lock = self.lock()?;
        for mut record in self.listed() {
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

    /// Remove the records whose session's record is gone and that hold
    /// nothing that may still be in the helper. The record `keep` stays.
    fn prune(&self, keep: &Key) {
        let sessions = Sessions::at(&self.output);
        for record in self.listed() {
            let finished = record.key != *keep
                && !record.holds_anything()
                && sessions.read(&record.session).is_none();
            if finished {
                let _ = fs::remove_file(self.path(&record.key));
            }
        }
    }

    /// The records among the first [`LISTED`] entries of the folder.
    fn listed(&self) -> Vec<Record> {
        if !own_folder(&self.directory) {
            return Vec::new();
        }
        let Ok(entries) = fs::read_dir(&self.directory) else {
            return Vec::new();
        };

        entries
            .flatten()
            .take(LISTED)
            .filter_map(|entry| {
                entry
                    .file_name()
                    .to_str()?
                    .strip_suffix(".json")
                    .and_then(Key::parse)
            })
            .filter_map(|key| self.load(&key))
            .collect()
    }

    fn read(&self, key: &Key) -> Read {
        let path = self.path(key);
        if !own_folder(&self.directory) || fs::symlink_metadata(&path).is_err() {
            return Read::Missing;
        }
        let record = read_own(&path, RECORD_BYTES)
            .and_then(|bytes| serde_json::from_slice::<Record>(&bytes).ok())
            .filter(|record| record.key == *key && record.version <= VERSION);
        match record {
            Some(record) => Read::Record(Box::new(record)),
            None => Read::Kept,
        }
    }

    fn path(&self, key: &Key) -> PathBuf {
        self.directory.join(format!("{key}.json"))
    }

    /// Write `record` whole beside its place, through to the disk, then
    /// rename it there and make the rename durable too, so a reader never
    /// sees half of one and a power cut never loses one.
    fn write(&self, record: &Record) -> Result<()> {
        let path = self.path(&record.key);
        let partial = path.with_extension("json.partial");
        write_private(&partial, &serde_json::to_vec(record)?)?;
        fs::rename(&partial, &path).map_err(|error| Error::writing(&path, error))?;
        File::open(&self.directory)?.sync_all()?;
        Ok(())
    }

    /// The records' lock, held while records are changed and never by a
    /// reader. A command waits at most [`LOCK_WAIT`] for another to let go
    /// of it, then is refused.
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
                "The task records' lock is not a plain file",
            ));
        }

        let until = Instant::now() + LOCK_WAIT;
        while FileExt::try_lock_exclusive(&lock).is_err() {
            if Instant::now() >= until {
                return Err(Error::new(
                    "busy",
                    "Another Atlas command is writing the task records",
                ));
            }
            std::thread::sleep(LOCK_POLL);
        }
        Ok(lock)
    }

    /// Make the `tasks` folder, private to this account, refusing a link or
    /// another account's folder.
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
