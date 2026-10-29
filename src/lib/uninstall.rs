//! `atlas uninstall`: undo what install.sh and init did on this Mac.
//! It asks first, logs the helper out before anything of it goes, keeps the
//! helper account unless asked to delete it (and says how, once it is
//! logged out), and moves the app to the Trash rather than deleting it.
use crate::{
    approval::Refusal,
    client::Client,
    config::{self, Config},
    control::{Controller, Ownership},
    error::{Code, Error, Result},
    onboarding::as_root::{self, Target, Via},
    setup::{
        self,
        helper::{Agent, LABEL, Layout, Request, plist, text_sha256},
        identity::{Identity, Standing},
    },
    ui::{
        progress::{done, say, skipped},
        prompt, text,
    },
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn problem(message: impl Into<String>) -> Error {
    Error::new(Code::Uninstall, message)
}

/// Remove `path` only if it is a link to the app's executable (or the app's
/// own probe copy), never a file someone else put there.
fn remove_link_to(path: &Path, executable: &Path) -> Result<bool> {
    let ours = fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
        && fs::canonicalize(path).ok().as_deref() == Some(executable);
    if ours {
        fs::remove_file(path)?;
    }

    Ok(ours)
}

/// Remove `path` only if it is a regular file this account owns.
fn remove_own_file(path: &Path) -> Result<bool> {
    use std::os::unix::fs::MetadataExt;

    let ours = fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.is_file() && metadata.uid() == nix::unistd::getuid().as_raw()
    });
    if ours {
        fs::remove_file(path)?;
    }

    Ok(ours)
}

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
    let Some(account) = setup::account_with_uid(helper.uid)? else {
        return Ok(None);
    };

    // As the helper but outside any login of it: it is logged out by now,
    // and reaching into its login would start one again. Each request is
    // checked again by the helper itself once sudo let it run: still the
    // account recorded, and still logged out.
    let ask = |request: &Request, failed: &str| -> Result<Value> {
        retirement_request(controller, request, || {
            let (succeeded, reply) =
                setup::request_logged_out_as(executable, &account, request, helper)?;
            if reply.is_null() {
                return Err(problem(failed));
            }

            if !succeeded || reply["ok"] != true {
                return Err(problem(format!(
                    "{failed}: {}",
                    reply["error"].as_str().unwrap_or("unknown")
                )));
            }

            Ok(reply)
        })
    };
    let stopping = "The helper job could not be stopped";

    let inspected = ask(
        &Request::Inspect {
            uid: helper.uid,
            labels: vec![LABEL.to_owned()],
        },
        stopping,
    )?;

    // The definition there now, else (none there) the one this Atlas
    // would write, whose backup a retired job leaves.
    let sha256 = match inspected["jobs"][LABEL]["plistSha256"].as_str() {
        Some(sha256) => sha256.to_owned(),
        None => {
            let layout = Layout {
                uid: helper.uid,
                home: account.home.clone(),
                guard: None,
            };

            let agent = Agent {
                label: LABEL.to_owned(),
                program: executable.to_owned(),
                uid: helper.uid,
                controller_uid: nix::unistd::getuid().as_raw(),
                socket: layout.socket(),
            };

            text_sha256(&plist(&agent, &layout.logs()))
        }
    };

    let opener = ask(
        &Request::ForgetOpener { uid: helper.uid },
        "Atlas's one-time start in the helper was not removed",
    )
    .map(|reply| reply["removed"] == true)
    .map_err(|error| error.error);

    ask(
        &Request::Retire {
            uid: helper.uid,
            label: LABEL.to_owned(),
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
    request: &Request,
    ask: impl FnOnce() -> Result<Value>,
) -> Result<Value> {
    if matches!(request, Request::Inspect { .. }) {
        ask()
    } else {
        claimed_mutation(controller, ask)
    }
}

/// Keep an uninstall mutation pending until its dispatcher acknowledges the
/// complete outcome. Executor liveness survives death of the calling process.
fn claimed_mutation<T>(controller: &Controller, act: impl FnOnce() -> Result<T>) -> Result<T> {
    let claim = controller.quarantine()?;
    let _executors = crate::control::ExecutorClaims::new(std::slice::from_ref(&claim))?;
    let outcome = act();
    if outcome.is_ok() {
        controller.clear_if(&claim)?;
    }
    outcome
}

/// What uninstall knows of the helper before it changes anything. Only the
/// person's words and the plan come from it: whatever uninstall acts on is
/// looked at again then, under control of the helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Facts {
    /// The settings load, so the helper is known.
    settings: bool,
    /// The helper's account exists, and is not known to be another's.
    account: bool,
    /// The account with the helper's user ID is not verified as the helper
    /// Atlas recorded: it is left alone.
    other: bool,
    /// A desktop login of the helper's user ID runs.
    logged_in: bool,
    /// The helper is on screen.
    on_screen: bool,
    /// The person asked for the helper account to be deleted too.
    remove_helper: bool,
}

/// One step of uninstalling, in the order [`plan`] gives them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// Stop: the helper is on screen.
    OnScreen,
    /// Take control, make sure no action's outcome is unknown, and stop
    /// input. Control is held until every step has run.
    Settle,
    /// Log the helper out if it is logged in now, and see it gone, or stop
    /// before removing anything.
    LogOut,
    /// Retire the helper's background job, once it is seen logged out.
    Retire,
    /// Everything else but the caches: the menu bar, the plugins, the app,
    /// the settings.
    Rest,
    /// Delete the helper account, once it is seen logged out.
    RemoveHelper,
    /// Say how to delete the helper account, once it is seen logged out.
    Hint,
    /// The caches, where control of the helper is kept: last of all.
    Caches,
}

/// The steps for `facts`, in order: nothing of the helper goes while it is
/// on screen or logged in, and its account is deleted, or the way to delete
/// it shown, only after it is logged out. The log-out is always planned:
/// whether the helper is logged in is read again when it runs.
fn plan(facts: &Facts) -> Vec<Step> {
    if facts.settings && facts.on_screen && !facts.other {
        return vec![Step::OnScreen];
    }

    let mut steps = Vec::new();
    if facts.settings {
        steps.push(Step::Settle);
        if !facts.other {
            steps.push(Step::LogOut);
        }
        steps.push(Step::Retire);
    }
    steps.push(Step::Rest);

    if facts.settings && facts.account {
        steps.push(if facts.remove_helper && !facts.other {
            Step::RemoveHelper
        } else {
            Step::Hint
        });
    }
    steps.push(Step::Caches);

    steps
}

/// What carrying out the steps does to this Mac, one method a step, so the
/// order and the checks between steps are [`execute`]'s alone.
trait Teardown {
    /// [`Step::Settle`]: take control and keep it until [`Teardown::release`].
    async fn settle(&mut self) -> Result<()>;
    /// Whether the helper's user ID is seen logged out now: no loginwindow
    /// and no Dock, twice half a second apart. A look that fails is not.
    fn logged_out(&mut self) -> bool;
    /// [`Step::LogOut`], for a helper seen logged in: logged out, or why
    /// not.
    async fn log_out(&mut self) -> Result<()>;
    /// [`Step::Retire`].
    fn retire(&mut self) -> Result<()>;
    /// [`Step::Rest`].
    fn rest(&mut self) -> Result<()>;
    /// [`Step::RemoveHelper`], for a helper seen logged out.
    fn remove_helper(&mut self);
    /// The account kept: it was seen logged in when it was to be deleted.
    fn keep_helper(&mut self);
    /// [`Step::Hint`], for a helper seen logged out.
    fn hint(&mut self);
    /// [`Step::Caches`].
    fn caches(&mut self);
    /// Let control of the helper go: every step has run.
    fn release(&mut self);
}

/// Carry out `steps` for `facts` through `world`. Whether the helper is
/// logged out is looked at again right before each step that needs it,
/// under control of the helper, so a login that started after the plan
/// was made is logged out first, or stops everything.
async fn execute(steps: &[Step], facts: &Facts, world: &mut impl Teardown) -> Result<()> {
    for step in steps {
        match step {
            Step::OnScreen => {
                return Err(problem(
                    "Switch back to your own account first, then run atlas uninstall again.",
                ));
            }
            Step::Settle => world.settle().await?,
            Step::LogOut => {
                if !world.logged_out() {
                    world.log_out().await?;
                }
            }
            Step::Retire => {
                if !facts.other && !world.logged_out() {
                    return Err(still_logged_in());
                }
                world.retire()?;
            }
            Step::Rest => world.rest()?,
            Step::RemoveHelper => {
                if world.logged_out() {
                    world.remove_helper();
                } else {
                    world.keep_helper();
                }
            }
            Step::Hint => {
                if facts.other || world.logged_out() {
                    world.hint();
                }
            }
            Step::Caches => world.caches(),
        }
    }

    world.release();
    Ok(())
}

/// Whether the account with the helper's user ID is the helper Atlas
/// recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Known {
    /// It is, as recorded.
    Verified(Identity),
    /// Nothing is recorded. A daemon answering at this user ID cannot
    /// establish which account owns it now; it stays untouched.
    Unrecorded(Identity),
    /// It is another account, or it or its record cannot be read.
    Other,
}

/// What the account's standing against its record makes it.
fn known(standing: Standing) -> Known {
    match standing {
        Standing::Same(identity) => Known::Verified(identity),
        Standing::Unrecorded(identity) => Known::Unrecorded(identity),
        Standing::Replaced | Standing::Unknown => Known::Other,
    }
}

/// Undo install and init. `keep_config` keeps settings and logs;
/// `remove_helper` deletes the helper account too, once it is logged out.
pub async fn run(
    config_path: &Path,
    keep_config: bool,
    yes: bool,
    remove_helper: bool,
) -> Result<Value> {
    let found = survey(config_path)?;
    let uid = found.client.as_ref().map(|client| client.config.helper_uid);
    let other = matches!(found.known, Some(Known::Other | Known::Unrecorded(_)));
    let facts = Facts {
        settings: uid.is_some(),
        account: found.helper.is_some(),
        other,
        logged_in: uid.is_some_and(|uid| crate::session::logged_out(uid) != Some(true)),
        on_screen: uid.is_some() && crate::session::console_owner() == uid,
        remove_helper,
    };
    let steps = plan(&facts);

    if steps.contains(&Step::OnScreen) {
        return Err(problem(
            "Switch back to your own account first, then run atlas uninstall again.",
        ));
    }

    if !yes {
        confirm(&found, config_path, keep_config, &facts)?;
    }

    let mut live = Live {
        found: &found,
        config_path,
        keep_config,
        yes,
        removed: Vec::new(),
        warnings: Vec::new(),
        hint: None,
        exclusion: None,
    };

    if let Some(error) = &found.unloaded {
        skipped("Helper job", "left in place: the settings do not load");
        live.warnings
            .push(format!("Helper job left in place: {error}"));
    }
    if let Some(Known::Unrecorded(identity)) = &found.known {
        let message = unrecorded_message(&identity.name);
        skipped("Helper account", &message);
        live.warnings.push(message);
    } else if other {
        skipped("Helper", "left alone: another account has its user ID now");
        live.warnings.push(
            "The account with the helper's user ID is not the helper Atlas set up, so it was \
             left alone"
                .to_owned(),
        );
    }

    execute(&steps, &facts, &mut live).await?;
    let Live {
        removed,
        warnings,
        hint,
        ..
    } = live;

    let deleted = removed.iter().any(|entry| {
        entry
            .as_str()
            .is_some_and(|text| text.starts_with("helper account"))
    });
    let mut report = json!({
        "ok": true,
        "removed": removed,
        "kept": {
            "helperAccount": if deleted { None } else { found.helper.clone() },
            "remove": hint,
        },
    });

    if found.unloaded.is_some() || matches!(found.known, Some(Known::Unrecorded(_))) {
        report["kept"]["helperJob"] = json!(true);
    }

    if !warnings.is_empty() {
        report["warnings"] = json!(warnings);
    }

    Ok(report)
}

/// Uninstall carried out on this Mac.
struct Live<'a> {
    found: &'a Found,
    config_path: &'a Path,
    keep_config: bool,
    yes: bool,
    removed: Vec<Value>,
    warnings: Vec<String>,
    hint: Option<String>,
    /// Control of the helper, from [`Step::Settle`] to the end.
    exclusion: Option<Ownership>,
}

impl Live<'_> {
    /// The helper as recorded, while it is still that account.
    fn identity(&self) -> Option<&Identity> {
        match &self.found.known {
            Some(Known::Verified(identity)) => Some(identity),
            _ => None,
        }
    }
}

impl Teardown for Live<'_> {
    async fn settle(&mut self) -> Result<()> {
        let Some(client) = &self.found.client else {
            return Ok(());
        };
        if matches!(self.found.known, Some(Known::Other | Known::Unrecorded(_))) {
            // Local exclusion only: preserve pending outcomes without
            // contacting or changing an account whose identity is unknown.
            self.exclusion = Some(client.controller.acquire(true)?);
            return Ok(());
        }
        self.exclusion = Some(settle(client, &mut self.warnings).await?);

        // Background jobs are stopped first, so none outlives Atlas in the
        // helper. One that could not be stopped is said, never a failure.
        let jobs =
            crate::jobs::stop_every(client, crate::daemon::protocol::StopReason::Stopped).await;
        let mut said = json!({});
        jobs.say(&mut said);
        if jobs.asked > 0 {
            done(
                "Background jobs",
                &format!("stopped {}", said["jobsStopped"].as_u64().unwrap_or(0)),
            );
        }
        for warning in said["warnings"].as_array().into_iter().flatten() {
            if let Some(warning) = warning.as_str() {
                self.warnings.push(warning.to_owned());
            }
        }

        Ok(())
    }

    fn logged_out(&mut self) -> bool {
        self.found
            .client
            .as_ref()
            .is_some_and(|client| logged_out_twice(client.config.helper_uid))
    }

    async fn log_out(&mut self) -> Result<()> {
        let (Some(client), Some(ownership)) = (&self.found.client, &self.exclusion) else {
            return Err(problem("Atlas lost control of the helper"));
        };
        let target = match (&self.found.helper, self.identity()) {
            (None, _) => Some(Target::Orphan(client.config.helper_uid)),
            (Some(_), Some(identity)) => Some(Target::Helper(identity.clone())),
            (Some(_), None) => None,
        };

        log_out_helper(client, ownership, self.yes, target).await
    }

    fn retire(&mut self) -> Result<()> {
        let Some(client) = &self.found.client else {
            return Ok(());
        };
        let job = retiring(self.found.known.as_ref())?;
        let _ownership = self
            .exclusion
            .as_ref()
            .ok_or_else(|| problem("Atlas lost control of the helper"))?;

        retire(
            client,
            &self.found.executable,
            job,
            &mut self.removed,
            &mut self.warnings,
        )
    }

    fn rest(&mut self) -> Result<()> {
        rest(
            self.found,
            self.config_path,
            self.keep_config,
            &mut self.removed,
            &mut self.warnings,
        )
    }

    fn remove_helper(&mut self) {
        let Some(name) = &self.found.helper else {
            return;
        };
        let Some(identity) = self.identity().cloned() else {
            skipped(
                "Helper account",
                "kept: Atlas can't tell it is still the helper",
            );
            self.warnings.push(format!(
                "The helper account {name} was kept: Atlas can't tell it is still the helper"
            ));
            return;
        };

        let (Some(client), Some(_ownership)) = (&self.found.client, &self.exclusion) else {
            self.warnings
                .push("The helper account was kept: Atlas lost control of it".into());
            return;
        };
        remove_account(
            &client.controller,
            &identity,
            &mut self.removed,
            &mut self.warnings,
        );
    }

    fn keep_helper(&mut self) {
        if let Some(name) = &self.found.helper {
            skipped("Helper account", "kept: it is still logged in");
            self.warnings.push(format!(
                "The helper account {name} was kept: it is still logged in"
            ));
        }
    }

    fn hint(&mut self) {
        self.hint = self.found.helper.as_ref().map(|name| {
            if matches!(self.found.known, Some(Known::Unrecorded(_))) {
                unrecorded_message(name)
            } else {
                format!("sudo sysadminctl -deleteUser {name}")
            }
        });
    }

    fn caches(&mut self) {
        // Caches hold the record of an action of unknown outcome, and the
        // lock that keeps control of the helper: both stay, and the rest
        // goes last, while control is still held. All of it stays while the
        // helper job they describe stays.
        if self.keep_config || self.found.unloaded.is_some() {
            return;
        }
        let Ok(home) = config::home() else {
            return;
        };

        let caches = home.join("Library/Caches/Atlas");
        match remove_caches(&caches, &mut self.removed) {
            Ok(false) => {}
            Ok(true) => self.warnings.push(format!(
                "The last action's outcome is unknown, so its record stays in {} until Atlas \
                 recovers it",
                text::tilde(&caches)
            )),
            Err(error) => self.warnings.push(format!(
                "{} was not removed: {}",
                text::tilde(&caches),
                error.error
            )),
        }
    }

    fn release(&mut self) {
        self.exclusion = None;
    }
}

/// Everything uninstall removes that is not the helper's, but the caches:
/// a waiting opener in another account, the menu bar, the plugins and the
/// command, the app and the settings.
fn rest(
    found: &Found,
    config_path: &Path,
    keep_config: bool,
    removed: &mut Vec<Value>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    forget_requested_opener(&found.executable, found.client.as_ref(), removed, warnings);
    stop_menubar(found.client.as_ref());
    disconnect(&found.executable, removed, warnings)?;

    // Stopped while the agents were changed: the app and settings stay.
    if crate::agents::stopping() {
        return Err(problem("Stopped before Atlas.app was removed"));
    }

    trash(&found.app, &found.leftovers, removed)?;
    remove_settings(config_path, keep_config, &found.data, removed)?;

    Ok(())
}

/// What uninstall finds before it asks.
struct Found {
    app: PathBuf,
    executable: PathBuf,
    /// For the settings, when they load.
    client: Option<Client>,
    /// Why settings that exist do not load: they name no helper, so its
    /// job stays.
    unloaded: Option<String>,
    /// The helper account's name, when it still exists and is not known
    /// to be another account.
    helper: Option<String>,
    /// Whether the account with the helper's user ID is the one recorded;
    /// `None` without settings or an account.
    known: Option<Known>,
    data: PathBuf,
    leftovers: Vec<PathBuf>,
}

/// Find what uninstall would remove and what it keeps, changing nothing.
fn survey(config_path: &Path) -> Result<Found> {
    let app = crate::update::installed_app()?;
    let executable = crate::update::installed_executable()?;

    // Settings that exist but do not load name no helper, so its job stays.
    let (client, unloaded) = match Config::load(config_path) {
        Ok(config) => (Some(Client::new(config)?), None),
        Err(error) if error.code == Code::ConfigInvalid => (None, Some(error.error)),
        Err(_) => (None, None),
    };

    let config = client.as_ref().map(|client| &client.config);
    let account =
        config.and_then(|config| setup::account_with_uid(config.helper_uid).ok().flatten());
    let known = account
        .as_ref()
        .map(|account| known(setup::identity::standing_of(account)));
    let helper = account
        .filter(|_| known != Some(Known::Other))
        .map(|account| account.name);
    let data = config::home()?.join("Library/Application Support/Atlas");
    let leftovers = config
        .map(|config| handed_over(&config.identity_probe))
        .unwrap_or_default();

    Ok(Found {
        app,
        executable,
        client,
        unloaded,
        helper,
        known,
        data,
        leftovers,
    })
}

/// Say what goes and what stays, and ask.
fn confirm(found: &Found, config_path: &Path, keep_config: bool, facts: &Facts) -> Result<()> {
    let Found {
        app,
        client,
        unloaded,
        helper,
        data,
        leftovers,
        ..
    } = found;

    // The helper's job is stopped only through settings that load.
    let mut parts = vec![
        if client.is_some() && !facts.other {
            "the helper's background job, the menu bar and the agent plugins".to_owned()
        } else {
            "the menu bar and the agent plugins".to_owned()
        },
        format!("{} (to the Trash)", text::tilde(app)),
    ];

    if !leftovers.is_empty() {
        parts.push("apps and files Atlas copied for the helper to open".to_owned());
    }

    if !keep_config {
        parts.push(format!(
            "settings in {} and logs in {}",
            text::tilde(config_path),
            text::tilde(data)
        ));
    }

    say("This removes from this Mac:");

    for part in parts {
        say(&format!("  · {part}"));
    }

    if let Some(name) = helper {
        let first = if facts.logged_in && !facts.other {
            "The helper is logged out first; its apps quit. "
        } else {
            ""
        };
        say(&if matches!(found.known, Some(Known::Unrecorded(_))) {
            unrecorded_message(name)
        } else if facts.remove_helper && !facts.other {
            format!("{first}Then the helper account {name} and its files are deleted.")
        } else if facts.logged_in {
            format!("{first}The account and its files stay.")
        } else {
            format!("The helper account {name} and its files stay.")
        });
    }

    if unloaded.is_some() {
        say(&format!(
            "The settings in {} do not load, so the helper's background job stays.",
            text::tilde(config_path)
        ));
    }

    if !prompt::confirm("Remove Atlas?", false, "pass --yes")? {
        return Err(prompt::cancelled());
    }

    Ok(())
}

/// Why the helper cannot settle an action whose outcome is unknown: its
/// account is gone, or it is logged out. None while it could: a helper
/// that is logged in but not answering may answer again after atlas
/// restart, and whether it is logged in at all may be unknown.
fn cannot_settle(account_exists: bool, logged_in: Option<bool>) -> Option<&'static str> {
    if !account_exists {
        Some("the helper account no longer exists")
    } else if logged_in == Some(false) {
        Some("the helper is logged out")
    } else {
        None
    }
}

/// Take control of the helper, so nothing controls it while it goes away,
/// and stop its input. An action whose outcome is unknown must be settled
/// first (atlas restart), unless the helper cannot settle it: retiring its
/// job then replays nothing, and the report says so. The control taken,
/// held until the job is retired.
async fn settle(client: &Client, warnings: &mut Vec<String>) -> Result<Ownership> {
    settle_with(
        &client.controller,
        warnings,
        async {
            let uid = client.config.helper_uid;
            let exists = setup::account_with_uid(uid)?.is_some();
            let status = client.status().await;
            let pid = status["inputService"]["pid"]
                .as_u64()
                .and_then(|pid| u32::try_from(pid).ok());
            Ok((exists, crate::session::logged_in(uid), pid))
        },
        |pid| client.stop_input(pid),
    )
    .await
}

async fn settle_with<F>(
    controller: &Controller,
    warnings: &mut Vec<String>,
    observe: impl std::future::Future<Output = Result<(bool, Option<bool>, Option<u32>)>>,
    stop: impl FnOnce(u32) -> F,
) -> Result<Ownership>
where
    F: std::future::Future<Output = Result<()>>,
{
    let exclusion = controller.acquire(true)?;
    let (exists, logged_in, pid) = observe.await?;
    match cannot_settle(exists, logged_in) {
        None => controller.require_known()?,
        Some(why) if controller.require_known().is_err() => {
            skipped("Last action", &format!("outcome unknown; {why}"));
            warnings.push(format!(
                "The last action's outcome is unknown and {why}, so it was not checked"
            ));
        }
        Some(_) => {}
    }

    if let Some(pid) = pid
        && let Err(error) = stop_input_claimed(controller, stop(pid)).await
    {
        warnings.push(format!(
            "Input shutdown was not confirmed; continuing with verified logout: {}",
            error.error
        ));
    }
    Ok(exclusion)
}

/// Publish shutdown before dispatch. A complete shutdown or definite refusal
/// settles only this claim; a missing acknowledgement remains for recovery.
async fn stop_input_claimed(
    controller: &Controller,
    stop: impl std::future::Future<Output = Result<()>>,
) -> Result<()> {
    let claim = controller.quarantine()?;
    let outcome = stop.await;
    if outcome.is_ok() || outcome.as_ref().is_err_and(|error| !error.uncertain) {
        controller.clear_if(&claim)?;
    }
    outcome
}

/// One way of logging the helper out, gentlest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Way {
    /// Through the helper's Atlas, as Log Out does: an app may hold it up.
    Ask,
    /// Through the helper's Atlas, ending the login at once: the helper
    /// ends its own login, with no administrator rights.
    Force,
    /// As root, under the sudo uninstall asks for: launchd ends the login.
    Root,
}

/// What came of one way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Came {
    /// The helper's login is gone.
    Gone,
    /// An app in the helper holds the log-out up.
    Held,
    /// The helper is still logged in: its Atlas did not answer, refused,
    /// or could not end it.
    Stayed,
}

/// What follows a way that came to something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Next {
    /// The helper is logged out.
    Done,
    /// Ask the person whether to end the login past the app holding it up.
    AskToForce,
    /// Try another way.
    Try(Way),
    /// Every way failed: stop before removing anything.
    Stop,
}

/// What to do after `way` came to `came`. Root acts only once the helper's
/// own Atlas could not log it out; forcing past an app needs `yes` or the
/// person's word.
fn after(way: Way, came: Came, yes: bool) -> Next {
    match (way, came) {
        (_, Came::Gone) => Next::Done,
        (Way::Ask, Came::Held) if yes => Next::Try(Way::Force),
        (Way::Ask, Came::Held) => Next::AskToForce,
        (Way::Ask | Way::Force, _) => Next::Try(Way::Root),
        (Way::Root, _) => Next::Stop,
    }
}

/// Log the helper out under `ownership`, gentlest way first, and see its
/// login gone; or stop before anything is removed. Root closes the login
/// it is named by (`target`), and is never asked without one. Every way is
/// followed by the strict look, whatever it said. A log-out whose outcome
/// was left unknown is settled once the login is seen gone.
async fn log_out_helper(
    client: &Client,
    ownership: &Ownership,
    yes: bool,
    target: Option<Target>,
) -> Result<()> {
    let uid = client.config.helper_uid;
    let before = client.controller.pending()?;
    let mut way = Way::Ask;

    say("Logging out the helper…");
    loop {
        let came = match way {
            Way::Ask | Way::Force => match client.logout_held(ownership, way == Way::Force).await {
                Ok(reply) if reply["loggedOut"] == true => Came::Gone,
                Err(error) if error.code == Code::LogoutHeld => Came::Held,
                _ => Came::Stayed,
            },
            Way::Root => {
                say("The helper's Atlas could not log it out; macOS may ask for your password.");
                root_log_out(client, ownership, target.clone())
            }
        };
        let came = strictly(came, logged_out_twice(uid));

        match after(way, came, yes) {
            Next::Done => break,
            Next::AskToForce => {
                if !prompt::confirm(
                    "An app in the helper is holding up the log-out. Log out anyway?",
                    false,
                    "pass --yes",
                )? {
                    return Err(still_logged_in());
                }
                way = Way::Force;
            }
            Next::Try(next) => way = next,
            Next::Stop => return Err(still_logged_in()),
        }
    }

    // Its outcome is known now: the login is gone.
    for generation in client.controller.pending()? {
        if !before.contains(&generation) {
            client.controller.clear_if(&generation)?;
        }
    }

    done("Helper", "logged out");
    Ok(())
}

/// The helper whose job is retired, from what is known of the account with
/// its user ID (`known`, `None` when it was gone): only the helper as
/// recorded. Another account's is never touched, only Atlas's own records
/// of the helper go; one never recorded is not taken for the helper, and
/// its account and job stay while Atlas itself can be removed.
fn retiring(known: Option<&Known>) -> Result<Option<&Identity>> {
    match known {
        Some(Known::Verified(identity)) => Ok(Some(identity)),
        Some(Known::Other | Known::Unrecorded(_)) | None => Ok(None),
    }
}

/// Explain why an unrecorded account is left alone and how to remove it.
fn unrecorded_message(name: &str) -> String {
    format!(
        "The account {name} and its background job are left alone because Atlas has no recorded \
         identity for it. To remove the account and its files, log it out yourself, then delete \
         {name} in System Settings > Users & Groups."
    )
}

/// Uninstall's refusal when the helper stays logged in.
fn still_logged_in() -> Error {
    Error::new(
        Code::Logout,
        "The helper is still logged in, so Atlas stopped before removing anything. Log it out \
         (atlas logout), then run atlas uninstall again.",
    )
}

/// Whether the helper's user ID `uid` has no loginwindow and no Dock, seen
/// twice half a second apart.
fn logged_out_twice(uid: u32) -> bool {
    (0..2).all(|_| {
        std::thread::sleep(std::time::Duration::from_millis(500));
        crate::session::logged_out(uid) == Some(true)
    })
}

/// What a way came to, once the helper's user ID was looked at strictly
/// (`logged_out`): only that look says it is gone.
fn strictly(came: Came, logged_out: bool) -> Came {
    match (came, logged_out) {
        (_, true) => Came::Gone,
        (Came::Gone, false) => Came::Stayed,
        (came, false) => came,
    }
}

/// End the helper's login as root, through sudo, under the control held
/// (`ownership`) and a pending claim kept until it is seen gone: checked
/// here first as not on screen and not this account, then again by root
/// itself. A helper account that still exists, as recorded (`target`), is
/// only booted out by launchd; one already deleted is closed as any deleted
/// account's login. Without a target root is not asked.
fn root_log_out(client: &Client, ownership: &Ownership, target: Option<Target>) -> Came {
    let uid = client.config.helper_uid;
    let me = nix::unistd::getuid().as_raw();
    let Some(target) = target else {
        say("Atlas can't tell that the account with the helper's user ID is still its helper.");
        return Came::Stayed;
    };
    if uid == me || crate::session::console_owner() == Some(uid) {
        return Came::Stayed;
    }

    let ended = crate::onboarding::end_as_root(
        &client.controller,
        ownership,
        || as_root::close(&[target], Via::Sudo { ask: true }),
        || logged_out_twice(uid),
    );

    match ended {
        Ok(()) => Came::Gone,
        Err(_) => Came::Stayed,
    }
}

/// Delete the logged-out helper account `helper` through sudo, while its
/// user ID is still that account; what is left of it is closed by root
/// once the account is gone.
fn remove_account(
    controller: &Controller,
    helper: &Identity,
    removed: &mut Vec<Value>,
    warnings: &mut Vec<String>,
) {
    let (uid, name) = (helper.uid, &helper.name);
    let me = nix::unistd::getuid().as_raw();
    if uid == me || crate::session::console_owner() == Some(uid) {
        skipped("Helper account", "kept: it is still logged in");
        warnings.push(format!(
            "The helper account {name} was kept: it is still logged in"
        ));
        return;
    }

    say("Deleting the helper account (macOS may ask for your password)…");
    match claimed_mutation(controller, || {
        as_root::remove_helper(helper, Via::Sudo { ask: true }).map_err(|refusal| {
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
            done("Helper account", &format!("{name} deleted"));
        }
        Err(error) => {
            let why = error.error;
            skipped("Helper account", &format!("deletion not confirmed: {why}"));
            warnings.push(format!("Deletion of the helper account {name} was not confirmed: {why}. Its outcome stays pending until Atlas recovers it."));
        }
    }
}

/// Retire the job of the helper `job`, as recorded, as the helper and
/// outside any login of it (never in another account given the helper's
/// user ID, nor without a record), and remove what pointed at it: the
/// setup record and the shared command.
fn retire(
    client: &Client,
    executable: &Path,
    job: Option<&Identity>,
    removed: &mut Vec<Value>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let config = &client.config;

    if let Some(helper) = job {
        say("Stopping the helper's background job (macOS may ask for your password)…");
        if let Some(opener) = retire_helper_job(executable, helper, &client.controller)? {
            removed.push(json!("helper job"));
            done("Helper job", "stopped");
            match opener {
                Ok(true) => removed.push(json!("Atlas's one-time start in the helper")),
                Ok(false) => {}
                Err(why) => warnings.push(why),
            }
        } else {
            skipped("Helper job", "the helper account no longer exists");
            warnings.push(format!(
                "The helper account (user ID {}) no longer exists, so no job was stopped",
                config.helper_uid
            ));
        }
    }

    // The setup record describes a job that is gone now: kept as evidence
    // (with --keep-config), but no longer read as a setup.
    if let Err(error) = setup::migrate::archive_uninstalled(config.helper_uid) {
        warnings.push(format!(
            "The setup record was not set aside: {}",
            error.error
        ));
    }

    if remove_link_to(&config.identity_probe, executable)? {
        removed.push(json!(config.identity_probe));
    }

    Ok(())
}

/// Stop a running menu bar and its opening at login. Turning it off here is
/// not the person's choice: unless they had turned it off themselves, the
/// record of a choice goes, so setting Atlas up again turns it on again
/// (settings kept with --keep-config keep that record otherwise).
fn stop_menubar(client: Option<&Client>) {
    let stopped = client
        .and_then(|client| client.output_dir().ok().map(Path::to_path_buf))
        .and_then(|output| crate::menubar::stop(&output))
        .is_some();

    let decided = crate::menubar::login_decided();
    let enabled = crate::menubar::set_login(crate::menubar::Login::Status)
        .ok()
        .and_then(|reply| reply["enabled"].as_bool());
    let login_off = crate::menubar::set_login(crate::menubar::Login::Off)
        .is_ok_and(|reply| reply["ok"] == true);

    if login_off && !keeps_login_choice(decided, enabled) {
        crate::menubar::forget_login();
    }

    let said = match (stopped, login_off) {
        (true, true) => "stopped and no longer opens at login",
        (true, false) => "stopped",
        (false, true) => "no longer opens at login",
        (false, false) => return,
    };

    done("Menu bar", said);
}

/// Whether the record of an Open at Login choice stays once uninstall turns
/// it off: only when the person had decided it, and decided off.
fn keeps_login_choice(decided: bool, enabled: Option<bool>) -> bool {
    decided && enabled == Some(false)
}

/// Agents forget the plugin (only those using a marketplace of Atlas's
/// own change), and `atlas` leaves the terminal.
fn disconnect(
    executable: &Path,
    removed: &mut Vec<Value>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    // An error here means no agent is installed, so there is nothing to remove.
    if let Ok(reply) = crate::agents::change(&[], crate::agents::Change::Disconnect) {
        let changed = reply["agents"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|agent| agent["changed"] == true);

        if reply["ok"] == true {
            if changed {
                removed.push(json!("agent plugins"));
                done("Agent plugins", "removed");
            }
        } else {
            let failed = failed_agents(&reply);
            skipped(
                "Agent plugins",
                &format!("not removed from {failed}; run atlas agents remove"),
            );
            warnings.push(format!(
                "The plugin was not removed from {failed}; run atlas agents remove"
            ));
        }

        for (agent, place) in kept_agents(&reply) {
            skipped(
                "Agent plugins",
                &format!("left in {agent}: its atlas marketplace is from {place}"),
            );
            warnings.push(format!(
                "{agent}'s marketplace named atlas is from {place}, not Atlas, so it was left alone"
            ));
        }
    }

    let command = config::home()?.join(".local/bin/atlas");
    if remove_link_to(&command, executable)? {
        removed.push(json!(command));
    }

    Ok(())
}

/// The agents a plugin change failed for, as "Claude Code and Codex".
fn failed_agents(reply: &Value) -> String {
    let names: Vec<&str> = reply["agents"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|agent| agent["ok"] != true)
        .filter_map(|agent| agent["agent"].as_str())
        .map(shown)
        .collect();

    if names.is_empty() {
        "an agent".to_owned()
    } else {
        names.join(" and ")
    }
}

/// The agents a plugin removal left alone, as their names and where their
/// marketplace named atlas comes from.
fn kept_agents(reply: &Value) -> Vec<(String, String)> {
    reply["agents"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|agent| {
            let place = agent["kept"].as_str()?;
            let name = agent["agent"].as_str()?;

            Some((shown(name).to_owned(), place.to_owned()))
        })
        .collect()
}

/// Take away the one-time start the window left in an account this
/// account asked for but whose setup it never finished (not the helper its
/// settings name, whose start goes with its job): with Atlas.app gone it
/// could never run, and would stay among that account's login items. sudo
/// may ask the person for their password.
fn forget_requested_opener(
    executable: &Path,
    client: Option<&Client>,
    removed: &mut Vec<Value>,
    warnings: &mut Vec<String>,
) {
    let me = nix::unistd::getuid().as_raw();
    let Some(pairing) = crate::onboarding::request().filter(|pairing| {
        pairing.controller_uid == me
            && client.is_none_or(|client| client.config.helper_uid != pairing.helper_uid)
    }) else {
        return;
    };
    let Ok(Some(account)) = setup::account_with_uid(pairing.helper_uid) else {
        return;
    };

    let request = Request::ForgetOpener { uid: account.uid };
    let Some(expected) = setup::identity::verified(account.uid) else {
        warnings.push("Atlas's one-time start was left in place: choose the helper account explicitly before removing it".into());
        return;
    };
    let outcome = Controller::new(account.uid).and_then(|controller| {
        forget_opener(&controller, || {
            let (succeeded, reply) =
                setup::request_as_helper(executable, &account, &request, false, &expected)?;
            if !succeeded || reply["ok"] != true {
                return Err(problem(reply["error"].as_str().unwrap_or("no reply")));
            }
            Ok(reply)
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

/// Own the requested helper independently of the configured helper, retaining
/// both exclusion and an executor-bound claim until removal is acknowledged.
fn forget_opener(controller: &Controller, ask: impl FnOnce() -> Result<Value>) -> Result<Value> {
    let _ownership = controller.acquire(true)?;
    claimed_mutation(controller, ask)
}

/// The name a person knows an agent by, from its name in a reply.
fn shown(name: &str) -> &str {
    crate::agents::Agent::named(name).map_or(name, |agent| agent.display_name())
}

/// The app goes to the Trash, where it can be recovered; what it left for
/// the helper goes with it.
fn trash(app: &Path, leftovers: &[PathBuf], removed: &mut Vec<Value>) -> Result<()> {
    let trash = config::home()?
        .join(".Trash")
        .join(format!("Atlas-{}.app", crate::files::token()));

    fs::rename(app, &trash).map_err(|_| {
        problem(format!(
            "Could not move {} to the Trash; remove it yourself",
            app.display()
        ))
    })?;

    removed.push(json!(app));
    done("Atlas.app", "moved to the Trash");

    // Copies an unfinished update or install.sh staged beside it, by their
    // reserved names only.
    for staged in crate::update::atlas_copies(app) {
        if fs::remove_dir_all(&staged).is_ok() {
            removed.push(json!(staged));
        }
    }

    let mut cleared = false;
    for leftover in leftovers {
        if fs::remove_dir_all(leftover).is_ok() {
            removed.push(json!(leftover));
            cleared = true;
        }
    }

    if cleared {
        done("Copies for the helper", "removed");
    }

    // The request naming the helper, when this account made it.
    let request = Path::new(crate::onboarding::REQUEST);
    if remove_own_file(request).unwrap_or(false) {
        removed.push(json!(request));
    }

    // The shared folder itself, once nothing but Atlas's is left in it:
    // another account could not set Atlas up while it is there.
    if let Some(shared) = Path::new(crate::handoff::ROOT).parent()
        && remove_own_empty_folder(shared)
    {
        removed.push(json!(shared));
    }

    Ok(())
}

/// Remove `folder` only if it is a real folder this account owns holding
/// nothing, or only the .DS_Store Finder leaves. Anything else stays.
fn remove_own_empty_folder(folder: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;

    let ours = fs::symlink_metadata(folder).is_ok_and(|metadata| {
        metadata.is_dir() && metadata.uid() == nix::unistd::getuid().as_raw()
    });
    if !ours {
        return false;
    }

    let Ok(entries) = fs::read_dir(folder) else {
        return false;
    };

    let mut finder = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else {
            return false;
        };

        let plain_file = entry.file_type().is_ok_and(|kind| kind.is_file());
        if entry.file_name() != ".DS_Store" || !plain_file {
            return false;
        }

        finder.push(entry.path());
    }

    finder.iter().all(|file| fs::remove_file(file).is_ok()) && fs::remove_dir(folder).is_ok()
}

/// The installed settings and the logs and records, unless kept.
fn remove_settings(
    config_path: &Path,
    keep_config: bool,
    data: &Path,
    removed: &mut Vec<Value>,
) -> Result<()> {
    // Only the installed settings; a checkout's own file is never removed.
    let installed = config::installed_path().ok();
    if !keep_config
        && installed.as_deref() == Some(config_path)
        && fs::symlink_metadata(config_path).is_ok()
    {
        fs::remove_file(config_path)?;
        removed.push(json!(config_path));
        done("Settings", "removed");
    }

    if !keep_config && data.exists() {
        fs::remove_dir_all(data)?;
        removed.push(json!(data));
        done("Logs and records", "removed");
    }

    Ok(())
}

/// Whether a file in Atlas's caches keeps control of a helper: a lock,
/// whose path must stay the same file while anyone may hold it (a lock made
/// anew in its place would let a second controller in), or the record of an
/// action whose outcome is unknown, with its claims, kept until recovery
/// settles it.
fn keeps_control(name: &str) -> bool {
    name.ends_with(".lock") || name.ends_with(".uncertain.json") || name.ends_with(".live")
}

/// Atlas's caches, but what keeps control of a helper ([`keeps_control`]):
/// update's scratch and the like. A linked folder is refused before cleanup.
/// Whether the record of an action whose outcome is unknown stays.
fn remove_caches(caches: &Path, removed: &mut Vec<Value>) -> Result<bool> {
    let Ok(metadata) = fs::symlink_metadata(caches) else {
        return Ok(false);
    };

    if metadata.file_type().is_symlink() {
        return Err(problem(
            "Atlas's cache directory is a symbolic link. Remove the link before uninstalling.",
        ));
    }

    if !metadata.is_dir() {
        fs::remove_file(caches)?;
        removed.push(json!(caches));
        done("Caches", "removed");
        return Ok(false);
    }

    let (mut kept, mut unknown) = (false, false);
    for entry in fs::read_dir(caches)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let plain = entry.file_type().is_ok_and(|kind| kind.is_file());
        if plain && keeps_control(&name) {
            kept = true;
            unknown |= !name.ends_with(".lock");
            continue;
        }

        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            fs::remove_dir_all(entry.path())?;
        } else {
            fs::remove_file(entry.path())?;
        }
    }

    if kept {
        done("Caches", "removed; Atlas's locks stay");
    } else {
        fs::remove_dir(caches)?;
        done("Caches", "removed");
    }
    removed.push(json!(caches));

    Ok(unknown)
}

/// What Atlas left for the helper: the copies `open` made, and environment
/// plans and requests beside the shared command. Only folders that are
/// provably Atlas's own are listed.
fn handed_over(shared_command: &Path) -> Vec<PathBuf> {
    let mut leftovers: Vec<PathBuf> = crate::handoff::root_if_owned().into_iter().collect();

    if let Some(shared) = shared_command.parent() {
        leftovers.extend(crate::environment::leftovers(shared));
    }

    leftovers
}

#[cfg(test)]
mod tests {
    #[test]
    fn removing_a_secondary_opener_keeps_control_and_pending_work_after_interruption() {
        if !crate::test_support::isolated(
            "uninstall::tests::removing_a_secondary_opener_keeps_control_and_pending_work_after_interruption",
        ) {
            return;
        }
        let directory = crate::test_support::interrupted_executor(
            "uninstall::tests::an_opener_removal_test_caller",
            "opener",
        );
        assert!(!directory.path().join("opener").exists());

        let controller = Controller::at(7001, &directory.path().join("state"));
        let teardown = controller.acquire(true).unwrap();
        assert!(
            forget_opener(&controller, || panic!(
                "a competing transaction cannot dispatch"
            ))
            .is_err()
        );
        drop(teardown);
        let interrupted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            forget_opener(&controller, || -> Result<Value> {
                fs::write(directory.path().join("removed"), b"").unwrap();
                panic!("the removal acknowledgement was lost");
            })
        }));
        assert!(interrupted.is_err());
        assert_eq!(controller.unresolved().unwrap().len(), 1);
        forget_opener(&controller, || Ok(json!({"ok": true, "removed": true}))).unwrap();
        assert_eq!(
            controller.pending().unwrap().len(),
            1,
            "only acknowledged work is settled"
        );
    }

    /// A separate caller with an executor that removes only a temporary opener.
    #[test]
    fn an_opener_removal_test_caller() {
        let Some(directory) = crate::test_support::directory() else {
            return;
        };
        let controller = Controller::at(7001, &directory.join("state"));
        let _ = forget_opener(&controller, || {
            crate::test_support::pause_executor(&directory);
            Ok(json!({"ok": true, "removed": true}))
        });
    }

    #[test]
    fn input_shutdown_stays_pending_when_its_caller_dies_after_dispatch() {
        use std::{process::Stdio, time::Duration};
        let directory = tempfile::tempdir().unwrap();
        let mut caller =
            crate::test_support::rerun("uninstall::tests::an_input_shutdown_test_caller")
                .arg("--nocapture")
                .env("ATLAS_MUTATION_TEST_DIRECTORY", directory.path())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
        for _ in 0..500 {
            if directory.path().join("dispatched").exists() || caller.try_wait().unwrap().is_some()
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let controller = Controller::at(7001, &directory.path().join("state"));
        let excluded = controller.acquire(true).is_err();
        let before = controller.pending().unwrap();
        let _ = caller.kill();
        let _ = caller.wait();
        assert!(directory.path().join("dispatched").exists());
        assert!(directory.path().join("claimed-before-dispatch").exists());
        assert!(excluded);
        assert_eq!(before.len(), 1);
        assert_eq!(controller.unresolved().unwrap(), before);
        assert!(controller.acquire(false).is_err());
    }

    /// A shutdown dispatch stand-in that deliberately never acknowledges completion.
    #[tokio::test]
    async fn an_input_shutdown_test_caller() {
        let Some(directory) = crate::test_support::directory() else {
            return;
        };
        let controller = Controller::at(7001, &directory.join("state"));
        let _ownership = controller.acquire(true).unwrap();
        let _ = stop_input_claimed(&controller, async {
            if controller.pending().unwrap().len() == 1 {
                fs::write(directory.join("claimed-before-dispatch"), b"").unwrap();
            }
            fs::write(directory.join("dispatched"), b"shutdown sent").unwrap();
            std::future::pending::<Result<()>>().await
        })
        .await;
    }

    #[tokio::test]
    async fn input_shutdown_settles_completion_and_definite_refusal_but_preserves_uncertainty() {
        let directory = tempfile::tempdir().unwrap();
        let controller = Controller::at(7001, directory.path());
        let _ownership = controller.acquire(true).unwrap();
        let other = controller.quarantine().unwrap();
        for outcome in [
            Ok(()),
            Err(problem("refused")),
            Err(problem("reply lost").uncertain()),
        ] {
            let uncertain = outcome.as_ref().is_err_and(|error| error.uncertain);
            let succeeded = outcome.is_ok();
            let before = controller.pending().unwrap().len();
            let result = stop_input_claimed(&controller, async {
                assert_eq!(controller.pending().unwrap().len(), before + 1);
                assert!(controller.acquire(true).is_err());
                outcome
            })
            .await;
            assert_eq!(result.is_ok(), succeeded);
            assert_eq!(
                controller.pending().unwrap().len(),
                before + usize::from(uncertain)
            );
        }
        drop(other);
    }
    #[test]
    fn interrupted_retirement_keeps_every_unacknowledged_mutation_pending() {
        if !crate::test_support::isolated(
            "uninstall::tests::interrupted_retirement_keeps_every_unacknowledged_mutation_pending",
        ) {
            return;
        }
        let requests = [
            Request::ForgetOpener { uid: 7001 },
            Request::Retire {
                uid: 7001,
                label: LABEL.into(),
                sha256: "a".repeat(64),
                missing_ok: true,
            },
        ];
        for request in requests {
            let dir = tempfile::tempdir().unwrap();
            let controller = Controller::at(7001, &dir.path().join("state"));
            let held = controller.acquire(true).unwrap();
            let claimed_before_dispatch = std::cell::Cell::new(false);
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                retirement_request(&controller, &request, || {
                    claimed_before_dispatch.set(controller.pending().unwrap().len() == 1);
                    fs::write(dir.path().join("changed"), b"retired").unwrap();
                    panic!("the worker changed a file but its caller received no acknowledgement");
                })
            }));
            assert!(outcome.is_err());
            assert!(dir.path().join("changed").exists());
            assert_eq!(controller.unresolved().unwrap().len(), 1);
            assert!(claimed_before_dispatch.get());
            drop(held);
            assert!(controller.acquire(false).is_err());
            let held = controller.acquire(true).unwrap();
            controller.clear().unwrap();
            retirement_request(&controller, &request, || Ok(json!({"ok": true}))).unwrap();
            assert!(
                controller.pending().unwrap().is_empty(),
                "acknowledged retirement settles its own claim"
            );
            drop(held);
            assert!(controller.acquire(false).is_ok());
        }
    }

    #[test]
    fn deletion_without_a_complete_acknowledgement_stays_pending() {
        let dir = tempfile::tempdir().unwrap();
        let controller = Controller::at(7001, &dir.path().join("state"));
        let held = controller.acquire(true).unwrap();
        let stand_in = dir.path().join("account");
        fs::write(&stand_in, b"temporary stand-in").unwrap();
        let claimed_before_dispatch = std::cell::Cell::new(false);
        let outcome: Result<()> = claimed_mutation(&controller, || {
            claimed_before_dispatch.set(controller.pending().unwrap().len() == 1);
            fs::remove_file(&stand_in).unwrap();
            Err(problem(
                "deletion completed but cleanup or its acknowledgement was lost",
            ))
        });
        assert!(outcome.is_err());
        assert!(!stand_in.exists());
        assert_eq!(controller.unresolved().unwrap().len(), 1);
        assert!(claimed_before_dispatch.get());
        let other = controller.quarantine().unwrap();
        claimed_mutation(&controller, || Ok(())).unwrap();
        assert_eq!(
            controller.pending().unwrap().len(),
            2,
            "success settles only its own generation"
        );
        drop(other);
        drop(held);
        assert!(controller.acquire(false).is_err());
    }

    #[tokio::test]
    async fn an_unrecorded_helper_never_uses_its_daemons_word_to_adopt_an_account() {
        let dir = tempfile::tempdir().unwrap();
        let mut client = Client::new(Config {
            helper_uid: 7001,
            socket_path: dir.path().join("no-daemon.sock"),
            input_socket: dir.path().join("no-input.sock"),
            identity_probe: dir.path().join("atlas"),
            output_dir: dir.path().join("runs"),
        })
        .unwrap();
        client.controller = Controller::at(7001, &dir.path().join("control"));
        let pending = client.controller.quarantine().unwrap();
        let generation = pending.generation().to_owned();
        drop(pending);
        // A surviving daemon only proves its numeric user ID, even when the
        // current directory entry has the same name as the deleted helper.
        let found = Found {
            app: dir.path().join("Atlas.app"),
            executable: dir.path().join("atlas"),
            client: Some(client),
            unloaded: None,
            helper: Some("helper".into()),
            known: Some(Known::Unrecorded(Identity {
                uid: 7001,
                name: "helper".into(),
                generated_uid: "11111111-2222-3333-4444-555555555555".into(),
            })),
            data: dir.path().join("data"),
            leftovers: vec![],
        };
        let mut live = Live {
            found: &found,
            config_path: &dir.path().join("settings"),
            keep_config: false,
            yes: true,
            removed: vec![],
            warnings: vec![],
            hint: None,
            exclusion: None,
        };
        live.settle().await.unwrap();
        assert!(live.exclusion.is_some());
        let controller = &live.found.client.as_ref().unwrap().controller;
        assert_eq!(controller.pending().unwrap(), [generation]);
        assert_eq!(controller.acquire(true).unwrap_err().code, "busy");
        assert!(live.identity().is_none());
        assert!(live.removed.is_empty());
        live.hint();
        let hint = live.hint.unwrap();
        assert!(hint.contains("no recorded identity"));
        assert!(hint.contains("System Settings > Users & Groups"));
    }

    #[test]
    fn a_linked_cache_is_refused_without_unlinking_control_state() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("state");
        let controller = crate::control::Controller::at(7001, &target);
        let held = controller.acquire(true).unwrap();
        let claim = controller.quarantine().unwrap();
        let caches = dir.path().join("Atlas");
        std::os::unix::fs::symlink(&target, &caches).unwrap();
        let error = remove_caches(&caches, &mut Vec::new()).unwrap_err();
        assert!(error.error.contains("symbolic link"));
        assert!(
            fs::symlink_metadata(&caches)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(controller.pending().unwrap(), [claim.generation()]);
        assert!(
            crate::control::Controller::at(7001, &caches)
                .acquire(true)
                .is_err()
        );
        assert!(controller.acquire(true).is_err());
        drop(claim);
        drop(held);
    }

    use super::*;

    #[test]
    fn only_links_to_the_app_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let executable = root.join("Atlas.app/Contents/MacOS/atlas");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, b"").unwrap();

        let ours = root.join("ours");
        std::os::unix::fs::symlink(&executable, &ours).unwrap();
        assert!(remove_link_to(&ours, &executable).unwrap());
        assert!(!ours.exists());

        let theirs = root.join("theirs");
        fs::write(&theirs, b"someone's file").unwrap();
        assert!(!remove_link_to(&theirs, &executable).unwrap());
        assert!(theirs.exists(), "a regular file is left alone");

        let elsewhere = root.join("elsewhere");
        std::os::unix::fs::symlink(&theirs, &elsewhere).unwrap();
        assert!(!remove_link_to(&elsewhere, &executable).unwrap());
        assert!(!remove_link_to(&root.join("missing"), &executable).unwrap());
    }

    /// The shared folder goes only when nothing but Finder's .DS_Store is
    /// left in it.
    #[test]
    fn only_an_empty_folder_of_this_accounts_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("Atlas");

        assert!(!remove_own_empty_folder(&folder), "nothing there");

        fs::create_dir(&folder).unwrap();
        assert!(remove_own_empty_folder(&folder));
        assert!(!folder.exists());

        fs::create_dir(&folder).unwrap();
        fs::write(folder.join(".DS_Store"), b"").unwrap();
        assert!(remove_own_empty_folder(&folder));
        assert!(!folder.exists());

        fs::create_dir(&folder).unwrap();
        fs::write(folder.join(".DS_Store"), b"").unwrap();
        fs::write(folder.join("notes.txt"), b"someone's").unwrap();
        assert!(!remove_own_empty_folder(&folder));
        assert!(folder.join("notes.txt").exists() && folder.join(".DS_Store").exists());

        fs::remove_file(folder.join("notes.txt")).unwrap();
        fs::create_dir(folder.join("Open")).unwrap();
        assert!(!remove_own_empty_folder(&folder), "a folder inside stays");

        let link = dir.path().join("link");
        std::os::unix::fs::symlink(dir.path().join("empty"), &link).unwrap();
        fs::create_dir(dir.path().join("empty")).unwrap();
        assert!(!remove_own_empty_folder(&link), "a link is never followed");
        assert!(dir.path().join("empty").exists());

        // A folder another account (here root) owns stays.
        assert!(!remove_own_empty_folder(Path::new("/var/empty")));
    }

    #[test]
    fn caches_go_and_a_link_there_is_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let caches = dir.path().join("Caches/Atlas");
        let mut removed = Vec::new();

        remove_caches(&caches, &mut removed).unwrap();
        assert!(removed.is_empty(), "nothing there");

        fs::create_dir_all(caches.join("update")).unwrap();
        fs::write(caches.join("scratch"), b"").unwrap();
        assert!(!remove_caches(&caches, &mut removed).unwrap());
        assert!(!caches.exists());
        assert_eq!(removed, vec![json!(caches)]);

        let target = dir.path().join("elsewhere");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("keep"), b"").unwrap();
        std::os::unix::fs::symlink(&target, &caches).unwrap();
        assert!(remove_caches(&caches, &mut removed).is_err());
        assert!(target.join("keep").exists());
    }

    #[test]
    fn a_helper_held_while_its_caches_go_stays_held() {
        if !crate::test_support::isolated(
            "uninstall::tests::a_helper_held_while_its_caches_go_stays_held",
        ) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let caches = dir.path().join("Caches/Atlas");
        let controller = crate::control::Controller::at(60001, &caches);
        let held = controller.acquire(true).unwrap();
        fs::create_dir_all(caches.join("update/staging")).unwrap();
        fs::write(caches.join("update/staging/Atlas.zip"), b"").unwrap();

        assert!(!remove_caches(&caches, &mut Vec::new()).unwrap());
        assert!(!caches.join("update").exists(), "the scratch goes");
        let other = crate::control::Controller::at(60001, &caches);
        assert_eq!(
            other.acquire(true).unwrap_err().code,
            "busy",
            "another controller finds the same lock, still held"
        );
        drop(held);
        assert!(other.acquire(true).is_ok());
    }

    #[test]
    fn an_action_of_unknown_outcome_keeps_its_record_through_uninstall() {
        let dir = tempfile::tempdir().unwrap();
        let caches = dir.path().join("Caches/Atlas");
        let controller = crate::control::Controller::at(60001, &caches);
        // One cut short, and one still being carried out.
        let lost = controller.quarantine().unwrap();
        let generation = lost.generation().to_owned();
        drop(lost);
        let running = controller.quarantine().unwrap();

        assert!(remove_caches(&caches, &mut Vec::new()).unwrap());
        let pending = controller.pending().unwrap();
        assert!(pending.contains(&generation), "{pending:?}");
        assert!(pending.contains(&running.generation().to_owned()));
        assert_eq!(
            controller.unresolved().unwrap(),
            [generation],
            "the claim of the one still running stays too"
        );
    }

    #[test]
    fn only_this_accounts_own_files_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("onboarding.json");
        fs::write(&file, b"{}").unwrap();
        assert!(remove_own_file(&file).unwrap());
        assert!(!file.exists());
        assert!(!remove_own_file(&file).unwrap(), "nothing there");

        let target = dir.path().join("target.json");
        fs::write(&target, b"{}").unwrap();
        std::os::unix::fs::symlink(&target, &file).unwrap();
        assert!(!remove_own_file(&file).unwrap(), "a link is left alone");
        assert!(target.exists());

        assert!(!remove_own_file(dir.path()).unwrap(), "only a file");
        // A file another account owns (here root's) stays.
        assert!(!remove_own_file(Path::new("/etc/hosts")).unwrap());
    }

    /// An unknown outcome blocks uninstall while the helper could settle
    /// it: its account exists and it is not known to be logged out, even
    /// when Atlas does not answer there now.
    #[test]
    fn a_helper_that_could_answer_again_must_settle_an_unknown_outcome() {
        assert_eq!(cannot_settle(true, Some(true)), None);
        assert_eq!(cannot_settle(true, None), None, "unknown is not logged out");
        assert_eq!(
            cannot_settle(true, Some(false)),
            Some("the helper is logged out")
        );
        assert_eq!(
            cannot_settle(false, Some(true)),
            Some("the helper account no longer exists")
        );
    }

    fn facts() -> Facts {
        Facts {
            settings: true,
            account: true,
            other: false,
            logged_in: true,
            on_screen: false,
            remove_helper: false,
        }
    }

    fn at(steps: &[Step], step: Step) -> usize {
        steps
            .iter()
            .position(|found| *found == step)
            .unwrap_or_else(|| panic!("no {step:?} in {steps:?}"))
    }

    #[test]
    fn the_helper_is_logged_out_before_its_job_is_retired() {
        let steps = plan(&facts());
        assert_eq!(
            steps,
            [
                Step::Settle,
                Step::LogOut,
                Step::Retire,
                Step::Rest,
                Step::Hint,
                Step::Caches
            ]
        );

        // Logged out when the plan was made: the log-out is still planned,
        // since whether it is logged in is read again when it runs.
        let logged_out = plan(&Facts {
            logged_in: false,
            ..facts()
        });
        assert_eq!(logged_out, steps);
    }

    #[test]
    fn nothing_is_retired_or_deleted_while_the_helper_is_logged_in() {
        for remove_helper in [false, true] {
            let steps = plan(&Facts {
                remove_helper,
                ..facts()
            });
            let log_out = at(&steps, Step::LogOut);
            for later in [Step::Retire, Step::Rest] {
                assert!(log_out < at(&steps, later), "{later:?}");
            }
        }

        // Every way failed: stop, never go on to remove anything.
        assert_eq!(after(Way::Root, Came::Stayed, true), Next::Stop);
        assert_eq!(after(Way::Root, Came::Held, true), Next::Stop);
    }

    #[test]
    fn root_ends_the_login_only_after_the_helpers_own_atlas_could_not() {
        assert_eq!(after(Way::Ask, Came::Gone, false), Next::Done);
        assert_eq!(after(Way::Ask, Came::Stayed, false), Next::Try(Way::Root));
        assert_eq!(after(Way::Ask, Came::Held, true), Next::Try(Way::Force));
        assert_eq!(after(Way::Ask, Came::Held, false), Next::AskToForce);
        assert_eq!(after(Way::Force, Came::Gone, false), Next::Done);
        assert_eq!(after(Way::Force, Came::Stayed, false), Next::Try(Way::Root));
        assert_eq!(after(Way::Force, Came::Held, true), Next::Try(Way::Root));
        assert_eq!(after(Way::Root, Came::Gone, false), Next::Done);
    }

    #[test]
    fn the_way_to_delete_the_helper_is_shown_only_after_it_is_logged_out() {
        let steps = plan(&facts());
        assert!(at(&steps, Step::LogOut) < at(&steps, Step::Hint));
        assert_eq!(steps[steps.len() - 2..], [Step::Hint, Step::Caches]);

        let gone = plan(&Facts {
            account: false,
            ..facts()
        });
        assert!(!gone.contains(&Step::Hint), "no account to delete");
    }

    #[test]
    fn the_helper_account_is_deleted_only_after_it_is_logged_out() {
        let steps = plan(&Facts {
            remove_helper: true,
            ..facts()
        });
        assert!(at(&steps, Step::LogOut) < at(&steps, Step::RemoveHelper));
        assert!(at(&steps, Step::Retire) < at(&steps, Step::RemoveHelper));
        assert!(!steps.contains(&Step::Hint));
    }

    #[test]
    fn nothing_happens_while_the_helper_is_on_screen() {
        let steps = plan(&Facts {
            on_screen: true,
            remove_helper: true,
            ..facts()
        });
        assert_eq!(steps, [Step::OnScreen]);
    }

    #[test]
    fn without_settings_only_this_accounts_own_files_go() {
        let steps = plan(&Facts {
            settings: false,
            account: false,
            other: false,
            logged_in: false,
            on_screen: true,
            remove_helper: true,
        });
        assert_eq!(steps, [Step::Rest, Step::Caches]);
    }

    #[test]
    fn another_account_given_the_helpers_user_id_is_left_alone() {
        let steps = plan(&Facts {
            account: false,
            other: true,
            on_screen: true,
            remove_helper: true,
            ..facts()
        });
        assert_eq!(
            steps,
            [Step::Settle, Step::Retire, Step::Rest, Step::Caches]
        );
    }

    #[test]
    fn the_helper_is_the_account_recorded_only_while_it_still_is() {
        let opie = Identity {
            uid: 502,
            name: "opie".into(),
            generated_uid: "6DB4B7AF-BE3C-486C-BD46-1B4558946919".into(),
        };
        let reused = Identity {
            generated_uid: "11111111-2222-3333-4444-555555555555".into(),
            ..opie.clone()
        };
        assert_eq!(known(Standing::Same(opie.clone())), Known::Verified(opie));
        assert_eq!(known(Standing::Replaced), Known::Other);
        assert_eq!(known(Standing::Unknown), Known::Other, "unreadable");
        assert_eq!(
            known(Standing::Unrecorded(reused.clone())),
            Known::Unrecorded(reused)
        );
    }

    #[test]
    fn only_the_recorded_helpers_job_is_retired() {
        let opie = Identity {
            uid: 502,
            name: "opie".into(),
            generated_uid: "6DB4B7AF-BE3C-486C-BD46-1B4558946919".into(),
        };
        let verified = Known::Verified(opie.clone());
        assert_eq!(retiring(Some(&verified)).unwrap(), Some(&opie));
        assert_eq!(retiring(Some(&Known::Other)).unwrap(), None, "left alone");
        assert_eq!(retiring(None).unwrap(), None, "gone");
        assert_eq!(
            retiring(Some(&Known::Unrecorded(opie))).unwrap(),
            None,
            "never taken for the helper unrecorded"
        );
    }

    #[test]
    fn only_the_strict_look_says_the_helper_is_gone() {
        assert_eq!(strictly(Came::Gone, false), Came::Stayed, "root said so");
        assert_eq!(strictly(Came::Stayed, true), Came::Gone);
        assert_eq!(strictly(Came::Held, false), Came::Held);
        assert_eq!(strictly(Came::Gone, true), Came::Gone);
    }

    /// Uninstall with stand-ins for every step: each look at whether the
    /// helper is logged out answers from `looks` (logged out once they run
    /// out), and each step notes whether another controller could take
    /// control of the helper while it ran.
    struct Script {
        looks: std::collections::VecDeque<bool>,
        steps: Vec<String>,
        controller: crate::control::Controller,
        state: PathBuf,
        held: Option<Ownership>,
        shutdown: Option<Result<()>>,
        warnings: Vec<String>,
    }

    impl Script {
        fn new(dir: &Path, looks: &[bool]) -> Self {
            let state = dir.join("state");
            Self {
                looks: looks.iter().copied().collect(),
                steps: Vec::new(),
                controller: crate::control::Controller::at(60001, &state),
                state,
                held: None,
                shutdown: None,
                warnings: Vec::new(),
            }
        }

        /// Note `step`, and whether another controller found the helper
        /// taken while it ran.
        fn note(&mut self, step: &str) {
            let other = crate::control::Controller::at(60001, &self.state);
            let taken = other.acquire(true).is_err();
            self.steps
                .push(format!("{step}{}", if taken { "" } else { " (free)" }));
        }
    }

    impl Teardown for Script {
        async fn settle(&mut self) -> Result<()> {
            self.held = Some(if let Some(outcome) = self.shutdown.take() {
                settle_with(
                    &self.controller,
                    &mut self.warnings,
                    async { Ok((true, Some(true), Some(4242))) },
                    |_| async { outcome },
                )
                .await?
            } else {
                self.controller.acquire(true)?
            });
            self.note("settle");
            Ok(())
        }

        fn logged_out(&mut self) -> bool {
            let look = self.looks.pop_front().unwrap_or(true);
            self.steps.push(format!("look {look}"));
            look
        }

        async fn log_out(&mut self) -> Result<()> {
            self.note("log out");
            Ok(())
        }

        fn retire(&mut self) -> Result<()> {
            self.note("retire");
            Ok(())
        }

        fn rest(&mut self) -> Result<()> {
            self.note("rest");
            Ok(())
        }

        fn remove_helper(&mut self) {
            self.note("remove helper");
        }

        fn keep_helper(&mut self) {
            self.note("keep helper");
        }

        fn hint(&mut self) {
            self.note("hint");
        }

        fn caches(&mut self) {
            self.note("caches");
            remove_caches(&self.state, &mut Vec::new()).unwrap();
        }

        fn release(&mut self) {
            self.held = None;
        }
    }

    #[tokio::test]
    async fn an_unrecorded_account_is_kept_while_atlas_and_its_caches_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let facts = Facts {
            other: true,
            on_screen: true,
            remove_helper: true,
            ..facts()
        };
        let steps = plan(&facts);
        assert_eq!(
            steps,
            [
                Step::Settle,
                Step::Retire,
                Step::Rest,
                Step::Hint,
                Step::Caches
            ]
        );
        let mut script = Script::new(dir.path(), &[false]);
        let claim = script.controller.quarantine().unwrap();
        let generation = claim.generation().to_owned();
        drop(claim);
        execute(&steps, &facts, &mut script).await.unwrap();
        assert_eq!(script.steps, ["settle", "retire", "rest", "hint", "caches"]);
        assert_eq!(script.controller.pending().unwrap(), [generation]);
        assert!(script.controller.acquire(true).is_ok());
    }

    #[tokio::test]
    async fn unsupported_or_vanished_input_still_reaches_verified_logout_and_teardown() {
        if !crate::test_support::isolated(
            "uninstall::tests::unsupported_or_vanished_input_still_reaches_verified_logout_and_teardown",
        ) {
            return;
        }
        for outcome in [
            Err(crate::error::Error::new(
                Code::Input,
                "Unknown native operation",
            )),
            Err(crate::error::Error::new(
                Code::Io,
                "No such file or directory",
            )),
            Err(crate::error::Error::new(Code::Uncertain, "Shutdown reply lost").uncertain()),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let uncertain = outcome.as_ref().is_err_and(|error| error.uncertain);
            let mut script = Script::new(dir.path(), &[false, true, true]);
            script.shutdown = Some(outcome);
            fs::create_dir_all(script.state.join("update")).unwrap();
            fs::write(script.state.join("update/scratch"), b"temporary").unwrap();
            execute(&plan(&facts()), &facts(), &mut script)
                .await
                .unwrap();
            assert_eq!(
                script.steps,
                [
                    "settle",
                    "look false",
                    "log out",
                    "look true",
                    "retire",
                    "rest",
                    "look true",
                    "hint",
                    "caches"
                ]
            );
            assert_eq!(
                script.controller.pending().unwrap().len(),
                usize::from(uncertain)
            );
            assert!(!script.state.join("update").exists());
            assert_eq!(script.warnings.len(), 1);
            assert!(script.controller.acquire(true).is_ok());
        }
        let dir = tempfile::tempdir().unwrap();
        let mut script = Script::new(dir.path(), &[false, false]);
        script.shutdown = Some(Err(crate::error::Error::new(
            Code::Input,
            "Unknown native operation",
        )));
        assert_eq!(
            execute(&plan(&facts()), &facts(), &mut script)
                .await
                .unwrap_err()
                .code,
            "logout"
        );
        assert_eq!(
            script.steps,
            ["settle", "look false", "log out", "look false"]
        );
    }

    #[tokio::test]
    async fn a_login_that_starts_after_the_plan_is_logged_out_before_the_job_is_retired() {
        let dir = tempfile::tempdir().unwrap();
        let facts = Facts {
            logged_in: false,
            ..facts()
        };
        // Logged out when planned, logged in once control is taken.
        let mut script = Script::new(dir.path(), &[false, true, true]);
        execute(&plan(&facts), &facts, &mut script).await.unwrap();
        assert_eq!(
            script.steps,
            [
                "settle",
                "look false",
                "log out",
                "look true",
                "retire",
                "rest",
                "look true",
                "hint",
                "caches"
            ]
        );
    }

    #[tokio::test]
    async fn a_helper_still_logged_in_when_its_job_would_go_stops_everything() {
        let dir = tempfile::tempdir().unwrap();
        let mut script = Script::new(dir.path(), &[false, false]);
        let error = execute(&plan(&facts()), &facts(), &mut script)
            .await
            .unwrap_err();
        assert_eq!(error.code, "logout");
        assert_eq!(
            script.steps,
            ["settle", "look false", "log out", "look false"]
        );
    }

    #[tokio::test]
    async fn a_helper_logged_in_again_before_its_deletion_keeps_its_account() {
        let dir = tempfile::tempdir().unwrap();
        let facts = Facts {
            remove_helper: true,
            ..facts()
        };
        let mut script = Script::new(dir.path(), &[true, true, false]);
        execute(&plan(&facts), &facts, &mut script).await.unwrap();
        assert!(script.steps.contains(&"keep helper".to_owned()));
        assert!(!script.steps.contains(&"remove helper".to_owned()));
        assert_eq!(script.steps.last().map(String::as_str), Some("caches"));
    }

    #[tokio::test]
    async fn no_other_controller_takes_the_helper_until_the_caches_are_gone() {
        if !crate::test_support::isolated(
            "uninstall::tests::no_other_controller_takes_the_helper_until_the_caches_are_gone",
        ) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        for remove_helper in [false, true] {
            let facts = Facts {
                remove_helper,
                ..facts()
            };
            let mut script = Script::new(dir.path(), &[]);
            execute(&plan(&facts), &facts, &mut script).await.unwrap();
            let free: Vec<&String> = script
                .steps
                .iter()
                .filter(|step| step.ends_with("(free)"))
                .collect();
            assert!(free.is_empty(), "{:?}", script.steps);
            assert_eq!(script.steps.last().map(String::as_str), Some("caches"));
            assert!(
                script.controller.acquire(true).is_ok(),
                "let go once every step has run"
            );
        }
    }

    /// Agents whose marketplace named atlas is someone else's are named.
    #[test]
    fn agents_left_alone_are_named_with_their_marketplace() {
        let reply = json!({"ok": true, "agents": [
            {"agent": "claude", "ok": true, "changed": true},
            {"agent": "codex", "ok": true, "changed": false, "kept": "someone/atlas"},
        ]});
        assert_eq!(
            kept_agents(&reply),
            vec![("Codex".to_owned(), "someone/atlas".to_owned())]
        );
        assert!(kept_agents(&json!({"ok": true, "agents": []})).is_empty());
    }

    /// Uninstall's own "off" is never kept as the person's choice; one
    /// they made to turn it off is.
    #[test]
    fn only_a_choice_to_keep_it_off_survives_uninstall() {
        assert!(keeps_login_choice(true, Some(false)));
        assert!(!keeps_login_choice(true, Some(true)), "it was on");
        assert!(!keeps_login_choice(false, Some(false)), "never decided");
        assert!(!keeps_login_choice(false, None));
        assert!(
            !keeps_login_choice(true, None),
            "unknown: setup decides again"
        );
    }
}
