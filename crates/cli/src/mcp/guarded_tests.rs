use super::*;
use crate::mcp::{control::Held, resources::screen_contents, tests::LOOKING};

fn root(path: &std::path::Path) -> Server {
    let root = super::super::tests::server(path);
    root.app_views.get_or_init(|| Arc::new(Registry::default()));
    root.views.store(true, Ordering::SeqCst);
    root
}

#[test]
fn the_close_tool_is_listed_with_only_its_own_fields() {
    assert_eq!(
        catalog(Tool::CloseView),
        json!({
            "name": "view_close",
            "description": "Close this Atlas view's private context.",
            "inputSchema": {
                "type": "object",
                "properties": {ARGUMENT: {"type": "string"}},
                "required": [ARGUMENT],
                "additionalProperties": false,
            },
            "_meta": {
                "ui": {"resourceUri": "ui://atlas/screen.html", "visibility": ["app"]},
                "openai/visibility": "private",
                "openai/widgetAccessible": true,
            },
        })
    );
}

#[tokio::test]
async fn bootstrap_credentials_are_private_and_public_calls_never_claim_control() {
    let dir = tempfile::tempdir().unwrap();
    let root = root(dir.path());
    let registry = root.app_views.get().unwrap();
    let public = registry
        .call(&root, &json!(1), &json!({"name":"show_screen"}))
        .await;
    assert!(
        registry.is_empty(),
        "ordinary model calls allocate no authority"
    );
    assert_eq!(
        public.value["result"]["structuredContent"]["canControl"],
        false
    );
    assert_eq!(
        public.value["result"]["structuredContent"]["control"],
        false
    );
    let opened = registry
        .call(
            &root,
            &json!(2),
            &json!({"name":"show_screen","arguments":{"_atlasOpen":true}}),
        )
        .await;
    let token = opened.value["result"]["_meta"][META].as_str().unwrap();
    assert_eq!(token.len(), atlas_core::files::TOKEN_BYTES * 2);
    let mut visible = opened.value.clone();
    visible["result"].as_object_mut().unwrap().remove("_meta");
    assert!(!visible.to_string().contains(token));
    for flag in ["canControl", "canResume", "control"] {
        assert_eq!(visible["result"]["structuredContent"][flag], false);
    }
    assert!(!screen_contents().to_string().contains(token));
    assert!(!root.tools().to_string().contains(token));
    let (child, _reservation, minted) = registry
        .reserve(&root, Some(&json!(token)), false, false)
        .unwrap();
    assert!(minted.is_none());
    assert!(child.app_views.get().is_none());
    assert!(!child.controlling().await);
    assert_eq!(child.client.actor, root.client.actor);
    assert!(child.client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn every_mutation_is_refused_before_reaching_a_child_without_valid_authority() {
    let dir = tempfile::tempdir().unwrap();
    let root = root(dir.path());
    let registry = root.app_views.get().unwrap();
    let names = root.tools();
    for tool in names
        .as_array()
        .unwrap()
        .iter()
        .filter(|tool| !LOOKING.contains(&tool["name"].as_str().unwrap()))
    {
        for arguments in [
            json!({}),
            json!({ARGUMENT:7}),
            json!({ARGUMENT:"bad"}),
            json!({ARGUMENT:"0".repeat(48)}),
        ] {
            let refused = registry
                .call(
                    &root,
                    &json!(1),
                    &json!({"name":tool["name"],"arguments":arguments}),
                )
                .await;
            assert_eq!(refused.value["error"]["code"], -32602);
        }
    }
    assert!(registry.is_empty());
    assert!(!dir.path().join("state").exists());
    let refused = registry
        .call(
            &root,
            &json!(1),
            &json!({"name":"screen_frame","arguments":{ARGUMENT:"bad"}}),
        )
        .await;
    assert_eq!(
        refused.value["error"]["code"], -32602,
        "bad read tokens cannot fall back"
    );
}

#[tokio::test]
async fn guarded_cli_and_read_only_sessions_never_offer_mutations() {
    let dir = tempfile::tempdir().unwrap();
    let mut root = root(dir.path());
    root.views.store(false, Ordering::SeqCst);
    let names = root
        .tools()
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "atlas_status",
            "show_screen",
            "take_screenshot",
            "screen_frame"
        ]
    );
    root.views.store(true, Ordering::SeqCst);
    root.read_only = true;
    for tool in root.tools().as_array().unwrap() {
        assert!(LOOKING.contains(&tool["name"].as_str().unwrap()));
    }
    let registry = root.app_views.get().unwrap();
    let reply = registry
        .call(
            &root,
            &json!(1),
            &json!({"name":"show_screen","arguments":{"_atlasOpen":true}}),
        )
        .await;
    assert!(reply.value["result"]["_meta"][META].is_null());
    assert!(registry.is_empty());
}

#[tokio::test]
async fn contexts_are_bounded_and_tokens_cannot_cross_processes_or_expiry() {
    let dir = tempfile::tempdir().unwrap();
    let root = root(dir.path());
    let registry = root.app_views.get().unwrap();
    let mut tokens = Vec::new();
    for _ in 0..LIMIT {
        let (_, reservation, token) = registry.reserve(&root, None, true, false).unwrap();
        tokens.push(token.unwrap());
        drop(reservation);
    }
    assert!(registry.reserve(&root, None, true, false).is_none());
    assert!(
        Registry::default()
            .reserve(&root, Some(&json!(tokens[0])), false, false)
            .is_none()
    );
    registry.locked()[0].used = Instant::now() - IDLE - Duration::from_secs(1);
    assert!(
        registry
            .reserve(&root, Some(&json!(tokens[0])), false, false)
            .is_none()
    );
    registry.sweep(false).await;
    assert_eq!(registry.locked().len(), LIMIT - 1);
    assert!(registry.reserve(&root, None, true, false).is_some());
}

#[tokio::test]
async fn expiry_waits_for_inflight_acquisition_and_retains_unknown_outcomes() {
    let dir = tempfile::tempdir().unwrap();
    let root = root(dir.path());
    let registry = root.app_views.get().unwrap();
    let (_, reservation, token) = registry.reserve(&root, None, true, true).unwrap();
    registry.locked()[0].used = Instant::now() - IDLE - Duration::from_secs(1);
    registry.sweep(false).await;
    assert_eq!(registry.locked().len(), 1);
    assert!(
        registry
            .reserve(&root, Some(&json!(token)), false, false)
            .is_none()
    );
    drop(reservation); // cancelled acquire: Held is empty but outcome is unknown
    registry.sweep(true).await;
    assert_eq!(registry.locked().len(), 1);
    assert!(registry.locked()[0].usage.uncertain.load(Ordering::SeqCst));
}

#[tokio::test]
async fn response_delivery_and_teardown_preserve_other_views() {
    let dir = tempfile::tempdir().unwrap();
    let root = root(dir.path());
    let registry = root.app_views.get().unwrap();
    let (first, reservation, token) = registry.reserve(&root, None, true, true).unwrap();
    let (second, second_reservation, _) = registry.reserve(&root, None, true, false).unwrap();
    assert!(!Arc::ptr_eq(&first, &second));
    assert!(!std::ptr::eq(&first.held, &second.held));
    let mut delivered = Reply {
        value: json!({}),
        reservation: Some(reservation),
    };
    delivered.delivered();
    drop(delivered);
    let mut closed = registry
        .call(
            &root,
            &json!(1),
            &json!({"name":"view_close","arguments":{ARGUMENT:token}}),
        )
        .await;
    registry.sweep(false).await;
    assert_eq!(
        registry.locked().len(),
        2,
        "reply handling retains its reservation"
    );
    closed.delivered();
    drop(closed);
    registry.sweep(false).await;
    assert_eq!(registry.locked().len(), 1);
    assert!(Arc::ptr_eq(&registry.locked()[0].child, &second));
    drop(second_reservation);
    registry.sweep(true).await;
    assert!(registry.is_empty(), "EOF cleans settled contexts");

    let (_, reservation, _) = registry.reserve(&root, None, true, true).unwrap();
    drop(Reply {
        value: json!({}),
        reservation: Some(reservation),
    });
    registry.sweep(true).await;
    assert!(
        !registry.is_empty(),
        "a failed response write cannot claim acknowledgement"
    );
}

/// How the fixture viewer answers an acquire.
#[derive(Clone, Copy, PartialEq)]
enum Acquire {
    Granted,
    WithoutLease,
    Unreadable,
}

/// A menu bar viewer recorded for `root`, whose status read fails while
/// `status_fails` is set and whose control requests are recorded.
struct Viewer {
    status_fails: Arc<AtomicBool>,
    acquire: Arc<Mutex<Acquire>>,
    acquired: Arc<Mutex<Vec<String>>>,
    released: Arc<Mutex<Vec<String>>>,
    serving: tokio::task::JoinHandle<()>,
}

impl Viewer {
    async fn serve(root: &mut Server) -> Self {
        use axum::{
            Json, Router,
            http::StatusCode,
            routing::{get, post},
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        root.port = listener.local_addr().unwrap().port();
        let output = root.client.output_dir().unwrap().to_path_buf();
        std::fs::create_dir_all(&output).unwrap();
        let url = format!("http://127.0.0.1:{}/viewer/", root.port);
        std::fs::write(
            output.join(crate::viewer::MENUBAR_RECORD),
            json!({"url": url, "pid": std::process::id()}).to_string(),
        )
        .unwrap();

        let status_fails = Arc::new(AtomicBool::new(true));
        let acquire = Arc::new(Mutex::new(Acquire::Granted));
        let acquired = Arc::new(Mutex::new(Vec::new()));
        let released = Arc::new(Mutex::new(Vec::new()));
        let png = atlas_native::native::capture::sample_png(64, 36);
        let app = Router::new()
            .route(
                "/viewer/",
                get(|| async { Json(json!({"service": "atlas-viewer", "pid": std::process::id()})) }),
            )
            .route(
                "/viewer/status.json",
                get({
                    let status_fails = status_fails.clone();
                    move || async move {
                        if status_fails.load(Ordering::SeqCst) {
                            (StatusCode::SERVICE_UNAVAILABLE, Json(json!({})))
                        } else {
                            (
                                StatusCode::OK,
                                Json(json!({"overview": {"state": "idle", "label": "Idle", "headline": "Waiting"}})),
                            )
                        }
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
                                ("x-display-width", "64"),
                                ("x-display-height", "36"),
                            ],
                            png,
                        )
                    }
                }),
            )
            .route(
                "/viewer/control/acquire",
                post({
                    let (acquire, acquired) = (acquire.clone(), acquired.clone());
                    move || async move {
                        let mode = *acquire.lock().unwrap();
                        let mut acquired = acquired.lock().unwrap();
                        let lease = format!("lease-{}", acquired.len());
                        acquired.push(lease.clone());
                        match mode {
                            Acquire::Granted => json!({"ok": true, "lease": lease}).to_string(),
                            Acquire::WithoutLease => json!({"ok": true}).to_string(),
                            Acquire::Unreadable => "not json".to_owned(),
                        }
                    }
                }),
            )
            .route(
                "/viewer/control/release",
                post({
                    let released = released.clone();
                    move |Json(body): Json<Value>| async move {
                        released
                            .lock()
                            .unwrap()
                            .push(body["lease"].as_str().unwrap().to_owned());
                        Json(json!({"ok": true}))
                    }
                }),
            );
        Self {
            status_fails,
            acquire,
            acquired,
            released,
            serving: tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }),
        }
    }
}

impl Drop for Viewer {
    fn drop(&mut self) {
        self.serving.abort();
    }
}

/// Call `tool` in the view whose authority is `token`, with its reply
/// written, and return its result.
async fn delivered(registry: &Registry, root: &Server, tool: &str, token: &str) -> Value {
    let mut reply = registry
        .call(
            root,
            &json!(1),
            &json!({"name": tool, "arguments": {ARGUMENT: token}}),
        )
        .await;
    reply.delivered();
    reply.value["result"].clone()
}

/// Open a view and return its authority.
async fn opened(registry: &Registry, root: &Server) -> String {
    let mut reply = registry
        .call(
            root,
            &json!(1),
            &json!({"name":"show_screen","arguments":{OPEN:true}}),
        )
        .await;
    reply.delivered();
    reply.value["result"]["_meta"][META]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn a_failed_status_read_after_an_acknowledged_release_does_not_consume_a_view_slot() {
    let dir = tempfile::tempdir().unwrap();
    let mut root = root(dir.path());
    let viewer = Viewer::serve(&mut root).await;
    let registry = root.app_views.get().unwrap();

    for lifecycle in 0..=LIMIT {
        let token = opened(registry, &root).await;
        viewer.status_fails.store(false, Ordering::SeqCst);
        let taken = delivered(registry, &root, "take_control", &token).await;
        assert!(taken["isError"].is_null(), "{taken}");

        viewer.status_fails.store(true, Ordering::SeqCst);
        let given = delivered(registry, &root, "give_back", &token).await;
        assert_eq!(given["isError"], true);
        assert_eq!(given["structuredContent"]["error"]["code"], "menubar");
        assert_eq!(given["structuredContent"]["error"]["uncertain"], false);
        let lease = format!("lease-{lifecycle}");
        assert_eq!(viewer.released.lock().unwrap().last(), Some(&lease));

        delivered(registry, &root, "view_close", &token).await;
        registry.sweep(false).await;
        assert!(
            registry.is_empty(),
            "lifecycle {lifecycle}: a closed view with no lease frees its slot"
        );
    }
    assert_eq!(viewer.released.lock().unwrap().len(), LIMIT + 1);
}

#[tokio::test]
async fn a_failed_status_read_after_an_acknowledged_acquire_still_releases_its_known_lease() {
    let dir = tempfile::tempdir().unwrap();
    let mut root = root(dir.path());
    let viewer = Viewer::serve(&mut root).await;
    let registry = root.app_views.get().unwrap();
    let token = opened(registry, &root).await;
    let other = opened(registry, &root).await;

    let taken = delivered(registry, &root, "take_control", &token).await;
    assert_eq!(taken["isError"], true);
    assert_eq!(taken["structuredContent"]["error"]["code"], "menubar");
    assert_eq!(*viewer.acquired.lock().unwrap(), ["lease-0"]);

    delivered(registry, &root, "view_close", &token).await;
    registry.sweep(false).await;
    assert_eq!(*viewer.released.lock().unwrap(), ["lease-0"]);
    assert_eq!(registry.locked().len(), 1, "only the closed view ends");
    assert!(
        registry
            .reserve(&root, Some(&json!(other)), false, false)
            .is_some()
    );
}

#[tokio::test]
async fn an_acquire_whose_acknowledgement_is_unknown_keeps_its_view() {
    for mode in [Acquire::WithoutLease, Acquire::Unreadable] {
        let dir = tempfile::tempdir().unwrap();
        let mut root = root(dir.path());
        let viewer = Viewer::serve(&mut root).await;
        *viewer.acquire.lock().unwrap() = mode;
        let registry = root.app_views.get().unwrap();
        let token = opened(registry, &root).await;

        let taken = delivered(registry, &root, "take_control", &token).await;
        assert_eq!(taken["isError"], true);
        assert_eq!(taken["structuredContent"]["error"]["uncertain"], true);
        assert_eq!(viewer.acquired.lock().unwrap().len(), 1);

        delivered(registry, &root, "view_close", &token).await;
        registry.sweep(true).await;
        assert_eq!(registry.locked().len(), 1, "an unknown acquire is retained");
        assert!(registry.locked()[0].usage.uncertain.load(Ordering::SeqCst));

        // A later acknowledged release never clears that unknown acquire.
        let child = registry.locked()[0].child.clone();
        *child.held.lock().await = Some(Held {
            url: format!("http://127.0.0.1:{}/viewer/", root.port),
            lease: "known".into(),
            kept: Instant::now(),
        });
        registry.sweep(true).await;
        assert_eq!(*viewer.released.lock().unwrap(), ["known"]);
        assert_eq!(registry.locked().len(), 1);
    }
}

#[tokio::test]
async fn refused_cleanup_keeps_its_lease_and_retries_without_touching_another_view() {
    use axum::{Router, routing::post};
    let dir = tempfile::tempdir().unwrap();
    let root = root(dir.path());
    let registry = root.app_views.get().unwrap();
    let (first, reservation, _) = registry.reserve(&root, None, true, false).unwrap();
    let (second, other, _) = registry.reserve(&root, None, true, false).unwrap();
    drop(reservation);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let refuse = Arc::new(AtomicBool::new(true));
    let response = refuse.clone();
    let app = Router::new().route(
        "/view/control/release",
        post(move || {
            let response = response.clone();
            async move {
                if response.load(Ordering::SeqCst) {
                    (
                        axum::http::StatusCode::SERVICE_UNAVAILABLE,
                        axum::Json(
                            json!({"ok":false,"code":"busy","error":"fixture refuses release"}),
                        ),
                    )
                } else {
                    (axum::http::StatusCode::OK, axum::Json(json!({"ok":true})))
                }
            }
        }),
    );
    let serving = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    *first.held.lock().await = Some(Held {
        url: format!("http://{address}/view/"),
        lease: "fixture-first".into(),
        kept: Instant::now(),
    });
    *second.held.lock().await = Some(Held {
        url: "http://127.0.0.1:1/other/".into(),
        lease: "fixture-second".into(),
        kept: Instant::now(),
    });
    registry.locked()[0].closing = true;
    registry.sweep(false).await;
    assert_eq!(
        first.held.lock().await.as_ref().unwrap().lease,
        "fixture-first"
    );
    assert_eq!(registry.locked().len(), 2);
    refuse.store(false, Ordering::SeqCst);
    registry.sweep(false).await;
    assert!(first.held.lock().await.is_none());
    assert_eq!(registry.locked().len(), 1);
    assert_eq!(
        second.held.lock().await.as_ref().unwrap().lease,
        "fixture-second"
    );
    drop(other);
    serving.abort();
    let _ = serving.await;
}
