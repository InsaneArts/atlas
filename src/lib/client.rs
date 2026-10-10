use crate::{
    actions,
    config::Config,
    control::{Controller, SharedLease, write_private},
    digest::file_sha256,
    error::{Error, Result},
    session::{Display, Epoch, Session, input_current, require_hidden},
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

#[derive(Clone)]
pub struct Client {
    pub config: Config,
    pub transport: Transport,
    pub controller: Controller,
    installed: BuildCache,
}

pub fn timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

impl Client {
    pub fn install(&self) -> Result<Value> {
        let _ownership = self.controller.acquire(false)?;
        let source = std::env::current_exe()?;
        let target = &self.config.identity_probe;
        if source == *target {
            return Ok(json!({ "ok": true, "path": target, "alreadyInstalled": true }));
        }
        let parent = target
            .parent()
            .ok_or_else(|| Error::new("config", "Shared executable needs a parent directory"))?;
        fs::create_dir_all(parent)?;
        let temporary = target.with_extension(format!("{}.new", crate::control::token()));
        let result = (|| -> Result<()> {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o755)
                .open(&temporary)?;
            std::io::copy(&mut fs::File::open(source)?, &mut file)?;
            file.set_permissions(fs::Permissions::from_mode(0o755))?;
            file.sync_all()?;
            // A running helper may still map the previous executable. Renaming
            // a fresh inode avoids modifying its code-signed pages in place.
            fs::rename(&temporary, target)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result?;
        Ok(json!({ "ok": true, "path": target }))
    }
    /// Construction only validates; reads such as status and doctor never write.
    pub fn new(config: Config) -> Result<Self> {
        crate::session::validate_uid(config.helper_uid)?;
        Ok(Self {
            controller: Controller::new(config.helper_uid, &config.socket_path)?,
            transport: Transport {
                path: config.socket_path.clone(),
                timeout: Duration::from_secs(30),
                peer_uid: config.helper_uid,
            },
            installed: BuildCache::default(),
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
                .create(path)?;
        }
        let metadata = fs::symlink_metadata(path)?;
        // SAFETY: getuid has no pointer arguments or preconditions.
        if !metadata.is_dir()
            || metadata.uid() != unsafe { libc::getuid() }
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
                    "Install the Rust identity probe at {} and ensure the helper can execute it.",
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
        Ok(json!({
            "ok": true,
            "path": path,
            "width": frame.width,
            "height": frame.height,
            "display": frame.display,
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
            return Err(Error::new(
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
            ));
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
        if !session.input {
            return Err(Error::new(
                "permission",
                "The native input service is unavailable; run understudy start-input",
            ));
        }
        actions::validate(actions, &session.epoch.display)?;
        let standalone = crate::control::token();
        let owner = lease.map(|(_, token)| token).unwrap_or(&standalone);
        if lease.is_none() {
            self.acquire_input(owner, &session.epoch).await?;
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
            if result.is_ok() && cleanup.is_err() {
                self.controller.quarantine()?;
                return Err(Error::new(
                    "uncertain",
                    "Input finished but native release was not confirmed",
                )
                .uncertain());
            }
        }
        result
    }
    pub async fn run(&self, argv: Vec<String>) -> Result<Value> {
        if argv.is_empty() || argv[0].is_empty() || argv.iter().any(|a| a.contains('\0')) {
            return Err(Error::new(
                "input",
                "Provide a command and arguments after run --",
            ));
        }
        let _ownership = self.controller.acquire(false)?;
        self.identity().await?;
        self.mutate(
            json!({"op": "run", "argv": argv, "timeoutMs": 20000}),
            None,
            None,
        )
        .await
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
        let metadata = fs::symlink_metadata(path).ok()?;
        let key = (
            metadata.dev(),
            metadata.ino(),
            metadata.size(),
            metadata.mtime(),
            metadata.mtime_nsec(),
        );
        let mut cache = self.installed.lock().ok()?;
        if let Some((cached, build)) = cache.as_ref()
            && *cached == key
        {
            return build.clone();
        }
        // A failed read is not cached: permissions may be repaired without
        // changing the identity key.
        let build = file_sha256(path)?;
        *cache = Some((key, Some(build.clone())));
        Some(build)
    }
    async fn launch_input(&self) -> Result<Value> {
        let status = self.status().await;
        let session = require_hidden(&status, self.config.helper_uid)?;
        let native = &status["inputService"];
        let current = native["build"].is_string() && native["build"] == status["installedBuild"];
        if native["ok"] == true && !current {
            return self.upgrade_input(&status).await;
        }
        if session.input {
            return Ok(json!({
                "ok": true,
                "alreadyRunning": true,
                "pid": session.epoch.input_pid,
            }));
        }
        if native["ok"] == true {
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
            return Ok(json!({ "ok": true, "recovered": true, "pid": session.epoch.input_pid }));
        }
        let pid = self.spawn_input().await?;
        Ok(json!({ "ok": true, "started": true, "pid": pid }))
    }
    /// Replace a running service whose build differs from the installed
    /// executable. Its own SIGTERM handling closes ownership and releases held
    /// input. The replacement stays a pending mutation until the new process
    /// is ready, runs the installed build and holds nothing.
    async fn upgrade_input(&self, status: &Value) -> Result<Value> {
        let native = &status["inputService"];
        if !status["installedBuild"].is_string() {
            return Err(Error::new(
                "startup",
                "The installed executable cannot be read; refusing to replace the running service",
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
    async fn stop_input(&self, pid: u32) -> Result<()> {
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
    /// Start the installed executable as the helper's input service. Launching
    /// is a pending mutation until a ready service answers.
    async fn spawn_input(&self) -> Result<u32> {
        // Arguments stay separate from shell syntax, including configured paths.
        let script = concat!(
            "nohup \"$1\" input-service --uid \"$2\" --controller-uid \"$3\" --socket \"$4\" ",
            ">\"$HOME/Library/Logs/Understudy-input.log\" 2>&1 </dev/null & echo $!",
        );
        // SAFETY: getuid has no pointer arguments or preconditions.
        let controller_uid = unsafe { libc::getuid() };
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
        let generation = self.controller.quarantine()?;
        let reply = self.transport.call(&request).await?;
        if reply["ok"] != true || reply["exitCode"] != 0 {
            return Err(Error::new(
                "startup",
                "The helper could not launch the Rust input service",
            )
            .uncertain());
        }
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            let state = self.status().await;
            if let Ok(session) = require_hidden(&state, self.config.helper_uid)
                && session.input
            {
                self.controller.clear_if(&generation)?;
                return Ok(session.epoch.input_pid);
            }
            if Instant::now() >= deadline {
                return Err(Error::new(
                    "startup",
                    "Input service did not become ready. Check ~/Library/Logs/Understudy-input.log \
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
        self.controller.quarantine()?;
        // Input has its own process. Clear held state there before restarting
        // the capture/command helper; a new capture PID alone proves nothing
        // about an uncertain mouse or keyboard action.
        self.launch_input().await?;
        let session = self.identity().await?;
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
                if let Ok(session) = require_hidden(&status, self.config.helper_uid)
                    && session.epoch.capture_pid != before.epoch.capture_pid
                {
                    let native = &status["inputService"];
                    if !session.input
                        || native["controlActive"] != false
                        || native["heldInput"] != false
                    {
                        continue;
                    }
                    self.controller.clear()?;
                    return Ok(json!({"ok": true, "restarted": true, "session": status}));
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(10), recovery)
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
        let transport = if request["op"] == "input" {
            self.input_transport()
        } else {
            self.transport.clone()
        };
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
        let generation = self.controller.quarantine()?;
        let pending = match connection.dispatch(&request).await {
            Ok(pending) => pending,
            Err(error) => {
                if !error.uncertain && request["op"] != "restart" {
                    self.controller.clear_if(&generation)?;
                }
                return Err(error);
            }
        };
        drop(authorization);
        let mut reply = pending.finish().await?;
        validate_completion(&request, &reply)?;
        if request["op"] != "restart"
            && reply["uncertain"] != true
            && let Err(error) = self.controller.clear_if(&generation)
        {
            reply["warnings"] = json!([format!(
                "Operation completed, but its pending marker could not be cleared: {error}"
            )]);
        }
        if self.record(&request, &reply, started.elapsed()).is_err() {
            reply["warnings"] =
                json!(["Operation result is valid; local metadata could not be recorded"]);
        }
        Ok(reply)
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
    // SAFETY: signal 0 only checks for existence and permission.
    let alive = unsafe { libc::kill(pid, 0) } == 0;
    alive || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(test)]
mod tests {
    use super::process_exists;

    #[test]
    fn process_existence_ignores_ownership() {
        assert!(process_exists(std::process::id()));
        assert!(process_exists(1), "launchd exists though owned by root");
        assert!(!process_exists(u32::MAX));
    }
}
