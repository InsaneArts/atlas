//! Terminal choices, progress and waiting for attended setup.
use super::prompt;
use atlas_cli::onboarding::{close_attended, create_helper};
use atlas_cli::ui::{
    self,
    progress::{Spinner, done, heading, say, skipped},
    style,
};
use atlas_client::setup::{FULL_NAME, missing, validate_account_name as valid_account_name};
use atlas_client::setup::{
    configure_helper, first_login_setup, prepare_helper, refresh_helper, restart_helper,
};
use atlas_client::{client::Client, setup};
use atlas_core::{
    account,
    config::{self, Config},
    error::{Code, Error, Result},
};
use atlas_daemon::helper_setup::{self, LABEL};
use serde_json::{Value, json};
use std::{
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

fn problem(message: impl Into<String>) -> Error {
    Error::new(Code::Init, message)
}

/// How long init waits for the person to log in or click Allow.
const PATIENCE: Duration = Duration::from_secs(30 * 60);

/// A command that must succeed and talks to the person itself (sudo and
/// sysadminctl ask for passwords in their own words).
fn attended(program: &str, args: &[&str], why: &str) -> Result<()> {
    let status = atlas_core::spawn::status(Command::new(program).args(args))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| problem(why.to_owned()))
}

/// Choose an existing standard account, or (only when asked for, and
/// confirmed) create one.
fn choose_helper() -> Result<(account::Account, helper_setup::identity::Identity)> {
    let candidates = atlas_client::switch::standard_accounts()?;
    let mut items: Vec<String> = candidates
        .iter()
        .map(|account| match &account.real_name {
            Some(real) if real != &account.name => format!("{real} ({})", account.name),
            _ => account.name.clone(),
        })
        .collect();
    items.push("Create a new account…".to_owned());

    let choice = if candidates.is_empty() {
        say("There is no other standard account yet, so Atlas will make one.");
        items.len() - 1
    } else {
        prompt::select("Which account should be the helper?", &items, 0, "")?
    };

    if let Some(account) = candidates.get(choice) {
        return selected(
            account::account_named(&account.name)?
                .ok_or_else(|| problem("That account disappeared"))?,
        );
    }

    let name = prompt::input(
        "Short name for the new account",
        "atlas",
        |name| {
            valid_account_name(name)?;
            match account::account_named(name) {
                Ok(Some(_)) => Err(format!(
                    "{name} already exists; choose it from the list instead."
                )),
                _ => Ok(()),
            }
        },
        "",
    )?;

    if !prompt::confirm(
        &format!("Create a standard account named {name}?"),
        true,
        "",
    )? {
        return Err(problem(
            "No account was created; run atlas init again to choose",
        ));
    }

    say("macOS asks for your administrator password, then a password for the new account.");
    selected(create_helper(&name, FULL_NAME)?)
}

/// Capture the account and its permanent identity at selection time.
fn selected(
    helper: account::Account,
) -> Result<(account::Account, helper_setup::identity::Identity)> {
    let identity = helper_setup::identity::current(&helper)
        .ok_or_else(|| problem("macOS could not identify the selected helper account"))?;
    Ok((helper, identity))
}

/// Ask for administrator rights once, saying `why`, and keep them fresh
/// while `rights` lasts.
fn administrator_rights(rights: &mut setup::AdministratorRights, why: &str) -> Result<()> {
    heading("Administrator rights");
    say(why);

    rights.request()?;
    done("Administrator rights", "");

    Ok(())
}

/// Poll `ready` every few seconds until it holds or patience runs out.
async fn wait_for<T>(what: &str, mut ready: impl AsyncFnMut() -> Option<T>) -> Result<T> {
    let deadline = Instant::now() + PATIENCE;

    loop {
        if let Some(value) = ready().await {
            return Ok(value);
        }

        if Instant::now() >= deadline {
            return Err(problem(format!(
                "Gave up waiting for {what}; run atlas init again"
            )));
        }

        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

/// The closing steps, which never fail a setup that worked: the menu bar at
/// login, and teaching the installed agents.
fn finish(done_steps: &mut Vec<Value>, settings: &Path) -> Result<()> {
    open_at_login(done_steps, settings);
    teach_agents(done_steps)
}

/// Turn the menu bar on at login, only by the first setup (a later run
/// leaves the person's choice alone) and only for the default settings,
/// the ones the login item uses.
fn open_at_login(done_steps: &mut Vec<Value>, settings: &Path) {
    // Until it is decided once (here, or by the person), never again after.
    if atlas_cli::menubar::login_decided() {
        return;
    }

    if config::installed_path().ok().as_deref() != Some(settings) {
        skipped(
            "Menu bar",
            &format!(
                "Open at Login uses the default settings; start it with atlas menubar --config {}",
                atlas_core::config::tilde(settings)
            ),
        );
        return;
    }

    // Anything but a plain yes in the app's reply is no.
    let status = atlas_cli::menubar::login("status").unwrap_or_default();
    if status["enabled"] == true {
        atlas_cli::menubar::decide_login();
    } else if status["available"] == true {
        match atlas_cli::menubar::login("on") {
            Ok(login) if login["enabled"] == true => {
                atlas_cli::menubar::decide_login();
                done(
                    "Menu bar",
                    "opens at login (atlas menubar --login off turns it off)",
                );
                done_steps.push(json!({"step": "menubar", "login": login}));
            }
            _ => skipped(
                "Menu bar",
                "turn on Open Atlas at login in Atlas › Settings › General later",
            ),
        }
    }
}

/// Offer to connect the agents here that answered and are not connected
/// yet, the Claude app beside them.
fn teach_agents(done_steps: &mut Vec<Value>) -> Result<()> {
    let atlas_agents::Unconnected { agents, app } = atlas_agents::unconnected();
    let mut names: Vec<String> = agents
        .iter()
        .map(|agent| agent.display_name().to_owned())
        .collect();
    if app {
        names.push("the Claude app".to_owned());
    }
    if !names.is_empty()
        && prompt::confirm(
            &format!(
                "Teach {} to use Atlas? (adds a plugin)",
                atlas_core::name::listed(&names)
            ),
            true,
            "run atlas agents install later",
        )?
    {
        // Setup is already done; this step can only be skipped, never fail it.
        let (ok, connected) = {
            let _spinner = Spinner::new("Connecting…");
            let changed = atlas_agents::change_these(
                &agents,
                atlas_agents::Covering {
                    every_agent: false,
                    app,
                },
                atlas_agents::Change::Connect,
            );
            match changed {
                Ok(changed) => (changed.ok, changed.report),
                Err(error) => (false, json!({"ok": false, "error": error.error})),
            }
        };
        if ok {
            done(
                "Agents",
                &format!(
                    "{} connected",
                    atlas_core::name::capitalized(&atlas_core::name::listed(&names))
                ),
            );
        } else {
            skipped(
                "Agents",
                "not every agent connected; run atlas agents install later",
            );
        }

        done_steps.push(json!({"step": "agents", "result": connected}));
    }

    Ok(())
}

/// The closing lines: ready, and what to try.
fn ready() {
    say(&format!("\n{}", style::bold("Atlas is ready.")));
    for line in ui::suggestions() {
        say(&line);
    }
}

/// Lock the screen for the person, if they want, so they can switch to
/// the helper `name` (user ID `uid`); otherwise say how. Refused with
/// `not-ready`, and nothing locked, while a new login could hang.
fn offer_lock(name: &str, uid: u32) -> Result<()> {
    let account = atlas_client::switch::Account {
        name: name.to_owned(),
        uid,
        real_name: None,
        helper: true,
    };
    // Checked before asking: a new login waits while another could hang it.
    atlas_client::switch::check(&account)?;

    if prompt::confirm(
        &format!("Lock the screen now so you can switch to {name}?"),
        true,
        "switch accounts yourself",
    )? {
        match atlas_client::switch::to(&account) {
            Ok(_) => {
                let _ = LOCKED.set(std::time::Instant::now());
                done(
                    "Screen locked",
                    &format!("click Switch User and choose {name}"),
                );
            }
            Err(error) if error.code == Code::NotReady => return Err(error),
            Err(error) => skipped("Screen not locked", &error.error),
        }
    } else {
        say(&format!(
            "  To switch: Apple menu › Lock Screen, then Switch User and choose {name}."
        ));
    }

    Ok(())
}

/// When init locked the screen for the person to switch to the helper.
static LOCKED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

/// Note in the login timing that the helper reached `step` (`answering`,
/// `hidden`) this long after init locked the screen for it.
fn mark_since_lock(step: &str) {
    if let Some(locked) = LOCKED.get() {
        atlas_core::login_log::mark(&format!(
            "helper.{step} after={}",
            atlas_core::login_log::seconds(locked.elapsed())
        ));
    }
}

/// Guide the person from an installed app to a ready helper.
pub(super) async fn run(config_path: &Path, allow_disrupt: bool) -> Result<Value> {
    super::dispatch::as_yourself("init")?;
    if !ui::interactive() {
        return Err(problem("init is a guided setup; run it in a terminal"));
    }

    let mut done_steps = Vec::new();

    say(&format!(
        "{}\n{}",
        style::bold("Atlas setup"),
        style::dim(
            "A hidden second desktop for your agents. About five minutes; safe to run again."
        )
    ));
    say("");

    let linked = setup::installed_app::link_installed_command()?;
    done(
        "Atlas.app",
        &atlas_core::config::tilde(atlas_core::app_bundle::installed_app()?),
    );
    done_steps.push(json!({"step": "app", "command": linked}));
    // Kept fresh for this run only, however often it asks.
    let mut rights = setup::AdministratorRights::default();
    let (config, helper, expected) = helper_step(config_path, &mut rights)?;
    let name = helper.name.clone();
    // Whether an earlier run already finished setup (decides the first-run steps).
    let settled_before = setup::migrate::settled(helper.uid);
    let mut client = Client::new(config.clone())?;
    client.host_session = atlas_core::host::HostSession::from_env();
    client.allow_disrupt = allow_disrupt;

    client.bootstrap_probe()?;

    // Only a helper that answers in the background with Accessibility can
    // be recovered now; otherwise the steps below start it first.
    let status = client.status().await;
    if config
        .desktop()
        .prove(&atlas_core::session::Observation::decode(&status))
        .is_ok()
        && status["permissions"]["accessibility"] != false
    {
        recover(&client).await?;
    }

    // Ready now, set up before, and running this very build throughout.
    let status = client.status().await;
    if atlas_client::capability::doctor(status.clone(), config.desktop().second_login())["ready"]
        == true
        && settled_before
        && atlas_core::session::input_build_current(&status)
    {
        done("Already set up", "everything checks out");
        done_steps.push(json!({"step": "verified"}));
        finish(&mut done_steps, config_path)?;
        ready();

        return Ok(json!({"ok": true, "ready": true, "alreadySetUp": true, "steps": done_steps}));
    }

    done_steps.push(json!({"step": "config", "path": config_path, "helper": name}));

    if !daemon_answers(&client, config.desktop().second_login().uid).await {
        start_helper(
            &client,
            &config,
            &helper,
            &mut done_steps,
            &expected,
            &mut rights,
        )
        .await?;
    }

    // The probe installed above is ready for the session checks below.
    permissions_step(&client, &config, &name).await?;
    let mut spinner = None;
    let refreshed = refresh_helper(&client, &helper, &expected, |notice| {
        match notice {
            setup::Progress::Administrator(why) => administrator_rights(&mut rights, why)?,
            setup::Progress::Waiting(message) => spinner = Some(Spinner::new(message)),
            setup::Progress::Done(label, detail) => done(label, detail),
        }
        Ok(())
    })
    .await?;
    drop(spinner);
    done_steps.extend(refreshed);
    verify_step(&client, config_path).await?;
    done("Checked", "the helper works");
    done_steps.push(json!({"step": "verified"}));

    finish(&mut done_steps, config_path)?;
    ready();

    Ok(json!({"ok": true, "ready": true, "steps": done_steps, "config": config_path}))
}

/// Adapt the account choice and configuration operation to terminal progress.
fn helper_step(
    config_path: &Path,
    rights: &mut setup::AdministratorRights,
) -> Result<(Config, account::Account, helper_setup::identity::Identity)> {
    let (config, helper, identity) = configure_helper(config_path, choose_helper, say)?;
    if close_attended(helper.uid, Some(&identity), || {
        heading("A deleted account is still logged in");
        say(
            "  macOS kept a login open for an account that was deleted. Close it before you go on,\n  or the next login can stall.",
        );
        if !prompt::confirm("Close it now? macOS asks for your password.", true, "")? {
            return Err(problem(
                "It's still open. Close it before you switch: run atlas init again.",
            ));
        }
        rights.request()
    })? {
        done("Closed", "the login the deleted account left open");
    }
    done(
        "Helper account",
        &format!(
            "{} · settings in {}",
            helper.name,
            atlas_core::config::tilde(config_path)
        ),
    );
    if setup::migrate::archive_foreign(helper.uid, &helper.home)? {
        say("  Set aside a setup record left by an earlier account with the same ID.");
    }
    Ok((config, helper, identity))
}

/// An earlier action of unknown outcome is recovered before anything else
/// when it can be; recovery needs Accessibility, so without it recovery
/// waits until the person has allowed it.
async fn recover(client: &Client) -> Result<()> {
    if !client.controller.pending()?.is_empty() {
        let spinner = Spinner::new("Recovering an unfinished action…");
        client.restart().await?;
        drop(spinner);
        done("Recovered an unfinished action", "");
    }

    Ok(())
}

/// Whether the helper's daemon answers (in the background or not).
async fn daemon_answers(client: &Client, uid: u32) -> bool {
    let status = client.status().await;
    atlas_core::session::require_hidden(&status, uid).is_ok() || status["daemon"]["pid"].is_number()
}

/// Whether the helper's daemon answers within 30 tries, `pause` apart.
async fn daemon_answers_within(client: &Client, uid: u32, pause: Duration) -> bool {
    for _ in 0..30 {
        if daemon_answers(client, uid).await {
            return true;
        }

        tokio::time::sleep(pause).await;
    }

    false
}

/// Ask for administrator rights once in a run, to install the helper's job.
fn job_rights(rights: &mut setup::AdministratorRights, granted: &mut bool) -> Result<()> {
    if !*granted {
        administrator_rights(
            rights,
            "  macOS asks for your password once, to install the helper's background job.",
        )?;
        *granted = true;
    }

    Ok(())
}

/// Bring up a helper daemon that does not answer: the person signs the
/// helper in, and (unless an earlier setup's job starts by itself) Atlas
/// installs the helper's background job with administrator rights.
async fn start_helper(
    client: &Client,
    config: &Config,
    helper: &account::Account,
    done_steps: &mut Vec<Value>,
    expected: &helper_setup::identity::Identity,
    rights: &mut setup::AdministratorRights,
) -> Result<()> {
    // Set up before: the helper's job is installed and starts by itself
    // once the helper signs in (after a restart, say). No password then.
    let installed = setup::migrate::settled(helper.uid);
    let mut granted = false;

    if !installed {
        job_rights(rights, &mut granted)?;
    }

    if atlas_core::session::logged_in(helper.uid) != Some(true) {
        log_in_once(client, helper, expected, installed, done_steps).await?;
    }

    let mut started = false;

    if installed {
        let _spinner = Spinner::new("Waiting for the helper to start…");
        started = daemon_answers_within(
            client,
            config.desktop().second_login().uid,
            Duration::from_secs(2),
        )
        .await;
        if started {
            mark_since_lock("answering");
        }
    }

    if !started {
        job_rights(rights, &mut granted)?;
        install_job(client, config, helper, expected).await?;
        done_steps.push(json!({"step": "daemon", "label": LABEL}));
    }

    Ok(())
}

/// Have the person log in to the helper, offering to lock the screen so
/// they can switch to it, and wait until they have.
async fn log_in_once(
    client: &Client,
    helper: &account::Account,
    expected: &helper_setup::identity::Identity,
    installed: bool,
    done_steps: &mut Vec<Value>,
) -> Result<()> {
    let name = &helper.name;

    // A helper never set up skips macOS's setup questions at its first
    // login. Only a convenience: a refusal leaves them to the person,
    // and the step says what came of it.
    if !installed {
        let step = match first_login_setup(client, helper, expected) {
            Ok(reply) => json!({"step": "firstLogin", "answered": reply["answered"]}),
            Err(error) => json!({"step": "firstLogin", "error": error.error}),
        };
        done_steps.push(step);
    }

    heading(&format!("Log in to {name} once"));

    if installed {
        say(&format!(
            "  {name} is logged out (after a restart or a log-out). Switch to {name}, log in,\n  \
                 then switch back here."
        ));
    } else {
        say(&format!(
            "  Switch to {name} and log in. If macOS asks you to set anything up, choose\n  \
                 Set Up Later. If it asks about Accessibility or Screen Recording for Atlas,\n  \
                 turn Atlas on. Then switch back here."
        ));
    }

    offer_lock(name, helper.uid)?;

    {
        let _spinner = Spinner::new(format!(
            "Waiting for {name} to log in… (Ctrl-C to stop; run atlas init again later)"
        ));
        wait_for("the helper to log in", async || {
            (atlas_core::session::logged_in(helper.uid) == Some(true)).then_some(())
        })
        .await?;
    }

    done(&format!("{name} logged in"), "");
    Ok(())
}

/// Install the helper's background job and wait for its daemon to answer.
async fn install_job(
    client: &Client,
    config: &Config,
    helper: &account::Account,
    expected: &helper_setup::identity::Identity,
) -> Result<()> {
    // The rights may have lapsed during a long wait: ask again then.
    if !setup::administrator_rights_current() {
        say("  Your administrator rights expired while Atlas waited.");
        attended(
            "/usr/bin/sudo",
            &["-v"],
            "Administrator rights were not granted",
        )?;
    }

    let spinner = Spinner::new("Installing the helper's background job…");
    let log = prepare_helper(client, helper, expected).await?;

    spinner.set("Starting the helper…");
    let answered = daemon_answers_within(
        client,
        config.desktop().second_login().uid,
        Duration::from_secs(1),
    )
    .await;
    drop(spinner);
    if !answered {
        return Err(problem(format!(
            "The helper's background job was installed but did not start. Its log is \
                 {} in {}'s account; run atlas init again after checking it.",
            log.display(),
            helper.name
        )));
    }

    mark_since_lock("answering");
    done("Helper job", "installed and running");
    Ok(())
}

/// Unless `status` has the helper in the background, wait until it is; then
/// until the person is back in this login and settled there: work started
/// while they are unlocking would land on the switch itself.
async fn wait_until_back(client: &Client, uid: u32, status: &Value) -> Result<()> {
    if atlas_core::session::require_hidden(status, uid).is_err() {
        let _spinner = Spinner::new("Waiting for you to switch back to your own account…");
        wait_for("you to switch back", async || {
            atlas_core::session::require_hidden(&client.status().await, uid)
                .is_ok()
                .then_some(())
        })
        .await?;
        mark_since_lock("hidden");
    }

    atlas_client::switch::until_back(PATIENCE).await
}

/// Screen Recording and Accessibility for Atlas in the helper: already
/// allowed, or guided until the person allows both there and comes back.
async fn permissions_step(client: &Client, config: &Config, name: &str) -> Result<()> {
    let status = client.status().await;
    if missing(&status).is_empty() {
        // macOS keeps these approvals for Atlas across the Mac, so a helper
        // can have them before it is ever asked.
        done(
            "Permissions",
            "Screen Recording · Accessibility already allowed",
        );
        wait_until_back(client, config.desktop().second_login().uid, &status).await?;
    } else {
        heading(&format!("Allow Atlas in {name}'s account"));

        say(&format!(
            "  Atlas needs two macOS permissions there:\n    \
             · Screen Recording, to see {name}'s screen\n    \
             · Accessibility, to click and type there\n  \
             1. Switch to {name} (log in if asked).\n  \
             2. macOS asks about Accessibility: click Open System Settings and turn Atlas on.\n  \
             3. System Settings then shows Screen & System Audio Recording: turn Atlas on\n     \
             there too. If macOS offers to quit and reopen Atlas, either choice is fine.\n  \
             4. When Atlas says it is set up there, switch back here. Atlas carries on by itself."
        ));

        loop {
            // The person is here, in their own account: offer the way over.
            if client.status().await["session"]["onConsole"] != true {
                offer_lock(name, config.desktop().second_login().uid)?;
            }

            // Done once the helper has been on screen and is hidden again (or
            // everything is already granted).
            {
                let spinner = Spinner::new("Waiting for you to allow and come back…");
                let mut visited = false;
                wait_for("you to allow and switch back", async || {
                    let status = client.status().await;
                    if status["session"]["onConsole"] == true {
                        visited = true;
                    }

                    let mark = |key: &str| style::mark(status["permissions"][key].as_bool());
                    spinner.set(format!(
                        "Waiting for you to allow and come back…  Screen Recording {}  Accessibility {}",
                        mark("screenCapture"),
                        mark("accessibility")
                    ));

                    let observation = atlas_core::session::Observation::decode(&status);
                    let back = config.desktop().prove(&observation).is_ok();
                    (missing(&status).is_empty() || (visited && back)).then_some(())
                })
                .await?;
            }

            // macOS applies a new Screen Recording grant to new processes
            // only, so the helper restarts to pick up what the person allowed.
            if !missing(&client.status().await).is_empty() {
                wait_until_back(
                    client,
                    config.desktop().second_login().uid,
                    &client.status().await,
                )
                .await?;
                let _spinner = Spinner::new("Restarting the helper to pick up the permissions…");
                restart_helper(client).await?;
            }

            let lacking = missing(&client.status().await);
            if lacking.is_empty() {
                break;
            }

            say(&format!(
                "{} Atlas is still off. In {name}'s session, open System Settings ›\n  \
                 Privacy & Security and turn Atlas on in:",
                style::fail()
            ));

            for pane in &lacking {
                say(&format!("    · {pane}"));
            }

            if !prompt::confirm("Go back and turn it on now?", true, "")? {
                return Err(problem(format!(
                    "Atlas is still off in {name}'s {}. Turn it on there, switch back \
                     and run atlas init again.",
                    lacking.join(" and ")
                )));
            }
        }

        done("Permissions", "Screen Recording · Accessibility");
        // Both can be on while the person is still in the helper: the checks
        // below need it in the background again.
        wait_until_back(
            client,
            config.desktop().second_login().uid,
            &client.status().await,
        )
        .await?;
    }

    Ok(())
}

/// Recover, start input, record the setup and check it end to end.
async fn verify_step(client: &Client, config_path: &Path) -> Result<()> {
    recover(client).await?;

    let _spinner = Spinner::new("Starting input and checking everything…");
    let doctor = setup::migrate::finalize(config_path, client, Code::Init).await?;
    if !doctor.ready {
        return Err(problem(
            "Setup finished but is not ready; run atlas doctor to see why",
        ));
    }

    Ok(())
}
