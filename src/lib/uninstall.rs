//! `atlas uninstall`: undo what install.sh and init did on this Mac.
//! It asks first, keeps the helper account (and says how to remove it), and
//! moves the app to the Trash rather than deleting it.
use crate::{
    client::Client,
    config::{self, Config},
    error::{Error, Result},
    setup::{
        self,
        helper::{Agent, LABEL, Layout, Request, plist, text_sha256},
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
    Error::new("uninstall", message)
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

/// Stop the helper's job as the helper, outside the daemon it would stop:
/// inspected first and retired as it is, as installing it does, whichever
/// Atlas wrote its definition. None when the helper account is gone, and
/// its job with it. sudo may ask the person for their password.
fn retire_helper_job(executable: &Path, config: &Config) -> Result<Option<Value>> {
    let Some(helper) = setup::account_with_uid(config.helper_uid)? else {
        return Ok(None);
    };

    let ask = |request: &Request| -> Result<Value> {
        let (_, reply) = setup::request_as_helper(executable, &helper, request, false)?;
        if reply.is_null() {
            return Err(problem("The helper job could not be stopped"));
        }

        if reply["ok"] != true {
            return Err(problem(format!(
                "The helper job could not be stopped: {}",
                reply["error"].as_str().unwrap_or("unknown")
            )));
        }

        Ok(reply)
    };

    let inspected = ask(&Request::Inspect {
        uid: helper.uid,
        labels: vec![LABEL.to_owned()],
    })?;

    // The definition there now, else (none there) the one this Atlas
    // would write, whose backup a retired job leaves.
    let sha256 = match inspected["jobs"][LABEL]["plistSha256"].as_str() {
        Some(sha256) => sha256.to_owned(),
        None => {
            let layout = Layout {
                uid: helper.uid,
                home: helper.home.clone(),
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

    ask(&Request::Retire {
        uid: helper.uid,
        label: LABEL.to_owned(),
        sha256,
        missing_ok: true,
    })
    .map(Some)
}

/// Undo install and init. `keep_config` keeps settings and logs.
pub async fn run(config_path: &Path, keep_config: bool, yes: bool) -> Result<Value> {
    let found = survey(config_path)?;

    if !yes {
        confirm(&found, config_path, keep_config)?;
    }

    let Found {
        app,
        executable,
        client,
        unloaded,
        helper,
        data,
        leftovers,
    } = found;

    let mut removed = Vec::new();
    let mut warnings = Vec::new();

    if let Some(error) = &unloaded {
        skipped("Helper job", "left in place: the settings do not load");
        warnings.push(format!("Helper job left in place: {error}"));
    }

    if let Some(client) = &client {
        stop_helper(client, &executable, &mut removed, &mut warnings).await?;
    }

    stop_menubar(client.as_ref());
    disconnect(&executable, &mut removed, &mut warnings)?;

    // Stopped while the agents were changed: the app and settings stay.
    if crate::agents::stopping() {
        return Err(problem("Stopped before Atlas.app was removed"));
    }

    trash(&app, &leftovers, &mut removed)?;
    remove_settings(config_path, keep_config, &data, &mut removed)?;

    let mut report = json!({
        "ok": true,
        "removed": removed,
        "kept": {
            "helperAccount": helper,
            "remove": helper.as_ref().map(|name| format!("sudo sysadminctl -deleteUser {name}")),
        },
    });

    if unloaded.is_some() {
        report["kept"]["helperJob"] = json!(true);
    }

    if !warnings.is_empty() {
        report["warnings"] = json!(warnings);
    }

    Ok(report)
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
    /// The helper account's name, when it still exists.
    helper: Option<String>,
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
        Err(error) if error.code == "config-invalid" => (None, Some(error.error)),
        Err(_) => (None, None),
    };

    let config = client.as_ref().map(|client| &client.config);
    let helper = config
        .and_then(|config| setup::account_with_uid(config.helper_uid).ok().flatten())
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
        data,
        leftovers,
    })
}

/// Say what goes and what stays, and ask.
fn confirm(found: &Found, config_path: &Path, keep_config: bool) -> Result<()> {
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
        if client.is_some() {
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
        say(&format!(
            "The helper account {name} and its files stay, including any copied into it with atlas environment."
        ));
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

/// Stop input and the helper's job, and remove the shared command.
/// Nothing may be controlling the helper while its job goes away. An
/// action whose outcome is unknown must be settled first (atlas restart),
/// unless the helper cannot settle it: retiring its job then replays
/// nothing, and the report says so.
async fn stop_helper(
    client: &Client,
    executable: &Path,
    removed: &mut Vec<Value>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let config = &client.config;

    // Held from here on, so the outcome checked below stays as read.
    let _exclusion = client.controller.acquire(true)?;

    let exists = setup::account_with_uid(config.helper_uid)?.is_some();
    let status = client.status().await;

    match cannot_settle(exists, crate::session::logged_in(config.helper_uid)) {
        None => client.controller.require_known()?,
        Some(why) if client.controller.require_known().is_err() => {
            skipped("Last action", &format!("outcome unknown; {why}"));
            warnings.push(format!(
                "The last action's outcome is unknown and {why}, so it was not checked"
            ));
        }
        Some(_) => {}
    }

    if let Some(pid) = status["inputService"]["pid"]
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok())
    {
        let _ = client.stop_input(pid).await;
    }

    say("Stopping the helper's background job (macOS may ask for your password)…");
    if retire_helper_job(executable, config)?.is_some() {
        removed.push(json!("helper job"));
        done("Helper job", "stopped");
    } else {
        skipped("Helper job", "the helper account no longer exists");
        warnings.push(format!(
            "The helper account (user ID {}) no longer exists, so no job was stopped",
            config.helper_uid
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
    if let Ok(reply) = crate::agents::change(&[], true) {
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

/// The name a person knows an agent by, from its name in a reply.
fn shown(name: &str) -> &str {
    crate::agents::Agent::named(name).map_or(name, |agent| agent.display_name())
}

/// The app goes to the Trash, where it can be recovered; what it left for
/// the helper goes with it.
fn trash(app: &Path, leftovers: &[PathBuf], removed: &mut Vec<Value>) -> Result<()> {
    let trash = config::home()?
        .join(".Trash")
        .join(format!("Atlas-{}.app", crate::control::token()));

    fs::rename(app, &trash).map_err(|_| {
        problem(format!(
            "Could not move {} to the Trash; remove it yourself",
            app.display()
        ))
    })?;

    removed.push(json!(app));
    done("Atlas.app", "moved to the Trash");

    // Earlier apps an unfinished update kept beside it.
    for staged in crate::update::staged_copies(app) {
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

    Ok(())
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
