//! The helper daemon's socket: one request per connection, answered with
//! protocol v1 replies.
use super::capture::{Capture, Screen, screenshot};
use crate::daemon::{
    consent, facts,
    job::{self, Jobs},
    preferences, proof, read, run,
    sound::{Set, Sound},
};
use atlas_core::{
    error::{Code, Error, Result},
    protocol::{
        self, ConnectionPhase, MAX_REQUEST, Policy, ReadRequest, Request, failure, interrupted,
        refused,
    },
    session::validate_target_uid,
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
    net::{
        UnixListener, UnixStream,
        unix::{OwnedReadHalf, OwnedWriteHalf},
    },
    signal::unix::{Signal, SignalKind, signal},
    sync::{Mutex, Notify, Semaphore, mpsc, oneshot, watch},
    task::{Id, JoinError, JoinHandle, JoinSet},
};

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
    /// The one live capture source; held through its native source's drain.
    source: Arc<capture::source::Source>,
    /// The persistent child that answers session-proof requests.
    proof_worker: Mutex<Option<proof::Worker>>,
    /// Whether the helper's apps can be heard on the person's speakers.
    sound: Sound,
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
                session,
            },
            restarting: Notify::new(),
            stopping: watch::Sender::new(false),
            jobs: None,
            runs: Mutex::new(JoinSet::new()),
            source: Arc::new(capture::source::Source::new(capture::stop_display_stream)),
            proof_worker: Mutex::new(None),
            sound: Sound::detached(),
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
        atlas_core::login_log::mark(&format!("daemon.start uid={uid}"));
        preferences::type_exactly();
        preferences::keep_desktop()?;

        let mut daemon = Self::new(
            manager,
            capture::main_display,
            facts::on_console,
            atlas_native::native::session_info,
        );
        // The helper's apps go silent before any agent acts, as the person
        // last chose.
        daemon.sound = Sound::start(uid, std::env::var_os("HOME").map(PathBuf::from));
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
            atlas_core::login_log::mark("daemon.firstHello");
        }
        match self.jobs.as_deref().map(Jobs::census).transpose() {
            Ok(census) => {
                let mut hello = facts::hello(&self.manager, census);
                hello["sound"] = self.sound.report().json();
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
        let marker = crate::helper_setup::Layout::new(uid, PathBuf::from(home)).stop_marker();
        Some(tokio::spawn(job::supervise(
            self.jobs.clone()?,
            marker,
            self.screen.on_console,
            self.stopping.subscribe(),
        )))
    }

    fn fail_supervision(&self) {
        if let Some(jobs) = &self.jobs {
            jobs.fail_supervision();
        }
    }

    /// Completes once the daemon has begun to stop.
    async fn stopped(&self) {
        let _ = self
            .stopping
            .subscribe()
            .wait_for(|stopping| *stopping)
            .await;
    }

    /// Completes once the helper is on the console, checked about once a
    /// second.
    async fn on_screen(&self) {
        loop {
            tokio::time::sleep(CONSOLE_CHECK).await;

            if (self.screen.on_console)() {
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
    let drained = drain_connections(&mut serving.tasks, &mut serving.connections, deadline).await;
    let runs = drain_runs(&daemon, deadline).await;

    asking.abort();
    recording.abort();
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
    let (mut reader, mut writer) = stream.into_split();

    let admitted = admit(&mut reader, &daemon, controller).await;
    phase.send_replace(
        admitted
            .as_ref()
            .map_or(ConnectionPhase::Refusal, |(_, policy)| policy.phase),
    );
    let (request, policy) = match admitted {
        Ok(admitted) => admitted,
        Err((refusal, true)) => return stream::write_header(&mut writer, &refusal).await,
        Err((refusal, false)) => return write(&mut writer, &refusal).await,
    };

    let reply = match request {
        Request::Stream {
            max_dimension,
            pin,
            backend,
        } => return stream::serve(daemon, reader, writer, max_dimension, pin, backend).await,
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
        Request::HostSession(claims) => tokio::task::spawn_blocking(move || {
            atlas_core::host::verification::verify(&claims, caller, peer.uid())
        })
        .await
        .unwrap_or_else(|_| failure(Code::State, "The caller's host could not be verified")),
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
            return serve_run(daemon, argv, timeout, policy, (reader, writer), phase).await;
        }
        Request::Restart => {
            return acknowledge(&mut writer, "restart", || daemon.restarting.notify_one()).await;
        }
        // Answered before the log-out ends this process with the session.
        Request::Logout { force: true } => {
            return acknowledge(&mut writer, "logout", force_log_out).await;
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
    };

    write(&mut writer, &reply).await
}

/// The connection's one request and its policy, or the reply refusing it and
/// whether that reply is framed as a stream's, in the order of refusal:
/// unreadable, malformed, from the helper's own account, then the policy's
/// own checks.
async fn admit(
    reader: &mut OwnedReadHalf,
    daemon: &Daemon,
    controller: bool,
) -> std::result::Result<(Request, Policy), (Value, bool)> {
    let line = endpoint::read_line(reader, MAX_REQUEST)
        .await
        .map_err(|error| (refused(error), false))?;
    let request = protocol::parse(&line).map_err(|refusal| (refusal, false))?;
    let framed = matches!(request, Request::Stream { .. });
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

    match policy.check(daemon.screen.on_console, daemon.screen.session) {
        Some(refusal) => Err((refusal, framed)),
        None => Ok((request, policy)),
    }
}

/// Reply that `op` is done, then do it, whether or not the reply got through.
async fn acknowledge(writer: &mut OwnedWriteHalf, op: &str, effect: impl FnOnce()) -> Result<()> {
    let written = write(writer, &json!({"ok": true, "op": op})).await;
    effect();
    written
}

/// Retain the daemon while the window worker checks admission and reads its
/// snapshot, even if the connection disappears before the worker finishes.
async fn serve_windows(daemon: Arc<Daemon>, dock: bool, policy: Policy) -> Value {
    tokio::task::spawn_blocking(move || {
        let guard = || policy.check(daemon.screen.on_console, daemon.screen.session);
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
async fn serve_run(
    daemon: Arc<Daemon>,
    argv: Vec<String>,
    timeout: Duration,
    policy: Policy,
    connection: (OwnedReadHalf, OwnedWriteHalf),
    phase: watch::Sender<ConnectionPhase>,
) -> Result<()> {
    let (mut handler, outcome) = oneshot::channel();
    let (start, registered) = oneshot::channel();
    let runner = daemon.clone();
    let mut runs = daemon.runs.lock().await;
    runs.spawn(async move {
        if registered.await.is_err() {
            return Ok(());
        }
        let (mut reader, writer) = connection;
        let result = run_registered(
            &runner,
            &argv,
            timeout,
            policy,
            (&mut reader, writer),
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

/// Admission a queued command must pass again, its execution, and the replies
/// that report it to `handler`, which losing cancels the command.
async fn run_registered(
    daemon: &Daemon,
    argv: &[String],
    timeout: Duration,
    policy: Policy,
    (reader, mut writer): (&mut OwnedReadHalf, OwnedWriteHalf),
    phase: &watch::Sender<ConnectionPhase>,
    handler: &mut oneshot::Sender<Result<()>>,
) -> Result<()> {
    if let Some(refusal) = policy.check(daemon.screen.on_console, daemon.screen.session) {
        phase.send_replace(ConnectionPhase::Refusal);
        return write(&mut writer, &refusal).await;
    }
    // Events reach the client through their own writer, so a slow
    // client never stalls supervision of the command.
    let (events, queue) = mpsc::channel(run::EVENTS);
    let mut delivery = tokio::spawn(deliver(queue, writer, || Ok(())));
    // The client never sends after its request. Losing either
    // the client or its handler cancels the owned command.
    let cancel = async {
        let mut byte = [0; 1];
        tokio::select! {
            _ = reader.read(&mut byte) => run::Cancel::Left,
            () = handler.closed() => run::Cancel::Left,
            () = daemon.stopped() => run::Cancel::Stopping,
            () = daemon.on_screen() => run::Cancel::OnConsole,
        }
    };
    let reply = run::run(argv, timeout, events, cancel).await;
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
    mut writer: OwnedWriteHalf,
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
pub(super) mod tests;
