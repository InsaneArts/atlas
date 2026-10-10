use crate::{
    config::{home, resolve_path},
    error::{Error, Result},
};
use fs2::FileExt;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

pub fn token() -> String {
    let mut bytes = [0; 24];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// Lock, pending-marker and marker-lock files for one exclusion key.
#[derive(Clone)]
struct Keyed {
    lock: PathBuf,
    marker: PathBuf,
    marker_lock: PathBuf,
}
impl Keyed {
    fn new(directory: &Path, key: &str) -> Self {
        let digest = format!("{:x}", Sha256::digest(key.as_bytes()));
        let name = &digest[..24];
        Self {
            lock: directory.join(format!("{name}.lock")),
            marker: directory.join(format!("{name}.uncertain.json")),
            marker_lock: directory.join(format!("{name}.state.lock")),
        }
    }
}

fn open_lock(path: &Path) -> Result<File> {
    Ok(OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?)
}

/// Exclusive control of one helper, held until dropped.
#[derive(Debug)]
pub struct Ownership {
    _locks: [File; 2],
}

/// Controller exclusion and pending-mutation state for one helper account.
/// It is keyed by helper UID, so a changed socket path cannot admit a second
/// controller. Controllers that predate this key used `uid:socket`; that lock
/// is taken and its marker honoured too, so they still exclude each other.
#[derive(Clone)]
pub struct Controller {
    directory: PathBuf,
    current: Keyed,
    legacy: Keyed,
}
impl Controller {
    pub fn new(uid: u32, socket: &Path) -> Result<Self> {
        Self::at(uid, socket, &home()?.join("Library/Caches/Understudy"))
    }
    /// Locating state is read-only; the directory appears with the first lock.
    pub fn at(uid: u32, socket: &Path, directory: &Path) -> Result<Self> {
        let legacy = format!("{uid}:{}", resolve_path(socket)?.display());
        Ok(Self {
            directory: directory.to_owned(),
            current: Keyed::new(directory, &format!("helper:{uid}")),
            legacy: Keyed::new(directory, &legacy),
        })
    }
    pub fn acquire(&self, recovery: bool) -> Result<Ownership> {
        fs::create_dir_all(&self.directory)?;
        let busy = || {
            Error::new(
                "busy",
                "Another Understudy controller is active. Release it first.",
            )
        };
        let current = open_lock(&self.current.lock)?;
        FileExt::try_lock_exclusive(&current).map_err(|_| busy())?;
        let legacy = open_lock(&self.legacy.lock)?;
        FileExt::try_lock_exclusive(&legacy).map_err(|_| busy())?;
        if !recovery {
            self.require_known()?;
        }
        Ok(Ownership {
            _locks: [current, legacy],
        })
    }
    pub fn require_known(&self) -> Result<()> {
        if self.current.marker.try_exists()? || self.legacy.marker.try_exists()? {
            return Err(Error::new(
                "uncertain",
                "The last action's outcome is unknown. Observe the desktop, then run 'understudy restart'.",
            ));
        }
        Ok(())
    }
    fn lock_marker(&self, keyed: &Keyed) -> Result<File> {
        fs::create_dir_all(&self.directory)?;
        let file = open_lock(&keyed.marker_lock)?;
        FileExt::lock_exclusive(&file)?;
        Ok(file)
    }
    pub fn quarantine(&self) -> Result<String> {
        let _lock = self.lock_marker(&self.current)?;
        let generation = token();
        let mut pending = self.pending()?;
        pending.push(generation.clone());
        write_private(&self.current.marker, &serde_json::to_vec(&pending)?)?;
        Ok(generation)
    }
    fn pending(&self) -> Result<Vec<String>> {
        match fs::read_to_string(&self.current.marker) {
            Ok(text) => {
                Ok(serde_json::from_str(&text).unwrap_or_else(|_| vec!["legacy-unknown".into()]))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error.into()),
        }
    }
    pub fn clear_if(&self, generation: &str) -> Result<()> {
        let _lock = self.lock_marker(&self.current)?;
        let mut pending = self.pending()?;
        pending.retain(|entry| entry != generation);
        if pending.is_empty() {
            remove_marker(&self.current.marker)
        } else {
            write_private(&self.current.marker, &serde_json::to_vec(&pending)?)
        }
    }
    /// Verified recovery settles every pending mutation, legacy ones included.
    pub fn clear(&self) -> Result<()> {
        for keyed in [&self.current, &self.legacy] {
            let _lock = self.lock_marker(keyed)?;
            remove_marker(&keyed.marker)?;
        }
        Ok(())
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
        if requested.is_some() && requested != current.map(String::as_str) {
            return Err(Error::new(
                "lease-lost",
                "This token cannot release another controller",
            ));
        }
        let previous = self.owner.take().or(self.retired.take());
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
        if let Ok(mut state) = self.lease.try_lock() {
            state.inflight = false;
            state.expire();
        } else {
            let lease = self.lease.clone();
            tokio::spawn(async move {
                let mut state = lease.lock().await;
                state.inflight = false;
                state.expire();
            });
        }
    }
}
