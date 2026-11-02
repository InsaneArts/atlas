//! A cleanup's receipt: one file per cleanup in the private output folder's
//! `cleanup` folder, written through to the disk before each step starts
//! and after it ends, so a run cut short is finished from it, never by
//! looking around the helper again. Each step keeps what it came to: the
//! files a copy into the helper left as the helper changed them, the files
//! an installed app wrote outside its bundle, and why a step failed. It
//! keeps tokens, IDs, app names and paths inside the helper's home, never
//! the name or path of a file on this Mac.
use super::plan::Step;
pub(super) use crate::environment::SideEffect;
use crate::{client::Actor, task::record::CleanupState};
use atlas_core::{
    error::Result,
    files::{Durability, Token},
    host::Key,
    record_folder::{LISTED, RecordFolder, Records},
    time::now_ms,
};
use serde::{Deserialize, Serialize};
use std::{path::Path, time::Duration};

/// The receipt format this build writes.
const VERSION: u32 = 1;
/// How long a finished receipt, or one nothing else names, is kept.
const KEEP: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// The most characters of a failure's words a step keeps.
const SAID: usize = 300;

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
pub(super) struct Receipt {
    pub(super) version: u32,
    pub(super) id: Token,
    /// The session whose work it cleans up.
    pub(super) session: Key,
    /// The one task it covers; none for the whole session.
    pub(super) task: Option<Key>,
    /// The session that took the work over, when it was another session's
    /// that had ended; none when the session cleans up its own, or when
    /// the person did through Atlas.app.
    pub(super) adopted_by: Option<Key>,
    /// Who ran it last.
    pub(super) actor: Actor,
    pub(super) started: u64,
    pub(super) updated: u64,
    pub(super) state: CleanupState,
    pub(super) steps: Vec<Entry>,
}

impl Receipt {
    /// A new, empty receipt of `session`'s work (or `task`'s), started now.
    pub(super) fn new(session: Key, task: Option<Key>, actor: Actor) -> Self {
        let now = now_ms();
        Self {
            version: VERSION,
            id: Token::new(),
            session,
            task,
            adopted_by: None,
            actor,
            started: now,
            updated: now,
            state: CleanupState::Incomplete,
            steps: Vec::new(),
        }
    }

    /// The overall state its steps add up to, worst first: uncertain when
    /// any step is, blocked when any is, incomplete while any has not been
    /// done or kept, complete otherwise.
    pub(super) fn summed(&self) -> CleanupState {
        if self
            .steps
            .iter()
            .any(|step| step.state == StepState::Uncertain)
        {
            CleanupState::Uncertain
        } else if self
            .steps
            .iter()
            .any(|step| step.state == StepState::Blocked && step.reason != Some(Reason::Uncertain))
        {
            CleanupState::Blocked
        } else if self.steps.iter().all(|step| step.state.finished()) {
            CleanupState::Complete
        } else {
            CleanupState::Incomplete
        }
    }

    /// The step numbered `n`.
    pub(super) fn step(&self, n: u32) -> Option<&Entry> {
        self.steps.iter().find(|step| step.n == n)
    }
}

/// One step of a receipt, as it was planned, how far it got and what it
/// came to.
/// Done/Kept describe the effect independently of `settled`: acknowledgement
/// may still need retry after either outcome. `already_gone` is a known Done
/// outcome; it never stands for uncertainty. `set` resets acknowledgement and
/// derives retry from the reason, while legacy missing fields keep their defaults.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Entry {
    pub(super) n: u32,
    #[serde(flatten)]
    pub(super) step: Step,
    /// The steps that must be done or kept before this one runs.
    pub(super) after: Vec<u32>,
    pub(super) state: StepState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) reason: Option<Reason>,
    /// Whether running the cleanup again may finish it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(super) retry: bool,
    /// Whether it was done because the thing was already gone.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(super) already_gone: bool,
    /// Whether the task records that claim its thing were told what became
    /// of it. A step done or kept but not yet settled is settled before
    /// anything is planned again.
    #[serde(default)]
    pub(super) settled: bool,
    /// What it came to, beyond its state.
    #[serde(flatten)]
    pub(super) came_to: CameTo,
    /// When it last changed, in milliseconds since 1970; 0 before it ran.
    #[serde(default)]
    pub(super) at: u64,
}

impl Entry {
    pub(super) fn new(n: u32, step: Step) -> Self {
        Self {
            n,
            step,
            after: Vec::new(),
            state: StepState::Pending,
            reason: None,
            retry: false,
            already_gone: false,
            settled: false,
            came_to: CameTo::default(),
            at: 0,
        }
    }

    /// Note that it got to `state` now, for `reason`, with nothing else
    /// known of it yet.
    pub(super) fn set(&mut self, state: StepState, reason: Option<Reason>) {
        self.state = state;
        self.retry = reason.is_some_and(Reason::retried) && state == StepState::Failed;
        self.reason = reason;
        self.already_gone = false;
        self.settled = false;
        self.came_to = CameTo::default();
        self.at = now_ms();
    }
}

/// What one step came to, beyond its state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CameTo {
    /// The files undoing a copy into the helper left as the helper changed
    /// them, by their path in its home.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) targets: Vec<String>,
    /// How many folders that copy made it left, as something else uses
    /// them.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub(super) retained_directories: usize,
    /// The files an installed app keeps outside its bundle that exist now,
    /// which are never removed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) side_effects: Vec<SideEffect>,
    /// Where in the helper an install was moved aside to, which keeps it
    /// for a while before deleting it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) held: Option<String>,
    /// The apps that may run from a copy macOS moved them to (App
    /// Translocation), which are never quit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) untraced: Vec<String>,
    /// Why it failed, in the helper's words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
    /// The files whose collected copies are missing or changed, by where
    /// they are in the helper: their sources are never removed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) not_kept: Vec<NotKept>,
    /// The processes that left a stopped job's group and still ran, which
    /// were never signalled.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) left: Vec<atlas_core::protocol::Left>,
}

/// A file in the helper whose collected copy is missing or changed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NotKept {
    /// Where it is in the helper.
    pub(super) source: String,
    /// Where it is relative to the helper's home, when it is in it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) in_home: Option<String>,
}

impl CameTo {
    /// A failure said in `words`, bounded.
    pub(super) fn failed(words: &str) -> Self {
        Self {
            error: Some(words.chars().take(SAID).collect()),
            ..Self::default()
        }
    }
}

fn is_zero(count: &usize) -> bool {
    *count == 0
}

/// How far one step got.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum StepState {
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
    pub(super) fn finished(self) -> bool {
        match self {
            Self::Done | Self::Kept => true,
            Self::Pending | Self::Started | Self::Failed | Self::Blocked | Self::Uncertain => false,
        }
    }
}

/// Why a step was kept, failed or blocked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Reason {
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
    /// Kept: the folder it would be moved aside to is on another volume,
    /// and Atlas never copies something there to delete it here.
    OtherVolume,
    /// Kept: Atlas cannot read its own record of it.
    Unreadable,
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
    /// Failed: a collected copy is missing or changed. Blocked: the step
    /// would remove the file that copy came from.
    OutputNotKept,
    /// Failed: a job has not ended yet; Atlas keeps stopping it.
    StillStopping,
    /// Failed: a job is no longer the process Atlas started.
    Identity,
    /// Failed: part of a job runs as another account.
    OtherAccount,
    /// Blocked: the helper could not be reached, or cannot keep jobs.
    Daemon,
    /// Blocked: an earlier action's outcome is unknown, so the step waits
    /// for recovery.
    Uncertain,
}

impl Reason {
    /// Whether a step that failed for this reason may succeed when run
    /// again.
    fn retried(self) -> bool {
        match self {
            Self::Timeout
            | Self::InUse
            | Self::Io
            | Self::Helper
            | Self::NeedsStep
            | Self::OutputNotKept
            | Self::StillStopping
            | Self::Uncertain => true,
            Self::Changed
            | Self::Replaced
            | Self::AnotherSession
            | Self::LabelUnreadable
            | Self::KeptTargets
            | Self::OtherVolume
            | Self::Unreadable
            | Self::Stopped
            | Self::Identity
            | Self::OtherAccount
            | Self::Daemon => false,
        }
    }
}

/// The receipts of one output folder.
pub(super) struct Receipts {
    folder: RecordFolder<Token>,
}

impl Receipts {
    pub(super) fn at(output: &Path) -> Self {
        Self {
            folder: RecordFolder::at(output, &RECEIPTS),
        }
    }

    /// One receipt, when it is there and reads.
    pub(super) fn load(&self, id: &Token) -> Option<Receipt> {
        self.folder
            .read_json::<Receipt>(id)
            .filter(|receipt| receipt.id == *id && receipt.version <= VERSION)
    }

    /// A complete bounded discovery of readable receipts for this session,
    /// including a run cut short before its task named the receipt. Opaque
    /// formats retain load's omission policy; physical scan failures refuse.
    pub(super) fn of_session(&self, session: &Key) -> Result<Vec<Receipt>> {
        self.folder.newest_complete(|id, bytes| {
            serde_json::from_slice::<Receipt>(bytes)
                .ok()
                .filter(|receipt| {
                    receipt.id == *id && receipt.version <= VERSION && receipt.session == *session
                })
        })
    }

    /// Write `receipt` whole, through to the disk, as its state is now.
    pub(super) fn save(&self, receipt: &mut Receipt) -> Result<()> {
        receipt.updated = now_ms();
        receipt.state = receipt.summed();
        self.folder.make()?;
        let _lock = self.folder.lock()?;
        self.folder.write_json(&receipt.id, receipt)
    }

    /// Remove the receipts older than a week that are finished, or that
    /// belong to a session `recorded` no longer knows of; the one `keep`,
    /// and any with a finished step its records have not yet acknowledged,
    /// stay.
    pub(super) fn prune(&self, keep: &Token, recorded: impl Fn(&Key) -> bool) {
        let Ok(_lock) = self.folder.lock() else {
            return;
        };
        let week = atlas_core::time::ms(KEEP);
        let now = now_ms();
        for id in self.folder.listed(LISTED) {
            let old = self.load(&id).is_some_and(|receipt| {
                let settled = receipt
                    .steps
                    .iter()
                    .all(|entry| !entry.state.finished() || entry.settled);
                now.saturating_sub(receipt.updated) > week
                    && settled
                    && (receipt.state == CleanupState::Complete || !recorded(&receipt.session))
            });
            if old && id != *keep {
                self.folder.remove(&id);
            }
        }
    }
}
