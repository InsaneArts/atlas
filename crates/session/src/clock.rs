//! The clock a lease is measured on: one that counts the time the machine
//! slept, so a heartbeat from before a sleep never looks fresh after it.
use std::time::Duration;

/// A clock that should only go forward.
pub trait Clock: Send + Sync {
    /// How long since some fixed moment; none when the clock cannot be
    /// read.
    fn now(&self) -> Option<Duration>;
}

/// The system's continuous clock, which counts sleep (`CLOCK_MONOTONIC`
/// on macOS, the clock `mach_continuous_time` reads).
#[derive(Clone, Copy, Debug, Default)]
pub struct Continuous;

impl Clock for Continuous {
    fn now(&self) -> Option<Duration> {
        nix::time::clock_gettime(nix::time::ClockId::CLOCK_MONOTONIC)
            .ok()
            .map(Duration::from)
    }
}

/// A clock a test moves by hand, or makes fail. It starts at zero.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug)]
pub struct Manual(std::sync::Mutex<Option<Duration>>);

#[cfg(any(test, feature = "test-support"))]
impl Default for Manual {
    fn default() -> Self {
        Self(std::sync::Mutex::new(Some(Duration::ZERO)))
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Manual {
    fn reading(&self) -> std::sync::MutexGuard<'_, Option<Duration>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Move the clock on by `by`; a failing clock starts again from zero.
    pub fn advance(&self, by: Duration) {
        let mut now = self.reading();
        *now = Some(now.unwrap_or_default() + by);
    }

    /// Make the clock read `now`: earlier than before, say, or failing.
    pub fn set(&self, now: Option<Duration>) {
        *self.reading() = now;
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Clock for Manual {
    fn now(&self) -> Option<Duration> {
        *self.reading()
    }
}

#[cfg(test)]
#[path = "clock_tests.rs"]
mod tests;
