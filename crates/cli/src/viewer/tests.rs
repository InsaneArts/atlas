//! The viewer's checks: its routes and who may ask them, sharing frames,
//! taking and ending control, following the settings and what each read
//! leaves out.
use super::{
    control::release_native,
    frames::Capture,
    http::{
        CONCURRENT_REQUESTS, CONNECTIONS, FIRST_REQUEST, accept, bind, header, json_response,
        respond,
    },
    local::{answering, exchange, reply_status},
    reads::{ACTIVITY_ROWS, activity_row},
    *,
};
use atlas_client::overview::READ_DEADLINE;
use atlas_core::{config::Config, files, test_support::SETTINGS};
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
    atlas_client::test_support::client_in(dir)
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
        output.join(atlas_client::activity_log::FILE),
        "{\"at\":\"1\",\"operation\":\"input\",\"ok\":true}\n",
    )
    .unwrap();
    let observation = atlas_client::activity_log::snapshot::Observation::watch(output);
    let runtime_worker = std::thread::current().id();

    let status = viewer.status().await;

    assert_eq!(status["ready"], false);
    assert_eq!(status["overview"]["state"], "notReady");
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
    posted_raw(viewer, route, &body.to_string()).await
}

async fn posted_raw(viewer: &Arc<Viewer>, route: &str, body: &str) -> (u16, Value) {
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("{}{route}", viewer.prefix))
        .header("host", &viewer.authority)
        .header("origin", format!("http://{}", viewer.authority))
        .header("content-type", "application/json")
        .header("content-length", body.len())
        .body(Body::from(body.to_owned()))
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
    use atlas_client::task::{Owned, State, Task, store::Store as Tasks};
    use atlas_core::host::{Host, Key};

    let dir = tempfile::tempdir().unwrap();
    let viewer = unreachable(dir.path()).await;
    let output = viewer.client().output_dir().unwrap().to_owned();
    let session = Key::of(Host::Codex, "left");
    atlas_client::test_support::claim_in(
        Tasks::at(&output),
        Task::of(&session, None).unwrap(),
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
async fn answering_off(url: &str) -> Option<u32> {
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
async fn stop_fields_use_the_last_boolean_and_invalid_values_leave_the_stop_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let viewer = http::configured(client(dir.path()), "127.0.0.1:0".into(), None);
    let client = viewer.client();
    tokio::time::timeout(Duration::from_secs(5), async {
        let (status, reply) = posted(&viewer, "control/hold", &json!({"on": true})).await;
        assert_eq!(status, 200, "{reply}");
        assert_eq!(reply["held"], true);
        assert!(client.controller.stop_noted());
        let marker = std::fs::read_dir(dir.path().join("state"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "stopped")
            })
            .unwrap();
        std::fs::write(&marker, b"stop").unwrap();

        for stopped in [true, false] {
            let body = format!(r#"{{"on":{stopped},"other":1,"other":2}}"#);
            let (status, reply) = posted_raw(&viewer, "control/hold", &body).await;
            assert_eq!(status, 200, "{reply}");
            assert_eq!(reply["held"], stopped);
            if stopped {
                std::fs::write(&marker, b"stop").unwrap();
            }
            for body in [
                r#"{"on":true,"on":false}"#,
                r#"{"on":false,"on":true}"#,
                r#"{"on":true,"on":true}"#,
                r#"{"on":false,"on":false}"#,
                r#"{"on":true,"\u006fn":false}"#,
                r#"{"\u006f\u006e":false,"on":true}"#,
                r#"{"on":null,"on":true}"#,
                r#"{"on":false,"on":null}"#,
                r#"{}"#,
                r#"{"on":null}"#,
                r#"{"on":"false"}"#,
                r#"{"on":0}"#,
                r#"{"on":[]}"#,
                r#"{"on":{}}"#,
            ] {
                let (status, reply) = posted_raw(&viewer, "control/hold", body).await;
                let Ok(wire) = serde_json::from_str::<serde_json::Value>(body) else {
                    panic!("invalid stop fixture");
                };
                if let Some(on) = wire["on"].as_bool() {
                    assert_eq!(status, 200, "{body}: {reply}");
                    assert_eq!(reply["held"], on, "{body}");
                    assert_eq!(client.controller.stop_noted(), on, "{body}");
                    let (status, reply) =
                        posted(&viewer, "control/hold", &json!({"on": stopped})).await;
                    assert_eq!(status, 200, "{reply}");
                    if stopped {
                        assert!(std::fs::write(&marker, b"stop").is_ok());
                    }
                    continue;
                }
                assert_eq!(status, 400, "{body}: {reply}");
                assert_eq!(reply["code"], "input");
                assert_eq!(reply["error"], "Hold requires a boolean on");
                assert_eq!(reply["uncertain"], false);
                assert_eq!(client.controller.stop_noted(), stopped, "{body}");
                if stopped {
                    assert_eq!(std::fs::read(&marker).unwrap(), b"stop", "{body}");
                } else {
                    assert!(!marker.exists(), "{body}");
                }
            }
        }
    })
    .await
    .expect("hold requests finish within five seconds");
}

#[tokio::test]
async fn stop_validation_preserves_other_body_errors_and_routes() {
    let dir = tempfile::tempdir().unwrap();
    let viewer = http::configured(client(dir.path()), "127.0.0.1:0".into(), None);
    for body in ["null", "[]", r#"{"on":true"#] {
        let (status, reply) = posted_raw(&viewer, "control/hold", body).await;
        assert_eq!(status, 400);
        assert_eq!(
            reply,
            json!({"ok":false,"error":"Request must be a JSON object"})
        );
        assert!(!viewer.client().controller.stop_noted());
    }

    let (lines, mut received) = tokio::sync::mpsc::unbounded_channel();
    *viewer.app.lock().unwrap() = Some(lines);
    let (status, reply) = posted_raw(&viewer, "app/viewer", r#"{"on":true,"on":false}"#).await;
    assert_eq!(status, 200);
    assert_eq!(reply, json!({"ok":true,"viewer":"open"}));
    assert!(matches!(
        received.try_recv().unwrap(),
        crate::menubar::Line::Viewer
    ));
    assert!(!viewer.client().controller.stop_noted());
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
    if !atlas_core::test_support::isolated(
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
    assert_eq!(refused.holder, Some(atlas_core::error::Holder::Agent));
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
    let idle = viewer.revoke().await.unwrap();
    assert_eq!(idle, json!({"ok": true, "released": false}));
    viewer.lease.lock().await.acquire(&controller).unwrap();

    // The helper cannot confirm native cleanup, so control ends but the
    // release stays an unknown outcome for recovery.
    let error = viewer.revoke().await.unwrap_err();
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
        move || {
            exchange(
                &url,
                "GET",
                "status.json",
                None,
                Duration::from_secs(5),
                1 << 16,
            )
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(status.status, 503, "reads wait for a slot");

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
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let dir = tempfile::tempdir().unwrap();
    let (viewer, server) = served(dir.path(), 4).await;
    let address: std::net::SocketAddr = viewer.authority.parse().unwrap();
    let held = tokio::time::timeout(Duration::from_secs(5), async {
        let mut held = Vec::new();
        for _ in 0..4 {
            let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
            let request = format!(
                "GET {} HTTP/1.1\r\nHost: {}\r\n\r\n",
                viewer.prefix, viewer.authority
            );
            stream.write_all(request.as_bytes()).await.unwrap();
            let mut response = Vec::new();
            loop {
                let mut chunk = [0; 4096];
                let read = stream.read(&mut chunk).await.unwrap();
                assert_ne!(read, 0, "the accepted connection stays open");
                response.extend_from_slice(&chunk[..read]);
                if let Some(status) = reply_status(&response) {
                    assert_eq!(status, 200, "the connection reached its private route");
                    break;
                }
            }
            held.push(stream);
        }
        held
    })
    .await
    .expect("all four connections were admitted before probing capacity");

    assert_eq!(answering_off(&viewer.url).await, None, "all are taken");

    drop(held);
    assert_eq!(answering_off(&viewer.url).await, Some(std::process::id()));

    server.abort();
}

/// Requests asking for a frame while one is taken share it, and a
/// helper that never answers ends the capture before the app gives up.
/// The status read ends in time too.
#[tokio::test]
async fn frames_asked_together_share_one_capture_that_ends_in_time() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncBufReadExt, BufReader};

    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("s.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let (accepted, mut connections) = tokio::sync::mpsc::unbounded_channel();
    let accepts = Arc::new(AtomicUsize::new(0));
    let observed = accepts.clone();
    let server = tokio::spawn(async move {
        let mut held = Vec::new();

        while let Ok((stream, _)) = listener.accept().await {
            observed.fetch_add(1, Ordering::SeqCst);
            let mut stream = BufReader::new(stream);
            let mut request = String::new();
            assert_ne!(stream.read_line(&mut request).await.unwrap(), 0);
            assert!(request.ends_with('\n'), "the request was dispatched whole");
            held.push(stream);
            let _ = accepted.send(());
        }
    });

    let mut client = client(dir.path());
    client.transport.path = socket;
    client.transport.peer_uid = nix::unistd::getuid().as_raw();
    let (_, viewer) = bind(client, 0, None).await.unwrap();

    let wall_started = std::time::Instant::now();
    let started = tokio::time::Instant::now();
    let requests = async { tokio::join!(viewer.frame(), viewer.frame(), viewer.status()) };
    tokio::pin!(requests);
    let mut asked = 0;
    tokio::time::timeout(READ_DEADLINE, async {
        for _ in 0..2 {
            tokio::select! {
                _ = &mut requests => panic!("the unanswered requests ended before dispatch"),
                accepted = connections.recv() => {
                    assert!(accepted.is_some(), "the fixture still accepts requests");
                    asked += 1;
                }
            }
        }
    })
    .await
    .expect("the capture and status requests reached the service");
    tokio::time::pause();
    let (first, second, status) = requests.await;
    assert!(started.elapsed() < Duration::from_secs(7));
    assert!(
        wall_started.elapsed() < Duration::from_secs(7),
        "the real status work also retains its wall-time deadline"
    );
    assert_ne!(status["ready"], true);

    for outcome in [first, second] {
        let error = outcome.err().unwrap();
        assert_eq!(
            (error.code, error.error.as_str()),
            (Code::Transport, "The helper did not answer in time")
        );
    }

    while connections.try_recv().is_ok() {
        asked += 1;
    }

    server.abort();
    assert!(server.await.unwrap_err().is_cancelled());
    assert_eq!(
        accepts.load(Ordering::SeqCst),
        2,
        "exactly two connections were accepted"
    );
    assert_eq!(asked, 2, "one capture and one status read");
}

/// A capture taken since the latest input is shared; one started
/// before it is waited out and a fresh one taken.
#[tokio::test]
async fn a_capture_from_before_the_latest_input_is_not_shared() {
    use std::{future::Future, task::Poll};

    let dir = tempfile::tempdir().unwrap();
    let viewer = unreachable(dir.path()).await;
    let running = |revision| {
        let (sender, outcome) = watch::channel(None);
        *viewer.capture.lock().unwrap() = Some(Capture { revision, outcome });
        sender
    };

    let current = running(0);
    let shared = viewer.frame();
    tokio::pin!(shared);
    std::future::poll_fn(|cx| {
        assert!(shared.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    assert_eq!(current.receiver_count(), 2, "the current capture is shared");
    current
        .send(Some(Err((Code::Capture, "the shared picture".into()))))
        .unwrap();
    *viewer.capture.lock().unwrap() = None;
    assert_eq!(shared.await.err().unwrap().error, "the shared picture");

    let old = running(0);
    viewer.frames.lock().await.input_revision = 1;
    let fresh = viewer.frame();
    tokio::pin!(fresh);
    std::future::poll_fn(|cx| {
        assert!(
            fresh.as_mut().poll(cx).is_pending(),
            "it waits out the running capture"
        );
        Poll::Ready(())
    })
    .await;
    assert_eq!(
        old.receiver_count(),
        2,
        "the stale capture is being waited out"
    );

    *viewer.capture.lock().unwrap() = None;
    old.send(Some(Err((Code::Capture, "an old picture".into()))))
        .unwrap();
    assert_ne!(
        fresh.await.err().unwrap().error,
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
    let controller = atlas_control::Controller::at(60001, &base.join("state"));
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
                        "input" => json!({
                            "ok": true,
                            "performed": request["actions"].as_array().map_or(0, Vec::len),
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
        display: atlas_core::session::Display {
            width: 1.0,
            height: 1.0,
            scale: 1.0,
        },
        security_session_id: 0,
        display_id: 0,
        input_pid: 0,
    }
}

#[tokio::test]
async fn expiry_during_acquire_keeps_the_descriptor_until_cleanup_completes() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("a.sock");
        let (mut asked, answer) = input_service(&socket, "acquire");
        let viewer = Arc::new(http::configured(
            client_with_input(dir.path(), &socket),
            "127.0.0.1:1".into(),
            None,
        ));
        let client = viewer.client();
        let mut taking = tokio::spawn({
            let viewer = viewer.clone();
            let client = client.clone();
            async move { viewer.take_control(&client, &epoch()).await }
        });
        tokio::select! {
            biased;
            result = &mut taking => {
                panic!("acquire completed before its native request: {result:?}");
            }
            request = asked.recv() => assert_eq!(request.as_deref(), Some("acquire")),
        }
        viewer.lease.lock().await.expire_for_test();
        assert!(viewer.lease.lock().await.expired());
        assert!(viewer.lease.lock().await.active());
        assert_eq!(client.controller.acquire(false).unwrap_err().code, "busy");
        assert!(!taking.is_finished());

        answer.notify_one();
        let lost = taking.await.unwrap().unwrap_err();
        assert_eq!(lost.code, "lease-lost");
        assert_eq!(asked.recv().await.as_deref(), Some("release"));
        assert!(client.controller.pending().unwrap().is_empty());
        assert!(!viewer.lease.lock().await.active());
        assert!(client.controller.acquire(false).is_ok());
    })
    .await
    .expect("acquire expiry scenario did not finish");
}

#[tokio::test]
async fn expiry_while_release_awaits_keeps_a_successor_excluded() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("a.sock");
        let (mut asked, answer) = input_service(&socket, "release");
        let viewer = Arc::new(http::configured(
            client_with_input(dir.path(), &socket),
            "127.0.0.1:1".into(),
            None,
        ));
        let client = viewer.client();
        let owner = viewer.take_control(&client, &epoch()).await.unwrap();
        assert_eq!(asked.recv().await.as_deref(), Some("acquire"));
        let mut releasing = tokio::spawn({
            let viewer = viewer.clone();
            let owner = owner.clone();
            async move { viewer.release(Some(&owner)).await }
        });
        tokio::select! {
            biased;
            result = &mut releasing => {
                panic!("release completed before its native request: {result:?}");
            }
            request = asked.recv() => assert_eq!(request.as_deref(), Some("release")),
        }
        viewer.lease.lock().await.expire_for_test();
        assert!(viewer.lease.lock().await.active());
        assert_eq!(
            viewer.lease.lock().await.renew(&owner).unwrap_err().code,
            "lease-lost"
        );
        assert_eq!(client.controller.acquire(false).unwrap_err().code, "busy");
        assert_eq!(client.controller.pending().unwrap().len(), 1);
        assert!(!releasing.is_finished());

        answer.notify_one();
        releasing.await.unwrap().unwrap();
        assert!(!viewer.lease.lock().await.active());
        assert!(client.controller.pending().unwrap().is_empty());
        assert!(client.controller.acquire(false).is_ok());
    })
    .await
    .expect("release expiry scenario did not finish");
}

#[tokio::test]
async fn expiry_with_unconfirmed_cleanup_keeps_recovery_pending() {
    let dir = tempfile::tempdir().unwrap();
    let viewer = http::configured(client(dir.path()), "127.0.0.1:1".into(), None);
    let client = viewer.client();
    let owner = viewer
        .lease
        .lock()
        .await
        .acquire(&client.controller)
        .unwrap();
    viewer.lease.lock().await.expire_for_test();
    assert!(viewer.lease.lock().await.active());
    assert_eq!(client.controller.acquire(false).unwrap_err().code, "busy");

    let error = viewer.release(Some(&owner)).await.unwrap_err();
    assert!(error.uncertain);
    assert!(!viewer.lease.lock().await.active());
    assert_eq!(client.controller.unresolved().unwrap().len(), 1);
    assert_eq!(
        client.controller.acquire(false).unwrap_err().code,
        "uncertain"
    );
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
    other.controller = atlas_control::Controller::at(60001, &dir.path().join("other"));
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
    assert!(atlas_client::activity_log::recent(client.output_dir().unwrap(), 1).is_empty());
    drop(recording);
    giving_back.await.unwrap();
    assert_eq!(
        atlas_client::activity_log::recent(client.output_dir().unwrap(), 1)[0].action_count,
        Some(1)
    );
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
        atlas_client::activity_log::recent(client.output_dir().unwrap(), 1)[0].action_count,
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
    assert!(atlas_client::activity_log::recent(&output, 5).is_empty());

    let _ = viewer.release(None).await;
    let rows = atlas_client::activity_log::recent(&output, 5);
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

    tokio::time::pause();
    let started = tokio::time::Instant::now();
    let error = viewer.frame().await.err().unwrap();
    assert!(started.elapsed() >= READ_DEADLINE);
    assert!(started.elapsed() < READ_DEADLINE + Duration::from_secs(1));
    assert_eq!(error.error, "The helper did not answer in time");
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

    let closing = viewer.close();
    tokio::pin!(closing);
    std::future::poll_fn(|cx| {
        assert!(std::future::Future::poll(closing.as_mut(), cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;

    answer.notify_one();
    taking.await.unwrap().unwrap();
    closing.await;
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
                if let Some(status) = reply_status(&reply) {
                    return status;
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

/// The helper's activity, sessions and screenshots answer only at the
/// viewer's private address, from its own origin, within a free slot.
#[tokio::test]
async fn the_helpers_activity_and_screenshots_answer_only_the_viewers_own_origin() {
    let dir = tempfile::tempdir().unwrap();
    let viewer = unreachable(dir.path()).await;
    let routes = [
        "activity.json",
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
    let path = output.join(atlas_client::activity_log::FILE);

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
    assert!(atlas_client::activity_log::recent(&output, ACTIVITY_ROWS).is_empty());
    let (status, reply) = read_json(&viewer, "status.json").await;
    assert_eq!(status, 200);
    assert_eq!(reply["overview"]["activity"], json!([]));
    std::fs::remove_dir(path).unwrap();
    std::fs::write(
        output.join(atlas_client::test_support::PREVIOUS),
        "bad row\n{}\n{\"at\":\"1\",\"operation\":\"input\",\"ok\":true}\n",
    )
    .unwrap();
    let (status, reply) = read_json(&viewer, "activity.json").await;
    assert_eq!((status, reply["ok"].clone()), (200, json!(true)));
    assert_eq!(reply["activity"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn activity_json_gives_the_app_each_kept_pictures_after_when_taken_and_facts() {
    use std::os::unix::fs::DirBuilderExt;

    let dir = tempfile::tempdir().unwrap();
    let viewer = unreachable(dir.path()).await;
    let output = viewer.client().output_dir().unwrap().to_path_buf();
    let folder = output.join(atlas_client::before::FOLDER);
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&folder)
        .unwrap();
    let taken = atlas_core::time::timestamp() - 3_000_000_000;
    let before = format!("{taken}-before.jpg");
    let after = format!("{taken}-after.jpg");
    let earlier = taken - 600_000_000;
    let earlier_before = format!("{earlier}-before.jpg");
    for file in [&before, &after, &earlier_before] {
        std::fs::write(folder.join(file), b"jpeg").unwrap();
    }
    // A picture after an action says when it was taken by when it changed.
    let after_taken = taken + 1_300_000_000;
    std::fs::File::options()
        .write(true)
        .open(folder.join(&after))
        .unwrap()
        .set_modified(
            std::time::UNIX_EPOCH
                + std::time::Duration::from_nanos(u64::try_from(after_taken).unwrap()),
        )
        .unwrap();
    std::fs::write(
        folder.join(format!("{taken}-facts.json")),
        json!({"action": "click", "outcome": "worked"}).to_string(),
    )
    .unwrap();
    let ended = taken + 100_000_000;
    let rows = [
        json!({"at": ended.to_string(), "operation": "input", "ok": true, "actor": "agent",
            "verb": "click", "command": "c1", "before": before}),
        json!({"at": ended.to_string(), "operation": "input", "ok": true, "actor": "agent",
            "verb": "click", "command": "c2", "before": "1-before.jpg"}),
        json!({"at": (earlier + 100_000_000).to_string(), "operation": "input", "ok": true,
            "actor": "agent", "verb": "click", "command": "c3", "before": earlier_before}),
    ];
    std::fs::write(
        output.join(atlas_client::activity_log::FILE),
        rows.iter()
            .map(|row| format!("{row}\n"))
            .collect::<String>(),
    )
    .unwrap();

    let (_, reply) = read_json(&viewer, "activity.json").await;
    let listed = reply["activity"].as_array().unwrap();
    let kept = listed.iter().find(|row| row["before"] == before).unwrap();
    assert_eq!(kept["after"], after);
    assert_eq!(kept["afterAt"], json!(after_taken / 1_000_000));
    let followed = listed.iter().find(|row| row["logicalId"] == "c3").unwrap();
    assert_eq!(
        (&followed["after"], &followed["afterAt"]),
        (&json!(before), &json!(taken / 1_000_000)),
        "the next step's picture before, taken when its name says"
    );
    assert_eq!(
        kept["facts"],
        json!({"action": "click", "outcome": "worked"})
    );
    let gone = listed.iter().find(|row| row["logicalId"] == "c2").unwrap();
    assert_eq!(
        (&gone["before"], &gone["after"], &gone["afterAt"]),
        (&Value::Null, &Value::Null, &Value::Null),
        "a picture no longer kept is not named"
    );
    assert!(gone.get("facts").is_none());
}

#[tokio::test]
async fn activity_json_never_carries_typed_text() {
    let dir = tempfile::tempdir().unwrap();
    let viewer = unreachable(dir.path()).await;
    let output = viewer.client().output_dir().unwrap().to_path_buf();
    let at = atlas_core::time::timestamp() - 3_000_000_000;
    // Fields the log never keeps, written here all the same: none of
    // them may reach the app.
    let row = json!({
        "at": at.to_string(), "operation": "input", "ok": true, "actionCount": 2,
        "actor": "agent", "verb": "type", "app": "TextEdit", "command": "0123456789ab",
        "text": "hunter2", "argv": ["rm", "secret-argument"], "x": 123.5, "y": 456.25,
        "title": "Secret plans", "elapsedMs": 4.0,
    });
    let key = json!({
        "at": at.to_string(), "operation": "input", "ok": true, "verb": "key",
        "app": "TextEdit", "bundleId": "com.apple.TextEdit", "label": "⇧⌘Z Page Up",
        "actionCount": 2, "actor": "agent", "command": "c1", "text": "hunter2",
    });
    std::fs::write(
        output.join(atlas_client::activity_log::FILE),
        format!("{key}\n{row}\n"),
    )
    .unwrap();

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
            "after",
            "afterAt",
            "ageSeconds",
            "app",
            "before",
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
    assert_eq!(listed["says"], "typed in TextEdit");
    assert_eq!(listed["count"], 2);
    assert_eq!(listed["ageSeconds"], 3);
    assert_eq!(listed["session"], Value::Null);
    assert_eq!(reply["sessions"], json!({}));
    let pressed = &reply["activity"][1];
    assert_eq!(pressed["app"], "TextEdit");
    assert_eq!(pressed["bundleId"], "com.apple.TextEdit");
    assert_eq!(pressed["label"], "⇧⌘Z Page Up");
    assert_eq!(
        pressed["logicalId"], "c1",
        "a command's steps share its token"
    );
    assert_eq!(pressed["keys"], json!([["⇧", "⌘", "Z"], ["Page Up"]]));

    // A row of an agent's host session names it, and nothing more.
    let session =
        atlas_core::host::HostSession::new(atlas_core::host::Host::Claude, "left").unwrap();
    let mut row = row;
    row["session"] = json!(session.key());
    std::fs::write(
        output.join(atlas_client::activity_log::FILE),
        format!("{row}\n"),
    )
    .unwrap();
    atlas_client::host_session::store::note(
        &viewer.client(),
        &session,
        atlas_client::host_session::store::Kind::Other,
        Some(&atlas_client::doing::Doing::new("type")),
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
        atlas_core::host::HostSession::new(atlas_core::host::Host::Codex, "right").unwrap(),
        atlas_core::host::HostSession::new(atlas_core::host::Host::Pi, "gone").unwrap(),
        atlas_core::host::HostSession::new(atlas_core::host::Host::Claude, "quiet").unwrap(),
    );
    for session in [&named, &unmentioned] {
        atlas_client::host_session::store::note(
            &client,
            session,
            atlas_client::host_session::store::Kind::Other,
            Some(&atlas_client::doing::Doing::new("click")),
            &Ok(json!({"ok": true})),
        )
        .unwrap();
    }
    let at = atlas_core::time::timestamp();
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
    std::fs::write(output.join(atlas_client::activity_log::FILE), rows).unwrap();

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
    std::fs::write(output.join(atlas_client::activity_log::FILE), log).unwrap();

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
        .open(output.join(atlas_client::activity_log::FILE))
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

/// Write `files` as the log in `output`, newest first: the current file,
/// the previous one, then the history.
fn kept_log(output: &Path, files: &[String]) {
    let history = output.join(atlas_client::test_support::HISTORY);
    std::fs::create_dir(&history).unwrap();
    std::fs::set_permissions(
        &history,
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )
    .unwrap();
    for (index, text) in files.iter().enumerate() {
        let path = match index {
            0 => output.join(atlas_client::activity_log::FILE),
            1 => output.join(atlas_client::test_support::PREVIOUS),
            _ => history.join(format!("{}.jsonl", files.len() - index)),
        };
        std::fs::write(path, text).unwrap();
    }
}

/// `count` of an agent's clicks from `first` milliseconds on, in `session`
/// when it names one.
fn clicks(first: u128, count: u128, session: Option<&str>) -> String {
    (first..first + count)
        .map(|at| {
            let mut row = json!({"at": (at * 1_000_000).to_string(), "operation": "input",
                "ok": true, "actor": "agent", "verb": "click",
                "command": format!("{at:012}-{}", session.unwrap_or("none"))});
            if let Some(session) = session {
                row["session"] = json!(session);
            }
            format!("{row}\n")
        })
        .collect()
}

#[tokio::test]
async fn activity_json_for_a_session_finds_its_rows_in_the_oldest_kept_file() {
    let dir = tempfile::tempdir().unwrap();
    let viewer = unreachable(dir.path()).await;
    let client = viewer.client();
    let output = client.output_dir().unwrap().to_path_buf();
    let (old, busy) = (
        atlas_core::host::HostSession::new(atlas_core::host::Host::Codex, "demo").unwrap(),
        atlas_core::host::HostSession::new(atlas_core::host::Host::Claude, "load").unwrap(),
    );
    let (old_key, busy_key) = (old.key().as_str(), busy.key().as_str());
    kept_log(
        &output,
        &[
            clicks(5_000, 300, Some(busy_key)),
            clicks(4_000, 300, Some(busy_key)),
            clicks(3_000, 300, Some(busy_key)),
            clicks(1_000, 3, Some(old_key)) + &clicks(2_000, 300, Some(busy_key)),
        ],
    );

    let (_, every) = read_json(&viewer, "activity.json").await;
    assert!(
        every["activity"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["session"] == busy_key),
        "every session's newest rows are the busy one's"
    );
    assert!(every.get("cleared").is_none());

    let (status, reply) = read_json(&viewer, &format!("activity.json?session={old_key}")).await;
    assert_eq!(status, 200);
    let rows = reply["activity"].as_array().unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| row["startedAt"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [1_002, 1_001, 1_000]
    );
    assert!(rows.iter().all(|row| row["session"] == old_key));
    assert_eq!(reply["cleared"], false);
    assert_eq!(
        rows[0]["id"],
        {
            let (_, again) = read_json(&viewer, &format!("activity.json?session={old_key}")).await;
            again["activity"][0]["id"].clone()
        },
        "a row keeps its id across reads"
    );
    let (_, busy) = read_json(&viewer, &format!("activity.json?session={busy_key}")).await;
    assert_eq!(busy["activity"].as_array().unwrap().len(), ACTIVITY_ROWS);
    assert_eq!(busy["activity"][0]["startedAt"], 5_299);
}

#[tokio::test]
async fn activity_json_says_a_sessions_steps_were_cleared_only_once_the_log_no_longer_reaches_them()
{
    let dir = tempfile::tempdir().unwrap();
    let viewer = unreachable(dir.path()).await;
    let client = viewer.client();
    let output = client.output_dir().unwrap().to_path_buf();
    let (session, silent) = (
        atlas_core::host::HostSession::new(atlas_core::host::Host::Codex, "demo").unwrap(),
        atlas_core::host::HostSession::new(atlas_core::host::Host::Codex, "quiet").unwrap(),
    );
    let mut acting = (*client).clone();
    acting.actor = atlas_client::client::Actor::Agent;
    acting.host_session = Some(session.clone());
    acting.record_for_test(&json!({"op": "input"}), &json!({"ok": true}));
    for session in [&session, &silent] {
        atlas_client::host_session::store::note(
            &client,
            session,
            atlas_client::host_session::store::Kind::Other,
            Some(&atlas_client::doing::Doing::new("click")),
            &Ok(json!({"ok": true})),
        )
        .unwrap();
    }
    let route = format!("activity.json?session={}", session.key());
    let now = atlas_core::time::timestamp() / 1_000_000;

    // The log starts after the session's newest row: its rows are gone.
    kept_log(&output, &[clicks(now + 60_000, 2, None)]);
    let (status, reply) = read_json(&viewer, &route).await;
    assert_eq!(status, 200);
    assert_eq!(reply["activity"], json!([]));
    assert_eq!(reply["cleared"], true);
    // One that counted a click but wrote no row had nothing to clear.
    let (_, reply) = read_json(&viewer, &format!("activity.json?session={}", silent.key())).await;
    assert_eq!(reply["cleared"], false);

    // A log that reaches back before it holds none of its rows: nothing was
    // cleared.
    std::fs::write(
        output.join(atlas_client::activity_log::FILE),
        clicks(now - 60_000, 2, None),
    )
    .unwrap();
    let (_, reply) = read_json(&viewer, &route).await;
    assert_eq!(reply["cleared"], false);

    // A session with no record is never said to be cleared.
    let unrecorded = atlas_core::host::Key::of(atlas_core::host::Host::Pi, "none");
    let (_, reply) = read_json(&viewer, &format!("activity.json?session={unrecorded}")).await;
    assert_eq!(reply["cleared"], false);
}

#[tokio::test]
async fn activity_json_reads_terminals_rows_by_the_empty_key_and_refuses_a_key_that_is_not_one() {
    let dir = tempfile::tempdir().unwrap();
    let viewer = unreachable(dir.path()).await;
    let output = viewer.client().output_dir().unwrap().to_path_buf();
    let person = json!({"at": "9000000", "operation": "input", "ok": true, "actor": "viewer"});
    kept_log(
        &output,
        &[clicks(5_000, 2, Some("0123456789ab"))
            + &format!("{person}\n")
            + &clicks(1_000, 1, None)],
    );
    let (status, reply) = read_json(&viewer, "activity.json?session=").await;
    assert_eq!(status, 200);
    let rows = reply["activity"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        1,
        "an agent's command no host named, never the person's"
    );
    assert_eq!(rows[0]["startedAt"], 1_000);
    assert_eq!(reply["cleared"], false);

    for bad in ["nope", "0123456789AB", "0123456789ab0", "%2e%2e"] {
        let (status, reply) = read_json(&viewer, &format!("activity.json?session={bad}")).await;
        assert_eq!(
            (status, reply["code"].clone()),
            (400, json!("input")),
            "{bad}"
        );
    }
}

#[tokio::test]
async fn reading_the_helpers_apps_and_windows_records_no_activity() {
    let dir = tempfile::tempdir().unwrap();

    let agent = client(dir.path()).doing(atlas_client::doing::Doing::new("screenshot"));
    let filter = atlas_client::agent::WindowFilter {
        app: None,
        title: None,
        all: false,
    };
    assert!(agent.apps().await.is_err());
    assert!(atlas_client::agent::list(&agent, &filter).await.is_err());
    let output = agent.output_dir().unwrap().to_path_buf();
    assert!(atlas_client::activity_log::recent(&output, 5).is_empty());

    // The same client records what acts, even when no helper answers.
    assert!(
        agent
            .capture(atlas_core::screen::frame::Size::Standard, None, false)
            .await
            .is_err()
    );
    let rows = atlas_client::activity_log::recent(&output, 5);
    assert_eq!(rows.len(), 1);
    assert!(
        atlas_client::overview::shown(&rows[0], 0).reason.is_some(),
        "a failed capture says why: {:?}",
        rows[0]
    );
}

#[test]
fn the_apps_activity_row_keeps_its_fields_and_values() {
    let dir = tempfile::tempdir().unwrap();
    let line = r#"{"at":"90000000000","operation":"input","ok":false,"code":"focus-changed","reason":"window","partial":true,"verb":"click","app":"Opie","label":"Save","session":"0123456789ab","capture":"1-capture.png","actionCount":1,"actor":"agent","command":"c1","before":"90000000000-before.jpg"}"#;
    std::fs::write(
        dir.path().join(atlas_client::activity_log::FILE),
        format!("{line}\n{{\"at\":\"1\",\"operation\":\"run\"}}\n"),
    )
    .unwrap();
    let rows = atlas_client::activity_log::recent(dir.path(), 5);
    let shown: Vec<Value> = rows
        .iter()
        .map(|row| activity_row(row, 100_000_000_000))
        .collect();
    // AtlasService decodes this same fixture, so producer and consumer cannot
    // silently drift while asserting separate copies of the wire contract.
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/activity-projection.json"
    ))
    .unwrap();
    assert_eq!(json!({"activity": shown}), fixture);
}

#[test]
fn a_commands_logical_id_holds_from_start_to_end_and_an_input_runs_holds_as_it_grows() {
    use atlas_client::activity_log::{FILE, Row, recent};

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
        dir.path().join(atlas_client::test_support::PREVIOUS),
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
    use atlas_client::host_session::store;
    use atlas_core::host::{Host, HostSession};

    let dir = tempfile::tempdir().unwrap();
    let viewer = unreachable(dir.path()).await;
    let client = viewer.client();
    let output = client.output_dir().unwrap().to_path_buf();
    let shot = "1700000005000000000-capture.png";
    std::fs::write(
        output.join(shot),
        atlas_native::native::capture::sample_png(64, 40),
    )
    .unwrap();
    let session = HostSession::new(Host::Claude, "secret-host-id").unwrap();
    store::note(
        &client,
        &session,
        store::Kind::Screenshot,
        Some(&atlas_client::doing::Doing::new("screenshot")),
        &Ok(json!({"ok": true, "path": output.join(shot)})),
    )
    .unwrap();
    store::note(
        &client,
        &session,
        store::Kind::Open,
        Some(&atlas_client::doing::Doing::new("open").app(Some("Notes"))),
        &Ok(json!({"ok": true})),
    )
    .unwrap();
    let at = atlas_core::time::timestamp();
    let row = json!({"at": at.to_string(), "operation": "open", "ok": true, "actor": "agent",
        "verb": "open", "app": "Notes", "command": "000000000001", "session": session.key()});
    std::fs::write(
        output.join(atlas_client::activity_log::FILE),
        format!("{row}\n"),
    )
    .unwrap();

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
    use atlas_client::host_session::record::Record;
    use atlas_core::host::{Host, HostSession};

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
        (atlas_client::environment::Phase::RollingBack, 3),
        (atlas_client::environment::Phase::Applied, 2),
        (atlas_client::environment::Phase::Applying, 1),
    ] {
        working.planned(atlas_core::files::Token::new(), phase, at);
    }
    let records = [
        working,
        record("quiet", 2 * hour),
        record("week", 8 * 24 * hour),
    ];
    let history = atlas_client::session_view::HistorySource {
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
        .map(|past| past["state"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(states, ["idle", "idle"], "a week and more is gone");
    assert_eq!(history[1]["endedAt"], Value::Null, "it only went quiet");
    let plans: Vec<_> = history[0]["plans"]
        .as_array()
        .unwrap()
        .iter()
        .map(|plan| (plan["phase"].clone(), plan["canRollBack"].clone()))
        .collect();
    assert_eq!(
        plans,
        [
            (json!("applying"), json!(true)),
            (json!("applied"), json!(true)),
            (json!("rollingBack"), json!(false)),
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
            atlas_native::native::capture::sample_png(64, 40),
        )
        .unwrap();
    }
    let session =
        atlas_core::host::HostSession::new(atlas_core::host::Host::Claude, "left").unwrap();
    atlas_client::host_session::store::note(
        &client,
        &session,
        atlas_client::host_session::store::Kind::Screenshot,
        Some(&atlas_client::doing::Doing::new("screenshot")),
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
    let older = atlas_native::native::capture::sample_png(1280, 800);
    let newer = atlas_native::native::capture::sample_png(640, 400);
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

#[tokio::test]
async fn a_picture_before_an_action_is_served_from_its_own_folder_by_its_own_name() {
    let dir = tempfile::tempdir().unwrap();
    let viewer = unreachable(dir.path()).await;
    let output = viewer.client().output_dir().unwrap().to_path_buf();
    let folder = output.join(atlas_client::before::FOLDER);
    std::os::unix::fs::DirBuilderExt::mode(&mut std::fs::DirBuilder::new(), 0o700)
        .create(&folder)
        .unwrap();
    let png = atlas_native::native::capture::sample_png(640, 400);
    let (jpeg, _, _) = atlas_client::helper_info::jpeg(&png, (640, 0.6)).unwrap();
    std::fs::write(folder.join("1700000000000000000-before.jpg"), &jpeg).unwrap();
    std::fs::write(output.join("1700000005000000000-before.jpg"), &jpeg).unwrap();
    std::os::unix::fs::symlink(
        folder.join("1700000000000000000-before.jpg"),
        folder.join("1700000009000000000-before.jpg"),
    )
    .unwrap();
    std::fs::write(folder.join("1700000000000000000-capture.png"), &png).unwrap();
    std::fs::write(dir.path().join("u.toml"), SETTINGS).unwrap();

    let (status, kind, body) = read(
        &viewer,
        "screenshot.png?name=1700000000000000000-before.jpg&size=full",
    )
    .await;
    assert_eq!((status, kind.as_str()), (200, "image/jpeg"));
    assert_eq!(body, jpeg, "as kept");
    let (status, _, body) = read(
        &viewer,
        "screenshot.png?name=1700000000000000000-before.jpg",
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(&body[..2], [0xFF, 0xD8], "a thumbnail");

    let (_, listed) = read_json(&viewer, "screenshots.json").await;
    assert_eq!(
        listed["screenshots"],
        json!([]),
        "never listed as a screenshot"
    );
    for query in [
        "?name=1700000005000000000-before.jpg",
        "?name=1700000009000000000-before.jpg",
        "?name=before/1700000000000000000-before.jpg",
        "?name=1700000000000000000-capture.png",
    ] {
        let (status, refused) = read_json(&viewer, &format!("screenshot.png{query}")).await;
        assert_eq!(
            (status, refused["code"].clone()),
            (404, json!("bad-request")),
            "{query}"
        );
    }
}

#[tokio::test]
async fn control_tokens_keep_their_validation_order_for_missing_and_non_string_fields()
-> atlas_core::error::Result<()> {
    let directory = tempfile::tempdir()?;
    let viewer = http::configured(client(directory.path()), "127.0.0.1:0".into(), None);
    for body in [
        json!({}),
        json!({"lease":null,"frame":null}),
        json!({"lease":true,"frame":false}),
        json!({"lease":1,"frame":2}),
        json!({"lease":[],"frame":{}}),
        json!({"lease":"unknown","frame":"unknown","actions":"invalid"}),
    ] {
        for (route, code) in [
            ("control/acquire", "stale-frame"),
            ("control/renew", "lease-lost"),
            ("input", "lease-lost"),
        ] {
            let (status, reply) = posted(&viewer, route, &body).await;
            assert_eq!(status, 409, "{route}: {body}: {reply}");
            assert_eq!(reply["code"], code, "{route}: {body}: {reply}");
            assert_eq!(reply["uncertain"], false);
            assert!(viewer.client().controller.unresolved()?.is_empty());
        }
    }
    Ok(())
}

async fn fresh_input_frame(viewer: &Viewer, key: &str) {
    let epoch = epoch();
    let frame = atlas_core::screen::frame::Frame {
        png: Arc::from(&b"stand-in frame"[..]),
        width: 1,
        height: 1,
        epoch,
        at: std::time::Instant::now(),
    };
    let mut frames = viewer.frames.lock().await;
    frames.items.clear();
    frames.items.push_back((key.into(), Arc::new(frame)));
}

/// Whether the frame service has been told of sent input since it last was.
async fn frame_service_told(viewer: &Viewer) -> bool {
    tokio::time::timeout(Duration::ZERO, viewer.input_sent.notified())
        .await
        .is_ok()
}

#[tokio::test]
async fn raw_input_with_a_live_lease_rejects_the_whole_invalid_batch_before_native_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("input.sock");
    let (mut asked, _never) = input_service(&socket, "none");
    let viewer = http::configured(
        client_with_input(dir.path(), &socket),
        "127.0.0.1:1".into(),
        None,
    );
    let client = viewer.client();
    let owner = viewer.take_control(&client, &epoch()).await.unwrap();
    assert_eq!(asked.try_recv().ok().as_deref(), Some("acquire"));
    let owner_json = json!(owner).to_string();
    let mut rejected = vec![
        "null".to_owned(),
        "false".into(),
        "1".into(),
        r#""actions""#.into(),
        "{}".into(),
        "[]".into(),
        json!(vec![json!({"type":"key","key":"a"}); 33]).to_string(),
    ];
    let alternatives = [
        json!({"type":"pointer","phase":"down","x":0,"y":0}),
        json!({"type":"move","x":0,"y":0}),
        json!({"type":"click","x":0,"y":0}),
        json!({"type":"drag","fromX":0,"fromY":0,"toX":0,"toY":0}),
        json!({"type":"scroll","x":0,"y":0}),
        json!({"type":"type","text":"a"}),
        json!({"type":"key","key":"a"}),
        json!({"type":"wait","ms":0}),
    ];
    for action in &alternatives {
        for field in action.as_object().unwrap().keys() {
            let mut missing = action.clone();
            missing.as_object_mut().unwrap().remove(field);
            rejected.push(json!([{"type":"key","key":"a"}, missing]).to_string());
            for wrong in [Value::Null, json!(true), json!([]), json!({})] {
                let mut malformed = action.clone();
                malformed[field] = wrong;
                rejected.push(json!([{"type":"key","key":"a"}, malformed]).to_string());
            }
        }
    }
    for action in [
        json!({"type":"key","key":"a","future":true}),
        json!({"type":"pointer","phase":"down","x":0,"y":0,"button":null}),
        json!({"type":"pointer","phase":"down","x":0,"y":0,"count":0}),
        json!({"type":"click","x":0,"y":0,"count":6}),
        json!({"type":"click","x":0,"y":0,"modifiers":[null]}),
        json!({"type":"drag","fromX":0,"fromY":0,"toX":1,"toY":0}),
        json!({"type":"scroll","x":0,"y":0,"dx":101}),
        json!({"type":"type","text":""}),
        json!({"type":"key","key":"invalid"}),
        json!({"type":"wait","ms":251}),
    ] {
        rejected.push(json!([{"type":"key","key":"a"}, action]).to_string());
    }
    rejected.push(r#"[{"type":"key","key":"a"},{"type":"key","key":"a","\u006bey":null}]"#.into());
    for actions in std::iter::once(None).chain(rejected.into_iter().map(Some)) {
        viewer.lease.lock().await.renew(&owner).unwrap();
        fresh_input_frame(&viewer, "observed").await;
        let revision = viewer.frames.lock().await.input_revision;
        let actions = actions
            .map(|actions| format!(r#", "actions":{actions}"#))
            .unwrap_or_default();
        let body = format!(r#"{{"lease":{owner_json},"frame":"observed"{actions}}}"#);
        let (status, reply) = posted_raw(&viewer, "input", &body).await;
        assert_eq!(status, 400, "{body}: {reply}");
        assert!(
            ["input", "invalid-json"].contains(&reply["code"].as_str().unwrap()),
            "{body}: {reply}"
        );
        assert_eq!(reply["uncertain"], false, "{body}");
        assert!(
            matches!(
                asked.try_recv(),
                Err(tokio::sync::mpsc::error::TryRecvError::Empty)
            ),
            "{body}"
        );
        viewer.lease.lock().await.require(&owner).unwrap();
        assert!(client.controller.pending().unwrap().is_empty(), "{body}");
        assert_eq!(viewer.frames.lock().await.input_revision, revision + 1);
    }

    for action in alternatives {
        viewer.lease.lock().await.renew(&owner).unwrap();
        fresh_input_frame(&viewer, "observed").await;
        let body = format!(
            r#"{{"lease":{owner_json},"frame":"observed","actions":[{action}],"other":1,"other":2}}"#
        );
        let (status, reply) = posted_raw(&viewer, "input", &body).await;
        assert_eq!(
            (status, reply["performed"].clone()),
            (200, json!(1)),
            "{body}: {reply}"
        );
        assert_eq!(asked.try_recv().ok().as_deref(), Some("input"), "{body}");
        viewer.lease.lock().await.require(&owner).unwrap();
        assert!(client.controller.pending().unwrap().is_empty());
    }
    fresh_input_frame(&viewer, "observed").await;
    let body = format!(
        r#"{{"lease":"wrong","lease":{owner_json},"frame":"stale","\u0066rame":"observed","actions":null,"actions":[{{"type":"key","key":null,"key":"a"}}]}}"#
    );
    let (status, reply) = posted_raw(&viewer, "input", &body).await;
    assert_eq!(
        (status, reply["performed"].clone()),
        (200, json!(1)),
        "{reply}"
    );
    assert_eq!(asked.try_recv().ok().as_deref(), Some("input"));
    viewer.release(Some(&owner)).await.unwrap();
    assert_eq!(asked.try_recv().ok().as_deref(), Some("release"));
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn raw_input_frame_refusals_clean_up_only_the_live_lease_without_dispatching_input() {
    for frame in [
        None,
        Some(Value::Null),
        Some(json!(true)),
        Some(json!(1)),
        Some(json!([])),
        Some(json!({})),
        Some(json!("")),
        Some(json!("stale")),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("input.sock");
        let (mut asked, _never) = input_service(&socket, "none");
        let viewer = http::configured(
            client_with_input(dir.path(), &socket),
            "127.0.0.1:1".into(),
            None,
        );
        let client = viewer.client();
        let owner = viewer.take_control(&client, &epoch()).await.unwrap();
        assert_eq!(asked.try_recv().ok().as_deref(), Some("acquire"));
        fresh_input_frame(&viewer, "observed").await;
        let valid = json!({"lease":owner,"frame":"observed","actions":[{"type":"key","key":"a"}]});
        let (status, reply) = posted(&viewer, "input", &valid).await;
        assert_eq!(
            (status, reply["performed"].clone()),
            (200, json!(1)),
            "{reply}"
        );
        assert_eq!(asked.try_recv().ok().as_deref(), Some("input"));
        assert!(frame_service_told(&viewer).await);
        fresh_input_frame(&viewer, "observed").await;
        let revision = viewer.frames.lock().await.input_revision;
        let mut malformed = valid;
        match frame {
            Some(frame) => malformed["frame"] = frame,
            None => {
                malformed.as_object_mut().unwrap().remove("frame");
            }
        }
        let (status, reply) = posted(&viewer, "input", &malformed).await;
        assert_eq!(status, 409, "{malformed}: {reply}");
        assert_eq!(reply["code"], "stale-frame", "{malformed}");
        assert_eq!(reply["uncertain"], false, "{malformed}");
        assert_eq!(asked.try_recv().ok().as_deref(), Some("release"));
        assert!(matches!(
            asked.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));
        assert!(!viewer.lease.lock().await.active());
        assert!(client.controller.pending().unwrap().is_empty());
        assert_eq!(viewer.frames.lock().await.input_revision, revision);
        assert!(
            !frame_service_told(&viewer).await,
            "refused input tells nothing"
        );
    }
}

#[tokio::test]
async fn input_tells_the_frame_service_before_the_helper_finishes_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("input.sock");
    let (mut asked, finish) = input_service(&socket, "input");
    let viewer = http::configured(
        client_with_input(dir.path(), &socket),
        "127.0.0.1:1".into(),
        None,
    );
    let client = viewer.client();
    let owner = viewer.take_control(&client, &epoch()).await.unwrap();
    assert_eq!(asked.recv().await.as_deref(), Some("acquire"));
    fresh_input_frame(&viewer, "observed").await;
    let sending = viewer.clone();
    let typed = json!({"lease":owner,"frame":"observed","actions":[{"type":"type","text":"a"}]});
    let input = tokio::spawn(async move { posted(&sending, "input", &typed).await });
    // The helper has the input and has not answered it.
    assert_eq!(asked.recv().await.as_deref(), Some("input"));
    assert!(
        frame_service_told(&viewer).await,
        "told while the input is still running"
    );
    finish.notify_one();
    let (status, reply) = input.await.unwrap();
    assert_eq!(
        (status, reply["performed"].clone()),
        (200, json!(1)),
        "{reply}"
    );
    assert!(
        frame_service_told(&viewer).await,
        "told again once it is done"
    );
    viewer.release(Some(&owner)).await.unwrap();
}

fn recovery_session(id: &str) -> atlas_core::host::HostSession {
    use atlas_core::host::{Host, HostSession};
    HostSession::new(Host::Named, id).unwrap()
}

#[tokio::test]
async fn recovery_replaces_stale_input_during_restart_despite_recent_work_and_waiters() {
    use serde_json::Value;
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::UnixListener,
    };

    for (own, other_owner, through_app) in [
        (None, false, false),
        (Some(recovery_session("recovering")), false, false),
        (None, true, false),
        (None, true, true),
    ] {
        for waiter in [false, true] {
            if other_owner && waiter {
                continue;
            }
            let directory = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
            let mut client = atlas_client::test_support::client(directory.path());
            client.host_session = own.clone();
            client.controller = client.controller.labelled(own.as_ref());
            client.transport.peer_uid = nix::unistd::getuid().as_raw();
            client.input_peer_uid = Some(client.transport.peer_uid);
            std::fs::write(&client.config.identity_probe, b"installed build").unwrap();
            let build = client.installed_build().unwrap();
            let other = recovery_session("worker");
            let at = atlas_core::time::now_ms() - if other_owner { 120_001 } else { 0 };
            atlas_client::host_session::store::Store::at(client.output_dir().unwrap())
                .update(&other, at, &client.controller, |record| {
                    record.last_seen = at;
                    record.ended = None;
                })
                .unwrap();
            if waiter {
                let holder = client
                    .controller
                    .clone()
                    .labelled(Some(&other))
                    .acquire(true)
                    .unwrap();
                atlas_client::test_support::waiting_note(
                    &directory.path().join("state"),
                    Some(&other),
                );
                drop(holder);
            }
            let controller = client.controller.clone().labelled(if other_owner {
                Some(&other)
            } else {
                own.as_ref()
            });
            drop(controller.quarantine().unwrap());
            let pending = client.controller.pending().unwrap();
            assert_eq!(pending.len(), 1);

            std::fs::create_dir_all(client.transport.path.parent().unwrap()).unwrap();
            let daemon = UnixListener::bind(&client.transport.path).unwrap();
            let mut input = UnixListener::bind(&client.config.input_socket).unwrap();
            let previous_pid = i32::MAX as u32;
            assert!(!atlas_client::test_support::process_exists(previous_pid));
            let mut capture_pid = 123;
            let mut replaced = false;
            let mut changes = Vec::new();
            let serve = async {
                loop {
                    let (native, (peer, _)) = tokio::select! {
                        peer = daemon.accept() => (false, peer.unwrap()),
                        peer = input.accept() => (true, peer.unwrap()),
                    };
                    let mut peer = BufReader::new(peer);
                    let mut line = String::new();
                    peer.read_line(&mut line).await.unwrap();
                    let request: Value = serde_json::from_str(&line).unwrap();
                    let proof = json!({
                        "ok": true, "uid": client.config.helper_uid,
                        "hasGraphicAccess": true, "onConsole": false,
                        "parentPID": capture_pid, "securitySessionID": 42,
                        "displayID": 7, "width": 100, "height": 100,
                    });
                    let reply = match (native, request["op"].as_str().unwrap()) {
                        (false, "hello") => json!({
                            "ok": true, "user": {"uid": client.config.helper_uid},
                            "daemon": {"pid": capture_pid},
                            "session": {"managerName": "Aqua", "onConsole": false},
                            "display": {"width": 100, "height": 100, "scale": 1},
                        }),
                        (true, "hello") => json!({
                            "ok": true, "pid": if replaced { 456 } else { previous_pid },
                            "protocolVersion": 1, "accessibility": true,
                            "ready": capture_pid == 123 || replaced,
                            "session": proof, "controlActive": false, "heldInput": false,
                            "build": if replaced { build.as_str() } else { "old build" },
                            "capabilities": [atlas_core::session::SHUTDOWN],
                        }),
                        (false, "job") => {
                            json!({"ok": true, "op": "job", "action": "show", "jobs": []})
                        }
                        (false, "run") if request["argv"][1] == "session-info" => {
                            atlas_client::test_support::completed(&proof.to_string())
                        }
                        (true, "reset") => {
                            assert_eq!(request["epoch"]["processId"], previous_pid);
                            changes.push("reset");
                            json!({"ok": true, "released": true})
                        }
                        (false, "restart") => {
                            assert_eq!(
                                client.controller.acquire(true).unwrap_err().code,
                                Code::Busy
                            );
                            assert!(pending.iter().all(|claim| {
                                client.controller.pending().unwrap().contains(claim)
                            }));
                            changes.push("restart");
                            capture_pid = 124;
                            json!({"ok": true})
                        }
                        (true, "shutdown") => {
                            assert_eq!(request["pid"], previous_pid);
                            assert_eq!(
                                client.controller.acquire(true).unwrap_err().code,
                                Code::Busy
                            );
                            changes.push("shutdown");
                            std::fs::remove_file(&client.config.input_socket).unwrap();
                            json!({"ok": true})
                        }
                        (false, "run") => {
                            assert_eq!(request["argv"][1], "-c");
                            assert_eq!(
                                client.controller.acquire(true).unwrap_err().code,
                                Code::Busy
                            );
                            changes.push("spawn");
                            input = UnixListener::bind(&client.config.input_socket).unwrap();
                            replaced = true;
                            atlas_client::test_support::completed("456\n")
                        }
                        _ => panic!("unexpected request: {request}"),
                    };
                    peer.get_mut()
                        .write_all(format!("{reply}\n").as_bytes())
                        .await
                        .unwrap();
                }
            };
            let outcome = tokio::time::timeout(Duration::from_secs(5), async {
                tokio::select! {
                    outcome = async {
                        if through_app {
                            Ok(recover_from_app(client.clone()).await)
                        } else {
                            client.restart().await
                        }
                    } => outcome,
                    _ = serve => unreachable!(),
                }
            })
            .await
            .unwrap()
            .unwrap();
            assert_eq!(outcome["ok"], true);
            assert_eq!(outcome["inputReplaced"], true);
            assert_eq!(outcome["session"]["daemon"]["pid"], 124);
            assert!(atlas_core::session::input_current(
                &outcome["session"],
                client.config.helper_uid,
                456,
            ));
            assert_eq!(changes, ["reset", "restart", "shutdown", "spawn"]);
            assert!(client.controller.pending().unwrap().is_empty());
            assert!(client.controller.holder_note().is_none());
            if waiter {
                assert_eq!(
                    client
                        .controller
                        .checked_waiting_notes()
                        .map(|notes| notes.len()),
                    Some(1)
                );
            }
        }
    }
}
