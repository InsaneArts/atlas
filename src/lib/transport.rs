use crate::error::{Error, Result};
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
