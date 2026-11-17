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
use crate::machine::{
    processes::Remembered,
    supervise::invocation::{self, Invocation, Supervisor},
};
use atlas_core::{
    error::Code,
    protocol::{MAX_REPLY_EVENTS, failure, interrupted},
    session::Console,
};
use atlas_session::generation::Generation;
use base64::{Engine, engine::general_purpose::STANDARD};
use keeper::{Keepers, Verdict};
use nix::{
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use serde_json::{Value, json};
use std::{
    future::Future,
    os::{
        fd::OwnedFd,
        unix::{fs::PermissionsExt, process::ExitStatusExt},
    },
    path::{Path, PathBuf},
    pin::Pin,
    process::{ExitStatus, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::AsyncReadExt,
    net::unix::{OwnedReadHalf, pipe},
    process::Command,
    sync::mpsc::{
        Sender,
        error::{SendTimeoutError, TrySendError},
    },
    sync::oneshot,
};

/// Output forwarded per run; the rest is drained and reported as truncated.
const MAX_OUTPUT: usize = 16 * 1024 * 1024;

/// How long a timed-out group has between SIGTERM and SIGKILL.
const GRACE: Duration = Duration::from_secs(2);
// A supervisor TERMed with its group proves its work ended within this
// grace, before the group's SIGKILL takes it too.
const _: () = assert!(
    crate::machine::supervise::TERMINATING.as_millis()
        + crate::machine::supervise::ENDING.as_millis()
        < GRACE.as_millis()
);

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
    finish_group(group).await;
}

/// After a SIGTERM already sent, wait for every member of the group to end,
/// with SIGKILL once the grace has passed.
async fn finish_group(group: Pid) {
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
    /// The screen left where this desktop keeps it: the helper came on
    /// screen, where a person may be using it, or a machine's own session
    /// left its screen.
    Away(Console),
    /// Agents were stopped on a machine desktop.
    Halted,
}

impl Cancel {
    fn reply(self) -> Value {
        let (code, message) = match self {
            Self::Left => (Code::Transport, "The client went away"),
            Self::Stopping => (Code::Restart, "The helper stopped while the command ran"),
            Self::Away(Console::Off) => (
                Code::OnConsole,
                "The helper came on screen, so the command was stopped",
            ),
            Self::Away(Console::Own) => (
                Code::SessionChanged,
                "The desktop's own session left its screen, so the command was stopped",
            ),
            Self::Halted => (
                Code::Stopped,
                "Agents were stopped on this desktop, so the command was ended",
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

/// [`run_unless`] with nothing to stop it.
#[cfg(test)]
pub(super) async fn run(
    argv: &[String],
    timeout: Duration,
    events: Sender<Value>,
    cancel: impl Future<Output = Cancel>,
) -> Value {
    run_unless(argv, timeout, events, cancel, &|| None, None).await
}

/// What stops a command from starting, checked immediately before it does:
/// the refusal to give instead, if any.
pub(super) type Unless<'a> = &'a (dyn Fn() -> Option<Value> + Send + Sync);

/// How long a supervised command's lease runs unrenewed.
const LEASE: Duration = Duration::from_secs(2);

/// What a command on a machine desktop runs under: the supervisor that ends
/// its work once the daemon is gone or stops renewing, where it is kept
/// before it starts, and what ends the renewing.
pub(in crate::daemon) struct Lease {
    pub(super) supervisor: Supervisor,
    /// Where supervisors leave their proof.
    pub(super) proofs: PathBuf,
    /// The generation it is started for.
    pub(super) generation: Generation,
    /// Completes once the work may no longer run: its generation is over,
    /// or agents were stopped since it was admitted.
    pub(super) over: Pin<Box<dyn Future<Output = ()> + Send>>,
    /// The refusal of the word to start, checked right before it is given;
    /// none while the command is still admitted.
    pub(super) admitted: Box<dyn Fn() -> Option<Value> + Send + Sync>,
    /// Where its supervisor is kept on the desktop's disk.
    pub(super) kept: Arc<dyn Remembered>,
    pub(super) keepers: Arc<Keepers>,
    /// Where a job's graceful stops come from, each its grace in
    /// milliseconds; none for a command.
    pub(super) stops: Option<tokio::sync::mpsc::UnboundedReceiver<u16>>,
    /// Where the keeper says, once it has judged the work's end, whether it
    /// was proved ended; none for a command.
    pub(super) judged: Option<oneshot::Sender<bool>>,
    /// The background job it runs, kept beside its supervisor; none for a
    /// command.
    pub(super) job: Option<crate::machine::processes::JobRecord>,
    /// The change it is started for, kept beside its supervisor.
    pub(super) change: Option<atlas_core::files::Token>,
    /// Where what a command prints is kept with what it comes to; none
    /// for a job, whose output goes to its log.
    pub(super) printed: Option<Arc<std::sync::Mutex<Printed>>>,
}

/// What a command on a machine desktop printed, kept with what it came to
/// so its reply, lost on its way, still says it: its first
/// [`OUTPUT`](crate::machine::changes::OUTPUT) bytes, and whether more were
/// cut.
#[derive(Debug, Default)]
pub(in crate::daemon) struct Printed {
    started: Option<i32>,
    output: Vec<u8>,
    truncated: bool,
}

impl Printed {
    fn push(&mut self, bytes: &[u8]) {
        let room = crate::machine::changes::OUTPUT.saturating_sub(self.output.len());
        let kept = bytes.len().min(room);
        self.truncated |= kept < bytes.len();
        self.output.extend_from_slice(&bytes[..kept]);
    }

    /// `reply` as it is kept: with the command's start and what it printed
    /// as the events a reply carries, and whether that was cut short.
    pub(in crate::daemon) fn kept(&self, reply: &Value) -> Value {
        let mut events = Vec::new();
        if let Some(pid) = self.started {
            events.push(json!({"event": "started", "pid": pid}));
        }
        if !self.output.is_empty() {
            events.push(json!({"event": "output", "data": STANDARD.encode(&self.output)}));
        }
        let mut kept = reply.clone();
        kept["events"] = json!(events);
        kept["truncated"] = json!(self.truncated);
        kept
    }
}

/// Keep what `printed` holds, when it is there, by `keep`.
fn printing(printed: Option<&Arc<std::sync::Mutex<Printed>>>, keep: impl FnOnce(&mut Printed)) {
    if let Some(printed) = printed {
        keep(
            &mut printed
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
    }
}

/// Run `argv`, sending its events to `events`, and return the final reply.
/// When `cancel` completes (the client left, the daemon is stopping or the
/// screen left where the desktop keeps it) or the client stops taking
/// events, the group is ended before returning. It is refused instead when
/// `unless` says so immediately before the command would start, after every
/// wait that comes before it. Under a `lease`, it runs under a supervisor,
/// and what it leaves running lives only while the lease is renewed.
pub(super) async fn run_unless(
    argv: &[String],
    timeout: Duration,
    events: Sender<Value>,
    cancel: impl Future<Output = Cancel>,
    unless: Unless<'_>,
    lease: Option<Lease>,
) -> Value {
    // The command runs in the helper's home, as a shell there would.
    run_at(argv, home(), timeout, events, cancel, unless, lease).await
}

/// A command running in a process group of its own.
pub(in crate::daemon) struct Spawned {
    pub(in crate::daemon) child: Owned,
    /// Its standard output and error, merged.
    pipe: pipe::Receiver,
    pub(in crate::daemon) group: Pid,
    started: Instant,
    /// What its supervisor says of the program, when it has one.
    pub(in crate::daemon) told: Option<Told>,
    /// Its supervisor, as read while it was still held unreaped.
    pub(in crate::daemon) supervisor: Option<atlas_core::process::Instance>,
}

/// Who holds the command's own child, unreaped while its group may still
/// be signalled.
pub(in crate::daemon) enum Owned {
    /// The command, which reaps it.
    Here(tokio::process::Child),
    /// Its supervisor's keeper, which reaps it once this is dropped: once
    /// the command is done signalling its group.
    /// Held only to be dropped, which lets its keeper reap the supervisor.
    Kept(oneshot::Sender<()>),
}

/// The supervisor's end of the lifeline, as the daemon reads it: that the
/// program started, then how it ended, each said once; and what its keeper
/// said of the word to start.
pub(in crate::daemon) struct Told {
    lifeline: OwnedReadHalf,
    said: Vec<u8>,
    started: bool,
    pub(in crate::daemon) verdict: oneshot::Receiver<Verdict>,
    /// Set once the command stops waiting for its program to start: its
    /// keeper gives no word to start after.
    starting: Arc<std::sync::Mutex<bool>>,
}

/// One frame a supervisor said.
pub(in crate::daemon) enum Heard {
    Started,
    /// A stop the daemon asked for has begun.
    Stopping,
    Ended(ExitStatus),
    /// It went without saying more, or said something else.
    Gone,
}

impl Told {
    /// The next frame the supervisor says. Anything it said before it went
    /// is read first.
    pub(in crate::daemon) async fn hear(&mut self) -> Heard {
        let mut chunk = [0; invocation::FRAME];
        loop {
            if let Some(end) = self.said.iter().position(|byte| *byte == b'\n') {
                let frame: Vec<u8> = self.said.drain(..=end).collect();
                if frame == invocation::STARTED && !self.started {
                    self.started = true;
                    return Heard::Started;
                }
                if frame == invocation::STOPPING {
                    return Heard::Stopping;
                }
                return invocation::heard(&frame).map_or(Heard::Gone, Heard::Ended);
            }
            if self.said.len() >= invocation::FRAME {
                return Heard::Gone;
            }
            match self.lifeline.read(&mut chunk).await {
                Ok(0) | Err(_) => return Heard::Gone,
                Ok(count) => self.said.extend_from_slice(&chunk[..count]),
            }
        }
    }

    /// How the program ended, once the supervisor says it; none when it is
    /// gone without saying.
    async fn ended(&mut self) -> Option<ExitStatus> {
        loop {
            match self.hear().await {
                Heard::Started | Heard::Stopping => {}
                Heard::Ended(status) => return Some(status),
                Heard::Gone => return None,
            }
        }
    }
}

/// Spawn with merged output in a group of its own, retaining the child until
/// pipe setup either succeeds or has killed and reaped it. Under a `lease`,
/// the group is its supervisor's, and the program runs in it once its keeper
/// says so; its output goes to `log` when one is given, as a job's does, and
/// to the pipe otherwise.
pub(in crate::daemon) async fn spawn(
    argv: &[String],
    home: Option<&Path>,
    unless: Unless<'_>,
    lease: Option<Lease>,
    log: Option<OwnedFd>,
) -> Result<Spawned, Value> {
    // The protocol never passes an empty argv; refuse one rather than panic.
    let Some((name, arguments)) = argv.split_first() else {
        return Err(failure(Code::SpawnFailed, "No command to run"));
    };

    let (executable, path) = program(name, home)?;
    let (reader, writer) =
        atlas_core::spawn::pipe().map_err(|error| failure(Code::SpawnFailed, error.to_string()))?;

    let unstarted = |error: std::io::Error| failure(Code::SpawnFailed, format!("{name}: {error}"));
    let mut lifeline = None;
    let mut command = match &lease {
        None => {
            let errors = writer.try_clone().map_err(unstarted)?;
            let mut command = Command::new(&executable);
            command
                .arg0(name)
                .args(arguments)
                .stdin(Stdio::null())
                .stdout(Stdio::from(OwnedFd::from(writer)))
                .stderr(Stdio::from(OwnedFd::from(errors)));
            command
        }
        Some(lease) => {
            let (daemon, theirs) = atlas_core::spawn::socket_pair().map_err(unstarted)?;
            let invocation = Invocation {
                lease: LEASE,
                proofs: lease.proofs.clone(),
                argv0: name.into(),
                program: executable.clone(),
                arguments: arguments.iter().map(Into::into).collect(),
            };
            let mut command = Command::new(lease.supervisor.program());
            command
                .args(lease.supervisor.arguments(&invocation))
                .stdin(Stdio::from(OwnedFd::from(theirs)))
                .stdout(Stdio::null())
                .stderr(Stdio::from(log.unwrap_or_else(|| OwnedFd::from(writer))));
            lifeline = Some(daemon);
            command
        }
    };
    command.process_group(0).kill_on_drop(false);
    in_helper(&mut command, &path, home);
    let mut refusal = None;
    let spawned = atlas_core::spawn::spawn_async_if(&mut command, || {
        refusal = unless();
        refusal.is_none()
    });
    // The command keeps this process's copies of the pipe's write end.
    drop(command);
    let Some(mut child) = spawned.map_err(unstarted)? else {
        return Err(
            refusal.unwrap_or_else(|| failure(Code::Cancelled, "The command was not started"))
        );
    };

    let started = Instant::now();

    let Some(group) = child.id().and_then(|pid| i32::try_from(pid).ok()) else {
        return Err(interrupted(
            Code::SpawnFailed,
            "The command exited before it could be observed",
        ));
    };

    let group = Pid::from_raw(group);

    // Checks that this is a pipe and makes it non-blocking.
    let set_up = pipe::Receiver::from_owned_fd(OwnedFd::from(reader)).and_then(|pipe| {
        let halves = lifeline
            .map(|lifeline| {
                lifeline.set_nonblocking(true)?;
                Ok::<_, std::io::Error>(tokio::net::UnixStream::from_std(lifeline)?.into_split())
            })
            .transpose()?;
        Ok((pipe, halves))
    });
    let (pipe, halves) = match set_up {
        Ok(set_up) => set_up,
        Err(error) => {
            signal_group(group, Signal::SIGKILL);
            let _ = child.wait().await;
            return Err(interrupted(Code::SpawnFailed, error.to_string()));
        }
    };
    let (child, told, supervisor) = match (halves, lease) {
        (Some((reading, writing)), Some(lease)) => {
            // A supervisor told nothing starts nothing: one this cannot name
            // is let go, and ends itself.
            let supervisor = atlas_core::process::start(group.as_raw())
                .map(|start| atlas_core::process::Instance::new(group.as_raw(), start))
                .map_err(|error| failure(Code::SpawnFailed, error.error))?;
            let (verdict, hearing) = oneshot::channel();
            let (released, releasing) = oneshot::channel();
            let starting = Arc::new(std::sync::Mutex::new(false));
            tokio::spawn(keeper::keep(
                supervisor,
                writing,
                lease,
                verdict,
                starting.clone(),
                child,
                releasing,
            ));
            let told = Told {
                lifeline: reading,
                said: Vec::new(),
                started: false,
                verdict: hearing,
                starting,
            };
            (Owned::Kept(released), Some(told), Some(supervisor))
        }
        _ => (Owned::Here(child), None, None),
    };

    Ok(Spawned {
        child,
        pipe,
        group,
        started,
        told,
        supervisor,
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
    /// It runs under a supervisor, the leader of its group.
    supervised: bool,
    /// What tells a supervised command's keeper the command no longer
    /// waits for its program to start.
    starting: Option<Arc<std::sync::Mutex<bool>>>,
    /// Where what it prints is kept with what it comes to, when it is.
    printed: Option<Arc<std::sync::Mutex<Printed>>>,
}

impl Watch {
    fn terminate(&mut self, group: Pid) {
        if let Some(starting) = &self.starting {
            *starting
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
        }
        if self.supervised {
            // The supervisor alone passes it on, once to each process it
            // started: a second SIGTERM would cut a program's own grace
            // short.
            let _ = nix::sys::signal::kill(group, Signal::SIGTERM);
        } else {
            signal_group(group, Signal::SIGTERM);
        }
        self.stopping = true;
    }

    fn read(&mut self, read: std::io::Result<usize>, buffer: &[u8], events: &Sender<Value>) {
        match read {
            Ok(0) => self.closed = true,
            Err(_) => self.abandon_pipe(),
            Ok(count) if !self.cause.abandoned() => {
                printing(self.printed.as_ref(), |printed| {
                    printed.push(&buffer[..count])
                });
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

/// How a command's end was observed.
enum Exit {
    /// Its own child exited, observed without reaping it.
    Child(std::io::Result<()>),
    /// Its supervisor said how the program ended, or went without saying.
    Told(Option<ExitStatus>),
}

/// The command's end: its supervisor's word, when it has one, as the
/// supervisor exiting is not the program's end; otherwise its child's exit.
async fn exit(child: &Owned, told: Option<&mut Told>) -> Exit {
    match (told, child) {
        (Some(told), _) => Exit::Told(told.ended().await),
        (None, Owned::Here(child)) => Exit::Child(child_exited(child).await),
        (None, Owned::Kept(_)) => std::future::pending().await,
    }
}

/// The reply for a supervised command whose supervisor went without saying
/// how the program ended: no status is made up for it.
fn untold(timed_out: bool) -> Value {
    if timed_out {
        let mut reply = interrupted(
            Code::WaitFailed,
            "The command timed out and was ended; how it ended is not known",
        );
        reply["timedOut"] = json!(true);
        return reply;
    }
    interrupted(
        Code::WaitFailed,
        "The command's supervisor ended without saying how the command ended",
    )
}

/// What `cancel` completed with, if it already has.
fn ready(cancel: Pin<&mut impl Future<Output = Cancel>>) -> Option<Cancel> {
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    match cancel.poll(&mut context) {
        std::task::Poll::Ready(reason) => Some(reason),
        std::task::Poll::Pending => None,
    }
}

/// [`run_unless`] with `home` as the helper's home.
async fn run_at(
    argv: &[String],
    home: Option<PathBuf>,
    timeout: Duration,
    events: Sender<Value>,
    cancel: impl Future<Output = Cancel>,
    unless: Unless<'_>,
    lease: Option<Lease>,
) -> Value {
    let printed = lease.as_ref().and_then(|lease| lease.printed.clone());
    let mut spawned = match spawn(argv, home.as_deref(), unless, lease, None).await {
        Ok(spawned) => spawned,
        Err(refusal) => return refusal,
    };
    let group = spawned.group;
    let supervised = spawned.told.is_some();
    let mut watch = Watch {
        supervised,
        starting: spawned.told.as_ref().map(|told| told.starting.clone()),
        printed,
        ..Watch::default()
    };
    printing(watch.printed.as_ref(), |printed| {
        printed.started = Some(group.as_raw());
    });
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
            observed = exit(&spawned.child, spawned.told.as_mut()), if !exited => {
                exited = true;
                match observed {
                    // No termination began, so no later group signal is
                    // permitted: preserve prompt ordinary reaping even
                    // while a descendant still holds the output pipe.
                    Exit::Child(Ok(())) if !watch.stopping => {
                        if let Owned::Here(child) = &mut spawned.child {
                            watch.status = Some(child.wait().await);
                        }
                    }
                    Exit::Child(Ok(())) | Exit::Told(None) => {}
                    Exit::Child(Err(error)) => watch.status = Some(Err(error)),
                    Exit::Told(Some(status)) => watch.status = Some(Ok(status)),
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

    // A command that already finished keeps whatever it left running. One
    // ended here is ended whole before the reply; a supervisor, its PID
    // keeping the group, passes the ending on to what left the group.
    if watch.stopping && supervised {
        finish_group(group).await;
    } else if watch.stopping && watch.status.is_none() {
        end_group(group).await;
    }
    // A supervisor is never waited for: what the program left lives under
    // the lease, and the runtime reaps it once it ends.
    if watch.status.is_none()
        && let Owned::Here(child) = &mut spawned.child
    {
        watch.status = Some(child.wait().await);
    }
    // A supervised program that never said it started: nothing ran if its
    // keeper never gave the word, which it says once it has decided.
    if let Some(told) = spawned.told.as_mut()
        && !told.started
        && watch.status.is_none()
    {
        let decided = tokio::select! {
            decided = &mut told.verdict => decided.ok(),
            reason = &mut cancel, if !watch.cause.abandoned() => {
                watch.cause = StopCause::Cancelled(reason);
                None
            }
        };
        if let Some(Verdict::Refused(refusal)) = decided {
            // Ended before it started: what ended it says so, and nothing
            // ran.
            return match watch.cause {
                StopCause::Cancelled(reason) => {
                    let mut reply = reason.reply();
                    reply["uncertain"] = json!(false);
                    reply
                }
                StopCause::Running | StopCause::Timeout => refusal,
            };
        }
    }
    // A supervised command's work may have been ended by what also cancels
    // it (a Stop ends its lease) before the cancel was seen, or while it was
    // being ended: the reply names the cancel, never an unknown end.
    if supervised
        && !watch.cause.abandoned()
        && let Some(reason) = ready(cancel.as_mut())
    {
        watch.cause = StopCause::Cancelled(reason);
    }

    let timed_out = match watch.cause {
        StopCause::Cancelled(reason) => return reason.reply(),
        StopCause::Running => false,
        StopCause::Timeout => true,
    };

    if watch.output.finish(&events).await.is_err() {
        return Cancel::Left.reply();
    }
    if supervised && let Some(reason) = ready(cancel.as_mut()) {
        return reason.reply();
    }

    match watch.status {
        Some(Ok(status)) => ended(
            status,
            timed_out,
            spawned.started,
            watch.output.truncated,
            watch.unfinished,
        ),
        None if supervised => untold(timed_out),
        Some(Err(_)) | None => interrupted(
            Code::WaitFailed,
            "The command's exit status could not be read",
        ),
    }
}

pub(crate) mod keeper;

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "run_supervised_tests.rs"]
mod supervised_tests;
