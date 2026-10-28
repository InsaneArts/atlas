//! The one failure shape Atlas reports, as JSON and as text: a short code
//! agents act on, a plain sentence, and whether the action may have
//! happened anyway.
use serde::Serialize;
use std::{error::Error as StdError, fmt};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Serialize)]
pub struct Error {
    pub ok: bool,
    pub code: &'static str,
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
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
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
                "blocked",
                format!(
                    "This shell may not write {} (operation not permitted), usually because \
                     an agent's sandbox blocks it: run atlas outside the sandbox. Outside one, \
                     check the file's permissions and flags.",
                    path.display()
                ),
            );
        }

        Self::new("io", format!("{}: {error}", path.display()))
    }
}

/// Why an agent is refused while the person has stopped agents.
pub const STOPPED: &str =
    "The person stopped agents in the helper. Ask them before doing anything more there.";

/// What an agent hears when the person has control of the helper.
pub const PERSON_HAS_CONTROL: &str =
    "The person has control of the helper. Wait until they give it back, then try again.";

/// Every failure code Atlas reports. Agents act on these (see the
/// plugin's troubleshooting.md, which a test keeps in step).
pub const CODES: &[&str] = &[
    "agents",
    "apps",
    "bad-request",
    "blocked",
    "busy",
    "cancelled",
    "capture",
    "config",
    "config-invalid",
    "daemon",
    "elements",
    "environment",
    "identity",
    "init",
    "input",
    "install",
    "invalid-json",
    "io",
    "lease-lost",
    "logged-out",
    "logout",
    "logout-held",
    "menubar",
    "native",
    "no-environment",
    "no-window",
    "not-ready",
    "on-console",
    "onboarding",
    "permission",
    "platform",
    "port-in-use",
    "protocol",
    "restart",
    "session-changed",
    "setup",
    "spawn-failed",
    "stale-frame",
    "startup",
    "state",
    "stopped",
    "switch",
    "terminal",
    "transport",
    "uncertain",
    "uninstall",
    "update",
    "usage",
    "wait-failed",
    "windows",
];

/// The code another Atlas process reported, if it is one of ours.
pub fn code(name: &str) -> Option<&'static str> {
    CODES.iter().copied().find(|code| *code == name)
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.error)
    }
}

impl StdError for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::new("io", value.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::new("invalid-json", value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failure_names_its_holder_and_details_only_when_set() {
        let plain = Error::new("busy", "Wait");
        assert_eq!(
            serde_json::to_value(&plain).unwrap(),
            serde_json::json!({"ok": false, "code": "busy", "error": "Wait", "uncertain": false})
        );

        let held = Error::new("busy", "Wait")
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
        assert_eq!(code("busy"), Some("busy"));
        assert_eq!(code("made-up"), None);
    }

    /// Every code a source file passes to the error constructor is one of
    /// [`CODES`].
    #[test]
    fn every_constructed_code_is_listed() {
        // Spelled in two parts so this file's own text never matches.
        let call = concat!("Error::new", "(");
        let mut directories = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        let mut unlisted = Vec::new();
        let mut seen = 0;

        while let Some(directory) = directories.pop() {
            for entry in std::fs::read_dir(directory).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    directories.push(path);
                    continue;
                }

                if path.extension().is_none_or(|extension| extension != "rs") {
                    continue;
                }

                let source = std::fs::read_to_string(&path).unwrap();
                for (index, _) in source.match_indices(call) {
                    let rest = source[index + call.len()..].trim_start();
                    // Codes are lower-case words; anything else is the call
                    // spelled inside some other string, such as a test's.
                    let Some(literal) = rest
                        .strip_prefix('"')
                        .and_then(|r| r.split('"').next())
                        .filter(|literal| {
                            !literal.is_empty()
                                && literal.chars().all(|c| c.is_ascii_lowercase() || c == '-')
                        })
                    else {
                        continue;
                    };

                    seen += 1;
                    if code(literal).is_none() {
                        unlisted.push(format!("{}: {literal}", path.display()));
                    }
                }
            }
        }

        assert!(seen > 100, "only {seen} constructions found");
        assert!(
            unlisted.is_empty(),
            "codes missing from CODES: {unlisted:?}"
        );
    }
}
