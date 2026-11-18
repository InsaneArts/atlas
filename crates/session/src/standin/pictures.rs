//! The stand-in's picture lanes, as a machine guest paces them: what a test
//! gives for a request sent through the generation's bulk credit, at most
//! two chunks on their way at once, its lane's credit waited for whole
//! before it ends, and given back once the lane is cancelled or its
//! generation retires. What the station says on the lane after its request
//! is kept, and what each lane still owed when it ended. A live picture
//! answers each readiness and its stop as the test says, as a guest's
//! stream does.
use super::StandIn;
use crate::{
    credit::{CHUNK, Credit},
    lane::{Header, LaneId, Lanes},
};
use atlas_core::error::{Code, Error, Result};
use serde_json::Value;
use std::sync::{Arc, Mutex, PoisonError};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// What a picture lane sends for its request: its bytes, exactly; none
/// leaves the lane open and silent, as a stalled capture's would be.
pub type Pictures = Arc<dyn Fn(&Value) -> Option<Vec<u8>> + Send + Sync>;

/// What the station has told a live picture's lane, as its guest hears it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Told {
    /// Its request, and nothing since.
    Asked,
    /// Its readiness for a frame, the `n`th, counted from 1.
    Ready(u64),
    /// Its stop, and the stop's ID.
    Stop(u64),
}

/// One part of what a live picture's lane sends.
#[derive(Clone)]
pub enum Part {
    /// These bytes, exactly.
    Bytes(Vec<u8>),
    /// Run this once everything sent before it is repaid: only once the
    /// station passed it on, as a keeper repays.
    Then(Arc<dyn Fn() + Send + Sync>),
}

impl std::fmt::Debug for Part {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bytes(bytes) => write!(formatter, "Bytes({})", bytes.len()),
            Self::Then(_) => formatter.write_str("Then(..)"),
        }
    }
}

/// What a live picture's lane sends once its station told it `Told`: its
/// parts in order, and whether the lane ends after them; none leaves the
/// lane open and silent, as a stalled capture's would be.
pub type Streams = Arc<dyn Fn(&Value, Told) -> Option<(Vec<Part>, bool)> + Send + Sync>;

/// How a test makes the stand-in's picture lanes answer; none answers them
/// with the test's own answer, unpaced.
#[derive(Default)]
pub struct Picturing {
    pictures: Mutex<Option<Pictures>>,
    streams: Mutex<Option<Streams>>,
}

impl Picturing {
    /// Answer every picture lane to come with `pictures`.
    pub fn set(&self, pictures: Pictures) {
        *self.pictures.lock().unwrap_or_else(PoisonError::into_inner) = Some(pictures);
    }

    /// Answer every live picture to come with `streams`, as it is told.
    pub fn stream(&self, streams: Streams) {
        *self.streams.lock().unwrap_or_else(PoisonError::into_inner) = Some(streams);
    }

    fn get(&self) -> Option<Pictures> {
        self.pictures
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn streams(&self) -> Option<Streams> {
        self.streams
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl std::fmt::Debug for Picturing {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Picturing(..)")
    }
}

/// How a picture lane's pump ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pumped {
    pub lane: LaneId,
    /// Whether it sent everything and was repaid for it.
    pub whole: bool,
    /// What it was still owed when it ended, given back then.
    pub owed: u64,
}

impl StandIn {
    /// The live generation's credit, which its picture lanes spend.
    pub fn bulk_credit(&self) -> Option<Arc<Credit>> {
        self.credit
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Every picture lane's pump that ended, in order.
    pub fn pumped(&self) -> Vec<Pumped> {
        self.pumped
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// What the station said on picture lane `lane` after its request.
    pub fn heard_on(&self, lane: LaneId) -> Vec<u8> {
        self.heard
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&lane)
            .cloned()
            .unwrap_or_default()
    }

    /// Keep `said`, what the station said on picture lane `lane`.
    fn hear(&self, lane: LaneId, said: &[u8]) {
        self.heard
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(lane)
            .or_default()
            .extend_from_slice(said);
    }
}

/// Answer the picture lane `header` opened, its request `request`, on
/// `stream`, when the stand-in is told how: none when it is not.
pub(super) async fn pictured<S>(
    stand_in: &StandIn,
    lanes: &Lanes,
    header: &Header,
    request: &Value,
    stream: S,
) -> Option<Result<()>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let streams = stand_in
        .pictures
        .streams()
        .filter(|_| request["op"] == "stream");
    let pictures = stand_in.pictures.get();
    if streams.is_none() && pictures.is_none() {
        return None;
    }
    let Some(credit) = stand_in.bulk_credit() else {
        return Some(Err(Error::new(Code::Transport, "No generation is live.")));
    };
    let lane = header.lane;
    let (mut reading, mut writing) = tokio::io::split(stream);
    let whole = if let Some(streams) = streams {
        let lane = Lane {
            stand_in,
            credit: &credit,
            lane,
        };
        tokio::select! {
            sent = lane.streamed(request, &streams, &mut reading, &mut writing) => sent.is_ok(),
            () = lanes.ended(lane.lane) => false,
        }
    } else {
        let hearing = async {
            let mut said = [0_u8; 64];
            while let Ok(read @ 1..) = reading.read(&mut said).await {
                stand_in.hear(lane, &said[..read]);
            }
            std::future::pending::<()>().await;
        };
        let sending = async {
            let Some(bytes) = pictures.and_then(|pictures| pictures(request)) else {
                return std::future::pending().await;
            };
            paced(&credit, lane, &mut writing, &bytes).await?;
            ended(&credit, lane, &mut writing).await
        };
        tokio::select! {
            sent = sending => sent.is_ok(),
            () = lanes.ended(lane) => false,
            () = hearing => false,
        }
    };
    let owed = credit.owing(lane);
    credit.closed(lane);
    stand_in
        .pumped
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(Pumped { lane, whole, owed });
    Some(Ok(()))
}

/// One picture lane of the stand-in, and the credit it spends.
struct Lane<'a> {
    stand_in: &'a StandIn,
    credit: &'a Credit,
    lane: LaneId,
}

impl Lane<'_> {
    /// A live picture asked by `request`: what `streams` gives for its
    /// request, then for each readiness and for its stop as the station
    /// tells them, until it ends the lane or the station goes.
    async fn streamed<R, W>(
        &self,
        request: &Value,
        streams: &Streams,
        reading: &mut R,
        writing: &mut W,
    ) -> Result<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let mut told = Told::Asked;
        let mut readies = 0;
        loop {
            let Some((parts, ends)) = streams(request, told) else {
                return std::future::pending().await;
            };
            for part in parts {
                match part {
                    Part::Bytes(bytes) => paced(self.credit, self.lane, writing, &bytes).await?,
                    Part::Then(then) => {
                        self.credit.repaid(self.lane).await;
                        then();
                    }
                }
            }
            if ends {
                return ended(self.credit, self.lane, writing).await;
            }
            let mut said = [0_u8; 1];
            reading.read_exact(&mut said).await.map_err(gone)?;
            self.stand_in.hear(self.lane, &said);
            told = match said[0] {
                1 => {
                    readies += 1;
                    Told::Ready(readies)
                }
                0 => {
                    let mut id = [0_u8; 8];
                    reading.read_exact(&mut id).await.map_err(gone)?;
                    self.stand_in.hear(self.lane, &id);
                    Told::Stop(u64::from_be_bytes(id))
                }
                _ => return Err(Error::new(Code::BadRequest, "A live picture needs READY.")),
            };
        }
    }
}

/// Send `bytes` on lane `lane` through `credit`, at most two chunks on
/// their way at once.
async fn paced<W: AsyncWrite + Unpin>(
    credit: &Credit,
    lane: LaneId,
    writing: &mut W,
    bytes: &[u8],
) -> Result<()> {
    for chunk in bytes.chunks(CHUNK) {
        credit.owing_at_most(lane, CHUNK as u64).await;
        credit.send(lane, writing, chunk).await?;
    }
    Ok(())
}

/// End lane `lane` once all it sent through `credit` is repaid.
async fn ended<W: AsyncWrite + Unpin>(
    credit: &Credit,
    lane: LaneId,
    writing: &mut W,
) -> Result<()> {
    credit.repaid(lane).await;
    writing.shutdown().await.map_err(gone)
}

fn gone(error: std::io::Error) -> Error {
    Error::new(Code::Transport, "The stand-in's lane failed.").with_source(error)
}
