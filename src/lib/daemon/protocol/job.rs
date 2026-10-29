//! What is asked of the helper's background jobs, and what the daemon says
//! back: each job as a reply shows it, why a request was refused, and the
//! reply to each action. Both sides of the socket read and write these
//! types, so a field or reason is named once.
use super::{SessionPin, failure};
use crate::{
    error::Code,
    host_session::Key,
    name::{lower_hex, name_type, random_hex},
    process::Start,
};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::time::Duration;

name_type! {
    /// A background job's handle: 12 lowercase hexadecimal digits, drawn at
    /// random when it starts, so it says nothing of the job.
    #[derive(Hash, PartialOrd, Ord)]
    pub struct JobId;
    refused(text) = format!("{text} is not a job's ID");
    display
}

impl JobId {
    /// How many hexadecimal digits a job's ID has.
    pub const LEN: usize = 12;

    /// A new random ID.
    pub fn new() -> Self {
        Self(random_hex(Self::LEN))
    }

    /// `text` as a job's ID, when it is exactly [`JobId::LEN`] lowercase
    /// hexadecimal digits.
    pub fn parse(text: &str) -> Option<Self> {
        lower_hex(text, Self::LEN).then(|| Self(text.to_owned()))
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

/// What the person's side knows a job to be: the boot it started in, its
/// leader's PID and exact start time, and its process group. A record a
/// later build wrote may hold more beside it; a request names exactly
/// these.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Expected {
    pub boot: String,
    pub pid: i32,
    pub start: Start,
    pub pgid: i32,
}

/// Why every job is being stopped at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StopReason {
    /// The person stopped agents.
    Person,
    /// The helper came on screen.
    OnConsole,
    /// Atlas is logging the helper out or being uninstalled.
    Stopped,
}

/// What a job start asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobStart {
    pub argv: Vec<String>,
    pub lifetime: Duration,
    pub name: Option<String>,
    pub task: Option<Key>,
    pub session: Option<Key>,
}

/// What is asked of the helper's background jobs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobRequest {
    /// Start a job for the helper account `uid`: its program runs for at
    /// most the start's lifetime, for its task of its host session.
    Start {
        start: JobStart,
        uid: u32,
        pin: Option<SessionPin>,
    },
    /// Stop job `id` when it is still what `expect` says, waiting `grace`
    /// between TERM and KILL.
    Stop {
        id: JobId,
        expect: Expected,
        grace: Duration,
    },
    /// Stop every job, by the daemon's own records alone.
    StopAll { reason: StopReason },
    /// One job, or every one.
    Show { id: Option<JobId> },
    /// The last `bytes` of a job's output.
    Log { id: JobId, bytes: usize },
}

/// How far a job got.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    /// Written before anything is spawned: no process yet.
    Starting,
    /// Its launcher runs, with its PID and start time, but it has not yet
    /// said it is a session of its own.
    Launched,
    Running,
    /// Told to end: TERM, then KILL after the grace.
    Stopping,
    Ended,
}

impl JobState {
    /// Whether a job in this state may still have processes.
    pub fn live(self) -> bool {
        matches!(self, Self::Launched | Self::Running | Self::Stopping)
    }
}

/// How a job ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum How {
    /// It ended by itself.
    Exited,
    /// An agent stopped it.
    Stopped,
    /// It ran past its time limit.
    Timeout,
    /// The person stopped agents.
    Person,
    /// The helper came on screen.
    OnConsole,
    /// It was found ended with nothing to say why: the daemon was down, or
    /// its PID is another process's now.
    Gone,
    /// It started in another boot.
    Restarted,
    /// Its leader is gone while processes of its group remain: they are
    /// listed in `left` and never signalled, since without its leader
    /// nothing proves the group is still the job's.
    LeaderGone,
}

impl From<StopReason> for How {
    fn from(reason: StopReason) -> Self {
        match reason {
            StopReason::Person => Self::Person,
            StopReason::OnConsole => Self::OnConsole,
            StopReason::Stopped => Self::Stopped,
        }
    }
}

/// A process that left a job's group, or that was in it when its leader
/// went, and still runs: reported, never signalled.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Left {
    pub pid: i32,
    pub name: String,
}

/// Why a job request was refused, or why nothing could be sent to a job.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JobRefusal {
    /// No record names the job, its record cannot be read, or it has not
    /// finished starting.
    Unknown,
    /// The job is no longer the process Atlas started.
    Identity,
    /// Its processes could not be read.
    Unobserved,
    /// Part of the job belongs to another account.
    OtherAccount,
    /// As many jobs run as may.
    Limit,
    /// It may run, but its start could not be confirmed.
    Unconfirmed,
    /// It belongs to another task, which was not named.
    OtherTask,
    /// It was told to stop and has not ended yet.
    StillStopping,
}

/// A job as replies show it. The arguments it was started with are never
/// shown: they may carry secrets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobView {
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
    /// How often its log was emptied for growing past its limit.
    pub log_resets: u32,
    /// Found again by a daemon that did not start it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub adopted: bool,
    #[serde(flatten)]
    pub progress: Progress,
}

impl JobView {
    /// What the job is, when it has a process: what a stop is checked
    /// against.
    pub fn expected(&self) -> Option<Expected> {
        Some(Expected {
            boot: self.boot.clone(),
            pid: self.pid?,
            start: self.start?,
            pgid: self.pgid?,
        })
    }

    /// How it ended, once it did.
    pub fn ended(&self) -> Option<&EndedView> {
        match &self.progress {
            Progress::Ended(ended) => Some(ended),
            _ => None,
        }
    }
}

/// How a stop is going, or how the job ended: shown beside the job once
/// either is known.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum Progress {
    Ended(EndedView),
    Stopping(StoppingView),
    Neither {},
}

/// How a job ended, and what it left.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndedView {
    pub how: How,
    /// When, in milliseconds since 1970.
    pub ended_at: u64,
    /// The last signal Atlas sent it, if any.
    pub signal: Option<String>,
    /// Its leader's exit, when the daemon collected it.
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_signal: Option<String>,
    pub left: Vec<Left>,
    /// How many processes its leader left behind, for a job whose leader
    /// went while its group still ran.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining: Option<usize>,
}

/// A stop under way.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StoppingView {
    /// The last signal Atlas sent, if any.
    pub signal: Option<String>,
    /// How many of its processes outlived KILL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining: Option<usize>,
    /// Why no signal could be sent the last time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refused: Option<JobRefusal>,
}

/// Read leniently: a job that says how it ended has ended, one that says
/// how its stop goes is stopping, and any other has neither, whatever else
/// a reply leaves out.
impl<'de> Deserialize<'de> for Progress {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Fields {
            how: Option<How>,
            ended_at: Option<u64>,
            #[serde(default, deserialize_with = "present")]
            signal: Option<Option<String>>,
            exit_code: Option<i32>,
            exit_signal: Option<String>,
            #[serde(default)]
            left: Vec<Left>,
            remaining: Option<usize>,
            refused: Option<JobRefusal>,
        }

        /// A field that is there, null or not.
        fn present<'de, D: Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Option<Option<String>>, D::Error> {
            Option::deserialize(deserializer).map(Some)
        }

        let fields = Fields::deserialize(deserializer)?;
        Ok(match fields.how {
            Some(how) => Self::Ended(EndedView {
                how,
                ended_at: fields.ended_at.unwrap_or(0),
                signal: fields.signal.flatten(),
                exit_code: fields.exit_code,
                exit_signal: fields.exit_signal,
                left: fields.left,
                remaining: fields.remaining,
            }),
            None if fields.signal.is_some()
                || fields.remaining.is_some()
                || fields.refused.is_some() =>
            {
                Self::Stopping(StoppingView {
                    signal: fields.signal.flatten(),
                    remaining: fields.remaining,
                    refused: fields.refused,
                })
            }
            None => Self::Neither {},
        })
    }
}

/// A job whose record cannot be read, so whether it runs is not known.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unreadable {
    pub id: JobId,
    pub state: Unread,
    pub error: String,
}

/// The state an unreadable record's job is shown in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Unread {
    Unknown,
}

/// One job as the daemon shows it: its view, or that its record cannot be
/// read. A row of another shape, which a later build may send, is kept as
/// it came.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Shown {
    Job(Box<JobView>),
    Unreadable(Unreadable),
    Later(Value),
}

impl Shown {
    /// The job's view, when its record could be read.
    pub fn job(&self) -> Option<&JobView> {
        match self {
            Self::Job(job) => Some(job),
            _ => None,
        }
    }

    /// The job's ID, when the row names one.
    pub fn id(&self) -> Option<JobId> {
        match self {
            Self::Job(job) => Some(job.id.clone()),
            Self::Unreadable(unreadable) => Some(unreadable.id.clone()),
            Self::Later(row) => row["id"].as_str().and_then(JobId::parse),
        }
    }

    /// When the job started; 0 when the row does not say.
    pub fn started_at(&self) -> u64 {
        self.job().map_or(0, |job| job.started_at)
    }

    /// How far the job got, as far as the row says: none for a job whose
    /// record cannot be read.
    pub fn state(&self) -> Option<JobState> {
        match self {
            Self::Job(job) => Some(job.state),
            Self::Unreadable(_) => None,
            Self::Later(row) => JobState::deserialize(&row["state"]).ok(),
        }
    }

    /// Whether the job's record cannot be read, so whether it runs is not
    /// known.
    pub fn unreadable(&self) -> bool {
        match self {
            Self::Job(_) => false,
            Self::Unreadable(_) => true,
            Self::Later(row) => row["state"] == "unknown",
        }
    }
}

/// The daemon's reply to a job request it carried out, by its action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum JobReply {
    Start { job: JobView },
    Stop(Stopped),
    StopAll(StoppedAll),
    Show(Showing),
    Log(Logged),
}

impl JobReply {
    /// The reply as it goes on the wire, with `ok` and `op` beside it.
    pub fn into_value(self) -> Value {
        match serde_json::to_value(self) {
            Ok(Value::Object(mut reply)) => {
                reply.insert("ok".into(), Value::Bool(true));
                reply.insert("op".into(), Value::from("job"));
                Value::Object(reply)
            }
            _ => failure(
                Code::Job,
                "The helper could not write its reply about its jobs",
            ),
        }
    }
}

/// The reply to a stop: the job, and what the stop came to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stopped {
    pub job: JobView,
    #[serde(flatten)]
    pub came_to: StopCameTo,
}

/// What a stop of one job came to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StopCameTo {
    /// The signal sent to its group.
    Signalled(String),
    /// It was already being stopped.
    AlreadyStopping(bool),
    /// It had already ended.
    AlreadyEnded(bool),
}

/// The reply to a stop of every job: how many are stopping and ended, and
/// which could not be signalled or read.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StoppedAll {
    pub stopping: u64,
    pub ended: u64,
    /// Those nothing could be sent to, and why.
    pub refused: Vec<RefusedJob>,
    /// Those whose processes could not be read.
    pub unknown: Vec<NamedJob>,
    /// Those whose records could not be read.
    pub unresolved: Vec<NamedJob>,
}

/// A job nothing could be sent to, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefusedJob {
    pub id: JobId,
    pub reason: JobRefusal,
}

/// A job named by its ID alone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedJob {
    pub id: JobId,
}

/// The reply to a show: one job, or every one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Showing {
    Every { jobs: Vec<Shown> },
    One { job: Option<Shown> },
}

/// The end of a job's output.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Logged {
    pub id: JobId,
    /// The task the job belongs to.
    pub task: Option<Key>,
    /// The output's last bytes, as base64.
    pub data: String,
    /// How long the log is now.
    pub size: u64,
    pub output_truncated: bool,
    pub log_resets: u32,
}

/// What the daemon's hello says of its jobs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Census {
    /// How many may run.
    pub running: usize,
    /// How many a start is launching.
    pub launching: usize,
    /// How many records cannot be read, whose jobs may still run too.
    pub unresolved: usize,
}

/// The daemon's refusal of a job request: why, and which job, with the
/// job as the daemon keeps it when its start could not be confirmed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobFailure {
    pub error: String,
    pub reason: JobRefusal,
    /// Whether the job may run anyway.
    pub uncertain: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<JobId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<JobView>,
}

impl JobFailure {
    /// A refusal for `reason`: never of unknown outcome.
    pub fn new(reason: JobRefusal, error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            reason,
            uncertain: false,
            id: None,
            job: None,
        }
    }

    /// The refusal, naming job `id`.
    #[must_use]
    pub fn about(self, id: &JobId) -> Self {
        Self {
            id: Some(id.clone()),
            ..self
        }
    }

    /// The refusal as it goes on the wire, in the shape every Atlas
    /// failure has.
    pub fn into_value(self) -> Value {
        let mut reply = failure(Code::Job, "");
        if let (Ok(Value::Object(fields)), Some(reply)) =
            (serde_json::to_value(self), reply.as_object_mut())
        {
            reply.extend(fields);
        }
        reply
    }
}
