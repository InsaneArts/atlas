//! Whether a window-off Atlas VM confirmed ends a maintenance. The word of
//! the run that opened the window does, and any run's when none opened it.
//! Another run's does only once the opener is proven gone with its window:
//! the run that turned its own window's input off, answering now and not
//! the opener, says with the VM's power confirmed that every earlier run's
//! VM is gone, which it says only once it recovered under the lock no
//! earlier run still holds; a run still recovering is asked again while
//! the end's patience lasts. A lost answer never counts, and a build that
//! cannot say keeps the maintenance.
use super::{Maintainer, READING};
use crate::vm::{
    lifecycle::record::{Ended, Maintenance},
    runner::Operation,
};
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
};
use std::time::Instant;

impl Maintainer {
    /// The proof that the window-off `close`, done by the run `by` and
    /// answered by the run `answering`, ends `maintenance`; none when a run
    /// gone since did it, so the run answering now is asked again under a
    /// change of its own.
    ///
    /// # Errors
    ///
    /// Refuses while another run than the opener answers and cannot yet say
    /// every earlier run's VM is gone, does not answer, or is of a build
    /// that cannot say.
    pub(super) fn proof(
        &self,
        maintenance: &Maintenance,
        close: &Token,
        by: Option<String>,
        answering: Option<&str>,
    ) -> Result<Option<Ended>> {
        let Some(by) = by else {
            return Err(self.cannot_prove());
        };
        let ended = |earlier: Option<String>| Ended {
            by: by.clone(),
            close: close.clone(),
            earlier_gone: earlier.is_some().then_some(true),
            earlier,
        };
        let opener = match &maintenance.opened_by {
            Some(opener) if *opener != by => opener,
            _ => return Ok(Some(ended(None))),
        };
        if answering != Some(by.as_str()) {
            return Ok(None);
        }
        // A run still recovering says so once it has: asked again, about once
        // a second, while the patience lasts.
        let runner = self.runner();
        let deadline = Instant::now() + self.patience.within;
        loop {
            self.controller.interrupt().check()?;
            let answered = match runner.read_within(&Operation::State, READING) {
                Ok(state) if state.ok => {
                    match state.incarnation {
                        Some(said) if said.id == by => {}
                        // Ran again since it turned its window off.
                        Some(_) => return Ok(None),
                        None => return Err(self.another_run()),
                    }
                    match (state.confirmed, state.earlier_gone) {
                        (_, None) => return Err(self.cannot_prove()),
                        (Some(true), Some(true)) => return Ok(Some(ended(Some(opener.clone())))),
                        _ => true,
                    }
                }
                Ok(_) | Err(_) => false,
            };
            if Instant::now() + self.patience.every > deadline {
                return Err(if answered {
                    self.another_run()
                } else {
                    self.not_answering()
                });
            }
            std::thread::sleep(self.patience.every);
        }
    }

    /// Why an end waits on an Atlas VM of a build that cannot say whether
    /// its earlier runs' VMs are gone.
    fn cannot_prove(&self) -> Error {
        Error::new(
            Code::Update,
            format!(
                "{name}'s Atlas VM is from an older Atlas and cannot prove its earlier run's \
                 window gone, so agents wait. Update Atlas when convenient, then run atlas \
                 desktop maintain {name} --end again.",
                name = self.name
            ),
        )
    }
}

#[cfg(test)]
#[path = "proof_tests.rs"]
mod tests;
