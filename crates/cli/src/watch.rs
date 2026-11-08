//! `atlas watch`: the helper's status as it changes, one JSON line each, for
//! the agent apps that show what an agent does in the helper. With a frames
//! folder it also keeps the helper's newest screen there, as `frame.png`
//! with `frame.json` beside it, while an agent works or the person has
//! control.
//!
//! It never sends input, takes controller ownership or records activity.
//! The menu bar's viewer answers when it runs: it knows who holds control
//! and shares one capture among everyone watching, and reading it is what
//! Atlas.app's own reading does (it may start input once the helper logs
//! in). Otherwise the helper is read directly.
use atlas_client::{
    client::Client,
    overview::{READ_DEADLINE, Reading, State},
};
use atlas_core::host::{HostSession, Key};
use atlas_core::{
    error::{Code, Error, Result},
    files::{Durability, write_private_with},
    screen::png::png_dimensions,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Stdout, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

/// How often the activity log is looked at for a change.
const TICK: Duration = Duration::from_millis(300);
/// The least time between two status reads, however fast rows come.
const SOONEST: Duration = Duration::from_secs(1);
/// How often the status is read when the log has not changed.
const READ_EVERY: Duration = Duration::from_secs(5);
/// After this long with nothing to say, a blank line shows it still runs.
const HEARTBEAT: Duration = Duration::from_secs(5);
/// How often the screen is kept while an agent works or the person has
/// control; it is not kept otherwise.
const FRAME_EVERY: Duration = Duration::from_secs(1);
/// How long `--once` waits for a status before it ends with none.
const ONCE_LIMIT: Duration = Duration::from_secs(8);
/// How often while nobody acts: the person still sees the helper as it is.
const IDLE_FRAME_EVERY: Duration = Duration::from_secs(5);
/// The widest screen captured here, in pixels.
const FRAME_WIDTH: u32 = 1280;
/// The largest reply read from the viewer: a status, and a frame.
const STATUS_LIMIT: usize = 1 << 16;
const FRAME_LIMIT: usize = 16 << 20;
/// How long a read from the viewer may take: a little longer than the
/// viewer itself waits on the helper.
const VIEWER_DEADLINE: Duration = READ_DEADLINE.saturating_add(Duration::from_secs(1));

/// The files a frames folder holds, whole or being written.
const FRAME_FILES: [&str; 4] = [
    "frame.png",
    "frame.json",
    "frame.png.partial",
    "frame.json.partial",
];

/// The activity log's length and modification time, which change with
/// each recorded row and when the log is started anew.
type Mark = Option<(u64, SystemTime)>;

/// Watch until the reader goes away or the process is asked to stop, or
/// with `once` until the first status is said; `frames`, when given, is a
/// private folder for the newest screen.
///
/// # Errors
///
/// Returns configuration, output, listener or watch-service failures. Streaming
/// observations do not authorize later input against a changed session.
pub async fn watch(client: Client, port: u16, frames: Option<PathBuf>, once: bool) -> Result<()> {
    // Only a folder this watch made is removed at the end.
    let made = frames.as_ref().is_some_and(|frames| !frames.exists());
    if let Some(frames) = &frames {
        atlas_core::files::private_folder(frames, Code::Usage)?;
        // A screen an earlier watch left behind is not this one's.
        clear(frames);
    }

    let output = client.output_dir()?.to_path_buf();
    let mut lines = Lines {
        stdout: std::io::stdout(),
        written: Instant::now(),
    };
    // The host session this watch runs for, if any: only its own host id
    // is passed on, for the pane that shows "this session".
    let own = HostSession::from_env().map(|session| session.key().clone());
    let mut watch = Watch::new(frames).for_session(own);

    let stop = stopped();
    tokio::pin!(stop);

    // A look is never cut short (each step has its own time limit), so the
    // screen it was writing is whole before it is cleared.
    let began = Instant::now();
    while watch.tick(&client, &output, port, &mut lines).await {
        // Once: the first status, or nothing once a busy viewer has had
        // its time.
        if once && (!watch.said.is_null() || began.elapsed() >= ONCE_LIMIT) {
            break;
        }
        tokio::select! {
            _ = &mut stop => break,
            _ = tokio::time::sleep(TICK) => {}
        }
    }

    // A screen left behind would outlive what it shows; the folder was
    // this watch's own.
    if let Some(frames) = &watch.frames {
        clear(frames);
        if made {
            let _ = fs::remove_dir(frames);
        }
    }
    Ok(())
}

/// Once the process is asked to stop: an interrupt, a terminate or a
/// hang-up (the app that started it closed).
pub(crate) async fn stopped() {
    use tokio::signal::unix::{SignalKind, signal};

    let (Ok(mut terminate), Ok(mut hangup)) = (
        signal(SignalKind::terminate()),
        signal(SignalKind::hangup()),
    ) else {
        let _ = tokio::signal::ctrl_c().await;
        return;
    };

    tokio::select! {
        _ = terminate.recv() => {}
        _ = hangup.recv() => {}
        _ = tokio::signal::ctrl_c() => {}
    }
}

/// Remove every frame file from `frames`.
fn clear(frames: &Path) {
    for name in FRAME_FILES {
        let _ = fs::remove_file(frames.join(name));
    }
}

/// What the watch has said and kept so far.
struct Watch {
    frames: Option<PathBuf>,
    log: Mark,
    said: Value,
    state: State,
    read_at: Option<Instant>,
    framed_at: Option<Instant>,
    /// The kept screen's digest, and the last id given: ids only go up, so
    /// a reader never takes a new screen for one it has.
    kept: Option<[u8; 32]>,
    last_id: u64,
    /// The host session the watch runs for: other sessions' host ids are
    /// never passed on.
    own: Option<Key>,
}

impl Watch {
    fn new(frames: Option<PathBuf>) -> Self {
        Self {
            frames,
            log: None,
            said: Value::Null,
            state: State::Missing,
            read_at: None,
            framed_at: None,
            kept: None,
            last_id: 0,
            own: None,
        }
    }

    /// This watch, passing on only `own` session's host id.
    fn for_session(mut self, own: Option<Key>) -> Self {
        self.own = own;
        self
    }

    /// One look: the status when it is due, then the screen, then a blank
    /// line when nothing was said for a while. False once the reader went.
    async fn tick(&mut self, client: &Client, output: &Path, port: u16, lines: &mut Lines) -> bool {
        let mark = log_mark(output);
        let now = Instant::now();
        if self.read_due(mark, now) {
            self.log = mark;
            self.read_at = Some(now);

            // A viewer that runs but did not answer leaves the last status
            // standing, rather than a guess at who holds control.
            if let Some((mut status, source)) = read_typed(client, output, port).await {
                self.add_own_session(client, &mut status).await;
                if status.state == State::NotReady {
                    self.forget_frame();
                }

                if let Some(line) = self.news(&status, source, mark)
                    && !lines.line(&line)
                {
                    return false;
                }
            }
        }

        if self.frame_due(Instant::now())
            && let Some(line) = self.keep_frame(client, output, port).await
            && !lines.line(&line)
        {
            return false;
        }

        lines.written.elapsed() < HEARTBEAT || lines.blank()
    }

    /// `status` with the session this watch runs for as `ownSession` in its
    /// overview, found among every live session when the status's own list
    /// is too short to hold it; null when it is none of them.
    async fn add_own_session(&self, client: &Client, status: &mut Reading) {
        let Some(own) = &self.own else {
            return;
        };
        if let Some((_, entry)) = status.sessions.iter().find(|(key, _)| key == own.as_str()) {
            status.value["overview"]["ownSession"] = entry.clone();
        } else {
            atlas_client::overview::add_sessions_for(
                status,
                client,
                atlas_core::time::timestamp(),
                Some(own),
            )
            .await;
        }
    }

    /// Whether to read the status now: soon after the log changed, and
    /// every so often regardless.
    fn read_due(&self, mark: Mark, now: Instant) -> bool {
        let Some(read_at) = self.read_at else {
            return true;
        };
        let since = now.saturating_duration_since(read_at);
        (mark != self.log && since >= SOONEST) || since >= READ_EVERY
    }

    /// The line for `status`, when it says something the last one did not.
    fn news(&mut self, status: &Reading, source: &str, mark: Mark) -> Option<Value> {
        let signature = signature(&status.value, mark);
        if signature == self.said {
            return None;
        }

        self.said = signature;
        self.state = status.state.clone();
        let mut overview = status.value["overview"].clone();
        keep_own_host_id(&mut overview, self.own.as_ref());
        Some(json!({
            "type": "status",
            "source": source,
            "ready": status.ready,
            "overview": overview,
        }))
    }

    /// Whether to keep the screen now.
    fn frame_due(&self, now: Instant) -> bool {
        let every = match self.state {
            State::Working | State::Control => FRAME_EVERY,
            State::Idle | State::NeedsYou | State::Stopped | State::Extension(_) => {
                IDLE_FRAME_EVERY
            }
            State::NotReady | State::Empty | State::Missing => return false,
        };
        self.frames.is_some()
            && self
                .framed_at
                .is_none_or(|at| now.saturating_duration_since(at) >= every)
    }

    /// Keep the helper's newest screen, and the line that says so; None
    /// when there is none to keep or it has not changed.
    async fn keep_frame(&mut self, client: &Client, output: &Path, port: u16) -> Option<Value> {
        self.framed_at = Some(Instant::now());
        let frames = self.frames.clone()?;
        let png = screen(client, output, port).await?.png;
        let (width, height) = png_dimensions(&png).ok()?;

        let digest: [u8; 32] = Sha256::digest(&png).into();
        if self.kept == Some(digest) {
            return None;
        }

        let id = self.last_id + 1;
        let record = json!({"id": id, "width": width, "height": height});
        let written = tokio::task::spawn_blocking({
            let frames = frames.clone();
            move || {
                replace(&frames, "frame.png", &png)?;
                replace(&frames, "frame.json", record.to_string().as_bytes())
            }
        })
        .await
        .ok()?;
        written.ok()?;

        self.kept = Some(digest);
        self.last_id = id;
        Some(json!({
            "type": "frame",
            "id": id,
            "path": frames.join("frame.png").to_string_lossy(),
            "width": width,
            "height": height,
        }))
    }

    /// Remove the kept screen, if any.
    fn forget_frame(&mut self) {
        if let (Some(frames), Some(_)) = (&self.frames, self.kept.take()) {
            clear(frames);
        }
    }
}

/// Lines to the reader, and when the last one went.
struct Lines {
    stdout: Stdout,
    written: Instant,
}

impl Lines {
    /// Write one JSON line; false once the reader has gone. Lines are
    /// small, so writing them in place does not hold the runtime.
    fn line(&mut self, value: &Value) -> bool {
        self.send(format!("{value}\n").as_bytes())
    }

    fn blank(&mut self) -> bool {
        self.send(b"\n")
    }

    fn send(&mut self, bytes: &[u8]) -> bool {
        self.written = Instant::now();
        let mut stdout = self.stdout.lock();
        stdout.write_all(bytes).is_ok() && stdout.flush().is_ok()
    }
}

/// The status and where it came from: the menu bar's viewer when it runs
/// (or the helper, read here, when that viewer is from a build that does
/// not say the state yet); the helper when no viewer runs. None when a
/// viewer runs but did not answer: only it knows who holds control.
pub(crate) async fn read(
    client: &Client,
    output: &Path,
    port: u16,
) -> Option<(Value, &'static str)> {
    read_typed(client, output, port)
        .await
        .map(|(status, source)| (status.value, source))
}

async fn read_typed(client: &Client, output: &Path, port: u16) -> Option<(Reading, &'static str)> {
    let output = output.to_path_buf();
    let viewer = tokio::task::spawn_blocking(move || {
        let Some(url) = crate::viewer::local(&output, port) else {
            // Recorded and running, but slow to answer: busy, not gone.
            return crate::viewer::recorded(&output, port).then_some(None);
        };
        let status = crate::viewer::fetch(&url, "status.json", VIEWER_DEADLINE, STATUS_LIMIT)
            .ok()
            .and_then(|body| {
                serde_json::from_slice::<Value>(&body)
                    .ok()
                    .map(Reading::decode)
            });
        Some(status)
    })
    .await
    .ok()
    .flatten();

    match viewer {
        Some(Some(status)) if status.state.is_string() => Some((status, "viewer")),
        Some(Some(_)) | None => Some((atlas_client::overview::read(client).await, "helper")),
        Some(None) => None,
    }
}

/// The helper's screen as a PNG, and, when the menu bar's viewer gave it,
/// the key it keeps it under and the display's size in points: what taking
/// control there names.
pub(crate) struct Screen {
    pub(crate) png: Vec<u8>,
    pub(crate) shared: Option<(String, (f64, f64))>,
}

/// The helper's newest screen: the viewer's shared frame when it runs, else
/// a capture of the verified helper. None while the person signs the helper
/// in to an account, or when a sign-in began or ended while it was read.
pub(crate) async fn screen(client: &Client, output: &Path, port: u16) -> Option<Screen> {
    client
        .controller
        .shown(async { screen_now(client, output, port).await.ok_or_else(unseen) })
        .await
        .ok()
}

/// No screen was read.
fn unseen() -> Error {
    Error::new(Code::Capture, "The helper's screen could not be read")
}

/// [`screen`], whatever the screen's privacy.
async fn screen_now(client: &Client, output: &Path, port: u16) -> Option<Screen> {
    let output = output.to_path_buf();
    let shared = tokio::task::spawn_blocking(move || {
        let url = crate::viewer::local(&output, port)?;
        Some(crate::viewer::fetch_frame(&url, VIEWER_DEADLINE, FRAME_LIMIT).ok())
    })
    .await
    .ok()
    .flatten();

    match shared {
        Some(frame) => frame.map(|frame| Screen {
            png: frame.png,
            shared: Some((frame.key, frame.display)),
        }),
        None => tokio::time::timeout(READ_DEADLINE, client.frame(Some(FRAME_WIDTH)))
            .await
            .ok()?
            .ok()
            .map(|frame| Screen {
                png: frame.png.to_vec(),
                shared: None,
            }),
    }
}

/// Write `name` in `folder` whole: a reader opening it mid-write sees the
/// last whole one. Kept to the kernel, as a screen need not outlive a
/// power cut.
fn replace(folder: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    let partial = folder.join(format!("{name}.partial"));
    write_private_with(&partial, bytes, Durability::Kernel)?;
    fs::rename(&partial, folder.join(name)).map_err(|error| Error::writing(folder, error))
}

/// `overview` without the host id of any session but `own`: every other
/// session is named, never identified to its host.
fn keep_own_host_id(overview: &mut Value, own: Option<&Key>) {
    let sessions = overview["hostSessions"]
        .as_array_mut()
        .into_iter()
        .flatten();
    for session in sessions.filter_map(Value::as_object_mut) {
        let mine =
            own.is_some_and(|own| session.get("key").and_then(Value::as_str) == Some(own.as_str()));
        if !mine {
            session.remove("hostSession");
        }
    }
}

/// What a status says, without the ages that change every second; with
/// the log's mark, so a repeat of the same action still counts.
fn signature(status: &Value, mark: Mark) -> Value {
    let mut overview = status["overview"].clone();
    if let Some(rows) = overview["activity"].as_array_mut() {
        for row in rows.iter_mut().filter_map(Value::as_object_mut) {
            row.remove("ageSeconds");
        }
    }
    // A wait that counts up, or a latest action growing older, is not news;
    // a session that starts or stops waiting still is.
    if let Some(sessions) = overview["hostSessions"].as_array_mut() {
        sessions.iter_mut().for_each(without_ticking_ages);
    }
    if let Some(own) = overview.get_mut("ownSession") {
        without_ticking_ages(own);
    }

    json!({
        "ready": status["ready"],
        "overview": overview,
        "log": mark,
    })
}

/// `session` without the ages that count up while nothing else changes.
fn without_ticking_ages(session: &mut Value) {
    let Some(session) = session.as_object_mut() else {
        return;
    };
    session.remove("waitingSeconds");
    if let Some(latest) = session.get_mut("latest").and_then(Value::as_object_mut) {
        latest.remove("ageSeconds");
    }
}

fn log_mark(output: &Path) -> Mark {
    let metadata = fs::metadata(output.join(atlas_client::activity_log::FILE)).ok()?;
    Some((metadata.len(), metadata.modified().ok()?))
}

#[cfg(test)]
impl Watch {
    fn fixture_news(&mut self, status: &Value, source: &str, mark: Mark) -> Option<Value> {
        self.news(&Reading::decode(status.clone()), source, mark)
    }

    async fn fixture_own_session(&self, client: &Client, status: &mut Value) {
        let mut reading = Reading::decode(status.take());
        self.add_own_session(client, &mut reading).await;
        *status = reading.value;
    }
}

#[cfg(test)]
mod tests;
