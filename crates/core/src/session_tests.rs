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
        assert!(proof.input_epoch(77, super::epoch::Console::Off).is_err());
        proof.width = Some(100.0);
        proof.height = Some(invalid);
        assert!(proof.input_epoch(77, super::epoch::Console::Off).is_err());
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

/// A status of the machine desktop paired as `paired()`: its own screen,
/// machine mode, input ready.
fn machine_status() -> serde_json::Value {
    let proof = json!({"ok": true, "uid": 501, "hasGraphicAccess": true, "parentPID": 123,
        "onConsole": true, "securitySessionID": 42, "displayID": 7, "width": 1440,
        "height": 900});
    json!({
        "ok": true,
        "user": {"uid": 501},
        "daemon": {"pid": 123},
        "machine": {"mode": "machine", "boot": "boot-1"},
        "session": {"managerName": "Aqua", "onConsole": true},
        "display": {"width": 1440, "height": 900, "scale": 2},
        "permissions": {"screenCapture": true, "accessibility": true},
        "sessionProof": proof,
        "inputService": {"ok": true, "protocolVersion": 1, "pid": 456, "ready": true,
            "accessibility": true, "session": proof},
    })
}

/// The station's own key in these tests.
fn station_key() -> super::key::PeerKey {
    super::key::PeerKey::from_bytes([2; 32])
}

fn paired() -> super::paired::Paired {
    super::paired::Paired {
        station: station_key().fingerprint(),
        guest: super::key::PeerKey::from_bytes([1; 32]),
        uid: 501,
        boot: "boot-0".to_owned(),
        change: crate::files::Token::new(),
    }
}

fn expect(paired: &super::paired::Paired) -> super::MachineExpect<'_> {
    super::MachineExpect {
        paired,
        peer: paired.guest,
        station: station_key(),
        eligible: true,
        hands_off: Some(true),
    }
}

#[test]
fn a_paired_machine_on_its_own_screen_is_proved_with_its_whole_epoch() {
    let paired = paired();

    let session = Observation::decode(&machine_status())
        .require_machine(&expect(&paired))
        .unwrap();

    assert!(session.capture);
    assert!(
        session.input,
        "its input service is ready on its own screen"
    );
    assert_eq!(session.epoch.uid, 501);
    assert_eq!(session.epoch.boot, "boot-1");
    assert_eq!(session.epoch.display.scale, 2.0);
    assert_eq!(
        (session.epoch.capture_pid, session.epoch.input_pid),
        (123, 456)
    );
}

#[test]
fn a_machine_may_share_the_stations_uid_where_a_second_login_may_not() {
    let paired = paired();

    assert!(
        Observation::decode(&machine_status())
            .require_machine(&expect(&paired))
            .is_ok()
    );
    assert!(validate_target_uid(501, 501).is_err());
}

/// A change made to a machine's status.
type StatusChange = fn(&mut serde_json::Value);
/// A change made to what a machine is held to, given another key.
type ExpectChange = fn(&mut super::MachineExpect<'_>, super::key::PeerKey);

#[test]
fn each_fact_a_machine_is_held_to_refuses_when_it_changes() {
    let paired = paired();
    let other = super::key::PeerKey::from_bytes([3; 32]);
    let status_changes: [(&str, StatusChange); 10] = [
        ("another account", |status| {
            status["user"]["uid"] = json!(502)
        }),
        ("not machine mode", |status| {
            status["machine"]["mode"] = json!("user")
        }),
        ("no boot", |status| status["machine"]["boot"] = json!(null)),
        ("off its screen", |status| {
            status["session"]["onConsole"] = json!(false)
        }),
        ("proof off its screen", |status| {
            status["sessionProof"]["onConsole"] = json!(false);
        }),
        ("not Aqua", |status| {
            status["session"]["managerName"] = json!("Background")
        }),
        ("another proof parent", |status| {
            status["sessionProof"]["parentPID"] = json!(9)
        }),
        ("another proof size", |status| {
            status["sessionProof"]["width"] = json!(800)
        }),
        ("no capture process", |status| {
            status["daemon"]["pid"] = json!(0)
        }),
        ("no answer", |status| status["ok"] = json!(false)),
    ];
    for (what, change) in status_changes {
        let mut status = machine_status();
        change(&mut status);
        assert!(
            Observation::decode(&status)
                .require_machine(&expect(&paired))
                .is_err(),
            "{what}"
        );
    }

    let expect_changes: [(&str, ExpectChange); 5] = [
        ("another key", |expect, other| expect.peer = other),
        ("the station's own key", |expect, _| {
            expect.peer = expect.station
        }),
        ("not eligible", |expect, _| expect.eligible = false),
        ("hands unknown", |expect, _| expect.hands_off = None),
        ("hands on", |expect, _| expect.hands_off = Some(false)),
    ];
    for (what, change) in expect_changes {
        let mut held = expect(&paired);
        change(&mut held, other);
        assert!(
            Observation::decode(&machine_status())
                .require_machine(&held)
                .is_err(),
            "{what}"
        );
    }
    let mut low = paired.clone();
    low.uid = 500;
    let mut status = machine_status();
    status["user"]["uid"] = json!(500);
    status["sessionProof"]["uid"] = json!(500);
    status["inputService"]["session"]["uid"] = json!(500);
    assert!(
        Observation::decode(&status)
            .require_machine(&expect(&low))
            .is_err()
    );
}

#[test]
fn hands_that_may_be_on_the_machine_refuse_as_the_screen_in_use() {
    let paired = paired();
    let mut held = expect(&paired);
    held.hands_off = None;

    let refused = Observation::decode(&machine_status())
        .require_machine(&held)
        .unwrap_err();

    assert_eq!(refused.code, Code::OnConsole);
}

#[test]
fn a_change_of_any_fact_of_a_machines_epoch_is_another_epoch() {
    let paired = paired();
    let epoch_of = |status: &serde_json::Value| {
        Observation::decode(status)
            .require_machine(&expect(&paired))
            .unwrap()
            .epoch
    };
    let first = epoch_of(&machine_status());
    let changes: [(&str, StatusChange); 7] = [
        ("boot", |status| status["machine"]["boot"] = json!("boot-2")),
        ("scale", |status| status["display"]["scale"] = json!(1)),
        ("capture process", |status| {
            status["daemon"]["pid"] = json!(124);
            status["sessionProof"]["parentPID"] = json!(124);
        }),
        ("security session", |status| {
            status["sessionProof"]["securitySessionID"] = json!(43);
            status["inputService"]["session"]["securitySessionID"] = json!(43);
        }),
        ("display", |status| {
            status["sessionProof"]["displayID"] = json!(8);
            status["inputService"]["session"]["displayID"] = json!(8);
        }),
        ("input process", |status| {
            status["inputService"]["pid"] = json!(457)
        }),
        ("size", |status| {
            status["display"]["width"] = json!(1280);
            status["sessionProof"]["width"] = json!(1280);
            status["inputService"]["session"]["width"] = json!(1280);
        }),
    ];
    for (what, change) in changes {
        let mut status = machine_status();
        change(&mut status);

        assert_ne!(epoch_of(&status), first, "{what}");
    }
}

#[test]
fn a_second_logins_proof_never_takes_a_machine_on_its_own_screen() {
    let mut status = machine_status();
    status["user"]["uid"] = json!(7001);
    status["sessionProof"]["uid"] = json!(7001);

    let refused = Observation::decode(&status)
        .require_hidden(7001)
        .unwrap_err();

    assert_eq!(refused.code, Code::OnConsole);
}

#[test]
fn a_machines_input_is_not_ready_when_its_input_service_proves_another_screen_or_epoch() {
    let paired = paired();
    let changes: [(&str, StatusChange); 4] = [
        ("off its screen", |status| {
            status["inputService"]["session"]["onConsole"] = json!(false);
        }),
        ("another security session", |status| {
            status["inputService"]["session"]["securitySessionID"] = json!(43);
        }),
        ("another display", |status| {
            status["inputService"]["session"]["displayID"] = json!(8);
        }),
        ("another size", |status| {
            status["inputService"]["session"]["width"] = json!(800)
        }),
    ];
    for (what, change) in changes {
        let mut status = machine_status();
        change(&mut status);

        let session = Observation::decode(&status)
            .require_machine(&expect(&paired))
            .unwrap();

        assert!(!session.input, "{what}");
    }
}

#[test]
fn a_pairing_that_names_the_stations_own_key_is_refused() {
    let mut paired = paired();
    let station = station_key();
    paired.guest = station;
    let held = super::MachineExpect {
        paired: &paired,
        peer: station,
        station,
        eligible: true,
        hands_off: Some(true),
    };

    assert!(
        Observation::decode(&machine_status())
            .require_machine(&held)
            .is_err()
    );
}

#[test]
fn a_machine_paired_with_another_station_is_refused() {
    let mut paired = paired();
    paired.station = super::key::PeerKey::from_bytes([9; 32]).fingerprint();

    assert!(
        Observation::decode(&machine_status())
            .require_machine(&expect(&paired))
            .is_err()
    );
}
