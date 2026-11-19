use super::*;

/// Everything the window needs to choose its step, read fresh each time.
///
/// # Errors
///
/// Returns configuration or onboarding-state failures. Status may perform the existing
/// receipt or listener repair; it never invents successful consent.
pub async fn status(config_path: &Path) -> Result<Value> {
    let account = this_account()?;
    let uid = account.uid;
    let me = json!({
        "uid": uid,
        "name": account.name,
        "realName": atlas_core::user::account::real_name(&account.name),
    });
    let app = app();

    // Atlas opened in an account another one chose as its helper (an
    // account with Atlas settings of its own is a controller, whatever a
    // shared file says).
    if atlas_client::user::setup::configuration::absent(config_path) {
        let pairing = atlas_daemon::user::setup::pairing::request()
            .filter(|pairing| pairing.helper_uid == uid);
        if pairing.is_some() || helper_job_here(uid) {
            return Ok(json!({
                "ok": true,
                "role": "helper",
                "me": me,
                "app": app,
                "helper": helper_side(uid, pairing.as_ref()).await,
            }));
        }

        let mut reply = choosing(me, app, false, logins(None))?;
        reply["desktops"] = desktops().await;
        reply["vm"] = json!(first_vm(config_path).await);
        return Ok(reply);
    }

    // Settings that list VMs alone: no Second Login to look at.
    if only_vms(config_path) {
        let vm = first_vm(config_path).await;
        let mut reply = on_vms(
            me,
            app,
            vm,
            desktops().await,
            crate::menubar::login_decided(),
        );
        reply["adding"] = adding(config_path).await;
        return Ok(reply);
    }

    // Settings' Add a Desktop… opens the window on a setup already made:
    // it chooses again and follows the add it asks for.
    let mut reply = configured(config_path, uid, me, app).await?;
    reply["desktops"] = desktops().await;
    reply["adding"] = adding(config_path).await;
    Ok(reply)
}

/// Whether the settings at `config_path` load and list no Second Login.
fn only_vms(config_path: &Path) -> bool {
    Settings::load(config_path).is_ok_and(|settings| settings.user().is_err())
}

/// The setup of settings that list VMs alone: configured, with no account to
/// choose, the VM of the first setup `vm` and whether this Mac can make
/// another, `desktops`; finished once that VM is ready.
fn on_vms(me: Value, app: Value, vm: Option<Value>, desktops: Value, login_decided: bool) -> Value {
    let finished = vm.as_ref().is_some_and(|vm| vm["ready"] == true);
    json!({
        "ok": true,
        "role": "controller",
        "me": me,
        "app": app,
        "configured": true,
        "candidates": [],
        "desktops": desktops,
        "vm": vm,
        "finished": finished,
        "menubarLogin": login_decided,
        "leftoverLogins": [],
        "blockers": [],
    })
}

/// The desktops a first setup can choose beyond Second Login: whether this
/// Mac can make a VM now.
async fn desktops() -> Value {
    let room = tokio::task::spawn_blocking(|| {
        config::home().map_or_else(
            |_| json!({"available": false, "why": atlas_client::vm::room::Lack::Disk}),
            |home| atlas_client::vm::room::now(&home),
        )
    })
    .await
    .unwrap_or(Value::Null);
    json!({"vm": room})
}

/// The VM of the first setup, as `atlas desktop list` shows it, with
/// whether it is ready for agents; none when there is none, or the list
/// cannot be read.
async fn first_vm(config_path: &Path) -> Option<Value> {
    let path = config_path.to_owned();
    let listing = tokio::task::spawn_blocking(move || atlas_client::vm::listing::list(&path))
        .await
        .ok()?
        .ok()?;
    let mut vm = setting_up(&listing)?;
    vm["ready"] = json!(ready(config_path, &vm).await);
    Some(vm)
}

/// Of the desktops `listing` shows, the VM a first setup makes: one being
/// added, else the default one once added. None once the settings list a
/// Second Login, whose own setup the window shows.
fn setting_up(listing: &Value) -> Option<Value> {
    let desktops = listing["desktops"].as_array()?;
    if desktops.iter().any(|desktop| desktop["kind"] == "user") {
        return None;
    }
    let vms = || desktops.iter().filter(|desktop| desktop["kind"] == "vm");
    vms()
        .find(|vm| vm["lifecycle"] == "adding")
        .or_else(|| vms().find(|vm| vm["default"] == true && vm["listed"] == true))
        .cloned()
}

/// The VM being added beside the desktops the settings list, as `atlas
/// desktop list` shows it, which Settings' Add a Desktop… follows in the
/// window; never ready, as it is still being added. Null while no add is
/// under way, and nothing is asked of any VM then.
async fn adding(config_path: &Path) -> Value {
    let path = config_path.to_owned();
    tokio::task::spawn_blocking(move || atlas_client::vm::listing::while_adding(&path))
        .await
        .ok()
        .and_then(Result::ok)
        .flatten()
        .and_then(|listing| being_added(&listing))
        .map_or(Value::Null, |mut vm| {
            vm["ready"] = json!(false);
            vm
        })
}

/// Of the desktops `listing` shows, the VM being added, whatever else the
/// settings list.
fn being_added(listing: &Value) -> Option<Value> {
    listing["desktops"]
        .as_array()?
        .iter()
        .find(|desktop| desktop["kind"] == "vm" && desktop["lifecycle"] == "adding")
        .cloned()
}

/// Whether the VM `vm` shows is ready for agents, as its status says: never
/// while it is still being added.
async fn ready(config_path: &Path, vm: &Value) -> bool {
    if vm["lifecycle"] == "adding" {
        return false;
    }
    let Some(name) = vm["name"].as_str() else {
        return false;
    };
    let Ok(settings) = Settings::load(config_path) else {
        return false;
    };
    let Some(listed) = settings
        .desktops()
        .iter()
        .find(|listed| listed.name == name)
    else {
        return false;
    };
    match Client::new(settings.selected(listed)) {
        Ok(client) => client.status().await["ready"] == true,
        Err(_) => false,
    }
}

/// The controller with no helper to look at: the window offers the accounts.
fn choosing(
    me: Value,
    app: Value,
    configured: bool,
    (leftovers, blockers): (Value, Value),
) -> Result<Value> {
    Ok(json!({
        "ok": true,
        "role": "controller",
        "me": me,
        "app": app,
        "configured": configured,
        "candidates": candidates(None)?,
        "menubarLogin": crate::menubar::login_decided(),
        "leftoverLogins": leftovers,
        "blockers": blockers,
    }))
}

async fn configured(config_path: &Path, uid: u32, me: Value, app: Value) -> Result<Value> {
    let config = Config::load_user(config_path)?;
    let (leftovers, blockers) = logins(Some(config.user().uid));
    let Some(helper) = account::account_with_uid(config.user().uid)? else {
        let mut reply = choosing(me, app, true, (leftovers, blockers))?;
        reply["helper"] = json!({"exists": false, "uid": config.user().uid});
        return Ok(reply);
    };

    let admin = atlas_core::user::account::administrator(&helper.name);

    // This account asked this helper: its request names it (the window), or
    // a setup with it is recorded (`atlas init`, or the window's finish).
    let paired = asked(uid, &helper);
    let signed_in = atlas_core::user::logins::logged_in(helper.uid) == Some(true);

    let client = Client::new(Selected::provisional(config.clone()))?;
    let acceptance = if let Some(pairing) = atlas_daemon::user::setup::pairing::request()
        .filter(|pairing| pairing.controller_uid == uid && pairing.helper_uid == helper.uid)
    {
        acceptance::observe(&config, pairing).await
    } else {
        json!(null)
    };
    let reply = client.status().await;
    let atlas_core::session::Diagnosis {
        observation, ready, ..
    } = atlas_client::user::capability::doctor_report(reply.clone(), &config.user()).diagnosis;
    let hidden = config.desktop().prove(&observation).is_ok();
    let answering = hidden || reply["daemon"]["pid"].is_number();
    let finished = ready
        && atlas_client::user::setup::migrate::settled(helper.uid)
        && observation.input_build_current();

    // Never asked and silent (a choice cut short, or an `atlas init` stopped
    // once it wrote settings): the window offers the choice again, and
    // choosing another account replaces these settings. A helper already
    // prepared for this account is the only choice, as `choose` keeps it.
    let only_choice = !paired && !answering && prepared(&helper);
    let candidates = if paired || answering {
        Vec::new()
    } else {
        candidates(only_choice.then_some(helper.uid))?
    };

    // Asked by this window but never answered: the request may be taken
    // back, so another account can be chosen.
    let forgettable = Asked {
        requested: requested(uid, &helper),
        recorded: atlas_client::user::setup::migrate::in_effect_for(helper.uid, &helper.home),
        answered: answering || answers(&config),
        prepared: prepared(&helper),
    }
    .may_forget();

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
            "realName": atlas_core::user::account::real_name(&helper.name),
            "administrator": admin,
            "paired": paired,
            "forgettable": forgettable,
            "signedIn": signed_in,
            "answering": answering,
            "onConsole": reply["session"]["onConsole"] == true,
            "hidden": hidden,
            "permissions": permissions_from(Some(&reply)),
        },
        "setupProblem": acceptance["error"].as_str().map(|error| format!("{error}. Open Atlas in the helper, then return here to retry. Any unfinished setup stays pending until recovery.")),
        "acceptance": acceptance,
        "finished": finished,
        "menubarLogin": crate::menubar::login_decided(),
        "homePrivacyOffered": atlas_client::user::home_privacy::offered(helper.uid),
        "leftoverLogins": leftovers,
        "blockers": blockers,
    }))
}

/// The logins of deleted accounts that are still open, and what would hold
/// up a new login now, as the window reads them; `helper` is the configured
/// helper. Both are empty when the process list could not be read.
fn logins(helper: Option<u32>) -> (Value, Value) {
    match atlas_core::user::logins::sessions(helper) {
        Some(sessions) => (json!(sessions.leftovers()), json!(sessions.blockers)),
        None => (json!([]), json!([])),
    }
}

/// The helper's side of the status: who asked (unknown when `atlas init`
/// set it up), whether its job runs here, and its permissions as that job,
/// the process that uses them, reports them.
async fn helper_side(
    uid: u32,
    pairing: Option<&atlas_daemon::user::setup::pairing::Pairing>,
) -> Value {
    let layout =
        this_account().map(|account| atlas_daemon::user::setup::Layout::new(uid, account.home));

    // Installed for the account asking now: a job left by an earlier Atlas
    // (another app, another account) needs Allow again.
    let installed = layout.as_ref().is_ok_and(|layout| {
        let current = atlas_daemon::user::setup::pairing::read_text(
            &layout.agent_plist(atlas_daemon::user::setup::LABEL),
            atlas_daemon::user::setup::pairing::MAX_RECORD,
        );
        match (current, pairing) {
            (None, _) => false,
            (Some(_), None) => true,
            (Some(text), Some(pairing)) => expected_agent(layout, pairing.controller_uid)
                .is_ok_and(|agent| {
                    text == atlas_daemon::user::setup::plist(&agent, &layout.paths().logs())
                }),
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
    let running = layout.as_ref().is_ok_and(|layout| {
        atlas_daemon::user::setup::launchd::job_pid(layout, atlas_daemon::user::setup::LABEL)
            .is_some()
    });

    // macOS counts a process the window starts as the window's own, so only
    // the job's answer tells what Atlas itself may do here. Without one (not
    // running, or restarting to pick up Screen Recording) they are unknown,
    // not off.
    let hello = match &layout {
        Ok(layout) if running => {
            let transport = atlas_core::transport::Transport {
                path: layout.paths().socket(),
                timeout: Duration::from_secs(3),
                peer_uid: uid,
            };
            transport.call(&json!({"op": "hello"})).await.ok()
        }
        _ => None,
    };
    let permissions = permissions_from(hello.as_ref());

    let controller = pairing.map(|pairing| {
        json!({
            "uid": pairing.controller_uid,
            "name": pairing.controller_name,
            "realName": atlas_core::user::account::real_name(&pairing.controller_name),
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

/// Standard accounts that could be the helper: only user ID `only`, if given.
fn candidates(only: Option<u32>) -> Result<Vec<Value>> {
    Ok(atlas_client::user::switch::standard_accounts()?
        .into_iter()
        .filter(|account| only.is_none_or(|uid| account.uid == uid))
        .map(|account| {
            json!({"name": account.name, "uid": account.uid, "realName": account.real_name})
        })
        .collect())
}

/// The app this runs from, whether it is signed by Atlas's developer, and
/// whether `atlas` in a terminal reaches it.
fn app() -> Value {
    match app_bundle::app_executable() {
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
                "signed": app_bundle::developer_id(&executable),
                "linked": linked,
            })
        }
        Err(error) => json!({"installed": false, "error": error.error}),
    }
}

/// Whether Atlas is set up in this account as someone's helper already
/// (its background job is installed here, as `atlas init` leaves it).
fn helper_job_here(uid: u32) -> bool {
    this_account().is_ok_and(|account| {
        atlas_daemon::user::setup::Layout::new(uid, account.home)
            .agent_plist(atlas_daemon::user::setup::LABEL)
            .exists()
    })
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
