use super::*;
use crate::{
    station::enrolment::Enrolling,
    vm::{Untrusted, tests::home},
};
use atlas_core::files::Token;
use atlas_session::{
    keys::PairingSecret,
    pairing::{GuestFacts, GuestRecords, guest_pairs},
    standin::{
        Live, StandIn,
        link::{Conditions, Connection, Link},
        serve::{Answer as Answering, Dialer},
    },
};
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

/// The station's record in memory.
#[derive(Default)]
pub(super) struct Kept(Mutex<Option<Enrolment>>);

impl StationRecords for Kept {
    fn read(&self) -> Result<Option<Enrolment>> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn replace(&self, expected: Option<&Enrolment>, record: &Enrolment) -> Result<()> {
        let mut kept = self.0.lock().unwrap();
        if kept.as_ref() != expected {
            return Err(Error::new(Code::Busy, "Changed meanwhile."));
        }
        *kept = Some(record.clone());
        Ok(())
    }
}

/// The guest's side of its pairing, in memory.
struct Held {
    committed: Mutex<Option<(PeerKey, Token)>>,
    secret: Mutex<Option<[u8; 32]>>,
}

impl GuestRecords for Held {
    fn committed(&self) -> Result<Option<(PeerKey, Token)>> {
        Ok(self.committed.lock().unwrap().clone())
    }

    fn commit(&self, station: PeerKey, change: &Token) -> Result<()> {
        let mut committed = self.committed.lock().unwrap();
        match &*committed {
            None => *committed = Some((station, change.clone())),
            Some(kept) if *kept == (station, change.clone()) => {}
            Some(_) => return Err(Error::new(Code::Identity, "Committed elsewhere.")),
        }
        Ok(())
    }

    fn secret(&self) -> Result<Option<PairingSecret>> {
        Ok(self.secret.lock().unwrap().map(PairingSecret::from_bytes))
    }

    fn forget_secret(&self) -> Result<()> {
        *self.secret.lock().unwrap() = None;
        Ok(())
    }
}

/// The enrolment in memory, whose deletion fails as many more times as its count says.
#[derive(Default)]
pub(super) struct Memory(
    pub(super) Mutex<Option<([u8; 32], Token)>>,
    pub(super) AtomicUsize,
);

impl Enrolments for Memory {
    fn keep(&self, _: &DesktopId, enrolling: &Enrolling) -> Result<()> {
        *self.0.lock().unwrap() = Some((*enrolling.secret.as_bytes(), enrolling.change.clone()));
        Ok(())
    }

    fn kept(&self, _: &DesktopId) -> Result<Option<Enrolling>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .clone()
            .map(|(secret, change)| Enrolling {
                change,
                secret: PairingSecret::from_bytes(secret),
            }))
    }

    fn forget(&self, _: &DesktopId) -> Result<bool> {
        if self
            .1
            .fetch_update(SeqCst, SeqCst, |left| left.checked_sub(1))
            .is_ok()
        {
            return Err(Error::new(Code::Identity, "The Keychain is locked."));
        }
        Ok(self.0.lock().unwrap().take().is_some())
    }
}

/// The stand-in guest as the keeper reaches it across a link: control,
/// lanes answered by the test, and pairing by the guest's own code.
pub(super) struct Guest {
    link: Arc<Link>,
    pub(super) standin: Arc<StandIn>,
    station: PeerKey,
    change: Token,
    held: Arc<dyn GuestRecords>,
    live: Arc<Mutex<Option<Live>>>,
    /// When each control dial came, and its connection.
    controls: Mutex<Vec<(tokio::time::Instant, Connection)>>,
    /// How many control dials to come fail.
    failing: AtomicUsize,
    /// While set, every dial is refused for trust on these facts.
    untrusted: Mutex<Option<Facts>>,
    facts: Mutex<Option<Facts>>,
    /// The Stop the guest's hello reports.
    pub(super) stop: Arc<Mutex<&'static str>>,
    /// The work the guest's hello reports.
    pub(super) work: Arc<Mutex<Value>>,
    /// How its lanes are answered in place of the stand-in's own hello and
    /// echo, when a test says.
    pub(super) answering: Arc<Mutex<Option<Answering>>>,
    /// What Atlas VM says of the VM's state.
    pub(super) runner: Mutex<Value>,
    /// While set, Atlas VM's state is said only once this is told, the
    /// control connection cut first when it says so.
    pub(super) runner_held: Mutex<Option<(Arc<tokio::sync::Notify>, bool)>>,
    /// How many times Atlas VM's state was asked.
    pub(super) runner_asked: AtomicUsize,
    /// While set, Atlas VM's state is asked on this socket, as the station
    /// asks it.
    pub(super) runner_socket: Mutex<Option<std::path::PathBuf>>,
    pairings: AtomicUsize,
    pub(super) held_lanes: Mutex<Vec<Dialer>>,
    connections: Mutex<Vec<Connection>>,
}

impl Guest {
    pub(super) fn controls(&self) -> Vec<tokio::time::Instant> {
        self.controls
            .lock()
            .unwrap()
            .iter()
            .map(|(when, _)| *when)
            .collect()
    }

    /// Cut the last control connection.
    pub(super) fn cut(&self) {
        if let Some((_, connection)) = self.controls.lock().unwrap().last() {
            connection.cut();
        }
    }

    fn failed() -> Refused {
        Refused::Failed(Error::new(Code::Transport, "The guest is not there."))
    }
}

impl Reach for Guest {
    fn reach(&self, purpose: Purpose) -> Reaching<'_> {
        Box::pin(async move {
            if let Some(facts) = self.untrusted.lock().unwrap().clone() {
                if purpose == Purpose::Control {
                    self.controls
                        .lock()
                        .unwrap()
                        .push((tokio::time::Instant::now(), self.link.connect().2));
                }
                return Err(Refused::Untrusted {
                    why: Untrusted::NotLaunchds,
                    facts,
                });
            }
            match purpose {
                Purpose::Control => {
                    let (station, guest, connection) = self.link.connect();
                    self.controls
                        .lock()
                        .unwrap()
                        .push((tokio::time::Instant::now(), connection));
                    if self
                        .failing
                        .fetch_update(SeqCst, SeqCst, |left| left.checked_sub(1))
                        .is_ok()
                    {
                        return Err(Self::failed());
                    }
                    let (standin, live, key) =
                        (self.standin.clone(), self.live.clone(), self.station);
                    tokio::spawn(async move {
                        if let Ok(went) = standin.control(guest, key).await {
                            *live.lock().unwrap() = Some(went);
                        }
                    });
                    Ok(Box::new(station) as Raw)
                }
                Purpose::Lane => {
                    let lanes = self
                        .live
                        .lock()
                        .unwrap()
                        .as_ref()
                        .map(|live| live.lanes.clone());
                    let lanes = lanes.ok_or_else(Self::failed)?;
                    let (stop, work) = (self.stop.clone(), self.work.clone());
                    let answering = self.answering.clone();
                    let answer: Answering = Arc::new(move |header, request| {
                        let told = answering.lock().unwrap().clone();
                        if let Some(told) = told {
                            return told(header, request);
                        }
                        if request["op"] == "hello" {
                            json!({"ok": true, "op": "hello",
                                "stop": {"state": *stop.lock().unwrap()},
                                "work": work.lock().unwrap().clone()})
                        } else {
                            json!({"ok": true, "echo": request})
                        }
                    });
                    let dialer = Dialer::new(
                        self.link.clone(),
                        self.standin.clone(),
                        lanes,
                        self.station,
                        answer,
                    );
                    let raw = dialer.dial().await.map_err(|_| Self::failed())?;
                    self.held_lanes.lock().unwrap().push(dialer);
                    Ok(raw)
                }
                Purpose::Pairing => {
                    self.pairings.fetch_add(1, SeqCst);
                    let (station, guest, connection) = self.link.connect();
                    self.connections.lock().unwrap().push(connection);
                    let (standin, change, held) =
                        (self.standin.clone(), self.change.clone(), self.held.clone());
                    tokio::spawn(async move {
                        let facts = GuestFacts {
                            uid: 502,
                            boot: "boot-1".to_owned(),
                        };
                        let _ = guest_pairs(
                            guest,
                            &standin.keys,
                            &standin.desktop,
                            &change,
                            &facts,
                            &held,
                        )
                        .await;
                    });
                    Ok(Box::new(station) as Raw)
                }
            }
        })
    }

    fn facts(&self) -> Pin<Box<dyn Future<Output = Option<Facts>> + Send + '_>> {
        Box::pin(async move { self.facts.lock().unwrap().clone() })
    }

    fn runner(
        &self,
        within: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<crate::vm::runner::Reply>> + Send + '_>> {
        Box::pin(async move {
            self.runner_asked.fetch_add(1, SeqCst);
            let socket = self.runner_socket.lock().unwrap().clone();
            if let Some(socket) = socket {
                return runner_state(move || Ok(crate::vm::runner::Runner::at(socket)), within)
                    .await;
            }
            let held = self.runner_held.lock().unwrap().clone();
            if let Some((held, cut)) = held {
                let told = held.notified();
                if cut {
                    self.cut();
                }
                told.await;
            }
            let said = self.runner.lock().unwrap().clone();
            serde_json::from_value(said)
                .map_err(|_| Error::new(Code::Transport, "Not a runner's reply."))
        })
    }
}

/// What a machine guest says on its lanes, as a test makes it say.
#[derive(Default)]
pub(super) struct Saying {
    /// Every challenge its proof was asked for, in order.
    pub(super) challenges: Mutex<Vec<String>>,
    /// The challenge its proof answers in place of the one asked.
    pub(super) replaying: Mutex<Option<String>>,
    /// Its input service's hello in place of a ready one's.
    pub(super) input: Mutex<Option<Value>>,
    /// Changes made to its hello.
    pub(super) hello: Mutex<Option<fn(&mut Value)>>,
    /// Whether its input lanes stall: open, and silent.
    pub(super) input_stalls: std::sync::atomic::AtomicBool,
    /// Every command it was asked to run, in order.
    pub(super) runs: Mutex<Vec<Value>>,
    /// What a command it runs replies, in place of one that printed
    /// `hi`.
    pub(super) ran: Mutex<Option<Value>>,
    /// How it answers anything else, in place of an echo.
    pub(super) otherwise: Mutex<Option<Answering>>,
    /// How long its proof takes, when a test says.
    pub(super) proof_takes: Mutex<Option<std::time::Duration>>,
}

/// The reply of a command a machine guest ran that printed `hi` and kept
/// what it printed.
pub(super) fn ran_hi() -> Value {
    json!({"ok": true, "exitCode": 0, "timedOut": false, "signal": null, "truncated": false,
        "events": [{"event": "started", "pid": 321}, {"event": "output", "data": "aGkK"}]})
}

pub(super) use crate::test_support::{input_hello, machine_hello, screen};

impl Guest {
    /// Answer its lanes as a machine guest that proves itself, as the
    /// `Saying` it gives says.
    pub(super) fn proving(&self) -> Arc<Saying> {
        let saying = Arc::new(Saying::default());
        let (said, stop) = (saying.clone(), self.stop.clone());
        let answer: Answering =
            Arc::new(
                move |header, request| match (&header.kind, request["op"].as_str()) {
                    (LaneKind::Input { .. }, Some("hello")) if said.input_stalls.load(SeqCst) => {
                        Value::Null
                    }
                    (LaneKind::Input { .. }, Some("hello")) => said
                        .input
                        .lock()
                        .unwrap()
                        .clone()
                        .unwrap_or_else(input_hello),
                    (LaneKind::Read, Some("hello")) => {
                        let mut hello = machine_hello(*stop.lock().unwrap());
                        if let Some(change) = *said.hello.lock().unwrap() {
                            change(&mut hello);
                        }
                        hello
                    }
                    (LaneKind::Change { .. }, Some("run")) => {
                        said.runs.lock().unwrap().push(request);
                        said.ran.lock().unwrap().clone().unwrap_or_else(ran_hi)
                    }
                    (LaneKind::Read, Some("session-proof")) => {
                        if let Some(takes) = *said.proof_takes.lock().unwrap() {
                            std::thread::sleep(takes);
                        }
                        let asked = request["challenge"].as_str().unwrap_or_default().to_owned();
                        said.challenges.lock().unwrap().push(asked.clone());
                        let answered = said.replaying.lock().unwrap().clone().unwrap_or(asked);
                        json!({"ok": true, "challenge": answered, "proof": screen()})
                    }
                    _ => match said.otherwise.lock().unwrap().clone() {
                        Some(otherwise) => otherwise(header, request),
                        None => json!({"ok": true, "echo": request}),
                    },
                },
            );
        *self.answering.lock().unwrap() = Some(answer);
        saying
    }
}

/// What Atlas VM says of a running VM, its window `window`, its picture
/// fence at `fence`, from one run of it.
pub(super) fn atlas_vm_state(window: &str, fence: u64) -> Value {
    json!({"ok": true, "op": "state", "power": "running", "confirmed": true,
        "window": window, "fence": fence,
        "incarnation": {"pid": 4242, "startSeconds": 1_700_000_000, "startMicroseconds": 5,
            "id": "6f1c0de2-run"}})
}

/// A keeper of a VM not paired yet, its enrolment kept, and its guest.
pub(super) struct Fixture {
    pub(super) keeper: Arc<Keeper>,
    pub(super) guest: Arc<Guest>,
    pub(super) kept: Arc<Kept>,
    pub(super) enrolments: Arc<Memory>,
    _state: tempfile::TempDir,
}

pub(super) fn fixture() -> Fixture {
    let standin = StandIn::new(
        DesktopId::parse("0123456789abcdef01234567").unwrap(),
        Duration::ZERO,
    );
    let own = Arc::new(Keypair::generate().unwrap());
    let secret = PairingSecret::generate().unwrap();
    let change = Token::new();
    let enrolments = Arc::new(Memory::default());
    *enrolments.0.lock().unwrap() = Some((*secret.as_bytes(), change.clone()));
    let guest = Arc::new(Guest {
        link: Arc::new(Link::new(Conditions::default())),
        standin: standin.clone(),
        station: own.public(),
        change,
        held: Arc::new(Held {
            committed: Mutex::new(None),
            secret: Mutex::new(Some(*secret.as_bytes())),
        }),
        live: Arc::default(),
        controls: Mutex::default(),
        failing: AtomicUsize::new(0),
        untrusted: Mutex::new(None),
        facts: Mutex::new(None),
        stop: Arc::new(Mutex::new("clear")),
        work: Arc::new(Mutex::new(json!({"state": "none"}))),
        answering: Arc::default(),
        runner: Mutex::new(atlas_vm_state("closed", 0)),
        runner_held: Mutex::new(None),
        runner_asked: AtomicUsize::new(0),
        runner_socket: Mutex::new(None),
        pairings: AtomicUsize::new(0),
        held_lanes: Mutex::default(),
        connections: Mutex::default(),
    });
    let state = tempfile::tempdir().unwrap();
    let vm = standin.desktop.clone();
    let kept = Arc::new(Kept::default());
    let stop = Controller::of_at(
        &atlas_core::desktop::Desktop::from(atlas_core::vm::Vm { id: vm.clone() }),
        state.path(),
    );
    let keeper = Arc::new(Keeper {
        vm,
        own,
        reach: guest.clone(),
        records: kept.clone(),
        enrolments: enrolments.clone(),
        stop,
        turn: tokio::sync::Mutex::new(()),
        applied: Mutex::default(),
        forgotten: std::sync::atomic::AtomicBool::new(false),
        folder: state.path().join("station"),
        program: Program::default(),
        instance: atlas_core::process::Instance::current(),
        handing: std::sync::atomic::AtomicBool::new(false),
        ended: tokio::sync::Notify::new(),
        observing: observe::room(),
        recovering: tokio::sync::Mutex::new(()),
        progress: Mutex::new(picture::PROGRESS),
        shared: Shared::default(),
    });
    std::fs::create_dir(&keeper.folder).unwrap();
    Fixture {
        keeper,
        guest,
        kept,
        enrolments,
        _state: state,
    }
}

/// The keeper launchd starts after `keeper`: of the same VM, over the same
/// records, guest, Stop and station folder, with nothing of its own yet.
pub(super) fn next(keeper: &Keeper) -> Arc<Keeper> {
    next_with(keeper, Program::default())
}

/// [`next`], its program `program`.
fn next_with(keeper: &Keeper, program: Program) -> Arc<Keeper> {
    Arc::new(Keeper {
        vm: keeper.vm.clone(),
        own: keeper.own.clone(),
        reach: keeper.reach.clone(),
        records: keeper.records.clone(),
        enrolments: keeper.enrolments.clone(),
        stop: keeper.stop.clone(),
        turn: tokio::sync::Mutex::new(()),
        applied: Mutex::default(),
        forgotten: std::sync::atomic::AtomicBool::new(false),
        folder: keeper.folder.clone(),
        program,
        instance: None,
        handing: std::sync::atomic::AtomicBool::new(false),
        ended: tokio::sync::Notify::new(),
        observing: observe::room(),
        recovering: tokio::sync::Mutex::new(()),
        progress: Mutex::new(picture::PROGRESS),
        shared: Shared::default(),
    })
}

/// Wait, within a minute, until `holds`.
pub(super) async fn until(what: &str, holds: impl Fn() -> bool) {
    tokio::time::timeout(Duration::from_secs(60), async {
        while !holds() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("never: {what}"));
}

fn generation(keeper: &Keeper) -> Option<Generation> {
    keeper
        .shared
        .live()
        .map(|session| session.control.generation())
}

fn reported(keeper: &Keeper) -> Option<String> {
    keeper
        .shared
        .state()
        .report
        .as_ref()
        .and_then(|report| report["stop"]["state"].as_str().map(str::to_owned))
}

#[tokio::test(start_paused = true)]
async fn a_keeper_pairs_once_and_holds_one_generation_for_many_clients() {
    let fixture = fixture();
    let running = tokio::spawn({
        let keeper = fixture.keeper.clone();
        async move { keeper.run().await }
    });
    until("live", || generation(&fixture.keeper).is_some()).await;
    let held = generation(&fixture.keeper);

    let session = fixture.keeper.shared.live().unwrap();
    let reads: Vec<_> = (0..8)
        .map(|asked| {
            let session = session.clone();
            tokio::spawn(async move {
                let mut lines = Vec::new();
                read(
                    &session,
                    LaneKind::Read,
                    &json!({"op": "echo", "asked": asked}),
                    Duration::from_secs(10),
                    |line| {
                        lines.push(line);
                        true
                    },
                )
                .await
                .map(|()| lines)
            })
        })
        .collect();
    let mut answered = Vec::new();
    for reading in reads {
        answered.push(reading.await.unwrap());
    }

    for (asked, lines) in answered.into_iter().enumerate() {
        let lines = lines.unwrap();
        let reply: Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(reply["echo"]["asked"], asked);
    }
    let hello = fixture.keeper.hello();
    assert_eq!(hello["live"], true, "{hello}");
    assert_eq!(hello["generation"], json!(held));
    assert_eq!(generation(&fixture.keeper), held);
    assert_eq!(fixture.guest.controls().len(), 1);
    assert_eq!(fixture.guest.pairings.load(SeqCst), 1);
    assert!(fixture.enrolments.0.lock().unwrap().is_none());
    // No Stop is held by the station, so none is said, and none is lifted.
    assert_eq!(fixture.guest.standin.stops.load(SeqCst), 0);
    assert_eq!(fixture.guest.standin.resumes.load(SeqCst), 0);
    running.abort();
}

#[tokio::test(start_paused = true)]
async fn a_lost_generation_is_dialled_again_with_backoff_and_pairing_is_never_repeated() {
    let fixture = fixture();
    let running = tokio::spawn({
        let keeper = fixture.keeper.clone();
        async move { keeper.run().await }
    });
    until("live", || generation(&fixture.keeper).is_some()).await;
    let first = generation(&fixture.keeper);
    fixture.guest.failing.store(2, SeqCst);

    let cut = tokio::time::Instant::now();
    fixture.guest.cut();
    until("live again", || {
        generation(&fixture.keeper).is_some_and(|now| Some(now) != first)
    })
    .await;

    let dials = fixture.guest.controls();
    assert_eq!(dials.len(), 4, "{dials:?}");
    let gaps = [dials[1] - cut, dials[2] - dials[1], dials[3] - dials[2]];
    for (gap, expected) in gaps.into_iter().zip([1, 2, 4]) {
        let expected = Duration::from_secs(expected);
        assert!(
            gap >= expected && gap < expected + Duration::from_millis(500),
            "{gaps:?}"
        );
    }
    assert_eq!(fixture.guest.pairings.load(SeqCst), 1);
    running.abort();
}

#[tokio::test(start_paused = true)]
async fn the_backoff_never_waits_longer_than_thirty_seconds() {
    let fixture = fixture();
    fixture.guest.failing.store(9, SeqCst);
    let running = tokio::spawn({
        let keeper = fixture.keeper.clone();
        async move { keeper.run().await }
    });
    // Nine waits that went nowhere: 1, 2, 4, 8, 16, then 30 each.
    tokio::time::sleep(Duration::from_secs(150)).await;

    until("live", || generation(&fixture.keeper).is_some()).await;

    let dials = fixture.guest.controls();
    let gaps: Vec<_> = dials.windows(2).map(|pair| pair[1] - pair[0]).collect();
    assert!(
        gaps.iter()
            .all(|gap| *gap < MOST + Duration::from_millis(500)),
        "{gaps:?}"
    );
    assert!(
        gaps.iter().filter(|gap| **gap >= MOST).count() >= 3,
        "{gaps:?}"
    );
    running.abort();
}

#[tokio::test(start_paused = true)]
async fn atlas_vm_not_trusted_is_never_dialled_again_until_what_it_was_judged_on_changes() {
    let fixture = fixture();
    *fixture.guest.untrusted.lock().unwrap() = Some(Facts::numbered(1));
    *fixture.guest.facts.lock().unwrap() = Some(Facts::numbered(1));
    let running = tokio::spawn({
        let keeper = fixture.keeper.clone();
        async move { keeper.run().await }
    });
    // Paired first: the enrolment is kept while the pairing dial is refused.
    until("refused", || fixture.keeper.shared.state().why.is_some()).await;
    tokio::time::sleep(Duration::from_secs(300)).await;

    let why = fixture.keeper.shared.state().why.clone().unwrap();
    assert!(why.contains(&Untrusted::NotLaunchds.to_string()), "{why}");
    assert_eq!(fixture.guest.pairings.load(SeqCst), 0);
    assert!(fixture.enrolments.0.lock().unwrap().is_some());

    *fixture.guest.untrusted.lock().unwrap() = None;
    *fixture.guest.facts.lock().unwrap() = Some(Facts::numbered(2));
    until("live", || generation(&fixture.keeper).is_some()).await;
    assert_eq!(fixture.guest.pairings.load(SeqCst), 1);
    running.abort();
}

#[tokio::test(start_paused = true)]
async fn the_guests_own_stop_is_read_on_each_new_generation() {
    let fixture = fixture();
    *fixture.guest.stop.lock().unwrap() = "stopped";
    let running = tokio::spawn({
        let keeper = fixture.keeper.clone();
        async move { keeper.run().await }
    });
    until("reported stopped", || {
        reported(&fixture.keeper).as_deref() == Some("stopped")
    })
    .await;
    let first = generation(&fixture.keeper);

    *fixture.guest.stop.lock().unwrap() = "clear";
    fixture.guest.cut();

    until("reported clear on a new generation", || {
        reported(&fixture.keeper).as_deref() == Some("clear")
            && generation(&fixture.keeper).is_some_and(|now| Some(now) != first)
    })
    .await;
    assert_eq!(fixture.guest.standin.resumes.load(SeqCst), 0);
    running.abort();
}

#[tokio::test(start_paused = true)]
async fn a_stop_the_station_holds_is_said_on_each_generation_until_the_guest_applied_it() {
    let fixture = fixture();
    fixture.keeper.stop.note_stop(true).unwrap();
    // The guest could not keep the first one: it is said again.
    fixture.guest.standin.unkept.store(1, SeqCst);
    let running = tokio::spawn({
        let keeper = fixture.keeper.clone();
        async move { keeper.run().await }
    });

    until("said twice", || {
        fixture.guest.standin.stops.load(SeqCst) == 2
    })
    .await;
    tokio::time::sleep(Duration::from_secs(60)).await;
    assert_eq!(
        fixture.guest.standin.stops.load(SeqCst),
        2,
        "said again once applied"
    );

    fixture.guest.cut();
    until("said on the new generation", || {
        fixture.guest.standin.stops.load(SeqCst) == 3
    })
    .await;
    tokio::time::sleep(Duration::from_secs(60)).await;
    assert_eq!(fixture.guest.standin.stops.load(SeqCst), 3);
    assert_eq!(
        fixture.guest.standin.resumes.load(SeqCst),
        0,
        "resumed by itself"
    );
    assert!(
        fixture.keeper.stop.stop_noted(),
        "the station's Stop was let go"
    );
    running.abort();
}

#[test]
fn a_second_keeper_of_the_same_vm_is_refused_by_the_first_ones_lock() {
    let (_home, vms) = home();
    let vm = crate::vm::tests::desktop();

    let first = hold(&vms, &vm).unwrap();
    let second = hold(&vms, &vm).unwrap_err();

    assert_eq!(second.code, Code::Busy);
    drop(first);
    assert!(hold(&vms, &vm).is_ok(), "the lock goes with its keeper");
}

#[test]
fn a_keeper_starting_while_its_vm_is_being_removed_is_refused() {
    let (_home, vms) = home();
    let vm = crate::vm::tests::desktop();
    std::fs::create_dir_all(vms.removals()).unwrap();
    std::fs::write(vms.removal(&vm), b"{}").unwrap();

    let refused = hold(&vms, &vm).unwrap_err();

    assert_eq!(refused.code, Code::State);
}

#[test]
fn a_keeper_serves_only_a_vm_its_settings_list_by_that_key() {
    let folder = tempfile::tempdir().unwrap();
    let settings = folder.path().join("config.toml");
    let vm = crate::vm::tests::desktop();
    std::fs::write(
        &settings,
        format!(
            "default = \"work\"\n\n[[desktops]]\nkind = \"vm\"\nname = \"work\"\nid = \"{}\"\n",
            vm.as_str()
        ),
    )
    .unwrap();

    assert_eq!(
        listed(&format!("vm-{}", vm.as_str()), &settings).unwrap(),
        vm
    );
    let other = DesktopId::parse("ffffffffffffffffffffffff").unwrap();
    assert!(listed(&format!("vm-{}", other.as_str()), &settings).is_err());
    assert_eq!(
        listed("user", &settings).unwrap_err().code,
        Code::BadRequest
    );
    assert!(
        listed(
            &format!("vm-{}", vm.as_str()),
            &folder.path().join("none.toml")
        )
        .is_err()
    );
}

#[test]
fn a_keepers_socket_fits_an_address_whatever_the_homes_length() {
    let vm = crate::vm::tests::desktop();

    let socket = socket_of(&vm).unwrap();

    // An address holds 104 bytes, its last one ending the path.
    assert!(socket.as_os_str().len() < 104, "{}", socket.display());
    assert_eq!(
        socket.parent().unwrap(),
        atlas_core::own_folder::sockets().unwrap()
    );
    let long_home = std::path::Path::new("/Users/alexandermorganwithaveryveryverylongname");
    let station = crate::station::Station::under(long_home).path_of(&vm);
    assert!(
        station.join("keeper.sock").as_os_str().len() > 104,
        "the station folder itself could not hold it"
    );
}

#[tokio::test(start_paused = true)]
async fn a_stop_given_while_a_generation_lives_is_said_without_a_reconnect() {
    let fixture = fixture();
    let running = tokio::spawn({
        let keeper = fixture.keeper.clone();
        async move { keeper.run().await }
    });
    until("live", || generation(&fixture.keeper).is_some()).await;
    let held = generation(&fixture.keeper);
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(fixture.guest.standin.stops.load(SeqCst), 0);

    fixture.keeper.stop.note_stop(true).unwrap();
    until("said", || fixture.guest.standin.stops.load(SeqCst) == 1).await;
    // Another Stop, a revision of its own, is said again; the same one is not.
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert_eq!(fixture.guest.standin.stops.load(SeqCst), 1);
    fixture.keeper.stop.note_stop(true).unwrap();
    until("said again", || {
        fixture.guest.standin.stops.load(SeqCst) == 2
    })
    .await;

    assert_eq!(generation(&fixture.keeper), held);
    assert_eq!(fixture.guest.controls().len(), 1);
    assert_eq!(fixture.guest.standin.resumes.load(SeqCst), 0);
    running.abort();
}

#[tokio::test(start_paused = true)]
async fn a_keeper_started_after_a_pairing_kept_deletes_the_enrolment_left_and_tries_until_it_goes()
{
    let first = fixture();
    let running = tokio::spawn({
        let keeper = first.keeper.clone();
        async move { keeper.run().await }
    });
    until("live", || generation(&first.keeper).is_some()).await;
    running.abort();
    // The keeper went after keeping the pairing, before deleting the
    // enrolment, and its deletion fails twice more.
    *first.enrolments.0.lock().unwrap() = Some(([9; 32], Token::new()));
    first.enrolments.1.store(2, SeqCst);
    let again = next(&first.keeper);
    let running = tokio::spawn({
        let keeper = again.clone();
        async move { keeper.run().await }
    });

    until("live again", || generation(&again).is_some()).await;
    tokio::time::timeout(Duration::from_secs(60), async {
        while first.enrolments.0.lock().unwrap().is_some() {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the enrolment left was deleted");
    assert_eq!(first.guest.pairings.load(SeqCst), 1, "paired again");
    assert_eq!(first.enrolments.1.load(SeqCst), 0);
    running.abort();
}

#[test]
fn a_reply_line_that_is_not_an_event_is_the_final_one() {
    assert!(!final_reply(r#"{"event": "progress", "done": 1}"#));
    assert!(final_reply(r#"{"ok": true, "op": "hello"}"#));
    assert!(final_reply(r#"{"ok": false, "event": 3}"#));
    assert!(final_reply("not json"));
}

/// The VM's station socket as Atlas VM serves it, dialled through the
/// station's own trusted dial, its listener's facts stood in for.
struct Through {
    vms: crate::vm::Vms,
    system: Arc<crate::vm::tests::Fake>,
    vm: DesktopId,
}

impl Dial for Through {
    fn dial(&self) -> Dialing<'_> {
        Box::pin(async move {
            crate::vm::station::dial_with(
                self.vms.clone(),
                self.system.clone(),
                |_| Some((std::process::id(), nix::unistd::getuid().as_raw())),
                &self.vm,
                Purpose::Lane,
            )
            .await
            .map_err(|refused| std::io::Error::other(refused.into_error(&self.vm)))
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_read_through_the_station_socket_ends_at_its_final_reply_however_the_guest_ends_the_lane()
{
    use tokio::io::AsyncReadExt;
    let home = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let vms = crate::vm::Vms::under(home.path(), nix::unistd::getuid().as_raw());
    let vm = crate::vm::tests::desktop();
    let folder = vms.folder(&vm);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::set_permissions(&folder, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
    let listener = tokio::net::UnixListener::bind(folder.join("station")).unwrap();
    let (guest, own) = (
        Keypair::generate().unwrap(),
        Arc::new(Keypair::generate().unwrap()),
    );
    let station_key = own.public();
    let named = vm.clone();
    // The guest's own way: its reply lines, then the lane dropped unfinished.
    let guest_key = guest.public();
    let serving = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut purpose = [0; 1];
        stream.read_exact(&mut purpose).await.unwrap();
        assert_eq!(purpose[0], Purpose::Lane.byte());
        let mut secure = atlas_session::secure::connect_as_guest(
            stream,
            &guest,
            station_key,
            &named,
            Purpose::Lane,
        )
        .await
        .unwrap();
        let header = atlas_session::lane::read_header(&mut secure).await.unwrap();
        assert_eq!(header.kind, LaneKind::Read);
        let mut asked = Vec::new();
        let mut byte = [0; 1];
        while byte[0] != b'\n' {
            secure.read_exact(&mut byte).await.unwrap();
            asked.push(byte[0]);
        }
        secure
            .write_all(b"{\"event\":\"progress\"}\n{\"ok\":true,\"op\":\"hello\"}\n")
            .await
            .unwrap();
        secure.flush().await.unwrap();
        String::from_utf8(asked).unwrap()
    });
    let system = Arc::new(crate::vm::tests::Fake::new());
    *system.job.lock().unwrap() = Some(atlas_core::launchd::Loaded {
        pid: Some(std::process::id()),
        ..atlas_core::launchd::Loaded::default()
    });
    let desktop = Desktop::new(
        Peers {
            desktop: vm.clone(),
            own,
            key: guest_key,
        },
        Generation(1),
        Arc::new(atlas_session::control::Outbox::default()),
        Box::new(Through { vms, system, vm }),
    );

    let opened = desktop.open(LaneKind::Read).await.unwrap();
    let mut lines = Vec::new();
    carry(opened.stream, &json!({"op": "hello"}), |line| {
        lines.push(line);
        true
    })
    .await
    .unwrap();

    assert_eq!(serving.await.unwrap(), "{\"op\":\"hello\"}\n");
    assert_eq!(
        lines,
        ["{\"event\":\"progress\"}", "{\"ok\":true,\"op\":\"hello\"}"]
    );
}

#[tokio::test(start_paused = true)]
async fn a_leftover_enrolment_is_deleted_while_atlas_vm_stays_untrusted_once_the_keychain_recovers()
{
    let fixture = fixture();
    *fixture.kept.0.lock().unwrap() =
        Some(Enrolment::Paired(atlas_core::session::paired::Paired {
            station: fixture.keeper.own.public().fingerprint(),
            guest: fixture.guest.standin.keys.public(),
            uid: 502,
            boot: "boot-1".to_owned(),
            change: Token::new(),
        }));
    *fixture.guest.untrusted.lock().unwrap() = Some(Facts::numbered(1));
    *fixture.guest.facts.lock().unwrap() = Some(Facts::numbered(1));
    fixture.enrolments.1.store(3, SeqCst);
    let running = tokio::spawn({
        let keeper = fixture.keeper.clone();
        async move { keeper.run().await }
    });

    until("refused", || fixture.keeper.shared.state().why.is_some()).await;
    tokio::time::timeout(Duration::from_secs(120), async {
        while fixture.enrolments.0.lock().unwrap().is_some() {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the enrolment left was deleted");

    assert_eq!(fixture.enrolments.1.load(SeqCst), 0);
    assert!(fixture.keeper.shared.live().is_none(), "trusted after all");
    assert_eq!(fixture.guest.pairings.load(SeqCst), 0);
    running.abort();
}

#[tokio::test(start_paused = true)]
async fn a_generation_left_on_one_way_silence_never_renews_the_guests_lease_whoever_still_holds_it()
{
    let fixture = fixture();
    let running = tokio::spawn({
        let keeper = fixture.keeper.clone();
        async move { keeper.run().await }
    });
    until("live", || generation(&fixture.keeper).is_some()).await;
    // A client's request still holds the session; no new one can be made.
    let held = fixture.keeper.shared.live().unwrap();
    fixture.guest.failing.store(usize::MAX, SeqCst);

    // The guest falls silent one way: it still hears the station's beats.
    fixture
        .guest
        .live
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .fall_silent();
    until("left", || fixture.keeper.shared.live().is_none()).await;
    tokio::time::sleep(Duration::from_secs(10)).await;

    let guest = fixture.guest.live.lock().unwrap();
    let retired = guest.as_ref().unwrap().authority.retired();
    assert!(retired.is_some(), "the guest's generation was kept alive");
    assert!(held.control.stop().is_err(), "said on a generation left");
    assert!(
        held.control
            .say(atlas_session::control::Said::Beat)
            .is_err()
    );
    drop(guest);
    running.abort();
}

#[tokio::test(start_paused = true)]
async fn a_handover_cut_short_before_its_stop_was_kept_is_finished_by_the_next_keeper() {
    let fixture = fixture();
    // The update recorded the handover, and its keeper was killed before it
    // kept the Stop.
    let from = atlas_core::process::Instance::new(41, [100, 0]);
    crate::vm::handover::Record::new(Token::new(), from)
        .keep(&fixture.keeper.folder)
        .unwrap();
    assert!(!fixture.keeper.stop.stop_noted());

    fixture.keeper.finish_handover().unwrap();
    let running = tokio::spawn({
        let keeper = fixture.keeper.clone();
        async move { keeper.run().await }
    });
    until("the Stop said", || {
        fixture.guest.standin.stops.load(SeqCst) > 0
    })
    .await;
    tokio::time::sleep(Duration::from_secs(60)).await;

    let kept = fixture.keeper.stop.stop_revision().unwrap().unwrap();
    assert_eq!(kept.why(), StopWhy::Update);
    assert_eq!(
        fixture.guest.standin.resumes.load(SeqCst),
        0,
        "nothing resumes agents"
    );
    running.abort();
}

#[tokio::test]
async fn a_stop_the_person_gave_is_kept_as_it_is_when_a_handover_cut_short_is_finished() {
    let fixture = fixture();
    crate::vm::handover::Record::new(
        Token::new(),
        atlas_core::process::Instance::new(41, [100, 0]),
    )
    .keep(&fixture.keeper.folder)
    .unwrap();
    fixture.keeper.stop.note_stop(true).unwrap();
    let given = fixture.keeper.stop.stop_revision().unwrap();

    fixture.keeper.finish_handover().unwrap();

    assert_eq!(fixture.keeper.stop.stop_revision().unwrap(), given);
    // With no handover recorded, a keeper's start notes nothing.
    let other = super::tests::fixture();
    other.keeper.finish_handover().unwrap();
    assert!(!other.keeper.stop.stop_noted());
}

#[tokio::test]
async fn a_keepers_hello_says_which_keeper_it_is_and_whether_its_program_was_replaced() {
    let fixture = fixture();

    let hello = fixture.keeper.hello();

    assert_eq!(
        hello["instance"],
        json!(atlas_core::process::Instance::current())
    );
    assert!(hello["program"].get("replaced").is_some(), "{hello}");
    assert!(hello["program"].get("code").is_some(), "{hello}");
}

#[tokio::test(start_paused = true)]
async fn a_keeper_whose_program_is_replaced_keeps_serving_and_does_nothing_until_asked() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("atlas");
    std::fs::write(&file, b"A").unwrap();
    let fixture = fixture();
    let keeper = next_with(&fixture.keeper, Program::started_from(&file));
    let running = tokio::spawn({
        let keeper = keeper.clone();
        async move { keeper.run().await }
    });
    until("live", || generation(&keeper).is_some()).await;
    let held = generation(&keeper);
    assert_eq!(keeper.hello()["program"]["replaced"], false);

    // An update puts the new build's file in its place.
    std::fs::write(folder.path().join("atlas.new"), b"B").unwrap();
    std::fs::rename(folder.path().join("atlas.new"), &file).unwrap();
    tokio::time::sleep(Duration::from_secs(60)).await;

    let hello = keeper.hello();
    assert_eq!(hello["program"]["replaced"], true, "{hello}");
    assert_eq!(hello["live"], true);
    assert_eq!(generation(&keeper), held, "the same generation serves");
    let session = keeper.shared.live().unwrap();
    let mut lines = Vec::new();
    read(
        &session,
        LaneKind::Read,
        &json!({"op": "echo"}),
        Duration::from_secs(10),
        |line| {
            lines.push(line);
            true
        },
    )
    .await
    .unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!(
        fixture.guest.standin.stops.load(SeqCst),
        0,
        "no Stop of its own"
    );
    assert_eq!(fixture.guest.standin.resumes.load(SeqCst), 0);
    assert!(!keeper.handing.load(SeqCst));
    assert_eq!(fixture.guest.controls().len(), 1);
    running.abort();
}

#[test]
fn a_keepers_start_failure_is_kept_for_the_next_status_and_forgotten_once_it_starts() {
    let folder = tempfile::tempdir().unwrap();
    let failed = Error::new(
        Code::Identity,
        "The station's key was made by another program.",
    );
    assert_eq!(start_failure(folder.path()), None);

    remember_failure(folder.path(), &failed);

    assert_eq!(
        start_failure(folder.path()).as_deref(),
        Some("The station's key was made by another program.")
    );
    forget_failure(folder.path());
    assert_eq!(start_failure(folder.path()), None);
    // A VM whose station folder is gone, as its removal leaves it, keeps none.
    let gone = folder.path().join("vm-gone");
    remember_failure(&gone, &failed);
    assert!(!gone.exists());
}
