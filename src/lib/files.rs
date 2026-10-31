//! Atlas's own files: each written private to this account, read only
//! when it is a plain file (and this account's, where that matters), and
//! named by random tokens no person chose.
use crate::{
    error::{Code, Error, Result},
    name::{lower_hex, name_type, random_hex},
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
    time::{Duration, Instant},
};

/// Random bytes in a [`token`]; it is twice as many hexadecimal digits.
pub const TOKEN_BYTES: usize = 24;

/// A new random name: [`TOKEN_BYTES`] random bytes as lowercase
/// hexadecimal digits.
pub fn token() -> String {
    random_hex(TOKEN_BYTES * 2)
}

name_type! {
    /// A name shaped as a [`token`] makes it: Atlas's own random name for a
    /// copy folder or an environment plan, never one a person chose.
    #[derive(Hash, PartialOrd, Ord)]
    pub struct Token;
    refused(text) = format!("{text} is not one of Atlas's tokens");
    display
}

impl Token {
    /// `text` as a token, when it is [`TOKEN_BYTES`] bytes as lowercase
    /// hexadecimal digits.
    pub fn parse(text: &str) -> Option<Self> {
        lower_hex(text, TOKEN_BYTES * 2).then(|| Self(text.to_owned()))
    }

    /// A new random token.
    pub fn new() -> Self {
        Self(token())
    }
}

impl Default for Token {
    fn default() -> Self {
        Self::new()
    }
}

/// How far a private file is written before the call returns.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Durability {
    /// To the disk itself: the file outlives a power cut.
    Disk,
    /// To the kernel, ordered before whatever is written next: the file
    /// outlives the process that wrote it, and a power cut leaves it whole
    /// or absent, never half written. On macOS a write to the disk itself
    /// takes milliseconds, too slow for a record kept up to date between
    /// keystrokes.
    Kernel,
}

/// Write `bytes` as a file only this account can read, all the way to the
/// disk.
pub fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    write_private_with(path, bytes, Durability::Disk)
}

/// Write `bytes` as a file only this account can read, as far as
/// `durability` asks.
pub fn write_private_with(path: &Path, bytes: &[u8], durability: Durability) -> Result<()> {
    // Emptied only once it is known to be ours.
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| Error::writing(path, error))?;
    own_private(&file, path)?;
    file.set_len(0)?;
    file.write_all(bytes)?;

    match durability {
        Durability::Disk => file.sync_all()?,
        Durability::Kernel => {
            nix::fcntl::fcntl(&file, nix::fcntl::FcntlArg::F_BARRIERFSYNC)
                .map_err(std::io::Error::from)?;
        }
    }

    Ok(())
}

/// Whether a new private file can be made in `directory` right now: one is
/// created under a name of its own and removed again. Reopening a file that
/// is already there would prove nothing about the next one.
pub fn probe_write(directory: &Path) -> Result<()> {
    fs::create_dir_all(directory).map_err(|error| Error::writing(directory, error))?;

    // A probe an interrupted run left behind is litter; one of a process
    // still running is its own, about to be removed.
    if let Ok(entries) = fs::read_dir(directory) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Some(rest) = name.strip_prefix(".probe-") else {
                continue;
            };

            let alive = rest
                .split('-')
                .next()
                .and_then(|pid| pid.parse::<i32>().ok())
                .is_some_and(|pid| {
                    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok()
                });
            if !alive {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    let moment = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let path = directory.join(format!(".probe-{}-{moment}", std::process::id()));
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .map_err(|error| Error::writing(&path, error))?;

    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        // Another process's cleanup may have taken it: it was written.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Error::writing(&path, error)),
    }
}

/// The lock file at `path`, made private to this account when it is new,
/// and refused unless it is a plain file of this account.
pub(crate) fn open_lock(path: &Path) -> Result<LockFile> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| Error::writing(path, error))?;
    own_private(&file, path)?;

    Ok(LockFile(file))
}

/// Unlocks on drop even while a forked child still has the open file description.
#[derive(Debug)]
pub(crate) struct LockFile(pub File);

impl Drop for LockFile {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.0);
    }
}

/// How a lock is held: beside other readers, or by one holder alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lock {
    Shared,
    Exclusive,
}

/// Another holder kept a lock past the patience given for it.
#[derive(Debug)]
pub(crate) struct Held;

/// How often a lock that looks held is tried again.
const LOCK_POLL: Duration = Duration::from_millis(5);

/// Take `lock` on `file`, trying again until `patience` has passed; none at
/// all tries once.
pub(crate) fn lock_within(
    file: &LockFile,
    lock: Lock,
    patience: Duration,
) -> std::result::Result<(), Held> {
    lock_retrying(file, lock, patience, |deadline| Instant::now() < deadline)
}

/// [`lock_within`], trying again only while `retry` says to, given the
/// moment `patience` runs out.
pub(crate) fn lock_retrying(
    file: &LockFile,
    lock: Lock,
    patience: Duration,
    mut retry: impl FnMut(Instant) -> bool,
) -> std::result::Result<(), Held> {
    let deadline = Instant::now() + patience;
    loop {
        let taken = match lock {
            Lock::Shared => fs2::FileExt::try_lock_shared(&file.0),
            Lock::Exclusive => fs2::FileExt::try_lock_exclusive(&file.0),
        };
        if taken.is_ok() {
            return Ok(());
        }
        if !retry(deadline) {
            return Err(Held);
        }
        std::thread::sleep(LOCK_POLL);
    }
}

/// Refuse a file that is not a plain file of this account; one that others
/// may touch (made with a wider mode before) is tightened to 0600.
fn own_private(file: &File, path: &Path) -> Result<()> {
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.uid() != nix::unistd::getuid().as_raw() {
        return Err(Error::new(
            Code::State,
            format!("{} is not a plain file of this account", path.display()),
        ));
    }

    if metadata.mode() & 0o077 != 0 {
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }

    Ok(())
}

/// `path` opened to read, when it is a plain file and not a link. The check
/// is made on the opened descriptor, so the file read is the file checked,
/// and it never waits: a pipe in a screenshot's or the log's place does not
/// hold the reader up.
pub fn open_plain(path: &Path) -> Option<File> {
    try_open_plain(path).ok().flatten()
}

/// Like [`open_plain`], preserving I/O failures and refused files. Only a
/// missing path has no contents.
pub(crate) fn try_open_plain(path: &Path) -> std::io::Result<Option<File>> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "The path is not a plain file",
        ));
    }
    Ok(Some(file))
}

/// What the plain file at `path` holds, when it is this account's and at
/// most `limit` bytes: no link, pipe or other account's file is read.
pub(crate) fn read_own(path: &Path, limit: u64) -> Option<Vec<u8>> {
    let file = open_plain(path)?;
    if file.metadata().ok()?.uid() != nix::unistd::getuid().as_raw() {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= limit).then_some(bytes)
}

/// Whether `folder` is a real folder of this account, not a link to one.
pub(crate) fn own_folder(folder: &Path) -> bool {
    fs::symlink_metadata(folder).is_ok_and(|metadata| {
        metadata.is_dir()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == nix::unistd::getuid().as_raw()
    })
}

#[cfg(test)]
mod tests {
    use super::{Held, Lock, lock_within, open_lock};
    use std::time::{Duration, Instant};

    #[test]
    #[allow(
        unsafe_code,
        reason = "The isolated fork child only calls async-signal-safe syscalls"
    )]
    fn a_lock_is_released_even_when_a_forked_child_inherits_it() {
        use nix::{
            sys::{
                signal::{Signal, kill},
                wait::{WaitPidFlag, WaitStatus, waitpid},
            },
            unistd::{ForkResult, alarm, fork, pause},
        };

        if !crate::test_support::isolated(
            "files::tests::a_lock_is_released_even_when_a_forked_child_inherits_it",
        ) {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("lock");
        let held = open_lock(&path).unwrap();
        lock_within(&held, Lock::Exclusive, Duration::ZERO).unwrap();
        let next = open_lock(&path).unwrap();
        // SAFETY: The child only calls async-signal-safe alarm and pause;
        // it expires after five seconds even if the parent's cleanup fails.
        let child = match unsafe { fork() }.unwrap() {
            ForkResult::Parent { child } => child,
            ForkResult::Child => {
                alarm::set(5);
                loop {
                    pause();
                }
            }
        };

        let refused = lock_within(&next, Lock::Exclusive, Duration::ZERO);
        drop(held);
        let released = lock_within(&next, Lock::Exclusive, Duration::ZERO);

        let killed = kill(child, Signal::SIGKILL);
        let deadline = Instant::now() + Duration::from_secs(2);
        let reaped = loop {
            match waitpid(child, Some(WaitPidFlag::WNOHANG)) {
                Ok(WaitStatus::StillAlive) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                status => break status,
            }
        };
        killed.unwrap();
        assert_eq!(
            reaped.unwrap(),
            WaitStatus::Signaled(child, Signal::SIGKILL, false)
        );
        assert!(matches!(refused, Err(Held)));
        assert!(
            released.is_ok(),
            "the inherited descriptor kept the lock: {released:?}"
        );
    }

    #[test]
    fn a_held_lock_is_refused_once_its_patience_runs_out() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lock");
        let (first, second) = (open_lock(&path).unwrap(), open_lock(&path).unwrap());

        lock_within(&first, Lock::Shared, Duration::ZERO).unwrap();
        lock_within(&second, Lock::Shared, Duration::ZERO).expect("readers share it");
        fs2::FileExt::unlock(&second.0).unwrap();

        let started = Instant::now();
        let refused = lock_within(&second, Lock::Exclusive, Duration::from_millis(30));
        assert!(matches!(refused, Err(Held)));
        assert!(started.elapsed() >= Duration::from_millis(30), "it waited");

        fs2::FileExt::unlock(&first.0).unwrap();
        lock_within(&second, Lock::Exclusive, Duration::ZERO)
            .expect("a free lock is taken at once");
    }
}
