//! The stand-in's lanes: each connection the station dials across the link
//! is authenticated, admitted in one generation and answered once.
use super::{
    Asked, Controlling, Live, StandIn,
    link::{Connection, Link, Tasks},
    station_control,
};
use crate::{
    keys::Keypair,
    lane::{Header, LaneKind, Lanes, Opener, read_header},
    secure::{Purpose, connect_as_guest},
    station::{Desktop, Dial, Dialing, Peers, Raw},
};
use atlas_core::{
    error::{Code, Error, Result},
    session::key::PeerKey,
};
use serde_json::Value;
use std::sync::{Arc, Mutex, PoisonError, atomic::Ordering::SeqCst};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};

/// The longest request a stand-in lane reads.
const REQUEST: u64 = 1 << 20;

/// How the stand-in answers a request, given its lane's header. A null
/// answer is none: the lane stays open and says nothing, as a stalled
/// guest's would, until the stand-in goes. A list is the answer's lines,
/// said in order.
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
    let request: Value = serde_json::from_slice(&line)?;
    let answered = match &header.kind {
        // A change to input is kept as any change is, as a real guest keeps
        // it before its input service is asked.
        LaneKind::Change { change }
        | LaneKind::Input {
            change: Some(change),
        } => match stand_in.ask(change) {
            Asked::Begun => {
                let Some(reply) = carried_out(stand_in, lanes, &header, request, answer).await
                else {
                    return Ok(());
                };
                reply
            }
            Asked::Again(Some(reply)) => reply,
            Asked::Again(None) => serde_json::json!({"ok": false, "code": "uncertain",
                "error": "This change was asked for before, and what it came to is unknown",
                "uncertain": true}),
        },
        LaneKind::Stage { change } => {
            let staged =
                super::copies::staged(stand_in, lanes, &header, change, &request, &mut secure)
                    .await?;
            let Some(reply) = staged else {
                return Ok(());
            };
            reply
        }
        LaneKind::Picture => {
            match super::pictures::pictured(stand_in, lanes, &header, &request, &mut secure).await {
                Some(pictured) => return pictured,
                None => answer(&header, request),
            }
        }
        LaneKind::Read if request["op"] == "copies" => stand_in.inventory(&request),
        LaneKind::Read if request["op"] == "copy-contents" => stand_in.copy_contents(&request),
        LaneKind::Read if request["op"] == "installed" => stand_in.installed(&request),
        // A scan may wait for the fixture, off the runtime.
        LaneKind::Read if request["op"] == "native" => {
            tokio::task::spawn_blocking(move || super::native(&request))
                .await
                .unwrap_or_else(|_| {
                    atlas_core::protocol::failure(
                        atlas_core::error::Code::Native,
                        "The native check's folder could not be read.",
                    )
                })
        }
        LaneKind::Read if request["op"] == "outcome" => {
            let kept = request["change"]
                .as_str()
                .and_then(atlas_core::files::Token::parse)
                .and_then(|change| stand_in.outcome(&change));
            match kept {
                Some(outcome) => outcome,
                None => answer(&header, request),
            }
        }
        _ => answer(&header, request),
    };
    if answered.is_null() {
        return std::future::pending().await;
    }
    // A list is its lines in order: events first, its final reply last.
    let lines = match answered {
        Value::Array(lines) => lines,
        one => vec![one],
    };
    for line in lines {
        let mut reply = serde_json::to_vec(&line)?;
        reply.push(b'\n');
        secure.write_all(&reply).await.map_err(io)?;
    }
    secure.shutdown().await.map_err(io)
}

/// Carry out the change on the lane `header` opened, as `answer` answers
/// it: held first while the stand-in holds changes, ended as cancelled
/// when its lane is cancelled meanwhile, and kept as what it came to
/// before it is answered. A change to input held when a Stop comes is cut
/// short there, uncertain, as a real guest's is. None when no reply goes:
/// it was cancelled, or the stand-in drops this one.
async fn carried_out(
    stand_in: &StandIn,
    lanes: &Lanes,
    header: &Header,
    request: Value,
    answer: &Answer,
) -> Option<Value> {
    let (change, input) = match &header.kind {
        LaneKind::Change { change } => (change, false),
        LaneKind::Input {
            change: Some(change),
        } => (change, true),
        _ => return Some(answer(header, request)),
    };
    let stops = stand_in.stops.load(SeqCst);
    while stand_in.holding.load(SeqCst) {
        if input && stand_in.stops.load(SeqCst) != stops {
            let stopped = serde_json::json!({"ok": false, "code": "stopped",
                "error": "Agents were stopped while the change ran", "uncertain": true});
            stand_in.done(change, &stopped);
            return Some(stopped);
        }
        if lanes.cancelled(header.lane) {
            // Cut short with nothing proving what it left ended, as a
            // cancelled command is.
            let cancelled = serde_json::json!({"ok": false, "code": "cancelled",
                "error": "The change was cancelled", "uncertain": true});
            stand_in.done(change, &cancelled);
            return None;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    // The owner input is taken for is kept first, as a change of its own
    // whose outcome is known once its release is acknowledged.
    let owner = request["owner"]
        .as_str()
        .and_then(atlas_core::files::Token::parse)
        .filter(|_| input);
    let operation = request["op"].as_str().map(str::to_owned);
    if let Some(owner) = &owner
        && operation.as_deref() == Some("acquire")
    {
        stand_in.ask(owner);
    }
    // A copy is removed by the guest's own code, as a change it keeps.
    let reply = if request["op"] == "remove-copy" {
        stand_in.remove_copy(&request)
    } else {
        answer(header, request)
    };
    if reply.is_null() {
        return Some(reply);
    }
    // Kept as its final reply, the last of its lines.
    let last = reply
        .as_array()
        .and_then(|lines| lines.last())
        .unwrap_or(&reply);
    // Its owner let go of, or refused input for certain before any was given.
    let ended = match operation.as_deref() {
        Some("release") if last["ok"] == true && last["released"] == true => Some(last.clone()),
        Some("acquire") if last["ok"] == false && last["uncertain"] != true => {
            Some(serde_json::json!({"ok": true, "op": "owner", "taken": false}))
        }
        _ => None,
    };
    if let (Some(owner), Some(ended)) = (&owner, ended)
        && stand_in
            .outcome(owner)
            .is_some_and(|kept| kept["outcome"] == "unknown")
    {
        stand_in.done(owner, &ended);
    }
    stand_in.done(change, last);
    let dropped = stand_in
        .dropping
        .fetch_update(SeqCst, SeqCst, |left| left.checked_sub(1))
        .is_ok();
    (!dropped).then_some(reply)
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
