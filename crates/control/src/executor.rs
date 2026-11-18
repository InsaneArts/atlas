//! The executor record kept beside a claim: a privileged executor's PID,
//! boot and start time, published before it acts, so recovery waits while
//! that process can still act after its caller is gone, and never for a
//! process that reused its PID.
use super::{Pending, marker::Liveness};
pub(super) use atlas_core::process::{boot as executor_boot, start as executor_start};
use atlas_core::{
    context::context,
    error::{Code, Error, Result},
    files::token,
    process::Start,
};
use std::{
    cell::RefCell,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

thread_local! {
    /// Claims for the synchronous privileged dispatch on this thread.
    static EXECUTOR_CLAIMS: RefCell<Vec<(PathBuf, File)>> = const { RefCell::new(Vec::new()) };
}

/// Bind durable claims to every executor dispatched in this scope. Before
/// acknowledging an executor, its PID, boot and start time are synced into
/// a record alongside each claim. If the caller dies, recovery refuses while
/// that process instance can act.
pub struct ExecutorClaims(Vec<(PathBuf, File)>);

impl ExecutorClaims {
    /// # Errors
    ///
    /// Returns the failure to keep a claim's file open for the executor.
    pub fn new(claims: &[Pending]) -> Result<Self> {
        let files = claims
            .iter()
            .map(|claim| {
                claim
                    .lock
                    .0
                    .try_clone()
                    .map(|file| (claim.live.clone(), file))
                    .map_err(context("Clone executor claim", &claim.live))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self(EXECUTOR_CLAIMS.with(|bound| bound.replace(files))))
    }
}

impl Drop for ExecutorClaims {
    fn drop(&mut self) {
        EXECUTOR_CLAIMS.with(|bound| bound.replace(std::mem::take(&mut self.0)));
    }
}

/// Persist the executor before allowing it past its initial handshake.
///
/// # Errors
///
/// Returns the failure to record the executor, which must not go on then.
pub fn record_executor(pid: nix::unistd::Pid) -> Result<()> {
    record_executor_with(pid, executor_boot, executor_start)
}

pub(super) fn record_executor_with(
    pid: nix::unistd::Pid,
    boot: impl FnOnce() -> Result<String>,
    start: impl FnOnce(i32) -> Result<Start>,
) -> Result<()> {
    EXECUTOR_CLAIMS.with(|claims| {
        let claims = claims.borrow();
        if claims.is_empty() {
            return Ok(());
        }
        let executor = Executor {
            pid: pid.as_raw(),
            boot: boot()?,
            start: start(pid.as_raw())?,
        };
        let record = serde_json::to_vec(&executor)?;
        for (live, file) in claims.iter() {
            publish_executor_with(live, &record, |file, record| file.write_all(record))?;
            let mut file = file;
            file.set_len(0)
                .and_then(|()| file.seek(SeekFrom::Start(0)))
                .and_then(|_| file.write_all(&record))
                .and_then(|()| file.sync_all())
                .map_err(context("Persist executor claim", live))?;
        }
        Ok(())
    })
}

/// The extension an executor record is written under before it is renamed
/// over the claim's executor record; a random token follows it.
pub(super) const EXECUTOR_WRITING: &str = "executor-";

/// Publish identity separately from the stable claim inode: a failed write
/// cannot destroy the previous executor's identity or release its flock.
/// Only the holder of the claim writes it, so a record still being written
/// is never swept while its claim is held.
pub(super) fn publish_executor_with(
    live: &Path,
    record: &[u8],
    write: impl FnOnce(&mut File, &[u8]) -> std::io::Result<()>,
) -> Result<()> {
    let Some(directory) = live.parent() else {
        return Err(Error::new(Code::State, "A claim needs a folder"));
    };
    let path = live.with_extension("executor.live");
    let temporary = live.with_extension(format!("{EXECUTOR_WRITING}{}", token()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temporary)
        .map_err(context("Create executor record for", live))?;
    let result = (|| {
        write(&mut file, record)?;
        file.sync_all()?;
        fs::rename(&temporary, &path)?;
        File::open(directory)?.sync_all()?;
        Ok(())
    })();
    let _ = fs::remove_file(temporary);
    result.map_err(|error: Error| context("Publish executor record", &path)(error))
}

pub(super) fn claim_executor_running(file: &File, live: &Path) -> bool {
    claim_executor(file, live) != Liveness::Gone
}

/// Whether an executor still carries out the claim at `live`, whose file
/// is `file`, as its record says: [`Liveness::Unknown`] whenever the record
/// or the process it names cannot be read.
pub(super) fn claim_executor(file: &File, live: &Path) -> Liveness {
    match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(live.with_extension("executor.live"))
    {
        Ok(record) => executor_liveness(&record),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => executor_liveness(file),
        Err(_) => Liveness::Unknown,
    }
}

/// The executor record: which process instance, on which boot, carries out
/// the claimed change.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Executor {
    pub(super) pid: i32,
    pub(super) boot: String,
    pub(super) start: Start,
}

/// A different boot or process instance cannot be the dispatched executor.
/// Unreadable identities and records remain conservative.
#[cfg(test)]
pub(super) fn executor_running(file: &File) -> bool {
    executor_liveness(file) != Liveness::Gone
}

fn executor_liveness(file: &File) -> Liveness {
    executor_liveness_with(file, executor_boot, seen)
}

/// [`executor_running`], with how the boot and a process's start are read:
/// a process whose start is read is taken as not exited.
#[cfg(test)]
pub(super) fn executor_running_with(
    file: &File,
    boot: impl FnOnce() -> Result<String>,
    start: impl FnOnce(i32) -> Result<Start>,
) -> bool {
    let seen = |pid| {
        start(pid).map(|start| {
            Some(Seen {
                start,
                exited: false,
            })
        })
    };
    executor_liveness_with(file, boot, seen) != Liveness::Gone
}

/// What the kernel says now of a process an executor record names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Seen {
    pub(super) start: Start,
    /// It has exited and waits to be reaped: it acts no more.
    pub(super) exited: bool,
}

/// Process `pid` as the kernel says it now; none when there is no such
/// process.
fn seen(pid: i32) -> Result<Option<Seen>> {
    Ok(atlas_core::process::info(pid)?.map(|info| Seen {
        start: info.instance.start,
        exited: info.zombie,
    }))
}

/// Whether the executor the record `file` names still runs: live only when
/// its process is read, is that very instance, on this boot, and has not
/// exited; gone when it names none, or its process is gone, exited or
/// another; unknown when the record or the process cannot be read.
pub(super) fn executor_liveness_with(
    mut file: &File,
    boot: impl FnOnce() -> Result<String>,
    seen: impl FnOnce(i32) -> Result<Option<Seen>>,
) -> Liveness {
    let mut text = String::new();
    if file
        .seek(SeekFrom::Start(0))
        .and_then(|_| file.read_to_string(&mut text))
        .is_err()
    {
        return Liveness::Unknown;
    }
    if text.is_empty() {
        return Liveness::Gone;
    }
    let Some(executor) = serde_json::from_str::<Executor>(&text)
        .ok()
        .filter(|executor| executor.pid > 0)
    else {
        return Liveness::Unknown;
    };
    // Only ESRCH proves absence here; permission errors and unavailable
    // identities still retain the claim.
    if nix::sys::signal::kill(nix::unistd::Pid::from_raw(executor.pid), None)
        == Err(nix::errno::Errno::ESRCH)
    {
        return Liveness::Gone;
    }
    let (Ok(boot), Ok(seen)) = (boot(), seen(executor.pid)) else {
        return Liveness::Unknown;
    };
    // An exited process still answers a signal and keeps its start time
    // until it is reaped, so its state decides, not its identity alone.
    match seen {
        Some(seen) if boot == executor.boot && seen.start == executor.start && !seen.exited => {
            Liveness::Live
        }
        Some(_) | None => Liveness::Gone,
    }
}
