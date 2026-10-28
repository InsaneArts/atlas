//! The steps that undo what a task made, and the order they run in: quit
//! what runs from the task's own installs and copies; undo its copies into
//! the helper, newest first; remove its installs, which a copy into the
//! helper may have written into; remove its copy folders. Each step names
//! the steps it waits for. Steps for background jobs and collected outputs
//! slot into this order as kinds of their own, before the copies into the
//! helper are undone, without changing these.
use super::receipt::Entry;
use crate::{control::Token, task::Owned};
use serde::{Deserialize, Serialize};

/// One step of a cleanup: what it does, and to which recorded thing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "kebab-case")]
pub(crate) enum Step {
    /// Quit every process running from these bundles, chosen by where it
    /// runs from, never by its name.
    Quit { bundles: Vec<Bundle> },
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
            Self::Quit { .. } => 0,
            Self::UndoPlan { .. } => 1,
            Self::RemoveInstall { .. } => 2,
            Self::RemoveCopy { .. } => 3,
        }
    }

    /// The thing this step removes or undoes, when it is one.
    pub fn owned(&self) -> Option<Owned> {
        match self {
            Self::Quit { .. } => None,
            Self::UndoPlan { plan } => Some(Owned::Plan { id: plan.clone() }),
            Self::RemoveInstall { install, app } => Some(Owned::Install {
                id: install.clone(),
                app: app.clone(),
            }),
            Self::RemoveCopy { copy } => Some(Owned::Copy { id: copy.clone() }),
        }
    }
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
/// and nothing a step already covers is planned twice. Pure: it decides
/// from what it is given only.
pub(crate) fn merge(steps: &mut Vec<Entry>, work: &[Undoable]) {
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
        .position(|entry| entry.step.rank() >= 1)
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
