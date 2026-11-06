//! macOS's screen recorder in this login, `replayd`, which keeps part of
//! every capture it serves until it exits. Its memory is read at most once a
//! minute, and once it passes [`LIMIT`] the helper's own recorder is ended
//! between two captures. launchd starts a fresh one for the next capture,
//! and Screen Recording stays allowed.
use atlas_core::process::{Instance, State};
use atlas_native::native::{
    capture::source::Source,
    resources::{self, Process, Usage},
};
use nix::{
    errno::Errno,
    sys::signal::{Signal, kill},
    unistd::Pid,
};
use serde::Serialize;
use std::{
    path::Path,
    sync::{Mutex, PoisonError},
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
/// then puts it off to the next reading.
const CAPTURES: Duration = Duration::from_secs(1);

/// How long an ended recorder is given to exit, and how often that is
/// checked meanwhile.
const EXIT: Duration = Duration::from_secs(2);
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
}

impl System {
    /// macOS itself.
    pub(super) fn real() -> Self {
        Self {
            find: || resources::running(Path::new(RECORDER)),
            describe: resources::describe,
            kill: sigkill,
            exits,
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

/// Whether `instance` exits within [`EXIT`]: it is gone, and its PID free
/// or another process's.
fn exits(instance: Instance) -> bool {
    let deadline = Instant::now() + EXIT;
    loop {
        if instance.state() == State::Gone {
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
    /// Readings in a row that found it past the limit and could not restart
    /// it.
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
    /// account's recorder: nothing was sent.
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
    /// The last reading and when it was taken. It is held while reading and
    /// restarting, so a status asked for meanwhile waits for that reading,
    /// not a second one.
    last: Mutex<(Option<Instant>, Report)>,
}

impl Recorder {
    pub(super) fn new(system: System, helper: Option<u32>) -> Self {
        Self {
            system,
            helper,
            last: Mutex::new((None, Report::default())),
        }
    }

    /// The screen recorder as last read, read again when older than a
    /// minute; none when there is nothing to say. A restart waits up to
    /// [`CAPTURES`] for the captures of `source` that are running, and none
    /// begins under it.
    pub(super) fn report(&self, source: &Source) -> Option<Report> {
        self.report_at(source, Instant::now())
    }

    fn report_at(&self, source: &Source, now: Instant) -> Option<Report> {
        let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        let (at, report) = &mut *last;
        if !at.is_some_and(|at| now.saturating_duration_since(at) < READING) {
            *report = self.read(source, *report);
            *at = Some(now);
        }
        report.any().then_some(*report)
    }

    /// A new reading after `report`, restarting the recorder first when it
    /// has grown past [`LIMIT`].
    fn read(&self, source: &Source, mut report: Report) -> Report {
        let found = (self.system.find)();
        report.usage = found.as_ref().map(|(_, usage)| *usage);
        let (Some(helper), Some((measured, usage))) = (self.helper, found) else {
            report.reset_failures = 0;
            return report;
        };
        if usage.footprint <= LIMIT {
            report.reset_failures = 0;
            return report;
        }
        match source.between_captures(CAPTURES, || self.restart(source, &measured, helper)) {
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
            Some(Restart::Failed) | None => report.reset_failures += 1,
        }
        report
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
        if (self.system.kill)(pid).is_err() {
            return Restart::Failed;
        }
        if (self.system.exits)(measured.instance) {
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
