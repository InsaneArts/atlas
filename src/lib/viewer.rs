//! The local service Atlas.app talks to: a capability URL on the loopback
//! interface that answers status, pictures of the helper, control and input
//! for one menu bar, and keeps a private record of itself in the output
//! directory so `atlas monitor` and the login item can find it.
use crate::{
    client::{Client, Frame},
    control::{self, ActionPermit, Lease, SharedLease},
    error::{Error, Result},
    menubar::{App, Line},
    session::Epoch,
};
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    response::Response,
    routing::any,
};
use serde_json::{Value, json};
use std::{collections::VecDeque, path::Path, sync::Arc, time::Duration};
use tokio::{
    net::TcpListener,
    sync::{Mutex, Semaphore},
};

const FRAME_LIFETIME: Duration = Duration::from_secs(8);
#[derive(Default)]
struct Frames {
    items: VecDeque<(String, Arc<Frame>)>,
    input_revision: u64,
    capture_revision: Option<u64>,
}

pub struct Viewer {
    client: Client,
    lease: SharedLease,
    lease_epoch: Mutex<Option<Epoch>>,
    frames: Mutex<Frames>,
    capture: Mutex<()>,
    requests: Semaphore,
    pub url: String,
    prefix: String,
    authority: String,
    /// The helper account's full name, once read.
    account_name: std::sync::OnceLock<String>,
    /// The app this viewer serves, when it serves one.
    app: std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<Line>>>,
}
impl Viewer {
    /// Pass `line` to the app this viewer serves.
    fn send(&self, line: Line) -> Result<()> {
        let app = self
            .app
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        app.as_ref()
            .ok_or_else(|| Error::new("menubar", "No menu bar is attached to this viewer"))?
            .send(line)
            .map_err(|_| Error::new("menubar", "The menu bar is not running"))
    }

    async fn frame(&self) -> Result<(String, Arc<Frame>)> {
        let _capture = self.capture.lock().await;
        let current = self.client.identity().await?.epoch;
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
        let frame = Arc::new(self.client.frame(Some(1600)).await?);
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
        while frames.items.len() > 24 {
            frames.items.pop_front();
        }
        Ok((key, frame))
    }
    async fn observed(&self, key: &str) -> Result<Arc<Frame>> {
        let frame = self
            .frames
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
            })?;
        Ok(frame)
    }
    async fn release(&self, owner: Option<&str>) -> Result<()> {
        let previous = {
            let mut lease = self.lease.lock().await;
            lease.begin_release(owner)?
        };
        if let Some(previous) = previous {
            let pending = self.client.controller.quarantine();
            let result = self.client.native_control("release", &previous).await;
            let completion = match (pending, result) {
                (Ok(generation), Ok(_)) => self.client.controller.clear_if(&generation),
                (Err(error), _) => Err(error),
                (_, Err(error)) => Err(Error::new(
                    "uncertain",
                    format!(
                        "Control stopped accepting input, but native cleanup was not confirmed: {error}"
                    ),
                )
                .uncertain()),
            };
            self.lease.lock().await.end_release();
            completion?;
        }
        Ok(())
    }
    async fn status(&self) -> Value {
        let mut status =
            crate::capability::doctor(self.client.status().await, self.client.config.helper_uid);
        if status["ready"] != true {
            let _ = self.release(None).await;
        }
        let active = self.lease.lock().await.active();
        status["controlActive"] = json!(active);
        // An unreadable marker is unknown, so it counts as unresolved.
        let controller = &self.client.controller;
        let total = controller.pending().map_or(1, |pending| pending.len());
        let unresolved = controller.unresolved().map_or(1, |pending| pending.len());
        let pending = crate::overview::Pending {
            unresolved,
            in_flight: total.saturating_sub(unresolved),
        };
        let activity = self
            .client
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
        // The helper's full name, as people know the account; looked up
        // until it is known.
        if let Some(name) = status["overview"]["account"].as_str() {
            let title = self.account_name.get().cloned().or_else(|| {
                let title = crate::onboarding::real_name(name)?;
                let _ = self.account_name.set(title.clone());
                Some(title)
            });
            status["overview"]["accountName"] = json!(title);
        }
        status
    }
    /// Explicit, person-initiated recovery: the verified restart when an
    /// outcome is unknown, otherwise starting, resetting or upgrading input.
    /// It never takes control or resumes input.
    async fn recover(&self) -> Result<Value> {
        if self.lease.lock().await.active() {
            return Err(Error::new("busy", "Release control before recovering"));
        }
        if !self.client.controller.pending()?.is_empty() {
            return self.client.restart().await;
        }
        self.client.start_input().await
    }
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
            let current = self.client.identity().await?;
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
                let token = lease.acquire(&self.client.controller)?;
                *self.lease_epoch.lock().await = Some(frame.epoch.clone());
                token
            };
            if let Err(error) = self.client.acquire_input(&token, &frame.epoch).await {
                if error.uncertain {
                    let _ = self.release(Some(&token)).await;
                } else {
                    self.lease.lock().await.abandon_acquire(&token);
                }
                return Err(error);
            }
            let authorization = self.lease.lock().await.require(&token);
            if let Err(error) = authorization {
                let _ = self.client.native_control("release", &token).await;
                return Err(error);
            }
            return Ok(json!({"ok": true, "lease": token, "ttlSeconds": 10}));
        }
        if self.lease_epoch.lock().await.as_ref() != Some(&frame.epoch) {
            return Err(Error::new(
                "session-changed",
                "The controlled session changed",
            ));
        }
        if route == "control/renew" {
            self.lease.lock().await.renew(owner)?;
            self.client.native_control("renew", owner).await?;
            return Ok(json!({"ok": true}));
        }
        let _permit = ActionPermit::begin(self.lease.clone(), owner).await?;
        let result = self
            .client
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

pub async fn bind(mut client: Client, port: u16) -> Result<(TcpListener, Arc<Viewer>, Router)> {
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
        client,
        lease: Arc::new(Mutex::new(Lease::default())),
        lease_epoch: Mutex::new(None),
        frames: Mutex::new(Frames::default()),
        capture: Mutex::new(()),
        requests: Semaphore::new(32),
        url: format!("http://{authority}{prefix}"),
        prefix,
        authority,
        app: std::sync::Mutex::new(None),
        account_name: std::sync::OnceLock::new(),
    });
    let router = Router::new()
        .fallback(any(handle))
        .with_state(viewer.clone());
    Ok((listener, viewer, router))
}

/// Serve the viewer for the menu bar app until the person quits it, or
/// until interrupted.
pub async fn serve(client: Client, port: u16, program: crate::menubar::Program) -> Result<()> {
    let output = client.output_dir()?.to_path_buf();
    let path = output.join(MENUBAR_RECORD);
    let (listener, viewer, router) = match bind(client, port).await {
        Ok(bound) => bound,
        Err(error) if error.code == "port-in-use" => {
            return already_running(&output, port, error);
        }
        Err(error) => return Err(error),
    };
    let (lines, receiver) = tokio::sync::mpsc::unbounded_channel();
    *viewer
        .app
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(lines);
    // The record names a viewer with an app; none is left when the app
    // does not start.
    let mut app = App::launch(&program, &viewer.url, receiver).await?;
    let record = json!({
        "url": viewer.url,
        "pid": std::process::id(),
        "startedAt": crate::client::timestamp().to_string(),
    });
    control::write_private(&path, &serde_json::to_vec_pretty(&record)?)?;
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
    let result = axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                () = app.exited() => {}
            }
            let _ = shutdown_view.release(None).await;
        })
        .await;
    expiry.abort();
    let _ = viewer.release(None).await;
    // The record goes with this viewer, unless another has replaced it.
    let ours = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .is_some_and(|record| record["pid"] == std::process::id());
    if ours {
        let _ = std::fs::remove_file(&path);
    }
    result.map_err(Error::from)
}

/// The record of the running viewer, in the output folder.
pub const MENUBAR_RECORD: &str = "menubar.json";

/// The viewer recorded at `record`, if it serves `port` and answers there now.
pub fn running(record: &Path, port: u16) -> Option<Value> {
    std::fs::read(record)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(|running| {
            let url = running["url"].as_str().unwrap_or("");
            url.starts_with(&format!("http://127.0.0.1:{port}/")) && answers(url)
        })
}

/// Whether a viewer answers at `url` (its private address) right now.
fn answers(url: &str) -> bool {
    exchange(url, "GET", "", None, Duration::from_secs(1)).is_ok_and(|(status, _)| status == 200)
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
    let url = record["url"].as_str().unwrap_or("");
    post_local(url, "app/viewer", &json!({}))?;
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
    let (listener, viewer, router) = bind(client, 0).await?;
    let server = tokio::spawn(async move { axum::serve(listener, router).await });
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

// A client can abort any request. Axum drops the handler future when that
// happens, which could stop between a quarantine or lease change and its
// matching cleanup. Each request therefore runs to completion in its own task.
async fn handle(State(viewer): State<Arc<Viewer>>, request: Request) -> Response {
    tokio::spawn(respond(viewer, request))
        .await
        .unwrap_or_else(|_| {
            json_response(500, json!({"ok": false, "error": "Viewer request failed"}))
        })
}

async fn respond(viewer: Arc<Viewer>, request: Request) -> Response {
    let Ok(_slot) = viewer.requests.try_acquire() else {
        return json_response(503, json!({"ok": false, "error": "Viewer is busy"}));
    };
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
    if method == Method::GET {
        // The address itself answers, so a running viewer can be found.
        if route.is_empty() {
            return json_response(200, json!({"ok": true, "service": "atlas-viewer"}));
        }
        if route == "status.json" {
            return json_response(200, viewer.status().await);
        }
        if route == "frame.png" {
            return match viewer.frame().await {
                Ok((key, frame)) => response(
                    200,
                    "image/png",
                    frame.png.to_vec(),
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
                    let _ = viewer.release(None).await;
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
        .filter(|n| (1..=32768).contains(n));
    let Some(length) = length else {
        return json_response(
            400,
            json!({"ok": false, "error": "Request body must contain 1–32768 bytes"}),
        );
    };
    let body =
        match tokio::time::timeout(Duration::from_secs(5), to_bytes(request.into_body(), 32768))
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
            if result["ok"] != true {
                let _ = viewer.release(body["lease"].as_str()).await;
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
    response(
        code,
        "application/json",
        value.to_string().into_bytes(),
        &[],
    )
}
fn response(code: u16, mime: &str, body: Vec<u8>, headers: &[(&str, String)]) -> Response {
    let mut response = Response::new(Body::from(body));
    *response.status_mut() =
        StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    for (key, value) in [
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
    ] {
        if let (Ok(key), Ok(value)) = (
            axum::http::HeaderName::try_from(key),
            HeaderValue::from_str(value),
        ) {
            response.headers_mut().insert(key, value);
        }
    }
    for (key, value) in headers {
        if let (Ok(key), Ok(value)) = (
            axum::http::HeaderName::try_from(*key),
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

    /// A viewer whose helper is unreachable and whose controller state is
    /// private to the test.
    /// A viewer bound to a free port, for a helper that is not there.
    async fn bound(dir: &std::path::Path) -> (tokio::net::TcpListener, Arc<Viewer>, Router) {
        let text = "helper_uid = 60001\nsocket_path = \"c.sock\"\noutput_dir = \"out\"\n\
                    identity_probe = \"probe\"\ninput_socket = \"i.sock\"\n";
        let config = crate::config::Config::parse(text.as_bytes(), &dir.join("u.toml")).unwrap();
        let mut client = Client::new(config).unwrap();
        client.controller = control::Controller::at(60001, &dir.join("state"));
        bind(client, 0).await.unwrap()
    }

    async fn unreachable(dir: &std::path::Path) -> Arc<Viewer> {
        bound(dir).await.1
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
        let (listener, viewer, router) = bound(dir.path()).await;
        let server = tokio::spawn(async move { axum::serve(listener, router).await });
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
        assert!(
            tokio::task::spawn_blocking(move || answers(&url))
                .await
                .unwrap()
        );
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
        let controller = viewer.client.controller.clone();
        viewer.lease.lock().await.acquire(&controller).unwrap();
        let refused = viewer.recover().await.unwrap_err();
        assert_eq!(refused.code, "busy");
        assert!(controller.pending().unwrap().is_empty());
    }

    #[tokio::test]
    async fn revoking_ends_any_lease_and_keeps_unconfirmed_cleanup_pending() {
        let dir = tempfile::tempdir().unwrap();
        let viewer = unreachable(dir.path()).await;
        let controller = viewer.client.controller.clone();
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

    #[tokio::test]
    async fn failed_recovery_keeps_unknown_outcomes_pending() {
        let dir = tempfile::tempdir().unwrap();
        let viewer = unreachable(dir.path()).await;
        let controller = viewer.client.controller.clone();
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
}
