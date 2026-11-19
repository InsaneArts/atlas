//! A VM's window opened with the person's input and turned off again, as
//! Atlas VM is asked for each: shared by the person's maintenance and the
//! add's setup window.
//!
//! A window change is kept in the VM's lifecycle record before it is sent,
//! and followed until Atlas VM keeps an outcome for it: a lost reply is
//! followed by what Atlas VM says it came to, and the very same change is
//! sent again when Atlas VM never heard of it. A window-on still on its way
//! is settled for good before its window-off is asked: Atlas VM answers
//! what it came to, or voids it, never having seen it.
//!
//! A window-off ends what it closes only on the word of the run that opened
//! the window, of any run's when none did, or of a later run that turned
//! its own window's input off and says every earlier run's VM, the
//! opener's among them, is gone: it says that only once it recovered under
//! the lock no earlier run still holds, so a run still recovering is asked
//! again while the patience lasts. A lost answer never counts, and a build
//! that cannot say proves nothing.
use super::{
    lifecycle::record::{Ended, Record},
    runner::{Operation, Outcome, Reply},
};
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
};
use std::time::{Duration, Instant};

/// Atlas VM, as a window change meets it, and the time it is given: given
/// apart so each caller asks in its own way and tests stand in.
pub(super) trait AtlasVm {
    /// Ask the change `operation`, kept in `record` before it is sent, the
    /// whole exchange within `within`.
    fn change(&self, record: &mut Record, operation: &Operation, within: Duration)
    -> Result<Reply>;
    /// Ask the read `operation`.
    fn read(&self, operation: &Operation) -> Result<Reply>;
    /// Refuse once the command was interrupted.
    fn interrupted(&self) -> Result<()>;
    /// Let `how_long` go by.
    fn pause(&self, how_long: Duration);
    /// Now, as the pauses count it.
    fn now(&self) -> Instant;
}

/// How long a change is followed, and how often it is looked at.
#[derive(Clone, Copy, Debug)]
pub(super) struct Patience {
    pub(super) within: Duration,
    pub(super) every: Duration,
}

/// What a window change asked of Atlas VM came to.
pub(super) enum Asked {
    /// Atlas VM's own word, with the run that did it and the run that
    /// answered.
    Came {
        outcome: Outcome,
        by: Option<String>,
        answering: Option<String>,
    },
    /// Its only submission was not taken, for this reason: nothing was
    /// begun.
    NotTaken(String),
    /// Not known yet.
    Unknown,
}

impl Asked {
    /// What Atlas VM's `reply` says the change came to, naming the runs;
    /// none while it does not say.
    fn came(reply: Reply) -> Option<Self> {
        Some(Self::Came {
            outcome: reply.outcome?,
            by: reply.by,
            answering: reply.incarnation.map(|incarnation| incarnation.id),
        })
    }
}

/// Whether a window-off Atlas VM confirmed ends what it closes.
#[derive(Debug)]
pub(super) enum Proof {
    /// It does: the proof to keep.
    Ended(Ended),
    /// A run gone since did it: the run answering now is asked again,
    /// under a change of its own.
    Again,
    /// Another run than the opener answers and has not said every earlier
    /// run's VM is gone, while the patience lasted.
    AnotherRun,
    /// Atlas VM did not answer while the patience lasted.
    NotAnswering,
    /// Atlas VM is of a build that cannot say who did it, or whether its
    /// earlier runs' VMs are gone.
    CannotProve,
}

/// Ask Atlas VM `vm` for the change `operation`, kept in `record` before it
/// is sent, and follow it until Atlas VM keeps an outcome for it or
/// `patience` runs out. `uncertain` when it may have been sent before. A
/// lost reply is followed by what Atlas VM says it came to, and the very
/// same change is sent again when Atlas VM never heard of it. A refusal
/// unkept is taken as the change's only while nothing else may have sent
/// it.
///
/// # Errors
///
/// Fails when the command is interrupted, the change cannot be kept, or
/// Atlas VM answers for another change.
pub(super) fn ask(
    vm: &impl AtlasVm,
    record: &mut Record,
    operation: &Operation,
    uncertain: bool,
    patience: Patience,
) -> Result<Asked> {
    let change = operation
        .change()
        .cloned()
        .ok_or_else(|| Error::new(Code::BadRequest, "A read is not a change of the VM."))?;
    let deadline = vm.now() + patience.within;
    let mut uncertain = uncertain;
    let mut asking = true;
    loop {
        if asking {
            asking = false;
            vm.interrupted()?;
            let left = deadline.saturating_duration_since(vm.now());
            match vm.change(record, operation, left) {
                Ok(reply) if reply.ok => {
                    if let Some(came) = Asked::came(reply) {
                        return Ok(came);
                    }
                    uncertain = true;
                }
                Ok(reply) if !uncertain => {
                    return Ok(Asked::NotTaken(reply.error.unwrap_or_else(|| {
                        "Atlas VM did not take the change.".to_owned()
                    })));
                }
                Ok(_) => {}
                Err(error) if error.code == Code::Transport => uncertain = true,
                Err(error) => return Err(error),
            }
        }
        if vm.now() + patience.every > deadline {
            return Ok(Asked::Unknown);
        }
        vm.pause(patience.every);
        match vm.read(&Operation::Outcome(change.clone())) {
            Ok(reply) if reply.ok && reply.change.as_ref() == Some(&change) => {
                let pending = reply.pending;
                match (Asked::came(reply), pending) {
                    (Some(came), _) => return Ok(came),
                    (None, Some(true)) => {}
                    (None, _) => asking = true,
                }
            }
            Ok(_) | Err(_) => {}
        }
    }
}

/// Atlas VM `vm`'s answer to a void of `change`, asked until it says what
/// `change` came to; none once `patience` runs out first.
///
/// # Errors
///
/// Fails when the command is interrupted.
pub(super) fn voided(
    vm: &impl AtlasVm,
    change: &Token,
    patience: Patience,
) -> Result<Option<Reply>> {
    let deadline = vm.now() + patience.within;
    loop {
        vm.interrupted()?;
        if let Ok(reply) = vm.read(&Operation::Void(change.clone()))
            && reply.ok
            && reply.change.as_ref() == Some(change)
            && reply.outcome.is_some()
        {
            return Ok(Some(reply));
        }
        if vm.now() + patience.every > deadline {
            return Ok(None);
        }
        vm.pause(patience.every);
    }
}

/// Whether the window-off `close`, done by the run `by` and answered by the
/// run `answering`, ends a window `opened_by` opened, none when no run is
/// known to have: a run still recovering says so once it has, asked again,
/// `patience.every` apart, while `patience` lasts.
///
/// # Errors
///
/// Fails when the command is interrupted.
pub(super) fn proof(
    vm: &impl AtlasVm,
    opened_by: Option<&str>,
    close: &Token,
    by: Option<String>,
    answering: Option<&str>,
    patience: Patience,
) -> Result<Proof> {
    let Some(by) = by else {
        return Ok(Proof::CannotProve);
    };
    let ended = |earlier: Option<String>| {
        Proof::Ended(Ended {
            by: by.clone(),
            close: close.clone(),
            earlier_gone: earlier.is_some().then_some(true),
            earlier,
        })
    };
    let opener = match opened_by {
        Some(opener) if opener != by => opener,
        _ => return Ok(ended(None)),
    };
    if answering != Some(by.as_str()) {
        return Ok(Proof::Again);
    }
    let deadline = vm.now() + patience.within;
    loop {
        vm.interrupted()?;
        let answered = match vm.read(&Operation::State) {
            Ok(state) if state.ok => {
                match state.incarnation {
                    Some(said) if said.id == by => {}
                    // Ran again since it turned its window off.
                    Some(_) => return Ok(Proof::Again),
                    None => return Ok(Proof::AnotherRun),
                }
                match (state.confirmed, state.earlier_gone) {
                    (_, None) => return Ok(Proof::CannotProve),
                    (Some(true), Some(true)) => return Ok(ended(Some(opener.to_owned()))),
                    _ => true,
                }
            }
            Ok(_) | Err(_) => false,
        };
        if vm.now() + patience.every > deadline {
            return Ok(if answered {
                Proof::AnotherRun
            } else {
                Proof::NotAnswering
            });
        }
        vm.pause(patience.every);
    }
}
