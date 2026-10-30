//! What to do about pending changes of unknown outcome, decided once for
//! every surface that words a step for them: the asking session recovers
//! its own, waits for others', or the person recovers.
use super::Key;
use crate::{control::Cause, ui::hints};

/// Whether every one of `causes` (the pending changes) was claimed by
/// session `own` and none is still being carried out: only then may that
/// session recover them itself. A command an agent started that still runs
/// is work under way, not an unknown outcome of anyone's, so it is passed
/// over. Every surface that says so asks this.
pub(super) fn owns_every_unknown(causes: &[Cause], own: &Key) -> bool {
    let mut unknown = causes.iter().filter(|cause| !cause.command).peekable();
    unknown.peek().is_some()
        && unknown.all(|cause| cause.session.as_ref() == Some(own) && !cause.running)
}

/// Who asks what to do about changes of unknown outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asker<'a> {
    /// An agent's command, of this host session when it has one.
    Agent(Option<&'a Key>),
    /// The person: from Atlas, or at a terminal with no host session.
    Person,
}

/// What to do about pending changes of unknown outcome: the one rule every
/// surface words its step by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recovery {
    /// Every one is the asking session's own and none still runs: it
    /// recovers them itself.
    Own,
    /// As [`Recovery::Own`], while a command an agent started still runs:
    /// recovery waits for it to end.
    OwnOnceCommandEnds,
    /// One may be another's: the agent waits, or tells the person.
    Others,
    /// The person asks, and recovers.
    Person,
    /// As [`Recovery::Person`], while a command an agent started still
    /// runs: recovery waits for it to end.
    PersonOnceCommandEnds,
}

impl Recovery {
    /// What `asker` does about the pending changes `causes`; none while
    /// every one is still being carried out, which is work in progress.
    pub fn of(causes: &[Cause], asker: Asker<'_>) -> Option<Self> {
        if causes.iter().all(|cause| cause.running) {
            return None;
        }
        let command_runs = causes.iter().any(|cause| cause.command);
        Some(match asker {
            Asker::Agent(Some(own)) if owns_every_unknown(causes, own) => {
                if command_runs {
                    Self::OwnOnceCommandEnds
                } else {
                    Self::Own
                }
            }
            Asker::Agent(_) => Self::Others,
            Asker::Person if command_runs => Self::PersonOnceCommandEnds,
            Asker::Person => Self::Person,
        })
    }

    /// Whether the asking session recovers them itself.
    pub fn own(self) -> bool {
        matches!(self, Self::Own | Self::OwnOnceCommandEnds)
    }

    /// Its step, in one sentence.
    pub fn step(self) -> &'static str {
        match self {
            Self::Own => hints::UNKNOWN_OWN,
            Self::OwnOnceCommandEnds => hints::UNKNOWN_OWN_ONCE_COMMAND_ENDS,
            Self::Others => hints::UNKNOWN_OTHERS,
            Self::Person => hints::UNKNOWN_FOR_PERSON,
            Self::PersonOnceCommandEnds => hints::UNKNOWN_FOR_PERSON_ONCE_COMMAND_ENDS,
        }
    }
}
