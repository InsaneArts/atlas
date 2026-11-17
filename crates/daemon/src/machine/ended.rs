//! Whether the work of a change whose outcome is unknown is proven gone,
//! by evidence of the change's own kind. A command, the environment worker
//! and a job's start leave
//! a durable link before any effect: the change's intent, then the record
//! of the supervisor started for it, and only then its program. So one
//! this daemon no longer carries out, whose every supervisor is done with,
//! has nothing of it running, whether it ran or not. A job's stop is
//! bound to its target, the job's own supervisor: its work is gone only
//! once that supervisor is done with. Input taken, sent or let go of, and
//! the owner it was taken for, are gone only once the input service itself
//! said, after the change was no longer carried out here, that no owner
//! holds input and nothing is held: its owner's cleanup acknowledged,
//! whatever owner it was. A vanished lane or handler proves nothing. A
//! kind with no such evidence here is never judged: what it came to stays
//! unknown.
use super::{
    changes::Intended,
    processes::{Remembered, Supervised, done_with},
};
use atlas_core::{
    files::Token,
    process::Instance,
    protocol::{self, JobId, JobRequest, Request},
};
use std::path::Path;

/// A command's operation.
const RUN: &str = "run";
/// The environment worker, run as a command is: its supervisor, kept naming
/// the change before the worker starts, is what proves it gone.
const ENVIRONMENT: &str = "environment";
/// What is judged as a command is.
const RUN_LIKE: [&str; 2] = [RUN, ENVIRONMENT];
/// A job's start or stop: only a start leaves a supervisor naming it.
const JOB: &str = "job";
/// An owner of the input service, kept before it takes input and until
/// the service acknowledges it let go.
pub(crate) const OWNER: &str = "owner";
/// Input taken, sent or let go of on the input service's own lanes, and
/// the owner it was for.
pub(crate) const INPUT: [&str; 4] = ["acquire", "input", "release", OWNER];

/// What a change's work is judged by.
pub(crate) struct Evidence<'a> {
    /// The supervisors kept on the guest's disk.
    pub(crate) kept: &'a dyn Remembered,
    /// Where supervisors leave their proof.
    pub(crate) proofs: &'a Path,
    /// Whether this daemon still carries a change out.
    pub(crate) carried: &'a dyn Fn(&Token) -> bool,
    /// Whether a keeper of this daemon owns a supervisor.
    pub(crate) owned: &'a dyn Fn(&Instance) -> bool,
    /// Whether a supervisor's process still runs.
    pub(crate) running: &'a dyn Fn(&Instance) -> bool,
    /// Whether the input service said, once a change to input was seen no
    /// longer carried out here, that no owner holds input and nothing is
    /// held.
    pub(crate) released: &'a dyn Fn(&Token) -> bool,
}

/// What a job's change was asked to do, as its kept request says.
enum Asked {
    Start,
    /// A stop, and the job it stops.
    Stop(JobId),
}

impl Asked {
    /// What `intended` asked, when its request was kept: none for one a
    /// guest from before kept, which says no more than its operation.
    fn of(intended: &Intended) -> Option<Self> {
        let line = serde_json::to_vec(intended.asked.as_ref()?).ok()?;
        match protocol::parse(&line).ok()?.request {
            Request::Job(JobRequest::Start { .. }) => Some(Self::Start),
            Request::Job(JobRequest::Stop { id, .. }) => Some(Self::Stop(id)),
            _ => None,
        }
    }
}

impl Evidence<'_> {
    /// Those of `unresolved` whose work is proven gone. Blocking.
    ///
    /// Each is looked at as carried out here before the supervisors' record
    /// is read: one no longer carried never gains a supervisor after, so a
    /// record read later names every supervisor it ever had, unless
    /// retired, which only one proven done with is.
    pub(crate) fn proven_gone(&self, unresolved: Vec<Intended>) -> Vec<Token> {
        let (input, judged): (Vec<Intended>, Vec<Intended>) = unresolved
            .into_iter()
            .filter(|intended| !(self.carried)(&intended.change))
            .filter(|intended| {
                RUN_LIKE.contains(&intended.operation.as_str())
                    || intended.operation == JOB
                    || INPUT.contains(&intended.operation.as_str())
            })
            .partition(|intended| INPUT.contains(&intended.operation.as_str()));
        let mut gone: Vec<Token> = input
            .into_iter()
            .filter(|intended| (self.released)(&intended.change))
            .map(|intended| intended.change)
            .collect();
        if judged.is_empty() {
            return gone;
        }
        let (Ok(work), Ok(boot)) = (self.kept.kept(), atlas_core::process::boot()) else {
            return gone;
        };
        gone.extend(
            judged
                .into_iter()
                .filter(|intended| self.gone(intended, &work, &boot))
                .map(|intended| intended.change),
        );
        gone
    }

    /// Whether the work of `intended` is gone, `work` being every
    /// supervisor kept and `boot` this boot.
    fn gone(&self, intended: &Intended, work: &[Supervised], boot: &str) -> bool {
        let done = |supervised: &&Supervised| {
            !(self.owned)(&supervised.supervisor)
                && done_with(supervised, boot, self.proofs, self.running)
        };
        let named: Vec<&Supervised> = work
            .iter()
            .filter(|supervised| supervised.change.as_ref() == Some(&intended.change))
            .collect();
        // One kept by an earlier build names no change: it may be this one's.
        let unnamed_done = work
            .iter()
            .filter(|supervised| supervised.change.is_none())
            .all(|supervised| done(&supervised));
        let named_done = named.iter().all(done);
        match (intended.operation.as_str(), Asked::of(intended)) {
            (operation, _) if RUN_LIKE.contains(&operation) => unnamed_done && named_done,
            (JOB, Some(Asked::Start)) => unnamed_done && named_done,
            // Its target is the supervisor running that job, whichever build
            // kept it: a stop that began proves nothing while the job may
            // still run. None kept means none ran it, or it was retired once
            // proven done with.
            (JOB, Some(Asked::Stop(id))) => work
                .iter()
                .filter(|supervised| supervised.job.as_ref().is_some_and(|job| job.id == id))
                .all(|supervised| done(&supervised)),
            // Its request not kept: a stop names no supervisor, nor does a
            // start whose one was retired, so with none which it was is not
            // told.
            (JOB, None) => !named.is_empty() && unnamed_done && named_done,
            _ => false,
        }
    }
}

#[cfg(test)]
#[path = "ended_tests.rs"]
mod tests;
