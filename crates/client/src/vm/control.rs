//! A VM's runner asked on its control socket: one request line, one reply
//! line, the whole exchange under one deadline. The socket answers only
//! Atlas's own code, which the station is.
use atlas_core::error::{Code, Error, Result};
use serde_json::Value;
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::Path,
    time::{Duration, Instant},
};

/// How long a runner is given to answer, the whole exchange; a VM's stop
/// alone may take a minute and more.
const ANSWERING: Duration = Duration::from_secs(150);
/// The longest reply read.
const LONGEST: usize = 64 * 1024;

fn late() -> Error {
    Error::new(Code::Transport, "The VM's runner did not answer in time.")
}

/// The time left before `deadline`; none once it has passed.
fn left(deadline: Instant) -> Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|left| !left.is_zero())
        .ok_or_else(late)
}

/// The runner's reply to `request` on the socket `socket`.
///
/// # Errors
///
/// Fails when nothing listens there, or the runner does not answer whole
/// in time: which says nothing of whether it took effect.
pub(super) fn ask(socket: &Path, request: &Value) -> Result<Value> {
    let deadline = Instant::now() + ANSWERING;
    let failed = |error: std::io::Error| {
        if matches!(
            error.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        ) {
            return late();
        }
        Error::new(Code::Transport, "The VM's runner did not answer.").with_source(error)
    };
    // A Unix socket's connect never waits: macOS refuses one whose
    // listener's queue is full at once.
    let mut stream = UnixStream::connect(socket).map_err(failed)?;
    let mut line = serde_json::to_vec(request)?;
    line.push(b'\n');
    stream
        .set_write_timeout(Some(left(deadline)?))
        .map_err(failed)?;
    stream.write_all(&line).map_err(failed)?;
    // Each read given only what is left, so a peer that trickles bytes
    // cannot stretch the exchange.
    let mut reply = Vec::new();
    let mut chunk = [0_u8; 4096];
    while !reply.ends_with(b"\n") {
        stream
            .set_read_timeout(Some(left(deadline)?))
            .map_err(failed)?;
        let read = stream.read(&mut chunk).map_err(failed)?;
        if read == 0 || reply.len() + read > LONGEST {
            return Err(Error::new(
                Code::Transport,
                "The VM's runner did not answer whole.",
            ));
        }
        reply.extend_from_slice(&chunk[..read]);
    }
    Ok(serde_json::from_slice(&reply)?)
}

#[cfg(test)]
#[path = "control_tests.rs"]
mod tests;
