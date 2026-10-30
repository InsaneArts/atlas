//! How a job starts: its record and log written first, then its launcher
//! spawned, recorded and told to go on, then its own session and exec seen
//! and its leader checked. A launch that fails before its exec leaves
//! nothing running; one that may have started something is kept.
use super::{
    Jobs,
    record::{JobIdentity, Record, VERSION},
    reply::unconfirmed,
    store::Found,
    supervise,
    verdict::Verdict,
};
use crate::{
    daemon::{
        protocol::{JobFailure, JobId, JobRefusal, JobReply, JobStart, JobState, failure},
        run,
    },
    error::Code,
    host_session::now_ms,
};
use nix::sys::signal::Signal;
use serde_json::Value;
use std::{
    ffi::OsString,
    fs::OpenOptions,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::Stdio,
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
        let lifetime = crate::time::ms(start.lifetime);
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
            return Err(JobFailure::new(
                JobRefusal::Limit,
                "16 background jobs already run in the helper; stop one first",
            )
            .into_value());
        }
        if start.task.is_some()
            && running.iter().filter(|job| job.task == start.task).count() >= TASK_MAX
        {
            return Err(JobFailure::new(
                JobRefusal::Limit,
                "This task already runs 8 background jobs; stop one first",
            )
            .into_value());
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
        let Prepared {
            record,
            executable,
            path,
            home,
            lifetime,
            log,
        } = prepared;
        let program = &start.argv[0];
        let mut command = self.command(start, &executable, &path, home.as_deref(), &log);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                self.forget(&record.id).await;
                return failure(Code::SpawnFailed, format!("{program}: {error}"));
            }
        };

        let Some(record) = self.record_launched(&record, &child).await else {
            let _ = child.start_kill();
            let _ = tokio::time::timeout(REAP, child.wait()).await;
            self.forget(&record.id).await;
            return failure(
                Code::SpawnFailed,
                "The job's process could not be observed or recorded",
            );
        };
        let Some(identity) = record.identity() else {
            return failure(Code::SpawnFailed, "The job's process could not be observed");
        };

        if let Err(Launch { said, setsid }) = go(&mut child).await {
            let said = format!("{program}: {said}");
            return self.fail_launch(record, child, said, setsid).await;
        }

        let jobs = self.clone();
        tokio::task::spawn_blocking(move || jobs.running_now(record, child, lifetime, &identity))
            .await
            .unwrap_or_else(|_| failure(Code::Job, "The job's start could not be confirmed"))
    }

    /// The launcher's command: it logs to `log`, gives the program its own
    /// name as `argv[0]`, and runs `executable` with the rest of the start's
    /// arguments, in the helper's home, waiting on its standard input to go
    /// on and saying how its launch went on its standard output.
    fn command(
        &self,
        start: &JobStart,
        executable: &Path,
        path: &str,
        home: Option<&Path>,
        log: &Path,
    ) -> Command {
        let mut argv0 = OsString::from("--argv0=");
        argv0.push(&start.argv[0]);
        let mut log_arg = OsString::from("--log=");
        log_arg.push(log);

        let mut command = Command::new(&self.launcher);
        command
            .arg("job-launch")
            .arg(log_arg)
            .arg(argv0)
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
        command
    }

    /// `record` as launched by `child`, its process's identity read and
    /// written, or nothing when it could not be read or recorded.
    async fn record_launched(self: &Arc<Self>, record: &Record, child: &Child) -> Option<Record> {
        // Read while it is this daemon's unreaped child, so it is exact. The
        // launcher waits on its standard input until its record says so: a
        // daemon that ends before then leaves nothing running.
        let pid = child.id().and_then(|pid| i32::try_from(pid).ok());
        let jobs = self.clone();
        let mut launched_record = record.clone();
        tokio::task::spawn_blocking(move || {
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
        .flatten()
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
        if !setsid {
            // Before its session of its own the launcher is this
            // daemon's child alone, still waiting or exiting: ending it
            // by its own PID is certain, and nothing else ran.
            let _ = child.start_kill();
            return match tokio::time::timeout(REAP, child.wait()).await {
                Ok(_) => {
                    self.forget(&record.id).await;
                    failure(Code::SpawnFailed, said)
                }
                Err(_) => {
                    let jobs = self.clone();
                    tokio::task::spawn_blocking(move || jobs.keep_unconfirmed(record, child, &said))
                        .await
                        .unwrap_or_else(|_| {
                            failure(Code::Job, "The job's start could not be confirmed")
                        })
                }
            };
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
                return failure(Code::SpawnFailed, said);
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
        .unwrap_or_else(|_| failure(Code::Job, "The job's start could not be confirmed"))
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
