//! The event feed as the service serves it: the log its producer writes,
//! and the readers waiting on it. `events.json?feed=F&after=N` answers the
//! events after N, a reset when N does not follow on, or, with nothing new,
//! waits a while for something. Its waits have a small pool of their own,
//! outside the requests every other route shares, and each is owned by its
//! connection, so a reader that goes away frees its place.
//!
//! The producer writes off the runtime, holding the log while it syncs;
//! readers wait for the log without holding a worker.
use super::events::{EventLog, Kind, Since};
use atlas_core::error::Result;
use serde_json::{Value, json};
use std::{path::Path, time::Duration};
use tokio::sync::{Semaphore, watch};

/// The longest a reader waits for an event.
const WAIT: Duration = Duration::from_secs(25);
/// How many readers wait at once.
pub(super) const WAITS: usize = 8;

/// Where the feed stands, as readers waiting on it are told.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Position {
    /// Its identity and newest event.
    At(String, u64),
    /// It cannot be written now.
    Unavailable,
    /// The service is ending.
    Closed,
}

/// The feed: written by the producer alone, read by any reader.
pub(super) struct Feed {
    log: tokio::sync::Mutex<Option<EventLog>>,
    position: watch::Sender<Position>,
    waits: Semaphore,
}

impl Default for Feed {
    fn default() -> Self {
        Self {
            log: tokio::sync::Mutex::new(None),
            position: watch::Sender::new(Position::Unavailable),
            waits: Semaphore::new(WAITS),
        }
    }
}

impl Feed {
    /// Keep the feed in `folder`: the one there when it goes on, else a new
    /// one. Readers waiting are told where it stands. It blocks: the
    /// producer calls it off the runtime, as it does [`Feed::begin`] and
    /// [`Feed::append`].
    ///
    /// # Errors
    ///
    /// Returns the failure to open it; the feed is unavailable then.
    pub(super) fn open(&self, folder: &Path) -> Result<()> {
        self.kept(EventLog::open(folder))
    }

    /// Begin a new feed in `folder`, replacing what is there.
    ///
    /// # Errors
    ///
    /// Returns the failure to begin it; the feed stays unavailable then.
    pub(super) fn begin(&self, folder: &Path) -> Result<()> {
        self.kept(EventLog::begin(folder))
    }

    /// Write that `kind` of `desktop` changed. A failure makes the feed
    /// unavailable, and every waiting reader is told at once.
    ///
    /// # Errors
    ///
    /// Returns the failure to write it, or that the feed is unavailable.
    pub(super) fn append(&self, desktop: &str, kind: Kind) -> Result<()> {
        let mut log = self.log.blocking_lock();
        let Some(open) = log.as_mut().filter(|_| !self.closed()) else {
            return Err(unavailable());
        };
        match open.append(desktop, kind) {
            Ok(event) => {
                self.tell(Position::At(event.feed, event.seq));
                Ok(())
            }
            Err(error) => {
                *log = None;
                self.tell(Position::Unavailable);
                Err(error)
            }
        }
    }

    /// The feed's identity and newest event, while it is available.
    #[cfg(test)]
    pub(super) fn head(&self) -> Option<(String, u64)> {
        match self.position.borrow().clone() {
            Position::At(feed, head) => Some((feed, head)),
            Position::Unavailable | Position::Closed => None,
        }
    }

    /// How many readers may still wait.
    #[cfg(test)]
    pub(super) fn free_waits(&self) -> usize {
        self.waits.available_permits()
    }

    /// Every wait, taken until the result is dropped.
    #[cfg(test)]
    pub(super) fn hold_every_wait(&self) -> tokio::sync::SemaphorePermit<'_> {
        self.waits
            .try_acquire_many(u32::try_from(WAITS).unwrap_or(u32::MAX))
            .expect("no wait is taken")
    }

    /// Whether the feed can be read and written now.
    pub(super) fn available(&self) -> bool {
        matches!(*self.position.borrow(), Position::At(..))
    }

    /// The service is ending: every waiting reader is answered now, and
    /// the feed is never written or begun again.
    pub(super) fn close(&self) {
        self.position.send_replace(Position::Closed);
    }

    /// Whether the service is ending.
    fn closed(&self) -> bool {
        *self.position.borrow() == Position::Closed
    }

    /// The answer for a reader who last saw `after` in feed `feed`: the
    /// events after it, or with none yet, the first to come within
    /// [`WAIT`]; a reset when its position does not follow on; told to poll
    /// when every wait is taken.
    pub(super) async fn answer(&self, feed: Option<&str>, after: u64) -> (u16, Value) {
        let mut changes = self.position.subscribe();
        let first = self.since(feed, after).await;
        if !nothing_new(first.as_ref()) {
            return reply(first);
        }
        let Ok(_waiting) = self.waits.try_acquire() else {
            let mut answer = reply(first);
            answer.1["poll"] = json!(true);
            return answer;
        };
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            if tokio::time::timeout_at(deadline, changes.changed())
                .await
                .is_err()
            {
                return reply(self.since(feed, after).await);
            }
            let now = self.since(feed, after).await;
            if !nothing_new(now.as_ref()) {
                return reply(now);
            }
        }
    }

    /// What the log says after `after`; none while it is unavailable.
    async fn since(&self, feed: Option<&str>, after: u64) -> Option<Since> {
        if self.closed() {
            return None;
        }
        let log = self.log.lock().await;
        let log = log.as_ref()?;
        Some(feed.map_or_else(
            || {
                let (feed, head) = log.position();
                Since::Reset {
                    feed: feed.to_owned(),
                    head,
                }
            },
            |feed| log.since(feed, after),
        ))
    }

    /// Keep the log `opened`, or none when it failed, and tell readers
    /// where the feed stands.
    fn kept(&self, opened: Result<EventLog>) -> Result<()> {
        let mut kept = self.log.blocking_lock();
        if self.closed() {
            return Err(unavailable());
        }
        let (log, outcome) = match opened {
            Ok(log) => (Some(log), Ok(())),
            Err(error) => (None, Err(error)),
        };
        let position = log.as_ref().map_or(Position::Unavailable, |log| {
            let (feed, head) = log.position();
            Position::At(feed.to_owned(), head)
        });
        *kept = log;
        self.tell(position);
        outcome
    }

    /// Tell waiting readers the feed stands at `position`, unless the
    /// service ended meanwhile: a write that ends after it closed opens
    /// nothing again.
    fn tell(&self, position: Position) {
        self.position.send_if_modified(|now| {
            if *now == Position::Closed {
                return false;
            }
            *now = position;
            true
        });
    }
}

/// Whether `since` is a following position with no event after it yet.
fn nothing_new(since: Option<&Since>) -> bool {
    matches!(since, Some(Since::Events { events, .. }) if events.is_empty())
}

/// The reply to a reader: its status code and body.
fn reply(since: Option<Since>) -> (u16, Value) {
    match since {
        None => (503, json!({"ok": false, "unavailable": true})),
        Some(Since::Reset { feed, head }) => (
            200,
            json!({"ok": true, "reset": {"feed": feed, "head": head}}),
        ),
        Some(Since::Events { feed, events, head }) => (
            200,
            json!({"ok": true, "feed": feed, "head": head, "events": events}),
        ),
    }
}

/// The feed cannot be written now.
fn unavailable() -> atlas_core::error::Error {
    atlas_core::error::Error::new(
        atlas_core::error::Code::Io,
        "Atlas cannot keep what changed now",
    )
}

#[cfg(test)]
#[path = "feed_tests.rs"]
mod tests;
