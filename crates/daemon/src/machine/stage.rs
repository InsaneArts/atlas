//! Where a machine desktop keeps what the station stages into it: one
//! private folder, opened once and worked in only by descriptor. Each
//! stage is written whole into a staging folder of its own, every digest
//! checked, then put in place by one rename, its evidence inside; what
//! was cut short is put right from the disk when the daemon starts.
use crate::machine::changes::Changes;
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
    fs_move::{Replace, rename_within},
};
use nix::{
    fcntl::{OFlag, open},
    sys::stat::{Mode, SFlag, fstat},
    unistd::mkdir,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    ffi::CString,
    fs::File,
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

pub(crate) mod copies;
pub(crate) mod write;

/// How many update stages one Stop takes, enough to try an update again.
const STOPPED_UPDATES: usize = 3;

/// The operation a stage is kept under in the change records.
pub(crate) const OPERATION: &str = "stage";

/// The file inside a placed stage's folder that says which change put it
/// there, and what it holds.
pub(crate) const EVIDENCE: &str = ".atlas-stage";

/// What a placed stage says of itself: its change, and the request it was
/// asked with, which names what it holds and its digest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Evidence {
    pub(crate) change: Token,
    pub(crate) request: Value,
}

impl Evidence {
    /// The reply of the stage it is the evidence of, placed in `root`.
    pub(crate) fn reply(&self, root: &Root) -> Option<Value> {
        let name = self.request["name"].as_str()?;
        let digest = self.request["digest"].as_str()?;
        let plain = !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\0']);
        plain.then(|| reply(root, &self.change, name, digest))
    }
}

/// The guest's folder of staged copies, held open.
pub(crate) struct Root {
    pub(crate) path: PathBuf,
    pub(crate) folder: File,
}

impl Root {
    /// Open the folder at `path`, making it first if it is not there: a
    /// folder of this account's, private to it, with no ACL for anything in
    /// it to inherit.
    ///
    /// # Errors
    ///
    /// Refuses what is there when it is a link, not a folder, another
    /// account's, or not private.
    pub(crate) fn open(path: &Path) -> Result<Self> {
        match mkdir(path, Mode::S_IRWXU) {
            Ok(()) | Err(nix::errno::Errno::EEXIST) => {}
            Err(error) => return Err(io::Error::from(error).into()),
        }
        let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
        let folder = File::from(open(path, flags, Mode::empty()).map_err(io::Error::from)?);
        let stat = fstat(&folder).map_err(io::Error::from)?;
        let folder_kind = SFlag::from_bits_truncate(stat.st_mode & SFlag::S_IFMT.bits());
        let private = stat.st_uid == nix::unistd::getuid().as_raw()
            && u32::from(stat.st_mode) & 0o777 == 0o700;
        if folder_kind != SFlag::S_IFDIR || !private {
            return Err(Error::new(
                Code::State,
                "The folder for staged copies is not this account's own private folder",
            ));
        }
        atlas_core::acl::clear(&folder)?;
        Ok(Self {
            path: path.to_owned(),
            folder,
        })
    }

    /// Where `change`'s stage is placed.
    pub(crate) fn placed(&self, change: &Token, name: &str) -> PathBuf {
        self.path.join(change.as_str()).join(name)
    }
}

/// The name a stage is written under before it is placed.
pub(crate) fn staging(change: &Token) -> CString {
    named(&format!(".staging-{}", change.as_str()))
}

/// The name a placed stage has: its change.
pub(crate) fn placed(change: &Token) -> CString {
    named(change.as_str())
}

fn named(name: &str) -> CString {
    // A token is plain text with no NUL in it.
    CString::new(name).unwrap_or_default()
}

/// What putting a written stage in place came to.
#[derive(Debug)]
pub(crate) enum Placed {
    /// In place, and durable.
    Done,
    /// In place, but whether that survives a crash is unknown.
    Unsynced,
    /// Not moved: its staging folder is still there.
    NotMoved(Error),
}

/// Put `change`'s written stage in place, in one rename that never
/// replaces anything, and make that durable.
pub(crate) fn place(root: &Root, change: &Token) -> Placed {
    match rename_within(
        &root.folder,
        &staging(change),
        &placed(change),
        Replace::Never,
    ) {
        Ok(()) if root.folder.sync_all().is_ok() => Placed::Done,
        Ok(()) => Placed::Unsynced,
        Err(error) => Placed::NotMoved(error.into()),
    }
}

/// The reply a placed stage gives.
pub(crate) fn reply(root: &Root, change: &Token, name: &str, digest: &str) -> Value {
    json!({
        "ok": true,
        "path": root.placed(change, name),
        "digest": digest,
    })
}

/// The reply of a stage that placed nothing and left nothing behind.
pub(crate) fn nothing_placed(why: &str) -> Value {
    atlas_core::protocol::failure(Code::Cancelled, format!("Nothing was staged: {why}"))
}

/// What a placed stage's evidence says, when it is there and readable.
pub(crate) fn evidence(root: &Root, change: &Token) -> Option<Evidence> {
    let folder = write::open_folder(&root.folder, &placed(change)).ok()?;
    let read = write::read_small(&folder, EVIDENCE).ok()?;
    serde_json::from_slice(&read).ok()
}

/// Put right every stage a daemon before this one left intended, now that
/// none can be running. One placed, with evidence naming its change and
/// the very request it was asked with, came to its path; one whose staging
/// folder is removed, or was never made, came to nothing placed. Either is
/// made durable before it is kept. Anything else (a folder that cannot be
/// looked at, evidence that does not match, a removal or a sync that
/// fails) stays intended, to be tried at the next start.
///
/// # Errors
///
/// Fails when the change records cannot be read.
pub(crate) fn recover<V: crate::machine::vault::Vault>(
    root: &Root,
    changes: &Changes<V>,
) -> Result<()> {
    for intended in changes.intended()? {
        if intended.operation != OPERATION {
            continue;
        }
        if let Some(reply) = came_to(root, &intended) {
            changes.done(&intended.change, &reply)?;
        }
    }
    Ok(())
}

/// What the intended stage `intended` came to, judged from the disk and
/// made durable: none when that cannot be told.
fn came_to(root: &Root, intended: &crate::machine::changes::Intended) -> Option<Value> {
    let change = &intended.change;
    let reply = if write::exists(&root.folder, &placed(change)).ok()? {
        // Placed: what it came to is only what its own evidence says.
        let evidence = evidence(root, change)?;
        let asked = crate::machine::changes::request_digest(&evidence.request).ok()?;
        if evidence.change != *change || asked != intended.request {
            return None;
        }
        evidence.reply(root)?
    } else {
        write::discard(&root.folder, &staging(change)).ok()?;
        nothing_placed("the daemon ended before it was placed")
    };
    root.folder.sync_all().ok()?;
    Some(reply)
}

/// Stages running at once; more are refused as busy.
const AT_ONCE: usize = 4;

/// Room a stage leaves free on the guest's volume, past what it needs.
const MARGIN: u64 = 2 << 30;

/// Room reserved for each entry's own metadata.
const PER_ENTRY: u64 = 4096;

/// A machine desktop's stages: its root, once opened and put right at
/// start, and the stages running in it.
pub struct Stages {
    path: PathBuf,
    root: std::sync::OnceLock<Root>,
    /// Held while a stage is looked up, kept as intended and registered,
    /// so one change never starts twice.
    pub(crate) admitting: tokio::sync::Mutex<()>,
    running: std::sync::Mutex<std::collections::HashMap<Token, tokio::sync::watch::Receiver<bool>>>,
    /// The copies being removed now, one removal each.
    removals: Arc<std::sync::Mutex<std::collections::HashSet<Token>>>,
    permits: Arc<tokio::sync::Semaphore>,
    reserved: Arc<std::sync::Mutex<u64>>,
    committing: Arc<std::sync::atomic::AtomicUsize>,
    /// The Stop the update stages taken while agents were stopped came in,
    /// and how many: no copy is removed then, so they are bounded.
    stopped_updates: std::sync::Mutex<(u64, usize)>,
    /// Where a test holds the next stage right before it is authorized:
    /// told once it is there, and let go when told.
    #[cfg(test)]
    hold: std::sync::Mutex<Option<Hold>>,
}

/// A test's hold on a stage: told once it is held, let go when told.
#[cfg(test)]
type Hold = (
    tokio::sync::oneshot::Sender<()>,
    tokio::sync::oneshot::Receiver<()>,
);

/// A stage registered as running: dropped, it is done, and whoever waits
/// for it is told.
pub(crate) struct Running {
    change: Token,
    running: Arc<Stages>,
    done: tokio::sync::watch::Sender<bool>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.running
            .running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.change);
        self.done.send_replace(true);
    }
}

/// A copy taken for one removal: dropped, it is free again.
pub(crate) struct Claimed {
    token: Token,
    removals: Arc<std::sync::Mutex<std::collections::HashSet<Token>>>,
}

impl Drop for Claimed {
    fn drop(&mut self) {
        self.removals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.token);
    }
}

/// Room on the guest's volume held for one stage until it ends.
pub(crate) struct Reserved {
    bytes: u64,
    reserved: Arc<std::sync::Mutex<u64>>,
}

impl Drop for Reserved {
    fn drop(&mut self) {
        let mut reserved = self
            .reserved
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *reserved = reserved.saturating_sub(self.bytes);
    }
}

/// A stage authorized to commit: counted until it is done, so a Resume
/// waits for what it came to.
pub(crate) struct Committing(Arc<std::sync::atomic::AtomicUsize>);

impl Drop for Committing {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Stages {
    /// Stages kept in the folder at `path`, opened when the daemon starts.
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            root: std::sync::OnceLock::new(),
            admitting: tokio::sync::Mutex::new(()),
            running: std::sync::Mutex::default(),
            removals: Arc::default(),
            permits: Arc::new(tokio::sync::Semaphore::new(AT_ONCE)),
            reserved: Arc::default(),
            committing: Arc::default(),
            stopped_updates: std::sync::Mutex::default(),
            #[cfg(test)]
            hold: std::sync::Mutex::new(None),
        }
    }

    /// Take one of the few update stages a Stop numbered `stops` allows:
    /// whether one was left.
    pub(crate) fn take_stopped_update(&self, stops: u64) -> bool {
        let mut taken = self
            .stopped_updates
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if taken.0 != stops {
            *taken = (stops, 0);
        }
        if taken.1 >= STOPPED_UPDATES {
            return false;
        }
        taken.1 += 1;
        true
    }

    /// Hold the next stage right before it is authorized: what says it is
    /// held, and what lets it go.
    #[cfg(test)]
    pub(crate) fn hold_before_commit(
        &self,
    ) -> (
        tokio::sync::oneshot::Receiver<()>,
        tokio::sync::oneshot::Sender<()>,
    ) {
        let (reached, held) = tokio::sync::oneshot::channel();
        let (release, released) = tokio::sync::oneshot::channel();
        *self
            .hold
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((reached, released));
        (held, release)
    }

    /// Wait here while a test holds this stage.
    #[cfg(test)]
    pub(crate) async fn held_before_commit(&self) {
        let hold = self
            .hold
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some((reached, released)) = hold {
            let _ = reached.send(());
            let _ = released.await;
        }
    }

    /// Open the root and put right what a daemon before this one left in
    /// it, before any stage is served.
    ///
    /// # Errors
    ///
    /// Fails when the root cannot be opened or the records read.
    pub(crate) fn start<V: crate::machine::vault::Vault>(
        &self,
        changes: &Changes<V>,
    ) -> Result<()> {
        let root = Root::open(&self.path)?;
        recover(&root, changes)?;
        copies::recover(&root, changes)?;
        let _ = self.root.set(root);
        Ok(())
    }

    /// The root, once it is open and put right.
    pub(crate) fn root(&self) -> Option<&Root> {
        self.root.get()
    }

    /// Whether any stage is committing: a Resume waits for what it came to.
    pub(crate) fn committing(&self) -> bool {
        self.committing.load(std::sync::atomic::Ordering::SeqCst) > 0
    }

    /// Count one more stage committing, until what it hands back is dropped.
    pub(crate) fn commit(&self) -> Committing {
        self.committing
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Committing(self.committing.clone())
    }

    /// The stage of `change` running now, if one is: told once it is done.
    pub(crate) fn running(&self, change: &Token) -> Option<tokio::sync::watch::Receiver<bool>> {
        self.running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(change)
            .cloned()
    }

    /// Register `change`'s stage as running, under [`Stages::admitting`].
    pub(crate) fn register(self: &Arc<Self>, change: &Token) -> Running {
        let (done, waiting) = tokio::sync::watch::channel(false);
        self.running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(change.clone(), waiting);
        Running {
            change: change.clone(),
            running: self.clone(),
            done,
        }
    }

    /// Every token work is registered on now: stages running and copies
    /// being removed.
    pub(crate) fn busy(&self) -> Vec<Token> {
        let mut busy: Vec<Token> = self
            .running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .keys()
            .cloned()
            .collect();
        busy.extend(
            self.removals
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .cloned(),
        );
        busy
    }

    /// Whether `token`'s copy is being removed now.
    pub(crate) fn removing(&self, token: &Token) -> bool {
        self.removals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(token)
    }

    /// Take `token`'s copy for one removal, under [`Stages::admitting`]: none
    /// while it is being removed or staged already.
    pub(crate) fn claim(&self, token: &Token) -> Option<Claimed> {
        if self.running(token).is_some() {
            return None;
        }
        let mut removals = self
            .removals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        removals.insert(token.clone()).then(|| Claimed {
            token: token.clone(),
            removals: self.removals.clone(),
        })
    }

    /// A place among the stages running at once, if one is free.
    pub(crate) fn place_among(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        self.permits.clone().try_acquire_owned().ok()
    }

    /// Hold `bytes` of the guest's volume, when that much is free past the
    /// margin and what every running stage holds already.
    pub(crate) fn reserve(&self, entries: u64, bytes: u64) -> Option<Reserved> {
        let needed = bytes.checked_add(entries.checked_mul(PER_ENTRY)?)?;
        let stats = nix::sys::statvfs::statvfs(&self.path).ok()?;
        let free = u64::from(stats.blocks_available()).checked_mul(stats.fragment_size())?;
        let mut reserved = self
            .reserved
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let wanted = reserved.checked_add(needed)?.checked_add(MARGIN)?;
        if wanted > free {
            return None;
        }
        *reserved += needed;
        Some(Reserved {
            bytes: needed,
            reserved: self.reserved.clone(),
        })
    }
}

#[cfg(test)]
#[path = "stage_tests.rs"]
mod tests;
