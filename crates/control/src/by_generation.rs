//! Records kept beside the marker, one entry per pending generation, each
//! read only against the marker: an entry the marker no longer has means
//! nothing. Two are labels, kept as `{"<generation>": <value>}`: which host
//! session claimed each pending change, and who started each command agents
//! started with `run` while it runs. A label is for display only; what
//! blocks and what recovery clears never depend on it, so it is written in
//! place, to the kernel, at every change: one cut short reads as empty and
//! loses its labels, nothing more. The list of which changes are commands
//! decides what may go on beside them, so it is replaced whole instead.
use atlas_core::{
    error::Result,
    files::{Durability, write_private_with},
};
use serde::{Serialize, de::DeserializeOwned};
use std::{collections::BTreeMap, fs, path::Path};

/// A label of each pending generation.
pub(super) type ByGeneration<V> = BTreeMap<String, V>;

/// The most a record may hold before it is treated as missing.
const LIMIT: u64 = 64 * 1024;

/// The record at `path`; empty when it is missing, unreadable, malformed,
/// too large or not this account's.
pub(super) fn read<T: DeserializeOwned + Default>(path: &Path) -> T {
    atlas_core::files::read_own(path, LIMIT)
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
    labels.retain(|kept, _| pending.contains(kept));
    labels.insert(generation.to_owned(), value.clone());
    write(path, &labels)
}

/// Forget every label at `path` whose generation is no longer `pending`,
/// removing the file once none is left. A missing file stays missing.
pub(super) fn keep<V: Serialize + DeserializeOwned>(path: &Path, pending: &[String]) -> Result<()> {
    keep_with(
        path,
        |path| {
            let mut labels: ByGeneration<V> = read(path);
            labels.retain(|kept, _| pending.contains(kept));
            (!labels.is_empty()).then_some(labels)
        },
        write,
    )
}

/// Keep the nonempty record `read` retains, or remove it. Check existence
/// before reading so an absent record never creates a new file.
pub(super) fn keep_with<T>(
    path: &Path,
    read: impl FnOnce(&Path) -> Option<T>,
    write: impl FnOnce(&Path, &T) -> Result<()>,
) -> Result<()> {
    if fs::symlink_metadata(path).is_err() {
        return Ok(());
    }

    match read(path) {
        Some(entries) => write(path, &entries),
        None => match fs::remove_file(path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        },
    }
}

fn write<V: Serialize>(path: &Path, labels: &ByGeneration<V>) -> Result<()> {
    write_private_with(path, &serde_json::to_vec(labels)?, Durability::Kernel)
}
