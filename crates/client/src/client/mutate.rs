//! Changes sent to the helper as recorded mutations: refused while agents
//! are stopped, kept pending until a reply proves what they did, and cut
//! short by a stop. A reply's completion is checked against its mutation.
use super::{
    Actor, Client,
    record::{NOT_RECORDED, recorded_code},
    run::RUN_MARGIN,
};
use crate::control::{Lease, Ownership, Pending, SharedLease};
use atlas_core::{
    actions::{Batch, Expect},
    error::{Code, Error, Result},
    protocol::{
        Expected, JobId, JobReply, JobStart, JobState, JobView, SessionPin, StopCameTo, Stopped,
    },
    session::InputEpoch,
    time::ms,
    transport::{self, Transport},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use tokio::sync::MutexGuard;

/// One change to the helper. Its wire request, completion proof, destination
/// and ownership all come from this one value, independently of the tolerant
/// activity-log vocabulary, so they cannot disagree.
#[derive(Debug)]
pub(crate) enum Mutation<'a> {
    Input {
        actions: Batch<'a>,
        owner: &'a str,
        epoch: InputEpoch,
        /// An agent's command names the app it acted in.
        app: bool,
        expect: Option<&'a Expect>,
    },
    Run {
        argv: Vec<String>,
        timeout: Duration,
        pin: Option<SessionPin>,
    },
    JobStart {
        start: JobStart,
        uid: u32,
        pin: SessionPin,
    },
    JobStop {
        id: &'a JobId,
        expected: &'a Expected,
        grace: Duration,
    },
    Restart,
    Logout {
        force: bool,
    },
}

impl Mutation<'_> {
    /// The request this mutation sends.
    pub(super) fn request(&self) -> Value {
        match self {
            Self::Input {
                actions,
                owner,
                epoch,
                app,
                expect,
            } => {
                let mut request = json!({
                    "op": "input",
                    "actions": actions.wire(),
                    "owner": owner,
                    "epoch": epoch,
                    "app": app,
                });
                if let Some(expect) = expect {
                    request["expect"] = json!(expect);
                }
                request
            }
            Self::Run { argv, timeout, pin } => {
                let mut request = json!({"op": "run", "argv": argv, "timeoutMs": ms(*timeout)});
                if let Some(pin) = pin {
                    request["session"] = json!(pin);
                }
                request
            }
            Self::JobStart { start, uid, pin } => {
                let mut request = json!({
                    "op": "job",
                    "action": "start",
                    "argv": start.argv,
                    "lifetimeMs": ms(start.lifetime),
                    "uid": uid,
                    "session": pin,
                });
                if let Some(name) = &start.name {
                    request["name"] = json!(name);
                }
                if let Some(task) = &start.task {
                    request["task"] = json!(task);
                }
                if let Some(session) = &start.session {
                    request["hostSession"] = json!(session);
                }
                request
            }
            Self::JobStop {
                id,
                expected,
                grace,
            } => json!({
                "op": "job",
                "action": "stop",
                "id": id,
                "expect": expected,
                "graceMs": ms(*grace),
            }),
            Self::Restart => json!({"op": "restart"}),
            Self::Logout { force } => json!({"op": "logout", "force": force}),
        }
    }

    pub(crate) fn validate(&self, reply: &Value) -> Result<()> {
        if reply["ok"] != true {
            return Ok(());
        }

        let valid = match self {
            // A checked batch is confirmed only with its expectation echoed: a
            // service that ignored it posted the input unchecked.
            Self::Input {
                actions, expect, ..
            } => {
                reply["performed"].as_u64() == Some(actions.actions().len() as u64)
                    && expect.is_none_or(|expect| reply["expect"] == json!(expect))
            }
            Self::JobStart { uid, .. } => {
                matches!(JobReply::deserialize(reply), Ok(JobReply::Start { job }) if job_started(*uid, &job))
            }
            Self::JobStop { id, .. } => match JobReply::deserialize(reply) {
                Ok(JobReply::Stop(Stopped { job, came_to })) => {
                    let confirmed = match came_to {
                        StopCameTo::Signalled(signal) => signal == "SIGTERM",
                        StopCameTo::AlreadyEnded(ended) => ended,
                        StopCameTo::AlreadyStopping(stopping) => stopping,
                    };
                    id.as_str() == job.id.as_str() && confirmed
                }
                _ => false,
            },
            Self::Run { .. } => {
                reply["timedOut"].is_boolean()
                    && reply.get("exitCode").is_some()
                    && (reply["exitCode"].is_i64()
                        || (reply["exitCode"].is_null()
                            && (reply["timedOut"] == true
                                || reply["signal"]
                                    .as_str()
                                    .is_some_and(|s| s.starts_with("SIG") && s.len() <= 32))))
            }
            Self::Restart | Self::Logout { .. } => true,
        };

        valid.then_some(()).ok_or_else(unconfirmed)
    }
}

/// A reply that does not prove what the helper did, which is then not known.
pub(crate) fn unconfirmed() -> Error {
    Error::new(
        Code::Protocol,
        "Helper did not confirm operation completion",
    )
    .uncertain()
}

/// Whether a job start's reply proves the job it started: a leader that
/// is a group of its own, an exact start time, the boot it started in, the
/// helper account `uid` it was for, and a state. Else a job may run that
/// the person's side knows nothing of.
fn job_started(uid: u32, job: &JobView) -> bool {
    job.pid.is_some_and(|pid| pid > 1)
        && job.pgid == job.pid
        && job
            .start
            .is_some_and(|[seconds, micros]| seconds > 0 && micros < 1_000_000)
        && !job.boot.is_empty()
        && uid == job.uid
        && match job.state {
            JobState::Running | JobState::Stopping | JobState::Ended => true,
            JobState::Starting | JobState::Launched => false,
        }
}

/// The person's lease, locked for only the final authorization and the
/// bounded sending of the request, once it still names `lease`'s owner and
/// the frame seen at `observed_at` is still fresh.
async fn authorize<'a>(
    lease: Option<(&'a SharedLease, &str)>,
    observed_at: Option<Instant>,
) -> Result<Option<MutexGuard<'a, Lease>>> {
    let authorization = match lease {
        Some((state, owner)) => {
            let mut state = state.lock().await;
            state.require(owner)?;
            Some(state)
        }
        None => None,
    };

    if observed_at.is_some_and(|at| at.elapsed() >= Duration::from_secs(8)) {
        return Err(Error::stale_frame());
    }

    Ok(authorization)
}

impl Client {
    /// Send `mutation` to the helper, or to its input service for input, as
    /// a recorded mutation: refused while agents are stopped when it is one
    /// of theirs, carried out under its own claim unless its caller holds
    /// one, and settled only once its reply proves what it did. With
    /// `lease` the person's input is authorized under it, and with
    /// `observed_at` the frame it acts on must still be fresh.
    pub(crate) async fn mutate(
        &self,
        mutation: Mutation<'_>,
        lease: Option<(&SharedLease, &str)>,
        observed_at: Option<Instant>,
        // Control to let go of once the request has reached the helper; its
        // claim is then marked as a running command.
        release: Option<Ownership>,
    ) -> Result<Value> {
        debug_assert!(
            release.is_none() || matches!(mutation, Mutation::Run { .. }),
            "only a command started with run lets go of control while it runs"
        );
        let started = Instant::now();
        self.controller.interrupt().check()?;
        let command = release.is_some();
        let agent = self.actor == Actor::Agent && self.doing.is_some();
        let (agent_command, stop_before) = match mutation {
            Mutation::Run { .. } => (agent, agent),
            Mutation::JobStart { .. } | Mutation::JobStop { .. } => (false, agent),
            Mutation::Input { .. } | Mutation::Restart | Mutation::Logout { .. } => (false, false),
        };
        if stop_before && self.stop_marked() {
            return Err(Error::stopped());
        }

        // Connect without holding the lease. Release can cancel slow preflight.
        let connection = self.connect(&mutation).await?;
        let authorization = authorize(lease, observed_at).await?;
        let mut claim = self.claim(&mutation, lease.is_some(), command)?;
        let request = mutation.request();
        let sent = self.send(connection, &request, claim.as_ref()).await?;

        drop(authorization);
        // A command an agent started lets go of control once its request is
        // sent: from here its claim, marked as a running command, keeps
        // recovery out until it ends, and other agents may act beside it.
        drop(release);

        let reply = if agent_command {
            self.run_to_its_end(sent, &request, command, &mut claim, started)
                .await?
        } else {
            self.answer(sent).await?
        };
        mutation.validate(&reply)?;

        self.settle(&request, reply, command, claim, started).await
    }

    /// Where `mutation` goes: the input service for input, and the helper
    /// daemon for everything else.
    pub(super) fn transport_for(&self, mutation: &Mutation<'_>) -> Transport {
        match mutation {
            Mutation::Input { .. } => self.input_transport(),
            // A long command's reply comes after it ends, and after the
            // helper has stopped whatever it left running.
            Mutation::Run { timeout, .. } => {
                let mut transport = self.transport.clone();
                transport.timeout = transport.timeout.max(*timeout + RUN_MARGIN);
                transport
            }
            Mutation::JobStart { .. }
            | Mutation::JobStop { .. }
            | Mutation::Restart
            | Mutation::Logout { .. } => self.transport.clone(),
        }
    }

    async fn connect(&self, mutation: &Mutation<'_>) -> Result<transport::Pending> {
        let transport = self.transport_for(mutation);
        transport
            .connect()
            .await
            .map_err(atlas_core::context::context(
                "Connect mutation service",
                &transport.path,
            ))
    }

    /// The claim the mutation is carried out under, marked as a running command for
    /// a `command` that lets go of control. Restart, log-out and standalone
    /// input (input without a lease) run under the claim their caller holds
    /// through verification or cleanup, so they take none here.
    fn claim(
        &self,
        mutation: &Mutation<'_>,
        leased: bool,
        command: bool,
    ) -> Result<Option<Pending>> {
        let callers = match mutation {
            Mutation::Restart | Mutation::Logout { .. } => true,
            Mutation::Input { .. } => !leased,
            Mutation::Run { .. } | Mutation::JobStart { .. } | Mutation::JobStop { .. } => false,
        };
        if callers {
            return Ok(None);
        }

        let claim = if command {
            self.controller.quarantine_command()
        } else {
            self.controller.quarantine()
        };
        claim.map(Some)
    }

    /// Send `request` over `connection`, the last moment a stop refuses it:
    /// it is sent whole, or not at all, so its `claim` is settled whenever
    /// it is not sent.
    async fn send(
        &self,
        connection: transport::Pending,
        request: &Value,
        claim: Option<&Pending>,
    ) -> Result<transport::Pending> {
        let interrupt = self.controller.interrupt();
        let sent = {
            let _sending = interrupt.sending();
            match interrupt.check() {
                Ok(()) => connection.dispatch(request).await,
                Err(stop) => Err(stop),
            }
        };

        if let (Err(error), Some(claim)) = (&sent, claim)
            && !error.uncertain
        {
            self.controller.clear_if(claim)?;
        }

        sent
    }

    /// The reply to a sent request, unless a signal's stop cuts it short
    /// once its grace is over.
    pub(super) async fn answer(&self, sent: transport::Pending) -> Result<Value> {
        let interrupt = self.controller.interrupt();
        tokio::select! {
            biased;
            reply = sent.finish() => reply,
            signal = interrupt.cut() => Err(interrupt.cut_short(signal)),
        }
    }

    /// The reply to an agent's sent command, which is written as started
    /// first. It runs until it ends, unless the person stops agents: then
    /// the connection is dropped, which ends the command in the helper. What
    /// it did by then is unknown, so it stays pending. A command that
    /// started ends with a row too, however it ends. A signal's stop cuts a
    /// command still unanswered after its grace the same way.
    async fn run_to_its_end(
        &self,
        sent: transport::Pending,
        request: &Value,
        command: bool,
        claim: &mut Option<Pending>,
        started: Instant,
    ) -> Result<Value> {
        let interrupt = self.controller.interrupt();
        let _ = self.record_start(request).await;
        let ended = tokio::select! {
            biased;
            reply = sent.finish() => reply,
            () = self.stopped_while_running() => {
                Err(Error::stopped().uncertain())
            }
            signal = interrupt.cut() => Err(interrupt.cut_short(signal)),
        };

        let Err(error) = ended else {
            return ended;
        };

        // Its outcome is unknown from here: it stops counting as a running
        // command before its row is written, which may wait for another
        // command's.
        let _recording = interrupt.step();
        if command && let Some(claim) = claim.take() {
            self.controller.abandon_command(claim);
        }
        let failed = json!({"ok": false, "code": recorded_code(&error)});
        let _ = self.record(request, &failed, started.elapsed()).await;
        Err(error)
    }

    /// Settle `request` once its `reply` is confirmed: a command whose
    /// helper could not say what it did is unknown from here, before its row
    /// is written, and any other outcome the helper is sure of clears its
    /// claim. What could not be cleared or recorded is said in warnings.
    async fn settle(
        &self,
        request: &Value,
        mut reply: Value,
        command: bool,
        mut claim: Option<Pending>,
        started: Instant,
    ) -> Result<Value> {
        let unknown = command && reply["uncertain"] == true;
        let _recording = unknown.then(|| self.controller.interrupt().step());
        if unknown && let Some(claim) = claim.take() {
            self.controller.abandon_command(claim);
        }

        let mut warnings = Vec::new();
        if let Some(claim) = &claim
            && reply["uncertain"] != true
            && let Err(error) = self.controller.clear_if(claim)
        {
            warnings.push(format!(
                "Operation completed, but its pending marker could not be cleared: {error}"
            ));
        }

        if self
            .record(request, &reply, started.elapsed())
            .await
            .is_err()
        {
            warnings.push(NOT_RECORDED.to_owned());
        }

        if !warnings.is_empty() {
            reply["warnings"] = json!(warnings);
        }

        Ok(reply)
    }
}

#[cfg(test)]
mod tests;
