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
pub(crate) mod as_root;
pub use acceptance::accept_worker;

use crate::{
    approval::Refusal,
    client::Client,
    config::{self, Config},
    control::{Controller, Ownership},
    error::{Error, Result},
    init,
    setup::{
        self,
        helper::{Agent, LABEL, Layout},
        identity::Identity,
    },
};
use as_root::Target;
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Where the person's account names the helper it chose, for Atlas in the
/// helper to find. It holds no secret; the helper's owner confirms it.
pub const REQUEST: &str = "/Users/Shared/Atlas/onboarding.json";

/// The onboarding window in the helper, while it is open: the helper's
/// daemon then leaves telling the person to the window.
pub const WINDOW_MARKER: &str = "onboarding-window";

/// How long a step under the person's approval may take, the approval
/// included.
const APPROVAL: Duration = Duration::from_secs(10 * 60);

fn problem(message: impl Into<String>) -> Error {
    Error::new("onboarding", message)
}

/// An account (the controller) asking another one to be its helper.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Pairing {
    /// A fresh request identity; old acceptance never grants a new request.
    pub ticket: String,
    pub controller_uid: u32,
    pub controller_name: String,
    pub helper_uid: u32,
    pub helper_name: String,
    /// The account chosen before any authorization or login wait.
    pub helper_identity: setup::identity::Identity,
}

/// Read the shared request, trusting it only when it is a plain file the
/// controller it names owns, in a folder no other account can change, and
/// both accounts it names are still the ones with those user IDs.
pub fn request() -> Option<Pairing> {
    read_request(Path::new(REQUEST)).filter(|pairing| {
        let named = |uid: u32, name: &str| {
            setup::account_with_uid(uid)
                .ok()
                .flatten()
                .is_some_and(|account| account.name == name)
        };

        named(pairing.controller_uid, &pairing.controller_name)
            && named(pairing.helper_uid, &pairing.helper_name)
    })
}

fn read_request(path: &Path) -> Option<Pairing> {
    let folder = fs::symlink_metadata(path.parent()?).ok()?;

    // Opened without following a link, and checked as opened.
    let mut file = crate::helper_info::open_plain(path)?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.mode() & 0o022 != 0 {
        return None;
    }

    let mut body = Vec::new();
    (&mut file)
        .take(64 * 1024 + 1)
        .read_to_end(&mut body)
        .ok()?;
    if body.len() > 64 * 1024 {
        return None;
    }
    let value: Value = serde_json::from_slice(&body).ok()?;
    let request = Pairing {
        ticket: match value.get("ticket") {
            Some(Value::String(ticket)) => ticket.clone(),
            None => setup::helper::text_sha256(&String::from_utf8_lossy(&body))
                [..crate::control::TOKEN_BYTES * 2]
                .to_owned(),
            _ => return None,
        },
        controller_uid: value["controllerUid"].as_u64()?.try_into().ok()?,
        controller_name: value["controllerName"].as_str()?.to_owned(),
        helper_uid: value["helperUid"].as_u64()?.try_into().ok()?,
        helper_name: value["helperName"].as_str()?.to_owned(),
        helper_identity: serde_json::from_value(value["helperIdentity"].clone()).ok()?,
    };
    if request.ticket.len() != crate::control::TOKEN_BYTES * 2
        || !request.ticket.bytes().all(|byte| byte.is_ascii_hexdigit())
        || request.helper_identity.uid != request.helper_uid
        || request.helper_identity.name != request.helper_name
    {
        return None;
    }
    let folder_safe = folder.is_dir()
        && (folder.uid() == request.controller_uid || folder.uid() == 0)
        && folder.mode() & 0o022 == 0;

    (folder_safe && metadata.uid() == request.controller_uid).then_some(request)
}

/// Write the request, readable by every account and changed only by this one.
fn write_request(path: &Path, request: &Pairing) -> Result<()> {
    let body = json!({
        "ticket": request.ticket,
        "controllerUid": request.controller_uid,
        "controllerName": request.controller_name,
        "helperUid": request.helper_uid,
        "helperName": request.helper_name,
        "helperIdentity": request.helper_identity,
    });

    config::write_atomic(path, serde_json::to_string_pretty(&body)?.as_bytes(), 0o644)
}

/// This account's user ID, short name and full name.
fn me() -> Result<(u32, String, Option<String>)> {
    let uid = nix::unistd::getuid().as_raw();
    let account =
        setup::account_with_uid(uid)?.ok_or_else(|| problem("This account cannot be resolved"))?;
    let real = setup::real_name(&account.name);

    Ok((uid, account.name, real))
}

/// This account's home as directory services record it, not `$HOME` (see
/// [`config::home`]): the helper's layout and launchd job live there, and
/// the window that runs these steps may have any environment.
fn home() -> Result<PathBuf> {
    let uid = nix::unistd::getuid().as_raw();
    Ok(setup::account_with_uid(uid)?
        .ok_or_else(|| problem("This account cannot be resolved"))?
        .home)
}

/// Whether Atlas is set up in this account as someone's helper already
/// (its background job is installed here, as `atlas init` leaves it).
fn helper_job_here(uid: u32) -> bool {
    home().is_ok_and(|home| Layout::new(uid, home).agent_plist(LABEL).exists())
}

/// The app this runs from, whether it is signed by Atlas's developer, and
/// whether `atlas` in a terminal reaches it.
fn app() -> Value {
    match setup::installed_app::app_executable() {
        Ok(executable) => {
            let link = config::home()
                .map(|home| home.join(".local/bin/atlas"))
                .ok();
            let linked = link
                .and_then(|link| fs::canonicalize(link).ok())
                .is_some_and(|target| target == executable);

            json!({
                "installed": true,
                "path": executable.parent().and_then(Path::parent).and_then(Path::parent),
                "signed": setup::developer_id(&executable),
                "linked": linked,
            })
        }
        Err(error) => json!({"installed": false, "error": error.error}),
    }
}

/// Everything the window needs to choose its step, read fresh each time.
pub async fn status(config_path: &Path) -> Result<Value> {
    let (uid, name, real) = me()?;
    let me = json!({"uid": uid, "name": name, "realName": real});
    let app = app();
    let absent = matches!(
        fs::symlink_metadata(config_path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    );

    // Atlas opened in an account another one chose as its helper (an
    // account with Atlas settings of its own is a controller, whatever a
    // shared file says).
    if absent {
        let pairing = request().filter(|pairing| pairing.helper_uid == uid);
        if pairing.is_some() || helper_job_here(uid) {
            return Ok(json!({
                "ok": true,
                "role": "helper",
                "me": me,
                "app": app,
                "helper": helper_side(uid, pairing.as_ref()).await,
            }));
        }

        let (leftovers, blockers) = logins(None);
        return Ok(json!({
            "ok": true,
            "role": "controller",
            "me": me,
            "app": app,
            "configured": false,
            "candidates": candidates()?,
            "menubarLogin": crate::menubar::login_decided(),
            "leftoverLogins": leftovers,
            "blockers": blockers,
        }));
    }

    let config = Config::load(config_path)?;
    let (leftovers, blockers) = logins(Some(config.helper_uid));
    let Some(helper) = setup::account_with_uid(config.helper_uid)? else {
        return Ok(json!({
            "ok": true,
            "role": "controller",
            "me": me,
            "app": app,
            "configured": true,
            "helper": {"exists": false, "uid": config.helper_uid},
            "candidates": candidates()?,
            "menubarLogin": crate::menubar::login_decided(),
            "leftoverLogins": leftovers,
            "blockers": blockers,
        }));
    };

    let admin = setup::administrator(&helper.name);

    // This account asked this helper: its request names it (the window), or
    // a setup with it is recorded (`atlas init`, or the window's finish).
    let paired = asked(uid, &helper);
    let signed_in = crate::session::logged_in(helper.uid) == Some(true);

    let client = Client::new(config.clone())?;
    let acceptance = if let Some(pairing) = request()
        .filter(|pairing| pairing.controller_uid == uid && pairing.helper_uid == helper.uid)
    {
        acceptance::observe(&config, pairing).await
    } else {
        json!(null)
    };
    let reply = client.status().await;
    let hidden = crate::session::require_hidden(&reply, config.helper_uid).is_ok();
    let answering = hidden || reply["daemon"]["pid"].is_number();
    let ready = crate::capability::doctor(reply.clone(), config.helper_uid)["ready"] == true;
    let finished =
        ready && setup::migrate::settled(helper.uid) && crate::session::input_build_current(&reply);

    // Never asked and silent (a choice cut short, or an `atlas init` stopped
    // once it wrote settings): the window offers the choice again, and
    // choosing another account replaces these settings. A helper already
    // prepared for this account is the only choice, as `choose` keeps it.
    let only_choice = !paired && !answering && prepared(&helper);
    let candidates = if paired || answering {
        Vec::new()
    } else if only_choice {
        candidates()?
            .into_iter()
            .filter(|candidate| candidate["uid"] == helper.uid)
            .collect()
    } else {
        candidates()?
    };

    // Asked by this window but never answered: the request may be taken
    // back, so another account can be chosen.
    let forgettable = may_forget(
        requested(uid, &helper),
        setup::migrate::in_effect_for(helper.uid, &helper.home),
        answering || answers(&config),
        prepared(&helper),
    );

    Ok(json!({
        "ok": true,
        "role": "controller",
        "me": me,
        "app": app,
        "configured": true,
        "candidates": candidates,
        "onlyChoice": only_choice,
        "helper": {
            "exists": true,
            "uid": helper.uid,
            "name": helper.name,
            "realName": setup::real_name(&helper.name),
            "administrator": admin,
            "paired": paired,
            "forgettable": forgettable,
            "signedIn": signed_in,
            "answering": answering,
            "onConsole": reply["session"]["onConsole"] == true,
            "hidden": hidden,
            "permissions": {
                "screenCapture": reply["permissions"]["screenCapture"] == true,
                "accessibility": reply["permissions"]["accessibility"] == true,
            },
        },
        "setupProblem": acceptance["error"].as_str().map(|error| format!("{error}. Open Atlas in the helper, then return here to retry. Any unfinished setup stays pending until recovery.")),
        "acceptance": acceptance,
        "finished": finished,
        "menubarLogin": crate::menubar::login_decided(),
        "leftoverLogins": leftovers,
        "blockers": blockers,
    }))
}

/// The logins of deleted accounts that are still open, and what would hold
/// up a new login now, as the window reads them; `helper` is the configured
/// helper. Both are empty when the process list could not be read.
fn logins(helper: Option<u32>) -> (Value, Value) {
    match crate::session::sessions(helper) {
        Some(sessions) => (json!(sessions.leftovers()), json!(sessions.blockers)),
        None => (json!([]), json!([])),
    }
}

/// The helper's side of the status: who asked (unknown when `atlas init`
/// set it up), whether its job runs here, and its permissions as that job,
/// the process that uses them, reports them.
async fn helper_side(uid: u32, pairing: Option<&Pairing>) -> Value {
    let layout = home().map(|home| Layout::new(uid, home));

    // Installed for the account asking now: a job left by an earlier Atlas
    // (another app, another account) needs Allow again.
    let installed = layout.as_ref().is_ok_and(|layout| {
        let current = read_text(&layout.agent_plist(LABEL), 64 * 1024);
        match (current, pairing) {
            (None, _) => false,
            (Some(_), None) => true,
            (Some(text), Some(pairing)) => expected_agent(layout, pairing.controller_uid)
                .is_ok_and(|agent| text == setup::helper::plist(&agent, &layout.logs())),
        }
    });
    let accepted = pairing.is_some_and(acceptance::accepted);
    // A lost acceptance process changes no jobs on its own. Reopening this
    // window can safely restore its listener using the same consent record.
    if accepted
        && !acceptance::listening(uid)
        && let Some(pairing) = pairing
    {
        let _ = acceptance::signal(pairing);
    }
    let running = layout
        .as_ref()
        .is_ok_and(|layout| setup::helper::job_pid(layout, LABEL).is_some());

    // macOS counts a process the window starts as the window's own, so only
    // the job's answer tells what Atlas itself may do here. Without one (not
    // running, or restarting to pick up Screen Recording) they are unknown,
    // not off.
    let mut hello = None;
    if let (true, Ok(layout)) = (running, &layout) {
        let transport = crate::transport::Transport {
            path: layout.socket(),
            timeout: Duration::from_secs(3),
            peer_uid: uid,
        };
        hello = transport.call(&json!({"op": "hello"})).await.ok();
    }
    let permissions = permissions_from(hello.as_ref());

    let controller = pairing.map_or(json!(null), |pairing| {
        json!({
            "uid": pairing.controller_uid,
            "name": pairing.controller_name,
            "realName": setup::real_name(&pairing.controller_name),
        })
    });

    json!({
        "controller": controller,
        "installed": installed,
        "accepted": accepted,
        "running": running,
        "permissions": permissions,
    })
}

/// The two permissions as the job's `hello` reports them, or unknown
/// (`null`) without one, never off.
fn permissions_from(hello: Option<&Value>) -> Value {
    hello.map_or(Value::Null, |hello| {
        json!({
            "screenCapture": hello["permissions"]["screenCapture"] == true,
            "accessibility": hello["permissions"]["accessibility"] == true,
        })
    })
}

/// Standard accounts that could be the helper.
fn candidates() -> Result<Vec<Value>> {
    Ok(crate::switch::standard_accounts()?
        .into_iter()
        .map(|account| {
            json!({"name": account.name, "uid": account.uid, "realName": account.real_name})
        })
        .collect())
}

/// The settings at `config_path`, when there are any.
fn settings(config_path: &Path) -> Result<Option<Config>> {
    match Config::load(config_path) {
        Ok(settings) => Ok(Some(settings)),
        Err(_) if !config_path.exists() => Ok(None),
        Err(error) => Err(error),
    }
}

/// Whether this account (`controller`) asked `helper` to be its helper: its
/// request names it, or a setup with it (not an earlier account with its
/// user ID) is recorded.
fn asked(controller: u32, helper: &setup::Account) -> bool {
    requested(controller, helper) || setup::migrate::in_effect_for(helper.uid, &helper.home)
}

/// Whether the shared request is this account's (`controller`), naming
/// `helper`.
fn requested(controller: u32, helper: &setup::Account) -> bool {
    request().is_some_and(|pairing| {
        pairing.controller_uid == controller
            && pairing.helper_uid == helper.uid
            && pairing.helper_name == helper.name
    })
}

/// Whether a request may be taken back: this account's own, for a helper
/// that never answered, was never prepared for it and has no setup
/// recorded. A real setup is never undone this way.
fn may_forget(requested: bool, recorded: bool, answered: bool, prepared: bool) -> bool {
    requested && !recorded && !answered && !prepared
}

/// Whether `helper`'s Atlas folder was prepared for this account, as `atlas
/// init` does just before installing the helper's background job there.
/// Only the account it was prepared for may look inside it.
fn prepared(helper: &setup::Account) -> bool {
    let layout = Layout {
        uid: helper.uid,
        home: helper.home.clone(),
        guard: None,
    };

    fs::metadata(layout.daemon()).is_ok_and(|metadata| metadata.is_dir())
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
    match setup::account_with_uid(previous.helper_uid)? {
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
    if selected == Some(previous.helper_uid) {
        Ok(())
    } else {
        keep(controller, previous)
    }
}

/// `name`, an existing standard account other than `controller`'s, that
/// no deleted account's session still holds, with a user ID from 501 up and
/// a home of its own in /Users: one that may be the helper.
fn choosable(name: &str, controller: u32) -> Result<setup::Account> {
    let helper = setup::account_named(name)?
        .ok_or_else(|| problem(format!("There is no account {name}")))?;
    if helper.uid == controller {
        return Err(problem("The helper must be another account than yours"));
    }

    match setup::administrator(&helper.name) {
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

    if crate::session::sessions(Some(helper.uid)).is_some_and(|sessions| {
        !sessions
            .leftover_uids(crate::session::Kind::HelperLeftover)
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
    if helper.uid < setup::FIRST_USER_UID || !home_ours {
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
pub fn choose(config_path: &Path, name: &str) -> Result<Value> {
    let (uid, _, _) = me()?;
    let helper = choosable(name, uid)?;
    ready_to_choose(config_path, uid, Some(helper.uid))?;

    let expected = setup::identity::current(&helper)
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
    helper: &setup::Account,
    expected: &setup::identity::Identity,
) -> Result<Value> {
    let (uid, me_name, _) = me()?;
    let executable = signed_executable()?;
    let previous = settings(config_path)?;
    let fresh = setup::configuration::proposed(helper)?;

    // As `atlas init` does: the settings for this same helper stay (only
    // their sockets follow its home); another helper in use is kept.
    let config = match &previous {
        Some(previous) if previous.helper_uid == helper.uid => {
            if previous.socket_path.starts_with(&helper.home) {
                previous.clone()
            } else {
                Config {
                    socket_path: fresh.socket_path.clone(),
                    input_socket: fresh.input_socket.clone(),
                    ..previous.clone()
                }
            }
        }
        Some(previous) => {
            keep_other_helper(uid, previous)?;
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
    if setup::identity::current(helper).as_ref() != Some(expected) {
        return Err(problem(
            "The chosen account changed while Atlas waited; choose it again",
        ));
    }
    setup::identity::remember_as(helper, expected, true)?;

    // The shared executable (which also makes /Users/Shared/Atlas) and the
    // request first, the settings last: a choice cut short leaves no
    // settings that point at a helper that was never asked.
    Client::new(config.clone())?.bootstrap_probe()?;

    let _ownership = Controller::new(helper.uid)?.acquire(true)?;
    write_request(
        Path::new(REQUEST),
        &Pairing {
            ticket: crate::control::token(),
            controller_uid: uid,
            controller_name: me_name,
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
    crate::session::sessions(None)
        .map(|sessions| sessions.leftover_uids(crate::session::Kind::Orphan))
        .ok_or_else(|| problem("macOS could not list its logins. Try again."))
}

/// Run `act`, the one run as root that closes the deleted accounts' logins
/// `orphans` before it makes or readies `helper_uid`. Every affected UID,
/// including the helper with no orphans, is held and claimed until setup
/// acknowledges completion and orphan cleanup is verified.
pub(crate) fn closing_first(
    helper_uid: u32,
    orphans: &[u32],
    act: impl FnOnce() -> std::result::Result<(), as_root::Unclosed>,
) -> Result<std::result::Result<(), Refusal>> {
    let gone = || orphans.is_empty() || crate::session::confirm_gone(orphans);
    closing_first_using(helper_uid, orphans, Controller::new, act, gone)
}

/// Select every affected controller before entering the setup transaction.
fn closing_first_using(
    helper_uid: u32,
    orphans: &[u32],
    controller: impl Fn(u32) -> Result<Controller>,
    act: impl FnOnce() -> std::result::Result<(), as_root::Unclosed>,
    gone: impl FnOnce() -> bool,
) -> Result<std::result::Result<(), Refusal>> {
    let mut affected = orphans.to_vec();
    affected.push(helper_uid);
    affected.sort_unstable();
    affected.dedup();
    let controllers = affected
        .into_iter()
        .map(controller)
        .collect::<Result<Vec<_>>>()?;
    setup_batch(&controllers, act, gone)
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
            orphans
                .iter()
                .map(|uid| format!("sudo launchctl bootout user/{uid}"))
                .collect::<Vec<_>>()
                .join(" && ")
        )));
    }

    interactive()
}

/// What macOS asks the person to approve when Atlas closes leftover logins.
const CLOSE_PROMPT: &str = "Atlas will close a login left open by a deleted account.";

/// The logins deleted accounts left open, as root closes them: each
/// deleted account's login, and one still running under the user ID the
/// helper (`helper`, as recorded and still that account) was given after
/// it. Without a verified record of the helper, a login under its user ID
/// is never closed.
pub(crate) fn leftover_targets(
    sessions: &crate::session::Sessions,
    helper: Option<&Identity>,
) -> std::result::Result<Vec<Target>, Refusal> {
    use crate::session::Kind;

    sessions
        .logins
        .iter()
        .filter_map(|(window, kind)| match kind {
            Kind::Orphan => Some(Ok(Target::Orphan(window.uid))),
            Kind::HelperLeftover => Some(match helper {
                Some(helper) if helper.uid == window.uid => Ok(Target::Leftover {
                    helper: helper.clone(),
                    pid: window.pid,
                }),
                _ => Err(Refusal::Failed(
                    "Atlas can't tell that the account with the helper's user ID is still its \
                     helper"
                        .into(),
                )),
            }),
            _ => None,
        })
        .collect()
}

/// Close every login in `targets` with one run as root (`close`), so the
/// person approves once, then see them all gone (`gone`). Control of every
/// user ID the batch touches (`controllers`, one per user ID: any of them
/// may be a helper's) is held throughout, and the run is recorded as
/// pending with each before root is asked, as [`owned`] does. The user IDs
/// closed.
pub(crate) fn close_batch(
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

/// The controllers of user IDs `uids`, one each: every user ID a run as
/// root closes a login of may be a helper's, whose control it must hold.
pub(crate) fn controllers_of(uids: &[u32]) -> Result<Vec<Controller>> {
    let mut uids = uids.to_vec();
    uids.sort_unstable();
    uids.dedup();
    uids.into_iter().map(Controller::new).collect()
}

/// Close the logins deleted accounts left open, the one the helper's user
/// ID was given after it among them, under the person's one approval; then
/// check, twice, that they are gone. Root checks each user ID again itself.
pub fn close_leftovers(config_path: &Path) -> Result<Value> {
    let helper = settings(config_path)?.map(|config| config.helper_uid);
    let sessions = crate::session::sessions(helper)
        .ok_or_else(|| problem("macOS could not list its logins. Try again."))?;
    let identity = helper.and_then(setup::identity::verified);
    let all: Vec<u32> = sessions
        .logins
        .iter()
        .filter(|(_, kind)| {
            matches!(
                kind,
                crate::session::Kind::Orphan | crate::session::Kind::HelperLeftover
            )
        })
        .map(|(window, _)| window.uid)
        .collect();
    let targets = leftover_targets(&sessions, identity.as_ref())
        .map_err(|refusal| not_closed(refusal, &all))?;

    let uids: Vec<u32> = targets.iter().map(Target::uid).collect();
    let closed = close_batch(
        &controllers_of(&uids)?,
        &targets,
        |targets| as_root::close(targets, as_root::Via::Approval(CLOSE_PROMPT)),
        crate::session::confirm_gone,
    )
    .map_err(|held| match held {
        Held::State(error) => error,
        Held::Refused(refusal) => not_closed(refusal, &all),
    })?;

    Ok(json!({"ok": true, "closed": closed}))
}

/// Log out the helper whose Atlas is silent, as root under the person's
/// approval, through launchd and without killing anything: only the
/// configured helper, still the account recorded, logged in with its
/// desktop up for minutes, off screen, and with nothing answering at its
/// socket. Root checks it again itself. Control of the helper is held
/// throughout, so no other controller starts there meanwhile.
pub fn close_helper(config_path: &Path) -> Result<Value> {
    let config = Config::load(config_path)?;
    let client = Client::new(config.clone())?;
    let uid = config.helper_uid;
    let me = nix::unistd::getuid().as_raw();
    let prompt = "Atlas will log out its helper, where Atlas isn't answering.";

    let checks = || {
        if uid == me || crate::session::console_owner() == Some(uid) {
            return Err(problem(
                "The helper is on screen. Switch back to your own account first.",
            ));
        }
        if answers(&config) {
            return Err(problem(
                "Atlas answers in the helper, so log it out with Log Out instead.",
            ));
        }
        if crate::session::stale(uid) != Some(true) {
            return Err(problem(
                "The helper isn't logged in, or is still logging in. Try again in a minute.",
            ));
        }

        setup::identity::verified(uid).ok_or_else(|| {
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
        || crate::session::confirm_gone(&[uid]),
    )
    .map_err(|error| match error {
        Held::State(error) => error,
        Held::Refused(refusal) => not_closed(refusal, &[uid]),
    })?;

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
pub(crate) fn owned<T>(
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
    let _executors = crate::control::ExecutorClaims::new(&claims)?;
    let settle = |claims: &[crate::control::Pending]| -> Result<()> {
        for (controller, claim) in controllers.iter().zip(claims) {
            controller.clear_if(claim)?;
        }
        Ok(())
    };

    match act() {
        Err(unclosed) if !unclosed.ended => {
            // No executor exit was established. Keep a live claim so even
            // recovery in this process must refuse further work.
            for claim in claims {
                std::mem::forget(claim);
            }
            Err(Held::Refused(unclosed.refusal))
        }
        Err(unclosed) if !unclosed.acted => {
            settle(&claims)?;
            Err(Held::Refused(unclosed.refusal))
        }
        outcome if gone() => {
            settle(&claims)?;
            outcome.map_err(|unclosed| Held::Refused(unclosed.refusal))
        }
        // Root may have acted, and a login is not seen gone: the outcome
        // stays pending until recovery.
        outcome => Err(Held::Refused(match outcome {
            Ok(_) => Refusal::Failed("it is still running".into()),
            Err(unclosed) => unclosed.refusal,
        })),
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
            uids.iter()
                .map(|uid| format!("sudo launchctl bootout user/{uid}"))
                .collect::<Vec<_>>()
                .join(" && ")
        ),
    })
}

/// Take back this account's request of the helper its settings name, so
/// the window can offer the choice again: only while that helper never
/// answered, was never prepared for this account and has no setup recorded.
/// The settings stay; choosing another account replaces them. No account is
/// deleted; the reply says whether the helper's login is still open.
pub fn forget(config_path: &Path) -> Result<Value> {
    let (uid, _, _) = me()?;
    let config = Config::load(config_path)?;
    let _ownership = Controller::new(config.helper_uid)?.acquire(true)?;
    let helper = setup::account_with_uid(config.helper_uid)?
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
    let logged_in = crate::session::login(helper.uid).is_some();
    Ok(json!({"ok": true, "forgotten": helper.name, "loggedIn": logged_in}))
}

/// Create a standard account and make it the helper. Its password arrives
/// on standard input (from the window) and is typed into sysadminctl's own
/// prompt through a pseudo-terminal, so it is never in a command line or a
/// file. macOS asks for one approval, under which the account is also
/// readied for its first login (see `as_root`); on a macOS without that
/// approval, sysadminctl asks for it in its own dialog instead.
pub fn create(config_path: &Path, name: &str, full_name: &str) -> Result<Value> {
    init::valid_account_name(name).map_err(problem)?;

    if full_name.is_empty()
        || full_name.chars().count() > 64
        || full_name.chars().any(|c| c.is_control() || c == '@')
        || full_name.starts_with('-')
    {
        return Err(problem(
            "Use a full name of 1 to 64 characters, without @, that does not start with -",
        ));
    }

    if setup::account_named(name)?.is_some() {
        return Err(problem(format!("{name} already exists; choose it instead")));
    }

    // Choosing it checks these too, but only once macOS has made the
    // account: a refusal then would leave an account nothing uses.
    let (uid, _, _) = me()?;
    ready_to_choose(config_path, uid, None)?;

    let password = Secret(read_secret(std::io::stdin())?);
    let next_uid = init::free_uid()?;
    let mut readied = true;
    let orphans = orphans_now()?;
    let made = closing_first(next_uid, &orphans, || {
        as_root::create(name, next_uid, full_name, &password.0, &orphans)
    })?;
    let account = match made {
        Ok(()) => created(name, None)?,
        // Looked at again: a login may have been left open since.
        Err(Refusal::Unavailable) => init::setup_transaction(&Controller::new(next_uid)?, || {
            without_approval(&orphans_now()?, || {
                readied = false;
                let mut args = init::add_user_args(name, full_name, next_uid);
                args.push("interactive".to_owned());
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                answer_prompts(init::SYSADMINCTL, &args, &password.0, APPROVAL)?;
                created(name, None)
            })
        })?,
        Err(Refusal::Cancelled) => {
            return Err(problem(
                "The account was not created. Approve the macOS dialog to create it.",
            ));
        }
        Err(Refusal::Failed(reason)) => {
            // Made, but not readied: it can be chosen again, which readies it.
            return Err(problem(match setup::account_named(name)? {
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
    let expected = setup::identity::current(&account)
        .ok_or_else(|| problem("macOS could not identify the new account"))?;
    let chosen = use_helper(config_path, &account, &expected)?;
    Ok(json!({
        "ok": true,
        "created": {"name": account.name, "uid": account.uid},
        "chosen": chosen,
        "readied": readied,
    }))
}

/// The standard account `name` a creation made, or why not, from what the
/// creation said when it failed (`detail`). sysadminctl reports success even
/// when it did nothing: the account itself is the answer.
fn created(name: &str, detail: Option<String>) -> Result<setup::Account> {
    let Some(account) = setup::account_named(name)? else {
        let detail = detail.unwrap_or_default();
        return Err(problem(if detail.is_empty() {
            "The account was not created. Approve the macOS dialog to create it.".to_owned()
        } else {
            format!("The account was not created: {detail}")
        }));
    };

    if setup::administrator(&account.name) != Some(false) {
        return Err(problem(format!(
            "{name} was created, but not as a standard account"
        )));
    }

    Ok(account)
}

/// A password, zeroed when dropped.
struct Secret(Vec<u8>);

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

/// The last thing a program said: its last line that is not empty and not a
/// password question.
fn last_words(text: &str) -> Option<&str> {
    text.lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty() && !line.to_ascii_lowercase().ends_with("password:"))
}

/// A password from standard input: one line of 1 to 256 bytes, with no
/// control characters (a terminal would edit the line with them, so macOS
/// would receive a different password than the one typed).
fn read_secret(input: impl Read) -> Result<Vec<u8>> {
    // To the end: the window closes its input after the password.
    let mut secret = Vec::new();
    input.take(258).read_to_end(&mut secret)?;

    if secret.last() == Some(&b'\n') {
        secret.pop();
    }

    if secret.is_empty()
        || secret.len() > 256
        || secret.iter().any(|byte| *byte < 0x20 || *byte == 0x7F)
    {
        secret.fill(0);
        return Err(problem(
            "Give one password of 1 to 256 characters, without control characters",
        ));
    }

    Ok(secret)
}

/// Bind the terminal worker before it executes `program`, then type `secret`
/// only when asked. Its exact PID stays in the transaction's live claim even
/// if the caller dies; the secret is cut out of anything kept from output.
fn answer_prompts(
    program: &str,
    args: &[&str],
    secret: &[u8],
    patience: Duration,
) -> Result<String> {
    let script = [as_root::EXECUTOR, "exec \"$@\"\n"].concat();
    let (mut child, executor) = as_root::start_executor(
        Command::new("/usr/bin/script")
            .args([
                "-q",
                "/dev/null",
                "/bin/zsh",
                "-f",
                "-c",
                &script,
                "zsh",
                program,
            ])
            .args(args)
            .stderr(Stdio::piped()),
    )
    .map_err(|refusal| {
        problem(format!("The terminal worker did not start: {refusal:?}")).uncertain()
    })?;

    let mut input = child.stdin.take().ok_or_else(|| problem("No input pipe"))?;
    let output = child
        .stdout
        .take()
        .ok_or_else(|| problem("No output pipe"))?;
    let errors = child
        .stderr
        .take()
        .ok_or_else(|| problem("No error pipe"))?;

    let receiver = init::forward(output);
    // Drained as it comes, so a chatty program never blocks on it.
    let complaints = init::forward(errors);
    let mut seen = Vec::new();
    let answered = answer(
        &mut input,
        &receiver,
        secret,
        MAX_ANSWERS,
        Instant::now() + patience,
        &mut seen,
    );
    if let Err(error) = answered {
        let _ = child.kill();
        let _ = child.wait();
        as_root::wait_until_ended(executor);
        seen.fill(0);
        return Err(error);
    }

    drop(input);
    let status = child.wait()?;
    as_root::wait_until_ended(executor);

    // The secret is cut out byte for byte, never copied into a string.
    let kept = without(&seen, secret);
    seen.fill(0);
    let text = String::from_utf8_lossy(&kept).into_owned();

    if status.success() {
        return Ok(text);
    }

    let complained: Vec<u8> = complaints.try_iter().flatten().collect();
    let complained = String::from_utf8_lossy(&without(&complained, secret)).into_owned();
    let summary = last_words(&text)
        .or_else(|| last_words(&complained))
        .unwrap_or_default()
        .to_owned();

    Err(problem(summary))
}

/// Read what a program writes until it stops (its output closes) or
/// `deadline` passes, collecting it into `seen`, and type `secret` and a
/// newline into `input` each time it has asked for a password, up to
/// `answers` times.
fn answer(
    input: &mut impl Write,
    receiver: &std::sync::mpsc::Receiver<Vec<u8>>,
    secret: &[u8],
    answers: usize,
    deadline: Instant,
    seen: &mut Vec<u8>,
) -> Result<()> {
    let mut answered = 0;

    loop {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(bytes) => seen.extend(bytes),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        }

        // A question ends without a newline: "User password:".
        let asked = seen
            .windows(PROMPT.len())
            .filter(|window| window.eq_ignore_ascii_case(PROMPT))
            .count();
        if asked > answered && answered < answers {
            // Let the prompt turn echo off before typing.
            std::thread::sleep(Duration::from_millis(150));

            input
                .write_all(secret)
                .and_then(|()| input.write_all(b"\n"))
                .and_then(|()| input.flush())?;
            answered += 1;
        }

        if Instant::now() >= deadline {
            return Err(problem("The step took too long and was stopped"));
        }
    }
}

/// What a password question ends with, in any case.
const PROMPT: &[u8] = b"password:";

/// The most password questions answered, so a program that keeps asking
/// (after a refused password, say) is not fed the secret forever.
const MAX_ANSWERS: usize = 3;

/// `bytes` with every occurrence of `secret` cut out.
fn without(bytes: &[u8], secret: &[u8]) -> Vec<u8> {
    if secret.is_empty() {
        return bytes.to_vec();
    }

    let mut kept = Vec::with_capacity(bytes.len());
    let mut rest = bytes;
    while !rest.is_empty() {
        if rest.starts_with(secret) {
            rest = &rest[secret.len()..];
        } else {
            kept.push(rest[0]);
            rest = &rest[1..];
        }
    }

    kept
}

/// In the helper: record Allow and open a setup worker, without authorization
/// or changing jobs. The requesting account drives setup. Only for the
/// account the person was shown (`controller_uid`, `controller_name`): a
/// request changed meanwhile is refused.
pub fn accept(controller_uid: u32, controller_name: &str) -> Result<Value> {
    let (uid, my_name, _) = me()?;

    acceptance::allow(
        (controller_uid, controller_name),
        (uid, &my_name),
        config::installed_path().is_ok_and(|path| path.exists()),
        setup::administrator(&my_name),
        request(),
        acceptance::signal,
    )
}

/// The installed Atlas.app's executable, refused unless its developer signed it.
pub(crate) fn signed_executable() -> Result<PathBuf> {
    signed_executable_using(setup::installed_app::app_executable()?, setup::developer_id)
}

pub(crate) fn signed_executable_using(
    executable: PathBuf,
    developer_id: impl FnOnce(&Path) -> bool,
) -> Result<PathBuf> {
    // Installation location for the shared app. The approved executor
    // independently protects and verifies its own copy before using it.
    if !executable.starts_with("/Applications") {
        return Err(problem(
            "Atlas.app must be in Applications to set up the helper. Move it there, then open it again.",
        ));
    }

    if !developer_id(&executable) {
        return Err(problem(
            "This Atlas.app is not signed by Atlas's developer. Install it again.",
        ));
    }

    Ok(executable)
}

/// The background job Atlas installs in this account for `controller_uid`.
fn expected_agent(layout: &Layout, controller_uid: u32) -> Result<Agent> {
    Ok(Agent {
        label: LABEL.to_owned(),
        program: setup::installed_app::app_executable()?,
        uid: layout.uid,
        controller_uid,
        socket: layout.socket(),
    })
}

/// Mark (`true`) or clear the onboarding window being open in this account.
pub fn window(open: bool) -> Result<Value> {
    let path = home()?.join(setup::helper::ROOT).join(WINDOW_MARKER);

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

/// Whether this account's onboarding window is open: the process it names
/// still lives and is Atlas's window (a reused process ID is not).
pub fn window_open(home: &Path) -> bool {
    let Some(pid) = read_text(&home.join(setup::helper::ROOT).join(WINDOW_MARKER), 64)
        .and_then(|text| text.trim().parse::<i32>().ok())
        .filter(|pid| *pid > 1)
    else {
        return false;
    };

    if nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_err() {
        return false;
    }

    Command::new("/bin/ps")
        .args(["-o", "comm=", "-p", &pid.to_string()])
        .output()
        .is_ok_and(|output| {
            Path::new(String::from_utf8_lossy(&output.stdout).trim()).file_name()
                == Some(std::ffi::OsStr::new(crate::menubar::APP_EXECUTABLE))
        })
}

/// Read only a bounded plain text file, without waiting for a pipe's writer.
fn read_text(path: &Path, limit: u64) -> Option<String> {
    let mut text = String::new();
    crate::helper_info::open_plain(path)?
        .take(limit + 1)
        .read_to_string(&mut text)
        .ok()?;
    (text.len() as u64 <= limit).then_some(text)
}

/// Back in the person's account: start input, record and check the setup
/// end to end, as `atlas init` does.
pub async fn finish(config_path: &Path) -> Result<Value> {
    let config = Config::load(config_path)?;
    let client = Client::new(config.clone())?;
    client.bootstrap_probe()?;
    let status = client.status().await;
    crate::session::require_hidden(&status, config.helper_uid)?;

    if !init::missing(&status).is_empty() {
        return Err(problem(
            "Screen Recording or Accessibility is still off in the helper",
        ));
    }

    if !client.controller.pending()?.is_empty() {
        client.restart().await?;
    }

    if let Some(helper) = setup::account_with_uid(config.helper_uid)? {
        setup::migrate::archive_foreign(helper.uid, &helper.home)?;
    }

    client.start_input().await?;

    let ownership = client.controller.acquire(true)?;
    let recorded = setup::migrate::apply_fresh(config_path, &ownership).await?;
    drop(ownership);

    if recorded["ok"] != true {
        return Err(problem(format!(
            "Setup could not be verified: {}",
            recorded["error"].as_str().unwrap_or("unknown")
        )));
    }

    let doctor = crate::capability::doctor(client.status().await, config.helper_uid);
    if doctor["ready"] != true {
        return Err(problem(
            "Atlas is set up but not ready yet. Try again in a moment.",
        ));
    }

    Ok(json!({"ok": true, "ready": true, "doctor": doctor}))
}

/// Open the menu bar at login, or not, as the person chose. As with `atlas
/// init`, only for the default settings, the ones the login item uses.
pub fn login(config_path: &Path, on: bool) -> Result<Value> {
    let default = || {
        config::installed_path()
            .is_ok_and(|default| crate::agents::same_place(&default, config_path))
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

#[cfg(test)]
mod tests {
    #[test]
    fn choosing_the_configured_helper_again_bypasses_the_replacement_refusal() {
        let config = Config {
            helper_uid: 502,
            socket_path: "/unused/daemon.sock".into(),
            identity_probe: "/unused/atlas".into(),
            input_socket: "/unused/input.sock".into(),
            output_dir: "/unused/runs".into(),
        };
        let keep = |_: u32, _: &Config| Err(problem("The configured helper is in use"));
        assert!(check_choice(501, Some(502), &config, keep).is_ok());
        assert!(check_choice(501, Some(503), &config, keep).is_err());
        assert!(check_choice(501, None, &config, keep).is_err());
    }

    #[test]
    fn setup_without_orphans_excludes_teardown_and_keeps_its_executor_after_caller_death() {
        if !crate::transaction_test::isolated(
            "onboarding::tests::setup_without_orphans_excludes_teardown_and_keeps_its_executor_after_caller_death",
        ) {
            return;
        }
        for mode in ["selected", "allocated"] {
            crate::transaction_test::interrupted_executor(
                "onboarding::tests::an_account_setup_test_caller",
                mode,
            );
        }
    }

    #[test]
    fn interactive_creation_keeps_its_worker_claim_after_the_caller_dies() {
        if !crate::transaction_test::isolated(
            "onboarding::tests::interactive_creation_keeps_its_worker_claim_after_the_caller_dies",
        ) {
            return;
        }
        crate::transaction_test::interrupted_executor(
            "onboarding::tests::an_account_setup_test_caller",
            "interactive",
        );
    }

    /// A subprocess entry point using only temporary state and a harmless executor.
    #[test]
    fn an_account_setup_test_caller() {
        let Some(directory) = crate::transaction_test::directory() else {
            return;
        };
        if std::env::var("ATLAS_MUTATION_TEST_MODE").unwrap() == "interactive" {
            let controller = Controller::at(7001, &directory.join("state"));
            let _ = init::setup_transaction(&controller, || {
                let script = r#"print -r -- $$ > "$4"; /usr/bin/touch "$3" "$2"; while [[ ! -e $1 ]]; do /bin/sleep 0.01; done"#;
                let paths = [
                    directory.join("continue"),
                    directory.join("paused"),
                    directory.join("changed"),
                    directory.join("executor"),
                ];
                let paths: Vec<&str> = paths.iter().map(|path| path.to_str().unwrap()).collect();
                let mut args = vec!["-f", "-c", script, "zsh"];
                args.extend(paths);
                answer_prompts("/bin/zsh", &args, b"unused", Duration::from_secs(30))
            });
            return;
        }
        let _ = closing_first_using(
            7001,
            &[],
            |uid| Ok(Controller::at(uid, &directory.join("state"))),
            || {
                crate::transaction_test::pause_executor(&directory);
                Ok(())
            },
            || true,
        );
    }

    #[test]
    fn setup_requires_its_own_acknowledgement_even_when_every_orphan_is_gone() {
        if !crate::transaction_test::isolated(
            "onboarding::tests::setup_requires_its_own_acknowledgement_even_when_every_orphan_is_gone",
        ) {
            return;
        }
        for outcome in ["complete", "refused", "partial", "unknown"] {
            let directory = tempfile::tempdir().unwrap();
            let controller = Controller::at(7001, directory.path());
            let competing_teardown = controller.acquire(true).unwrap();
            let blocked = closing_first_using(
                7001,
                &[],
                |uid| Ok(Controller::at(uid, directory.path())),
                || panic!("setup cannot dispatch while teardown owns the helper"),
                || true,
            );
            assert!(matches!(blocked, Err(error) if error.code == "busy"));
            drop(competing_teardown);
            let directory = tempfile::tempdir().unwrap();
            let controller = Controller::at(7001, directory.path());
            let result = closing_first_using(
                7001,
                &[],
                |uid| Ok(Controller::at(uid, directory.path())),
                || {
                    assert!(controller.acquire(true).is_err());
                    assert_eq!(controller.pending().unwrap().len(), 1);
                    if outcome == "complete" {
                        Ok(())
                    } else {
                        Err(as_root::Unclosed {
                            refusal: Refusal::Failed("setup did not complete".into()),
                            acted: outcome != "refused",
                            ended: outcome != "unknown",
                        })
                    }
                },
                || true,
            )
            .unwrap();
            assert_eq!(result.is_ok(), outcome == "complete");
            assert_eq!(
                controller.pending().unwrap().len(),
                usize::from(matches!(outcome, "partial" | "unknown"))
            );
        }
    }
    #[test]
    fn preparing_an_account_holds_every_orphan_and_records_interrupted_cleanup() {
        if !crate::transaction_test::isolated(
            "onboarding::tests::preparing_an_account_holds_every_orphan_and_records_interrupted_cleanup",
        ) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let controllers = [
            Controller::at(7001, dir.path()),
            Controller::at(7002, dir.path()),
        ];
        for busy in &controllers {
            let other = busy.acquire(true).unwrap();
            let result = setup_batch(
                &controllers,
                || -> std::result::Result<(), as_root::Unclosed> {
                    unreachable!("setup never runs while an orphan is controlled")
                },
                || true,
            );
            assert!(matches!(result, Err(error) if error.code == "busy"));
            drop(other);
        }
        let result = setup_batch(
            &controllers,
            || -> std::result::Result<(), as_root::Unclosed> {
                for controller in &controllers {
                    assert!(controller.acquire(true).is_err());
                    assert_eq!(controller.pending().unwrap().len(), 1);
                }
                Err(as_root::Unclosed {
                    refusal: Refusal::Failed("cleanup was interrupted".into()),
                    acted: true,
                    ended: true,
                })
            },
            || false,
        );
        assert!(matches!(result, Ok(Err(Refusal::Failed(_)))));
        for controller in controllers {
            assert_eq!(controller.pending().unwrap().len(), 1);
        }
    }

    use super::*;

    #[test]
    fn permissions_without_the_jobs_answer_are_unknown_never_off() {
        assert_eq!(permissions_from(None), Value::Null);
        let hello = json!({"permissions": {"screenCapture": true, "accessibility": false}});
        assert_eq!(
            permissions_from(Some(&hello)),
            json!({"screenCapture": true, "accessibility": false})
        );
    }

    #[test]
    fn a_secret_is_one_short_plain_line() {
        assert_eq!(read_secret(&b"hunter2\n"[..]).unwrap(), b"hunter2");
        assert_eq!(read_secret(&b"hunter2"[..]).unwrap(), b"hunter2");
        assert_eq!(
            read_secret("pässwörd\n".as_bytes()).unwrap(),
            "pässwörd".as_bytes()
        );
        assert!(read_secret(&b"\n"[..]).is_err());
        assert!(read_secret(&b"two\nlines\n"[..]).is_err());
        assert!(read_secret(&b"carriage\rreturn"[..]).is_err());
        assert!(read_secret(&b"del\x7fete"[..]).is_err());
        assert!(read_secret(&[b'x'; 300][..]).is_err());
    }

    /// The secret is typed only once asked, reaches a terminal prompt
    /// with echo off, and never appears in what is kept of the output.
    #[test]
    fn prompts_are_answered_through_a_terminal() {
        let script = "printf 'User password:'; stty -echo; read p; stty echo; echo; \
                      [ \"$p\" = 'se cret' ] && echo matched";
        let text = answer_prompts(
            "/bin/sh",
            &["-c", script],
            b"se cret",
            Duration::from_secs(10),
        )
        .unwrap();
        assert!(text.contains("matched"), "{text}");
        assert!(!text.contains("se cret"), "{text}");

        let wrong = "printf 'Password:'; read p; exit 3";
        let failed = answer_prompts("/bin/sh", &["-c", wrong], b"abc", Duration::from_secs(10));
        assert!(failed.is_err());
    }

    #[test]
    fn the_secret_is_cut_out_byte_for_byte() {
        assert_eq!(without(b"a se cret b se cret", b"se cret"), b"a  b ");
        assert_eq!(without(b"sesecret", b"secret"), b"se");
        assert_eq!(without(b"\xff\xfepass", b"\xfe"), b"\xffpass");
        assert_eq!(without(b"kept", b""), b"kept");
    }

    fn pairing(controller: u32) -> Pairing {
        Pairing {
            ticket: "a".repeat(crate::control::TOKEN_BYTES * 2),
            controller_uid: controller,
            controller_name: "person".into(),
            helper_uid: controller + 1,
            helper_name: "helper".into(),
            helper_identity: setup::identity::Identity {
                uid: controller + 1,
                name: "helper".into(),
                generated_uid: "6DB4B7AF-BE3C-486C-BD46-1B4558946919".into(),
            },
        }
    }

    #[test]
    fn a_request_is_trusted_only_from_its_controller() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
        let path = dir.path().join("onboarding.json");
        let me = nix::unistd::getuid().as_raw();

        write_request(&path, &pairing(me)).unwrap();
        assert_eq!(read_request(&path), Some(pairing(me)));

        // Existing requests can finish without repeating account preparation.
        let mut legacy: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        legacy.as_object_mut().unwrap().remove("ticket");
        config::write_atomic(&path, &serde_json::to_vec(&legacy).unwrap(), 0o644).unwrap();
        let read = read_request(&path).unwrap();
        assert_eq!(read.controller_uid, me);
        assert_eq!(read.ticket.len(), crate::control::TOKEN_BYTES * 2);
        assert_eq!(read_request(&path), Some(read));

        // Written by someone else than the controller it names: ignored.
        write_request(&path, &pairing(me + 7)).unwrap();
        assert_eq!(read_request(&path), None);

        // Writable by others: ignored.
        write_request(&path, &pairing(me)).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
        assert_eq!(read_request(&path), None);

        // In a folder others can change: ignored.
        write_request(&path, &pairing(me)).unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o777)).unwrap();
        assert_eq!(read_request(&path), None);
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();

        // A link to a request: ignored.
        let link = dir.path().join("link.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert_eq!(read_request(&link), None);
    }

    #[test]
    fn shared_requests_and_helper_text_never_wait_for_a_pipe_writer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("onboarding.json");
        nix::unistd::mkfifo(&path, nix::sys::stat::Mode::S_IRUSR).unwrap();
        assert_eq!(read_request(&path), None);
        assert_eq!(read_text(&path, 64 * 1024), None);
        fs::remove_file(&path).unwrap();
        fs::write(&path, "a".repeat(65)).unwrap();
        assert_eq!(read_text(&path, 64), None);
        fs::write(&path, "a".repeat(64)).unwrap();
        assert_eq!(read_text(&path, 64), Some("a".repeat(64)));
    }

    /// A helper answers only while something listens at its socket; a
    /// socket file left behind, or none, is silent.
    #[test]
    fn a_helper_answers_only_while_its_socket_listens() {
        if !crate::transaction_test::isolated(
            "onboarding::tests::a_helper_answers_only_while_its_socket_listens",
        ) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("atlas.sock");
        let config = Config {
            helper_uid: 502,
            socket_path: socket.clone(),
            identity_probe: dir.path().join("atlas"),
            input_socket: dir.path().join("input.sock"),
            output_dir: dir.path().join("runs"),
        };
        assert!(!answers(&config), "no socket");

        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        assert!(answers(&config));

        drop(listener);
        assert!(socket.exists());
        assert!(!answers(&config), "a socket nothing listens at");
    }

    /// A helper whose Atlas folder was prepared for this account may have
    /// its job installed, so its settings are not replaced by another's.
    #[test]
    fn a_helper_prepared_for_this_account_is_recognised() {
        let dir = tempfile::tempdir().unwrap();
        let helper = setup::Account {
            name: "helper".into(),
            uid: 502,
            home: dir.path().to_owned(),
        };
        assert!(!prepared(&helper), "nothing prepared");

        let layout = Layout {
            uid: helper.uid,
            home: helper.home.clone(),
            guard: None,
        };
        fs::create_dir_all(layout.root()).unwrap();
        assert!(!prepared(&helper), "only its root");

        fs::create_dir_all(layout.daemon()).unwrap();
        assert!(prepared(&helper));
    }

    /// Only a request this account made, for a helper that never answered,
    /// was never prepared and has no setup recorded, may be taken back.
    #[test]
    fn only_an_unanswered_request_may_be_taken_back() {
        assert!(may_forget(true, false, false, false));
        assert!(
            !may_forget(false, false, false, false),
            "not this account's"
        );
        assert!(!may_forget(true, true, false, false), "a setup is recorded");
        assert!(!may_forget(true, false, true, false), "the helper answered");
        assert!(
            !may_forget(true, false, false, true),
            "prepared for this account"
        );
    }

    #[test]
    fn the_window_marker_names_a_living_window() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(setup::helper::ROOT);
        fs::create_dir_all(&root).unwrap();

        // A living process that is not Atlas's window does not count.
        fs::write(
            root.join(WINDOW_MARKER),
            format!("{}\n", std::process::id()),
        )
        .unwrap();
        assert!(!window_open(dir.path()));

        fs::write(root.join(WINDOW_MARKER), "999999\n").unwrap();
        assert!(!window_open(dir.path()));

        fs::remove_file(root.join(WINDOW_MARKER)).unwrap();
        assert!(!window_open(dir.path()));

        nix::unistd::mkfifo(&root.join(WINDOW_MARKER), nix::sys::stat::Mode::S_IRUSR).unwrap();
        assert!(!window_open(dir.path()));
    }

    fn opie() -> Identity {
        Identity {
            uid: 503,
            name: "opie".into(),
            generated_uid: "6DB4B7AF-BE3C-486C-BD46-1B4558946919".into(),
        }
    }

    fn sessions(logins: &[(u32, u32, crate::session::Kind)]) -> crate::session::Sessions {
        crate::session::Sessions {
            logins: logins
                .iter()
                .map(|(uid, pid, kind)| {
                    let window = crate::session::Window {
                        uid: *uid,
                        pid: *pid,
                        age: Duration::from_secs(600),
                    };
                    (window, *kind)
                })
                .collect(),
            blockers: Vec::new(),
        }
    }

    #[test]
    fn a_login_under_the_helpers_user_id_is_closed_only_as_the_recorded_helpers() {
        use crate::session::Kind;

        let found = sessions(&[
            (7001, 11, Kind::Orphan),
            (503, 22, Kind::HelperLeftover),
            (501, 33, Kind::Mine),
            (504, 44, Kind::OtherPerson),
        ]);
        assert_eq!(
            leftover_targets(&found, Some(&opie())),
            Ok(vec![
                Target::Orphan(7001),
                Target::Leftover {
                    helper: opie(),
                    pid: 22
                },
            ])
        );
        assert!(leftover_targets(&found, None).is_err(), "nothing recorded");
        let other = Identity { uid: 505, ..opie() };
        assert!(leftover_targets(&found, Some(&other)).is_err());
    }

    #[test]
    fn every_leftover_login_closes_under_one_approval() {
        if !crate::transaction_test::isolated(
            "onboarding::tests::every_leftover_login_closes_under_one_approval",
        ) {
            return;
        }
        let targets = [
            Target::Orphan(7001),
            Target::Leftover {
                helper: opie(),
                pid: 22,
            },
            Target::Orphan(7003),
        ];
        let dir = tempfile::tempdir().unwrap();
        let held = controller(dir.path());
        for controllers in [&[][..], std::slice::from_ref(&held)] {
            let asked = std::cell::Cell::new(0);
            let closed = close_batch(
                controllers,
                &targets,
                |all| {
                    asked.set(asked.get() + 1);
                    assert_eq!(all, targets, "every target in the one run");
                    Ok(all.iter().map(Target::uid).collect())
                },
                |uids| uids == [7001, 503, 7003],
            );
            assert_eq!(closed.unwrap(), [7001, 503, 7003]);
            assert_eq!(asked.get(), 1, "one approval");
        }
        assert!(held.pending().unwrap().is_empty());

        let none = close_batch(
            std::slice::from_ref(&held),
            &[],
            |_| unreachable!("nothing to close"),
            |_| true,
        );
        assert_eq!(none.unwrap(), Vec::<u32>::new());

        let still = close_batch(
            &[],
            &targets,
            |all| Ok(all.iter().map(Target::uid).collect()),
            |_| false,
        );
        assert!(
            matches!(still, Err(Held::Refused(Refusal::Failed(why))) if why == "it is still running"),
            "seen gone, not taken on root's word"
        );
    }

    #[test]
    fn closing_leftover_logins_holds_the_helper_and_keeps_an_unknown_outcome() {
        if !crate::transaction_test::isolated(
            "onboarding::tests::closing_leftover_logins_holds_the_helper_and_keeps_an_unknown_outcome",
        ) {
            return;
        }
        let targets = [Target::Orphan(7001), Target::Orphan(503)];
        let dir = tempfile::tempdir().unwrap();
        let held = controller(dir.path());

        // Another controller holds the helper: root is never asked.
        let other = held.acquire(true).unwrap();
        let busy = close_batch(
            std::slice::from_ref(&held),
            &targets,
            |_| unreachable!("root is not asked"),
            |_| true,
        );
        assert!(matches!(busy, Err(Held::State(error)) if error.code == "busy"));
        drop(other);

        // Held throughout, and recorded before root is asked.
        let interrupted = close_batch(
            std::slice::from_ref(&held),
            &targets,
            |_| {
                assert!(controller(dir.path()).acquire(true).is_err(), "taken");
                assert_eq!(controller(dir.path()).pending().unwrap().len(), 1);
                Err(as_root::Unclosed {
                    refusal: Refusal::Failed("macOS didn't close the login of user ID 503".into()),
                    acted: true,
                    ended: true,
                })
            },
            |_| false,
        );
        assert!(interrupted.is_err());
        assert_eq!(held.pending().unwrap().len(), 1, "kept for recovery");
    }

    #[test]
    fn creating_without_the_approval_call_waits_while_a_deleted_accounts_login_is_open() {
        let error = without_approval(&[7001], || -> Result<()> {
            unreachable!("no account is made")
        })
        .unwrap_err();
        assert!(
            error
                .error
                .contains("sudo launchctl bootout user/7001, then try again"),
            "{}",
            error.error
        );
        assert_eq!(without_approval(&[], || Ok(7)).unwrap(), 7);
    }

    /// A controller kept in `dir`, and the generations it has pending.
    fn controller(dir: &Path) -> Controller {
        Controller::at(60001, &dir.join("state"))
    }

    fn untouched(refusal: Refusal) -> as_root::Unclosed {
        as_root::Unclosed {
            refusal,
            acted: false,
            ended: true,
        }
    }

    #[test]
    fn the_helper_is_logged_out_as_root_only_under_control_held_throughout() {
        if !crate::transaction_test::isolated(
            "onboarding::tests::the_helper_is_logged_out_as_root_only_under_control_held_throughout",
        ) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let held = controller(dir.path());

        // Another controller holds the helper: root is never asked.
        let other = held.acquire(true).unwrap();
        let busy = close_helper_held(
            &held,
            || Ok(opie()),
            |_| unreachable!("root is not asked"),
            || true,
        );
        assert!(matches!(busy, Err(Held::State(error)) if error.code == "busy"));
        drop(other);

        // Held while root runs and while the login is looked at.
        let ended = (0..50)
            .find_map(|_| {
                let result = close_helper_held(
                    &held,
                    || Ok(opie()),
                    |helper| {
                        assert!(controller(dir.path()).acquire(true).is_err(), "taken");
                        assert_eq!(helper, opie());
                        Ok(vec![503])
                    },
                    || {
                        assert!(controller(dir.path()).acquire(true).is_err(), "taken");
                        true
                    },
                );
                match result {
                    Err(Held::State(error)) if error.code == "busy" => {
                        std::thread::sleep(Duration::from_millis(20));
                        None
                    }
                    result => Some(result),
                }
            })
            .expect("a child that inherited the old lock finishes its exec");
        assert!(ended.is_ok());
        assert!(held.pending().unwrap().is_empty(), "settled once seen gone");
        assert!(held.acquire(false).is_ok(), "let go after");
    }

    #[test]
    fn a_log_out_as_root_stays_pending_until_the_login_is_seen_gone() {
        if !crate::transaction_test::isolated(
            "onboarding::tests::a_log_out_as_root_stays_pending_until_the_login_is_seen_gone",
        ) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let held = controller(dir.path());

        // Recorded before root is asked, so a run cut short leaves it.
        let ownership = held.acquire(true).unwrap();
        let lost = end_as_root(
            &held,
            &ownership,
            || {
                assert_eq!(controller(dir.path()).pending().unwrap().len(), 1);
                Err(as_root::Unclosed {
                    refusal: Refusal::Failed(
                        "Atlas stopped waiting for the approved step, which may still finish"
                            .into(),
                    ),
                    acted: true,
                    ended: false,
                })
            },
            || false,
        );
        assert!(matches!(lost, Err(Held::Refused(Refusal::Failed(_)))));
        assert_eq!(
            held.pending().unwrap().len(),
            1,
            "a lost reply stays pending"
        );

        // Root said it closed it, but it is still seen: pending too.
        let stayed = end_as_root(&held, &ownership, || Ok(vec![503]), || false);
        assert!(stayed.is_err());
        assert_eq!(held.pending().unwrap().len(), 2);
        drop(ownership);
        assert_eq!(
            held.acquire(false).unwrap_err().code,
            "uncertain",
            "recovery comes first"
        );
    }

    #[test]
    fn a_root_run_whose_reply_was_lost_stays_pending_though_the_login_looks_gone() {
        if !crate::transaction_test::isolated(
            "onboarding::tests::a_root_run_whose_reply_was_lost_stays_pending_though_the_login_looks_gone",
        ) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let held = controller(dir.path());
        let ownership = held.acquire(true).unwrap();

        // The desktop is gone for now, but root may still be waiting to act.
        let lost = end_as_root(
            &held,
            &ownership,
            || {
                Err(as_root::Unclosed {
                    refusal: Refusal::Failed("The step did not finish".into()),
                    acted: true,
                    ended: false,
                })
            },
            || unreachable!("how the login looks proves nothing yet"),
        );
        assert!(matches!(lost, Err(Held::Refused(_))));
        assert_eq!(held.pending().unwrap().len(), 1);
    }

    #[test]
    fn a_log_out_root_never_started_leaves_nothing_pending() {
        if !crate::transaction_test::isolated(
            "onboarding::tests::a_log_out_root_never_started_leaves_nothing_pending",
        ) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let held = controller(dir.path());
        let ownership = held.acquire(true).unwrap();

        for refusal in [
            Refusal::Cancelled,
            Refusal::Unavailable,
            Refusal::Failed(
                "the helper is on screen; switch back to your own account first".into(),
            ),
        ] {
            let ended = end_as_root(
                &held,
                &ownership,
                || Err(untouched(refusal.clone())),
                || false,
            );
            assert!(matches!(ended, Err(Held::Refused(said)) if said == refusal));
        }
        assert!(held.pending().unwrap().is_empty());

        // Root stopped after it acted, but the login is gone: settled.
        let late = end_as_root(
            &held,
            &ownership,
            || {
                Err(as_root::Unclosed {
                    refusal: Refusal::Failed("macOS didn't end the login of user ID 503".into()),
                    acted: true,
                    ended: true,
                })
            },
            || true,
        );
        assert!(late.is_ok());
        assert!(held.pending().unwrap().is_empty());
    }
}
