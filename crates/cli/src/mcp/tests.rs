use super::control::{ACTIONS, LEASE, LeaseRequest, checked};
use super::presentation::{only_title_and_size, own_session, said, summary};
use super::resources::{APP_MIME, FRAME_META, SCREEN, SCREEN_SCRIPT, STATUS_META, VERSIONS};
use super::*;
use atlas_client::{client::AppList, helper_info::open_apps};
use atlas_core::host::Key;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub(super) fn server(dir: &std::path::Path) -> Server {
    // The controller's notes (a Stop among them) stay in this test's folder.
    Server::new(
        atlas_client::test_support::client_in(dir),
        dir.join("u.toml"),
        1,
        false,
        None,
    )
}

async fn ask(server: &Server, method: &str, params: Value) -> Value {
    let request = json!({"jsonrpc": "2.0", "id": 7, "method": method, "params": params});
    server.prepare(&request);
    let mut reply = server.answer(&request).await.unwrap();
    reply.delivered();
    reply.value
}

#[tokio::test(start_paused = true)]
async fn maintenance_ticks_never_lose_a_partially_read_request() {
    use std::{
        cell::Cell,
        future::{Future, poll_fn},
        task::Poll,
    };

    let (mut writer, reader) = tokio::io::duplex(256);
    writer.write_all(b"{\"id\":").await.unwrap();
    let mut reader = BufReader::new(reader);
    let mut ticks = tokio::time::interval(Duration::from_millis(5));
    let swept = Cell::new(0);
    let next = next_with_maintenance(&mut reader, &mut ticks, || swept.set(swept.get() + 1));
    tokio::pin!(next);
    // Consume the prefix before advancing any maintenance deadline.
    poll_fn(|cx| {
        assert!(next.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    for _ in 0..2 {
        tokio::time::advance(Duration::from_millis(5)).await;
        poll_fn(|cx| {
            assert!(next.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
    }
    assert!(swept.get() >= 2);
    writer.write_all(b"1,\"method\":\"ping\"}\n").await.unwrap();
    let line = next.await.unwrap().unwrap().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&line).unwrap(),
        json!({"id":1,"method":"ping"})
    );
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
            [
                "atlas_status",
                "show_screen",
                "take_screenshot",
                "screen_frame"
            ]
        );
    }
}

#[tokio::test]
async fn an_older_initialize_answer_cannot_restore_capabilities_removed_by_newer_negotiation() {
    let apps = json!({"extensions": {UI_EXTENSION: {"mimeTypes": [APP_MIME]}}});
    let initialize = |id: i64, capabilities: &Value| {
        json!({"jsonrpc":"2.0","id":id,"method":"initialize","params":{
            "clientInfo":{"name":"cursor-agent"},
            "capabilities":capabilities
        }})
    };
    for (older, newer, views) in [(apps.clone(), json!({}), false), (json!({}), apps, true)] {
        let dir = tempfile::tempdir().unwrap();
        let server = Arc::new(server(dir.path()));
        let (older, newer) = (initialize(1, &older), initialize(2, &newer));
        // Transport ingestion prepares each in read order.
        server.prepare(&older);
        server.prepare(&newer);

        let (release, released) = tokio::sync::oneshot::channel::<()>();
        let late = tokio::spawn({
            let server = server.clone();
            async move {
                released.await.unwrap();
                server.answer(&older).await.unwrap()
            }
        });
        server.answer(&newer).await.unwrap();
        release.send(()).unwrap();
        let older = late.await.unwrap();
        assert_eq!(older.value["id"], 1);

        assert_eq!(server.draws_views(), views);
        assert!(server.app_views.get().is_some(), "Cursor stays restricted");
        let listed = names(&ask(&server, "tools/list", json!({})).await);
        assert_eq!(listed.contains(&"take_control".to_owned()), views);
        if !views {
            assert_eq!(
                listed,
                [
                    "atlas_status",
                    "show_screen",
                    "take_screenshot",
                    "screen_frame"
                ]
            );
            let refused = ask(&server, "tools/call", json!({"name":"take_control"})).await;
            assert_eq!(refused["error"]["code"], -32602);
        }
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
            "take_screenshot",
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
    use atlas_client::test_support::Helper;
    use atlas_core::test_support::UID;

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
    let png = atlas_native::native::capture::sample_png(640, 360);

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
    use atlas_core::host::Host;
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
    use atlas_core::host::Host;
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
    use atlas_core::host::Host;
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
    use atlas_client::host_session::record::Record;
    use atlas_core::{
        host::{Host, HostSession},
        process::Instance,
    };

    let claude = Instance::new(4242, [1_700_000_000, 5]);
    let record = |host: Host, id: &str, process: Option<Instance>, last_seen: u64| {
        let session = HostSession::new(host, id).unwrap();
        let mut record = Record::new(&session, last_seen, None);
        record.host_process = process;
        atlas_client::host_session::record::Seen {
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
        .doing(atlas_client::doing::Doing::new("input"));
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
            .join(atlas_client::activity_log::FILE),
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
    assert!(
        result["structuredContent"]["status"]
            .get("activity")
            .is_none(),
        "the model is not given the history it already has in the text"
    );
    assert_eq!(
        result["_meta"][STATUS_META], status,
        "the view gets it whole"
    );
    assert!(
        result["_meta"].get(FRAME_META).is_none(),
        "no screen for the model"
    );
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

#[test]
fn a_running_command_keeps_its_ellipsis_through_the_status_summary() {
    let status = summary(
        &json!({"overview": {
            "state": "working",
            "label": "Working",
            "headline": "Running a command…",
            "activity": [{"actor": "agent", "says": "running a command…"}],
        }}),
        &BTreeMap::new(),
        None,
    );
    let reply = said(&status);
    assert_eq!(
        reply["content"][0]["text"],
        "Atlas: Working. Running a command… Recent: running a command…"
    );
    assert_eq!(reply["_meta"][STATUS_META], status);
    assert_eq!(
        reply["structuredContent"]["status"]["headline"],
        "Running a command…"
    );
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

#[test]
fn personal_permission_is_decided_before_action_field_validation() {
    for action in [
        json!({"type":"pointer"}),
        json!({"type":"move","x":null}),
        json!({"type":"scroll","dy":"invalid"}),
        json!({"type":"type","text":false}),
        json!({"type":"key"}),
    ] {
        assert!(checked(&json!([action])).is_ok());
    }
    for action in [
        json!({"type":"click","x":1,"y":2}),
        json!({"type":"drag","fromX":1,"fromY":2,"toX":3,"toY":4}),
        json!({"type":"wait","ms":0}),
        json!({"type":"press","ref":"8123abc.12"}),
        json!({"type":"set-value","ref":"8123abc.12","value":"x"}),
        json!({"type":"future"}),
        json!({"type":null}),
        json!({}),
    ] {
        let Err(error) = checked(&json!([action])) else {
            panic!("an excluded action must be refused");
        };
        assert_eq!(error.code, Code::Input);
        assert_eq!(
            error.error,
            "Only clicks, scrolls, typing and keys may be sent"
        );
    }
}

#[tokio::test]
async fn a_screenshot_is_read_only_by_a_name_a_capture_wrote() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path());
    let output = server.client.output_dir().unwrap().to_path_buf();
    std::fs::create_dir_all(&output).unwrap();
    let png = atlas_native::native::capture::sample_png(1280, 800);
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
    let png = atlas_native::native::capture::sample_png(640, 360);
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
    server
        .send_input(picture["key"].as_str(), &click)
        .await
        .unwrap();
    assert_eq!(received.lock().await["frame"], "shown");
    assert_eq!(
        server.send_input(None, &click).await.unwrap_err().code,
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
        Code::LeaseLost,
        "Native control was released or expired",
    ));

    *server.held.lock().await = Some(held("b", now));
    server
        .settle(&held("a", now), &lost, LeaseRequest::Renew)
        .await;
    assert_eq!(server.held.lock().await.as_ref().unwrap().lease, "b");

    server
        .settle(
            &held("b", now),
            &Err(Error::new(Code::Busy, "busy")),
            LeaseRequest::Renew,
        )
        .await;
    assert!(
        server.held.lock().await.is_some(),
        "a busy helper ends nothing"
    );
    let unanswered = Err(Error::new(Code::Menubar, "Atlas's menu bar didn't answer"));
    server
        .settle(&held("b", now), &unanswered, LeaseRequest::Renew)
        .await;
    assert!(
        server.held.lock().await.is_some(),
        "an answer that never came ends nothing"
    );

    let long_ago = now - Duration::from_secs(5);
    *server.held.lock().await = Some(held("b", long_ago));
    server
        .settle(&held("b", long_ago), &Ok(json!({})), LeaseRequest::Input)
        .await;
    assert_eq!(
        server.held.lock().await.as_ref().unwrap().kept,
        long_ago,
        "input keeps nothing"
    );
    server
        .settle(&held("b", long_ago), &Ok(json!({})), LeaseRequest::Renew)
        .await;
    assert!(
        server.held.lock().await.as_ref().unwrap().kept > long_ago,
        "a renewal keeps it"
    );

    server
        .settle(&held("b", now), &lost, LeaseRequest::Renew)
        .await;
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

fn cost_fixtures(source: &str) -> Vec<Value> {
    serde_json::from_str(source).unwrap()
}

/// A status whose overview lists `rows` recent actions, as the status
/// reply-cost fixtures read it.
fn status_rows(rows: usize) -> Value {
    let activity: Vec<Value> = (0..rows)
        .map(|index| {
            json!({
                "actor": if index % 3 == 0 { "viewer" } else { "agent" },
                "says": atlas_core::test_support::reply_cost_text(index),
                "ok": index % 2 == 0,
                "ageSeconds": index,
                "nothingSent": index == 1,
                "uncertain": index == 2,
            })
        })
        .collect();
    json!({"overview": {
        "state": "idle",
        "label": "Idle",
        "headline": atlas_core::test_support::reply_cost_text(0),
        "accountName": "Helper 東京",
        "display": {"width": 2560, "height": 1440, "scale": 1},
        "pending": 0,
        "commands": 0,
        "activity": activity,
    }})
}

/// `rows` activity log rows, every odd one refused, as the activity
/// reply-cost fixtures read them.
fn activity_rows(rows: usize) -> Value {
    (0..rows)
        .map(|index| {
            let mut row = json!({
                "at": "999999999999999999999999999999",
                "id": format!("synthetic-{index}"),
                "operation": "input",
                "ok": index % 2 == 0,
                "actor": "agent",
                "verb": "click",
                "app": atlas_core::test_support::reply_cost_text(index),
                "label": "Save \"東京\"",
                "started": false,
                "partial": false,
            });
            if index % 2 == 1 {
                row["code"] = json!("focus-changed");
                row["reason"] = json!("covered");
            }
            row
        })
        .collect()
}

/// `rows` running apps, each with one window, as the app reply-cost
/// fixtures read them.
fn app_rows(rows: usize) -> Value {
    let text = atlas_core::test_support::reply_cost_text;
    let apps: Vec<Value> = (0..rows)
        .map(|index| {
            json!({
                "pid": 812 + index,
                "name": text(index),
                "policy": "regular",
                "active": index == 0,
                "hidden": false,
            })
        })
        .collect();
    let windows: Vec<Value> = (0..rows)
        .map(|index| {
            json!({
                "number": index + 1,
                "pid": 812 + index,
                "owner": text(index),
                "title": text(index),
                "layer": 0,
                "alpha": 1,
                "bounds": {"x": index, "y": index, "width": 640, "height": 480},
            })
        })
        .collect();
    json!({"apps": apps, "windows": windows})
}

fn cost_activity(server: &Server, fixture: &Value) {
    let output = server.client.output_dir().unwrap();
    std::fs::create_dir_all(output).unwrap();
    let rows = fixture["input"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| format!("{row}\n"))
        .collect::<String>();
    std::fs::write(output.join(atlas_client::activity_log::FILE), rows).unwrap();
}

#[test]
fn reply_cost_counts_utf8_bytes_and_escaping() {
    for fixture in atlas_core::test_support::reply_cost_fixtures(
        include_str!("../../../../tests/fixtures/reply-cost/status.json"),
        status_rows,
    ) {
        let status = summary(&fixture["input"], &BTreeMap::new(), None);
        let reply = said(&status);
        let serialized = reply.to_string();
        if fixture["id"] != "status-pending" {
            assert!(serialized.len() > serialized.chars().count());
            assert!(serialized.contains("\\\"draft\\\""));
            assert!(serialized.contains("\\\\"));
        }
        atlas_core::test_support::reply_cost(&reply, &fixture);
        atlas_core::test_support::reply_cost(&status, &fixture["summaryCost"]);
    }
}

#[test]
fn reply_cost_refuses_larger_bytes_rows_and_duplicate_content() {
    let fixture = &atlas_core::test_support::reply_cost_fixtures(
        include_str!("../../../../tests/fixtures/reply-cost/status.json"),
        status_rows,
    )[1];
    let reply = said(&summary(&fixture["input"], &BTreeMap::new(), None));
    for field in ["bytes", "rows"] {
        let mut changed = fixture.clone();
        changed[field] = json!(fixture[field].as_u64().unwrap() + 1);
        assert!(
            std::panic::catch_unwind(|| {
                atlas_core::test_support::reply_cost(&reply, &changed);
            })
            .is_err()
        );
    }
    let mut duplicated = reply.clone();
    duplicated["content"]
        .as_array_mut()
        .unwrap()
        .push(reply["content"][0].clone());
    assert!(
        std::panic::catch_unwind(|| {
            atlas_core::test_support::reply_cost(&duplicated, fixture);
        })
        .is_err()
    );
}

#[tokio::test]
async fn reply_cost_counts_the_complete_serialized_envelope() {
    let activity = atlas_core::test_support::reply_cost_fixtures(
        include_str!("../../../../tests/fixtures/reply-cost/activity.json"),
        activity_rows,
    );
    for fixture in cost_fixtures(include_str!(
        "../../../../tests/fixtures/reply-cost/answer.json"
    )) {
        let dir = tempfile::tempdir().unwrap();
        let server = server(dir.path());
        server.views.store(true, Ordering::Relaxed);
        if let Some(source) = fixture["source"].as_str() {
            cost_activity(
                &server,
                activity.iter().find(|row| row["id"] == source).unwrap(),
            );
        }
        let mut reply = server.answer(&fixture["request"]).await.unwrap();
        atlas_core::test_support::reply_cost(&reply.value, &fixture);
        reply.delivered();
    }
}

#[tokio::test]
async fn activity_reply_cost_keeps_empty_and_populated_rows() {
    for fixture in atlas_core::test_support::reply_cost_fixtures(
        include_str!("../../../../tests/fixtures/reply-cost/activity.json"),
        activity_rows,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let server = server(dir.path());
        cost_activity(&server, &fixture);
        atlas_core::test_support::reply_cost(&server.activity().await.unwrap(), &fixture);
    }
}

#[tokio::test]
async fn reply_cost_preserves_uncertain_input_and_its_retained_control() {
    use axum::{Json, Router, http::StatusCode, routing::post};
    use std::sync::atomic::AtomicUsize;

    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/reply-cost/input.json"
    ))
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path());
    server.views.store(true, Ordering::Relaxed);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/viewer/", listener.local_addr().unwrap());
    let calls = Arc::new(AtomicUsize::new(0));
    let received = calls.clone();
    let refusal = fixture["viewerReply"].clone();
    let body = fixture["viewerRequest"].clone();
    let app = Router::new().route(
        "/viewer/input",
        post(move |Json(request): Json<Value>| {
            received.fetch_add(1, Ordering::Relaxed);
            assert_eq!(request, body);
            let refusal = refusal.clone();
            async move { (StatusCode::CONFLICT, Json(refusal)) }
        }),
    );
    let serving = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    *server.held.lock().await = Some(Held {
        url,
        ..held("synthetic-lease", Instant::now())
    });
    let mut reply = server.answer(&fixture["request"]).await.unwrap();
    serving.abort();
    atlas_core::test_support::reply_cost(&reply.value, &fixture);
    assert_eq!(
        json!(calls.load(Ordering::Relaxed)),
        fixture["viewerRequests"]
    );
    assert!(server.held.lock().await.is_some());
    reply.delivered();
}

#[tokio::test]
async fn representative_reads_keep_their_helper_request_counts() {
    use atlas_client::test_support::Helper;
    use atlas_core::test_support::UID;

    for fixture in atlas_core::test_support::reply_cost_fixtures(
        include_str!("../../../../tests/fixtures/reply-cost/apps.json"),
        app_rows,
    ) {
        for envelope in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let mut server = server(dir.path());
            server.views.store(true, Ordering::Relaxed);
            let (client, helper) = Helper::client(dir.path(), UID);
            server.client = client;
            let mut operations = Vec::new();
            let listed = helper
                .serve(
                    async {
                        if envelope {
                            let mut reply = server.answer(&fixture["request"]).await.unwrap();
                            reply.delivered();
                            reply.value
                        } else {
                            server.helper_apps().await.unwrap()
                        }
                    },
                    |request| {
                        let op = request["op"].as_str().unwrap();
                        operations.push(op.to_owned());
                        Some(json!({"ok":true, op:fixture["input"][op], "starts":{}}))
                    },
                )
                .await;
            atlas_core::test_support::reply_cost(
                &listed,
                if envelope {
                    &fixture["answerCost"]
                } else {
                    &fixture
                },
            );
            assert_eq!(json!(operations), fixture["helperOperations"]);
            assert_eq!(json!(helper.proofs()), fixture["helperProofs"]);
            assert_eq!(json!(helper.requests()), fixture["helperRequests"]);
        }
    }
}

#[tokio::test]
async fn reply_cost_includes_the_private_frame_in_the_outgoing_envelope() {
    use atlas_client::test_support::Helper;
    use atlas_core::test_support::UID;
    use base64::{Engine, engine::general_purpose::STANDARD};

    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/reply-cost/frame.json"
    ))
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut server = server(dir.path());
    server.views.store(true, Ordering::Relaxed);
    let (client, helper) = Helper::client(dir.path(), UID);
    server.client = client;
    let png = atlas_native::native::capture::sample_png(640, 360);
    *server.framed.lock().await = Some((Sha256::digest(&png).into(), fixture["frame"].clone()));
    *server.last.lock().await = Some((
        Instant::now() + Duration::from_secs(60),
        fixture["status"].clone(),
    ));
    let mut operations = Vec::new();
    let mut reply = helper
        .serve(
            server.answer(&json!({
                "jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"screen_frame"}
            })),
            |request| {
                operations.push(request["op"].as_str().unwrap().to_owned());
                Some(json!({"ok":true,"width":640,"height":360,"png":STANDARD.encode(&png)}))
            },
        )
        .await
        .unwrap();
    atlas_core::test_support::reply_cost(&reply.value, &fixture);
    assert_eq!(json!(operations), fixture["helperOperations"]);
    assert_eq!(json!(helper.proofs()), fixture["helperProofs"]);
    assert_eq!(json!(helper.requests()), fixture["helperRequests"]);
    assert_eq!(
        json!(
            STANDARD
                .decode(fixture["frame"]["data"].as_str().unwrap())
                .unwrap()
                .len()
        ),
        fixture["decodedImageBytes"]
    );
    reply.delivered();
}

#[test]
fn app_reply_cost_preserves_the_window_projection() {
    for fixture in atlas_core::test_support::reply_cost_fixtures(
        include_str!("../../../../tests/fixtures/reply-cost/apps.json"),
        app_rows,
    ) {
        let windows: Vec<atlas_core::screen::windows::Window> =
            serde_json::from_value(fixture["input"]["windows"].clone()).unwrap();
        let running =
            AppList::decode(json!({"ok": true, "apps": fixture["input"]["apps"]})).unwrap();
        let mut apps = open_apps(&running, &windows).unwrap();
        for app in &mut apps {
            only_title_and_size(app);
        }
        let reply = json!({
            "content":[{"type":"text","text":format!("{} apps open in the helper.", apps.len())}],
            "structuredContent":{"apps":apps},
        });
        atlas_core::test_support::reply_cost(&reply, &fixture);
    }
}

/// The lists below stay whole, so no tool escapes the checks that loop over
/// them.
#[test]
fn the_tool_lists_name_every_tool_of_their_policy() {
    let named = |chosen: fn(Audience, Access) -> bool| {
        let mut names: Vec<String> = Tool::ALL
            .into_iter()
            .filter(|tool| {
                let (audience, access) = tool.policy();
                chosen(audience, access)
            })
            .map(|tool| tool.catalog()["name"].as_str().unwrap().to_owned())
            .collect();
        names.sort();
        names
    };
    let sorted = |list: &[&str]| {
        let mut names: Vec<String> = list.iter().map(|&name| name.to_owned()).collect();
        names.sort();
        names
    };
    assert_eq!(
        named(|audience, _| matches!(audience, Audience::Views)),
        sorted(&VIEW_ONLY)
    );
    assert_eq!(named(|_, access| access == Access::Read), sorted(&LOOKING));
}

/// The tools only a client that draws views is offered: they are the
/// person's, and only such a client keeps a view's tools from the model.
pub(super) const VIEW_ONLY: [&str; 11] = [
    "resume_agent",
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
];
/// The tools that only look, all a read-only server offers.
pub(super) const LOOKING: [&str; 8] = [
    "atlas_status",
    "show_screen",
    "take_screenshot",
    "screen_frame",
    "activity",
    "helper_apps",
    "screenshots",
    "screenshot",
];

#[tokio::test]
async fn the_views_screen_is_refused_while_the_person_signs_in() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path());
    *server.last.lock().await = Some((Instant::now(), json!({"state": "control"})));
    let mut lease = atlas_control::Lease::default();
    let controller = server.client.controller.clone();
    let token = lease.acquire(&controller).unwrap();
    let hold = atlas_control::PrivateHold {
        site: "github.com".to_owned(),
        kind: atlas_control::HoldKind::SignIn,
    };
    lease.hide_screen(&controller, &token, &hold).unwrap();

    let refused = server.screen(None).await.unwrap_err();

    assert_eq!(refused.code, "busy");
    assert_eq!(refused.holder, Some(atlas_core::error::Holder::Person));
    controller.end_private().unwrap();
    lease.release();
}
