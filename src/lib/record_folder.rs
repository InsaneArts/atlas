//! A private folder of small JSON records, one file per key, in the output
//! folder: how session and task records and cleanup receipts are kept.
//! Each record is written whole beside its place and renamed there, so a
//! reader never sees half of one; writers take the folder's lock, readers
//! never do.
use crate::{
    control::{Durability, Token, write_private_with},
    error::{Error, Result},
    helper_info::{own_folder, read_own},
    host_session::Key,
};
use fs2::FileExt;
use std::{
    fmt::Display,
    fs::{self, File, OpenOptions},
    marker::PhantomData,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
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
/// How often a writer tries the folder's lock while it waits.
const LOCK_POLL: Duration = Duration::from_millis(10);

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

/// One output folder's records of one kind, each named by a `K`.
pub(crate) struct RecordFolder<K = Key> {
    directory: PathBuf,
    records: &'static Records,
    names: PhantomData<K>,
}

impl<K: RecordName> RecordFolder<K> {
    pub fn at(output: &Path, records: &'static Records) -> Self {
        Self {
            directory: output.join(records.folder),
            records,
            names: PhantomData,
        }
    }

    /// Whether the folder is there, a real folder of this account.
    pub fn exists(&self) -> bool {
        own_folder(&self.directory)
    }

    /// Where the record of `key` is.
    pub fn path(&self, key: &K) -> PathBuf {
        self.directory.join(format!("{key}.json"))
    }

    /// Whether anything is where the record of `key` goes, a record or not.
    pub fn holds(&self, key: &K) -> bool {
        self.exists() && fs::symlink_metadata(self.path(key)).is_ok()
    }

    /// The bytes of `key`'s record, when it is a plain file of this account
    /// no larger than its kind allows, in a folder of this account.
    pub fn read(&self, key: &K) -> Option<Vec<u8>> {
        self.exists()
            .then(|| read_own(&self.path(key), self.records.bytes))
            .flatten()
    }

    /// The keys of the newest `limit` records by when each was written, of
    /// the first [`LISTED`] records in the folder, newest first. Entries
    /// that are not `KEY.json` are never counted.
    pub fn listed(&self, limit: usize) -> Vec<K> {
        if !self.exists() {
            return Vec::new();
        }
        let Ok(entries) = fs::read_dir(&self.directory) else {
            return Vec::new();
        };

        let mut named: Vec<(SystemTime, K)> = entries
            .flatten()
            .filter_map(|entry| {
                let key = entry
                    .file_name()
                    .to_str()?
                    .strip_suffix(".json")
                    .and_then(K::parse)?;
                Some((entry.metadata().ok()?.modified().ok()?, key))
            })
            .take(LISTED)
            .collect();
        named.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
        named.into_iter().take(limit).map(|(_, key)| key).collect()
    }

    /// Write `bytes` as `key`'s record, as far as its kind asks.
    pub fn write(&self, key: &K, bytes: &[u8]) -> Result<()> {
        let path = self.path(key);
        let partial = path.with_extension("json.partial");
        write_private_with(&partial, bytes, self.records.durability)?;
        fs::rename(&partial, &path).map_err(|error| Error::writing(&path, error))?;
        if self.records.durability == Durability::Disk {
            File::open(&self.directory)?.sync_all()?;
        }
        Ok(())
    }

    /// Remove `key`'s record.
    pub fn remove(&self, key: &K) {
        let _ = fs::remove_file(self.path(key));
    }

    /// The folder's lock, held while records are changed and never by a
    /// reader. A writer waits at most its kind's lock wait for another to
    /// let go of it, then is refused, so recording never holds a command
    /// up for long.
    pub fn lock(&self) -> Result<File> {
        self.lock_within(self.records.lock_wait)
    }

    /// [`RecordFolder::lock`], waiting at most `wait`: none at all tries
    /// once.
    pub fn lock_within(&self, wait: Duration) -> Result<File> {
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
                format!(
                    "The {} records' lock is not a plain file",
                    self.records.noun
                ),
            ));
        }

        let until = Instant::now() + wait;
        while FileExt::try_lock_exclusive(&lock).is_err() {
            if Instant::now() >= until {
                return Err(Error::new(
                    "busy",
                    format!(
                        "Another Atlas command is writing the {} records",
                        self.records.noun
                    ),
                ));
            }
            std::thread::sleep(LOCK_POLL);
        }
        Ok(lock)
    }

    /// Make the folder, private to this account, refusing a link or another
    /// account's folder. A folder made here is written through to its
    /// parent as far as its kind asks.
    pub fn make(&self) -> Result<()> {
        match fs::DirBuilder::new().mode(0o700).create(&self.directory) {
            Ok(()) if self.records.durability == Durability::Disk => {
                if let Some(output) = self.directory.parent() {
                    File::open(output)?.sync_all()?;
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
                "state",
                format!(
                    "{} is a link or not a folder of this account",
                    self.directory.display()
                ),
            ))
        }
    }

    /// When the folder last changed, when it is a real folder.
    pub fn modified(&self) -> Option<SystemTime> {
        fs::symlink_metadata(&self.directory)
            .ok()
            .filter(|metadata| metadata.is_dir())
            .and_then(|metadata| metadata.modified().ok())
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }
}
