use super::*;
use crate::{
    client::tests::{labelled, persons_client, rows},
    doing::Doing,
    task::Task,
    test_support::client_in,
};
use std::{
    fs::{self, OpenOptions},
    os::unix::fs::OpenOptionsExt,
};

#[tokio::test(start_paused = true)]
async fn the_input_timer_keeps_a_newer_command_first_in_recent_activity() {
    let directory = tempfile::tempdir().unwrap();
    let input = PersonInput::default();
    let row = |at: &str, actor: &str| {
        Row::parse(&json!({
            "at": at, "operation": "input", "actor": actor,
            "ok": true, "actionCount": 1
        }))
        .unwrap()
    };
    input
        .gather(directory.path(), row("1000000000", "viewer"))
        .unwrap();
    let timer = input.untimed().unwrap();
    activity_log::append(directory.path(), &row("2000000000", "agent")).unwrap();
    tokio::time::advance(PERSON_INPUT_SPAN).await;
    input.flush_due(timer).unwrap();

    let one = activity_log::recent(directory.path(), 1);
    assert_eq!(one[0].at.as_deref(), Some("2000000000"));
    let both = activity_log::recent(directory.path(), 2);
    assert_eq!(
        both.iter().map(Row::actor).collect::<Vec<_>>(),
        ["agent", "viewer"]
    );
}

#[tokio::test(start_paused = true)]
async fn input_on_either_side_of_local_midnight_stays_separate_when_buffered_and_read() {
    if !atlas_core::test_support::in_timezone(
        "client::record::tests::input_on_either_side_of_local_midnight_stays_separate_when_buffered_and_read",
        "Asia/Tbilisi",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let input = PersonInput::default();
    // Midnight at UTC+4, while both timestamps belong to the same UTC day.
    let midnight = 72_000_000;
    let times = [midnight - 2, midnight - 1, midnight, midnight + 1];
    for at in times {
        let row = Row::parse(&json!({
            "at": (u128::from(at) * 1_000_000).to_string(),
            "operation": "input", "actor": "viewer", "ok": true,
            "actionCount": 1, "elapsedMs": 2.5
        }))
        .unwrap();
        input.gather(directory.path(), row).unwrap();
    }

    let read = || {
        std::fs::read_to_string(directory.path().join(activity_log::FILE))
            .unwrap()
            .lines()
            .map(|line| Row::parse(&serde_json::from_str(line).unwrap()).unwrap())
            .map(|row| (row.at_ms(), row.action_count, row.elapsed_ms))
            .collect::<Vec<_>>()
    };
    assert_eq!(read(), [(Some(midnight - 1), Some(2), Some(5.0))]);
    input.flush().unwrap();
    assert_eq!(
        read(),
        [
            (Some(midnight - 1), Some(2), Some(5.0)),
            (Some(midnight + 1), Some(2), Some(5.0))
        ]
    );
    let shown = activity_log::recent(directory.path(), 5);
    assert_eq!(shown.len(), 2);
    assert_eq!(shown[0].at_ms(), Some(midnight + 1));
    assert_eq!(shown[1].at_ms(), Some(midnight - 1));
    assert!(shown.iter().all(|row| row.action_count == Some(2)));
}

#[test]
fn an_agent_row_carries_its_host_session_key() {
    let dir = tempfile::tempdir().unwrap();
    let client = labelled(&client_in(dir.path()));
    let key = client.host_session.as_ref().unwrap().key().clone();
    client.record_for_test(
        &json!({"op": "input", "actions": [{}]}),
        &json!({"ok": true}),
    );
    assert_eq!(rows(&client)[0]["session"], key.as_str());

    let mut viewer = client.clone();
    viewer.actor = Actor::Viewer;
    viewer.record_for_test(
        &json!({"op": "input", "actions": [{}]}),
        &json!({"ok": true}),
    );
    assert!(
        rows(&client)[1].get("session").is_none(),
        "the person's never does"
    );
}

#[test]
fn every_kind_of_row_is_written_byte_for_byte_as_the_log_keeps_it() {
    let dir = tempfile::tempdir().unwrap();
    let agent = labelled(&client_in(dir.path()));
    let key = agent.host_session.as_ref().unwrap().key().clone();
    let mut tasked = agent
        .clone()
        .doing(Doing::new("type").app(Some("TextEdit")));
    tasked.task = Some(Task::of(&key, Some("settings-sheet")).unwrap());
    let typed = json!({"op": "input", "actions": [{}, {}]});
    tasked.record_start_blocking(&typed).unwrap();
    tasked
        .record_blocking(&typed, &json!({"ok": true}), Duration::from_millis(5))
        .unwrap();
    let ran = agent
        .clone()
        .doing(Doing::new("open").app(Some("Calculator")));
    let run = json!({"op": "run", "argv": ["open"]});
    ran.record_blocking(&run, &json!({"ok": true, "exitCode": 2}), Duration::ZERO)
        .unwrap();
    ran.record_blocking(&run, &json!({"ok": true, "timedOut": true}), Duration::ZERO)
        .unwrap();
    let refused = agent.clone().doing(Doing::new("click"));
    refused
        .record_blocking(
            &typed,
            &json!({"ok": false, "code": "focus-changed", "performed": 1, "reason": "window"}),
            Duration::ZERO,
        )
        .unwrap();
    agent
        .clone()
        .doing(Doing::new("collect"))
        .record_blocking(
            &json!({"op": "collect"}),
            &json!({"ok": true, "count": 2, "bytes": 30}),
            Duration::ZERO,
        )
        .unwrap();
    agent
        .clone()
        .doing(Doing::new("screenshot"))
        .record_blocking(
            &json!({"op": "screenshot"}),
            &json!({"ok": true, "capture": "1761660429000123456-capture.png"}),
            Duration::ZERO,
        )
        .unwrap();
    let mut viewer = client_in(dir.path());
    viewer.actor = Actor::Viewer;
    viewer
        .record_blocking(
            &json!({"op": "restart"}),
            &json!({"ok": true}),
            Duration::ZERO,
        )
        .unwrap();

    let log = fs::read_to_string(agent.output_dir().unwrap().join(activity_log::FILE)).unwrap();
    let lines: Vec<String> = log
        .lines()
        .map(|line| {
            let row: Value = serde_json::from_str(line).unwrap();
            let mut line = line.replace(row["at"].as_str().unwrap(), "AT");
            if let Some(command) = row["command"].as_str() {
                line = line.replace(command, "COMMAND");
            }
            line.replace(key.as_str(), "SESSION")
                .replace(tasked.task.as_ref().unwrap().key().as_str(), "TASK")
        })
        .collect();
    assert_eq!(
        lines,
        [
            r#"{"actionCount":2,"actor":"agent","app":"TextEdit","at":"AT","command":"COMMAND","elapsedMs":0.0,"ok":null,"operation":"input","session":"SESSION","started":true,"task":"TASK","verb":"type"}"#,
            r#"{"actionCount":2,"actor":"agent","app":"TextEdit","at":"AT","command":"COMMAND","elapsedMs":5.0,"ok":true,"operation":"input","session":"SESSION","task":"TASK","verb":"type"}"#,
            r#"{"actionCount":null,"actor":"agent","app":"Calculator","at":"AT","code":"exit","command":"COMMAND","elapsedMs":0.0,"ok":false,"operation":"run","session":"SESSION","verb":"open"}"#,
            r#"{"actionCount":null,"actor":"agent","app":"Calculator","at":"AT","code":"timeout","command":"COMMAND","elapsedMs":0.0,"ok":false,"operation":"run","session":"SESSION","verb":"open"}"#,
            r#"{"actionCount":2,"actor":"agent","at":"AT","code":"focus-changed","command":"COMMAND","elapsedMs":0.0,"ok":false,"operation":"input","partial":true,"reason":"window","session":"SESSION","verb":"click"}"#,
            r#"{"actionCount":null,"actor":"agent","at":"AT","bytes":30,"command":"COMMAND","count":2,"elapsedMs":0.0,"ok":true,"operation":"collect","session":"SESSION","verb":"collect"}"#,
            r#"{"actionCount":null,"actor":"agent","at":"AT","capture":"1761660429000123456-capture.png","command":"COMMAND","elapsedMs":0.0,"ok":true,"operation":"screenshot","session":"SESSION","verb":"screenshot"}"#,
            r#"{"actionCount":null,"actor":"viewer","at":"AT","elapsedMs":0.0,"ok":true,"operation":"restart"}"#,
        ]
    );
}

#[test]
fn a_refusal_for_another_app_in_front_says_whether_some_was_sent() {
    let dir = tempfile::tempdir().unwrap();
    let client = labelled(&client_in(dir.path()));
    let request = json!({"op": "input", "actions": [{}, {}]});
    for reply in [
        json!({"ok": false, "code": "focus-changed", "performed": 0, "partial": false}),
        json!({"ok": false, "code": "focus-changed", "performed": 1, "partial": false}),
        json!({"ok": false, "code": "focus-changed", "performed": 0, "partial": true}),
        json!({"ok": false, "code": "busy"}),
    ] {
        client.record_for_test(&request, &reply);
    }
    let partial: Vec<Value> = rows(&client)
        .iter()
        .map(|row| row["partial"].clone())
        .collect();
    assert_eq!(
        partial,
        [json!(false), json!(true), json!(true), Value::Null]
    );
}

#[test]
fn a_focus_refusals_row_keeps_why_it_was_refused_in_the_services_own_words_only() {
    let dir = tempfile::tempdir().unwrap();
    let client = labelled(&client_in(dir.path()));
    let request = json!({"op": "input", "actions": [{}]});
    for reason in [
        json!("app"),
        json!("point"),
        json!("unreadable"),
        json!("window"),
        json!("sheet"),
        json!("field"),
        json!("disabled"),
        json!("Calculator has keyboard focus"),
        Value::Null,
    ] {
        client.record_for_test(
            &request,
            &json!({"ok": false, "code": "focus-changed", "reason": reason, "performed": 0}),
        );
    }
    client.record_for_test(
        &request,
        &json!({"ok": false, "code": "busy", "reason": "app"}),
    );
    let reasons: Vec<Value> = rows(&client)
        .iter()
        .map(|row| row["reason"].clone())
        .collect();
    assert_eq!(
        reasons,
        [
            json!("app"),
            json!("point"),
            json!("unreadable"),
            json!("window"),
            json!("sheet"),
            json!("field"),
            json!("disabled"),
            Value::Null,
            Value::Null,
            Value::Null,
        ]
    );
}

#[test]
fn input_names_the_app_it_landed_in_and_a_command_its_start() {
    let dir = tempfile::tempdir().unwrap();
    let client = client_in(dir.path()).doing(Doing::new("type"));
    let typed = json!({"op": "input", "actions": [{"type": "type", "text": "x"}]});
    client
        .record_blocking(
            &typed,
            &json!({"ok": true, "app": "TextEdit"}),
            Duration::ZERO,
        )
        .unwrap();
    let rows = crate::activity_log::recent(client.output_dir().unwrap(), 5);
    assert_eq!(crate::doing::says(&rows[0]), "typed in TextEdit");

    let client = client.doing(Doing::new("open").app(Some("Calculator")));
    let run = json!({"op": "run", "argv": ["open", "-a", "Calculator"]});
    client.record_start_blocking(&run).unwrap();
    let rows = crate::activity_log::recent(client.output_dir().unwrap(), 5);
    assert_eq!(crate::doing::says(&rows[0]), "opening Calculator…");
    client
        .record_blocking(&run, &json!({"ok": true, "exitCode": 0}), Duration::ZERO)
        .unwrap();
    let rows = crate::activity_log::recent(client.output_dir().unwrap(), 5);
    assert_eq!(
        crate::doing::says(&rows[0]),
        "opened Calculator",
        "the end stands for the start"
    );
    assert_eq!(rows.len(), 2, "one row per command");
}

#[test]
fn input_names_the_bundle_of_the_app_it_landed_in_as_the_helper_saw_it() {
    let dir = tempfile::tempdir().unwrap();
    let client = client_in(dir.path()).doing(Doing::new("click"));
    let clicked = json!({"op": "input", "actions": [{"type": "click", "x": 1, "y": 2}]});
    let landed = |reply: Value| {
        client
            .record_blocking(&clicked, &reply, Duration::ZERO)
            .unwrap();
        activity_log::recent(client.output_dir().unwrap(), 1).remove(0)
    };

    let row = landed(json!({"ok": true, "app": "TextEdit", "bundleId": "com.apple.TextEdit"}));
    assert_eq!(row.app.as_deref(), Some("TextEdit"));
    assert_eq!(row.bundle_id.as_deref(), Some("com.apple.TextEdit"));

    for odd in [
        json!("/Applications/TextEdit.app"),
        json!("com.apple TextEdit"),
        json!(7),
    ] {
        let row = landed(json!({"ok": true, "app": "TextEdit", "bundleId": odd}));
        assert_eq!(row.bundle_id, None, "{odd}");
    }
    let row = landed(json!({"ok": true, "bundleId": "com.apple.TextEdit"}));
    assert_eq!(
        (row.app, row.bundle_id),
        (None, None),
        "an identifier for no app named"
    );
}

#[test]
fn typed_text_names_its_field_only_by_the_role_the_helper_found_it_of() {
    let dir = tempfile::tempdir().unwrap();
    let client =
        client_in(dir.path()).doing(Doing::new("type").app(Some("Notes")).field(Some("Email")));
    let typed = json!({"op": "input", "actions": [{"type": "type", "text": "sam@example.com"}]});
    let landed = |reply: Value| {
        client
            .record_blocking(&typed, &reply, Duration::ZERO)
            .unwrap();
        activity_log::recent(client.output_dir().unwrap(), 1).remove(0)
    };

    let row = landed(json!({"ok": true, "app": "Notes",
        "field": {"role": "AXTextField", "value": "sam@example.com", "name": "private-page-text"}}));
    assert_eq!(row.label.as_deref(), Some("Email"));
    assert_eq!(crate::doing::says(&row), "typed into Email in Notes");

    for unchecked in [
        json!({"ok": true, "app": "Notes"}),
        json!({"ok": true, "app": "Notes", "field": {"role": "AXTextArea"}}),
        json!({"ok": true, "app": "Notes", "field": {"role": 7}}),
        json!({"ok": true, "app": "Notes", "field": "AXTextField"}),
        json!({"ok": false, "code": "focus-changed", "reason": "field",
            "field": {"role": "AXTextField"}}),
    ] {
        let row = landed(unchecked.clone());
        assert_eq!(row.label, None, "{unchecked}");
    }
    let written =
        fs::read_to_string(client.output_dir().unwrap().join(activity_log::FILE)).unwrap();
    assert!(
        !written.contains("sam@example.com"),
        "typed text is never kept"
    );
    assert!(
        !written.contains("private-page-text"),
        "reply text is never kept"
    );
}

#[test]
fn a_bundle_id_seen_for_another_app_than_the_one_named_is_not_kept() {
    let dir = tempfile::tempdir().unwrap();
    let typed = json!({"op": "input", "actions": [{"type": "type", "text": "x"}]});
    let landed = |named: &str, seen: &str| {
        let client = client_in(dir.path()).doing(Doing::new("type").app(Some(named)));
        let reply = json!({"ok": true, "app": seen, "bundleId": "com.apple.calculator"});
        client
            .record_blocking(&typed, &reply, Duration::ZERO)
            .unwrap();
        activity_log::recent(client.output_dir().unwrap(), 1).remove(0)
    };

    let other = landed("TextEdit", "Calculator");
    assert_eq!(other.app.as_deref(), Some("TextEdit"));
    assert_eq!(other.bundle_id, None);
    let same = landed("calculator", "Calculator");
    assert_eq!(same.bundle_id.as_deref(), Some("com.apple.calculator"));
}

#[test]
fn an_install_row_keeps_the_installed_apps_bundle_id() {
    use base64::{Engine, engine::general_purpose::STANDARD};

    let dir = tempfile::tempdir().unwrap();
    let client =
        client_in(dir.path()).doing(Doing::new("install").app(Some("/tmp/Secret/Probe.app")));
    let run = json!({"op": "run", "argv": ["atlas", "environment-worker"]});
    let answered = |answer: Value| {
        let output = STANDARD.encode(answer.to_string());
        let reply = json!({"ok": true, "exitCode": 0, "timedOut": false, "signal": null,
            "events": [{"event": "output", "data": output}]});
        client
            .record_blocking(&run, &reply, Duration::ZERO)
            .unwrap();
        activity_log::recent(client.output_dir().unwrap(), 1).remove(0)
    };
    let installed = |app: &str, id: Value| {
        json!({"ok": true, "installed": true, "installId": "ID", "app": app,
            "path": "/Users/helper/Applications/Probe.app", "bundleId": id,
            "version": "1.0 (1)", "registered": true})
    };

    let row = answered(installed("Probe", json!("test.atlas.install-probe")));
    assert_eq!(crate::doing::says(&row), "installed Probe");
    assert_eq!(row.bundle_id.as_deref(), Some("test.atlas.install-probe"));
    let text =
        std::fs::read_to_string(client.output_dir().unwrap().join(activity_log::FILE)).unwrap();
    assert!(
        !text.contains("/Users/helper") && !text.contains("Secret"),
        "{text}"
    );

    assert_eq!(
        answered(installed("Other", json!("test.other"))).bundle_id,
        None
    );
    assert_eq!(answered(installed("Probe", json!(null))).bundle_id, None);
    assert_eq!(answered(installed("Probe", json!("a b"))).bundle_id, None);
    let refused = json!({"ok": false, "installId": "ID", "phase": "held", "error": "x"});
    assert_eq!(answered(refused).bundle_id, None);
}

/// A span no test outlasts, so gathering never ends while a test runs.
const LONG_SPAN: Duration = Duration::from_secs(3600);

/// A span advanced explicitly by timer tests.
const SHORT_SPAN: Duration = Duration::from_millis(200);

#[test]
fn the_persons_input_through_a_viewer_is_one_row_a_span_counting_every_action() {
    let dir = tempfile::tempdir().unwrap();
    let client = persons_client(dir.path(), LONG_SPAN);
    let clicked = json!({"op": "input", "actions": [{"type": "click", "x": 412, "y": 96}, {"type": "type", "text": "hunter2"}]});
    for _ in 0..30 {
        client
            .record_blocking(
                &clicked,
                &json!({"ok": true, "performed": 2}),
                Duration::ZERO,
            )
            .unwrap();
    }
    let output = client.output_dir().unwrap().to_path_buf();
    assert!(
        activity_log::recent(&output, 5).is_empty(),
        "gathered until its span ends"
    );

    let refused = json!({"ok": false, "code": "busy"});
    client
        .record_blocking(&clicked, &refused, Duration::ZERO)
        .unwrap();
    let rows = activity_log::recent(&output, 5);
    assert_eq!(
        rows.iter()
            .map(|row| (row.failed(), row.action_count))
            .collect::<Vec<_>>(),
        [(true, Some(2)), (false, Some(60))],
        "a failure is written at once, after the input before it"
    );

    client
        .record_blocking(&clicked, &json!({"ok": true}), Duration::ZERO)
        .unwrap();
    client.person_input.as_ref().unwrap().flush().unwrap();
    let text = std::fs::read_to_string(output.join(activity_log::FILE)).unwrap();
    assert_eq!(text.lines().count(), 3);
    // Digits can turn up in a row's times by chance, so coordinates are
    // looked for as fields, and the typed text as words.
    assert!(!text.contains("hunter2"), "no typed text");
    for line in text.lines() {
        let row: Value = serde_json::from_str(line).unwrap();
        for field in ["x", "y", "text", "actions"] {
            assert!(row.get(field).is_none(), "{field} in {line}");
        }
    }

    // An agent's input, and the person's elsewhere, are written at once.
    let agent = client_in(dir.path()).doing(Doing::new("click"));
    agent
        .record_blocking(&clicked, &json!({"ok": true}), Duration::ZERO)
        .unwrap();
    let mut unshared = persons_client(dir.path(), LONG_SPAN);
    unshared.person_input = None;
    unshared
        .record_blocking(&clicked, &json!({"ok": true}), Duration::ZERO)
        .unwrap();
    let text = std::fs::read_to_string(output.join(activity_log::FILE)).unwrap();
    assert_eq!(text.lines().count(), 5);
}

#[test]
fn a_failed_flush_keeps_every_buffered_action_until_the_log_can_be_written() {
    let dir = tempfile::tempdir().unwrap();
    let client = persons_client(dir.path(), LONG_SPAN);
    let output = client.output_dir().unwrap();
    fs::create_dir_all(output).unwrap();
    let blocked = output.join(activity_log::FILE);
    fs::create_dir(&blocked).unwrap();
    let input = json!({"op": "input", "actions": [{"type": "click"}]});
    client.record_for_test(&input, &json!({"ok": true}));
    let gathering = client.person_input.as_ref().unwrap();
    assert!(gathering.flush().is_err());
    assert!(
        client
            .record_blocking(&input, &json!({"ok": true}), Duration::ZERO)
            .is_err()
    );
    assert!(gathering.flush().is_err());

    fs::remove_dir(blocked).unwrap();
    // An interrupted writer can leave a prefix even after the disk is usable.
    fs::write(output.join(activity_log::FILE), b"{\"operation\":").unwrap();
    gathering.flush().unwrap();
    gathering.flush().unwrap();
    let text = fs::read_to_string(output.join(activity_log::FILE)).unwrap();
    assert_eq!(text.lines().count(), 2);
    assert_eq!(activity_log::recent(output, 1)[0].action_count, Some(2));
}

#[test]
fn the_persons_input_gathers_five_seconds_a_row_unless_made_with_another_span() {
    assert_eq!(PersonInput::default().span(), PERSON_INPUT_SPAN);
    assert_eq!(PERSON_INPUT_SPAN, Duration::from_secs(5));
    assert_eq!(PersonInput::with_span(SHORT_SPAN).span(), SHORT_SPAN);
}

#[tokio::test(start_paused = true)]
async fn the_persons_gathered_input_is_written_once_its_span_ends_with_nothing_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let client = persons_client(dir.path(), SHORT_SPAN);
    let output = client.output_dir().unwrap().to_path_buf();
    let clicked = json!({"op": "input", "actions": [{"type": "click", "x": 1, "y": 2}]});
    for _ in 0..3 {
        client
            .record_blocking(&clicked, &json!({"ok": true}), Duration::ZERO)
            .unwrap();
    }
    let gathering = client.person_input.clone().unwrap();
    let timer = gathering.untimed().unwrap();
    let mut writing = Box::pin(gathering.write_at(timer));
    assert!(activity_log::recent(&output, 5).is_empty());

    for _ in 0..2 {
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(writing.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        tokio::time::advance(SHORT_SPAN / 2).await;
    }
    writing.await.unwrap();
    let rows = activity_log::recent(&output, 5);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].action_count, Some(3));
    assert_eq!(rows[0].actor(), "viewer");
}

#[tokio::test(start_paused = true)]
async fn an_input_timer_keeps_the_deadline_set_when_gathering_began() {
    let dir = tempfile::tempdir().unwrap();
    let client = persons_client(dir.path(), SHORT_SPAN);
    client.record_for_test(
        &json!({"op": "input", "actions": [{"type": "click"}]}),
        &json!({"ok": true}),
    );
    let deadline = tokio::time::Instant::now() + SHORT_SPAN;
    tokio::time::advance(SHORT_SPAN / 2).await;
    let gathering = client.person_input.clone().unwrap();
    let timer = gathering.untimed().unwrap();
    let writing = tokio::spawn(gathering.write_at(timer));
    tokio::time::advance(SHORT_SPAN / 2).await;
    writing.await.unwrap().unwrap();
    assert_eq!(tokio::time::Instant::now(), deadline);
    assert_eq!(
        activity_log::recent(client.output_dir().unwrap(), 1)[0].action_count,
        Some(1)
    );
}

#[tokio::test(start_paused = true)]
async fn an_old_input_timer_cannot_flush_input_gathered_after_an_explicit_flush() {
    let dir = tempfile::tempdir().unwrap();
    let client = persons_client(dir.path(), SHORT_SPAN);
    let input = json!({"op": "input", "actions": [{"type": "click"}]});
    client.record_for_test(&input, &json!({"ok": true}));
    let gathering = client.person_input.clone().unwrap();
    let earlier = gathering.untimed().unwrap();
    tokio::time::advance(SHORT_SPAN / 2).await;
    gathering.flush().unwrap();
    client.record_for_test(&input, &json!({"ok": true}));
    let later = gathering.untimed().unwrap();
    assert!(gathering.untimed().is_none(), "each span gets one timer");

    let old_writing = tokio::spawn(gathering.clone().write_at(earlier));
    tokio::time::advance(SHORT_SPAN / 2).await;
    old_writing.await.unwrap().unwrap();
    let path = client.output_dir().unwrap().join(activity_log::FILE);
    assert_eq!(fs::read_to_string(&path).unwrap().lines().count(), 1);

    let new_writing = tokio::spawn(gathering.write_at(later));
    tokio::time::advance(SHORT_SPAN / 2).await;
    new_writing.await.unwrap().unwrap();
    assert_eq!(fs::read_to_string(path).unwrap().lines().count(), 2);
}

#[test]
fn a_command_that_failed_or_timed_out_is_recorded_as_failed() {
    assert!(worked(&json!({"ok": true})));
    assert!(worked(
        &json!({"ok": true, "exitCode": 0, "timedOut": false})
    ));
    assert!(!worked(&json!({"ok": true, "exitCode": 1})));
    assert!(!worked(
        &json!({"ok": true, "exitCode": 0, "timedOut": true})
    ));
    assert!(!worked(&json!({"ok": false})));
}

#[test]
fn a_failure_keeps_its_fixed_code_and_reads_as_plain_words_but_never_its_message() {
    let dir = tempfile::tempdir().unwrap();
    let click = json!({"op": "input", "actions": [{"type": "click"}]});
    let refused = json!({"ok": false, "code": "elements", "error": "No element is named secret"});
    let odd = json!({"ok": false, "code": "Not A Code: secret"});
    let failed = json!({"ok": true, "exitCode": 2});

    // One client per action: rows of one command read as its newest step.
    for reply in [&refused, &odd, &failed] {
        client_in(dir.path())
            .doing(Doing::new("click").app(Some("Opie")))
            .record_blocking(&click, reply, Duration::ZERO)
            .unwrap();
    }

    let client = client_in(dir.path());
    let directory = client.output_dir().unwrap();
    let log = std::fs::read_to_string(directory.join(activity_log::FILE)).unwrap();
    assert!(!log.contains("secret"), "{log}");
    let rows = crate::activity_log::recent(directory, 5);
    let reasons = rows
        .iter()
        .map(|row| json!(crate::overview::shown(row, 0).reason))
        .collect::<Vec<_>>();
    assert_eq!(
        reasons,
        [
            json!("The command failed"),
            Value::Null,
            json!("Nothing on screen matched")
        ]
    );
}

#[test]
fn recorded_rows_name_the_actor_and_what_was_done_but_never_typed_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = client_in(dir.path()).doing(Doing::new("type").app(Some("TextEdit")));
    let typed = json!({"op": "input", "actions": [{"type": "type", "text": "secret"}]});
    let ok = json!({"ok": true});

    client
        .record_blocking(&typed, &ok, Duration::from_millis(5))
        .unwrap();
    client.doing = None;
    client.actor = Actor::Viewer;
    client
        .record_blocking(&json!({"op": "restart"}), &ok, Duration::ZERO)
        .unwrap();

    let directory = client.output_dir().unwrap();
    let rows = crate::activity_log::recent(directory, 5);
    assert_eq!(
        (rows[0].actor(), &rows[0].operation),
        ("viewer", &Some(Op::Restart))
    );
    assert_eq!((rows[1].actor(), rows[1].action_count), ("agent", Some(1)));
    assert_eq!(
        (rows[1].verb.as_deref(), rows[1].app.as_deref()),
        (Some("type"), Some("TextEdit"))
    );
    assert!(rows[0].verb.is_none(), "a row says only what it knows");

    let log = fs::read_to_string(directory.join(activity_log::FILE)).unwrap();
    assert!(!log.contains("secret"), "no typed text is recorded");
}

#[test]
fn recording_refuses_non_files_without_waiting_for_a_reader() {
    const WORKER: &str = "ATLAS_TEST_METADATA_DIRECTORY";
    if let Some(directory) = std::env::var_os(WORKER) {
        let client = client_in(Path::new(&directory));
        assert!(client.record_start_blocking(&json!({"op": "run"})).is_err());
        return;
    }

    let mut refused = Vec::new();
    for name in [activity_log::LOCK, activity_log::FILE] {
        for kind in [
            "fifo",
            "fifo with reader",
            "directory",
            "link",
            "large link",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let client = client_in(dir.path());
            let output = client.output_dir().unwrap();
            let path = output.join(name);
            let target = output.join("personal-file");
            let contents = if kind == "large link" {
                vec![b'x'; 1024 * 1024 + 1]
            } else {
                b"kept".to_vec()
            };
            fs::write(&target, &contents).unwrap();
            match kind {
                "fifo" | "fifo with reader" => nix::unistd::mkfifo(
                    &path,
                    nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
                )
                .unwrap(),
                "directory" => fs::create_dir(&path).unwrap(),
                "link" | "large link" => std::os::unix::fs::symlink(&target, &path).unwrap(),
                _ => unreachable!(),
            }
            let _reader = (kind == "fifo with reader").then(|| {
                OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NONBLOCK)
                    .open(&path)
                    .unwrap()
            });

            // The synchronous open cannot be interrupted by a Tokio timeout.
            // Confine it to a child we always reap, even on the old blocking path.
            let mut child = atlas_core::spawn::spawn(
                atlas_core::test_support::rerun(
                    "client::record::tests::recording_refuses_non_files_without_waiting_for_a_reader",
                )
                .env(WORKER, dir.path())
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null()),
            )
            .unwrap();
            let deadline = Instant::now() + Duration::from_secs(2);
            let success = loop {
                if let Some(status) = child.try_wait().unwrap() {
                    break status.success();
                }
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    child.wait().unwrap();
                    break false;
                }
                std::thread::sleep(Duration::from_millis(10));
            };
            assert_eq!(fs::read(&target).unwrap(), contents);
            refused.push((name, kind, success));
        }
    }
    assert!(
        refused.iter().all(|(_, _, success)| *success),
        "{refused:?}"
    );
}

#[test]
fn recording_appends_privately_and_rotates_a_full_log() {
    let dir = tempfile::tempdir().unwrap();
    let client = client_in(dir.path()).doing(Doing::new("run"));
    let request = json!({"op": "run"});
    client.record_start_blocking(&request).unwrap();
    client
        .record_blocking(&request, &json!({"ok": true}), Duration::ZERO)
        .unwrap();
    let output = client.output_dir().unwrap();
    let path = output.join(activity_log::FILE);
    assert_eq!(fs::read_to_string(&path).unwrap().lines().count(), 2);
    for name in [activity_log::LOCK, activity_log::FILE] {
        assert_eq!(
            std::os::unix::fs::MetadataExt::mode(&fs::metadata(output.join(name)).unwrap()) & 0o777,
            0o600
        );
    }

    let rows = fs::read(&path).unwrap();
    let full = rows.repeat(1024 * 1024 / rows.len() + 1);
    fs::write(&path, &full).unwrap();
    client.record_start_blocking(&request).unwrap();
    assert_eq!(fs::read(output.join(activity_log::PREVIOUS)).unwrap(), full);
    let row: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(row["operation"], "run");
    assert_eq!(row["started"], true);
}

#[test]
fn app_screenshot_rows_keep_only_safe_identity_from_the_nested_reply() {
    let directory = tempfile::tempdir().unwrap();
    let client = client_in(directory.path()).doing(Doing::new("screenshot"));
    client.record_for_test(
        &json!({"op": "screenshot", "pid": 812}),
        &json!({
            "ok": true, "capture": "1761660429000123456-capture.png",
            "app": {"pid": 812, "name": "TextEdit", "bundleId": "com.apple.TextEdit",
                "title": "PRIVATE DOCUMENT TITLE", "bounds": {"x": 12}},
            "overlapping": [{"title": "ANOTHER PRIVATE TITLE"}], "png": "IMAGE PAYLOAD"
        }),
    );
    let row = activity_log::recent(client.output_dir().unwrap(), 1).remove(0);
    assert_eq!(row.app.as_deref(), Some("TextEdit"));
    assert_eq!(row.bundle_id.as_deref(), Some("com.apple.TextEdit"));
    assert_eq!(row.label, None);
    assert_eq!(
        row.capture.as_deref(),
        Some("1761660429000123456-capture.png")
    );
    let log = fs::read_to_string(client.output_dir().unwrap().join(activity_log::FILE)).unwrap();
    for private in ["PRIVATE", "bounds", "overlapping", "IMAGE PAYLOAD"] {
        assert!(!log.contains(private), "{private}: {log}");
    }
}

/// An agent's click in a command, as its row is made from `reply`.
fn clicked(reply: Value) -> Row {
    let directory = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client_in(directory.path());
    client.doing = Some(crate::doing::Doing::new("click"));
    client.row(
        &json!({"op": "input", "actions": [{"type": "click"}]}),
        &reply,
        Duration::ZERO,
        Step::End,
    )
}

#[test]
fn an_agents_input_row_names_the_listed_account_and_what_let_it_act_there() {
    let account = json!({"name": "github.com",
        "key": {"kind": "site", "id": "github.com", "browser": "com.google.Chrome"}});
    for (allowed, kept) in [
        ("once", activity_log::Allowed::Once),
        ("task", activity_log::Allowed::Task),
        ("trusted", activity_log::Allowed::Trusted),
    ] {
        let row = clicked(json!({"ok": true, "app": "Google Chrome",
            "account": account, "allowed": allowed}));
        assert_eq!(row.account.as_ref().unwrap().name, "github.com");
        assert_eq!(row.allowed, Some(kept), "{allowed}");
        assert_eq!(crate::doing::says_to_person(&row), "clicked on github.com");
        assert_eq!(crate::doing::says(&row), "clicked in Google Chrome");
    }
    // In an account that asks, a click that sent nothing needed allowing.
    let row = clicked(json!({"ok": true, "account": account}));
    assert!(row.account.is_some());
    assert_eq!(row.allowed, None);
}

#[test]
fn an_unlisted_app_or_an_allowance_without_its_account_names_none() {
    let row = clicked(json!({"ok": true, "app": "TextEdit"}));
    assert_eq!((row.account, row.allowed), (None, None));
    let row = clicked(json!({"ok": true, "app": "TextEdit", "allowed": "trusted"}));
    assert_eq!((row.account, row.allowed), (None, None));
    let row =
        clicked(json!({"ok": true, "account": {"name": " ", "key": {"kind": "app", "id": "x"}}}));
    assert_eq!(row.account, None, "a blank name is no account");
}
