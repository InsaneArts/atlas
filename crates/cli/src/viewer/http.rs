//! The viewer's HTTP server on the loopback interface: connections within
//! their limits, each request only from the viewer's own origin and private
//! address, routed once and answered in a task of its own.
use super::{
    LEASE_SECONDS, LeaseRefusal, SERVICE, Viewer,
    frames::{FRAME_REUSE, FRAME_WIDTH, Frames},
    unpoisoned,
};
use crate::menubar::Line;
use atlas_client::{client::Client, helper_info};
use atlas_control::Lease;
use atlas_core::screen::frame::Frame;
use atlas_core::{
    error::{Code, Error, Result},
    files,
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
            _ => error.into(),
        })?;

    let authority = listener.local_addr()?.to_string();
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
    client.actor = atlas_client::client::Actor::Viewer;
    client.person_input = Some(atlas_client::client::PersonInput::default());
    let prefix = format!("/{}/", files::token());
    let outputs = vec![client.config.output_dir.clone()];
    Arc::new(Viewer {
        client: std::sync::Mutex::new(Arc::new(client)),
        settings: settings.map(Path::to_path_buf),
        settings_text: std::sync::Mutex::new(None),
        outputs: std::sync::Mutex::new(outputs),
        output_moved: tokio::sync::Notify::new(),
        lease: Arc::new(Mutex::new(Lease::new(Duration::from_secs(LEASE_SECONDS)))),
        lease_epoch: Mutex::new(None),
        lease_client: std::sync::Mutex::new(None),
        input_recording: Mutex::new(()),
        closed: RwLock::new(false),
        frames: Mutex::new(Frames::default()),
        committed: std::sync::Mutex::new(std::collections::VecDeque::new()),
        pipe: std::sync::Mutex::new(None),
        input_sent: Arc::new(tokio::sync::Notify::new()),
        frame_width: FRAME_WIDTH,
        frame_reuse: FRAME_REUSE,
        capture: std::sync::Mutex::new(None),
        requests: Semaphore::new(CONCURRENT_REQUESTS),
        urgent: Semaphore::new(URGENT_REQUESTS),
        url: format!("http://{authority}{prefix}"),
        prefix,
        authority,
        app: std::sync::Mutex::new(None),
        account_name: std::sync::Mutex::new(None),
        accounts_home: std::sync::Mutex::new(atlas_core::config::home().ok()),
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
    // Small input requests must not wait behind Nagle and delayed ACKs.
    if stream.set_nodelay(true).is_err() {
        return;
    }
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

        tokio::select! {
            _ = served.as_mut() => return,
            _ = closed.changed() => {}
            named = first => {
                if !named {
                    return;
                }

                tokio::select! {
                    _ = served.as_mut() => return,
                    _ = closed.changed() => {}
                }
            }
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
        .unwrap_or_else(|_| refused(500, "Viewer request failed"))
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

/// A route's identity, method and path, declared together.
macro_rules! routes {
    (
        GET { $($read:ident => $read_path:literal,)+ }
        POST { $($change:ident => $change_path:literal,)+ }
    ) => {
        /// A read for the app: it changes nothing in the helper.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum Read { $($read),+ }

        /// A change in the helper, which shutdown waits for.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum Change { $($change),+ }

        impl Route {
            /// The route `method` asks for at `path`; None for any other.
            fn parse(method: &Method, path: &str) -> Option<Self> {
                if *method == Method::GET {
                    return match path {
                        "" => Some(Self::Service),
                        $($read_path => Some(Self::Read(Read::$read)),)+
                        _ => None,
                    };
                }

                if *method != Method::POST {
                    return None;
                }

                match path {
                    "app/viewer" => Some(Self::OpenViewer),
                    $($change_path => Some(Self::Change(Change::$change)),)+
                    _ => None,
                }
            }
        }
    };
}

routes! {
    GET {
        Status => "status.json",
        Activity => "activity.json",
        Sessions => "sessions.json",
        Apps => "apps.json",
        // What cleaning up a session would do.
        Cleanup => "cleanup.json",
        Screenshots => "screenshots.json",
        Screenshot => "screenshot.png",
        Frame => "frame.png",
        // How to pair with the frame service, when this viewer runs one.
        Pipe => "pipe.json",
        // What Atlas asks the person before an agent sends, with what would
        // be sent: theirs alone.
        Asks => "asks.json",
    }
    POST {
        Recover => "recover",
        Cleanup => "cleanup",
        // The person's stop for agents, or their resume.
        Hold => "control/hold",
        Input => "input",
        Acquire => "control/acquire",
        Renew => "control/renew",
        Release => "control/release",
        Revoke => "control/revoke",
        // The person's own list of the helper's accounts.
        AddAccount => "accounts/add",
        RemoveAccount => "accounts/remove",
        SetLevel => "accounts/level",
        // The person's answer to a question, and the end of what they allowed.
        Answer => "asks/answer",
        EndAllowed => "allowed/end",
    }
}

impl Route {
    /// Whether it ends control or stops agents, which have slots of their
    /// own.
    fn urgent(self) -> bool {
        match self {
            // The person's answer ends an agent's wait: it never waits behind
            // other requests.
            Self::Change(Change::Hold | Change::Release | Change::Revoke | Change::Answer) => true,
            Self::Service
            | Self::Read(_)
            | Self::OpenViewer
            | Self::Change(
                Change::Recover
                | Change::Cleanup
                | Change::Input
                | Change::Acquire
                | Change::Renew
                | Change::AddAccount
                | Change::RemoveAccount
                | Change::SetLevel
                | Change::EndAllowed,
            ) => false,
        }
    }
}

/// Answer one request: only from the viewer's own origin and private
/// address, within a free slot, and for a known route.
pub(super) async fn respond(viewer: Arc<Viewer>, request: Request) -> Response {
    let origin = format!("http://{}", viewer.authority);
    if header(request.headers(), "host") != Some(&viewer.authority)
        || (request.method() == Method::POST
            && header(request.headers(), "origin") != Some(&origin))
        || header(request.headers(), "origin").is_some_and(|given| given != origin)
    {
        return refused(403, "Local same-origin access only");
    }

    let Some(path) = request.uri().path().strip_prefix(&viewer.prefix) else {
        return refused(404, "Unknown viewer address");
    };
    let route = Route::parse(request.method(), path);

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

    match route {
        Some(Route::Read(read)) => answer(&viewer, read, request.uri().query()).await,
        Some(Route::OpenViewer) => match read_body(request).await {
            Ok(_) => open_app_viewer(&viewer),
            Err(response) => response,
        },
        Some(Route::Change(change)) => match read_body(request).await {
            Ok(body) => apply(&viewer, change, &body).await,
            Err(response) => response,
        },
        Some(Route::Service) | None => refused(404, "Unknown operation"),
    }
}

/// Ask the app to open its viewer window.
fn open_app_viewer(viewer: &Viewer) -> Response {
    match viewer.send(Line::Viewer) {
        Ok(()) => json_response(200, json!({"ok": true, "viewer": "open"})),
        Err(error) => json_response(409, json!(error)),
    }
}

/// Make `change` in the helper as `body` asks. Absent tokens read as empty,
/// so the lease and frame checks keep their precedence.
async fn apply(viewer: &Arc<Viewer>, change: Change, body: &Value) -> Response {
    // Held to the end of the request, cleanup included, so shutdown waits
    // for it.
    let Some(_changing) = viewer.changing().await else {
        return refused(503, "The viewer is closing");
    };

    let owner = body["lease"].as_str();
    let lease = owner.unwrap_or_default();
    let frame = body["frame"].as_str().unwrap_or_default();
    let outcome = match change {
        Change::Recover => viewer.recover().await,
        Change::Cleanup => viewer.clean_up(body["session"].as_str()).await,
        // Control a viewer holds is left alone either way.
        Change::Hold => match body["on"].as_bool() {
            Some(on) => viewer.client().hold(on).await,
            None => Err(Error::new(Code::Input, "Hold requires a boolean on")),
        },
        Change::Input => viewer.input(lease, frame, &body["actions"]).await,
        Change::Acquire => viewer.acquire(frame).await,
        Change::Renew => viewer.renew(lease, frame).await,
        Change::Release => viewer
            .release(Some(lease))
            .await
            .map(|()| json!({"ok": true})),
        Change::Revoke => viewer.revoke().await,
        Change::AddAccount => super::accounts::add(&viewer.accounts_home(), body),
        Change::RemoveAccount => super::accounts::remove(&viewer.accounts_home(), body),
        Change::SetLevel => super::accounts::set_level(&viewer.accounts_home(), body),
        Change::Answer => super::accounts::answer(&viewer.client(), body),
        Change::EndAllowed => super::accounts::end_allowed(&viewer.client(), body),
    };

    mutation_reply(viewer, owner, outcome).await
}

/// Read one bounded JSON object with the existing last-member interpretation.
async fn read_body(request: Request) -> std::result::Result<Value, Response> {
    if request.headers().contains_key("transfer-encoding")
        || header(request.headers(), "content-type").and_then(|kind| kind.split(';').next())
            != Some("application/json")
    {
        return Err(refused(400, "Send JSON with Content-Length"));
    }

    let length = header(request.headers(), "content-length")
        .and_then(|given| given.parse::<usize>().ok())
        .filter(|length| (1..=MAX_BODY).contains(length))
        .ok_or_else(|| {
            refused(
                400,
                &format!("Request body must contain 1–{MAX_BODY} bytes"),
            )
        })?;

    let body = tokio::time::timeout(
        Duration::from_secs(5),
        to_bytes(request.into_body(), MAX_BODY),
    )
    .await
    .ok()
    .and_then(std::result::Result::ok)
    .filter(|bytes| bytes.len() == length)
    .ok_or_else(|| refused(400, "Incomplete or oversized body"))?;

    match serde_json::from_slice::<Value>(&body) {
        Ok(value @ Value::Object(_)) => Ok(value),
        _ => Err(refused(400, "Request must be a JSON object")),
    }
}

/// What a change's outcome does to the lease its request named.
enum LeaseAfter {
    Kept,
    Ended,
}

/// Map a mutation's outcome and finish only the lease that request named.
async fn mutation_reply(viewer: &Viewer, owner: Option<&str>, outcome: Result<Value>) -> Response {
    let (status, lease, reply) = match outcome {
        Ok(result) if result["ok"] == true => (200, LeaseAfter::Kept, result),
        Ok(result) => (409, LeaseAfter::Ended, result),
        Err(error) => {
            let (status, lease) = match LeaseRefusal::from(error.code) {
                LeaseRefusal::Unread => (400, LeaseAfter::Kept),
                LeaseRefusal::Busy | LeaseRefusal::Lost => (409, LeaseAfter::Kept),
                LeaseRefusal::Unanswered | LeaseRefusal::Failed => (409, LeaseAfter::Ended),
            };
            (status, lease, json!(error))
        }
    };

    // Only the lease this request named ends; a request without one never
    // ends someone else's. The reply says what went wrong already, and a
    // release that fails keeps its own outcome pending for recovery.
    if let (LeaseAfter::Ended, Some(owner)) = (lease, owner) {
        let _ = viewer.release(Some(owner)).await;
    }

    json_response(status, reply)
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
        Read::Asks => json_response(200, super::accounts::asks(&viewer.client())),
        Read::Pipe => match unpoisoned(&viewer.pipe).clone() {
            Some(descriptor) => json_response(200, descriptor),
            None => json_response(
                404,
                json!({"ok": false, "code": "not-offered", "error": "This viewer runs no frame service"}),
            ),
        },
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
            ("X-Display-Width", frame.epoch.display.width.to_string()),
            ("X-Display-Height", frame.epoch.display.height.to_string()),
            (
                "X-Session-Revision",
                format!(
                    "{}:{}:{}:{}:{}:{}:{}:{}",
                    frame.epoch.uid,
                    frame.epoch.capture_pid,
                    frame.epoch.display.width,
                    frame.epoch.display.height,
                    frame.epoch.display.scale,
                    frame.epoch.security_session_id,
                    frame.epoch.display_id,
                    frame.epoch.input_pid
                ),
            ),
        ],
    )
}

/// A refusal in `message`'s words, with status `code`.
fn refused(code: u16, message: &str) -> Response {
    json_response(code, json!({"ok": false, "error": message}))
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
