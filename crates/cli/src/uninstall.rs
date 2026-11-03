//! `atlas uninstall`: undo what install.sh and init did on this Mac.
//! It asks first, logs the helper out before anything of it goes, keeps the
//! helper account unless asked to delete it (and says how, once it is
//! logged out), and moves the app to the Trash rather than deleting it.
mod inspection;
mod log_out;
#[cfg(test)]
mod stand_in;
use inspection::{Found, survey};
use log_out::{log_out_helper, logged_out_twice, retiring, still_logged_in, unrecorded_message};

use crate::onboarding::as_root::{self, Target, Via};
use atlas_client::{client::Client, setup};
use atlas_control::{Controller, Ownership};
use atlas_core::{
    account,
    config::{self, Config},
    error::{Code, Error, Result},
};
use atlas_daemon::{
    helper_setup,
    helper_setup::{
        Agent, JobReport, LABEL, Layout, Request, Unfinished,
        identity::{Identity, Standing},
    },
};
use atlas_native::approval::Refusal;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// A decision made while teardown retains its current ownership.
pub enum Question {
    Remove,
    ForceLogout,
}

/// Progress from the owned teardown, rendered by its caller.
pub enum Progress<'a> {
    Say(&'a str),
    Done(&'a str, &'a str),
    Skipped(&'a str, &'a str),
}

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
    let Some(account) = account::account_with_uid(helper.uid)? else {
        return Ok(None);
    };

    // As the helper but outside any login of it: it is logged out by now,
    // and reaching into its login would start one again. Each request is
    // checked again by the helper itself once sudo let it run: still the
    // account recorded, and still logged out.
    let ask = |request: &Request, failed: &str| -> Result<Value> {
        retirement_request(controller, request, || {
            setup::request_as_helper(
                executable,
                &account,
                request,
                setup::Invocation::LoggedOut,
                helper,
            )?
            .map_err(|unfinished| {
                let said = match &unfinished {
                    Unfinished::Silent(_) => failed.to_owned(),
                    Unfinished::Unchanged(why) | Unfinished::Uncertain(why) => {
                        format!("{failed}: {}", why.as_deref().unwrap_or("unknown"))
                    }
                };
                unfinished.behind(problem(said))
            })
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
    let sha256 = JobReport::from(&inspected["jobs"][LABEL])
        .sha256
        .unwrap_or_else(|| {
            let layout = Layout::new(helper.uid, account.home.clone());
            let controller = nix::unistd::getuid().as_raw();
            Agent::managed(&layout, executable.to_owned(), controller).plist_sha256(&layout.logs())
        });

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
    if request.requires_claim() {
        setup::executor::setup_transaction_held(controller, ask)
    } else {
        ask()
    }
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
    logged_in: bool,
    on_screen: bool,
    remove_helper: bool,
}

/// One step of uninstalling, in the order [`plan`] gives them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
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
fn plan(facts: &Facts) -> Result<Vec<Step>> {
    if facts.settings && facts.on_screen && !facts.other {
        return Err(problem(
            "Switch back to your own account first, then run atlas uninstall again.",
        ));
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

    Ok(steps)
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
    fn retire(&mut self) -> Result<()>;
    fn rest(&mut self) -> Result<()>;
    /// [`Step::RemoveHelper`], for a helper seen logged out.
    fn remove_helper(&mut self);
    /// The account kept: it was seen logged in when it was to be deleted.
    fn keep_helper(&mut self);
    /// [`Step::Hint`], for a helper seen logged out.
    fn hint(&mut self);
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

/// Undo install and init. `keep_config` keeps settings and logs;
/// `remove_helper` deletes the helper account too, once it is logged out.
///
/// # Errors
///
/// Returns confirmation, ownership, account-identity, executor or teardown failures.
/// Earlier removals may be complete; unknown effects retain recovery state and are not
/// reported as successful teardown.
pub async fn run(
    config_path: &Path,
    keep_config: bool,
    yes: bool,
    remove_helper: bool,
    ask: &dyn Fn(Question) -> Result<bool>,
    progress: &dyn Fn(Progress<'_>),
) -> Result<Value> {
    let found = survey(config_path)?;
    let uid = found.client.as_ref().map(|client| client.config.helper_uid);
    let other = found.not_the_helper();
    let facts = Facts {
        settings: uid.is_some(),
        account: found.helper.is_some(),
        other,
        logged_in: uid.is_some_and(|uid| atlas_core::session::logged_out(uid) != Some(true)),
        on_screen: uid.is_some() && atlas_core::session::console_owner() == uid,
        remove_helper,
    };
    let steps = plan(&facts)?;

    if !yes {
        confirm(&found, config_path, keep_config, &facts, ask, progress)?;
    }

    let mut live = Live {
        found: &found,
        config_path,
        keep_config,
        yes,
        removed: Vec::new(),
        warnings: Vec::new(),
        hint: None,
        deleted: false,
        exclusion: None,
        progress,
        ask,
    };

    if let Some(error) = &found.unloaded {
        progress(Progress::Skipped(
            "Helper job",
            "left in place: the settings do not load",
        ));
        live.warnings
            .push(format!("Helper job left in place: {error}"));
    }
    if let Some(Standing::Unrecorded(identity)) = &found.known {
        let message = unrecorded_message(&identity.name);
        progress(Progress::Skipped("Helper account", &message));
        live.warnings.push(message);
    } else if other {
        progress(Progress::Skipped(
            "Helper",
            "left alone: another account has its user ID now",
        ));
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
        deleted,
        ..
    } = live;

    let mut report = json!({
        "ok": true,
        "removed": removed,
        "kept": {
            "helperAccount": if deleted { None } else { found.helper.clone() },
            "remove": hint,
        },
    });

    if found.unloaded.is_some() || matches!(found.known, Some(Standing::Unrecorded(_))) {
        report["kept"]["helperJob"] = json!(true);
    }

    if !warnings.is_empty() {
        report["warnings"] = json!(warnings);
    }

    Ok(report)
}

struct Live<'a> {
    found: &'a Found,
    config_path: &'a Path,
    keep_config: bool,
    yes: bool,
    removed: Vec<Value>,
    warnings: Vec<String>,
    hint: Option<String>,
    deleted: bool,
    /// Control of the helper, from [`Step::Settle`] to the end.
    exclusion: Option<Ownership>,
    progress: &'a dyn Fn(Progress<'_>),
    ask: &'a dyn Fn(Question) -> Result<bool>,
}

impl Teardown for Live<'_> {
    async fn settle(&mut self) -> Result<()> {
        let Some(client) = &self.found.client else {
            return Ok(());
        };
        if self.found.not_the_helper() {
            // Local exclusion only: preserve pending outcomes without
            // contacting or changing an account whose identity is unknown.
            self.exclusion = Some(client.controller.acquire(true)?);
            return Ok(());
        }
        self.exclusion = Some(settle(client, &mut self.warnings, self.progress).await?);

        // Background jobs are stopped first, so none outlives Atlas in the
        // helper. One that could not be stopped is said, never a failure.
        let jobs =
            atlas_client::jobs::stop_every(client, atlas_core::protocol::StopReason::Stopped).await;
        if jobs.asked > 0 {
            (self.progress)(Progress::Done(
                "Background jobs",
                &format!("stopped {}", jobs.stopped()),
            ));
        }
        self.warnings.extend(jobs.warnings());

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
        let target = match (&self.found.helper, retiring(self.found.known.as_ref())) {
            (None, _) => Some(Target::Orphan(client.config.helper_uid)),
            (Some(_), Some(identity)) => Some(Target::Helper(identity.clone())),
            (Some(_), None) => None,
        };

        log_out_helper(client, ownership, self.yes, target, self.ask, self.progress).await
    }

    fn retire(&mut self) -> Result<()> {
        let Some(client) = &self.found.client else {
            return Ok(());
        };
        let job = retiring(self.found.known.as_ref());
        let _ownership = self
            .exclusion
            .as_ref()
            .ok_or_else(|| problem("Atlas lost control of the helper"))?;

        let config = &client.config;
        if let Some(helper) = job {
            (self.progress)(Progress::Say(
                "Stopping the helper's background job (macOS may ask for your password)…",
            ));
            if let Some(opener) =
                retire_helper_job(&self.found.executable, helper, &client.controller)?
            {
                self.removed.push(json!("helper job"));
                (self.progress)(Progress::Done("Helper job", "stopped"));
                match opener {
                    Ok(true) => self
                        .removed
                        .push(json!("Atlas's one-time start in the helper")),
                    Ok(false) => {}
                    Err(why) => self.warnings.push(why),
                }
            } else {
                (self.progress)(Progress::Skipped(
                    "Helper job",
                    "the helper account no longer exists",
                ));
                self.warnings.push(format!(
                    "The helper account (user ID {}) no longer exists, so no job was stopped",
                    config.helper_uid
                ));
            }
        }
        // Archive evidence only after the job is gone; it must stop counting as setup.
        if let Err(error) = setup::migrate::archive_uninstalled(config.helper_uid) {
            self.warnings.push(format!(
                "The setup record was not set aside: {}",
                error.error
            ));
        }
        if remove_link_to(&config.identity_probe, &self.found.executable)? {
            self.removed.push(json!(config.identity_probe));
        }
        Ok(())
    }

    fn rest(&mut self) -> Result<()> {
        self.forget_requested_opener();
        self.stop_menubar()?;
        self.disconnect()?;
        // Stopped while the agents were changed: the app and settings stay.
        if atlas_agents::stopping() {
            return Err(problem("Stopped before Atlas.app was removed"));
        }
        self.trash()?;
        self.remove_settings()
    }

    fn remove_helper(&mut self) {
        if self.found.helper.is_none() {
            return;
        }
        let Some(identity) = retiring(self.found.known.as_ref()).cloned() else {
            self.kept_account("Atlas can't tell it is still the helper");
            return;
        };

        let (Some(client), Some(_ownership)) = (&self.found.client, &self.exclusion) else {
            self.warnings
                .push("The helper account was kept: Atlas lost control of it".into());
            return;
        };
        let uid = identity.uid;
        if uid == nix::unistd::getuid().as_raw()
            || atlas_core::session::console_owner() == Some(uid)
        {
            self.keep_helper();
            return;
        }

        (self.progress)(Progress::Say(
            "Deleting the helper account (macOS may ask for your password)…",
        ));
        let name = &identity.name;
        match setup::executor::setup_transaction_held(&client.controller, || {
            as_root::remove_helper(&identity, Via::Sudo { ask: true }).map_err(|refusal| {
                problem(match refusal {
                    Refusal::Failed(why) => why,
                    Refusal::Cancelled | Refusal::Unavailable => {
                        "administrator rights were not granted".into()
                    }
                })
            })
        }) {
            Ok(()) => {
                self.removed.push(json!(format!("helper account {name}")));
                self.deleted = true;
                (self.progress)(Progress::Done("Helper account", &format!("{name} deleted")));
            }
            Err(error) => {
                let why = error.error;
                (self.progress)(Progress::Skipped(
                    "Helper account",
                    &format!("deletion not confirmed: {why}"),
                ));
                self.warnings.push(format!(
                    "Deletion of the helper account {name} was not confirmed: {why}. Its outcome \
                     stays pending until Atlas recovers it."
                ));
            }
        }
    }

    fn keep_helper(&mut self) {
        self.kept_account("it is still logged in");
    }

    fn hint(&mut self) {
        self.hint = self.found.helper.as_ref().map(|name| {
            if matches!(self.found.known, Some(Standing::Unrecorded(_))) {
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
        let Ok(caches) = atlas_client::update::caches() else {
            return;
        };

        match remove_caches(&caches, &mut self.removed, self.progress) {
            Ok(false) => {}
            Ok(true) => self.warnings.push(format!(
                "The last action's outcome is unknown, so its record stays in {} until Atlas \
                 recovers it",
                config::tilde(&caches)
            )),
            Err(error) => self.warnings.push(format!(
                "{} was not removed: {}",
                config::tilde(&caches),
                error.error
            )),
        }
    }

    fn release(&mut self) {
        self.exclusion = None;
    }
}

impl Live<'_> {
    fn kept_account(&mut self, why: &str) {
        if let Some(name) = &self.found.helper {
            (self.progress)(Progress::Skipped("Helper account", &format!("kept: {why}")));
            self.warnings
                .push(format!("The helper account {name} was kept: {why}"));
        }
    }

    /// Stop a running menu bar, then its opening at login. Turning it off
    /// here is not the person's choice: unless they had turned it off
    /// themselves, the record of a choice goes, so setting Atlas up again
    /// turns it on again (settings kept with --keep-config keep that record
    /// otherwise). A menu bar not seen to stop or known not to run changes
    /// nothing, and is an error.
    fn stop_menubar(&self) -> Result<()> {
        let output = match &self.found.client {
            Some(client) => client.output_dir().map(Path::to_path_buf),
            None => Err(Error::new(
                Code::Config,
                self.found
                    .unloaded
                    .clone()
                    .unwrap_or_else(|| "Atlas has no settings".to_owned()),
            )),
        };
        let stopped = crate::menubar::stop_located(output)
            .map_err(|error| {
                problem(format!(
                    "{}, so Atlas.app and its settings were kept. Quit the menu bar, then run \
                     atlas uninstall again.",
                    error.error
                ))
                .with_source(error)
            })?
            .is_some();
        #[cfg(test)]
        if stand_in::reached("login") {
            return Ok(());
        }

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
            (false, false) => return Ok(()),
        };

        (self.progress)(Progress::Done("Menu bar", said));
        Ok(())
    }

    /// Agents forget the plugin (only those using a marketplace of Atlas's
    /// own change), and `atlas` leaves the terminal.
    fn disconnect(&mut self) -> Result<()> {
        #[cfg(test)]
        if stand_in::reached("agents") {
            return Ok(());
        }
        // An error here means no agent is installed, so there is nothing to remove.
        if let Ok(reply) = atlas_agents::change(&[], atlas_agents::Change::Disconnect) {
            let changed = agents_in(&reply).any(|agent| agent["changed"] == true);

            if reply["ok"] == true {
                if changed {
                    self.removed.push(json!("agent plugins"));
                    (self.progress)(Progress::Done("Agent plugins", "removed"));
                }
            } else {
                let failed = failed_agents(&reply);
                (self.progress)(Progress::Skipped(
                    "Agent plugins",
                    &format!("not removed from {failed}; run atlas agents remove"),
                ));
                self.warnings.push(format!(
                    "The plugin was not removed from {failed}; run atlas agents remove"
                ));
            }

            for (agent, place) in kept_agents(&reply) {
                (self.progress)(Progress::Skipped(
                    "Agent plugins",
                    &format!("left in {agent}: its atlas marketplace is from {place}"),
                ));
                self.warnings.push(format!(
                    "{agent}'s marketplace named atlas is from {place}, not Atlas, so it was left alone"
                ));
            }
        }

        let command = config::home()?.join(".local/bin/atlas");
        if remove_link_to(&command, &self.found.executable)? {
            self.removed.push(json!(command));
        }

        Ok(())
    }

    /// Take away the one-time start the window left in an account this
    /// account asked for but whose setup it never finished (not the helper
    /// its settings name, whose start goes with its job): with Atlas.app gone
    /// it could never run, and would stay among that account's login items.
    /// sudo may ask the person for their password.
    fn forget_requested_opener(&mut self) {
        #[cfg(test)]
        if stand_in::reached("opener") {
            return;
        }
        let me = nix::unistd::getuid().as_raw();
        let client = self.found.client.as_ref();
        let Some(pairing) = atlas_daemon::helper_setup::pairing::request().filter(|pairing| {
            pairing.controller_uid == me
                && client.is_none_or(|client| client.config.helper_uid != pairing.helper_uid)
        }) else {
            return;
        };
        let Ok(Some(account)) = account::account_with_uid(pairing.helper_uid) else {
            return;
        };

        let request = Request::ForgetOpener { uid: account.uid };
        let Some(expected) = helper_setup::identity::verified(account.uid) else {
            self.warnings.push("Atlas's one-time start was left in place: choose the helper account explicitly before removing it".into());
            return;
        };
        let outcome = Controller::new(account.uid).and_then(|controller| {
            setup::executor::setup_transaction(&controller, || {
                setup::request_as_helper(
                    &self.found.executable,
                    &account,
                    &request,
                    setup::Invocation::Login,
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
                    self.removed
                        .push(json!(format!("Atlas's one-time start in {}", account.name)));
                }
            }
            Err(error) => self.warnings.push(format!(
                "Atlas's one-time start in {} was not removed: {}",
                account.name, error.error
            )),
        }
    }

    /// The app goes to the Trash, where it can be recovered; what it left for
    /// the helper goes with it.
    fn trash(&mut self) -> Result<()> {
        #[cfg(test)]
        if stand_in::reached("trash") {
            return Ok(());
        }
        let app = &self.found.app;
        let trash = config::home()?
            .join(".Trash")
            .join(format!("Atlas-{}.app", atlas_core::files::token()));

        fs::rename(app, &trash).map_err(|error| {
            problem(format!(
                "Could not move {} to the Trash; remove it yourself",
                app.display()
            ))
            .with_source(error)
        })?;

        self.removed.push(json!(app));
        (self.progress)(Progress::Done("Atlas.app", "moved to the Trash"));

        // Copies an unfinished update or install.sh staged beside it, by their
        // reserved names only.
        for staged in atlas_client::update::atlas_copies(app) {
            if fs::remove_dir_all(&staged).is_ok() {
                self.removed.push(json!(staged));
            }
        }

        let mut cleared = false;
        for leftover in &self.found.leftovers {
            if fs::remove_dir_all(leftover).is_ok() {
                self.removed.push(json!(leftover));
                cleared = true;
            }
        }

        if cleared {
            (self.progress)(Progress::Done("Copies for the helper", "removed"));
        }

        // The request naming the helper, when this account made it.
        let request = Path::new(atlas_daemon::helper_setup::pairing::REQUEST);
        if remove_own_file(request).unwrap_or(false) {
            self.removed.push(json!(request));
        }

        // The shared folder itself, once nothing but Atlas's is left in it:
        // another account could not set Atlas up while it is there.
        if let Some(shared) = Path::new(atlas_client::handoff::ROOT).parent()
            && remove_own_empty_folder(shared)
        {
            self.removed.push(json!(shared));
        }

        Ok(())
    }

    /// The installed settings and the logs and records, unless kept.
    fn remove_settings(&mut self) -> Result<()> {
        #[cfg(test)]
        if stand_in::reached("settings") {
            return Ok(());
        }
        if self.keep_config {
            return Ok(());
        }

        // Only the installed settings; a checkout's own file is never removed.
        let installed = config::installed_path().ok();
        if installed.as_deref() == Some(self.config_path)
            && fs::symlink_metadata(self.config_path).is_ok()
        {
            fs::remove_file(self.config_path)?;
            self.removed.push(json!(self.config_path));
            (self.progress)(Progress::Done("Settings", "removed"));
        }

        let data = &self.found.data;
        if data.exists() {
            fs::remove_dir_all(data)?;
            self.removed.push(json!(data));
            (self.progress)(Progress::Done("Logs and records", "removed"));
        }

        Ok(())
    }
}

fn confirm(
    found: &Found,
    config_path: &Path,
    keep_config: bool,
    facts: &Facts,
    ask: &dyn Fn(Question) -> Result<bool>,
    progress: &dyn Fn(Progress<'_>),
) -> Result<()> {
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
        format!("{} (to the Trash)", config::tilde(app)),
    ];

    if !leftovers.is_empty() {
        parts.push("apps and files Atlas copied for the helper to open".to_owned());
    }

    if !keep_config {
        parts.push(format!(
            "settings in {} and logs in {}",
            config::tilde(config_path),
            config::tilde(data)
        ));
    }

    progress(Progress::Say("This removes from this Mac:"));

    for part in parts {
        progress(Progress::Say(&format!("  · {part}")));
    }

    if let Some(name) = helper {
        let first = if facts.logged_in && !facts.other {
            "The helper is logged out first; its apps quit. "
        } else {
            ""
        };
        progress(Progress::Say(
            &if matches!(found.known, Some(Standing::Unrecorded(_))) {
                unrecorded_message(name)
            } else if facts.remove_helper && !facts.other {
                format!("{first}Then the helper account {name} and its files are deleted.")
            } else if facts.logged_in {
                format!("{first}The account and its files stay.")
            } else {
                format!("The helper account {name} and its files stay.")
            },
        ));
    }

    if unloaded.is_some() {
        progress(Progress::Say(&format!(
            "The settings in {} do not load, so the helper's background job stays.",
            config::tilde(config_path)
        )));
    }

    if !ask(Question::Remove)? {
        return Err(Error::new(Code::Cancelled, "Stopped."));
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
async fn settle(
    client: &Client,
    warnings: &mut Vec<String>,
    progress: &dyn Fn(Progress<'_>),
) -> Result<Ownership> {
    settle_with(
        &client.controller,
        warnings,
        async {
            let uid = client.config.helper_uid;
            let exists = account::account_with_uid(uid)?.is_some();
            let status = client.status().await;
            let pid = setup::reported_pid(&status["inputService"]["pid"]);
            Ok((exists, atlas_core::session::logged_in(uid), pid))
        },
        |pid| client.stop_input(pid),
        progress,
    )
    .await
}

async fn settle_with<F>(
    controller: &Controller,
    warnings: &mut Vec<String>,
    observe: impl std::future::Future<Output = Result<(bool, Option<bool>, Option<u32>)>>,
    stop: impl FnOnce(u32) -> F,
    progress: &dyn Fn(Progress<'_>),
) -> Result<Ownership>
where
    F: std::future::Future<Output = Result<()>>,
{
    let exclusion = controller.acquire(true)?;
    let (exists, logged_in, pid) = observe.await?;
    match cannot_settle(exists, logged_in) {
        None => controller.require_known()?,
        Some(why) if controller.require_known().is_err() => {
            progress(Progress::Skipped(
                "Last action",
                &format!("outcome unknown; {why}"),
            ));
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
    if !outcome.as_ref().is_err_and(|error| error.uncertain) {
        controller.clear_if(&claim)?;
    }
    outcome
}

/// Whether the record of an Open at Login choice stays once uninstall turns
/// it off: only when the person had decided it, and decided off.
fn keeps_login_choice(decided: bool, enabled: Option<bool>) -> bool {
    decided && enabled == Some(false)
}

fn agents_in(reply: &Value) -> impl Iterator<Item = &Value> {
    reply["agents"].as_array().into_iter().flatten()
}

/// The agents a plugin change failed for, as "Claude Code and Codex".
fn failed_agents(reply: &Value) -> String {
    let names: Vec<&str> = agents_in(reply)
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
    agents_in(reply)
        .filter_map(|agent| {
            let place = agent["kept"].as_str()?;
            let name = agent["agent"].as_str()?;

            Some((shown(name).to_owned(), place.to_owned()))
        })
        .collect()
}

fn shown(name: &str) -> &str {
    atlas_agents::Agent::named(name).map_or(name, |agent| agent.display_name())
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
fn remove_caches(
    caches: &Path,
    removed: &mut Vec<Value>,
    progress: &dyn Fn(Progress<'_>),
) -> Result<bool> {
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
        progress(Progress::Done("Caches", "removed"));
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
        progress(Progress::Done("Caches", "removed; Atlas's locks stay"));
    } else {
        fs::remove_dir(caches)?;
        progress(Progress::Done("Caches", "removed"));
    }
    removed.push(json!(caches));

    Ok(unknown)
}

/// What Atlas left for the helper: the copies `open` made, and environment
/// plans and requests beside the shared command. Only folders that are
/// provably Atlas's own are listed.
fn handed_over(shared_command: &Path) -> Vec<PathBuf> {
    let mut leftovers: Vec<PathBuf> = atlas_client::handoff::root_if_owned().into_iter().collect();

    if let Some(shared) = shared_command.parent() {
        leftovers.extend(atlas_client::environment::leftovers(shared));
    }

    leftovers
}

#[cfg(test)]
mod tests;
