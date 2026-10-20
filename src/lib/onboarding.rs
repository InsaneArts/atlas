//! The steps the Atlas window's onboarding drives, as `atlas onboarding …`
//! commands that answer in JSON. They share `atlas init`'s own helpers, so
//! the terminal and the window set Atlas up the same way, and each step
//! checks before it acts.
//!
//! In the person's account the window chooses or creates the helper and
//! finishes; in the helper's account (Atlas opened there once) it installs
//! the helper's background job as that account itself, with no
//! administrator rights, once that account allows the one shown. Passwords
//! are never stored, logged or put in a command line: the one Atlas handles
//! (a new account's) goes from the window, through a pipe, straight into
//! macOS's own password prompt.
use crate::{
    client::Client,
    config::{self, Config},
    error::{Error, Result},
    init,
    setup::{
        self,
        helper::{Agent, LABEL, Layout, Request},
    },
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Where the person's account names the helper it chose, for Atlas in the
/// helper to find. It holds no secret; the helper's owner confirms it.
pub const REQUEST: &str = "/Users/Shared/Atlas/onboarding.json";

/// The onboarding window in the helper, while it is open: the helper's
/// daemon then leaves telling the person to the window.
pub const WINDOW_MARKER: &str = "onboarding-window";

/// How long creating an account may wait for the person to approve it.
const APPROVAL: Duration = Duration::from_secs(10 * 60);

fn problem(message: impl Into<String>) -> Error {
    Error::new("onboarding", message)
}

/// An account (the controller) asking another one to be its helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pairing {
    pub controller_uid: u32,
    pub controller_name: String,
    pub helper_uid: u32,
    pub helper_name: String,
}

/// Read the shared request, trusting it only when it is a plain file the
/// controller it names owns, in a folder no other account can change, and
/// both accounts it names are still the ones with those user IDs.
pub fn request() -> Option<Pairing> {
    read_request(Path::new(REQUEST)).filter(|pairing| {
        let named = |uid: u32, name: &str| {
            setup::account_with_uid(uid)
                .ok()
                .flatten()
                .is_some_and(|account| account.name == name)
        };
        named(pairing.controller_uid, &pairing.controller_name)
            && named(pairing.helper_uid, &pairing.helper_name)
    })
}

fn read_request(path: &Path) -> Option<Pairing> {
    let folder = fs::symlink_metadata(path.parent()?).ok()?;

    // Opened without following a link, and checked as opened.
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.mode() & 0o022 != 0 {
        return None;
    }

    let mut body = Vec::new();
    (&mut file).take(64 * 1024).read_to_end(&mut body).ok()?;
    let value: Value = serde_json::from_slice(&body).ok()?;
    let request = Pairing {
        controller_uid: value["controllerUid"].as_u64()?.try_into().ok()?,
        controller_name: value["controllerName"].as_str()?.to_owned(),
        helper_uid: value["helperUid"].as_u64()?.try_into().ok()?,
        helper_name: value["helperName"].as_str()?.to_owned(),
    };
    let folder_safe = folder.is_dir()
        && (folder.uid() == request.controller_uid || folder.uid() == 0)
        && folder.mode() & 0o022 == 0;

    (folder_safe && metadata.uid() == request.controller_uid).then_some(request)
}

/// Write the request, readable by every account and changed only by this one.
fn write_request(path: &Path, request: &Pairing) -> Result<()> {
    let body = json!({
        "controllerUid": request.controller_uid,
        "controllerName": request.controller_name,
        "helperUid": request.helper_uid,
        "helperName": request.helper_name,
    });

    config::write_atomic(path, serde_json::to_string_pretty(&body)?.as_bytes(), 0o644)
}

/// This account's user ID, short name and full name.
fn me() -> Result<(u32, String, Option<String>)> {
    let uid = nix::unistd::getuid().as_raw();
    let account =
        setup::account_with_uid(uid)?.ok_or_else(|| problem("This account cannot be resolved"))?;
    let real = setup::real_name(&account.name);

    Ok((uid, account.name, real))
}

/// This account's home as directory services record it, not `$HOME` (see
/// [`config::home`]): the helper's layout and launchd job live there, and
/// the window that runs these steps may have any environment.
fn home() -> Result<PathBuf> {
    let uid = nix::unistd::getuid().as_raw();
    Ok(setup::account_with_uid(uid)?
        .ok_or_else(|| problem("This account cannot be resolved"))?
        .home)
}

/// Whether Atlas is set up in this account as someone's helper already
/// (its background job is installed here, as `atlas init` leaves it).
fn helper_job_here(uid: u32) -> bool {
    home().is_ok_and(|home| Layout { uid, home }.agent_plist(LABEL).exists())
}

/// The app this runs from, whether it is signed by Atlas's developer, and
/// whether `atlas` in a terminal reaches it.
fn app() -> Value {
    match init::app_executable() {
        Ok(executable) => {
            let link = config::home()
                .map(|home| home.join(".local/bin/atlas"))
                .ok();
            let linked = link
                .and_then(|link| fs::canonicalize(link).ok())
                .is_some_and(|target| target == executable);

            json!({
                "installed": true,
                "path": executable.parent().and_then(Path::parent).and_then(Path::parent),
                "signed": setup::developer_id(&executable),
                "linked": linked,
            })
        }
        Err(error) => json!({"installed": false, "error": error.error}),
    }
}

/// Everything the window needs to choose its step, read fresh each time.
pub async fn status(config_path: &Path) -> Result<Value> {
    let (uid, name, real) = me()?;
    let me = json!({"uid": uid, "name": name, "realName": real});
    let app = app();
    let absent = matches!(
        fs::symlink_metadata(config_path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    );

    // Atlas opened in an account another one chose as its helper (an
    // account with Atlas settings of its own is a controller, whatever a
    // shared file says).
    if absent {
        let pairing = request().filter(|pairing| pairing.helper_uid == uid);
        if pairing.is_some() || helper_job_here(uid) {
            return Ok(json!({
                "ok": true,
                "role": "helper",
                "me": me,
                "app": app,
                "helper": helper_side(uid, pairing.as_ref()).await,
            }));
        }

        return Ok(json!({
            "ok": true,
            "role": "controller",
            "me": me,
            "app": app,
            "configured": false,
            "candidates": candidates()?,
            "menubarLogin": crate::menubar::login_decided(),
        }));
    }

    let config = Config::load(config_path)?;
    let Some(helper) = setup::account_with_uid(config.helper_uid)? else {
        return Ok(json!({
            "ok": true,
            "role": "controller",
            "me": me,
            "app": app,
            "configured": true,
            "helper": {"exists": false, "uid": config.helper_uid},
            "candidates": candidates()?,
            "menubarLogin": crate::menubar::login_decided(),
        }));
    };

    let admin = setup::administrator(&helper.name);
    let leftover = crate::session::leftover_session(helper.uid, &helper.name, &helper.home);

    // This account's request names this helper (set up by the window), or
    // the helper runs already (as after `atlas init`).
    let paired = request()
        .is_some_and(|pairing| pairing.controller_uid == uid && pairing.helper_uid == helper.uid);
    let signed_in = crate::session::logged_in(helper.uid) == Some(true);

    let client = Client::new(config.clone())?;
    let reply = client.status().await;
    let hidden = crate::session::require_hidden(&reply, config.helper_uid).is_ok();
    let answering = hidden || reply["daemon"]["pid"].is_number();
    let ready = crate::capability::doctor(reply.clone(), config.helper_uid)["ready"] == true;
    let finished =
        ready && setup::migrate::settled(helper.uid) && crate::session::input_build_current(&reply);

    // Not asked yet (a choice cut short, or an interrupted `atlas init`):
    // the window offers the choice again.
    let candidates = if !paired && !answering {
        candidates()?
    } else {
        Vec::new()
    };

    Ok(json!({
        "ok": true,
        "role": "controller",
        "me": me,
        "app": app,
        "configured": true,
        "candidates": candidates,
        "helper": {
            "exists": true,
            "uid": helper.uid,
            "name": helper.name,
            "realName": setup::real_name(&helper.name),
            "administrator": admin,
            "leftoverSession": leftover,
            "paired": paired,
            "signedIn": signed_in,
            "answering": answering,
            "onConsole": reply["session"]["onConsole"] == true,
            "hidden": hidden,
            "permissions": {
                "screenCapture": reply["permissions"]["screenCapture"] == true,
                "accessibility": reply["permissions"]["accessibility"] == true,
            },
        },
        "finished": finished,
        "menubarLogin": crate::menubar::login_decided(),
    }))
}

/// The helper's side of the status: who asked (unknown when `atlas init`
/// set it up), whether its job runs here, and its permissions as that job,
/// the process that uses them, reports them.
async fn helper_side(uid: u32, pairing: Option<&Pairing>) -> Value {
    let layout = home().map(|home| Layout { uid, home });

    // Installed for the account asking now: a job left by an earlier Atlas
    // (another app, another account) needs Allow again.
    let installed = layout.as_ref().is_ok_and(|layout| {
        let current = fs::read_to_string(layout.agent_plist(LABEL)).ok();
        match (current, pairing) {
            (None, _) => false,
            (Some(_), None) => true,
            (Some(text), Some(pairing)) => expected_agent(layout, pairing.controller_uid)
                .is_ok_and(|agent| text == setup::helper::plist(&agent, &layout.logs())),
        }
    });
    let running = layout
        .as_ref()
        .is_ok_and(|layout| setup::helper::job_pid(layout, LABEL).is_some());

    // macOS counts a process the window starts as the window's own, so only
    // the job's answer tells what Atlas itself may do here.
    let mut permissions = json!({"screenCapture": false, "accessibility": false});
    if let (true, Ok(layout)) = (running, &layout) {
        let transport = crate::transport::Transport {
            path: layout.socket(),
            timeout: Duration::from_secs(3),
            peer_uid: uid,
        };
        if let Ok(hello) = transport.call(&json!({"op": "hello"})).await {
            permissions = json!({
                "screenCapture": hello["permissions"]["screenCapture"] == true,
                "accessibility": hello["permissions"]["accessibility"] == true,
            });
        }
    }

    let controller = pairing.map_or(json!(null), |pairing| {
        json!({
            "uid": pairing.controller_uid,
            "name": pairing.controller_name,
            "realName": setup::real_name(&pairing.controller_name),
        })
    });

    json!({
        "controller": controller,
        "installed": installed,
        "running": running,
        "permissions": permissions,
    })
}

/// Standard accounts that could be the helper.
fn candidates() -> Result<Vec<Value>> {
    Ok(crate::switch::standard_accounts()?
        .into_iter()
        .map(|account| {
            json!({"name": account.name, "uid": account.uid, "realName": account.real_name})
        })
        .collect())
}

/// Make `name`, an existing standard account, the helper: the shared
/// executable, the request Atlas in the helper reads, and the settings.
/// Settings for another helper that still exists are never replaced.
pub fn choose(config_path: &Path, name: &str) -> Result<Value> {
    let helper = setup::account_named(name)?
        .ok_or_else(|| problem(format!("There is no account {name}")))?;
    let (uid, me_name, _) = me()?;
    if helper.uid == uid {
        return Err(problem("The helper must be another account than yours"));
    }

    match setup::administrator(&helper.name) {
        Some(false) => {}
        Some(true) => {
            return Err(problem(format!(
                "{name} administers this Mac; the helper must be a standard account"
            )));
        }
        None => {
            return Err(problem(format!(
                "macOS could not say whether {name} is an administrator"
            )));
        }
    }

    if crate::session::leftover_session(helper.uid, &helper.name, &helper.home) {
        return Err(problem(format!(
            "macOS still runs a session of a deleted account with {name}'s user ID. Restart \
             the Mac, then choose {name} again."
        )));
    }

    let executable = signed_executable()?;

    let previous = match Config::load(config_path) {
        Ok(previous) => Some(previous),
        Err(_) if !config_path.exists() => None,
        Err(error) => return Err(error),
    };

    let fresh = init::proposed(&helper)?;

    // As `atlas init` does: the settings for this same helper stay (only
    // their sockets follow its home); another existing helper is kept.
    let config = match &previous {
        Some(previous) if previous.helper_uid == helper.uid => {
            if previous.socket_path.starts_with(&helper.home) {
                previous.clone()
            } else {
                Config {
                    socket_path: fresh.socket_path.clone(),
                    input_socket: fresh.input_socket.clone(),
                    ..previous.clone()
                }
            }
        }
        Some(previous) => match setup::account_with_uid(previous.helper_uid)? {
            Some(other) => {
                return Err(problem(format!(
                    "Atlas already uses {} as its helper. Choose {} again, or remove Atlas \
                     with atlas uninstall first.",
                    other.name, other.name
                )));
            }
            None => fresh,
        },
        None => fresh,
    };

    let linked = init::link_command(&executable)?;

    // A setup record left by an earlier account with this user ID.
    setup::migrate::archive_foreign(helper.uid, &helper.home)?;

    // The shared executable (which also makes /Users/Shared/Atlas) and the
    // request first, the settings last: a choice cut short leaves no
    // settings that point at a helper that was never asked.
    Client::new(config.clone())?.install()?;

    write_request(
        Path::new(REQUEST),
        &Pairing {
            controller_uid: uid,
            controller_name: me_name,
            helper_uid: helper.uid,
            helper_name: helper.name.clone(),
        },
    )?;

    match previous {
        Some(previous) if previous == config => {}
        Some(previous) => init::replace_config(config_path, &previous, &config)?,
        None => init::write_config(config_path, &config)?,
    }

    Ok(json!({
        "ok": true,
        "helper": {"name": helper.name, "uid": helper.uid},
        "config": config_path,
        "command": linked,
    }))
}

/// Create a standard account. Its password arrives on standard input (from
/// the window) and is typed into sysadminctl's own prompt through a
/// pseudo-terminal, so it is never in a command line or a file; macOS asks
/// for an administrator's approval in its own dialog.
pub fn create(config_path: &Path, name: &str, full_name: &str) -> Result<Value> {
    init::valid_account_name(name).map_err(problem)?;

    if full_name.is_empty()
        || full_name.chars().count() > 64
        || full_name.chars().any(|c| c.is_control())
    {
        return Err(problem("Use a full name of 1 to 64 characters"));
    }

    if setup::account_named(name)?.is_some() {
        return Err(problem(format!("{name} already exists; choose it instead")));
    }

    let mut password = read_secret(std::io::stdin())?;
    let mut args = init::add_user_args(name, full_name, init::free_uid()?);
    args.push("interactive".to_owned());
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = answer_prompts(init::SYSADMINCTL, &args, &password, APPROVAL);
    password.fill(0);

    // sysadminctl reports success even when it did nothing: the account
    // itself is the answer.
    let created = setup::account_named(name)?;
    let Some(account) = created else {
        let detail = result.err().map(|error| error.error).unwrap_or_default();
        return Err(problem(if detail.is_empty() {
            "The account was not created. Approve the macOS dialog to create it.".to_owned()
        } else {
            format!("The account was not created: {detail}")
        }));
    };

    if setup::administrator(&account.name) != Some(false) {
        return Err(problem(format!(
            "{name} was created, but not as a standard account"
        )));
    }

    let chosen = choose(config_path, &account.name)?;
    Ok(json!({"ok": true, "created": {"name": account.name, "uid": account.uid}, "chosen": chosen}))
}

/// A password from standard input: one line of 1 to 256 bytes, with no
/// control characters (a terminal would edit the line with them, so macOS
/// would receive a different password than the one typed).
fn read_secret(input: impl Read) -> Result<Vec<u8>> {
    let mut secret = Vec::new();
    input.take(258).read_to_end(&mut secret)?;

    if secret.last() == Some(&b'\n') {
        secret.pop();
    }

    if secret.is_empty()
        || secret.len() > 256
        || secret.iter().any(|byte| *byte < 0x20 || *byte == 0x7F)
    {
        secret.fill(0);
        return Err(problem(
            "Give one password of 1 to 256 characters, without control characters",
        ));
    }

    Ok(secret)
}

/// Run `program` in a pseudo-terminal (`script`) and type `secret` each time
/// it asks for a password, waiting until it has asked so the answer is not
/// flushed away. The secret is cut out of anything kept from its output.
fn answer_prompts(
    program: &str,
    args: &[&str],
    secret: &[u8],
    patience: Duration,
) -> Result<String> {
    let mut child = Command::new("/usr/bin/script")
        .args(["-q", "/dev/null", program])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut input = child.stdin.take().ok_or_else(|| problem("No input pipe"))?;
    let output = child
        .stdout
        .take()
        .ok_or_else(|| problem("No output pipe"))?;
    let errors = child
        .stderr
        .take()
        .ok_or_else(|| problem("No error pipe"))?;

    let receiver = init::forward(output);
    // Drained as it comes, so a chatty program never blocks on it.
    let complaints = init::forward(errors);
    let deadline = Instant::now() + patience;
    let mut seen = Vec::new();
    let mut answered = 0;

    loop {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(bytes) => seen.extend(bytes),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }

        // A question ends without a newline: "User password:".
        let asked = seen
            .windows(PROMPT.len())
            .filter(|window| window.eq_ignore_ascii_case(PROMPT))
            .count();
        if asked > answered && answered < MAX_ANSWERS {
            // Let the prompt turn echo off before typing.
            std::thread::sleep(Duration::from_millis(150));

            let typed = input
                .write_all(secret)
                .and_then(|()| input.write_all(b"\n"))
                .and_then(|()| input.flush());
            if let Err(error) = typed {
                let _ = child.kill();
                let _ = child.wait();
                seen.fill(0);
                return Err(error.into());
            }

            answered += 1;
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            break;
        }
    }

    drop(input);
    let status = child.wait()?;

    // The secret is cut out byte for byte, never copied into a string.
    let kept = without(&seen, secret);
    seen.fill(0);
    let text = String::from_utf8_lossy(&kept).into_owned();

    if status.success() {
        return Ok(text);
    }

    let complained: Vec<u8> = complaints.try_iter().flatten().collect();
    let last = |text: &str| {
        text.lines()
            .map(str::trim)
            .rfind(|line| !line.is_empty() && !line.to_ascii_lowercase().contains("password"))
            .map(str::to_owned)
    };
    let summary = last(&text)
        .or_else(|| last(&String::from_utf8_lossy(&without(&complained, secret))))
        .unwrap_or_default();

    Err(problem(summary))
}

/// What a password question ends with, in any case.
const PROMPT: &[u8] = b"password:";

/// The most password questions answered, so a program that keeps asking
/// (after a refused password, say) is not fed the secret forever.
const MAX_ANSWERS: usize = 3;

/// `bytes` with every occurrence of `secret` cut out.
fn without(bytes: &[u8], secret: &[u8]) -> Vec<u8> {
    if secret.is_empty() {
        return bytes.to_vec();
    }

    let mut kept = Vec::with_capacity(bytes.len());
    let mut rest = bytes;
    while !rest.is_empty() {
        if rest.starts_with(secret) {
            rest = &rest[secret.len()..];
        } else {
            kept.push(rest[0]);
            rest = &rest[1..];
        }
    }

    kept
}

/// In the helper: let the account that asked use this one, by installing
/// Atlas's background job here as this account itself. Only for the
/// account the person was shown (`controller_uid`, `controller_name`): a
/// request changed meanwhile is refused.
pub fn accept(controller_uid: u32, controller_name: &str) -> Result<Value> {
    let (uid, my_name, _) = me()?;

    if config::installed_path().is_ok_and(|path| path.exists()) {
        return Err(problem(
            "This account has Atlas settings of its own, so it cannot be a helper",
        ));
    }

    let request = request()
        .filter(|request| request.helper_uid == uid && request.helper_name == my_name)
        .ok_or_else(|| problem("No account has chosen this one as its helper"))?;
    if request.controller_uid != controller_uid || request.controller_name != controller_name {
        return Err(problem(
            "The account asking changed since this window showed it; open Atlas again",
        ));
    }

    if setup::administrator(&my_name) != Some(false) {
        return Err(problem(
            "This account administers the Mac; a helper must be a standard account",
        ));
    }

    let controller = setup::account_with_uid(request.controller_uid)?
        .filter(|account| account.name == request.controller_name)
        .ok_or_else(|| problem("The account that asked no longer exists"))?;
    signed_executable()?;

    let layout = Layout { uid, home: home()? };
    let run = |request: &Request| -> Result<Value> {
        let reply = setup::helper::run(&serde_json::to_string(request)?)?;
        if reply["ok"] != true {
            return Err(problem(format!(
                "Setting up this account failed: {}",
                reply["error"].as_str().unwrap_or("no reply")
            )));
        }

        Ok(reply)
    };

    run(&Request::Prepare {
        uid,
        controller: controller.name.clone(),
    })?;

    let agent = expected_agent(&layout, controller.uid)?;
    let installed = init::install_job(run, uid, agent, &layout.logs())?;

    // Install waits for the job to load, not to run; a job launchd has not
    // started yet (or gave up on earlier) is started now.
    if installed["pid"].is_null() {
        let output = Command::new("/bin/launchctl")
            .args(["kickstart", &format!("gui/{uid}/{LABEL}")])
            .output()?;
        if !output.status.success() {
            return Err(problem(
                "Atlas was set up here but did not start; log out of this account, log in again and open Atlas",
            ));
        }
    }

    Ok(json!({"ok": true, "controller": controller.name, "installed": installed}))
}

/// This Atlas.app's executable, refused unless Atlas's developer signed it.
fn signed_executable() -> Result<PathBuf> {
    let executable = init::app_executable()?;
    if !setup::developer_id(&executable) {
        return Err(problem(
            "This Atlas.app is not signed by Atlas's developer; install it again",
        ));
    }

    Ok(executable)
}

/// The background job Atlas installs in this account for `controller_uid`.
fn expected_agent(layout: &Layout, controller_uid: u32) -> Result<Agent> {
    Ok(Agent {
        label: LABEL.to_owned(),
        program: init::app_executable()?,
        uid: layout.uid,
        controller_uid,
        socket: layout.socket(),
    })
}

/// Mark (`true`) or clear the onboarding window being open in this account.
pub fn window(open: bool) -> Result<Value> {
    let path = home()?.join(setup::helper::ROOT).join(WINDOW_MARKER);

    if open {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, format!("{}\n", std::os::unix::process::parent_id()))?;
    } else {
        let _ = fs::remove_file(&path);
    }

    Ok(json!({"ok": true, "open": open}))
}

/// Whether this account's onboarding window is open: the process it names
/// still lives and is Atlas's window (a reused process ID is not).
pub fn window_open(home: &Path) -> bool {
    let Some(pid) = fs::read_to_string(home.join(setup::helper::ROOT).join(WINDOW_MARKER))
        .ok()
        .and_then(|text| text.trim().parse::<i32>().ok())
        .filter(|pid| *pid > 1)
    else {
        return false;
    };

    if nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_err() {
        return false;
    }

    Command::new("/bin/ps")
        .args(["-o", "comm=", "-p", &pid.to_string()])
        .output()
        .is_ok_and(|output| {
            Path::new(String::from_utf8_lossy(&output.stdout).trim()).file_name()
                == Some(std::ffi::OsStr::new(crate::menubar::APP_EXECUTABLE))
        })
}

/// Back in the person's account: start input, record and check the setup
/// end to end, as `atlas init` does.
pub async fn finish(config_path: &Path) -> Result<Value> {
    let config = Config::load(config_path)?;
    let client = Client::new(config.clone())?;
    let status = client.status().await;
    crate::session::require_hidden(&status, config.helper_uid)?;

    if !init::missing(&status).is_empty() {
        return Err(problem(
            "Screen Recording or Accessibility is still off in the helper",
        ));
    }

    if !client.controller.pending()?.is_empty() {
        client.restart().await?;
    }

    if let Some(helper) = setup::account_with_uid(config.helper_uid)? {
        setup::migrate::archive_foreign(helper.uid, &helper.home)?;
    }

    client.install()?;
    client.start_input().await?;

    let ownership = client.controller.acquire(true)?;
    let recorded = setup::migrate::apply_fresh(config_path, &ownership).await?;
    drop(ownership);

    if recorded["ok"] != true {
        return Err(problem(format!(
            "Setup could not be verified: {}",
            recorded["error"].as_str().unwrap_or("unknown")
        )));
    }

    let doctor = crate::capability::doctor(client.status().await, config.helper_uid);
    if doctor["ready"] != true {
        return Err(problem(
            "Setup finished but is not ready; run atlas doctor to see why",
        ));
    }

    Ok(json!({"ok": true, "ready": true, "doctor": doctor}))
}

/// Open the menu bar at login, or not, as the person chose. As with `atlas
/// init`, only for the default settings, the ones the login item uses.
pub fn login(config_path: &Path, on: bool) -> Result<Value> {
    let same = |a: &Path, b: &Path| match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    };
    if on && !config::installed_path().is_ok_and(|default| same(&default, config_path)) {
        return Err(problem(format!(
            "Open at Login uses the default settings; start the menu bar with atlas menubar \
             --config {}",
            config_path.display()
        )));
    }

    crate::menubar::set_login(if on {
        crate::menubar::Login::On
    } else {
        crate::menubar::Login::Off
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_is_one_short_plain_line() {
        assert_eq!(read_secret(&b"hunter2\n"[..]).unwrap(), b"hunter2");
        assert_eq!(read_secret(&b"hunter2"[..]).unwrap(), b"hunter2");
        assert_eq!(
            read_secret("pässwörd\n".as_bytes()).unwrap(),
            "pässwörd".as_bytes()
        );
        assert!(read_secret(&b"\n"[..]).is_err());
        assert!(read_secret(&b"two\nlines\n"[..]).is_err());
        assert!(read_secret(&b"carriage\rreturn"[..]).is_err());
        assert!(read_secret(&b"del\x7fete"[..]).is_err());
        assert!(read_secret(&[b'x'; 300][..]).is_err());
    }

    /// The secret is typed only once asked, reaches a terminal prompt
    /// with echo off, and never appears in what is kept of the output.
    #[test]
    fn prompts_are_answered_through_a_terminal() {
        let script = "printf 'User password:'; stty -echo; read p; stty echo; echo; \
                      [ \"$p\" = 'se cret' ] && echo matched";
        let text = answer_prompts(
            "/bin/sh",
            &["-c", script],
            b"se cret",
            Duration::from_secs(10),
        )
        .unwrap();
        assert!(text.contains("matched"), "{text}");
        assert!(!text.contains("se cret"), "{text}");

        let wrong = "printf 'Password:'; read p; exit 3";
        let failed = answer_prompts("/bin/sh", &["-c", wrong], b"abc", Duration::from_secs(10));
        assert!(failed.is_err());
    }

    #[test]
    fn the_secret_is_cut_out_byte_for_byte() {
        assert_eq!(without(b"a se cret b se cret", b"se cret"), b"a  b ");
        assert_eq!(without(b"sesecret", b"secret"), b"se");
        assert_eq!(without(b"\xff\xfepass", b"\xfe"), b"\xffpass");
        assert_eq!(without(b"kept", b""), b"kept");
    }

    fn pairing(controller: u32) -> Pairing {
        Pairing {
            controller_uid: controller,
            controller_name: "person".into(),
            helper_uid: controller + 1,
            helper_name: "helper".into(),
        }
    }

    #[test]
    fn a_request_is_trusted_only_from_its_controller() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
        let path = dir.path().join("onboarding.json");
        let me = nix::unistd::getuid().as_raw();

        write_request(&path, &pairing(me)).unwrap();
        assert_eq!(read_request(&path), Some(pairing(me)));

        // Written by someone else than the controller it names: ignored.
        write_request(&path, &pairing(me + 7)).unwrap();
        assert_eq!(read_request(&path), None);

        // Writable by others: ignored.
        write_request(&path, &pairing(me)).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
        assert_eq!(read_request(&path), None);

        // In a folder others can change: ignored.
        write_request(&path, &pairing(me)).unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o777)).unwrap();
        assert_eq!(read_request(&path), None);
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();

        // A link to a request: ignored.
        let link = dir.path().join("link.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert_eq!(read_request(&link), None);
    }

    #[test]
    fn the_window_marker_names_a_living_window() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(setup::helper::ROOT);
        fs::create_dir_all(&root).unwrap();

        // A living process that is not Atlas's window does not count.
        fs::write(
            root.join(WINDOW_MARKER),
            format!("{}\n", std::process::id()),
        )
        .unwrap();
        assert!(!window_open(dir.path()));

        fs::write(root.join(WINDOW_MARKER), "999999\n").unwrap();
        assert!(!window_open(dir.path()));

        fs::remove_file(root.join(WINDOW_MARKER)).unwrap();
        assert!(!window_open(dir.path()));
    }
}
