//! The replies the daemon gives about one job that a request could not act
//! on as asked: one it could not confirm, one no record names, one that is
//! no longer the process Atlas started, and one that has already ended.
use super::{record::Record, verdict::Verdict};
use crate::protocol::{JobFailure, JobId, JobRefusal, JobReply, StopCameTo, Stopped};
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

/// The refusal of a request about job `id`.
pub(super) fn refusal(reason: JobRefusal, said: &str, id: &JobId) -> Value {
    JobFailure::new(reason, said).about(id).into_value()
}

/// The refusal of a job no record names.
pub(super) fn unknown(id: &JobId) -> Value {
    refusal(
        JobRefusal::Unknown,
        "No background job with that ID is known in the helper",
        id,
    )
}

/// The refusal a verdict that sends nothing gives.
pub(super) fn refused_by(verdict: &Verdict, id: &JobId) -> Value {
    let (reason, said) = verdict.refusal();
    refusal(reason, said, id)
}

/// The reply to a stop of `record`'s job, and what it came to.
pub(super) fn stop_reply(record: &Record, came_to: StopCameTo) -> Value {
    JobReply::Stop(Stopped {
        job: record.view(),
        came_to,
    })
    .into_value()
}

/// The reply to a stop of a job that has already ended.
pub(super) fn already_ended(record: &Record) -> Value {
    stop_reply(record, StopCameTo::AlreadyEnded(true))
}
