//! The input service's life in the helper: started from the installed
//! executable, reused or reset when it is there, and replaced through its
//! authenticated shutdown when its build is stale.
use super::Client;
use atlas_core::{
    desktop::Kind,
    error::{Code, Error, Result},
    session::{Observation, SHUTDOWN},
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

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
            let exited = match self.desktop().kind() {
                Kind::User(login) => self.input_exited_in(login, pid).await,
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

    /// Start the installed executable as the helper's input service. The caller
    /// records the launch as a pending mutation and settles it on success.
    async fn spawn_input(&self) -> Result<u32> {
        match self.desktop().kind() {
            Kind::User(login) => self.spawn_input_in(login).await,
        }
    }
}
