//! A private folder of small JSON records, one file per key, in the output
//! folder: how session and task records and cleanup receipts are kept.
//! Each record is written whole beside its place and renamed there, so a
//! reader never sees half of one. Ordinary readers need no lock; writers
//! and complete cleanup scans take the folder's lock.
use crate::{
    error::{Code, Error, Result},
    files::{
        Durability, Held, Lock, LockFile, Token, lock_within, own_folder, read_own,
        write_private_with,
    },
    host_session::Key,
};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fmt::Display,
    fs::{self, File, OpenOptions},
    marker::PhantomData,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant, SystemTime},
};

/// What a record is named by: one of Atlas's own names, so a file name
/// that is not one is never read as a record.
pub(crate) trait RecordName: Display + Sized {
    fn parse(text: &str) -> Option<Self>;
}

impl RecordName for Key {
    fn parse(text: &str) -> Option<Self> {
        Key::parse(text)
    }
}

impl RecordName for Token {
    fn parse(text: &str) -> Option<Self> {
        Token::parse(text)
    }
}

impl RecordName for crate::daemon::protocol::JobId {
    fn parse(text: &str) -> Option<Self> {
        Self::parse(text)
    }
}

/// The most entries of a records folder looked at.
pub(crate) const LISTED: usize = 256;

/// Cleanup can inspect more small records within the same encoded-byte budget.
/// The walker reads one at a time; its visitor owns retained decoded projections.
pub(crate) const CLEANUP_ENTRIES: usize = 4096;

/// What kind of records a folder keeps, and how.
pub(crate) struct Records {
    /// The folder's name in the output folder.
    pub folder: &'static str,
    /// What one record is called in errors: "session", "task".
    pub noun: &'static str,
    /// How long a writer waits for another to let go of the lock before it
    /// is refused.
    pub lock_wait: Duration,
    /// How far each write goes before the command goes on.
    pub durability: Durability,
    /// The most a record may hold before it is treated as unreadable.
    pub bytes: u64,
}

/// What was last read of a records folder, by the folder and when it last
/// changed: every write renames a record into it, which changes it.
pub(crate) struct Cached<T> {
    directory: PathBuf,
    modified: SystemTime,
    taken: Instant,
    read: Arc<[T]>,
}

/// One output folder's records of one kind, each named by a `K`.
pub(crate) struct RecordFolder<K> {
    directory: PathBuf,
    records: &'static Records,
    names: PhantomData<K>,
}

/// A bounded listing and whether the folder was read to its end without errors.
pub(crate) struct Listing<K> {
    pub keys: Vec<K>,
    pub complete: bool,
}

impl<K: RecordName> RecordFolder<K> {
    pub(crate) fn at(output: &Path, records: &'static Records) -> Self {
        Self {
            directory: output.join(records.folder),
            records,
            names: PhantomData,
        }
    }

    /// Whether the folder is there, a real folder of this account.
    pub(crate) fn exists(&self) -> bool {
        own_folder(&self.directory)
    }

    /// Where the record of `key` is.
    pub(crate) fn path(&self, key: &K) -> PathBuf {
        self.directory.join(format!("{key}.json"))
    }

    /// Whether anything is where the record of `key` goes, a record or not.
    pub(crate) fn holds(&self, key: &K) -> bool {
        self.exists() && fs::symlink_metadata(self.path(key)).is_ok()
    }

    /// The bytes of `key`'s record, when it is a plain file of this account
    /// no larger than its kind allows, in a folder of this account.
    pub(crate) fn read(&self, key: &K) -> Option<Vec<u8>> {
        self.exists()
            .then(|| read_own(&self.path(key), self.records.bytes))
            .flatten()
    }

    /// The keys of the newest `limit` records by when each was written, of
    /// the first [`LISTED`] entries in the folder, newest first. Entries
    /// that are not `KEY.json` consume the scan budget but are not returned.
    pub(crate) fn listed(&self, limit: usize) -> Vec<K> {
        self.listing(limit).keys
    }

    /// Like [`Self::listed`], retaining uncertainty from a failed or bounded scan.
    pub(crate) fn listing(&self, limit: usize) -> Listing<K> {
        if fs::symlink_metadata(&self.directory)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
        {
            return Listing {
                keys: Vec::new(),
                complete: true,
            };
        }
        if !self.exists() {
            return Listing {
                keys: Vec::new(),
                complete: false,
            };
        }
        let Ok(entries) = fs::read_dir(&self.directory) else {
            return Listing {
                keys: Vec::new(),
                complete: false,
            };
        };

        Self::listed_from(entries, limit)
    }

    fn listed_from(
        entries: impl Iterator<Item = std::io::Result<fs::DirEntry>>,
        limit: usize,
    ) -> Listing<K> {
        let mut complete = true;
        let mut scanned = 0;
        let mut named = Vec::new();
        for entry in entries.take(LISTED) {
            scanned += 1;
            let Ok(entry) = entry else {
                complete = false;
                continue;
            };
            let Some(key) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.strip_suffix(".json"))
                .and_then(K::parse)
            else {
                continue;
            };
            let modified = entry.metadata().and_then(|metadata| metadata.modified());
            let modified = modified.unwrap_or_else(|_| {
                complete = false;
                SystemTime::UNIX_EPOCH
            });
            named.push((modified, key));
        }
        named.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
        // Reaching the budget cannot prove there is no next entry.
        complete &= scanned < LISTED && named.len() <= limit;
        Listing {
            keys: named.into_iter().take(limit).map(|(_, key)| key).collect(),
            complete,
        }
    }

    /// The directory cleanup must keep locked throughout its complete scans and writes.
    /// Only a confirmed missing folder is empty; replacement cannot inherit this proof.
    pub(crate) fn cleanup_identity(&self) -> Result<Option<(u64, u64)>> {
        let metadata = match fs::symlink_metadata(&self.directory) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(cleanup_read().with_source(error)),
        };
        if !metadata.is_dir() || metadata.uid() != nix::unistd::getuid().as_raw() {
            return Err(cleanup_read());
        }
        Ok(Some((metadata.dev(), metadata.ino())))
    }

    pub(crate) fn check_cleanup_identity(&self, identity: (u64, u64)) -> Result<()> {
        if self.cleanup_identity()? != Some(identity) {
            return Err(cleanup_read());
        }
        Ok(())
    }

    /// One pass has the old listing's worst encoded-byte bound, without its full-record Vec.
    pub(crate) fn cleanup_bytes(&self) -> Result<u64> {
        self.records
            .bytes
            .checked_mul(LISTED as u64)
            .ok_or_else(cleanup_read)
    }

    pub(crate) fn read_cleanup(&self, key: &K, remaining: &mut u64) -> Result<Vec<u8>> {
        let bytes = read_own(&self.path(key), self.records.bytes.min(*remaining))
            .ok_or_else(cleanup_read)?;
        *remaining -= bytes.len() as u64;
        Ok(bytes)
    }

    /// A complete uncached pass for cleanup, under its caller's existing folder lock.
    /// The visitor only reads: finish and drop this iterator before renaming any record.
    pub(crate) fn visit_cleanup(
        &self,
        identity: (u64, u64),
        visit: impl FnMut(&K, &[u8], SystemTime) -> Result<()>,
    ) -> Result<()> {
        self.check_cleanup_identity(identity)?;
        let entries =
            fs::read_dir(&self.directory).map_err(|error| cleanup_read().with_source(error))?;
        self.cleanup_from(entries, visit)?;
        self.check_cleanup_identity(identity)
    }

    fn cleanup_from(
        &self,
        mut entries: impl Iterator<Item = std::io::Result<fs::DirEntry>>,
        mut visit: impl FnMut(&K, &[u8], SystemTime) -> Result<()>,
    ) -> Result<()> {
        let mut remaining = self.cleanup_bytes()?;
        for scanned in 0..=CLEANUP_ENTRIES {
            let Some(entry) = entries.next() else {
                return Ok(());
            };
            // Probe EOF at the exact limit; another entry never becomes absence.
            if scanned == CLEANUP_ENTRIES {
                return Err(cleanup_read());
            }
            let entry = entry.map_err(|error| cleanup_read().with_source(error))?;
            let Some(key) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.strip_suffix(".json"))
                .and_then(K::parse)
            else {
                continue;
            };
            let modified = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .map_err(|error| cleanup_read().with_source(error))?;
            let bytes = self.read_cleanup(&key, &mut remaining)?;
            visit(&key, &bytes, modified)?;
        }
        Err(cleanup_read())
    }

    /// `key`'s record read as a `T`, when it is there and reads as one.
    pub(crate) fn read_json<T: DeserializeOwned>(&self, key: &K) -> Option<T> {
        self.read(key)
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
    }

    /// Write `record` whole as `key`'s record.
    pub(crate) fn write_json<T: Serialize>(&self, key: &K, record: &T) -> Result<()> {
        self.write(key, &serde_json::to_vec(record)?)
    }

    /// What `read` gives, as `kept` holds it from the last time: read again
    /// only once the folder changed, or once `fresh` passed since, so
    /// readers that come often look at the folder once and nothing more.
    /// Nothing when the folder is not there.
    pub(crate) fn cached<T>(
        &self,
        kept: &Mutex<Option<Cached<T>>>,
        fresh: Duration,
        read: impl FnOnce() -> Vec<T>,
    ) -> Arc<[T]> {
        self.cached_observed(kept, fresh, self.modified(), |taken| taken.elapsed(), read)
    }

    pub(crate) fn cached_observed<T>(
        &self,
        kept: &Mutex<Option<Cached<T>>>,
        fresh: Duration,
        modified: Option<SystemTime>,
        age: impl FnOnce(Instant) -> Duration,
        read: impl FnOnce() -> Vec<T>,
    ) -> Arc<[T]> {
        let Some(modified) = modified else {
            return Arc::from([]);
        };
        let mut kept = kept.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(cached) = kept.as_ref()
            && cached.directory == self.directory
            && cached.modified == modified
            && age(cached.taken) < fresh
        {
            return Arc::clone(&cached.read);
        }
        let read: Arc<[T]> = read().into();
        *kept = Some(Cached {
            directory: self.directory.clone(),
            modified,
            taken: Instant::now(),
            read: Arc::clone(&read),
        });
        read
    }

    /// Write `bytes` as `key`'s record, as far as its kind asks.
    pub(crate) fn write(&self, key: &K, bytes: &[u8]) -> Result<()> {
        let path = self.path(key);
        let partial = path.with_extension("json.partial");
        write_private_with(&partial, bytes, self.records.durability)
            .map_err(crate::context::context("Write record", &path))?;
        fs::rename(&partial, &path).map_err(|error| Error::writing(&path, error))?;
        if self.records.durability == Durability::Disk {
            File::open(&self.directory)
                .and_then(|file| file.sync_all())
                .map_err(crate::context::context(
                    "Sync record directory",
                    &self.directory,
                ))?;
        }
        Ok(())
    }

    /// Remove `key`'s record.
    pub(crate) fn remove(&self, key: &K) {
        let _ = fs::remove_file(self.path(key));
    }

    /// The folder's lock, held by writers and complete cleanup scans.
    /// Acquisition waits at most this record kind's lock wait before
    /// refusing. That bound does not limit the work done under the lock.
    pub(crate) fn lock(&self) -> Result<LockFile> {
        self.lock_within(self.records.lock_wait)
    }

    /// [`RecordFolder::lock`], waiting at most `wait`: none at all tries
    /// once.
    pub(crate) fn lock_within(&self, wait: Duration) -> Result<LockFile> {
        let path = self.directory.join(".lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&path)
            .map_err(|error| Error::writing(&path, error))?;
        if !lock
            .metadata()
            .map_err(crate::context::context("Inspect record lock", &path))?
            .is_file()
        {
            return Err(Error::new(
                Code::State,
                format!(
                    "The {} records' lock is not a plain file",
                    self.records.noun
                ),
            ));
        }

        let lock = LockFile(lock);
        lock_within(&lock, Lock::Exclusive, wait).map_err(|Held| {
            Error::new(
                Code::Busy,
                format!(
                    "Another Atlas command is writing the {} records",
                    self.records.noun
                ),
            )
        })?;
        Ok(lock)
    }

    /// Make the folder, private to this account, refusing a link or another
    /// account's folder. A folder made here is written through to its
    /// parent as far as its kind asks.
    pub(crate) fn make(&self) -> Result<()> {
        match fs::DirBuilder::new().mode(0o700).create(&self.directory) {
            Ok(()) if self.records.durability == Durability::Disk => {
                if let Some(output) = self.directory.parent() {
                    File::open(output)
                        .and_then(|file| file.sync_all())
                        .map_err(crate::context::context(
                            "Sync record parent directory",
                            output,
                        ))?;
                }
            }
            Err(error) if error.kind() != std::io::ErrorKind::AlreadyExists => {
                return Err(Error::writing(&self.directory, error));
            }
            _ => {}
        }

        if self.exists() {
            Ok(())
        } else {
            Err(Error::new(
                Code::State,
                format!(
                    "{} is a link or not a folder of this account",
                    self.directory.display()
                ),
            ))
        }
    }

    /// When the folder last changed, when it is a real folder.
    pub(crate) fn modified(&self) -> Option<SystemTime> {
        fs::symlink_metadata(&self.directory)
            .ok()
            .filter(|metadata| metadata.is_dir())
            .and_then(|metadata| metadata.modified().ok())
    }

    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }
}

fn cleanup_read() -> Error {
    Error::new(
        Code::State,
        "Record folder could not be read completely for cleanup",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_records_follow_the_observed_directory_revision_and_path() {
        static RECORDS: Records = Records {
            folder: "records",
            noun: "test",
            lock_wait: Duration::ZERO,
            durability: Durability::Kernel,
            bytes: 1024,
        };
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let folder = RecordFolder::<Key>::at(first.path(), &RECORDS);
        let other = RecordFolder::<Key>::at(second.path(), &RECORDS);
        folder.make().unwrap();
        other.make().unwrap();
        let revision = SystemTime::UNIX_EPOCH + Duration::from_secs(1);
        let set_modified = |folder: &RecordFolder<Key>, modified| {
            File::open(folder.directory())
                .unwrap()
                .set_times(fs::FileTimes::new().set_modified(modified))
                .unwrap();
        };
        set_modified(&folder, revision);
        let kept = Mutex::new(None);
        let read = folder.cached(&kept, Duration::MAX, || vec![1]);
        let reused = folder.cached(&kept, Duration::MAX, || panic!("unchanged folder"));
        assert!(Arc::ptr_eq(&read, &reused));
        let revision = revision + Duration::from_secs(1);
        set_modified(&folder, revision);
        assert_eq!(&*folder.cached(&kept, Duration::MAX, || vec![2]), &[2]);
        set_modified(&other, revision);
        assert_eq!(&*other.cached(&kept, Duration::MAX, || vec![3]), &[3]);
    }

    #[test]
    fn record_listing_bounds_unrelated_files_and_unreadable_entries() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("unrelated"), "").unwrap();
        for unreadable in [false, true] {
            let entries = std::iter::repeat_with(|| {
                if unreadable {
                    Err(std::io::Error::other("unreadable entry"))
                } else {
                    fs::read_dir(directory.path()).unwrap().next().unwrap()
                }
            })
            .take(LISTED)
            .chain(std::iter::once_with(|| {
                panic!("read beyond the entry limit")
            }));
            let listing = RecordFolder::<Key>::listed_from(entries, 64);
            assert!(listing.keys.is_empty());
            assert!(!listing.complete);
        }
    }

    #[test]
    fn a_running_job_beyond_the_scan_budget_leaves_the_listing_incomplete() {
        use crate::daemon::protocol::JobId;

        let directory = tempfile::tempdir().unwrap();
        let unrelated = directory.path().join("unrelated");
        fs::write(&unrelated, "").unwrap();
        let id = JobId::parse("7c2e91a04b3f").unwrap();
        let job = directory.path().join(format!("{id}.json"));
        fs::write(&job, r#"{"state":"running"}"#).unwrap();
        let entry = |path: &Path| {
            fs::read_dir(directory.path())
                .unwrap()
                .find(|entry| entry.as_ref().unwrap().path() == path)
                .unwrap()
        };
        let entries = std::iter::repeat_with(|| entry(&unrelated))
            .take(LISTED)
            .chain(std::iter::once_with(|| entry(&job)));
        let listing = RecordFolder::<JobId>::listed_from(entries, LISTED);
        assert!(listing.keys.is_empty());
        assert!(!listing.complete, "an omitted running job cannot mean idle");
    }

    #[test]
    fn a_failed_entry_or_metadata_read_leaves_the_listing_incomplete() {
        let failed = std::iter::once(Err(std::io::Error::other("scan failed")));
        assert!(!RecordFolder::<Key>::listed_from(failed, LISTED).complete);

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("7c2e91a04b3f.json");
        fs::write(&path, "{}").unwrap();
        let entry = fs::read_dir(directory.path()).unwrap().next().unwrap();
        fs::remove_file(path).unwrap();
        let listing = RecordFolder::<crate::daemon::protocol::JobId>::listed_from(
            std::iter::once(entry),
            LISTED,
        );
        assert!(!listing.complete);
        assert_eq!(listing.keys.len(), 1, "the unreadable record stays known");
    }
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;
    use std::cell::Cell;

    static RECORDS: Records = Records {
        folder: "records",
        noun: "test",
        lock_wait: Duration::ZERO,
        durability: Durability::Kernel,
        bytes: 4,
    };

    fn fixture() -> (tempfile::TempDir, RecordFolder<Key>, Key) {
        let dir = tempfile::tempdir().unwrap();
        let folder = RecordFolder::at(dir.path(), &RECORDS);
        folder.make().unwrap();
        let key = Key::parse("000000000001").unwrap();
        folder.write(&key, b"1234").unwrap();
        (dir, folder, key)
    }

    fn entry(path: &Path) -> std::io::Result<fs::DirEntry> {
        fs::read_dir(path.parent().unwrap())?
            .find(|entry| entry.as_ref().is_ok_and(|entry| entry.path() == path))
            .unwrap()
    }

    #[test]
    fn cleanup_probes_eof_at_its_entry_bound_and_never_reads_an_unbounded_tail() {
        let (_dir, folder, _) = fixture();
        let unrelated = folder.directory().join("unrelated");
        fs::write(&unrelated, b"").unwrap();
        let exact = std::iter::repeat_with(|| entry(&unrelated)).take(CLEANUP_ENTRIES);
        folder
            .cleanup_from(exact, |_, _, _| panic!("unrelated names carry no record"))
            .unwrap();
        let calls = Cell::new(0);
        let over = std::iter::repeat_with(|| {
            calls.set(calls.get() + 1);
            entry(&unrelated)
        })
        .take(CLEANUP_ENTRIES + 1)
        .chain(std::iter::once_with(|| {
            panic!("read past the bounded EOF probe")
        }));
        assert!(folder.cleanup_from(over, |_, _, _| Ok(())).is_err());
        assert_eq!(calls.get(), CLEANUP_ENTRIES + 1);
    }

    #[test]
    fn cleanup_charges_actual_bytes_before_visiting_and_keeps_exact_bounds() {
        let (_dir, folder, key) = fixture();
        let path = folder.path(&key);
        let calls = Cell::new(0);
        let mut visit = |_: &Key, bytes: &[u8], _| {
            calls.set(calls.get() + 1);
            assert_eq!(bytes, b"1234");
            Ok(())
        };
        folder
            .cleanup_from(
                std::iter::repeat_with(|| entry(&path)).take(LISTED),
                &mut visit,
            )
            .unwrap();
        assert_eq!(calls.get(), LISTED);
        calls.set(0);
        assert!(
            folder
                .cleanup_from(
                    std::iter::repeat_with(|| entry(&path)).take(LISTED + 1),
                    &mut visit
                )
                .is_err()
        );
        assert_eq!(
            calls.get(),
            LISTED,
            "no byte beyond the aggregate cap reaches decoding"
        );
        let mut zero = 0;
        assert!(folder.read_cleanup(&key, &mut zero).is_err());
        assert_eq!(zero, 0);
        fs::write(&path, b"12345").unwrap();
        calls.set(0);
        assert!(
            folder
                .cleanup_from(std::iter::once_with(|| entry(&path)), &mut visit)
                .is_err()
        );
        assert_eq!(
            calls.get(),
            0,
            "an oversized single record is never decoded"
        );
    }

    #[test]
    fn cleanup_never_treats_scan_read_or_directory_replacement_errors_as_eof() {
        let (dir, folder, key) = fixture();
        let failed = std::iter::once(Err(std::io::Error::other("entry read failed")));
        assert!(
            folder
                .cleanup_from(failed, |_, _, _| panic!("failed entry"))
                .is_err()
        );
        let path = folder.path(&key);
        let stale = entry(&path);
        fs::remove_file(&path).unwrap();
        assert!(
            folder
                .cleanup_from(std::iter::once(stale), |_, _, _| panic!(
                    "unreadable record"
                ))
                .is_err()
        );
        let identity = folder.cleanup_identity().unwrap().unwrap();
        fs::rename(folder.directory(), dir.path().join("kept")).unwrap();
        folder.make().unwrap();
        assert!(
            folder
                .visit_cleanup(identity, |_, _, _| panic!("replacement directory"))
                .is_err()
        );
    }

    #[test]
    fn cleanup_refuses_recognized_links_directories_and_pipes_before_visiting() {
        for kind in ["link", "directory", "pipe"] {
            let (_dir, folder, key) = fixture();
            let path = folder.path(&key);
            fs::remove_file(&path).unwrap();
            match kind {
                "link" => {
                    let target = folder.directory().join("unrelated");
                    fs::write(&target, b"1234").unwrap();
                    std::os::unix::fs::symlink(target, &path).unwrap();
                }
                "directory" => fs::create_dir(&path).unwrap(),
                _ => nix::unistd::mkfifo(&path, nix::sys::stat::Mode::S_IRUSR).unwrap(),
            }
            let identity = folder.cleanup_identity().unwrap().unwrap();
            assert!(
                folder
                    .visit_cleanup(identity, |_, _, _| panic!("not a record"))
                    .is_err()
            );
        }
    }

    #[test]
    fn cleanup_drops_its_iterator_before_writing_and_opens_readback_from_the_beginning() {
        struct Entries<'a> {
            entry: Option<std::io::Result<fs::DirEntry>>,
            dropped: &'a Cell<bool>,
        }
        impl Iterator for Entries<'_> {
            type Item = std::io::Result<fs::DirEntry>;
            fn next(&mut self) -> Option<Self::Item> {
                self.entry.take()
            }
        }
        impl Drop for Entries<'_> {
            fn drop(&mut self) {
                self.dropped.set(true);
            }
        }
        let (_dir, folder, key) = fixture();
        let dropped = Cell::new(false);
        let entries = Entries {
            entry: Some(entry(&folder.path(&key))),
            dropped: &dropped,
        };
        folder
            .cleanup_from(entries, |_, bytes, _| {
                assert!(!dropped.get());
                assert_eq!(bytes, b"1234");
                Ok(())
            })
            .unwrap();
        assert!(dropped.get());
        folder.write(&key, b"5678").unwrap();
        let identity = folder.cleanup_identity().unwrap().unwrap();
        let mut visited = 0;
        folder
            .visit_cleanup(identity, |_, bytes, _| {
                assert_eq!(bytes, b"5678");
                visited += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(visited, 1);
    }
}
