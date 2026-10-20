//! The local service Atlas.app talks to: a capability URL on the loopback
//! interface that answers status, pictures of the helper, control and input
//! for one menu bar, and keeps a private record of itself in the output
//! directory so `atlas monitor` and the login item can find it.
use crate::{
    client::{Client, Frame},
    config::Config,
    control::{self, ActionPermit, Lease, SharedLease},
    error::{Error, Result},
    menubar::{App, Line},
    session::Epoch,
};
use axum::{
    body::{Body, Bytes, to_bytes},
    extract::Request,
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    response::Response,
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{Mutex, OwnedSemaphorePermit, Semaphore, watch},
};

const FRAME_LIFETIME: Duration = Duration::from_secs(8);
/// How long control lasts without a renewal; the app is told the same.
const LEASE_SECONDS: u64 = 10;
/// The widest picture of the helper the viewer asks for, in pixels.
const FRAME_WIDTH: u32 = 1600;
/// How many recent frames are kept for control requests to name.
const KEPT_FRAMES: usize = 24;
/// How many requests the viewer serves at once.
const CONCURRENT_REQUESTS: usize = 32;
/// Requests kept for ending control, so a busy viewer still lets go.
const URGENT_REQUESTS: usize = 8;
/// How many connections are open at once; more wait to be accepted.
const CONNECTIONS: usize = 64;
/// How long a connection may take to send a request's headers. An idle
/// connection closes after as long.
const HEADER_DEADLINE: Duration = Duration::from_secs(15);
/// How long reading the status or a frame may take: less than the app
/// waits (8 seconds), so it hears why rather than giving up.
const READ_DEADLINE: Duration = Duration::from_secs(5);
/// The largest request body, in bytes.
const MAX_BODY: usize = 32768;
/// What the viewer's own address answers, so it is told apart from any
/// other program on its port.
const SERVICE: &str = "atlas-viewer";

#[derive(Default)]
struct Frames {
    items: VecDeque<(String, Arc<Frame>)>,
    input_revision: u64,
    capture_revision: Option<u64>,
}

/// A frame capture in progress: the input it shows, and its outcome once
/// known, which every request waiting on it shares.
struct Capture {
    revision: u64,
    outcome: watch::Receiver<Option<Captured>>,
}

/// A capture's outcome as each request waiting on it reads it.
type Captured = std::result::Result<(String, Arc<Frame>), (&'static str, String)>;

/// Empties the capture slot when the capture that filled it ends, however
/// it ends.
struct Capturing<'a>(&'a std::sync::Mutex<Option<Capture>>);

impl Drop for Capturing<'_> {
    fn drop(&mut self) {
        *unpoisoned(self.0) = None;
    }
}

pub struct Viewer {
    /// The helper this viewer serves. It follows the settings when they
    /// name another one.
    client: std::sync::Mutex<Arc<Client>>,
    /// The settings file the client was made from, when it follows one.
    settings: Option<PathBuf>,
    /// That file's text when last found to match the client.
    settings_text: std::sync::Mutex<Option<Vec<u8>>>,
    lease: SharedLease,
    lease_epoch: Mutex<Option<Epoch>>,
    frames: Mutex<Frames>,
    capture: std::sync::Mutex<Option<Capture>>,
    requests: Semaphore,
    urgent: Semaphore,
    pub url: String,
    prefix: String,
    authority: String,
    /// The helper account's short name and its full name (None when it has
    /// none), once read.
    account_name: std::sync::Mutex<Option<(String, Option<String>)>>,
    /// The app this viewer serves, when it serves one.
    app: std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<Line>>>,
    /// The helper daemon and display input was last started or re-pinned
    /// for, so each gets one try.
    input_started: std::sync::Mutex<Option<String>>,
    /// Where this viewer's record is kept and what it says, once written.
    record: std::sync::Mutex<Option<(PathBuf, Value)>>,
}

impl Viewer {
    /// The helper this viewer serves right now.
    fn client(&self) -> Arc<Client> {
        unpoisoned(&self.client).clone()
    }

    /// Follow the settings file: once it names another helper (Getting
    /// Started set one up, say), serve that one without a restart. Only a
    /// change to the file's text is read. Control held in the previous
    /// helper ends there first; while control is still held the move waits
    /// for a later read.
    async fn follow_settings(&self) {
        let Some(path) = self.settings.clone() else {
            return;
        };

        let read = path.clone();
        let Ok(Ok(text)) = tokio::task::spawn_blocking(move || std::fs::read(read)).await else {
            return;
        };

        if unpoisoned(&self.settings_text).as_deref() == Some(text.as_slice()) {
            return;
        }

        let Ok(Ok(config)) = tokio::task::spawn_blocking(move || Config::load(&path)).await else {
            return;
        };

        let current = self.client();
        if config == current.config {
            *unpoisoned(&self.settings_text) = Some(text);
            return;
        }

        let Ok(mut next) = Client::new(config) else {
            return;
        };
        next.actor = current.actor;

        let _ = self.release(None).await;

        {
            let mut lease = self.lease.lock().await;
            let mut client = unpoisoned(&self.client);
            if lease.active() || client.config == next.config {
                return;
            }

            *client = Arc::new(next);
            *unpoisoned(&self.settings_text) = Some(text);
        }

        self.frames.lock().await.items.clear();
        *unpoisoned(&self.input_started) = None;
        self.move_record();
    }

    /// Keep this viewer's record in the output folder the settings name
    /// now, so commands reading them still find the menu bar.
    fn move_record(&self) {
        let client = self.client();
        let mut kept = unpoisoned(&self.record);
        let Some((path, record)) = kept.clone() else {
            return;
        };

        let Ok(output) = client.output_dir() else {
            return;
        };

        let moved = output.join(MENUBAR_RECORD);
        if moved == path {
            return;
        }

        let written = serde_json::to_vec_pretty(&record)
            .map_err(Error::from)
            .and_then(|bytes| control::write_private(&moved, &bytes));
        if written.is_ok() {
            remove_record(&path);
            *kept = Some((moved, record));
        }
    }

    /// Pass `line` to the app this viewer serves.
    fn send(&self, line: Line) -> Result<()> {
        unpoisoned(&self.app)
            .as_ref()
            .ok_or_else(|| Error::new("menubar", "No menu bar is attached to this viewer"))?
            .send(line)
            .map_err(|_| Error::new("menubar", "The menu bar is not running"))
    }

    /// A current frame of the helper. One capture runs at a time and every
    /// request asking meanwhile shares it; one started before the latest
    /// input is waited out, then a fresh one taken.
    async fn frame(&self) -> Result<(String, Arc<Frame>)> {
        loop {
            let revision = self.frames.lock().await.input_revision;

            // Either this request captures, or it waits on the one running.
            let ours = {
                let mut slot = unpoisoned(&self.capture);
                match slot.as_ref() {
                    Some(running) => Err((running.outcome.clone(), running.revision >= revision)),
                    None => {
                        let (sender, outcome) = watch::channel(None);
                        *slot = Some(Capture { revision, outcome });
                        Ok(sender)
                    }
                }
            };

            let sender = match ours {
                Ok(sender) => sender,
                Err((mut outcome, current)) => {
                    let shared = outcome
                        .wait_for(Option::is_some)
                        .await
                        .ok()
                        .and_then(|outcome| outcome.clone());
                    match shared {
                        Some(captured) if current => {
                            return captured.map_err(|(code, message)| Error::new(code, message));
                        }
                        _ => continue,
                    }
                }
            };

            let _clear = Capturing(&self.capture);
            let outcome = tokio::time::timeout(READ_DEADLINE, self.take_frame())
                .await
                .unwrap_or_else(|_| Err(late()));
            let shared = match &outcome {
                Ok(frame) => Ok(frame.clone()),
                Err(error) => Err((error.code, error.error.clone())),
            };
            let _ = sender.send(Some(shared));

            return outcome;
        }
    }

    /// Take a frame, or reuse the last one when it is recent and no input
    /// came since.
    async fn take_frame(&self) -> Result<(String, Arc<Frame>)> {
        let client = self.client();
        let current = client.identity().await?.epoch;
        let revision;

        {
            let mut frames = self.frames.lock().await;
            frames
                .items
                .retain(|(_, frame)| frame.at.elapsed() < FRAME_LIFETIME && frame.epoch == current);
            revision = frames.input_revision;

            if frames.capture_revision == Some(revision)
                && let Some((key, frame)) = frames.items.back()
                && frame.at.elapsed() < Duration::from_millis(400)
            {
                return Ok((key.clone(), frame.clone()));
            }
        }

        let frame = Arc::new(client.frame(Some(FRAME_WIDTH)).await?);
        if frame.at.elapsed() >= FRAME_LIFETIME {
            return Err(Error::new(
                "stale-frame",
                "Capture became stale while checking its session",
            ));
        }

        let key = control::token();
        let mut frames = self.frames.lock().await;
        frames.items.push_back((key.clone(), frame.clone()));
        frames.capture_revision = Some(revision);

        while frames.items.len() > KEPT_FRAMES {
            frames.items.pop_front();
        }

        Ok((key, frame))
    }

    async fn observed(&self, key: &str) -> Result<Arc<Frame>> {
        self.frames
            .lock()
            .await
            .items
            .iter()
            .find(|(id, _)| id == key)
            .map(|(_, frame)| frame.clone())
            .filter(|f| f.at.elapsed() < FRAME_LIFETIME)
            .ok_or_else(|| {
                Error::new(
                    "stale-frame",
                    "Wait for a fresh frame before taking control",
                )
            })
    }

    async fn release(&self, owner: Option<&str>) -> Result<()> {
        let previous = {
            let mut lease = self.lease.lock().await;
            lease.begin_release(owner)?
        };
        if let Some(previous) = previous {
            let completion = self.release_native(&previous).await;
            self.lease.lock().await.end_release();
            completion?;
        }

        Ok(())
    }

    /// End whichever control is held in a task of its own, which finishes
    /// however long the helper takes; the caller may wait on it or not.
    fn release_in_background(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let viewer = self.clone();

        tokio::spawn(async move {
            let _ = viewer.release(None).await;
        })
    }

    /// End `owner`'s native control in the helper. The outcome is marked
    /// pending first and cleared only once the helper confirms, so an
    /// unconfirmed release stays an unknown outcome for recovery.
    async fn release_native(&self, owner: &str) -> Result<()> {
        let client = self.client();
        let pending = client.controller.quarantine();
        let result = client.native_control("release", owner).await;
        match (pending, result) {
            (Ok(generation), Ok(_)) => client.controller.clear_if(&generation),
            (Err(error), _) => Err(error),
            (_, Err(error)) => Err(Error::new(
                "uncertain",
                format!(
                    "Control stopped accepting input, but native cleanup was not confirmed: {error}"
                ),
            )
            .uncertain()),
        }
    }

    async fn status(self: &Arc<Self>) -> Value {
        self.follow_settings().await;

        // Reading waits on the helper, for a few seconds at most.
        let until = tokio::time::Instant::now() + READ_DEADLINE;
        let client = self.client();
        let uid = client.config.helper_uid;
        let reply = tokio::time::timeout_at(until, client.status())
            .await
            .unwrap_or_else(|_| json!(late()));

        // Why a silent helper is down, asked off the runtime (and only then).
        let absence = if crate::session::silent(&reply) {
            tokio::time::timeout_at(until, blocking(move || Ok(crate::session::absence(uid))))
                .await
                .ok()
                .and_then(Result::ok)
                .flatten()
        } else {
            None
        };

        let mut status = crate::capability::doctor(reply, uid);

        if let Some(absence) = &absence {
            absence.explain(&mut status);
        }

        // Ending control waits on the helper too: it finishes by itself
        // when that takes longer than a moment.
        if status["ready"] != true {
            let ending = self.release_in_background();
            let _ = tokio::time::timeout(Duration::from_secs(1), ending).await;
        }

        let active = self.lease.lock().await.active();
        status["controlActive"] = json!(active);

        // An unreadable marker is unknown, so it counts as unresolved.
        let controller = &client.controller;
        let total = controller.pending().map_or(1, |pending| pending.len());
        let unresolved = controller.unresolved().map_or(1, |pending| pending.len());
        let pending = crate::overview::Pending {
            unresolved,
            in_flight: total.saturating_sub(unresolved),
        };

        self.start_input_after_login(&client, &status["session"], total == 0 && !active);

        let activity = client
            .output_dir()
            .map(|directory| crate::overview::activity(directory, 12))
            .unwrap_or_default();
        status["overview"] = crate::overview::overview(
            &status,
            active,
            pending,
            &activity,
            crate::client::timestamp(),
        );

        // The helper's full name, as people know the account; looked up once
        // per account (off the runtime, it asks directory services), also
        // when it has none.
        if let Some(name) = status["overview"]["account"].as_str() {
            let known = unpoisoned(&self.account_name)
                .clone()
                .filter(|(account, _)| account == name);
            let title = match known {
                Some((_, title)) => title,
                None => {
                    let account = name.to_owned();
                    match tokio::task::spawn_blocking(move || crate::setup::real_name(&account))
                        .await
                    {
                        Ok(title) => {
                            *unpoisoned(&self.account_name) =
                                Some((name.to_owned(), title.clone()));
                            title
                        }
                        Err(_) => None,
                    }
                }
            };

            status["overview"]["accountName"] = json!(title);
        }

        status
    }

    /// A helper just logged in has no input service yet, and switching
    /// accounts can leave one pinned to a display that is gone or renumbered
    /// (which taking control refuses). Input starts or re-pins once per
    /// helper daemon and display, as Recover would, so agents and viewers
    /// carry on by themselves: only in the background, with Accessibility on
    /// and nothing pending or held.
    fn start_input_after_login(&self, client: &Arc<Client>, session: &Value, settled: bool) {
        let native = &session["inputService"];
        let starts = settled
            && native["controlActive"] != true
            && native["heldInput"] != true
            && session["permissions"]["accessibility"] == true;
        let Some(hidden) = starts
            .then(|| crate::session::require_hidden(session, client.config.helper_uid).ok())
            .flatten()
            .filter(|hidden| !hidden.input)
        else {
            return;
        };

        let Some(daemon) = session["daemon"]["pid"].as_u64() else {
            return;
        };

        let display = &hidden.epoch.display;
        let key = format!(
            "{daemon}:{}:{}x{}",
            hidden.epoch.display_id, display.width, display.height
        );

        let first = self
            .input_started
            .lock()
            .map(|mut started| started.replace(key.clone()) != Some(key))
            .unwrap_or(false);
        if first {
            let client = client.clone();
            tokio::spawn(async move {
                let _ = client.start_input().await;
            });
        }
    }

    /// Explicit, person-initiated recovery: the verified restart when an
    /// outcome is unknown, otherwise starting, resetting or upgrading input.
    /// It never takes control or resumes input.
    async fn recover(&self) -> Result<Value> {
        self.follow_settings().await;

        if self.lease.lock().await.active() {
            return Err(Error::new("busy", "Release control before recovering"));
        }

        let client = self.client();
        if !client.controller.pending()?.is_empty() {
            return client.restart().await;
        }

        client.start_input().await
    }

    async fn control(&self, route: &str, body: &Value) -> Result<Value> {
        let owner = body["lease"].as_str().unwrap_or_default();
        let client = self.client();

        // The menu bar's emergency stop: end whichever lease is held, with
        // the same cleanup as any release. Nothing is resumed.
        if route == "control/revoke" {
            let held = self.lease.lock().await.active();
            self.release(None).await?;
            return Ok(json!({"ok": true, "released": held}));
        }

        if route == "control/release" {
            self.release(Some(owner)).await?;
            return Ok(json!({"ok": true}));
        }

        if route != "control/acquire" {
            self.lease.lock().await.require(owner)?;
        }

        let frame = self
            .observed(body["frame"].as_str().unwrap_or_default())
            .await?;

        if route == "control/acquire" {
            let current = client.identity().await?;
            if current.epoch != frame.epoch {
                return Err(Error::new(
                    "session-changed",
                    "Wait for a fresh frame from the current helper",
                ));
            }

            if !current.input {
                return Err(Error::new(
                    "permission",
                    "Accessibility permission is unavailable",
                ));
            }

            let token = {
                let mut lease = self.lease.lock().await;

                // Settings that moved to another helper meanwhile refuse it.
                if !Arc::ptr_eq(&client, &self.client()) {
                    return Err(Error::new(
                        "session-changed",
                        "Wait for a fresh frame from the current helper",
                    ));
                }

                let token = lease.acquire(&client.controller)?;
                *self.lease_epoch.lock().await = Some(frame.epoch.clone());
                token
            };
            if let Err(error) = client.acquire_input(&token, &frame.epoch).await {
                if error.uncertain {
                    let _ = self.release(Some(&token)).await;
                } else {
                    self.lease.lock().await.abandon_acquire(&token);
                }
                return Err(error);
            }

            let authorization = self.lease.lock().await.require(&token);
            if let Err(error) = authorization {
                // The lease ended while input was being acquired: undo the
                // acquire, and say when that could not be confirmed.
                return Err(match self.release_native(&token).await {
                    Ok(()) => error,
                    Err(cleanup) => Error {
                        error: format!("{} {}", error.error, cleanup.error),
                        ..cleanup
                    },
                });
            }

            return Ok(json!({"ok": true, "lease": token, "ttlSeconds": LEASE_SECONDS}));
        }

        if self.lease_epoch.lock().await.as_ref() != Some(&frame.epoch) {
            return Err(Error::new(
                "session-changed",
                "The controlled session changed",
            ));
        }

        if route == "control/renew" {
            self.lease.lock().await.renew(owner)?;
            client.native_control("renew", owner).await?;
            return Ok(json!({"ok": true}));
        }

        let _permit = ActionPermit::begin(self.lease.clone(), owner).await?;
        let result = client
            .input(
                &body["actions"],
                Some((&frame.epoch, frame.at)),
                Some((&self.lease, owner)),
            )
            .await;

        self.frames.lock().await.input_revision += 1;
        result
    }
}

/// Bind the viewer for `client` on `port` (0 picks a free one). With
/// `settings`, it follows that file when it names another helper.
pub async fn bind(
    mut client: Client,
    port: u16,
    settings: Option<&Path>,
) -> Result<(TcpListener, Arc<Viewer>)> {
    // Everything this viewer does in the helper is the person's doing.
    client.actor = crate::client::Actor::Viewer;

    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::AddrInUse => Error::new(
                "port-in-use",
                format!("Port {port} is in use by another program."),
            ),
            _ => error.into(),
        })?;

    let authority = listener.local_addr()?.to_string();
    let prefix = format!("/{}/", control::token());
    let viewer = Arc::new(Viewer {
        client: std::sync::Mutex::new(Arc::new(client)),
        settings: settings.map(Path::to_path_buf),
        settings_text: std::sync::Mutex::new(None),
        lease: Arc::new(Mutex::new(Lease::new(Duration::from_secs(LEASE_SECONDS)))),
        lease_epoch: Mutex::new(None),
        frames: Mutex::new(Frames::default()),
        capture: std::sync::Mutex::new(None),
        requests: Semaphore::new(CONCURRENT_REQUESTS),
        urgent: Semaphore::new(URGENT_REQUESTS),
        url: format!("http://{authority}{prefix}"),
        prefix,
        authority,
        app: std::sync::Mutex::new(None),
        account_name: std::sync::Mutex::new(None),
        input_started: std::sync::Mutex::new(None),
        record: std::sync::Mutex::new(None),
    });

    Ok((listener, viewer))
}

/// Answer requests on `listener` until `shutdown` resolves. A connection
/// must send each request's headers within [`HEADER_DEADLINE`], and at most
/// `connections` are open at once; more wait to be accepted.
async fn accept(
    listener: TcpListener,
    viewer: Arc<Viewer>,
    connections: usize,
    shutdown: impl std::future::Future<Output = ()>,
) {
    let open = Arc::new(Semaphore::new(connections));
    let (closing, closed) = watch::channel(());
    let mut http = hyper::server::conn::http1::Builder::new();
    http.timer(hyper_util::rt::TokioTimer::new())
        .header_read_timeout(HEADER_DEADLINE);
    tokio::pin!(shutdown);

    loop {
        let next = tokio::select! {
            () = &mut shutdown => break,
            next = connection(&listener, &open) => next,
        };
        let Some((stream, permit)) = next else {
            continue;
        };

        serve_connection(&http, stream, permit, &viewer, closed.clone());
    }

    drop(listener);
    drop(closing);

    // Requests in progress finish, within a few seconds.
    let _ = tokio::time::timeout(
        Duration::from_secs(5),
        open.acquire_many(u32::try_from(connections).unwrap_or(u32::MAX)),
    )
    .await;
}

/// Answer requests on `stream` in a task of its own, which holds `permit`
/// until the connection closes. Once `closed` changes, the request in
/// progress finishes and the connection closes.
fn serve_connection(
    http: &hyper::server::conn::http1::Builder,
    stream: TcpStream,
    permit: OwnedSemaphorePermit,
    viewer: &Arc<Viewer>,
    mut closed: watch::Receiver<()>,
) {
    let service = hyper::service::service_fn({
        let viewer = viewer.clone();
        move |request: hyper::Request<hyper::body::Incoming>| {
            let viewer = viewer.clone();
            async move {
                Ok::<_, std::convert::Infallible>(handle(viewer, request.map(Body::new)).await)
            }
        }
    });
    let served = http.serve_connection(hyper_util::rt::TokioIo::new(stream), service);

    tokio::spawn(async move {
        let _permit = permit;
        tokio::pin!(served);

        tokio::select! {
            _ = served.as_mut() => return,
            _ = closed.changed() => {}
        }

        // Closing: the request in progress finishes, then it closes.
        served.as_mut().graceful_shutdown();
        let _ = served.await;
    });
}

/// The next connection, once a place is open for it. None when accepting
/// failed (too many open files, say); it pauses first.
async fn connection(
    listener: &TcpListener,
    open: &Arc<Semaphore>,
) -> Option<(TcpStream, OwnedSemaphorePermit)> {
    let permit = open.clone().acquire_owned().await.ok()?;

    match listener.accept().await {
        Ok((stream, _)) => Some((stream, permit)),
        Err(_) => {
            tokio::time::sleep(Duration::from_millis(100)).await;
            None
        }
    }
}

/// Serve the viewer for the menu bar app until the person quits it, or
/// until interrupted, terminated or hung up.
pub async fn serve(client: Client, port: u16, program: crate::menubar::Program) -> Result<()> {
    use tokio::signal::unix::{SignalKind, signal};

    let output = client.output_dir()?.to_path_buf();
    let path = output.join(MENUBAR_RECORD);

    let (listener, viewer) = match bind(client, port, Some(&program.config)).await {
        Ok(bound) => bound,
        Err(error) if error.code == "port-in-use" => {
            let output = output.clone();
            return blocking(move || already_running(&output, port, error)).await;
        }
        Err(error) => return Err(error),
    };

    // Stopping it any of these ways releases control and removes its record.
    let mut terminated = signal(SignalKind::terminate())?;
    let mut hung_up = signal(SignalKind::hangup())?;

    let (lines, receiver) = tokio::sync::mpsc::unbounded_channel();
    *unpoisoned(&viewer.app) = Some(lines);

    // The record names a viewer with an app; none is left when the app
    // does not start.
    let mut app = App::launch(&program, &viewer.url, receiver).await?;
    let record = json!({
        "url": viewer.url,
        "pid": std::process::id(),
        "startedAt": crate::client::timestamp().to_string(),
    });
    control::write_private(&path, &serde_json::to_vec_pretty(&record)?)?;
    *unpoisoned(&viewer.record) = Some((path, record.clone()));

    // The URL is a bearer capability: it stays in the private record and
    // with the app, never in output.
    let mut shown = record.clone();
    shown["url"] = json!("held by the menu bar");

    if crate::ui::human() {
        println!("The menu bar is running. Quit it from its menu, or press Ctrl-C here.");
    } else {
        println!("{}", serde_json::to_string_pretty(&shown)?);
    }

    let timer_view = viewer.clone();
    let expiry = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;

            let expired = {
                let mut lease = timer_view.lease.lock().await;
                lease.expire();
                lease.retired.is_some()
            };
            if expired {
                let _ = timer_view.release(None).await;
            }
        }
    });

    let shutdown_view = viewer.clone();
    accept(listener, viewer.clone(), CONNECTIONS, async move {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminated.recv() => {}
            _ = hung_up.recv() => {}
            () = app.exited() => {}
        }
        let _ = shutdown_view.release(None).await;
    })
    .await;

    expiry.abort();
    let _ = viewer.release(None).await;

    // The record goes with this viewer, unless another has replaced it.
    if let Some((path, _)) = unpoisoned(&viewer.record).take() {
        remove_record(&path);
    }

    Ok(())
}

/// Remove the record at `path` when it names this process.
fn remove_record(path: &Path) {
    let ours = std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .is_some_and(|record| record["pid"] == std::process::id());
    if ours {
        let _ = std::fs::remove_file(path);
    }
}

/// The record of the running viewer, in the output folder.
pub const MENUBAR_RECORD: &str = "menubar.json";

/// A lock that a panicked holder left poisoned still guards plain data.
fn unpoisoned<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A read of the helper that took longer than the app waits.
fn late() -> Error {
    Error::new("transport", "The helper did not answer in time")
}

/// Run `work`, which waits on the local viewer's socket (for seconds at
/// most), off the async runtime.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| Error::new("menubar", "The menu bar's viewer did not answer"))?
}

/// The viewer recorded at `record`, if it serves `port` and answers there
/// now as the very process the record names. It blocks for up to a second:
/// call it off the async runtime.
pub fn running(record: &Path, port: u16) -> Option<Value> {
    std::fs::read(record)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(|running| {
            let url = running["url"].as_str().unwrap_or("");
            url.starts_with(&format!("http://127.0.0.1:{port}/"))
                && answering(url).is_some_and(|pid| running["pid"] == pid)
        })
}

/// The process of the viewer answering at `url` (its private address)
/// right now, if a viewer does.
fn answering(url: &str) -> Option<u32> {
    let (status, body) = exchange(url, "GET", "", None, Duration::from_secs(1)).ok()?;
    let reply: Value = serde_json::from_slice(&body).ok()?;
    if status != 200 || reply["service"] != SERVICE {
        return None;
    }

    reply["pid"]
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok())
}

/// One request to the local viewer at `url`, as its own origin, within
/// `deadline` overall: the reply's status and body. Replies are small; a
/// larger one is refused.
fn exchange(
    url: &str,
    method: &str,
    route: &str,
    body: Option<&[u8]>,
    deadline: Duration,
) -> Result<(u16, Vec<u8>)> {
    use std::io::{Read, Write};

    let refused = || Error::new("menubar", "The menu bar's viewer did not answer");
    // Any trouble on the wire reads the same to the person.
    let io = |_: std::io::Error| refused();

    let rest = url.strip_prefix("http://").ok_or_else(refused)?;
    let (authority, path) = rest.split_at(rest.find('/').ok_or_else(refused)?);
    let address: std::net::SocketAddr = authority.parse().map_err(|_| refused())?;

    let until = std::time::Instant::now() + deadline;
    let left = || {
        until
            .checked_duration_since(std::time::Instant::now())
            .filter(|left| !left.is_zero())
            .ok_or_else(refused)
    };

    let mut stream = std::net::TcpStream::connect_timeout(&address, left()?).map_err(io)?;
    stream.set_write_timeout(Some(left()?)).map_err(io)?;
    let mut head =
        format!("{method} {path}{route} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n");

    if let Some(body) = body {
        head.push_str(&format!(
            "Origin: http://{authority}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        ));
    }

    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).map_err(io)?;

    if let Some(body) = body {
        stream.write_all(body).map_err(io)?;
    }

    let mut reply = Vec::new();
    let mut chunk = [0u8; 4096];

    loop {
        stream.set_read_timeout(Some(left()?)).map_err(io)?;
        let read = stream.read(&mut chunk).map_err(io)?;
        if read == 0 {
            break;
        }

        reply.extend_from_slice(&chunk[..read]);

        if reply.len() > 1 << 16 {
            return Err(refused());
        }

        if let Some(parsed) = parse_response(&reply) {
            return Ok(parsed);
        }
    }

    parse_response(&reply).ok_or_else(refused)
}

/// A complete HTTP/1.1 reply's status and body, framed by Content-Length
/// or chunked encoding; None until all of it has arrived.
fn parse_response(reply: &[u8]) -> Option<(u16, Vec<u8>)> {
    let split = reply.windows(4).position(|window| window == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&reply[..split]).ok()?;
    let rest = &reply[split + 4..];
    let mut lines = head.split("\r\n");
    let status = lines
        .next()?
        .strip_prefix("HTTP/1.1 ")?
        .get(..3)?
        .parse::<u16>()
        .ok()?;

    let mut length = None;
    let mut chunked = false;

    for line in lines {
        let (name, value) = line.split_once(':')?;
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            length = Some(value.parse::<usize>().ok()?);
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            chunked = value.eq_ignore_ascii_case("chunked");
        }
    }

    if chunked {
        let mut body = Vec::new();
        let mut at = rest;

        loop {
            let end = at.windows(2).position(|window| window == b"\r\n")?;
            let size = std::str::from_utf8(&at[..end])
                .ok()?
                .split(';')
                .next()?
                .trim();
            let size = usize::from_str_radix(size, 16).ok()?;
            at = &at[end + 2..];

            if size == 0 {
                return Some((status, body));
            }

            body.extend_from_slice(at.get(..size)?);
            at = at.get(size + 2..)?;
        }
    }

    let length = length?;
    Some((status, rest.get(..length)?.to_vec()))
}

/// The port is taken. When the menu bar holds it, say so and succeed.
fn already_running(output: &Path, port: u16, taken: Error) -> Result<()> {
    let Some(bar) = running(&output.join(MENUBAR_RECORD), port) else {
        return Err(taken);
    };

    if crate::ui::human() {
        println!(
            "{} The menu bar is already running. Click Atlas at the top of the screen; quit it from its menu.",
            crate::ui::style::ok()
        );
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({"ok": true, "alreadyRunning": true, "running": true, "pid": bar["pid"]})
            )?
        );
    }

    Ok(())
}

/// Open the viewer window in the menu bar app, starting the menu bar first
/// when it is not running.
pub async fn monitor(config: &Path, port: u16, output: &Path) -> Result<()> {
    let (_, record) = crate::menubar::launch(config, port, output).await?;
    let url = record["url"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::new("menubar", "The menu bar's record names no viewer"))?;
    blocking(move || post_local(&url, "app/viewer", &json!({}))).await?;

    if crate::ui::human() {
        println!(
            "{} The viewer is open. Watch the helper there, or take control.",
            crate::ui::style::ok()
        );
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({"ok": true, "viewer": "open"}))?
        );
    }

    Ok(())
}

/// POST `body` to `route` of the local viewer at `url`, as its own origin,
/// and read its JSON reply; a refusal is an error in its own words.
fn post_local(url: &str, route: &str, body: &Value) -> Result<Value> {
    let (status, reply) = exchange(
        url,
        "POST",
        route,
        Some(&serde_json::to_vec(body)?),
        Duration::from_secs(5),
    )?;
    let reply: Value = serde_json::from_slice(&reply)
        .map_err(|_| Error::new("menubar", "The menu bar's viewer did not answer"))?;

    if status != 200 {
        return Err(Error::new(
            "menubar",
            reply["error"]
                .as_str()
                .unwrap_or("The menu bar's viewer refused")
                .to_owned(),
        ));
    }

    Ok(reply)
}

/// Render the menu bar panel, fed by a private viewer on a free port, in the
/// light and dark appearances. For visual checks; it never takes control.
pub async fn render_menubar(
    client: Client,
    program: &crate::menubar::Program,
    directory: &Path,
) -> Result<Value> {
    let (listener, viewer) = bind(client, 0, None).await?;
    let server = tokio::spawn(accept(
        listener,
        viewer.clone(),
        CONNECTIONS,
        std::future::pending(),
    ));
    let mut paths = Vec::new();
    let mut outcome = Ok(());

    for (name, dark) in [("light", false), ("dark", true)] {
        let path = directory.join(format!("menubar-{name}.png"));
        outcome = crate::menubar::render(program, &viewer.url, &path, dark).await;
        if outcome.is_err() {
            break;
        }
        paths.push(path);
    }

    server.abort();
    let _ = viewer.release(None).await;
    outcome?;

    Ok(json!({"ok": true, "paths": paths}))
}

fn header<'a>(headers: &'a HeaderMap, key: &str) -> Option<&'a str> {
    headers.get(key)?.to_str().ok()
}

/// A client can abort any request. Its connection drops the request's
/// future when that happens, which could stop between a quarantine or
/// lease change and its matching cleanup. Each request therefore runs to
/// completion in its own task.
async fn handle(viewer: Arc<Viewer>, request: Request) -> Response {
    tokio::spawn(respond(viewer, request))
        .await
        .unwrap_or_else(|_| {
            json_response(500, json!({"ok": false, "error": "Viewer request failed"}))
        })
}

async fn respond(viewer: Arc<Viewer>, request: Request) -> Response {
    let method = request.method().clone();
    let origin = format!("http://{}", viewer.authority);
    if header(request.headers(), "host") != Some(&viewer.authority)
        || (method == Method::POST && header(request.headers(), "origin") != Some(&origin))
        || header(request.headers(), "origin").is_some_and(|v| v != origin)
    {
        return json_response(
            403,
            json!({"ok": false, "error": "Local same-origin access only"}),
        );
    }

    let Some(route) = request
        .uri()
        .path()
        .strip_prefix(&viewer.prefix)
        .map(str::to_owned)
    else {
        return json_response(404, json!({"ok": false, "error": "Unknown viewer address"}));
    };

    // The address itself answers, with the process serving it, so a
    // running viewer can be found and stopped however busy it is.
    if method == Method::GET && route.is_empty() {
        return json_response(
            200,
            json!({"ok": true, "service": SERVICE, "pid": std::process::id()}),
        );
    }

    // Ending control has slots of its own, so a busy viewer still lets go.
    let urgent =
        method == Method::POST && matches!(route.as_str(), "control/release" | "control/revoke");
    let slots = if urgent {
        &viewer.urgent
    } else {
        &viewer.requests
    };
    let Ok(_slot) = slots.try_acquire() else {
        return json_response(503, json!({"ok": false, "error": "Viewer is busy"}));
    };

    if method == Method::GET {
        if route == "status.json" {
            return json_response(200, viewer.status().await);
        }

        if route == "frame.png" {
            return match viewer.frame().await {
                Ok((key, frame)) => response(
                    200,
                    "image/png",
                    // Served from the shared frame, not copied.
                    Body::from(Bytes::from_owner(frame.png.clone())),
                    &[
                        ("X-Frame-Id", key),
                        ("X-Display-Width", frame.display.width.to_string()),
                        ("X-Display-Height", frame.display.height.to_string()),
                        (
                            "X-Session-Revision",
                            format!(
                                "{}:{}:{}:{}:{}:{}:{}:{}",
                                frame.epoch.uid,
                                frame.epoch.capture_pid,
                                frame.display.width,
                                frame.display.height,
                                frame.display.scale,
                                frame.epoch.security_session_id,
                                frame.epoch.display_id,
                                frame.epoch.input_pid
                            ),
                        ),
                    ],
                ),
                Err(error) => {
                    // Ending control finishes by itself; the answer is not held.
                    viewer.release_in_background();
                    json_response(503, json!(error))
                }
            };
        }
    }

    if method != Method::POST
        || !matches!(
            route.as_str(),
            "input"
                | "control/acquire"
                | "control/renew"
                | "control/release"
                | "control/revoke"
                | "recover"
                | "app/viewer"
        )
    {
        return json_response(404, json!({"ok": false, "error": "Unknown operation"}));
    }

    if request.headers().contains_key("transfer-encoding")
        || header(request.headers(), "content-type").and_then(|v| v.split(';').next())
            != Some("application/json")
    {
        return json_response(
            400,
            json!({"ok": false, "error": "Send JSON with Content-Length"}),
        );
    }

    let length = header(request.headers(), "content-length")
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| (1..=MAX_BODY).contains(n));
    let Some(length) = length else {
        return json_response(
            400,
            json!({"ok": false, "error": format!("Request body must contain 1–{MAX_BODY} bytes")}),
        );
    };

    let body = match tokio::time::timeout(
        Duration::from_secs(5),
        to_bytes(request.into_body(), MAX_BODY),
    )
    .await
    {
        Ok(Ok(bytes)) if bytes.len() == length => bytes,
        _ => {
            return json_response(
                400,
                json!({"ok": false, "error": "Incomplete or oversized body"}),
            );
        }
    };

    let body: Value = match serde_json::from_slice(&body) {
        Ok(value @ Value::Object(_)) => value,
        _ => {
            return json_response(
                400,
                json!({"ok": false, "error": "Request must be a JSON object"}),
            );
        }
    };

    if route == "app/viewer" {
        return match viewer.send(Line::Viewer) {
            Ok(()) => json_response(200, json!({"ok": true, "viewer": "open"})),
            Err(error) => json_response(409, json!(error)),
        };
    }

    let outcome = if route == "recover" {
        viewer.recover().await
    } else {
        viewer.control(&route, &body).await
    };

    match outcome {
        Ok(result) => {
            // Only the lease this request named ends; a request without one
            // never ends someone else's.
            if result["ok"] != true
                && let Some(owner) = body["lease"].as_str()
            {
                let _ = viewer.release(Some(owner)).await;
            }

            json_response(if result["ok"] == true { 200 } else { 409 }, result)
        }
        Err(error) => {
            if !matches!(error.code, "busy" | "lease-lost" | "invalid-json" | "input")
                && let Some(owner) = body["lease"].as_str()
            {
                let _ = viewer.release(Some(owner)).await;
            }

            json_response(
                if matches!(error.code, "invalid-json" | "input") {
                    400
                } else {
                    409
                },
                json!(error),
            )
        }
    }
}

fn json_response(code: u16, value: Value) -> Response {
    response(code, "application/json", Body::from(value.to_string()), &[])
}

fn response(code: u16, mime: &str, body: Body, headers: &[(&str, String)]) -> Response {
    let mut response = Response::new(body);
    *response.status_mut() =
        StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    let fixed = [
        ("Content-Type", mime),
        ("Cache-Control", "no-store"),
        ("Referrer-Policy", "no-referrer"),
        ("X-Content-Type-Options", "nosniff"),
        (
            "Content-Security-Policy",
            concat!(
                "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; ",
                "img-src 'self' blob:; connect-src 'self'; frame-ancestors 'none'; ",
                "base-uri 'none'; form-action 'none'",
            ),
        ),
    ];
    let given = headers.iter().map(|(key, value)| (*key, value.as_str()));

    for (key, value) in fixed.into_iter().chain(given) {
        if let (Ok(key), Ok(value)) = (
            axum::http::HeaderName::try_from(key),
            HeaderValue::from_str(value),
        ) {
            response.headers_mut().insert(key, value);
        }
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETTINGS: &str = "helper_uid = 60001\nsocket_path = \"c.sock\"\noutput_dir = \"out\"\n\
                            identity_probe = \"probe\"\ninput_socket = \"i.sock\"\n";

    /// A client for a helper that is not there, with controller state
    /// private to the test.
    fn client(dir: &Path) -> Client {
        let config = Config::parse(SETTINGS.as_bytes(), &dir.join("u.toml")).unwrap();
        let mut client = Client::new(config).unwrap();
        client.controller = control::Controller::at(60001, &dir.join("state"));
        client
    }

    /// A viewer bound to a free port, for a helper that is not there.
    async fn bound(dir: &Path) -> (TcpListener, Arc<Viewer>) {
        bind(client(dir), 0, None).await.unwrap()
    }

    async fn unreachable(dir: &Path) -> Arc<Viewer> {
        bound(dir).await.1
    }

    /// A viewer answering on its own port, at most `connections` at once,
    /// and the task serving it.
    async fn served(dir: &Path, connections: usize) -> (Arc<Viewer>, tokio::task::JoinHandle<()>) {
        let (listener, viewer) = bound(dir).await;
        let server = tokio::spawn(accept(
            listener,
            viewer.clone(),
            connections,
            std::future::pending(),
        ));

        (viewer, server)
    }

    /// [`answering`] off the runtime.
    async fn answering_off(url: &str) -> Option<u32> {
        let url = url.to_owned();
        tokio::task::spawn_blocking(move || answering(&url))
            .await
            .unwrap()
    }

    #[test]
    fn replies_are_read_by_their_framing_and_status() {
        let plain = b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 11\r\n\r\n{\"ok\":true}";
        assert_eq!(
            parse_response(plain),
            Some((200, b"{\"ok\":true}".to_vec()))
        );
        assert_eq!(
            parse_response(&plain[..plain.len() - 3]),
            None,
            "not all here yet"
        );

        let chunked = b"HTTP/1.1 409 Conflict\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n{\"ok\r\n8\r\n\":false}\r\n0\r\n\r\n";
        assert_eq!(
            parse_response(chunked),
            Some((409, b"{\"ok\":false}".to_vec()))
        );
        assert_eq!(parse_response(b"HTTP/1.1 200 OK\r\n\r\n"), None, "unframed");
        assert_eq!(parse_response(b"garbage\r\n\r\n"), None);
    }

    #[tokio::test]
    async fn post_local_reads_the_viewers_answer_in_its_own_words() {
        let dir = tempfile::tempdir().unwrap();
        let (viewer, server) = served(dir.path(), CONNECTIONS).await;
        let url = viewer.url.clone();
        let refused = tokio::task::spawn_blocking({
            let url = url.clone();
            move || post_local(&url, "app/viewer", &json!({}))
        })
        .await
        .unwrap();
        assert_eq!(refused.unwrap_err().code, "menubar", "no menu bar attached");

        let (lines, mut received) = tokio::sync::mpsc::unbounded_channel();
        *viewer.app.lock().unwrap() = Some(lines);

        let opened = tokio::task::spawn_blocking({
            let url = url.clone();
            move || post_local(&url, "app/viewer", &json!({}))
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(opened["viewer"], "open");
        assert_eq!(received.recv().await, Some(Line::Viewer));
        assert_eq!(answering_off(&url).await, Some(std::process::id()));

        server.abort();
    }

    #[tokio::test]
    async fn the_viewer_window_opens_only_through_the_menu_bar_it_serves() {
        let dir = tempfile::tempdir().unwrap();
        let viewer = unreachable(dir.path()).await;
        assert_eq!(viewer.send(Line::Viewer).unwrap_err().code, "menubar");

        let (lines, mut received) = tokio::sync::mpsc::unbounded_channel();
        *viewer.app.lock().unwrap() = Some(lines);
        viewer.send(Line::Viewer).unwrap();
        assert_eq!(received.recv().await, Some(Line::Viewer));
        drop(received);
        assert_eq!(viewer.send(Line::Viewer).unwrap_err().code, "menubar");
    }

    #[tokio::test]
    async fn recovery_is_refused_while_control_is_held() {
        let dir = tempfile::tempdir().unwrap();
        let viewer = unreachable(dir.path()).await;
        let controller = viewer.client().controller.clone();
        viewer.lease.lock().await.acquire(&controller).unwrap();
        let refused = viewer.recover().await.unwrap_err();
        assert_eq!(refused.code, "busy");
        assert!(controller.pending().unwrap().is_empty());
    }

    #[tokio::test]
    async fn revoking_ends_any_lease_and_keeps_unconfirmed_cleanup_pending() {
        let dir = tempfile::tempdir().unwrap();
        let viewer = unreachable(dir.path()).await;
        let controller = viewer.client().controller.clone();
        let idle = viewer.control("control/revoke", &json!({})).await.unwrap();
        assert_eq!(idle, json!({"ok": true, "released": false}));
        viewer.lease.lock().await.acquire(&controller).unwrap();

        // The helper cannot confirm native cleanup, so control ends but the
        // release stays an unknown outcome for recovery.
        let error = viewer
            .control("control/revoke", &json!({}))
            .await
            .unwrap_err();
        assert!(error.uncertain);
        assert!(!viewer.lease.lock().await.active());
        assert_eq!(controller.unresolved().unwrap().len(), 1);
        assert_eq!(controller.acquire(false).unwrap_err().code, "uncertain");
    }

    /// Undoing an acquire whose lease was lost meanwhile is a release like
    /// any other: unconfirmed, it stays an unknown outcome.
    #[tokio::test]
    async fn an_unconfirmed_native_release_stays_pending() {
        let dir = tempfile::tempdir().unwrap();
        let viewer = unreachable(dir.path()).await;
        let controller = viewer.client().controller.clone();
        let error = viewer.release_native("lost-lease").await.unwrap_err();
        assert!(error.uncertain);
        assert_eq!(error.code, "uncertain");
        assert_eq!(controller.unresolved().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn failed_recovery_keeps_unknown_outcomes_pending() {
        let dir = tempfile::tempdir().unwrap();
        let viewer = unreachable(dir.path()).await;
        let controller = viewer.client().controller.clone();

        // Without a pending outcome recovery only starts input, adding nothing.
        assert!(viewer.recover().await.is_err());
        assert!(controller.pending().unwrap().is_empty());

        drop(controller.quarantine().unwrap());
        // With one it takes the verified restart, which cannot confirm an
        // unreachable helper, so the outcome stays unknown.
        assert!(viewer.recover().await.is_err());
        assert_eq!(controller.unresolved().unwrap().len(), 1);
        assert_eq!(controller.require_known().unwrap_err().code, "uncertain");

        let status = viewer.status().await;
        assert_eq!(status["overview"]["mode"], "unavailable");
        assert_eq!(status["overview"]["pending"], 1);
    }

    /// Only the viewer process a record names counts as running: not
    /// another process, and not another program answering on the port.
    #[tokio::test]
    async fn a_running_viewer_is_the_process_its_record_names() {
        let dir = tempfile::tempdir().unwrap();
        let (viewer, server) = served(dir.path(), CONNECTIONS).await;
        let port = viewer
            .authority
            .rsplit(':')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let record = dir.path().join(MENUBAR_RECORD);
        let check = |record: std::path::PathBuf| {
            tokio::task::spawn_blocking(move || running(&record, port))
        };

        let write = |pid: u32, url: &str| {
            std::fs::write(&record, json!({"url": url, "pid": pid}).to_string()).unwrap();
        };

        write(std::process::id(), &viewer.url);
        assert!(check(record.clone()).await.unwrap().is_some());

        write(1, &viewer.url);
        assert!(
            check(record.clone()).await.unwrap().is_none(),
            "another process"
        );

        // A program that answers anything with a 200.
        let other = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/secret/", other.local_addr().unwrap());
        std::thread::spawn(move || {
            use std::io::{Read, Write};

            for mut stream in other.incoming().flatten() {
                let _ = stream.read(&mut [0u8; 1024]);
                let _ =
                    stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 11\r\n\r\n{\"ok\":true}");
            }
        });
        assert_eq!(answering_off(&url).await, None);

        server.abort();
    }

    /// With every request slot taken, the viewer can still be found and
    /// control can still be ended.
    #[tokio::test]
    async fn finding_the_viewer_and_ending_control_never_wait_for_a_slot() {
        let dir = tempfile::tempdir().unwrap();
        let (viewer, server) = served(dir.path(), CONNECTIONS).await;
        let _busy = viewer
            .requests
            .try_acquire_many(CONCURRENT_REQUESTS as u32)
            .unwrap();
        let url = viewer.url.clone();

        let status = tokio::task::spawn_blocking({
            let url = url.clone();
            move || exchange(&url, "GET", "status.json", None, Duration::from_secs(5))
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(status.0, 503, "reads wait for a slot");

        assert_eq!(answering_off(&url).await, Some(std::process::id()));

        let ended = tokio::task::spawn_blocking(move || {
            let revoked = post_local(&url, "control/revoke", &json!({}))?;
            let released = post_local(&url, "control/release", &json!({"lease": "gone"}))?;
            Ok::<_, Error>((revoked, released))
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(ended.0, json!({"ok": true, "released": false}));
        assert_eq!(ended.1, json!({"ok": true}));

        server.abort();
    }

    /// Connections past the limit wait to be accepted, and are once others
    /// close.
    #[tokio::test]
    async fn connections_past_the_limit_wait_their_turn() {
        let dir = tempfile::tempdir().unwrap();
        let (viewer, server) = served(dir.path(), 4).await;
        let address: std::net::SocketAddr = viewer.authority.parse().unwrap();
        let idle: Vec<_> = (0..4)
            .map(|_| std::net::TcpStream::connect(address).unwrap())
            .collect();
        tokio::time::sleep(Duration::from_millis(200)).await;

        assert_eq!(answering_off(&viewer.url).await, None, "all are taken");

        drop(idle);
        assert_eq!(answering_off(&viewer.url).await, Some(std::process::id()));

        server.abort();
    }

    /// Requests asking for a frame while one is taken share it, and a
    /// helper that never answers ends the capture before the app gives up.
    /// The status read ends in time too.
    #[tokio::test]
    async fn frames_asked_together_share_one_capture_that_ends_in_time() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("s.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        let (accepted, mut connections) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut held = Vec::new();

            while let Ok((stream, _)) = listener.accept().await {
                held.push(stream);
                let _ = accepted.send(());
            }
        });

        let mut client = client(dir.path());
        client.transport.path = socket;
        client.transport.peer_uid = nix::unistd::getuid().as_raw();
        let (_, viewer) = bind(client, 0, None).await.unwrap();

        let started = std::time::Instant::now();
        let (first, second, status) = tokio::join!(viewer.frame(), viewer.frame(), viewer.status());
        assert!(started.elapsed() < Duration::from_secs(7));
        assert_ne!(status["ready"], true);

        for outcome in [first, second] {
            let error = outcome.err().unwrap();
            assert_eq!(
                (error.code, error.error.as_str()),
                ("transport", "The helper did not answer in time")
            );
        }

        let mut asked = 0;

        while connections.try_recv().is_ok() {
            asked += 1;
        }

        assert_eq!(asked, 2, "one capture and one status read");
    }

    /// A capture taken since the latest input is shared; one started
    /// before it is waited out and a fresh one taken.
    #[tokio::test]
    async fn a_capture_from_before_the_latest_input_is_not_shared() {
        let dir = tempfile::tempdir().unwrap();
        let viewer = unreachable(dir.path()).await;
        let running = |revision| {
            let (sender, outcome) = watch::channel(None);
            *viewer.capture.lock().unwrap() = Some(Capture { revision, outcome });
            sender
        };

        let ask = || {
            let viewer = viewer.clone();
            tokio::spawn(async move { viewer.frame().await })
        };

        let current = running(0);
        let shared = ask();
        tokio::time::sleep(Duration::from_millis(50)).await;
        current
            .send(Some(Err(("capture", "the shared picture".into()))))
            .unwrap();
        *viewer.capture.lock().unwrap() = None;
        assert_eq!(
            shared.await.unwrap().err().unwrap().error,
            "the shared picture"
        );

        let old = running(0);
        viewer.frames.lock().await.input_revision = 1;
        let fresh = ask();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!fresh.is_finished(), "it waits out the running capture");

        *viewer.capture.lock().unwrap() = None;
        old.send(Some(Err(("capture", "an old picture".into()))))
            .unwrap();
        assert_ne!(
            fresh.await.unwrap().err().unwrap().error,
            "an old picture",
            "it took its own"
        );
    }

    /// Once the settings name another helper, the running viewer serves it:
    /// control held in the previous one ends (unconfirmed, it stays pending
    /// there), and the record moves to the new output folder.
    #[tokio::test]
    async fn a_settings_change_moves_the_viewer_to_the_new_helper() {
        let dir = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(dir.path()).unwrap();
        let settings = base.join("u.toml");
        std::fs::write(&settings, SETTINGS).unwrap();

        let mut client = Client::new(Config::load(&settings).unwrap()).unwrap();
        let controller = control::Controller::at(60001, &base.join("state"));
        client.controller = controller.clone();
        let (_, viewer) = bind(client, 0, Some(&settings)).await.unwrap();

        let record = base.join("out").join(MENUBAR_RECORD);
        std::fs::create_dir_all(record.parent().unwrap()).unwrap();
        let written = json!({"url": viewer.url, "pid": std::process::id()});
        std::fs::write(&record, written.to_string()).unwrap();
        *viewer.record.lock().unwrap() = Some((record.clone(), written));
        viewer.lease.lock().await.acquire(&controller).unwrap();

        viewer.follow_settings().await;
        assert_eq!(viewer.client().config.helper_uid, 60001, "unchanged");
        assert!(viewer.lease.lock().await.active());

        std::fs::write(
            &settings,
            SETTINGS
                .replace("60001", "60002")
                .replace("\"out\"", "\"moved\""),
        )
        .unwrap();
        viewer.follow_settings().await;

        assert_eq!(viewer.client().config.helper_uid, 60002);
        assert!(!viewer.lease.lock().await.active());
        assert_eq!(controller.unresolved().unwrap().len(), 1);

        let moved = base.join("moved").join(MENUBAR_RECORD);
        assert!(moved.exists());
        assert!(!record.exists());
        assert_eq!(viewer.record.lock().unwrap().as_ref().unwrap().0, moved);
    }
}
