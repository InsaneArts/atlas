//! Fresh session facts from one independent child, requested by nonce over
//! private pipes. No proof is cached, prefetched, or shared between requests.
use atlas_core::{
    error::{Code, Error, Result},
    protocol::valid_challenge,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, Read, Write},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
};

/// The longest request or reply line, in bytes.
const LINE: usize = 4096;
/// How long one exchange, or the wait for the worker, may take.
const DEADLINE: Duration = Duration::from_secs(2);

fn invalid() -> Error {
    Error::new(
        Code::Identity,
        "The independent session proof worker failed",
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Challenge {
    challenge: String,
}

/// The hidden worker command. Every complete input line triggers a new native
/// query in this process, whose real parent remains the capture daemon.
///
/// # Errors
///
/// Returns an error for a malformed or oversized request line, a failed
/// session query, or a closed output pipe.
pub fn serve_blocking() -> Result<()> {
    serve(
        io::stdin().lock(),
        io::stdout().lock(),
        atlas_native::native::session_info,
    )
}

fn serve(
    mut input: impl BufRead,
    mut output: impl Write,
    mut proof: impl FnMut() -> Result<Value>,
) -> Result<()> {
    loop {
        let mut line = Vec::new();
        if Read::take(&mut input, LINE as u64 + 1).read_until(b'\n', &mut line)? == 0 {
            return Ok(());
        }
        if line.len() > LINE || line.last() != Some(&b'\n') {
            return Err(invalid());
        }
        let request: Challenge = serde_json::from_slice(&line).map_err(|_| invalid())?;
        if !valid_challenge(&request.challenge) {
            return Err(invalid());
        }
        let value = proof()?;
        let mut reply =
            serde_json::to_vec(&json!({"ok":true,"challenge":request.challenge,"proof":value}))?;
        if reply.len() >= LINE {
            return Err(invalid());
        }
        reply.push(b'\n');
        output.write_all(&reply)?;
        output.flush()?;
    }
}

/// The daemon's one proof child and its pipes.
pub(super) struct Worker {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl Worker {
    fn spawn(command: &mut Command) -> Result<Self> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let input = child.stdin.take().ok_or_else(invalid)?;
        let output = BufReader::new(child.stdout.take().ok_or_else(invalid)?);
        Ok(Self {
            child,
            input,
            output,
        })
    }

    async fn exchange(&mut self, challenge: &str) -> Result<Value> {
        if !valid_challenge(challenge) || self.child.try_wait()?.is_some() {
            return Err(invalid());
        }
        let mut request = serde_json::to_vec(&json!({"challenge":challenge}))?;
        request.push(b'\n');
        self.input.write_all(&request).await?;
        self.input.flush().await?;
        let mut line = Vec::new();
        (&mut self.output)
            .take(LINE as u64 + 1)
            .read_until(b'\n', &mut line)
            .await?;
        if line.len() > LINE || line.last() != Some(&b'\n') {
            return Err(invalid());
        }
        let reply: Value = serde_json::from_slice(&line).map_err(|_| invalid())?;
        if reply["ok"] != true
            || reply["challenge"].as_str() != Some(challenge)
            || !reply["proof"].is_object()
        {
            return Err(invalid());
        }
        Ok(reply)
    }

    async fn stop(mut self) {
        // kill() also waits for the child. On cancellation or an unlikely
        // reap timeout, kill_on_drop still terminates it and Tokio reaps it.
        let _ = tokio::time::timeout(DEADLINE, self.child.kill()).await;
    }
}

/// A fresh proof for `challenge` from the worker kept in `state`, started
/// when there is none. A failed or late exchange ends that worker.
pub(super) async fn query(state: &Mutex<Option<Worker>>, challenge: &str) -> Result<Value> {
    let mut state = tokio::time::timeout(DEADLINE, state.lock())
        .await
        .map_err(|_| invalid())?;
    // Take ownership across awaits: cancellation drops and kills this worker
    // instead of leaving an old nonce's reply queued for the next request.
    let mut worker = match state.take() {
        Some(worker) => worker,
        None => Worker::spawn(Command::new(std::env::current_exe()?).arg("session-proof-serve"))?,
    };
    match tokio::time::timeout(DEADLINE, worker.exchange(challenge)).await {
        Ok(Ok(reply)) => {
            *state = Some(worker);
            Ok(reply)
        }
        result => {
            worker.stop().await;
            Err(match result {
                Ok(Err(error)) => error,
                _ => invalid(),
            })
        }
    }
}

#[cfg(test)]
mod tests;
