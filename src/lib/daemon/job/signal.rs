//! The one place a job is signalled. Every signal, a first TERM, a TERM
//! sent again and each KILL alike, is preceded by a fresh look at the job's
//! leader and group and a fresh verdict, so nothing older than that look
//! ever authorizes it, and only the job's own group is ever named.
use super::{
    Jobs, State,
    record::Record,
    verdict::{Observation, Verdict, verdict},
};
use crate::daemon::protocol::Expected;
use nix::sys::signal::Signal;

/// What a signal to a job came to.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Signalled {
    /// It reached the job's group.
    Sent,
    /// It was not sent, for the reason the verdict says.
    NotSent(Verdict),
}

impl Jobs {
    /// One fresh look at `record`'s leader and group; none when the kernel
    /// could not say.
    pub(super) fn observe(&self, state: &State, record: &Record) -> Option<Observation> {
        let identity = record.identity()?;
        let leader = self.kernel.process(identity.pid()).ok()?;
        let members = self.kernel.group(identity.pgid()).ok()?;
        Some(Observation {
            leader,
            members,
            own_child: state.children.contains_key(&record.id),
        })
    }

    /// What may be done to `record`'s group now, as one fresh look says.
    pub(super) fn judge(
        &self,
        state: &State,
        record: &Record,
        expect: Option<&Expected>,
    ) -> (Verdict, Option<Observation>) {
        let seen = self.observe(state, record);
        let judged = verdict(record, expect, &self.boot, self.uid, seen.as_ref());
        (judged, seen)
    }

    /// Send `signal` to `record`'s group, only when a fresh verdict says it
    /// is still the job's. A group the kernel refuses is looked at again:
    /// one with nothing but an unreaped leader left has ended, and any other
    /// refusal is not taken to say anything about the job.
    pub(super) fn signal(
        &self,
        state: &State,
        record: &Record,
        expect: Option<&Expected>,
        signal: Signal,
    ) -> Signalled {
        let judged = self.judge(state, record, expect).0;
        let Verdict::Signal { pgid } = judged else {
            return Signalled::NotSent(judged);
        };
        if self.kernel.killpg(pgid, signal).is_ok() {
            return Signalled::Sent;
        }
        match self.judge(state, record, expect).0 {
            Verdict::Signal { .. } => Signalled::NotSent(Verdict::Unknown),
            after => Signalled::NotSent(after),
        }
    }
}
