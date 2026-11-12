//! The helper's sound: whether its apps stay silent on the person's
//! speakers. One thread owns the mute and rebuilds it when audio processes
//! come or go or Core Audio restarts; replies and the hello read only what
//! it last reported, never Core Audio itself. The person's choice is kept in
//! the helper, so a new daemon mutes again before any agent acts.
use atlas_core::error::Result;
use atlas_native::native::audio::{
    self, Access, Backend, Change, CoreAudio, Engine, Listener, Refusal,
};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    time::{Duration, Instant},
};

/// How long changes to the audio processes settle before the mute is
/// rebuilt, so a burst of them rebuilds it once.
const SETTLE: Duration = Duration::from_millis(300);

/// How long a change of the setting waits for the mute to follow it.
const APPLY: Duration = Duration::from_secs(5);

/// Where the helper keeps whether its sound is off.
const NOTE: &str = "helper-sound";

/// Whether the helper's apps can be heard now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum State {
    Muted,
    On,
    Unavailable(Reason),
}

/// The person's choice, and what came of it: a reply says both, so a
/// switch shows the choice and the reason is its caption.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Report {
    /// Whether the person wants the helper silent, as the helper keeps it.
    pub(crate) wanted: bool,
    pub(crate) state: State,
}

/// Why the helper's apps are not as the person chose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reason {
    /// The person has not been asked yet.
    Unasked,
    Denied,
    /// macOS before 14.2 has no process taps.
    TooOld,
    Failed,
    /// Apps that started lately could not be muted yet; the others still are.
    Behind,
    /// An earlier mute could not be ended yet: it may still run.
    Ending,
}

impl Reason {
    /// What is wrong, and the one thing that fixes it.
    fn words(self) -> &'static str {
        match self {
            Self::Unasked => {
                "Atlas needs permission to keep the helper's apps silent. Switch to the helper \
                 and allow Atlas when macOS asks."
            }
            Self::Denied => {
                "Atlas may not keep the helper's apps silent. In the helper's System Settings › \
                 Privacy & Security › Screen & System Audio Recording, turn on Atlas."
            }
            Self::TooOld => "Keeping the helper's apps silent needs macOS 14.2 or later.",
            Self::Failed => "Atlas could not keep the helper's apps silent. Run atlas restart.",
            Self::Ending => {
                "Atlas is still turning off an earlier mute, so apps in the helper may stay \
                 silent; it keeps trying. Run atlas restart if this stays."
            }
            Self::Behind => {
                "Apps that just started in the helper may be heard: Atlas could not mute them \
                 yet. Run atlas restart if this stays."
            }
        }
    }
}

impl Report {
    /// The reply's fields: `helperSound`, why when unavailable, and the
    /// person's choice as `wanted`.
    pub(crate) fn json(self) -> Value {
        let (word, reason) = match self.state {
            State::Muted => ("muted", None),
            State::On => ("on", None),
            State::Unavailable(reason) => ("unavailable", Some(reason.words())),
        };
        json!({"helperSound": word, "reason": reason, "wanted": self.wanted})
    }
}

/// What the mute should be, from the person's wish, whether this macOS
/// has process taps, and whether the person allowed Atlas to record the
/// helper's audio. Only a yes starts anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Plan {
    Off,
    Mute,
    Unavailable(Reason),
}

pub(crate) fn plan(muted: bool, supported: bool, access: Access) -> Plan {
    match (muted, supported, access) {
        (false, _, _) => Plan::Off,
        (true, false, _) => Plan::Unavailable(Reason::TooOld),
        (true, true, Access::Granted) => Plan::Mute,
        (true, true, Access::Denied) => Plan::Unavailable(Reason::Denied),
        (true, true, Access::Undetermined) => Plan::Unavailable(Reason::Unasked),
    }
}

/// Whether to ask the person, now on screen in the helper, to let Atlas
/// record its audio: only for a wanted mute this macOS can make, only when
/// never answered, and only when this daemon is not about to restart (a
/// prompt whose process ends is lost).
pub(crate) fn asks(muted: bool, supported: bool, access: Access, restarting: bool) -> bool {
    muted && supported && access == Access::Undetermined && !restarting
}

/// Ask with `request` when [`asks`] says so: the permission is read afresh
/// first, and whether the helper is on screen last, right before asking, as
/// a prompt raised off screen becomes a lasting denial. Whether it asked.
fn ask_now(
    muted: bool,
    supported: bool,
    restarting: bool,
    access: impl FnOnce() -> Access,
    on_screen: impl FnOnce() -> bool,
    request: impl FnOnce() -> bool,
) -> bool {
    asks(muted, supported, access(), restarting) && on_screen() && request()
}

/// How many times in a row a failed rebuild is tried again.
const RETRIES: u32 = 5;

/// How long after its `failures`-th failure in a row a rebuild is tried
/// again: a second more each time, and never after [`RETRIES`].
fn retry_after(failures: u32) -> Option<Duration> {
    (1..=RETRIES)
        .contains(&failures)
        .then(|| Duration::from_secs(u64::from(failures)))
}

/// What the mute's thread is told.
enum Message {
    /// The person's wish, and where to say what came of it: none when it
    /// could not be saved, and nothing changed.
    Want(bool, Sender<Option<Report>>),
    /// Read the permission again.
    Check,
    /// Core Audio changed, in the generation it was then in.
    Changed(Change, u64),
}

/// What came of changing the helper's sound.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Set {
    /// Saved and applied: what the mute reports now.
    Done(Report),
    /// The choice could not be saved, so nothing changed.
    Unsaved,
    /// Not confirmed in time, or the mute's thread is gone: it may still
    /// happen, or may not have.
    Uncertain,
}

/// The daemon's handle on the helper's mute.
#[derive(Clone)]
pub(crate) struct Sound {
    report: Arc<Mutex<Report>>,
    muted: Arc<AtomicBool>,
    supported: bool,
    /// None for a daemon that never runs the mute (in tests).
    sender: Option<Sender<Message>>,
}

impl Sound {
    /// A handle on no mute at all.
    pub(crate) fn detached() -> Self {
        Self {
            report: Arc::new(Mutex::new(Report {
                wanted: true,
                state: State::Unavailable(Reason::Failed),
            })),
            muted: Arc::new(AtomicBool::new(true)),
            supported: false,
            sender: None,
        }
    }

    /// Start the mute's thread for the helper `uid`, with the wish kept in
    /// `home`, applied before this returns.
    pub(crate) fn start(uid: u32, home: Option<PathBuf>) -> Self {
        let note = home.map(|home| home.join(crate::helper_paths::ROOT).join(NOTE));
        let muted = note.as_deref().is_none_or(wanted);
        let supported = CoreAudio::new(uid, Arc::default()).is_some();
        let (sender, receiver) = mpsc::channel();
        let sound = Self {
            report: Arc::new(Mutex::new(Report {
                wanted: muted,
                state: State::Unavailable(Reason::Unasked),
            })),
            muted: Arc::new(AtomicBool::new(muted)),
            supported,
            sender: Some(sender.clone()),
        };

        let handle = sound.clone();
        // The mute and its listener stay on the thread that made them.
        let started = std::thread::Builder::new()
            .name("sound".into())
            .spawn(move || {
                let system = System {
                    uid,
                    generation: Arc::default(),
                };
                Driver::new(system, note, &handle, sender).run(&receiver);
            });
        if started.is_err()
            && let Ok(mut report) = sound.report.lock()
        {
            report.state = State::Unavailable(Reason::Failed);
        }
        sound
    }

    /// What the mute last reported. It never asks Core Audio.
    pub(crate) fn report(&self) -> Report {
        self.report.lock().map_or(
            Report {
                wanted: self.muted.load(Ordering::Relaxed),
                state: State::Unavailable(Reason::Failed),
            },
            |report| *report,
        )
    }

    /// Keep the wish to mute the helper or let it play, then apply it, and
    /// say what came of it: waits for the mute, briefly, and blocks while
    /// it does.
    pub(crate) fn set(&self, muted: bool) -> Set {
        let Some(sender) = &self.sender else {
            return Set::Done(self.report());
        };
        let (answer, answered) = mpsc::channel();
        if sender.send(Message::Want(muted, answer)).is_err() {
            return Set::Uncertain;
        }
        match answered.recv_timeout(APPLY) {
            Ok(Some(report)) => Set::Done(report),
            Ok(None) => Set::Unsaved,
            Err(_) => Set::Uncertain,
        }
    }

    /// Read the permission again: the person may have answered.
    pub(crate) fn check(&self) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(Message::Check);
        }
    }

    /// Ask the person, with macOS's prompt, to let Atlas record the
    /// helper's audio, when [`asks`] says so and `on_screen` still says the
    /// helper is on screen right before asking. The only way Atlas asks. It
    /// reads the permission, so it blocks. Whether macOS was asked.
    pub(crate) fn ask(&self, restarting: bool, on_screen: fn() -> bool) -> bool {
        let Some(sender) = self.sender.clone() else {
            return false;
        };
        ask_now(
            self.muted.load(Ordering::Relaxed),
            self.supported,
            restarting,
            audio::access,
            on_screen,
            // The thread may have ended with the daemon: nothing to tell then.
            move || audio::request_access(move || _ = sender.send(Message::Check)),
        )
    }
}

/// Whether the wish kept at `note` is silence: anything but "on", missing
/// or unreadable included.
fn wanted(note: &Path) -> bool {
    std::fs::read_to_string(note).map_or(true, |kept| kept.trim() != "on")
}

/// Keep the wish at `note`, replacing the file whole.
///
/// # Errors
///
/// Returns the failure to make its folder or write it; the kept wish is
/// then the one before.
fn remember(note: &Path, muted: bool) -> Result<()> {
    let wish: &[u8] = if muted { b"muted\n" } else { b"on\n" };
    atlas_core::files::save_private(note, wish)
}

/// What the mute's thread reaches outside itself: Core Audio and the
/// permission in Atlas, fakes in tests.
trait Audio {
    type Backend: Backend;
    type Listener;

    /// Process taps, or none on a macOS before 14.2.
    fn backend(&self) -> Option<Self::Backend>;

    /// The permission, read afresh without asking.
    fn access(&self) -> Access;

    /// Tell the thread at `sender` of Core Audio's changes until the
    /// listener is dropped, each with the generation it brought.
    fn listen(&self, sender: Sender<Message>) -> Result<Self::Listener>;
}

/// The helper's own Core Audio.
struct System {
    uid: u32,
    /// Core Audio's restarts, counted the moment it says so, before its
    /// message waits in line.
    generation: Arc<AtomicU64>,
}

impl Audio for System {
    type Backend = CoreAudio;
    type Listener = Listener;

    fn backend(&self) -> Option<CoreAudio> {
        CoreAudio::new(self.uid, self.generation.clone())
    }

    fn access(&self) -> Access {
        audio::access()
    }

    fn listen(&self, sender: Sender<Message>) -> Result<Listener> {
        let generation = self.generation.clone();
        audio::listen(move |change| {
            let now = match change {
                Change::Restarted => generation.fetch_add(1, Ordering::SeqCst) + 1,
                Change::Processes => generation.load(Ordering::SeqCst),
            };
            // The thread may have ended with the daemon: nothing to tell then.
            _ = sender.send(Message::Changed(change, now));
        })
    }
}

/// The mute's own thread: everything Core Audio is touched from here. The
/// engine holds the live mute, which runs only while the person wants the
/// helper silent and allows it, and the earlier mutes still being ended,
/// which are tried again on their own timer whatever the person chooses.
/// Core Audio is listened to while the person wants the helper silent.
struct Driver<A: Audio> {
    audio: A,
    note: Option<PathBuf>,
    report: Arc<Mutex<Report>>,
    muted: Arc<AtomicBool>,
    sender: Sender<Message>,
    /// None on a macOS without process taps.
    engine: Option<Engine<A::Backend>>,
    listener: Option<A::Listener>,
    /// What the live mute is, before an earlier one still being ended is
    /// counted in.
    state: State,
    /// When to rebuild the live mute for changes or after a failure.
    rebuild: Option<Instant>,
    /// Rebuilds that failed in a row.
    failures: u32,
}

impl<A: Audio> Driver<A> {
    fn new(audio: A, note: Option<PathBuf>, sound: &Sound, sender: Sender<Message>) -> Self {
        Self {
            engine: audio.backend().map(Engine::new),
            audio,
            note,
            report: sound.report.clone(),
            muted: sound.muted.clone(),
            sender,
            listener: None,
            state: State::Unavailable(Reason::Unasked),
            rebuild: None,
            failures: 0,
        }
    }

    fn run(mut self, receiver: &Receiver<Message>) {
        self.apply();
        loop {
            let message = match self.due() {
                Some(at) => {
                    match receiver.recv_timeout(at.saturating_duration_since(Instant::now())) {
                        Ok(message) => Some(message),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
                None => match receiver.recv() {
                    Ok(message) => Some(message),
                    Err(_) => break,
                },
            };
            self.handle(message);
        }
    }

    /// When something is due: a rebuild, or another try at ending an
    /// earlier mute.
    fn due(&self) -> Option<Instant> {
        let cleanup = self.engine.as_ref().and_then(Engine::next_retry);
        self.rebuild.into_iter().chain(cleanup).min()
    }

    fn wants(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    /// Act on one message; none when something fell due.
    fn handle(&mut self, message: Option<Message>) {
        match message {
            None => {
                let now = Instant::now();
                if let Some(engine) = &mut self.engine {
                    engine.sweep(now);
                }
                if self.rebuild.is_some_and(|at| at <= now) {
                    self.rebuild = None;
                    if self.wants() {
                        self.state = self.mute();
                    }
                }
                self.publish();
            }
            Some(Message::Want(muted, answer)) => {
                if let Some(note) = &self.note
                    && let Err(error) = remember(note, muted)
                {
                    eprintln!("{}", error.error);
                    // The asker may have stopped waiting: nothing changed.
                    let _ = answer.send(None);
                    return;
                }
                self.muted.store(muted, Ordering::Relaxed);
                // The asker may have stopped waiting; the change stands.
                let _ = answer.send(Some(self.apply()));
            }
            Some(Message::Check) => {
                self.apply();
            }
            // A live mute made before this restart is ended: only its
            // objects still proven ours are destroyed. One made since stays.
            // Listeners added before no longer hear Core Audio.
            Some(Message::Changed(Change::Restarted, generation)) => {
                if let Some(engine) = &mut self.engine {
                    engine.end_older(generation);
                }
                if self.wants() {
                    self.listener = None;
                    self.listen();
                    self.soon(SETTLE);
                }
            }
            // A new helper app is muted at once, so not even its first word
            // plays; one that ended can wait for others to settle.
            Some(Message::Changed(Change::Processes, _)) if self.wants() => {
                let added = self
                    .engine
                    .as_ref()
                    .is_some_and(|engine| engine.uncovered().unwrap_or(true));
                if added {
                    self.rebuild = None;
                    self.state = self.mute();
                    self.publish();
                } else {
                    self.soon(SETTLE);
                }
            }
            Some(Message::Changed(Change::Processes, _)) => {}
        }
    }

    /// Rebuild `after` from now, unless a rebuild is due sooner.
    fn soon(&mut self, after: Duration) {
        self.rebuild = Some(self.rebuild.unwrap_or_else(|| Instant::now() + after));
    }

    /// Say what the helper's sound is now: the live mute's state, unless
    /// the person wants sound on while an earlier mute may still run.
    fn publish(&self) -> Report {
        let wanted = self.wants();
        let ending = self
            .engine
            .as_ref()
            .is_some_and(|engine| engine.next_retry().is_some());
        let state = if !wanted && ending {
            State::Unavailable(Reason::Ending)
        } else {
            self.state
        };
        let report = Report { wanted, state };
        if let Ok(mut current) = self.report.lock() {
            *current = report;
        }
        report
    }

    /// Make the mute what the wish and the permission say now.
    fn apply(&mut self) -> Report {
        let wanted = self.wants();
        let access = if self.engine.is_some() {
            self.audio.access()
        } else {
            Access::Undetermined
        };
        if wanted {
            self.listen();
        } else {
            self.listener = None;
            self.rebuild = None;
        }
        self.state = match plan(wanted, self.engine.is_some(), access) {
            Plan::Mute => self.mute(),
            Plan::Off => {
                self.stop();
                State::On
            }
            Plan::Unavailable(reason) => {
                self.stop();
                State::Unavailable(reason)
            }
        };
        self.publish()
    }

    /// Mute the helper's processes, only while Core Audio is listened to:
    /// without its notices new apps would play under a mute said to hold.
    /// Listening that fails is tried again as a failed rebuild is, the mute
    /// already running kept meanwhile.
    fn mute(&mut self) -> State {
        self.listen();
        if self.listener.is_some() {
            return self.refresh();
        }
        self.retry();
        let running = self.engine.as_ref().is_some_and(Engine::running);
        State::Unavailable(if running {
            Reason::Behind
        } else {
            Reason::Failed
        })
    }

    /// Try the rebuild again a little later, a few times in a row.
    fn retry(&mut self) {
        self.failures += 1;
        match retry_after(self.failures) {
            Some(after) => self.soon(after),
            None => self.failures = 0,
        }
    }

    /// Listen for Core Audio's changes, unless already listening.
    fn listen(&mut self) {
        if self.listener.is_some() || self.engine.is_none() {
            return;
        }
        match self.audio.listen(self.sender.clone()) {
            Ok(listener) => self.listener = Some(listener),
            Err(error) => eprintln!("{}", error.error),
        }
    }

    /// End the live mute; what is left of it is ended later.
    fn stop(&mut self) {
        if let Some(engine) = &mut self.engine {
            engine.stop();
        }
    }

    /// Mute the processes there are now. A rebuild the permission no
    /// longer allows ends the mute; one Core Audio fails is tried again a
    /// few times, the mute already running kept meanwhile.
    fn refresh(&mut self) -> State {
        let Some(engine) = &mut self.engine else {
            return State::Unavailable(Reason::TooOld);
        };
        let refreshed = engine.refresh();
        let running = engine.running();
        match refreshed {
            Ok(()) => {
                self.failures = 0;
                State::Muted
            }
            Err(Refusal::Access(access)) => {
                self.failures = 0;
                match plan(true, true, access) {
                    Plan::Unavailable(reason) => State::Unavailable(reason),
                    Plan::Off | Plan::Mute => State::Unavailable(Reason::Failed),
                }
            }
            // Core Audio restarted under it: list the processes again soon.
            Err(Refusal::Stale) => {
                self.soon(SETTLE);
                State::Unavailable(Reason::Behind)
            }
            Err(Refusal::Failed(error)) => {
                eprintln!("{}", error.error);
                // A mute already running stays, but no longer covers every
                // process: say so until a rebuild does.
                let reason = if running {
                    Reason::Behind
                } else {
                    Reason::Failed
                };
                self.retry();
                State::Unavailable(reason)
            }
        }
    }
}

#[cfg(test)]
#[path = "sound_tests.rs"]
mod tests;
