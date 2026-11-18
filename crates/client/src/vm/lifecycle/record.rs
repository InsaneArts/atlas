//! Each VM's lifecycle record, `<id>.json`: what it is, which settings file
//! it is added to, how far its add or removal got, and every change sent to
//! Atlas VM for it, kept before it is sent. It outlives the VM's own
//! folders and is removed last, once nothing of the VM is left.
use super::{Lifecycle, Provenance, keep, read, remove};
use crate::vm::runner::Operation;
use atlas_core::{
    desktop::DesktopId,
    error::{Code, Error, Result},
    files::Token,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

/// Where a VM's lifecycle stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// Being added: every command on it but the person's Stop is refused.
    Adding,
    /// Added.
    Created,
    /// Being removed.
    Removing,
}

/// A step of an add, kept once done.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Step {
    /// The VM's runner's agent is loaded.
    Agent,
    /// Atlas VM made the VM.
    Created,
    /// Atlas VM started it.
    Started,
    /// Its enrolment is kept.
    Enrolled,
    /// Atlas in its guest reached the close of its install.
    Installed,
    /// Its settings entry is written.
    Listed,
    /// Its keeper's agent is loaded.
    Keeper,
    /// Its guest paired, and Atlas VM was told.
    Paired,
    /// Its guest was found closed, and Atlas VM was told.
    Closed,
}

/// A change Atlas VM was asked for, kept before it was sent: its ID, what
/// was asked as a digest that holds no secret, and the enrolment it names.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Sent {
    pub op: String,
    pub change: Token,
    pub request: String,
    #[serde(default)]
    pub enrolment: Option<Token>,
}

/// Why a lifecycle last stopped short.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Failure {
    pub code: String,
    pub error: String,
}

/// A start asked when a command needed the VM running that Atlas VM
/// refused: kept, so none is asked again for a while.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RefusedStart {
    pub change: Token,
    /// Why, in Atlas VM's own words.
    pub reason: String,
    /// When it was refused, in milliseconds since 1970.
    pub at_ms: u64,
}

/// How far the person's maintenance of a VM got.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    /// Kept: its keeper is being asked to hold every agent's lane, and
    /// Atlas VM is asked nothing until it has.
    Holding,
    /// Atlas VM is being asked to open its window with input.
    Opening,
    /// Its window takes the person's input.
    Open,
    /// Atlas VM is being asked to turn its window's input off.
    Closing,
}

/// The proof that ended a maintenance: the run of Atlas VM that confirmed
/// the window takes no input, and the change it confirmed it for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Ended {
    pub by: String,
    pub close: Token,
}

/// The person's maintenance of a VM, its window taking their input.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Maintenance {
    /// Its ID, which its keeper's hold names.
    pub change: Token,
    pub phase: Phase,
    /// Whether the person asked the VM stopped and started again first.
    #[serde(default)]
    pub restart: bool,
    /// When it was kept, in milliseconds since 1970.
    pub since_ms: u64,
    /// The change that opens its window with input, once one is kept.
    #[serde(default)]
    pub open: Option<Token>,
    /// The change that turns its window's input off, once one is kept.
    #[serde(default)]
    pub close: Option<Token>,
    /// The run of Atlas VM that opened the window, as its ledger says;
    /// none while it is not known.
    #[serde(default)]
    pub opened_by: Option<String>,
    /// Kept once Atlas VM confirmed the window takes no input.
    #[serde(default)]
    pub ended: Option<Ended>,
}

impl Maintenance {
    /// Whether it is ending: its window's input asked off, or confirmed off
    /// and not yet forgotten.
    #[must_use]
    pub fn ending(&self) -> bool {
        self.phase == Phase::Closing || self.ended.is_some()
    }
}

/// A VM's lifecycle record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Record {
    pub id: DesktopId,
    pub name: String,
    /// Whether the person named it; an unnamed add took the first free
    /// name.
    pub named: bool,
    /// The settings file it is added to, by its canonical path.
    pub settings: PathBuf,
    pub state: State,
    #[serde(default)]
    pub steps: Vec<Step>,
    #[serde(default)]
    pub operations: Vec<Sent>,
    /// The removal's change, kept before the removal begins.
    #[serde(default)]
    pub removal: Option<Token>,
    /// What its removal found in the VM's places and left, being
    /// another's: kept here, and said, for as long as the record is.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kept: Vec<String>,
    /// The Atlas that made its Keychain items.
    pub provenance: Provenance,
    #[serde(default)]
    pub failure: Option<Failure>,
    /// Every start asked because a command needed the VM running, kept
    /// before it is claimed and asked, until its claim is cleared.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub starts: Vec<Token>,
    /// The last of them Atlas VM refused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused_start: Option<RefusedStart>,
    /// The person's maintenance of the VM, kept before anything of it is
    /// asked and until its window is confirmed to take no input: agents
    /// wait for as long as it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maintenance: Option<Maintenance>,
}

impl Record {
    /// The record of an add of the VM `id` as `name`, to the settings file
    /// `settings`, by the Atlas `provenance` says, before anything of it is
    /// done.
    #[must_use]
    pub fn adding(
        reservation: &super::Reservation,
        settings: PathBuf,
        provenance: Provenance,
    ) -> Self {
        Self {
            id: reservation.id.clone(),
            name: reservation.name.clone(),
            named: reservation.named,
            settings,
            state: State::Adding,
            steps: Vec::new(),
            operations: Vec::new(),
            removal: None,
            kept: Vec::new(),
            provenance,
            failure: None,
            starts: Vec::new(),
            refused_start: None,
            maintenance: None,
        }
    }

    /// Whether `step` is done.
    #[must_use]
    pub fn has(&self, step: Step) -> bool {
        self.steps.contains(&step)
    }

    /// Note `step` done.
    pub fn done(&mut self, step: Step) {
        if !self.has(step) {
            self.steps.push(step);
        }
    }

    /// Note why it stopped short.
    pub fn failed(&mut self, error: &Error) {
        self.failure = Some(Failure {
            code: error.code.to_string(),
            error: error.error.clone(),
        });
    }

    /// The change `change` as it was kept, when it was.
    #[must_use]
    pub fn sent(&self, change: &Token) -> Option<&Sent> {
        self.operations.iter().find(|sent| &sent.change == change)
    }

    /// The last change kept as the operation `op`, by its name on the
    /// socket.
    #[must_use]
    pub(in crate::vm) fn last_sent(&self, op: &str) -> Option<&Sent> {
        self.operations.iter().rev().find(|sent| sent.op == op)
    }

    /// Forget the start `change`, asked because a command needed the VM
    /// running, once its claim is cleared: it and what was sent for it.
    pub(in crate::vm) fn start_settled(&mut self, change: &Token) {
        if self.starts.contains(change) {
            self.starts.retain(|kept| kept != change);
            self.operations.retain(|sent| sent.change != *change);
        }
    }

    /// Keep `operation`, a change, as about to be sent. The very same
    /// request asked again is kept once. Going on with an install is kept
    /// as that install, which must be.
    ///
    /// # Errors
    ///
    /// Refuses a read, which is never kept, another request kept under the
    /// same change, and going on with an install not kept.
    pub(crate) fn sending(&mut self, operation: &Operation) -> Result<()> {
        let Some(change) = operation.change() else {
            return Err(Error::new(
                Code::BadRequest,
                "A read of Atlas VM is not a change to keep.",
            ));
        };
        if let Operation::InstallResume(_) = operation {
            return match self.sent(change) {
                Some(kept) if kept.op == "install" => Ok(()),
                _ => Err(Error::new(
                    Code::State,
                    "No install was kept under this change to go on with; it is not sent.",
                )),
            };
        }
        let sent = Sent {
            op: operation.name().to_owned(),
            change: change.clone(),
            request: operation.digest(),
            enrolment: operation.enrolment().cloned(),
        };
        match self.sent(change) {
            Some(kept) if *kept == sent => Ok(()),
            Some(_) => Err(Error::new(
                Code::State,
                "Another request was kept under this change; it is not sent.",
            )),
            None => {
                self.operations.push(sent);
                Ok(())
            }
        }
    }
}

/// What the removal record in Atlas's VMs folder says of a lifecycle's
/// removal, reconciled by the change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Removal {
    /// None has begun.
    None,
    /// The removal the record names is under way.
    UnderWay,
    /// It is done: with what it found and left, being another's, what
    /// became of each password, and of the VM's enrolment when it had one
    /// (`left` when it could not be deleted).
    Done {
        kept: Vec<String>,
        passwords: BTreeMap<String, String>,
        enrolment: Option<String>,
    },
    /// A removal of the VM is recorded under another change.
    Other,
}

impl Lifecycle {
    fn record_path(&self, id: &DesktopId) -> Result<PathBuf> {
        Ok(self.folder()?.join(format!("{}.json", id.as_str())))
    }

    /// The record of the VM `id`; none when there is none.
    ///
    /// # Errors
    ///
    /// Fails when it cannot be read: a record that is there is never taken
    /// for absent.
    pub fn record(&self, id: &DesktopId) -> Result<Option<Record>> {
        let record: Option<Record> = read(&self.record_path(id)?, "A VM's lifecycle record")?;
        match record {
            Some(record) if record.id != *id => Err(Error::new(
                Code::State,
                "A VM's lifecycle record names another VM.",
            )),
            record => Ok(record),
        }
    }

    /// Where the VM `id`'s record is, made or not, no folder made.
    #[must_use]
    pub(crate) fn record_at(&self, id: &DesktopId) -> PathBuf {
        self.folder.join(format!("{}.json", id.as_str()))
    }

    /// Where the VM `id`'s lifecycle stands, its record read where it is
    /// and no folder made: none when it has none.
    ///
    /// # Errors
    ///
    /// Fails when its record cannot be read, or is not one this Atlas
    /// reads, or names another VM.
    pub(crate) fn state_of(&self, id: &DesktopId) -> Result<Option<State>> {
        let record: Option<Record> = read(&self.record_at(id), "A VM's lifecycle record")?;
        match record {
            Some(record) if record.id != *id => Err(Error::new(
                Code::State,
                "A VM's lifecycle record names another VM.",
            )),
            record => Ok(record.map(|record| record.state)),
        }
    }

    /// The person's maintenance of the VM `id`, its record read where it is
    /// and no folder made: none when it has no record or none is kept.
    ///
    /// # Errors
    ///
    /// Fails when its record cannot be read, or is not one this Atlas
    /// reads, or names another VM: a maintenance is never taken for none.
    pub(crate) fn maintenance_of(&self, id: &DesktopId) -> Result<Option<(String, Maintenance)>> {
        let record: Option<Record> = read(&self.record_at(id), "A VM's lifecycle record")?;
        match record {
            Some(record) if record.id != *id => Err(Error::new(
                Code::State,
                "A VM's lifecycle record names another VM.",
            )),
            record => Ok(record.and_then(|record| Some((record.name, record.maintenance?)))),
        }
    }

    /// Keep `record`, replacing the one kept.
    ///
    /// # Errors
    ///
    /// Fails when it cannot be written.
    pub fn keep_record(&self, record: &Record) -> Result<()> {
        keep(&self.record_path(&record.id)?, record)
    }

    /// Remove the record of the VM `id`: the last of a removal.
    ///
    /// # Errors
    ///
    /// Fails when it cannot be removed.
    pub fn forget_record(&self, id: &DesktopId) -> Result<()> {
        remove(&self.record_path(id)?)
    }

    /// Every VM's record, by identifier.
    ///
    /// # Errors
    ///
    /// Fails when the folder or a record in it cannot be read.
    pub fn records(&self) -> Result<Vec<Record>> {
        let folder = self.folder()?;
        let mut records = Vec::new();
        let entries = std::fs::read_dir(folder).map_err(|error| Error::writing(folder, error))?;
        for entry in entries {
            let entry = entry.map_err(|error| Error::writing(folder, error))?;
            let name = entry.file_name();
            let id = name
                .to_str()
                .and_then(|name| name.strip_suffix(".json"))
                .and_then(DesktopId::parse);
            if let Some(record) = id.map(|id| self.record(&id)).transpose()?.flatten() {
                records.push(record);
            }
        }
        records.sort_by(|one, other| one.id.as_str().cmp(other.id.as_str()));
        Ok(records)
    }

    /// The removal recorded for the VM `id` in this account's VMs folder,
    /// whatever its lifecycle record says: its change, and whether it is
    /// done; none when none is recorded.
    ///
    /// # Errors
    ///
    /// Fails when the removal record cannot be read.
    pub(in crate::vm) fn recorded_removal(&self, id: &DesktopId) -> Result<Option<(Token, bool)>> {
        let vms = self.vms();
        Ok(
            crate::vm::removal::recorded(&vms, id)?
                .map(|recorded| (recorded.change, recorded.done)),
        )
    }

    /// What became of `record`'s removal, as the removal record in this
    /// account's VMs folder says.
    ///
    /// # Errors
    ///
    /// Fails when the removal record cannot be read.
    pub fn removal(&self, record: &Record) -> Result<Removal> {
        let vms = self.vms();
        let recorded = crate::vm::removal::recorded(&vms, &record.id)?;
        Ok(match (&record.removal, recorded) {
            (None, None) | (Some(_), None) => Removal::None,
            (Some(change), Some(recorded)) if recorded.change == *change => {
                if recorded.done {
                    Removal::Done {
                        kept: recorded.kept,
                        passwords: recorded.passwords,
                        enrolment: recorded.enrolment,
                    }
                } else {
                    Removal::UnderWay
                }
            }
            (_, Some(_)) => Removal::Other,
        })
    }
}

#[cfg(test)]
#[path = "record_tests.rs"]
mod tests;
