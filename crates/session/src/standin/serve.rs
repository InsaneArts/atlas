//! The stand-in's lanes: each connection the station dials across the link
//! is authenticated, admitted in one generation and answered once.
use super::{
    Controlling, Live, StandIn,
    link::{Connection, Link, Tasks},
    station_control,
};
use crate::{
    keys::Keypair,
    lane::{Header, Lanes, Opener, read_header},
    secure::{Purpose, connect_as_guest},
    station::{Desktop, Dial, Dialing, Peers, Raw},
};
use atlas_core::{
    error::{Code, Error, Result},
    session::key::PeerKey,
};
use serde_json::Value;
use std::sync::{Arc, Mutex, PoisonError};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};

/// The longest request a stand-in lane reads.
const REQUEST: u64 = 1 << 20;

/// How the stand-in answers a request, given its lane's header.
pub type Answer = Arc<dyn Fn(&Header, Value) -> Value + Send + Sync>;

/// The station's way to the stand-in across a link, in one generation.
pub struct Dialer {
    link: Arc<Link>,
    stand_in: Arc<StandIn>,
    lanes: Arc<Lanes>,
    station: PeerKey,
    answer: Answer,
    /// Every header the stand-in admitted, in order.
    pub admitted: Arc<Mutex<Vec<Header>>>,
    serving: Tasks,
}

impl Dialer {
    /// Dial `stand_in` across `link`, its lanes admitted by `lanes` from
    /// the station whose key is `station`, each request answered by
    /// `answer`.
    pub fn new(
        link: Arc<Link>,
        stand_in: Arc<StandIn>,
        lanes: Arc<Lanes>,
        station: PeerKey,
        answer: Answer,
    ) -> Self {
        Self {
            link,
            stand_in,
            lanes,
            station,
            answer,
            admitted: Arc::default(),
            serving: Tasks::default(),
        }
    }
}

impl Dial for Dialer {
    fn dial(&self) -> Dialing<'_> {
        let (station, desktop, connection) = self.link.connect();
        let (stand_in, lanes, station_key, answer, admitted) = (
            self.stand_in.clone(),
            self.lanes.clone(),
            self.station,
            self.answer.clone(),
            self.admitted.clone(),
        );
        let serving = tokio::spawn(async move {
            // A refused lane is closed, which is all the station hears.
            let _ = serve(&stand_in, &lanes, desktop, station_key, &answer, &admitted).await;
            drop(connection);
        });
        self.serving.hold(serving.abort_handle());
        Box::pin(async move { Ok(Box::new(station) as Raw) })
    }
}

/// Authenticate the lane on `stream` as the stand-in, admit its header and
/// answer its one request.
async fn serve<S>(
    stand_in: &StandIn,
    lanes: &Lanes,
    stream: S,
    station: PeerKey,
    answer: &Answer,
    admitted: &Mutex<Vec<Header>>,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut secure = connect_as_guest(
        stream,
        &stand_in.keys,
        station,
        &stand_in.desktop,
        Purpose::Lane,
    )
    .await?;
    let header = read_header(&mut secure).await?;
    lanes.admit(&header, Opener::Station)?;
    admitted
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(header.clone());
    let mut line = Vec::new();
    BufReader::new((&mut secure).take(REQUEST))
        .read_until(b'\n', &mut line)
        .await
        .map_err(io)?;
    let request = serde_json::from_slice(&line)?;
    let mut reply = serde_json::to_vec(&answer(&header, request))?;
    reply.push(b'\n');
    secure.write_all(&reply).await.map_err(io)?;
    secure.shutdown().await.map_err(io)
}

fn io(error: std::io::Error) -> Error {
    Error::new(Code::Io, "The stand-in's lane failed.").with_source(error)
}

/// A stand-in the station reaches across a link, live in one generation.
pub struct Reached {
    pub desktop: Arc<Desktop>,
    pub live: Live,
    pub admitted: Arc<Mutex<Vec<Header>>>,
    pub controlling: Controlling,
    _connection: Connection,
}

/// Reach a new stand-in across `link`: its control connection, and lanes
/// whose requests `answer` answers.
///
/// # Panics
///
/// When the control connection cannot be made.
pub async fn reach(link: &Arc<Link>, answer: Answer) -> Reached {
    let stand_in = StandIn::new(
        atlas_core::desktop::Desktop::from(atlas_core::user::User { uid: 502 }).id(),
        std::time::Duration::ZERO,
    );
    let own = Arc::new(Keypair::generate().expect("a key"));
    let (station_end, desktop_end, connection) = link.connect();
    let (live, controlling) = tokio::join!(
        stand_in.control(desktop_end, own.public()),
        station_control(station_end, &own, stand_in.keys.public(), &stand_in.desktop),
    );
    let (live, controlling) = (
        live.expect("the stand-in goes live"),
        controlling.expect("the station hears it"),
    );
    let dialer = Dialer::new(
        link.clone(),
        stand_in.clone(),
        live.lanes.clone(),
        own.public(),
        answer,
    );
    let admitted = dialer.admitted.clone();
    let peers = Peers {
        desktop: stand_in.desktop.clone(),
        own,
        key: stand_in.keys.public(),
    };
    let desktop = Arc::new(Desktop::new(
        peers,
        controlling.generation,
        controlling.outbox.clone(),
        Box::new(dialer),
    ));
    Reached {
        desktop,
        live,
        admitted,
        controlling,
        _connection: connection,
    }
}
