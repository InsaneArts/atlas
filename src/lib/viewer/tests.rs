//! The viewer's checks: its routes and who may ask them, sharing frames,
//! taking and ending control, following the settings and what each read
//! leaves out.
use super::{
    control::{Control, release_native},
    frames::Capture,
    http::{
        CONCURRENT_REQUESTS, CONNECTIONS, FIRST_REQUEST, accept, bind, header, json_response,
        respond,
    },
    local::{Answer, answering, exchange, parse_response},
    reads::{ACTIVITY_ROWS, account_title, activity_row, listed_apps},
    *,
};
use crate::{config::Config, files, overview::READ_DEADLINE, setup::Title, test_support::SETTINGS};
use axum::{
    body::{Body, to_bytes},
    extract::Request,
    http::Method,
};
use serde_json::json;
use std::{path::Path, time::Duration};
use tokio::{net::TcpListener, sync::watch};

/// A client for a helper that is not there, with controller state
/// private to the test.
fn client(dir: &Path) -> Client {
    crate::test_support::client_in(dir)
}

/// A viewer bound to a free port, for a helper that is not there.
async fn bound(dir: &Path) -> (TcpListener, Arc<Viewer>) {
    bind(client(dir), 0, None).await.unwrap()
}

/// A viewer for a helper that is not there, not answering requests.
async fn unreachable(dir: &Path) -> Arc<Viewer> {
    bound(dir).await.1
}

#[tokio::test]
async fn the_async_status_read_never_parses_the_snapshot_on_the_runtime_worker() {
    let dir = tempfile::tempdir().unwrap();
    let viewer = http::configured(client(dir.path()), "127.0.0.1:0".to_owned(), None);
    let client = viewer.client();
    let output = client.output_dir().unwrap();
    std::fs::create_dir_all(output).unwrap();
    std::fs::write(
        output.join(crate::activity_log::FILE),
        "{\"at\":\"1\",\"operation\":\"input\",\"ok\":true}\n",
    )
    .unwrap();
    let observation = crate::activity_log::snapshot::Observation::watch(output);
    let runtime_worker = std::thread::current().id();

    let status = viewer.status().await;

    assert_eq!(status["overview"]["activity"].as_array().unwrap().len(), 1);
    let threads = observation.threads();
    assert!(!threads.is_empty(), "the cold status read built a snapshot");
    assert!(threads.iter().all(|thread| *thread != runtime_worker));
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

/// `body` posted to `route` of `viewer` as its app posts it.
async fn posted(viewer: &Arc<Viewer>, route: &str, body: &Value) -> (u16, Value) {
    let body = body.to_string();
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("{}{route}", viewer.prefix))
        .header("host", &viewer.authority)
        .header("origin", format!("http://{}", viewer.authority))
        .header("content-type", "application/json")
        .header("content-length", body.len())
        .body(Body::from(body))
        .unwrap();
    let reply = respond(viewer.clone(), request).await;
    let status = reply.status().as_u16();
    let body = to_bytes(reply.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

pub(crate) async fn recover_from_app(client: Client) -> Value {
    let viewer = http::configured(client, "127.0.0.1:0".to_owned(), None);
    let (status, outcome) = posted(&viewer, "recover", &json!({})).await;
    assert_eq!(status, 200, "{outcome}");
    outcome
}

#[tokio::test]
async fn the_cleanup_routes_preview_and_apply_as_the_person() {
    use crate::{
        host_session::{Host, Key},
        task::{Claims, Owned, State, Task, store::Store as Tasks},
    };

    let dir = tempfile::tempdir().unwrap();
    let viewer = unreachable(dir.path()).await;
    let output = viewer.client().output_dir().unwrap().to_owned();
    let session = Key::of(Host::Codex, "left");
    Claims::in_store(Tasks::at(&output), Task::of(&session, None).unwrap())
        .claim(
            &Owned::Launch {
                app: "TextEdit".into(),
            },
            State::Made,
        )
        .unwrap();

    let (status, preview) = read_json(&viewer, &format!("cleanup.json?session={session}")).await;
    assert_eq!(status, 200, "{preview}");
    assert_eq!(preview["preview"], true);
    assert_eq!(
        preview["active"], true,
        "its task claimed something just now"
    );
    assert_eq!(preview["steps"], json!([]));
    assert_eq!(preview["left"][0]["app"], "TextEdit");

    let (status, applied) = posted(&viewer, "cleanup", &json!({"session": session})).await;
    assert_eq!((status, &applied["ok"]), (200, &json!(true)), "{applied}");
    assert_eq!(applied["preview"], false);

    for refused in ["cleanup.json", "cleanup.json?session=../x"] {
        assert_eq!(read_json(&viewer, refused).await.0, 400, "{refused}");
    }
    let nobody = Key::of(Host::Codex, "nobody");
    let (status, unknown) = posted(&viewer, "cleanup", &json!({"session": nobody})).await;
    assert_eq!((status, &unknown["code"]), (400, &json!("input")));
}

/// [`answering`] off the runtime.
async fn answering_off(url: &str) -> Answer {
    let url = url.to_owned();
    tokio::task::spawn_blocking(move || answering(&url))
        .await
        .unwrap()
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
    assert_eq!(
        answering_off(&url).await,
        Answer::Process(std::process::id())
    );

    server.abort();
}

#[tokio::test]
async fn post_local_preserves_the_code_and_uncertainty_of_an_input_refusal() {
    for uncertain in [true, false] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let app = axum::Router::new().route(
            "/input",
            axum::routing::post(move || async move {
                json_response(
                    409,
                    json!({
                        "ok": false,
                        "code": "session-changed",
                        "error": "The session changed during input",
                        "uncertain": uncertain,
                    }),
                )
            }),
        );
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let error =
            tokio::task::spawn_blocking(move || post_local(&url, "input", &json!({"actions": []})))
                .await
                .unwrap()
                .unwrap_err();
        server.abort();

        assert_eq!(error.code, "session-changed");
        assert_eq!(error.error, "The session changed during input");
        assert_eq!(error.uncertain, uncertain);
        let reported = serde_json::to_value(error).unwrap();
        assert_eq!(reported["code"], "session-changed");
        assert_eq!(reported["uncertain"], uncertain);
    }
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
async fn the_person_takes_control_beside_a_running_command() {
    if !crate::test_support::isolated(
        "viewer::tests::the_person_takes_control_beside_a_running_command",
    ) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let viewer = unreachable(dir.path()).await;
    let controller = viewer.client().controller.clone();
    let command = controller.quarantine_command().unwrap();

    viewer.lease.lock().await.acquire(&controller).unwrap();
    assert!(viewer.lease.lock().await.active());
    viewer.lease.lock().await.release();

    let refused = viewer.recover().await.unwrap_err();
    assert_eq!(refused.code, "busy");
    assert_eq!(refused.holder, Some(crate::error::Holder::Agent));
    assert!(refused.error.contains("still running"), "{}", refused.error);
    assert_eq!(
        controller.pending().unwrap(),
        [command.generation().to_owned()],
        "recovery never settles a running command"
    );
    controller.clear_if(&command).unwrap();
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
    let idle = viewer.control(Control::Revoke, &json!({})).await.unwrap();
    assert_eq!(idle, json!({"ok": true, "released": false}));
    viewer.lease.lock().await.acquire(&controller).unwrap();

    // The helper cannot confirm native cleanup, so control ends but the
    // release stays an unknown outcome for recovery.
    let error = viewer
        .control(Control::Revoke, &json!({}))
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
    let owner = files::token();

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
    let check =
        |record: std::path::PathBuf| tokio::task::spawn_blocking(move || running(&record, port));

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
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 11\r\n\r\n{\"ok\":true}");
        }
    });
    assert_eq!(answering_off(&url).await, Answer::Nobody);

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

    assert_eq!(
        answering_off(&url).await,
        Answer::Process(std::process::id())
    );

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

    assert_eq!(
        answering_off(&viewer.url).await,
        Answer::Nobody,
        "all are taken"
    );

    drop(idle);
    assert_eq!(
        answering_off(&viewer.url).await,
        Answer::Process(std::process::id())
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
            (Code::Transport, "The helper did not answer in time")
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
        .send(Some(Err((Code::Capture, "the shared picture".into()))))
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
    old.send(Some(Err((Code::Capture, "an old picture".into()))))
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
    let controller = crate::control::Controller::at(60001, &base.join("state"));
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
    other.controller = crate::control::Controller::at(60001, &dir.path().join("other"));
    *viewer.client.lock().unwrap() = Arc::new(other);

    let error = viewer.release(None).await.unwrap_err();
    assert!(error.uncertain);
    assert_eq!(taken.controller.unresolved().unwrap().len(), 1);
    assert!(viewer.client().controller.pending().unwrap().is_empty());
    assert!(viewer.lease_client.lock().unwrap().is_none());
}

#[tokio::test]
async fn giving_control_back_waits_for_the_final_input_recording() {
    let dir = tempfile::tempdir().unwrap();
    let viewer = http::configured(client(dir.path()), "127.0.0.1:1".into(), None);
    let client = viewer.client();
    let recording = viewer.input_recording.lock().await;
    let giving_back = viewer.release(None);
    tokio::pin!(giving_back);
    std::future::poll_fn(|cx| {
        assert!(std::future::Future::poll(giving_back.as_mut(), cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    client.record_for_test(
        &json!({"op": "input", "actions": [{"type": "click"}]}),
        &json!({"ok": true}),
    );
    assert!(crate::activity_log::recent(client.output_dir().unwrap(), 1).is_empty());
    drop(recording);
    giving_back.await.unwrap();
    assert_eq!(
        crate::activity_log::recent(client.output_dir().unwrap(), 1)[0].action_count,
        Some(1)
    );
}

#[tokio::test]
async fn a_failed_release_keeps_the_recording_failure_and_pending_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let viewer = http::configured(client(directory.path()), "127.0.0.1:1".into(), None);
    let client = viewer.client();
    viewer
        .lease
        .lock()
        .await
        .acquire(&client.controller)
        .unwrap();
    client.record_for_test(
        &json!({"op": "input", "actions": [{"type": "text", "text": "private-input-canary"}]}),
        &json!({"ok": true}),
    );
    let output = client.output_dir().unwrap();
    let obstruction = output.join(crate::activity_log::LOCK);
    std::fs::create_dir(&obstruction).unwrap();

    let error = tokio::time::timeout(Duration::from_secs(5), viewer.release(None))
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, Code::Uncertain);
    assert!(error.uncertain);
    assert!(
        error.error.contains("native cleanup was not confirmed"),
        "{error}"
    );
    assert!(
        error.error.contains("recording released input also failed"),
        "{error}"
    );
    assert!(
        error.error.contains(&output.display().to_string()),
        "{error}"
    );
    let public = serde_json::to_string(&error).unwrap();
    assert!(!public.contains("private-input-canary"));
    assert_eq!(client.controller.unresolved().unwrap().len(), 1);
    assert!(!viewer.lease.lock().await.active());

    std::fs::remove_dir(obstruction).unwrap();
    client.flush_person_input().await.unwrap();
    assert_eq!(crate::activity_log::recent(output, 1).len(), 1);
    assert_eq!(client.controller.unresolved().unwrap().len(), 1);
}

#[tokio::test]
async fn shutdown_flushes_buffered_input_even_without_a_control_token() {
    let dir = tempfile::tempdir().unwrap();
    let viewer = http::configured(client(dir.path()), "127.0.0.1:1".into(), None);
    let client = viewer.client();
    client.record_for_test(
        &json!({"op": "input", "actions": [{"type": "click"}]}),
        &json!({"ok": true}),
    );
    viewer.close().await;
    viewer.release(None).await.unwrap();
    assert_eq!(
        crate::activity_log::recent(client.output_dir().unwrap(), 1)[0].action_count,
        Some(1)
    );
}

#[tokio::test]
async fn the_persons_input_is_logged_once_they_give_control_back() {
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
    let output = taken.output_dir().unwrap().to_path_buf();
    let clicked = json!({"op": "input", "actions": [{"type": "click", "x": 1, "y": 2}]});
    for _ in 0..4 {
        taken.record_for_test(&clicked, &json!({"ok": true}));
    }
    assert!(crate::activity_log::recent(&output, 5).is_empty());

    let _ = viewer.release(None).await;
    let rows = crate::activity_log::recent(&output, 5);
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].actor(), rows[0].action_count), ("viewer", Some(4)));
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

    assert_eq!(answering_off(&url).await, Answer::Older);
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

        std::thread::sleep(
            (FIRST_REQUEST + Duration::from_millis(500)).saturating_sub(started.elapsed()),
        );
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
        "sessions.json",
        "screenshot.png?name=1-capture.png",
    ];
    let own = format!("http://{}", viewer.authority);

    for route in routes {
        let path = format!("{}{route}", viewer.prefix);
        let elsewhere = Some("http://evil.example");
        let (status, ..) = asked(&viewer, Method::GET, &path, &viewer.authority, elsewhere).await;
        assert_eq!(status, 403, "{route} from another origin");
        let (status, ..) = asked(&viewer, Method::GET, &path, "localhost", None).await;
        assert_eq!(status, 403, "{route} at another host");

        let bare = format!("/{route}");
        let (status, _, body) = asked(&viewer, Method::GET, &bare, &viewer.authority, None).await;
        assert_eq!(status, 404, "{route} without the private address");
        assert!(String::from_utf8_lossy(&body).contains("Unknown viewer address"));

        let (status, _, body) =
            asked(&viewer, Method::POST, &path, &viewer.authority, Some(&own)).await;
        assert_eq!(status, 404, "{route} only reads");
        assert!(String::from_utf8_lossy(&body).contains("Unknown operation"));
    }

    assert_eq!(read_json(&viewer, "activity.json").await.0, 200);
    assert_eq!(read_json(&viewer, "screenshots.json").await.0, 200);
    assert_eq!(read_json(&viewer, "sessions.json").await.0, 200);
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
async fn activity_json_reports_an_unreadable_log_and_recovers_after_it_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let viewer = http::configured(client(dir.path()), "127.0.0.1:0".to_owned(), None);
    let output = viewer.client().output_dir().unwrap().to_path_buf();
    let path = output.join(crate::activity_log::FILE);

    let (status, reply) = read_json(&viewer, "activity.json").await;
    assert_eq!((status, reply["ok"].clone()), (200, json!(true)));
    assert_eq!(reply["activity"], json!([]));
    std::fs::create_dir(&path).unwrap();
    for _ in 0..2 {
        let (status, reply) = read_json(&viewer, "activity.json").await;
        assert_eq!(status, 503);
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["code"], "io");
        assert_eq!(reply["error"], "The activity log could not be read.");
        assert_eq!(reply["uncertain"], false);
        assert!(reply.get("activity").is_none());
    }
    assert!(crate::activity_log::recent(&output, ACTIVITY_ROWS).is_empty());
    let (status, reply) = read_json(&viewer, "status.json").await;
    assert_eq!(status, 200);
    assert_eq!(reply["overview"]["activity"], json!([]));
    std::fs::remove_dir(path).unwrap();
    std::fs::write(
        output.join(crate::activity_log::PREVIOUS),
        "bad row\n{}\n{\"at\":\"1\",\"operation\":\"input\",\"ok\":true}\n",
    )
    .unwrap();
    let (status, reply) = read_json(&viewer, "activity.json").await;
    assert_eq!((status, reply["ok"].clone()), (200, json!(true)));
    assert_eq!(reply["activity"].as_array().unwrap().len(), 1);
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
    std::fs::write(output.join(crate::activity_log::FILE), format!("{row}\n")).unwrap();

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
            "app",
            "bundleId",
            "capture",
            "count",
            "id",
            "keys",
            "label",
            "logicalId",
            "nothingSent",
            "ok",
            "operation",
            "partial",
            "reason",
            "says",
            "session",
            "started",
            "startedAt",
            "uncertain",
            "verb"
        ]
    );
    assert_eq!(listed["says"], "typed in TextEdit");
    assert_eq!(listed["count"], 2);
    assert_eq!(listed["ageSeconds"], 3);
    assert_eq!(listed["session"], Value::Null);
    assert_eq!(reply["sessions"], json!({}));

    // A row of an agent's host session names it, and nothing more.
    let session =
        crate::host_session::HostSession::new(crate::host_session::Host::Claude, "left").unwrap();
    let mut row = row;
    row["session"] = json!(session.key());
    std::fs::write(output.join(crate::activity_log::FILE), format!("{row}\n")).unwrap();
    crate::host_session::store::note(
        &viewer.client(),
        &session,
        crate::host_session::store::Kind::Other,
        Some(&crate::doing::Doing::new("type")),
        &Ok(json!({"ok": true})),
    )
    .unwrap();
    let (_, reply) = read_json(&viewer, "activity.json").await;
    let text = reply.to_string();
    assert!(
        !text.contains("hunter2") && !text.contains("\"left\""),
        "{text}"
    );
    assert_eq!(reply["activity"][0]["session"], session.key().as_str());
}

#[tokio::test]
async fn activity_json_names_the_sessions_its_rows_mention() {
    let dir = tempfile::tempdir().unwrap();
    let viewer = unreachable(dir.path()).await;
    let client = viewer.client();
    let output = client.output_dir().unwrap().to_path_buf();
    let (named, gone, unmentioned) = (
        crate::host_session::HostSession::new(crate::host_session::Host::Codex, "right").unwrap(),
        crate::host_session::HostSession::new(crate::host_session::Host::Pi, "gone").unwrap(),
        crate::host_session::HostSession::new(crate::host_session::Host::Claude, "quiet").unwrap(),
    );
    for session in [&named, &unmentioned] {
        crate::host_session::store::note(
            &client,
            session,
            crate::host_session::store::Kind::Other,
            Some(&crate::doing::Doing::new("click")),
            &Ok(json!({"ok": true})),
        )
        .unwrap();
    }
    let at = crate::client::timestamp();
    let rows: String = [&named, &gone]
        .iter()
        .enumerate()
        .map(|(n, session)| {
            format!(
                "{}\n",
                json!({"at": (at - n as u128).to_string(), "operation": "input", "ok": true,
                    "actor": "agent", "verb": "click", "command": format!("{n:012}"),
                    "session": session.key(), "capture": null})
            )
        })
        .collect();
    std::fs::write(output.join(crate::activity_log::FILE), rows).unwrap();

    let (_, reply) = read_json(&viewer, "activity.json").await;
    let sessions = reply["sessions"].as_object().unwrap();
    assert_eq!(sessions.keys().collect::<Vec<_>>(), [named.key().as_str()]);
    let entry = &sessions[named.key().as_str()];
    assert_eq!(entry["host"], "codex");
    assert_eq!(entry["hostLabel"], "Codex");
    assert!(entry["name"].as_str().unwrap().starts_with("Codex"));
    assert!(entry["started"].is_u64() && entry["ended"].is_null());
    assert!(entry.get("hostSession").is_none());
}

#[tokio::test]
async fn activity_json_lists_the_newest_200_rows_by_ids_that_hold_across_reads() {
    let dir = tempfile::tempdir().unwrap();
    let viewer = unreachable(dir.path()).await;
    let output = viewer.client().output_dir().unwrap().to_path_buf();
    let log = (0..250)
        .map(|at| {
            let row = json!({"at": format!("{}", 1_000_000 + at), "operation": "input",
                "ok": true, "actionCount": 1, "actor": "agent", "elapsedMs": 2.5});
            format!("{row}\n")
        })
        .collect::<String>();
    std::fs::write(output.join(crate::activity_log::FILE), log).unwrap();

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
        .open(output.join(crate::activity_log::FILE))
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
            "pid": 3, "name": "Opie", "bundle": "/Applications/Opie.app", "bundleId": null,
            "front": true, "hidden": false, "launchedAt": null,
            "windows": [
                {"id": 12, "title": "Ring", "x": 10.0, "y": 40.0, "width": 520.0, "height": 860.0,
                    "order": 1},
                {"id": 7, "title": "Starter", "x": 10.0, "y": 40.0, "width": 520.0, "height": 860.0,
                    "order": 2},
            ],
        })
    );
    assert_eq!(listed[2]["windows"], json!([]), "Finder has none");
    assert_eq!(listed[2]["front"], false);
}

#[test]
fn apps_name_their_bundle_id_whether_hidden_when_launched_and_their_windows_order() {
    let apps = json!([
        {"name": "Notes", "pid": 4, "policy": "regular", "hidden": true,
            "bundleId": "com.apple.Notes", "launchedAt": 1_800_000_000_000_u64,
            "executable": "/System/Applications/Notes.app/Contents/MacOS/Notes"},
        {"name": "Probe", "pid": 5, "policy": "regular", "bundleId": "not an id",
            "executable": "/Users/helper/Applications/Probe.app/Contents/MacOS/Probe"},
    ]);
    let window = |number: i64, pid: i64| {
        json!({"number": number, "pid": pid, "title": "Secret plans", "layer": 0,
            "bounds": {"x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0}})
    };
    let windows = json!([window(9, 5), window(8, 4), window(7, 5)]);

    let listed = listed_apps(&apps, &windows).unwrap();
    assert_eq!(listed[0]["name"], "Probe");
    assert_eq!(listed[0]["bundleId"], Value::Null, "only an identifier");
    assert_eq!(listed[0]["hidden"], false);
    let orders = |app: &Value| {
        app["windows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|window| window["order"].clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(orders(&listed[0]), [json!(0), json!(2)]);
    assert_eq!(listed[1]["bundleId"], "com.apple.Notes");
    assert_eq!(listed[1]["hidden"], true);
    assert_eq!(listed[1]["launchedAt"], 1_800_000_000_000_u64);
    assert_eq!(
        orders(&listed[1]),
        [json!(1)],
        "behind Probe's front window"
    );
}

#[test]
fn an_activity_row_as_the_app_lists_it_holds_exactly_these_fields() {
    let dir = tempfile::tempdir().unwrap();
    let line = r#"{"at":"90000000000","operation":"input","ok":true,"verb":"key","app":"TextEdit","bundleId":"com.apple.TextEdit","label":"⇧⌘Z Page Up","actionCount":2,"actor":"agent","command":"c1","text":"hunter2"}"#;
    std::fs::write(
        dir.path().join(crate::activity_log::FILE),
        format!("{line}\n"),
    )
    .unwrap();
    let rows = crate::activity_log::recent(dir.path(), 5);
    let shown = activity_row(&rows[0], 100_000_000_000);
    let keys = shown.as_object().unwrap().keys().collect::<Vec<_>>();
    assert_eq!(
        keys,
        [
            "actor",
            "ageSeconds",
            "app",
            "bundleId",
            "capture",
            "count",
            "id",
            "keys",
            "label",
            "logicalId",
            "nothingSent",
            "ok",
            "operation",
            "partial",
            "reason",
            "says",
            "session",
            "started",
            "startedAt",
            "uncertain",
            "verb"
        ],
        "a field added here is added on purpose"
    );
    assert_eq!(shown["app"], "TextEdit");
    assert_eq!(shown["bundleId"], "com.apple.TextEdit");
    assert_eq!(shown["label"], "⇧⌘Z Page Up");
    assert_eq!(
        shown["logicalId"], "c1",
        "a command's steps share its token"
    );
    assert_eq!(shown["keys"], json!([["⇧", "⌘", "Z"], ["Page Up"]]));
    assert!(!shown.to_string().contains("hunter2"));
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
    assert!(crate::activity_log::recent(&output, 5).is_empty());

    // The same client records what acts, even when no helper answers.
    assert!(
        agent
            .capture(crate::client::Size::Standard, None, false)
            .await
            .is_err()
    );
    let rows = crate::activity_log::recent(&output, 5);
    assert_eq!(rows.len(), 1);
    assert!(
        crate::overview::shown(&rows[0], 0).reason.is_some(),
        "a failed capture says why: {:?}",
        rows[0]
    );
}

#[test]
fn the_apps_activity_row_keeps_its_fields_and_values() {
    let dir = tempfile::tempdir().unwrap();
    let line = r#"{"at":"90000000000","operation":"input","ok":false,"code":"focus-changed","reason":"window","partial":true,"verb":"click","app":"Opie","label":"Save","session":"0123456789ab","capture":"1-capture.png","actionCount":1,"actor":"agent","command":"c1"}"#;
    std::fs::write(
        dir.path().join(crate::activity_log::FILE),
        format!("{line}\n{{\"at\":\"1\",\"operation\":\"run\"}}\n"),
    )
    .unwrap();
    let rows = crate::activity_log::recent(dir.path(), 5);
    let shown: Vec<Value> = rows
        .iter()
        .map(|row| activity_row(row, 100_000_000_000))
        .collect();
    // AtlasService decodes this same fixture, so producer and consumer cannot
    // silently drift while asserting separate copies of the wire contract.
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/activity-projection.json"
    ))
    .unwrap();
    assert_eq!(json!({"activity": shown}), fixture);
}

#[test]
fn a_commands_logical_id_holds_from_start_to_end_and_an_input_runs_holds_as_it_grows() {
    use crate::activity_log::{FILE, Row, recent};

    let dir = tempfile::tempdir().unwrap();
    let start = json!({"at": "1000000000", "operation": "run", "started": true,
        "command": "0123456789ab"});
    let input = json!({"at": "2000000000", "operation": "input", "actor": "viewer",
        "ok": true, "actionCount": 2});
    let end = json!({"at": "3000000000", "operation": "run", "ok": true,
        "command": "0123456789ab"});
    let more_input = json!({"at": "4000000000", "operation": "input", "actor": "viewer",
        "ok": true, "actionCount": 3});
    let write = |rows: &[Value]| {
        std::fs::write(
            dir.path().join(FILE),
            rows.iter()
                .map(|row| format!("{row}\n"))
                .collect::<String>(),
        )
        .unwrap();
    };
    let shown = || activity_row(&recent(dir.path(), 1)[0], 5_000_000_000);

    write(std::slice::from_ref(&start));
    let before = shown();
    write(&[start, end]);
    let completed = shown();
    assert_eq!(before["logicalId"], "0123456789ab");
    assert_eq!(completed["logicalId"], before["logicalId"]);
    assert_ne!(completed["id"], before["id"]);

    write(std::slice::from_ref(&input));
    let before = shown();
    // The run keeps its first identity even across a log rotation.
    std::fs::rename(
        dir.path().join(FILE),
        dir.path().join(crate::activity_log::PREVIOUS),
    )
    .unwrap();
    write(&[more_input]);
    let grown = shown();
    assert_eq!(before["logicalId"], input["at"]);
    assert_eq!(grown["logicalId"], before["logicalId"]);
    assert_ne!(grown["id"], before["id"]);
    assert_eq!(grown["count"], 5);
    let serialized = serde_json::to_value(&recent(dir.path(), 1)[0]).unwrap();
    assert!(serialized.get("firstInputId").is_none());
    assert!(serialized.get("logicalId").is_none());
    assert!(serialized.get("id").is_none());
    let mut spoofed = input;
    spoofed["firstInputId"] = json!("untrusted");
    assert_eq!(Row::parse(&spoofed).unwrap().logical_id(), "2000000000");
    let standalone = Row::parse(&json!({"operation": "read"})).unwrap();
    assert_eq!(activity_row(&standalone, 0)["logicalId"], standalone.id);
}

#[tokio::test]
async fn sessions_json_never_carries_ids_paths_or_tokens() {
    use crate::host_session::{Host, HostSession, store};

    let dir = tempfile::tempdir().unwrap();
    let viewer = unreachable(dir.path()).await;
    let client = viewer.client();
    let output = client.output_dir().unwrap().to_path_buf();
    let shot = "1700000005000000000-capture.png";
    std::fs::write(
        output.join(shot),
        crate::native::capture::sample_png(64, 40),
    )
    .unwrap();
    let session = HostSession::new(Host::Claude, "secret-host-id").unwrap();
    store::note(
        &client,
        &session,
        store::Kind::Screenshot,
        Some(&crate::doing::Doing::new("screenshot")),
        &Ok(json!({"ok": true, "path": output.join(shot)})),
    )
    .unwrap();
    store::note(
        &client,
        &session,
        store::Kind::Open,
        Some(&crate::doing::Doing::new("open").app(Some("Notes"))),
        &Ok(json!({"ok": true})),
    )
    .unwrap();
    let at = crate::client::timestamp();
    let row = json!({"at": at.to_string(), "operation": "open", "ok": true, "actor": "agent",
        "verb": "open", "app": "Notes", "command": "000000000001", "session": session.key()});
    std::fs::write(output.join(crate::activity_log::FILE), format!("{row}\n")).unwrap();

    let (status, reply) = read_json(&viewer, "sessions.json").await;
    assert_eq!(status, 200);
    let live = reply["live"].as_array().unwrap();
    assert_eq!(live.len(), 1, "every live session, as the status lists it");
    assert_eq!(live[0]["key"], session.key().as_str());
    assert_eq!(live[0]["state"], "idle");
    let listed = reply["sessions"].as_array().unwrap();
    assert_eq!(listed.len(), 1);
    let entry = &listed[0];
    assert_eq!(entry["key"], session.key().as_str());
    assert_eq!(entry["host"], "claude");
    assert_eq!(entry["state"], "idle");
    assert_eq!(entry["endedAt"], Value::Null);
    assert_eq!(entry["actions"], 2);
    assert_eq!(entry["apps"], json!([{"name": "Notes", "copied": false}]));
    assert_eq!(entry["screenshots"], json!([shot]));
    let text = reply.to_string();
    for hidden in [
        "secret-host-id",
        output.to_str().unwrap(),
        "hostSession",
        "planId",
    ] {
        assert!(!text.contains(hidden), "{hidden} in {text}");
    }
}

#[test]
fn a_quiet_session_has_no_known_end_and_is_listed_for_seven_days() {
    use crate::host_session::{Host, HostSession, record::Record};

    let now = 1_793_170_000_000_u64;
    let hour = 60 * 60 * 1000;
    let record = |id: &str, ago: u64| {
        let session = HostSession::new(Host::Codex, id).unwrap();
        let mut record = Record::new(&session, now - ago, Some("opie".into()));
        record.host_process = None;
        record
    };
    let mut working = record("now", 60_000);
    for (phase, at) in [
        (crate::environment::Phase::RollingBack, 3),
        (crate::environment::Phase::Applied, 2),
        (crate::environment::Phase::Applying, 1),
    ] {
        working.planned(crate::files::Token::new(), phase, at);
    }
    let records = [
        working,
        record("quiet", 2 * hour),
        record("week", 8 * 24 * hour),
    ];
    let history = crate::session_view::HistorySource {
        records: &records,
        rows: &[],
        captures: &Default::default(),
        copies: &Default::default(),
        tasks: &[],
        now,
    }
    .history();

    let states: Vec<_> = history
        .iter()
        .map(|past| json!(past.state).as_str().unwrap().to_owned())
        .collect();
    assert_eq!(states, ["idle", "idle"], "a week and more is gone");
    assert_eq!(history[1].ended_at, None, "it only went quiet");
    let plans: Vec<_> = history[0]
        .plans
        .iter()
        .map(|plan| (json!(plan.phase), plan.can_roll_back))
        .collect();
    assert_eq!(
        plans,
        [
            (json!("applying"), true),
            (json!("applied"), true),
            (json!("rollingBack"), false),
        ]
    );
}

#[tokio::test]
async fn screenshots_json_names_each_shots_session() {
    let dir = tempfile::tempdir().unwrap();
    let viewer = unreachable(dir.path()).await;
    let client = viewer.client();
    let output = client.output_dir().unwrap().to_path_buf();
    let (shot, other) = (
        "1700000005000000000-capture.png",
        "1700000000000000000-capture.png",
    );
    for name in [shot, other] {
        std::fs::write(
            output.join(name),
            crate::native::capture::sample_png(64, 40),
        )
        .unwrap();
    }
    let session =
        crate::host_session::HostSession::new(crate::host_session::Host::Claude, "left").unwrap();
    crate::host_session::store::note(
        &client,
        &session,
        crate::host_session::store::Kind::Screenshot,
        Some(&crate::doing::Doing::new("screenshot")),
        &Ok(json!({"ok": true, "path": output.join(shot)})),
    )
    .unwrap();

    let (_, listed) = read_json(&viewer, "screenshots.json").await;
    let shots = listed["screenshots"].as_array().unwrap();
    assert_eq!(shots[0]["session"], session.key().as_str());
    assert!(
        shots[1].get("session").is_none(),
        "nobody's record lists it"
    );
    let sessions = listed["sessions"].as_object().unwrap();
    assert!(
        sessions[session.key().as_str()]["name"]
            .as_str()
            .unwrap()
            .starts_with("Claude Code")
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
        json!({"ok": true, "folder": output, "sessions": {}, "screenshots": [
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

#[test]
fn screenshot_rows_keep_the_folder_they_were_listed_from() {
    use std::future::{Future, poll_fn};

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    runtime.block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let viewer = unreachable(dir.path()).await;
        let original = viewer.client();
        let output = original.output_dir().unwrap().to_path_buf();
        std::fs::write(output.join("1-capture.png"), b"old capture").unwrap();

        // Hold the only blocking worker so settings can change after
        // listing has captured its client but before it reads that folder.
        let (release, held) = std::sync::mpsc::channel();
        let (started, ready) = tokio::sync::oneshot::channel();
        let worker = tokio::task::spawn_blocking(move || {
            let _ = started.send(());
            let _ = held.recv_timeout(Duration::from_secs(5));
        });
        ready.await.unwrap();
        let mut listing = Box::pin(viewer.screenshots());
        poll_fn(|context| {
            assert!(listing.as_mut().poll(context).is_pending());
            std::task::Poll::Ready(())
        })
        .await;

        let text = SETTINGS.replace("\"out\"", "\"moved\"").into_bytes();
        let config = Config::parse(&text, &dir.path().join("u.toml")).unwrap();
        viewer.follow_output(&original, config, text).await;
        assert_ne!(
            viewer.client().config.output_dir,
            original.config.output_dir
        );
        release.send(()).unwrap();
        worker.await.unwrap();

        let listed = listing.await.unwrap();
        assert_eq!(listed["folder"], json!(output));
        assert_eq!(
            listed["screenshots"],
            json!([{"name": "1-capture.png", "takenAt": 0}])
        );
    });
}

#[tokio::test]
async fn an_account_title_is_asked_again_after_no_answer_and_kept_once_answered() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static ASKED: AtomicUsize = AtomicUsize::new(0);
    fn ask(_: &str) -> Title {
        match ASKED.fetch_add(1, Ordering::SeqCst) {
            0 => Title::Unknown,
            1 => Title::Titled("Atlas".to_owned()),
            _ => Title::Titled("Not asked".to_owned()),
        }
    }
    let known = std::sync::Mutex::new(None);

    assert_eq!(account_title(&known, "atlas", ask).await, None);
    assert_eq!(
        account_title(&known, "atlas", ask).await.as_deref(),
        Some("Atlas")
    );
    assert_eq!(
        account_title(&known, "atlas", ask).await.as_deref(),
        Some("Atlas")
    );
    assert_eq!(ASKED.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn an_account_with_no_full_name_is_asked_once() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static ASKED: AtomicUsize = AtomicUsize::new(0);
    fn ask(_: &str) -> Title {
        ASKED.fetch_add(1, Ordering::SeqCst);
        Title::Untitled
    }
    let known = std::sync::Mutex::new(None);

    assert_eq!(account_title(&known, "atlas", ask).await, None);
    assert_eq!(account_title(&known, "atlas", ask).await, None);
    assert_eq!(ASKED.load(Ordering::SeqCst), 1);
}
