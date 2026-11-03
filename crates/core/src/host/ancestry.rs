//! A bounded ancestry walk with each process's account and exact start.
use crate::process::{Info, Own, own};

/// What macOS says of `pid`, when it is a live process of this account.
pub fn process(pid: i32) -> Option<Info> {
    match own(pid) {
        Own::Found(info) if info.uid == nix::unistd::geteuid().as_raw() => Some(info),
        Own::Found(_) | Own::Gone | Own::Unknown => None,
    }
}

/// This process's ancestors, parent first, at most `limit`, ending at
/// launchd or at the first that cannot be read.
pub(crate) fn ancestors(limit: usize) -> Vec<Info> {
    ancestors_from(nix::unistd::getppid().as_raw(), limit, process)
}

/// [`ancestors`] from `first`, described by `describe`.
pub(crate) fn ancestors_from(
    first: i32,
    limit: usize,
    describe: impl Fn(i32) -> Option<Info>,
) -> Vec<Info> {
    let mut found: Vec<Info> = Vec::new();
    let mut pid = first;

    while found.len() < limit && pid > 0 && !found.iter().any(|seen| seen.instance.pid == pid) {
        let Some(info) = describe(pid) else {
            break;
        };
        let parent = info.ppid;
        found.push(info);
        if pid == 1 {
            break;
        }
        pid = parent;
    }

    found
}

#[cfg(test)]
mod tests;
