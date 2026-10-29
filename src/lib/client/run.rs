//! Commands run in the verified hidden helper: an agent's own command, a
//! step of a change whose control the caller holds, or a fixed read-only
//! inspection, each stopped after its timeout.
use super::{Client, Verified, validate_completion};
use crate::{
    control::Ownership,
    daemon::protocol::SessionPin,
    error::{Code, Error, Result},
    transport::accepted,
};
use serde_json::{Value, json};
use std::time::Duration;

/// How long `run` lets a command take unless told otherwise.
pub const RUN_TIMEOUT: Duration = Duration::from_secs(20);
/// How long a fixed, read-only inspection may take unless told otherwise.
pub(crate) const INSPECT_TIMEOUT: Duration = Duration::from_secs(5);
/// The longest a command may run (the helper's own limit).
pub const RUN_TIMEOUT_MAX: Duration = Duration::from_secs(600);
/// Time beyond a command's timeout for the helper to stop what it left
/// running and send its last output.
pub(super) const RUN_MARGIN: Duration = Duration::from_secs(10);

/// The request that runs `argv` in the helper, stopping it after `timeout`:
/// a program and its arguments, none holding a NUL, for 1 second to 10
/// minutes.
fn run_request(argv: Vec<String>, timeout: Duration) -> Result<Value> {
    if argv.is_empty() || argv[0].is_empty() || argv.iter().any(|a| a.contains('\0')) {
        return Err(Error::new(
            Code::Input,
            "Provide a command and arguments after run --",
        ));
    }

    if timeout.is_zero() || timeout > RUN_TIMEOUT_MAX {
        return Err(Error::new(
            Code::Usage,
            "A command may run for 1 second to 10 minutes",
        ));
    }

    let timeout_ms = crate::time::ms(timeout);
    Ok(json!({"op": "run", "argv": argv, "timeoutMs": timeout_ms}))
}

impl Client {
    pub async fn run(&self, argv: Vec<String>) -> Result<Value> {
        self.run_within(argv, RUN_TIMEOUT).await
    }

    /// [`Client::run`], stopping the command after `timeout` (at most ten
    /// minutes). Control stays held until it ends.
    async fn run_within(&self, argv: Vec<String>, timeout: Duration) -> Result<Value> {
        let ownership = self.controller.acquire(false)?;
        self.run_owned_within(&ownership, argv, timeout).await
    }

    /// Run argv in the verified hidden helper as an agent's own command,
    /// stopping it after `timeout`. Control is held only until the request
    /// reaches the helper; while the command runs, its claim keeps
    /// recovery, restart, update and log-out out, and other agents may act.
    /// Its outcome is pending until it ends.
    pub async fn run_beside(&self, argv: Vec<String>, timeout: Duration) -> Result<Value> {
        let request = run_request(argv, timeout)?;
        let ownership = self.controller.acquire_beside_commands()?;
        self.identity().await?;

        self.mutate(request, None, None, Some(ownership)).await
    }

    /// Execute argv in the verified hidden helper as a recorded mutation under
    /// controller ownership the caller already holds.
    pub(crate) async fn run_owned(
        &self,
        ownership: &Ownership,
        argv: Vec<String>,
    ) -> Result<Value> {
        self.run_owned_within(ownership, argv, RUN_TIMEOUT).await
    }

    /// [`Client::run_owned`], stopping the command after `timeout`.
    pub async fn run_owned_within(
        &self,
        _ownership: &Ownership,
        argv: Vec<String>,
        timeout: Duration,
    ) -> Result<Value> {
        let request = run_request(argv, timeout)?;
        self.identity().await?;

        self.mutate(request, None, None, None).await
    }

    /// Run a fixed, read-only inspection command (such as the window list) in
    /// the verified hidden helper. It records no pending mutation, so callers
    /// must only pass commands that cannot change the helper.
    pub(crate) async fn inspect(&self, argv: Vec<String>) -> Result<Value> {
        self.inspect_within(argv, INSPECT_TIMEOUT).await
    }

    /// [`Client::inspect`] bounded by `limit`. A read changes nothing, so
    /// its failure is never uncertain.
    pub(crate) async fn inspect_within(&self, argv: Vec<String>, limit: Duration) -> Result<Value> {
        self.run_pending(argv, limit).await.map_err(Error::certain)
    }

    /// [`Client::inspect_within`] in the helper session `verified` names,
    /// which the caller verified moments before. A daemon that can be told
    /// the session refuses the run in any other, so it is not verified
    /// again; an older one is verified again first, as for any inspection.
    pub(crate) async fn inspect_in(
        &self,
        verified: &Verified,
        argv: Vec<String>,
        limit: Duration,
    ) -> Result<Value> {
        let pin = verified.pin();
        if pin.is_none() {
            self.identity().await?;
        }

        self.run_verified(argv, limit, pin)
            .await
            .map_err(Error::certain)
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
    pub(crate) async fn run_pending(&self, argv: Vec<String>, timeout: Duration) -> Result<Value> {
        self.identity().await?;
        self.run_verified(argv, timeout, None).await
    }

    /// [`Client::run_pending`] once the helper is verified, served only in
    /// the session `pin` names when there is one.
    async fn run_verified(
        &self,
        argv: Vec<String>,
        timeout: Duration,
        pin: Option<SessionPin>,
    ) -> Result<Value> {
        let timeout_ms = crate::time::ms(timeout);
        let mut request = json!({"op": "run", "argv": argv, "timeoutMs": timeout_ms});
        if let Some(pin) = pin {
            request["session"] = json!(pin);
        }

        // The reply comes after the command ends, as for any other run.
        let mut transport = self.transport.clone();
        transport.timeout = transport.timeout.max(timeout + RUN_MARGIN);

        let reply = transport.call(&request).await?;
        validate_completion(&request, &reply)?;

        accepted(reply)
    }
}
