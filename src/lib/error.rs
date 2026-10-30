//! The one failure shape Atlas reports, as JSON and as text: a short code
//! agents act on, a plain sentence, and whether the action may have
//! happened anyway.
use serde::{Deserialize, Serialize};
use std::{error::Error as StdError, fmt};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Serialize)]
pub struct Error {
    pub ok: bool,
    pub code: Code,
    pub error: String,
    pub uncertain: bool,
    /// Who holds control, on busy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holder: Option<Holder>,
    /// More for a program to act on, such as candidates or a scan's diagnostics.
    #[serde(flatten)]
    pub details: serde_json::Map<String, serde_json::Value>,
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
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            code,
            error: message.into(),
            uncertain: false,
            holder: None,
            details: serde_json::Map::new(),
        }
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
    /// failure has are never replaced: such a key is ignored (and refused
    /// in a debug build).
    #[must_use]
    pub fn with(mut self, key: &str, value: impl Serialize) -> Self {
        const OWN: [&str; 5] = ["ok", "code", "error", "uncertain", "holder"];
        debug_assert!(!OWN.contains(&key), "{key} is part of every failure");
        if OWN.contains(&key) {
            return self;
        }

        let value = serde_json::to_value(value).unwrap_or(serde_json::Value::Null);
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
            );
        }

        Self::new(Code::Io, format!("{}: {error}", path.display()))
    }
}

/// Why an agent is refused while the person has stopped agents.
pub const STOPPED: &str =
    "The person stopped agents in the helper. Ask them before doing anything more there.";

/// What an agent hears when the person has control of the helper.
pub const PERSON_HAS_CONTROL: &str =
    "The person has control of the helper. Wait until they give it back, then try again.";

/// Every failure code Atlas reports. Agents act on these (see the
/// plugin's troubleshooting.md, which a test keeps in step), and each is
/// written as its word, the same on the wire, in the activity log and in
/// every reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Code {
    Agents,
    Apps,
    BadRequest,
    Blocked,
    Busy,
    Cancelled,
    Capture,
    Cleanup,
    Collect,
    Config,
    ConfigInvalid,
    Daemon,
    Elements,
    Environment,
    FocusChanged,
    Identity,
    Init,
    Input,
    Install,
    Interrupted,
    InvalidJson,
    Io,
    Job,
    LeaseLost,
    LoggedOut,
    Logout,
    LogoutHeld,
    Menubar,
    Native,
    NoEnvironment,
    NoWindow,
    NotReady,
    OnConsole,
    Onboarding,
    Permission,
    Platform,
    PortInUse,
    Protocol,
    Restart,
    SessionChanged,
    Setup,
    SpawnFailed,
    StaleFrame,
    Startup,
    State,
    Stopped,
    Switch,
    Terminal,
    Transport,
    Uncertain,
    Uninstall,
    Update,
    Usage,
    WaitFailed,
    Windows,
}

impl Code {
    /// Every code, in the order troubleshooting.md lists them.
    pub const ALL: [Self; 55] = [
        Self::Agents,
        Self::Apps,
        Self::BadRequest,
        Self::Blocked,
        Self::Busy,
        Self::Cancelled,
        Self::Capture,
        Self::Cleanup,
        Self::Collect,
        Self::Config,
        Self::ConfigInvalid,
        Self::Daemon,
        Self::Elements,
        Self::Environment,
        Self::FocusChanged,
        Self::Identity,
        Self::Init,
        Self::Input,
        Self::Install,
        Self::Interrupted,
        Self::InvalidJson,
        Self::Io,
        Self::Job,
        Self::LeaseLost,
        Self::LoggedOut,
        Self::Logout,
        Self::LogoutHeld,
        Self::Menubar,
        Self::Native,
        Self::NoEnvironment,
        Self::NoWindow,
        Self::NotReady,
        Self::OnConsole,
        Self::Onboarding,
        Self::Permission,
        Self::Platform,
        Self::PortInUse,
        Self::Protocol,
        Self::Restart,
        Self::SessionChanged,
        Self::Setup,
        Self::SpawnFailed,
        Self::StaleFrame,
        Self::Startup,
        Self::State,
        Self::Stopped,
        Self::Switch,
        Self::Terminal,
        Self::Transport,
        Self::Uncertain,
        Self::Uninstall,
        Self::Update,
        Self::Usage,
        Self::WaitFailed,
        Self::Windows,
    ];

    /// The word for it, as every reply gives it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::Apps => "apps",
            Self::BadRequest => "bad-request",
            Self::Blocked => "blocked",
            Self::Busy => "busy",
            Self::Cancelled => "cancelled",
            Self::Capture => "capture",
            Self::Cleanup => "cleanup",
            Self::Collect => "collect",
            Self::Config => "config",
            Self::ConfigInvalid => "config-invalid",
            Self::Daemon => "daemon",
            Self::Elements => "elements",
            Self::Environment => "environment",
            Self::FocusChanged => "focus-changed",
            Self::Identity => "identity",
            Self::Init => "init",
            Self::Input => "input",
            Self::Install => "install",
            Self::Interrupted => "interrupted",
            Self::InvalidJson => "invalid-json",
            Self::Io => "io",
            Self::Job => "job",
            Self::LeaseLost => "lease-lost",
            Self::LoggedOut => "logged-out",
            Self::Logout => "logout",
            Self::LogoutHeld => "logout-held",
            Self::Menubar => "menubar",
            Self::Native => "native",
            Self::NoEnvironment => "no-environment",
            Self::NoWindow => "no-window",
            Self::NotReady => "not-ready",
            Self::OnConsole => "on-console",
            Self::Onboarding => "onboarding",
            Self::Permission => "permission",
            Self::Platform => "platform",
            Self::PortInUse => "port-in-use",
            Self::Protocol => "protocol",
            Self::Restart => "restart",
            Self::SessionChanged => "session-changed",
            Self::Setup => "setup",
            Self::SpawnFailed => "spawn-failed",
            Self::StaleFrame => "stale-frame",
            Self::Startup => "startup",
            Self::State => "state",
            Self::Stopped => "stopped",
            Self::Switch => "switch",
            Self::Terminal => "terminal",
            Self::Transport => "transport",
            Self::Uncertain => "uncertain",
            Self::Uninstall => "uninstall",
            Self::Update => "update",
            Self::Usage => "usage",
            Self::WaitFailed => "wait-failed",
            Self::Windows => "windows",
        }
    }

    /// The code another Atlas process reported, if it is one of ours.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|code| code.as_str() == name)
    }
}

/// Every code's word, in the order [`Code::ALL`] gives them.
pub const CODES: [&str; 55] = {
    let mut words = [""; 55];
    let mut index = 0;
    while index < 55 {
        words[index] = Code::ALL[index].as_str();
        index += 1;
    }
    words
};

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

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.error)
    }
}

impl StdError for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::new(Code::Io, value.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::new(Code::InvalidJson, value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failure_names_its_holder_and_details_only_when_set() {
        let plain = Error::new(Code::Busy, "Wait");
        assert_eq!(
            serde_json::to_value(&plain).unwrap(),
            serde_json::json!({"ok": false, "code": "busy", "error": "Wait", "uncertain": false})
        );

        let held = Error::new(Code::Busy, "Wait")
            .held_by(Holder::Person)
            .with("next", "Wait for them");
        assert_eq!(
            serde_json::to_value(&held).unwrap(),
            serde_json::json!({"ok": false, "code": "busy", "error": "Wait", "uncertain": false,
                               "holder": "person", "next": "Wait for them"})
        );

        assert_eq!(Holder::parse("agent"), Some(Holder::Agent));
        assert_eq!(Holder::parse("person"), Some(Holder::Person));
        assert_eq!(Holder::parse("someone"), None);
    }

    /// Agents learn every code from the plugin's table, and only those.
    #[test]
    fn the_codes_are_the_documented_ones() {
        let table = include_str!("../../agents/atlas/skills/atlas/references/troubleshooting.md");
        let documented: Vec<&str> = table
            .lines()
            .filter_map(|line| line.strip_prefix("| `"))
            .filter_map(|rest| rest.split('`').next())
            .collect();

        assert_eq!(
            documented, CODES,
            "keep CODES and troubleshooting.md in step"
        );
        assert_eq!(Code::parse("busy"), Some(Code::Busy));
        assert_eq!(Code::parse("made-up"), None);
    }

    /// Each code is written as the same word everywhere, whichever way
    /// it goes out: programs, Atlas.app and older builds match on these.
    #[test]
    fn every_code_keeps_its_word() {
        let words = [
            (Code::Agents, "agents"),
            (Code::Apps, "apps"),
            (Code::BadRequest, "bad-request"),
            (Code::Blocked, "blocked"),
            (Code::Busy, "busy"),
            (Code::Cancelled, "cancelled"),
            (Code::Capture, "capture"),
            (Code::Cleanup, "cleanup"),
            (Code::Collect, "collect"),
            (Code::Config, "config"),
            (Code::ConfigInvalid, "config-invalid"),
            (Code::Daemon, "daemon"),
            (Code::Elements, "elements"),
            (Code::Environment, "environment"),
            (Code::FocusChanged, "focus-changed"),
            (Code::Identity, "identity"),
            (Code::Init, "init"),
            (Code::Input, "input"),
            (Code::Install, "install"),
            (Code::Interrupted, "interrupted"),
            (Code::InvalidJson, "invalid-json"),
            (Code::Io, "io"),
            (Code::Job, "job"),
            (Code::LeaseLost, "lease-lost"),
            (Code::LoggedOut, "logged-out"),
            (Code::Logout, "logout"),
            (Code::LogoutHeld, "logout-held"),
            (Code::Menubar, "menubar"),
            (Code::Native, "native"),
            (Code::NoEnvironment, "no-environment"),
            (Code::NoWindow, "no-window"),
            (Code::NotReady, "not-ready"),
            (Code::OnConsole, "on-console"),
            (Code::Onboarding, "onboarding"),
            (Code::Permission, "permission"),
            (Code::Platform, "platform"),
            (Code::PortInUse, "port-in-use"),
            (Code::Protocol, "protocol"),
            (Code::Restart, "restart"),
            (Code::SessionChanged, "session-changed"),
            (Code::Setup, "setup"),
            (Code::SpawnFailed, "spawn-failed"),
            (Code::StaleFrame, "stale-frame"),
            (Code::Startup, "startup"),
            (Code::State, "state"),
            (Code::Stopped, "stopped"),
            (Code::Switch, "switch"),
            (Code::Terminal, "terminal"),
            (Code::Transport, "transport"),
            (Code::Uncertain, "uncertain"),
            (Code::Uninstall, "uninstall"),
            (Code::Update, "update"),
            (Code::Usage, "usage"),
            (Code::WaitFailed, "wait-failed"),
            (Code::Windows, "windows"),
        ];
        assert_eq!(words.map(|(code, _)| code), Code::ALL);
        for (code, word) in words {
            assert_eq!(code.as_str(), word);
            assert_eq!(serde_json::to_value(code).unwrap(), word);
            assert_eq!(serde_json::from_value::<Code>(word.into()).unwrap(), code);
            assert_eq!(code.to_string(), word);
        }
        assert_eq!(CODES, words.map(|(_, word)| word));
    }
}
