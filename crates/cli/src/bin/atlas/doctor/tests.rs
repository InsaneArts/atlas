use super::*;
use atlas_client::control::Cause;
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
