//! `understudy init`: a guided first setup, plain text, safe to run again.
//! Each step checks before it acts and is skipped when already done. It
//! never types or stores a password and never touches privacy settings: the
//! person signs in and clicks Allow themselves; Understudy waits and checks.
use crate::{
    client::Client,
    config::{self, Config},
    error::{Error, Result},
    setup::{
        self,
        helper::{Agent, LABEL, Layout, Request},
    },
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, IsTerminal, Write},
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

fn say(text: &str) {
    eprintln!("{text}");
}

fn step(text: &str) {
    eprintln!("\n• {text}");
}

fn ask(question: &str) -> Result<String> {
    eprint!("{question} ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    Ok(answer.trim().to_owned())
}

fn yes(question: &str) -> Result<bool> {
    Ok(matches!(
        ask(&format!("{question} [Y/n]"))?.as_str(),
        "" | "y" | "Y" | "yes"
    ))
}

/// A command that must succeed; its output goes to the person.
fn interactive(program: &str, args: &[&str], why: &str) -> Result<()> {
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

/// What the person answered to "which account?".
#[derive(Debug, PartialEq, Eq)]
enum Choice {
    Existing(usize),
    Create,
    Unknown,
}

/// Read an answer as a listed number or an account's short name; the number
/// after the list means "create a new account".
fn parse_choice(answer: &str, names: &[String]) -> Choice {
    let answer = answer.trim();
    if let Ok(number) = answer.parse::<usize>() {
        return match number {
            0 => Choice::Unknown,
            number if number <= names.len() => Choice::Existing(number - 1),
            number if number == names.len() + 1 => Choice::Create,
            _ => Choice::Unknown,
        };
    }
    names
        .iter()
        .position(|name| name == answer)
        .map_or(Choice::Unknown, Choice::Existing)
}

/// Choose an existing standard account, or (only when asked for, and
/// confirmed) create one.
fn choose_helper() -> Result<setup::Account> {
    let candidates: Vec<_> = crate::switch::accounts(None)?
        .into_iter()
        .filter(|account| !admin(&account.name))
        .collect();
    let names: Vec<String> = candidates
        .iter()
        .map(|account| account.name.clone())
        .collect();
    loop {
        say("Standard accounts on this Mac:");
        for (index, account) in candidates.iter().enumerate() {
            let label = account.real_name.as_deref().unwrap_or(&account.name);
            say(&format!("  {}. {label} — {}", index + 1, account.name));
        }
        say(&format!(
            "  {}. Create a new helper account",
            names.len() + 1
        ));
        match parse_choice(
            &ask("Which account should be the helper? (number or name)")?,
            &names,
        ) {
            Choice::Existing(index) => {
                return setup::account_named(&names[index])?
                    .ok_or_else(|| problem("That account disappeared"));
            }
            Choice::Create => break,
            Choice::Unknown => say("  Please type one of the numbers or account names above."),
        }
    }
    let name = match ask("Short name for the new helper account [understudy]:")? {
        empty if empty.is_empty() => "understudy".to_owned(),
        name => name,
    };
    if !name
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    {
        return Err(problem(
            "Account names use letters, digits, '.', '_' and '-'",
        ));
    }
    if setup::account_named(&name)?.is_some() {
        return Err(problem(format!(
            "An account named {name} already exists; choose it instead"
        )));
    }
    let answer = ask(&format!(
        "Create a new standard account named {name}? [y/N]"
    ))?;
    if !matches!(answer.as_str(), "y" | "Y" | "yes") {
        return Err(problem(
            "No account was created; run understudy init again to choose",
        ));
    }
    say("macOS will ask for your administrator password, then a password for the helper.");
    interactive(
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
/// the new one to answer.
async fn restart_daemon(client: &Client, config: &Config) -> Result<()> {
    let before = client.status().await["daemon"]["pid"].clone();
    let transport = crate::transport::Transport {
        path: config.socket_path.clone(),
        timeout: Duration::from_secs(10),
        peer_uid: config.helper_uid,
    };
    transport.call(&json!({"op": "restart"})).await?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let pid = client.status().await["daemon"]["pid"].clone();
        if pid.is_number() && pid != before {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(problem(
                "The helper daemon did not come back after a restart",
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

/// The optional finishing touches, offered only when not already in place:
/// the menu bar at login, and teaching the installed agents.
fn finish(done: &mut Vec<Value>) -> Result<()> {
    // The menu bar is part of Understudy: on at login unless turned off
    // (understudy menubar --login off).
    let login = crate::menubar::login("status").unwrap_or_default();
    if login["available"] == true && login["enabled"] != true {
        step("Menu bar");
        let login = crate::menubar::login("on")?;
        say(if login["enabled"] == true {
            "  Understudy is in the menu bar and opens at login \
             (understudy menubar --login off turns that off)."
        } else {
            "  macOS did not enable it; turn on Open at Login from the menu bar later."
        });
        done.push(json!({"step": "menubar", "login": login}));
    }
    let agents = crate::agents::status();
    let found: Vec<&str> = agents["agents"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|agent| agent["installed"] == true && agent["connected"] != true)
        .filter_map(|agent| agent["agent"].as_str())
        .collect();
    if !found.is_empty() {
        step("Agents");
        if yes(&format!(
            "  Teach {} to use Understudy (adds its plugin)?",
            found.join(" and ")
        ))? {
            let connected = crate::agents::change(&[], false)?;
            say(if connected["ok"] == true {
                "  Connected. New agent sessions know how to use Understudy."
            } else {
                "  Not every agent connected; run understudy agents install later."
            });
            done.push(json!({"step": "agents", "result": connected}));
        }
    }
    Ok(())
}

/// Guide the person from an installed app to a ready helper.
pub async fn run(config_path: &Path) -> Result<Value> {
    if !std::io::stdin().is_terminal() {
        return Err(problem("init is a guided setup; run it in a terminal"));
    }
    let mut done = Vec::new();

    step("Understudy.app");
    let executable = app_executable()?;
    let linked = link_command(&executable)?;
    say(&format!("  Ready at {}", executable.display()));
    done.push(json!({"step": "app", "command": linked}));

    step("Configuration and helper account");
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
        say(&format!("  Wrote {}", config_path.display()));
        config
    } else {
        let config = Config::load(&config_path)?;
        say(&format!("  Using {}", config_path.display()));
        config
    };
    let helper = setup::account_with_uid(config.helper_uid)?
        .ok_or_else(|| problem("The configured helper account does not exist"))?;
    if setup::migrate::archive_foreign(helper.uid, &helper.home)? {
        say("  Set aside a setup record left by an earlier account with the same ID.");
    }
    let client = Client::new(config.clone())?;
    // An earlier action of unknown outcome is recovered before anything else
    // when it can be; recovery needs Accessibility, so without it recovery
    // waits until the person has allowed it below.
    let recover = async || -> Result<()> {
        if !client.controller.pending()?.is_empty() {
            step("Recovering an unfinished action");
            client.restart().await?;
        }
        Ok(())
    };
    if client.status().await["permissions"]["accessibility"] != false {
        recover().await?;
    }
    if crate::capability::doctor(client.status().await, config.helper_uid)["ready"] == true
        && setup::migrate::settled(config.helper_uid)
    {
        say("  The helper is already set up and ready.");
        let mut done = vec![json!({"step": "verified"})];
        finish(&mut done)?;
        say("\nUnderstudy is ready. Try: understudy screenshot");
        return Ok(json!({"ok": true, "ready": true, "alreadySetUp": true, "steps": done}));
    }
    done.push(json!({"step": "config", "path": config_path, "helper": helper.name}));

    let daemon_answers = || async {
        let status = client.status().await;
        crate::session::require_hidden(&status, config.helper_uid).is_ok()
            || status["daemon"]["pid"].is_number()
    };
    if !daemon_answers().await {
        step("Administrator rights (once)");
        say("  Understudy needs your administrator password once, to install its helper job.");
        interactive(
            "/usr/bin/sudo",
            &["-v"],
            "Administrator rights were not granted",
        )?;
        keep_sudo_alive();

        step(&format!("Sign in to {} once", helper.name));
        if !helper_logged_in(helper.uid) {
            say(&format!(
                "  Switch to {name}, sign in (finish any setup questions), and stay there:\n  \
                 Understudy will ask for two permissions in that session.",
                name = helper.name
            ));
            let manual = format!(
                "  To switch: click your name (or the people icon) in the menu bar and choose\n  \
                 {name}, or use Apple menu › Lock Screen, click Switch User and pick {name}.",
                name = helper.name
            );
            if yes(&format!(
                "  Lock the screen now so you can switch to {}?",
                helper.name
            ))? {
                match crate::switch::lock_screen() {
                    Ok(_) => say(&format!(
                        "  Locked. On the lock screen, click Switch User, choose {} and sign in.",
                        helper.name
                    )),
                    Err(error) => say(&format!("  {} \n{manual}", error.error)),
                }
            } else {
                say(&manual);
            }
            say(&format!(
                "\n  Waiting for {} to sign in… (Ctrl-C stops; running init again resumes.)",
                helper.name
            ));
            wait_for("the helper to sign in", async || {
                helper_logged_in(helper.uid).then_some(())
            })
            .await?;
        }

        step("Helper job");
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
        done.push(json!({"step": "daemon", "label": LABEL}));
    }

    // The session checks below run this executable in the helper.
    client.install()?;

    let missing = |status: &Value| -> Vec<&'static str> {
        [
            ("screenCapture", "Screen & System Audio Recording"),
            ("accessibility", "Accessibility"),
        ]
        .into_iter()
        .filter(|(key, _)| status["permissions"][*key] != true)
        .map(|(_, pane)| pane)
        .collect()
    };
    if missing(&client.status().await).is_empty() {
        step("Switch back to your own account");
        say("  Understudy works while the helper runs in the background. Waiting for you…");
        wait_for("you to switch back", async || {
            crate::session::require_hidden(&client.status().await, config.helper_uid)
                .is_ok()
                .then_some(())
        })
        .await?;
    } else {
        step("Permissions in the helper session");
        say(&format!(
            "  Understudy asks for Screen Recording and Accessibility in {name}'s session,
               one request after the other. Go there, click Allow (or Open System Settings and
               turn Understudy on) for each, then switch back to your own account.",
            name = helper.name
        ));
        loop {
            // Already back in this account: offer the way over.
            if crate::session::require_hidden(&client.status().await, config.helper_uid).is_ok()
                && yes(&format!(
                    "  Lock the screen now so you can switch to {}?",
                    helper.name
                ))?
            {
                match crate::switch::lock_screen() {
                    Ok(_) => say(&format!(
                        "  Locked. Click Switch User, choose {}, allow, then switch back.",
                        helper.name
                    )),
                    Err(error) => say(&format!("  {}", error.error)),
                }
            }
            say("  Waiting for you to allow and come back…");
            // Done once the helper has been on screen and is hidden again (or
            // everything is already granted).
            let mut visited = false;
            wait_for("you to allow and switch back", async || {
                let status = client.status().await;
                if status["session"]["onConsole"] == true {
                    visited = true;
                }
                let back = crate::session::require_hidden(&status, config.helper_uid).is_ok();
                (missing(&status).is_empty() || (visited && back)).then_some(())
            })
            .await?;
            // macOS applies a new Screen Recording grant to new processes
            // only, so the daemon restarts to pick up what the person allowed.
            if !missing(&client.status().await).is_empty() {
                restart_daemon(&client, &config).await?;
            }
            let lacking = missing(&client.status().await);
            if lacking.is_empty() {
                break;
            }
            let panes = lacking.join(" and ");
            say(&format!(
                "\n  Understudy is still off for {panes}. In {name}'s session, open\n  \
                 System Settings › Privacy & Security › {panes} and turn Understudy on.",
                name = helper.name
            ));
            if !yes("  Try again?")? {
                return Err(problem(format!(
                    "Understudy still lacks {panes} in {name}. Switch to {name}, open System \
                     Settings › Privacy & Security › {panes}, turn Understudy on, switch back \
                     and run understudy init again.",
                    name = helper.name
                )));
            }
        }
    }

    recover().await?;
    step("Input and verification");
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
            "Setup finished but doctor does not report ready; run understudy doctor",
        ));
    }
    done.push(json!({"step": "verified"}));

    finish(&mut done)?;
    say("\nUnderstudy is ready. Try: understudy screenshot");
    Ok(json!({"ok": true, "ready": true, "steps": done, "config": config_path}))
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
    fn answers_name_an_account_by_number_or_name_and_never_guess() {
        let names = vec!["atlastest".to_owned(), "other".to_owned()];
        assert_eq!(parse_choice("1", &names), Choice::Existing(0));
        assert_eq!(parse_choice("atlastest", &names), Choice::Existing(0));
        assert_eq!(parse_choice(" other ", &names), Choice::Existing(1));
        assert_eq!(parse_choice("3", &names), Choice::Create);
        assert_eq!(parse_choice("0", &names), Choice::Unknown);
        assert_eq!(parse_choice("4", &names), Choice::Unknown);
        assert_eq!(parse_choice("Atlas Test", &names), Choice::Unknown);
        assert_eq!(parse_choice("", &names), Choice::Unknown);
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
