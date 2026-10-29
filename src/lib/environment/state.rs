//! Private write-ahead state. Preview opens existing state without creating it.
use super::{model::*, *};
use crate::files::{Lock, lock_retrying};

/// How long a lock that looks held is retried, as [`lock_retrying`] says.
const LOCK_PATIENCE: std::time::Duration = std::time::Duration::from_millis(250);

/// Why the store could not be used: another operation has it.
const RUNNING: &str = "Another environment operation is running";

/// Take the store lock without blocking on a real holder.
fn take(lock: &fs::File, write: bool, retry: impl FnMut(std::time::Instant) -> bool) -> Result<()> {
    let held = if write { Lock::Exclusive } else { Lock::Shared };
    lock_retrying(lock, held, LOCK_PATIENCE, retry).map_err(|_| problem(RUNNING))
}

pub(super) struct Store {
    pub path: PathBuf,
    lock: Option<fs::File>,
    /// Whether the store's folder was there when it was opened.
    existed: bool,
}

impl Store {
    /// The store in `home`, its lock held as `lock` says: shared to read,
    /// exclusive to change it, which makes its folder when it is not there.
    pub fn open(home: &Path, lock: Lock) -> Result<Self> {
        Self::open_with_retry(home, lock, |deadline| std::time::Instant::now() < deadline)
    }

    pub(super) fn open_with_retry(
        home: &Path,
        lock: Lock,
        retry: impl FnMut(std::time::Instant) -> bool,
    ) -> Result<Self> {
        let write = lock == Lock::Exclusive;
        let path = home.join(STATE);

        let directory = if write {
            Some(private_directory(&path)?)
        } else {
            directory(&path, false)?
        };

        let Some(directory) = directory else {
            return Ok(Self {
                path,
                lock: None,
                existed: false,
            });
        };

        validate_private(&directory, true)?;
        let lock = open_regular(&path.join("lock"), write)?;
        if let Some(lock) = &lock {
            validate_private(lock, false)?;
            take(lock, write, retry)?;
        }

        Ok(Self {
            path,
            lock,
            existed: true,
        })
    }

    /// Refuse what was read through a store opened without its lock, which
    /// happens only while there is nothing to lock yet, if another
    /// operation has since made the store or its lock: what was read may be
    /// part of that operation's change.
    pub fn confirm_read(&self) -> Result<()> {
        let appeared = |path: &Path| fs::symlink_metadata(path).is_ok();
        if self.lock.is_none()
            && ((!self.existed && appeared(&self.path)) || appeared(&self.path.join("lock")))
        {
            return Err(problem(RUNNING));
        }

        Ok(())
    }

    pub fn ledger(&self) -> Result<Ledger> {
        let ledger: Ledger = self
            .read(&self.path.join("files.json"))?
            .unwrap_or_default();
        if ledger.version > 1 {
            return Err(problem("Unsupported provisioning ledger version"));
        }

        unique_targets(ledger.files.keys().map(PathBuf::as_path))?;
        Ok(ledger)
    }

    pub fn save_ledger(&self, ledger: &Ledger) -> Result<()> {
        self.write(&self.path.join("files.json"), ledger)
    }

    pub fn transaction(&self, id: &str) -> Result<Option<Transaction>> {
        validate_id(id)?;
        let transaction: Option<Transaction> =
            self.read(&self.folder(id).join("transaction.json"))?;
        if let Some(transaction) = &transaction {
            if transaction.version != 1
                || transaction.plan.version != 1
                || transaction.plan.id != id
                || transaction.progress.len() != transaction.plan.files.len()
                || transaction.plan.files.is_empty()
                || transaction.plan.files.len() > 32
            {
                return Err(problem("Invalid transaction record"));
            }

            unique_targets(
                transaction
                    .plan
                    .files
                    .iter()
                    .map(|file| file.payload.target.as_path()),
            )?;

            for directory in &transaction.directories {
                relative_directory(&directory.target)?;
            }
        }

        Ok(transaction)
    }

    pub fn save(&self, transaction: &Transaction) -> Result<()> {
        self.write(
            &self.folder(&transaction.plan.id).join("transaction.json"),
            transaction,
        )
    }

    pub fn folder(&self, id: &str) -> PathBuf {
        self.path.join(format!("transaction-{id}"))
    }

    pub fn blob(&self, id: &str, index: usize, before: bool) -> PathBuf {
        self.folder(id).join(format!(
            "{}-{index}",
            if before { "before" } else { "after" }
        ))
    }

    /// Delete a rolled-back transaction's file copies, which may hold
    /// secrets, and keep its small record. A copy that cannot be removed
    /// now is tried again by the next rollback of the same ID.
    pub fn discard_copies(&self, transaction: &Transaction) {
        if self.lock.is_none() {
            return;
        }

        for index in 0..transaction.plan.files.len() {
            for before in [true, false] {
                let _ = unlink(&self.blob(&transaction.plan.id, index, before), false);
            }
        }
    }

    /// Delete the folders of transactions whose record was never written:
    /// copies a preparation left when it stopped part way. Only called
    /// under the write lock, so no preparation is running.
    pub fn discard_unstarted(&self) {
        if self.lock.is_none() {
            return;
        }

        let Ok(entries) = fs::read_dir(&self.path) else {
            return;
        };

        for entry in entries.flatten() {
            let name = entry.file_name();
            let started = !matches!(
                fs::symlink_metadata(entry.path().join("transaction.json")),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound
            );
            let folder = name
                .to_string_lossy()
                .strip_prefix("transaction-")
                .is_some_and(|id| validate_id(id).is_ok())
                && fs::symlink_metadata(entry.path()).is_ok_and(|metadata| metadata.is_dir());

            if folder && !started {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    }

    /// The transaction that must be resumed or rolled back before any
    /// other, when one must.
    pub fn active(&self) -> Result<Option<String>> {
        let active = self.read::<String>(&self.path.join("active.json"))?;
        if let Some(active) = &active {
            validate_id(active)?;
        }
        Ok(active)
    }

    pub fn require_available(&self, id: &str) -> Result<()> {
        if let Some(active) = self.active()?
            && active != id
        {
            return Err(problem(format!(
                "Transaction {active} must be resumed or rolled back first"
            )));
        }

        Ok(())
    }

    pub fn activate(&self, id: &str) -> Result<()> {
        self.require_available(id)?;
        self.write(&self.path.join("active.json"), &id)
    }

    pub fn finish(&self, id: &str) -> Result<()> {
        let path = self.path.join("active.json");
        if let Some(active) = self.read::<String>(&path)? {
            if active != id {
                return Err(problem("Active transaction changed"));
            }
            unlink(&path, false)?;
        }

        Ok(())
    }

    fn read<T: serde::de::DeserializeOwned>(&self, path: &Path) -> Result<Option<T>> {
        let Some(file) = open_regular(path, false)? else {
            return Ok(None);
        };

        validate_private(&file, false)?;
        Ok(Some(decode(&read_file(file, MAX_RECORD)?)?))
    }

    fn write<T: Serialize>(&self, path: &Path, value: &T) -> Result<()> {
        if self.lock.is_none() {
            return Err(problem("Provisioning state is read-only"));
        }

        let bytes = serde_json::to_vec_pretty(value)?;
        if bytes.len() as u64 > MAX_RECORD {
            return Err(problem("Provisioning record exceeds its 4 MiB limit"));
        }

        atomic_write(path, &bytes, 0o600)
    }
}
