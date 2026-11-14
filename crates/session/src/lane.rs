//! Lanes: a connection of its own for one piece of work, so a picture, a
//! stage or a long request never waits behind another.
//!
//! A lane's header is its opener's first message after the handshake:
//! which generation it belongs to, its ID, and what it is for. The end that
//! answers acts on nothing before the header arrives and is admitted. A
//! lane that changes something names its change, by its kind's type.
use crate::{authority::Authority, generation::Generation, secure::Secure};
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    sync::{Mutex, PoisonError},
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// The longest header read.
const HEADER: usize = 4096;

/// A lane's ID, which the station gives it within its generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LaneId(pub u64);

/// What a lane is for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum LaneKind {
    /// A request that changes nothing.
    Read,
    /// The pictures of the desktop's screen.
    Picture,
    /// A mirror of files.
    Mirror,
    /// A request that changes something.
    Change { change: Token },
    /// A stage committed.
    Stage { change: Token },
}

/// The first message on a lane; a field it does not know is refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Header {
    pub generation: Generation,
    pub lane: LaneId,
    pub kind: LaneKind,
}

/// Which end opened a lane's connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opener {
    Station,
    /// The guest, as the station asked over control.
    Guest,
}

/// The lanes of one generation, as the guest admits them.
#[derive(Debug)]
pub struct Lanes {
    authority: Authority,
    state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
    /// Lanes the station asked the guest to open, not opened yet.
    asked: HashSet<LaneId>,
    /// Lanes cancelled, kept for the rest of the generation: a cancel can
    /// come before its lane.
    cancelled: HashSet<LaneId>,
    /// Lanes admitted; an ID is never admitted twice.
    admitted: HashSet<LaneId>,
}

impl Lanes {
    /// The lanes of the generation `authority` is the right to act for.
    pub fn new(authority: Authority) -> Self {
        Self {
            authority,
            state: Mutex::new(State::default()),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The station asked the guest to open `lane`.
    pub fn asked(&self, lane: LaneId) {
        self.state().asked.insert(lane);
    }

    /// The station cancelled `lane`, open or not.
    pub fn cancel(&self, lane: LaneId) {
        let mut state = self.state();
        state.asked.remove(&lane);
        state.cancelled.insert(lane);
    }

    /// Whether `lane` was cancelled: its work stops at its next check.
    pub fn cancelled(&self, lane: LaneId) -> bool {
        self.state().cancelled.contains(&lane)
    }

    /// Admit the lane `header` opens, which `opener` opened.
    ///
    /// # Errors
    ///
    /// Refuses a lane of another generation or of a retired one, one
    /// cancelled, one admitted before, and one the guest opened that the
    /// station never asked for.
    ///
    /// A lane is admitted under the lock retiring takes: admitted before a
    /// retirement, it is among the lanes the retirement's hook finds; after
    /// one, it is refused.
    pub fn admit(&self, header: &Header, opener: Opener) -> Result<()> {
        if header.generation != self.authority.generation() {
            return Err(refused("it belongs to another connection to the station"));
        }
        self.authority.while_live(|| {
            let mut state = self.state();
            if state.cancelled.contains(&header.lane) {
                return Err(refused("it was cancelled"));
            }
            if state.admitted.contains(&header.lane) {
                return Err(refused("its ID was used before"));
            }
            if opener == Opener::Guest && !state.asked.remove(&header.lane) {
                return Err(refused("the station never asked for it"));
            }
            state.admitted.insert(header.lane);
            Ok(())
        })
    }

    /// Every lane admitted so far: what a retirement's hook cancels.
    pub fn admitted(&self) -> Vec<LaneId> {
        self.state().admitted.iter().copied().collect()
    }
}

/// Send a lane's header: the opener's first message.
///
/// # Errors
///
/// Fails when the connection does.
pub async fn send_header<S: AsyncRead + AsyncWrite + Unpin>(
    lane: &mut Secure<S>,
    header: &Header,
) -> Result<()> {
    let mut line = serde_json::to_vec(header)?;
    line.push(b'\n');
    lane.write_all(&line).await.map_err(io)?;
    lane.flush().await.map_err(io)
}

/// Read a lane's header, the first message on it.
///
/// # Errors
///
/// Fails when the connection does, or what came is not a header.
pub async fn read_header<S: AsyncRead + AsyncWrite + Unpin>(
    lane: &mut Secure<S>,
) -> Result<Header> {
    let mut line = Vec::new();
    let mut byte = [0; 1];
    // Byte by byte, so nothing after the header is read here.
    while line.len() <= HEADER {
        lane.read_exact(&mut byte).await.map_err(io)?;
        if byte[0] == b'\n' {
            return serde_json::from_slice(&line)
                .map_err(|_| Error::new(Code::Protocol, "A lane opened with no header."));
        }
        line.push(byte[0]);
    }
    Err(Error::new(Code::Protocol, "A lane's header is too long."))
}

fn refused(why: &str) -> Error {
    Error::new(Code::Denied, format!("The lane was refused: {why}."))
}

fn io(error: std::io::Error) -> Error {
    Error::new(Code::Io, "The lane's connection failed.").with_source(error)
}

#[cfg(test)]
#[path = "lane_tests.rs"]
mod tests;
