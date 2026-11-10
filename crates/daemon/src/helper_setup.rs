//! Typed setup actions executed *inside* the helper login (`setup-helper`).
//! Every action inspects before it acts, fails closed when inspection fails,
//! and is safe to repeat. Plists are only moved into the private backup
//! directory, never deleted, and only when their bytes are the expected ones.
//!
//! Nothing here reaches the controller's client: the helper's daemon shares
//! this layout, and the controller's setup drives these actions from the other side.
pub mod first_login;
pub mod identity;
mod launchd;
pub mod pairing;

use atlas_core::{
    account::{Account, account_named, account_with_uid},
    acl::{acl_lines, acl_policy},
    digest::file_sha256,
    error::{Code, Error, Result},
};
use identity::Identity;
pub use launchd::{Definition, Job, JobReport, job_pid};
use launchd::{as_loaded, job, launchctl, wait_for};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
};

pub fn problem(message: impl Into<String>) -> Error {
    Error::new(Code::Setup, message)
}

/// Private Atlas directory in the helper home and its entries.
pub const ROOT: &str = ".atlas";
/// Where the helper daemon's socket lives.
const DAEMON: &str = "helper";
const INPUT: &str = "input";
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

impl Agent {
    /// The managed job running `program` for `controller_uid` in `layout`.
    pub fn managed(layout: &Layout, program: PathBuf, controller_uid: u32) -> Self {
        Self {
            label: LABEL.to_owned(),
            program,
            uid: layout.uid,
            controller_uid,
            socket: layout.socket(),
        }
    }

    pub fn plist_sha256(&self, logs: &Path) -> String {
        text_sha256(&plist(self, logs))
    }
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
    /// cleanup of an artifact setup may never have created sets it.
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
    /// Ready the account for its next login: macOS's setup questions marked
    /// answered while it runs nothing, and, with `opener`, the one-time job
    /// that opens Atlas there (started now when it is logged in).
    FirstLogin { uid: u32, opener: bool },
    /// Take away a waiting opener.
    ForgetOpener { uid: u32 },
    /// Start the managed helper job after installation if launchd has not.
    Kickstart { uid: u32 },
}

impl Request {
    fn uid(&self) -> u32 {
        match self {
            Self::Inspect { uid, .. }
            | Self::Prepare { uid, .. }
            | Self::Retire { uid, .. }
            | Self::Restore { uid, .. }
            | Self::FirstLogin { uid, .. }
            | Self::ForgetOpener { uid }
            | Self::Kickstart { uid } => *uid,
            Self::Install { agent } => agent.uid,
        }
    }

    /// Whether running this needs an executor claim: everything but a look.
    pub fn requires_claim(&self) -> bool {
        match self {
            Self::Inspect { .. } => false,
            Self::Prepare { .. }
            | Self::Install { .. }
            | Self::Retire { .. }
            | Self::Restore { .. }
            | Self::FirstLogin { .. }
            | Self::ForgetOpener { .. }
            | Self::Kickstart { .. } => true,
        }
    }

    /// Whether the acceptance worker may run this for the requesting account.
    pub fn allowed_in_acceptance(&self) -> bool {
        match self {
            Self::FirstLogin { .. } => false,
            Self::Inspect { .. }
            | Self::Prepare { .. }
            | Self::Install { .. }
            | Self::Retire { .. }
            | Self::Restore { .. }
            | Self::ForgetOpener { .. }
            | Self::Kickstart { .. } => true,
        }
    }
}

/// A refusal that changed nothing, as the worker reports it: `unchanged`
/// tells the caller it is certain, so no pending marker waits on it.
fn refused(message: impl Into<String>) -> Value {
    json!({ "ok": false, "code": "setup", "error": message.into(), "unchanged": true })
}

/// Why a finished worker's reply does not acknowledge its request.
#[derive(Debug)]
pub enum Unfinished {
    /// It printed no JSON (with the decoding failure), or only null.
    Silent(Option<serde_json::Error>),
    /// A refusal it says changed nothing, so the outcome is certain.
    Unchanged(Option<String>),
    /// Any other reply: it may have changed state part way.
    Uncertain(Option<String>),
}

impl Unfinished {
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Silent(_) => None,
            Self::Unchanged(why) | Self::Uncertain(why) => why.as_deref(),
        }
    }

    /// `error`, keeping the decoding failure behind a silent worker.
    pub fn behind(self, error: Error) -> Error {
        match self {
            Self::Silent(Some(cause)) => error.with_source(cause),
            Self::Silent(None) | Self::Unchanged(_) | Self::Uncertain(_) => error,
        }
    }
}

/// A worker's reply, decoded once: `Ok` only when it says `ok: true`. The
/// result fields stay in the reply, each action's own.
///
/// # Errors
///
/// Returns [`Unfinished`] when the reply does not say `ok: true`.
pub fn acknowledged(reply: Value) -> std::result::Result<Value, Unfinished> {
    if reply["ok"] == true {
        return Ok(reply);
    }

    let why = reply["error"].as_str().map(str::to_owned);
    Err(if reply.is_null() {
        Unfinished::Silent(None)
    } else if reply["ok"] == false && reply["unchanged"] == true {
        Unfinished::Unchanged(why)
    } else {
        Unfinished::Uncertain(why)
    })
}

/// launchd labels and account names stay within a conservative charset; they
/// become file names and launchctl service targets.
fn valid_name(name: &str) -> bool {
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

pub(super) fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The common XML envelope; callers own the dictionary's exact entries.
pub(super) fn document(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n{body}</dict>\n</plist>\n"
    )
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

    document(&format!(
        "\t<key>Label</key>\n\t{label}\n\
         \t<key>ProgramArguments</key>\n\t<array>\n{arguments}\t</array>\n\
         \t<key>RunAtLoad</key>\n\t<true/>\n\
         \t<key>KeepAlive</key>\n\t<dict>\n\t\t<key>SuccessfulExit</key>\n\t\t<false/>\n\t</dict>\n\
         \t<key>LimitLoadToSessionType</key>\n\t<string>Aqua</string>\n\
         \t<key>ProcessType</key>\n\t<string>Interactive</string>\n\
         \t<key>StandardOutPath</key>\n\t{log}\n\
         \t<key>StandardErrorPath</key>\n\t{log}\n",
        label = string(&agent.label),
        log = string(&log.to_string_lossy()),
    ))
}

pub(crate) fn text_sha256(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// The managed layout inside one helper home.
pub struct Layout {
    pub uid: u32,
    pub home: PathBuf,
    /// Expected account for a setup worker; ordinary read-only layouts have none.
    pub guard: Option<Guard>,
}

impl Layout {
    /// Describe an account's paths without granting permission to mutate them.
    pub fn new(uid: u32, home: PathBuf) -> Self {
        Self {
            uid,
            home,
            guard: None,
        }
    }

    /// Revalidate the worker's account and home after all waiting checks,
    /// immediately before a filesystem or launchd mutation.
    pub(super) fn check(&self) -> Result<()> {
        if let Some(guard) = &self.guard {
            let account = checked_account(
                guard,
                || observe(self.uid),
                || atlas_core::session::confirm_gone(&[self.uid]),
                || atlas_core::session::logged_out(self.uid) == Some(true),
            )?;
            if account.home != self.home {
                return Err(problem(
                    "The helper's home changed, so nothing more was changed",
                ));
            }
        }
        Ok(())
    }

    pub fn root(&self) -> PathBuf {
        self.home.join(ROOT)
    }

    pub fn logs(&self) -> PathBuf {
        self.root().join(LOGS)
    }

    pub fn daemon(&self) -> PathBuf {
        self.root().join(DAEMON)
    }

    pub fn input_socket(&self) -> PathBuf {
        self.root().join(INPUT).join("input.sock")
    }

    /// The input service's saved stop, beside its socket: present while the
    /// person has stopped agents.
    pub(crate) fn stop_marker(&self) -> PathBuf {
        self.input_socket().with_extension("stopped")
    }

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

/// What a request checks of its own account before it does anything.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Guard {
    /// The required identity, as recorded.
    pub account: Option<Identity>,
    /// The account must be logged out: no loginwindow and no Dock.
    pub logged_out: bool,
}

/// An account as directory services show it, and its persistent identity.
type Observed = (Account, Option<Identity>);

/// Read the account, its home, and its persistent identity.
/// A changed directory mapping is refused, rather than joining two accounts' facts.
fn observe(uid: u32) -> Result<Observed> {
    let account = account_with_uid(uid)?.ok_or_else(|| problem("Helper account not found"))?;
    let identity = identity::current(&account)
        .ok_or_else(|| problem("macOS could not read the helper's account identity"))?;
    if account_with_uid(uid)?.as_ref() != Some(&account) {
        return Err(problem("The helper account changed while it was checked"));
    }
    Ok((account, Some(identity)))
}

/// Resolve the expected account again after logout verification, then observe
/// processes once more. No identity or home captured before the wait is trusted.
fn checked_account(
    guard: &Guard,
    mut read: impl FnMut() -> Result<Observed>,
    wait: impl FnOnce() -> bool,
    out_now: impl FnOnce() -> bool,
) -> Result<Account> {
    let expected = guard.account.as_ref().ok_or_else(|| {
        problem("setup-helper requires an expected account identity; nothing was changed")
    })?;
    let check = |account: &Account, identity: &Option<Identity>| {
        if identity.as_ref() != Some(expected)
            || account.uid != expected.uid
            || account.name != expected.name
        {
            Err(problem(
                "This account is no longer the helper Atlas chose, so nothing more was changed",
            ))
        } else {
            Ok(())
        }
    };
    let (account, identity) = read()?;
    check(&account, &identity)?;
    if guard.logged_out && !wait() {
        return Err(problem(
            "The helper is logged in again, so nothing more was changed",
        ));
    }
    let (account, identity) = read()?;
    check(&account, &identity)?;
    if guard.logged_out && !out_now() {
        return Err(problem(
            "The helper is logged in again, so nothing more was changed",
        ));
    }
    Ok(account)
}

/// Run setup only as the expected account, requiring and revalidating its
/// identity and home after waits and before each mutation.
///
/// # Errors
///
/// Rejects changed account identity or an unsuitable login, and returns layout, ACL or
/// launchd failures. Unknown effects remain recorded by the caller's executor claim.
pub fn run_guarded(request: Request, guard: &Guard) -> Result<Value> {
    let uid = nix::unistd::getuid().as_raw();
    run_guarded_with(
        request,
        guard,
        || observe(uid),
        || atlas_core::session::confirm_gone(&[uid]),
        || atlas_core::session::logged_out(uid) == Some(true),
        execute,
    )
}

fn run_guarded_with(
    request: Request,
    guard: &Guard,
    read: impl FnMut() -> Result<Observed>,
    wait: impl FnOnce() -> bool,
    out_now: impl FnOnce() -> bool,
    execute: impl FnOnce(Request, &Layout) -> Result<Value>,
) -> Result<Value> {
    let uid = nix::unistd::getuid().as_raw();
    // Never as root with the helper's real user ID: files would be root's,
    // written in a home the helper controls.
    if request.uid() != uid
        || uid < atlas_core::account::FIRST_USER_UID
        || nix::unistd::geteuid() != nix::unistd::getuid()
    {
        return Err(problem(
            "setup-helper must run as the requested helper account",
        ));
    }

    let account = checked_account(guard, read, wait, out_now)?;
    let layout = Layout {
        uid,
        home: account.home,
        guard: Some(guard.clone()),
    };

    execute(request, &layout)
}

fn execute(request: Request, layout: &Layout) -> Result<Value> {
    match request {
        Request::Inspect { labels, .. } => inspect(layout, &labels),
        Request::Prepare { controller, .. } => prepare(layout, &controller),
        Request::Install { agent } => install(layout, &agent),
        Request::Retire {
            label,
            sha256,
            missing_ok,
            ..
        } => retire(layout, &label, &sha256, missing_ok),
        Request::Restore { label, sha256, .. } => restore(layout, &label, &sha256),
        Request::FirstLogin { opener, .. } => first_login::first_login(layout, opener),
        Request::ForgetOpener { .. } => first_login::forget_opener(layout),
        Request::Kickstart { uid } => kickstart(layout, uid),
    }
}

/// Start the managed job launchd has not started on its own.
fn kickstart(layout: &Layout, uid: u32) -> Result<Value> {
    layout.check()?;
    let output = atlas_core::spawn::output(
        Command::new("/bin/launchctl").args(["kickstart", &format!("gui/{uid}/{LABEL}")]),
    )?;
    if !output.status.success() {
        return Err(
            problem("The helper job did not start; reopen Atlas in the helper").uncertain(),
        );
    }
    Ok(json!({"ok":true}))
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum File {
    Absent,
    Regular(String),
    /// A symlink, directory or anything else; never followed or replaced.
    Other,
}

pub(super) fn file(path: &Path) -> Result<File> {
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

fn prepare(layout: &Layout, controller: &str) -> Result<Value> {
    require_name(controller)?;
    let controller_uid = account_named(controller)?
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
        layout.check()?;
        match fs::DirBuilder::new().mode(0o700).create(path) {
            Ok(()) => changed.push(path.clone()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }

        if fs::symlink_metadata(path)?.mode() & 0o7777 != 0o700 {
            // A deleted socket directory comes back 0755 from the capture daemon.
            layout.check()?;
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
            changed.push(path.clone());
        }

        if *shared && !granted {
            layout.check()?;
            let status = atlas_core::spawn::status(
                Command::new("/bin/chmod")
                    .args(["+a", &format!("user:{controller} allow search")])
                    .arg(path),
            )?;
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

/// Write the managed agent's plist (or find identical bytes) and load it. A
/// refusal before anything is written says it changed nothing.
fn install(layout: &Layout, agent: &Agent) -> Result<Value> {
    if agent.label != LABEL || agent.uid != layout.uid || agent.socket != layout.socket() {
        return Ok(refused(
            "Only the managed helper daemon in the private layout can be installed",
        ));
    }

    if !fs::symlink_metadata(&agent.program).is_ok_and(|metadata| metadata.is_file())
        || !agent.program.is_absolute()
    {
        return Ok(refused(
            "The helper daemon program must be an absolute regular file",
        ));
    }

    let text = plist(agent, &layout.logs());
    let path = layout.agent_plist(LABEL);
    match file(&path)? {
        File::Regular(sha256) if sha256 == text_sha256(&text) => {}
        File::Absent => {
            layout.check()?;
            fs::create_dir_all(path.parent().unwrap_or(&layout.home))?;
            layout.check()?;
            atlas_core::files::write_atomic(&path, text.as_bytes(), 0o644)?;
        }
        // An earlier Atlas's definition is replaced only from outside the
        // daemon it runs: atlas init does that, with a backup.
        File::Regular(_) | File::Other => {
            return Ok(refused(format!(
                "The helper's background job was set up by another version of Atlas, so {} \
                 was not changed. Run atlas init to replace it.",
                path.display()
            )));
        }
    }

    let pid = loaded(layout, LABEL, &path)?;
    Ok(json!({ "ok": true, "plist": path, "pid": pid, "sha256": text_sha256(&text) }))
}

/// Load `label` from `path` unless launchd has it already; its pid, once
/// running. A job loaded from another definition is an error.
pub(super) fn loaded(layout: &Layout, label: &str, path: &Path) -> Result<Option<u32>> {
    let (pid, from) = match as_loaded(job(layout, label)?) {
        Some(loaded) => loaded,
        None => {
            layout.check()?;
            launchctl("bootstrap", &[&layout.domain(), &path.to_string_lossy()])?;
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

pub(super) fn unload(layout: &Layout, label: &str) -> Result<()> {
    if job(layout, label)? != Job::Absent {
        bootout(layout, label)?;
    }

    Ok(())
}

fn bootout(layout: &Layout, label: &str) -> Result<()> {
    layout.check()?;
    launchctl("bootout", &[&format!("{}/{label}", layout.domain())])?;
    wait_for(layout, label, "unloaded", |job| {
        (job == Job::Absent).then_some(())
    })
}

/// Whether `label`'s definition, its backup and the job read by `read_job`
/// let retirement go ahead, with the definition and job it found.
fn retirement_with(
    layout: &Layout,
    label: &str,
    sha256: &str,
    missing_ok: bool,
    read_job: impl FnOnce() -> Result<Job>,
) -> Result<(File, Job)> {
    require_name(label)?;
    require_sha256(sha256)?;

    let path = layout.agent_plist(label);
    let backup = layout.backup(label, sha256);
    let plist = file(&path)?;
    let saved = file(&backup)?;
    let job = read_job()?;

    match (&plist, &saved) {
        (File::Regular(current), File::Absent) if current == sha256 => {}
        (File::Regular(current), File::Regular(saved)) if current == sha256 && saved == sha256 => {}
        (File::Absent, File::Regular(saved)) if saved == sha256 => {}
        (File::Absent, File::Absent) if missing_ok && job == Job::Absent => {}
        _ => {
            return Err(problem(format!(
                "{} is not the expected definition; nothing was changed",
                path.display()
            )));
        }
    }
    if let Job::Loaded { path: loaded, .. } = &job
        && *loaded != path
    {
        return Err(problem(format!(
            "{label} is loaded from {}; it was not changed",
            loaded.display()
        )));
    }
    Ok((plist, job))
}

fn retire(layout: &Layout, label: &str, sha256: &str, missing_ok: bool) -> Result<Value> {
    let (plist, observed) =
        retirement_with(layout, label, sha256, missing_ok, || job(layout, label))?;
    let path = layout.agent_plist(label);
    let backup = layout.backup(label, sha256);
    if missing_ok && plist == File::Absent && file(&backup)? == File::Absent {
        return Ok(json!({ "ok": true, "label": label, "backup": null, "absent": true }));
    }

    if let Job::Loaded { pid, .. } = observed {
        // Unloading the job that runs this worker would sever its own reply.
        if pid == u32::try_from(nix::unistd::getppid().as_raw()).ok() {
            return Err(problem(format!(
                "Refusing to unload {label} through its own capture daemon"
            )));
        }

        bootout(layout, label)?;
    }

    if let File::Regular(_) = plist {
        layout.check()?;
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
            layout.check()?;
            fs::create_dir_all(path.parent().unwrap_or(&layout.home))?;
            layout.check()?;
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
#[path = "helper_setup_tests.rs"]
mod tests;
