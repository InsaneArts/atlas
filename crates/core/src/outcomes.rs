//! What a helper service remembers of the changes it was asked for: each
//! one that named itself, while it is carried out and, once it ends, the
//! reply it came to. A reply lost on its way is then not an outcome nobody
//! knows: whoever asked may ask what became of the change. Only the most
//! recent are kept, in memory, so a service that restarted remembers none.
use crate::{files::Token, transport::final_row};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

/// The capability to remember what changes came to, as a service's hello
/// lists it.
pub const OUTCOMES: &str = "outcomes-v1";

/// The most ended changes a service remembers; the oldest is forgotten
/// first.
const KEPT: usize = 256;

/// The longest reply remembered, as JSON text. A change that ends with a
/// longer one is forgotten.
const REPLY_MAX: usize = 64 * 1024;

/// What a service knows of one change.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "lowercase")]
pub enum Outcome {
    /// Never asked of this service since it started, or forgotten since.
    Unknown,
    /// Asked, and still being carried out.
    Running,
    /// Ended, with this final reply.
    Done { reply: Value },
}

impl Outcome {
    /// The reply to a request for this outcome.
    pub fn answer(&self) -> Value {
        let mut answer = json!(self);
        answer["ok"] = json!(true);
        answer
    }

    /// The final reply a service's `answer` to an outcome request holds:
    /// only that of a change that ended, shaped as a final reply is.
    /// Anything else leaves what the change came to unknown.
    pub fn remembered(answer: &Value) -> Option<Value> {
        if answer["ok"] != true {
            return None;
        }
        match Self::deserialize(answer).ok()? {
            Self::Done { reply }
                if reply
                    .as_object()
                    .is_some_and(|row| matches!(final_row(row), Ok(true))) =>
            {
                Some(reply)
            }
            Self::Done { .. } | Self::Running | Self::Unknown => None,
        }
    }
}

/// The changes one service remembers.
#[derive(Default)]
pub struct Outcomes {
    changes: Mutex<Changes>,
}

#[derive(Default)]
struct Changes {
    /// Every remembered change, with no reply while it is carried out.
    known: HashMap<Token, Option<Value>>,
    /// Those that ended, oldest first.
    ended: VecDeque<Token>,
}

impl Outcomes {
    /// Start remembering `change`. None when it is remembered already,
    /// under way or ended: a change asked for twice is carried out once.
    pub fn begin(self: &Arc<Self>, change: Token) -> Option<Remembering> {
        let mut changes = self.changes();
        if changes.known.contains_key(&change) {
            return None;
        }
        changes.known.insert(change.clone(), None);
        Some(Remembering {
            outcomes: self.clone(),
            change: Some(change),
        })
    }

    /// What this service knows of `change`.
    pub fn of(&self, change: &Token) -> Outcome {
        match self.changes().known.get(change) {
            None => Outcome::Unknown,
            Some(None) => Outcome::Running,
            Some(Some(reply)) => Outcome::Done {
                reply: reply.clone(),
            },
        }
    }

    /// The memory is plain data, whole after every change to it, so one a
    /// panicking holder left is still read.
    fn changes(&self) -> MutexGuard<'_, Changes> {
        self.changes.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A remembered change being carried out. Dropped without its reply, it
/// is forgotten: the service itself cannot say what it came to.
#[must_use = "dropping it at once forgets the change"]
pub struct Remembering {
    outcomes: Arc<Outcomes>,
    /// The change, until its reply is remembered.
    change: Option<Token>,
}

impl Remembering {
    /// Remember `reply` as what the change came to: should the reply be
    /// lost on its way, it can still be asked for.
    pub fn done(mut self, reply: &Value) {
        let Some(change) = self.change.take() else {
            return;
        };
        let fits = serde_json::to_string(reply).is_ok_and(|text| text.len() <= REPLY_MAX);
        let mut changes = self.outcomes.changes();
        if !fits {
            changes.known.remove(&change);
            return;
        }
        changes.known.insert(change.clone(), Some(reply.clone()));
        changes.ended.push_back(change);
        while changes.ended.len() > KEPT
            && let Some(oldest) = changes.ended.pop_front()
        {
            changes.known.remove(&oldest);
        }
    }
}

impl Drop for Remembering {
    fn drop(&mut self) {
        if let Some(change) = self.change.take() {
            self.outcomes.changes().known.remove(&change);
        }
    }
}

#[cfg(test)]
#[path = "outcomes_tests.rs"]
mod tests;
