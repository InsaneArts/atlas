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

/// Choose an existing standard account, or create one.
fn choose_helper() -> Result<setup::Account> {
    let candidates: Vec<_> = crate::switch::accounts(None)?
        .into_iter()
        .filter(|account| !admin(&account.name))
        .collect();
    if !candidates.is_empty() {
        say("Standard accounts on this Mac:");
        for (index, account) in candidates.iter().enumerate() {
            let label = account.real_name.as_deref().unwrap_or(&account.name);
            say(&format!("  {}. {label} — {}", index + 1, account.name));
        }
        say(&format!(
            "  {}. Create a new helper account",
            candidates.len() + 1
        ));
        let choice = ask("Which account should be the helper?")?;
        if let Some(account) = choice
            .parse::<usize>()
            .ok()
            .and_then(|number| number.checked_sub(1))
            .and_then(|index| candidates.get(index))
        {
            return setup::account_named(&account.name)?
                .ok_or_else(|| problem("That account disappeared"));
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

fn write_config(path: &Path, config: &Config) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
    }
    fs::write(path, config.to_toml()?)?;
    Ok(())
}

/// Whether the helper's graphical session exists (it has logged in).
fn helper_logged_in(uid: u32) -> bool {
    sudo(&["/bin/launchctl", "print", &format!("gui/{uid}")])
        .is_ok_and(|output| output.status.success())
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
    let config_path = if config_path.exists() {
        config_path.to_owned()
    } else {
        config::installed_path()?
    };
    let config = match Config::load(&config_path) {
        Ok(config) => {
            say(&format!("  Using {}", config_path.display()));
            config
        }
        Err(_) => {
            let helper = choose_helper()?;
            let config = proposed(&helper)?;
            write_config(&config_path, &config)?;
            say(&format!("  Wrote {}", config_path.display()));
            config
        }
    };
    let helper = setup::account_with_uid(config.helper_uid)?
        .ok_or_else(|| problem("The configured helper account does not exist"))?;
    let client = Client::new(config.clone())?;
    if crate::capability::doctor(client.status().await, config.helper_uid)["ready"] == true
        && setup::migrate::recorded(config.helper_uid)
    {
        say("\nUnderstudy is already set up and ready.");
        return Ok(json!({"ok": true, "ready": true, "alreadySetUp": true}));
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

        step(&format!("Sign in to {} once", helper.name));
        if !helper_logged_in(helper.uid) {
            say(&format!(
                "  Switch to {name}, sign in (finish any setup questions), and stay there:\n  \
                 Understudy will ask for two permissions in that session.",
                name = helper.name
            ));
            if yes(&format!("  Switch to {} now?", helper.name))? {
                crate::switch::to(&crate::switch::Account {
                    name: helper.name.clone(),
                    uid: helper.uid,
                    real_name: None,
                    helper: true,
                })?;
            }
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

    step("Permissions in the helper session");
    let mut told = false;
    wait_for("Screen Recording and Accessibility", async || {
        let status = client.status().await;
        let permissions = &status["permissions"];
        let granted = permissions["screenCapture"] == true && permissions["accessibility"] == true;
        if !granted && !told {
            told = true;
            say(&format!(
                "  In {}'s session, allow Understudy in the two prompts (Screen Recording and\n  \
                 Accessibility). If a prompt was dismissed, open System Settings › Privacy &\n  \
                 Security there and turn Understudy on.",
                helper.name
            ));
        }
        granted.then_some(())
    })
    .await?;

    step("Switch back to your own account");
    say("  Understudy works while the helper runs in the background.");
    wait_for("you to switch back", async || {
        crate::session::require_hidden(&client.status().await, config.helper_uid)
            .is_ok()
            .then_some(())
    })
    .await?;

    step("Input and verification");
    client.install()?;
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
    say("\nUnderstudy is ready. Try: understudy screenshot");
    done.push(json!({"step": "verified"}));
    Ok(json!({"ok": true, "ready": true, "steps": done, "config": config_path}))
}

#[cfg(test)]
mod tests {
    use super::*;

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
