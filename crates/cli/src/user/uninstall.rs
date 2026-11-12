//! The user desktop's part of `atlas uninstall`: its account steps.
//! Uninstall's order, control and report are `crate::uninstall`'s; it calls
//! each step here where it is done, with what the step needs.
mod log_out;

use crate::uninstall::{
    Progress, Question, inspection::Found, problem, remove_link_to, remove_own_empty_folder,
    remove_own_file,
};
use atlas_client::{client::Client, setup};
use atlas_control::{Controller, Ownership};
use atlas_core::{account, config::Config, desktop::Desktop, error::Result};
use atlas_daemon::identity::{Identity, Standing};
use atlas_native::approval::Refusal;
use log_out::{log_out_helper, logged_out_twice, retiring};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Stop the helper's job as the helper (`helper`, as recorded), outside the
/// daemon it would stop:
/// inspected first and retired as it is, as installing it does, whichever
/// Atlas wrote its definition. A one-time opener still waiting there goes
/// too, since it could never run again once Atlas.app is gone. None when
/// the helper account is gone, and its job with it. sudo may ask the person
/// for their password.
/// Some with what taking the opener away came to: whether one was there and
/// is gone now, or why it stays.
fn retire_helper_job(
    executable: &Path,
    helper: &Identity,
    controller: &Controller,
) -> Result<Option<std::result::Result<bool, String>>> {
    let Some(account) = account::account_with_uid(helper.uid)? else {
        return Ok(None);
    };

    // As the helper but outside any login of it: it is logged out by now,
    // and reaching into its login would start one again. Each request is
    // checked again by the helper itself once sudo let it run: still the
    // account recorded, and still logged out.
    let ask = |request: &atlas_daemon::user::setup::Request, failed: &str| -> Result<Value> {
        retirement_request(controller, request, || {
            atlas_client::user::setup::request_as_helper(
                executable,
                &account,
                request,
                atlas_client::user::setup::Invocation::LoggedOut,
                helper,
            )?
            .map_err(|unfinished| {
                let said = match &unfinished {
                    atlas_daemon::user::setup::Unfinished::Silent(_) => failed.to_owned(),
                    atlas_daemon::user::setup::Unfinished::Unchanged(why)
                    | atlas_daemon::user::setup::Unfinished::Uncertain(why) => {
                        format!("{failed}: {}", why.as_deref().unwrap_or("unknown"))
                    }
                };
                unfinished.behind(problem(said))
            })
        })
    };
    let stopping = "The helper job could not be stopped";

    let inspected = ask(
        &atlas_daemon::user::setup::Request::Inspect {
            uid: helper.uid,
            labels: vec![atlas_daemon::user::setup::LABEL.to_owned()],
        },
        stopping,
    )?;

    // The definition there now, else (none there) the one this Atlas
    // would write, whose backup a retired job leaves.
    let sha256 = atlas_daemon::user::setup::JobReport::from(
        &inspected["jobs"][atlas_daemon::user::setup::LABEL],
    )
    .sha256
    .unwrap_or_else(|| {
        let layout = atlas_daemon::user::setup::Layout::new(helper.uid, account.home.clone());
        let controller = nix::unistd::getuid().as_raw();
        atlas_daemon::user::setup::Agent::managed(&layout, executable.to_owned(), controller)
            .plist_sha256(&layout.paths().logs())
    });

    let opener = ask(
        &atlas_daemon::user::setup::Request::ForgetOpener { uid: helper.uid },
        "Atlas's one-time start in the helper was not removed",
    )
    .map(|reply| reply["removed"] == true)
    .map_err(|error| error.error);

    ask(
        &atlas_daemon::user::setup::Request::Retire {
            uid: helper.uid,
            label: atlas_daemon::user::setup::LABEL.to_owned(),
            sha256,
            missing_ok: true,
        },
        stopping,
    )?;
    Ok(Some(opener))
}

/// Claim every retiring worker mutation before dispatch; inspection changes
/// nothing. A failed or lost reply keeps its own generation pending.
fn retirement_request(
    controller: &Controller,
    request: &atlas_daemon::user::setup::Request,
    ask: impl FnOnce() -> Result<Value>,
) -> Result<Value> {
    if request.requires_claim() {
        setup::executor::setup_transaction_held(controller, ask)
    } else {
        ask()
    }
}

/// Whether the helper `found` names is seen logged out now: no loginwindow
/// and no Dock, twice half a second apart. A look that fails is not.
pub(crate) fn logged_out(found: &Found) -> bool {
    found
        .client
        .as_ref()
        .is_some_and(|client| logged_out_twice(client.desktop().user().uid))
}

/// Log the helper `found` names out, under `exclusion`, as
/// [`log_out_helper`] does: root closes the login of the helper as
/// recorded, or of its user ID once its account is gone.
pub(crate) async fn log_out(
    found: &Found,
    exclusion: Option<&Ownership>,
    yes: bool,
    ask: &dyn Fn(Question) -> Result<bool>,
    progress: &dyn Fn(Progress<'_>),
) -> Result<()> {
    let (Some(client), Some(ownership)) = (&found.client, exclusion) else {
        return Err(problem("Atlas lost control of the helper"));
    };
    let target = match (&found.helper, retiring(found.known.as_ref())) {
        (None, _) => Some(crate::user::onboarding::as_root::Target::Orphan(
            client.desktop().user().uid,
        )),
        (Some(_), Some(identity)) => Some(crate::user::onboarding::as_root::Target::Helper(
            identity.clone(),
        )),
        (Some(_), None) => None,
    };

    log_out_helper(client, ownership, yes, target, ask, progress).await
}

/// Retire the job of the helper `found` names, under `exclusion`, set its
/// setup record aside and take away the shared command's link, saying what
/// went in `removed` and what did not in `warnings`.
pub(crate) fn retire(
    found: &Found,
    exclusion: Option<&Ownership>,
    removed: &mut Vec<Value>,
    warnings: &mut Vec<String>,
    progress: &dyn Fn(Progress<'_>),
) -> Result<()> {
    let Some(client) = &found.client else {
        return Ok(());
    };
    let job = retiring(found.known.as_ref());
    let _ownership = exclusion.ok_or_else(|| problem("Atlas lost control of the helper"))?;

    let config = &client.config;
    if let Some(helper) = job {
        progress(Progress::Say(
            "Stopping the helper's background job (macOS may ask for your password)…",
        ));
        if let Some(opener) = retire_helper_job(&found.executable, helper, &client.controller)? {
            removed.push(json!("helper job"));
            progress(Progress::Done("Helper job", "stopped"));
            match opener {
                Ok(true) => removed.push(json!("Atlas's one-time start in the helper")),
                Ok(false) => {}
                Err(why) => warnings.push(why),
            }
        } else {
            progress(Progress::Skipped(
                "Helper job",
                "the helper account no longer exists",
            ));
            warnings.push(format!(
                "The helper account (user ID {}) no longer exists, so no job was stopped",
                config.desktop().user().uid
            ));
        }
    }
    // Archive evidence only after the job is gone; it must stop counting as setup.
    if let Err(error) =
        atlas_client::user::setup::migrate::archive_uninstalled(config.desktop().user().uid)
    {
        warnings.push(format!(
            "The setup record was not set aside: {}",
            error.error
        ));
    }
    if remove_link_to(&config.identity_probe, &found.executable)? {
        removed.push(json!(config.identity_probe));
    }
    Ok(())
}

/// Delete the account of the helper `found` names, under `exclusion`, or
/// keep it and say why; `deleted` says it went.
pub(crate) fn remove_account(
    found: &Found,
    exclusion: Option<&Ownership>,
    removed: &mut Vec<Value>,
    warnings: &mut Vec<String>,
    deleted: &mut bool,
    progress: &dyn Fn(Progress<'_>),
) {
    if found.helper.is_none() {
        return;
    }
    let Some(identity) = retiring(found.known.as_ref()).cloned() else {
        kept_account(
            found,
            warnings,
            progress,
            "Atlas can't tell it is still the helper",
        );
        return;
    };

    let (Some(client), Some(_ownership)) = (&found.client, exclusion) else {
        warnings.push("The helper account was kept: Atlas lost control of it".into());
        return;
    };
    let uid = identity.uid;
    if uid == nix::unistd::getuid().as_raw()
        || atlas_core::user::logins::console_owner() == Some(uid)
    {
        kept_account(found, warnings, progress, "it is still logged in");
        return;
    }

    progress(Progress::Say(
        "Deleting the helper account (macOS may ask for your password)…",
    ));
    let name = &identity.name;
    match setup::executor::setup_transaction_held(&client.controller, || {
        crate::user::onboarding::as_root::remove_helper(
            &identity,
            crate::user::onboarding::as_root::Via::Sudo { ask: true },
        )
        .map_err(|refusal| {
            problem(match refusal {
                Refusal::Failed(why) => why,
                Refusal::Cancelled | Refusal::Unavailable => {
                    "administrator rights were not granted".into()
                }
            })
        })
    }) {
        Ok(()) => {
            removed.push(json!(format!("helper account {name}")));
            *deleted = true;
            progress(Progress::Done("Helper account", &format!("{name} deleted")));
        }
        Err(error) => {
            let why = error.error;
            progress(Progress::Skipped(
                "Helper account",
                &format!("deletion not confirmed: {why}"),
            ));
            warnings.push(format!(
                "Deletion of the helper account {name} was not confirmed: {why}. Its outcome \
                 stays pending until Atlas recovers it."
            ));
        }
    }
}

/// Say that the account of the helper `found` names was kept, and `why`.
pub(crate) fn kept_account(
    found: &Found,
    warnings: &mut Vec<String>,
    progress: &dyn Fn(Progress<'_>),
    why: &str,
) {
    if let Some(name) = &found.helper {
        progress(Progress::Skipped("Helper account", &format!("kept: {why}")));
        warnings.push(format!("The helper account {name} was kept: {why}"));
    }
}

/// Take away the one-time start left in an account this account asked for
/// but never finished setting up, other than the helper `found` names. What
/// went is said in `removed`, what did not in `warnings`.
pub(crate) fn forget_requested_opener(
    found: &Found,
    removed: &mut Vec<Value>,
    warnings: &mut Vec<String>,
) {
    let me = nix::unistd::getuid().as_raw();
    let client = found.client.as_ref();
    let Some(pairing) = atlas_daemon::user::setup::pairing::request().filter(|pairing| {
        pairing.controller_uid == me
            && client.is_none_or(|client| client.desktop().user().uid != pairing.helper_uid)
    }) else {
        return;
    };
    let Ok(Some(account)) = account::account_with_uid(pairing.helper_uid) else {
        return;
    };

    let request = atlas_daemon::user::setup::Request::ForgetOpener { uid: account.uid };
    let Some(expected) = atlas_daemon::user::setup::identity::verified(account.uid) else {
        warnings.push("Atlas's one-time start was left in place: choose the helper account explicitly before removing it".into());
        return;
    };
    let desktop = Desktop::from(atlas_core::user::User { uid: account.uid });
    let outcome = Controller::new(&desktop).and_then(|controller| {
        setup::executor::setup_transaction(&controller, || {
            atlas_client::user::setup::request_as_helper(
                &found.executable,
                &account,
                &request,
                atlas_client::user::setup::Invocation::Login,
                &expected,
            )?
            .map_err(|unfinished| {
                let error = problem(unfinished.reason().unwrap_or("no reply"));
                unfinished.behind(error)
            })
        })
    });
    match outcome {
        Ok(reply) => {
            if reply["removed"] == true {
                removed.push(json!(format!("Atlas's one-time start in {}", account.name)));
            }
        }
        Err(error) => warnings.push(format!(
            "Atlas's one-time start in {} was not removed: {}",
            account.name, error.error
        )),
    }
}

/// What setting the helper up left in the shared folder, named in `removed`
/// as it goes: the request naming the helper, when this account made it,
/// and the shared folder itself, once nothing but Atlas's is left in it
/// (another account could not set Atlas up while it is there).
pub(crate) fn remove_pairing_leftovers(removed: &mut Vec<Value>) {
    let request = Path::new(atlas_daemon::user::setup::pairing::REQUEST);
    if remove_own_file(request).unwrap_or(false) {
        removed.push(json!(request));
    }

    if let Some(shared) = Path::new(atlas_client::user::copies::ROOT).parent()
        && remove_own_empty_folder(shared)
    {
        removed.push(json!(shared));
    }
}

/// What settling needs to know of the second login `login`: whether its
/// account exists, whether it is logged in, and its input service's
/// process, as its status reports it.
pub(crate) async fn observed(
    client: &Client,
    login: &atlas_core::user::User,
) -> Result<(bool, Option<bool>, Option<u32>)> {
    let uid = login.uid;
    let exists = account::account_with_uid(uid)?.is_some();
    let status = client.status().await;
    let pid = atlas_client::user::setup::reported_pid(&status["inputService"]["pid"]);
    Ok((exists, atlas_core::user::logins::logged_in(uid), pid))
}

/// What uninstall says while the home folder is private from the helper
/// `name`, its entry at `index` (unreadable: perhaps): it stays, as only the person opens the home
/// folder, and how to open it. Once the account is deleted its name no longer
/// works in `chmod`, so the entry's place is given too.
pub(crate) fn home_note(
    name: &str,
    index: Result<Option<usize>>,
    remove_helper: bool,
) -> Vec<String> {
    let undo = atlas_client::user::home_privacy::undo_line(name);
    let index = match index {
        Ok(None) => return Vec::new(),
        Ok(Some(index)) => index,
        // Unreadable: it may be set, so say how to undo it, by name.
        Err(_) => {
            return vec![format!(
                "If your home folder is private from the helper, run this to open it again: {undo}"
            )];
        }
    };
    let mut lines = vec![
        "Your home folder stays private from the helper.".to_owned(),
        format!("To let it open your home folder again, run: {undo}"),
    ];
    if remove_helper {
        lines.push(format!(
            "Once the helper is deleted, run instead: chmod -a# {index} ~ (check with ls -le ~ first)"
        ));
    }
    lines
}

/// What Atlas left for the helper: the copies `open` made, and environment
/// plans and requests beside the shared command. Only folders that are
/// provably Atlas's own are listed.
pub(crate) fn handed_over(shared_command: &Path) -> Vec<PathBuf> {
    let mut leftovers: Vec<PathBuf> = atlas_client::user::copies::root_if_owned()
        .into_iter()
        .collect();

    if let Some(shared) = shared_command.parent() {
        leftovers.extend(atlas_client::user::environment::stage::leftovers(shared));
    }

    leftovers
}

/// The account of the helper `config` names: its name, when it still exists
/// and is not known to be another account, and whether it is the one
/// recorded (`None` without an account).
pub(crate) fn helper_account(config: &Config) -> (Option<String>, Option<Standing>) {
    let account = account::account_with_uid(config.desktop().user().uid)
        .ok()
        .flatten();
    let known = account
        .as_ref()
        .map(atlas_daemon::user::setup::identity::standing_of);
    let helper = account
        .filter(|_| !matches!(known, Some(Standing::Replaced | Standing::Unknown)))
        .map(|account| account.name);
    (helper, known)
}

#[cfg(test)]
#[path = "uninstall_tests.rs"]
mod tests;
