//! Who started each command agents started with `run`, kept beside the
//! marker as `{"<generation>": <note>}` while it runs. Such a command lets
//! go of control once it is sent, so no holder note names it, and a session
//! on its first command has no record until that command ends: this note
//! names it meanwhile, as its record will. It is read only against the
//! marks: an entry no mark names means nothing. A label for display only;
//! what blocks and what recovery clears never depend on it.
use super::{Entries, keep_entries, notes::Note};
use crate::{
    error::Result,
    files::{Durability, write_private_with},
};
use std::{collections::BTreeMap, path::Path};

/// The most a list may hold before it is treated as missing.
const LIMIT: u64 = 64 * 1024;

/// Each running command's note, by its generation, as the file at `path`
/// holds them; empty when it is missing, unreadable, too large or not this
/// account's.
pub(super) fn read(path: &Path) -> BTreeMap<String, Note> {
    crate::files::read_own(path, LIMIT)
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Note who started `generation`, keeping only the generations still
/// `pending`.
pub(super) fn note(path: &Path, generation: &str, note: &Note, pending: &[String]) -> Result<()> {
    let mut notes = read(path);
    notes.retain(|kept, _| pending.contains(kept));
    notes.insert(generation.to_owned(), note.clone());
    write(path, &notes)
}

/// Forget every note whose generation is no longer in `kept`, removing the
/// file once none is left. A missing file stays missing.
pub(super) fn keep(path: &Path, kept: &[String]) -> Result<()> {
    keep_entries(path, kept, read, write)
}

impl Entries for BTreeMap<String, Note> {
    fn retain_pending(&mut self, pending: &[String]) {
        self.retain(|kept, _| pending.contains(kept));
    }

    fn is_empty(&self) -> bool {
        BTreeMap::is_empty(self)
    }
}

fn write(path: &Path, notes: &BTreeMap<String, Note>) -> Result<()> {
    write_private_with(path, &serde_json::to_vec(notes)?, Durability::Kernel)
}
