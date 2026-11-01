use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
    mpsc,
};

const TEST_REUSE: Duration = Duration::from_secs(1);

fn shared<T: Clone + Send + 'static>() -> &'static Shared<T> {
    Box::leak(Box::new(Shared::new(TEST_REUSE)))
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .unwrap()
}

struct Worker {
    release: Option<mpsc::Sender<()>>,
    started: mpsc::Receiver<()>,
    completed: Option<mpsc::Receiver<()>>,
}

impl Worker {
    fn held(found: usize) -> (Self, impl FnOnce() -> usize + Send + 'static) {
        let (release, gate) = mpsc::channel();
        let (started, began) = mpsc::channel();
        let (completed, ended) = mpsc::channel();
        (
            Self {
                release: Some(release),
                started: began,
                completed: Some(ended),
            },
            move || {
                started.send(()).unwrap();
                let _ = gate.recv();
                completed.send(()).unwrap();
                found
            },
        )
    }

    fn started(&self) {
        self.started.recv_timeout(Duration::from_secs(5)).unwrap();
    }

    fn finish(&mut self) {
        drop(self.release.take());
        if let Some(completed) = self.completed.take() {
            completed.recv_timeout(Duration::from_secs(5)).unwrap();
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.finish();
    }
}

#[test]
fn a_fresh_result_expires_at_its_exact_boundary() {
    let shared = shared::<usize>();
    let taken = Instant::now();
    assert!(!shared.fresh_at(None, taken));
    assert!(shared.fresh_at(Some(taken), taken + TEST_REUSE - Duration::from_nanos(1)));
    assert!(!shared.fresh_at(Some(taken), taken + TEST_REUSE));
    assert_eq!(
        shared.trusted_at(Some((7, taken)), taken + TEST_REUSE),
        Some(7)
    );
    assert_eq!(
        shared.trusted_at(
            Some((7, taken)),
            taken + TEST_REUSE * 2 - Duration::from_nanos(1)
        ),
        Some(7)
    );
    assert_eq!(
        shared.trusted_at(Some((7, taken)), taken + TEST_REUSE * 2),
        None
    );
}

#[test]
fn a_fresh_look_is_reused_and_an_old_one_taken_again() {
    let shared = shared::<usize>();
    let runtime = runtime();
    runtime.block_on(async {
        let until = Instant::now() + Duration::from_secs(5);
        assert_eq!(shared.get(503, until, || 1).await, Some(1));
        assert_eq!(shared.get(503, until, || 2).await, Some(1));
        tokio::time::advance(TEST_REUSE).await;
        assert_eq!(shared.get(503, until + TEST_REUSE, || 2).await, Some(2));
    });
}

#[test]
fn a_look_is_never_reused_for_another_helper() {
    let shared = shared::<usize>();
    runtime().block_on(async {
        let until = Instant::now() + Duration::from_secs(5);
        assert_eq!(shared.get(503, until, || 1).await, Some(1));
        assert_eq!(shared.get(504, until, || 2).await, Some(2));
        assert_eq!(shared.get(503, until, || 3).await, Some(3));
    });
}

#[test]
fn an_abandoned_caller_leaves_only_one_worker() {
    let shared = shared::<usize>();
    let runtime = runtime();
    let (mut worker, look) = Worker::held(7);
    runtime.block_on(async {
        let caller = tokio::spawn(shared.get(503, Instant::now() + TEST_REUSE, look));
        tokio::task::yield_now().await;
        worker.started();
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        for key in [503, 504].into_iter().cycle().take(50) {
            assert_eq!(
                shared
                    .get(key, Instant::now(), || panic!("second worker"))
                    .await,
                None
            );
        }
        assert!(shared.lock().running);
    });
    worker.finish();
    drop(runtime); // Runtime shutdown joins the blocking look before inspecting completion.
    assert!(!shared.lock().running);
    assert_eq!(
        shared.lock().last.as_ref().map(|(_, found, _)| *found),
        Some(7)
    );
}

#[test]
fn a_deadline_leaves_the_worker_running_and_stamps_its_completion() {
    let shared = shared::<usize>();
    let runtime = runtime();
    let (mut worker, look) = Worker::held(7);
    let completed_at = runtime.block_on(async {
        let caller = tokio::spawn(shared.get(503, Instant::now() + TEST_REUSE, look));
        tokio::task::yield_now().await;
        worker.started();
        tokio::time::advance(TEST_REUSE).await;
        assert_eq!(caller.await.unwrap(), None);
        assert!(shared.lock().running);
        tokio::time::advance(TEST_REUSE * 10).await;
        Instant::now()
    });
    worker.finish();
    drop(runtime);
    let kept = shared.lock();
    assert!(!kept.running);
    assert!(kept.last.as_ref().unwrap().2 >= completed_at);
}

#[test]
fn stale_fallback_is_trusted_only_before_its_exact_boundary() {
    for age in [TEST_REUSE, TEST_REUSE * 2] {
        let shared = shared::<usize>();
        let runtime = runtime();
        let (mut worker, look) = Worker::held(2);
        runtime.block_on(async {
            shared.lock().last = Some((503, 1, Instant::now()));
            tokio::time::advance(age).await;
            let caller =
                tokio::spawn(shared.get(503, Instant::now() + Duration::from_millis(10), look));
            tokio::task::yield_now().await;
            worker.started();
            tokio::time::advance(Duration::from_millis(10)).await;
            let expected = (age == TEST_REUSE).then_some(1);
            assert_eq!(caller.await.unwrap(), expected);
            assert_eq!(
                shared
                    .get(503, Instant::now(), || panic!("second worker"))
                    .await,
                expected
            );
            assert_eq!(
                shared
                    .get(504, Instant::now(), || panic!("wrong key worker"))
                    .await,
                None
            );
        });
        worker.finish();
        drop(runtime);
        assert_eq!(shared.lock().last.as_ref().unwrap().1, 2);
    }
}

#[test]
fn a_panicked_look_releases_running_ownership() {
    let shared = shared::<usize>();
    runtime().block_on(async {
        assert_eq!(
            shared
                .get(503, Instant::now() + TEST_REUSE, || panic!("failed look"))
                .await,
            None
        );
        assert!(!shared.lock().running);
        assert_eq!(
            shared.get(503, Instant::now() + TEST_REUSE, || 2).await,
            Some(2)
        );
    });
}

#[test]
fn a_blocking_timeout_retains_one_worker_until_completion() {
    let shared = shared::<usize>();
    let (mut worker, look) = Worker::held(7);
    assert_eq!(shared.get_blocking(503, Duration::ZERO, look), None);
    worker.started();
    for key in [503, 504].into_iter().cycle().take(50) {
        assert_eq!(
            shared.get_blocking(key, Duration::ZERO, || panic!("second worker")),
            None
        );
    }
    worker.finish();
    shared.join_workers();
    assert!(!shared.lock().running);
    assert_eq!(
        shared.get_blocking(503, Duration::ZERO, || panic!("fresh result")),
        Some(7)
    );
}

#[test]
fn blocking_looks_reuse_only_the_matching_key() {
    let shared = shared::<usize>();
    let calls = Arc::new(AtomicUsize::new(0));
    for (key, expected) in [(503, 1), (503, 1), (504, 2), (503, 3)] {
        let calls = calls.clone();
        assert_eq!(
            shared.get_blocking(key, Duration::from_secs(5), move || calls
                .fetch_add(1, Ordering::SeqCst)
                + 1),
            Some(expected)
        );
        shared.join_workers();
    }
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}
