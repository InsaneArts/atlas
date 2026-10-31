//! Side-by-side migration of the capture endpoint into the private layout.
//!
//! A managed LaunchAgent runs Atlas's own signed helper daemon on a
//! persistent private socket directory beside the existing capture job, which
//! it replaces only once verified. Each pass
//! reconciles reality toward the target instead of trusting completion flags:
//! every helper action is idempotent, re-inspects first and runs under a
//! pending marker recorded in a durable log, so an interrupted pass resumes or
//! rolls back without blind replays. The job carrying a reply is never the one
//! being unloaded, and only artifacts this migration created are undone.
use super::{
    Account, account_with_uid, endpoint, executable_of,
    helper::{Agent, LABEL, Layout, Request, plist, text_sha256},
    helper_job, running_developer_id, signature,
};
use crate::{
    client::{Client, Run, timestamp},
    config::{Config, home, save_private},
    control::Ownership,
    error::{Code, Error, Result},
    session::{input_current, require_hidden},
    transport::run_output,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, Instant, SystemTime},
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
    /// A first setup (by `atlas init`): no earlier capture job, so
    /// nothing is moved or retired and there is nothing to roll back to.
    #[serde(default)]
    pub fresh: bool,
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
    Error::new(Code::Setup, message)
}

fn log_path(uid: u32) -> Result<PathBuf> {
    Ok(home()?
        .join("Library/Application Support/Atlas")
        .join(format!("setup-{uid}.json")))
}

/// Whether helper `uid`'s setup is applied or still being applied, so an
/// update carries it over to the new app. A setup rolled back (or being
/// rolled back) is the person's choice and stays as it is; a record that
/// cannot be read counts, so the step that carries it over says why.
pub fn in_effect(uid: u32) -> bool {
    log_path(uid).is_ok_and(|path| carried_over(load_log(&path)))
}

fn carried_over(log: Result<Option<Log>>) -> bool {
    match log {
        Ok(Some(log)) => matches!(log.state, State::Applying | State::Applied),
        Ok(None) => false,
        Err(_) => true,
    }
}

/// Whether helper `uid`'s setup is in effect (as [`in_effect`]) for the
/// account whose home is `home`. A record left by a deleted account whose
/// user ID was reused does not count; one that cannot be read does.
pub fn in_effect_for(uid: u32, home: &Path) -> bool {
    log_path(uid).is_ok_and(|path| in_effect_with(load_log(&path), home))
}

fn in_effect_with(log: Result<Option<Log>>, home: &Path) -> bool {
    let theirs = matches!(&log, Ok(Some(log)) if log.plan.home != home);

    !theirs && carried_over(log)
}

/// Archive helper `uid`'s setup record if it belongs to another account
/// (a deleted helper whose UID was reused): it describes nothing here.
/// Whether a record was archived.
pub fn archive_foreign(uid: u32, home: &Path) -> Result<bool> {
    let path = log_path(uid)?;
    let Some(log) = load_log(&path)? else {
        return Ok(false);
    };

    if log.plan.home == home {
        return Ok(false);
    }

    archive_as(&path, "foreign")
}

/// Archive helper `uid`'s setup record once uninstall has removed the job
/// it describes, as `setup-<uid>.<time>.uninstalled.json`: kept as evidence,
/// but no longer read as a setup (neither here nor by install.sh). Whether a
/// record was archived.
pub fn archive_uninstalled(uid: u32) -> Result<bool> {
    archive_as(&log_path(uid)?, "uninstalled")
}

/// Move the record at `path` aside as `<name>.<time>.<why>.json`; false
/// when there is none.
fn archive_as(path: &Path, why: &str) -> Result<bool> {
    let archive = path.with_extension(format!("{}.{why}.json", timestamp()));

    match fs::rename(path, archive) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// Whether any setup of helper `uid` is recorded, whatever its state.
pub fn recorded(uid: u32) -> bool {
    log_path(uid).is_ok_and(|path| fs::symlink_metadata(path).is_ok())
}

/// Whether helper `uid`'s setup is recorded as applied with nothing it
/// owns still pending.
pub fn settled(uid: u32) -> bool {
    log_path(uid)
        .ok()
        .and_then(|path| load_log(&path).ok().flatten())
        .is_some_and(|log| log.state == State::Applied && log.pending.is_empty())
}

fn load_log(path: &Path) -> Result<Option<Log>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn save_log(path: &Path, log: &Log) -> Result<()> {
    save_private(path, &serde_json::to_vec_pretty(log)?)
}

/// The configuration with sockets moved into the private layout.
pub fn managed(config: &Config, layout: &Layout) -> Config {
    Config {
        socket_path: layout.socket(),
        input_socket: layout.input_socket(),
        ..config.clone()
    }
}

fn helper_argv(client: &Client, request: &Request) -> Result<Vec<String>> {
    helper_argv_with(
        client,
        request,
        setup_identity(client.config.helper_uid, None)?,
    )
}

/// A legacy helper needs provenance before its identity can guard workers.
/// This does not persist an identity: only the person's selection does that.
fn setup_identity(uid: u32, config_path: Option<&Path>) -> Result<super::identity::Identity> {
    setup_identity_with(
        super::identity::on_file(uid),
        || super::identity::verified(uid),
        || {
            let account = account_with_uid(uid)?.ok_or_else(choose_helper)?;
            let identity = super::identity::current(&account).ok_or_else(choose_helper)?;
            let created = crate::session::account_created(&account.name);
            let recorded = config_path
                .and_then(|path| fs::metadata(path).ok())
                .and_then(|metadata| metadata.created().ok());
            let sessions = crate::session::sessions(Some(uid));
            legacy_provenance(created, recorded, sessions.as_ref(), uid)?;
            if account_with_uid(uid)?.as_ref() != Some(&account)
                || super::identity::current(&account).as_ref() != Some(&identity)
            {
                return Err(choose_helper());
            }
            Ok(identity)
        },
    )
}

fn choose_helper() -> Error {
    problem(
        "Atlas could not verify the helper account. Choose the helper again in Atlas to record its identity before setting it up.",
    )
}

fn legacy_provenance(
    created: Option<SystemTime>,
    recorded: Option<SystemTime>,
    sessions: Option<&crate::session::Sessions>,
    uid: u32,
) -> Result<()> {
    let (Some(created), Some(recorded), Some(sessions)) = (created, recorded, sessions) else {
        return Err(choose_helper());
    };
    let age = created.elapsed().map_err(|_| choose_helper())?;
    if created >= recorded
        || sessions.logins.iter().any(|(window, kind)| {
            window.uid == uid
                && (*kind == crate::session::Kind::HelperLeftover
                    || window.age >= age + crate::session::REUSED_SLACK)
        })
    {
        return Err(choose_helper());
    }
    Ok(())
}

fn setup_identity_with(
    on_file: super::identity::OnFile,
    verified: impl FnOnce() -> Option<super::identity::Identity>,
    legacy: impl FnOnce() -> Result<super::identity::Identity>,
) -> Result<super::identity::Identity> {
    match on_file {
        super::identity::OnFile::Nothing => legacy(),
        _ => verified().ok_or_else(choose_helper),
    }
}

fn helper_argv_with(
    client: &Client,
    request: &Request,
    expected: super::identity::Identity,
) -> Result<Vec<String>> {
    let mut argv = vec![
        client.config.identity_probe.to_string_lossy().into_owned(),
        "setup-helper".to_owned(),
        serde_json::to_string(request)?,
    ];
    argv.extend(["--account".into(), serde_json::to_string(&expected)?]);
    Ok(argv)
}

/// Whether a completed helper run is a refusal the worker says changed
/// nothing, so its outcome is certain.
fn refused_unchanged(reply: &Value) -> bool {
    run_output(reply, 65536)
        .ok()
        .and_then(|output| serde_json::from_slice::<Value>(&output).ok())
        .is_some_and(|value| value["ok"] == false && value["unchanged"] == true)
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
                "The helper did not answer in time ({}). Run atlas doctor.",
                crate::ui::text::tilde(&client.config.socket_path)
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
    account: OnceLock<super::identity::Identity>,
}

impl Migration {
    /// The migration `log` at `path`, with a client for each of the plan's
    /// two configurations. Construction reads nothing.
    fn new(path: PathBuf, log: Log) -> Result<Self> {
        let old = Config::parse(log.plan.old_config.as_bytes(), &log.plan.config_path)?;
        let new = Config::parse(log.plan.new_config.as_bytes(), &log.plan.config_path)?;
        Ok(Self {
            path,
            log,
            old: Client::new(old)?,
            new: Client::new(new)?,
            account: OnceLock::new(),
        })
    }

    fn argv(&self, through: Endpoint, request: &Request) -> Result<Vec<String>> {
        if self.account.get().is_none() {
            let identity = setup_identity(self.log.plan.uid, Some(&self.log.plan.config_path))?;
            let account = account_with_uid(self.log.plan.uid)?.ok_or_else(choose_helper)?;
            if account.home != self.log.plan.home {
                return Err(choose_helper());
            }
            let _ = self.account.set(identity);
        }
        helper_argv_with(
            self.client(through),
            request,
            self.account.get().unwrap().clone(),
        )
    }

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
            guard: None,
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
        let generation = crate::files::token();
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
                Code::Uncertain,
                "Another action's outcome is unknown. Observe the desktop, run atlas \
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

    /// Run one typed request inside the helper as a recorded mutation. An
    /// affirmative worker result settles it, and so does a refusal the
    /// worker says changed nothing. An interrupted run, missing or
    /// malformed output, or any other worker error may have changed state
    /// part way, so the generation stays pending for this migration to
    /// reconcile.
    async fn helper(&mut self, through: Endpoint, request: Request) -> Result<Value> {
        let argv = self.argv(through, &request)?;
        let generation = self.begin()?;

        let outcome = match self
            .client(through)
            .run(argv, Run::pending().within(Duration::from_secs(20)))
            .await
        {
            Ok(reply) if reply["timedOut"] == true || !reply["signal"].is_null() => {
                Err(problem("The helper setup command was interrupted").uncertain())
            }
            Ok(reply) if refused_unchanged(&reply) => helper_result(&reply),
            Ok(reply) => helper_result(&reply).map_err(Error::uncertain),
            Err(error) if !error.uncertain => {
                // Never dispatched or refused by the daemon: nothing ran.
                self.settle(&generation)?;
                return Err(error);
            }
            Err(error) => Err(error),
        };

        let value = self.conclude(&generation, outcome)?;
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
            "The helper is not running in the background. Log in to the helper account \
             once, switch back, then run atlas init again.",
        ))
    }

    /// The managed endpoint is private, captures, and runs the planned binary.
    async fn verify_new(&self) -> Result<()> {
        await_endpoint(&self.new).await?;

        let status = self.new.status().await;
        if status["permissions"]["screenCapture"] != true {
            return Err(problem(
                "Atlas lacks Screen Recording in the helper account: log in to the \
                 helper once, allow Atlas in System Settings, switch back, then run \
                 setup again",
            ));
        }

        self.new.frame(Some(256)).await?;

        let pid = status["daemon"]["pid"]
            .as_u64()
            .and_then(|pid| u32::try_from(pid).ok())
            .ok_or_else(|| {
                problem("The helper did not say which process it is. Run atlas restart.")
            })?;

        // The file it runs must be the planned build, and its loaded code
        // must be valid, Atlas's, and still that file.
        let installed = executable_of(pid)
            .and_then(|path| signature(&path))
            .and_then(|signature| signature.cdhash);
        if installed.as_deref() != Some(self.log.plan.cdhash.as_str()) {
            return Err(problem(
                "The helper is not running the signed Atlas build that was expected. Run \
                 atlas restart.",
            ));
        }

        if !running_developer_id(pid) {
            return Err(problem(
                "The helper still runs an older build. Run atlas restart, then \
                 atlas setup --apply.",
            ));
        }

        let controller = nix::unistd::getuid().as_raw();
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

    /// Whether setup must switch the configuration, refusing unknown contents.
    fn check_config(&self, from: &str, want: &str) -> Result<bool> {
        let path = &self.log.plan.config_path;
        let current = fs::read_to_string(path)?;
        if current == want {
            return Ok(false);
        }

        if current != from {
            return Err(problem(format!(
                "{} changed during setup; it was not overwritten",
                path.display()
            )));
        }

        // Replacing a link would leave the file it points to unchanged.
        if fs::symlink_metadata(path)?.file_type().is_symlink() {
            return Err(problem(format!(
                "{} is a link, so setup cannot switch it. Replace the link with the file it \
                 points to, then run setup again.",
                path.display()
            )));
        }

        Ok(true)
    }

    fn switch_config(&mut self, from: &str, want: &str) -> Result<()> {
        if !self.check_config(from, want)? {
            return Ok(());
        }
        save_private(&self.log.plan.config_path, want.as_bytes())?;
        self.note("configuration switched")
    }

    /// Both labels as launchd reports them, inspected through `through`.
    async fn jobs(&self, through: Endpoint) -> Result<Value> {
        let request = Request::Inspect {
            uid: self.log.plan.uid,
            labels: vec![self.log.plan.old_label.clone(), self.log.plan.label.clone()],
        };
        Ok(self.read_only(through, request).await?["jobs"].clone())
    }

    async fn read_only(&self, through: Endpoint, request: Request) -> Result<Value> {
        let argv = self.argv(through, &request)?;

        // Inspection is read-only and needs no pending marker.
        let reply = self
            .client(through)
            .run(argv, Run::pending().within(Duration::from_secs(20)))
            .await?;

        helper_result(&reply)
    }

    /// The same worker inspections update uses before any bundle swap.
    async fn preflight_with(
        &self,
        resuming: bool,
        updated_program: &Path,
        signed: impl FnOnce(&Path) -> Result<String>,
        run: impl AsyncFn(Request) -> Result<Value>,
    ) -> Result<()> {
        self.require_owned_pending()?;
        self.check_config(&self.log.plan.old_config, &self.log.plan.new_config)?;
        let updated = signed(updated_program)?;
        if resuming
            && self.log.state != State::Applied
            && !self.log.plan.fresh
            && updated != self.log.plan.cdhash
        {
            return Err(problem(
                "Atlas.app changed since setup was planned; run setup --rollback, then setup --apply again",
            ));
        }
        let plan = &self.log.plan;
        let mut labels = vec![plan.label.clone()];
        if !plan.fresh {
            labels.push(plan.old_label.clone());
        }
        let observed = run(Request::Inspect {
            uid: plan.uid,
            labels,
        })
        .await?;
        let job = &observed["jobs"][&plan.label];
        let expected = text_sha256(&plist(&self.agent(), &self.layout().logs()));
        if !(job["plistKind"] == "absent"
            || job["plistKind"] == "regular" && job["plistSha256"] == expected)
            || job["loaded"] == true && job["path"] != json!(self.layout().agent_plist(&plan.label))
        {
            return Err(problem(
                "The helper's background job differs from its setup; run atlas init to replace it",
            ));
        }
        if !plan.fresh {
            run(Request::CheckRetire {
                uid: plan.uid,
                label: plan.old_label.clone(),
                sha256: plan.old_plist_sha256.clone(),
                missing_ok: false,
            })
            .await?;
        }
        Ok(())
    }

    /// A first setup: the managed daemon is already running (init started
    /// it); prepare and install idempotently through it, verify it and the
    /// input service, and record the result.
    async fn apply_fresh(&mut self) -> Result<()> {
        let plan = self.log.plan.clone();
        let uid = plan.uid;
        let controller = plan.controller.clone();
        self.helper(Endpoint::New, Request::Prepare { uid, controller })
            .await?;

        if signed_daemon(&plan.program)? != plan.cdhash {
            return Err(problem(
                "Atlas.app changed since setup was planned; run atlas init again",
            ));
        }

        let agent = self.agent();
        self.helper(Endpoint::New, Request::Install { agent })
            .await?;
        self.verify_new().await?;

        if !self.input_ready(Endpoint::New).await {
            return Err(problem("Input is not ready; run atlas start-input"));
        }

        self.settle_all()?;
        self.log.state = State::Applied;

        save_log(&self.path, &self.log)
    }

    async fn apply(&mut self) -> Result<()> {
        if self.log.plan.fresh {
            return self.apply_fresh().await;
        }

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
                "Atlas.app changed since setup was planned; run setup --rollback, then \
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
/// signed as Atlas with a Developer ID.
fn signed_daemon(program: &Path) -> Result<String> {
    if !super::developer_id(program) {
        return Err(problem(format!(
            "{} is not Atlas signed with a Developer ID, so macOS consent would not \
             persist",
            program.display()
        )));
    }

    signature(program)
        .and_then(|signature| signature.cdhash)
        .ok_or_else(|| problem("The helper app's signature could not be read"))
}

/// The stable installed executable the helper daemon runs.
fn daemon_program() -> Result<PathBuf> {
    super::installed_app::app_executable()
}

/// What every plan starts from: the configuration (its canonical path,
/// meaning and text), both accounts, and the helper's managed layout.
struct Basis {
    config_path: PathBuf,
    config: Config,
    text: String,
    account: Account,
    controller: Account,
    layout: Layout,
}

fn basis(config_path: &Path) -> Result<Basis> {
    let config_path = fs::canonicalize(config_path)?;
    let config = Config::load(&config_path)?;
    let text = fs::read_to_string(&config_path)?;
    let account = account_with_uid(config.helper_uid)?
        .ok_or_else(|| problem("The configured helper account does not exist"))?;
    let controller = account_with_uid(nix::unistd::getuid().as_raw())?
        .ok_or_else(|| problem("The controller account cannot be resolved"))?;
    let layout = Layout {
        uid: config.helper_uid,
        home: account.home.clone(),
        guard: None,
    };

    Ok(Basis {
        config_path,
        config,
        text,
        account,
        controller,
        layout,
    })
}

/// Plan a first setup of the configuration at `config_path`, whose helper
/// already runs the managed daemon (started by `atlas init`).
async fn plan_fresh(config_path: &Path) -> Result<Plan> {
    let Basis {
        config_path,
        config,
        text,
        account,
        controller,
        layout,
    } = basis(config_path)?;

    if config.socket_path != layout.socket() {
        return Err(problem(
            "The configuration does not use the managed private layout",
        ));
    }

    let status = Client::new(config.clone())?.status().await;
    require_hidden(&status, config.helper_uid).map_err(|_| {
        problem(
            "The helper is not running in the background. Switch back to your own account, then \
             run atlas doctor.",
        )
    })?;

    let program = daemon_program()?;
    let cdhash = signed_daemon(&program)?;

    Ok(Plan {
        uid: config.helper_uid,
        controller: controller.name,
        home: account.home,
        old_label: String::new(),
        old_plist_sha256: String::new(),
        label: LABEL.to_owned(),
        controller_uid: controller.uid,
        program,
        cdhash,
        config_path,
        old_config: text.clone(),
        new_config: text,
        fresh: true,
    })
}

/// Where a pass starts from the recorded log.
#[derive(Debug, PartialEq)]
enum Start {
    /// Resume or re-verify the recorded setup.
    Resume(Log),
    /// Plan anew, archiving the earlier record, if any.
    Plan(Option<Log>),
}

fn unknown_version(path: &Path) -> Error {
    problem(format!(
        "The setup record has an unknown version ({})",
        path.display()
    ))
}

fn rollback_unfinished(path: &Path) -> Error {
    problem(format!(
        "A rollback is unfinished; run setup --rollback to complete it ({})",
        path.display()
    ))
}

/// Where [`apply`] starts from `log`, recorded at `path`.
fn apply_start(log: Option<Log>, path: &Path) -> Result<Start> {
    match log {
        Some(log) if log.version != 1 => Err(unknown_version(path)),
        // Applied by an Atlas before its own daemon: plan the move to it.
        Some(log) if log.plan.label.is_empty() && log.state == State::Applying => {
            Err(problem(format!(
                "A setup recorded by an older Atlas is unfinished; finish or roll it back \
                 with that version ({})",
                path.display()
            )))
        }
        Some(log)
            if matches!(log.state, State::Applying | State::Applied)
                && !log.plan.label.is_empty() =>
        {
            Ok(Start::Resume(log))
        }
        Some(log) if log.state == State::RollingBack => Err(rollback_unfinished(path)),
        previous => Ok(Start::Plan(previous)),
    }
}

/// Where [`apply_fresh`] starts from `log`, recorded at `path`; None for a
/// migrated (not fresh) setup, which re-verifies the way it was made.
fn fresh_start(log: Option<Log>, path: &Path) -> Result<Option<Start>> {
    match log {
        Some(log) if log.version != 1 => Err(unknown_version(path)),
        Some(log) if log.state == State::RollingBack => Err(rollback_unfinished(path)),
        Some(log) if !log.plan.label.is_empty() && !log.plan.fresh => Ok(None),
        Some(log) if log.plan.fresh => Ok(Some(Start::Resume(log))),
        previous => Ok(Some(Start::Plan(previous))),
    }
}

/// The setup log for the configuration at `config_path`: where it is
/// recorded, and what is recorded there.
fn recorded_log(config_path: &Path) -> Result<(PathBuf, Option<Log>)> {
    let path = log_path(Config::load(config_path)?.helper_uid)?;
    let log = load_log(&path)?;
    Ok((path, log))
}

/// The log a pass works from: the recorded one, or a new one for `plan`
/// with the earlier record (if any) archived beside it first.
async fn starting_log(
    path: &Path,
    start: Start,
    plan: impl Future<Output = Result<Plan>>,
) -> Result<Log> {
    match start {
        Start::Resume(log) => Ok(log),
        Start::Plan(previous) => {
            if let Some(previous) = previous {
                let archive = path.with_extension(format!("{}.json", timestamp()));
                save_log(&archive, &previous)?;
            }

            Ok(Log {
                version: 1,
                plan: plan.await?,
                state: State::Applying,
                pending: Vec::new(),
                history: Vec::new(),
            })
        }
    }
}

/// Apply `log`, first recording an updated Atlas.app (still validly signed
/// as Atlas) as the same setup when `recheck` is set.
async fn carry_out(path: PathBuf, log: Log, recheck: bool) -> Result<Value> {
    let mut migration = Migration::new(path, log)?;
    migration.require_owned_pending()?;

    if recheck {
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

/// Record and verify a first setup (see [`plan_fresh`]). Re-running on an
/// applied setup re-verifies it, like [`apply`].
pub async fn apply_fresh(
    config_path: &Path,
    client: &Client,
    ownership: &Ownership,
) -> Result<Value> {
    client.require_service_idle().await?;
    let (path, log) = recorded_log(config_path)?;
    let Some(start) = fresh_start(log, &path)? else {
        return apply(config_path, client, ownership).await;
    };

    let log = starting_log(&path, start, plan_fresh(config_path)).await?;

    // An updated Atlas.app, still validly signed as Atlas, continues the
    // same setup: its new signature is recorded, as apply does.
    carry_out(path, log, true).await
}

/// How launchd and the helper's LaunchAgents folder show the managed job,
/// read through `client`'s daemon (read-only, so no pending marker): the
/// worker's report for [`LABEL`] (`loaded`, `plistKind`, `plistSha256`...).
pub(crate) async fn managed_job(client: &Client) -> Result<Value> {
    let request = Request::Inspect {
        uid: client.config.helper_uid,
        labels: vec![LABEL.to_owned()],
    };

    let reply = client
        .run(
            helper_argv(client, &request)?,
            Run::pending().within(Duration::from_secs(20)),
        )
        .await?;

    Ok(helper_result(&reply)?["jobs"][LABEL].clone())
}

/// Plan from the live configuration. Reads only; refuses unless the managed
/// label has no artifacts and the current job is a verified plain plist.
async fn plan(config_path: &Path) -> Result<Plan> {
    let Basis {
        config_path,
        config,
        text: old_config,
        account,
        controller,
        layout,
    } = basis(config_path)?;

    let target = managed(&config, &layout);
    if config.socket_path == target.socket_path {
        return Err(problem(
            "The configuration already uses the managed private layout",
        ));
    }

    let client = Client::new(config.clone())?;
    let built = std::env::current_exe()
        .and_then(fs::canonicalize)
        .ok()
        .and_then(|path| crate::digest::file_sha256(&path));
    if built.is_none() || client.installed_build() != built {
        return Err(problem(
            "The helper runs the installed executable; run atlas install first",
        ));
    }

    let status = client.status().await;
    require_hidden(&status, config.helper_uid).map_err(|_| {
        problem(
            "The helper is not running in the background. Switch back to your own account, then \
             run atlas doctor.",
        )
    })?;

    let pid = status["daemon"]["pid"]
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok())
        .ok_or_else(|| problem("The helper did not say which process it is. Run atlas restart."))?;
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
        .run(
            helper_argv_with(
                &client,
                &request,
                setup_identity(config.helper_uid, Some(&config_path))?,
            )?,
            Run::pending().within(Duration::from_secs(20)),
        )
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
        fresh: false,
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
pub async fn apply(config_path: &Path, client: &Client, _ownership: &Ownership) -> Result<Value> {
    client.require_service_idle().await?;
    let (path, log) = recorded_log(config_path)?;
    let start = apply_start(log, &path)?;
    let log = starting_log(&path, start, plan(config_path)).await?;

    // An applied setup re-verifies. An updated Atlas.app is accepted if it
    // is still validly signed as Atlas: its new hash is recorded, and the
    // running daemon's executable must be that binary. A first setup
    // accepts it however far it got, as atlas init does.
    let recheck = log.state == State::Applied || log.plan.fresh;
    carry_out(path, log, recheck).await
}

/// Check setup's prerequisites before update replaces the app. Read-only:
/// no log is created, archived or changed, and no setup mutation is dispatched.
/// The caller holds controller exclusion through this check and the swap.
pub async fn preflight_apply(config_path: &Path, updated_program: &Path) -> Result<()> {
    let (path, log) = recorded_log(config_path)?;
    let start = apply_start(log, &path)?;
    let (log, resuming) = match start {
        Start::Resume(log) => (log, true),
        Start::Plan(_) => (
            Log {
                version: 1,
                plan: plan(config_path).await?,
                state: State::Applying,
                pending: Vec::new(),
                history: Vec::new(),
            },
            false,
        ),
    };
    let migration = Migration::new(path, log)?;
    migration
        .preflight_with(resuming, updated_program, signed_daemon, async |request| {
            let through = migration
                .reachable_endpoint([Endpoint::Old, Endpoint::New])
                .await?;
            migration.read_only(through, request).await
        })
        .await
}

/// Undo an applied or interrupted migration recorded in the log.
/// Callers prove controller ownership (acquired for recovery) by passing it.
pub async fn rollback(
    config_path: &Path,
    client: &Client,
    _ownership: &Ownership,
) -> Result<Value> {
    client.require_service_idle().await?;
    let (path, log) = recorded_log(config_path)?;
    let log = log.ok_or_else(|| problem("No setup migration is recorded"))?;
    if log.plan.label.is_empty() {
        return Err(problem(
            "This setup was recorded by an older Atlas; roll it back with that version",
        ));
    }

    if log.plan.fresh {
        return Err(problem(
            "This was a first setup: there is nothing earlier to roll back to",
        ));
    }

    let mut migration = Migration::new(path, log)?;
    migration.require_owned_pending()?;
    let outcome = migration.rollback().await;

    Ok(report(&migration.log, &migration.path, outcome))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_genuine_legacy_helper_proceeds_with_an_identity_on_every_worker() {
        let recorded = super::super::identity::Identity {
            uid: 502,
            name: "helper".into(),
            generated_uid: "6DB4B7AF-BE3C-486C-BD46-1B4558946919".into(),
        };
        let sessions = legacy_sessions(1000, 900);
        let expected = setup_identity_with(
            super::super::identity::OnFile::Nothing,
            || panic!("no persistent identity exists"),
            || {
                legacy_provenance(Some(at(100)), Some(at(200)), Some(&sessions), 502)?;
                Ok(recorded.clone())
            },
        )
        .unwrap();
        let client = Client::new(config()).unwrap();
        for request in [
            Request::Inspect {
                uid: 502,
                labels: vec![LABEL.into()],
            },
            Request::Prepare {
                uid: 502,
                controller: "c".into(),
            },
            Request::Install {
                agent: Agent {
                    label: LABEL.into(),
                    program: "/Applications/Atlas.app/Contents/MacOS/Atlas".into(),
                    uid: 502,
                    controller_uid: 501,
                    socket: "/Users/helper/.atlas/helper/502.sock".into(),
                },
            },
            Request::Retire {
                uid: 502,
                label: "old".into(),
                sha256: "a".repeat(64),
                missing_ok: false,
            },
            Request::Restore {
                uid: 502,
                label: "old".into(),
                sha256: "a".repeat(64),
            },
            Request::FirstLogin {
                uid: 502,
                opener: true,
            },
            Request::ForgetOpener { uid: 502 },
            Request::Kickstart { uid: 502 },
        ] {
            let argv = helper_argv_with(&client, &request, expected.clone()).unwrap();
            assert_eq!(argv[3], "--account");
            assert_eq!(
                serde_json::from_str::<super::super::identity::Identity>(&argv[4]).unwrap(),
                recorded
            );
        }
        assert!(
            setup_identity_with(
                super::super::identity::OnFile::Unreadable,
                || None,
                || panic!("unreadable is not legacy")
            )
            .is_err()
        );
        assert!(
            setup_identity_with(
                super::super::identity::OnFile::Recorded(recorded.clone()),
                || None,
                || panic!("a mismatched identity is not legacy"),
            )
            .is_err(),
            "a mismatched or unreadable account still refuses"
        );
    }

    fn at(seconds: u64) -> SystemTime {
        std::time::UNIX_EPOCH + Duration::from_secs(seconds)
    }

    fn legacy_sessions(account_age: u64, login_age: u64) -> crate::session::Sessions {
        crate::session::Sessions {
            logins: crate::session::classify(
                &[crate::session::Window {
                    uid: 502,
                    pid: 1234,
                    age: Duration::from_secs(login_age),
                }],
                |_| crate::session::Record {
                    passwd: true,
                    directory: true,
                    age: Some(Duration::from_secs(account_age)),
                },
                501,
                Some(502),
                Some(501),
            ),
            blockers: Vec::new(),
        }
    }

    #[test]
    fn a_surviving_daemon_cannot_set_up_a_new_account_that_reused_the_helper_uid() {
        let replacement = Account {
            uid: 502,
            name: "replacement".into(),
            home: "/Users/replacement".into(),
        };
        assert_ne!(replacement.home, Path::new("/Users/helper"));
        let sessions = legacy_sessions(60, 900);
        let error = setup_identity_with(
            super::super::identity::OnFile::Nothing,
            || panic!("no persistent identity exists"),
            || {
                legacy_provenance(
                    Some(at(300)),
                    Some(at(200)),
                    Some(&sessions),
                    replacement.uid,
                )?;
                panic!("a replacement must be refused before dispatch")
            },
        )
        .unwrap_err();
        assert!(error.error.contains("Choose the helper again"));
        // Even a configuration rewritten after UID reuse cannot bless the surviving login.
        assert!(legacy_provenance(Some(at(300)), Some(at(400)), Some(&sessions), 502).is_err());
        for (created, recorded, sessions) in [
            (None, Some(at(200)), Some(&sessions)),
            (Some(at(100)), None, Some(&sessions)),
            (Some(at(100)), Some(at(200)), None),
        ] {
            assert!(legacy_provenance(created, recorded, sessions, 502).is_err());
        }
    }

    #[tokio::test]
    async fn update_preflight_refuses_a_missing_retirement_backup() {
        let dir = tempfile::tempdir().unwrap();
        let mut migration = sandbox(dir.path());
        migration.log.state = State::Applied;
        migration.log.plan.home = dir.path().join("helper");
        migration.log.plan.old_plist_sha256 = text_sha256("old definition");
        let layout = migration.layout();
        let backup = layout.root().join("backup").join(format!(
            "{}-{}.plist",
            migration.log.plan.old_label,
            &migration.log.plan.old_plist_sha256[..16]
        ));
        fs::create_dir_all(backup.parent().unwrap()).unwrap();
        let inspect = async |request| match request {
            Request::Inspect { .. } => {
                Ok(json!({"ok": true, "jobs": {LABEL: {"loaded": false, "plistKind": "absent"}}}))
            }
            Request::CheckRetire {
                label,
                sha256,
                missing_ok,
                ..
            } => {
                super::super::helper::retirement_with(
                    &layout,
                    &label,
                    &sha256,
                    missing_ok,
                    || Ok(super::super::helper::Job::Absent),
                )?;
                Ok(json!({"ok": true}))
            }
            _ => panic!("preflight must only inspect"),
        };
        fs::write(&backup, "old definition").unwrap();
        let program = Path::new("/updated/atlas");
        let signed = |path: &Path| {
            assert_eq!(path, program);
            Ok("updated-hash".into())
        };
        migration
            .preflight_with(true, program, signed, inspect)
            .await
            .unwrap();
        fs::remove_file(&backup).unwrap();
        let error = migration
            .preflight_with(true, program, signed, inspect)
            .await
            .unwrap_err();
        assert!(error.error.contains("not the expected definition"));
        assert!(!layout.agent_plist(&migration.log.plan.old_label).exists());
        assert!(
            !migration.path.exists(),
            "preflight writes no migration record"
        );
    }

    fn config() -> Config {
        Config {
            helper_uid: 502,
            socket_path: "/tmp/atlas-live-502/502.sock".into(),
            identity_probe: "/Users/Shared/Atlas/atlas".into(),
            input_socket: "/tmp/atlas-input-502/input.sock".into(),
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
            fresh: false,
        }
    }

    #[test]
    fn the_managed_layout_only_moves_sockets() {
        let layout = Layout {
            uid: 502,
            home: "/Users/atlastest".into(),
            guard: None,
        };
        let target = managed(&config(), &layout);
        assert_eq!(
            target.socket_path,
            PathBuf::from("/Users/atlastest/.atlas/helper/502.sock")
        );
        assert_eq!(
            target.input_socket,
            PathBuf::from("/Users/atlastest/.atlas/input/input.sock")
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
            plan: plan("/c/atlas.local.toml".into(), "a"),
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

    /// A setup counts for the account at its home only: a record left by a
    /// deleted account whose user ID was reused describes someone else.
    #[test]
    fn a_setup_is_in_effect_only_for_the_account_it_was_made_with() {
        let log = |state| {
            Ok(Some(Log {
                version: 1,
                plan: plan("/c/atlas.local.toml".into(), "a"),
                state,
                pending: Vec::new(),
                history: Vec::new(),
            }))
        };
        let home = Path::new("/Users/h");

        assert!(in_effect_with(log(State::Applied), home));
        assert!(in_effect_with(log(State::Applying), home));
        assert!(!in_effect_with(log(State::RolledBack), home));
        assert!(!in_effect_with(
            log(State::Applied),
            Path::new("/Users/new")
        ));
        assert!(!in_effect_with(Ok(None), home));
        assert!(
            in_effect_with(Err(problem("unreadable")), home),
            "a record that cannot be read counts"
        );
    }

    #[test]
    fn logs_round_trip_privately() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deep/setup-502.json");
        let log = Log {
            version: 1,
            plan: plan("/c/atlas.local.toml".into(), "a"),
            state: State::Applying,
            pending: vec!["g".into()],
            history: vec![("prepare".into(), "1".into())],
        };

        save_log(&path, &log).unwrap();
        assert_eq!(load_log(&path).unwrap(), Some(log));
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(load_log(&dir.path().join("missing.json")).unwrap(), None);
    }

    #[test]
    fn plans_resolve_relative_configuration_against_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("atlas.local.toml");
        let text = "helper_uid = 502\nsocket_path = \"s.sock\"\nidentity_probe = \"p\"\n\
                    input_socket = \"i.sock\"\noutput_dir = \"runs\"\n";
        let log = Log {
            version: 1,
            plan: plan(config_path, text),
            state: State::Applying,
            pending: Vec::new(),
            history: Vec::new(),
        };

        let migration = Migration::new(dir.path().join("setup-502.json"), log).unwrap();
        assert_eq!(
            migration.old.config.socket_path,
            fs::canonicalize(dir.path()).unwrap().join("s.sock")
        );
    }

    /// Only a refusal the worker marks unchanged is certain; any other
    /// failed result may have changed something.
    #[test]
    fn only_a_refusal_marked_unchanged_is_certain() {
        use base64::{Engine, engine::general_purpose::STANDARD};

        let reply = |text: &str| {
            json!({"ok": true, "exitCode": 1, "events": [
                {"event": "output", "data": STANDARD.encode(text)}
            ]})
        };

        assert!(refused_unchanged(&reply(
            r#"{"ok":false,"error":"no","unchanged":true}"#
        )));
        assert!(!refused_unchanged(&reply(r#"{"ok":false,"error":"no"}"#)));
        assert!(!refused_unchanged(&reply(
            r#"{"ok":true,"unchanged":true}"#
        )));
        assert!(!refused_unchanged(&reply("garbage")));
        assert!(!refused_unchanged(&json!({"ok": true})));
    }

    /// Uninstall archives the record of the setup it removed; the archive
    /// is never read as a setup.
    #[test]
    fn an_archived_record_is_no_longer_a_setup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("setup-502.json");
        assert!(!archive_as(&path, "uninstalled").unwrap(), "nothing there");

        let log = Log {
            version: 1,
            plan: plan("/c/atlas.toml".into(), "a"),
            state: State::Applied,
            pending: Vec::new(),
            history: Vec::new(),
        };
        save_log(&path, &log).unwrap();
        assert!(archive_as(&path, "uninstalled").unwrap());
        assert_eq!(load_log(&path).unwrap(), None);

        let names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 1);
        let name = &names[0];
        assert!(
            name.starts_with("setup-502.") && name.ends_with(".uninstalled.json"),
            "{name}"
        );

        // install.sh counts only setup-<digits>.json as a live record.
        let uid = name
            .strip_prefix("setup-")
            .and_then(|rest| rest.strip_suffix(".json"))
            .unwrap();
        assert!(!uid.bytes().all(|byte| byte.is_ascii_digit()), "{uid}");
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

    const OLD: &str = "helper_uid = 60001\nsocket_path = \"a.sock\"\nidentity_probe = \"p\"\n\
                       input_socket = \"i.sock\"\noutput_dir = \"runs\"\n";
    const NEW: &str = "helper_uid = 60001\nsocket_path = \"b.sock\"\nidentity_probe = \"p\"\n\
                       input_socket = \"i.sock\"\noutput_dir = \"runs\"\n";

    /// A migration from OLD to NEW kept entirely in `dir`: its configuration
    /// file, its log and its pending-mutation state.
    fn sandbox(dir: &Path) -> Migration {
        let config_path = dir.join("atlas.toml");
        fs::write(&config_path, OLD).unwrap();
        let mut planned = plan(config_path, OLD);
        planned.new_config = NEW.into();

        let log = Log {
            version: 1,
            plan: planned,
            state: State::Applying,
            pending: Vec::new(),
            history: Vec::new(),
        };

        let mut migration = Migration::new(dir.join("setup.json"), log).unwrap();
        let controller = crate::control::Controller::at(60001, &dir.join("state"));
        migration.old.controller = controller.clone();
        migration.new.controller = controller;

        migration
    }

    #[test]
    fn the_configuration_switches_only_from_known_contents() {
        let dir = tempfile::tempdir().unwrap();
        let mut migration = sandbox(dir.path());
        let path = migration.log.plan.config_path.clone();

        migration.switch_config(OLD, NEW).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), NEW);

        migration.switch_config(OLD, NEW).unwrap();
        assert_eq!(migration.log.history.len(), 1, "already switched");

        fs::write(&path, "edited").unwrap();
        assert!(migration.switch_config(NEW, OLD).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "edited");

        // A link is never replaced, even with the expected contents behind it.
        let real = dir.path().join("real.toml");
        fs::write(&real, NEW).unwrap();
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&real, &path).unwrap();
        assert!(migration.switch_config(NEW, OLD).is_err());
        assert!(
            fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&real).unwrap(), NEW);
    }

    #[test]
    fn uncertain_outcomes_stay_pending_and_definitive_ones_settle() {
        if !crate::test_support::isolated(
            "setup::migrate::tests::uncertain_outcomes_stay_pending_and_definitive_ones_settle",
        ) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let mut migration = sandbox(dir.path());
        let step = |migration: &mut Migration| {
            let claim = migration.begin().unwrap();
            claim.to_string()
        };

        let uncertain = step(&mut migration);
        let outcome = migration.conclude(&uncertain, Err::<(), _>(problem("lost").uncertain()));
        assert!(outcome.unwrap_err().uncertain);

        let definitive = step(&mut migration);
        assert!(
            migration
                .conclude(&definitive, Err::<(), _>(problem("refused")))
                .is_err()
        );

        let succeeded = step(&mut migration);
        assert_eq!(migration.conclude(&succeeded, Ok(7)).unwrap(), 7);
        assert_eq!(migration.log.pending, vec![uncertain.clone()]);
        assert_eq!(
            migration.old.controller.pending().unwrap(),
            vec![uncertain.clone()]
        );

        let saved = load_log(&migration.path).unwrap().unwrap();
        assert_eq!(saved.pending, vec![uncertain]);
    }

    #[test]
    fn only_pending_actions_this_setup_owns_let_it_continue() {
        if !crate::test_support::isolated(
            "setup::migrate::tests::only_pending_actions_this_setup_owns_let_it_continue",
        ) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let mut migration = sandbox(dir.path());
        migration.require_owned_pending().unwrap();

        drop(migration.begin().unwrap());
        migration.require_owned_pending().unwrap();

        drop(migration.old.controller.quarantine().unwrap());
        let error = migration.require_owned_pending().unwrap_err();
        assert!(error.uncertain);
        assert_eq!(error.code, "uncertain");
    }

    fn recorded(state: State, label: &str, fresh: bool, version: u32) -> Log {
        let mut planned = plan("/c/atlas.toml".into(), "a");
        planned.label = label.into();
        planned.fresh = fresh;

        Log {
            version,
            plan: planned,
            state,
            pending: Vec::new(),
            history: Vec::new(),
        }
    }

    #[test]
    fn apply_starts_only_from_a_log_it_can_continue() {
        let path = Path::new("/x/setup-502.json");
        assert_eq!(apply_start(None, path).unwrap(), Start::Plan(None));

        for state in [State::Applying, State::Applied] {
            let log = recorded(state, LABEL, false, 1);
            assert_eq!(
                apply_start(Some(log.clone()), path).unwrap(),
                Start::Resume(log)
            );
        }

        for refused in [
            recorded(State::Applied, LABEL, false, 2),
            recorded(State::Applying, "", false, 1),
            recorded(State::RollingBack, LABEL, false, 1),
        ] {
            assert!(
                apply_start(Some(refused.clone()), path).is_err(),
                "{refused:?}"
            );
        }

        for replanned in [
            recorded(State::RolledBack, LABEL, false, 1),
            recorded(State::Applied, "", false, 1),
        ] {
            assert_eq!(
                apply_start(Some(replanned.clone()), path).unwrap(),
                Start::Plan(Some(replanned))
            );
        }
    }

    #[test]
    fn an_update_carries_over_only_a_setup_still_in_effect() {
        for state in [State::Applying, State::Applied] {
            assert!(carried_over(Ok(Some(recorded(state, LABEL, true, 1)))));
        }

        for state in [State::RollingBack, State::RolledBack] {
            assert!(!carried_over(Ok(Some(recorded(state, LABEL, false, 1)))));
        }

        assert!(!carried_over(Ok(None)));
        assert!(carried_over(Err(problem("unreadable"))));
    }

    #[test]
    fn a_first_setup_resumes_its_own_log_and_defers_to_a_migration() {
        let path = Path::new("/x/setup-502.json");
        assert_eq!(fresh_start(None, path).unwrap(), Some(Start::Plan(None)));

        let fresh = recorded(State::Applied, LABEL, true, 1);
        assert_eq!(
            fresh_start(Some(fresh.clone()), path).unwrap(),
            Some(Start::Resume(fresh))
        );

        let migrated = recorded(State::Applied, LABEL, false, 1);
        assert_eq!(fresh_start(Some(migrated), path).unwrap(), None);
        assert!(fresh_start(Some(recorded(State::Applied, LABEL, true, 2)), path).is_err());
        assert!(fresh_start(Some(recorded(State::RollingBack, LABEL, true, 1)), path).is_err());

        // An unfinished setup by an older Atlas is archived and planned anew.
        let legacy = recorded(State::Applying, "", false, 1);
        assert_eq!(
            fresh_start(Some(legacy.clone()), path).unwrap(),
            Some(Start::Plan(Some(legacy)))
        );
    }
}
