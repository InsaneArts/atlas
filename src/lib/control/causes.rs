//! Which host session claimed each pending change, kept beside the marker
//! as `{"<generation>": "<key>"}`. It is read only against the marker: an
//! entry the marker no longer has means nothing. A label for display only;
//! what blocks and what recovery clears never depend on it.
use super::{Durability, Entries, keep_entries, write_private_with};
use crate::{error::Result, host_session::Key};
use std::{collections::BTreeMap, path::Path};

/// The most a causes file may hold before it is treated as missing.
const LIMIT: u64 = 64 * 1024;

/// Each generation's host session, as the file at `path` names it; empty
/// when it is missing, unreadable, too large or not this account's.
pub(super) fn read(path: &Path) -> BTreeMap<String, Key> {
    crate::helper_info::read_own(path, LIMIT)
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Note that host session `key` claimed `generation`, keeping only the
/// generations still `pending`.
pub(super) fn note(path: &Path, generation: &str, key: &Key, pending: &[String]) -> Result<()> {
    let mut causes = read(path);
    causes.retain(|kept, _| pending.contains(kept));
    causes.insert(generation.to_owned(), key.clone());
    write(path, &causes)
}

/// Forget every cause whose generation is no longer `pending`, removing the
/// file once none is left. A missing file stays missing.
pub(super) fn keep(path: &Path, pending: &[String]) -> Result<()> {
    keep_entries(path, pending, read, write)
}

impl Entries for BTreeMap<String, Key> {
    fn retain_pending(&mut self, pending: &[String]) {
        self.retain(|kept, _| pending.contains(kept));
    }

    fn is_empty(&self) -> bool {
        BTreeMap::is_empty(self)
    }
}

fn write(path: &Path, causes: &BTreeMap<String, Key>) -> Result<()> {
    write_private_with(path, &serde_json::to_vec(causes)?, Durability::Kernel)
}
