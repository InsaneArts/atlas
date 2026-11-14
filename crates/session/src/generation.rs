//! Generations: the guest numbers each control connection it accepts, and
//! keeps the count on its disk, so a number is never given twice, across
//! restarts too.
use atlas_core::error::Result;
use serde::{Deserialize, Serialize};

/// One generation of a desktop's session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Generation(pub u64);

/// Where the guest keeps the last generation it gave.
pub trait Generations {
    /// The next generation, kept durably before it is returned.
    ///
    /// # Errors
    ///
    /// Fails when it cannot be kept; then no generation is given.
    fn next(&self) -> Result<Generation>;
}

/// Generations kept in memory, for tests.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Default)]
pub struct Counted(std::sync::atomic::AtomicU64);

#[cfg(any(test, feature = "test-support"))]
impl Generations for Counted {
    fn next(&self) -> Result<Generation> {
        Ok(Generation(
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1,
        ))
    }
}
