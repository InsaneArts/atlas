//! A value that may come from another build, in a record or a reply.
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A value this build knows, or one a later build wrote, kept as it was.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Lenient<T> {
    Known(T),
    Later(Value),
}

impl<T> From<T> for Lenient<T> {
    fn from(known: T) -> Self {
        Self::Known(known)
    }
}
