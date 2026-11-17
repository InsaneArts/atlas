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
    /// A VM on this Mac, run by Atlas VM and reached over its session.
    Vm(crate::vm::Vm),
}

/// The name a desktop's live state carries: its locks, Stop, pending
/// outcomes and frames. It names the account or machine behind the
/// desktop, so replacing that names another.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DesktopId(String);

impl TryFrom<String> for DesktopId {
    type Error = String;

    fn try_from(text: String) -> std::result::Result<Self, Self::Error> {
        Self::parse(&text).ok_or_else(|| format!("{text} is not a desktop's identifier"))
    }
}

impl From<DesktopId> for String {
    fn from(id: DesktopId) -> Self {
        id.0
    }
}

crate::name::name_type! {
    /// The name a desktop's records, sign-ins and session bindings carry:
    /// a simple word that a repair of the desktop keeps.
    pub struct RecordKey;
    refused(text) = format!("{text} is not a desktop's record key");
    display
}

impl RecordKey {
    /// The longest record key.
    const MAX: usize = 64;

    /// The key of the one Second Login.
    pub fn user() -> Self {
        Self("user".to_owned())
    }

    /// The key of the VM named `id`.
    pub fn vm(id: &DesktopId) -> Self {
        Self(format!("vm-{}", id.as_str()))
    }

    /// The VM this key names, when it is a VM's.
    pub fn vm_id(&self) -> Option<DesktopId> {
        self.0.strip_prefix("vm-").and_then(DesktopId::parse)
    }

    /// `text` as a record key: 1 to 64 lowercase letters, digits and dashes.
    pub fn parse(text: &str) -> Option<Self> {
        let plain = text
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
        ((1..=Self::MAX).contains(&text.len()) && plain).then(|| Self(text.to_owned()))
    }
}

impl From<crate::user::User> for Desktop {
    fn from(user: crate::user::User) -> Self {
        Self {
            kind: Kind::User(user),
        }
    }
}

impl From<crate::vm::Vm> for Desktop {
    fn from(vm: crate::vm::Vm) -> Self {
        Self { kind: Kind::Vm(vm) }
    }
}

impl Desktop {
    /// What kind of desktop this is, for an owner to match on.
    pub fn kind(&self) -> &Kind {
        &self.kind
    }

    /// The hidden user this desktop is, when it is one: the way in for that
    /// user's own setup, update and removal, which work on its account, its
    /// processes in this kernel and its files on this disk. Anything that
    /// differs between kinds matches on [`Desktop::kind`] instead.
    pub fn user(&self) -> Option<&crate::user::User> {
        match &self.kind {
            Kind::User(user) => Some(user),
            Kind::Vm(_) => None,
        }
    }

    /// The name this desktop's locks and records carry. A second login's
    /// comes from its account alone, so a changed socket path names the
    /// same desktop.
    pub fn id(&self) -> DesktopId {
        let named = match &self.kind {
            Kind::User(user) => user.record_name(),
            // Atlas VM's own identifier already names it, in the same form.
            Kind::Vm(vm) => return vm.id.clone(),
        };
        let digest = bytes_sha256(named.as_bytes());
        DesktopId(digest[..DesktopId::LENGTH].to_owned())
    }

    /// The name this desktop's records carry. There is one Second Login,
    /// and replacing its account repairs it, so its records stay its own.
    pub fn record_key(&self) -> RecordKey {
        match &self.kind {
            Kind::User(_) => RecordKey::user(),
            Kind::Vm(vm) => vm.record_key(),
        }
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
            // A VM proves itself over its session, which the client opens;
            // one it cannot reach is refused before any observation is read.
            Kind::Vm(_) => Err(crate::error::Error::new(
                crate::error::Code::Platform,
                "This VM desktop's proof comes over its session.",
            )),
        }
    }

    /// Whether `observation` shows input service `pid` ready in this
    /// desktop with no controller and nothing held, running the installed
    /// build.
    pub fn input_current(&self, observation: &Observation, pid: u32) -> bool {
        match &self.kind {
            Kind::User(login) => observation.input_current(login.uid, pid),
            Kind::Vm(_) => false,
        }
    }

    /// Whether anything an ended job or a finished command left may still
    /// run in this desktop: a process it listed in `left`, or any process in
    /// its own group `group` or in a group a listed one was in. What cannot
    /// be read or listed may run.
    pub fn left_may_run(&self, left: &[Left], group: Option<i32>) -> bool {
        match &self.kind {
            Kind::User(user) => user.left_may_run(left, group),
            // A VM's processes are not in this kernel: told only by its guest.
            Kind::Vm(_) => true,
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
            Kind::Vm(_) => Ok(true),
        }
    }

    /// Process `pid` of this desktop, told apart from a later process with
    /// the same number; none when it cannot be looked at.
    pub fn instance(&self, pid: u32) -> Option<Instance> {
        match &self.kind {
            Kind::User(_) => crate::user::User::instance(pid),
            Kind::Vm(_) => None,
        }
    }

    /// Whether `instance` of this desktop has exited: its PID is gone, a
    /// zombie, or another process's. Unknown is not exited.
    pub fn exited(&self, instance: Instance) -> bool {
        match &self.kind {
            Kind::User(_) => crate::user::User::exited(instance),
            Kind::Vm(_) => false,
        }
    }
}

impl DesktopId {
    /// How many characters of the digest a name keeps.
    const LENGTH: usize = 24;

    /// `text` as a desktop's name, as one is written: [`Self::LENGTH`]
    /// lowercase hexadecimal digits of its digest.
    pub fn parse(text: &str) -> Option<Self> {
        let digits = text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        (text.len() == Self::LENGTH && digits).then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
#[path = "desktop_tests.rs"]
mod tests;
