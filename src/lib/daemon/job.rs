//! Background jobs: programs an agent keeps running in the helper after the
//! command that started them returns, each in a session and process group
//! of its own, supervised by the daemon and stopped only by their identity.
//!
//! A job is known by the boot it started in, its leader's PID and exact
//! start time, and its process group, which is the leader's PID: a session
//! leader can never move to another group. Only that group is ever
//! signalled, and only once the daemon's record, the person's side's copy
//! of it and the live processes agree, checked again right before each
//! signal. Nothing is ever found or signalled by name.
//!
//! The daemon's records live beside its socket, one file a job, written
//! whole at every change of state, and the job's output goes to a log
//! beside it. A daemon that restarts finds its jobs again by those
//! records: a job outlives its supervisor, because launchd ends only the
//! daemon's own process group when the daemon exits.
use super::{
    protocol::{Expected, JobId, JobRequest, StopReason, failure},
    run,
};
use crate::{
    control::Durability,
    error::Result,
    host_session::{Key, now_ms},
    process::{self, Info, Instance},
    record_folder::{LISTED, RecordFolder, Records},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use nix::{
    errno::Errno,
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom},
    os::unix::{fs::OpenOptionsExt, process::ExitStatusExt},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};
use tokio::{io::AsyncReadExt, process::Child};

/// The record format this build writes.
const VERSION: u32 = 1;
/// The most jobs running in the helper at once.
const RUNNING_MAX: usize = 16;
/// The most jobs one task keeps running at once.
const TASK_MAX: usize = 8;
/// The most descendants a job keeps note of.
const SEEN_MAX: usize = 256;
/// The most records kept; the oldest ended ones go first.
const RECORDS_MAX: usize = 64;
/// How long an ended job's record and log are kept.
const KEEP: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// How often old records are pruned.
const PRUNE_EVERY: Duration = Duration::from_secs(60 * 60);
/// A log past this is emptied, and its job writes on from the start.
const LOG_LIMIT: u64 = 32 * 1024 * 1024;
/// How long the launcher has to say it is a session of its own and exec.
const LAUNCH: Duration = Duration::from_secs(5);
/// How long a launch that failed waits to collect its launcher.
const REAP: Duration = Duration::from_secs(2);
/// How long KILL has, once sent, before a job is said to be stuck.
const KILLED: Duration = Duration::from_secs(2);
/// How often KILL is sent again to a stuck job, and for how long.
const KILL_AGAIN: Duration = Duration::from_secs(5);
const KILL_FOR: Duration = Duration::from_secs(60);
/// How often jobs are looked at while one is stopping, and otherwise.
const WATCH_STOPPING: Duration = Duration::from_millis(200);
const WATCH: Duration = Duration::from_secs(1);
/// What the daemon says of a record it cannot read.
const UNREADABLE: &str = "Atlas cannot tell whether this job's process is running";

/// How job records are kept: written whole and ordered on the disk before
/// the daemon goes on, a power cut leaving each whole or absent.
static JOB_RECORDS: Records = Records {
    folder: "jobs",
    noun: "job",
    lock_wait: Duration::from_secs(2),
    durability: Durability::Kernel,
    bytes: 128 * 1024,
};

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
    /// Told to end: TERM was sent, KILL follows after the grace.
    Stopping,
    Ended,
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
}

impl How {
    fn of(reason: StopReason) -> Self {
        match reason {
            StopReason::Person => Self::Person,
            StopReason::OnConsole => Self::OnConsole,
            StopReason::Stopped => Self::Stopped,
        }
    }
}

/// A process that left a job's group and still runs: reported, never
/// signalled.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Left {
    pub pid: i32,
    pub name: String,
}

/// A stop under way.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Stopping {
    /// How the job ends once nothing of it is left.
    pub why: How,
    /// When TERM was sent, in milliseconds since 1970.
    pub since: u64,
    pub grace_ms: u64,
    /// The last signal Atlas sent.
    pub signal: String,
    /// When KILL was first sent, and last.
    #[serde(default)]
    pub killed: Option<u64>,
    #[serde(default)]
    pub killed_last: Option<u64>,
    /// How many of its processes outlived KILL.
    #[serde(default)]
    pub stuck: Option<usize>,
    /// Why nothing more was signalled: its processes no longer match it.
    #[serde(default)]
    pub refused: Option<String>,
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
    /// The agent's name for it, or its program's file name; never its
    /// arguments, which may carry secrets.
    pub name: String,
    pub task: Option<Key>,
    pub session: Option<Key>,
    /// The helper account it runs as.
    pub uid: u32,
    pub boot: String,
    pub pid: Option<i32>,
    pub start: Option<[u64; 2]>,
    pub pgid: Option<i32>,
    pub state: JobState,
    pub started_at: u64,
    pub ends_at: u64,
    /// Found again by a daemon that did not start it.
    #[serde(default)]
    pub adopted: bool,
    /// Its descendants seen while it ran, by PID and start time.
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
    /// What the job is, when it has a process.
    pub fn expected(&self) -> Option<Expected> {
        Some(Expected {
            boot: self.boot.clone(),
            pid: self.pid?,
            start: self.start?,
            pgid: self.pgid?,
        })
    }

    /// Whether it may still have processes.
    fn live(&self) -> bool {
        matches!(
            self.state,
            JobState::Launched | JobState::Running | JobState::Stopping
        )
    }

    /// The job as replies show it.
    pub fn view(&self) -> Value {
        let mut view = json!({
            "id": self.id,
            "name": self.name,
            "task": self.task,
            "session": self.session,
            "uid": self.uid,
            "boot": self.boot,
            "pid": self.pid,
            "start": self.start,
            "pgid": self.pgid,
            "state": self.state,
            "startedAt": self.started_at,
            "endsAt": self.ends_at,
            "logResets": self.log_resets,
        });
        if self.adopted {
            view["adopted"] = json!(true);
        }
        if let Some(stopping) = &self.stopping
            && self.state == JobState::Stopping
        {
            view["signal"] = json!(stopping.signal);
            if let Some(stuck) = stopping.stuck {
                view["remaining"] = json!(stuck);
            }
            if let Some(refused) = &stopping.refused {
                view["refused"] = json!(refused);
            }
        }
        if let Some(ended) = &self.ended {
            view["how"] = json!(ended.how);
            view["endedAt"] = json!(ended.at);
            view["signal"] = json!(ended.signal);
            view["exitCode"] = json!(ended.exit_code);
            if let Some(signal) = &ended.exit_signal {
                view["exitSignal"] = json!(signal);
            }
            view["left"] = json!(ended.left);
        }
        view
    }
}

/// What may be done to a job's process group now, as its records and the
/// live processes say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// It is the job's group, and something of it runs: this group, and
    /// nothing else, may be signalled.
    Signal { pgid: i32 },
    /// Nothing of the job runs any more.
    Ended,
    /// The leader's PID is another process now: the job ended, and its PID
    /// was reused.
    Reused,
    /// It started in another boot.
    Restarted,
    /// The records disagree, or the group holds a process older than the
    /// job or the leader is in another group: nothing is signalled.
    Identity,
    /// It, or a process in its group, runs as another account: nothing is
    /// signalled.
    OtherAccount,
}

/// What may be done to the job `record` names, as `expect` (the person's
/// side's copy, when there is one) and the live processes say: `leader`,
/// the process with the leader's PID now, and `members`, its group's
/// processes, read just before. Checked in order: the account, the
/// person's copy, the boot, the leader, every member.
pub fn verdict(
    record: &Record,
    expect: Option<&Expected>,
    boot: &str,
    uid: u32,
    leader: Option<&Info>,
    members: &[Info],
) -> Verdict {
    if record.uid != uid {
        return Verdict::OtherAccount;
    }
    let Some(known) = record.expected() else {
        return Verdict::Identity;
    };
    if expect.is_some_and(|expect| *expect != known) {
        return Verdict::Identity;
    }
    if known.boot != boot {
        return Verdict::Restarted;
    }
    if let Some(leader) = leader {
        if leader.instance.start != known.start {
            return Verdict::Reused;
        }
        if !leader.runs_as(uid) {
            return Verdict::OtherAccount;
        }
        if leader.pgid != known.pgid {
            return Verdict::Identity;
        }
    }

    let live: Vec<&Info> = members
        .iter()
        .filter(|member| member.pgid == known.pgid && !member.zombie)
        .collect();
    if live.iter().any(|member| !member.runs_as(uid)) {
        return Verdict::OtherAccount;
    }
    // Without its leader, a group of that ID is the job's only when each
    // of its processes was seen as the job's, or descends from one that
    // was: a group made after the PID was used again is not.
    if leader.is_none() && !live.is_empty() {
        let mut known_pids: Vec<i32> = live
            .iter()
            .filter(|member| record.seen.contains(&member.instance))
            .map(|member| member.instance.pid)
            .collect();
        loop {
            let more: Vec<i32> = live
                .iter()
                .filter(|member| {
                    !known_pids.contains(&member.instance.pid) && known_pids.contains(&member.ppid)
                })
                .map(|member| member.instance.pid)
                .collect();
            if more.is_empty() {
                break;
            }
            known_pids.extend(more);
        }
        if live
            .iter()
            .any(|member| !known_pids.contains(&member.instance.pid))
        {
            return Verdict::Identity;
        }
    }
    if live
        .iter()
        .any(|member| member.instance.start < known.start)
    {
        return Verdict::Identity;
    }
    if live.is_empty() {
        return Verdict::Ended;
    }
    Verdict::Signal { pgid: known.pgid }
}

/// Note in `seen` the descendants of `job` among `processes` (the helper
/// account's): every live member of its group, every process whose parent
/// is one of those or already seen and still the same process, and every
/// process in the job's session, which only the job's own processes can
/// join. Each started no earlier than the job. At most [`SEEN_MAX`] are
/// kept. Best effort: a process whose parent exited between two looks is
/// missed.
pub fn observe(
    seen: &mut Vec<Instance>,
    job: &Instance,
    members: &[Info],
    processes: &[Info],
    session_of: impl Fn(i32) -> Option<i32>,
) {
    let alive: HashMap<i32, &Info> = processes
        .iter()
        .map(|process| (process.instance.pid, process))
        .collect();
    let add = |seen: &mut Vec<Instance>, instance: Instance| {
        if seen.len() < SEEN_MAX && !seen.contains(&instance) && instance.start >= job.start {
            seen.push(instance);
            true
        } else {
            false
        }
    };
    for member in members.iter().filter(|member| !member.zombie) {
        add(seen, member.instance);
    }

    loop {
        let parents: Vec<i32> = seen
            .iter()
            .filter(|instance| {
                alive
                    .get(&instance.pid)
                    .is_some_and(|process| process.instance.start == instance.start)
            })
            .map(|instance| instance.pid)
            .collect();
        let mut grew = false;
        for process in processes.iter().filter(|process| !process.zombie) {
            let descends = parents.contains(&process.ppid)
                || (process.ppid != 1 && process.ppid == job.pid)
                || session_of(process.instance.pid) == Some(job.pid);
            if descends {
                grew |= add(seen, process.instance);
            }
        }
        if !grew || seen.len() >= SEEN_MAX {
            break;
        }
    }
}

/// The processes in `seen` that still run as the same process, as `uid`,
/// outside group `pgid`: they left the job's group.
pub fn left(seen: &[Instance], processes: &[Info], pgid: i32, uid: u32) -> Vec<Left> {
    seen.iter()
        .filter_map(|instance| {
            processes
                .iter()
                .find(|process| process.instance == *instance)
        })
        .filter(|process| !process.zombie && process.runs_as(uid) && process.pgid != pgid)
        .map(|process| Left {
            pid: process.instance.pid,
            name: process.name.clone(),
        })
        .collect()
}

/// A record file: one this build reads, or one it never acts on.
enum Found {
    Record(Box<Record>),
    Unreadable(JobId),
}

/// What a signal to a job's group came to.
#[derive(Debug, PartialEq, Eq)]
enum Delivered {
    Sent,
    /// None of its processes is left, or only its unreaped leader.
    Ended,
    /// The kernel refused, and something of another account is there.
    OtherAccount,
}

/// What the daemon holds of the jobs it supervises.
struct State {
    /// The leaders this daemon started, held unreaped until their group is
    /// empty so the group's ID stays theirs.
    children: HashMap<JobId, Child>,
    /// How many jobs may still have processes.
    running: usize,
    /// Whether one is stopping, so it is looked at more often.
    stopping: bool,
    pruned: Option<Instant>,
    /// The jobs a start is launching now, which no look touches yet.
    launching: std::collections::HashSet<JobId>,
    /// When every job was last stopped at once, and why: a job still
    /// launching then is stopped as soon as it runs.
    stopped_all: Option<(u64, How)>,
}

/// The helper's background jobs.
pub struct Jobs {
    folder: RecordFolder<JobId>,
    /// The program that becomes each job: this daemon's own executable.
    launcher: PathBuf,
    uid: u32,
    boot: String,
    log_limit: u64,
    state: Mutex<State>,
}

/// What a job start asks for.
pub struct Start {
    pub argv: Vec<String>,
    pub lifetime: Duration,
    pub name: Option<String>,
    pub task: Option<Key>,
    pub session: Option<Key>,
}

impl Jobs {
    /// The jobs whose records are in `daemon`'s `jobs` folder, for the
    /// helper account `uid`, started through `launcher`. Every record is
    /// read, and each job still running is found again by its identity.
    pub fn open(daemon: &Path, launcher: PathBuf, uid: u32) -> Result<Self> {
        Self::open_with(daemon, launcher, uid, LOG_LIMIT)
    }

    /// [`Jobs::open`], emptying a log past `log_limit` bytes.
    pub fn open_with(daemon: &Path, launcher: PathBuf, uid: u32, log_limit: u64) -> Result<Self> {
        let folder = RecordFolder::at(daemon, &JOB_RECORDS);
        folder.make()?;
        let jobs = Self {
            folder,
            launcher,
            uid,
            boot: process::boot()?,
            log_limit,
            state: Mutex::new(State {
                children: HashMap::new(),
                running: 0,
                stopping: false,
                pruned: None,
                launching: std::collections::HashSet::new(),
                stopped_all: None,
            }),
        };
        jobs.adopt();
        Ok(jobs)
    }

    /// The folder the records are in.
    pub fn folder(&self) -> &Path {
        self.folder.directory()
    }

    /// How many jobs may still have processes.
    pub fn running(&self) -> usize {
        self.state().running
    }

    /// Carry out `request`, the reply its connection gets. What reads the
    /// process table and writes records runs on a blocking thread, so the
    /// daemon's other requests never wait for it.
    pub async fn serve(self: &Arc<Self>, request: JobRequest) -> Value {
        let jobs = self.clone();
        let blocking = |answer: Box<dyn FnOnce(&Jobs) -> Value + Send>| async move {
            tokio::task::spawn_blocking(move || answer(&jobs))
                .await
                .unwrap_or_else(|_| failure("job", "The helper could not answer about its jobs"))
        };
        match request {
            JobRequest::Start {
                argv,
                lifetime,
                name,
                task,
                session,
                uid,
                ..
            } => {
                if uid != self.uid {
                    return failure(
                        "identity",
                        "This helper daemon runs as another account than the one asked for",
                    );
                }
                self.start(Start {
                    argv,
                    lifetime,
                    name,
                    task,
                    session,
                })
                .await
            }
            JobRequest::Stop { id, expect, grace } => {
                blocking(Box::new(move |jobs| jobs.stop(&id, &expect, grace))).await
            }
            JobRequest::StopAll { reason } => {
                blocking(Box::new(move |jobs| jobs.stop_all(reason))).await
            }
            JobRequest::Show { id } => blocking(Box::new(move |jobs| jobs.show(id.as_ref()))).await,
            JobRequest::Log { id, bytes } => {
                blocking(Box::new(move |jobs| jobs.log(&id, bytes))).await
            }
        }
    }

    /// Start a job: write its record, spawn the launcher, see it become a
    /// session and group of its own and exec the program, check its
    /// identity, and reply with it. Nothing of a launch that fails before
    /// its exec is left running.
    pub async fn start(&self, start: Start) -> Value {
        let Some(program) = start.argv.first() else {
            return failure("spawn-failed", "No command to run");
        };
        let home = run::home();
        let path = run::search_path(home.as_deref());
        let Some(executable) = run::resolve(program, Some(&path), home.as_deref()) else {
            return run::unresolved(program, home.as_deref());
        };
        let name = start.name.clone().unwrap_or_else(|| {
            Path::new(program)
                .file_name()
                .map(|name| name.to_string_lossy().chars().take(64).collect())
                .filter(|name: &String| !name.is_empty())
                .unwrap_or_else(|| "job".to_owned())
        });

        let now = now_ms();
        let lifetime = u64::try_from(start.lifetime.as_millis()).unwrap_or(u64::MAX);
        let mut record = Record {
            version: VERSION,
            id: JobId::new(),
            name,
            task: start.task.clone(),
            session: start.session.clone(),
            uid: self.uid,
            boot: self.boot.clone(),
            pid: None,
            start: None,
            pgid: None,
            state: JobState::Starting,
            started_at: now,
            ends_at: now.saturating_add(lifetime),
            adopted: false,
            seen: Vec::new(),
            stopping: None,
            ended: None,
            log_resets: 0,
        };

        {
            let state = self.state();
            let records = self.records();
            let running: Vec<&Record> = records
                .iter()
                .filter_map(|read| match read {
                    Found::Record(record) if record.live() => Some(record.as_ref()),
                    _ => None,
                })
                .collect();
            if running.len() >= RUNNING_MAX {
                return refusal(
                    "limit",
                    "16 background jobs already run in the helper; stop one first",
                );
            }
            if start.task.is_some()
                && running.iter().filter(|job| job.task == start.task).count() >= TASK_MAX
            {
                return refusal(
                    "limit",
                    "This task already runs 8 background jobs; stop one first",
                );
            }
            while self.folder.holds(&record.id) {
                record.id = JobId::new();
            }
            if let Err(error) = self.write(&record) {
                return failure(error.code, error.error);
            }
            let mut state = state;
            state.launching.insert(record.id.clone());
        }
        let launched = self
            .launch(
                record,
                &start,
                &executable,
                &path,
                home.as_deref(),
                lifetime,
            )
            .await;
        self.state().launching.remove(&launched.1);
        launched.0
    }

    /// [`Jobs::start`] once its record is written: launch it, and reply.
    /// Returns the job's ID beside the reply.
    async fn launch(
        &self,
        mut record: Record,
        start: &Start,
        executable: &Path,
        path: &str,
        home: Option<&Path>,
        lifetime: u64,
    ) -> (Value, JobId) {
        let id = record.id.clone();
        let program = &start.argv[0];

        let log = self.log_path(&record.id);
        let created = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&log);
        if let Err(error) = created {
            self.forget(&record.id);
            return (
                failure(
                    "spawn-failed",
                    format!("The job's log could not be made: {error}"),
                ),
                id,
            );
        }

        let mut command = tokio::process::Command::new(&self.launcher);
        command
            .arg("job-launch")
            .arg("--log")
            .arg(&log)
            .arg("--argv0")
            .arg(program)
            .arg("--")
            .arg(executable)
            .args(&start.argv[1..])
            .env("PATH", path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(false);
        if let Some(lang) = run::locale() {
            command.env("LANG", lang);
        }
        if let Some(home) = home {
            command.current_dir(home);
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                self.forget(&record.id);
                return (failure("spawn-failed", format!("{program}: {error}")), id);
            }
        };

        // Read while it is this daemon's unreaped child, so it is exact. The
        // launcher waits on its standard input until its record says so: a
        // daemon that ends before then leaves nothing running.
        let pid = child.id().and_then(|pid| i32::try_from(pid).ok());
        let started = pid.map(process::start);
        let (Some(pid), Some(Ok(started))) = (pid, started) else {
            let _ = child.start_kill();
            let _ = tokio::time::timeout(REAP, child.wait()).await;
            self.forget(&record.id);
            return (
                failure("spawn-failed", "The job's process could not be observed"),
                id,
            );
        };
        record.pid = Some(pid);
        record.start = Some(started);
        record.pgid = Some(pid);
        record.state = JobState::Launched;
        if let Err(error) = self.locked_write(&record) {
            let _ = child.start_kill();
            let _ = tokio::time::timeout(REAP, child.wait()).await;
            self.forget(&record.id);
            return (failure(error.code, error.error), id);
        }
        let go = match child.stdin.take() {
            Some(mut go) => tokio::io::AsyncWriteExt::write_all(&mut go, b"go\n")
                .await
                .is_ok(),
            None => false,
        };

        let said = if go {
            launched(&mut child).await
        } else {
            Err(Launch {
                said: "the job's launcher could not be told to go on".into(),
                setsid: false,
            })
        };
        if let Err(Launch { said, setsid }) = said {
            // Before its session of its own, the launcher is this daemon's
            // child alone; after, its group may hold the program's
            // processes too.
            if setsid {
                let _ = killpg(Pid::from_raw(pid), Signal::SIGKILL);
            }
            let _ = child.start_kill();
            match tokio::time::timeout(REAP, child.wait()).await {
                Ok(_) => {
                    self.forget(&record.id);
                    return (failure("spawn-failed", format!("{program}: {said}")), id);
                }
                Err(_) => {
                    return (
                        self.keep_unconfirmed(record, child, &format!("{program}: {said}")),
                        id,
                    );
                }
            }
        }

        let leader = process::info(pid).ok().flatten();
        let own = leader.as_ref().is_some_and(|leader| {
            leader.instance.start == started && leader.pgid == pid && leader.runs_as(self.uid)
        });
        if !own {
            // It runs, but Atlas cannot confirm it is a group of the helper's
            // own: it is kept and shown, and signalled only once it is.
            return (
                self.keep_unconfirmed(
                    record,
                    child,
                    &format!(
                        "{program} started, but Atlas could not confirm it runs as the helper in a process group of its own"
                    ),
                ),
                id,
            );
        }

        let mut state = self.state();
        record.state = JobState::Running;
        // Its time limit runs from when it runs.
        record.ends_at = now_ms().saturating_add(lifetime);
        let written = self.write(&record);
        state.children.insert(record.id.clone(), child);
        state.running += 1;
        if let Err(error) = written {
            // It runs while its record says launched: the next look finds it
            // again by its identity.
            return (unconfirmed(&record, &error.error), id);
        }
        let processes = process::of_user(self.uid).unwrap_or_default();
        // Every job was stopped while this one launched: it stops too.
        if let Some((at, why)) = state.stopped_all
            && at >= record.started_at
        {
            self.begin_stop(&mut state, &mut record, why, &processes, now_ms());
        } else {
            // A program that already ended is reported so.
            self.look(&mut state, &mut record, &processes, false, false, now_ms());
        }
        (
            json!({"ok": true, "op": "job", "action": "start", "job": record.view()}),
            id,
        )
    }

    /// Keep `record`'s job, which may run, supervised but unconfirmed: its
    /// record stays, its leader is held, and the reply says its outcome is
    /// not known.
    fn keep_unconfirmed(&self, mut record: Record, child: Child, said: &str) -> Value {
        let mut state = self.state();
        record.state = JobState::Running;
        let _ = self.write(&record);
        state.children.insert(record.id.clone(), child);
        state.running += 1;
        unconfirmed(&record, said)
    }

    /// Stop job `id` when it is still what `expect` says: TERM to its group
    /// now, KILL after `grace` from the supervision, which needs no client.
    pub fn stop(&self, id: &JobId, expect: &Expected, grace: Duration) -> Value {
        let mut state = self.state();
        let mut record = match self.read(id) {
            None => return unknown(id),
            Some(Found::Unreadable(_)) => {
                return refusal("unknown", UNREADABLE).with_id(id);
            }
            Some(Found::Record(record)) => *record,
        };
        match record.state {
            JobState::Ended => return already_ended(&record),
            JobState::Starting | JobState::Launched => {
                return refusal("unknown", "The job has not finished starting").with_id(id);
            }
            JobState::Stopping => {
                return json!({"ok": true, "op": "job", "action": "stop",
                    "job": record.view(), "alreadyStopping": true});
            }
            JobState::Running => {}
        }
        if record.expected().as_ref() != Some(expect) {
            return identity(id);
        }

        let processes = process::of_user(self.uid).unwrap_or_default();
        let now = now_ms();
        match self.judge(&record, Some(expect)) {
            Verdict::Signal { pgid } => {
                self.note_seen(&mut record, &processes);
                record.state = JobState::Stopping;
                record.stopping = Some(Stopping {
                    why: How::Stopped,
                    since: now,
                    grace_ms: u64::try_from(grace.as_millis()).unwrap_or(u64::MAX),
                    signal: "SIGTERM".into(),
                    killed: None,
                    killed_last: None,
                    stuck: None,
                    refused: None,
                });
                if let Err(error) = self.write(&record) {
                    return failure(error.code, error.error);
                }
                state.stopping = true;
                match deliver(pgid, Signal::SIGTERM) {
                    Delivered::Sent => json!({"ok": true, "op": "job", "action": "stop",
                        "job": record.view(), "signalled": "SIGTERM"}),
                    Delivered::Ended => {
                        self.finish(&mut state, &mut record, How::Exited, &processes);
                        already_ended(&record)
                    }
                    Delivered::OtherAccount => {
                        record.state = JobState::Running;
                        record.stopping = None;
                        let _ = self.write(&record);
                        other_account(id)
                    }
                }
            }
            Verdict::Ended => {
                self.finish(&mut state, &mut record, How::Exited, &processes);
                already_ended(&record)
            }
            Verdict::Reused => {
                self.finish(&mut state, &mut record, How::Gone, &processes);
                already_ended(&record)
            }
            Verdict::Restarted => {
                self.finish(&mut state, &mut record, How::Restarted, &processes);
                already_ended(&record)
            }
            Verdict::Identity => identity(id),
            Verdict::OtherAccount => other_account(id),
        }
    }

    /// Stop every running job by the daemon's own records and the live
    /// processes, as `reason` says: the person's Stop, the helper on
    /// screen, or a log-out or uninstall.
    pub fn stop_all(&self, reason: StopReason) -> Value {
        let mut state = self.state();
        let processes = process::of_user(self.uid).unwrap_or_default();
        let now = now_ms();
        state.stopped_all = Some((now, How::of(reason)));
        let mut stopping = 0;
        let mut ended = 0;
        let mut refused = Vec::new();
        for read in self.records() {
            let Found::Record(mut record) = read else {
                continue;
            };
            match record.state {
                JobState::Running => {
                    self.begin_stop(&mut state, &mut record, How::of(reason), &processes, now);
                    match record.state {
                        JobState::Stopping
                            if record
                                .stopping
                                .as_ref()
                                .is_some_and(|stop| stop.refused.is_some()) =>
                        {
                            refused.push(json!({"id": record.id, "reason": record.stopping.as_ref().and_then(|stop| stop.refused.clone())}));
                        }
                        JobState::Stopping => stopping += 1,
                        JobState::Running => {
                            refused.push(json!({"id": record.id, "reason": "io"}));
                        }
                        _ => ended += 1,
                    }
                }
                JobState::Stopping => stopping += 1,
                _ => {}
            }
        }
        json!({"ok": true, "op": "job", "action": "stop-all", "stopping": stopping,
            "ended": ended, "refused": refused})
    }

    /// One job, or every one, as it stands now.
    pub fn show(&self, id: Option<&JobId>) -> Value {
        let mut state = self.state();
        let processes = process::of_user(self.uid).unwrap_or_default();
        let now = now_ms();
        let mut views = Vec::new();
        let reads = match id {
            Some(id) => match self.read(id) {
                Some(read) => vec![read],
                None => return unknown(id),
            },
            None => self.records(),
        };
        for read in reads {
            match read {
                Found::Unreadable(id) => views.push(json!({"id": id, "state": "unknown",
                    "error": UNREADABLE})),
                Found::Record(mut record) => {
                    if matches!(record.state, JobState::Running | JobState::Stopping) {
                        self.look(&mut state, &mut record, &processes, false, false, now);
                    }
                    views.push(record.view());
                }
            }
        }
        views.sort_by_key(|view| view["startedAt"].as_u64().unwrap_or(0));
        match id {
            Some(_) => json!({"ok": true, "op": "job", "action": "show", "job": views.pop()}),
            None => json!({"ok": true, "op": "job", "action": "show", "jobs": views}),
        }
    }

    /// The last `bytes` of job `id`'s output, as base64.
    pub fn log(&self, id: &JobId, bytes: usize) -> Value {
        let resets = match self.read(id) {
            None => return unknown(id),
            Some(Found::Record(record)) => record.log_resets,
            Some(Found::Unreadable(_)) => 0,
        };
        let path = self.log_path(id);
        let tail = (|| -> std::io::Result<(Vec<u8>, u64)> {
            let mut file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(&path)?;
            let size = file.metadata()?.len();
            let from = size.saturating_sub(bytes as u64);
            file.seek(SeekFrom::Start(from))?;
            let mut tail = Vec::new();
            file.take(bytes as u64).read_to_end(&mut tail)?;
            Ok((tail, size))
        })();
        match tail {
            Ok((tail, size)) => json!({"ok": true, "op": "job", "action": "log", "id": id,
                "data": STANDARD.encode(&tail), "size": size,
                "outputTruncated": size > tail.len() as u64, "logResets": resets}),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                json!({"ok": true, "op": "job", "action": "log", "id": id, "data": "",
                    "size": 0, "outputTruncated": false, "logResets": resets})
            }
            Err(error) => failure("io", format!("The job's log could not be read: {error}")),
        }
    }

    /// Look at every job once: end what has ended, stop what ran past its
    /// time, everything once the person has stopped agents (`stopped`) or
    /// the helper is on screen (`on_console`), escalate stops, keep logs
    /// within their limit, and prune old records now and then.
    pub fn watch_once(&self, stopped: bool, on_console: bool) {
        let mut state = self.state();
        let now = now_ms();
        let due = state
            .pruned
            .is_none_or(|pruned| pruned.elapsed() >= PRUNE_EVERY);
        if state.running == 0 && !due {
            return;
        }
        let records = self.records();
        let processes = if records
            .iter()
            .any(|read| matches!(read, Found::Record(record) if record.live()))
        {
            process::of_user(self.uid).unwrap_or_default()
        } else {
            Vec::new()
        };
        let mut running = 0;
        let mut any_stopping = false;
        for read in records {
            let Found::Record(mut record) = read else {
                continue;
            };
            if record.state == JobState::Launched && !state.launching.contains(&record.id) {
                self.found_launched(&mut state, &mut record, &processes);
            }
            if matches!(record.state, JobState::Running | JobState::Stopping) {
                self.look(
                    &mut state,
                    &mut record,
                    &processes,
                    stopped,
                    on_console,
                    now,
                );
                self.bound_log(&mut record);
            }
            if record.live() {
                running += 1;
            }
            any_stopping |= record.state == JobState::Stopping;
        }
        state.running = running;
        state.stopping = any_stopping;
        if due {
            self.prune(now);
            state.pruned = Some(Instant::now());
        }
    }

    /// How long until the next look: sooner while a job is stopping.
    pub fn interval(&self) -> Duration {
        if self.state().stopping {
            WATCH_STOPPING
        } else {
            WATCH
        }
    }

    /// Look at one running or stopping job: stop it when it must stop,
    /// escalate its stop, note its descendants, and end it once nothing of
    /// it is left.
    fn look(
        &self,
        state: &mut State,
        record: &mut Record,
        processes: &[Info],
        stopped: bool,
        on_console: bool,
        now: u64,
    ) {
        if record.state == JobState::Running {
            let why = if on_console {
                Some(How::OnConsole)
            } else if stopped {
                Some(How::Person)
            } else if now >= record.ends_at {
                Some(How::Timeout)
            } else {
                None
            };
            if let Some(why) = why {
                self.begin_stop(state, record, why, processes, now);
                return;
            }
            match self.judge(record, None) {
                Verdict::Signal { .. } => {
                    let before = record.seen.len();
                    self.note_seen(record, processes);
                    if record.seen.len() != before {
                        let _ = self.write(record);
                    }
                }
                Verdict::Ended => self.finish(state, record, How::Exited, processes),
                Verdict::Reused => self.finish(state, record, How::Gone, processes),
                Verdict::Restarted => self.finish(state, record, How::Restarted, processes),
                Verdict::Identity | Verdict::OtherAccount => {}
            }
            return;
        }
        self.escalate(state, record, processes, now);
    }

    /// Settle a job whose record says launched while no start is launching
    /// it, as when its record could not be written as running: supervised
    /// from here when it is still the job, ended when nothing of it is
    /// left, and otherwise shown and never signalled.
    fn found_launched(&self, state: &mut State, record: &mut Record, processes: &[Info]) {
        match self.judge(record, None) {
            Verdict::Signal { .. } => {
                record.state = JobState::Running;
                let _ = self.write(record);
            }
            Verdict::Ended | Verdict::Reused => self.finish(state, record, How::Gone, processes),
            Verdict::Restarted => self.finish(state, record, How::Restarted, processes),
            Verdict::Identity | Verdict::OtherAccount => {}
        }
    }

    /// Begin stopping `record` for `why`: TERM to its group when it is
    /// still the job's, its end when nothing of it is left.
    fn begin_stop(
        &self,
        state: &mut State,
        record: &mut Record,
        why: How,
        processes: &[Info],
        now: u64,
    ) {
        let verdict = self.judge(record, None);
        let refused = match verdict {
            Verdict::Signal { .. } => None,
            Verdict::Ended => return self.finish(state, record, How::Exited, processes),
            Verdict::Reused => return self.finish(state, record, How::Gone, processes),
            Verdict::Restarted => return self.finish(state, record, How::Restarted, processes),
            Verdict::Identity => Some("identity"),
            Verdict::OtherAccount => Some("other-account"),
        };
        self.note_seen(record, processes);
        record.state = JobState::Stopping;
        record.stopping = Some(Stopping {
            why,
            since: now,
            grace_ms: u64::try_from(super::protocol::JOB_GRACE.as_millis()).unwrap_or(5000),
            signal: "SIGTERM".into(),
            killed: None,
            killed_last: None,
            stuck: None,
            refused: refused.map(str::to_owned),
        });
        if self.write(record).is_err() {
            return;
        }
        state.stopping = true;
        if let Verdict::Signal { pgid } = verdict
            && deliver(pgid, Signal::SIGTERM) == Delivered::Ended
        {
            self.finish(state, record, why, processes);
        }
    }

    /// Carry a stop on: once its grace has passed, check the job again and
    /// KILL its group; once KILL has had its time, say how many are stuck
    /// and send it again now and then for a minute; once nothing of it is
    /// left, end it.
    fn escalate(&self, state: &mut State, record: &mut Record, processes: &[Info], now: u64) {
        let Some(mut stopping) = record.stopping.clone() else {
            return;
        };
        let pgid = match self.judge(record, None) {
            Verdict::Ended => return self.finish(state, record, stopping.why, processes),
            Verdict::Reused => return self.finish(state, record, How::Gone, processes),
            Verdict::Restarted => return self.finish(state, record, How::Restarted, processes),
            refused @ (Verdict::Identity | Verdict::OtherAccount) => {
                let why = if refused == Verdict::OtherAccount {
                    "other-account"
                } else {
                    "identity"
                };
                if stopping.refused.as_deref() != Some(why) {
                    stopping.refused = Some(why.to_owned());
                    record.stopping = Some(stopping);
                    let _ = self.write(record);
                }
                return;
            }
            Verdict::Signal { pgid } => pgid,
        };
        self.note_seen(record, processes);

        // What refused a signal before is gone: the TERM it never got is
        // sent now, and its grace runs from here.
        if stopping.refused.take().is_some() && stopping.killed.is_none() {
            stopping.since = now;
            stopping.signal = "SIGTERM".into();
            if deliver(pgid, Signal::SIGTERM) == Delivered::Ended {
                record.stopping = Some(stopping.clone());
                return self.finish(state, record, stopping.why, processes);
            }
            record.stopping = Some(stopping);
            let _ = self.write(record);
            return;
        }

        let after = |since: u64, wait: Duration| {
            now >= since.saturating_add(u64::try_from(wait.as_millis()).unwrap_or(u64::MAX))
        };
        let kill = match (stopping.killed, stopping.killed_last) {
            (None, _) => after(stopping.since, Duration::from_millis(stopping.grace_ms)),
            (Some(first), Some(last)) => {
                after(first, KILLED) && after(last, KILL_AGAIN) && !after(first, KILL_FOR)
            }
            (Some(_), None) => true,
        };
        if stopping.killed.is_some_and(|first| after(first, KILLED)) {
            let members = process::group(pgid).unwrap_or_default();
            stopping.stuck = Some(members.iter().filter(|member| !member.zombie).count());
        }
        if kill {
            // Checked again right before the signal.
            if let Verdict::Signal { pgid } = self.judge(record, None) {
                match deliver(pgid, Signal::SIGKILL) {
                    Delivered::Sent => {
                        stopping.killed.get_or_insert(now);
                        stopping.killed_last = Some(now);
                        stopping.signal = "SIGKILL".into();
                    }
                    Delivered::Ended => {
                        record.stopping = Some(stopping.clone());
                        return self.finish(state, record, stopping.why, processes);
                    }
                    Delivered::OtherAccount => stopping.refused = Some("other-account".into()),
                }
            }
        }
        if record.stopping.as_ref() != Some(&stopping) {
            record.stopping = Some(stopping);
            let _ = self.write(record);
        }
    }

    /// End `record` `how`, once nothing of it is left: its leader's exit
    /// collected when this daemon started it, and what left its group
    /// worked out. A leader that cannot be collected yet is looked at again
    /// next time.
    fn finish(&self, state: &mut State, record: &mut Record, how: How, processes: &[Info]) {
        let mut exit = None;
        if let Some(child) = state.children.get_mut(&record.id) {
            match child.try_wait() {
                Ok(Some(status)) => exit = Some(status),
                Ok(None) if how != How::Gone && how != How::Restarted => return,
                _ => {}
            }
            state.children.remove(&record.id);
        }
        let pgid = record.pgid.unwrap_or_default();
        let ended = Ended {
            how,
            at: now_ms(),
            signal: record
                .stopping
                .as_ref()
                .map(|stopping| stopping.signal.clone()),
            exit_code: exit.and_then(|status| status.code()),
            exit_signal: exit
                .and_then(|status| status.signal())
                .map(run::signal_name),
            left: left(&record.seen, processes, pgid, self.uid),
        };
        record.state = JobState::Ended;
        record.ended = Some(ended);
        let _ = self.write(record);
        state.running = state.running.saturating_sub(1);
    }

    /// Whether `record`'s group may be signalled now, as the live processes
    /// say, read just before.
    fn judge(&self, record: &Record, expect: Option<&Expected>) -> Verdict {
        let (Some(pid), Some(pgid)) = (record.pid, record.pgid) else {
            return Verdict::Identity;
        };
        let (Ok(leader), Ok(members)) = (process::info(pid), process::group(pgid)) else {
            return Verdict::Identity;
        };
        verdict(
            record,
            expect,
            &self.boot,
            self.uid,
            leader.as_ref(),
            &members,
        )
    }

    /// Note `record`'s descendants among `processes`.
    fn note_seen(&self, record: &mut Record, processes: &[Info]) {
        let (Some(pid), Some(start), Some(pgid)) = (record.pid, record.start, record.pgid) else {
            return;
        };
        let members: Vec<Info> = processes
            .iter()
            .filter(|process| process.pgid == pgid)
            .cloned()
            .collect();
        observe(
            &mut record.seen,
            &Instance { pid, start },
            &members,
            processes,
            process::session,
        );
    }

    /// Empty `record`'s log once it grows past its limit, through a handle
    /// of the daemon's own: the job's appends go on at the new end.
    fn bound_log(&self, record: &mut Record) {
        let path = self.log_path(&record.id);
        let over = fs::symlink_metadata(&path)
            .is_ok_and(|metadata| metadata.is_file() && metadata.len() > self.log_limit);
        if !over {
            return;
        }
        let emptied = OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&path)
            .and_then(|file| file.set_len(0));
        if emptied.is_ok() {
            record.log_resets = record.log_resets.saturating_add(1);
            let _ = self.write(record);
        }
    }

    /// Find again, when the daemon starts, every job its records say may
    /// still run: one still the same process is supervised again, one that
    /// is not is ended; one whose record cannot be read, or that never got
    /// a process, is shown and never signalled.
    fn adopt(&self) {
        let mut state = self.state();
        let processes = process::of_user(self.uid).unwrap_or_default();
        let mut running = 0;
        for read in self.records() {
            let Found::Record(mut record) = read else {
                continue;
            };
            if !matches!(
                record.state,
                JobState::Launched | JobState::Running | JobState::Stopping
            ) {
                continue;
            }
            match self.judge(&record, None) {
                Verdict::Signal { .. } => {
                    record.adopted = true;
                    if record.state == JobState::Launched {
                        record.state = JobState::Running;
                    }
                    let _ = self.write(&record);
                    running += 1;
                }
                Verdict::Ended | Verdict::Reused => {
                    state.running += 1;
                    self.finish(&mut state, &mut record, How::Gone, &processes);
                }
                Verdict::Restarted => {
                    state.running += 1;
                    self.finish(&mut state, &mut record, How::Restarted, &processes);
                }
                // Never signalled; shown as its record says.
                Verdict::Identity | Verdict::OtherAccount => running += 1,
            }
        }
        state.running = running;
        state.stopping = false;
    }

    /// Remove ended records older than a week, with their logs, and the
    /// oldest ended ones past [`RECORDS_MAX`]. A record of a job that may
    /// still run, or one that cannot be read, is never removed.
    fn prune(&self, now: u64) {
        let week = u64::try_from(KEEP.as_millis()).unwrap_or(u64::MAX);
        let mut ended: Vec<(u64, JobId)> = self
            .records()
            .into_iter()
            .filter_map(|read| match read {
                Found::Record(record) if record.state == JobState::Ended => Some((
                    record
                        .ended
                        .as_ref()
                        .map_or(record.started_at, |ended| ended.at),
                    record.id,
                )),
                _ => None,
            })
            .collect();
        ended.sort();
        let total = self.folder.listed(LISTED).len();
        let mut over = total.saturating_sub(RECORDS_MAX);
        for (at, id) in ended {
            if now.saturating_sub(at) > week || over > 0 {
                self.forget(&id);
                over = over.saturating_sub(1);
            }
        }
    }

    /// Every record, as read.
    fn records(&self) -> Vec<Found> {
        self.folder
            .listed(LISTED)
            .into_iter()
            .filter_map(|id| self.read(&id))
            .collect()
    }

    /// The record of `id`: none when there is no file, unreadable when it
    /// does not parse, is of another version or another job.
    fn read(&self, id: &JobId) -> Option<Found> {
        if !self.folder.holds(id) {
            return None;
        }
        let record = self
            .folder
            .read(id)
            .and_then(|bytes| serde_json::from_slice::<Record>(&bytes).ok())
            .filter(|record| record.version == VERSION && record.id == *id);
        Some(match record {
            Some(record) if record.state != JobState::Starting => Found::Record(Box::new(record)),
            _ => Found::Unreadable(id.clone()),
        })
    }

    /// Write `record` whole.
    fn write(&self, record: &Record) -> Result<()> {
        self.folder.write(&record.id, &serde_json::to_vec(record)?)
    }

    /// [`Jobs::write`], taking the state first.
    fn locked_write(&self, record: &Record) -> Result<()> {
        let _state = self.state();
        self.write(record)
    }

    /// Remove `id`'s record and log.
    fn forget(&self, id: &JobId) {
        self.folder.remove(id);
        let _ = fs::remove_file(self.log_path(id));
    }

    fn log_path(&self, id: &JobId) -> PathBuf {
        self.folder.directory().join(format!("{id}.log"))
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Watch `jobs` until dropped: about once a second, five times a second
/// while one is stopping. A Stop is seen through the input service's
/// saved `stop_marker`; the helper on screen through `on_console`.
pub async fn supervise(jobs: Arc<Jobs>, stop_marker: PathBuf, on_console: fn() -> bool) {
    loop {
        tokio::time::sleep(jobs.interval()).await;
        let watched = jobs.clone();
        let marker = stop_marker.clone();
        let _ = tokio::task::spawn_blocking(move || {
            watched.watch_once(marker.exists(), on_console());
        })
        .await;
    }
}

/// Wait for a launcher to say it is a session of its own and then exec:
/// `setsid` on its status pipe, which closes when its exec succeeds. A line
/// that says `error`, a close without `setsid`, or no close within
/// [`LAUNCH`] means its program never ran.
async fn launched(child: &mut Child) -> std::result::Result<(), Launch> {
    let fail = |said: &str, setsid: bool| Launch {
        said: said.to_owned(),
        setsid,
    };
    let Some(mut status) = child.stdout.take() else {
        return Err(fail("the job's launcher has no status pipe", false));
    };
    let mut said = Vec::new();
    let read = tokio::time::timeout(LAUNCH, async {
        let mut buffer = [0_u8; 512];
        loop {
            let count = status.read(&mut buffer).await?;
            if count == 0 || said.len() > 4096 {
                return Ok::<_, std::io::Error>(());
            }
            said.extend_from_slice(&buffer[..count]);
        }
    })
    .await;
    let said = String::from_utf8_lossy(&said);
    let setsid = said.lines().any(|line| line == "setsid");
    if let Some(error) = said.lines().find_map(|line| line.strip_prefix("error ")) {
        return Err(fail(error, setsid));
    }
    match read {
        Err(_) => Err(fail("the job did not start within 5 seconds", setsid)),
        Ok(Err(error)) => Err(fail(&error.to_string(), setsid)),
        Ok(Ok(())) if setsid => Ok(()),
        Ok(Ok(())) => Err(fail(
            "the job's launcher ended before it started the program",
            false,
        )),
    }
}

/// Why a launch failed, and whether the launcher had made its own session
/// by then, so its group may hold more than the launcher.
struct Launch {
    said: String,
    setsid: bool,
}

/// The reply for a job that may run but could not be confirmed: of unknown
/// outcome, with the job as the daemon keeps it.
fn unconfirmed(record: &Record, said: &str) -> Value {
    let mut reply = failure("job", said);
    reply["reason"] = json!("unconfirmed");
    reply["uncertain"] = json!(true);
    reply["id"] = json!(record.id);
    reply["job"] = record.view();
    reply
}

/// Send `signal` to group `pgid`, deciding a refusal from who is left in
/// it: only the daemon's own unreaped leader means the job has ended.
fn deliver(pgid: i32, signal: Signal) -> Delivered {
    match killpg(Pid::from_raw(pgid), signal) {
        Ok(()) => Delivered::Sent,
        Err(Errno::ESRCH) => Delivered::Ended,
        Err(_) => {
            let members = process::group(pgid).unwrap_or_default();
            if members.iter().all(|member| member.zombie) {
                Delivered::Ended
            } else {
                Delivered::OtherAccount
            }
        }
    }
}

/// A refusal with code `job`, for `reason`: never of unknown outcome.
fn refusal(reason: &str, error: &str) -> Value {
    let mut refusal = failure("job", error);
    refusal["reason"] = json!(reason);
    refusal
}

/// The refusal of a job no record names.
fn unknown(id: &JobId) -> Value {
    refusal(
        "unknown",
        "No background job with that ID is known in the helper",
    )
    .with_id(id)
}

fn identity(id: &JobId) -> Value {
    refusal(
        "identity",
        "The job is no longer the process Atlas started, so nothing was signalled",
    )
    .with_id(id)
}

fn other_account(id: &JobId) -> Value {
    refusal(
        "other-account",
        "Part of the job runs as another account, so nothing was signalled",
    )
    .with_id(id)
}

/// The reply to a stop of a job that has already ended.
fn already_ended(record: &Record) -> Value {
    json!({"ok": true, "op": "job", "action": "stop", "job": record.view(), "alreadyEnded": true})
}

/// A reply naming the job it is about.
trait WithId {
    fn with_id(self, id: &JobId) -> Self;
}

impl WithId for Value {
    fn with_id(mut self, id: &JobId) -> Self {
        self["id"] = json!(id);
        self
    }
}

#[cfg(test)]
mod tests;
