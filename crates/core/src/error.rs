//! The one failure shape Atlas reports, as JSON and as text: a short code
//! agents act on, a plain sentence, and whether the action may have
//! happened anyway.
pub mod advice;

use serde::{Deserialize, Serialize};
use std::{error::Error as StdError, fmt};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Serialize)]
pub struct Error {
    ok: bool,
    pub code: Code,
    pub error: String,
    pub uncertain: bool,
    /// Who holds control, on busy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holder: Option<Holder>,
    /// More for a program to act on, such as candidates or a scan's diagnostics.
    #[serde(flatten)]
    pub details: serde_json::Map<String, serde_json::Value>,
    #[serde(skip)]
    pub source: Option<Box<dyn StdError + Send + Sync>>,
}

/// Who holds control when Atlas answers busy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Holder {
    /// The person, through a viewer.
    Person,
    /// Another Atlas command, usually another agent's.
    Agent,
}

impl Holder {
    /// The holder another Atlas process named, if it is one of these.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "person" => Some(Self::Person),
            "agent" => Some(Self::Agent),
            _ => None,
        }
    }
}

impl Error {
    /// The refusal in `reply`: `code`, its words or else `otherwise`, and
    /// whether what it refused may have happened anyway.
    pub fn refused(code: Code, reply: &serde_json::Value, otherwise: &str) -> Self {
        let error = Self::new(code, reply["error"].as_str().unwrap_or(otherwise));
        if reply["uncertain"] == true {
            error.uncertain()
        } else {
            error
        }
    }

    /// The refusal of what was tied to a session or display the helper has left.
    pub fn session_changed() -> Self {
        Self::new(
            Code::SessionChanged,
            "The helper changed since Atlas checked it; look again",
        )
    }

    /// The refusal of what needs a recent frame and has none.
    pub fn stale_frame() -> Self {
        Self::new(Code::StaleFrame, "Wait for a fresh frame, then try again")
    }

    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            code,
            error: message.into(),
            uncertain: false,
            holder: None,
            details: serde_json::Map::new(),
            source: None,
        }
    }

    pub fn with_source(mut self, source: impl StdError + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    #[must_use]
    pub fn uncertain(mut self) -> Self {
        self.uncertain = true;
        self
    }

    /// The same failure from something that only read: nothing in the
    /// helper changed, whatever it says.
    #[must_use]
    pub fn certain(self) -> Self {
        Self {
            uncertain: false,
            ..self
        }
    }

    /// An agent refused because the person stopped agents.
    pub fn stopped() -> Self {
        Self::new(Code::Stopped, STOPPED)
    }

    /// Say who holds control.
    #[must_use]
    pub fn held_by(mut self, holder: Holder) -> Self {
        self.holder = Some(holder);
        self
    }

    /// Add `value` under `key` for programs to act on. The keys every
    /// failure has are never replaced: such a key is ignored.
    ///
    /// Details must already be JSON values; arbitrary serializers cannot
    /// silently turn a failed serialization into null.
    ///
    /// ```compile_fail,E0308
    /// use atlas_core::error::{Code, Error};
    /// use serde::{Serialize, Serializer};
    ///
    /// struct Failing;
    /// impl Serialize for Failing {
    ///     fn serialize<S: Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
    ///         Err(serde::ser::Error::custom("cannot serialize"))
    ///     }
    /// }
    ///
    /// let _ = Error::new(Code::Io, "Read failed").with("detail", Failing);
    /// ```
    #[must_use]
    pub fn with(mut self, key: &str, value: serde_json::Value) -> Self {
        const OWN: [&str; 5] = ["ok", "code", "error", "uncertain", "holder"];
        if OWN.contains(&key) {
            return self;
        }

        self.details.insert(key.to_owned(), value);
        self
    }

    /// A file Atlas keeps could not be written at `path`. The kernel answers
    /// EPERM, not EACCES, when policy forbids the write: usually an agent's
    /// sandbox, sometimes a locked file. Atlas itself may be fine, so say so.
    pub fn writing(path: &std::path::Path, error: std::io::Error) -> Self {
        if error.raw_os_error() == Some(libc::EPERM) {
            return Self::new(
                Code::Blocked,
                format!(
                    "This shell may not write {} (operation not permitted), usually because \
                     an agent's sandbox blocks it: run atlas outside the sandbox. Outside one, \
                     check the file's permissions and flags.",
                    path.display()
                ),
            )
            .with_source(error);
        }

        Self::new(Code::Io, format!("{}: {error}", path.display())).with_source(error)
    }
}

/// Why an agent is refused while the person has stopped agents.
pub const STOPPED: &str =
    "The person stopped agents in the helper. Ask them before doing anything more there.";

/// What an agent hears when the person has control of the helper.
pub const PERSON_HAS_CONTROL: &str =
    "The person has control of the helper. Wait until they give it back, then try again.";

/// Keep the code identity, stable word and ordered catalog in one declaration.
macro_rules! codes {
    ($($code:ident => $word:literal),+ $(,)?) => {
        /// Every failure code Atlas reports, written identically on every boundary.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Code { $($code),+ }

        impl Code {
            /// Every code, in the order troubleshooting.md lists them.
            pub const ALL: [Self; [$($word),+].len()] = [$(Self::$code),+];

            /// The word for it, as every reply gives it.
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$code => $word),+ }
            }

            /// The code another Atlas process reported, if it is one of ours.
            pub fn parse(name: &str) -> Option<Self> {
                match name { $($word => Some(Self::$code),)+ _ => None }
            }
        }

        #[cfg(any(test, feature = "test-support"))]
        pub const CODES: [&str; Code::ALL.len()] = [$($word),+];
    };
}

codes! {
    Agents => "agents",
    Apps => "apps",
    BadRequest => "bad-request",
    Blocked => "blocked",
    Busy => "busy",
    Cancelled => "cancelled",
    Capture => "capture",
    Cleanup => "cleanup",
    Collect => "collect",
    Config => "config",
    ConfigInvalid => "config-invalid",
    Daemon => "daemon",
    Elements => "elements",
    Environment => "environment",
    FocusChanged => "focus-changed",
    Identity => "identity",
    Init => "init",
    Input => "input",
    Install => "install",
    Interrupted => "interrupted",
    InvalidJson => "invalid-json",
    Io => "io",
    Job => "job",
    LeaseLost => "lease-lost",
    LoggedOut => "logged-out",
    Logout => "logout",
    LogoutHeld => "logout-held",
    Menubar => "menubar",
    Native => "native",
    NoEnvironment => "no-environment",
    NoWindow => "no-window",
    NotReady => "not-ready",
    OnConsole => "on-console",
    Onboarding => "onboarding",
    Permission => "permission",
    Platform => "platform",
    PortInUse => "port-in-use",
    Protocol => "protocol",
    Restart => "restart",
    SessionChanged => "session-changed",
    Setup => "setup",
    SpawnFailed => "spawn-failed",
    StaleFrame => "stale-frame",
    Startup => "startup",
    State => "state",
    Stopped => "stopped",
    Switch => "switch",
    Terminal => "terminal",
    Transport => "transport",
    Uncertain => "uncertain",
    Uninstall => "uninstall",
    Update => "update",
    Usage => "usage",
    WaitFailed => "wait-failed",
    Windows => "windows",
}

impl Serialize for Code {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Code {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let word = String::deserialize(deserializer)?;
        Self::parse(&word)
            .ok_or_else(|| serde::de::Error::custom(format!("{word} is not a failure code")))
    }
}

impl Code {
    /// The code `reply` names when it is one of Atlas's, else `otherwise`.
    pub fn of_reply(reply: &serde_json::Value, otherwise: Self) -> Self {
        reply["code"]
            .as_str()
            .and_then(Self::parse)
            .unwrap_or(otherwise)
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A code compared with the word a reply or a test gives.
impl PartialEq<&str> for Code {
    fn eq(&self, word: &&str) -> bool {
        self.as_str() == *word
    }
}

impl PartialEq<str> for Code {
    fn eq(&self, word: &str) -> bool {
        self.as_str() == word
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Error")
            .field("ok", &self.ok)
            .field("code", &self.code)
            .field("error", &self.error)
            .field("uncertain", &self.uncertain)
            .field("holder", &self.holder)
            .field("details", &self.details)
            .finish()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.error)
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source.as_deref().map(|source| source as _)
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::new(Code::Io, value.to_string()).with_source(value)
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::new(Code::InvalidJson, value.to_string()).with_source(value)
    }
}

#[cfg(test)]
mod tests;
