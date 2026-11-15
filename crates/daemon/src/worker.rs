//! Whether a hidden worker Atlas starts for itself runs where it belongs.
//! These are not agent commands: started anywhere else, by an agent on the
//! person's Mac above all, each refuses before it does anything. Each
//! desktop's daemon tells its own workers apart: a Second Login's in
//! [`crate::user::worker`]. In a machine guest, the daemon proves itself to
//! launchd by its root-owned plist ([`crate::machine::launch`]), and its
//! supervisor is its child, its lifeline a socket the daemon made. On the
//! station, what Atlas does to a VM for itself is started by an Atlas command
//! alone: its parent runs this very Atlas.
use atlas_core::error::{Code, Error, Result};
use std::{os::fd::AsFd, path::Path};

/// The one refusal of a worker started where it does not belong.
pub const REFUSAL: &str = "Atlas runs this itself; it can't be run from here";

/// In a test's Atlas, the process that stands for the daemon launchd would
/// run, by its PID: tests run the workers as their own children.
#[cfg(any(test, feature = "test-support"))]
pub const TEST_DAEMON: &str = "ATLAS_TEST_WORKER_DAEMON";

/// The process at the other end of a socket, and its account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Peer {
    pub(crate) pid: u32,
    pub(crate) uid: u32,
}

/// The refusal of a worker started where it does not belong.
pub fn refusal() -> Error {
    Error::new(Code::Denied, REFUSAL)
}

/// Nothing when `holds`, else the one refusal.
pub(crate) fn allowed(holds: bool) -> Result<()> {
    if holds { Ok(()) } else { Err(refusal()) }
}

/// Whether `handed`, who made this process's standard input, is `parent` in
/// the account `uid`.
pub(crate) fn handed_by(handed: Option<Peer>, parent: u32, uid: u32) -> bool {
    handed == Some(Peer { pid: parent, uid })
}

/// Refuse unless this process is a supervisor as a machine guest's daemon
/// starts one: that daemon's child, as launchd started it, with a lifeline
/// the daemon made as its standard input.
///
/// # Errors
///
/// Returns the one refusal.
pub fn machine_supervisor() -> Result<()> {
    let parent = std::os::unix::process::parent_id();
    allowed(handed_by(handed(), parent, nix::unistd::getuid().as_raw()))?;
    #[cfg(any(test, feature = "test-support"))]
    if test_daemon() == Some(parent) {
        return Ok(());
    }
    crate::machine::launch::prove_parent_now().map_err(|error| refusal().with_source(error))
}

/// Refuse unless an Atlas command started this process: its parent runs
/// this very program, or another of the same Atlas app. What the station's
/// own work on a VM, its agent and its removal, requires.
///
/// # Errors
///
/// Returns the one refusal.
pub fn started_by_atlas() -> Result<()> {
    if HAND_RUN {
        return Ok(());
    }
    let parent =
        atlas_core::app_bundle::signature::executable_of(std::os::unix::process::parent_id());
    let own = std::env::current_exe().ok();
    allowed(matches!((parent, own), (Some(parent), Some(own)) if same_atlas(&parent, &own)))
}

/// Set when this build was made for live checks, whose hidden VM commands a
/// person runs by hand; `scripts/release.sh` refuses to release one.
const HAND_RUN: bool = option_env!("ATLAS_TEST_HAND_RUN").is_some();

/// Whether the programs at `parent` and `own` are one Atlas: the same file,
/// or both in the same app.
fn same_atlas(parent: &Path, own: &Path) -> bool {
    let (Ok(parent), Ok(own)) = (std::fs::canonicalize(parent), std::fs::canonicalize(own)) else {
        return false;
    };
    let app = atlas_core::app_bundle::app_of;
    parent == own || app(&parent).is_some_and(|parent| app(&own) == Some(parent))
}

/// The daemon a test named in place of launchd's, by its PID.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn test_daemon() -> Option<u32> {
    std::env::var(TEST_DAEMON).ok()?.parse().ok()
}

/// Who made the socket that is standard input, when it is one.
pub(crate) fn handed() -> Option<Peer> {
    let input = std::io::stdin();
    let input = input.as_fd();
    let pid = nix::sys::socket::getsockopt(&input, nix::sys::socket::sockopt::LocalPeerPid).ok()?;
    let (uid, _) = nix::unistd::getpeereid(input).ok()?;
    Some(Peer {
        pid: u32::try_from(pid).ok()?,
        uid: uid.as_raw(),
    })
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
