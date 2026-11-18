//! A maintenance's VM stopped and started again, or only started, before
//! its window opens. Each is a change of its own, kept, claimed as Atlas
//! VM's and asked, its claim cleared only on Atlas VM's answer, and the
//! next phase is begun only once Atlas VM confirms the VM's power: a lost
//! answer never takes a stop for done. Neither is a start a command asked
//! because it needed the VM running, so neither is kept among those.
use super::{Asked, Maintainer, READING};
use crate::vm::{
    lifecycle::record::{Maintenance, Phase, Record},
    runner::{Operation, Outcome, Power},
};
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
};
use serde_json::json;
use std::time::Instant;

impl Maintainer {
    /// Take `maintenance`, kept in `record`, from `stopping` or `starting`
    /// to `holding`: its VM stopped first while stopping, then started,
    /// each confirmed, every step said to `say`.
    pub(super) fn powered(
        &self,
        record: &mut Record,
        maintenance: &mut Maintenance,
        say: &dyn Fn(&str),
    ) -> Result<()> {
        if maintenance.phase == Phase::Stopping {
            say(&format!(
                "Asking Atlas VM to stop {}; work running there ends.",
                self.name
            ));
            let stop = maintenance.stop.get_or_insert_with(Token::new).clone();
            self.keep(record, maintenance)?;
            self.changed(record, &Operation::Stop(stop))?;
            if !self.confirmed(Power::Stopped)? {
                return Err(self.not_stopped());
            }
            maintenance.phase = Phase::Starting;
            self.keep(record, maintenance)?;
        }
        say(&format!("Asking Atlas VM to start {}.", self.name));
        let start = maintenance.start.get_or_insert_with(Token::new).clone();
        self.keep(record, maintenance)?;
        // A start Atlas VM has not taken is sent only on its word, now, that
        // the VM is stopped: a run started since may not see it so yet. One
        // it took is followed under its own ID.
        if !self.taken(record, &start)? && !self.confirmed(Power::Stopped)? {
            return Err(self.not_stopped());
        }
        self.changed(record, &Operation::Start(start))?;
        if !self.confirmed(Power::Running)? {
            return Err(Error::new(
                Code::NotReady,
                format!(
                    "{} is not confirmed running yet, so its window is not opened. {}",
                    self.name,
                    self.again(false)
                ),
            ));
        }
        maintenance.phase = Phase::Holding;
        self.keep(record, maintenance)
    }

    /// Ask Atlas VM for the stop or start `operation`, kept in `record`,
    /// claimed as Atlas VM's first and cleared once it answered. One it
    /// refused, or did not take, forgets the maintenance and lets agents in
    /// again, as nothing of its window was asked; one it has not answered
    /// stays pending, its claim kept, for the next begin or end.
    fn changed(&self, record: &mut Record, operation: &Operation) -> Result<()> {
        let change = operation
            .change()
            .cloned()
            .ok_or_else(|| Error::new(Code::BadRequest, "A read is not a change of the VM."))?;
        let (asking, asked) = match operation {
            Operation::Stop(_) => ("stop", "stopped"),
            _ => ("start", "started"),
        };
        let claim = self.claim(&change)?;
        let uncertain = record.sent(&change).is_some();
        // A stop is answered once the guest shut down, or was stopped
        // outright after its minute: followed, by its outcome, that long.
        match self.ask_within(record, operation, uncertain, self.patience.powering)? {
            Asked::Came {
                outcome: Outcome::Done,
                ..
            } => self.clear(&change, claim),
            Asked::Came {
                outcome: Outcome::Refused { reason },
                ..
            }
            | Asked::NotTaken(reason) => {
                self.clear(&change, claim)?;
                let maintenance = record
                    .maintenance
                    .as_ref()
                    .map(|maintenance| maintenance.change.clone());
                record.maintenance_settled();
                self.lifecycle.keep_record(record)?;
                let refused = Error::new(
                    Code::State,
                    format!("Atlas VM did not {asking} {}: {reason}", self.name),
                );
                Err(
                    if maintenance.is_none_or(|maintenance| self.released(&maintenance)) {
                        refused
                    } else {
                        refused.with("next", json!(self.still_held()))
                    },
                )
            }
            Asked::Unknown => Err(Error::new(
                Code::NotReady,
                format!(
                    "Atlas VM has not said whether it {asked} {}; agents wait. {}",
                    self.name,
                    self.again(false)
                ),
            )),
        }
    }

    /// Whether Atlas VM keeps the change `change`, done or pending: never
    /// when it was not kept as sent.
    ///
    /// # Errors
    ///
    /// Refuses while Atlas VM does not say.
    fn taken(&self, record: &Record, change: &Token) -> Result<bool> {
        if record.sent(change).is_none() {
            return Ok(false);
        }
        match self
            .runner()
            .read_within(&Operation::Outcome(change.clone()), READING)
        {
            Ok(reply) if reply.ok && reply.change.as_ref() == Some(change) => {
                Ok(reply.outcome.is_some() || reply.pending == Some(true))
            }
            Ok(_) | Err(_) => Err(Error::new(
                Code::NotReady,
                format!(
                    "Atlas VM has not said whether it started {}; agents wait. {}",
                    self.name,
                    self.again(false)
                ),
            )),
        }
    }

    /// Why no start is asked while the VM is not confirmed stopped.
    fn not_stopped(&self) -> Error {
        Error::new(
            Code::NotReady,
            format!(
                "{} is not confirmed stopped yet, so it is not started for maintenance. {}",
                self.name,
                self.again(false)
            ),
        )
    }

    /// Whether Atlas VM confirms the VM's power is `power`, looked at until
    /// the patience for the VM's power runs out.
    fn confirmed(&self, power: Power) -> Result<bool> {
        let runner = self.runner();
        let deadline = Instant::now() + self.patience.powering;
        loop {
            self.controller.interrupt().check()?;
            let said = runner.read_within(&Operation::State, READING);
            if said.is_ok_and(|reply| {
                reply.ok && reply.confirmed == Some(true) && reply.power == Some(power)
            }) {
                return Ok(true);
            }
            if Instant::now() + self.patience.every > deadline {
                return Ok(false);
            }
            std::thread::sleep(self.patience.every);
        }
    }
}

#[cfg(test)]
#[path = "power_tests.rs"]
mod tests;
