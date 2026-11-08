//! Counts of one-shot pictures by where they came from. macOS's screen
//! recorder keeps part of every CoreGraphics capture it serves, so each one
//! is counted with why the stream could not give it.
use super::stream::Unserved;
use serde_json::{Value, json};
use std::sync::{
    LazyLock, Mutex,
    atomic::{AtomicU64, Ordering},
};

/// Where one picture came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Captured {
    /// The stream gave it, proven current by a sample after it was asked
    /// for.
    Stream,
    /// CoreGraphics took it, as the stream could not.
    CoreGraphics(Unserved),
}

#[derive(Default)]
pub(super) struct Counts {
    streamed: AtomicU64,
    core_graphics: AtomicU64,
    last: Mutex<Option<&'static str>>,
}

impl Counts {
    pub(super) fn captured(&self, captured: Captured) {
        let why = match captured {
            Captured::Stream => {
                self.streamed.fetch_add(1, Ordering::Relaxed);
                return;
            }
            Captured::CoreGraphics(why) => why.name(),
        };
        *self.last.lock().unwrap_or_else(|p| p.into_inner()) = Some(why);
        self.core_graphics.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn snapshot(&self) -> Value {
        let last = *self.last.lock().unwrap_or_else(|p| p.into_inner());
        json!({
            "streamed": self.streamed.load(Ordering::Relaxed),
            "coreGraphics": self.core_graphics.load(Ordering::Relaxed),
            "lastCoreGraphics": last.unwrap_or("none"),
        })
    }
}

/// This process's one-shot pictures.
pub(super) static COUNTS: LazyLock<Counts> = LazyLock::new(Counts::default);

#[cfg(test)]
mod tests;
