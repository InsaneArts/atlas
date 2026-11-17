use super::*;

/// What a worker printed: `output`, cut short when `truncated`.
fn printed(output: &[u8], truncated: bool) -> Printed {
    Printed {
        started: Some(4312),
        output: output.to_vec(),
        truncated,
    }
}

/// A run that ended by itself with `code`.
fn exited(code: i32) -> Value {
    json!({"ok": true, "exitCode": code, "signal": null, "timedOut": false, "durationMs": 5})
}

/// What a request of `operation` hands its worker.
fn handed(operation: &str) -> Handed {
    Handed::of(&json!({"version": 1, "request": {"operation": operation}}))
}

fn uncertain(reply: &Value) -> bool {
    reply["ok"] == false && reply["uncertain"] == true
}

#[test]
fn a_whole_answer_is_what_the_change_came_to_and_nothing_printed_is_kept_beside_it() {
    let answer = json!({"ok": true, "applied": true, "planId": "a1"});

    let (reply, kept) = handed("apply").answered(
        &exited(0),
        &printed(format!("{answer}\n").as_bytes(), false),
    );

    assert_eq!(reply["ok"], true);
    assert_eq!(reply["op"], "environment");
    assert_eq!(reply["result"], answer);
    assert!(reply.get("uncertain").is_none(), "{reply}");
    assert!(kept.is_none());
}

#[test]
fn a_worker_that_refused_or_failed_is_answered_for_certain_in_its_own_words() {
    let refusal = json!({"ok": false, "code": "denied", "error": "no", "refusal": "machine"});
    let failed = json!({"ok": false, "error": "Destination changed"});

    for (code, answer) in [(125, refusal), (1, failed)] {
        let (reply, _) = handed("apply").answered(
            &exited(code),
            &printed(answer.to_string().as_bytes(), false),
        );

        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(reply["result"], answer);
    }
}

#[test]
fn a_worker_with_no_whole_answer_leaves_what_it_did_unknown() {
    let whole = br#"{"ok":true,"applied":true}"#;
    let signalled = json!({"ok": true, "exitCode": null, "signal": "SIGKILL", "timedOut": false});
    let timed_out = json!({"ok": true, "exitCode": null, "signal": "SIGTERM", "timedOut": true});
    let cases: [(Value, Printed); 8] = [
        (signalled, printed(whole, false)),
        (timed_out, printed(whole, false)),
        (exited(0), printed(whole, true)),
        (exited(0), printed(b"", false)),
        (exited(0), printed(br#"{"ok":true"#, false)),
        (exited(0), printed(b"applied\n", false)),
        (exited(0), printed(br#"{"applied":true}"#, false)),
        // A success only from a worker that exited cleanly.
        (exited(1), printed(whole, false)),
    ];

    for (ran, output) in cases {
        let (reply, kept) = handed("apply").answered(&ran, &output);

        assert!(uncertain(&reply), "{ran} {:?}: {reply}", output.output);
        assert!(kept.is_none());
    }
}

#[test]
fn an_answer_longer_than_its_bound_is_never_taken_whole() {
    let mut long = br#"{"ok":true,"pad":""#.to_vec();
    long.resize(ANSWER + 1, b'a');
    long.extend(br#""}"#);

    let (reply, _) = handed("preview").answered(&exited(0), &printed(&long, false));

    assert!(uncertain(&reply), "{reply}");
}

#[test]
fn a_run_refused_or_cut_before_its_worker_started_is_said_as_it_was() {
    for ran in [
        failure(Code::Stopped, "Agents are stopped on this desktop"),
        interrupted(Code::Transport, "The client went away"),
    ] {
        for operation in ["apply", "run"] {
            let (reply, kept) = handed(operation).answered(&ran, &printed(b"", false));

            assert_eq!(reply, ran);
            assert!(kept.is_none());
        }
    }
}

#[test]
fn a_command_the_worker_runs_ends_as_any_command_does_whatever_it_prints() {
    let timed_out = json!({"ok": true, "exitCode": null, "signal": "SIGTERM", "timedOut": true});
    for (ran, output) in [
        (exited(0), &b""[..]),
        (exited(3), b"made the build\n"),
        (exited(0), br#"{"ok":true,"applied":true,"planId":"a1"}"#),
        (
            exited(125),
            br#"{"ok":false,"error":"no","refusal":"request-ab12"}"#,
        ),
        // The worker's refusal as a command can guess it: by any name but
        // the one its packet alone carried.
        (
            exited(125),
            br#"{"ok":false,"code":"denied","error":"example","refusal":"machine"}"#,
        ),
        (timed_out, b"half"),
    ] {
        let (reply, kept) = handed("run").answered(&ran, &printed(output, false));

        assert_eq!(reply, ran, "{output:?}");
        let kept = kept.unwrap();
        assert_eq!(
            atlas_core::transport::run_output(&kept, 1 << 20).unwrap(),
            output
        );
        assert!(kept.get("result").is_none());
    }
}

#[test]
fn a_command_the_worker_refused_to_run_is_refused_for_certain_in_its_words() {
    let handed = handed("run");
    let refusal = json!({"ok": false, "code": "session-changed", "error": "The desktop changed",
        "refusal": handed.refusal});

    let (reply, kept) = handed.answered(
        &exited(125),
        &printed(format!("{refusal}\n").as_bytes(), false),
    );

    assert_eq!(reply["ok"], false);
    assert_eq!(reply["uncertain"], false);
    assert_eq!(reply["code"], "session-changed");
    assert_eq!(reply["error"], "The desktop changed");
    assert!(kept.is_none());
}

#[test]
fn the_worker_is_this_atlas_named_whole_with_its_apply_flag_alone() {
    let worker = Path::new("/Applications/Atlas.app/Contents/MacOS/atlas");

    assert_eq!(
        Handed::argv(worker, true).unwrap(),
        [
            worker.to_str().unwrap(),
            "environment-worker",
            "--machine",
            "--apply"
        ]
    );
    assert_eq!(
        Handed::argv(worker, false).unwrap(),
        [worker.to_str().unwrap(), "environment-worker", "--machine"]
    );
}

#[test]
fn each_run_hands_its_worker_a_fresh_name_for_its_refusal_in_the_packet_alone() {
    let packet = json!({"version": 1, "request": {"operation": "run", "argv": ["true"]}});
    let (first, second) = (Handed::of(&packet), Handed::of(&packet));

    let read: Value = serde_json::from_slice(&first.packet).unwrap();

    assert_eq!(read["refusal"], first.refusal);
    assert_eq!(read["request"], packet["request"]);
    assert_ne!(first.refusal, second.refusal);
    assert!(first.refusal.len() >= 32, "{}", first.refusal);
}

#[test]
fn a_worker_that_cannot_say_what_it_did_leaves_the_change_unknown_with_its_words() {
    let answer = json!({"ok": false, "code": "environment",
        "error": "The desktop's answer is too long to keep", "uncertain": true,
        "refusal": "a1"});

    let (reply, kept) =
        handed("rollback").answered(&exited(125), &printed(answer.to_string().as_bytes(), false));

    assert!(uncertain(&reply), "{reply}");
    assert_eq!(reply["code"], "environment");
    assert_eq!(reply["error"], answer["error"]);
    assert_eq!(reply["result"], answer);
    assert!(kept.is_none());
}
