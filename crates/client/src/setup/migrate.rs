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
use super::{configuration::managed, endpoint, helper_job, reported_pid};
use crate::client::{Client, Run, timestamp};
use atlas_control::Ownership;
use atlas_core::{
    account::{Account, account_with_uid},
    app_bundle::signed_daemon,
    config::{Config, home},
    error::{Code, Error, Result},
    files::save_private,
    session::{input_current, require_hidden},
    transport::run_output,
};
use atlas_daemon::helper_setup::{
    Agent, Definition, JobReport, LABEL, Layout, Request, Unfinished, acknowledged,
    identity::{self, Identity},
    problem,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, Instant},
};

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
    /// A first setup (by `atlas init`): no earlier capture job, so
    /// nothing is moved or retired and there is nothing to roll back to.
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
    /// Pending-mutation generations this migration recorded and still owns.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Endpoint {
    Old,
    New,
}

const WORKER_PATIENCE: Duration = Duration::from_secs(20);

const APP_CHANGED: &str =
    "Atlas.app changed since setup was planned; run setup --rollback, then setup --apply again";

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
    identity::verified(uid).ok_or_else(choose_helper)
}

fn choose_helper() -> Error {
    problem(
        "Atlas could not verify the helper account. Choose the helper again in Atlas to record its identity before setting it up.",
    )
}

fn helper_argv(client: &Client, request: &Request, expected: &Identity) -> Result<Vec<String>> {
    Ok(vec![
        client.config.identity_probe.to_string_lossy().into_owned(),
        "setup-helper".to_owned(),
        serde_json::to_string(request)?,
        "--account".to_owned(),
        serde_json::to_string(expected)?,
    ])
}

fn helper_reply(reply: &Value) -> Result<Value> {
    let output = run_output(reply, 65536)
        .map_err(|_| problem("The helper setup command returned malformed output"))?;
    serde_json::from_slice(&output)
        .map_err(|error| problem("The helper setup command returned no result").with_source(error))
}

/// A worker's reply as an outcome: only a refusal that says it changed
/// nothing is certain.
fn helper_result(reply: Value) -> Result<Value> {
    acknowledged(reply).map_err(|unfinished| {
        let error = problem(format!(
            "Helper setup refused: {}",
            unfinished.reason().unwrap_or("unknown error")
        ));
        match unfinished {
            Unfinished::Unchanged(_) => error,
            Unfinished::Silent(_) | Unfinished::Uncertain(_) => error.uncertain(),
        }
    })
}

/// Run one read-only request inside the helper as `expected`. Inspection
/// changes nothing, so it needs no pending marker.
async fn inspect(client: &Client, request: &Request, expected: &Identity) -> Result<Value> {
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

    reported_pid(&hello["pid"])
        .filter(|_| hello["ok"] == true)
        .map(Some)
        .ok_or_else(|| problem("Input service state is unknown"))
}

async fn reachable(client: &Client) -> bool {
    require_hidden(&client.status().await, client.config.helper_uid).is_ok()
}

fn require_background(status: &Value, uid: u32) -> Result<()> {
    require_hidden(status, uid).map(|_| ()).map_err(|_| {
        problem(
            "The helper is not running in the background. Switch back to your own account, then \
             run atlas doctor.",
        )
    })
}

fn daemon_pid(status: &Value) -> Result<u32> {
    reported_pid(&status["daemon"]["pid"])
        .ok_or_else(|| problem("The helper did not say which process it is. Run atlas restart."))
}

async fn await_endpoint(client: &Client) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(15);

    while !reachable(client).await {
        if Instant::now() >= deadline {
            return Err(problem(format!(
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
    old: Client,
    new: Client,
    account: OnceLock<Identity>,
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

    fn client(&self, endpoint: Endpoint) -> &Client {
        match endpoint {
            Endpoint::Old => &self.old,
            Endpoint::New => &self.new,
        }
    }

    fn layout(&self) -> Layout {
        Layout::new(self.log.plan.uid, self.log.plan.home.clone())
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
    fn begin(&mut self) -> Result<atlas_control::Pending> {
        let generation = atlas_core::files::token();
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
        let argv = helper_argv(self.client(through), &request, self.account()?)?;
        let generation = self.begin()?;

        let outcome = match self
            .client(through)
            .run(argv, Run::pending().within(WORKER_PATIENCE))
            .await
        {
            Ok(reply) if reply["timedOut"] == true || !reply["signal"].is_null() => {
                Err(problem("The helper setup command was interrupted").uncertain())
            }
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

        atlas_core::app_bundle::require_current(daemon_pid(&status)?, &self.log.plan.cdhash)?;

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
        reported_pid(&status["inputService"]["pid"])
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

    async fn jobs(&self, through: Endpoint) -> Result<(JobReport, JobReport)> {
        let request = Request::Inspect {
            uid: self.log.plan.uid,
            labels: vec![self.log.plan.old_label.clone(), self.log.plan.label.clone()],
        };
        let inspected = self.read_only(through, &request).await?;
        let jobs = &inspected["jobs"];
        Ok((
            JobReport::from(&jobs[&self.log.plan.old_label]),
            JobReport::from(&jobs[&self.log.plan.label]),
        ))
    }

    async fn read_only(&self, through: Endpoint, request: &Request) -> Result<Value> {
        inspect(self.client(through), request, self.account()?).await
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
            return Err(problem(APP_CHANGED));
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
        let job = JobReport::from(&observed["jobs"][&plan.label]);
        let expected = self.agent().plist_sha256(&self.layout().logs());
        let expected_definition = match job.definition {
            Definition::Absent => true,
            Definition::Regular => job.sha256.as_deref() == Some(expected.as_str()),
            Definition::Other => false,
        };
        let elsewhere = !job.defined_at(&self.layout().agent_plist(&plan.label));
        if !expected_definition || job.loaded == Some(true) && elsewhere {
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

    async fn apply(&mut self) -> Result<()> {
        let plan = self.log.plan.clone();
        let uid = plan.uid;
        let via = if plan.fresh {
            Endpoint::New
        } else {
            self.reachable_endpoint([Endpoint::Old, Endpoint::New])
                .await?
        };
        let controller = plan.controller.clone();
        self.helper(via, Request::Prepare { uid, controller })
            .await?;

        // The app may have changed since planning: install only the binary
        // that was planned, still validly signed.
        if signed_daemon(&plan.program)? != plan.cdhash {
            return Err(problem(if plan.fresh {
                "Atlas.app changed since setup was planned; run atlas init again"
            } else {
                APP_CHANGED
            }));
        }

        let agent = self.agent();
        self.helper(via, Request::Install { agent }).await?;
        self.verify_new().await?;
        if !plan.fresh {
            self.move_input(Endpoint::Old, Endpoint::New).await?;
            self.switch_config(&plan.old_config, &plan.new_config)?;
            // The fallback goes only once its replacement is verified again.
            self.verify_new().await?;
        }

        if !self.input_ready(Endpoint::New).await {
            return Err(problem(if plan.fresh {
                "Input is not ready; run atlas start-input"
            } else {
                "Input is not ready on the managed endpoint"
            }));
        }

        if !plan.fresh {
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

            let (old, new) = self.jobs(Endpoint::New).await?;
            if old.loaded != Some(false) || new.loaded != Some(true) {
                return Err(problem(
                    "launchd does not show the expected jobs after retiring",
                ));
            }
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
        let sha256 = self.agent().plist_sha256(&self.layout().logs());
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

        let (old, new) = self.jobs(Endpoint::Old).await?;
        if old.loaded != Some(true) || new.loaded != Some(false) {
            return Err(problem(
                "launchd does not show the original job after rollback",
            ));
        }

        self.settle_all()?;
        self.log.state = State::RolledBack;

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
    let layout = Layout::new(config.helper_uid, account.home.clone());

    Ok(Basis {
        config_path,
        config,
        text,
        account,
        controller,
        layout,
    })
}

impl Basis {
    /// The plan on this basis for the signed `program`: from `old` (the
    /// earlier job's label and definition hash) to `new_config`, or without
    /// one a first setup.
    fn into_plan(
        self,
        program: PathBuf,
        cdhash: String,
        old: Option<(String, String)>,
        new_config: String,
    ) -> Plan {
        let fresh = old.is_none();
        let (old_label, old_plist_sha256) = old.unwrap_or_default();

        Plan {
            uid: self.config.helper_uid,
            controller: self.controller.name,
            home: self.account.home,
            old_label,
            old_plist_sha256,
            label: LABEL.to_owned(),
            controller_uid: self.controller.uid,
            program,
            cdhash,
            config_path: self.config_path,
            old_config: self.text,
            new_config,
            fresh,
        }
    }
}

/// Plan a first setup of the configuration at `config_path`, whose helper
/// already runs the managed daemon (started by `atlas init`).
async fn plan_fresh(config_path: &Path) -> Result<Plan> {
    let base = basis(config_path)?;
    let config = &base.config;

    if config.socket_path != base.layout.socket() {
        return Err(problem(
            "The configuration does not use the managed private layout",
        ));
    }

    let status = Client::new(config.clone())?.status().await;
    require_background(&status, config.helper_uid)?;

    let program = atlas_core::app_bundle::app_executable()?;
    let cdhash = signed_daemon(&program)?;
    let new_config = base.text.clone();

    Ok(base.into_plan(program, cdhash, None, new_config))
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

fn apply_start(log: Option<Log>, path: &Path) -> Result<Start> {
    let Some(log) = log else {
        return Ok(Start::Plan(None));
    };
    if log.version != 1 {
        return Err(unknown_version(path));
    }

    match log.state {
        State::Applying | State::Applied => Ok(Start::Resume(log)),
        State::RollingBack => Err(rollback_unfinished(path)),
        State::RolledBack => Ok(Start::Plan(Some(log))),
    }
}

/// Where [`apply_fresh`] starts from `log`, recorded at `path`; None for a
/// migrated (not fresh) setup, which re-verifies the way it was made.
fn fresh_start(log: Option<Log>, path: &Path) -> Result<Option<Start>> {
    let Some(log) = log else {
        return Ok(Some(Start::Plan(None)));
    };
    if log.version != 1 {
        return Err(unknown_version(path));
    }

    match log.state {
        State::RollingBack => Err(rollback_unfinished(path)),
        State::Applying | State::Applied | State::RolledBack => {
            Ok(log.plan.fresh.then_some(Start::Resume(log)))
        }
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

            Ok(Log::new(plan.await?))
        }
    }
}

/// A finished pass: what it reports, and why it did not complete, if it did not.
pub struct Pass {
    report: Value,
    pub failure: Option<String>,
}

/// Apply `log`, first recording an updated Atlas.app (still validly signed
/// as Atlas) as the same setup when `recheck` is set.
async fn carry_out(path: PathBuf, log: Log, recheck: bool) -> Result<Pass> {
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
    let failure = outcome.as_ref().err().map(|error| error.error.clone());

    Ok(Pass {
        report: report(&migration.log, &migration.path, outcome),
        failure,
    })
}

/// Record and verify a first setup (see [`plan_fresh`]). Re-running on an
/// applied setup re-verifies it, like [`apply`].
///
/// # Errors
///
/// Returns identity, layout, executor or migration failures. Partial or unknown migration
/// work retains the saved recovery state.
pub async fn apply_fresh(
    config_path: &Path,
    client: &Client,
    ownership: &Ownership,
) -> Result<Value> {
    Ok(fresh_pass(config_path, client, ownership).await?.report)
}

/// # Errors
///
/// Returns a refusal while the service is in use, or the failure of the setup
/// it runs.
pub async fn fresh_pass(
    config_path: &Path,
    client: &Client,
    ownership: &Ownership,
) -> Result<Pass> {
    client.require_service_idle().await?;
    let (path, log) = recorded_log(config_path)?;
    let Some(start) = fresh_start(log, &path)? else {
        return applied(config_path, client, ownership).await;
    };

    let log = starting_log(&path, start, plan_fresh(config_path)).await?;

    // An updated Atlas.app, still validly signed as Atlas, continues the
    // same setup: its new signature is recorded, as apply does.
    carry_out(path, log, true).await
}

/// How launchd and the helper's LaunchAgents folder show the managed job,
/// read through `client`'s daemon (read-only, so no pending marker): the
/// worker's report for [`LABEL`] (`loaded`, `plistKind`, `plistSha256`...).
///
/// # Errors
///
/// Returns the failure to verify the helper or to read the job through its
/// daemon.
pub async fn managed_job(client: &Client) -> Result<JobReport> {
    let request = Request::Inspect {
        uid: client.config.helper_uid,
        labels: vec![LABEL.to_owned()],
    };
    let expected = setup_identity(client.config.helper_uid)?;

    Ok(JobReport::from(
        &inspect(client, &request, &expected).await?["jobs"][LABEL],
    ))
}

/// Plan from the live configuration. Reads only; refuses unless the managed
/// label has no artifacts and the current job is a verified plain plist.
async fn plan(config_path: &Path) -> Result<Plan> {
    let base = basis(config_path)?;
    let (config, layout) = (&base.config, &base.layout);

    let target = managed(config, layout);
    if config.socket_path == target.socket_path {
        return Err(problem(
            "The configuration already uses the managed private layout",
        ));
    }

    let client = Client::new(config.clone())?;
    let built = std::env::current_exe()
        .and_then(fs::canonicalize)
        .ok()
        .and_then(|path| atlas_core::digest::file_sha256(&path));
    if built.is_none() || client.installed_build() != built {
        return Err(problem(
            "The helper runs the installed executable; run atlas install first",
        ));
    }

    let status = client.status().await;
    require_background(&status, config.helper_uid)?;

    let pid = daemon_pid(&status)?;
    let program = atlas_core::app_bundle::app_executable()?;
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

    let expected = setup_identity(config.helper_uid)?;
    let inspected = inspect(&client, &request, &expected).await?;
    let jobs = &inspected["jobs"];

    let old = JobReport::from(&jobs[&old_label]);
    let old_plist_sha256 = old
        .sha256
        .clone()
        .filter(|_| {
            old.loaded == Some(true)
                && old.definition == Definition::Regular
                && old.defined_at(&layout.agent_plist(&old_label))
        })
        .ok_or_else(|| {
            problem("The current capture job is not loaded from a plain plist in LaunchAgents")
        })?;

    let managed = JobReport::from(&jobs[LABEL]);
    if managed.loaded != Some(false) || managed.definition != Definition::Absent {
        return Err(problem(format!(
            "{LABEL} already exists in the helper; remove it before setup manages it"
        )));
    }

    let new_config = target.to_toml()?;
    Ok(base.into_plan(
        program,
        cdhash,
        Some((old_label, old_plist_sha256)),
        new_config,
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

/// Migrate, resume, or re-verify the managed private capture endpoint.
/// Callers prove controller ownership (acquired for recovery) by passing it.
///
/// # Errors
///
/// Returns admission, account-identity, migration or settlement failures. Pending work is
/// cleared only after the existing read-back proof succeeds.
pub async fn apply(config_path: &Path, client: &Client, ownership: &Ownership) -> Result<Value> {
    Ok(applied(config_path, client, ownership).await?.report)
}

async fn applied(config_path: &Path, client: &Client, _ownership: &Ownership) -> Result<Pass> {
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
///
/// # Errors
///
/// Rejects unavailable settings, incompatible account identity or an unsuitable replacement
/// program before migration effects begin.
pub async fn preflight_apply(config_path: &Path, updated_program: &Path) -> Result<()> {
    let (path, log) = recorded_log(config_path)?;
    let start = apply_start(log, &path)?;
    let (log, resuming) = match start {
        Start::Resume(log) => (log, true),
        Start::Plan(_) => (Log::new(plan(config_path).await?), false),
    };
    let migration = Migration::new(path, log)?;
    migration
        .preflight_with(resuming, updated_program, signed_daemon, async |request| {
            let through = migration
                .reachable_endpoint([Endpoint::Old, Endpoint::New])
                .await?;
            migration.read_only(through, &request).await
        })
        .await
}

/// Undo an applied or interrupted migration recorded in the log.
/// Callers prove controller ownership (acquired for recovery) by passing it.
///
/// # Errors
///
/// Returns ownership, identity, rollback or read-back failures. Unknown rollback work
/// remains pending.
pub async fn rollback(
    config_path: &Path,
    client: &Client,
    _ownership: &Ownership,
) -> Result<Value> {
    client.require_service_idle().await?;
    let (path, log) = recorded_log(config_path)?;
    let log = log.ok_or_else(|| problem("No setup migration is recorded"))?;
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
mod tests;
