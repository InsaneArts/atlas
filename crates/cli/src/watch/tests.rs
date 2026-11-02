use super::*;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

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
    use atlas_core::host::Host;
    let (own, other) = (Key::of(Host::Claude, "own"), Key::of(Host::Codex, "other"));
    let status = json!({"ready": true, "overview": {"state": "idle", "hostSessions": [
        {"key": own, "name": "Claude Code · left", "hostSession": "own"},
        {"key": other, "name": "Codex · right", "hostSession": "other"},
    ]}});

    let line = Watch::new(None)
        .for_session(Some(own))
        .fixture_news(&status, "helper", None)
        .unwrap();
    let sessions = line["overview"]["hostSessions"].as_array().unwrap();
    assert_eq!(sessions[0]["hostSession"], "own");
    assert!(sessions[1].get("hostSession").is_none());
    assert_eq!(sessions[1]["name"], "Codex · right");

    let line = Watch::new(None)
        .fixture_news(&status, "helper", None)
        .unwrap();
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
    use atlas_core::host::Host;
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
        .fixture_news(&status("waiting", 1, 0), "helper", None)
        .unwrap();
    let sessions = first["overview"]["hostSessions"].as_array().unwrap();
    assert_eq!(sessions[0]["hostSession"], "own", "its own session is told");
    assert!(sessions[1].get("hostSession").is_none());
    assert_eq!(sessions[1]["waitingSeconds"], 1);

    assert!(
        watch
            .fixture_news(&status("waiting", 9, 7), "helper", None)
            .is_none(),
        "only ages changed"
    );
    assert!(
        watch
            .fixture_news(&status("idle", 9, 7), "helper", None)
            .is_some(),
        "a state change is news"
    );
}

#[tokio::test]
async fn a_watch_keeps_its_own_session_beside_the_listed_ones() {
    use atlas_client::host_session::store;
    use atlas_core::host::{Host, HostSession};

    let directory = tempfile::tempdir().unwrap();
    let client = atlas_client::test_support::client(directory.path());
    let own = HostSession::new(Host::Claude, "own").unwrap();
    store::note(
        &client,
        &own,
        store::Kind::Other,
        Some(&atlas_client::doing::Doing::new("click")),
        &Ok(json!({"ok": true})),
    )
    .unwrap();
    let watch = Watch::new(None).for_session(Some(own.key().clone()));

    let mut listed = json!({"overview": {"hostSessions": [
        {"key": own.key(), "name": "Claude Code · left", "state": "acting"},
    ]}});
    watch.fixture_own_session(&client, &mut listed).await;
    assert_eq!(listed["overview"]["ownSession"]["state"], "acting");

    let mut cut = json!({"overview": {"hostSessions": []}});
    watch.fixture_own_session(&client, &mut cut).await;
    assert_eq!(
        cut["overview"]["ownSession"]["key"],
        own.key().as_str(),
        "found among every live session when the list left it out"
    );
    assert_eq!(cut["overview"]["ownSession"]["state"], "idle");

    let mut none = json!({"overview": {}});
    Watch::new(None)
        .fixture_own_session(&client, &mut none)
        .await;
    assert!(none["overview"].get("ownSession").is_none());
}

#[test]
fn a_status_is_said_first_then_only_when_what_it_says_changes() {
    let mut watch = Watch::new(None);
    let first = watch.fixture_news(&status("working", "clicked Save", 1), "helper", mark(10));
    assert_eq!(first.as_ref().unwrap()["type"], "status");
    assert_eq!(first.as_ref().unwrap()["source"], "helper");
    assert_eq!(first.unwrap()["overview"]["headline"], "clicked Save");

    assert_eq!(
        watch.fixture_news(&status("working", "clicked Save", 9), "helper", mark(10)),
        None,
        "an age is not news"
    );
    assert!(
        watch
            .fixture_news(&status("working", "clicked Save", 1), "helper", mark(20))
            .is_some(),
        "the same action again is news"
    );
    assert!(
        watch
            .fixture_news(&status("idle", "clicked Save", 1), "helper", mark(20))
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
        watch.fixture_news(&status(state, "", 0), "viewer", None);
        assert_eq!(watch.frame_due(start), due, "{state:?}");
    }

    watch.framed_at = Some(start);
    watch.fixture_news(&status("working", "", 0), "viewer", None);
    assert!(!watch.frame_due(start + Duration::from_millis(500)));
    assert!(watch.frame_due(start + FRAME_EVERY));
    watch.fixture_news(&status("idle", "", 0), "viewer", None);
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
                .fixture_news(&json!({"overview": overview}), "viewer", None)
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
    let client = atlas_client::test_support::client_in(dir.path());
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
    let png = atlas_native::native::capture::sample_png(8, 8);
    let app = Router::new()
        .route(
            "/viewer/",
            get(|| async { Json(json!({"service": "atlas-viewer", "pid": std::process::id()})) }),
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
    atlas_core::files::private_folder(&frames, Code::Usage).unwrap();
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

    fs::write(dir.path().join(atlas_client::activity_log::FILE), "{}\n").unwrap();
    let first = log_mark(dir.path());
    fs::write(
        dir.path().join(atlas_client::activity_log::FILE),
        "{}\n{}\n",
    )
    .unwrap();
    assert_ne!(log_mark(dir.path()), first);
}

#[test]
fn frames_go_only_to_a_private_folder() {
    let dir = tempfile::tempdir().unwrap();
    let made = dir.path().join("frames");
    atlas_core::files::private_folder(&made, Code::Usage).unwrap();
    assert_eq!(fs::metadata(&made).unwrap().mode() & 0o777, 0o700);

    let open = dir.path().join("open");
    fs::create_dir(&open).unwrap();
    fs::set_permissions(&open, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        atlas_core::files::private_folder(&open, Code::Usage)
            .unwrap_err()
            .code,
        "usage"
    );
}
#[tokio::test]
async fn a_resumed_session_is_own_before_its_first_action() {
    use atlas_client::host_session::store;
    use atlas_core::{
        host::{Host, HostSession},
        process::Instance,
    };
    let directory = tempfile::tempdir().unwrap();
    let client = atlas_client::test_support::client(directory.path());
    let pid = nix::unistd::getppid().as_raw();
    let process = Instance::new(pid, atlas_core::host::ancestry::process(pid).unwrap().start);
    let own = HostSession::new(Host::Claude, "resumed")
        .unwrap()
        .run_by(Some(process));
    store::touch(&client, &own).unwrap();
    let watch = Watch::new(None).for_session(Some(own.key().clone()));
    let began = Instant::now();
    let mut status = json!({"overview": {"hostSessions": []}});
    loop {
        watch.fixture_own_session(&client, &mut status).await;
        if status["overview"]["ownSession"]["live"] == true {
            break;
        }
        assert!(began.elapsed() < Duration::from_secs(2));
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let entry = &status["overview"]["ownSession"];
    assert_eq!(entry["state"], "idle");
    assert!(entry["ended"].is_null());
    assert!(
        atlas_client::host_session::store::Store::at(client.output_dir().unwrap())
            .read(own.key())
            .unwrap()
            .counts
            .is_empty()
    );
}

#[tokio::test]
async fn a_dead_host_cannot_be_revived_by_a_watchs_environment() {
    use atlas_client::host_session::store;
    use atlas_core::{
        host::{Host, HostSession},
        process::Instance,
    };
    let directory = tempfile::tempdir().unwrap();
    let client = atlas_client::test_support::client(directory.path());
    let own = HostSession::new(Host::Codex, "dead")
        .unwrap()
        .run_by(Some(Instance::new(i32::MAX, [1, 0])));
    store::note(
        &client,
        &own,
        store::Kind::Other,
        Some(&atlas_client::doing::Doing::new("click")),
        &Ok(json!({"ok": true})),
    )
    .unwrap();
    let path = client
        .output_dir()
        .unwrap()
        .join("sessions")
        .join(format!("{}.json", own.key()));
    let before = fs::read(&path).unwrap();
    let watch = Watch::new(None).for_session(Some(own.key().clone()));
    let mut status = json!({"overview": {"hostSessions": []}});
    watch.fixture_own_session(&client, &mut status).await;
    assert!(status["overview"]["ownSession"].is_null());
    assert_eq!(fs::read(path).unwrap(), before);
}

#[tokio::test]
async fn an_ended_causers_own_entry_has_no_live_flag() {
    use atlas_core::host::{Host, Key};
    let directory = tempfile::tempdir().unwrap();
    let client = atlas_client::test_support::client(directory.path());
    let own = Key::of(Host::Codex, "dead");
    let watch = Watch::new(None).for_session(Some(own.clone()));
    for (state, work, ended, live) in [
        ("needsYou", false, json!(100), false),
        ("acting", true, json!(100), false),
        ("acting", true, Value::Null, true),
    ] {
        let mut status = json!({"overview": {"hostSessions": [{"key": own, "state": state, "live": live, "active": live, "ended": ended, "workInProgress": work, "holdsHelper": false}]}});
        watch.fixture_own_session(&client, &mut status).await;
        let entry = &status["overview"]["ownSession"];
        assert_eq!(entry["live"], live);
        assert_eq!(entry["ended"], ended);
        assert_eq!(entry["workInProgress"], work);
    }
}

#[test]
fn watch_preserves_unknown_future_state_without_inventing_readiness() {
    let dir = tempfile::tempdir().unwrap();
    let start = Instant::now();
    for (state, accepted, frames) in [
        (json!("futureBusy"), true, true),
        (json!(""), true, false),
        (Value::Null, false, false),
        (json!(7), false, false),
        (json!(false), false, false),
    ] {
        let reading = Reading::decode(json!({"overview": {"state": state, "future": 3}}));
        assert_eq!(reading.state.is_string(), accepted);
        let mut watch = Watch::new(Some(dir.path().to_path_buf()));
        let line = watch.news(&reading, "viewer", None).unwrap();
        assert_eq!(line["overview"]["state"], state);
        assert_eq!(line["overview"]["future"], 3);
        assert_eq!(line["ready"], false);
        assert_eq!(watch.frame_due(start), frames);
        watch.framed_at = Some(start);
        assert!(!watch.frame_due(start + FRAME_EVERY));
        assert_eq!(watch.frame_due(start + IDLE_FRAME_EVERY), frames);
    }
}

#[test]
fn frame_cadence_uses_the_retained_state_instead_of_the_projection() {
    let dir = tempfile::tempdir().unwrap();
    let start = Instant::now();
    let mut reading = Reading::decode(json!({"ready": false, "overview": {"state": "working"}}));
    reading.value["ready"] = json!(true);
    reading.value["overview"]["state"] = json!("notReady");
    let mut watch = Watch::new(Some(dir.path().to_path_buf()));
    let line = watch.news(&reading, "viewer", None).unwrap();
    assert_eq!(line["ready"], false);
    assert!(watch.frame_due(start));
    watch.framed_at = Some(start);
    watch.said["overview"]["state"] = json!("idle");
    assert!(watch.frame_due(start + FRAME_EVERY));
}

#[tokio::test]
async fn an_own_session_uses_the_decoded_key_instead_of_the_projection() {
    let dir = tempfile::tempdir().unwrap();
    let client = atlas_client::test_support::client(dir.path());
    let own = Key::of(atlas_core::host::Host::Codex, "own");
    let mut reading = Reading::decode(json!({"overview": {"hostSessions": [
        {"key": own, "state": "futureBusy", "extension": true}
    ]}}));
    reading.value["overview"]["hostSessions"] = json!([]);
    Watch::new(None)
        .for_session(Some(own.clone()))
        .add_own_session(&client, &mut reading)
        .await;
    assert_eq!(
        reading.value["overview"]["ownSession"],
        json!({
            "key": own, "state": "futureBusy", "extension": true
        })
    );
}
