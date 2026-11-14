//! What each change the station asked a machine guest for came to, kept
//! on the guest's disk so a reply lost on the way, or a daemon that died
//! meanwhile, never leaves the station to guess:
//!
//! 1. its intent, the change named for its operation and arguments, kept
//!    before anything is done;
//! 2. its outcome, kept before it is answered;
//! 3. its record removed only once the station settled it.
//!
//! Every write goes through the store's manifest, whose revision the
//! Keychain vouches for ([`super::commitment`]): a record deleted, rolled
//! back or forged leaves the whole store unknown, never empty. A change
//! still unresolved is never forgotten; at the bound, a new one is refused
//! as busy instead.
//!
//! A record's file is named for its change and its content's digest, and
//! written whole beside its name before it is renamed into place: no
//! version the current manifest names is ever written over.
use super::{
    commitment::{Commitment, Judged, Revision},
    vault::Vault,
};
use atlas_core::{
    digest::bytes_sha256,
    error::{Code, Error, Result},
    files::{Token, write_atomic},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::Read,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
};

/// The store's name among the guest's recovery stores.
pub const STORE: &str = "outcomes";

/// Changes kept at once; a new one past it is refused as busy.
pub const BOUND: usize = 1024;

/// The longest reply kept whole; a longer one keeps what it came to.
const REPLY: usize = 256 * 1024;

/// The longest a kept text of a shortened reply may be.
const SHORT: usize = 1024;

const VERSION: u32 = 1;

const MANIFEST: &str = "manifest.json";

/// The longest a manifest may be: every change's name and digest.
const MANIFEST_LIMIT: u64 = 1 << 20;

/// The longest a record may be: a reply kept whole, and its request.
const RECORD_LIMIT: u64 = 2 * REPLY as u64;

/// Records by change, each shared rather than copied.
type Records = BTreeMap<String, Arc<Record>>;

/// Where a change stands.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum State {
    /// Asked for and about to be done: whether it was is unknown until
    /// its outcome says.
    Intended,
    /// Done, with the reply it came to.
    Done { reply: Value },
}

/// One change's record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    change: Token,
    /// The operation, and a digest of its request, which the change is
    /// bound to: the same ID asked again with anything else is refused.
    operation: String,
    request: String,
    state: State,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    number: u64,
    /// Each record's file, by change, with the file's digest.
    records: BTreeMap<String, String>,
}

/// What the station asks of a change ID it sent.
#[derive(Debug, PartialEq, Eq)]
pub enum Asked {
    /// New: kept as intended, to be done now.
    Begun,
    /// Asked before: what it came to, or still unknown.
    Again(State),
}

/// What was last read and checked: the manifest, by its digest, and every
/// record it names.
struct Verified {
    digest: String,
    manifest: Manifest,
    records: Records,
}

/// The guest's change records in `folder`, vouched for by `vault`.
pub struct Changes<V> {
    folder: PathBuf,
    commitment: Commitment<V>,
    /// Changes kept at once.
    bound: usize,
    /// One write or read at a time, and what it last verified: a record
    /// that did not change is not read again. A change to its file is never
    /// answered from, and is caught when the daemon next starts.
    verified: Mutex<Option<Verified>>,
}

impl<V: Vault> Changes<V> {
    pub fn new(folder: impl Into<PathBuf>, vault: V) -> Self {
        Self {
            folder: folder.into(),
            commitment: Commitment::new(vault, STORE),
            bound: BOUND,
            verified: Mutex::new(None),
        }
    }

    /// A store keeping at most `bound` changes, for a test of the bound.
    #[cfg(test)]
    fn bounded(folder: impl Into<PathBuf>, vault: V, bound: usize) -> Self {
        Self {
            bound,
            ..Self::new(folder, vault)
        }
    }

    /// A record's file.
    fn record_path(&self, change: &str, digest: &str) -> PathBuf {
        self.folder.join(format!("{change}.{digest}.json"))
    }

    /// The manifest and records on disk, once the commitment vouches for
    /// them; a write a crash left between its commitment and its files is
    /// finished or given up here.
    fn trusted(&self, last: &mut Option<Verified>) -> Result<(Manifest, Records)> {
        let found = read_regular(&self.folder.join(MANIFEST), MANIFEST_LIMIT)
            .inspect_err(|_| *last = None)?;
        let revision = found.as_ref().map(|bytes| Revision {
            number: manifest_number(bytes),
            digest: bytes_sha256(bytes),
        });
        let judged = self.commitment.judge(revision.as_ref())?;
        if judged == Judged::Unknown {
            *last = None;
            return Err(unknown());
        }
        // Every record checked first: a revision is settled only once all
        // of it is there, and left pending otherwise.
        let trusted = match (found, revision) {
            (Some(bytes), Some(revision)) => self.checked(last, &bytes, revision.digest),
            _ => Ok((Manifest::default(), BTreeMap::new())),
        };
        let Ok(trusted) = trusted else {
            *last = None;
            return Err(unknown());
        };
        match judged {
            Judged::Next => {
                sync_folder(&self.folder)?;
                self.commitment.settle()?;
            }
            Judged::Current | Judged::Unknown => self.commitment.abandon()?,
        }
        Ok(trusted)
    }

    /// The manifest `bytes`, whose digest is `digest`, and every record it
    /// names, each exactly what it says; what was verified before is not
    /// read again.
    fn checked(
        &self,
        last: &mut Option<Verified>,
        bytes: &[u8],
        digest: String,
    ) -> Result<(Manifest, Records)> {
        if let Some(verified) = last.as_ref().filter(|last| last.digest == digest) {
            return Ok((verified.manifest.clone(), verified.records.clone()));
        }
        let manifest: Manifest = serde_json::from_slice(bytes).map_err(|_| unknown())?;
        if manifest.version != VERSION {
            return Err(unknown());
        }
        let mut records = BTreeMap::new();
        for (change, record_digest) in &manifest.records {
            let known = last.as_ref().and_then(|last| {
                (last.manifest.records.get(change) == Some(record_digest))
                    .then(|| last.records.get(change).cloned())
                    .flatten()
            });
            let record = match known {
                Some(record) => record,
                None => Arc::new(read_record(
                    &self.record_path(change, record_digest),
                    change,
                    record_digest,
                )?),
            };
            records.insert(change.clone(), record);
        }
        *last = Some(Verified {
            digest,
            manifest: manifest.clone(),
            records: records.clone(),
        });
        Ok((manifest, records))
    }

    /// Keep `records`, which differ from `before` at most in `changed`, as
    /// the store's next revision: committed first, then written and kept
    /// for good, then made current.
    fn keep(
        &self,
        last: &mut Option<Verified>,
        before: &Manifest,
        mut records: Records,
        changed: &[&str],
    ) -> Result<()> {
        let mut manifest = Manifest {
            version: VERSION,
            number: before
                .number
                .checked_add(1)
                .ok_or_else(|| Error::new(Code::State, "The change records are full."))?,
            records: before.records.clone(),
        };
        for change in changed {
            match records.get(*change) {
                Some(record) => {
                    let bytes = publishable(record)?;
                    let digest = bytes_sha256(&bytes);
                    let path = self.record_path(change, &digest);
                    // One already there is used only when it is a file of
                    // its own holding exactly this, kept for good.
                    if !kept_as(&path, &bytes) {
                        write_atomic(&path, &bytes, 0o600)?;
                    }
                    manifest.records.insert((*change).to_owned(), digest);
                }
                None => {
                    manifest.records.remove(*change);
                }
            }
        }
        // The records' names are kept for good before a manifest names them.
        sync_folder(&self.folder)?;
        let bytes = serde_json::to_vec(&manifest)?;
        let digest = bytes_sha256(&bytes);
        self.commitment.begin(Revision {
            number: manifest.number,
            digest: digest.clone(),
        })?;
        write_atomic(&self.folder.join(MANIFEST), &bytes, 0o600)?;
        sync_folder(&self.folder)?;
        self.commitment.settle()?;
        records.retain(|change, _| manifest.records.contains_key(change));
        self.sweep(&manifest);
        *last = Some(Verified {
            digest,
            manifest,
            records,
        });
        Ok(())
    }

    /// Remove every record file the current manifest does not name: left
    /// by settled or replaced versions, and by writes a crash cut short.
    fn sweep(&self, manifest: &Manifest) {
        let named: std::collections::HashSet<PathBuf> = manifest
            .records
            .iter()
            .map(|(change, digest)| self.record_path(change, digest))
            .collect();
        let Ok(entries) = std::fs::read_dir(&self.folder) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let record = name.ends_with(".json") && name != MANIFEST;
            let left = name.starts_with('.');
            if (record || left) && !named.contains(&path) {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    fn verified(&self) -> std::sync::MutexGuard<'_, Option<Verified>> {
        self.verified.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The station asks for `change`, the operation `operation` with
    /// request `request`: kept as intended before anything is done, or
    /// what it came to if it was asked before.
    ///
    /// # Errors
    ///
    /// Refuses the same ID with another request, a new change past the
    /// bound (busy), and anything while the store is unknown.
    pub fn ask(&self, change: &Token, operation: &str, request: &Value) -> Result<Asked> {
        let mut last = self.verified();
        let (manifest, mut records) = self.trusted(&mut last)?;
        let digest = bytes_sha256(&serde_json::to_vec(request)?);
        if let Some(record) = records.get(change.as_str()) {
            if record.operation != operation || record.request != digest {
                return Err(Error::new(
                    Code::BadRequest,
                    "This change was asked for before with another request.",
                ));
            }
            return Ok(Asked::Again(record.state.clone()));
        }
        if records.len() >= self.bound {
            return Err(Error::new(
                Code::Busy,
                "The desktop keeps too many changes the station has not settled.",
            ));
        }
        records.insert(
            change.as_str().to_owned(),
            Arc::new(Record {
                change: change.clone(),
                operation: operation.to_owned(),
                request: digest,
                state: State::Intended,
            }),
        );
        self.keep(&mut last, &manifest, records, &[change.as_str()])?;
        Ok(Asked::Begun)
    }

    /// Keep what `change` came to, before it is answered.
    ///
    /// # Errors
    ///
    /// Fails when the change was never asked for, or the store is unknown
    /// or cannot be written.
    pub fn done(&self, change: &Token, reply: &Value) -> Result<()> {
        let mut last = self.verified();
        let (manifest, mut records) = self.trusted(&mut last)?;
        let mut record = records
            .get(change.as_str())
            .map(|record| Record::clone(record))
            .ok_or_else(|| Error::new(Code::State, "No such change is kept."))?;
        record.state = State::Done {
            reply: kept(reply)?,
        };
        // A reply recovery could not read back keeps only what it came to.
        if publishable(&record).is_err() {
            record.state = State::Done {
                reply: shortened(reply),
            };
        }
        records.insert(change.as_str().to_owned(), Arc::new(record));
        self.keep(&mut last, &manifest, records, &[change.as_str()])
    }

    /// What `change` came to, as kept.
    ///
    /// # Errors
    ///
    /// Fails when the store is unknown.
    pub fn of(&self, change: &Token) -> Result<Option<State>> {
        let mut last = self.verified();
        let (_, records) = self.trusted(&mut last)?;
        Ok(records
            .get(change.as_str())
            .map(|record| record.state.clone()))
    }

    /// Every change done and not yet settled: what the guest tells the
    /// station at each hello.
    ///
    /// # Errors
    ///
    /// Fails when the store is unknown.
    pub fn unsettled(&self) -> Result<Vec<Token>> {
        let mut last = self.verified();
        let (_, records) = self.trusted(&mut last)?;
        Ok(records
            .values()
            .filter(|record| matches!(record.state, State::Done { .. }))
            .map(|record| record.change.clone())
            .collect())
    }

    /// The station settled `changes`: their records go, in one write.
    /// Settling one that is not kept, or not done, changes nothing.
    ///
    /// # Errors
    ///
    /// Fails when the store is unknown or cannot be written.
    pub fn settled(&self, changes: &[Token]) -> Result<()> {
        let mut last = self.verified();
        let (manifest, mut records) = self.trusted(&mut last)?;
        let done: Vec<&str> = changes
            .iter()
            .map(Token::as_str)
            .filter(|change| {
                records
                    .get(*change)
                    .is_some_and(|record| matches!(record.state, State::Done { .. }))
            })
            .collect();
        if done.is_empty() {
            return Ok(());
        }
        for change in &done {
            records.remove(*change);
        }
        self.keep(&mut last, &manifest, records, &done)
    }
}

/// `reply`, whole when it is small enough, else what it came to, its texts
/// cut short.
fn kept(reply: &Value) -> Result<Value> {
    if serde_json::to_vec(reply)?.len() <= REPLY {
        return Ok(reply.clone());
    }
    Ok(shortened(reply))
}

/// What `reply` came to, its texts cut short.
fn shortened(reply: &Value) -> Value {
    let mut short = serde_json::Map::new();
    for key in [
        "ok",
        "op",
        "code",
        "error",
        "uncertain",
        "exitCode",
        "signal",
    ] {
        let value = match reply.get(key) {
            Some(Value::String(text)) => Value::String(text.chars().take(SHORT).collect()),
            Some(value @ (Value::Bool(_) | Value::Number(_) | Value::Null)) => value.clone(),
            Some(_) | None => continue,
        };
        short.insert(key.to_owned(), value);
    }
    short.insert("shortened".to_owned(), Value::Bool(true));
    Value::Object(short)
}

/// A manifest's revision number, as it says, without trusting the rest.
fn manifest_number(bytes: &[u8]) -> u64 {
    serde_json::from_slice::<Value>(bytes)
        .ok()
        .and_then(|manifest| manifest["number"].as_u64())
        .unwrap_or(u64::MAX)
}

/// The record at `path`, when it is exactly what its manifest says.
fn read_record(path: &Path, change: &str, digest: &str) -> Result<Record> {
    let bytes = read_regular(path, RECORD_LIMIT)?.ok_or_else(unknown)?;
    if bytes_sha256(&bytes) != digest {
        return Err(unknown());
    }
    let record: Record = serde_json::from_slice(&bytes).map_err(|_| unknown())?;
    if record.change.as_str() != change {
        return Err(unknown());
    }
    Ok(record)
}

/// The file at `path`, open, when it is a file of its own (never a link,
/// a pipe or another kind) of at most `limit` bytes, with what it holds;
/// none when there is none. Anything else there is unknown.
fn open_regular(path: &Path, limit: u64) -> Result<Option<(std::fs::File, Vec<u8>)>> {
    let opened = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path);
    let file = match opened {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(unknown()),
    };
    let metadata = file.metadata().map_err(|_| unknown())?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(unknown());
    }
    let mut bytes = Vec::new();
    (&file)
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| unknown())?;
    if bytes.len() as u64 > limit {
        return Err(unknown());
    }
    Ok(Some((file, bytes)))
}

/// What the file at `path` holds, as [`open_regular`] finds it.
fn read_regular(path: &Path, limit: u64) -> Result<Option<Vec<u8>>> {
    Ok(open_regular(path, limit)?.map(|(_, bytes)| bytes))
}

/// Whether `path` is already a file of its own holding exactly `bytes`,
/// kept for good: the very file read, through the one descriptor.
fn kept_as(path: &Path, bytes: &[u8]) -> bool {
    let Ok(Some((file, found))) = open_regular(path, RECORD_LIMIT) else {
        return false;
    };
    found == bytes && file.sync_all().is_ok()
}

/// `record` as it is kept, when recovery reads it back as it is: within
/// the size it reads, and decoding to the same record.
fn publishable(record: &Record) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(record)?;
    let read_back = serde_json::from_slice::<Record>(&bytes).ok();
    if bytes.len() as u64 > RECORD_LIMIT || read_back.as_ref() != Some(record) {
        return Err(Error::new(
            Code::BadRequest,
            "This change is too large to keep a record of.",
        ));
    }
    Ok(bytes)
}

/// Keep what changed in `folder`, its names included, for good.
fn sync_folder(folder: &Path) -> Result<()> {
    std::fs::File::open(folder)
        .and_then(|folder| folder.sync_all())
        .map_err(|error| {
            Error::new(Code::Io, "The change records cannot be kept.").with_source(error)
        })
}

/// The store cannot be trusted: everything in it is unknown, and nothing
/// new is taken until it is recovered.
fn unknown() -> Error {
    Error::new(
        Code::Uncertain,
        "The desktop's change records cannot be trusted: what its changes came to is unknown.",
    )
}

#[cfg(test)]
#[path = "changes_tests.rs"]
mod tests;
