//! The input service's life in the helper: started from the installed
//! executable, reused or reset when it is there, and replaced through its
//! authenticated shutdown when its build is stale.
use super::Client;
use crate::{
    error::{Code, Error, Result},
    session::{SHUTDOWN, has_capability, input_current, require_hidden},
    transport::run_output,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// Whether a process exists, whoever owns it (EPERM still means it exists).
pub(super) fn process_exists(pid: u32) -> bool {
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
pub(super) fn require_accessibility(status: &Value) -> Result<()> {
    if status["permissions"]["accessibility"] == false
        || status["inputService"]["accessibility"] == false
    {
        return Err(Error::new(
            Code::Permission,
            "The helper does not allow Atlas Accessibility yet, which input needs. In the \
             helper's System Settings › Privacy & Security › Accessibility, turn Atlas on \
             (atlas init guides this).",
        ));
    }

    Ok(())
}

impl Client {
    pub async fn start_input(&self) -> Result<Value> {
        let _ownership = self.controller.acquire(false)?;
        self.identity().await?;
        self.launch_input().await
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
    pub(super) async fn ensure_input(&self, status: &Value) -> Result<(u32, &'static str)> {
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
                    Code::Startup,
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

    /// Replace a running service whose build differs from the installed
    /// executable through its authenticated shutdown. The replacement stays
    /// a pending mutation until it is ready, current and holds nothing.
    pub(super) async fn upgrade_input(&self, status: &Value) -> Result<Value> {
        let native = &status["inputService"];

        if !status["installedBuild"].is_string() {
            return Err(Error::new(
                Code::Startup,
                "The installed executable cannot be read; refusing to replace the running service",
            ));
        }

        if !has_capability(native, SHUTDOWN) {
            return Err(Error::new(
                Code::Startup,
                "The running input service predates authenticated shutdown and cannot be \
                 replaced automatically. Stop that process in the helper account (or log the \
                 helper out and in), then run atlas start-input.",
            ));
        }

        if native["controlActive"] != false {
            return Err(Error::new(
                Code::Busy,
                "The input service has an active controller; release it before upgrading",
            ));
        }

        let previous = native["pid"]
            .as_u64()
            .and_then(|pid| u32::try_from(pid).ok())
            .filter(|pid| *pid > 1)
            .ok_or_else(|| {
                Error::new(Code::Startup, "The running input service has no valid PID")
            })?;
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
                Code::Uncertain,
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
    pub(crate) async fn stop_input(&self, pid: u32) -> Result<()> {
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
                    Code::Uncertain,
                    "The previous input service did not confirm exit",
                )
                .uncertain());
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Start the installed executable as the helper's input service. The caller
    /// records the launch as a pending mutation and settles it on success.
    pub(crate) async fn spawn_input(&self) -> Result<u32> {
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
                    Code::Startup,
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
                    Code::Startup,
                    "Input service did not become ready. Check ~/Library/Logs/Atlas-input.log \
                     in the helper account and confirm that identity_probe points to the current Rust binary.",
                )
                .uncertain());
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}
