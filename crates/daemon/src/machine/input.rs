//! The daemon's end of a machine desktop's input service: started as its
//! own child, one at a time, and started again when it ends; told over its
//! channel which generation gives control and until when, and whether
//! agents are held, each word acknowledged. What is not acknowledged yet is
//! unsettled, and no Resume passes over it.
use atlas_core::error::{Code, Error, Result};
use atlas_native::native::service::channel::{Connection, Frame, Generation};
use atlas_session::authority::Authority;
use std::{
    collections::{BTreeSet, HashMap},
    ffi::OsString,
    os::fd::OwnedFd,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    net::unix::OwnedReadHalf,
    sync::{mpsc, watch},
};

/// How long a generation's authority holds past each renewal.
const LEASE: Duration = Duration::from_secs(2);
/// How often the live generation is renewed.
const RENEWAL: Duration = Duration::from_millis(500);
/// Frames waiting to be written that say something to the service.
const CONTROL: usize = 64;
/// Frames of data waiting to be written, across every connection.
const DATA: usize = 256;
/// The longest one frame may take to write before the channel is lost.
const WRITING: Duration = Duration::from_secs(1);
/// The longest a service that ended may take to be gone, before another is
/// started.
const GONE: Duration = Duration::from_secs(10);
/// The longest a service has to end on its own once its channel ends.
const ENDING: Duration = Duration::from_secs(5);
/// The longest wait before a service that ended is started again.
const BACKOFF: Duration = Duration::from_secs(30);

/// How the daemon starts an input service: a program, and the words it is
/// started with.
#[derive(Clone, Debug)]
pub struct Starter {
    program: PathBuf,
    arguments: Vec<OsString>,
}

impl Starter {
    /// This very program's hidden input command, keeping its files in
    /// `folder`.
    ///
    /// # Errors
    ///
    /// Returns the error of finding this program.
    pub fn atlas(folder: &Path) -> std::io::Result<Self> {
        Ok(Self::new(
            std::env::current_exe()?,
            vec![
                "machine-input".into(),
                "--folder".into(),
                folder.as_os_str().to_owned(),
            ],
        ))
    }

    pub fn new(program: PathBuf, arguments: Vec<OsString>) -> Self {
        Self { program, arguments }
    }

    /// This test binary, running only the test that is an input service
    /// when given a folder after `--`.
    #[cfg(test)]
    pub(crate) fn of_these_tests(folder: &Path) -> Self {
        Self::new(
            std::env::current_exe().expect("the test binary"),
            vec![
                "--exact".into(),
                "machine::input::tests::an_input_service_runs_when_started_for_these_tests".into(),
                "--test-threads=1".into(),
                "-q".into(),
                "--".into(),
                folder.as_os_str().to_owned(),
            ],
        )
    }
}

/// How the input service stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standing {
    /// Started, not yet saying it is ready.
    Starting,
    /// Saying it is ready.
    Ready,
    /// Ended, with nothing it held left held.
    Gone,
    /// Ended with input still held as its journal says, or an earlier one
    /// will not go: agents stay held until a new one recovers it.
    CleanupUnknown,
}

/// The daemon's end of its input service.
pub struct Input {
    inner: Arc<Inner>,
}

struct Inner {
    folder: PathBuf,
    /// The channel to the service running now.
    link: Mutex<Option<Arc<Link>>>,
    /// The daemon's newest word on the hold: its number, and whether held.
    word: Mutex<(u64, bool)>,
    /// The number of the newest word the service acknowledged.
    acknowledged: AtomicU64,
    /// The generation that gives control now, and its authority.
    live: Mutex<Option<(Generation, Authority)>>,
    /// Revoked generations whose work the service has not said is over.
    revoked: Mutex<BTreeSet<Generation>>,
    standing: watch::Sender<Standing>,
    /// Set once the daemon is done with its input service.
    ending: watch::Sender<bool>,
    /// Held while the service stops counting as ready, and while a Resume
    /// finds input settled and lifts the hold: the two never cross.
    turning: Mutex<()>,
    /// Set the moment the running service's channel is known lost, before
    /// anything else is done about it.
    lost: std::sync::atomic::AtomicBool,
    /// The deadline the last renewal gave, on the continuous clock.
    renewed: Mutex<Option<(Generation, u64)>>,
    /// Why the last service ended before it was ready, as it said itself;
    /// none once one is ready.
    failure: Mutex<Option<Failure>>,
}

/// What an input service said as it failed to start: its code and words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Failure {
    pub(crate) code: String,
    pub(crate) error: String,
}

impl Failure {
    /// The failure the last line of `said` gives, when it is one Atlas
    /// writes.
    fn of(said: &[u8]) -> Option<Self> {
        let line = said
            .split(|byte| *byte == b'\n')
            .rev()
            .find(|line| !line.is_empty())?;
        let failed: serde_json::Value = serde_json::from_slice(line).ok()?;
        Some(Self {
            code: failed["code"].as_str()?.to_owned(),
            error: failed["error"].as_str()?.to_owned(),
        })
    }
}

impl Inner {
    /// Say the service stands as `standing`, under the lock its failure is
    /// kept under, so a snapshot never pairs one service's standing with
    /// another's failure.
    fn stand(&self, standing: Standing) {
        let _failure = self.failure.lock().unwrap_or_else(PoisonError::into_inner);
        self.standing.send_replace(standing);
    }

    /// Say the service is ready: no failure from then.
    fn ready(&self) {
        let mut failure = self.failure.lock().unwrap_or_else(PoisonError::into_inner);
        *failure = None;
        self.standing.send_replace(Standing::Ready);
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        self.inner.ending.send_replace(true);
    }
}

/// One running service's channel: what to write on it, and its connections.
struct Link {
    control: mpsc::Sender<Frame>,
    data: mpsc::Sender<Frame>,
    next: AtomicU32,
    connections: Mutex<HashMap<Connection, mpsc::UnboundedSender<Vec<u8>>>>,
    /// Told when the channel is lost.
    lost: Arc<tokio::sync::Notify>,
    /// The daemon's end it belongs to, told of the loss at once.
    inner: std::sync::Weak<Inner>,
    /// Set to end the channel cleanly: the service ends as on its own.
    closing: watch::Sender<bool>,
    /// Set once this channel is lost: nothing is opened on it after.
    lost_here: std::sync::atomic::AtomicBool,
}

/// One connection opened to the service: what it says, and how to write to
/// it. Dropping it closes it.
pub struct Opened {
    link: Arc<Link>,
    connection: Connection,
    pub(crate) said: mpsc::UnboundedReceiver<Vec<u8>>,
}

impl Opened {
    /// Send `bytes` on the connection.
    ///
    /// # Errors
    ///
    /// Fails once the service is gone.
    pub(crate) async fn send(&self, bytes: &[u8]) -> Result<()> {
        for chunk in bytes.chunks(atlas_native::native::service::channel::PAYLOAD) {
            self.link
                .data
                .send(Frame::Data(self.connection, chunk.to_vec()))
                .await
                .map_err(|_| gone())?;
        }
        Ok(())
    }
}

impl Drop for Opened {
    fn drop(&mut self) {
        self.link
            .connections
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.connection);
        self.link.say(Frame::Close(self.connection));
    }
}

impl Input {
    /// Start the service `starter` names, and keep one running, its files
    /// in `folder`.
    pub fn start(starter: Starter, folder: PathBuf) -> Self {
        let inner = Arc::new(Inner {
            folder,
            link: Mutex::new(None),
            word: Mutex::new((0, false)),
            acknowledged: AtomicU64::new(0),
            live: Mutex::new(None),
            revoked: Mutex::new(BTreeSet::new()),
            standing: watch::Sender::new(Standing::Starting),
            ending: watch::Sender::new(false),
            turning: Mutex::new(()),
            lost: std::sync::atomic::AtomicBool::new(false),
            renewed: Mutex::new(None),
            failure: Mutex::new(None),
        });
        tokio::spawn(keep_running(inner.clone(), starter));
        tokio::spawn(renew(inner.clone()));
        Self { inner }
    }

    /// How the service stands now, with why the last one ended before it
    /// was ready, read together, so the two belong to one moment.
    pub(crate) fn snapshot(&self) -> (Standing, Option<Failure>) {
        // Every change of standing is made under this lock too.
        let failure = self
            .inner
            .failure
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        (*self.inner.standing.borrow(), failure.clone())
    }

    /// How the service stands now, and as it changes.
    pub fn standing(&self) -> watch::Receiver<Standing> {
        self.inner.standing.subscribe()
    }

    /// `generation` gives control from now, while `authority` holds; the
    /// one before is revoked.
    pub fn live(&self, generation: Generation, authority: Authority) {
        // Revoked under the lock renewing takes, so no renewal of it is
        // said after.
        let mut live = self
            .inner
            .live
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some((before, _)) = live.replace((generation, authority.clone())) {
            self.inner.revoke(before);
        }
        drop(live);
        // Revoked the moment its authority ends, not at the next renewal.
        let (inner, watched) = (self.inner.clone(), authority.clone());
        tokio::spawn(async move {
            watched.ended().await;
            inner.retire(generation);
        });
        self.inner.renew_now();
    }

    /// Hold agents' input, or lift the hold: the word goes at once, ahead
    /// of any data, and is unsettled until acknowledged. Called where the
    /// Stop or Resume it carries is decided, so the words go in order.
    pub fn hold(&self, on: bool) {
        let word = self.inner.next_word(on);
        self.inner.say_hold(word);
    }

    /// The daemon's newest word on the hold: its number, and whether held.
    #[cfg(test)]
    pub(crate) fn word(&self) -> (u64, bool) {
        *self
            .inner
            .word
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Run `reopen` and lift the hold, only while input is settled, with the
    /// service unable to stop counting as ready meanwhile: whether it ran.
    pub fn lift_if_settled(&self, reopen: impl FnOnce()) -> bool {
        let unqueued = {
            let _turning = self
                .inner
                .turning
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if self.unsettled() {
                return false;
            }
            let Some(link) = self.inner.link() else {
                return false;
            };
            let (seq, on) = self.inner.next_word(false);
            if link.control.try_send(Frame::Hold { seq, on }).is_ok() {
                reopen();
                return true;
            }
            // Not even queued: still held, as a service started again is
            // told, and the Resume is not carried out.
            self.inner.next_word(true);
            link
        };
        // Lost once the lock is let go, as losing takes it too.
        unqueued.lose();
        false
    }

    /// Whether anything said to the service is not acknowledged: the hold,
    /// a revoked generation's work, or a service that is not ready.
    pub fn unsettled(&self) -> bool {
        let (seq, _) = *self
            .inner
            .word
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.inner.lost.load(Ordering::SeqCst)
            || seq > self.inner.acknowledged.load(Ordering::SeqCst)
            || !self
                .inner
                .revoked
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_empty()
            || *self.inner.standing.borrow() != Standing::Ready
    }

    /// Open a connection to the service for `generation`.
    ///
    /// # Errors
    ///
    /// Fails while no service runs.
    pub fn open(&self, generation: Generation) -> Result<Opened> {
        let link = self
            .inner
            .link
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or_else(gone)?;
        link.open(generation)
    }
}

impl Inner {
    /// The daemon's next word on the hold.
    fn next_word(&self, on: bool) -> (u64, bool) {
        let mut word = self.word.lock().unwrap_or_else(PoisonError::into_inner);
        *word = (word.0 + 1, on);
        *word
    }

    fn link(&self) -> Option<Arc<Link>> {
        self.link
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn say_hold(&self, (seq, on): (u64, bool)) {
        if let Some(link) = self.link() {
            link.say(Frame::Hold { seq, on });
        }
    }

    fn revoke(&self, generation: Generation) {
        self.revoked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(generation);
        if let Some(link) = self.link() {
            link.say(Frame::Revoke(generation));
        }
    }

    /// Renew the live generation now, or revoke it once its authority ended.
    fn renew_now(&self) {
        // Held through the renewal, which revoking takes too: a generation
        // revoked is never renewed after.
        let live = self.live.lock().unwrap_or_else(PoisonError::into_inner);
        let Some((generation, authority)) = live.clone() else {
            return;
        };
        let link = self.link();
        // Never past when the authority itself runs out: the clock is read
        // before its lease, so time passing between only makes this earlier,
        // as a heartbeat since only makes the lease longer.
        let expires = continuous()
            .zip(authority.remaining())
            .map(|(now, left)| now + nanos(left));
        // Said only while the authority holds, under the lock retiring takes.
        let renewed = authority.while_live(|| {
            // Read again here: a pause before this moment counts.
            let now = continuous().ok_or_else(gone)?;
            let deadline = expires.ok_or_else(gone)?.min(now + nanos(LEASE));
            // The service retires a generation whose deadline passed, for
            // good: a daemon stalled past it gives that one up too.
            let mut renewed = self.renewed.lock().unwrap_or_else(PoisonError::into_inner);
            let stalled = matches!(*renewed, Some((renewing, before))
                if renewing == generation && now >= before);
            if stalled {
                return Err(gone());
            }
            *renewed = Some((generation, deadline));
            if let Some(link) = &link {
                link.say(Frame::Renew(generation, deadline));
            }
            Ok(())
        });
        drop(live);
        if renewed.is_err() {
            authority.retire(atlas_session::authority::Retired::Lapsed);
            self.retire(generation);
        }
    }

    /// Revoke `generation` if it is the live one: its authority ended.
    fn retire(&self, generation: Generation) {
        let mut live = self.live.lock().unwrap_or_else(PoisonError::into_inner);
        if live.as_ref().is_some_and(|(live, _)| *live == generation) {
            *live = None;
            self.revoke(generation);
        }
    }
}

impl Link {
    /// Say `frame` ahead of any data: a word that cannot even be queued
    /// loses the channel, and the service is started again and told what
    /// is still to be said.
    fn say(self: &Arc<Self>, frame: Frame) {
        if self.control.try_send(frame).is_err() {
            self.lose();
        }
    }

    /// The channel is lost: unsettled from this moment, then torn down.
    fn lose(self: &Arc<Self>) {
        self.lost_here.store(true, Ordering::SeqCst);
        if let Some(inner) = self.inner.upgrade() {
            // Under the lock a Resume lifts the hold under, and only for the
            // service running now: a channel lost before touches nothing.
            let _turning = inner.turning.lock().unwrap_or_else(PoisonError::into_inner);
            self.if_current(&inner, || inner.lost.store(true, Ordering::SeqCst));
        }
        // Every connection waiting on it hears its end.
        self.connections
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        self.lost.notify_one();
    }

    /// Run `then` only while this is the daemon's current channel, under the
    /// lock a new channel is put in place under.
    fn if_current(self: &Arc<Self>, inner: &Inner, then: impl FnOnce()) {
        let current = inner.link.lock().unwrap_or_else(PoisonError::into_inner);
        if current
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, self))
        {
            then();
        }
    }

    fn open(self: &Arc<Self>, generation: Generation) -> Result<Opened> {
        // Numbered, kept and queued under one lock, so connections reach the
        // service in the order of their numbers.
        let mut connections = self
            .connections
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if self.lost_here.load(Ordering::SeqCst) {
            return Err(gone());
        }
        let connection = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        let (telling, said) = mpsc::unbounded_channel();
        if self
            .control
            .try_send(Frame::Open(connection, generation))
            .is_err()
        {
            drop(connections);
            self.lose();
            return Err(gone());
        }
        connections.insert(connection, telling);
        drop(connections);
        Ok(Opened {
            link: self.clone(),
            connection,
            said,
        })
    }
}

/// Renew the live generation every [`RENEWAL`], ending with the daemon.
async fn renew(inner: Arc<Inner>) {
    let mut every = tokio::time::interval(RENEWAL);
    let mut ending = inner.ending.subscribe();
    loop {
        tokio::select! {
            _ = every.tick() => inner.renew_now(),
            _ = ending.wait_for(|ending| *ending) => return,
        }
    }
}

/// Keep one service running: started once no earlier one is left, told the
/// hold and the live generation first, judged ready by its hello, and
/// started again with a growing wait once it ends.
async fn keep_running(inner: Arc<Inner>, starter: Starter) {
    let mut wait = Duration::from_secs(1);
    let mut ending = inner.ending.subscribe();
    while !*ending.borrow() {
        if !earlier_gone(&starter).await {
            inner.stand(Standing::CleanupUnknown);
        } else if let Ok(ended) = run_one(&inner, &starter).await {
            let left = atlas_native::native::service::held_input(&inner.folder) != Some(false);
            inner.stand(if left {
                Standing::CleanupUnknown
            } else {
                Standing::Gone
            });
            if ended {
                wait = Duration::from_secs(1);
            }
        }
        tokio::select! {
            () = tokio::time::sleep(wait) => {}
            _ = ending.wait_for(|ending| *ending) => return,
        }
        wait = (wait * 2).min(BACKOFF);
    }
}

/// Run one service until it ends: whether it got as far as ready.
async fn run_one(inner: &Arc<Inner>, starter: &Starter) -> Result<bool> {
    let (ours, theirs) = atlas_core::spawn::socket_pair()?;
    let mut command = tokio::process::Command::new(&starter.program);
    command
        .args(&starter.arguments)
        .stdin(Stdio::from(OwnedFd::from(theirs)))
        .stdout(Stdio::null())
        // What it says as it fails, in the words Atlas writes, kept.
        .stderr(Stdio::piped())
        .env("ATLAS_JSON", "1")
        .kill_on_drop(true);
    let mut child = atlas_core::spawn::spawn_async(&mut command)?;
    drop(command);
    // Read to its end, so a service that writes much never waits on it;
    // only the last of it kept.
    let said = child.stderr.take().map(|mut stderr| {
        tokio::spawn(async move {
            let mut said = Vec::new();
            let mut chunk = [0_u8; 4096];
            while let Ok(read) = tokio::io::AsyncReadExt::read(&mut stderr, &mut chunk).await {
                if read == 0 {
                    break;
                }
                said.extend_from_slice(&chunk[..read]);
                let over = said.len().saturating_sub(SAID);
                said.drain(..over);
            }
            said
        })
    });

    ours.set_nonblocking(true)?;
    let (reading, writing) = tokio::net::UnixStream::from_std(ours)?.into_split();
    let (control, controls) = mpsc::channel(CONTROL);
    let (data, datas) = mpsc::channel(DATA);
    let lost = Arc::new(tokio::sync::Notify::new());
    let (closing, closed) = watch::channel(false);
    let link = Arc::new(Link {
        control,
        data,
        next: AtomicU32::new(0),
        connections: Mutex::new(HashMap::new()),
        lost: lost.clone(),
        inner: Arc::downgrade(inner),
        closing,
        lost_here: std::sync::atomic::AtomicBool::new(false),
    });
    tokio::spawn(write(writing, controls, datas, link.clone(), closed));
    tokio::spawn(read(reading, inner.clone(), link.clone()));
    inner.stand(Standing::Starting);
    // A new service holds nothing an earlier one did, and knows nothing
    // of what was said to it: the hold first, then the live generation.
    let earlier = {
        // Put in place with nothing acknowledged, under the lock every
        // acknowledgement is counted under.
        let mut current = inner.link.lock().unwrap_or_else(PoisonError::into_inner);
        inner.acknowledged.store(0, Ordering::SeqCst);
        inner.lost.store(false, Ordering::SeqCst);
        *current = Some(link.clone());
        // Revoked before this service: their work went with the one that
        // had it.
        inner
            .revoked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    };
    let word = *inner.word.lock().unwrap_or_else(PoisonError::into_inner);
    if word.0 > 0 {
        inner.say_hold(word);
    }
    inner.renew_now();
    let mut ending = inner.ending.subscribe();
    let ready = tokio::select! {
        ready = ready(&link) => ready,
        () = lost.notified() => false,
        _ = child.wait() => false,
        _ = ending.wait_for(|ending| *ending) => false,
    };
    if ready {
        // What earlier services held went with them, recovered by this one;
        // what was revoked since stays until this one settles it.
        inner
            .revoked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|revoked| !earlier.contains(revoked));
        inner.ready();
        tokio::select! {
            () = lost.notified() => {}
            _ = child.wait() => {}
            _ = ending.wait_for(|ending| *ending) => {}
        }
    }
    // Lost: no longer ready from this moment, before anything else.
    {
        let _turning = inner.turning.lock().unwrap_or_else(PoisonError::into_inner);
        inner.stand(Standing::Gone);
    }
    *inner.link.lock().unwrap_or_else(PoisonError::into_inner) = None;
    // Its channel ends cleanly first, so it ends as it does on its own:
    // cancelling, joining and releasing what it held. Killed only when it
    // does not, which its journal then tells.
    link.closing.send_replace(true);
    if tokio::time::timeout(ENDING, child.wait()).await.is_err() {
        let _ = child.kill().await;
    }
    if !ready
        && let Some(said) = said
        && let Ok(Ok(said)) = tokio::time::timeout(ENDING, said).await
        && let Some(failure) = Failure::of(&said)
    {
        *inner.failure.lock().unwrap_or_else(PoisonError::into_inner) = Some(failure);
    }
    Ok(ready)
}

/// The most a service's failure is read of.
const SAID: usize = 16 * 1024;

/// Whether the service says it is ready, asked until it does.
async fn ready(link: &Arc<Link>) -> bool {
    for _ in 0..60 {
        let Ok(mut opened) = link.open(0) else {
            return false;
        };
        if opened.send(b"{\"op\":\"hello\"}\n").await.is_err() {
            return false;
        }
        let mut said = Vec::new();
        let replied = tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(bytes) = opened.said.recv().await {
                said.extend(bytes);
                if said.ends_with(b"\n") {
                    break;
                }
            }
        })
        .await;
        let hello: Option<serde_json::Value> = serde_json::from_slice(&said).ok();
        if replied.is_ok() && hello.is_some_and(|hello| hello["ready"] == true) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    false
}

/// Read what the service says: acknowledgements, and its connections' data.
async fn read(mut reading: OwnedReadHalf, inner: Arc<Inner>, link: Arc<Link>) {
    while let Ok(frame) = Frame::read(&mut reading).await {
        match frame {
            // Counted only from the service running now, never a late word
            // from one lost before.
            Frame::Held { seq, .. } => link.if_current(&inner, || {
                inner.acknowledged.fetch_max(seq, Ordering::SeqCst);
            }),
            Frame::Revoked(_) => {}
            Frame::Settled(generation) => link.if_current(&inner, || {
                inner
                    .revoked
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .retain(|revoked| *revoked > generation);
            }),
            Frame::Data(connection, bytes) => {
                let connections = link
                    .connections
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                if let Some(telling) = connections.get(&connection) {
                    let _ = telling.send(bytes);
                }
            }
            Frame::Close(connection) => {
                link.connections
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(&connection);
            }
            Frame::Open(..) | Frame::Renew(..) | Frame::Revoke(_) | Frame::Hold { .. } => break,
        }
    }
    link.lose();
}

/// Write frames to the service, control first, each within [`WRITING`].
async fn write(
    mut writing: tokio::net::unix::OwnedWriteHalf,
    mut controls: mpsc::Receiver<Frame>,
    mut datas: mpsc::Receiver<Frame>,
    link: Arc<Link>,
    mut closed: watch::Receiver<bool>,
) {
    use tokio::io::AsyncWriteExt;
    loop {
        let frame = tokio::select! {
            biased;
            // It only ever turns to closed.
            _ = closed.changed() => {
                let _ = writing.shutdown().await;
                return;
            }
            Some(frame) = controls.recv() => frame,
            Some(frame) = datas.recv() => frame,
            else => return,
        };
        let written = tokio::time::timeout(WRITING, frame.write(&mut writing)).await;
        if !matches!(written, Ok(Ok(()))) {
            link.lose();
            return;
        }
    }
}

/// Whether no earlier input service `starter` started is left, waiting up to
/// [`GONE`] for one to finish ending.
async fn earlier_gone(starter: &Starter) -> bool {
    let started = std::time::Instant::now();
    loop {
        let starter = starter.clone();
        let left = tokio::task::spawn_blocking(move || serving(&starter))
            .await
            .unwrap_or(true);
        if !left {
            return true;
        }
        if started.elapsed() >= GONE {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Whether a process of this account is an input service `starter` started:
/// its program, by the name the kernel keeps, and its very words. One with
/// that name whose words cannot be read counts as one, as does an account
/// whose processes cannot be listed.
fn serving(starter: &Starter) -> bool {
    let uid = nix::unistd::getuid().as_raw();
    let Ok(processes) = atlas_core::process::of_user(uid) else {
        return true;
    };
    let name: String = starter
        .program
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let words: Vec<String> = starter
        .arguments
        .iter()
        .map(|word| word.to_string_lossy().into_owned())
        .collect();
    processes
        .iter()
        // The kernel keeps the first bytes of a program's name.
        .filter(|process| {
            !process.zombie && !process.name.is_empty() && name.starts_with(&process.name)
        })
        .any(|process| {
            atlas_core::process::arguments(process.instance.pid).map_or(true, |arguments| {
                arguments.get(1..) == Some(words.as_slice())
            })
        })
}

/// Now on the continuous clock, in nanoseconds.
fn continuous() -> Option<u64> {
    let now = nix::time::clock_gettime(nix::time::ClockId::CLOCK_MONOTONIC).ok()?;
    let seconds = u64::try_from(now.tv_sec()).ok()?;
    let nanos = u64::try_from(now.tv_nsec()).ok()?;
    seconds.checked_mul(1_000_000_000)?.checked_add(nanos)
}

fn nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

fn gone() -> Error {
    Error::new(Code::Native, "The desktop's input service is not running")
}

#[cfg(test)]
#[path = "input_tests.rs"]
pub(crate) mod tests;
