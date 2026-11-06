//! macOS's screen recorder in this login, `replayd`, which keeps part of
//! every one-shot capture it serves until it exits. Its memory is read at
//! most once a minute, and once it passes [`LIMIT`] the helper's own
//! recorder is ended between two captures, with a live stream paused around
//! it, and only while no input runs or has for a moment. launchd starts a
//! fresh one for the next capture, and Screen Recording stays allowed.
use atlas_core::process::Instance;
use atlas_native::native::{
    activity,
    capture::{self, source::Source},
    resources::{self, Process, Usage},
};
use nix::{
    errno::Errno,
    sys::signal::{Signal, kill},
    unistd::Pid,
};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, PoisonError, TryLockError,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// The screen recorder's executable.
const RECORDER: &str = "/usr/libexec/replayd";

/// How long a reading is kept. Status is asked for every few seconds, and
/// each reading lists the account's processes.
const READING: Duration = Duration::from_secs(60);

/// Past this footprint the screen recorder is restarted.
const LIMIT: u64 = 1 << 30;

/// How long a restart waits for running captures to end; one still running
/// then puts it off, which is not a failure.
const CAPTURES: Duration = Duration::from_secs(1);

/// How long no input may have run before a restart. An agent pauses longer
/// than this between commands, and the person between gestures.
const QUIET: Duration = Duration::from_secs(2);

/// How soon a restart put off is tried again, so it lands in the next pause
/// of input rather than a minute later.
const RETRY: Duration = Duration::from_secs(1);

/// How long an ended recorder is given to exit, and how often that is
/// checked meanwhile. A recorder sent SIGKILL exits within milliseconds;
/// input waits for a restart longer than this (see
/// [`activity`](atlas_native::native::activity)).
const EXIT: Duration = Duration::from_secs(1);
const EXIT_CHECK: Duration = Duration::from_millis(20);

/// What finding and ending the screen recorder takes from macOS.
#[derive(Clone, Copy)]
pub(super) struct System {
    /// This account's screen recorder and its memory now.
    find: fn() -> Option<(Process, Usage)>,
    /// Process `pid` as it is now.
    describe: fn(i32) -> Option<Process>,
    /// Sends SIGKILL to process `pid`.
    kill: fn(i32) -> nix::Result<()>,
    /// Whether this very process exits within [`EXIT`].
    exits: fn(Instance) -> bool,
    /// Tells the live stream that its next picture comes from a new
    /// recorder, which takes longer to send its first.
    restarted: fn(),
}

impl System {
    /// macOS itself.
    pub(super) fn real() -> Self {
        Self {
            find: || resources::running(Path::new(RECORDER)),
            describe: resources::describe,
            kill: sigkill,
            exits,
            restarted: capture::recorder_restarted,
        }
    }
}

/// SIGKILL to the one process `pid`; never to a group or to every process,
/// as a PID of 0 or below would ask, nor to launchd.
fn sigkill(pid: i32) -> nix::Result<()> {
    if pid <= 1 {
        return Err(Errno::EINVAL);
    }
    kill(Pid::from_raw(pid), Signal::SIGKILL)
}

/// Whether `instance` exits within [`EXIT`]: see [`Instance::ended`].
fn exits(instance: Instance) -> bool {
    let deadline = Instant::now() + EXIT;
    loop {
        if instance.ended() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(EXIT_CHECK);
    }
}

/// What a hello reports of the screen recorder: its memory when it runs,
/// and the restarts since the daemon started.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Report {
    #[serde(flatten)]
    pub(super) usage: Option<Usage>,
    /// How many times it was restarted.
    pub(super) resets: u64,
    /// When it last was, in milliseconds since 1970.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) last_reset_at: Option<u64>,
    /// Restarts in a row that were tried and failed: the signal was refused
    /// or the recorder did not exit. One put off because a capture was
    /// running is not counted.
    pub(super) reset_failures: u64,
}

impl Report {
    /// Whether there is anything to report.
    fn any(&self) -> bool {
        self.usage.is_some() || self.resets > 0 || self.reset_failures > 0
    }
}

/// How one restart went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Restart {
    /// The recorder measured was ended and has exited.
    Done,
    /// The process found now is not the one measured, or is not this
    /// account's recorder, or it is already gone: nothing was ended.
    Refused,
    /// The signal was refused, or the recorder did not exit.
    Failed,
}

/// The screen recorder of this login: read at most once a minute, and
/// restarted at such a reading when it is too large and this daemon is the
/// helper's.
pub(super) struct Recorder {
    system: System,
    /// The helper account whose recorder may be restarted; none only reads.
    helper: Option<u32>,
    /// The input service's activity file: a restart waits for a pause in
    /// the input it tells of. None has no input to wait for.
    input: Option<PathBuf>,
    /// The last reading and when the next is due. It is held while reading
    /// and restarting, so a reading asked for meanwhile waits for that one,
    /// not a second one.
    last: Mutex<(Option<Instant>, Report)>,
    /// What a status shows: the newest finished reading. It is never held
    /// while reading.
    shown: Mutex<Option<Report>>,
    /// Whether a reading runs on a thread of its own now.
    reading: AtomicBool,
}

impl Recorder {
    pub(super) fn new(system: System, helper: Option<u32>) -> Self {
        Self {
            system,
            helper,
            input: None,
            last: Mutex::new((None, Report::default())),
            shown: Mutex::new(None),
            reading: AtomicBool::new(false),
        }
    }

    /// This recorder, restarted only in a pause of the input that the
    /// activity file at `path` tells of.
    pub(super) fn beside_input(self, path: PathBuf) -> Self {
        Self {
            input: Some(path),
            ..self
        }
    }

    /// The screen recorder as last read; none when there is nothing to say.
    /// Once that reading is a minute old, a new one starts on a thread of its
    /// own, so a status, and every live frame's session proof with it, never
    /// waits for a reading or a restart; it shows from a later status on.
    pub(super) fn report(self: &Arc<Self>, source: &Arc<Source>) -> Option<Report> {
        let now = Instant::now();
        let due = match self.last.try_lock() {
            Ok(last) => last.0.is_none_or(|due| now >= due),
            Err(TryLockError::Poisoned(last)) => last.into_inner().0.is_none_or(|due| now >= due),
            Err(TryLockError::WouldBlock) => false,
        };
        if due && !self.reading.swap(true, Ordering::AcqRel) {
            let (recorder, source) = (self.clone(), source.clone());
            let started = std::thread::Builder::new()
                .name("atlas-screen-recorder".into())
                .spawn(move || {
                    let report = recorder.report_at(&source, Instant::now());
                    *recorder
                        .shown
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) = report;
                    recorder.reading.store(false, Ordering::Release);
                });
            // Without a thread, the next status tries again.
            if started.is_err() {
                self.reading.store(false, Ordering::Release);
            }
        }
        *self.shown.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The screen recorder as read at `now`, read again once a minute, or a
    /// moment after a restart was put off. A restart waits for a pause in
    /// input, then up to [`CAPTURES`] for the captures of `source` that are
    /// running, and neither begins under it.
    fn report_at(&self, source: &Source, now: Instant) -> Option<Report> {
        let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        let (due, report) = &mut *last;
        if due.is_none_or(|due| now >= due) {
            let (read, next) = self.read(source, *report);
            *report = read;
            *due = Some(now + next);
        }
        report.any().then_some(*report)
    }

    /// A new reading after `report`, restarting the recorder first when it
    /// has grown past [`LIMIT`], and how long until the next.
    fn read(&self, source: &Source, mut report: Report) -> (Report, Duration) {
        let found = (self.system.find)();
        report.usage = found.as_ref().map(|(_, usage)| *usage);
        let (Some(helper), Some((measured, usage))) = (self.helper, found) else {
            report.reset_failures = 0;
            return (report, READING);
        };
        if usage.footprint <= LIMIT {
            report.reset_failures = 0;
            return (report, READING);
        }
        let restarted = match &self.input {
            // Input is kept out only around the restart itself, once no
            // capture runs; a first look spares the stream a needless pause.
            Some(input) if activity::quiet(input, QUIET, || ()).is_some() => source
                .between_captures(CAPTURES, || {
                    activity::quiet(input, QUIET, || self.restart(source, &measured, helper))
                })
                .flatten(),
            Some(_) => None,
            None => source.between_captures(CAPTURES, || self.restart(source, &measured, helper)),
        };
        match restarted {
            Some(Restart::Done) => {
                report.resets += 1;
                report.last_reset_at = Some(atlas_core::time::now_ms());
                report.reset_failures = 0;
                report.usage = (self.system.find)().map(|(_, usage)| usage);
            }
            Some(Restart::Refused) => {
                report.usage = (self.system.find)().map(|(_, usage)| usage);
                if report.usage.is_none_or(|usage| usage.footprint <= LIMIT) {
                    report.reset_failures = 0;
                }
            }
            Some(Restart::Failed) => report.reset_failures += 1,
            // Put off: input or a capture still ran, or a live stream could
            // not be paused. It is tried again shortly.
            None => return (report, RETRY),
        }
        (report, READING)
    }

    /// End `measured` when it is, right now, still exactly the recorder that
    /// was measured, and wait for it to exit. One that has not exited by
    /// then keeps the captures of `source` out until it has.
    fn restart(&self, source: &Source, measured: &Process, helper: u32) -> Restart {
        let pid = measured.instance.pid;
        let now = (self.system.describe)(pid);
        if !may_restart(measured, now.as_ref(), helper) {
            return Restart::Refused;
        }
        match (self.system.kill)(pid) {
            Ok(()) => {}
            // It exited on its own since it was described.
            Err(Errno::ESRCH) => return Restart::Refused,
            Err(_) => return Restart::Failed,
        }
        if (self.system.exits)(measured.instance) {
            (self.system.restarted)();
            Restart::Done
        } else {
            source.until_gone(measured.instance);
            Restart::Failed
        }
    }
}

/// Whether `measured` is the helper's own screen recorder and `now`, read
/// right before ending it, is still that very process: the same PID and
/// start time, the same effective and real user, the same executable. A
/// PID reused by another process has another start time.
fn may_restart(measured: &Process, now: Option<&Process>, helper: u32) -> bool {
    measured.instance.pid > 1
        && measured.uid == helper
        && measured.real_uid == helper
        && measured.path == Path::new(RECORDER)
        && now == Some(measured)
}

#[cfg(test)]
mod tests;
