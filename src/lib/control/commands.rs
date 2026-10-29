//! Which pending changes are commands agents started with `run`, kept
//! beside the marker as `["<generation>", …]`. It is read only against the
//! marker and the claims: an entry the marker no longer has means nothing,
//! and one whose claim no live process holds is an outcome nothing will
//! settle, like any other. A list that is missing, unreadable, malformed or
//! too large reads as empty, so its commands count as unknown outcomes.
use super::{by_generation, marker::replace_private};
use crate::error::Result;
use std::path::Path;

/// The generations the list at `path` marks as commands.
pub(super) fn read(path: &Path) -> Vec<String> {
    by_generation::read(path)
}

/// Mark `generation` as a command, keeping only the marks of generations
/// still `pending`.
pub(super) fn mark(path: &Path, generation: &str, pending: &[String]) -> Result<()> {
    let mut commands = read(path);
    commands.retain(|kept| kept != generation && pending.contains(kept));
    commands.push(generation.to_owned());
    write(path, &commands)
}

/// Forget every mark whose generation is no longer `pending`, removing the
/// list once none is left. A missing list stays missing.
pub(super) fn keep(path: &Path, pending: &[String]) -> Result<()> {
    by_generation::keep_with(path, pending, read, |path, commands: &Vec<String>| {
        write(path, commands)
    })
}

/// Replace the list at `path` with `commands`, whole or not at all.
fn write(path: &Path, commands: &[String]) -> Result<()> {
    replace_private(path, &serde_json::to_vec(commands)?)
}
