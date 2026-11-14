//! The control connection of one generation: heartbeats both ways, the
//! station's Stop, release and cancels, its requests that the guest open a
//! lane, and the credit bulk lanes spend.
//!
//! Each message is one JSON line. The guest's first is its hello, naming
//! the generation; the station takes nothing from the connection before it
//! arrives, since only a live guest can seal it. What the station says goes
//! out in two queues: Stop, release, cancels and heartbeats first, then
//! dial requests, which are bounded, so a flood of them is refused rather
//! than queued ahead of a Stop. What comes in is handed on without waiting:
//! a cancel or a Stop never waits for a dial handler or for work to finish.
use crate::{
    authority::{Authority, Retired},
    generation::Generation,
    lane::{LaneId, LaneKind, Lanes},
};
use atlas_core::error::{Code, Error, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    sync::Notify,
};

/// How often each end says it is there.
pub const HEARTBEAT: Duration = Duration::from_secs(1);
/// How long the station waits, hearing nothing, before it holds agents on
/// the desktop.
pub const SILENCE: Duration = Duration::from_secs(3);
/// The most dial requests waiting to go; more are refused.
pub const DIALS: usize = 16;
/// The longest control message read.
const LINE: usize = 16 * 1024;

/// One control message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum Said {
    /// The guest's first message: the generation it gave this connection.
    Hello { generation: Generation },
    /// Either end is there.
    Beat,
    /// The person's Stop, from the station.
    Stop,
    /// The station lets control go.
    Release,
    /// The station cancels a lane, open or not.
    Cancel { lane: LaneId },
    /// The station asks the guest to open a lane.
    Dial { lane: LaneId, kind: LaneKind },
    /// The reading end of a bulk lane read this many more bytes of it.
    Credit { lane: LaneId, bytes: u64 },
}

impl Said {
    /// Whether it goes ahead of dial requests.
    const fn urgent(&self) -> bool {
        !matches!(self, Self::Dial { .. })
    }
}

/// What goes out on a control connection, urgent first.
#[derive(Debug, Default)]
pub struct Outbox {
    queues: Mutex<Queues>,
    ready: Notify,
}

#[derive(Debug, Default)]
struct Queues {
    /// Hello, Stop, release and cancels, in the order said.
    urgent: VecDeque<Said>,
    /// Credit waiting to go back, gathered by lane: however much is read,
    /// one message per lane waits.
    credits: BTreeMap<LaneId, u64>,
    dials: VecDeque<Said>,
    closed: bool,
}

impl Queues {
    /// The next to go: urgent, then credit, then dial requests.
    fn pop(&mut self) -> Option<Said> {
        if let Some(said) = self.urgent.pop_front() {
            return Some(said);
        }
        if let Some((lane, bytes)) = self.credits.pop_first() {
            return Some(Said::Credit { lane, bytes });
        }
        self.dials.pop_front()
    }
}

impl Outbox {
    fn queues(&self) -> std::sync::MutexGuard<'_, Queues> {
        self.queues.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Queue `said` to go out.
    ///
    /// # Errors
    ///
    /// Refuses a dial request while [`DIALS`] wait already, and anything
    /// once the connection closed.
    pub fn say(&self, said: Said) -> Result<()> {
        {
            let mut queues = self.queues();
            if queues.closed {
                return Err(Error::new(
                    Code::Transport,
                    "The control connection to the desktop is closed.",
                ));
            }
            if let Said::Credit { lane, bytes } = said {
                let gathered = queues.credits.entry(lane).or_default();
                *gathered = gathered.saturating_add(bytes);
            } else if said.urgent() {
                queues.urgent.push_back(said);
            } else if queues.dials.len() >= DIALS {
                return Err(Error::new(
                    Code::Busy,
                    "Too many lanes are waiting to open on the desktop.",
                ));
            } else {
                queues.dials.push_back(said);
            }
        }
        self.ready.notify_one();
        Ok(())
    }

    /// The hello, when it is the first thing waiting.
    fn hello_first(&self) -> Option<Said> {
        let mut queues = self.queues();
        matches!(queues.urgent.front(), Some(Said::Hello { .. }))
            .then(|| queues.urgent.pop_front())
            .flatten()
    }

    /// The next message waiting, urgent first, without waiting for one.
    fn try_next(&self) -> Option<Said> {
        self.queues().pop()
    }

    /// The next message to send, urgent first; none once closed and empty.
    async fn next(&self) -> Option<Said> {
        loop {
            let notified = self.ready.notified();
            {
                let mut queues = self.queues();
                if let Some(said) = queues.pop() {
                    return Some(said);
                }
                if queues.closed {
                    return None;
                }
            }
            notified.await;
        }
    }

    /// Close: what is queued still goes, then the connection ends, and
    /// nothing more is taken.
    pub fn close(&self) {
        self.queues().closed = true;
        self.ready.notify_one();
    }

    /// The guest's outbox, its hello first: nothing goes before it.
    pub fn hello(generation: Generation) -> Self {
        let outbox = Self::default();
        outbox.queues().urgent.push_back(Said::Hello { generation });
        outbox
    }
}

/// Send what `outbox` holds on `writer`, with a heartbeat every
/// [`HEARTBEAT`], until it closes or the connection fails.
///
/// # Errors
///
/// Fails when the connection does.
///
/// A heartbeat that is due goes before anything queued, so a busy outbox
/// never starves them; the first is due a heartbeat after the start, so
/// the guest's hello goes first. However the sending ends, the outbox
/// closes: nothing more is taken for a connection nobody sends on.
pub async fn send<W: AsyncWrite + Unpin>(mut writer: W, outbox: Arc<Outbox>) -> Result<()> {
    /// Closes the outbox when sending ends, failed or dropped included.
    struct Closing<'a>(&'a Outbox);
    impl Drop for Closing<'_> {
        fn drop(&mut self) {
            self.0.close();
        }
    }
    let _closing = Closing(&outbox);
    // The guest's hello goes before anything else, whatever the clock.
    if let Some(hello) = outbox.hello_first() {
        let mut line = serde_json::to_vec(&hello)?;
        line.push(b'\n');
        writer.write_all(&line).await.map_err(io)?;
        writer.flush().await.map_err(io)?;
    }
    let mut beat_due = tokio::time::Instant::now() + HEARTBEAT;
    let mut just_beat = false;
    loop {
        // After a heartbeat, what waits goes next, so heartbeats that are
        // slow to write never keep a Stop waiting behind them.
        let waiting = if just_beat { outbox.try_next() } else { None };
        let said = match waiting {
            Some(said) => said,
            None if tokio::time::Instant::now() >= beat_due => Said::Beat,
            None => tokio::select! {
                said = outbox.next() => match said {
                    Some(said) => said,
                    None => return writer.shutdown().await.map_err(io),
                },
                () = tokio::time::sleep_until(beat_due) => Said::Beat,
            },
        };
        let mut line = serde_json::to_vec(&said)?;
        line.push(b'\n');
        writer.write_all(&line).await.map_err(io)?;
        writer.flush().await.map_err(io)?;
        // The next heartbeat is due a heartbeat after this one went.
        just_beat = said == Said::Beat;
        if just_beat {
            beat_due = tokio::time::Instant::now() + HEARTBEAT;
        }
    }
}

/// Read one control message.
///
/// # Errors
///
/// Fails when the connection fails or ends, or what came is not a message.
pub async fn read<R: AsyncRead + Unpin>(reader: &mut BufReader<R>) -> Result<Said> {
    let mut line = Vec::new();
    let read = AsyncReadExt::take(&mut *reader, LINE as u64 + 1)
        .read_until(b'\n', &mut line)
        .await
        .map_err(io)?;
    if read == 0 {
        return Err(ended());
    }
    if !line.ends_with(b"\n") {
        return Err(Error::new(Code::Protocol, "A control message is too long."));
    }
    serde_json::from_slice(&line)
        .map_err(|_| Error::new(Code::Protocol, "A control message was not understood."))
}

/// What the guest does with what the station says, each without waiting:
/// a Stop or release is handed on to whatever carries it out.
pub trait Guest: Send + Sync {
    /// The person's Stop came.
    fn stop(&self);
    /// The station let control go.
    fn release(&self);
    /// The station asks for lane `lane`: open one and send its header.
    fn dial(&self, lane: LaneId, kind: LaneKind);
}

/// Follow what the station says on `reader` for the generation `authority`
/// is the right to act for, until the connection fails, which retires it.
pub async fn guest_reads<R: AsyncRead + Unpin>(
    reader: R,
    authority: &Authority,
    lanes: &Lanes,
    credit: &crate::credit::Credit,
    guest: &dyn Guest,
) {
    /// Retires the generation however reading ends, dropped included:
    /// with nothing read, no Stop or cancel could arrive.
    struct Retiring<'a>(&'a Authority);
    impl Drop for Retiring<'_> {
        fn drop(&mut self) {
            self.0.retire(Retired::ControlLost);
        }
    }
    let _retiring = Retiring(authority);
    let mut reader = BufReader::new(reader);
    loop {
        match read(&mut reader).await {
            Ok(Said::Beat) => authority.heard(),
            Ok(Said::Stop) => guest.stop(),
            Ok(Said::Release) => guest.release(),
            Ok(Said::Cancel { lane }) => lanes.cancel(lane),
            Ok(Said::Dial { lane, kind }) => {
                lanes.asked(lane);
                guest.dial(lane, kind);
            }
            Ok(Said::Credit { lane, bytes }) => credit.returned(lane, bytes),
            // A guest never hears a hello.
            Ok(Said::Hello { .. }) | Err(_) => return,
        }
    }
}

/// What the station knows of the guest at the other end of control.
pub trait Station: Send + Sync {
    /// Nothing came for [`SILENCE`]: agents on the desktop are held, and
    /// changes nobody answered stay pending.
    fn silent(&self);
    /// The guest is heard again in the same generation.
    fn heard_again(&self);
}

/// Wait for the guest's hello, the first thing the station takes from a
/// control connection.
///
/// # Errors
///
/// Fails when the connection fails, or its first message is not a hello.
pub async fn hello<R: AsyncRead + Unpin>(reader: &mut BufReader<R>) -> Result<Generation> {
    match read(reader).await? {
        Said::Hello { generation } => Ok(generation),
        _ => Err(Error::new(
            Code::Protocol,
            "The desktop's control connection did not begin with its hello.",
        )),
    }
}

/// Follow what the guest says after its hello, telling `station` when it
/// falls silent and when it is heard again, until the connection fails.
///
/// # Errors
///
/// Returns why the connection ended.
pub async fn station_reads<R: AsyncRead + Unpin>(
    reader: BufReader<R>,
    credit: &crate::credit::Credit,
    station: &dyn Station,
) -> Result<()> {
    // Reading goes on by itself, so a silence never drops half a message.
    let (heard, mut hearing) = tokio::sync::mpsc::channel(1);
    let reading = async move {
        let mut reader = reader;
        loop {
            let said = read(&mut reader).await;
            let ended = said.is_err();
            if heard.send(said).await.is_err() || ended {
                return;
            }
        }
    };
    tokio::pin!(reading);
    let (mut silent, mut read_all) = (false, false);
    loop {
        // In this order: what came, then reading on, and only then the
        // silence, so input already there is never taken for silence.
        let said = tokio::select! {
            biased;
            said = hearing.recv() => said.unwrap_or_else(|| Err(ended())),
            () = &mut reading, if !read_all => {
                read_all = true;
                continue;
            }
            () = tokio::time::sleep(SILENCE) => match hearing.try_recv() {
                // Something came as the silence ended: it is not silence.
                Ok(said) => said,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => Err(ended()),
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                    if !silent {
                        silent = true;
                        station.silent();
                    }
                    continue;
                }
            },
        }?;
        // Checked first: a message the guest may not send ends the
        // connection without counting as the guest heard again.
        match said {
            Said::Beat => {}
            Said::Credit { lane, bytes } => credit.returned(lane, bytes),
            _ => {
                return Err(Error::new(
                    Code::Protocol,
                    "The desktop said something only a station says.",
                ));
            }
        }
        if silent {
            silent = false;
            station.heard_again();
        }
    }
}

fn ended() -> Error {
    Error::new(
        Code::Transport,
        "The control connection to the desktop ended.",
    )
}

fn io(error: std::io::Error) -> Error {
    Error::new(
        Code::Transport,
        "The control connection to the desktop failed.",
    )
    .with_source(error)
}

#[cfg(test)]
#[path = "control_tests.rs"]
mod tests;
