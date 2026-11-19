//! The keeper's mirrors of this account's loopback endpoints in its guest.
//! A generation whose guest mirrors is told the endpoints this account
//! listens on, read every [`LISTING`], once it opens and on every change,
//! and told a list again every [`RESAY`] until the guest answers that very
//! list. A connection the guest accepted on one of them is carried on a
//! mirror lane of its own, at most [`MIRROR_LANES`] at once, so pictures
//! always keep half of the generation's bulk credit. It is forwarded only
//! to the very process the endpoint was listed for, checked before any
//! byte goes either way.
//!
//! Mirrors run through the person's Stop, outlive every client and end with
//! their generation. The person's maintenance holds them as it holds every
//! agent's lane: nothing is listed and no connection is taken while it is
//! kept, ending or cannot be read, and a hold is answered only once every
//! relay ended and the guest took the empty list.
use super::{Keeper, Session, held, lanes::Work};
use crate::vm::{
    maintenance::Kept,
    mirrors::{Chosen, Incarnations, Listeners, endpoints},
};
use atlas_core::error::{Code, Error, Result};
use atlas_native::native::listeners::Listener;
use atlas_session::{
    compatibility::{MIRRORS, Usable, judge},
    control::Said,
    generation::Generation,
    mirror::{self, ENDPOINTS, Endpoint, Family, PENDING, Reason, Refused},
    station::control::Event,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, VecDeque},
    future::Future,
    net::SocketAddr,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    task::Poll,
    time::Duration,
};
use tokio::{
    sync::{Semaphore, mpsc, watch},
    time::{Instant, MissedTickBehavior},
};

/// The most mirror lanes open at once: each has at most a chunk on its
/// way, so half of the generation's bulk credit is always left for
/// pictures.
pub(super) const MIRROR_LANES: usize = 8;
/// How often this account's listeners and the person's maintenance are
/// read.
const LISTING: Duration = Duration::from_secs(2);
/// How long a list waits for its answer before it is said again.
const RESAY: Duration = Duration::from_secs(5);
/// How long the guest holds a connection it accepted for its lane: one that
/// waits longer for room is closed there.
const WAITING: Duration = Duration::from_secs(10);
/// How long a connection is given to be found taken by the very listener
/// its endpoint was listed for, while it may still wait in that listener's
/// queue, and how often whoever took it is looked for meanwhile.
const SETTING_UP: Duration = Duration::from_secs(1);
const LOOKING: Duration = Duration::from_millis(20);
/// How long a hold waits for the guest to take the empty list.
const CLOSING: Duration = Duration::from_secs(10);
/// How many connections of a generation are remembered as asked for, so a
/// request said twice opens no second lane.
const SEEN: usize = 1024;
/// The most of the guest's mirror messages waiting for its mirrors.
pub(super) const HEARD: usize = 64;

/// The keeper's mirrors, across its generations.
pub(super) struct Mirrors {
    listeners: Arc<dyn Listeners>,
    incarnations: Mutex<Incarnations>,
    /// What the live generation's mirrors stand at.
    standing: watch::Sender<Standing>,
    /// Told whenever whether mirrors may run changes: a hold or a release.
    relisting: watch::Sender<u64>,
    /// Room for the blocking reads of this account's processes: one listing
    /// at a time, and a check per lane. Each read holds its place until it
    /// returns, so one given up never leaves more running than these.
    scans: Arc<Semaphore>,
    checks: Arc<Semaphore>,
}

/// What a generation's mirrors stand at, for status and for a hold.
#[derive(Clone, Debug, Default)]
struct Standing {
    generation: Option<Generation>,
    /// Why its guest does not mirror, and what to do next, when it does
    /// not.
    unserved: Option<(String, Option<Value>)>,
    /// Whether the list said last was emptied for the person's maintenance.
    held: bool,
    /// The list said last, its endpoints, and whether the guest answered
    /// it, with what of it the guest could not listen on.
    list: u64,
    endpoints: Vec<Endpoint>,
    answered: bool,
    refused: Vec<Refused>,
    /// The endpoints left out past the bound.
    omitted: Vec<(Family, u16)>,
    /// How many connections closed with nothing carried.
    failed: u64,
}

impl Mirrors {
    pub(super) fn new(listeners: Arc<dyn Listeners>) -> Self {
        Self {
            listeners,
            incarnations: Mutex::default(),
            standing: watch::Sender::new(Standing::default()),
            relisting: watch::Sender::new(0),
            scans: Arc::new(Semaphore::new(1)),
            checks: Arc::new(Semaphore::new(MIRROR_LANES)),
        }
    }

    /// The mirrors of a keeper started after this one's, over the same
    /// listeners.
    #[cfg(test)]
    pub(super) fn for_next(&self) -> Self {
        Self::new(self.listeners.clone())
    }

    fn incarnations(&self) -> MutexGuard<'_, Incarnations> {
        self.incarnations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Have the live generation's list read and said again now.
    pub(super) fn relist(&self) {
        self.relisting.send_modify(|asked| *asked += 1);
    }

    /// A connection of `generation` closed with nothing carried.
    fn failed(&self, generation: Generation) {
        self.standing.send_if_modified(|standing| {
            let ours = standing.generation == Some(generation);
            if ours {
                standing.failed += 1;
            }
            ours
        });
    }

    /// What the live generation's mirrors stand at, in the words of the VM
    /// `name`: none while no generation's mirrors run.
    pub(super) fn said(&self, name: &str) -> Value {
        let standing = self.standing.borrow().clone();
        if standing.generation.is_none() {
            return Value::Null;
        }
        if let Some((why, next)) = standing.unserved {
            let mut said = json!({"served": false, "why": why});
            if let Some(next) = next {
                said["next"] = next;
            }
            return said;
        }
        let place = |family: Family, port: u16| json!({"family": family, "port": port});
        let listed: Vec<Value> = standing
            .endpoints
            .iter()
            .map(|endpoint| place(endpoint.family, endpoint.port))
            .collect();
        let refused: Vec<Value> = standing
            .refused
            .iter()
            .map(|refused| {
                let mut said = place(refused.endpoint.family, refused.endpoint.port);
                said["reason"] = json!(refused.reason);
                said["why"] = json!(refusal(name, refused.endpoint.port, refused.reason));
                said
            })
            .collect();
        let omitted: Vec<Value> = standing
            .omitted
            .iter()
            .map(|&(family, port)| {
                let mut said = place(family, port);
                said["why"] = json!(omission(name, port));
                said
            })
            .collect();
        json!({"served": true, "held": standing.held, "listed": listed,
            "answered": standing.answered, "refused": refused, "omitted": omitted,
            "failed": standing.failed})
    }
}

/// Why the guest of the VM `name` could not listen on `port`, as `reason`
/// says.
fn refusal(name: &str, port: u16, reason: Reason) -> String {
    match reason {
        Reason::InUse => format!(
            "Something in {name} already listens on port {port}, so this Mac's port {port} does \
             not reach it."
        ),
        Reason::Denied => format!("{name} does not let Atlas listen on port {port}."),
        Reason::Failed => format!("{name} could not listen on port {port}."),
    }
}

/// Why this Mac's `port` is not mirrored into the VM `name`.
fn omission(name: &str, port: u16) -> String {
    format!(
        "Atlas mirrors at most {ENDPOINTS} of this Mac's ports into {name}, so port {port} is \
         left out."
    )
}

/// The generation `generation`'s mirrors as status says them, from their
/// start until they end, however they end.
struct Opened<'a> {
    mirrors: &'a Mirrors,
    generation: Generation,
}

impl<'a> Opened<'a> {
    fn new(mirrors: &'a Mirrors, generation: Generation, unserved: Option<Error>) -> Self {
        let unserved =
            unserved.map(|unserved| (unserved.error, unserved.details.get("next").cloned()));
        mirrors.standing.send_replace(Standing {
            generation: Some(generation),
            unserved,
            ..Standing::default()
        });
        Self {
            mirrors,
            generation,
        }
    }
}

impl Drop for Opened<'_> {
    fn drop(&mut self) {
        self.mirrors.standing.send_if_modified(|standing| {
            let ours = standing.generation == Some(self.generation);
            if ours {
                *standing = Standing::default();
            }
            ours
        });
    }
}

/// The relays a generation's mirrors run, each until its connection ends.
#[derive(Default)]
struct Relays<'a>(Vec<Pin<Box<dyn Future<Output = ()> + Send + 'a>>>);

impl<'a> Relays<'a> {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn push(&mut self, relay: impl Future<Output = ()> + Send + 'a) {
        self.0.push(Box::pin(relay));
    }

    /// Completes once one or more of them ended, each dropped then; never
    /// while none runs.
    async fn ended(&mut self) {
        std::future::poll_fn(|context| {
            let running = self.0.len();
            self.0
                .retain_mut(|relay| relay.as_mut().poll(context).is_pending());
            if self.0.len() < running {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await;
    }
}

/// The connections of a generation asked for, the newest [`SEEN`] of them.
#[derive(Default)]
struct Seen(BTreeSet<u64>);

impl Seen {
    /// Whether `connection` is asked for the first time: one older than
    /// every connection remembered counts as asked before.
    fn first(&mut self, connection: u64) -> bool {
        if self.0.len() >= SEEN && self.0.first().is_some_and(|oldest| connection <= *oldest) {
            return false;
        }
        let first = self.0.insert(connection);
        if self.0.len() > SEEN {
            self.0.pop_first();
        }
        first
    }
}

/// A connection the guest accepted and asked a lane for.
#[derive(Clone, Copy, Debug)]
struct Asked {
    connection: u64,
    endpoint: Endpoint,
}

/// One generation's mirrors, as its loop keeps them.
struct Mirroring<'a> {
    /// The number of the list said last, and its endpoints, which every
    /// relay watches: one of an endpoint no longer listed ends.
    list: u64,
    endpoints: watch::Sender<Vec<Endpoint>>,
    /// The endpoints left out of that list, past the bound.
    omitted: Vec<(Family, u16)>,
    /// When the list is said again, until it is answered.
    resay: Option<Instant>,
    seen: Seen,
    /// Connections waiting for room among the lanes, each until the guest
    /// gives up on it.
    waiting: VecDeque<(Asked, Instant)>,
    relays: Relays<'a>,
}

/// What this account's listeners give the mirrors now.
enum Wanted {
    /// The person's maintenance holds them: nothing is listed.
    Held,
    Chosen(Chosen),
    /// The listing failed: the list stays as it was.
    Unknown,
}

/// What woke a generation's mirrors.
enum Woke {
    /// The person's maintenance and the listeners are due to be read.
    Listing,
    /// A hold or a release: a new list is said.
    Relisting,
    /// The person's maintenance was read: whether it holds the mirrors.
    Maintained(bool),
    /// The listeners were read.
    Discovered(Wanted),
    Resay,
    Heard(Event),
    Ended,
}

/// A read under way.
type Pending<'a, T> = Option<Pin<Box<dyn Future<Output = T> + Send + 'a>>>;

/// Completes with what the read under way in `pending` came to; never
/// while none is.
async fn finished<T>(pending: &mut Pending<'_, T>) -> T {
    std::future::poll_fn(|context| {
        pending
            .as_mut()
            .map_or(Poll::Pending, |read| read.as_mut().poll(context))
    })
    .await
}

impl Keeper {
    /// Mirror this account's loopback endpoints in the guest of `session`
    /// for its life, told what the guest says of them on `heard`. A guest
    /// that does not mirror is told nothing. The person's maintenance is
    /// read apart from the listeners, so a listing that stalls never keeps
    /// it from emptying the list, and relays go on while either is read.
    pub(super) async fn mirroring(&self, session: &Session, mut heard: mpsc::Receiver<Event>) {
        let generation = session.control.generation();
        let unserved = self.unserved(session, &[MIRRORS], |name| {
            format!("{name}'s Atlas cannot mirror this Mac's ports.")
        });
        let served = unserved.is_none();
        let _opened = Opened::new(&self.mirrors, generation, unserved);
        if !served {
            return std::future::pending().await;
        }
        let mut relisting = self.mirrors.relisting.subscribe();
        let mut mirroring = Mirroring {
            list: 0,
            endpoints: watch::Sender::new(Vec::new()),
            omitted: Vec::new(),
            resay: None,
            seen: Seen::default(),
            waiting: VecDeque::new(),
            relays: Relays::default(),
        };
        let mut listing = tokio::time::interval(LISTING);
        listing.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut checking: Pending<'_, bool> = None;
        let mut reading: Pending<'_, Wanted> = None;
        // Nothing is listed until the person's maintenance is read.
        let (mut maintained, mut fresh, mut hearing) = (true, false, true);
        loop {
            let resay = mirroring.resay;
            let woke = tokio::select! {
                _ = listing.tick() => Woke::Listing,
                changed = relisting.changed() => match changed {
                    Ok(()) => Woke::Relisting,
                    Err(_) => return std::future::pending().await,
                },
                kept = finished(&mut checking), if checking.is_some() => Woke::Maintained(kept),
                wanted = finished(&mut reading), if reading.is_some() => Woke::Discovered(wanted),
                () = tokio::time::sleep_until(resay.unwrap_or_else(Instant::now)),
                    if resay.is_some() => Woke::Resay,
                event = heard.recv(), if hearing => match event {
                    Some(event) => Woke::Heard(event),
                    None => {
                        hearing = false;
                        continue;
                    }
                },
                () = mirroring.relays.ended(), if !mirroring.relays.is_empty() => Woke::Ended,
            };
            match woke {
                Woke::Listing | Woke::Relisting => {
                    if matches!(woke, Woke::Relisting) {
                        fresh = true;
                    }
                    // A hold empties the list at once, whatever is being read.
                    if self.lanes.held().is_some() && fresh {
                        self.listed(session, &mut mirroring, Wanted::Held, true);
                        fresh = false;
                    }
                    if checking.is_none() {
                        checking = Some(Box::pin(self.maintained()));
                    }
                    if !maintained && reading.is_none() && self.lanes.held().is_none() {
                        reading = Some(Box::pin(self.discovered()));
                    }
                }
                Woke::Maintained(kept) => {
                    checking = None;
                    maintained = kept;
                    if maintained || self.lanes.held().is_some() {
                        self.listed(session, &mut mirroring, Wanted::Held, fresh);
                        fresh = false;
                    } else if reading.is_none() {
                        reading = Some(Box::pin(self.discovered()));
                    }
                }
                Woke::Discovered(wanted) => {
                    reading = None;
                    // Read before the maintenance held the mirrors: left
                    // unsaid.
                    if !maintained && self.lanes.held().is_none() {
                        self.listed(session, &mut mirroring, wanted, fresh);
                        fresh = false;
                    }
                }
                Woke::Resay => {
                    let endpoints = mirroring.endpoints.borrow().clone();
                    let list = mirroring.list;
                    let _ = session.control.say(Said::Mirror { list, endpoints });
                    mirroring.resay = Some(Instant::now() + RESAY);
                }
                Woke::Heard(Event::Taken { list, refused }) if list == mirroring.list => {
                    mirroring.resay = None;
                    self.mirrors.standing.send_modify(|standing| {
                        standing.answered = true;
                        standing.refused = refused;
                    });
                }
                Woke::Heard(Event::Connect {
                    connection,
                    endpoint,
                    ..
                }) => {
                    let asked = Asked {
                        connection,
                        endpoint,
                    };
                    self.asked(session, &mut mirroring, asked);
                }
                Woke::Heard(_) | Woke::Ended => {}
            }
            self.make_room(session, &mut mirroring);
        }
    }

    /// Whether the person's maintenance holds the mirrors now: kept, ending
    /// or unreadable. A record read keeping none lets a hold taken from it
    /// go.
    async fn maintained(&self) -> bool {
        let (kept, _) = held::kept(self).await;
        if kept == Kept::None {
            self.lanes.settled();
        }
        kept != Kept::None
    }

    /// What this account's listeners give the mirrors now, read one listing
    /// at a time: a listing that stalls holds its place until it returns,
    /// even once given up, so none ever piles up behind it.
    async fn discovered(&self) -> Wanted {
        let Ok(place) = self.mirrors.scans.clone().acquire_owned().await else {
            return Wanted::Unknown;
        };
        let listeners = self.mirrors.listeners.clone();
        let found = tokio::task::spawn_blocking(move || {
            let _place = place;
            let listing = listeners.listing()?;
            Ok::<_, std::io::Error>(endpoints(&listing, |listener| listeners.excluded(listener)))
        })
        .await;
        match found {
            Ok(Ok(found)) => Wanted::Chosen(self.mirrors.incarnations().list(&found)),
            Ok(Err(_)) | Err(_) => Wanted::Unknown,
        }
    }

    /// Say what `wanted` gives as the list, when it changed or `fresh` asks
    /// for a new one; a hold that came meanwhile empties it.
    fn listed(
        &self,
        session: &Session,
        mirroring: &mut Mirroring<'_>,
        wanted: Wanted,
        fresh: bool,
    ) {
        let (endpoints, omitted, held) = match wanted {
            Wanted::Chosen(_) | Wanted::Unknown if self.lanes.held().is_some() => {
                (Vec::new(), Vec::new(), true)
            }
            Wanted::Held => (Vec::new(), Vec::new(), true),
            Wanted::Chosen(chosen) => (chosen.endpoints, chosen.omitted, false),
            Wanted::Unknown if fresh => (
                mirroring.endpoints.borrow().clone(),
                mirroring.omitted.clone(),
                false,
            ),
            Wanted::Unknown => return,
        };
        let same = mirroring.list > 0 && *mirroring.endpoints.borrow() == endpoints;
        if same && !fresh {
            mirroring.omitted.clone_from(&omitted);
            self.mirrors.standing.send_if_modified(|standing| {
                let changed = standing.omitted != omitted || standing.held != held;
                standing.omitted = omitted;
                standing.held = held;
                changed
            });
            return;
        }
        mirroring.list += 1;
        let list = mirroring.list;
        mirroring.endpoints.send_replace(endpoints.clone());
        mirroring.omitted.clone_from(&omitted);
        mirroring
            .waiting
            .retain(|(asked, _)| endpoints.contains(&asked.endpoint));
        self.mirrors.standing.send_modify(|standing| {
            standing.list = list;
            standing.endpoints.clone_from(&endpoints);
            standing.answered = false;
            standing.refused.clear();
            standing.omitted = omitted;
            standing.held = held;
        });
        mirroring.resay = Some(Instant::now() + RESAY);
        let _ = session.control.say(Said::Mirror { list, endpoints });
    }

    /// The guest asks a lane for the connection `asked`: carried at once
    /// when there is room, else kept waiting while the guest holds it. One
    /// asked before, on an endpoint not listed in that very incarnation, or
    /// while the person's maintenance holds agents, opens nothing.
    fn asked<'a>(&'a self, session: &'a Session, mirroring: &mut Mirroring<'a>, asked: Asked) {
        if !mirroring.seen.first(asked.connection)
            || !mirroring.endpoints.borrow().contains(&asked.endpoint)
            || self.lanes.held().is_some()
        {
            return;
        }
        if mirroring.relays.len() < MIRROR_LANES {
            self.relay(session, mirroring, asked);
        } else if mirroring.waiting.len() < PENDING {
            mirroring
                .waiting
                .push_back((asked, Instant::now() + WAITING));
        }
    }

    /// Carry the connections waiting while there is room, leaving out those
    /// the guest gave up on.
    fn make_room<'a>(&'a self, session: &'a Session, mirroring: &mut Mirroring<'a>) {
        let now = Instant::now();
        while mirroring.relays.len() < MIRROR_LANES {
            let Some((asked, until)) = mirroring.waiting.pop_front() else {
                return;
            };
            if until > now && self.lanes.held().is_none() {
                self.relay(session, mirroring, asked);
            }
        }
        mirroring.waiting.retain(|(_, until)| *until > now);
    }

    /// Run the relay of the connection `asked`, to the listeners its
    /// endpoint was listed for.
    fn relay<'a>(&'a self, session: &'a Session, mirroring: &mut Mirroring<'a>, asked: Asked) {
        let Some(owners) = self
            .mirrors
            .incarnations()
            .owners(&asked.endpoint)
            .map(<[Listener]>::to_vec)
        else {
            return;
        };
        let listed = mirroring.endpoints.subscribe();
        mirroring
            .relays
            .push(self.relayed(session, asked, owners, listed));
    }

    /// Carry the connection `asked` on a mirror lane of `session` to its
    /// endpoint on this Mac, once whoever took it there is one of `owners`
    /// and is not excluded, under a ticket of its own: it ends with its
    /// connection, a hold, the endpoint leaving `listed`, or its generation.
    /// One closed with nothing carried is counted.
    async fn relayed(
        &self,
        session: &Session,
        asked: Asked,
        owners: Vec<Listener>,
        mut listed: watch::Receiver<Vec<Endpoint>>,
    ) {
        let Ok(ticket) = held::admitted(self, Work::Mirror).await else {
            return;
        };
        let endpoint = asked.endpoint;
        let relay = mirror::relay(
            &session.desktop,
            session.control.credit(),
            session.control.outbox(),
            asked.connection,
            endpoint,
            endpoint.address(),
            |own, peer| self.serving(own, peer, owners),
        );
        let failed = tokio::select! {
            relayed = relay => relayed.is_err(),
            () = ticket.stopped() => false,
            _ = listed.wait_for(|listed| !listed.contains(&endpoint)) => false,
        };
        if failed {
            self.mirrors.failed(session.control.generation());
        }
    }

    /// Whether the connection from `own` to `peer` reached one of `owners`,
    /// the listeners its endpoint was listed for, in that very process, and
    /// one never mirrored now, all told within [`SETTING_UP`]: nothing is
    /// carried otherwise.
    async fn serving(
        &self,
        own: SocketAddr,
        peer: SocketAddr,
        owners: Vec<Listener>,
    ) -> Result<()> {
        let checked = tokio::time::timeout(SETTING_UP, self.checked(own, peer, owners)).await;
        if checked != Ok(true) {
            return Err(Error::new(
                Code::Denied,
                "The mirrored connection did not reach the process its endpoint was listed for.",
            ));
        }
        Ok(())
    }

    /// [`serving`](Self::serving), with no deadline of its own: whoever
    /// holds the other end of the connection from `own` to `peer` is looked
    /// for while it may still wait in its listener's queue.
    async fn checked(&self, own: SocketAddr, peer: SocketAddr, owners: Vec<Listener>) -> bool {
        let holder = loop {
            let held = self
                .blocking(move |listeners| listeners.held_by(own, peer))
                .await;
            if let Some(Ok(Some(holder))) = held {
                break holder;
            }
            tokio::time::sleep(LOOKING).await;
        };
        let Some(listener) = owners
            .into_iter()
            .find(|listener| listener.owner() == holder)
        else {
            return false;
        };
        self.blocking(move |listeners| !listeners.excluded(&listener))
            .await
            .unwrap_or(false)
    }

    /// `read` of this account's processes, off the runtime, once one of the
    /// places for a connection's check is free; it holds that place until
    /// it returns, given up or not. None when it could not be run.
    async fn blocking<T: Send + 'static>(
        &self,
        read: impl FnOnce(&dyn Listeners) -> T + Send + 'static,
    ) -> Option<T> {
        let place = self.mirrors.checks.clone().acquire_owned().await.ok()?;
        let listeners = self.mirrors.listeners.clone();
        tokio::task::spawn_blocking(move || {
            let _place = place;
            read(listeners.as_ref())
        })
        .await
        .ok()
    }

    /// Whether the live generation's guest has stopped mirroring for a
    /// hold, waited for within `within` and at most [`CLOSING`]: at once
    /// with no live generation or one whose guest does not mirror; else
    /// once it took an empty list said after the hold, or the generation
    /// ended first.
    pub(super) async fn unmirrored(&self, within: Duration) -> bool {
        let Some(session) = self.shared.live() else {
            return true;
        };
        let generation = session.control.generation();
        if !matches!(
            judge(session.control.declared(), &[MIRRORS]),
            Usable::Usable
        ) {
            return true;
        }
        let mut standing = self.mirrors.standing.subscribe();
        let before = {
            let standing = standing.borrow();
            if standing.generation == Some(generation) {
                standing.list
            } else {
                0
            }
        };
        self.mirrors.relist();
        let closed = standing.wait_for(|standing| {
            standing.generation == Some(generation)
                && standing.list > before
                && standing.endpoints.is_empty()
                && standing.answered
        });
        let closed = tokio::select! {
            closed = tokio::time::timeout(within.min(CLOSING), closed) => {
                matches!(closed, Ok(Ok(_)))
            }
            () = session.ended() => true,
        };
        closed || *session.retired.borrow()
    }
}

#[cfg(test)]
#[path = "mirrors_tests.rs"]
pub(super) mod tests;
