//! Private write-ahead state. Preview opens existing state without creating it.
use super::{model::*, *};
use fs2::FileExt;

pub(super) struct Store {
    pub path: PathBuf,
    _lock: Option<fs::File>,
}

impl Store {
    pub fn open(home: &Path, write: bool) -> Result<Self> {
        let path = home.join(STATE);
        let directory = if write {
            Some(private_directory(&path)?)
        } else {
            directory(&path, false)?
        };
        let Some(directory) = directory else {
            return Ok(Self { path, _lock: None });
        };
        validate_private(&directory, true)?;
        let lock = open_regular(&path.join("lock"), write)?;
        if let Some(lock) = &lock {
            validate_private(lock, false)?;
            if write {
                lock.try_lock_exclusive()
            } else {
                FileExt::try_lock_shared(lock)
            }
            .map_err(|_| problem("Another environment operation is running"))?;
        }
        Ok(Self { path, _lock: lock })
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

    pub fn require_available(&self, id: &str) -> Result<()> {
        if let Some(active) = self.read::<String>(&self.path.join("active.json"))? {
            validate_id(&active)?;
            if active != id {
                return Err(problem(format!(
                    "Transaction {active} must be resumed or rolled back first"
                )));
            }
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
        if self._lock.is_none() {
            return Err(problem("Provisioning state is read-only"));
        }
        let bytes = serde_json::to_vec_pretty(value)?;
        if bytes.len() as u64 > MAX_RECORD {
            return Err(problem("Provisioning record exceeds its 4 MiB limit"));
        }
        atomic_write(path, &bytes, 0o600)
    }
}
