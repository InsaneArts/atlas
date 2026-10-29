//! What the daemon keeps of one job: its validated identity, how far it
//! got, how a stop is going and how it ended. The arguments it was started
//! with are never kept: they may carry secrets.
use crate::{
    daemon::protocol::{
        EndedView, Expected, How, JobId, JobRefusal, JobState, JobView, Left, Progress,
        StoppingView,
    },
    host_session::Key,
    process::{Instance, Start},
};
use serde::{Deserialize, Serialize};

/// The record format this build writes.
pub(super) const VERSION: u32 = 1;
/// The highest process ID macOS hands out.
const PID_MAX: i32 = 99_999;
/// The longest boot session identity accepted.
const BOOT_MAX: usize = 64;

/// What a job is: the boot it started in, its leader's PID and exact start
/// time, and its process group, which is the leader's PID. Only
/// [`JobIdentity::new`] makes one, so every identity in hand is in bounds
/// and names a group led by its own leader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobIdentity {
    boot: String,
    leader: Instance,
}

impl JobIdentity {
    /// The identity these say, when the boot is named, the PID is a user
    /// process's, the start time is a real one, and the group is the
    /// leader's own.
    pub fn new(boot: &str, pid: i32, start: Start, pgid: i32) -> Option<Self> {
        let sound = !boot.is_empty()
            && boot.len() <= BOOT_MAX
            && (2..=PID_MAX).contains(&pid)
            && pgid == pid
            && start[0] > 0
            && start[1] < 1_000_000;
        sound.then(|| Self {
            boot: boot.to_owned(),
            leader: Instance::new(pid, start),
        })
    }

    /// The identity the person's side sent, when it is a sound one.
    pub fn expected(expected: &Expected) -> Option<Self> {
        Self::new(&expected.boot, expected.pid, expected.start, expected.pgid)
    }

    pub fn boot(&self) -> &str {
        &self.boot
    }

    /// The leader's PID.
    pub fn pid(&self) -> i32 {
        self.leader.pid
    }

    /// The job's process group: always its leader's PID.
    pub fn pgid(&self) -> i32 {
        self.leader.pid
    }

    pub fn start(&self) -> Start {
        self.leader.start
    }

    /// The leader as a process instance.
    pub fn leader(&self) -> Instance {
        self.leader
    }

    /// As the person's side keeps it.
    pub fn as_expected(&self) -> Expected {
        Expected {
            boot: self.boot.clone(),
            pid: self.leader.pid,
            start: self.leader.start,
            pgid: self.leader.pid,
        }
    }
}

/// A stop under way.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Stopping {
    /// How the job ends once nothing of it is left.
    pub why: How,
    /// When the stop began, in milliseconds since 1970: the grace runs from
    /// the TERM, which is sent at once when the job's identity holds.
    pub since: u64,
    pub grace_ms: u64,
    /// Whether TERM reached its group.
    #[serde(default)]
    pub term_sent: bool,
    /// The last signal Atlas sent, if any.
    #[serde(default)]
    pub signal: Option<String>,
    /// When KILL was first sent, and last.
    #[serde(default)]
    pub killed: Option<u64>,
    #[serde(default)]
    pub killed_last: Option<u64>,
    /// How many of its processes outlived KILL.
    #[serde(default)]
    pub stuck: Option<usize>,
    /// Why no signal could be sent the last time: its processes no longer
    /// match it, or could not be read.
    #[serde(default)]
    pub refused: Option<JobRefusal>,
}

impl Stopping {
    /// A stop for `why` beginning `now`, with `grace` between TERM and KILL.
    pub(super) fn new(why: How, now: u64, grace_ms: u64) -> Self {
        Self {
            why,
            since: now,
            grace_ms,
            term_sent: false,
            signal: None,
            killed: None,
            killed_last: None,
            stuck: None,
            refused: None,
        }
    }
}

/// How a job ended, and what it left.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Ended {
    pub how: How,
    /// When, in milliseconds since 1970.
    pub at: u64,
    /// The last signal Atlas sent it, if any.
    #[serde(default)]
    pub signal: Option<String>,
    /// Its leader's exit, when the daemon collected it.
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub exit_signal: Option<String>,
    #[serde(default)]
    pub left: Vec<Left>,
}

/// The daemon's record of one job: the truth about its process.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Record {
    pub version: u32,
    pub id: JobId,
    /// The agent's name for it, or its program's file name.
    pub name: String,
    pub task: Option<Key>,
    pub session: Option<Key>,
    /// The helper account it runs as.
    pub uid: u32,
    pub boot: String,
    pub pid: Option<i32>,
    pub start: Option<Start>,
    pub pgid: Option<i32>,
    pub state: JobState,
    pub started_at: u64,
    pub ends_at: u64,
    /// Found again by a daemon that did not start it.
    #[serde(default)]
    pub adopted: bool,
    /// Its descendants seen while it ran, by PID and start time: only to
    /// say which left its group, never to decide a signal.
    #[serde(default)]
    pub seen: Vec<Instance>,
    #[serde(default)]
    pub stopping: Option<Stopping>,
    #[serde(default)]
    pub ended: Option<Ended>,
    /// How often its log was emptied for growing past its limit.
    #[serde(default)]
    pub log_resets: u32,
}

impl Record {
    /// Its identity, when it has a process and the identity is sound.
    pub fn identity(&self) -> Option<JobIdentity> {
        JobIdentity::new(&self.boot, self.pid?, self.start?, self.pgid?)
    }

    /// Whether this record may be acted on: it is of this build's format,
    /// and once it names a process, that process's identity is sound. A
    /// record that fails this is shown and never acted on.
    pub(super) fn sound(&self) -> bool {
        let process = self.pid.is_some() || self.start.is_some() || self.pgid.is_some();
        let needs_process = matches!(
            self.state,
            JobState::Launched | JobState::Running | JobState::Stopping
        );
        self.version == VERSION
            && self.state != JobState::Starting
            && (!(process || needs_process) || self.identity().is_some())
    }

    /// What the job is, when it has a process.
    pub fn expected(&self) -> Option<Expected> {
        self.identity().map(|identity| identity.as_expected())
    }

    /// Whether it may still have processes.
    pub(super) fn live(&self) -> bool {
        self.state.live()
    }

    /// The job as replies show it.
    pub fn view(&self) -> JobView {
        let progress = match (&self.ended, &self.stopping) {
            (Some(ended), _) => Progress::Ended(EndedView {
                how: ended.how,
                ended_at: ended.at,
                signal: ended.signal.clone(),
                exit_code: ended.exit_code,
                exit_signal: ended.exit_signal.clone(),
                left: ended.left.clone(),
                remaining: (ended.how == How::LeaderGone).then_some(ended.left.len()),
            }),
            (None, Some(stopping)) if self.state == JobState::Stopping => {
                Progress::Stopping(StoppingView {
                    signal: stopping.signal.clone(),
                    remaining: stopping.stuck,
                    refused: stopping.refused,
                })
            }
            (None, _) => Progress::Neither {},
        };
        JobView {
            id: self.id.clone(),
            name: self.name.clone(),
            task: self.task.clone(),
            session: self.session.clone(),
            uid: self.uid,
            boot: self.boot.clone(),
            pid: self.pid,
            start: self.start,
            pgid: self.pgid,
            state: self.state,
            started_at: self.started_at,
            ends_at: self.ends_at,
            log_resets: self.log_resets,
            adopted: self.adopted,
            progress,
        }
    }
}
