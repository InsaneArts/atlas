//! `understudy uninstall`: undo what install.sh and init did on this Mac.
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
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, IsTerminal, Write},
    path::Path,
    process::Command,
};

fn problem(message: impl Into<String>) -> Error {
    Error::new("uninstall", message)
}

fn confirm(question: &str) -> Result<bool> {
    eprint!("{question} [y/N] ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
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

/// Stop the helper's job as the helper, outside the daemon it would stop.
fn retire_helper_job(executable: &Path, config: &Config) -> Result<Value> {
    let helper = setup::account_with_uid(config.helper_uid)?
        .ok_or_else(|| problem("The helper account does not exist"))?;
    let controller_uid = nix::unistd::getuid().as_raw();
    let layout = Layout {
        uid: helper.uid,
        home: helper.home.clone(),
    };
    let agent = Agent {
        label: LABEL.to_owned(),
        program: executable.to_owned(),
        uid: helper.uid,
        controller_uid,
        socket: layout.socket(),
    };
    let request = Request::Retire {
        uid: helper.uid,
        label: LABEL.to_owned(),
        sha256: text_sha256(&plist(&agent, &layout.logs())),
        missing_ok: true,
    };
    let output = Command::new("/usr/bin/sudo")
        .args([
            "/bin/launchctl",
            "asuser",
            &helper.uid.to_string(),
            "/usr/bin/sudo",
            "-u",
            &helper.name,
        ])
        .arg(executable)
        .arg("setup-helper")
        .arg(serde_json::to_string(&request)?)
        .output()?;
    serde_json::from_slice(&output.stdout)
        .or_else(|_| serde_json::from_slice(&output.stderr))
        .map_err(|_| problem("The helper job could not be stopped"))
}

/// Undo install and init. `keep_config` keeps settings and logs.
pub async fn run(config_path: &Path, keep_config: bool, yes: bool) -> Result<Value> {
    let interactive = std::io::stdin().is_terminal();
    if !yes && !interactive {
        return Err(problem(
            "uninstall asks first: run it in a terminal, or pass --yes",
        ));
    }
    if !yes && !confirm("Remove Understudy from this Mac? The helper account and its files stay.")?
    {
        return Ok(json!({"ok": false, "cancelled": true}));
    }
    let app = crate::update::installed_app()?;
    let executable = app.join("Contents/MacOS/understudy");
    let mut removed = Vec::new();

    // Nothing may be controlling the helper while its job goes away.
    let config = Config::load(config_path).ok();
    if let Some(config) = &config {
        let client = Client::new(config.clone())?;
        let _exclusion = client.controller.acquire(false)?;
        if let Some(pid) = client.status().await["inputService"]["pid"].as_u64() {
            let _ = client.stop_input(pid as u32).await;
        }
        let retired = retire_helper_job(&executable, config)?;
        if retired["ok"] != true {
            return Err(problem(format!(
                "The helper job could not be stopped: {}",
                retired["error"].as_str().unwrap_or("unknown")
            )));
        }
        removed.push(json!("helper job"));
        if remove_link_to(&config.identity_probe, &executable)? {
            removed.push(json!(config.identity_probe));
        }
    }
    let _ = crate::menubar::login("off");
    // Agents forget the plugin (only connected ones change).
    let disconnected = crate::agents::change(&[], true).ok();
    if disconnected.is_some() {
        removed.push(json!("agent plugins"));
    }
    let command = config::home()?.join(".local/bin/understudy");
    if remove_link_to(&command, &executable)? {
        removed.push(json!(command));
    }
    // The app goes to the Trash, where it can be recovered.
    let trash = config::home()?
        .join(".Trash")
        .join(format!("Understudy-{}.app", crate::control::token()));
    fs::rename(&app, &trash).map_err(|_| {
        problem(format!(
            "Could not move {} to the Trash; remove it yourself",
            app.display()
        ))
    })?;
    removed.push(json!(app));
    if !keep_config {
        let data = config::home()?.join("Library/Application Support/Understudy");
        if data.exists() {
            fs::remove_dir_all(&data)?;
            removed.push(json!(data));
        }
    }
    let helper = config
        .as_ref()
        .and_then(|config| setup::account_with_uid(config.helper_uid).ok().flatten())
        .map(|account| account.name);
    Ok(json!({
        "ok": true,
        "removed": removed,
        "kept": {
            "helperAccount": helper,
            "remove": helper.as_ref().map(|name| format!("sudo sysadminctl -deleteUser {name}")),
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_links_to_the_app_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let executable = root.join("Understudy.app/Contents/MacOS/understudy");
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
}
