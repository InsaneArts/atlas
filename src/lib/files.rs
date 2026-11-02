//! Atlas's own files: each written private to this account, read only
//! when it is a plain file (and this account's, where that matters), and
//! named by random tokens no person chose.
use crate::{
    context::context,
    error::{Code, Error, Result},
    name::{lower_hex, name_type, random_hex},
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
    time::{Duration, Instant},
};

pub use crate::fs_move::exchange;

/// Random bytes in a [`token`]; it is twice as many hexadecimal digits.
pub(crate) const TOKEN_BYTES: usize = 24;

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
pub(crate) enum Durability {
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
///
/// # Errors
///
/// Returns path, ownership, write or synchronization failures. The owned file is truncated
/// before writing, so an I/O failure can leave partial contents; this is not atomic replacement.
pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    write_private_with(path, bytes, Durability::Disk)
}

/// Write `bytes` as a file only this account can read, as far as
/// `durability` asks.
///
/// # Errors
///
/// Returns path, ownership, write or synchronization failures for the requested durability.
/// Writing starts only after ownership checks, but a later failure can leave partial contents.
pub(crate) fn write_private_with(path: &Path, bytes: &[u8], durability: Durability) -> Result<()> {
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

/// Write a fresh file beside `path` (created with `mode`, never through a
/// link), flush it to disk, then put it in place with `place`, which is
/// given the temporary path. The temporary file never outlives the call.
pub(crate) fn write_beside(
    path: &Path,
    mode: u32,
    write: impl FnOnce(&mut fs::File) -> Result<()>,
    place: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(Error::new(
            Code::Config,
            format!("{} is not a file path", path.display()),
        ));
    };

    let temporary = parent.join(format!(".{}.{}", name.to_string_lossy(), token()));

    let result = (|| -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(mode)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temporary)
            .map_err(context("Create replacement for", path))?;
        write(&mut file).map_err(context("Write replacement for", path))?;
        file.sync_all()
            .map_err(context("Sync replacement for", path))?;

        place(&temporary)
    })();
    let _ = fs::remove_file(&temporary);

    result
}

/// Replace `path` with `bytes` in one rename, so readers see the old file
/// or the new one, never part of either. The file gets exactly `mode`:
/// the umask would otherwise narrow it, and a file shared with the helper
/// (such as the onboarding request) must stay readable there.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    write_beside(
        path,
        mode,
        |file| {
            file.set_permissions(fs::Permissions::from_mode(mode))?;
            Ok(file.write_all(bytes)?)
        },
        |temporary| fs::rename(temporary, path).map_err(context("Place replacement at", path)),
    )
}

/// [`write_atomic`] for a file only this account may read, making its
/// missing folders private (0700) first.
pub(crate) fn save_private(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .map_err(context("Create private directory", parent))?;
    }

    write_atomic(path, bytes, 0o600)
}

/// Whether a new private file can be made in `directory` right now: one is
/// created under a name of its own and removed again. Reopening a file that
/// is already there would prove nothing about the next one.
///
/// # Errors
///
/// Returns private-storage or temporary-file write failures. This proves a local write at
/// the time of the probe, not later mutation authority.
pub(crate) fn probe_write(directory: &Path) -> Result<()> {
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

    let moment = crate::time::timestamp();
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

/// How a lock file's descriptor is opened.
#[derive(Clone, Copy)]
pub(crate) enum LockOpen {
    ReadWrite,
    WriteOnly,
    WriteNonblocking,
}

/// The lock file at `path`, made private when new and never through a
/// symlink. Checking it and taking its lock stay with each caller.
pub(crate) fn open_lock_file(path: &Path, access: LockOpen) -> std::io::Result<File> {
    let (read, flags) = match access {
        LockOpen::ReadWrite => (true, libc::O_NOFOLLOW),
        LockOpen::WriteOnly => (false, libc::O_NOFOLLOW),
        LockOpen::WriteNonblocking => (false, libc::O_NOFOLLOW | libc::O_NONBLOCK),
    };
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(read)
        .write(true)
        .mode(0o600)
        .custom_flags(flags)
        .open(path)
}

/// The lock file at `path`, made private to this account when it is new,
/// and refused unless it is a plain file of this account.
pub(crate) fn open_lock(path: &Path) -> Result<LockFile> {
    let file =
        open_lock_file(path, LockOpen::ReadWrite).map_err(|error| Error::writing(path, error))?;
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

/// Another holder kept a lock past the patience given for it, with what
/// the last try said.
#[derive(Debug)]
pub(crate) struct Held(pub(crate) std::io::Error);

/// How often a lock that looks held is tried again.
const LOCK_POLL: Duration = Duration::from_millis(5);

#[cfg(test)]
std::thread_local! {
    static NEXT_LOCK_CONTENTION: std::cell::RefCell<Option<std::sync::mpsc::Sender<()>>> =
        const { std::cell::RefCell::new(None) };
}

/// Tell a fixture once this thread's next lock attempt actually would block.
#[cfg(test)]
pub(crate) fn on_next_lock_contention(notice: std::sync::mpsc::Sender<()>) {
    NEXT_LOCK_CONTENTION.with(|next| {
        assert!(
            next.borrow_mut().replace(notice).is_none(),
            "the fixture already waits for lock contention"
        );
    });
}

/// Take `lock` on `file`, trying again until `patience` has passed; none at
/// all tries once.
pub(crate) fn lock_within(
    file: &LockFile,
    lock: Lock,
    patience: Duration,
) -> std::result::Result<(), Held> {
    lock_retrying(file, lock, Instant::now() + patience, |deadline| {
        Instant::now() < deadline
    })
}

/// [`lock_within`], trying again only while `retry` says to, given the
/// absolute deadline. Even an expired deadline still permits the first probe.
pub(crate) fn lock_retrying(
    file: &LockFile,
    lock: Lock,
    deadline: Instant,
    mut retry: impl FnMut(Instant) -> bool,
) -> std::result::Result<(), Held> {
    loop {
        let taken = match lock {
            Lock::Shared => fs2::FileExt::try_lock_shared(&file.0),
            Lock::Exclusive => fs2::FileExt::try_lock_exclusive(&file.0),
        };
        let Err(error) = taken else {
            return Ok(());
        };
        #[cfg(test)]
        if error.kind() == std::io::ErrorKind::WouldBlock {
            NEXT_LOCK_CONTENTION.with(|next| {
                if let Some(notice) = next.borrow_mut().take() {
                    // The fixture receiver may be gone after its outer watchdog fails.
                    let _ = notice.send(());
                }
            });
        }
        if !retry(deadline) {
            return Err(Held(error));
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
pub(crate) fn open_plain(path: &Path) -> Option<File> {
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

/// What `reader` gives, up to exactly `count` bytes. Whether there was more,
/// and what that means, is the caller's.
pub(crate) fn read_up_to(reader: impl Read, count: u64) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(count).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// All `reader` gives, when it reads and is at most `limit` bytes.
pub(crate) fn read_limited(reader: impl Read, limit: u64) -> Option<Vec<u8>> {
    let bytes = read_up_to(reader, limit + 1).ok()?;
    (bytes.len() as u64 <= limit).then_some(bytes)
}

/// What the plain file at `path` holds, when it is this account's and at
/// most `limit` bytes: no link, pipe or other account's file is read.
pub(crate) fn read_own(path: &Path, limit: u64) -> Option<Vec<u8>> {
    let file = open_plain(path)?;
    if file.metadata().ok()?.uid() != nix::unistd::getuid().as_raw() {
        return None;
    }
    read_limited(file, limit)
}

/// `folder`, made private (0700, with any missing parents) when missing. One
/// that exists is never re-permissioned: it must already be a private folder
/// this account owns, and otherwise is refused with `code`.
pub(crate) fn private_folder(folder: &Path, code: Code) -> Result<()> {
    if !folder.exists() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(folder)
            .map_err(|error| Error::writing(folder, error))?;
    }

    let metadata = fs::symlink_metadata(folder).map_err(context("Inspect folder", folder))?;
    if !metadata.is_dir()
        || metadata.uid() != nix::unistd::getuid().as_raw()
        || metadata.mode() & 0o077 != 0
    {
        return Err(Error::new(
            code,
            format!(
                "{} must be a private (0700) folder you own",
                folder.display()
            ),
        ));
    }

    Ok(())
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
mod tests;
