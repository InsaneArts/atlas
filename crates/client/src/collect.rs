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

pub(crate) use receipt::{Intact, Kept, ReadSpan, ReceiptId, intact};

use crate::{
    client::Client,
    task::{Task, record::OutputEntry, store::Store},
};
use atlas_core::{
    error::{Code, Error, Result},
    files::write_private,
    protocol::{
        FileIdentity, READ, READ_PATH_MAX, READ_RANGE_MAX, READ_SIZE_MAX, ReadRefusal, SessionPin,
    },
    time::now_ms,
    transport::{OutputError, Transport, append_event, refusal},
};
use receipt::{Receipt, VERSION, owner_folder};
use scavenge::remove_abandoned;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
use tokio::runtime::Handle;

/// The most files one call collects.
pub(crate) const MAX_FILES: usize = 16;
/// The most bytes one call collects, every file together.
pub(crate) const MAX_TOTAL: u64 = 1 << 30;
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
        Self {
            reason,
            error: format!("{source}: {}", reason.said()),
        }
    }
}

/// Copy `files` out of the helper into a private folder of their own, each
/// a regular file the helper owns of at most `max_size` bytes. The reply
/// names each copy's path, size and SHA-256, never its contents; files
/// collected before one that fails stay collected. Nothing here is ever of
/// unknown outcome: it changes nothing in the helper, and here it either
/// kept a verified copy or removed what it began.
///
/// # Errors
///
/// Rejects invalid collection arguments, unavailable helper identity or output-storage
/// failures. Per-file refusals remain in the reply; only verified size and digest
/// completion publishes a collected copy.
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
    let person = atlas_core::config::home().ok();
    for source in files {
        if let Some(refused) = given(source, person.as_deref()) {
            return Err(
                Error::new(Code::Collect, refused.error).with("reason", json!(refused.reason))
            );
        }
    }

    let verified = client.verify_offering(READ, OLDER).await?;
    collected(client, verified.pin()?, files, max_size, started).await
}

/// [`collect`] once the helper is verified, every read pinned to the
/// session verified: a daemon that offers reads refuses each in any other,
/// so a helper that changed between two files has the rest refused.
async fn collected(
    client: &Client,
    pin: SessionPin,
    files: &[String],
    max_size: u64,
    started: Instant,
) -> Result<Value> {
    let Collected {
        receipt,
        kept,
        failed,
        warning,
    } = Collection::of(client, pin, files, max_size)?.run().await?;
    let total: u64 = kept.iter().map(|file| file.size).sum();

    let _ = client
        .record(
            &json!({"op": "collect"}),
            &json!({"ok": failed.is_empty(), "count": kept.len(), "bytes": total,
            "code": (!failed.is_empty()).then_some("collect")}),
            started.elapsed(),
        )
        .await;

    Ok(reply(files.len(), receipt, kept, failed, warning))
}

/// Where a test may hold or fail a copy: before each part is appended to
/// its partial file, or before that file is synced.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Append,
    Sync,
}

/// What a test does at each [`Step`].
#[cfg(test)]
type Fault = dyn Fn(Step) -> std::io::Result<()> + Send + Sync;

/// One call's collection on this Mac: the files it reads from the helper,
/// pinned to the session verified, and where it keeps their copies.
struct Collection {
    transport: Transport,
    output: PathBuf,
    task: Option<Task>,
    pin: SessionPin,
    files: Vec<String>,
    max_size: u64,
    #[cfg(test)]
    fault: Arc<Fault>,
}

/// What one call collected: its receipt, the copies kept, the files that
/// failed and why, and a warning when its task's record could not note it.
struct Collected {
    receipt: Option<ReceiptId>,
    kept: Vec<Kept>,
    failed: Vec<(String, Failure)>,
    warning: Option<&'static str>,
}

impl Collection {
    /// The collection of `files` that `client` asks for.
    fn of(client: &Client, pin: SessionPin, files: &[String], max_size: u64) -> Result<Self> {
        Ok(Self {
            transport: client.transport.clone(),
            output: client.output_dir()?.to_owned(),
            task: client.task.clone(),
            pin,
            files: files.to_vec(),
            max_size,
            #[cfg(test)]
            fault: Arc::new(|_| Ok(())),
        })
    }

    /// Collect every file into a folder of its own, write its receipt and
    /// note it in its task's record, on a blocking thread that asks the
    /// helper for each part through this runtime: writing and syncing the
    /// copies never holds up the runtime's other work. A caller that goes
    /// away stops it before its next read and before anything is kept; it
    /// still removes what it began.
    async fn run(self) -> Result<Collected> {
        let runtime = Handle::current();
        let left = Arc::new(AtomicBool::new(false));
        let _left = Left(left.clone());
        tokio::task::spawn_blocking(move || self.write(&runtime, &left))
            .await
            .map_err(|error| {
                Error::new(Code::Collect, "The collection stopped unexpectedly").with_source(error)
            })?
    }

    fn write(&self, runtime: &Handle, left: &AtomicBool) -> Result<Collected> {
        // The folder is checked before anything in it is looked at.
        let collected_root = private_folders(&self.output, &[COLLECTED])?;
        remove_abandoned(&collected_root);

        let at = now_ms();
        let receipt = ReceiptId::new(at);
        let owner = owner_folder(self.task.as_ref().map(Task::key));
        let folder = private_folders(&self.output, &[COLLECTED, &owner, receipt.as_str()])?;
        let copies = private_folders(&folder, &["files"])?;

        let (kept, failed) = self.copy_each(&folder, &copies, runtime, left);
        if left.load(Ordering::SeqCst) {
            // No one learns of these copies: remove them, as when none was kept.
            let _ = fs::remove_dir_all(&folder);
            return Ok(Collected {
                receipt: None,
                kept: Vec::new(),
                failed,
                warning: None,
            });
        }
        let total: u64 = kept.iter().map(|file| file.size).sum();
        let receipt = write_receipt(self.task.as_ref(), &folder, receipt, at, &kept)?;
        let warning = note_output(
            self.task.as_ref(),
            &self.output,
            receipt.as_ref(),
            at,
            &kept,
            total,
        );
        Ok(Collected {
            receipt,
            kept,
            failed,
            warning,
        })
    }

    /// Collect each file in turn, its partial file in `folder` and its copy
    /// in `copies`: the copies kept, and the files that failed and why.
    /// Once the helper itself changed, every later file is refused unread;
    /// once the caller left, none is.
    fn copy_each(
        &self,
        folder: &Path,
        copies: &Path,
        runtime: &Handle,
        left: &AtomicBool,
    ) -> (Vec<Kept>, Vec<(String, Failure)>) {
        let mut kept: Vec<Kept> = Vec::new();
        let mut failed: Vec<(String, Failure)> = Vec::new();
        let mut total = 0;
        let mut changed = false;
        for (n, source) in self.files.iter().enumerate() {
            if left.load(Ordering::SeqCst) {
                break;
            }
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
                max_size: self.max_size,
                room: MAX_TOTAL.saturating_sub(total),
                partial: folder.join(format!(".{n}.partial")),
                copies,
                taken: &names,
                transport: &self.transport,
                runtime,
                left,
                pin: self.pin,
                #[cfg(test)]
                fault: &*self.fault,
            };
            match plan.collect() {
                Ok(file) => {
                    total += file.size;
                    kept.push(file);
                }
                Err(failure) => {
                    changed |= failure.reason.ends_the_call();
                    failed.push((source.clone(), failure));
                }
            }
        }
        (kept, failed)
    }
}

/// Set once the caller of a collection went away.
struct Left(Arc<AtomicBool>);

impl Drop for Left {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// Write the receipt of the copies `kept` into `folder`, made at `at`: the
/// receipt, or none, and the folder removed, when nothing was kept.
fn write_receipt(
    task: Option<&Task>,
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
        task: task.map(|task| task.key().clone()),
        files: kept.to_vec(),
    };
    write_private(&folder.join("receipt.json"), &serde_json::to_vec(&written)?)?;
    Ok(Some(receipt))
}

/// Note the collection `receipt` in its task's record in `output`: a
/// warning, when it could not be.
fn note_output(
    task: Option<&Task>,
    output: &Path,
    receipt: Option<&ReceiptId>,
    at: u64,
    kept: &[Kept],
    total: u64,
) -> Option<&'static str> {
    let (receipt, task) = (receipt?, task?);
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
    noted
        .is_err()
        .then_some("Atlas could not note this collection in the task's record")
}

/// The reply to a call that asked for `asked` files: its receipt, the
/// copies kept, and, when any failed, which and why.
fn reply(
    asked: usize,
    receipt: Option<ReceiptId>,
    kept: Vec<Kept>,
    failed: Vec<(String, Failure)>,
    warning: Option<&str>,
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
    if let Some(warning) = warning {
        reply["warnings"] = json!([warning]);
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
    transport: &'a Transport,
    /// The runtime that drives the transport.
    runtime: &'a Handle,
    /// Whether the caller went away.
    left: &'a AtomicBool,
    /// The session every read is pinned to.
    pin: SessionPin,
    #[cfg(test)]
    fault: &'a Fault,
}

/// What the helper said of a file it sent whole: the version read, the
/// path it read, and that path relative to its home, when it is in it.
struct Received {
    identity: FileIdentity,
    real: String,
    in_home: Option<String>,
}

/// A read's final reply. A field absent or of another type is none here, and
/// each step decides what that means for it.
struct ReadReply {
    file: Option<Received>,
    length: Option<u64>,
    sha256: Option<String>,
}

impl One<'_> {
    fn fail(&self, reason: ReadRefusal) -> Failure {
        Failure::new(reason, self.source)
    }

    fn decoded(&self, reply: &Value) -> ReadReply {
        let file = &reply["file"];
        let identity: Option<FileIdentity> = serde_json::from_value(json!({
            "dev": file["dev"],
            "ino": file["ino"],
            "size": file["size"],
            "mtime": file["mtime"],
            "ctime": file["ctime"],
        }))
        .ok();
        ReadReply {
            file: identity.map(|identity| Received {
                identity,
                real: file["path"].as_str().unwrap_or(self.source).to_owned(),
                in_home: file["inHome"].as_str().map(str::to_owned),
            }),
            length: reply["length"].as_u64(),
            sha256: reply["sha256"].as_str().map(str::to_owned),
        }
    }

    /// Collect it, removing whatever was begun when it fails.
    fn collect(&self) -> std::result::Result<Kept, Failure> {
        let collected = || {
            let name = self.name()?;

            let started = now_ms();
            let mut partial = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&self.partial)
                .map_err(|_| self.fail(ReadRefusal::Space))?;
            let mut hasher = Sha256::new();
            let Received {
                identity,
                real,
                in_home,
            } = self.receive(&mut partial, &mut hasher)?;
            let sha256 = atlas_core::name::hex(&hasher.finalize());
            self.confirm(identity, &mut partial, &sha256)?;
            self.sync(&partial)
                .map_err(|_| self.fail(ReadRefusal::Space))?;
            drop(partial);
            self.staying()?;

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
        };
        let collected = collected();
        let _ = fs::remove_file(&self.partial);
        collected
    }

    /// The name its copy takes: the source's own, when it can be a file
    /// here and no file collected before it in this call has it.
    fn name(&self) -> std::result::Result<&str, Failure> {
        let name = Path::new(self.source)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| {
                !name.is_empty() && name.len() <= NAME_MAX && !name.chars().any(char::is_control)
            })
            .ok_or_else(|| self.fail(ReadRefusal::Name))?;
        if self
            .taken
            .iter()
            .any(|taken| taken.eq_ignore_ascii_case(name))
        {
            return Err(self.fail(ReadRefusal::Exists));
        }
        Ok(name)
    }

    /// Read the file part by part into `partial`, hashing each part, every
    /// part of the same version as the first: once the first says how large
    /// it is, it must fit what is left of the call and of the disk.
    fn receive(
        &self,
        partial: &mut File,
        hasher: &mut Sha256,
    ) -> std::result::Result<Received, Failure> {
        let mut first: Option<Received> = None;
        let mut offset = 0;
        loop {
            let mut request = json!({
                "op": "read",
                "path": self.source,
                "offset": offset,
                "length": READ_RANGE_MAX,
                "maxSize": self.max_size,
            });
            if let Some(first) = &first {
                request["expect"] = json!(first.identity);
            }
            request["session"] = json!(self.pin);
            let (reply, received) = self.read_part(&request, partial, hasher)?;
            let file = reply
                .file
                .ok_or_else(|| self.fail(ReadRefusal::Unreadable))?;
            if reply.length != Some(received) {
                return Err(self.fail(ReadRefusal::Changing));
            }
            let size = file.identity.size;
            if let Some(first) = &first {
                if first.identity != file.identity {
                    return Err(self.fail(ReadRefusal::Changing));
                }
            } else {
                if size > self.room {
                    return Err(self.fail(ReadRefusal::TooLarge));
                }
                if !room_for(self.copies, size) {
                    return Err(self.fail(ReadRefusal::Space));
                }
                first = Some(file);
            }
            offset += received;
            if offset >= size || received == 0 {
                return first
                    .filter(|first| offset == first.identity.size)
                    .ok_or_else(|| self.fail(ReadRefusal::Changing));
            }
        }
    }

    /// Check that the helper's digest of the version `identity` is
    /// `sha256`, the digest of the parts it sent.
    fn confirm(
        &self,
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
        request["session"] = json!(self.pin);
        let (reply, _) = self.read_part(&request, partial, &mut Sha256::new())?;
        if reply.sha256.as_deref() != Some(sha256) {
            return Err(self.fail(ReadRefusal::Changing));
        }
        Ok(())
    }

    /// Refused once the caller went away: nothing more is read or kept.
    fn staying(&self) -> std::result::Result<(), Failure> {
        if self.left.load(Ordering::SeqCst) {
            return Err(self.fail(ReadRefusal::Gone));
        }
        Ok(())
    }

    /// Append `bytes` to `partial`.
    fn append(&self, partial: &mut File, bytes: &[u8]) -> std::io::Result<()> {
        #[cfg(test)]
        (self.fault)(Step::Append)?;
        partial.write_all(bytes)
    }

    /// Make what was written to `partial` durable.
    fn sync(&self, partial: &File) -> std::io::Result<()> {
        #[cfg(test)]
        (self.fault)(Step::Sync)?;
        partial.sync_all()
    }

    /// Link the finished partial file into the copies folder as `name`,
    /// never over anything: the copy's path.
    fn link(&self, name: &str) -> std::result::Result<PathBuf, Failure> {
        let path = self.copies.join(name);
        match fs::hard_link(&self.partial, &path) {
            Ok(()) => Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(self.fail(ReadRefusal::Exists))
            }
            Err(_) => Err(self.fail(ReadRefusal::Space)),
        }
    }

    /// Send one read `request` and write the bytes of its `data` events to
    /// `partial` as they arrive, hashing them: the final reply, and how many
    /// bytes came. A refusal is the reason it gives.
    fn read_part(
        &self,
        request: &Value,
        partial: &mut File,
        hasher: &mut Sha256,
    ) -> std::result::Result<(ReadReply, u64), Failure> {
        self.staying()?;
        let mut buffer = Vec::new();
        let mut received = 0_u64;
        let mut written = Ok(());
        let reply = self.runtime.block_on(async {
            let pending = self.transport.connect().await?.dispatch(request).await?;
            pending
                .finish_each(&mut |event| {
                    if event["event"] != "data" {
                        return Ok(());
                    }
                    buffer.clear();
                    append_event(&event, &mut buffer, READ_RANGE_MAX as usize).map_err(bad_part)?;
                    received += buffer.len() as u64;
                    if received > READ_RANGE_MAX {
                        return Err(bad_part(OutputError::Oversized));
                    }
                    hasher.update(&buffer);
                    if let Err(error) = self.append(partial, &buffer) {
                        written = Err(error);
                        return Err(Error::new(Code::Io, "The copy could not be written"));
                    }
                    Ok(())
                })
                .await
        });
        if written.is_err() {
            return Err(self.fail(ReadRefusal::Space));
        }
        let reply = reply.map_err(|error| self.fail(helper_changed(error.code)))?;
        if reply["ok"] != true {
            let reason =
                ReadRefusal::of(&reply).unwrap_or_else(|| helper_changed(refusal(&reply).code));
            return Err(self.fail(reason));
        }
        Ok((self.decoded(&reply), received))
    }
}

/// The protocol failure of a part that could not be taken.
fn bad_part(error: OutputError) -> Error {
    Error::new(
        Code::Protocol,
        match error {
            OutputError::Malformed => "A part of the file came back malformed",
            OutputError::Oversized => "A part of the file came back too large",
        },
    )
}

/// The reason a read that failed with `code` was not served: the helper
/// itself changed, which ends the call, or the file could not be read.
fn helper_changed(code: Code) -> ReadRefusal {
    match code {
        Code::SessionChanged => ReadRefusal::SessionChanged,
        Code::OnConsole => ReadRefusal::OnConsole,
        _ => ReadRefusal::Unreadable,
    }
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
#[path = "collect_tests.rs"]
mod tests;
