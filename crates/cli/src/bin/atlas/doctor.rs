//! Controller-side observations and enrichment for the doctor report.
use atlas_cli::ui;
use atlas_client::{
    client::{Actor, Client},
    host_session::{Asker, Recovery},
};
use atlas_core::host::{HostSession, Key};
use serde_json::{Value, json};

/// Doctor's check that no change's outcome is unknown: while one is, Atlas
/// refuses every agent's action until the helper is recovered.
const NO_UNKNOWN_OUTCOME: &str = "No action of unknown outcome";

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
    let mut report = atlas_client::capability::doctor(status, uid);

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
    pending_report(&mut report, causes, asking);
    if verbose || ui::human() {
        report
    } else {
        atlas_client::capability::summary(report)
    }
}

/// Warn once the helper's screen recorder holds more than this. macOS keeps
/// part of every capture it serves there until the helper logs out.
const RECORDER_LIMIT: u64 = 1 << 30;

/// Warn the person when the helper's screen recorder, as its status says,
/// uses more memory than [`RECORDER_LIMIT`]. Only the person can log the
/// helper out, so an agent is not told, verbose or not; the figure itself
/// stays in a verbose report's status.
fn recorder_report(report: &mut Value, asking: Asker<'_>) {
    if asking != Asker::Person {
        return;
    }
    let Some(bytes) = report["session"]["daemon"]["resources"]["screenRecorder"]["footprint"]
        .as_u64()
        .filter(|bytes| *bytes > RECORDER_LIMIT)
    else {
        return;
    };
    let warning = format!(
        "The helper's screen recorder is using {:.1} GB of memory. Log the helper out and back \
         in to reset it.",
        bytes as f64 / (1_u64 << 30) as f64
    );
    if let Some(warnings) = report["warnings"].as_array_mut() {
        warnings.push(json!(warning));
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
