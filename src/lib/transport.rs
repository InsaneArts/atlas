//! One request to a helper-side socket and its reply: a JSON line in, event
//! lines and a final reply out, within one deadline, from a peer the kernel
//! says is the helper account.
use crate::error::{Error, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    time::{Instant, timeout_at},
};

/// Longest request line, without its newline: what the helper daemon reads.
const MAX_REQUEST: usize = crate::daemon::protocol::MAX_REQUEST;
/// Longest the request itself may take to send. A peer that is not reading
/// leaves the outcome unknown, so this stays short.
const DISPATCH: Duration = Duration::from_millis(500);
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
        let stream = timeout_at(deadline, UnixStream::connect(&self.path))
            .await
            .map_err(|_| Error::new("transport", "Connection timed out"))?
            .map_err(refused)?;
        if stream.peer_cred()?.uid() != self.peer_uid {
            return Err(Error::new(
                "identity",
                format!(
                    "{} is not served by the configured helper account",
                    self.path.display()
                ),
            ));
        }
        Ok(Pending {
            stream,
            deadline,
            started,
        })
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
fn refused(error: std::io::Error) -> Error {
    if error.raw_os_error() == Some(libc::EPERM) {
        return Error::new(
            "blocked",
            "This shell may not connect to the helper (operation not permitted). An agent's \
             sandbox blocks local sockets: run atlas outside the sandbox. The helper itself \
             may be fine.",
        );
    }
    Error::from(error)
}

impl Pending {
    pub async fn dispatch(mut self, request: &Value) -> Result<Self> {
        let mut payload = serde_json::to_vec(request)?;
        if payload.len() > MAX_REQUEST {
            return Err(Error::new("protocol", "Request exceeds 1 MiB"));
        }
        payload.push(b'\n');
        let deadline = self.deadline.min(Instant::now() + DISPATCH);
        timeout_at(deadline, self.stream.write_all(&payload))
            .await
            .map_err(|_| Error::new("transport", "Dispatch timed out").uncertain())?
            .map_err(|e| Error::new("transport", e.to_string()).uncertain())?;
        Ok(self)
    }

    pub async fn finish(mut self) -> Result<Value> {
        let deadline = self.deadline;
        timeout_at(deadline, self.receive())
            .await
            .map_err(|_| {
                Error::new("transport", "Helper exceeded the request deadline").uncertain()
            })?
            .map_err(Error::uncertain)
    }

    async fn receive(&mut self) -> Result<Value> {
        let mut pending = Vec::new();
        let mut total = 0;
        let mut events = Vec::new();
        // Keep the receive storage out of every enclosing async future. A stack
        // array multiplies through the HTTP/session futures in debug builds.
        let mut buffer = vec![0; 65536];
        loop {
            let count = self.stream.read(&mut buffer).await?;
            if count == 0 {
                return Err(Error::new(
                    "protocol",
                    "Connection ended without a final reply",
                ));
            }
            total += count;
            if total > MAX_REPLY {
                return Err(Error::new("protocol", "Reply exceeds 64 MiB"));
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
                    .ok_or_else(|| Error::new("protocol", "Reply must be a JSON object"))?;
                if let Some(ok) = object.get("ok") {
                    if !ok.is_boolean() {
                        return Err(Error::new("protocol", "Invalid success flag"));
                    }
                    if !events.is_empty() {
                        object.insert("events".into(), json!(events));
                    }
                    object.insert(
                        "elapsedMs".into(),
                        json!(self.started.elapsed().as_secs_f64() * 1000.0),
                    );
                    return Ok(row);
                }
                if object.get("event").and_then(Value::as_str).is_none()
                    || events.len() >= MAX_REPLY_EVENTS
                {
                    return Err(Error::new(
                        "protocol",
                        "Malformed or excessive helper events",
                    ));
                }
                events.push(row);
            }
            pending.extend_from_slice(&buffer[offset..count]);
        }
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
    let mut output = Vec::new();
    for event in reply["events"].as_array().into_iter().flatten() {
        if event["event"] != "output" {
            continue;
        }
        let encoded = event["data"].as_str().ok_or(OutputError::Malformed)?;
        if output.len() + (encoded.len() / 4 * 3).saturating_sub(2) > limit {
            return Err(OutputError::Oversized);
        }
        STANDARD
            .decode_vec(encoded, &mut output)
            .map_err(|_| OutputError::Malformed)?;
        if output.len() > limit {
            return Err(OutputError::Oversized);
        }
    }
    Ok(output)
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
    use super::{OUTPUT_LIMIT, OutputError, refused, run_output, with_output};
    use base64::{Engine, engine::general_purpose::STANDARD};
    use serde_json::json;

    /// The client refuses exactly the requests the helper daemon would: a
    /// line longer than its limit, not counting the newline.
    #[tokio::test]
    async fn the_request_limit_is_the_daemons() {
        use super::{MAX_REQUEST, Transport};
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("t.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut sink = Vec::new();
                    let _ = tokio::io::AsyncReadExt::read_to_end(&mut stream, &mut sink).await;
                });
            }
        });
        let transport = Transport {
            path: socket,
            timeout: std::time::Duration::from_secs(10),
            peer_uid: nix::unistd::getuid().as_raw(),
        };
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
            ("protocol", "Request exceeds 1 MiB")
        );
        assert!(!refused.uncertain, "nothing was sent");
    }

    /// Only a connection policy forbids is `blocked`; a missing socket or a
    /// permission on its folder stays what it is.
    #[test]
    fn a_forbidden_connection_is_blocked() {
        let error = |code| refused(std::io::Error::from_raw_os_error(code)).code;
        assert_eq!(error(libc::EPERM), "blocked");
        assert_eq!(error(libc::EACCES), "io");
        assert_eq!(error(libc::ENOENT), "io");
        assert_eq!(error(libc::ECONNREFUSED), "io");
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
}
