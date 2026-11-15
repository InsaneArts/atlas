//! The guest's root installer: the one process that may replace the guest's
//! root-owned Atlas, and only with a bundle the genuine machine daemon
//! staged, copied in by the installer itself, verified, swapped into place
//! whole, kept recoverable at every step, and rolled back when no daemon of
//! the new build releases it in time.
pub(in crate::machine) mod copy;
pub(in crate::machine) mod own;
pub(in crate::machine) mod record;
mod serve;
mod transaction;

pub use serve::serve_blocking;

use std::path::{Path, PathBuf};

/// Where the installer works: `/` in the guest; a temporary root in tests.
#[derive(Clone, Debug)]
struct Layout {
    root: PathBuf,
}

impl Layout {
    /// The guest's own.
    fn guest() -> Self {
        Self {
            root: PathBuf::from("/"),
        }
    }

    /// One under `root`, for a test.
    #[cfg(test)]
    pub(crate) fn under(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
        }
    }

    /// The folder Atlas is installed in.
    fn applications(&self) -> PathBuf {
        self.root.join("Applications")
    }

    /// The installer's own folder, root-owned.
    fn home(&self) -> PathBuf {
        self.root
            .join("Library/Application Support/Atlas/Installer")
    }

    /// Its transactions' records and folders, private to root.
    fn transactions(&self) -> PathBuf {
        self.home().join("Transactions")
    }

    /// Every request's record, private to root.
    fn requests(&self) -> PathBuf {
        self.home().join("Requests")
    }

    /// The socket it answers on.
    fn socket(&self) -> PathBuf {
        self.root
            .join("var/run/com.insanearts.atlas.machine-installer")
    }
}

/// What the installer asks of the system, given apart so tests stand in.
trait System: Send + Sync {
    /// The code directory hash of `bundle`'s `atlas`, when the bundle is
    /// Atlas's, whole and signed with its Developer ID; none otherwise.
    fn verified(&self, bundle: &Path) -> Option<Vec<u8>>;
    /// Start the machine job of the account `uid` again, ending the process
    /// it runs now, whatever that process is doing.
    fn restart(&self, uid: u32) -> bool;
    /// The code directory hash of what the machine job of the account `uid`
    /// runs now; none when it runs nothing, or that cannot be told.
    fn job_code(&self, uid: u32) -> Option<Vec<u8>>;
    /// Now, in milliseconds past the epoch.
    fn now(&self) -> u64;
}

/// How long a signature check or a launchd request may take before it
/// counts as failed.
const CHECKING: std::time::Duration = std::time::Duration::from_secs(120);

/// The code directory hash of `bundle`'s `atlas`, when the bundle is
/// Atlas's, whole and signed with its Developer ID; none otherwise. Bounded:
/// a stalled check is a failed one, never a wait without end.
pub(in crate::machine) fn verified_bundle(bundle: &Path) -> Option<Vec<u8>> {
    let whole = atlas_core::bounded::output(
        std::process::Command::new("/usr/bin/codesign")
            .args(["--verify", "--strict", "--deep"])
            .arg(bundle),
        CHECKING,
    )
    .is_ok_and(|output| output.status.success());
    let program = bundle.join("Contents/MacOS/atlas");
    let signed = whole && atlas_core::app_bundle::developer_id(&program);
    signed
        .then(|| atlas_native::peer::file_cdhash(&program))
        .flatten()
}

/// The guest's own system.
struct Real;

impl System for Real {
    fn verified(&self, bundle: &Path) -> Option<Vec<u8>> {
        verified_bundle(bundle)
    }

    fn restart(&self, uid: u32) -> bool {
        let job = format!("gui/{uid}/{}", super::launch::LABEL);
        atlas_core::bounded::output(
            std::process::Command::new("/bin/launchctl").args(["kickstart", "-k", &job]),
            CHECKING,
        )
        .is_ok_and(|output| output.status.success())
    }

    fn job_code(&self, uid: u32) -> Option<Vec<u8>> {
        let job = atlas_core::launchd::job(&format!("gui/{uid}"), super::launch::LABEL).ok()?;
        let atlas_core::launchd::Job::Loaded(loaded) = job else {
            return None;
        };
        let pid = i32::try_from(loaded.pid?).ok()?;
        let token = atlas_native::native::process_token::Token::of(pid).ok()?;
        atlas_native::peer::running_code(token.words())
    }

    fn now(&self) -> u64 {
        atlas_core::time::now_ms()
    }
}
