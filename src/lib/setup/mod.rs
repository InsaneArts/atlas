//! Helper setup. [`preview`] is read-only: it reports what exists, what is
//! reachable and what cannot be known from here, writing nothing and starting
//! nothing; a fact we cannot observe is `null`, never `false`. [`migrate`]
//! moves capture to the managed private layout through [`helper`] actions run
//! inside the helper login.
pub(crate) mod configuration;
pub mod first_login;
pub mod helper;
pub mod identity;
pub(crate) mod installed_app;
pub mod migrate;

pub(crate) mod account;
pub use account::{
    Account, FIRST_USER_UID, FULL_NAME, account_named, account_with_uid, administrator,
    close_leftovers, create_helper, real_name, validate_account_name,
};
pub use configuration::configure_helper;
pub use executor::{administrator_rights_current, request_administrator_rights};
pub use helper::{first_login_setup, prepare_helper, refresh_helper, restart_helper};
pub use installed_app::link_installed_command as link_command;

/// Notices emitted while a complete helper operation retains ownership.
pub enum Progress<'a> {
    Administrator(&'a str),
    Waiting(&'a str),
    Done(&'a str, &'a str),
}

pub(crate) use account::{Title, asked_real_name};
pub(crate) mod executor;
mod signature;
pub(crate) use signature::requirement_for;
pub use signature::{APP_IDENTIFIER, IDENTIFIER, developer_id};
pub(crate) use signature::{app_signed, runs_current};
use signature::{executable_of, signature};

use crate::{
    acl::Acl,
    client::Client,
    config::Config,
    digest::file_sha256,
    error::{Code, Error, Result},
    session::{require_hidden, validate_target_uid},
    transport::run_output,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::{Path, PathBuf},
    process::Command,
};

fn problem(message: impl Into<String>) -> Error {
    Error::new(Code::Setup, message)
}

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
pub(crate) enum Invocation {
    CachedLogin,
    Login,
    BeforeLogin,
    LoggedOut,
}

/// Run an identity-bound worker through the selected login/consent route.
/// Logged-out routes never create a graphical session; the worker rechecks
/// the account and absence of a login immediately before every mutation.
pub(crate) fn request_as_helper(
    executable: &Path,
    helper: &Account,
    request: &helper::Request,
    invocation: Invocation,
    expected: &identity::Identity,
) -> Result<(bool, Value)> {
    let (cached_rights, mode, last) = match invocation {
        Invocation::CachedLogin => (true, "login", "--"),
        Invocation::Login => (false, "login", "--"),
        Invocation::BeforeLogin => (true, "plain", "--logged-out"),
        Invocation::LoggedOut => (false, "plain", "--logged-out"),
    };
    if expected.uid != helper.uid || expected.name != helper.name {
        return Err(Error::new(
            Code::Setup,
            "The helper is not the account Atlas chose",
        ));
    }
    let mut command = Command::new("/usr/bin/sudo");
    if cached_rights {
        command.arg("-n");
    }
    let script = [
        crate::setup::executor::EXECUTOR,
        crate::setup::executor::BECOME_ROOT,
        &crate::setup::executor::staging(),
        DISPATCH,
    ]
    .concat();
    command
        .args([
            "/usr/bin/env",
            "-i",
            "PATH=/usr/bin:/bin:/usr/sbin:/sbin",
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
    let output = crate::setup::executor::tracked_output(&mut command).map_err(|refusal| {
        Error::new(
            Code::Setup,
            format!("The setup executor did not finish: {refusal:?}"),
        )
        .uncertain()
    })?;

    Ok(worker_reply(&output))
}

/// Whether a `setup-helper` run succeeded, and the JSON it printed: on
/// standard output, or (a refusal) on standard error; null when none.
fn worker_reply(output: &std::process::Output) -> (bool, Value) {
    let reply = serde_json::from_slice(&output.stdout)
        .or_else(|_| serde_json::from_slice(&output.stderr))
        .unwrap_or(Value::Null);

    (output.status.success(), reply)
}

/// Parse `launchctl print-disabled` output for one service.
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
    let disabled = Command::new("/bin/launchctl")
        .args(["print-disabled", "system"])
        .output()
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

/// Output of a read-only command run inside the helper, if it succeeded.
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
    let unobserved = json!({"label": null, "observed": false});

    let Some(list) = read(client, &["/bin/launchctl", "list"]).await else {
        return unobserved;
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
        (Some(name), _) => account_named(name)?.ok_or_else(|| {
            Error::new(
                Code::Setup,
                format!("No account named {name} exists on this Mac"),
            )
        }),
        (None, Some(config)) => account_with_uid(config.helper_uid)?.ok_or_else(|| {
            Error::new(
                Code::Setup,
                format!("Configured helper UID {} has no account", config.helper_uid),
            )
        }),
        (None, None) => Err(Error::new(
            Code::Usage,
            "No usable configuration; name the helper account with --helper NAME",
        )),
    }
}

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
    let installed = crate::digest::probe_executable(&effective.identity_probe)
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
    let absence = if crate::session::silent(&status) {
        tokio::task::spawn_blocking(move || crate::session::absence(uid))
            .await
            .ok()
            .flatten()
    } else {
        None
    };

    report["session"] = json!({
        "reachable": reachable,
        "code": status["code"],
        "absence": absence.as_ref().map(crate::session::Absence::as_str),
        "graphical": status["session"]["managerName"],
        "onConsole": status["session"]["onConsole"],
        "hidden": reachable.then_some(session.is_some()),
        "error": (!reachable).then(|| status["error"].clone()),
    });

    let pid = status["daemon"]["pid"]
        .as_u64()
        .and_then(|p| u32::try_from(p).ok());

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
            "current": crate::session::input_build_current(&status),
            "accessibility": status["inputService"]["accessibility"],
            "ready": status["inputService"]["ready"],
            "error": status["inputService"]["error"],
        });
    }

    report["capture"]["job"] = match (&session, pid) {
        (Some(_), Some(pid)) => helper_job(client, pid).await,
        _ => json!({"label": null, "observed": false}),
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
            Some("loggedOut") => crate::session::LOGGED_OUT,
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
mod tests {
    #[test]
    fn the_dispatcher_removes_first_login_copies_on_success_and_failure() {
        use std::os::unix::fs::PermissionsExt;

        for failure in ["none", "identity", "worker"] {
            let dir = tempfile::tempdir().unwrap();
            let source = dir.path().join("Source.app/Contents/MacOS");
            fs::create_dir_all(&source).unwrap();
            fs::write(source.join("atlas"), "worker").unwrap();
            fs::write(source.join("AtlasApp"), "app").unwrap();
            let guid = "6DB4B7AF-BE3C-486C-BD46-1B4558946919";
            let mut script = [crate::setup::executor::staging().as_str(), DISPATCH]
                .concat()
                .replace(
                    "/private/var/tmp/atlas-approved.XXXXXXXX",
                    &format!("{}/atlas-approved.XXXXXXXX", dir.path().display()),
                );
            for (tool, body) in [
                (
                    "/usr/bin/dscl",
                    format!(
                        "[[ $failure == identity && -e verified ]] && exit 1; print 'GeneratedUID: {guid}'"
                    ),
                ),
                ("/usr/bin/id", "[[ $1 == -u ]] && print 7001 || print helper".into()),
                ("/usr/sbin/chown", "exit 0".into()),
                ("/usr/bin/codesign", "touch verified".into()),
                (
                    "/usr/bin/sudo",
                    "[[ -f $4 ]] || exit 2; print -r -- \"$4\" > worker-path; [[ $failure == worker ]] && exit 1; print '{\"ok\":true}'".into(),
                ),
            ] {
                let path = dir.path().join(Path::new(tool).file_name().unwrap());
                fs::write(&path, format!("#!/bin/zsh -f\n{body}\n")).unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
                script = script.replace(tool, path.to_str().unwrap());
            }
            let output = Command::new("/bin/zsh")
                .args(["-f", "-c", &script, "zsh", "7001", "helper", guid, "plain"])
                .arg(source.join("atlas"))
                .args([
                    r#"{"action":"first-login","uid":7001,"opener":true}"#,
                    "{}",
                    "--logged-out",
                ])
                .env("failure", failure)
                .current_dir(dir.path())
                .output()
                .unwrap();
            assert_eq!(
                output.status.success(),
                failure == "none",
                "{failure}: {output:?}"
            );
            assert!(dir.path().join("verified").is_file());
            assert!(
                !fs::read_dir(dir.path()).unwrap().any(|entry| {
                    entry
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with("atlas-approved.")
                }),
                "{failure}"
            );
            if failure != "identity" {
                let worker = fs::read_to_string(dir.path().join("worker-path")).unwrap();
                assert!(worker.contains("/atlas-approved."));
                assert!(!Path::new(worker.trim()).exists());
            }
        }
    }

    #[test]
    fn the_privileged_dispatcher_checks_identity_before_entering_a_session() {
        use std::os::unix::fs::PermissionsExt;
        let identity = identity::Identity {
            uid: 7001,
            name: "helper".into(),
            generated_uid: "6DB4B7AF-BE3C-486C-BD46-1B4558946919".into(),
        };
        let requests = [
            helper::Request::FirstLogin {
                uid: 7001,
                opener: false,
            },
            helper::Request::Prepare {
                uid: 7001,
                controller: "person".into(),
            },
            helper::Request::Install {
                agent: helper::Agent {
                    label: helper::LABEL.into(),
                    uid: 7001,
                    controller_uid: 501,
                    program: "/Applications/Atlas.app/Contents/MacOS/atlas".into(),
                    socket: "/Users/helper/.atlas/helper/7001.sock".into(),
                },
            },
        ];
        for request in requests {
            for replacement in ["never", "before", "during"] {
                let dir = tempfile::tempdir().unwrap();
                let replaced = replacement != "never";
                if replacement == "before" {
                    fs::write(dir.path().join("replaced"), "").unwrap();
                } else if replacement == "during" {
                    fs::write(dir.path().join("replace-during-copy"), "").unwrap();
                }
                let mut script = [
                    "stage_executable() { executable=/protected-atlas; if [[ -e replace-during-copy ]]; then touch replaced; fi; }\n",
                    DISPATCH,
                ]
                .concat();
                for (tool, body) in [
                    (
                        "/usr/bin/dscl",
                        format!(
                            "if [[ -e replaced ]]; then print 'GeneratedUID: 11111111-2222-3333-4444-555555555555'; else print 'GeneratedUID: {}'; fi",
                            identity.generated_uid
                        ),
                    ),
                    (
                        "/usr/bin/id",
                        "[[ $1 == -u ]] && print 7001 || print helper".into(),
                    ),
                    ("/bin/launchctl", "print -rl -- \"$@\" > dispatch".into()),
                    ("/usr/bin/sudo", "print -rl -- \"$@\" > dispatch".into()),
                ] {
                    let path = dir.path().join(Path::new(tool).file_name().unwrap());
                    fs::write(&path, format!("#!/bin/zsh -f\n{body}\n")).unwrap();
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
                    script = script.replace(tool, path.to_str().unwrap());
                }
                let encoded = serde_json::to_string(&identity).unwrap();
                let output = Command::new("/bin/zsh")
                    .args([
                        "-f",
                        "-c",
                        &script,
                        "zsh",
                        "7001",
                        "helper",
                        &identity.generated_uid,
                        "login",
                        "/unused-atlas",
                        &serde_json::to_string(&request).unwrap(),
                        &encoded,
                        "--",
                    ])
                    .current_dir(dir.path())
                    .output()
                    .unwrap();
                assert_eq!(
                    output.status.success(),
                    !replaced,
                    "{}",
                    String::from_utf8_lossy(&output.stdout)
                );
                let dispatched = fs::read_to_string(dir.path().join("dispatch"));
                if replaced {
                    assert!(dispatched.is_err(), "no launchctl asuser or worker ran");
                } else {
                    let dispatched = dispatched.unwrap();
                    assert!(
                        dispatched.contains(&encoded),
                        "the worker receives the same expected identity"
                    );
                    assert!(dispatched.contains("/protected-atlas"));
                    assert!(!dispatched.contains("/unused-atlas"));
                }
            }
        }
    }

    use super::*;

    #[test]
    fn disabled_services_parse_exactly() {
        let text = "disabled services = {\n\t\t\"com.apple.screensharing\" => disabled\n\
                    \t\t\"com.openssh.sshd\" => enabled\n\t}\n";
        assert_eq!(parse_disabled(text, "com.apple.screensharing"), Some(true));
        assert_eq!(parse_disabled(text, "com.openssh.sshd"), Some(false));
        assert_eq!(parse_disabled(text, "com.apple.screen"), None);
    }

    #[test]
    fn endpoint_privacy_reflects_owner_mode_and_acl() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let socket = dir.path().join("socket");
        let me = nix::unistd::getuid().as_raw();

        let report = endpoint(&socket, me, me + 7);
        assert!(!report.present);
        assert_eq!(report.private, Some(true));
        assert_eq!(report.directory_acl, Some(Vec::new()));
        assert_eq!(endpoint(&socket, me + 1, me + 7).private, Some(false));

        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(endpoint(&socket, me, me + 7).private, Some(false));
    }

    #[test]
    fn launchd_rows_and_paths_parse_exactly() {
        let list = "PID\tStatus\tLabel\n-\t0\tcom.apple.x\n68151\t0\tdev.atlas.live-sessiond\n\
                    6815\t0\tother\n";
        assert_eq!(
            job_label(list, 68151).as_deref(),
            Some("dev.atlas.live-sessiond")
        );
        assert_eq!(job_label(list, 6815).as_deref(), Some("other"));
        assert_eq!(job_label(list, 1), None);
        assert_eq!(job_label("68151\t0\ta\tb\n", 68151), None);

        let print = "gui/502/dev.atlas.live-sessiond = {\n\tactive count = 1\n\t\
                     path = /Users/atlastest/Library/LaunchAgents/dev.atlas.live-sessiond.plist\n}";
        assert_eq!(
            job_path(print),
            Some(PathBuf::from(
                "/Users/atlastest/Library/LaunchAgents/dev.atlas.live-sessiond.plist"
            ))
        );
        assert_eq!(job_path("\tpath = relative.plist\n"), None);
    }

    #[test]
    fn only_helper_and_controller_grants_keep_a_directory_private() {
        let user = |id: u32, kind: &str| format!("user:UUID:name:{id}:{kind}:list,search");
        assert!(grants_only(&[], &[502, 501]));
        assert!(grants_only(
            &[user(501, "allow"), user(9, "deny")],
            &[502, 501]
        ));
        assert!(!grants_only(&[user(503, "allow")], &[502, 501]));
        assert!(!grants_only(
            &["group:UUID:everyone:12:allow:list".to_owned()],
            &[502, 501]
        ));
        assert!(!grants_only(&["garbage".to_owned()], &[502, 501]));
    }

    #[test]
    fn preview_names_its_helper_or_says_how() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("atlas.toml");
        assert_eq!(read_config(&missing), (None, "missing", None));

        fs::write(&missing, "not = [toml").unwrap();
        let (config, state, error) = read_config(&missing);
        assert!(config.is_none() && state == "invalid" && error.is_some());

        assert_eq!(preview_account(None, None).unwrap_err().code, "usage");
        let unknown = preview_account(Some("no-such-atlas-account"), None).unwrap_err();
        assert_eq!(unknown.code, "setup");
    }

    #[test]
    fn login_is_advised_only_for_a_confirmed_logout() {
        for absence in [Some("loggedOut"), Some("missing"), None] {
            let steps = next_steps(&json!({
                "session": {"reachable": false, "absence": absence},
            }));
            if absence == Some("loggedOut") {
                assert!(steps[0].contains("Log in"));
            } else {
                assert!(!steps[0].contains("Log in"), "{absence:?}");
                assert!(steps[0].contains("atlas init"), "{absence:?}");
            }
        }
    }

    #[tokio::test]
    async fn observing_a_blocked_transport_preserves_its_code_and_advice() {
        use tokio::{
            io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
            net::UnixListener,
        };

        let dir = tempfile::Builder::new()
            .prefix("atlas-")
            .tempdir_in("/tmp")
            .unwrap();
        let uid = 60001;
        let mut client = Client::new(Config {
            helper_uid: uid,
            socket_path: dir.path().join("helper.sock"),
            identity_probe: dir.path().join("atlas"),
            input_socket: dir.path().join("input.sock"),
            output_dir: dir.path().join("runs"),
        })
        .unwrap();
        client.controller = crate::control::Controller::at(uid, &dir.path().join("state"));
        client.transport.peer_uid = nix::unistd::getuid().as_raw();
        let listener = UnixListener::bind(&client.transport.path).unwrap();
        let status = json!({
            "ok": false,
            "code": "blocked",
            "error": "This shell may not connect to the helper; run atlas outside a sandbox.",
        });
        let serving = async {
            let (stream, _) = listener.accept().await.unwrap();
            let mut stream = BufReader::new(stream);
            let mut request = String::new();
            stream.read_line(&mut request).await.unwrap();
            stream
                .get_mut()
                .write_all(format!("{status}\n").as_bytes())
                .await
                .unwrap();
        };
        let mut report = json!({});
        tokio::join!(observe(&client, uid, &mut report), serving);

        let steps = next_steps(&report);
        let doctor = crate::session::diagnose(status, uid);
        assert_eq!(steps, vec![doctor["nextSteps"][0].as_str().unwrap()]);
        assert!(!steps.join(" ").contains("Log in"));
        assert_eq!(report["session"]["code"], "blocked");
    }

    #[test]
    fn a_blocked_transport_advises_running_outside_the_sandbox() {
        for error in [
            json!("This shell may not connect to the helper; run atlas outside a sandbox."),
            Value::Null,
        ] {
            let steps = next_steps(&json!({
                "session": {"reachable": false, "code": "blocked", "error": error},
            }));
            assert_eq!(
                steps,
                ["This shell may not connect to the helper; run atlas outside a sandbox."]
            );
        }
    }

    #[test]
    fn screen_sharing_is_never_a_setup_step() {
        for service in ["disabled", "unknown", "enabled"] {
            let steps = next_steps(&json!({"config": {"state": "present"},
                "session": {"reachable": true, "hidden": true},
                "screenSharing": {"service": service}}));
            assert!(!steps.join(" ").contains("Screen Sharing"), "{service}");
        }
    }
}
