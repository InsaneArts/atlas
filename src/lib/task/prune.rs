//! What the session and task records keep each other for, said in one
//! place: a session's record stays while one of its tasks still holds
//! something in the helper, and a task's record goes once it holds nothing
//! and its session's record is gone. Each store prunes its own records by
//! these rules and never reads the other's.
use super::{record::Record, store::Store as Tasks};
use crate::host_session::{Key, store::Store as Sessions};
use std::path::Path;

/// The sessions in the output folder `output` whose tasks still hold
/// something, an unfinished cleanup among them.
pub(crate) fn sessions_held(output: &Path) -> Vec<Key> {
    Tasks::at(output)
        .recent()
        .iter()
        .filter(|task| task.holds_anything())
        .map(|task| task.session.clone())
        .collect()
}

/// Whether the task `record` in the output folder `output` may go: it
/// holds nothing that may still be in the helper, and its session's record
/// is gone.
pub(crate) fn finished(output: &Path, record: &Record) -> bool {
    !record.holds_anything() && Sessions::at(output).read(&record.session).is_none()
}
