//! Controller-side observations and enrichment for the doctor report.
use atlas::{
    client::{Actor, Client},
    host_session::{Asker, HostSession, Key, Recovery},
    ui,
};
use serde_json::{Value, json};

/// Doctor's check that no change's outcome is unknown: while one is, Atlas
/// refuses every agent's action until the helper is recovered.
const NO_UNKNOWN_OUTCOME: &str = "No action of unknown outcome";

/// Collect the report before its caller chooses full or summarized output.
pub(super) async fn collect(client: &Client) -> Value {
    let uid = client.config.helper_uid;
    let status = client.status().await;
    let account = atlas::setup::account_with_uid(uid).ok().flatten();

    // Not answering because the helper is logged out or gone: only the
    // person can log it in again.
    let absence = if atlas::session::silent(&status) {
        tokio::task::spawn_blocking(move || atlas::session::absence(uid))
            .await
            .ok()
            .flatten()
    } else {
        None
    };

    let shell_blocked = status["code"] == "blocked";
    let mut report = atlas::capability::doctor(status, uid);

    if let Some(absence) = &absence {
        absence.explain(&mut report);
    }

    // Both states make the helper unusable; init refuses them too.
    let mut first_steps = Vec::new();

    // A deleted account's login left running can stall the next login, or
    // be mistaken for the helper's.
    let sessions = tokio::task::spawn_blocking(move || atlas::session::sessions(Some(uid)))
        .await
        .ok()
        .flatten();
    let leftover = sessions.as_ref().map(|sessions| {
        sessions
            .leftover_uids(atlas::session::Kind::Orphan)
            .into_iter()
            .chain(sessions.leftover_uids(atlas::session::Kind::HelperLeftover))
            .next()
    });
    let left = leftover.flatten().map(|uid| {
        format!(
            "A deleted account (user ID {uid}) is still logged in. Close it in Atlas › Settings › \
             Helper, or run atlas init."
        )
    });
    if let Some(checks) = report["checks"].as_array_mut() {
        checks.push(json!({
            "name": "No logins left by deleted accounts",
            "ok": leftover.map(|uid| uid.is_none()),
            "nextStep": left,
        }));
    }
    if let Some(step) = left {
        first_steps.push(step);
    }

    if let Some(account) = &account {
        match atlas::setup::administrator(&account.name) {
            Some(false) => {}
            Some(true) => first_steps.push(format!(
                "{} must be a standard account, not an administrator: change it in System \
                 Settings › Users & Groups.",
                account.name
            )),
            None => first_steps.push(format!(
                "macOS did not say whether {} is an administrator. Run atlas doctor outside \
                 any sandbox.",
                account.name
            )),
        }
    }

    // Every command keeps a lock and screenshots in the person's Library;
    // a shell that may not write there cannot use the helper. (A shell
    // that could not even connect has been told so already.)
    if !shell_blocked && let Err(error) = client.writable() {
        first_steps.push(error.error);
    }

    if !first_steps.is_empty() {
        report["ready"] = json!(false);
        if let Some(steps) = report["nextSteps"].as_array_mut() {
            for (index, step) in first_steps.into_iter().enumerate() {
                steps.insert(index, json!(step));
            }
        }
    }

    let controller = client.controller.clone();
    let causes = pending_result(
        tokio::time::timeout(
            std::time::Duration::from_millis(250),
            tokio::task::spawn_blocking(move || {
                controller.pending_causes_within(std::time::Duration::from_millis(50))
            }),
        )
        .await
        .ok(),
    );
    let session = client.host_session.as_ref().map(HostSession::key);
    pending_report(
        &mut report,
        causes,
        asker(client.actor, session, ui::interactive()),
    );
    report
}

fn pending_result(
    result: Option<
        std::result::Result<
            atlas::error::Result<Vec<atlas::control::Cause>>,
            tokio::task::JoinError,
        >,
    >,
) -> atlas::error::Result<Vec<atlas::control::Cause>> {
    result.and_then(Result::ok).unwrap_or_else(|| {
        Err(atlas::error::Error::new(
            atlas::error::Code::State,
            "Atlas could not read pending actions",
        ))
    })
}

/// Readiness is unknown when pending state cannot be read. Recovery advice
/// comes only from a successful snapshot and the existing ownership rules.
fn pending_report(
    report: &mut Value,
    causes: atlas::error::Result<Vec<atlas::control::Cause>>,
    asking: Asker<'_>,
) {
    let (ok, step) = match causes {
        Ok(causes) => {
            let step = Recovery::of(&causes, asking).map(Recovery::step);
            (Some(step.is_none()), step)
        }
        Err(_) => (
            None,
            Some(
                "Atlas could not read whether an action is still pending. Run atlas doctor again.",
            ),
        ),
    };
    if let Some(checks) = report["checks"].as_array_mut() {
        checks.push(json!({"name": NO_UNKNOWN_OUTCOME, "ok": ok, "nextStep": step}));
    }
    if ok != Some(true) {
        report["ready"] = json!(false);
        if let Some(step) = step
            && let Some(steps) = report["nextSteps"].as_array_mut()
        {
            steps.push(json!(step));
        }
    }
}

/// Who asks doctor: the person from Atlas, or at a terminal they type in
/// with no host session; otherwise an agent, of `session` when it has one.
/// How the output is printed says nothing of who reads it.
fn asker(actor: Actor, session: Option<&Key>, interactive: bool) -> Asker<'_> {
    match (actor, session) {
        (Actor::Viewer, _) => Asker::Person,
        (_, None) if interactive => Asker::Person,
        (_, session) => Asker::Agent(session),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas::{control::Cause, host_session::Host};

    fn cause(session: &Key) -> Cause {
        Cause {
            generation: atlas::files::token(),
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
        for report in [verbose.clone(), atlas::capability::summary(verbose)] {
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
                atlas::capability::summary(report),
                atlas::capability::summary(ready())
            );
        }
    }

    #[test]
    fn an_unreadable_pending_state_is_unknown_and_not_ready() {
        let mut verbose = ready();
        pending_report(
            &mut verbose,
            Err(atlas::error::Error::new(atlas::error::Code::Busy, "busy")),
            Asker::Person,
        );
        for report in [verbose.clone(), atlas::capability::summary(verbose)] {
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
        let failed = tokio::task::spawn_blocking(|| -> atlas::error::Result<Vec<Cause>> {
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
}
