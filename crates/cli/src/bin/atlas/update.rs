//! App replacement and handover to the newly installed executable.
use atlas_cli::ui::progress::Spinner;
use atlas_client::client::Client;
use atlas_core::host::HostSession;
use atlas_core::{
    config::{
        Config,
        desktops::{Selected, Settings},
    },
    error::{Code, Error, Result},
};
use serde_json::{Value, json};

/// The configuration, or None when Atlas is not set up here yet. A file
/// that is there but does not load is an error, never "not set up".
fn set_up(config: &std::path::Path) -> Result<Option<Config>> {
    match Config::load_user(config) {
        Ok(loaded) => Ok(Some(loaded)),
        Err(error) if error.code == Code::Config => Ok(None),
        Err(error) => Err(error),
    }
}

/// The settings, of every desktop, or None when Atlas is not set up here
/// yet, as [`set_up`] judges it.
fn settings_in(config: &std::path::Path) -> Result<Option<Settings>> {
    match Settings::load(config) {
        Ok(loaded) => Ok(Some(loaded)),
        Err(error) if error.code == Code::Config => Ok(None),
        Err(error) => Err(error),
    }
}

/// Where the menu bar of `settings` keeps its records: the folder the whole
/// Mac keeps its runs in, whichever desktops they list.
fn menubar_records(settings: &Settings) -> Result<std::path::PathBuf> {
    let records = settings.default_desktop().desktop.output_dir();
    atlas_core::files::private_folder(records, Code::Config)?;
    Ok(records.to_owned())
}

/// `work`, which waits on keepers and launchd, off the runtime.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| Error::new(Code::Update, "The update's check of its VMs stopped."))?
}

/// A client of the Second Login `config` names: an update moves it alone.
fn helper_client(config: Config) -> Result<Client> {
    Client::new(Selected::provisional(config))
}

/// Say whether a newer release is out, or update the app, then hand over to
/// the new executable, so the steps that move the helper onto it are always
/// the new version's own. With Atlas current here, each VM whose Atlas is
/// not this build is brought to it instead, under the update's own hold.
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
    update_using(config, args, host, allow_disrupt, helper_client).await
}

/// [`run`]'s update, as `args` asks it.
async fn update_using(
    config: &std::path::Path,
    args: super::cli::Update,
    host: Option<&HostSession>,
    allow_disrupt: bool,
    make_client: impl FnOnce(Config) -> Result<Client>,
) -> Result<Value> {
    let super::cli::Update {
        allow_unnotarized,
        force,
        version,
        retry_vms,
        ..
    } = args;
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
    // So is the VM of every session keeper launchd runs, whatever settings
    // this update was given, the update refused while work runs there; each
    // keeper is handed over once the new build is in place.
    let keepers = std::sync::Arc::new(
        blocking(move || atlas_client::vm::handover::hold_for_update(allow_disrupt)).await?,
    );

    // One update at a time, from the download to the handover.
    let exclusive = atlas_client::update::exclusive()?;

    let spinner = Spinner::new("Checking for an update…");
    let mut report = atlas_client::update::install(
        &source,
        allow_unnotarized,
        force,
        &exclusive,
        async |program| {
            preflight(config, program, client.as_ref()).await?;
            let held = keepers.clone();
            blocking(move || held.idle(allow_disrupt)).await
        },
    )
    .await?;

    drop(exclusion);

    if report["updated"] != true {
        // Atlas is current here: each VM not on this build is brought to
        // it, under this update's own hold and Stop; one whose update to it
        // failed only when the person asked to try again.
        let held = std::sync::Arc::into_inner(keepers).ok_or_else(|| {
            Error::new(Code::Update, "The update's hold of its VMs was not let go.")
        })?;
        if !held.is_empty() {
            spinner.set("Bringing each VM's Atlas to this build…");
        }
        let vms = blocking(move || held.bring_to_this_build(allow_disrupt, retry_vms)).await?;
        if !vms.is_empty() {
            report["vms"] = json!(vms);
        }
        return Ok(report);
    }
    drop(keepers);

    spinner.set("Moving the helper onto the new version…");
    let executable = atlas_core::app_bundle::installed_executable()?;

    // update-finish always answers in JSON.
    let mut finish = std::process::Command::new(&executable);
    finish.arg("update-finish");
    // It moves the Second Login, this command's desktop.
    if let Some((variable, key)) = atlas_core::config::desktops::for_children() {
        finish.env(variable, key);
    }
    if allow_disrupt {
        finish.arg("--allow-disrupt");
    }
    // The VMs' keepers are handed over only with a token this attempt made
    // and gives on standard input alone: never by a finish run by hand, nor
    // with a token another attempt made.
    let token = atlas_client::update::grant_handover()?;
    let output = authorized(finish.arg("--config").arg(config), token.as_deref())?;
    let (moved, mut finished) = helper_moved(&output);
    // What each VM's Atlas came to is the update's own to say.
    if let Some(vms) = finished
        .as_object_mut()
        .and_then(|finished| finished.remove("vms"))
    {
        report["vms"] = vms;
    }
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
    let mut command = std::process::Command::new(executable);
    if let Some((variable, key)) = atlas_core::config::desktops::for_children() {
        command.env(variable, key);
    }
    atlas_core::spawn::status(
        command
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
    let output = match Config::load_user(config) {
        // Settings that list no Second Login still say where its records are.
        Err(error) if error.code == Code::Config => {
            settings_in(config).and_then(|settings| menubar_records(&settings.ok_or(error)?))
        }
        loaded => loaded
            .and_then(helper_client)
            .and_then(|client| client.output_dir().map(std::path::Path::to_path_buf)),
    };
    stop_menubar_at(output)
}

/// Stop the menu bar whose records `output` holds; when where they are cannot
/// be told, the session keepers launchd runs are never taken for one.
fn stop_menubar_at(output: Result<std::path::PathBuf>) -> Result<Option<u16>> {
    let known = if output.is_err() {
        atlas_client::vm::handover::keeper_pids()
    } else {
        Vec::new()
    };
    atlas_cli::menubar::stop_located_beside(output, &known)
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
        crate::user::update::preflight_in(config, program, &loaded.user()).await?;
    }
    if let Some(client) = client {
        client.require_service_idle().await?;
    }
    Ok(())
}

/// Move the desktops onto this (just installed) executable: each VM's
/// session keeper, handed over to this build and asked to bring its VM's
/// Atlas to it (each answer in `vms`), then the helper: the shared
/// executable, the daemon, the input service, and (when a setup is
/// recorded) setup's verification of the new signed build. Stops at the
/// first failure and names it. With no helper job yet (setup stopped before
/// installing it), there is nothing to move: atlas init finishes setup.
/// Settings that list no Second Login move their VMs alone. Then a running
/// menu bar moves onto this executable too.
pub(super) async fn update_finish(
    config: &std::path::Path,
    host: Option<&HostSession>,
    allow_disrupt: bool,
) -> Result<Value> {
    super::dispatch::as_yourself("update")?;
    let given = given_token();
    update_finish_with(config, host, allow_disrupt, helper_client, given.as_deref()).await
}

/// [`update_finish`] given no update's token: no VM's keeper is handed over.
#[cfg(test)]
async fn update_finish_using(
    config: &std::path::Path,
    host: Option<&HostSession>,
    allow_disrupt: bool,
    make_client: impl FnOnce(Config) -> Result<Client>,
) -> Result<Value> {
    update_finish_with(config, host, allow_disrupt, make_client, None).await
}

/// [`update_finish`], given `token` on its standard input: the VMs' keepers
/// are handed over only when it is the token this attempt at the update
/// granted, which taking it uses up; the settings given, even missing,
/// never keep a keeper from it.
async fn update_finish_with(
    config: &std::path::Path,
    host: Option<&HostSession>,
    allow_disrupt: bool,
    make_client: impl FnOnce(Config) -> Result<Client>,
    token: Option<&str>,
) -> Result<Value> {
    let executable = std::env::current_exe()?;
    let mut run = |step: &[&str]| {
        let mut command = std::process::Command::new(&executable);
        command.args(step);
        if let Some((variable, key)) = atlas_core::config::desktops::for_children() {
            command.env(variable, key);
        }
        if allow_disrupt {
            command.arg("--allow-disrupt");
        }
        atlas_core::spawn::output(command.arg("--json").arg("--config").arg(config))
    };
    // Each session keeper launchd runs is handed over to this build first, a
    // step each, by the settings its agent pins, whatever settings this
    // finish was given: only for the attempt whose own token it was given,
    // which is used up, so a finish run by hand never disrupts their work.
    let authorized = match token {
        Some(token) => atlas_client::update::take_handover(token)?,
        None => false,
    };
    let found = if authorized {
        match atlas_client::vm::handover::discover() {
            Ok(found) => found,
            Err(error) => {
                return Ok(json!({"ok": false, "failedStep": "station handover",
                    "failure": error, "completedSteps": []}));
            }
        }
    } else {
        Vec::new()
    };
    let keepers: Vec<(String, String)> = found
        .iter()
        .map(|keeper| (keeper.key(), keeper.settings().display().to_string()))
        .collect();
    // Then each one handed over brings its VM's Atlas to this build; what
    // each came to is said, and none stops the rest.
    let (handed, vms) = keepers_moved(&keepers, &mut run)?;
    let with_vms = |mut finished: Value| {
        if !vms.is_empty() {
            finished["vms"] = json!(vms);
        }
        finished
    };
    let handed = match handed {
        Ok(handed) => handed,
        Err(failed) => return Ok(with_vms(failed)),
    };

    // The settings given move the helper and the menu bar alone.
    let Some(settings) = settings_in(config)? else {
        return Ok(with_vms(if handed.is_empty() {
            json!({"ok": true, "next": "atlas init"})
        } else {
            json!({"ok": true, "completedSteps": handed, "agents": atlas_agents::refresh(),
                "next": "atlas init"})
        }));
    };
    let Ok(loaded) = settings.user() else {
        let finished = if handed.is_empty() {
            json!({"ok": true, "completedSteps": []})
        } else {
            json!({"ok": true, "completedSteps": handed, "agents": atlas_agents::refresh()})
        };
        return Ok(with_vms(move_menubar(
            finished,
            || stop_menubar_at(menubar_records(&settings)),
            |port| restart_menubar(&executable, config, port),
        )));
    };

    let mut client = make_client(loaded.clone())?;
    client.host_session = host.cloned();
    client.allow_disrupt = allow_disrupt;
    client.require_service_idle().await?;
    let daemon = client.status().await["daemon"]["pid"].is_number();
    let mut done = handed;
    let steps = crate::user::update::helper_steps_in(&loaded.user(), daemon);
    let finished = if let Some(steps) = steps {
        match run_steps(&steps, &mut run)? {
            Ok(moved) => done.extend(moved),
            Err(mut failed) => {
                if let Some(moved) = failed["completedSteps"].as_array_mut() {
                    moved.splice(0..0, done.into_iter().map(Value::from));
                }
                return Ok(with_vms(failed));
            }
        }

        let agents = atlas_agents::refresh();
        json!({"ok": true, "completedSteps": done, "agents": agents})
    } else {
        json!({"ok": true, "completedSteps": done, "next": "atlas init"})
    };

    // The menu bar moves here, by this version's rules, even when an older
    // version started the update.
    Ok(with_vms(move_menubar(
        finished,
        || stop_menubar_at(client.output_dir().map(std::path::Path::to_path_buf)),
        |port| restart_menubar(&executable, config, port),
    )))
}

/// Hand each keeper, by its record key and the settings its agent pins,
/// over to this build, a step each through `run`, stopping at the first that
/// fails; then ask each one handed over, a handover that failed after it
/// included, to bring its VM's Atlas to that build ([`guest_updates`]). The
/// handovers done, or the failed step's report, and what each VM came to.
fn keepers_moved(
    keepers: &[(String, String)],
    mut run: impl FnMut(&[&str]) -> std::io::Result<std::process::Output>,
) -> Result<(Steps, Vec<Value>)> {
    let handovers: Vec<[&str; 6]> = keepers
        .iter()
        .map(|(key, settings)| ["station", "handover", "--key", key, "--settings", settings])
        .collect();
    let handovers: Vec<&[&str]> = handovers.iter().map(<[&str; 6]>::as_slice).collect();
    let handed = run_steps(&handovers, &mut run)?;
    let over = match &handed {
        Ok(handed) => handed.len(),
        Err(failed) => failed["completedSteps"].as_array().map_or(0, Vec::len),
    };
    let vms = guest_updates(keepers.get(..over).unwrap_or_default(), run);
    Ok((handed, vms))
}

/// Ask each keeper, by its record key and the settings its agent pins, to
/// bring its VM's Atlas to its own build, a step each through `run`: what
/// each answered, or why it did not, in order. A step that fails stops
/// none of the rest: each VM's Atlas is its own.
fn guest_updates(
    keepers: &[(String, String)],
    mut run: impl FnMut(&[&str]) -> std::io::Result<std::process::Output>,
) -> Vec<Value> {
    keepers
        .iter()
        .map(|(key, settings)| {
            let step = [
                "station",
                "update-guest",
                "--key",
                key,
                "--settings",
                settings,
            ];
            let answered = match run(&step) {
                Ok(output) => acknowledged(&output, "The VM's update step"),
                Err(error) => Err(json!({"ok": false,
                    "error": format!("The VM's update step did not run: {error}")})),
            };
            let mut answered = answered.unwrap_or_else(|failed| failed);
            if answered.get("desktop").is_none() {
                answered["key"] = json!(key);
            }
            answered
        })
        .collect()
}

/// The token the update that started this finish gave it on its standard
/// input; none from a terminal, from nothing, or when none comes at once.
fn given_token() -> Option<String> {
    use std::io::{BufRead, IsTerminal, Read};
    if std::io::stdin().is_terminal() {
        return None;
    }
    let (said, heard) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let read = std::io::stdin()
            .lock()
            .take(128)
            .read_line(&mut line)
            .map(|_| line);
        let _ = said.send(read);
    });
    let line = heard
        .recv_timeout(std::time::Duration::from_secs(2))
        .ok()?
        .ok()?;
    atlas_core::files::Token::parse(line.trim()).map(|token| token.as_str().to_owned())
}

/// Run `command`, an Atlas command run here, giving it `token` on its
/// standard input: what it wrote.
fn authorized(
    command: &mut std::process::Command,
    token: Option<&str>,
) -> std::io::Result<std::process::Output> {
    use std::io::Write;
    command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = atlas_core::spawn::spawn(command)?;
    if let Some(mut input) = child.stdin.take() {
        // A child that reads none of it is no failure.
        let _ = writeln!(input, "{}", token.unwrap_or_default());
    }
    child.wait_with_output()
}

/// The steps an update ran, or the report of the one that failed.
type Steps = std::result::Result<Vec<String>, Value>;

/// Run `steps` in order through `run`, stopping at the first that fails:
/// the steps completed, or the failed-step report.
fn run_steps(
    steps: &[&[&str]],
    mut run: impl FnMut(&[&str]) -> std::io::Result<std::process::Output>,
) -> Result<Steps> {
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

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;
