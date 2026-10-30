//! Background jobs: programs an agent keeps running in the helper after the
//! command that started them returns, each in a session and process group
//! of its own, supervised by the daemon and stopped only by their identity.
//!
//! A job is known by the boot it started in, its leader's PID and exact
//! start time, and its process group, which is the leader's PID: a session
//! leader can never move to another group. Only that group is ever
//! signalled, from one place, and only while its leader is still the job's
//! and the group is the helper's, by a fresh look right before each signal.
//! A job whose leader is gone is never signalled: what remains of its group
//! is reported. Nothing is ever found or signalled by name.
//!
//! The daemon's records live beside its socket, one file a job, written
//! whole at every change of state, and the job's output goes to a log
//! beside it. A daemon that restarts finds its jobs again by those
//! records: a job outlives its supervisor, because launchd ends only the
//! daemon's own process group when the daemon exits.
mod launch;
mod observe;
mod record;
mod reply;
mod signal;
mod store;
mod supervise;
mod verdict;

pub use supervise::supervise;

use super::protocol::{
    Census, Expected, How, JobFailure, JobId, JobRefusal, JobReply, JobRequest, JobStart, JobState,
    Logged, NamedJob, RefusedJob, Showing, Shown, StopCameTo, StopReason, Stopped, StoppedAll,
    Unread, Unreadable, failure,
};
use crate::{
    error::{Code, Error, Result},
    host_session::now_ms,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use nix::sys::signal::Signal;
use observe::{Kernel, Live};
use record::Stopping;
use reply::{already_ended, identity, refused_by, unknown};
use serde_json::Value;
use signal::Signalled;
use std::{
    collections::{HashMap, HashSet},
    fs::OpenOptions,
    io::{Read, Seek, SeekFrom},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};
use store::{Found, Store};
use tokio::process::Child;
use verdict::Verdict;

/// A log past this is emptied, and whatever writes it goes on from the start.
const LOG_LIMIT: u64 = 32 * 1024 * 1024;
/// What the daemon says of a record it cannot read.
const UNREADABLE: &str = "Atlas cannot tell whether this job's process is running";

/// What the daemon holds of the jobs it supervises.
pub(super) struct State {
    store: Store,
    /// The leaders this daemon started, held unreaped until their group is
    /// empty so the group's ID stays theirs.
    children: HashMap<JobId, Child>,
    /// How many jobs may still run, as the records said last.
    running: usize,
    /// How many records could not be read, so their jobs may still run.
    unresolved: usize,
    /// Whether one is stopping, so it is looked at more often.
    stopping: bool,
    pruned: Option<Instant>,
    /// The jobs a start is launching now, which no look touches yet.
    launching: HashSet<JobId>,
    /// When every job was last stopped at once, and why: a job still
    /// launching then is stopped as soon as it runs.
    stopped_all: Option<(u64, How)>,
}

/// The helper's background jobs.
pub struct Jobs {
    /// The program that becomes each job: this daemon's own executable.
    launcher: PathBuf,
    uid: u32,
    boot: String,
    log_limit: u64,
    kernel: Box<dyn Kernel>,
    state: Mutex<State>,
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
        Self::open_in(
            daemon,
            launcher,
            uid,
            log_limit,
            Box::new(Live),
            process_boot()?,
        )
    }

    /// [`Jobs::open_with`], reading processes and signalling through
    /// `kernel`, in the boot `boot`.
    pub fn open_in(
        daemon: &Path,
        launcher: PathBuf,
        uid: u32,
        log_limit: u64,
        kernel: Box<dyn Kernel>,
        boot: String,
    ) -> Result<Self> {
        let jobs = Self {
            launcher,
            uid,
            boot,
            log_limit,
            kernel,
            state: Mutex::new(State {
                store: Store::open(daemon)?,
                children: HashMap::new(),
                running: 0,
                unresolved: 0,
                stopping: false,
                pruned: None,
                launching: HashSet::new(),
                stopped_all: None,
            }),
        };
        jobs.adopt();
        Ok(jobs)
    }

    /// The folder the records are in.
    pub fn folder(&self) -> PathBuf {
        self.state().store.directory().to_owned()
    }

    /// How many jobs may still run, as the records said last.
    pub fn running(&self) -> usize {
        self.state().running
    }

    /// What the daemon's hello says of its jobs: how many may run, how many
    /// are launching, and how many records it cannot read, whose jobs may
    /// still run too.
    pub fn census(&self) -> Census {
        let state = self.state();
        Census {
            running: state.running,
            launching: state.launching.len(),
            unresolved: state.unresolved,
        }
    }

    /// Carry out `request`, the reply its connection gets. What reads the
    /// process table and writes records runs on a blocking thread, so the
    /// daemon's other requests never wait for it.
    pub async fn serve(self: &Arc<Self>, request: JobRequest) -> Value {
        let blocking = |answer: Box<dyn FnOnce(&Jobs) -> Value + Send>| {
            let jobs = self.clone();
            async move {
                tokio::task::spawn_blocking(move || answer(&jobs))
                    .await
                    .unwrap_or_else(|_| {
                        failure(Code::Job, "The helper could not answer about its jobs")
                    })
            }
        };
        match request {
            JobRequest::Start { start, uid, .. } => {
                if uid != self.uid {
                    return failure(
                        Code::Identity,
                        "This helper daemon runs as another account than the one asked for",
                    );
                }
                self.start(start).await
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

    /// Start a job: write its record, spawn the launcher, let it go on once
    /// its process is recorded, see it become a session and group of its
    /// own and exec the program, check its identity, and reply with it.
    /// The file and process work runs on blocking threads. A launch that
    /// fails before its exec leaves nothing running; one that may have
    /// started something is kept, supervised, and said to be unconfirmed.
    pub async fn start(self: &Arc<Self>, start: JobStart) -> Value {
        let jobs = self.clone();
        let asked = start.clone();
        let prepared = match tokio::task::spawn_blocking(move || jobs.prepare(&asked)).await {
            Ok(Ok(prepared)) => prepared,
            Ok(Err(refusal)) => return refusal,
            Err(_) => return failure(Code::SpawnFailed, "The job could not be prepared"),
        };
        let id = prepared.record.id.clone();
        let reply = self.launch(prepared, &start).await;
        self.state().launching.remove(&id);
        reply
    }

    /// Stop job `id` when it is still what `expect` says: the stop is kept
    /// first, then TERM goes to its group after a fresh look; KILL follows
    /// after `grace` from the supervision, which needs no client.
    pub fn stop(&self, id: &JobId, expect: &Expected, grace: Duration) -> Value {
        let mut state = self.state();
        let mut record = match state.store.read(id) {
            None => return unknown(id),
            Some(Found::Unreadable(_)) => {
                return JobFailure::new(JobRefusal::Unknown, UNREADABLE)
                    .about(id)
                    .into_value();
            }
            Some(Found::Record(record)) => *record,
        };
        match record.state {
            JobState::Ended => return already_ended(&record),
            JobState::Starting | JobState::Launched => {
                return JobFailure::new(JobRefusal::Unknown, "The job has not finished starting")
                    .about(id)
                    .into_value();
            }
            JobState::Stopping => {
                return JobReply::Stop(Stopped {
                    job: record.view(),
                    came_to: StopCameTo::AlreadyStopping(true),
                })
                .into_value();
            }
            JobState::Running => {}
        }
        if record.expected().as_ref() != Some(expect) {
            return identity(id);
        }

        let processes = self.kernel.of_user(self.uid).ok();
        let (verdict, _) = self.judge(&state, &record, Some(expect));
        if self.settle(
            &mut state,
            &mut record,
            &verdict,
            How::Exited,
            processes.as_deref(),
        ) {
            return already_ended(&record);
        }
        if !matches!(verdict, Verdict::Signal { .. }) {
            return refused_by(&verdict, id);
        }

        let grace_ms = crate::time::ms(grace);
        let before = record.clone();
        record.state = JobState::Stopping;
        record.stopping = Some(Stopping::new(How::Stopped, now_ms(), grace_ms));
        state.store.put(&record);
        state.stopping = true;
        match self.signal(&state, &record, Some(expect), Signal::SIGTERM) {
            Signalled::Sent => {
                if let Some(stopping) = record.stopping.as_mut() {
                    stopping.term_sent = true;
                    stopping.signal = Some("SIGTERM".into());
                    stopping.since = now_ms();
                }
                state.store.put(&record);
                JobReply::Stop(Stopped {
                    job: record.view(),
                    came_to: StopCameTo::Signalled("SIGTERM".into()),
                })
                .into_value()
            }
            Signalled::NotSent(verdict) => {
                if self.settle(
                    &mut state,
                    &mut record,
                    &verdict,
                    How::Exited,
                    processes.as_deref(),
                ) {
                    return already_ended(&record);
                }
                state.store.put(&before);
                refused_by(&verdict, id)
            }
        }
    }

    /// Stop every job by the daemon's own records and fresh looks at their
    /// processes, as `reason` says: the person's Stop, the helper on
    /// screen, or a log-out or uninstall. A job still launching is stopped
    /// as soon as it runs. The reply counts the jobs stopping and ended,
    /// and names those nothing could be sent to and those whose processes
    /// could not be read.
    pub fn stop_all(&self, reason: StopReason) -> Value {
        let mut state = self.state();
        let processes = self.kernel.of_user(self.uid).ok();
        let now = now_ms();
        state.stopped_all = Some((now, How::from(reason)));
        let (mut stopping, mut ended) = (0, 0);
        let (mut refused, mut unobserved, mut unresolved) = (Vec::new(), Vec::new(), Vec::new());
        for read in state.store.records() {
            let mut record = match read {
                Found::Record(record) => *record,
                // A start is writing it; it is stopped as soon as it runs.
                Found::Unreadable(id) if state.launching.contains(&id) => continue,
                // Never acted on, and never taken to have ended.
                Found::Unreadable(id) => {
                    unresolved.push(NamedJob { id });
                    continue;
                }
            };
            if record.state == JobState::Launched && !state.launching.contains(&record.id) {
                self.found_launched(&mut state, &mut record, processes.as_deref());
            }
            if record.state == JobState::Running {
                self.begin_stop(
                    &mut state,
                    &mut record,
                    How::from(reason),
                    supervise::GRACE_MS,
                    processes.as_deref(),
                    now,
                );
            } else if record.state != JobState::Stopping {
                if record.state == JobState::Launched && !state.launching.contains(&record.id) {
                    refused.push(RefusedJob {
                        id: record.id,
                        reason: JobRefusal::Identity,
                    });
                }
                continue;
            }
            match (
                record.state,
                record.stopping.as_ref().and_then(|stop| stop.refused),
            ) {
                (JobState::Ended, _) => ended += 1,
                (_, Some(JobRefusal::Unobserved)) => unobserved.push(NamedJob { id: record.id }),
                (_, Some(reason)) => refused.push(RefusedJob {
                    id: record.id,
                    reason,
                }),
                _ => stopping += 1,
            }
        }
        self.count(&mut state);
        JobReply::StopAll(StoppedAll {
            stopping,
            ended,
            refused,
            unknown: unobserved,
            unresolved,
        })
        .into_value()
    }

    /// One job, or every one, as it stands now.
    pub fn show(&self, id: Option<&JobId>) -> Value {
        let mut state = self.state();
        let processes = self.kernel.of_user(self.uid).ok();
        let now = now_ms();
        let reads = match id {
            Some(id) => match state.store.read(id) {
                Some(read) => vec![read],
                None => return unknown(id),
            },
            None => state.store.records(),
        };
        let mut views = Vec::new();
        for read in reads {
            match read {
                Found::Unreadable(id) => views.push(Shown::Unreadable(Unreadable {
                    id,
                    state: Unread::Unknown,
                    error: UNREADABLE.to_owned(),
                })),
                Found::Record(mut record) => {
                    self.look(&mut state, &mut record, processes.as_deref(), None, now);
                    views.push(Shown::Job(Box::new(record.view())));
                }
            }
        }
        views.sort_by_key(Shown::started_at);
        JobReply::Show(match id {
            Some(_) => Showing::One { job: views.pop() },
            None => Showing::Every { jobs: views },
        })
        .into_value()
    }

    /// The last `bytes` of job `id`'s output, as base64, with the task the
    /// job belongs to.
    pub fn log(&self, id: &JobId, bytes: usize) -> Value {
        let state = self.state();
        let (resets, task) = match state.store.read(id) {
            None => return unknown(id),
            Some(Found::Record(record)) => (record.log_resets, record.task.clone()),
            Some(Found::Unreadable(_)) => (0, None),
        };
        let path = state.store.log_path(id);
        drop(state);
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
        let (tail, size) = match tail {
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (Vec::new(), 0),
            Err(error) => {
                return failure(
                    Code::Io,
                    format!("The job's log could not be read: {error}"),
                );
            }
        };
        JobReply::Log(Logged {
            id: id.clone(),
            task,
            data: STANDARD.encode(&tail),
            size,
            output_truncated: size > tail.len() as u64,
            log_resets: resets,
        })
        .into_value()
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// This boot's identity.
fn process_boot() -> Result<String> {
    crate::process::boot().map_err(|error| Error::new(Code::Job, error.error))
}

#[cfg(test)]
mod tests;
