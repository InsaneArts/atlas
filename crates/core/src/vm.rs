//! A VM on this Mac that Atlas VM runs: a desktop whose apps, processes and
//! files are its guest's, reached only over its session.
use crate::desktop::{DesktopId, RecordKey};

/// A VM desktop, named by the identifier Atlas VM gave it: its folder, its
/// launchd job and its Keychain items carry the same one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vm {
    pub id: DesktopId,
}

impl Vm {
    /// The name its records carry. Reinstalling Atlas in the same VM keeps
    /// its identifier, so a repaired VM keeps its records; a new VM is a new
    /// desktop with records of its own.
    pub fn record_key(&self) -> RecordKey {
        RecordKey::vm(&self.id)
    }
}
