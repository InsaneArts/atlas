//! Whether a job's group may be signalled now: decided from its identity,
//! the person's side's copy of it, and one fresh look at its leader and its
//! group. Pure, so every rule is tested on its own.
use super::record::{JobIdentity, Record};
use crate::{
    daemon::protocol::{Expected, JobRefusal},
    process::Info,
};

/// One look at a job's processes: its leader's PID now, and its group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    /// The process with the leader's PID; none when there is none.
    pub leader: Option<Info>,
    /// Every process in the group, unreaped ones included.
    pub members: Vec<Info>,
    /// Whether the leader is this daemon's own child, held unreaped: its
    /// PID, and so the group's ID, cannot go to another process.
    pub own_child: bool,
}

/// What may be done to a job's group now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Its leader is still the job's and its group is the helper's: this
    /// group, and nothing else, may be signalled.
    Signal { pgid: i32 },
    /// Nothing of the job runs any more.
    Ended,
    /// Its leader is gone while processes of its group remain. They are
    /// never signalled: without the leader nothing proves the group is
    /// still the job's.
    LeaderGone { remaining: Vec<Info> },
    /// The leader's PID is another process now: the job ended, and its PID
    /// was used again.
    Reused,
    /// It started in another boot.
    Restarted,
    /// The records disagree, or the leader is in another group: nothing is
    /// signalled.
    Identity,
    /// Part of the group belongs to another account by the kernel's own
    /// rule: nothing is signalled.
    OtherAccount,
    /// Its processes could not be read: nothing is signalled, and nothing
    /// is taken to have ended.
    Unknown,
}

impl Verdict {
    /// The reason a refusal gives for this verdict.
    pub fn reason(&self) -> JobRefusal {
        match self {
            Self::OtherAccount => JobRefusal::OtherAccount,
            Self::Unknown => JobRefusal::Unobserved,
            _ => JobRefusal::Identity,
        }
    }
}

/// Whether the group's own entry for the leader's PID tells the same
/// process as the leader read after it: the same start time and owner, or
/// no entry when the leader is gone. Two reads that disagree saw the
/// leader replaced in between, and say nothing reliable about the group.
fn consistent(leader: Option<&Info>, members: &[Info], pid: i32) -> bool {
    let listed = members.iter().find(|member| member.instance.pid == pid);
    match (leader, listed) {
        (Some(leader), Some(listed)) => {
            listed.instance == leader.instance
                && (listed.uid, listed.ruid, listed.svuid)
                    == (leader.uid, leader.ruid, leader.svuid)
                && listed.pgid == leader.pgid
        }
        // Listed in its group, then gone: it ended in between.
        (None, Some(_)) => true,
        // Read alive after a listing that lacked it: alive only in its own
        // group's listing can be trusted.
        (Some(leader), None) => leader.zombie || leader.pgid != pid,
        (None, None) => true,
    }
}

/// What may be done to the job `record` names, as `expect` (the person's
/// side's copy, when there is one) and `seen` (one fresh look at its
/// processes, or the error that look met) say, for the helper account
/// `uid` in this boot. Checked in order: the record, the person's copy,
/// the boot, the leader, every member.
pub fn verdict(
    record: &Record,
    expect: Option<&Expected>,
    boot: &str,
    uid: u32,
    seen: Option<&Observation>,
) -> Verdict {
    if record.uid != uid {
        return Verdict::OtherAccount;
    }
    let Some(identity) = record.identity() else {
        return Verdict::Identity;
    };
    if let Some(expect) = expect
        && JobIdentity::expected(expect).as_ref() != Some(&identity)
    {
        return Verdict::Identity;
    }
    if identity.boot() != boot {
        return Verdict::Restarted;
    }
    let Some(seen) = seen else {
        return Verdict::Unknown;
    };
    if !consistent(seen.leader.as_ref(), &seen.members, identity.pid()) {
        return Verdict::Unknown;
    }

    let leader = match &seen.leader {
        Some(leader) if leader.instance.start != identity.start() => return Verdict::Reused,
        Some(leader) if leader.pgid != identity.pgid() => return Verdict::Identity,
        // A leader this daemon has not reaped counts as live: its unreaped
        // PID, and so its group's ID, cannot go to another process. Any
        // other zombie leader counts as gone.
        Some(leader) if leader.zombie && !seen.own_child => None,
        Some(leader) if !leader.owned_by(uid) => return Verdict::OtherAccount,
        leader => leader.as_ref(),
    };
    let live: Vec<&Info> = seen
        .members
        .iter()
        .filter(|member| member.pgid == identity.pgid() && !member.zombie)
        .collect();
    if live.iter().any(|member| !member.owned_by(uid)) {
        return Verdict::OtherAccount;
    }
    match (leader, live.is_empty()) {
        (_, true) => Verdict::Ended,
        (Some(_), false) => Verdict::Signal {
            pgid: identity.pgid(),
        },
        (None, false) => Verdict::LeaderGone {
            remaining: live.into_iter().cloned().collect(),
        },
    }
}
