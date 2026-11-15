//! Blocking work begun for connections no one has authenticated yet, or
//! that may be cancelled, bounded by what is still running: a permit is
//! held for as long as the work runs, which a timeout or a cancellation of
//! whoever waits on it does not shorten.
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
    session::key::PeerKey,
};
use atlas_session::{keys::PairingSecret, pairing::GuestRecords};
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Blocking work of that kind running at once; more is refused.
pub(super) const BLOCKING: usize = 8;

/// The refusal of work over the bound.
fn busy() -> Error {
    Error::new(Code::Busy, "The guest is busy; try again.")
}

/// Run `work` on a blocking thread under one of `permits`, held until it
/// returns: none when none is free, or it failed to run.
pub(super) async fn off_runtime<T: Send + 'static>(
    permits: &Arc<Semaphore>,
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let permit = permits.clone().try_acquire_owned().ok()?;
    tokio::task::spawn_blocking(move || {
        let _running = permit;
        work()
    })
    .await
    .ok()
}

/// A guest's pairing records, each read or write under one of `permits`
/// for as long as it runs.
pub(super) struct Bounded {
    pub(super) records: Arc<dyn GuestRecords>,
    pub(super) permits: Arc<Semaphore>,
}

impl Bounded {
    fn run<T>(&self, work: impl FnOnce(&dyn GuestRecords) -> Result<T>) -> Result<T> {
        let _running = self.permits.try_acquire().map_err(|_| busy())?;
        work(self.records.as_ref())
    }
}

impl GuestRecords for Bounded {
    fn committed(&self) -> Result<Option<(PeerKey, Token)>> {
        self.run(|records| records.committed())
    }

    fn commit(&self, station: PeerKey, change: &Token) -> Result<()> {
        self.run(|records| records.commit(station, change))
    }

    fn secret(&self) -> Result<Option<PairingSecret>> {
        self.run(|records| records.secret())
    }

    fn forget_secret(&self) -> Result<()> {
        self.run(|records| records.forget_secret())
    }
}

#[cfg(test)]
#[path = "bounded_tests.rs"]
mod tests;
