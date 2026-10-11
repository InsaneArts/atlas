//! `atlas init`: a guided first setup, safe to run again. Each step
//! checks before it acts and is skipped when already done. It never types or
//! stores a password and never touches privacy settings: the person signs in
//! and clicks Allow themselves; Atlas waits and checks.
use crate::{
    client::Client,
    config::{self, Config},
    error::{Error, Result},
    setup::{
        self,
        helper::{Agent, LABEL, Layout, Request},
    },
    ui::{
        self,
        progress::{Spinner, done, heading, say, skipped},
        prompt, style, text,
    },
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

/// How long init waits for the person to sign in or click Allow.
const PATIENCE: Duration = Duration::from_secs(30 * 60);

fn problem(message: impl Into<String>) -> Error {
    Error::new("init", message)
}

/// A command that must succeed and talks to the person itself (sudo and
/// sysadminctl ask for passwords in their own words).
fn attended(program: &str, args: &[&str], why: &str) -> Result<()> {
    let status = Command::new(program).args(args).status()?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| problem(why.to_owned()))
}

/// Like `attended`, but timestamped log lines on standard error (as
/// sysadminctl writes) are dropped; anything else, such as a password
/// question, is shown as it arrives. On failure the dropped lines are shown.
fn quietly_attended(program: &str, args: &[&str], why: &str) -> Result<()> {
    use std::io::{Read, Write};
    let mut child = Command::new(program)
        .args(args)
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    let mut stream = child
        .stderr
        .take()
        .ok_or_else(|| problem("The command has no error output"))?;
    let (sender, receiver) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        while let Ok(read @ 1..) = stream.read(&mut chunk) {
            if sender.send(chunk[..read].to_vec()).is_err() {
                break;
            }
        }
    });
    let log_line = |line: &[u8]| {
        let text = String::from_utf8_lossy(line);
        let bytes = text.as_bytes();
        // "2026-10-11 14:28:19.767 sysadminctl[…]": a date, then a time.
        bytes.len() > 20
            && bytes[..4].iter().all(u8::is_ascii_digit)
            && bytes[4] == b'-'
            && bytes[10] == b' '
            && bytes[13] == b':'
    };
    let mut dropped = Vec::new();
    let mut pending = Vec::new();
    let mut terminal = std::io::stderr();
    loop {
        match receiver.recv_timeout(Duration::from_millis(250)) {
            Ok(bytes) => {
                pending.extend(bytes);
                while let Some(end) = pending.iter().position(|&byte| byte == b'\n') {
                    let line: Vec<u8> = pending.drain(..=end).collect();
                    if log_line(&line) {
                        dropped.extend(line);
                    } else {
                        let _ = terminal.write_all(&line);
                    }
                }
            }
            // A question waits for an answer without ending its line.
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if !pending.is_empty() && !log_line(&pending) {
                    let _ = terminal.write_all(&pending);
                    let _ = terminal.flush();
                    pending.clear();
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let status = child.wait()?;
    if status.success() {
        return Ok(());
    }
    let _ = terminal.write_all(&dropped);
    let _ = terminal.write_all(&pending);
    Err(problem(why.to_owned()))
}

/// A command with cached administrator rights (`sudo -n`), never prompting.
fn sudo(args: &[&str]) -> Result<std::process::Output> {
    Ok(Command::new("/usr/bin/sudo")
        .arg("-n")
        .args(args)
        .output()?)
}

/// The installed app's executable; init runs from it.
fn app_executable() -> Result<PathBuf> {
    let app = crate::update::installed_app()?;
    let executable = app.join("Contents/MacOS/atlas");
    if !crate::update::reachable_by_others(&executable) {
        return Err(problem(
            "Atlas.app must be in /Applications so the helper account can run it",
        ));
    }
    Ok(executable)
}

/// `~/.local/bin/atlas` → the app, so `atlas` works in a terminal.
fn link_command(executable: &Path) -> Result<Value> {
    let bin = config::home()?.join(".local/bin");
    fs::create_dir_all(&bin)?;
    let link = bin.join("atlas");
    if fs::canonicalize(&link).ok().as_deref() == Some(executable) {
        return Ok(json!({"linked": link, "changed": false}));
    }
    let temporary = bin.join(format!(".atlas.{}", crate::control::token()));
    std::os::unix::fs::symlink(executable, &temporary)?;
    fs::rename(&temporary, &link)?;
    Ok(json!({"linked": link, "changed": true}))
}

/// Whether `name` can be a new account's short name.
fn valid_account_name(name: &str) -> std::result::Result<(), String> {
    if name.is_empty() || name.len() > 31 {
        return Err("Use 1 to 31 characters.".into());
    }
    if !name
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        || name.starts_with(['.', '-'])
    {
        return Err("Use letters, digits, '.', '_' and '-'.".into());
    }
    Ok(())
}

/// Choose an existing standard account, or (only when asked for, and
/// confirmed) create one.
fn choose_helper() -> Result<setup::Account> {
    let candidates: Vec<_> = crate::switch::accounts(None)?
        .into_iter()
        .filter(|account| setup::administrator(&account.name) == Some(false))
        .collect();
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
        return setup::account_named(&account.name)?
            .ok_or_else(|| problem("That account disappeared"));
    }
    let name = prompt::input(
        "Short name for the new account",
        "atlas",
        |name| {
            valid_account_name(name)?;
            match setup::account_named(name) {
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
    // sysadminctl logs every step to the terminal; only its questions show.
    // A user ID nothing runs as: a deleted account's session can outlive
    // it, and sharing its ID would mix that session with the helper's.
    let uid = free_uid()?.to_string();
    quietly_attended(
        "/usr/bin/sudo",
        &[
            "/usr/sbin/sysadminctl",
            "-addUser",
            &name,
            "-fullName",
            "Atlas Helper",
            "-UID",
            &uid,
            "-password",
            "-",
        ],
        "The helper account could not be created",
    )?;
    setup::account_named(&name)?.ok_or_else(|| problem("The new account was not found"))
}

/// The lowest user ID from 501 that no account has and no process runs as.
fn free_uid() -> Result<u32> {
    let taken = |program: &str, args: &[&str]| -> Result<Vec<u32>> {
        let output = Command::new(program).args(args).output()?;
        if !output.status.success() {
            return Err(problem("Could not list the user IDs in use"));
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .filter_map(|word| word.parse().ok())
            .collect())
    };
    let accounts = taken("/usr/bin/dscl", &[".", "-list", "/Users", "UniqueID"])?;
    let running = taken("/bin/ps", &["-axo", "uid="])?;
    (501..600)
        .find(|uid| !accounts.contains(uid) && !running.contains(uid))
        .ok_or_else(|| problem("No free user ID for the helper account"))
}

/// The configuration for `helper` in the managed private layout.
fn proposed(helper: &setup::Account) -> Result<Config> {
    let layout = Layout {
        uid: helper.uid,
        home: helper.home.clone(),
    };
    Ok(Config {
        helper_uid: helper.uid,
        socket_path: layout.socket(),
        identity_probe: "/Users/Shared/Atlas/atlas".into(),
        input_socket: layout.root().join(setup::helper::INPUT).join("input.sock"),
        output_dir: config::home()?.join("Library/Application Support/Atlas/runs"),
    })
}

/// Write a new configuration atomically, readable only by this account.
fn write_config(path: &Path, config: &Config) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let parent = path
        .parent()
        .ok_or_else(|| problem("The configuration needs a directory"))?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    let temporary = parent.join(format!(".atlas.toml.{}", crate::control::token()));
    let written = (|| -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(config.to_toml()?.as_bytes())?;
        file.sync_all()?;
        // Never replaces a configuration that appeared meanwhile.
        fs::hard_link(&temporary, path)?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    written
}

/// Replace the configuration that named `previous` (its helper account is
/// gone or changed), atomically, and only while the file still says so.
fn replace_config(path: &Path, previous: &Config, config: &Config) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    // Replacing a link would leave the file it points to unchanged.
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(problem(format!(
            "{} is a link, and its helper account changed. Update the file it points to (or \
             remove the link), then run atlas init again.",
            text::tilde(path)
        )));
    }
    let parent = path
        .parent()
        .ok_or_else(|| problem("The configuration needs a directory"))?;
    let temporary = parent.join(format!(".atlas.toml.{}", crate::control::token()));
    let written = (|| -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(config.to_toml()?.as_bytes())?;
        file.sync_all()?;
        if Config::load(path).ok().as_ref() != Some(previous) {
            return Err(problem(format!(
                "{} changed while Atlas was updating it; run atlas init again",
                text::tilde(path)
            )));
        }
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    written
}

/// Whether the helper's graphical session exists (it has logged in).
fn helper_logged_in(uid: u32) -> bool {
    // A signed-in desktop runs its Dock; visible without administrator rights.
    Command::new("/usr/bin/pgrep")
        .args(["-U", &uid.to_string(), "-x", "Dock"])
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Keep the administrator rights from `sudo -v` fresh while init waits
/// for the person (sudo forgets them after five minutes otherwise).
fn keep_sudo_alive() {
    std::thread::spawn(|| {
        loop {
            std::thread::sleep(Duration::from_secs(60));
            let _ = Command::new("/usr/bin/sudo").args(["-n", "-v"]).output();
        }
    });
}

/// Run one setup-helper request as the helper, inside its own login, with
/// the administrator rights init holds.
fn as_helper(executable: &Path, helper: &setup::Account, request: &Request) -> Result<Value> {
    let request = serde_json::to_string(request)?;
    let program = executable.to_string_lossy();
    let output = sudo(&[
        "/bin/launchctl",
        "asuser",
        &helper.uid.to_string(),
        "/usr/bin/sudo",
        "-u",
        &helper.name,
        &program,
        "setup-helper",
        &request,
    ])?;
    let reply: Value = serde_json::from_slice(&output.stdout)
        .or_else(|_| serde_json::from_slice(&output.stderr))
        .unwrap_or(Value::Null);
    if !output.status.success() || reply["ok"] != true {
        return Err(problem(format!(
            "Setting up the helper failed: {}",
            reply["error"].as_str().unwrap_or("no reply")
        )));
    }
    Ok(reply)
}

/// Restart the helper daemon (launchd starts a fresh process) and wait for
/// the new one to answer. No other controller may act meanwhile, and the
/// restart stays pending until a new daemon is seen; if that never happens
/// it is left for recovery, like any action of unknown outcome.
async fn restart_daemon(client: &Client, config: &Config) -> Result<()> {
    let _exclusion = client.controller.acquire(true)?;
    let before = client.status().await["daemon"]["pid"].clone();
    let transport = crate::transport::Transport {
        path: config.socket_path.clone(),
        timeout: Duration::from_secs(10),
        peer_uid: config.helper_uid,
    };
    let pending = client.controller.quarantine()?;
    transport.call(&json!({"op": "restart"})).await?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let pid = client.status().await["daemon"]["pid"].clone();
        if pid.is_number() && pid != before {
            client.controller.clear_if(&pending)?;
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(problem(
                "The helper did not start again after a restart. Run atlas doctor.",
            ));
        }
    }
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

/// The finishing touches, offered only when not already in place: the menu
/// bar at login, and teaching the installed agents.
/// The closing steps, which never fail a setup that worked. The menu bar is
/// turned on at login only by the first setup (a later run leaves the
/// person's choice alone), and only for the default settings, the ones the
/// login item uses.
fn finish(done_steps: &mut Vec<Value>, settings: &Path) -> Result<()> {
    let default = config::installed_path().ok().as_deref() == Some(settings);
    // Until it is decided once (here, or by the person), never again after.
    let first = !crate::menubar::login_decided();
    if first && !default {
        skipped(
            "Menu bar",
            &format!(
                "Open at Login uses the default settings; start it with atlas menubar --config {}",
                text::tilde(settings)
            ),
        );
    } else if first {
        let status = crate::menubar::login("status").unwrap_or_default();
        if status["enabled"] == true {
            crate::menubar::decide_login();
        } else if status["available"] == true {
            match crate::menubar::login("on") {
                Ok(login) if login["enabled"] == true => {
                    crate::menubar::decide_login();
                    done(
                        "Menu bar",
                        "opens at login (atlas menubar --login off turns it off)",
                    );
                    done_steps.push(json!({"step": "menubar", "login": login}));
                }
                _ => skipped("Menu bar", "turn on Open at Login from its menu later"),
            }
        }
    }
    let agents = crate::agents::status();
    let found: Vec<crate::agents::Agent> = agents["agents"]
        .as_array()
        .into_iter()
        .flatten()
        // Only agents that answered and are not connected yet.
        .filter(|agent| agent["installed"] == true && agent["connected"] == false)
        .filter_map(|agent| {
            agent["agent"]
                .as_str()
                .and_then(crate::agents::Agent::named)
        })
        .collect();
    let names: Vec<&str> = found.iter().map(|agent| agent.display_name()).collect();
    if !found.is_empty()
        && prompt::confirm(
            &format!(
                "Teach {} to use Atlas? (adds a plugin)",
                names.join(" and ")
            ),
            true,
            "run atlas agents install later",
        )?
    {
        // Setup is already done; this step can only be skipped, never fail it.
        let connected = {
            let _spinner = Spinner::new("Connecting…");
            crate::agents::change(&found, false)
                .unwrap_or_else(|error| json!({"ok": false, "error": error.error}))
        };
        if connected["ok"] == true {
            done("Agents", &format!("{} connected", names.join(" and ")));
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
/// `name`; otherwise say how.
fn offer_lock(name: &str) -> Result<()> {
    if prompt::confirm(
        &format!("Lock the screen now so you can switch to {name}?"),
        true,
        "switch accounts yourself",
    )? {
        match crate::switch::lock_screen() {
            Ok(_) => done(
                "Screen locked",
                &format!("click Switch User and choose {name}"),
            ),
            Err(error) => skipped("Screen not locked", &error.error),
        }
    } else {
        say(&format!(
            "  To switch: Apple menu › Lock Screen, then Switch User and choose {name}."
        ));
    }
    Ok(())
}

/// The permissions the helper still lacks, by their System Settings names.
fn missing(status: &Value) -> Vec<&'static str> {
    [
        ("screenCapture", "Screen & System Audio Recording"),
        ("accessibility", "Accessibility"),
    ]
    .into_iter()
    .filter(|(key, _)| status["permissions"][*key] != true)
    .map(|(_, pane)| pane)
    .collect()
}

/// Guide the person from an installed app to a ready helper.
pub async fn run(config_path: &Path) -> Result<Value> {
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

    let executable = app_executable()?;
    // Checked before anything is set up: macOS keeps the helper's permissions
    // only for Atlas signed by its Developer ID.
    if !setup::developer_id(&executable) {
        return Err(problem(
            "This Atlas.app is not signed by Atlas's developer. Install Atlas with install.sh \
             (or atlas update), then run atlas init again.",
        ));
    }
    let linked = link_command(&executable)?;
    done("Atlas.app", &text::tilde(crate::update::installed_app()?));
    done_steps.push(json!({"step": "app", "command": linked}));

    // The path is the one the command line resolved (the installed one by
    // default); a configuration is created only where none exists, and an
    // existing one that does not load is reported, never replaced.
    let config_path = config_path.to_owned();
    let absent = matches!(
        fs::symlink_metadata(&config_path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    );
    let mut config = if absent {
        let helper = choose_helper()?;
        let config = proposed(&helper)?;
        write_config(&config_path, &config)?;
        config
    } else {
        Config::load(&config_path)?
    };
    // The account the settings name was deleted: choose (or create) another.
    if setup::account_with_uid(config.helper_uid)?.is_none() {
        say(&format!(
            "  The helper account in {} (user ID {}) no longer exists.",
            text::tilde(&config_path),
            config.helper_uid
        ));
        let helper = choose_helper()?;
        let replacement = proposed(&helper)?;
        replace_config(&config_path, &config, &replacement)?;
        config = replacement;
    }
    let helper = setup::account_with_uid(config.helper_uid)?
        .ok_or_else(|| problem("The helper account was not created"))?;
    // Another account now has the user ID (with its own home): every path
    // in the settings follows the account that is there now.
    if !config.socket_path.starts_with(&helper.home) {
        let fresh = proposed(&helper)?;
        let current = Config {
            socket_path: fresh.socket_path,
            input_socket: fresh.input_socket,
            ..config.clone()
        };
        replace_config(&config_path, &config, &current)?;
        config = current;
    }
    let name = helper.name.clone();
    match setup::administrator(&name) {
        Some(false) => {}
        Some(true) => {
            return Err(problem(format!(
                "{name} administers this Mac, and the helper must be a standard account. Make it \
                 a standard account in System Settings › Users & Groups, then run atlas init again."
            )));
        }
        None => {
            return Err(problem(format!(
                "macOS could not say whether {name} is an administrator; run atlas init again"
            )));
        }
    }
    // An older account with the same user ID was deleted while signed in:
    // its session still runs and would be mistaken for the helper's.
    if crate::session::leftover_session(helper.uid, &helper.name, &helper.home) {
        return Err(problem(format!(
            "macOS still runs a session of a deleted account with {name}'s user ID, and \
             mixes it up with {name}. Restart the Mac, then run atlas init again."
        )));
    }
    done(
        "Helper account",
        &format!("{name} · settings in {}", text::tilde(&config_path)),
    );
    if setup::migrate::archive_foreign(helper.uid, &helper.home)? {
        say("  Set aside a setup record left by an earlier account with the same ID.");
    }
    // Whether an earlier run already finished setup (decides the first-run steps).
    let settled_before = setup::migrate::settled(helper.uid);
    let client = Client::new(config.clone())?;
    // An earlier action of unknown outcome is recovered before anything else
    // when it can be; recovery needs Accessibility, so without it recovery
    // waits until the person has allowed it below.
    let recover = async || -> Result<()> {
        if !client.controller.pending()?.is_empty() {
            let spinner = Spinner::new("Recovering an unfinished action…");
            client.restart().await?;
            drop(spinner);
            done("Recovered an unfinished action", "");
        }
        Ok(())
    };
    // Only a helper that answers in the background with Accessibility can
    // be recovered now; otherwise the steps below start it first.
    let status = client.status().await;
    if crate::session::require_hidden(&status, config.helper_uid).is_ok()
        && status["permissions"]["accessibility"] != false
    {
        recover().await?;
    }
    // Ready now, set up before, and running this very build throughout.
    let status = client.status().await;
    if crate::capability::doctor(status.clone(), config.helper_uid)["ready"] == true
        && settled_before
        && crate::session::input_build_current(&status)
    {
        done("Already set up", "everything checks out");
        done_steps.push(json!({"step": "verified"}));
        finish(&mut done_steps, &config_path)?;
        ready();
        return Ok(json!({"ok": true, "ready": true, "alreadySetUp": true, "steps": done_steps}));
    }
    done_steps.push(json!({"step": "config", "path": config_path, "helper": name}));

    let daemon_answers = || async {
        let status = client.status().await;
        crate::session::require_hidden(&status, config.helper_uid).is_ok()
            || status["daemon"]["pid"].is_number()
    };
    if !daemon_answers().await {
        // Set up before: the helper's job is installed and starts by itself
        // once the helper signs in (after a restart, say). No password then.
        let installed = setup::migrate::settled(helper.uid);
        let mut sudo_ready = false;
        let admin = |sudo_ready: &mut bool| -> Result<()> {
            if *sudo_ready {
                return Ok(());
            }
            heading("Administrator rights");
            say("  macOS asks for your password once, to install the helper's background job.");
            attended(
                "/usr/bin/sudo",
                &["-v"],
                "Administrator rights were not granted",
            )?;
            keep_sudo_alive();
            done("Administrator rights", "");
            *sudo_ready = true;
            Ok(())
        };
        if !installed {
            admin(&mut sudo_ready)?;
        }

        if !helper_logged_in(helper.uid) {
            heading(&format!("Sign in to {name} once"));
            if installed {
                say(&format!(
                    "  {name} is signed out (after a restart, say). Switch to {name}, sign in,\n  \
                     then switch back here."
                ));
            } else {
                say(&format!(
                    "  Switch to {name}, sign in and finish macOS's setup questions. If macOS then\n  \
                     asks about Accessibility or Screen Recording for Atlas, turn Atlas on.\n  \
                     Then switch back here."
                ));
            }
            offer_lock(&name)?;
            {
                let _spinner = Spinner::new(format!(
                    "Waiting for {name} to sign in… (Ctrl-C to stop; run atlas init again later)"
                ));
                wait_for("the helper to sign in", async || {
                    helper_logged_in(helper.uid).then_some(())
                })
                .await?;
            }
            done(&format!("{name} signed in"), "");
        }

        let mut started = false;
        if installed {
            let _spinner = Spinner::new("Waiting for the helper to start…");
            for _ in 0..30 {
                if daemon_answers().await {
                    started = true;
                    break;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
        if !started {
            admin(&mut sudo_ready)?;
            // The rights may have lapsed during a long wait: ask again then.
            if !sudo(&["-v"]).is_ok_and(|output| output.status.success()) {
                say("  Your administrator rights expired while Atlas waited.");
                attended(
                    "/usr/bin/sudo",
                    &["-v"],
                    "Administrator rights were not granted",
                )?;
            }
            let spinner = Spinner::new("Installing the helper's background job…");
            let controller = setup::account_with_uid(nix::unistd::getuid().as_raw())?
                .ok_or_else(|| problem("This account cannot be resolved"))?;
            let layout = Layout {
                uid: helper.uid,
                home: helper.home.clone(),
            };
            as_helper(
                &executable,
                &helper,
                &Request::Prepare {
                    uid: helper.uid,
                    controller: controller.name.clone(),
                },
            )?;
            as_helper(
                &executable,
                &helper,
                &Request::Install {
                    agent: Agent {
                        label: LABEL.to_owned(),
                        program: executable.clone(),
                        uid: helper.uid,
                        controller_uid: controller.uid,
                        socket: layout.socket(),
                    },
                },
            )?;
            spinner.set("Starting the helper…");
            let mut answered = false;
            for _ in 0..30 {
                if daemon_answers().await {
                    answered = true;
                    break;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            drop(spinner);
            if !answered {
                return Err(problem(format!(
                    "The helper's background job was installed but did not start. Its log is \
                     {} in {name}'s account; run atlas init again after checking it.",
                    layout.logs().join(format!("{LABEL}.log")).display()
                )));
            }
            done("Helper job", "installed and running");
            done_steps.push(json!({"step": "daemon", "label": LABEL}));
        }
    }

    // The session checks below run this executable in the helper.
    client.install()?;

    let status = client.status().await;
    if missing(&status).is_empty() {
        // macOS keeps these approvals for Atlas across the Mac, so a helper
        // can have them before it is ever asked.
        done(
            "Permissions",
            "Screen Recording · Accessibility already allowed",
        );
        if crate::session::require_hidden(&status, config.helper_uid).is_err() {
            let _spinner = Spinner::new("Waiting for you to switch back to your own account…");
            wait_for("you to switch back", async || {
                crate::session::require_hidden(&client.status().await, config.helper_uid)
                    .is_ok()
                    .then_some(())
            })
            .await?;
        }
    } else {
        heading(&format!("Allow Atlas in {name}'s account"));
        say(&format!(
            "  Atlas needs two macOS permissions there:\n    \
             · Screen Recording, to see {name}'s screen\n    \
             · Accessibility, to click and type there\n  \
             1. Switch to {name} (sign in if asked).\n  \
             2. macOS asks about Accessibility: click Open System Settings and turn Atlas on.\n  \
             3. System Settings then shows Screen & System Audio Recording: turn Atlas on\n     \
             there too. If macOS offers to quit and reopen Atlas, either choice is fine.\n  \
             4. Switch back here. Atlas carries on by itself."
        ));
        loop {
            // The person is here, in their own account: offer the way over.
            if client.status().await["session"]["onConsole"] != true {
                offer_lock(&name)?;
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
                    let back = crate::session::require_hidden(&status, config.helper_uid).is_ok();
                    (missing(&status).is_empty() || (visited && back)).then_some(())
                })
                .await?;
            }
            // macOS applies a new Screen Recording grant to new processes
            // only, so the helper restarts to pick up what the person allowed.
            if !missing(&client.status().await).is_empty() {
                let _spinner = Spinner::new("Restarting the helper to pick up the permissions…");
                restart_daemon(&client, &config).await?;
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
        if crate::session::require_hidden(&client.status().await, config.helper_uid).is_err() {
            let _spinner = Spinner::new("Waiting for you to switch back to your own account…");
            wait_for("you to switch back", async || {
                crate::session::require_hidden(&client.status().await, config.helper_uid)
                    .is_ok()
                    .then_some(())
            })
            .await?;
        }
    }

    recover().await?;
    {
        let _spinner = Spinner::new("Starting input and checking everything…");
        client.start_input().await?;
        let ownership = client.controller.acquire(true)?;
        let recorded = setup::migrate::apply_fresh(&config_path, &ownership).await?;
        if recorded["ok"] != true {
            return Err(problem(format!(
                "Setup could not be verified: {}",
                recorded["error"].as_str().unwrap_or("unknown")
            )));
        }
        drop(ownership);
        let doctor = crate::capability::doctor(client.status().await, config.helper_uid);
        if doctor["ready"] != true {
            return Err(problem(
                "Setup finished but is not ready; run atlas doctor to see why",
            ));
        }
    }
    done("Checked", "the helper works");
    done_steps.push(json!({"step": "verified"}));

    finish(&mut done_steps, &config_path)?;
    ready();
    Ok(json!({"ok": true, "ready": true, "steps": done_steps, "config": config_path}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_commands_hide_log_lines_and_keep_their_outcome() {
        let script = "echo '2026-10-11 14:28:19.767 sysadminctl[1:2] noise' >&2; \
                      printf 'User password:' >&2; sleep 0.4; echo >&2; exit $0";
        assert!(quietly_attended("/bin/sh", &["-c", script, "0"], "failed").is_ok());
        let failed = quietly_attended("/bin/sh", &["-c", script, "3"], "it failed").unwrap_err();
        assert_eq!(failed.error, "it failed");
    }

    /// A configuration is written once, privately, and never over another.
    #[test]
    fn a_configuration_is_never_written_over_another() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Atlas/atlas.toml");
        let helper = setup::Account {
            name: "atlas".into(),
            uid: 503,
            home: "/Users/atlas".into(),
        };
        let config = proposed(&helper).unwrap();
        write_config(&path, &config).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let before = fs::read(&path).unwrap();
        assert!(
            write_config(&path, &config).is_err(),
            "an existing file stays"
        );
        assert_eq!(fs::read(&path).unwrap(), before);
        let leftovers = fs::read_dir(path.parent().unwrap()).unwrap().count();
        assert_eq!(leftovers, 1, "no temporary file is left behind");
    }

    #[test]
    fn new_account_names_are_short_and_plain() {
        assert!(valid_account_name("atlas").is_ok());
        assert!(valid_account_name("qa_helper-2").is_ok());
        assert!(valid_account_name("").is_err());
        assert!(valid_account_name("has space").is_err());
        assert!(valid_account_name("-dash").is_err());
        assert!(valid_account_name(&"x".repeat(32)).is_err());
    }

    /// A new configuration points everything at the helper's private layout.
    #[test]
    fn a_new_configuration_uses_the_managed_layout() {
        let helper = setup::Account {
            name: "atlas".into(),
            uid: 503,
            home: "/Users/atlas".into(),
        };
        let config = proposed(&helper).unwrap();
        assert_eq!(config.helper_uid, 503);
        assert_eq!(
            config.socket_path,
            PathBuf::from("/Users/atlas/.atlas/helper/503.sock")
        );
        assert_eq!(
            config.input_socket,
            PathBuf::from("/Users/atlas/.atlas/input/input.sock")
        );
        assert_eq!(
            config.identity_probe,
            PathBuf::from("/Users/Shared/Atlas/atlas")
        );
        assert!(
            config
                .output_dir
                .ends_with("Library/Application Support/Atlas/runs")
        );
    }
}
