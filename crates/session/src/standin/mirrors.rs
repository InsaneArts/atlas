//! The stand-in's mirrors, as a machine guest keeps them. Every list the
//! station says is kept and answered, but for answers a test has it lose.
//! The endpoints a test says something in it holds are refused, and every
//! other one is listened on at a free port of its family on this Mac, as
//! the station's own server holds the endpoint's port here: a test connects
//! there in its place. Each connection accepted asks the station for its
//! lane, and is carried on that lane once it comes, through the
//! generation's credit, until either end or the lane does.
use super::StandIn;
use crate::{
    control::{Outbox, Said},
    lane::{LaneId, Lanes},
    mirror::{Endpoint, Family, Reason, Refused},
};
use atlas_core::error::Result;
use std::{
    collections::{BTreeMap, HashMap},
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError, Weak,
        atomic::{AtomicU64, AtomicUsize, Ordering::SeqCst},
    },
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpListener, TcpStream},
    sync::Notify,
    task::AbortHandle,
};

/// The stand-in's mirrors, across its generations.
#[derive(Default)]
pub struct Mirroring {
    /// Every list the station said, in order.
    lists: Mutex<Vec<(u64, Vec<Endpoint>)>>,
    /// How many answers to lists to come it keeps and never says.
    pub losing: AtomicUsize,
    /// What it says of the endpoints at these places in place of listening
    /// on them: something in it holds them, say.
    pub refusing: Mutex<BTreeMap<(Family, u16), Reason>>,
    listening: Mutex<BTreeMap<(Family, u16), Listening>>,
    /// The list in force, which each connection is asked for under.
    list: AtomicU64,
    /// Connections accepted and waiting for their lanes, by their number.
    pending: Mutex<HashMap<u64, (Endpoint, TcpStream)>>,
    next: AtomicU64,
    /// How many connections are joined to their lanes now, and the most
    /// ever at once.
    joined: AtomicUsize,
    most: AtomicUsize,
    /// Told whenever a list comes, or a connection is joined or ends.
    told: Notify,
}

/// One endpoint listened on, at `at` in its place.
struct Listening {
    endpoint: Endpoint,
    at: SocketAddr,
    accepting: AbortHandle,
}

impl Drop for Mirroring {
    fn drop(&mut self) {
        for listening in lock(&self.listening).values() {
            listening.accepting.abort();
        }
    }
}

impl std::fmt::Debug for Mirroring {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Mirroring(..)")
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Mirroring {
    /// Every list the station said, in order.
    pub fn lists(&self) -> Vec<(u64, Vec<Endpoint>)> {
        lock(&self.lists).clone()
    }

    /// The newest list said, once `holds` holds of it, within a minute.
    ///
    /// # Panics
    ///
    /// When no such list is said in that time.
    pub async fn listed(&self, holds: impl Fn(&[Endpoint]) -> bool) -> (u64, Vec<Endpoint>) {
        let said = async {
            loop {
                let told = self.told.notified();
                if let Some(last) = lock(&self.lists)
                    .last()
                    .filter(|(_, endpoints)| holds(endpoints))
                {
                    return last.clone();
                }
                told.await;
            }
        };
        tokio::time::timeout(std::time::Duration::from_secs(60), said)
            .await
            .expect("such a list was said")
    }

    /// Where it listens in place of the endpoint at `place`, while it does.
    pub fn at(&self, place: (Family, u16)) -> Option<SocketAddr> {
        lock(&self.listening)
            .get(&place)
            .map(|listening| listening.at)
    }

    /// How many connections are joined to their lanes now.
    pub fn joined(&self) -> usize {
        self.joined.load(SeqCst)
    }

    /// The most connections ever joined to their lanes at once.
    pub fn most(&self) -> usize {
        self.most.load(SeqCst)
    }
}

/// A connection joined to its lane, counted until it ends.
struct Joined<'a>(&'a Mirroring);

impl<'a> Joined<'a> {
    fn new(mirroring: &'a Mirroring) -> Self {
        let now = mirroring.joined.fetch_add(1, SeqCst) + 1;
        mirroring.most.fetch_max(now, SeqCst);
        mirroring.told.notify_waiters();
        Self(mirroring)
    }
}

impl Drop for Joined<'_> {
    fn drop(&mut self) {
        self.0.joined.fetch_sub(1, SeqCst);
        self.0.told.notify_waiters();
    }
}

impl StandIn {
    /// Take the station's list `list` of `endpoints`, answered on `outbox`
    /// unless it is to be lost: what is no longer listed, or listed in
    /// another incarnation, is closed; what is new is listened on, but for
    /// what the test has it refuse.
    pub(super) fn mirrored(
        self: &Arc<Self>,
        list: u64,
        endpoints: Vec<Endpoint>,
        answer: impl FnOnce(Said),
    ) {
        let mirroring = &self.mirroring;
        lock(&mirroring.lists).push((list, endpoints.clone()));
        mirroring.list.store(list, SeqCst);
        let refusing = lock(&mirroring.refusing).clone();
        let mut refused = Vec::new();
        {
            let mut listening = lock(&mirroring.listening);
            listening.retain(|place, listening| {
                let kept = endpoints.contains(&listening.endpoint) && !refusing.contains_key(place);
                if !kept {
                    listening.accepting.abort();
                }
                kept
            });
            for endpoint in &endpoints {
                let place = endpoint.place();
                if let Some(reason) = refusing.get(&place) {
                    refused.push(Refused {
                        endpoint: *endpoint,
                        reason: *reason,
                    });
                    continue;
                }
                if listening.contains_key(&place) {
                    continue;
                }
                match bound(endpoint.family) {
                    Ok((listener, at)) => {
                        let accepting =
                            tokio::spawn(accepting(Arc::downgrade(self), *endpoint, listener))
                                .abort_handle();
                        listening.insert(
                            place,
                            Listening {
                                endpoint: *endpoint,
                                at,
                                accepting,
                            },
                        );
                    }
                    Err(_) => refused.push(Refused {
                        endpoint: *endpoint,
                        reason: Reason::Failed,
                    }),
                }
            }
        }
        mirroring.told.notify_waiters();
        if mirroring
            .losing
            .fetch_update(SeqCst, SeqCst, |left| left.checked_sub(1))
            .is_err()
        {
            answer(Said::Taken { list, refused });
        }
    }

    /// Hold `stream`, accepted on `endpoint`, and ask the live generation's
    /// station for its lane.
    fn accepted(&self, endpoint: Endpoint, stream: TcpStream) {
        let mirroring = &self.mirroring;
        let Some(outbox) = lock(&self.outbox).clone() else {
            return;
        };
        let connection = mirroring.next.fetch_add(1, SeqCst) + 1;
        let mut pending = lock(&mirroring.pending);
        pending.insert(connection, (endpoint, stream));
        let asked = outbox.say(Said::Connect {
            connection,
            endpoint,
            list: mirroring.list.load(SeqCst),
        });
        if asked.is_err() {
            pending.remove(&connection);
        }
    }
}

/// A listener on loopback in `family` at a free port, and its address.
fn bound(family: Family) -> std::io::Result<(TcpListener, SocketAddr)> {
    let address: SocketAddr = match family {
        Family::Ipv4 => (Ipv4Addr::LOCALHOST, 0).into(),
        Family::Ipv6 => (Ipv6Addr::LOCALHOST, 0).into(),
    };
    let listener = std::net::TcpListener::bind(address)?;
    listener.set_nonblocking(true)?;
    let at = listener.local_addr()?;
    Ok((TcpListener::from_std(listener)?, at))
}

/// Accept on `listener` for the endpoint `endpoint` until it is closed or
/// the stand-in goes.
async fn accepting(stand_in: Weak<StandIn>, endpoint: Endpoint, listener: TcpListener) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            continue;
        };
        let Some(stand_in) = stand_in.upgrade() else {
            return;
        };
        stand_in.accepted(endpoint, stream);
    }
}

/// Carry the connection `connection`, accepted on `endpoint`, on the
/// mirror lane `lane` the station opened on `stream`, through the live
/// generation's credit, until both ends ended, either failed, or the lane
/// is cancelled or its generation retires. A lane naming no connection
/// waiting on that endpoint is closed.
pub(super) async fn carried<S>(
    stand_in: &StandIn,
    lanes: &Lanes,
    lane: LaneId,
    (connection, endpoint): (u64, Endpoint),
    stream: S,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mirroring = &stand_in.mirroring;
    let local = {
        let mut pending = lock(&mirroring.pending);
        let fits = pending
            .get(&connection)
            .is_some_and(|(waiting, _)| *waiting == endpoint);
        fits.then(|| pending.remove(&connection))
            .flatten()
            .map(|(_, stream)| stream)
    };
    let outbox: Option<Arc<Outbox>> = lock(&stand_in.outbox).clone();
    let (Some(local), Some(credit), Some(outbox)) = (local, stand_in.bulk_credit(), outbox) else {
        return Ok(());
    };
    let _joined = Joined::new(mirroring);
    let (reading, writing) = tokio::io::split(stream);
    tokio::select! {
        () = crate::mirror::carry(local, reading, writing, lane, &credit, &outbox) => {}
        () = lanes.ended(lane) => {}
    }
    Ok(())
}

impl StandIn {
    /// Ask the live generation's station for a lane for `endpoint` with no
    /// connection behind it, as a guest whose request crossed a newer list
    /// does: the number it asked under, none when nothing could be said.
    pub fn ask_lane(&self, endpoint: Endpoint) -> Option<u64> {
        let outbox = lock(&self.outbox).clone()?;
        let connection = self.mirroring.next.fetch_add(1, SeqCst) + 1;
        let list = self.mirroring.list.load(SeqCst);
        outbox
            .say(Said::Connect {
                connection,
                endpoint,
                list,
            })
            .ok()?;
        Some(connection)
    }
}
