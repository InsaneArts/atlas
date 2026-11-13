use super::{
    Absence, FOCUS_APP, LOGGED_OUT, Observation, diagnosis, has_capability, input_build_current,
    require_hidden, silent, validate_target_uid,
};
use crate::error::Code;
use serde_json::json;

#[test]
fn discovery_requires_an_absolute_helper_home() {
    for value in [
        json!({}),
        json!(null),
        json!(false),
        json!(12),
        json!(""),
        json!("relative"),
    ] {
        let observation = Observation::decode(&json!({"user": {"home": value}}));
        let error = observation.helper_home().unwrap_err();
        assert_eq!(error.code, "identity");
        assert_eq!(error.error, "The helper home is unknown; run atlas doctor");
    }
    let observation = Observation::decode(&json!({"user": {"home": "/Users/helper"}}));
    assert_eq!(
        observation.helper_home().unwrap(),
        std::path::Path::new("/Users/helper")
    );
}

#[test]
fn an_unqueried_permission_remains_distinct_from_denial() {
    let unknown = json!({"ok": true});
    let unavailable = json!({"ok": true, "inputService": {"ok": false}});
    let denied = json!({"ok": true, "inputService": {"ok": true, "accessibility": false}});
    let allowed = json!({"ok": true, "inputService": {"ok": true, "accessibility": true}});
    for (value, expected, projection) in [
        (unknown, None, json!(null)),
        (unavailable, None, json!(null)),
        (denied, Some(false), json!(false)),
        (allowed, Some(true), json!(true)),
    ] {
        let observation = Observation::decode(&value);
        assert_eq!(observation.native.accessibility, expected);
        let report = diagnosis(value, 502).value;
        let check = report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|check| check["name"] == "Accessibility allowed")
            .unwrap();
        assert_eq!(check["ok"], projection);
    }

    for (value, expected) in [
        (json!({}), None),
        (json!({"ok": true}), None),
        (
            json!({"ok": false, "permissions": {"screenCapture": true}}),
            None,
        ),
        (
            json!({"ok": true, "permissions": {"screenCapture": false}}),
            Some(false),
        ),
        (
            json!({"ok": true, "permissions": {"screenCapture": true}}),
            Some(true),
        ),
    ] {
        assert_eq!(Observation::decode(&value).capture, expected);
    }
}

#[test]
fn a_partial_observation_never_proves_a_hidden_session() {
    for value in [
        json!(null),
        json!(true),
        json!([]),
        json!({}),
        json!({"ok": true}),
        json!({"ok": true, "user": {"uid": 502},
                "session": {"managerName": "Aqua", "onConsole": false}}),
    ] {
        let observation = Observation::decode(&value);
        assert!(observation.require_hidden(502).is_err(), "{value}");
        assert!(!observation.input_current(502, 77), "{value}");
        assert_eq!(diagnosis(value, 502).value["ready"], false);
    }
}

#[test]
fn nonfinite_or_nonpositive_geometry_never_proves_an_input_epoch() {
    let value = json!({"ok": true, "uid": 502, "onConsole": false,
            "hasGraphicAccess": true, "securitySessionID": 42, "displayID": 7,
            "width": 100, "height": 100});
    for invalid in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        0.0,
        -1.0,
        32769.0,
    ] {
        let mut proof = super::ProofObservation::decode(&value);
        proof.width = Some(invalid);
        assert!(proof.input_epoch(77).is_err());
        proof.width = Some(100.0);
        proof.height = Some(invalid);
        assert!(proof.input_epoch(77).is_err());
        assert!(
            !super::Display {
                width: invalid,
                height: 100.0,
                scale: 1.0
            }
            .valid()
        );
    }
}

#[test]
fn a_service_lists_only_the_capabilities_it_names() {
    let native = json!({"capabilities": ["shutdown", "focus-app"]});
    assert!(has_capability(&native, "shutdown"));
    assert!(has_capability(&native, FOCUS_APP));
    assert!(!has_capability(&native, "hold"));
    assert!(!has_capability(&json!({}), "hold"));
    assert!(!has_capability(&json!({"capabilities": "hold"}), "hold"));
}

#[test]
fn only_a_helper_that_did_not_answer_is_explained() {
    assert!(silent(
        &json!({"ok": false, "code": "io", "error": "No such file"})
    ));
    assert!(silent(&json!({"ok": false, "code": "transport"})));

    for code in ["blocked", "identity", "busy"] {
        assert!(!silent(&json!({"ok": false, "code": code})), "{code}");
    }

    assert!(!silent(&json!({"ok": true})));
}

#[test]
fn a_logged_out_helper_says_how_to_log_it_in() {
    let mut report = diagnosis(json!({"ok": false, "code": "io"}), 501).value;
    Absence::LoggedOut("atlas".into()).explain(&mut report);
    assert_eq!(report["absence"], "loggedOut");
    assert_eq!(report["account"], "atlas");
    assert_eq!(report["checks"][0]["nextStep"], LOGGED_OUT);
    assert_eq!(report["nextSteps"][0], LOGGED_OUT);

    let mut gone = diagnosis(json!({"ok": false, "code": "io"}), 501).value;
    Absence::Missing.explain(&mut gone);
    assert_eq!(gone["absence"], "missing");
    assert!(gone.get("account").is_none());
}

/// A failed connection keeps its step and socket, so an error alone says
/// which connection failed.
#[test]
fn a_helper_that_cannot_be_reached_keeps_the_cause_of_the_failure() {
    let uid = (nix::unistd::getuid().as_raw() + 1).max(501);
    let cause = "Could not connect to socket /tmp/helper.sock: Connection refused";
    let error = require_hidden(&json!({"ok": false, "error": cause}), uid).unwrap_err();
    assert_eq!(
        (error.code, error.error),
        (
            Code::Transport,
            format!("The helper is not answering: {cause}. Run atlas doctor.")
        )
    );

    let error = require_hidden(&json!({"ok": false}), uid).unwrap_err();
    assert_eq!(
        error.error,
        "The helper is not answering. Run atlas doctor."
    );
}

/// A helper too busy for one more caller is busy, never "not answering",
/// so nobody is sent to restart a helper that is up.
#[test]
fn a_helper_turning_callers_away_is_busy_not_silent() {
    let status = json!({"ok": false, "code": "busy", "error": "The helper is serving too many requests; try again"});
    let uid = nix::unistd::getuid().as_raw() + 1;
    let error = require_hidden(&status, uid.max(501)).unwrap_err();
    assert_eq!(
        (error.code, error.error.as_str()),
        (
            Code::Busy,
            "The helper is serving too many requests; try again"
        )
    );
}

/// A shell that may not reach the helper is told so, never that the
/// helper stopped or that setup must run again.
#[test]
fn a_blocked_shell_is_not_a_stopped_helper() {
    let status = json!({"ok": false, "code": "blocked", "error": "This shell may not connect"});
    let uid = nix::unistd::getuid().as_raw() + 1;
    let error = require_hidden(&status, uid.max(501)).unwrap_err();
    assert_eq!(
        (error.code, error.error.as_str()),
        (Code::Blocked, "This shell may not connect")
    );

    let report = diagnosis(status, uid.max(501)).value;
    assert_eq!(report["checks"][0]["name"], "Helper reachable from here");
    assert_eq!(report["nextSteps"][0], "This shell may not connect");

    for check in report["checks"].as_array().unwrap().iter().skip(1) {
        assert_eq!(
            check["nextStep"],
            "Unknown until this shell can reach the helper."
        );
    }

    let stopped = diagnosis(
        json!({"ok": false, "code": "io", "error": "No such file"}),
        501,
    )
    .value;
    assert_eq!(stopped["checks"][0]["name"], "Helper running");
}

#[test]
fn input_pinned_to_an_earlier_same_size_display_counts_as_ready() {
    use super::{Display, Epoch, NativeObservation, Session};

    let session = Session {
        epoch: Epoch {
            uid: 503,
            capture_pid: 10,
            display: Display {
                width: 1512.0,
                height: 982.0,
                scale: 2.0,
            },
            security_session_id: 100135,
            display_id: 15,
            input_pid: 11318,
        },
        capture: true,
        input: false,
    };

    let status = |width: f64| {
        json!({"inputService": {"ok": true, "protocolVersion": 1,
            "accessibility": true, "enforcedEpoch": {"uid": 503, "securitySessionId": 100135,
            "displayId": 9, "processId": 11318, "width": width, "height": 982.0}}})
    };
    assert!(NativeObservation::decode(&status(1512.0)["inputService"]).repinnable(&session));
    assert!(
        !NativeObservation::decode(&status(1440.0)["inputService"]).repinnable(&session),
        "a new size needs a new look"
    );

    let mut other = status(1512.0);
    other["inputService"]["enforcedEpoch"]["securitySessionId"] = json!(1);
    assert!(
        !NativeObservation::decode(&other["inputService"]).repinnable(&session),
        "never across sessions"
    );
}

/// A service from a replaced file is stale even when its build matches.
#[test]
fn a_replaced_input_service_is_not_current() {
    let status = |replaced: bool| {
        json!({
            "installedBuild": "abc",
            "inputService": {"build": "abc", "replaced": replaced},
        })
    };
    assert!(input_build_current(&status(false)));
    assert!(!input_build_current(&status(true)));

    let mut older = status(false);
    older["inputService"]["build"] = json!("old");
    assert!(!input_build_current(&older));

    let mut legacy = status(false);
    legacy["inputService"]
        .as_object_mut()
        .unwrap()
        .remove("replaced");
    assert!(
        input_build_current(&legacy),
        "services that predate the field"
    );
}

#[test]
fn helper_may_be_the_first_normal_account_but_never_the_controller() {
    assert!(validate_target_uid(501, 502).is_ok());
    assert!(validate_target_uid(502, 501).is_ok());
    assert!(validate_target_uid(501, 501).is_err());
    assert!(validate_target_uid(502, 502).is_err());
    assert!(validate_target_uid(500, 502).is_err());
    assert!(validate_target_uid(0, 502).is_err());
}

#[test]
fn unsupported_report_roots_return_a_certain_error_instead_of_panicking() {
    let mut outcomes = Vec::new();
    for absence in [Absence::Missing, Absence::LoggedOut("helper".into())] {
        for value in crate::test_support::unsupported_report_roots() {
            outcomes.push(std::panic::catch_unwind(|| {
                let mut report = value;
                absence.explain(&mut report);
                report
            }));
        }
    }
    for outcome in outcomes {
        let report = outcome.expect("An invalid report shape must not panic");
        assert_eq!(
            report,
            json!({"ok": false, "code": "state", "error": "A doctor report must be an object or null", "uncertain": false})
        );
        assert!(!report.to_string().contains("private-report-input-canary"));
    }
}

#[test]
fn a_legacy_null_report_keeps_its_existing_absence_fields() {
    let mut missing = json!(null);
    Absence::Missing.explain(&mut missing);
    assert_eq!(missing, json!({"absence": "missing", "checks": null}));
    let mut logged_out = json!(null);
    Absence::LoggedOut("helper".into()).explain(&mut logged_out);
    assert_eq!(
        logged_out,
        json!({"absence": "loggedOut", "account": "helper", "checks": null})
    );
}
