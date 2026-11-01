//! Records kept beside the marker, one entry per pending generation, each
//! read only against the marker: an entry the marker no longer has means
//! nothing. Two are labels, kept as `{"<generation>": <value>}`: which host
//! session claimed each pending change, and who started each command agents
//! started with `run` while it runs. A label is for display only; what
//! blocks and what recovery clears never depend on it, so it is written in
//! place, to the kernel, at every change: one cut short reads as empty and
//! loses its labels, nothing more. The list of which changes are commands
//! decides what may go on beside them, so it is replaced whole instead.
use crate::{
    error::Result,
    files::{Durability, write_private_with},
};
use serde::{Serialize, de::DeserializeOwned};
use std::{collections::BTreeMap, fs, path::Path};

/// A label of each pending generation.
pub(super) type ByGeneration<V> = BTreeMap<String, V>;

/// The most a record may hold before it is treated as missing.
const LIMIT: u64 = 64 * 1024;

/// A record kept beside the marker, one entry per pending generation.
pub(super) trait Entries {
    /// Keep only the entries whose generations are still `pending`.
    fn retain_pending(&mut self, pending: &[String]);

    fn is_empty(&self) -> bool;
}

impl<V> Entries for ByGeneration<V> {
    fn retain_pending(&mut self, pending: &[String]) {
        self.retain(|kept, _| pending.contains(kept));
    }

    fn is_empty(&self) -> bool {
        BTreeMap::is_empty(self)
    }
}

impl Entries for Vec<String> {
    fn retain_pending(&mut self, pending: &[String]) {
        self.retain(|kept| pending.contains(kept));
    }

    fn is_empty(&self) -> bool {
        <[String]>::is_empty(self)
    }
}

/// The record at `path`; empty when it is missing, unreadable, malformed,
/// too large or not this account's.
pub(super) fn read<T: DeserializeOwned + Default>(path: &Path) -> T {
    crate::files::read_own(path, LIMIT)
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Label `generation` with `value` in the labels at `path`, keeping only
/// the generations still `pending`.
pub(super) fn note<V: Serialize + DeserializeOwned + Clone>(
    path: &Path,
    generation: &str,
    value: &V,
    pending: &[String],
) -> Result<()> {
    let mut labels: ByGeneration<V> = read(path);
    labels.retain_pending(pending);
    labels.insert(generation.to_owned(), value.clone());
    write(path, &labels)
}

/// Forget every label at `path` whose generation is no longer `pending`,
/// removing the file once none is left. A missing file stays missing.
pub(super) fn keep<V: Serialize + DeserializeOwned>(path: &Path, pending: &[String]) -> Result<()> {
    keep_with(path, pending, read::<ByGeneration<V>>, write)
}

/// Keep only the entries of the record at `path`, read with `read` and
/// written with `write`, whose generations are still `pending`, removing it
/// once none is left. A missing record stays missing.
pub(super) fn keep_with<T: Entries>(
    path: &Path,
    pending: &[String],
    read: impl FnOnce(&Path) -> T,
    write: impl FnOnce(&Path, &T) -> Result<()>,
) -> Result<()> {
    if fs::symlink_metadata(path).is_err() {
        return Ok(());
    }

    let mut entries = read(path);
    entries.retain_pending(pending);
    if entries.is_empty() {
        return match fs::remove_file(path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                Err(crate::context::context(format_args!(
                    "Remove generation labels {}",
                    path.display()
                ))(error))
            }
            _ => Ok(()),
        };
    }

    write(path, &entries)
}

fn write<V: Serialize>(path: &Path, labels: &ByGeneration<V>) -> Result<()> {
    write_private_with(path, &serde_json::to_vec(labels)?, Durability::Kernel)
}
