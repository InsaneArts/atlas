//! Registered work, connection outcomes and the cleanup boundary.
use super::{Gate, Perform};
use atlas_core::error::{Code, Error, Result};
use std::{
    collections::HashMap,
    error::Error as StdError,
    fmt,
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::{
    sync::{Mutex, OwnedMutexGuard, oneshot, watch},
    task::{Id, JoinError, JoinSet},
};

/// Request futures can disappear while AX is in flight. Registered tasks
/// own the engine guard and always join their blocking child before leaving.
/// Engine work, which writes the journal and posts, runs on that blocking
/// child: slow storage never holds a runtime worker, so the listener and
/// cancellation stay responsive.
#[derive(Default)]
pub(super) struct Operations {
    tasks: StdMutex<Vec<Arc<Mutex<Work>>>>,
    pub(super) connections: StdMutex<HashMap<Id, (watch::Receiver<ConnectionPhase>, bool)>>,
    failed: AtomicBool,
    /// What first left outcomes unknown since the last recovery. Taken inside
    /// `connections`, never around it.
    cause: StdMutex<Option<Cause>>,
    recovery: Mutex<()>,
}

type Work = Option<tokio::task::JoinHandle<()>>;

/// Why recovery is pending: internal, never part of a reply.
pub(super) type Cause = Arc<dyn StdError + Send + Sync>;

impl Operations {
    pub(super) async fn start<T: Send + 'static>(
        &self,
        work: impl std::future::Future<Output = T> + Send + 'static,
    ) -> Result<oneshot::Receiver<T>> {
        self.reap().await?;
        self.register(work, true).map(|(receive, _)| receive)
    }

    /// Run `work` on the locked `engine` as registered cleanup and join it.
    /// Unlike [`Self::start`] it runs while outcomes are unknown, since
    /// releasing held input is part of settling them. If this future is
    /// dropped, the work still ends with the engine locked and stays
    /// registered until it is joined.
    pub(super) async fn clean_up<E: Send + 'static, T: Send + 'static>(
        &self,
        engine: OwnedMutexGuard<E>,
        work: impl FnOnce(&mut E) -> T + Send + 'static,
    ) -> Result<(OwnedMutexGuard<E>, T)> {
        self.reap().await?;
        let (outcome, work) = self.register(on_engine(engine, work), false)?;
        self.join(&mut *work.lock().await).await;
        self.remove_joined()?;
        outcome
            .await
            .map_err(|cause| worker_failed().with_source(cause))
    }

    /// [`Self::clean_up`] without waiting for it.
    pub(super) async fn begin_clean_up<E: Send + 'static>(
        &self,
        engine: OwnedMutexGuard<E>,
        work: impl FnOnce(&mut E) + Send + 'static,
    ) -> Result<()> {
        self.reap().await?;
        self.register(on_engine(engine, work), false).map(drop)
    }

    /// Register `work`, then let it start; when `checked`, only while no
    /// earlier outcome is unknown.
    fn register<T: Send + 'static>(
        &self,
        work: impl std::future::Future<Output = T> + Send + 'static,
        checked: bool,
    ) -> Result<(oneshot::Receiver<T>, Arc<Mutex<Work>>)> {
        let mut tasks = self.tasks.lock().map_err(|_| worker_failed())?;
        if checked {
            self.check()?;
        }
        let (send, receive) = oneshot::channel();
        let (start, registered) = oneshot::channel();
        let task = tokio::spawn(async move {
            if registered.await.is_err() {
                return;
            }
            let result = work.await;
            // A gone requester cannot acknowledge the work; it stays registered.
            let _ = send.send(result);
        });
        let work = Arc::new(Mutex::new(Some(task)));
        tasks.push(work.clone());
        drop(tasks);
        // AX cannot start before registration. No await separates this
        // handoff from registration, even if the request is aborted.
        // A lost worker is observed when its registered handle is joined.
        let _ = start.send(());
        Ok((receive, work))
    }

    /// Leave outcomes unknown until recovery, keeping what caused that.
    fn fail(&self, cause: impl StdError + Send + Sync + 'static) -> Cause {
        let cause: Cause = Arc::new(cause);
        self.failed.store(true, Ordering::Relaxed);
        // A poisoned slot loses only the diagnosis; the failure stands.
        if let Ok(mut first) = self.cause.lock()
            && first.is_none()
        {
            *first = Some(cause.clone());
        }
        cause
    }

    async fn join(&self, work: &mut Work) {
        // Await by reference: dropping cleanup leaves the handle registered.
        if let Some(task) = &mut *work {
            let outcome = task.await;
            *work = None;
            if let Err(cause) = outcome {
                self.fail(cause);
            }
        }
    }

    fn remove_joined(&self) -> Result<bool> {
        let mut tasks = self.tasks.lock().map_err(|_| worker_failed())?;
        tasks.retain(|task| task.try_lock().map_or(true, |work| work.is_some()));
        Ok(tasks.is_empty())
    }

    pub(super) async fn reap(&self) -> Result<()> {
        let tasks = self.tasks.lock().map_err(|_| worker_failed())?.clone();
        for task in tasks {
            if let Ok(mut work) = task.try_lock()
                && work.as_ref().is_some_and(|task| task.is_finished())
            {
                self.join(&mut work).await;
            }
        }
        self.remove_joined()?;
        Ok(())
    }

    pub(super) fn check(&self) -> Result<()> {
        if self.failed.load(Ordering::Relaxed) {
            let cause = self.cause.lock().map_err(|_| worker_failed())?.clone();
            Err(cause.map_or_else(worker_failed, |cause| worker_failed().with_source(cause)))
        } else {
            Ok(())
        }
    }

    pub(super) async fn join_all(&self) -> Result<()> {
        loop {
            let tasks = self.tasks.lock().map_err(|_| worker_failed())?.clone();
            for task in tasks {
                let mut work = task.lock().await;
                self.join(&mut work).await;
            }
            if self.remove_joined()? {
                return Ok(());
            }
        }
    }

    pub(super) async fn drain(&self) -> Result<()> {
        self.join_all().await?;
        self.check()
    }
}

#[cfg(test)]
impl Operations {
    pub(super) fn registered(&self) -> usize {
        self.tasks.lock().unwrap().len()
    }

    pub(super) fn has_failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }

    pub(super) fn set_failed(&self, failed: bool) {
        self.failed.store(failed, Ordering::Relaxed);
    }

    pub(super) async fn hold_recovery(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.recovery.lock().await
    }

    pub(super) fn recovering(&self) -> bool {
        self.recovery.try_lock().is_err()
    }
}

/// `work` on the locked `engine`, on a blocking thread, giving the engine
/// back with its result. A panic there resumes in the registered task, whose
/// join leaves outcomes unknown until recovery.
pub(super) async fn on_engine<E: Send + 'static, T: Send + 'static>(
    mut engine: OwnedMutexGuard<E>,
    work: impl FnOnce(&mut E) -> T + Send + 'static,
) -> (OwnedMutexGuard<E>, T) {
    let worker = tokio::task::spawn_blocking(move || {
        let done = work(&mut engine);
        (engine, done)
    });
    match worker.await {
        Ok(done) => done,
        Err(cause) => match cause.try_into_panic() {
            Ok(panic) => std::panic::resume_unwind(panic),
            Err(cause) => std::panic::panic_any(cause),
        },
    }
}

pub(super) fn worker_failed() -> Error {
    Error::new(
        Code::Uncertain,
        "The input worker did not report its outcome",
    )
    .uncertain()
}

pub(super) struct CleanupFailures {
    pub(super) primary: Error,
    pub(super) cleanup: Error,
}

impl fmt::Debug for CleanupFailures {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CleanupFailures")
            .field("primary", &self.primary.code)
            .field("cleanup", &self.cleanup.code)
            .field(
                "uncertain",
                &(self.primary.uncertain || self.cleanup.uncertain),
            )
            .finish()
    }
}

impl fmt::Display for CleanupFailures {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Native work and cleanup failed ({})", self.cleanup.code)
    }
}

impl StdError for CleanupFailures {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.primary)
    }
}

pub(super) fn combine_cleanup(primary: Result<()>, cleanup: Result<()>) -> Result<()> {
    match (primary, cleanup) {
        (Ok(()), outcome) | (outcome, Ok(())) => outcome,
        (Err(primary), Err(cleanup)) => {
            let uncertain = primary.uncertain || cleanup.uncertain;
            let mut error = Error::new(
                primary.code,
                format!(
                    "{}; cleanup: {} (recovery pending: {uncertain})",
                    primary.error, cleanup.code,
                ),
            );
            error.uncertain = uncertain;
            error.holder = primary.holder;
            error.details = primary.details.clone();
            Err(error.with_source(CleanupFailures { primary, cleanup }))
        }
    }
}

/// Cancelling a request changes ownership without waiting for the engine.
pub(super) struct Cancellation {
    pub(super) gate: Arc<Gate>,
    pub(super) owner: Option<String>,
}

impl Drop for Cancellation {
    fn drop(&mut self) {
        if let Some(owner) = &self.owner {
            // Drop can only try to cancel ownership. It cannot acknowledge
            // release, clear sticky worker failure or settle the controller's
            // pending record, even if cancellation succeeds.
            let _ = self.gate.cancel(Some(owner));
        }
    }
}

pub(super) async fn drain_and_release<E: Perform + Send + 'static>(
    operations: &Operations,
    engine: &Arc<Mutex<E>>,
    gate: &Gate,
) -> Result<()> {
    let drained = operations.drain().await;
    let engine = engine.clone().lock_owned().await;
    let released = match gate.active() {
        Ok(false) => operations
            .clean_up(engine, |engine| engine.release())
            .await
            .and_then(|(_, released)| released),
        Ok(true) => Ok(()),
        Err(error) => Err(error),
    };
    combine_cleanup(drained, released)
}

/// Only explicit recovery can settle an unknown worker outcome. If recovery
/// is cancelled or any step fails, input stays blocked until another reset.
pub(super) async fn reset<E: Send + 'static>(
    operations: &Operations,
    engine: &Arc<Mutex<E>>,
    gate: &Gate,
    rebind: impl FnOnce(&mut E) -> Result<()> + Send + 'static,
) -> Result<()> {
    let _recovery = operations.recovery.lock().await;
    operations.failed.store(true, Ordering::Relaxed);
    gate.cancel(None)?;
    operations.join_all().await?;
    let engine = engine.clone().lock_owned().await;
    // An acquire admitted just before recovery may have held the engine
    // while the first cancellation ran. Cancel it before cleanup as well.
    gate.cancel(None)?;
    let (_engine, rebound) = operations.clean_up(engine, rebind).await?;
    rebound?;
    let mut connections = operations.connections.lock().map_err(|_| worker_failed())?;
    for (phase, recovered) in connections.values_mut() {
        // A closed sender has finished dispatch and reply delivery. Its
        // unobserved outcome belongs to the work this reset just settled.
        *recovered |= phase.has_changed().is_err();
    }
    *operations.cause.lock().map_err(|_| worker_failed())? = None;
    operations.failed.store(false, Ordering::Relaxed);
    Ok(())
}

/// Every service exit uses the same close, request cancellation and worker
/// drain boundary, including TERM and internal failures.
pub(super) async fn finish<E: Perform + Send + 'static>(
    gate: &Gate,
    requests: &mut JoinSet<Result<()>>,
    operations: &Operations,
    engine: &Arc<Mutex<E>>,
) -> Result<()> {
    let closed = gate.close();
    let stopping = closed.is_ok();
    requests.abort_all();
    let mut connections = Ok(());
    while let Some(outcome) = requests.join_next_with_id().await {
        connections = combine_cleanup(
            connections,
            connection_outcome(outcome, operations, stopping),
        );
    }
    let cleanup = drain_and_release(operations, engine, gate).await;
    combine_cleanup(closed, combine_cleanup(connections, cleanup))
}

#[derive(Clone, Copy, Debug)]
pub(super) enum ConnectionPhase {
    Refusal,
    Read,
    Mutation(&'static str),
}

pub(super) fn connection_outcome(
    outcome: std::result::Result<(Id, Result<()>), JoinError>,
    operations: &Operations,
    stopping: bool,
) -> Result<()> {
    let id = match &outcome {
        Ok((id, _)) => *id,
        Err(error) => error.id(),
    };
    let mut connections = operations.connections.lock().map_err(|_| worker_failed())?;
    let (phase, recovered) = connections
        .remove(&id)
        .map(|(phase, recovered)| (*phase.borrow(), recovered))
        .unwrap_or((ConnectionPhase::Mutation("connection"), false));
    let (operation, effects) = match phase {
        ConnectionPhase::Refusal => ("refusal", false),
        ConnectionPhase::Read => ("hello", false),
        ConnectionPhase::Mutation(operation) => (operation, true),
    };
    let unknown = || {
        Error::new(
            Code::Uncertain,
            format!("Native {operation} connection outcome is unknown; recovery pending"),
        )
        .uncertain()
    };
    match outcome {
        Ok((_, Ok(()))) => Ok(()),
        // Refusals have no effects, and a read client need not receive its
        // reply. Neither transport failure degrades the input engine.
        Ok((_, Err(_))) if !effects => Ok(()),
        Err(error) if stopping && error.is_cancelled() => Ok(()),
        // Reset already joined the work and rebound the engine after this
        // connection finished. Observing it cannot undo that recovery.
        _ if recovered => Ok(()),
        Ok((_, Err(cause))) => Err(unknown().with_source(operations.fail(cause))),
        Err(cause) => Err(unknown().with_source(operations.fail(cause))),
    }
}
