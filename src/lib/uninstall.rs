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
/// inspected first and retired as it is, as onboarding does, whichever
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
    if !yes {
        let mut parts = vec![
            if unloaded.is_some() {
                "the menu bar and the agent plugins".to_owned()
            } else {
                "the helper's background job, the menu bar and the agent plugins".to_owned()
            },
            format!("{} (to the Trash)", text::tilde(&app)),
        ];
        if !leftovers.is_empty() {
            parts.push("apps and files Atlas copied for the helper to open".to_owned());
        }
        if !keep_config {
            parts.push(format!(
                "settings in {} and logs in {}",
                text::tilde(config_path),
                text::tilde(&data)
            ));
        }
        say("This removes from this Mac:");
        for part in parts {
            say(&format!("  · {part}"));
        }
        if let Some(name) = &helper {
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
    }
    let mut removed = Vec::new();
    let mut warnings = Vec::new();
    if let Some(error) = &unloaded {
        skipped("Helper job", "left in place: the settings do not load");
        warnings.push(format!("Helper job left in place: {error}"));
    }

    // Nothing may be controlling the helper while its job goes away.
    if let Some(client) = &client {
        let config = &client.config;
        let _exclusion = client.controller.acquire(false)?;
        if let Some(pid) = client.status().await["inputService"]["pid"]
            .as_u64()
            .and_then(|pid| u32::try_from(pid).ok())
        {
            let _ = client.stop_input(pid).await;
        }
        say("Stopping the helper's background job (macOS may ask for your password)…");
        if retire_helper_job(&executable, config)?.is_some() {
            removed.push(json!("helper job"));
            done("Helper job", "stopped");
        } else {
            skipped("Helper job", "the helper account no longer exists");
            warnings.push(format!(
                "The helper account (user ID {}) no longer exists, so no job was stopped",
                config.helper_uid
            ));
        }
        if remove_link_to(&config.identity_probe, &executable)? {
            removed.push(json!(config.identity_probe));
        }
    }
    let stopped = client
        .as_ref()
        .and_then(|client| client.output_dir().ok().map(Path::to_path_buf))
        .and_then(|output| crate::menubar::stop(&output))
        .is_some();
    let login_off = crate::menubar::set_login(crate::menubar::Login::Off)
        .is_ok_and(|reply| reply["ok"] == true);
    if login_off || stopped {
        done("Menu bar", "stopped and no longer opens at login");
    }
    // Agents forget the plugin (only connected ones change).
    let disconnected = crate::agents::change(&[], true).ok();
    if disconnected.is_some() {
        removed.push(json!("agent plugins"));
        done("Agent plugins", "removed");
    }
    let command = config::home()?.join(".local/bin/atlas");
    if remove_link_to(&command, &executable)? {
        removed.push(json!(command));
    }
    // The app goes to the Trash, where it can be recovered.
    let trash = config::home()?
        .join(".Trash")
        .join(format!("Atlas-{}.app", crate::control::token()));
    fs::rename(&app, &trash).map_err(|_| {
        problem(format!(
            "Could not move {} to the Trash; remove it yourself",
            app.display()
        ))
    })?;
    removed.push(json!(app));
    done("Atlas.app", "moved to the Trash");
    let mut cleared = false;
    for leftover in &leftovers {
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
        fs::remove_dir_all(&data)?;
        removed.push(json!(data));
        done("Logs and records", "removed");
    }
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
}
