//! The controller's side of the helper: one [`Client`] per configuration
//! that talks to the helper daemon and the input service over their
//! sockets, proves the helper is the hidden session it expects before every
//! action, holds controller exclusion while acting, and keeps the
//! screenshots and activity log in the private output folder.
use crate::{
    actions,
    config::Config,
    control::{Controller, Ownership, SharedLease, write_private},
    digest::file_sha256,
    doing::Doing,
    error::{Error, Result},
    session::{Display, Epoch, Session, input_current, input_settled, require_hidden},
    transport::{Transport, run_output},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use fs2::FileExt;
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub struct Frame {
    pub png: Arc<[u8]>,
    pub width: u32,
    pub height: u32,
    pub display: Display,
    pub epoch: Epoch,
    pub at: Instant,
}

/// Identity (device, inode, size, mtime) of the installed executable and its hash.
type BuildCache = Arc<std::sync::Mutex<Option<((u64, u64, u64, i64, i64), Option<String>)>>>;

/// How long `run` lets a command take unless told otherwise.
pub const RUN_TIMEOUT: Duration = Duration::from_secs(20);
/// The longest a command may run (the helper's own limit).
pub const RUN_TIMEOUT_MAX: Duration = Duration::from_secs(600);
/// Time beyond a command's timeout for the helper to stop what it left
/// running and send its last output.
const RUN_MARGIN: Duration = Duration::from_secs(10);

/// Who recorded mutations are attributed to in the activity log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Actor {
    Agent,
    Viewer,
}

impl Actor {
    pub fn as_str(self) -> &'static str {
        match self {
            Actor::Agent => "agent",
            Actor::Viewer => "viewer",
        }
    }
}

#[derive(Clone)]
pub struct Client {
    pub config: Config,
    pub transport: Transport,
    pub controller: Controller,
    pub actor: Actor,
    /// What the command acting through this client does, for the log.
    pub doing: Option<Doing>,
    installed: BuildCache,
}

/// Screenshots are kept for a week; older ones are removed as new ones come.
fn prune_captures(output: &Path) {
    const WEEK: Duration = Duration::from_secs(7 * 24 * 60 * 60);

    let Ok(entries) = fs::read_dir(output) else {
        return;
    };

    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > WEEK);

        let capture = entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.ends_with("-capture.png"));
        if old && capture && entry.file_type().is_ok_and(|kind| kind.is_file()) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// The folder holding the shared command: made (or repaired) so the helper
/// can pass through it, whatever this terminal's umask. Only a folder of
/// this account is changed; anyone else's is left for the checks to refuse.
fn shared_folder(folder: &Path) -> Result<()> {
    match fs::symlink_metadata(folder) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(folder)?;
        }
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }

    let metadata = fs::symlink_metadata(folder)?;
    let me = nix::unistd::getuid().as_raw();
    // Any account can make folders in /Users/Shared: one another account
    // made, or that others can change, is never used.
    let foreign = metadata.uid() != me && metadata.uid() != 0;
    let open = metadata.uid() == 0 && metadata.mode() & 0o022 != 0;
    if !metadata.file_type().is_dir() || foreign || open {
        return Err(Error::new(
            "install",
            format!(
                "{} belongs to another account, or others can change it. Atlas is set up for \
                 one person per Mac; if no one else uses it, remove the folder (an \
                 administrator can) and try again.",
                folder.display()
            ),
        ));
    }

    if metadata.uid() == me && metadata.mode() & 0o777 != 0o755 {
        fs::set_permissions(folder, fs::Permissions::from_mode(0o755))?;
    }

    Ok(())
}

pub fn timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

impl Client {
    pub fn install(&self) -> Result<Value> {
        self.install_probe(false)
    }

    /// Repair the probe under recovery exclusion without settling previous work.
    pub(crate) fn bootstrap_probe(&self) -> Result<Value> {
        self.install_probe(true)
    }

    fn install_probe(&self, recovery: bool) -> Result<Value> {
        let source = fs::canonicalize(std::env::current_exe()?)?;
        let target = &self.config.identity_probe;

        let shared = target
            .parent()
            .filter(|parent| Path::new(crate::handoff::ROOT).parent() == Some(*parent));
        let folder_ready = shared.is_none_or(|parent| {
            fs::symlink_metadata(parent).is_ok_and(|metadata| {
                metadata.is_dir()
                    && (metadata.uid() == nix::unistd::getuid().as_raw()
                        && metadata.mode() & 0o777 == 0o755
                        || metadata.uid() == 0 && metadata.mode() & 0o022 == 0)
            })
        });
        // An unchanged link and folder need no mutation or exclusion.
        // A copy is always replaced, using a fresh inode.
        if folder_ready && fs::canonicalize(target).ok().as_ref() == Some(&source) {
            return Ok(json!({ "ok": true, "path": target, "alreadyInstalled": true }));
        }

        let _ownership = self.controller.acquire(recovery)?;
        if let Some(parent) = shared {
            shared_folder(parent)?;
        }
        if fs::canonicalize(target).ok().as_ref() == Some(&source) {
            return Ok(json!({ "ok": true, "path": target, "alreadyInstalled": true }));
        }

        let parent = target
            .parent()
            .ok_or_else(|| Error::new("config", "Shared executable needs a parent directory"))?;
        fs::create_dir_all(parent)?;

        // Atlas.app's executable is sealed with its bundle: a copy would
        // fail its signature, so the shared path links to it instead, and
        // follows the app through updates.
        if crate::update::app_of(&source).is_some() {
            if !crate::update::reachable_by_others(&source) {
                return Err(Error::new(
                    "install",
                    "The helper account cannot reach Atlas.app here; install it in /Applications",
                ));
            }

            let temporary = target.with_extension(format!("{}.new", crate::control::token()));
            let linked = std::os::unix::fs::symlink(&source, &temporary)
                .and_then(|()| fs::rename(&temporary, target));
            if linked.is_err() {
                let _ = fs::remove_file(&temporary);
            }

            linked?;
            return Ok(json!({ "ok": true, "path": target, "linkedTo": source }));
        }

        crate::config::write_beside(
            target,
            0o755,
            |file| {
                std::io::copy(&mut fs::File::open(&source)?, file)?;
                file.set_permissions(fs::Permissions::from_mode(0o755))?;
                Ok(())
            },
            // A running helper may still map the previous executable. Renaming
            // a fresh inode avoids modifying its code-signed pages in place.
            |temporary| Ok(fs::rename(temporary, target)?),
        )?;

        Ok(json!({ "ok": true, "path": target }))
    }

    /// Construction only validates; reads such as status never write; doctor
    /// only probes that Atlas's own folders can be written, making them when
    /// missing.
    pub fn new(config: Config) -> Result<Self> {
        crate::session::validate_uid(config.helper_uid)?;
        Ok(Self {
            controller: Controller::new(config.helper_uid)?,
            transport: Transport {
                path: config.socket_path.clone(),
                timeout: Duration::from_secs(30),
                peer_uid: config.helper_uid,
            },
            installed: BuildCache::default(),
            actor: Actor::Agent,
            doing: None,
            config,
        })
    }

    /// This client, recording what its command does in each activity row.
    pub fn doing(mut self, doing: Doing) -> Self {
        self.doing = Some(doing);
        self
    }

    /// This client, keeping no control's name in what it records.
    pub fn unlabelled(mut self) -> Self {
        self.doing = self.doing.map(Doing::unlabelled);
        self
    }

    /// The private output directory, created 0700 (with any missing parents)
    /// on first use. An existing directory must already be private; it is
    /// never re-permissioned. The umask can only remove mode bits.
    pub fn output_dir(&self) -> Result<&Path> {
        let path = &self.config.output_dir;
        if !path.exists() {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(path)
                .map_err(|error| Error::writing(path, error))?;
        }

        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_dir()
            || metadata.uid() != nix::unistd::getuid().as_raw()
            || metadata.mode() & 0o077 != 0
        {
            return Err(Error::new(
                "config",
                format!(
                    "Output directory {} must be a private (0700) directory you own",
                    path.display()
                ),
            ));
        }

        Ok(path)
    }

    pub async fn status(&self) -> Value {
        self.verified_status().await.unwrap_or_else(|e| json!(e))
    }

    async fn verified_status(&self) -> Result<Value> {
        let mut hello = self.transport.call(&json!({"op": "hello"})).await?;

        // Do not launch the probe into an unverified UID or the visible desktop.
        if hello["ok"] != true
            || hello["user"]["uid"].as_u64() != Some(self.config.helper_uid.into())
            || hello["session"]["onConsole"] != false
        {
            return Ok(hello);
        }

        hello["inputService"] = self
            .input_transport()
            .call(&json!({ "op": "hello" }))
            .await
            .unwrap_or_else(|error| json!(error));
        hello["installedBuild"] = json!(self.installed_build());
        hello["sessionProof"] = self
            .session_proof()
            .await
            .unwrap_or_else(|error| json!(error));

        Ok(hello)
    }

    async fn session_proof(&self) -> Result<Value> {
        let request = json!({
            "op": "run",
            "argv": [self.config.identity_probe, "session-info"],
            "timeoutMs": 3000,
        });
        let reply = self.transport.call(&request).await?;
        if reply["ok"] != true || reply["exitCode"] != 0 || reply["timedOut"] != false {
            return Err(Error::new(
                "identity",
                format!(
                    "The helper cannot run the shared Atlas command at {}. Run atlas \
                     install.",
                    self.config.identity_probe.display()
                ),
            ));
        }

        let output = run_output(&reply, 4096)
            .map_err(|_| Error::new("identity", "Malformed or oversized identity output"))?;

        Ok(serde_json::from_slice(&output)?)
    }

    pub async fn identity(&self) -> Result<Session> {
        require_hidden(&self.status().await, self.config.helper_uid)
    }

    pub async fn frame(&self, maximum: Option<u32>) -> Result<Frame> {
        if maximum.is_some_and(|m| m == 0 || m > 8192) {
            return Err(Error::new("input", "Maximum dimension must be 1–8192"));
        }

        let before = self.identity().await?;
        let mut request = json!({"op": "screenshot"});

        if let Some(max) = maximum {
            request["maxDimension"] = json!(max);
        }

        let reply = self.transport.call(&request).await?;
        if reply["ok"] != true {
            if reply["code"].is_string() {
                return Err(crate::transport::refusal(&reply));
            }

            return Err(Error::new(
                "capture",
                reply["error"].as_str().unwrap_or("Capture failed"),
            ));
        }

        let at = Instant::now();
        let encoded = reply["png"]
            .as_str()
            .ok_or_else(|| Error::new("capture", "Missing PNG"))?;
        let png = STANDARD
            .decode(encoded)
            .map_err(|_| Error::new("capture", "Invalid PNG encoding"))?;
        let (width, height) = png_dimensions(&png)?;
        if reply["width"].as_u64() != Some(width.into())
            || reply["height"].as_u64() != Some(height.into())
        {
            return Err(Error::new(
                "capture",
                "PNG dimensions disagree with metadata",
            ));
        }

        let after = self.identity().await?;
        if before.epoch != after.epoch {
            return Err(Error::new(
                "session-changed",
                "Session changed during capture; observe again",
            ));
        }

        Ok(Frame {
            png: png.into(),
            width,
            height,
            display: after.epoch.display.clone(),
            epoch: after.epoch,
            at,
        })
    }

    pub async fn capture(&self, maximum: Option<u32>) -> Result<Value> {
        let started = Instant::now();
        let captured = self.capture_unrecorded(maximum).await;

        // An agent's look at the screen is activity too; a recording that
        // fails never fails the capture.
        let request = json!({"op": "screenshot"});
        let reply = match &captured {
            Ok(_) => json!({"ok": true}),
            Err(error) => json!({"ok": false, "code": recorded_code(error)}),
        };
        let _ = self.record(&request, &reply, started.elapsed());

        captured
    }

    /// [`Client::capture`], kept in the output folder but not in the log.
    async fn capture_unrecorded(&self, maximum: Option<u32>) -> Result<Value> {
        let frame = self.frame(maximum).await?;
        let path = self
            .output_dir()?
            .join(format!("{}-capture.png", timestamp()));
        write_private(&path, &frame.png)?;
        prune_captures(self.output_dir()?);

        Ok(json!({
            "ok": true,
            "path": path,
            "width": frame.width,
            "height": frame.height,
            "display": frame.display,
            // Input coordinates are display points: multiply image pixels by this.
            "pointsPerPixel": frame.display.width / f64::from(frame.width),
        }))
    }

    pub fn input_transport(&self) -> Transport {
        Transport {
            path: self.config.input_socket.clone(),
            timeout: Duration::from_secs(5),
            peer_uid: self.config.helper_uid,
        }
    }

    pub async fn native_control(&self, operation: &str, owner: &str) -> Result<Value> {
        self.native_request(json!({ "op": operation, "owner": owner }))
            .await
    }

    pub async fn acquire_input(&self, owner: &str, epoch: &Epoch) -> Result<Value> {
        // The person's viewer may take over while agents are stopped.
        self.pending_until_known(self.native_request(json!({
            "op": "acquire",
            "owner": owner,
            "epoch": epoch.input_epoch(),
            "person": self.actor == Actor::Viewer,
        })))
        .await
    }

    /// Stop agents in the helper (`on`), or let them go on. Stopping ends
    /// an agent's input under way and refuses its next actions; the person
    /// keeps the viewer. Only the person resumes, through a viewer. An input
    /// service too old to stop is refused.
    pub async fn hold(&self, on: bool) -> Result<Value> {
        if !on && self.actor != Actor::Viewer {
            return Err(Error::new(
                "stopped",
                "Only the person resumes agents, from the Atlas menu bar",
            ));
        }

        // The person's side keeps the Stop first, so it holds even when the
        // helper's input service is down or older: agents' commands read it.
        if on {
            self.controller.note_stop(true)?;
        }

        let unreachable = |why: &str| {
            if !on {
                if !matches!(
                    self.config
                        .input_socket
                        .with_extension("stopped")
                        .try_exists(),
                    Ok(false)
                ) {
                    return Err(Error::new(
                        "stopped",
                        format!("{why} Recover input before agents can resume."),
                    ));
                }
                self.controller.note_stop(false)?;
            }
            Ok(json!({
                "ok": true,
                "held": on,
                "kept": true,
                "warnings": [format!("{why} Agents are {} here all the same.", if on { "stopped" } else { "free to go on" })],
            }))
        };
        let Ok(hello) = self.input_transport().call(&json!({"op": "hello"})).await else {
            return unreachable("The helper's input service isn't running.");
        };
        let capable = hello["capabilities"]
            .as_array()
            .is_some_and(|capabilities| capabilities.iter().any(|c| c == "hold"));
        if !capable {
            return unreachable("The helper's input service is from an older build.");
        }

        let mut reply = self.native_request(json!({"op": "hold", "on": on})).await?;
        if !on {
            self.controller.note_stop(false)?;
        }
        if reply["kept"] == false {
            reply["warnings"] = json!([
                "The helper could not save this, so it lasts only until its input restarts"
            ]);
        }

        Ok(reply)
    }

    /// Once the person stops agents: either side's saved marker appears.
    async fn stopped_while_running(&self) {
        let marker = self.config.input_socket.with_extension("stopped");
        loop {
            tokio::time::sleep(Duration::from_millis(300)).await;
            if marker.exists() || self.controller.stop_noted() {
                return;
            }
        }
    }

    /// Refuse an agent's command that would change the helper while the
    /// person has stopped agents. Input is also refused by the input service
    /// itself; commands run through the helper daemon, which knows nothing
    /// of a stop. The stop's saved marker counts too, so a stopped helper
    /// whose input service is down still refuses.
    pub async fn require_going(&self) -> Result<()> {
        if self.actor == Actor::Viewer {
            return Ok(());
        }

        let marked = self.config.input_socket.with_extension("stopped").exists()
            || self.controller.stop_noted();
        let hello = self.input_transport().call(&json!({"op": "hello"})).await;
        if marked || hello.is_ok_and(|hello| hello["held"] == true) {
            return Err(Error::new("stopped", crate::error::STOPPED));
        }

        Ok(())
    }

    async fn native_request(&self, request: Value) -> Result<Value> {
        let result = self.input_transport().call(&request).await?;
        if result["ok"] != true {
            let error = Error::new(
                match result["code"].as_str() {
                    Some("busy") => "busy",
                    Some("lease-lost") => "lease-lost",
                    Some("cancelled") => "cancelled",
                    Some("stopped") => "stopped",
                    Some("identity") => "identity",
                    Some("session-changed") => "session-changed",
                    Some("permission") => "permission",
                    _ => "native",
                },
                result["error"]
                    .as_str()
                    .unwrap_or("Native control request failed"),
            );

            // The service reports when a failure may already have changed state.
            return Err(if result["uncertain"] == true {
                error.uncertain()
            } else {
                error
            });
        }

        Ok(result)
    }

    pub async fn input(
        &self,
        actions: &Value,
        expected: Option<(&Epoch, Instant)>,
        lease: Option<(&SharedLease, &str)>,
    ) -> Result<Value> {
        let _ownership = if lease.is_none() {
            Some(self.controller.acquire(false)?)
        } else {
            None
        };

        self.input_inner(actions, expected, lease).await
    }

    /// Send one validated batch under controller ownership the caller already
    /// holds, refusing unless the helper still has exactly `epoch`.
    pub async fn input_owned(
        &self,
        _ownership: &Ownership,
        actions: &Value,
        epoch: &Epoch,
    ) -> Result<Value> {
        self.input_inner(actions, Some((epoch, Instant::now())), None)
            .await
    }

    /// The input service, still running in this same session, was pinned
    /// to an earlier display (switching accounts can renumber it): pin it to
    /// the current verified display, as start-input does. When the display
    /// changed size it is re-pinned too, but the action is refused so the
    /// caller looks at the screen again before acting on it.
    async fn repin(&self, session: Session) -> Result<Session> {
        let unavailable = || {
            Error::new(
                "permission",
                "The native input service is unavailable; run atlas start-input",
            )
        };
        let status = self.status().await;
        if !crate::session::pinned_to_session(&status, &session) {
            return Err(unavailable());
        }

        let epoch = &session.epoch;
        let pinned = &status["inputService"]["enforcedEpoch"];
        let same_size = pinned["width"].as_f64() == Some(epoch.display.width)
            && pinned["height"].as_f64() == Some(epoch.display.height);

        self.pending_until_known(
            self.native_request(json!({"op": "reset", "epoch": epoch.input_epoch()})),
        )
        .await?;
        let current = self.identity().await?;
        if !current.input {
            return Err(unavailable());
        }

        if !same_size {
            return Err(Error::new(
                "session-changed",
                "The helper's display changed size. Take a new screenshot, then try again.",
            ));
        }

        Ok(current)
    }

    async fn input_inner(
        &self,
        actions: &Value,
        expected: Option<(&Epoch, Instant)>,
        lease: Option<(&SharedLease, &str)>,
    ) -> Result<Value> {
        self.controller.require_known()?;

        // A viewer's fresh observation is already tied to its acquired lease.
        // The native process validates that exact epoch again before every event.
        let session = match (expected, lease) {
            (Some((epoch, _)), Some(_)) => Session {
                epoch: epoch.clone(),
                capture: true,
                input: true,
            },
            _ => self.identity().await?,
        };

        if expected.is_some_and(|(epoch, _)| *epoch != session.epoch) {
            return Err(Error::new(
                "session-changed",
                "The helper changed since this frame",
            ));
        }

        let session = if session.input || lease.is_some() {
            session
        } else {
            self.repin(session).await?
        };

        actions::validate(actions, &session.epoch.display)?;

        self.input_in_session(actions, expected, lease, &session)
            .await
    }

    /// Carry out validated input in the verified session, retaining the
    /// standalone action's claim until native cleanup is acknowledged.
    async fn input_in_session(
        &self,
        actions: &Value,
        expected: Option<(&Epoch, Instant)>,
        lease: Option<(&SharedLease, &str)>,
        session: &Session,
    ) -> Result<Value> {
        let standalone = crate::control::token();
        let owner = lease.map(|(_, token)| token).unwrap_or(&standalone);
        if lease.is_none()
            && let Err(error) = self.acquire_input(owner, &session.epoch).await
        {
            if error.uncertain {
                // The service may now hold input for `owner`: release it, or
                // leave the outcome pending for recovery. The unacknowledged
                // acquire stays pending even if this cleanup succeeds.
                let _ = self
                    .pending_until_known(self.native_control("release", owner))
                    .await;
            }

            return Err(error);
        }

        let generation = lease
            .is_none()
            .then(|| self.controller.quarantine())
            .transpose()?;
        let result = self
            .mutate(
                json!({
                    "op": "input",
                    "actions": actions,
                    "owner": owner,
                    "epoch": session.epoch.input_epoch(),
                    // An agent's command names the app it acted in.
                    "app": self.doing.is_some(),
                }),
                lease,
                expected.map(|(_, at)| at),
            )
            .await;

        if lease.is_none() {
            let cleanup = self.native_control("release", owner).await;
            let unknown = result
                .as_ref()
                .map_or_else(|error| error.uncertain, |reply| reply["uncertain"] == true)
                || cleanup.as_ref().is_err_and(|error| error.uncertain);
            if cleanup.is_ok()
                && !unknown
                && let Some(generation) = &generation
            {
                self.controller.clear_if(generation)?;
            }
            if cleanup.is_err() {
                let error = match result {
                    Ok(_) => Error::new(
                        "uncertain",
                        "Input finished but native release was not confirmed",
                    ),
                    Err(error) => Error::new(
                        "uncertain",
                        format!("{} Native release was not confirmed either.", error.error),
                    ),
                };
                return Err(error.uncertain());
            }
        }

        result
    }

    pub async fn run(&self, argv: Vec<String>) -> Result<Value> {
        self.run_within(argv, RUN_TIMEOUT).await
    }

    /// [`Client::run`], stopping the command after `timeout` (at most ten
    /// minutes). Control stays held until it ends.
    pub async fn run_within(&self, argv: Vec<String>, timeout: Duration) -> Result<Value> {
        let ownership = self.controller.acquire(false)?;
        self.run_owned_within(&ownership, argv, timeout).await
    }

    /// Execute argv in the verified hidden helper as a recorded mutation under
    /// controller ownership the caller already holds.
    pub async fn run_owned(&self, ownership: &Ownership, argv: Vec<String>) -> Result<Value> {
        self.run_owned_within(ownership, argv, RUN_TIMEOUT).await
    }

    /// [`Client::run_owned`], stopping the command after `timeout`.
    pub async fn run_owned_within(
        &self,
        _ownership: &Ownership,
        argv: Vec<String>,
        timeout: Duration,
    ) -> Result<Value> {
        if argv.is_empty() || argv[0].is_empty() || argv.iter().any(|a| a.contains('\0')) {
            return Err(Error::new(
                "input",
                "Provide a command and arguments after run --",
            ));
        }

        if timeout.is_zero() || timeout > RUN_TIMEOUT_MAX {
            return Err(Error::new(
                "usage",
                "A command may run for 1 second to 10 minutes",
            ));
        }

        let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
        self.identity().await?;

        self.mutate(
            json!({"op": "run", "argv": argv, "timeoutMs": timeout_ms}),
            None,
            None,
        )
        .await
    }

    /// Run a fixed, read-only inspection command (such as the window list) in
    /// the verified hidden helper. It records no pending mutation, so callers
    /// must only pass commands that cannot change the helper.
    pub(crate) async fn inspect(&self, argv: Vec<String>) -> Result<Value> {
        self.inspect_within(argv, Duration::from_secs(5)).await
    }

    /// [`Client::inspect`] bounded by `limit`. A read changes nothing, so
    /// its failure is never uncertain.
    pub(crate) async fn inspect_within(&self, argv: Vec<String>, limit: Duration) -> Result<Value> {
        self.run_pending(argv, limit).await.map_err(|error| Error {
            uncertain: false,
            ..error
        })
    }

    /// [`Client::inspect_within`] with a run's usual limit, under controller
    /// ownership the caller already holds. Only for commands that cannot
    /// change the helper: nothing is left pending when it fails.
    pub(crate) async fn inspect_owned(
        &self,
        _ownership: &Ownership,
        argv: Vec<String>,
    ) -> Result<Value> {
        self.inspect_within(argv, RUN_TIMEOUT).await
    }

    /// Execute argv in the verified hidden helper as part of a mutation the
    /// caller holds ownership for and has already recorded as pending. The
    /// caller settles that record once the outcome is established. A
    /// refusal is an error with the helper's code, uncertain when the
    /// command may have started.
    pub async fn run_pending(&self, argv: Vec<String>, timeout: Duration) -> Result<Value> {
        self.identity().await?;

        let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
        let request = json!({"op": "run", "argv": argv, "timeoutMs": timeout_ms});

        // The reply comes after the command ends, as for any other run.
        let mut transport = self.transport.clone();
        transport.timeout = transport.timeout.max(timeout + RUN_MARGIN);

        let reply = transport.call(&request).await?;
        validate_completion(&request, &reply)?;

        if reply["ok"] != true {
            return Err(crate::transport::refusal(&reply));
        }

        Ok(reply)
    }

    pub async fn apps(&self) -> Result<Value> {
        self.identity().await?;
        self.transport.call(&json!({"op": "apps"})).await
    }

    pub async fn start_input(&self) -> Result<Value> {
        let _ownership = self.controller.acquire(false)?;
        self.identity().await?;
        self.launch_input().await
    }

    /// SHA-256 of the installed shared executable, rehashed only when its
    /// device, inode, size or modification time changes.
    pub fn installed_build(&self) -> Option<String> {
        let path = &self.config.identity_probe;
        // The executable itself (through a link to the app), not the link.
        let metadata = fs::metadata(path).ok()?;
        let key = (
            metadata.dev(),
            metadata.ino(),
            metadata.size(),
            metadata.mtime(),
            metadata.mtime_nsec(),
        );
        if let Some((cached, build)) = self.installed.lock().ok()?.as_ref()
            && *cached == key
        {
            return build.clone();
        }

        // Hashed without the lock, so other callers are not held up. A
        // failed read is not cached: permissions may be repaired without
        // changing the identity key.
        let build = crate::digest::probe_executable(path).and_then(|path| file_sha256(&path))?;

        if let Ok(mut cache) = self.installed.lock() {
            *cache = Some((key, Some(build.clone())));
        }

        Some(build)
    }

    /// start-input: replace a stale service, otherwise make one ready.
    async fn launch_input(&self) -> Result<Value> {
        let status = self.status().await;
        require_hidden(&status, self.config.helper_uid)?;

        let native = &status["inputService"];
        let current = crate::session::input_build_current(&status);
        if native["ok"] == true && !current {
            return self.upgrade_input(&status).await;
        }

        let (pid, outcome) = self.ensure_input(&status).await?;
        Ok(json!({ "ok": true, outcome: true, "pid": pid }))
    }

    /// Make a ready input service exist without replacing a running one:
    /// reuse it, reset it, or start the installed build.
    async fn ensure_input(&self, status: &Value) -> Result<(u32, &'static str)> {
        let session = require_hidden(status, self.config.helper_uid)?;
        if session.input {
            return Ok((session.epoch.input_pid, "alreadyRunning"));
        }

        if status["inputService"]["ok"] == true {
            let reset = self.native_request(json!({
                "op": "reset",
                "epoch": session.epoch.input_epoch(),
            }));
            self.pending_until_known(reset).await?;

            if !self.identity().await?.input {
                return Err(Error::new(
                    "startup",
                    "Native input did not become ready after reset",
                ));
            }

            return Ok((session.epoch.input_pid, "recovered"));
        }

        // Without Accessibility a new service can never become ready, and a
        // start that cannot be confirmed would stay pending; refuse plainly.
        require_accessibility(status)?;
        let pid = self.pending_until_known(self.spawn_input()).await?;

        Ok((pid, "started"))
    }

    /// Carry out a change to the input service as a pending mutation: it
    /// stays pending when its outcome is unknown, and clears once it is
    /// known, done or refused.
    async fn pending_until_known<T>(&self, change: impl Future<Output = Result<T>>) -> Result<T> {
        let generation = self.controller.quarantine()?;
        let outcome = change.await;
        if outcome
            .as_ref()
            .map_or_else(|error| !error.uncertain, |_| true)
        {
            self.controller.clear_if(&generation)?;
        }

        outcome
    }

    /// Replace a running service whose build differs from the installed
    /// executable through its authenticated shutdown. The replacement stays
    /// a pending mutation until it is ready, current and holds nothing.
    async fn upgrade_input(&self, status: &Value) -> Result<Value> {
        let native = &status["inputService"];

        if !status["installedBuild"].is_string() {
            return Err(Error::new(
                "startup",
                "The installed executable cannot be read; refusing to replace the running service",
            ));
        }

        let stoppable = native["capabilities"]
            .as_array()
            .is_some_and(|capabilities| capabilities.iter().any(|c| c == "shutdown"));
        if !stoppable {
            return Err(Error::new(
                "startup",
                "The running input service predates authenticated shutdown and cannot be \
                 replaced automatically. Stop that process in the helper account (or log the \
                 helper out and in), then run atlas start-input.",
            ));
        }

        if native["controlActive"] != false {
            return Err(Error::new(
                "busy",
                "The input service has an active controller; release it before upgrading",
            ));
        }

        let previous = native["pid"]
            .as_u64()
            .and_then(|pid| u32::try_from(pid).ok())
            .filter(|pid| *pid > 1)
            .ok_or_else(|| Error::new("startup", "The running input service has no valid PID"))?;
        let generation = self.controller.quarantine()?;

        if let Err(error) = self.stop_input(previous).await {
            if !error.uncertain {
                self.controller.clear_if(&generation)?;
            }
            return Err(error);
        }

        let pid = self.spawn_input().await?;
        let state = self.status().await;
        if !input_current(&state, self.config.helper_uid, pid) {
            return Err(Error::new(
                "uncertain",
                "The replacement input service is not confirmed current with nothing held",
            )
            .uncertain());
        }

        self.controller.clear_if(&generation)?;

        Ok(json!({
            "ok": true,
            "upgraded": true,
            "previousPid": previous,
            "pid": pid,
            "build": state["inputService"]["build"],
        }))
    }

    /// Ask exactly `pid` to shut down over its authenticated socket, then wait
    /// until its endpoint is closed and the process is gone. Anything short of
    /// that is uncertain; a definitive refusal changed nothing.
    pub async fn stop_input(&self, pid: u32) -> Result<()> {
        self.native_request(json!({ "op": "shutdown", "pid": pid }))
            .await
            .map_err(|error| {
                if error.uncertain {
                    error
                } else {
                    Error::new(
                        error.code,
                        format!("The running input service refused to stop: {}", error.error),
                    )
                }
            })?;

        let deadline = Instant::now() + Duration::from_secs(5);

        loop {
            let closed = matches!(
                tokio::net::UnixStream::connect(&self.config.input_socket).await,
                Err(error) if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                )
            );
            if closed && !process_exists(pid) {
                return Ok(());
            }

            if Instant::now() >= deadline {
                return Err(Error::new(
                    "uncertain",
                    "The previous input service did not confirm exit",
                )
                .uncertain());
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Start the installed executable as the helper's input service. The caller
    /// records the launch as a pending mutation and settles it on success.
    pub async fn spawn_input(&self) -> Result<u32> {
        // Arguments stay separate from shell syntax, including configured paths.
        let script = concat!(
            "nohup \"$1\" input-service --uid \"$2\" --controller-uid \"$3\" --socket \"$4\" ",
            ">\"$HOME/Library/Logs/Atlas-input.log\" 2>&1 </dev/null & echo $!",
        );
        let controller_uid = nix::unistd::getuid().as_raw();
        let request = json!({
            "op": "run",
            "argv": [
                "/bin/sh", "-c", script, "sh",
                self.config.identity_probe,
                self.config.helper_uid.to_string(),
                controller_uid.to_string(),
                self.config.input_socket,
            ],
            "timeoutMs": 3000,
        });
        let reply = self.transport.call(&request).await?;

        // The launch prints the child PID; only that process may satisfy readiness.
        let child = (reply["ok"] == true && reply["exitCode"] == 0)
            .then(|| run_output(&reply, 32).ok())
            .flatten()
            .and_then(|output| String::from_utf8(output).ok())
            .and_then(|text| text.trim().parse::<u32>().ok())
            .filter(|pid| *pid > 1)
            .ok_or_else(|| {
                Error::new(
                    "startup",
                    "The helper could not launch the Rust input service",
                )
                .uncertain()
            })?;

        let deadline = Instant::now() + Duration::from_secs(6);

        loop {
            let state = self.status().await;
            if require_hidden(&state, self.config.helper_uid)
                .is_ok_and(|session| session.input && session.epoch.input_pid == child)
            {
                return Ok(child);
            }

            if Instant::now() >= deadline {
                return Err(Error::new(
                    "startup",
                    "Input service did not become ready. Check ~/Library/Logs/Atlas-input.log \
                     in the helper account and confirm that identity_probe points to the current Rust binary.",
                )
                .uncertain());
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub async fn restart(&self) -> Result<Value> {
        let _ownership = self.controller.acquire(true)?;
        let before = self.identity().await?;

        // Recovery goes through the input service; with none running and no
        // Accessibility to start one, it cannot finish, so nothing is marked.
        let current = self.status().await;
        if current["inputService"]["ok"] != true {
            require_accessibility(&current)?;
        }

        // Held through verification, so the restart reads as in progress.
        // A refusal that changed nothing clears it, as for a log-out.
        let claim = self.controller.quarantine()?;
        let outcome = self.restart_claimed(&before).await;

        self.settle_restart(&claim, outcome)
    }

    /// Settle this restart after verified recovery or a definite refusal.
    /// Other live mutations retain their own claims.
    fn settle_restart(&self, claim: &str, outcome: Result<Value>) -> Result<Value> {
        let refused = match &outcome {
            Ok(reply) => reply["ok"] != true && reply["uncertain"] != true,
            Err(error) => !error.uncertain,
        };
        if refused || outcome.as_ref().is_ok_and(|reply| reply["ok"] == true) {
            self.controller.clear_if(claim).map_err(Error::uncertain)?;
        }

        outcome
    }

    /// The restart itself, under the claim [`Client::restart`] holds: input
    /// cleared first, then the helper restarted and confirmed new.
    async fn restart_claimed(&self, before: &Session) -> Result<Value> {
        // Input has its own process. Clear held state there before restarting
        // the capture/command helper; a new capture PID alone proves nothing
        // about an uncertain mouse or keyboard action.
        let initial = self.status().await;
        // A service whose file was replaced (an app update) cannot become
        // ready under the restarted helper; it is replaced there instead.
        let stale = !crate::session::input_build_current(&initial);
        let (input_pid, _) = self.ensure_input(&initial).await?;
        let session = self.identity().await?;
        if session.epoch.input_pid != input_pid {
            return Err(
                Error::new("uncertain", "The input service changed while recovering").uncertain(),
            );
        }

        let cleanup = self
            .native_request(json!({ "op": "reset", "epoch": session.epoch.input_epoch() }))
            .await?;
        if cleanup["released"] != true {
            return Err(
                Error::new("uncertain", "Native input cleanup was not confirmed").uncertain(),
            );
        }

        let reply = self.mutate(json!({"op": "restart"}), None, None).await?;
        if reply["ok"] != true {
            return Ok(reply);
        }

        // The helper was told to restart: from here on, every failure leaves
        // the outcome unknown, so the claim stays until recovery.
        let recovery = async {
            loop {
                tokio::time::sleep(Duration::from_millis(150)).await;
                let status = self.status().await;

                // Settle only with a new capture process and the same input
                // service whose cleanup was acknowledged, idle and empty.
                let Ok(session) = require_hidden(&status, self.config.helper_uid) else {
                    continue;
                };

                if session.epoch.capture_pid == before.epoch.capture_pid {
                    continue;
                }

                if input_settled(&status, self.config.helper_uid, input_pid) {
                    self.controller.clear().map_err(Error::uncertain)?;
                    return Ok(json!({"ok": true, "restarted": true, "session": status}));
                }

                // Its cleanup was confirmed above, so nothing is held: a stale
                // service that is not ready under the new helper is replaced.
                if stale
                    && status["inputService"]["pid"] == input_pid
                    && status["inputService"]["ready"] == false
                {
                    self.upgrade_input(&status)
                        .await
                        .map_err(Error::uncertain)?;
                    self.controller.clear().map_err(Error::uncertain)?;
                    let status = self.status().await;

                    return Ok(json!({
                        "ok": true,
                        "restarted": true,
                        "inputReplaced": true,
                        "session": status,
                    }));
                }
            }
        };

        tokio::time::timeout(Duration::from_secs(20), recovery)
            .await
            .map_err(|_| {
                Error::new(
                    "restart",
                    "Restart acknowledged, but no new hidden helper was confirmed",
                )
                .uncertain()
            })?
    }

    /// Log the helper out (`atlas logout`) while it is in the background
    /// (`identity` refuses it on screen). Its apps quit as at any log-out.
    /// Like a restart, it holds its own claim until the outcome is known:
    /// cleared once the desktop is gone or macOS refused, kept pending when
    /// it may still happen (an app holding it up, an unconfirmed request).
    /// `force` ends the session without asking the apps, and may run with
    /// outcomes still unknown: it is the person's way out of one.
    pub async fn logout(&self, force: bool) -> Result<Value> {
        let ownership = self.controller.acquire(force)?;
        self.logout_held(&ownership, force).await
    }

    /// [`Client::logout`] under control already held (`_ownership`), for a
    /// caller that keeps it across more than the log-out, such as uninstall.
    pub async fn logout_held(&self, _ownership: &Ownership, force: bool) -> Result<Value> {
        self.identity().await?;
        let claim = self.controller.quarantine()?;

        match self
            .mutate(json!({"op": "logout", "force": force}), None, None)
            .await
        {
            Ok(reply) if reply["ok"] == true => {}
            // Refused before anything happened: nothing is pending.
            Ok(reply) if reply["uncertain"] != true => {
                self.controller.clear_if(&claim)?;
                return Ok(reply);
            }
            Ok(reply) => return Ok(reply),
            Err(error) => {
                if !error.uncertain {
                    self.controller.clear_if(&claim)?;
                }
                return Err(error);
            }
        }

        if self.desktop_gone(if force { 15 } else { 30 }).await {
            self.controller.clear_if(&claim)?;
            return Ok(json!({"ok": true, "loggedOut": true}));
        }

        if force {
            return Err(Error::new(
                "logout",
                "macOS took the request but the helper is still logged in",
            )
            .uncertain());
        }

        let open = self
            .apps()
            .await
            .map(|reply| crate::agent::left_open(&reply))
            .unwrap_or_default();
        let still = if open.is_empty() {
            String::new()
        } else {
            format!(" Still open: {}.", open.join(", "))
        };

        Err(Error::new(
            "logout-held",
            format!("The helper has not logged out yet; an app may be asking to save.{still}"),
        )
        .uncertain())
    }

    /// Whether the helper's login goes away within `seconds`: no
    /// loginwindow and no Dock of it, seen twice in a row half a second
    /// apart (a Dock alone can restart in a login that lives on), and its
    /// daemon no longer answering. A look that fails counts as present.
    async fn desktop_gone(&self, seconds: u64) -> bool {
        let uid = self.config.helper_uid;
        let looked = move || async move {
            tokio::task::spawn_blocking(move || crate::session::logged_out(uid))
                .await
                .ok()
                .flatten()
                == Some(true)
        };
        let answers = || async { self.transport.call(&json!({"op": "hello"})).await.is_ok() };

        gone_within(
            looked,
            answers,
            Duration::from_secs(seconds),
            Duration::from_millis(500),
        )
        .await
    }

    async fn mutate(
        &self,
        request: Value,
        lease: Option<(&SharedLease, &str)>,
        observed_at: Option<Instant>,
    ) -> Result<Value> {
        let started = Instant::now();

        // An agent's command that changes the helper is refused, before it
        // starts, once the person has stopped agents; reads and Atlas's own
        // steps (no `doing`) go on.
        let cuttable = request["op"] == "run" && self.actor == Actor::Agent && self.doing.is_some();
        if cuttable
            && (self.config.input_socket.with_extension("stopped").exists()
                || self.controller.stop_noted())
        {
            return Err(Error::new("stopped", crate::error::STOPPED));
        }

        // Connect without holding the lease. Release can cancel slow preflight.
        let mut transport = if request["op"] == "input" {
            self.input_transport()
        } else {
            self.transport.clone()
        };

        // A long command's reply comes after it ends, and after the helper
        // has stopped whatever it left running.
        if let Some(run) = request["timeoutMs"]
            .as_u64()
            .filter(|_| request["op"] == "run")
        {
            transport.timeout = transport
                .timeout
                .max(Duration::from_millis(run) + RUN_MARGIN);
        }

        let connection = transport.connect().await?;

        // Serialize only final authorization and bounded byte submission.
        let mut authorization = match lease {
            Some((state, _)) => Some(state.lock().await),
            None => None,
        };

        if let (Some(state), Some((_, owner))) = (&mut authorization, lease) {
            state.require(owner)?;
        }

        if observed_at.is_some_and(|at| at.elapsed() >= Duration::from_secs(8)) {
            return Err(Error::new(
                "stale-frame",
                "The observation expired before dispatch",
            ));
        }

        // Restart, log-out and standalone input run under the claim their
        // caller holds through verification or cleanup.
        let generation = (request["op"] != "restart"
            && request["op"] != "logout"
            && !(request["op"] == "input" && lease.is_none()))
        .then(|| self.controller.quarantine())
        .transpose()?;

        let pending = match connection.dispatch(&request).await {
            Ok(pending) => pending,
            Err(error) => {
                if let Some(generation) = &generation
                    && !error.uncertain
                {
                    self.controller.clear_if(generation)?;
                }
                return Err(error);
            }
        };

        drop(authorization);

        // An agent's command runs until it ends, unless the person stops
        // agents: then the connection is dropped, which ends the command in
        // the helper. What it did by then is unknown, so it stays pending.
        // A command that started ends with a row too, however it ends.
        let finishing = pending.finish();
        let mut reply = if cuttable {
            let _ = self.record_start(&request);
            let ended = tokio::select! {
                reply = finishing => reply,
                () = self.stopped_while_running() => {
                    Err(Error::new("stopped", crate::error::STOPPED).uncertain())
                }
            };
            match ended {
                Ok(reply) => reply,
                Err(error) => {
                    let failed = json!({"ok": false, "code": recorded_code(&error)});
                    let _ = self.record(&request, &failed, started.elapsed());
                    return Err(error);
                }
            }
        } else {
            finishing.await?
        };
        validate_completion(&request, &reply)?;

        let mut warnings = Vec::new();
        if let Some(generation) = &generation
            && reply["uncertain"] != true
            && let Err(error) = self.controller.clear_if(generation)
        {
            warnings.push(format!(
                "Operation completed, but its pending marker could not be cleared: {error}"
            ));
        }

        if self.record(&request, &reply, started.elapsed()).is_err() {
            warnings
                .push("Operation result is valid; local metadata could not be recorded".to_owned());
        }

        if !warnings.is_empty() {
            reply["warnings"] = json!(warnings);
        }

        Ok(reply)
    }

    /// Whether this shell may keep Atlas's own files: the controller lock
    /// and the screenshots. An agent's sandbox may forbid both while the
    /// helper itself is fine.
    pub fn writable(&self) -> Result<()> {
        self.controller.writable()?;
        crate::control::probe_write(self.output_dir()?)?;

        // Builds and environment plans are staged beside the shared Atlas
        // command for the helper to read; a missing folder is setup's
        // concern, not this check's.
        if let Some(shared) = self.config.identity_probe.parent()
            && shared.is_dir()
        {
            crate::control::probe_write(shared)?;
        }

        Ok(())
    }

    /// Record that a command began, so what it does shows while it runs;
    /// the row it ends with stands for it after.
    fn record_start(&self, request: &Value) -> Result<()> {
        self.write_row(request, &json!({"ok": null}), Duration::ZERO, true)
    }

    fn record(&self, request: &Value, reply: &Value, elapsed: Duration) -> Result<()> {
        self.write_row(request, reply, elapsed, false)
    }

    fn write_row(
        &self,
        request: &Value,
        reply: &Value,
        elapsed: Duration,
        start: bool,
    ) -> Result<()> {
        let directory = self.output_dir()?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(directory.join("metadata.lock"))?;
        FileExt::lock_exclusive(&lock)?;

        let path = directory.join("metadata.jsonl");
        if fs::metadata(&path).is_ok_and(|m| m.len() > 1024 * 1024) {
            fs::rename(&path, directory.join("metadata.previous.jsonl"))?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?;

        // No input text, argv, coordinates, window titles, output or image
        // payloads in metadata: only what `Doing` names.
        let ok = worked(reply);
        let mut row = json!({
            "at": timestamp().to_string(),
            "operation": request["op"],
            "ok": if start { Value::Null } else { json!(ok) },
            "elapsedMs": elapsed.as_secs_f64() * 1000.0,
            "actionCount": request["actions"].as_array().map(Vec::len),
            "actor": self.actor.as_str(),
        });

        // A failure keeps its fixed code (never its message), so the app can
        // say why in plain words.
        if !start
            && !ok
            && let (Some(code), Some(fields)) = (failure_code(reply), row.as_object_mut())
        {
            fields.insert("code".into(), json!(code));
        }

        if let (Some(doing), Some(fields)) = (&self.doing, row.as_object_mut()) {
            doing.fields(fields, ok);
            if start {
                fields.insert("started".into(), json!(true));
            }
            // Input names the app it landed in, as the helper saw it.
            if !fields.contains_key("app")
                && let Some(app) = reply["app"].as_str()
            {
                fields.insert("app".into(), json!(crate::doing::bounded(app)));
            }
        }

        writeln!(file, "{row}")?;
        Ok(())
    }
}

/// The code a failure is recorded with: `uncertain` when it may have
/// finished in the helper all the same, so the activity never says it
/// didn't happen.
fn recorded_code(error: &Error) -> &'static str {
    if error.uncertain {
        "uncertain"
    } else {
        error.code
    }
}

/// Why a mutation failed, as a fixed code: the helper's error code, or
/// `timeout` and `exit` for a command that ran too long or failed. Only a
/// short lowercase code is kept, so no message text reaches the log.
fn failure_code(reply: &Value) -> Option<String> {
    if let Some(code) = reply["code"].as_str() {
        let fixed = !code.is_empty()
            && code.len() <= 32
            && code
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'-');
        return fixed.then(|| code.to_owned());
    }
    if reply["timedOut"] == true {
        return Some("timeout".into());
    }
    reply["exitCode"]
        .as_i64()
        .is_some_and(|code| code != 0)
        .then(|| "exit".into())
}

/// Whether a mutation did what it was for: the helper accepted it and, for
/// a command, the command ran to its end and succeeded.
fn worked(reply: &Value) -> bool {
    reply["ok"] == true
        && reply["exitCode"].as_i64().is_none_or(|code| code == 0)
        && reply["timedOut"] != true
}

pub fn validate_completion(request: &Value, reply: &Value) -> Result<()> {
    if reply["ok"] != true {
        return Ok(());
    }

    let valid = match request["op"].as_str() {
        Some("input") => {
            reply["performed"].as_u64() == request["actions"].as_array().map(|v| v.len() as u64)
        }
        Some("run") => {
            reply["timedOut"].is_boolean()
                && reply.get("exitCode").is_some()
                && (reply["exitCode"].is_i64()
                    || (reply["exitCode"].is_null()
                        && (reply["timedOut"] == true
                            || reply["signal"]
                                .as_str()
                                .is_some_and(|s| s.starts_with("SIG") && s.len() <= 32))))
        }
        _ => true,
    };

    if valid {
        Ok(())
    } else {
        Err(Error::new("protocol", "Helper did not confirm operation completion").uncertain())
    }
}

pub fn png_dimensions(png: &[u8]) -> Result<(u32, u32)> {
    let invalid = || Error::new("capture", "Invalid PNG header or dimensions");

    if png.len() < 33 || &png[..16] != b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR" {
        return Err(invalid());
    }

    let width = u32::from_be_bytes(png[16..20].try_into().map_err(|_| invalid())?);
    let height = u32::from_be_bytes(png[20..24].try_into().map_err(|_| invalid())?);
    if width == 0 || height == 0 || width > 32768 || height > 32768 {
        return Err(invalid());
    }

    Ok((width, height))
}

/// Whether a process exists, whoever owns it (EPERM still means it exists).
fn process_exists(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };

    matches!(
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None),
        Ok(()) | Err(nix::errno::Errno::EPERM)
    )
}

/// Refuse input work while the helper's Atlas lacks Accessibility; an
/// unknown answer is not a refusal.
fn require_accessibility(status: &Value) -> Result<()> {
    if status["permissions"]["accessibility"] == false {
        return Err(Error::new(
            "permission",
            "The helper does not allow Atlas Accessibility yet, which input needs. In the \
             helper's System Settings › Privacy & Security › Accessibility, turn Atlas on \
             (atlas init guides this).",
        ));
    }

    Ok(())
}

/// Whether a login is seen gone within `limit`: `looked` (no loginwindow
/// and no Dock) true twice in a row, `pause` apart, and then `answers` (its
/// daemon says hello) false. The processes are looked at all the while: a
/// login seen again, even while its daemon still answers, starts the count
/// over. Only silence after two looks that found it gone, and a third look
/// after that silence (the daemon's answer can take a while, and the login
/// can come back meanwhile), ends the wait.
async fn gone_within<L, A>(
    mut looked: impl FnMut() -> L,
    mut answers: impl FnMut() -> A,
    limit: Duration,
    pause: Duration,
) -> bool
where
    L: std::future::Future<Output = bool>,
    A: std::future::Future<Output = bool>,
{
    let gone = async {
        let mut absent = 0;
        loop {
            tokio::time::sleep(pause).await;
            if !looked().await {
                absent = 0;
                continue;
            }

            absent += 1;
            if absent >= 2 && !answers().await {
                if looked().await {
                    return;
                }
                absent = 0;
            }
        }
    };

    tokio::time::timeout(limit, gone).await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::{UnixListener, UnixStream},
    };

    /// A native socket and controller confined to a short temporary path.
    fn native_client() -> (tempfile::TempDir, Client, UnixListener, Session) {
        let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let mut client = client_in(dir.path());
        client.controller = Controller::at(60001, &dir.path().join("state"));
        client.config.helper_uid = nix::unistd::getuid().as_raw();
        client.transport = client.input_transport();
        let listener = UnixListener::bind(&client.config.input_socket).unwrap();
        let session = Session {
            epoch: Epoch {
                uid: client.config.helper_uid,
                capture_pid: 123,
                display: Display {
                    width: 100.0,
                    height: 100.0,
                    scale: 1.0,
                },
                security_session_id: 42,
                display_id: 7,
                input_pid: 456,
            },
            capture: true,
            input: true,
        };
        (dir, client, listener, session)
    }

    /// Drive the caller until the stand-in has received a complete request,
    /// leaving its reply under the test's control.
    async fn dispatched<T: std::fmt::Debug>(
        caller: &mut (impl Future<Output = Result<T>> + Unpin),
        listener: &UnixListener,
        operation: &str,
    ) -> BufReader<UnixStream> {
        tokio::select! {
            outcome = caller => panic!("caller ended before {operation}: {outcome:?}"),
            peer = tokio::time::timeout(Duration::from_secs(2), async {
                let (peer, _) = listener.accept().await.unwrap();
                let mut peer = BufReader::new(peer);
                let mut request = String::new();
                peer.read_line(&mut request).await.unwrap();
                assert_eq!(serde_json::from_str::<Value>(&request).unwrap()["op"], operation);
                peer
            }) => peer.unwrap(),
        }
    }

    async fn answer(peer: &mut BufReader<UnixStream>, reply: Value) {
        peer.get_mut()
            .write_all(format!("{reply}\n").as_bytes())
            .await
            .unwrap();
    }

    /// Exercise the transaction after session verification, with no native
    /// events: the stand-in acknowledges a press and move without a release.
    async fn standalone_input(client: &Client, session: &Session) -> Result<Value> {
        let _ownership = client.controller.acquire(false)?;
        let actions = json!([
            {"type": "pointer", "phase": "down", "x": 1, "y": 1},
            {"type": "pointer", "phase": "move", "x": 2, "y": 2},
        ]);
        actions::validate(&actions, &session.epoch.display)?;
        client.input_in_session(&actions, None, None, session).await
    }

    fn recovery_settles_the_abandoned_claim(client: &Client) {
        assert_eq!(client.controller.pending().unwrap().len(), 1);
        assert_eq!(client.controller.unresolved().unwrap().len(), 1);
        assert_eq!(
            client.controller.acquire(false).unwrap_err().code,
            "uncertain"
        );
        let ownership = client.controller.acquire(true).unwrap();
        client.controller.clear().unwrap();
        drop(ownership);
        assert!(client.controller.pending().unwrap().is_empty());
        assert!(client.controller.acquire(false).is_ok());
    }

    #[tokio::test]
    async fn a_release_interrupted_after_dispatch_stays_pending_until_recovery() {
        let (_dir, client, listener, session) = native_client();
        let mut caller = Box::pin(standalone_input(&client, &session));
        let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
        answer(&mut acquire, json!({"ok": true})).await;
        let mut input = dispatched(&mut caller, &listener, "input").await;
        let generation = client.controller.pending().unwrap();
        assert_eq!(generation.len(), 1);
        answer(&mut input, json!({"ok": true, "performed": 2})).await;
        let _release = dispatched(&mut caller, &listener, "release").await;
        assert_eq!(client.controller.pending().unwrap(), generation);
        assert_eq!(client.controller.acquire(true).unwrap_err().code, "busy");
        drop(caller);
        recovery_settles_the_abandoned_claim(&client);
    }

    #[tokio::test]
    async fn a_successful_release_settles_the_standalone_input_claim() {
        let (_dir, client, listener, session) = native_client();
        let mut caller = Box::pin(standalone_input(&client, &session));
        let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
        answer(&mut acquire, json!({"ok": true})).await;
        let mut input = dispatched(&mut caller, &listener, "input").await;
        let generation = client.controller.pending().unwrap();
        assert_eq!(generation.len(), 1);
        answer(&mut input, json!({"ok": true, "performed": 2})).await;
        let mut release = dispatched(&mut caller, &listener, "release").await;
        assert_eq!(client.controller.pending().unwrap(), generation);
        assert_eq!(client.controller.acquire(true).unwrap_err().code, "busy");
        answer(&mut release, json!({"ok": true})).await;
        assert_eq!(caller.await.unwrap()["ok"], true);
        assert!(client.controller.pending().unwrap().is_empty());
        assert!(client.controller.acquire(false).is_ok());
    }

    #[tokio::test]
    async fn standalone_input_with_an_uncertain_outcome_stays_pending_until_recovery() {
        for (input_reply, release_reply) in [
            (json!({"ok": true, "performed": 2}), None),
            (
                json!({"ok": false, "uncertain": true, "error": "unknown"}),
                Some(json!({"ok": true})),
            ),
            (
                json!({"ok": true, "performed": 2}),
                Some(json!({"ok": false, "uncertain": true, "error": "unknown"})),
            ),
        ] {
            let (_dir, client, listener, session) = native_client();
            let mut caller = Box::pin(standalone_input(&client, &session));
            let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
            answer(&mut acquire, json!({"ok": true})).await;
            let mut input = dispatched(&mut caller, &listener, "input").await;
            answer(&mut input, input_reply).await;
            let mut release = dispatched(&mut caller, &listener, "release").await;
            assert_eq!(client.controller.pending().unwrap().len(), 1);
            if let Some(reply) = release_reply {
                answer(&mut release, reply).await;
            }
            drop(release);
            let outcome = caller.await;
            assert!(
                outcome
                    .as_ref()
                    .map_or_else(|error| error.uncertain, |reply| reply["uncertain"] == true)
            );
            recovery_settles_the_abandoned_claim(&client);
        }
    }

    #[tokio::test]
    async fn an_input_whose_native_release_is_definitely_refused_stays_pending_until_recovery() {
        let (_dir, client, listener, session) = native_client();
        let mut caller = Box::pin(standalone_input(&client, &session));
        let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
        answer(&mut acquire, json!({"ok": true})).await;
        let mut input = dispatched(&mut caller, &listener, "input").await;
        answer(&mut input, json!({"ok": true, "performed": 2})).await;
        let mut release = dispatched(&mut caller, &listener, "release").await;
        let generation = client.controller.pending().unwrap();
        assert_eq!(generation.len(), 1);
        answer(
            &mut release,
            json!({"ok": false, "code": "lease-lost", "error": "refused"}),
        )
        .await;
        let error = caller.await.unwrap_err();
        assert_eq!(error.code, "uncertain");
        assert!(error.uncertain);
        assert_eq!(client.controller.pending().unwrap(), generation);
        recovery_settles_the_abandoned_claim(&client);
    }

    #[tokio::test]
    async fn releasing_after_a_lost_acquisition_reply_keeps_the_acquisition_pending() {
        let (_dir, client, listener, session) = native_client();
        let mut caller = Box::pin(standalone_input(&client, &session));
        let acquire = dispatched(&mut caller, &listener, "acquire").await;
        drop(acquire);
        let mut release = dispatched(&mut caller, &listener, "release").await;
        assert_eq!(client.controller.pending().unwrap().len(), 2);
        assert_eq!(client.controller.unresolved().unwrap().len(), 1);
        assert_eq!(client.controller.acquire(true).unwrap_err().code, "busy");
        answer(&mut release, json!({"ok": true})).await;
        assert!(caller.await.unwrap_err().uncertain);
        recovery_settles_the_abandoned_claim(&client);
    }

    #[tokio::test]
    async fn an_acquisition_interrupted_after_dispatch_stays_pending_until_recovery() {
        let (_dir, client, listener, session) = native_client();
        let mut caller = Box::pin(client.acquire_input("owner", &session.epoch));
        let _acquire = dispatched(&mut caller, &listener, "acquire").await;
        assert_eq!(client.controller.pending().unwrap().len(), 1);
        drop(caller);
        recovery_settles_the_abandoned_claim(&client);
    }

    #[tokio::test]
    async fn an_acquisition_settles_only_an_acknowledged_outcome() {
        for reply in [
            json!({"ok": true}),
            json!({"ok": false, "code": "busy", "error": "refused"}),
            json!({"ok": false, "uncertain": true, "error": "unknown"}),
        ] {
            let (_dir, client, listener, session) = native_client();
            let mut caller = Box::pin(client.acquire_input("owner", &session.epoch));
            let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
            assert_eq!(client.controller.pending().unwrap().len(), 1);
            answer(&mut acquire, reply.clone()).await;
            assert_eq!(caller.await.is_ok(), reply["ok"] == true);
            if reply["uncertain"] == true {
                recovery_settles_the_abandoned_claim(&client);
            } else {
                assert!(client.controller.pending().unwrap().is_empty());
                assert!(client.controller.acquire(false).is_ok());
            }
        }
    }

    /// A status reply sufficient for repinning preflight. It deliberately
    /// lacks the independent proof required by later session verification.
    fn repinnable_status(session: &Session) -> Value {
        json!({"ok": true, "inputService": {
            "ok": true, "protocolVersion": 1, "accessibility": true,
            "enforcedEpoch": session.epoch.input_epoch(),
        }})
    }

    #[tokio::test]
    async fn a_display_reset_interrupted_after_dispatch_stays_pending_until_recovery() {
        let (_dir, client, listener, session) = native_client();
        let status = repinnable_status(&session);
        let mut caller = Box::pin(client.repin(session));
        let mut hello = dispatched(&mut caller, &listener, "hello").await;
        answer(&mut hello, status).await;
        let _reset = dispatched(&mut caller, &listener, "reset").await;
        assert_eq!(client.controller.pending().unwrap().len(), 1);
        drop(caller);
        recovery_settles_the_abandoned_claim(&client);
    }

    #[tokio::test]
    async fn an_acknowledged_display_reset_is_settled_before_session_verification() {
        let (_dir, client, listener, session) = native_client();
        let status = repinnable_status(&session);
        let mut caller = Box::pin(client.repin(session));
        let mut hello = dispatched(&mut caller, &listener, "hello").await;
        answer(&mut hello, status.clone()).await;
        let mut reset = dispatched(&mut caller, &listener, "reset").await;
        assert_eq!(client.controller.pending().unwrap().len(), 1);
        answer(&mut reset, json!({"ok": true, "released": true})).await;
        let mut verifying = dispatched(&mut caller, &listener, "hello").await;
        assert!(client.controller.pending().unwrap().is_empty());
        answer(&mut verifying, status).await;
        assert_eq!(caller.await.unwrap_err().code, "identity");
        assert!(client.controller.pending().unwrap().is_empty());
        assert!(client.controller.acquire(false).is_ok());
    }

    #[test]
    fn an_interrupted_setup_can_repair_its_probe_without_settling_pending_work() {
        let dir = tempfile::tempdir().unwrap();
        let mut client = client_in(dir.path());
        client.controller = Controller::at(60001, &dir.path().join("state"));
        let previous = client.controller.quarantine().unwrap();
        let generation = previous.generation().to_owned();
        drop(previous);
        assert!(!client.config.identity_probe.exists());
        assert!(client.install().is_err());
        let competing = client.controller.acquire(true).unwrap();
        assert!(client.bootstrap_probe().is_err());
        assert!(!client.config.identity_probe.exists());
        drop(competing);
        client.bootstrap_probe().unwrap();
        assert!(client.config.identity_probe.is_file());
        assert_eq!(
            file_sha256(&client.config.identity_probe).unwrap(),
            file_sha256(&std::env::current_exe().unwrap()).unwrap()
        );
        assert_eq!(
            client.controller.pending().unwrap(),
            std::slice::from_ref(&generation)
        );
        assert!(client.controller.acquire(false).is_err());
        fs::write(&client.config.identity_probe, b"interrupted copy").unwrap();
        client.bootstrap_probe().unwrap();
        assert_eq!(
            file_sha256(&client.config.identity_probe).unwrap(),
            file_sha256(&std::env::current_exe().unwrap()).unwrap()
        );
        assert_eq!(client.controller.pending().unwrap(), [generation]);
        let owned = client.controller.acquire(true).unwrap();
        client.controller.clear().unwrap();
        drop(owned);
        assert!(client.controller.acquire(false).is_ok());
    }

    #[test]
    fn a_successful_recovery_allows_the_next_normal_acquisition() {
        let dir = tempfile::tempdir().unwrap();
        let mut client = client_in(dir.path());
        client.controller = Controller::at(60001, &dir.path().join("state"));
        let ownership = client.controller.acquire(true).unwrap();
        drop(client.controller.quarantine().unwrap());
        let claim = client.controller.quarantine().unwrap();
        // The verified recovery clears abandoned work while retaining its
        // own live claim, which the restart success path must then settle.
        client.controller.clear().unwrap();
        client
            .settle_restart(&claim, Ok(json!({"ok": true, "restarted": true})))
            .unwrap();
        drop(claim);
        drop(ownership);
        assert!(client.controller.pending().unwrap().is_empty());
        assert!(client.controller.acquire(false).is_ok());

        let other = client.controller.quarantine().unwrap();
        let claim = client.controller.quarantine().unwrap();
        client.controller.clear().unwrap();
        client
            .settle_restart(&claim, Ok(json!({"ok": true, "restarted": true})))
            .unwrap();
        drop(claim);
        assert_eq!(client.controller.pending().unwrap(), [other.generation()]);
        assert!(client.controller.acquire(false).is_err());
    }

    /// A client for a configuration kept in `dir`.
    fn client_in(dir: &Path) -> Client {
        let text = "helper_uid = 60001\nsocket_path = \"c.sock\"\noutput_dir = \"out\"\n\
                    identity_probe = \"probe\"\ninput_socket = \"i.sock\"\n";
        let config = Config::parse(text.as_bytes(), &dir.join("u.toml")).unwrap();
        Client::new(config).unwrap()
    }

    /// A login is gone only once it is seen gone twice in a row and its
    /// daemon stopped answering; a look in between that sees it again
    /// starts the count over.
    #[tokio::test]
    async fn a_login_is_gone_only_when_seen_gone_twice_and_silent() {
        use std::{cell::RefCell, collections::VecDeque};

        let pause = Duration::from_millis(1);
        let limit = Duration::from_secs(5);
        let run = |looks: &[bool], hellos: &[bool]| {
            let looks = RefCell::new(looks.iter().copied().collect::<VecDeque<_>>());
            let hellos = RefCell::new(hellos.iter().copied().collect::<VecDeque<_>>());
            let seen = RefCell::new(0);
            async move {
                let gone = gone_within(
                    || {
                        *seen.borrow_mut() += 1;
                        let look = looks.borrow_mut().pop_front().unwrap_or(false);
                        async move { look }
                    },
                    || {
                        let hello = hellos.borrow_mut().pop_front().unwrap_or(false);
                        async move { hello }
                    },
                    limit,
                    pause,
                )
                .await;
                (gone, seen.into_inner())
            }
        };

        assert_eq!(run(&[true, true, true], &[]).await, (true, 3));
        assert_eq!(
            run(&[true, false, true, true, true], &[]).await,
            (true, 5),
            "a login seen again starts the count over"
        );
        assert_eq!(
            run(&[true, true, true, true, true], &[true, true]).await,
            (true, 5),
            "waits for the daemon to go quiet, looking all the while"
        );
        assert_eq!(
            run(&[true, true, false, true, true, true], &[true, false]).await,
            (true, 6),
            "a login back after the second look starts the count over"
        );
        assert_eq!(
            run(&[true, true, false, true, true, true], &[false, false]).await,
            (true, 6),
            "a login back while the daemon was asked starts the count over"
        );

        // Back after the second look, then its daemon goes quiet: still
        // logged in, never gone.
        let back = Duration::from_millis(50);
        let looks = std::cell::Cell::new(0);
        let hellos = std::cell::Cell::new(0);
        let returned = gone_within(
            || {
                looks.set(looks.get() + 1);
                let look = looks.get() <= 2;
                async move { look }
            },
            || {
                hellos.set(hellos.get() + 1);
                let hello = hellos.get() == 1;
                async move { hello }
            },
            back,
            pause,
        )
        .await;
        assert!(!returned, "a login that came back is not gone");

        // It logs in again while the daemon is still being asked, and the
        // daemon then says nothing: still logged in.
        let again = std::cell::Cell::new(false);
        let returned = gone_within(
            || {
                let look = !again.get();
                async move { look }
            },
            || {
                again.set(true);
                async move {
                    tokio::time::sleep(pause).await;
                    false
                }
            },
            back,
            pause,
        )
        .await;
        assert!(!returned, "back while its daemon was asked");

        let short = Duration::from_millis(50);
        let stays = gone_within(|| async { false }, || async { false }, short, pause).await;
        assert!(!stays, "a login that stays is not gone");
        let answering = gone_within(|| async { true }, || async { true }, short, pause).await;
        assert!(!answering, "a daemon that keeps answering is not gone");
    }

    #[tokio::test]
    async fn an_input_change_stays_pending_only_while_its_outcome_is_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let mut client = client_in(dir.path());
        client.controller = crate::control::Controller::at(60001, &dir.path().join("state"));

        assert_eq!(
            client.pending_until_known(async { Ok(1) }).await.unwrap(),
            1
        );

        let refused = Error::new("native", "refused");
        assert!(
            client
                .pending_until_known(async { Err::<(), _>(refused) })
                .await
                .is_err()
        );
        assert!(
            client.controller.pending().unwrap().is_empty(),
            "known outcomes clear"
        );

        let lost = Error::new("transport", "reply lost").uncertain();
        assert!(
            client
                .pending_until_known(async { Err::<(), _>(lost) })
                .await
                .is_err()
        );
        assert_eq!(
            client.controller.unresolved().unwrap().len(),
            1,
            "an unknown one stays"
        );
    }

    #[test]
    fn process_existence_ignores_ownership() {
        assert!(process_exists(std::process::id()));
        assert!(process_exists(1), "launchd exists though owned by root");
        assert!(!process_exists(u32::MAX));
    }

    #[tokio::test]
    async fn only_the_person_resumes_agents() {
        let dir = tempfile::tempdir().unwrap();
        let client = client_in(dir.path());
        assert_eq!(client.hold(false).await.unwrap_err().code, "stopped");
    }

    #[tokio::test]
    async fn a_stop_holds_on_the_persons_side_while_the_helpers_input_is_down() {
        let dir = tempfile::tempdir().unwrap();
        let mut person = client_in(dir.path());
        person.controller = crate::control::Controller::at(60001, &dir.path().join("state"));
        person.actor = Actor::Viewer;
        let mut agent = client_in(dir.path());
        agent.controller = crate::control::Controller::at(60001, &dir.path().join("state"));

        let stopped = person.hold(true).await.unwrap();
        assert_eq!(stopped["held"], true);
        assert!(
            stopped["warnings"][0]
                .as_str()
                .unwrap()
                .contains("isn't running")
        );
        assert_eq!(agent.require_going().await.unwrap_err().code, "stopped");
        assert!(
            tokio::time::timeout(Duration::from_secs(2), agent.stopped_while_running())
                .await
                .is_ok()
        );
        let acting = agent.clone().doing(Doing::new("run"));
        let refused = acting
            .mutate(json!({"op": "run", "argv": ["true"]}), None, None)
            .await
            .unwrap_err();
        assert_eq!(refused.code, "stopped");
        assert!(!refused.uncertain, "nothing started");

        let marker = person.config.input_socket.with_extension("stopped");
        fs::write(&marker, b"").unwrap();
        let refused = person.hold(false).await.unwrap_err();
        assert_eq!(refused.code, "stopped");
        assert!(person.controller.stop_noted());
        assert!(marker.exists());

        fs::remove_file(marker).unwrap();
        person.hold(false).await.unwrap();
        assert!(agent.require_going().await.is_ok(), "resumed");
    }

    #[tokio::test]
    async fn a_running_command_hears_of_a_stop_from_its_saved_marker() {
        let dir = tempfile::tempdir().unwrap();
        let client = client_in(dir.path());
        let marker = client.config.input_socket.with_extension("stopped");

        let waiting =
            tokio::time::timeout(Duration::from_millis(500), client.stopped_while_running());
        assert!(waiting.await.is_err(), "no stop, no end");

        std::fs::write(&marker, b"").unwrap();
        let heard = tokio::time::timeout(Duration::from_secs(2), client.stopped_while_running());
        assert!(heard.await.is_ok(), "the stop is heard within a moment");
    }

    #[tokio::test]
    async fn a_local_stop_cancels_a_running_command_and_refuses_its_next_dispatch() {
        let (dir, mut client, input, _session) = native_client();
        drop(input);
        client.transport.path = dir.path().join("helper.sock");
        let helper = UnixListener::bind(&client.transport.path).unwrap();
        let mut person = client.clone();
        person.actor = Actor::Viewer;
        let acting = client.doing(Doing::new("run"));
        let run = json!({"op": "run", "argv": ["true"], "timeoutMs": 1000});
        let ownership = acting.controller.acquire(false).unwrap();
        let mut caller = Box::pin(acting.mutate(run.clone(), None, None));
        let mut peer = dispatched(&mut caller, &helper, "run").await;
        let pending = acting.controller.pending().unwrap();
        assert_eq!(pending.len(), 1);

        let stopped = person.hold(true).await.unwrap();
        assert_eq!(stopped["ok"], true);
        assert_eq!(stopped["held"], true);
        assert!(
            !acting
                .config
                .input_socket
                .with_extension("stopped")
                .exists()
        );
        let error = tokio::time::timeout(Duration::from_secs(2), caller)
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(error.code, "stopped");
        assert!(error.uncertain);
        assert_eq!(acting.controller.pending().unwrap(), pending);
        assert_eq!(acting.controller.acquire(true).unwrap_err().code, "busy");
        let mut line = String::new();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), peer.read_line(&mut line))
                .await
                .unwrap()
                .unwrap(),
            0,
            "the helper sees the cancelled connection"
        );

        let error = acting.mutate(run, None, None).await.unwrap_err();
        assert_eq!(error.code, "stopped");
        assert!(!error.uncertain, "the next command never starts");
        assert!(
            tokio::time::timeout(Duration::from_millis(50), helper.accept())
                .await
                .is_err()
        );
        drop(ownership);
        recovery_settles_the_abandoned_claim(&acting);
    }

    #[tokio::test]
    async fn resume_keeps_the_local_stop_until_the_helpers_stop_can_be_cleared() {
        let (_dir, client, listener, _session) = native_client();
        let mut person = client;
        person.actor = Actor::Viewer;
        let marker = person.config.input_socket.with_extension("stopped");
        let mut stopping = Box::pin(person.hold(true));
        let mut hello = dispatched(&mut stopping, &listener, "hello").await;
        answer(&mut hello, json!({"ok": true, "capabilities": ["hold"]})).await;
        let mut hold = dispatched(&mut stopping, &listener, "hold").await;
        fs::write(&marker, b"").unwrap();
        answer(&mut hold, json!({"ok": true, "held": true, "kept": true})).await;
        assert_eq!(stopping.await.unwrap()["held"], true);
        drop(listener);

        let error = person.hold(false).await.unwrap_err();
        assert_eq!(error.code, "stopped");
        assert!(
            error
                .error
                .contains("Recover input before agents can resume")
        );
        assert!(person.controller.stop_noted());
        assert!(marker.exists());
        let mut agent = person.clone();
        agent.actor = Actor::Agent;
        assert_eq!(agent.require_going().await.unwrap_err().code, "stopped");

        fs::remove_file(marker).unwrap();
        assert_eq!(person.hold(false).await.unwrap()["held"], false);
        assert!(!person.controller.stop_noted());
    }

    #[tokio::test]
    async fn resume_clears_the_local_stop_only_after_native_acknowledgment() {
        for acknowledged in [false, true] {
            let (_dir, client, listener, _session) = native_client();
            let mut person = client;
            person.actor = Actor::Viewer;
            person.controller.note_stop(true).unwrap();
            let mut resuming = Box::pin(person.hold(false));
            let mut hello = dispatched(&mut resuming, &listener, "hello").await;
            assert!(person.controller.stop_noted());
            answer(&mut hello, json!({"ok": true, "capabilities": ["hold"]})).await;
            let mut hold = dispatched(&mut resuming, &listener, "hold").await;
            assert!(person.controller.stop_noted());
            answer(
                &mut hold,
                json!({"ok": acknowledged, "code": "native", "error": "refused", "held": false}),
            )
            .await;
            assert_eq!(resuming.await.is_ok(), acknowledged);
            assert_eq!(person.controller.stop_noted(), !acknowledged);
        }
    }

    #[tokio::test]
    async fn a_stopped_agents_command_is_refused_before_it_starts_and_a_read_is_not() {
        let dir = tempfile::tempdir().unwrap();
        let client = client_in(dir.path());
        std::fs::write(client.config.input_socket.with_extension("stopped"), b"").unwrap();
        let run = json!({"op": "run", "argv": ["true"], "timeoutMs": 1000});

        let acting = client.clone().doing(Doing::new("run"));
        let refused = acting.mutate(run.clone(), None, None).await.unwrap_err();
        assert_eq!(refused.code, "stopped");
        assert!(!refused.uncertain, "nothing started");

        let reading = client.mutate(run, None, None).await.unwrap_err();
        assert_ne!(
            reading.code, "stopped",
            "a read goes on (and finds no helper here)"
        );
    }

    #[test]
    fn input_names_the_app_it_landed_in_and_a_command_its_start() {
        let dir = tempfile::tempdir().unwrap();
        let client = client_in(dir.path()).doing(Doing::new("type"));
        let typed = json!({"op": "input", "actions": [{"type": "type", "text": "x"}]});
        client
            .record(
                &typed,
                &json!({"ok": true, "app": "TextEdit"}),
                Duration::ZERO,
            )
            .unwrap();
        let rows = crate::overview::activity(client.output_dir().unwrap(), 5);
        assert_eq!(crate::doing::says(&rows[0]), "typed in TextEdit");

        let client = client.doing(Doing::new("open").app(Some("Calculator")));
        let run = json!({"op": "run", "argv": ["open", "-a", "Calculator"]});
        client.record_start(&run).unwrap();
        let rows = crate::overview::activity(client.output_dir().unwrap(), 5);
        assert_eq!(crate::doing::says(&rows[0]), "opening Calculator…");
        client
            .record(&run, &json!({"ok": true, "exitCode": 0}), Duration::ZERO)
            .unwrap();
        let rows = crate::overview::activity(client.output_dir().unwrap(), 5);
        assert_eq!(
            crate::doing::says(&rows[0]),
            "opened Calculator",
            "the end stands for the start"
        );
        assert_eq!(rows.len(), 2, "one row per command");
    }

    #[test]
    fn a_command_that_failed_or_timed_out_is_recorded_as_failed() {
        assert!(worked(&json!({"ok": true})));
        assert!(worked(
            &json!({"ok": true, "exitCode": 0, "timedOut": false})
        ));
        assert!(!worked(&json!({"ok": true, "exitCode": 1})));
        assert!(!worked(
            &json!({"ok": true, "exitCode": 0, "timedOut": true})
        ));
        assert!(!worked(&json!({"ok": false})));
    }

    #[tokio::test]
    async fn a_screenshot_is_recorded_as_the_agent_looking_and_a_failed_one_as_failed() {
        let dir = tempfile::tempdir().unwrap();
        let client = client_in(dir.path()).doing(Doing::new("screenshot"));
        assert!(client.capture(None).await.is_err(), "no helper answers");

        let rows = crate::overview::activity(client.output_dir().unwrap(), 5);
        assert_eq!(
            (&rows[0]["operation"], &rows[0]["ok"], &rows[0]["actor"]),
            (&json!("screenshot"), &json!(false), &json!("agent"))
        );
        assert_eq!(crate::doing::says(&rows[0]), "couldn't take a screenshot");
    }

    #[tokio::test]
    async fn a_status_read_without_a_viewer_says_no_viewer_has_control() {
        let dir = tempfile::tempdir().unwrap();
        let mut client = client_in(dir.path());
        client.controller = crate::control::Controller::at(60001, &dir.path().join("state"));

        let status = crate::overview::read(&client).await;
        assert_eq!(status["controlActive"], false);
        assert_eq!(status["overview"]["state"], "notReady", "no helper answers");
        assert_eq!(status["overview"]["label"], "Not ready");
        assert!(
            !status["overview"]["headline"]
                .as_str()
                .unwrap()
                .ends_with('.')
        );
    }

    #[test]
    fn a_failure_keeps_its_fixed_code_and_reads_as_plain_words_but_never_its_message() {
        let dir = tempfile::tempdir().unwrap();
        let click = json!({"op": "input", "actions": [{"type": "click"}]});
        let refused =
            json!({"ok": false, "code": "elements", "error": "No element is named secret"});
        let odd = json!({"ok": false, "code": "Not A Code: secret"});
        let failed = json!({"ok": true, "exitCode": 2});

        // One client per action: rows of one command read as its newest step.
        for reply in [&refused, &odd, &failed] {
            client_in(dir.path())
                .doing(Doing::new("click").app(Some("Opie")))
                .record(&click, reply, Duration::ZERO)
                .unwrap();
        }

        let client = client_in(dir.path());
        let directory = client.output_dir().unwrap();
        let log = std::fs::read_to_string(directory.join("metadata.jsonl")).unwrap();
        assert!(!log.contains("secret"), "{log}");
        let rows = crate::overview::activity(directory, 5);
        let reasons = rows
            .iter()
            .map(|row| crate::overview::shown(row, 0)["reason"].clone())
            .collect::<Vec<_>>();
        assert_eq!(
            reasons,
            [
                json!("The command failed"),
                Value::Null,
                json!("Nothing on screen matched")
            ]
        );
    }

    #[test]
    fn recorded_rows_name_the_actor_and_what_was_done_but_never_typed_text() {
        let dir = tempfile::tempdir().unwrap();
        let mut client = client_in(dir.path()).doing(Doing::new("type").app(Some("TextEdit")));
        let typed = json!({"op": "input", "actions": [{"type": "type", "text": "secret"}]});
        let ok = json!({"ok": true});

        client
            .record(&typed, &ok, Duration::from_millis(5))
            .unwrap();
        client.doing = None;
        client.actor = Actor::Viewer;
        client
            .record(&json!({"op": "restart"}), &ok, Duration::ZERO)
            .unwrap();

        let directory = client.output_dir().unwrap();
        let rows = crate::overview::activity(directory, 5);
        assert_eq!(
            (&rows[0]["actor"], &rows[0]["operation"]),
            (&json!("viewer"), &json!("restart"))
        );
        assert_eq!(
            (&rows[1]["actor"], &rows[1]["actionCount"]),
            (&json!("agent"), &json!(1))
        );
        assert_eq!(
            (&rows[1]["verb"], &rows[1]["app"]),
            (&json!("type"), &json!("TextEdit"))
        );
        assert!(
            rows[0].get("verb").is_none(),
            "a row says only what it knows"
        );

        let log = fs::read_to_string(directory.join("metadata.jsonl")).unwrap();
        assert!(!log.contains("secret"), "no typed text is recorded");
    }
}
