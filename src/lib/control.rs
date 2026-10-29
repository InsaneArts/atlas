//! Who may act on the helper, and what is still unsettled: one controller at
//! a time ([`Controller`], a file lock per helper account), a marker of every
//! mutation whose outcome is unknown until verified recovery clears it, and
//! the short-lived [`Lease`] a viewer holds while a person has control.
//! A command an agent starts with `run` lets go of the lock once it is sent
//! and is marked as one, so other agents' actions go on beside it while
//! recovery still waits for it.
//! Beside the lock and the marker it keeps labels for people and agents:
//! which host session holds control, which waits, and which claimed each
//! pending change. Nothing here decides anything by them.
mod causes;
mod commands;
#[cfg(test)]
mod commands_tests;
mod notes;
#[cfg(test)]
mod notes_tests;
mod running;
mod turns;

pub use notes::Note;

use crate::{
    config::home,
    error::{Error, Holder, Result},
    host_session::{HostSession, Key, Label},
    interrupt::{Hold, Interrupt},
};
use fs2::FileExt;
use rand::{Rng, RngCore};
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

/// How long an agent's command that changes the helper waits for another
/// Atlas command to let go of it, unless told otherwise: long enough for the
/// usual command ahead, short enough that the wait and a default command
/// stay within an agent tool's two-minute limit.
pub const CONTROL_WAIT: Duration = Duration::from_secs(30);

/// The longest a command may be told to wait for control: as long as the
/// longest run.
pub const CONTROL_WAIT_MAX: Duration = Duration::from_secs(600);

/// Random bytes in a [`token`]; it is twice as many hexadecimal digits.
pub const TOKEN_BYTES: usize = 24;

pub fn token() -> String {
    let mut bytes = [0; TOKEN_BYTES];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A name shaped as a [`token`] makes it: Atlas's own random name for a
/// copy folder or an environment plan, never one a person chose.
#[derive(
    Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(try_from = "String", into = "String")]
pub struct Token(String);

impl Token {
    /// `text` as a token, when it is [`TOKEN_BYTES`] bytes as lowercase
    /// hexadecimal digits.
    pub fn parse(text: &str) -> Option<Self> {
        crate::digest::lower_hex(text, TOKEN_BYTES * 2).then(|| Self(text.to_owned()))
    }

    /// A new random token.
    pub fn new() -> Self {
        Self(token())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for Token {
    fn default() -> Self {
        Self::new()
    }
}

impl TryFrom<String> for Token {
    type Error = String;

    fn try_from(text: String) -> std::result::Result<Self, Self::Error> {
        Self::parse(&text).ok_or_else(|| format!("{text} is not one of Atlas's tokens"))
    }
}

impl From<Token> for String {
    fn from(token: Token) -> Self {
        token.0
    }
}

impl std::fmt::Display for Token {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
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

/// A wait in whole seconds, rounded to the nearest, as a reply says how
/// long Atlas waited: a full 30-second wait that ends a few milliseconds
/// short of it is 30, as its message says.
pub(crate) fn seconds(waited: Duration) -> u64 {
    waited.saturating_add(Duration::from_millis(500)).as_secs()
}

/// Busy because the person has control through a viewer: never waited for.
/// `waited` is how long a controller given a wait had waited by then.
fn busy_person(waited: Option<Duration>) -> Error {
    let error = Error::new("busy", crate::error::PERSON_HAS_CONTROL).held_by(Holder::Person);
    match waited {
        Some(waited) => error.with("waited", seconds(waited)),
        None => error,
    }
}

/// Busy because another Atlas command kept control for all of `wait`, after
/// this try `waited` that long; a controller given no wait answers at once.
/// A command Atlas.app runs takes the lock too, so "agent" is another Atlas
/// command, usually another agent's.
fn busy_agent(waited: Option<Duration>, wait: Duration) -> Error {
    let message = match waited {
        Some(_) => format!(
            "Another Atlas command, often another agent's, kept control for all of the {} seconds Atlas could wait.",
            seconds(wait)
        ),
        None => "Another Atlas controller is active: wait for it to finish, or release it in the viewer."
            .to_owned(),
    };

    let error = Error::new("busy", message).held_by(Holder::Agent);
    match waited {
        Some(waited) => error
            .with("waited", seconds(waited))
            .with("next", crate::ui::hints::WAITED),
        None => error,
    }
}

/// The most commands agents started with `run` that may run at once, so
/// the helper keeps connections free for reads and other actions.
pub const RUNNING_COMMANDS_MAX: usize = 8;

/// Busy because `message` names commands agents started with `run` that
/// still run: held by an agent, so nobody is told to recover them, and
/// marked as a running command for programs to tell from a held lock.
fn busy_running(message: impl Into<String>) -> Error {
    Error::new("busy", message)
        .held_by(Holder::Agent)
        .with("running", "command")
        .with("next", crate::ui::hints::COMMAND_RUNNING)
}

/// Busy because a command an agent started with `run` still runs, for
/// control taken for `purpose`: setup, removal and recovery hear that
/// stopping agents ends it at once.
fn busy_command(purpose: Purpose) -> Error {
    busy_running(match purpose {
        Purpose::Recovery => {
            "A command an agent started is still running in the helper. Wait for it to end, or stop agents to end it now."
        }
        Purpose::Change(_) => {
            "A command an agent started is still running in the helper. Wait for it to end."
        }
    })
}

/// What an acquisition may go on beside.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Beside {
    /// Nothing pending at all, running or not.
    Nothing,
    /// Commands agents started with `run` that are still running. Every
    /// other pending outcome still refuses.
    Commands,
}

/// What control is taken for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Purpose {
    /// Recovery, setup and removal: refused while any pending change is
    /// still being carried out, so never beside a running command.
    Recovery,
    /// A change, going on beside what it names.
    Change(Beside),
}

/// How a pending change is recorded beside the marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    /// A change carried out under control until its outcome is known.
    Change,
    /// A command an agent started with `run`, which others act beside.
    Command,
}

/// What is pending, as one read under the marker lock finds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unsettled {
    /// Nothing: there is no marker.
    Nothing,
    /// Only commands agents started with `run`, each still running under
    /// its own claim.
    Commands,
    /// An outcome nothing may settle yet, or one not known to be a command.
    Unknown,
}

/// How long to sleep before trying the lock again: about a tenth of a
/// second, spread by 25 ms either way so waiters do not try in step, and
/// never past the end of the wait.
pub(crate) fn poll(remaining: Duration) -> Duration {
    poll_with(remaining, rand::rng().random_range(75..=125))
}

/// [`poll`] with its spread chosen: `jitter_ms` milliseconds, or what is
/// left of the wait if that is less.
fn poll_with(remaining: Duration, jitter_ms: u64) -> Duration {
    Duration::from_millis(jitter_ms).min(remaining)
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
    /// Keeps a signal's stop waiting until the claim file is let go.
    _hold: Hold,
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
    /// The note naming the host session that holds it, gone before the lock
    /// is, so the lock is never free while a note still names its holder.
    holder: Option<PathBuf>,
}

impl Drop for Ownership {
    fn drop(&mut self) {
        for note in [&self.holder, &self.person].into_iter().flatten() {
            let _ = fs::remove_file(note);
        }
        let _ = FileExt::unlock(&self._lock);
    }
}

/// One pending change, as far as people and agents are told of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cause {
    pub generation: String,
    /// Whether a live process is still carrying it out.
    pub running: bool,
    /// Whether it is a command an agent started with `run`, still running
    /// under its own claim: work under way, not an unknown outcome.
    pub command: bool,
    /// The host session that claimed it, when one is recorded.
    pub session: Option<Key>,
}

/// Where a wait for another controller has got to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitEvent {
    /// The first try found the lock held, and the controller waits.
    Began,
    /// The wait is over: control was taken, or the answer is a refusal.
    Ended,
}

/// What [`Controller::acquire`] tells about its wait; it runs on the
/// waiting thread.
pub type WaitHook = Arc<dyn Fn(WaitEvent) + Send + Sync>;

/// A wait under way: its hook hears [`WaitEvent::Began`] when it is made
/// and [`WaitEvent::Ended`] when it is dropped.
struct WaitNotice<'a>(Option<&'a WaitHook>);

impl<'a> WaitNotice<'a> {
    fn begin(hook: Option<&'a WaitHook>) -> Self {
        if let Some(hook) = hook {
            hook(WaitEvent::Began);
        }
        Self(hook)
    }
}

impl Drop for WaitNotice<'_> {
    fn drop(&mut self) {
        if let Some(hook) = self.0 {
            hook(WaitEvent::Ended);
        }
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
    /// How long [`Controller::acquire`] may keep trying, in all, while
    /// another controller has the lock, before answering busy. Zero
    /// answers at once.
    wait: Duration,
    /// When that wait runs out, shared by every acquire and every clone.
    until: Option<Instant>,
    /// Whether a Stop ends the wait and refuses control taken after one.
    heeds_stop: bool,
    /// Told when [`Controller::acquire`] starts waiting and when it stops.
    on_wait: Option<WaitHook>,
    /// The stop a signal asks: it refuses control from then on, and waits
    /// for the changes this controller has under way.
    interrupt: Interrupt,
    /// The host session this controller's changes are claimed for, so one
    /// of unknown outcome can be shown to the session that made it. A label
    /// only: what blocks and what recovery clears never depend on it.
    label: Option<Label>,
    /// Which host session claimed each pending generation, beside the marker.
    causes: PathBuf,
    /// Which pending generations are commands agents started with `run`,
    /// beside the marker.
    commands: PathBuf,
    /// Who started each of those commands, beside the marker.
    running: PathBuf,
    /// Which host session holds control right now, beside the lock.
    holder: PathBuf,
    /// The commands waiting for control, one note each, in the order they
    /// take it.
    waiting: PathBuf,
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
            wait: Duration::ZERO,
            until: None,
            heeds_stop: false,
            on_wait: None,
            interrupt: Interrupt::process().clone(),
            label: None,
            causes: directory.join(format!("{name}.causes.json")),
            commands: directory.join(format!("{name}.commands.json")),
            running: directory.join(format!("{name}.running.json")),
            holder: directory.join(format!("{name}.holder.json")),
            waiting: directory.join(format!("{name}.waiting")),
        }
    }

    /// This controller, trying for up to `wait` to take control. The wait
    /// is one allowance, starting now, for every acquire of this controller
    /// and its clones: each spends what it waits, and the next gets what is
    /// left. It blocks the calling thread, so only a command-line process
    /// whose command is its only work may be given one; a server (viewer,
    /// MCP, watch) never is.
    #[must_use]
    pub fn waiting(mut self, wait: Duration) -> Self {
        self.wait = wait;
        self.until = Some(Instant::now() + wait);
        self
    }

    /// How long, in all, [`Controller::acquire`] may wait for another
    /// controller.
    pub fn wait(&self) -> Duration {
        self.wait
    }

    /// What is left of [`Controller::wait`].
    pub fn remaining(&self) -> Duration {
        self.until.map_or(Duration::ZERO, |until| {
            until.saturating_duration_since(Instant::now())
        })
    }

    /// This controller, for a command the person's Stop refuses: once
    /// agents are stopped, a wait ends with `stopped`, and control the
    /// lock gives after waiting is refused too. With no wait, the Stop is
    /// the caller's to check, as before.
    #[must_use]
    pub fn heeding_stop(mut self) -> Self {
        self.heeds_stop = true;
        self
    }

    /// This controller, telling `hook` when a wait for another controller
    /// begins and when it ends. A try that takes control at once, or is
    /// refused at once, waits for nothing and tells nothing. A process that
    /// exits while waiting (`std::process::exit`, a signal) never tells
    /// [`WaitEvent::Ended`].
    #[must_use]
    pub fn on_wait(mut self, hook: impl Fn(WaitEvent) + Send + Sync + 'static) -> Self {
        self.on_wait = Some(Arc::new(hook));
        self
    }

    /// This controller, claiming its changes and holding control for host
    /// `session`, as labels only.
    #[must_use]
    pub fn labelled(mut self, session: Option<&HostSession>) -> Self {
        self.label = session.map(HostSession::label);
        self
    }

    /// This controller, stopped by `interrupt` instead of this process's
    /// signals.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn interrupted_by(mut self, interrupt: Interrupt) -> Self {
        self.interrupt = interrupt;
        self
    }

    /// The stop a signal asks of this controller's command.
    pub(crate) fn interrupt(&self) -> &Interrupt {
        &self.interrupt
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

    /// Take control. Recovery is refused while any pending change is still
    /// being carried out; anything else while any change is pending at all,
    /// a command an agent started included.
    pub fn acquire(&self, recovery: bool) -> Result<Ownership> {
        self.acquire_for(if recovery {
            Purpose::Recovery
        } else {
            Purpose::Change(Beside::Nothing)
        })
    }

    /// [`Controller::acquire`] for an agent's own action, going on beside
    /// commands still running under their own claims.
    pub fn acquire_beside_commands(&self) -> Result<Ownership> {
        self.acquire_for(Purpose::Change(Beside::Commands))
    }

    /// [`Controller::acquire`] for `purpose`.
    fn acquire_for(&self, purpose: Purpose) -> Result<Ownership> {
        // Nothing new starts once a signal asked this command to stop.
        self.interrupt.check()?;
        self.make_directory()?;

        let lock = open_lock(&self.lock)?;
        self.take(&lock)?;

        // Nobody held it, so a note left by a viewer that ended is stale.
        let _ = fs::remove_file(&self.person);

        match purpose {
            Purpose::Recovery => self.require_nothing_running()?,
            Purpose::Change(beside) => self.require_known_beside(beside)?,
        }

        Ok(Ownership {
            _lock: lock,
            person: None,
            holder: self.note_holder(),
        })
    }

    /// Name this controller's host session beside the lock it now holds, or
    /// clear the note an earlier holder left when it has none: a label for
    /// whoever finds the helper busy. The note's path, when one was written.
    fn note_holder(&self) -> Option<PathBuf> {
        let Some(label) = &self.label else {
            let _ = fs::remove_file(&self.holder);
            return None;
        };

        Note::now(Some(label))
            .and_then(|note| note.write(&self.holder).ok())
            .map(|()| self.holder.clone())
    }

    /// The holder note, as written, when there is one that parses. Whether
    /// its command still runs is the reader's to check.
    pub fn holder_note(&self) -> Option<Note> {
        Note::read(&self.holder)
    }

    /// The notes of the commands waiting for control now, oldest first:
    /// for apps to show who waits, never to decide who goes next.
    pub fn waiting_notes(&self) -> Vec<Note> {
        let mut notes = turns::waiting(&self.waiting);
        notes.sort_by_key(|note| note.since);
        notes
    }

    /// The notes of the commands agents started with `run` that still run,
    /// oldest first: such a command holds no control while it runs, so
    /// only its note names a session on its first command. For apps to
    /// show who acts, never to decide who may.
    pub fn running_notes(&self) -> Vec<Note> {
        let marked = self.commands();
        let mut notes: Vec<Note> = running::read(&self.running)
            .into_iter()
            .filter(|(generation, note)| marked.contains(generation) && note.live())
            .map(|(_, note)| note)
            .collect();
        notes.sort_by_key(|note| note.since);
        notes
    }

    /// Where the holder note is kept, for tests that stand in for another
    /// command's.
    #[cfg(test)]
    pub(crate) fn holder_path(&self) -> &Path {
        &self.holder
    }

    /// Lock `lock`, trying again for what is left of [`Controller::wait`]
    /// while another controller holds it. A controller given a wait lets
    /// those already waiting go first.
    fn take(&self, lock: &File) -> Result<()> {
        let started = Instant::now();
        let first = self.wait.is_zero() || !turns::queued(&self.waiting);
        if self.try_take(lock, started, first)? {
            return Ok(());
        }

        self.wait_for_lock(lock, started)
    }

    /// Try `lock` again about ten times a second, between telling the hook
    /// that the wait began and that it ended, whenever no command that began
    /// waiting earlier is still waiting. A signal's stop ends the wait
    /// within one poll.
    fn wait_for_lock(&self, lock: &File, started: Instant) -> Result<()> {
        let _notice = WaitNotice::begin(self.on_wait.as_ref());
        let turn = turns::Turn::join(&self.waiting, self.label.as_ref());
        loop {
            std::thread::sleep(poll(self.remaining()));
            self.interrupt.check_waiting(started.elapsed())?;
            if self.try_take(lock, started, turn.first())? {
                return Ok(());
            }
        }
    }

    /// One try at `lock`, `started` being when the first was made, unless it
    /// is not this command's `turn`: true once it is held, false while
    /// another controller holds it (or goes first) and some of the wait is
    /// left, else why not. The person's note and a Stop are looked at on
    /// every try, so either ends a wait within one poll, and the person is
    /// never waited for. A refused lock is let go when the caller drops it.
    fn try_take(&self, lock: &File, started: Instant, turn: bool) -> Result<bool> {
        let taken = turn && FileExt::try_lock_exclusive(lock).is_ok();
        let waited = (!self.wait.is_zero()).then(|| started.elapsed());
        if !taken && self.person.exists() {
            return Err(busy_person(waited));
        }
        if self.heeds_stop && waited.is_some() && self.stop_noted() {
            return Err(Error::new("stopped", crate::error::STOPPED));
        }
        if taken {
            return Ok(true);
        }
        if self.remaining().is_zero() {
            return Err(busy_agent(waited, self.wait));
        }

        Ok(false)
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
    /// while it is held, others hear that the person has control. A command
    /// an agent started keeps running beside it, as it does beside other
    /// agents' actions.
    pub fn acquire_for_person(&self) -> Result<Ownership> {
        let mut ownership = self.acquire_beside_commands()?;
        fs::write(&self.person, b"").map_err(|error| Error::writing(&self.directory, error))?;
        ownership.person = Some(self.person.clone());
        Ok(ownership)
    }

    /// Refuse while any change is pending, a command still running
    /// included.
    pub fn require_known(&self) -> Result<()> {
        self.require_known_beside(Beside::Nothing)
    }

    /// Refuse while an outcome is unknown. A command still running is not
    /// unknown: beside commands it is let through; otherwise it answers
    /// `busy`, held by an agent, so nobody is told to recover it.
    pub(crate) fn require_known_beside(&self, beside: Beside) -> Result<()> {
        if !self.marker.try_exists()? {
            return Ok(());
        }

        // A marker that cannot be read is an unknown outcome, as before.
        match (self.unsettled().unwrap_or(Unsettled::Unknown), beside) {
            (Unsettled::Nothing, _) | (Unsettled::Commands, Beside::Commands) => Ok(()),
            (Unsettled::Commands, Beside::Nothing) => {
                Err(busy_command(Purpose::Change(Beside::Nothing)))
            }
            (Unsettled::Unknown, _) => Err(Error::new(
                "uncertain",
                "The last action's outcome is unknown; Atlas waits until it is recovered.",
            )),
        }
    }

    /// What is pending, from one read under the marker lock. A marker with
    /// no entries is an unknown outcome, as an unreadable one is.
    fn unsettled(&self) -> Result<Unsettled> {
        let _lock = self.lock_marker()?;
        if !self.marker.try_exists()? {
            return Ok(Unsettled::Nothing);
        }

        let pending = pending(&self.marker)?;
        let marked = self.commands();
        let running = |generation: &String| self.running_command(&marked, generation);
        Ok(if !pending.is_empty() && pending.iter().all(running) {
            Unsettled::Commands
        } else {
            Unsettled::Unknown
        })
    }

    /// Refuse recovery while a pending change is still being carried out:
    /// a command an agent started, or an executor that outlived its caller.
    /// Recovery never settles either.
    fn require_nothing_running(&self) -> Result<()> {
        let _lock = self.lock_marker()?;
        let pending = pending(&self.marker)?;
        let marked = self.commands();
        if pending
            .iter()
            .any(|generation| self.running_command(&marked, generation))
        {
            return Err(busy_command(Purpose::Recovery));
        }
        if pending
            .iter()
            .any(|generation| self.in_progress(generation))
        {
            return Err(Error::new(
                "busy",
                "An Atlas executor is still carrying out an action. Wait for it to end before recovering.",
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

    /// Every pending generation, whether a live process is still carrying
    /// it out, and the host session that claimed it when one is recorded;
    /// from one locked read of the marker, or none when there is no marker.
    pub fn pending_causes(&self) -> Result<Vec<Cause>> {
        if !self.marker.try_exists()? {
            return Ok(Vec::new());
        }

        let _lock = self.lock_marker()?;
        let mut claimed = causes::read(&self.causes);
        let marked = self.commands();
        Ok(pending(&self.marker)?
            .into_iter()
            .map(|generation| Cause {
                running: self.in_progress(&generation),
                command: self.running_command(&marked, &generation),
                session: claimed.remove(&generation),
                generation,
            })
            .collect())
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
        self.pending_snapshot().map(|snapshot| snapshot.unresolved)
    }

    /// Unresolved generations, the total and the commands still running,
    /// from one locked marker read.
    pub(crate) fn pending_snapshot(&self) -> Result<Snapshot> {
        // Under the marker lock a finishing mutation cannot clear its entry
        // between reading the marker and probing its claim.
        let _lock = self.lock_marker()?;
        let pending = pending(&self.marker)?;
        let marked = self.commands();
        let total = pending.len();
        let commands = pending
            .iter()
            .filter(|generation| self.running_command(&marked, generation))
            .count();
        let unresolved = pending
            .into_iter()
            .filter(|generation| !self.in_progress(generation))
            .collect();

        Ok(Snapshot {
            unresolved,
            total,
            commands,
        })
    }

    /// The claim file for a generation; only plain tokens name one.
    fn live(&self, generation: &str) -> Option<PathBuf> {
        (!generation.is_empty() && generation.bytes().all(|b| b.is_ascii_hexdigit())).then(|| {
            self.directory
                .join(format!("{}.{generation}.live", self.name))
        })
    }

    /// Whether a live process holds this generation's claim file locked: a
    /// plain lock test, never an executor's record, so a claim kept only by
    /// an executor that outlived its caller is not held.
    fn held(&self, generation: &str) -> bool {
        self.open_claim(generation).is_some_and(|(_, file)| {
            FileExt::try_lock_shared(&file)
                .is_err_and(|error| error.kind() == std::io::ErrorKind::WouldBlock)
        })
    }

    /// Whether `generation` is a command an agent started with `run` that
    /// still runs: `marked` as one, and its claim held by a live process.
    fn running_command(&self, marked: &[String], generation: &str) -> bool {
        marked.iter().any(|entry| entry == generation) && self.held(generation)
    }

    /// The claim file for `generation`, and where it is, when there is one
    /// that opens without following a link.
    fn open_claim(&self, generation: &str) -> Option<(PathBuf, File)> {
        let live = self.live(generation)?;
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&live)
            .ok()?;
        Some((live, file))
    }

    /// The generations marked as commands agents started with `run`, as
    /// the list beside the marker names them; whether each still runs is
    /// the reader's to check.
    fn commands(&self) -> Vec<String> {
        commands::read(&self.commands)
    }

    /// Whether some live mutation, in any process, still holds the claim.
    fn in_progress(&self, generation: &str) -> bool {
        self.open_claim(generation).is_some_and(|(live, file)| {
            FileExt::try_lock_shared(&file).is_err() || claim_executor_running(&file, &live)
        })
    }

    /// Record a pending mutation this process is carrying out.
    pub fn quarantine(&self) -> Result<Pending> {
        self.quarantine_as(&token())
    }

    /// Record a command an agent started with `run`: its claim, its marker
    /// entry and its mark as a running command, so other agents may act
    /// beside it while it runs and recovery waits for it.
    pub(crate) fn quarantine_command(&self) -> Result<Pending> {
        self.quarantine_marked(&token(), Mark::Command)
    }

    /// Record a pending mutation under a generation the caller chose, so it
    /// can persist its own claim on that generation first. The claim is
    /// taken before the marker is written, so a marked generation is never
    /// briefly mistaken for an abandoned one. It is never refused after a
    /// signal: what settles or cleans up a change under way still records.
    pub fn quarantine_as(&self, generation: &str) -> Result<Pending> {
        self.quarantine_marked(generation, Mark::Change)
    }

    /// [`Controller::quarantine_as`], recorded as `mark` says.
    fn quarantine_marked(&self, generation: &str, mark: Mark) -> Result<Pending> {
        let hold = self.interrupt.claim();
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
            _hold: hold,
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

        // Marked before the marker names it: a mark the marker does not
        // name means nothing, and a command that cannot be marked never
        // starts. So many already run that another is refused.
        if mark == Mark::Command {
            let marked = self.commands();
            let running = pending
                .iter()
                .filter(|entry| self.running_command(&marked, entry))
                .count();
            if running >= RUNNING_COMMANDS_MAX {
                return Err(busy_running(format!(
                    "{RUNNING_COMMANDS_MAX} commands agents started are already running in the helper. Wait for one to end."
                )));
            }
            commands::mark(&self.commands, generation, &pending)?;
        }
        replace_private(&self.marker, &serde_json::to_vec(&pending)?)?;
        if let Some(label) = &self.label {
            // Whose change it is, for display only; a lost note loses a label.
            let _ = causes::note(&self.causes, generation, &label.key, &pending);
            if mark == Mark::Command
                && let Some(note) = Note::now(Some(label))
            {
                let _ = running::note(&self.running, generation, &note, &pending);
            }
        }
        Ok(claim)
    }

    /// Leave a command an agent started with `run` whose outcome became
    /// unknown as any other unknown outcome: its mark goes, then its claim,
    /// while its marker entry stays for recovery. It is never settled here.
    pub(crate) fn abandon_command(&self, claim: Pending) {
        if let Ok(_lock) = self.lock_marker() {
            let kept: Vec<String> = pending(&self.marker)
                .unwrap_or_default()
                .into_iter()
                .filter(|entry| entry != claim.generation())
                .collect();
            let _ = commands::keep(&self.commands, &kept);
            let _ = running::keep(&self.running, &kept);
        }
        drop(claim);
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
        let _ = causes::keep(&self.causes, &pending);
        let _ = commands::keep(&self.commands, &pending);
        let _ = running::keep(&self.running, &pending);

        if pending.is_empty() {
            remove_marker(&self.marker)
        } else {
            replace_private(&self.marker, &serde_json::to_vec(&pending)?)
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
        let replacing = format!(".{prefix}");

        for entry in fs::read_dir(&self.directory)?.flatten() {
            let name = entry.file_name();
            // Only a writer holding the marker lock has a replacement under
            // way, so one found now was left by a writer that was killed.
            if name
                .to_str()
                .is_some_and(|name| name.starts_with(&replacing) && name.ends_with(REPLACING))
            {
                let _ = fs::remove_file(entry.path());
                continue;
            }
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
        let _ = causes::keep(&self.causes, &running);
        let _ = commands::keep(&self.commands, &running);
        let _ = running::keep(&self.running, &running);

        if running.is_empty() {
            remove_marker(&self.marker)
        } else {
            replace_private(&self.marker, &serde_json::to_vec(&running)?)
        }
    }
}

/// The end of the name a replacement is written under before it is renamed
/// over the file it replaces.
const REPLACING: &str = ".replacing";

/// Replace the file at `path` with `bytes`, private to this account, so a
/// reader only ever sees the old file or the new one whole, and a writer
/// killed partway leaves the old one: the new one is written beside it,
/// synced, renamed over it, and the folder synced. Only one writer at a
/// time may replace a file, as the marker lock keeps them.
fn replace_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let (Some(directory), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(Error::new("state", "A record needs a folder and a name"));
    };
    let temporary = directory.join(format!(
        ".{}.{}{REPLACING}",
        name.to_string_lossy(),
        token()
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temporary)
        .map_err(|error| Error::writing(&temporary, error))?;

    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    #[cfg(test)]
    if commands_tests::INTERRUPT_REPLACE.with(|interrupt| {
        let stops = interrupt
            .get()
            .is_some_and(|ending| name.to_string_lossy().ends_with(ending));
        if stops {
            interrupt.set(None);
        }
        stops
    }) {
        // As a writer killed here would, it leaves the replacement unnamed.
        return Err(Error::new("state", "interrupted before the rename"));
    }
    let replaced = written
        .and_then(|()| fs::rename(&temporary, path))
        .and_then(|()| File::open(directory)?.sync_all());
    if replaced.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    replaced.map_err(|error| Error::writing(path, error))
}

/// A record kept beside the marker, one entry per pending generation.
trait Entries {
    /// Keep only the entries whose generations are still `pending`.
    fn retain_pending(&mut self, pending: &[String]);

    fn is_empty(&self) -> bool;
}

/// Keep only the entries of the record at `path`, read with `read` and
/// written with `write`, whose generations are still `pending`, removing it
/// once none is left. A missing record stays missing.
fn keep_entries<T: Entries>(
    path: &Path,
    pending: &[String],
    read: impl FnOnce(&Path) -> T,
    write: impl FnOnce(&Path, &T) -> Result<()>,
) -> Result<()> {
    if fs::symlink_metadata(path).is_err() {
        return Ok(());
    }

    let mut entries = read(path);
    entries.retain_pending(pending);
    if entries.is_empty() {
        return match fs::remove_file(path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        };
    }

    write(path, &entries)
}

/// The pending changes, as one locked read of the marker counts them.
#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    /// Generations no mutation in progress will settle.
    pub unresolved: Vec<String>,
    /// How many are pending in all.
    pub total: usize,
    /// How many are commands agents started with `run`, still running.
    pub commands: usize,
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

    /// What agents and people read gives the default wait the code uses.
    #[test]
    fn the_documented_default_wait_is_the_one_used() {
        let said = format!("up to {} s", CONTROL_WAIT.as_secs());
        for (file, text) in [
            (
                "limits.md",
                include_str!("../../agents/atlas/skills/atlas/references/limits.md"),
            ),
            (
                "troubleshooting.md",
                include_str!("../../agents/atlas/skills/atlas/references/troubleshooting.md"),
            ),
            ("docs/cli.md", include_str!("../../docs/cli.md")),
            (
                "docs/troubleshooting.md",
                include_str!("../../docs/troubleshooting.md"),
            ),
            (
                "docs/architecture.md",
                include_str!("../../docs/architecture.md"),
            ),
        ] {
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(text.contains(&said), "{file} does not say {said}");
        }
    }

    fn pending_counts(controller: &Controller) -> (usize, usize, usize) {
        let (pending, total) = crate::overview::pending(controller);
        (total, pending.unresolved, pending.in_flight)
    }

    #[test]
    fn reporting_pending_claims_never_settles_them_or_admits_control() {
        let directory = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, directory.path());
        let observer = Controller::at(505, directory.path());
        let report = |expected| {
            let before = fs::read(&controller.marker).ok();
            assert_eq!(pending_counts(&observer), expected);
            assert_eq!(fs::read(&controller.marker).ok(), before);
        };

        report((0, 0, 0));
        let abandoned = controller.quarantine().unwrap();
        let generation = abandoned.to_string();
        report((1, 0, 1));
        assert!(observer.unresolved().unwrap().is_empty());
        assert_eq!(observer.acquire(false).unwrap_err().code, "uncertain");

        drop(abandoned);
        report((1, 1, 0));
        assert_eq!(observer.unresolved().unwrap(), vec![generation.clone()]);
        assert_eq!(observer.acquire(false).unwrap_err().code, "uncertain");

        let working = controller.quarantine().unwrap();
        report((2, 1, 1));
        assert_eq!(observer.unresolved().unwrap(), vec![generation.clone()]);
        controller.clear_if(&working).unwrap();
        drop(working);
        report((1, 1, 0));
        assert_eq!(observer.pending().unwrap(), vec![generation]);
        assert_eq!(observer.acquire(false).unwrap_err().code, "uncertain");

        controller.clear().unwrap();
        report((0, 0, 0));
        assert!(observer.acquire(false).is_ok());
    }

    #[test]
    fn reporting_empty_legacy_and_malformed_markers_preserves_their_contents() {
        let directory = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, directory.path());
        for (contents, generations) in [
            ("[]", vec![]),
            ("not JSON", vec!["legacy-unknown"]),
            (
                r#"["1234.legacy","5678.legacy"]"#,
                vec!["1234.legacy", "5678.legacy"],
            ),
        ] {
            fs::write(&controller.marker, contents).unwrap();
            assert_eq!(
                pending_counts(&controller),
                (generations.len(), generations.len(), 0)
            );
            assert_eq!(controller.pending().unwrap(), generations);
            assert_eq!(controller.unresolved().unwrap(), generations);
            assert_eq!(fs::read_to_string(&controller.marker).unwrap(), contents);
            assert_eq!(controller.acquire(false).unwrap_err().code, "uncertain");
        }
    }

    #[test]
    fn unreadable_markers_and_unopenable_locks_are_reported_as_unresolved() {
        for marker_fails in [true, false] {
            let directory = tempfile::tempdir().unwrap();
            let controller = Controller::at(505, directory.path());
            let blocked = if marker_fails {
                &controller.marker
            } else {
                &controller.marker_lock
            };
            fs::create_dir(blocked).unwrap();
            assert!(controller.pending().is_err());
            assert!(controller.unresolved().is_err());
            assert_eq!(pending_counts(&controller), (1, 1, 0));
            assert!(blocked.is_dir());
        }
    }

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
        assert_eq!(pending_counts(&controller), (1, 0, 1));
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
        assert_eq!(refused.holder, Some(Holder::Person));
        assert!(refused.details.get("waited").is_none(), "never waited");
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
        let refused = controller.acquire(false).err().unwrap();
        assert_ne!(
            refused.error,
            crate::error::PERSON_HAS_CONTROL,
            "another agent is not the person"
        );
        assert_eq!(refused.holder, Some(Holder::Agent));
        drop(agent);

        fs::write(&controller.person, b"").unwrap();
        drop(controller.acquire(false).unwrap());
        assert!(!controller.person.exists(), "a stale note is cleared");
    }

    /// `controller` given `wait`, and what its hook hears, in order.
    fn watched(
        controller: &Controller,
        wait: Duration,
    ) -> (Controller, std::sync::mpsc::Receiver<WaitEvent>) {
        let (tell, events) = std::sync::mpsc::channel();
        let watched = controller.clone().waiting(wait).on_wait(move |event| {
            let _ = tell.send(event);
        });
        (watched, events)
    }

    /// The next thing a watched wait tells, within five seconds.
    fn told(events: &std::sync::mpsc::Receiver<WaitEvent>) -> WaitEvent {
        events.recv_timeout(Duration::from_secs(5)).unwrap()
    }

    /// Acquire on another thread, answering with when it returned.
    fn acquiring(
        controller: Controller,
        recovery: bool,
    ) -> std::thread::JoinHandle<(Result<()>, Instant)> {
        std::thread::spawn(move || {
            let outcome = controller.acquire(recovery).map(drop);
            (outcome, Instant::now())
        })
    }

    /// Wait for `path` to appear, for at most five seconds.
    fn appears(path: &Path) {
        let started = Instant::now();
        while !path.exists() {
            assert!(started.elapsed() < Duration::from_secs(5), "{path:?}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Stand in for another Atlas command, as the test `test` run again in
    /// a process of its own: it takes control of the helper kept in
    /// `directory`, marks a change and says so in `held`.
    fn holder(test: &str, directory: &Path, hold: Duration) -> std::process::Child {
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test, "--nocapture"])
            .env("ATLAS_CONTROL_TEST_HOLDER", directory)
            .env("ATLAS_CONTROL_TEST_HOLD_MS", hold.as_millis().to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap()
    }

    /// The other command [`holder`] starts, when this process is it: it
    /// keeps control and its change for the time it was given, settles the
    /// change, says so in `released` and exits still in control, so the
    /// lock goes only with the process.
    fn hold_if_holder() {
        let Some(directory) = std::env::var_os("ATLAS_CONTROL_TEST_HOLDER") else {
            return;
        };
        let directory = PathBuf::from(directory);
        let hold = std::env::var("ATLAS_CONTROL_TEST_HOLD_MS")
            .unwrap()
            .parse()
            .unwrap();
        let controller = Controller::at(505, &directory.join("state"));
        let ownership = controller.acquire(false).unwrap();
        let change = controller.quarantine().unwrap();
        fs::write(directory.join("held"), b"").unwrap();
        std::thread::sleep(Duration::from_millis(hold));
        controller.clear_if(&change).unwrap();
        drop(change);
        fs::write(directory.join("released"), b"").unwrap();
        std::mem::forget(ownership);
        std::process::exit(0);
    }

    #[test]
    fn waiting_for_control_takes_the_lock_once_the_holder_lets_go() {
        if !crate::transaction_test::isolated(
            "control::tests::waiting_for_control_takes_the_lock_once_the_holder_lets_go",
        ) {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, directory.path());
        let held = controller.acquire(false).unwrap();
        let (waiter, events) = watched(&controller, Duration::from_secs(5));
        let waiting = acquiring(waiter, false);

        assert_eq!(told(&events), WaitEvent::Began);
        std::thread::sleep(Duration::from_millis(300));
        let released = Instant::now();
        drop(held);
        let (outcome, taken) = waiting.join().unwrap();
        outcome.unwrap();
        assert!(taken >= released);
        assert!(taken - released < Duration::from_millis(500));
        assert_eq!(told(&events), WaitEvent::Ended);
    }

    #[test]
    fn two_processes_that_change_the_helper_both_succeed_one_after_the_other() {
        const TEST: &str =
            "control::tests::two_processes_that_change_the_helper_both_succeed_one_after_the_other";
        hold_if_holder();
        if !crate::transaction_test::isolated(TEST) {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, &directory.path().join("state"));
        let mut child = holder(TEST, directory.path(), Duration::from_millis(400));
        appears(&directory.path().join("held"));
        assert_eq!(controller.acquire(false).unwrap_err().code, "busy");

        let ownership = controller
            .clone()
            .waiting(Duration::from_secs(5))
            .acquire(false)
            .unwrap();
        assert!(
            directory.path().join("released").exists(),
            "control came only after the first command let go"
        );
        let change = controller.quarantine().unwrap();
        controller.clear_if(&change).unwrap();
        drop(change);
        drop(ownership);

        assert!(child.wait().unwrap().success());
        assert!(controller.pending().unwrap().is_empty());
    }

    #[test]
    fn waiting_gives_up_with_busy_after_its_wait_and_says_how_long() {
        if !crate::transaction_test::isolated(
            "control::tests::waiting_gives_up_with_busy_after_its_wait_and_says_how_long",
        ) {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, directory.path());
        let _held = controller.acquire(false).unwrap();
        let (waiter, events) = watched(&controller, Duration::from_secs(1));

        let started = Instant::now();
        let refused = waiter.acquire(false).unwrap_err();
        let waited = started.elapsed();
        assert!(waited >= Duration::from_secs(1), "{waited:?}");
        assert!(waited < Duration::from_millis(1500), "{waited:?}");
        assert_eq!(refused.code, "busy");
        assert_eq!(refused.holder, Some(Holder::Agent));
        assert!(
            refused.error.contains("all of the 1 seconds"),
            "{}",
            refused.error
        );
        assert_eq!(refused.details["waited"], 1);
        assert_eq!(refused.details["next"], crate::ui::hints::WAITED);
        assert_eq!(told(&events), WaitEvent::Began);
        assert_eq!(told(&events), WaitEvent::Ended);
    }

    /// A wait that used all of its time ends a little short of it, after
    /// what the command did first: it says as many seconds as its message.
    #[test]
    fn a_full_wait_says_the_seconds_its_message_does() {
        let refused = busy_agent(Some(Duration::from_millis(29_600)), Duration::from_secs(30));
        assert_eq!(refused.details["waited"], 30);
        assert!(
            refused.error.contains("all of the 30 seconds"),
            "{}",
            refused.error
        );

        assert_eq!(
            busy_person(Some(Duration::from_millis(1_400))).details["waited"],
            1
        );
        assert_eq!(seconds(Duration::from_millis(499)), 0);
        assert_eq!(seconds(Duration::from_millis(500)), 1);
        assert_eq!(seconds(Duration::MAX), u64::MAX);
    }

    #[test]
    fn one_wait_is_shared_by_every_acquire_of_a_command() {
        if !crate::transaction_test::isolated(
            "control::tests::one_wait_is_shared_by_every_acquire_of_a_command",
        ) {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, directory.path());
        let _held = controller.acquire(false).unwrap();
        let waiter = controller.clone().waiting(Duration::from_secs(1));

        assert_eq!(waiter.acquire(false).unwrap_err().code, "busy");
        assert_eq!(waiter.remaining(), Duration::ZERO);
        for again in [waiter.clone(), waiter] {
            let started = Instant::now();
            let refused = again.acquire(false).unwrap_err();
            assert!(started.elapsed() < Duration::from_millis(50));
            assert_eq!(refused.details["waited"], 0);
            assert_eq!(refused.details["next"], crate::ui::hints::WAITED);
        }
    }

    #[test]
    fn a_wait_of_zero_answers_busy_at_once_as_before() {
        if !crate::transaction_test::isolated(
            "control::tests::a_wait_of_zero_answers_busy_at_once_as_before",
        ) {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, directory.path());
        let _held = controller.acquire(false).unwrap();
        let (waiter, events) = watched(&controller, Duration::ZERO);

        let started = Instant::now();
        let refused = waiter.acquire(false).unwrap_err();
        assert!(started.elapsed() < Duration::from_millis(50));
        assert_eq!(refused.code, "busy");
        assert_eq!(refused.holder, Some(Holder::Agent));
        assert_eq!(
            refused.error,
            "Another Atlas controller is active: wait for it to finish, or release it in the viewer."
        );
        assert!(refused.details.is_empty(), "the reply is as it was");
        assert!(events.try_recv().is_err(), "nothing waited for");
    }

    #[test]
    fn the_person_in_control_is_never_waited_for() {
        if !crate::transaction_test::isolated(
            "control::tests::the_person_in_control_is_never_waited_for",
        ) {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, directory.path());
        let _person = controller.acquire_for_person().unwrap();
        let (waiter, events) = watched(&controller, Duration::from_secs(5));

        let started = Instant::now();
        let refused = waiter.acquire(false).unwrap_err();
        assert!(started.elapsed() < Duration::from_millis(50));
        assert_eq!(refused.code, "busy");
        assert_eq!(refused.error, crate::error::PERSON_HAS_CONTROL);
        assert_eq!(refused.holder, Some(Holder::Person));
        assert_eq!(refused.details["waited"], 0);
        assert!(events.try_recv().is_err(), "nothing waited for");
    }

    #[test]
    fn the_person_taking_control_during_a_wait_ends_it_at_once() {
        if !crate::transaction_test::isolated(
            "control::tests::the_person_taking_control_during_a_wait_ends_it_at_once",
        ) {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, directory.path());
        let _held = controller.acquire(false).unwrap();
        let (waiter, events) = watched(&controller, Duration::from_secs(5));
        let waiting = std::thread::spawn(move || {
            let refused = waiter.acquire(false).unwrap_err();
            (refused, Instant::now())
        });

        assert_eq!(told(&events), WaitEvent::Began);
        std::thread::sleep(Duration::from_millis(1100));
        let noted = Instant::now();
        fs::write(&controller.person, b"").unwrap();
        let (refused, answered) = waiting.join().unwrap();
        assert!(answered - noted < Duration::from_millis(300));
        assert_eq!(refused.code, "busy");
        assert_eq!(refused.holder, Some(Holder::Person));
        assert_eq!(refused.details["waited"], 1, "seen partway through");
        assert_eq!(told(&events), WaitEvent::Ended);
    }

    #[test]
    fn a_stop_noted_before_or_during_a_wait_ends_it_at_once() {
        if !crate::transaction_test::isolated(
            "control::tests::a_stop_noted_before_or_during_a_wait_ends_it_at_once",
        ) {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, directory.path());
        let held = controller.acquire(false).unwrap();
        let (waiter, events) = watched(&controller, Duration::from_secs(5));
        let waiter = waiter.heeding_stop();

        controller.note_stop(true).unwrap();
        let started = Instant::now();
        assert_eq!(waiter.acquire(false).unwrap_err().code, "stopped");
        assert!(started.elapsed() < Duration::from_millis(50));
        assert!(events.try_recv().is_err(), "nothing waited for");
        let refused = controller
            .clone()
            .heeding_stop()
            .acquire(false)
            .unwrap_err();
        assert_eq!(refused.code, "busy", "no wait, today's answer");
        controller.note_stop(false).unwrap();

        let waiting = std::thread::spawn(move || {
            let refused = waiter.acquire(false).unwrap_err();
            (refused, Instant::now())
        });
        assert_eq!(told(&events), WaitEvent::Began);
        let noted = Instant::now();
        controller.note_stop(true).unwrap();
        let (refused, answered) = waiting.join().unwrap();
        assert!(answered - noted < Duration::from_millis(300));
        assert_eq!(refused.code, "stopped");
        assert_eq!(refused.error, crate::error::STOPPED);
        assert_eq!(told(&events), WaitEvent::Ended);

        // What a Stop does not refuse, such as recovery, waits on as before.
        let (recovery, events) = watched(&controller, Duration::from_secs(5));
        let waiting = acquiring(recovery, true);
        assert_eq!(told(&events), WaitEvent::Began);
        drop(held);
        waiting.join().unwrap().0.unwrap();
    }

    #[test]
    fn a_stop_noted_as_the_holder_lets_go_refuses_the_control_a_wait_takes() {
        if !crate::transaction_test::isolated(
            "control::tests::a_stop_noted_as_the_holder_lets_go_refuses_the_control_a_wait_takes",
        ) {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, directory.path());
        let held = controller.acquire(false).unwrap();
        let (began, waits) = std::sync::mpsc::channel();
        let (go, gone) = std::sync::mpsc::channel::<()>();
        let gone = std::sync::Mutex::new(gone);
        // The waiter's next try comes only once the Stop is noted and the
        // lock let go, so that try takes it.
        let waiter = controller
            .clone()
            .waiting(Duration::from_secs(5))
            .heeding_stop()
            .on_wait(move |event| {
                if event == WaitEvent::Began {
                    began.send(()).unwrap();
                    gone.lock().unwrap().recv().unwrap();
                }
            });
        let waiting = acquiring(waiter, false);

        waits.recv_timeout(Duration::from_secs(5)).unwrap();
        controller.note_stop(true).unwrap();
        drop(held);
        go.send(()).unwrap();
        assert_eq!(waiting.join().unwrap().0.unwrap_err().code, "stopped");

        controller.note_stop(false).unwrap();
        assert!(
            controller.acquire(false).is_ok(),
            "the refused lock was let go"
        );
    }

    #[test]
    fn a_holder_killed_while_others_wait_hands_control_on_at_once() {
        const TEST: &str =
            "control::tests::a_holder_killed_while_others_wait_hands_control_on_at_once";
        hold_if_holder();
        if !crate::transaction_test::isolated(TEST) {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, &directory.path().join("state"));
        let mut child = holder(TEST, directory.path(), Duration::from_secs(30));
        appears(&directory.path().join("held"));

        let (waiter, events) = watched(&controller, Duration::from_secs(10));
        let waiting = acquiring(waiter, false);
        assert_eq!(told(&events), WaitEvent::Began);
        let killed = Instant::now();
        child.kill().unwrap();
        let (outcome, answered) = waiting.join().unwrap();
        assert!(answered - killed < Duration::from_millis(500));
        assert!(!child.wait().unwrap().success());
        assert_eq!(
            outcome.unwrap_err().code,
            "uncertain",
            "the killed command's change is unknown"
        );

        let ownership = controller.acquire(true).unwrap();
        controller.clear().unwrap();
        assert!(controller.pending().unwrap().is_empty());
        drop(ownership);
        assert!(controller.acquire(false).is_ok());
    }

    #[test]
    fn recovery_waits_for_the_lock_but_never_for_an_executor_still_running() {
        if !crate::transaction_test::isolated(
            "control::tests::recovery_waits_for_the_lock_but_never_for_an_executor_still_running",
        ) {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, directory.path());
        let held = controller.acquire(false).unwrap();
        let running = controller.quarantine().unwrap();
        let (recovery, events) = watched(&controller, Duration::from_secs(2));
        let waiting = std::thread::spawn(move || {
            let refused = recovery.acquire(true).unwrap_err();
            (refused, Instant::now())
        });

        assert_eq!(told(&events), WaitEvent::Began);
        std::thread::sleep(Duration::from_millis(300));
        let released = Instant::now();
        drop(held);
        let (refused, answered) = waiting.join().unwrap();
        assert!(answered >= released);
        assert_eq!(refused.code, "busy");
        assert!(
            refused.error.contains("executor is still carrying out"),
            "{}",
            refused.error
        );

        controller.clear_if(&running).unwrap();
        drop(running);
        let started = Instant::now();
        drop(
            controller
                .clone()
                .waiting(Duration::from_secs(2))
                .acquire(true)
                .unwrap(),
        );
        assert!(started.elapsed() < Duration::from_millis(50));
    }

    #[test]
    fn a_wait_tells_its_hook_when_it_begins_and_when_it_ends() {
        if !crate::transaction_test::isolated(
            "control::tests::a_wait_tells_its_hook_when_it_begins_and_when_it_ends",
        ) {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let controller = Controller::at(505, directory.path());
        let (waiter, events) = watched(&controller, Duration::from_secs(5));
        drop(waiter.acquire(false).unwrap());
        assert!(
            events.try_recv().is_err(),
            "taken at once, nothing waited for"
        );

        // Taken after a wait.
        let held = controller.acquire(false).unwrap();
        let waiting = acquiring(waiter, false);
        assert_eq!(told(&events), WaitEvent::Began);
        assert!(events.try_recv().is_err(), "still waiting");
        drop(held);
        let (outcome, _) = waiting.join().unwrap();
        outcome.unwrap();
        assert_eq!(told(&events), WaitEvent::Ended);

        // Refused once the wait ran out.
        let _held = controller.acquire(false).unwrap();
        let (waiter, events) = watched(&controller, Duration::from_millis(300));
        assert_eq!(waiter.acquire(false).unwrap_err().code, "busy");
        assert_eq!(told(&events), WaitEvent::Began);
        assert_eq!(told(&events), WaitEvent::Ended);
        assert!(events.try_recv().is_err(), "told once each");
    }

    #[test]
    fn a_poll_is_about_a_tenth_of_a_second_and_never_past_the_wait() {
        for jitter in 75..=125 {
            assert_eq!(
                poll_with(Duration::from_secs(1), jitter),
                Duration::from_millis(jitter)
            );
        }
        assert_eq!(
            poll_with(Duration::from_millis(30), 100),
            Duration::from_millis(30)
        );
        for _ in 0..100 {
            let slept = poll(Duration::from_secs(1));
            assert!(
                (Duration::from_millis(75)..=Duration::from_millis(125)).contains(&slept),
                "{slept:?}"
            );
        }
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
