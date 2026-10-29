//! The replies the daemon gives about one job that a request could not act
//! on as asked: one it could not confirm, one no record names, one that is
//! no longer the process Atlas started, and one that has already ended.
use super::{record::Record, verdict::Verdict};
use crate::daemon::protocol::{JobFailure, JobId, JobRefusal, JobReply, StopCameTo, Stopped};
use serde_json::Value;

/// The reply for a job that may run but could not be confirmed: of unknown
/// outcome, with the job as the daemon keeps it.
pub(super) fn unconfirmed(record: &Record, said: &str) -> Value {
    JobFailure {
        uncertain: true,
        job: Some(record.view()),
        ..JobFailure::new(JobRefusal::Unconfirmed, said).about(&record.id)
    }
    .into_value()
}

/// The refusal of a job no record names.
pub(super) fn unknown(id: &JobId) -> Value {
    JobFailure::new(
        JobRefusal::Unknown,
        "No background job with that ID is known in the helper",
    )
    .about(id)
    .into_value()
}

/// The refusal of a job that is no longer the process Atlas started.
pub(super) fn identity(id: &JobId) -> Value {
    JobFailure::new(
        JobRefusal::Identity,
        "The job is no longer the process Atlas started, so nothing was signalled",
    )
    .about(id)
    .into_value()
}

/// The refusal a verdict that sends nothing gives.
pub(super) fn refused_by(verdict: &Verdict, id: &JobId) -> Value {
    let error = match verdict.reason() {
        JobRefusal::OtherAccount => {
            "Part of the job belongs to another account, so nothing was signalled"
        }
        JobRefusal::Unobserved => {
            "Atlas could not read the job's processes, so nothing was signalled; try again"
        }
        _ => return identity(id),
    };
    JobFailure::new(verdict.reason(), error)
        .about(id)
        .into_value()
}

/// The reply to a stop of a job that has already ended.
pub(super) fn already_ended(record: &Record) -> Value {
    JobReply::Stop(Stopped {
        job: record.view(),
        came_to: StopCameTo::AlreadyEnded(true),
    })
    .into_value()
}
