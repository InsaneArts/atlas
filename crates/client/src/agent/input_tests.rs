use super::*;
use crate::agent::tests::display;
use atlas_core::actions::Batch;
use std::time::Duration;

#[test]
fn sugar_produces_valid_actions() {
    assert_eq!(click(1.0, 2.0, Button::Right, 1, &[])[0]["button"], "right");
    assert_eq!(
        drag((1.0, 2.0), (3.0, 4.0), Button::Middle)[0]["button"],
        "middle"
    );

    for actions in [
        click(
            10.0,
            20.0,
            Button::Right,
            2,
            &["cmd".into(), "shift".into()],
        ),
        drag((1.0, 2.0), (300.0, 400.0), Button::Left),
        keys(&["cmd+s".into(), "return".into()]).unwrap(),
    ] {
        Batch::decode(&actions).unwrap().check(&display()).unwrap();
    }
}

#[test]
fn typing_splits_long_text_on_grapheme_boundaries() {
    let family = "👨‍👩‍👧‍👦";
    let text = format!("{}{family}{}", "a".repeat(250), "b".repeat(300));
    let batches = type_batches(&text).unwrap();

    assert_eq!(batches.concat(), text);
    assert!(
        batches.iter().all(|t| t.chars().count() <= TYPE_BATCH),
        "each part is short enough to finish in time on a busy Mac"
    );
    assert!(
        batches.iter().any(|t| t.contains(family)),
        "cluster kept whole"
    );

    for batch in &batches {
        Batch::decode(&json!([{ "type": "type", "text": batch }]))
            .unwrap()
            .check(&display())
            .unwrap();
    }

    let decomposed = "e\u{301}".repeat(200);
    let batches = type_batches(&decomposed).unwrap();
    assert!(batches.iter().all(|t| !t.starts_with('\u{301}')));
    assert_eq!(batches.concat(), decomposed);

    assert!(type_batches("").is_err());

    let long_cluster = format!("a{}", "\u{301}".repeat(99));
    let batches = type_batches(&format!("xy{long_cluster}z")).unwrap();
    assert_eq!(
        batches,
        ["xy", long_cluster.as_str(), "z"],
        "a long cluster goes alone"
    );
}

#[test]
fn text_is_typed_in_parts_of_at_most_sixteen_characters() {
    let plain = "x".repeat(64);
    let parts = type_parts(&plain);
    assert_eq!(parts.len(), 4);
    assert!(parts.iter().all(|part| part.chars().count() == TYPE_PART));

    let family = "👨‍👩‍👧‍👦";
    let text = format!("{}{family}{}", "a".repeat(14), "b".repeat(20));
    let parts = type_parts(&text);
    assert_eq!(parts.concat(), text);
    assert!(parts.iter().any(|part| part.contains(family)), "kept whole");
    assert!(parts.iter().all(|part| part.chars().count() <= TYPE_PART));

    let decomposed = "e\u{301}".repeat(20);
    let parts = type_parts(&decomposed);
    assert!(parts.iter().all(|part| !part.starts_with('\u{301}')));
    assert_eq!(parts.concat(), decomposed);

    let long_cluster = format!("a{}", "\u{301}".repeat(30));
    assert_eq!(
        type_parts(&format!("xy{long_cluster}z")),
        ["xy", long_cluster.as_str(), "z"],
        "a cluster longer than a part goes alone"
    );

    for batch in type_batches(&"y".repeat(200)).unwrap() {
        let parts: Vec<Value> = type_parts(&batch)
            .iter()
            .map(|part| json!({"type": "type", "text": part}))
            .collect();
        assert!(parts.len() <= TYPE_BATCH / TYPE_PART);
        Batch::decode(&Value::Array(parts))
            .unwrap()
            .check(&display())
            .unwrap();
    }
}

/// The report of typing `batches` batches of 64 characters with no command
/// under way, the first `confirmed` of which the helper confirms before
/// the next ends with `failure`.
async fn stopped_at(batches: usize, confirmed: usize, failure: Result<Value>) -> Value {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let interrupt = Interrupt::new_for_test(Duration::from_secs(10));
    let batches = type_batches(&"z".repeat(64 * batches)).unwrap();
    let mut failure = Some(failure);
    let mut sent = 0;
    type_recorded(&client, &batches, &interrupt, async |actions: &Value| {
        sent += 1;
        if sent > confirmed {
            return failure.take().unwrap();
        }
        Ok(json!({"ok": true, "performed": actions.as_array().unwrap().len()}))
    })
    .await
}

#[tokio::test]
async fn a_stopped_type_counts_the_parts_the_helper_performed() {
    let refused = json!({"ok": false, "code": "focus-changed", "reason": "app",
        "found": {"app": "Calculator", "pid": 900}, "error": "Calculator has keyboard focus",
        "holder": "person", "unread": "slow",
        "performed": 2, "partial": false, "uncertain": false, "cleanupError": "kept out",
        "secret": "unrelated-refusal-secret-4b72", "id": "job", "task": "task", "state": "running"});

    let report = stopped_at(3, 1, Ok(refused.clone())).await;
    assert_eq!(report["typed"], 64 + 32);
    assert_eq!(report["batches"], 3);
    assert_eq!(report["confirmedBatches"], 1);
    assert_eq!(report["partial"], true);
    assert_eq!(report["uncertain"], false);
    assert_eq!(report["code"], "focus-changed");
    assert_eq!(report["error"], "Calculator has keyboard focus");
    assert_eq!(report["reason"], "app");
    assert_eq!(report["found"], refused["found"]);
    assert_eq!(
        report,
        json!({"ok": false, "code": "focus-changed", "reason": "app",
            "found": {"app": "Calculator", "pid": 900}, "error": "Calculator has keyboard focus",
            "holder": "person", "unread": "slow", "uncertain": false,
            "typed": 96, "batches": 3, "confirmedBatches": 1, "partial": true})
    );
    assert!(
        report.get("cleanupError").is_none(),
        "only what agents act on"
    );

    let first = stopped_at(1, 0, Ok(json!({"performed": 0, "code": "focus-changed"}))).await;
    assert_eq!(
        (first["typed"].clone(), first["partial"].clone()),
        (json!(0), json!(false))
    );
    let broke = json!({"performed": 0, "partial": true, "uncertain": true});
    assert_eq!(stopped_at(1, 0, Ok(broke)).await["partial"], true);

    let left = json!({"ok": false, "code": "focus-changed", "reason": "field",
        "found": {"app": "TextEdit", "pid": 812, "window": 4120,
            "field": {"role": "AXTextField", "name": "Second"}},
        "error": "The field with keyboard focus is not the one named First",
        "performed": 0, "partial": false, "uncertain": false});
    let report = stopped_at(4, 2, Ok(left.clone())).await;
    assert_eq!(
        (
            report["typed"].clone(),
            report["confirmedBatches"].clone(),
            report["partial"].clone(),
            report["uncertain"].clone()
        ),
        (json!(128), json!(2), json!(true), json!(false)),
        "a field left after two batches: those sent, exactly"
    );
    assert_eq!(report["reason"], "field");
    assert_eq!(report["found"], left["found"]);

    let lost = Error::new(Code::Transport, "Native reply timed out").uncertain();
    let report = stopped_at(2, 1, Err(lost)).await;
    assert_eq!(report["typed"], 64, "no part of a failed batch counts");
    assert_eq!(report["code"], "transport");
    assert_eq!(report["uncertain"], true);
    assert!(report.get("reason").is_none() && report.get("found").is_none());

    let busy = Error::new(Code::Busy, "held").held_by(Holder::Agent);
    assert_eq!(stopped_at(1, 0, Err(busy)).await["holder"], "agent");
    let busy = json!({"code": "busy", "holder": "person"});
    assert_eq!(stopped_at(1, 0, Ok(busy)).await["holder"], "person");

    let unknown = stopped_at(1, 0, Ok(json!({"code": "made-up"}))).await;
    assert_eq!(unknown["code"], "input");
    assert_eq!(unknown["error"], "The helper did not confirm the text");
}

#[tokio::test]
async fn typing_records_the_whole_commands_outcome_after_its_batches() {
    use crate::{activity_log, doing::Doing};

    let batches = type_batches(&"private typed text ".repeat(5)).unwrap();
    assert_eq!(batches.len(), 2);
    for (completed, performed, partial, uncertain) in [
        (0, 0, false, false),
        (0, 1, false, false),
        (0, 0, true, false),
        (1, 0, false, false),
        (1, 1, false, false),
        (1, 0, false, true),
        (2, 0, false, false),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let client = crate::test_support::client(directory.path()).doing(
            Doing::new("type")
                .app(Some("TextEdit"))
                .field(Some("Email")),
        );
        let interrupt = Interrupt::new_for_test(Duration::from_secs(10));
        let mut sent = 0;
        let report = type_recorded(&client, &batches, &interrupt, async |actions: &Value| {
            let reply = if sent < completed {
                json!({"ok": true, "performed": actions.as_array().unwrap().len(),
                    "app": {"name": "TextEdit", "bundleId": "com.apple.TextEdit"},
                    "field": {"role": "AXTextField"}})
            } else {
                json!({"ok": false, "code": "focus-changed", "reason": "field",
                    "performed": performed, "partial": partial, "uncertain": uncertain,
                    "error": "private failure message"})
            };
            client
                .record(
                    &json!({"op": "input", "actions": actions}),
                    &reply,
                    Duration::ZERO,
                )
                .await?;
            sent += 1;
            Ok(reply)
        })
        .await;

        let rows = activity_log::recent(client.output_dir().unwrap(), 5);
        assert_eq!(rows.len(), 1, "the terminal row replaces every batch");
        let row = &rows[0];
        assert_eq!(
            row.action_count,
            Some(6),
            "all requested type actions count"
        );
        assert_eq!(row.worked(), completed == 2);
        let shown = crate::overview::shown(row, 0);
        if completed > 0 {
            assert_eq!(row.bundle_id.as_deref(), Some("com.apple.TextEdit"));
        }
        if completed < 2 && !uncertain {
            let delivered = completed > 0 || performed > 0 || partial;
            assert_eq!(row.partial, Some(delivered));
            assert_eq!(shown.nothing_sent, !delivered);
            assert_eq!(row.reason.as_deref(), Some("field"));
        }
        if uncertain {
            assert!(shown.uncertain);
            assert!(!shown.nothing_sent);
        }
        if completed == 2 {
            assert_eq!(row.bundle_id.as_deref(), Some("com.apple.TextEdit"));
            assert_eq!(row.label.as_deref(), Some("Email"));
            assert_eq!(report["typed"], 95);
        } else {
            assert!(row.label.is_none(), "a refused field is never named");
        }
        let log =
            std::fs::read_to_string(client.output_dir().unwrap().join(activity_log::FILE)).unwrap();
        assert!(!log.contains("private typed text"));
        assert!(!log.contains("private failure message"));
    }
}

#[tokio::test]
async fn typed_text_names_its_field_only_when_every_batch_went_into_it() {
    use crate::{activity_log, doing::Doing};

    let batches = type_batches(&"z".repeat(95)).unwrap();
    for (second, named) in [
        (json!({"role": "AXTextField"}), true),
        (Value::Null, false),
        (json!({"role": "AXSecureTextField"}), false),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let client = crate::test_support::client(directory.path()).doing(
            Doing::new("type")
                .app(Some("TextEdit"))
                .field(Some("Email")),
        );
        let interrupt = Interrupt::new_for_test(Duration::from_secs(10));
        let mut sent = 0;
        type_recorded(&client, &batches, &interrupt, async |actions: &Value| {
            let field = if sent == 0 {
                json!({"role": "AXTextField"})
            } else {
                second.clone()
            };
            sent += 1;
            Ok(
                json!({"ok": true, "performed": actions.as_array().unwrap().len(),
                "field": field}),
            )
        })
        .await;

        let row = activity_log::recent(client.output_dir().unwrap(), 1).remove(0);
        assert_eq!(row.label.is_some(), named, "{second}");
    }
}

#[tokio::test]
async fn typing_without_a_command_context_keeps_only_its_batch_rows() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let interrupt = Interrupt::new_for_test(Duration::from_secs(10));
    let batches = type_batches(&"x".repeat(100)).unwrap();
    let report = type_recorded(&client, &batches, &interrupt, async |actions: &Value| {
        let reply = json!({"ok": true, "performed": actions.as_array().unwrap().len()});
        client
            .record(
                &json!({"op": "input", "actions": actions}),
                &reply,
                Duration::ZERO,
            )
            .await?;
        Ok(reply)
    })
    .await;
    assert_eq!(report["typed"], 100);
    let rows = crate::activity_log::recent(client.output_dir().unwrap(), 5);
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.command.is_none() && row.worked()));
}

#[tokio::test]
async fn typing_records_a_stop_between_batches_after_confirmed_text() {
    use crate::{activity_log, doing::Doing};
    use nix::sys::signal::Signal;

    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path()).doing(Doing::new("type"));
    let interrupt = Interrupt::new_for_test(Duration::from_secs(10));
    let batches = type_batches(&"x".repeat(100)).unwrap();
    let mut sent = 0;
    let report = type_recorded(&client, &batches, &interrupt, async |actions: &Value| {
        let reply = json!({"ok": true, "performed": actions.as_array().unwrap().len()});
        client
            .record(
                &json!({"op": "input", "actions": actions}),
                &reply,
                Duration::ZERO,
            )
            .await?;
        sent += 1;
        interrupt.ask_for_test(Signal::SIGTERM);
        Ok(reply)
    })
    .await;
    assert_eq!(sent, 1);
    assert_eq!(report["typed"], 64);
    let rows = activity_log::recent(client.output_dir().unwrap(), 5);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].coded(Code::Interrupted));
    assert!(rows[0].failed());
}

#[tokio::test]
async fn typing_stops_between_parts_and_says_how_much_was_typed() {
    use nix::sys::signal::Signal;

    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let interrupt = Interrupt::new_for_test(Duration::from_secs(10));
    let batches = type_batches(&"x".repeat(200)).unwrap();
    assert_eq!(batches.len(), 4);
    let mut sent = 0;

    let report = type_recorded(&client, &batches, &interrupt, async |actions: &Value| {
        sent += 1;
        interrupt.ask_for_test(Signal::SIGTERM);
        Ok(json!({"ok": true, "performed": actions.as_array().unwrap().len()}))
    })
    .await;
    assert_eq!(sent, 1, "nothing more is sent once a stop was asked");
    assert_eq!(report["ok"], false);
    assert_eq!(report["code"], "interrupted");
    assert_eq!(report["uncertain"], false);
    assert_eq!(report["typed"], 64);
    assert_eq!(report["batches"], 4);
    assert_eq!(report["confirmedBatches"], 1);

    let interrupt = Interrupt::new_for_test(Duration::from_secs(10));
    let mut first = true;
    let report = type_recorded(&client, &batches, &interrupt, async |actions: &Value| {
        if std::mem::take(&mut first) {
            Ok(json!({"ok": true, "performed": actions.as_array().unwrap().len()}))
        } else {
            Err(interrupt.cut_short(Signal::SIGTERM))
        }
    })
    .await;
    assert_eq!(report["code"], "interrupted");
    assert_eq!(report["uncertain"], true, "the part under way was cut");
    assert_eq!(report["typed"], 64, "only confirmed parts count");
}

#[tokio::test]
async fn typing_stopped_at_a_later_batch_reports_how_far_it_got_and_is_never_typed_again_whole() {
    let ask = json!({
        "account": "slack.com",
        "accountKey": {"kind": "site", "id": "slack.com", "browser": "com.google.Chrome"},
        "what": "newline",
        "action": "part",
        "bind": {"action": "part"},
        "once": true,
    });
    let stopped = json!({"ok": false, "code": "waiting-for-person",
        "error": "slack.com is signed in", "ask": ask,
        "performed": 1, "partial": false, "uncertain": false});
    let report = stopped_at(3, 1, Ok(stopped)).await;
    assert_eq!(report["typed"], 64 + 16, "the confirmed offset");
    assert_eq!(
        report["ask"]["what"], "newline",
        "the question goes with it"
    );

    let directory = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(directory.path());
    client.controller = client
        .controller
        .clone()
        .waiting(Duration::from_millis(100));
    let mut attempts = 0;
    let outcome = crate::asking::after_asking(&client, async || {
        attempts += 1;
        Ok(report.clone())
    })
    .await
    .unwrap();
    assert_eq!(attempts, 1, "what was typed is never typed again");
    assert!(
        outcome["next"]
            .as_str()
            .unwrap()
            .contains("from character 81"),
        "{outcome}"
    );
    assert_eq!(
        client
            .controller
            .asks()
            .open(atlas_core::time::now_ms())
            .len(),
        1
    );
}

#[tokio::test]
async fn a_typing_commands_row_names_the_account_its_batches_acted_in_and_its_report_never_does() {
    let directory = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(directory.path());
    let doing = crate::doing::Doing::new("type");
    client.doing = Some(doing.clone());
    let interrupt = Interrupt::new_for_test(Duration::from_secs(10));
    let batches = type_batches(&"z".repeat(64 * 2)).unwrap();
    let account = json!({"name": "slack.com", "key": {"kind": "site", "id": "slack.com"}});
    // Each batch's reply, its account already taken for its own row.
    let report = type_recorded(&client, &batches, &interrupt, async |actions: &Value| {
        doing.note_account(account.clone(), Some(json!("trusted")));
        Ok(json!({"ok": true, "performed": actions.as_array().unwrap().len()}))
    })
    .await;

    assert!(report.get("account").is_none() && report.get("allowed").is_none());
    let rows = crate::activity_log::recent(client.output_dir().unwrap(), 1);
    assert_eq!(rows[0].account.as_ref().unwrap().name, "slack.com");
    assert_eq!(rows[0].allowed, Some(crate::activity_log::Allowed::Trusted));
}
