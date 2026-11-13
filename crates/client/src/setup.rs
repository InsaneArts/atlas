//! What setting a helper up shares with every kind of desktop: the executor
//! that runs privileged steps, linking the installed command, the setup
//! error, and the permissions a helper still lacks. The user desktop's own
//! setup is [`crate::user::setup`].
pub mod executor;
pub mod installed_app;

pub use executor::{AdministratorRights, administrator_rights_current};

use atlas_core::error::{Code, Error};
use serde_json::Value;

/// A setup failure, as setup and its executor report it.
pub fn problem(message: impl Into<String>) -> Error {
    Error::new(Code::Init, message)
}

/// The full name a new helper account gets.
pub const FULL_NAME: &str = "Atlas";

/// The permissions the helper still lacks, by their System Settings names.
pub fn missing(status: &Value) -> Vec<&'static str> {
    [
        ("screenCapture", "Screen & System Audio Recording"),
        ("accessibility", "Accessibility"),
    ]
    .into_iter()
    .filter(|(key, _)| status["permissions"][*key] != true)
    .map(|(_, pane)| pane)
    .collect()
}
