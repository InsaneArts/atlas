//! Copies of chosen files from the helper, kept in Atlas's private output
//! folder with their size and digest, and the receipts that the task's
//! record and cleanup read. The files in the helper are never changed.
//!
//! Each file comes over the helper daemon's own socket in parts, each part
//! written to a private partial file as its events arrive, so a large file
//! is never held whole. A copy is kept only once its size and SHA-256 match
//! what the helper read, and under a name nothing else has: it is linked
//! into place, never written over anything.
mod receipt;
mod scavenge;

pub(crate) use receipt::{Intact, intact};
pub use receipt::{Kept, ReadSpan, ReceiptId};

use crate::{
    client::Client,
    daemon::protocol::{
        FileIdentity, READ, READ_PATH_MAX, READ_RANGE_MAX, READ_SIZE_MAX, ReadRefusal, SessionPin,
    },
    error::{Code, Error, Result},
    files::write_private,
    host_session::now_ms,
    task::{record::OutputEntry, store::Store},
    transport::{OutputError, append_event},
};
use receipt::{Receipt, VERSION};
use scavenge::remove_abandoned;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Component, Path, PathBuf},
    time::Instant,
};

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
/// The folder in the output folder that keeps collected copies.
const COLLECTED: &str = "collected";
/// The folder of the copies of a command with no task.
const UNLABELLED: &str = "unlabelled";
/// The longest a file name may be, in bytes.
const NAME_MAX: usize = 255;

/// Why one file was not collected.
#[derive(Debug, PartialEq, Eq)]
struct Failure {
    reason: ReadRefusal,
    error: String,
}

impl Failure {
    fn new(reason: ReadRefusal, source: &str) -> Self {
        let error = match reason {
            ReadRefusal::Outside => format!(
                "{source} is not in the helper's home, its temporary folder or /tmp, so it was not collected"
            ),
            ReadRefusal::Protected => format!(
                "{source} is in a folder macOS guards, such as Documents or Library, so it was not collected"
            ),
            ReadRefusal::Symlink => format!(
                "{source} goes through a link, which Atlas never follows; collect the file itself"
            ),
            ReadRefusal::NotRegular => {
                format!("{source} is not a regular file; archive a folder first with ditto -c -k")
            }
            ReadRefusal::Dataless => format!("{source}'s contents are not on this Mac yet"),
            ReadRefusal::Owner => format!("{source} does not belong to the helper"),
            ReadRefusal::TooLarge => format!(
                "{source} is larger than allowed: pass --max-size (at most 1G), and collect at most 1 GiB a call"
            ),
            ReadRefusal::Changing => format!(
                "{source} changed in the last 2 seconds, or while it was read, so it was not collected; wait until whatever writes it is done"
            ),
            ReadRefusal::Missing => format!("There is no {source} in the helper"),
            ReadRefusal::Name => format!("{source} has no name that can be a file here"),
            ReadRefusal::Exists => {
                format!("A file named like {source} was already collected in this call")
            }
            ReadRefusal::Space => format!("There is not enough room here for {source}"),
            ReadRefusal::SessionChanged => {
                "The helper changed since Atlas checked it, so this file was not collected".into()
            }
            ReadRefusal::OnConsole => {
                "The helper came on screen, so this file was not collected".into()
            }
            ReadRefusal::Unreadable | ReadRefusal::Gone => format!("{source} could not be read"),
        };
        Self { reason, error }
    }

    /// Whether the helper itself changed, so no later file is read either.
    fn ends_the_call(&self) -> bool {
        matches!(
            self.reason,
            ReadRefusal::SessionChanged | ReadRefusal::OnConsole
        )
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
        return Err(Error::new(Code::Usage, "Give 1 to 16 files to collect"));
    }
    if !(1..=READ_SIZE_MAX).contains(&max_size) {
        return Err(Error::new(
            Code::Usage,
            "Give a size such as 200M, at most 1G",
        ));
    }
    let person = crate::config::home().ok();
    for source in files {
        if let Some(refused) = given(source, person.as_deref()) {
            return Err(Error::new(Code::Collect, refused.error).with("reason", refused.reason));
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
    // The folder is checked before anything in it is looked at.
    let collected_root = private_folders(output, &[COLLECTED])?;
    remove_abandoned(&collected_root);

    let at = now_ms();
    let receipt = ReceiptId::new(at);
    let owner = client
        .task
        .as_ref()
        .map_or_else(|| UNLABELLED.to_owned(), |task| task.key().to_string());
    let folder = private_folders(output, &[COLLECTED, &owner, receipt.as_str()])?;
    let copies = private_folders(&folder, &["files"])?;

    let Copied {
        kept,
        failed,
        total,
    } = copy_each(client, pin, files, max_size, &folder, &copies).await;
    let receipt = write_receipt(client, &folder, receipt, at, &kept)?;
    let warnings = note_output(client, output, receipt.as_ref(), at, &kept, total);

    let _ = client
        .record(
            &json!({"op": "collect"}),
            &json!({"ok": failed.is_empty(), "count": kept.len(), "bytes": total,
            "code": (!failed.is_empty()).then_some("collect")}),
            started.elapsed(),
        )
        .await;

    Ok(reply(files.len(), receipt, kept, failed, &warnings))
}

/// What a call's files came to: the copies kept, the files that failed
/// and why, and the bytes kept, every copy together.
struct Copied {
    kept: Vec<Kept>,
    failed: Vec<(String, Failure)>,
    total: u64,
}

/// Collect each of `files` in turn, its partial file in `folder` and its
/// copy in `copies`. Once the helper itself changed, every later file is
/// refused unread.
async fn copy_each(
    client: &Client,
    pin: Option<SessionPin>,
    files: &[String],
    max_size: u64,
    folder: &Path,
    copies: &Path,
) -> Copied {
    let mut kept: Vec<Kept> = Vec::new();
    let mut failed: Vec<(String, Failure)> = Vec::new();
    let mut total = 0;
    let mut changed = false;
    for (n, source) in files.iter().enumerate() {
        if changed {
            failed.push((
                source.clone(),
                Failure::new(ReadRefusal::SessionChanged, source),
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
            copies,
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
    Copied {
        kept,
        failed,
        total,
    }
}

/// Write the receipt of the copies `kept` into `folder`, made at `at`: the
/// receipt, or none, and the folder removed, when nothing was kept.
fn write_receipt(
    client: &Client,
    folder: &Path,
    receipt: ReceiptId,
    at: u64,
    kept: &[Kept],
) -> Result<Option<ReceiptId>> {
    if kept.is_empty() {
        let _ = fs::remove_dir_all(folder);
        return Ok(None);
    }
    let written = Receipt {
        version: VERSION,
        receipt: receipt.clone(),
        at,
        task: client.task.as_ref().map(|task| task.key().clone()),
        files: kept.to_vec(),
    };
    write_private(&folder.join("receipt.json"), &serde_json::to_vec(&written)?)?;
    Ok(Some(receipt))
}

/// Note the collection `receipt` in its task's record in `output`: the
/// warnings, when it could not be.
fn note_output(
    client: &Client,
    output: &Path,
    receipt: Option<&ReceiptId>,
    at: u64,
    kept: &[Kept],
    total: u64,
) -> Vec<&'static str> {
    let mut warnings = Vec::new();
    if let (Some(receipt), Some(task)) = (receipt, &client.task) {
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
    warnings
}

/// The reply to a call that asked for `asked` files: its receipt, the
/// copies kept, and, when any failed, which and why.
fn reply(
    asked: usize,
    receipt: Option<ReceiptId>,
    kept: Vec<Kept>,
    failed: Vec<(String, Failure)>,
    warnings: &[&str],
) -> Value {
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
            asked,
            if asked == 1 { "file was" } else { "files were" }
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
    reply
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
        return Some(Failure::new(ReadRefusal::Outside, source));
    }
    if person.is_some_and(|home| path.is_absolute() && path.starts_with(home)) {
        return Some(Failure {
            reason: ReadRefusal::Outside,
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

/// What the helper said of a file it sent whole: the version read, the
/// path it read, and that path relative to its home, when it is in it.
struct Received {
    identity: FileIdentity,
    real: String,
    in_home: Option<String>,
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
        let name = self.name()?;

        let started = now_ms();
        let mut partial = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&self.partial)
            .map_err(|_| fail(ReadRefusal::Space))?;
        let mut hasher = Sha256::new();
        let Received {
            identity,
            real,
            in_home,
        } = self.receive(client, pin, &mut partial, &mut hasher).await?;
        let sha256 = crate::name::hex(&hasher.finalize());
        self.confirm(client, pin, identity, &mut partial, &sha256)
            .await?;
        partial.sync_all().map_err(|_| fail(ReadRefusal::Space))?;
        drop(partial);

        let path = self.link(name)?;
        Ok(Kept {
            source: real,
            in_home,
            path,
            size: identity.size,
            sha256,
            read: Some(ReadSpan {
                started,
                finished: now_ms(),
            }),
        })
    }

    /// The name its copy takes: the source's own, when it can be a file
    /// here and no file collected before it in this call has it.
    fn name(&self) -> std::result::Result<&str, Failure> {
        let fail = |reason| Failure::new(reason, self.source);
        let name = Path::new(self.source)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| {
                !name.is_empty() && name.len() <= NAME_MAX && !name.chars().any(char::is_control)
            })
            .ok_or_else(|| fail(ReadRefusal::Name))?;
        if self
            .taken
            .iter()
            .any(|taken| taken.eq_ignore_ascii_case(name))
        {
            return Err(fail(ReadRefusal::Exists));
        }
        Ok(name)
    }

    /// Read the file part by part into `partial`, hashing each part, every
    /// part of the same version as the first: once the first says how large
    /// it is, it must fit what is left of the call and of the disk.
    async fn receive(
        &self,
        client: &Client,
        pin: Option<SessionPin>,
        partial: &mut File,
        hasher: &mut Sha256,
    ) -> std::result::Result<Received, Failure> {
        let fail = |reason| Failure::new(reason, self.source);
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
            let (reply, received) = read_part(client, &request, partial, hasher).await?;
            let file: FileIdentity = serde_json::from_value(json!({
                "dev": reply["file"]["dev"],
                "ino": reply["file"]["ino"],
                "size": reply["file"]["size"],
                "mtime": reply["file"]["mtime"],
                "ctime": reply["file"]["ctime"],
            }))
            .map_err(|_| fail(ReadRefusal::Unreadable))?;
            if reply["length"].as_u64() != Some(received) {
                return Err(fail(ReadRefusal::Changing));
            }
            if identity.is_none() {
                if file.size > self.room {
                    return Err(fail(ReadRefusal::TooLarge));
                }
                if !room_for(self.copies, file.size) {
                    return Err(fail(ReadRefusal::Space));
                }
                real = reply["file"]["path"]
                    .as_str()
                    .unwrap_or(self.source)
                    .to_owned();
                in_home = reply["file"]["inHome"].as_str().map(str::to_owned);
            } else if identity != Some(file) {
                return Err(fail(ReadRefusal::Changing));
            }
            identity = Some(file);
            offset += received;
            if offset >= file.size || received == 0 {
                break;
            }
        }
        let Some(identity) = identity else {
            return Err(fail(ReadRefusal::Unreadable));
        };
        if offset != identity.size {
            return Err(fail(ReadRefusal::Changing));
        }
        Ok(Received {
            identity,
            real,
            in_home,
        })
    }

    /// Check that the helper's digest of the version `identity` is
    /// `sha256`, the digest of the parts it sent.
    async fn confirm(
        &self,
        client: &Client,
        pin: Option<SessionPin>,
        identity: FileIdentity,
        partial: &mut File,
        sha256: &str,
    ) -> std::result::Result<(), Failure> {
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
        let (reply, _) = read_part(client, &request, partial, &mut Sha256::new()).await?;
        if reply["sha256"].as_str() != Some(sha256) {
            return Err(Failure::new(ReadRefusal::Changing, self.source));
        }
        Ok(())
    }

    /// Link the finished partial file into the copies folder as `name`,
    /// never over anything: the copy's path.
    fn link(&self, name: &str) -> std::result::Result<PathBuf, Failure> {
        let fail = |reason| Failure::new(reason, self.source);
        let path = self.copies.join(name);
        match fs::hard_link(&self.partial, &path) {
            Ok(()) => Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(fail(ReadRefusal::Exists))
            }
            Err(_) => Err(fail(ReadRefusal::Space)),
        }
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
                        Code::Protocol,
                        match error {
                            OutputError::Malformed => "A part of the file came back malformed",
                            OutputError::Oversized => "A part of the file came back too large",
                        },
                    )
                })?;
                received += buffer.len() as u64;
                if received > READ_RANGE_MAX {
                    return Err(Error::new(
                        Code::Protocol,
                        "A part of the file came back too large",
                    ));
                }
                hasher.update(&buffer);
                if let Err(error) = partial.write_all(&buffer) {
                    written = Err(error);
                    return Err(Error::new(Code::Io, "The copy could not be written"));
                }
                Ok(())
            })
            .await
    }
    .await;
    if written.is_err() {
        return Err(fail(ReadRefusal::Space));
    }
    let reply = reply.map_err(|error| match error.code {
        Code::SessionChanged => fail(ReadRefusal::SessionChanged),
        Code::OnConsole => fail(ReadRefusal::OnConsole),
        _ => fail(ReadRefusal::Unreadable),
    })?;
    if reply["ok"] != true {
        let reason = match (ReadRefusal::of(&reply), reply["code"].as_str()) {
            (Some(reason), _) => reason,
            (None, Some("session-changed")) => ReadRefusal::SessionChanged,
            (None, Some("on-console")) => ReadRefusal::OnConsole,
            (None, _) => ReadRefusal::Unreadable,
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
                Code::State,
                format!(
                    "{} is a link, or not a private folder of this account, so nothing was collected",
                    folder.display()
                ),
            ));
        }
    }
    Ok(folder)
}

#[cfg(test)]
mod tests;
