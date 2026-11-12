//! Where agents' apps run: a desktop, what kind it is, the name its records
//! carry, and the proof its session has to give.
use crate::{
    digest::bytes_sha256,
    error::Result,
    session::{Observation, Session},
};

/// A desktop agents act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Desktop {
    kind: Kind,
}

/// Why a desktop cannot touch the person's screen.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Kind {
    /// A second login on this Mac, kept off the console.
    Hidden(Hidden),
}

/// A hidden login, by its account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hidden {
    pub uid: u32,
}

/// The name a desktop's locks and records carry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopId(String);

impl From<Hidden> for Desktop {
    fn from(hidden: Hidden) -> Self {
        Self {
            kind: Kind::Hidden(hidden),
        }
    }
}

impl Desktop {
    /// The hidden login this desktop is. Whoever asks relies on a second
    /// login on this Mac: its account, its processes in this kernel, its
    /// files on this disk.
    pub fn hidden(&self) -> &Hidden {
        let Kind::Hidden(hidden) = &self.kind;
        hidden
    }

    /// The name this desktop's locks and records carry. A hidden login's
    /// comes from its account alone, so a changed socket path names the
    /// same desktop.
    pub fn id(&self) -> DesktopId {
        let Kind::Hidden(hidden) = &self.kind;
        let digest = bytes_sha256(format!("helper:{}", hidden.uid).as_bytes());
        DesktopId(digest[..DesktopId::LENGTH].to_owned())
    }

    /// Verify that `observation` describes this desktop, out of the
    /// person's sight.
    ///
    /// # Errors
    ///
    /// For a hidden login, what [`Observation::require_hidden`] refuses.
    pub fn prove(&self, observation: &Observation) -> Result<Session> {
        let Kind::Hidden(hidden) = &self.kind;
        observation.require_hidden(hidden.uid)
    }

    /// Whether `observation` shows input service `pid` ready in this
    /// desktop with no controller and nothing held, running the installed
    /// build.
    pub fn input_current(&self, observation: &Observation, pid: u32) -> bool {
        let Kind::Hidden(hidden) = &self.kind;
        observation.input_current(hidden.uid, pid)
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
