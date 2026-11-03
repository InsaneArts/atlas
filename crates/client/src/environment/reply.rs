//! What the environment worker answers about the helper's copies and
//! installs, sent as JSON and read back on the person's side. Each read is
//! lenient: a field an older helper leaves out takes its default, and a
//! phase or reason this build does not know is kept as it came.
use super::{
    installs::InstallPhase,
    model::{Node, Operation, Phase},
};
use atlas_core::lenient::Lenient;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

/// The facts orchestration reads from a worker's reply; the reply itself
/// stays for whoever is shown it. Optional fields retain the difference
/// between explicit proof and a missing or malformed field.
pub(super) struct WorkerReply {
    pub(super) ok: Option<bool>,
    pub(super) phase: Option<Phase>,
    pub(super) error: String,
    pub(super) resume_required: Option<bool>,
    pub(super) kept_targets: Vec<String>,
    pub(super) retained_directories: usize,
}

impl WorkerReply {
    pub(super) fn read(reply: &Value) -> Self {
        let phase = reply["phase"]
            .as_str()
            .and_then(|_| Phase::deserialize(&reply["phase"]).ok());
        Self {
            ok: reply["ok"].as_bool(),
            phase,
            error: reply["error"]
                .as_str()
                .unwrap_or("The helper did not finish undoing this copy")
                .to_owned(),
            resume_required: reply["resumeRequired"].as_bool(),
            kept_targets: reply["keptTargets"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|target| target.as_str().map(str::to_owned))
                .collect(),
            retained_directories: reply["retainedDirectories"].as_array().map_or(0, Vec::len),
        }
    }

    /// Whether the helper confirmed the transaction reached `phase` with
    /// nothing left to resume.
    pub(super) fn settled(&self, phase: Phase) -> bool {
        self.ok == Some(true) && self.phase == Some(phase) && self.resume_required == Some(false)
    }

    /// Whether the helper said it never started this plan.
    pub(super) fn never_started(&self) -> bool {
        self.ok == Some(false) && self.error == super::worker::MISSING
    }
}

/// One file of a plan, by its target and what the plan does to it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileReport {
    pub(crate) target: PathBuf,
    pub(crate) operation: Lenient<Operation>,
    /// Kept on the wire; always false, as no record has an earlier format.
    #[serde(default)]
    pub(crate) legacy_previous: bool,
}

/// One file of a plan as `environment list` shows it: its target and
/// operation, and nothing else the helper sends.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ShownFile {
    pub(crate) target: PathBuf,
    pub(crate) operation: Lenient<Operation>,
}

impl From<&FileReport> for ShownFile {
    fn from(file: &FileReport) -> Self {
        Self {
            target: file.target.clone(),
            operation: file.operation.clone(),
        }
    }
}

/// One transaction in the helper, as its list names it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransactionSummary {
    pub(crate) plan_id: String,
    pub(crate) phase: Lenient<Phase>,
    /// Whether it is the one that must be resumed or rolled back first.
    #[serde(default)]
    pub(crate) active: bool,
    /// When its record last changed, in milliseconds since 1970.
    #[serde(default)]
    pub(crate) updated_at: u64,
    #[serde(default)]
    pub(crate) files: Vec<FileReport>,
    /// How many folders undoing it could not remove.
    #[serde(default)]
    pub(crate) retained_directories: usize,
}

/// The helper's list of its transactions: those listed, and how many were
/// hidden for being rolled back, left unread past the limit, or could not
/// be read.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Transactions {
    pub(crate) ok: bool,
    /// Each as the helper sent it; one this build cannot read is kept as
    /// it came, and counted as unreadable.
    #[serde(default)]
    pub(crate) transactions: Vec<Lenient<TransactionSummary>>,
    #[serde(default)]
    pub(crate) hidden: u64,
    #[serde(default)]
    pub(crate) more: u64,
    #[serde(default)]
    pub(crate) unreadable: u64,
}

impl Transactions {
    /// The transactions this build can read.
    pub(super) fn known(&self) -> impl Iterator<Item = &TransactionSummary> {
        self.transactions
            .iter()
            .filter_map(|transaction| match transaction {
                Lenient::Known(transaction) => Some(transaction),
                Lenient::Later(_) => None,
            })
    }

    /// Whether the helper and this build read every transaction the helper
    /// looked at.
    pub(super) fn all_read(&self) -> bool {
        self.unreadable == 0 && self.known().count() == self.transactions.len()
    }
}

/// How far an install got, as a read of the helper's installs says: none
/// is recorded, its record cannot be read, or the phase its record names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum InstallSeen {
    Missing,
    Unreadable,
    #[serde(untagged)]
    Recorded(InstallPhase),
}

/// How one install stands, as the helper's read of its installs says.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InstallState {
    pub(crate) install_id: String,
    pub(crate) phase: Lenient<InstallSeen>,
    /// Whether its destination holds the exact folder Atlas placed; not
    /// said of an install with no record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) verified: Option<bool>,
    /// What its record says, when it has one that reads.
    #[serde(flatten)]
    pub(crate) recorded: Option<InstalledApp>,
    /// Where it is and what runs from it, while it is verified.
    #[serde(flatten)]
    pub(crate) placed: Option<Placed>,
}

impl InstallState {
    /// Install `id` in `phase`, verified as `verified` says, with nothing
    /// more said of it.
    pub(crate) fn of(id: &str, phase: InstallSeen, verified: Option<bool>) -> Self {
        Self {
            install_id: id.to_owned(),
            phase: phase.into(),
            verified,
            recorded: None,
            placed: None,
        }
    }

    /// Whether the helper verified it is still the folder Atlas placed.
    pub(crate) fn verified(&self) -> bool {
        self.verified == Some(true)
    }
}

/// An install's app, as its record names it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InstalledApp {
    /// Its bundle's name, without `.app`.
    pub(crate) app: String,
    pub(crate) bundle_id: Option<String>,
}

/// A verified install: where it is, its folder's node, whether it is
/// unchanged, and what runs from it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Placed {
    pub(crate) path: PathBuf,
    pub(crate) root: Node,
    #[serde(default)]
    pub(crate) present: bool,
    /// None when printing it did not finish in time.
    #[serde(default)]
    pub(crate) unchanged: Option<bool>,
    /// The processes running from it.
    #[serde(default)]
    pub(crate) processes: Vec<RunningFrom>,
    /// The processes running from a copy macOS moved an app to that may
    /// be this install's: never signalled.
    #[serde(default)]
    pub(crate) unproven: Vec<RunningFrom>,
}

/// A process in the helper, by its PID and executable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RunningFrom {
    pub(crate) pid: u32,
    pub(crate) executable: String,
}

/// The reply to an install Atlas placed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Installed {
    pub(crate) ok: bool,
    pub(crate) installed: bool,
    pub(crate) install_id: String,
    pub(crate) app: String,
    pub(crate) path: PathBuf,
    pub(crate) bundle_id: Option<String>,
    pub(crate) version: Option<String>,
    /// Whether LaunchServices took it, so it opens by its name.
    pub(crate) registered: bool,
}

/// The reply to an install or removal that changed the helper and then
/// failed: one to finish, never a refusal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Unfinished {
    pub(crate) ok: bool,
    pub(crate) install_id: String,
    pub(crate) phase: InstallPhase,
    pub(crate) error: String,
}

/// Why an install was left in place.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum KeptReason {
    /// Something else is at its destination.
    Replaced,
    /// It could not be moved aside without copying.
    OtherVolume,
    /// Its record cannot be read.
    Unreadable,
    /// It changed since Atlas placed it. A reason this build does not
    /// know reads as this one, so it is never taken away.
    #[default]
    #[serde(other)]
    Changed,
}

/// What taking an install away came to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub(crate) enum Outcome {
    /// Moved aside among the held installs, to this path.
    #[serde(rename_all = "camelCase")]
    Held {
        held_path: PathBuf,
    },
    AlreadyGone,
    Kept {
        #[serde(default)]
        reason: KeptReason,
    },
    InUse,
}

/// The reply to taking an install away: what became of the bundle, and,
/// apart from it, which files keyed by its identifier exist now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Removal {
    pub(crate) ok: bool,
    #[serde(default)]
    pub(crate) install_id: String,
    #[serde(flatten)]
    pub(crate) outcome: Outcome,
    /// Its bundle's name, without `.app`, when its record reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) app: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) side_effects: Option<SideEffects>,
}

/// What the helper answered to taking an install away.
pub(crate) enum RemovalReply {
    Finished(Removal),
    /// It did not finish, in its own words.
    Unfinished(String),
    /// It said it finished, in a shape this build cannot read.
    Unread,
}

impl RemovalReply {
    pub(crate) fn read(reply: Value) -> Self {
        if reply["ok"] != true {
            let said = reply["error"]
                .as_str()
                .unwrap_or("The helper did not finish moving the app aside");
            return Self::Unfinished(said.to_owned());
        }
        serde_json::from_value(reply).map_or(Self::Unread, Self::Finished)
    }
}

/// The files an installed app keeps outside its bundle that exist now.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SideEffects {
    #[serde(default)]
    pub(crate) kept: Vec<SideEffect>,
}

/// A file an installed app keeps outside its bundle, by its path in the
/// helper's home, and whether it was there before the install.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SideEffect {
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) existed_before: bool,
}

#[cfg(test)]
mod tests;
