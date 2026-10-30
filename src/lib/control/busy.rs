//! The busy replies: who holds control when a command cannot take it (the
//! person through a viewer, another Atlas command, or a command an agent
//! started with `run`), and how long it waited first.
use super::Purpose;
use crate::error::{Code, Error, Holder};
use std::time::Duration;

/// A wait in whole seconds, rounded to the nearest, as a reply says how
/// long Atlas waited: a full 30-second wait that ends a few milliseconds
/// short of it is 30, as its message says.
pub(crate) fn seconds(waited: Duration) -> u64 {
    waited.saturating_add(Duration::from_millis(500)).as_secs()
}

/// Busy because the person has control through a viewer: never waited for.
/// `waited` is how long a controller given a wait had waited by then.
pub(super) fn busy_person(waited: Option<Duration>) -> Error {
    let error = Error::new(Code::Busy, crate::error::PERSON_HAS_CONTROL).held_by(Holder::Person);
    match waited {
        Some(waited) => error.with("waited", seconds(waited)),
        None => error,
    }
}

/// Busy because another Atlas command kept control for all of `wait`, after
/// this try `waited` that long; a controller given no wait answers at once.
/// A command Atlas.app runs takes the lock too, so "agent" is another Atlas
/// command, usually another agent's.
pub(super) fn busy_agent(waited: Option<Duration>, wait: Duration) -> Error {
    let message = match waited {
        Some(_) => format!(
            "Another Atlas command, often another agent's, kept control for all of the {} seconds Atlas could wait.",
            seconds(wait)
        ),
        None => "Another Atlas controller is active: wait for it to finish, or release it in the viewer."
            .to_owned(),
    };

    let error = Error::new(Code::Busy, message).held_by(Holder::Agent);
    match waited {
        Some(waited) => error
            .with("waited", seconds(waited))
            .with("next", crate::ui::hints::WAITED),
        None => error,
    }
}

/// Busy because `message` names commands agents started with `run` that
/// still run: held by an agent, so nobody is told to recover them, and
/// marked as a running command for programs to tell from a held lock.
pub(super) fn busy_running(message: impl Into<String>) -> Error {
    Error::new(Code::Busy, message)
        .held_by(Holder::Agent)
        .with("running", "command")
        .with("next", crate::ui::hints::COMMAND_RUNNING)
}

/// Busy because a command an agent started with `run` still runs, for
/// control taken for `purpose`: setup, removal and recovery hear that
/// stopping agents ends it at once.
pub(super) fn busy_command(purpose: Purpose) -> Error {
    busy_running(match purpose {
        Purpose::Recovery => {
            "A command an agent started is still running in the helper. Wait for it to end, or stop agents to end it now."
        }
        Purpose::Change(_) => {
            "A command an agent started is still running in the helper. Wait for it to end."
        }
    })
}
