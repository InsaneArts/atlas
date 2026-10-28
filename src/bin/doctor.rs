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

    // An action of unknown outcome refuses every agent's action until the
    // helper is recovered, whoever caused it; recovery needs the helper
    // up, so its step comes last. One read of the marker, which makes
    // nothing when there is none; one that fails says nothing.
    let controller = client.controller.clone();
    let causes = tokio::task::spawn_blocking(move || controller.pending_causes())
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
    let session = client.host_session.as_ref().map(HostSession::key);
    let asking = asker(client.actor, session, ui::interactive());
    let unknown = Recovery::of(&causes, asking).map(Recovery::step);
    if let Some(checks) = report["checks"].as_array_mut() {
        checks.push(json!({
            "name": NO_UNKNOWN_OUTCOME,
            "ok": unknown.is_none(),
            "nextStep": unknown,
        }));
    }
    if let Some(step) = unknown {
        report["ready"] = json!(false);
        if let Some(steps) = report["nextSteps"].as_array_mut() {
            steps.push(json!(step));
        }
    }

    report
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
            generation: atlas::control::token(),
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
}
