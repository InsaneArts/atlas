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
mod observe;
mod record;
mod signal;
mod store;
mod supervise;
mod verdict;

pub use observe::{Kernel, Live, left, observe};
pub use record::{Ended, How, JobIdentity, JobState, Left, Record, Stopping};
pub use supervise::supervise;
pub use verdict::{Observation, Verdict, verdict};

use super::{
    protocol::{Expected, JobId, JobRequest, StopReason, failure},
    run,
};
use crate::{
    error::{Error, Result},
    host_session::{Key, now_ms},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use nix::sys::signal::Signal;
use record::VERSION;
use serde_json::{Value, json};
use signal::Signalled;
use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    fs::OpenOptions,
    io::{Read, Seek, SeekFrom},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};
use store::{Found, Store};
use tokio::{io::AsyncReadExt, process::Child};

/// The most jobs running in the helper at once.
const RUNNING_MAX: usize = 16;
/// The most jobs one task keeps running at once.
const TASK_MAX: usize = 8;
/// A log past this is emptied, and whatever writes it goes on from the start.
const LOG_LIMIT: u64 = 32 * 1024 * 1024;
/// How long the launcher has to say it is a session of its own and exec.
const LAUNCH: Duration = Duration::from_secs(5);
/// How long a launch that failed waits to collect its launcher.
const REAP: Duration = Duration::from_secs(2);
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

/// What a job start asks for.
pub struct Start {
    pub argv: Vec<String>,
    pub lifetime: Duration,
    pub name: Option<String>,
    pub task: Option<Key>,
    pub session: Option<Key>,
}

/// A start's job once its record is written, before it is launched.
struct Prepared {
    record: Record,
    executable: PathBuf,
    path: String,
    home: Option<PathBuf>,
    lifetime: u64,
    log: PathBuf,
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
    pub fn census(&self) -> Value {
        let state = self.state();
        let launching = state.launching.len();
        json!({
            "running": state.running,
            "launching": launching,
            "unresolved": state.unresolved,
        })
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
                        failure("job", "The helper could not answer about its jobs")
                    })
            }
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

    /// Start a job: write its record, spawn the launcher, let it go on once
    /// its process is recorded, see it become a session and group of its
    /// own and exec the program, check its identity, and reply with it.
    /// The file and process work runs on blocking threads. A launch that
    /// fails before its exec leaves nothing running; one that may have
    /// started something is kept, supervised, and said to be unconfirmed.
    pub async fn start(self: &Arc<Self>, start: Start) -> Value {
        let jobs = self.clone();
        let asked = Start {
            argv: start.argv.clone(),
            lifetime: start.lifetime,
            name: start.name.clone(),
            task: start.task.clone(),
            session: start.session.clone(),
        };
        let prepared = match tokio::task::spawn_blocking(move || jobs.prepare(&asked)).await {
            Ok(Ok(prepared)) => prepared,
            Ok(Err(refusal)) => return refusal,
            Err(_) => return failure("spawn-failed", "The job could not be prepared"),
        };
        let id = prepared.record.id.clone();
        let reply = self.launch(prepared, &start).await;
        self.state().launching.remove(&id);
        reply
    }

    /// Resolve the program, check the limits, and write the job's record
    /// and log: the job, or the refusal.
    fn prepare(&self, start: &Start) -> std::result::Result<Prepared, Value> {
        let Some(program) = start.argv.first() else {
            return Err(failure("spawn-failed", "No command to run"));
        };
        let home = run::home();
        let path = run::search_path(home.as_deref());
        let Some(executable) = run::resolve(program, Some(&path), home.as_deref()) else {
            return Err(run::unresolved(program, home.as_deref()));
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

        let mut state = self.state();
        let records = state.store.records();
        let running: Vec<&Record> = records
            .iter()
            .filter_map(|read| match read {
                Found::Record(record) if record.live() => Some(record.as_ref()),
                _ => None,
            })
            .collect();
        if running.len() + state.launching.len() >= RUNNING_MAX {
            return Err(refusal(
                "limit",
                "16 background jobs already run in the helper; stop one first",
            ));
        }
        if start.task.is_some()
            && running.iter().filter(|job| job.task == start.task).count() >= TASK_MAX
        {
            return Err(refusal(
                "limit",
                "This task already runs 8 background jobs; stop one first",
            ));
        }
        while state.store.holds(&record.id) {
            record.id = JobId::new();
        }
        state
            .store
            .write(&record)
            .map_err(|error| failure(error.code, error.error))?;
        let log = state.store.log_path(&record.id);
        let created = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&log);
        if let Err(error) = created {
            state.store.forget(&record.id);
            return Err(failure(
                "spawn-failed",
                format!("The job's log could not be made: {error}"),
            ));
        }
        state.launching.insert(record.id.clone());
        Ok(Prepared {
            record,
            executable,
            path,
            home,
            lifetime,
            log,
        })
    }

    /// Launch a prepared job, and reply.
    async fn launch(self: &Arc<Self>, prepared: Prepared, start: &Start) -> Value {
        let Prepared {
            mut record,
            executable,
            path,
            home,
            lifetime,
            log,
        } = prepared;
        let program = &start.argv[0];
        let mut argv0 = OsString::from("--argv0=");
        argv0.push(program);
        let mut log_arg = OsString::from("--log=");
        log_arg.push(&log);

        let mut command = tokio::process::Command::new(&self.launcher);
        command
            .arg("job-launch")
            .arg(log_arg)
            .arg(argv0)
            .arg("--")
            .arg(&executable)
            .args(&start.argv[1..])
            .env("PATH", &path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(false);
        if let Some(lang) = run::locale() {
            command.env("LANG", lang);
        }
        if let Some(home) = &home {
            command.current_dir(home);
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                self.forget(&record.id).await;
                return failure("spawn-failed", format!("{program}: {error}"));
            }
        };

        // Read while it is this daemon's unreaped child, so it is exact. The
        // launcher waits on its standard input until its record says so: a
        // daemon that ends before then leaves nothing running.
        let pid = child.id().and_then(|pid| i32::try_from(pid).ok());
        let jobs = self.clone();
        let mut launched_record = record.clone();
        let recorded = tokio::task::spawn_blocking(move || {
            let pid = pid?;
            let started = crate::process::start(pid).ok()?;
            launched_record.pid = Some(pid);
            launched_record.start = Some(started);
            launched_record.pgid = Some(pid);
            launched_record.state = JobState::Launched;
            launched_record.identity()?;
            jobs.state()
                .store
                .write(&launched_record)
                .ok()
                .map(|()| launched_record)
        })
        .await
        .ok()
        .flatten();
        let Some(recorded) = recorded else {
            let _ = child.start_kill();
            let _ = tokio::time::timeout(REAP, child.wait()).await;
            self.forget(&record.id).await;
            return failure(
                "spawn-failed",
                "The job's process could not be observed or recorded",
            );
        };
        record = recorded;
        let Some(identity) = record.identity() else {
            return failure("spawn-failed", "The job's process could not be observed");
        };

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
            let said = format!("{program}: {said}");
            if !setsid {
                // Before its session of its own the launcher is this
                // daemon's child alone, still waiting or exiting: ending it
                // by its own PID is certain, and nothing else ran.
                let _ = child.start_kill();
                return match tokio::time::timeout(REAP, child.wait()).await {
                    Ok(_) => {
                        self.forget(&record.id).await;
                        failure("spawn-failed", said)
                    }
                    Err(_) => {
                        let jobs = self.clone();
                        tokio::task::spawn_blocking(move || {
                            jobs.keep_unconfirmed(record, child, &said)
                        })
                        .await
                        .unwrap_or_else(|_| {
                            failure("job", "The job's start could not be confirmed")
                        })
                    }
                };
            }
            // After it, its group may hold the program's processes too: it
            // is ended like any job, through a fresh look, and kept and
            // supervised unless that look finds nothing of it left.
            return self.abandon_launch(record, child, said).await;
        }

        let jobs = self.clone();
        tokio::task::spawn_blocking(move || jobs.running_now(record, child, lifetime, &identity))
            .await
            .unwrap_or_else(|_| failure("job", "The job's start could not be confirmed"))
    }

    /// The last step of a start: the job's leader checked, its record written
    /// as running with its time limit from now, and, when every job was
    /// stopped while it launched, its stop begun.
    fn running_now(
        &self,
        mut record: Record,
        child: Child,
        lifetime: u64,
        identity: &JobIdentity,
    ) -> Value {
        let leader = self.kernel.process(identity.pid()).ok().flatten();
        let own = leader.as_ref().is_some_and(|leader| {
            leader.instance.start == identity.start()
                && leader.pgid == identity.pgid()
                && leader.owned_by(self.uid)
        });
        if !own {
            return self.keep_unconfirmed(
                record,
                child,
                "The job started, but Atlas could not confirm it runs as the helper in a process group of its own",
            );
        }
        let mut state = self.state();
        record.state = JobState::Running;
        record.ends_at = now_ms().saturating_add(lifetime);
        state.children.insert(record.id.clone(), child);
        let written = state.store.put(&record);
        state.running += 1;
        let processes = self.kernel.of_user(self.uid).ok();
        if let Some((at, why)) = state.stopped_all
            && at >= record.started_at
        {
            self.begin_stop(
                &mut state,
                &mut record,
                why,
                supervise::GRACE_MS,
                processes.as_deref(),
                now_ms(),
            );
        } else {
            // A program that already ended is reported so.
            self.look(
                &mut state,
                &mut record,
                processes.as_deref(),
                false,
                false,
                now_ms(),
            );
        }
        if !written {
            return unconfirmed(&record, "The job runs, but its record could not be written");
        }
        json!({"ok": true, "op": "job", "action": "start", "job": record.view()})
    }

    /// End a launch that failed after its launcher made a session of its
    /// own: KILL through the one signalling path, wait a moment for it, and
    /// forget it only once a fresh look finds nothing of it left. Otherwise
    /// it stays supervised and the reply says its outcome is not known.
    async fn abandon_launch(self: &Arc<Self>, record: Record, child: Child, said: String) -> Value {
        let id = record.id.clone();
        let jobs = self.clone();
        let killing = record.clone();
        let _ = tokio::task::spawn_blocking(move || {
            let mut state = jobs.state();
            state.children.insert(killing.id.clone(), child);
            let _ = jobs.signal(&state, &killing, None, Signal::SIGKILL);
        })
        .await;
        let until = tokio::time::Instant::now() + REAP;
        loop {
            let jobs = self.clone();
            let looked = record.clone();
            let ended = tokio::task::spawn_blocking(move || {
                let mut state = jobs.state();
                let reaped = state
                    .children
                    .get_mut(&looked.id)
                    .is_some_and(|child| matches!(child.try_wait(), Ok(Some(_))));
                if reaped {
                    state.children.remove(&looked.id);
                }
                reaped && jobs.judge(&state, &looked, None).0 == Verdict::Ended
            })
            .await
            .unwrap_or(false);
            if ended {
                self.forget(&id).await;
                return failure("spawn-failed", said);
            }
            if tokio::time::Instant::now() >= until {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let jobs = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut state = jobs.state();
            let mut record = record;
            record.state = JobState::Running;
            state.store.put(&record);
            state.running += 1;
            unconfirmed(&record, &said)
        })
        .await
        .unwrap_or_else(|_| failure("job", "The job's start could not be confirmed"))
    }

    /// Keep `record`'s job, which may run, supervised but unconfirmed: its
    /// record stays, its leader is held, and the reply says its outcome is
    /// not known.
    fn keep_unconfirmed(&self, mut record: Record, child: Child, said: &str) -> Value {
        let mut state = self.state();
        record.state = JobState::Running;
        state.children.insert(record.id.clone(), child);
        state.store.put(&record);
        state.running += 1;
        unconfirmed(&record, said)
    }

    /// Remove `id`'s record and log, on a blocking thread.
    async fn forget(self: &Arc<Self>, id: &JobId) {
        let jobs = self.clone();
        let id = id.clone();
        let _ = tokio::task::spawn_blocking(move || jobs.state().store.forget(&id)).await;
    }

    /// Stop job `id` when it is still what `expect` says: the stop is kept
    /// first, then TERM goes to its group after a fresh look; KILL follows
    /// after `grace` from the supervision, which needs no client.
    pub fn stop(&self, id: &JobId, expect: &Expected, grace: Duration) -> Value {
        let mut state = self.state();
        let mut record = match state.store.read(id) {
            None => return unknown(id),
            Some(Found::Unreadable(_)) => return refusal("unknown", UNREADABLE).with_id(id),
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

        let grace_ms = u64::try_from(grace.as_millis()).unwrap_or(u64::MAX);
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
                json!({"ok": true, "op": "job", "action": "stop",
                    "job": record.view(), "signalled": "SIGTERM"})
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
        state.stopped_all = Some((now, How::of(reason)));
        let (mut stopping, mut ended) = (0, 0);
        let (mut refused, mut unobserved, mut unresolved) = (Vec::new(), Vec::new(), Vec::new());
        for read in state.store.records() {
            let mut record = match read {
                Found::Record(record) => *record,
                // A start is writing it; it is stopped as soon as it runs.
                Found::Unreadable(id) if state.launching.contains(&id) => continue,
                // Never acted on, and never taken to have ended.
                Found::Unreadable(id) => {
                    unresolved.push(json!({"id": id}));
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
                    How::of(reason),
                    supervise::GRACE_MS,
                    processes.as_deref(),
                    now,
                );
            } else if record.state != JobState::Stopping {
                if record.state == JobState::Launched && !state.launching.contains(&record.id) {
                    refused.push(json!({"id": record.id, "reason": "identity"}));
                }
                continue;
            }
            match (
                record.state,
                record
                    .stopping
                    .as_ref()
                    .and_then(|stop| stop.refused.as_deref()),
            ) {
                (JobState::Ended, _) => ended += 1,
                (_, Some("unobserved")) => unobserved.push(json!({"id": record.id})),
                (_, Some(why)) => refused.push(json!({"id": record.id, "reason": why})),
                _ => stopping += 1,
            }
        }
        self.count(&mut state);
        json!({"ok": true, "op": "job", "action": "stop-all", "stopping": stopping,
            "ended": ended, "refused": refused, "unknown": unobserved,
            "unresolved": unresolved})
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
                Found::Unreadable(id) => views.push(json!({"id": id, "state": "unknown",
                    "error": UNREADABLE})),
                Found::Record(mut record) => {
                    self.look(
                        &mut state,
                        &mut record,
                        processes.as_deref(),
                        false,
                        false,
                        now,
                    );
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
        match tail {
            Ok((tail, size)) => json!({"ok": true, "op": "job", "action": "log", "id": id,
                "task": task, "data": STANDARD.encode(&tail), "size": size,
                "outputTruncated": size > tail.len() as u64, "logResets": resets}),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                json!({"ok": true, "op": "job", "action": "log", "id": id, "task": task,
                    "data": "", "size": 0, "outputTruncated": false, "logResets": resets})
            }
            Err(error) => failure("io", format!("The job's log could not be read: {error}")),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// This boot's identity.
fn process_boot() -> Result<String> {
    crate::process::boot().map_err(|error| Error::new("job", error.error))
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

/// The refusal a verdict that sends nothing gives.
fn refused_by(verdict: &Verdict, id: &JobId) -> Value {
    match verdict {
        Verdict::OtherAccount => refusal(
            "other-account",
            "Part of the job belongs to another account, so nothing was signalled",
        )
        .with_id(id),
        Verdict::Unknown => refusal(
            "unobserved",
            "Atlas could not read the job's processes, so nothing was signalled; try again",
        )
        .with_id(id),
        _ => identity(id),
    }
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
