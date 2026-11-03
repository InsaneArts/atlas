//! How a job starts: its record and log written first, then its launcher
//! spawned, recorded and told to go on, then its own session and exec seen
//! and its leader checked. A launch that fails before its exec leaves
//! nothing running; one that may have started something is kept.
use super::{
    Jobs, State,
    record::{Record, VERSION},
    reply::unconfirmed,
    store::Found,
    verdict::Verdict,
};
use crate::daemon::run;
use atlas_core::{
    error::{Code, Error, Result},
    host::Key,
    protocol::{
        JOB_NAME_MAX, JobFailure, JobId, JobRefusal, JobReply, JobStart, JobState, failure,
    },
    time::now_ms,
};
use nix::sys::signal::Signal;
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    fs::OpenOptions,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::AsyncReadExt,
    process::{Child, Command},
};

/// The most jobs running in the helper at once.
const RUNNING_MAX: usize = 16;
/// The most jobs one task keeps running at once.
const TASK_MAX: usize = 8;
/// How long the launcher has to say it is a session of its own and exec.
const LAUNCH: Duration = Duration::from_secs(5);
/// How long a launch that failed waits to collect its launcher.
const REAP: Duration = Duration::from_secs(2);

/// A start's job once its record is written, before it is launched.
pub(super) struct Prepared {
    pub(super) record: Record,
    executable: PathBuf,
    path: String,
    home: Option<PathBuf>,
    lifetime: u64,
    log: PathBuf,
}

/// Why a launcher could not be recorded, with its record when its identity
/// was read and only writing it failed.
struct Unrecorded {
    error: Error,
    observed: Option<Box<Record>>,
}

impl From<Error> for Unrecorded {
    fn from(error: Error) -> Self {
        Self {
            error,
            observed: None,
        }
    }
}

/// Why a launch failed, and whether the launcher had made its own session
/// by then, so its group may hold more than the launcher.
struct Launch {
    said: String,
    setsid: bool,
}

impl Jobs {
    /// Resolve the program, check the limits, and write the job's record
    /// and log: the job, or the refusal.
    pub(super) fn prepare(&self, start: &JobStart) -> std::result::Result<Prepared, Value> {
        let Some(program) = start.argv.first() else {
            return Err(failure(Code::SpawnFailed, "No command to run"));
        };
        let home = run::home();
        let (executable, path) = run::program(program, home.as_deref())?;
        let name = start.name.clone().unwrap_or_else(|| {
            Path::new(program)
                .file_name()
                .map(|name| name.to_string_lossy().chars().take(JOB_NAME_MAX).collect())
                .filter(|name: &String| !name.is_empty())
                .unwrap_or_else(|| "job".to_owned())
        });
        let now = now_ms();
        let lifetime = atlas_core::time::ms(start.lifetime);
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

        let mut state = self.state().map_err(|error| json!(error))?;
        within_limits(&state, start.task.as_ref())?;
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
                Code::SpawnFailed,
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
    pub(super) async fn launch(self: &Arc<Self>, prepared: Prepared, start: &JobStart) -> Value {
        let program = &start.argv[0];
        let mut command = self.command(start, &prepared);
        let (record, lifetime) = (prepared.record, prepared.lifetime);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                if let Err(error) = self.forget(&record.id).await {
                    return json!(error);
                }
                return failure(Code::SpawnFailed, format!("{program}: {error}"));
            }
        };

        let record = match self.record_launched(&record, &child).await {
            Ok(record) => record,
            Err(Unrecorded { error, observed }) if error.code == Code::SpawnFailed => {
                if self.end_launcher(&mut child).await {
                    return self.forgotten(&record, error.error).await;
                }
                return self
                    .keep_unrecorded(record, observed, child, error.error)
                    .await;
            }
            Err(Unrecorded { error, .. }) => {
                self.retain_unobserved_child(record.id.clone(), child);
                return unconfirmed(&record, &error.error);
            }
        };
        let Some(leader) = record.identity() else {
            return failure(Code::SpawnFailed, "The job's process could not be observed");
        };

        if let Err(Launch { said, setsid }) = go(&mut child).await {
            let said = format!("{program}: {said}");
            return self.fail_launch(record, child, said, setsid).await;
        }

        let jobs = self.clone();
        self.launch_reply(
            tokio::task::spawn_blocking(move || jobs.running_now(record, child, lifetime, leader))
                .await,
        )
    }

    /// The launcher's command: it logs to the job's log, gives the program
    /// its own name as `argv[0]`, and runs the executable with the rest of
    /// the start's arguments, in the helper's home, waiting on its standard
    /// input to go on and saying how its launch went on its standard output.
    fn command(&self, start: &JobStart, prepared: &Prepared) -> Command {
        let mut argv0 = OsString::from("--argv0=");
        argv0.push(&start.argv[0]);
        let mut log_arg = OsString::from("--log=");
        log_arg.push(&prepared.log);

        let mut command = Command::new(&self.launcher);
        command
            .arg("job-launch")
            .arg(log_arg)
            .arg(argv0)
            .arg("--")
            .arg(&prepared.executable)
            .args(&start.argv[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(false);
        run::in_helper(&mut command, &prepared.path, prepared.home.as_deref());
        command
    }

    /// `record` as launched by `child`, its process's identity read and
    /// written, or why it could not be read or recorded, with the record
    /// once its identity was read.
    async fn record_launched(
        self: &Arc<Self>,
        record: &Record,
        child: &Child,
    ) -> std::result::Result<Record, Unrecorded> {
        // Read while it is this daemon's unreaped child, so it is exact. The
        // launcher waits on its standard input until its record says so: a
        // daemon that ends before then leaves nothing running.
        let pid = child.id().and_then(|pid| i32::try_from(pid).ok());
        let jobs = self.clone();
        let mut launched_record = record.clone();
        tokio::task::spawn_blocking(move || {
            let state = jobs.state()?;
            let refused = || {
                Error::new(
                    Code::SpawnFailed,
                    "The job's process could not be observed or recorded",
                )
            };
            let pid = pid.ok_or_else(refused)?;
            let started =
                atlas_core::process::start(pid).map_err(|error| refused().with_source(error))?;
            launched_record.pid = Some(pid);
            launched_record.start = Some(started);
            launched_record.pgid = Some(pid);
            launched_record.state = JobState::Launched;
            launched_record.identity().ok_or_else(refused)?;
            match state.store.write(&launched_record) {
                Ok(()) => Ok(launched_record),
                Err(error) => Err(Unrecorded {
                    error: refused().with_source(error),
                    observed: Some(Box::new(launched_record)),
                }),
            }
        })
        .await
        .map_err(|error| {
            self.fail_supervision();
            Self::unavailable().with_source(error)
        })?
    }

    /// Keep a launcher that could not be recorded and may not have been
    /// reaped, and reply that its outcome is not known: by the record its
    /// observed identity gives, held in memory when it cannot be written,
    /// or as a child whose identity was never observed.
    async fn keep_unrecorded(
        self: &Arc<Self>,
        record: Record,
        observed: Option<Box<Record>>,
        child: Child,
        said: String,
    ) -> Value {
        let Some(observed) = observed else {
            self.retain_unobserved_child(record.id.clone(), child);
            return unconfirmed(&record, &said);
        };
        let jobs = self.clone();
        self.launch_reply(
            tokio::task::spawn_blocking(move || match jobs.holding(&observed.id, child) {
                Ok(mut state) => keep_running(&mut state, *observed, &said),
                Err(error) => unconfirmed(&observed, &error.error),
            })
            .await,
        )
    }

    /// End a launch that failed with `said`, and reply. `setsid` says
    /// whether the launcher had made a session of its own by then.
    async fn fail_launch(
        self: &Arc<Self>,
        record: Record,
        mut child: Child,
        said: String,
        setsid: bool,
    ) -> Value {
        if let Err(error) = self.state() {
            self.retain_unobserved_child(record.id.clone(), child);
            return unconfirmed(&record, &error.error);
        }
        if !setsid {
            // Before its session of its own the launcher is this
            // daemon's child alone, still waiting or exiting: ending it
            // by its own PID is certain, and nothing else ran.
            if self.end_launcher(&mut child).await {
                return self.forgotten(&record, said).await;
            }
            let jobs = self.clone();
            return self.launch_reply(
                tokio::task::spawn_blocking(move || match jobs.holding(&record.id, child) {
                    Ok(mut state) => keep_running(&mut state, record, &said),
                    Err(error) => unconfirmed(&record, &error.error),
                })
                .await,
            );
        }
        // After it, its group may hold the program's processes too: it
        // is ended like any job, through a fresh look, and kept and
        // supervised unless that look finds nothing of it left.
        self.abandon_launch(record, child, said).await
    }

    /// The last step of a start: the job's leader checked, its record written
    /// as running with its time limit from now, and, when every job was
    /// stopped while it launched, its stop begun.
    fn running_now(
        &self,
        mut record: Record,
        child: Child,
        lifetime: u64,
        identity: atlas_core::process::Instance,
    ) -> Value {
        let mut state = match self.holding(&record.id, child) {
            Ok(state) => state,
            Err(error) => return unconfirmed(&record, &error.error),
        };
        let leader = self.kernel.process(identity.pid).ok().flatten();
        let own = leader.as_ref().is_some_and(|leader| {
            leader.instance.start == identity.start
                && leader.pgid == identity.pid
                && leader.owned_by(self.uid)
        });
        if !own {
            return keep_running(
                &mut state,
                record,
                "The job started, but Atlas could not confirm it runs as the helper in a process group of its own",
            );
        }
        record.state = JobState::Running;
        record.ends_at = now_ms().saturating_add(lifetime);
        let written = state.store.put(&record);
        state.running += 1;
        let processes = self.processes();
        if let Some((at, why)) = state.stopped_all
            && at >= record.started_at
        {
            self.begin_stop(&mut state, &mut record, why, processes.as_deref(), now_ms());
        } else {
            // A program that already ended is reported so.
            self.look(
                &mut state,
                &mut record,
                processes.as_deref(),
                None,
                now_ms(),
            );
        }
        if !written {
            return unconfirmed(&record, "The job runs, but its record could not be written");
        }
        JobReply::Start { job: record.view() }.into_value()
    }

    /// End a launch that failed after its launcher made a session of its
    /// own: KILL through the one signalling path, wait a moment for it, and
    /// forget it only once a fresh look finds nothing of it left. Otherwise
    /// it stays supervised and the reply says its outcome is not known.
    async fn abandon_launch(self: &Arc<Self>, record: Record, child: Child, said: String) -> Value {
        let jobs = self.clone();
        let killing = record.clone();
        let outcome = tokio::task::spawn_blocking(move || -> Result<()> {
            let state = jobs.holding(&killing.id, child)?;
            let _ = jobs.signal(&state, &killing, None, Signal::SIGKILL);
            Ok(())
        })
        .await;
        if let Err(error) = self.worker_outcome(outcome) {
            return unconfirmed(&record, &error.error);
        }
        let until = tokio::time::Instant::now() + REAP;
        loop {
            let jobs = self.clone();
            let looked = record.clone();
            let ended = tokio::task::spawn_blocking(move || -> Result<bool> {
                let mut state = jobs.state()?;
                let reaped = state
                    .children
                    .get_mut(&looked.id)
                    .is_some_and(|child| matches!(child.try_wait(), Ok(Some(_))));
                if reaped {
                    state.children.remove(&looked.id);
                }
                Ok(reaped && jobs.judge(&state, &looked, None) == Verdict::Ended)
            })
            .await;
            let ended = match ended {
                Ok(Ok(ended)) => ended,
                Ok(Err(error)) => return unconfirmed(&record, &error.error),
                Err(_) => {
                    self.fail_supervision();
                    return unconfirmed(&record, &Self::unavailable().error);
                }
            };
            if ended {
                return self.forgotten(&record, said).await;
            }
            if tokio::time::Instant::now() >= until {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let jobs = self.clone();
        self.launch_reply(
            tokio::task::spawn_blocking(move || match jobs.state() {
                Ok(mut state) => keep_running(&mut state, record, &said),
                Err(error) => unconfirmed(&record, &error.error),
            })
            .await,
        )
    }

    fn launch_reply(&self, outcome: std::result::Result<Value, tokio::task::JoinError>) -> Value {
        outcome.unwrap_or_else(|_| {
            self.fail_supervision();
            json!(Self::unavailable().uncertain())
        })
    }

    /// Forget a launch that left nothing running, and refuse it with `said`.
    async fn forgotten(self: &Arc<Self>, record: &Record, said: String) -> Value {
        match self.forget(&record.id).await {
            Ok(()) => failure(Code::SpawnFailed, said),
            Err(error) => unconfirmed(record, &error.error),
        }
    }

    /// Remove `id`'s record and log, on a blocking thread.
    async fn forget(self: &Arc<Self>, id: &JobId) -> Result<()> {
        let jobs = self.clone();
        let id = id.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            jobs.state()?.store.forget(&id);
            Ok(())
        })
        .await;
        self.worker_outcome(outcome)
    }
}

/// How waiting for a launcher goes wrong in a test.
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(super) enum WaitFault {
    /// Waiting returns an error.
    Fails,
    /// Waiting never comes to an end.
    Hangs,
}

impl Jobs {
    /// Kill a launcher that is this daemon's child alone: whether waiting for
    /// it returned its exit status in time. A wait that fails or runs out
    /// proves nothing about the launcher, which then stays this daemon's.
    async fn end_launcher(&self, child: &mut Child) -> bool {
        let _ = child.start_kill();
        matches!(
            tokio::time::timeout(REAP, self.wait_launcher(child)).await,
            Ok(Ok(_))
        )
    }

    async fn wait_launcher(&self, child: &mut Child) -> std::io::Result<ExitStatus> {
        #[cfg(test)]
        {
            let fault = *self
                .wait_fault
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match fault {
                Some(WaitFault::Fails) => return Err(std::io::Error::other("wait failed")),
                Some(WaitFault::Hangs) => return std::future::pending().await,
                None => {}
            }
        }
        child.wait().await
    }
}

/// Refuse a start past the limit on jobs running, in the helper or for `task`,
/// or when the records cannot all be read, so the jobs running are not known.
fn within_limits(state: &State, task: Option<&Key>) -> std::result::Result<(), Value> {
    let scan = state.inventory();
    if !scan.complete {
        return Err(JobFailure::new(
            JobRefusal::Unobserved,
            "Atlas could not read every background job record, so it started no job; try again",
        )
        .into_value());
    }
    let running = || {
        scan.found
            .iter()
            .filter_map(|read| match read {
                Found::Record(record) => Some(record),
                Found::Unreadable(_) => None,
            })
            .filter(|record| record.state.live())
    };
    let refused = |said: &str| JobFailure::new(JobRefusal::Limit, said).into_value();
    if running().count() + state.launching.len() >= RUNNING_MAX {
        return Err(refused(
            "16 background jobs already run in the helper; stop one first",
        ));
    }
    if task.is_some() && running().filter(|job| job.task.as_ref() == task).count() >= TASK_MAX {
        return Err(refused(
            "This task already runs 8 background jobs; stop one first",
        ));
    }
    Ok(())
}

/// Keep `record`'s job, which may run, supervised: the reply says `said` and
/// that its outcome is not known.
fn keep_running(state: &mut State, mut record: Record, said: &str) -> Value {
    record.state = JobState::Running;
    state.store.put(&record);
    state.running += 1;
    unconfirmed(&record, said)
}

/// Tell `child`'s launcher to go on, and wait for it to say it launched.
async fn go(child: &mut Child) -> std::result::Result<(), Launch> {
    let go = match child.stdin.take() {
        Some(mut go) => tokio::io::AsyncWriteExt::write_all(&mut go, b"go\n")
            .await
            .is_ok(),
        None => false,
    };
    if go {
        launched(child).await
    } else {
        Err(Launch {
            said: "the job's launcher could not be told to go on".into(),
            setsid: false,
        })
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
