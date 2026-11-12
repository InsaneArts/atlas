//! The user desktop: a hidden user on this Mac, kept off the console, which
//! people know as Second Login. What only it needs lives here: its account,
//! its logins and their log, and its processes as this Mac's kernel sees
//! them. [`crate::desktop`] delegates its identity and process operations
//! here.
pub mod account;
pub mod login_log;
pub mod logins;

use crate::{
    error::Result,
    process::{self, Info, Instance},
    protocol::{self, Left},
};
use std::collections::HashSet;

/// A hidden user on this Mac, by its account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct User {
    pub uid: u32,
}

impl User {
    /// What its records' name is made from: its account alone.
    pub(crate) fn record_name(&self) -> String {
        format!("helper:{}", self.uid)
    }

    /// [`crate::desktop::Desktop::left_may_run`] for this user: its
    /// processes in this kernel.
    pub(crate) fn left_may_run(&self, left: &[Left], group: Option<i32>) -> bool {
        protocol::left_may_run(left, group, self.uid)
    }

    /// [`crate::desktop::Desktop::orphans_run`] for this user: its
    /// processes in this kernel.
    pub(crate) fn orphans_run(&self) -> Result<bool> {
        Ok(orphaned(&process::of_user(self.uid)?))
    }

    /// [`crate::desktop::Desktop::instance`] for this user: a process of
    /// this kernel.
    pub(crate) fn instance(pid: u32) -> Option<Instance> {
        i32::try_from(pid)
            .ok()
            .and_then(|pid| process::info(pid).ok().flatten())
            .map(|info| info.instance)
    }

    /// [`crate::desktop::Desktop::exited`] for this user: a process of this
    /// kernel.
    pub(crate) fn exited(instance: Instance) -> bool {
        match process::info(instance.pid) {
            Ok(None) => true,
            Ok(Some(info)) => info.instance.start != instance.start || info.zombie,
            Err(_) => false,
        }
    }
}

/// Whether one of `processes` runs in a group whose leader isn't among them.
fn orphaned(processes: &[Info]) -> bool {
    let alive: HashSet<i32> = processes
        .iter()
        .filter(|process| !process.zombie)
        .map(|process| process.instance.pid)
        .collect();
    processes
        .iter()
        .any(|process| !process.zombie && process.pgid > 0 && !alive.contains(&process.pgid))
}

#[cfg(test)]
#[path = "user_tests.rs"]
mod tests;
