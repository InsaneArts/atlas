//! `run`: one command inside the helper login. Its standard output and error
//! share one pipe and stream back as base64 `output` events; the final reply
//! reports how it ended. The command gets its own process group, so a timeout
//! ends everything it started, and a daemon restart does not.
//!
//! Supervision never waits on the client: events go to a bounded channel that
//! a separate writer drains. Output is coalesced into a bounded number of
//! events; what the client cannot take is dropped and reported as truncated.
//! A client that goes away, or a daemon that stops, ends the command. Any
//! failure once the command has started is uncertain: it may have done some
//! of its work.
use atlas_core::{
    error::Code,
    protocol::{MAX_REPLY_EVENTS, failure, interrupted},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use nix::{
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use serde_json::{Value, json};
use std::{
    os::{
        fd::OwnedFd,
        unix::{fs::PermissionsExt, process::ExitStatusExt},
    },
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    time::{Duration, Instant},
};
use tokio::{
    io::AsyncReadExt,
    net::unix::pipe,
    process::Command,
    sync::mpsc::{
        Sender,
        error::{SendTimeoutError, TrySendError},
    },
};

/// Output forwarded per run; the rest is drained and reported as truncated.
const MAX_OUTPUT: usize = 16 * 1024 * 1024;

/// How long a timed-out group has between SIGTERM and SIGKILL.
const GRACE: Duration = Duration::from_secs(2);

/// How long output is still collected after the command itself exits, when
/// something it left running keeps the pipe open.
const LINGER: Duration = Duration::from_millis(200);

/// Events buffered for a slow client.
pub(super) const EVENTS: usize = 256;

/// Output events per run, leaving room in what the client accepts per reply
/// for the `started` event.
const MAX_EVENTS: usize = 1000;
const _: () = assert!(MAX_EVENTS < MAX_REPLY_EVENTS);

/// Pending output sent at once, without waiting for more.
const FLUSH: usize = 64 * 1024;

/// Output held back for a slow client before it is dropped.
const HELD: usize = 1024 * 1024;

/// How long the last output waits for a slow client.
const FINISH: Duration = Duration::from_secs(2);

/// How soon early output is sent, or sent again once a slow client has room.
const SOON: Duration = Duration::from_millis(50);

/// The daemon's own PATH, for a daemon without a home.
const PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

/// The locale a command gets when the daemon has none: a daemon's child
/// inherits launchd's bare environment, in which tools print every
/// non-ASCII character as a question mark.
const LANG: &str = "en_US.UTF-8";

/// The PATH a command searches: the helper's, the same places readiness and
/// `environment run` look in (`.local/bin` and Homebrew among them), or the
/// daemon's own without a home.
fn search_path(home: Option<&Path>) -> String {
    home.and_then(|home| std::env::join_paths(atlas_core::account::helper_search_path(home)).ok())
        .and_then(|path| path.into_string().ok())
        .unwrap_or_else(|| PATH.to_owned())
}

/// Where a `program` with a slash is: a relative one starts at `home`, where
/// the command runs, as it would in a shell there.
fn locate(program: &str, home: Option<&Path>) -> PathBuf {
    match home {
        Some(home) if Path::new(program).is_relative() => home.join(program),
        _ => PathBuf::from(program),
    }
}

/// The executable for `program`: where [`locate`] puts it when it contains a
/// slash, else the first match on `path`.
fn resolve(program: &str, path: &str, home: Option<&Path>) -> Option<PathBuf> {
    let executable = |candidate: &Path| {
        std::fs::metadata(candidate)
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    };

    if program.contains('/') {
        let candidate = locate(program, home);
        return executable(&candidate).then_some(candidate);
    }

    path.split(':')
        .filter(|directory| !directory.is_empty())
        .map(|directory| Path::new(directory).join(program))
        .find(|candidate| executable(candidate))
}

/// The refusal of a `name` that [`resolve`] found no program for, saying
/// why: not on PATH, a folder, not executable, or not there.
fn unresolved(name: &str, home: Option<&Path>) -> Value {
    let located = locate(name, home);
    let reason = if !name.contains('/') {
        "command not found on PATH"
    } else if located.is_dir() {
        "is a folder, not a program"
    } else if located.exists() {
        "is not executable (chmod +x it)"
    } else {
        "no such program"
    };

    failure(Code::SpawnFailed, format!("{name}: {reason}"))
}

/// The executable that runs `name` and the PATH it was found on, or the
/// refusal saying why there is none.
pub(super) fn program(name: &str, home: Option<&Path>) -> Result<(PathBuf, String), Value> {
    let path = search_path(home);
    match resolve(name, &path, home) {
        Some(executable) => Ok((executable, path)),
        None => Err(unresolved(name, home)),
    }
}

/// The helper's home, where a command starts, when it is a folder.
pub(super) fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_dir())
}

/// The locale a command gets: the daemon's own, or UTF-8 English when it
/// has none.
fn locale() -> Option<&'static str> {
    std::env::var_os("LANG")
        .is_none_or(|lang| lang.is_empty())
        .then_some(LANG)
}

/// Set `command` up as a shell in the helper would be: `path`, its locale,
/// and `home` to start in.
pub(super) fn in_helper(command: &mut Command, path: &str, home: Option<&Path>) {
    command.env("PATH", path);

    if let Some(lang) = locale() {
        command.env("LANG", lang);
    }

    if let Some(home) = home {
        command.current_dir(home);
    }
}

/// The conventional name of a macOS signal number, as "SIGTERM".
pub(crate) fn signal_name(signal: i32) -> String {
    Signal::try_from(signal)
        .map(|signal| signal.as_str().to_owned())
        .unwrap_or_else(|_| format!("SIG{signal}"))
}

/// The final reply for a command that ran.
fn ended(
    status: ExitStatus,
    timed_out: bool,
    started: Instant,
    truncated: bool,
    unfinished: bool,
) -> Value {
    let mut reply = json!({
        "ok": true,
        "exitCode": status.code(),
        "signal": status.signal().map(signal_name),
        "timedOut": timed_out,
        "durationMs": atlas_core::time::ms(started.elapsed()),
    });

    if truncated {
        reply["outputTruncated"] = json!(true);
    }

    // Something the command started still held its output when collection
    // stopped, or the pipe failed: the end of the output may be missing.
    if unfinished {
        reply["outputUnfinished"] = json!(true);
    }

    reply
}

/// Signal the command's whole process group (`None` only probes); whether
/// any member remains.
fn signal_group(group: Pid, signal: impl Into<Option<Signal>>) -> bool {
    killpg(group, signal).is_ok()
}

/// After a timeout, end every member of the group before replying: SIGTERM,
/// then SIGKILL once the grace has passed, until none remains.
async fn end_group(group: Pid) {
    if !signal_group(group, Signal::SIGTERM) {
        return;
    }

    let deadline = Instant::now() + GRACE;

    while signal_group(group, None) {
        if atlas_core::process::group(group.as_raw())
            .is_ok_and(|members| members.iter().all(|member| member.zombie))
        {
            break;
        }
        if Instant::now() >= deadline {
            signal_group(group, Signal::SIGKILL);
        }

        tokio::time::sleep(Duration::from_millis(20)).await;
        if Instant::now() >= deadline + GRACE {
            break;
        }
    }
}

/// Output not yet sent, coalesced so that a command writing a little at a
/// time costs few events: the client accepts 1024 events per reply.
#[derive(Default)]
struct Output {
    pending: Vec<u8>,
    /// When the pending bytes are sent if no more arrive first.
    due: Option<tokio::time::Instant>,
    /// Bytes delivered to the writer.
    forwarded: usize,
    /// Output events delivered to the writer.
    sent: usize,
    truncated: bool,
}

/// The client went away; nothing more can be delivered.
struct Gone;

impl Output {
    /// Hold `bytes` for sending, within the output budget.
    fn push(&mut self, bytes: &[u8]) {
        let room = MAX_OUTPUT.saturating_sub(self.forwarded + self.pending.len());
        let kept = bytes.len().min(room);
        self.truncated |= kept < bytes.len();
        self.pending.extend_from_slice(&bytes[..kept]);

        if self.due.is_none() && !self.pending.is_empty() {
            // Early output streams promptly; a long trickle is batched.
            let wait = if self.sent < MAX_EVENTS / 2 {
                SOON
            } else {
                Duration::from_secs(1)
            };
            self.due = Some(tokio::time::Instant::now() + wait);
        }
    }

    fn full(&self) -> bool {
        self.pending.len() >= FLUSH
    }

    /// The pending output as one event; None when nothing is pending, or
    /// when the event budget is spent and the output is dropped as truncated.
    fn event(&mut self) -> Option<Value> {
        if self.pending.is_empty() {
            return None;
        }

        if self.sent >= MAX_EVENTS {
            self.dropped();
            return None;
        }

        Some(json!({"event": "output", "data": STANDARD.encode(&self.pending)}))
    }

    /// The pending output reached the writer.
    fn delivered(&mut self) {
        self.forwarded += self.pending.len();
        self.sent += 1;
        self.pending.clear();
    }

    /// The pending output will never reach the client.
    fn dropped(&mut self) {
        self.truncated = true;
        self.pending.clear();
    }

    /// Send what is pending as one event. A slow client leaves it pending
    /// until it grows past what is held back, then it is dropped.
    fn flush(&mut self, events: &Sender<Value>) -> Result<(), Gone> {
        self.due = None;
        let Some(event) = self.event() else {
            return Ok(());
        };

        match events.try_send(event) {
            Ok(()) => self.delivered(),
            Err(TrySendError::Full(_)) if self.pending.len() < HELD => {
                self.due = Some(tokio::time::Instant::now() + SOON);
            }
            Err(TrySendError::Full(_)) => self.dropped(),
            Err(TrySendError::Closed(_)) => return Err(Gone),
        }

        Ok(())
    }

    /// Deliver the last pending output, waiting a bounded time for a slow
    /// client; whatever cannot be delivered is reported as truncated.
    async fn finish(&mut self, events: &Sender<Value>) -> Result<(), Gone> {
        self.due = None;
        let Some(event) = self.event() else {
            return Ok(());
        };

        match events.send_timeout(event, FINISH).await {
            Ok(()) => self.delivered(),
            Err(SendTimeoutError::Timeout(_)) => self.dropped(),
            Err(SendTimeoutError::Closed(_)) => return Err(Gone),
        }

        Ok(())
    }
}

/// Why a command was ended before it finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Cancel {
    /// The client hung up.
    Left,
    /// The daemon is stopping.
    Stopping,
    /// The helper came on screen, where a person may be using it.
    OnConsole,
}

impl Cancel {
    fn reply(self) -> Value {
        let (code, message) = match self {
            Self::Left => (Code::Transport, "The client went away"),
            Self::Stopping => (Code::Restart, "The helper stopped while the command ran"),
            Self::OnConsole => (
                Code::OnConsole,
                "The helper came on screen, so the command was stopped",
            ),
        };
        interrupted(code, message)
    }
}

#[derive(Clone, Copy, Default)]
enum StopCause {
    #[default]
    Running,
    Timeout,
    Cancelled(Cancel),
}

impl StopCause {
    fn abandoned(self) -> bool {
        match self {
            Self::Cancelled(_) => true,
            Self::Running | Self::Timeout => false,
        }
    }
}

/// Run `argv`, sending its events to `events`, and return the final reply.
/// When `cancel` completes (the client left, the daemon is stopping or the
/// helper came on screen) or the client stops taking events, the group is
/// ended before returning.
pub(super) async fn run(
    argv: &[String],
    timeout: Duration,
    events: Sender<Value>,
    cancel: impl Future<Output = Cancel>,
) -> Value {
    // The command runs in the helper's home, as a shell there would.
    run_at(argv, home(), timeout, events, cancel).await
}

/// A command running in a process group of its own.
struct Spawned {
    child: tokio::process::Child,
    /// Its standard output and error, merged.
    pipe: pipe::Receiver,
    group: Pid,
    started: Instant,
}

/// Spawn with merged output in a group of its own, retaining the child until
/// pipe setup either succeeds or has killed and reaped it.
async fn spawn(argv: &[String], home: Option<&Path>) -> Result<Spawned, Value> {
    // The protocol never passes an empty argv; refuse one rather than panic.
    let Some((name, arguments)) = argv.split_first() else {
        return Err(failure(Code::SpawnFailed, "No command to run"));
    };

    let (executable, path) = program(name, home)?;
    let (reader, writer) =
        atlas_core::spawn::pipe().map_err(|error| failure(Code::SpawnFailed, error.to_string()))?;

    let unstarted = |error: std::io::Error| failure(Code::SpawnFailed, format!("{name}: {error}"));
    let errors = writer.try_clone().map_err(unstarted)?;
    let mut command = Command::new(&executable);
    command
        .arg0(name)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::from(OwnedFd::from(writer)))
        .stderr(Stdio::from(OwnedFd::from(errors)))
        .process_group(0)
        .kill_on_drop(false);
    in_helper(&mut command, &path, home);
    let spawned = atlas_core::spawn::spawn_async(&mut command);
    // The command keeps this process's copies of the pipe's write end.
    drop(command);
    let mut child = spawned.map_err(unstarted)?;

    let started = Instant::now();

    let Some(group) = child.id().and_then(|pid| i32::try_from(pid).ok()) else {
        return Err(interrupted(
            Code::SpawnFailed,
            "The command exited before it could be observed",
        ));
    };

    let group = Pid::from_raw(group);

    // Checks that this is a pipe and makes it non-blocking.
    let pipe = match pipe::Receiver::from_owned_fd(OwnedFd::from(reader)) {
        Ok(receiver) => receiver,
        Err(error) => {
            signal_group(group, Signal::SIGKILL);
            let _ = child.wait().await;
            return Err(interrupted(Code::SpawnFailed, error.to_string()));
        }
    };

    Ok(Spawned {
        child,
        pipe,
        group,
        started,
    })
}

/// What supervision has seen of a command so far. Its exit, its pipe closing
/// and cancellation are independent observations: cancellation overrides a
/// timeout, but does not start termination after the command exited.
#[derive(Default)]
struct Watch {
    output: Output,
    cause: StopCause,
    status: Option<std::io::Result<ExitStatus>>,
    /// Termination was initiated; the deadline now marks when SIGKILL follows.
    stopping: bool,
    /// The pipe holds no more output, or it stopped being collected.
    closed: bool,
    /// Something the command started still held its output when collection
    /// stopped, or the pipe failed.
    unfinished: bool,
    /// When output stops being collected after the command's exit.
    linger: Option<tokio::time::Instant>,
}

impl Watch {
    fn terminate(&mut self, group: Pid) {
        signal_group(group, Signal::SIGTERM);
        self.stopping = true;
    }

    fn read(&mut self, read: std::io::Result<usize>, buffer: &[u8], events: &Sender<Value>) {
        match read {
            Ok(0) => self.closed = true,
            Err(_) => self.abandon_pipe(),
            Ok(count) if !self.cause.abandoned() => {
                self.output.push(&buffer[..count]);
                if self.output.full() {
                    self.flush(events);
                }
            }
            Ok(_) => {}
        }
    }

    fn flush(&mut self, events: &Sender<Value>) {
        if self.output.flush(events).is_err() {
            self.cause = StopCause::Cancelled(Cancel::Left);
        }
    }

    /// The deadline passed: its first passing times the command out, and
    /// every later one, a grace after a termination, sends SIGKILL.
    fn deadline_passed(&mut self, group: Pid) {
        if self.stopping {
            signal_group(group, Signal::SIGKILL);
        } else {
            self.terminate(group);
            self.cause = StopCause::Timeout;
        }
    }

    fn abandon_pipe(&mut self) {
        self.closed = true;
        self.unfinished = true;
    }
}

/// The wait for `due`. A select builds it even when its guard, that `due` is
/// set, leaves the branch disabled.
fn sleep_until(due: Option<tokio::time::Instant>) -> tokio::time::Sleep {
    tokio::time::sleep_until(due.unwrap_or_else(tokio::time::Instant::now))
}

/// Observe an owned child's exit without reaping it. Its PID reserves the
/// group until timeout/cancel cleanup has sent its last signal.
async fn child_exited(child: &tokio::process::Child) -> std::io::Result<()> {
    let pid = child
        .id()
        .ok_or_else(|| std::io::Error::other("The command's child identity is gone"))?;
    // Subscribe before checking so an exit between the check and recv is
    // retained. SIGCHLD is a wakeup; only waitid authorizes the observation.
    let mut changes = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::child())?;
    loop {
        if atlas_core::process::child_exited(pid)? {
            return Ok(());
        }
        changes
            .recv()
            .await
            .ok_or_else(|| std::io::Error::other("The child exit signal stream ended"))?;
    }
}

/// [`run`] with `home` as the helper's home.
async fn run_at(
    argv: &[String],
    home: Option<PathBuf>,
    timeout: Duration,
    events: Sender<Value>,
    cancel: impl Future<Output = Cancel>,
) -> Value {
    let mut spawned = match spawn(argv, home.as_deref()).await {
        Ok(spawned) => spawned,
        Err(refusal) => return refusal,
    };
    let group = spawned.group;
    let mut watch = Watch::default();
    if events
        .try_send(json!({"event": "started", "pid": group.as_raw()}))
        .is_err()
    {
        watch.cause = StopCause::Cancelled(Cancel::Left);
    }
    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline, cancel);

    let mut buffer = vec![0; FLUSH];
    let mut exited = false;

    while !exited || !watch.closed {
        if watch.cause.abandoned() && !watch.stopping && !exited {
            watch.terminate(group);
            deadline.as_mut().reset(tokio::time::Instant::now() + GRACE);
        }
        tokio::select! {
            read = spawned.pipe.read(&mut buffer), if !watch.closed => {
                watch.read(read, &buffer, &events);
            }
            () = sleep_until(watch.output.due),
                if watch.output.due.is_some() && !watch.cause.abandoned() => watch.flush(&events),
            observed = child_exited(&spawned.child), if !exited => {
                exited = true;
                match observed {
                    // No termination began, so no later group signal is
                    // permitted: preserve prompt ordinary reaping even
                    // while a descendant still holds the output pipe.
                    Ok(()) if !watch.stopping => watch.status = Some(spawned.child.wait().await),
                    Ok(()) => {},
                    Err(error) => watch.status = Some(Err(error)),
                }
                watch.linger = Some(tokio::time::Instant::now() + LINGER);
            }
            () = &mut deadline, if !exited => {
                watch.deadline_passed(group);
                deadline.as_mut().reset(tokio::time::Instant::now() + GRACE);
            }
            reason = &mut cancel, if !watch.cause.abandoned() => {
                watch.cause = StopCause::Cancelled(reason);
            }
            () = sleep_until(watch.linger), if watch.linger.is_some() && !watch.closed => {
                watch.abandon_pipe();
            }
        }
    }

    // A command that already finished keeps whatever it left running.
    if watch.stopping && watch.status.is_none() {
        end_group(group).await;
    }
    if watch.status.is_none() {
        watch.status = Some(spawned.child.wait().await);
    }

    let timed_out = match watch.cause {
        StopCause::Cancelled(reason) => return reason.reply(),
        StopCause::Running => false,
        StopCause::Timeout => true,
    };

    if watch.output.finish(&events).await.is_err() {
        return Cancel::Left.reply();
    }

    match watch.status {
        Some(Ok(status)) => ended(
            status,
            timed_out,
            spawned.started,
            watch.output.truncated,
            watch.unfinished,
        ),
        Some(Err(_)) | None => interrupted(
            Code::WaitFailed,
            "The command's exit status could not be read",
        ),
    }
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
