//! `run`: one command inside the helper login. Its standard output and error
//! share one pipe and stream back as base64 `output` events; the final reply
//! reports how it ended. The command gets its own process group, so a timeout
//! ends everything it started, and a daemon restart does not.
//!
//! Supervision never waits on the client: events go to a bounded channel that
//! a separate writer drains. Output is coalesced into a bounded number of
//! events; what the client cannot take is dropped and reported as truncated.
//! A client that goes away, or a daemon that stops, ends the command.
use crate::daemon::protocol::failure;
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
pub const EVENTS: usize = 256;
/// Output events per run, leaving room in what the client accepts per reply
/// for the `started` event.
const MAX_EVENTS: usize = 1000;
const _: () = assert!(MAX_EVENTS < crate::transport::MAX_REPLY_EVENTS);
/// Pending output sent at once, without waiting for more.
const FLUSH: usize = 64 * 1024;
/// Output held back for a slow client before it is dropped.
const HELD: usize = 1024 * 1024;
/// How long the last output waits for a slow client.
const FINISH: Duration = Duration::from_secs(2);
const PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

/// The executable for `program`: as given when it contains a slash, else the
/// first match on `PATH`.
fn resolve(program: &str, path: Option<&str>) -> Option<PathBuf> {
    let executable = |candidate: &Path| {
        std::fs::metadata(candidate)
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    };
    if program.contains('/') {
        let candidate = PathBuf::from(program);
        return executable(&candidate).then_some(candidate);
    }
    path.unwrap_or(PATH)
        .split(':')
        .filter(|directory| !directory.is_empty())
        .map(|directory| Path::new(directory).join(program))
        .find(|candidate| executable(candidate))
}

/// The conventional name of a macOS signal number, as "SIGTERM".
pub(crate) fn signal_name(signal: i32) -> String {
    Signal::try_from(signal)
        .map(|signal| signal.as_str().to_owned())
        .unwrap_or_else(|_| format!("SIG{signal}"))
}

/// The final reply for a command that ran.
fn ended(status: ExitStatus, timed_out: bool, started: Instant, truncated: bool) -> Value {
    let mut reply = json!({
        "ok": true,
        "exitCode": status.code(),
        "signal": status.signal().map(signal_name),
        "timedOut": timed_out,
        "durationMs": u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    });
    if truncated {
        reply["outputTruncated"] = json!(true);
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
    fn new() -> Self {
        Self {
            pending: Vec::new(),
            due: None,
            forwarded: 0,
            sent: 0,
            truncated: false,
        }
    }

    /// Hold `bytes` for sending, within the output budget.
    fn push(&mut self, bytes: &[u8]) {
        let room = MAX_OUTPUT.saturating_sub(self.forwarded + self.pending.len());
        let kept = bytes.len().min(room);
        self.truncated |= kept < bytes.len();
        self.pending.extend_from_slice(&bytes[..kept]);
        if self.due.is_none() && !self.pending.is_empty() {
            // Early output streams promptly; a long trickle is batched.
            let wait = if self.sent < MAX_EVENTS / 2 {
                Duration::from_millis(50)
            } else {
                Duration::from_secs(1)
            };
            self.due = Some(tokio::time::Instant::now() + wait);
        }
    }

    fn full(&self) -> bool {
        self.pending.len() >= FLUSH
    }

    /// Send what is pending as one event. A slow client leaves it pending
    /// until it grows past what is held back, then it is dropped.
    fn flush(&mut self, events: &Sender<Value>) -> Result<(), Gone> {
        self.due = None;
        if self.pending.is_empty() {
            return Ok(());
        }
        if self.sent >= MAX_EVENTS {
            self.truncated = true;
            self.pending.clear();
            return Ok(());
        }
        let event = json!({"event": "output", "data": STANDARD.encode(&self.pending)});
        match events.try_send(event) {
            Ok(()) => {
                self.forwarded += self.pending.len();
                self.sent += 1;
                self.pending.clear();
            }
            Err(TrySendError::Full(_)) if self.pending.len() < HELD => {
                self.due = Some(tokio::time::Instant::now() + Duration::from_millis(50));
            }
            Err(TrySendError::Full(_)) => {
                self.truncated = true;
                self.pending.clear();
            }
            Err(TrySendError::Closed(_)) => return Err(Gone),
        }
        Ok(())
    }

    /// Deliver the last pending output, waiting a bounded time for a slow
    /// client; whatever cannot be delivered is reported as truncated.
    async fn finish(&mut self, events: &Sender<Value>) -> Result<(), Gone> {
        self.due = None;
        if self.pending.is_empty() {
            return Ok(());
        }
        if self.sent >= MAX_EVENTS {
            self.truncated = true;
            self.pending.clear();
            return Ok(());
        }
        let event = json!({"event": "output", "data": STANDARD.encode(&self.pending)});
        match events.send_timeout(event, FINISH).await {
            Ok(()) => {
                self.forwarded += self.pending.len();
                self.sent += 1;
            }
            Err(SendTimeoutError::Timeout(_)) => self.truncated = true,
            Err(SendTimeoutError::Closed(_)) => return Err(Gone),
        }
        self.pending.clear();
        Ok(())
    }
}

/// Run `argv`, sending its events to `events`, and return the final reply.
/// When `cancel` completes — the client left or the daemon is stopping — or
/// the client stops taking events, the group is ended before returning.
pub async fn run(
    argv: &[String],
    timeout: Duration,
    events: Sender<Value>,
    cancel: impl Future<Output = ()>,
) -> Value {
    // The protocol never passes an empty argv; refuse one rather than panic.
    let Some(name) = argv.first() else {
        return failure("spawn-failed", "No command to run");
    };
    let Some(program) = resolve(name, std::env::var("PATH").ok().as_deref()) else {
        return failure("spawn-failed", format!("{name}: command not found on PATH"));
    };
    let (reader, writer) = match std::io::pipe() {
        Ok(pair) => pair,
        Err(error) => return failure("spawn-failed", error.to_string()),
    };
    let spawned = (|| {
        let errors = writer.try_clone()?;
        let mut command = Command::new(&program);
        command
            .arg0(&argv[0])
            .args(&argv[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::from(OwnedFd::from(writer)))
            .stderr(Stdio::from(OwnedFd::from(errors)))
            .process_group(0)
            .kill_on_drop(false);
        command.spawn()
    })();
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => return failure("spawn-failed", format!("{}: {error}", argv[0])),
    };
    let started = Instant::now();
    let Some(group) = child.id().and_then(|pid| i32::try_from(pid).ok()) else {
        return failure(
            "spawn-failed",
            "The command exited before it could be observed",
        );
    };
    let pid = group;
    let group = Pid::from_raw(group);
    // Checks that this is a pipe and makes it non-blocking.
    let mut pipe = match pipe::Receiver::from_owned_fd(OwnedFd::from(reader)) {
        Ok(receiver) => receiver,
        Err(error) => {
            signal_group(group, Signal::SIGKILL);
            let _ = child.wait().await;
            return failure("spawn-failed", error.to_string());
        }
    };
    let mut abandoned = events
        .try_send(json!({"event": "started", "pid": pid}))
        .is_err();
    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline, cancel);
    let mut buffer = vec![0; FLUSH];
    let mut output = Output::new();
    let mut status = None;
    // SIGTERM went to the group; the deadline now marks when SIGKILL follows.
    let mut stopping = false;
    let mut timed_out = false;
    let mut open = true;
    let mut linger = None;
    if abandoned {
        signal_group(group, Signal::SIGTERM);
        stopping = true;
        deadline.as_mut().reset(tokio::time::Instant::now() + GRACE);
    }
    while status.is_none() || open {
        tokio::select! {
            read = pipe.read(&mut buffer), if open => match read {
                Ok(0) | Err(_) => open = false,
                Ok(count) if !abandoned => {
                    output.push(&buffer[..count]);
                    if output.full() && output.flush(&events).is_err() {
                        abandoned = true;
                    }
                }
                Ok(_) => {}
            },
            () = tokio::time::sleep_until(output.due.unwrap_or_else(tokio::time::Instant::now)),
                if output.due.is_some() && !abandoned => {
                if output.flush(&events).is_err() {
                    abandoned = true;
                }
            }
            exited = child.wait(), if status.is_none() => {
                status = Some(exited);
                linger = Some(tokio::time::Instant::now() + LINGER);
            }
            () = &mut deadline, if status.is_none() => {
                if stopping {
                    signal_group(group, Signal::SIGKILL);
                } else {
                    signal_group(group, Signal::SIGTERM);
                    stopping = true;
                    timed_out = true;
                }
                deadline.as_mut().reset(tokio::time::Instant::now() + GRACE);
            }
            () = &mut cancel, if !abandoned => abandoned = true,
            () = tokio::time::sleep_until(linger.unwrap_or_else(tokio::time::Instant::now)),
                if linger.is_some() && open => {
                open = false;
            }
        }
        if abandoned && !stopping && status.is_none() {
            signal_group(group, Signal::SIGTERM);
            stopping = true;
            deadline.as_mut().reset(tokio::time::Instant::now() + GRACE);
        }
    }
    // A command that already finished keeps whatever it left running.
    if stopping {
        end_group(group).await;
    }
    if abandoned {
        return failure("transport", "The client went away");
    }
    if output.finish(&events).await.is_err() {
        return failure("transport", "The client went away");
    }
    match status {
        Some(Ok(status)) => ended(status, timed_out, started, output.truncated),
        _ => failure("wait-failed", "The command's exit status could not be read"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nix::sys::signal::kill;
    use tokio::sync::mpsc;

    async fn execute(argv: &[&str], timeout_ms: u64) -> (Vec<Value>, Value) {
        let argv: Vec<String> = argv.iter().map(|a| (*a).to_owned()).collect();
        let (sender, mut receiver) = mpsc::channel(EVENTS);
        let collected = tokio::spawn(async move {
            let mut events = Vec::new();
            while let Some(event) = receiver.recv().await {
                events.push(event);
            }
            events
        });
        let reply = run(
            &argv,
            Duration::from_millis(timeout_ms),
            sender,
            std::future::pending(),
        )
        .await;
        (collected.await.unwrap(), reply)
    }

    fn output(events: &[Value]) -> String {
        let bytes: Vec<u8> = events
            .iter()
            .filter(|event| event["event"] == "output")
            .flat_map(|event| STANDARD.decode(event["data"].as_str().unwrap()).unwrap())
            .collect();
        String::from_utf8(bytes).unwrap()
    }

    fn group_alive(group: i64) -> bool {
        signal_group(Pid::from_raw(group as i32), None)
    }

    #[tokio::test]
    async fn a_command_streams_merged_output_then_its_exit() {
        let (events, reply) =
            execute(&["/bin/sh", "-c", "echo out; echo err >&2; exit 3"], 5000).await;
        assert_eq!(events[0]["event"], "started");
        assert!(events[0]["pid"].as_u64().unwrap() > 1);
        assert_eq!(output(&events), "out\nerr\n");
        assert_eq!(reply["ok"], true);
        assert_eq!(reply["exitCode"], 3);
        assert_eq!(reply["signal"], Value::Null);
        assert_eq!(reply["timedOut"], false);
        let (events, reply) = execute(&["echo", "on", "path"], 5000).await;
        assert_eq!(
            (output(&events).as_str(), &reply["exitCode"]),
            ("on path\n", &json!(0))
        );
    }

    #[tokio::test]
    async fn a_timeout_ends_the_whole_group_with_sigterm() {
        let started = Instant::now();
        let (events, reply) = execute(&["/bin/sh", "-c", "sleep 30 & sleep 30"], 300).await;
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(reply["timedOut"], true);
        assert_eq!(reply["exitCode"], Value::Null);
        assert_eq!(reply["signal"], "SIGTERM");
        let group = events[0]["pid"].as_i64().unwrap();
        assert!(!group_alive(group), "no member of the group survives");
        let (_, reply) = execute(&["/bin/sh", "-c", "kill -TERM $$"], 5000).await;
        assert_eq!(
            (&reply["signal"], &reply["timedOut"]),
            (&json!("SIGTERM"), &json!(false))
        );
    }

    /// The leader dies on SIGTERM; a descendant that ignores it is killed too.
    #[tokio::test]
    async fn a_timeout_kills_descendants_that_ignore_sigterm() {
        let (events, reply) =
            execute(&["/bin/sh", "-c", "(trap '' TERM; sleep 30) & wait"], 300).await;
        assert_eq!(reply["timedOut"], true);
        let group = events[0]["pid"].as_i64().unwrap();
        assert!(
            !group_alive(group),
            "the descendant was killed before the reply"
        );
    }

    #[tokio::test]
    async fn something_left_running_does_not_hold_the_reply() {
        let started = Instant::now();
        let (events, reply) = execute(&["/bin/sh", "-c", "sleep 5 & echo $!"], 5000).await;
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "returned after the shell exited"
        );
        assert_eq!(reply["exitCode"], 0);
        let child = Pid::from_raw(output(&events).trim().parse().unwrap());
        assert!(
            kill(child, None).is_ok(),
            "a background process outlives the run"
        );
        kill(child, Signal::SIGKILL).unwrap();
    }

    /// A client that stops reading cannot stall supervision or the timeout.
    #[tokio::test]
    async fn a_client_that_never_reads_cannot_stall_the_timeout() {
        let (sender, _receiver) = mpsc::channel(1);
        let argv = ["/bin/sh".to_owned(), "-c".to_owned(), "yes".to_owned()];
        let started = Instant::now();
        let reply = run(
            &argv,
            Duration::from_millis(300),
            sender,
            std::future::pending(),
        )
        .await;
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(
            (&reply["timedOut"], &reply["outputTruncated"]),
            (&json!(true), &json!(true))
        );
        let (sender, receiver) = mpsc::channel(1);
        drop(receiver);
        let gone = run(
            &argv,
            Duration::from_secs(30),
            sender,
            std::future::pending(),
        )
        .await;
        assert_eq!(
            gone["code"], "transport",
            "a departed client ends the command"
        );
    }

    /// Cancelling ends a command that prints nothing, and its descendants.
    #[tokio::test]
    async fn cancelling_ends_a_quiet_command_and_its_group() {
        let (sender, mut receiver) = mpsc::channel(EVENTS);
        let argv = [
            "/bin/sh".to_owned(),
            "-c".to_owned(),
            "(trap '' TERM; sleep 30) & sleep 30".to_owned(),
        ];
        let started = Instant::now();
        let cancel = tokio::time::sleep(Duration::from_millis(300));
        let reply = run(&argv, Duration::from_secs(60), sender, cancel).await;
        assert!(started.elapsed() < Duration::from_secs(8));
        assert_eq!(reply["code"], "transport");
        let group = receiver.recv().await.unwrap()["pid"].as_i64().unwrap();
        assert!(!group_alive(group), "nothing the command started survives");
    }

    /// The last output waits for a slow client instead of being dropped, and
    /// output that cannot be delivered is reported as truncated.
    #[tokio::test]
    async fn the_last_output_reaches_a_slow_client_or_is_reported() {
        let argv = [
            "/bin/sh".to_owned(),
            "-c".to_owned(),
            "printf one; sleep 0.3; printf two".to_owned(),
        ];
        // One slot: `started` fills it until the client reads, 1 s later.
        let (sender, mut receiver) = mpsc::channel(1);
        let slow = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let mut events = Vec::new();
            while let Some(event) = receiver.recv().await {
                events.push(event);
            }
            events
        });
        let reply = run(
            &argv,
            Duration::from_secs(10),
            sender,
            std::future::pending(),
        )
        .await;
        let events = slow.await.unwrap();
        assert_eq!(output(&events), "onetwo");
        assert_eq!(reply.get("outputTruncated"), None);
        let (sender, _never) = mpsc::channel(1);
        let reply = run(
            &argv,
            Duration::from_secs(20),
            sender,
            std::future::pending(),
        )
        .await;
        assert_eq!(reply["outputTruncated"], true);
    }

    /// Output written a little at a time stays within the client's event
    /// limit and arrives whole.
    #[tokio::test]
    async fn trickled_output_is_coalesced_into_few_events() {
        let (events, reply) = execute(
            &[
                "/bin/sh",
                "-c",
                "i=0; while [ $i -lt 2000 ]; do printf x; i=$((i+1)); done",
            ],
            20000,
        )
        .await;
        assert_eq!(reply["exitCode"], 0);
        assert_eq!(reply.get("outputTruncated"), None);
        assert_eq!(output(&events), "x".repeat(2000));
        assert!(events.len() < 100, "{} events", events.len());
    }

    #[tokio::test]
    async fn missing_commands_and_huge_output_are_reported() {
        let (events, reply) = execute(&["/nonexistent/binary"], 3000).await;
        assert!(events.is_empty());
        assert_eq!(
            reply,
            failure(
                "spawn-failed",
                "/nonexistent/binary: command not found on PATH"
            )
        );
        let (_, reply) = execute(&["no-such-command-atlas"], 3000).await;
        assert_eq!(reply["code"], "spawn-failed");
        let (events, reply) = execute(&[], 3000).await;
        assert!(events.is_empty());
        assert_eq!(reply, failure("spawn-failed", "No command to run"));
        let (events, reply) =
            execute(&["/bin/sh", "-c", "head -c 17000000 /dev/zero"], 20000).await;
        assert_eq!(reply["outputTruncated"], true);
        let forwarded: usize = events
            .iter()
            .filter(|event| event["event"] == "output")
            .map(|event| {
                STANDARD
                    .decode(event["data"].as_str().unwrap())
                    .unwrap()
                    .len()
            })
            .sum();
        assert!(forwarded <= MAX_OUTPUT);
    }

    #[test]
    fn signals_have_conventional_names() {
        assert_eq!(signal_name(Signal::SIGTERM as i32), "SIGTERM");
        assert_eq!(signal_name(Signal::SIGKILL as i32), "SIGKILL");
        assert_eq!(signal_name(Signal::SIGXCPU as i32), "SIGXCPU");
        assert_eq!(signal_name(Signal::SIGXFSZ as i32), "SIGXFSZ");
        assert_eq!(signal_name(Signal::SIGSYS as i32), "SIGSYS");
        assert_eq!(signal_name(Signal::SIGINFO as i32), "SIGINFO");
        assert_eq!(signal_name(64), "SIG64");
    }
}
