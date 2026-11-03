//! A request that may stall the runtime it runs on, watched from outside
//! that runtime, so a test can tell a stall and still end it.
use std::{
    future::Future,
    sync::mpsc::{Receiver, Sender, channel},
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};

/// How long a test waits for what should happen at once before it takes
/// the runtime for stalled.
pub const WATCHDOG: Duration = Duration::from_secs(5);

/// An operation a request started on a runtime of its own thread, beside
/// a heartbeat that answers whenever that runtime is free, until the
/// operation ends or the request is dropped.
pub struct Requester<T> {
    ping: mpsc::UnboundedSender<Sender<()>>,
    stop: Option<oneshot::Sender<()>>,
    ended: Receiver<Option<T>>,
}

impl<T: Send + 'static> Requester<T> {
    /// Start the future `operation` makes, on the runtime of a new thread.
    pub fn start<F: Future<Output = T>>(operation: impl FnOnce() -> F + Send + 'static) -> Self {
        let (ping, mut pings) = mpsc::unbounded_channel::<Sender<()>>();
        let (stop, stopped) = oneshot::channel::<()>();
        let (end, ended) = channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                tokio::spawn(async move {
                    while let Some(answer) = pings.recv().await {
                        let _ = answer.send(());
                    }
                });
                let outcome = tokio::select! {
                    outcome = operation() => Some(outcome),
                    _ = stopped => None,
                };
                let _ = end.send(outcome);
            });
        });
        Self {
            ping,
            stop: Some(stop),
            ended,
        }
    }

    /// Whether the runtime answered the heartbeat in time.
    pub fn responsive(&self) -> bool {
        let (answer, answered) = channel();
        self.ping.send(answer).is_ok() && answered.recv_timeout(WATCHDOG).is_ok()
    }

    /// Drop the request's future, as a requester that went away does.
    pub fn abandon(mut self) {
        let _ = self.stop.take().unwrap().send(());
        assert!(
            matches!(self.ended.recv_timeout(WATCHDOG), Ok(None)),
            "the request was not dropped"
        );
    }

    /// What the operation came to.
    pub fn outcome(self) -> T {
        self.ended.recv_timeout(WATCHDOG).unwrap().unwrap()
    }
}
