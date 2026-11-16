//! The station's side of one desktop's live generation: lanes it opens to
//! the desktop, each a connection of its own that names its generation,
//! ID and kind first, and their cancelling over control.
use crate::{
    control::{Outbox, Said},
    generation::Generation,
    keys::Keypair,
    lane::{Header, LaneId, LaneKind, send_header},
    secure::{Purpose, Secure, connect_as_station},
};
use atlas_core::{
    desktop::DesktopId,
    error::{Code, Error, Result},
    session::key::PeerKey,
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering::SeqCst},
    },
};
use tokio::io::{AsyncRead, AsyncWrite};

pub mod control;

/// A connection's bytes, before or after its handshake.
pub trait Duplex: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T: AsyncRead + AsyncWrite + Unpin + Send> Duplex for T {}

/// A connection to the desktop, not authenticated yet.
pub type Raw = Box<dyn Duplex>;

/// What a dial gives: a new connection to the desktop.
pub type Dialing<'a> = Pin<Box<dyn Future<Output = std::io::Result<Raw>> + Send + 'a>>;

/// How the station reaches the desktop: the kind's own transport.
pub trait Dial: Send + Sync {
    fn dial(&self) -> Dialing<'_>;
}

/// The desktop as the station reaches it in one live generation.
pub struct Desktop {
    desktop: DesktopId,
    own: Arc<Keypair>,
    key: PeerKey,
    generation: Generation,
    /// The generation's control connection, for cancels.
    outbox: Arc<Outbox>,
    dial: Box<dyn Dial>,
    /// The next lane's ID; the station gives each one once per generation.
    next: AtomicU64,
}

impl std::fmt::Debug for Desktop {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Desktop")
            .field("desktop", &self.desktop)
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

/// A lane open to the desktop.
#[derive(Debug)]
pub struct Opened {
    pub lane: LaneId,
    pub stream: Secure<Raw>,
}

/// Who a desktop is to the station: its ID, the station's own keys, and
/// the desktop's key.
#[derive(Debug, Clone)]
pub struct Peers {
    pub desktop: DesktopId,
    pub own: Arc<Keypair>,
    pub key: PeerKey,
}

impl Desktop {
    /// The desktop `peers` names, live in `generation`, whose control
    /// connection sends from `outbox`, reached by `dial`.
    pub fn new(
        peers: Peers,
        generation: Generation,
        outbox: Arc<Outbox>,
        dial: Box<dyn Dial>,
    ) -> Self {
        Self {
            desktop: peers.desktop,
            own: peers.own,
            key: peers.key,
            generation,
            outbox,
            dial,
            next: AtomicU64::new(1),
        }
    }

    pub fn id(&self) -> &DesktopId {
        &self.desktop
    }

    pub fn generation(&self) -> Generation {
        self.generation
    }

    /// Open a lane of `kind`: dial, authenticate the desktop, then name the
    /// lane. The desktop admits it, or refuses it by closing.
    ///
    /// # Errors
    ///
    /// Fails when the dial, the handshake or the header does.
    pub async fn open(&self, kind: LaneKind) -> Result<Opened> {
        let lane = LaneId(self.next.fetch_add(1, SeqCst));
        let raw = self.dial.dial().await.map_err(|error| {
            Error::new(
                Code::Transport,
                format!("Could not reach desktop {}.", self.desktop.as_str()),
            )
            .with_source(error)
        })?;
        let mut stream =
            connect_as_station(raw, &self.own, self.key, &self.desktop, Purpose::Lane).await?;
        let header = Header {
            generation: self.generation,
            lane,
            kind,
        };
        send_header(&mut stream, &header).await?;
        Ok(Opened { lane, stream })
    }

    /// Cancel `lane`, whose request was given up. A control connection
    /// already closed retires the generation, and its lanes with it.
    pub fn cancel(&self, lane: LaneId) {
        let _ = self.outbox.say(Said::Cancel { lane });
    }
}

#[cfg(test)]
#[path = "station_tests.rs"]
mod tests;
