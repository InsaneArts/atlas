//! The local service Atlas.app talks to: a capability URL on the loopback
//! interface that answers status, pictures of the helper, control and input
//! for one menu bar, and keeps a private record of itself in the output
//! directory so `atlas monitor` and the login item can find it.
use crate::{
    client::{Client, Frame},
    config::Config,
    control::{self, ActionPermit, Lease, SharedLease},
    error::{Error, Result},
    helper_info,
    menubar::{App, Line},
    overview::{READ_DEADLINE, late},
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
    sync::{Mutex, OwnedSemaphorePermit, RwLock, RwLockReadGuard, Semaphore, watch},
};

/// How long a frame is kept and may be named by control requests.
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
/// How long a new connection may take to ask for something at the viewer's
/// private address. One that has not closes, so connections nobody uses
/// cannot keep every place taken.
const FIRST_REQUEST: Duration = Duration::from_secs(3);
/// The largest request body, in bytes.
const MAX_BODY: usize = 32768;
/// What the viewer's own address answers, so it is told apart from any
/// other program on its port.
const SERVICE: &str = "atlas-viewer";
/// The most activity rows and screenshots one read lists: what the app's
/// tabs show, a day or so of an agent's work.
const ACTIVITY_ROWS: usize = 200;
const SHOTS: usize = 200;

/// The recent frames, each under the key control requests name it by.
#[derive(Default)]
struct Frames {
    items: VecDeque<(String, Arc<Frame>)>,
    /// Counts input sent, so a capture can tell whether it shows the latest.
    input_revision: u64,
    /// The input revision the newest frame was captured at.
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

/// One running viewer: the helper it serves, the control lease the app
/// holds, and the frames it has shown.
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
    /// The helper the lease was taken on. Control ends there, and input
    /// goes there, even once the settings name another helper.
    lease_client: std::sync::Mutex<Option<Arc<Client>>>,
    /// Shared by each change being made in the helper (taking or ending
    /// control, input, recovery) for as long as it runs. Shutdown takes it
    /// whole, so it waits for those in progress, and sets it to true, so no
    /// more begin.
    closed: RwLock<bool>,
    frames: Mutex<Frames>,
    capture: std::sync::Mutex<Option<Capture>>,
    requests: Semaphore,
    urgent: Semaphore,
    /// The viewer's private address. Whoever holds it may control the
    /// helper, so it is never printed.
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
    /// The helper's app icons, made once.
    icons: helper_info::Icons,
}

impl Viewer {
    /// The helper this viewer serves right now.
    fn client(&self) -> Arc<Client> {
        unpoisoned(&self.client).clone()
    }

    /// The helper control ends in: the one the lease was taken on, or the
    /// current one when none was.
    fn holder(&self) -> Arc<Client> {
        let held = unpoisoned(&self.lease_client).clone();
        held.unwrap_or_else(|| self.client())
    }

    /// Follow the settings file. Only a change to the file's text is read.
    /// A new output folder alone keeps the same helper: control carries on
    /// and the record moves there. Once the file names another helper
    /// (Getting Started set one up, say), the viewer serves that one without
    /// a restart. Control held in the previous helper ends there first, in a
    /// task of its own that is waited on for a moment; while control is
    /// still held (or being taken) the move waits for a later read.
    async fn follow_settings(self: &Arc<Self>) {
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

        if same_helper(&config, &current.config) {
            self.follow_output(&current, config, text).await;
            return;
        }

        let Ok(mut next) = Client::new(config) else {
            return;
        };
        next.actor = current.actor;

        let ending = self.release_in_background();
        let _ = tokio::time::timeout(Duration::from_secs(1), ending).await;

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

    /// The settings name another output folder for the same helper: serve
    /// it with `config` (whose file reads `text`) in place of `current`,
    /// control and all, and move the record there.
    async fn follow_output(&self, current: &Arc<Client>, config: Config, text: Vec<u8>) {
        let mut next = Client::clone(current);
        next.config = config;
        let next = Arc::new(next);

        {
            let _lease = self.lease.lock().await;
            let mut client = unpoisoned(&self.client);
            if !Arc::ptr_eq(&client, current) {
                return;
            }

            let mut held = unpoisoned(&self.lease_client);
            if held.as_ref().is_some_and(|held| Arc::ptr_eq(held, current)) {
                *held = Some(next.clone());
            }

            *client = next;
            *unpoisoned(&self.settings_text) = Some(text);
        }

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
    /// input is waited out, then a fresh one taken. All of it, waiting
    /// included, ends within one deadline, so the app hears why before it
    /// gives up.
    async fn frame(&self) -> Result<(String, Arc<Frame>)> {
        let until = tokio::time::Instant::now() + READ_DEADLINE;

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
                    let Ok(waited) =
                        tokio::time::timeout_at(until, outcome.wait_for(Option::is_some)).await
                    else {
                        return Err(late());
                    };

                    let shared = waited.ok().and_then(|outcome| outcome.clone());
                    match shared {
                        Some(captured) if current => {
                            return captured.map_err(|(code, message)| Error::new(code, message));
                        }
                        _ => continue,
                    }
                }
            };

            let _clear = Capturing(&self.capture);
            let outcome = tokio::time::timeout_at(until, self.take_frame())
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

    /// The recent frame kept under `key`.
    async fn observed(&self, key: &str) -> Result<Arc<Frame>> {
        self.frames
            .lock()
            .await
            .items
            .iter()
            .find(|(id, _)| id == key)
            .map(|(_, frame)| frame.clone())
            .filter(|frame| frame.at.elapsed() < FRAME_LIFETIME)
            .ok_or_else(|| {
                Error::new(
                    "stale-frame",
                    "Wait for a fresh frame before taking control",
                )
            })
    }

    /// End `owner`'s control (any control, without one) in the helper it
    /// was taken on.
    async fn release(&self, owner: Option<&str>) -> Result<()> {
        let (previous, client) = {
            let mut lease = self.lease.lock().await;
            (lease.begin_release(owner)?, self.holder())
        };

        if let Some(previous) = previous {
            let completion = release_native(&client, &previous, false).await;

            {
                let mut lease = self.lease.lock().await;
                lease.end_release();
                if !lease.active() {
                    *unpoisoned(&self.lease_client) = None;
                }
            }

            completion?;
        }

        Ok(())
    }

    /// End whichever control is held in a task of its own, which finishes
    /// however long the helper takes; the caller may wait on it or not.
    /// Once shutdown began, its own release ends control in place of this.
    fn release_in_background(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let viewer = self.clone();

        tokio::spawn(async move {
            let Some(_changing) = viewer.changing().await else {
                return;
            };

            let _ = viewer.release(None).await;
        })
    }

    /// Leave to change something in the helper, held until the change ends;
    /// None once shutdown began.
    async fn changing(&self) -> Option<RwLockReadGuard<'_, bool>> {
        let closed = self.closed.read().await;
        (!*closed).then_some(closed)
    }

    /// Wait for every change in progress in the helper to end, however long
    /// that takes, then let no more begin.
    async fn close(&self) {
        *self.closed.write().await = true;
    }

    /// Take control of `client`'s helper for a new lease, in `epoch`: the
    /// lease's token. Until the helper answers, the acquire counts as input
    /// in progress, so control stays held (the settings cannot move to
    /// another helper meanwhile, nor another controller come in). When the
    /// lease ended meanwhile, or the answer is unknown, control is ended
    /// again on that same helper.
    async fn take_control(&self, client: &Arc<Client>, epoch: &Epoch) -> Result<String> {
        let token = {
            let mut lease = self.lease.lock().await;

            // Settings that moved to another helper meanwhile refuse it.
            if !Arc::ptr_eq(client, &self.client()) {
                return Err(Error::new(
                    "session-changed",
                    "Wait for a fresh frame from the current helper",
                ));
            }

            let token = lease.acquire(&client.controller)?;
            *unpoisoned(&self.lease_client) = Some(client.clone());
            *self.lease_epoch.lock().await = Some(epoch.clone());
            token
        };

        // Nothing was asked of the helper yet, so a lease ended this soon
        // just fails here.
        let _acquiring = ActionPermit::begin(self.lease.clone(), &token).await?;
        let acquired = client.acquire_input(&token, epoch).await;
        let unknown = acquired.as_ref().is_err_and(|error| error.uncertain);
        let mut lease = self.lease.lock().await;

        let error = match acquired {
            Ok(_) => match lease.require(&token) {
                Ok(()) => return Ok(token),
                Err(lost) => lost,
            },
            Err(error) if !error.uncertain => {
                // Refused for certain: nothing to undo, the lease just ends.
                if lease.require(&token).is_ok() {
                    lease.release();
                }

                return Err(error);
            }
            Err(error) => error,
        };

        // Control may be granted with no lease left to hold it, or no one
        // knows: end it on the helper it was asked of, and say when that
        // could not be confirmed.
        let ending = lease.begin_release(Some(&token));
        drop(lease);

        let cleanup = release_native(client, &token, unknown).await;
        if matches!(ending, Ok(Some(_))) {
            self.lease.lock().await.end_release();
        }

        Err(match cleanup {
            Ok(()) => error,
            Err(cleanup) => Error {
                error: format!("{} {}", error.error, cleanup.error),
                ..cleanup
            },
        })
    }

    /// The helper's status and overview as the app shows them, with
    /// whether control is held.
    async fn status(self: &Arc<Self>) -> Value {
        // Reading waits on the helper, for a few seconds at most, following
        // the settings included.
        let until = tokio::time::Instant::now() + READ_DEADLINE;
        self.follow_settings().await;

        let client = self.client();
        let mut status = crate::overview::report(&client, until).await;

        // Ending control waits on the helper too: it finishes by itself
        // when that takes longer than a moment.
        if status["ready"] != true {
            let ending = self.release_in_background();
            let _ = tokio::time::timeout(Duration::from_secs(1), ending).await;
        }

        let active = self.lease.lock().await.active();
        status["controlActive"] = json!(active);

        let (pending, total) = crate::overview::pending(&client.controller);

        self.start_input_after_login(&client, &status["session"], total == 0 && !active);

        let activity = client
            .output_dir()
            .map(|directory| crate::overview::activity(directory, crate::overview::ROWS))
            .unwrap_or_default();
        status["overview"] = crate::overview::overview(
            &status,
            active,
            pending,
            &activity,
            crate::client::timestamp(),
        );

        if let Some(name) = status["overview"]["account"].as_str() {
            status["overview"]["accountName"] = json!(self.account_title(name).await);
        }

        status
    }

    /// The helper account `name`'s full name, as people know it. It is
    /// looked up once per account (off the runtime, as it asks directory
    /// services), also when it has none.
    async fn account_title(&self, name: &str) -> Option<String> {
        let known = unpoisoned(&self.account_name)
            .clone()
            .filter(|(account, _)| account == name);
        if let Some((_, title)) = known {
            return title;
        }

        let account = name.to_owned();
        let title = tokio::task::spawn_blocking(move || crate::setup::real_name(&account))
            .await
            .ok()?;

        *unpoisoned(&self.account_name) = Some((name.to_owned(), title.clone()));
        title
    }

    /// What was done in the helper lately, newest first, as the app lists
    /// it. Reading it records nothing.
    async fn activity(&self) -> Result<Value> {
        let client = self.client();
        // A folder that can't be read is said, never shown as no activity.
        client.output_dir()?;
        let rows = tokio::task::spawn_blocking(move || {
            client
                .output_dir()
                .map(|directory| crate::overview::activity(directory, ACTIVITY_ROWS))
                .unwrap_or_default()
        })
        .await
        .unwrap_or_default();

        let now = crate::client::timestamp();
        let rows = rows
            .iter()
            .map(|row| activity_row(row, now))
            .collect::<Vec<_>>();
        Ok(json!({"ok": true, "activity": rows}))
    }

    /// The apps open in the helper with their windows, front to back, and
    /// the icons of those installed where apps go. It only reads: nothing
    /// is recorded and no control is taken.
    async fn apps(&self) -> Result<Value> {
        let client = self.client();
        let read = async {
            let apps = answered(client.apps().await?)?;
            let filter = crate::agent::WindowFilter {
                app: None,
                title: None,
                all: false,
            };
            let windows = crate::agent::list(&client, &filter).await?;
            listed_apps(&apps["apps"], &windows["windows"])
        };
        let mut apps = tokio::time::timeout(READ_DEADLINE, read)
            .await
            .map_err(|_| late())??;

        for app in apps.iter_mut().take(helper_info::ICON_CACHE) {
            if let Some(bundle) = app["bundle"].as_str().map(PathBuf::from)
                && let Some(icon) = self.icons.get(bundle).await
            {
                app["icon"] = json!(icon);
            }
        }

        Ok(json!({"ok": true, "apps": apps}))
    }

    /// The screenshots agents took, newest first: each one's name, when it
    /// was taken (milliseconds since 1970) and its size when its header
    /// reads.
    async fn screenshots(&self) -> Result<Value> {
        let client = self.client();
        // A folder that can't be read is said, never shown as no screenshots.
        client.output_dir()?;
        let shots = tokio::task::spawn_blocking(move || {
            let Ok(output) = client.output_dir() else {
                return Vec::new();
            };

            helper_info::captures(output, SHOTS)
                .into_iter()
                .map(|(at, name)| {
                    let size = helper_info::capture_size(output, &name);
                    let taken = u64::try_from(at / 1_000_000).unwrap_or(u64::MAX);
                    let mut shot = json!({"name": name, "takenAt": taken});
                    if let Some((width, height)) = size {
                        shot["width"] = json!(width);
                        shot["height"] = json!(height);
                    }
                    shot
                })
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();

        // The folder they're in, so the app can show one in the Finder.
        let folder = self.client().output_dir().ok().map(Path::to_path_buf);
        Ok(json!({"ok": true, "screenshots": shots, "folder": folder}))
    }

    /// One screenshot an agent took, as `query` names it: its thumbnail as
    /// a JPEG, or with `size=full` the PNG as taken. Any other name, a link
    /// or anything but a plain file is not found.
    async fn screenshot(&self, query: Option<&str>) -> Response {
        let refused = || {
            json_response(
                404,
                json!(Error::new("bad-request", "No screenshot has that name")),
            )
        };

        let (mut name, mut size) = (None, None);
        for pair in query.unwrap_or_default().split('&') {
            match pair.split_once('=') {
                Some(("name", given)) => name = Some(given),
                Some(("size", given)) => size = Some(given),
                _ => {}
            }
        }

        let full = match size {
            None | Some("thumb") => false,
            Some("full") => true,
            Some(_) => return refused(),
        };
        let Some(name) = name
            .filter(|name| helper_info::capture_time(name).is_some())
            .map(str::to_owned)
        else {
            return refused();
        };

        let client = self.client();
        let read = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, &'static str)> {
            let png = helper_info::read_capture(client.output_dir()?, &name)?;
            if full {
                return Ok((png, "image/png"));
            }
            let (thumbnail, _, _) = helper_info::jpeg(&png, helper_info::THUMB)?;
            Ok((thumbnail, "image/jpeg"))
        })
        .await;

        match read {
            Ok(Ok((bytes, mime))) => response(200, mime, Body::from(bytes), &[]),
            Ok(Err(error)) if error.code == "bad-request" => json_response(404, json!(error)),
            Ok(Err(error)) => json_response(500, json!(error)),
            Err(_) => json_response(
                500,
                json!(Error::new("capture", "The screenshot could not be read")),
            ),
        }
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
    async fn recover(self: &Arc<Self>) -> Result<Value> {
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

    /// Take, renew or end control, or send input, as `route` asks.
    async fn control(&self, route: &str, body: &Value) -> Result<Value> {
        let owner = body["lease"].as_str().unwrap_or_default();

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
            let client = self.client();
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

            let token = self.take_control(&client, &frame.epoch).await?;
            return Ok(json!({"ok": true, "lease": token, "ttlSeconds": LEASE_SECONDS}));
        }

        if self.lease_epoch.lock().await.as_ref() != Some(&frame.epoch) {
            return Err(Error::new(
                "session-changed",
                "The controlled session changed",
            ));
        }

        // Renewing and input go to the helper control was taken on.
        if route == "control/renew" {
            let client = {
                let mut lease = self.lease.lock().await;
                lease.renew(owner)?;
                self.holder()
            };

            client.native_control("renew", owner).await?;
            return Ok(json!({"ok": true}));
        }

        let _permit = ActionPermit::begin(self.lease.clone(), owner).await?;
        let client = self.holder();
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

/// End `owner`'s native control in `client`'s helper. The outcome is
/// marked pending first and cleared only once the helper confirms, so an
/// unconfirmed release stays an unknown outcome for recovery.
///
/// The helper also settles it when it says for certain that `owner` holds
/// no control to end (lease-lost): it was never granted, or another
/// controller took over since. An acquire still under way is ended by the
/// one that asked for it once it is answered. After an acquire whose answer
/// is unknown (`unknown_acquire`), that acquire may still be granted later,
/// so only a confirmed release settles it.
async fn release_native(client: &Client, owner: &str, unknown_acquire: bool) -> Result<()> {
    let pending = client.controller.quarantine();
    let result = client.native_control("release", owner).await;

    let nothing_held =
        |error: &Error| !unknown_acquire && error.code == "lease-lost" && !error.uncertain;

    match (pending, result) {
        (Ok(generation), Ok(_)) => client.controller.clear_if(&generation),
        (Ok(generation), Err(error)) if nothing_held(&error) => {
            client.controller.clear_if(&generation)
        }
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

/// Whether two settings name the same helper: everything but where output
/// goes.
fn same_helper(one: &Config, other: &Config) -> bool {
    let moved = Config {
        output_dir: other.output_dir.clone(),
        ..one.clone()
    };

    moved == *other
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
        lease_client: std::sync::Mutex::new(None),
        closed: RwLock::new(false),
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
        icons: helper_info::Icons::default(),
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

    // Requests still open finish, within a few seconds. Changes in the
    // helper are not cut here: shutdown waits for those itself.
    let _ = tokio::time::timeout(
        Duration::from_secs(5),
        open.acquire_many(u32::try_from(connections).unwrap_or(u32::MAX)),
    )
    .await;
}

/// Answer requests on `stream` in a task of its own, which holds `permit`
/// until the connection closes. A connection that has not asked for
/// anything at the viewer's private address within [`FIRST_REQUEST`]
/// closes then; one that has stays open between requests for the app.
/// Once `closed` changes, the request in progress finishes and the
/// connection closes.
fn serve_connection(
    http: &hyper::server::conn::http1::Builder,
    stream: TcpStream,
    permit: OwnedSemaphorePermit,
    viewer: &Arc<Viewer>,
    mut closed: watch::Receiver<()>,
) {
    let (named, mut naming) = watch::channel(false);
    let service = hyper::service::service_fn({
        let viewer = viewer.clone();
        move |request: hyper::Request<hyper::body::Incoming>| {
            if request.uri().path().starts_with(&viewer.prefix) {
                named.send_replace(true);
            }

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

        let first = async move {
            tokio::time::timeout(FIRST_REQUEST, naming.wait_for(|named| *named))
                .await
                .is_ok_and(|named| named.is_ok())
        };

        // Whether it named the address in time; None when closing first.
        let named = tokio::select! {
            _ = served.as_mut() => return,
            _ = closed.changed() => None,
            named = first => Some(named),
        };

        match named {
            Some(false) => return,
            Some(true) => {
                tokio::select! {
                    _ = served.as_mut() => return,
                    _ = closed.changed() => {}
                }
            }
            None => {}
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

            // Ending expired control is a change like any other; once
            // shutdown began, its own release ends control.
            let Some(_changing) = timer_view.changing().await else {
                break;
            };

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

    // Changes in progress in the helper (taking control, input, recovery)
    // end first, however long the helper takes, and no more begin; then
    // control ends. Only then are open connections given a few seconds.
    let shutdown_view = viewer.clone();
    accept(listener, viewer.clone(), CONNECTIONS, async move {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminated.recv() => {}
            _ = hung_up.recv() => {}
            () = app.exited() => {}
        }

        shutdown_view.close().await;
        let _ = shutdown_view.release(None).await;
    })
    .await;

    expiry.abort();
    viewer.close().await;
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

/// The local viewer could not be reached, or its reply not read.
fn unanswered() -> Error {
    Error::new("menubar", "The menu bar's viewer did not answer")
}

/// Run `work`, which waits on the local viewer's socket (for seconds at
/// most), off the async runtime.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| unanswered())?
}

/// The viewer recorded at `record`, if it serves `port` and answers there
/// now as the very process the record names. A viewer from an older build
/// does not say its process; the record's counts then, when that process
/// runs Atlas. It blocks for up to a second: call it off the async runtime.
pub fn running(record: &Path, port: u16) -> Option<Value> {
    std::fs::read(record)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(|running| {
            let url = running["url"].as_str().unwrap_or("");
            let recorded = running["pid"]
                .as_u64()
                .and_then(|pid| u32::try_from(pid).ok());
            if !url.starts_with(&format!("http://127.0.0.1:{port}/")) {
                return false;
            }

            match answering(url) {
                Some(Some(pid)) => recorded == Some(pid),
                Some(None) => recorded
                    .and_then(|pid| i32::try_from(pid).ok())
                    .is_some_and(crate::menubar::is_atlas),
                None => false,
            }
        })
}

/// Whether a viewer answers at `url` (its private address) right now, and
/// then the process it says serves it: None from a viewer built before
/// viewers said so.
fn answering(url: &str) -> Option<Option<u32>> {
    let (status, body) = exchange(url, "GET", "", None, Duration::from_secs(1)).ok()?;
    let reply: Value = serde_json::from_slice(&body).ok()?;
    if status != 200 || reply["service"] != SERVICE {
        return None;
    }

    Some(
        reply["pid"]
            .as_u64()
            .and_then(|pid| u32::try_from(pid).ok()),
    )
}

/// Whether the menu bar's viewer is recorded in `output` as serving `port`
/// and its process still runs Atlas, answering or not.
pub fn recorded(output: &Path, port: u16) -> bool {
    std::fs::read(output.join(MENUBAR_RECORD))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(|record| {
            record["url"]
                .as_str()
                .is_some_and(|url| url.starts_with(&format!("http://127.0.0.1:{port}/")))
        })
        .and_then(|record| record["pid"].as_i64())
        .and_then(|pid| i32::try_from(pid).ok())
        .is_some_and(crate::menubar::is_atlas)
}

/// The private address of the menu bar's viewer, when it runs and answers
/// as the process its record names. It blocks for up to a second: call it
/// off the async runtime.
pub fn local(output: &Path, port: u16) -> Option<String> {
    running(&output.join(MENUBAR_RECORD), port)
        .and_then(|running| running["url"].as_str().map(str::to_owned))
}

/// What the local viewer at `url` serves at `route`, read within `deadline`
/// and at most `limit` bytes. Anything but a 200 is refused.
pub fn fetch(url: &str, route: &str, deadline: Duration, limit: usize) -> Result<Vec<u8>> {
    match exchange_within(url, "GET", route, None, deadline, limit)? {
        (200, body) => Ok(body),
        _ => Err(unanswered()),
    }
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
    exchange_within(url, method, route, body, deadline, 1 << 16)
}

/// [`exchange`], refusing a reply larger than `limit` bytes.
fn exchange_within(
    url: &str,
    method: &str,
    route: &str,
    body: Option<&[u8]>,
    deadline: Duration,
    limit: usize,
) -> Result<(u16, Vec<u8>)> {
    let reply = exchange_raw(url, method, route, body, deadline, limit)?;
    parse_response(&reply).ok_or_else(unanswered)
}

/// The menu bar viewer's newest frame, with the key control requests name
/// it by and the display's size in points, which input is given in.
pub(crate) struct SharedFrame {
    pub png: Vec<u8>,
    pub key: String,
    pub display: (f64, f64),
}

/// The local viewer's newest frame at `url`, read within `deadline` and at
/// most `limit` bytes.
pub(crate) fn fetch_frame(url: &str, deadline: Duration, limit: usize) -> Result<SharedFrame> {
    let reply = exchange_raw(url, "GET", "frame.png", None, deadline, limit)?;
    let Some((200, png)) = parse_response(&reply) else {
        return Err(unanswered());
    };
    let named = |name: &str| reply_header(&reply, name);
    let points = |name: &str| named(name).and_then(|value| value.parse::<f64>().ok());

    Ok(SharedFrame {
        png,
        key: named("x-frame-id").ok_or_else(unanswered)?,
        display: (
            points("x-display-width").ok_or_else(unanswered)?,
            points("x-display-height").ok_or_else(unanswered)?,
        ),
    })
}

/// The value of the header `name` in a whole reply.
fn reply_header(reply: &[u8], name: &str) -> Option<String> {
    let split = reply.windows(4).position(|window| window == b"\r\n\r\n")?;
    std::str::from_utf8(&reply[..split])
        .ok()?
        .split("\r\n")
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .find(|(header, _)| header.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.trim().to_owned())
}

/// One request to the local viewer at `url`: its whole reply, head and
/// body, once all of it has arrived.
fn exchange_raw(
    url: &str,
    method: &str,
    route: &str,
    body: Option<&[u8]>,
    deadline: Duration,
    limit: usize,
) -> Result<Vec<u8>> {
    use std::io::{Read, Write};

    // Any trouble on the wire reads the same to the person.
    let io = |_: std::io::Error| unanswered();

    let rest = url.strip_prefix("http://").ok_or_else(unanswered)?;
    let (authority, path) = rest.split_at(rest.find('/').ok_or_else(unanswered)?);
    let address: std::net::SocketAddr = authority.parse().map_err(|_| unanswered())?;

    let until = std::time::Instant::now() + deadline;
    let left = || {
        until
            .checked_duration_since(std::time::Instant::now())
            .filter(|left| !left.is_zero())
            .ok_or_else(unanswered)
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

        if reply.len() > limit {
            return Err(unanswered());
        }

        if parse_response(&reply).is_some() {
            return Ok(reply);
        }
    }

    Ok(reply)
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
/// when it is not running. It prints nothing.
pub async fn open_viewer(config: &Path, port: u16, output: &Path) -> Result<()> {
    let (_, record) = crate::menubar::launch(config, port, output).await?;
    let url = record["url"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::new("menubar", "The menu bar's record names no viewer"))?;
    blocking(move || post_local(&url, "app/viewer", &json!({}))).await?;
    Ok(())
}

/// [`open_viewer`], saying so.
pub async fn monitor(config: &Path, port: u16, output: &Path) -> Result<()> {
    open_viewer(config, port, output).await?;

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
pub(crate) fn post_local(url: &str, route: &str, body: &Value) -> Result<Value> {
    let (status, reply) = exchange(
        url,
        "POST",
        route,
        Some(&serde_json::to_vec(body)?),
        Duration::from_secs(5),
    )?;
    let reply: Value = serde_json::from_slice(&reply).map_err(|_| unanswered())?;

    if status != 200 {
        // A refusal keeps its own code (busy, stale-frame…) when it is one
        // Atlas knows.
        let code = reply["code"]
            .as_str()
            .and_then(crate::error::code)
            .unwrap_or("menubar");
        return Err(Error::new(
            code,
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

/// The header `key` in `headers`, when it is there as text.
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

/// Answer one request: only from the viewer's own origin and private
/// address, within a free slot, and for a known route.
async fn respond(viewer: Arc<Viewer>, request: Request) -> Response {
    let method = request.method().clone();
    let origin = format!("http://{}", viewer.authority);
    if header(request.headers(), "host") != Some(&viewer.authority)
        || (method == Method::POST && header(request.headers(), "origin") != Some(&origin))
        || header(request.headers(), "origin").is_some_and(|given| given != origin)
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
    let urgent = method == Method::POST
        && matches!(
            route.as_str(),
            "control/release" | "control/revoke" | "control/hold"
        );
    let slots = if urgent {
        &viewer.urgent
    } else {
        &viewer.requests
    };
    let Ok(_slot) = slots.try_acquire() else {
        return json_response(
            503,
            json!({"ok": false, "code": "busy", "error": "Viewer is busy"}),
        );
    };

    if method == Method::GET {
        if route == "status.json" {
            return json_response(200, viewer.status().await);
        }

        if route == "activity.json" {
            return match viewer.activity().await {
                Ok(activity) => json_response(200, activity),
                Err(error) => json_response(503, json!(error)),
            };
        }

        if route == "apps.json" {
            return match viewer.apps().await {
                Ok(apps) => json_response(200, apps),
                Err(error) => json_response(503, json!(error)),
            };
        }

        if route == "screenshots.json" {
            return match viewer.screenshots().await {
                Ok(shots) => json_response(200, shots),
                Err(error) => json_response(503, json!(error)),
            };
        }

        if route == "screenshot.png" {
            return viewer.screenshot(request.uri().query()).await;
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
                | "control/hold"
                | "recover"
                | "app/viewer"
        )
    {
        return json_response(404, json!({"ok": false, "error": "Unknown operation"}));
    }

    if request.headers().contains_key("transfer-encoding")
        || header(request.headers(), "content-type").and_then(|kind| kind.split(';').next())
            != Some("application/json")
    {
        return json_response(
            400,
            json!({"ok": false, "error": "Send JSON with Content-Length"}),
        );
    }

    let length = header(request.headers(), "content-length")
        .and_then(|given| given.parse::<usize>().ok())
        .filter(|length| (1..=MAX_BODY).contains(length));
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

    // Held to the end of the request, cleanup included, so shutdown waits
    // for it.
    let Some(_changing) = viewer.changing().await else {
        return json_response(503, json!({"ok": false, "error": "The viewer is closing"}));
    };

    let outcome = if route == "recover" {
        viewer.recover().await
    } else if route == "control/hold" {
        // The person's stop for agents, or their resume. Control a viewer
        // holds is left alone either way.
        viewer.client().hold(body["on"] == true).await
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

/// A logged row as the app's activity lists it: what was done in words, by
/// whom, how long ago, whether it worked and how many actions it sent.
/// Only these fields go, through [`crate::overview::shown`]: never typed
/// text, argv, coordinates or titles, which the log never keeps either.
fn activity_row(row: &Value, now: u128) -> Value {
    let shown = crate::overview::shown(row, now);
    json!({
        "id": crate::overview::row_id(row),
        // When it started, in milliseconds since 1970, so its time never
        // moves between readings.
        "startedAt": row["at"]
            .as_str()
            .and_then(|at| at.parse::<u128>().ok())
            .and_then(|at| u64::try_from(at / 1_000_000).ok()),
        "says": shown["says"],
        "ok": shown["ok"],
        "started": shown["started"],
        "actor": shown["actor"],
        "ageSeconds": shown["ageSeconds"],
        "reason": shown["reason"],
        "operation": shown["operation"],
        "count": shown["actionCount"],
    })
}

/// The helper's reply when it answered with what was asked; a refusal is an
/// answer too, and never reads as an empty list.
fn answered(reply: Value) -> Result<Value> {
    if reply["ok"] == true {
        Ok(reply)
    } else {
        Err(crate::transport::refusal(&reply))
    }
}

/// The helper's regular apps as the app lists them, front to back, each
/// with its windows front to back. A window's id is its window number.
fn listed_apps(apps: &Value, windows: &Value) -> Result<Vec<Value>> {
    Ok(helper_info::open_apps(apps, windows)?
        .into_iter()
        .map(|app| {
            json!({
                "pid": app["pid"],
                "name": app["name"],
                "bundle": app["bundle"],
                "front": app["active"],
                "windows": app["windows"],
            })
        })
        .collect())
}

/// A reply of `value` as JSON, with status `code`.
fn json_response(code: u16, value: Value) -> Response {
    response(code, "application/json", Body::from(value.to_string()), &[])
}

/// A reply of `body` as `mime`, with status `code`, the headers every
/// reply carries (not cached, not shown inside other pages, nothing
/// loaded from elsewhere) and then `headers`.
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

    /// A viewer for a helper that is not there, not answering requests.
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

    /// `path` asked of `viewer` with `method` at `host`, from `origin` when
    /// one is given: the reply's status, type and body.
    async fn asked(
        viewer: &Arc<Viewer>,
        method: Method,
        path: &str,
        host: &str,
        origin: Option<&str>,
    ) -> (u16, String, Vec<u8>) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("host", host);
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }

        let reply = respond(viewer.clone(), request.body(Body::empty()).unwrap()).await;
        let status = reply.status().as_u16();
        let kind = header(reply.headers(), "content-type")
            .unwrap_or_default()
            .to_owned();
        let body = to_bytes(reply.into_body(), usize::MAX).await.unwrap();
        (status, kind, body.to_vec())
    }

    /// `route` read from `viewer` as its app reads it: at its private
    /// address, from its own origin.
    async fn read(viewer: &Arc<Viewer>, route: &str) -> (u16, String, Vec<u8>) {
        let path = format!("{}{route}", viewer.prefix);
        let origin = format!("http://{}", viewer.authority);
        asked(viewer, Method::GET, &path, &viewer.authority, Some(&origin)).await
    }

    /// `route` read from `viewer` as JSON, with the reply's status.
    async fn read_json(viewer: &Arc<Viewer>, route: &str) -> (u16, Value) {
        let (status, kind, body) = read(viewer, route).await;
        assert_eq!(kind, "application/json", "{route}");
        (status, serde_json::from_slice(&body).unwrap())
    }

    /// [`answering`] off the runtime.
    async fn answering_off(url: &str) -> Option<Option<u32>> {
        let url = url.to_owned();
        tokio::task::spawn_blocking(move || answering(&url))
            .await
            .unwrap()
    }

    #[test]
    fn a_frames_key_and_display_size_are_read_from_its_headers() {
        let reply = b"HTTP/1.1 200 OK\r\nX-Frame-Id: abc123\r\nx-display-width: 1440\r\nContent-Length: 2\r\n\r\nok";
        assert_eq!(reply_header(reply, "x-frame-id").as_deref(), Some("abc123"));
        assert_eq!(
            reply_header(reply, "X-Display-Width").as_deref(),
            Some("1440"),
            "named in any case"
        );
        assert_eq!(reply_header(reply, "x-display-height"), None);
        assert_eq!(
            reply_header(b"HTTP/1.1 200 OK\r\nX-Frame-Id: a", "x-frame-id"),
            None,
            "not until the head has ended"
        );
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
    async fn a_fetch_reads_the_viewers_status_within_its_limit_and_refuses_more() {
        let dir = tempfile::tempdir().unwrap();
        let (viewer, server) = served(dir.path(), CONNECTIONS).await;
        let url = viewer.url.clone();

        let (status, small) = tokio::task::spawn_blocking(move || {
            let deadline = Duration::from_secs(8);
            (
                fetch(&url, "status.json", deadline, 1 << 16),
                fetch(&url, "status.json", deadline, 16),
            )
        })
        .await
        .unwrap();

        let status: Value = serde_json::from_slice(&status.unwrap()).unwrap();
        assert!(
            status["overview"]["state"].is_string(),
            "watch reads the state"
        );
        assert_eq!(small.unwrap_err().code, "menubar");
        server.abort();
    }

    #[tokio::test]
    async fn the_person_stops_and_resumes_agents_through_the_viewer() {
        let dir = tempfile::tempdir().unwrap();
        let (viewer, server) = served(dir.path(), CONNECTIONS).await;
        let url = viewer.url.clone();

        // No input service runs here: the Stop is kept on the person's side
        // all the same, and says the service did not hear it.
        let replies = tokio::task::spawn_blocking(move || {
            [true, false].map(|on| post_local(&url, "control/hold", &json!({"on": on})))
        })
        .await
        .unwrap();

        for (reply, on) in replies.into_iter().zip([true, false]) {
            let reply = reply.unwrap();
            assert_eq!(reply["held"], on);
            assert!(
                reply["warnings"][0]
                    .as_str()
                    .unwrap()
                    .contains("isn't running")
            );
        }
        server.abort();
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
        assert_eq!(answering_off(&url).await, Some(Some(std::process::id())));

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
        let error = release_native(&viewer.client(), "lost-lease", false)
            .await
            .unwrap_err();
        assert!(error.uncertain);
        assert_eq!(error.code, "uncertain");
        assert_eq!(controller.unresolved().unwrap().len(), 1);
    }

    /// A release the helper refuses for certain, because the owner holds no
    /// control, settles; after an acquire whose answer is unknown it does
    /// not, since that acquire may still be granted.
    #[tokio::test]
    async fn releasing_control_never_granted_settles_unless_an_acquire_is_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(dir.path()).unwrap();
        let socket = base.join("a.sock");
        let (mut asked, _never) = input_service(&socket, "none");
        let client = client_with_input(&base, &socket);
        let owner = control::token();

        release_native(&client, &owner, false).await.unwrap();
        assert_eq!(asked.recv().await.as_deref(), Some("release"));
        assert!(client.controller.pending().unwrap().is_empty());

        let error = release_native(&client, &owner, true).await.unwrap_err();
        assert_eq!(asked.recv().await.as_deref(), Some("release"));
        assert!(error.uncertain);
        assert_eq!(client.controller.unresolved().unwrap().len(), 1);
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

        assert_eq!(answering_off(&url).await, Some(Some(std::process::id())));

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
        assert_eq!(
            answering_off(&viewer.url).await,
            Some(Some(std::process::id()))
        );

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

    /// A new output folder alone keeps the helper and control with it, and
    /// moves the record. Once the settings name another helper, the
    /// running viewer serves it: control held in the previous one ends
    /// (unconfirmed, it stays pending there), and the record moves again.
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

        std::fs::write(&settings, SETTINGS.replace("\"out\"", "\"moved\"")).unwrap();
        viewer.follow_settings().await;

        let moved = base.join("moved").join(MENUBAR_RECORD);
        assert!(viewer.lease.lock().await.active(), "control carries on");
        assert!(controller.pending().unwrap().is_empty());
        assert!(moved.exists());
        assert!(!record.exists());

        std::fs::write(
            &settings,
            SETTINGS
                .replace("60001", "60002")
                .replace("\"out\"", "\"elsewhere\""),
        )
        .unwrap();
        viewer.follow_settings().await;

        assert_eq!(viewer.client().config.helper_uid, 60002);
        assert!(!viewer.lease.lock().await.active());
        assert_eq!(controller.unresolved().unwrap().len(), 1);

        let elsewhere = base.join("elsewhere").join(MENUBAR_RECORD);
        assert!(elsewhere.exists());
        assert!(!moved.exists());
        assert_eq!(viewer.record.lock().unwrap().as_ref().unwrap().0, elsewhere);
    }

    /// A stand-in for the helper's input service at `socket`. It reports
    /// each request's operation, and answers it at once, except `held`,
    /// which waits for the returned gate to open (once per request). Like
    /// the real gate, it grants an acquire when answering it, and refuses
    /// for certain to release an owner it never granted.
    fn input_service(
        socket: &Path,
        held: &'static str,
    ) -> (
        tokio::sync::mpsc::UnboundedReceiver<String>,
        Arc<tokio::sync::Notify>,
    ) {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        let listener = tokio::net::UnixListener::bind(socket).unwrap();
        let (asked, operations) = tokio::sync::mpsc::unbounded_channel();
        let gate = Arc::new(tokio::sync::Notify::new());
        let opens = gate.clone();
        let granted = Arc::new(std::sync::Mutex::new(None::<String>));

        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let asked = asked.clone();
                let opens = opens.clone();
                let granted = granted.clone();

                tokio::spawn(async move {
                    let mut stream = BufReader::new(stream);
                    let mut line = String::new();
                    stream.read_line(&mut line).await.unwrap();

                    let request: Value = serde_json::from_str(&line).unwrap();
                    let operation = request["op"].as_str().unwrap().to_owned();
                    let _ = asked.send(operation.clone());

                    if operation == held {
                        opens.notified().await;
                    }

                    let owner = request["owner"].as_str().map(str::to_owned);
                    let reply = {
                        let mut granted = granted.lock().unwrap();
                        match operation.as_str() {
                            "acquire" => {
                                *granted = owner;
                                json!({"ok": true})
                            }
                            "release" if *granted != owner => json!({
                                "ok": false,
                                "code": "lease-lost",
                                "error": "This token cannot release another controller",
                                "uncertain": false,
                            }),
                            _ => json!({"ok": true}),
                        }
                    };

                    let line = format!("{reply}\n");
                    let _ = stream.get_mut().write_all(line.as_bytes()).await;
                });
            }
        });

        (operations, gate)
    }

    /// A client whose input service is the stand-in at `socket`.
    fn client_with_input(dir: &Path, socket: &Path) -> Client {
        let mut client = client(dir);
        client.config.input_socket = socket.to_owned();
        client.config.helper_uid = nix::unistd::getuid().as_raw();
        client
    }

    /// A session for input that a stand-in service accepts as is.
    fn epoch() -> Epoch {
        Epoch {
            uid: 0,
            capture_pid: 0,
            display: crate::session::Display {
                width: 1.0,
                height: 1.0,
                scale: 1.0,
            },
            security_session_id: 0,
            display_id: 0,
            input_pid: 0,
        }
    }

    /// Control ends in the helper it was taken on, though the viewer
    /// serves another one now.
    #[tokio::test]
    async fn ending_control_reaches_the_helper_it_was_taken_on() {
        let dir = tempfile::tempdir().unwrap();
        let viewer = unreachable(dir.path()).await;
        let taken = viewer.client();
        viewer
            .lease
            .lock()
            .await
            .acquire(&taken.controller)
            .unwrap();
        *viewer.lease_client.lock().unwrap() = Some(taken.clone());

        let mut other = client(dir.path());
        other.controller = control::Controller::at(60001, &dir.path().join("other"));
        *viewer.client.lock().unwrap() = Arc::new(other);

        let error = viewer.release(None).await.unwrap_err();
        assert!(error.uncertain);
        assert_eq!(taken.controller.unresolved().unwrap().len(), 1);
        assert!(viewer.client().controller.pending().unwrap().is_empty());
        assert!(viewer.lease_client.lock().unwrap().is_none());
    }

    /// While the helper has not answered an acquire, control stays held:
    /// the settings naming another helper end it, but only in the helper it
    /// was asked of, and the viewer moves only once the acquire settled. A
    /// grant that comes after the lease ended is undone there too.
    #[tokio::test]
    async fn control_being_taken_keeps_the_viewer_on_its_helper() {
        let dir = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(dir.path()).unwrap();
        let socket = base.join("a.sock");
        let (mut asked, answer) = input_service(&socket, "acquire");
        let settings = base.join("u.toml");
        let (_, viewer) = bind(client_with_input(&base, &socket), 0, Some(&settings))
            .await
            .unwrap();
        let taken = viewer.client();

        let taking = tokio::spawn({
            let viewer = viewer.clone();
            let taken = taken.clone();
            async move { viewer.take_control(&taken, &epoch()).await }
        });
        assert_eq!(asked.recv().await.as_deref(), Some("acquire"));

        std::fs::write(&settings, SETTINGS.replace("60001", "60002")).unwrap();
        viewer.follow_settings().await;
        assert_eq!(asked.recv().await.as_deref(), Some("release"));
        assert!(Arc::ptr_eq(&viewer.client(), &taken), "it waits");
        assert!(viewer.lease.lock().await.active());

        answer.notify_one();
        let lost = taking.await.unwrap().unwrap_err();
        assert_eq!(lost.code, "lease-lost");
        assert_eq!(asked.recv().await.as_deref(), Some("release"), "undone");
        assert!(taken.controller.pending().unwrap().is_empty());

        viewer.follow_settings().await;
        assert_eq!(viewer.client().config.helper_uid, 60002);
        assert!(asked.try_recv().is_err(), "nothing more asked of it");
    }

    /// Ending control in a helper that does not answer never holds up the
    /// status read when the settings name another helper.
    #[tokio::test]
    async fn following_the_settings_never_holds_up_the_status() {
        let dir = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(dir.path()).unwrap();
        let socket = base.join("a.sock");
        let (mut asked, _never) = input_service(&socket, "release");
        let settings = base.join("u.toml");
        let (_, viewer) = bind(client_with_input(&base, &socket), 0, Some(&settings))
            .await
            .unwrap();

        viewer
            .take_control(&viewer.client(), &epoch())
            .await
            .unwrap();
        assert_eq!(asked.recv().await.as_deref(), Some("acquire"));
        std::fs::write(&settings, SETTINGS.replace("60001", "60002")).unwrap();

        let started = std::time::Instant::now();
        let status = viewer.status().await;
        assert!(started.elapsed() < Duration::from_secs(3));
        assert_ne!(status["ready"], true);
        assert_eq!(asked.recv().await.as_deref(), Some("release"));
    }

    /// A frame request that waits out a capture from before the latest
    /// input ends within the same deadline as any other.
    #[tokio::test]
    async fn a_frame_waiting_out_an_older_capture_still_ends_in_time() {
        let dir = tempfile::tempdir().unwrap();
        let viewer = unreachable(dir.path()).await;
        let (_never, outcome) = watch::channel(None);
        *viewer.capture.lock().unwrap() = Some(Capture {
            revision: 0,
            outcome,
        });
        viewer.frames.lock().await.input_revision = 1;

        let started = std::time::Instant::now();
        let error = viewer.frame().await.err().unwrap();
        assert!(started.elapsed() < READ_DEADLINE + Duration::from_secs(1));
        assert_eq!(error.error, "The helper did not answer in time");
    }

    /// A viewer from an older build answers without its process: the one
    /// its record names counts when that runs Atlas.
    #[tokio::test]
    async fn an_older_viewer_is_found_by_its_record() {
        let dir = tempfile::tempdir().unwrap();
        let older = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = older.local_addr().unwrap();
        std::thread::spawn(move || {
            use std::io::{Read, Write};

            let reply = "{\"ok\":true,\"service\":\"atlas-viewer\"}";
            for mut stream in older.incoming().flatten() {
                let _ = stream.read(&mut [0u8; 1024]);
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n{reply}",
                        reply.len()
                    )
                    .as_bytes(),
                );
            }
        });

        let record = dir.path().join(MENUBAR_RECORD);
        let url = format!("http://{address}/secret/");
        let check = |pid: u32| {
            std::fs::write(&record, json!({"url": url, "pid": pid}).to_string()).unwrap();
            let record = record.clone();
            tokio::task::spawn_blocking(move || running(&record, address.port()))
        };

        assert_eq!(answering_off(&url).await, Some(None));
        assert!(check(std::process::id()).await.unwrap().is_some());
        assert!(check(1).await.unwrap().is_none(), "not Atlas");
    }

    /// Shutdown waits for control being taken to settle, however long the
    /// helper takes, and then lets no more changes begin.
    #[tokio::test]
    async fn shutdown_waits_for_changes_in_progress() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("a.sock");
        let (mut asked, answer) = input_service(&socket, "acquire");
        let (listener, viewer) = bind(client_with_input(dir.path(), &socket), 0, None)
            .await
            .unwrap();
        let server = tokio::spawn(accept(
            listener,
            viewer.clone(),
            CONNECTIONS,
            std::future::pending(),
        ));

        let taking = tokio::spawn({
            let viewer = viewer.clone();
            async move {
                let _changing = viewer.changing().await;
                viewer.take_control(&viewer.client(), &epoch()).await
            }
        });
        assert_eq!(asked.recv().await.as_deref(), Some("acquire"));

        let closing = tokio::spawn({
            let viewer = viewer.clone();
            async move { viewer.close().await }
        });
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!closing.is_finished(), "it waits for the acquire");

        answer.notify_one();
        taking.await.unwrap().unwrap();
        closing.await.unwrap();
        viewer.release(None).await.unwrap();
        assert_eq!(asked.recv().await.as_deref(), Some("release"));
        assert!(viewer.client().controller.pending().unwrap().is_empty());

        let url = viewer.url.clone();
        let refused =
            tokio::task::spawn_blocking(move || post_local(&url, "control/revoke", &json!({})))
                .await
                .unwrap()
                .unwrap_err();
        assert_eq!(refused.error, "The viewer is closing");

        server.abort();
    }

    /// A connection that never asks for anything at the viewer's private
    /// address closes within moments; one that did stays open between
    /// requests.
    #[tokio::test]
    async fn connections_that_never_name_the_viewer_close_soon() {
        let dir = tempfile::tempdir().unwrap();
        let (viewer, server) = served(dir.path(), CONNECTIONS).await;
        let address: std::net::SocketAddr = viewer.authority.parse().unwrap();
        let authority = viewer.authority.clone();
        let prefix = viewer.prefix.clone();

        let outcome = tokio::task::spawn_blocking(move || {
            use std::io::{Read, Write};

            let open = |ask: Option<&str>| {
                let mut stream = std::net::TcpStream::connect(address).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                if let Some(path) = ask {
                    let request = format!("GET {path} HTTP/1.1\r\nHost: {authority}\r\n\r\n");
                    stream.write_all(request.as_bytes()).unwrap();
                }
                stream
            };

            let reply = |stream: &mut std::net::TcpStream| {
                let mut reply = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let read = stream.read(&mut chunk).unwrap();
                    assert_ne!(read, 0, "closed before replying");
                    reply.extend_from_slice(&chunk[..read]);
                    if let Some(parsed) = parse_response(&reply) {
                        return parsed.0;
                    }
                }
            };

            let started = std::time::Instant::now();
            let mut idle = open(None);
            let mut elsewhere = open(Some("/elsewhere"));
            let mut named = open(Some(&prefix));
            assert_eq!(reply(&mut named), 200);
            assert_eq!(reply(&mut elsewhere), 404);

            for stream in [&mut idle, &mut elsewhere] {
                let _ = stream.read_to_end(&mut Vec::new());
                assert!(started.elapsed() < FIRST_REQUEST + Duration::from_secs(2));
            }

            std::thread::sleep(FIRST_REQUEST + Duration::from_millis(500) - started.elapsed());
            let request = format!("GET {prefix} HTTP/1.1\r\nHost: {authority}\r\n\r\n");
            named.write_all(request.as_bytes()).unwrap();
            reply(&mut named)
        })
        .await
        .unwrap();
        assert_eq!(outcome, 200, "still open");

        server.abort();
    }

    /// The helper's activity, apps and screenshots answer only at the
    /// viewer's private address, from its own origin, within a free slot.
    #[tokio::test]
    async fn the_helpers_activity_apps_and_screenshots_answer_only_the_viewers_own_origin() {
        let dir = tempfile::tempdir().unwrap();
        let viewer = unreachable(dir.path()).await;
        let routes = [
            "activity.json",
            "apps.json",
            "screenshots.json",
            "screenshot.png?name=1-capture.png",
        ];
        let own = format!("http://{}", viewer.authority);

        for route in routes {
            let path = format!("{}{route}", viewer.prefix);
            let elsewhere = Some("http://evil.example");
            let (status, ..) =
                asked(&viewer, Method::GET, &path, &viewer.authority, elsewhere).await;
            assert_eq!(status, 403, "{route} from another origin");
            let (status, ..) = asked(&viewer, Method::GET, &path, "localhost", None).await;
            assert_eq!(status, 403, "{route} at another host");

            let bare = format!("/{route}");
            let (status, _, body) =
                asked(&viewer, Method::GET, &bare, &viewer.authority, None).await;
            assert_eq!(status, 404, "{route} without the private address");
            assert!(String::from_utf8_lossy(&body).contains("Unknown viewer address"));

            let (status, _, body) =
                asked(&viewer, Method::POST, &path, &viewer.authority, Some(&own)).await;
            assert_eq!(status, 404, "{route} only reads");
            assert!(String::from_utf8_lossy(&body).contains("Unknown operation"));
        }

        assert_eq!(read_json(&viewer, "activity.json").await.0, 200);
        assert_eq!(read_json(&viewer, "screenshots.json").await.0, 200);
        let (status, apps) = read_json(&viewer, "apps.json").await;
        assert_eq!(
            (status, apps["ok"].clone()),
            (503, json!(false)),
            "no helper"
        );
        assert_eq!(read_json(&viewer, "screenshot.jpg").await.0, 404);

        let _busy = viewer
            .requests
            .try_acquire_many(CONCURRENT_REQUESTS as u32)
            .unwrap();
        for route in routes {
            let (status, reply) = read_json(&viewer, route).await;
            assert_eq!(
                (status, reply["code"].clone()),
                (503, json!("busy")),
                "{route}"
            );
        }
    }

    #[tokio::test]
    async fn activity_json_never_carries_typed_text() {
        let dir = tempfile::tempdir().unwrap();
        let viewer = unreachable(dir.path()).await;
        let output = viewer.client().output_dir().unwrap().to_path_buf();
        let at = crate::client::timestamp() - 3_000_000_000;
        // Fields the log never keeps, written here all the same: none of
        // them may reach the app.
        let row = json!({
            "at": at.to_string(), "operation": "input", "ok": true, "actionCount": 2,
            "actor": "agent", "verb": "type", "app": "TextEdit", "command": "0123456789ab",
            "text": "hunter2", "argv": ["rm", "secret-argument"], "x": 123.5, "y": 456.25,
            "title": "Secret plans", "elapsedMs": 4.0,
        });
        std::fs::write(output.join("metadata.jsonl"), format!("{row}\n")).unwrap();

        let (status, reply) = read_json(&viewer, "activity.json").await;
        assert_eq!((status, reply["ok"].clone()), (200, json!(true)));
        let text = reply.to_string();
        for secret in [
            "hunter2",
            "secret-argument",
            "Secret plans",
            "123.5",
            "456.25",
        ] {
            assert!(!text.contains(secret), "{secret}");
        }

        let listed = &reply["activity"][0];
        let mut keys = listed
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        keys.sort();
        assert_eq!(
            keys,
            [
                "actor",
                "ageSeconds",
                "count",
                "id",
                "ok",
                "operation",
                "reason",
                "says",
                "started",
                "startedAt"
            ]
        );
        assert_eq!(listed["says"], "typed in TextEdit");
        assert_eq!(listed["count"], 2);
        assert_eq!(listed["ageSeconds"], 3);
    }

    #[tokio::test]
    async fn activity_json_lists_the_newest_200_rows_by_ids_that_hold_across_reads() {
        let dir = tempfile::tempdir().unwrap();
        let viewer = unreachable(dir.path()).await;
        let output = viewer.client().output_dir().unwrap().to_path_buf();
        let log = (0..250)
            .map(|at| {
                let row = json!({"at": format!("{}", 1_000_000 + at), "operation": "input",
                    "ok": true, "actionCount": 1, "actor": "viewer", "elapsedMs": 2.5});
                format!("{row}\n")
            })
            .collect::<String>();
        std::fs::write(output.join("metadata.jsonl"), log).unwrap();

        let first = read_json(&viewer, "activity.json").await.1;
        let rows = first["activity"].as_array().unwrap();
        assert_eq!(rows.len(), ACTIVITY_ROWS);
        let ids = rows
            .iter()
            .map(|row| row["id"].as_str().unwrap().to_owned())
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(ids.len(), ACTIVITY_ROWS, "no two rows share an id");

        let newer = json!({"at": "2000000", "operation": "input", "ok": false, "actor": "agent"});
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(output.join("metadata.jsonl"))
            .unwrap();
        std::io::Write::write_all(&mut file, format!("{newer}\n").as_bytes()).unwrap();

        let second = read_json(&viewer, "activity.json").await.1;
        assert_eq!(second["activity"][0]["ok"], false, "newest first");
        assert_eq!(
            second["activity"][1]["id"], rows[0]["id"],
            "a row keeps its id as newer ones come"
        );
        assert!(!ids.contains(second["activity"][0]["id"].as_str().unwrap()));
    }

    #[test]
    fn apps_json_lists_windows_front_to_back() {
        let apps = json!([
            {"name": "Finder", "pid": 1, "policy": "regular",
                "executable": "/System/Library/CoreServices/Finder.app/Contents/MacOS/Finder"},
            {"name": "Dock", "pid": 2, "policy": "accessory", "executable": "/x"},
            {"name": "Opie", "pid": 3, "policy": "regular", "active": true,
                "executable": "/Applications/Opie.app/Contents/MacOS/Opie"},
            {"name": "Notes", "pid": 4, "policy": "regular",
                "executable": "/System/Applications/Notes.app/Contents/MacOS/Notes"},
        ]);
        let window = |number: i64, pid: i64, title: &str| {
            json!({"number": number, "pid": pid, "title": title, "layer": 0, "alpha": 1.0,
                "bounds": {"x": 10.0, "y": 40.0, "width": 520.0, "height": 860.0}})
        };
        let windows = json!([
            window(31, 4, "QA checklist"),
            window(12, 3, "Ring"),
            window(7, 3, "Starter"),
        ]);

        let listed = listed_apps(&apps, &windows).unwrap();
        let order = listed
            .iter()
            .map(|app| app["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            order,
            ["Notes", "Opie", "Finder"],
            "by each app's front window"
        );
        assert_eq!(
            listed[1],
            json!({
                "pid": 3, "name": "Opie", "bundle": "/Applications/Opie.app", "front": true,
                "windows": [
                    {"id": 12, "title": "Ring", "x": 10.0, "y": 40.0, "width": 520.0, "height": 860.0},
                    {"id": 7, "title": "Starter", "x": 10.0, "y": 40.0, "width": 520.0, "height": 860.0},
                ],
            })
        );
        assert_eq!(listed[2]["windows"], json!([]), "Finder has none");
        assert_eq!(listed[2]["front"], false);
    }

    #[test]
    fn a_refused_app_listing_never_reads_as_no_apps() {
        let refused = answered(json!({"ok": false, "code": "busy", "error": "Busy"}));
        assert_eq!(refused.map_err(|error| error.code).unwrap_err(), "busy");
        assert!(answered(json!({"ok": true, "apps": []})).is_ok());
    }

    #[tokio::test]
    async fn reading_the_helpers_apps_and_windows_records_no_activity() {
        let dir = tempfile::tempdir().unwrap();
        let viewer = unreachable(dir.path()).await;
        assert_eq!(read_json(&viewer, "apps.json").await.0, 503, "no helper");

        let agent = client(dir.path()).doing(crate::doing::Doing::new("screenshot"));
        let filter = crate::agent::WindowFilter {
            app: None,
            title: None,
            all: false,
        };
        assert!(agent.apps().await.is_err());
        assert!(crate::agent::list(&agent, &filter).await.is_err());
        let output = agent.output_dir().unwrap().to_path_buf();
        assert!(crate::overview::activity(&output, 5).is_empty());

        // The same client records what acts, even when no helper answers.
        assert!(agent.capture(None).await.is_err());
        let rows = crate::overview::activity(&output, 5);
        assert_eq!(rows.len(), 1);
        assert!(
            crate::overview::shown(&rows[0], 0)["reason"].is_string(),
            "a failed capture says why: {}",
            rows[0]
        );
    }

    #[tokio::test]
    async fn screenshots_are_listed_newest_first_and_served_only_by_a_capture_name() {
        let dir = tempfile::tempdir().unwrap();
        let viewer = unreachable(dir.path()).await;
        let output = viewer.client().output_dir().unwrap().to_path_buf();
        let older = crate::native::capture::sample_png(1280, 800);
        let newer = crate::native::capture::sample_png(640, 400);
        std::fs::write(output.join("1700000000000000000-capture.png"), &older).unwrap();
        std::fs::write(output.join("1700000005000000000-capture.png"), &newer).unwrap();
        std::os::unix::fs::symlink(
            output.join("1700000000000000000-capture.png"),
            output.join("1700000009000000000-capture.png"),
        )
        .unwrap();
        std::fs::create_dir(output.join("1700000007000000000-capture.png")).unwrap();
        std::fs::write(dir.path().join("u.toml"), SETTINGS).unwrap();

        let (status, listed) = read_json(&viewer, "screenshots.json").await;
        assert_eq!(status, 200);
        assert_eq!(
            listed,
            json!({"ok": true, "folder": output, "screenshots": [
                {"name": "1700000005000000000-capture.png", "takenAt": 1_700_000_005_000_u64,
                    "width": 640, "height": 400},
                {"name": "1700000000000000000-capture.png", "takenAt": 1_700_000_000_000_u64,
                    "width": 1280, "height": 800},
            ]})
        );

        let (status, kind, body) = read(
            &viewer,
            "screenshot.png?name=1700000000000000000-capture.png&size=full",
        )
        .await;
        assert_eq!((status, kind.as_str()), (200, "image/png"));
        assert_eq!(body, older, "as taken");

        let (status, kind, body) = read(
            &viewer,
            "screenshot.png?name=1700000000000000000-capture.png",
        )
        .await;
        assert_eq!((status, kind.as_str()), (200, "image/jpeg"));
        assert_eq!(&body[..2], [0xFF, 0xD8], "a thumbnail");

        for query in [
            "",
            "?name=../u.toml",
            "?name=1700000009000000000-capture.png",
            "?name=1700000007000000000-capture.png",
            "?name=1800000000000000000-capture.png",
            "?name=1700000005000000000-capture.png&size=huge",
        ] {
            let (status, refused) = read_json(&viewer, &format!("screenshot.png{query}")).await;
            assert_eq!(
                (status, refused["code"].clone()),
                (404, json!("bad-request")),
                "{query}"
            );
        }
    }
}
