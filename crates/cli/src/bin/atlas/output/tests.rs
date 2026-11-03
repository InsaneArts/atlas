use super::*;

#[test]
fn every_failure_for_programs_says_what_to_do_next() {
    for code in Code::ALL {
        let error = Error::new(code, "It failed");
        assert_eq!(
            failure(&error, true)["next"],
            hints::next(code, None),
            "{code}"
        );
    }

    let uncertain = Error::new(Code::Transport, "No reply").uncertain();
    assert_eq!(failure(&uncertain, true)["next"], hints::UNCERTAIN);

    let held = Error::new(Code::Busy, "Wait").held_by(Holder::Person);
    assert_eq!(
        failure(&held, true)["next"],
        hints::next(Code::Busy, Some(Holder::Person))
    );

    let waited = Error::new(Code::Busy, "Kept")
        .held_by(Holder::Agent)
        .with("waited", json!(30))
        .with("next", json!(hints::WAITED));
    assert_eq!(
        failure(&waited, true)["next"],
        hints::WAITED,
        "a wait already made is kept"
    );

    let yours = Error::new(Code::Uncertain, "Unknown")
        .with("yours", json!(true))
        .with("next", json!(hints::UNKNOWN_OWN));
    assert_eq!(
        failure(&yours, true)["next"],
        hints::UNKNOWN_OWN,
        "an unknown action of this session's own keeps its step"
    );

    let said = Error::new(Code::Elements, "Incomplete").with("next", json!("Try again."));
    assert_eq!(
        failure(&said, true)["next"],
        "Try again.",
        "its own is kept"
    );

    let mut result = json!({"ok": false, "code": "wait-failed", "uncertain": false});
    advise(&mut result);
    assert_eq!(result["next"], hints::next(Code::WaitFailed, None));

    let mut typed = json!({"ok": false, "code": "busy", "holder": "agent", "uncertain": true});
    advise(&mut typed);
    assert_eq!(typed["next"], hints::UNCERTAIN);

    // Waiting for the person keeps its own step, even when it may have
    // happened: never atlas restart.
    for (mut waiting, code, holder) in [
        (
            json!({"ok": false, "code": "stopped", "uncertain": true}),
            Code::Stopped,
            None,
        ),
        (
            json!({"ok": false, "code": "cancelled", "uncertain": true}),
            Code::Cancelled,
            None,
        ),
        (
            json!({"ok": false, "code": "busy", "holder": "person", "uncertain": true}),
            Code::Busy,
            Some(Holder::Person),
        ),
    ] {
        advise(&mut waiting);
        assert_eq!(waiting["next"], hints::next(code, holder), "{code}");
    }
    let stopped = Error::new(Code::Stopped, "Stopped").uncertain();
    assert_eq!(
        failure(&stopped, true)["next"],
        hints::next(Code::Stopped, None)
    );

    for mut untouched in [
        json!({"ok": true, "code": "elements"}),
        json!({"ok": true, "next": "atlas init"}),
        json!({"ok": false}),
    ] {
        let before = untouched.clone();
        advise(&mut untouched);
        assert_eq!(untouched, before);
    }
}

#[test]
fn a_slow_focus_read_is_tried_once_more_and_then_told_to_the_person() {
    for (unread, expected) in [
        (
            "slow",
            "Your app did not say where its focus is, and nothing was sent. Wait a moment and try once more; if it refuses again, tell the person the app is not answering instead of repeating the command.",
        ),
        (
            "failed",
            "Atlas couldn't tell what was in front, often because the active app has no window, and nothing of the command was sent. Click where your app shows, or open the app if none of it shows, then run it again.",
        ),
    ] {
        let mut result = json!({"ok": false, "code": "focus-changed",
            "reason": "unreadable", "unread": unread, "performed": 0});
        advise(&mut result);
        assert_eq!(result["next"], expected, "{unread}");
    }
}

/// A focus refusal says earlier actions were sent only when the result
/// counts some, and asks for the app first when what was in front could
/// not be read.
#[test]
fn a_focus_refusal_is_advised_by_what_was_sent_before_it() {
    let refusal = |fields: Value| {
        let mut value = json!({"ok": false, "code": "focus-changed", "uncertain": false});
        value
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        advise(&mut value);
        value["next"].clone()
    };
    let advised = |sent: bool, reason: &str| {
        hints::focus_advice(&json!({
            "code": "focus-changed",
            "reason": reason,
            "performed": u64::from(sent),
        }))
        .unwrap()
    };

    for (fields, sent, reason) in [
        (
            json!({"reason": "app", "performed": 0, "partial": false}),
            false,
            "app",
        ),
        (json!({"reason": "point"}), false, "app"),
        (
            json!({"reason": "unreadable", "performed": 0}),
            false,
            "unreadable",
        ),
        (json!({"reason": "app", "performed": 2}), true, "app"),
        (
            json!({"reason": "app", "typed": 48, "partial": true}),
            true,
            "app",
        ),
        (
            json!({"reason": "point", "typed": 0, "partial": true}),
            true,
            "app",
        ),
        (
            json!({"reason": "unreadable", "performed": 1}),
            true,
            "unreadable",
        ),
        (json!({"reason": "sheet", "performed": 0}), false, "sheet"),
        (
            json!({"reason": "field", "typed": 16, "partial": true}),
            true,
            "field",
        ),
        (json!({"reason": "window", "performed": 0}), false, "window"),
    ] {
        assert_eq!(refusal(fields.clone()), advised(sent, reason), "{fields}");
    }

    let error = Error::new(Code::FocusChanged, "Calculator has keyboard focus")
        .with("reason", json!("unreadable"));
    assert_eq!(failure(&error, true)["next"], advised(false, "unreadable"));
}

#[test]
fn a_failure_another_atlas_process_reads_has_no_next_step() {
    let error = Error::new(Code::Transport, "No reply");
    assert!(failure(&error, false).get("next").is_none());
    assert_eq!(failure(&error, false), json!(error));
}

#[test]
fn programs_get_each_result_on_one_line() {
    let value = json!({
        "ok": true,
        "text": "first line\nsecond line",
        "nested": {"list": [1, {"deep": "a\nb"}], "empty": {}},
    });
    let text = value.to_string();

    assert!(!text.contains('\n'), "{text}");
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), value);
}

#[test]
fn null_and_ordinary_success_results_keep_a_successful_exit_status() {
    for result in [
        Value::Null,
        json!({}),
        json!(true),
        json!(false),
        json!({"ok": true}),
        json!({"ready": true}),
        json!({"timedOut": false}),
        json!({"exitCode": 0}),
        json!({"ok": 0, "ready": "", "timedOut": 1}),
    ] {
        assert!(!failed(&result), "{result}");
    }
}

#[test]
fn any_explicit_failure_result_sets_a_failure_exit_status() {
    for result in [
        json!({"ok": false}),
        json!({"ready": false}),
        json!({"timedOut": true}),
        json!({"exitCode": 1}),
        json!({"exitCode": -1}),
        json!({"ok": true, "ready": false, "exitCode": 0}),
    ] {
        assert!(failed(&result), "{result}");
    }
}

#[test]
fn present_noninteger_exit_codes_retain_their_failure_status() {
    for code in [
        Value::Null,
        json!("0"),
        json!(true),
        json!([]),
        json!({}),
        json!(0.0),
    ] {
        let result = json!({"exitCode": code});
        assert!(failed(&result), "{result}");
    }
}
