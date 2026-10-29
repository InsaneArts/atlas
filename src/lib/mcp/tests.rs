use super::*;

pub(super) fn server(dir: &std::path::Path) -> Server {
    // The controller's notes (a Stop among them) stay in this test's folder.
    Server {
        client: crate::test_support::client_in(dir),
        config: dir.join("u.toml"),
        port: 1,
        read_only: false,
        app_views: OnceLock::new(),
        views: AtomicBool::new(false),
        last: Mutex::new(None),
        framed: Mutex::new(None),
        shared: Mutex::new(None),
        held: Mutex::new(None),
        icons: Icons::default(),
        host_session: None,
    }
}

async fn ask(server: &Server, method: &str, params: Value) -> Value {
    let request = json!({"jsonrpc": "2.0", "id": 7, "method": method, "params": params});
    let mut reply = server.answer(&request).await.unwrap();
    reply.delivered();
    reply.value
}

#[tokio::test]
async fn maintenance_ticks_never_lose_a_partially_read_request() {
    let (mut writer, reader) = tokio::io::duplex(256);
    let sending = tokio::spawn(async move {
        writer.write_all(b"{\"id\":").await.unwrap();
        tokio::time::sleep(Duration::from_millis(35)).await;
        writer.write_all(b"1,\"method\":\"ping\"}\n").await.unwrap();
    });
    let mut reader = BufReader::new(reader);
    let mut ticks = tokio::time::interval(Duration::from_millis(5));
    let mut swept = 0;
    let line = next_with_maintenance(&mut reader, &mut ticks, || swept += 1)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(swept >= 2);
    assert_eq!(
        serde_json::from_str::<Value>(&line).unwrap(),
        json!({"id":1,"method":"ping"})
    );
    sending.await.unwrap();
}

fn names(listed: &Value) -> Vec<String> {
    listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn cursor_imported_connections_require_private_authority_without_a_launcher_flag() {
    for name in ["cursor-vscode", "Cursor", "cursor-agent", "cursor-cli"] {
        let dir = tempfile::tempdir().unwrap();
        let server = server(dir.path());
        let initialize = json!({"id":1,"method":"initialize","params":{
            "clientInfo":{"name":name,"version":"1.0.0"},
            "capabilities":{"extensions":{UI_EXTENSION:{}}}
        }});
        // Transport applies this before spawning initialize's answer, so
        // even a pipelined call must use the restricted route.
        server.prepare(&initialize);
        let listed = ask(&server, "tools/list", json!({})).await;
        for tool in listed["result"]["tools"].as_array().unwrap() {
            assert!(tool["inputSchema"]["properties"][guarded::ARGUMENT].is_object());
            let tool = tool["name"].as_str().unwrap();
            if !LOOKING.contains(&tool) {
                let refused = ask(&server, "tools/call", json!({"name":tool,"arguments":{}})).await;
                assert_eq!(refused["error"]["code"], -32602, "{name}: {tool}");
            }
        }
        assert!(server.app_views.get().unwrap().is_empty());
        // Reinitialization may restrict available tools further, but can
        // never drop the private-view requirement.
        ask(
            &server,
            "initialize",
            json!({"clientInfo":{"name":"another host"}}),
        )
        .await;
        assert!(server.app_views.get().is_some());
        assert_eq!(
            names(&ask(&server, "tools/list", json!({})).await),
            ["atlas_status", "show_screen", "screen_frame"]
        );
    }
}

#[tokio::test]
async fn other_hosts_keep_their_existing_negotiation_and_explicit_guards_stay_enabled() {
    for name in ["claude", "codex", "Claude Desktop", "unknown"] {
        let dir = tempfile::tempdir().unwrap();
        let server = server(dir.path());
        let params =
            json!({"clientInfo":{"name":name},"capabilities":{"extensions":{UI_EXTENSION:{}}}});
        ask(&server, "initialize", params.clone()).await;
        assert!(server.app_views.get().is_none());
        let listed = ask(&server, "tools/list", json!({})).await;
        assert!(names(&listed).contains(&"take_control".to_owned()));
        assert!(
            listed["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .all(|tool| tool["inputSchema"]["properties"][guarded::ARGUMENT].is_null())
        );
        server
            .app_views
            .get_or_init(|| Arc::new(guarded::Registry::default()));
        ask(&server, "initialize", params).await;
        assert!(server.app_views.get().is_some());
    }
}

#[tokio::test]
async fn it_answers_in_the_clients_protocol_version_when_it_speaks_it() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path());

    let known = ask(
        &server,
        "initialize",
        json!({"protocolVersion": "2025-11-25"}),
    )
    .await;
    assert_eq!(known["id"], 7);
    assert_eq!(known["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(known["result"]["serverInfo"]["name"], "atlas");
    assert!(known["result"]["capabilities"]["tools"].is_object());

    let newer = ask(
        &server,
        "initialize",
        json!({"protocolVersion": "2099-01-01"}),
    )
    .await;
    assert_eq!(
        newer["result"]["protocolVersion"], VERSIONS[0],
        "its newest"
    );
}

#[tokio::test]
async fn resume_is_offered_only_to_a_client_that_keeps_view_tools_from_the_model() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path());

    ask(
        &server,
        "initialize",
        json!({"protocolVersion": "2025-06-18"}),
    )
    .await;
    let listed = ask(&server, "tools/list", json!({})).await;
    assert!(!names(&listed).contains(&"resume_agent".to_owned()));
    let refused = ask(&server, "tools/call", json!({"name": "resume_agent"})).await;
    assert_eq!(
        refused["error"]["code"], -32602,
        "a model that sees every tool cannot resume"
    );

    let views = json!({"extensions": {UI_EXTENSION: {"mimeTypes": [APP_MIME]}}});
    let started = json!({"protocolVersion": "2025-06-18", "capabilities": views});
    ask(&server, "initialize", started).await;
    let listed = ask(&server, "tools/list", json!({})).await;
    assert!(names(&listed).contains(&"resume_agent".to_owned()));
}

#[tokio::test]
async fn a_read_only_server_offers_only_what_looks() {
    let dir = tempfile::tempdir().unwrap();
    let mut server = server(dir.path());
    server.read_only = true;
    server.views.store(true, Ordering::Relaxed);

    let listed = ask(&server, "tools/list", json!({})).await;
    assert_eq!(
        names(&listed),
        [
            "atlas_status",
            "show_screen",
            "screen_frame",
            "activity",
            "helper_apps",
            "screenshots",
            "screenshot"
        ]
    );
    for name in ["stop_agent", "take_control", "send_input", "log_out_helper"] {
        let refused = ask(&server, "tools/call", json!({"name": name})).await;
        assert_eq!(refused["error"]["code"], -32602, "{name}");
    }
}

#[tokio::test]
async fn result_flags_match_offered_tools_on_successful_and_failed_calls() {
    use axum::{Json, Router, routing::get};

    for (views, read_only) in [(false, false), (false, true), (true, false), (true, true)] {
        let dir = tempfile::tempdir().unwrap();
        let mut server = server(dir.path());
        server.read_only = read_only;
        let capabilities = if views {
            json!({"extensions": {UI_EXTENSION: {"mimeTypes": [APP_MIME]}}})
        } else {
            json!({})
        };
        ask(&server, "initialize", json!({"capabilities": capabilities})).await;
        let listed = names(&ask(&server, "tools/list", json!({})).await);
        let resume = listed.contains(&"resume_agent".to_owned());
        let control = listed.contains(&"take_control".to_owned());
        assert_eq!(resume, views && !read_only);
        assert_eq!(control, views && !read_only);
        for (name, offered) in [("resume_agent", resume), ("take_control", control)] {
            if !offered {
                let refused = ask(&server, "tools/call", json!({"name": name})).await;
                assert_eq!(refused["error"]["code"], -32602);
                assert!(
                    refused.get("result").is_none(),
                    "an RPC refusal has no tool flags"
                );
            }
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        server.port = listener.local_addr().unwrap().port();
        let url = format!("http://127.0.0.1:{}/viewer/", server.port);
        let record = json!({"url": url, "pid": std::process::id()});
        std::fs::write(
            server
                .client
                .output_dir()
                .unwrap()
                .join(crate::viewer::MENUBAR_RECORD),
            record.to_string(),
        )
        .unwrap();
        let app = Router::new()
            .route("/viewer/", get(|| async {
                Json(json!({"service": "atlas-viewer", "pid": std::process::id()}))
            }))
            .route("/viewer/status.json", get(|| async {
                Json(json!({"overview": {"state": "idle", "label": "Idle", "headline": "Waiting"}}))
            }));
        let serving = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        for failed in [false, true] {
            if failed {
                let blocked = dir.path().join("not-a-directory");
                std::fs::write(&blocked, b"not a directory").unwrap();
                server.client.config.output_dir = blocked;
            }
            let called = ask(&server, "tools/call", json!({"name": "atlas_status"})).await;
            let result = &called["result"];
            assert_eq!(result["isError"] == true, failed);
            if failed {
                assert!(result["structuredContent"]["error"]["code"].is_string());
            } else {
                assert_eq!(result["structuredContent"]["status"]["state"], "idle");
            }
            assert_eq!(result["structuredContent"]["canResume"], resume);
            assert_eq!(result["structuredContent"]["canControl"], control);
            assert_eq!(result["structuredContent"]["control"], false);
        }
        serving.abort();
        let _ = serving.await;
    }
}

#[tokio::test]
async fn the_model_sees_the_status_and_the_view_and_only_the_view_calls_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path());
    server.views.store(true, Ordering::Relaxed);
    let listed = ask(&server, "tools/list", json!({})).await;
    let tools = listed["result"]["tools"].as_array().unwrap();
    let visibility = |name: &str| {
        let tool = tools.iter().find(|tool| tool["name"] == name).unwrap();
        tool["_meta"]["ui"]["visibility"].clone()
    };

    assert_eq!(
        visibility("atlas_status"),
        Value::Null,
        "the model's by default"
    );
    assert_eq!(visibility("show_screen"), json!(["model", "app"]));
    for name in [
        "screen_frame",
        "stop_agent",
        "resume_agent",
        "recover",
        "open_viewer",
        "take_control",
        "keep_control",
        "give_back",
        "send_input",
        "activity",
        "helper_apps",
        "screenshots",
        "screenshot",
        "switch_helper",
        "log_out_helper",
    ] {
        assert_eq!(visibility(name), json!(["app"]), "{name}");
    }

    let show = tools
        .iter()
        .find(|tool| tool["name"] == "show_screen")
        .unwrap();
    assert_eq!(show["_meta"]["ui"]["resourceUri"], SCREEN_URI);
    assert_eq!(
        show["_meta"]["ui/resourceUri"], SCREEN_URI,
        "for older hosts"
    );
    assert_eq!(
        show["_meta"]["openai/ui"]["entrypoints"][0]["type"],
        "thread"
    );
}

#[tokio::test]
async fn the_view_is_one_page_with_its_script_inline() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path());

    let listed = ask(&server, "resources/list", json!({})).await;
    assert_eq!(listed["result"]["resources"][0]["mimeType"], APP_MIME);

    let read = ask(&server, "resources/read", json!({"uri": SCREEN_URI})).await;
    let content = &read["result"]["contents"][0];
    let page = content["text"].as_str().unwrap();
    assert_eq!(content["mimeType"], APP_MIME);
    assert!(page.contains("ui/initialize") && !page.contains("/*SCRIPT*/"));
    assert!(
        !page.contains("http://") && !page.contains("https://"),
        "nothing loads from a network"
    );
    assert_eq!(content["_meta"]["ui"]["prefersBorder"], false);

    let missing = ask(
        &server,
        "resources/read",
        json!({"uri": "ui://atlas/other"}),
    )
    .await;
    assert_eq!(missing["error"]["code"], -32002);
}

#[tokio::test]
async fn notifications_get_no_answer_and_unknown_methods_and_tools_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path());

    let note = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
    assert!(server.answer(&note).await.is_none());
    let unknown = ask(&server, "sampling/createMessage", json!({})).await;
    assert_eq!(unknown["error"]["code"], -32601);
    assert_eq!(ask(&server, "ping", json!({})).await["result"], json!({}));
    let click = ask(&server, "tools/call", json!({"name": "click"})).await;
    assert_eq!(click["error"]["code"], -32602, "no such tool");
}

#[tokio::test]
async fn a_tool_that_fails_says_why_as_an_error_result() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path());
    server.views.store(true, Ordering::Relaxed);

    let recover = ask(&server, "tools/call", json!({"name": "recover"})).await;
    assert_eq!(recover["result"]["isError"], true, "no helper answers here");
    assert!(recover["result"]["structuredContent"]["error"]["code"].is_string());
}

#[tokio::test]
async fn a_refused_app_list_is_an_error_in_the_view_not_zero_apps() {
    use crate::test_support::{Helper, UID};

    let dir = tempfile::tempdir().unwrap();
    let mut server = server(dir.path());
    server.views.store(true, Ordering::Relaxed);
    let (client, helper) = Helper::client(dir.path(), UID);
    server.client = client;

    let listed = helper
        .serve(
            ask(&server, "tools/call", json!({"name": "helper_apps"})),
            |_| {
                Some(json!({"ok": false, "code": "busy", "uncertain": false,
                    "error": "The helper is serving too many connections"}))
            },
        )
        .await;

    assert_eq!(listed["result"]["isError"], true, "{listed}");
    assert_eq!(
        listed["result"]["structuredContent"]["error"]["code"],
        "busy"
    );
    assert!(listed["result"]["structuredContent"]["apps"].is_null());
}

#[tokio::test]
async fn a_status_read_that_began_earlier_never_replaces_a_later_one() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path());
    // A read that began after this one, such as the one a stop made.
    let later = Instant::now() + Duration::from_secs(60);
    *server.last.lock().await = Some((later, json!({"state": "stopped"})));

    assert!(server.status().await.is_ok());
    let (at, kept) = server.last.lock().await.clone().unwrap();
    assert_eq!(at, later);
    assert_eq!(kept["state"], "stopped");
}

#[tokio::test]
async fn a_screen_that_has_not_changed_reuses_its_frame() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path());
    let png = crate::native::capture::sample_png(640, 360);

    let first = server.frame_of(png.clone()).await.unwrap();
    let marker = json!({"id": "kept"});
    server.framed.lock().await.as_mut().unwrap().1 = marker.clone();
    assert_eq!(
        server.frame_of(png).await.unwrap(),
        marker,
        "not made again"
    );
    assert_eq!(first["mimeType"], "image/jpeg");
}

#[test]
fn the_model_sees_session_names_only_when_two_sessions_acted() {
    use crate::host_session::Host;
    let (claude, codex) = (Key::of(Host::Claude, "left"), Key::of(Host::Codex, "right"));
    let named = BTreeMap::from([
        (claude.clone(), "Claude Code · left".to_owned()),
        (codex.clone(), "Codex · right".to_owned()),
    ]);
    let status = |sessions: [&Key; 2]| {
        json!({"overview": {"activity": [
            {"says": "clicked Save", "actor": "agent", "ok": true, "ageSeconds": 1, "session": sessions[0]},
            {"says": "typed in TextEdit", "actor": "agent", "ok": true, "ageSeconds": 5, "session": sessions[1]},
            {"says": "sent 1 action", "actor": "viewer", "ok": true, "ageSeconds": 9, "session": null},
        ]}})
    };

    let alone = summary(&status([&claude, &claude]), &named, None);
    assert!(
        alone["activity"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row.get("session").is_none())
    );

    let two = summary(&status([&claude, &codex]), &named, None);
    assert_eq!(two["activity"][0]["session"], "Claude Code · left");
    assert_eq!(two["activity"][1]["session"], "Codex · right");
    assert!(two["activity"][2].get("session").is_none());
    let text = two.to_string() + &said(&two).to_string();
    assert!(
        !text.contains(claude.as_str()) && !text.contains(codex.as_str()),
        "{text}"
    );
    assert!(!text.contains("hostSession") && !text.contains("hostSessions"));
    assert_eq!(
        said(&two)["content"],
        said(&alone)["content"],
        "the model's words are as before"
    );
}

#[test]
fn the_models_text_is_unchanged_by_sessions() {
    use crate::host_session::Host;
    let (claude, codex) = (Key::of(Host::Claude, "left"), Key::of(Host::Codex, "right"));
    let named = BTreeMap::from([
        (claude.clone(), "Claude Code · left".to_owned()),
        (codex.clone(), "Codex · right".to_owned()),
    ]);
    let rows = json!([
        {"says": "clicked Save", "actor": "agent", "ok": true, "ageSeconds": 1, "session": claude},
        {"says": "didn't type in Notes", "actor": "agent", "ok": false, "ageSeconds": 3,
            "session": codex, "nothingSent": true},
    ]);
    let plain = json!({"overview": {"state": "working", "label": "Working",
        "headline": "Clicked Save", "activity": rows}});
    let mut with_sessions = plain.clone();
    with_sessions["overview"]["sessionsShown"] = json!(true);
    with_sessions["overview"]["holder"] = json!(claude);
    with_sessions["overview"]["hostSessions"] = json!([
        {"key": claude, "host": "claude", "hostLabel": "Claude Code", "name": "Claude Code · left",
            "hostSession": "left", "folder": "left", "state": "acting", "holdsHelper": true},
        {"key": codex, "host": "codex", "hostLabel": "Codex", "name": "Codex · right",
            "hostSession": "right", "state": "waiting", "waitingFor": "helper",
            "waitingSeconds": 4, "holdsHelper": false},
    ]);

    let before = summary(&plain, &named, None);
    let after = summary(&with_sessions, &named, Some(&claude));
    assert_eq!(said(&before)["content"], said(&after)["content"]);
    assert_eq!(before["sessionsShown"], false);
    assert_eq!(after["sessionsShown"], true);
    assert_eq!(after["holder"], claude.as_str());
    assert_eq!(after["self"], claude.as_str());
    assert_eq!(after["sessions"][1]["state"], "waiting");
    assert!(
        after["sessions"][1].get("waitingSeconds").is_none(),
        "only what the view draws"
    );
    assert_eq!(after["sessions"][1]["name"], "Codex · right");
    assert_eq!(after["activity"][1]["nothingSent"], true);
    assert_eq!(after["activity"][1]["session"], "Codex · right");
    let text = after.to_string();
    assert!(
        !text.contains("hostSession\"") && !text.contains("\"folder\""),
        "no host id or folder reaches the view: {text}"
    );
}

#[test]
fn rows_are_named_from_the_status_they_came_with_and_never_taken_for_terminal() {
    use crate::host_session::Host;
    let (first, gone) = (Key::of(Host::Codex, "first"), Key::of(Host::Pi, "gone"));
    let status = json!({"overview": {"sessionsShown": true, "activity": [
        {"says": "opened Notes", "actor": "agent", "ok": true, "ageSeconds": 1, "session": first},
        {"says": "clicked", "actor": "agent", "ok": true, "ageSeconds": 2, "session": gone},
        {"says": "ran a command", "actor": "agent", "ok": true, "ageSeconds": 3, "session": null},
    ], "hostSessions": [
        {"key": first, "host": "codex", "name": "Codex", "state": "acting", "holdsHelper": true},
    ]}});

    let summary = summary(&status, &BTreeMap::new(), None);
    assert_eq!(
        summary["activity"][0]["session"], "Codex",
        "a session on its first command has no record, but is listed"
    );
    assert_eq!(summary["activity"][1]["session"], "Another session");
    assert!(summary["activity"][2].get("session").is_none());
}

#[test]
fn the_view_learns_its_own_session_only_when_certain() {
    use crate::host_session::{Host, HostSession, Instance, record::Record};

    let claude = Instance::new(4242, [1_700_000_000, 5]);
    let record = |host: Host, id: &str, process: Option<Instance>, last_seen: u64| {
        let session = HostSession::new(host, id).unwrap();
        let mut record = Record::new(&session, last_seen, None);
        record.host_process = process;
        crate::host_session::record::Seen {
            record,
            copies_left: 0,
        }
    };
    let seen = [
        record(Host::Claude, "before-clear", Some(claude), 10),
        record(Host::Claude, "after-clear", Some(claude), 20),
        record(
            Host::Claude,
            "elsewhere",
            Some(Instance::new(7, [1, 0])),
            30,
        ),
    ];
    let own = HostSession::new(Host::Claude, "before-clear")
        .unwrap()
        .run_by(Some(claude));

    assert_eq!(
        own_session(Some(&own), &seen, 4242, false),
        Some(seen[1].record.key.clone()),
        "followed past /clear to its process's newest session"
    );
    assert_eq!(
        own_session(Some(&own), &seen, 99, false),
        None,
        "not started by that Claude Code"
    );
    assert_eq!(
        own_session(Some(&own), &[], 4242, false),
        None,
        "nothing recorded yet"
    );
    let codex = HostSession::new(Host::Codex, "shared").unwrap();
    assert_eq!(
        own_session(Some(&codex), &seen, 4242, false),
        None,
        "a host that shares one server among chats"
    );
    assert_eq!(own_session(None, &seen, 4242, false), None);

    let cleared = HostSession::new(Host::Claude, "after-another-clear")
        .unwrap()
        .run_by(Some(claude));
    assert_eq!(
        own_session(Some(&cleared), &seen, 4242, false),
        None,
        "a new conversation with only older records is not the newest of them"
    );
    assert_eq!(
        own_session(Some(&own), &seen, 4242, true),
        None,
        "a session on its first command may be this chat's newest"
    );
}

#[test]
fn the_page_carries_the_host_logos() {
    let page = screen_contents()["text"].as_str().unwrap().to_owned();
    assert!(!page.contains("/*LOGOS*/") && !page.contains("/*SCRIPT*/"));
    for host in ["claude", "codex", "cursor"] {
        assert!(
            page.contains(&format!("\"{host}\":\"data:image/png;base64,")),
            "{host}"
        );
    }
    assert!(page.len() < SCREEN.len() + SCREEN_SCRIPT.len() + 16 * 1024);
}

#[test]
fn mcp_actions_are_recorded_without_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path());
    assert_eq!(server.client.host_session, None);

    let mut acting = server
        .client
        .clone()
        .doing(crate::doing::Doing::new("input"));
    acting.actor = Actor::Viewer;
    acting.record_for_test(
        &json!({"op": "input", "actions": [{}]}),
        &json!({"ok": true}),
    );
    let log = std::fs::read_to_string(
        server
            .client
            .output_dir()
            .unwrap()
            .join(crate::activity_log::FILE),
    )
    .unwrap();
    assert!(!log.contains("session"), "{log}");
}

#[test]
fn a_status_reads_in_a_line_for_the_model_and_whole_for_the_view() {
    let status = summary(
        &json!({"overview": {
            "state": "working",
            "label": "Working",
            "headline": "Clicked Save in TextEdit",
            "accountName": "Atlas",
            "activity": [
                {"says": "clicked Save in TextEdit", "actor": "agent", "ok": true, "ageSeconds": 1, "verb": "click"},
                {"says": "sent 1 action", "actor": "viewer", "ok": true, "ageSeconds": 2},
                {"says": "opened TextEdit", "actor": "agent", "ok": true, "ageSeconds": 9},
            ],
        }}),
        &BTreeMap::new(),
        None,
    );
    assert_eq!(status["helper"], "Atlas");
    assert_eq!(
        status["activity"][0].get("verb"),
        None,
        "only what the view shows"
    );

    let step = summary(
        &json!({"overview": {"headline": "Run atlas start-input"}}),
        &BTreeMap::new(),
        None,
    );
    assert_eq!(
        step["headline"], "The helper isn't ready. Open Atlas to see why",
        "no commands for the person"
    );

    let result = said(&status);
    assert_eq!(
        result["content"][0]["text"],
        "Atlas: Working. Clicked Save in TextEdit. Recent: clicked Save in TextEdit; opened TextEdit."
    );
    assert_eq!(result["structuredContent"]["status"]["state"], "working");
    assert_eq!(result.get("_meta"), None, "no screen for the model");
}

#[test]
fn a_line_that_ends_with_an_ellipsis_gets_no_full_stop_after_it() {
    let status = json!({
        "state": "working",
        "label": "Working",
        "headline": "Running a command…",
        "activity": [
            {"actor": "agent", "says": "running a command…"},
            {"actor": "agent", "says": "opened TextEdit"},
        ],
    });
    assert_eq!(
        said(&status)["content"][0]["text"],
        "Atlas: Working. Running a command… Recent: running a command…; opened TextEdit."
    );

    let last = json!({
        "label": "Working",
        "headline": "Clicked Save",
        "activity": [{"actor": "agent", "says": "running a command…"}],
    });
    assert_eq!(
        said(&last)["content"][0]["text"],
        "Atlas: Working. Clicked Save. Recent: running a command…"
    );

    let quiet = json!({"label": "Idle", "headline": "", "activity": []});
    assert_eq!(said(&quiet)["content"][0]["text"], "Atlas: Idle.");
}

#[tokio::test]
async fn control_and_the_persons_actions_are_offered_only_to_a_client_that_draws_views() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path());

    let listed = names(&ask(&server, "tools/list", json!({})).await);
    for name in VIEW_ONLY {
        assert!(!listed.contains(&name.to_owned()), "{name}");
        let refused = ask(&server, "tools/call", json!({"name": name})).await;
        assert_eq!(refused["error"]["code"], -32602, "{name}");
    }

    server.views.store(true, Ordering::Relaxed);
    let listed = names(&ask(&server, "tools/list", json!({})).await);
    for name in VIEW_ONLY {
        assert!(listed.contains(&name.to_owned()), "{name}");
    }
}

#[tokio::test]
async fn input_without_control_is_refused_and_every_result_says_whether_control_is_ours() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path());
    server.views.store(true, Ordering::Relaxed);

    let click = json!([{"type": "pointer", "phase": "down", "x": 1, "y": 1}]);
    let sent = ask(
        &server,
        "tools/call",
        json!({"name": "send_input", "arguments": {"actions": click}}),
    )
    .await;
    assert_eq!(sent["result"]["isError"], true);
    assert_eq!(
        sent["result"]["structuredContent"]["error"]["code"],
        "lease-lost"
    );
    assert_eq!(sent["result"]["structuredContent"]["control"], false);
    assert_eq!(sent["result"]["structuredContent"]["canControl"], true);

    let kept = ask(&server, "tools/call", json!({"name": "keep_control"})).await;
    assert_eq!(kept["result"]["isError"], true, "nothing to keep");
}

#[test]
fn only_clicks_scrolls_typing_and_keys_go_to_the_helper_as_many_as_it_takes_at_once() {
    let click = json!([{"type": "pointer", "phase": "down", "x": 1, "y": 2}, {"type": "key", "key": "cmd+s"}]);
    assert!(checked(&click).is_ok());

    for refused in [
        json!([]),
        json!({"type": "key"}),
        json!([{"type": "wait", "ms": 60000}]),
        json!([{"type": "run"}]),
        Value::Array(vec![json!({"type": "key", "key": "a"}); ACTIONS + 1]),
    ] {
        assert_eq!(checked(&refused).unwrap_err().code, "input", "{refused}");
    }
}

#[tokio::test]
async fn a_screenshot_is_read_only_by_a_name_a_capture_wrote() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path());
    let output = server.client.output_dir().unwrap().to_path_buf();
    std::fs::create_dir_all(&output).unwrap();
    let png = crate::native::capture::sample_png(1280, 800);
    std::fs::write(output.join("5-capture.png"), &png).unwrap();
    std::os::unix::fs::symlink(output.join("5-capture.png"), output.join("6-capture.png")).unwrap();

    let thumb = server.screenshot("5-capture.png", false).await.unwrap();
    assert_eq!(thumb["_meta"][FRAME_META]["width"], 480);
    let full = server.screenshot("5-capture.png", true).await.unwrap();
    assert_eq!(full["_meta"][FRAME_META]["width"], 1024);

    for name in ["../u.toml", "6-capture.png", "7-capture.png"] {
        assert_eq!(
            server.screenshot(name, false).await.unwrap_err().code,
            "bad-request",
            "{name}"
        );
    }
}

fn held(lease: &str, kept: Instant) -> Held {
    // Nothing listens here, so releasing it fails at once.
    Held {
        url: "http://127.0.0.1:9/viewer/".to_owned(),
        lease: lease.to_owned(),
        kept,
    }
}

#[tokio::test]
async fn taking_control_returns_its_picture_and_input_keeps_the_displayed_frames_key() {
    use axum::{Json, Router, routing::get, routing::post};

    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path());
    let png = crate::native::capture::sample_png(640, 360);
    let expected = server.frame_of(png.clone()).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/viewer/", listener.local_addr().unwrap());
    let received = Arc::new(Mutex::new(Value::Null));
    let input = received.clone();
    let app = Router::new()
        .route(
            "/viewer/frame.png",
            get(move || {
                let png = png.clone();
                async move {
                    (
                        [
                            ("x-frame-id", "shown"),
                            ("x-display-width", "1280"),
                            ("x-display-height", "720"),
                        ],
                        png,
                    )
                }
            }),
        )
        .route(
            "/viewer/control/renew",
            post(|Json(body): Json<Value>| async move {
                assert_eq!(body["frame"], "shown");
                Json(json!({"ok": true}))
            }),
        )
        .route(
            "/viewer/input",
            post(move |Json(body): Json<Value>| async move {
                *input.lock().await = body;
                Json(json!({"ok": true}))
            }),
        );
    let serving = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    *server.held.lock().await = Some(Held {
        url,
        ..held("ours", Instant::now())
    });

    let taken = server.take_control().await.unwrap();
    let picture = &taken["_meta"][FRAME_META];
    assert_eq!(picture["id"], expected["id"]);
    assert_eq!(picture["data"], expected["data"]);
    assert_eq!(picture["key"], "shown");
    assert_eq!(
        taken["structuredContent"]["points"],
        json!({"width": 1280.0, "height": 720.0})
    );

    *server.shared.lock().await = Some(Seen {
        key: "unseen".to_owned(),
        at: Instant::now(),
    });
    let click = json!([{"type": "pointer", "phase": "down", "x": 1, "y": 1}]);
    server.send_input(&picture["key"], &click).await.unwrap();
    assert_eq!(received.lock().await["frame"], "shown");
    assert_eq!(
        server
            .send_input(&Value::Null, &click)
            .await
            .unwrap_err()
            .code,
        "stale-frame"
    );
    serving.abort();
}

#[tokio::test]
async fn a_late_failure_on_older_control_never_ends_control_taken_since() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path());
    let now = Instant::now();
    let lost = Err(Error::new(
        "lease-lost",
        "Native control was released or expired",
    ));

    *server.held.lock().await = Some(held("b", now));
    server.settle(&held("a", now), &lost, true).await;
    assert_eq!(server.held.lock().await.as_ref().unwrap().lease, "b");

    server
        .settle(&held("b", now), &Err(Error::new("busy", "busy")), true)
        .await;
    assert!(
        server.held.lock().await.is_some(),
        "a busy helper ends nothing"
    );
    let unanswered = Err(Error::new("menubar", "Atlas's menu bar didn't answer"));
    server.settle(&held("b", now), &unanswered, true).await;
    assert!(
        server.held.lock().await.is_some(),
        "an answer that never came ends nothing"
    );

    let long_ago = now - Duration::from_secs(5);
    *server.held.lock().await = Some(held("b", long_ago));
    server
        .settle(&held("b", long_ago), &Ok(json!({})), false)
        .await;
    assert_eq!(
        server.held.lock().await.as_ref().unwrap().kept,
        long_ago,
        "input keeps nothing"
    );
    server
        .settle(&held("b", long_ago), &Ok(json!({})), true)
        .await;
    assert!(
        server.held.lock().await.as_ref().unwrap().kept > long_ago,
        "a renewal keeps it"
    );

    server.settle(&held("b", now), &lost, true).await;
    assert!(
        server.held.lock().await.is_none(),
        "its own failure ends it"
    );
}

#[tokio::test]
async fn control_nobody_kept_lapses_and_is_let_go_of() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path());

    *server.held.lock().await = Some(held("a", Instant::now()));
    assert!(server.controlling().await);

    let long_ago = Instant::now() - LEASE - Duration::from_secs(1);
    *server.held.lock().await = Some(held("a", long_ago));
    assert!(!server.controlling().await);
    assert_eq!(server.holding().await.err().unwrap().code, "lease-lost");
    assert!(server.held.lock().await.is_none());
}

#[tokio::test]
async fn every_tool_only_the_view_may_call_is_private_in_openais_words_too() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path());
    server.views.store(true, Ordering::Relaxed);
    let listed = ask(&server, "tools/list", json!({})).await;

    for tool in listed["result"]["tools"].as_array().unwrap() {
        let app_only = tool["_meta"]["ui"]["visibility"] == json!(["app"]);
        assert_eq!(
            tool["_meta"]["openai/visibility"] == "private",
            app_only,
            "{}",
            tool["name"]
        );
    }
}

#[tokio::test]
async fn a_line_too_long_or_not_text_is_refused_and_the_session_goes_on() {
    let mut text = Vec::new();
    text.extend_from_slice(b"{\"a\":1}\n");
    text.extend(std::iter::repeat_n(b'x', MAX_LINE + 10));
    text.extend_from_slice(b"\n\xff\xfe\n{\"b\":2}\r\n{\"c\":3}");
    let mut input = tokio::io::BufReader::new(&text[..]);

    assert_eq!(
        next_request(&mut input).await.unwrap(),
        Some(Ok("{\"a\":1}".to_owned()))
    );
    let long = next_request(&mut input)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(long["error"]["code"], -32600);
    let bad = next_request(&mut input)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(bad["error"]["code"], -32700);
    assert_eq!(
        next_request(&mut input).await.unwrap(),
        Some(Ok("{\"b\":2}".to_owned()))
    );
    assert_eq!(
        next_request(&mut input).await.unwrap(),
        Some(Ok("{\"c\":3}".to_owned())),
        "the last line needs no newline"
    );
    assert_eq!(next_request(&mut input).await.unwrap(), None);
}

#[tokio::test]
async fn request_lines_have_the_same_payload_limit_for_every_ending_and_chunk_size() {
    let refused = json!({"jsonrpc": "2.0", "id": null,
        "error": {"code": -32600, "message": "Request too large"}});
    for length in [MAX_LINE - 1, MAX_LINE, MAX_LINE + 1] {
        for (ending, extra_payload) in [
            ("", 0),
            ("\n", 0),
            ("\r\n", 0),
            ("\r", 0),
            ("\r\r\n", 1),
            ("\r\r", 1),
        ] {
            let mut bytes = vec![b'x'; length];
            bytes.extend_from_slice(ending.as_bytes());
            if ending.ends_with('\n') {
                bytes.extend_from_slice(b"{}\n");
            }
            // MAX_LINE + 1 splits a maximum-sized payload's CR and LF.
            for capacity in [8192, MAX_LINE + 1, bytes.len() + 1] {
                let mut input = tokio::io::BufReader::with_capacity(capacity, &bytes[..]);
                let expected = if length + extra_payload > MAX_LINE {
                    Err(refused.clone())
                } else {
                    Ok("x".repeat(length))
                };
                assert!(
                    next_request(&mut input).await.unwrap() == Some(expected),
                    "payload {length}, ending {ending:?}, capacity {capacity}"
                );
                if ending.ends_with('\n') {
                    assert_eq!(
                        next_request(&mut input).await.unwrap(),
                        Some(Ok("{}".to_owned()))
                    );
                }
                assert_eq!(next_request(&mut input).await.unwrap(), None);
            }
        }
    }
}

#[tokio::test]
async fn request_lines_check_size_before_utf8_and_drain_a_large_chunk() {
    for length in [MAX_LINE, MAX_LINE + 1, MAX_LINE * 2] {
        for ending in ["", "\n", "\r\n", "\r"] {
            let mut bytes = vec![b'x'; length];
            bytes[0] = 0xff;
            bytes.extend_from_slice(ending.as_bytes());
            if ending.ends_with('\n') {
                bytes.extend_from_slice(b"{}\n");
            }
            let mut input = tokio::io::BufReader::with_capacity(bytes.len() + 1, &bytes[..]);
            let (code, message) = if length > MAX_LINE {
                (-32600, "Request too large")
            } else {
                (-32700, "Parse error")
            };
            assert_eq!(
                next_request(&mut input).await.unwrap(),
                Some(Err(json!({"jsonrpc": "2.0", "id": null,
                    "error": {"code": code, "message": message}}))),
                "payload {length}, ending {ending:?}"
            );
            if ending.ends_with('\n') {
                assert_eq!(
                    next_request(&mut input).await.unwrap(),
                    Some(Ok("{}".to_owned()))
                );
            }
            assert_eq!(next_request(&mut input).await.unwrap(), None);
        }
    }
}

#[tokio::test]
async fn request_lines_keep_blank_lines_distinct_from_eof() {
    for bytes in [b"".as_slice(), b"\n", b"\r\n", b"\r"] {
        let mut input = tokio::io::BufReader::with_capacity(1, bytes);
        let expected = (!bytes.is_empty()).then(|| Ok(String::new()));
        assert_eq!(next_request(&mut input).await.unwrap(), expected);
        assert_eq!(next_request(&mut input).await.unwrap(), None);
    }
}

#[test]
fn the_view_gets_each_windows_title_and_size_but_never_its_place_or_number() {
    let mut app = json!({"name": "Opie", "windows": [
        {"id": 12, "title": "Ring", "x": 10.0, "y": 40.0, "width": 520.0, "height": 860.0},
    ]});

    only_title_and_size(&mut app);

    assert_eq!(
        app["windows"],
        json!([{"title": "Ring", "width": 520.0, "height": 860.0}])
    );
}
