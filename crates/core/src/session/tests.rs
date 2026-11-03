use super::logins::{
    Process, blockers, desktop_up_in, gone_twice, logged_out_in, parse_processes, record_with,
    running_for, stale_in,
};
use super::{
    Absence, Blocker, FOCUS_APP, Kind, LOGGED_OUT, Observation, Record, Window, classify,
    diagnosis, has_capability, input_build_current, require_hidden, silent, validate_target_uid,
};
use crate::error::Code;
use serde_json::json;
use std::time::Duration;

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

fn listed_processes(listing: &str) -> Vec<Process> {
    parse_processes(listing).expect("valid process list")
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
fn the_desktop_is_up_once_setup_assistant_is_gone_and_the_dock_and_finder_run() {
    let core = "/System/Library/CoreServices";
    let listing = |extra: &str| {
        listed_processes(&format!(
            "502 10 00:10 {core}/Dock.app/Contents/MacOS/Dock\n\
                 502 11 00:10 {core}/Finder.app/Contents/MacOS/Finder\n\
                 502 12 00:10 {core}/KeyboardSetupAssistant.app/Contents/MacOS/KeyboardSetupAssistant\n\
                 {extra}"
        ))
    };
    let assistant = |uid: u32| {
        format!("{uid} 13 00:10 {core}/Setup Assistant.app/Contents/MacOS/Setup Assistant\n")
    };
    assert!(desktop_up_in(&listing(""), 502));
    assert!(!desktop_up_in(&listing(&assistant(502)), 502));
    assert!(
        desktop_up_in(&listing(&assistant(503)), 502),
        "another account's Setup Assistant"
    );
    assert!(!desktop_up_in(&listing(""), 503), "no Dock of its own");
}

#[test]
fn processes_are_read_from_ps_lines() {
    let listing = "  505  1158 04-00:04:57 /System/Library/CoreServices/loginwindow.app/Contents/MacOS/loginwindow\n\
                       501   700       05:32 /Applications/Some App.app/Contents/MacOS/Some App\n\
                       -2    40       00:10 /usr/sbin/system-service\n";
    let listed = listed_processes(listing);
    assert_eq!(listed.len(), 2);
    assert_eq!((listed[0].uid, listed[0].pid), (505, 1158));
    assert_eq!(
        listed[0].running,
        std::time::Duration::from_secs(4 * 86_400 + 4 * 60 + 57)
    );
    assert_eq!(
        listed[1].command, "/Applications/Some App.app/Contents/MacOS/Some App",
        "a command keeps its spaces"
    );
}

#[test]
fn process_lists_ignore_negative_ids_but_refuse_unreadable_rows() {
    let listing = "-2 40 00:10 /usr/sbin/system-service\n\
                       1 41 00:10 /usr/sbin/daemon\n\
                       99 42 00:10 /usr/sbin/unknown\n\
                       502 43 00:10 /System/Library/CoreServices/Dock.app/Contents/MacOS/Dock\n";
    let listed = parse_processes(listing).unwrap();
    assert_eq!(
        listed.iter().map(|process| process.uid).collect::<Vec<_>>(),
        [1, 99, 502]
    );
    for line in [
        "garbage",
        "NaN 44 00:10 /Dock",
        "4294967296 44 00:10 /Dock",
        "502 bad 00:10 /Dock",
        "502 44 bad /Dock",
        "502 44 00:10",
    ] {
        let malformed = format!("{listing}{line}\n");
        assert!(parse_processes(&malformed).is_none(), "{line}");
        assert!(!gone_twice(&[502], || parse_processes(&malformed), || {}));
    }
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

#[test]
fn elapsed_times_read_like_ps_writes_them() {
    use std::time::Duration;

    assert_eq!(running_for("05:09"), Some(Duration::from_secs(309)));
    assert_eq!(running_for("12:05:09"), Some(Duration::from_secs(43_509)));
    assert_eq!(
        running_for("2-00:00:01"),
        Some(Duration::from_secs(172_801))
    );
    assert_eq!(running_for("soon"), None);
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

fn window(uid: u32, seconds: u64) -> Window {
    Window {
        uid,
        pid: uid + 1000,
        age: Duration::from_secs(seconds),
    }
}

fn account(seconds: Option<u64>) -> Record {
    Record {
        passwd: true,
        directory: true,
        age: seconds.map(Duration::from_secs),
    }
}

fn kinds(classified: &[(Window, Kind)]) -> Vec<(u32, Kind)> {
    classified
        .iter()
        .map(|(window, kind)| (window.uid, *kind))
        .collect()
}

#[test]
fn a_login_whose_account_no_directory_has_is_an_orphan() {
    let windows = [window(0, 900), window(501, 900), window(502, 900)];
    let classified = classify(
        &windows,
        |uid| {
            if uid == 502 {
                Record::default()
            } else {
                account(Some(5000))
            }
        },
        501,
        Some(503),
        Some(501),
    );
    assert_eq!(
        kinds(&classified),
        [(0, Kind::Picker), (501, Kind::Mine), (502, Kind::Orphan)]
    );
}

#[test]
fn a_reused_user_id_is_a_leftover_only_while_its_login_is_older_than_the_account() {
    let older = classify(
        &[window(503, 900)],
        |_| account(Some(60)),
        501,
        Some(503),
        Some(501),
    );
    assert_eq!(kinds(&older), [(503, Kind::HelperLeftover)]);

    let newer = classify(
        &[window(503, 30)],
        |_| account(Some(60)),
        501,
        Some(503),
        Some(501),
    );
    assert_eq!(kinds(&newer), [(503, Kind::Helper)]);

    let rounding = classify(
        &[window(503, 65)],
        |_| account(Some(60)),
        501,
        Some(503),
        None,
    );
    assert_eq!(kinds(&rounding), [(503, Kind::Helper)], "within the slack");

    let unknown = classify(&[window(503, 900)], |_| account(None), 501, Some(503), None);
    assert_eq!(
        kinds(&unknown),
        [(503, Kind::Helper)],
        "no age, no leftover"
    );
}

#[test]
fn the_login_window_itself_is_the_picker() {
    let classified = classify(
        &[window(0, 10), window(88, 10)],
        |_| Record::default(),
        501,
        None,
        None,
    );
    assert_eq!(kinds(&classified), [(0, Kind::Picker), (88, Kind::Picker)]);
}

#[test]
fn this_account_is_never_acted_on_even_without_a_record() {
    let classified = classify(
        &[window(501, 10)],
        |_| Record::default(),
        501,
        Some(501),
        None,
    );
    assert_eq!(kinds(&classified), [(501, Kind::Mine)]);
}

#[test]
fn the_account_on_screen_is_never_an_orphan() {
    let classified = classify(
        &[window(504, 900)],
        |_| Record::default(),
        501,
        Some(503),
        Some(504),
    );
    assert_eq!(kinds(&classified), [(504, Kind::OtherPerson)]);

    let helper = classify(
        &[window(503, 900)],
        |_| account(Some(60)),
        501,
        Some(503),
        Some(503),
    );
    assert_eq!(
        kinds(&helper),
        [(503, Kind::Helper)],
        "the helper on screen"
    );
}

#[test]
fn an_account_getpwuid_found_is_never_searched_for_again() {
    let found = Ok(Some(crate::account::Account {
        name: "helper".to_owned(),
        uid: 503,
        home: "/Users/helper".into(),
    }));
    let record = record_with(503, false, found, |_| panic!("searched every directory"));
    assert!(record.exists());

    let failed = Err(crate::error::Error::new(Code::Setup, "The lookup failed"));
    let record = record_with(503, false, failed, |_| panic!("searched every directory"));
    assert!(
        record.exists(),
        "a failed lookup counts as an account found"
    );
}

#[test]
fn an_account_getpwuid_did_not_find_is_searched_for_in_every_directory() {
    let mut asked = None;
    let record = record_with(1201, false, Ok(None), |uid| {
        asked = Some(uid);
        true
    });
    assert_eq!(asked, Some(1201));
    assert!(record.exists(), "a network account");

    let record = record_with(1202, false, Ok(None), |_| false);
    assert!(!record.exists(), "a deleted account");
}

#[test]
fn a_network_account_is_never_an_orphan() {
    let network = Record {
        passwd: false,
        directory: true,
        age: None,
    };
    let classified = classify(&[window(1201, 900)], |_| network, 501, Some(503), Some(501));
    assert_eq!(kinds(&classified), [(1201, Kind::OtherPerson)]);

    let cached = Record {
        passwd: true,
        directory: false,
        age: None,
    };
    let classified = classify(&[window(1202, 900)], |_| cached, 501, None, Some(501));
    assert_eq!(kinds(&classified), [(1202, Kind::OtherPerson)]);
}

#[test]
fn a_login_without_a_dock_for_long_is_stuck_but_setup_assistant_is_not() {
    let core = "/System/Library/CoreServices";
    let classified = [(window(503, 130), Kind::Helper)];

    let bare = listed_processes(&format!(
        "503 10 02:10 {core}/loginwindow.app/Contents/MacOS/loginwindow\n"
    ));
    assert_eq!(
        blockers(&classified, &bare),
        [Blocker::StuckLogin { uid: 503 }]
    );

    let assistant = listed_processes(&format!(
        "503 11 02:00 {core}/Setup Assistant.app/Contents/MacOS/Setup Assistant\n"
    ));
    assert!(blockers(&classified, &assistant).is_empty());

    let docked = listed_processes(&format!(
        "503 12 02:00 {core}/Dock.app/Contents/MacOS/Dock\n"
    ));
    assert!(blockers(&classified, &docked).is_empty());

    let young = [(window(503, 90), Kind::Helper)];
    assert!(blockers(&young, &bare).is_empty(), "still within its time");

    let mine = [(window(501, 900), Kind::Mine)];
    assert!(blockers(&mine, &bare).is_empty(), "never this account");
}

#[test]
fn a_running_account_updater_and_deleted_accounts_block_a_login() {
    let updater = listed_processes("0 40 00:05 /System/Library/CoreServices/UserAccountUpdater\n");
    assert_eq!(blockers(&[], &updater), [Blocker::AccountUpdating]);

    let classified = [
        (window(502, 900), Kind::Orphan),
        (window(503, 900), Kind::HelperLeftover),
    ];
    let found = blockers(&classified, &[]);
    assert_eq!(
        found,
        [
            Blocker::Orphan { uid: 502 },
            Blocker::HelperLeftover { uid: 503 }
        ]
    );
    assert_eq!(
        found.iter().map(Blocker::uid).collect::<Vec<_>>(),
        [Some(502), Some(503)]
    );
    assert_eq!(Blocker::AccountUpdating.uid(), None);
    assert_eq!(
        Blocker::StuckLogin { uid: 1 }.words(),
        "another account is still logging in."
    );
}

#[test]
fn a_login_is_over_only_once_its_loginwindow_and_dock_are_both_gone() {
    let core = "/System/Library/CoreServices";
    let both = listed_processes(&format!(
        "502 10 00:10 {core}/loginwindow.app/Contents/MacOS/loginwindow\n\
             502 11 00:10 {core}/Dock.app/Contents/MacOS/Dock\n"
    ));
    assert!(!logged_out_in(&both, 502));
    assert!(!logged_out_in(&both[..1], 502), "a loginwindow alone");
    assert!(!logged_out_in(&both[1..], 502), "a Dock alone");
    assert!(logged_out_in(&[], 502));
    assert!(logged_out_in(&both, 503), "another account's");
}

#[test]
fn closed_logins_are_gone_only_once_seen_without_loginwindow_and_dock_twice() {
    use std::{cell::RefCell, collections::VecDeque};

    let core = "/System/Library/CoreServices";
    let dock = listed_processes(&format!(
        "7001 11 00:10 {core}/Dock.app/Contents/MacOS/Dock\n"
    ));
    let window = listed_processes(&format!(
        "7003 10 00:10 {core}/loginwindow.app/Contents/MacOS/loginwindow\n"
    ));
    let gone = |looks: Vec<Option<Vec<Process>>>| {
        let looks = RefCell::new(looks.into_iter().collect::<VecDeque<_>>());
        gone_twice(
            &[7001, 7003],
            || looks.borrow_mut().pop_front().flatten(),
            || {},
        )
    };

    assert!(gone(vec![Some(vec![]), Some(vec![])]));
    assert!(!gone(vec![Some(dock.clone()), Some(vec![])]), "a Dock left");
    assert!(
        !gone(vec![Some(vec![]), Some(window)]),
        "back by the second look"
    );
    assert!(!gone(vec![None, Some(vec![])]), "a list that failed");
}

#[test]
fn a_login_is_left_behind_only_once_its_desktop_has_been_up_for_minutes() {
    let core = "/System/Library/CoreServices";
    let listing = |age: &str, dock: bool| {
        let mut text = format!("503 10 {age} {core}/loginwindow.app/Contents/MacOS/loginwindow\n");
        if dock {
            text.push_str(&format!(
                "503 11 {age} {core}/Dock.app/Contents/MacOS/Dock\n"
            ));
        }
        listed_processes(&text)
    };
    assert!(stale_in(&listing("04:00", true), 503));
    assert!(!stale_in(&listing("02:00", true), 503), "still starting");
    assert!(!stale_in(&listing("04:00", false), 503), "no desktop yet");
    assert!(!stale_in(&listing("04:00", true), 504), "another account's");
}

#[test]
fn the_desktop_waits_for_the_account_updater_to_end() {
    let core = "/System/Library/CoreServices";
    let listed = listed_processes(&format!(
        "502 10 00:10 {core}/Dock.app/Contents/MacOS/Dock\n\
             502 11 00:10 {core}/Finder.app/Contents/MacOS/Finder\n"
    ));
    assert!(desktop_up_in(&listed, 502));

    let mut updating = listed.clone();
    updating.extend(listed_processes(&format!(
        "0 12 00:01 {core}/UserAccountUpdater\n"
    )));
    assert!(!desktop_up_in(&updating, 502));
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
