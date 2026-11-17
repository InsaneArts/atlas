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
    let held = machine_hello(
        "Aqua",
        &declared,
        &itself(),
        open,
        ready(),
        &clear(),
        idle(),
    );
    let admitted = machine_hello(
        "Aqua",
        &declared,
        &itself(),
        Posture::CLOSED,
        ready(),
        &clear(),
        idle(),
    );

    assert_eq!(held["remoteLogin"], "on");
    assert_eq!(held["agents"]["admitted"], false);
    assert_eq!(held["agents"]["held"], "Remote Login is on in the guest.");
    assert_eq!(
        admitted["agents"],
        json!({"admitted": true, "held": null, "stopped": false})
    );
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
            &itself(),
            Posture::CLOSED,
            Input {
                standing,
                failure: failure.map(|(code, error)| crate::machine::input::Failure {
                    code: code.to_owned(),
                    error: error.to_owned(),
                }),
            },
            &clear(),
            idle(),
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

/// No Stop kept, and none in effect.
fn clear() -> Stop {
    Stop {
        kept: Some(crate::machine::stop::Kept::Clear),
        halted: false,
    }
}

/// Nothing running, every part known.
fn idle() -> Work {
    Work {
        jobs: Some(0),
        changes: Some(0),
        supervisors: Some(0),
    }
}

/// A virtual machine in its first boot, its daemon of the build `b1`.
fn itself() -> Itself {
    Itself {
        boot: Some("boot-1".to_owned()),
        virtual_machine: Some(true),
        build: Some("b1".to_owned()),
    }
}

#[test]
fn the_machine_hello_says_what_the_station_proves_it_by() {
    let declared = atlas_session::compatibility::Declared::ours(vec!["machine-proof-v1".into()]);
    let ready = Input {
        standing: Some("ready"),
        failure: None,
    };

    let hello = machine_hello(
        "Aqua",
        &declared,
        &itself(),
        Posture::CLOSED,
        ready,
        &clear(),
        idle(),
    );

    assert_eq!(
        hello["machine"],
        json!({"mode": "machine", "boot": "boot-1", "virtual": true})
    );
    assert_eq!(hello["daemon"]["build"], "b1");
    assert_eq!(hello["daemon"]["capabilities"], json!(["machine-proof-v1"]));
    for side in ["width", "height", "scale"] {
        assert!(
            hello["display"][side].as_f64().is_some_and(|it| it > 0.0),
            "{hello}"
        );
    }
    for permission in ["accessibility", "postEvents", "screenCapture"] {
        assert!(hello["permissions"][permission].is_boolean(), "{hello}");
    }
}

#[test]
fn a_machine_finds_itself_in_this_boot_as_its_kernel_says() {
    let found = Itself::now(Some("b1".to_owned()));

    assert_eq!(found.boot, atlas_core::process::boot().ok());
    assert_eq!(
        found.virtual_machine,
        atlas_core::process::in_virtual_machine()
    );
    assert_eq!(found.build.as_deref(), Some("b1"));
}

/// A machine hello in every respect closed and ready, but `stop` and `work`.
fn reporting(stop: &Stop, work: Work) -> Value {
    let ready = Input {
        standing: Some("ready"),
        failure: None,
    };
    let declared = atlas_session::compatibility::Declared::ours(Vec::new());
    machine_hello(
        "Aqua",
        &declared,
        &itself(),
        Posture::CLOSED,
        ready,
        stop,
        work,
    )
}

#[test]
fn the_machine_hello_says_whether_its_stop_is_clear_kept_and_why_or_unknown() {
    use crate::machine::stop::{Kept, Why};
    let stopped = |why| Stop {
        kept: Some(Kept::Stopped { why }),
        halted: true,
    };

    let clear = reporting(&clear(), idle());
    let person = reporting(&stopped(Why::Person), idle());
    let lost = reporting(&stopped(Why::ControlLost), idle());
    let unknown = reporting(
        &Stop {
            kept: None,
            halted: true,
        },
        idle(),
    );

    assert_eq!(clear["stop"], json!({"state": "clear"}));
    assert_eq!(person["stop"], json!({"state": "stopped", "why": "person"}));
    assert_eq!(
        lost["stop"],
        json!({"state": "stopped", "why": "control-lost"})
    );
    assert_eq!(unknown["stop"], json!({"state": "unknown"}));
    assert_eq!(person["agents"]["stopped"], true);
    // Admission by posture is said apart from the Stop.
    assert_eq!(person["agents"]["admitted"], true);
}

#[test]
fn the_machine_hello_says_work_runs_when_any_part_runs_and_none_only_when_all_are_known() {
    let with = |jobs, changes, supervisors| {
        reporting(
            &clear(),
            Work {
                jobs,
                changes,
                supervisors,
            },
        )["work"]
            .clone()
    };

    let none = with(Some(0), Some(0), Some(0));
    let job = with(Some(1), Some(0), Some(0));
    let change = with(Some(0), Some(2), Some(0));
    let left = with(Some(0), Some(0), Some(1));
    let running_beside_unknown = with(None, Some(1), None);

    assert_eq!(
        none,
        json!({"state": "none", "jobs": 0, "changes": 0, "supervisors": 0})
    );
    assert_eq!(job["state"], "running");
    assert_eq!(change["state"], "running");
    assert_eq!(left["state"], "running");
    assert_eq!(running_beside_unknown["state"], "running");
    assert_eq!(running_beside_unknown["jobs"], "unknown");
}

#[test]
fn work_any_part_of_which_cannot_be_told_is_never_said_to_be_none() {
    for unknown in [
        Work {
            jobs: None,
            ..idle()
        },
        Work {
            changes: None,
            ..idle()
        },
        Work {
            supervisors: None,
            ..idle()
        },
    ] {
        let work = reporting(&clear(), unknown)["work"].clone();

        assert_eq!(work["state"], "unknown", "{work}");
    }
}
