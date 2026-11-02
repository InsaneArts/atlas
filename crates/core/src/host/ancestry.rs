//! A bounded ancestry walk with each process's account and exact start.
use crate::process::{Instance, Own, Start, own};

/// What macOS says of one described process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Facts {
    pub(crate) parent: i32,
    pub(crate) uid: u32,
    /// Its command name, at most 16 bytes as the kernel keeps it.
    pub(crate) command: String,
    pub start: Start,
}

/// One ancestor of this process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Ancestor {
    pub(crate) pid: i32,
    pub(crate) uid: u32,
    pub(crate) instance: Instance,
    pub(crate) parent: i32,
    pub(crate) command: String,
}

/// The facts of `pid`, when it is a live process of this account.
pub fn process(pid: i32) -> Option<Facts> {
    match own(pid) {
        Own::Found(info) if info.uid == nix::unistd::geteuid().as_raw() => Some(Facts {
            parent: info.ppid,
            uid: info.uid,
            command: info.name,
            start: info.instance.start,
        }),
        Own::Found(_) | Own::Gone | Own::Unknown => None,
    }
}

/// This process's ancestors, parent first, at most `limit`, ending at
/// launchd or at the first that cannot be read.
pub(crate) fn ancestors(limit: usize) -> Vec<Ancestor> {
    ancestors_from(nix::unistd::getppid().as_raw(), limit, process)
}

/// [`ancestors`] from `first`, described by `describe`.
pub(crate) fn ancestors_from(
    first: i32,
    limit: usize,
    describe: impl Fn(i32) -> Option<Facts>,
) -> Vec<Ancestor> {
    let mut found: Vec<Ancestor> = Vec::new();
    let mut pid = first;

    while found.len() < limit && pid > 0 && !found.iter().any(|seen| seen.pid == pid) {
        let Some(facts) = describe(pid) else {
            break;
        };
        found.push(Ancestor {
            pid,
            uid: facts.uid,
            instance: Instance::new(pid, facts.start),
            parent: facts.parent,
            command: facts.command,
        });
        if pid == 1 {
            break;
        }
        pid = facts.parent;
    }

    found
}

#[cfg(test)]
mod tests;
