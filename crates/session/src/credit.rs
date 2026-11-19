//! The credit bulk lanes spend: pictures, stages and mirrors together have
//! at most [`BULK`] bytes on their way at once, so a Stop behind them at
//! the link waits for no more than that. The reading end gives credit back
//! over control, lane by lane, for every read.
//!
//! Credit is counted per lane, and a chunk is owed before any of it is
//! written, so credit the other end gives back always finds its debt. A
//! chunk none of which was written is not owed. One written only in part,
//! because the write failed or was given up, stays owed until its lane
//! closes: what is on its way then cannot be told from what never left, and
//! a closed lane's unread bytes are never read. Credit for a lane that
//! closed, or beyond what it owes, is never counted.
//!
//! A reading end that gives its lane up for good says so with [`ENDED`] in
//! place of a count: everything the lane owes comes back, a wait for it to
//! be repaid ends, and nothing more is sent on it. An end of an older build
//! takes it as a count larger than any debt.
use crate::lane::LaneId;
use atlas_core::error::{Code, Error, Result};
use std::{
    collections::{BTreeSet, HashMap},
    sync::{Mutex, MutexGuard, PoisonError},
};
use tokio::{
    io::{AsyncWrite, AsyncWriteExt},
    sync::Semaphore,
};

/// The most bulk bytes on their way at once, across every lane.
pub const BULK: u32 = 1 << 20;
/// The most sent at once on one lane before credit is asked for again.
pub const CHUNK: usize = 64 * 1024;
/// What a reading end gives back in place of a count once it gave its lane
/// up for good.
pub const ENDED: u64 = u64::MAX;
/// The most lanes remembered as given up, the newest kept: each is
/// forgotten once it closes, soon after.
const GIVEN_UP: usize = 1024;

/// The sending end's credit.
#[derive(Debug)]
pub struct Credit {
    available: Semaphore,
    /// What each open lane owes.
    owed: Mutex<HashMap<LaneId, u64>>,
    /// The lanes their reading end gave up, until they close.
    given_up: Mutex<BTreeSet<LaneId>>,
    /// Told whenever credit comes back.
    repaying: tokio::sync::Notify,
}

impl Default for Credit {
    fn default() -> Self {
        Self {
            available: Semaphore::new(BULK as usize),
            owed: Mutex::new(HashMap::new()),
            given_up: Mutex::default(),
            repaying: tokio::sync::Notify::new(),
        }
    }
}

/// A chunk owed while it is written: none of it written, it is not owed
/// after all, however its write ends.
struct Owing<'a> {
    credit: &'a Credit,
    lane: LaneId,
    bytes: u64,
    written: bool,
}

impl Drop for Owing<'_> {
    fn drop(&mut self) {
        if !self.written {
            self.credit.give_back(self.lane, self.bytes);
        }
    }
}

impl Credit {
    fn owed(&self) -> std::sync::MutexGuard<'_, HashMap<LaneId, u64>> {
        self.owed.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn gone(&self) -> MutexGuard<'_, BTreeSet<LaneId>> {
        self.given_up.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether `lane`'s reading end gave it up.
    fn is_given_up(&self, lane: LaneId) -> bool {
        self.gone().contains(&lane)
    }

    /// Send `data` on bulk lane `lane`, waiting for credit for each chunk.
    ///
    /// # Errors
    ///
    /// Fails when the lane does, or its reading end gave it up. A lane whose
    /// send failed, or was given up, is then closed with [`Credit::closed`].
    pub async fn send<W: AsyncWrite + Unpin>(
        &self,
        lane: LaneId,
        writer: &mut W,
        data: &[u8],
    ) -> Result<()> {
        for chunk in data.chunks(CHUNK) {
            let bytes = u32::try_from(chunk.len()).unwrap_or(u32::MAX);
            let permits = self
                .available
                .acquire_many(bytes)
                .await
                .map_err(|_| Error::new(Code::Transport, "The desktop's link closed."))?;
            // Its reader gave it up, maybe while it waited: nothing more goes.
            if self.is_given_up(lane) {
                return Err(given_up());
            }
            permits.forget();
            *self.owed().entry(lane).or_default() += u64::from(bytes);
            let mut owing = Owing {
                credit: self,
                lane,
                bytes: u64::from(bytes),
                written: false,
            };
            let mut at = 0;
            while at < chunk.len() {
                let taken = writer.write(&chunk[at..]).await.map_err(io)?;
                if taken == 0 {
                    return Err(io(std::io::ErrorKind::WriteZero.into()));
                }
                owing.written = true;
                at += taken;
            }
            writer.flush().await.map_err(io)?;
        }
        Ok(())
    }

    /// The reading end read `bytes` more of `lane`: that much may go again.
    /// More than the lane owes is never given back. [`ENDED`] says it gave
    /// the lane up: everything it owes comes back, and nothing more goes.
    pub fn returned(&self, lane: LaneId, bytes: u64) {
        if bytes == ENDED {
            self.give_up(lane);
        } else {
            self.give_back(lane, bytes);
        }
    }

    /// `lane`'s reading end gave it up for good.
    fn give_up(&self, lane: LaneId) {
        {
            let mut given_up = self.gone();
            given_up.insert(lane);
            while given_up.len() > GIVEN_UP {
                given_up.pop_first();
            }
        }
        let given = self.owed().get_mut(&lane).map_or(0, std::mem::take);
        self.available
            .add_permits(usize::try_from(given).unwrap_or(0));
        self.repaying.notify_waiters();
    }

    fn give_back(&self, lane: LaneId, bytes: u64) {
        let given = {
            let mut owed = self.owed();
            let Some(owing) = owed.get_mut(&lane) else {
                return;
            };
            let given = bytes.min(*owing);
            *owing -= given;
            given
        };
        self.available
            .add_permits(usize::try_from(given).unwrap_or(0));
        self.repaying.notify_waiters();
    }

    /// Completes once `lane` owes nothing: everything it sent was read. A
    /// wait dropped changes nothing.
    pub async fn repaid(&self, lane: LaneId) {
        self.owing_at_most(lane, 0).await;
    }

    /// What `lane` owes now: sent and not yet read back.
    pub fn owing(&self, lane: LaneId) -> u64 {
        self.owed().get(&lane).copied().unwrap_or(0)
    }

    /// Completes once `lane` owes at most `bytes`, or its reading end gave it
    /// up: what keeps one lane from having more than that on its way. A
    /// wait dropped changes nothing.
    pub async fn owing_at_most(&self, lane: LaneId, bytes: u64) {
        loop {
            let repaying = self.repaying.notified();
            let owes = self.owed().get(&lane).is_some_and(|owing| *owing > bytes);
            if !owes || self.is_given_up(lane) {
                return;
            }
            repaying.await;
        }
    }

    /// Completes once `lane`'s reading end gave it up; never otherwise.
    pub async fn given_up(&self, lane: LaneId) {
        loop {
            let repaying = self.repaying.notified();
            if self.is_given_up(lane) {
                return;
            }
            repaying.await;
        }
    }

    /// How much credit is left to spend now.
    #[cfg(any(test, feature = "test-support"))]
    pub fn left(&self) -> usize {
        self.available.available_permits()
    }

    /// `lane` closed: what it owes will never be read, and comes back.
    pub fn closed(&self, lane: LaneId) {
        self.gone().remove(&lane);
        let given = self.owed().remove(&lane).unwrap_or(0);
        self.available
            .add_permits(usize::try_from(given).unwrap_or(0));
        self.repaying.notify_waiters();
    }
}

fn given_up() -> Error {
    Error::new(Code::Transport, "The lane's other end gave it up.")
}

fn io(error: std::io::Error) -> Error {
    Error::new(Code::Transport, "A lane to the desktop failed.").with_source(error)
}

#[cfg(test)]
#[path = "credit_tests.rs"]
mod tests;
