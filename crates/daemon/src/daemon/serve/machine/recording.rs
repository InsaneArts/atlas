//! A machine desktop's Screen Recording, which its person grants in the
//! VM's own window, with no setup window to guide them, and which only a
//! new process of its daemon takes.
//!
//! While this daemon may not record, its record says capture is held, kept
//! before macOS is asked, once for the guest, through its own prompt. A
//! daemon that starts and may record while the record says held started
//! again for the grant, by the prompt's Quit & Reopen, by System Settings
//! or by this daemon's own restart: it saves a Stop before any agent is
//! admitted, so agents wait for the person's Resume. Asking stops nothing:
//! a guest never granted goes on with its pictures refused. A grant this
//! daemon cannot take restarts it at most once for each grant a new process
//! sees: one that starts again and still may not record never restarts for
//! it again.
use atlas_core::files::{read_limited, try_open_plain, write_durable};
use serde::{Deserialize, Serialize};
use std::{future::Future, path::PathBuf, sync::Arc, time::Duration};
use tokio::time::Instant;

/// How often a grant is looked for while the person is likely at the
/// VM's window, just asked: every 2 s, for the first minute.
const EAGER: Duration = Duration::from_secs(2);
/// For how long the daemon looks so often.
const EAGERLY: Duration = Duration::from_secs(60);
/// How often a grant is looked for after that: every minute.
const LATER: Duration = Duration::from_secs(60);
/// The longest one look takes: a fresh process asked, killed past it.
const PROBE: Duration = Duration::from_secs(5);

/// The longest the record may be.
const LIMIT: u64 = 4096;

const VERSION: u32 = 1;

/// What the guest does about Screen Recording, and how it reads it.
pub(super) trait Recording: Send + Sync {
    /// Whether this process may record its screen.
    fn permitted(&self) -> bool;
    /// Whether a new process may, as one started to look says: none when
    /// it could not tell.
    fn seen(&self) -> impl Future<Output = Option<bool>> + Send;
    /// Ask macOS for it, with its prompt in the VM's window.
    fn request(&self);
    /// Restart the daemon to take it, its Stop saved first: whether it
    /// restarts.
    fn restart(&self) -> impl Future<Output = bool> + Send;
}

/// What the guest kept of Screen Recording: whether a daemon of it could
/// not record, whether it asked macOS, and whether it restarted for the
/// grant a new process sees now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Kept {
    version: u32,
    held: bool,
    asked: bool,
    restarted: bool,
}

/// Where the guest keeps it, beside its records.
pub(super) struct Record(PathBuf);

impl Record {
    pub(super) fn at(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    /// As kept: nothing yet when none was, none when it cannot be read.
    fn read(&self) -> Option<Kept> {
        let Some(file) = try_open_plain(&self.0).ok()? else {
            return Some(Kept::default());
        };
        let bytes = read_limited(file, LIMIT)?;
        serde_json::from_slice::<Kept>(&bytes)
            .ok()
            .filter(|kept| kept.version == VERSION)
    }

    /// Whether capture is noted held; held when that cannot be read.
    pub(super) fn held(&self) -> bool {
        self.read().is_none_or(|kept| kept.held)
    }

    /// At a daemon's start, before any agent is admitted, `permitted` saying
    /// whether it may record: one that may while capture is noted held
    /// started again for the grant, and saves a Stop for it with `stop`
    /// before the hold is forgotten. Whether agents may be admitted: never
    /// while that Stop could not be saved, the hold then kept. Blocking.
    pub(super) fn reconcile(&self, permitted: bool, stop: impl FnOnce() -> bool) -> bool {
        if !permitted || !self.held() {
            return true;
        }
        if !stop() {
            return false;
        }
        // One not forgotten is reconciled again at the next start, which
        // finds the Stop saved already.
        let kept = self.read().unwrap_or_default();
        self.keep(Kept {
            held: false,
            ..kept
        });
        true
    }

    /// Keep `kept`: whether it was.
    fn keep(&self, kept: Kept) -> bool {
        let kept = Kept {
            version: VERSION,
            ..kept
        };
        // Its name synced with it: a hold lost to a crash would let the
        // daemon that starts after a grant admit agents unstopped.
        serde_json::to_vec(&kept).is_ok_and(|bytes| write_durable(&self.0, &bytes, 0o600).is_ok())
    }
}

/// Ask for Screen Recording, unless this daemon may record, then watch for
/// a grant it cannot take, until it may record or restarts.
pub(super) async fn watch(guest: &impl Recording, record: &Record) {
    if !guest.permitted() {
        ask(guest, record).await;
    }
    let started = Instant::now();
    loop {
        if guest.permitted() {
            // Taken: a grant given again after one taken away restarts
            // once more.
            if let Some(kept) = record.read().filter(|kept| kept.restarted) {
                record.keep(Kept {
                    restarted: false,
                    ..kept
                });
            }
            return;
        }
        // A look that cannot tell, timed out or not, is neither.
        let seen = tokio::time::timeout(PROBE, guest.seen())
            .await
            .ok()
            .flatten();
        match record.read() {
            // Kept before it goes, or never made: one that cannot be kept
            // could restart over and over.
            Some(kept) if seen == Some(true) && !kept.restarted => {
                let restarted = Kept {
                    restarted: true,
                    ..kept
                };
                if record.keep(restarted) && guest.restart().await {
                    return;
                }
            }
            // Denied, as a new process says for certain: the next grant is
            // a new one.
            Some(kept) if seen == Some(false) && kept.restarted => {
                record.keep(Kept {
                    restarted: false,
                    ..kept
                });
            }
            _ => {}
        }
        let pause = if started.elapsed() < EAGERLY {
            EAGER
        } else {
            LATER
        };
        tokio::time::sleep(pause).await;
    }
}

/// Note capture held, then ask macOS for Screen Recording once for this
/// guest, as macOS shows its prompt only once: never before the hold is
/// kept, which tells a daemon that starts again that a grant came. A record
/// that cannot be read asks again.
async fn ask(guest: &impl Recording, record: &Record) {
    let kept = record.read();
    if kept.is_some_and(|kept| kept.held && kept.asked) {
        return;
    }
    let asked = kept.is_some_and(|kept| kept.asked);
    let held = Kept {
        held: true,
        asked: true,
        ..kept.unwrap_or_default()
    };
    if record.keep(held) && !asked {
        guest.request();
    }
}

/// The machine's own Screen Recording, as macOS gives it to this process.
pub(super) struct Own(pub(super) Arc<super::Machine>);

impl Recording for Own {
    fn permitted(&self) -> bool {
        (self.0.daemon.screen.recording)()
    }

    fn seen(&self) -> impl Future<Output = Option<bool>> + Send {
        crate::daemon::consent::screen_recording_seen()
    }

    fn request(&self) {
        atlas_native::native::capture::request();
    }

    fn restart(&self) -> impl Future<Output = bool> + Send {
        self.0.restart_for_recording()
    }
}

#[cfg(test)]
#[path = "recording_tests.rs"]
mod tests;
