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
        configuration::{proposed, replace_config, write_config},
        helper::{Agent, LABEL, Layout, Request},
        installed_app::{app_executable, link_command},
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
pub(crate) fn quietly_attended(mut child: std::process::Child, why: &str) -> Result<()> {
    let output = child.stdout.take().map(|mut stream| {
        std::thread::spawn(move || {
            use std::io::Read;
            let mut bytes = Vec::new();
            stream.read_to_end(&mut bytes).map(|_| bytes)
        })
    });

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
    let output = output
        .and_then(|reader| reader.join().ok())
        .transpose()?
        .unwrap_or_default();
    if status.success() {
        return Ok(());
    }

    let _ = terminal.write_all(&dropped);
    let _ = terminal.write_all(&pending);
    let _ = terminal.write_all(&output);

    Err(problem(why.to_owned()))
}

/// A command with cached administrator rights (`sudo -n`), never prompting.
fn sudo(args: &[&str]) -> Result<std::process::Output> {
    Ok(Command::new("/usr/bin/sudo")
        .arg("-n")
        .args(args)
        .output()?)
}

/// Whether `name` can be a new account's short name.
pub(crate) fn valid_account_name(name: &str) -> std::result::Result<(), String> {
    if name.chars().any(char::is_whitespace) {
        return Err("Account names can't have spaces.".into());
    }

    if !name
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    {
        return Err(
            "Account names can only use letters, numbers, dots, dashes and underscores.".into(),
        );
    }

    if name.starts_with(['.', '-']) {
        return Err("Account names start with a letter or a number.".into());
    }

    if name.is_empty() || name.len() > 31 {
        return Err("Account names are 1 to 31 characters.".into());
    }

    // macOS reads a name of only digits as a user ID.
    if name.bytes().all(|c| c.is_ascii_digit()) {
        return Err("Account names need at least one letter.".into());
    }

    Ok(())
}

/// Choose an existing standard account, or (only when asked for, and
/// confirmed) create one.
fn choose_helper() -> Result<(setup::Account, setup::identity::Identity)> {
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
        return selected(
            setup::account_named(&account.name)?
                .ok_or_else(|| problem("That account disappeared"))?,
        );
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
    let orphans = crate::session::sessions(None)
        .map(|sessions| sessions.leftover_uids(crate::session::Kind::Orphan))
        .ok_or_else(|| problem("macOS could not list its logins, so no account was made"))?;
    let uid = free_uid()?;
    crate::onboarding::closing_first(uid, &orphans, || {
        crate::onboarding::as_root::create_in_terminal(&name, uid, FULL_NAME, &orphans)
    })?
    .map_err(|refusal| {
        problem(format!(
            "The helper account could not be created: {refusal:?}"
        ))
    })?;

    selected(setup::account_named(&name)?.ok_or_else(|| problem("The new account was not found"))?)
}

/// Capture the account and its permanent identity at selection time.
fn selected(helper: setup::Account) -> Result<(setup::Account, setup::identity::Identity)> {
    let identity = setup::identity::current(&helper)
        .ok_or_else(|| problem("macOS could not identify the selected helper account"))?;
    Ok((helper, identity))
}

/// Keep the selected account through configuration, refusing UID reassignment
/// before recording it. The recorder also checks its permanent identity.
fn record_selected(
    helper: &setup::Account,
    expected: &setup::identity::Identity,
    lookup: impl FnOnce(u32) -> Result<Option<setup::Account>>,
    record: impl FnOnce(&setup::Account, &setup::identity::Identity) -> Result<()>,
) -> Result<()> {
    if lookup(helper.uid)?.as_ref() != Some(helper) {
        return Err(problem(
            "The chosen helper account changed; choose it again",
        ));
    }
    record(helper, expected)
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
        let output =
            crate::bounded::output(Command::new(program).args(args), crate::bounded::LISTING)?;
        if !output.status.success() {
            return Err(problem("Could not list the user IDs in use"));
        }
        used_uids(
            &String::from_utf8(output.stdout)
                .map_err(|_| problem("Could not read the user IDs in use"))?,
        )
    };

    let accounts = taken("/usr/bin/dscl", &["/Search", "-list", "/Users", "UniqueID"])?;
    let running = taken("/bin/ps", &["-axo", "uid="])?;
    available_uid(&accounts, &running, |uid| {
        Ok(setup::account_with_uid(uid)?.is_some())
    })
}

/// Directory services can list negative system IDs, which cannot collide
/// with the helper. Any unreadable ID still makes the whole list unusable.
fn used_uids(listing: &str) -> Result<Vec<u32>> {
    let mut uids = Vec::new();
    for line in listing.lines() {
        let unreadable = || problem("Could not read a user ID in use");
        let uid = line
            .split_whitespace()
            .last()
            .and_then(|word| word.parse::<i64>().ok())
            .ok_or_else(unreadable)?;
        if uid >= 0 {
            uids.push(u32::try_from(uid).map_err(|_| unreadable())?);
        }
    }
    Ok(uids)
}

fn available_uid(
    accounts: &[u32],
    running: &[u32],
    mut resolves: impl FnMut(u32) -> Result<bool>,
) -> Result<u32> {
    for uid in setup::FIRST_USER_UID..600 {
        if !accounts.contains(&uid) && !running.contains(&uid) && !resolves(uid)? {
            return Ok(uid);
        }
    }
    Err(problem("No free user ID for the helper account"))
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
        guard: None,
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
fn as_helper(
    executable: &Path,
    helper: &setup::Account,
    request: &Request,
    expected: &setup::identity::Identity,
) -> Result<Value> {
    let (succeeded, reply) = setup::request_as_helper(executable, helper, request, true, expected)?;
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
    agent: Agent,
    layout: &Layout,
    run: impl Fn(&Request) -> Result<Value>,
) -> Result<()> {
    let _exclusion = client.controller.acquire(true)?;
    let before = client.status().await["daemon"]["pid"].clone();
    let pending = client.controller.quarantine()?;

    {
        let _executors = crate::control::ExecutorClaims::new(std::slice::from_ref(&pending))?;
        install_job(run, helper.uid, agent, &layout.logs()).map_err(Error::uncertain)?;
    }

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
    expected: &setup::identity::Identity,
) -> Result<()> {
    let (agent, layout) = helper_agent(helper, executable)?;
    let expected_hash = setup::helper::text_sha256(&setup::helper::plist(&agent, &layout.logs()));
    let job = setup::migrate::managed_job(client).await?;

    if outdated_job(&job, &expected_hash) {
        administrator_rights(
            "  macOS asks for your password once, to replace the helper's background job with \
             this version's.",
        )?;

        let _spinner = Spinner::new("Replacing the helper's background job…");
        replace_job(client, helper, agent, &layout, |request| {
            as_helper(executable, helper, request, expected)
        })
        .await?;
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
                _ => skipped(
                    "Menu bar",
                    "turn on Open Atlas at login in Atlas › Settings › General later",
                ),
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
    // The Claude app is offered beside them when it is here and not yet
    // connected.
    let app = agents["agents"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|agent| {
            agent["agent"] == crate::claude_app::ID
                && agent["installed"] == true
                && agent["connected"] == false
        });
    let mut names: Vec<&str> = found.iter().map(|agent| agent.display_name()).collect();
    if app {
        names.push("the Claude app");
    }
    if !names.is_empty()
        && prompt::confirm(
            &format!("Teach {} to use Atlas? (adds a plugin)", listed(&names)),
            true,
            "run atlas agents install later",
        )?
    {
        // Setup is already done; this step can only be skipped, never fail it.
        let connected = {
            let _spinner = Spinner::new("Connecting…");
            crate::agents::change_these(
                &found,
                crate::agents::Covering {
                    every_agent: false,
                    app,
                },
                crate::agents::Change::Connect,
            )
            .unwrap_or_else(|error| json!({"ok": false, "error": error.error}))
        };
        if connected["ok"] == true {
            done(
                "Agents",
                &format!("{} connected", crate::doing::capitalized(&listed(&names))),
            );
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

/// Names as a sentence lists them: "A", "A and B", "A, B and C".
fn listed(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The closing lines: ready, and what to try.
fn ready() {
    say(&format!("\n{}", style::bold("Atlas is ready.")));
    for line in ui::suggestions() {
        say(&line);
    }
}

/// Lock the screen for the person, if they want, so they can switch to
/// the helper `name` (user ID `uid`); otherwise say how. Refused with
/// `not-ready`, and nothing locked, while a new login could hang.
fn offer_lock(name: &str, uid: u32) -> Result<()> {
    let account = crate::switch::Account {
        name: name.to_owned(),
        uid,
        real_name: None,
        helper: true,
    };
    // Checked before asking: a new login waits while another could hang it.
    crate::switch::check(&account)?;

    if prompt::confirm(
        &format!("Lock the screen now so you can switch to {name}?"),
        true,
        "switch accounts yourself",
    )? {
        match crate::switch::to(&account) {
            Ok(_) => {
                let _ = LOCKED.set(std::time::Instant::now());
                done(
                    "Screen locked",
                    &format!("click Switch User and choose {name}"),
                );
            }
            Err(error) if error.code == "not-ready" => return Err(error),
            Err(error) => skipped("Screen not locked", &error.error),
        }
    } else {
        say(&format!(
            "  To switch: Apple menu › Lock Screen, then Switch User and choose {name}."
        ));
    }

    Ok(())
}

/// When init locked the screen for the person to switch to the helper.
static LOCKED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

/// Note in the login timing that the helper reached `step` (`answering`,
/// `hidden`) this long after init locked the screen for it.
fn mark_since_lock(step: &str) {
    if let Some(locked) = LOCKED.get() {
        crate::login_log::mark(&format!(
            "helper.{step} after={}",
            crate::login_log::seconds(locked.elapsed())
        ));
    }
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
    let (config, helper, expected) = helper_step(config_path)?;
    let name = helper.name.clone();
    // Whether an earlier run already finished setup (decides the first-run steps).
    let settled_before = setup::migrate::settled(helper.uid);
    let client = Client::new(config.clone())?;

    client.bootstrap_probe()?;

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
        start_helper(
            &client,
            &config,
            &helper,
            &executable,
            &mut done_steps,
            &expected,
        )
        .await?;
    }

    // The probe installed above is ready for the session checks below.
    permissions_step(&client, &config, &name).await?;
    current_daemon_step(&client, &helper, &executable, &mut done_steps, &expected).await?;
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
/// created) when there are none or their account is gone, retaining its
/// permanent identity throughout configuration and setup.
fn helper_step(config_path: &Path) -> Result<(Config, setup::Account, setup::identity::Identity)> {
    // The path is the one the command line resolved (the installed one by
    // default); a configuration is created only where none exists, and an
    // existing one that does not load is reported, never replaced.
    let absent = matches!(
        fs::symlink_metadata(config_path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    );
    // Whether the person chose the helper in this run: only then may its
    // record replace one of another account.
    let mut choice = None;
    let mut config = if absent {
        let (helper, identity) = choose_helper()?;
        let config = proposed(&helper)?;
        choice = Some((helper, identity));
        write_config(config_path, &config)?;
        config
    } else {
        Config::load(config_path)?
    };

    // The account the settings name was deleted, or its user ID now belongs
    // to another account than the helper Atlas recorded: choose (or create)
    // another, before anything is changed or recorded.
    let account = setup::account_with_uid(config.helper_uid)?;
    let gone = account.is_none();
    let other = match &account {
        Some(account) if choice.is_none() => replaced(account)?,
        _ => false,
    };
    if choice.is_none() && (gone || other) {
        say(&format!(
            "  {} (user ID {}) {}.",
            if gone {
                format!("The helper account in {}", text::tilde(config_path))
            } else {
                "The account".to_owned()
            },
            config.helper_uid,
            if gone {
                "no longer exists"
            } else {
                "is not the helper Atlas set up"
            }
        ));

        let (helper, identity) = choose_helper()?;
        let replacement = proposed(&helper)?;
        replace_config(config_path, &config, &replacement)?;
        config = replacement;
        choice = Some((helper, identity));
    }

    let chosen = choice.is_some();
    let (helper, identity) = match choice {
        Some(selected) => selected,
        None => {
            let helper = account.ok_or_else(|| problem("The helper account was not created"))?;
            let identity = match setup::identity::standing_of(&helper) {
                setup::identity::Standing::Same(identity) => identity,
                _ => {
                    return Err(problem(
                        "Choose the helper account again before setting it up",
                    ));
                }
            };
            (helper, identity)
        }
    };
    record_selected(
        &helper,
        &identity,
        setup::account_with_uid,
        |helper, identity| setup::identity::remember_as(helper, identity, chosen),
    )?;

    // Keep the socket paths in the selected account's home.
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

    // Which account this is, so nothing later acts on another account that
    // is given its user ID. Unrecorded, nothing later ends a login under
    // its user ID or deletes it.
    setup::identity::remember_as(&helper, &identity, chosen)?;

    // An account deleted while logged in leaves its login running, which
    // can stall the next login, or be mistaken for the helper's.
    close_leftovers(helper.uid, Some(&identity))?;

    done(
        "Helper account",
        &format!("{name} · settings in {}", text::tilde(config_path)),
    );

    if setup::migrate::archive_foreign(helper.uid, &helper.home)? {
        say("  Set aside a setup record left by an earlier account with the same ID.");
    }

    Ok((config, helper, identity))
}

/// Whether the account with the helper's user ID is not the helper Atlas
/// recorded: another account took the user ID. A record that cannot be
/// compared stops init before anything changes.
fn replaced(account: &setup::Account) -> Result<bool> {
    use setup::identity::{OnFile, Standing};

    match setup::identity::standing_of(account) {
        Standing::Replaced | Standing::Unrecorded(_) => Ok(true),
        Standing::Unknown if setup::identity::on_file(account.uid) != OnFile::Nothing => {
            Err(problem(format!(
                "macOS could not say whether {} is still the helper Atlas set up; run atlas init \
                 again",
                account.name
            )))
        }
        Standing::Same(_) | Standing::Unknown => Ok(false),
    }
}

/// Close the logins deleted accounts left open, once the person agrees:
/// under sudo, which asks for their password here, all in one run as root.
/// Root checks each user ID again itself; only a login the helper's user ID
/// (`uid`, its account as just recorded: `helper`) was given after is closed
/// while its account exists, and then without killing anything.
fn close_leftovers(uid: u32, helper: Option<&crate::setup::identity::Identity>) -> Result<()> {
    use crate::onboarding;

    let Some(sessions) = crate::session::sessions(Some(uid)) else {
        return Ok(());
    };
    let still_open =
        || problem("It's still open. Close it before you switch: run atlas init again.");
    let failed = |refusal| match refusal {
        crate::approval::Refusal::Failed(reason) => problem(format!(
            "macOS didn't close it: {reason}. Restart the Mac to clear it, then run atlas init again."
        )),
        _ => still_open(),
    };
    let targets = onboarding::leftover_targets(&sessions, helper).map_err(failed)?;
    close_targets(&targets)
}

/// Close the logins `targets`, once the person agrees: under sudo, which
/// asks for their password here, all in one run as root under control of
/// every user ID it touches, recorded as pending until they are seen gone.
fn close_targets(targets: &[crate::onboarding::as_root::Target]) -> Result<()> {
    use crate::onboarding::{self, as_root::Via};

    if targets.is_empty() {
        return Ok(());
    }
    let still_open =
        || problem("It's still open. Close it before you switch: run atlas init again.");
    let failed = |refusal| match refusal {
        crate::approval::Refusal::Failed(reason) => problem(format!(
            "macOS didn't close it: {reason}. Restart the Mac to clear it, then run atlas init again."
        )),
        _ => still_open(),
    };

    heading("A deleted account is still logged in");
    say(
        "  macOS kept a login open for an account that was deleted. Close it before you go on,\n  \
         or the next login can stall.",
    );
    if !prompt::confirm("Close it now? macOS asks for your password.", true, "")? {
        return Err(still_open());
    }

    attended(
        "/usr/bin/sudo",
        &["-v"],
        "Administrator rights were not granted",
    )?;
    keep_sudo_alive();

    let uids: Vec<u32> = targets.iter().map(|target| target.uid()).collect();
    onboarding::close_batch(
        &onboarding::controllers_of(&uids)?,
        targets,
        |targets| crate::onboarding::as_root::close(targets, Via::Sudo { ask: false }),
        crate::session::confirm_gone,
    )
    .map_err(|held| match held {
        onboarding::Held::State(error) => error,
        onboarding::Held::Refused(refusal) => failed(refusal),
    })?;

    done("Closed", "the login the deleted account left open");
    Ok(())
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
    expected: &setup::identity::Identity,
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
        // A helper never set up skips macOS's setup questions at its first
        // login. Only a convenience: a refusal leaves them to the person,
        // and the step says what came of it.
        if !installed {
            let request = Request::FirstLogin {
                uid: helper.uid,
                opener: false,
            };
            let step = match setup_transaction(&client.controller, || {
                let (succeeded, reply) =
                    setup::request_before_login(executable, helper, &request, expected)?;
                if !succeeded || reply["ok"] != true {
                    return Err(
                        problem("The helper did not acknowledge first-login setup").uncertain()
                    );
                }
                Ok(reply)
            }) {
                Ok(reply) => {
                    json!({"step": "firstLogin", "answered": reply["answered"]})
                }
                Err(error) => json!({"step": "firstLogin", "error": error.error}),
            };
            done_steps.push(step);
        }

        heading(&format!("Log in to {name} once"));

        if installed {
            say(&format!(
                "  {name} is logged out (after a restart or a log-out). Switch to {name}, log in,\n  \
                     then switch back here."
            ));
        } else {
            say(&format!(
                "  Switch to {name} and log in. If macOS asks you to set anything up, choose\n  \
                     Set Up Later. If it asks about Accessibility or Screen Recording for Atlas,\n  \
                     turn Atlas on. Then switch back here."
            ));
        }

        offer_lock(name, helper.uid)?;

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
        if started {
            mark_since_lock("answering");
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
        let (agent, layout) = helper_agent(helper, executable)?;

        prepare_and_install(
            &client.controller,
            &controller.name,
            agent,
            &layout.logs(),
            |request| as_helper(executable, helper, request, expected),
        )?;

        spinner.set("Starting the helper…");
        let answered =
            daemon_answers_within(client, config.helper_uid, Duration::from_secs(1)).await;
        drop(spinner);
        if answered {
            mark_since_lock("answering");
        }

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

/// Hold and claim synchronous setup before any worker can change the helper.
/// Only a complete acknowledgement settles this transaction's generation.
pub(crate) fn setup_transaction<T>(
    controller: &crate::control::Controller,
    act: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let _ownership = controller.acquire(true)?;
    let pending = controller.quarantine()?;
    let _executors = crate::control::ExecutorClaims::new(std::slice::from_ref(&pending))?;
    let outcome = act();
    if outcome.is_ok() {
        controller.clear_if(&pending)?;
    }
    outcome
}

/// Prepare and install under one claim, including rollback dispatches.
fn prepare_and_install(
    controller: &crate::control::Controller,
    controller_name: &str,
    agent: Agent,
    logs: &Path,
    run: impl Fn(&Request) -> Result<Value>,
) -> Result<Value> {
    setup_transaction(controller, || {
        run(&Request::Prepare {
            uid: agent.uid,
            controller: controller_name.to_owned(),
        })?;
        install_job(run, agent.uid, agent, logs)
    })
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
        mark_since_lock("hidden");
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
                offer_lock(name, config.helper_uid)?;
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
    #[test]
    fn uid_lists_ignore_negative_system_ids_and_keep_nonnegative_ids() {
        let accounts = super::used_uids(
            "nobody                  -2\ndaemon                   1\n_unknown                99\nme                     501\n",
        )
        .unwrap();
        assert_eq!(accounts, [1, 99, 501]);
        let running = super::used_uids("   -2\n     1\n    99\n   502\n").unwrap();
        assert_eq!(running, [1, 99, 502]);
        assert_eq!(
            super::available_uid(&accounts, &running, |_| Ok(false)).unwrap(),
            503
        );
        assert_eq!(
            super::used_uids("root 0\nhigh 4294967295\n").unwrap(),
            [0, u32::MAX]
        );
    }

    #[test]
    fn uid_lists_refuse_garbage_missing_ids_and_out_of_range_numbers() {
        for line in [
            "garbage",
            "broken NaN",
            "",
            "large 4294967296",
            "huge 9223372036854775808",
        ] {
            let listing = format!("nobody -2\ndaemon 1\n_unknown 99\n{line}\nme 501\n");
            let error = super::used_uids(&listing).unwrap_err();
            assert_eq!(
                error.to_string(),
                "Could not read a user ID in use",
                "{line}"
            );
        }
    }

    #[test]
    fn uid_allocation_skips_directory_accounts_and_processes_and_refuses_lookup_errors() {
        let uid = super::available_uid(&[501], &[502], |uid| Ok(uid == 503)).unwrap();
        assert_eq!(uid, 504);
        let error = super::available_uid(&[], &[], |_| Err(super::problem("lookup failed")));
        assert!(error.is_err());
    }

    #[test]
    fn replacing_a_job_keeps_its_executor_claim_after_the_caller_dies() {
        if !crate::test_support::isolated(
            "init::tests::replacing_a_job_keeps_its_executor_claim_after_the_caller_dies",
        ) {
            return;
        }
        for mode in ["retire", "install", "restore"] {
            crate::test_support::interrupted_executor(
                "init::tests::a_job_installation_test_caller",
                mode,
            );
        }
    }

    #[test]
    fn preparing_and_installing_a_helper_excludes_teardown_after_the_caller_dies() {
        if !crate::test_support::isolated(
            "init::tests::preparing_and_installing_a_helper_excludes_teardown_after_the_caller_dies",
        ) {
            return;
        }
        for mode in ["prepare", "fresh-install"] {
            crate::test_support::interrupted_executor(
                "init::tests::a_job_installation_test_caller",
                mode,
            );
        }
    }

    /// A subprocess entry point; all effects stay in its explicit temporary directory.
    #[tokio::test]
    async fn a_job_installation_test_caller() {
        let Some(directory) = crate::test_support::directory() else {
            return;
        };
        let client = crate::test_support::client(&directory);
        let helper = setup::Account {
            uid: 7001,
            name: "helper".into(),
            home: directory.join("home"),
        };
        let layout = Layout {
            uid: helper.uid,
            home: helper.home.clone(),
            guard: None,
        };
        let agent = Agent {
            label: LABEL.into(),
            program: directory.join("atlas"),
            uid: helper.uid,
            controller_uid: nix::unistd::getuid().as_raw(),
            socket: layout.socket(),
        };
        let mode = std::env::var("ATLAS_MUTATION_TEST_MODE").unwrap();
        let run = |request: &Request| {
            let pause = matches!(
                (mode.as_str(), request),
                ("retire", Request::Retire { .. })
                    | ("install" | "fresh-install", Request::Install { .. })
                    | ("restore", Request::Restore { .. })
                    | ("prepare", Request::Prepare { .. })
            );
            if pause {
                crate::test_support::pause_executor(&directory);
            }
            if mode == "restore" && matches!(request, Request::Install { .. }) {
                return Err(problem("installation was refused"));
            }
            Ok(json!({"ok": true, "jobs": {LABEL: {"plistSha256": "b".repeat(64)}}}))
        };
        if matches!(mode.as_str(), "prepare" | "fresh-install") {
            let _ =
                prepare_and_install(&client.controller, "controller", agent, &layout.logs(), run);
        } else {
            let _ = replace_job(&client, &helper, agent, &layout, run).await;
        }
    }

    #[test]
    fn installation_acknowledgements_settle_only_their_own_setup_mutations() {
        if !crate::test_support::isolated(
            "init::tests::installation_acknowledgements_settle_only_their_own_setup_mutations",
        ) {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let client = crate::test_support::client(directory.path());
        let controller = &client.controller;
        let other = controller.quarantine().unwrap();
        drop(other);
        setup_transaction(controller, || Ok(())).unwrap();
        assert_eq!(controller.pending().unwrap().len(), 1);
        let outcome: Result<()> =
            setup_transaction(controller, || Err(problem("reply lost").uncertain()));
        assert!(outcome.is_err());
        assert_eq!(controller.pending().unwrap().len(), 2);
    }
    use super::*;

    #[test]
    fn a_selected_account_is_never_replaced_while_its_configuration_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let helper = setup::Account {
            uid: 7001,
            name: "selected".into(),
            home: dir.path().join("selected"),
        };
        let expected = setup::identity::Identity {
            uid: helper.uid,
            name: helper.name.clone(),
            generated_uid: "6DB4B7AF-BE3C-486C-BD46-1B4558946919".into(),
        };
        let configuration = proposed(&helper).unwrap();
        write_config(&dir.path().join("atlas.toml"), &configuration).unwrap();
        let replacement = setup::Account {
            name: "replacement".into(),
            home: dir.path().join("replacement"),
            ..helper.clone()
        };
        let recorded = std::cell::RefCell::new(None);
        let record = |_: &setup::Account, identity: &setup::identity::Identity| {
            recorded.replace(Some(identity.clone()));
            Ok(())
        };
        let outcome = record_selected(&helper, &expected, |_| Ok(Some(replacement)), record);
        assert!(
            outcome.is_err(),
            "a reused UID must require another explicit selection"
        );
        assert!(
            recorded.borrow().is_none(),
            "a replacement must never be recorded"
        );
        record_selected(&helper, &expected, |_| Ok(Some(helper.clone())), record).unwrap();
        assert_eq!(recorded.into_inner(), Some(expected));
    }

    #[test]
    fn quiet_commands_hide_log_lines_and_keep_their_outcome() {
        let script = "echo '2026-10-11 14:28:19.767 sysadminctl[1:2] noise' >&2; \
                      printf 'User password:' >&2; sleep 0.4; echo >&2; exit $0";
        let run = |status| {
            Command::new("/bin/sh")
                .args(["-c", script, status])
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap()
        };
        assert!(quietly_attended(run("0"), "failed").is_ok());

        let failed = quietly_attended(run("3"), "it failed").unwrap_err();
        assert_eq!(failed.error, "it failed");
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
    fn new_account_names_are_short_and_plain() {
        assert!(valid_account_name("atlas").is_ok());
        assert!(valid_account_name("qa_helper-2").is_ok());
        assert!(valid_account_name("").is_err());
        assert!(valid_account_name("has space").is_err());
        assert!(valid_account_name("-dash").is_err());
        assert!(
            valid_account_name("503").is_err(),
            "macOS reads digits alone as a user ID"
        );
        assert!(valid_account_name("503a").is_ok());
        assert!(valid_account_name(&"x".repeat(32)).is_err());
    }
}
