//! The viewer's HTTP server on the loopback interface: connections within
//! their limits, each request only from the viewer's own origin and private
//! address, routed once and answered in a task of its own.
use super::{LEASE_SECONDS, SERVICE, Viewer, control::Control, frames::Frames};
use crate::{
    client::{Client, Frame},
    control::Lease,
    error::{Code, Error, Result},
    files, helper_info,
    menubar::Line,
};
use axum::{
    body::{Body, Bytes, to_bytes},
    extract::Request,
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    response::Response,
};
use serde_json::{Value, json};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{Mutex, OwnedSemaphorePermit, RwLock, Semaphore, watch},
};

/// How many requests the viewer serves at once.
pub(super) const CONCURRENT_REQUESTS: usize = 32;
/// Requests kept for ending control, so a busy viewer still lets go.
const URGENT_REQUESTS: usize = 8;
/// How many connections are open at once; more wait to be accepted.
pub(super) const CONNECTIONS: usize = 64;
/// How long a connection may take to send a request's headers. An idle
/// connection closes after as long.
const HEADER_DEADLINE: Duration = Duration::from_secs(15);
/// How long a new connection may take to ask for something at the viewer's
/// private address. One that has not closes, so connections nobody uses
/// cannot keep every place taken.
pub(super) const FIRST_REQUEST: Duration = Duration::from_secs(3);
/// The largest request body, in bytes.
const MAX_BODY: usize = 32768;

/// Bind the viewer for `client` on `port` (0 picks a free one). With
/// `settings`, it follows that file when it names another helper.
pub(super) async fn bind(
    client: Client,
    port: u16,
    settings: Option<&Path>,
) -> Result<(TcpListener, Arc<Viewer>)> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::AddrInUse => Error::new(
                Code::PortInUse,
                format!("Port {port} is in use by another program."),
            ),
            _ => crate::context::context(format_args!("Bind viewer listener at 127.0.0.1:{port}"))(
                error,
            ),
        })?;

    let authority = listener
        .local_addr()
        .map_err(crate::context::context(
            "Read bound viewer listener address",
        ))?
        .to_string();
    Ok((listener, configured(client, authority, settings)))
}

/// Configure the viewer independently of its listener, so control and
/// recording can be checked without networking.
pub(super) fn configured(
    mut client: Client,
    authority: String,
    settings: Option<&Path>,
) -> Arc<Viewer> {
    // Everything this viewer does in the helper is the person's doing, and
    // their input is gathered into a row a span.
    client.actor = crate::client::Actor::Viewer;
    client.person_input = Some(crate::client::PersonInput::default());
    let prefix = format!("/{}/", files::token());
    Arc::new(Viewer {
        client: std::sync::Mutex::new(Arc::new(client)),
        settings: settings.map(Path::to_path_buf),
        settings_text: std::sync::Mutex::new(None),
        lease: Arc::new(Mutex::new(Lease::new(Duration::from_secs(LEASE_SECONDS)))),
        lease_epoch: Mutex::new(None),
        lease_client: std::sync::Mutex::new(None),
        input_recording: Mutex::new(()),
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
    })
}

/// Answer requests on `listener` until `shutdown` resolves. A connection
/// must send each request's headers within [`HEADER_DEADLINE`], and at most
/// `connections` are open at once; more wait to be accepted.
pub(super) async fn accept(
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

/// The header `key` in `headers`, when it is there as text.
pub(super) fn header<'a>(headers: &'a HeaderMap, key: &str) -> Option<&'a str> {
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

/// What a request at the viewer's private address asks for, by its method
/// and the path after that address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    /// The address itself, which says what serves it.
    Service,
    Read(Read),
    /// Opening the viewer window in the app.
    OpenViewer,
    Change(Change),
}

/// A read for the app: it changes nothing in the helper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Read {
    Status,
    Activity,
    Sessions,
    Apps,
    /// What cleaning up a session would do.
    Cleanup,
    Screenshots,
    Screenshot,
    Frame,
}

/// A change in the helper, which shutdown waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Change {
    Recover,
    Cleanup,
    /// The person's stop for agents, or their resume.
    Hold,
    Control(Control),
}

impl Route {
    /// The route `method` asks for at `path`; None for any other.
    fn parse(method: &Method, path: &str) -> Option<Self> {
        if *method == Method::GET {
            return Some(match path {
                "" => Self::Service,
                "status.json" => Self::Read(Read::Status),
                "activity.json" => Self::Read(Read::Activity),
                "sessions.json" => Self::Read(Read::Sessions),
                "apps.json" => Self::Read(Read::Apps),
                "cleanup.json" => Self::Read(Read::Cleanup),
                "screenshots.json" => Self::Read(Read::Screenshots),
                "screenshot.png" => Self::Read(Read::Screenshot),
                "frame.png" => Self::Read(Read::Frame),
                _ => return None,
            });
        }

        if *method != Method::POST {
            return None;
        }

        Some(match path {
            "app/viewer" => Self::OpenViewer,
            "recover" => Self::Change(Change::Recover),
            "cleanup" => Self::Change(Change::Cleanup),
            "control/hold" => Self::Change(Change::Hold),
            "input" => Self::Change(Change::Control(Control::Input)),
            "control/acquire" => Self::Change(Change::Control(Control::Acquire)),
            "control/renew" => Self::Change(Change::Control(Control::Renew)),
            "control/release" => Self::Change(Change::Control(Control::Release)),
            "control/revoke" => Self::Change(Change::Control(Control::Revoke)),
            _ => return None,
        })
    }

    /// Whether it ends control or stops agents, which have slots of their
    /// own.
    fn urgent(self) -> bool {
        matches!(
            self,
            Self::Change(Change::Hold | Change::Control(Control::Release | Control::Revoke))
        )
    }
}

/// Answer one request: only from the viewer's own origin and private
/// address, within a free slot, and for a known route.
pub(super) async fn respond(viewer: Arc<Viewer>, request: Request) -> Response {
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

    let Some(path) = request.uri().path().strip_prefix(&viewer.prefix) else {
        return json_response(404, json!({"ok": false, "error": "Unknown viewer address"}));
    };
    let route = Route::parse(&method, path);

    // The address itself answers, with the process serving it, so a
    // running viewer can be found and stopped however busy it is.
    if route == Some(Route::Service) {
        return json_response(
            200,
            json!({"ok": true, "service": SERVICE, "pid": std::process::id()}),
        );
    }

    // Ending control has slots of its own, so a busy viewer still lets go.
    let slots = if route.is_some_and(Route::urgent) {
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

    let change = match route {
        Some(Route::Read(read)) => return answer(&viewer, read, request.uri().query()).await,
        Some(Route::OpenViewer) => None,
        Some(Route::Change(change)) => Some(change),
        Some(Route::Service) | None => {
            return json_response(404, json!({"ok": false, "error": "Unknown operation"}));
        }
    };

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

    let Some(change) = change else {
        return match viewer.send(Line::Viewer) {
            Ok(()) => json_response(200, json!({"ok": true, "viewer": "open"})),
            Err(error) => json_response(409, json!(error)),
        };
    };

    // Held to the end of the request, cleanup included, so shutdown waits
    // for it.
    let Some(_changing) = viewer.changing().await else {
        return json_response(503, json!({"ok": false, "error": "The viewer is closing"}));
    };

    let outcome = match change {
        Change::Recover => viewer.recover().await,
        Change::Cleanup => viewer.clean_up(&body).await,
        // Control a viewer holds is left alone either way.
        Change::Hold => viewer.client().hold(body["on"] == true).await,
        Change::Control(control) => viewer.control(control, &body).await,
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
            if !matches!(
                error.code,
                Code::Busy | Code::LeaseLost | Code::InvalidJson | Code::Input
            ) && let Some(owner) = body["lease"].as_str()
            {
                let _ = viewer.release(Some(owner)).await;
            }

            json_response(
                if matches!(error.code, Code::InvalidJson | Code::Input) {
                    400
                } else {
                    409
                },
                json!(error),
            )
        }
    }
}

/// Answer `read`, which the request's `query` may say more of.
async fn answer(viewer: &Arc<Viewer>, read: Read, query: Option<&str>) -> Response {
    match read {
        Read::Status => json_response(200, viewer.status().await),
        Read::Activity => read_reply(viewer.activity().await),
        Read::Sessions => read_reply(viewer.sessions().await),
        Read::Apps => read_reply(viewer.apps().await),
        Read::Cleanup => match viewer.cleanup_preview(query).await {
            Err(error) if error.code == Code::Input => json_response(400, json!(error)),
            outcome => read_reply(outcome),
        },
        Read::Screenshots => read_reply(viewer.screenshots().await),
        Read::Screenshot => viewer.screenshot(query).await,
        Read::Frame => match viewer.frame().await {
            Ok((key, frame)) => frame_reply(key, &frame),
            Err(error) => {
                // Ending control finishes by itself; the answer is not held.
                viewer.release_in_background();
                json_response(503, json!(error))
            }
        },
    }
}

/// A read's reply: what it found, or why the helper could not be read.
fn read_reply(outcome: Result<Value>) -> Response {
    match outcome {
        Ok(found) => json_response(200, found),
        Err(error) => json_response(503, json!(error)),
    }
}

/// `frame` as a PNG, with the key control requests name it by, the
/// display's size and the session it shows.
fn frame_reply(key: String, frame: &Frame) -> Response {
    response(
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
    )
}

/// A reply of `value` as JSON, with status `code`.
pub(super) fn json_response(code: u16, value: Value) -> Response {
    response(code, "application/json", Body::from(value.to_string()), &[])
}

/// A reply of `body` as `mime`, with status `code`, the headers every
/// reply carries (not cached, not shown inside other pages, nothing
/// loaded from elsewhere) and then `headers`.
pub(super) fn response(code: u16, mime: &str, body: Body, headers: &[(&str, String)]) -> Response {
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
