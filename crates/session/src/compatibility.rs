//! What a guest declares it speaks and serves, and the station's judgment
//! of it for each operation: by protocol and capabilities, never by build.
//!
//! The protocol changes only when what goes over the link changes in a way
//! an older end cannot read: a control message, a field of one, a lane
//! kind or a header field. Anything else new is a new capability name, and
//! an old name's meaning never changes. The transport's own version, in
//! the handshake's prologue, never changes, so two ends can always say
//! which protocol each speaks.
use atlas_core::error::{Code, Error};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The session protocol this build speaks.
pub const PROTOCOL: u32 = 1;

/// Change IDs, each kept with what it came to and settled by the station.
pub const CHANGES: &str = "changes-v1";
/// Whether work the guest started may still run.
pub const LEFTOVERS: &str = "leftovers-v1";
/// Files and folders staged into the guest.
pub const STAGE: &str = "stage-v1";
/// The guest's copies listed and removed.
pub const COPIES: &str = "copies-v1";
/// Input lanes to the guest's input service.
pub const INPUT_LANES: &str = "input-lanes-v1";

/// What a guest declares in its hello.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Declared {
    pub protocol: u32,
    pub capabilities: Vec<String>,
    /// Its Atlas version, for the person: nothing decides on it.
    pub version: String,
}

impl Declared {
    /// What this build declares, serving `capabilities`.
    pub fn ours(capabilities: Vec<String>) -> Self {
        Self {
            protocol: PROTOCOL,
            capabilities,
            version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }

    /// What a guest's first control message declares, read leniently from
    /// `hello`, as plain JSON: whatever else it holds, in whatever protocol.
    ///
    /// # Errors
    ///
    /// Refuses a message that is not a hello, or that declares no protocol,
    /// capabilities or version.
    pub fn of_hello(message: &Value) -> atlas_core::error::Result<Self> {
        let hello = &message["hello"];
        let malformed = || {
            Error::new(
                Code::Protocol,
                "The desktop's control connection did not begin with a hello this Mac can read.",
            )
        };
        let protocol = hello["protocol"]
            .as_u64()
            .and_then(|protocol| u32::try_from(protocol).ok())
            .ok_or_else(malformed)?;
        let capabilities = hello["capabilities"]
            .as_array()
            .ok_or_else(malformed)?
            .iter()
            .map(|name| name.as_str().map(str::to_owned).ok_or_else(malformed))
            .collect::<atlas_core::error::Result<_>>()?;
        let version = hello["version"].as_str().ok_or_else(malformed)?.to_owned();
        Ok(Self {
            protocol,
            capabilities,
            version,
        })
    }
}

/// Whether a guest can serve an operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Usable {
    /// Its protocol supports it. Whether it may run now is judged apart.
    Usable,
    /// The same protocol, without these: an older guest, or one built
    /// without the service behind them.
    MissingCapabilities { missing: Vec<String> },
    /// Another protocol: `theirs`.
    Incompatible { theirs: u32 },
}

impl Usable {
    /// The refusal of an operation that cannot be served, with what to do.
    pub fn refusal(&self) -> Option<Error> {
        match self {
            Self::Usable => None,
            Self::MissingCapabilities { missing } => Some(Error::new(
                Code::Update,
                format!(
                    "The desktop's Atlas does not serve this ({}).",
                    missing.join(", ")
                ),
            )),
            Self::Incompatible { theirs } => {
                let older = if *theirs < PROTOCOL {
                    "Update Atlas on the desktop"
                } else {
                    "Update Atlas on this Mac"
                };
                Some(Error::new(
                    Code::Update,
                    format!(
                        "The desktop speaks Atlas protocol {theirs}, this Mac {PROTOCOL}. {older}."
                    ),
                ))
            }
        }
    }
}

/// Judge `declared` for an operation that `needs` these capabilities.
/// Names this build does not know, and more than are needed, change
/// nothing; the version never does.
pub fn judge(declared: &Declared, needs: &[&str]) -> Usable {
    if declared.protocol != PROTOCOL {
        return Usable::Incompatible {
            theirs: declared.protocol,
        };
    }
    let missing: Vec<String> = needs
        .iter()
        .filter(|need| !declared.capabilities.iter().any(|has| has == *need))
        .map(|need| (*need).to_owned())
        .collect();
    if missing.is_empty() {
        Usable::Usable
    } else {
        Usable::MissingCapabilities { missing }
    }
}

#[cfg(test)]
#[path = "compatibility_tests.rs"]
mod tests;
