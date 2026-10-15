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
}

impl Error {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            code,
            error: message.into(),
            uncertain: false,
        }
    }
    pub fn uncertain(mut self) -> Self {
        self.uncertain = true;
        self
    }
}

/// Every failure code Atlas reports. Agents act on these (see the
/// plugin's troubleshooting.md, which a test keeps in step).
pub const CODES: &[&str] = &[
    "agents",
    "apps",
    "bad-request",
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
    "menubar",
    "native",
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
}
