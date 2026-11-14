//! Following the menu bar's event feed: the status is read when the feed
//! says it may have changed, rather than on every look at the log. A thread
//! of its own waits on the feed and counts what it says; the watch reads
//! the status once the count moved past the one it last read for. While no
//! menu bar serves the feed, or it cannot be written, the watch looks at the
//! log as it always did, and the feed is asked again now and then.
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering::SeqCst},
    },
    time::Duration,
};

/// How long one wait on the feed may take: longer than the service waits.
const DEADLINE: Duration = Duration::from_secs(35);
/// How long a feed that could not be followed is left before it is asked
/// again.
const PROBE: Duration = Duration::from_secs(30);
/// How long a reader past the service's waits leaves it before asking
/// again.
const POLL: Duration = Duration::from_secs(2);
/// The largest answer read: a batch of events.
const LIMIT: usize = 1 << 17;
/// The kinds of event after which the status may say something new.
const READ_ON: [&str; 4] = ["status", "activity", "control", "sessions"];

/// What the feed has said, as the watch reads it.
#[derive(Default)]
pub(super) struct Following {
    /// How many times the feed said the status may have changed.
    told: AtomicU64,
    /// Whether the feed is followed now.
    live: AtomicBool,
}

impl Following {
    /// Whether the feed is followed now; while not, the log is looked at.
    pub(super) fn live(&self) -> bool {
        self.live.load(SeqCst)
    }

    /// How many times the feed said the status may have changed.
    pub(super) fn told(&self) -> u64 {
        self.told.load(SeqCst)
    }

    /// Take in one answer of the feed at the position `at`: the position to
    /// ask from next, and how long to leave it first.
    pub(super) fn take(
        &self,
        at: Option<(String, u64)>,
        answer: &Value,
    ) -> (Option<(String, u64)>, Duration) {
        if let Some(reset) = answer.get("reset") {
            // Everything is read again, from the position given.
            self.live.store(true, SeqCst);
            self.told.fetch_add(1, SeqCst);
            let position = reset["feed"]
                .as_str()
                .zip(reset["head"].as_u64())
                .map(|(feed, head)| (feed.to_owned(), head));
            return (position, Duration::ZERO);
        }
        // An answer that is neither is not the feed's.
        let Some(events) = answer["events"].as_array() else {
            self.lost();
            return (at, PROBE);
        };
        let read_on = events.iter().any(|event| {
            // A kind this build does not know may say anything.
            event["kind"]
                .as_str()
                .is_none_or(|kind| READ_ON.contains(&kind) || !KNOWN.contains(&kind))
        });
        if read_on {
            self.told.fetch_add(1, SeqCst);
        }
        let next = events
            .last()
            .and_then(|event| Some((event["feed"].as_str()?.to_owned(), event["seq"].as_u64()?)))
            .or(at);
        if answer["poll"] == true {
            self.live.store(false, SeqCst);
            return (next, POLL);
        }
        self.live.store(true, SeqCst);
        (next, Duration::ZERO)
    }

    /// The feed could not be followed: the log is looked at meanwhile.
    fn lost(&self) {
        self.live.store(false, SeqCst);
    }
}

/// Every kind this build knows.
const KNOWN: [&str; 6] = [
    "status", "activity", "control", "sessions", "asks", "desktops",
];

/// Follow the feed of the menu bar keeping `output` on `port`, in a thread
/// of its own for as long as the process runs.
pub(super) fn follow(output: PathBuf, port: u16) -> Arc<Following> {
    let following = Arc::new(Following::default());
    let follower = following.clone();
    std::thread::spawn(move || {
        let mut at: Option<(String, u64)> = None;
        loop {
            let pause = match crate::viewer::serving_events(&output, port) {
                Some(url) => {
                    let route = at.as_ref().map_or_else(
                        || "events.json".to_owned(),
                        |(feed, after)| format!("events.json?feed={feed}&after={after}"),
                    );
                    match crate::viewer::fetch(&url, &route, DEADLINE, LIMIT)
                        .ok()
                        .and_then(|body| serde_json::from_slice::<Value>(&body).ok())
                    {
                        Some(answer) => {
                            let (next, pause) = follower.take(at.take(), &answer);
                            at = next;
                            pause
                        }
                        None => {
                            follower.lost();
                            PROBE
                        }
                    }
                }
                None => {
                    follower.lost();
                    PROBE
                }
            };
            std::thread::sleep(pause);
        }
    });
    following
}

#[cfg(test)]
#[path = "follow_tests.rs"]
mod tests;
