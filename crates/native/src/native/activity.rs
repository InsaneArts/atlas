//! Input in the helper, as other processes there see it: a lock file beside
//! the input service's socket. The input service holds it shared while it
//! acts or reads an app's elements, and stamps its modification time when
//! it is done. Work that must never overlap input, such as restarting
//! macOS's screen recorder, takes it whole, and only once nothing has acted
//! for a while; input that comes meanwhile waits for that work and is never
//! refused. The kernel lets go of the lock of a holder that exits.
use atlas_core::files::{self, Lock, LockFile};
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

/// How long input waits for work that keeps it out: longer than a screen
/// recorder restart may hold it, a signal and the 1 s its recorder is given
/// to exit. Past it, input goes on anyway, so a holder that never lets go
/// cannot stall input. With the focus check's 1.25 s and a batch's 2 s of
/// pauses, a request still ends within the controller's 5 s.
const PATIENCE: Duration = Duration::from_millis(1500);

/// The activity file of the input service listening on `socket`.
pub fn beside(socket: &Path) -> PathBuf {
    socket.with_extension("activity")
}

/// Input under way, from [`acting`]: it holds the file shared, and stamps
/// when it ended as it is dropped.
#[must_use]
pub struct Acting(LockFile);

impl Drop for Acting {
    fn drop(&mut self) {
        // Stamped before the lock goes, so work that takes it next sees it.
        let _ = self.0.0.set_modified(SystemTime::now());
    }
}

/// Input about to begin at `path`, once no work that keeps input out runs,
/// waiting up to [`PATIENCE`] for it. This blocks. It never fails: input
/// without the file, or past its patience, goes on unannounced, holding
/// nothing.
pub fn acting(path: &Path) -> Option<Acting> {
    acting_within(path, PATIENCE)
}

fn acting_within(path: &Path, patience: Duration) -> Option<Acting> {
    let file = files::open_lock(path).ok()?;
    files::lock_within(&file, Lock::Shared, patience).ok()?;
    Some(Acting(file))
}

/// Run `work` with input at `path` kept out until it returns, once nothing
/// acts there and nothing has for `gap`; none, without waiting, when
/// something does or did. A file that cannot be opened has no input service
/// beside it to wait for, so `work` runs.
pub fn quiet<T>(path: &Path, gap: Duration, work: impl FnOnce() -> T) -> Option<T> {
    let Ok(file) = files::open_lock(path) else {
        return Some(work());
    };
    files::lock_within(&file, Lock::Exclusive, Duration::ZERO).ok()?;
    let stamped = file.0.metadata().and_then(|metadata| metadata.modified());
    // A clock set back since the stamp still counts the time between.
    let since = stamped
        .ok()?
        .elapsed()
        .unwrap_or_else(|ahead| ahead.duration());
    (since >= gap).then(work)
}

#[cfg(test)]
#[path = "activity_tests.rs"]
mod tests;
