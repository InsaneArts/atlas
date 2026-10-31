//! The helper daemon's socket: one request per connection, answered with
//! protocol v1 replies.
use crate::{
    daemon::{
        crop, facts,
        job::{self, Jobs},
        protocol::{self, Crop, JobRequest, MAX_REQUEST, ReadRequest, Region, Request, failure},
        read, run,
    },
    error::{Code, Error, Result},
    native::{apps, capture, endpoint, windows},
    session::{InputEpoch, validate_target_uid},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::UnixStream,
    signal::unix::{SignalKind, signal},
    sync::{Notify, Semaphore, mpsc, watch},
    task::JoinSet,
};

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

/// Takes a screenshot of a display, or of an area of it in display points:
/// the real display in the daemon, a synthetic image in tests, so ordinary
/// tests never capture the desktop they run on.
type Capture = fn(u32, Option<CGRect>, Option<u32>) -> Result<capture::Shot>;

struct Daemon {
    /// launchd's session type, read once at start.
    manager: String,
    capture: Capture,
    /// The session's on-screen windows, front to back.
    windows: fn() -> Result<Vec<windows::Window>>,
    /// Bundle identity is read in this helper session, never the controller's.
    bundle_id: fn(i64) -> Option<String>,
    /// Where the Dock in a window list takes clicks.
    dock: fn(&[windows::Window]) -> windows::Dock,
    /// Whether the helper's session is on the console right now.
    on_console: fn() -> bool,
    session: fn() -> Result<Value>,
    restarting: Notify,
    /// Set when the daemon stops, so running commands are ended first.
    stopping: watch::Sender<bool>,
    /// The background jobs it supervises; none when their folder could not
    /// be opened, and then it offers none.
    jobs: Option<Arc<Jobs>>,
}

impl Daemon {
    fn new(
        manager: String,
        capture: Capture,
        on_console: fn() -> bool,
        session: fn() -> Result<Value>,
    ) -> Self {
        Self {
            manager,
            capture,
            windows: windows::windows,
            bundle_id: apps::bundle_id,
            dock: read_dock,
            on_console,
            session,
            restarting: Notify::new(),
            stopping: watch::Sender::new(false),
            jobs: None,
        }
    }

    /// The daemon's hello: its facts, and the background jobs it keeps.
    fn hello(&self) -> Value {
        facts::hello(&self.manager, self.jobs.as_deref().map(Jobs::census))
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

            if (self.on_console)() {
                return;
            }
        }
    }
}

/// Refuse to start outside the helper's own logged-in graphical session.
fn require_graphical_session(uid: u32, manager: &str) -> Result<()> {
    let proof = crate::native::session_info()?;
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
pub async fn serve(uid: u32, controller_uid: u32, socket: PathBuf) -> Result<()> {
    validate_target_uid(uid, controller_uid)?;

    if nix::unistd::getuid().as_raw() != uid {
        return Err(Error::new(
            Code::Identity,
            "The helper daemon must run as the configured helper account",
        ));
    }

    let manager = facts::manager_name();
    require_graphical_session(uid, &manager)?;
    crate::login_log::mark(&format!("daemon.start uid={uid}"));
    facts::type_exactly();

    let mut daemon = Daemon::new(
        manager,
        capture::main_display,
        facts::on_console,
        crate::native::session_info,
    );
    // Jobs are kept beside the socket; the jobs a daemon before this one
    // started are found again by their records.
    if let (Some(folder), Ok(launcher)) = (socket.parent(), std::env::current_exe()) {
        match Jobs::open(folder, launcher, uid) {
            Ok(jobs) => daemon.jobs = Some(Arc::new(jobs)),
            Err(error) => eprintln!("Background jobs are unavailable: {}", error.error),
        }
    }
    let daemon = Arc::new(daemon);

    let (listener, _endpoint) = endpoint::bind(
        &socket,
        uid,
        "helper daemon",
        endpoint::Directory::Private {
            controller: controller_uid,
        },
    )
    .await?;

    // A Screen Recording grant reaches this process only through a restart.
    let restarter = daemon.clone();
    let consent = tokio::spawn(facts::request_consent_on_arrival(
        controller_uid,
        move || {
            restarter.restarting.notify_one();
        },
    ));
    let watcher = daemon.clone();
    let recording = tokio::spawn(facts::restart_for_recording(move || {
        watcher.restarting.notify_one();
    }));
    // The person's Stop is seen through the input service's saved marker,
    // where the helper's layout puts it: the configuration's input socket
    // comes from the same layout.
    let marker = std::env::var_os("HOME")
        .map(|home| crate::setup::helper::Layout::new(uid, PathBuf::from(home)).stop_marker());
    let supervision = match (&daemon.jobs, marker) {
        (Some(jobs), Some(marker)) => Some(tokio::spawn(job::supervise(
            jobs.clone(),
            marker,
            daemon.on_console,
        ))),
        _ => None,
    };

    let slots = Arc::new(Semaphore::new(CONNECTIONS));
    let mut tasks = JoinSet::new();

    // Both listeners exist before the loop, so a signal that arrives between
    // two passes is still delivered.
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;

    let outcome = loop {
        tokio::select! {
            // Stopped from outside (macOS's Quit & Reopen after a new
            // permission, say): exit unsuccessfully so launchd starts it
            // again. Uninstalling unloads the job first, so it stays down.
            _ = interrupt.recv() => {
                break Err(Error::new(Code::Restart, "Interrupted; launchd starts it again"));
            }
            _ = terminate.recv() => {
                break Err(Error::new(Code::Restart, "Stopped; launchd starts it again"));
            }
            () = daemon.restarting.notified() => {
                break Err(Error::new(Code::Restart, "Restarting at the controller's request"));
            }
            Some(_) = tasks.join_next(), if !tasks.is_empty() => {}
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    continue;
                };

                let Ok(peer) = stream.peer_cred() else {
                    continue;
                };
                if peer.uid() != controller_uid && peer.uid() != uid {
                    continue;
                }

                let Ok(slot) = slots.clone().try_acquire_owned() else {
                    // Nothing ran: say so, rather than hang up in a way a
                    // client must treat as uncertain.
                    endpoint::turn_away(stream, &failure(
                        Code::Busy,
                        "The helper is serving too many requests; try again",
                    ));
                    continue;
                };

                let daemon = daemon.clone();
                let controller = peer.uid() == controller_uid;
                tasks.spawn(async move {
                    let _slot = slot;
                    let _ = handle(stream, daemon, controller).await;
                });
            }
        }
    };

    // Running commands are ended (TERM, then KILL) before the daemon exits,
    // so a restart never leaves one unsupervised. Background jobs go on: the
    // next daemon finds them again by their records.
    daemon.stopping.send_replace(true);
    let drained =
        tokio::time::timeout(STOP, async { while tasks.join_next().await.is_some() {} }).await;
    if drained.is_err() {
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
    }

    consent.abort();
    recording.abort();
    if let Some(supervision) = supervision {
        supervision.abort();
    }
    outcome
}

/// Serve on a Tokio runtime in a worker thread while this (main) thread runs
/// the Core Foundation run loop, which AppKit needs to keep its view of
/// running applications current.
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

/// An error as a protocol reply.
fn refused(error: Error) -> Value {
    failure(error.code, error.error)
}

/// Whether `request` may run in the session's current state: commands,
/// captures, new background jobs and reads of files never run while the
/// helper is on the console, where they would act on a desktop a person is
/// using. Stopping, showing and reading a job's log go on: ending things is
/// what the helper on screen asks for.
fn allowed(request: &Request, on_console: bool) -> Option<Value> {
    let acts = matches!(
        request,
        Request::Run { .. }
            | Request::Screenshot { .. }
            | Request::Logout { .. }
            | Request::Job(JobRequest::Start { .. })
            | Request::Read(_)
    );

    (acts && on_console).then(|| {
        failure(
            Code::OnConsole,
            "The helper is on the screen; switch back to your own account first",
        )
    })
}

/// The refusal of a request pinned to a helper session other than this
/// one, as `session` reads it now; none for a request with no pin.
fn pinned_elsewhere(request: &Request, session: fn() -> Result<Value>) -> Option<Value> {
    let pin = match request {
        Request::Apps { pin }
        | Request::Run { pin, .. }
        | Request::Job(JobRequest::Start { pin, .. })
        | Request::Read(ReadRequest { pin, .. }) => (*pin)?,
        _ => return None,
    };
    let here = session()
        .and_then(|proof| InputEpoch::from_proof(&proof, std::process::id()))
        .map(|epoch| protocol::SessionPin {
            security_session_id: epoch.security_session_id,
            display_id: epoch.display_id,
        });

    match here {
        Ok(here) if here == pin => None,
        Ok(_) => Some(failure(
            Code::SessionChanged,
            "The helper changed since Atlas checked it; look again",
        )),
        Err(error) => Some(refused(error)),
    }
}

/// Ask macOS to log this login out, as its Apple menu's Log Out does but
/// without the confirmation: apps quit as at any log-out, and one may hold
/// it up (asking to save). loginwindow answers at once; only its error is a
/// refusal. A request that never got an answer may still be carried out.
async fn ask_to_log_out() -> Value {
    let asked = tokio::process::Command::new("/usr/bin/osascript")
        .args(["-e", "tell application \"loginwindow\" to «event aevtrlgo»"])
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();

    let unanswered = |why: &str| {
        let mut reply = failure(
            Code::Logout,
            format!("macOS did not answer the log-out request ({why})"),
        );
        reply["uncertain"] = json!(true);
        reply
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
    let started = std::process::Command::new("/bin/launchctl")
        .args(["bootout", &format!("gui/{uid}")])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .spawn();
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

/// Where the Dock in `windows` takes clicks, read through Accessibility.
fn read_dock(windows: &[windows::Window]) -> windows::Dock {
    crate::native::ax::dock(windows, crate::native::ax::DOCK_TIMEOUT)
}

/// Windows that lie entirely outside the main display.
fn off_display_windows(windows: &[windows::Window]) -> Vec<Value> {
    let (x, y, width, height) = facts::display_bounds();
    windows
        .iter()
        .filter(|window| window.layer == 0)
        .filter(|window| {
            let b = &window.bounds;
            b.x + b.width <= x || b.y + b.height <= y || b.x >= x + width || b.y >= y + height
        })
        .map(
            |window| json!({"number": window.number, "owner": window.owner, "title": window.title}),
        )
        .collect()
}

/// A region as the rectangle a capture takes.
fn rect(region: Region) -> CGRect {
    CGRect::new(
        CGPoint::new(f64::from(region.x), f64::from(region.y)),
        CGSize::new(f64::from(region.width), f64::from(region.height)),
    )
}

/// What a cropped screenshot's reply adds: the area captured and, for an
/// app, which app and what lies in front of it.
fn crop_keys(
    area: Region,
    app: Option<(i64, crop::AppCrop)>,
    bundle_id: fn(i64) -> Option<String>,
) -> serde_json::Map<String, Value> {
    let mut keys = serde_json::Map::new();
    keys.insert("region".into(), json!(area));

    if let Some((pid, found)) = app {
        let overlapping: Vec<Value> = found
            .overlapping
            .iter()
            .map(|window| {
                json!({
                    "owner": window.owner,
                    "pid": window.pid,
                    "title": window.title,
                    "bounds": window.bounds,
                })
            })
            .collect();
        keys.insert(
            "app".into(),
            json!({"pid": pid, "name": found.name, "bundleId": bundle_id(pid)}),
        );
        keys.insert("overlapping".into(), json!(overlapping));
    }

    keys
}

async fn screenshot(
    daemon: &Daemon,
    max_dimension: Option<u32>,
    per_point: bool,
    crop: Option<Crop>,
) -> Value {
    let capture = daemon.capture;
    let list = daemon.windows;
    let bundle_id = daemon.bundle_id;
    let dock = daemon.dock;
    let on_console = daemon.on_console;
    let session = daemon.session;
    let epoch = match session().and_then(|proof| InputEpoch::from_proof(&proof, std::process::id()))
    {
        Ok(epoch) => epoch,
        Err(error) => return refused(error),
    };
    let taken = tokio::task::spawn_blocking(move || {
        let request = Request::Screenshot {
            max_dimension,
            per_point,
            crop,
        };
        let validate = || {
            if let Some(refusal) = allowed(&request, on_console()) {
                return Err(refusal);
            }
            let current = InputEpoch::from_proof(&session().map_err(refused)?, std::process::id())
                .map_err(refused)?;
            if current != epoch {
                return Err(failure(
                    Code::SessionChanged,
                    "Session changed during capture; observe again",
                ));
            }
            Ok(())
        };
        validate()?;

        // One read of the window list serves the crop and the report of
        // windows off the display.
        let windows = list();
        // The main display's corner is the origin of global points, so a
        // window's bounds are points of this display.
        let display = Region {
            x: 0,
            y: 0,
            width: epoch.width.round() as i32,
            height: epoch.height.round() as i32,
        };
        let (area, app) = match crop {
            None => (None, None),
            Some(Crop::Region(asked)) => {
                let area = crop::clamp(asked, &display).ok_or_else(|| {
                    failure(Code::BadRequest, "\"region\" lies outside the display")
                })?;
                (Some(area), None)
            }
            Some(Crop::App { pid }) => {
                let on_screen = windows
                    .as_ref()
                    .map_err(|_| failure(Code::Windows, "The window list is unavailable"))?;
                let found = crop::app(on_screen, pid, &display, &dock(on_screen)).ok_or_else(|| {
                    failure(
                        Code::NoWindow,
                        format!(
                            "Process {pid} has no window on screen; open one, or take a full screenshot"
                        ),
                    )
                })?;
                (Some(found.region), Some((pid, found)))
            }
        };

        let limit = if per_point {
            let (width, height) = area.map_or((epoch.width, epoch.height), |area| {
                (f64::from(area.width), f64::from(area.height))
            });
            capture::per_point(max_dimension, width.max(height))
        } else {
            max_dimension
        };
        let shot = capture(epoch.display_id as u32, area.map(rect), limit).map_err(refused)?;
        let off_display = off_display_windows(windows.as_deref().unwrap_or_default());

        let cropped = area.map(|area| crop_keys(area, app, bundle_id));
        validate()?;

        Ok((shot, off_display, cropped))
    })
    .await;
    match taken {
        Ok(Ok((shot, off_display, cropped))) => {
            let mut reply = json!({
                "ok": true,
                "png": STANDARD.encode(&shot.png),
                "width": shot.width,
                "height": shot.height,
                "scale": shot.scale,
                "offDisplayWindows": off_display,
            });
            if let (Some(reply), Some(cropped)) = (reply.as_object_mut(), cropped) {
                reply.extend(cropped);
            }
            reply
        }
        Ok(Err(refusal)) => refusal,
        Err(_) => failure(Code::Capture, "The capture failed"),
    }
}

/// Serve one request. `controller` says whether it came from the controller:
/// the helper's own processes (an app under test among them) may only say
/// hello, as the setup window does there, never act with Atlas's
/// permissions.
async fn handle(stream: UnixStream, daemon: Arc<Daemon>, controller: bool) -> Result<()> {
    let peer = stream.peer_cred()?;
    // Capture the connection owner's start before reading a request, so a
    // passed socket or a reused PID cannot acquire another caller's ancestry.
    let caller = peer
        .pid()
        .and_then(|pid| crate::process::info(pid).ok().flatten())
        .filter(|info| info.uid == peer.uid() && !info.zombie)
        .map(|info| info.instance);
    let (mut reader, mut writer) = stream.into_split();
    let request = match endpoint::read_line(&mut reader, MAX_REQUEST).await {
        Ok(line) => protocol::parse(&line),
        Err(error) => Err(refused(error)),
    };

    let request = request.and_then(|request| {
        if controller || matches!(request, Request::Hello) {
            Ok(request)
        } else {
            Err(failure(
                Code::Permission,
                "Only the account Atlas works for may ask the helper that",
            ))
        }
    });

    let request = request.and_then(|request| match allowed(&request, (daemon.on_console)()) {
        Some(refusal) => Err(refusal),
        None => Ok(request),
    });
    let request = request.and_then(|request| match pinned_elsewhere(&request, daemon.session) {
        Some(refusal) => Err(refusal),
        None => Ok(request),
    });
    let reply = match request {
        Err(reply) => reply,
        Ok(Request::HostSession(claims)) => tokio::task::spawn_blocking(move || {
            crate::host_session::verification::verify(&claims, caller, peer.uid())
        })
        .await
        .unwrap_or_else(|_| failure(Code::State, "The caller's host could not be verified")),
        Ok(Request::Hello) => {
            // The first hello of this login: Atlas answers in the helper.
            static GREETED: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !GREETED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                crate::login_log::mark("daemon.firstHello");
            }
            daemon.hello()
        }
        Ok(Request::Apps { .. }) => match tokio::task::spawn_blocking(|| {
            crate::native::processes::snapshot(
                || Ok(apps::running()),
                |app| app["pid"].as_i64().expect("running apps have process IDs"),
            )
        })
        .await
        {
            Ok(Ok((apps, starts))) => json!({"ok": true, "apps": apps, "starts": starts}),
            Ok(Err(error)) => json!(error),
            Err(_) => failure(Code::Apps, "Running applications could not be listed"),
        },
        Ok(Request::Screenshot {
            max_dimension,
            per_point,
            crop,
        }) => screenshot(&daemon, max_dimension, per_point, crop).await,
        Ok(Request::Run { argv, timeout, .. }) => {
            // Events reach the client through their own writer, so a slow
            // client never stalls supervision of the command.
            let (events, mut queue) = mpsc::channel(run::EVENTS);
            let delivery = tokio::spawn(async move {
                while let Some(event) = queue.recv().await {
                    write(&mut writer, &event).await?;
                }

                Ok::<_, Error>(writer)
            });

            // The client never sends after its request, so any read result
            // means it left. A command never goes on while a person may be
            // using the helper's desktop.
            let cancel = async {
                let mut byte = [0; 1];
                tokio::select! {
                    _ = reader.read(&mut byte) => run::Cancel::Left,
                    () = daemon.stopped() => run::Cancel::Stopping,
                    () = daemon.on_screen() => run::Cancel::OnConsole,
                }
            };

            let reply = run::run(&argv, timeout, events, cancel).await;
            let mut writer = delivery
                .await
                .map_err(|_| Error::new(Code::Transport, "The event writer failed"))??;

            return write(&mut writer, &reply).await;
        }
        Ok(Request::Restart) => {
            let written = write(&mut writer, &json!({"ok": true, "op": "restart"})).await;
            daemon.restarting.notify_one();
            return written;
        }
        // Answered before the log-out ends this process with the session.
        Ok(Request::Logout { force: true }) => {
            let written = write(&mut writer, &json!({"ok": true, "op": "logout"})).await;
            force_log_out();
            return written;
        }
        Ok(Request::Logout { force: false }) => ask_to_log_out().await,
        Ok(Request::Job(request)) => match &daemon.jobs {
            Some(jobs) => jobs.serve(request).await,
            None => failure(
                Code::Daemon,
                "This helper daemon cannot keep background jobs; run atlas restart",
            ),
        },
        Ok(Request::Read(request)) => return serve_read(&daemon, request, writer).await,
    };

    write(&mut writer, &reply).await
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
    mut writer: tokio::net::unix::OwnedWriteHalf,
) -> Result<()> {
    let on_console = daemon.on_console;
    let session = daemon.session;
    let epoch = match session().and_then(|proof| InputEpoch::from_proof(&proof, std::process::id()))
    {
        Ok(epoch) => epoch,
        Err(error) => return write(&mut writer, &refused(error)).await,
    };

    // Off screen, and in the session and display it was in when the
    // request came: asked by the worker before it reads and by the writer
    // before it delivers each part already read.
    let screened = Request::Read(request.clone());
    let guard = move || {
        if let Some(refusal) = allowed(&screened, on_console()) {
            return Err(refusal);
        }
        let current = InputEpoch::from_proof(&session().map_err(refused)?, std::process::id())
            .map_err(refused)?;
        if current == epoch {
            Ok(())
        } else {
            Err(failure(
                Code::SessionChanged,
                "The helper changed since Atlas checked it; look again",
            ))
        }
    };
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
        Ok(read::read(&request, &places, uid, &guard, &mut |event| {
            events.blocking_send(event).is_ok()
        }))
    })
    .await
    .map_or_else(
        |_| failure(Code::Collect, "The file could not be read"),
        |reply: std::result::Result<Value, Value>| reply.unwrap_or_else(|refusal| refusal),
    );
    let (mut writer, cut) = delivery
        .await
        .map_err(|_| Error::new(Code::Transport, "The event writer failed"))??;
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
mod tests {
    use super::*;
    use crate::transport::{Transport, run_output};
    use tokio::io::{AsyncBufReadExt, BufReader};

    /// The run loop mode is passed by name; it is the default mode.
    #[test]
    #[allow(
        unsafe_code,
        reason = "FFI: the framework's constants are extern statics"
    )]
    fn the_run_loop_mode_is_the_default_mode() {
        // SAFETY: reading an immutable framework constant in a test.
        let mode = unsafe { objc2_core_foundation::kCFRunLoopDefaultMode }.unwrap();
        assert_eq!(mode.to_string(), "kCFRunLoopDefaultMode");
    }

    fn synthetic(display: u32, _: Option<CGRect>, _: Option<u32>) -> Result<capture::Shot> {
        assert_eq!(display, 7);
        Ok(capture::Shot {
            png: b"\x89PNG synthetic".to_vec(),
            width: 64,
            height: 36,
            scale: 1.0,
        })
    }

    fn window(pid: i64, owner: &str, layer: i64, bounds: [f64; 4]) -> windows::Window {
        windows::Window {
            number: pid * 10 + layer,
            pid,
            owner: Some(owner.into()),
            title: Some(format!("{owner} window")),
            layer,
            alpha: 1.0,
            bounds: windows::Bounds {
                x: bounds[0],
                y: bounds[1],
                width: bounds[2],
                height: bounds[3],
            },
        }
    }

    /// A 64 × 36 point display: an alert of System Events in front of a
    /// TextEdit window, and a Finder window behind both.
    fn fake_windows() -> Result<Vec<windows::Window>> {
        Ok(vec![
            window(300, "System Events", 8, [20.0, 10.0, 10.0, 8.0]),
            window(200, "TextEdit", 0, [10.0, 5.0, 30.0, 20.0]),
            window(100, "Finder", 0, [0.0, 0.0, 64.0, 36.0]),
        ])
    }

    fn fake_bundle_id(pid: i64) -> Option<String> {
        (pid == 200).then(|| "com.apple.TextEdit".to_owned())
    }

    fn session() -> Result<Value> {
        Ok(json!({
            "ok": true,
            "uid": 502,
            "securitySessionID": 42,
            "hasGraphicAccess": true,
            "onConsole": false,
            "displayID": 7,
            "width": 64.0,
            "height": 36.0,
        }))
    }

    fn daemon(on_console: bool) -> Arc<Daemon> {
        let mut daemon = Daemon::new(
            "Aqua".into(),
            synthetic,
            if on_console { || true } else { || false },
            session,
        );
        daemon.windows = fake_windows;
        daemon.bundle_id = fake_bundle_id;
        daemon.dock = |_| windows::Dock::Icons(Vec::new());
        Arc::new(daemon)
    }

    /// Send one request line over a socket pair and collect every reply line.
    async fn exchange(daemon: Arc<Daemon>, request: &str) -> Vec<Value> {
        exchange_as(daemon, request, true).await
    }

    /// [`exchange`] from the controller, or from the helper's own account.
    async fn exchange_as(daemon: Arc<Daemon>, request: &str, controller: bool) -> Vec<Value> {
        let (mut client, server) = UnixStream::pair().unwrap();
        let served = tokio::spawn(handle(server, daemon, controller));

        client
            .write_all(format!("{request}\n").as_bytes())
            .await
            .unwrap();

        let mut lines = BufReader::new(client).lines();
        let mut replies = Vec::new();

        while let Some(line) = lines.next_line().await.unwrap() {
            let value: Value = serde_json::from_str(&line).unwrap();
            let last = value.get("ok").is_some();
            replies.push(value);
            if last {
                break;
            }
        }

        served.await.unwrap().unwrap();
        replies
    }

    /// Anything in the helper's own account is answered hello and refused
    /// everything else, so nothing changed.
    #[tokio::test]
    async fn the_helpers_own_processes_may_only_say_hello() {
        let hello = exchange_as(daemon(false), r#"{"op":"hello"}"#, false).await;
        assert_eq!(hello[0]["op"], "hello");

        for request in [
            r#"{"op":"run","argv":["/bin/echo","hi"]}"#,
            r#"{"op":"screenshot"}"#,
            r#"{"op":"apps"}"#,
            r#"{"op":"restart"}"#,
            r#"{"op":"logout","force":true}"#,
            r#"{"op":"job","action":"show"}"#,
            r#"{"op":"job","action":"stop-all","reason":"person"}"#,
            r#"{"op":"read","path":".zshrc","maxSize":1024}"#,
        ] {
            let reply = &exchange_as(daemon(false), request, false).await[0];
            assert_eq!(reply["code"], "permission", "{request}");
            assert_eq!(reply["uncertain"], false, "{request}");
        }
    }

    /// A read pinned to the session Atlas verified is served only in that
    /// session: one that logged in again, or moved to another display, is
    /// refused before anything runs.
    #[tokio::test]
    async fn a_read_pinned_to_another_session_is_refused_before_it_runs() {
        let pinned = |session: &str| {
            [
                format!(r#"{{"op":"apps","session":{session}}}"#),
                format!(r#"{{"op":"run","argv":["/bin/echo","hi"],"session":{session}}}"#),
            ]
        };

        for request in pinned(r#"{"securitySessionId":42,"displayId":7}"#) {
            let reply = exchange(daemon(false), &request).await;
            assert_ne!(reply[0]["code"], "session-changed", "{request}");
            assert!(
                reply.iter().any(|line| line["ok"] == true),
                "{request}: {reply:?}"
            );
        }

        for session in [
            r#"{"securitySessionId":43,"displayId":7}"#,
            r#"{"securitySessionId":42,"displayId":8}"#,
        ] {
            for request in pinned(session) {
                let reply = exchange(daemon(false), &request).await;
                assert_eq!(reply.len(), 1, "nothing ran: {reply:?}");
                assert_eq!(reply[0]["code"], "session-changed", "{request}");
                assert_eq!(reply[0]["uncertain"], false);
            }
        }
    }

    #[tokio::test]
    async fn requests_are_answered_on_the_wire() {
        let hello = exchange(daemon(false), r#"{"op":"hello"}"#).await;
        assert_eq!(hello[0]["op"], "hello");
        assert_eq!(hello[0]["session"]["managerName"], "Aqua");

        let refused = exchange(daemon(false), r#"{"op":"nope"}"#).await;
        assert_eq!(refused[0]["error"], "unknown op 'nope'");

        let ran = exchange(
            daemon(false),
            r#"{"op":"run","argv":["/bin/echo","hi"],"timeoutMs":3000}"#,
        )
        .await;
        assert_eq!(ran[0]["event"], "started");
        assert_eq!(ran[1], json!({"event": "output", "data": "aGkK"}));
        assert_eq!(ran[2]["exitCode"], 0);

        let shot = exchange(daemon(false), r#"{"op":"screenshot","maxDimension":64}"#).await;
        assert_eq!(
            (&shot[0]["width"], &shot[0]["height"]),
            (&json!(64), &json!(36))
        );
        assert!(shot[0]["offDisplayWindows"].is_array());

        let apps = exchange(daemon(false), r#"{"op":"apps"}"#).await;
        assert!(
            apps[0]["apps"]
                .as_array()
                .is_some_and(|apps| !apps.is_empty())
        );
    }

    #[tokio::test]
    async fn nothing_acts_while_the_helper_is_on_screen() {
        for request in [
            r#"{"op":"run","argv":["/bin/echo","hi"]}"#,
            r#"{"op":"screenshot"}"#,
            r#"{"op":"screenshot","pid":200}"#,
            r#"{"op":"screenshot","region":{"x":0,"y":0,"width":10,"height":10}}"#,
        ] {
            let reply = exchange(daemon(true), request).await;
            assert_eq!(reply.len(), 1, "{request}");
            assert_eq!(reply[0]["code"], "on-console", "{request}");
        }

        let hello = exchange(daemon(true), r#"{"op":"hello"}"#).await;
        assert_eq!(hello[0]["ok"], true, "facts stay available");
    }

    #[test]
    fn nothing_is_captured_if_the_helper_comes_on_screen_while_the_worker_waits() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

        static SHOWN: AtomicBool = AtomicBool::new(false);
        static CHECKED: Notify = Notify::const_new();
        static CAPTURES: AtomicUsize = AtomicUsize::new(0);

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .max_blocking_threads(1)
            .build()
            .unwrap();
        let (release, waiting) = std::sync::mpsc::channel();
        let blocker = runtime.spawn_blocking(move || {
            waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        let daemon = Arc::new(Daemon::new(
            "Aqua".into(),
            |display, area, limit| {
                CAPTURES.fetch_add(1, Ordering::SeqCst);
                synthetic(display, area, limit)
            },
            || {
                let shown = SHOWN.load(Ordering::SeqCst);
                CHECKED.notify_one();
                shown
            },
            session,
        ));

        runtime.block_on(async {
            let request = tokio::spawn(exchange(daemon, r#"{"op":"screenshot"}"#));
            tokio::time::timeout(Duration::from_secs(5), CHECKED.notified())
                .await
                .unwrap();
            SHOWN.store(true, Ordering::SeqCst);
            release.send(()).unwrap();
            blocker.await.unwrap();

            let reply = request.await.unwrap();
            assert_eq!(CAPTURES.load(Ordering::SeqCst), 0);
            assert_eq!(
                reply,
                vec![
                    allowed(
                        &Request::Screenshot {
                            max_dimension: None,
                            per_point: false,
                            crop: None
                        },
                        true
                    )
                    .unwrap()
                ]
            );
        });
    }

    #[tokio::test]
    async fn a_capture_is_refused_when_the_display_epoch_changes() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        static READS: AtomicUsize = AtomicUsize::new(0);
        static CHANGE_AT: AtomicUsize = AtomicUsize::new(1);
        static CAPTURES: AtomicUsize = AtomicUsize::new(0);

        for (change_at, request) in [1, 2].into_iter().flat_map(|change_at| {
            [
                r#"{"op":"screenshot"}"#,
                r#"{"op":"screenshot","pid":200}"#,
                r#"{"op":"screenshot","region":{"x":0,"y":0,"width":10,"height":10}}"#,
            ]
            .map(|request| (change_at, request))
        }) {
            READS.store(0, Ordering::SeqCst);
            CHANGE_AT.store(change_at, Ordering::SeqCst);
            CAPTURES.store(0, Ordering::SeqCst);
            let mut daemon = Daemon::new(
                "Aqua".into(),
                |display, area, limit| {
                    CAPTURES.fetch_add(1, Ordering::SeqCst);
                    synthetic(display, area, limit)
                },
                || false,
                || {
                    let mut proof = session()?;
                    if READS.fetch_add(1, Ordering::SeqCst) >= CHANGE_AT.load(Ordering::SeqCst) {
                        proof["displayID"] = json!(8);
                    }
                    Ok(proof)
                },
            );
            daemon.windows = fake_windows;
            daemon.dock = |_| windows::Dock::Icons(Vec::new());

            let reply = exchange(Arc::new(daemon), request).await;
            assert_eq!(CAPTURES.load(Ordering::SeqCst), change_at - 1, "{request}");
            assert_eq!(
                reply,
                vec![failure(
                    Code::SessionChanged,
                    "Session changed during capture; observe again"
                )]
            );
        }
    }

    #[tokio::test]
    async fn pixels_are_discarded_if_the_helper_comes_on_screen_during_capture() {
        use std::sync::atomic::{AtomicBool, Ordering};

        static SHOWN: AtomicBool = AtomicBool::new(false);
        let daemon = Arc::new(Daemon::new(
            "Aqua".into(),
            |display, area, limit| {
                SHOWN.store(true, Ordering::SeqCst);
                synthetic(display, area, limit)
            },
            || SHOWN.load(Ordering::SeqCst),
            session,
        ));

        let reply = exchange(daemon, r#"{"op":"screenshot"}"#).await;
        assert_eq!(
            reply,
            vec![
                allowed(
                    &Request::Screenshot {
                        max_dimension: None,
                        per_point: false,
                        crop: None
                    },
                    true
                )
                .unwrap()
            ]
        );
    }

    /// The reply shapes clients and the live checks read.
    #[tokio::test]
    async fn replies_keep_their_shapes() {
        let keys = |value: &Value| {
            let mut keys: Vec<_> = value.as_object().unwrap().keys().cloned().collect();
            keys.sort();
            keys
        };

        let hello = &exchange(daemon(false), r#"{"op":"hello"}"#).await[0];
        assert_eq!(
            keys(hello),
            [
                "daemon",
                "display",
                "ok",
                "op",
                "permissions",
                "session",
                "user"
            ]
        );
        assert_eq!(
            keys(&hello["daemon"]),
            ["capabilities", "pid", "protocol", "version"]
        );
        assert_eq!(keys(&hello["display"]), ["height", "scale", "width"]);
        assert_eq!(keys(&hello["user"]), ["home", "name", "uid"]);

        let shot = &exchange(daemon(false), r#"{"op":"screenshot"}"#).await[0];
        assert_eq!(
            keys(shot),
            ["height", "offDisplayWindows", "ok", "png", "scale", "width"]
        );

        let region = &exchange(
            daemon(false),
            r#"{"op":"screenshot","region":{"x":0,"y":0,"width":10,"height":10}}"#,
        )
        .await[0];
        assert_eq!(
            keys(region),
            [
                "height",
                "offDisplayWindows",
                "ok",
                "png",
                "region",
                "scale",
                "width"
            ]
        );
        assert_eq!(keys(&region["region"]), ["height", "width", "x", "y"]);

        let app = &exchange(daemon(false), r#"{"op":"screenshot","pid":200}"#).await[0];
        assert_eq!(
            keys(app),
            [
                "app",
                "height",
                "offDisplayWindows",
                "ok",
                "overlapping",
                "png",
                "region",
                "scale",
                "width"
            ]
        );
        assert_eq!(keys(&app["app"]), ["bundleId", "name", "pid"]);
        assert_eq!(app["app"]["bundleId"], "com.apple.TextEdit");
        assert_eq!(
            keys(&app["overlapping"][0]),
            ["bounds", "owner", "pid", "title"]
        );

        let ran = exchange(daemon(false), r#"{"op":"run","argv":["/bin/echo"]}"#).await;
        assert_eq!(keys(&ran[0]), ["event", "pid"]);
        assert_eq!(
            keys(ran.last().unwrap()),
            ["durationMs", "exitCode", "ok", "signal", "timedOut"]
        );

        let refusal = &exchange(daemon(false), r#"{"op":"run","argv":[]}"#).await[0];
        assert_eq!(keys(refusal), ["code", "error", "ok", "uncertain"]);

        let apps = &exchange(daemon(false), r#"{"op":"apps"}"#).await[0];
        assert_eq!(
            keys(&apps["apps"][0]),
            [
                "active",
                "bundleId",
                "executable",
                "hidden",
                "launchedAt",
                "name",
                "pid",
                "policy"
            ]
        );
    }

    /// The area each capture was asked for, kept in `areas`.
    type Areas = std::sync::Mutex<Vec<Option<CGRect>>>;

    fn recorded(areas: &Areas, area: Option<CGRect>) -> Result<capture::Shot> {
        areas.lock().unwrap().push(area);
        synthetic(7, area, None)
    }

    fn with_windows(
        capture: Capture,
        windows: fn() -> Result<Vec<windows::Window>>,
    ) -> Arc<Daemon> {
        let mut daemon = Daemon::new("Aqua".into(), capture, || false, session);
        daemon.windows = windows;
        daemon.bundle_id = fake_bundle_id;
        daemon.dock = |_| windows::Dock::Icons(Vec::new());
        Arc::new(daemon)
    }

    #[tokio::test]
    async fn anonymous_captures_skip_routing_but_app_crops_still_read_the_dock() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static LISTS: AtomicUsize = AtomicUsize::new(0);
        static DOCKS: AtomicUsize = AtomicUsize::new(0);
        let mut daemon = Daemon::new("Aqua".into(), synthetic, || false, session);
        daemon.windows = || {
            LISTS.fetch_add(1, Ordering::SeqCst);
            fake_windows()
        };
        daemon.dock = |_| {
            DOCKS.fetch_add(1, Ordering::SeqCst);
            windows::Dock::Unread
        };
        for crop in [
            None,
            Some(Crop::Region(Region {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            })),
            Some(Crop::App { pid: 200 }),
        ] {
            LISTS.store(0, Ordering::SeqCst);
            DOCKS.store(0, Ordering::SeqCst);
            let reply = tokio::time::timeout(
                Duration::from_secs(5),
                screenshot(&daemon, None, false, crop),
            )
            .await
            .unwrap();
            assert_eq!(reply["ok"], true);
            assert!(reply.get("windows").is_none());
            assert_eq!(
                LISTS.load(Ordering::SeqCst),
                1,
                "only the existing window report is read"
            );
            assert_eq!(
                DOCKS.load(Ordering::SeqCst),
                usize::from(matches!(crop, Some(Crop::App { .. })))
            );
        }
    }

    fn sides(area: CGRect) -> (f64, f64, f64, f64) {
        (
            area.origin.x,
            area.origin.y,
            area.size.width,
            area.size.height,
        )
    }

    #[tokio::test]
    async fn an_app_screenshot_covers_its_windows_and_names_what_is_in_front() {
        static AREAS: Areas = std::sync::Mutex::new(Vec::new());
        let daemon = with_windows(|_, area, _| recorded(&AREAS, area), fake_windows);

        let reply = &exchange(daemon, r#"{"op":"screenshot","pid":200}"#).await[0];

        let areas = AREAS.lock().unwrap().clone();
        assert_eq!(areas.len(), 1);
        assert_eq!(sides(areas[0].unwrap()), (2.0, 0.0, 46.0, 33.0));
        assert_eq!(
            reply["region"],
            json!({"x": 2, "y": 0, "width": 46, "height": 33})
        );
        assert_eq!(
            reply["app"],
            json!({"pid": 200, "name": "TextEdit", "bundleId": "com.apple.TextEdit"})
        );
        assert_eq!(
            reply["overlapping"],
            json!([{
                "owner": "System Events",
                "pid": 300,
                "title": "System Events window",
                "bounds": {"x": 20.0, "y": 10.0, "width": 10.0, "height": 8.0},
            }])
        );
    }

    #[tokio::test]
    async fn an_app_without_a_window_on_screen_is_refused_without_capturing() {
        static AREAS: Areas = std::sync::Mutex::new(Vec::new());
        let capture: Capture = |_, area, _| recorded(&AREAS, area);
        let unavailable = || Err(Error::new(Code::Windows, "The window list is unavailable"));

        let reply = exchange(
            with_windows(capture, fake_windows),
            r#"{"op":"screenshot","pid":4242}"#,
        )
        .await;
        assert_eq!(
            reply,
            vec![failure(
                Code::NoWindow,
                "Process 4242 has no window on screen; open one, or take a full screenshot"
            )]
        );

        let reply = exchange(
            with_windows(capture, unavailable),
            r#"{"op":"screenshot","pid":200}"#,
        )
        .await;
        assert_eq!(reply[0]["code"], "windows");
        assert!(AREAS.lock().unwrap().is_empty(), "nothing was captured");

        // A full screenshot does without the window list.
        let reply =
            &exchange(with_windows(capture, unavailable), r#"{"op":"screenshot"}"#).await[0];
        assert_eq!(
            (&reply["ok"], &reply["offDisplayWindows"]),
            (&json!(true), &json!([]))
        );
        assert_eq!(*AREAS.lock().unwrap(), [None]);
    }

    #[tokio::test]
    async fn a_region_screenshot_is_cut_to_the_display() {
        static AREAS: Areas = std::sync::Mutex::new(Vec::new());
        let capture: Capture = |_, area, _| recorded(&AREAS, area);

        let reply = &exchange(
            with_windows(capture, fake_windows),
            r#"{"op":"screenshot","region":{"x":-10,"y":30,"width":100,"height":100}}"#,
        )
        .await[0];
        assert_eq!(
            reply["region"],
            json!({"x": 0, "y": 30, "width": 64, "height": 6})
        );
        assert!(reply.get("app").is_none() && reply.get("overlapping").is_none());

        let reply = exchange(
            with_windows(capture, fake_windows),
            r#"{"op":"screenshot","region":{"x":64,"y":0,"width":10,"height":10}}"#,
        )
        .await;
        assert_eq!(
            reply,
            vec![failure(
                Code::BadRequest,
                "\"region\" lies outside the display"
            )]
        );

        let areas = AREAS.lock().unwrap().clone();
        assert_eq!(areas.len(), 1, "the region outside was not captured");
        assert_eq!(sides(areas[0].unwrap()), (0.0, 30.0, 64.0, 6.0));
    }

    #[tokio::test]
    async fn a_picture_per_point_is_limited_to_the_points_it_shows() {
        static LIMITS: std::sync::Mutex<Vec<Option<u32>>> = std::sync::Mutex::new(Vec::new());
        let capture: Capture = |display, area, limit| {
            LIMITS.lock().unwrap().push(limit);
            synthetic(display, area, limit)
        };

        for request in [
            r#"{"op":"screenshot","maxDimension":1456,"perPoint":true}"#,
            r#"{"op":"screenshot","maxDimension":40,"perPoint":true}"#,
            r#"{"op":"screenshot","perPoint":true}"#,
            r#"{"op":"screenshot","maxDimension":1456,"perPoint":true,"region":{"x":0,"y":0,"width":12,"height":20}}"#,
            r#"{"op":"screenshot","maxDimension":1456,"perPoint":true,"region":{"x":-10,"y":30,"width":100,"height":100}}"#,
            r#"{"op":"screenshot","maxDimension":1456,"perPoint":true,"pid":200}"#,
            r#"{"op":"screenshot","maxDimension":1456,"region":{"x":0,"y":0,"width":12,"height":20}}"#,
            r#"{"op":"screenshot","maxDimension":1456}"#,
            r#"{"op":"screenshot"}"#,
        ] {
            let reply = &exchange(with_windows(capture, fake_windows), request).await[0];
            assert_eq!(reply["ok"], true, "{request}");
        }

        assert_eq!(
            *LIMITS.lock().unwrap(),
            [
                Some(64),
                Some(40),
                Some(64),
                Some(20),
                Some(64),
                Some(46),
                Some(1456),
                Some(1456),
                None
            ]
        );
    }

    #[tokio::test]
    async fn restart_answers_then_asks_the_daemon_to_exit() {
        let daemon = daemon(false);
        let notified = daemon.clone();
        let waiting = tokio::spawn(async move { notified.restarting.notified().await });

        tokio::task::yield_now().await;
        let reply = exchange(daemon, r#"{"op":"restart"}"#).await;
        assert_eq!(reply[0], json!({"ok": true, "op": "restart"}));

        tokio::time::timeout(Duration::from_secs(2), waiting)
            .await
            .unwrap()
            .unwrap();
    }

    /// A client that hangs up ends its quiet command; so does a stopping
    /// daemon.
    #[tokio::test]
    async fn quiet_commands_end_when_the_client_or_daemon_leaves() {
        for leave in ["client", "daemon"] {
            let daemon = daemon(false);
            let (mut client, server) = UnixStream::pair().unwrap();
            let served = tokio::spawn(handle(server, daemon.clone(), true));

            client
                .write_all(
                    b"{\"op\":\"run\",\"argv\":[\"/bin/sleep\",\"30\"],\"timeoutMs\":60000}\n",
                )
                .await
                .unwrap();

            let mut lines = BufReader::new(client).lines();
            let started: Value =
                serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
            let group = started["pid"].as_i64().unwrap() as i32;

            match leave {
                "client" => drop(lines),
                _ => {
                    daemon.stopping.send_replace(true);

                    // The client still listens: it hears that the command
                    // was cut short, with an outcome nobody can know.
                    let reply: Value =
                        serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
                    assert_eq!(
                        (&reply["code"], &reply["uncertain"]),
                        (&json!("restart"), &json!(true))
                    );
                }
            }

            tokio::time::timeout(Duration::from_secs(8), served)
                .await
                .expect(leave)
                .unwrap()
                .ok();

            let group = nix::unistd::Pid::from_raw(group);
            assert!(nix::sys::signal::killpg(group, None).is_err(), "{leave}");
        }
    }

    /// A running command stops when the helper comes on screen, and the
    /// client hears that its outcome is unknown.
    #[tokio::test]
    async fn a_command_stops_when_the_helper_comes_on_screen() {
        static SHOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

        let daemon = Arc::new(Daemon::new(
            "Aqua".into(),
            synthetic,
            || SHOWN.load(std::sync::atomic::Ordering::SeqCst),
            session,
        ));
        let (mut client, server) = UnixStream::pair().unwrap();
        let served = tokio::spawn(handle(server, daemon, true));

        client
            .write_all(b"{\"op\":\"run\",\"argv\":[\"/bin/sleep\",\"30\"],\"timeoutMs\":60000}\n")
            .await
            .unwrap();

        let mut lines = BufReader::new(client).lines();
        let started: Value =
            serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        let group = started["pid"].as_i64().unwrap() as i32;

        SHOWN.store(true, std::sync::atomic::Ordering::SeqCst);

        let line = tokio::time::timeout(Duration::from_secs(8), lines.next_line())
            .await
            .expect("the command was stopped")
            .unwrap()
            .unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            (&reply["code"], &reply["uncertain"]),
            (&json!("on-console"), &json!(true))
        );

        served.await.unwrap().unwrap();

        let group = nix::unistd::Pid::from_raw(group);
        assert!(nix::sys::signal::killpg(group, None).is_err());
    }

    /// The client's own transport reads this daemon's replies.
    #[tokio::test]
    async fn the_client_transport_understands_the_daemon() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("d.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        let daemon = daemon(false);

        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(handle(stream, daemon.clone(), true));
            }
        });

        let transport = Transport {
            path: socket,
            timeout: Duration::from_secs(10),
            peer_uid: nix::unistd::getuid().as_raw(),
        };

        let reply = transport
            .call(&json!({"op": "run", "argv": ["/bin/sh", "-c", "printf ok; exit 4"], "timeoutMs": 3000}))
            .await
            .unwrap();
        assert_eq!(run_output(&reply, 64).unwrap(), b"ok");
        assert_eq!(
            (&reply["exitCode"], &reply["timedOut"]),
            (&json!(4), &json!(false))
        );
        crate::client::validate_completion(&json!({"op": "run"}), &reply).unwrap();

        let hello = transport.call(&json!({"op": "hello"})).await.unwrap();
        assert_eq!(hello["daemon"]["pid"], std::process::id());
    }

    /// A daemon that keeps its jobs in a folder of its own.
    fn with_jobs(on_console: bool) -> (tempfile::TempDir, Arc<Daemon>) {
        let folder = tempfile::tempdir().unwrap();
        let mut daemon = Daemon::new(
            "Aqua".into(),
            synthetic,
            if on_console { || true } else { || false },
            session,
        );
        daemon.jobs = Some(Arc::new(
            Jobs::open(
                folder.path(),
                std::env::current_exe().unwrap(),
                nix::unistd::getuid().as_raw(),
            )
            .unwrap(),
        ));
        (folder, Arc::new(daemon))
    }

    #[tokio::test]
    async fn the_daemon_offers_jobs_and_reads() {
        let hello = &exchange(daemon(false), r#"{"op":"hello"}"#).await[0];
        assert_eq!(
            hello["daemon"]["capabilities"],
            json!(["session-pin", "read"]),
            "no jobs without their folder"
        );
        let refused = &exchange(daemon(false), r#"{"op":"job","action":"show"}"#).await[0];
        assert_eq!(refused["code"], "daemon");

        let (_folder, daemon) = with_jobs(false);
        let hello = &exchange(daemon.clone(), r#"{"op":"hello"}"#).await[0];
        assert_eq!(
            hello["daemon"]["capabilities"],
            json!(["session-pin", "read", "jobs"])
        );
        assert_eq!(
            hello["daemon"]["jobs"],
            json!({"running": 0, "launching": 0, "unresolved": 0})
        );
        let shown = &exchange(daemon, r#"{"op":"job","action":"show"}"#).await[0];
        assert_eq!(shown["jobs"], json!([]));
    }

    #[tokio::test]
    async fn jobs_and_reads_are_refused_while_the_helper_is_on_screen() {
        let (_folder, daemon) = with_jobs(true);
        for request in [
            r#"{"op":"job","action":"start","argv":["/bin/sleep","9"],"lifetimeMs":5000,"uid":502}"#,
            r#"{"op":"read","path":".zshrc","maxSize":1024}"#,
        ] {
            let reply = exchange(daemon.clone(), request).await;
            assert_eq!(reply.len(), 1, "{request}");
            assert_eq!(reply[0]["code"], "on-console", "{request}");
        }
        for request in [
            r#"{"op":"job","action":"show"}"#,
            r#"{"op":"job","action":"stop-all","reason":"on-console"}"#,
            r#"{"op":"job","action":"log","id":"7c2e91a04b3f","bytes":10}"#,
        ] {
            let reply = &exchange(daemon.clone(), request).await[0];
            assert_ne!(
                reply["code"], "on-console",
                "{request}: ending things goes on"
            );
        }
    }

    #[tokio::test]
    async fn a_job_start_for_another_account_is_refused() {
        let (_folder, daemon) = with_jobs(false);
        let reply = &exchange(
            daemon,
            r#"{"op":"job","action":"start","argv":["/bin/sleep","9"],"lifetimeMs":5000,"uid":1}"#,
        )
        .await[0];
        assert_eq!(reply["code"], "identity");
        assert_eq!(reply["uncertain"], false);
    }

    #[test]
    fn nothing_is_read_if_the_helper_comes_on_screen_while_the_worker_waits() {
        use std::sync::atomic::{AtomicBool, Ordering};

        static SHOWN: AtomicBool = AtomicBool::new(false);
        static CHECKED: Notify = Notify::const_new();

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .max_blocking_threads(1)
            .build()
            .unwrap();
        let (release, waiting) = std::sync::mpsc::channel();
        let blocker = runtime.spawn_blocking(move || {
            waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        let daemon = Arc::new(Daemon::new(
            "Aqua".into(),
            synthetic,
            || {
                let shown = SHOWN.load(Ordering::SeqCst);
                CHECKED.notify_one();
                shown
            },
            session,
        ));

        runtime.block_on(async {
            let request = tokio::spawn(exchange(
                daemon,
                r#"{"op":"read","path":".zshrc","maxSize":1048576}"#,
            ));
            tokio::time::timeout(Duration::from_secs(5), CHECKED.notified())
                .await
                .unwrap();
            SHOWN.store(true, Ordering::SeqCst);
            release.send(()).unwrap();
            blocker.await.unwrap();

            let reply = request.await.unwrap();
            assert_eq!(reply.len(), 1, "no part was sent: {reply:?}");
            assert_eq!(reply[0]["code"], "on-console");
            assert_eq!(reply[0]["uncertain"], false);
        });
    }

    #[tokio::test]
    async fn parts_waiting_to_be_delivered_are_dropped_once_the_session_changes() {
        let (events, queue) = mpsc::channel(8);
        for n in 0..3 {
            events
                .send(json!({"event": "data", "data": n.to_string()}))
                .await
                .unwrap();
        }
        drop(events);
        let checks = std::sync::atomic::AtomicUsize::new(0);
        let gate = || {
            if checks.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                Ok(())
            } else {
                Err(failure(
                    Code::SessionChanged,
                    "The helper changed since Atlas checked it; look again",
                ))
            }
        };
        let (written, cut) = deliver(queue, Vec::new(), gate).await.unwrap();
        let lines = String::from_utf8(written).unwrap();
        assert_eq!(
            lines.lines().count(),
            1,
            "only the part before the change: {lines}"
        );
        assert_eq!(cut.unwrap()["code"], "session-changed");
    }
}
