use super::*;

fn assess(record: Option<&Record>, current: Option<&Generation>, at: u64) -> Value {
    assessment(record, current, at).1
}

fn generation() -> Generation {
    Generation {
        capture_pid: 1,
        input_pid: 2,
        security_session_id: 3,
        display_id: 4,
        display: Display {
            width: 2560.0,
            height: 1440.0,
            scale: 1.0,
        },
        boot: 5,
    }
}

fn check(kind: &str, passed: bool) -> Check {
    Check {
        kind: kind.into(),
        passed,
        stage: (!passed).then(|| format!("{kind} confirmation enabled")),
        error: None,
    }
}

fn record(save: bool, open: bool, observed_at: u64) -> Record {
    Record {
        observed_at,
        passed: save && open,
        checks: vec![check("save", save), check("open", open)],
        generation: generation(),
        evidence: Vec::new(),
    }
}

#[test]
fn observations_count_only_for_the_same_fresh_generation() {
    let current = generation();
    let state = |record: Option<&Record>, current: Option<&Generation>, at| {
        assess(record, current, at)["state"].clone()
    };

    assert_eq!(state(None, Some(&current), 100), "unknown");

    let passed = record(true, true, 100);
    assert_eq!(state(Some(&passed), Some(&current), 160), "passed");
    let failed = record(true, false, 100);
    assert_eq!(state(Some(&failed), Some(&current), 160), "failed");
    assert_eq!(state(Some(&passed), Some(&current), 100 + 31 * 60), "stale");

    let mut resized = generation();
    resized.display.width = 1920.0;
    assert_eq!(state(Some(&passed), Some(&resized), 160), "stale");

    let mut restarted = generation();
    restarted.input_pid = 9;
    assert_eq!(state(Some(&passed), Some(&restarted), 160), "stale");
    assert_eq!(state(Some(&passed), None, 160), "stale");
}

#[test]
fn inconsistent_or_future_records_are_not_trusted() {
    let current = generation();
    let state = |record: &Record| assess(Some(record), Some(&current), 1000)["state"].clone();

    let mut save_only = record(true, true, 900);
    save_only.checks.pop();
    assert_eq!(state(&save_only), "unknown");

    let mut duplicated = record(true, true, 900);
    duplicated.checks[1] = check("save", true);
    assert_eq!(state(&duplicated), "unknown");

    let mut claimed = record(true, false, 900);
    claimed.passed = true;
    assert_eq!(state(&claimed), "unknown");

    let mut launch = record(false, false, 900);
    launch.checks = vec![check("launch", false)];
    assert_eq!(state(&launch), "unknown");

    assert_eq!(state(&record(true, true, 1000 + SKEW + 1)), "unknown");
    assert_eq!(state(&record(true, true, 1000 + SKEW)), "passed");
}

fn report(ready: bool, warnings: Value, native: &str) -> Value {
    with_capabilities(
        json!({
            "ok": true,
            "ready": ready,
            "canObserve": true,
            "capabilities": {
                "observe": true,
                "input": ready,
            },
            "checks": [{"kind": "input", "ok": ready}],
            "nextSteps": if ready { json!([]) } else { json!(["Run atlas start-input."]) },
            "warnings": warnings,
            "absence": "loggedOut",
            "account": "atlas",
            "session": {
                "user": {"name": "atlas", "uid": 502},
                "display": {"width": 1920, "height": 1080},
            },
        }),
        match native {
            "passed" => NativeState::Passed,
            "failed" => NativeState::Failed,
            "stale" => NativeState::Stale,
            _ => NativeState::Unknown,
        },
        json!({"state": native}),
    )
}

#[test]
fn a_doctor_warns_about_panels_and_webkit_once_in_each_format() {
    for (state, native) in [
        (
            "unknown",
            "Native Save/Open is unqualified; use atlas run or atlas open.",
        ),
        (
            "stale",
            "Native Save/Open's check is stale; use atlas run or atlas open.",
        ),
        ("failed", NATIVE_DIALOGS_FAILED),
        (
            "passed",
            "Native Save/Open passed a dated check, not a connection check.",
        ),
    ] {
        for ready in [false, true] {
            let full = report(ready, json!([]), state);
            let warning = format!("{native} {WEB_CONTENT}");
            assert_eq!(full["warnings"], json!([warning]));
            for verbose in [false, true] {
                let rendered =
                    crate::ui::render::render(&crate::ui::render::View::Doctor { verbose }, &full);
                assert_eq!(
                    rendered.out.matches(&warning).count(),
                    1,
                    "{}",
                    rendered.out
                );
                assert_eq!(rendered.out.matches(native).count(), 1, "{}", rendered.out);
            }
            let summed = summary(full);
            assert_eq!(summed["warnings"], json!([warning]));
            if ready {
                assert_eq!(
                    summed,
                    json!({"ok":true, "ready":true, "warnings":[warning]})
                );
                assert!(summed.to_string().len() < 384);
            }
        }
    }
}

#[test]
fn compact_capability_advice_preserves_existing_warnings() {
    let summed = summary(report(true, json!(["The display changed."]), "passed"));
    assert_eq!(summed["warnings"][0], "The display changed.");
    assert_eq!(summed["warnings"].as_array().unwrap().len(), 2);
}

#[test]
fn a_pass_reports_its_date_without_claiming_a_connected_session() {
    let passed = record(true, true, 100);
    let assessment = assess(Some(&passed), Some(&generation()), 160);
    assert_eq!(assessment["observedAt"], 100);
    assert_eq!(assessment["ageSeconds"], 60);
    assert!(
        assessment["reason"]
            .as_str()
            .unwrap()
            .contains("not a connection check")
    );
    assert_eq!(
        assess(Some(&passed), Some(&generation()), u64::MAX)["state"],
        "stale"
    );
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
fn missing_proof_never_upgrades_capability() {
    for status in [
        Value::Null,
        json!({}),
        json!({"ok": true}),
        json!({
            "ok": true, "user": {"uid": 502},
            "session": {"managerName": "Aqua", "onConsole": false},
            "inputService": {"ok": true, "ready": true, "accessibility": true}
        }),
    ] {
        let diagnosis = crate::session::diagnosis(status, 502);
        assert!(!diagnosis.ready);
        assert!(diagnosis.observation.require_hidden(502).is_err());
        assert!(!diagnosis.value["capabilities"]["input"].as_bool().unwrap());
    }
    let (state, report) = assessment(None, None, 100);
    assert_eq!(state, NativeState::Unknown);
    assert_eq!(
        report,
        json!({
            "state": "unknown",
            "reason": "Native Save/Open has no recorded check. Panels can be inert without authenticated Screen Sharing; use atlas run or atlas open instead."
        })
    );
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
    let outcomes: Vec<_> = crate::test_support::unsupported_report_roots()
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
