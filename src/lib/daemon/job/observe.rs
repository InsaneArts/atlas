//! What the daemon reads of processes, through the kernel, and what it
//! notes of a job's descendants to say which left its group. Nothing noted
//! here ever decides a signal.
use super::record::Left;
use crate::{
    error::Result,
    process::{self, Info, Instance},
};
use nix::{errno::Errno, sys::signal::Signal, unistd::Pid};
use std::collections::HashMap;

/// The most descendants a job keeps note of.
pub(super) const SEEN_MAX: usize = 256;

/// The kernel as the daemon uses it: process facts, and the one way it
/// signals. Tests stand in for it.
pub trait Kernel: Send + Sync {
    /// `pid` now, whoever's it is; none when there is no such process.
    fn process(&self, pid: i32) -> Result<Option<Info>>;
    /// Every process in group `pgid`, unreaped ones included.
    fn group(&self, pgid: i32) -> Result<Vec<Info>>;
    /// Every process whose effective user ID is `uid`.
    fn of_user(&self, uid: u32) -> Result<Vec<Info>>;
    /// The session `pid` belongs to, when it can be read.
    fn session(&self, pid: i32) -> Option<i32>;
    /// Send `signal` to process group `pgid`.
    fn killpg(&self, pgid: i32, signal: Signal) -> std::result::Result<(), Errno>;
}

/// The real kernel.
pub struct Live;

impl Kernel for Live {
    fn process(&self, pid: i32) -> Result<Option<Info>> {
        process::info(pid)
    }

    fn group(&self, pgid: i32) -> Result<Vec<Info>> {
        process::group(pgid)
    }

    fn of_user(&self, uid: u32) -> Result<Vec<Info>> {
        process::of_user(uid)
    }

    fn session(&self, pid: i32) -> Option<i32> {
        process::session(pid)
    }

    fn killpg(&self, pgid: i32, signal: Signal) -> std::result::Result<(), Errno> {
        nix::sys::signal::killpg(Pid::from_raw(pgid), signal)
    }
}

/// Note in `seen` the descendants of `job` among `processes` (the helper
/// account's): every live member of its group, every process whose parent
/// is one of those or already seen and still the same process, and every
/// process in the job's session. Each started no earlier than the job. At
/// most [`SEEN_MAX`] are kept. Best effort, and only ever used to report
/// what left the group: a process whose parent exited between two looks is
/// missed.
pub fn observe(
    seen: &mut Vec<Instance>,
    job: &Instance,
    members: &[Info],
    processes: &[Info],
    session_of: impl Fn(i32) -> Option<i32>,
) {
    let alive: HashMap<i32, &Info> = processes
        .iter()
        .map(|process| (process.instance.pid, process))
        .collect();
    let add = |seen: &mut Vec<Instance>, instance: Instance| {
        if seen.len() < SEEN_MAX && !seen.contains(&instance) && instance.start >= job.start {
            seen.push(instance);
            true
        } else {
            false
        }
    };
    for member in members.iter().filter(|member| !member.zombie) {
        add(seen, member.instance);
    }

    loop {
        let parents: Vec<i32> = seen
            .iter()
            .filter(|instance| {
                alive
                    .get(&instance.pid)
                    .is_some_and(|process| process.instance.start == instance.start)
            })
            .map(|instance| instance.pid)
            .collect();
        let mut grew = false;
        for process in processes.iter().filter(|process| !process.zombie) {
            let descends = parents.contains(&process.ppid)
                || (process.ppid != 1 && process.ppid == job.pid)
                || session_of(process.instance.pid) == Some(job.pid);
            if descends {
                grew |= add(seen, process.instance);
            }
        }
        if !grew || seen.len() >= SEEN_MAX {
            break;
        }
    }
}

/// The processes in `seen` that still run as the same process, as `uid`,
/// outside group `pgid`: they left the job's group.
pub fn left(seen: &[Instance], processes: &[Info], pgid: i32, uid: u32) -> Vec<Left> {
    seen.iter()
        .filter_map(|instance| {
            processes
                .iter()
                .find(|process| process.instance == *instance)
        })
        .filter(|process| !process.zombie && process.owned_by(uid) && process.pgid != pgid)
        .map(as_left)
        .collect()
}

/// A process as a report names it.
pub fn as_left(process: &Info) -> Left {
    Left {
        pid: process.instance.pid,
        name: process.name.clone(),
    }
}
