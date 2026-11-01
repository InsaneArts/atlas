//! One request to a helper-side socket and its reply: a JSON line in, event
//! lines and a final reply out, within one deadline, from a peer the kernel
//! says is the helper account.
use crate::error::{Code, Error, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    future::Future,
    io,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    time::{Instant, sleep, timeout_at},
};

/// Longest request line, without its newline: what the helper daemon reads.
const MAX_REQUEST: usize = crate::daemon::protocol::MAX_REQUEST;
/// Longest the request itself may take to send. Both helper-side servers act
/// only on a whole line, so a request cut short changed nothing.
const DISPATCH: Duration = Duration::from_millis(500);
/// How long a failed send looks for a refusal the peer already sent.
const WAITING: Duration = Duration::from_millis(100);
const IDENTITY_RETRY: Duration = Duration::from_millis(10);
/// Longest reply, every line together.
const MAX_REPLY: usize = 64 * 1024 * 1024;
/// Most event lines before the final reply.
pub const MAX_REPLY_EVENTS: usize = 1024;

#[derive(Clone)]
pub struct Transport {
    pub path: PathBuf,
    pub timeout: Duration,
    /// Only this account may serve the socket. Endpoints live in shared
    /// directories, so the kernel-reported peer, not the reply, proves identity.
    pub peer_uid: u32,
}

pub struct Pending {
    stream: UnixStream,
    deadline: Instant,
    started: Instant,
}

impl Transport {
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
            let stream = timeout_at(deadline, connect())
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

    pub async fn call(&self, request: &Value) -> Result<Value> {
        self.connect()
            .await?
            .dispatch(request)
            .await?
            .finish()
            .await
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

impl Pending {
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
            Ok(Err(error)) => error.to_string(),
            Err(_) => "The helper did not take the request in time".to_owned(),
        };

        Err(self
            .waiting_refusal()
            .await
            .unwrap_or_else(|| Error::new(Code::Transport, failed)))
    }

    /// A definite refusal the peer sent before it hung up, if one is waiting.
    async fn waiting_refusal(&mut self) -> Option<Error> {
        let deadline = self.deadline.min(Instant::now() + WAITING);
        let reply = timeout_at(deadline, self.receive()).await.ok()?.ok()?;

        (reply["ok"] == false && reply["uncertain"] != true).then(|| refusal(&reply))
    }

    pub async fn finish(mut self) -> Result<Value> {
        let deadline = self.deadline;
        timeout_at(deadline, self.receive())
            .await
            .map_err(|_| {
                Error::new(Code::Transport, "Helper exceeded the request deadline").uncertain()
            })?
            .map_err(Error::uncertain)
    }

    /// The final reply, each event line before it handed to `each` as it
    /// arrives rather than kept, so a long reply is never held whole. A
    /// failure of `each` ends the reply there.
    pub async fn finish_each(
        mut self,
        each: &mut (dyn FnMut(Value) -> Result<()> + Send),
    ) -> Result<Value> {
        let deadline = self.deadline;
        timeout_at(deadline, self.receive_each(each))
            .await
            .map_err(|_| {
                Error::new(Code::Transport, "Helper exceeded the request deadline").uncertain()
            })?
            .map_err(Error::uncertain)
    }

    async fn receive(&mut self) -> Result<Value> {
        let mut events = Vec::new();
        let mut reply = self
            .receive_each(&mut |event| {
                events.push(event);
                Ok(())
            })
            .await?;
        if !events.is_empty() {
            reply["events"] = json!(events);
        }
        Ok(reply)
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
            let count = self.stream.read(&mut buffer).await?;
            if count == 0 {
                return Err(Error::new(
                    Code::Protocol,
                    "Connection ended without a final reply",
                ));
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
                if let Some(ok) = object.get("ok") {
                    if !ok.is_boolean() {
                        return Err(Error::new(Code::Protocol, "Invalid success flag"));
                    }

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

/// A reply the helper accepted, or its refusal. A successful reply does not
/// imply a successful child exit; callers inspect that result separately.
pub(crate) fn accepted(reply: Value) -> Result<Value> {
    if reply["ok"] == true {
        Ok(reply)
    } else {
        Err(refusal(&reply))
    }
}

/// A helper's refusal as an error: its code when it is one of Atlas's, its
/// words, and whether what it refused may have happened anyway.
pub(crate) fn refusal(reply: &Value) -> Error {
    let error = Error::new(
        reply["code"]
            .as_str()
            .and_then(crate::error::Code::parse)
            .unwrap_or(Code::Daemon),
        reply["error"]
            .as_str()
            .unwrap_or("The helper refused the request"),
    );

    if reply["uncertain"] == true {
        error.uncertain()
    } else {
        error
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum OutputError {
    Malformed,
    Oversized,
}

/// Concatenate the base64 `output` events of a helper `run` reply. An event
/// that must decode past `limit` is refused unread; a decoded total over
/// `limit` (by at most a padding remainder) is refused after decoding.
pub fn run_output(reply: &Value, limit: usize) -> std::result::Result<Vec<u8>, OutputError> {
    event_bytes(reply, "output", limit)
}

/// Concatenate the base64 data of a reply's events named `name`, exactly
/// as the helper sent them, within `limit` as [`run_output`] keeps it.
fn event_bytes(
    reply: &Value,
    name: &str,
    limit: usize,
) -> std::result::Result<Vec<u8>, OutputError> {
    let mut output = Vec::new();

    for event in reply["events"].as_array().into_iter().flatten() {
        if event["event"] != name {
            continue;
        }
        append_event(event, &mut output, limit)?;
    }

    Ok(output)
}

/// Append the base64 `data` of one `event` to `output`, refusing it unread
/// when it must decode past `limit` in all, and after decoding when it
/// did. Binary data is never decoded as text.
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
pub const OUTPUT_LIMIT: usize = 1 << 20;

/// The first `limit` bytes of a reply's output, read event by event; None
/// when an event before the limit does not decode.
fn output_prefix(reply: &Value, limit: usize) -> Option<Vec<u8>> {
    let mut output = Vec::new();

    for event in reply["events"].as_array().into_iter().flatten() {
        if event["event"] != "output" {
            continue;
        }

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
/// past [`OUTPUT_LIMIT`] is cut and said to be (`outputTruncated`). Events
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

    let pid = reply["events"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|event| event["event"] == "started")
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
mod tests {
    use super::{OUTPUT_LIMIT, OutputError, Transport, accepted, refused, run_output, with_output};
    use crate::error::{Code, Error};
    use base64::{Engine, engine::general_purpose::STANDARD};
    use serde_json::json;
    use std::{cell::Cell, error::Error as _, io, sync::Arc, time::Duration};
    use tokio::time::{Instant, timeout};

    fn endpoint() -> Transport {
        Transport {
            path: "/tmp/atlas-test.sock".into(),
            timeout: Duration::from_millis(25),
            peer_uid: 502,
        }
    }

    fn io_cause(error: &Error) -> &io::Error {
        error.source().unwrap().downcast_ref().unwrap()
    }

    #[tokio::test(start_paused = true)]
    async fn a_transient_identity_failure_tries_a_fresh_connection_before_returning_it() {
        let transport = endpoint();
        let started = Instant::now();
        let attempts = Cell::new(0);
        let mut previous = std::sync::Weak::new();
        let verified = timeout(
            Duration::from_secs(1),
            transport.connect_verified(
                started + transport.timeout,
                || {
                    assert!(previous.upgrade().is_none(), "the failed peer was dropped");
                    attempts.set(attempts.get() + 1);
                    let connection = Arc::new(attempts.get());
                    previous = Arc::downgrade(&connection);
                    std::future::ready(Ok(connection))
                },
                |connection| match **connection {
                    1 => Err(io::Error::from_raw_os_error(libc::ENOTCONN)),
                    _ => Ok(transport.peer_uid),
                },
            ),
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(*verified, 2);
        assert_eq!(attempts.get(), 2);
        assert_eq!(started.elapsed(), super::IDENTITY_RETRY);
    }

    #[tokio::test(start_paused = true)]
    async fn a_wrong_peer_account_is_refused_without_a_retry() {
        let transport = endpoint();
        let started = Instant::now();
        let attempts = Cell::new(0);
        let error = timeout(
            Duration::from_secs(1),
            transport.connect_verified(
                started + transport.timeout,
                || {
                    attempts.set(attempts.get() + 1);
                    std::future::ready(Ok(()))
                },
                |_| Ok(transport.peer_uid + 1),
            ),
        )
        .await
        .unwrap()
        .unwrap_err();

        assert_eq!(attempts.get(), 1);
        assert_eq!(started.elapsed(), Duration::ZERO);
        assert_eq!(error.code, Code::Identity);
        assert!(!error.uncertain);
        assert!(error.error.contains("verify the peer account"));
        assert!(error.error.contains(transport.path.to_str().unwrap()));
    }

    #[tokio::test(start_paused = true)]
    async fn other_identity_failures_keep_their_context_and_cause_without_retrying() {
        let transport = endpoint();
        for errno in [libc::EACCES, libc::EPERM, libc::EINVAL] {
            let attempts = Cell::new(0);
            let error = timeout(
                Duration::from_secs(1),
                transport.connect_verified(
                    Instant::now() + transport.timeout,
                    || {
                        attempts.set(attempts.get() + 1);
                        std::future::ready(Ok(()))
                    },
                    |_| Err(io::Error::from_raw_os_error(errno)),
                ),
            )
            .await
            .unwrap()
            .unwrap_err();

            assert_eq!(attempts.get(), 1);
            assert_eq!(error.code, Code::Io);
            assert!(!error.uncertain);
            assert!(error.error.contains("read the peer account"));
            assert!(error.error.contains(transport.path.to_str().unwrap()));
            assert_eq!(io_cause(&error).raw_os_error(), Some(errno));
            let wire = serde_json::to_value(&error).unwrap();
            assert_eq!(wire.as_object().unwrap().len(), 4);
            assert!(wire.get("source").is_none());
        }
    }

    #[tokio::test(start_paused = true)]
    async fn connection_failures_are_never_retried_as_identity_failures() {
        let transport = endpoint();
        for errno in [libc::ENOTCONN, libc::EACCES, libc::ECONNREFUSED] {
            let attempts = Cell::new(0);
            let error = timeout(
                Duration::from_secs(1),
                transport.connect_verified(
                    Instant::now() + transport.timeout,
                    || {
                        attempts.set(attempts.get() + 1);
                        std::future::ready(Err::<(), _>(io::Error::from_raw_os_error(errno)))
                    },
                    |_| panic!("a failed connection has no peer to verify"),
                ),
            )
            .await
            .unwrap()
            .unwrap_err();

            assert_eq!(attempts.get(), 1);
            assert_eq!(error.code, Code::Io);
            assert!(!error.uncertain);
            assert!(error.error.contains("connect to socket"));
            assert!(error.error.contains(transport.path.to_str().unwrap()));
            assert_eq!(io_cause(&error).raw_os_error(), Some(errno));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn repeated_identity_failures_stop_at_the_original_deadline_with_the_cause() {
        let transport = endpoint();
        let started = Instant::now();
        let attempts = Cell::new(0);
        let error = timeout(
            Duration::from_secs(1),
            transport.connect_verified(
                started + transport.timeout,
                || {
                    attempts.set(attempts.get() + 1);
                    std::future::ready(Ok(()))
                },
                |_| Err(io::Error::from_raw_os_error(libc::ENOTCONN)),
            ),
        )
        .await
        .unwrap()
        .unwrap_err();

        assert_eq!(attempts.get(), 3);
        assert_eq!(started.elapsed(), transport.timeout);
        assert_eq!(error.code, Code::Transport);
        assert!(!error.uncertain);
        assert!(error.error.contains("Timed out reading the peer account"));
        assert!(error.error.contains(transport.path.to_str().unwrap()));
        assert_eq!(io_cause(&error).raw_os_error(), Some(libc::ENOTCONN));
    }

    #[tokio::test(start_paused = true)]
    async fn a_stalled_fresh_connection_uses_only_the_remaining_deadline() {
        let transport = endpoint();
        let started = Instant::now();
        let attempts = Cell::new(0);
        let error = timeout(
            Duration::from_secs(1),
            transport.connect_verified(
                started + transport.timeout,
                || {
                    attempts.set(attempts.get() + 1);
                    let attempt = attempts.get();
                    async move {
                        if attempt > 1 {
                            std::future::pending::<()>().await;
                        }
                        Ok(())
                    }
                },
                |_| Err(io::Error::from_raw_os_error(libc::ENOTCONN)),
            ),
        )
        .await
        .unwrap()
        .unwrap_err();

        assert_eq!(attempts.get(), 2);
        assert_eq!(started.elapsed(), transport.timeout);
        assert_eq!(error.code, Code::Transport);
        assert!(!error.uncertain);
        assert!(error.error.contains("Timed out connecting to socket"));
        assert!(error.error.contains(transport.path.to_str().unwrap()));
        assert!(error.source().unwrap().is::<tokio::time::error::Elapsed>());
    }

    /// A listener on a socket in a new folder (kept while the folder is), and
    /// a transport to it.
    fn listening() -> (tempfile::TempDir, tokio::net::UnixListener, Transport) {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("t.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        let transport = Transport {
            path: socket,
            timeout: std::time::Duration::from_secs(10),
            peer_uid: nix::unistd::getuid().as_raw(),
        };

        (dir, listener, transport)
    }

    /// The client refuses exactly the requests the helper daemon would: a
    /// line longer than its limit, not counting the newline.
    #[tokio::test]
    async fn the_request_limit_is_the_daemons() {
        use super::MAX_REQUEST;

        let (_dir, listener, transport) = listening();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut sink = Vec::new();
                    let _ = tokio::io::AsyncReadExt::read_to_end(&mut stream, &mut sink).await;
                });
            }
        });

        // A JSON string is its characters plus two quotes.
        let request = |length: usize| json!("x".repeat(length - 2));
        let fits = transport.connect().await.unwrap();
        assert!(fits.dispatch(&request(MAX_REQUEST)).await.is_ok());

        let over = transport.connect().await.unwrap();
        let refused = over
            .dispatch(&request(MAX_REQUEST + 1))
            .await
            .err()
            .unwrap();

        assert_eq!(
            (refused.code, refused.error.as_str()),
            (Code::Protocol, "Request exceeds 1 MiB")
        );
        assert!(!refused.uncertain, "nothing was sent");
    }

    /// A peer that refused and hung up before the request was sent is
    /// heard: nothing ran, so the outcome is known.
    #[tokio::test]
    async fn a_refusal_sent_before_the_request_is_heard() {
        let (_dir, listener, transport) = listening();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                crate::native::endpoint::turn_away(
                    stream,
                    &crate::daemon::protocol::failure(Code::Busy, "Too many requests"),
                );
            }
        });

        let pending = transport.connect().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let refused = pending
            .dispatch(&json!({"op": "run", "pad": "x".repeat(512 * 1024)}))
            .await
            .err()
            .unwrap();

        assert_eq!(
            (refused.code, refused.error.as_str()),
            (Code::Busy, "Too many requests")
        );
        assert!(!refused.uncertain, "nothing ran");
    }

    /// A request the peer never reads whole was never acted on: both servers
    /// wait for its newline, which is the last byte sent.
    #[tokio::test]
    async fn a_request_never_sent_whole_is_certain() {
        use super::DISPATCH;

        let (_dir, listener, transport) = listening();
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((stream, _)) = listener.accept().await {
                held.push(stream);
            }
        });

        let started = std::time::Instant::now();
        let stalled = transport
            .connect()
            .await
            .unwrap()
            .dispatch(&json!({"op": "run", "pad": "x".repeat(1000 * 1024)}))
            .await
            .err()
            .unwrap();

        assert!(started.elapsed() < DISPATCH * 4);
        assert_eq!(stalled.code, "transport");
        assert!(!stalled.uncertain, "the newline was never sent");
    }

    /// A peer that reads part of the request and hangs up without a word
    /// never saw its newline either.
    #[tokio::test]
    async fn a_peer_that_hangs_up_mid_request_changed_nothing() {
        use tokio::io::AsyncReadExt;

        let (_dir, listener, transport) = listening();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut start = [0; 16];
                let _ = stream.read_exact(&mut start).await;
                drop(stream);
            }
        });

        let cut = transport
            .connect()
            .await
            .unwrap()
            .dispatch(&json!({"op": "run", "pad": "x".repeat(1000 * 1024)}))
            .await
            .err()
            .unwrap();

        assert_eq!(cut.code, "transport");
        assert!(!cut.uncertain, "the newline was never sent");
    }

    /// A refusal keeps the helper's code when Atlas knows it, and says
    /// whether what was refused may have happened.
    #[test]
    fn a_refusal_keeps_its_code_and_doubt() {
        let refused = super::refusal(
            &json!({"ok": false, "code": "bad-request", "error": "no", "uncertain": false}),
        );
        assert_eq!((refused.code, refused.uncertain), (Code::BadRequest, false));

        let unknown = super::refusal(
            &json!({"ok": false, "code": "something-new", "error": "?", "uncertain": true}),
        );
        assert_eq!((unknown.code, unknown.uncertain), (Code::Daemon, true));
    }

    #[test]
    fn a_command_the_helper_stopped_part_way_stays_uncertain_with_its_code() {
        for code in ["on-console", "restart", "transport"] {
            let reply = json!({
                "ok": false,
                "code": code,
                "error": "stopped",
                "uncertain": true,
            });

            let error = accepted(reply).unwrap_err();
            assert_eq!(error.code, code);
            assert!(error.uncertain, "{code}");
        }
    }

    #[test]
    fn a_command_the_helper_never_started_is_a_certain_refusal() {
        let reply = json!({"ok": false, "code": "spawn-failed", "error": "no such file"});

        let error = accepted(reply).unwrap_err();
        assert_eq!(error.code, "spawn-failed");
        assert!(!error.uncertain);
    }

    #[test]
    fn a_command_that_ran_to_its_end_is_read_for_its_exit() {
        let reply = json!({"ok": true, "exitCode": 1, "timedOut": false});

        assert_eq!(accepted(reply.clone()).unwrap(), reply);
    }

    #[test]
    fn a_refused_app_listing_never_reads_as_no_apps() {
        let refused = accepted(json!({"ok": false, "code": "busy", "error": "Busy"}));
        assert_eq!(refused.map_err(|error| error.code).unwrap_err(), "busy");
        assert!(accepted(json!({"ok": true, "apps": []})).is_ok());
    }

    #[test]
    fn only_explicit_success_is_accepted_and_unknown_refusals_keep_their_details() {
        for reply in [
            json!(null),
            json!({}),
            json!({"ok": null}),
            json!({"ok": 0}),
            json!({"ok": "true"}),
            json!({"ok": []}),
            json!({"ok": {}}),
        ] {
            let error = accepted(reply).unwrap_err();
            assert_eq!(error.code, "daemon");
            assert_eq!(error.error, "The helper refused the request");
            assert!(!error.uncertain);
        }

        let error = accepted(json!({
            "ok": false,
            "code": "future-code",
            "error": "The helper stopped",
            "uncertain": true,
        }))
        .unwrap_err();
        assert_eq!(error.code, "daemon");
        assert_eq!(error.error, "The helper stopped");
        assert!(error.uncertain);
    }

    /// Only a connection policy forbids is `blocked`; a missing socket or a
    /// permission on its folder stays what it is.
    #[test]
    fn a_forbidden_connection_is_blocked() {
        let transport = endpoint();
        for (errno, code) in [
            (libc::EPERM, Code::Blocked),
            (libc::EACCES, Code::Io),
            (libc::ENOENT, Code::Io),
            (libc::ECONNREFUSED, Code::Io),
        ] {
            let error = refused(&transport.path, io::Error::from_raw_os_error(errno));
            assert_eq!(error.code, code);
            assert!(!error.uncertain);
            assert!(error.error.contains("connect"));
            assert!(error.error.contains(transport.path.to_str().unwrap()));
            assert_eq!(io_cause(&error).raw_os_error(), Some(errno));
        }
    }

    #[test]
    fn run_output_joins_only_output_events_within_the_limit() {
        let reply = json!({"events": [
            {"event": "started", "pid": 4},
            {"event": "output", "data": "aGVs"},
            {"event": "output", "data": "bG8="},
        ]});
        assert_eq!(run_output(&reply, 5).unwrap(), b"hello");
        assert_eq!(run_output(&reply, 4), Err(OutputError::Oversized));
        assert_eq!(run_output(&json!({}), 0).unwrap(), b"");

        for data in [json!(7), json!("not base64!")] {
            let reply = json!({"events": [{"event": "output", "data": data}]});
            assert_eq!(run_output(&reply, 64), Err(OutputError::Malformed));
        }

        let huge = json!({"events": [{"event": "output", "data": "A".repeat(4096)}]});
        assert_eq!(run_output(&huge, 16), Err(OutputError::Oversized));

        let decoded = with_output(reply.clone());
        assert_eq!(decoded["output"], "hello");
        assert_eq!(decoded["pid"], 4, "the process is still named");
        assert!(
            decoded.get("events").is_none(),
            "decoded events are not repeated"
        );

        let undecodable = with_output(json!({"events": [{"event": "output", "data": 7}]}));
        assert!(undecodable.get("output").is_none());
        assert!(
            undecodable.get("events").is_some(),
            "what could not be decoded stays"
        );

        let chunk = STANDARD.encode("x".repeat(600_000));
        let long = with_output(json!({"events": [
            {"event": "started", "pid": 9},
            {"event": "output", "data": chunk},
            {"event": "output", "data": chunk},
        ]}));
        assert_eq!(long["output"].as_str().unwrap().len(), OUTPUT_LIMIT);
        assert_eq!(long["outputTruncated"], true, "the cut is said");
        assert_eq!(long["pid"], 9);
        assert!(long.get("events").is_none());
    }

    #[test]
    fn binary_event_data_is_returned_exactly() {
        let bytes: Vec<u8> = (0..=255).chain([0, 0xff, 0xfe, 0xc0]).collect();
        let half = bytes.len() / 2;
        let reply = json!({"ok": true, "events": [
            {"event": "data", "data": STANDARD.encode(&bytes[..half])},
            {"event": "output", "data": STANDARD.encode(b"not this")},
            {"event": "data", "data": STANDARD.encode(&bytes[half..])},
        ]});
        assert_eq!(super::event_bytes(&reply, "data", 1024).unwrap(), bytes);
        assert_eq!(
            super::event_bytes(&reply, "data", 10),
            Err(OutputError::Oversized)
        );
        assert_eq!(run_output(&reply, 1024).unwrap(), b"not this");
    }

    /// Each event is handed on as it arrives and never kept, so a long
    /// reply is never held whole; the final reply carries none of them.
    #[tokio::test]
    async fn events_are_handed_on_as_they_arrive() {
        use tokio::io::AsyncWriteExt;

        let (_dir, listener, transport) = listening();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut line = Vec::new();
            loop {
                let mut byte = [0_u8; 1];
                tokio::io::AsyncReadExt::read_exact(&mut stream, &mut byte)
                    .await
                    .unwrap();
                if byte[0] == b'\n' {
                    break;
                }
                line.push(byte[0]);
            }
            for n in 0..3 {
                let event = json!({"event": "data", "data": STANDARD.encode([n])});
                stream
                    .write_all(format!("{event}\n").as_bytes())
                    .await
                    .unwrap();
            }
            stream.write_all(b"{\"ok\":true}\n").await.unwrap();
        });

        let mut seen = Vec::new();
        let reply = transport
            .connect()
            .await
            .unwrap()
            .dispatch(&json!({"op": "read"}))
            .await
            .unwrap()
            .finish_each(&mut |event| {
                super::append_event(&event, &mut seen, 16).unwrap();
                Ok(())
            })
            .await
            .unwrap();
        assert_eq!(seen, [0, 1, 2]);
        assert_eq!(reply["ok"], true);
        assert!(reply.get("events").is_none());
    }
}
