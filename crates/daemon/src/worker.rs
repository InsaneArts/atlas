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

/// The exit code of an environment worker that refused before it did
/// anything, its refusal its only output.
pub const REFUSED: u8 = 125;

/// The name a machine guest's environment worker gives its refusal.
pub const MACHINE_REFUSAL: &str = "machine";

/// The longest answer a machine guest's environment worker may give, its
/// line's end included: kept whole as what its change came to.
pub const ANSWER: usize = 256 * 1024;
const _: () = assert!(ANSWER <= crate::machine::changes::OUTPUT);

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

/// Where a machine guest's environment worker finds itself.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Handing {
    /// This process's account.
    uid: u32,
    /// The process that started it.
    parent: u32,
    /// Who made the socket that is its standard input, when it is one.
    handed: Option<Peer>,
    /// Its parent's account and parent, as the kernel says now; none when
    /// it would not say.
    supervisor: Option<(u32, u32)>,
    /// Whether its parent runs this very Atlas.
    same_atlas: bool,
}

impl Handing {
    /// Where this process is now.
    fn now() -> Self {
        let parent = std::os::unix::process::parent_id();
        let supervisor = i32::try_from(parent)
            .ok()
            .and_then(|pid| atlas_core::process::info(pid).ok().flatten())
            .filter(|info| !info.zombie)
            .and_then(|info| Some((info.uid, u32::try_from(info.ppid).ok()?)));
        let program = atlas_core::app_bundle::signature::executable_of(parent);
        let own = std::env::current_exe().ok();
        Self {
            uid: nix::unistd::getuid().as_raw(),
            parent,
            handed: handed(),
            supervisor,
            same_atlas: matches!((program, own), (Some(program), Some(own)) if same_atlas(&program, &own)),
        }
    }
}

/// The daemon a machine guest's environment worker was started for: its
/// parent's parent, once its standard input is a socket its parent made in
/// its own account and its parent runs this very Atlas. Whether that daemon
/// is the one launchd runs is proven apart.
///
/// # Errors
///
/// Returns the one refusal.
fn judge_environment_worker(place: &Handing) -> Result<u32> {
    allowed(handed_by(place.handed, place.parent, place.uid) && place.same_atlas)?;
    match place.supervisor {
        Some((uid, daemon)) if uid == place.uid && daemon > 1 => Ok(daemon),
        _ => Err(refusal()),
    }
}

/// Refuse unless this process is a machine guest's environment worker as
/// its daemon starts one: handed its packet by a supervisor of this very
/// Atlas, on a socket that supervisor made, the supervisor the child of
/// the daemon launchd runs. The daemon, by its PID, read once: what the
/// packet must name as the session's.
///
/// # Errors
///
/// Returns the one refusal.
pub fn machine_environment_worker() -> Result<u32> {
    let daemon = judge_environment_worker(&Handing::now())?;
    #[cfg(any(test, feature = "test-support"))]
    if test_daemon() == Some(daemon) {
        return Ok(daemon);
    }
    crate::machine::launch::prove_pid_now(daemon).map_err(|error| refusal().with_source(error))?;
    Ok(daemon)
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
    crate::machine::launch::prove_pid_now(parent).map_err(|error| refusal().with_source(error))
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

/// Refuse unless this process is the very one launchd runs now as the job
/// `label` in this account's own session: what a long-lived worker of the
/// station's, started by its own agent and by nothing else, requires.
///
/// # Errors
///
/// Returns the one refusal.
pub fn launchd_job(label: &str) -> Result<()> {
    if HAND_RUN {
        return Ok(());
    }
    judge_job(std::process::id(), job_pid(label))
}

/// [`launchd_job`]'s decision: `own` this process's PID, `launchd` the one
/// launchd runs as the job.
fn judge_job(own: u32, launchd: Option<u32>) -> Result<()> {
    allowed(launchd == Some(own))
}

/// The process launchd runs as the job `label` in this account's session;
/// none when it runs none, has no such job or cannot be asked in time. In a
/// test's Atlas, the one the test named stands for it.
fn job_pid(label: &str) -> Option<u32> {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(pid) = test_daemon() {
        return Some(pid);
    }
    let domain = format!("gui/{}", nix::unistd::getuid().as_raw());
    match atlas_core::launchd::job(&domain, label).ok()? {
        atlas_core::launchd::Job::Loaded(loaded) => loaded.pid,
        atlas_core::launchd::Job::Absent => None,
    }
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
