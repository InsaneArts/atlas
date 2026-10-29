//! Copies of chosen files from the helper, kept in Atlas's private output
//! folder with their size and digest, and the receipts that the task's
//! record and cleanup read. The files in the helper are never changed.
//!
//! Each file comes over the helper daemon's own socket in parts, each part
//! written to a private partial file as its events arrive, so a large file
//! is never held whole. A copy is kept only once its size and SHA-256 match
//! what the helper read, and under a name nothing else has: it is linked
//! into place, never written over anything.
use crate::{
    client::Client,
    control::write_private,
    daemon::protocol::{
        FileIdentity, READ, READ_PATH_MAX, READ_RANGE_MAX, READ_SIZE_MAX, SessionPin,
    },
    digest::file_sha256,
    error::{Error, Result},
    host_session::{Key, now_ms},
    task::{record::OutputEntry, store::Store},
    transport::{OutputError, append_event},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

mod reason;

pub use reason::Refused;

/// The most files one call collects.
pub const MAX_FILES: usize = 16;
/// The most bytes one call collects, every file together.
pub const MAX_TOTAL: u64 = 1 << 30;
/// The largest file collected unless the agent allows more.
pub const DEFAULT_SIZE: u64 = 64 << 20;
/// What an older helper daemon is told it cannot do.
const OLDER: &str = "The helper is running an older Atlas that cannot collect files; run atlas restart, then try again";
/// Room left free beside a copy, so collecting never fills the disk.
const ROOM: u64 = 64 << 20;
/// A partial file older than this was left by a collection that was killed.
const ABANDONED: Duration = Duration::from_secs(60 * 60);
/// The folder in the output folder that keeps collected copies.
const COLLECTED: &str = "collected";
/// The folder of the copies of a command with no task.
const UNLABELLED: &str = "unlabelled";
/// The receipt format this build writes.
const VERSION: u32 = 1;
/// The longest a file name may be, in bytes.
const NAME_MAX: usize = 255;

/// A collection's receipt: when it was made, in milliseconds since 1970,
/// and six random hexadecimal digits, so two collections never share one.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ReceiptId(String);

impl ReceiptId {
    /// A new receipt, made at `at`.
    fn new(at: u64) -> Self {
        let mut random = crate::control::token();
        random.truncate(6);
        Self(format!("{at}-{random}"))
    }

    /// `text` as a receipt, when it is one of Atlas's.
    pub fn parse(text: &str) -> Option<Self> {
        let (at, random) = text.split_once('-')?;
        let at_ok =
            !at.is_empty() && at.len() <= 20 && at.bytes().all(|byte| byte.is_ascii_digit());
        (at_ok && crate::digest::lower_hex(random, 6)).then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ReceiptId {
    type Error = String;

    fn try_from(text: String) -> std::result::Result<Self, Self::Error> {
        Self::parse(&text).ok_or_else(|| format!("{text} is not a collection's receipt"))
    }
}

impl From<ReceiptId> for String {
    fn from(receipt: ReceiptId) -> Self {
        receipt.0
    }
}

impl std::fmt::Display for ReceiptId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// One file a collection kept: where it was in the helper, where its copy
/// is, and what the copy must still be.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Kept {
    pub source: String,
    /// The source relative to the helper's home, when it is in it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_home: Option<String>,
    pub path: PathBuf,
    pub size: u64,
    pub sha256: String,
}

/// A collection's receipt file, beside its copies.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Receipt {
    pub version: u32,
    pub receipt: ReceiptId,
    pub at: u64,
    pub task: Option<Key>,
    pub files: Vec<Kept>,
}

/// Why one file was not collected.
#[derive(Debug, PartialEq, Eq)]
struct Failure {
    reason: Refused,
    error: String,
}

impl Failure {
    fn new(reason: Refused, source: &str) -> Self {
        let error = match reason {
            Refused::Outside => format!(
                "{source} is not in the helper's home, its temporary folder or /tmp, so it was not collected"
            ),
            Refused::Protected => format!(
                "{source} is in a folder macOS guards, such as Documents or Library, so it was not collected"
            ),
            Refused::Symlink => format!(
                "{source} goes through a link, which Atlas never follows; collect the file itself"
            ),
            Refused::NotRegular => {
                format!("{source} is not a regular file; archive a folder first with ditto -c -k")
            }
            Refused::Dataless => format!("{source}'s contents are not on this Mac yet"),
            Refused::Owner => format!("{source} does not belong to the helper"),
            Refused::TooLarge => format!(
                "{source} is larger than allowed: pass --max-size (at most 1G), and collect at most 1 GiB a call"
            ),
            Refused::Changing => format!(
                "{source} changed in the last 2 seconds, or while it was read, so it was not collected; wait until whatever writes it is done"
            ),
            Refused::Missing => format!("There is no {source} in the helper"),
            Refused::Name => format!("{source} has no name that can be a file here"),
            Refused::Exists => {
                format!("A file named like {source} was already collected in this call")
            }
            Refused::Space => format!("There is not enough room here for {source}"),
            Refused::SessionChanged => {
                "The helper changed since Atlas checked it, so this file was not collected".into()
            }
            Refused::OnConsole => {
                "The helper came on screen, so this file was not collected".into()
            }
            Refused::Unreadable | Refused::Gone => format!("{source} could not be read"),
        };
        Self { reason, error }
    }

    /// Whether the helper itself changed, so no later file is read either.
    fn ends_the_call(&self) -> bool {
        matches!(self.reason, Refused::SessionChanged | Refused::OnConsole)
    }
}

/// Copy `files` out of the helper into a private folder of their own, each
/// a regular file the helper owns of at most `max_size` bytes. The reply
/// names each copy's path, size and SHA-256, never its contents; files
/// collected before one that fails stay collected. Nothing here is ever of
/// unknown outcome: it changes nothing in the helper, and here it either
/// kept a verified copy or removed what it began.
pub async fn collect(client: &Client, files: &[String], max_size: u64) -> Result<Value> {
    let started = Instant::now();
    if files.is_empty() || files.len() > MAX_FILES {
        return Err(Error::new("usage", "Give 1 to 16 files to collect"));
    }
    if !(1..=READ_SIZE_MAX).contains(&max_size) {
        return Err(Error::new("usage", "Give a size such as 200M, at most 1G"));
    }
    let person = crate::config::home().ok();
    for source in files {
        if let Some(refused) = given(source, person.as_deref()) {
            return Err(Error::new("collect", refused.error).with("reason", refused.reason));
        }
    }

    let verified = client.verify_offering(READ, OLDER).await?;
    collected(client, verified.pin(), files, max_size, started).await
}

/// [`collect`] once the helper is verified, every read pinned to the
/// session verified: a daemon that offers reads refuses each in any other,
/// so a helper that changed between two files has the rest refused.
async fn collected(
    client: &Client,
    pin: Option<SessionPin>,
    files: &[String],
    max_size: u64,
    started: Instant,
) -> Result<Value> {
    let output = client.output_dir()?;
    let collected_root = output.join(COLLECTED);
    remove_abandoned(&collected_root);

    let at = now_ms();
    let receipt = ReceiptId::new(at);
    let owner = client
        .task
        .as_ref()
        .map_or_else(|| UNLABELLED.to_owned(), |task| task.key().to_string());
    let folder = private_folders(output, &[COLLECTED, &owner, receipt.as_str()])?;
    let copies = private_folders(&folder, &["files"])?;

    let mut kept: Vec<Kept> = Vec::new();
    let mut failed: Vec<(String, Failure)> = Vec::new();
    let mut total = 0;
    let mut changed = false;
    for (n, source) in files.iter().enumerate() {
        if changed {
            failed.push((
                source.clone(),
                Failure::new(Refused::SessionChanged, source),
            ));
            continue;
        }
        let names: Vec<&str> = kept
            .iter()
            .filter_map(|kept| kept.path.file_name()?.to_str())
            .collect();
        let plan = One {
            source,
            max_size,
            room: MAX_TOTAL.saturating_sub(total),
            partial: folder.join(format!(".{n}.partial")),
            copies: &copies,
            taken: &names,
        };
        match plan.collect(client, pin).await {
            Ok(file) => {
                total += file.size;
                kept.push(file);
            }
            Err(failure) => {
                changed |= failure.ends_the_call();
                failed.push((source.clone(), failure));
            }
        }
    }

    let receipt = if kept.is_empty() {
        let _ = fs::remove_dir_all(&folder);
        None
    } else {
        let written = Receipt {
            version: VERSION,
            receipt: receipt.clone(),
            at,
            task: client.task.as_ref().map(|task| task.key().clone()),
            files: kept.clone(),
        };
        write_private(&folder.join("receipt.json"), &serde_json::to_vec(&written)?)?;
        Some(receipt)
    };

    let mut warnings = Vec::new();
    if let (Some(receipt), Some(task)) = (&receipt, &client.task) {
        let entry = OutputEntry {
            receipt: receipt.clone(),
            at,
            files: kept.len(),
            bytes: total,
            later: Map::new(),
        };
        let noted = Store::at(output).update(task, now_ms(), |record| {
            record.noted_output(entry);
            Ok(())
        });
        if noted.is_err() {
            warnings.push("Atlas could not note this collection in the task's record");
        }
    }

    let _ = client.record(
        &json!({"op": "collect"}),
        &json!({"ok": failed.is_empty(), "count": kept.len(), "bytes": total,
            "code": (!failed.is_empty()).then_some("collect")}),
        started.elapsed(),
    );

    let mut reply = json!({
        "ok": failed.is_empty(),
        "receipt": receipt,
        "collected": kept,
    });
    if !failed.is_empty() {
        reply["code"] = json!("collect");
        reply["uncertain"] = json!(false);
        reply["error"] = json!(format!(
            "{} of {} {} not collected",
            failed.len(),
            files.len(),
            if files.len() == 1 {
                "file was"
            } else {
                "files were"
            }
        ));
        reply["failed"] = failed
            .into_iter()
            .map(|(source, failure)| {
                json!({"source": source, "reason": failure.reason, "error": failure.error})
            })
            .collect();
    }
    if !warnings.is_empty() {
        reply["warnings"] = json!(warnings);
    }
    Ok(reply)
}

/// Why `source` is refused before it is sent: a path, not a pattern, with
/// no `.` or `..` part, of at most 1024 bytes, and not in `person`'s own
/// home, which the shell gives for `~`.
fn given(source: &str, person: Option<&Path>) -> Option<Failure> {
    let path = Path::new(source);
    let dotted = path
        .components()
        .any(|part| matches!(part, Component::CurDir | Component::ParentDir));
    if source.is_empty() || source.len() > READ_PATH_MAX || source.contains('\0') || dotted {
        return Some(Failure::new(Refused::Outside, source));
    }
    if person.is_some_and(|home| path.is_absolute() && path.starts_with(home)) {
        return Some(Failure {
            reason: Refused::Outside,
            error: "Give a path in the helper's home, relative to it, such as \
                    atlas-work/report.pdf: your shell turns ~ into your own home"
                .into(),
        });
    }
    None
}

/// One file to collect, and where.
struct One<'a> {
    source: &'a str,
    max_size: u64,
    /// What is left of the call's 1 GiB.
    room: u64,
    /// The private file its parts are written to.
    partial: PathBuf,
    /// The folder its copy is linked into.
    copies: &'a Path,
    /// The names already collected in this call.
    taken: &'a [&'a str],
}

impl One<'_> {
    /// Collect it, removing whatever was begun when it fails.
    async fn collect(
        &self,
        client: &Client,
        pin: Option<SessionPin>,
    ) -> std::result::Result<Kept, Failure> {
        let collected = self.copy(client, pin).await;
        let _ = fs::remove_file(&self.partial);
        collected
    }

    async fn copy(
        &self,
        client: &Client,
        pin: Option<SessionPin>,
    ) -> std::result::Result<Kept, Failure> {
        let fail = |reason| Failure::new(reason, self.source);
        let name = Path::new(self.source)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| {
                !name.is_empty() && name.len() <= NAME_MAX && !name.chars().any(char::is_control)
            })
            .ok_or_else(|| fail(Refused::Name))?;
        if self
            .taken
            .iter()
            .any(|taken| taken.eq_ignore_ascii_case(name))
        {
            return Err(fail(Refused::Exists));
        }

        let mut partial = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&self.partial)
            .map_err(|_| fail(Refused::Space))?;
        let mut hasher = Sha256::new();
        let mut identity: Option<FileIdentity> = None;
        let mut real = String::new();
        let mut in_home = None;
        let mut offset = 0;
        loop {
            let mut request = json!({
                "op": "read",
                "path": self.source,
                "offset": offset,
                "length": READ_RANGE_MAX,
                "maxSize": self.max_size,
            });
            if let Some(expect) = &identity {
                request["expect"] = json!(expect);
            }
            if let Some(pin) = pin {
                request["session"] = json!(pin);
            }
            let (reply, received) = read_part(client, &request, &mut partial, &mut hasher).await?;
            let file: FileIdentity = serde_json::from_value(json!({
                "dev": reply["file"]["dev"],
                "ino": reply["file"]["ino"],
                "size": reply["file"]["size"],
                "mtime": reply["file"]["mtime"],
                "ctime": reply["file"]["ctime"],
            }))
            .map_err(|_| fail(Refused::Unreadable))?;
            if reply["length"].as_u64() != Some(received) {
                return Err(fail(Refused::Changing));
            }
            if identity.is_none() {
                if file.size > self.room {
                    return Err(fail(Refused::TooLarge));
                }
                if !room_for(self.copies, file.size) {
                    return Err(fail(Refused::Space));
                }
                real = reply["file"]["path"]
                    .as_str()
                    .unwrap_or(self.source)
                    .to_owned();
                in_home = reply["file"]["inHome"].as_str().map(str::to_owned);
            } else if identity != Some(file) {
                return Err(fail(Refused::Changing));
            }
            identity = Some(file);
            offset += received;
            if offset >= file.size || received == 0 {
                break;
            }
        }
        let Some(identity) = identity else {
            return Err(fail(Refused::Unreadable));
        };
        if offset != identity.size {
            return Err(fail(Refused::Changing));
        }

        let mut request = json!({
            "op": "read",
            "path": self.source,
            "digest": true,
            "length": 0,
            "maxSize": self.max_size,
            "expect": identity,
        });
        if let Some(pin) = pin {
            request["session"] = json!(pin);
        }
        let (reply, _) = read_part(client, &request, &mut partial, &mut Sha256::new()).await?;
        let sha256: String = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if reply["sha256"].as_str() != Some(sha256.as_str()) {
            return Err(fail(Refused::Changing));
        }
        partial.sync_all().map_err(|_| fail(Refused::Space))?;
        drop(partial);

        let path = self.copies.join(name);
        match fs::hard_link(&self.partial, &path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(fail(Refused::Exists));
            }
            Err(_) => return Err(fail(Refused::Space)),
        }
        Ok(Kept {
            source: real,
            in_home,
            path,
            size: identity.size,
            sha256,
        })
    }
}

/// Send one read `request` and write the bytes of its `data` events to
/// `partial` as they arrive, hashing them: the final reply, and how many
/// bytes came. A refusal is the reason it gives.
async fn read_part(
    client: &Client,
    request: &Value,
    partial: &mut File,
    hasher: &mut Sha256,
) -> std::result::Result<(Value, u64), Failure> {
    let source = request["path"].as_str().unwrap_or_default();
    let fail = |reason| Failure::new(reason, source);
    let mut buffer = Vec::new();
    let mut received = 0_u64;
    let mut written = Ok(());
    let reply = async {
        let pending = client.transport.connect().await?.dispatch(request).await?;
        pending
            .finish_each(&mut |event| {
                if event["event"] != "data" {
                    return Ok(());
                }
                buffer.clear();
                append_event(&event, &mut buffer, READ_RANGE_MAX as usize).map_err(|error| {
                    Error::new(
                        "protocol",
                        match error {
                            OutputError::Malformed => "A part of the file came back malformed",
                            OutputError::Oversized => "A part of the file came back too large",
                        },
                    )
                })?;
                received += buffer.len() as u64;
                if received > READ_RANGE_MAX {
                    return Err(Error::new(
                        "protocol",
                        "A part of the file came back too large",
                    ));
                }
                hasher.update(&buffer);
                if let Err(error) = partial.write_all(&buffer) {
                    written = Err(error);
                    return Err(Error::new("io", "The copy could not be written"));
                }
                Ok(())
            })
            .await
    }
    .await;
    if written.is_err() {
        return Err(fail(Refused::Space));
    }
    let reply = reply.map_err(|error| match error.code {
        "session-changed" => fail(Refused::SessionChanged),
        "on-console" => fail(Refused::OnConsole),
        _ => fail(Refused::Unreadable),
    })?;
    if reply["ok"] != true {
        let reason = match (Refused::of(&reply), reply["code"].as_str()) {
            (Some(reason), _) => reason,
            (None, Some("session-changed")) => Refused::SessionChanged,
            (None, Some("on-console")) => Refused::OnConsole,
            (None, _) => Refused::Unreadable,
        };
        return Err(fail(reason));
    }
    Ok((reply, received))
}

/// Whether the volume of `folder` has room for `size` bytes and
/// [`ROOM`] more.
fn room_for(folder: &Path, size: u64) -> bool {
    nix::sys::statvfs::statvfs(folder).is_ok_and(|stats| {
        u64::from(stats.blocks_available()).saturating_mul(stats.fragment_size())
            >= size.saturating_add(ROOM)
    })
}

/// `root` joined with each of `levels`, each made a private folder of this
/// account one at a time, or found to be one already: a link, or a folder
/// that is not this account's, is refused.
fn private_folders(root: &Path, levels: &[&str]) -> Result<PathBuf> {
    let me = nix::unistd::getuid().as_raw();
    let mut folder = root.to_owned();
    for level in levels {
        folder.push(level);
        match fs::DirBuilder::new().mode(0o700).create(&folder) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(Error::writing(&folder, error)),
        }
        let private = fs::symlink_metadata(&folder).is_ok_and(|metadata| {
            metadata.is_dir()
                && !metadata.file_type().is_symlink()
                && metadata.uid() == me
                && metadata.mode() & 0o077 == 0
        });
        if !private {
            return Err(Error::new(
                "state",
                format!(
                    "{} is a link, or not a private folder of this account, so nothing was collected",
                    folder.display()
                ),
            ));
        }
    }
    Ok(folder)
}

/// Remove what collections killed partway left under `root`, once it is an
/// hour old: their partial files, and the folder of one that never wrote
/// its receipt, whose copies were never reported.
fn remove_abandoned(root: &Path) {
    let Ok(owners) = fs::read_dir(root) else {
        return;
    };
    for owner in owners.flatten() {
        let Ok(receipts) = fs::read_dir(owner.path()) else {
            continue;
        };
        for receipt in receipts.flatten() {
            let folder = receipt.path();
            let stale = receipt
                .metadata()
                .ok()
                .filter(|metadata| metadata.is_dir())
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age > ABANDONED);
            let named = receipt
                .file_name()
                .to_str()
                .is_some_and(|name| ReceiptId::parse(name).is_some());
            if stale && named && !folder.join("receipt.json").exists() {
                let _ = fs::remove_dir_all(&folder);
                continue;
            }
            let Ok(entries) = fs::read_dir(&folder) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name();
                let partial = name
                    .to_str()
                    .is_some_and(|name| name.starts_with('.') && name.ends_with(".partial"));
                let old = entry
                    .metadata()
                    .ok()
                    .filter(|metadata| metadata.is_file())
                    .and_then(|metadata| metadata.modified().ok())
                    .and_then(|modified| modified.elapsed().ok())
                    .is_some_and(|age| age > ABANDONED);
                if partial && old {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }
}

/// How a collection's copies stand now.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Intact {
    /// The files whose copies are as they were collected.
    pub kept: Vec<Kept>,
    /// The files whose copies are gone.
    pub missing: Vec<Kept>,
    /// The files whose copies no longer have their size and digest.
    pub changed: Vec<Kept>,
    /// The receipt itself is gone or cannot be read, so which files it
    /// kept is not known.
    pub unknown: bool,
}

impl Intact {
    /// Whether every copy is as it was collected.
    pub fn whole(&self) -> bool {
        self.missing.is_empty() && self.changed.is_empty() && !self.unknown
    }
}

/// Whether the copies the collection `receipt` of `task` kept in `output`
/// still have their recorded size and digest. A receipt that cannot be
/// read leaves nothing known kept.
pub(crate) fn intact(output: &Path, task: Option<&Key>, receipt: &ReceiptId) -> Intact {
    let owner = task.map_or_else(|| UNLABELLED.to_owned(), Key::to_string);
    let path = output
        .join(COLLECTED)
        .join(owner)
        .join(receipt.as_str())
        .join("receipt.json");
    let read = crate::helper_info::read_own(&path, 1024 * 1024)
        .and_then(|bytes| serde_json::from_slice::<Receipt>(&bytes).ok())
        .filter(|read| read.receipt == *receipt);
    let Some(read) = read else {
        return Intact {
            unknown: true,
            ..Intact::default()
        };
    };
    let mut intact = Intact::default();
    for file in read.files {
        let size = fs::symlink_metadata(&file.path)
            .ok()
            .filter(|metadata| metadata.is_file())
            .map(|metadata| metadata.len());
        match size {
            None => intact.missing.push(file),
            Some(size) if size != file.size => intact.changed.push(file),
            Some(_) if file_sha256(&file.path).as_deref() != Some(file.sha256.as_str()) => {
                intact.changed.push(file);
            }
            Some(_) => intact.kept.push(file),
        }
    }
    intact
}

#[cfg(test)]
mod tests;
