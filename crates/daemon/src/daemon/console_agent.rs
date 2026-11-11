//! An Apple agent that works only for the login on the screen, kept paused
//! in the helper.
//!
//! Off screen it quits a few milliseconds after it starts, still busy, and
//! launchd starts an agent of its kind again whenever it quits that way:
//! every ten seconds, for as long as the helper stays logged in. Each of
//! those short lives opens a window server connection, and a macOS bug
//! keeps some of them for good, which slowly slows the whole Mac. launchd
//! honours no disable for it. What ends the restarts is one start that
//! never finishes: the agent is paused the moment it starts, and launchd,
//! which sees it running, starts no other.
//!
//! It runs again when the helper comes on screen, where it works, and
//! whenever the daemon stops. launchd's starts, and the moment one becomes
//! the agent, come as notices from the kernel; once a second the keeper
//! looks at where the helper is and at the process it paused. One thread
//! does all of it, and every signal goes through the kernel's own token for
//! the very process that was looked at, so no other is ever reached.
//!
//! Nothing is written down: a paused agent is itself the record. A daemon
//! that starts takes over one it finds paused, keeps it while the helper is
//! hidden and lets it run on screen.
use atlas_core::process::{self, Info, Instance};
use atlas_native::native::{
    process_token::Token,
    process_watch::{Event, Watch},
};
use nix::sys::signal::Signal;
use serde_json::{Value, json};
use std::{
    cell::Cell,
    collections::HashSet,
    fmt::Display,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

/// The agent's program: launchd's `com.apple.BTServer.cloudpairing`, the
/// one `atlas doctor` warns of while it keeps restarting.
const PROGRAM: &str = "/System/Library/CoreServices/audioaccessoryd";

/// launchd, which starts every agent.
const LAUNCHD: i32 = 1;

/// How old a start may be when it is paused. What launchd starts becomes
/// the agent within about 17 ms, and the agent was first seen to reach
/// another process about 29 ms after that. A start this young is paused
/// before then, so no other process is left waiting on it; an older one is
/// left to quit as it does.
const YOUNG: Duration = Duration::from_millis(25);

/// How often the helper's place and the paused agent are looked at.
const LOOK: Duration = Duration::from_secs(1);

/// How long a process launchd started is watched to become the agent.
const BECOMING: Duration = Duration::from_secs(5);

/// How many times a signalled process is looked at for the signal to show,
/// [`SETTLE`] apart.
const LOOKS: u32 = 20;

/// How long between those looks.
const SETTLE: Duration = Duration::from_micros(250);

/// How many more times a keeper that is ending continues an agent not yet
/// seen to run again.
const LAST_TRIES: u32 = 20;

/// How long a stopping daemon waits, each time it asks, for the keeper to
/// end.
const LET_GO: Duration = Duration::from_secs(2);

/// What is done about the agent now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    /// Nothing: this daemon keeps no agent, or its keeper has ended.
    Off,
    /// The helper is on screen, or may be: the agent runs as macOS has it.
    OnScreen,
    /// The helper is hidden: launchd's next start of the agent is awaited.
    Watching,
    /// The helper is hidden and the agent is paused.
    Paused,
    /// The agent can't be paused: the keeper did not start, or the kernel
    /// will not tell launchd's starts, which is asked for again. The agent
    /// restarts as it did.
    Unavailable,
}

/// What the keeper last did, as the hello says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Report {
    state: State,
    /// Starts paused since this daemon began.
    pauses: u64,
    /// Starts that were seen too late to pause, or would not pause.
    misses: u64,
}

impl Report {
    const OFF: Self = Self {
        state: State::Off,
        pauses: 0,
        misses: 0,
    };

    /// The hello's fields.
    pub(crate) fn json(self) -> Value {
        let state = match self.state {
            State::Off => "off",
            State::OnScreen => "onScreen",
            State::Watching => "watching",
            State::Paused => "paused",
            State::Unavailable => "unavailable",
        };
        json!({"state": state, "pauses": self.pauses, "misses": self.misses})
    }
}

/// Starts the keeper's thread, which says what it does through the report
/// it is given; none when it could not be started.
type Starter = Box<dyn Fn(Arc<Mutex<Report>>) -> Option<Running> + Send + Sync>;

/// The daemon's handle on the agent's keeper. Dropping it ends the keeper,
/// which lets the agent run again.
pub(crate) struct ConsoleAgent {
    report: Arc<Mutex<Report>>,
    /// How the keeper is started; none for a daemon that keeps no agent
    /// (in tests).
    starter: Option<Starter>,
    /// The keeper's thread, from its start until it is known to have ended.
    keeper: Mutex<Option<Running>>,
    /// How long each stop waits for the keeper to end.
    patience: Duration,
}

/// The keeper's thread, as the handle reaches it.
struct Running {
    stopping: Arc<AtomicBool>,
    /// Ends the thread's wait, so it sees `stopping` at once.
    wake: Box<dyn Fn() + Send + Sync>,
    /// Whether the agent was seen to run again, said as the thread ends;
    /// closed without a word by a thread that panicked.
    ended: Receiver<bool>,
    thread: JoinHandle<()>,
}

/// Ends the keeper when dropped, so the agent runs again however the scope
/// that started it is left: its end, an early return or a panic.
#[must_use = "the keeper is ended when this is dropped"]
pub(crate) struct Keeping<'a>(&'a ConsoleAgent);

impl Drop for Keeping<'_> {
    fn drop(&mut self) {
        self.0.stop();
    }
}

impl ConsoleAgent {
    /// A handle that keeps no agent.
    pub(crate) fn detached() -> Self {
        Self {
            report: Arc::new(Mutex::new(Report::OFF)),
            starter: None,
            keeper: Mutex::new(None),
            patience: LET_GO,
        }
    }

    /// A handle for the helper `uid`, on screen when `on_console` says so
    /// or cannot say. Nothing runs until [`ConsoleAgent::start`].
    pub(crate) fn of(uid: u32, on_console: fn() -> bool) -> Self {
        Self::with(
            Box::new(move |report| Running::keeping(uid, on_console, report)),
            LET_GO,
        )
    }

    /// A handle whose keeper `starter` starts, and whose every stop waits
    /// `patience` for it to end.
    fn with(starter: Starter, patience: Duration) -> Self {
        Self {
            report: Arc::new(Mutex::new(Report::OFF)),
            starter: Some(starter),
            keeper: Mutex::new(None),
            patience,
        }
    }

    /// Start the keeper's thread, once; it is ended when what this returns
    /// is dropped. A keeper that cannot start leaves the agent as macOS has
    /// it, and the hello says so.
    pub(crate) fn start(&self) -> Keeping<'_> {
        if let Some(starter) = &self.starter {
            let mut keeper = self.keeper.lock().unwrap_or_else(PoisonError::into_inner);
            if keeper.is_none() {
                *keeper = starter(self.report.clone());
                if keeper.is_none()
                    && let Ok(mut report) = self.report.lock()
                {
                    report.state = State::Unavailable;
                }
            }
        }
        Keeping(self)
    }

    /// End the keeper, which lets the agent run again: waits for it,
    /// briefly, and blocks while it does. A keeper that has not ended by
    /// then is waited for again by the next stop; nothing when it has.
    pub(crate) fn stop(&self) {
        let mut keeper = self.keeper.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(running) = keeper.as_ref() else {
            return;
        };
        running.stopping.store(true, Ordering::SeqCst);
        (running.wake)();
        let let_go = match running.ended.recv_timeout(self.patience) {
            Ok(let_go) => let_go,
            Err(RecvTimeoutError::Disconnected) => false,
            Err(RecvTimeoutError::Timeout) => {
                eprintln!("The agent macOS keeps restarting is not let go yet");
                return;
            }
        };
        if !let_go {
            eprintln!(
                "The agent macOS keeps restarting may still be paused; Atlas takes it over when it next starts"
            );
        }
        // It has said its last, or panicked: the thread is at its end. A
        // panic was told above, and says nothing more here.
        if let Some(running) = keeper.take() {
            let _ = running.thread.join();
        }
    }

    /// What the keeper last did. It never looks at a process.
    pub(crate) fn report(&self) -> Report {
        self.report.lock().map_or(Report::OFF, |report| *report)
    }
}

impl Drop for ConsoleAgent {
    fn drop(&mut self) {
        self.stop();
        // A keeper that has not ended is asked once more. One stuck in a
        // system call is waited for no longer, as the daemon waits for no
        // blocking work without limit at its end: the next daemon takes
        // over what it left paused.
        self.stop();
    }
}

impl Running {
    /// A thread that runs `keep` until told to stop: `keep` is given the
    /// flag that tells it, which `wake` makes it look at, and answers
    /// whether the agent was seen to run again.
    ///
    /// # Errors
    ///
    /// Returns the system's refusal to start a thread.
    fn spawn(
        wake: impl Fn() + Send + Sync + 'static,
        keep: impl FnOnce(&AtomicBool) -> bool + Send + 'static,
    ) -> std::io::Result<Self> {
        let stopping = Arc::new(AtomicBool::new(false));
        let (ending, ended) = mpsc::channel();
        let stop = stopping.clone();
        let thread = std::thread::Builder::new()
            .name("console-agent".into())
            .spawn(move || {
                // Nobody waits any more when the handle has given up.
                let _ = ending.send(keep(&stop));
            })?;
        Ok(Self {
            stopping,
            wake: Box::new(wake),
            ended,
            thread,
        })
    }

    /// The keeper's thread for the helper `uid`; none when the kernel gives
    /// no watch or no token for this process, or no thread starts.
    fn keeping(uid: u32, on_console: fn() -> bool, report: Arc<Mutex<Report>>) -> Option<Self> {
        // Only calls and their status codes are in these causes.
        let unavailable = |why: &str, cause: &dyn std::fmt::Display| {
            eprintln!("The agent macOS keeps restarting can't be paused: {why} ({cause})");
        };
        let watch = match Watch::new() {
            Ok(watch) => Arc::new(watch),
            Err(cause) => {
                unavailable("the kernel gave no process watch", &cause);
                return None;
            }
        };
        // The agent's login is this process's own.
        let own = i32::try_from(std::process::id())
            .map_err(std::io::Error::other)
            .and_then(Token::of);
        let session = match own {
            Ok(token) => token.session(),
            Err(cause) => {
                unavailable("this login's session is unknown", &cause);
                return None;
            }
        };

        let waker = watch.clone();
        Self::spawn(
            move || waker.wake(),
            move |stopping| {
                let mut keeper = Keeper::new(Mac::new(on_console, watch), uid, session, report);
                keeper.run(stopping);
                keeper.end()
            },
        )
        .map_err(|cause| unavailable("its thread did not start", &cause))
        .ok()
    }
}

/// What the kernel says of one PID now.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Seen {
    Found(Info),
    /// No process has that PID.
    Gone,
    /// The kernel did not say: nothing is concluded from it.
    Unknown,
}

/// One process as an exact look found it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Exact {
    /// The kernel's name for this very process.
    token: Token,
    /// The program it runs.
    program: PathBuf,
    /// How long ago it had started, by the kernel's own count, at `read`
    /// at the latest.
    age: Duration,
    /// When its age was read.
    read: Instant,
}

/// What an exact look makes of one PID.
#[derive(Debug, PartialEq, Eq)]
enum Is {
    /// The agent in this login.
    Agent(Info, Exact),
    /// Another process, or none.
    Other,
    /// The kernel did not say enough to tell.
    Unknown,
}

/// What the keeper reaches outside itself: the kernel's word on processes
/// and its notices of them in the helper, fakes in tests.
trait System {
    /// Whether the helper is known to be off screen.
    fn hidden(&self) -> bool;

    fn now(&self) -> Instant;

    /// The processes `parent` has started that still run; none when the
    /// kernel does not list them.
    fn children(&self, parent: i32) -> Option<Vec<i32>>;

    /// Every process of the account `uid`; none when the kernel does not
    /// list them.
    fn processes(&self, uid: u32) -> Option<Vec<Info>>;

    fn process(&self, pid: i32) -> Seen;

    /// `pid` by its token, with what is read through it; none when the
    /// kernel does not give it all for one and the same process.
    fn exact(&self, pid: i32) -> Option<Exact>;

    /// Whether `signal` was sent to the very process `token` names.
    fn signal(&self, token: &Token, signal: Signal) -> bool;

    /// Give a signal a moment to show.
    fn settle(&self);

    /// Whether each start by `parent` is told from now on.
    fn watch_starts(&self, parent: i32) -> bool;

    /// Whether `pid` becoming another program, and ending, is told from
    /// now on.
    fn watch(&self, pid: i32) -> bool;

    fn forget(&self, pid: i32);

    /// The kernel's notices, waiting at most `longest` for the first; none
    /// when it will not tell, once `longest` has passed all the same.
    fn wait(&self, longest: Duration) -> Option<Vec<Event>>;
}

/// Something the kernel may refuse, time after time: only a refusal that
/// follows its being given is news.
#[derive(Default)]
struct Refusal(Cell<bool>);

impl Refusal {
    /// Whether this outcome is a refusal not yet told of.
    fn news(&self, refused: bool) -> bool {
        let before = self.0.replace(refused);
        refused && !before
    }

    /// What `asked` gave; a refusal is logged with its cause, a call and
    /// its status, when it is news.
    fn given<T>(&self, what: &str, asked: Result<T, impl Display>) -> Option<T> {
        if self.news(asked.is_err())
            && let Err(cause) = &asked
        {
            eprintln!("The agent macOS keeps restarting can't be paused: {what} ({cause})");
        }
        asked.ok()
    }
}

/// What the kernel may refuse the keeper, each logged once until it is
/// given again.
#[derive(Default)]
struct Refusals {
    processes: Refusal,
    children: Refusal,
    read: Refusal,
    token: Refusal,
    program: Refusal,
    age: Refusal,
    signal: Refusal,
    starts: Refusal,
    start: Refusal,
    notices: Refusal,
}

/// The helper's own processes and the kernel's notices of them.
struct Mac {
    on_console: fn() -> bool,
    watch: Arc<Watch>,
    refused: Refusals,
}

impl Mac {
    fn new(on_console: fn() -> bool, watch: Arc<Watch>) -> Self {
        Self {
            on_console,
            watch,
            refused: Refusals::default(),
        }
    }

    /// What `asked` gave of the process `pid`. A process may end at any
    /// time, so a refusal for one that is known to have ended is no news.
    fn of_process<T>(
        pid: i32,
        refusal: &Refusal,
        what: &str,
        asked: std::io::Result<T>,
    ) -> Option<T> {
        if asked.is_err() && has_ended(&process::info(pid)) {
            return None;
        }
        refusal.given(what, asked)
    }
}

/// Whether `found`, the kernel's word on a PID, says its process has
/// ended. A word the kernel did not give says nothing.
fn has_ended(found: &atlas_core::error::Result<Option<Info>>) -> bool {
    match found {
        Ok(None) => true,
        Ok(Some(found)) => found.zombie,
        Err(_) => false,
    }
}

impl System for Mac {
    fn hidden(&self) -> bool {
        !(self.on_console)()
    }

    fn now(&self) -> Instant {
        Instant::now()
    }

    fn children(&self, parent: i32) -> Option<Vec<i32>> {
        let listed = process::children(parent).map_err(|refusal| refusal.error);
        self.refused
            .children
            .given("launchd's processes are not listed", listed)
    }

    fn processes(&self, uid: u32) -> Option<Vec<Info>> {
        let listed = process::of_user(uid).map_err(|refusal| refusal.error);
        self.refused
            .processes
            .given("the helper's processes are not listed", listed)
    }

    fn process(&self, pid: i32) -> Seen {
        let read = process::info(pid).map_err(|refusal| refusal.error);
        match self.refused.read.given("a process is not described", read) {
            Some(Some(found)) => Seen::Found(found),
            Some(None) => Seen::Gone,
            None => Seen::Unknown,
        }
    }

    fn exact(&self, pid: i32) -> Option<Exact> {
        let refused = &self.refused;
        let token = Self::of_process(
            pid,
            &refused.token,
            "a process's token is not given",
            Token::of(pid),
        )?;
        let program = Self::of_process(
            pid,
            &refused.program,
            "a process's program is not told",
            token.program(),
        )?;
        // Taken first, so the age holds for this moment at the latest.
        let read = Instant::now();
        let age = Self::of_process(
            pid,
            &refused.age,
            "a process's age is not told",
            token.age(),
        )?;
        Some(Exact {
            token,
            program,
            age,
            read,
        })
    }

    fn signal(&self, token: &Token, signal: Signal) -> bool {
        let sent = token.signal(signal);
        Self::of_process(
            token.pid(),
            &self.refused.signal,
            "a signal is not sent",
            sent,
        )
        .is_some()
    }

    fn settle(&self) {
        std::thread::sleep(SETTLE);
    }

    fn watch_starts(&self, parent: i32) -> bool {
        let watched = self.watch.spawns(parent);
        self.refused
            .starts
            .given("launchd's starts are not told", watched)
            .is_some()
    }

    fn watch(&self, pid: i32) -> bool {
        match self.watch.process(pid) {
            // It ended first, as a process may at any time: nothing to tell.
            Err(nix::errno::Errno::ESRCH) => false,
            watched => self
                .refused
                .start
                .given("a start is not watched", watched)
                .is_some(),
        }
    }

    fn forget(&self, pid: i32) {
        self.watch.forget(pid);
    }

    fn wait(&self, longest: Duration) -> Option<Vec<Event>> {
        let told = self.watch.wait(longest);
        self.refused
            .notices
            .given("the kernel's notices are not told", told)
    }
}

/// A process launchd started, watched until it becomes the agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Start {
    process: Instance,
    /// When the keeper first saw it.
    seen: Instant,
}

/// The agent as this keeper stopped it or took it over: signalled only
/// through its token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Kept {
    process: Instance,
    token: Token,
}

/// What the keeper does about the agent now.
#[derive(Debug, PartialEq, Eq)]
enum Phase {
    /// The helper is on screen, or may be: nothing is watched or paused.
    OnScreen,
    /// The helper is hidden. launchd's starts are told, and each process
    /// it started is watched until it becomes the agent.
    Watching(Vec<Start>),
    /// The helper is hidden and the agent is paused.
    Paused(Kept),
}

/// The agent's keeper: everything is decided and signalled from its one
/// thread. However it goes, the agent is continued: at its end, and as it
/// is dropped, a panic's unwinding included.
struct Keeper<S: System> {
    system: S,
    uid: u32,
    /// This login's session, which the agent's must be.
    session: i32,
    phase: Phase,
    /// Whether the helper's processes were looked through for an agent an
    /// earlier daemon left paused, and each one that might be it was read.
    looked_for_one_left: bool,
    /// Processes this keeper stopped and then continued that were not yet
    /// seen to run again, or gone: each is continued again at every look.
    owed: Vec<Kept>,
    /// launchd's children when last listed: one not among them is new.
    known: HashSet<i32>,
    report: Arc<Mutex<Report>>,
    pauses: u64,
    misses: u64,
    /// Whether the kernel refused, at the last try, to tell launchd's
    /// starts.
    blind: bool,
}

impl<S: System> Keeper<S> {
    /// A keeper for the helper `uid`, whose login is `session`. It does
    /// nothing until its first look.
    fn new(system: S, uid: u32, session: i32, report: Arc<Mutex<Report>>) -> Self {
        Self {
            system,
            uid,
            session,
            phase: Phase::OnScreen,
            looked_for_one_left: false,
            owed: Vec::new(),
            known: HashSet::new(),
            report,
            pauses: 0,
            misses: 0,
            blind: false,
        }
    }

    /// One look at where the helper is and how the agent is.
    fn look(&mut self) {
        self.continue_owed();
        if !self.looked_for_one_left {
            self.looked_for_one_left = self.take_over_one_left();
        }
        let hidden = self.system.hidden();
        match (&self.phase, hidden) {
            (Phase::OnScreen, false) => self.blind = false,
            (Phase::OnScreen, true) => self.watch_starts(),
            (Phase::Watching(_), true) => self.forget_stale(),
            (Phase::Paused(kept), true) => {
                let kept = *kept;
                self.check(&kept);
            }
            (_, false) => self.let_go(),
        }
        self.tell();
    }

    /// Take in one of the kernel's notices.
    fn on(&mut self, event: Event) {
        match event {
            Event::Spawned(_) => self.started(),
            Event::Became(pid) => self.became(pid),
            Event::Ended(pid) => self.ended(pid),
            Event::Woken => {}
        }
        self.tell();
    }

    /// Take over an agent found paused, as a daemon that ended without
    /// continuing it leaves it: it is this keeper's from here. Whether the
    /// look was complete: a list or a process the kernel did not give is
    /// asked for again at the next look.
    fn take_over_one_left(&mut self) -> bool {
        let Some(processes) = self.system.processes(self.uid) else {
            return false;
        };
        let mut complete = true;
        for found in processes {
            if !found.stopped || found.ppid != LAUNCHD || found.zombie {
                continue;
            }
            match self.identify(found.instance.pid) {
                Is::Agent(found, exact) => {
                    self.keep(Kept {
                        process: found.instance,
                        token: exact.token,
                    });
                    return true;
                }
                Is::Other => {}
                Is::Unknown => complete = false,
            }
        }
        complete
    }

    /// Wait for launchd's next start of the agent. A watch or a list the
    /// kernel refuses is asked for again at the next look.
    fn watch_starts(&mut self) {
        let children = self
            .system
            .watch_starts(LAUNCHD)
            .then(|| self.system.children(LAUNCHD))
            .flatten();
        self.blind = children.is_none();
        let Some(children) = children else {
            self.system.forget(LAUNCHD);
            self.phase = Phase::OnScreen;
            return;
        };
        self.known = children.into_iter().collect();
        self.phase = Phase::Watching(Vec::new());
    }

    /// launchd started something: each process new among its children is
    /// watched until it becomes the agent, and looked at once in case it
    /// already has. A list the kernel does not give leaves the start to
    /// the next notice.
    fn started(&mut self) {
        if !matches!(self.phase, Phase::Watching(_)) {
            return;
        }
        let Some(children) = self.system.children(LAUNCHD) else {
            return;
        };
        let new: Vec<i32> = children
            .iter()
            .copied()
            .filter(|pid| !self.known.contains(pid))
            .collect();
        self.known = children.into_iter().collect();

        for pid in new {
            let Seen::Found(found) = self.system.process(pid) else {
                continue;
            };
            if found.ppid != LAUNCHD || found.zombie || !self.system.watch(pid) {
                continue;
            }
            let seen = self.system.now();
            let Phase::Watching(starting) = &mut self.phase else {
                // An earlier one of these was the agent, and is paused; or
                // the helper came on screen.
                self.system.forget(pid);
                continue;
            };
            starting.push(Start {
                process: found.instance,
                seen,
            });
            self.became(pid);
        }
    }

    /// A process launchd started is another program now, or may be: when
    /// it is the agent, it is paused. One that is not stays watched, as
    /// what launchd starts becomes its own go-between first and the agent
    /// only then.
    fn became(&mut self, pid: i32) {
        let Phase::Watching(starting) = &self.phase else {
            return;
        };
        let Some(start) = starting.iter().find(|start| start.process.pid == pid) else {
            return;
        };
        let process = start.process;
        if let Is::Agent(found, exact) = self.identify(pid)
            && found.instance == process
        {
            self.pause(&found, &exact);
        }
    }

    /// A watched process ended: the paused agent, so its next start is
    /// awaited, or one launchd started that never became it.
    fn ended(&mut self, pid: i32) {
        match &mut self.phase {
            Phase::Paused(kept) if kept.process.pid == pid => self.watch_starts(),
            Phase::Watching(starting) => starting.retain(|start| start.process.pid != pid),
            Phase::Paused(_) | Phase::OnScreen => {}
        }
    }

    /// What `pid` is now. The agent in this login is launchd's own child,
    /// this account's, in this login's session, running the agent's
    /// program. It is read before and after its token, so a PID reused in
    /// between is not taken for it.
    fn identify(&self, pid: i32) -> Is {
        let ours = |found: &Info| {
            found.uid == self.uid
                && found.ruid == self.uid
                && found.ppid == LAUNCHD
                && !found.zombie
        };
        let before = match self.system.process(pid) {
            Seen::Found(found) if ours(&found) => found,
            Seen::Found(_) | Seen::Gone => return Is::Other,
            Seen::Unknown => return Is::Unknown,
        };
        let Some(exact) = self.system.exact(pid) else {
            return Is::Unknown;
        };
        let after = match self.system.process(pid) {
            Seen::Found(found) if ours(&found) && found.instance == before.instance => found,
            Seen::Found(_) | Seen::Gone => return Is::Other,
            Seen::Unknown => return Is::Unknown,
        };
        let named = exact.token.pid() == pid
            && exact.token.uid() == self.uid
            && exact.token.session() == self.session
            && exact.program == Path::new(PROGRAM);
        if named {
            Is::Agent(after, exact)
        } else {
            Is::Other
        }
    }

    /// Pause the agent `found`, a start just seen: only while it is young,
    /// and only while the helper is still hidden. An older start may be in
    /// the middle of its work with other processes, and is left to quit as
    /// it does.
    fn pause(&mut self, found: &Info, exact: &Exact) {
        let kept = Kept {
            process: found.instance,
            token: exact.token,
        };
        if found.stopped {
            // Someone else's stop: it is paused all the same, and kept.
            self.keep(kept);
            return;
        }
        // The helper may have come on screen since the last look.
        if !self.system.hidden() {
            self.let_go();
            return;
        }
        // Its age now, which the looks since it was read have added to.
        let young = |keeper: &Self| {
            let since = keeper.system.now().saturating_duration_since(exact.read);
            exact.age.saturating_add(since) <= YOUNG
        };
        if !young(self) {
            self.misses += 1;
            self.unwatch(kept.process.pid);
            return;
        }

        // Owed its run from before the stop is sent until it is kept or
        // seen to run again, so it is continued however what follows ends,
        // a panic included.
        self.owe(kept);
        // A stop sent while it was young, and seen to hold while the helper
        // is still hidden, is kept.
        let sent = self.system.signal(&kept.token, Signal::SIGSTOP);
        let in_time = sent && young(self) && self.settled(&kept, true);
        if in_time && self.system.hidden() {
            self.pauses += 1;
            self.keep(kept);
            self.owed.retain(|owed| *owed != kept);
            return;
        }
        // A stop that came late, or did not show, may find the agent at
        // work; one that showed as the helper came on screen is not wanted.
        // Each is taken back, and stays owed until the process is seen to
        // run.
        if !in_time {
            self.misses += 1;
        }
        self.unwatch(kept.process.pid);
        if sent {
            self.release(kept);
        } else {
            // Nothing reached it, so nothing is owed.
            self.owed.retain(|owed| *owed != kept);
        }
    }

    /// Owe `kept` its run, once.
    fn owe(&mut self, kept: Kept) {
        if !self.owed.contains(&kept) {
            self.owed.push(kept);
        }
    }

    /// Hold `kept` paused: launchd starts no other meanwhile, so nothing
    /// more is watched but its end.
    fn keep(&mut self, kept: Kept) {
        self.system.forget(LAUNCHD);
        if let Phase::Watching(starting) = &self.phase {
            for start in starting {
                if start.process != kept.process {
                    self.system.forget(start.process.pid);
                }
            }
        }
        // One that ends before its end can be watched is seen gone at the
        // next look.
        self.system.watch(kept.process.pid);
        self.phase = Phase::Paused(kept);
    }

    /// Watch `pid` no more, as a start that will not be paused.
    fn unwatch(&mut self, pid: i32) {
        if let Phase::Watching(starting) = &mut self.phase {
            starting.retain(|start| start.process.pid != pid);
        }
        self.system.forget(pid);
    }

    /// Look at the paused agent. One that someone else continued quits and
    /// is started again, as one that is gone; one the kernel does not
    /// describe just now is still this keeper's.
    fn check(&mut self, kept: &Kept) {
        match self.system.process(kept.process.pid) {
            Seen::Unknown => {}
            Seen::Found(found)
                if found.instance == kept.process && found.stopped && !found.zombie => {}
            Seen::Found(_) | Seen::Gone => {
                self.system.forget(kept.process.pid);
                self.watch_starts();
            }
        }
    }

    /// Whether `kept` came to be stopped, or to be stopped no more, soon
    /// after its signal. One that is gone is not stopped; one the kernel
    /// does not describe is not known to be either.
    fn settled(&self, kept: &Kept, stopped: bool) -> bool {
        for _ in 0..LOOKS {
            match self.system.process(kept.process.pid) {
                Seen::Found(found) if found.instance == kept.process && !found.zombie => {
                    if found.stopped == stopped {
                        return true;
                    }
                }
                Seen::Found(_) | Seen::Gone => return !stopped,
                Seen::Unknown => {}
            }
            self.system.settle();
        }
        false
    }

    /// Continue `kept` through its token, which reaches that very process
    /// or nothing. It is owed its run from before the signal until it is
    /// seen to run again, or gone.
    fn release(&mut self, kept: Kept) {
        self.owe(kept);
        // The kernel refuses the token of a process that is gone, and then
        // nothing is left to continue: what was sent is seen below.
        self.system.signal(&kept.token, Signal::SIGCONT);
        if self.settled(&kept, false) {
            self.owed.retain(|owed| *owed != kept);
        }
    }

    /// Continue again each process still owed its run.
    fn continue_owed(&mut self) {
        for kept in self.owed.clone() {
            self.release(kept);
        }
    }

    /// The helper is on screen, or the keeper is going: the agent runs
    /// again and nothing is watched.
    fn let_go(&mut self) {
        // Owed its run before it stops being the paused one.
        if let Phase::Paused(kept) = &self.phase {
            let kept = *kept;
            self.owe(kept);
        }
        match std::mem::replace(&mut self.phase, Phase::OnScreen) {
            Phase::OnScreen => {}
            Phase::Watching(starting) => {
                self.system.forget(LAUNCHD);
                for start in starting {
                    self.system.forget(start.process.pid);
                }
            }
            Phase::Paused(kept) => {
                self.system.forget(kept.process.pid);
                self.release(kept);
            }
        }
    }

    /// Watch no longer the processes launchd started that are gone, or
    /// have had their time to become the agent. One the kernel does not
    /// describe just now keeps its time.
    fn forget_stale(&mut self) {
        let Phase::Watching(starting) = &mut self.phase else {
            return;
        };
        let now = self.system.now();
        starting.retain(|start| {
            let gone = match self.system.process(start.process.pid) {
                Seen::Found(found) => found.instance != start.process || found.zombie,
                Seen::Gone => true,
                Seen::Unknown => false,
            };
            let stale = gone || now.saturating_duration_since(start.seen) > BECOMING;
            if stale {
                self.system.forget(start.process.pid);
            }
            !stale
        });
    }

    /// End keeping: the agent is continued, a few times over when it is
    /// not seen to run at once. Whether nothing is left paused.
    fn end(mut self) -> bool {
        self.let_go();
        for _ in 0..LAST_TRIES {
            if self.owed.is_empty() {
                break;
            }
            self.continue_owed();
        }
        self.owed.is_empty()
    }

    /// Keep the agent until `stopping`: the kernel's notices as they come,
    /// and a look each [`LOOK`].
    fn run(&mut self, stopping: &AtomicBool) {
        self.look();
        let mut next = self.system.now() + LOOK;
        while !stopping.load(Ordering::SeqCst) {
            let wait = next.saturating_duration_since(self.system.now());
            match self.system.wait(wait) {
                Some(events) => events.into_iter().for_each(|event| self.on(event)),
                None => self.untold(),
            }
            if self.system.now() >= next {
                self.look();
                next = self.system.now() + LOOK;
            }
        }
    }

    /// The kernel will not tell its notices: launchd's starts are no longer
    /// awaited, and are asked for again at the next look. A paused agent
    /// stays kept, as each look still sees it.
    fn untold(&mut self) {
        if matches!(self.phase, Phase::Watching(_)) {
            self.let_go();
            self.blind = true;
        }
        self.tell();
    }

    /// Say what is done now, for the hello.
    fn tell(&self) {
        let state = match self.phase {
            Phase::OnScreen if self.blind => State::Unavailable,
            Phase::OnScreen => State::OnScreen,
            Phase::Watching(_) => State::Watching,
            Phase::Paused(_) => State::Paused,
        };
        self.say(state);
    }

    fn say(&self, state: State) {
        if let Ok(mut report) = self.report.lock() {
            *report = Report {
                state,
                pauses: self.pauses,
                misses: self.misses,
            };
        }
    }
}

impl<S: System> Drop for Keeper<S> {
    fn drop(&mut self) {
        self.let_go();
        self.continue_owed();
        self.say(State::Off);
    }
}

#[cfg(test)]
#[path = "console_agent_tests.rs"]
mod tests;
