//! App replacement and handover to the newly installed executable.
use atlas::{
    client::Client,
    config::Config,
    error::{Code, Result},
    host_session::HostSession,
    ui::progress::Spinner,
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

/// Update the app, then hand over to the new executable, so the steps that
/// move the helper onto it are always the new version's own.
pub(super) async fn update(
    config: &std::path::Path,
    allow_unnotarized: bool,
    force: bool,
    version: Option<String>,
    host: Option<&HostSession>,
    allow_disrupt: bool,
) -> Result<Value> {
    update_using(
        config,
        allow_unnotarized,
        force,
        version,
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
    let source = atlas::update::Source::from_env(version);

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
    let exclusive = atlas::update::exclusive()?;

    let spinner = Spinner::new("Checking for an update…");
    let mut report = atlas::update::install(
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
    let executable = atlas::update::installed_executable()?;

    // update-finish always answers in JSON, including releases that
    // predate --json. An older selected release may also predate the
    // activity guard and its override option.
    let mut finish = std::process::Command::new(&executable);
    finish.arg("update-finish");
    if allow_disrupt && supports_disruption_option(&executable) {
        finish.arg("--allow-disrupt");
    }
    let output = finish.arg("--config").arg(config).output()?;
    let finished: Value = serde_json::from_slice(&output.stdout)
        .or_else(|_| serde_json::from_slice(&output.stderr))
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
        let port = output.and_then(|output| atlas::menubar::stop(&output));

        report["previous"] = atlas::update::finished()?;

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
        && atlas::setup::migrate::in_effect(loaded.helper_uid)
    {
        atlas::setup::migrate::preflight_apply(config, program).await?;
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
    if !has_helper_job(atlas::setup::migrate::recorded(loaded.helper_uid), daemon) {
        return Ok(json!({"ok": true, "completedSteps": [], "next": "atlas init"}));
    }

    let executable = std::env::current_exe()?;

    // The daemon restarts first: a daemon whose app was swapped under it can
    // no longer vouch (to TCC) for a new input service it starts, while the
    // input service it already runs is cleaned up by the restart itself.
    let mut steps: Vec<&[&str]> = vec![&["install"], &["restart"], &["start-input"]];

    if atlas::setup::migrate::in_effect(loaded.helper_uid) {
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
        // Replies go to stdout; errors to stderr.
        let reply: Value = serde_json::from_slice(&output.stdout)
            .or_else(|_| serde_json::from_slice(&output.stderr))
            .unwrap_or(Value::Null);
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

    let agents = atlas::agents::refresh();
    Ok(json!({"ok": true, "completedSteps": done, "agents": agents}))
}

/// Whether the helper has a job to move onto a new version: a setup is
/// recorded, or its daemon answers (setup stopped after installing it).
fn has_helper_job(recorded: bool, daemon_answers: bool) -> bool {
    recorded || daemon_answers
}

fn supports_disruption_option(executable: &std::path::Path) -> bool {
    std::process::Command::new(executable)
        .arg("--help")
        .output()
        .is_ok_and(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .split_whitespace()
                    .any(|word| word == "--allow-disrupt")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_client(config: Config) -> Result<Client> {
        let controller = atlas::control::Controller::at(
            config.helper_uid,
            &config.output_dir.parent().unwrap().join("state"),
        );
        let mut client = Client::new(config)?;
        client.controller = controller;
        Ok(client)
    }

    #[tokio::test]
    async fn a_waiter_arriving_during_download_refuses_the_swap_under_ownership() {
        use atlas::host_session::{Host, Instance};

        let directory = tempfile::tempdir().unwrap();
        let settings = directory.path().join("config.toml");
        std::fs::write(
            &settings,
            "helper_uid = 60001\nsocket_path = \"helper/missing.sock\"\n\
             identity_probe = \"atlas\"\ninput_socket = \"input.sock\"\noutput_dir = \"runs\"\n",
        )
        .unwrap();
        let mut client = temporary_client(Config::load(&settings).unwrap()).unwrap();
        client.host_session = Some(HostSession::new(Host::Named, "updater").unwrap());
        client.controller = client.controller.labelled(client.host_session.as_ref());
        let ownership = client.service_change(false).await.unwrap();
        preflight(
            &settings,
            &directory.path().join("new-atlas"),
            Some(&client),
        )
        .await
        .unwrap();

        let holder = std::fs::read_dir(directory.path().join("state"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.to_string_lossy().ends_with(".holder.json"))
            .unwrap();
        let waiting = holder.with_file_name(
            holder
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .replace(".holder.json", ".waiting"),
        );
        std::fs::create_dir(&waiting).unwrap();
        let worker = HostSession::new(Host::Named, "worker").unwrap();
        let note = atlas::control::Note {
            session: Some(worker.key().clone()),
            host: Some(Host::Named),
            folder: None,
            named: Some(worker.id().clone()),
            process: Instance::current().unwrap(),
            since: u64::try_from(atlas::client::timestamp() / 1_000_000).unwrap(),
        };
        std::fs::write(
            waiting.join("0000000000000001.json"),
            serde_json::to_vec(&note).unwrap(),
        )
        .unwrap();

        let error = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            preflight(
                &settings,
                &directory.path().join("new-atlas"),
                Some(&client),
            ),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert_eq!(error.code, Code::Busy);
        assert_eq!(error.error, "worker is active in the helper.");
        assert_eq!(
            client.controller.acquire(false).unwrap_err().code,
            Code::Busy
        );
        assert!(
            client
                .controller
                .holder_note()
                .unwrap()
                .process
                .is_this_one()
        );
        drop(ownership);
    }

    #[tokio::test]
    async fn updates_and_handover_refuse_active_work_before_replacing_anything() {
        use atlas::host_session::Host;
        use std::{fs, os::unix::fs::PermissionsExt, time::Duration};

        let directory = tempfile::tempdir().unwrap();
        let settings = directory.path().join("config.toml");
        fs::write(
            &settings,
            "helper_uid = 60001\nsocket_path = \"missing.sock\"\n\
            identity_probe = \"atlas\"\ninput_socket = \"input.sock\"\noutput_dir = \"runs\"\n",
        )
        .unwrap();
        let output = directory.path().join("runs");
        fs::create_dir(&output).unwrap();
        fs::set_permissions(&output, fs::Permissions::from_mode(0o700)).unwrap();
        let sessions = output.join("sessions");
        fs::create_dir(&sessions).unwrap();
        let host = HostSession::new(Host::Named, "worker").unwrap();
        let now = atlas::client::timestamp() / 1_000_000;
        let record = sessions.join(format!("{}.json", host.key()));
        let bytes = serde_json::to_vec(&json!({"version": 1, "key": host.key(),
            "host": "named", "hostSession": "worker", "started": now, "lastSeen": now}))
        .unwrap();
        fs::write(&record, &bytes).unwrap();

        for force in [false, true] {
            let error = tokio::time::timeout(
                Duration::from_secs(1),
                update_using(&settings, false, force, None, None, false, temporary_client),
            )
            .await
            .unwrap()
            .unwrap_err();
            assert_eq!(error.code, Code::Busy);
            assert_eq!(error.error, "worker is active in the helper.");
        }
        let error = tokio::time::timeout(
            Duration::from_secs(1),
            update_finish_using(&settings, None, false, temporary_client),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert_eq!(error.code, Code::Busy);
        assert_eq!(error.error, "worker is active in the helper.");
        assert_eq!(fs::read(&record).unwrap(), bytes);
    }

    #[test]
    fn setup_detection_propagates_broken_configuration_paths() {
        use std::{fs, os::unix::fs::symlink};

        let dir = tempfile::tempdir().unwrap();
        for missing in ["missing.toml", "missing/settings.toml"] {
            assert!(set_up(&dir.path().join(missing)).unwrap().is_none());
        }
        let valid = dir.path().join("valid.toml");
        fs::write(&valid, include_str!("../../examples/config.toml")).unwrap();
        assert_eq!(
            set_up(&valid).unwrap().unwrap(),
            Config::load(&valid).unwrap()
        );

        let malformed = dir.path().join("malformed.toml");
        fs::write(&malformed, "helper_uid = [").unwrap();
        let dangling = dir.path().join("dangling.toml");
        symlink(dir.path().join("missing.toml"), &dangling).unwrap();
        let looping = dir.path().join("loop");
        symlink(&looping, &looping).unwrap();
        assert_eq!(fs::read_link(&looping).unwrap(), looping);
        for broken in [malformed, dangling, looping.join("settings.toml")] {
            assert_eq!(set_up(&broken).unwrap_err().code, "config-invalid");
        }
    }

    /// Only missing settings mean "not set up"; a broken file is an error.
    #[tokio::test]
    async fn finishing_an_update_needs_settings_that_load() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("config.toml");
        let finished = update_finish(&missing, None, false).await.unwrap();
        assert_eq!(finished, json!({"ok": true, "next": "atlas init"}));

        let broken = dir.path().join("broken.toml");
        std::fs::write(&broken, "helper_uid = [").unwrap();
        let error = update_finish(&broken, None, false).await.unwrap_err();
        assert_eq!(error.code, "config-invalid", "{}", error.error);
    }

    /// Settings written before setup installed the helper's job leave no
    /// daemon to move: the update finishes, and atlas init does the rest.
    #[tokio::test]
    async fn finishing_an_update_skips_a_helper_with_no_job() {
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join("config.toml");
        std::fs::write(
            &settings,
            "helper_uid = 60001\nsocket_path = \"helper/missing.sock\"\n\
             identity_probe = \"atlas\"\ninput_socket = \"input.sock\"\n\
             output_dir = \"runs\"\n",
        )
        .unwrap();

        let finished = update_finish_using(&settings, None, false, temporary_client)
            .await
            .unwrap();
        assert_eq!(
            finished,
            json!({"ok": true, "completedSteps": [], "next": "atlas init"})
        );

        assert!(has_helper_job(true, false), "a recorded setup");
        assert!(has_helper_job(false, true), "a daemon that answers");
        assert!(!has_helper_job(false, false));
    }
}
