//! Controller-side observations and enrichment for the doctor report.
use atlas::client::Client;
use serde_json::{Value, json};

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

    report
}
