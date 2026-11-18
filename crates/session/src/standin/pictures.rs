//! The stand-in's picture lanes, as a machine guest paces them: what a test
//! gives for a request sent through the generation's bulk credit, at most
//! two chunks on their way at once, its lane's credit waited for whole
//! before it ends, and given back once the lane is cancelled or its
//! generation retires. What the station says on the lane after its request
//! is kept, and what each lane still owed when it ended.
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

/// How a test makes the stand-in's picture lanes answer; none answers them
/// with the test's own answer, unpaced.
#[derive(Default)]
pub struct Picturing(Mutex<Option<Pictures>>);

impl Picturing {
    /// Answer every picture lane to come with `pictures`.
    pub fn set(&self, pictures: Pictures) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(pictures);
    }

    fn get(&self) -> Option<Pictures> {
        self.0
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
    let pictures = stand_in.pictures.get()?;
    let Some(credit) = stand_in.bulk_credit() else {
        return Some(Err(Error::new(Code::Transport, "No generation is live.")));
    };
    let lane = header.lane;
    let (mut reading, mut writing) = tokio::io::split(stream);
    let hearing = async {
        let mut said = [0_u8; 64];
        while let Ok(read @ 1..) = reading.read(&mut said).await {
            stand_in
                .heard
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(lane)
                .or_default()
                .extend_from_slice(&said[..read]);
        }
        std::future::pending::<()>().await;
    };
    let sending = async {
        let Some(bytes) = pictures(request) else {
            return std::future::pending().await;
        };
        for chunk in bytes.chunks(CHUNK) {
            credit.owing_at_most(lane, CHUNK as u64).await;
            credit.send(lane, &mut writing, chunk).await?;
        }
        credit.repaid(lane).await;
        writing.shutdown().await.map_err(|error| {
            Error::new(Code::Transport, "The stand-in's lane failed.").with_source(error)
        })
    };
    let whole = tokio::select! {
        sent = sending => sent.is_ok(),
        () = lanes.ended(lane) => false,
        () = hearing => false,
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
