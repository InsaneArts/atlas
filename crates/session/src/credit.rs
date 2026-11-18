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
use crate::lane::LaneId;
use atlas_core::error::{Code, Error, Result};
use std::{
    collections::HashMap,
    sync::{Mutex, PoisonError},
};
use tokio::{
    io::{AsyncWrite, AsyncWriteExt},
    sync::Semaphore,
};

/// The most bulk bytes on their way at once, across every lane.
pub const BULK: u32 = 1 << 20;
/// The most sent at once on one lane before credit is asked for again.
pub const CHUNK: usize = 64 * 1024;

/// The sending end's credit.
#[derive(Debug)]
pub struct Credit {
    available: Semaphore,
    /// What each open lane owes.
    owed: Mutex<HashMap<LaneId, u64>>,
    /// Told whenever credit comes back.
    repaying: tokio::sync::Notify,
}

impl Default for Credit {
    fn default() -> Self {
        Self {
            available: Semaphore::new(BULK as usize),
            owed: Mutex::new(HashMap::new()),
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

    /// Send `data` on bulk lane `lane`, waiting for credit for each chunk.
    ///
    /// # Errors
    ///
    /// Fails when the lane does. A lane whose send failed, or was given up,
    /// is then closed with [`Credit::closed`].
    pub async fn send<W: AsyncWrite + Unpin>(
        &self,
        lane: LaneId,
        writer: &mut W,
        data: &[u8],
    ) -> Result<()> {
        for chunk in data.chunks(CHUNK) {
            let bytes = u32::try_from(chunk.len()).unwrap_or(u32::MAX);
            self.available
                .acquire_many(bytes)
                .await
                .map_err(|_| Error::new(Code::Transport, "The desktop's link closed."))?
                .forget();
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
    /// More than the lane owes is never given back.
    pub fn returned(&self, lane: LaneId, bytes: u64) {
        self.give_back(lane, bytes);
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

    /// Completes once `lane` owes at most `bytes`: what keeps one lane from
    /// having more than that on its way. A wait dropped changes nothing.
    pub async fn owing_at_most(&self, lane: LaneId, bytes: u64) {
        loop {
            let repaying = self.repaying.notified();
            if self.owed().get(&lane).is_none_or(|owing| *owing <= bytes) {
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
        let given = self.owed().remove(&lane).unwrap_or(0);
        self.available
            .add_permits(usize::try_from(given).unwrap_or(0));
        self.repaying.notify_waiters();
    }
}

fn io(error: std::io::Error) -> Error {
    Error::new(Code::Transport, "A lane to the desktop failed.").with_source(error)
}

#[cfg(test)]
#[path = "credit_tests.rs"]
mod tests;
