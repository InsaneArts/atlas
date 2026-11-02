//! App replacement and handover to the newly installed executable.
use atlas_cli::ui::progress::Spinner;
use atlas_client::client::Client;
use atlas_core::host::HostSession;
use atlas_core::{
    config::Config,
    error::{Code, Result},
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
    let output = finish.arg("--config").arg(config).output()?;
    let finished = child_reply(&output)
        .unwrap_or_else(|_| json!({"ok": false, "error": "The new version did not report"}));
    report["ok"] = finished["ok"].clone();
    report["helper"] = finished;

    if report["ok"] == true {
        // A running menu bar moves onto the new version too. It stops before
        // the previous app, the bundle it still runs from, is removed.
        let output = Config::load(config)
            .ok()
            .and_then(|loaded| Client::new(loaded).ok())
            .and_then(|client| client.output_dir().ok().map(std::path::Path::to_path_buf));
        let port = output.and_then(|output| atlas_cli::menubar::stop(&output));

        report["previous"] = atlas_client::update::finished()?;

        if let Some(port) = port {
            spinner.set("Restarting the menu bar…");

            let restarted = std::process::Command::new(&executable)
                .arg("menubar")
                .arg("--port")
                .arg(port.to_string())
                .arg("--config")
                .arg(config)
                .env("ATLAS_JSON", "1")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|status| status.success());
            report["menubarRestarted"] = json!(restarted);
        }
    } else {
        report["next"] = json!("Fix the failed step, then run atlas update again to resume");
    }

    Ok(report)
}

/// Check the staged app immediately before replacement, while still holding control.
async fn preflight(
    config: &std::path::Path,
    program: &std::path::Path,
    client: Option<&Client>,
) -> Result<()> {
    if let Some(loaded) = set_up(config)?
        && atlas_client::setup::migrate::in_effect(loaded.helper_uid)
    {
        atlas_client::setup::migrate::preflight_apply(config, program).await?;
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
    if !has_helper_job(
        atlas_client::setup::migrate::recorded(loaded.helper_uid),
        daemon,
    ) {
        return Ok(json!({"ok": true, "completedSteps": [], "next": "atlas init"}));
    }

    let executable = std::env::current_exe()?;

    // The daemon restarts first: a daemon whose app was swapped under it can
    // no longer vouch (to TCC) for a new input service it starts, while the
    // input service it already runs is cleaned up by the restart itself.
    let mut steps: Vec<&[&str]> = vec![&["install"], &["restart"], &["start-input"]];

    if atlas_client::setup::migrate::in_effect(loaded.helper_uid) {
        steps.push(&["setup", "--apply"]);
    }

    let mut done = Vec::new();

    for step in steps {
        let mut command = std::process::Command::new(&executable);
        command.args(step);
        if allow_disrupt {
            command.arg("--allow-disrupt");
        }
        let output = command.arg("--json").arg("--config").arg(config).output()?;
        let reply = child_reply(&output).unwrap_or(Value::Null);
        if !output.status.success() || reply["ok"] == false {
            return Ok(json!({
                "ok": false,
                "failedStep": step.join(" "),
                "failure": reply,
                "completedSteps": done,
            }));
        }

        done.push(step.join(" "));
    }

    let agents = atlas_agents::refresh();
    Ok(json!({"ok": true, "completedSteps": done, "agents": agents}))
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
mod tests;
