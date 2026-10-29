//! What the environment worker answers about the helper's copies and
//! installs, sent as JSON and read back on the person's side. Each read is
//! lenient: a field an older helper leaves out takes its default, and a
//! phase or reason this build does not know is kept as it came.
use super::{
    installs::InstallPhase,
    model::{Node, Operation, Phase},
};
use crate::task::record::Lenient;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// One file of a plan, by its target and what the plan does to it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileReport {
    pub target: PathBuf,
    pub operation: Lenient<Operation>,
    /// Whether the file it replaces was copied by an earlier format.
    #[serde(default)]
    pub legacy_previous: bool,
}

/// One file of a plan as `environment list` shows it: its target and
/// operation, and nothing else the helper sends.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ShownFile {
    pub target: PathBuf,
    pub operation: Lenient<Operation>,
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
    pub plan_id: String,
    pub phase: Lenient<Phase>,
    /// Whether it is the one that must be resumed or rolled back first.
    #[serde(default)]
    pub active: bool,
    /// When its record last changed, in milliseconds since 1970.
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default)]
    pub files: Vec<FileReport>,
    /// How many folders undoing it could not remove.
    #[serde(default)]
    pub retained_directories: usize,
}

/// The helper's list of its transactions: those listed, and how many were
/// hidden for being rolled back, left unread past the limit, or could not
/// be read.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Transactions {
    pub ok: bool,
    /// Each as the helper sent it; one this build cannot read is kept as
    /// it came, and counted as unreadable.
    #[serde(default)]
    pub transactions: Vec<Lenient<TransactionSummary>>,
    #[serde(default)]
    pub hidden: u64,
    #[serde(default)]
    pub more: u64,
    #[serde(default)]
    pub unreadable: u64,
}

impl Transactions {
    /// The transactions this build can read.
    pub fn known(&self) -> impl Iterator<Item = &TransactionSummary> {
        self.transactions
            .iter()
            .filter_map(|transaction| match transaction {
                Lenient::Known(transaction) => Some(transaction),
                Lenient::Later(_) => None,
            })
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
    pub install_id: String,
    pub phase: Lenient<InstallSeen>,
    /// Whether its destination holds the exact folder Atlas placed; not
    /// said of an install with no record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified: Option<bool>,
    /// What its record says, when it has one that reads.
    #[serde(flatten)]
    pub recorded: Option<InstalledApp>,
    /// Where it is and what runs from it, while it is verified.
    #[serde(flatten)]
    pub placed: Option<Placed>,
}

impl InstallState {
    /// Install `id` in `phase`, verified as `verified` says, with nothing
    /// more said of it.
    pub fn of(id: &str, phase: InstallSeen, verified: Option<bool>) -> Self {
        Self {
            install_id: id.to_owned(),
            phase: phase.into(),
            verified,
            recorded: None,
            placed: None,
        }
    }

    /// Whether the helper verified it is still the folder Atlas placed.
    pub fn verified(&self) -> bool {
        self.verified == Some(true)
    }
}

/// An install's app, as its record names it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InstalledApp {
    /// Its bundle's name, without `.app`.
    pub app: String,
    pub bundle_id: Option<String>,
}

/// A verified install: where it is, its folder's node, whether it is
/// unchanged, and what runs from it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Placed {
    pub path: PathBuf,
    pub root: Node,
    #[serde(default)]
    pub present: bool,
    /// None when printing it did not finish in time.
    #[serde(default)]
    pub unchanged: Option<bool>,
    /// The processes running from it.
    #[serde(default)]
    pub processes: Vec<RunningFrom>,
    /// The processes running from a copy macOS moved an app to that may
    /// be this install's: never signalled.
    #[serde(default)]
    pub unproven: Vec<RunningFrom>,
}

/// A process in the helper, by its PID and executable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RunningFrom {
    pub pid: u32,
    pub executable: String,
}

/// The reply to an install Atlas placed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Installed {
    pub ok: bool,
    pub installed: bool,
    pub install_id: String,
    pub app: String,
    pub path: PathBuf,
    pub bundle_id: Option<String>,
    pub version: Option<String>,
    /// Whether LaunchServices took it, so it opens by its name.
    pub registered: bool,
}

/// The reply to an install or removal that changed the helper and then
/// failed: one to finish, never a refusal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Unfinished {
    pub ok: bool,
    pub install_id: String,
    pub phase: InstallPhase,
    pub error: String,
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
    pub ok: bool,
    #[serde(default)]
    pub install_id: String,
    #[serde(flatten)]
    pub outcome: Outcome,
    /// Its bundle's name, without `.app`, when its record reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side_effects: Option<SideEffects>,
}

/// The files an installed app keeps outside its bundle that exist now.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SideEffects {
    #[serde(default)]
    pub kept: Vec<SideEffect>,
}

/// A file an installed app keeps outside its bundle, by its path in the
/// helper's home, and whether it was there before the install.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SideEffect {
    pub path: String,
    #[serde(default)]
    pub existed_before: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    /// `text` read as `T` and written again, as JSON with sorted keys.
    fn again<T: Serialize + serde::de::DeserializeOwned>(text: &str) -> String {
        let read: T = serde_json::from_str(text).unwrap();
        serde_json::to_string(&serde_json::to_value(read).unwrap()).unwrap()
    }

    #[test]
    fn each_reply_reads_back_as_it_was_sent() {
        let installs = [
            r#"{"app":"Probe","bundleId":"test.atlas.install-probe","installId":"ID","path":"/h/Applications/Probe.app","phase":"installed","present":true,"processes":[{"executable":"/h/Applications/Probe.app/Contents/MacOS/Probe","pid":812}],"root":{"device":1,"inode":2},"unchanged":null,"unproven":[],"verified":true}"#,
            r#"{"app":"Probe","bundleId":null,"installId":"ID","phase":"held","verified":false}"#,
            r#"{"installId":"ID","phase":"unreadable","verified":false}"#,
            r#"{"installId":"ID","phase":"missing"}"#,
        ];
        for text in installs {
            assert_eq!(again::<InstallState>(text), text);
        }
        let removals = [
            r#"{"app":"Probe","heldPath":"/h/held/ID/Probe.app","installId":"ID","ok":true,"outcome":"held","sideEffects":{"kept":[{"existedBefore":false,"path":"Library/Caches/x"}]}}"#,
            r#"{"app":"Probe","installId":"ID","ok":true,"outcome":"kept","reason":"other-volume","sideEffects":{"kept":[]}}"#,
            r#"{"installId":"ID","ok":true,"outcome":"kept","reason":"unreadable"}"#,
            r#"{"installId":"ID","ok":true,"outcome":"alreadyGone"}"#,
            r#"{"app":"Probe","installId":"ID","ok":true,"outcome":"inUse"}"#,
        ];
        for text in removals {
            assert_eq!(again::<Removal>(text), text);
        }
        let listed = r#"{"hidden":1,"more":0,"ok":true,"transactions":[{"active":true,"files":[{"legacyPrevious":false,"operation":"create","target":"a"}],"phase":"applied","planId":"P","retainedDirectories":0,"updatedAt":5}],"unreadable":0}"#;
        assert_eq!(again::<Transactions>(listed), listed);
    }

    #[test]
    fn a_phase_or_reason_this_build_does_not_know_is_never_taken_for_one_it_does() {
        let state: InstallState =
            serde_json::from_value(json!({"installId": "ID", "phase": "sideways"})).unwrap();
        assert_eq!(state.phase, Lenient::Later(json!("sideways")));
        assert!(!state.verified());

        let removal: Removal =
            serde_json::from_value(json!({"ok": true, "outcome": "kept", "reason": "new"}))
                .unwrap();
        assert_eq!(
            removal.outcome,
            Outcome::Kept {
                reason: KeptReason::Changed
            }
        );

        let listed: Transactions = serde_json::from_value(json!({"ok": true, "transactions": [
            {"planId": "P", "phase": "sideways"},
            {"phase": "applied"},
        ]}))
        .unwrap();
        assert_eq!(listed.known().count(), 1);
        assert!(matches!(
            listed.transactions[1],
            Lenient::Later(Value::Object(_))
        ));
    }
}
