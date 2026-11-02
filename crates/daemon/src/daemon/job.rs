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

pub(super) use supervise::supervise;

use atlas_core::protocol::{
    Census, Expected, How, JobFailure, JobId, JobRefusal, JobReply, JobRequest, JobStart, JobState,
    Logged, NamedJob, RefusedJob, Showing, Shown, StopCameTo, StopReason, StoppedAll, Unread,
    Unreadable, failure,
};
use atlas_core::{
    error::{Code, Error, Result},
    process::Info,
    time::now_ms,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use nix::sys::signal::Signal;
use observe::{Kernel, Live};
use record::{Record, Stopping};
use reply::{already_ended, refusal, refused_by, stop_reply, unknown};
use serde_json::{Value, json};
use signal::Signalled;
use std::{
    collections::{HashMap, HashSet},
    fs::OpenOptions,
    io::{Seek, SeekFrom},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
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
struct State {
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

impl State {
    /// Whether `record` says launched while no start is launching it.
    fn left_launched(&self, record: &Record) -> bool {
        record.state == JobState::Launched && !self.launching.contains(&record.id)
    }
}

/// The helper's background jobs.
pub struct Jobs {
    /// The program that becomes each job: this daemon's own executable.
    launcher: PathBuf,
    uid: u32,
    boot: String,
    log_limit: u64,
    kernel: Box<dyn Kernel>,
    /// Independent of poison; only a fresh supervisor can trust ownership again.
    unavailable: AtomicBool,
    state: Mutex<State>,
}

impl Jobs {
    /// The jobs whose records are in `daemon`'s `jobs` folder, for the
    /// helper account `uid`, started through `launcher`. Every record is
    /// read, and each job still running is found again by its identity.
    ///
    /// # Errors
    ///
    /// Returns boot-identity, job-storage or adoption failures. Launcher and UID are supplied
    /// by the daemon owner; this constructor does not independently validate those arguments.
    pub fn open(daemon: &Path, launcher: PathBuf, uid: u32) -> Result<Self> {
        Self::open_with(daemon, launcher, uid, LOG_LIMIT)
    }

    /// [`Jobs::open`], emptying a log past `log_limit` bytes.
    ///
    /// # Errors
    ///
    /// Returns boot-identity, job-storage or adoption failures. The supplied launcher, UID
    /// and log limit are policy inputs, not values validated by this constructor.
    pub fn open_with(daemon: &Path, launcher: PathBuf, uid: u32, log_limit: u64) -> Result<Self> {
        Self::open_in(
            daemon,
            launcher,
            uid,
            log_limit,
            Box::new(Live),
            atlas_core::process::boot()
                .map_err(|error| Error::new(Code::Job, error.error.clone()).with_source(error))?,
        )
    }

    /// [`Jobs::open_with`], reading processes and signalling through
    /// `kernel`, in the boot `boot`.
    fn open_in(
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
            unavailable: AtomicBool::new(false),
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
        jobs.adopt()?;
        Ok(jobs)
    }

    /// The folder the records are in.
    ///
    /// # Errors
    ///
    /// Returns a supervision-state error if the job owner cannot be accessed safely.
    pub fn folder(&self) -> Result<PathBuf> {
        Ok(self.state()?.store.directory().to_owned())
    }

    /// How many jobs may still run, as the records said last.
    ///
    /// # Errors
    ///
    /// Returns a supervision-state error rather than reporting zero jobs when supervision is
    /// unavailable.
    pub fn running(&self) -> Result<usize> {
        Ok(self.state()?.running)
    }

    /// What the daemon's hello says of its jobs: how many may run, how many
    /// are launching, and how many records it cannot read, whose jobs may
    /// still run too.
    /// Returns an error when supervision is unavailable; unknown jobs never become zero.
    pub(crate) fn census(&self) -> Result<Census> {
        let state = self.state()?;
        Ok(Census {
            running: state.running,
            launching: state.launching.len(),
            unresolved: state.unresolved,
        })
    }

    /// Carry out `request`, the reply its connection gets. What reads the
    /// process table and writes records runs on a blocking thread, so the
    /// daemon's other requests never wait for it.
    pub(crate) async fn serve(self: &Arc<Self>, request: JobRequest) -> Value {
        let answer: Box<dyn FnOnce(&Jobs) -> Value + Send> = match request {
            JobRequest::Start { start, uid, .. } => {
                if uid != self.uid {
                    return failure(
                        Code::Identity,
                        "This helper daemon runs as another account than the one asked for",
                    );
                }
                return self.start(start).await;
            }
            JobRequest::Stop { id, expect, grace } => {
                Box::new(move |jobs| jobs.stop(&id, &expect, grace))
            }
            JobRequest::StopAll { reason } => Box::new(move |jobs| jobs.stop_all(reason)),
            JobRequest::Show { id } => Box::new(move |jobs| jobs.show(id.as_ref())),
            JobRequest::Log { id, bytes } => Box::new(move |jobs| jobs.log(&id, bytes)),
        };
        let jobs = self.clone();
        tokio::task::spawn_blocking(move || answer(&jobs))
            .await
            .unwrap_or_else(|_| {
                self.fail_supervision();
                failure(Code::Job, "The helper could not answer about its jobs")
            })
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
            Err(_) => {
                self.fail_supervision();
                return failure(Code::SpawnFailed, "The job could not be prepared");
            }
        };
        let id = prepared.record.id.clone();
        let reply = self.launch(prepared, &start).await;
        match self.state() {
            Ok(mut state) => {
                state.launching.remove(&id);
            }
            Err(error) => return json!(error.uncertain()),
        }
        reply
    }

    /// Stop job `id` when it is still what `expect` says: the stop is kept
    /// first, then TERM goes to its group after a fresh look; KILL follows
    /// after `grace` from the supervision, which needs no client.
    pub fn stop(&self, id: &JobId, expect: &Expected, grace: Duration) -> Value {
        let mut state = match self.state() {
            Ok(state) => state,
            Err(error) => return json!(error),
        };
        let mut record = match state.store.read(id) {
            None => return unknown(id),
            Some(Found::Unreadable(_)) => return refusal(JobRefusal::Unknown, UNREADABLE, id),
            Some(Found::Record(record)) => *record,
        };
        match record.state {
            JobState::Ended => return already_ended(&record),
            JobState::Starting | JobState::Launched => {
                return refusal(JobRefusal::Unknown, "The job has not finished starting", id);
            }
            JobState::Stopping => return stop_reply(&record, StopCameTo::AlreadyStopping(true)),
            JobState::Running => {}
        }
        if record.expected().as_ref() != Some(expect) {
            return refused_by(&Verdict::Identity, id);
        }

        let processes = self.processes();
        let verdict = self.judge(&state, &record, Some(expect));
        if self.settle(
            &mut state,
            &mut record,
            &verdict,
            How::Exited,
            processes.as_deref(),
        ) {
            return already_ended(&record);
        }
        if !verdict.signals() {
            return refused_by(&verdict, id);
        }

        let grace_ms = atlas_core::time::ms(grace);
        let before = record.clone();
        record.state = JobState::Stopping;
        record.stopping = Some(Stopping::new(How::Stopped, now_ms(), grace_ms));
        state.store.put(&record);
        state.stopping = true;
        match self.signal(&state, &record, Some(expect), Signal::SIGTERM) {
            Signalled::Sent => {
                if let Some(stopping) = record.stopping.as_mut() {
                    stopping.term_reached(now_ms());
                }
                state.store.put(&record);
                stop_reply(&record, StopCameTo::Signalled("SIGTERM".into()))
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
        let mut state = match self.state() {
            Ok(state) => state,
            Err(error) => return json!(error),
        };
        let processes = self.processes();
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
            if state.left_launched(&record) {
                self.found_again(&mut state, &mut record, processes.as_deref());
            }
            if record.state == JobState::Running {
                self.begin_stop(
                    &mut state,
                    &mut record,
                    How::from(reason),
                    processes.as_deref(),
                    now,
                );
            } else if record.state != JobState::Stopping {
                if state.left_launched(&record) {
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
                (_, None) => stopping += 1,
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
        let mut state = match self.state() {
            Ok(state) => state,
            Err(error) => return json!(error),
        };
        let processes = self.processes();
        let now = now_ms();
        let reads = match id {
            Some(id) => match state.store.read(id) {
                Some(read) => vec![read],
                None => return unknown(id),
            },
            None => {
                let (records, complete) = state.store.scanned_records();
                if !complete {
                    return JobFailure::new(
                        JobRefusal::Unobserved,
                        "Atlas could not read every background job record; a job may still be running.",
                    ).into_value();
                }
                records
            }
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
        let state = match self.state() {
            Ok(state) => state,
            Err(error) => return json!(error),
        };
        let (resets, task) = match state.store.read(id) {
            None => return unknown(id),
            Some(Found::Record(record)) => (record.log_resets, record.task.clone()),
            Some(Found::Unreadable(_)) => (0, None),
        };
        let path = state.store.log_path(id);
        drop(state);
        let (tail, size) = match log_tail(&path, bytes as u64) {
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

    fn state(&self) -> Result<MutexGuard<'_, State>> {
        if self.unavailable.load(Ordering::SeqCst) {
            return Err(Self::unavailable());
        }
        let state = self.state.lock().map_err(|_| {
            self.fail_supervision();
            Self::unavailable()
        })?;
        if self.unavailable.load(Ordering::SeqCst) {
            return Err(Self::unavailable());
        }
        Ok(state)
    }

    pub(super) fn unavailable() -> Error {
        Error::new(
            Code::Job,
            "The helper could not answer about its jobs; restart the daemon to recover",
        )
    }

    pub(super) fn fail_supervision(&self) {
        self.unavailable.store(true, Ordering::SeqCst);
    }

    /// The helper account's processes now, when the kernel says.
    fn processes(&self) -> Option<Vec<Info>> {
        self.kernel.of_user(self.uid).ok()
    }

    /// Poison recovery here only retains ownership; it cannot observe or settle jobs.
    fn retain_unobserved_child(&self, id: JobId, child: Child) {
        self.fail_supervision();
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        state.children.insert(id, child);
    }

    /// The state, with `child` held in it as job `id`'s leader; unavailable
    /// supervision still retains it.
    fn holding(&self, id: &JobId, child: Child) -> Result<MutexGuard<'_, State>> {
        match self.state() {
            Ok(mut state) => {
                state.children.insert(id.clone(), child);
                Ok(state)
            }
            Err(error) => {
                self.retain_unobserved_child(id.clone(), child);
                Err(error)
            }
        }
    }
}

/// The last `bytes` of the log at `path`, and how long the log is.
fn log_tail(path: &Path, bytes: u64) -> std::io::Result<(Vec<u8>, u64)> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let size = file.metadata()?.len();
    file.seek(SeekFrom::Start(size.saturating_sub(bytes)))?;
    Ok((atlas_core::files::read_up_to(file, bytes)?, size))
}

#[cfg(test)]
mod tests;
