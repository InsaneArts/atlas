//! Who may act on the helper, and what is still unsettled: one controller at
//! a time ([`Controller`], a file lock per helper account), a marker of every
//! mutation whose outcome is unknown until verified recovery clears it, and
//! the short-lived [`Lease`] a viewer holds while a person has control.
use crate::{
    config::home,
    error::{Error, Result},
};
use fs2::FileExt;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

/// Random bytes in a [`token`]; it is twice as many hexadecimal digits.
pub const TOKEN_BYTES: usize = 24;

pub fn token() -> String {
    let mut bytes = [0; TOKEN_BYTES];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
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

fn open_lock(path: &Path) -> Result<File> {
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

    Ok(file)
}

/// Refuse a file that is not a plain file of this account; one that others
/// may touch (made with a wider mode before) is tightened to 0600.
fn own_private(file: &File, path: &Path) -> Result<()> {
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.uid() != nix::unistd::getuid().as_raw() {
        return Err(Error::new(
            "state",
            format!("{} is not a plain file of this account", path.display()),
        ));
    }

    if metadata.mode() & 0o077 != 0 {
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }

    Ok(())
}

/// A mutation still being carried out. For its lifetime it holds an
/// exclusive lock on a file named for its generation, so any process can tell
/// work in progress from an outcome nothing will settle. Its marker entry
/// outlives it until the outcome is acknowledged.
#[must_use = "dropping it at once reports the mutation as abandoned"]
#[derive(Debug)]
pub struct Pending {
    generation: String,
    live: PathBuf,
    /// The claim file, exclusively locked for as long as it is open.
    lock: File,
}

impl std::ops::Deref for Pending {
    type Target = str;
    fn deref(&self) -> &str {
        self.generation()
    }
}

impl Pending {
    /// The generation this mutation is recorded under.
    pub fn generation(&self) -> &str {
        &self.generation
    }

    /// Whether the claim path still names the file this claim locked.
    fn names_own_file(&self) -> bool {
        let (named, held) = (fs::symlink_metadata(&self.live), self.lock.metadata());
        matches!((named, held), (Ok(a), Ok(b)) if (a.dev(), a.ino()) == (b.dev(), b.ino()))
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        // Only an unlocked claim file is ever removed by others, and a
        // replacement can only appear once this one is gone, so a path that
        // still names the locked file stays ours until it is removed here.
        // The lock goes with the descriptor.
        if self.names_own_file() && !claim_executor_running(&self.lock, &self.live) {
            remove_claim(&self.live);
        }
    }
}

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

fn record_executor_with(
    pid: nix::unistd::Pid,
    boot: impl FnOnce() -> Result<String>,
    start: impl FnOnce(i32) -> Result<[u64; 2]>,
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

/// Publish identity separately from the stable claim inode: a failed write
/// cannot destroy the previous executor's identity or release its flock.
fn publish_executor_with(
    live: &Path,
    record: &[u8],
    write: impl FnOnce(&mut File, &[u8]) -> std::io::Result<()>,
) -> Result<()> {
    let path = live.with_extension("executor.live");
    let temporary = live.with_extension(format!("executor-{}", token()));
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
        File::open(path.parent().expect("claim directory"))?.sync_all()?;
        Ok(())
    })();
    let _ = fs::remove_file(temporary);
    result
}

fn claim_executor_running(file: &File, live: &Path) -> bool {
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

fn remove_claim(live: &Path) {
    match fs::remove_file(live) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return,
        _ => {}
    }
    #[cfg(test)]
    tests::INTERRUPT_CLAIM_REMOVAL.with(|interrupt| {
        assert!(!interrupt.replace(false), "interrupted cleanup");
    });
    let _ = fs::remove_file(live.with_extension("executor.live"));
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Executor {
    pid: i32,
    boot: String,
    start: [u64; 2],
}

/// The immutable identity of this boot, unaffected by wall-clock changes.
fn executor_boot() -> Result<String> {
    executor_boot_with(executor_sysctl)
}

fn executor_boot_with(read: impl FnOnce(&std::ffi::CStr) -> Result<String>) -> Result<String> {
    read(c"kern.bootsessionuuid")
}

#[allow(unsafe_code, reason = "FFI: kern.bootsessionuuid has no safe binding")]
fn executor_sysctl(name: &std::ffi::CStr) -> Result<String> {
    let mut boot = [0_u8; 37];
    let mut size = std::mem::size_of_val(&boot);
    // SAFETY: NUL-terminated name and writable buffer of the declared size; no input buffer.
    let status = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            boot.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 || size != std::mem::size_of_val(&boot) {
        return Err(std::io::Error::last_os_error().into());
    }
    std::ffi::CStr::from_bytes_with_nul(&boot)
        .ok()
        .and_then(|boot| boot.to_str().ok())
        .map(str::to_owned)
        .ok_or_else(|| Error::new("state", "Invalid boot session UUID"))
}

#[allow(unsafe_code, reason = "FFI: KERN_PROC_PID has no safe binding")]
fn executor_start(pid: i32) -> Result<[u64; 2]> {
    // PROC_PIDTBSDINFO refuses another user's process, including the root
    // executor. KERN_PROC_PID exposes the same exact start time across users.
    let mut name = [libc::CTL_KERN, libc::KERN_PROC, libc::KERN_PROC_PID, pid];
    let mut size = 0;
    // SAFETY: the four-element MIB is valid; a null output asks only its size.
    let status = unsafe {
        libc::sysctl(
            name.as_mut_ptr(),
            name.len() as u32,
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let invalid = || Error::new("state", "The executor's start time is unavailable");
    if size < std::mem::size_of::<libc::timeval>() {
        return Err(invalid());
    }
    let mut info = vec![0_u8; size];
    // SAFETY: the MIB is valid and info has the declared writable capacity.
    let status = unsafe {
        libc::sysctl(
            name.as_mut_ptr(),
            name.len() as u32,
            info.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    if size < std::mem::size_of::<libc::timeval>() || size > info.len() {
        return Err(invalid());
    }
    // SAFETY: sys/sysctl.h's kinfo_proc starts with kp_proc; sys/proc.h's
    // extern_proc starts with p_starttime, a timeval. The returned prefix is
    // long enough, and read_unaligned does not assume the byte buffer's alignment.
    let start = unsafe { info.as_ptr().cast::<libc::timeval>().read_unaligned() };
    if start.tv_sec <= 0 || !(0..1_000_000).contains(&start.tv_usec) {
        return Err(invalid());
    }
    Ok([start.tv_sec as u64, start.tv_usec as u64])
}

/// A different boot or process instance cannot be the dispatched executor.
/// Unreadable identities and legacy PID-only records remain conservative.
fn executor_running(file: &File) -> bool {
    executor_running_with(file, executor_boot, executor_start)
}

fn executor_running_with(
    mut file: &File,
    boot: impl FnOnce() -> Result<String>,
    start: impl FnOnce(i32) -> Result<[u64; 2]>,
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

/// Exclusive control of one helper, held until dropped.
#[derive(Debug)]
pub struct Ownership {
    _lock: File,
    /// The note that the person holds this ownership through a viewer, gone
    /// with it.
    person: Option<PathBuf>,
}

impl Drop for Ownership {
    fn drop(&mut self) {
        if let Some(note) = &self.person {
            let _ = fs::remove_file(note);
        }
        let _ = FileExt::unlock(&self._lock);
    }
}

/// Controller exclusion and pending-mutation state for one helper account,
/// keyed by helper UID so a changed socket path cannot admit a second
/// controller or hide a pending mutation.
#[derive(Clone)]
pub struct Controller {
    directory: PathBuf,
    lock: PathBuf,
    marker: PathBuf,
    marker_lock: PathBuf,
    /// Present while the person holds control through a viewer, so whoever
    /// finds the controller busy is told to wait for them.
    person: PathBuf,
    /// Present while the person has stopped agents: kept on the person's
    /// side, so a Stop holds even while the helper's input service is down.
    stopped: PathBuf,
    name: String,
}

impl Controller {
    pub fn new(uid: u32) -> Result<Self> {
        Ok(Self::at(uid, &home()?.join("Library/Caches/Atlas")))
    }

    /// Locating state is read-only; the directory appears with the first lock.
    pub fn at(uid: u32, directory: &Path) -> Self {
        let digest = format!("{:x}", Sha256::digest(format!("helper:{uid}").as_bytes()));
        let name = &digest[..24];
        Self {
            directory: directory.to_owned(),
            lock: directory.join(format!("{name}.lock")),
            marker: directory.join(format!("{name}.uncertain.json")),
            marker_lock: directory.join(format!("{name}.state.lock")),
            person: directory.join(format!("{name}.person")),
            stopped: directory.join(format!("{name}.stopped")),
            name: name.to_owned(),
        }
    }

    /// Whether this process may keep its locks and claims here at all. An
    /// agent's sandbox may forbid writing, and the error then says so.
    pub fn writable(&self) -> Result<()> {
        if fs::symlink_metadata(&self.directory).is_ok() {
            real_directory(&self.directory)?;
        }
        probe_write(&self.directory)
    }

    /// The state folder, made if missing, and refused when it is a link:
    /// locks and records kept through a link would be another folder's once
    /// the link is changed or removed, and a second controller could take a
    /// lock of its own.
    fn make_directory(&self) -> Result<()> {
        fs::create_dir_all(&self.directory)
            .map_err(|error| Error::writing(&self.directory, error))?;
        real_directory(&self.directory)
    }

    pub fn acquire(&self, recovery: bool) -> Result<Ownership> {
        self.make_directory()?;

        let lock = open_lock(&self.lock)?;
        if FileExt::try_lock_exclusive(&lock).is_err() {
            return Err(if self.person.exists() {
                Error::new("busy", crate::error::PERSON_HAS_CONTROL)
            } else {
                Error::new(
                    "busy",
                    "Another Atlas controller is active: wait for it to finish, or release it in the viewer.",
                )
            });
        }

        // Nobody held it, so a note left by a viewer that ended is stale.
        let _ = fs::remove_file(&self.person);

        if recovery
            && self
                .pending()?
                .iter()
                .any(|generation| self.in_progress(generation))
        {
            return Err(Error::new(
                "busy",
                "An Atlas executor is still carrying out an action. Wait for it to end before recovering.",
            ));
        }
        if !recovery {
            self.require_known()?;
        }

        Ok(Ownership {
            _lock: lock,
            person: None,
        })
    }

    /// Whether the person stopped agents, as noted on their side.
    pub fn stop_noted(&self) -> bool {
        self.stopped.exists()
    }

    /// Note that the person stopped agents (`on`), or let them go on.
    pub fn note_stop(&self, on: bool) -> Result<()> {
        if fs::symlink_metadata(&self.directory).is_ok() {
            real_directory(&self.directory)?;
        }
        if on {
            self.make_directory()?;
            fs::write(&self.stopped, b"").map_err(|error| Error::writing(&self.directory, error))
        } else {
            match fs::remove_file(&self.stopped) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    Err(Error::writing(&self.directory, error))
                }
                _ => Ok(()),
            }
        }
    }

    /// [`Controller::acquire`] for the person's control through a viewer:
    /// while it is held, others hear that the person has control.
    pub fn acquire_for_person(&self) -> Result<Ownership> {
        let mut ownership = self.acquire(false)?;
        fs::write(&self.person, b"").map_err(|error| Error::writing(&self.directory, error))?;
        ownership.person = Some(self.person.clone());
        Ok(ownership)
    }

    pub fn require_known(&self) -> Result<()> {
        if self.marker.try_exists()? {
            return Err(Error::new(
                "uncertain",
                "The last action's outcome is unknown. Observe the desktop, then run 'atlas restart'.",
            ));
        }

        Ok(())
    }

    fn lock_marker(&self) -> Result<File> {
        self.make_directory()?;
        let file = open_lock(&self.marker_lock)?;
        FileExt::lock_exclusive(&file)?;

        Ok(file)
    }

    /// Generations of mutations whose outcome is still unknown.
    pub fn pending(&self) -> Result<Vec<String>> {
        let _lock = self.lock_marker()?;
        pending(&self.marker)
    }

    /// Generations no mutation in progress will settle: their writer
    /// dropped or lost its claim (an uncertain failure or a crash), or the
    /// entry is not one of ours. For display only; every pending generation
    /// still blocks control until verified recovery.
    pub fn unresolved(&self) -> Result<Vec<String>> {
        // Under the marker lock a finishing mutation cannot clear its entry
        // between reading the marker and probing its claim.
        let _lock = self.lock_marker()?;
        Ok(pending(&self.marker)?
            .into_iter()
            .filter(|generation| !self.in_progress(generation))
            .collect())
    }

    /// The claim file for a generation; only plain tokens name one.
    fn live(&self, generation: &str) -> Option<PathBuf> {
        (!generation.is_empty() && generation.bytes().all(|b| b.is_ascii_hexdigit())).then(|| {
            self.directory
                .join(format!("{}.{generation}.live", self.name))
        })
    }

    /// Whether some live mutation, in any process, still holds the claim.
    fn in_progress(&self, generation: &str) -> bool {
        let Some(live) = self.live(generation) else {
            return false;
        };

        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&live)
            .is_ok_and(|file| {
                FileExt::try_lock_shared(&file).is_err() || claim_executor_running(&file, &live)
            })
    }

    /// Record a pending mutation this process is carrying out.
    pub fn quarantine(&self) -> Result<Pending> {
        self.quarantine_as(&token())
    }

    /// Record a pending mutation under a generation the caller chose, so it
    /// can persist its own claim on that generation first. The claim is
    /// taken before the marker is written, so a marked generation is never
    /// briefly mistaken for an abandoned one.
    pub fn quarantine_as(&self, generation: &str) -> Result<Pending> {
        let live = self
            .live(generation)
            .ok_or_else(|| Error::new("state", "Pending generations are hexadecimal tokens"))?;

        self.make_directory()?;
        let file = open_lock(&live)?;
        FileExt::try_lock_exclusive(&file)
            .map_err(|_| Error::new("busy", "That generation is already being carried out"))?;

        let claim = Pending {
            generation: generation.to_owned(),
            live,
            lock: file,
        };

        // Claim files are only unlinked under the marker lock or by their
        // holder, so if the path still names the locked file here, it will
        // until the claim is dropped.
        let _lock = self.lock_marker()?;
        if !claim.names_own_file() {
            return Err(Error::new(
                "busy",
                "That generation's claim changed while it was taken",
            ));
        }

        let mut pending = pending(&self.marker)?;
        if !pending.iter().any(|entry| entry == generation) {
            pending.push(generation.to_owned());
        }

        write_private(&self.marker, &serde_json::to_vec(&pending)?)?;
        Ok(claim)
    }

    /// Settle one acknowledged mutation; others stay pending.
    pub fn clear_if(&self, generation: &str) -> Result<()> {
        let _lock = self.lock_marker()?;

        if let Some(live) = self.live(generation)
            && !self.in_progress(generation)
        {
            remove_claim(&live);
        }

        let mut pending = pending(&self.marker)?;
        pending.retain(|entry| entry != generation);

        if pending.is_empty() {
            remove_marker(&self.marker)
        } else {
            write_private(&self.marker, &serde_json::to_vec(&pending)?)
        }
    }

    /// Verified recovery settles every pending mutation no live claim holds
    /// and removes every claim file no live mutation holds, listed or not. A
    /// mutation still being carried out (an approved root run among them)
    /// stays pending: only its own end settles it.
    pub fn clear(&self) -> Result<()> {
        let _lock = self.lock_marker()?;
        let running: Vec<String> = pending(&self.marker)?
            .into_iter()
            .filter(|generation| self.in_progress(generation))
            .collect();
        let prefix = format!("{}.", self.name);

        for entry in fs::read_dir(&self.directory)?.flatten() {
            let name = entry.file_name();
            let generation = name
                .to_str()
                .and_then(|name| name.strip_prefix(&prefix)?.strip_suffix(".live"));
            if let Some(generation) = generation
                && self.live(generation).is_some()
                && !self.in_progress(generation)
            {
                remove_claim(&entry.path());
            }
        }

        if running.is_empty() {
            remove_marker(&self.marker)
        } else {
            write_private(&self.marker, &serde_json::to_vec(&running)?)
        }
    }
}

/// Refuse `directory` unless it is a real folder of this account, not a
/// link to one.
fn real_directory(directory: &Path) -> Result<()> {
    let metadata =
        fs::symlink_metadata(directory).map_err(|error| Error::writing(directory, error))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != nix::unistd::getuid().as_raw()
    {
        return Err(Error::new(
            "state",
            format!(
                "{} is a link or not a folder of this account. Remove it, then try again.",
                directory.display()
            ),
        ));
    }

    Ok(())
}

/// Pending generations in a marker; an unreadable one stays pending as unknown.
fn pending(marker: &Path) -> Result<Vec<String>> {
    match fs::read_to_string(marker) {
        Ok(text) => {
            Ok(serde_json::from_str(&text).unwrap_or_else(|_| vec!["legacy-unknown".into()]))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

fn remove_marker(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => Ok(other?),
    }
}

pub struct Lease {
    pub owner: Option<String>,
    pub retired: Option<String>,
    descriptor: Option<Ownership>,
    expires: Instant,
    inflight: bool,
    cleaning: bool,
    ttl: Duration,
}

impl Default for Lease {
    fn default() -> Self {
        Self::new(Duration::from_secs(10))
    }
}

impl Lease {
    pub fn new(ttl: Duration) -> Self {
        Self {
            owner: None,
            retired: None,
            descriptor: None,
            expires: Instant::now(),
            inflight: false,
            cleaning: false,
            ttl,
        }
    }

    pub fn expire(&mut self) {
        if self.owner.is_some() && Instant::now() >= self.expires {
            self.retired = self.owner.take();
            self.cleaning = true;
        }

        if self.owner.is_none() && !self.inflight && !self.cleaning {
            self.descriptor = None;
        }
    }

    pub fn acquire(&mut self, controller: &Controller) -> Result<String> {
        self.expire();

        if self.descriptor.is_some() {
            return Err(Error::new("busy", "Another viewer has control"));
        }

        self.descriptor = Some(controller.acquire_for_person()?);
        let token = token();
        self.owner = Some(token.clone());
        self.expires = Instant::now() + self.ttl;

        Ok(token)
    }

    pub fn require(&mut self, token: &str) -> Result<()> {
        self.expire();

        // Tokens are fixed-length random strings; compare without an early byte exit.
        let matches = self.owner.as_ref().is_some_and(|owner| {
            owner.len() == token.len()
                && owner
                    .bytes()
                    .zip(token.bytes())
                    .fold(0u8, |v, (a, b)| v | (a ^ b))
                    == 0
        });
        if !matches {
            return Err(Error::new(
                "lease-lost",
                "Control was released or expired. Take control again.",
            ));
        }

        Ok(())
    }

    pub fn renew(&mut self, token: &str) -> Result<()> {
        self.require(token)?;
        self.expires = Instant::now() + self.ttl;
        Ok(())
    }

    pub fn release(&mut self) {
        self.owner = None;
        self.retired = None;
        self.cleaning = false;
        self.expire();
    }

    pub fn abandon_acquire(&mut self, owner: &str) {
        if self.require(owner).is_ok() && !self.inflight && !self.cleaning {
            self.release();
        }
    }

    pub fn begin_release(&mut self, requested: Option<&str>) -> Result<Option<String>> {
        self.expire();
        let current = self.owner.as_ref().or(self.retired.as_ref());

        // Releasing a lease that already ended is a no-op; a token can
        // never end a different controller's lease.
        if requested.is_some() && current.is_none() {
            return Ok(None);
        }

        if requested.is_some() && requested != current.map(String::as_str) {
            return Err(Error::new(
                "lease-lost",
                "This token cannot release another controller",
            ));
        }

        // Both the live and any retired token end here.
        let retired = self.retired.take();
        let previous = self.owner.take().or(retired);
        if previous.is_some() {
            self.cleaning = true;
        }

        Ok(previous)
    }

    pub fn end_release(&mut self) {
        self.cleaning = false;
        self.expire();
    }

    pub fn active(&mut self) -> bool {
        self.expire();
        self.descriptor.is_some()
    }
}

pub type SharedLease = Arc<Mutex<Lease>>;

/// Holds the controller flock until an acknowledged or uncertain operation settles.
pub struct ActionPermit {
    lease: SharedLease,
}

impl ActionPermit {
    pub async fn begin(lease: SharedLease, token: &str) -> Result<Self> {
        {
            let mut state = lease.lock().await;
            state.require(token)?;

            if state.inflight {
                return Err(Error::new("busy", "Wait for the current input to finish"));
            }

            state.inflight = true;
        }

        Ok(Self { lease })
    }
}

impl Drop for ActionPermit {
    fn drop(&mut self) {
        let finish = |state: &mut Lease| {
            state.inflight = false;
            state.expire();
        };

        if let Ok(mut state) = self.lease.try_lock() {
            finish(&mut state);
        } else if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let lease = self.lease.clone();
            runtime.spawn(async move { finish(&mut *lease.lock().await) });
        } else {
            // Outside any runtime, waiting here blocks no async task.
            finish(&mut self.lease.blocking_lock());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropping_ownership_releases_control_while_a_duplicate_descriptor_stays_open() {
        if !crate::transaction_test::isolated(
            "control::tests::dropping_ownership_releases_control_while_a_duplicate_descriptor_stays_open",
        ) {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, directory.path());
        let ownership = controller.acquire_for_person().unwrap();
        let duplicate = ownership._lock.try_clone().unwrap();
        assert_eq!(controller.acquire(false).unwrap_err().code, "busy");

        drop(ownership);

        assert!(!controller.person.exists());
        let _next = controller.acquire(false).unwrap();
        assert!(duplicate.metadata().is_ok());
    }

    #[test]
    fn a_privileged_executor_is_identified_and_keeps_recovery_excluded() {
        let dir = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, dir.path());
        let pending = controller.quarantine().unwrap();
        let claims = ExecutorClaims::new(std::slice::from_ref(&pending)).unwrap();
        // launchd is a stable, root-owned process. Observe it only: this test
        // never dispatches privileged work or changes a real helper's state.
        let pid = nix::unistd::Pid::from_raw(1);
        record_executor(pid).unwrap();
        let live = pending.live.clone();
        let record = live.with_extension("executor.live");
        let executor: Executor = serde_json::from_slice(&fs::read(&record).unwrap()).unwrap();
        assert_eq!(executor.pid, 1);
        assert_eq!(executor.boot, executor_boot().unwrap());
        assert_eq!(executor.start, executor_start(1).unwrap());
        assert!(executor.start[0] > 0);
        drop(claims);
        drop(pending);
        assert!(live.exists(), "the executor outlives its caller's claim");
        assert!(executor_running(&File::open(record).unwrap()));
        assert_eq!(
            controller.acquire(true).unwrap_err().code,
            "busy",
            "recovery stays excluded"
        );
    }

    #[test]
    #[allow(
        unsafe_code,
        reason = "FFI: compare the previously stored proc_pidinfo timestamp"
    )]
    fn executor_start_keeps_the_previously_recorded_microsecond_identity() {
        let pid = nix::unistd::getpid().as_raw();
        let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::uninit();
        let size = std::mem::size_of_val(&info) as i32;
        // SAFETY: proc_pidinfo receives a writable buffer of the declared size.
        let read = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                info.as_mut_ptr().cast(),
                size,
            )
        };
        assert_eq!(read, size);
        // SAFETY: proc_pidinfo initialized the complete proc_bsdinfo above.
        let info = unsafe { info.assume_init() };
        assert_eq!(
            executor_start(pid).unwrap(),
            [info.pbi_start_tvsec, info.pbi_start_tvusec]
        );
    }

    #[test]
    fn an_exited_executor_has_no_start_identity() {
        let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
        let pid = child.id() as i32;
        assert!(child.wait().unwrap().success());
        assert!(executor_start(pid).is_err());
    }

    #[test]
    fn executor_claims_persist_the_pid_boot_and_process_start_before_dispatch() {
        let dir = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, dir.path());
        let pending = controller.quarantine().unwrap();
        let _claims = ExecutorClaims::new(std::slice::from_ref(&pending)).unwrap();
        let pid = nix::unistd::getpid();
        record_executor_with(
            pid,
            || Ok("boot-session".into()),
            |observed| {
                assert_eq!(observed, pid.as_raw());
                Ok([300, 400])
            },
        )
        .unwrap();
        assert!(pending.names_own_file());
        let executor: Executor = serde_json::from_slice(
            &fs::read(pending.live.with_extension("executor.live")).unwrap(),
        )
        .unwrap();
        assert_eq!(executor.pid, pid.as_raw());
        assert_eq!(executor.boot, "boot-session");
        assert_eq!(executor.start, [300, 400]);
    }

    #[test]
    fn a_reader_of_the_claim_file_sees_a_running_executor() {
        let dir = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, dir.path());
        let pending = controller.quarantine().unwrap();
        let _claims = ExecutorClaims::new(std::slice::from_ref(&pending)).unwrap();
        record_executor_with(
            nix::unistd::getpid(),
            || Ok("boot-session".into()),
            |_| Ok([300, 400]),
        )
        .unwrap();
        assert!(executor_running_with(
            &File::open(&pending.live).unwrap(),
            || Ok("boot-session".into()),
            |_| Ok([300, 400]),
        ));
    }

    #[test]
    fn a_reused_executor_pid_is_not_alive_on_a_different_boot_or_start_time() {
        let mut file = tempfile::tempfile().unwrap();
        let executor = Executor {
            pid: nix::unistd::getpid().as_raw(),
            boot: "boot-session".into(),
            start: [300, 400],
        };
        file.write_all(&serde_json::to_vec(&executor).unwrap())
            .unwrap();
        assert!(executor_running_with(
            &file,
            || Ok(executor.boot.clone()),
            |pid| {
                assert_eq!(pid, executor.pid);
                Ok(executor.start)
            }
        ));
        for changed in ["boot", "start"] {
            let mut boot = executor.boot.clone();
            let mut start = executor.start;
            match changed {
                "boot" => boot.push('2'),
                _ => start[1] += 1,
            }
            assert!(
                !executor_running_with(
                    &file,
                    || Ok(boot),
                    |pid| {
                        assert_eq!(pid, executor.pid);
                        Ok(start)
                    }
                ),
                "different {changed}"
            );
        }
        assert!(
            executor_running_with(
                &file,
                || Err(Error::new("state", "unreadable boot")),
                |_| Err(Error::new("state", "unreadable process")),
            ),
            "unreadable observations do not release a live executor"
        );
        assert!(executor_running_with(
            &file,
            || Err(Error::new("state", "unreadable boot")),
            |_| Ok([0, 0]),
        ));
        assert!(executor_running_with(
            &file,
            || Ok("different boot".into()),
            |_| Err(Error::new("state", "unreadable process")),
        ));
    }

    #[test]
    fn a_clock_change_does_not_release_a_surviving_executor() {
        let dir = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, dir.path());
        let pending = controller.quarantine().unwrap();
        let _claims = ExecutorClaims::new(std::slice::from_ref(&pending)).unwrap();
        let boottime = std::cell::Cell::new(100);
        let read = |name: &std::ffi::CStr| match name.to_bytes() {
            b"kern.bootsessionuuid" => Ok("same-boot-session".into()),
            b"kern.boottime" => Ok(boottime.get().to_string()),
            _ => panic!("unexpected sysctl"),
        };
        record_executor_with(
            nix::unistd::getpid(),
            || executor_boot_with(read),
            |_| Ok([300, 400]),
        )
        .unwrap();
        boottime.set(200);
        assert!(executor_running_with(
            &File::open(pending.live.with_extension("executor.live")).unwrap(),
            || executor_boot_with(read),
            |_| Ok([300, 400]),
        ));
    }

    #[test]
    fn unreadable_and_legacy_executor_records_do_not_release_a_live_process() {
        let mut file = tempfile::tempfile().unwrap();
        for record in [nix::unistd::getpid().to_string(), "{".into(), "-1".into()] {
            file.set_len(0).unwrap();
            file.seek(SeekFrom::Start(0)).unwrap();
            file.write_all(record.as_bytes()).unwrap();
            assert!(executor_running(&file));
        }
    }

    #[test]
    fn an_exited_executor_releases_its_claim_when_its_identity_is_unavailable() {
        let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
        let pid = child.id() as i32;
        assert!(child.wait().unwrap().success());
        let mut file = tempfile::tempfile().unwrap();
        let executor = Executor {
            pid,
            boot: "boot-session".into(),
            start: [300, 400],
        };
        file.write_all(&serde_json::to_vec(&executor).unwrap())
            .unwrap();
        assert!(!executor_running_with(
            &file,
            || Err(Error::new("state", "unreadable boot")),
            |_| Err(Error::new("state", "process no longer exists")),
        ));
    }

    #[test]
    fn a_partial_executor_record_does_not_block_recovery_after_its_executor_and_boot_are_gone() {
        if !crate::transaction_test::isolated(
            "control::tests::a_partial_executor_record_does_not_block_recovery_after_its_executor_and_boot_are_gone",
        ) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, dir.path());
        let pending = controller.quarantine().unwrap();
        let generation = pending.to_string();
        let live = pending.live.clone();
        let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
        let pid = child.id() as i32;
        assert!(child.wait().unwrap().success());
        let record = serde_json::to_vec(&Executor {
            pid,
            boot: "previous-boot-session".into(),
            start: [300, 400],
        })
        .unwrap();
        publish_executor_with(&live, &record, |file, record| file.write_all(record)).unwrap();
        assert!(
            publish_executor_with(&live, &record, |file, record| {
                file.write_all(&record[..record.len() / 2])?;
                file.sync_all()?;
                Err(std::io::Error::other("interrupted write"))
            })
            .is_err()
        );
        FileExt::unlock(&pending.lock).unwrap();

        assert_eq!(controller.pending().unwrap(), vec![generation]);
        assert!(live.exists());
        let _ownership = controller.acquire(true).unwrap();
        controller.clear().unwrap();
        assert!(controller.pending().unwrap().is_empty());
        assert!(!live.exists());
        assert!(!live.with_extension("executor.live").exists());
    }

    thread_local! {
        pub(super) static INTERRUPT_CLAIM_REMOVAL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    #[test]
    fn interrupted_claim_cleanup_leaves_recovery_unblocked_despite_a_malformed_record() {
        let dir = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, dir.path());
        let pending = controller.quarantine().unwrap();
        let generation = pending.to_string();
        let live = pending.live.clone();
        let sidecar = live.with_extension("executor.live");
        let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
        let pid = child.id() as i32;
        assert!(child.wait().unwrap().success());
        let record = serde_json::to_vec(&Executor {
            pid,
            boot: "previous-boot-session".into(),
            start: [300, 400],
        })
        .unwrap();
        fs::write(&live, b"{").unwrap();
        fs::write(&sidecar, &record).unwrap();

        INTERRUPT_CLAIM_REMOVAL.with(|interrupt| interrupt.set(true));
        assert!(std::panic::catch_unwind(|| drop(pending)).is_err());

        assert!(!live.exists());
        assert_eq!(fs::read(&sidecar).unwrap(), record);
        assert_eq!(
            controller.pending().unwrap(),
            std::slice::from_ref(&generation)
        );
        assert_eq!(controller.unresolved().unwrap(), [generation]);
        let ownership = controller.acquire(true).unwrap();
        controller.clear().unwrap();
        assert!(controller.pending().unwrap().is_empty());
        assert!(sidecar.exists());
        drop(ownership);
        assert!(controller.acquire(false).is_ok());
    }

    #[test]
    fn a_partial_executor_record_keeps_a_possibly_live_executor_pending() {
        if !crate::transaction_test::isolated(
            "control::tests::a_partial_executor_record_keeps_a_possibly_live_executor_pending",
        ) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, dir.path());
        let pending = controller.quarantine().unwrap();
        let generation = pending.to_string();
        let live = pending.live.clone();
        let record = nix::unistd::getpid().to_string().into_bytes();
        fs::write(&live, &record).unwrap();
        let next = serde_json::to_vec(&Executor {
            pid: nix::unistd::getpid().as_raw(),
            boot: "boot-session".into(),
            start: [300, 400],
        })
        .unwrap();
        assert!(
            publish_executor_with(&live, &next, |file, record| {
                file.write_all(&record[..record.len() / 2])?;
                file.sync_all()?;
                Err(std::io::Error::other("interrupted write"))
            })
            .is_err()
        );
        assert!(pending.names_own_file());
        drop(pending);

        assert_eq!(controller.acquire(true).unwrap_err().code, "busy");
        controller.clear().unwrap();
        assert_eq!(controller.pending().unwrap(), vec![generation]);
        assert!(live.exists());
        assert_eq!(fs::read(live).unwrap(), record);
    }

    /// Checking that the lock can be kept creates its directory, leaves no
    /// file behind and never touches the lock; a place that cannot be
    /// written says so.
    #[test]
    fn writable_leaves_the_directory_as_it_found_it() {
        let directory = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, &directory.path().join("Atlas"));
        controller.writable().unwrap();

        assert_eq!(fs::read_dir(&controller.directory).unwrap().count(), 0);
        controller.acquire(true).unwrap();

        let unwritable = Controller::at(505, Path::new("/dev/null/Atlas"));
        assert_eq!(unwritable.writable().unwrap_err().code, "io");
    }

    #[test]
    fn a_state_folder_that_is_a_link_is_refused_before_any_lock_or_record() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("elsewhere");
        fs::create_dir(&target).unwrap();
        let link = dir.path().join("Atlas");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let controller = Controller::at(505, &link);

        assert_eq!(controller.acquire(true).unwrap_err().code, "state");
        assert_eq!(controller.quarantine().unwrap_err().code, "state");
        assert_eq!(controller.pending().unwrap_err().code, "state");
        assert_eq!(controller.writable().unwrap_err().code, "state");
        assert_eq!(controller.note_stop(false).unwrap_err().code, "state");
        assert_eq!(controller.note_stop(true).unwrap_err().code, "state");
        assert_eq!(
            fs::read_dir(&target).unwrap().count(),
            0,
            "nothing kept through the link"
        );
    }

    #[test]
    fn recovery_never_settles_a_mutation_still_being_carried_out() {
        let dir = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, &dir.path().join("Atlas"));
        let lost = controller.quarantine().unwrap();
        drop(lost);
        let running = controller.quarantine().unwrap();

        assert_eq!(controller.acquire(true).unwrap_err().code, "busy");

        // Recovery from another process, while the run goes on.
        Controller::at(505, &dir.path().join("Atlas"))
            .clear()
            .unwrap();
        assert_eq!(
            controller.pending().unwrap(),
            [running.generation().to_owned()]
        );
        assert!(controller.require_known().is_err());

        controller.clear_if(&running).unwrap();
        drop(running);
        assert!(controller.pending().unwrap().is_empty());
    }

    /// While the person holds control through a viewer, an agent finding
    /// the controller busy is told to wait for them; once they let go, or
    /// when the note outlived a viewer that ended, nobody is.
    #[test]
    fn an_agent_finding_the_person_in_control_is_told_to_wait_for_them() {
        let directory = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, &directory.path().join("Atlas"));

        let person = controller.acquire_for_person().unwrap();
        let refused = controller.acquire(false).err().unwrap();
        assert_eq!(refused.code, "busy");
        assert_eq!(refused.error, crate::error::PERSON_HAS_CONTROL);
        drop(person);
        assert!(
            !controller.person.exists(),
            "the note goes with the control"
        );

        // A child another test is starting may hold the lock's file a moment
        // past its release, until it runs its own program.
        let agent = (0..50)
            .find_map(|_| {
                controller.acquire(false).ok().or_else(|| {
                    std::thread::sleep(Duration::from_millis(20));
                    None
                })
            })
            .expect("the lock comes free");
        assert_ne!(
            controller.acquire(false).err().unwrap().error,
            crate::error::PERSON_HAS_CONTROL,
            "another agent is not the person"
        );
        drop(agent);

        fs::write(&controller.person, b"").unwrap();
        drop(controller.acquire(false).unwrap());
        assert!(!controller.person.exists(), "a stale note is cleared");
    }

    /// A permit dropped outside any runtime, while the lease is locked
    /// elsewhere, waits for it instead of panicking.
    #[test]
    fn a_permit_settles_without_a_runtime() {
        let lease: SharedLease = Arc::new(Mutex::new(Lease::new(Duration::from_secs(60))));
        let token = token();

        {
            let mut state = lease.blocking_lock();
            state.owner = Some(token.clone());
            state.expires = Instant::now() + Duration::from_secs(60);
        }

        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let permit = runtime
            .block_on(ActionPermit::begin(lease.clone(), &token))
            .unwrap();

        drop(runtime);
        let held = lease.clone().blocking_lock_owned();
        let holder = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            drop(held);
        });

        drop(permit);
        holder.join().unwrap();
        assert!(!lease.blocking_lock().inflight);
    }

    #[test]
    fn beginning_a_release_clears_the_retired_token_too() {
        let mut lease = Lease::new(Duration::from_secs(60));
        lease.owner = Some("a".repeat(48));
        lease.expires = Instant::now() + Duration::from_secs(60);
        lease.retired = Some("b".repeat(48));

        assert_eq!(lease.begin_release(None).unwrap(), Some("a".repeat(48)));
        assert_eq!((lease.owner.clone(), lease.retired.clone()), (None, None));

        lease.retired = Some("b".repeat(48));
        assert_eq!(lease.begin_release(None).unwrap(), Some("b".repeat(48)));
        assert_eq!(lease.retired, None);
    }
}
