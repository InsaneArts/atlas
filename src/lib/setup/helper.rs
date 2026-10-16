//! Typed setup actions executed *inside* the helper login (`setup-helper`).
//! Every action inspects before it acts, fails closed when inspection fails,
//! and is safe to repeat. Plists are only moved into the private backup
//! directory, never deleted, and only when their bytes are the expected ones.
use super::account_with_uid;
use crate::{
    acl::Acl,
    digest::file_sha256,
    error::{Error, Result},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

/// Private Atlas directory in the helper home and its entries.
pub const ROOT: &str = ".atlas";
/// Where the helper daemon's socket lives.
pub const DAEMON: &str = "helper";
pub const INPUT: &str = "input";
const LOGS: &str = "logs";
const BACKUP: &str = "backup";
/// Label of the managed LaunchAgent running Atlas's own helper daemon.
pub const LABEL: &str = "com.insanearts.atlas.helper";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Agent {
    pub label: String,
    pub program: PathBuf,
    pub uid: u32,
    pub controller_uid: u32,
    pub socket: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Request {
    /// Report the given launchd labels and their plists.
    Inspect { uid: u32, labels: Vec<String> },
    /// Create or repair the private layout with controller search access.
    Prepare { uid: u32, controller: String },
    /// Write the managed agent's plist (or verify identical bytes) and load it.
    Install { agent: Agent },
    /// Unload a label whose plist has the expected hash and back the plist up.
    /// `missing_ok` accepts a label with no job, plist or backup at all; only
    /// cleanup of an artifact this migration may never have created sets it.
    Retire {
        uid: u32,
        label: String,
        sha256: String,
        missing_ok: bool,
    },
    /// Put the plist with the expected hash back in place and load it.
    Restore {
        uid: u32,
        label: String,
        sha256: String,
    },
}

impl Request {
    fn uid(&self) -> u32 {
        match self {
            Self::Inspect { uid, .. }
            | Self::Prepare { uid, .. }
            | Self::Retire { uid, .. }
            | Self::Restore { uid, .. } => *uid,
            Self::Install { agent } => agent.uid,
        }
    }
}

fn problem(message: impl Into<String>) -> Error {
    Error::new("setup", message)
}

/// launchd labels and account names stay within a conservative charset; they
/// become file names and launchctl service targets.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with(['.', '-'])
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
}

fn require_name(name: &str) -> Result<()> {
    if valid_name(name) {
        Ok(())
    } else {
        Err(problem(format!(
            "Invalid launchd label or account name: {name}"
        )))
    }
}

fn require_sha256(value: &str) -> Result<()> {
    if value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(problem("Invalid SHA-256"))
    }
}

/// Escape text for a plist `<string>`.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The LaunchAgent definition that runs the helper daemon `agent` describes.
pub fn plist(agent: &Agent, logs: &Path) -> String {
    let string = |value: &str| format!("<string>{}</string>", escape(value));
    let arguments = [
        agent.program.to_string_lossy().into_owned(),
        "helper-serve".into(),
        "--uid".into(),
        agent.uid.to_string(),
        "--controller-uid".into(),
        agent.controller_uid.to_string(),
        "--socket".into(),
        agent.socket.to_string_lossy().into_owned(),
    ]
    .iter()
    .map(|argument| format!("\t\t{}\n", string(argument)))
    .collect::<String>();
    let log = logs.join(format!("{}.log", agent.label));
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n\
         \t<key>Label</key>\n\t{label}\n\
         \t<key>ProgramArguments</key>\n\t<array>\n{arguments}\t</array>\n\
         \t<key>RunAtLoad</key>\n\t<true/>\n\
         \t<key>KeepAlive</key>\n\t<dict>\n\t\t<key>SuccessfulExit</key>\n\t\t<false/>\n\t</dict>\n\
         \t<key>LimitLoadToSessionType</key>\n\t<string>Aqua</string>\n\
         \t<key>ProcessType</key>\n\t<string>Interactive</string>\n\
         \t<key>StandardOutPath</key>\n\t{log}\n\
         \t<key>StandardErrorPath</key>\n\t{log}\n\
         </dict>\n</plist>\n",
        label = string(&agent.label),
        log = string(&log.to_string_lossy()),
    )
}

/// SHA-256 of text, as recorded for plists.
pub fn text_sha256(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// The managed layout inside one helper home.
pub struct Layout {
    pub uid: u32,
    pub home: PathBuf,
}

impl Layout {
    pub fn root(&self) -> PathBuf {
        self.home.join(ROOT)
    }

    pub fn logs(&self) -> PathBuf {
        self.root().join(LOGS)
    }

    pub fn daemon(&self) -> PathBuf {
        self.root().join(DAEMON)
    }

    /// The managed daemon's socket.
    pub fn socket(&self) -> PathBuf {
        self.daemon().join(format!("{}.sock", self.uid))
    }

    pub fn agent_plist(&self, label: &str) -> PathBuf {
        self.home
            .join("Library/LaunchAgents")
            .join(format!("{label}.plist"))
    }

    fn backup(&self, label: &str, sha256: &str) -> PathBuf {
        self.root()
            .join(BACKUP)
            .join(format!("{label}-{}.plist", &sha256[..16]))
    }

    fn domain(&self) -> String {
        format!("gui/{}", self.uid)
    }
}

/// Run one request as the helper and report JSON. The requested UID must be
/// this process's own; the home comes from directory services, not `$HOME`.
pub fn run(request: &str) -> Result<Value> {
    let request: Request = serde_json::from_str(request)?;
    let uid = nix::unistd::getuid().as_raw();
    if request.uid() != uid || uid < super::FIRST_USER_UID {
        return Err(problem(
            "setup-helper must run as the requested helper account",
        ));
    }
    let account = account_with_uid(uid)?.ok_or_else(|| problem("Helper account not found"))?;
    let layout = Layout {
        uid,
        home: account.home,
    };
    match request {
        Request::Inspect { labels, .. } => inspect(&layout, &labels),
        Request::Prepare { controller, .. } => prepare(&layout, &controller),
        Request::Install { agent } => install(&layout, &agent),
        Request::Retire {
            label,
            sha256,
            missing_ok,
            ..
        } => retire(&layout, &label, &sha256, missing_ok),
        Request::Restore { label, sha256, .. } => restore(&layout, &label, &sha256),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Job {
    Absent,
    Loaded { pid: Option<u32>, path: PathBuf },
}

/// Parse `launchctl print` for a job: its pid, if running, and the definition
/// path launchd recorded, which a loaded job must have.
fn parse_job(text: &str) -> Result<Job> {
    let field = |key: &str| {
        text.lines()
            .find_map(|line| line.trim().strip_prefix(key).map(str::to_owned))
    };
    let path = field("path = ")
        .filter(|path| path.starts_with('/'))
        .ok_or_else(|| problem("launchctl reported a job without a definition path"))?;
    Ok(Job::Loaded {
        pid: field("pid = ").and_then(|pid| pid.parse().ok()),
        path: PathBuf::from(path),
    })
}

/// launchctl's exit status for a service its domain does not have.
const NO_SUCH_SERVICE: i32 = 113;
/// launchctl's exit status for a domain that does not exist.
const NO_SUCH_DOMAIN: i32 = 125;

/// Fails closed: only launchctl's "no such service" and "no such desktop
/// domain" (the helper is signed out) statuses mean absent.
fn job(layout: &Layout, label: &str) -> Result<Job> {
    let output = Command::new("/bin/launchctl")
        .args(["print", &format!("{}/{label}", layout.domain())])
        .output()?;
    match output.status.code() {
        Some(0) => parse_job(&String::from_utf8_lossy(&output.stdout)),
        // When the helper is signed out its desktop domain does not exist,
        // so nothing can be loaded there.
        Some(NO_SUCH_SERVICE | NO_SUCH_DOMAIN) => Ok(Job::Absent),
        _ => Err(problem(format!(
            "launchctl could not inspect {label}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))),
    }
}

/// The pid of `label`'s job in this login, when launchd has it running.
pub(crate) fn job_pid(layout: &Layout, label: &str) -> Option<u32> {
    match job(layout, label).ok()? {
        Job::Loaded { pid, .. } => pid,
        Job::Absent => None,
    }
}

fn launchctl(arguments: &[&str]) -> Result<()> {
    let output = Command::new("/bin/launchctl").args(arguments).output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(problem(format!(
            "launchctl {} failed: {}",
            arguments[0],
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

/// Poll `label` for up to five seconds until `wanted` accepts its state,
/// returning what `wanted` takes from it; `what` names the awaited state.
fn wait_for<T>(
    layout: &Layout,
    label: &str,
    what: &str,
    wanted: impl Fn(Job) -> Option<T>,
) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(value) = wanted(job(layout, label)?) {
            return Ok(value);
        }
        if Instant::now() >= deadline {
            return Err(problem(format!("{label} did not become {what}")));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// A loaded job's pid and definition path.
fn as_loaded(job: Job) -> Option<(Option<u32>, PathBuf)> {
    match job {
        Job::Loaded { pid, path } => Some((pid, path)),
        Job::Absent => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum File {
    Absent,
    Regular(String),
    /// A symlink, directory or anything else; never followed or replaced.
    Other,
}

fn file(path: &Path) -> Result<File> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(File::Absent),
        Err(error) => Err(error.into()),
        Ok(metadata) if metadata.is_file() => file_sha256(path)
            .map(File::Regular)
            .ok_or_else(|| problem(format!("{} cannot be read", path.display()))),
        Ok(_) => Ok(File::Other),
    }
}

fn job_report(layout: &Layout, label: &str) -> Result<Value> {
    let plist = layout.agent_plist(label);
    let (loaded, pid, path) = match job(layout, label)? {
        Job::Absent => (false, None, None),
        Job::Loaded { pid, path } => (true, pid, Some(path)),
    };
    let (kind, sha256) = match file(&plist)? {
        File::Absent => ("absent", None),
        File::Regular(sha256) => ("regular", Some(sha256)),
        File::Other => ("other", None),
    };
    Ok(json!({
        "loaded": loaded,
        "pid": pid,
        "path": path,
        "plist": plist,
        "plistKind": kind,
        "plistSha256": sha256,
    }))
}

fn inspect(layout: &Layout, labels: &[String]) -> Result<Value> {
    let mut jobs = serde_json::Map::new();
    for label in labels {
        require_name(label)?;
        jobs.insert(label.clone(), job_report(layout, label)?);
    }
    Ok(json!({ "ok": true, "home": layout.home, "jobs": jobs }))
}

/// ACL rights a controller entry may carry: traversal only, no inheritance.
fn search_only(rights: &str) -> bool {
    rights
        .split(',')
        .all(|right| matches!(right, "search" | "execute"))
}

/// Check an ACL against the policy: shared directories may hold one
/// search-only allow entry for `controller`; private ones hold none. Returns
/// whether the controller entry is present; anything else is refused.
pub fn acl_policy(lines: &[String], controller: Option<u32>) -> std::result::Result<bool, String> {
    let mut granted = false;
    for line in lines {
        let fields: Vec<&str> = line.split(':').collect();
        match (fields.as_slice(), controller) {
            (["user", _, _, id, "allow", rights], Some(controller))
                if id.parse() == Ok(controller) && search_only(rights) && !granted =>
            {
                granted = true;
            }
            _ => return Err(format!("unexpected access entry {line}")),
        }
    }
    Ok(granted)
}

pub(crate) fn acl_lines(path: &Path) -> Result<Vec<String>> {
    Ok(match Acl::of_path(path)? {
        None => Vec::new(),
        Some(acl) => acl.text()?.lines().skip(1).map(str::to_owned).collect(),
    })
}

fn prepare(layout: &Layout, controller: &str) -> Result<Value> {
    require_name(controller)?;
    let controller_uid = super::account_named(controller)?
        .ok_or_else(|| problem(format!("No controller account named {controller}")))?
        .uid;
    if controller_uid == layout.uid {
        return Err(problem("The controller must be a different account"));
    }
    let root = layout.root();
    let directories = [
        (root.clone(), true),
        (root.join(DAEMON), true),
        (root.join(INPUT), true),
        (root.join(LOGS), false),
        (root.join(BACKUP), false),
    ];
    // Validate everything that exists before changing anything.
    let mut grants = Vec::new();
    for (path, shared) in &directories {
        let granted = match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.into()),
            Ok(metadata) => {
                if !metadata.is_dir() || metadata.uid() != layout.uid {
                    return Err(problem(format!(
                        "{} exists but is not a directory owned by the helper",
                        path.display()
                    )));
                }
                acl_policy(&acl_lines(path)?, shared.then_some(controller_uid)).map_err(
                    |reason| problem(format!("{}: {reason}; it was not changed", path.display())),
                )?
            }
        };
        grants.push(granted);
    }
    let mut changed = Vec::new();
    for ((path, shared), granted) in directories.iter().zip(grants) {
        match fs::DirBuilder::new().mode(0o700).create(path) {
            Ok(()) => changed.push(path.clone()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        if fs::symlink_metadata(path)?.mode() & 0o7777 != 0o700 {
            // A deleted socket directory comes back 0755 from the capture daemon.
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
            changed.push(path.clone());
        }
        if *shared && !granted {
            let status = Command::new("/bin/chmod")
                .args(["+a", &format!("user:{controller} allow search")])
                .arg(path)
                .status()?;
            if !status.success() {
                return Err(problem(format!(
                    "Could not grant {controller} search access to {}",
                    path.display()
                )));
            }
            changed.push(path.clone());
        }
    }
    changed.dedup();
    Ok(json!({ "ok": true, "root": root, "changed": changed }))
}

fn install(layout: &Layout, agent: &Agent) -> Result<Value> {
    if agent.label != LABEL || agent.uid != layout.uid || agent.socket != layout.socket() {
        return Err(problem(
            "Only the managed helper daemon in the private layout can be installed",
        ));
    }
    if !fs::symlink_metadata(&agent.program).is_ok_and(|metadata| metadata.is_file())
        || !agent.program.is_absolute()
    {
        return Err(problem(
            "The helper daemon program must be an absolute regular file",
        ));
    }
    let text = plist(agent, &layout.logs());
    let path = layout.agent_plist(LABEL);
    match file(&path)? {
        File::Regular(sha256) if sha256 == text_sha256(&text) => {}
        File::Absent => {
            fs::create_dir_all(path.parent().unwrap_or(&layout.home))?;
            crate::config::write_atomic(&path, text.as_bytes(), 0o644)?;
        }
        _ => {
            return Err(problem(format!(
                "{} exists with other contents; it was not changed",
                path.display()
            )));
        }
    }
    let pid = loaded(layout, LABEL, &path)?;
    Ok(json!({ "ok": true, "plist": path, "pid": pid, "sha256": text_sha256(&text) }))
}

/// Load `label` from `path` unless launchd has it already; its pid, once
/// running. A job loaded from another definition is an error.
fn loaded(layout: &Layout, label: &str, path: &Path) -> Result<Option<u32>> {
    let (pid, from) = match as_loaded(job(layout, label)?) {
        Some(loaded) => loaded,
        None => {
            launchctl(&["bootstrap", &layout.domain(), &path.to_string_lossy()])?;
            wait_for(layout, label, "loaded", as_loaded)?
        }
    };
    if from != path {
        return Err(problem(format!(
            "{label} is loaded from {}",
            from.display()
        )));
    }
    Ok(pid)
}

fn retire(layout: &Layout, label: &str, sha256: &str, missing_ok: bool) -> Result<Value> {
    require_name(label)?;
    require_sha256(sha256)?;
    let path = layout.agent_plist(label);
    let backup = layout.backup(label, sha256);
    let plist = file(&path)?;
    // Validate the definition before unloading anything.
    match &plist {
        File::Regular(current) if current == sha256 => {}
        File::Absent if file(&backup)? == File::Regular(sha256.to_owned()) => {}
        // Never installed: nothing to undo, and nothing to preserve.
        File::Absent if missing_ok && job(layout, label)? == Job::Absent => {
            return Ok(json!({ "ok": true, "label": label, "backup": null, "absent": true }));
        }
        _ => {
            return Err(problem(format!(
                "{} is not the expected definition; nothing was changed",
                path.display()
            )));
        }
    }
    if let Job::Loaded { pid, path: loaded } = job(layout, label)? {
        // Unloading the job that runs this worker would sever its own reply.
        if pid == u32::try_from(nix::unistd::getppid().as_raw()).ok() {
            return Err(problem(format!(
                "Refusing to unload {label} through its own capture daemon"
            )));
        }
        if loaded != path {
            return Err(problem(format!(
                "{label} is loaded from {}; it was not changed",
                loaded.display()
            )));
        }
        launchctl(&["bootout", &format!("{}/{label}", layout.domain())])?;
        wait_for(layout, label, "unloaded", |job| {
            (job == Job::Absent).then_some(())
        })?;
    }
    if let File::Regular(_) = plist {
        match file(&backup)? {
            File::Absent => fs::rename(&path, &backup)?,
            // Identical bytes are already preserved; the source may go.
            File::Regular(saved) if saved == sha256 => fs::remove_file(&path)?,
            _ => {
                return Err(problem(format!(
                    "{} exists with other contents",
                    backup.display()
                )));
            }
        }
    }
    Ok(json!({ "ok": true, "label": label, "backup": backup, "sha256": sha256 }))
}

fn restore(layout: &Layout, label: &str, sha256: &str) -> Result<Value> {
    require_name(label)?;
    require_sha256(sha256)?;
    let path = layout.agent_plist(label);
    let backup = layout.backup(label, sha256);
    match file(&path)? {
        File::Regular(current) if current == sha256 => {}
        File::Absent if file(&backup)? == File::Regular(sha256.to_owned()) => {
            fs::create_dir_all(path.parent().unwrap_or(&layout.home))?;
            fs::rename(&backup, &path)?;
        }
        File::Absent => return Err(problem(format!("No backup of {label} with that hash"))),
        _ => {
            return Err(problem(format!(
                "{} exists with other contents; it was not changed",
                path.display()
            )));
        }
    }
    let pid = loaded(layout, label, &path)?;
    Ok(json!({ "ok": true, "plist": path, "pid": pid }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_conservative() {
        for name in ["dev.atlas.capture", "theiskaa", "a_b-c.1"] {
            assert!(valid_name(name), "{name}");
        }
        for name in [
            "",
            ".hidden",
            "-flag",
            "a/b",
            "a b",
            "../x",
            "a\nb",
            &"x".repeat(129),
        ] {
            assert!(!valid_name(name), "{name:?}");
        }
    }

    #[test]
    fn plist_is_valid_and_escapes_text() {
        let agent = Agent {
            label: LABEL.into(),
            program: "/Users/Shared/A&B <x>/sessiond".into(),
            uid: 502,
            controller_uid: 501,
            socket: "/Users/h/.atlas/helper/502.sock".into(),
        };
        let text = plist(&agent, Path::new("/Users/h/.atlas/logs"));
        assert!(text.contains("<string>/Users/Shared/A&amp;B &lt;x&gt;/sessiond</string>"));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.plist");
        fs::write(&path, &text).unwrap();
        assert_eq!(file(&path).unwrap(), File::Regular(text_sha256(&text)));
        let lint = Command::new("/usr/bin/plutil")
            .arg("-lint")
            .arg(&path)
            .output()
            .unwrap();
        assert!(lint.status.success());
        let json = Command::new("/usr/bin/plutil")
            .args(["-convert", "json", "-o", "-"])
            .arg(&path)
            .output()
            .unwrap();
        let parsed: Value = serde_json::from_slice(&json.stdout).unwrap();
        assert_eq!(parsed["Label"], LABEL);
        assert_eq!(
            parsed["ProgramArguments"][0],
            "/Users/Shared/A&B <x>/sessiond"
        );
        assert_eq!(parsed["ProgramArguments"][1], "helper-serve");
        assert_eq!(parsed["ProgramArguments"][3], "502");
        assert_eq!(parsed["ProgramArguments"][5], "501");
        assert_eq!(
            parsed["ProgramArguments"][7],
            "/Users/h/.atlas/helper/502.sock"
        );
        assert_eq!(parsed["ProcessType"], "Interactive");
        assert_eq!(parsed["KeepAlive"]["SuccessfulExit"], false);
        assert_eq!(parsed["LimitLoadToSessionType"], "Aqua");
    }

    #[test]
    fn files_distinguish_absent_regular_and_everything_else() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p");
        assert_eq!(file(&path).unwrap(), File::Absent);
        fs::write(&path, "abc").unwrap();
        assert!(matches!(file(&path).unwrap(), File::Regular(_)));
        let link = dir.path().join("l");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert_eq!(file(&link).unwrap(), File::Other);
        let dangling = dir.path().join("d");
        std::os::unix::fs::symlink(dir.path().join("missing"), &dangling).unwrap();
        assert_eq!(file(&dangling).unwrap(), File::Other);
        assert_eq!(file(dir.path()).unwrap(), File::Other);
    }

    #[test]
    fn launchctl_print_needs_a_definition_path() {
        let text = "gui/502/x = {\n\tpath = /Users/h/Library/LaunchAgents/x.plist\n\
                    \tstate = running\n\tpid = 42\n}";
        assert_eq!(
            parse_job(text).unwrap(),
            Job::Loaded {
                pid: Some(42),
                path: "/Users/h/Library/LaunchAgents/x.plist".into()
            }
        );
        assert!(parse_job("gui/502/x = {\n\tstate = waiting\n}").is_err());
        assert!(parse_job("\tpath = relative.plist\n").is_err());
    }

    #[test]
    fn acl_policy_allows_only_one_search_grant_for_the_controller() {
        let entry = |id: u32, kind: &str, rights: &str| format!("user:U:n:{id}:{kind}:{rights}");
        assert_eq!(acl_policy(&[], Some(501)), Ok(false));
        assert_eq!(
            acl_policy(&[entry(501, "allow", "search")], Some(501)),
            Ok(true)
        );
        assert_eq!(
            acl_policy(&[entry(501, "allow", "execute")], Some(501)),
            Ok(true)
        );
        for lines in [
            vec![entry(501, "allow", "list")],
            vec![entry(501, "allow", "search,list")],
            vec![entry(501, "allow", "search,file_inherit")],
            vec![entry(501, "deny", "search")],
            vec![entry(503, "allow", "search")],
            vec![entry(501, "allow", "search"), entry(501, "allow", "search")],
            vec!["group:U:everyone:12:allow:search".to_owned()],
        ] {
            assert!(acl_policy(&lines, Some(501)).is_err(), "{lines:?}");
        }
        assert!(acl_policy(&[entry(501, "allow", "search")], None).is_err());
    }

    #[test]
    fn requests_are_typed_and_strict() {
        let request: Request = serde_json::from_str(
            r#"{"action":"retire","uid":502,"label":"dev.atlas.live-sessiond","sha256":"00","missing_ok":false}"#,
        )
        .unwrap();
        assert_eq!(request.uid(), 502);
        assert!(
            serde_json::from_str::<Request>(r#"{"action":"retire","uid":502,"label":"x"}"#)
                .is_err()
        );
        assert!(serde_json::from_str::<Request>(r#"{"action":"delete","uid":502}"#).is_err());
        assert!(require_sha256("00").is_err());
        assert!(require_sha256(&"a".repeat(64)).is_ok());
    }

    #[test]
    fn the_worker_refuses_another_account() {
        let other = nix::unistd::getuid().as_raw() + 1;
        let request = json!({"action": "inspect", "uid": other, "labels": []}).to_string();
        assert_eq!(run(&request).unwrap_err().code, "setup");
    }

    #[test]
    fn install_accepts_only_the_managed_agent() {
        let layout = Layout {
            uid: nix::unistd::getuid().as_raw(),
            home: "/nonexistent-home".into(),
        };
        let agent = |label: &str, socket: PathBuf| Agent {
            label: label.into(),
            program: "/bin/sh".into(),
            uid: layout.uid,
            controller_uid: 0,
            socket,
        };
        for agent in [
            agent("other.label", layout.socket()),
            agent(LABEL, layout.root().join("helper/../elsewhere.sock")),
            agent(LABEL, layout.daemon().join("other.sock")),
            agent(LABEL, "/tmp/x".into()),
        ] {
            assert!(install(&layout, &agent).is_err());
        }
    }

    fn sandbox() -> (tempfile::TempDir, Layout, String) {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout {
            uid: nix::unistd::getuid().as_raw(),
            home: fs::canonicalize(dir.path()).unwrap(),
        };
        fs::create_dir_all(layout.home.join("Library/LaunchAgents")).unwrap();
        // A label launchd has never seen: inspection is real and read-only.
        let label = format!("dev.atlas.test.{}", &crate::control::token()[..12]);
        (dir, layout, label)
    }

    #[test]
    fn retire_backs_up_exactly_the_expected_plist_and_repeats_its_receipt() {
        let (_dir, layout, label) = sandbox();
        prepare(&layout, "root").unwrap();
        let path = layout.agent_plist(&label);
        fs::write(&path, "definition").unwrap();
        let sha256 = text_sha256("definition");
        assert!(retire(&layout, &label, &"0".repeat(64), false).is_err());
        assert!(path.exists(), "a mismatched plist is left in place");
        let first = retire(&layout, &label, &sha256, false).unwrap();
        assert!(!path.exists());
        assert_eq!(
            fs::read_to_string(first["backup"].as_str().unwrap()).unwrap(),
            "definition"
        );
        assert_eq!(retire(&layout, &label, &sha256, false).unwrap(), first);
        let other = format!("{label}.other");
        assert!(
            retire(&layout, &other, &sha256, false).is_err(),
            "the original must be preserved"
        );
        assert_eq!(
            retire(&layout, &other, &sha256, true).unwrap()["absent"],
            true
        );
    }

    #[test]
    fn retire_and_restore_never_touch_symlinks_or_other_contents() {
        let (_dir, layout, label) = sandbox();
        prepare(&layout, "root").unwrap();
        let path = layout.agent_plist(&label);
        let target = layout.home.join("real.plist");
        fs::write(&target, "definition").unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        let sha256 = text_sha256("definition");
        assert!(retire(&layout, &label, &sha256, true).is_err());
        assert!(restore(&layout, &label, &sha256).is_err());
        assert!(
            fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        fs::remove_file(&path).unwrap();
        assert!(
            restore(&layout, &label, &sha256).is_err(),
            "no backup to restore"
        );
        fs::write(&path, "changed").unwrap();
        assert!(restore(&layout, &label, &sha256).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "changed");
    }

    #[test]
    fn prepare_creates_repairs_and_refuses_foreign_access() {
        let (_dir, layout, _) = sandbox();
        prepare(&layout, "root").unwrap();
        let capture = layout.daemon();
        let mode = |path: &Path| fs::metadata(path).unwrap().mode() & 0o777;
        assert_eq!(mode(&capture), 0o700);
        assert_eq!(acl_policy(&acl_lines(&capture).unwrap(), Some(0)), Ok(true));
        assert!(acl_lines(&layout.logs()).unwrap().is_empty());
        fs::set_permissions(&capture, fs::Permissions::from_mode(0o755)).unwrap();
        let repaired = prepare(&layout, "root").unwrap();
        assert_eq!(repaired["changed"], json!([capture]));
        assert_eq!(mode(&capture), 0o700);
        assert_eq!(
            acl_lines(&capture).unwrap().len(),
            1,
            "the grant is not duplicated"
        );
        let user = std::env::var("USER").unwrap();
        Command::new("/bin/chmod")
            .args(["+a", &format!("user:{user} allow list")])
            .arg(layout.logs())
            .status()
            .unwrap();
        fs::set_permissions(&capture, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(prepare(&layout, "root").is_err());
        assert_eq!(
            mode(&capture),
            0o755,
            "nothing changes when validation fails"
        );
    }
}
