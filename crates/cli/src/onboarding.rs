//! The steps the Atlas window's onboarding drives, as `atlas onboarding …`
//! commands that answer in JSON. They share `atlas init`'s own helpers, so
//! the terminal and the window set Atlas up the same way, and each step
//! checks before it acts.
//!
//! In the person's account the window chooses or creates the helper and
//! finishes; the helper's Allow records acceptance and opens an unprivileged
//! worker connection. The requesting Atlas holds its own control state while
//! driving setup through that connection. Passwords
//! are never stored, logged or put in a command line: the one Atlas handles
//! (a new account's) goes from the window, through a pipe, straight into
//! macOS's own password prompt.
mod acceptance;
mod status;
pub use status::status;
pub(crate) mod as_root;
pub use acceptance::accept_worker;

use as_root::{Exit, Target};
use atlas_client::{
    client::Client,
    setup::{
        self,
        account::problem as init_problem,
        executor::{APPROVAL, Secret, answer_prompts, problem, read_secret, setup_transaction},
    },
};
use atlas_control::{Controller, Ownership};
use atlas_core::{
    account, app_bundle,
    app_bundle::signed_executable,
    config::{self, Config},
    desktop::Desktop,
    error::{Code, Error, Result},
};
use atlas_daemon::{
    helper_setup,
    helper_setup::{
        Agent, LABEL, Layout,
        identity::Identity,
        pairing::{
            MAX_RECORD, Pairing, REQUEST, WINDOW_MARKER, read_request, read_text, request,
            write_request,
        },
    },
};
use atlas_native::approval::Refusal;
use serde_json::{Value, json};
use std::{fs, os::unix::fs::MetadataExt, path::Path, time::Duration};

const NOT_CREATED: &str = "The account was not created. Approve the macOS dialog to create it.";

/// This account as directory services record it. Its home is not `$HOME`
/// (see [`config::home`]): the helper's layout and launchd job live there,
/// and the window that runs these steps may have any environment.
fn this_account() -> Result<account::Account> {
    account::account_with_uid(nix::unistd::getuid().as_raw())?
        .ok_or_else(|| problem("This account cannot be resolved"))
}

fn settings(config_path: &Path) -> Result<Option<Config>> {
    match Config::load(config_path) {
        Ok(settings) => Ok(Some(settings)),
        Err(error) if error.code == Code::Config => Ok(None),
        Err(error) => Err(error),
    }
}

/// Whether this account (`controller`) asked `helper` to be its helper: its
/// request names it, or a setup with it (not an earlier account with its
/// user ID) is recorded.
fn asked(controller: u32, helper: &account::Account) -> bool {
    requested(controller, helper) || setup::migrate::in_effect_for(helper.uid, &helper.home)
}

/// Whether the shared request is this account's (`controller`), naming
/// `helper`.
fn requested(controller: u32, helper: &account::Account) -> bool {
    request().is_some_and(|pairing| {
        pairing.controller_uid == controller
            && pairing.helper_uid == helper.uid
            && pairing.helper_name == helper.name
    })
}

/// What is known of a shared request when it is shown.
#[derive(Clone, Copy, Debug, Default)]
struct Asked {
    requested: bool,
    recorded: bool,
    answered: bool,
    prepared: bool,
}

impl Asked {
    /// Whether the request may be taken back: this account's own, for a
    /// helper that never answered, was never prepared for it and has no
    /// setup recorded. A real setup is never undone this way.
    fn may_forget(self) -> bool {
        self.requested && !self.recorded && !self.answered && !self.prepared
    }
}

/// Whether `helper`'s Atlas folder was prepared for this account, as `atlas
/// init` does just before installing the helper's background job there.
/// Only the account it was prepared for may look inside it.
fn prepared(helper: &account::Account) -> bool {
    let layout = Layout::new(helper.uid, helper.home.clone());

    fs::metadata(layout.paths().daemon()).is_ok_and(|metadata| metadata.is_dir())
}

/// Whether something answers at the helper socket `config` names. Checked
/// without waiting: a helper never asked has no socket at all.
fn answers(config: &Config) -> bool {
    std::os::unix::net::UnixStream::connect(&config.socket_path).is_ok()
}

/// Refuse to replace settings that name another helper still in use: one
/// this account asked, one that answers, or one prepared for this account
/// (an `atlas init` stopped once it may have installed the helper's job,
/// which replacing the settings would leave with nothing using it).
/// Settings for an account that is gone, or one never asked that is silent
/// (an `atlas init` stopped earlier), give way, as the window offers the
/// choice again then.
fn keep_other_helper(uid: u32, previous: &Config) -> Result<()> {
    match account::account_with_uid(previous.desktop().user().uid)? {
        Some(other) if asked(uid, &other) || answers(previous) || prepared(&other) => {
            Err(problem(format!(
                "Atlas already uses {} as its helper; choose {} again",
                other.name, other.name
            )))
        }
        _ => Ok(()),
    }
}

/// What choosing any helper needs that does not depend on the account:
/// this Atlas.app signed by its developer, settings that name no other
/// helper in use, and `atlas` in a terminal.
fn ready_to_choose(config_path: &Path, uid: u32, selected: Option<u32>) -> Result<()> {
    let executable = signed_executable()?;

    if let Some(previous) = settings(config_path)? {
        check_choice(uid, selected, &previous, keep_other_helper)?;
    }

    setup::installed_app::link_command(&executable)?;
    Ok(())
}

/// Choosing the configured account again repairs its setup. Creating an
/// account, or selecting another one, must first protect the existing helper.
fn check_choice(
    controller: u32,
    selected: Option<u32>,
    previous: &Config,
    keep: impl FnOnce(u32, &Config) -> Result<()>,
) -> Result<()> {
    if selected == Some(previous.desktop().user().uid) {
        Ok(())
    } else {
        keep(controller, previous)
    }
}

/// `name`, an existing standard account other than `controller`'s, that
/// no deleted account's session still holds, with a user ID from 501 up and
/// a home of its own in /Users: one that may be the helper.
fn choosable(name: &str, controller: u32) -> Result<account::Account> {
    let helper = account::account_named(name)?
        .ok_or_else(|| problem(format!("There is no account {name}")))?;
    if helper.uid == controller {
        return Err(problem("The helper must be another account than yours"));
    }

    match atlas_core::user::account::administrator(&helper.name) {
        Some(false) => {}
        Some(true) => {
            return Err(problem(format!(
                "{name} administers this Mac; the helper must be a standard account"
            )));
        }
        None => {
            return Err(problem(format!(
                "macOS could not say whether {name} is an administrator"
            )));
        }
    }

    if atlas_core::user::logins::sessions(Some(helper.uid)).is_some_and(|sessions| {
        !sessions
            .leftover_uids(atlas_core::user::logins::Kind::HelperLeftover)
            .is_empty()
    }) {
        return Err(problem(format!(
            "A deleted account is still logged in with {name}'s user ID. Restart the Mac, then \
             choose {name} again."
        )));
    }

    // A person's account: a user ID from 501 up, and a home in /Users that
    // is its own, when it has one yet.
    let home_ours = helper.home.parent() == Some(Path::new("/Users"))
        && fs::symlink_metadata(&helper.home).map_or(true, |metadata| {
            metadata.is_dir() && metadata.uid() == helper.uid
        });
    if helper.uid < account::FIRST_USER_UID || !home_ours {
        return Err(problem(format!(
            "{name} is not a person's account, so it cannot be the helper"
        )));
    }

    Ok(helper)
}

/// Make `name`, an existing standard account, the helper: readied for its
/// next login first (macOS asks for one approval, unless Atlas already
/// answers there for this account), then the shared executable, the request
/// Atlas in the helper reads, and the settings. Settings for another helper
/// in use are never replaced.
///
/// # Errors
///
/// Rejects an unsuitable or changed account and returns configuration or identity-record
/// failures.
pub fn choose(config_path: &Path, name: &str) -> Result<Value> {
    let uid = this_account()?.uid;
    let helper = choosable(name, uid)?;
    ready_to_choose(config_path, uid, Some(helper.uid))?;

    let expected = helper_setup::identity::current(&helper)
        .ok_or_else(|| problem("macOS could not identify the chosen helper account"))?;
    // Atlas already answering there needs nothing readied.
    let mut readied = true;
    let orphans = orphans_now()?;
    if !orphans.is_empty()
        || !setup::configuration::proposed(&helper).is_ok_and(|config| answers(&config))
    {
        // Root readies this account only while it is still the one checked.
        match closing_first(helper.uid, &orphans, || as_root::ready(&expected, &orphans))? {
            Ok(()) => {}
            // Without the approval call the account is chosen unreadied:
            // Atlas is opened there by hand, as the window then says.
            Err(Refusal::Unavailable) => readied = false,
            Err(Refusal::Cancelled) => {
                return Err(problem(format!(
                    "{name} was not chosen. Approve the macOS dialog to get it ready."
                )));
            }
            Err(Refusal::Failed(reason)) => {
                return Err(problem(format!("{name} was not chosen: {reason}")));
            }
        }
    }

    let mut chosen = use_helper(config_path, &helper, &expected)?;
    chosen["readied"] = json!(readied);
    Ok(chosen)
}

/// Make `helper` the helper: the shared executable, the request Atlas in
/// the helper reads, and the settings.
fn use_helper(
    config_path: &Path,
    helper: &account::Account,
    expected: &helper_setup::identity::Identity,
) -> Result<Value> {
    let me = this_account()?;
    let executable = signed_executable()?;
    let previous = settings(config_path)?;
    let fresh = setup::configuration::proposed(helper)?;

    // As `atlas init` does: the settings for this same helper stay (only
    // their sockets follow its home); another helper in use is kept.
    let config = match &previous {
        Some(previous) if previous.desktop().user().uid == helper.uid => {
            setup::configuration::in_home(previous, helper)
        }
        Some(previous) => {
            keep_other_helper(me.uid, previous)?;
            fresh
        }
        None => fresh,
    };

    let linked = setup::installed_app::link_command(&executable)?;

    // A setup record left by an earlier account with this user ID.
    setup::migrate::archive_foreign(helper.uid, &helper.home)?;

    // Which account this is, so nothing later acts on another account that
    // is given its user ID. Unrecorded, nothing later ends a login under
    // its user ID or deletes it.
    if helper_setup::identity::current(helper).as_ref() != Some(expected) {
        return Err(problem(
            "The chosen account changed while Atlas waited; choose it again",
        ));
    }
    helper_setup::identity::remember_as(helper, expected, true)?;

    // The shared executable (which also makes /Users/Shared/Atlas) and the
    // request first, the settings last: a choice cut short leaves no
    // settings that point at a helper that was never asked.
    Client::new(config.clone())?.bootstrap_probe()?;

    let _ownership = controller_of(helper.uid)?.acquire(true)?;
    write_request(
        Path::new(REQUEST),
        &Pairing {
            ticket: atlas_core::files::token(),
            controller_uid: me.uid,
            controller_name: me.name,
            helper_uid: helper.uid,
            helper_name: helper.name.clone(),
            helper_identity: expected.clone(),
        },
    )?;

    match previous {
        Some(previous) if previous == config => {}
        Some(previous) => setup::configuration::replace_config(config_path, &previous, &config)?,
        None => setup::configuration::write_config(config_path, &config)?,
    }

    Ok(json!({
        "ok": true,
        "helper": {"name": helper.name, "uid": helper.uid},
        "config": config_path,
        "command": linked,
    }))
}

/// The user IDs of deleted accounts whose logins are still open, which
/// creating or readying a helper closes first. Logins that cannot be listed
/// are never taken for none.
fn orphans_now() -> Result<Vec<u32>> {
    atlas_core::user::logins::sessions(None)
        .map(|sessions| sessions.leftover_uids(atlas_core::user::logins::Kind::Orphan))
        .ok_or_else(|| problem("macOS could not list its logins. Try again."))
}

/// Run `act`, the one run as root that closes the deleted accounts' logins
/// `orphans` before it makes or readies `helper_uid`. Every affected UID,
/// including the helper with no orphans, is held and claimed until setup
/// acknowledges completion and orphan cleanup is verified.
fn closing_first(
    helper_uid: u32,
    orphans: &[u32],
    act: impl FnOnce() -> std::result::Result<(), as_root::Unclosed>,
) -> Result<std::result::Result<(), Refusal>> {
    let gone = || orphans.is_empty() || atlas_core::user::logins::confirm_gone(orphans);
    closing_first_using(helper_uid, orphans, controller_of, act, gone)
}

fn closing_first_using(
    helper_uid: u32,
    orphans: &[u32],
    controller: impl Fn(u32) -> Result<Controller>,
    act: impl FnOnce() -> std::result::Result<(), as_root::Unclosed>,
    gone: impl FnOnce() -> bool,
) -> Result<std::result::Result<(), Refusal>> {
    let mut affected = orphans.to_vec();
    affected.push(helper_uid);
    setup_batch(&controllers_of(&affected, controller)?, act, gone)
}

/// Hold setup and its cleanup together. Orphan absence alone cannot settle
/// a failed setup that may already have changed the helper.
fn setup_batch(
    controllers: &[Controller],
    act: impl FnOnce() -> std::result::Result<(), as_root::Unclosed>,
    gone: impl FnOnce() -> bool,
) -> Result<std::result::Result<(), Refusal>> {
    let acknowledged = std::cell::Cell::new(false);
    match owned(
        controllers,
        || {
            let outcome = act();
            acknowledged.set(outcome.is_ok());
            outcome
        },
        || acknowledged.get() && gone(),
    ) {
        Ok(()) => Ok(Ok(())),
        Err(Held::Refused(refusal)) => Ok(Err(refusal)),
        Err(Held::State(error)) => Err(error),
    }
}

/// Create the account without macOS's approval call, through sysadminctl's
/// own dialog (`interactive`), which runs nothing as root that could close
/// a deleted account's login first: refused while one is open (`orphans`),
/// so no account is made under it.
fn without_approval<T>(orphans: &[u32], interactive: impl FnOnce() -> Result<T>) -> Result<T> {
    if !orphans.is_empty() {
        return Err(problem(format!(
            "A deleted account is still logged in, and macOS can't ask for your approval to \
             close it here, so no account was made. In Terminal, run: {}, then try again.",
            bootout_commands(orphans)
        )));
    }

    interactive()
}

fn bootout_commands(uids: &[u32]) -> String {
    uids.iter()
        .map(|uid| format!("sudo launchctl bootout user/{uid}"))
        .collect::<Vec<_>>()
        .join(" && ")
}

const CLOSE_PROMPT: &str = "Atlas will close a login left open by a deleted account.";

/// The logins deleted accounts left open, as root closes them: each
/// deleted account's login, and one still running under the user ID the
/// helper (`helper`, as recorded and still that account) was given after
/// it. Without a verified record of the helper, a login under its user ID
/// is never closed.
fn leftover_logins(
    sessions: &atlas_core::user::logins::Sessions,
    helper: Option<&Identity>,
) -> Vec<(u32, std::result::Result<Target, Refusal>)> {
    sessions
        .logins
        .iter()
        .filter_map(|(window, kind)| {
            let target = match kind {
                atlas_core::user::logins::Kind::Orphan => Ok(Target::Orphan(window.uid)),
                atlas_core::user::logins::Kind::HelperLeftover => match helper {
                    Some(helper) if helper.uid == window.uid => Ok(Target::Leftover {
                        helper: helper.clone(),
                        pid: window.pid,
                    }),
                    _ => Err(Refusal::Failed(
                        "Atlas can't tell that the account with the helper's user ID is still \
                         its helper"
                            .into(),
                    )),
                },
                atlas_core::user::logins::Kind::Mine
                | atlas_core::user::logins::Kind::OtherPerson
                | atlas_core::user::logins::Kind::Helper
                | atlas_core::user::logins::Kind::Picker => return None,
            };
            Some((window.uid, target))
        })
        .collect()
}

fn leftover_targets(
    sessions: &atlas_core::user::logins::Sessions,
    helper: Option<&Identity>,
) -> std::result::Result<Vec<Target>, Refusal> {
    leftover_logins(sessions, helper)
        .into_iter()
        .map(|(_, target)| target)
        .collect()
}

/// Close every login in `targets` with one run as root (`close`), so the
/// person approves once, then see them all gone (`gone`). Control of every
/// user ID the batch touches (`controllers`, one per user ID: any of them
/// may be a helper's) is held throughout, and the run is recorded as
/// pending with each before root is asked, as [`owned`] does. The user IDs
/// closed.
fn close_batch(
    controllers: &[Controller],
    targets: &[Target],
    close: impl FnOnce(&[Target]) -> std::result::Result<Vec<u32>, as_root::Unclosed>,
    gone: impl FnOnce(&[u32]) -> bool,
) -> std::result::Result<Vec<u32>, Held> {
    if targets.is_empty() {
        return Ok(Vec::new());
    }

    let uids: Vec<u32> = targets.iter().map(Target::uid).collect();
    owned(controllers, || close(targets), || gone(&uids))?;

    Ok(uids)
}

/// Close `targets` through `via` as [`close_batch`] does, holding the control
/// of each, with a refusal in `refused`'s words. The user IDs closed.
fn close_all(
    targets: &[Target],
    via: as_root::Via,
    refused: impl FnOnce(Refusal) -> Error,
) -> Result<Vec<u32>> {
    let uids: Vec<u32> = targets.iter().map(Target::uid).collect();
    close_batch(
        &controllers_of(&uids, controller_of)?,
        targets,
        |targets| as_root::close(targets, via),
        atlas_core::user::logins::confirm_gone,
    )
    .map_err(|held| held.into_error(refused))
}

/// The controllers of user IDs `uids`, one each: every user ID a run as
/// root closes a login of may be a helper's, whose control it must hold.
fn controllers_of(
    uids: &[u32],
    controller: impl Fn(u32) -> Result<Controller>,
) -> Result<Vec<Controller>> {
    let mut uids = uids.to_vec();
    uids.sort_unstable();
    uids.dedup();
    uids.into_iter().map(controller).collect()
}

/// The controller of the hidden login of user ID `uid`.
fn controller_of(uid: u32) -> Result<Controller> {
    Controller::new(&Desktop::from(atlas_core::user::User { uid }))
}

/// Create the selected standard account under claims for every affected UID.
/// Errors preserve incomplete creation and orphan cleanup for recovery.
///
/// # Errors
///
/// Rejects an invalid or conflicting account name and returns privilege, account-creation
/// or verification failures. Never retries an unknown creation outcome blindly.
pub fn create_helper(name: &str, full_name: &str) -> Result<account::Account> {
    let orphans = atlas_core::user::logins::sessions(None)
        .map(|sessions| sessions.leftover_uids(atlas_core::user::logins::Kind::Orphan))
        .ok_or_else(|| init_problem("macOS could not list its logins, so no account was made"))?;
    let uid = setup::account::free_uid()?;
    closing_first(uid, &orphans, || {
        as_root::create_in_terminal(name, uid, full_name, &orphans)
    })?
    .map_err(|refusal| {
        init_problem(format!(
            "The helper account could not be created: {refusal:?}"
        ))
    })?;

    account::account_named(name)?.ok_or_else(|| init_problem("The new account was not found"))
}

/// Close the logins deleted accounts left open, once the person agrees:
/// under sudo, which asks for their password here, all in one run as root.
/// Root checks each user ID again itself; only a login the helper's user ID
/// (`uid`, its account as just recorded: `helper`) was given after is closed
/// while its account exists, and then without killing anything.
///
/// # Errors
///
/// Returns identity, ownership or cleanup failures for the selected account's Atlas
/// resources. Unrelated or unverifiable resources are retained.
pub fn close_attended(
    uid: u32,
    helper: Option<&Identity>,
    approve: impl FnOnce() -> Result<()>,
) -> Result<bool> {
    let Some(sessions) = atlas_core::user::logins::sessions(Some(uid)) else {
        return Ok(false);
    };
    let still_open =
        || init_problem("It's still open. Close it before you switch: run atlas init again.");
    let failed = |refusal| match refusal {
        Refusal::Failed(reason) => init_problem(format!(
            "macOS didn't close it: {reason}. Restart the Mac to clear it, then run atlas init again."
        )),
        Refusal::Cancelled | Refusal::Unavailable => still_open(),
    };
    let targets = leftover_targets(&sessions, helper).map_err(failed)?;
    if targets.is_empty() {
        return Ok(false);
    }
    approve()?;
    close_all(&targets, as_root::Via::Sudo { ask: false }, failed)?;

    Ok(true)
}

/// Close the logins deleted accounts left open, the one the helper's user
/// ID was given after it among them, under the person's one approval; then
/// check, twice, that they are gone. Root checks each user ID again itself.
///
/// # Errors
///
/// Returns account-identity or leftover-cleanup failures. Unrelated or unverifiable
/// resources are not removed.
pub fn close_leftovers(config_path: &Path) -> Result<Value> {
    let helper = settings(config_path)?.map(|config| config.desktop().user().uid);
    let sessions = atlas_core::user::logins::sessions(helper)
        .ok_or_else(|| problem("macOS could not list its logins. Try again."))?;
    let identity = helper.and_then(helper_setup::identity::verified);
    let all: Vec<u32> = leftover_logins(&sessions, identity.as_ref())
        .iter()
        .map(|(uid, _)| *uid)
        .collect();
    let targets = leftover_targets(&sessions, identity.as_ref())
        .map_err(|refusal| not_closed(refusal, &all))?;

    let closed = close_all(&targets, as_root::Via::Approval(CLOSE_PROMPT), |refusal| {
        not_closed(refusal, &all)
    })?;

    Ok(json!({"ok": true, "closed": closed}))
}

/// Log out the helper whose Atlas is silent, as root under the person's
/// approval, through launchd and without killing anything: only the
/// configured helper, still the account recorded, logged in with its
/// desktop up for minutes, off screen, and with nothing answering at its
/// socket. Root checks it again itself. Control of the helper is held
/// throughout, so no other controller starts there meanwhile.
///
/// # Errors
///
/// Returns identity, active-work or helper-close failures. Unknown cleanup is retained
/// rather than acknowledged.
pub fn close_helper(config_path: &Path) -> Result<Value> {
    let config = Config::load(config_path)?;
    let client = Client::new(config.clone())?;
    let uid = config.desktop().user().uid;
    let me = nix::unistd::getuid().as_raw();
    let prompt = "Atlas will log out its helper, where Atlas isn't answering.";

    let checks = || {
        if uid == me || atlas_core::user::logins::console_owner() == Some(uid) {
            return Err(problem(
                "The helper is on screen. Switch back to your own account first.",
            ));
        }
        if answers(&config) {
            return Err(problem(
                "Atlas answers in the helper, so log it out with Log Out instead.",
            ));
        }
        if atlas_core::user::logins::stale(uid) != Some(true) {
            return Err(problem(
                "The helper isn't logged in, or is still logging in. Try again in a minute.",
            ));
        }

        helper_setup::identity::verified(uid).ok_or_else(|| {
            problem(
                "Atlas can't tell that the account with the helper's user ID is still its \
                 helper. Run atlas init, then try again.",
            )
        })
    };

    close_helper_held(
        &client.controller,
        checks,
        |helper| as_root::close(&[Target::Helper(helper)], as_root::Via::Approval(prompt)),
        || atlas_core::user::logins::confirm_gone(&[uid]),
    )
    .map_err(|held| held.into_error(|refusal| not_closed(refusal, &[uid])))?;

    Ok(json!({"ok": true, "closed": [uid]}))
}

/// Why a log-out as root under held control did not finish.
#[derive(Debug)]
pub(crate) enum Held {
    /// Control could not be taken, a check refused, or the claim could not
    /// be kept.
    State(Error),
    /// Root did not end the login, or it is not seen gone.
    Refused(Refusal),
}

impl From<Error> for Held {
    fn from(error: Error) -> Self {
        Self::State(error)
    }
}

impl Held {
    fn into_error(self, refused: impl FnOnce(Refusal) -> Error) -> Error {
        match self {
            Self::State(error) => error,
            Self::Refused(refusal) => refused(refusal),
        }
    }
}

/// [`close_helper`]'s order: take control of the helper, run `checks`
/// under it, then [`end_as_root`].
fn close_helper_held(
    controller: &Controller,
    checks: impl FnOnce() -> Result<Identity>,
    end: impl FnOnce(Identity) -> std::result::Result<Vec<u32>, as_root::Unclosed>,
    gone: impl FnOnce() -> bool,
) -> std::result::Result<(), Held> {
    // A log-out is the person's way out of an unknown outcome, as atlas
    // logout --force is: it may run with one pending.
    let ownership = controller.acquire(true)?;
    let helper = checks()?;

    end_as_root(controller, &ownership, || end(helper), gone)
}

/// End the helper's login as root (`end`) under control already held
/// (`_ownership`), as [`claimed`] records and settles it.
pub(crate) fn end_as_root(
    controller: &Controller,
    _ownership: &Ownership,
    end: impl FnOnce() -> std::result::Result<Vec<u32>, as_root::Unclosed>,
    gone: impl FnOnce() -> bool,
) -> std::result::Result<(), Held> {
    let mut verified = false;
    let outcome = claimed(&[controller], end, || {
        verified = gone();
        verified
    });
    if verified {
        Ok(())
    } else {
        outcome.map(|_| ())
    }
}

/// Take control of the helper each of `controllers` keeps (every user ID a
/// run as root may touch), then run `act` as [`claimed`] records and
/// settles it, all under that control.
fn owned<T>(
    controllers: &[Controller],
    act: impl FnOnce() -> std::result::Result<T, as_root::Unclosed>,
    gone: impl FnOnce() -> bool,
) -> std::result::Result<T, Held> {
    let _held = controllers
        .iter()
        .map(|controller| controller.acquire(true))
        .collect::<Result<Vec<_>>>()?;
    let controllers: Vec<&Controller> = controllers.iter().collect();

    claimed(&controllers, act, gone)
}

/// Run `act`, one run as root, recorded as pending with each of
/// `controllers` before root is asked, so a run cut short or a reply that
/// never came leaves it for recovery. The claims are settled only when root
/// stopped before it acted, or once root is known to have ended and the
/// logins it acted on are then seen gone (`gone`, which checks strictly).
/// While root may still be acting, the claims stay held by this process,
/// so recovery cannot settle them while it lasts.
fn claimed<T>(
    controllers: &[&Controller],
    act: impl FnOnce() -> std::result::Result<T, as_root::Unclosed>,
    gone: impl FnOnce() -> bool,
) -> std::result::Result<T, Held> {
    let claims = controllers
        .iter()
        .map(|controller| controller.quarantine())
        .collect::<Result<Vec<_>>>()?;
    let _executors = atlas_control::ExecutorClaims::new(&claims)?;
    let settle = |claims: &[atlas_control::Pending]| -> Result<()> {
        for (controller, claim) in controllers.iter().zip(claims) {
            controller.clear_if(claim)?;
        }
        Ok(())
    };

    match act() {
        Err(as_root::Unclosed {
            refusal,
            exit: Exit::Unknown,
        }) => {
            // No executor exit was established. Keep a live claim so even
            // recovery in this process must refuse further work.
            for claim in claims {
                std::mem::forget(claim);
            }
            Err(Held::Refused(refusal))
        }
        Err(as_root::Unclosed {
            refusal,
            exit: Exit::WithoutEffects,
        }) => {
            settle(&claims)?;
            Err(Held::Refused(refusal))
        }
        outcome @ (Ok(_)
        | Err(as_root::Unclosed {
            exit: Exit::AfterEffects,
            ..
        })) => {
            if gone() {
                settle(&claims)?;
                return outcome.map_err(|unclosed| Held::Refused(unclosed.refusal));
            }

            // Root may have acted, and a login is not seen gone: the outcome
            // stays pending until recovery.
            Err(Held::Refused(match outcome {
                Ok(_) => Refusal::Failed("it is still running".into()),
                Err(unclosed) => unclosed.refusal,
            }))
        }
    }
}

/// Why leftover logins `uids` were not closed, in the window's words.
fn not_closed(refusal: Refusal, uids: &[u32]) -> Error {
    problem(match refusal {
        Refusal::Cancelled => "It's still open. Close it before you switch.".to_owned(),
        Refusal::Failed(reason) => format!(
            "macOS didn't close it: {reason}. Restart the Mac to clear it, then open Atlas again."
        ),
        Refusal::Unavailable => format!(
            "macOS can't ask for your approval here. In Terminal, run: {}",
            bootout_commands(uids)
        ),
    })
}

/// Take back this account's request of the helper its settings name, so
/// the window can offer the choice again: only while that helper never
/// answered, was never prepared for this account and has no setup recorded.
/// The settings stay; choosing another account replaces them. No account is
/// deleted; the reply says whether the helper's login is still open.
///
/// # Errors
///
/// Rejects identity or retained-work conflicts and returns failures removing Atlas-owned
/// onboarding records.
pub fn forget(config_path: &Path) -> Result<Value> {
    let uid = this_account()?.uid;
    let config = Config::load(config_path)?;
    let _ownership = Controller::new(&config.desktop())?.acquire(true)?;
    let helper = account::account_with_uid(config.desktop().user().uid)?
        .ok_or_else(|| problem("The helper account no longer exists."))?;

    let set_up = setup::migrate::in_effect_for(helper.uid, &helper.home)
        || answers(&config)
        || prepared(&helper);
    let asked = requested(uid, &helper);

    if set_up {
        return Err(problem(format!(
            "Atlas is already set up in {}, so it stays the helper.",
            helper.name
        )));
    }

    // Nothing asked and nothing set up: there is nothing to give up.
    if !asked {
        return Ok(json!({"ok": true, "forgotten": null}));
    }

    match fs::remove_file(REQUEST) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }

    // Forgetting deletes no account. One whose login is still open must be
    // logged out before anyone deletes it, or its login lives on.
    let logged_in = atlas_core::user::logins::login(helper.uid).is_some();
    Ok(json!({"ok": true, "forgotten": helper.name, "loggedIn": logged_in}))
}

/// Create a standard account and make it the helper. Its password arrives
/// on standard input (from the window) and is typed into sysadminctl's own
/// prompt through a pseudo-terminal, so it is never in a command line or a
/// file. macOS asks for one approval, under which the account is also
/// readied for its first login (see `as_root`); on a macOS without that
/// approval, sysadminctl asks for it in its own dialog instead.
///
/// # Errors
///
/// Rejects invalid account names or an unsuitable account and returns privileged
/// account-creation or identity-record failures.
pub fn create(config_path: &Path, name: &str, full_name: &str) -> Result<Value> {
    setup::validate_account_name(name).map_err(problem)?;

    if full_name.is_empty()
        || full_name.chars().count() > 64
        || full_name.chars().any(|c| c.is_control() || c == '@')
        || full_name.starts_with('-')
    {
        return Err(problem(
            "Use a full name of 1 to 64 characters, without @, that does not start with -",
        ));
    }

    if account::account_named(name)?.is_some() {
        return Err(problem(format!("{name} already exists; choose it instead")));
    }

    // Choosing it checks these too, but only once macOS has made the
    // account: a refusal then would leave an account nothing uses.
    let uid = this_account()?.uid;
    ready_to_choose(config_path, uid, None)?;

    let password = Secret(read_secret(std::io::stdin())?);
    let next_uid = setup::account::free_uid()?;
    let mut readied = true;
    let orphans = orphans_now()?;
    let made = closing_first(next_uid, &orphans, || {
        as_root::create(name, next_uid, full_name, &password.0, &orphans)
    })?;
    let account = match made {
        Ok(()) => created(name)?,
        // Looked at again: a login may have been left open since.
        Err(Refusal::Unavailable) => setup_transaction(&controller_of(next_uid)?, || {
            without_approval(&orphans_now()?, || {
                readied = false;
                let mut args = setup::account::add_user_args(name, full_name, next_uid);
                args.push("interactive".to_owned());
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                answer_prompts(setup::account::SYSADMINCTL, &args, &password.0, APPROVAL)?;
                created(name)
            })
        })?,
        Err(Refusal::Cancelled) => return Err(problem(NOT_CREATED)),
        Err(Refusal::Failed(reason)) => {
            // Made, but not readied: it can be chosen again, which readies it.
            return Err(problem(match account::account_named(name)? {
                Some(_) => {
                    format!("{name} was created, but Atlas could not get it ready: {reason}")
                }
                None => format!("The account was not created: {reason}"),
            }));
        }
    };
    drop(password);

    if account.uid != next_uid {
        return Err(problem(format!(
            "An account named {name} appeared that Atlas did not create"
        )));
    }

    let account = choosable(&account.name, uid)?;
    let expected = helper_setup::identity::current(&account)
        .ok_or_else(|| problem("macOS could not identify the new account"))?;
    let chosen = use_helper(config_path, &account, &expected)?;
    Ok(json!({
        "ok": true,
        "created": {"name": account.name, "uid": account.uid},
        "chosen": chosen,
        "readied": readied,
    }))
}

/// The standard account `name` a creation made. sysadminctl reports success
/// even when it did nothing: the account itself is the answer.
fn created(name: &str) -> Result<account::Account> {
    let account = account::account_named(name)?.ok_or_else(|| problem(NOT_CREATED))?;

    if atlas_core::user::account::administrator(&account.name) != Some(false) {
        return Err(problem(format!(
            "{name} was created, but not as a standard account"
        )));
    }

    Ok(account)
}

/// In the helper: record Allow and open a setup worker, without authorization
/// or changing jobs. The requesting account drives setup. Only for the
/// account the person was shown (`controller_uid`, `controller_name`): a
/// request changed meanwhile is refused.
///
/// # Errors
///
/// Rejects an invalid controller identity and returns failures preparing or carrying out
/// the onboarding acceptance workflow.
pub fn accept(controller_uid: u32, controller_name: &str) -> Result<Value> {
    let me = this_account()?;

    acceptance::allow(
        (controller_uid, controller_name),
        (me.uid, &me.name),
        config::installed_path().is_ok_and(|path| path.exists()),
        atlas_core::user::account::administrator(&me.name),
        request(),
        acceptance::signal,
    )
}

fn expected_agent(layout: &Layout, controller_uid: u32) -> Result<Agent> {
    let program = app_bundle::app_executable()?;
    Ok(Agent::managed(layout, program, controller_uid))
}

/// Mark (`true`) or clear the onboarding window being open in this account.
///
/// # Errors
///
/// Returns failures resolving the current home or writing the open-window marker.
/// Clearing an absent or unreadable marker is best effort; this does not open a window.
pub fn window(open: bool) -> Result<Value> {
    let path = this_account()?
        .home
        .join(atlas_daemon::helper_paths::ROOT)
        .join(WINDOW_MARKER);

    if open {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&path, format!("{}\n", std::os::unix::process::parent_id()))?;
    } else {
        let _ = fs::remove_file(&path);
    }

    Ok(json!({"ok": true, "open": open}))
}

/// How long setup's end-to-end check waits for the person to be back in
/// their own account before it gives up.
const BACK_WITHIN: Duration = Duration::from_secs(60);

/// Back in the person's account: start input, record and check the setup
/// end to end, as `atlas init` does. Nothing starts until the person is
/// back in this login and settled there: while they are still unlocking
/// after the switch, this work would land on the switch itself.
///
/// # Errors
///
/// Returns configuration, helper-readiness, permission or finalization failures. Missing
/// consent or incomplete setup never counts as finished.
pub async fn finish(config_path: &Path) -> Result<Value> {
    let config = Config::load(config_path)?;
    let client = Client::new(config.clone())?;
    client.bootstrap_probe()?;
    ready_to_finish(&client.status().await, config.desktop().user().uid)?;
    atlas_client::switch::until_back(BACK_WITHIN).await?;
    // Places this check against the switch in a report's unified log.
    atlas_core::user::login_log::mark("onboarding.finish");
    ready_to_finish(&client.status().await, config.desktop().user().uid)?;

    if !client.controller.pending()?.is_empty() {
        client.restart().await?;
    }

    if let Some(helper) = account::account_with_uid(config.desktop().user().uid)? {
        setup::migrate::archive_foreign(helper.uid, &helper.home)?;
    }

    let doctor = setup::migrate::finalize(config_path, &client, Code::Onboarding).await?;
    if !doctor.ready {
        return Err(problem(
            "Atlas is set up but not ready yet. Try again in a moment.",
        ));
    }

    Ok(json!({"ok": true, "ready": true, "doctor": doctor.value}))
}

/// Whether `status` lets setup's end-to-end check go on: the helper `uid`
/// in the background with both permissions on. Read before the check waits
/// for the person and again after, since either may change meanwhile.
fn ready_to_finish(status: &Value, uid: u32) -> Result<()> {
    atlas_core::session::require_hidden(status, uid)?;
    if !setup::missing(status).is_empty() {
        return Err(problem(
            "Screen Recording or Accessibility is still off in the helper",
        ));
    }

    Ok(())
}

/// Open the menu bar at login, or not, as the person chose. As with `atlas
/// init`, only for the default settings, the ones the login item uses.
///
/// # Errors
///
/// Returns configuration, helper-identity or login-item update failures.
pub fn login(config_path: &Path, on: bool) -> Result<Value> {
    let default = || {
        config::installed_path()
            .is_ok_and(|default| atlas_agents::same_place(&default, config_path))
    };

    if on && !default() {
        return Err(problem(format!(
            "Open at Login uses the default settings; start the menu bar with atlas menubar \
             --config {}",
            config_path.display()
        )));
    }

    crate::menubar::set_login(if on {
        crate::menubar::Login::On
    } else {
        crate::menubar::Login::Off
    })
}

/// A change to the home folder's privacy from the helper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HomeChange {
    /// Shut the helper out of the home folder.
    Private,
    /// Let the helper open it again: the person only.
    Open,
    /// Change nothing, only remember that it was offered (Not Now).
    Offered,
}

/// Change how the home folder stands for the configured helper, or report
/// it, with whether it was offered and whether agents may copy sign-ins in.
///
/// # Errors
///
/// Returns configuration and identity errors, `stopped` when an agent asks
/// to open the home folder, and `io` with the Terminal line when a change
/// fails.
pub fn files(config_path: &Path, change: Option<HomeChange>, by_person: bool) -> Result<Value> {
    use atlas_client::home_privacy;

    let uid = Config::load(config_path)?.desktop().user().uid;
    let files = match change {
        None => home_privacy::status(uid)?,
        Some(HomeChange::Private) => home_privacy::make_private(uid)?,
        Some(HomeChange::Open) => home_privacy::open(uid, by_person)?,
        Some(HomeChange::Offered) => {
            home_privacy::mark_offered(uid);
            home_privacy::status(uid)?
        }
    };
    let mut reply = json!(files);
    reply["ok"] = json!(true);
    reply["offered"] = json!(home_privacy::offered_now(uid));
    reply["signInCopies"] = json!(atlas_client::sign_in_copies::allowed());
    Ok(reply)
}

/// Let agents copy sign-ins and secrets into the helper (`Some(true)`, the
/// person only), stop them (`Some(false)`), or say which it is.
///
/// # Errors
///
/// Returns `stopped` when an agent asks to allow copies, and file errors
/// when the choice can't be written.
pub fn sign_ins(allow: Option<bool>, by_person: bool) -> Result<Value> {
    let allowed = match allow {
        Some(allow) => atlas_client::sign_in_copies::set(allow, by_person)?,
        None => atlas_client::sign_in_copies::allowed(),
    };
    Ok(json!({"ok": true, "signInCopies": allowed}))
}

#[cfg(test)]
#[path = "onboarding_tests.rs"]
mod tests;
