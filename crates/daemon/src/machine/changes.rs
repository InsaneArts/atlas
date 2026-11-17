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
//!
//! A change whose recovery must know what was asked (a job's start or
//! stop) keeps its request beside its record, in a file of its own whose
//! digest is the one its record is bound to: never part of the record or
//! the manifest, whose every field a guest from before reads strictly, and
//! named so such a guest neither reads it nor sweeps it away.
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

/// The most of what a command printed that is kept with what it came to.
pub(crate) const OUTPUT: usize = 256 * 1024;

/// The most of what commands printed kept across every record at once: a
/// command that ends past it keeps none, and a settled record frees its
/// share.
const PRINTED: usize = 32 * 1024 * 1024;

/// The longest reply kept whole; a longer one keeps what it came to. A
/// command's reply fits with its kept output, which goes as base64.
const REPLY: usize = 384 * 1024;
const _: () = assert!(OUTPUT.div_ceil(3) * 4 + 16 * 1024 <= REPLY);

/// The longest a kept text of a shortened reply may be.
const SHORT: usize = 1024;

const VERSION: u32 = 1;

const MANIFEST: &str = "manifest.json";

/// The longest a manifest may be: every change's name and digest.
const MANIFEST_LIMIT: u64 = 1 << 20;

/// The longest a record may be: a reply kept whole, and its request.
const RECORD_LIMIT: u64 = 2 * REPLY as u64;

/// The operations whose request is kept beside their record: a job's
/// start and stop, which recovery tells apart, and a stop's target.
const ASKED_KEPT: [&str; 1] = ["job"];

/// The ending of the file a change's request is kept in.
const ASKED: &str = "asked";

/// The longest a kept request may be: a request the guest takes, written
/// again.
const ASKED_LIMIT: u64 = 2 * atlas_core::protocol::MAX_REQUEST as u64;

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

/// A change kept as intended, or whose outcome is otherwise unknown: what
/// it is, and the digest of the request it was asked with
/// ([`request_digest`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Intended {
    pub change: Token,
    pub operation: String,
    pub request: String,
    /// The request itself, when it was kept beside the record and is the
    /// very one its digest names; none for one a guest from before kept,
    /// and for every operation that keeps none.
    pub asked: Option<Value>,
}

/// The digest a change is bound to: of its request, as JSON.
///
/// # Errors
///
/// Fails when the request cannot be written down.
pub fn request_digest(request: &Value) -> Result<String> {
    Ok(bytes_sha256(&serde_json::to_vec(request)?))
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
    /// The most of what commands printed kept across every record.
    printed: usize,
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
            printed: PRINTED,
            verified: Mutex::new(None),
        }
    }

    /// A store keeping at most `printed` bytes of what commands printed,
    /// for a test of that bound.
    #[cfg(test)]
    pub(crate) fn printing_at_most(folder: impl Into<PathBuf>, vault: V, printed: usize) -> Self {
        Self {
            printed,
            ..Self::new(folder, vault)
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

    /// The file a change's request is kept in.
    fn asked_path(&self, change: &str) -> PathBuf {
        self.folder.join(format!("{change}.{ASKED}"))
    }

    /// The request `record` was asked with, as kept beside it: none when
    /// none was kept, or what is there is not the request its digest names.
    fn asked_of(&self, record: &Record) -> Option<Value> {
        let bytes = read_regular(&self.asked_path(record.change.as_str()), ASKED_LIMIT).ok()??;
        if bytes_sha256(&bytes) != record.request {
            return None;
        }
        serde_json::from_slice(&bytes).ok()
    }

    /// `record` as recovery looks at it, with its request when kept.
    fn intended_of(&self, record: &Record) -> Intended {
        Intended {
            change: record.change.clone(),
            operation: record.operation.clone(),
            request: record.request.clone(),
            asked: ASKED_KEPT
                .contains(&record.operation.as_str())
                .then(|| self.asked_of(record))
                .flatten(),
        }
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

    /// Remove every record file the current manifest does not name, and
    /// every kept request of a change it does not name: left by settled or
    /// replaced versions, and by writes a crash cut short.
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
            let asked = name
                .strip_suffix(&format!(".{ASKED}"))
                .is_some_and(|change| !manifest.records.contains_key(change));
            if asked || (record || left) && !named.contains(&path) {
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
        let digest = request_digest(request)?;
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
        // Kept for good before the intent names it; one a crash leaves
        // unnamed is swept with the next write.
        if ASKED_KEPT.contains(&operation) {
            write_atomic(
                &self.asked_path(change.as_str()),
                &serde_json::to_vec(request)?,
                0o600,
            )?;
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
        self.done_with(change, |_| reply)
    }

    /// Keep what `change`, a command, came to: `printed`, its `reply` with
    /// what it printed, while what every other record keeps of what
    /// commands printed leaves room for it within the store's bound; else
    /// `reply` alone, whose recovery then says its output is unavailable.
    ///
    /// # Errors
    ///
    /// As [`Changes::done`].
    pub fn done_printing(&self, change: &Token, reply: &Value, printed: &Value) -> Result<()> {
        let room = self.printed;
        self.done_with(change, |records| {
            let others: usize = records
                .iter()
                .filter(|(kept, _)| kept.as_str() != change.as_str())
                .map(|(_, record)| match &record.state {
                    State::Done { reply } => printed_size(reply),
                    State::Intended => 0,
                })
                .sum();
            if others.saturating_add(printed_size(printed)) <= room {
                printed
            } else {
                reply
            }
        })
    }

    /// Keep as what `change` came to the reply `choose` picks, given every
    /// record kept now.
    fn done_with<'a>(
        &self,
        change: &Token,
        choose: impl FnOnce(&Records) -> &'a Value,
    ) -> Result<()> {
        let mut last = self.verified();
        let (manifest, mut records) = self.trusted(&mut last)?;
        let reply = choose(&records);
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

    /// Every change kept as intended and not yet done: what recovery looks
    /// at once nothing can be running it.
    ///
    /// # Errors
    ///
    /// Fails when the store is unknown.
    pub fn intended(&self) -> Result<Vec<Intended>> {
        let mut last = self.verified();
        let (_, records) = self.trusted(&mut last)?;
        Ok(records
            .values()
            .filter(|record| record.state == State::Intended)
            .map(|record| self.intended_of(record))
            .collect())
    }

    /// Every change whose outcome is unknown: kept as intended, or done
    /// with an uncertain reply not yet proven ended. What [`Changes::ended`]
    /// may answer once its work is proven gone.
    ///
    /// # Errors
    ///
    /// Fails when the store is unknown.
    pub fn unresolved(&self) -> Result<Vec<Intended>> {
        let mut last = self.verified();
        let (_, records) = self.trusted(&mut last)?;
        Ok(records
            .values()
            .filter(|record| unresolved(&record.state))
            .map(|record| self.intended_of(record))
            .collect())
    }

    /// The work of `change` is proven gone: kept as ended, uncertain still,
    /// in place of an intent or an uncertain reply. A change kept with a
    /// certain reply, already ended, or not kept is left as it is. Whether
    /// it was kept as ended now.
    ///
    /// # Errors
    ///
    /// Fails when the store is unknown or cannot be written.
    pub fn ended(&self, change: &Token) -> Result<bool> {
        let mut last = self.verified();
        let (manifest, mut records) = self.trusted(&mut last)?;
        let Some(record) = records
            .get(change.as_str())
            .filter(|record| unresolved(&record.state))
        else {
            return Ok(false);
        };
        let mut record = Record::clone(record);
        record.state = State::Done {
            reply: ended_reply(),
        };
        records.insert(change.as_str().to_owned(), Arc::new(record));
        self.keep(&mut last, &manifest, records, &[change.as_str()])?;
        Ok(true)
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

/// Whether a change standing at `state` has an outcome nobody knows: an
/// intent, or an uncertain reply not proven ended.
fn unresolved(state: &State) -> bool {
    match state {
        State::Intended => true,
        State::Done { reply } => reply["uncertain"] == true && reply["ended"] != true,
    }
}

/// What a change whose work is proven gone came to: still not known to
/// have happened or not, and nothing of it runs any more.
fn ended_reply() -> Value {
    serde_json::json!({
        "ok": false,
        "code": "uncertain",
        "error": "What this change came to is not known, but its work is proven ended.",
        "uncertain": true,
        "ended": true,
    })
}

/// `reply`, whole when it is small enough, else what it came to, its texts
/// cut short.
fn kept(reply: &Value) -> Result<Value> {
    if serde_json::to_vec(reply)?.len() <= REPLY {
        return Ok(reply.clone());
    }
    Ok(shortened(reply))
}

/// How much of what a command printed `reply` keeps: its output events'
/// bytes, as their base64 says.
fn printed_size(reply: &Value) -> usize {
    reply["events"].as_array().map_or(0, |events| {
        events
            .iter()
            .filter(|event| event["event"] == "output")
            .filter_map(|event| event["data"].as_str())
            .map(|data| data.len() / 4 * 3)
            .sum()
    })
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

impl<V: Vault> crate::daemon::serve::Kept for Changes<V> {
    fn done(&self, change: &Token, reply: &Value) -> Result<()> {
        Changes::done(self, change, reply)
    }

    fn done_printing(&self, change: &Token, reply: &Value, printed: &Value) -> Result<()> {
        Changes::done_printing(self, change, reply, printed)
    }

    /// An intended change is unknown, never running: after a restart
    /// nothing says whether it ran.
    fn outcome(&self, change: &Token) -> Result<atlas_core::outcomes::Outcome> {
        use atlas_core::outcomes::Outcome;
        Ok(match self.of(change)? {
            Some(State::Done { reply }) => Outcome::Done { reply },
            Some(State::Intended) | None => Outcome::Unknown,
        })
    }
}

#[cfg(test)]
#[path = "changes_tests.rs"]
mod tests;
