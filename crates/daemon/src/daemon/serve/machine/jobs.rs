//! A machine desktop's background jobs: supervised work no lane owns. Each
//! is started as a change with an ID, runs under its supervisor with its
//! output in a log of its own, and ends with its generation, a Stop, the
//! daemon going, the screen leaving, its lifetime, or its own stop, which
//! gives it its grace. A job is ended only once its keeper has proved its
//! work ended; until then, and for any record no keeper owns, it is shown
//! as running, stopping or unknown, never as ended.
use super::super::{Daemon, Gate, uncertain};
use crate::{
    daemon::run::{self, Heard, Lease, Owned},
    machine::processes::{JobRecord, Remembered},
};
use atlas_core::{
    files::Token,
    protocol::{
        EndedView, Expected, How, JobFailure, JobId, JobRefusal, JobReply, JobStart, JobState,
        JobView, Logged, Progress, Showing, Shown, StopCameTo, Stopped, StoppingView, Unread,
        Unreadable,
    },
};
use nix::{
    fcntl::{AtFlags, OFlag, openat},
    sys::stat::{Mode, SFlag, fstat, fstatat},
    unistd::{UnlinkatFlags, unlinkat},
};
use serde_json::Value;
use std::{
    collections::{HashMap, VecDeque},
    ffi::CString,
    fs::File,
    io::{Read, Seek, SeekFrom},
    os::fd::OwnedFd,
    path::Path,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};
use tokio::sync::{mpsc, oneshot, watch};

/// The most jobs running at once, and for one task.
const RUNNING_MAX: usize = 16;
const TASK_MAX: usize = 8;

/// The most ended jobs remembered.
const ENDED_MAX: usize = 64;

/// A log past this is emptied.
const LOG_LIMIT: u64 = 32 << 20;

/// How long a start waits for its program to be started, and a stop for
/// its supervisor to say it has begun.
const STARTING: Duration = Duration::from_secs(10);
const STOPPING: Duration = Duration::from_secs(2);

/// A machine desktop's jobs, their logs in a private folder held open.
pub(crate) struct Jobs {
    folder: File,
    pub(super) uid: u32,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    running: HashMap<JobId, Arc<Running>>,
    /// Jobs done with, newest last: ended, or of an end not proved.
    done: VecDeque<Done>,
    /// Places held, each from before its job's spawn until its keeper
    /// judged it: jobs starting, running and being ended alike.
    launching: usize,
    tasks: HashMap<atlas_core::host::Key, usize>,
}

/// A job done with.
enum Done {
    Ended(Box<JobView>),
    /// Its keeper could not prove its work ended: unknown until recovery.
    Unknown(JobId),
}

impl Done {
    fn id(&self) -> &JobId {
        match self {
            Self::Ended(view) => &view.id,
            Self::Unknown(id) => id,
        }
    }
}

/// One job running, or being ended.
struct Running {
    record: JobRecord,
    boot: String,
    identity: atlas_core::process::Instance,
    stops: mpsc::UnboundedSender<u16>,
    /// Told once its supervisor says a stop has begun.
    stopping: watch::Sender<bool>,
    going: Mutex<Going>,
    /// What ended its lease, as it did: none while none has.
    cause: Arc<Mutex<Option<How>>>,
}

/// How a job is going.
#[derive(Default)]
struct Going {
    /// Its program's own end, once its supervisor said it.
    program: Option<std::process::ExitStatus>,
    /// A stop was asked of it.
    asked: bool,
    log_resets: u32,
}

/// A place among the jobs running, held from before the spawn until the
/// job is judged: dropped, it is free again.
struct Place {
    jobs: Arc<Jobs>,
    task: Option<atlas_core::host::Key>,
}

impl Drop for Place {
    fn drop(&mut self) {
        let mut state = self.jobs.state();
        state.launching = state.launching.saturating_sub(1);
        if let Some(task) = &self.task
            && let Some(count) = state.tasks.get_mut(task)
        {
            *count = count.saturating_sub(1);
            if *count == 0 {
                state.tasks.remove(task);
            }
        }
    }
}

fn refusal(reason: JobRefusal, error: &str) -> Value {
    JobFailure::new(reason, error).into_value()
}

fn unconfirmed(id: &JobId, error: &str) -> Value {
    let mut failure = JobFailure::new(JobRefusal::Unconfirmed, error).about(id);
    failure.uncertain = true;
    failure.into_value()
}

fn log_name(id: &JobId) -> CString {
    CString::new(format!("{id}.log")).unwrap_or_default()
}

fn sys<T>(result: nix::Result<T>) -> std::io::Result<T> {
    result.map_err(std::io::Error::from)
}

impl Jobs {
    /// The jobs of the account `uid`, their logs in the folder at `path`,
    /// made private if it is not there and opened once: never a link, and
    /// only this account's own private folder.
    ///
    /// # Errors
    ///
    /// Fails when it cannot be made, or is not such a folder.
    pub(crate) fn open(path: &Path, uid: u32) -> std::io::Result<Self> {
        match nix::unistd::mkdir(path, Mode::S_IRWXU) {
            Ok(()) | Err(nix::errno::Errno::EEXIST) => {}
            Err(error) => return Err(error.into()),
        }
        let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
        let folder = File::from(sys(nix::fcntl::open(path, flags, Mode::empty()))?);
        let stat = sys(fstat(&folder))?;
        let private = SFlag::from_bits_truncate(stat.st_mode & SFlag::S_IFMT.bits())
            == SFlag::S_IFDIR
            && stat.st_uid == uid
            && u32::from(stat.st_mode) & 0o077 == 0;
        if !private {
            return Err(std::io::Error::other(
                "The jobs' folder is not this account's private folder",
            ));
        }
        Ok(Self {
            folder,
            uid,
            state: Mutex::default(),
        })
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether no job runs, starts or is being ended, and none ended
    /// without its end proved.
    pub(crate) fn idle(&self) -> bool {
        let state = self.state();
        state.running.is_empty()
            && state.launching == 0
            && !state
                .done
                .iter()
                .any(|done| matches!(done, Done::Unknown(_)))
    }

    /// How many jobs run, start or are being ended: none when one ended
    /// without its end proved, which may run still.
    pub(crate) fn running(&self) -> Option<usize> {
        let state = self.state();
        let unproved = state
            .done
            .iter()
            .any(|done| matches!(done, Done::Unknown(_)));
        (!unproved).then(|| state.launching.max(state.running.len()))
    }

    /// Open job `id`'s log with `flags`, relative to the folder, never
    /// through a link, and only as a regular file.
    fn open_log(&self, id: &JobId, flags: OFlag) -> std::io::Result<File> {
        let flags = flags | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC;
        let log = File::from(sys(openat(
            &self.folder,
            log_name(id).as_c_str(),
            flags,
            Mode::S_IRUSR | Mode::S_IWUSR,
        ))?);
        let stat = sys(fstat(&log))?;
        if SFlag::from_bits_truncate(stat.st_mode & SFlag::S_IFMT.bits()) != SFlag::S_IFREG {
            return Err(std::io::Error::other("A job's log is not a file"));
        }
        Ok(log)
    }

    fn remove_log(&self, id: &JobId) {
        let _ = unlinkat(
            &self.folder,
            log_name(id).as_c_str(),
            UnlinkatFlags::NoRemoveDir,
        );
    }

    fn log_exists(&self, id: &JobId) -> bool {
        fstatat(
            &self.folder,
            log_name(id).as_c_str(),
            AtFlags::AT_SYMLINK_NOFOLLOW,
        )
        .is_ok()
    }

    /// Remove every log in the folder that no job `kept` names. Run before
    /// any job may start, so none of its own logs is ever among them.
    pub(crate) fn sweep(&self, kept: &dyn Remembered) -> Vec<JobId> {
        let Ok(entries) = kept.kept() else {
            return Vec::new();
        };
        let ids: Vec<JobId> = entries
            .into_iter()
            .filter_map(|entry| entry.job)
            .map(|job| job.id)
            .collect();
        let named: Vec<String> = ids.iter().map(|id| format!("{id}.log")).collect();
        let Ok(folder) = self.folder.try_clone() else {
            return ids;
        };
        let Ok(mut listing) = nix::dir::Dir::from_fd(OwnedFd::from(folder)) else {
            return ids;
        };
        let stray: Vec<CString> = listing
            .iter()
            .flatten()
            .map(|entry| entry.file_name().to_owned())
            .filter(|name| {
                let text = name.to_string_lossy();
                text.ends_with(".log") && !named.iter().any(|kept| *kept == text)
            })
            .collect();
        for name in stray {
            let _ = unlinkat(&self.folder, name.as_c_str(), UnlinkatFlags::NoRemoveDir);
        }
        ids
    }

    /// Take a place for a job of `task`, and a fresh ID none other has.
    fn reserve(
        self: &Arc<Self>,
        task: Option<&atlas_core::host::Key>,
    ) -> Result<(Place, JobId), Value> {
        let mut state = self.state();
        if state.launching >= RUNNING_MAX {
            return Err(refusal(
                JobRefusal::Limit,
                "As many jobs run as may on this desktop",
            ));
        }
        if let Some(task) = task
            && state.tasks.get(task).copied().unwrap_or(0) >= TASK_MAX
        {
            return Err(refusal(
                JobRefusal::Limit,
                "As many jobs run for this task as may",
            ));
        }
        let id = loop {
            let id = JobId::new();
            let taken = state.running.contains_key(&id)
                || state.done.iter().any(|done| *done.id() == id)
                || self.log_exists(&id);
            if !taken {
                break id;
            }
        };
        state.launching += 1;
        if let Some(task) = task {
            *state.tasks.entry(task.clone()).or_default() += 1;
        }
        Ok((
            Place {
                jobs: self.clone(),
                task: task.cloned(),
            },
            id,
        ))
    }

    /// Start `start` as the change `change`, admitted by `gate`, on
    /// `daemon`: its reply, a whole job once its program started, or why it
    /// did not. Whatever it comes to, its place and its record are kept
    /// until its keeper has judged its work.
    pub(crate) async fn start(
        self: &Arc<Self>,
        daemon: &Arc<Daemon>,
        gate: Gate,
        change: Token,
        start: JobStart,
        policy: atlas_core::protocol::Policy,
    ) -> Value {
        // Admitted as every request is: its console and its session pin.
        let screen = &daemon.screen;
        if let Some(refused) = policy.check(screen.console, screen.on_console, screen.session) {
            return refused;
        }
        let Some(supervising) = daemon.supervising.as_ref() else {
            return refusal(
                JobRefusal::Unknown,
                "This desktop cannot keep background jobs",
            );
        };
        if *daemon.stopping.borrow() {
            return refusal(JobRefusal::Unknown, "The desktop's daemon is stopping");
        }
        let (place, id) = match self.reserve(start.task.as_ref()) {
            Ok(reserved) => reserved,
            Err(refused) => return refused,
        };
        let log = match self.open_log(
            &id,
            OFlag::O_WRONLY | OFlag::O_APPEND | OFlag::O_CREAT | OFlag::O_EXCL,
        ) {
            Ok(log) => log,
            Err(_) => return refusal(JobRefusal::Unknown, "The job's log could not be made"),
        };
        let now = atlas_core::time::now_ms();
        let lifetime = start.lifetime;
        let ends_at = now.saturating_add(u64::try_from(lifetime.as_millis()).unwrap_or(u64::MAX));
        let name = start.name.clone().unwrap_or_else(|| {
            Path::new(&start.argv[0]).file_name().map_or_else(
                || start.argv[0].clone(),
                |name| name.to_string_lossy().into_owned(),
            )
        });
        let record = JobRecord {
            id: id.clone(),
            name,
            task: start.task.clone(),
            session: start.session.clone(),
            started_at: now,
            ends_at,
            change,
        };
        let deadline = tokio::time::Instant::now() + lifetime;
        let (stops, stopping_rx) = mpsc::unbounded_channel();
        let (judged, judging) = oneshot::channel();
        let cause = Arc::new(Mutex::new(None));
        let over = {
            let (over, stopping, away) = (gate.over(), daemon.clone(), daemon.clone());
            let (authority, ended_by) = (gate.authority.clone(), cause.clone());
            Box::pin(async move {
                // Kept as it happens: what ended it, not what is guessed after.
                let how = tokio::select! {
                    // Its generation retired (a lapse, control lost, a revoke or
                    // a newer one), or else a person's Stop.
                    () = over => {
                        if authority.retired().is_some() { How::Gone } else { How::Person }
                    }
                    () = tokio::time::sleep_until(deadline) => How::Timeout,
                    () = stopping.stopped() => How::Gone,
                    () = away.moved_away() => How::LeftScreen,
                };
                ended_by
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .get_or_insert(how);
            })
        };
        let admitted = {
            let (still, keepers, daemon) = (gate.clone(), daemon.keepers.clone(), daemon.clone());
            Box::new(move || {
                if tokio::time::Instant::now() >= deadline {
                    return Some(refusal(
                        JobRefusal::Unknown,
                        "The job's lifetime ran out before it started",
                    ));
                }
                if *daemon.stopping.borrow() || daemon.screen.away() {
                    return Some(refusal(
                        JobRefusal::Unknown,
                        "The desktop's screen or daemon went",
                    ));
                }
                // Its console and pin, again, right before its program starts.
                let screen = &daemon.screen;
                policy
                    .check(screen.console, screen.on_console, screen.session)
                    .or_else(|| still.refused())
                    .or_else(|| uncertain(&keepers))
            })
        };
        let lease = Lease {
            supervisor: supervising.supervisor.clone(),
            proofs: supervising.proofs.clone(),
            generation: gate.authority.generation(),
            over,
            admitted,
            kept: supervising.kept.clone(),
            keepers: daemon.keepers.clone(),
            stops: Some(stopping_rx),
            judged: Some(judged),
            change: Some(record.change.clone()),
            carrying: Some(daemon.keepers.carry(&record.change)),
            job: Some(record.clone()),
            printed: None,
        };
        let spawned = run::spawn(
            &start.argv,
            run::home().as_deref(),
            &|| None,
            Some(lease),
            Some(OwnedFd::from(log)),
        )
        .await;
        let spawned = match spawned {
            Ok(spawned) => spawned,
            Err(refused) => {
                self.remove_log(&id);
                return refused;
            }
        };
        let run::Spawned {
            child,
            told,
            supervisor,
            ..
        } = spawned;
        let Some(mut told) = told else {
            return unconfirmed(&id, "The job's supervisor could not be heard");
        };
        let heard = tokio::time::timeout(STARTING, told.hear()).await;
        // Read by the spawn while the supervisor was still held unreaped.
        let identity = supervisor;
        // Let go of it now: its keeper reaps it whenever it ends, and hands
        // its verdict to whoever follows the job.
        if let Owned::Kept(released) = child {
            drop(released);
        }
        let Some(identity) = identity else {
            tokio::spawn(self.clone().settle(id.clone(), place, judging));
            return unconfirmed(&id, "The job's supervisor could not be read");
        };
        // Said to have started, or not yet heard: a program that may still
        // start is followed as one running. One whose supervisor is gone, or
        // refused, never ran as one.
        let confirmed = match heard {
            Ok(Heard::Started) => true,
            Err(_) => false,
            Ok(_) => {
                let reply = match told.verdict.try_recv() {
                    Ok(run::keeper::Verdict::Refused(refused)) => refused,
                    _ => unconfirmed(&id, "The job's start could not be confirmed"),
                };
                tokio::spawn(self.clone().settle(id, place, judging));
                return reply;
            }
        };
        let running = Arc::new(Running {
            record,
            boot: atlas_core::process::boot().unwrap_or_default(),
            identity,
            stops,
            stopping: watch::Sender::new(false),
            going: Mutex::default(),
            cause,
        });
        self.state().running.insert(id.clone(), running.clone());
        let view = self.view(&running);
        tokio::spawn(self.clone().follow(running, told, judging, place));
        if !confirmed {
            return unconfirmed(&id, "The job's start could not be confirmed");
        }
        JobReply::Start { job: view }.into_value()
    }

    /// Hold a job that never ran as one, its `place` and its log, until its
    /// keeper has judged its work: then it is done with.
    async fn settle(self: Arc<Self>, id: JobId, place: Place, judging: oneshot::Receiver<bool>) {
        let proven = judging.await.unwrap_or(false);
        self.finish(Done::Unknown(id.clone()), proven, &id);
        drop(place);
        if !proven {
            tokio::spawn(self.keep_bounding(id, false));
        }
    }

    /// Follow a running job until its keeper has judged its end: what its
    /// supervisor says, its log's bound, and then how it ended. Every frame
    /// it said is read before it is done with.
    async fn follow(
        self: Arc<Self>,
        running: Arc<Running>,
        mut told: run::Told,
        judging: oneshot::Receiver<bool>,
        place: Place,
    ) {
        let mut judging = std::pin::pin!(judging);
        let mut bounding = tokio::time::interval(Duration::from_secs(1));
        let mut hearing = true;
        let proven = loop {
            tokio::select! {
                biased;
                heard = told.hear(), if hearing => hearing = running.heard(heard),
                _ = bounding.tick() => self.bound_log(&running),
                proven = &mut judging => break proven.unwrap_or(false),
            }
        };
        // Whatever the supervisor said before it went is read first.
        while hearing {
            match tokio::time::timeout(Duration::from_secs(1), told.hear()).await {
                Ok(heard) => hearing = running.heard(heard),
                Err(_) => break,
            }
        }
        let bounded = self.bound(&running.record.id);
        if matches!(bounded, Ok(true)) {
            running.going().log_resets += 1;
        }
        let id = running.record.id.clone();
        let done = if proven {
            Done::Ended(Box::new(self.ended_view(&running)))
        } else {
            Done::Unknown(id.clone())
        };
        self.state().running.remove(&id);
        self.finish(done, proven, &id);
        drop(place);
        // What it left may still write, or its last bounding failed: its log
        // stays bounded.
        if !proven || bounded.is_err() {
            tokio::spawn(self.keep_bounding(id, proven));
        }
    }

    /// Keep `done` among the jobs done with, the oldest forgotten past the
    /// bound with its log. A job proved ended that never ran keeps no log.
    fn finish(&self, done: Done, proven: bool, id: &JobId) {
        let never_ran = matches!(done, Done::Unknown(_)) && proven;
        let mut state = self.state();
        if never_ran {
            drop(state);
            self.remove_log(id);
            return;
        }
        state.done.push_back(done);
        while state.done.len() > ENDED_MAX {
            if let Some(forgotten) = state.done.pop_front() {
                self.remove_log(forgotten.id());
            }
        }
    }

    /// Empty the job's log once it is past its limit, counting it.
    fn bound_log(&self, running: &Running) {
        if matches!(self.bound(&running.record.id), Ok(true)) {
            running.going().log_resets += 1;
        }
    }

    /// Empty job `id`'s log if it is past its limit: whether it did. A log
    /// that is gone needs nothing.
    fn bound(&self, id: &JobId) -> std::io::Result<bool> {
        let log = match self.open_log(id, OFlag::O_WRONLY) {
            Ok(log) => log,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        if log.metadata()?.len() <= LOG_LIMIT {
            return Ok(false);
        }
        log.set_len(0)?;
        Ok(true)
    }

    /// Keep bounding job `id`'s log while it is kept among the jobs done
    /// with: for good while its work may still run, or, once `proven` ended,
    /// until a bounding succeeds.
    async fn keep_bounding(self: Arc<Self>, id: JobId, proven: bool) {
        let mut every = tokio::time::interval(Duration::from_secs(1));
        loop {
            every.tick().await;
            let kept = self.state().done.iter().any(|done| *done.id() == id);
            if !kept {
                return;
            }
            if self.bound(&id).is_ok() && proven {
                return;
            }
        }
    }

    /// Keep bounding the logs of jobs `ids`, kept in `kept` from before this
    /// daemon, whose work may still run: each until its record goes.
    pub(crate) fn bound_kept(self: &Arc<Self>, ids: Vec<JobId>, kept: &Arc<dyn Remembered>) {
        for id in ids {
            let (jobs, kept) = (self.clone(), kept.clone());
            tokio::spawn(async move {
                let mut every = tokio::time::interval(Duration::from_secs(1));
                for tick in 0_u64.. {
                    every.tick().await;
                    let _ = jobs.bound(&id);
                    // The record looked at again now and then: a Keychain read.
                    if tick % 30 == 29 {
                        let (looked_for, reading) = (id.clone(), kept.clone());
                        let still = tokio::task::spawn_blocking(move || {
                            reading.kept().map(|entries| {
                                entries.iter().any(|entry| {
                                    entry.job.as_ref().is_some_and(|job| job.id == looked_for)
                                })
                            })
                        })
                        .await;
                        if matches!(still, Ok(Ok(false))) {
                            return;
                        }
                    }
                }
            });
        }
    }

    fn view(&self, running: &Running) -> JobView {
        let going = running.going();
        let (state, progress) = if going.asked {
            (
                JobState::Stopping,
                Progress::Stopping(StoppingView {
                    signal: Some("SIGTERM".into()),
                    remaining: None,
                    refused: None,
                }),
            )
        } else {
            (JobState::Running, Progress::Neither {})
        };
        JobView {
            id: running.record.id.clone(),
            name: running.record.name.clone(),
            task: running.record.task.clone(),
            session: running.record.session.clone(),
            uid: self.uid,
            boot: running.boot.clone(),
            pid: Some(running.identity.pid),
            start: Some(running.identity.start),
            pgid: Some(running.identity.pid),
            state,
            started_at: running.record.started_at,
            ends_at: running.record.ends_at,
            log_resets: going.log_resets,
            adopted: false,
            progress,
        }
    }

    /// How a job proved ended came to: its own program, its stop, its
    /// lifetime, or what ended its generation.
    fn ended_view(&self, running: &Running) -> JobView {
        use std::os::unix::process::ExitStatusExt;
        let mut view = self.view(running);
        let going = running.going();
        let now = atlas_core::time::now_ms();
        // What ended its lease, as it did; else its own stop; else its own
        // program, which ended with everything it left.
        let cause = *running.cause.lock().unwrap_or_else(PoisonError::into_inner);
        let how = cause.unwrap_or(if going.asked {
            How::Stopped
        } else if going.program.is_some() {
            How::Exited
        } else {
            How::Gone
        });
        view.state = JobState::Ended;
        view.progress = Progress::Ended(EndedView {
            how,
            ended_at: now,
            signal: going.asked.then(|| "SIGTERM".into()),
            exit_code: going.program.and_then(|status| status.code()),
            exit_signal: going
                .program
                .and_then(|status| status.signal())
                .map(|signal| format!("{signal}")),
            left: Vec::new(),
            remaining: None,
        });
        view
    }

    /// Stop job `id`, while it is still what `expect` names and `gate`
    /// still admits it, with `grace`: said once its supervisor has begun.
    pub(crate) async fn stop(
        &self,
        gate: &Gate,
        id: &JobId,
        expect: &Expected,
        grace: Duration,
    ) -> Value {
        let running = self.state().running.get(id).cloned();
        let Some(running) = running else {
            let state = self.state();
            return match state.done.iter().find(|done| *done.id() == *id) {
                Some(Done::Ended(job)) => JobReply::Stop(Stopped {
                    job: (**job).clone(),
                    came_to: StopCameTo::AlreadyEnded(true),
                })
                .into_value(),
                Some(Done::Unknown(_)) => {
                    unconfirmed(id, "Whether the job's work ended is unknown")
                }
                None => JobFailure::new(JobRefusal::Unknown, "No such job runs here")
                    .about(id)
                    .into_value(),
            };
        };
        let named = expect.boot == running.boot
            && expect.pid == running.identity.pid
            && expect.start == running.identity.start
            && expect.pgid == running.identity.pid;
        if !named {
            return JobFailure::new(JobRefusal::Identity, "The job is not the process named")
                .about(id)
                .into_value();
        }
        let mut begun = running.stopping.subscribe();
        let already = {
            let mut going = running.going();
            std::mem::replace(&mut going.asked, true)
        };
        if !already {
            // Admitted right before the word goes.
            if let Some(refused) = gate.refused() {
                running.going().asked = false;
                return refused;
            }
            let millis = u16::try_from(grace.as_millis()).unwrap_or(u16::MAX).clamp(
                *crate::machine::supervise::GRACE.start(),
                *crate::machine::supervise::GRACE.end(),
            );
            if running.stops.send(millis).is_err() {
                return unconfirmed(id, "The job's stop could not be asked");
            }
        }
        // Said only once the supervisor has: a stop asked before, too.
        let said = tokio::time::timeout(STOPPING, begun.wait_for(|begun| *begun)).await;
        if !matches!(said, Ok(Ok(_))) {
            return unconfirmed(id, "The job's stop could not be confirmed");
        }
        let came_to = if already {
            StopCameTo::AlreadyStopping(true)
        } else {
            StopCameTo::Signalled("SIGTERM".into())
        };
        JobReply::Stop(Stopped {
            job: self.view(&running),
            came_to,
        })
        .into_value()
    }

    /// Every job, or job `id`: those running, those done with, and any
    /// `kept` names that no keeper owns here, as unknown.
    pub(crate) fn show(&self, id: Option<&JobId>, kept: &dyn Remembered) -> Value {
        let unknown = |id: JobId, error: &str| {
            Shown::Unreadable(Unreadable {
                id,
                state: Unread::Unknown,
                error: error.into(),
            })
        };
        let state = self.state();
        let mut shown: Vec<Shown> = state
            .running
            .values()
            .map(|running| Shown::Job(Box::new(self.view(running))))
            .collect();
        for done in &state.done {
            shown.push(match done {
                Done::Ended(job) => Shown::Job(job.clone()),
                Done::Unknown(id) => unknown(id.clone(), "Whether its work ended is unknown"),
            });
        }
        let known: Vec<JobId> = state
            .running
            .keys()
            .cloned()
            .chain(state.done.iter().map(|done| done.id().clone()))
            .collect();
        drop(state);
        let Ok(entries) = kept.kept() else {
            let mut failure = JobFailure::new(
                JobRefusal::Unobserved,
                "The desktop's job records could not be read",
            );
            failure.uncertain = true;
            return failure.into_value();
        };
        for record in entries.into_iter().filter_map(|entry| entry.job) {
            if !known.contains(&record.id) {
                shown.push(unknown(
                    record.id,
                    "Its work is left from before and not yet known ended",
                ));
            }
        }
        let reply = match id {
            None => Showing::Every { jobs: shown },
            Some(id) => Showing::One {
                job: shown.into_iter().find(|shown| match shown {
                    Shown::Job(job) => job.id == *id,
                    Shown::Unreadable(unreadable) => unreadable.id == *id,
                    Shown::Later(_) => false,
                }),
            },
        };
        JobReply::Show(reply).into_value()
    }

    /// The last `bytes` of job `id`'s log: a job running, done with, or
    /// left in `kept` from before.
    pub(crate) fn log(&self, id: &JobId, bytes: usize, kept: &dyn Remembered) -> Value {
        let found = {
            let state = self.state();
            if let Some(running) = state.running.get(id) {
                Some((running.record.task.clone(), running.going().log_resets))
            } else {
                state
                    .done
                    .iter()
                    .find(|done| *done.id() == *id)
                    .map(|done| match done {
                        Done::Ended(job) => (job.task.clone(), job.log_resets),
                        Done::Unknown(_) => (None, 0),
                    })
            }
        };
        let found = found.or_else(|| {
            kept.kept()
                .ok()?
                .into_iter()
                .filter_map(|entry| entry.job)
                .find(|job| job.id == *id)
                .map(|job| (job.task, 0))
        });
        let Some((task, log_resets)) = found else {
            return JobFailure::new(JobRefusal::Unknown, "No such job is kept here")
                .about(id)
                .into_value();
        };
        let read = self.open_log(id, OFlag::O_RDONLY).and_then(|mut log| {
            let size = log.metadata()?.len();
            let from = size.saturating_sub(bytes as u64);
            log.seek(SeekFrom::Start(from))?;
            let mut tail = Vec::new();
            log.take(bytes as u64).read_to_end(&mut tail)?;
            Ok((size, from > 0, tail))
        });
        let Ok((size, output_truncated, tail)) = read else {
            return JobFailure::new(JobRefusal::Unobserved, "The job's log could not be read")
                .about(id)
                .into_value();
        };
        use base64::Engine;
        JobReply::Log(Logged {
            id: id.clone(),
            task,
            data: base64::engine::general_purpose::STANDARD.encode(tail),
            size,
            output_truncated,
            log_resets,
        })
        .into_value()
    }
}

impl Running {
    fn going(&self) -> std::sync::MutexGuard<'_, Going> {
        self.going.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Take in what its supervisor said: whether it may say more.
    fn heard(&self, heard: Heard) -> bool {
        match heard {
            Heard::Stopping => {
                self.stopping.send_replace(true);
                true
            }
            Heard::Ended(status) => {
                self.going().program = Some(status);
                true
            }
            Heard::Started => true,
            Heard::Gone => false,
        }
    }
}

impl super::Machine {
    /// Serve the job request `job` on a lane of `kind`, admitted by `gate`:
    /// a start or a stop is a change already kept as intended, whose job
    /// owns itself to the end and whose outcome is kept before its reply;
    /// a show or a log is a read.
    pub(super) async fn job_lane(
        self: &Arc<Self>,
        kind: &atlas_session::lane::LaneKind,
        job: atlas_core::protocol::JobRequest,
        gate: Gate,
        mut writer: super::Writer,
    ) -> atlas_core::error::Result<()> {
        use atlas_core::protocol::JobRequest;
        use atlas_session::lane::LaneKind;
        let change = match kind {
            LaneKind::Change { change } => Some(change.clone()),
            _ => None,
        };
        let (lanes, lane) = (gate.lanes.clone(), gate.lane);
        let Some(jobs) = self.jobs.clone() else {
            let refused = refusal(
                JobRefusal::Unknown,
                "This desktop cannot keep background jobs",
            );
            return self.answer_job(change, refused, &mut writer).await;
        };
        let kept = self
            .daemon
            .supervising
            .as_ref()
            .map(|supervising| supervising.kept.clone());
        let policy = atlas_core::protocol::Request::Job(job.clone()).policy();
        let doing = match job {
            JobRequest::Show { id } => {
                let shown = tokio::task::spawn_blocking(move || match kept {
                    Some(kept) => jobs.show(id.as_ref(), &*kept),
                    None => refusal(JobRefusal::Unobserved, "The desktop keeps no work records"),
                })
                .await
                .unwrap_or_else(|_| refusal(JobRefusal::Unobserved, "The jobs could not be read"));
                return super::write(&mut writer, &shown).await;
            }
            JobRequest::Log { id, bytes } => {
                let logged = tokio::task::spawn_blocking(move || match kept {
                    Some(kept) => jobs.log(&id, bytes, &*kept),
                    None => refusal(JobRefusal::Unobserved, "The desktop keeps no work records"),
                })
                .await
                .unwrap_or_else(|_| refusal(JobRefusal::Unobserved, "The log could not be read"));
                return super::write(&mut writer, &logged).await;
            }
            JobRequest::StopAll { .. } => {
                // A Stop already ends every job through its lease.
                let refused = atlas_core::protocol::failure(
                    atlas_core::error::Code::Platform,
                    "A machine desktop ends its jobs with a Stop",
                );
                return self.answer_job(change, refused, &mut writer).await;
            }
            JobRequest::Start { start, uid, .. } => {
                let Some(started_as) = change.clone() else {
                    let refused =
                        refusal(JobRefusal::Unknown, "A job is started on its change's lane");
                    return super::write(&mut writer, &refused).await;
                };
                if uid != jobs.uid {
                    let refused = refusal(
                        JobRefusal::OtherAccount,
                        "Jobs here run as this desktop's account",
                    );
                    return self.answer_job(change, refused, &mut writer).await;
                }
                let (machine, daemon) = (self.clone(), self.daemon.clone());
                // Carried out here until its outcome is kept, whatever
                // happens to its lane.
                let carrying = daemon.keepers.carry(&started_as);
                tokio::spawn(async move {
                    let _carrying = carrying;
                    let reply = jobs
                        .start(&daemon, gate, started_as.clone(), start, policy)
                        .await;
                    machine.came_to(started_as, reply).await
                })
            }
            JobRequest::Stop { id, expect, grace } => {
                let Some(stopped_as) = change.clone() else {
                    let refused =
                        refusal(JobRefusal::Unknown, "A job is stopped on its change's lane");
                    return super::write(&mut writer, &refused).await;
                };
                let machine = self.clone();
                let carrying = self.daemon.keepers.carry(&stopped_as);
                tokio::spawn(async move {
                    let _carrying = carrying;
                    let reply = jobs.stop(&gate, &id, &expect, grace).await;
                    machine.came_to(stopped_as, reply).await
                })
            }
        };
        // The job owns itself and keeps what it came to, whatever happens
        // to its lane.
        let mut doing = doing;
        tokio::select! {
            joined = &mut doing => match joined {
                Ok(reply) => super::write(&mut writer, &reply).await,
                Err(_) => Ok(()),
            },
            () = lanes.ended(lane) => Ok(()),
        }
    }

    /// Answer a job request with `reply`: kept first when it is a change's.
    async fn answer_job(
        &self,
        change: Option<Token>,
        reply: Value,
        writer: &mut super::Writer,
    ) -> atlas_core::error::Result<()> {
        let reply = match change {
            Some(change) => self.came_to(change, reply).await,
            None => reply,
        };
        super::write(writer, &reply).await
    }
}
