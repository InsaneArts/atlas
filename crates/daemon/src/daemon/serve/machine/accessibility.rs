//! A machine desktop's Accessibility, which its person allows in the VM's
//! own window, asked as Second Login asks it: macOS's prompt, then its
//! System Settings page should the person not have allowed it a while
//! later. Nothing is allowed here; the person decides, in the normal way.
//!
//! macOS shows one prompt at a time, so it is asked only once this daemon
//! may record its screen: Screen Recording is asked first, and its grant
//! restarts the daemon, which then asks for Accessibility. It is asked once
//! for the guest, as Screen Recording is: kept asked before macOS is asked,
//! so no later daemon asks again. A record that cannot be read asks again.
use atlas_core::files::{read_limited, try_open_plain, write_durable};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc, time::Duration};

/// How often Screen Recording, then Accessibility, is looked at.
const LOOK: Duration = Duration::from_secs(2);
/// How many looks after the prompt its System Settings page is opened, the
/// person not having allowed it yet.
const SETTINGS_AFTER: u32 = 15;

/// The longest the record may be.
const LIMIT: u64 = 4096;

const VERSION: u32 = 1;

/// What the guest does about Accessibility, and how it reads it.
pub(super) trait Trust: Send + Sync {
    /// Whether this daemon may record its screen.
    fn recording(&self) -> bool;
    /// Whether Atlas is allowed Accessibility.
    fn trusted(&self) -> bool;
    /// Ask macOS for it, with its prompt in the VM's window.
    fn request(&self);
    /// Open its page of System Settings in the VM's window.
    fn open_settings(&self);
}

/// What the guest kept of Accessibility: whether a daemon of it asked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Kept {
    version: u32,
    asked: bool,
}

/// Where the guest keeps it, beside its records.
pub(super) struct Record(PathBuf);

impl Record {
    pub(super) fn at(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    /// Whether a daemon of this guest asked already; never when that cannot
    /// be read.
    fn asked(&self) -> bool {
        let Ok(Some(file)) = try_open_plain(&self.0) else {
            return false;
        };
        read_limited(file, LIMIT)
            .and_then(|bytes| serde_json::from_slice::<Kept>(&bytes).ok())
            .is_some_and(|kept| kept.version == VERSION && kept.asked)
    }

    /// Keep it asked: whether it was.
    fn keep_asked(&self) -> bool {
        let kept = Kept {
            version: VERSION,
            asked: true,
        };
        serde_json::to_vec(&kept).is_ok_and(|bytes| write_durable(&self.0, &bytes, 0o600).is_ok())
    }
}

/// Ask for Accessibility once for the guest, once this daemon may record
/// its screen, unless it is allowed, and open its System Settings page a
/// while later should the person not have allowed it by then.
pub(super) async fn watch(guest: &impl Trust, record: &Record) {
    while !guest.recording() {
        if guest.trusted() {
            return;
        }
        tokio::time::sleep(LOOK).await;
    }
    // Kept before it is asked: one that cannot be kept is not asked, as
    // each daemon that starts would ask again.
    if guest.trusted() || record.asked() || !record.keep_asked() {
        return;
    }
    guest.request();
    for _ in 0..SETTINGS_AFTER {
        tokio::time::sleep(LOOK).await;
        if guest.trusted() {
            return;
        }
    }
    guest.open_settings();
}

/// The machine's own Accessibility, as macOS gives it to this process.
pub(super) struct Own(pub(super) Arc<super::Machine>);

impl Trust for Own {
    fn recording(&self) -> bool {
        (self.0.daemon.screen.recording)()
    }

    fn trusted(&self) -> bool {
        atlas_native::native::accessibility_trusted()
    }

    fn request(&self) {
        crate::daemon::consent::request_accessibility();
    }

    fn open_settings(&self) {
        // It waits on `open`, so it runs off the runtime.
        tokio::task::spawn_blocking(|| {
            crate::daemon::consent::open_settings("Privacy_Accessibility");
        });
    }
}

#[cfg(test)]
#[path = "accessibility_tests.rs"]
mod tests;
