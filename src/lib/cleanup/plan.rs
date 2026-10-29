//! The steps that undo what a task made, and the order they run in: check
//! that the files it collected are still kept, before anything is removed;
//! quit what runs from the task's own installs and copies; stop its
//! background jobs; undo its copies into the helper, newest first; remove
//! its installs, which a copy into the helper may have written into;
//! remove its copy folders. Each step names the steps it waits for.
use super::receipt::Entry;
use crate::{
    collect::ReceiptId, control::Token, daemon::protocol::JobId, host_session::Key, task::Owned,
};
use serde::{Deserialize, Serialize};

/// One step of a cleanup: what it does, and to which recorded thing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "kebab-case")]
pub(crate) enum Step {
    /// Check that every copy these collections kept still has its size and
    /// digest. It is checked again whenever the cleanup runs, and a step
    /// that would remove the source of a copy that is not kept is blocked.
    CheckOutputs { outputs: Vec<Collection> },
    /// Quit every process running from these bundles, chosen by where it
    /// runs from, never by its name.
    Quit { bundles: Vec<Bundle> },
    /// Stop one background job of a task by its identity; its name is for
    /// replies only.
    StopJob { job: JobId, task: Key, name: String },
    /// Undo one copy into the helper, by its plan ID.
    UndoPlan {
        #[serde(rename = "planId")]
        plan: Token,
    },
    /// Remove one app installed into the helper's Applications, by its
    /// install ID; its name is for replies only.
    RemoveInstall {
        #[serde(rename = "installId")]
        install: Token,
        app: String,
    },
    /// Remove one copy folder, by its token.
    RemoveCopy { copy: Token },
}

impl Step {
    /// Where a step of this kind runs among the others: lower first.
    fn rank(&self) -> u8 {
        match self {
            Self::CheckOutputs { .. } => 0,
            Self::Quit { .. } => 1,
            Self::StopJob { .. } => 2,
            Self::UndoPlan { .. } => PLANS,
            Self::RemoveInstall { .. } => 4,
            Self::RemoveCopy { .. } => 5,
        }
    }

    /// The thing this step removes or undoes, when it is one.
    pub fn owned(&self) -> Option<Owned> {
        match self {
            Self::CheckOutputs { .. } | Self::Quit { .. } | Self::StopJob { .. } => None,
            Self::UndoPlan { plan } => Some(Owned::Plan { id: plan.clone() }),
            Self::RemoveInstall { install, app } => Some(Owned::Install {
                id: install.clone(),
                app: app.clone(),
            }),
            Self::RemoveCopy { copy } => Some(Owned::Copy { id: copy.clone() }),
        }
    }
}

/// Where plans rank among the kinds of step.
const PLANS: u8 = 3;

/// A collection one of the tasks made: whose, and its receipt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Collection {
    pub task: Key,
    pub receipt: ReceiptId,
}

/// A background job one of the tasks started and that may still run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Job {
    pub id: JobId,
    pub task: Key,
    pub name: String,
}

/// What a cleanup covers beside the things the tasks claim: the jobs they
/// started and the collections they made.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Others {
    pub jobs: Vec<Job>,
    pub outputs: Vec<Collection>,
}

/// A bundle the task owns, which processes may run from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Bundle {
    /// An app installed into the helper's Applications, by its install ID.
    Install(Token),
    /// A copy folder, by its token.
    Copy(Token),
}

/// Something a task made that a cleanup undoes, and when it was claimed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Undoable {
    pub owned: Owned,
    /// When its claim was made or last settled, in milliseconds since 1970:
    /// plans are undone newest first by it.
    pub claimed: u64,
}

/// Add to `steps` (a receipt's, possibly empty) a step for each of `work`
/// that no step covers yet, each placed by the order steps run in: a new
/// quit before any new removal, new plans newest first ahead of those
/// already planned. Steps already planned keep their numbers and places,
/// and nothing a step already covers is planned twice. Then every step not
/// yet done or kept waits for what its kind waits for among all the steps,
/// old and new. Pure: it decides from what it is given only.
#[cfg(test)]
pub(crate) fn merge(steps: &mut Vec<Entry>, work: &[Undoable]) {
    merge_with(steps, work, &Others::default());
}

/// [`merge`], with a step to stop each of `others`' jobs that no step
/// covers yet, after the quits, and one check of every collection first.
/// A check that already ran is run again whenever another step is left to
/// run: a copy may have gone since.
pub(crate) fn merge_with(steps: &mut Vec<Entry>, work: &[Undoable], others: &Others) {
    add(steps, work);
    add_others(steps, others);
    rewire(steps);
}

/// Make every step not yet done or kept wait for every step it must: a
/// check of outputs and a quit for nothing, a job's stop for every quit, a
/// plan and a copy for every quit and every job's stop, an install for all
/// of those and every plan. Steps done or kept keep what they waited for,
/// and every step keeps its number.
fn rewire(steps: &mut [Entry]) {
    let quits = numbers(steps, |step| matches!(step, Step::Quit { .. }));
    let jobs = numbers(steps, |step| matches!(step, Step::StopJob { .. }));
    let plans = numbers(steps, |step| matches!(step, Step::UndoPlan { .. }));
    for entry in steps.iter_mut().filter(|entry| !entry.state.finished()) {
        entry.after = match entry.step {
            Step::CheckOutputs { .. } | Step::Quit { .. } => Vec::new(),
            Step::StopJob { .. } => quits.clone(),
            Step::UndoPlan { .. } | Step::RemoveCopy { .. } => {
                quits.iter().chain(&jobs).copied().collect()
            }
            Step::RemoveInstall { .. } => {
                quits.iter().chain(&jobs).chain(&plans).copied().collect()
            }
        };
    }
}

/// Add a step for each of `others` no step covers yet: one check of every
/// collection, first, and a stop of each job, after the quits.
fn add_others(steps: &mut Vec<Entry>, others: &Others) {
    let mut next = steps.iter().map(|entry| entry.n).max().unwrap_or(0) + 1;
    let mut number = || {
        let n = next;
        next += 1;
        n
    };

    if !others.outputs.is_empty() {
        let unfinished = steps.iter().any(|entry| {
            !entry.state.finished() && !matches!(entry.step, Step::CheckOutputs { .. })
        });
        let check = steps
            .iter_mut()
            .find(|entry| matches!(entry.step, Step::CheckOutputs { .. }));
        match check {
            Some(entry) => {
                if let Step::CheckOutputs { outputs } = &mut entry.step {
                    for collection in &others.outputs {
                        if !outputs.contains(collection) {
                            outputs.push(collection.clone());
                        }
                    }
                }
                if entry.state.finished() && unfinished {
                    entry.set(super::receipt::StepState::Pending, None);
                }
            }
            None => {
                let step = Step::CheckOutputs {
                    outputs: others.outputs.clone(),
                };
                steps.insert(0, Entry::new(number(), step, Vec::new()));
            }
        }
    }

    for job in &others.jobs {
        let covered = steps.iter().any(
            |entry| matches!(&entry.step, Step::StopJob { job: known, .. } if *known == job.id),
        );
        if !covered {
            let step = Step::StopJob {
                job: job.id.clone(),
                task: job.task.clone(),
                name: job.name.clone(),
            };
            place(steps, Entry::new(number(), step, Vec::new()));
        }
    }
}

fn add(steps: &mut Vec<Entry>, work: &[Undoable]) {
    let covered = |owned: &Owned, steps: &[Entry]| {
        steps
            .iter()
            .any(|entry| entry.step.owned().is_some_and(|known| known.same(owned)))
    };
    let mut new: Vec<&Undoable> = Vec::new();
    for undoable in work {
        let undone = !matches!(undoable.owned, Owned::Launch { .. });
        let twice = new.iter().any(|known| known.owned.same(&undoable.owned));
        if undone && !twice && !covered(&undoable.owned, steps) {
            new.push(undoable);
        }
    }
    if new.is_empty() {
        return;
    }

    let mut next = steps.iter().map(|entry| entry.n).max().unwrap_or(0) + 1;
    let mut number = || {
        let n = next;
        next += 1;
        n
    };

    let bundles: Vec<Bundle> = new
        .iter()
        .filter_map(|undoable| match &undoable.owned {
            Owned::Install { id, .. } => Some(Bundle::Install(id.clone())),
            Owned::Copy { id } => Some(Bundle::Copy(id.clone())),
            _ => None,
        })
        .collect();
    if !bundles.is_empty() {
        place(
            steps,
            Entry::new(number(), Step::Quit { bundles }, Vec::new()),
        );
    }
    let quits = numbers(steps, |step| matches!(step, Step::Quit { .. }));

    let mut plans: Vec<&&Undoable> = new
        .iter()
        .filter(|undoable| matches!(undoable.owned, Owned::Plan { .. }))
        .collect();
    plans.sort_by_key(|undoable| std::cmp::Reverse(undoable.claimed));
    let first_plan = steps
        .iter()
        .position(|entry| entry.step.rank() >= PLANS)
        .unwrap_or(steps.len());
    for (offset, undoable) in plans.into_iter().enumerate() {
        let Owned::Plan { id } = &undoable.owned else {
            continue;
        };
        let entry = Entry::new(number(), Step::UndoPlan { plan: id.clone() }, quits.clone());
        steps.insert(first_plan + offset, entry);
    }

    // An install waits for every plan too: a copy into the helper may have
    // written inside it.
    let before_installs = numbers(steps, |step| {
        matches!(step, Step::Quit { .. } | Step::UndoPlan { .. })
    });
    for undoable in &new {
        if let Owned::Install { id, app } = &undoable.owned {
            let step = Step::RemoveInstall {
                install: id.clone(),
                app: app.clone(),
            };
            place(steps, Entry::new(number(), step, before_installs.clone()));
        }
    }

    for undoable in &new {
        if let Owned::Copy { id } = &undoable.owned {
            let step = Step::RemoveCopy { copy: id.clone() };
            place(steps, Entry::new(number(), step, quits.clone()));
        }
    }
}

/// Put `entry` after every step that runs before or with it.
fn place(steps: &mut Vec<Entry>, entry: Entry) {
    let rank = entry.step.rank();
    let at = steps
        .iter()
        .rposition(|known| known.step.rank() <= rank)
        .map_or(0, |last| last + 1);
    steps.insert(at, entry);
}

/// The numbers of the steps `which` picks.
fn numbers(steps: &[Entry], which: impl Fn(&Step) -> bool) -> Vec<u32> {
    steps
        .iter()
        .filter(|entry| which(&entry.step))
        .map(|entry| entry.n)
        .collect()
}
