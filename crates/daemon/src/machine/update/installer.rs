//! The guest's root installer asked, as only the genuine guest daemon may:
//! one request line and one reply line on its socket, the whole exchange
//! under one deadline. The installer judges who asks by the connection
//! itself, and answers a request it knows from its own record.
use atlas_core::error::{Code, Error, Result};
use serde_json::Value;
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::Path,
    time::{Duration, Instant},
};

/// Where the installer answers.
pub(super) const SOCKET: &str = "/var/run/com.insanearts.atlas.machine-installer";
/// How long it is given: an install copies and checks a whole bundle.
const ANSWERING: Duration = Duration::from_secs(300);
/// The longest reply read.
const LONGEST: usize = 64 * 1024;

fn late() -> Error {
    Error::new(
        Code::Transport,
        "The guest's installer did not answer in time.",
    )
}

/// The installer's reply to `request` on the socket at `socket`.
///
/// # Errors
///
/// Fails when nothing listens there or it does not answer whole in time,
/// which says nothing of whether it took effect.
pub(super) fn ask(socket: &Path, request: &Value) -> Result<Value> {
    let deadline = Instant::now() + ANSWERING;
    let left = || {
        deadline
            .checked_duration_since(Instant::now())
            .filter(|left| !left.is_zero())
            .ok_or_else(late)
    };
    let failed = |error: std::io::Error| {
        if matches!(
            error.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        ) {
            return late();
        }
        Error::new(Code::Transport, "The guest's installer did not answer.").with_source(error)
    };
    let mut stream = UnixStream::connect(socket).map_err(failed)?;
    let mut line = serde_json::to_vec(request)?;
    line.push(b'\n');
    stream.set_write_timeout(Some(left()?)).map_err(failed)?;
    stream.write_all(&line).map_err(failed)?;
    let mut reply = Vec::new();
    let mut chunk = [0_u8; 4096];
    while !reply.ends_with(b"\n") {
        stream.set_read_timeout(Some(left()?)).map_err(failed)?;
        let read = stream.read(&mut chunk).map_err(failed)?;
        if read == 0 || reply.len() + read > LONGEST {
            return Err(Error::new(
                Code::Transport,
                "The guest's installer did not answer whole.",
            ));
        }
        reply.extend_from_slice(&chunk[..read]);
    }
    Ok(serde_json::from_slice(&reply)?)
}
