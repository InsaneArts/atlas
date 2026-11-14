//! Who carries out each pending change, when it is not the desktop's own
//! services, kept beside the marker as `{"<generation>": "<executor>"}`
//! and replaced whole. A change named nowhere here is the helper daemon's
//! or the input service's, as every change was before others existed, and
//! recovery may settle it once their restart proves it safe. One named
//! here waits for its own executor's answer. A record that cannot be read
//! is never taken as empty: recovery then settles nothing.
use super::{
    by_generation::ByGeneration,
    marker::{UNREADABLE, replace_private},
};
use atlas_core::{
    error::{Code, Error, Result},
    files::read_own,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The most the record may hold.
const LIMIT: u64 = 64 * 1024;

/// Who carries out a change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Executor {
    /// The desktop's daemon, the helper login's or a machine guest's.
    Daemon,
    /// The desktop's input service.
    Input,
    /// A machine desktop's own records, answered over its session.
    Guest,
    /// Atlas VM, which runs a virtual machine desktop.
    Vm,
    /// The station service.
    Station,
}

impl Executor {
    /// Whether restarting the desktop's own services settles it.
    pub const fn local(self) -> bool {
        matches!(self, Self::Daemon | Self::Input)
    }
}

/// The executors of the pending changes at `path`: none when there is no
/// record.
///
/// # Errors
///
/// Fails when a record is there but cannot be read whole.
pub(super) fn read(path: &Path) -> Result<ByGeneration<Executor>> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ByGeneration::new());
        }
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    read_own(path, LIMIT)
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or_else(|| {
            Error::new(
                Code::State,
                "Who carries out the pending changes cannot be read; nothing is settled.",
            )
        })
}

/// Name `executor` as `generation`'s, keeping only the generations still
/// `pending`, before the marker names it.
///
/// # Errors
///
/// Fails when the record cannot be read or replaced: then the change is not
/// recorded at all.
pub(super) fn name(
    path: &Path,
    generation: &str,
    executor: Executor,
    pending: &[String],
) -> Result<()> {
    let mut named = read(path)?;
    prune(&mut named, pending);
    named.insert(generation.to_owned(), executor);
    let bytes = serde_json::to_vec(&named)?;
    // Never more than its reader takes: the change is refused instead.
    if bytes.len() as u64 > LIMIT {
        return Err(Error::new(
            Code::Busy,
            "Other executors hold too many pending changes; wait for them to answer.",
        ));
    }
    replace_private(path, &bytes)
}

/// Forget every executor whose generation is no longer `pending`, removing
/// the record once none is left. A record that cannot be read stays as it
/// is while any change is pending.
pub(super) fn keep(path: &Path, pending: &[String]) -> Result<()> {
    let Ok(mut named) = read(path) else {
        // Damaged, it says nothing once no change is pending at all.
        if pending.is_empty() {
            return forget(path);
        }
        return Ok(());
    };
    if std::fs::symlink_metadata(path).is_err() {
        return Ok(());
    }
    prune(&mut named, pending);
    if named.is_empty() {
        return match std::fs::remove_file(path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        };
    }
    replace_private(path, &serde_json::to_vec(&named)?)
}

/// Keep in `named` only the generations still `pending`: all of them while
/// the marker cannot be read, since it may name any of them.
fn prune(named: &mut ByGeneration<Executor>, pending: &[String]) {
    if pending.iter().any(|entry| entry == UNREADABLE) {
        return;
    }
    named.retain(|kept, _| pending.contains(kept));
}

/// Remove the record, nothing being pending any more.
///
/// # Errors
///
/// Fails when it is there and cannot be removed.
pub(super) fn forget(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}
