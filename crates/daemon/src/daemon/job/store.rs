//! Where the daemon keeps its job records and logs: one record file and one
//! log a job, beside its socket. A record that cannot be written is kept in
//! memory instead, so what the daemon decided is never lost with it.
use super::record::Record;
use atlas_core::{
    error::Result,
    files::Durability,
    protocol::{JobId, JobState},
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

/// Job records are replaced whole with kernel-level ordering for process-crash
/// recovery. This does not request an fsync or a power-loss durability guarantee.
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
    pub(super) fn open(daemon: &Path) -> Result<Self> {
        let folder = RecordFolder::at(daemon, &JOB_RECORDS);
        folder.make()?;
        Ok(Self {
            folder,
            unwritten: HashMap::new(),
        })
    }

    pub(super) fn directory(&self) -> &Path {
        self.folder.directory()
    }

    /// Whether anything is where `id`'s record goes.
    pub(super) fn holds(&self, id: &JobId) -> bool {
        self.unwritten.contains_key(id) || self.folder.holds(id)
    }

    /// Every record, as read.
    pub(super) fn records(&self) -> Vec<Found> {
        self.scanned_records().0
    }

    /// Every record found, and whether the scan can account for all jobs.
    pub(super) fn scanned_records(&self) -> (Vec<Found>, bool) {
        let listing = self.folder.listing(LISTED);
        let mut ids = listing.keys;
        for id in self.unwritten.keys() {
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
        let records = ids
            .into_iter()
            .map(|id| self.read(&id).unwrap_or(Found::Unreadable(id)))
            .collect();
        (records, listing.complete)
    }

    /// The record of `id`: none when there is none, unreadable when it does
    /// not parse, is of another version, never got a process, or names one
    /// whose identity is not sound.
    pub(super) fn read(&self, id: &JobId) -> Option<Found> {
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
    pub(super) fn put(&mut self, record: &Record) -> bool {
        let written = self.write(record).is_ok();
        if written {
            self.unwritten.remove(&record.id);
        } else {
            self.unwritten.insert(record.id.clone(), record.clone());
        }
        written
    }

    /// Write `record` whole, or say why not; nothing is held in memory.
    pub(super) fn write(&self, record: &Record) -> Result<()> {
        self.folder.write(&record.id, &serde_json::to_vec(record)?)
    }

    /// Remove `id`'s record and log.
    pub(super) fn forget(&mut self, id: &JobId) {
        self.unwritten.remove(id);
        self.folder.remove(id);
        let _ = fs::remove_file(self.log_path(id));
    }

    pub(super) fn log_path(&self, id: &JobId) -> PathBuf {
        self.folder.directory().join(format!("{id}.log"))
    }

    /// Empty `id`'s log once it grows past `limit`, through a handle of the
    /// daemon's own: the appends of whatever still writes it go on at the
    /// new end. Whether it was emptied.
    pub(super) fn bound_log(&self, id: &JobId, limit: u64) -> bool {
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
    /// still run, cannot be read, or has an unacknowledged write is never
    /// removed. The in-memory result must reach disk before retention can
    /// discard it and the previous durable record.
    pub(super) fn prune(&mut self, now: u64) {
        let week = atlas_core::time::ms(KEEP);
        let mut ended: Vec<(u64, JobId)> = self
            .records()
            .into_iter()
            .filter_map(|read| match read {
                Found::Record(record)
                    if record.state == JobState::Ended
                        && !self.unwritten.contains_key(&record.id) =>
                {
                    Some((
                        record
                            .ended
                            .as_ref()
                            .map_or(record.started_at, |ended| ended.at),
                        record.id,
                    ))
                }
                Found::Record(_) | Found::Unreadable(_) => None,
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
