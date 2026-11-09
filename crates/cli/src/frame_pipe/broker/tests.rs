use super::{source::Acquire, *};
use atlas_core::frame_pipe::{
    budget::{BROKER_PIXELS, Budget},
    model::{
        Attached, ClientKind, Demand, Hex32, Longest, Measure, Right, Role, Source, UpdateKind,
        flow::{Subscribe, SubscribeReason},
    },
    pixels::FramePixels,
    wire::{Decoder, Item, encode as frame},
};
use serde::Serialize;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixListener,
};

/// A source of 4 × 2 pictures whose bytes are their sequence, which can be
/// told to stall, counting what it was asked.
#[derive(Clone, Default)]
pub(super) struct Scripted {
    pub(super) acquired: Arc<AtomicU64>,
    pub(super) stopped: Arc<AtomicU64>,
    pub(super) hang: Arc<AtomicBool>,
    pub(super) fail: Arc<Mutex<Option<Reason>>>,
    /// The helper session the pictures show.
    pub(super) session: Arc<AtomicU64>,
    /// How old, in milliseconds, each picture already is when made.
    pub(super) aged: Arc<AtomicU64>,
}

fn epoch(session: u64) -> Epoch {
    Epoch {
        uid: 501,
        capture_pid: 1,
        display: atlas_core::session::Display {
            width: 4.0,
            height: 2.0,
            scale: 1.0,
        },
        security_session_id: session,
        display_id: 1,
        input_pid: 1,
    }
}

impl Acquire for Scripted {
    async fn acquire(&mut self, budget: &budget::Budget) -> Result<Picture, Reason> {
        // As slow as the live reader's deadline lets an acquisition be.
        if self.hang.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_secs(2)).await;
            return Err(Reason::SourceStale);
        }
        if let Some(reason) = *self.fail.lock().unwrap() {
            return Err(reason);
        }
        let sequence = self.acquired.fetch_add(1, Ordering::SeqCst) + 1;
        let mut pixels = FramePixels::reserve(budget, 4, 2)?;
        pixels.as_mut_slice().fill(sequence as u8);
        let measure = |value| Measure::new(value).unwrap();
        Ok(Picture {
            pixels,
            sequence: Positive::new(sequence).unwrap(),
            at: Instant::now() - Duration::from_millis(self.aged.load(Ordering::SeqCst)),
            display: [measure(4.0), measure(2.0), measure(1.0)],
            backend: Source::Cg,
            epoch: epoch(self.session.load(Ordering::SeqCst)),
            visible: None,
        })
    }

    async fn stop(&mut self) -> Result<(), Reason> {
        self.stopped.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// A broker serving a scripted source from sockets in a private folder.
struct Running {
    _folder: tempfile::TempDir,
    control: std::path::PathBuf,
    frame: std::path::PathBuf,
    service: Hex32,
    bootstrap: Hex64,
    source: Scripted,
    /// Every commit witnessed: its frame key and session.
    witnessed: Arc<Mutex<Vec<(String, u64)>>>,
    /// Whose screen privacy the broker serves by, and the person's lease
    /// that hides it during a sign-in.
    controller: atlas_control::Controller,
    lease: Mutex<atlas_control::Lease>,
    /// Whether every observer is shown every picture, as the app is.
    exempt: Arc<AtomicBool>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

impl Running {
    async fn start(source: Scripted) -> Self {
        let folder = tempfile::Builder::new()
            .prefix("br")
            .tempdir_in("/tmp")
            .unwrap();
        let path = |name: &str| folder.path().join(name);
        let listeners = Listeners {
            control: UnixListener::bind(path("c.sock")).unwrap(),
            frame: UnixListener::bind(path("f.sock")).unwrap(),
        };
        let (service, bootstrap) = (Hex32::random(), Hex64::random());
        let gate = Gate::new(
            service.clone(),
            bootstrap.clone(),
            vec![Right::Observe],
            vec![UpdateKind::Full],
        );
        let controller = atlas_control::Controller::at(505, &folder.path().join("state"));
        let seen = controller.clone();
        let exempt = Arc::new(AtomicBool::new(false));
        let excused = exempt.clone();
        let privacy = Privacy {
            visible: Arc::new(move || seen.screen_visible().ok()),
            exempt: Arc::new(move |_| excused.load(Ordering::SeqCst)),
        };
        let handle = source::spawn(
            source.clone(),
            Budget::new(BROKER_PIXELS),
            Arc::new(tokio::sync::Notify::new()),
            privacy.visible.clone(),
        );
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let uid = nix::unistd::getuid().as_raw();
        let witnessed = Arc::new(Mutex::new(Vec::new()));
        let record = witnessed.clone();
        let witness: Witness = Arc::new(move |key: &str, epoch: &Epoch, _| {
            record
                .lock()
                .unwrap()
                .push((key.to_owned(), epoch.security_session_id));
        });
        let task = tokio::spawn(run(listeners, uid, gate, handle, witness, privacy, async {
            let _ = stopped.await;
        }));
        Self {
            control: path("c.sock"),
            frame: path("f.sock"),
            _folder: folder,
            service,
            bootstrap,
            source,
            witnessed,
            controller,
            lease: Mutex::new(atlas_control::Lease::new(Duration::from_secs(30))),
            exempt,
            stop: Some(stop),
            task,
        }
    }

    /// Hide the screen, as the person's sign-in does.
    fn hide(&self) {
        let mut lease = self.lease.lock().unwrap();
        let token = lease.acquire(&self.controller).unwrap();
        let hold = atlas_control::PrivateHold {
            site: "github.com".to_owned(),
            kind: atlas_control::HoldKind::SignIn,
        };
        lease.hide_screen(&self.controller, &token, &hold).unwrap();
    }

    /// Show it again, as Done does.
    fn show(&self) {
        self.controller.end_private().unwrap();
        self.lease.lock().unwrap().release();
    }

    async fn shut(mut self) {
        let _ = self.stop.take().unwrap().send(());
        self.task.await.unwrap();
    }
}

/// One lane as a client sees it.
struct Side_ {
    stream: UnixStream,
    decoder: Decoder,
    pending: Vec<u8>,
}

/// A received packet and its whole payload.
struct Received {
    head: Head,
    metadata: Vec<u8>,
    payload: Vec<u8>,
}

impl Side_ {
    async fn connect(path: &std::path::Path, lane: Lane) -> Self {
        Self {
            stream: UnixStream::connect(path).await.unwrap(),
            decoder: Decoder::new(lane),
            pending: Vec::new(),
        }
    }

    async fn send<T: Serialize>(&mut self, kind: Kind, request: u64, metadata: &T) {
        let bytes = frame(kind, request, &encode(metadata).unwrap(), 0).unwrap();
        self.stream.write_all(&bytes).await.unwrap();
    }

    /// The next whole packet, or none once the broker closed the lane.
    async fn receive(&mut self) -> Option<Received> {
        let mut found: Option<Received> = None;
        loop {
            let mut input = self.pending.as_slice();
            while let Some(item) = self.decoder.next(&mut input).unwrap() {
                match item {
                    Item::Packet(head, metadata) => {
                        found = Some(Received {
                            head,
                            metadata,
                            payload: Vec::new(),
                        });
                    }
                    Item::Payload(piece) => found.as_mut().unwrap().payload.extend(piece),
                }
                let whole = found
                    .as_ref()
                    .is_some_and(|got| got.payload.len() as u64 == got.head.payload);
                if whole {
                    let rest = input.to_vec();
                    self.pending = rest;
                    return found;
                }
            }
            self.pending.clear();
            let mut buffer = vec![0; 65536];
            let count = tokio::time::timeout(Duration::from_secs(5), self.stream.read(&mut buffer))
                .await
                .expect("the broker answers in time")
                .unwrap_or(0);
            if count == 0 {
                return None;
            }
            self.pending.extend_from_slice(&buffer[..count]);
        }
    }

    /// The next packet, which must be of `kind`.
    async fn expect<T: Metadata>(&mut self, kind: Kind) -> (T, Received) {
        let got = self.receive().await.expect("a packet");
        assert_eq!(
            got.head.kind,
            kind,
            "{}",
            String::from_utf8_lossy(&got.metadata)
        );
        (decode(&got.metadata).unwrap(), got)
    }
}

/// A paired client.
struct Client_ {
    control: Side_,
    frame: Side_,
    pair: Pair,
    request: u64,
}

impl Client_ {
    async fn pair(broker: &Running) -> Self {
        let mut control = Side_::connect(&broker.control, Lane::Control).await;
        let mut frame = Side_::connect(&broker.frame, Lane::Frame).await;
        let hello = Hello {
            protocol_major: 1,
            service_instance: broker.service.clone(),
            bootstrap_capability: broker.bootstrap.clone(),
            client_challenge: Hex32::random(),
            role: Role::Observer,
            client_kind: ClientKind::Terminal,
            requested_rights: vec![Right::Observe],
            offered_updates: vec![UpdateKind::Full],
        };
        control.send(Kind::Hello, 1, &hello).await;
        let (welcome, _): (Welcome, _) = control.expect(Kind::Welcome).await;
        let attach = Attach {
            service_instance: welcome.service_instance.clone(),
            observer_id: welcome.observer_id.clone(),
            observer_generation: welcome.observer_generation,
            control_connection_id: welcome.control_connection_id.clone(),
            frame_challenge: Hex32::random(),
            pair_token: welcome.pair_token.clone(),
        };
        frame.send(Kind::Attach, 1, &attach).await;
        let (attached, _): (Attached, _) = frame.expect(Kind::Attached).await;
        let pair = Pair {
            service_instance: welcome.service_instance,
            observer_id: welcome.observer_id,
            observer_generation: welcome.observer_generation,
            control_connection_id: welcome.control_connection_id,
            frame_connection_id: attached.frame_connection_id,
        };
        control.send(Kind::PairConfirm, 2, &pair).await;
        let (ready, _): (Pair, _) = control.expect(Kind::PairReady).await;
        assert_eq!(ready, pair);
        Self {
            control,
            frame,
            pair,
            request: 2,
        }
    }

    fn request(&mut self) -> u64 {
        self.request += 1;
        self.request
    }

    async fn subscribe(&mut self) -> Subscribed {
        self.subscribe_for(SubscribeReason::First).await
    }

    async fn subscribe_for(&mut self, reason: SubscribeReason) -> Subscribed {
        let subscribe = Subscribe {
            reason,
            demand: Demand {
                desired_longest: Longest::new(1600).unwrap(),
                backend: Source::Cg,
                hard_ceiling: None,
            },
            view_generation: Positive::new(1).unwrap(),
        };
        let request = self.request();
        self.control
            .send(Kind::Subscribe, request, &subscribe)
            .await;
        self.control.expect(Kind::Subscribed).await.0
    }

    /// Ask for picture `credit` under `subscribed`.
    async fn credit(&mut self, subscribed: &Subscribed, credit: u64, barrier: Barrier) -> u64 {
        let profile = subscribed.profile;
        let credit = Credit {
            credit_id: Positive::new(credit).unwrap(),
            view_generation: Positive::new(1).unwrap(),
            stream_generation: profile.stream_generation,
            geometry_generation: profile.geometry_generation,
            barrier,
            barrier_generation: (barrier != Barrier::Ordinary)
                .then_some(subscribed.barrier_generation),
            input_revision: None,
        };
        let request = self.request();
        self.control.send(Kind::Credit, request, &credit).await;
        let (accepted, _): (CreditOnly, _) = self.control.expect(Kind::CreditAccepted).await;
        assert_eq!(accepted.credit_id, credit.credit_id);
        request
    }

    /// The picture for a credit, and its reference.
    async fn picture(&mut self) -> (FrameMetadata, Vec<u8>, Ref) {
        let (metadata, got): (FrameMetadata, _) = self.frame.expect(Kind::FullFrame).await;
        metadata
            .check(Kind::FullFrame, got.payload.len() as u64)
            .unwrap();
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
        (metadata, got.payload, reference)
    }

    /// Decode and commit `reference` for `credit`.
    async fn settle(&mut self, credit: u64, reference: Ref) {
        let settle = Settle {
            credit_id: Positive::new(credit).unwrap(),
            reference,
        };
        for (kind, answer) in [
            (Kind::Decoded, Kind::DecodedAck),
            (Kind::Commit, Kind::CommitAck),
        ] {
            let request = self.request();
            self.control.send(kind, request, &settle).await;
            let (echo, got): (Settle, _) = self.control.expect(answer).await;
            assert_eq!(echo, settle);
            assert_eq!(got.head.request, request);
        }
    }
}

#[tokio::test]
async fn a_paired_observer_subscribes_and_commits_exact_pictures() {
    let broker = Running::start(Scripted::default()).await;
    let mut client = Client_::pair(&broker).await;
    let subscribed = client.subscribe().await;
    assert_eq!(
        (
            subscribed.profile.width.get(),
            subscribed.profile.height.get()
        ),
        (4, 2)
    );
    assert_eq!(subscribed.profile.row_stride, 12);
    for credit in 1..=3 {
        let barrier = if credit == 1 {
            Barrier::Subscribe
        } else {
            Barrier::Ordinary
        };
        let request = client.credit(&subscribed, credit, barrier).await;
        let (metadata, pixels, reference) = client.picture().await;
        assert_eq!(metadata.credit_id.get(), credit);
        assert_eq!(metadata.observer_id, client.pair.observer_id);
        assert!(metadata.age_at_header_ns < 500_000_000);
        assert_eq!(pixels, vec![metadata.source_sequence.get() as u8; 24]);
        assert!(
            metadata.source_sequence.get() >= credit,
            "never the same picture twice"
        );
        client.settle(credit, reference).await;
        let _ = request;
    }
    broker.shut().await;
}

#[tokio::test]
async fn two_observers_share_one_source() {
    let broker = Running::start(Scripted::default()).await;
    let mut first = Client_::pair(&broker).await;
    let mut second = Client_::pair(&broker).await;
    let one = first.subscribe().await;
    let two = second.subscribe().await;
    assert_eq!(one.profile, two.profile);
    for credit in 1..=3 {
        first.credit(&one, credit, Barrier::Ordinary).await;
        second.credit(&two, credit, Barrier::Ordinary).await;
        let (_, _, a) = first.picture().await;
        let (_, _, b) = second.picture().await;
        first.settle(credit, a).await;
        second.settle(credit, b).await;
    }
    let acquired = broker.source.acquired.load(Ordering::SeqCst);
    assert!(acquired <= 8, "{acquired} acquisitions for six pictures");
    broker.shut().await;
}

#[tokio::test]
async fn an_unserved_credit_expires_and_the_observer_may_credit_again() {
    let broker = Running::start(Scripted::default()).await;
    let mut client = Client_::pair(&broker).await;
    let subscribed = client.subscribe().await;
    broker.source.hang.store(true, Ordering::SeqCst);
    // The subscription's own picture is too old to serve by now.
    tokio::time::sleep(FRESHNESS).await;
    let request = client.credit(&subscribed, 1, Barrier::Ordinary).await;
    let (expired, got): (CreditOnly, _) = client.control.expect(Kind::CreditExpired).await;
    assert_eq!((expired.credit_id.get(), got.head.request), (1, request));
    client.credit(&subscribed, 2, Barrier::Ordinary).await;
    broker.shut().await;
}

#[tokio::test]
async fn an_observer_the_screen_is_hidden_from_gets_no_picture_until_it_is_shown_again() {
    let broker = Running::start(Scripted::default()).await;
    let mut client = Client_::pair(&broker).await;
    let subscribed = client.subscribe().await;
    broker.hide();

    let request = client.credit(&subscribed, 1, Barrier::Ordinary).await;
    let (expired, got): (CreditOnly, _) = client.control.expect(Kind::CreditExpired).await;
    assert_eq!((expired.credit_id.get(), got.head.request), (1, request));

    broker.show();
    client.credit(&subscribed, 2, Barrier::Ordinary).await;
    let (metadata, _, _) = client.picture().await;
    assert_eq!(metadata.credit_id.get(), 2);
    broker.shut().await;
}

#[tokio::test]
async fn a_picture_read_during_a_sign_in_is_never_shown_to_another_observer_after_it() {
    let broker = Running::start(Scripted::default()).await;
    let mut app = Client_::pair(&broker).await;
    let mut agent = Client_::pair(&broker).await;
    let shown_to_app = app.subscribe().await;
    let shown_to_agent = agent.subscribe().await;

    // The app is shown a picture read while the person signs in.
    broker.exempt.store(true, Ordering::SeqCst);
    broker.hide();
    app.credit(&shown_to_app, 1, Barrier::Ordinary).await;
    let (_, private, reference) = app.picture().await;
    app.settle(1, reference).await;
    broker.exempt.store(false, Ordering::SeqCst);
    broker.show();

    // Fresh as it still is, the agent is shown only one read since.
    agent.credit(&shown_to_agent, 1, Barrier::Ordinary).await;
    let (_, shown, _) = agent.picture().await;
    assert!(
        shown[0] > private[0],
        "picture {} read during the sign-in was shown after it",
        private[0]
    );
    broker.shut().await;
}

#[tokio::test]
async fn a_protocol_error_ends_both_lanes_with_a_fatal_error() {
    let broker = Running::start(Scripted::default()).await;
    let mut client = Client_::pair(&broker).await;
    let credit = CreditOnly {
        credit_id: Positive::new(1).unwrap(),
    };
    let request = client.request();
    client.control.send(Kind::Decoded, request, &credit).await;
    let (error, got): (ErrorMetadata, _) = client.control.expect(Kind::Error).await;
    assert_eq!(error.disposition, Disposition::PairFatal);
    assert_eq!(got.head.request, request);
    assert!(client.control.receive().await.is_none());
    assert!(client.frame.receive().await.is_none());
    broker.shut().await;
}

#[tokio::test]
async fn losing_one_lane_ends_the_other() {
    let broker = Running::start(Scripted::default()).await;
    let mut client = Client_::pair(&broker).await;
    drop(client.frame);
    assert!(client.control.receive().await.is_none());
    let mut other = Client_::pair(&broker).await;
    assert!(
        other.subscribe().await.profile.width.get() == 4,
        "others are untouched"
    );
    broker.shut().await;
}

#[tokio::test]
async fn a_lane_that_never_pairs_is_closed() {
    let broker = Running::start(Scripted::default()).await;
    let mut idle = Side_::connect(&broker.control, Lane::Control).await;
    let (error, _): (ErrorMetadata, _) = idle.expect(Kind::Error).await;
    assert_eq!(error.failure.reason, Reason::ExpiredCapability);
    assert!(idle.receive().await.is_none());
    broker.shut().await;
}

#[tokio::test]
async fn a_wrong_capability_earns_nothing() {
    let broker = Running::start(Scripted::default()).await;
    let mut control = Side_::connect(&broker.control, Lane::Control).await;
    let hello = Hello {
        protocol_major: 1,
        service_instance: broker.service.clone(),
        bootstrap_capability: Hex64::random(),
        client_challenge: Hex32::random(),
        role: Role::Observer,
        client_kind: ClientKind::Swift,
        requested_rights: vec![Right::Observe],
        offered_updates: vec![UpdateKind::Full],
    };
    control.send(Kind::Hello, 1, &hello).await;
    let (error, _): (ErrorMetadata, _) = control.expect(Kind::Error).await;
    assert_eq!(error.failure.reason, Reason::ExpiredCapability);
    assert!(control.receive().await.is_none());
    broker.shut().await;
}

#[tokio::test]
async fn shutting_down_gives_the_source_back() {
    let broker = Running::start(Scripted::default()).await;
    let mut client = Client_::pair(&broker).await;
    client.subscribe().await;
    let stopped = broker.source.stopped.clone();
    broker.shut().await;
    assert!(stopped.load(Ordering::SeqCst) >= 1);
}

#[tokio::test]
async fn only_committed_pictures_are_witnessed() {
    let broker = Running::start(Scripted::default()).await;
    let mut client = Client_::pair(&broker).await;
    let subscribed = client.subscribe().await;
    client.credit(&subscribed, 1, Barrier::Subscribe).await;
    let (_, _, reference) = client.picture().await;
    assert!(broker.witnessed.lock().unwrap().is_empty());
    let key = reference.frame_key.as_str().to_owned();
    client.settle(1, reference).await;
    assert_eq!(*broker.witnessed.lock().unwrap(), [(key, 0)]);
    broker.shut().await;
}

#[tokio::test]
async fn a_new_session_retires_old_credits_and_asks_for_a_new_subscription() {
    let broker = Running::start(Scripted::default()).await;
    let mut client = Client_::pair(&broker).await;
    let first = client.subscribe().await;
    client.credit(&first, 1, Barrier::Subscribe).await;
    let (_, _, reference) = client.picture().await;
    client.settle(1, reference).await;

    broker.source.session.store(7, Ordering::SeqCst);
    broker.source.hang.store(true, Ordering::SeqCst);
    let stale = client.credit(&first, 2, Barrier::Resume).await;
    broker.source.hang.store(false, Ordering::SeqCst);
    let (state, _): (State, _) = client.control.expect(Kind::State).await;
    assert_eq!(state.phase, Phase::ProfileChanged);
    let profile = state.profile.unwrap();
    assert_eq!(
        profile.stream_generation.get(),
        first.profile.stream_generation.get() + 1
    );
    let (error, got): (ErrorMetadata, _) = client.control.expect(Kind::Error).await;
    assert_eq!(error.disposition, Disposition::CreditRetired);
    assert_eq!(got.head.request, stale);

    let second = client.subscribe_for(SubscribeReason::Reconfigure).await;
    assert_eq!(second.profile, profile);
    client.credit(&second, 3, Barrier::Reconfigure).await;
    let (metadata, _, _) = client.picture().await;
    assert_eq!(metadata.stream_generation, profile.stream_generation);
    broker.shut().await;
}

#[tokio::test]
async fn subscribed_observers_hear_once_when_the_source_fails_and_when_it_works_again() {
    let broker = Running::start(Scripted::default()).await;
    let mut client = Client_::pair(&broker).await;
    let subscribed = client.subscribe().await;
    *broker.source.fail.lock().unwrap() = Some(Reason::SourceStale);
    tokio::time::sleep(FRESHNESS).await;
    client.credit(&subscribed, 1, Barrier::Ordinary).await;
    let (state, _): (State, _) = client.control.expect(Kind::State).await;
    assert_eq!(state.phase, Phase::Unavailable);
    assert_eq!(
        state.failure.map(|failure| failure.reason),
        Some(Reason::SourceStale)
    );
    let (expired, _): (CreditOnly, _) = client.control.expect(Kind::CreditExpired).await;
    assert_eq!(expired.credit_id.get(), 1);

    *broker.source.fail.lock().unwrap() = None;
    client.credit(&subscribed, 2, Barrier::Ordinary).await;
    let (state, _): (State, _) = client.control.expect(Kind::State).await;
    assert_eq!(state.phase, Phase::Live);
    let (metadata, _, _) = client.picture().await;
    assert_eq!(metadata.credit_id.get(), 2);
    broker.shut().await;
}

#[tokio::test]
async fn a_subscription_whose_first_picture_fails_is_refused() {
    let source = Scripted::default();
    *source.fail.lock().unwrap() = Some(Reason::BackendInUse);
    let broker = Running::start(source).await;
    let mut client = Client_::pair(&broker).await;
    let request = client.request();
    let subscribe = Subscribe {
        reason: SubscribeReason::First,
        demand: Demand {
            desired_longest: Longest::new(1600).unwrap(),
            backend: Source::Cg,
            hard_ceiling: None,
        },
        view_generation: Positive::new(1).unwrap(),
    };
    client
        .control
        .send(Kind::Subscribe, request, &subscribe)
        .await;
    let (error, got): (ErrorMetadata, _) = client.control.expect(Kind::Error).await;
    assert_eq!(error.failure.reason, Reason::BackendInUse);
    assert_eq!(got.head.request, request);
    broker.shut().await;
}

#[tokio::test]
async fn a_credit_after_a_profile_change_is_retired_and_the_pair_lives_on() {
    let broker = Running::start(Scripted::default()).await;
    let mut client = Client_::pair(&broker).await;
    let first = client.subscribe().await;
    client.credit(&first, 1, Barrier::Subscribe).await;
    let (_, _, reference) = client.picture().await;
    client.settle(1, reference).await;
    // Another observer's demand finds the new session first.
    broker.source.session.store(9, Ordering::SeqCst);
    let mut other = Client_::pair(&broker).await;
    other.subscribe().await;
    let (state, _): (State, _) = client.control.expect(Kind::State).await;
    assert_eq!(state.phase, Phase::ProfileChanged);
    let credit = Credit {
        credit_id: Positive::new(2).unwrap(),
        view_generation: Positive::new(1).unwrap(),
        stream_generation: first.profile.stream_generation,
        geometry_generation: first.profile.geometry_generation,
        barrier: Barrier::Ordinary,
        barrier_generation: None,
        input_revision: None,
    };
    let request = client.request();
    client.control.send(Kind::Credit, request, &credit).await;
    let (error, _): (ErrorMetadata, _) = client.control.expect(Kind::Error).await;
    assert_eq!(error.disposition, Disposition::CreditRetired);
    let second = client.subscribe_for(SubscribeReason::Reconfigure).await;
    client.credit(&second, 3, Barrier::Reconfigure).await;
    client.picture().await;
    broker.shut().await;
}

#[tokio::test]
async fn a_picture_committed_after_its_freshness_names_no_input() {
    let folder = tempfile::tempdir().unwrap();
    let controller = atlas_control::Controller::at(505, &folder.path().join("state"));
    let privacy = Privacy {
        visible: Arc::new(move || controller.screen_visible().ok()),
        exempt: Arc::new(|_| false),
    };
    let now = Instant::now();
    let committed = now + Duration::from_millis(40);
    let (service, bootstrap) = (Hex32::random(), Hex64::random());
    let mut gate = Gate::new(
        service.clone(),
        bootstrap.clone(),
        vec![Right::Observe],
        vec![UpdateKind::Full],
    );
    let (client, server) = UnixStream::pair().unwrap();
    let uid = nix::unistd::getuid().as_raw();
    let welcome = gate
        .hello(
            auth::peer(&server, uid).unwrap(),
            &Hello {
                protocol_major: 1,
                service_instance: service,
                bootstrap_capability: bootstrap,
                client_challenge: Hex32::random(),
                role: Role::Observer,
                client_kind: ClientKind::Terminal,
                requested_rights: vec![Right::Observe],
                offered_updates: vec![UpdateKind::Full],
            },
            now,
        )
        .unwrap();
    let reference = Ref {
        service_instance: welcome.service_instance.clone(),
        observer_id: welcome.observer_id.clone(),
        observer_generation: welcome.observer_generation,
        stream_generation: Positive::new(1).unwrap(),
        geometry_generation: Positive::new(1).unwrap(),
        source_sequence: Positive::new(1).unwrap(),
        image_sequence: Positive::new(1).unwrap(),
        frame_key: Hex64::random(),
        view_generation: Positive::new(1).unwrap(),
    };
    let credit = Positive::new(1).unwrap();
    let mut subscription = Subscription::default();
    subscription.subscribe().unwrap();
    subscription.credit(credit, now).unwrap();
    assert_eq!(subscription.select(reference.clone(), now), Some(credit));
    subscription.delivered(credit, now);
    let budget = Budget::new(BROKER_PIXELS);
    let mut scripted = Scripted::default();
    let mut picture = scripted.acquire(&budget).await.unwrap();
    picture.at = now - Duration::from_millis(480);
    picture.visible = (privacy.visible)();
    let sent = std::sync::Arc::new(picture);
    let source = source::spawn(
        scripted,
        budget,
        Arc::new(tokio::sync::Notify::new()),
        privacy.visible.clone(),
    );
    let latest = source.answers.borrow().clone();
    let (events, _incoming) = mpsc::channel(32);
    let witnessed = Arc::new(Mutex::new(Vec::new()));
    let record = witnessed.clone();
    let mut broker = Broker {
        controller: uid,
        gate,
        source,
        latest,
        wanted: 0,
        profile: None,
        epoch: None,
        failing: false,
        witness: Arc::new(move |key, _, _| record.lock().unwrap().push(key.to_owned())),
        privacy,
        connections: HashMap::new(),
        observers: HashMap::new(),
        events,
        next: 0,
    };
    broker.accept(server, Lane::Control);
    broker.observers.insert(
        1,
        Observer {
            welcome,
            frame: None,
            half: None,
            paired: None,
            subscription,
            subscribing: None,
            subscribed: true,
            waiting: None,
            last_sequence: 1,
            sent: Some(sent),
        },
    );
    let mut client = Side_ {
        stream: client,
        decoder: Decoder::new(Lane::Control),
        pending: Vec::new(),
    };
    let settle = Settle {
        credit_id: credit,
        reference: reference.clone(),
    };
    broker
        .acknowledge(
            1,
            1,
            credit,
            Ack::Decoded(reference.clone()),
            Kind::DecodedAck,
            &settle,
            now,
        )
        .unwrap();
    client.expect::<Settle>(Kind::DecodedAck).await;
    broker
        .acknowledge(
            1,
            2,
            credit,
            Ack::Commit(reference),
            Kind::CommitAck,
            &settle,
            committed,
        )
        .unwrap();
    let (error, got): (ErrorMetadata, _) = client.expect(Kind::Error).await;
    assert_eq!(error.failure.reason, Reason::SourceStale);
    assert_eq!(error.disposition, Disposition::CreditRetired);
    assert_eq!(got.head.request, 2);
    assert!(witnessed.lock().unwrap().is_empty());

    let credit = Positive::new(2).unwrap();
    let mut reference = settle.reference;
    reference.image_sequence = credit;
    reference.source_sequence = credit;
    reference.frame_key = Hex64::random();
    let observer = broker.observers.get_mut(&1).unwrap();
    observer.subscription.credit(credit, committed).unwrap();
    assert_eq!(
        observer.subscription.select(reference.clone(), committed),
        Some(credit)
    );
    observer.subscription.delivered(credit, committed);
    let mut fresh = Scripted::default()
        .acquire(&Budget::new(BROKER_PIXELS))
        .await
        .unwrap();
    fresh.at = committed;
    fresh.visible = (broker.privacy.visible)();
    observer.sent = Some(Arc::new(fresh));
    let settle = Settle {
        credit_id: credit,
        reference: reference.clone(),
    };
    broker
        .acknowledge(
            1,
            3,
            credit,
            Ack::Decoded(reference.clone()),
            Kind::DecodedAck,
            &settle,
            committed,
        )
        .unwrap();
    client.expect::<Settle>(Kind::DecodedAck).await;
    broker
        .acknowledge(
            1,
            4,
            credit,
            Ack::Commit(reference.clone()),
            Kind::CommitAck,
            &settle,
            committed,
        )
        .unwrap();
    client.expect::<Settle>(Kind::CommitAck).await;
    assert_eq!(
        *witnessed.lock().unwrap(),
        vec![reference.frame_key.as_str()]
    );
    let Broker { source, .. } = broker;
    source.close().await;
}
