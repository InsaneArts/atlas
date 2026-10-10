//! Side-by-side migration of the capture endpoint into the private layout.
//!
//! A managed LaunchAgent runs Understudy's own signed helper daemon on a
//! persistent private socket directory beside the existing capture job, which
//! it replaces only once verified. Each pass
//! reconciles reality toward the target instead of trusting completion flags:
//! every helper action is idempotent, re-inspects first and runs under a
//! pending marker recorded in a durable log, so an interrupted pass resumes or
//! rolls back without blind replays. The job carrying a reply is never the one
//! being unloaded, and only artifacts this migration created are undone.
use super::{
    account_with_uid, endpoint, executable_of,
    helper::{Agent, LABEL, Layout, Request, plist, text_sha256},
    helper_job, running_developer_id, signature,
};
use crate::{
    client::{Client, timestamp},
    config::{Config, home},
    control::Ownership,
    error::{Error, Result},
    session::{input_current, require_hidden},
    transport::run_output,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Plan {
    pub uid: u32,
    pub controller: String,
    pub home: PathBuf,
    pub old_label: String,
    pub old_plist_sha256: String,
    /// The managed label. Empty in logs from before the own daemon, whose
    /// managed job ran the previous capture daemon.
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub controller_uid: u32,
    pub program: PathBuf,
    pub cdhash: String,
    pub config_path: PathBuf,
    pub old_config: String,
    pub new_config: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum State {
    Applying,
    Applied,
    RollingBack,
    RolledBack,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Log {
    pub version: u32,
    pub plan: Plan,
    pub state: State,
    /// Pending-mutation generations this migration recorded and still owns.
    pub pending: Vec<String>,
    /// Completed actions with nanosecond timestamps, for evidence only.
    pub history: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Endpoint {
    Old,
    New,
}

fn problem(message: impl Into<String>) -> Error {
    Error::new("setup", message)
}

fn log_path(uid: u32) -> Result<PathBuf> {
    Ok(home()?
        .join("Library/Application Support/Understudy")
        .join(format!("setup-{uid}.json")))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| problem("Path has no parent directory"))?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    let temporary = path.with_extension(format!("{}.new", crate::control::token()));
    let result = (|| -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn load_log(path: &Path) -> Result<Option<Log>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn save_log(path: &Path, log: &Log) -> Result<()> {
    write_atomic(path, &serde_json::to_vec_pretty(log)?)
}

/// The configuration with sockets moved into the private layout.
pub fn managed(config: &Config, layout: &Layout) -> Config {
    Config {
        socket_path: layout.socket(),
        input_socket: layout.root().join(super::helper::INPUT).join("input.sock"),
        ..config.clone()
    }
}

fn helper_argv(client: &Client, request: &Request) -> Result<Vec<String>> {
    Ok(vec![
        client.config.identity_probe.to_string_lossy().into_owned(),
        "setup-helper".to_owned(),
        serde_json::to_string(request)?,
    ])
}

/// The worker's JSON result from a completed helper run.
fn helper_result(reply: &Value) -> Result<Value> {
    let output = run_output(reply, 65536)
        .map_err(|_| problem("The helper setup command returned malformed output"))?;
    let value: Value = serde_json::from_slice(&output)
        .map_err(|_| problem("The helper setup command returned no result"))?;
    if value["ok"] != true {
        return Err(problem(format!(
            "Helper setup refused: {}",
            value["error"].as_str().unwrap_or("unknown error")
        )));
    }
    Ok(value)
}

/// The PID of the input service answering on `client`'s input socket: None
/// only when the socket is missing or refuses connections.
async fn input_presence(client: &Client) -> Result<Option<u32>> {
    match tokio::net::UnixStream::connect(&client.config.input_socket).await {
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
            ) =>
        {
            return Ok(None);
        }
        _ => {}
    }
    let hello = client
        .input_transport()
        .call(&json!({ "op": "hello" }))
        .await
        .map_err(|error| problem(format!("Input service state is unknown: {}", error.error)))?;
    hello["pid"]
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok())
        .filter(|_| hello["ok"] == true)
        .map(Some)
        .ok_or_else(|| problem("Input service state is unknown"))
}

async fn reachable(client: &Client) -> bool {
    require_hidden(&client.status().await, client.config.helper_uid).is_ok()
}

async fn await_endpoint(client: &Client) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !reachable(client).await {
        if Instant::now() >= deadline {
            return Err(problem(format!(
                "No verified capture daemon answered at {}",
                client.config.socket_path.display()
            )));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Ok(())
}

struct Migration {
    path: PathBuf,
    log: Log,
    old: Client,
    new: Client,
}

impl Migration {
    fn client(&self, endpoint: Endpoint) -> &Client {
        match endpoint {
            Endpoint::Old => &self.old,
            Endpoint::New => &self.new,
        }
    }
    fn layout(&self) -> Layout {
        Layout {
            uid: self.log.plan.uid,
            home: self.log.plan.home.clone(),
        }
    }
    fn agent(&self) -> Agent {
        Agent {
            label: self.log.plan.label.clone(),
            program: self.log.plan.program.clone(),
            uid: self.log.plan.uid,
            controller_uid: self.log.plan.controller_uid,
            socket: self.layout().socket(),
        }
    }
    fn note(&mut self, action: &str) -> Result<()> {
        self.log
            .history
            .push((action.to_owned(), timestamp().to_string()));
        save_log(&self.path, &self.log)
    }
    /// Claim a generation in the log, then record the pending mutation under
    /// it. A crash between the two leaves only a harmless log-only claim.
    /// The claim is held while the step runs.
    fn begin(&mut self) -> Result<crate::control::Pending> {
        let generation = crate::control::token();
        self.log.pending.push(generation.clone());
        save_log(&self.path, &self.log)?;
        self.old.controller.quarantine_as(&generation)
    }
    fn settle(&mut self, generation: &str) -> Result<()> {
        self.old.controller.clear_if(generation)?;
        self.log.pending.retain(|entry| entry != generation);
        save_log(&self.path, &self.log)
    }
    /// Settle a definitive outcome; keep an uncertain one pending.
    fn conclude<T>(&mut self, generation: &str, outcome: Result<T>) -> Result<T> {
        match outcome {
            Err(error) if error.uncertain => Err(error),
            outcome => {
                self.settle(generation)?;
                outcome
            }
        }
    }
    /// Refuse to act while a mutation this migration does not own is pending.
    fn require_owned_pending(&self) -> Result<()> {
        let foreign = self
            .old
            .controller
            .pending()?
            .into_iter()
            .any(|generation| !self.log.pending.contains(&generation));
        if foreign {
            return Err(Error::new(
                "uncertain",
                "Another action's outcome is unknown. Observe the desktop, run understudy \
                 restart, then run setup again.",
            )
            .uncertain());
        }
        Ok(())
    }
    /// Settle every generation this migration owns, once a full verification
    /// has established the outcome of all its actions.
    fn settle_all(&mut self) -> Result<()> {
        for generation in self.log.pending.clone() {
            self.settle(&generation)?;
        }
        Ok(())
    }
    /// Run one typed request inside the helper as a recorded mutation. Only an
    /// affirmative worker result settles it: an interrupted run, missing or
    /// malformed output, or a worker error may have changed state part way,
    /// so the generation stays pending for this migration to reconcile.
    async fn helper(&mut self, through: Endpoint, request: Request) -> Result<Value> {
        let argv = helper_argv(self.client(through), &request)?;
        let generation = self.begin()?;
        let outcome = match self
            .client(through)
            .run_pending(argv, Duration::from_secs(20))
            .await
        {
            Ok(reply) if reply["timedOut"] == true || !reply["signal"].is_null() => {
                Err(problem("The helper setup command was interrupted"))
            }
            Ok(reply) => helper_result(&reply),
            Err(error) if !error.uncertain => {
                // Never dispatched or refused by the daemon: nothing ran.
                self.settle(&generation)?;
                return Err(error);
            }
            Err(error) => Err(error),
        };
        let value = outcome.map_err(Error::uncertain)?;
        self.settle(&generation)?;
        self.note(&format!("{request:?}"))?;
        Ok(value)
    }
    /// The first endpoint in `order` with a verified hidden helper.
    async fn reachable_endpoint(&self, order: [Endpoint; 2]) -> Result<Endpoint> {
        for endpoint in order {
            if reachable(self.client(endpoint)).await {
                return Ok(endpoint);
            }
        }
        Err(problem(
            "Neither capture daemon answers in a hidden helper session; log in to the helper \
             once, switch back, then run setup again",
        ))
    }
    /// The managed endpoint is private, captures, and runs the planned binary.
    async fn verify_new(&self) -> Result<()> {
        await_endpoint(&self.new).await?;
        let status = self.new.status().await;
        if status["permissions"]["screenCapture"] != true {
            return Err(problem(
                "Understudy lacks Screen Recording in the helper account: log in to the \
                 helper once, allow Understudy in System Settings, switch back, then run \
                 setup again",
            ));
        }
        self.new.frame(Some(256)).await?;
        let pid = status["daemon"]["pid"]
            .as_u64()
            .and_then(|pid| u32::try_from(pid).ok())
            .ok_or_else(|| problem("The managed helper daemon reported no PID"))?;
        // The file it runs must be the planned build, and its loaded code
        // must be valid, Understudy's, and still that file.
        let installed = executable_of(pid)
            .and_then(|path| signature(&path))
            .and_then(|signature| signature.cdhash);
        if installed.as_deref() != Some(self.log.plan.cdhash.as_str()) {
            return Err(problem(
                "The managed helper daemon is not the signed binary that was planned",
            ));
        }
        if !running_developer_id(pid) {
            return Err(problem(
                "The helper daemon still runs a previous build; run understudy restart, then \
                 setup --apply again",
            ));
        }
        // SAFETY: getuid has no pointer arguments or preconditions.
        let controller = unsafe { libc::getuid() };
        if endpoint(&self.new.config.socket_path, self.log.plan.uid, controller).private
            != Some(true)
        {
            return Err(problem("The managed socket directory is not private"));
        }
        Ok(())
    }
    /// Whether `endpoint`'s socket serves the current, idle input service.
    async fn input_ready(&self, endpoint: Endpoint) -> bool {
        let client = self.client(endpoint);
        let status = client.status().await;
        status["inputService"]["pid"]
            .as_u64()
            .and_then(|pid| u32::try_from(pid).ok())
            .is_some_and(|pid| input_current(&status, client.config.helper_uid, pid))
    }
    /// Make the service on `to`'s socket the only, current input service.
    /// Presence is read from each input socket directly, independent of the
    /// capture daemons, and only a refused or missing socket counts as absent.
    async fn move_input(&mut self, from: Endpoint, to: Endpoint) -> Result<()> {
        // Both configurations may name the same input socket (moving from an
        // earlier managed layout): then the service stays and is only
        // confirmed current through the target daemon.
        let shared = self.client(from).config.input_socket == self.client(to).config.input_socket;
        let ready = self.input_ready(to).await;
        if shared && ready {
            return Ok(());
        }
        let source = if shared {
            None
        } else {
            input_presence(self.client(from)).await?
        };
        if source.is_none() && ready {
            return Ok(());
        }
        let generation = self.begin()?;
        let (previous, target) = (self.client(from), self.client(to));
        let outcome = async {
            if let Some(pid) = source {
                previous.stop_input(pid).await?;
            }
            if !ready {
                // A stale or unready service would hold the endpoint's lock.
                if let Some(pid) = input_presence(target).await? {
                    target.stop_input(pid).await?;
                }
                let pid = target.spawn_input().await?;
                if !input_current(&target.status().await, target.config.helper_uid, pid) {
                    return Err(
                        problem("The moved input service is not ready, current and idle")
                            .uncertain(),
                    );
                }
            }
            if !shared && input_presence(previous).await?.is_some() {
                return Err(problem("The previous input service is still running").uncertain());
            }
            Ok(())
        }
        .await;
        self.conclude(&generation, outcome)?;
        self.note("input moved")
    }
    /// Make the configuration file hold `want`, refusing unknown contents.
    fn switch_config(&mut self, from: &str, want: &str) -> Result<()> {
        let path = self.log.plan.config_path.clone();
        let current = fs::read_to_string(&path)?;
        if current == want {
            return Ok(());
        }
        if current != from {
            return Err(problem(format!(
                "{} changed during setup; it was not overwritten",
                path.display()
            )));
        }
        write_atomic(&path, want.as_bytes())?;
        self.note("configuration switched")
    }
    /// Both labels as launchd reports them, inspected through `through`.
    async fn jobs(&mut self, through: Endpoint) -> Result<Value> {
        let request = Request::Inspect {
            uid: self.log.plan.uid,
            labels: vec![self.log.plan.old_label.clone(), self.log.plan.label.clone()],
        };
        let argv = helper_argv(self.client(through), &request)?;
        // Inspection is read-only and needs no pending marker.
        let reply = self
            .client(through)
            .run_pending(argv, Duration::from_secs(20))
            .await?;
        Ok(helper_result(&reply)?["jobs"].clone())
    }

    async fn apply(&mut self) -> Result<()> {
        let plan = self.log.plan.clone();
        let uid = plan.uid;
        let via = self
            .reachable_endpoint([Endpoint::Old, Endpoint::New])
            .await?;
        let controller = plan.controller.clone();
        self.helper(via, Request::Prepare { uid, controller })
            .await?;
        // The app may have changed since planning: install only the binary
        // that was planned, still validly signed.
        if signed_daemon(&plan.program)? != plan.cdhash {
            return Err(problem(
                "Understudy.app changed since setup was planned; run setup --rollback, then \
                 setup --apply again",
            ));
        }
        let agent = self.agent();
        self.helper(via, Request::Install { agent }).await?;
        self.verify_new().await?;
        self.move_input(Endpoint::Old, Endpoint::New).await?;
        self.switch_config(&plan.old_config, &plan.new_config)?;
        // The fallback goes only once its replacement is verified again.
        self.verify_new().await?;
        if !self.input_ready(Endpoint::New).await {
            return Err(problem("Input is not ready on the managed endpoint"));
        }
        let (label, sha256) = (plan.old_label.clone(), plan.old_plist_sha256.clone());
        self.helper(
            Endpoint::New,
            Request::Retire {
                uid,
                label,
                sha256,
                missing_ok: false,
            },
        )
        .await?;
        let jobs = self.jobs(Endpoint::New).await?;
        if jobs[&plan.old_label]["loaded"] != false || jobs[&plan.label]["loaded"] != true {
            return Err(problem(
                "launchd does not show the expected jobs after retiring",
            ));
        }
        self.settle_all()?;
        self.log.state = State::Applied;
        save_log(&self.path, &self.log)
    }

    async fn rollback(&mut self) -> Result<()> {
        let plan = self.log.plan.clone();
        let uid = plan.uid;
        self.log.state = State::RollingBack;
        save_log(&self.path, &self.log)?;
        if reachable(&self.new).await {
            // Idempotent: puts the original definition back only if absent.
            let (label, sha256) = (plan.old_label.clone(), plan.old_plist_sha256.clone());
            self.helper(Endpoint::New, Request::Restore { uid, label, sha256 })
                .await?;
        }
        await_endpoint(&self.old).await?;
        self.move_input(Endpoint::New, Endpoint::Old).await?;
        self.switch_config(&plan.new_config, &plan.old_config)?;
        // Planning proved the managed label had no artifacts, so anything
        // there now was created by this migration.
        let sha256 = text_sha256(&plist(&self.agent(), &self.layout().logs()));
        let label = plan.label.clone();
        self.helper(
            Endpoint::Old,
            Request::Retire {
                uid,
                label,
                sha256,
                missing_ok: true,
            },
        )
        .await?;
        let jobs = self.jobs(Endpoint::Old).await?;
        if jobs[&plan.old_label]["loaded"] != true || jobs[&plan.label]["loaded"] != false {
            return Err(problem(
                "launchd does not show the original job after rollback",
            ));
        }
        self.settle_all()?;
        self.log.state = State::RolledBack;
        save_log(&self.path, &self.log)
    }
}

/// The code directory hash of `program`, once it is shown to be validly
/// signed as Understudy with a Developer ID.
fn signed_daemon(program: &Path) -> Result<String> {
    if !super::developer_id(program) {
        return Err(problem(format!(
            "{} is not Understudy signed with a Developer ID, so macOS consent would not \
             persist",
            program.display()
        )));
    }
    signature(program)
        .and_then(|signature| signature.cdhash)
        .ok_or_else(|| problem("The helper daemon's signature could not be read"))
}

/// The signed Understudy executable the helper daemon runs: this one when it
/// is inside the app, else the installed app's.
fn daemon_program() -> Result<PathBuf> {
    const INSTALLED: &str = "/Applications/Understudy.app/Contents/MacOS/understudy";
    let current = std::env::current_exe().and_then(fs::canonicalize).ok();
    current
        .filter(|path| {
            path.parent()
                .is_some_and(|dir| dir.ends_with("Contents/MacOS"))
        })
        .or_else(|| Some(PathBuf::from(INSTALLED)).filter(|path| path.is_file()))
        .ok_or_else(|| {
            problem("Install Understudy.app first; setup runs the helper daemon from it")
        })
}

fn clients(plan: &Plan) -> Result<(Client, Client)> {
    let old = Config::parse(plan.old_config.as_bytes(), &plan.config_path)?;
    let new = Config::parse(plan.new_config.as_bytes(), &plan.config_path)?;
    Ok((Client::new(old)?, Client::new(new)?))
}

/// Plan from the live configuration. Reads only; refuses unless the managed
/// label has no artifacts and the current job is a verified plain plist.
async fn plan(config_path: &Path) -> Result<Plan> {
    let config_path = fs::canonicalize(config_path)?;
    let config = Config::load(&config_path)?;
    let old_config = fs::read_to_string(&config_path)?;
    let account = account_with_uid(config.helper_uid)?
        .ok_or_else(|| problem("The configured helper account does not exist"))?;
    // SAFETY: getuid has no pointer arguments or preconditions.
    let controller = account_with_uid(unsafe { libc::getuid() })?
        .ok_or_else(|| problem("The controller account cannot be resolved"))?;
    let layout = Layout {
        uid: config.helper_uid,
        home: account.home.clone(),
    };
    let target = managed(&config, &layout);
    if config.socket_path == target.socket_path {
        return Err(problem(
            "The configuration already uses the managed private layout",
        ));
    }
    let client = Client::new(config.clone())?;
    let built = std::env::current_exe()
        .ok()
        .and_then(|path| crate::digest::file_sha256(&path));
    if built.is_none() || client.installed_build() != built {
        return Err(problem(
            "The helper runs the installed executable; run understudy install first",
        ));
    }
    let status = client.status().await;
    require_hidden(&status, config.helper_uid).map_err(|_| {
        problem("Setup needs the current capture daemon reachable in a hidden helper session")
    })?;
    let pid = status["daemon"]["pid"]
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok())
        .ok_or_else(|| problem("The capture daemon reported no PID"))?;
    let program = daemon_program()?;
    let cdhash = signed_daemon(&program)?;
    let old_label = helper_job(&client, pid).await["label"]
        .as_str()
        .ok_or_else(|| problem("The current capture job could not be identified"))?
        .to_owned();
    if old_label == LABEL {
        return Err(problem(
            "The current capture job is already the managed one",
        ));
    }
    let request = Request::Inspect {
        uid: config.helper_uid,
        labels: vec![old_label.clone(), LABEL.to_owned()],
    };
    // Inspection is read-only, so it needs no pending marker.
    let reply = client
        .run_pending(helper_argv(&client, &request)?, Duration::from_secs(20))
        .await?;
    let jobs = helper_result(&reply)?["jobs"].clone();
    let old = &jobs[&old_label];
    let old_plist_sha256 = old["plistSha256"]
        .as_str()
        .filter(|_| {
            old["loaded"] == true
                && old["plistKind"] == "regular"
                && old["path"] == json!(layout.agent_plist(&old_label))
        })
        .ok_or_else(|| {
            problem("The current capture job is not loaded from a plain plist in LaunchAgents")
        })?
        .to_owned();
    let managed = &jobs[LABEL];
    if managed["loaded"] != false || managed["plistKind"] != "absent" {
        return Err(problem(format!(
            "{LABEL} already exists in the helper; remove it before setup manages it"
        )));
    }
    Ok(Plan {
        uid: config.helper_uid,
        controller: controller.name,
        home: account.home,
        old_label,
        old_plist_sha256,
        label: LABEL.to_owned(),
        controller_uid: controller.uid,
        program,
        cdhash,
        config_path,
        new_config: target.to_toml()?,
        old_config,
    })
}

fn report(log: &Log, path: &Path, outcome: Result<()>) -> Value {
    let mut value = json!({
        "ok": outcome.is_ok(),
        "state": log.state,
        "log": path,
        "oldLabel": log.plan.old_label,
        "label": log.plan.label,
        "config": log.plan.config_path,
        "pending": log.pending.len(),
        "history": log.history.iter().map(|(action, _)| action).collect::<Vec<_>>(),
    });
    if let Err(error) = outcome {
        value["resumeRequired"] = json!(true);
        value["uncertain"] = json!(error.uncertain);
        value["error"] = json!(error.error);
    }
    value
}

/// Migrate, resume, or re-verify the managed private capture endpoint.
/// Callers prove controller ownership (acquired for recovery) by passing it.
pub async fn apply(config_path: &Path, _ownership: &Ownership) -> Result<Value> {
    let uid = Config::load(config_path)?.helper_uid;
    let path = log_path(uid)?;
    let log = match load_log(&path)? {
        // Applied by an Understudy before its own daemon: plan the move to it.
        Some(log) if log.plan.label.is_empty() && log.state == State::Applying => {
            return Err(problem(format!(
                "A setup recorded by an older Understudy is unfinished; finish or roll it back \
                 with that version ({})",
                path.display()
            )));
        }
        Some(log)
            if matches!(log.state, State::Applying | State::Applied)
                && !log.plan.label.is_empty() =>
        {
            log
        }
        Some(log) if log.state == State::RollingBack => {
            return Err(problem(format!(
                "A rollback is unfinished; run setup --rollback to complete it ({})",
                path.display()
            )));
        }
        previous => {
            if let Some(previous) = previous {
                let archive = path.with_extension(format!("{}.json", timestamp()));
                save_log(&archive, &previous)?;
            }
            Log {
                version: 1,
                plan: plan(config_path).await?,
                state: State::Applying,
                pending: Vec::new(),
                history: Vec::new(),
            }
        }
    };
    let (old, new) = clients(&log.plan)?;
    let mut migration = Migration {
        path,
        log,
        old,
        new,
    };
    migration.require_owned_pending()?;
    if migration.log.state == State::Applied {
        // An applied setup re-verifies. An updated Understudy.app is accepted
        // if it is still validly signed as Understudy: its new hash is
        // recorded, and the running daemon's executable must be that binary.
        let current = signed_daemon(&migration.log.plan.program)?;
        if current != migration.log.plan.cdhash {
            migration.log.plan.cdhash = current;
            migration.note("helper daemon updated")?;
        }
    }
    migration.log.state = State::Applying;
    save_log(&migration.path, &migration.log)?;
    let outcome = migration.apply().await;
    Ok(report(&migration.log, &migration.path, outcome))
}

/// Undo an applied or interrupted migration recorded in the log.
/// Callers prove controller ownership (acquired for recovery) by passing it.
pub async fn rollback(config_path: &Path, _ownership: &Ownership) -> Result<Value> {
    let path = log_path(Config::load(config_path)?.helper_uid)?;
    let log = load_log(&path)?.ok_or_else(|| problem("No setup migration is recorded"))?;
    if log.plan.label.is_empty() {
        return Err(problem(
            "This setup was recorded by an older Understudy; roll it back with that version",
        ));
    }
    let (old, new) = clients(&log.plan)?;
    let mut migration = Migration {
        path,
        log,
        old,
        new,
    };
    migration.require_owned_pending()?;
    let outcome = migration.rollback().await;
    Ok(report(&migration.log, &migration.path, outcome))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config {
            helper_uid: 502,
            socket_path: "/tmp/atlas-live-502/502.sock".into(),
            identity_probe: "/Users/Shared/Understudy/understudy".into(),
            input_socket: "/tmp/understudy-input-502/input.sock".into(),
            output_dir: "/Users/c/runs".into(),
        }
    }

    fn plan(config_path: PathBuf, text: &str) -> Plan {
        Plan {
            uid: 502,
            controller: "c".into(),
            home: "/Users/h".into(),
            old_label: "dev.atlas.live-sessiond".into(),
            old_plist_sha256: "a".repeat(64),
            label: LABEL.into(),
            controller_uid: 501,
            program: "/x".into(),
            cdhash: "ab".into(),
            config_path,
            old_config: text.into(),
            new_config: text.into(),
        }
    }

    #[test]
    fn the_managed_layout_only_moves_sockets() {
        let layout = Layout {
            uid: 502,
            home: "/Users/atlastest".into(),
        };
        let target = managed(&config(), &layout);
        assert_eq!(
            target.socket_path,
            PathBuf::from("/Users/atlastest/.understudy/helper/502.sock")
        );
        assert_eq!(
            target.input_socket,
            PathBuf::from("/Users/atlastest/.understudy/input/input.sock")
        );
        assert_eq!(target.identity_probe, config().identity_probe);
        assert_eq!(target.output_dir, config().output_dir);
        assert!(target.socket_path.to_string_lossy().len() < 104);
    }

    /// A log from before the own daemon has no managed label; it reads, and
    /// marks itself as one this version must not resume or roll back.
    #[test]
    fn logs_from_the_previous_daemon_are_recognised() {
        let mut legacy = serde_json::to_value(Log {
            version: 1,
            plan: plan("/c/understudy.local.toml".into(), "a"),
            state: State::Applied,
            pending: Vec::new(),
            history: Vec::new(),
        })
        .unwrap();
        let fields = legacy["plan"].as_object_mut().unwrap();
        fields.remove("label");
        fields.remove("controllerUid");
        let log: Log = serde_json::from_value(legacy).unwrap();
        assert!(log.plan.label.is_empty());
    }

    #[test]
    fn logs_round_trip_privately() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deep/setup-502.json");
        let log = Log {
            version: 1,
            plan: plan("/c/understudy.local.toml".into(), "a"),
            state: State::Applying,
            pending: vec!["g".into()],
            history: vec![("prepare".into(), "1".into())],
        };
        save_log(&path, &log).unwrap();
        assert_eq!(load_log(&path).unwrap(), Some(log));
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(load_log(&dir.path().join("missing.json")).unwrap(), None);
    }

    #[test]
    fn plans_resolve_relative_configuration_against_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("understudy.local.toml");
        let text = "helper_uid = 502\nsocket_path = \"s.sock\"\nidentity_probe = \"p\"\n\
                    input_socket = \"i.sock\"\noutput_dir = \"runs\"\n";
        let (old, _) = clients(&plan(config_path, text)).unwrap();
        assert_eq!(
            old.config.socket_path,
            fs::canonicalize(dir.path()).unwrap().join("s.sock")
        );
    }

    #[test]
    fn helper_results_must_be_affirmative_json() {
        use base64::{Engine, engine::general_purpose::STANDARD};
        let reply = |text: &str| {
            json!({"ok": true, "exitCode": 0, "events": [
                {"event": "output", "data": STANDARD.encode(text)}
            ]})
        };
        assert_eq!(
            helper_result(&reply(r#"{"ok":true,"x":1}"#)).unwrap()["x"],
            1
        );
        assert!(helper_result(&reply(r#"{"ok":false,"error":"no"}"#)).is_err());
        assert!(helper_result(&reply("garbage")).is_err());
        assert!(helper_result(&json!({"ok": true, "exitCode": 0})).is_err());
    }
}
