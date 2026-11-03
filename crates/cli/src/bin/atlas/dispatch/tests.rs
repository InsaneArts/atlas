use super::*;
use crate::cli::Cli;
use clap::Parser;

fn parse(args: &[&str]) -> std::result::Result<Cli, clap::Error> {
    Cli::try_parse_from(std::iter::once("atlas").chain(args.iter().copied()))
}

#[tokio::test]
async fn a_sessions_first_read_is_seen_even_when_the_helper_is_unavailable() {
    use atlas_core::host::Host;

    let dir = tempfile::tempdir().unwrap();
    let mut client = Client::new(Config {
        helper_uid: 60001,
        socket_path: dir.path().join("helper.sock"),
        identity_probe: dir.path().join("probe"),
        input_socket: dir.path().join("input.sock"),
        output_dir: dir.path().join("out"),
    })
    .unwrap();
    let host = HostSession::new(Host::Named, "worker").unwrap();
    client.controller =
        atlas_control::Controller::at(60001, &dir.path().join("state")).labelled(Some(&host));
    client.host_session = Some(host.clone());
    let command = parse(&["apps"]).unwrap().command.unwrap();
    let policy = Policy::of(&command);
    assert!(policy.labelled(Actor::Agent));
    let result = tokio::time::timeout(
        Duration::from_secs(1),
        execute_with(
            &dir.path().join("settings.toml"),
            command,
            &policy,
            Some(&host),
            &|| Ok(client.clone()),
            false,
        ),
    )
    .await
    .unwrap();
    assert!(result.is_err());
    let own = HostSession::new(Host::Named, "updater").unwrap();
    client.controller = client.controller.labelled(Some(&own));
    client.host_session = Some(own);
    let error = tokio::time::timeout(Duration::from_secs(1), client.service_change(false))
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, Code::Busy);
    assert_eq!(error.error, "worker is active in the helper.");
}

#[tokio::test]
async fn a_sessions_first_read_defers_another_sessions_service_change() {
    use atlas_core::host::Host;
    use base64::{Engine, engine::general_purpose::STANDARD};
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::UnixListener,
    };

    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let mut client = Client::new(Config {
        helper_uid: 60001,
        socket_path: dir.path().join("helper.sock"),
        identity_probe: dir.path().join("probe"),
        input_socket: dir.path().join("input.sock"),
        output_dir: dir.path().join("out"),
    })
    .unwrap();
    let host = HostSession::new(Host::Named, "worker").unwrap();
    client.controller =
        atlas_control::Controller::at(60001, &dir.path().join("state")).labelled(Some(&host));
    client.host_session = Some(host.clone());
    client.transport.peer_uid = nix::unistd::getuid().as_raw();
    let command = parse(&["apps"]).unwrap().command.unwrap();
    let policy = Policy::of(&command);
    assert!(policy.labelled(Actor::Agent));
    let listener = UnixListener::bind(&client.transport.path).unwrap();
    let helper = async {
        let proof = json!({"ok": true, "uid": 60001, "hasGraphicAccess": true,
            "parentPID": 123, "onConsole": false, "securitySessionID": 42,
            "displayID": 7, "width": 100, "height": 100});
        for (operation, reply) in [
            (
                "hello",
                json!({"ok": true, "user": {"uid": 60001}, "daemon": {"pid": 123},
                    "session": {"managerName": "Aqua", "onConsole": false},
                    "display": {"width": 100, "height": 100, "scale": 1}}),
            ),
            (
                "run",
                json!({"ok": true, "exitCode": 0, "timedOut": false,
                    "events": [{"event": "output", "data": STANDARD.encode(proof.to_string())}]}),
            ),
            ("apps", json!({"ok": true, "apps": []})),
        ] {
            let (peer, _) = listener.accept().await.unwrap();
            let mut peer = BufReader::new(peer);
            let mut line = String::new();
            peer.read_line(&mut line).await.unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(&line).unwrap()["op"],
                operation
            );
            peer.get_mut()
                .write_all(format!("{reply}\n").as_bytes())
                .await
                .unwrap();
        }
    };
    let factory = || Ok(client.clone());
    let config = dir.path().join("settings.toml");
    let (result, ()) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(
            execute_with(&config, command, &policy, Some(&host), &factory, false,),
            helper
        )
    })
    .await
    .unwrap();
    assert_eq!(result.unwrap()["ok"], true);
    assert!(
        !client
            .output_dir()
            .unwrap()
            .join(atlas_client::activity_log::FILE)
            .exists()
    );

    let mut updater = client.clone();
    let own = HostSession::new(Host::Named, "updater").unwrap();
    updater.controller = updater.controller.labelled(Some(&own));
    updater.host_session = Some(own);
    let error = tokio::time::timeout(Duration::from_secs(1), updater.service_change(false))
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, Code::Busy);
    assert_eq!(error.error, "worker is active in the helper.");
    assert!(updater.controller.holder_note().is_none());
}

#[test]
fn a_rollback_a_command_finished_is_settled_in_the_task_that_claimed_the_plan() {
    let dir = tempfile::tempdir().unwrap();
    let client = Client::new(Config {
        helper_uid: 60001,
        socket_path: dir.path().join("helper.sock"),
        identity_probe: dir.path().join("probe"),
        input_socket: dir.path().join("input.sock"),
        output_dir: dir.path().join("out"),
    })
    .unwrap();
    let (task, session, plan) = ("5be0d1a2c3f4", "3f9a1c07b2e4", atlas_core::files::token());
    let tasks = client.output_dir().unwrap().join("tasks");
    std::fs::create_dir(&tasks).unwrap();
    let record = tasks.join(format!("{task}.json"));
    let written = json!({"version": 1, "key": task, "session": session, "name": "probe",
        "started": 1, "updated": 1,
        "owned": [{"kind": "plan", "id": plan, "at": 1, "state": "made"}]});
    std::fs::write(&record, written.to_string()).unwrap();
    let state = || {
        let read: Value = serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
        read["owned"][0]["state"].clone()
    };
    let command = |args: &[&str]| parse(args).unwrap().command.unwrap();
    let rolled_back = Ok(json!({"ok": true, "planId": plan, "phase": "rolledBack",
        "keptTargets": [], "retainedDirectories": []}));

    let preview = command(&["environment", "rollback", &plan]);
    note_outcome(&client, None, Policy::of(&preview).kind, None, &rolled_back);
    assert_eq!(state(), "made", "a preview undoes nothing");

    let host = HostSession::new(atlas_core::host::Host::Named, "unreadable").unwrap();
    let sessions = client.output_dir().unwrap().join("sessions");
    std::fs::create_dir(&sessions).unwrap();
    let session_record = sessions.join(format!("{}.json", host.key()));
    let unreadable = b"{private durable canary";
    std::fs::write(&session_record, unreadable).unwrap();
    let undo = command(&["environment", "rollback", &plan, "--apply"]);
    note_outcome(
        &client,
        Some(&host),
        Policy::of(&undo).kind,
        None,
        &rolled_back,
    );
    assert_eq!(state(), "gone");
    assert_eq!(std::fs::read(session_record).unwrap(), unreadable);
    assert_eq!(rolled_back.as_ref().unwrap()["ok"], true);
}

#[test]
fn a_task_name_that_is_not_plain_refuses_only_commands_that_make_something() {
    use atlas_core::host::Host;

    let host = HostSession::new(Host::Codex, "tasks").unwrap();
    let task = |args: &[&str], name: &str| {
        let command = parse(args).unwrap().command.unwrap();
        task_of(
            Policy::of(&command).makes(),
            Some(&host),
            Some(OsStr::new(name)),
        )
    };

    for making in [
        &["open", "--app", "TextEdit"][..],
        &["run", "--", "ls"],
        &["environment", "manifest.toml"],
        &["environment", "manifest.toml", "--apply"],
        &["environment", "apply", "plan"],
        &["environment", "rollback", "plan", "--apply"],
        &["environment", "profile", "profile.toml"],
        &["environment", "run", "--", "ls"],
        &["cleanup"],
        &["cleanup", "--apply"],
    ] {
        assert_eq!(
            task(making, "not plain").unwrap_err().code,
            "usage",
            "{making:?}"
        );
        let named = task(making, "probe").unwrap().unwrap();
        assert_eq!(named.name().map(|name| name.as_str()), Some("probe"));
    }

    for other in [
        &["status"][..],
        &["activity"],
        &["stop"],
        &["resume"],
        &["restart"],
        &["apps"],
        &["click", "1", "2"],
        &["close", "--app", "TextEdit"],
        &["environment", "list"],
    ] {
        assert_eq!(task(other, "not plain").unwrap(), None, "{other:?}");
        assert!(task(other, "probe").unwrap().is_some(), "{other:?}");
    }

    let open = parse(&["open", "--app", "TextEdit"])
        .unwrap()
        .command
        .unwrap();
    assert_eq!(
        task_of(
            Policy::of(&open).makes(),
            None,
            Some(OsStr::new("not plain"))
        )
        .unwrap(),
        None,
        "without a host session there is no task to name"
    );
}

/// The agents `flags` pick for a connect.
fn picked(flags: &[&str]) -> Vec<atlas_agents::Agent> {
    let args: Vec<&str> = ["agents", "install"].iter().chain(flags).copied().collect();
    match parse(&args).unwrap().command.unwrap() {
        Command::Agents {
            action: Some(AgentsCommand::Install { picks }),
        } => picks.agents(),
        _ => panic!("a connect"),
    }
}

#[test]
fn pi_and_opencode_can_be_selected_independently_or_together() {
    use atlas_agents::Agent;
    assert_eq!(picked(&["--pi"]), [Agent::Pi]);
    assert_eq!(picked(&["--opencode"]), [Agent::Opencode]);
    assert_eq!(
        picked(&["--pi", "--opencode"]),
        [Agent::Opencode, Agent::Pi]
    );
    for verb in ["install", "remove"] {
        assert!(parse(&["agents", verb, "--pi", "--opencode"]).is_ok());
    }
}

#[test]
fn cursor_has_its_own_selector_and_internal_hooks_stay_out_of_help() {
    assert_eq!(picked(&["--cursor"]), [atlas_agents::Agent::Cursor]);
    for verb in ["install", "remove"] {
        assert!(parse(&["agents", verb, "--cursor", "--codex"]).is_ok());
    }
    assert!(parse(&["agents", "cursor-hook", "session-start"]).is_ok());
    let help = super::help::command()
        .find_subcommand_mut("agents")
        .unwrap()
        .render_long_help()
        .to_string();
    assert!(!help.contains("cursor-hook"));
}

#[test]
fn apps_takes_all_to_list_every_process() {
    assert!(matches!(
        parse(&["apps", "--all"]).unwrap().command,
        Some(Command::Apps { all: true })
    ));
    assert!(matches!(
        parse(&["apps"]).unwrap().command,
        Some(Command::Apps { all: false })
    ));
}

#[test]
fn each_command_that_acts_in_the_helper_says_what_it_does() {
    let said = |args: &[&str]| {
        let command = parse(args).unwrap().command.unwrap();
        Policy::of(&command).doing.map(|doing| {
            let mut row = atlas_client::activity_log::Row {
                ok: Some(true),
                action_count: Some(1),
                ..Default::default()
            };
            doing.fill(&mut row, true, None);
            atlas_client::doing::says(&row)
        })
    };
    let says = |args: &[&str], words: &str| assert_eq!(said(args).as_deref(), Some(words));

    says(&["screenshot"], "took a screenshot");
    says(
        &["click", "--app", "TextEdit", "--name", "Save"],
        "clicked Save in TextEdit",
    );
    says(&["click", "10", "20", "--count", "2"], "double-clicked");
    says(&["click", "10", "20", "--button", "right"], "right-clicked");
    says(&["move", "1", "2"], "moved the pointer");
    says(&["drag", "1", "2", "3", "4"], "dragged");
    says(&["scroll", "1", "2", "--dy", "3"], "scrolled");
    says(&["type", "secret"], "typed text");
    says(&["key", "cmd+s"], "pressed ⌘S");
    says(
        &["open", "--app", "TextEdit", "notes.txt"],
        "opened TextEdit",
    );
    says(&["open", "https://example.com"], "opened a link");
    says(&["close", "--app", "TextEdit"], "closed TextEdit");
    says(&["run", "--", "ls", "/private"], "ran a command");
    says(&["environment", "run", "--", "ls"], "ran a command");
    assert_eq!(
        said(&["windows"]),
        None,
        "a read does nothing in the helper"
    );
    assert_eq!(
        said(&["restart"]),
        None,
        "recovery is worded by its operation"
    );
}

#[test]
fn an_actions_app_is_named_in_its_activity() {
    let app = |args: &[&str]| {
        let command = parse(args).unwrap().command.unwrap();
        let mut row = atlas_client::activity_log::Row::default();
        Policy::of(&command)
            .doing
            .unwrap()
            .fill(&mut row, true, None);
        row.app.map(|app| json!(app))
    };

    for args in [
        &["type", "--app", "TextEdit", "hi"][..],
        &["key", "--app", "TextEdit", "cmd+s"],
        &["scroll", "1", "2", "--dy", "3", "--app", "TextEdit"],
        &["drag", "1", "2", "3", "4", "--app", "TextEdit"],
        &["click", "1", "2", "--app", "TextEdit"],
    ] {
        assert_eq!(app(args), Some(json!("TextEdit")), "{args:?}");
    }
    assert_eq!(
        app(&["type", "--pid", "42", "hi"]),
        None,
        "a pid names no app"
    );
}

#[test]
fn typed_text_names_the_field_it_was_sent_to_once_the_helper_checked_it() {
    let said = |args: &[&str], role: Option<&str>| {
        let command = parse(args).unwrap().command.unwrap();
        let mut row = atlas_client::activity_log::Row {
            ok: Some(true),
            ..Default::default()
        };
        Policy::of(&command)
            .doing
            .unwrap()
            .fill(&mut row, true, role);
        atlas_client::doing::says(&row)
    };
    let typed = ["type", "--app", "Notes", "--field", "Email", "secret"];

    assert_eq!(
        said(&typed, Some("AXTextField")),
        "typed into Email in Notes"
    );
    assert_eq!(said(&typed, None), "typed in Notes");
    assert_eq!(
        said(&["type", "--app", "Notes", "secret"], Some("AXTextField")),
        "typed in Notes"
    );
    assert_eq!(
        said(
            &["key", "--app", "Notes", "--field", "Email", "tab"],
            Some("AXTextField")
        ),
        "pressed Tab in Notes"
    );
}

#[tokio::test]
async fn a_stop_cancels_an_environment_run_after_dispatch() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::UnixListener,
    };

    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let mut client = Client::new(Config {
        helper_uid: 60001,
        socket_path: dir.path().join("helper.sock"),
        identity_probe: dir.path().join("probe"),
        input_socket: dir.path().join("input.sock"),
        output_dir: dir.path().join("out"),
    })
    .unwrap();
    client.controller = atlas_control::Controller::at(60001, &dir.path().join("state"));
    client.transport.peer_uid = nix::unistd::getuid().as_raw();
    let command = parse(&["environment", "run", "--", "tool"])
        .unwrap()
        .command
        .unwrap();
    client.doing = Policy::of(&command).doing;
    let Command::Environment(cli::Environment {
        command: Some(EnvironmentCommand::Run(cli::Run { argv, timeout })),
        ..
    }) = command
    else {
        panic!("environment run did not parse");
    };
    let listener = UnixListener::bind(&client.transport.path).unwrap();
    let ownership = client.controller.acquire(false).unwrap();
    let helper = async {
        let proof = json!({"ok": true, "uid": 60001, "hasGraphicAccess": true,
            "parentPID": 123, "onConsole": false, "securitySessionID": 42,
            "displayID": 7, "width": 100, "height": 100});
        for (operation, reply) in [
            (
                "hello",
                json!({"ok": true, "user": {"uid": 60001}, "daemon": {"pid": 123},
                    "session": {"managerName": "Aqua", "onConsole": false},
                    "display": {"width": 100, "height": 100, "scale": 1}}),
            ),
            (
                "run",
                json!({"ok": true, "exitCode": 0, "timedOut": false,
                    "events": [{"event": "output", "data": STANDARD.encode(proof.to_string())}]}),
            ),
        ] {
            let (peer, _) = listener.accept().await.unwrap();
            let mut peer = BufReader::new(peer);
            let mut line = String::new();
            peer.read_line(&mut line).await.unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(&line).unwrap()["op"],
                operation
            );
            peer.get_mut()
                .write_all(format!("{reply}\n").as_bytes())
                .await
                .unwrap();
        }

        let (peer, _) = listener.accept().await.unwrap();
        let mut peer = BufReader::new(peer);
        let mut line = String::new();
        peer.read_line(&mut line).await.unwrap();
        let request: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(request["op"], "run");
        assert_eq!(request["argv"], json!(["tool"]));
        assert_eq!(client.controller.pending().unwrap().len(), 1);
        client.controller.note_stop(true).unwrap();
        line.clear();
        assert_eq!(peer.read_line(&mut line).await.unwrap(), 0);
    };
    let running = client.run_owned_within(&ownership, argv, Duration::from_secs(timeout));
    let (outcome, ()) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(running, helper)
    })
    .await
    .unwrap();
    let error = outcome.unwrap_err();
    assert_eq!(error.code, "stopped");
    assert!(error.uncertain);
    assert_eq!(client.controller.pending().unwrap().len(), 1);
    assert_eq!(client.controller.acquire(true).unwrap_err().code, "busy");
    drop(ownership);
    assert_eq!(
        client.controller.acquire(false).unwrap_err().code,
        "uncertain"
    );
}

#[tokio::test]
async fn atlas_run_lets_others_act_while_it_runs() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::UnixListener,
    };

    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let mut client = Client::new(Config {
        helper_uid: 60001,
        socket_path: dir.path().join("helper.sock"),
        identity_probe: dir.path().join("probe"),
        input_socket: dir.path().join("input.sock"),
        output_dir: dir.path().join("out"),
    })
    .unwrap();
    client.controller = atlas_control::Controller::at(60001, &dir.path().join("state"));
    client.transport.peer_uid = nix::unistd::getuid().as_raw();
    let command = parse(&["run", "--", "tool"]).unwrap().command.unwrap();
    client.doing = Policy::of(&command).doing;
    let other = atlas_control::Controller::at(60001, &dir.path().join("state"));
    let listener = UnixListener::bind(&client.transport.path).unwrap();
    let helper = async {
        let proof = json!({"ok": true, "uid": 60001, "hasGraphicAccess": true,
            "parentPID": 123, "onConsole": false, "securitySessionID": 42,
            "displayID": 7, "width": 100, "height": 100});
        for (operation, reply) in [
            (
                "hello",
                json!({"ok": true, "user": {"uid": 60001}, "daemon": {"pid": 123},
                    "session": {"managerName": "Aqua", "onConsole": false},
                    "display": {"width": 100, "height": 100, "scale": 1}}),
            ),
            (
                "run",
                json!({"ok": true, "exitCode": 0, "timedOut": false,
                    "events": [{"event": "output", "data": STANDARD.encode(proof.to_string())}]}),
            ),
        ] {
            let (peer, _) = listener.accept().await.unwrap();
            let mut peer = BufReader::new(peer);
            let mut line = String::new();
            peer.read_line(&mut line).await.unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(&line).unwrap()["op"],
                operation
            );
            peer.get_mut()
                .write_all(format!("{reply}\n").as_bytes())
                .await
                .unwrap();
        }

        let (peer, _) = listener.accept().await.unwrap();
        let mut peer = BufReader::new(peer);
        let mut line = String::new();
        peer.read_line(&mut line).await.unwrap();
        let request: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(request["op"], "run");
        assert_eq!(request["argv"], json!(["tool"]));

        // The run lets go once it has sent its request; a child another
        // test is starting may hold the lock's file a moment past its
        // release, until it runs its own program.
        let mut beside = None;
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            beside = other.acquire_beside_commands().ok();
            if beside.is_some() {
                break;
            }
        }
        assert!(beside.is_some(), "another agent acts while it runs");
        drop(beside);
        assert_eq!(other.acquire(true).unwrap_err().code, "busy");

        peer.get_mut()
            .write_all(
                format!(
                    "{}\n",
                    json!({"ok": true, "exitCode": 0, "timedOut": false})
                )
                .as_bytes(),
            )
            .await
            .unwrap();
    };
    let client = || Ok(client.clone());
    let running = carry_out(Path::new("unused.toml"), command, &client, None, false);
    let (outcome, ()) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(running, helper)
    })
    .await
    .unwrap();
    assert_eq!(outcome.unwrap()["exitCode"], 0);
    assert!(other.pending().unwrap().is_empty());
}

#[test]
fn a_stop_refuses_what_changes_the_helper_and_lets_looking_and_recovery_through() {
    let changes = |args: &[&str]| {
        let command = parse(args).unwrap().command.unwrap();
        Policy::of(&command).changes_helper()
    };

    for acting in [
        &["click", "1", "2"][..],
        &["type", "x"],
        &["key", "cmd+s"],
        &["open", "--app", "TextEdit"],
        &["close", "--app", "TextEdit"],
        &["run", "--", "ls"],
        &["input", "[]"],
        &["logout"],
        &["environment", "run", "--", "ls"],
        &["environment", "apply", "plan"],
        &["verify-native"],
        &["install"],
        &["update"],
        &["setup", "--apply"],
        &["environment", "rollback", "plan", "--apply"],
        &["environment", "profile", "profile.toml", "--apply"],
        &["job", "start", "--", "/bin/sleep", "9"],
        &["job", "stop", "7c2e91a04b3f"],
    ] {
        assert!(changes(acting), "{acting:?}");
    }

    for looking in [
        &["screenshot"][..],
        &["windows"],
        &["elements", "--app", "TextEdit"],
        &["restart"],
        &["start-input"],
        &["readiness", "--tool", "git"],
        &["stop"],
        &["environment"],
        &["environment", "rollback", "plan"],
        &["environment", "list", "--all"],
        &["environment", "profile", "profile.toml"],
        &["update", "--check"],
        &["setup"],
        &["job", "list", "--all"],
        &["job", "log", "7c2e91a04b3f"],
        &["collect", "atlas-work/report.pdf"],
    ] {
        assert!(!changes(looking), "{looking:?}");
    }
}

#[test]
fn servers_helper_commands_and_setup_previews_carry_no_host_session() {
    let attributed =
        |args: &[&str]| Policy::of(&parse(args).unwrap().command.unwrap()).labelled(Actor::Agent);

    for unattributed in [
        &["mcp"][..],
        &["watch"],
        &["menubar"],
        &["monitor"],
        &["session-info"],
        &["permissions-info"],
        &["elements-info", "1"],
        &["init"],
        &["uninstall"],
        &["update", "--check"],
        &["setup"],
        &["switch"],
    ] {
        assert!(!attributed(unattributed), "{unattributed:?}");
    }

    for agents in [
        &["update"][..],
        &["update-finish"],
        &["setup", "--apply"],
        &["click", "1", "2"][..],
        &["type", "x"],
        &["screenshot"],
        &["open", "--app", "TextEdit"],
        &["close", "--app", "TextEdit"],
        &["run", "--", "ls"],
        &["environment"],
        &["activity"],
        &["agents"],
        &["agents", "status", "--brief"],
    ] {
        assert!(attributed(agents), "{agents:?}");
    }
}

#[test]
fn only_an_environment_request_that_changes_the_helper_can_own_a_plan() {
    let kind = |args: &[&str]| Policy::of(&parse(args).unwrap().command.unwrap()).kind;

    for changes in [
        &["environment", "profile.toml", "--apply"][..],
        &["environment", "apply", "plan"],
        &["environment", "rollback", "plan", "--apply"],
        &["environment", "profile", "profile.toml", "--apply"],
    ] {
        assert_eq!(
            kind(changes),
            Kind::Environment { changes: true },
            "{changes:?}"
        );
    }
    for looks in [
        &["environment", "profile.toml"][..],
        &["environment", "rollback", "plan"],
        &["environment", "profile", "profile.toml"],
    ] {
        assert_eq!(
            kind(looks),
            Kind::Environment { changes: false },
            "{looks:?}"
        );
    }
    for other in [
        &["environment", "discover"][..],
        &["environment", "list"],
        &["environment", "run", "--", "ls"],
    ] {
        assert_eq!(kind(other), Kind::Other, "{other:?}");
    }
    assert!(!Kind::Other.recorded(None), "a read records nothing");
    assert!(Kind::Environment { changes: false }.recorded(None));
}

#[test]
fn the_viewers_commands_never_carry_a_host_session() {
    for args in [
        &["click", "1", "2"][..],
        &["type", "x"],
        &["restart"],
        &["activity"],
    ] {
        let command = parse(args).unwrap().command.unwrap();
        assert!(Policy::of(&command).labelled(Actor::Agent), "{args:?}");
        assert!(!Policy::of(&command).labelled(Actor::Viewer), "{args:?}");
    }
}

#[test]
fn only_commands_that_take_control_wait_and_listen_for_signals() {
    let waits = |args: &[&str]| Policy::of(&parse(args).unwrap().command.unwrap()).controls();

    for controlling in [
        &["input", "[]"][..],
        &["click", "1", "2"],
        &["click", "--app", "Calculator", "--name", "7"],
        &["move", "1", "2"],
        &["drag", "1", "2", "3", "4"],
        &["scroll", "1", "2", "--dy", "3"],
        &["type", "x"],
        &["key", "cmd+s"],
        &["open", "--app", "TextEdit"],
        &["close", "--app", "TextEdit"],
        &["run", "--", "ls"],
        &["restart"],
        &["start-input"],
        &["install"],
        &["readiness", "--tool", "git"],
        &["environment"],
        &["environment", "run", "--", "ls"],
        &["verify-native"],
        &["logout", "--yes"],
        &["job", "start", "--", "/bin/sleep", "9"],
        &["job", "stop", "7c2e91a04b3f"],
    ] {
        assert!(waits(controlling), "{controlling:?}");
    }

    for at_once in [
        &["screenshot"][..],
        &["apps"],
        &["windows"],
        &["elements", "--app", "TextEdit"],
        &["wait", "--app", "TextEdit"],
        &["status"],
        &["doctor"],
        &["mcp"],
        &["watch"],
        &["menubar"],
        &["monitor"],
        &["init"],
        &["uninstall"],
        &["update"],
        &["setup", "--apply"],
        &["switch"],
        &["logout"],
        &["job", "list"],
        &["job", "log", "7c2e91a04b3f"],
        &["collect", "a.bin"],
    ] {
        assert!(!waits(at_once), "{at_once:?}");
    }
}

#[test]
fn job_and_collect_rows_name_no_arguments_or_files() {
    let fields = |args: &[&str]| {
        let command = parse(args).unwrap().command.unwrap();
        let mut row = atlas_client::activity_log::Row::default();
        Policy::of(&command)
            .doing
            .unwrap()
            .fill(&mut row, true, None);
        serde_json::to_value(row).unwrap()
    };
    for (args, verb) in [
        (
            &[
                "job",
                "start",
                "--name",
                "web",
                "--",
                "/usr/bin/nc",
                "-lk",
                "8790",
            ][..],
            "job-start",
        ),
        (&["job", "stop", "7c2e91a04b3f"], "job-stop"),
        (&["collect", "atlas-work/secret-report.pdf"], "collect"),
    ] {
        let fields = fields(args);
        assert_eq!(fields["verb"], verb);
        let text = fields.to_string();
        for word in ["web", "nc", "8790", "secret-report", "7c2e91a04b3f"] {
            assert!(!text.contains(word), "{args:?}: {text}");
        }
    }
    let list = parse(&["job", "list"]).unwrap().command.unwrap();
    assert!(Policy::of(&list).doing.is_none());
}

#[test]
fn starting_jobs_and_collecting_are_refused_for_a_task_name_that_is_not_plain() {
    for args in [
        &["job", "start", "--", "/bin/sleep", "9"][..],
        &["collect", "a.bin"],
    ] {
        let command = parse(args).unwrap().command.unwrap();
        assert!(Policy::of(&command).makes(), "{args:?}");
    }
    let list = parse(&["job", "list"]).unwrap().command.unwrap();
    assert!(!Policy::of(&list).makes());
}

#[test]
fn only_an_agent_waits_and_never_for_what_the_person_does_from_the_app() {
    for args in [
        &["type", "x"][..],
        &["run", "--", "ls"],
        &["restart"],
        &["logout", "--yes"],
    ] {
        let command = parse(args).unwrap().command.unwrap();
        assert!(Policy::of(&command).waits(Actor::Agent), "{args:?}");
        assert!(!Policy::of(&command).waits(Actor::Viewer), "{args:?}");
    }
    let looking = parse(&["screenshot"]).unwrap().command.unwrap();
    assert!(!Policy::of(&looking).waits(Actor::Agent));
}

#[test]
fn setup_commands_refuse_to_run_as_root() {
    let error = refuse_root("update", true).unwrap_err();
    assert_eq!(error.error, "Run atlas update as yourself, not with sudo.");
    assert!(refuse_root("init", false).is_ok());
}
