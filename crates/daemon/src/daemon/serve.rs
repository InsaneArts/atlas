//! The helper daemon's socket: one request per connection, answered with
//! protocol v1 replies.
use super::capture::{Capture, Screen, screenshot};
use crate::daemon::{
    consent,
    console_agent::ConsoleAgent,
    facts,
    job::{self, Jobs},
    preferences, proof, read,
    recorder::{self, Recorder},
    run,
    sound::{Set, Sound},
};
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
    outcomes::{Outcome, Outcomes, Remembering},
    protocol::{
        self, Asked, ConnectionPhase, MAX_REQUEST, Policy, ReadRequest, Request, failure,
        interrupted, refused,
    },
    session::{Console, validate_target_uid},
};
use atlas_native::native::{apps, capture, endpoint, windows};
#[cfg(test)]
use objc2_core_foundation::CGRect;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime},
};
use tokio::{
    io::{AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    signal::unix::{Signal, SignalKind, signal},
    sync::{Mutex, Notify, Semaphore, mpsc, oneshot, watch},
    task::{Id, JoinError, JoinHandle, JoinSet},
};

pub mod machine;
mod stream;

/// Connections served at once.
const CONNECTIONS: usize = 32;

/// Longest a single reply write may take; a client that stops reading is
/// dropped rather than holding a connection slot.
const WRITE: Duration = Duration::from_secs(10);

/// Longest the daemon waits for running commands to end when it stops.
const STOP: Duration = Duration::from_secs(6);

/// Longest the runtime waits for blocking work once serving has ended.
const SHUTDOWN: Duration = Duration::from_secs(2);

/// How often a running command checks that the helper is still off screen.
const CONSOLE_CHECK: Duration = Duration::from_secs(1);

/// The file beside the socket that records the screen recorder's one
/// restart after an update.
const RESTARTED_AFTER_UPDATE: &str = "recorder-restarted-after-update";

/// A connection's reading half, whatever carries it: the helper login's
/// socket, or a lane of a machine's session.
pub(super) type Reader = Box<dyn tokio::io::AsyncRead + Unpin + Send>;

/// A connection's writing half, whatever carries it.
pub(super) type Writer = Box<dyn Outgoing>;

/// What a connection writes to.
pub(super) trait Outgoing: AsyncWrite + Unpin + Send {
    /// Make room for `bytes` to go at once, where the connection can.
    fn make_room(&self, _bytes: usize) {}
}

impl Outgoing for tokio::net::unix::OwnedWriteHalf {
    /// The socket's own small buffer would hand a large write over in
    /// thousands of turns. Without the room it goes all the same, only
    /// slower.
    fn make_room(&self, bytes: usize) {
        let _ =
            nix::sys::socket::setsockopt(self.as_ref(), nix::sys::socket::sockopt::SndBuf, &bytes);
    }
}

/// A lane of a session: its bulk credit, not a buffer, paces what it sends.
impl<S: tokio::io::AsyncWrite + Unpin + Send> Outgoing for tokio::io::WriteHalf<S> {}

type Connections = HashMap<Id, watch::Receiver<ConnectionPhase>>;

fn connection_outcome(
    outcome: std::result::Result<(Id, Result<()>), JoinError>,
    connections: &mut Connections,
    stopping: bool,
) -> Result<()> {
    let id = match &outcome {
        Ok((id, _)) => *id,
        Err(error) => error.id(),
    };
    let phase = connections
        .remove(&id)
        .map(|phase| *phase.borrow())
        .unwrap_or(ConnectionPhase::Mutation("connection"));
    let (operation, mutation) = match phase {
        ConnectionPhase::Refusal => ("refusal", false),
        ConnectionPhase::Read(operation) => (operation, false),
        ConnectionPhase::Mutation(operation) => (operation, true),
    };
    match outcome {
        Ok((_, Ok(()))) => Ok(()),
        // A refused request has no effects; abandoned read replies are
        // expected and say nothing about daemon or supervisor health.
        Ok((_, Err(_))) if !mutation => Ok(()),
        Err(error) if stopping && error.is_cancelled() => Ok(()),
        Err(error) => supervisor_outcome(Err(error), stopping).map_err(|cause| {
            Error::new(
                cause.code,
                format!("Daemon {operation} connection failed; outcome unknown"),
            )
            .uncertain()
            .with_source(cause)
        }),
        Ok((_, Err(cause))) if cause.code == Code::Job => Err(cause),
        Ok((_, Err(cause))) => Err(Error::new(
            Code::Transport,
            format!("Daemon {operation} reply was lost; outcome unknown"),
        )
        .uncertain()
        .with_source(cause)),
    }
}

async fn drain_connections(
    tasks: &mut JoinSet<Result<()>>,
    connections: &mut Connections,
    deadline: tokio::time::Instant,
) -> Result<()> {
    let mut failure = Ok(());
    let mut observe = |outcome| {
        if let Err(error) = connection_outcome(outcome, connections, true) {
            eprintln!("{}", error.error);
            failure = Err(error);
        }
    };
    let drain = async {
        while let Some(outcome) = tasks.join_next_with_id().await {
            observe(outcome);
        }
    };
    if tokio::time::timeout_at(deadline, drain).await.is_err() {
        tasks.abort_all();
        while let Some(outcome) = tasks.join_next_with_id().await {
            observe(outcome);
        }
    }
    failure
}

fn run_outcome(daemon: &Daemon, outcome: std::result::Result<Result<()>, JoinError>) -> Result<()> {
    let outcome = match outcome {
        Ok(outcome) => outcome.map_err(Error::uncertain),
        Err(error) => supervisor_outcome(Err(error), false).map_err(Error::uncertain),
    };
    if let Err(error) = &outcome {
        // Only fixed labels and codes reach diagnostics; the retained
        // source may contain details of the abandoned transport.
        eprintln!("Daemon run outcome unknown ({})", error.code);
        if error.code == Code::Job {
            daemon.fail_supervision();
        }
    }
    outcome
}

async fn drain_runs(daemon: &Daemon, deadline: tokio::time::Instant) -> Result<()> {
    let drain = async {
        let mut runs = daemon.runs.lock().await;
        let mut failure = Ok(());
        while let Some(outcome) = runs.join_next().await {
            if let Err(error) = run_outcome(daemon, outcome) {
                failure = Err(error);
            }
        }
        failure
    };
    tokio::time::timeout_at(deadline, drain)
        .await
        .unwrap_or_else(|_| {
            let error = Error::new(
                Code::Daemon,
                "Daemon commands did not finish before shutdown; outcome unknown",
            )
            .uncertain();
            eprintln!("{}", error.error);
            Err(error)
        })
}

pub struct Daemon {
    /// launchd's session type, read once at start.
    manager: String,
    screen: Screen,
    restarting: Notify,
    /// Set when the daemon stops, so running commands are ended first.
    stopping: watch::Sender<bool>,
    /// The background jobs it supervises; none when their folder could not
    /// be opened, and then it offers none.
    jobs: Option<Arc<Jobs>>,
    /// Foreground commands outlive a cancelled socket handler until their
    /// process group and observed event writer have finished.
    runs: Mutex<JoinSet<Result<()>>>,
    /// Set, under the `runs` lock, once a restart of its own is coming:
    /// new foreground commands are then refused before they run, so the
    /// restart ends none.
    commands_closed: AtomicBool,
    /// Asks the listener to collect finished commands now, which it
    /// otherwise does only when something else wakes it.
    collect_runs: Notify,
    /// The one live capture source; held through its native source's drain.
    source: Arc<capture::source::Source>,
    /// The persistent child that answers session-proof requests.
    proof_worker: Mutex<Option<proof::Worker>>,
    /// Whether the helper's apps can be heard on the person's speakers.
    sound: Sound,
    /// The Apple agent that would restart over and over while the helper
    /// is hidden, kept paused.
    agent: ConsoleAgent,
    /// The screen recorder, as each hello reports it; restarted only in the
    /// helper.
    recorder: Arc<Recorder>,
    /// What the changes that named themselves came to, for whoever lost
    /// the reply to one.
    outcomes: Arc<Outcomes>,
    /// Where a machine's changes are kept on its disk, outliving this
    /// process; none for the helper login, whose station keeps its own.
    kept: Option<Arc<dyn Kept>>,
    /// What a machine's commands run under, and where each is kept before it
    /// starts; none for the helper login, whose commands run as they are.
    supervising: Option<crate::machine::processes::Supervising>,
    /// The supervisors its keepers own, and how each stands.
    keepers: Arc<run::keeper::Keepers>,
    /// A machine's input service, its daemon's child; none for the helper
    /// login, whose input service is its own.
    input: Option<Arc<crate::machine::input::Input>>,
    /// What a machine's station stages into it; none for the helper login,
    /// whose copies go through the shared folder.
    stages: Option<Arc<crate::machine::stage::Stages>>,
    /// Whether agents are stopped on a machine desktop, and how many Stops
    /// there have been: no change is taken while stopped, and a command
    /// admitted before a Stop ends with it, a Resume after or not. Never
    /// raised for the helper login, whose station stops agents itself.
    halted: watch::Sender<Halt>,
}

/// Whether agents are stopped on a machine desktop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Halt {
    /// Every Stop so far, saved and carried out: only ever goes up.
    pub(super) stops: u64,
    /// Whether no change is taken now.
    pub(super) halted: bool,
}

/// Changes kept on the desktop's disk: what each came to, before it is
/// answered, and what one came to when it is asked again.
pub(crate) trait Kept: Send + Sync {
    /// Keep `reply` as what `change` came to.
    ///
    /// # Errors
    ///
    /// Fails when it cannot be kept.
    fn done(&self, change: &Token, reply: &Value) -> Result<()>;

    /// What `change` came to, if it is kept: none while unresolved.
    ///
    /// # Errors
    ///
    /// Fails when the records cannot be trusted.
    fn outcome(&self, change: &Token) -> Result<Outcome>;
}

impl Daemon {
    pub(crate) fn new(
        manager: String,
        capture: Capture,
        on_console: fn() -> bool,
        session: fn() -> Result<Value>,
    ) -> Self {
        Self {
            manager,
            screen: Screen {
                capture,
                raw_capture: capture::main_display_rgb,
                windows: windows::windows,
                bundle_id: apps::bundle_id,
                dock: |windows| {
                    atlas_native::native::ax::dock(windows, atlas_native::native::ax::DOCK_TIMEOUT)
                },
                on_console,
                console: Console::kept(),
                session,
            },
            restarting: Notify::new(),
            stopping: watch::Sender::new(false),
            jobs: None,
            runs: Mutex::new(JoinSet::new()),
            commands_closed: AtomicBool::new(false),
            collect_runs: Notify::new(),
            source: Arc::new(capture::source::Source::new(capture::stop_display_stream)),
            proof_worker: Mutex::new(None),
            sound: Sound::detached(),
            agent: ConsoleAgent::detached(),
            recorder: Arc::new(Recorder::new(recorder::System::real(Console::kept()), None)),
            outcomes: Arc::default(),
            kept: None,
            supervising: None,
            keepers: Arc::default(),
            input: None,
            stages: None,
            halted: watch::Sender::new(Halt::default()),
        }
    }

    /// Establish this process's helper identity before applying login preferences
    /// or adopting jobs. Missing job storage leaves that capability unavailable.
    fn installed(uid: u32, socket: &Path) -> Result<Arc<Self>> {
        if nix::unistd::getuid().as_raw() != uid {
            return Err(Error::new(
                Code::Identity,
                "The helper daemon must run as the configured helper account",
            ));
        }

        let manager = facts::manager_name();
        require_graphical_session(uid, &manager)?;
        atlas_core::user::login_log::mark(&format!("daemon.start uid={uid}"));
        preferences::type_exactly();
        preferences::keep_desktop()?;

        let mut daemon = Self::new(
            manager,
            capture::main_display,
            facts::observed(Console::kept()),
            atlas_native::native::session_info,
        );
        // The helper's apps go silent before any agent acts, as the person
        // last chose.
        daemon.sound = Sound::start(uid, std::env::var_os("HOME").map(PathBuf::from));
        daemon.agent = ConsoleAgent::of(uid, facts::on_console);
        // Only a daemon shown to run as the helper restarts its screen
        // recorder; one built any other way only reads it. A restart never
        // overlaps the input service's input, told beside its socket.
        let mut recorder = Recorder::new(recorder::System::real(Console::kept()), Some(uid));
        if let Some(home) = std::env::var_os("HOME") {
            let input =
                crate::helper_paths::HelperPaths::new(uid, PathBuf::from(home)).input_socket();
            recorder = recorder.beside_input(atlas_native::native::activity::beside(&input));
        }
        // A recorder grown under an earlier version is restarted once, early;
        // that it was is kept beside the socket, so it never repeats.
        if let Some(folder) = socket.parent() {
            recorder = recorder.once_after_update(folder.join(RESTARTED_AFTER_UPDATE));
        }
        daemon.recorder = Arc::new(recorder);
        // Jobs are kept beside the socket; the jobs a daemon before this one
        // started are found again by their records.
        if let (Some(folder), Ok(launcher)) = (socket.parent(), std::env::current_exe()) {
            match Jobs::open(folder, launcher, uid) {
                Ok(jobs) => daemon.jobs = Some(Arc::new(jobs)),
                Err(error) => eprintln!("Background jobs are unavailable: {}", error.error),
            }
        }
        Ok(Arc::new(daemon))
    }

    /// The daemon's hello: its facts, and the background jobs it keeps.
    fn hello(&self) -> Value {
        // The first hello of this login: Atlas answers in the helper.
        static GREETED: AtomicBool = AtomicBool::new(false);
        if !GREETED.swap(true, Ordering::Relaxed) {
            atlas_core::user::login_log::mark("daemon.firstHello");
        }
        match self.jobs.as_deref().map(Jobs::census).transpose() {
            Ok(census) => {
                let mut hello =
                    facts::hello(&self.manager, census, self.recorder.report(&self.source));
                hello["sound"] = self.sound.report().json();
                hello["consoleAgent"] = self.agent.report().json();
                hello
            }
            Err(error) => json!(error),
        }
    }

    /// Start supervising the jobs, when there are any.
    fn supervision(&self, uid: u32) -> Option<JoinHandle<Result<()>>> {
        // The person's Stop is seen through the input service's saved marker,
        // where the helper's layout puts it: the configuration's input socket
        // comes from the same layout.
        let home = std::env::var_os("HOME")?;
        let marker = crate::helper_paths::HelperPaths::new(uid, PathBuf::from(home)).stop_marker();
        Some(tokio::spawn(job::supervise(
            self.jobs.clone()?,
            marker,
            (self.screen.console, self.screen.on_console),
            self.stopping.subscribe(),
        )))
    }

    fn fail_supervision(&self) {
        if let Some(jobs) = &self.jobs {
            jobs.fail_supervision();
        }
    }

    /// Completes once agents were stopped since `stops` Stops, whatever
    /// came after.
    async fn stopped_since(&self, stops: u64) {
        let _ = self
            .halted
            .subscribe()
            .wait_for(|halt| halt.stops > stops)
            .await;
    }

    /// Completes once the daemon has begun to stop.
    async fn stopped(&self) {
        let _ = self
            .stopping
            .subscribe()
            .wait_for(|stopping| *stopping)
            .await;
    }

    /// Close to new foreground commands when none runs and the screen is
    /// where this desktop keeps it, so a restart that follows ends none. Says whether it
    /// closed; once closed it stays so until this process ends. Commands
    /// that finished but were not collected yet count until the listener,
    /// asked here, collects them and sees what they came to.
    async fn close_commands(&self) -> bool {
        let runs = self.runs.lock().await;
        if !runs.is_empty() {
            self.collect_runs.notify_one();
            return false;
        }
        if self.screen.away() {
            return false;
        }
        self.commands_closed.store(true, Ordering::Release);
        true
    }

    /// Completes once the screen is away from where this desktop keeps it,
    /// checked about once a second.
    async fn moved_away(&self) {
        loop {
            tokio::time::sleep(CONSOLE_CHECK).await;

            if self.screen.away() {
                return;
            }
        }
    }
}

/// Refuse to start outside the helper's own logged-in graphical session.
fn require_graphical_session(uid: u32, manager: &str) -> Result<()> {
    let proof = atlas_native::native::session_info()?;
    let refused = |why: &str| {
        Error::new(
            Code::Identity,
            format!("The helper daemon must run in the helper's graphical login: {why}"),
        )
    };

    if manager != "Aqua" {
        return Err(refused(&format!("launchd session type is {manager}")));
    }

    match facts::session_owner() {
        Some((owner, true)) if owner == i64::from(uid) => {}
        Some((_, true)) => return Err(refused("the session belongs to another user")),
        Some((_, false)) => return Err(refused("the login has not finished")),
        None => return Err(refused("the session's owner is unknown")),
    }

    if proof["hasGraphicAccess"] != true {
        return Err(refused("the session has no graphics access"));
    }

    Ok(())
}

/// Serve protocol v1 on `socket` as the helper `uid`, for the controller
/// `controller_uid`. Returns when stopped; a `restart` request ends it with
/// an error so launchd starts a fresh daemon.
pub(crate) async fn serve(uid: u32, controller_uid: u32, socket: PathBuf) -> Result<()> {
    // Read first: the setup below can take long enough for the person to
    // switch away meanwhile.
    let started_on_screen = facts::on_console();
    validate_target_uid(uid, controller_uid)?;

    let daemon = Daemon::installed(uid, &socket)?;

    let (listener, _endpoint) = endpoint::bind(
        &socket,
        uid,
        "helper daemon",
        endpoint::Directory::Private {
            controller: controller_uid,
        },
    )
    .await?;

    // Both listeners exist before work starts, so setup failure leaves no worker behind.
    let interrupt = signal(SignalKind::interrupt())?;
    let terminate = signal(SignalKind::terminate())?;

    // A Screen Recording grant reaches this process only through a restart.
    let restarter = daemon.clone();
    let asking = tokio::spawn(consent::request_consent_on_arrival(
        controller_uid,
        move || {
            restarter.restarting.notify_one();
        },
        daemon.sound.clone(),
    ));
    let watcher = daemon.clone();
    let recording = tokio::spawn(consent::restart_for_recording(move || {
        watcher.restarting.notify_one();
    }));
    // Started on the screen, the hidden login has no mode for the display it
    // gets off it. A machine's own session is on its screen by design.
    let restarter = daemon.clone();
    let closer = daemon.clone();
    let displaying = tokio::spawn(super::display_mode::restart_off_screen(
        started_on_screen && daemon.screen.console == Console::Off,
        daemon.screen.on_console,
        facts::display_has_mode,
        async move || closer.close_commands().await,
        move || restarter.restarting.notify_one(),
    ));
    let releasing = tokio::spawn(stream::release_kept_pictures(daemon.clone()));
    // Kept paused from here until serving ends, however it ends: a daemon
    // that is gone must not leave it paused.
    let _keeping = daemon.agent.start();
    let mut serving = Serving {
        supervision: daemon.supervision(uid),
        daemon: daemon.clone(),
        helper: uid,
        controller: controller_uid,
        interrupt,
        terminate,
        slots: Arc::new(Semaphore::new(CONNECTIONS)),
        tasks: JoinSet::new(),
        connections: Connections::new(),
    };
    let outcome = serving.listen(&listener).await;

    // Running commands are asked to end (TERM, then KILL) before the daemon
    // exits; work that misses the deadline stays unknown. Background jobs
    // go on: the next daemon finds them again by their records.
    daemon.stopping.send_replace(true);
    let deadline = tokio::time::Instant::now() + STOP;
    // The agent runs again first, off the runtime, as the wait for it blocks.
    let letting_go = daemon.clone();
    let let_go = tokio::task::spawn_blocking(move || letting_go.agent.stop());
    let drained = drain_connections(&mut serving.tasks, &mut serving.connections, deadline).await;
    let runs = drain_runs(&daemon, deadline).await;

    asking.abort();
    recording.abort();
    displaying.abort();
    releasing.abort();
    // A keeper that has not ended by now is waited for once more as
    // `_keeping` is dropped.
    let _ = let_go.await;
    if let Some(supervision) = serving.supervision {
        let supervised = drain_supervision(supervision, deadline).await;
        if supervised.is_err() {
            daemon.fail_supervision();
            return supervised;
        }
    }
    outcome.and(drained).and(runs)
}

/// What serving holds until it ends: what can end it, and the connections it
/// has admitted.
struct Serving {
    daemon: Arc<Daemon>,
    helper: u32,
    controller: u32,
    interrupt: Signal,
    terminate: Signal,
    supervision: Option<JoinHandle<Result<()>>>,
    slots: Arc<Semaphore>,
    tasks: JoinSet<Result<()>>,
    connections: Connections,
}

impl Serving {
    /// Admit credentialed connections and observe every registered owner until
    /// something ends serving, which is the error returned.
    async fn listen(&mut self, listener: &UnixListener) -> Result<()> {
        let daemon = self.daemon.clone();
        loop {
            let mut runs = daemon.runs.lock().await;
            while let Some(outcome) = runs.try_join_next() {
                if let Err(error) = run_outcome(&daemon, outcome)
                    && error.code == Code::Job
                {
                    return Err(error);
                }
            }
            drop(runs);
            tokio::select! {
                result = supervision_ended(&mut self.supervision) => {
                    self.supervision = None;
                    daemon.fail_supervision();
                    return supervisor_outcome(result, false);
                }
                // Stopped from outside (macOS's Quit & Reopen after a new
                // permission, say): exit unsuccessfully so launchd starts it
                // again. Uninstalling unloads the job first, so it stays down.
                _ = self.interrupt.recv() => {
                    return Err(Error::new(Code::Restart, "Interrupted; launchd starts it again"));
                }
                _ = self.terminate.recv() => {
                    return Err(Error::new(Code::Restart, "Stopped; launchd starts it again"));
                }
                () = daemon.restarting.notified() => {
                    return Err(Error::new(Code::Restart, "Restarting at the controller's request"));
                }
                // Finished commands are collected at the top of the loop.
                () = daemon.collect_runs.notified() => {}
                Some(result) = self.tasks.join_next_with_id(), if !self.tasks.is_empty() => {
                    if let Err(error) = connection_outcome(result, &mut self.connections, false) {
                        if error.code == Code::Job {
                            daemon.fail_supervision();
                            return Err(error);
                        }
                        eprintln!("{}", error.error);
                    }
                }
                accepted = listener.accept() => match accepted {
                    Ok((stream, _)) => self.admit(stream),
                    Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
                },
            }
        }
    }

    fn admit(&mut self, stream: UnixStream) {
        let Ok(peer) = stream.peer_cred() else {
            return;
        };
        let controller = peer.uid() == self.controller;
        if !controller && peer.uid() != self.helper {
            return;
        }
        let Ok(slot) = self.slots.clone().try_acquire_owned() else {
            // Nothing ran: say so, rather than hang up in a way a client must
            // treat as uncertain.
            endpoint::turn_away(
                stream,
                &failure(
                    Code::Busy,
                    "The helper is serving too many requests; try again",
                ),
            );
            return;
        };
        let daemon = self.daemon.clone();
        let (phase, observed) = watch::channel(ConnectionPhase::Refusal);
        let task = self.tasks.spawn(async move {
            let _slot = slot;
            handle(stream, daemon, controller, phase).await
        });
        self.connections.insert(task.id(), observed);
    }
}

/// Completes when supervision ends; without any, never.
async fn supervision_ended(
    supervision: &mut Option<JoinHandle<Result<()>>>,
) -> std::result::Result<Result<()>, JoinError> {
    match supervision {
        Some(supervision) => supervision.await,
        None => std::future::pending().await,
    }
}

async fn drain_supervision(
    mut task: JoinHandle<Result<()>>,
    deadline: tokio::time::Instant,
) -> Result<()> {
    match tokio::time::timeout_at(deadline, &mut task).await {
        Ok(outcome) => supervisor_outcome(outcome, true),
        Err(error) => {
            // Aborting the async owner cannot acknowledge any blocking
            // record work. Its durable facts remain for the next daemon.
            task.abort();
            Err(Jobs::unavailable().uncertain().with_source(error))
        }
    }
}

fn supervisor_outcome(
    outcome: std::result::Result<Result<()>, JoinError>,
    stopping: bool,
) -> Result<()> {
    match outcome {
        Ok(Ok(())) if stopping => Ok(()),
        Ok(Ok(())) => Err(Jobs::unavailable()),
        Ok(Err(error)) => Err(error),
        Err(error) => Err(Jobs::unavailable().with_source(error)),
    }
}

/// Serve on a Tokio runtime in a worker thread while this (main) thread runs
/// the Core Foundation run loop, which AppKit needs to keep its view of
/// running applications current.
///
/// # Errors
///
/// Rejects the wrong UID or graphical session, unsafe socket storage and runtime setup
/// failures. Service termination and unfinished owned work retain their failure or
/// uncertainty status.
pub fn serve_blocking(uid: u32, controller_uid: u32, socket: PathBuf) -> Result<()> {
    use objc2_core_foundation::{CFRunLoop, CFString};

    // Run-loop modes are compared by name; this is kCFRunLoopDefaultMode.
    let default_mode = CFString::from_static_str("kCFRunLoopDefaultMode");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;

    let worker = std::thread::spawn(move || {
        let outcome = runtime.block_on(serve(uid, controller_uid, socket));
        // Dropping the runtime would wait for blocking work (a capture or
        // app listing stuck in a framework call) without limit.
        runtime.shutdown_timeout(SHUTDOWN);
        outcome
    });

    while !worker.is_finished() {
        CFRunLoop::run_in_mode(Some(&default_mode), 0.25, false);
    }

    worker
        .join()
        .map_err(|_| Error::new(Code::Daemon, "The helper daemon stopped unexpectedly"))?
}

/// Write one reply line, bounded in time.
async fn write<W: AsyncWrite + Unpin>(stream: &mut W, reply: &Value) -> Result<()> {
    let mut line = serde_json::to_vec(reply)?;
    line.push(b'\n');

    tokio::time::timeout(WRITE, async {
        stream.write_all(&line).await?;
        stream.flush().await
    })
    .await
    .map_err(|_| Error::new(Code::Transport, "The client stopped reading"))??;

    Ok(())
}

/// Ask macOS to log this login out, as its Apple menu's Log Out does but
/// without the confirmation: apps quit as at any log-out, and one may hold
/// it up (asking to save). loginwindow answers at once; only its error is a
/// refusal. A request that never got an answer may still be carried out.
async fn ask_to_log_out() -> Value {
    let asked = atlas_core::spawn::output_async(
        tokio::process::Command::new("/usr/bin/osascript")
            .args(["-e", "tell application \"loginwindow\" to «event aevtrlgo»"])
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true),
    );

    let unanswered = |why: &str| {
        interrupted(
            Code::Logout,
            format!("macOS did not answer the log-out request ({why})"),
        )
    };

    match tokio::time::timeout(Duration::from_secs(5), asked).await {
        Ok(Ok(output)) if output.status.success() => json!({"ok": true, "op": "logout"}),
        // Stopped by a signal: the log-out itself may have ended it.
        Ok(Ok(output)) if output.status.code().is_none() => unanswered("it was stopped"),
        Ok(Ok(output)) => {
            eprintln!(
                "osascript refused the log-out: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
            failure(Code::Logout, "macOS refused to log the helper out")
        }
        Ok(Err(_)) => failure(Code::Logout, "The log-out request could not start"),
        Err(_) => unanswered("it may be asking in the helper"),
    }
}

/// End this login at once, without asking its apps. Its process goes with
/// the session, so how it went is only logged.
fn force_log_out() {
    let uid = nix::unistd::getuid().as_raw();
    let started = atlas_core::spawn::spawn(
        std::process::Command::new("/bin/launchctl")
            .args(["bootout", &format!("gui/{uid}")])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null()),
    );
    match started {
        Ok(mut child) => {
            std::thread::spawn(move || {
                if let Ok(status) = child.wait()
                    && !status.success()
                {
                    eprintln!("launchctl bootout gui/{uid} failed: {status}");
                }
            });
        }
        Err(error) => eprintln!("launchctl bootout gui/{uid} did not start: {error}"),
    }
}

/// Serve one request. `controller` says whether it came from the controller:
/// the helper's own processes (an app under test among them) may only say
/// hello, as the setup window does there, never act with Atlas's
/// permissions.
pub(crate) async fn handle(
    stream: UnixStream,
    daemon: Arc<Daemon>,
    controller: bool,
    phase: watch::Sender<ConnectionPhase>,
) -> Result<()> {
    let peer = stream.peer_cred()?;
    // Capture the connection owner's start before reading a request, so a
    // passed socket or a reused PID cannot acquire another caller's ancestry.
    let caller = peer
        .pid()
        .and_then(|pid| atlas_core::process::info(pid).ok().flatten())
        .filter(|info| info.uid == peer.uid() && !info.zombie)
        .map(|info| info.instance);
    let (reader, writer) = stream.into_split();
    let asker = Asker {
        controller,
        host: Some((caller, peer.uid())),
        gate: None,
    };
    answer(Box::new(reader), Box::new(writer), daemon, asker, phase).await
}

/// Who asked, as their connection showed it.
pub(super) struct Asker {
    /// Whether it is the controller, not the helper's own processes.
    pub(super) controller: bool,
    /// The process that connected, as it was when it did, and its account:
    /// what a host session is verified against. None where the connection
    /// cannot show it.
    pub(super) host: Option<(Option<atlas_core::process::Instance>, u32)>,
    /// The lane of a machine's session the request came on, checked
    /// immediately before each change runs; none for the helper login.
    pub(super) gate: Option<Gate>,
}

/// What a change on a machine's session runs only while: its generation,
/// and its lane not cancelled.
#[derive(Clone)]
pub(super) struct Gate {
    pub(super) authority: atlas_session::authority::Authority,
    pub(super) lanes: Arc<atlas_session::lane::Lanes>,
    pub(super) lane: atlas_session::lane::LaneId,
    /// Whether agents are stopped on the desktop.
    pub(super) halted: watch::Receiver<Halt>,
    /// The Stops there had been when the lane was admitted: one since
    /// refuses its change, even resumed.
    pub(super) stops: u64,
    /// Whether a Stop holds what it admitted: always, but for the stage of
    /// an update, which only writes into the daemon's own folder and must
    /// reach a guest whose agents stay stopped.
    pub(super) held: bool,
}

impl Gate {
    /// The refusal of what it admitted now, when its lane was cancelled, the
    /// generation it came in is over, or agents were stopped since.
    fn refused(&self) -> Option<Value> {
        let halt = *self.halted.borrow();
        if self.held && (halt.halted || halt.stops != self.stops) {
            return Some(failure(Code::Stopped, "Agents are stopped on this desktop"));
        }
        if self.lanes.cancelled(self.lane) {
            return Some(failure(Code::Cancelled, "The change was cancelled"));
        }
        self.authority.check().err().map(refused)
    }

    /// Run `then` only while what it admitted may still run: agents not
    /// stopped since, its lane not cancelled, and its generation live,
    /// looked at without retiring it, so a caller may hold a lock the
    /// retirement hook takes. `then` must be short and never wait. The
    /// refusal when it may not, or when `then` says no.
    fn authorize(&self, then: impl FnOnce() -> bool) -> Option<Value> {
        let halt = *self.halted.borrow();
        if self.held && (halt.halted || halt.stops != self.stops) {
            return Some(failure(Code::Stopped, "Agents are stopped on this desktop"));
        }
        if self.lanes.cancelled(self.lane) {
            return Some(failure(Code::Cancelled, "The change was cancelled"));
        }
        match self.authority.if_live(|| Ok(then())) {
            Ok(true) => None,
            Ok(false) => Some(failure(Code::Cancelled, "The change was ended")),
            Err(error) => Some(refused(error)),
        }
    }

    /// Completes once the work it admitted may no longer run: its generation
    /// is over, or agents were stopped since, a Resume after or not.
    fn over(&self) -> impl Future<Output = ()> + Send + 'static {
        let authority = self.authority.clone();
        let mut halted = self.halted.clone();
        let (stops, held) = (self.stops, self.held);
        async move {
            tokio::select! {
                () = authority.ended() => {}
                _ = halted.wait_for(|halt| held && halt.stops > stops) => {}
            }
        }
    }
}

impl Asker {
    /// The refusal of a change now, when its lane was cancelled or the
    /// generation it came in is over.
    fn refused_now(&self) -> Option<Value> {
        self.gate.as_ref()?.refused()
    }

    /// The refusal of `request` on a machine's session: there, a change is
    /// only a command, until the others keep their ownership to the end.
    fn unavailable(&self, request: &Request, phase: ConnectionPhase) -> Option<Value> {
        let changes = matches!(phase, ConnectionPhase::Mutation(_));
        (self.gate.is_some() && changes && !matches!(request, Request::Run { .. }))
            .then(|| failure(Code::Platform, "This desktop cannot make that change yet"))
    }
}

/// Serve the one request on `reader`, answering on `writer`.
pub(super) async fn answer(
    mut reader: Reader,
    mut writer: Writer,
    daemon: Arc<Daemon>,
    asker: Asker,
    phase: watch::Sender<ConnectionPhase>,
) -> Result<()> {
    let admitted = admit(&mut reader, &daemon, asker.controller).await;
    phase.send_replace(
        admitted
            .as_ref()
            .map_or(ConnectionPhase::Refusal, |(_, policy, _)| policy.phase),
    );
    let (request, policy, remembering) = match admitted {
        Ok(admitted) => admitted,
        Err((refusal, true)) => return stream::write_header(&mut writer, &refusal).await,
        Err((refusal, false)) => return write(&mut writer, &refusal).await,
    };

    // A change runs only while the generation it came in does.
    let refusal = asker.unavailable(&request, policy.phase).or_else(|| {
        matches!(policy.phase, ConnectionPhase::Mutation(_))
            .then(|| asker.refused_now())
            .flatten()
    });
    if let Some(refusal) = refusal {
        let refusal = keep(&daemon, remembering, &refusal).await;
        return write(&mut writer, &refusal).await;
    }

    let reply = match request {
        Request::Stream {
            max_dimension,
            pin,
            backend,
        } => return stream::serve(daemon, reader, writer, max_dimension, pin, backend).await,
        Request::Frame { max_dimension, pin } => {
            return stream::one(daemon, reader, writer, max_dimension, pin).await;
        }
        Request::SessionProof { challenge } => proof::query(&daemon.proof_worker, &challenge)
            .await
            .unwrap_or_else(refused),
        Request::StreamDrainStatus {
            daemon_pid,
            producer_generation,
            query_id,
            stop_request,
        } => stream::drain_status(
            &daemon.source,
            daemon_pid,
            producer_generation,
            &query_id,
            stop_request,
        ),
        Request::HostSession(claims) => match asker.host {
            Some((caller, uid)) => tokio::task::spawn_blocking(move || {
                atlas_core::host::verification::verify(&claims, caller, uid)
            })
            .await
            .unwrap_or_else(|_| failure(Code::State, "The caller's host could not be verified")),
            None => failure(
                Code::Permission,
                "A host session cannot be verified over this connection",
            ),
        },
        // The census waits for the jobs' lock, and the facts look up the
        // account and read the display: neither holds a runtime thread.
        Request::Hello => {
            let daemon = daemon.clone();
            tokio::task::spawn_blocking(move || daemon.hello())
                .await
                .unwrap_or_else(|_| failure(Code::Daemon, "The helper could not say hello"))
        }
        Request::Apps { .. } => match tokio::task::spawn_blocking(|| {
            atlas_native::native::processes::snapshot(
                || Ok(apps::running()),
                |app| i64::from(app.pid),
            )
        })
        .await
        {
            Ok(Ok((apps, starts))) => json!({"ok": true, "apps": apps, "starts": starts}),
            Ok(Err(error)) => json!(error),
            Err(_) => failure(Code::Apps, "Running applications could not be listed"),
        },
        Request::Windows { dock, .. } => serve_windows(daemon.clone(), dock, policy).await,
        Request::Screenshot {
            max_dimension,
            per_point,
            crop,
        } => {
            screenshot(
                &daemon.screen,
                daemon.source.clone(),
                max_dimension,
                per_point,
                crop,
            )
            .await
        }
        Request::Run { argv, timeout, .. } => {
            let connection = (reader, writer, remembering);
            let ran = Ran {
                argv,
                timeout,
                policy,
                asker,
            };
            return serve_run(daemon, ran, connection, phase).await;
        }
        Request::Restart => {
            return acknowledge(&mut writer, "restart", remembering, || {
                daemon.restarting.notify_one();
            })
            .await;
        }
        // Answered before the log-out ends this process with the session.
        Request::Logout { force: true } => {
            return acknowledge(&mut writer, "logout", remembering, force_log_out).await;
        }
        Request::Logout { force: false } => ask_to_log_out().await,
        Request::Job(request) => match &daemon.jobs {
            Some(jobs) => jobs.serve(request).await,
            None => failure(
                Code::Daemon,
                "This helper daemon cannot keep background jobs; run atlas restart",
            ),
        },
        Request::Read(request) => return serve_read(&daemon, request, policy, writer).await,
        // Setting it waits briefly for the mute to follow; reading it only
        // says what the mute last reported.
        Request::Sound { muted } => {
            let daemon = daemon.clone();
            tokio::task::spawn_blocking(move || {
                let set = match muted {
                    Some(muted) => daemon.sound.set(muted),
                    None => Set::Done(daemon.sound.report()),
                };
                let report = match set {
                    Set::Done(report) => report,
                    Set::Unsaved => {
                        return failure(
                            Code::Daemon,
                            "The helper could not save this choice, so nothing changed",
                        );
                    }
                    Set::Uncertain => {
                        return interrupted(
                            Code::Daemon,
                            "The helper did not confirm its sound changed; run atlas sound to see",
                        );
                    }
                };
                let mut reply = report.json();
                reply["ok"] = json!(true);
                reply["op"] = json!("sound");
                reply
            })
            .await
            .unwrap_or_else(|_| interrupted(Code::Daemon, "The helper's sound could not be set"))
        }
        Request::Leftovers => match &daemon.supervising {
            // The helper login keeps its own ledger, on the station.
            None => failure(
                Code::Platform,
                "Only a machine desktop's daemon says what work it left",
            ),
            Some(_) => {
                let asking = daemon.clone();
                let leftovers = tokio::task::spawn_blocking(move || asking.leftovers())
                    .await
                    .unwrap_or("unknown");
                json!({"ok": true, "op": "leftovers", "leftovers": leftovers})
            }
        },
        Request::Copies { copy, after } => match daemon.stages.clone() {
            // The helper login's copies are the station's, in its shared folder.
            None => failure(
                Code::Platform,
                "Only a machine desktop keeps copies of its own",
            ),
            Some(stages) => tokio::task::spawn_blocking(move || match stages.root() {
                Some(root) => crate::machine::stage::copies::inventory(
                    root,
                    &stages,
                    copy.as_ref(),
                    after.as_ref(),
                ),
                None => failure(Code::State, "The desktop's folder for copies is not ready"),
            })
            .await
            .unwrap_or_else(|_| failure(Code::Io, "The desktop's copies could not be listed")),
        },
        // A machine desktop removes a copy on the lane of its change, before
        // here; the helper login has none to remove.
        Request::RemoveCopy { .. } => failure(
            Code::Platform,
            "Only a machine desktop removes copies of its own",
        ),
        // A machine desktop updates its Atlas on the lane of its change.
        Request::Update { .. } => failure(
            Code::Platform,
            "Only a machine desktop updates its own Atlas this way",
        ),
        Request::Outcome { change } => match &daemon.kept {
            // The disk outlives this process: it says, whatever was asked before.
            Some(kept) => {
                let kept = kept.clone();
                tokio::task::spawn_blocking(move || kept.outcome(&change))
                    .await
                    .map_err(|_| Error::new(Code::State, "The change records could not be read."))
                    .and_then(|outcome| outcome)
                    .map_or_else(refused, |outcome| outcome.answer())
            }
            None => daemon.outcomes.of(&change).answer(),
        },
    };

    remember(remembering, &reply);
    write(&mut writer, &reply).await
}

/// [`remember`], and on a machine's disk too before it is answered, off
/// the runtime's threads: the reply to send. One whose outcome could not
/// be kept is never sent as it is: the change stays intended there, so it
/// goes as uncertain, and what it came to stays unknown.
async fn keep(daemon: &Daemon, remembering: Option<Remembering>, reply: &Value) -> Value {
    let mut reply = reply.clone();
    if let (Some(kept), Some(change)) = (
        daemon.kept.clone(),
        remembering.as_ref().and_then(Remembering::change).cloned(),
    ) {
        let keeping = reply.clone();
        let done = tokio::task::spawn_blocking(move || kept.done(&change, &keeping)).await;
        if !matches!(done, Ok(Ok(()))) {
            reply = interrupted(
                Code::Uncertain,
                "The change ended, but what it came to could not be kept",
            );
        }
    }
    remember(remembering, &reply);
    reply
}

/// Remember `reply` as what a change that named itself came to: one lost
/// on its way can then be asked for.
fn remember(remembering: Option<Remembering>, reply: &Value) {
    if let Some(remembering) = remembering {
        remembering.done(reply);
    }
}

/// The connection's one request, its policy and, for a change that named
/// itself, the memory of it; or the reply refusing it and whether that
/// reply is framed as a stream's, in the order of refusal: unreadable,
/// malformed, from the helper's own account, a change this daemon already
/// took, then the policy's own checks. A change those checks refuse is
/// remembered as refused.
async fn admit(
    reader: &mut Reader,
    daemon: &Daemon,
    controller: bool,
) -> std::result::Result<(Request, Policy, Option<Remembering>), (Value, bool)> {
    let line = endpoint::read_line(reader, MAX_REQUEST)
        .await
        .map_err(|error| (refused(error), false))?;
    let Asked { request, change } = protocol::parse(&line).map_err(|refusal| (refusal, false))?;
    let framed = matches!(request, Request::Stream { .. } | Request::Frame { .. });
    let policy = request.policy();
    if !controller && !policy.open_to_helper() {
        return Err((
            failure(
                Code::Permission,
                "Only the account Atlas works for may ask the helper that",
            ),
            framed,
        ));
    }

    // Only a change is remembered: a read may be asked again.
    let remembering = match change {
        Some(change) if matches!(policy.phase, ConnectionPhase::Mutation(_)) => Some(
            daemon
                .outcomes
                .begin(change)
                .ok_or_else(|| (already_asked(), framed))?,
        ),
        Some(_) | None => None,
    };

    match policy.check(
        daemon.screen.console,
        daemon.screen.on_console,
        daemon.screen.session,
    ) {
        Some(refusal) => Err((keep(daemon, remembering, &refusal).await, framed)),
        None => Ok((request, policy, remembering)),
    }
}

/// The refusal of a change this daemon was already asked for: it is never
/// carried out twice, and `outcome` says what the first came to.
fn already_asked() -> Value {
    failure(
        Code::BadRequest,
        "The helper was already asked for this change; ask for its outcome",
    )
}

/// Reply that `op` is done, then do it, whether or not the reply got through.
/// It is remembered as done only once it is: a handler cut while it writes
/// did nothing, and remembers nothing.
async fn acknowledge(
    writer: &mut Writer,
    op: &str,
    remembering: Option<Remembering>,
    effect: impl FnOnce(),
) -> Result<()> {
    let reply = json!({"ok": true, "op": op});
    let written = write(writer, &reply).await;
    effect();
    remember(remembering, &reply);
    written
}

/// Retain the daemon while the window worker checks admission and reads its
/// snapshot, even if the connection disappears before the worker finishes.
async fn serve_windows(daemon: Arc<Daemon>, dock: bool, policy: Policy) -> Value {
    tokio::task::spawn_blocking(move || {
        let guard = || {
            policy.check(
                daemon.screen.console,
                daemon.screen.on_console,
                daemon.screen.session,
            )
        };
        if let Some(refusal) = guard() {
            return refusal;
        }
        let reply =
            match atlas_native::native::processes::snapshot(daemon.screen.windows, |w| w.pid) {
                Ok((windows, starts)) => {
                    let mut reply = json!({"ok": true, "windows": windows, "starts": starts});
                    if dock {
                        reply["dock"] = json!((daemon.screen.dock)(&windows));
                    }
                    reply
                }
                Err(error) => json!(error),
            };
        guard().unwrap_or(reply)
    })
    .await
    .unwrap_or_else(|_| failure(Code::Windows, "The helper window list is unavailable"))
}

/// Register a streamed command before permitting execution, retaining its writer
/// and child cleanup independently of the socket handler's lifetime.
/// A command asked for, and who asked.
struct Ran {
    argv: Vec<String>,
    timeout: Duration,
    policy: Policy,
    asker: Asker,
}

async fn serve_run(
    daemon: Arc<Daemon>,
    ran: Ran,
    connection: (Reader, Writer, Option<Remembering>),
    phase: watch::Sender<ConnectionPhase>,
) -> Result<()> {
    let (mut handler, outcome) = oneshot::channel();
    let (start, registered) = oneshot::channel();
    let runner = daemon.clone();
    let mut runs = daemon.runs.lock().await;
    // Read under the lock that closing takes: a command either registers
    // before the daemon closes, which then waits for it, or is refused here
    // with nothing run.
    if daemon.commands_closed.load(Ordering::Acquire) {
        drop(runs);
        let (_, mut writer, remembering) = connection;
        let reply = failure(
            Code::Busy,
            "The helper daemon is restarting; try again in a moment",
        );
        let reply = keep(&daemon, remembering, &reply).await;
        return write(&mut writer, &reply).await;
    }
    runs.spawn(async move {
        if registered.await.is_err() {
            return Ok(());
        }
        let (mut reader, writer, remembering) = connection;
        let result = run_registered(
            &runner,
            &ran,
            (&mut reader, writer, remembering),
            &phase,
            &mut handler,
        )
        .await;
        match handler.send(result) {
            Ok(()) => Ok(()),
            Err(Err(error)) => Err(error),
            Err(Ok(())) => Err(handler_left()),
        }
    });
    drop(runs);
    // Registration precedes execution, with no cancellation point
    // between recording the task and allowing it to start.
    let _ = start.send(());
    outcome.await.map_err(|error| {
        Error::new(Code::Job, "Daemon run did not report its outcome")
            .uncertain()
            .with_source(error)
    })?
}

/// The refusal of a command on a machine's session when nothing could
/// supervise it, as none runs there unsupervised, or while work left
/// earlier is not known to have ended.
fn unsupervised(daemon: &Daemon, asker: &Asker) -> Option<Value> {
    asker.gate.as_ref()?;
    if daemon.supervising.is_none() {
        return Some(failure(
            Code::Platform,
            "This desktop cannot supervise commands, so none is run",
        ));
    }
    uncertain(&daemon.keepers)
}

impl Daemon {
    /// Recover what earlier daemons of this machine left: whether all of
    /// it is done with. See [`crate::machine::processes::recover`].
    /// Whether work this machine's daemons started may still run, as the
    /// station asks before it takes the desktop: `running`, `none` or
    /// `unknown`, which holds as running does. Another boot's records are
    /// forgotten before `none` is said. Blocking.
    fn leftovers(&self) -> &'static str {
        let Some(supervising) = &self.supervising else {
            return "unknown";
        };
        let (Ok(work), Ok(boot)) = (supervising.kept.kept(), atlas_core::process::boot()) else {
            return "unknown";
        };
        if self.keepers.uncertain() {
            return "unknown";
        }
        if self.keepers.keeping() {
            return "running";
        }
        let mut unproven = false;
        let mut other_boots = Vec::new();
        for supervised in work {
            if supervised.boot != boot {
                other_boots.push(supervised);
            } else if !supervised.supervisor.ended() {
                return "running";
            } else {
                let code = crate::machine::supervise::proof::left(
                    &supervising.proofs,
                    &supervised.supervisor,
                );
                unproven |= !matches!(
                    code,
                    Some(crate::machine::supervise::DONE | crate::machine::supervise::ENDED)
                );
            }
        }
        if unproven {
            return "unknown";
        }
        if !other_boots.is_empty() && supervising.kept.retire(&other_boots).is_err() {
            return "unknown";
        }
        "none"
    }

    /// [`Daemon::recover_published`] with no wait, at once or not at all: not
    /// done with when another recovery is under way. Blocking.
    pub(super) fn recover_now(&self, retiring: bool) -> bool {
        let Ok(_one) = self.keepers.recovering.try_lock() else {
            return false;
        };
        let resolved = self.recover(Duration::ZERO, retiring);
        self.keepers.orphans(!resolved);
        resolved
    }

    /// [`Daemon::recover`], and what it found told, under the one lock every
    /// recovery and every keeper handing over its record takes. Blocking.
    pub(super) fn recover_published(&self, wait: Duration, retiring: bool) -> bool {
        let _one = self.keepers.recovering.blocking_lock();
        let resolved = self.recover(wait, retiring);
        self.keepers.orphans(!resolved);
        resolved
    }

    fn recover(&self, wait: Duration, retiring: bool) -> bool {
        let Some(supervising) = &self.supervising else {
            return true;
        };
        crate::machine::processes::recover(
            &*supervising.kept,
            &supervising.proofs,
            |supervisor| self.keepers.owns(supervisor),
            |supervisor| !supervisor.ended(),
            wait,
            retiring,
        )
    }
}

/// The refusal of a change while work started earlier on a machine is not
/// known to have ended.
fn uncertain(keepers: &run::keeper::Keepers) -> Option<Value> {
    keepers.uncertain().then(|| {
        failure(
            Code::Stopped,
            "Work started earlier on this desktop is not known to have ended, so agents are stopped",
        )
    })
}

/// Admission a queued command must pass again, its execution, and the replies
/// that report it to `handler`, which losing cancels the command. What it
/// came to is remembered, for a command that named itself, before its final
/// reply is written.
async fn run_registered(
    daemon: &Daemon,
    ran: &Ran,
    (reader, mut writer, remembering): (&mut Reader, Writer, Option<Remembering>),
    phase: &watch::Sender<ConnectionPhase>,
    handler: &mut oneshot::Sender<Result<()>>,
) -> Result<()> {
    let Ran {
        argv,
        timeout,
        policy,
        asker,
    } = ran;
    // Queued behind others, it is checked again, its generation too.
    let refusal = policy
        .check(
            daemon.screen.console,
            daemon.screen.on_console,
            daemon.screen.session,
        )
        .or_else(|| asker.refused_now())
        .or_else(|| unsupervised(daemon, asker));
    if let Some(refusal) = refusal {
        phase.send_replace(ConnectionPhase::Refusal);
        let refusal = keep(daemon, remembering, &refusal).await;
        return write(&mut writer, &refusal).await;
    }
    // Events reach the client through their own writer, so a slow
    // client never stalls supervision of the command.
    let (events, queue) = mpsc::channel(run::EVENTS);
    let mut delivery = tokio::spawn(deliver(queue, writer, || Ok(())));
    // A Stop since it was admitted ends it, a Resume after or not.
    let stops = asker
        .gate
        .as_ref()
        .map_or_else(|| daemon.halted.borrow().stops, |gate| gate.stops);
    // The client never sends after its request. Losing either
    // the client or its handler cancels the owned command.
    let cancel = async {
        let mut byte = [0; 1];
        tokio::select! {
            _ = reader.read(&mut byte) => run::Cancel::Left,
            () = handler.closed() => run::Cancel::Left,
            () = daemon.stopped() => run::Cancel::Stopping,
            () = daemon.moved_away() => run::Cancel::Away(daemon.screen.console),
            () = daemon.stopped_since(stops) => run::Cancel::Halted,
        }
    };
    // Cancelled or retired during any wait before it starts, it never does.
    let unless = || asker.refused_now();
    let lease = asker
        .gate
        .as_ref()
        .zip(daemon.supervising.as_ref())
        .map(|(gate, supervising)| {
            let (still, keepers) = (gate.clone(), daemon.keepers.clone());
            run::Lease {
                supervisor: supervising.supervisor.clone(),
                proofs: supervising.proofs.clone(),
                generation: gate.authority.generation(),
                over: Box::pin(gate.over()),
                admitted: Box::new(move || still.refused().or_else(|| uncertain(&keepers))),
                kept: supervising.kept.clone(),
                keepers: daemon.keepers.clone(),
                stops: None,
                judged: None,
                job: None,
            }
        });
    let reply = run::run_unless(argv, *timeout, events, cancel, &unless, lease).await;
    let reply = keep(daemon, remembering, &reply).await;
    let delivered = tokio::select! {
        delivered = &mut delivery => delivered,
        () = handler.closed() => {
            // The command has already settled. Stop and join its writer too
            // when the handler is cancelled, rather than drain queued events
            // to a lost owner.
            delivery.abort();
            delivery.await
        }
    };
    let (mut writer, _) = writer_back(delivered)?;
    tokio::select! {
        written = write(&mut writer, &reply) => written,
        () = handler.closed() => Err(handler_left()),
    }
}

fn handler_left() -> Error {
    Error::new(
        Code::Transport,
        "The run handler left before observing completion",
    )
    .uncertain()
}

/// What a finished event writer handed back.
fn writer_back<T>(delivered: std::result::Result<Result<T>, JoinError>) -> Result<T> {
    delivered.map_err(|_| Error::new(Code::Transport, "The event writer failed"))?
}

/// Read part of a file for the person's side, its `data` events written as
/// they are read, so the daemon never holds more than a few of them. The
/// worker checks again, before it opens the file, before each part and
/// before the digest, that the helper is still off screen and in the
/// session it was when the request came, as a screenshot's worker does; a
/// part already queued is not sent once the helper is on screen.
async fn serve_read(
    daemon: &Daemon,
    request: ReadRequest,
    policy: Policy,
    mut writer: Writer,
) -> Result<()> {
    let screen = daemon.screen;
    let epoch = match screen.epoch() {
        Ok(epoch) => epoch,
        Err(error) => return write(&mut writer, &refused(error)).await,
    };

    // Off screen, and in the session and display it was in when the
    // request came: asked by the worker before it reads and by the writer
    // before it delivers each part already read.
    let guard = move || screen.guard(&epoch, policy);
    let (events, queue) = mpsc::channel(8);
    let delivery = tokio::spawn(deliver(queue, writer, guard.clone()));
    let uid = nix::unistd::getuid().as_raw();
    let reply = tokio::task::spawn_blocking(move || {
        guard()?;
        let Some(places) = read::Places::here() else {
            return Ok(failure(
                Code::Collect,
                "The helper's home could not be found",
            ));
        };
        Ok(read::read_at(
            &request,
            &places,
            uid,
            &guard,
            &mut |event| events.blocking_send(event).is_ok(),
            SystemTime::now(),
        ))
    })
    .await
    .map_or_else(
        |_| failure(Code::Collect, "The file could not be read"),
        |reply: std::result::Result<Value, Value>| reply.unwrap_or_else(|refusal| refusal),
    );
    let (mut writer, cut) = writer_back(delivery.await)?;
    write(&mut writer, &cut.unwrap_or(reply)).await
}

/// Write each event of `queue` to `writer` while `gate` allows it; the
/// first refusal ends delivery, and nothing still queued is written. The
/// writer, and the refusal that ended it if one did.
async fn deliver<W: AsyncWrite + Unpin>(
    mut queue: mpsc::Receiver<Value>,
    mut writer: W,
    gate: impl Fn() -> std::result::Result<(), Value>,
) -> Result<(W, Option<Value>)> {
    while let Some(event) = queue.recv().await {
        if let Err(refusal) = gate() {
            return Ok((writer, Some(refusal)));
        }
        write(&mut writer, &event).await?;
    }
    Ok((writer, None))
}

#[cfg(test)]
#[path = "serve_tests.rs"]
pub(super) mod tests;
