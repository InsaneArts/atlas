//! A VM desktop started when a command needs it running: an agent's `run`,
//! a job's start, `open` and input ask for it, and nothing else does. The
//! person allowed this for every VM once ("start when needed and keep
//! running"), so nothing asks them.
//!
//! The station's Stop holds it before anything is looked at. A VM still
//! being added or removed, past an uninstall's barrier, or being handed over
//! by an update, is never started. Atlas VM is asked to start it only once
//! it is confirmed stopped, one start at a time, under the VM's
//! station-operation lock: each start is kept in the VM's lifecycle record,
//! then claimed as Atlas VM's, then asked, and its claim is cleared only on
//! Atlas VM's own word of what it came to, whichever command asked it. One
//! Atlas VM refused is not asked again for a minute.
//!
//! A guest started this way lost its session with the station, so its saved
//! Stop holds agents until the person resumes them: the command's own
//! admission says so, in the guest's words.
use super::{
    Vms, handover, hold_in,
    lifecycle::{
        Lifecycle,
        record::{Record, RefusedStart, State},
    },
    runner::{Operation, Outcome, Power, Reply, Runner},
};
use crate::client::Client;
use atlas_control::{Controller, Pending, executors::Executor};
use atlas_core::{
    config::desktops::Chosen,
    desktop::DesktopId,
    error::{Code, Error, Result},
    files::Token,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// How long a command waits for its VM to run, its keeper to hold a session
/// to its guest, and the guest to prove itself.
const WITHIN: Duration = Duration::from_secs(180);
/// How often what is waited for is looked at again.
const EVERY: Duration = Duration::from_secs(1);
/// How long a start Atlas VM refused keeps another from being asked.
const BACK_OFF: Duration = Duration::from_secs(60);
/// How long Atlas VM is given to answer a read.
const READING: Duration = Duration::from_secs(10);
/// How long the pending changes' lock is waited for when a start's claim is
/// cleared.
const CLEARING: Duration = Duration::from_secs(5);

/// How long a start is waited for, and how often it is looked at.
#[derive(Clone, Copy, Debug)]
struct Patience {
    within: Duration,
    every: Duration,
}

impl Patience {
    const REAL: Self = Self {
        within: WITHIN,
        every: EVERY,
    };
}

/// A start a command asked of Atlas VM, which came to the VM running.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Started {
    vm: String,
    change: Token,
}

impl Started {
    /// The start as a reply says it.
    #[must_use]
    pub fn said(&self) -> Value {
        json!({"desktop": self.vm, "change": self.change})
    }

    /// `error`, which came after this start, saying it.
    #[must_use]
    pub fn onto(&self, error: Error) -> Error {
        error.with("started", self.said())
    }
}

/// Make sure the VM `client` acts on runs, starting it when it is stopped:
/// for the commands that need it running alone. Second Login, and a VM
/// running already, are let through as they are.
///
/// # Errors
///
/// Refuses while the station's Stop holds agents, for a VM still being
/// added or removed, past an uninstall's barrier or handed over by an
/// update, one Atlas VM cannot run, and one whose start Atlas VM refused a
/// moment ago, in its own words; fails when it is not running, its keeper
/// holding a session and its guest proved, within about three minutes, and
/// when a start's answer cannot be kept.
pub async fn ensure_running(client: &Client) -> Result<Option<Started>> {
    ensure_running_within(client, Patience::REAL).await
}

async fn ensure_running_within(client: &Client, patience: Patience) -> Result<Option<Started>> {
    let Chosen::Vm(config) = &client.selected.desktop else {
        return Ok(None);
    };
    if client.stop_marked() {
        return Err(client.stopped());
    }
    client.reachable()?;
    let lifecycle = client
        .lifecycle
        .clone()
        .ok_or_else(|| client.unreachable())?;
    let began = Instant::now();
    let deadline = began + patience.within;
    let starter = Starter {
        vms: lifecycle.vms(),
        lifecycle,
        vm: config.id.clone(),
        name: client.selected.name.clone(),
        controller: client.controller.clone(),
        stopped: client.stopped().error,
        patience,
    };
    let came = tokio::task::spawn_blocking(move || starter.running(deadline))
        .await
        .map_err(|_| Error::new(Code::State, "The VM's start could not be looked after."))??;
    let started = came.started.map(|change| Started {
        vm: client.selected.name.clone(),
        change,
    });
    if let Some(started) = &started {
        // Its row says the VM was started, not what the command does.
        let mut logging = client.clone();
        logging.doing = None;
        let _ = logging
            .record(
                &json!({"op": "start"}),
                &json!({"ok": true, "started": started.said()}),
                began.elapsed(),
            )
            .await;
    }
    if (came.waited || started.is_some())
        && let Err(error) = session_proven(client, deadline, patience.every).await
    {
        return Err(match &started {
            Some(started) => started.onto(error),
            None => error,
        });
    }
    Ok(started)
}

/// How long a status gives Atlas VM to say the VM's power.
const STATUS_READ: Duration = Duration::from_secs(2);

/// Why `client`'s VM proves nothing now, when Atlas VM confirms it is
/// stopped: what a status and doctor say in place of the failed connection
/// to its guest. None for anything else.
pub(crate) async fn stopped_now(client: &Client) -> Option<Error> {
    let Chosen::Vm(config) = &client.selected.desktop else {
        return None;
    };
    let runner = Runner::in_vms(&client.lifecycle.as_ref()?.vms(), &config.id);
    let reply =
        tokio::task::spawn_blocking(move || runner.read_within(&Operation::State, STATUS_READ))
            .await
            .ok()?
            .ok()?;
    (reply.ok && reply.confirmed == Some(true) && reply.power == Some(Power::Stopped)).then(|| {
        Error::new(
            Code::NotReady,
            format!(
                "{} is stopped. A command that needs it, such as run, starts it.",
                client.selected.name
            ),
        )
    })
}

/// Wait, until `deadline`, for `client`'s VM, now running, to be held by its
/// keeper and proved, `every` apart.
async fn session_proven(client: &Client, deadline: Instant, every: Duration) -> Result<()> {
    loop {
        if client.proven_now().await {
            return Ok(());
        }
        if Instant::now() + every > deadline {
            return Err(Error::new(
                Code::NotReady,
                format!(
                    "{} runs, but its session with this Mac is not proven yet. Run the command \
                     again in a moment.",
                    client.selected.name
                ),
            ));
        }
        tokio::time::sleep(every).await;
    }
}

/// What a command's wait for its VM came to.
#[derive(Debug, Default)]
struct Came {
    /// The start it asked, which came to the VM running.
    started: Option<Token>,
    /// Whether it waited for the VM at all, which then may still be coming
    /// up.
    waited: bool,
}

/// What one look at the VM found.
enum Look {
    /// It runs, and no start is unknown.
    Running,
    /// A start was asked just now: looked at again at once.
    Again,
    /// Not yet, for this reason.
    Wait(String),
}

/// What a start asked of Atlas VM came to.
enum Asked {
    /// Its own answer, kept by Atlas VM for its change.
    Came(Outcome),
    /// Its only submission was not taken, for this reason: nothing was
    /// begun, and nothing else asked it.
    NotTaken(String),
    /// Held, by the person's Stop or a signal's stop of this command, as
    /// `why` says, before it was submitted (again); `asked` when it may have
    /// been submitted before.
    Held { asked: bool, why: Error },
    /// Not known yet.
    Unknown,
}

/// One command's start of one VM.
struct Starter {
    vms: Vms,
    lifecycle: Lifecycle,
    vm: DesktopId,
    name: String,
    controller: Controller,
    /// What the person's Stop on the VM says.
    stopped: String,
    patience: Patience,
}

impl Starter {
    /// Look at the VM, starting it when it is confirmed stopped, until it
    /// runs or `deadline` passes.
    fn running(&self, deadline: Instant) -> Result<Came> {
        let mut came = Came::default();
        loop {
            match self.look(&mut came, deadline)? {
                Look::Running => return Ok(came),
                Look::Again => {}
                Look::Wait(why) => {
                    came.waited = true;
                    if Instant::now() + self.patience.every > deadline {
                        let late = Error::new(
                            Code::NotReady,
                            format!(
                                "{} is not running yet. {why} Run the command again in a moment.",
                                self.name
                            ),
                        );
                        return Err(self.after(&came, late));
                    }
                    std::thread::sleep(self.patience.every);
                }
            }
        }
    }

    /// `error`, saying the start `came` asked, when it asked one.
    fn after(&self, came: &Came, error: Error) -> Error {
        match &came.started {
            Some(change) => Started {
                vm: self.name.clone(),
                change: change.clone(),
            }
            .onto(error),
            None => error,
        }
    }

    /// One look: the VM let through when it runs and no start of it is
    /// unknown; else, under its station-operation lock and then its
    /// exclusion (which an update's handover holds), every start kept for it
    /// looked after and, once it is confirmed stopped, a new one asked.
    fn look(&self, came: &mut Came, deadline: Instant) -> Result<Look> {
        let admitted = self.lifecycle.admit()?;
        let record = self.created()?;
        let power = self.power();
        if record.starts.is_empty() && running(&power) {
            return Ok(Look::Running);
        }
        let Some(_hold) = hold_in(&self.vms, &self.vm, &admitted)? else {
            return Ok(Look::Wait(format!(
                "Another command is starting {} or changing it.",
                self.name
            )));
        };
        let Some(_exclusion) = self.exclusion()? else {
            return Ok(Look::Wait(format!(
                "An update or another Atlas command holds {}.",
                self.name
            )));
        };
        // Looked at again under both: a removal or a handover that began
        // meanwhile is seen now, and none begins until both are let go.
        let mut record = self.created()?;
        if let Some(why) = self.recovered(&mut record, came, deadline)? {
            return Ok(Look::Wait(why));
        }
        let reply = match self.power() {
            Ok(reply) if reply.ok && reply.confirmed == Some(true) => reply,
            Ok(_) => {
                return Ok(Look::Wait(format!(
                    "Atlas VM cannot tell yet whether {} runs.",
                    self.name
                )));
            }
            Err(_) => {
                return Ok(Look::Wait(format!(
                    "Atlas VM, which runs {}, is not answering.",
                    self.name
                )));
            }
        };
        match reply.power {
            Some(Power::Running) => Ok(Look::Running),
            Some(Power::Stopped) if came.started.is_some() => {
                let reason = format!("{} stopped again once it started.", self.name);
                self.refused(
                    &mut record,
                    came.started.clone().unwrap_or_default(),
                    &reason,
                )?;
                Err(self.after(came, refused(&self.name, &reason)))
            }
            Some(Power::Stopped) => {
                self.not_refused_lately(&record)?;
                // The person's Stop, or a signal's stop of this command,
                // given while it waited holds it before anything is begun.
                if let Some(why) = self.held() {
                    return Err(why);
                }
                // Too little time left to ask and follow a start.
                if left(deadline) <= self.patience.every * 2 {
                    return Ok(Look::Wait(format!("{} is stopped.", self.name)));
                }
                match self.start(&mut record, deadline)? {
                    Ok(change) => {
                        came.started = Some(change);
                        Ok(Look::Again)
                    }
                    Err(why) => Ok(Look::Wait(why)),
                }
            }
            Some(Power::Absent) => Err(Error::new(
                Code::State,
                format!(
                    "{}'s VM is not made, so Atlas VM cannot start it.",
                    self.name
                ),
            )
            .with(
                "next",
                json!(format!(
                    "Remove it with atlas desktop remove {}, then add it again with atlas \
                     desktop add vm.",
                    self.name
                )),
            )),
            Some(Power::Starting) => Ok(Look::Wait(format!("{} is starting.", self.name))),
            Some(Power::Stopping) => Ok(Look::Wait(format!("{} is stopping.", self.name))),
            Some(Power::Unknown) | None => Ok(Look::Wait(format!(
                "Atlas VM cannot tell yet whether {} runs.",
                self.name
            ))),
        }
    }

    /// The VM's exclusion, which an update holds while it hands the VM's
    /// session over; none while another holds it.
    fn exclusion(&self) -> Result<Option<atlas_control::Ownership>> {
        match self
            .controller
            .clone()
            .waiting(Duration::ZERO)
            .acquire_for_executor()
        {
            Ok(exclusion) => Ok(Some(exclusion)),
            Err(error) if error.code == Code::Busy => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Why no start may be begun or sent now: the person's Stop on the VM,
    /// or a signal that asked this command to stop.
    fn held(&self) -> Option<Error> {
        if self.controller.stop_noted() {
            return Some(Error::new(Code::Stopped, self.stopped.clone()));
        }
        self.controller.interrupt().check().err()
    }

    /// The VM's lifecycle record, once it is added and nothing removes it
    /// or hands its session over.
    fn created(&self) -> Result<Record> {
        let removing = || {
            Error::new(
                Code::State,
                format!("{} is being removed, so it is not started.", self.name),
            )
        };
        let record = match self.lifecycle.record(&self.vm)? {
            Some(record) if record.state == State::Created => record,
            Some(record) if record.state == State::Removing => return Err(removing()),
            Some(_) => {
                return Err(Error::new(
                    Code::NotReady,
                    format!("{} is still being added, so it is not started.", self.name),
                ));
            }
            None => {
                return Err(Error::new(
                    Code::State,
                    format!(
                        "No VM named {} is kept on this Mac, so none is started.",
                        self.name
                    ),
                ));
            }
        };
        if record.removal.is_some() || self.lifecycle.recorded_removal(&self.vm)?.is_some() {
            return Err(removing());
        }
        let station = crate::station::Station::under(self.lifecycle.home()).path_of(&self.vm);
        if let Some(why) = handover::unfinished_in(&station, &self.name) {
            return Err(Error::new(Code::Stopped, why));
        }
        Ok(record)
    }

    /// The VM's power, as Atlas VM says it now.
    fn power(&self) -> Result<Reply> {
        self.runner().read_within(&Operation::State, READING)
    }

    fn runner(&self) -> Runner {
        Runner::in_vms(&self.vms, &self.vm)
    }

    /// The claim on the start `change`, as Atlas VM's, the marker's lock
    /// waited for no longer than `deadline` leaves; none while it stays
    /// busy, with nothing recorded.
    fn claim(&self, change: &Token, deadline: Instant) -> Result<Option<Pending>> {
        match self.controller.quarantine_by_within(
            change.as_str(),
            Executor::Vm,
            left(deadline).min(CLEARING),
        ) {
            Ok(claim) => Ok(Some(claim)),
            Err(error) if error.code == Code::Busy => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Look after every start kept in `record`, as Atlas VM answers what
    /// each came to: one it answered is cleared, a refusal's minute begun
    /// first; one it never heard of is claimed again and asked again as the
    /// very same start; one it has not finished is kept. Why the VM is
    /// waited for, while a start is kept unknown.
    fn recovered(
        &self,
        record: &mut Record,
        came: &mut Came,
        deadline: Instant,
    ) -> Result<Option<String>> {
        let mut unknown = false;
        for change in record.starts.clone() {
            let answer = self
                .runner()
                .read_within(&Operation::Outcome(change.clone()), READING);
            let mut claim = None;
            let asked = match answer {
                Ok(reply) if reply.ok && reply.change.as_ref() == Some(&change) => {
                    match (reply.outcome, reply.pending) {
                        (Some(outcome), _) => Asked::Came(outcome),
                        (None, Some(true)) => Asked::Unknown,
                        // Never heard of: its earlier request may still be
                        // on its way, so it is asked again as the very same
                        // start, claimed first.
                        (None, _) => match self.claim(&change, deadline)? {
                            Some(held) => {
                                claim = Some(held);
                                self.ask(record, &change, true, deadline)?
                            }
                            None => Asked::Unknown,
                        },
                    }
                }
                Ok(_) | Err(_) => Asked::Unknown,
            };
            let asked_again = claim.is_some();
            match asked {
                // Left for whichever command looks next.
                Asked::Unknown | Asked::NotTaken(_) => unknown = true,
                Asked::Held { why, .. } => return Err(why),
                Asked::Came(Outcome::Done) => {
                    self.settle(record, &change, claim, deadline)?;
                    if asked_again {
                        came.started = Some(change);
                    }
                }
                Asked::Came(Outcome::Refused { reason }) => {
                    // Its time not known: its minute begins now, kept
                    // before the start is forgotten.
                    self.refused(record, change.clone(), &reason)?;
                    self.settle(record, &change, claim, deadline)?;
                    if asked_again {
                        return Err(refused(&self.name, &reason));
                    }
                }
            }
        }
        Ok(unknown.then(|| {
            format!(
                "Atlas VM has not said yet what an earlier start of {} came to.",
                self.name
            )
        }))
    }

    /// Ask Atlas VM to start the VM, a new start kept in `record` first,
    /// then claimed as Atlas VM's: the start, once it is done; why the VM is
    /// waited for when Atlas VM did not take it.
    fn start(
        &self,
        record: &mut Record,
        deadline: Instant,
    ) -> Result<std::result::Result<Token, String>> {
        let change = Token::new();
        record.starts.push(change.clone());
        self.lifecycle.keep_record(record)?;
        // Never claimed, so never asked: forgotten.
        let unclaimed = |record: &mut Record| {
            record.start_settled(&change);
            self.lifecycle.keep_record(record)
        };
        let claim = match self.claim(&change, deadline) {
            Ok(Some(claim)) => claim,
            Ok(None) => {
                unclaimed(record)?;
                return Ok(Err(format!(
                    "The changes pending on {} are busy.",
                    self.name
                )));
            }
            Err(error) => {
                let _ = unclaimed(record);
                return Err(error);
            }
        };
        match self.ask(record, &change, false, deadline)? {
            Asked::Came(Outcome::Done) => {
                self.settle(record, &change, Some(claim), deadline)?;
                if record.refused_start.take().is_some() {
                    self.lifecycle.keep_record(record)?;
                }
                Ok(Ok(change))
            }
            Asked::Came(Outcome::Refused { reason }) => {
                self.refused(record, change.clone(), &reason)?;
                self.settle(record, &change, Some(claim), deadline)?;
                Err(refused(&self.name, &reason))
            }
            Asked::NotTaken(why) => {
                self.settle(record, &change, Some(claim), deadline)?;
                Ok(Err(why))
            }
            // Never sent: cleared. Maybe sent before: kept, claim and all.
            Asked::Held { asked, why } => {
                if !asked {
                    self.settle(record, &change, Some(claim), deadline)?;
                }
                Err(why)
            }
            // Left for whichever command looks next, the start kept with it.
            Asked::Unknown => Err(Error::new(
                Code::NotReady,
                format!(
                    "{} is not running yet. Atlas VM has not said what its start came to. It \
                     stays pending until it does; run the command again in a moment.",
                    self.name
                ),
            )),
        }
    }

    /// Ask Atlas VM for the start `change`, kept in `record` and claimed,
    /// and follow it until Atlas VM keeps an outcome for it or `deadline`
    /// passes. `asked` when it may have been submitted before. The person's
    /// Stop is looked at right before each submission. A lost reply is
    /// followed by what Atlas VM says it came to, and the very same start is
    /// asked again when Atlas VM never heard of it. A refusal unkept is
    /// taken as the start's only while nothing else may have asked it: else
    /// an earlier request may still be on its way, and only an outcome
    /// Atlas VM keeps for the change settles it.
    fn ask(
        &self,
        record: &mut Record,
        change: &Token,
        asked: bool,
        deadline: Instant,
    ) -> Result<Asked> {
        let runner = self.runner();
        let operation = Operation::Start(change.clone());
        let lifecycle = &self.lifecycle;
        let mut uncertain = asked;
        let mut asking = true;
        loop {
            if asking {
                asking = false;
                if let Some(why) = self.held() {
                    return Ok(Asked::Held {
                        asked: uncertain,
                        why,
                    });
                }
                let answer = runner.change_within(
                    record,
                    |record| lifecycle.keep_record(record),
                    &operation,
                    left(deadline),
                );
                match answer {
                    Ok(reply) if reply.ok => {
                        if let Some(outcome) = reply.outcome {
                            return Ok(Asked::Came(outcome));
                        }
                        uncertain = true;
                    }
                    Ok(reply) if !uncertain => {
                        return Ok(Asked::NotTaken(
                            reply
                                .error
                                .unwrap_or_else(|| "Atlas VM did not take the start.".to_owned()),
                        ));
                    }
                    // Refused while an earlier request may still be on its
                    // way: followed until Atlas VM keeps an outcome.
                    Ok(_) => {}
                    // Its reply lost: what it came to is asked.
                    Err(error) if error.code == Code::Transport => uncertain = true,
                    Err(error) => return Err(error),
                }
            }
            if Instant::now() + self.patience.every > deadline {
                return Ok(Asked::Unknown);
            }
            std::thread::sleep(self.patience.every);
            match runner.read_within(&Operation::Outcome(change.clone()), READING) {
                Ok(reply) if reply.ok && reply.change.as_ref() == Some(change) => {
                    match (reply.outcome, reply.pending) {
                        (Some(outcome), _) => return Ok(Asked::Came(outcome)),
                        (None, Some(true)) => {}
                        (None, _) => asking = true,
                    }
                }
                Ok(_) | Err(_) => {}
            }
        }
    }

    /// Clear the claim on the start `change`, then forget it in `record`:
    /// kept, claim and all, when the claim cannot be cleared, a marker that
    /// cannot be read included, within what `deadline` leaves.
    fn settle(
        &self,
        record: &mut Record,
        change: &Token,
        claim: Option<Pending>,
        deadline: Instant,
    ) -> Result<()> {
        let wait = left(deadline).clamp(self.patience.every, CLEARING);
        if let Err(error) = self.controller.clear_known_within(change.as_str(), wait) {
            return Err(Error::new(
                Code::State,
                format!(
                    "Atlas VM answered {}'s start, but its pending claim could not be cleared, so \
                     the start is kept: {}",
                    self.name, error.error
                ),
            )
            .with(
                "next",
                json!("The next command that needs the VM looks after it again."),
            ));
        }
        drop(claim);
        record.start_settled(change);
        self.lifecycle.keep_record(record)
    }

    /// Keep that Atlas VM refused the start `change` for `reason`, so none
    /// is asked again for a minute from now.
    fn refused(&self, record: &mut Record, change: Token, reason: &str) -> Result<()> {
        record.refused_start = Some(RefusedStart {
            change,
            reason: reason.to_owned(),
            at_ms: atlas_core::time::now_ms(),
        });
        self.lifecycle.keep_record(record)
    }

    /// Refuse while the last start Atlas VM refused was a moment ago.
    fn not_refused_lately(&self, record: &Record) -> Result<()> {
        let Some(last) = &record.refused_start else {
            return Ok(());
        };
        let since = atlas_core::time::now_ms().saturating_sub(last.at_ms);
        let back_off = u64::try_from(BACK_OFF.as_millis()).unwrap_or(u64::MAX);
        if since >= back_off {
            return Ok(());
        }
        Err(refused(&self.name, &last.reason).with(
            "next",
            json!(format!(
                "Atlas asks again once a minute has passed since, in {} s.",
                (back_off - since).div_ceil(1000)
            )),
        ))
    }
}

/// What is left before `deadline`.
fn left(deadline: Instant) -> Duration {
    deadline.saturating_duration_since(Instant::now())
}

/// Whether `power`, as Atlas VM said it, is the VM confirmed running.
fn running(power: &Result<Reply>) -> bool {
    power.as_ref().is_ok_and(|reply| {
        reply.ok && reply.confirmed == Some(true) && reply.power == Some(Power::Running)
    })
}

/// Atlas VM's refusal to start the VM `name`, for `reason` in its own words.
fn refused(name: &str, reason: &str) -> Error {
    Error::new(
        Code::State,
        format!("Atlas VM refused to start {name}: {reason}"),
    )
}

#[cfg(test)]
#[path = "start_tests.rs"]
mod tests;
