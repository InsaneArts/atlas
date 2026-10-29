//! The executor record kept beside a claim: a privileged executor's PID,
//! boot and start time, published before it acts, so recovery waits while
//! that process can still act after its caller is gone, and never for a
//! process that reused its PID.
use super::Pending;
use crate::{
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
pub(crate) struct ExecutorClaims(Vec<(PathBuf, File)>);

impl ExecutorClaims {
    pub(crate) fn new(claims: &[Pending]) -> Result<Self> {
        let files = claims
            .iter()
            .map(|claim| {
                claim
                    .lock
                    .try_clone()
                    .map(|file| (claim.live.clone(), file))
            })
            .collect::<std::io::Result<Vec<_>>>()?;
        Ok(Self(EXECUTOR_CLAIMS.with(|bound| bound.replace(files))))
    }
}

impl Drop for ExecutorClaims {
    fn drop(&mut self) {
        EXECUTOR_CLAIMS.with(|bound| bound.replace(std::mem::take(&mut self.0)));
    }
}

/// Persist the executor before allowing it past its initial handshake.
pub(crate) fn record_executor(pid: nix::unistd::Pid) -> Result<()> {
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
            file.set_len(0)?;
            file.seek(SeekFrom::Start(0))?;
            file.write_all(&record)?;
            file.sync_all()?;
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
        .open(&temporary)?;
    let result = (|| {
        write(&mut file, record)?;
        file.sync_all()?;
        fs::rename(&temporary, &path)?;
        File::open(directory)?.sync_all()?;
        Ok(())
    })();
    let _ = fs::remove_file(temporary);
    result
}

pub(super) fn claim_executor_running(file: &File, live: &Path) -> bool {
    match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(live.with_extension("executor.live"))
    {
        Ok(record) => executor_running(&record),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => executor_running(file),
        Err(_) => true,
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

/// The immutable identity of this boot, unaffected by wall-clock changes.
pub(super) fn executor_boot() -> Result<String> {
    crate::process::boot()
}

#[cfg(test)]
pub(super) fn executor_boot_with(
    read: impl FnOnce(&std::ffi::CStr) -> Result<String>,
) -> Result<String> {
    read(c"kern.bootsessionuuid")
}

/// The exact start time of the executor, whichever account it runs as:
/// `proc_pidinfo` refuses another user's process, the root executor
/// included, and the kernel's process sysctl does not.
pub(super) fn executor_start(pid: i32) -> Result<Start> {
    crate::process::start(pid)
}

/// A different boot or process instance cannot be the dispatched executor.
/// Unreadable identities and legacy PID-only records remain conservative.
pub(super) fn executor_running(file: &File) -> bool {
    executor_running_with(file, executor_boot, executor_start)
}

pub(super) fn executor_running_with(
    mut file: &File,
    boot: impl FnOnce() -> Result<String>,
    start: impl FnOnce(i32) -> Result<Start>,
) -> bool {
    let mut text = String::new();
    if file
        .seek(SeekFrom::Start(0))
        .and_then(|_| file.read_to_string(&mut text))
        .is_err()
    {
        return true;
    }
    if text.is_empty() {
        return false;
    }
    let executor = serde_json::from_str::<Executor>(&text)
        .ok()
        .filter(|executor| executor.pid > 0);
    let pid = match &executor {
        Some(executor) => executor.pid,
        None => match text.trim().parse::<i32>().ok().filter(|pid| *pid > 0) {
            Some(pid) => pid,
            None => return true,
        },
    };
    // An exited process has no start time to read. Only ESRCH proves absence;
    // permission errors and unavailable identities still retain the claim.
    if nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None)
        == Err(nix::errno::Errno::ESRCH)
    {
        return false;
    }
    if let Some(executor) = executor {
        let (Ok(boot), Ok(start)) = (boot(), start(pid)) else {
            return true;
        };
        if boot != executor.boot || start != executor.start {
            return false;
        }
    }
    true
}
