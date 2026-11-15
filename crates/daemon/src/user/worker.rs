//! The helper daemon's own workers, told apart from anything an agent runs.
//! The helper daemon is the job launchd runs from the helper's own
//! LaunchAgent in that account's session; the account that runs the station
//! has no such job, and nothing an agent can ask of Atlas gives it one. Its
//! workers are its children, and a job's launcher takes its word to go on
//! only from a socket the daemon made and handed it as its standard input,
//! never from a pipe anyone could fill.
use crate::worker::{Peer, allowed, handed, handed_by};
use atlas_core::error::Result;

/// Where a worker finds itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Place {
    /// This process's account.
    uid: u32,
    /// The process that started it.
    parent: u32,
    /// The process launchd runs as the helper daemon's job in this account's
    /// own session, when there is one.
    daemon: Option<u32>,
    /// Who made the socket that is this process's standard input, when it is
    /// one.
    handed: Option<Peer>,
}

/// Refuse unless this process's parent is the helper daemon launchd runs for
/// its account.
///
/// # Errors
///
/// Returns the one refusal.
fn judge_worker(place: &Place) -> Result<()> {
    allowed(place.daemon == Some(place.parent))
}

/// Refuse unless this process is the helper daemon's own child, handed its
/// standard input by that daemon on a socket it made.
///
/// # Errors
///
/// Returns the one refusal.
fn judge_job_launcher(place: &Place) -> Result<()> {
    judge_worker(place)?;
    allowed(handed_by(place.handed, place.parent, place.uid))
}

/// Refuse unless this process is the helper daemon's child: what a worker it
/// runs, as the environment's, requires.
///
/// # Errors
///
/// Returns the one refusal.
pub fn worker() -> Result<()> {
    judge_worker(&Place::now())
}

/// Refuse unless this process is a job's launcher as the helper daemon
/// starts one: its child, its standard input a socket the daemon made.
///
/// # Errors
///
/// Returns the one refusal.
pub fn job_launcher() -> Result<()> {
    judge_job_launcher(&Place::now())
}

/// Refuse unless this process is the helper daemon launchd runs: serving as
/// its own account (`service_uid` its own uid) for another (`controller_uid`
/// not its own), and the very process launchd names as the helper's job. A
/// hand-started copy, which could name another account as its controller,
/// never serves.
///
/// # Errors
///
/// Returns the one refusal.
pub fn serving(service_uid: u32, controller_uid: u32) -> Result<()> {
    judge_serving(
        &Place::now(),
        std::process::id(),
        service_uid,
        controller_uid,
    )
}

/// [`serving`]'s decision: `place` is where this process is, `own` its PID.
fn judge_serving(place: &Place, own: u32, service_uid: u32, controller_uid: u32) -> Result<()> {
    allowed(service_uid == place.uid && controller_uid != place.uid && place.daemon == Some(own))
}

impl Place {
    /// Where this process is now.
    fn now() -> Self {
        let uid = nix::unistd::getuid().as_raw();
        Self {
            uid,
            parent: std::os::unix::process::parent_id(),
            daemon: daemon_pid(uid),
            handed: handed(),
        }
    }
}

/// The process launchd runs as the helper daemon's job in `uid`'s session,
/// as it says now; none when it has no such job, runs none, or cannot be
/// asked in time.
fn daemon_pid(uid: u32) -> Option<u32> {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(pid) = crate::worker::test_daemon() {
        return Some(pid);
    }
    helper_job(uid)?.pid
}

/// Whether launchd has the helper daemon's job loaded in `uid`'s session:
/// true only in a helper's own account, never in the person's.
fn helper_job_loaded(uid: u32) -> bool {
    #[cfg(any(test, feature = "test-support"))]
    if crate::worker::test_daemon().is_some() {
        return true;
    }
    helper_job(uid).is_some()
}

/// The helper daemon's job in `uid`'s session as launchd prints it; none
/// when it has no such job or cannot be asked in time.
fn helper_job(uid: u32) -> Option<atlas_core::launchd::Loaded> {
    let output = atlas_core::bounded::output(
        std::process::Command::new("/bin/launchctl")
            .args(["print", &format!("gui/{uid}/{}", super::setup::LABEL)])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null()),
        atlas_core::bounded::LOOKUP,
    )
    .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(atlas_core::launchd::Loaded::parse(
        &String::from_utf8_lossy(&output.stdout),
    ))
}

/// Refuse unless this process is an input service in a helper's own account:
/// the helper daemon's job is loaded in this account's session (the person's
/// has none), it serves as its own account (`service_uid` its own uid), for
/// another (`controller_uid` not its own). It is started from the helper's
/// own shell, not by launchd, so only the account is proven, not the
/// process.
///
/// # Errors
///
/// Returns the one refusal.
pub fn input_service(service_uid: u32, controller_uid: u32) -> Result<()> {
    let me = nix::unistd::getuid().as_raw();
    judge_input_service(me, helper_job_loaded(me), service_uid, controller_uid)
}

/// [`input_service`]'s decision: `me` this process's account, `loaded`
/// whether the helper daemon's job is loaded in its session.
fn judge_input_service(me: u32, loaded: bool, service_uid: u32, controller_uid: u32) -> Result<()> {
    allowed(loaded && service_uid == me && controller_uid != me)
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
