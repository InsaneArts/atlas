//! The person's maintenance of a VM desktop: its window takes their keyboard
//! and mouse, so agents wait. It is kept in the VM's lifecycle record before
//! anything of it is asked, and forgotten only once Atlas VM confirmed the
//! window takes no input. Every admission of an agent reads it here, the
//! station's and the keeper's, and says what holds them in the VM's own
//! words. A record that cannot be read holds agents as a kept one does.
mod maintain;

pub use maintain::{Maintain, maintain};

#[cfg(test)]
pub(in crate::vm) use maintain::maintained_by;

use super::lifecycle::{Lifecycle, record::Maintenance};
use atlas_core::{
    desktop::DesktopId,
    error::{Code, Error, Holder},
};
use serde_json::{Value, json};

/// What a VM's lifecycle record says of its maintenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Kept {
    /// None is kept.
    None,
    /// One is kept and its window may take the person's input.
    On(Maintenance),
    /// One is ending: its window's input is asked off, or confirmed off and
    /// not yet forgotten.
    Ending(Maintenance),
    /// The record cannot be read.
    Unreadable,
}

impl Kept {
    /// The maintenance kept for the VM `vm` in `lifecycle`, and the VM's name
    /// as its record says it; none when no lifecycle can be read.
    pub(crate) fn read(lifecycle: Option<&Lifecycle>, vm: &DesktopId) -> (Self, Option<String>) {
        let Some(lifecycle) = lifecycle else {
            return (Self::Unreadable, None);
        };
        match lifecycle.maintenance_of(vm) {
            Ok(None) => (Self::None, None),
            Ok(Some((name, maintenance))) => (Self::of(Some(maintenance)), Some(name)),
            Err(_) => (Self::Unreadable, None),
        }
    }

    /// What `maintenance`, as a record read whole keeps it, holds.
    pub(crate) fn of(maintenance: Option<Maintenance>) -> Self {
        match maintenance {
            None => Self::None,
            Some(maintenance) if maintenance.ending() => Self::Ending(maintenance),
            Some(maintenance) => Self::On(maintenance),
        }
    }

    /// The maintenance kept, when one is read.
    pub(crate) const fn maintenance(&self) -> Option<&Maintenance> {
        match self {
            Self::On(maintenance) | Self::Ending(maintenance) => Some(maintenance),
            Self::None | Self::Unreadable => None,
        }
    }

    /// Why an agent waits on the VM `name`: none while no maintenance is
    /// kept.
    pub(crate) fn held(&self, name: &str) -> Option<Error> {
        Some(match self {
            Self::None => return None,
            Self::On(_) => on(name),
            Self::Ending(_) => Error::new(
                Code::Busy,
                format!(
                    "{name}'s maintenance is ending, but Atlas VM has not confirmed its window \
                     takes no input, so agents still wait."
                ),
            )
            .with(
                "next",
                json!(format!("Run atlas desktop maintain {name} --end again.")),
            )
            .held_by(Holder::Person),
            Self::Unreadable => Error::new(
                Code::State,
                format!("Whether {name} is under maintenance cannot be read, so agents wait."),
            ),
        })
    }

    /// Why the person's Resume of agents on the VM `name` is refused: none
    /// while no maintenance is kept.
    pub(crate) fn resume_refused(&self, name: &str) -> Option<Error> {
        Some(match self {
            Self::None => return None,
            Self::On(_) => resume_on(name),
            Self::Ending(_) => Error::new(
                Code::Busy,
                format!(
                    "Atlas VM has not confirmed that {name}'s window takes no input, so agents \
                     stay stopped. Run atlas desktop maintain {name} --end again, then resume."
                ),
            )
            .held_by(Holder::Person),
            Self::Unreadable => Error::new(
                Code::State,
                format!(
                    "Whether {name} is under maintenance cannot be read, so agents stay stopped."
                ),
            ),
        })
    }

    /// What a status and doctor say of the VM `name`'s maintenance: its
    /// state, and once one is kept, why agents wait and what ends it.
    pub(crate) fn said(&self, name: &str) -> Value {
        match self {
            Self::None => json!({"state": "none"}),
            Self::On(maintenance) => json!({
                "state": "on",
                "phase": maintenance.phase,
                "sinceMs": maintenance.since_ms,
                "reason": format!(
                    "{name} is under maintenance: its window takes your input and agents \
                     wait. End it with atlas desktop maintain {name} --end."
                ),
            }),
            Self::Ending(maintenance) => json!({
                "state": "ending",
                "phase": maintenance.phase,
                "sinceMs": maintenance.since_ms,
                "reason": format!(
                    "{name}'s maintenance is ending, but its window's input off is not \
                     confirmed yet. Run atlas desktop maintain {name} --end again."
                ),
            }),
            Self::Unreadable => json!({
                "state": "unknown",
                "reason": format!(
                    "Whether {name} is under maintenance cannot be read, so agents wait."
                ),
            }),
        }
    }
}

/// Why an agent waits on the VM `name` while its maintenance is on.
pub(crate) fn on(name: &str) -> Error {
    Error::new(
        Code::Busy,
        format!(
            "{name} is under maintenance: its window takes the person's input. Agents wait \
             until the person ends it."
        ),
    )
    .with(
        "next",
        json!(format!(
            "Ask the person; they end it with atlas desktop maintain {name} --end."
        )),
    )
    .held_by(Holder::Person)
}

/// Why the person's Resume of agents on the VM `name` is refused while its
/// maintenance is on.
pub(crate) fn resume_on(name: &str) -> Error {
    Error::new(
        Code::Busy,
        format!(
            "{name} is under maintenance, so agents stay stopped. End it with atlas desktop \
             maintain {name} --end, then resume."
        ),
    )
    .held_by(Holder::Person)
}

/// Why a status or doctor of a VM says it is not ready, as its status's
/// `maintenance` says it: none when none is kept, and a status that does not
/// say is never none.
pub(crate) fn holds(name: &str, said: &Value) -> Option<String> {
    match said["state"].as_str() {
        Some("none") => None,
        _ => Some(said["reason"].as_str().map_or_else(
            || format!("Whether {name} is under maintenance cannot be read, so agents wait."),
            str::to_owned,
        )),
    }
}

#[cfg(test)]
#[path = "maintenance_tests.rs"]
mod tests;
