//! The overview's checks: each mode, state and headline, the sessions a
//! status lists, and how each logged row is shown.
use super::{activity::*, sessions::*, *};
use std::{collections::BTreeMap, fs};

/// The facts of a status with `pending` whether an outcome is unknown,
/// at `now`.
fn facts(pending: bool, now: u128) -> SessionFacts {
    SessionFacts {
        pending,
        agent_holds: false,
        now,
    }
}

const NOW: u128 = 100_000_000_000;

/// `value` as a logged row.
fn logged(value: Value) -> Row {
    Row::parse(&value).unwrap()
}

/// The logged row `row` as every app shows it, at `now`.
fn shown(row: &Value, now: u128) -> Value {
    json!(super::shown(&logged(row.clone()), now))
}

#[test]
fn every_screenshot_refusal_has_a_plain_reason_in_the_activity_list() {
    for (code, words) in [
        ("capture", "The screen couldn't be read"),
        ("identity", "Atlas couldn't reach the helper"),
        ("on-console", "Atlas couldn't reach the helper"),
        ("permission", "The helper can't record the screen"),
        ("session-changed", "The helper's screen changed meanwhile"),
        ("no-window", "The app had no window on screen"),
    ] {
        let row = json!({"operation": "screenshot", "ok": false, "code": code});
        assert_eq!(shown(&row, NOW)["reason"], words, "{code}");
    }
    for (operation, words) in [
        ("input", "The helper can't use the mouse and keyboard"),
        ("run", "The helper doesn't have permission"),
    ] {
        let row = json!({"operation": operation, "ok": false, "code": "permission"});
        assert_eq!(shown(&row, NOW)["reason"], words, "{operation}");
    }
}

#[test]
fn activity_rows_carry_their_session_and_capture() {
    let row = json!({"at": "99000000000", "operation": "screenshot", "ok": true,
        "actor": "agent", "verb": "screenshot", "command": "0123456789ab",
        "session": "3f9a1c07b2e4", "capture": "1761660429000123456-capture.png"});
    let seen = shown(&row, NOW);
    assert_eq!(seen["session"], "3f9a1c07b2e4");
    assert_eq!(seen["capture"], "1761660429000123456-capture.png");

    let plain = shown(
        &json!({"at": "99000000000", "operation": "input", "ok": true}),
        NOW,
    );
    assert_eq!(plain["session"], Value::Null);
    assert_eq!(plain["capture"], Value::Null);
}

#[test]
fn shown_rows_never_carry_a_task() {
    let row = json!({"at": "99000000000", "operation": "input", "ok": true,
        "actor": "agent", "verb": "click", "session": "3f9a1c07b2e4",
        "task": "5be0d1a2c3f4"});
    let shown = shown(&row, NOW);

    assert!(shown.get("task").is_none());
    assert!(!shown.to_string().contains("5be0d1a2c3f4"));
}

#[test]
fn the_status_lists_recent_host_sessions_and_who_holds_the_helper() {
    use crate::host_session::{
        Host, HostSession,
        store::{Kind, note},
    };

    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let session = |id: String| HostSession::new(Host::Codex, &id).unwrap();
    let worked = |session: &HostSession| {
        let doing = crate::doing::Doing::new("click");
        note(
            &client,
            session,
            Kind::Other,
            Some(&doing),
            &Ok(json!({"ok": true})),
        )
        .unwrap();
    };
    let sessions: Vec<HostSession> = (0..10).map(|n| session(format!("worker-{n}"))).collect();
    for each in &sessions {
        worked(each);
        std::thread::sleep(Duration::from_millis(2));
    }

    // One long idle, with an unknown outcome of its own.
    let idle = session("idle".into());
    worked(&idle);
    let path = client
        .output_dir()
        .unwrap()
        .join(format!("sessions/{}.json", idle.key()));
    let mut record: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    record["lastSeen"] = json!(1_000);
    fs::write(&path, record.to_string()).unwrap();
    drop(
        client
            .controller
            .clone()
            .labelled(Some(&idle))
            .quarantine()
            .unwrap(),
    );
    let holding = client
        .controller
        .clone()
        .labelled(Some(&sessions[9]))
        .acquire(true)
        .unwrap();

    let listed = host_sessions(&client, facts(true, crate::client::timestamp()));
    assert_eq!(listed.sessions.len(), 8);
    assert_eq!(listed.more, 3);
    assert!(listed.shown, "ten live sessions");
    assert_eq!(listed.holder.as_deref(), Some(sessions[9].key().as_str()));
    let sessions_listed = &listed.sessions;
    assert_eq!(
        sessions_listed[0]["key"],
        idle.key().as_str(),
        "the one needing the person first, however long idle"
    );
    assert_eq!(sessions_listed[0]["state"], "needsYou");
    assert_eq!(sessions_listed[0]["live"], true);
    let first = &sessions_listed[1];
    assert_eq!(
        first["key"],
        sessions[9].key().as_str(),
        "then the one acting"
    );
    assert_eq!(first["state"], "acting");
    assert_eq!(first["holdsHelper"], true);
    assert_eq!(sessions_listed[2]["holdsHelper"], false);
    assert_eq!(
        sessions_listed[2]["key"],
        sessions[8].key().as_str(),
        "then newest first"
    );
    assert_eq!(first["host"], "codex");
    assert_eq!(first["hostLabel"], "Codex");
    assert_eq!(first["hostSession"], "worker-9");
    assert_eq!(first["active"], true);
    assert_eq!(first["uncertain"], 0);
    assert_eq!(first["leftovers"], json!({"copies": 0, "plans": 0}));
    assert_eq!(first["waitingFor"], Value::Null);
    assert!(first["name"].as_str().unwrap().starts_with("Codex"));
    drop(holding);

    let listed = host_sessions(
        &client,
        facts(true, crate::client::timestamp() + 2 * 3_600_000_000_000),
    );
    assert_eq!(
        listed.sessions.len(),
        1,
        "only the one with an unknown outcome, hours later"
    );
    assert!(!listed.shown, "one live session speaks of none");
    assert_eq!(listed.holder, None);
    let only = &listed.sessions[0];
    assert_eq!(only["key"], idle.key().as_str());
    assert_eq!(only["uncertain"], 1);
    assert_eq!(only["active"], false);
}

#[test]
fn the_agents_commands_no_host_named_are_listed_as_terminal_beside_the_hosts() {
    use crate::host_session::{Host, HostSession, store::note};

    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let output = client.output_dir().unwrap().to_path_buf();
    let session = HostSession::new(Host::Claude, "left").unwrap();
    note(
        &client,
        &session,
        crate::host_session::store::Kind::Other,
        Some(&crate::doing::Doing::new("click")),
        &Ok(json!({"ok": true})),
    )
    .unwrap();
    let now = crate::client::timestamp();
    let rows = [
        json!({"at": (now - 1_000_000_000).to_string(), "operation": "input", "ok": true,
            "actor": "agent", "verb": "click", "app": "Notes", "command": "000000000001"}),
        json!({"at": (now - 2_000_000_000).to_string(), "operation": "input", "ok": true,
            "actor": "agent", "verb": "type", "app": "Opie", "command": "000000000002",
            "session": session.key()}),
    ];
    let log: String = rows.iter().rev().map(|row| format!("{row}\n")).collect();
    fs::write(output.join(activity_log::FILE), log).unwrap();

    let listed = host_sessions(&client, facts(false, now));
    assert!(listed.shown, "a host's session and Terminal make two");
    let names: Vec<&str> = listed
        .sessions
        .iter()
        .map(|entry| entry["name"].as_str().unwrap())
        .collect();
    assert!(
        names[0].starts_with("Claude Code"),
        "its record was seen last"
    );
    assert_eq!(names[1], "Terminal");
    let terminal = &listed.sessions[1];
    assert_eq!(terminal["key"], "");
    assert_eq!(terminal["host"], "none");
    assert_eq!(terminal["state"], "idle");
    assert_eq!(terminal["latest"]["says"], "clicked in Notes");
    assert!(terminal.get("hostSession").is_none());
    assert_eq!(listed.sessions[0]["latest"]["says"], "typed in Opie");
}

#[test]
fn a_session_on_its_first_command_is_listed_from_its_note() {
    use crate::host_session::{Host, HostSession};

    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let session = HostSession::new(Host::Codex, "first").unwrap();
    let holding = client
        .controller
        .clone()
        .labelled(Some(&session))
        .acquire(true)
        .unwrap();

    let listed = host_sessions(&client, facts(false, crate::client::timestamp()));
    assert_eq!(listed.sessions.len(), 1);
    let entry = &listed.sessions[0];
    assert_eq!(entry["key"], session.key().as_str());
    assert_eq!(entry["name"], "Codex");
    assert_eq!(entry["state"], "acting");
    assert!(entry.get("hostSession").is_none());
    assert_eq!(listed.holder.as_deref(), Some(session.key().as_str()));
    drop(holding);

    let listed = host_sessions(&client, facts(false, crate::client::timestamp()));
    assert!(listed.sessions.is_empty(), "gone with its note");
}

#[test]
fn a_session_on_its_first_command_is_named_by_its_host_and_folder_as_its_record_will_be() {
    use crate::host_session::{Host, HostSession, store::note};

    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let first = HostSession::new(Host::Codex, "first")
        .unwrap()
        .in_folder(Some("left"));
    let holding = client
        .controller
        .clone()
        .labelled(Some(&first))
        .acquire(true)
        .unwrap();

    let listed = host_sessions(&client, facts(false, crate::client::timestamp()));
    let entry = &listed.sessions[0];
    assert_eq!(entry["name"], "Codex · left");
    assert_eq!(entry["folder"], "left");

    // Another session of that host in that folder: both gain their
    // start times, as two records would.
    let other = HostSession::new(Host::Codex, "other")
        .unwrap()
        .in_folder(Some("left"));
    note(
        &client,
        &other,
        crate::host_session::store::Kind::Other,
        Some(&crate::doing::Doing::new("click")),
        &Ok(json!({"ok": true})),
    )
    .unwrap();
    let listed = host_sessions(&client, facts(false, crate::client::timestamp()));
    let names: BTreeMap<&str, &str> = listed
        .sessions
        .iter()
        .map(|entry| {
            (
                entry["key"].as_str().unwrap(),
                entry["name"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(names.len(), 2);
    for key in [first.key(), other.key()] {
        assert!(
            names[key.as_str()].starts_with("Codex · left · "),
            "{names:?}"
        );
    }
    assert_ne!(names[first.key().as_str()], names[other.key().as_str()]);
    drop(holding);

    let named = HostSession::new(Host::Named, "release")
        .unwrap()
        .in_folder(Some("left"));
    let holding = client
        .controller
        .clone()
        .labelled(Some(&named))
        .acquire(true)
        .unwrap();
    let listed = host_sessions(&client, facts(false, crate::client::timestamp()));
    let entry = listed
        .sessions
        .iter()
        .find(|entry| entry["key"] == named.key().as_str())
        .unwrap();
    assert_eq!(
        entry["name"], "release · left",
        "a named session by its label"
    );
    drop(holding);
}

#[test]
fn a_first_commands_name_stays_once_its_record_is_written() {
    use crate::host_session::{Host, HostSession, Instance, store::note};

    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let worked = |session: &HostSession| {
        note(
            &client,
            session,
            crate::host_session::store::Kind::Other,
            Some(&crate::doing::Doing::new("click")),
            &Ok(json!({"ok": true})),
        )
        .unwrap();
    };
    let other = HostSession::new(Host::Codex, "other")
        .unwrap()
        .in_folder(Some("left"));
    worked(&other);
    // This test process stands for the first command: its notes and
    // its record both start when it did.
    let first = HostSession::new(Host::Codex, "first")
        .unwrap()
        .in_folder(Some("left"))
        .begun(Instance::current().map(|process| process.started_ms()));
    let holding = client
        .controller
        .clone()
        .labelled(Some(&first))
        .acquire(true)
        .unwrap();
    let name = |listed: &HostSessions| {
        listed
            .sessions
            .iter()
            .find(|entry| entry["key"] == first.key().as_str())
            .map(|entry| (entry["name"].clone(), entry["started"].clone()))
            .unwrap()
    };

    let before = name(&host_sessions(
        &client,
        facts(false, crate::client::timestamp()),
    ));
    assert!(before.0.as_str().unwrap().starts_with("Codex · left · "));
    worked(&first);
    let after = name(&host_sessions(
        &client,
        facts(false, crate::client::timestamp()),
    ));
    assert_eq!(after, before);
    drop(holding);
}

#[test]
fn a_session_running_its_first_command_is_listed_acting_by_its_name() {
    use crate::host_session::{Host, HostSession};

    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let agent = HostSession::new(Host::Named, "agent-a")
        .unwrap()
        .in_folder(Some("left"));
    // A command started with `run` lets go of control once sent: no
    // holder note names it, and its session has no record yet.
    let command = client
        .controller
        .clone()
        .labelled(Some(&agent))
        .quarantine_command()
        .unwrap();
    assert!(client.controller.holder_note().is_none());

    let listed = host_sessions(&client, facts(false, crate::client::timestamp()));
    let entry = listed
        .sessions
        .iter()
        .find(|entry| entry["key"] == agent.key().as_str())
        .expect("listed from its running command's note");
    assert_eq!(entry["name"], "agent-a · left");
    assert_eq!(entry["state"], "acting");
    assert_eq!(entry["ended"], Value::Null);
    assert_eq!(pending(&client.controller).0.commands, 1);
    assert!(first_command_running(&client, &[]));

    client.controller.clear_if(&command).unwrap();
    drop(command);
    let listed = host_sessions(&client, facts(false, crate::client::timestamp()));
    assert!(
        listed
            .sessions
            .iter()
            .all(|entry| entry["key"] != agent.key().as_str()),
        "nothing lists it once its command ends with no record"
    );
}

#[test]
fn a_session_at_work_has_never_ended_whatever_its_record_says() {
    use crate::host_session::{
        Host, HostSession, Instance,
        store::{Kind, note, touch},
    };

    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let first = HostSession::new(Host::Claude, "resumed")
        .unwrap()
        .run_by(Some(Instance::new(i32::MAX, [1, 0])));
    note(
        &client,
        &first,
        Kind::Other,
        Some(&crate::doing::Doing::new("click")),
        &Ok(json!({"ok": true})),
    )
    .unwrap();
    let listed = host_sessions(&client, facts(false, crate::client::timestamp()));
    assert!(listed.sessions.is_empty(), "the process it names is gone");

    // Resumed in a new process, the one this command runs in: its
    // command holds the helper before anything is recorded.
    let parent = nix::unistd::getppid().as_raw();
    let host = crate::host_session::ancestry::process(parent).unwrap();
    let resumed = HostSession::new(Host::Claude, "resumed")
        .unwrap()
        .run_by(Some(Instance::new(parent, host.start)));
    let holding = client
        .controller
        .clone()
        .labelled(Some(&resumed))
        .acquire(true)
        .unwrap();
    let listed = host_sessions(&client, facts(false, crate::client::timestamp()));
    let entry = &listed.sessions[0];
    assert_eq!(entry["state"], "acting");
    assert_eq!(
        (&entry["live"], &entry["ended"], &entry["active"]),
        (&json!(true), &Value::Null, &json!(true))
    );
    drop(holding);

    // Any command of it refreshes its record, a read too.
    touch(&client, &resumed).unwrap();
    let listed = host_sessions(&client, facts(false, crate::client::timestamp()));
    let entry = &listed.sessions[0];
    assert_eq!(entry["state"], "idle");
    assert_eq!(
        (&entry["live"], &entry["ended"], &entry["active"]),
        (&json!(true), &Value::Null, &json!(true))
    );
}

#[test]
fn an_ended_session_waits_for_no_recovery() {
    use crate::host_session::{Host, HostSession, store::note};

    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let session = HostSession::new(Host::Codex, "done").unwrap();
    note(
        &client,
        &session,
        crate::host_session::store::Kind::Other,
        Some(&crate::doing::Doing::new("click")),
        &Ok(json!({"ok": true})),
    )
    .unwrap();
    let output = client.output_dir().unwrap().to_path_buf();
    let now = crate::client::timestamp();
    let row = json!({"at": (now - 1_000_000_000).to_string(), "operation": "input",
        "ok": true, "actor": "agent", "verb": "click", "session": session.key()});
    fs::write(output.join(activity_log::FILE), format!("{row}\n")).unwrap();

    let listed = host_sessions(&client, facts(true, now));
    assert_eq!(listed.sessions[0]["waitingFor"], "recovery");

    let path = output.join(format!("sessions/{}.json", session.key()));
    let mut record: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    record["ended"] = record["lastSeen"].clone();
    // Renamed into place, as every record is written, so status reads
    // see the folder changed.
    let partial = path.with_extension("json.partial");
    fs::write(&partial, record.to_string()).unwrap();
    fs::rename(&partial, &path).unwrap();
    let listed = host_sessions(&client, facts(true, now));
    assert!(listed.sessions.is_empty(), "{:?}", listed.sessions);
}

#[test]
fn ended_sessions_are_neither_listed_nor_counted_as_more() {
    use crate::host_session::{Host, HostSession, store::note};

    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let sessions: Vec<HostSession> = (0..12)
        .map(|n| HostSession::new(Host::Codex, &format!("worker-{n}")).unwrap())
        .collect();
    for session in &sessions {
        note(
            &client,
            session,
            crate::host_session::store::Kind::Other,
            Some(&crate::doing::Doing::new("click")),
            &Ok(json!({"ok": true})),
        )
        .unwrap();
    }
    let folder = client.output_dir().unwrap().join("sessions");
    for session in &sessions[2..] {
        let path = folder.join(format!("{}.json", session.key()));
        let mut record: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        record["ended"] = record["lastSeen"].clone();
        fs::write(&path, record.to_string()).unwrap();
    }

    let listed = host_sessions(&client, facts(false, crate::client::timestamp()));
    assert_eq!(listed.sessions.len(), 2, "ten ended");
    assert_eq!(listed.more, 0);
    assert!(listed.sessions.iter().all(|entry| entry["live"] == true));
}

#[test]
fn an_overview_with_no_records_has_no_sessions() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let listed = host_sessions(&client, facts(false, crate::client::timestamp()));
    assert!(listed.sessions.is_empty() && !listed.shown);
    assert_eq!((listed.more, listed.holder), (0, None));
}

#[test]
fn a_focus_refusal_has_a_plain_reason() {
    let row = json!({"operation": "input", "ok": false, "code": "focus-changed",
        "verb": "type", "app": "TextEdit"});
    let shown = shown(&row, NOW);
    assert_eq!(shown["reason"], "Another app was in front");
    assert_eq!(shown["says"], "didn't type in TextEdit");
}

#[test]
fn a_focus_refusal_atlas_could_not_read_says_so_rather_than_blaming_another_app() {
    let reason = |why: &str| {
        let row = json!({"operation": "input", "ok": false, "code": "focus-changed",
            "verb": "click", "reason": why});
        shown(&row, NOW)["reason"].clone()
    };
    assert_eq!(
        reason("unreadable"),
        "Atlas couldn't tell which app was in front"
    );
    for other in ["app", "point"] {
        assert_eq!(reason(other), "Another app was in front", "{other}");
    }
    let unreadable = json!({"operation": "input", "ok": false, "code": "focus-changed",
        "reason": "unreadable"});
    assert_eq!(shown(&unreadable, NOW)["nothingSent"], true);
}

#[test]
fn a_focus_refusal_sent_nothing_and_is_not_uncertain() {
    let refused = shown(
        &json!({"operation": "input", "ok": false, "code": "focus-changed",
            "verb": "type", "app": "Notes", "partial": true}),
        NOW,
    );
    assert_eq!(refused["ok"], false, "older apps still show it as not done");
    assert_eq!(refused["nothingSent"], true);
    assert_eq!(refused["partial"], true);
    assert_eq!(refused["uncertain"], false);
    assert_eq!(refused["says"], "stopped typing in Notes");

    let unknown = shown(
        &json!({"operation": "input", "ok": false, "code": "uncertain", "verb": "click"}),
        NOW,
    );
    assert_eq!(unknown["nothingSent"], false);
    assert_eq!(unknown["partial"], false);
    assert_eq!(unknown["uncertain"], true);
}

fn report(reachable: bool, ready: bool) -> Value {
    json!({
        "ready": ready,
        "nextSteps": ["Run atlas start-input"],
        "capabilities": {"nativeDialogs": {"state": "failed"}},
        "session": {"ok": reachable, "user": {"name": "atlastest"},
            "inputService": {"controlActive": false}},
    })
}

fn row(seconds_ago: u128) -> Row {
    logged(
        json!({"at": (NOW - seconds_ago * 1_000_000_000).to_string(),
        "operation": "input", "ok": true, "actionCount": 1}),
    )
}

fn unresolved(count: usize) -> Pending {
    Pending {
        unresolved: count,
        ..Pending::default()
    }
}

fn mode(report: &Value, lease: bool, pending: usize, activity: &[Row]) -> Value {
    overview(report, lease, unresolved(pending), activity, NOW)["mode"].clone()
}

#[test]
fn a_not_ready_helper_shows_only_valid_display_geometry() {
    let mut report = report(true, false);
    for display in [
        json!({"width": 1e100, "height": 1, "scale": 1}),
        json!({"width": 1, "height": 32769, "scale": 1}),
        json!({"width": 0, "height": 1, "scale": 1}),
        json!({"width": 1, "height": 1, "scale": 9}),
        json!({"width": f64::INFINITY, "height": 1, "scale": 1}),
    ] {
        report["session"]["display"] = display;
        let shown = overview(&report, false, unresolved(0), &[], NOW);
        assert_eq!(shown["state"], "notReady");
        assert!(shown["display"].is_null());
    }
    let display = json!({"width": 32768, "height": 1, "scale": 8});
    report["session"]["display"] = display.clone();
    assert_eq!(
        overview(&report, false, unresolved(0), &[], NOW)["display"],
        display
    );
}

#[test]
fn every_mode_has_one_of_the_states_agent_apps_show() {
    let ready = report(true, true);
    let state = |report: &Value, lease: bool, pending: usize, activity: &[Row]| {
        let summary = overview(report, lease, unresolved(pending), activity, NOW);
        (summary["state"].clone(), summary["label"].clone())
    };

    assert_eq!(
        state(&ready, false, 0, &[row(2)]),
        (json!("working"), json!("Working"))
    );
    assert_eq!(state(&ready, false, 0, &[]), (json!("idle"), json!("Idle")));
    assert_eq!(
        state(&ready, true, 0, &[]),
        (json!("control"), json!("You have control"))
    );
    assert_eq!(
        state(&ready, false, 1, &[]),
        (json!("needsYou"), json!("Needs you"))
    );

    let mut stopped = report(true, false);
    stopped["nextSteps"] = json!([crate::session::START_INPUT]);
    assert_eq!(
        state(&stopped, false, 0, &[]).0,
        "needsYou",
        "Recover fixes it"
    );
    assert_eq!(state(&report(true, false), false, 0, &[]).0, "notReady");
    assert_eq!(state(&report(false, false), false, 0, &[]).0, "notReady");
}

#[test]
fn the_headline_is_the_newest_thing_an_agent_did() {
    let ready = report(true, true);
    let mut save = row(2);
    save.verb = Some("click".into());
    save.app = Some("TextEdit".into());
    save.label = Some("Save".into());
    let mut own = row(1);
    own.actor = Some("viewer".into());

    let summary = overview(&ready, false, unresolved(0), &[own, save], NOW);
    assert_eq!(summary["headline"], "Clicked Save in TextEdit");
    assert_eq!(summary["activity"][1]["says"], "clicked Save in TextEdit");
    assert_eq!(summary["activity"][0]["says"], "sent 1 action");

    let quiet = overview(&ready, false, unresolved(0), &[], NOW);
    assert_eq!(quiet["headline"], quiet["reason"], "nothing done yet");

    let unsure = overview(&ready, false, unresolved(1), &[row(2)], NOW);
    assert_eq!(
        unsure["headline"], unsure["reason"],
        "why it needs the person"
    );
}

#[test]
fn a_stop_shows_until_resumed_but_an_unknown_outcome_comes_first() {
    let mut held = report(true, true);
    held["session"]["inputService"]["held"] = json!(true);

    let summary = overview(&held, false, unresolved(0), &[row(1)], NOW);
    assert_eq!(
        (&summary["mode"], &summary["state"], &summary["label"]),
        (&json!("stopped"), &json!("stopped"), &json!("Stopped"))
    );
    assert_eq!(
        summary["headline"],
        "You stopped the agent. It waits until you resume it"
    );
    assert_eq!(
        mode(&held, true, 0, &[]),
        "controlling",
        "the person may take over"
    );
    assert_eq!(mode(&held, false, 1, &[]), "recovering");
}

#[test]
fn input_failure_after_a_stop_offers_recovery_and_keeps_agents_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, dir.path());
    controller.note_stop(true).unwrap();
    let mut held = report(true, true);
    held["session"]["inputService"]["held"] = json!(true);
    assert_eq!(mode(&held, false, 0, &[]), "stopped");

    held["ready"] = json!(false);
    held["nextSteps"] = json!([crate::session::START_INPUT]);
    let summary = overview(&held, false, pending(&controller).0, &[], NOW);
    assert_eq!(summary["mode"], "blocked");
    assert_eq!(summary["state"], "needsYou");
    assert_eq!(summary["recoveryHelps"], true);
    assert!(controller.stop_noted());
    assert_eq!(held["session"]["inputService"]["held"], true);

    held["ready"] = json!(true);
    let recovered = overview(&held, false, pending(&controller).0, &[], NOW);
    assert_eq!(recovered["mode"], "stopped");
    assert_eq!(recovered["recoveryHelps"], false);
    assert!(controller.stop_noted());
    assert_eq!(held["session"]["inputService"]["held"], true);

    controller.note_stop(false).unwrap();
    held["session"]["inputService"]["held"] = json!(false);
    assert_eq!(mode(&held, false, 0, &[]), "observing");
}

#[test]
fn a_permission_failure_after_a_stop_shows_what_must_be_allowed_before_resuming() {
    let dir = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, dir.path());
    controller.note_stop(true).unwrap();
    let mut held = report(true, true);
    held["session"]["inputService"]["held"] = json!(true);
    assert_eq!(mode(&held, false, 0, &[]), "stopped");

    held["ready"] = json!(false);
    held["nextSteps"] = json!(["Turn on Accessibility for Atlas in the helper first."]);
    let summary = overview(&held, false, pending(&controller).0, &[], NOW);
    assert_eq!(summary["mode"], "blocked");
    assert_eq!(summary["state"], "notReady");
    assert_eq!(summary["reason"], held["nextSteps"][0]);
    assert_eq!(summary["recoveryHelps"], false);
    assert!(controller.stop_noted());
    assert_eq!(held["session"]["inputService"]["held"], true);

    held["ready"] = json!(true);
    assert_eq!(mode(&held, false, 0, &[]), "stopped");
}

#[test]
fn an_unknown_outcome_keeps_its_claim_while_recovery_waits_for_a_prerequisite() {
    let dir = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, dir.path());
    drop(controller.quarantine().unwrap());
    let claims = controller.pending().unwrap();
    let mut report = report(true, true);
    let summary = overview(&report, false, pending(&controller).0, &[], NOW);
    assert_eq!(summary["mode"], "recovering");
    assert_eq!(summary["recoveryHelps"], true);

    for step in [
        "Turn on Accessibility for Atlas in the helper first.",
        "Allow Screen Recording for Atlas in the helper first.",
        "Switch back to your own account without logging the helper out.",
    ] {
        report["ready"] = json!(false);
        report["nextSteps"] = json!([step]);
        let summary = overview(&report, false, pending(&controller).0, &[], NOW);
        assert_eq!(summary["mode"], "recovering");
        assert_eq!(summary["state"], "needsYou");
        assert_eq!(summary["pending"], 1);
        assert_eq!(summary["reason"], step);
        assert_eq!(summary["recoveryHelps"], false);
        assert_eq!(controller.pending().unwrap(), claims);
    }

    report["nextSteps"] = json!([crate::session::START_INPUT]);
    let summary = overview(&report, false, pending(&controller).0, &[], NOW);
    assert_eq!(summary["mode"], "recovering");
    assert_eq!(summary["recoveryHelps"], true);
    assert_eq!(controller.pending().unwrap(), claims);

    report["ready"] = json!(true);
    assert_eq!(mode(&report, false, 1, &[]), "recovering");
    controller.clear().unwrap();
    assert_eq!(
        overview(&report, false, pending(&controller).0, &[], NOW)["mode"],
        "observing"
    );
}

#[test]
fn an_idle_headline_says_what_was_done_last() {
    let mut save = row(60);
    save.verb = Some("click".into());
    save.label = Some("Save".into());

    let summary = overview(&report(true, true), false, unresolved(0), &[save], NOW);
    assert_eq!(summary["state"], "idle");
    assert_eq!(summary["headline"], "Last: clicked Save");
}

#[test]
fn input_that_recover_restarts_is_offered_as_recover_not_a_command() {
    let mut stopped = report(true, false);
    stopped["nextSteps"] = json!([crate::session::START_INPUT]);
    let summary = overview(&stopped, false, unresolved(0), &[], NOW);
    assert_eq!(summary["mode"], "blocked");
    assert_eq!(summary["recoverable"], true);
    assert!(!summary["reason"].as_str().unwrap().contains("atlas "));

    let mut settings = report(true, false);
    settings["nextSteps"] = json!(["Turn on Accessibility for Atlas in the helper first."]);
    let summary = overview(&settings, false, unresolved(0), &[], NOW);
    assert_eq!(summary["recoverable"], false);
    assert_eq!(summary["reason"], settings["nextSteps"][0]);
}

#[test]
fn modes_follow_reachability_uncertainty_readiness_and_control() {
    let ready = report(true, true);
    assert_eq!(mode(&report(false, false), false, 0, &[]), "unavailable");
    assert_eq!(
        mode(&ready, true, 1, &[]),
        "recovering",
        "uncertainty first"
    );
    assert_eq!(mode(&report(true, false), false, 0, &[]), "blocked");
    assert_eq!(mode(&ready, true, 0, &[]), "controlling");
    assert_eq!(mode(&ready, false, 0, &[row(2)]), "agent");
    assert_eq!(mode(&ready, false, 0, &[row(30)]), "observing");

    let mut own = row(1);
    own.actor = Some("viewer".into());
    let mut restart = row(1);
    restart.operation = Some(Op::Restart);

    assert_eq!(
        mode(&ready, false, 0, &[own.clone(), restart, row(30)]),
        "observing",
        "the person's input and recovery are not an agent"
    );
    assert_eq!(mode(&ready, false, 0, &[own, row(2)]), "agent");

    let mut native = report(true, true);
    native["session"]["inputService"]["controlActive"] = json!(true);
    assert_eq!(mode(&native, false, 0, &[]), "agent");

    let working = Pending {
        in_flight: 1,
        ..Pending::default()
    };

    assert_eq!(
        overview(&ready, false, working, &[], NOW)["mode"],
        "agent",
        "work in progress is not an unknown outcome"
    );
    assert_eq!(
        overview(&ready, true, working, &[], NOW)["mode"],
        "controlling"
    );
    assert_eq!(
        overview(&report(true, false), false, working, &[], NOW)["mode"],
        "agent",
        "an input start or upgrade in progress is not a blocked helper"
    );

    let blocked = overview(&report(true, false), false, unresolved(0), &[], NOW);
    assert_eq!(blocked["reason"], "Run atlas start-input");
    assert_eq!(
        overview(&ready, false, unresolved(0), &[row(30)], NOW)["activity"][0]["ageSeconds"],
        30
    );
}

#[test]
fn recovery_says_it_waits_for_a_running_command() {
    let ready = report(true, true);
    let waiting = Pending {
        unresolved: 1,
        in_flight: 1,
        commands: 1,
    };

    let shown = overview(&ready, false, waiting, &[], NOW);
    assert_eq!(shown["mode"], "recovering");
    assert_eq!(shown["commands"], 1);
    assert_eq!(shown["recoveryHelps"], false, "recovery waits for it");
    assert_eq!(
        overview(&ready, false, unresolved(1), &[], NOW)["recoveryHelps"],
        true
    );
    assert_eq!(
        shown["reason"],
        "An action's outcome is unknown; recovery waits for the running command to end"
    );
    assert_eq!(
        overview(&ready, false, unresolved(1), &[], NOW)["reason"],
        "An action's outcome is unknown; recover before taking control",
        "without a running command recovery can go ahead"
    );
}

#[test]
fn control_says_a_command_is_still_running() {
    let ready = report(true, true);
    let running = Pending {
        in_flight: 1,
        commands: 1,
        ..Pending::default()
    };
    let input = Pending {
        in_flight: 1,
        ..Pending::default()
    };

    let shown = overview(&ready, true, running, &[], NOW);
    assert_eq!(shown["mode"], "controlling");
    assert_eq!(
        shown["reason"],
        "You have control. A command an agent started is still running"
    );
    assert_eq!(
        overview(&ready, true, input, &[], NOW)["reason"],
        "You have control. The agent waits",
        "the person's own input under way is no command"
    );
    assert_eq!(
        overview(&ready, false, running, &[], NOW)["reason"],
        "Work is in progress in the helper"
    );
}

#[test]
fn a_silent_helper_is_explained_without_commands() {
    let mut silent = report(false, false);
    silent["absence"] = json!("loggedOut");
    silent["account"] = json!("atlas");
    let logged_out = overview(&silent, false, unresolved(1), &[], NOW);
    assert_eq!(logged_out["mode"], "loggedOut", "before any recovery");
    assert_eq!(logged_out["account"], "atlas");

    silent["absence"] = json!("missing");
    assert_eq!(mode(&silent, false, 0, &[]), "unavailable");

    for absence in [json!("missing"), Value::Null] {
        silent["absence"] = absence;
        let reason = overview(&silent, false, unresolved(0), &[], NOW)["reason"].clone();
        assert!(!reason.as_str().unwrap().contains("atlas "), "{reason}");
    }
}

#[test]
fn older_and_partial_rows_still_read_and_show_as_they_always_did() {
    let dir = tempfile::tempdir().unwrap();
    let log = [
        // A row with no outcome, actor or step: from before any of them.
        r#"{"at":"40000000000","operation":"run"}"#,
        // Whole-number milliseconds and a field no build reads.
        r#"{"at":"50000000000","operation":"input","ok":true,"elapsedMs":12,"actionCount":3,"verb":"key","label":"⌘S","app":"TextEdit","extra":{"x":1}}"#,
        r#"{"operation":7}"#,
        "not json",
        r#"{"at":"soon","operation":"screenshot","ok":false,"code":"permission","actor":"viewer","capture":"1-capture.png"}"#,
        r#"{"at":"90000000000","operation":"input","ok":false,"code":"focus-changed","reason":"sheet","partial":true,"verb":"type","session":"0123456789ab","command":"c1"}"#,
        r#"{"at":"95000000000","operation":"input","ok":null,"started":true,"verb":"type","command":"c2","task":"ba9876543210"}"#,
        r#"{"at":"99000000000","operation":"run","ok":false,"code":"uncertain","verb":"open","app":"Calculator","command":"c3"}"#,
    ]
    .join("\n");
    fs::write(dir.path().join(activity_log::FILE), format!("{log}\n")).unwrap();
    let rows = activity_log::recent(dir.path(), 10);
    let shown: Vec<String> = rows
        .iter()
        .map(|row| json!(super::shown(row, NOW)).to_string())
        .collect();
    assert_eq!(
        shown,
        [
            r#"{"actionCount":null,"actor":"agent","ageSeconds":1,"app":"Calculator","bundleId":null,"capture":null,"id":"436d0e30331dd417","label":null,"nothingSent":false,"ok":false,"operation":"run","partial":false,"reason":"It may not have finished","says":"couldn't open Calculator","session":null,"started":false,"uncertain":true,"verb":"open"}"#,
            r#"{"actionCount":null,"actor":"agent","ageSeconds":5,"app":null,"bundleId":null,"capture":null,"id":"78ca8deb5a96fe76","label":null,"nothingSent":false,"ok":true,"operation":"input","partial":false,"reason":null,"says":"running a command…","session":null,"started":true,"uncertain":false,"verb":"type"}"#,
            r#"{"actionCount":null,"actor":"agent","ageSeconds":10,"app":null,"bundleId":null,"capture":null,"id":"182c45e5106ca226","label":null,"nothingSent":true,"ok":false,"operation":"input","partial":true,"reason":"A sheet was open over the window","says":"stopped typing","session":"0123456789ab","started":false,"uncertain":false,"verb":"type"}"#,
            r#"{"actionCount":null,"actor":"viewer","ageSeconds":null,"app":null,"bundleId":null,"capture":"1-capture.png","id":"07e383625bdf0af1","label":null,"nothingSent":false,"ok":false,"operation":"screenshot","partial":false,"reason":"The helper can't record the screen","says":"couldn't take a screenshot","session":null,"started":false,"uncertain":false,"verb":null}"#,
            r#"{"actionCount":3,"actor":"agent","ageSeconds":50,"app":"TextEdit","bundleId":null,"capture":null,"id":"355e7aff1bb0ef3c","label":"⌘S","nothingSent":false,"ok":true,"operation":"input","partial":false,"reason":null,"says":"pressed ⌘S in TextEdit","session":null,"started":false,"uncertain":false,"verb":"key"}"#,
            r#"{"actionCount":null,"actor":"agent","ageSeconds":60,"app":null,"bundleId":null,"capture":null,"id":"6f5d59054d040875","label":null,"nothingSent":false,"ok":false,"operation":"run","partial":false,"reason":null,"says":"couldn't run a command","session":null,"started":false,"uncertain":false,"verb":null}"#,
        ]
    );
}

#[test]
fn why_the_helper_is_silent_is_looked_at_again_within_seconds() {
    assert!(ABSENCE.fresh_for <= Duration::from_secs(5));
    assert!(ABSENCE.fresh_for < LOGINS.fresh_for);
}

#[test]
fn the_overview_counts_running_jobs() {
    let mut ready = report(true, true);
    assert_eq!(
        overview(&ready, false, unresolved(0), &[], NOW)["jobs"],
        json!({"running": 0, "launching": 0, "unresolved": 0})
    );
    ready["session"]["daemon"] = json!({"jobs": {"running": 3, "launching": 1, "unresolved": 2}});
    assert_eq!(
        overview(&ready, false, unresolved(0), &[], NOW)["jobs"],
        json!({"running": 3, "launching": 1, "unresolved": 2})
    );
    let refused = logged(json!({"ok": false, "code": "job", "operation": "job"}));
    assert_eq!(reason(&refused), Some("A background job was refused"));
    let refused = logged(json!({"ok": false, "code": "collect", "operation": "collect"}));
    assert_eq!(reason(&refused), Some("A file could not be collected"));
}
