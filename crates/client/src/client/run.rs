//! Commands run in the verified hidden helper: an agent's own command, a
//! step of a change whose control the caller holds, or a fixed read-only
//! inspection, each stopped after its timeout. [`Run`] says which, and
//! [`Client::run`] runs it.
use super::{Client, Mutation, Verified};
use atlas_control::{Ownership, SharedLease};
use atlas_core::{
    error::{Code, Error, Result},
    transport::{OutputError, accepted, run_output},
};
use serde_json::Value;
use std::{borrow::Cow, time::Duration};

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
    /// The person's lease the command is sent under, by its token: checked
    /// once connected and held until the request is sent, so a lease that
    /// ran out before then sends nothing.
    lease: Option<(&'a SharedLease, &'a str)>,
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
            lease: None,
        }
    }

    /// This run, sent only while the person's `lease` still holds `token`.
    pub(crate) fn leased(self, lease: Option<(&'a SharedLease, &'a str)>) -> Self {
        Self { lease, ..self }
    }

    /// A recorded change under control taken for it. Control stays held
    /// until the command ends.
    pub(crate) fn held() -> Self {
        Self::new(Mode::Held, RUN_TIMEOUT)
    }

    /// A recorded change under controller ownership the caller already
    /// holds.
    pub(crate) fn owned(_ownership: &'a Ownership) -> Self {
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

/// What a caller decides by once the helper accepted a command: whether it
/// succeeded, whether its time ran out, and what it printed.
pub(crate) struct Finished {
    pub(crate) succeeded: bool,
    pub(crate) timed_out: bool,
    output: std::result::Result<Vec<u8>, OutputError>,
}

impl Finished {
    /// `reply` read once, keeping at most `limit` bytes of output.
    pub(crate) fn decode(reply: &Value, limit: usize) -> Self {
        Self {
            succeeded: reply["exitCode"] == 0,
            timed_out: reply["timedOut"] == true,
            output: run_output(reply, limit),
        }
    }

    /// What it printed; nothing when that could not be read.
    pub(crate) fn output(&self) -> &[u8] {
        self.output.as_deref().unwrap_or_default()
    }

    /// [`Finished::output`] as text.
    pub(crate) fn said(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(self.output())
    }

    /// Why its output could not be read, when it could not.
    pub(crate) fn unread(&self) -> Option<OutputError> {
        self.output.as_ref().err().copied()
    }
}

/// Whether the helper may run `argv`, stopping it after `timeout`: a program
/// and its arguments, none holding a NUL, for 1 second to 10 minutes.
fn check_run(argv: &[String], timeout: Duration) -> Result<()> {
    if argv.first().is_none_or(String::is_empty) || argv.iter().any(|a| a.contains('\0')) {
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

    Ok(())
}

impl Client {
    /// Run argv in the verified hidden helper as `run` says.
    pub(crate) async fn run(&self, argv: Vec<String>, run: Run<'_>) -> Result<Value> {
        check_run(&argv, run.timeout)?;
        let taken = match run.mode {
            Mode::Held => Some(self.controller.acquire(false)?),
            Mode::Beside => Some(self.controller.acquire_beside_commands()?),
            Mode::Owned | Mode::Pending | Mode::Inspect => None,
        };

        let pin = match run.pin {
            Some(verified) => Some(verified.pin()?),
            None => {
                self.identity().await?;
                None
            }
        };
        let mutation = Mutation::Run {
            argv,
            timeout: run.timeout,
            pin,
        };

        match run.mode {
            // Control taken for a held run stays held until the command
            // ends; one beside others lets go of it once its request is sent.
            Mode::Held | Mode::Owned => self.mutate(mutation, run.lease, None, None).await,
            Mode::Beside => self.mutate(mutation, None, None, taken).await,
            Mode::Pending => self.run_unrecorded(&mutation).await,
            Mode::Inspect => self.run_unrecorded(&mutation).await.map_err(Error::certain),
        }
    }

    /// Run argv in the verified hidden helper as an agent's own command,
    /// stopping it after `timeout`. Control is held only until the request
    /// reaches the helper; while the command runs, its claim keeps
    /// recovery, restart, update and log-out out, and other agents may act.
    /// Its outcome is pending until it ends.
    ///
    /// # Errors
    ///
    /// Rejects invalid arguments or timeout, unavailable ownership, changed identity or an
    /// unconfirmed reply. A failed or timed-out child can also be represented in the returned
    /// reply; unknown execution stays pending.
    pub async fn run_beside(&self, argv: Vec<String>, timeout: Duration) -> Result<Value> {
        self.run(argv, Run::new(Mode::Beside, timeout)).await
    }

    /// Execute argv in the verified hidden helper as a recorded mutation
    /// under controller ownership the caller already holds, stopping the
    /// command after `timeout`.
    ///
    /// # Errors
    ///
    /// Returns argument, identity, transport or completion failures under the supplied
    /// ownership. An uncertain result does not release the caller from recovery obligations.
    pub async fn run_owned_within(
        &self,
        ownership: &Ownership,
        argv: Vec<String>,
        timeout: Duration,
    ) -> Result<Value> {
        self.run(argv, Run::owned(ownership).within(timeout)).await
    }

    /// [`Client::run`] under `ownership`, refused unless the helper accepted
    /// the command, keeping at most `limit` bytes of its output.
    pub(crate) async fn run_finished(
        &self,
        ownership: &Ownership,
        argv: Vec<String>,
        limit: usize,
    ) -> Result<Finished> {
        self.run_finished_under(ownership, None, argv, limit).await
    }

    /// [`Client::run_finished`], sent only while `lease` still holds, when
    /// it names the person's lease and its token.
    pub(crate) async fn run_finished_under(
        &self,
        ownership: &Ownership,
        lease: Option<(&SharedLease, &str)>,
        argv: Vec<String>,
        limit: usize,
    ) -> Result<Finished> {
        let run = Run::owned(ownership).leased(lease);
        let reply = accepted(self.run(argv, run).await?)?;
        Ok(Finished::decode(&reply, limit))
    }

    /// Send the run `mutation` to the verified helper, recording nothing.
    /// The reply comes after the command ends, as for any other run.
    async fn run_unrecorded(&self, mutation: &Mutation<'_>) -> Result<Value> {
        let reply = self
            .transport_for(mutation)
            .call(&mutation.request())
            .await?;
        mutation.validate(&reply)?;

        accepted(reply)
    }
}
