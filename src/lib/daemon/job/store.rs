//! Where the daemon keeps its job records and logs: one record file and one
//! log a job, beside its socket. A record that cannot be written is kept in
//! memory instead, so what the daemon decided is never lost with it.
use super::record::{JobState, Record};
use crate::{
    control::Durability,
    daemon::protocol::JobId,
    error::Result,
    record_folder::{LISTED, RecordFolder, Records},
};
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

/// The most records kept; the oldest ended ones go first.
const RECORDS_MAX: usize = 64;
/// How long an ended job's record and log are kept.
const KEEP: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// How job records are kept: written whole and ordered on the disk before
/// the daemon goes on, a power cut leaving each whole or absent.
static JOB_RECORDS: Records = Records {
    folder: "jobs",
    noun: "job",
    lock_wait: Duration::from_secs(2),
    durability: Durability::Kernel,
    bytes: 128 * 1024,
};

/// A record as read: one this build may act on, or one it never acts on.
pub(super) enum Found {
    Record(Box<Record>),
    Unreadable(JobId),
}

/// The records folder, and the records that could not be written to it.
pub(super) struct Store {
    folder: RecordFolder<JobId>,
    /// Records whose last write failed, by ID: they stand for the file
    /// until a write succeeds.
    unwritten: HashMap<JobId, Record>,
}

impl Store {
    /// The `jobs` folder beside the daemon's socket in `daemon`, made
    /// private when it is missing.
    pub fn open(daemon: &Path) -> Result<Self> {
        let folder = RecordFolder::at(daemon, &JOB_RECORDS);
        folder.make()?;
        Ok(Self {
            folder,
            unwritten: HashMap::new(),
        })
    }

    pub fn directory(&self) -> &Path {
        self.folder.directory()
    }

    /// Whether anything is where `id`'s record goes.
    pub fn holds(&self, id: &JobId) -> bool {
        self.unwritten.contains_key(id) || self.folder.holds(id)
    }

    /// Every record, as read.
    pub fn records(&self) -> Vec<Found> {
        let mut ids = self.folder.listed(LISTED);
        for id in self.unwritten.keys() {
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
        ids.into_iter().filter_map(|id| self.read(&id)).collect()
    }

    /// The record of `id`: none when there is none, unreadable when it does
    /// not parse, is of another version, never got a process, or names one
    /// whose identity is not sound.
    pub fn read(&self, id: &JobId) -> Option<Found> {
        if let Some(record) = self.unwritten.get(id) {
            return Some(Found::Record(Box::new(record.clone())));
        }
        if !self.folder.holds(id) {
            return None;
        }
        let record = self
            .folder
            .read(id)
            .and_then(|bytes| serde_json::from_slice::<Record>(&bytes).ok())
            .filter(|record| record.id == *id && record.sound());
        Some(match record {
            Some(record) => Found::Record(Box::new(record)),
            None => Found::Unreadable(id.clone()),
        })
    }

    /// Keep `record`: written whole, or held in memory when the write
    /// fails. Whether it reached the disk.
    pub fn put(&mut self, record: &Record) -> bool {
        let written = serde_json::to_vec(record)
            .ok()
            .is_some_and(|bytes| self.folder.write(&record.id, &bytes).is_ok());
        if written {
            self.unwritten.remove(&record.id);
        } else {
            self.unwritten.insert(record.id.clone(), record.clone());
        }
        written
    }

    /// Write `record` whole, or say why not; nothing is held in memory.
    pub fn write(&self, record: &Record) -> Result<()> {
        self.folder.write(&record.id, &serde_json::to_vec(record)?)
    }

    /// Remove `id`'s record and log.
    pub fn forget(&mut self, id: &JobId) {
        self.unwritten.remove(id);
        self.folder.remove(id);
        let _ = fs::remove_file(self.log_path(id));
    }

    pub fn log_path(&self, id: &JobId) -> PathBuf {
        self.folder.directory().join(format!("{id}.log"))
    }

    /// Empty `id`'s log once it grows past `limit`, through a handle of the
    /// daemon's own: the appends of whatever still writes it go on at the
    /// new end. Whether it was emptied.
    pub fn bound_log(&self, id: &JobId, limit: u64) -> bool {
        let path = self.log_path(id);
        let over = fs::symlink_metadata(&path)
            .is_ok_and(|metadata| metadata.is_file() && metadata.len() > limit);
        over && OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&path)
            .and_then(|file| file.set_len(0))
            .is_ok()
    }

    /// Remove ended records older than a week, with their logs, and the
    /// oldest ended ones past [`RECORDS_MAX`]. A record of a job that may
    /// still run, or one that cannot be read, is never removed.
    pub fn prune(&mut self, now: u64) {
        let week = u64::try_from(KEEP.as_millis()).unwrap_or(u64::MAX);
        let mut ended: Vec<(u64, JobId)> = self
            .records()
            .into_iter()
            .filter_map(|read| match read {
                Found::Record(record) if record.state == JobState::Ended => Some((
                    record
                        .ended
                        .as_ref()
                        .map_or(record.started_at, |ended| ended.at),
                    record.id,
                )),
                _ => None,
            })
            .collect();
        ended.sort();
        let total = self.folder.listed(LISTED).len();
        let mut over = total.saturating_sub(RECORDS_MAX);
        for (at, id) in ended {
            if now.saturating_sub(at) > week || over > 0 {
                self.forget(&id);
                over = over.saturating_sub(1);
            }
        }
    }
}
