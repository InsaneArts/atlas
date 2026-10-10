use crate::{
    client::{Client, Frame},
    control::{self, ActionPermit, Lease, SharedLease},
    error::{Error, Result},
    session::{Epoch, diagnose},
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
use std::{collections::VecDeque, sync::Arc, time::Duration};
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
}
impl Viewer {
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
        let mut status = diagnose(self.client.status().await, self.client.config.helper_uid);
        if status["ready"] != true {
            let _ = self.release(None).await;
        }
        status["controlActive"] = json!(self.lease.lock().await.active());
        status
    }
    async fn control(&self, route: &str, body: &Value) -> Result<Value> {
        let owner = body["lease"].as_str().unwrap_or_default();
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

pub async fn bind(client: Client, port: u16) -> Result<(TcpListener, Arc<Viewer>, Router)> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
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
    });
    let router = Router::new()
        .fallback(any(handle))
        .with_state(viewer.clone());
    Ok((listener, viewer, router))
}

pub async fn serve(client: Client, port: u16) -> Result<()> {
    let (listener, viewer, router) = bind(client, port).await?;
    let record = json!({
        "url": viewer.url,
        "pid": std::process::id(),
        "control": "explicit lease",
        "startedAt": crate::client::timestamp().to_string(),
    });
    control::write_private(
        &viewer.client.output_dir()?.join("monitor.json"),
        &serde_json::to_vec_pretty(&record)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&record)?);
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
            let _ = tokio::signal::ctrl_c().await;
            let _ = shutdown_view.release(None).await;
        })
        .await;
    expiry.abort();
    let _ = viewer.release(None).await;
    result.map_err(Error::from)
}

fn header<'a>(headers: &'a HeaderMap, key: &str) -> Option<&'a str> {
    headers.get(key)?.to_str().ok()
}

// A browser can abort any request. Axum drops the handler future when that
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
        return json_response(
            404,
            json!({"ok": false, "error": "Use the viewer URL printed by Understudy"}),
        );
    };
    if method == Method::GET {
        if let Some((mime, bytes)) = asset(&route) {
            return response(200, mime, bytes.to_vec(), &[]);
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
            "input" | "control/acquire" | "control/renew" | "control/release"
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
    match viewer.control(&route, &body).await {
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

fn asset(route: &str) -> Option<(&'static str, &'static [u8])> {
    Some(match route {
        "" => (
            "text/html; charset=utf-8",
            include_bytes!("../../web/index.html"),
        ),
        "app.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../../web/app.js"),
        ),
        "input.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../../web/input.js"),
        ),
        "queue.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../../web/queue.js"),
        ),
        "pointer.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../../web/pointer.js"),
        ),
        "preview.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../../web/preview.js"),
        ),
        "styles.css" => (
            "text/css; charset=utf-8",
            include_bytes!("../../web/styles.css"),
        ),
        "favicon.svg" => ("image/svg+xml", include_bytes!("../../web/favicon.svg")),
        _ => return None,
    })
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
