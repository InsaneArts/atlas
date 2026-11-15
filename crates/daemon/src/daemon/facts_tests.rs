use super::*;

#[test]
fn hello_describes_this_process_in_its_session() {
    let reply = hello(&manager_name(), None, None);
    assert_eq!(reply["user"]["uid"], nix::unistd::getuid().as_raw());
    assert!(reply["user"]["name"].is_string());
    assert!(
        reply["user"]["home"]
            .as_str()
            .is_some_and(|home| home.starts_with('/'))
    );
    assert_eq!(reply["daemon"]["pid"], std::process::id());
    assert_eq!(reply["daemon"]["protocol"], 1);
    assert!(atlas_core::session::has_capability(
        &reply["daemon"],
        atlas_core::protocol::SESSION_PIN
    ));
    assert!(reply["permissions"]["screenCapture"].is_boolean());
    assert!(
        reply["display"]["scale"]
            .as_f64()
            .is_some_and(|scale| scale >= 1.0)
    );
    assert!(reply["daemon"]["resources"]["footprint"].is_u64());
    assert!(reply["daemon"]["resources"].get("screenRecorder").is_none());
}

#[test]
fn hello_reports_the_screen_recorders_memory_and_restarts_beside_its_own() {
    let recorder = recorder::Report {
        usage: Some(resources::Usage {
            footprint: 6 << 30,
            peak_footprint: 7 << 30,
            resident: 2 << 30,
        }),
        resets: 2,
        last_reset_at: Some(1_793_923_943_551),
        reset_failures: 0,
        reset_after_update: true,
    };
    let reply = hello(&manager_name(), None, Some(recorder));
    let resources = &reply["daemon"]["resources"];
    assert!(resources["footprint"].is_u64());
    assert_eq!(
        resources["screenRecorder"],
        json!({"footprint": 6_u64 << 30, "peakFootprint": 7_u64 << 30, "resident": 2_u64 << 30,
            "resets": 2, "lastResetAt": 1_793_923_943_551_u64, "resetFailures": 0,
            "resetAfterUpdate": true})
    );
}

#[test]
fn a_recorder_that_is_not_running_still_reports_its_restarts() {
    let reply = hello(
        &manager_name(),
        None,
        Some(recorder::Report {
            reset_failures: 1,
            ..recorder::Report::default()
        }),
    );
    assert_eq!(
        reply["daemon"]["resources"]["screenRecorder"],
        json!({"resets": 0, "resetFailures": 1})
    );
}

#[test]
fn remote_login_is_off_only_when_disabled_unloaded_and_not_listening() {
    assert_eq!(
        remote_login_from(Some(true), Some(false), Some(false)),
        Some(false)
    );
    for on in [
        (Some(false), Some(false), Some(false)),
        (Some(true), Some(true), Some(false)),
        (Some(true), Some(false), Some(true)),
        (None, None, Some(true)),
    ] {
        assert_eq!(remote_login_from(on.0, on.1, on.2), Some(true), "{on:?}");
    }
    assert_eq!(remote_login_from(Some(true), None, Some(false)), None);
}

#[test]
fn launchd_says_whether_remote_login_is_disabled() {
    let printed = "disabled services = {\n\t\t\"com.openssh.sshd\" => disabled\n\t\t\"com.apple.other\" => enabled\n}\n";
    assert_eq!(sshd_disabled(printed), Some(true));
    assert_eq!(
        sshd_disabled(&printed.replace("=> disabled", "=> enabled")),
        Some(false)
    );
    assert_eq!(sshd_disabled("disabled services = {\n}\n"), None);
}

#[test]
fn agents_are_held_unless_remote_login_is_off_and_the_account_no_administrator() {
    assert_eq!(Posture::CLOSED.held(), None);
    for open in [
        (Some(true), Some(false)),
        (None, Some(false)),
        (Some(false), Some(true)),
        (Some(false), None),
        (None, None),
    ] {
        let posture = Posture {
            remote_login: open.0,
            admin: open.1,
        };
        assert!(posture.held().is_some(), "{posture:?}");
    }
}

#[test]
fn the_machine_hello_says_why_agents_are_held() {
    let declared = atlas_session::compatibility::Declared::ours(Vec::new());
    let open = Posture {
        remote_login: Some(true),
        admin: Some(false),
    };

    let ready = || Input {
        standing: Some("ready"),
        failure: None,
    };
    let held = machine_hello("Aqua", &declared, open, ready());
    let admitted = machine_hello("Aqua", &declared, Posture::CLOSED, ready());

    assert_eq!(held["remoteLogin"], "on");
    assert_eq!(held["agents"]["admitted"], false);
    assert_eq!(held["agents"]["held"], "Remote Login is on in the guest.");
    assert_eq!(admitted["agents"], json!({"admitted": true, "held": null}));
}

#[test]
fn the_guest_posture_is_read_without_failing() {
    let posture = Posture::now();

    assert_eq!(posture.held().is_none(), posture == Posture::CLOSED);
}

#[test]
fn the_machine_hello_says_how_input_stands_and_what_the_person_can_do() {
    let declared = atlas_session::compatibility::Declared::ours(Vec::new());
    let input = |standing: Option<&'static str>, failure: Option<(&str, &str)>| {
        machine_hello(
            "Aqua",
            &declared,
            Posture::CLOSED,
            Input {
                standing,
                failure: failure.map(|(code, error)| crate::machine::input::Failure {
                    code: code.to_owned(),
                    error: error.to_owned(),
                }),
            },
        )["input"]
            .clone()
    };

    let ready = input(Some("ready"), None);
    let unallowed = input(
        Some("gone"),
        Some((
            "permission",
            "The input process does not have Accessibility permission",
        )),
    );
    let failed = input(
        Some("gone"),
        Some(("identity", "Native input requires the session")),
    );
    let starting = input(Some("starting"), None);
    let unsure = input(Some("cleanupUnknown"), None);
    let none = input(None, None);

    assert_eq!(ready["held"], Value::Null);
    assert_eq!(ready["standing"], "ready");
    assert_eq!(unallowed["failure"]["code"], "permission");
    assert!(
        unallowed["held"]
            .as_str()
            .unwrap()
            .contains("Privacy & Security")
    );
    assert_eq!(failed["held"], "Native input requires the session");
    assert!(starting["held"].as_str().unwrap().contains("starting"));
    assert!(
        unsure["held"]
            .as_str()
            .unwrap()
            .contains("not been confirmed")
    );
    assert_eq!(none["standing"], Value::Null);
    assert!(none["held"].is_string());
}
