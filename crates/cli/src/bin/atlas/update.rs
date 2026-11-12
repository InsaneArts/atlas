//! App replacement and handover to the newly installed executable.
use atlas_cli::ui::progress::Spinner;
use atlas_client::client::Client;
use atlas_core::host::HostSession;
use atlas_core::{
    config::Config,
    desktop::Kind,
    error::{Code, Error, Result},
};
use serde_json::{Value, json};

/// The configuration, or None when Atlas is not set up here yet. A file
/// that is there but does not load is an error, never "not set up".
fn set_up(config: &std::path::Path) -> Result<Option<Config>> {
    match Config::load(config) {
        Ok(loaded) => Ok(Some(loaded)),
        Err(error) if error.code == Code::Config => Ok(None),
        Err(error) => Err(error),
    }
}

/// Say whether a newer release is out, or update the app, then hand over to
/// the new executable, so the steps that move the helper onto it are always
/// the new version's own.
pub(super) async fn run(
    config: &std::path::Path,
    args: super::cli::Update,
    host: Option<&HostSession>,
    allow_disrupt: bool,
) -> Result<Value> {
    if args.check {
        return atlas_client::update::newer(&atlas_client::update::Source::from_env(args.version));
    }

    super::dispatch::as_yourself("update")?;
    update_using(
        config,
        args.allow_unnotarized,
        args.force,
        args.version,
        host,
        allow_disrupt,
        Client::new,
    )
    .await
}

async fn update_using(
    config: &std::path::Path,
    allow_unnotarized: bool,
    force: bool,
    version: Option<String>,
    host: Option<&HostSession>,
    allow_disrupt: bool,
    make_client: impl FnOnce(Config) -> Result<Client>,
) -> Result<Value> {
    let source = atlas_client::update::Source::from_env(version);

    // Nothing else may control the helper while its app is replaced: no
    // viewer or agent, and no action of unknown outcome. The new version's
    // steps take control again one by one.
    let client = match set_up(config)? {
        Some(loaded) => {
            let mut client = make_client(loaded)?;
            client.host_session = host.cloned();
            client.controller = client.controller.labelled(host);
            client.allow_disrupt = allow_disrupt;
            Some(client)
        }
        None => None,
    };
    let exclusion = match &client {
        Some(client) => Some(client.service_change(false).await?),
        None => None,
    };

    // One update at a time, from the download to the handover.
    let exclusive = atlas_client::update::exclusive()?;

    let spinner = Spinner::new("Checking for an update…");
    let mut report = atlas_client::update::install(
        &source,
        allow_unnotarized,
        force,
        &exclusive,
        async |program| preflight(config, program, client.as_ref()).await,
    )
    .await?;

    drop(exclusion);

    if report["updated"] != true {
        return Ok(report);
    }

    spinner.set("Moving the helper onto the new version…");
    let executable = atlas_core::app_bundle::installed_executable()?;

    // update-finish always answers in JSON.
    let mut finish = std::process::Command::new(&executable);
    finish.arg("update-finish");
    if allow_disrupt {
        finish.arg("--allow-disrupt");
    }
    let output = atlas_core::spawn::output(finish.arg("--config").arg(config))?;
    let (moved, finished) = helper_moved(&output);
    report["ok"] = json!(moved);
    report["helper"] = finished;

    if moved {
        // The new version moves a running menu bar onto itself as it
        // finishes; one older than that leaves it to this one. Either way
        // it stops before the previous app, the bundle it ran from, goes.
        let handed_over = report["helper"].get("menubarRestarted").cloned();
        let port = retire_previous(
            &mut report,
            || match handed_over {
                Some(_) => Ok(None),
                None => stop_menubar(config),
            },
            atlas_client::update::finished,
        )?;
        if let Some(restarted) = handed_over.filter(Value::is_boolean) {
            report["menubarRestarted"] = restarted;
        }

        if let Some(port) = port {
            spinner.set("Restarting the menu bar…");
            report["menubarRestarted"] = json!(restart_menubar(&executable, config, port));
        }
    } else if report["helper"]["failedStep"] == "menubar" {
        report["next"] = json!(QUIT_MENU_BAR);
    } else {
        report["next"] = json!("Fix the failed step, then run atlas update again to resume");
    }

    Ok(report)
}

/// What the person does when a menu bar may still run from the previous app.
const QUIT_MENU_BAR: &str = "Quit the menu bar, then run atlas update again";

/// Start the menu bar with `executable` on `port`, as the settings at
/// `config` say: whether it started.
fn restart_menubar(executable: &std::path::Path, config: &std::path::Path, port: u16) -> bool {
    atlas_core::spawn::status(
        std::process::Command::new(executable)
            .arg("menubar")
            .arg("--port")
            .arg(port.to_string())
            .arg("--config")
            .arg(config)
            .env("ATLAS_JSON", "1")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null()),
    )
    .is_ok_and(|status| status.success())
}

/// Stop the menu bar of the settings at `config`. Settings that don't say
/// where its records are leave it known not to run only when no other
/// Atlas process runs.
fn stop_menubar(config: &std::path::Path) -> Result<Option<u16>> {
    let output = Config::load(config)
        .and_then(Client::new)
        .and_then(|client| client.output_dir().map(std::path::Path::to_path_buf));
    atlas_cli::menubar::stop_located(output)
}

/// Stop a running menu bar with `stop`, then drop the previous app with
/// `finish`, noting it in `report`: the port the menu bar served, to start
/// it again there. A menu bar not seen to stop may still run from the
/// previous app, which is kept, as is the record that resumes the update.
fn retire_previous(
    report: &mut Value,
    stop: impl FnOnce() -> Result<Option<u16>>,
    finish: impl FnOnce() -> Result<Value>,
) -> Result<Option<u16>> {
    let port = stop().map_err(previous_kept)?;
    report["previous"] = finish()?;
    Ok(port)
}

/// `error`, the menu bar's failure to stop, saying the previous app stays.
fn previous_kept(error: Error) -> Error {
    Error::new(
        error.code,
        format!(
            "{}, so the previous Atlas.app was kept. Quit the menu bar, then run atlas update \
             again.",
            error.error
        ),
    )
    .with("next", json!(QUIT_MENU_BAR))
    .with_source(error)
}

/// Move the running menu bar onto this version, noting it in `finished`,
/// what finishing the update says: stop it with `stop`, then `restart` it on
/// the port it served, `menubarRestarted` saying whether it did (null when
/// none ran). One not seen to stop may still run from the previous app, so
/// the reply becomes the failure of the `menubar` step, which keeps that
/// app and the record that resumes the update.
fn move_menubar(
    mut finished: Value,
    stop: impl FnOnce() -> Result<Option<u16>>,
    restart: impl FnOnce(u16) -> bool,
) -> Value {
    match stop() {
        Ok(port) => {
            finished["menubarRestarted"] = json!(port.map(restart));
            finished
        }
        Err(error) => json!({
            "ok": false,
            "failedStep": "menubar",
            "failure": previous_kept(error),
            "completedSteps": finished["completedSteps"],
        }),
    }
}

/// Check the staged app immediately before replacement, while still holding control.
async fn preflight(
    config: &std::path::Path,
    program: &std::path::Path,
    client: Option<&Client>,
) -> Result<()> {
    if let Some(loaded) = set_up(config)? {
        match loaded.desktop().kind() {
            Kind::User(login) => preflight_in(config, program, login).await?,
        }
    }
    if let Some(client) = client {
        client.require_service_idle().await?;
    }
    Ok(())
}

/// Move the helper onto this (just installed) executable: the shared
/// executable, the daemon, the input service, and (when a setup is
/// recorded) setup's verification of the new signed build. Stops at the
/// first failure and names it. With no helper job yet (setup stopped before
/// installing it), there is nothing to move: atlas init finishes setup.
/// Then a running menu bar moves onto this executable too.
pub(super) async fn update_finish(
    config: &std::path::Path,
    host: Option<&HostSession>,
    allow_disrupt: bool,
) -> Result<Value> {
    super::dispatch::as_yourself("update")?;
    update_finish_using(config, host, allow_disrupt, Client::new).await
}

async fn update_finish_using(
    config: &std::path::Path,
    host: Option<&HostSession>,
    allow_disrupt: bool,
    make_client: impl FnOnce(Config) -> Result<Client>,
) -> Result<Value> {
    let Some(loaded) = set_up(config)? else {
        return Ok(json!({"ok": true, "next": "atlas init"}));
    };

    let mut client = make_client(loaded.clone())?;
    client.host_session = host.cloned();
    client.allow_disrupt = allow_disrupt;
    client.require_service_idle().await?;
    let daemon = client.status().await["daemon"]["pid"].is_number();
    let executable = std::env::current_exe()?;
    let steps = match loaded.desktop().kind() {
        Kind::User(login) => helper_steps_in(login, daemon),
    };
    let finished = if let Some(steps) = steps {
        let done = match run_steps(&steps, |step| {
            let mut command = std::process::Command::new(&executable);
            command.args(step);
            if allow_disrupt {
                command.arg("--allow-disrupt");
            }
            atlas_core::spawn::output(command.arg("--json").arg("--config").arg(config))
        })? {
            Ok(done) => done,
            Err(failed) => return Ok(failed),
        };

        let agents = atlas_agents::refresh();
        json!({"ok": true, "completedSteps": done, "agents": agents})
    } else {
        json!({"ok": true, "completedSteps": [], "next": "atlas init"})
    };

    // The menu bar moves here, by this version's rules, even when an older
    // version started the update.
    Ok(move_menubar(
        finished,
        || atlas_cli::menubar::stop_located(client.output_dir().map(std::path::Path::to_path_buf)),
        |port| restart_menubar(&executable, config, port),
    ))
}

/// [`preflight`] for the second login `login`: a setup still being moved
/// onto a new build checks the staged app first.
async fn preflight_in(
    config: &std::path::Path,
    program: &std::path::Path,
    login: &atlas_core::user::User,
) -> Result<()> {
    if atlas_client::user::setup::migrate::in_effect(login.uid) {
        atlas_client::user::setup::migrate::preflight_apply(config, program).await?;
    }
    Ok(())
}

/// The `atlas` steps that move the second login `login` onto this
/// executable, when it has a helper job (`daemon`: its daemon answers);
/// none when there is nothing to move.
fn helper_steps_in(
    login: &atlas_core::user::User,
    daemon: bool,
) -> Option<Vec<&'static [&'static str]>> {
    if !has_helper_job(
        atlas_client::user::setup::migrate::recorded(login.uid),
        daemon,
    ) {
        return None;
    }
    // The daemon restarts first: a daemon whose app was swapped under it
    // can no longer vouch (to TCC) for a new input service it starts,
    // while the input service it already runs is cleaned up by the
    // restart itself.
    let mut steps: Vec<&[&str]> = vec![&["install"], &["restart"], &["start-input"]];

    if atlas_client::user::setup::migrate::in_effect(login.uid) {
        steps.push(&["setup", "--apply"]);
    }

    Some(steps)
}

/// Run `steps` in order through `run`, stopping at the first that fails:
/// the steps completed, or the failed-step report.
fn run_steps(
    steps: &[&[&str]],
    mut run: impl FnMut(&[&str]) -> std::io::Result<std::process::Output>,
) -> Result<std::result::Result<Vec<String>, Value>> {
    let mut done = Vec::new();

    for step in steps {
        if let Err(failure) = acknowledged(&run(step)?, "The step") {
            return Ok(Err(json!({
                "ok": false,
                "failedStep": step.join(" "),
                "failure": failure,
                "completedSteps": done,
            })));
        }

        done.push(step.join(" "));
    }

    Ok(Ok(done))
}

/// Whether `update-finish` moved the helper, and its report.
fn helper_moved(output: &std::process::Output) -> (bool, Value) {
    match acknowledged(output, "The new version") {
        Ok(finished) => (true, finished),
        Err(failure) => (false, failure),
    }
}

/// The reply of an Atlas command run here, `who`, when it acknowledged
/// success: it exited successfully and answered an object whose `ok` is
/// true. Otherwise its failure: its own reply when that says `ok` is false,
/// or else one saying it did not report or confirm.
fn acknowledged(output: &std::process::Output, who: &str) -> std::result::Result<Value, Value> {
    match child_reply(output) {
        Ok(reply) if reply["ok"] == false => Err(reply),
        Ok(reply) if output.status.success() && reply["ok"] == true => Ok(reply),
        Ok(_) => Err(json!({"ok": false, "error": format!("{who} did not confirm it finished")})),
        Err(_) => Err(json!({"ok": false, "error": format!("{who} did not report")})),
    }
}

/// What an Atlas command run here answered: replies go to stdout, errors
/// to stderr.
fn child_reply(output: &std::process::Output) -> serde_json::Result<Value> {
    serde_json::from_slice(&output.stdout).or_else(|_| serde_json::from_slice(&output.stderr))
}

/// Whether the helper has a job to move onto a new version: a setup is
/// recorded, or its daemon answers (setup stopped after installing it).
fn has_helper_job(recorded: bool, daemon_answers: bool) -> bool {
    recorded || daemon_answers
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;
