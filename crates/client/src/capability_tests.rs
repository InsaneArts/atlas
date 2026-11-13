use super::{sample_report as report, *};

#[test]
fn compact_capability_advice_preserves_existing_warnings() {
    let summed = summary(report(true, json!(["The display changed."]), "passed"));
    assert_eq!(summed["warnings"][0], "The display changed.");
    assert_eq!(summed["warnings"].as_array().unwrap().len(), 2);
}

#[test]
fn a_doctor_that_is_not_ready_keeps_every_check_for_programs() {
    let summed = summary(report(false, json!(["The display changed."]), "failed"));

    assert_eq!(summed["ok"], true);
    assert_eq!(summed["ready"], false);
    assert_eq!(summed["helper"], json!({"name": "atlas", "uid": 502}));
    assert_eq!(summed["display"], json!({"width": 1920, "height": 1080}));
    assert_eq!(summed["checks"], json!([{"kind": "input", "ok": false}]));
    assert_eq!(summed["nextSteps"], json!(["Run atlas start-input."]));
    assert_eq!(
        summed["warnings"],
        json!(["The display changed.", compact_warning(NativeState::Failed)])
    );
    assert_eq!(summed["capabilities"]["nativeDialogs"]["state"], "failed");
    assert_eq!(summed["absence"], "loggedOut");
    assert_eq!(summed["account"], "atlas");
    assert!(summed.get("session").is_none());
}

#[test]
fn a_failed_native_check_keeps_its_exact_warning() {
    let report = with_capabilities(
        json!({"warnings": ["existing"]}),
        NativeState::Failed,
        json!({"state": "passed"}),
    );
    assert_eq!(
        report["warnings"],
        json!(["existing", format!("{NATIVE_DIALOGS_FAILED} {WEB_CONTENT}")])
    );
    assert_eq!(report["capabilities"]["nativeDialogs"]["state"], "passed");
}

#[test]
fn a_summary_uses_the_readiness_fact_instead_of_the_projection() {
    assert_eq!(
        summarize(json!({"ready": false, "warnings": ["kept"]}), true),
        json!({"ok": true, "ready": true, "warnings": ["kept"]})
    );
}

#[test]
fn unsupported_report_roots_return_a_certain_error_instead_of_panicking() {
    let outcomes: Vec<_> = atlas_core::test_support::unsupported_report_roots()
        .into_iter()
        .map(|value| std::panic::catch_unwind(|| summary(value)))
        .collect();
    for outcome in outcomes {
        let reply = outcome.expect("An invalid report shape must not panic");
        assert_eq!(
            reply,
            json!({"ok": false, "code": "state", "error": "A doctor report must be an object or null", "uncertain": false})
        );
        assert!(!reply.to_string().contains("private-report-input-canary"));
    }
}

#[test]
fn a_legacy_null_report_keeps_its_existing_summary_fields() {
    assert_eq!(
        summary(json!(null)),
        json!({"display": null, "helper": {"name": null, "uid": null}})
    );
}
