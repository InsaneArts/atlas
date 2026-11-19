use super::*;
use crate::cli::Cli;
use clap::Parser;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
};

fn parse(args: &[&str]) -> std::result::Result<Cli, clap::Error> {
    Cli::try_parse_from(std::iter::once("atlas").chain(args.iter().copied()))
}

/// Accept one request on `listener`, check its operation and send `reply`.
async fn answer(listener: &UnixListener, operation: &str, reply: Value) {
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

/// Answer the hello and the session proof a client checks before its first
/// request, as a helper running as the configured account does.
async fn handshake(listener: &UnixListener) {
    use base64::{Engine, engine::general_purpose::STANDARD};

    let proof = json!({"ok": true, "uid": 60001, "hasGraphicAccess": true,
        "parentPID": 123, "onConsole": false, "securitySessionID": 42,
        "displayID": 7, "width": 100, "height": 100});
    answer(
        listener,
        "hello",
        json!({"ok": true, "user": {"uid": 60001}, "daemon": {"pid": 123},
            "session": {"managerName": "Aqua", "onConsole": false},
            "display": {"width": 100, "height": 100, "scale": 1}}),
    )
    .await;
    answer(
        listener,
        "run",
        json!({"ok": true, "exitCode": 0, "timedOut": false,
            "events": [{"event": "output", "data": STANDARD.encode(proof.to_string())}]}),
    )
    .await;
}

#[tokio::test]
async fn a_sessions_first_read_is_seen_even_when_the_helper_is_unavailable() {
    use atlas_core::host::Host;

    let dir = tempfile::tempdir().unwrap();
    let mut client = Client::new(Selected::provisional(Config {
        helper_uid: 60001,
        socket_path: dir.path().join("helper.sock"),
        identity_probe: dir.path().join("probe"),
        input_socket: dir.path().join("input.sock"),
        output_dir: dir.path().join("out"),
        activity: Default::default(),
    }))
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

    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let mut client = atlas_client::test_support::client_in(dir.path());
    let host = HostSession::new(Host::Named, "worker").unwrap();
    client.controller = client.controller.labelled(Some(&host));
    client.host_session = Some(host.clone());
    client.link.socket_mut().peer_uid = nix::unistd::getuid().as_raw();
    let command = parse(&["apps"]).unwrap().command.unwrap();
    let policy = Policy::of(&command);
    assert!(policy.labelled(Actor::Agent));
    let listener = UnixListener::bind(&client.link.socket().unwrap().path).unwrap();
    let helper = async {
        handshake(&listener).await;
        answer(&listener, "apps", json!({"ok": true, "apps": []})).await;
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
    let client = Client::new(Selected::provisional(Config {
        helper_uid: 60001,
        socket_path: dir.path().join("helper.sock"),
        identity_probe: dir.path().join("probe"),
        input_socket: dir.path().join("input.sock"),
        output_dir: dir.path().join("out"),
        activity: Default::default(),
    }))
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
    says(
        &[
            "element",
            "press",
            "--ref",
            "8123abc.12",
            "--app",
            "TextEdit",
        ],
        "pressed a control in TextEdit",
    );
    says(
        &[
            "element",
            "set-value",
            "--ref",
            "8123abc.12",
            "--value",
            "secret",
        ],
        "set a value",
    );
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
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let mut client = atlas_client::test_support::client_in(dir.path());
    client.link.socket_mut().peer_uid = nix::unistd::getuid().as_raw();
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
    let listener = UnixListener::bind(&client.link.socket().unwrap().path).unwrap();
    let ownership = client.controller.acquire(false).unwrap();
    let helper = async {
        handshake(&listener).await;

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
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let mut client = atlas_client::test_support::client_in(dir.path());
    client.link.socket_mut().peer_uid = nix::unistd::getuid().as_raw();
    let command = parse(&["run", "--", "tool"]).unwrap().command.unwrap();
    client.doing = Policy::of(&command).doing;
    let other = atlas_control::Controller::at(60001, &dir.path().join("state"));
    let listener = UnixListener::bind(&client.link.socket().unwrap().path).unwrap();
    let helper = async {
        handshake(&listener).await;

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
        &["monitor", "--terminal"],
        &["monitor", "--terminal", "--read-only"],
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
        let mut fields = fields(args);
        assert_eq!(fields["verb"], verb);
        // Its command's random ID may hold any digits; only the rest is words.
        fields["command"].take();
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

#[tokio::test]
async fn every_look_at_the_screen_is_refused_while_the_person_signs_in() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let mut client = atlas_client::test_support::client_in(dir.path());
    client.link.socket_mut().peer_uid = nix::unistd::getuid().as_raw();
    // No helper answers: a look that got as far as asking it would fail
    // another way.
    let mut lease = atlas_control::Lease::default();
    let token = lease.acquire(&client.controller).unwrap();
    let hold = atlas_control::PrivateHold {
        site: "github.com".to_owned(),
        kind: atlas_control::HoldKind::SignIn,
        browser: None,
    };
    lease
        .hide_screen(&client.controller, &token, &hold)
        .unwrap();

    for args in [
        &["screenshot"][..],
        &["elements", "--app", "Safari"],
        &["windows"],
        &["wait", "--app", "Safari", "--timeout", "1"],
    ] {
        let command = parse(args).unwrap().command.unwrap();
        let made = client.clone();
        let refused = carry_out(
            Path::new("unused.toml"),
            command,
            &|| Ok(made.clone()),
            None,
            false,
        )
        .await
        .unwrap_err();
        assert_eq!(refused.code, Code::Busy, "{args:?}: {refused:?}");
        assert_eq!(
            refused.holder,
            Some(atlas_core::error::Holder::Person),
            "{args:?}"
        );
        assert_eq!(
            refused.error,
            "The person is managing an account in the helper; the screen is hidden until they finish."
        );
    }

    client.controller.end_private().unwrap();
    lease.release();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_command_on_a_vm_reaches_its_change_lane_through_dispatch_and_is_held_by_either_stop() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (mut client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
    let config = dir.path().join("settings.toml");
    let run = || {
        parse(&["run", "--", "/bin/echo", "hi"])
            .unwrap()
            .command
            .unwrap()
    };
    let policy = Policy::of(&run());
    assert!(policy.changes_helper());
    client.doing = policy.doing.clone();
    let factory = || Ok(client.clone());

    let ran = execute_with(&config, run(), &policy, None, &factory, false)
        .await
        .unwrap();
    client.controller.note_stop(true).unwrap();
    let station = execute_with(&config, run(), &policy, None, &factory, false).await;
    let revision = client.controller.stop_revision().unwrap();
    client.controller.lift_stop(revision.as_ref()).unwrap();
    let mut guest = Vec::new();
    for stop in ["stopped", "unknown"] {
        *keeper.stop.lock().unwrap() = stop;
        guest.push(execute_with(&config, run(), &policy, None, &factory, false).await);
    }

    assert_eq!(ran["output"], "hi\n", "{ran}");
    let changes = keeper.asked_for("change");
    assert_eq!(changes.len(), 1, "only the command let go: {changes:?}");
    assert_eq!(changes[0]["lane"], "change");
    assert_eq!(
        changes[0]["generation"],
        atlas_client::test_support::GENERATION
    );
    assert_eq!(changes[0]["request"]["op"], "run");
    assert_eq!(changes[0]["request"]["change"], changes[0]["change"]);
    assert_eq!(
        changes[0]["request"]["argv"],
        json!(["/bin/echo", "hi"]),
        "{changes:?}"
    );
    for held in std::iter::once(station).chain(guest) {
        let held = held.unwrap_err();
        assert_eq!(held.code, Code::Stopped);
        assert!(held.error.contains("work"), "{held:?}");
        assert!(!held.error.contains("helper"), "{held:?}");
    }
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_close_on_a_vm_reads_its_guests_apps_and_copies_through_dispatch_and_is_held_by_its_stop()
{
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (mut client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
    let config = dir.path().join("settings.toml");
    let close = || {
        parse(&["close", "--app", "Probe"])
            .unwrap()
            .command
            .unwrap()
    };
    let policy = Policy::of(&close());
    assert!(policy.changes_helper());
    client.doing = policy.doing.clone();
    let factory = || Ok(client.clone());

    let closed = execute_with(&config, close(), &policy, None, &factory, false)
        .await
        .unwrap();
    *keeper.stop.lock().unwrap() = "stopped";
    let held = execute_with(&config, close(), &policy, None, &factory, false).await;

    assert_eq!(closed["ok"], true, "{closed}");
    assert_eq!(closed["reason"], "Probe is not running", "{closed}");
    assert_eq!(closed["removed"], json!([]));
    let reads: Vec<Value> = keeper
        .asked_for("read")
        .into_iter()
        .map(|read| read["request"]["op"].clone())
        .collect();
    assert!(reads.contains(&json!("apps")), "{reads:?}");
    assert!(reads.contains(&json!("copies")), "{reads:?}");
    assert!(keeper.asked_for("change").is_empty(), "nothing to change");
    let held = held.unwrap_err();
    assert_eq!(held.code, Code::Stopped, "{held:?}");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_raw_batch_naming_an_element_on_a_vm_goes_whole_to_its_guests_input_service() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
    let config = dir.path().join("settings.toml");
    let batch = r#"[{"type": "press", "ref": "8123abc.12"}, {"type": "move", "x": 1, "y": 1}]"#;
    let file = dir.path().join("batch.json");
    std::fs::write(&file, batch).unwrap();
    let from_file = format!("@{}", file.display());
    let batches = [
        batch,
        batch,
        r#"[{"type": "reference", "ref": "8123abc.12"}]"#,
        r#"[{"type": "set-value", "ref": "8123abc.12", "value": "x"}]"#,
    ];

    for args in [
        &["input", batch][..],
        &["input", &from_file],
        &["input", batches[2]],
        &["input", batches[3]],
    ] {
        let command = parse(args).unwrap().command.unwrap();
        let done = executed(&config, command, &client).await.unwrap();
        assert_eq!(done["ok"], true, "{args:?}: {done}");
    }

    let sent: Vec<Value> = keeper
        .asked_for("change")
        .into_iter()
        .filter(|change| change["request"]["op"] == "input")
        .collect();
    assert_eq!(sent.len(), batches.len(), "{sent:?}");
    for (sent, batch) in sent.iter().zip(batches) {
        let asked: Value = serde_json::from_str(batch).unwrap();
        for (action, asked) in sent["request"]["actions"]
            .as_array()
            .unwrap()
            .iter()
            .zip(asked.as_array().unwrap())
        {
            assert_eq!(action["type"], asked["type"], "{sent}");
            assert_eq!(action["ref"], asked["ref"], "{sent}");
        }
        assert_eq!(sent["generation"], atlas_client::test_support::GENERATION);
        assert!(sent["request"].get("expect").is_none(), "{sent}");
    }
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agents_input_on_a_vm_goes_on_input_lanes_through_dispatch_and_is_held_by_either_stop() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
    let config = dir.path().join("settings.toml");
    let command = |args: &[&str]| parse(args).unwrap().command.unwrap();
    let inputs: [&[&str]; 7] = [
        &["click", "10", "20"],
        &["move", "1", "2"],
        &["key", "cmd+s"],
        &["type", "hi"],
        &["scroll", "1", "2", "--dy", "3"],
        &["drag", "1", "2", "3", "4"],
        &["input", r#"[{"type": "move", "x": 5, "y": 6}]"#],
    ];

    for args in inputs {
        let done = executed(&config, command(args), &client).await.unwrap();
        assert_eq!(done["ok"], true, "{args:?}: {done}");
    }
    client.controller.note_stop(true).unwrap();
    let station = executed(&config, command(&["click", "10", "20"]), &client).await;
    let revision = client.controller.stop_revision().unwrap();
    client.controller.lift_stop(revision.as_ref()).unwrap();
    let mut guest = Vec::new();
    for stop in ["stopped", "unknown"] {
        *keeper.stop.lock().unwrap() = stop;
        guest.push(executed(&config, command(&["key", "cmd+s"]), &client).await);
    }

    let changes = keeper.asked_for("change");
    assert_eq!(changes.len(), 3 * inputs.len(), "{changes:?}");
    for batch in changes.chunks(3) {
        let ops: Vec<&str> = batch
            .iter()
            .map(|change| change["request"]["op"].as_str().unwrap())
            .collect();
        assert_eq!(ops, ["acquire", "input", "release"], "{batch:?}");
        for change in batch {
            assert_eq!(change["lane"], "input");
            assert_eq!(change["generation"], atlas_client::test_support::GENERATION);
            assert_eq!(change["request"]["change"], change["change"]);
            assert_eq!(change["request"]["owner"], batch[0]["request"]["owner"]);
        }
    }
    for held in std::iter::once(station).chain(guest) {
        let held = held.unwrap_err();
        assert_eq!(held.code, Code::Stopped, "{held:?}");
        assert!(!held.error.contains("helper"), "{held:?}");
    }
    assert_eq!(keeper.asked_for("change").len(), 3 * inputs.len());
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_command_not_built_for_a_vm_is_refused_through_dispatch_before_its_keeper_is_asked_anything()
 {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
    let config = dir.path().join("settings.toml");
    let one_wording = client.unreachable().error;

    for args in [
        &["apps"][..],
        &["start-input"],
        &["environment", "discover", "--tool", "cat"],
    ] {
        let command = parse(args).unwrap().command.unwrap();
        let policy = Policy::of(&command);
        let mut acting = client.clone();
        acting.doing = policy.doing.clone();
        let factory = || Ok(acting.clone());

        let refused = execute_with(&config, command, &policy, None, &factory, false)
            .await
            .unwrap_err();

        assert_eq!(refused.error, one_wording, "{args:?}");
    }
    assert!(
        keeper.asked.lock().unwrap().is_empty(),
        "{:?}",
        keeper.asked.lock().unwrap()
    );
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn targeted_input_on_a_vm_goes_through_dispatch_with_what_its_guest_is_to_check() {
    use atlas_client::test_support::{FINDER, FINDER_WINDOW, GENERATION};
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
    let config = dir.path().join("settings.toml");
    let finder = json!({"pid": FINDER, "start": [123, 456]});
    let in_window = json!({"pid": FINDER, "start": [123, 456], "window": FINDER_WINDOW});
    let in_field = json!({"pid": FINDER, "start": [123, 456], "field": "Search"});
    let window = FINDER_WINDOW.to_string();
    let move_to = r#"[{"type": "move", "x": 30, "y": 40}]"#;
    let targeted: [(&[&str], &Value); 9] = [
        (&["click", "30", "40", "--app", "Finder"], &finder),
        (&["key", "cmd+s", "--app", "Finder"], &finder),
        (
            &["type", "hi", "--app", "Finder", "--field", "Search"],
            &in_field,
        ),
        (
            &["scroll", "30", "40", "--dy", "3", "--window", &window],
            &in_window,
        ),
        (
            &["drag", "30", "40", "50", "60", "--window", &window],
            &in_window,
        ),
        (&["input", move_to, "--app", "Finder"], &finder),
        (&["click", "--app", "Finder", "--name", "OK"], &in_window),
        (&["click", "--ref", "8123abc.0", "--app", "Finder"], &finder),
        (
            &["element", "press", "--ref", "8123abc.0", "--app", "Finder"],
            &finder,
        ),
    ];

    for (args, expect) in targeted {
        let before = keeper.asked_for("change").len();
        let command = parse(args).unwrap().command.unwrap();
        let done = executed(&config, command, &client).await.unwrap();

        assert_eq!(done["ok"], true, "{args:?}: {done}");
        let changes = keeper.asked_for("change").split_off(before);
        let ops: Vec<&str> = changes
            .iter()
            .map(|change| change["request"]["op"].as_str().unwrap())
            .collect();
        assert_eq!(ops, ["acquire", "input", "release"], "{args:?}");
        for change in &changes {
            assert_eq!(change["lane"], "input", "{args:?}");
            assert_eq!(change["generation"], GENERATION, "{args:?}");
        }
        assert_eq!(&changes[1]["request"]["expect"], expect, "{args:?}");
    }
    let looked = executed(
        &config,
        parse(&["elements", "--app", "Finder"])
            .unwrap()
            .command
            .unwrap(),
        &client,
    )
    .await
    .unwrap();
    let windows = executed(
        &config,
        parse(&["windows"]).unwrap().command.unwrap(),
        &client,
    )
    .await
    .unwrap();

    assert_eq!(looked["look"], "8123abc", "{looked}");
    let look = keeper
        .asked_for("read")
        .into_iter()
        .rfind(|read| read["request"]["op"] == "elements")
        .unwrap();
    assert_eq!(look["lane"], "input", "{look}");
    assert_eq!(look["generation"], GENERATION, "{look}");
    assert_eq!(look["request"]["expect"], finder, "{look}");
    assert_eq!(windows["windows"][0]["number"], FINDER_WINDOW, "{windows}");
    assert_eq!(windows["windows"][0]["owner"], "Finder", "{windows}");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn targeted_input_and_looks_on_a_vm_wait_for_accessibility_in_its_guests_words_through_dispatch()
 {
    const ACCESSIBILITY: &str =
        "Allow Atlas in Privacy & Security, Accessibility, in the VM's window: agents wait for it.";
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
    let config = dir.path().join("settings.toml");
    *keeper.held.lock().unwrap() = Some(ACCESSIBILITY);
    let allow = "Ask the person to open work's window with atlas desktop maintain work and allow \
                 Atlas in Privacy & Security, Accessibility, there.";

    for args in [
        &["click", "10", "10", "--app", "Finder"][..],
        &["elements", "--app", "Finder"],
        &["click", "--app", "Finder", "--name", "OK"],
        &["element", "press", "--ref", "8123abc.0"],
        &["wait", "--app", "Finder", "--name", "OK", "--timeout", "0"],
        &["type", "hi", "--app", "Finder"],
    ] {
        let command = parse(args).unwrap().command.unwrap();
        let refused = match executed(&config, command, &client).await {
            Err(refused) => json!(refused),
            Ok(reply) => reply,
        };

        assert_eq!(refused["ok"], false, "{args:?}: {refused}");
        assert_eq!(refused["code"], "permission", "{args:?}: {refused}");
        assert_eq!(refused["error"], ACCESSIBILITY, "{args:?}: {refused}");
        assert_eq!(refused["next"], allow, "{args:?}: {refused}");
        assert!(!refused.to_string().contains("helper"), "{refused}");
    }
    assert!(keeper.asked_for("change").is_empty(), "nothing sent");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_elements_windows_and_waits_are_served_under_either_stop_while_a_click_is_held() {
    for by_station in [true, false] {
        let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
        let config = dir.path().join("settings.toml");
        let command = |args: &[&str]| parse(args).unwrap().command.unwrap();
        if by_station {
            client.controller.note_stop(true).unwrap();
        } else {
            *keeper.stop.lock().unwrap() = "stopped";
        }

        let looked = executed(&config, command(&["elements", "--app", "Finder"]), &client)
            .await
            .unwrap();
        let windows = executed(&config, command(&["windows"]), &client)
            .await
            .unwrap();
        let waited = executed(
            &config,
            command(&["wait", "--app", "Finder", "--name", "OK", "--timeout", "0"]),
            &client,
        )
        .await
        .unwrap();
        let clicked = executed(
            &config,
            command(&["click", "30", "40", "--app", "Finder"]),
            &client,
        )
        .await;

        assert_eq!(looked["look"], "8123abc", "{by_station}: {looked}");
        assert_eq!(
            windows["windows"][0]["owner"], "Finder",
            "{by_station}: {windows}"
        );
        assert_eq!(waited["matched"], true, "{by_station}: {waited}");
        let held = clicked.unwrap_err();
        assert_eq!(held.code, Code::Stopped, "{by_station}: {held:?}");
        assert!(keeper.asked_for("change").is_empty(), "{by_station}");
        assert!(client.controller.pending().unwrap().is_empty());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_refusal_a_vm_gives_through_dispatch_says_helper() {
    let refusals = [
        json!({"ok": false, "code": "daemon", "error": "The helper daemon stopped unexpectedly"}),
        json!({"ok": false, "code": "permission",
            "error": "Screen Recording is not allowed for Atlas in the helper account"}),
        json!({"ok": false, "code": "busy", "holder": "person",
            "error": "The person is using the helper"}),
        json!({"ok": false, "code": "job",
            "error": "No background job with that ID is known in the helper"}),
    ];
    // Replies the station cannot read, which it refuses, or holds as of
    // unknown outcome, in its own words.
    let unreadable = json!({"ok": true, "apps": 5, "windows": 5});
    let changes: [&[&str]; 6] = [
        &["run", "--", "/bin/echo", "hi"],
        &["job", "start", "--", "/bin/sleep", "30"],
        &["click", "10", "20"],
        &["type", "hi"],
        &["key", "cmd+s"],
        &["element", "press", "--ref", "8123abc.0"],
    ];
    let reads: [&[&str]; 8] = [
        &["open", "--app", "Probe"],
        &["close", "--app", "Probe"],
        &["click", "10", "20", "--app", "Finder"],
        &["elements", "--app", "Finder"],
        &["windows"],
        &["wait", "--app", "Finder", "--timeout", "0"],
        &["wait", "--app", "Finder", "--name", "OK", "--timeout", "0"],
        &["job", "list"],
    ];
    let cases = refusals
        .iter()
        .flat_map(|said| changes.iter().chain(&reads).map(move |args| (said, *args)))
        .chain(
            changes
                .iter()
                .chain(&reads)
                .map(|args| (&unreadable, *args)),
        );

    for (said, args) in cases {
        let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
        let config = dir.path().join("settings.toml");
        *keeper.answering.lock().unwrap() = Some(said.clone());

        let outcome = executed(&config, parse(args).unwrap().command.unwrap(), &client).await;

        let shown = match outcome {
            Err(error) => crate::output::failure(&error, true),
            Ok(reply) => reply,
        };
        assert!(
            !shown.to_string().to_lowercase().contains("helper"),
            "{args:?} answered {said}: {shown}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_app_or_path_with_helper_in_its_name_is_named_as_it_is_on_a_vm() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
    let config = dir.path().join("settings.toml");
    let command = |args: &[&str]| parse(args).unwrap().command.unwrap();

    let absent = executed(
        &config,
        command(&["click", "10", "20", "--app", "BuildHelper"]),
        &client,
    )
    .await
    .unwrap_err();
    *keeper.answering.lock().unwrap() = Some(json!({"ok": false, "code": "spawn-failed",
        "error": "The file /tmp/helper/BuildHelper.app does not exist; the helper looked"}));
    let unopened = executed(&config, command(&["run", "--", "/usr/bin/true"]), &client)
        .await
        .unwrap();

    assert_eq!(
        absent.error, "BuildHelper is not running: run atlas open --app BuildHelper",
        "{absent:?}"
    );
    assert_eq!(
        unopened["error"], "The file /tmp/helper/BuildHelper.app does not exist; work looked",
        "{unopened}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_app_named_the_helper_is_named_as_it_is_while_the_guests_own_words_name_the_vm() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
    let config = dir.path().join("settings.toml");
    let command = |args: &[&str]| parse(args).unwrap().command.unwrap();

    let absent = executed(
        &config,
        command(&["click", "--app", "the helper", "10", "10"]),
        &client,
    )
    .await
    .unwrap_err();
    *keeper.answering.lock().unwrap() = Some(json!({"ok": false, "code": "daemon",
        "error": "the helper daemon stopped while the helper ran it"}));
    let stopped = executed(&config, command(&["run", "--", "/usr/bin/true"]), &client)
        .await
        .unwrap();

    assert_eq!(
        absent.error, "the helper is not running: run atlas open --app the helper",
        "{absent:?}"
    );
    let shown = crate::output::failure(&absent, true);
    assert_eq!(shown["error"], absent.error, "{shown}");
    assert_eq!(
        stopped["error"], "work's daemon stopped while work ran it",
        "{stopped}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_wait_on_a_vm_whose_session_moved_on_is_proved_again_and_goes_on() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
    let config = dir.path().join("settings.toml");
    // Its keeper reconnected after the wait proved it: the first look is
    // refused unread, as one of another generation is.
    *keeper.refusing_once.lock().unwrap() = Some(json!({"ok": false,
        "code": "session-changed", "sent": false,
        "error": "work's session changed since this command proved it."}));

    let waited = executed(
        &config,
        parse(&["wait", "--app", "Finder", "--timeout", "5"])
            .unwrap()
            .command
            .unwrap(),
        &client,
    )
    .await
    .unwrap();

    assert_eq!(waited["ok"], true, "{waited}");
    assert_eq!(waited["windows"][0]["owner"], "Finder", "{waited}");
    assert_eq!(keeper.asked_for("observe").len(), 2, "proved again once");
    let looks = keeper
        .asked_for("read")
        .into_iter()
        .filter(|read| read["request"]["op"] == "windows")
        .count();
    assert_eq!(looks, 2, "looked again in the new proof");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_on_a_vm_starts_and_stops_on_change_lanes_through_dispatch_its_list_read_through_the_keeper()
 {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
    let config = dir.path().join("settings.toml");
    let command = |args: &[&str]| parse(args).unwrap().command.unwrap();
    let start = || command(&["job", "start", "--", "/bin/sleep", "30"]);
    let stop = || {
        command(&[
            "job",
            "stop",
            atlas_client::test_support::JOB,
            "--grace",
            "1",
        ])
    };
    let started = executed(&config, start(), &client).await.unwrap();
    let listed = executed(&config, command(&["job", "list", "--all"]), &client)
        .await
        .unwrap();
    let stopped = executed(&config, stop(), &client).await.unwrap();
    client.controller.note_stop(true).unwrap();
    let station = executed(&config, start(), &client).await;
    let revision = client.controller.stop_revision().unwrap();
    client.controller.lift_stop(revision.as_ref()).unwrap();
    *keeper.stop.lock().unwrap() = "stopped";
    let guest = executed(&config, start(), &client).await;

    assert_eq!(started["state"], "running", "{started}");
    assert_eq!(
        listed["jobs"][0]["id"],
        atlas_client::test_support::JOB,
        "{listed}"
    );
    assert_eq!(stopped["how"], "stopped", "{stopped}");
    let changes = keeper.asked_for("change");
    assert_eq!(changes.len(), 2, "only a start and a stop: {changes:?}");
    for (change, action) in changes.iter().zip(["start", "stop"]) {
        assert_eq!(change["lane"], "change");
        assert_eq!(change["generation"], atlas_client::test_support::GENERATION);
        assert_eq!(change["request"]["op"], "job");
        assert_eq!(change["request"]["action"], action, "{change}");
        assert_eq!(change["request"]["change"], change["change"]);
    }
    assert_eq!(changes[0]["request"]["uid"], 502);
    assert!(
        keeper
            .asked_for("read")
            .iter()
            .any(|read| read["request"]["op"] == "job" && read["request"]["action"] == "show")
    );
    for held in [station, guest] {
        let held = held.unwrap_err();
        assert_eq!(held.code, Code::Stopped, "{held:?}");
        assert!(held.error.contains("work"), "{held:?}");
    }
    assert!(client.controller.pending().unwrap().is_empty());
}

/// `command` carried out through dispatch by an agent's `client`, its
/// settings at `config`.
async fn executed(config: &Path, command: Command, client: &Client) -> Result<Value> {
    let policy = Policy::of(&command);
    let mut acting = client.clone();
    acting.doing = policy.doing.clone();
    let factory = || Ok(acting.clone());
    execute_with(config, command, &policy, None, &factory, false).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stopped_vm_is_started_by_run_and_never_by_status_doctor_close_or_a_jobs_stop() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
    keeper.runner.set_power("stopped");
    let config = dir.path().join("settings.toml");
    let command = |args: &[&str]| parse(args).unwrap().command.unwrap();

    for args in [
        &["status"][..],
        &["doctor"],
        &["close", "--app", "Probe"],
        &["job", "stop", atlas_client::test_support::JOB],
    ] {
        let _ = executed(&config, command(args), &client).await;
        assert!(keeper.runner.asked_for("start").is_empty(), "{args:?}");
        assert_eq!(keeper.runner.power(), "stopped", "{args:?}");
    }
    let ran = executed(&config, command(&["run", "--", "/bin/echo", "hi"]), &client)
        .await
        .unwrap();

    assert_eq!(ran["output"], "hi\n", "{ran}");
    assert_eq!(ran["started"]["desktop"], "work", "{ran}");
    let starts = keeper.runner.asked_for("start");
    assert_eq!(starts.len(), 1, "{starts:?}");
    assert_eq!(ran["started"]["change"], starts[0]["change"]);
    assert_eq!(keeper.runner.power(), "running");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vm_started_for_a_command_holds_it_by_the_guests_saved_stop_in_the_vms_words() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
    keeper.runner.set_power("stopped");
    *keeper.stop.lock().unwrap() = "stopped";
    let config = dir.path().join("settings.toml");
    let run = || {
        parse(&["run", "--", "/bin/echo", "hi"])
            .unwrap()
            .command
            .unwrap()
    };

    let held = executed(&config, run(), &client).await.unwrap_err();
    let again = executed(&config, run(), &client).await.unwrap_err();

    assert_eq!(held.code, Code::Stopped, "{held:?}");
    assert_eq!(
        held.error,
        "work holds agents until the person resumes them from the Atlas menu bar."
    );
    assert_eq!(held.details["started"]["desktop"], "work", "{held:?}");
    assert_eq!(
        again.error, held.error,
        "held the same until the person's Resume"
    );
    assert!(
        again.details.get("started").is_none(),
        "started once: {again:?}"
    );
    assert_eq!(keeper.runner.asked_for("start").len(), 1);
    assert!(keeper.asked_for("change").is_empty(), "nothing ran");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stopped_vm_is_started_by_a_jobs_start_by_open_and_by_an_agents_input() {
    for args in [
        &["job", "start", "--", "/bin/sh", "-c", "sleep 1"][..],
        &["open", "--app", "TextEdit"],
        &["click", "10", "10"],
        &["key", "cmd+a"],
    ] {
        let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
        keeper.runner.set_power("stopped");
        let config = dir.path().join("settings.toml");
        let command = parse(args).unwrap().command.unwrap();

        let reply = executed(&config, command, &client).await;

        assert_eq!(keeper.runner.asked_for("start").len(), 1, "{args:?}");
        assert_eq!(keeper.runner.power(), "running", "{args:?}");
        let started = match &reply {
            Ok(reply) => reply["started"]["desktop"].clone(),
            Err(error) => error.details["started"]["desktop"].clone(),
        };
        assert_eq!(started, "work", "{args:?}: {reply:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn verify_native_on_a_vm_runs_in_one_proof_and_starts_it_when_needed() {
    for power in ["running", "stopped"] {
        let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
        keeper.runner.set_power(power);
        // A guest that cannot check, so the check ends at its proof.
        *keeper.observing.lock().unwrap() = Some(|answer| {
            let older: Vec<Value> = answer["declared"]["capabilities"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|name| **name != "native-check-v1")
                .cloned()
                .collect();
            answer["declared"]["capabilities"] = json!(older);
            answer["status"]["hello"]["daemon"]["capabilities"] = json!(older);
        });
        let fixture = dir.path().join("DesktopFixture");
        std::fs::write(&fixture, b"fixture").unwrap();
        let config = dir.path().join("settings.toml");
        let command = parse(&["verify-native", "--fixture", fixture.to_str().unwrap()])
            .unwrap()
            .command
            .unwrap();
        let before = keeper.asked_for("observe").len();

        let refused = executed(&config, command, &client).await.unwrap_err();

        assert_eq!(refused.code, Code::Update, "{power}: {refused:?}");
        assert_eq!(
            refused.error,
            "work's Atlas is from an older build and cannot check native Save and Open."
        );
        let starts = keeper.runner.asked_for("start").len();
        assert_eq!(starts, usize::from(power == "stopped"), "{power}");
        assert_eq!(
            refused.details.get("started").is_some(),
            power == "stopped",
            "{power}: {refused:?}"
        );
        if power == "running" {
            assert_eq!(
                keeper.asked_for("observe").len() - before,
                1,
                "one proof for the whole check"
            );
        }
        assert!(
            keeper.asked_for("stage").is_empty() && keeper.asked_for("change").is_empty(),
            "{power}: nothing copied or run"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stopped_vms_status_and_doctor_say_it_is_stopped_and_what_starts_it() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
    keeper.runner.set_power("stopped");
    *keeper.away.lock().unwrap() = true;
    let config = dir.path().join("settings.toml");
    let command = |args: &[&str]| parse(args).unwrap().command.unwrap();
    let words = "work is stopped. A command that needs it, such as run, starts it.";

    let status = executed(&config, command(&["status"]), &client)
        .await
        .unwrap();
    let doctor = executed(&config, command(&["doctor"]), &client)
        .await
        .unwrap();

    assert_eq!(status["proven"], false, "{status}");
    assert_eq!(status["ready"], false, "{status}");
    assert_eq!(status["notReady"], words, "{status}");
    assert_eq!(status["proof"]["code"], "not-ready", "{status}");
    assert_eq!(status["guest"]["error"], words, "{status}");
    assert_eq!(status["runner"]["power"], "stopped", "{status}");
    assert_eq!(doctor["ready"], false, "{doctor}");
    assert_eq!(doctor["notReady"], words, "{doctor}");
    assert_eq!(doctor["nextSteps"][0], words, "{doctor}");
    assert!(doctor.get("guest").is_none(), "{doctor}");
    assert!(keeper.runner.asked_for("start").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_ready_vms_status_and_doctor_say_it_is_ready_and_doctor_checks_nothing_it_cannot() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, _keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
    let config = dir.path().join("settings.toml");
    let command = |args: &[&str]| parse(args).unwrap().command.unwrap();

    let status = executed(&config, command(&["status"]), &client)
        .await
        .unwrap();
    let doctor = executed(&config, command(&["doctor"]), &client)
        .await
        .unwrap();
    let verbose = executed(&config, command(&["doctor", "--verbose"]), &client)
        .await
        .unwrap();

    assert_eq!(status["ready"], true, "{status}");
    assert!(status.get("notReady").is_none(), "{status}");
    assert_eq!(doctor["ready"], true, "{doctor}");
    assert!(
        doctor["warnings"][0]
            .as_str()
            .unwrap()
            .starts_with("Native Save/Open is not checked on work"),
        "{doctor}"
    );
    assert_eq!(
        verbose["capabilities"]["maintenance"],
        json!({"state": "none"})
    );
    assert_eq!(
        verbose["capabilities"]["nativeDialogs"]["state"], "notChecked",
        "{verbose}"
    );
    assert_eq!(
        verbose["capabilities"]["installedApps"]["state"], "checked",
        "{verbose}"
    );
    assert!(!verbose["capabilities"].to_string().contains("passed"));
    let shown = atlas_cli::ui::render::render(
        &atlas_cli::ui::render::View::Doctor { verbose: true },
        &verbose,
    )
    .out;
    assert!(shown.contains("VM work"), "{shown}");
    assert!(shown.contains("notChecked"), "{shown}");
    assert!(!shown.to_lowercase().contains("helper"), "{shown}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_doctor_of_a_vm_says_which_browsers_it_has_and_not_checked_before_it_can_ask() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
    let config = dir.path().join("settings.toml");
    let command = |args: &[&str]| parse(args).unwrap().command.unwrap();
    let doctor = async || {
        executed(&config, command(&["doctor", "--verbose"]), &client)
            .await
            .unwrap()
    };
    let installed_reads = || {
        keeper
            .asked_for("read")
            .into_iter()
            .filter(|read| read["request"]["op"] == "installed")
            .collect::<Vec<_>>()
    };

    let checked = doctor().await;
    let asked = installed_reads();
    // Its window is open, so its proof is refused and it cannot be asked.
    *keeper.observing.lock().unwrap() = Some(|answer| answer["runner"]["window"] = json!("open"));
    let unproven = doctor().await;
    // An older guest's proof says it cannot tell.
    *keeper.observing.lock().unwrap() = Some(|answer| {
        let older: Vec<Value> = answer["declared"]["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|name| *name != "installed-v1")
            .cloned()
            .collect();
        answer["declared"]["capabilities"] = json!(older);
        answer["status"]["hello"]["daemon"]["capabilities"] = json!(older);
    });
    let older = doctor().await;

    assert_eq!(
        checked["capabilities"]["installedApps"],
        json!({"state": "checked", "apps": {"Google Chrome": false, "Firefox": false},
            "reason": "Neither Google Chrome nor Firefox is installed in work; sign-ins there \
                need one of them."}),
        "{checked}"
    );
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert_eq!(
        asked[0]["request"]["names"],
        json!(["Google Chrome", "Firefox"])
    );
    assert!(asked[0]["request"]["session"].is_object(), "{asked:?}");
    assert_eq!(
        asked[0]["generation"],
        atlas_client::test_support::GENERATION
    );
    assert_eq!(
        unproven["capabilities"]["installedApps"],
        json!({"state": "notChecked",
            "reason": "Which apps work has is asked once it is proven again."}),
        "{unproven}"
    );
    assert_eq!(
        older["capabilities"]["installedApps"],
        json!({"state": "notChecked",
            "reason": "work's Atlas is from an older build and cannot say which apps it has."}),
        "{older}"
    );
    assert_eq!(installed_reads().len(), 1, "asked nothing more");
    for report in [&checked, &unproven, &older] {
        let said = report["capabilities"]["installedApps"].to_string();
        assert!(
            !said.contains("passed") && !said.contains("helper"),
            "{said}"
        );
    }
}

/// A manifest of one file, and a profile of it, in `folder`.
fn environment_files(folder: &Path) -> (PathBuf, PathBuf) {
    std::fs::write(folder.join("notes.txt"), b"notes\n").unwrap();
    let manifest = folder.join("manifest.toml");
    std::fs::write(
        &manifest,
        "[[files]]\nsource = \"notes.txt\"\ntarget = \".config/probe/notes.txt\"\n",
    )
    .unwrap();
    let profile = folder.join("profile.toml");
    std::fs::write(
        &profile,
        "version = 2\n\n[[files]]\nsource = \"notes.txt\"\ntarget = \".config/probe/notes.txt\"\n",
    )
    .unwrap();
    (manifest, profile)
}

/// A plan's ID, as a preview prints it.
const PLAN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_environment_preview_starts_a_stopped_vm_and_its_list_and_undo_preview_never_do() {
    let files = tempfile::tempdir().unwrap();
    let (manifest, profile) = environment_files(files.path());
    let (manifest, profile) = (manifest.to_str().unwrap(), profile.to_str().unwrap());
    for args in [
        &["environment", "list"][..],
        &["environment", "rollback", PLAN],
    ] {
        let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
        keeper.runner.set_power("stopped");
        let config = dir.path().join("settings.toml");

        let _ = executed(&config, parse(args).unwrap().command.unwrap(), &client).await;

        assert!(keeper.runner.asked_for("start").is_empty(), "{args:?}");
        assert_eq!(keeper.runner.power(), "stopped", "{args:?}");
    }
    for args in [
        &["environment", manifest][..],
        &["environment", "profile", profile],
    ] {
        let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
        keeper.runner.set_power("stopped");
        let config = dir.path().join("settings.toml");

        let reply = executed(&config, parse(args).unwrap().command.unwrap(), &client).await;

        assert_eq!(keeper.runner.asked_for("start").len(), 1, "{args:?}");
        assert_eq!(keeper.runner.power(), "running", "{args:?}");
        let started = match &reply {
            Ok(reply) => reply["started"]["desktop"].clone(),
            Err(error) => error.details["started"]["desktop"].clone(),
        };
        assert_eq!(started, "work", "{args:?}: {reply:?}");
        assert!(client.controller.pending().unwrap().is_empty(), "{args:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_environment_change_on_a_vm_goes_through_dispatch_and_is_held_by_either_stop() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
    let config = dir.path().join("settings.toml");
    let (manifest, profile) = environment_files(dir.path());
    let (manifest, profile) = (manifest.to_str().unwrap(), profile.to_str().unwrap());
    let changes: [&[&str]; 5] = [
        &["environment", "run", "--", "/bin/echo", "hi"],
        &["environment", "apply", PLAN],
        &["environment", "rollback", PLAN, "--apply"],
        &["environment", manifest, "--apply"],
        &["environment", "profile", profile, "--apply"],
    ];
    let command = |args: &[&str]| parse(args).unwrap().command.unwrap();

    let ran = executed(&config, command(changes[0]), &client)
        .await
        .unwrap();
    let asked = keeper.asked_for("change").len();
    client.controller.note_stop(true).unwrap();
    let mut held = Vec::new();
    for args in changes {
        held.push(executed(&config, command(args), &client).await);
    }
    let revision = client.controller.stop_revision().unwrap();
    client.controller.lift_stop(revision.as_ref()).unwrap();
    for stop in ["stopped", "unknown"] {
        *keeper.stop.lock().unwrap() = stop;
        for args in changes {
            held.push(executed(&config, command(args), &client).await);
        }
    }

    assert_eq!(ran["output"], "hi\n", "{ran}");
    let change = &keeper.asked_for("change")[0];
    assert_eq!(change["lane"], "change");
    assert_eq!(change["request"]["op"], "environment");
    assert_eq!(change["request"]["apply"], true);
    assert_eq!(
        change["request"]["packet"]["request"],
        json!({"operation": "run", "argv": ["/bin/echo", "hi"]})
    );
    assert_eq!(change["request"]["packet"]["expected"]["uid"], 502);
    assert_eq!(asked, 1, "only the command");
    for held in held {
        let held = held.unwrap_err();
        assert_eq!(held.code, Code::Stopped, "{held:?}");
        assert!(held.error.contains("work"), "{held:?}");
    }
    assert_eq!(keeper.asked_for("change").len(), asked, "nothing more went");
    assert!(keeper.asked_for("stage").is_empty(), "nothing staged");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_install_on_a_vm_is_admitted_through_dispatch_starts_a_stopped_vm_and_is_held_by_either_stop()
 {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (mut client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
    keeper.runner.set_power("stopped");
    let host = HostSession::new(atlas_core::host::Host::Codex, "installing").unwrap();
    client.task = Some(atlas_client::task::Task::of(host.key(), Some("probe")).unwrap());
    client.host_session = Some(host);
    let config = dir.path().join("settings.toml");
    let app = dir.path().join("Probe.app");
    std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
    std::fs::write(app.join("Contents/Info.plist"), "<plist/>").unwrap();
    let install = || {
        parse(&["install-app", app.to_str().unwrap()])
            .unwrap()
            .command
            .unwrap()
    };

    let reply = executed(&config, install(), &client).await;
    client.controller.note_stop(true).unwrap();
    let station = executed(&config, install(), &client).await;
    let revision = client.controller.stop_revision().unwrap();
    client.controller.lift_stop(revision.as_ref()).unwrap();
    *keeper.stop.lock().unwrap() = "stopped";
    let guest = executed(&config, install(), &client).await;

    assert_eq!(keeper.runner.asked_for("start").len(), 1);
    assert_eq!(keeper.runner.power(), "running");
    // Its guest keeps no copy here, so the stage is refused for certain:
    // the install went as far as staging its bundle.
    let refused = reply.unwrap_err();
    assert_ne!(refused.error, client.unreachable().error, "{refused:?}");
    assert!(!refused.uncertain, "{refused:?}");
    assert_eq!(refused.details["started"]["desktop"], "work", "{refused:?}");
    let stages = keeper.asked_for("stage");
    assert_eq!(stages.len(), 1, "{stages:?}");
    assert_eq!(
        stages[0]["generation"],
        atlas_client::test_support::GENERATION
    );
    for held in [station, guest] {
        let held = held.unwrap_err();
        assert_eq!(held.code, Code::Stopped, "{held:?}");
        assert!(held.error.contains("work"), "{held:?}");
    }
    assert_eq!(keeper.asked_for("stage").len(), 1, "nothing more staged");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[test]
fn an_environment_plans_steps_record_no_activity_and_still_heed_the_stop_as_an_agents() {
    // The client heeds the person's Stop for these by its actor alone: they
    // name no activity of their own.
    for args in [
        &["environment", "manifest.toml"][..],
        &["environment", "manifest.toml", "--apply"],
        &["environment", "profile", "profile.toml", "--apply"],
        &["environment", "apply", PLAN],
        &["environment", "rollback", PLAN],
        &["environment", "rollback", PLAN, "--apply"],
    ] {
        let policy = Policy::of(&parse(args).unwrap().command.unwrap());
        assert!(policy.doing.is_none(), "{args:?}");
        assert!(policy.labelled(Actor::Agent), "{args:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn atlas_apps_live_window_of_a_vm_is_refused_through_dispatch_in_its_one_wording() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
    let config = dir.path().join("settings.toml");
    let command = parse(&["monitor"]).unwrap().command.unwrap();
    let policy = Policy::of(&command);
    let factory = || Ok(client.clone());

    let refused = execute_with(&config, command, &policy, None, &factory, false)
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::Platform, "{refused:?}");
    assert_eq!(
        refused.error,
        "work's live picture shows only in the terminal, read-only: atlas monitor --terminal \
         --desktop work."
    );
    assert_eq!(
        refused.details["next"],
        "Run atlas monitor --terminal --desktop work."
    );
    let said = serde_json::to_string(&refused).unwrap();
    assert!(
        !said.contains("helper") && !said.contains("macOS only"),
        "{said}"
    );
    assert!(keeper.asked.lock().unwrap().is_empty());
}

/// The person's maintenance of the VM `client` acts on, kept at `phase` in
/// its lifecycle record under `dir`.
fn under_maintenance(
    dir: &Path,
    client: &Client,
    phase: atlas_client::vm::lifecycle::record::Phase,
) {
    atlas_client::test_support::under_maintenance(dir, &client.desktop().id(), phase);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_agent_command_on_a_vm_under_maintenance_is_refused_in_its_words_before_any_proof() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
    let config = dir.path().join("settings.toml");
    under_maintenance(
        dir.path(),
        &client,
        atlas_client::vm::lifecycle::record::Phase::Open,
    );
    let job = atlas_client::test_support::JOB;

    for args in [
        &["run", "--", "/bin/echo", "hi"][..],
        &["screenshot"],
        &["job", "log", job],
        &["job", "list"],
        &["job", "list", "--all"],
        &["collect", "notes.txt"],
        &["input", r#"[{"type": "move", "x": 1, "y": 1}]"#],
        &["open", "--app", "TextEdit"],
        &["windows"],
    ] {
        let command = parse(args).unwrap().command.unwrap();
        let refused = executed(&config, command, &client).await.unwrap_err();
        assert_eq!(refused.code, Code::Busy, "{args:?}: {refused:?}");
        assert_eq!(
            refused.error,
            "work is under maintenance: its window takes the person's input. Agents wait \
             until the person ends it.",
            "{args:?}"
        );
        assert_eq!(
            refused.holder,
            Some(atlas_core::error::Holder::Person),
            "{args:?}"
        );
    }

    for op in ["observe", "change", "stage", "picture"] {
        assert!(keeper.asked_for(op).is_empty(), "{op} asked");
    }
    let reads: Vec<Value> = keeper
        .asked_for("read")
        .into_iter()
        .map(|read| read["request"]["op"].clone())
        .filter(|op| !["hello", "outcome", "leftovers"].contains(&op.as_str().unwrap_or("")))
        .collect();
    assert!(reads.is_empty(), "{reads:?}");
    assert!(keeper.runner.asked_for("start").is_empty());
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_status_doctor_and_overview_of_a_vm_under_maintenance_say_so_and_what_ends_it() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
    let config = dir.path().join("settings.toml");
    let command = |args: &[&str]| parse(args).unwrap().command.unwrap();
    under_maintenance(
        dir.path(),
        &client,
        atlas_client::vm::lifecycle::record::Phase::Open,
    );
    let reason = "work is under maintenance: its window takes your input and agents wait. End \
                  it with atlas desktop maintain work --end.";

    let status = executed(&config, command(&["status"]), &client)
        .await
        .unwrap();
    let doctor = executed(&config, command(&["doctor", "--verbose"]), &client)
        .await
        .unwrap();
    let overview = atlas_client::overview::read(&client).await;

    assert_eq!(status["station"]["maintenance"]["state"], "on", "{status}");
    assert_eq!(status["station"]["maintenance"]["reason"], reason);
    assert_eq!(status["proven"], false, "{status}");
    assert_eq!(status["ready"], false, "{status}");
    assert_eq!(status["proof"]["error"], reason, "{status}");
    assert_eq!(doctor["nextSteps"], json!([reason]), "said once: {doctor}");
    assert_eq!(doctor["ready"], false, "{doctor}");
    assert_eq!(
        doctor["capabilities"]["maintenance"],
        json!({"state": "on", "reason": reason})
    );
    let check = doctor["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["name"] == "Not under maintenance")
        .cloned()
        .unwrap();
    assert_eq!(check["ok"], false, "{doctor}");
    assert_eq!(check["nextStep"], reason);
    assert!(!overview.ready, "{}", overview.value);
    assert_eq!(
        overview.value["overview"]["reason"], reason,
        "{}",
        overview.value
    );
    assert!(keeper.asked_for("observe").is_empty(), "no proof asked");
    for said in [&status, &doctor] {
        assert!(!said.to_string().contains("helper"), "{said}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn readiness_is_false_while_a_vm_is_under_maintenance_naming_it() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, _keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
    let config = dir.path().join("settings.toml");
    let command = |args: &[&str]| parse(args).unwrap().command.unwrap();
    under_maintenance(
        dir.path(),
        &client,
        atlas_client::vm::lifecycle::record::Phase::Closing,
    );

    let status = executed(&config, command(&["status"]), &client)
        .await
        .unwrap();

    assert_eq!(status["ready"], false, "{status}");
    assert_eq!(
        status["notReady"],
        "work's maintenance is ending, but its window's input off is not confirmed yet. Run \
         atlas desktop maintain work --end again."
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_maintenance_is_said_from_its_record_while_its_keeper_is_not_there() {
    use atlas_client::vm::lifecycle::record::Phase;
    let on = "work is under maintenance: its window takes your input and agents wait. End it \
              with atlas desktop maintain work --end.";
    let ending = "work's maintenance is ending, but its window's input off is not confirmed \
                  yet. Run atlas desktop maintain work --end again.";
    let unknown = "Whether work is under maintenance cannot be read, so agents wait.";
    for (kept, state, words) in [
        (Some(Phase::Open), "on", on),
        (Some(Phase::Closing), "ending", ending),
        (None, "unknown", unknown),
    ] {
        let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
        let config = dir.path().join("settings.toml");
        let command = |args: &[&str]| parse(args).unwrap().command.unwrap();
        let record =
            atlas_client::test_support::lifecycle_record(dir.path(), &client.desktop().id());
        match kept {
            Some(phase) => under_maintenance(dir.path(), &client, phase),
            None => std::fs::write(&record, b"{not a record").unwrap(),
        }
        // Its keeper is gone: nothing answers on its socket.
        drop(keeper);

        let status = client.status().await;
        let doctor = executed(&config, command(&["doctor"]), &client)
            .await
            .unwrap_err();
        let overview = atlas_client::overview::read(&client).await;

        assert_eq!(status["ok"], false, "{state}: {status}");
        assert_eq!(status["station"]["maintenance"]["state"], state, "{status}");
        assert_eq!(status["station"]["maintenance"]["reason"], words);
        assert_eq!(doctor.details["next"], words, "{state}: {doctor:?}");
        assert_eq!(
            doctor.details["station"]["maintenance"]["state"], state,
            "{doctor:?}"
        );
        assert!(!overview.ready, "{state}");
        assert_eq!(
            overview.value["overview"]["reason"], words,
            "{}",
            overview.value
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restart_on_a_vm_recovers_it_and_never_restarts_its_guest() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
    let config = dir.path().join("settings.toml");
    let command = parse(&["restart"]).unwrap().command.unwrap();

    let recovered = executed(&config, command, &client).await.unwrap();

    assert_eq!(recovered["ok"], true, "{recovered}");
    assert_eq!(recovered["op"], "recover");
    assert_eq!(recovered["desktop"], json!({"kind": "vm", "name": "work"}));
    assert_eq!(recovered["cleared"], json!([]));
    assert_eq!(recovered["kept"], json!([]));
    assert_eq!(recovered["guest"], json!({"recovered": true}));
    assert!(recovered.get("restarted").is_none(), "{recovered}");
    assert_eq!(keeper.asked_for("recover").len(), 1);
    assert!(
        keeper.asked_for("change").is_empty(),
        "nothing asked of its guest"
    );
    for op in ["start", "stop", "window"] {
        assert!(keeper.runner.asked_for(op).is_empty(), "{op} asked");
    }
    assert_eq!(keeper.runner.power(), "running");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn logout_on_a_vm_is_refused_in_its_own_words_and_claims_nothing() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = atlas_client::test_support::vm_through_keeper(dir.path());
    let config = dir.path().join("settings.toml");

    // Without --yes too: refused before the person is asked.
    for args in [
        &["logout"][..],
        &["logout", "--yes"],
        &["logout", "--force", "--yes"],
    ] {
        let command = parse(args).unwrap().command.unwrap();
        let refused = executed(&config, command, &client).await.unwrap_err();

        assert_eq!(refused.code, Code::Platform, "{args:?}: {refused:?}");
        assert_eq!(
            refused.error, "work is a VM desktop, so Atlas does not log it out.",
            "{args:?}"
        );
        assert_eq!(
            refused.details["next"], "To log out by hand, run atlas desktop maintain work.",
            "{args:?}"
        );
        assert!(!refused.uncertain, "{args:?}");
    }
    let refused = client.logout(false).await.unwrap_err();
    assert_eq!(refused.code, Code::Platform);

    assert!(
        keeper.asked.lock().unwrap().is_empty(),
        "nothing asked of its keeper"
    );
    assert!(keeper.runner.ops().is_empty(), "nothing asked of Atlas VM");
    assert!(client.controller.pending().unwrap().is_empty());
    let other = client.controller.clone().waiting(std::time::Duration::ZERO);
    assert!(other.acquire_for_executor().is_ok(), "no exclusion kept");
}

#[test]
fn a_reply_names_the_desktop_it_ran_on_by_the_name_its_selection_had() {
    use atlas_core::config::desktops::{Selection, Settings};

    let vm = |name: &str, id: &str| {
        format!("\n[[desktops]]\nname = \"{name}\"\nkind = \"vm\"\nid = \"{id}\"\n")
    };
    let (first, second) = ("0123456789abcdef01234567", "89abcdef0123456701234567");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("atlas.toml");
    let chosen_from = format!(
        "default = \"work\"\noutput_dir = \"out\"\n{}{}",
        vm("work", first),
        vm("archive", second)
    );
    // The names swapped once the command chose `work` by its record key.
    let swapped = format!(
        "default = \"work\"\noutput_dir = \"out\"\n{}{}",
        vm("archive", first),
        vm("work", second)
    );
    let settings = Settings::parse(chosen_from.as_bytes(), &path).unwrap();
    let key = settings.default_desktop().record_key();

    let settings = Settings::parse(swapped.as_bytes(), &path).unwrap();
    let selection = Selection {
        flag: None,
        parent: Some(key.as_str().to_owned()),
        named: None,
        bound: None,
    };
    let selected = settings.selected(settings.resolve(&selection).unwrap());
    let client = Client::new(selected).unwrap();

    assert_eq!(
        client.desktop().record_key(),
        key,
        "it ran on the chosen VM"
    );
    assert_eq!(
        reply_desktop(&client),
        json!({"name": "archive", "kind": "vm"})
    );
}
