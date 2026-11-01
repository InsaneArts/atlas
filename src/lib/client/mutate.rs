//! Changes sent to the helper as recorded mutations: refused while agents
//! are stopped, kept pending until a reply proves what they did, and cut
//! short by a stop. A reply's completion is checked against its request.
use super::{Actor, Client, record::recorded_code, run::RUN_MARGIN};
use crate::{
    activity_log::Op,
    control::{Lease, Ownership, Pending, SharedLease},
    daemon::protocol::{JobReply, JobState, JobView, StopCameTo, Stopped},
    error::{Code, Error, Result},
    transport,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use tokio::sync::MutexGuard;

pub fn validate_completion(request: &Value, reply: &Value) -> Result<()> {
    if reply["ok"] != true {
        return Ok(());
    }

    let valid = match Op::of(request) {
        // A checked batch is confirmed only with its expectation echoed: a
        // service that ignored it posted the input unchecked.
        Some(Op::Input) => {
            reply["performed"].as_u64() == request["actions"].as_array().map(|v| v.len() as u64)
                && (request["expect"].is_null() || reply["expect"] == request["expect"])
        }
        Some(Op::Job) => match (request["action"].as_str(), JobReply::deserialize(reply)) {
            (Some("start"), Ok(JobReply::Start { job })) => job_started(request, &job),
            (Some("start"), _) => false,
            (Some("stop"), Ok(JobReply::Stop(Stopped { job, came_to }))) => {
                let confirmed = match came_to {
                    StopCameTo::Signalled(signal) => signal == "SIGTERM",
                    StopCameTo::AlreadyEnded(ended) => ended,
                    StopCameTo::AlreadyStopping(stopping) => stopping,
                };
                request["id"] == job.id.as_str() && confirmed
            }
            (Some("stop"), _) => false,
            _ => true,
        },
        Some(Op::Run) => {
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
        Err(Error::new(
            Code::Protocol,
            "Helper did not confirm operation completion",
        )
        .uncertain())
    }
}

/// Whether a job start's reply proves the job it started: a leader that
/// is a group of its own, an exact start time, the boot it started in, the
/// helper account the request was for, and a state. Else a job may run
/// that the person's side knows nothing of.
fn job_started(request: &Value, job: &JobView) -> bool {
    job.pid.is_some_and(|pid| pid > 1)
        && job.pgid == job.pid
        && job
            .start
            .is_some_and(|[seconds, micros]| seconds > 0 && micros < 1_000_000)
        && !job.boot.is_empty()
        && request["uid"] == job.uid
        && matches!(
            job.state,
            JobState::Running | JobState::Stopping | JobState::Ended
        )
}

/// The person's lease, locked for only the final authorization and the
/// bounded sending of the request, once it still names `lease`'s owner and
/// the frame seen at `observed_at` is still fresh.
async fn authorize<'a>(
    lease: Option<(&'a SharedLease, &str)>,
    observed_at: Option<Instant>,
) -> Result<Option<MutexGuard<'a, Lease>>> {
    let mut authorization = match lease {
        Some((state, _)) => Some(state.lock().await),
        None => None,
    };

    if let (Some(state), Some((_, owner))) = (&mut authorization, lease) {
        state.require(owner)?;
    }

    if observed_at.is_some_and(|at| at.elapsed() >= Duration::from_secs(8)) {
        return Err(Error::new(
            Code::StaleFrame,
            "The observation expired before dispatch",
        ));
    }

    Ok(authorization)
}

impl Client {
    /// Send `request` to the helper, or to its input service for input, as
    /// a recorded mutation: refused while agents are stopped when it is one
    /// of theirs, carried out under its own claim unless its caller holds
    /// one, and settled only once its reply proves what it did. With
    /// `lease` the person's input is authorized under it, and with
    /// `observed_at` the frame it acts on must still be fresh.
    pub(super) async fn mutate(
        &self,
        request: Value,
        lease: Option<(&SharedLease, &str)>,
        observed_at: Option<Instant>,
        // Control to let go of once the request has reached the helper; its
        // claim is then marked as a running command.
        release: Option<Ownership>,
    ) -> Result<Value> {
        let op = Op::of(&request);
        debug_assert!(
            release.is_none() || op == Some(Op::Run),
            "only a command started with run lets go of control while it runs"
        );
        let started = Instant::now();
        self.controller.interrupt().check()?;
        let command = release.is_some();
        self.refuse_when_stopped(&request, op.as_ref())?;

        // Connect without holding the lease. Release can cancel slow preflight.
        let connection = self.connect(&request, op.as_ref()).await?;
        let authorization = authorize(lease, observed_at).await?;
        let mut claim = self.claim(op.as_ref(), lease.is_some(), command)?;
        let sent = self.send(connection, &request, claim.as_ref()).await?;

        drop(authorization);
        // A command an agent started lets go of control once its request is
        // sent: from here its claim, marked as a running command, keeps
        // recovery out until it ends, and other agents may act beside it.
        drop(release);

        let reply = if self.agents_command(op.as_ref()) {
            self.run_to_its_end(sent, &request, command, &mut claim, started)
                .await?
        } else {
            self.answer(sent).await?
        };
        validate_completion(&request, &reply)?;

        self.settle(&request, reply, command, claim, started).await
    }

    /// Whether `op` is an agent's own command, which a stop of agents
    /// refuses before it starts and cuts while it runs.
    fn agents_command(&self, op: Option<&Op>) -> bool {
        op == Some(&Op::Run) && self.actor == Actor::Agent && self.doing.is_some()
    }

    /// Refuse an agent's command that changes the helper, before it starts,
    /// once the person has stopped agents; reads and Atlas's own steps (no
    /// `doing`) go on.
    fn refuse_when_stopped(&self, request: &Value, op: Option<&Op>) -> Result<()> {
        let job_change = op == Some(&Op::Job)
            && matches!(request["action"].as_str(), Some("start" | "stop"))
            && self.actor == Actor::Agent
            && self.doing.is_some();
        if (self.agents_command(op) || job_change) && self.stop_marked() {
            return Err(Error::stopped());
        }

        Ok(())
    }

    /// A connection to where `request` goes: the input service for input,
    /// and the helper daemon for everything else.
    async fn connect(&self, request: &Value, op: Option<&Op>) -> Result<transport::Pending> {
        let mut transport = if op == Some(&Op::Input) {
            self.input_transport()
        } else {
            self.transport.clone()
        };

        // A long command's reply comes after it ends, and after the helper
        // has stopped whatever it left running.
        if let Some(run) = request["timeoutMs"]
            .as_u64()
            .filter(|_| op == Some(&Op::Run))
        {
            transport.timeout = transport
                .timeout
                .max(Duration::from_millis(run) + RUN_MARGIN);
        }

        transport.connect().await.map_err(crate::context::context(
            "Connect mutation service",
            &transport.path,
        ))
    }

    /// The claim `op` is carried out under, marked as a running command for
    /// a `command` that lets go of control. Restart, log-out and standalone
    /// input (input without a lease) run under the claim their caller holds
    /// through verification or cleanup, so they take none here.
    fn claim(&self, op: Option<&Op>, leased: bool, command: bool) -> Result<Option<Pending>> {
        let callers = op == Some(&Op::Restart)
            || op == Some(&Op::Logout)
            || (op == Some(&Op::Input) && !leased);
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

        match sent {
            Ok(sent) => Ok(sent),
            Err(error) => {
                if let Some(claim) = claim
                    && !error.uncertain
                {
                    self.controller.clear_if(claim)?;
                }
                Err(error)
            }
        }
    }

    /// The reply to a sent request, unless a signal's stop cuts it short
    /// once its grace is over.
    async fn answer(&self, sent: transport::Pending) -> Result<Value> {
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
            warnings
                .push("Operation result is valid; local metadata could not be recorded".to_owned());
        }

        if !warnings.is_empty() {
            reply["warnings"] = json!(warnings);
        }

        Ok(reply)
    }

    /// Send `request`, a change to the helper, as a recorded mutation under
    /// control the caller holds: it stays pending until its reply proves
    /// what it did.
    pub(crate) async fn change(&self, request: Value) -> Result<Value> {
        self.mutate(request, None, None, None).await
    }
}
