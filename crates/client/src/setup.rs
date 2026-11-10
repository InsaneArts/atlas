//! Helper setup. [`preview`] is read-only: it reports what exists, what is
//! reachable and what cannot be known from here, writing nothing and starting
//! nothing; a fact we cannot observe is `null`, never `false`. [`migrate`]
//! records and verifies the first setup of the managed private layout through
//! the actions [`atlas_daemon::helper_setup`] runs inside the helper login.
pub mod configuration;
pub mod installed_app;
pub mod job;
pub mod migrate;

pub mod account;
pub use account::{FULL_NAME, validate_account_name};
pub use configuration::configure_helper;
pub use executor::{AdministratorRights, administrator_rights_current};
pub use job::{first_login_setup, prepare_helper, refresh_helper, restart_helper};

/// Notices emitted while a complete helper operation retains ownership.
pub enum Progress<'a> {
    Administrator(&'a str),
    Waiting(&'a str),
    Done(&'a str, &'a str),
}

pub mod executor;

use crate::client::Client;
use atlas_core::{
    account::{Account, account_named, account_with_uid, administrator},
    acl::Acl,
    app_bundle::{executable_of, signature},
    config::Config,
    digest::file_sha256,
    error::{Code, Error, Result},
    session::{require_hidden, validate_target_uid},
    transport::run_output,
};
use atlas_daemon::helper_setup::{self, identity, problem};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::{Path, PathBuf},
    process::Command,
};

/// The privileged dispatcher checks the chosen account after authorization,
/// immediately before entering its launchd session. The worker repeats the
/// check as the account itself before every mutation.
const DISPATCH: &str = r##"uid=$1 name=$2 guid=$3 mode=$4 executable=$5 request=$6 identity=$7 last=$8
refuse() { print -r -- '{"ok":false,"error":"Atlas could not verify the helper account or protect its executable; nothing was changed"}'; exit 1 }
[[ $uid == <501-60000> && $name =~ '^[A-Za-z0-9_][A-Za-z0-9._-]*$' && $name != <-> ]] || refuse
[[ $guid =~ '^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}$' ]] || refuse
chosen() {
  [[ $(/usr/bin/dscl . -read /Users/$name GeneratedUID </dev/null) == "GeneratedUID: $guid" ]] || refuse
  [[ $(/usr/bin/id -u -- $name </dev/null) == $uid && $(/usr/bin/id -nu -- $uid </dev/null) == $name ]] || refuse
}
chosen
stage_executable
chosen
if [[ $mode == login ]]; then
  /bin/launchctl asuser $uid /usr/bin/sudo -n -u $name $executable setup-helper $request --account $identity $last
else
  /usr/bin/sudo -n -u $name $executable setup-helper $request --account $identity $last
fi
exit $?
"##;

/// Where setup runs and whether sudo already holds administrator consent.
#[derive(Clone, Copy)]
pub enum Invocation {
    CachedLogin,
    Login,
    BeforeLogin,
    LoggedOut,
}

/// Run an identity-bound worker through the selected login/consent route.
/// Logged-out routes never create a graphical session; the worker rechecks
/// the account and absence of a login immediately before every mutation.
/// What the worker replied, as [`worker_reply`] reads it, once it has run.
///
/// # Errors
///
/// Returns a refusal when the helper is not the chosen account, or the worker's
/// failure.
pub fn request_as_helper(
    executable: &Path,
    helper: &Account,
    request: &helper_setup::Request,
    invocation: Invocation,
    expected: &identity::Identity,
) -> Result<std::result::Result<Value, helper_setup::Unfinished>> {
    let (cached_rights, mode, last) = match invocation {
        Invocation::CachedLogin => (true, "login", "--"),
        Invocation::Login => (false, "login", "--"),
        Invocation::BeforeLogin => (true, "plain", "--logged-out"),
        Invocation::LoggedOut => (false, "plain", "--logged-out"),
    };
    if expected.uid != helper.uid || expected.name != helper.name {
        return Err(problem("The helper is not the account Atlas chose"));
    }
    let mut command = Command::new("/usr/bin/sudo");
    if cached_rights {
        command.arg("-n");
    }
    let script = [
        executor::EXECUTOR,
        executor::BECOME_ROOT,
        &executor::staging(),
        DISPATCH,
    ]
    .concat();
    command
        .args([
            "/usr/bin/env",
            "-i",
            executor::ROOT_PATH,
            "/bin/zsh",
            "-f",
            "-c",
            &script,
            "zsh",
        ])
        .args([
            &helper.uid.to_string(),
            &helper.name,
            &expected.generated_uid,
        ])
        .arg(mode)
        .arg(executable)
        .arg(serde_json::to_string(request)?)
        .arg(serde_json::to_string(expected)?)
        .arg(last)
        .stderr(std::process::Stdio::piped());
    let output = executor::tracked_output(&mut command).map_err(|refusal| {
        problem(format!("The setup executor did not finish: {refusal:?}")).uncertain()
    })?;

    Ok(worker_reply(&output))
}

/// The JSON a `setup-helper` run printed: on standard output, or (a refusal)
/// on standard error. Acknowledged only when the run succeeded too.
fn worker_reply(
    output: &std::process::Output,
) -> std::result::Result<Value, helper_setup::Unfinished> {
    use helper_setup::{Unfinished, acknowledged};

    let reply: Value = serde_json::from_slice(&output.stdout)
        .or_else(|_| serde_json::from_slice(&output.stderr))
        .map_err(|cause| Unfinished::Silent(Some(cause)))?;
    if output.status.success() {
        return acknowledged(reply);
    }

    acknowledged(reply).and_then(|reply| {
        Err(Unfinished::Uncertain(
            reply["error"].as_str().map(str::to_owned),
        ))
    })
}

pub fn reported_pid(value: &Value) -> Option<u32> {
    value.as_u64().and_then(|pid| u32::try_from(pid).ok())
}

fn parse_disabled(text: &str, service: &str) -> Option<bool> {
    let key = format!("\"{service}\" =>");
    text.lines().find_map(|line| {
        let state = line.trim().strip_prefix(&key)?.trim();
        match state {
            "disabled" | "true" => Some(true),
            "enabled" | "false" => Some(false),
            _ => None,
        }
    })
}

fn screen_sharing() -> Value {
    let disabled = atlas_core::spawn::output(
        Command::new("/bin/launchctl").args(["print-disabled", "system"]),
    )
    .ok()
    .filter(|output| output.status.success())
    .and_then(|output| {
        parse_disabled(
            &String::from_utf8_lossy(&output.stdout),
            "com.apple.screensharing",
        )
    });

    json!({
        "service": match disabled {
            Some(true) => "disabled",
            Some(false) => "enabled",
            None => "unknown",
        },
        // Service state never proves an authenticated session; only a fresh
        // native capability check can (atlas verify-native).
        "authenticatedConnection": Value::Null,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Endpoint {
    path: PathBuf,
    present: bool,
    socket: Option<bool>,
    owner_uid: Option<u32>,
    mode: Option<String>,
    directory_owner_uid: Option<u32>,
    directory_mode: Option<String>,
    directory_acl: Option<Vec<String>>,
    /// Only the helper and explicitly listed accounts can reach the socket.
    private: Option<bool>,
}

/// Whether every allow entry in `acl_to_text` lines (`tag:uuid:name:id:allow:rights`)
/// grants access to one of `users` only. Deny entries never widen access.
fn grants_only(lines: &[String], users: &[u32]) -> bool {
    lines.iter().all(|line| {
        let fields: Vec<&str> = line.split(':').collect();
        match fields.as_slice() {
            [_, _, _, _, "deny", ..] => true,
            ["user", _, _, id, "allow", ..] => id.parse().is_ok_and(|id: u32| users.contains(&id)),
            _ => false,
        }
    })
}

fn endpoint(path: &Path, helper: u32, controller: u32) -> Endpoint {
    let socket = fs::symlink_metadata(path).ok();
    let parent = path.parent().unwrap_or(Path::new("/"));
    let directory = fs::symlink_metadata(parent).ok();
    let acl = directory.as_ref().and_then(|_| match Acl::of_path(parent) {
        Ok(None) => Some(Vec::new()),
        Ok(Some(acl)) => acl
            .text()
            .ok()
            .map(|text| text.lines().skip(1).map(str::to_owned).collect()),
        Err(_) => None,
    });

    // Private: helper-owned, no group/other mode bits, and explicit grants
    // only to the helper or this controller. Unknown if either is unreadable.
    let private = directory.as_ref().zip(acl.as_ref()).map(|(metadata, acl)| {
        metadata.is_dir()
            && metadata.uid() == helper
            && metadata.mode() & 0o077 == 0
            && grants_only(acl, &[helper, controller])
    });

    let mode = |metadata: &fs::Metadata| format!("{:04o}", metadata.mode() & 0o7777);
    Endpoint {
        path: path.to_owned(),
        present: socket.is_some(),
        socket: socket.as_ref().map(|m| m.file_type().is_socket()),
        owner_uid: socket.as_ref().map(MetadataExt::uid),
        mode: socket.as_ref().map(mode),
        directory_owner_uid: directory.as_ref().map(MetadataExt::uid),
        directory_mode: directory.as_ref().map(mode),
        directory_acl: acl,
        private,
    }
}

fn binary(path: &Path) -> Value {
    json!({
        "path": path,
        "sha256": file_sha256(path),
        "signature": signature(path),
    })
}

/// The label of the `launchctl list` row (`PID\tStatus\tLabel`) for `pid`.
fn job_label(list: &str, pid: u32) -> Option<String> {
    list.lines().find_map(|line| {
        let mut columns = line.split('\t');
        let row = columns.next()?.trim().parse::<u32>().ok()?;
        let _status = columns.next()?;
        let label = columns.next()?;
        (row == pid && columns.next().is_none()).then(|| label.to_owned())
    })
}

/// The definition path launchd recorded for a job (`path = …` in `launchctl print`).
fn job_path(print: &str) -> Option<PathBuf> {
    print.lines().find_map(|line| {
        let path = line.trim().strip_prefix("path = ")?;
        path.starts_with('/').then(|| PathBuf::from(path))
    })
}

fn unobserved_job() -> Value {
    json!({"label": null, "observed": false})
}

async fn read(client: &Client, argv: &[&str]) -> Option<String> {
    let reply = client
        .transport
        .call(&json!({"op": "run", "argv": argv, "timeoutMs": 3000}))
        .await
        .ok()?;

    (reply["ok"] == true && reply["exitCode"] == 0)
        .then(|| run_output(&reply, 65536).ok())
        .flatten()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// Helper-side, read-only facts gathered through the capture daemon. Runs
/// only for a verified hidden helper, like the identity probe. The plist is
/// the file launchd itself recorded, and must name the same label.
async fn helper_job(client: &Client, capture_pid: u32) -> Value {
    let Some(list) = read(client, &["/bin/launchctl", "list"]).await else {
        return unobserved_job();
    };

    let Some(label) = job_label(&list, capture_pid) else {
        return json!({"label": null, "observed": true});
    };

    let print = "exec /bin/launchctl print \"gui/$(/usr/bin/id -u)/$1\"";
    let plist = read(client, &["/bin/sh", "-c", print, "sh", &label])
        .await
        .as_deref()
        .and_then(job_path);
    let definition = match &plist {
        Some(path) => read(
            client,
            &[
                "/usr/bin/plutil",
                "-convert",
                "json",
                "-o",
                "-",
                &path.to_string_lossy(),
            ],
        )
        .await
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .filter(|definition| definition["Label"] == label.as_str()),
        None => None,
    };

    json!({
        "label": label,
        "observed": true,
        "plist": plist,
        "definitionMatchesLabel": plist.as_ref().map(|_| definition.is_some()),
        "programArguments": definition.as_ref().map(|d| d["ProgramArguments"].clone()),
        "keepAlive": definition.as_ref().map(|d| d["KeepAlive"].clone()),
        "sessionType": definition.as_ref().map(|d| d["LimitLoadToSessionType"].clone()),
    })
}

/// The configuration at `config_path`, if it loads, its state (missing,
/// present or invalid), and why it does not load.
fn read_config(config_path: &Path) -> (Option<Config>, &'static str, Option<String>) {
    if fs::symlink_metadata(config_path).is_err() {
        (None, "missing", None)
    } else {
        match Config::load(config_path) {
            Ok(config) => (Some(config), "present", None),
            Err(error) => (None, "invalid", Some(error.error)),
        }
    }
}

/// The helper to preview: the account named `helper`, else the one the
/// configuration names.
fn preview_account(helper: Option<&str>, config: Option<&Config>) -> Result<Account> {
    match (helper, config) {
        (Some(name), _) => account_named(name)?
            .ok_or_else(|| problem(format!("No account named {name} exists on this Mac"))),
        (None, Some(config)) => account_with_uid(config.helper_uid)?.ok_or_else(|| {
            problem(format!(
                "Configured helper UID {} has no account",
                config.helper_uid
            ))
        }),
        (None, None) => Err(Error::new(
            Code::Usage,
            "No usable configuration; name the helper account with --helper NAME",
        )),
    }
}

/// Inspect the proposed helper setup without applying it.
///
/// # Errors
///
/// Returns invalid account selection, configuration, identity or inspection failures.
/// Preview does not grant permission to apply its observations later.
pub async fn preview(config_path: &Path, helper: Option<&str>) -> Result<Value> {
    let controller_uid = nix::unistd::getuid().as_raw();
    let controller = account_with_uid(controller_uid)?;
    let (config, config_state, config_error) = read_config(config_path);
    let account = preview_account(helper, config.as_ref())?;
    validate_target_uid(account.uid, controller_uid)?;

    let conflict = config
        .as_ref()
        .is_some_and(|config| config.helper_uid != account.uid);

    // Otherwise the configuration atlas init would write; shown, never
    // written, by preview.
    let effective = match &config {
        Some(config) if !conflict => config.clone(),
        _ => configuration::proposed(&account)?,
    };

    let current = std::env::current_exe()
        .and_then(fs::canonicalize)
        .ok()
        .and_then(|path| file_sha256(&path));
    let installed = atlas_core::digest::probe_executable(&effective.identity_probe)
        .and_then(|path| file_sha256(&path));

    let mut report = json!({
        "ok": true,
        "preview": true,
        "controller": controller,
        "helper": {
            "account": account,
            "admin": administrator(&account.name),
        },
        "config": {
            "path": config_path,
            "state": if conflict { "conflict" } else { config_state },
            "error": config_error,
            // TOML text ready to save at the configuration path.
            "proposed": if config.is_none() || conflict {
                Some(effective.to_toml()?)
            } else {
                None
            },
        },
        "capture": {"endpoint": endpoint(&effective.socket_path, account.uid, controller_uid)},
        "input": {"endpoint": endpoint(&effective.input_socket, account.uid, controller_uid)},
        "rustExecutable": {
            "path": effective.identity_probe,
            "sha256": installed,
            "matchesThisBuild": installed.as_ref().zip(current.as_ref()).map(|(a, b)| a == b),
        },
        "screenSharing": screen_sharing(),
    });

    let client = Client::new(effective)?;
    observe(&client, account.uid, &mut report).await;
    report["nextSteps"] = json!(next_steps(&report));

    Ok(report)
}

/// What the running helper (`uid`) reports into `report`: its session, and
/// when it answers, its daemon, input service and launchd job.
async fn observe(client: &Client, uid: u32, report: &mut Value) {
    let status = client.status().await;
    let reachable = status["ok"] == true;
    let session = require_hidden(&status, uid).ok();
    let absence = if atlas_core::session::silent(&status) {
        tokio::task::spawn_blocking(move || atlas_core::session::absence(uid))
            .await
            .ok()
            .flatten()
    } else {
        None
    };

    report["session"] = json!({
        "reachable": reachable,
        "code": status["code"],
        "absence": absence.as_ref().map(atlas_core::session::Absence::as_str),
        "graphical": status["session"]["managerName"],
        "onConsole": status["session"]["onConsole"],
        "hidden": reachable.then_some(session.is_some()),
        "error": (!reachable).then(|| status["error"].clone()),
    });

    let pid = reported_pid(&status["daemon"]["pid"]);

    if reachable {
        report["capture"]["daemon"] = json!({
            "pid": pid,
            "binary": pid.and_then(executable_of).map(|path| binary(&path)),
            "screenRecording": status["permissions"]["screenCapture"],
        });

        report["input"]["service"] = json!({
            "pid": status["inputService"]["pid"],
            "version": status["inputService"]["version"],
            "build": status["inputService"]["build"],
            "current": atlas_core::session::input_build_current(&status),
            "accessibility": status["inputService"]["accessibility"],
            "ready": status["inputService"]["ready"],
            "error": status["inputService"]["error"],
        });
    }

    report["capture"]["job"] = match (&session, pid) {
        (Some(_), Some(pid)) => helper_job(client, pid).await,
        _ => unobserved_job(),
    };
}

fn next_steps(report: &Value) -> Vec<String> {
    let mut steps = Vec::new();

    match report["config"]["state"].as_str() {
        Some("missing") => steps.push(
            "No configuration exists. Review the proposed configuration and save it at the \
             configuration path."
                .to_owned(),
        ),
        Some("invalid") => steps.push("Fix or move the invalid configuration file.".to_owned()),
        Some("conflict") => steps.push(
            "The configuration names a different helper account; it will not be overwritten."
                .to_owned(),
        ),
        _ => {}
    }

    if report["session"]["code"] == "blocked" {
        steps.push(
            report["session"]["error"]
                .as_str()
                .unwrap_or("This shell may not connect to the helper; run atlas outside a sandbox.")
                .to_owned(),
        );
    } else if report["session"]["reachable"] != true {
        let step = match report["session"]["absence"].as_str() {
            Some("loggedOut") => atlas_core::session::LOGGED_OUT,
            Some("missing") => {
                "The helper account no longer exists. Run atlas init to set up a new one."
            }
            _ => "Start the helper: run atlas init (safe to run again).",
        };
        steps.push(step.to_owned());
    } else if report["session"]["hidden"] != true {
        steps.push("Switch back to your own account without logging the helper out.".to_owned());
    }

    if report["capture"]["endpoint"]["private"] == false {
        steps.push(
            "The capture socket directory is accessible beyond the helper and its explicit \
             access list."
                .to_owned(),
        );
    }

    if report["capture"]["daemon"]["screenRecording"] == false {
        steps.push(
            "In the helper's System Settings › Privacy & Security › Screen & System Audio Recording, \
             turn Atlas on."
                .to_owned(),
        );
    }

    if report["input"]["service"]["accessibility"] == false {
        steps
            .push("Grant Accessibility to the input service inside the helper account.".to_owned());
    }

    if report["input"]["service"]["current"] == false {
        steps.push(
            "The input service runs a different build than the installed executable; run \
             atlas start-input to replace it."
                .to_owned(),
        );
    }

    if report["rustExecutable"]["matchesThisBuild"] == false {
        steps.push(
            "The shared Rust executable differs from this build; run atlas install.".to_owned(),
        );
    }

    // Whether Save/Open panels work is doctor's native-dialog report, not a step.
    steps
}

/// The permissions the helper still lacks, by their System Settings names.
pub fn missing(status: &Value) -> Vec<&'static str> {
    [
        ("screenCapture", "Screen & System Audio Recording"),
        ("accessibility", "Accessibility"),
    ]
    .into_iter()
    .filter(|(key, _)| status["permissions"][*key] != true)
    .map(|(_, pane)| pane)
    .collect()
}

#[cfg(test)]
#[path = "setup_tests.rs"]
mod tests;
