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

/// macOS numbers people's accounts from this user ID; lower ones belong to
/// the system.
pub const FIRST_USER_UID: u32 = 501;

#[allow(
    unsafe_code,
    reason = "FFI: getpw*_r, whose ERANGE nix misreads on macOS"
)]
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
    let name = CString::new(name)
        .map_err(|_| Error::new(Code::Setup, "Account names cannot contain NUL"))?;
    lookup(Lookup::Name(name))
}

/// Resolve an account through directory services by UID.
pub fn account_with_uid(uid: u32) -> Result<Option<Account>> {
    lookup(Lookup::Uid(uid))
}

/// The full name of account `name`, as people know it, if it has one.
pub fn real_name(name: &str) -> Option<String> {
    asked_real_name(name).flatten()
}

/// What directory services say of account `name`'s full name: `None` when
/// they did not answer in time, `Some(None)` when it has none.
pub fn asked_real_name(name: &str) -> Option<Option<String>> {
    let output = crate::bounded::output(
        Command::new("/usr/bin/dscl").args([".", "-read", &format!("/Users/{name}"), "RealName"]),
        crate::bounded::LOOKUP,
    )
    .ok()?;

    Some(parse_real_name(&output.stdout))
}

/// The full name in `dscl -read`'s answer for RealName, if it has one.
fn parse_real_name(answer: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(answer).ok()?;
    let value = text.strip_prefix("RealName:")?.trim().to_owned();

    (!value.is_empty()).then_some(value)
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

/// Run one `setup-helper` request as `helper`, inside its own login, through
/// sudo: with the administrator rights already held (`cached_rights`, as
/// init holds them), or letting sudo ask. The reply is whether the command
/// succeeded and the JSON it printed (null when it printed none).
pub(crate) fn request_as_helper(
    executable: &Path,
    helper: &Account,
    request: &helper::Request,
    cached_rights: bool,
    expected: &identity::Identity,
) -> Result<(bool, Value)> {
    run_as_helper(
        executable,
        helper,
        request,
        cached_rights,
        true,
        expected,
        false,
    )
}

/// Run one `setup-helper` request as `helper` while it is logged out (no
/// desktop): plainly through sudo, with the administrator rights already
/// held, so nothing (no per-user launchd session) starts in the account.
/// The reply as [`request_as_helper`] gives it.
pub(crate) fn request_before_login(
    executable: &Path,
    helper: &Account,
    request: &helper::Request,
    expected: &identity::Identity,
) -> Result<(bool, Value)> {
    run_as_helper(executable, helper, request, true, false, expected, true)
}

/// Run one `setup-helper` request as the logged-out `helper`, recorded as
/// `expected`: plainly through sudo, which may ask for the person's
/// password, so nothing starts a login of the account again. Once sudo lets
/// it run, the request itself refuses unless its account is still that one
/// and still logged out, so nothing changes in another account or in a
/// login that started meanwhile. The reply as [`request_as_helper`] gives
/// it.
pub(crate) fn request_logged_out_as(
    executable: &Path,
    helper: &Account,
    request: &helper::Request,
    expected: &identity::Identity,
) -> Result<(bool, Value)> {
    run_as_helper(executable, helper, request, false, false, expected, true)
}

/// Run one `setup-helper` request as `helper` through sudo, inside its login
/// (`in_login`, through `launchctl asuser`) or plainly, with what the
/// request checks of its account first (`guard`).
fn run_as_helper(
    executable: &Path,
    helper: &Account,
    request: &helper::Request,
    cached_rights: bool,
    in_login: bool,
    expected: &identity::Identity,
    logged_out: bool,
) -> Result<(bool, Value)> {
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
        crate::onboarding::as_root::EXECUTOR,
        crate::onboarding::as_root::BECOME_ROOT,
        &crate::onboarding::as_root::staging(),
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
        .arg(if in_login { "login" } else { "plain" })
        .arg(executable)
        .arg(serde_json::to_string(request)?)
        .arg(serde_json::to_string(expected)?)
        .arg(if logged_out { "--logged-out" } else { "--" })
        .stderr(std::process::Stdio::piped());
    let output = crate::onboarding::as_root::tracked_output(&mut command).map_err(|refusal| {
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

/// The code requirement for code signed as `identifier`: a Developer ID
/// Application signature by Atlas's team. install.sh checks the same.
pub(crate) fn requirement_for(identifier: &str) -> String {
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
    // An agent's sandbox may keep dseditgroup from the directory service;
    // the account's group list still answers there.
    directory_member(name).or_else(|| listed_member(name))
}

/// What the directory service says of `name` in the admin group.
fn directory_member(name: &str) -> Option<bool> {
    let output = crate::bounded::output(
        Command::new("/usr/sbin/dseditgroup").args(["-o", "checkmember", "-m", name, "admin"]),
        crate::bounded::LOOKUP,
    )
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

/// The admin group's ID, the same on every Mac.
const ADMIN_GID: &str = "80";

/// Whether the admin group is among the group IDs `id` lists for `name`.
/// IDs, not names: a name `id` cannot resolve is printed as its number.
fn listed_member(name: &str) -> Option<bool> {
    let output = crate::bounded::output(
        Command::new("/usr/bin/id").args(["-G", name]),
        crate::bounded::LOOKUP,
    )
    .ok()
    .filter(|output| output.status.success())?;

    parse_admin_listed(&String::from_utf8_lossy(&output.stdout))
}

/// Whether `id -G`'s list holds the admin group's ID; None for no list.
fn parse_admin_listed(groups: &str) -> Option<bool> {
    let mut groups = groups.split_whitespace().peekable();
    groups.peek()?;

    Some(groups.any(|group| group == ADMIN_GID))
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

/// Whether helper daemon `pid` runs `executable` exactly as it is on disk
/// now. False for a daemon still running a build an update replaced: its
/// file moved aside, or its loaded code no longer matches the file.
pub fn runs_current(pid: u32, executable: &Path) -> bool {
    let running = executable_of(pid).and_then(|path| fs::canonicalize(path).ok());

    running.is_some() && running == fs::canonicalize(executable).ok() && running_developer_id(pid)
}

#[allow(unsafe_code, reason = "FFI: proc_pidpath has no safe binding")]
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
            let mut script = [crate::onboarding::as_root::staging().as_str(), DISPATCH]
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
        assert_eq!(expanded, requirement_for(IDENTIFIER));
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
        assert!(
            !runs_current(std::process::id(), &test),
            "only Atlas's own signed build is current"
        );
        assert!(!runs_current(std::process::id(), Path::new("/bin/ls")));
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
        let text = "Executable=/x\nIdentifier=com.example.sessiond\nCDHash=0c61eed3\n\
                    Signature=adhoc\nTeamIdentifier=not set\n";
        assert_eq!(
            parse_signature(text),
            Signature {
                identifier: Some("com.example.sessiond".into()),
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
    fn group_lists_name_the_admin_group_exactly() {
        assert_eq!(parse_admin_listed("20 12 80 61\n"), Some(true));

        // An ID merely starting with 80 is not the admin group.
        assert_eq!(parse_admin_listed("20 12 801 61\n"), Some(false));

        assert_eq!(parse_admin_listed("\n"), None);
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
    #[test]
    fn a_full_name_is_read_from_directory_services_answer_on_one_line_or_the_next() {
        assert_eq!(
            parse_real_name(b"RealName: Atlas\n").as_deref(),
            Some("Atlas")
        );
        assert_eq!(
            parse_real_name(b"RealName:\n Atlas Helper\n").as_deref(),
            Some("Atlas Helper")
        );
        assert_eq!(parse_real_name(b"RealName:\n"), None);
        assert_eq!(parse_real_name(b"No such key: RealName\n"), None);
    }
}
