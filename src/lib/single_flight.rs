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
            let fresh = last.is_some_and(|(.., taken)| taken.elapsed() < self.fresh_for);
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
            let fresh = last.is_some_and(|(.., taken)| taken.elapsed() < self.fresh_for);
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
        receive
            .recv_timeout(wait)
            .ok()
            .or_else(|| self.trusted(known))
    }
}

impl<T, K> Shared<T, K> {
    fn lock(&self) -> MutexGuard<'_, Kept<T, K>> {
        self.kept.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// What a look found, unless it was taken more than twice as long ago
    /// as a result is reused for.
    fn trusted(&self, known: Option<(T, Instant)>) -> Option<T> {
        known
            .filter(|(_, taken)| taken.elapsed() < self.fresh_for * 2)
            .map(|(found, _)| found)
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
mod tests {
    use super::*;

    /// How long a look is reused in these tests.
    const TEST_REUSE: Duration = Duration::from_secs(1);

    fn shared<T: Clone + Send + 'static>() -> &'static Shared<T> {
        Box::leak(Box::new(Shared::new(TEST_REUSE)))
    }

    /// Make the last look `by` older than it is; so soon after the Mac
    /// started that no such time exists, wait that long instead.
    fn age<T>(shared: &Shared<T>, by: Duration) {
        let mut kept = shared.lock();
        let Some((.., taken)) = &mut kept.last else {
            return;
        };
        match taken.checked_sub(by) {
            Some(older) => *taken = older,
            None => {
                drop(kept);
                std::thread::sleep(by);
            }
        }
    }

    /// A look that counts how often it ran and gives that count.
    fn counting(
        runs: &std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) -> impl FnOnce() -> usize + Send + 'static {
        let runs = std::sync::Arc::clone(runs);
        move || runs.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
    }

    fn later() -> Instant {
        Instant::now() + Duration::from_secs(5)
    }

    #[tokio::test]
    async fn a_look_that_hangs_runs_once_and_no_status_read_waits_past_its_deadline() {
        use std::sync::{Arc, atomic::AtomicUsize, atomic::Ordering};

        let shared = shared::<usize>();
        let runs = Arc::new(AtomicUsize::new(0));
        let (release, gate) = std::sync::mpsc::channel::<()>();
        let gate = Arc::new(Mutex::new(gate));
        let look = || {
            let gate = Arc::clone(&gate);
            let count = counting(&runs);
            move || {
                let run = count();
                let _ = gate.lock().unwrap().recv();
                run
            }
        };

        for _ in 0..50 {
            let until = Instant::now() + Duration::from_millis(20);
            assert_eq!(shared.get(503, until, look()).await, None);
            assert!(
                Instant::now() < until + Duration::from_millis(500),
                "the read ended at its deadline"
            );
        }
        assert_eq!(runs.load(Ordering::SeqCst), 1, "only one look ran");

        drop(release);
        while shared.lock().running {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(
            shared.get(503, later(), look()).await,
            Some(1),
            "the look given up on kept what it found"
        );
        assert_eq!(runs.load(Ordering::SeqCst), 1);

        age(shared, TEST_REUSE + Duration::from_millis(100));
        assert_eq!(shared.get(503, later(), look()).await, Some(2));
        assert_eq!(runs.load(Ordering::SeqCst), 2, "an old look is taken again");
    }

    #[tokio::test]
    async fn a_fresh_look_is_reused_and_an_old_one_taken_again() {
        let shared = shared::<usize>();
        let runs = std::sync::Arc::default();

        assert_eq!(shared.get(503, later(), counting(&runs)).await, Some(1));
        age(shared, TEST_REUSE - Duration::from_millis(500));
        assert_eq!(
            shared.get(503, later(), counting(&runs)).await,
            Some(1),
            "a fresh look is reused"
        );

        age(shared, Duration::from_millis(600));
        assert_eq!(
            shared.get(503, later(), counting(&runs)).await,
            Some(2),
            "an old look is taken again"
        );
    }

    #[tokio::test]
    async fn a_look_is_never_reused_for_another_helper() {
        let shared = shared::<usize>();
        let runs = std::sync::Arc::default();

        assert_eq!(shared.get(503, later(), counting(&runs)).await, Some(1));
        assert_eq!(shared.get(504, later(), counting(&runs)).await, Some(2));
        assert_eq!(shared.get(503, later(), counting(&runs)).await, Some(3));
    }
    /// A look that waits until `gate` is released or dropped, then gives
    /// `found`.
    fn held(
        gate: &std::sync::Arc<Mutex<std::sync::mpsc::Receiver<()>>>,
        found: usize,
    ) -> impl FnOnce() -> usize + Send + 'static {
        let gate = std::sync::Arc::clone(gate);
        move || {
            let _ = gate.lock().unwrap().recv();
            found
        }
    }

    #[tokio::test]
    async fn a_look_somewhat_old_is_given_while_a_new_one_runs() {
        let shared = shared::<usize>();
        assert_eq!(shared.get(503, later(), || 1).await, Some(1));
        age(shared, TEST_REUSE + Duration::from_millis(100));

        let (release, gate) = std::sync::mpsc::channel::<()>();
        let gate = std::sync::Arc::new(Mutex::new(gate));
        let soon = Instant::now() + Duration::from_millis(20);
        assert_eq!(
            shared.get(503, soon, held(&gate, 2)).await,
            Some(1),
            "the caller whose look ran late gets the last result"
        );
        assert_eq!(
            shared.get(503, later(), || 3).await,
            Some(1),
            "another caller gets it while the look runs"
        );
        drop(release);
    }

    #[tokio::test]
    async fn a_look_too_old_to_trust_is_unknown_while_a_new_one_runs() {
        let shared = shared::<usize>();
        assert_eq!(shared.get(503, later(), || 1).await, Some(1));
        age(shared, TEST_REUSE * 2 + Duration::from_millis(100));

        let (release, gate) = std::sync::mpsc::channel::<()>();
        let gate = std::sync::Arc::new(Mutex::new(gate));
        let soon = Instant::now() + Duration::from_millis(20);
        assert_eq!(
            shared.get(503, soon, held(&gate, 2)).await,
            None,
            "the caller whose look ran late does not get the old result"
        );
        assert_eq!(
            shared.get(503, later(), || 3).await,
            None,
            "nor does another caller while the look runs"
        );
        drop(release);
    }
}
