//! One look at something slow to ask, such as directory services or the
//! process list, shared by every caller in the process: at most one runs at
//! a time, and what it found is reused for a while. A look that hangs keeps
//! one thread at most.
use std::{
    sync::{Mutex, MutexGuard, PoisonError},
    time::Duration,
};
use tokio::time::Instant;

/// One slow look shared by callers, keyed by its complete inputs. At most
/// one runs at a time; its result is reused for `fresh_for`.
pub(crate) struct Shared<T, K = u32> {
    pub(crate) fresh_for: Duration,
    kept: Mutex<Kept<T, K>>,
    #[cfg(test)]
    workers: Mutex<Vec<std::thread::JoinHandle<()>>>,
}

struct Kept<T, K> {
    /// The last look's key, result and completion time.
    last: Option<(K, T, Instant)>,
    /// Whether a look runs now.
    running: bool,
}

impl<T: Clone + Send + 'static, K: Eq + Clone + Send + 'static> Shared<T, K> {
    pub(crate) const fn new(fresh_for: Duration) -> Self {
        Self {
            fresh_for,
            kept: Mutex::new(Kept {
                last: None,
                running: false,
            }),
            #[cfg(test)]
            workers: Mutex::new(Vec::new()),
        }
    }

    /// What `look` finds for helper `uid`. While the last look's result is
    /// fresh, or another look runs, that result comes at once; otherwise a
    /// new look runs off the runtime and is waited for until `until`. A look
    /// given up on still ends and keeps what it found. `None` when nothing
    /// is known for `uid` in time, or only a result too old to trust.
    pub(crate) async fn get(
        &'static self,
        uid: K,
        until: Instant,
        look: impl FnOnce() -> T + Send + 'static,
    ) -> Option<T> {
        let known = {
            let mut kept = self.lock();
            let last = kept.last.as_ref().filter(|(key, ..)| *key == uid);
            let fresh = self.fresh_at(last.map(|(.., taken)| *taken), Instant::now());
            let known = last.map(|(_, found, taken)| (found.clone(), *taken));
            if fresh || kept.running {
                return self.trusted(known);
            }
            kept.running = true;
            known
        };

        // The look ends as not running however it ends, even never started.
        let ending = Ending(self);
        let looking = tokio::task::spawn_blocking(move || {
            let found = look();
            ending.0.lock().last = Some((uid, found.clone(), Instant::now()));
            drop(ending);
            found
        });

        match tokio::time::timeout_at(until, looking).await {
            Ok(Ok(found)) => Some(found),
            _ => self.trusted(known),
        }
    }
    /// One bounded synchronous look, sharing running ownership with `get`.
    pub(crate) fn get_blocking(
        &'static self,
        key: K,
        wait: Duration,
        look: impl FnOnce() -> T + Send + 'static,
    ) -> Option<T> {
        let known = {
            let mut kept = self.lock();
            let last = kept.last.as_ref().filter(|(old, ..)| *old == key);
            let fresh = self.fresh_at(last.map(|(.., taken)| *taken), Instant::now());
            let known = last.map(|(_, found, taken)| (found.clone(), *taken));
            if fresh || kept.running {
                return self.trusted(known);
            }
            kept.running = true;
            known
        };
        let ending = Ending(self);
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("session-liveness".into())
            .spawn(move || {
                let found = look();
                ending.0.lock().last = Some((key, found.clone(), Instant::now()));
                drop(ending);
                let _ = send.send(found);
            });
        if spawned.is_err() {
            return self.trusted(known);
        }
        #[cfg(test)]
        self.workers.lock().unwrap().push(spawned.unwrap());
        receive
            .recv_timeout(wait)
            .ok()
            .or_else(|| self.trusted(known))
    }
}

impl<T, K> Shared<T, K> {
    fn fresh_at(&self, taken: Option<Instant>, now: Instant) -> bool {
        taken.is_some_and(|taken| now.duration_since(taken) < self.fresh_for)
    }

    fn lock(&self) -> MutexGuard<'_, Kept<T, K>> {
        self.kept.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// What a look found, unless it was taken more than twice as long ago
    /// as a result is reused for.
    fn trusted(&self, known: Option<(T, Instant)>) -> Option<T> {
        self.trusted_at(known, Instant::now())
    }

    fn trusted_at(&self, known: Option<(T, Instant)>, now: Instant) -> Option<T> {
        known
            .filter(|(_, taken)| now.duration_since(*taken) < self.fresh_for * 2)
            .map(|(found, _)| found)
    }

    #[cfg(test)]
    pub(crate) fn join_workers(&self) {
        for worker in self.workers.lock().unwrap().drain(..) {
            worker.join().unwrap();
        }
    }
}

/// Marks a look as not running when dropped.
struct Ending<T: 'static, K: 'static>(&'static Shared<T, K>);

impl<T, K> Drop for Ending<T, K> {
    fn drop(&mut self) {
        self.0.lock().running = false;
    }
}

#[cfg(test)]
mod tests;
