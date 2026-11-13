//! The recorded first setup of the managed capture endpoint.
//!
//! A managed LaunchAgent runs Atlas's own signed helper daemon on a
//! persistent private socket directory. Each pass reconciles reality toward
//! the target instead of trusting completion flags: every helper action is
//! idempotent, re-inspects first and runs under a pending marker recorded in
//! a durable log, so an interrupted pass resumes without blind replays.
use super::{endpoint, reported_pid};
use crate::{
    client::{Client, Run, timestamp},
    user::capability::doctor_report,
};
use atlas_control::Ownership;
use atlas_core::{
    account::{Account, account_with_uid},
    app_bundle::signed_daemon,
    config::{Config, home},
    error::{Code, Error, Result},
    files::save_private,
    session::{Diagnosis, Observation, require_hidden},
    transport::run_output,
};
use atlas_daemon::identity::Identity;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, Instant},
};

/// A setup as recorded. A first setup moves and retires no earlier job: its
/// old label and definition hash are empty, and its old configuration is its
/// new one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Plan {
    uid: u32,
    controller: String,
    home: PathBuf,
    old_label: String,
    old_plist_sha256: String,
    label: String,
    controller_uid: u32,
    program: PathBuf,
    cdhash: String,
    config_path: PathBuf,
    old_config: String,
    new_config: String,
    /// A first setup (by `atlas init`). False only in a migration an earlier
    /// version of Atlas recorded, which is refused.
    fresh: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum State {
    Applying,
    Applied,
    RollingBack,
    RolledBack,
}

impl State {
    fn in_effect(self) -> bool {
        match self {
            Self::Applying | Self::Applied => true,
            Self::RollingBack | Self::RolledBack => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Log {
    version: u32,
    plan: Plan,
    state: State,
    /// Pending-mutation generations this setup recorded and still owns.
    pending: Vec<String>,
    /// Completed actions with nanosecond timestamps, for evidence only.
    history: Vec<(String, String)>,
}

impl Log {
    fn new(plan: Plan) -> Self {
        Self {
            version: 1,
            plan,
            state: State::Applying,
            pending: Vec::new(),
            history: Vec::new(),
        }
    }
}

const WORKER_PATIENCE: Duration = Duration::from_secs(20);

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
        Ok(Some(log)) => log.state.in_effect(),
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
///
/// # Errors
///
/// Returns identity, ownership or archive failures. An unverified foreign installation is
/// preserved rather than deleted.
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
///
/// # Errors
///
/// Returns the failure to read or move the record.
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

/// The helper the person chose, while the account is still that one. Only
/// the person's selection records an identity.
fn setup_identity(uid: u32) -> Result<Identity> {
    atlas_daemon::user::setup::identity::verified(uid).ok_or_else(choose_helper)
}

fn choose_helper() -> Error {
    atlas_daemon::user::setup::problem(
        "Atlas could not verify the helper account. Choose the helper again in Atlas to record its identity before setting it up.",
    )
}

fn helper_argv(
    client: &Client,
    request: &atlas_daemon::user::setup::Request,
    expected: &Identity,
) -> Result<Vec<String>> {
    Ok(vec![
        client.config.identity_probe.to_string_lossy().into_owned(),
        "setup-helper".to_owned(),
        serde_json::to_string(request)?,
        "--account".to_owned(),
        serde_json::to_string(expected)?,
    ])
}

fn helper_reply(reply: &Value) -> Result<Value> {
    let output = run_output(reply, 65536).map_err(|_| {
        atlas_daemon::user::setup::problem("The helper setup command returned malformed output")
    })?;
    serde_json::from_slice(&output).map_err(|error| {
        atlas_daemon::user::setup::problem("The helper setup command returned no result")
            .with_source(error)
    })
}

/// A worker's reply as an outcome: only a refusal that says it changed
/// nothing is certain.
fn helper_result(reply: Value) -> Result<Value> {
    atlas_daemon::user::setup::acknowledged(reply).map_err(|unfinished| {
        let error = atlas_daemon::user::setup::problem(format!(
            "Helper setup refused: {}",
            unfinished.reason().unwrap_or("unknown error")
        ));
        match unfinished {
            atlas_daemon::user::setup::Unfinished::Unchanged(_) => error,
            atlas_daemon::user::setup::Unfinished::Silent(_)
            | atlas_daemon::user::setup::Unfinished::Uncertain(_) => error.uncertain(),
        }
    })
}

/// Run one read-only request inside the helper as `expected`. Inspection
/// changes nothing, so it needs no pending marker.
async fn inspect(
    client: &Client,
    request: &atlas_daemon::user::setup::Request,
    expected: &Identity,
) -> Result<Value> {
    let reply = client
        .run(
            helper_argv(client, request, expected)?,
            Run::pending().within(WORKER_PATIENCE),
        )
        .await?;

    helper_reply(&reply)
        .and_then(helper_result)
        .map_err(Error::certain)
}

async fn reachable(client: &Client) -> bool {
    client
        .desktop()
        .prove(&Observation::decode(&client.status().await))
        .is_ok()
}

fn require_background(status: &Value, uid: u32) -> Result<()> {
    require_hidden(status, uid).map(|_| ()).map_err(|_| {
        atlas_daemon::user::setup::problem(
            "The helper is not running in the background. Switch back to your own account, then \
             run atlas doctor.",
        )
    })
}

fn daemon_pid(status: &Value) -> Result<u32> {
    reported_pid(&status["daemon"]["pid"]).ok_or_else(|| {
        atlas_daemon::user::setup::problem(
            "The helper did not say which process it is. Run atlas restart.",
        )
    })
}

async fn await_endpoint(client: &Client) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(15);

    while !reachable(client).await {
        if Instant::now() >= deadline {
            return Err(atlas_daemon::user::setup::problem(format!(
                "The helper did not answer in time ({}). Run atlas doctor.",
                atlas_core::config::tilde(&client.config.socket_path)
            )));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    Ok(())
}

struct Migration {
    path: PathBuf,
    log: Log,
    client: Client,
    account: OnceLock<Identity>,
}

impl Migration {
    /// The setup `log` at `path`, with a client for the plan's
    /// configuration. Construction reads nothing.
    fn new(path: PathBuf, log: Log) -> Result<Self> {
        let config = Config::parse(log.plan.new_config.as_bytes(), &log.plan.config_path)?;
        Ok(Self {
            path,
            log,
            client: Client::new(config)?,
            account: OnceLock::new(),
        })
    }

    /// The helper's identity, established once for every worker of this pass.
    fn account(&self) -> Result<&Identity> {
        if let Some(account) = self.account.get() {
            return Ok(account);
        }

        let identity = setup_identity(self.log.plan.uid)?;
        let account = account_with_uid(self.log.plan.uid)?.ok_or_else(choose_helper)?;
        if account.home != self.log.plan.home {
            return Err(choose_helper());
        }
        Ok(self.account.get_or_init(|| identity))
    }

    fn layout(&self) -> atlas_daemon::user::setup::Layout {
        atlas_daemon::user::setup::Layout::new(self.log.plan.uid, self.log.plan.home.clone())
    }

    fn agent(&self) -> atlas_daemon::user::setup::Agent {
        atlas_daemon::user::setup::Agent {
            label: self.log.plan.label.clone(),
            program: self.log.plan.program.clone(),
            uid: self.log.plan.uid,
            controller_uid: self.log.plan.controller_uid,
            socket: self.layout().paths().socket(),
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
    fn begin(&mut self) -> Result<atlas_control::Pending> {
        let generation = atlas_core::files::token();
        self.log.pending.push(generation.clone());
        save_log(&self.path, &self.log)?;
        self.client.controller.quarantine_as(&generation)
    }

    fn settle(&mut self, generation: &str) -> Result<()> {
        self.client.controller.clear_if(generation)?;
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

    /// Refuse to act while a mutation this setup does not own is pending.
    fn require_owned_pending(&self) -> Result<()> {
        let foreign = self
            .client
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

    /// Settle every generation this setup owns, once a full verification
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
    /// part way, so the generation stays pending for this setup to
    /// reconcile.
    async fn helper(&mut self, request: atlas_daemon::user::setup::Request) -> Result<Value> {
        let argv = helper_argv(&self.client, &request, self.account()?)?;
        let generation = self.begin()?;

        let outcome = match self
            .client
            .run(argv, Run::pending().within(WORKER_PATIENCE))
            .await
        {
            Ok(reply) if reply["timedOut"] == true || !reply["signal"].is_null() => Err(
                atlas_daemon::user::setup::problem("The helper setup command was interrupted")
                    .uncertain(),
            ),
            Ok(reply) => helper_reply(&reply)
                .map_err(Error::uncertain)
                .and_then(helper_result),
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

    /// The managed endpoint is private, captures, and runs the planned binary.
    async fn verify(&self) -> Result<()> {
        await_endpoint(&self.client).await?;

        let status = self.client.status().await;
        if status["permissions"]["screenCapture"] != true {
            return Err(atlas_daemon::user::setup::problem(
                "Atlas lacks Screen Recording in the helper account: log in to the \
                 helper once, allow Atlas in System Settings, switch back, then run \
                 setup again",
            ));
        }

        self.client.frame(Some(256)).await?;

        atlas_core::app_bundle::require_current(daemon_pid(&status)?, &self.log.plan.cdhash)?;

        let controller = nix::unistd::getuid().as_raw();
        let socket = endpoint(
            &self.client.config.socket_path,
            self.log.plan.uid,
            controller,
        );
        if socket.private != Some(true) {
            return Err(atlas_daemon::user::setup::problem(
                "The managed socket directory is not private",
            ));
        }

        Ok(())
    }

    /// Whether the managed socket serves the current, idle input service.
    async fn input_ready(&self) -> bool {
        let status = self.client.status().await;
        reported_pid(&status["inputService"]["pid"]).is_some_and(|pid| {
            self.client
                .desktop()
                .input_current(&Observation::decode(&status), pid)
        })
    }

    /// The same worker inspections update uses before any bundle swap: the
    /// configuration names the recorded helper, the updated program is validly
    /// signed, and the managed job is absent or the one this setup defines.
    async fn preflight_with(
        &self,
        updated_program: &Path,
        signed: impl FnOnce(&Path) -> Result<String>,
        run: impl AsyncFn(atlas_daemon::user::setup::Request) -> Result<Value>,
    ) -> Result<()> {
        self.require_owned_pending()?;
        let plan = &self.log.plan;
        // What the activity keeps may change after setup. The helper it set up
        // may not, nor the output folder that holds its sessions' records.
        let recorded = Config::parse(plan.new_config.as_bytes(), &plan.config_path)?;
        let current = fs::read(&plan.config_path)
            .ok()
            .and_then(|text| Config::parse(&text, &plan.config_path).ok());
        if !current.is_some_and(|current| {
            current.same_helper(&recorded) && current.output_dir == recorded.output_dir
        }) {
            return Err(atlas_daemon::user::setup::problem(format!(
                "{} changed during setup; it was not overwritten",
                plan.config_path.display()
            )));
        }
        signed(updated_program)?;
        let observed = run(atlas_daemon::user::setup::Request::Inspect {
            uid: plan.uid,
            labels: vec![plan.label.clone()],
        })
        .await?;
        let job =
            atlas_daemon::user::setup::launchd::JobReport::from(&observed["jobs"][&plan.label]);
        let expected = self.agent().plist_sha256(&self.layout().paths().logs());
        let expected_definition = match job.definition {
            atlas_daemon::user::setup::launchd::Definition::Absent => true,
            atlas_daemon::user::setup::launchd::Definition::Regular => {
                job.sha256.as_deref() == Some(expected.as_str())
            }
            atlas_daemon::user::setup::launchd::Definition::Other => false,
        };
        let elsewhere = !job.defined_at(&self.layout().agent_plist(&plan.label));
        if !expected_definition || job.loaded == Some(true) && elsewhere {
            return Err(atlas_daemon::user::setup::problem(
                "The helper's background job differs from its setup; run atlas init to replace it",
            ));
        }
        Ok(())
    }

    async fn apply(&mut self) -> Result<()> {
        let plan = self.log.plan.clone();
        let (uid, controller) = (plan.uid, plan.controller.clone());
        self.helper(atlas_daemon::user::setup::Request::Prepare { uid, controller })
            .await?;

        // The app may have changed since planning: install only the binary
        // that was planned, still validly signed.
        if signed_daemon(&plan.program)? != plan.cdhash {
            return Err(atlas_daemon::user::setup::problem(
                "Atlas.app changed since setup was planned; run atlas init again",
            ));
        }

        let agent = self.agent();
        self.helper(atlas_daemon::user::setup::Request::Install { agent })
            .await?;
        self.verify().await?;

        if !self.input_ready().await {
            return Err(atlas_daemon::user::setup::problem(
                "Input is not ready; run atlas start-input",
            ));
        }

        self.settle_all()?;
        self.log.state = State::Applied;

        save_log(&self.path, &self.log)
    }
}

/// What every plan starts from: the configuration (its canonical path,
/// meaning and text), both accounts, and the helper's managed layout.
struct Basis {
    config_path: PathBuf,
    config: Config,
    text: String,
    account: Account,
    controller: Account,
    layout: atlas_daemon::user::setup::Layout,
}

fn basis(config_path: &Path) -> Result<Basis> {
    let config_path = fs::canonicalize(config_path)?;
    let config = Config::load(&config_path)?;
    let text = fs::read_to_string(&config_path)?;
    let account = account_with_uid(config.desktop().user().uid)?.ok_or_else(|| {
        atlas_daemon::user::setup::problem("The configured helper account does not exist")
    })?;
    let controller = account_with_uid(nix::unistd::getuid().as_raw())?.ok_or_else(|| {
        atlas_daemon::user::setup::problem("The controller account cannot be resolved")
    })?;
    let layout =
        atlas_daemon::user::setup::Layout::new(config.desktop().user().uid, account.home.clone());

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
    let base = basis(config_path)?;
    let config = &base.config;

    if config.socket_path != base.layout.paths().socket() {
        return Err(atlas_daemon::user::setup::problem(
            "The configuration does not use the managed private layout",
        ));
    }

    let status = Client::new(config.clone())?.status().await;
    require_background(&status, config.desktop().user().uid)?;

    let program = atlas_core::app_bundle::app_executable()?;
    let cdhash = signed_daemon(&program)?;

    Ok(Plan {
        uid: config.desktop().user().uid,
        controller: base.controller.name,
        home: base.account.home,
        old_label: String::new(),
        old_plist_sha256: String::new(),
        label: atlas_daemon::user::setup::LABEL.to_owned(),
        controller_uid: base.controller.uid,
        program,
        cdhash,
        config_path: base.config_path,
        old_config: base.text.clone(),
        new_config: base.text,
        fresh: true,
    })
}

/// Why `setup --apply` with no setup to continue plans nothing: only
/// `atlas init` starts a setup.
fn unplanned(config_path: &Path) -> Error {
    match basis(config_path) {
        Err(error) => error,
        Ok(base) if base.config.socket_path == base.layout.paths().socket() => {
            atlas_daemon::user::setup::problem(
                "The configuration already uses the managed private layout",
            )
        }
        Ok(_) => atlas_daemon::user::setup::problem(
            "The configuration does not use the managed private layout",
        ),
    }
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
    atlas_daemon::user::setup::problem(format!(
        "The setup record has an unknown version ({})",
        path.display()
    ))
}

/// The refusal of a migration an earlier version of Atlas recorded. Nothing
/// is changed, so the actions it still owns stay pending.
fn earlier_migration(path: &Path) -> Error {
    atlas_daemon::user::setup::problem(format!(
        "The setup record comes from an earlier version of Atlas, which this one cannot \
         continue; run atlas uninstall, then atlas init ({})",
        path.display()
    ))
}

/// The refusal of a rollback left unfinished; only an earlier version of
/// Atlas rolled setups back.
fn rollback_unfinished(path: &Path) -> Error {
    atlas_daemon::user::setup::problem(format!(
        "A rollback is unfinished; run atlas uninstall, then atlas init ({})",
        path.display()
    ))
}

/// The recorded `log`, refused unless this version wrote it as a first setup.
fn first_setup(log: Log, path: &Path) -> Result<Log> {
    if log.version != 1 {
        return Err(unknown_version(path));
    }
    if !log.plan.fresh {
        return Err(earlier_migration(path));
    }

    Ok(log)
}

/// Where [`apply`] starts from `log`, recorded at `path`.
fn apply_start(log: Option<Log>, path: &Path) -> Result<Start> {
    let Some(log) = log else {
        return Ok(Start::Plan(None));
    };
    let log = first_setup(log, path)?;

    match log.state {
        State::Applying | State::Applied => Ok(Start::Resume(log)),
        State::RollingBack => Err(rollback_unfinished(path)),
        State::RolledBack => Ok(Start::Plan(Some(log))),
    }
}

/// Where [`fresh_pass`] starts from `log`, recorded at `path`.
fn fresh_start(log: Option<Log>, path: &Path) -> Result<Start> {
    let Some(log) = log else {
        return Ok(Start::Plan(None));
    };
    let log = first_setup(log, path)?;

    match log.state {
        State::RollingBack => Err(rollback_unfinished(path)),
        State::Applying | State::Applied | State::RolledBack => Ok(Start::Resume(log)),
    }
}

/// The setup log for the configuration at `config_path`: where it is
/// recorded, and what is recorded there.
fn recorded_log(config_path: &Path) -> Result<(PathBuf, Option<Log>)> {
    let path = log_path(Config::load(config_path)?.desktop().user().uid)?;
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

            Ok(Log::new(plan.await?))
        }
    }
}

/// A finished pass: what it reports, and why it did not complete, if it did not.
pub(crate) struct Pass {
    report: Value,
    failure: Option<String>,
}

/// Apply `log`, first recording an updated Atlas.app (still validly signed
/// as Atlas) as the same setup.
async fn carry_out(path: PathBuf, log: Log) -> Result<Pass> {
    let mut migration = Migration::new(path, log)?;
    migration.require_owned_pending()?;

    let current = signed_daemon(&migration.log.plan.program)?;
    if current != migration.log.plan.cdhash {
        migration.log.plan.cdhash = current;
        migration.note("helper daemon updated")?;
    }

    migration.log.state = State::Applying;
    save_log(&migration.path, &migration.log)?;
    let outcome = migration.apply().await;
    let failure = outcome.as_ref().err().map(|error| error.error.clone());

    Ok(Pass {
        report: report(&migration.log, &migration.path, outcome),
        failure,
    })
}

/// Start input, record and verify a first setup, and diagnose the helper it
/// left. Setup that could not be verified is refused with `code`, the caller's.
///
/// # Errors
///
/// Returns input, identity, layout, executor or setup failures. Partial or unknown
/// setup work retains the saved recovery state.
pub async fn finalize(config_path: &Path, client: &Client, code: Code) -> Result<Diagnosis> {
    client.start_input().await?;
    let ownership = client.controller.acquire(true)?;
    let pass = fresh_pass(config_path, client, &ownership).await?;
    drop(ownership);

    if let Some(failure) = pass.failure {
        return Err(Error::new(
            code,
            format!("Setup could not be verified: {failure}"),
        ));
    }

    let status = client.status().await;
    Ok(doctor_report(status, client.desktop().user()).diagnosis)
}

/// Record and verify a first setup (see [`plan_fresh`]). Re-running on an
/// applied setup re-verifies it, like [`apply`].
///
/// # Errors
///
/// Returns a refusal while the service is in use or for a migration an earlier
/// version of Atlas recorded, or the failure of the setup it runs. Partial or
/// unknown setup work retains the saved recovery state.
pub(crate) async fn fresh_pass(
    config_path: &Path,
    client: &Client,
    _ownership: &Ownership,
) -> Result<Pass> {
    client.require_service_idle().await?;
    let (path, log) = recorded_log(config_path)?;
    let start = fresh_start(log, &path)?;
    let log = starting_log(&path, start, plan_fresh(config_path)).await?;

    // An updated Atlas.app, still validly signed as Atlas, continues the
    // same setup: its new signature is recorded, as apply does.
    carry_out(path, log).await
}

/// How launchd and the helper's LaunchAgents folder show the managed job,
/// read through `client`'s daemon (read-only, so no pending marker): the
/// worker's report for [`LABEL`](atlas_daemon::user::setup::LABEL) (`loaded`, `plistKind`, `plistSha256`...).
///
/// # Errors
///
/// Returns the failure to verify the helper or to read the job through its
/// daemon.
pub async fn managed_job(client: &Client) -> Result<atlas_daemon::user::setup::launchd::JobReport> {
    let request = atlas_daemon::user::setup::Request::Inspect {
        uid: client.desktop().user().uid,
        labels: vec![atlas_daemon::user::setup::LABEL.to_owned()],
    };
    let expected = setup_identity(client.desktop().user().uid)?;

    Ok(atlas_daemon::user::setup::launchd::JobReport::from(
        &inspect(client, &request, &expected).await?["jobs"][atlas_daemon::user::setup::LABEL],
    ))
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

/// Resume or re-verify the recorded first setup.
/// Callers prove controller ownership (acquired for recovery) by passing it.
///
/// # Errors
///
/// Returns admission, record, account-identity, setup or settlement failures.
/// With no setup recorded it plans nothing and refuses. Pending work is cleared
/// only after the existing read-back proof succeeds.
pub async fn apply(config_path: &Path, client: &Client, _ownership: &Ownership) -> Result<Value> {
    client.require_service_idle().await?;
    let (path, log) = recorded_log(config_path)?;
    let start = apply_start(log, &path)?;
    let log = starting_log(&path, start, async { Err(unplanned(config_path)) }).await?;

    // An updated Atlas.app is accepted if it is still validly signed as
    // Atlas, however far the setup got: its new hash is recorded, and the
    // running daemon's executable must be that binary.
    Ok(carry_out(path, log).await?.report)
}

/// Check setup's prerequisites before update replaces the app. Read-only:
/// no log is created, archived or changed, and no setup mutation is dispatched.
/// The caller holds controller exclusion through this check and the swap.
///
/// # Errors
///
/// Rejects unavailable settings, a record this version cannot continue,
/// incompatible account identity or an unsuitable replacement program before
/// setup effects begin.
pub async fn preflight_apply(config_path: &Path, updated_program: &Path) -> Result<()> {
    let (path, log) = recorded_log(config_path)?;
    let Start::Resume(log) = apply_start(log, &path)? else {
        return Err(unplanned(config_path));
    };
    let migration = Migration::new(path, log)?;
    migration
        .preflight_with(updated_program, signed_daemon, async |request| {
            if !reachable(&migration.client).await {
                return Err(atlas_daemon::user::setup::problem(
                    "The helper is not running in the background. Log in to the helper account \
                     once, switch back, then run atlas init again.",
                ));
            }
            inspect(&migration.client, &request, migration.account()?).await
        })
        .await
}

#[cfg(test)]
#[path = "migrate_tests.rs"]
mod tests;
