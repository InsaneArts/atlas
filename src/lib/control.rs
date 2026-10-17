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
    fs::{self, File, OpenOptions},
    io::Write,
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
        if self.names_own_file() {
            let _ = fs::remove_file(&self.live);
        }
    }
}

/// Exclusive control of one helper, held until dropped.
#[derive(Debug)]
pub struct Ownership {
    _lock: File,
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
            name: name.to_owned(),
        }
    }

    /// Whether this process may keep its locks and claims here at all. An
    /// agent's sandbox may forbid writing, and the error then says so.
    pub fn writable(&self) -> Result<()> {
        probe_write(&self.directory)
    }

    pub fn acquire(&self, recovery: bool) -> Result<Ownership> {
        fs::create_dir_all(&self.directory)
            .map_err(|error| Error::writing(&self.directory, error))?;
        let lock = open_lock(&self.lock)?;
        FileExt::try_lock_exclusive(&lock).map_err(|_| {
            Error::new(
                "busy",
                "Another Atlas controller is active: wait for it to finish, or release it in the viewer.",
            )
        })?;
        if !recovery {
            self.require_known()?;
        }
        Ok(Ownership { _lock: lock })
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
        fs::create_dir_all(&self.directory)
            .map_err(|error| Error::writing(&self.directory, error))?;
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
            .open(live)
            .is_ok_and(|file| FileExt::try_lock_shared(&file).is_err())
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
        fs::create_dir_all(&self.directory)
            .map_err(|error| Error::writing(&self.directory, error))?;
        let file = open_lock(&live)?;
        FileExt::try_lock_exclusive(&file)
            .map_err(|_| Error::new("busy", "That generation is already being carried out"))?;
        let claim = Pending {
            generation: generation.to_owned(),
            live,
            lock: file,
        };
        let _lock = self.lock_marker()?;
        // Claim files are only unlinked under the marker lock or by their
        // holder, so if the path still names the locked file here, it will
        // until the claim is dropped.
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
            let _ = fs::remove_file(live);
        }
        let mut pending = pending(&self.marker)?;
        pending.retain(|entry| entry != generation);
        if pending.is_empty() {
            remove_marker(&self.marker)
        } else {
            write_private(&self.marker, &serde_json::to_vec(&pending)?)
        }
    }

    /// Verified recovery settles every pending mutation and removes every
    /// claim file no live mutation holds, listed or not.
    pub fn clear(&self) -> Result<()> {
        let _lock = self.lock_marker()?;
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
                let _ = fs::remove_file(entry.path());
            }
        }
        remove_marker(&self.marker)
    }
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
        self.descriptor = Some(controller.acquire(false)?);
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
// Holds the controller flock until an acknowledged or uncertain operation settles.
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
