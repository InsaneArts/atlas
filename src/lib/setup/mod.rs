//! Helper setup. [`preview`] is read-only: it reports what exists, what is
//! reachable and what cannot be known from here, writing nothing and starting
//! nothing; a fact we cannot observe is `null`, never `false`. [`migrate`]
//! moves capture to the managed private layout through [`helper`] actions run
//! inside the helper login.
pub mod helper;
pub mod migrate;

use crate::{
    acl::Acl,
    client::Client,
    config::Config,
    digest::file_sha256,
    error::{Error, Result},
    session::{require_hidden, validate_target_uid},
    transport::run_output,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    ffi::{CStr, CString},
    fs,
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Account {
    pub name: String,
    pub uid: u32,
    pub home: PathBuf,
}

enum Lookup {
    Name(CString),
    Uid(u32),
}

// FFI: getpw*_r, whose ERANGE nix misreads on macOS.
#[allow(unsafe_code)]
fn lookup(key: Lookup) -> Result<Option<Account>> {
    // SAFETY: passwd is plain C data for which all-zero is a valid value.
    let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
    let mut found = std::ptr::null_mut();
    let mut buffer = Vec::new();
    let mut status = libc::ERANGE;
    // Directory-service records can exceed any fixed buffer; grow on ERANGE.
    // The error is the return value: macOS need not set errno here.
    for size in [16 << 10, 64 << 10, 256 << 10, 1 << 20] {
        buffer.resize(size, 0);
        // SAFETY: valid entry, buffer and result pointers that outlive the call;
        // the strings in `entry` point into `buffer` and are copied before return.
        status = unsafe {
            match &key {
                Lookup::Name(name) => libc::getpwnam_r(
                    name.as_ptr(),
                    &mut entry,
                    buffer.as_mut_ptr(),
                    buffer.len(),
                    &mut found,
                ),
                Lookup::Uid(uid) => libc::getpwuid_r(
                    *uid,
                    &mut entry,
                    buffer.as_mut_ptr(),
                    buffer.len(),
                    &mut found,
                ),
            }
        };
        if status != libc::ERANGE {
            break;
        }
    }
    if status != 0 {
        return Err(std::io::Error::from_raw_os_error(status).into());
    }
    if found.is_null() {
        return Ok(None);
    }
    // SAFETY: getpw*_r succeeded, so both fields are NUL-terminated strings.
    let text = |field| {
        // SAFETY: see above.
        unsafe { CStr::from_ptr(field) }
            .to_string_lossy()
            .into_owned()
    };
    Ok(Some(Account {
        name: text(entry.pw_name),
        uid: entry.pw_uid,
        home: PathBuf::from(text(entry.pw_dir)),
    }))
}

/// Resolve an account through directory services by short name.
pub fn account_named(name: &str) -> Result<Option<Account>> {
    let name =
        CString::new(name).map_err(|_| Error::new("setup", "Account names cannot contain NUL"))?;
    lookup(Lookup::Name(name))
}

/// Resolve an account through directory services by UID.
pub fn account_with_uid(uid: u32) -> Result<Option<Account>> {
    lookup(Lookup::Uid(uid))
}

/// The full name of account `name`, as people know it, if it has one.
pub fn real_name(name: &str) -> Option<String> {
    let output = Command::new("/usr/bin/dscl")
        .args([".", "-read", &format!("/Users/{name}"), "RealName"])
        .output()
        .ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    let value = text.strip_prefix("RealName:")?.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

/// Run one `setup-helper` request as `helper`, inside its own login, through
/// sudo: with the administrator rights already held (`cached_rights`, as
/// init holds them), or letting sudo ask. The reply is whether the command
/// succeeded and the JSON it printed (null when it printed none).
pub(crate) fn request_as_helper(
    executable: &Path,
    helper: &Account,
    request: &helper::Request,
    cached_rights: bool,
) -> Result<(bool, Value)> {
    let mut command = Command::new("/usr/bin/sudo");
    if cached_rights {
        command.arg("-n");
    }
    let output = command
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
        .arg(serde_json::to_string(request)?)
        .output()?;
    let reply = serde_json::from_slice(&output.stdout)
        .or_else(|_| serde_json::from_slice(&output.stderr))
        .unwrap_or(Value::Null);
    Ok((output.status.success(), reply))
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Signature {
    pub identifier: Option<String>,
    pub cdhash: Option<String>,
    pub kind: Option<String>,
    pub team: Option<String>,
}

/// Parse `codesign -dvvv` diagnostics (written to stderr).
pub fn parse_signature(text: &str) -> Signature {
    let field = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(key))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    Signature {
        identifier: field("Identifier="),
        cdhash: field("CDHash="),
        kind: field("Signature="),
        team: field("TeamIdentifier=").filter(|team| team != "not set"),
    }
}

fn signature(path: &Path) -> Option<Signature> {
    let output = Command::new("/usr/bin/codesign")
        .arg("-dvvv")
        .arg(path)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| parse_signature(&String::from_utf8_lossy(&output.stderr)))
}

/// The bundle identifier the helper daemon must be signed as.
pub const IDENTIFIER: &str = "com.insanearts.atlas";
/// The Developer ID team that signs Atlas; consent follows it.
pub const TEAM: &str = "A474HM679A";
/// The identifier the app's executable inside Atlas.app is signed as.
pub const APP_IDENTIFIER: &str = "com.insanearts.atlas.bar";

/// The code requirement for Atlas: a Developer ID Application
/// signature by its team, under its identifier. install.sh checks the same.
pub fn requirement() -> String {
    requirement_for(IDENTIFIER)
}

/// The same requirement for code signed as `identifier`.
fn requirement_for(identifier: &str) -> String {
    format!(
        "anchor apple generic and identifier \"{identifier}\" \
         and certificate 1[field.1.2.840.113635.100.6.2.6] \
         and certificate leaf[field.1.2.840.113635.100.6.1.13] \
         and certificate leaf[subject.OU] = \"{TEAM}\""
    )
}

/// Whether `target` (a path, or a PID as text) validly satisfies a
/// Developer ID Application signature for Atlas by its team, the
/// requirement under which macOS consent persists. For a running process
/// this also checks that its loaded code still matches the file on disk.
fn satisfies_developer_id(target: &std::ffi::OsStr) -> bool {
    satisfies(target, IDENTIFIER)
}

/// Whether `target` validly satisfies the requirement for `identifier`.
fn satisfies(target: &std::ffi::OsStr, identifier: &str) -> bool {
    Command::new("/usr/bin/codesign")
        .args(["--verify", "--strict", "-R"])
        .arg(format!("={}", requirement_for(identifier)))
        .arg(target)
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Whether `name` administers this Mac (a helper must not); None when macOS
/// cannot say, which never counts as "no".
pub fn administrator(name: &str) -> Option<bool> {
    let output = Command::new("/usr/sbin/dseditgroup")
        .args(["-o", "checkmember", "-m", name, "admin"])
        .output()
        .ok()?;
    let answer = String::from_utf8_lossy(&output.stdout);
    if answer.starts_with("yes ") {
        Some(true)
    } else if answer.starts_with("no ") {
        Some(false)
    } else {
        None
    }
}

/// Whether the file at `path` is Atlas signed by its Developer ID.
pub fn developer_id(path: &Path) -> bool {
    satisfies_developer_id(path.as_os_str())
}

/// Whether the file at `path` is the app's executable signed by Atlas's
/// Developer ID.
pub fn app_signed(path: &Path) -> bool {
    satisfies(path.as_os_str(), APP_IDENTIFIER)
}

/// Whether process `pid` runs Atlas signed by its Developer ID, and
/// its loaded code is exactly the file now on disk. (`codesign -d` on a PID
/// describes the file at its path, so only verification proves this.)
fn running_developer_id(pid: u32) -> bool {
    satisfies_developer_id(pid.to_string().as_ref())
}

// FFI: proc_pidpath has no safe binding.
#[allow(unsafe_code)]
fn executable_of(pid: u32) -> Option<PathBuf> {
    let size = u32::try_from(libc::PROC_PIDPATHINFO_MAXSIZE).ok()?;
    let mut buffer = vec![0_u8; usize::try_from(size).ok()?];
    // SAFETY: the buffer is writable for the length passed.
    let length =
        unsafe { libc::proc_pidpath(i32::try_from(pid).ok()?, buffer.as_mut_ptr().cast(), size) };
    let length = usize::try_from(length).ok().filter(|length| *length > 0)?;
    buffer.truncate(length);
    Some(PathBuf::from(String::from_utf8_lossy(&buffer).into_owned()))
}

/// Parse `launchctl print-disabled` output for one service.
pub fn parse_disabled(text: &str, service: &str) -> Option<bool> {
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
pub fn grants_only(lines: &[String], users: &[u32]) -> bool {
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
pub fn job_label(list: &str, pid: u32) -> Option<String> {
    list.lines().find_map(|line| {
        let mut columns = line.split('\t');
        let row = columns.next()?.trim().parse::<u32>().ok()?;
        let _status = columns.next()?;
        let label = columns.next()?;
        (row == pid && columns.next().is_none()).then(|| label.to_owned())
    })
}

/// The definition path launchd recorded for a job (`path = …` in `launchctl print`).
pub fn job_path(print: &str) -> Option<PathBuf> {
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

/// A configuration the user could adopt; shown, never written, by preview.
fn proposal(account: &Account) -> Config {
    let uid = account.uid;
    Config {
        helper_uid: uid,
        socket_path: format!("/tmp/atlas-live-{uid}/{uid}.sock").into(),
        identity_probe: "/Users/Shared/Atlas/atlas".into(),
        input_socket: format!("/tmp/atlas-input-{uid}/input.sock").into(),
        output_dir: "runs".into(),
    }
}

pub async fn preview(config_path: &Path, helper: Option<&str>) -> Result<Value> {
    let controller_uid = nix::unistd::getuid().as_raw();
    let controller = account_with_uid(controller_uid)?;
    let (config, config_state, config_error) = if fs::symlink_metadata(config_path).is_err() {
        (None, "missing", None)
    } else {
        match Config::load(config_path) {
            Ok(config) => (Some(config), "present", None),
            Err(error) => (None, "invalid", Some(error.error)),
        }
    };
    let account = match (helper, &config) {
        (Some(name), _) => account_named(name)?.ok_or_else(|| {
            Error::new(
                "setup",
                format!("No account named {name} exists on this Mac"),
            )
        })?,
        (None, Some(config)) => account_with_uid(config.helper_uid)?.ok_or_else(|| {
            Error::new(
                "setup",
                format!("Configured helper UID {} has no account", config.helper_uid),
            )
        })?,
        (None, None) => {
            return Err(Error::new(
                "usage",
                "No usable configuration; name the helper account with --helper NAME",
            ));
        }
    };
    validate_target_uid(account.uid, controller_uid)?;
    let conflict = config
        .as_ref()
        .is_some_and(|config| config.helper_uid != account.uid);
    let effective = match &config {
        Some(config) if !conflict => config.clone(),
        _ => proposal(&account),
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
    let status = client.status().await;
    let reachable = status["ok"] == true;
    let session = require_hidden(&status, account.uid).ok();
    report["session"] = json!({
        "reachable": reachable,
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
        (Some(_), Some(pid)) => helper_job(&client, pid).await,
        _ => json!({"label": null, "observed": false}),
    };
    report["nextSteps"] = json!(next_steps(&report));
    Ok(report)
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
    if report["session"]["reachable"] != true {
        steps.push(
            "The helper is not running. Sign in to the helper account once, then switch back \
             without logging it out."
                .to_owned(),
        );
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The installer verifies downloads against exactly setup's requirement.
    #[test]
    fn the_installer_checks_the_same_requirement() {
        let script = include_str!("../../../install.sh");
        let line = script
            .lines()
            .find_map(|line| line.strip_prefix("REQUIREMENT=\""))
            .and_then(|line| line.strip_suffix('"'))
            .expect("install.sh sets REQUIREMENT");
        let expanded = line
            .replace("\\\"", "\"")
            .replace("$IDENTIFIER", IDENTIFIER)
            .replace("$TEAM", TEAM);
        assert_eq!(expanded, requirement());
        assert!(script.contains(&format!("IDENTIFIER=\"{IDENTIFIER}\"")));
        assert!(script.contains(&format!("TEAM=\"{TEAM}\"")));
    }

    /// Only Atlas signed by its own Developer ID team passes, as a file
    /// or as a running process.
    #[test]
    fn signatures_are_checked_against_atlass_identity() {
        assert!(
            !developer_id(Path::new("/bin/ls")),
            "Apple-signed, not ours"
        );
        let test = std::env::current_exe().unwrap();
        assert!(!developer_id(&test), "ad-hoc signed test build");
        assert!(
            !running_developer_id(std::process::id()),
            "this ad-hoc test process is not Atlas's Developer ID"
        );
    }

    #[test]
    fn accounts_resolve_through_directory_services() {
        let me = account_with_uid(nix::unistd::getuid().as_raw())
            .unwrap()
            .unwrap();
        assert_eq!(account_named(&me.name).unwrap(), Some(me.clone()));
        assert!(account_named("no-such-atlas-account").unwrap().is_none());
        assert!(account_named("bad\0name").is_err());
    }

    #[test]
    fn codesign_diagnostics_parse_without_inventing_fields() {
        let text = "Executable=/x\nIdentifier=dev.offstage.sessiond\nCDHash=0c61eed3\n\
                    Signature=adhoc\nTeamIdentifier=not set\n";
        assert_eq!(
            parse_signature(text),
            Signature {
                identifier: Some("dev.offstage.sessiond".into()),
                cdhash: Some("0c61eed3".into()),
                kind: Some("adhoc".into()),
                team: None,
            }
        );
        assert_eq!(parse_signature(""), Signature::default());
    }

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
        let dir = tempfile::tempdir().unwrap();
        use std::os::unix::fs::PermissionsExt;
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
    fn screen_sharing_is_never_a_setup_step() {
        for service in ["disabled", "unknown", "enabled"] {
            let steps = next_steps(&json!({"config": {"state": "present"},
                "session": {"reachable": true, "hidden": true},
                "screenSharing": {"service": service}}));
            assert!(!steps.join(" ").contains("Screen Sharing"), "{service}");
        }
    }
}
