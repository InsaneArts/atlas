//! Apple jobs that keep restarting in the helper: what doctor and the
//! overview report of them. Reading them from launchd's log on this Mac is
//! the user desktop's, in [`crate::user::console_agents`].
use serde::Serialize;
use serde_json::Value;

/// The agents whose restarts in the helper slow the Mac: each start opens
/// a window server connection, which the window server keeps when the agent
/// has quit before it took the connection in.
const SLOWING: &[&str] = &[
    // audioaccessoryd: "Exiting: Current console ID: …, getuid: …".
    "com.apple.BTServer.cloudpairing",
];

/// Whether an agent that slows the Mac restarted often in the helper in
/// the last hour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Restarting {
    /// None restarted often in the last hour.
    #[serde(rename = "none")]
    Quiet,
    /// One restarted often in the last hour: before it was paused, or
    /// because it could not be.
    Restarting,
    /// launchd's log could not be read.
    Unknown,
}

impl Restarting {
    /// From the jobs restarting in the helper, or `None` when launchd's log
    /// could not be read.
    pub fn of(jobs: Option<&[Restart]>) -> Self {
        match jobs {
            None => Self::Unknown,
            Some(jobs) if jobs.iter().any(|job| SLOWING.contains(&job.label.as_str())) => {
                Self::Restarting
            }
            Some(_) => Self::Quiet,
        }
    }
}

/// An Apple job in the helper that keeps quitting at once and restarting.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Restart {
    pub label: String,
    /// Its short runs in the last hour of the log.
    pub runs: usize,
}

/// Whether a status reply is the helper's own answer, so it is logged in
/// and what launchd's log says of its jobs is about this login.
pub fn answered(reply: &Value) -> bool {
    reply["ok"] == true
}

#[cfg(test)]
#[path = "console_agents_tests.rs"]
mod tests;
