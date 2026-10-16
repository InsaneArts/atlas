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
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
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
        let source = fs::canonicalize(std::env::current_exe()?)?;
        let target = &self.config.identity_probe;
        // Only Atlas's own shared folder is repaired; a custom location keeps
        // the permissions its owner chose.
        if let Some(parent) = target.parent()
            && Path::new(crate::handoff::ROOT).parent() == Some(parent)
        {
            shared_folder(parent)?;
        }
        // A link already pointing at this executable (or this executable
        // itself) changes nothing, so it needs no exclusion and works while
        // an earlier action still awaits recovery. A copy is always replaced.
        if fs::canonicalize(target).ok().as_ref() == Some(&source) {
            return Ok(json!({ "ok": true, "path": target, "alreadyInstalled": true }));
        }
        let _ownership = self.controller.acquire(false)?;
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

    /// Construction only validates; reads such as status never write; doctor only probes that Atlas's own folders can be written, making them when missing.
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
            config,
        })
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
        let proof = self
            .session_proof()
            .await
            .unwrap_or_else(|error| json!(error));
        hello["sessionProof"] = proof;
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
        self.native_request(json!({
            "op": "acquire",
            "owner": owner,
            "epoch": epoch.input_epoch(),
        }))
        .await
    }

    async fn native_request(&self, request: Value) -> Result<Value> {
        let result = self.input_transport().call(&request).await?;
        if result["ok"] != true {
            let error = Error::new(
                match result["code"].as_str() {
                    Some("busy") => "busy",
                    Some("lease-lost") => "lease-lost",
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
        self.native_request(json!({"op": "reset", "epoch": epoch.input_epoch()}))
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
        let standalone = crate::control::token();
        let owner = lease.map(|(_, token)| token).unwrap_or(&standalone);
        if lease.is_none()
            && let Err(error) = self.acquire_input(owner, &session.epoch).await
        {
            if error.uncertain {
                // The service may now hold input for `owner`: release it, or
                // leave the outcome pending for recovery.
                let pending = self.controller.quarantine()?;
                if self.native_control("release", owner).await.is_ok() {
                    self.controller.clear_if(&pending)?;
                }
            }
            return Err(error);
        }
        let result = self
            .mutate(
                json!({
                    "op": "input",
                    "actions": actions,
                    "owner": owner,
                    "epoch": session.epoch.input_epoch(),
                }),
                lease,
                expected.map(|(_, at)| at),
            )
            .await;
        if lease.is_none() {
            let cleanup = self.native_control("release", owner).await;
            if cleanup.is_err() {
                // Nothing will settle this release, whatever the input did;
                // it stays for recovery.
                drop(self.controller.quarantine()?);
                return Err(match result {
                    Ok(_) => Error::new(
                        "uncertain",
                        "Input finished but native release was not confirmed",
                    ),
                    Err(error) => Error::new(
                        "uncertain",
                        format!("{} Native release was not confirmed either.", error.error),
                    ),
                }
                .uncertain());
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

    /// [`Client::inspect`] bounded by `limit`.
    pub(crate) async fn inspect_within(&self, argv: Vec<String>, limit: Duration) -> Result<Value> {
        self.run_pending(argv, limit).await
    }

    /// Execute argv in the verified hidden helper as part of a mutation the
    /// caller holds ownership for and has already recorded as pending. The
    /// caller settles that record once the outcome is established.
    pub async fn run_pending(&self, argv: Vec<String>, timeout: Duration) -> Result<Value> {
        self.identity().await?;
        let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
        let request = json!({"op": "run", "argv": argv, "timeoutMs": timeout_ms});
        let reply = self
            .transport
            .connect()
            .await?
            .dispatch(&request)
            .await?
            .finish()
            .await?;
        validate_completion(&request, &reply)?;
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
            self.native_request(json!({
                "op": "reset",
                "epoch": session.epoch.input_epoch(),
            }))
            .await?;
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
        let generation = self.controller.quarantine()?;
        match self.spawn_input().await {
            Ok(pid) => {
                self.controller.clear_if(&generation)?;
                Ok((pid, "started"))
            }
            Err(error) => {
                if !error.uncertain {
                    self.controller.clear_if(&generation)?;
                }
                Err(error)
            }
        }
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
        let _recovery = self.controller.quarantine()?;
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
                    self.controller.clear()?;
                    return Ok(json!({"ok": true, "restarted": true, "session": status}));
                }
                // Its cleanup was confirmed above, so nothing is held: a stale
                // service that is not ready under the new helper is replaced.
                if stale
                    && status["inputService"]["pid"] == input_pid
                    && status["inputService"]["ready"] == false
                {
                    self.upgrade_input(&status).await?;
                    self.controller.clear()?;
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

    async fn mutate(
        &self,
        request: Value,
        lease: Option<(&SharedLease, &str)>,
        observed_at: Option<Instant>,
    ) -> Result<Value> {
        let started = Instant::now();
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
        // A restart runs under the claim its caller holds through
        // verification; every other mutation claims its own generation.
        let generation = (request["op"] != "restart")
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
        let mut reply = pending.finish().await?;
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
        crate::control::probe_write(self.output_dir()?)
    }

    fn record(&self, request: &Value, reply: &Value, elapsed: Duration) -> Result<()> {
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
        // No input text, argv, window titles, output or image payloads in metadata.
        let row = json!({
            "at": timestamp().to_string(),
            "operation": request["op"],
            "ok": reply["ok"],
            "elapsedMs": elapsed.as_secs_f64() * 1000.0,
            "actionCount": request["actions"].as_array().map(Vec::len),
            "actor": self.actor.as_str(),
        });
        writeln!(file, "{row}")?;
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_existence_ignores_ownership() {
        assert!(process_exists(std::process::id()));
        assert!(process_exists(1), "launchd exists though owned by root");
        assert!(!process_exists(u32::MAX));
    }

    #[test]
    fn recorded_rows_name_the_actor_and_carry_no_payload() {
        let dir = tempfile::tempdir().unwrap();
        let text = "helper_uid = 60001\nsocket_path = \"c.sock\"\noutput_dir = \"out\"\n\
                    identity_probe = \"probe\"\ninput_socket = \"i.sock\"\n";
        let config = Config::parse(text.as_bytes(), &dir.path().join("u.toml")).unwrap();
        let mut client = Client::new(config).unwrap();
        let typed = json!({"op": "input", "actions": [{"type": "type", "text": "secret"}]});
        let ok = json!({"ok": true});
        client
            .record(&typed, &ok, Duration::from_millis(5))
            .unwrap();
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
        let log = fs::read_to_string(directory.join("metadata.jsonl")).unwrap();
        assert!(!log.contains("secret"), "no typed text is recorded");
    }
}
