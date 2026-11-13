//! Where agents' apps run: a desktop, what kind it is, the name its records
//! carry, and the proof its session has to give.
use crate::{
    digest::bytes_sha256,
    error::Result,
    process::Instance,
    protocol::Left,
    session::{Observation, Session},
};

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
    /// A hidden user on this Mac, kept off the console: what people know
    /// as Second Login.
    User(crate::user::User),
}

/// The name a desktop's locks and records carry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopId(String);

impl From<crate::user::User> for Desktop {
    fn from(user: crate::user::User) -> Self {
        Self {
            kind: Kind::User(user),
        }
    }
}

impl Desktop {
    /// What kind of desktop this is, for an owner to match on.
    pub fn kind(&self) -> &Kind {
        &self.kind
    }

    /// The hidden user this desktop is: the way in for that user's own
    /// setup, update and removal, which work on its account, its processes
    /// in this kernel and its files on this disk. Anything that
    /// differs between kinds matches on [`Desktop::kind`] instead.
    pub fn user(&self) -> &crate::user::User {
        let Kind::User(user) = &self.kind;
        user
    }

    /// The name this desktop's locks and records carry. A second login's
    /// comes from its account alone, so a changed socket path names the
    /// same desktop.
    pub fn id(&self) -> DesktopId {
        let named = match &self.kind {
            Kind::User(user) => user.record_name(),
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
            Kind::User(login) => observation.require_hidden(login.uid),
        }
    }

    /// Whether `observation` shows input service `pid` ready in this
    /// desktop with no controller and nothing held, running the installed
    /// build.
    pub fn input_current(&self, observation: &Observation, pid: u32) -> bool {
        match &self.kind {
            Kind::User(login) => observation.input_current(login.uid, pid),
        }
    }

    /// Whether anything an ended job or a finished command left may still
    /// run in this desktop: a process it listed in `left`, or any process in
    /// its own group `group` or in a group a listed one was in. What cannot
    /// be read or listed may run.
    pub fn left_may_run(&self, left: &[Left], group: Option<i32>) -> bool {
        match &self.kind {
            Kind::User(user) => user.left_may_run(left, group),
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
            Kind::User(user) => user.orphans_run(),
        }
    }

    /// Process `pid` of this desktop, told apart from a later process with
    /// the same number; none when it cannot be looked at.
    pub fn instance(&self, pid: u32) -> Option<Instance> {
        match &self.kind {
            Kind::User(_) => crate::user::User::instance(pid),
        }
    }

    /// Whether `instance` of this desktop has exited: its PID is gone, a
    /// zombie, or another process's. Unknown is not exited.
    pub fn exited(&self, instance: Instance) -> bool {
        match &self.kind {
            Kind::User(_) => crate::user::User::exited(instance),
        }
    }
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
