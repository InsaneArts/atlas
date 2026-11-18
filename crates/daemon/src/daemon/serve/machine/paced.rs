//! What a bulk lane sends, paced by its generation's bulk credit: what its
//! request writes goes into a small pipe, and a pump of its own sends it
//! on, at most two chunks of the lane on their way at once, so pictures and
//! files never crowd out control or each other.
//!
//! The pump's owner gives the lane's credit back however the lane ends:
//! once everything sent was read, once the lane is cancelled or its
//! generation retired, once it made no progress in time, or once the owner
//! is dropped, inside [`Pump::finished`] or not. Each time the pump has
//! ended, and its connection with it, before the credit comes back. A pump
//! that fails ends its lane, so whatever its request still does sees the
//! lane ended too.
use atlas_core::error::{Code, Error, Result};
use atlas_session::{
    credit::{CHUNK, Credit},
    lane::{LaneId, Lanes},
};
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWrite, DuplexStream},
    task::JoinHandle,
    time::Instant,
};

/// How long a bulk lane may owe credit with none of it coming back, or
/// take over one send: past it, the lane is ended and its credit given
/// back, so a reader that stalls never holds the credit others need.
pub(super) const BULK_PROGRESS: Duration = Duration::from_secs(10);

/// The most of one lane's bytes on their way at once: two chunks.
const IN_FLIGHT: u64 = 2 * CHUNK as u64;

/// The pump of one bulk lane, and the credit it owes.
pub(super) struct Pump {
    /// Kept until the pump has ended: an owner dropped while it waits for
    /// it still ends it before its credit comes back.
    task: Option<JoinHandle<Result<()>>>,
    credit: Arc<Credit>,
    lane: LaneId,
}

/// A writer for the bulk lane `lane` of `lanes`, and the pump that sends
/// what is written to `writer` through `credit`, given up once the lane
/// owes credit for `progress` with none coming back, or one send takes
/// longer. The pump ends on its own once the lane is cancelled or its
/// generation retired, and ends the lane when it fails.
pub(super) fn paced<W>(
    lane: LaneId,
    lanes: Arc<Lanes>,
    credit: Arc<Credit>,
    writer: W,
    progress: Duration,
) -> (DuplexStream, Pump)
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (near, far) = tokio::io::duplex(CHUNK);
    let sending = credit.clone();
    let task = tokio::spawn(async move {
        let pumped = tokio::select! {
            pumped = pump(far, writer, lane, &sending, progress) => pumped,
            () = lanes.ended(lane) => Err(Error::new(Code::Cancelled, "The lane ended.")),
        };
        if pumped.is_err() {
            lanes.cancel(lane);
        }
        pumped
    });
    let pump = Pump {
        task: Some(task),
        credit,
        lane,
    };
    (near, pump)
}

impl Pump {
    /// What the pump came to once everything written was sent and read back,
    /// or it ended for another reason, with the lane's credit given back.
    ///
    /// # Errors
    ///
    /// Fails when the lane ended, or made no progress in time.
    pub(super) async fn finished(mut self) -> Result<()> {
        let pumped = match self.task.as_mut() {
            Some(task) => task.await.unwrap_or_else(|_| Err(gone())),
            None => Ok(()),
        };
        // Only now is it known to have ended.
        self.task = None;
        self.credit.closed(self.lane);
        pumped
    }
}

impl Drop for Pump {
    /// Dropped before it finished: the pump is ended, and once it has, its
    /// credit comes back. A pump left to run would owe for good.
    fn drop(&mut self) {
        let Some(task) = self.task.take() else {
            return;
        };
        task.abort();
        let (credit, lane) = (self.credit.clone(), self.lane);
        let refunding = async move {
            let _ = task.await;
            credit.closed(lane);
        };
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => drop(runtime.spawn(refunding)),
            // No runtime left to run the pump either: nothing is on its way.
            Err(_) => self.credit.closed(self.lane),
        }
    }
}

/// When a lane last made progress: when it last had credit back, or began
/// to owe. Only credit coming back moves it; nothing the lane's own end
/// does, its end included, does.
struct Progress<'a> {
    credit: &'a Credit,
    lane: LaneId,
    within: Duration,
    since: Instant,
    owed: u64,
}

impl<'a> Progress<'a> {
    fn of(credit: &'a Credit, lane: LaneId, within: Duration) -> Self {
        Self {
            credit,
            lane,
            within,
            since: Instant::now(),
            owed: 0,
        }
    }

    /// Note what the lane owes now: less than it did is credit back, which
    /// is progress.
    fn look(&mut self) {
        let owed = self.credit.owing(self.lane);
        if owed < self.owed {
            self.since = Instant::now();
        }
        self.owed = owed;
    }

    /// Note `bytes` more sent: a debt begun starts the clock, and owing
    /// less than that came to is credit back meanwhile.
    fn sent(&mut self, bytes: u64) {
        if self.owed == 0 {
            self.since = Instant::now();
        }
        self.owed += bytes;
        self.look();
    }

    /// Completes once the lane has owed credit for its time with none of it
    /// coming back; never while it owes nothing.
    async fn overdue(&mut self) {
        loop {
            self.look();
            let Some(less) = self.owed.checked_sub(1) else {
                return std::future::pending().await;
            };
            tokio::select! {
                () = self.credit.owing_at_most(self.lane, less) => {}
                () = tokio::time::sleep_until(self.since + self.within) => return,
            }
        }
    }
}

/// Send what comes on `from` to `to`, a chunk at a time through `credit`,
/// with at most [`IN_FLIGHT`] of `lane` on its way; once `from` ends, wait
/// until everything sent was read. Each wait, for what comes on `from`
/// included, is given up once the lane owes credit for `progress` with none
/// coming back.
async fn pump<W: AsyncWrite + Unpin>(
    mut from: DuplexStream,
    mut to: W,
    lane: LaneId,
    credit: &Credit,
    progress: Duration,
) -> Result<()> {
    let mut clock = Progress::of(credit, lane, progress);
    let mut chunk = vec![0; CHUNK];
    loop {
        let read = tokio::select! {
            read = from.read(&mut chunk) => read?,
            () = clock.overdue() => return Err(late()),
        };
        if read == 0 {
            break;
        }
        let room = IN_FLIGHT.saturating_sub(read as u64);
        tokio::select! {
            () = credit.owing_at_most(lane, room) => clock.look(),
            () = clock.overdue() => return Err(late()),
        }
        tokio::time::timeout(progress, credit.send(lane, &mut to, &chunk[..read]))
            .await
            .map_err(|_| late())??;
        clock.sent(read as u64);
    }
    tokio::select! {
        () = credit.repaid(lane) => Ok(()),
        () = clock.overdue() => Err(late()),
    }
}

fn late() -> Error {
    Error::new(
        Code::Transport,
        "The station read nothing more of the lane in time.",
    )
}

fn gone() -> Error {
    Error::new(Code::Transport, "The lane's pump stopped.")
}

#[cfg(test)]
#[path = "paced_tests.rs"]
mod tests;
