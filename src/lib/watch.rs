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
use crate::{
    client::{Client, png_dimensions},
    error::{Code, Error, Result},
    files::{Durability, write_private_with},
    host_session::{HostSession, Key},
    overview::READ_DEADLINE,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Stdout, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt},
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
pub async fn watch(client: Client, port: u16, frames: Option<PathBuf>, once: bool) -> Result<()> {
    // Only a folder this watch made is removed at the end.
    let made = frames.as_ref().is_some_and(|frames| !frames.exists());
    if let Some(frames) = &frames {
        private_folder(frames)?;
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
            if let Some((mut status, source)) = read(client, output, port).await {
                self.add_own_session(client, &mut status).await;
                if status["overview"]["state"] == "notReady" {
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
    async fn add_own_session(&self, client: &Client, status: &mut Value) {
        let Some(own) = &self.own else {
            return;
        };
        let mine = |session: &&Value| session["key"].as_str() == Some(own.as_str());
        let listed = status["overview"]["hostSessions"]
            .as_array()
            .and_then(|sessions| sessions.iter().find(mine))
            .cloned();
        let found = match listed {
            Some(entry) => Some(entry),
            None => {
                let facts = crate::overview::SessionFacts::of(status, crate::client::timestamp());
                crate::overview::live_host_sessions(client, facts)
                    .await
                    .iter()
                    .find(mine)
                    .cloned()
            }
        };
        status["overview"]["ownSession"] = found.unwrap_or(Value::Null);
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
    fn news(&mut self, status: &Value, source: &str, mark: Mark) -> Option<Value> {
        let signature = signature(status, mark);
        if signature == self.said {
            return None;
        }

        self.said = signature;
        let mut overview = status["overview"].clone();
        keep_own_host_id(&mut overview, self.own.as_ref());
        Some(json!({
            "type": "status",
            "source": source,
            "ready": status["ready"] == true,
            "overview": overview,
        }))
    }

    /// Whether to keep the screen now.
    fn frame_due(&self, now: Instant) -> bool {
        let state = self.said["overview"]["state"].as_str().unwrap_or_default();
        let every = match state {
            "working" | "control" => FRAME_EVERY,
            _ => IDLE_FRAME_EVERY,
        };
        self.frames.is_some()
            && state != "notReady"
            && !state.is_empty()
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
    let output = output.to_path_buf();
    let viewer = tokio::task::spawn_blocking(move || {
        let Some(url) = crate::viewer::local(&output, port) else {
            // Recorded and running, but slow to answer: busy, not gone.
            return crate::viewer::recorded(&output, port).then_some(None);
        };
        let status = crate::viewer::fetch(&url, "status.json", VIEWER_DEADLINE, STATUS_LIMIT)
            .ok()
            .and_then(|body| serde_json::from_slice::<Value>(&body).ok());
        Some(status)
    })
    .await
    .ok()
    .flatten();

    match viewer {
        Some(Some(status)) if status["overview"]["state"].is_string() => Some((status, "viewer")),
        Some(Some(_)) | None => Some((crate::overview::read(client).await, "helper")),
        Some(None) => None,
    }
}

/// The helper's screen as a PNG, and, when the menu bar's viewer gave it,
/// the key it keeps it under and the display's size in points: what taking
/// control there names.
pub(crate) struct Screen {
    pub png: Vec<u8>,
    pub shared: Option<(String, (f64, f64))>,
}

/// The helper's newest screen: the viewer's shared frame when it runs, else
/// a capture of the verified helper.
pub(crate) async fn screen(client: &Client, output: &Path, port: u16) -> Option<Screen> {
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
    let metadata = fs::metadata(output.join(crate::activity_log::FILE)).ok()?;
    Some((metadata.len(), metadata.modified().ok()?))
}

/// `folder`, made 0700 when missing; one that exists must be a private
/// folder this account owns.
fn private_folder(folder: &Path) -> Result<()> {
    if !folder.exists() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(folder)
            .map_err(|error| Error::writing(folder, error))?;
    }

    let metadata = fs::symlink_metadata(folder)?;
    if !metadata.is_dir()
        || metadata.uid() != nix::unistd::getuid().as_raw()
        || metadata.mode() & 0o077 != 0
    {
        return Err(Error::new(
            Code::Usage,
            format!(
                "{} must be a private (0700) folder you own",
                folder.display()
            ),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn status(state: &str, headline: &str, age: u64) -> Value {
        json!({
            "ready": true,
            "overview": {
                "state": state,
                "headline": headline,
                "activity": [{"says": headline, "ageSeconds": age}],
            },
        })
    }

    fn mark(length: u64) -> Mark {
        Some((length, SystemTime::UNIX_EPOCH))
    }

    #[test]
    fn a_watch_passes_on_only_its_own_sessions_host_id() {
        use crate::host_session::Host;
        let (own, other) = (Key::of(Host::Claude, "own"), Key::of(Host::Codex, "other"));
        let status = json!({"ready": true, "overview": {"state": "idle", "hostSessions": [
            {"key": own, "name": "Claude Code · left", "hostSession": "own"},
            {"key": other, "name": "Codex · right", "hostSession": "other"},
        ]}});

        let line = Watch::new(None)
            .for_session(Some(own))
            .news(&status, "helper", None)
            .unwrap();
        let sessions = line["overview"]["hostSessions"].as_array().unwrap();
        assert_eq!(sessions[0]["hostSession"], "own");
        assert!(sessions[1].get("hostSession").is_none());
        assert_eq!(sessions[1]["name"], "Codex · right");

        let line = Watch::new(None).news(&status, "helper", None).unwrap();
        let sessions = line["overview"]["hostSessions"].as_array().unwrap();
        assert!(
            sessions
                .iter()
                .all(|session| session.get("hostSession").is_none()),
            "without a session, none"
        );
    }

    #[test]
    fn a_watch_says_whose_it_is_and_a_ticking_age_says_nothing_new() {
        use crate::host_session::Host;
        let own = Key::of(Host::Claude, "own");
        let status = |state: &str, waited: u64, age: u64| {
            json!({"ready": true, "overview": {"state": "working", "sessionsShown": true,
            "hostSessions": [
                {"key": own, "name": "Claude Code · left", "hostSession": "own",
                    "state": "acting", "latest": {"says": "clicked", "ageSeconds": age}},
                {"key": Key::of(Host::Codex, "other"), "name": "Codex · right",
                    "hostSession": "other", "state": state, "waitingFor": "helper",
                    "waitingSeconds": waited},
            ]}})
        };
        let mut watch = Watch::new(None).for_session(Some(own.clone()));

        let first = watch
            .news(&status("waiting", 1, 0), "helper", None)
            .unwrap();
        let sessions = first["overview"]["hostSessions"].as_array().unwrap();
        assert_eq!(sessions[0]["hostSession"], "own", "its own session is told");
        assert!(sessions[1].get("hostSession").is_none());
        assert_eq!(sessions[1]["waitingSeconds"], 1);

        assert!(
            watch
                .news(&status("waiting", 9, 7), "helper", None)
                .is_none(),
            "only ages changed"
        );
        assert!(
            watch.news(&status("idle", 9, 7), "helper", None).is_some(),
            "a state change is news"
        );
    }

    #[tokio::test]
    async fn a_watch_keeps_its_own_session_beside_the_listed_ones() {
        use crate::host_session::{Host, HostSession, store};

        let directory = tempfile::tempdir().unwrap();
        let client = crate::test_support::client(directory.path());
        let own = HostSession::new(Host::Claude, "own").unwrap();
        store::note(
            &client,
            &own,
            store::Kind::Other,
            Some(&crate::doing::Doing::new("click")),
            &Ok(json!({"ok": true})),
        )
        .unwrap();
        let watch = Watch::new(None).for_session(Some(own.key().clone()));

        let mut listed = json!({"overview": {"hostSessions": [
            {"key": own.key(), "name": "Claude Code · left", "state": "acting"},
        ]}});
        watch.add_own_session(&client, &mut listed).await;
        assert_eq!(listed["overview"]["ownSession"]["state"], "acting");

        let mut cut = json!({"overview": {"hostSessions": []}});
        watch.add_own_session(&client, &mut cut).await;
        assert_eq!(
            cut["overview"]["ownSession"]["key"],
            own.key().as_str(),
            "found among every live session when the list left it out"
        );
        assert_eq!(cut["overview"]["ownSession"]["state"], "idle");

        let mut none = json!({"overview": {}});
        Watch::new(None).add_own_session(&client, &mut none).await;
        assert!(none["overview"].get("ownSession").is_none());
    }

    #[test]
    fn a_status_is_said_first_then_only_when_what_it_says_changes() {
        let mut watch = Watch::new(None);
        let first = watch.news(&status("working", "clicked Save", 1), "helper", mark(10));
        assert_eq!(first.as_ref().unwrap()["type"], "status");
        assert_eq!(first.as_ref().unwrap()["source"], "helper");
        assert_eq!(first.unwrap()["overview"]["headline"], "clicked Save");

        assert_eq!(
            watch.news(&status("working", "clicked Save", 9), "helper", mark(10)),
            None,
            "an age is not news"
        );
        assert!(
            watch
                .news(&status("working", "clicked Save", 1), "helper", mark(20))
                .is_some(),
            "the same action again is news"
        );
        assert!(
            watch
                .news(&status("idle", "clicked Save", 1), "helper", mark(20))
                .is_some()
        );
    }

    #[test]
    fn the_status_is_read_soon_after_a_row_and_every_few_seconds_otherwise() {
        let start = Instant::now();
        let mut watch = Watch::new(None);
        assert!(watch.read_due(mark(1), start), "the first read is at once");

        watch.read_at = Some(start);
        watch.log = mark(1);
        let at = |seconds: f64| start + Duration::from_secs_f64(seconds);

        assert!(!watch.read_due(mark(1), at(2.0)));
        assert!(
            !watch.read_due(mark(2), at(0.5)),
            "rows coming fast wait a moment"
        );
        assert!(watch.read_due(mark(2), at(1.0)));
        assert!(watch.read_due(mark(1), at(5.0)));
    }

    #[test]
    fn the_screen_is_kept_every_second_while_someone_acts_and_now_and_then_otherwise() {
        let dir = tempfile::tempdir().unwrap();
        let start = Instant::now();
        let mut watch = Watch::new(Some(dir.path().to_path_buf()));

        for (state, due) in [
            ("working", true),
            ("control", true),
            ("idle", true),
            ("needsYou", true),
            ("notReady", false),
            ("", false),
        ] {
            watch.news(&status(state, "", 0), "viewer", None);
            assert_eq!(watch.frame_due(start), due, "{state:?}");
        }

        watch.framed_at = Some(start);
        watch.news(&status("working", "", 0), "viewer", None);
        assert!(!watch.frame_due(start + Duration::from_millis(500)));
        assert!(watch.frame_due(start + FRAME_EVERY));
        watch.news(&status("idle", "", 0), "viewer", None);
        assert!(!watch.frame_due(start + FRAME_EVERY));
        assert!(watch.frame_due(start + IDLE_FRAME_EVERY));
        assert!(!Watch::new(None).frame_due(start), "only with a folder");
    }

    #[test]
    fn missing_or_nonstring_retained_state_never_requests_a_frame() {
        let dir = tempfile::tempdir().unwrap();
        for overview in [
            Value::Null,
            json!({}),
            json!({"state": null}),
            json!({"state": 7}),
            json!({"state": true}),
        ] {
            let mut watch = Watch::new(Some(dir.path().to_path_buf()));
            assert!(!watch.frame_due(Instant::now()), "nothing known yet");
            assert!(
                watch
                    .news(&json!({"overview": overview}), "viewer", None)
                    .is_some()
            );
            assert_eq!(watch.said["overview"]["state"], overview["state"]);
            assert!(!watch.frame_due(Instant::now()), "{overview}");
        }
    }

    #[tokio::test]
    async fn a_busy_viewer_keeps_known_cadence_until_not_ready_forgets_the_screen() {
        use axum::{Json, Router, http::StatusCode, routing::get};
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let dir = tempfile::tempdir().unwrap();
        let client = crate::test_support::client_in(dir.path());
        let output = client.output_dir().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = format!("http://127.0.0.1:{port}/viewer/");
        fs::write(
            output.join(crate::viewer::MENUBAR_RECORD),
            json!({"url": url, "pid": std::process::id()}).to_string(),
        )
        .unwrap();
        let observed = Arc::new(Mutex::new((
            StatusCode::OK,
            status("working", "working", 0),
        )));
        let reading = observed.clone();
        let png = crate::native::capture::sample_png(8, 8);
        let app = Router::new()
            .route(
                "/viewer/",
                get(|| async {
                    Json(json!({"service": "atlas-viewer", "pid": std::process::id()}))
                }),
            )
            .route(
                "/viewer/status.json",
                get(move || {
                    let reading = reading.clone();
                    async move {
                        let (code, status) = reading.lock().await.clone();
                        (code, Json(status))
                    }
                }),
            )
            .route(
                "/viewer/frame.png",
                get(move || {
                    let png = png.clone();
                    async move {
                        (
                            [
                                ("x-frame-id", "shown"),
                                ("x-display-width", "8"),
                                ("x-display-height", "8"),
                            ],
                            png,
                        )
                    }
                }),
            );
        let serving = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let frames = dir.path().join("frames");
        private_folder(&frames).unwrap();
        let mut watch = Watch::new(Some(frames.clone()));
        let mut lines = Lines {
            stdout: std::io::stdout(),
            written: Instant::now(),
        };

        assert!(watch.tick(&client, output, port, &mut lines).await);
        assert_eq!(watch.said["overview"]["state"], "working");
        assert!(frames.join("frame.png").exists());
        assert!(frames.join("frame.json").exists());
        let said = watch.said.clone();
        let kept = watch.kept;
        let id = watch.last_id;
        // Keep frame scheduling out of this read-only status transition; its cadence is checked at explicit instants.
        let framed = Instant::now() + Duration::from_secs(60);
        watch.framed_at = Some(framed);
        watch.read_at = None;
        *observed.lock().await = (StatusCode::SERVICE_UNAVAILABLE, json!({}));
        assert!(read(&client, output, port).await.is_none());
        assert!(watch.tick(&client, output, port, &mut lines).await);
        assert_eq!(
            watch.said, said,
            "an unanswered viewer is not a guessed helper state"
        );
        assert_eq!(watch.kept, kept);
        assert_eq!(watch.framed_at, Some(framed));
        assert_eq!(watch.last_id, id);
        assert!(!watch.frame_due(framed + Duration::from_millis(500)));
        assert!(
            watch.frame_due(framed + FRAME_EVERY),
            "the previous active cadence remains"
        );

        watch.read_at = None;
        *observed.lock().await = (StatusCode::OK, status("notReady", "not ready", 0));
        assert!(watch.tick(&client, output, port, &mut lines).await);
        assert_eq!(watch.said["overview"]["state"], "notReady");
        assert!(watch.kept.is_none());
        assert!(!frames.join("frame.png").exists());
        assert!(!frames.join("frame.json").exists());
        assert_eq!(watch.last_id, id, "frame identities never go backwards");
        assert!(!watch.frame_due(framed + IDLE_FRAME_EVERY));
        serving.abort();
        let _ = serving.await;
    }

    #[test]
    fn a_kept_screen_is_written_whole_and_forgotten_with_its_record() {
        let dir = tempfile::tempdir().unwrap();
        replace(dir.path(), "frame.png", b"png").unwrap();
        replace(dir.path(), "frame.json", b"{}").unwrap();
        assert_eq!(fs::read(dir.path().join("frame.png")).unwrap(), b"png");
        assert!(!dir.path().join("frame.png.partial").exists());
        let mode = fs::metadata(dir.path().join("frame.png")).unwrap().mode();
        assert_eq!(mode & 0o777, 0o600);

        let mut watch = Watch::new(Some(dir.path().to_path_buf()));
        watch.kept = Some([0; 32]);
        watch.last_id = 3;
        watch.forget_frame();
        assert!(!dir.path().join("frame.png").exists());
        assert!(!dir.path().join("frame.json").exists());
        assert_eq!(watch.last_id, 3, "ids go on from where they were");

        fs::write(dir.path().join("frame.png.partial"), b"half").unwrap();
        clear(dir.path());
        assert!(!dir.path().join("frame.png.partial").exists());
    }

    #[test]
    fn the_log_mark_changes_with_each_row() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(log_mark(dir.path()), None);

        fs::write(dir.path().join(crate::activity_log::FILE), "{}\n").unwrap();
        let first = log_mark(dir.path());
        fs::write(dir.path().join(crate::activity_log::FILE), "{}\n{}\n").unwrap();
        assert_ne!(log_mark(dir.path()), first);
    }

    #[test]
    fn frames_go_only_to_a_private_folder() {
        let dir = tempfile::tempdir().unwrap();
        let made = dir.path().join("frames");
        private_folder(&made).unwrap();
        assert_eq!(fs::metadata(&made).unwrap().mode() & 0o777, 0o700);

        let open = dir.path().join("open");
        fs::create_dir(&open).unwrap();
        fs::set_permissions(&open, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(private_folder(&open).unwrap_err().code, "usage");
    }
}
