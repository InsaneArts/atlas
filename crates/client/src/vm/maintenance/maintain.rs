//! `atlas desktop maintain <name> [--restart] [--end]`: the person's
//! maintenance of a VM desktop begun and ended, each once the person allows
//! it in Atlas.app.
//!
//! Begun on a running VM whose guest says no work agents started runs
//! there, or on one confirmed stopped, which runs none, under its
//! station-operation lock and then its exclusion, both held to the last
//! step. With `--restart`, a running VM is stopped and started again first,
//! whatever its guest says, ending any work there; its exclusion is then
//! taken whatever is pending, and no claim of the guest's is ever cleared.
//! The maintenance is kept first, so every agent's admission refuses from
//! then: at `stopping` for a restart, `starting` for a stopped VM, else
//! `holding`. Each stop and start is a change of its own, kept, claimed as
//! Atlas VM's and asked, and the next phase begins only once Atlas VM
//! confirms the VM stopped, or running. Then its keeper is asked to hold
//! every agent's lane, and Atlas VM is asked to open the window with input
//! only once the keeper said each lane ended. The window's change is kept,
//! claimed as Atlas VM's and asked; its claim is cleared only on Atlas VM's
//! own word, and the run Atlas VM's ledger says did it is kept as its
//! opener.
//!
//! Ended under the same locks, its exclusion taken whatever is pending, and
//! only for the very maintenance the person allowed ending, or one already
//! ending. Its window-on is settled first for good: Atlas VM answers what it
//! came to, or, never having seen it, voids it, so a request of it still on
//! its way is never done; so are its stop and start. Only then is Atlas VM
//! asked to turn the window's input off, under a change kept after that
//! settlement, and the maintenance ends only on the word of the run that
//! opened the window, of any run when none did, or of a later run that
//! turned its own window's input off and says every earlier run's VM, the
//! opener's among them, is gone. That proof is kept first; the keeper is
//! told to let agents in again, and the maintenance is forgotten only once
//! it did, or no keeper is there to hold them.
//!
//! Every phase goes on from its record when asked again: a change kept and
//! not answered is sent again under the same ID, which Atlas VM's ledger
//! answers from its record, so no window is opened twice.
mod power;
mod proof;

use super::Kept;
use crate::{
    client::{Client, Leftovers},
    link::{self, Link},
    vm::{
        Vms, handover, hold_in,
        lifecycle::{
            Lifecycle,
            grant::{self, Kind},
            record::{Ended, Maintenance, Phase, Record, State},
        },
        runner::{Operation, Outcome, Power, Reply, Runner},
    },
};
use atlas_control::{Controller, Ownership, Pending, executors::Executor};
use atlas_core::{
    config::desktops::{Chosen, Selected},
    desktop::DesktopId,
    error::{Code, Error, Result},
    files::Token,
};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

/// How long Atlas VM is given to answer a window change, the outcome it
/// is followed by included.
const ANSWERING: Duration = Duration::from_secs(60);
/// How long Atlas VM is given to stop or start the VM and confirm it: it
/// gives the guest a minute to shut down before it stops it outright.
const POWERING: Duration = Duration::from_secs(180);
/// How often a change whose reply was lost is looked at again.
const EVERY: Duration = Duration::from_secs(1);
/// How long Atlas VM is given to answer a read.
const READING: Duration = Duration::from_secs(10);
/// How long the keeper is given to hold every agent's lane, and to answer
/// anything else.
const HOLDING: Duration = Duration::from_secs(30);
const ASKING: Duration = Duration::from_secs(5);
/// How much longer than its own deadline the keeper's answer is waited for.
const MARGIN: Duration = Duration::from_secs(2);
/// How long the pending changes' lock is waited for.
const CLEARING: Duration = Duration::from_secs(5);

/// What the person asked of a VM's maintenance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Maintain {
    /// End it, rather than begin it.
    pub end: bool,
    /// Stop the VM and start it again before it begins.
    pub restart: bool,
}

/// Begin or end the person's maintenance of the VM `client` acts on, once
/// they allow it in Atlas.app, saying each step to `say` as a plain line.
///
/// # Errors
///
/// Refuses another desktop, a VM not added, being removed or handed over,
/// and without the person's grant; a begin on a VM neither confirmed
/// running nor stopped, whose guest runs or cannot say whether it runs work
/// agents started without a restart, beside a pending change without a
/// restart, not confirmed stopped or running after Atlas VM stopped or
/// started it, or whose stop, start or window Atlas VM refused; an end
/// whose window Atlas VM did not confirm takes no input, or confirmed only
/// by another run than the one that opened it while that run's VM is not
/// said gone. Fails when a record cannot be kept. Each says what is kept
/// and what to run.
pub fn maintain(client: &Client, asked: Maintain, say: &dyn Fn(&str)) -> Result<Value> {
    Maintainer::of(client, Patience::REAL)?.run(asked, &person, say)
}

/// [`maintain`], the person's grant given by `grants`, each window change
/// waited for `within` and looked at often, each step said to `say`.
#[cfg(test)]
pub(in crate::vm) fn maintained_by(
    client: &Client,
    asked: Maintain,
    within: Duration,
    grants: &impl Fn(&grant::Operation) -> Result<Token>,
    say: &dyn Fn(&str),
) -> Result<Value> {
    let patience = Patience {
        within,
        powering: within,
        every: Duration::from_millis(50),
    };
    Maintainer::of(client, patience)?.run(asked, grants, say)
}

/// How long a window change, and a stop or start of the VM, is waited
/// for, and how often each is looked at.
#[derive(Clone, Copy, Debug)]
struct Patience {
    within: Duration,
    powering: Duration,
    every: Duration,
}

impl Patience {
    const REAL: Self = Self {
        within: ANSWERING,
        powering: POWERING,
        every: EVERY,
    };
}

/// Ask the person, through Atlas.app, to allow `operation`, and use their
/// grant: the change it is made as.
fn person(operation: &grant::Operation) -> Result<Token> {
    grant::ask(operation.clone())?.take(operation)
}

/// What a window change asked of Atlas VM came to.
enum Asked {
    /// Atlas VM's own word, with the run that did it and the run that
    /// answered.
    Came {
        outcome: Outcome,
        by: Option<String>,
        answering: Option<String>,
    },
    /// Its only submission was not taken, for this reason: nothing was
    /// begun.
    NotTaken(String),
    /// Not known yet.
    Unknown,
}

impl Asked {
    /// What Atlas VM's `reply` says the change came to, naming the runs;
    /// none while it does not say.
    fn came(reply: Reply) -> Option<Self> {
        Some(Self::Came {
            outcome: reply.outcome?,
            by: reply.by,
            answering: reply.incarnation.map(|incarnation| incarnation.id),
        })
    }
}

/// How the VM's keeper is reached.
enum Reach {
    /// On its socket, each exchange judged as the keeper's before anything
    /// is said.
    At(link::Keeper),
    /// No keeper listens: its socket is not there.
    Absent,
    /// Whether one listens cannot be told.
    Unknown,
}

impl Reach {
    /// The keeper `link` reaches, or else the one listening on its socket,
    /// for `selected`'s VM `vm`: a link that did not answer its hello is no
    /// proof that no keeper is there.
    fn of(link: &Link, selected: &Selected, vm: &DesktopId) -> Self {
        match link {
            Link::Keeper(keeper) => Self::At(keeper.clone()),
            _ => Self::listening(selected, vm, crate::vm::keeper::socket_of(vm)),
        }
    }

    /// The keeper of `selected`'s VM `vm` listening at `socket`, absent only
    /// when nothing is there.
    fn listening(selected: &Selected, vm: &DesktopId, socket: Result<PathBuf>) -> Self {
        let Ok(socket) = socket else {
            return Self::Unknown;
        };
        match std::fs::symlink_metadata(&socket) {
            Ok(_) => Self::At(link::Keeper::at(
                Arc::new(selected.clone()),
                vm.clone(),
                socket,
                ASKING,
                Arc::new(link::Launchd),
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::Absent,
            Err(_) => Self::Unknown,
        }
    }
}

/// One command's maintenance of one VM.
struct Maintainer {
    lifecycle: Lifecycle,
    vms: Vms,
    vm: DesktopId,
    name: String,
    controller: Controller,
    /// The VM's keeper, as it was found.
    keeper: Reach,
    patience: Patience,
}

impl Maintainer {
    /// The maintenance of the VM `client` acts on.
    fn of(client: &Client, patience: Patience) -> Result<Self> {
        let name = client.selected.name.clone();
        let Chosen::Vm(config) = &client.selected.desktop else {
            return Err(Error::new(
                Code::Usage,
                format!("{name} is not a VM desktop; only a VM's window opens for maintenance."),
            ));
        };
        let lifecycle = client
            .lifecycle
            .clone()
            .ok_or_else(|| client.unreachable())?;
        let keeper = Reach::of(&client.link, &client.selected, &config.id);
        Ok(Self {
            vms: lifecycle.vms(),
            lifecycle,
            vm: config.id.clone(),
            name,
            controller: client.controller.clone(),
            keeper,
            patience,
        })
    }

    /// What the person `asked`, each grant asked of `grants`.
    fn run(
        &self,
        asked: Maintain,
        grants: &impl Fn(&grant::Operation) -> Result<Token>,
        say: &dyn Fn(&str),
    ) -> Result<Value> {
        if asked.end {
            self.end(grants, say)
        } else {
            self.begin(asked.restart, grants, say)
        }
    }

    /// Begin the maintenance, the VM stopped and started again first when
    /// `restart`, or go on with one begun and not yet open.
    fn begin(
        &self,
        restart: bool,
        grants: &impl Fn(&grant::Operation) -> Result<Token>,
        say: &dyn Fn(&str),
    ) -> Result<Value> {
        let record = self.created()?;
        // The maintenance the person is asked to go on with, if any: the
        // grant holds for that one alone, with its restart as the card said.
        let (first, restart) = match Kept::of(record.maintenance.clone()) {
            Kept::On(maintenance) if maintenance.phase == Phase::Open => {
                return Ok(self.opened(&maintenance, true));
            }
            Kept::On(maintenance) => (Some(maintenance.change), maintenance.restart),
            Kept::Ending(_) => return Err(self.ending()),
            Kept::None | Kept::Unreadable => (None, restart),
        };
        let going_on = first.is_some();
        // Refused before the person is asked, and judged again once held.
        if !going_on {
            self.not_updating()?;
            self.first_phase(restart)?;
            if !restart {
                self.controller
                    .require_known()
                    .map_err(|error| self.beside(error))?;
            }
        }
        say(&format!(
            "Asking you in Atlas.app to allow opening the window of the VM \u{201c}{}\u{201d} \
             for maintenance.",
            self.name
        ));
        let change = grants(&self.operation(&record, false, restart))?;
        let admitted = self.lifecycle.admit()?;
        let _hold = hold_in(&self.vms, &self.vm, &admitted)?.ok_or_else(|| self.busy(false))?;
        let _exclusion = self.exclusion(going_on || restart, false)?;
        // Looked at again under both: a removal, a handover or another
        // maintenance begun meanwhile is seen now.
        let mut record = self.created()?;
        let mut maintenance = match (record.maintenance.clone(), going_on) {
            (None, false) => {
                self.not_updating()?;
                let maintenance = Maintenance {
                    change,
                    phase: self.first_phase(restart)?,
                    restart,
                    since_ms: atlas_core::time::now_ms(),
                    stop: None,
                    start: None,
                    open: None,
                    close: None,
                    opened_by: None,
                    settled: false,
                    ended: None,
                };
                record.maintenance = Some(maintenance.clone());
                self.lifecycle.keep_record(&record)?;
                maintenance
            }
            (Some(kept), true)
                if Some(&kept.change) != first.as_ref() || kept.restart != restart =>
            {
                return Err(self.changed_meanwhile(false));
            }
            (Some(maintenance), true) if maintenance.phase == Phase::Open => {
                return Ok(self.opened(&maintenance, true));
            }
            (Some(maintenance), true) if !maintenance.ending() => maintenance,
            (None, true) => return Err(self.changed_meanwhile(false)),
            _ => return Err(self.busy(false)),
        };
        if matches!(maintenance.phase, Phase::Stopping | Phase::Starting) {
            self.powered(&mut record, &mut maintenance, say)?;
        }
        say(&format!(
            "Holding every agent's work on {} until you end it.",
            self.name
        ));
        self.hold(&maintenance)?;
        let open = maintenance.open.clone().unwrap_or_else(Token::new);
        maintenance.open = Some(open.clone());
        maintenance.phase = Phase::Opening;
        record.maintenance = Some(maintenance.clone());
        self.lifecycle.keep_record(&record)?;
        say(&format!("Asking Atlas VM to open {}'s window.", self.name));
        let claim = self.claim(&open)?;
        let window = Operation::Window {
            change: open.clone(),
            input: true,
        };
        let uncertain = record.sent(&open).is_some();
        match self.ask(&mut record, &window, uncertain)? {
            Asked::Came {
                outcome: Outcome::Done,
                by,
                ..
            } => {
                maintenance.phase = Phase::Open;
                maintenance.opened_by = by;
                record.maintenance = Some(maintenance.clone());
                self.lifecycle.keep_record(&record)?;
                self.clear(&open, claim)?;
                Ok(self.opened(&maintenance, false))
            }
            Asked::Came {
                outcome: Outcome::Refused { reason },
                ..
            }
            | Asked::NotTaken(reason) => {
                self.clear(&open, claim)?;
                record.maintenance_settled();
                self.lifecycle.keep_record(&record)?;
                let refused = Error::new(
                    Code::State,
                    format!("Atlas VM did not open {}'s window: {reason}", self.name),
                );
                Err(if self.released(&maintenance.change) {
                    refused
                } else {
                    refused.with("next", json!(self.still_held()))
                })
            }
            // Left pending, its claim kept, for the next begin or end.
            Asked::Unknown => Err(Error::new(
                Code::NotReady,
                format!(
                    "Atlas VM has not said whether it opened {}'s window; agents wait. Run \
                     atlas desktop maintain {} again.",
                    self.name, self.name
                ),
            )),
        }
    }

    /// End the maintenance, or go on with an end already recorded.
    fn end(
        &self,
        grants: &impl Fn(&grant::Operation) -> Result<Token>,
        say: &dyn Fn(&str),
    ) -> Result<Value> {
        let record = self.recorded()?;
        let Some(first) = record.maintenance.clone() else {
            return self.none_kept();
        };
        // Going on with an end already recorded can only turn input off, and
        // goes on with that very maintenance alone.
        let going_on = first.ending();
        if going_on {
            say(&format!(
                "Going on with ending maintenance of the VM \u{201c}{}\u{201d}.",
                self.name
            ));
        } else {
            say(&format!(
                "Asking you in Atlas.app to allow ending maintenance of the VM \u{201c}{}\u{201d}.",
                self.name
            ));
            grants(&self.operation(&record, true, false))?;
        }
        let admitted = self.lifecycle.admit()?;
        let _hold = hold_in(&self.vms, &self.vm, &admitted)?.ok_or_else(|| self.busy(true))?;
        let _exclusion = self.exclusion(true, true)?;
        let mut record = self.recorded()?;
        let mut maintenance = match record.maintenance.clone() {
            Some(kept) if kept.change == first.change && (!going_on || kept.ending()) => kept,
            None if !going_on => return self.none_kept(),
            _ => return Err(self.changed_meanwhile(true)),
        };
        if maintenance.ended.is_none() {
            let settled = maintenance.settled;
            // Closing from here: no begin goes on with its window-on.
            maintenance.phase = Phase::Closing;
            record.maintenance = Some(maintenance.clone());
            self.lifecycle.keep_record(&record)?;
            if !settled {
                self.settle(&mut record, &mut maintenance)?;
            }
            let ended = self.closed(&mut record, &mut maintenance, say)?;
            // Kept before anything of the maintenance goes.
            maintenance.ended = Some(ended);
            record.maintenance = Some(maintenance.clone());
            self.lifecycle.keep_record(&record)?;
        }
        for change in [
            &maintenance.stop,
            &maintenance.start,
            &maintenance.open,
            &maintenance.close,
        ]
        .into_iter()
        .flatten()
        {
            self.controller
                .clear_known_within(change.as_str(), CLEARING)
                .map_err(|error| {
                    Error::new(
                        Code::State,
                        format!(
                            "Atlas VM confirmed {}'s window takes no input, but a pending claim \
                             of it could not be cleared: {} {}",
                            self.name,
                            error.error,
                            self.again(true)
                        ),
                    )
                })?;
        }
        // Kept ended until the keeper lets agents in: the next end tells it
        // again.
        if !self.released(&maintenance.change) {
            return Err(Error::new(
                Code::NotReady,
                format!(
                    "Atlas VM confirmed {}'s window takes no input, but its keeper did not \
                     answer to let agents in again, so they still wait. {}",
                    self.name,
                    self.again(true)
                ),
            ));
        }
        // A keeper started since holds from the record alone, and lets
        // agents in once it keeps none.
        record.maintenance_settled();
        self.lifecycle.keep_record(&record)?;
        Ok(
            json!({"ok": true, "desktop": self.name, "maintenance": "none",
            "input": "off", "change": maintenance.change, "ended": maintenance.ended}),
        )
    }

    /// Ask Atlas VM to turn off the window's input under the change kept
    /// for it, or a new one, until a run whose word ends `maintenance`
    /// confirmed it: that proof. A window-off done by a run gone since is
    /// asked once more, of the run answering now, under a change of its own.
    fn closed(
        &self,
        record: &mut Record,
        maintenance: &mut Maintenance,
        say: &dyn Fn(&str),
    ) -> Result<Ended> {
        for _ in 0..2 {
            let close = maintenance.close.get_or_insert_with(Token::new).clone();
            self.keep(record, maintenance)?;
            say(&format!(
                "Asking Atlas VM to turn off {}'s window.",
                self.name
            ));
            let claim = self.claim(&close)?;
            let window = Operation::Window {
                change: close.clone(),
                input: false,
            };
            let uncertain = record.sent(&close).is_some();
            match self.ask(record, &window, uncertain)? {
                Asked::Came {
                    outcome: Outcome::Done,
                    by,
                    answering,
                } => {
                    self.clear(&close, claim)?;
                    if let Some(ended) =
                        self.proof(maintenance, &close, by, answering.as_deref())?
                    {
                        return Ok(ended);
                    }
                    maintenance.close = None;
                }
                Asked::Came {
                    outcome: Outcome::Refused { reason },
                    ..
                }
                | Asked::NotTaken(reason) => {
                    self.clear(&close, claim)?;
                    // The next end asks under a change of its own.
                    maintenance.close = None;
                    self.keep(record, maintenance)?;
                    return Err(Error::new(
                        Code::State,
                        format!(
                            "Atlas VM could not turn off {}'s window: {reason}",
                            self.name
                        ),
                    )
                    .with("next", json!(self.again(true))));
                }
                // Left pending, its claim kept, for the next end.
                Asked::Unknown => return Err(self.not_answering()),
            }
        }
        self.keep(record, maintenance)?;
        Err(self.another_run())
    }

    /// Why an end waits on an Atlas VM that does not answer.
    fn not_answering(&self) -> Error {
        Error::new(
            Code::NotReady,
            format!(
                "Atlas VM, which runs {}, is not answering, so its window may still take your \
                 input; agents wait. {}",
                self.name,
                self.again(true)
            ),
        )
    }

    /// Settle every change of `maintenance` for good, under Closing: Atlas
    /// VM answers what each came to, and voids each it never saw, so one
    /// still on its way is never done. Its stop and start are then Atlas
    /// VM's no more, and its window-on's opener is known, or there is none,
    /// so a window-off kept before is never its end's proof.
    fn settle(&self, record: &mut Record, maintenance: &mut Maintenance) -> Result<()> {
        let kept = [&maintenance.stop, &maintenance.start, &maintenance.open];
        if kept.iter().any(|change| change.is_some()) {
            let hello = self.runner().read_within(&Operation::Hello, READING);
            match hello {
                Ok(reply) if reply.hello.as_ref().is_some_and(|hello| hello.can("void")) => {}
                Ok(reply) if reply.ok => return Err(self.older()),
                Ok(_) | Err(_) => return Err(self.not_answering()),
            }
        }
        if let Some(stop) = &maintenance.stop {
            self.voided(stop, &format!("the request to stop {}", self.name))?;
        }
        if let Some(start) = &maintenance.start {
            self.voided(start, &format!("the request to start {}", self.name))?;
        }
        if let Some(open) = maintenance.open.clone() {
            let request = format!("the request to open {}'s window", self.name);
            let voided = self.voided(&open, &request)?;
            maintenance.opened_by = match (voided.outcome, voided.by) {
                (Some(Outcome::Done), Some(by)) => Some(by),
                (Some(Outcome::Refused { .. }), _) => None,
                _ => return Err(self.not_settled(&request)),
            };
        }
        maintenance.settled = true;
        maintenance.close = None;
        record.maintenance = Some(maintenance.clone());
        self.lifecycle.keep_record(record)
    }

    /// Atlas VM's answer to a void of `change`, `request` in words, asked
    /// until it says what `change` came to, or the patience runs out.
    fn voided(&self, change: &Token, request: &str) -> Result<Reply> {
        let runner = self.runner();
        let deadline = Instant::now() + self.patience.within;
        loop {
            self.controller.interrupt().check()?;
            if let Ok(reply) = runner.read_within(&Operation::Void(change.clone()), READING)
                && reply.ok
                && reply.change.as_ref() == Some(change)
                && reply.outcome.is_some()
            {
                return Ok(reply);
            }
            if Instant::now() + self.patience.every > deadline {
                return Err(self.not_settled(request));
            }
            std::thread::sleep(self.patience.every);
        }
    }

    /// None is kept: the keeper is still told to let agents in, as a hold
    /// whose end went before its release would keep them out.
    fn none_kept(&self) -> Result<Value> {
        // Any change will do: a keeper lets go once the record keeps none.
        if !self.released(&Token::new()) {
            return Err(Error::new(Code::NotReady, self.still_held()));
        }
        Ok(json!({"ok": true, "desktop": self.name, "maintenance": "none", "already": true}))
    }

    /// The VM's lifecycle record, once it is added and nothing removes it
    /// or hands its session over.
    fn created(&self) -> Result<Record> {
        let record = self.recorded()?;
        let removing = || {
            Error::new(
                Code::State,
                format!(
                    "{} is being removed, so its window is not opened.",
                    self.name
                ),
            )
        };
        match record.state {
            State::Created => {}
            State::Removing => return Err(removing()),
            State::Adding => {
                return Err(Error::new(
                    Code::NotReady,
                    format!(
                        "{} is still being added; run atlas desktop add vm to finish it first.",
                        self.name
                    ),
                ));
            }
        }
        if record.removal.is_some() || self.lifecycle.recorded_removal(&self.vm)?.is_some() {
            return Err(removing());
        }
        let station = crate::station::Station::under(self.lifecycle.home()).path_of(&self.vm);
        if let Some(why) = handover::unfinished_in(&station, &self.name) {
            return Err(Error::new(Code::Stopped, why));
        }
        Ok(record)
    }

    /// Refuse a maintenance begun while an update of the VM's Atlas may still
    /// be taken by its guest: an Atlas VM older than that update may never
    /// end one. A record of it that cannot be read counts as one under way.
    fn not_updating(&self) -> Result<()> {
        let station = crate::station::Station::under(self.lifecycle.home()).path_of(&self.vm);
        if crate::vm::keeper::update_unsettled(&station) {
            return Err(Error::new(
                Code::Busy,
                format!(
                    "{}'s Atlas is being updated; begin maintenance once it is done.",
                    self.name
                ),
            ));
        }
        Ok(())
    }

    /// The VM's lifecycle record.
    fn recorded(&self) -> Result<Record> {
        self.lifecycle.record(&self.vm)?.ok_or_else(|| {
            Error::new(
                Code::State,
                format!("No VM named {} is kept on this Mac.", self.name),
            )
        })
    }

    /// The grant a begin, or with `end` an end, asks of the person, the VM
    /// stopped and started again first when `restart`.
    fn operation(&self, record: &Record, end: bool, restart: bool) -> grant::Operation {
        grant::Operation {
            kind: Kind::Maintain,
            vm: self.vm.clone(),
            name: self.name.clone(),
            settings: record.settings.clone(),
            cancel: false,
            restart,
            end,
        }
    }

    /// The phase a new begin starts at, the VM stopped and started again
    /// first when `restart`: a VM confirmed stopped runs nothing and is
    /// started; a running one is stopped first when `restart`, else only
    /// while its guest says no work agents started runs there.
    fn first_phase(&self, restart: bool) -> Result<Phase> {
        let power = self.runner().read_within(&Operation::State, READING);
        match power {
            Ok(reply) if reply.ok && reply.confirmed == Some(true) => match reply.power {
                Some(Power::Running) if restart => Ok(Phase::Stopping),
                Some(Power::Running) => self.quiet().map(|()| Phase::Holding),
                Some(Power::Stopped) => Ok(Phase::Starting),
                _ => Err(self.not_running_yet()),
            },
            Ok(_) | Err(_) => Err(self.not_running_yet()),
        }
    }

    /// Refuse a begin unless the running VM's guest says no work agents
    /// started runs there.
    fn quiet(&self) -> Result<()> {
        let read = json!({"op": "read", "request": {"op": "leftovers"},
            "deadline_ms": ASKING.as_millis()});
        let keeper = match &self.keeper {
            Reach::At(keeper) => Some(keeper),
            Reach::Absent | Reach::Unknown => None,
        };
        let leftovers = keeper
            .and_then(|keeper| keeper.exchange(&read, ASKING + MARGIN).ok()?.pop())
            .and_then(|answer| Leftovers::of(&answer).ok())
            .unwrap_or(Leftovers::Unknown);
        let restart = format!(
            "add --restart to stop {} and start it again for maintenance",
            self.name
        );
        match leftovers {
            Leftovers::None => Ok(()),
            Leftovers::Running => Err(Error::new(
                Code::Busy,
                format!(
                    "Work an agent started still runs on {}, so its window cannot take your \
                     input yet. Wait for it to end, or {restart}; that work ends.",
                    self.name
                ),
            )),
            Leftovers::Unknown => Err(Error::new(
                Code::NotReady,
                format!(
                    "{}'s guest cannot say whether agents' work still runs there. Run atlas \
                     desktop maintain {} again in a moment, or {restart}; any work there ends.",
                    self.name, self.name
                ),
            )),
        }
    }

    fn not_running_yet(&self) -> Error {
        Error::new(
            Code::NotReady,
            format!(
                "Atlas VM cannot tell yet whether {} runs or is stopped, so its window is not \
                 opened. Run atlas desktop maintain {} again in a moment.",
                self.name, self.name
            ),
        )
    }

    /// Ask the keeper to hold every agent's lane for `maintenance`: answered
    /// only once each ended.
    fn hold(&self, maintenance: &Maintenance) -> Result<()> {
        let request = json!({"op": "hold", "change": maintenance.change,
            "deadline_ms": HOLDING.as_millis()});
        let answer = match &self.keeper {
            Reach::At(keeper) => Some(keeper.exchange(&request, HOLDING + MARGIN)),
            Reach::Absent | Reach::Unknown => None,
        };
        let why = match answer {
            Some(Ok(mut lines)) => match lines.pop() {
                Some(held) if held["ok"] == true && held["change"] == json!(maintenance.change) => {
                    return Ok(());
                }
                Some(refused) => refused["error"].as_str().map(str::to_owned),
                None => None,
            },
            Some(Err(error)) => Some(error.error),
            None => None,
        };
        let held = Error::new(
            Code::NotReady,
            format!(
                "Agents on {} are held, but its keeper has not said their work there stopped, \
                 so its window is not open yet. {}",
                self.name,
                self.again(false)
            ),
        );
        Err(match why {
            Some(why) => held.with("why", json!(why)),
            None => held,
        })
    }

    /// Whether the keeper let agents in again, asked once the maintenance
    /// `change` is recorded ended or none is kept: proven absent counts, as
    /// a keeper that starts reads the record afresh; one that cannot be
    /// told, or did not answer, never does.
    fn released(&self, change: &Token) -> bool {
        match &self.keeper {
            Reach::At(keeper) => release(keeper, change),
            Reach::Absent => true,
            Reach::Unknown => false,
        }
    }

    /// Why agents still wait once no maintenance is kept.
    fn still_held(&self) -> String {
        format!(
            "No maintenance of {} is kept, but its keeper has not let agents in again yet. {}",
            self.name,
            self.again(true)
        )
    }

    /// The VM's exclusion: for a new begin beside nothing pending, else
    /// whatever is, as Atlas VM's own claims stay pending until it answers,
    /// and so do the guest's on a restart. Refused busy in the words of an
    /// end when `end`, else of a begin.
    fn exclusion(&self, any: bool, end: bool) -> Result<Ownership> {
        let controller = self.controller.clone().waiting(Duration::ZERO);
        if any {
            return controller.acquire_for_executor().map_err(|error| {
                if error.code == Code::Busy {
                    self.busy(end)
                } else {
                    error
                }
            });
        }
        controller
            .acquire(false)
            .map_err(|error| self.beside(error))
    }

    /// `error`, the exclusion's refusal of a begin beside what is pending
    /// or running, said of the VM, its other details kept, with what to do.
    fn beside(&self, mut error: Error) -> Error {
        if error
            .details
            .get("running")
            .is_some_and(|running| running == "command")
        {
            error.error = format!(
                "A command an agent started is still running on {}. Wait for it to end.",
                self.name
            );
        }
        error.with(
            "next",
            json!(format!(
                "Wait for it to end and run atlas desktop maintain {} again, or add --restart \
                 to stop {} and start it again for maintenance.",
                self.name, self.name
            )),
        )
    }

    /// The claim on the window change `change`, as Atlas VM's.
    fn claim(&self, change: &Token) -> Result<Pending> {
        self.controller
            .quarantine_by_within(change.as_str(), Executor::Vm, CLEARING)
            .map_err(|error| {
                if error.code == Code::Busy {
                    Error::new(
                        Code::Busy,
                        format!(
                            "The changes pending on {} are busy. {}",
                            self.name,
                            self.again(false)
                        ),
                    )
                } else {
                    error
                }
            })
    }

    /// Clear the claim on the change `change`, which Atlas VM answered.
    fn clear(&self, change: &Token, claim: Pending) -> Result<()> {
        self.controller
            .clear_known_within(change.as_str(), CLEARING)
            .map_err(|error| {
                Error::new(
                    Code::State,
                    format!(
                        "Atlas VM answered a change of {}, but its pending claim could not be \
                         cleared, so it is kept: {}",
                        self.name, error.error
                    ),
                )
            })?;
        drop(claim);
        Ok(())
    }

    /// Ask Atlas VM for the change `operation`, kept in `record` before it
    /// is sent, and follow it until Atlas VM keeps an outcome for it or the
    /// patience for a window change runs out. `uncertain` when it may have been sent
    /// before. A lost reply is followed by what Atlas VM says it came to,
    /// and the very same change is sent again when Atlas VM never heard of
    /// it. A refusal unkept is taken as the change's only while nothing
    /// else may have sent it.
    fn ask(&self, record: &mut Record, operation: &Operation, uncertain: bool) -> Result<Asked> {
        self.ask_within(record, operation, uncertain, self.patience.within)
    }

    /// [`Self::ask`], followed for `within`.
    fn ask_within(
        &self,
        record: &mut Record,
        operation: &Operation,
        uncertain: bool,
        within: Duration,
    ) -> Result<Asked> {
        let change = operation
            .change()
            .cloned()
            .ok_or_else(|| Error::new(Code::BadRequest, "A read is not a change of the VM."))?;
        let runner = self.runner();
        let deadline = Instant::now() + within;
        let mut uncertain = uncertain;
        let mut asking = true;
        loop {
            if asking {
                asking = false;
                self.controller.interrupt().check()?;
                let lifecycle = &self.lifecycle;
                let answer = runner.change_within(
                    record,
                    |record| lifecycle.keep_record(record),
                    operation,
                    left(deadline),
                );
                match answer {
                    Ok(reply) if reply.ok => {
                        if let Some(came) = Asked::came(reply) {
                            return Ok(came);
                        }
                        uncertain = true;
                    }
                    Ok(reply) if !uncertain => {
                        return Ok(Asked::NotTaken(reply.error.unwrap_or_else(|| {
                            "Atlas VM did not take the change.".to_owned()
                        })));
                    }
                    Ok(_) => {}
                    Err(error) if error.code == Code::Transport => uncertain = true,
                    Err(error) => return Err(error),
                }
            }
            if Instant::now() + self.patience.every > deadline {
                return Ok(Asked::Unknown);
            }
            std::thread::sleep(self.patience.every);
            match runner.read_within(&Operation::Outcome(change.clone()), READING) {
                Ok(reply) if reply.ok && reply.change.as_ref() == Some(&change) => {
                    let pending = reply.pending;
                    match (Asked::came(reply), pending) {
                        (Some(came), _) => return Ok(came),
                        (None, Some(true)) => {}
                        (None, _) => asking = true,
                    }
                }
                Ok(_) | Err(_) => {}
            }
        }
    }

    fn runner(&self) -> Runner {
        Runner::in_vms(&self.vms, &self.vm)
    }

    /// Keep `maintenance` in `record`.
    fn keep(&self, record: &mut Record, maintenance: &Maintenance) -> Result<()> {
        record.maintenance = Some(maintenance.clone());
        self.lifecycle.keep_record(record)
    }

    /// What a begin answers once the window takes the person's input.
    /// A restart's says too when a change it left alone is still pending:
    /// the guest's own, which maintenance never clears.
    fn opened(&self, maintenance: &Maintenance, already: bool) -> Value {
        let mut opened = json!({"ok": true, "desktop": self.name, "maintenance": "on",
            "window": "open", "change": maintenance.change, "already": already,
            "next": format!("End it with atlas desktop maintain {} --end.", self.name)});
        let left = self
            .controller
            .pending()
            .is_ok_and(|pending| !pending.is_empty());
        if maintenance.restart && left {
            opened["pending"] = json!(format!(
                "A change on {} is still pending; Atlas settles it once {} answers.",
                self.name, self.name
            ));
        }
        opened
    }

    /// Why a begin is refused while the maintenance ends.
    fn ending(&self) -> Error {
        Error::new(
            Code::Busy,
            format!(
                "{}'s maintenance is ending. Run atlas desktop maintain {} --end to finish it.",
                self.name, self.name
            ),
        )
    }

    /// Why the window's input off ends nothing: another run of Atlas VM
    /// answered than the one that opened it, or no one can name that run.
    fn another_run(&self) -> Error {
        Error::new(
            Code::NotReady,
            format!(
                "Atlas VM ran again since it opened {}'s window, and whether that window is \
                 gone cannot be told yet, so agents wait. Run atlas desktop maintain {} --end \
                 again later.",
                self.name, self.name
            ),
        )
    }

    /// Why a begin, or with `end` an end, goes on with no maintenance the
    /// person allowed it for.
    fn changed_meanwhile(&self, end: bool) -> Error {
        let (waited, needs) = if end {
            ("end", "ending it")
        } else {
            ("begin", "opening its window")
        };
        Error::new(
            Code::Denied,
            format!(
                "{}'s maintenance changed while this {waited} waited, so {needs} needs your \
                 grant again; nothing was asked of it. {}",
                self.name,
                self.again(end)
            ),
        )
    }

    /// Why an end waits on an Atlas VM of a build that cannot void a change.
    fn older(&self) -> Error {
        Error::new(
            Code::Update,
            format!(
                "{name}'s Atlas VM is from an older Atlas and cannot make sure an earlier request \
                 to open its window is never done, so agents wait. Update Atlas, then run atlas \
                 desktop maintain {name} --end again.",
                name = self.name
            ),
        )
    }

    /// Why an end waits on `request`, a change of the maintenance whose
    /// outcome is not known yet.
    fn not_settled(&self, request: &str) -> Error {
        Error::new(
            Code::NotReady,
            format!(
                "Atlas VM has not said what {request} came to, so {}'s window may still take \
                 your input; agents wait. {}",
                self.name,
                self.again(true)
            ),
        )
    }

    /// Why another command keeps the VM now.
    fn busy(&self, end: bool) -> Error {
        Error::new(
            Code::Busy,
            format!(
                "Another Atlas command or an update holds {}. {}",
                self.name,
                self.again(end)
            ),
        )
    }

    /// What to run again: the end when `end`, else the begin.
    fn again(&self, end: bool) -> String {
        format!(
            "Run atlas desktop maintain {}{} again.",
            self.name,
            if end { " --end" } else { "" }
        )
    }
}

/// Whether `keeper` let agents in again, asked to release the maintenance
/// `change`.
fn release(keeper: &link::Keeper, change: &Token) -> bool {
    let request = json!({"op": "release", "change": change,
        "deadline_ms": ASKING.as_millis()});
    keeper
        .exchange(&request, ASKING + MARGIN)
        .ok()
        .and_then(|mut lines| lines.pop())
        .is_some_and(|answer| answer["ok"] == true)
}

/// What is left before `deadline`.
fn left(deadline: Instant) -> Duration {
    deadline.saturating_duration_since(Instant::now())
}

#[cfg(test)]
#[path = "maintain_tests.rs"]
mod tests;
