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
            .map_err(|_| Error::new("transport", "Connection timed out"))??;
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

impl Pending {
    pub async fn dispatch(mut self, request: &Value) -> Result<Self> {
        let mut payload = serde_json::to_vec(request)?;
        payload.push(b'\n');
        if payload.len() > 1024 * 1024 {
            return Err(Error::new("protocol", "Request exceeds 1 MiB"));
        }
        let deadline = self
            .deadline
            .min(Instant::now() + Duration::from_millis(500));
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
            if total > 64 * 1024 * 1024 {
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
                if object.get("event").and_then(Value::as_str).is_none() || events.len() >= 1024 {
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

/// A helper `run` reply with its output decoded into an `output` string (lossy
/// UTF-8, at most 1 MiB), so callers need not decode base64 events.
pub fn with_output(mut reply: Value) -> Value {
    if let Ok(output) = run_output(&reply, 1 << 20) {
        reply["output"] = json!(String::from_utf8_lossy(&output));
    }
    reply
}

#[cfg(test)]
mod tests {
    use super::{OutputError, run_output, with_output};
    use serde_json::json;

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
        assert_eq!(with_output(reply.clone())["output"], "hello");
        assert!(
            with_output(json!({"events": [{"event": "output", "data": 7}]}))
                .get("output")
                .is_none()
        );
    }
}
