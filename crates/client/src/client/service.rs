//! The input service's life in the helper: started from the installed
//! executable, reused or reset when it is there, and replaced through its
//! authenticated shutdown when its build is stale.
use super::Client;
use atlas_core::{
    desktop::Kind,
    error::{Code, Error, Result},
    session::{Observation, SHUTDOWN},
    transport::run_output,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// Whether a process exists, whoever owns it (EPERM still means it exists).
pub(crate) fn process_exists(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };

    matches!(
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None),
        Ok(()) | Err(nix::errno::Errno::EPERM)
    )
}

/// The process a launch printed, the only one that may satisfy readiness.
/// Any other answer may still have started something.
fn launched_pid(reply: &Value) -> Result<u32> {
    let failed = || {
        Error::new(
            Code::Startup,
            "The helper could not launch the Rust input service",
        )
        .uncertain()
    };
    if reply["ok"] != true || reply["exitCode"] != 0 {
        return Err(failed());
    }

    let output = run_output(reply, 32).map_err(|cause| failed().with_source(cause))?;
    let text = String::from_utf8(output).map_err(|cause| failed().with_source(cause))?;
    let pid = text
        .trim()
        .parse::<u32>()
        .map_err(|cause| failed().with_source(cause))?;
    if pid > 1 { Ok(pid) } else { Err(failed()) }
}

/// Refuse input work while the helper's Atlas lacks Accessibility; an
/// unknown answer is not a refusal.
pub(super) fn require_accessibility(observation: &Observation) -> Result<()> {
    if observation.accessibility == Some(false) || observation.native.accessibility == Some(false) {
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
    /// Start the input service for the verified hidden helper and wait for readiness.
    ///
    /// # Errors
    ///
    /// Returns helper-identity, permission, service-start or readiness failures. Starting a
    /// service does not settle unrelated pending work.
    pub async fn start_input(&self) -> Result<Value> {
        let _ownership = self.controller.acquire(false)?;
        self.identity().await?;
        let status = self.status().await;
        let observation = Observation::decode(&status);
        self.desktop().prove(&observation)?;

        if observation.native.ok && !observation.input_build_current() {
            return self.upgrade_input_for(&status, false).await;
        }

        let (pid, outcome) = self.ensure_input(&observation).await?;
        Ok(json!({ "ok": true, outcome: true, "pid": pid }))
    }

    /// Make a ready input service exist without replacing a running one:
    /// reuse it, reset it, or start the installed build.
    pub(super) async fn ensure_input(
        &self,
        observation: &Observation,
    ) -> Result<(u32, &'static str)> {
        let session = self.desktop().prove(observation)?;
        if session.input {
            return Ok((session.epoch.input_pid, "alreadyRunning"));
        }

        if observation.native.ok {
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
        require_accessibility(observation)?;
        let pid = self.pending_until_known(self.spawn_input()).await?;

        Ok((pid, "started"))
    }

    /// Replace a running service whose build differs from the installed
    /// executable through its authenticated shutdown. The replacement stays
    /// a pending mutation until it is ready, current and holds nothing.
    pub(super) async fn upgrade_input_for(&self, status: &Value, recovery: bool) -> Result<Value> {
        self.require_service_idle_for(recovery).await?;
        let observation = Observation::decode(status);
        let native = &observation.native;

        if observation.installed_build.is_none() {
            return Err(Error::new(
                Code::Startup,
                "The installed executable cannot be read; refusing to replace the running service",
            ));
        }

        if !native.has_capability(SHUTDOWN) {
            return Err(Error::new(
                Code::Startup,
                "The running input service predates authenticated shutdown and cannot be \
                 replaced automatically. Stop that process in the helper account (or log the \
                 helper out and in), then run atlas start-input.",
            ));
        }

        if native.control_active != Some(false) {
            return Err(Error::new(
                Code::Busy,
                "The input service has an active controller; release it before upgrading",
            ));
        }

        let previous = native.pid.filter(|pid| *pid > 1).ok_or_else(|| {
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
        if !self
            .desktop()
            .input_current(&Observation::decode(&state), pid)
        {
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
    ///
    /// # Errors
    ///
    /// Returns the service's refusal, or an error when it does not confirm its
    /// exit.
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
                    .with_source(error)
                }
            })?;

        let deadline = Instant::now() + Duration::from_secs(5);

        loop {
            // Gone once nothing listens on its socket and its process has
            // ended, both as this Mac sees them.
            let exited = match self.desktop().kind() {
                Kind::User(_) => {
                    matches!(
                        tokio::net::UnixStream::connect(&self.config.input_socket).await,
                        Err(error) if matches!(
                            error.kind(),
                            std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                        )
                    ) && !process_exists(pid)
                }
            };
            if exited {
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

    /// Launch the shared Atlas command, installed on this disk, as the input
    /// service of the second login `login`, for this account: its process
    /// ID, before it is known to be ready.
    async fn launch_input_in(&self, login: &atlas_core::user::User) -> Result<u32> {
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
                login.uid.to_string(),
                controller_uid.to_string(),
                self.config.input_socket,
            ],
            "timeoutMs": 3000,
        });
        launched_pid(&self.transport.call(&request).await?)
    }

    /// Start the installed executable as the helper's input service. The caller
    /// records the launch as a pending mutation and settles it on success.
    async fn spawn_input(&self) -> Result<u32> {
        match self.desktop().kind() {
            Kind::User(login) => self.spawn_input_in(login).await,
        }
    }

    /// [`Client::spawn_input`] in the second login `login`: launched, then
    /// waited for until it is ready.
    async fn spawn_input_in(&self, login: &atlas_core::user::User) -> Result<u32> {
        let child = self.launch_input_in(login).await?;
        let deadline = Instant::now() + Duration::from_secs(6);

        loop {
            let state = self.status().await;
            if self
                .desktop()
                .prove(&Observation::decode(&state))
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

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
