use super::*;

#[test]
fn reply_observations_keep_known_phases_and_leave_other_values_unknown() {
    for (phase, expected) in [
        (json!("applying"), Some(Phase::Applying)),
        (json!("applied"), Some(Phase::Applied)),
        (json!("rollingBack"), Some(Phase::RollingBack)),
        (json!("rolledBack"), Some(Phase::RolledBack)),
        (json!("future"), None),
        (Value::Null, None),
        (json!(false), None),
        (json!(7), None),
        (json!([]), None),
        (json!({}), None),
        (json!({"applying": null}), None),
        (json!({"applied": null}), None),
        (json!({"rollingBack": null}), None),
        (json!({"rolledBack": null}), None),
    ] {
        assert_eq!(WorkerReply::read(&json!({"phase": phase})).phase, expected);
    }
    assert_eq!(WorkerReply::read(&json!({})).phase, None);
}

#[test]
fn reply_observations_preserve_explicit_proof_and_error_defaults() {
    let fallback = "The helper did not finish undoing this copy";
    for (value, error) in [
        (Value::Null, fallback),
        (json!("false"), "false"),
        (json!(7), fallback),
        (json!([]), fallback),
        (json!({}), fallback),
    ] {
        let reply = WorkerReply::read(&json!({
            "ok": value, "resumeRequired": value, "error": value
        }));
        assert_eq!(reply.ok, None);
        assert_eq!(reply.resume_required, None);
        assert_eq!(reply.error, error);
    }
    for value in [false, true] {
        let reply = WorkerReply::read(&json!({
            "ok": value, "resumeRequired": value, "error": worker::MISSING
        }));
        assert_eq!(reply.ok, Some(value));
        assert_eq!(reply.resume_required, Some(value));
        assert_eq!(reply.error, worker::MISSING);
    }
}
