//! What doctor sees of the user desktop.
use crate::doctor::{Observed, sound_warning};
use atlas_client::{client::Client, console_agents};
use serde_json::{Value, json};

/// What doctor sees of the second login `login`, with what this Mac can
/// tell of it: its account, why it is silent, the logins Atlas can close
/// and the Apple agents restarting in it.
pub(crate) async fn user_report(client: &Client, login: &atlas_core::user::User) -> Observed {
    let uid = login.uid;
    let status = client.status().await;
    let account = atlas_core::account::account_with_uid(uid).ok().flatten();

    // Not answering because the helper is logged out or gone: only the
    // person can log it in again.
    let absence = if atlas_core::session::silent(&status) {
        tokio::task::spawn_blocking(move || atlas_core::user::logins::absence(uid))
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
        tokio::task::spawn_blocking(move || {
            atlas_client::user::console_agents::read_restarting(uid)
        })
        .await
        .ok()
        .flatten()
    };
    let mut report = atlas_client::user::capability::doctor(status, login);
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
    let sessions =
        tokio::task::spawn_blocking(move || atlas_core::user::logins::sessions(Some(uid)))
            .await
            .ok()
            .flatten();
    let leftover = sessions.as_ref().map(|sessions| {
        sessions
            .leftover_uids(atlas_core::user::logins::Kind::Orphan)
            .into_iter()
            .chain(sessions.leftover_uids(atlas_core::user::logins::Kind::HelperLeftover))
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
        match atlas_core::user::account::administrator(&account.name) {
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

    Observed {
        report,
        first_steps,
        shell_blocked,
        restarts,
    }
}

/// What the second login `login` can open in the person's home folder, for
/// the person, never in an agent's compact reply.
pub(crate) async fn user_files(login: &atlas_core::user::User, report: &mut Value) {
    let uid = login.uid;
    if let Ok(Ok(files)) =
        tokio::task::spawn_blocking(move || atlas_client::user::home_privacy::status(uid)).await
    {
        report["files"] = json!(files);
    }
}
