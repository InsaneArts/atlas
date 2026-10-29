//! Running a receipt's steps in order. Each step reads only the thing it
//! names, checks that thing's own state first and then acts, so a step cut
//! short is simply run again: what had finished counts as done, and what
//! had not is tried again with every proof. What each step came to is
//! kept in the receipt itself. Nothing here lists the copy root or the
//! helper's stores.
use super::{
    plan::{Bundle, Collection, Job, Step},
    receipt::{CameTo, NotKept, Reason, Receipt, StepState},
    targets::{install_in, installs_of, targets},
};
use crate::{
    app_selection::Process,
    collect::{Intact, Kept},
    daemon::protocol::{JobId, JobRefusal, JobState, Left, Shown},
    environment::{InstallState, KeptReason, Outcome as Removed, PlanState, Removal, Undone},
    error::{Error, Result},
    files::Token,
    handoff::{self, CopyRemoval, Running},
    host_session::Key,
};
use serde_json::Value;
use std::path::Path;

/// How waiting for a quit step's processes ended.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Quitted {
    Gone,
    /// Some were still running when the wait ran out, as said.
    TimedOut(String),
}

/// What a cleanup needs of the helper; tests stand in for it.
pub(crate) trait Hands {
    /// Where `open`'s copy folders are.
    fn copies(&self) -> &Path;
    /// The helper's running apps, from a list it gave.
    async fn apps(&self) -> Result<Value>;
    /// How far each of `ids` got, read without control.
    async fn plan_states(&self, ids: &[Token]) -> Result<Vec<PlanState>>;
    /// How each of `ids` stands in the helper, read without control: where
    /// it is while its folder is the one Atlas placed, whether it is
    /// unchanged, and what runs from it.
    async fn installs(&self, ids: &[Token]) -> Result<Vec<InstallState>>;
    /// Signal `processes`, each only while it runs from the folder it was
    /// proven to, and wait until no app in a fresh list runs from any of
    /// `folders`, nor any process from `installs`.
    async fn quit(
        &self,
        processes: &[Process],
        folders: &[String],
        installs: &[Token],
    ) -> Result<Quitted>;
    /// Undo one copy into the helper, under the control the cleanup holds.
    async fn undo_plan(&self, plan: &Token) -> Result<Undone>;
    /// Remove one install, when it is unchanged and nothing runs from it,
    /// under the control the cleanup holds.
    async fn remove_install(&self, install: &Token) -> Result<Value>;
    /// Whether each copy `collection` kept still has its size and digest.
    fn intact(&self, collection: &Collection) -> Intact;
    /// Background job `id` as the helper shows it now, read without
    /// control.
    async fn job(&self, id: &JobId) -> Result<Shown>;
    /// The jobs the helper's own records give to `session`'s work (to
    /// `task` only, when there is one) that may still run, so a job its
    /// task's record never noted is stopped too. None when the helper
    /// cannot say.
    async fn jobs_of(&self, session: &Key, task: Option<&Key>) -> Result<Vec<Job>>;
    /// Stop job `id` of `task` by its identity, under the control the
    /// cleanup holds, and wait for it to end: the helper's word on it then.
    async fn stop_job(&self, task: &Key, id: &JobId) -> Result<Value>;
    /// Whether the person has stopped agents.
    fn stopped(&self) -> bool;
    /// Whether an earlier action's outcome is unknown: only steps that
    /// recover nothing and change no app run until it is recovered.
    fn unknown(&self) -> bool;
    /// Whether a signal asked this command to stop.
    fn interrupted(&self) -> bool;
}

/// Run `receipt`'s steps that are not done or kept, in order, for a
/// cleanup of `session`'s work, calling `save` before each step starts and
/// after it ends, and `settled` with each step that removed, undid or kept
/// its thing, which says whether its task records took it. A Stop between
/// steps blocks the rest; a signal leaves them pending; a step whose
/// outcome is not known ends the run.
pub(crate) async fn run(
    hands: &impl Hands,
    receipt: &mut Receipt,
    session: &Key,
    mut save: impl FnMut(&mut Receipt) -> Result<()>,
    mut settled: impl FnMut(&Step, StepState) -> bool,
) -> Result<()> {
    for place in 0..receipt.steps.len() {
        if receipt.steps[place].state.finished() {
            continue;
        }
        if hands.interrupted() {
            break;
        }
        if hands.stopped() {
            for entry in receipt.steps[place..]
                .iter_mut()
                .filter(|entry| !entry.state.finished())
            {
                entry.set(StepState::Blocked, Some(Reason::Stopped));
            }
            save(receipt)?;
            break;
        }

        // While an outcome is unknown, only background jobs are stopped and
        // collected copies checked; the rest waits for recovery.
        let contained = matches!(
            receipt.steps[place].step,
            Step::CheckOutputs { .. } | Step::StopJob { .. } | Step::FindJobs
        );
        if hands.unknown() && !contained {
            receipt.steps[place].set(StepState::Blocked, Some(Reason::Uncertain));
            save(receipt)?;
            continue;
        }

        let waiting = !hands.unknown()
            && receipt.steps[place]
                .after
                .iter()
                .any(|n| receipt.step(*n).is_none_or(|entry| !entry.state.finished()));
        if waiting {
            receipt.steps[place].set(StepState::Failed, Some(Reason::NeedsStep));
            save(receipt)?;
            continue;
        }

        let step = receipt.steps[place].step.clone();
        let not_kept = not_kept(receipt);
        if !not_kept.is_empty() && removes_any(hands, &step, &not_kept).await {
            receipt.steps[place].set(StepState::Blocked, Some(Reason::OutputNotKept));
            save(receipt)?;
            continue;
        }

        receipt.steps[place].set(StepState::Started, None);
        save(receipt)?;
        let outcome = match step {
            Step::FindJobs => find_jobs(hands, receipt, session).await,
            _ => act(hands, &step, session).await,
        };
        let entry = &mut receipt.steps[place];
        entry.set(outcome.state, outcome.reason);
        entry.already_gone = outcome.already_gone;
        entry.came_to = outcome.came_to;
        save(receipt)?;
        if outcome.state.finished() {
            receipt.steps[place].settled = settled(&step, outcome.state);
            save(receipt)?;
        }
        if outcome.state == StepState::Uncertain {
            break;
        }
    }
    Ok(())
}

/// What one step came to.
struct Outcome {
    state: StepState,
    reason: Option<Reason>,
    already_gone: bool,
    came_to: CameTo,
}

impl Outcome {
    fn done() -> Self {
        Self {
            state: StepState::Done,
            reason: None,
            already_gone: false,
            came_to: CameTo::default(),
        }
    }

    fn gone() -> Self {
        Self {
            already_gone: true,
            ..Self::done()
        }
    }

    fn kept(reason: Reason) -> Self {
        Self {
            state: StepState::Kept,
            reason: Some(reason),
            ..Self::done()
        }
    }

    fn failed(reason: Reason) -> Self {
        Self {
            state: StepState::Failed,
            reason: Some(reason),
            ..Self::done()
        }
    }

    /// `self`, having come to `came_to`.
    fn with(self, came_to: CameTo) -> Self {
        Self { came_to, ..self }
    }

    /// A step that could not be carried out because of `error`: uncertain
    /// when its outcome is not known, otherwise a failure for `reason`.
    fn erred(error: &Error, reason: Reason) -> Self {
        if error.uncertain {
            Self {
                state: StepState::Uncertain,
                reason: None,
                ..Self::done()
            }
        } else {
            Self::failed(reason).with(CameTo::failed(&error.error))
        }
    }
}

/// The files whose collected copies a check of outputs in `receipt` found
/// missing or changed.
fn not_kept(receipt: &Receipt) -> Vec<NotKept> {
    receipt
        .steps
        .iter()
        .filter(|entry| matches!(entry.step, Step::CheckOutputs { .. }))
        .flat_map(|entry| entry.came_to.not_kept.iter().cloned())
        .collect()
}

/// Whether `step` would remove, roll back or uninstall any of `not_kept`:
/// a plan whose targets hold one, an install whose folder holds one. One
/// whose reach cannot be read is taken to hold them all, and so is a
/// source that is not known.
async fn removes_any(hands: &impl Hands, step: &Step, not_kept: &[NotKept]) -> bool {
    let unknown = not_kept.iter().any(|file| file.source.is_empty());
    match step {
        Step::UndoPlan { plan } => {
            let Ok(states) = hands.plan_states(std::slice::from_ref(plan)).await else {
                return true;
            };
            unknown
                || states
                    .iter()
                    .flat_map(|state| &state.targets)
                    .any(|target| {
                        not_kept.iter().any(|file| {
                            file.in_home
                                .as_deref()
                                .is_some_and(|inside| within(inside, target))
                        })
                    })
        }
        Step::RemoveInstall { install, .. } => {
            let Ok(answer) = hands.installs(std::slice::from_ref(install)).await else {
                return true;
            };
            let Some(placed) = install_in(&answer, install).and_then(|found| found.placed.as_ref())
            else {
                return unknown;
            };
            let folder = placed.path.to_string_lossy();
            unknown || not_kept.iter().any(|file| within(&file.source, &folder))
        }
        _ => false,
    }
}

/// Whether `path` is `folder` or lies inside it, compared in any case, as
/// the helper's file system compares names.
fn within(path: &str, folder: &str) -> bool {
    let folder = folder.trim_end_matches('/');
    let (path, folder) = (path.to_lowercase(), folder.to_lowercase());
    path == folder
        || path
            .strip_prefix(&folder)
            .is_some_and(|rest| rest.starts_with('/'))
}

async fn act(hands: &impl Hands, step: &Step, session: &Key) -> Outcome {
    match step {
        Step::CheckOutputs { outputs } => check_outputs(hands, outputs),
        // Listed by the run itself, which knows the receipt's job stops.
        Step::FindJobs => Outcome::failed(Reason::NeedsStep),
        Step::StopJob { job, task, .. } => stop_job(hands, task, job).await,
        Step::Quit { bundles } => quit(hands, bundles, session).await,
        Step::UndoPlan { plan } => match hands.undo_plan(plan).await {
            Ok(Undone::AlreadyGone) => Outcome::gone(),
            Ok(Undone::Discarded | Undone::RolledBack) => Outcome::done(),
            Ok(Undone::Kept {
                targets,
                directories,
            }) => Outcome::kept(Reason::KeptTargets).with(CameTo {
                targets,
                retained_directories: directories,
                ..CameTo::default()
            }),
            Ok(Undone::Unfinished(said)) => {
                Outcome::failed(Reason::Helper).with(CameTo::failed(&said))
            }
            Err(error) => Outcome::erred(&error, Reason::Helper),
        },
        Step::RemoveInstall { install, .. } => remove_install(hands, install).await,
        Step::RemoveCopy { copy } => remove_copy(hands, copy, session).await,
    }
}

/// List the helper's own jobs of `receipt`'s tasks again: done once the
/// list is read and a step of `receipt` stops each job on it; failed,
/// saying why, while it cannot be read or names a job no step stops yet,
/// which the next cleanup plans.
async fn find_jobs(hands: &impl Hands, receipt: &Receipt, session: &Key) -> Outcome {
    let listed = match hands.jobs_of(session, receipt.task.as_ref()).await {
        Ok(listed) => listed,
        Err(error) => {
            return Outcome::failed(Reason::Daemon).with(CameTo::failed(&format!(
                "The helper's list of this task's background jobs could not be read ({}), so \
                 nothing that may hold one was removed",
                error.error
            )));
        }
    };
    let unstopped: Vec<&Job> = listed
        .iter()
        .filter(|job| {
            !receipt.steps.iter().any(
                |entry| matches!(&entry.step, Step::StopJob { job: known, .. } if *known == job.id),
            )
        })
        .collect();
    if unstopped.is_empty() {
        return Outcome::done();
    }
    let names: Vec<&str> = unstopped.iter().map(|job| job.name.as_str()).collect();
    Outcome::failed(Reason::NeedsStep).with(CameTo::failed(&format!(
        "The helper lists background jobs no step stops yet ({}); run atlas cleanup --apply again",
        names.join(", ")
    )))
}

/// Check that every copy `outputs` kept is still as collected: done when
/// all are; otherwise failed, saying which files' copies are missing or
/// changed, so no later step removes those files.
fn check_outputs(hands: &impl Hands, outputs: &[Collection]) -> Outcome {
    let found: Vec<(&Collection, Intact)> = outputs
        .iter()
        .map(|collection| (collection, hands.intact(collection)))
        .collect();

    let mut not_kept = Vec::new();
    let mut said = Vec::new();
    for (collection, intact) in &found {
        // A receipt that is gone leaves which files it kept unknown: every
        // step that removes files in the helper waits until it is known.
        if intact.unknown {
            not_kept.push(NotKept {
                source: String::new(),
                in_home: None,
            });
            said.push(format!(
                "the receipt of collection {} is gone, so Atlas cannot tell which files it kept",
                collection.receipt
            ));
        }
    }

    // Each version of a file that was collected, from every collection.
    let mut versions: Vec<Version> = Vec::new();
    for (collection, intact) in &found {
        let at = collected_at(collection);
        for (files, how) in [
            (&intact.kept, None),
            (&intact.missing, Some("is gone")),
            (&intact.changed, Some("changed")),
        ] {
            versions.extend(files.iter().map(|file| Version {
                file,
                how,
                span: file.read.filter(|span| span.trusted(at)),
            }));
        }
    }
    let mut sources: Vec<&str> = Vec::new();
    for version in &versions {
        if !sources.contains(&version.file.source.as_str()) {
            sources.push(&version.file.source);
        }
    }
    for source in sources {
        let of: Vec<&Version> = versions
            .iter()
            .filter(|version| version.file.source == source)
            .collect();
        for (file, how) in unkept(&of) {
            not_kept.push(NotKept {
                source: file.source.clone(),
                in_home: file.in_home.clone(),
            });
            said.push(format!("the copy of {} {how}", file.source));
        }
    }
    if said.is_empty() {
        return Outcome::done();
    }
    Outcome::failed(Reason::OutputNotKept).with(CameTo {
        not_kept,
        ..CameTo::failed(&format!(
            "{}; collect them again, and the files they came from are kept until then",
            said.join("; ")
        ))
    })
}

/// One version of a file a collection kept: the copy, how it stands now
/// (none when it is as collected), and when it was read, when that can be
/// trusted to order it.
struct Version<'a> {
    file: &'a Kept,
    how: Option<&'static str>,
    span: Option<crate::collect::ReadSpan>,
}

/// The versions of one file (`of`, every one collected) whose copies must
/// still be kept and are not, with how each stands. The newest version
/// counts: one read wholly after every other version with other bytes.
/// Its bytes are kept when any copy of them is intact. When no version is
/// the newest, because reads overlapped, a time is missing, or the clock
/// moved backwards, any one of them may be the file's latest state, so a
/// copy of each different version must be intact.
fn unkept<'a>(of: &[&'a Version<'a>]) -> Vec<(&'a Kept, &'static str)> {
    let intact = |sha256: &str| {
        of.iter()
            .any(|version| version.how.is_none() && version.file.sha256 == sha256)
    };
    let newest = of.iter().find(|candidate| {
        let Some(span) = candidate.span else {
            return false;
        };
        of.iter()
            .filter(|other| other.file.sha256 != candidate.file.sha256)
            .all(|other| other.span.is_some_and(|other| other.before(&span)))
    });
    let must: Vec<&Version> = match newest {
        Some(newest) => vec![*newest],
        None => {
            let mut distinct: Vec<&Version> = Vec::new();
            for version in of {
                if !distinct
                    .iter()
                    .any(|known| known.file.sha256 == version.file.sha256)
                {
                    distinct.push(version);
                }
            }
            distinct
        }
    };
    must.into_iter()
        .filter(|version| !intact(&version.file.sha256))
        .map(|version| {
            // Say how the copy of these bytes stands: gone or changed.
            let how = of
                .iter()
                .filter(|other| other.file.sha256 == version.file.sha256)
                .find_map(|other| other.how)
                .unwrap_or("is gone");
            (version.file, how)
        })
        .collect()
}

/// When `collection` was made, in milliseconds since 1970, as its receipt
/// says; 0 when it does not say.
fn collected_at(collection: &Collection) -> u64 {
    collection
        .receipt
        .as_str()
        .split('-')
        .next()
        .and_then(|at| at.parse().ok())
        .unwrap_or(0)
}

/// Stop one job: found ended without a signal when a stop sent before
/// already ended it, and stopped by its identity otherwise. A job the
/// helper has no record of is gone.
async fn stop_job(hands: &impl Hands, task: &Key, id: &JobId) -> Outcome {
    let shown = match hands.job(id).await {
        Ok(shown) => shown,
        Err(error)
            if error.code == "job"
                && crate::jobs::refusal_of(&error) == Some(JobRefusal::Unknown) =>
        {
            return Outcome::gone();
        }
        Err(error) => return job_failed(&error),
    };
    if shown.unreadable() {
        return Outcome::failed(Reason::Identity).with(CameTo::failed(
            "Atlas cannot tell whether this job's process is running",
        ));
    }
    let ended = if shown.state() == Some(JobState::Ended) {
        serde_json::to_value(shown).unwrap_or_default()
    } else {
        match hands.stop_job(task, id).await {
            Ok(ended) => ended,
            Err(error) => return job_failed(&error),
        }
    };
    if ended["state"] != "ended" {
        return Outcome::failed(Reason::StillStopping).with(CameTo::failed(
            "The job has not ended yet; Atlas keeps stopping it",
        ));
    }
    let left: Vec<Left> = serde_json::from_value(ended["left"].clone()).unwrap_or_default();
    Outcome::done().with(CameTo {
        left,
        ..CameTo::default()
    })
}

/// What a job's step came to when the helper refused or could not be
/// asked: uncertain when its stop may have been sent, blocked by a Stop or
/// a helper out of reach, failed when its identity no longer matches.
fn job_failed(error: &Error) -> Outcome {
    if error.uncertain {
        return Outcome::erred(error, Reason::Helper);
    }
    let (state, reason) = match (error.code, crate::jobs::refusal_of(error)) {
        ("job", Some(JobRefusal::Identity)) => (StepState::Failed, Reason::Identity),
        ("job", Some(JobRefusal::OtherAccount)) => (StepState::Failed, Reason::OtherAccount),
        ("job", Some(JobRefusal::StillStopping)) => (StepState::Failed, Reason::StillStopping),
        ("stopped", _) => (StepState::Blocked, Reason::Stopped),
        ("daemon" | "transport" | "on-console" | "logged-out" | "blocked", _) => {
            (StepState::Blocked, Reason::Daemon)
        }
        _ => (StepState::Failed, Reason::Helper),
    };
    Outcome {
        state,
        reason: Some(reason),
        already_gone: false,
        came_to: CameTo::failed(&error.error),
    }
}

/// What the helper says of the installs among `bundles`; nothing to ask
/// when there are none.
pub(crate) async fn installs_answer(
    hands: &impl Hands,
    bundles: &[Bundle],
) -> Result<Vec<InstallState>> {
    let ids = installs_of(bundles);
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    hands.installs(&ids).await
}

async fn quit(hands: &impl Hands, bundles: &[Bundle], session: &Key) -> Outcome {
    let installs = match installs_answer(hands, bundles).await {
        Ok(installs) => installs,
        Err(error) => return Outcome::erred(&error, Reason::Helper),
    };
    let apps = match hands.apps().await {
        Ok(apps) => apps,
        Err(error) => return Outcome::erred(&error, Reason::Helper),
    };
    let targets = targets(hands.copies(), session, bundles, &installs, &apps);
    let left = CameTo {
        untraced: targets.untraced.clone(),
        ..CameTo::default()
    };
    if targets.processes.is_empty() {
        return Outcome::done().with(left);
    }
    match hands
        .quit(&targets.processes, &targets.folders, &installs_of(bundles))
        .await
    {
        Ok(Quitted::Gone) => Outcome::done().with(left),
        Ok(Quitted::TimedOut(still)) => Outcome::failed(Reason::Timeout).with(CameTo {
            error: Some(still),
            ..left
        }),
        Err(error) => Outcome::erred(&error, Reason::Helper),
    }
}

async fn remove_install(hands: &impl Hands, install: &Token) -> Outcome {
    let reply = match hands.remove_install(install).await {
        Ok(reply) => reply,
        Err(error) => return Outcome::erred(&error, Reason::Helper),
    };
    if reply["ok"] != true {
        let said = reply["error"]
            .as_str()
            .unwrap_or("The helper did not finish moving the app aside");
        return Outcome::failed(Reason::Helper).with(CameTo::failed(said));
    }
    let Ok(removal) = serde_json::from_value::<Removal>(reply) else {
        return Outcome::failed(Reason::Helper);
    };
    let came_to = CameTo {
        side_effects: removal.side_effects.unwrap_or_default().kept,
        held: match &removal.outcome {
            Removed::Held { held_path } => Some(held_path.to_string_lossy().into_owned()),
            _ => None,
        },
        ..CameTo::default()
    };
    let outcome = match removal.outcome {
        Removed::Held { .. } => Outcome::done(),
        Removed::AlreadyGone => Outcome::gone(),
        Removed::Kept { reason } => Outcome::kept(match reason {
            KeptReason::Replaced => Reason::Replaced,
            KeptReason::OtherVolume => Reason::OtherVolume,
            KeptReason::Unreadable => Reason::Unreadable,
            KeptReason::Changed => Reason::Changed,
        }),
        Removed::InUse => Outcome::failed(Reason::InUse),
    };
    outcome.with(came_to)
}

async fn remove_copy(hands: &impl Hands, copy: &Token, session: &Key) -> Outcome {
    let apps = match hands.apps().await {
        Ok(apps) => apps,
        Err(error) => return Outcome::erred(&error, Reason::Helper),
    };
    let running = Running::listed(&apps);
    match handoff::remove_copy(hands.copies(), copy, session, &running) {
        CopyRemoval::AlreadyGone => Outcome::gone(),
        CopyRemoval::Removed => Outcome::done(),
        CopyRemoval::AnotherSession => Outcome::kept(Reason::AnotherSession),
        CopyRemoval::Unreadable => Outcome::kept(Reason::LabelUnreadable),
        CopyRemoval::InUse => Outcome::failed(Reason::InUse),
        CopyRemoval::Failed(error) => Outcome::failed(Reason::Io).with(CameTo::failed(&error)),
    }
}
