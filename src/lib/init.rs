//! `understudy init`: a guided first setup, safe to run again. Each step
//! checks before it acts and is skipped when already done. It never types or
//! stores a password and never touches privacy settings: the person signs in
//! and clicks Allow themselves; Understudy waits and checks.
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
    let executable = app.join("Contents/MacOS/understudy");
    if !crate::update::reachable_by_others(&executable) {
        return Err(problem(
            "Understudy.app must be in /Applications so the helper account can run it",
        ));
    }
    Ok(executable)
}

/// `~/.local/bin/understudy` → the app, so `understudy` works in a terminal.
fn link_command(executable: &Path) -> Result<Value> {
    let bin = config::home()?.join(".local/bin");
    fs::create_dir_all(&bin)?;
    let link = bin.join("understudy");
    if fs::canonicalize(&link).ok().as_deref() == Some(executable) {
        return Ok(json!({"linked": link, "changed": false}));
    }
    let temporary = bin.join(format!(".understudy.{}", crate::control::token()));
    std::os::unix::fs::symlink(executable, &temporary)?;
    fs::rename(&temporary, &link)?;
    Ok(json!({"linked": link, "changed": true}))
}

/// Whether `name` administers this Mac (a helper must not).
fn admin(name: &str) -> bool {
    Command::new("/usr/sbin/dseditgroup")
        .args(["-o", "checkmember", "-m", name, "admin"])
        .output()
        .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).starts_with("yes"))
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
        .filter(|account| !admin(&account.name))
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
        say("There is no other standard account yet, so Understudy will make one.");
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
        "understudy",
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
            "No account was created; run understudy init again to choose",
        ));
    }
    say("macOS asks for your administrator password, then a password for the new account.");
    attended(
        "/usr/bin/sudo",
        &[
            "/usr/sbin/sysadminctl",
            "-addUser",
            &name,
            "-fullName",
            "Understudy Helper",
            "-password",
            "-",
        ],
        "The helper account could not be created",
    )?;
    setup::account_named(&name)?.ok_or_else(|| problem("The new account was not found"))
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
        identity_probe: "/Users/Shared/Understudy/understudy".into(),
        input_socket: layout.root().join(setup::helper::INPUT).join("input.sock"),
        output_dir: config::home()?.join("Library/Application Support/Understudy/runs"),
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
    let temporary = parent.join(format!(".understudy.toml.{}", crate::control::token()));
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
                "The helper did not start again after a restart. Run understudy doctor.",
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
                "Gave up waiting for {what}; run understudy init again"
            )));
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

/// The finishing touches, offered only when not already in place: the menu
/// bar at login, and teaching the installed agents.
fn finish(done_steps: &mut Vec<Value>) -> Result<()> {
    // The menu bar is part of Understudy: on at login unless turned off.
    let login = crate::menubar::login("status").unwrap_or_default();
    if login["available"] == true && login["enabled"] != true {
        let login = crate::menubar::login("on")?;
        if login["enabled"] == true {
            done(
                "Menu bar",
                "opens at login (understudy menubar --login off turns it off)",
            );
        } else {
            skipped("Menu bar", "turn on Open at Login from its menu later");
        }
        done_steps.push(json!({"step": "menubar", "login": login}));
    }
    let agents = crate::agents::status();
    let found: Vec<&str> = agents["agents"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|agent| agent["installed"] == true && agent["connected"] != true)
        .filter_map(|agent| {
            agent["agent"]
                .as_str()
                .and_then(crate::agents::Agent::named)
        })
        .map(crate::agents::Agent::display_name)
        .collect();
    if !found.is_empty()
        && prompt::confirm(
            &format!(
                "Teach {} to use Understudy? (adds a plugin)",
                found.join(" and ")
            ),
            true,
            "run understudy agents install later",
        )?
    {
        let connected = {
            let _spinner = Spinner::new("Connecting…");
            crate::agents::change(&[], false)?
        };
        if connected["ok"] == true {
            done("Agents", &format!("{} connected", found.join(" and ")));
        } else {
            skipped(
                "Agents",
                "not every agent connected; run understudy agents install later",
            );
        }
        done_steps.push(json!({"step": "agents", "result": connected}));
    }
    Ok(())
}

/// The closing lines: ready, and what to try.
fn ready() {
    say(&format!("\n{}", style::bold("Understudy is ready.")));
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
        style::bold("Understudy setup"),
        style::dim(
            "A hidden second desktop for your agents. About five minutes; safe to run again."
        )
    ));
    say("");

    let executable = app_executable()?;
    let linked = link_command(&executable)?;
    done(
        "Understudy.app",
        &text::tilde(crate::update::installed_app()?),
    );
    done_steps.push(json!({"step": "app", "command": linked}));

    // The path is the one the command line resolved (the installed one by
    // default); a configuration is created only where none exists, and an
    // existing one that does not load is reported, never replaced.
    let config_path = config_path.to_owned();
    let absent = matches!(
        fs::symlink_metadata(&config_path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    );
    let config = if absent {
        let helper = choose_helper()?;
        let config = proposed(&helper)?;
        write_config(&config_path, &config)?;
        config
    } else {
        Config::load(&config_path)?
    };
    let helper = setup::account_with_uid(config.helper_uid)?
        .ok_or_else(|| problem("The configured helper account does not exist"))?;
    let name = helper.name.clone();
    done(
        "Helper account",
        &format!("{name} · settings in {}", text::tilde(&config_path)),
    );
    if setup::migrate::archive_foreign(helper.uid, &helper.home)? {
        say("  Set aside a setup record left by an earlier account with the same ID.");
    }
    let client = Client::new(config.clone())?;
    // An earlier action of unknown outcome is recovered before anything else
    // when it can be; recovery needs Accessibility, so without it recovery
    // waits until the person has allowed it below.
    let recover = async || -> Result<()> {
        if !client.controller.pending()?.is_empty() {
            let _spinner = Spinner::new("Recovering an unfinished action…");
            client.restart().await?;
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
    if crate::capability::doctor(client.status().await, config.helper_uid)["ready"] == true
        && setup::migrate::settled(config.helper_uid)
    {
        done("Already set up", "everything checks out");
        done_steps.push(json!({"step": "verified"}));
        finish(&mut done_steps)?;
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
        heading("Administrator rights");
        say("  macOS asks for your password once, to install the helper's background job.");
        attended(
            "/usr/bin/sudo",
            &["-v"],
            "Administrator rights were not granted",
        )?;
        keep_sudo_alive();
        done("Administrator rights", "");

        if !helper_logged_in(helper.uid) {
            heading(&format!("Sign in to {name} once"));
            say(&format!(
                "  Switch to {name}, sign in and finish any setup questions. Stay there:\n  \
                 Understudy asks for two permissions next."
            ));
            offer_lock(&name)?;
            {
                let _spinner = Spinner::new(format!(
                    "Waiting for {name} to sign in… (Ctrl-C to stop; run understudy init again later)"
                ));
                wait_for("the helper to sign in", async || {
                    helper_logged_in(helper.uid).then_some(())
                })
                .await?;
            }
            done(&format!("{name} signed in"), "");
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
        drop(spinner);
        done("Helper job", "installed");
        done_steps.push(json!({"step": "daemon", "label": LABEL}));
    }

    // The session checks below run this executable in the helper.
    client.install()?;

    let status = client.status().await;
    if missing(&status).is_empty() {
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
        heading("Two permissions");
        say(&format!(
            "  In {name}'s session, Understudy asks for Screen Recording and Accessibility,\n  \
             one after the other. Click Allow (or Open System Settings and turn Understudy on)\n  \
             for each, then switch back here."
        ));
        loop {
            // Already back in this account: offer the way over.
            if crate::session::require_hidden(&client.status().await, config.helper_uid).is_ok() {
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
                "{} Understudy is still off. In {name}'s session, open System Settings ›\n  \
                 Privacy & Security and turn Understudy on in:",
                style::fail()
            ));
            for pane in &lacking {
                say(&format!("    · {pane}"));
            }
            if !prompt::confirm("Go back and turn it on now?", true, "")? {
                return Err(problem(format!(
                    "Understudy is still off in {name}'s {}. Turn it on there, switch back \
                     and run understudy init again.",
                    lacking.join(" and ")
                )));
            }
        }
        done("Permissions", "Screen Recording · Accessibility");
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
                "Setup finished but is not ready; run understudy doctor to see why",
            ));
        }
    }
    done("Checked", "the helper works");
    done_steps.push(json!({"step": "verified"}));

    finish(&mut done_steps)?;
    ready();
    Ok(json!({"ok": true, "ready": true, "steps": done_steps, "config": config_path}))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A configuration is written once, privately, and never over another.
    #[test]
    fn a_configuration_is_never_written_over_another() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Understudy/understudy.toml");
        let helper = setup::Account {
            name: "understudy".into(),
            uid: 503,
            home: "/Users/understudy".into(),
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
        assert!(valid_account_name("understudy").is_ok());
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
            name: "understudy".into(),
            uid: 503,
            home: "/Users/understudy".into(),
        };
        let config = proposed(&helper).unwrap();
        assert_eq!(config.helper_uid, 503);
        assert_eq!(
            config.socket_path,
            PathBuf::from("/Users/understudy/.understudy/helper/503.sock")
        );
        assert_eq!(
            config.input_socket,
            PathBuf::from("/Users/understudy/.understudy/input/input.sock")
        );
        assert_eq!(
            config.identity_probe,
            PathBuf::from("/Users/Shared/Understudy/understudy")
        );
        assert!(
            config
                .output_dir
                .ends_with("Library/Application Support/Understudy/runs")
        );
    }
}
