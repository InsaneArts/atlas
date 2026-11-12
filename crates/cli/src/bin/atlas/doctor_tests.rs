use super::*;
use atlas_control::Cause;
use atlas_core::host::Host;

fn cause(session: &Key) -> Cause {
    Cause {
        generation: atlas_core::files::token(),
        running: false,
        command: false,
        session: Some(session.clone()),
    }
}

#[test]
fn an_agent_session_at_a_terminal_is_never_told_to_restart_over_anothers_action() {
    let (own, other) = (Key::of(Host::Claude, "own"), Key::of(Host::Codex, "other"));
    let anothers = [cause(&other)];
    for interactive in [false, true] {
        let asking = asker(Actor::Agent, Some(&own), interactive);
        assert_eq!(asking, Asker::Agent(Some(&own)), "{interactive}");
        assert_eq!(
            Recovery::of(&anothers, asking),
            Some(Recovery::Others),
            "{interactive}"
        );
        assert_eq!(
            Recovery::of(&[cause(&own)], asking),
            Some(Recovery::Own),
            "{interactive}"
        );
    }
}

#[test]
fn only_the_app_or_a_terminal_with_no_session_asks_as_the_person() {
    let own = Key::of(Host::Claude, "own");
    assert_eq!(asker(Actor::Viewer, None, false), Asker::Person);
    assert_eq!(asker(Actor::Agent, None, true), Asker::Person);
    assert_eq!(asker(Actor::Agent, None, false), Asker::Agent(None));
    assert_eq!(
        asker(Actor::Agent, Some(&own), true),
        Asker::Agent(Some(&own))
    );
}
fn ready() -> Value {
    json!({"ok": true, "ready": true, "checks": [], "nextSteps": []})
}

#[test]
fn an_unknown_outcome_makes_both_doctor_outputs_not_ready() {
    let own = Key::of(Host::Claude, "own");
    let mut verbose = ready();
    pending_report(
        &mut verbose,
        Ok(vec![cause(&own)]),
        Asker::Agent(Some(&own)),
    );
    for report in [verbose.clone(), atlas_client::capability::summary(verbose)] {
        assert_eq!(report["ready"], false);
        assert_eq!(report["checks"][0]["ok"], false);
        assert!(
            report["nextSteps"][0]
                .as_str()
                .unwrap()
                .contains("atlas restart")
        );
    }
}

#[test]
fn a_live_command_alone_does_not_require_recovery() {
    let own = Key::of(Host::Codex, "own");
    let mut running = cause(&own);
    running.running = true;
    running.command = true;
    for causes in [vec![], vec![running]] {
        let mut report = ready();
        pending_report(&mut report, Ok(causes), Asker::Agent(Some(&own)));
        assert_eq!(report["checks"][0]["ok"], true);
        assert_eq!(
            atlas_client::capability::summary(report),
            atlas_client::capability::summary(ready())
        );
    }
}

#[test]
fn an_unreadable_pending_state_is_unknown_and_not_ready() {
    let mut verbose = ready();
    pending_report(
        &mut verbose,
        Err(atlas_core::error::Error::new(
            atlas_core::error::Code::Busy,
            "busy",
        )),
        Asker::Person,
    );
    for report in [verbose.clone(), atlas_client::capability::summary(verbose)] {
        assert_eq!(report["ready"], false);
        assert!(report["checks"][0]["ok"].is_null());
        let step = report["nextSteps"][0].as_str().unwrap();
        assert_eq!(
            step,
            "Atlas could not read whether an action is still pending. Run atlas doctor again."
        );
        assert!(!step.contains("restart"));
    }
}

#[test]
fn setup_steps_stay_before_recovery() {
    let own = Key::of(Host::Claude, "own");
    let mut report = ready();
    report["ready"] = json!(false);
    report["nextSteps"] = json!(["Finish setup"]);
    pending_report(&mut report, Ok(vec![cause(&own)]), Asker::Person);
    assert_eq!(report["nextSteps"][0], "Finish setup");
    assert!(report["nextSteps"][1].as_str().unwrap().contains("restart"));
    pending_report(&mut report, Ok(vec![]), Asker::Person);
    assert_eq!(report["ready"], false);
}

#[test]
fn doctor_never_tells_a_foreign_agent_to_restart() {
    let own = Key::of(Host::Claude, "own");
    let other = Key::of(Host::Codex, "other");
    let mut report = ready();
    pending_report(
        &mut report,
        Ok(vec![cause(&other)]),
        Asker::Agent(Some(&own)),
    );
    assert_eq!(report["ready"], false);
    assert!(
        !report["nextSteps"][0]
            .as_str()
            .unwrap()
            .contains("atlas restart")
    );
}
#[tokio::test]
async fn a_failed_pending_worker_is_not_a_clean_marker() {
    let failed = tokio::task::spawn_blocking(|| -> atlas_core::error::Result<Vec<Cause>> {
        panic!("reader failed")
    })
    .await;
    for result in [pending_result(Some(failed)), pending_result(None)] {
        let mut report = ready();
        pending_report(&mut report, result, Asker::Agent(None));
        assert_eq!(report["ready"], false);
        assert!(report["checks"][0]["ok"].is_null());
    }
}

#[test]
fn doctor_warns_when_the_helper_should_be_silent_and_is_not() {
    let unavailable = json!({"sound": {
        "helperSound": "unavailable",
        "reason": "Switch to the helper and allow Atlas when macOS asks."
    }});
    assert_eq!(
        sound_warning(&unavailable).as_deref(),
        Some("Switch to the helper and allow Atlas when macOS asks.")
    );

    for sound in ["muted", "on"] {
        let said = json!({"sound": {"helperSound": sound, "reason": null}});
        assert_eq!(sound_warning(&said), None, "{sound}");
    }
    assert_eq!(sound_warning(&json!({"ok": false})), None);
}

fn with_recorder(footprint: Value, failures: Value) -> Value {
    let mut report = ready();
    report["warnings"] = json!([]);
    report["session"] = json!({"daemon": {"resources": {
        "footprint": 1,
        "screenRecorder": {"footprint": footprint, "resets": 0, "resetFailures": failures},
    }}});
    report
}

#[test]
fn a_recorder_the_helper_cannot_restart_gets_a_warning_with_the_way_to_reset_it() {
    // Atlas.app runs doctor as the person, verbose.
    let asking = asker(Actor::Viewer, None, false);
    let mut report = with_recorder(json!(6 * GIGABYTE + GIGABYTE / 2), json!(RECORDER_TRIES));
    recorder_report(&mut report, asking);
    let warning = "The helper's screen recorder is using 6.5 GB of memory and Atlas could not \
                   restart it. Log the helper out and back in to reset it.";
    assert_eq!(report["warnings"], json!([warning]));
    assert_eq!(
        atlas_client::capability::summary(report)["warnings"],
        json!([warning])
    );
}

#[test]
fn a_large_screen_recorder_the_helper_restarts_on_its_own_gets_no_warning() {
    for failures in [
        json!(0),
        json!(RECORDER_TRIES - 1),
        Value::Null,
        json!("3"),
        json!(-1),
    ] {
        let mut report = with_recorder(json!(6 * GIGABYTE), failures.clone());
        recorder_report(&mut report, Asker::Person);
        assert_eq!(report["warnings"], json!([]), "{failures}");
    }
}

/// A ready report with its list of warnings, as doctor builds one.
fn warned() -> Value {
    let mut report = ready();
    report["warnings"] = json!([]);
    report
}

fn restart(label: &str) -> console_agents::Restart {
    console_agents::Restart {
        label: label.to_owned(),
        runs: 357,
    }
}

#[test]
fn an_unknown_screen_recorder_footprint_gets_no_warning() {
    for footprint in [Value::Null, json!("6 GB"), json!(-1)] {
        let mut report = with_recorder(footprint.clone(), json!(RECORDER_TRIES));
        recorder_report(&mut report, Asker::Person);
        assert_eq!(report["warnings"], json!([]), "{footprint}");
    }
}

#[test]
fn an_agents_verbose_report_keeps_the_figures_but_not_the_logout_warning() {
    let own = Key::of(Host::Claude, "own");
    for interactive in [false, true] {
        for asking in [
            asker(Actor::Agent, Some(&own), interactive),
            asker(Actor::Agent, None, false),
        ] {
            let mut report = with_recorder(json!(6 * GIGABYTE), json!(RECORDER_TRIES));
            recorder_report(&mut report, asking);
            assert_eq!(report["warnings"], json!([]), "{asking:?}");
            let recorder = &report["session"]["daemon"]["resources"]["screenRecorder"];
            assert_eq!(recorder["footprint"], 6 * GIGABYTE);
            assert_eq!(recorder["resetFailures"], RECORDER_TRIES);
        }
    }
}

#[test]
fn a_service_slowing_the_mac_warns_without_making_the_helper_not_ready() {
    let mut report = warned();
    let restarts = [restart("com.apple.BTServer.cloudpairing")];
    restarting_report(&mut report, Some(&restarts), Asker::Person, false);

    assert_eq!(report["ready"], true);
    assert_eq!(report["checks"], json!([]));
    let warning = report["warnings"][0].as_str().unwrap().to_owned();
    assert_eq!(warning, SLOWING);
    assert!(warning.contains("Atlas tries to keep it paused"));
    assert!(warning.contains("atlas logout"));
    assert!(report.get("restartingJobs").is_none());
    assert_eq!(
        atlas_client::capability::summary(report)["warnings"][0],
        *warning
    );
}

#[test]
fn other_restarting_jobs_or_an_unreadable_log_say_nothing_but_the_details() {
    for restarts in [Some(vec![restart("com.apple.example")]), Some(vec![]), None] {
        let mut report = warned();
        restarting_report(&mut report, restarts.as_deref(), Asker::Person, false);
        assert_eq!(report["warnings"], json!([]), "{restarts:?}");
    }

    let mut verbose = warned();
    restarting_report(
        &mut verbose,
        Some(&[restart("com.apple.example")]),
        Asker::Person,
        true,
    );
    assert_eq!(verbose["warnings"], json!([]));
    assert_eq!(
        verbose["restartingJobs"],
        json!([{"label": "com.apple.example", "runs": 357}])
    );
}

#[test]
fn an_agent_is_not_told_to_log_the_helper_out_but_keeps_the_details() {
    let own = Key::of(Host::Claude, "own");
    let restarts = [restart("com.apple.BTServer.cloudpairing")];
    for asking in [
        asker(Actor::Agent, Some(&own), true),
        asker(Actor::Agent, None, false),
    ] {
        let mut report = warned();
        restarting_report(&mut report, Some(&restarts), asking, true);
        assert_eq!(report["warnings"], json!([]), "{asking:?}");
        assert_eq!(
            report["restartingJobs"][0]["label"],
            "com.apple.BTServer.cloudpairing"
        );
    }
}

#[test]
fn account_names_show_only_to_the_person_in_text_on_a_terminal() {
    let own = Key::of(Host::Claude, "own");
    assert!(shows_accounts(Asker::Person, true, true));
    assert!(
        !shows_accounts(Asker::Person, true, false),
        "piped, where an agent may read it"
    );
    assert!(!shows_accounts(Asker::Person, false, true), "JSON");
    assert!(
        !shows_accounts(Asker::Agent(Some(&own)), true, true),
        "an agent asking"
    );
}

#[test]
fn another_account_named_at_the_persons_screen_warns_with_its_name_and_the_way_back() {
    let mut report = warned();
    named_report(
        &mut report,
        &Named::Another {
            name: "atlas".to_owned(),
        },
        Asker::Person,
    );

    assert_eq!(report["ready"], true);
    assert_eq!(report["checks"], json!([]));
    let warning = report["warnings"][0].as_str().unwrap().to_owned();
    assert!(warning.starts_with("macOS names atlas as the account at your screen, not you."));
    assert!(warning.contains("choose Switch User, then log back in as yourself"));
    assert_eq!(
        atlas_client::capability::summary(report)["warnings"][0],
        *warning
    );
}

#[test]
fn a_rightly_named_account_or_an_unknown_one_says_nothing() {
    for named in [Named::This, Named::Unknown] {
        let mut report = warned();
        named_report(&mut report, &named, Asker::Person);
        assert_eq!(report["warnings"], json!([]), "{named:?}");
    }
}

#[test]
fn an_agent_is_not_told_to_switch_to_the_login_window() {
    let own = Key::of(Host::Claude, "own");
    for asking in [
        asker(Actor::Agent, Some(&own), true),
        asker(Actor::Agent, None, false),
    ] {
        let mut report = warned();
        named_report(
            &mut report,
            &Named::Another {
                name: "atlas".to_owned(),
            },
            asking,
        );
        assert_eq!(report["warnings"], json!([]), "{asking:?}");
    }
}

#[tokio::test]
async fn what_macos_answers_in_time_is_what_doctor_goes_by() {
    let named = answer_within(std::time::Duration::from_secs(60), Named::Unknown, || {
        Named::Another {
            name: "atlas".to_owned(),
        }
    })
    .await;

    assert_eq!(
        named,
        Named::Another {
            name: "atlas".to_owned()
        }
    );
}

#[tokio::test(start_paused = true)]
async fn a_look_macos_never_answers_is_unknown_once_the_limit_has_passed() {
    let (release, held) = std::sync::mpsc::channel::<()>();
    let named = answer_within(LOOK_LIMIT, Named::Unknown, move || {
        // Held until the test lets go, as by a system service that never
        // answers.
        let _ = held.recv();
        Named::Another {
            name: "atlas".to_owned(),
        }
    })
    .await;

    assert_eq!(named, Named::Unknown);
    drop(release);
}

fn warned_for(idle_time: IdleTime) -> String {
    let mut report = warned();
    idle_time_report(&mut report, &idle_time, Asker::Person);
    assert_eq!(report["ready"], true);
    let warning = report["warnings"][0].as_str().unwrap().to_owned();
    assert!(warning.starts_with("Apps on your Mac think you're away"));
    warning
}

#[test]
fn a_stopped_idle_time_tells_the_person_which_app_to_quit_when_one_holds_it() {
    assert!(
        warned_for(IdleTime::Held {
            app: Some("Terminal".to_owned())
        })
        .ends_with("Quit Terminal to fix it.")
    );
    assert!(
        warned_for(IdleTime::Held { app: None })
            .ends_with("and Atlas couldn't tell which. Quitting that app fixes it.")
    );
    assert!(
        warned_for(IdleTime::Stuck)
            .ends_with("logins. Atlas tries to restart it while Atlas.app is open.")
    );
}

#[test]
fn a_live_or_unknown_idle_time_says_nothing() {
    for idle_time in [IdleTime::Live, IdleTime::Unknown] {
        let mut report = warned();
        idle_time_report(&mut report, &idle_time, Asker::Person);
        assert_eq!(report["warnings"], json!([]), "{idle_time:?}");
    }
}

#[test]
fn an_agent_is_not_told_to_quit_an_app() {
    let own = Key::of(Host::Claude, "own");
    let mut report = warned();
    idle_time_report(
        &mut report,
        &IdleTime::Held {
            app: Some("Terminal".to_owned()),
        },
        asker(Actor::Agent, Some(&own), true),
    );
    assert_eq!(report["warnings"], json!([]));
}
