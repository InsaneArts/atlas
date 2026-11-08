//! Controller-side observations and enrichment for the doctor report.
use atlas_cli::ui;
use atlas_client::{
    client::{Actor, Client},
    console_agents::{self, Restarting},
    host_session::{Asker, Recovery},
};
use atlas_core::host::{HostSession, Key};
use serde_json::{Value, json};

/// Doctor's check that no change's outcome is unknown: while one is, Atlas
/// refuses every agent's action until the helper is recovered.
const NO_UNKNOWN_OUTCOME: &str = "No action of unknown outcome";
/// Doctor's warning while a macOS service that slows the Mac keeps
/// restarting in the helper: Atlas can't stop it, and a log-out clears what
/// it has built up.
const SLOWING: &str = "A macOS service in the helper restarts every 10 seconds, and a macOS \
                       bug makes the restarts slowly slow your whole Mac. Atlas can't turn it \
                       off. Log the helper out and back in every few days to clear it: run \
                       atlas logout, then log in to the helper and switch back.";

const GIGABYTE: u64 = 1 << 30;

/// Warn once the helper has found its screen recorder too large this many
/// times in a row, a minute or more apart, and could not restart it.
const RECORDER_TRIES: u64 = 3;

/// Read and enrich the report, retaining full facts for people or verbose output.
pub(super) async fn report(client: &Client, verbose: bool) -> Value {
    let uid = client.config.helper_uid;
    let status = client.status().await;
    let account = atlas_core::account::account_with_uid(uid).ok().flatten();

    // Not answering because the helper is logged out or gone: only the
    // person can log it in again.
    let absence = if atlas_core::session::silent(&status) {
        tokio::task::spawn_blocking(move || atlas_core::session::absence(uid))
            .await
            .ok()
            .flatten()
    } else {
        None
    };

    let shell_blocked = status["code"] == "blocked";
    let sound = sound_warning(&status);
    // Apple jobs restarting in the helper, from launchd's log; only when
    // the helper itself answered, as a logged-out helper runs none.
    let restarts = if !console_agents::answered(&status) {
        None
    } else {
        tokio::task::spawn_blocking(move || console_agents::read_restarting(uid))
            .await
            .ok()
            .flatten()
    };
    let mut report = atlas_client::capability::doctor(status, uid);
    if let Some(warning) = sound
        && let Some(warnings) = report["warnings"].as_array_mut()
    {
        warnings.push(json!(warning));
    }

    if let Some(absence) = &absence {
        absence.explain(&mut report);
    }

    // Both states make the helper unusable; init refuses them too.
    let mut first_steps = Vec::new();

    // A deleted account's login left running can stall the next login, or
    // be mistaken for the helper's.
    let sessions = tokio::task::spawn_blocking(move || atlas_core::session::sessions(Some(uid)))
        .await
        .ok()
        .flatten();
    let leftover = sessions.as_ref().map(|sessions| {
        sessions
            .leftover_uids(atlas_core::session::Kind::Orphan)
            .into_iter()
            .chain(sessions.leftover_uids(atlas_core::session::Kind::HelperLeftover))
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
        match atlas_core::account::administrator(&account.name) {
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
    let asking = asker(client.actor, session, ui::interactive());
    recorder_report(&mut report, asking);
    restarting_report(&mut report, restarts.as_deref(), asking, verbose);
    pending_report(&mut report, causes, asking);
    if verbose || ui::human() {
        // What the helper can open in the person's home folder: for the
        // person, never in an agent's compact reply.
        if let Ok(Ok(files)) =
            tokio::task::spawn_blocking(move || atlas_client::home_privacy::status(uid)).await
        {
            report["files"] = json!(files);
        }
        report
    } else {
        atlas_client::capability::summary(report)
    }
}

/// Doctor's warning when the helper's mute is not what was asked: its
/// reason, which names the one step that fixes it. A helper that did not
/// answer, or cannot say, is reported elsewhere; sound turned on is the
/// person's choice.
fn sound_warning(status: &Value) -> Option<String> {
    let sound = &status["sound"];
    (sound["helperSound"] == "unavailable").then(|| {
        sound["reason"]
            .as_str()
            .unwrap_or("Apps in the helper are not muted. Run atlas sound off to mute them.")
            .to_owned()
    })
}

/// Warn the person when the helper, as its status says, keeps finding its
/// screen recorder too large and cannot restart it. The helper restarts it
/// on its own otherwise. Only the person can log the helper out, so an agent
/// is not told, verbose or not; the figures stay in a verbose report's
/// status.
fn recorder_report(report: &mut Value, asking: Asker<'_>) {
    if asking != Asker::Person {
        return;
    }
    let recorder = &report["session"]["daemon"]["resources"]["screenRecorder"];
    let stuck = recorder["resetFailures"]
        .as_u64()
        .is_some_and(|tries| tries >= RECORDER_TRIES);
    let Some(bytes) = recorder["footprint"].as_u64().filter(|_| stuck) else {
        return;
    };
    let warning = format!(
        "The helper's screen recorder is using {:.1} GB of memory and Atlas could not restart \
         it. Log the helper out and back in to reset it.",
        bytes as f64 / GIGABYTE as f64
    );
    if let Some(warnings) = report["warnings"].as_array_mut() {
        warnings.push(json!(warning));
    }
}

/// Doctor's warning while a macOS service that slows the Mac restarts in
/// the helper, and, with `verbose`, every Apple job restarting there. It
/// slows the Mac, not agents, so the helper stays ready; only the person
/// can log the helper out, so an agent is not told.
fn restarting_report(
    report: &mut Value,
    restarts: Option<&[console_agents::Restart]>,
    asking: Asker<'_>,
    verbose: bool,
) {
    if asking == Asker::Person
        && Restarting::of(restarts) == Restarting::Restarting
        && let Some(warnings) = report["warnings"].as_array_mut()
    {
        warnings.push(json!(SLOWING));
    }
    if verbose && let Some(restarts) = restarts {
        report["restartingJobs"] = json!(restarts);
    }
}

fn pending_result(
    result: Option<
        std::result::Result<
            atlas_core::error::Result<Vec<atlas_control::Cause>>,
            tokio::task::JoinError,
        >,
    >,
) -> atlas_core::error::Result<Vec<atlas_control::Cause>> {
    result.and_then(Result::ok).unwrap_or_else(|| {
        Err(atlas_core::error::Error::new(
            atlas_core::error::Code::State,
            "Atlas could not read pending actions",
        ))
    })
}

/// Readiness is unknown when pending state cannot be read. Recovery advice
/// comes only from a successful snapshot and the existing ownership rules.
fn pending_report(
    report: &mut Value,
    causes: atlas_core::error::Result<Vec<atlas_control::Cause>>,
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
mod tests;
