//! A machine desktop's mirrors of the station's loopback endpoints: one
//! owner per generation listens on what the station lists, holds each
//! connection it accepts until the station opens its lane, and counts the
//! lanes joined to them. An owner's end is a barrier: the next one binds
//! nothing until every listener, connection and lane of the one before is
//! gone.
use atlas_session::{
    authority::Authority,
    control::{Outbox, Said},
    mirror::{Endpoint, Family, PENDING, Reason, Refused, takeable},
};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering::SeqCst},
    },
    time::Duration,
};
use tokio::{
    net::{TcpListener, TcpSocket, TcpStream},
    sync::{OwnedSemaphorePermit, Semaphore, watch},
    task::JoinHandle,
};

/// How long an accepted connection waits for its lane.
const WAITING: Duration = Duration::from_secs(10);

/// How often an endpoint still listed but not listened on is tried again.
const RETRY: Duration = Duration::from_secs(10);

/// A machine desktop's mirrors, across its generations.
#[derive(Default)]
pub struct Mirrors {
    /// The owner of the generation before, until it has ended.
    previous: Mutex<Option<JoinHandle<()>>>,
}

/// One generation's mirrors.
pub(crate) struct Owner {
    /// The newest list the station gave, if any.
    lists: watch::Sender<Option<(u64, Vec<Endpoint>)>>,
    /// The number of the list in force, which each connection is asked for
    /// under.
    list: AtomicU64,
    pending: Mutex<HashMap<u64, Pending>>,
    next: AtomicU64,
    /// A place for each joined mirror lane: all of them back means none is.
    joined: Arc<Semaphore>,
    outbox: Arc<Outbox>,
}

/// A connection accepted and waiting for its lane.
struct Pending {
    endpoint: Endpoint,
    stream: TcpStream,
    /// Closes with its endpoint's incarnation.
    closing: watch::Receiver<bool>,
    /// When it is closed if no lane came for it.
    until: tokio::time::Instant,
}

/// A connection joined to its lane: what to carry, what ends it, and its
/// place among the joined.
pub(crate) struct Joined {
    pub(crate) stream: TcpStream,
    pub(crate) closing: watch::Receiver<bool>,
    pub(crate) _place: OwnedSemaphorePermit,
}

/// One endpoint listened on, in its incarnation.
struct Listening {
    endpoint: Endpoint,
    accepting: JoinHandle<()>,
    /// Told when this incarnation ends: its connections close.
    closing: watch::Sender<bool>,
}

impl Listening {
    /// Stop listening, and wait until the listener is gone, so its port is
    /// free: its connections close with it.
    async fn close(self) {
        self.closing.send_replace(true);
        self.accepting.abort();
        let _ = self.accepting.await;
    }
}

impl Mirrors {
    /// Start the mirrors of the generation `authority` gives, saying what
    /// they need on `outbox`. Nothing is bound until the generation before
    /// has ended all of its own.
    pub(crate) fn generation(&self, authority: Authority, outbox: Arc<Outbox>) -> Arc<Owner> {
        let owner = Arc::new(Owner {
            lists: watch::Sender::new(None),
            list: AtomicU64::new(0),
            pending: Mutex::default(),
            next: AtomicU64::new(0),
            joined: Arc::new(Semaphore::new(atlas_session::mirror::LANES)),
            outbox,
        });
        // Watched from now: a list that comes while the one before ends is
        // taken up once it has.
        let lists = owner.lists.subscribe();
        let mut previous = self.previous.lock().unwrap_or_else(PoisonError::into_inner);
        let before = previous.take();
        *previous = Some(tokio::spawn(run(owner.clone(), authority, lists, before)));
        owner
    }
}

impl Owner {
    /// The station's newest list: it replaces any not yet taken up.
    pub(crate) fn list(&self, list: u64, endpoints: Vec<Endpoint>) {
        self.lists.send_replace(Some((list, endpoints)));
    }

    /// The connection `connection`, accepted on `endpoint` in that very
    /// incarnation, still open, for its lane: none when no such connection
    /// waits, or every place for a joined lane is taken.
    pub(crate) fn join(&self, connection: u64, endpoint: Endpoint) -> Option<Joined> {
        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        let fits = pending
            .get(&connection)
            .is_some_and(|waiting| waiting.endpoint == endpoint && !*waiting.closing.borrow());
        if !fits {
            return None;
        }
        let place = self.joined.clone().try_acquire_owned().ok()?;
        let waiting = pending.remove(&connection)?;
        Some(Joined {
            stream: waiting.stream,
            closing: waiting.closing,
            _place: place,
        })
    }

    /// Hold `stream`, accepted on `endpoint`, and ask the station for its
    /// lane: closed at once past the bound, once its incarnation ended, or
    /// when it cannot be asked.
    fn accepted(&self, endpoint: Endpoint, stream: TcpStream, closing: watch::Receiver<bool>) {
        if *closing.borrow() {
            return;
        }
        let connection = self.next.fetch_add(1, SeqCst) + 1;
        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        if pending.len() >= PENDING {
            return;
        }
        let asked = self.outbox.say(Said::Connect {
            connection,
            endpoint,
            list: self.list.load(SeqCst),
        });
        if asked.is_err() {
            return;
        }
        pending.insert(
            connection,
            Pending {
                endpoint,
                stream,
                closing,
                until: tokio::time::Instant::now() + WAITING,
            },
        );
    }

    /// Close every waiting connection no lane came for in time, taking its
    /// request back.
    fn expire(&self) {
        let now = tokio::time::Instant::now();
        self.close_where(|waiting| waiting.until <= now);
    }

    /// Close every waiting connection of the endpoint at `place` that is
    /// not of `incarnation`, or all of them for none.
    fn close_pending(&self, place: (Family, u16), incarnation: Option<u64>) {
        self.close_where(|waiting| {
            waiting.endpoint.place() == place
                && incarnation.is_none_or(|kept| waiting.endpoint.incarnation != kept)
        });
    }

    fn close_where(&self, closes: impl Fn(&Pending) -> bool) {
        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        let gone: Vec<u64> = pending
            .iter()
            .filter(|(_, waiting)| closes(waiting))
            .map(|(connection, _)| *connection)
            .collect();
        for connection in gone {
            pending.remove(&connection);
            self.outbox.withdraw(connection);
        }
    }
}

/// An owner's life: after the one before has ended, listen as each list
/// says, try again what could not be listened on, close what waited too
/// long, and once its generation retires, close everything and wait for
/// its lanes to end.
async fn run(
    owner: Arc<Owner>,
    authority: Authority,
    mut lists: watch::Receiver<Option<(u64, Vec<Endpoint>)>>,
    before: Option<JoinHandle<()>>,
) {
    if let Some(before) = before {
        let _ = before.await;
    }
    let mut listening: HashMap<(Family, u16), Listening> = HashMap::new();
    // The newest list taken up, its endpoints, and what of them could not
    // be listened on: a list refused whole changes none of them.
    let (mut taken, mut wanted, mut refused) = (0, Vec::new(), Vec::new());
    let mut retry = tokio::time::interval(RETRY);
    retry.tick().await;
    let mut expiring = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            () = authority.ended() => break,
            changed = lists.changed() => {
                if changed.is_err() {
                    break;
                }
                let Some((list, endpoints)) = lists.borrow_and_update().clone() else {
                    continue;
                };
                if !takeable(&endpoints) {
                    let whole = endpoints
                        .iter()
                        .map(|endpoint| Refused { endpoint: *endpoint, reason: Reason::Failed })
                        .collect();
                    let _ = owner.outbox.say(Said::Taken { list, refused: whole });
                    continue;
                }
                (taken, wanted) = (list, endpoints);
                owner.list.store(taken, SeqCst);
                refused = follow(&owner, &mut listening, &wanted).await;
                let _ = owner.outbox.say(Said::Taken { list: taken, refused: refused.clone() });
            }
            _ = retry.tick(), if !refused.is_empty() => {
                let now = follow(&owner, &mut listening, &wanted).await;
                if now != refused {
                    refused = now;
                    let _ = owner.outbox.say(Said::Taken { list: taken, refused: refused.clone() });
                }
            }
            _ = expiring.tick() => owner.expire(),
        }
    }
    // Retired: every listener is gone and every waiting connection closed,
    // then the lanes joined in it end with it, before the next owner binds
    // anything.
    for (_, listener) in listening.drain() {
        listener.close().await;
    }
    owner
        .pending
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    let _ = owner
        .joined
        .acquire_many(u32::try_from(atlas_session::mirror::LANES).unwrap_or(u32::MAX))
        .await;
}

/// Listen on `wanted` and nothing else: what is no longer wanted closes,
/// with its connections; an endpoint listed in another incarnation closes
/// the old one's listener and connections and is listened on afresh; what
/// is new is bound. A listener closed is gone before anything is bound in
/// its place. What could not be bound, and why.
async fn follow(
    owner: &Arc<Owner>,
    listening: &mut HashMap<(Family, u16), Listening>,
    wanted: &[Endpoint],
) -> Vec<Refused> {
    let gone: Vec<(Family, u16)> = listening
        .iter()
        .filter(|(place, current)| {
            !wanted
                .iter()
                .any(|endpoint| endpoint.place() == **place && *endpoint == current.endpoint)
        })
        .map(|(place, _)| *place)
        .collect();
    for place in gone {
        if let Some(listener) = listening.remove(&place) {
            listener.close().await;
        }
        owner.close_pending(place, None);
    }
    let mut refused = Vec::new();
    for endpoint in wanted {
        let place = endpoint.place();
        if listening.contains_key(&place) {
            continue;
        }
        match bind(endpoint) {
            Ok(listener) => {
                let (closing, closed) = watch::channel(false);
                let accepting = tokio::spawn(accept(owner.clone(), *endpoint, listener, closed));
                listening.insert(
                    place,
                    Listening {
                        endpoint: *endpoint,
                        accepting,
                        closing,
                    },
                );
            }
            Err(error) => refused.push(Refused {
                endpoint: *endpoint,
                reason: match error.kind() {
                    std::io::ErrorKind::AddrInUse => Reason::InUse,
                    std::io::ErrorKind::PermissionDenied => Reason::Denied,
                    _ => Reason::Failed,
                },
            }),
        }
    }
    refused
}

/// Accept on `listener` until its incarnation ends: a failed accept (as
/// when the process runs out of descriptors for a moment) is tried again
/// after a pause, never the listener's end, which only its owner decides.
async fn accept(
    owner: Arc<Owner>,
    endpoint: Endpoint,
    listener: TcpListener,
    closed: watch::Receiver<bool>,
) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => owner.accepted(endpoint, stream, closed.clone()),
            Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }
}

/// Listen on `endpoint`'s address alone: IPv6 never also takes IPv4.
fn bind(endpoint: &Endpoint) -> std::io::Result<TcpListener> {
    let socket = match endpoint.family {
        Family::V4 => TcpSocket::new_v4()?,
        Family::V6 => {
            let socket = TcpSocket::new_v6()?;
            nix::sys::socket::setsockopt(&socket, nix::sys::socket::sockopt::Ipv6V6Only, &true)?;
            socket
        }
    };
    socket.set_reuseaddr(true)?;
    socket.bind(endpoint.address())?;
    socket.listen(64)
}
