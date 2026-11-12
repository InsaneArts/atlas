//! Where agents' apps run: a desktop, what kind it is, the name its records
//! carry, and the proof its session has to give.
use crate::{
    digest::bytes_sha256,
    error::Result,
    process::{self, Info},
    protocol::{self, Left},
    session::{Observation, Session},
};
use std::collections::HashSet;

/// A desktop agents act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Desktop {
    kind: Kind,
}

/// What a desktop is. Each thing that differs between kinds has one owner
/// that matches on this, so a new kind cannot be added without an answer
/// from every one of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A second login on this Mac, kept off the console.
    SecondLogin(SecondLogin),
}

/// A second login on this Mac, by its account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SecondLogin {
    pub uid: u32,
}

/// The name a desktop's locks and records carry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopId(String);

impl From<SecondLogin> for Desktop {
    fn from(login: SecondLogin) -> Self {
        Self {
            kind: Kind::SecondLogin(login),
        }
    }
}

impl Desktop {
    /// What kind of desktop this is, for an owner to match on.
    pub fn kind(&self) -> &Kind {
        &self.kind
    }

    /// The second login this desktop is. Whoever asks relies on a second
    /// login on this Mac: its account, its processes in this kernel, its
    /// files on this disk.
    pub fn second_login(&self) -> &SecondLogin {
        let Kind::SecondLogin(login) = &self.kind;
        login
    }

    /// The name this desktop's locks and records carry. A second login's
    /// comes from its account alone, so a changed socket path names the
    /// same desktop.
    pub fn id(&self) -> DesktopId {
        let named = match &self.kind {
            Kind::SecondLogin(login) => format!("helper:{}", login.uid),
        };
        let digest = bytes_sha256(named.as_bytes());
        DesktopId(digest[..DesktopId::LENGTH].to_owned())
    }

    /// Verify that `observation` describes this desktop, out of the
    /// person's sight.
    ///
    /// # Errors
    ///
    /// For a second login, what [`Observation::require_hidden`] refuses.
    pub fn prove(&self, observation: &Observation) -> Result<Session> {
        match &self.kind {
            Kind::SecondLogin(login) => observation.require_hidden(login.uid),
        }
    }

    /// Whether `observation` shows input service `pid` ready in this
    /// desktop with no controller and nothing held, running the installed
    /// build.
    pub fn input_current(&self, observation: &Observation, pid: u32) -> bool {
        match &self.kind {
            Kind::SecondLogin(login) => observation.input_current(login.uid, pid),
        }
    }

    /// Whether anything an ended job or a finished command left may still
    /// run in this desktop: a process it listed in `left`, or any process in
    /// its own group `group` or in a group a listed one was in. What cannot
    /// be read or listed may run.
    pub fn left_may_run(&self, left: &[Left], group: Option<i32>) -> bool {
        match &self.kind {
            Kind::SecondLogin(login) => protocol::left_may_run(left, group, login.uid),
        }
    }

    /// Whether a process of this desktop runs in a group whose leader is
    /// gone, as work a finished command left does: an app the desktop
    /// opened leads its own group.
    ///
    /// # Errors
    ///
    /// Returns the failure to list the desktop's processes: work that
    /// cannot be looked for may run.
    pub fn orphans_run(&self) -> Result<bool> {
        match &self.kind {
            Kind::SecondLogin(login) => Ok(orphaned(&process::of_user(login.uid)?)),
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

impl DesktopId {
    /// How many characters of the digest a name keeps.
    const LENGTH: usize = 24;

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
#[path = "desktop_tests.rs"]
mod tests;
