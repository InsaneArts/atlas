//! A cleanup's receipt: one file per cleanup in the private output folder's
//! `cleanup` folder, written through to the disk before each step starts
//! and after it ends, so a run cut short is finished from it, never by
//! looking around the helper again. It keeps tokens, IDs, app names and
//! reasons, never a file's name or path.
use super::plan::Step;
use crate::{
    control::{Durability, Token},
    error::Result,
    host_session::{Key, now_ms},
    record_folder::{RecordFolder, Records},
    task::record::CleanupState,
};
use serde::{Deserialize, Serialize};
use std::{path::Path, time::Duration};

/// The receipt format this build writes.
const VERSION: u32 = 1;
/// How long a complete receipt is kept.
const KEEP: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// How receipts are kept: through to the disk, and waited for longer than
/// session records are, as a cleanup holds control while it writes them.
static RECEIPTS: Records = Records {
    folder: "cleanup",
    noun: "cleanup",
    lock_wait: Duration::from_secs(2),
    durability: Durability::Disk,
    bytes: 512 * 1024,
};

/// One cleanup of a session's work, or of one task's.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Receipt {
    pub version: u32,
    pub id: Token,
    /// The session whose work it cleans up.
    pub session: Key,
    /// The one task it covers; none for the whole session.
    pub task: Option<Key>,
    /// The session that took the work over, when it was another session's
    /// that had ended; none when the session cleans up its own, or when
    /// the person did through Atlas.app.
    pub adopted_by: Option<Key>,
    /// Who ran it last: "agent" or "viewer".
    pub actor: String,
    pub started: u64,
    pub updated: u64,
    pub state: CleanupState,
    pub steps: Vec<Entry>,
}

impl Receipt {
    /// A new, empty receipt of `session`'s work (or `task`'s), started now.
    pub fn new(session: Key, task: Option<Key>, actor: &str, adopted_by: Option<Key>) -> Self {
        let now = now_ms();
        Self {
            version: VERSION,
            id: Token::new(),
            session,
            task,
            adopted_by,
            actor: actor.to_owned(),
            started: now,
            updated: now,
            state: CleanupState::Incomplete,
            steps: Vec::new(),
        }
    }

    /// The overall state its steps add up to, worst first: uncertain when
    /// any step is, blocked when any is, incomplete while any has not been
    /// done or kept, complete otherwise.
    pub fn summed(&self) -> CleanupState {
        let any = |state: StepState| self.steps.iter().any(|step| step.state == state);
        if any(StepState::Uncertain) {
            CleanupState::Uncertain
        } else if any(StepState::Blocked) {
            CleanupState::Blocked
        } else if self.steps.iter().all(|step| step.state.finished()) {
            CleanupState::Complete
        } else {
            CleanupState::Incomplete
        }
    }

    /// The step numbered `n`.
    pub fn step(&self, n: u32) -> Option<&Entry> {
        self.steps.iter().find(|step| step.n == n)
    }
}

/// One step of a receipt, as it was planned and how far it got.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Entry {
    pub n: u32,
    #[serde(flatten)]
    pub step: Step,
    /// The steps that must be done or kept before this one runs.
    pub after: Vec<u32>,
    pub state: StepState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<Reason>,
    /// Whether running the cleanup again may finish it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub retry: bool,
    /// Whether it was done because the thing was already gone.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub already_gone: bool,
    /// When it last changed, in milliseconds since 1970; 0 before it ran.
    #[serde(default)]
    pub at: u64,
}

impl Entry {
    pub fn new(n: u32, step: Step, after: Vec<u32>) -> Self {
        Self {
            n,
            step,
            after,
            state: StepState::Pending,
            reason: None,
            retry: false,
            already_gone: false,
            at: 0,
        }
    }

    /// Note that it got to `state` now, for `reason`.
    pub fn set(&mut self, state: StepState, reason: Option<Reason>) {
        self.state = state;
        self.retry = reason.is_some_and(Reason::retried) && state == StepState::Failed;
        self.reason = reason;
        self.already_gone = false;
        self.at = now_ms();
    }
}

/// How far one step got.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum StepState {
    Pending,
    /// Written before it acts: a run cut short here runs it again, checking
    /// first.
    Started,
    Done,
    /// Left on purpose: Atlas chose not to remove the thing.
    Kept,
    /// Tried, and could not.
    Failed,
    /// Not run: the person stopped agents.
    Blocked,
    /// Its outcome is not known.
    Uncertain,
}

impl StepState {
    /// Whether nothing is left to do for it.
    pub fn finished(self) -> bool {
        matches!(self, Self::Done | Self::Kept)
    }
}

/// Why a step was kept, failed or blocked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Reason {
    /// Kept: it changed since Atlas made it.
    Changed,
    /// Kept: something else is where Atlas put it.
    Replaced,
    /// Kept: another session made it.
    AnotherSession,
    /// Kept: whose it is cannot be read.
    LabelUnreadable,
    /// Kept: undoing a copy left files the helper changed since.
    KeptTargets,
    /// Failed: what it waited for did not end in time.
    Timeout,
    /// Failed: something still runs from it.
    InUse,
    /// Failed: the file system refused.
    Io,
    /// Failed: the helper could not do it.
    Helper,
    /// Failed: a step it waits for is not done.
    NeedsStep,
    /// Blocked: the person stopped agents.
    Stopped,
}

impl Reason {
    /// Whether a step that failed for this reason may succeed when run
    /// again.
    fn retried(self) -> bool {
        matches!(
            self,
            Self::Timeout | Self::InUse | Self::Io | Self::Helper | Self::NeedsStep
        )
    }

    /// The reason's name, as replies and receipts give it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Changed => "changed",
            Self::Replaced => "replaced",
            Self::AnotherSession => "another-session",
            Self::LabelUnreadable => "label-unreadable",
            Self::KeptTargets => "kept-targets",
            Self::Timeout => "timeout",
            Self::InUse => "in-use",
            Self::Io => "io",
            Self::Helper => "helper",
            Self::NeedsStep => "needs-step",
            Self::Stopped => "stopped",
        }
    }
}

/// The receipts of one output folder.
pub(crate) struct Receipts {
    folder: RecordFolder<Token>,
}

impl Receipts {
    pub fn at(output: &Path) -> Self {
        Self {
            folder: RecordFolder::at(output, &RECEIPTS),
        }
    }

    /// One receipt, when it is there and reads.
    pub fn load(&self, id: &Token) -> Option<Receipt> {
        self.folder
            .read(id)
            .and_then(|bytes| serde_json::from_slice::<Receipt>(&bytes).ok())
            .filter(|receipt| receipt.id == *id && receipt.version <= VERSION)
    }

    /// Write `receipt` whole, through to the disk, as its state is now.
    pub fn save(&self, receipt: &mut Receipt) -> Result<()> {
        receipt.updated = now_ms();
        receipt.state = receipt.summed();
        self.folder.make()?;
        let _lock = self.folder.lock()?;
        self.folder
            .write(&receipt.id, &serde_json::to_vec(receipt)?)
    }

    /// Remove the complete receipts older than a week; the one `keep` and
    /// every unfinished one stay.
    pub fn prune(&self, keep: &Token) {
        let Ok(_lock) = self.folder.lock() else {
            return;
        };
        let week = u64::try_from(KEEP.as_millis()).unwrap_or(u64::MAX);
        let now = now_ms();
        for id in self.folder.listed(crate::record_folder::LISTED) {
            let old = self.load(&id).is_some_and(|receipt| {
                receipt.state == CleanupState::Complete
                    && now.saturating_sub(receipt.updated) > week
            });
            if old && id != *keep {
                self.folder.remove(&id);
            }
        }
    }
}
