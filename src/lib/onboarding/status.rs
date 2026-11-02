use super::*;

/// Everything the window needs to choose its step, read fresh each time.
///
/// # Errors
///
/// Returns configuration or onboarding-state failures. Status may perform the existing
/// receipt or listener repair; it never invents successful consent.
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

    configured(config_path, uid, me, app).await
}

/// Observe the configured controller and its helper without repeating role selection.
async fn configured(config_path: &Path, uid: u32, me: Value, app: Value) -> Result<Value> {
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
    let crate::session::Diagnosis {
        observation, ready, ..
    } = crate::capability::doctor_report(reply.clone(), config.helper_uid).diagnosis;
    let hidden = observation.require_hidden(config.helper_uid).is_ok();
    let answering = hidden || reply["daemon"]["pid"].is_number();
    let finished =
        ready && setup::migrate::settled(helper.uid) && observation.input_build_current();

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
    let forgettable = Asked {
        requested: requested(uid, &helper),
        recorded: setup::migrate::in_effect_for(helper.uid, &helper.home),
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
pub(super) fn logins(helper: Option<u32>) -> (Value, Value) {
    match crate::session::sessions(helper) {
        Some(sessions) => (json!(sessions.leftovers()), json!(sessions.blockers)),
        None => (json!([]), json!([])),
    }
}

/// The helper's side of the status: who asked (unknown when `atlas init`
/// set it up), whether its job runs here, and its permissions as that job,
/// the process that uses them, reports them.
pub(super) async fn helper_side(uid: u32, pairing: Option<&Pairing>) -> Value {
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
pub(super) fn permissions_from(hello: Option<&Value>) -> Value {
    hello.map_or(Value::Null, |hello| {
        json!({
            "screenCapture": hello["permissions"]["screenCapture"] == true,
            "accessibility": hello["permissions"]["accessibility"] == true,
        })
    })
}

/// Standard accounts that could be the helper.
pub(super) fn candidates() -> Result<Vec<Value>> {
    Ok(crate::switch::standard_accounts()?
        .into_iter()
        .map(|account| {
            json!({"name": account.name, "uid": account.uid, "realName": account.real_name})
        })
        .collect())
}

/// The app this runs from, whether it is signed by Atlas's developer, and
/// whether `atlas` in a terminal reaches it.
pub(super) fn app() -> Value {
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

/// Whether Atlas is set up in this account as someone's helper already
/// (its background job is installed here, as `atlas init` leaves it).
pub(super) fn helper_job_here(uid: u32) -> bool {
    home().is_ok_and(|home| Layout::new(uid, home).agent_plist(LABEL).exists())
}
