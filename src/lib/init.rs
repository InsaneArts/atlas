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

/// How long init waits for the person to log in or click Allow.
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

/// Read `stream` on a thread of its own, passing each chunk on as it
/// arrives; the channel closes when the stream ends.
pub(crate) fn forward(
    mut stream: impl std::io::Read + Send + 'static,
) -> std::sync::mpsc::Receiver<Vec<u8>> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        while let Ok(read @ 1..) = stream.read(&mut chunk) {
            if sender.send(chunk[..read].to_vec()).is_err() {
                break;
            }
        }
    });

    receiver
}

/// Like `attended`, but timestamped log lines on standard error (as
/// sysadminctl writes) are dropped; anything else, such as a password
/// question, is shown as it arrives. On failure the dropped lines are shown.
fn quietly_attended(program: &str, args: &[&str], why: &str) -> Result<()> {
    let mut child = Command::new(program)
        .args(args)
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    let stream = child
        .stderr
        .take()
        .ok_or_else(|| problem("The command has no error output"))?;
    let receiver = forward(stream);

    let log_line = |bytes: &[u8]| {
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
pub(crate) fn app_executable() -> Result<PathBuf> {
    let executable = crate::update::installed_executable()?;
    if !crate::update::reachable_by_others(&executable) {
        return Err(problem(
            "Atlas.app must be in /Applications so the helper account can run it",
        ));
    }

    Ok(executable)
}

/// `~/.local/bin/atlas` → the app, so `atlas` works in a terminal.
pub(crate) fn link_command(executable: &Path) -> Result<Value> {
    link_command_in(&config::home()?.join(".local/bin"), executable)
}

/// [`link_command`] into the folder `bin`.
fn link_command_in(bin: &Path, executable: &Path) -> Result<Value> {
    fs::create_dir_all(bin)?;

    let link = bin.join("atlas");
    if fs::canonicalize(&link).ok().as_deref() == Some(executable) {
        return Ok(json!({"linked": link, "changed": false}));
    }

    if !replaceable(&link) {
        return Err(problem(format!(
            "{} is another program, so it was left alone. Move it aside, then try again.",
            crate::ui::text::tilde(&link)
        )));
    }

    let temporary = bin.join(format!(".atlas.{}", crate::control::token()));
    std::os::unix::fs::symlink(executable, &temporary)?;
    fs::rename(&temporary, &link)?;

    Ok(json!({"linked": link, "changed": true}))
}

/// Whether `link` may become Atlas's command: nothing there, a link to
/// nothing, or a link into an Atlas.app.
fn replaceable(link: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(link) else {
        return true;
    };

    if !metadata.file_type().is_symlink() {
        return false;
    }

    match fs::canonicalize(link) {
        Err(_) => true,
        Ok(target) => target
            .ancestors()
            .any(|folder| folder.file_name().is_some_and(|name| name == "Atlas.app")),
    }
}

/// Whether `name` can be a new account's short name.
pub(crate) fn valid_account_name(name: &str) -> std::result::Result<(), String> {
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
    let candidates = crate::switch::standard_accounts()?;
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
    let mut args = vec![SYSADMINCTL.to_owned()];
    args.extend(add_user_args(&name, FULL_NAME, free_uid()?));
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    quietly_attended(
        "/usr/bin/sudo",
        &args,
        "The helper account could not be created",
    )?;

    setup::account_named(&name)?.ok_or_else(|| problem("The new account was not found"))
}

/// The full name a new helper account gets.
pub const FULL_NAME: &str = "Atlas";

/// macOS's account tool, which creates the helper account.
pub(crate) const SYSADMINCTL: &str = "/usr/sbin/sysadminctl";

/// The arguments that make `sysadminctl` create a standard account `name`
/// called `full_name` with user ID `uid`, asking for its password itself
/// (`-password -`), with Atlas's picture when it is at hand.
pub(crate) fn add_user_args(name: &str, full_name: &str, uid: u32) -> Vec<String> {
    let mut args: Vec<String> = [
        "-addUser",
        name,
        "-fullName",
        full_name,
        "-UID",
        &uid.to_string(),
        "-password",
        "-",
    ]
    .map(str::to_owned)
    .to_vec();

    if let Some(picture) = helper_picture() {
        args.extend(["-picture".to_owned(), picture]);
    }

    args
}

/// The picture a new helper account gets, Atlas's mark: shipped in
/// Atlas.app, or in a development checkout drawn into app/Resources.
/// macOS keeps a copy on the account, so the file need not stay.
pub(crate) fn helper_picture() -> Option<String> {
    let bundled = crate::update::installed_app()
        .ok()
        .map(|app| app.join("Contents/Resources/HelperPicture.png"));
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("app/Resources/HelperPicture.png");
    [bundled, Some(checkout)]
        .into_iter()
        .flatten()
        .find(|path| path.is_file())
        .and_then(|path| path.to_str().map(str::to_owned))
}

/// The lowest user ID from the first user ID (501) that no account has and
/// no process runs as: a deleted account's session can outlive it, and
/// sharing its ID would mix that session with the helper's.
pub(crate) fn free_uid() -> Result<u32> {
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

    (setup::FIRST_USER_UID..600)
        .find(|uid| !accounts.contains(uid) && !running.contains(uid))
        .ok_or_else(|| problem("No free user ID for the helper account"))
}

/// The configuration for `helper` in the managed private layout.
pub(crate) fn proposed(helper: &setup::Account) -> Result<Config> {
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
pub(crate) fn write_config(path: &Path, config: &Config) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| problem("The configuration needs a directory"))?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;

    let toml = config.to_toml()?;
    config::write_beside(
        path,
        0o600,
        |file| Ok(file.write_all(toml.as_bytes())?),
        // Never replaces a configuration that appeared meanwhile.
        |temporary| Ok(fs::hard_link(temporary, path)?),
    )
}

/// Replace the configuration that named `previous` (its helper account is
/// gone or changed), atomically, and only while the file still says so.
pub(crate) fn replace_config(path: &Path, previous: &Config, config: &Config) -> Result<()> {
    // Replacing a link would leave the file it points to unchanged.
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(problem(format!(
            "{} is a link, and its helper account changed. Update the file it points to (or \
             remove the link), then run atlas init again.",
            text::tilde(path)
        )));
    }

    let toml = config.to_toml()?;
    config::write_beside(
        path,
        0o600,
        |file| Ok(file.write_all(toml.as_bytes())?),
        |temporary| {
            if Config::load(path).ok().as_ref() != Some(previous) {
                return Err(problem(format!(
                    "{} changed while Atlas was updating it; run atlas init again",
                    text::tilde(path)
                )));
            }
            Ok(fs::rename(temporary, path)?)
        },
    )
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

/// Ask for administrator rights once, saying `why`, and keep them fresh.
fn administrator_rights(why: &str) -> Result<()> {
    heading("Administrator rights");
    say(why);

    attended(
        "/usr/bin/sudo",
        &["-v"],
        "Administrator rights were not granted",
    )?;

    keep_sudo_alive();
    done("Administrator rights", "");

    Ok(())
}

/// The helper's background job as this Atlas writes it, for `helper` and
/// the app's `executable`, with the layout it lives in.
fn helper_agent(helper: &setup::Account, executable: &Path) -> Result<(Agent, Layout)> {
    let controller = setup::account_with_uid(nix::unistd::getuid().as_raw())?
        .ok_or_else(|| problem("This account cannot be resolved"))?;
    let layout = Layout {
        uid: helper.uid,
        home: helper.home.clone(),
    };

    let agent = Agent {
        label: LABEL.to_owned(),
        program: executable.to_path_buf(),
        uid: helper.uid,
        controller_uid: controller.uid,
        socket: layout.socket(),
    };

    Ok((agent, layout))
}

/// Run one setup-helper request as the helper, inside its own login, with
/// the administrator rights init holds.
fn as_helper(executable: &Path, helper: &setup::Account, request: &Request) -> Result<Value> {
    let (succeeded, reply) = setup::request_as_helper(executable, helper, request, true)?;
    if !succeeded || reply["ok"] != true {
        return Err(problem(format!(
            "Setting up the helper failed: {}",
            reply["error"].as_str().unwrap_or("no reply")
        )));
    }

    Ok(reply)
}

/// Install the helper's background job `agent` through `run`, which runs one
/// setup-helper request as the helper. An earlier Atlas's job (another app,
/// another account, an older version) is set aside first, its definition
/// kept as a backup, and comes back when the new one cannot go in.
pub(crate) fn install_job(
    run: impl Fn(&Request) -> Result<Value>,
    uid: u32,
    agent: Agent,
    logs: &Path,
) -> Result<Value> {
    let inspected = run(&Request::Inspect {
        uid,
        labels: vec![LABEL.to_owned()],
    })?;
    let earlier = &inspected["jobs"][LABEL];
    let expected = setup::helper::text_sha256(&setup::helper::plist(&agent, logs));
    let retired = earlier["plistSha256"]
        .as_str()
        .filter(|sha| *sha != expected)
        .map(str::to_owned);
    if let Some(sha256) = &retired {
        run(&Request::Retire {
            uid,
            label: LABEL.to_owned(),
            sha256: sha256.clone(),
            missing_ok: false,
        })?;
    }

    let mut error = match run(&Request::Install { agent }) {
        Ok(installed) => return Ok(installed),
        Err(error) => error,
    };

    let Some(sha256) = retired else {
        return Err(error);
    };

    // The earlier job comes back when the new one cannot go in. The new
    // definition install may have written goes first, or it would block
    // that return.
    let restored = run(&Request::Retire {
        uid,
        label: LABEL.to_owned(),
        sha256: expected,
        missing_ok: true,
    })
    .and_then(|_| {
        run(&Request::Restore {
            uid,
            label: LABEL.to_owned(),
            sha256,
        })
    });

    if let Err(lost) = restored {
        error.error = format!(
            "{}; the earlier helper job could not be put back: {}",
            error.error, lost.error
        );
    }

    Err(error)
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
    let reply = transport.call(&json!({"op": "restart"})).await?;
    if reply["ok"] != true {
        // A refusal changed nothing, unless the helper says it may have.
        let uncertain = reply["uncertain"] == true;
        if !uncertain {
            client.controller.clear_if(&pending)?;
        }

        let error = problem(format!(
            "The helper did not restart: {}",
            reply["error"].as_str().unwrap_or("no reason given")
        ));

        return Err(if uncertain { error.uncertain() } else { error });
    }

    if !new_daemon_answers(client, &before).await {
        // The restart stays pending: its outcome is unknown.
        return Err(
            problem("The helper did not start again after a restart. Run atlas doctor.")
                .uncertain(),
        );
    }

    client.controller.clear_if(&pending)?;
    Ok(())
}

/// Whether a daemon other than the one whose pid was `before` answers
/// within 30 seconds.
async fn new_daemon_answers(client: &Client, before: &Value) -> bool {
    let deadline = Instant::now() + Duration::from_secs(30);

    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let pid = client.status().await["daemon"]["pid"].clone();
        if pid.is_number() && pid != *before {
            return true;
        }

        if Instant::now() >= deadline {
            return false;
        }
    }
}

/// Replace the helper's background job an earlier Atlas wrote with this
/// version's, from outside the daemon it runs (through sudo, as installing
/// it does): the earlier definition is kept as a backup, and comes back if
/// the new one cannot go in. Pending until a new daemon answers, like a
/// restart.
async fn replace_job(
    client: &Client,
    helper: &setup::Account,
    executable: &Path,
    agent: Agent,
    layout: &Layout,
) -> Result<()> {
    let _exclusion = client.controller.acquire(true)?;
    let before = client.status().await["daemon"]["pid"].clone();
    let pending = client.controller.quarantine()?;

    let run = |request: &Request| as_helper(executable, helper, request);
    install_job(run, helper.uid, agent, &layout.logs()).map_err(Error::uncertain)?;

    if !new_daemon_answers(client, &before).await {
        return Err(problem(format!(
            "The helper's background job was replaced but the helper did not start again. Its \
             log is {} in {}'s account; run atlas init again after checking it.",
            layout.logs().join(format!("{LABEL}.log")).display(),
            helper.name
        ))
        .uncertain());
    }

    client.controller.clear_if(&pending)?;
    Ok(())
}

/// Put the helper's daemon on this version before checking it: its
/// background job replaced when another version of Atlas wrote it, else
/// the daemon restarted when it still runs an earlier build (an update
/// whose handover did not finish).
async fn current_daemon_step(
    client: &Client,
    helper: &setup::Account,
    executable: &Path,
    done_steps: &mut Vec<Value>,
) -> Result<()> {
    let (agent, layout) = helper_agent(helper, executable)?;
    let expected = setup::helper::text_sha256(&setup::helper::plist(&agent, &layout.logs()));
    let job = setup::migrate::managed_job(client).await?;

    if outdated_job(&job, &expected) {
        administrator_rights(
            "  macOS asks for your password once, to replace the helper's background job with \
             this version's.",
        )?;

        let _spinner = Spinner::new("Replacing the helper's background job…");
        replace_job(client, helper, executable, agent, &layout).await?;
        done("Helper job", "replaced with this version's");
        done_steps.push(json!({"step": "daemon", "label": LABEL, "replaced": true}));

        return Ok(());
    }

    let pid = client.status().await["daemon"]["pid"]
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok());
    if pid.is_some_and(|pid| !setup::runs_current(pid, executable)) {
        let _spinner = Spinner::new("Restarting the helper on this version…");
        client.restart().await?;
        done("Helper", "restarted on this version");
        done_steps.push(json!({"step": "restart"}));
    }

    Ok(())
}

/// Whether the managed job's definition, as inspected (`job`), is a file
/// another version of Atlas wrote, not the `expected` one.
fn outdated_job(job: &Value, expected: &str) -> bool {
    job["plistKind"] == "regular" && job["plistSha256"].as_str() != Some(expected)
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

/// The closing steps, which never fail a setup that worked: the menu bar at
/// login, and teaching the installed agents. The menu bar is turned on at
/// login only by the first setup (a later run leaves the person's choice
/// alone), and only for the default settings, the ones the login item uses.
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
        let status = crate::menubar::set_login(crate::menubar::Login::Status).unwrap_or_default();
        if status["enabled"] == true {
            crate::menubar::decide_login();
        } else if status["available"] == true {
            match crate::menubar::set_login(crate::menubar::Login::On) {
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
pub(crate) fn missing(status: &Value) -> Vec<&'static str> {
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

    let executable = app_step(&mut done_steps)?;
    let (config, helper) = helper_step(config_path)?;
    let name = helper.name.clone();
    // Whether an earlier run already finished setup (decides the first-run steps).
    let settled_before = setup::migrate::settled(helper.uid);
    let client = Client::new(config.clone())?;

    // Only a helper that answers in the background with Accessibility can
    // be recovered now; otherwise the steps below start it first.
    let status = client.status().await;
    if crate::session::require_hidden(&status, config.helper_uid).is_ok()
        && status["permissions"]["accessibility"] != false
    {
        recover(&client).await?;
    }

    // Ready now, set up before, and running this very build throughout.
    let status = client.status().await;
    if crate::capability::doctor(status.clone(), config.helper_uid)["ready"] == true
        && settled_before
        && crate::session::input_build_current(&status)
    {
        done("Already set up", "everything checks out");
        done_steps.push(json!({"step": "verified"}));
        finish(&mut done_steps, config_path)?;
        ready();

        return Ok(json!({"ok": true, "ready": true, "alreadySetUp": true, "steps": done_steps}));
    }

    done_steps.push(json!({"step": "config", "path": config_path, "helper": name}));

    if !daemon_answers(&client, config.helper_uid).await {
        start_helper(&client, &config, &helper, &executable, &mut done_steps).await?;
    }

    // The session checks below run this executable in the helper.
    client.install()?;
    permissions_step(&client, &config, &name).await?;
    current_daemon_step(&client, &helper, &executable, &mut done_steps).await?;
    verify_step(&client, &config, config_path).await?;
    done("Checked", "the helper works");
    done_steps.push(json!({"step": "verified"}));

    finish(&mut done_steps, config_path)?;
    ready();

    Ok(json!({"ok": true, "ready": true, "steps": done_steps, "config": config_path}))
}

/// The installed, signed app, and `atlas` in a terminal reaching it.
fn app_step(done_steps: &mut Vec<Value>) -> Result<PathBuf> {
    let executable = app_executable()?;
    // Checked before anything is set up: macOS keeps the helper's permissions
    // only for Atlas signed by its Developer ID.
    if !setup::developer_id(&executable) {
        return Err(problem(format!(
            "This Atlas.app is not signed by Atlas's developer. Install Atlas with: {} (or \
             atlas update), then run atlas init again.",
            crate::update::INSTALL_COMMAND
        )));
    }

    let linked = link_command(&executable)?;
    done("Atlas.app", &text::tilde(crate::update::installed_app()?));
    done_steps.push(json!({"step": "app", "command": linked}));

    Ok(executable)
}

/// The settings and the standard helper account they name: chosen (or
/// created) when there are none or their account is gone, and following
/// whichever account has the user ID now.
fn helper_step(config_path: &Path) -> Result<(Config, setup::Account)> {
    // The path is the one the command line resolved (the installed one by
    // default); a configuration is created only where none exists, and an
    // existing one that does not load is reported, never replaced.
    let absent = matches!(
        fs::symlink_metadata(config_path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    );
    let mut config = if absent {
        let helper = choose_helper()?;
        let config = proposed(&helper)?;
        write_config(config_path, &config)?;
        config
    } else {
        Config::load(config_path)?
    };

    // The account the settings name was deleted: choose (or create) another.
    if setup::account_with_uid(config.helper_uid)?.is_none() {
        say(&format!(
            "  The helper account in {} (user ID {}) no longer exists.",
            text::tilde(config_path),
            config.helper_uid
        ));

        let helper = choose_helper()?;
        let replacement = proposed(&helper)?;
        replace_config(config_path, &config, &replacement)?;
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
        replace_config(config_path, &config, &current)?;
        config = current;
    }

    let name = &helper.name;
    match setup::administrator(name) {
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

    // An older account with the same user ID was deleted while logged in:
    // its session still runs and would be mistaken for the helper's.
    if crate::session::leftover_session(helper.uid, &helper.name, &helper.home) {
        return Err(problem(format!(
            "macOS still runs a session of a deleted account with {name}'s user ID, and \
             mixes it up with {name}. Restart the Mac, then run atlas init again."
        )));
    }

    done(
        "Helper account",
        &format!("{name} · settings in {}", text::tilde(config_path)),
    );

    if setup::migrate::archive_foreign(helper.uid, &helper.home)? {
        say("  Set aside a setup record left by an earlier account with the same ID.");
    }

    Ok((config, helper))
}

/// An earlier action of unknown outcome is recovered before anything else
/// when it can be; recovery needs Accessibility, so without it recovery
/// waits until the person has allowed it.
async fn recover(client: &Client) -> Result<()> {
    if !client.controller.pending()?.is_empty() {
        let spinner = Spinner::new("Recovering an unfinished action…");
        client.restart().await?;
        drop(spinner);
        done("Recovered an unfinished action", "");
    }

    Ok(())
}

/// Whether the helper's daemon answers (in the background or not).
async fn daemon_answers(client: &Client, uid: u32) -> bool {
    let status = client.status().await;
    crate::session::require_hidden(&status, uid).is_ok() || status["daemon"]["pid"].is_number()
}

/// Whether the helper's daemon answers within 30 tries, `pause` apart.
async fn daemon_answers_within(client: &Client, uid: u32, pause: Duration) -> bool {
    for _ in 0..30 {
        if daemon_answers(client, uid).await {
            return true;
        }

        tokio::time::sleep(pause).await;
    }

    false
}

/// Bring up a helper daemon that does not answer: the person signs the
/// helper in, and (unless an earlier setup's job starts by itself) Atlas
/// installs the helper's background job with administrator rights.
async fn start_helper(
    client: &Client,
    config: &Config,
    helper: &setup::Account,
    executable: &Path,
    done_steps: &mut Vec<Value>,
) -> Result<()> {
    let name = &helper.name;
    // Set up before: the helper's job is installed and starts by itself
    // once the helper signs in (after a restart, say). No password then.
    let installed = setup::migrate::settled(helper.uid);

    let mut sudo_ready = false;
    let admin = |sudo_ready: &mut bool| -> Result<()> {
        if *sudo_ready {
            return Ok(());
        }

        administrator_rights(
            "  macOS asks for your password once, to install the helper's background job.",
        )?;
        *sudo_ready = true;

        Ok(())
    };

    if !installed {
        admin(&mut sudo_ready)?;
    }

    let logged_in = || crate::session::logged_in(helper.uid) == Some(true);
    if !logged_in() {
        heading(&format!("Log in to {name} once"));

        if installed {
            say(&format!(
                "  {name} is logged out (after a restart or a log-out). Switch to {name}, log in,\n  \
                     then switch back here."
            ));
        } else {
            say(&format!(
                "  Switch to {name}, log in and finish macOS's setup questions. If macOS then\n  \
                     asks about Accessibility or Screen Recording for Atlas, turn Atlas on.\n  \
                     Then switch back here."
            ));
        }

        offer_lock(name)?;

        {
            let _spinner = Spinner::new(format!(
                "Waiting for {name} to log in… (Ctrl-C to stop; run atlas init again later)"
            ));
            wait_for("the helper to log in", async || logged_in().then_some(())).await?;
        }

        done(&format!("{name} logged in"), "");
    }

    let mut started = false;

    if installed {
        let _spinner = Spinner::new("Waiting for the helper to start…");
        started = daemon_answers_within(client, config.helper_uid, Duration::from_secs(2)).await;
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
        let (agent, layout) = helper_agent(helper, executable)?;

        let run = |request: &Request| as_helper(executable, helper, request);
        run(&Request::Prepare {
            uid: helper.uid,
            controller: controller.name.clone(),
        })?;

        install_job(run, helper.uid, agent, &layout.logs())?;

        spinner.set("Starting the helper…");
        let answered =
            daemon_answers_within(client, config.helper_uid, Duration::from_secs(1)).await;
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

    Ok(())
}

/// Unless `status` has the helper in the background, wait until it is.
async fn wait_until_back(client: &Client, uid: u32, status: &Value) -> Result<()> {
    if crate::session::require_hidden(status, uid).is_err() {
        let _spinner = Spinner::new("Waiting for you to switch back to your own account…");
        wait_for("you to switch back", async || {
            crate::session::require_hidden(&client.status().await, uid)
                .is_ok()
                .then_some(())
        })
        .await?;
    }

    Ok(())
}

/// Screen Recording and Accessibility for Atlas in the helper: already
/// allowed, or guided until the person allows both there and comes back.
async fn permissions_step(client: &Client, config: &Config, name: &str) -> Result<()> {
    let status = client.status().await;
    if missing(&status).is_empty() {
        // macOS keeps these approvals for Atlas across the Mac, so a helper
        // can have them before it is ever asked.
        done(
            "Permissions",
            "Screen Recording · Accessibility already allowed",
        );
        wait_until_back(client, config.helper_uid, &status).await?;
    } else {
        heading(&format!("Allow Atlas in {name}'s account"));

        say(&format!(
            "  Atlas needs two macOS permissions there:\n    \
             · Screen Recording, to see {name}'s screen\n    \
             · Accessibility, to click and type there\n  \
             1. Switch to {name} (log in if asked).\n  \
             2. macOS asks about Accessibility: click Open System Settings and turn Atlas on.\n  \
             3. System Settings then shows Screen & System Audio Recording: turn Atlas on\n     \
             there too. If macOS offers to quit and reopen Atlas, either choice is fine.\n  \
             4. When Atlas says it is set up there, switch back here. Atlas carries on by itself."
        ));

        loop {
            // The person is here, in their own account: offer the way over.
            if client.status().await["session"]["onConsole"] != true {
                offer_lock(name)?;
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
                restart_daemon(client, config).await?;
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
        wait_until_back(client, config.helper_uid, &client.status().await).await?;
    }

    Ok(())
}

/// Recover, start input, record the setup and check it end to end.
async fn verify_step(client: &Client, config: &Config, config_path: &Path) -> Result<()> {
    recover(client).await?;

    {
        let _spinner = Spinner::new("Starting input and checking everything…");
        client.start_input().await?;
        let ownership = client.controller.acquire(true)?;
        let recorded = setup::migrate::apply_fresh(config_path, &ownership).await?;
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

    Ok(())
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

    /// An earlier job is retired only when its definition differs, and
    /// restored when the new one cannot go in.
    #[test]
    fn an_earlier_job_is_set_aside_and_restored_on_failure() {
        let agent = Agent {
            label: LABEL.to_owned(),
            program: "/Applications/Atlas.app/Contents/MacOS/atlas".into(),
            uid: 503,
            controller_uid: 501,
            socket: "/Users/atlas/.atlas/helper/503.sock".into(),
        };
        let logs = Path::new("/Users/atlas/.atlas/logs");
        let ours = setup::helper::text_sha256(&setup::helper::plist(&agent, logs));

        let attempt_with = |earlier: Option<&str>, install_fails: bool, restore_fails: bool| {
            let actions = std::cell::RefCell::new(Vec::new());
            let run = |request: &Request| -> Result<Value> {
                let action = serde_json::to_value(request).unwrap()["action"].clone();
                actions
                    .borrow_mut()
                    .push(action.as_str().unwrap().to_owned());

                match request {
                    Request::Inspect { .. } => {
                        Ok(json!({"ok": true, "jobs": {LABEL: {"plistSha256": earlier}}}))
                    }
                    Request::Install { .. } if install_fails => Err(problem("refused")),
                    Request::Restore { .. } if restore_fails => Err(problem("no backup")),
                    _ => Ok(json!({"ok": true})),
                }
            };
            let outcome = install_job(run, 503, agent.clone(), logs);
            (outcome.map_err(|error| error.error), actions.into_inner())
        };
        let attempt = |earlier: Option<&str>, install_fails: bool| {
            let (outcome, actions) = attempt_with(earlier, install_fails, false);
            (outcome.is_ok(), actions)
        };

        assert_eq!(
            attempt(None, false),
            (true, vec!["inspect".into(), "install".into()])
        );
        assert_eq!(
            attempt(Some(&ours), false),
            (true, vec!["inspect".into(), "install".into()])
        );

        let other = "0".repeat(64);
        assert_eq!(
            attempt(Some(&other), false),
            (
                true,
                vec!["inspect".into(), "retire".into(), "install".into()]
            )
        );
        assert_eq!(
            attempt(Some(&other), true),
            (
                false,
                vec![
                    "inspect".into(),
                    "retire".into(),
                    "install".into(),
                    "retire".into(),
                    "restore".into()
                ]
            )
        );

        // A job that cannot be put back is said, not dropped.
        let (outcome, _) = attempt_with(Some(&other), true, true);
        let error = outcome.unwrap_err();
        assert!(error.starts_with("refused;"), "{error}");
        assert!(
            error.ends_with("could not be put back: no backup"),
            "{error}"
        );
    }

    /// Only a definition file with other bytes is replaced; none there (a
    /// job from an older label) or the expected one is left alone.
    #[test]
    fn only_another_versions_definition_is_replaced() {
        let ours = "a".repeat(64);
        let earlier = "b".repeat(64);
        let job = |kind: &str, sha: Option<&str>| json!({"plistKind": kind, "plistSha256": sha});

        assert!(outdated_job(&job("regular", Some(&earlier)), &ours));
        assert!(!outdated_job(&job("regular", Some(&ours)), &ours));
        assert!(!outdated_job(&job("absent", None), &ours));
        assert!(
            !outdated_job(&job("other", None), &ours),
            "a link is never replaced"
        );
        assert!(!outdated_job(&Value::Null, &ours));
    }

    #[test]
    fn the_command_replaces_only_a_missing_dead_or_atlas_link() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let bin = root.join("bin");
        let executable = root.join("Applications/Atlas.app/Contents/MacOS/atlas");
        let older = root.join("Old/Atlas.app/Contents/MacOS/atlas");
        for path in [&executable, &older] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"").unwrap();
        }
        let link = bin.join("atlas");

        assert_eq!(link_command_in(&bin, &executable).unwrap()["changed"], true);
        assert_eq!(
            link_command_in(&bin, &executable).unwrap()["changed"],
            false
        );

        for target in [older, root.join("gone")] {
            fs::remove_file(&link).unwrap();
            std::os::unix::fs::symlink(&target, &link).unwrap();
            assert_eq!(link_command_in(&bin, &executable).unwrap()["changed"], true);
            assert_eq!(fs::canonicalize(&link).unwrap(), executable);
        }

        let other = root.join("other-atlas");
        fs::write(&other, b"another program").unwrap();
        fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink(&other, &link).unwrap();
        assert!(link_command_in(&bin, &executable).is_err());
        assert_eq!(fs::canonicalize(&link).unwrap(), other);

        fs::remove_file(&link).unwrap();
        fs::write(&link, b"a script").unwrap();
        assert!(link_command_in(&bin, &executable).is_err());
        assert_eq!(fs::read(&link).unwrap(), b"a script");
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
