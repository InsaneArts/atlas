//! Whether a window-off Atlas VM confirmed ends a maintenance, as the
//! window's own proof judges it, each refusal said of the maintenance.
use super::Maintainer;
use crate::vm::{
    lifecycle::record::{Ended, Maintenance},
    window::{self, Proof},
};
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
};

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
        let opener = maintenance.opened_by.as_deref();
        match window::proof(self, opener, close, by, answering, self.window_patience())? {
            Proof::Ended(ended) => Ok(Some(ended)),
            Proof::Again => Ok(None),
            Proof::AnotherRun => Err(self.another_run()),
            Proof::NotAnswering => Err(self.not_answering()),
            Proof::CannotProve => Err(self.cannot_prove()),
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
