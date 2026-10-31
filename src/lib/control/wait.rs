//! Waiting for the controller lock: a controller given a wait tries again
//! about ten times a second, in its turn among the commands waiting, until
//! it takes control, the wait runs out, the person has control or a Stop
//! ends it, and its hook hears when the wait begins and ends.
use super::{
    Controller,
    busy::{busy_agent, busy_person},
    turns,
};
use crate::{
    error::{Error, Result},
    files::LockFile,
};
use fs2::FileExt;
use rand::Rng;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// How long to sleep before trying the lock again: about a tenth of a
/// second, spread by 25 ms either way so waiters do not try in step, and
/// never past the end of the wait.
pub(crate) fn poll(remaining: Duration) -> Duration {
    poll_with(remaining, rand::rng().random_range(75..=125))
}

/// [`poll`] with its spread chosen: `jitter_ms` milliseconds, or what is
/// left of the wait if that is less.
pub(super) fn poll_with(remaining: Duration, jitter_ms: u64) -> Duration {
    Duration::from_millis(jitter_ms).min(remaining)
}

/// Where a wait for another controller has got to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitEvent {
    /// The first try found the lock held, and the controller waits.
    Began,
    /// The wait is over: control was taken, or the answer is a refusal.
    Ended,
}

/// What [`Controller::acquire`] tells about its wait; it runs on the
/// waiting thread.
pub type WaitHook = Arc<dyn Fn(WaitEvent) + Send + Sync>;

/// A wait under way: its hook hears [`WaitEvent::Began`] when it is made
/// and [`WaitEvent::Ended`] when it is dropped.
struct WaitNotice<'a>(Option<&'a WaitHook>);

impl<'a> WaitNotice<'a> {
    fn begin(hook: Option<&'a WaitHook>) -> Self {
        if let Some(hook) = hook {
            hook(WaitEvent::Began);
        }
        Self(hook)
    }
}

impl Drop for WaitNotice<'_> {
    fn drop(&mut self) {
        if let Some(hook) = self.0 {
            hook(WaitEvent::Ended);
        }
    }
}

impl Controller {
    /// Lock `lock`, trying again for what is left of [`Controller::wait`]
    /// while another controller holds it. A controller given a wait lets
    /// those already waiting go first.
    pub(super) fn take(&self, lock: &LockFile) -> Result<()> {
        let started = Instant::now();
        let first = self.wait.is_zero() || !turns::queued(&self.waiting);
        if self.try_take(lock, started, first)? {
            return Ok(());
        }

        self.wait_for_lock(lock, started)
    }

    /// Try `lock` again about ten times a second, between telling the hook
    /// that the wait began and that it ended, whenever no command that began
    /// waiting earlier is still waiting. A signal's stop ends the wait
    /// within one poll.
    fn wait_for_lock(&self, lock: &LockFile, started: Instant) -> Result<()> {
        let _notice = WaitNotice::begin(self.on_wait.as_ref());
        let turn = turns::Turn::join(&self.waiting, self.label.as_ref());
        loop {
            std::thread::sleep(poll(self.remaining()));
            self.interrupt.check_waiting(started.elapsed())?;
            if self.try_take(lock, started, turn.first())? {
                return Ok(());
            }
        }
    }

    /// One try at `lock`, `started` being when the first was made, unless it
    /// is not this command's `turn`: true once it is held, false while
    /// another controller holds it (or goes first) and some of the wait is
    /// left, else why not. The person's note and a Stop are looked at on
    /// every try, so either ends a wait within one poll, and the person is
    /// never waited for. A refused lock is let go when the caller drops it.
    fn try_take(&self, lock: &LockFile, started: Instant, turn: bool) -> Result<bool> {
        let taken = turn && FileExt::try_lock_exclusive(&lock.0).is_ok();
        let waited = (!self.wait.is_zero()).then(|| started.elapsed());
        if !taken && self.person.exists() {
            return Err(busy_person(waited));
        }
        if self.heeds_stop && waited.is_some() && self.stop_noted() {
            return Err(Error::stopped());
        }
        if taken {
            return Ok(true);
        }
        if self.remaining().is_zero() {
            return Err(busy_agent(waited, self.wait));
        }

        Ok(false)
    }
}
