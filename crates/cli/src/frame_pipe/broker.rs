//! The frame service's broker: one task that owns the pairing gate, every
//! observer's flow and the one source of pictures. Lanes hand it packets
//! and it answers in order, so no two observers race for the source and no
//! packet's handling interleaves with another's.
mod lane;
pub(crate) mod source;

use super::{
    auth::{self, Gate, Half, Paired, Peer},
    endpoint::Listeners,
    subscription::{Ack, Subscription},
};
use atlas_core::frame_pipe::{
    budget::{self, OBSERVERS_MAX},
    diagnostics::{Reason, Stage},
    model::{
        Attach, CreditOnly, Disposition, ErrorMetadata, Failure, FrameMetadata, Hello, Hex32,
        Hex64, Metadata, Pair, Positive, Profile, Ref, Side, Welcome, WireCode, decode, encode,
        flow::{Barrier, Close, Closed, Credit, Dropped, Resynced, Settle, Subscribe, Subscribed},
    },
    wire::{Head, Kind, Lane},
};
use lane::{Event, Id, Outgoing};
use source::{Answer, Latest, Picture};
use std::{
    collections::HashMap,
    future::Future,
    time::{Duration, Instant},
};
use tokio::{net::UnixStream, sync::mpsc};

/// How long a lane may take to pair.
const PAIRING: Duration = Duration::from_secs(2);
/// How old a picture may be when it is served.
const FRESHNESS: Duration = Duration::from_millis(500);
/// How often lapsed credits and slow pairings are looked for.
const TICK: Duration = Duration::from_millis(20);
/// The most sockets accepted at once: four paired observers and as many
/// half-open lanes again.
const SOCKETS: usize = 4 * OBSERVERS_MAX as usize;

/// One accepted socket.
struct Connection {
    lane: Lane,
    peer: Peer,
    socket: lane::Socket,
    opened: Instant,
    last_request: u64,
    /// The observer it belongs to, by its control connection.
    observer: Option<Id>,
}

/// A credit waiting to be served, and the source demand it waits on.
struct Waiting {
    credit: Credit,
    request: u64,
    demand: Option<u64>,
}

/// One observer, from its Welcome to its close.
struct Observer {
    welcome: Welcome,
    frame: Option<Id>,
    half: Option<Half>,
    paired: Option<Paired>,
    subscription: Subscription,
    /// A Subscribe waiting for the source's first picture: its request and
    /// the demand it made.
    subscribing: Option<(u64, u64)>,
    subscribed: bool,
    waiting: Option<Waiting>,
    /// The newest source picture sent, so none is sent twice.
    last_sequence: u64,
}

/// The broker's state.
struct Broker {
    controller: u32,
    service: Hex32,
    gate: Gate,
    source: source::Handle,
    latest: Answer,
    profile: Option<Profile>,
    connections: HashMap<Id, Connection>,
    observers: HashMap<Id, Observer>,
    events: mpsc::Sender<Event>,
    next: Id,
}

/// Serve `listeners` for `controller` until `shutdown`, pairing through
/// `gate` and serving pictures from `source`, then end every observer and
/// give the source back.
pub(crate) async fn run(
    listeners: Listeners,
    controller: u32,
    gate: Gate,
    service: Hex32,
    source: source::Handle,
    shutdown: impl Future<Output = ()>,
) {
    let (events, mut incoming) = mpsc::channel(lane::QUEUE * SOCKETS);
    let mut answers = source.answers.clone();
    let latest = answers.borrow_and_update().clone();
    let mut broker = Broker {
        controller,
        service,
        gate,
        source,
        latest,
        profile: None,
        connections: HashMap::new(),
        observers: HashMap::new(),
        events,
        next: 0,
    };
    let mut tick = tokio::time::interval(TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            () = &mut shutdown => break,
            Ok((stream, _)) = listeners.control.accept() => broker.accept(stream, Lane::Control),
            Ok((stream, _)) = listeners.frame.accept() => broker.accept(stream, Lane::Frame),
            Some(event) = incoming.recv() => broker.event(event, Instant::now()),
            Ok(()) = answers.changed() => {
                broker.latest = answers.borrow_and_update().clone();
                broker.answered(Instant::now());
            }
            _ = tick.tick() => broker.tick(Instant::now()),
        }
    }
    let Broker { source, .. } = broker;
    source.close().await;
}

impl Broker {
    fn accept(&mut self, stream: UnixStream, lane: Lane) {
        if self.connections.len() >= SOCKETS {
            return;
        }
        let Ok(peer) = auth::peer(&stream, self.controller) else {
            return;
        };
        self.next += 1;
        let socket = lane::start(stream, lane, self.next, self.events.clone());
        self.connections.insert(
            self.next,
            Connection {
                lane,
                peer,
                socket,
                opened: Instant::now(),
                last_request: 0,
                observer: None,
            },
        );
    }

    fn event(&mut self, event: Event, now: Instant) {
        match event {
            Event::Packet {
                connection,
                head,
                metadata,
            } => {
                if let Err(reason) = self.packet(connection, head, &metadata, now) {
                    self.fail(connection, head.request, reason);
                }
            }
            Event::Delivered {
                connection,
                credit,
                at,
            } => {
                if let Some(observer) = self.observer_of(connection) {
                    observer.subscription.delivered(credit, at);
                }
            }
            Event::Ended { connection } => self.end(connection),
        }
    }

    fn observer_of(&mut self, connection: Id) -> Option<&mut Observer> {
        let control = self.connections.get(&connection)?.observer?;
        self.observers.get_mut(&control)
    }

    /// Handle one packet from `connection`.
    fn packet(
        &mut self,
        connection: Id,
        head: Head,
        metadata: &[u8],
        now: Instant,
    ) -> Result<(), Reason> {
        let lane = self.ordered(connection, &head)?;
        let request = head.request;
        match (lane, head.kind) {
            (Lane::Control, Kind::Hello) => self.hello(connection, request, &read(metadata)?, now),
            (Lane::Frame, Kind::Attach) => self.attach(connection, request, &read(metadata)?, now),
            (Lane::Control, Kind::PairConfirm) => {
                self.confirm(connection, request, &read(metadata)?, now)
            }
            (Lane::Control, Kind::Subscribe) => {
                self.subscribe(connection, request, &read(metadata)?, now)
            }
            (Lane::Control, Kind::Credit) => self.credit(connection, request, read(metadata)?, now),
            (Lane::Control, Kind::Decoded) => {
                let settle: Settle = read(metadata)?;
                let ack = Ack::Decoded(settle.reference.clone());
                self.acknowledge(
                    connection,
                    request,
                    settle.credit_id,
                    ack,
                    Kind::DecodedAck,
                    &settle,
                    now,
                )
            }
            (Lane::Control, Kind::Commit) => {
                let settle: Settle = read(metadata)?;
                let ack = Ack::Commit(settle.reference.clone());
                self.acknowledge(
                    connection,
                    request,
                    settle.credit_id,
                    ack,
                    Kind::CommitAck,
                    &settle,
                    now,
                )
            }
            (Lane::Control, Kind::Drop) => {
                let dropped: Dropped = read(metadata)?;
                let ack = Ack::Drop(dropped.reason, dropped.reference.clone());
                self.acknowledge(
                    connection,
                    request,
                    dropped.credit_id,
                    ack,
                    Kind::DropAck,
                    &dropped,
                    now,
                )
            }
            (Lane::Control, Kind::Resync) => {
                let resynced: Resynced = read(metadata)?;
                let ack = Ack::Resync(resynced.reason);
                self.acknowledge(
                    connection,
                    request,
                    resynced.credit_id,
                    ack,
                    Kind::ResyncAck,
                    &resynced,
                    now,
                )
            }
            (Lane::Control, Kind::Close) => {
                let _: Close = read(metadata)?;
                let paired = self.observers.contains_key(&connection);
                if paired {
                    self.send(
                        connection,
                        Kind::Closed,
                        request,
                        &Closed {
                            cleanup_confirmed: true,
                        },
                    )?;
                }
                self.end(connection);
                Ok(())
            }
            _ => Err(Reason::InvalidSchema),
        }
    }

    /// The lane of `connection`, once `head`'s request keeps its order:
    /// each newer than the last, an acknowledgement's repeat excepted.
    fn ordered(&mut self, connection: Id, head: &Head) -> Result<Lane, Reason> {
        let held = self
            .connections
            .get_mut(&connection)
            .ok_or(Reason::TransportEnded)?;
        let repeat = matches!(
            head.kind,
            Kind::Decoded | Kind::Commit | Kind::Drop | Kind::Resync
        );
        if head.request < held.last_request || (head.request == held.last_request && !repeat) {
            return Err(Reason::InvalidSchema);
        }
        held.last_request = head.request;
        Ok(held.lane)
    }

    fn hello(
        &mut self,
        connection: Id,
        request: u64,
        hello: &Hello,
        now: Instant,
    ) -> Result<(), Reason> {
        let held = self
            .connections
            .get(&connection)
            .ok_or(Reason::TransportEnded)?;
        if held.observer.is_some() {
            return Err(Reason::InvalidSchema);
        }
        let welcome = self.gate.hello(held.peer, hello, now)?;
        self.send(connection, Kind::Welcome, request, &welcome)?;
        if let Some(held) = self.connections.get_mut(&connection) {
            held.observer = Some(connection);
        }
        self.observers.insert(
            connection,
            Observer {
                welcome,
                frame: None,
                half: None,
                paired: None,
                subscription: Subscription::default(),
                subscribing: None,
                subscribed: false,
                waiting: None,
                last_sequence: 0,
            },
        );
        Ok(())
    }

    fn attach(
        &mut self,
        connection: Id,
        request: u64,
        attach: &Attach,
        now: Instant,
    ) -> Result<(), Reason> {
        let held = self
            .connections
            .get(&connection)
            .ok_or(Reason::TransportEnded)?;
        if held.observer.is_some() {
            return Err(Reason::InvalidSchema);
        }
        let (attached, half) = self.gate.attach(held.peer, attach, now)?;
        let control = self
            .observers
            .iter()
            .find(|(_, observer)| observer.welcome.observer_id == attached.observer_id)
            .map(|(control, _)| *control)
            .ok_or(Reason::WrongChallenge)?;
        self.send(connection, Kind::Attached, request, &attached)?;
        if let Some(held) = self.connections.get_mut(&connection) {
            held.observer = Some(control);
        }
        if let Some(observer) = self.observers.get_mut(&control) {
            observer.frame = Some(connection);
            observer.half = Some(half);
        }
        Ok(())
    }

    fn confirm(
        &mut self,
        connection: Id,
        request: u64,
        confirm: &Pair,
        now: Instant,
    ) -> Result<(), Reason> {
        let observer = self
            .observers
            .get_mut(&connection)
            .ok_or(Reason::InvalidSchema)?;
        let half = observer.half.take().ok_or(Reason::InvalidSchema)?;
        observer.paired = Some(half.confirm(confirm, now)?);
        self.send(connection, Kind::PairReady, request, confirm)
    }

    fn subscribe(
        &mut self,
        connection: Id,
        request: u64,
        _: &Subscribe,
        now: Instant,
    ) -> Result<(), Reason> {
        let observer = self
            .observers
            .get_mut(&connection)
            .ok_or(Reason::InvalidSchema)?;
        if observer.paired.is_none() {
            return Err(Reason::InvalidSchema);
        }
        observer.subscription.subscribe()?;
        observer.subscribing = Some((request, self.source.want()));
        self.answered(now);
        Ok(())
    }

    fn credit(
        &mut self,
        connection: Id,
        request: u64,
        credit: Credit,
        now: Instant,
    ) -> Result<(), Reason> {
        let profile = self.profile.ok_or(Reason::InvalidSchema)?;
        let observer = self
            .observers
            .get_mut(&connection)
            .ok_or(Reason::InvalidSchema)?;
        if !observer.subscribed {
            return Err(Reason::InvalidSchema);
        }
        observer.subscription.credit(credit.credit_id, now)?;
        // A credit for a profile since replaced is retired, not served.
        if (credit.stream_generation, credit.geometry_generation)
            != (profile.stream_generation, profile.geometry_generation)
        {
            observer.subscription.retire(credit.credit_id);
            return self.retire(connection, request, credit.credit_id, Reason::WrongGeometry);
        }
        let demand = (credit.barrier != Barrier::Ordinary).then(|| self.source.want());
        observer.waiting = Some(Waiting {
            credit,
            request,
            demand,
        });
        self.send(
            connection,
            Kind::CreditAccepted,
            request,
            &CreditOnly {
                credit_id: credit.credit_id,
            },
        )?;
        self.serve(connection, now);
        Ok(())
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "one call per acknowledgement kind"
    )]
    fn acknowledge<T: serde::Serialize>(
        &mut self,
        connection: Id,
        request: u64,
        credit: Positive,
        ack: Ack,
        answer: Kind,
        metadata: &T,
        now: Instant,
    ) -> Result<(), Reason> {
        let observer = self
            .observers
            .get_mut(&connection)
            .ok_or(Reason::InvalidSchema)?;
        observer
            .subscription
            .acknowledge(request, credit, ack, now)?;
        self.send(connection, answer, request, metadata)
    }

    /// The source answered: subscriptions waiting on it are answered, and
    /// credits are served.
    fn answered(&mut self, now: Instant) {
        let profile = match &self.latest.latest {
            Latest::Picture(picture) => self.profile_of(picture),
            _ => None,
        };
        self.profile = profile.or(self.profile);
        let controls: Vec<Id> = self.observers.keys().copied().collect();
        for control in controls {
            if let Err(reason) = self.answer_subscribe(control) {
                self.fail(control, 0, reason);
                continue;
            }
            self.serve(control, now);
        }
    }

    /// The geometry `picture` has: the current profile while it matches,
    /// else a new geometry generation.
    fn profile_of(&self, picture: &Picture) -> Option<Profile> {
        let (width, height) = (picture.pixels.width(), picture.pixels.height());
        let [display_width, display_height, display_scale] = picture.display;
        let same = self.profile.filter(|profile| {
            (profile.width.get(), profile.height.get()) == (width, height)
                && (
                    profile.display_width,
                    profile.display_height,
                    profile.display_scale,
                ) == (display_width, display_height, display_scale)
                && profile.backend == picture.backend
        });
        if same.is_some() {
            return same;
        }
        let generation = self
            .profile
            .map_or(0, |profile| profile.geometry_generation.get());
        Some(Profile {
            stream_generation: Positive::new(1)?,
            geometry_generation: Positive::new(generation + 1)?,
            width: Side::new(width)?,
            height: Side::new(height)?,
            row_stride: width * 3,
            display_width,
            display_height,
            display_scale,
            backend: picture.backend,
        })
    }

    /// Answer a Subscribe once the source has a profile, if the broker can
    /// hold every subscribed observer's pictures.
    fn answer_subscribe(&mut self, control: Id) -> Result<(), Reason> {
        let Some(profile) = self.profile else {
            return Ok(());
        };
        let subscribed = self
            .observers
            .values()
            .filter(|observer| observer.subscribed)
            .count() as u64;
        let Some(observer) = self.observers.get_mut(&control) else {
            return Ok(());
        };
        let Some((request, demand)) = observer.subscribing else {
            return Ok(());
        };
        if self.latest.demand < demand {
            return Ok(());
        }
        if !budget::admits(profile.bytes(), subscribed + 1, false) {
            return Err(Reason::BudgetExhausted);
        }
        observer.subscribing = None;
        observer.subscribed = true;
        let subscribed = Subscribed {
            profile,
            barrier_generation: Positive::new(demand).ok_or(Reason::Oversized)?,
        };
        self.send(control, Kind::Subscribed, request, &subscribed)
    }

    /// Serve `control`'s waiting credit with the latest picture when it is
    /// fresh, new to the observer and answers the credit's demand; else ask
    /// the source for another.
    fn serve(&mut self, control: Id, now: Instant) {
        let Some(profile) = self.profile else { return };
        let candidate = match &self.latest.latest {
            Latest::Picture(picture)
                if now - picture.at < FRESHNESS && self.profile_of(picture) == Some(profile) =>
            {
                Some(picture.clone())
            }
            _ => None,
        };
        let Some(observer) = self.observers.get_mut(&control) else {
            return;
        };
        let Some(waiting) = &mut observer.waiting else {
            return;
        };
        let answered = waiting
            .demand
            .is_none_or(|demand| self.latest.demand >= demand);
        let last = observer.last_sequence;
        let Some(picture) = candidate.filter(|picture| answered && picture.sequence.get() > last)
        else {
            if answered {
                waiting.demand = Some(self.source.want());
            }
            return;
        };
        let Some(frame) = observer.frame else { return };
        let (credit, request) = (waiting.credit, waiting.request);
        let Some(paired) = &observer.paired else {
            return;
        };
        let metadata = FrameMetadata {
            service_instance: paired.pair.service_instance.clone(),
            observer_id: paired.pair.observer_id.clone(),
            observer_generation: paired.pair.observer_generation,
            stream_generation: profile.stream_generation,
            geometry_generation: profile.geometry_generation,
            credit_id: credit.credit_id,
            source_sequence: picture.sequence,
            image_sequence: picture.sequence,
            base_image_sequence: 0,
            frame_key: Hex64::random(),
            view_generation: credit.view_generation,
            input_revision: 0,
            width: profile.width,
            height: profile.height,
            row_stride: profile.row_stride,
            display_width: profile.display_width,
            display_height: profile.display_height,
            display_scale: profile.display_scale,
            age_at_header_ns: u64::try_from((now - picture.at).as_nanos()).unwrap_or(u64::MAX),
            rectangle_count: 0,
        };
        let reference = Ref {
            service_instance: metadata.service_instance.clone(),
            observer_id: metadata.observer_id.clone(),
            observer_generation: metadata.observer_generation,
            stream_generation: metadata.stream_generation,
            geometry_generation: metadata.geometry_generation,
            source_sequence: metadata.source_sequence,
            image_sequence: metadata.image_sequence,
            frame_key: metadata.frame_key.clone(),
            view_generation: metadata.view_generation,
        };
        if observer.subscription.select(reference, now).is_none() {
            return;
        }
        observer.waiting = None;
        observer.last_sequence = picture.sequence.get();
        let Ok(metadata) = encode(&metadata) else {
            return;
        };
        let queued = self.connections.get(&frame).is_some_and(|lane| {
            lane.socket
                .queue
                .try_send(Outgoing::Frame {
                    request,
                    credit: credit.credit_id,
                    metadata,
                    picture,
                })
                .is_ok()
        });
        if !queued {
            self.fail(control, 0, Reason::SlowWriter);
        }
    }

    /// Expire unserved credits, end deliveries left unsettled and lanes
    /// that never paired.
    fn tick(&mut self, now: Instant) {
        let controls: Vec<Id> = self.observers.keys().copied().collect();
        for control in controls {
            let Some(observer) = self.observers.get_mut(&control) else {
                continue;
            };
            match observer.subscription.lapse(now) {
                Ok(Some(credit)) => {
                    let request = observer.waiting.take().map_or(0, |waiting| waiting.request);
                    let expired = CreditOnly { credit_id: credit };
                    if let Err(reason) = self.send(control, Kind::CreditExpired, request, &expired)
                    {
                        self.fail(control, 0, reason);
                    }
                }
                Ok(None) => self.serve(control, now),
                Err(reason) => self.fail(control, 0, reason),
            }
        }
        let unpaired: Vec<Id> = self
            .connections
            .iter()
            .filter(|(_, held)| {
                now - held.opened >= PAIRING
                    && held
                        .observer
                        .and_then(|control| self.observers.get(&control))
                        .is_none_or(|observer| observer.paired.is_none())
            })
            .map(|(id, _)| *id)
            .collect();
        for connection in unpaired {
            self.fail(connection, 0, Reason::ExpiredCapability);
        }
    }

    /// Queue `metadata` as a `kind` packet answering `request` on
    /// `connection`.
    fn send<T: serde::Serialize>(
        &self,
        connection: Id,
        kind: Kind,
        request: u64,
        metadata: &T,
    ) -> Result<(), Reason> {
        let metadata = encode(metadata).map_err(|refusal| refusal.reason())?;
        let held = self
            .connections
            .get(&connection)
            .ok_or(Reason::TransportEnded)?;
        held.socket
            .queue
            .try_send(Outgoing::Packet {
                kind,
                request,
                metadata,
            })
            .map_err(|_| Reason::SlowWriter)
    }

    /// Retire a credit with an error that names it.
    fn retire(
        &self,
        connection: Id,
        request: u64,
        credit: Positive,
        reason: Reason,
    ) -> Result<(), Reason> {
        let error = ErrorMetadata {
            failure: failure(reason),
            disposition: Disposition::CreditRetired,
            credit_id: Some(credit),
        };
        self.send(connection, Kind::Error, request, &error)
    }

    /// End `connection`'s pair with a fatal error on its control lane, or on
    /// itself when it has no observer.
    fn fail(&mut self, connection: Id, request: u64, reason: Reason) {
        let control = self
            .connections
            .get(&connection)
            .and_then(|held| held.observer)
            .unwrap_or(connection);
        let error = ErrorMetadata {
            failure: failure(reason),
            disposition: Disposition::PairFatal,
            credit_id: None,
        };
        let _ = self.send(control, Kind::Error, request, &error);
        self.end(connection);
    }

    /// Forget `connection` and the observer it belongs to, both its lanes.
    fn end(&mut self, connection: Id) {
        let control = self
            .connections
            .get(&connection)
            .and_then(|held| held.observer);
        self.connections.remove(&connection);
        let Some(observer) = control.and_then(|control| self.observers.remove(&control)) else {
            return;
        };
        if let Some(control) = control {
            self.connections.remove(&control);
        }
        if let Some(frame) = observer.frame {
            self.connections.remove(&frame);
        }
    }
}

/// `metadata` as `T`, refused in the diagnostic vocabulary.
fn read<T: Metadata>(metadata: &[u8]) -> Result<T, Reason> {
    decode(metadata).map_err(|refusal| refusal.reason())
}

/// A failure in the protocol's fixed words, nothing dispatched.
fn failure(reason: Reason) -> Failure {
    let code = match reason {
        Reason::ClientStale | Reason::SourceStale => WireCode::StaleFrame,
        Reason::BudgetExhausted | Reason::SlowWriter | Reason::QueueOverflow => WireCode::Busy,
        Reason::WrongPeer
        | Reason::WrongChallenge
        | Reason::ExpiredCapability
        | Reason::RightsDenied => WireCode::Identity,
        _ => WireCode::Protocol,
    };
    Failure {
        code,
        stage: Stage::Protocol,
        uncertain: false,
        dispatched: false,
        reason,
    }
}

#[cfg(test)]
mod tests;
