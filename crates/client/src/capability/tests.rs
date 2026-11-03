use super::{sample_report as report, *};

fn assess(record: Option<&Record>, current: Option<&Generation>, at: u64) -> Value {
    let evidence = record.cloned().map_or(Evidence::Missing, Evidence::Present);
    assessment(&evidence, current, at).1
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
        let diagnosis = atlas_core::session::diagnosis(status, 502);
        assert!(!diagnosis.ready);
        assert!(diagnosis.observation.require_hidden(502).is_err());
        assert!(!diagnosis.value["capabilities"]["input"].as_bool().unwrap());
    }
    let (state, report) = assessment(&Evidence::Missing, None, 100);
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

/// Doctor's view of the record file at `path`, in the generation and at the
/// time `record` describes.
fn assess_file(path: Result<PathBuf>) -> (NativeState, Value) {
    assessment(&read_record(path), Some(&generation()), 160)
}

#[test]
fn unreadable_native_evidence_is_distinguished_from_a_missing_check() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("native-502.json");
    let (state, report) = assess_file(Ok(missing.clone()));
    assert_eq!(state, NativeState::Unknown);
    assert_eq!(
        report["reason"],
        "Native Save/Open has no recorded check. Panels can be inert without authenticated Screen Sharing; use atlas run or atlas open instead."
    );

    let present = dir.path().join("present.json");
    fs::write(
        &present,
        serde_json::to_vec(&record(true, true, 100)).unwrap(),
    )
    .unwrap();
    assert_eq!(assess_file(Ok(present.clone())).0, NativeState::Passed);

    let unread =
        "Native Save/Open's recorded check could not be read; use atlas run or atlas open instead.";
    let malformed = "Native Save/Open's recorded check is malformed and was ignored; use atlas run or atlas open instead.";

    let mut cases = vec![
        ("location", Err(problem("no home")), unread),
        ("directory", Ok(dir.path().to_path_buf()), unread),
    ];

    let locked = dir.path().join("locked.json");
    fs::copy(&present, &locked).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(&locked).is_err() {
        cases.push(("permission", Ok(locked), unread));
    }

    let garbled = dir.path().join("garbled.json");
    fs::write(&garbled, b"{\"observedAt\": ").unwrap();
    cases.push(("garbled", Ok(garbled), malformed));
    let unknown = dir.path().join("unknown.json");
    fs::write(&unknown, br#"{"observedAt": 100, "passed": true}"#).unwrap();
    cases.push(("wrong shape", Ok(unknown), malformed));

    for (name, path, reason) in cases {
        let (state, report) = assess_file(path);
        assert_eq!(state, NativeState::Unknown, "{name}");
        assert_eq!(
            report,
            json!({"state": "unknown", "reason": reason}),
            "{name}"
        );
        assert!(
            !report.to_string().contains(&*dir.path().to_string_lossy()),
            "{name}: no path is published"
        );

        let doctor = with_capabilities(json!({"ready": true, "warnings": []}), state, report);
        assert_eq!(doctor["ready"], true, "{name}: readiness is unchanged");
        assert_eq!(doctor["capabilities"]["nativeDialogs"]["state"], "unknown");
    }
}
