//! One request to a helper-side socket and its reply: a JSON line in, event
//! lines and a final reply out, within one deadline, from a peer the kernel
//! says is the helper account.
use crate::{
    error::{Code, Error, Result},
    protocol::{MAX_REPLY_EVENTS, MAX_REQUEST},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Map, Value, json};
use std::{
    future::Future,
    io,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::UnixStream,
    time::{Instant, sleep, sleep_until, timeout_at},
};

/// Longest the request itself may take to send. Both helper-side servers act
/// only on a whole line, so a request cut short changed nothing.
const DISPATCH: Duration = Duration::from_millis(500);
/// How long a failed send looks for a refusal the peer already sent.
const WAITING: Duration = Duration::from_millis(100);
const IDENTITY_RETRY: Duration = Duration::from_millis(10);
/// Pauses before connecting again after the kernel denied a connection
/// (EACCES), 175 ms in all. This account's search of the helper's folders
/// rests on an ACL entry the kernel checks through an identity lookup, which
/// can briefly fail under load.
const DENIED_PAUSES: [Duration; 3] = [
    Duration::from_millis(25),
    Duration::from_millis(50),
    Duration::from_millis(100),
];
/// Longest reply, every line together.
const MAX_REPLY: usize = 64 * 1024 * 1024;

#[derive(Clone)]
pub struct Transport {
    pub path: PathBuf,
    pub timeout: Duration,
    /// Only this account may serve the socket. Endpoints live in shared
    /// directories, so the kernel-reported peer, not the reply, proves identity.
    pub peer_uid: u32,
}

/// A request under way on a verified connection: sent with
/// [`Pending::dispatch`], answered by [`Pending::finish`]. It works over any
/// stream of bytes; [`Transport::connect`] gives one over a Unix socket.
pub struct Pending<S = UnixStream> {
    stream: S,
    deadline: Instant,
    started: Instant,
}

impl Transport {
    /// # Errors
    ///
    /// Returns the failure to connect or to verify the peer.
    pub async fn connect(&self) -> Result<Pending> {
        let started = Instant::now();
        let deadline = started + self.timeout;
        let stream = self
            .connect_verified(
                deadline,
                || UnixStream::connect(&self.path),
                |stream| stream.peer_cred().map(|cred| cred.uid()),
            )
            .await?;

        Ok(Pending {
            stream,
            deadline,
            started,
        })
    }

    async fn connect_verified<S, F: Future<Output = io::Result<S>>>(
        &self,
        deadline: Instant,
        mut connect: impl FnMut() -> F,
        mut peer_uid: impl FnMut(&S) -> io::Result<u32>,
    ) -> Result<S> {
        let mut pauses = DENIED_PAUSES.into_iter();
        loop {
            let timed_out = || {
                Error::new(
                    Code::Transport,
                    format!("Timed out connecting to socket {}", self.path.display()),
                )
            };
            if Instant::now() >= deadline {
                return Err(timed_out());
            }
            let stream = timeout_at(deadline, retry_denied(deadline, &mut pauses, &mut connect))
                .await
                .map_err(|cause| timed_out().with_source(cause))?
                .map_err(|cause| refused(&self.path, cause))?;
            if Instant::now() >= deadline {
                return Err(timed_out());
            }
            let uid = match peer_uid(&stream) {
                Ok(uid) => uid,
                Err(cause) => {
                    drop(stream);
                    // Darwin's peer PID lookup can lose a peer that closed after
                    // connect. Nothing was sent; only a fresh, verified peer is usable.
                    let retry = cause.raw_os_error() == Some(libc::ENOTCONN);
                    let expired = retry
                        && (timeout_at(deadline, sleep(IDENTITY_RETRY)).await.is_err()
                            || Instant::now() >= deadline);
                    if retry && !expired {
                        continue;
                    }
                    let step = if expired {
                        "Timed out reading the peer account for socket"
                    } else {
                        "Could not read the peer account for socket"
                    };
                    return Err(Error::new(
                        if expired { Code::Transport } else { Code::Io },
                        format!("{step} {}: {cause}", self.path.display()),
                    )
                    .with_source(cause));
                }
            };
            if uid != self.peer_uid {
                return Err(Error::new(
                    Code::Identity,
                    format!(
                        "Could not verify the peer account for socket {}: it is not served by the configured helper account",
                        self.path.display()
                    ),
                ));
            }
            return Ok(stream);
        }
    }

    /// A connection of its own to this transport's service, for a lane that
    /// reads its own reply, made by `until`, when the kernel says process
    /// `pid` of this transport's account is at its other end; none when
    /// another process is.
    ///
    /// # Errors
    ///
    /// Returns the failure to connect, then the failure to read the peer.
    pub async fn lane(&self, pid: u32, until: Instant) -> Result<Option<UnixStream>> {
        let stream = connect_socket(&self.path, until).await?;
        Ok(self.served_by(&stream, pid)?.then_some(stream))
    }

    /// Whether the kernel says the peer of `stream` is this transport's
    /// account, and process `pid` of it: what a lane that reads its own
    /// reply checks before it trusts one.
    fn served_by(&self, stream: &UnixStream, pid: u32) -> io::Result<bool> {
        let peer = stream.peer_cred()?;
        Ok(peer.uid() == self.peer_uid
            && peer.pid().and_then(|pid| u32::try_from(pid).ok()) == Some(pid))
    }

    /// Exchange one request with the verified socket peer within this transport's deadline.
    ///
    /// # Errors
    ///
    /// Returns peer-UID, connection, framing, timeout or reply failures. A failure after
    /// dispatch can be uncertain; the transport never makes a lost mutation safe to replay.
    pub async fn call(&self, request: &Value) -> Result<Value> {
        self.connect()
            .await?
            .dispatch(request)
            .await?
            .finish()
            .await
    }
}

/// Connect to the socket at `path`, trying again briefly while the kernel
/// denies the connection, never pausing up to `deadline`. The caller verifies
/// the peer.
///
/// # Errors
///
/// Returns the last failure to connect, as the kernel gave it.
pub async fn connect_socket(path: &Path, deadline: Instant) -> io::Result<UnixStream> {
    retry_denied(deadline, &mut DENIED_PAUSES.into_iter(), &mut || {
        UnixStream::connect(path)
    })
    .await
}

/// One connection from `connect`, tried again after each of `pauses` while
/// it fails with EACCES and the pause ends before `deadline`; a denial that
/// leaves no time for another try is returned as it is. A connection
/// that failed sent nothing, so trying again cannot repeat a request; only
/// the connection is retried, never a send or a read.
async fn retry_denied<S, F: Future<Output = io::Result<S>>>(
    deadline: Instant,
    pauses: &mut impl Iterator<Item = Duration>,
    connect: &mut impl FnMut() -> F,
) -> io::Result<S> {
    loop {
        match connect().await {
            Err(cause) if cause.raw_os_error() == Some(libc::EACCES) => {
                let wake = pauses.next().map(|pause| Instant::now() + pause);
                let Some(wake) = wake.filter(|wake| *wake < deadline) else {
                    return Err(cause);
                };
                sleep_until(wake).await;
                // A late wake-up never connects past the deadline.
                if Instant::now() >= deadline {
                    return Err(cause);
                }
            }
            outcome => return outcome,
        }
    }
}

/// A failed connection, said plainly. The kernel refuses with EPERM only
/// when policy forbids the connection, which is what an agent's sandbox
/// does to local sockets: the helper may be fine.
fn refused(path: &Path, error: io::Error) -> Error {
    if error.raw_os_error() == Some(libc::EPERM) {
        return Error::new(
            Code::Blocked,
            format!("This shell may not connect to helper socket {} (operation not permitted). An agent's \
             sandbox blocks local sockets: run atlas outside the sandbox. The helper itself \
             may be fine.", path.display()),
        )
        .with_source(error);
    }

    Error::new(
        Code::Io,
        format!("Could not connect to socket {}: {error}", path.display()),
    )
    .with_source(error)
}

impl<S: AsyncRead + AsyncWrite + Unpin> Pending<S> {
    /// # Errors
    ///
    /// Returns an error for an oversized request, or the failure to send it in
    /// time.
    pub async fn dispatch(mut self, request: &Value) -> Result<Self> {
        let mut payload = serde_json::to_vec(request)?;
        if payload.len() > MAX_REQUEST {
            return Err(Error::new(Code::Protocol, "Request exceeds 1 MiB"));
        }

        // The newline goes last, in the same write: JSON escapes any newline
        // inside a string, and the peer acts only once it reads this one.
        payload.push(b'\n');

        let deadline = self.deadline.min(Instant::now() + DISPATCH);
        let written = timeout_at(deadline, self.stream.write_all(&payload)).await;

        // A write that failed or ran out of time never sent the newline, so
        // the peer cannot have acted on the request. A refusal it already
        // sent (busy, say) says why.
        let failed = match written {
            Ok(Ok(())) => return Ok(self),
            Ok(Err(error)) => Error::new(Code::Transport, error.to_string()).with_source(error),
            Err(elapsed) => Error::new(
                Code::Transport,
                "The helper did not take the request in time",
            )
            .with_source(elapsed),
        };

        Err(self.waiting_refusal().await.unwrap_or(failed))
    }
}

impl<S: AsyncRead + Unpin> Pending<S> {
    /// A definite refusal the peer sent before it hung up, if one is waiting.
    async fn waiting_refusal(mut self) -> Option<Error> {
        self.deadline = self.deadline.min(Instant::now() + WAITING);
        let reply = self.finish().await.ok()?;

        (reply["ok"] == false && reply["uncertain"] != true).then(|| refusal(&reply))
    }

    /// # Errors
    ///
    /// Returns the failure to read a complete reply in time.
    pub async fn finish(self) -> Result<Value> {
        let mut events = Vec::new();
        let mut reply = self
            .finish_each(&mut |event| {
                events.push(event);
                Ok(())
            })
            .await?;
        if !events.is_empty() {
            reply["events"] = json!(events);
        }
        Ok(reply)
    }

    /// The final reply, each event line before it handed to `each` as it
    /// arrives rather than kept, so a long reply is never held whole. A
    /// failure of `each` ends the reply there.
    ///
    /// # Errors
    ///
    /// Returns the failure to read a complete reply in time.
    pub async fn finish_each(
        mut self,
        each: &mut (dyn FnMut(Value) -> Result<()> + Send),
    ) -> Result<Value> {
        let deadline = self.deadline;
        timeout_at(deadline, self.receive_each(each))
            .await
            .map_err(|_| {
                lost(Error::new(
                    Code::Transport,
                    "Helper exceeded the request deadline",
                ))
            })?
            .map_err(Error::uncertain)
    }

    async fn receive_each(
        &mut self,
        each: &mut (dyn FnMut(Value) -> Result<()> + Send),
    ) -> Result<Value> {
        let mut pending = Vec::new();
        let mut total = 0;
        let mut events = 0;
        // Keep the receive storage out of every enclosing async future. A stack
        // array multiplies through the HTTP/session futures in debug builds.
        let mut buffer = vec![0; 65536];

        loop {
            let count = self
                .stream
                .read(&mut buffer)
                .await
                .map_err(|cause| lost(cause.into()))?;
            if count == 0 {
                return Err(lost(Error::new(
                    Code::Protocol,
                    "Connection ended without a final reply",
                )));
            }

            total += count;
            if total > MAX_REPLY {
                return Err(Error::new(Code::Protocol, "Reply exceeds 64 MiB"));
            }

            // Scan incoming bytes once; large PNG lines never rescan the prefix.
            let mut offset = 0;

            for end in (0..count).filter(|i| buffer[*i] == b'\n') {
                pending.extend_from_slice(&buffer[offset..end]);
                let mut row: Value = serde_json::from_slice(&pending)?;
                pending.clear();
                offset = end + 1;

                let object = row
                    .as_object_mut()
                    .ok_or_else(|| Error::new(Code::Protocol, "Reply must be a JSON object"))?;
                if final_row(object)? {
                    object.insert(
                        "elapsedMs".into(),
                        json!(self.started.elapsed().as_secs_f64() * 1000.0),
                    );

                    return Ok(row);
                }

                if object.get("event").and_then(Value::as_str).is_none()
                    || events >= MAX_REPLY_EVENTS
                {
                    return Err(Error::new(
                        Code::Protocol,
                        "Malformed or excessive helper events",
                    ));
                }

                events += 1;
                each(row)?;
            }

            pending.extend_from_slice(&buffer[offset..count]);
        }
    }
}

/// Why no final reply was read: its connection ended or stalled before one
/// arrived whole, for the cause it wraps when there is one. The peer may
/// have carried the request out all the same, and may remember what it
/// came to.
#[derive(Debug)]
struct ReplyLost(Option<Box<dyn std::error::Error + Send + Sync>>);

impl std::fmt::Display for ReplyLost {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("the reply did not arrive")
    }
}

impl std::error::Error for ReplyLost {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0
            .as_deref()
            .map(|cause| cause as &(dyn std::error::Error + 'static))
    }
}

/// `failure` to read a reply, as one that never arrived whole: of unknown
/// outcome, with [`ReplyLost`] for its cause.
fn lost(mut failure: Error) -> Error {
    let cause = failure.source.take();
    failure.uncertain().with_source(ReplyLost(cause))
}

/// Whether `failure` is of a reply that never arrived whole, rather than
/// one that arrived and was wrong.
pub fn reply_lost(failure: &Error) -> bool {
    failure
        .source
        .as_deref()
        .is_some_and(|cause| cause.is::<ReplyLost>())
}

/// Whether `row` is the final reply, the one that says `ok`. Its flags must
/// be booleans: a caller decides uncertainty by them.
pub(crate) fn final_row(row: &Map<String, Value>) -> Result<bool> {
    let Some(ok) = row.get("ok") else {
        return Ok(false);
    };
    if !ok.is_boolean() {
        return Err(Error::new(Code::Protocol, "Invalid success flag"));
    }
    if row.get("uncertain").is_some_and(|flag| !flag.is_boolean()) {
        return Err(Error::new(Code::Protocol, "Invalid uncertainty flag"));
    }
    Ok(true)
}

/// A reply the helper accepted, or its refusal. A successful reply does not
/// imply a successful child exit; callers inspect that result separately.
///
/// # Errors
///
/// Returns the helper's refusal.
pub fn accepted(reply: Value) -> Result<Value> {
    if reply["ok"] == true {
        Ok(reply)
    } else {
        Err(refusal(&reply))
    }
}

/// A helper's refusal as an error: its code when it is one of Atlas's, its
/// words, and whether what it refused may have happened anyway.
pub fn refusal(reply: &Value) -> Error {
    Error::refused(
        Code::of_reply(reply, Code::Daemon),
        reply,
        "The helper refused the request",
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputError {
    Malformed,
    Oversized,
}

impl std::fmt::Display for OutputError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Malformed => "malformed command output",
            Self::Oversized => "oversized command output",
        })
    }
}

impl std::error::Error for OutputError {}

/// Concatenate the base64 `output` events of a helper `run` reply. An event
/// that must decode past `limit` is refused unread; a decoded total over
/// `limit` (by at most a padding remainder) is refused after decoding.
///
/// # Errors
///
/// Returns an [`OutputError`] for a malformed event or output past `limit`.
pub fn run_output(reply: &Value, limit: usize) -> std::result::Result<Vec<u8>, OutputError> {
    let mut output = Vec::new();
    for event in events(reply, "output") {
        append_event(event, &mut output, limit)?;
    }
    Ok(output)
}

fn events<'a>(reply: &'a Value, name: &'static str) -> impl Iterator<Item = &'a Value> {
    reply["events"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(move |event| event["event"] == name)
}

/// Append the base64 `data` of one `event` to `output`, refusing it unread
/// when it must decode past `limit` in all, and after decoding when it
/// did. Binary data is never decoded as text.
///
/// # Errors
///
/// Returns an [`OutputError`] for a malformed event or output past `limit`.
pub fn append_event(
    event: &Value,
    output: &mut Vec<u8>,
    limit: usize,
) -> std::result::Result<(), OutputError> {
    let encoded = event["data"].as_str().ok_or(OutputError::Malformed)?;
    if output.len() + (encoded.len() / 4 * 3).saturating_sub(2) > limit {
        return Err(OutputError::Oversized);
    }

    STANDARD
        .decode_vec(encoded, output)
        .map_err(|_| OutputError::Malformed)?;

    if output.len() > limit {
        return Err(OutputError::Oversized);
    }
    Ok(())
}

/// The most output `with_output` keeps as text.
const OUTPUT_LIMIT: usize = 1 << 20;

/// The first `limit` bytes of a reply's output, read event by event; None
/// when an event before the limit does not decode.
fn output_prefix(reply: &Value, limit: usize) -> Option<Vec<u8>> {
    let mut output = Vec::new();

    for event in events(reply, "output") {
        let decoded = STANDARD.decode(event["data"].as_str()?).ok()?;
        let room = limit.saturating_sub(output.len());
        output.extend_from_slice(&decoded[..decoded.len().min(room)]);

        if output.len() >= limit {
            break;
        }
    }

    Some(output)
}

/// A helper `run` reply with its output decoded into an `output` string
/// (lossy UTF-8) and the command's `pid` in place of the events they came
/// in, so callers need not decode them and never read them twice. Output
/// past 1 MiB is cut and said to be (`outputTruncated`). Events
/// stay only when they could not be decoded.
pub fn with_output(mut reply: Value) -> Value {
    let (output, cut) = match run_output(&reply, OUTPUT_LIMIT) {
        Ok(output) => (output, false),
        Err(OutputError::Oversized) => match output_prefix(&reply, OUTPUT_LIMIT) {
            Some(prefix) => (prefix, true),
            None => return reply,
        },
        Err(OutputError::Malformed) => return reply,
    };

    let pid = events(&reply, "started")
        .next()
        .and_then(|event| event["pid"].as_i64());

    reply["output"] = json!(String::from_utf8_lossy(&output));

    if cut {
        reply["outputTruncated"] = json!(true);
    }

    if let Some(pid) = pid {
        reply["pid"] = json!(pid);
    }

    if let Some(object) = reply.as_object_mut() {
        object.remove("events");
    }

    reply
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
