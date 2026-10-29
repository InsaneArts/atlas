//! Commands run in the verified hidden helper: an agent's own command, a
//! step of a change whose control the caller holds, or a fixed read-only
//! inspection, each stopped after its timeout. [`Run`] says which, and
//! [`Client::run`] runs it.
use super::{Client, Verified, validate_completion};
use crate::{
    control::Ownership,
    error::{Code, Error, Result},
    transport::accepted,
};
use serde_json::{Value, json};
use std::time::Duration;

/// How long `run` lets a command take unless told otherwise.
pub const RUN_TIMEOUT: Duration = Duration::from_secs(20);
/// How long a fixed, read-only inspection may take unless told otherwise.
const INSPECT_TIMEOUT: Duration = Duration::from_secs(5);
/// The longest a command may run (the helper's own limit).
pub const RUN_TIMEOUT_MAX: Duration = Duration::from_secs(600);
/// Time beyond a command's timeout for the helper to stop what it left
/// running and send its last output.
pub(super) const RUN_MARGIN: Duration = Duration::from_secs(10);

/// How a command runs in the helper: for how long, in which session, and
/// under whose control.
#[derive(Clone, Copy)]
pub(crate) struct Run<'a> {
    /// When the helper stops the command.
    timeout: Duration,
    /// The session the caller verified moments before, if it names one.
    pin: Option<&'a Verified>,
    mode: Mode,
}

/// Under whose control a command runs, and what is recorded of it.
#[derive(Clone, Copy)]
enum Mode {
    /// A recorded change under control taken for it.
    Held,
    /// An agent's own recorded command, under control taken beside other
    /// agents' input and let go once its request reaches the helper.
    Beside,
    /// A recorded change under control the caller holds.
    Owned,
    /// A step of a change the caller has recorded as pending itself.
    Pending,
    /// A read-only inspection, recorded nowhere.
    Inspect,
}

impl<'a> Run<'a> {
    fn new(mode: Mode, timeout: Duration) -> Self {
        Self {
            timeout,
            pin: None,
            mode,
        }
    }

    /// A recorded change under control taken for it. Control stays held
    /// until the command ends.
    pub(crate) fn held() -> Self {
        Self::new(Mode::Held, RUN_TIMEOUT)
    }

    /// An agent's own command. Control is held only until the request
    /// reaches the helper; while the command runs, its claim keeps
    /// recovery, restart, update and log-out out, and other agents may act.
    /// Its outcome is pending until it ends.
    fn beside() -> Self {
        Self::new(Mode::Beside, RUN_TIMEOUT)
    }

    /// A recorded change under controller ownership the caller already
    /// holds.
    pub(crate) fn owned(_ownership: &Ownership) -> Self {
        Self::new(Mode::Owned, RUN_TIMEOUT)
    }

    /// Part of a mutation the caller holds ownership for and has already
    /// recorded as pending. The caller settles that record once the outcome
    /// is established. A refusal is an error with the helper's code,
    /// uncertain when the command may have started.
    pub(crate) fn pending() -> Self {
        Self::new(Mode::Pending, RUN_TIMEOUT)
    }

    /// A fixed, read-only inspection command (such as the window list). It
    /// records no pending mutation, so callers must only pass commands that
    /// cannot change the helper. A read changes nothing, so its failure is
    /// never uncertain.
    pub(crate) fn inspect() -> Self {
        Self::new(Mode::Inspect, INSPECT_TIMEOUT)
    }

    /// This run, stopping the command after `timeout` (at most ten
    /// minutes).
    pub(crate) fn within(self, timeout: Duration) -> Self {
        Self { timeout, ..self }
    }

    /// This run in the helper session `verified` names, which the caller
    /// verified moments before. A daemon that can be told the session
    /// refuses the run in any other, so it is not verified again; an older
    /// one is verified again first, as for any run.
    pub(crate) fn pinned(self, verified: &'a Verified) -> Self {
        Self {
            pin: Some(verified),
            ..self
        }
    }
}

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
    /// Run argv in the verified hidden helper as `run` says.
    pub(crate) async fn run(&self, argv: Vec<String>, run: Run<'_>) -> Result<Value> {
        let mut request = run_request(argv, run.timeout)?;
        let taken = match run.mode {
            Mode::Held => Some(self.controller.acquire(false)?),
            Mode::Beside => Some(self.controller.acquire_beside_commands()?),
            Mode::Owned | Mode::Pending | Mode::Inspect => None,
        };

        match run.pin.and_then(Verified::pin) {
            Some(pin) => request["session"] = json!(pin),
            None => {
                self.identity().await?;
            }
        }

        match run.mode {
            // Control taken for a held run stays held until the command
            // ends; one beside others lets go of it once its request is sent.
            Mode::Held | Mode::Owned => self.mutate(request, None, None, None).await,
            Mode::Beside => self.mutate(request, None, None, taken).await,
            Mode::Pending => self.run_unrecorded(&request, run.timeout).await,
            Mode::Inspect => self
                .run_unrecorded(&request, run.timeout)
                .await
                .map_err(Error::certain),
        }
    }

    /// Run argv in the verified hidden helper as an agent's own command,
    /// stopping it after `timeout`. Control is held only until the request
    /// reaches the helper; while the command runs, its claim keeps
    /// recovery, restart, update and log-out out, and other agents may act.
    /// Its outcome is pending until it ends.
    pub async fn run_beside(&self, argv: Vec<String>, timeout: Duration) -> Result<Value> {
        self.run(argv, Run::beside().within(timeout)).await
    }

    /// Execute argv in the verified hidden helper as a recorded mutation
    /// under controller ownership the caller already holds, stopping the
    /// command after `timeout`.
    pub async fn run_owned_within(
        &self,
        ownership: &Ownership,
        argv: Vec<String>,
        timeout: Duration,
    ) -> Result<Value> {
        self.run(argv, Run::owned(ownership).within(timeout)).await
    }

    /// Run a fixed, read-only inspection command with a run's usual limit,
    /// under controller ownership the caller already holds. Only for
    /// commands that cannot change the helper: nothing is left pending when
    /// it fails.
    pub(crate) async fn inspect_owned(
        &self,
        _ownership: &Ownership,
        argv: Vec<String>,
    ) -> Result<Value> {
        self.run(argv, Run::inspect().within(RUN_TIMEOUT)).await
    }

    /// Send the run `request` to the verified helper, recording nothing.
    /// The reply comes after the command ends, as for any other run.
    async fn run_unrecorded(&self, request: &Value, timeout: Duration) -> Result<Value> {
        let mut transport = self.transport.clone();
        transport.timeout = transport.timeout.max(timeout + RUN_MARGIN);

        let reply = transport.call(request).await?;
        validate_completion(request, &reply)?;

        accepted(reply)
    }
}
