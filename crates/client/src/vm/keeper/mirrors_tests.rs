//! A VM's mirrors through a real keeper, its guest a stand-in that listens
//! for each endpoint listed at a free port of its family on this Mac, since
//! the test's own server holds the endpoint's port here: a client connects
//! there in the endpoint's place, and the keeper carries what it accepted to
//! the real server through the very code a station runs.
use super::*;
use crate::{
    test_support::under_maintenance,
    vm::keeper::{
        change::tests::{admitted, live, proven},
        clients::tests::{Serving, asked, serving_of},
        held::at_start,
        picture::tests::{begun, picture, rest, session},
        tests::{fixture, until},
    },
    vm::lifecycle::record::{Ended, Phase},
};
use atlas_core::{files::Token, process::Instance};
use atlas_native::native::listeners::{self, Family as Socket, Listing};
use atlas_session::{
    credit::{BULK, CHUNK},
    lane::LaneKind,
};
use std::{
    io,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

/// This account's listeners as a test gives them, those never mirrored
/// named by process, and who holds a connection found as the kernel finds
/// it unless a test says nobody can be told.
#[derive(Default)]
pub(in crate::vm::keeper) struct Listed {
    pub(in crate::vm::keeper) listeners: Mutex<Vec<Listener>>,
    pub(in crate::vm::keeper) excluded: Mutex<Vec<i32>>,
    pub(in crate::vm::keeper) unknown: AtomicBool,
    /// How many listings were read.
    pub(in crate::vm::keeper) listings: AtomicUsize,
    /// While set, each listing, and each look for who holds a connection,
    /// waits at its gate.
    scanning: Mutex<Option<Arc<Gate>>>,
    holding: Mutex<Option<Arc<Gate>>>,
}

/// A gate a blocking read waits at while it is shut, counting those
/// waiting.
#[derive(Default)]
struct Gate {
    shut: Mutex<bool>,
    opened: std::sync::Condvar,
    waiting: AtomicUsize,
}

impl Gate {
    /// A gate shut until the guard given with it goes, a failing test's
    /// included, so no read is left waiting.
    fn shut() -> (Arc<Self>, Opening) {
        let gate = Arc::new(Self {
            shut: Mutex::new(true),
            ..Self::default()
        });
        (gate.clone(), Opening(gate))
    }

    fn pass(&self) {
        self.waiting.fetch_add(1, SeqCst);
        let mut shut = self.shut.lock().unwrap();
        while *shut {
            shut = self.opened.wait(shut).unwrap();
        }
        drop(shut);
        self.waiting.fetch_sub(1, SeqCst);
    }

    fn waiting(&self) -> usize {
        self.waiting.load(SeqCst)
    }
}

/// Opens its gate once it goes.
struct Opening(Arc<Gate>);

impl Drop for Opening {
    fn drop(&mut self) {
        *self.0.shut.lock().unwrap_or_else(PoisonError::into_inner) = false;
        self.0.opened.notify_all();
    }
}

/// Wait at `gate`, if one is set.
fn pass(gate: &Mutex<Option<Arc<Gate>>>) {
    let gate = gate.lock().unwrap().clone();
    if let Some(gate) = gate {
        gate.pass();
    }
}

impl Listeners for Listed {
    fn listing(&self) -> io::Result<Listing> {
        pass(&self.scanning);
        self.listings.fetch_add(1, SeqCst);
        Ok(Listing {
            listeners: self.listeners.lock().unwrap().clone(),
            partial: false,
        })
    }

    fn excluded(&self, listener: &Listener) -> bool {
        self.excluded.lock().unwrap().contains(&listener.pid)
    }

    fn held_by(&self, local: SocketAddr, peer: SocketAddr) -> io::Result<Option<Instance>> {
        pass(&self.holding);
        if self.unknown.load(SeqCst) {
            return Ok(None);
        }
        listeners::held_by(local, peer)
    }
}

/// The listener at `address`, held by this very process.
fn ours(address: SocketAddr) -> Listener {
    let me = Instance::current().unwrap();
    Listener {
        pid: me.pid,
        start: me.start,
        family: if address.is_ipv4() {
            Socket::Ipv4
        } else {
            Socket::Ipv6
        },
        bound: address.ip(),
        port: address.port(),
        v6only: address.is_ipv6(),
        socket: 0,
    }
}

/// The family and port `address` is mirrored at.
fn place(address: SocketAddr) -> (Family, u16) {
    let family = if address.is_ipv4() {
        Family::Ipv4
    } else {
        Family::Ipv6
    };
    (family, address.port())
}

/// A server on this Mac at `at` that answers each connection with what it
/// read, upper-cased, once the client finished sending; and everything it
/// ever read.
async fn shouting(at: &str) -> (SocketAddr, Arc<Mutex<Vec<u8>>>) {
    let listener = TcpListener::bind(at).await.unwrap();
    let bound = listener.local_addr().unwrap();
    let read = Arc::new(Mutex::new(Vec::new()));
    let kept = read.clone();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let kept = kept.clone();
            tokio::spawn(async move {
                let mut heard = Vec::new();
                let _ = stream.read_to_end(&mut heard).await;
                kept.lock().unwrap().extend_from_slice(&heard);
                let _ = stream.write_all(&heard.to_ascii_uppercase()).await;
            });
        }
    });
    (bound, read)
}

/// A server on this Mac that answers each read at once, upper-cased, until
/// its client ends, and is told when one does.
async fn echoing() -> (SocketAddr, Arc<tokio::sync::Notify>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let bound = listener.local_addr().unwrap();
    let ended = Arc::new(tokio::sync::Notify::new());
    let told = ended.clone();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let told = told.clone();
            tokio::spawn(async move {
                let mut said = [0_u8; 1024];
                while let Ok(read @ 1..) = stream.read(&mut said).await {
                    if stream
                        .write_all(&said[..read].to_ascii_uppercase())
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                told.notify_one();
            });
        }
    });
    (bound, ended)
}

/// A server on this Mac that keeps each connection open and reads nothing
/// of it, writing to it without end when `writes`.
async fn stalling(writes: bool) -> (SocketAddr, Arc<Mutex<Vec<TcpStream>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let bound = listener.local_addr().unwrap();
    let held = Arc::new(Mutex::new(Vec::new()));
    let kept = held.clone();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            if writes {
                let (_reading, mut writing) = stream.into_split();
                tokio::spawn(async move {
                    let _reading = _reading;
                    let chunk = vec![7_u8; CHUNK];
                    while writing.write_all(&chunk).await.is_ok() {}
                });
            } else {
                kept.lock().unwrap().push(stream);
            }
        }
    });
    (bound, held)
}

/// What a client gets back sending `said` to `address`, its own end of
/// stream passed on.
async fn shout(address: SocketAddr, said: &[u8]) -> io::Result<Vec<u8>> {
    let mut stream = TcpStream::connect(address).await?;
    stream.write_all(said).await?;
    stream.shutdown().await?;
    let mut heard = Vec::new();
    tokio::time::timeout(Duration::from_secs(20), stream.read_to_end(&mut heard))
        .await
        .map_err(|_| io::Error::other("no answer"))??;
    Ok(heard)
}

/// A picture asked for by `request` through the keeper of `serving`: its
/// first line, and every byte after it, which must come within a picture's
/// progress deadline.
async fn pictured(serving: &Serving, request: &Value) -> (Value, Vec<u8>) {
    let (mut stream, first) = begun(serving, request).await;
    let came = rest(&mut stream, crate::vm::keeper::picture::PROGRESS).await;
    (first, came)
}

/// A keeper live with its guest, this account listing `listeners`.
async fn serving_listing(listeners: Vec<Listener>) -> Serving {
    let fixture = fixture();
    *fixture.listed.listeners.lock().unwrap() = listeners;
    serving_of(fixture, true).await
}

/// Where the guest of `serving` listens in place of the endpoint at
/// `place`, once it does.
async fn mirrored_at(serving: &Serving, place: (Family, u16)) -> SocketAddr {
    let mirroring = &serving.fixture.guest.standin.mirroring;
    until("the endpoint mirrored", || mirroring.at(place).is_some()).await;
    mirroring.at(place).unwrap()
}

/// What the keeper of `serving` says of its mirrors.
fn said(serving: &Serving) -> Value {
    serving.fixture.keeper.hello()["mirrors"].clone()
}

/// The places of the list the keeper of `serving` said last.
fn said_places(serving: &Serving) -> Vec<(Family, u16)> {
    serving
        .fixture
        .guest
        .standin
        .mirroring
        .lists()
        .last()
        .map(|(_, endpoints)| endpoints.iter().map(Endpoint::place).collect())
        .unwrap_or_default()
}

/// The mirror lanes the guest of `serving` admitted, by their connection.
fn mirror_lanes(serving: &Serving) -> Vec<u64> {
    admitted(serving)
        .iter()
        .filter_map(|header| match header.kind {
            LaneKind::Mirror { connection, .. } => Some(connection),
            _ => None,
        })
        .collect()
}

/// Keep the person's maintenance of the keeper's VM at `phase`: its ID.
fn maintained(serving: &Serving, phase: Phase) -> Token {
    let keeper = &serving.fixture.keeper;
    under_maintenance(keeper.lifecycle.home(), &keeper.vm, phase)
}

/// What the keeper of `serving` answers `request` with, its one line.
async fn one(serving: &Serving, request: &Value) -> Value {
    asked(&serving.socket, request).await.remove(0)
}

/// How many connections closed with nothing carried, as the keeper of
/// `serving` says.
fn failed(serving: &Serving) -> u64 {
    said(serving)["failed"].as_u64().unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_declaring_mirrors_is_told_this_accounts_loopback_endpoints_and_one_without_is_told_nothing()
 {
    let (v4, _) = shouting("127.0.0.1:0").await;
    let (v6, _) = shouting("[::1]:0").await;
    let mirroring = serving_listing(vec![ours(v4), ours(v6)]).await;
    let without = fixture();
    *without.listed.listeners.lock().unwrap() = vec![ours(v4), ours(v6)];
    without
        .guest
        .standin
        .undeclared
        .lock()
        .unwrap()
        .push(MIRRORS.to_owned());
    let without = serving_of(without, true).await;

    let standin = &mirroring.fixture.guest.standin;
    let (list, endpoints) = standin
        .mirroring
        .listed(|endpoints| !endpoints.is_empty())
        .await;
    until("answered", || said(&mirroring)["answered"] == true).await;
    tokio::time::sleep(LISTING * 2).await;

    let places: Vec<_> = endpoints.iter().map(Endpoint::place).collect();
    let mut expected = vec![place(v4), place(v6)];
    expected.sort_by_key(|&(family, port)| (port, family));
    assert_eq!(places, expected);
    assert_eq!(list, 1, "said once the generation opened");
    assert_eq!(
        standin.mirroring.lists().len(),
        1,
        "said again only on a change"
    );
    let told = said(&mirroring);
    assert_eq!(told["served"], true, "{told}");
    assert_eq!(told["listed"].as_array().unwrap().len(), 2, "{told}");
    assert!(without.fixture.guest.standin.mirroring.lists().is_empty());
    let unserved = said(&without);
    assert_eq!(unserved["served"], false, "{unserved}");
    assert_eq!(
        unserved["why"],
        "work's Atlas cannot mirror this Mac's ports."
    );
    assert!(unserved["next"].is_string(), "{unserved}");
    assert!(!unserved.to_string().contains("helper"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_connection_the_guest_accepted_reaches_the_stations_server_and_both_ways_half_close() {
    let (server, read) = shouting("127.0.0.1:0").await;
    let serving = serving_listing(vec![ours(server)]).await;
    let at = mirrored_at(&serving, place(server)).await;

    let heard = shout(at, b"hello, mirror").await.unwrap();

    assert_eq!(
        heard, b"HELLO, MIRROR",
        "the client's end passed on, then the server's"
    );
    assert_eq!(*read.lock().unwrap(), b"hello, mirror");
    assert_eq!(mirror_lanes(&serving).len(), 1);
    until("the relay ended", || {
        serving.fixture.keeper.lanes.open_now() == 0
    })
    .await;
    assert_eq!(failed(&serving), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_list_whose_answer_was_lost_is_said_again_until_it_is_answered() {
    let (server, _) = shouting("127.0.0.1:0").await;
    let fixture = fixture();
    *fixture.listed.listeners.lock().unwrap() = vec![ours(server)];
    fixture.guest.standin.mirroring.losing.store(2, SeqCst);
    let serving = serving_of(fixture, true).await;
    let standin = &serving.fixture.guest.standin;

    until("said three times", || standin.mirroring.lists().len() == 3).await;
    until("answered", || said(&serving)["answered"] == true).await;

    let lists = standin.mirroring.lists();
    assert!(lists.iter().all(|said| *said == lists[0]), "{lists:?}");
    assert_eq!(lists[0].0, 1);
    tokio::time::sleep(RESAY + LISTING).await;
    assert_eq!(standin.mirroring.lists().len(), 3, "never once answered");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_connect_for_an_endpoint_no_longer_listed_or_of_an_old_incarnation_opens_no_lane() {
    let (server, _) = shouting("127.0.0.1:0").await;
    let (other, _) = shouting("127.0.0.1:0").await;
    let serving = serving_listing(vec![ours(server), ours(other)]).await;
    let standin = &serving.fixture.guest.standin;
    let (_, first) = standin.mirroring.listed(|listed| listed.len() == 2).await;
    let gone = *first
        .iter()
        .find(|endpoint| endpoint.port == other.port())
        .unwrap();
    let listed = *first
        .iter()
        .find(|endpoint| endpoint.port == server.port())
        .unwrap();
    *serving.fixture.listed.listeners.lock().unwrap() = vec![ours(server)];
    standin.mirroring.listed(|listed| listed.len() == 1).await;
    let old = Endpoint {
        incarnation: listed.incarnation + 7,
        ..listed
    };

    let unlisted = standin.ask_lane(gone).unwrap();
    let older = standin.ask_lane(old).unwrap();
    let current = standin.ask_lane(listed).unwrap();
    until("the listed one's lane", || {
        mirror_lanes(&serving).contains(&current)
    })
    .await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    let lanes = mirror_lanes(&serving);
    assert!(!lanes.contains(&unlisted), "{lanes:?}");
    assert!(!lanes.contains(&older), "{lanes:?}");
    assert_eq!(lanes, vec![current]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_more_than_eight_mirror_lanes_are_open_at_once() {
    let (server, _) = shouting("127.0.0.1:0").await;
    let serving = serving_listing(vec![ours(server)]).await;
    let at = mirrored_at(&serving, place(server)).await;
    let mirroring = &serving.fixture.guest.standin.mirroring;
    let mut clients = Vec::new();
    for at_most in 0..10_u8 {
        let mut client = TcpStream::connect(at).await.unwrap();
        client.write_all(&[b'a' + at_most]).await.unwrap();
        clients.push(client);
    }

    until("eight joined", || mirroring.joined() == MIRROR_LANES).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let joined = mirroring.joined();
    let mut answers = Vec::new();
    for mut client in clients {
        client.shutdown().await.unwrap();
        let mut heard = Vec::new();
        tokio::time::timeout(Duration::from_secs(20), client.read_to_end(&mut heard))
            .await
            .unwrap()
            .unwrap();
        answers.push(heard);
    }

    assert_eq!(joined, MIRROR_LANES, "the other two wait");
    assert_eq!(mirroring.most(), MIRROR_LANES);
    let expected: Vec<Vec<u8>> = (0..10_u8).map(|at| vec![b'A' + at]).collect();
    assert_eq!(answers, expected, "each served once a lane was free");
    assert_eq!(mirror_lanes(&serving).len(), 10);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn eight_stalled_mirrors_leave_four_picture_lanes_room_to_finish_within_their_progress_deadline()
 {
    let (serving, _saying, _client, _directory) = proven().await;
    let (server, _held) = stalling(false).await;
    *serving.fixture.listed.listeners.lock().unwrap() = vec![ours(server)];
    let at = mirrored_at(&serving, place(server)).await;
    let standin = serving.fixture.guest.standin.clone();
    // Each client sends without end to a server that never reads.
    let mut senders = Vec::new();
    for _ in 0..MIRROR_LANES {
        let mut client = TcpStream::connect(at).await.unwrap();
        senders.push(tokio::spawn(async move {
            let chunk = vec![1_u8; CHUNK];
            while client.write_all(&chunk).await.is_ok() {}
        }));
    }
    until("eight joined", || {
        standin.mirroring.joined() == MIRROR_LANES
    })
    .await;
    let credit = standin.bulk_credit().unwrap();
    // Stalled: what the mirrors hold of the guest's credit no longer moves.
    let mut held = BULK as usize - credit.left();
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let now = BULK as usize - credit.left();
        if now == held && now > 0 {
            break;
        }
        held = now;
    }
    let bytes: Vec<u8> = (0..1024 * 1024).map(|at| (at % 251) as u8).collect();
    let sent = bytes.clone();
    standin.pictures.set(Arc::new(move |_| Some(sent.clone())));
    let frame = json!({"op": "frame", "maxDimension": 1280, "session": session()});

    let request = picture(frame, live(&serving));
    let (one, two, three, four) = tokio::join!(
        pictured(&serving, &request),
        pictured(&serving, &request),
        pictured(&serving, &request),
        pictured(&serving, &request),
    );
    let came = [one, two, three, four];

    assert!(held <= MIRROR_LANES * CHUNK, "{held} bytes held by mirrors");
    for (first, picture) in came {
        assert_eq!(first["ok"], true, "{first}");
        assert!(picture == bytes, "a picture came whole");
    }
    assert_eq!(standin.mirroring.joined(), MIRROR_LANES, "still stalled");
    for sender in senders {
        sender.abort();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mirrors_keep_working_through_a_stop() {
    let (server, _) = shouting("127.0.0.1:0").await;
    let serving = serving_listing(vec![ours(server)]).await;
    let at = mirrored_at(&serving, place(server)).await;
    let standin = &serving.fixture.guest.standin;

    serving.fixture.keeper.stop.note_stop(true).unwrap();
    until("the guest stopped", || standin.halted.load(SeqCst)).await;
    let heard = shout(at, b"still here").await.unwrap();
    tokio::time::sleep(LISTING * 2).await;

    assert_eq!(heard, b"STILL HERE");
    assert_eq!(said_places(&serving), vec![place(server)], "still listed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_maintenance_hold_closes_every_mirror_and_returns_its_credit_before_it_acknowledges() {
    let (server, _) = stalling(true).await;
    let serving = serving_listing(vec![ours(server)]).await;
    let at = mirrored_at(&serving, place(server)).await;
    let standin = &serving.fixture.guest.standin;
    // Clients that read nothing of what the server sends them.
    let mut clients = Vec::new();
    for _ in 0..2 {
        clients.push(TcpStream::connect(at).await.unwrap());
    }
    until("both joined", || standin.mirroring.joined() == 2).await;
    let session = serving.fixture.keeper.shared.live().unwrap();
    let credit = session.control.credit().clone();
    until("the station's credit spent", || {
        credit.left() < BULK as usize
    })
    .await;
    let change = maintained(&serving, Phase::Holding);

    let held = one(&serving, &json!({"op": "hold", "change": change})).await;
    let left = credit.left();
    let open = serving.fixture.keeper.lanes.open_now();
    let lists = standin.mirroring.lists();

    assert_eq!(held["ok"], true, "{held}");
    assert_eq!(held["cancelled"], json!({"mirror": 2}), "{held}");
    assert_eq!(open, 0, "every relay ended");
    assert_eq!(left, BULK as usize, "its credit back");
    assert_eq!(
        lists.last().unwrap().1,
        Vec::new(),
        "the guest took the empty list"
    );
    assert!(standin.mirroring.at(place(server)).is_none());
    let told = said(&serving);
    assert_eq!(told["held"], true, "{told}");
    assert_eq!(told["answered"], true, "{told}");
    for mut client in clients {
        let mut rest = Vec::new();
        let _ = tokio::time::timeout(Duration::from_secs(10), client.read_to_end(&mut rest)).await;
    }
    until("the guest's credit back", || {
        standin.bulk_credit().unwrap().left() == BULK as usize
    })
    .await;
    let asked = standin.ask_lane(Endpoint {
        family: Family::Ipv4,
        port: server.port(),
        incarnation: 1,
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(asked.is_some());
    assert_eq!(mirror_lanes(&serving).len(), 2, "no lane while held");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_keeper_started_during_maintenance_lists_nothing_until_it_ends() {
    let (server, _) = shouting("127.0.0.1:0").await;
    let fixture = fixture();
    *fixture.listed.listeners.lock().unwrap() = vec![ours(server)];
    let keeper = fixture.keeper.clone();
    let change = under_maintenance(keeper.lifecycle.home(), &keeper.vm, Phase::Open);
    at_start(&keeper).await;
    let serving = serving_of(fixture, true).await;
    let standin = &serving.fixture.guest.standin;

    until("said", || !standin.mirroring.lists().is_empty()).await;
    tokio::time::sleep(LISTING * 2).await;
    let while_kept = standin.mirroring.lists();
    let listings = serving.fixture.listed.listings.load(SeqCst);
    let mut record = keeper.lifecycle.record(&keeper.vm).unwrap().unwrap();
    let maintenance = record.maintenance.as_mut().unwrap();
    maintenance.phase = Phase::Closing;
    maintenance.ended = Some(Ended {
        by: "run-1".to_owned(),
        close: Token::new(),
        earlier: None,
        earlier_gone: None,
    });
    keeper.lifecycle.keep_record(&record).unwrap();
    let released = one(&serving, &json!({"op": "release", "change": change})).await;
    record.maintenance = None;
    keeper.lifecycle.keep_record(&record).unwrap();
    let at = mirrored_at(&serving, place(server)).await;

    assert!(
        while_kept.iter().all(|(_, endpoints)| endpoints.is_empty()),
        "{while_kept:?}"
    );
    assert_eq!(listings, 0, "this account's listeners never read");
    assert_eq!(released["ok"], true, "{released}");
    assert_eq!(shout(at, b"after").await.unwrap(), b"AFTER");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mirrors_outlive_every_client_and_end_with_their_generation() {
    let (server, ended) = echoing().await;
    let serving = serving_listing(vec![ours(server)]).await;
    let at = mirrored_at(&serving, place(server)).await;
    let mut client = TcpStream::connect(at).await.unwrap();
    let mut heard = [0_u8; 1];
    client.write_all(b"a").await.unwrap();
    client.read_exact(&mut heard).await.unwrap();
    assert_eq!(&heard, b"A");

    // A client of the keeper comes and goes; the mirror goes on.
    let hello = one(&serving, &json!({"op": "hello"})).await;
    client.write_all(b"b").await.unwrap();
    client.read_exact(&mut heard).await.unwrap();
    let ending = ended.notified();
    serving.fixture.guest.cut();
    tokio::time::timeout(Duration::from_secs(20), ending)
        .await
        .expect("the server's connection ended with the generation");
    let mut rest = Vec::new();
    let closed = tokio::time::timeout(Duration::from_secs(20), client.read_to_end(&mut rest)).await;

    assert_eq!(
        hello["mirrors"]["listed"].as_array().unwrap().len(),
        1,
        "{hello}"
    );
    assert_eq!(&heard, b"B");
    assert!(closed.is_ok(), "the guest's end closed too");
    assert!(rest.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_says_which_endpoints_the_guest_could_not_listen_on() {
    let (serving, _saying, client, _directory) = proven().await;
    let (server, _) = shouting("127.0.0.1:0").await;
    let (held, _) = shouting("127.0.0.1:0").await;
    serving
        .fixture
        .guest
        .standin
        .mirroring
        .refusing
        .lock()
        .unwrap()
        .insert(place(held), Reason::InUse);
    *serving.fixture.listed.listeners.lock().unwrap() = vec![ours(server), ours(held)];
    until("answered with a refusal", || {
        said(&serving)["refused"]
            .as_array()
            .is_some_and(|refused| !refused.is_empty())
    })
    .await;

    let status = client.status().await;

    let mirrors = &status["station"]["mirrors"];
    assert_eq!(
        mirrors["refused"],
        json!([{"family": "v4", "port": held.port(), "reason": "inUse",
            "why": format!("Something in work already listens on port {0}, so this Mac's port \
                {0} does not reach it.", held.port())}]),
        "{status}"
    );
    assert_eq!(mirrors["listed"].as_array().unwrap().len(), 2, "{mirrors}");
    assert_eq!(mirrors["omitted"], json!([]));
    assert!(!mirrors.to_string().contains("helper"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn of_thirty_three_endpoints_thirty_two_are_listed_and_status_names_the_one_left_out() {
    let me = Instance::current().unwrap();
    let listener = |port: u16| Listener {
        pid: me.pid,
        start: me.start,
        family: Socket::Ipv4,
        bound: "127.0.0.1".parse().unwrap(),
        port,
        v6only: false,
        socket: u64::from(port),
    };
    let ports: Vec<u16> = (0..33).map(|at| 40_000 + at).collect();
    let serving = serving_listing(ports.iter().map(|port| listener(*port)).collect()).await;
    let standin = &serving.fixture.guest.standin;
    let (_, first) = standin.mirroring.listed(|listed| !listed.is_empty()).await;
    let omitted = said(&serving)["omitted"].clone();

    serving
        .fixture
        .listed
        .listeners
        .lock()
        .unwrap()
        .retain(|listener| listener.port != 40_005);
    let (_, after) = standin
        .mirroring
        .listed(|listed| listed.iter().any(|endpoint| endpoint.port == 40_032))
        .await;

    assert_eq!(first.len(), 32);
    assert_eq!(first.last().unwrap().port, 40_031);
    assert_eq!(
        omitted,
        json!([{"family": "v4", "port": 40_032,
            "why": "Atlas mirrors at most 32 of this Mac's ports into work, so port 40032 is left \
                out."}])
    );
    assert_eq!(after.len(), 32);
    assert!(after.iter().all(|endpoint| endpoint.port != 40_005));
    assert_eq!(said(&serving)["omitted"], json!([]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_server_replaced_on_its_port_by_an_excluded_one_after_listing_is_forwarded_nothing() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let server = listener.local_addr().unwrap();
    let serving = serving_listing(vec![ours(server)]).await;
    let at = mirrored_at(&serving, place(server)).await;
    // The listed server goes, and another process, one never mirrored,
    // takes its port before the next listing.
    drop(listener);
    let mut replaced = atlas_core::spawn::spawn_async(
        tokio::process::Command::new("/usr/bin/nc")
            .args(["-l", "127.0.0.1", &server.port().to_string()])
            .stdout(std::process::Stdio::piped())
            .kill_on_drop(true),
    )
    .unwrap();
    let pid = i32::try_from(replaced.id().unwrap()).unwrap();
    serving.fixture.listed.excluded.lock().unwrap().push(pid);
    let uid = nix::unistd::geteuid().as_raw();
    until("the other process listens", || {
        listeners::of_account(uid).is_ok_and(|listing| {
            listing
                .listeners
                .iter()
                .any(|listener| listener.pid == pid && listener.port == server.port())
        })
    })
    .await;

    let heard = shout(at, b"for the listed server only").await.unwrap();
    until("counted", || failed(&serving) == 1).await;
    let _ = replaced.start_kill();
    let mut forwarded = Vec::new();
    let mut output = replaced.stdout.take().unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(5), output.read_to_end(&mut forwarded)).await;

    assert!(heard.is_empty(), "{heard:?}");
    assert!(forwarded.is_empty(), "{forwarded:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_listed_process_whose_pid_now_names_another_is_forwarded_nothing() {
    let (server, read) = shouting("127.0.0.1:0").await;
    let mut listed = ours(server);
    listed.start = [listed.start[0].saturating_sub(60), listed.start[1]];
    let serving = serving_listing(vec![listed]).await;
    let at = mirrored_at(&serving, place(server)).await;

    let heard = shout(at, b"for the process listed").await.unwrap();
    until("counted", || failed(&serving) == 1).await;

    assert!(heard.is_empty(), "{heard:?}");
    assert!(
        read.lock().unwrap().is_empty(),
        "nothing reached the server"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_connection_whose_holder_cannot_be_told_is_closed_with_nothing_forwarded() {
    let (server, read) = shouting("127.0.0.1:0").await;
    let fixture = fixture();
    *fixture.listed.listeners.lock().unwrap() = vec![ours(server)];
    fixture.listed.unknown.store(true, SeqCst);
    let serving = serving_of(fixture, true).await;
    let at = mirrored_at(&serving, place(server)).await;

    let begun = Instant::now();
    let heard = shout(at, b"to nobody known").await.unwrap();
    until("counted", || failed(&serving) == 1).await;

    assert!(heard.is_empty(), "{heard:?}");
    assert!(
        read.lock().unwrap().is_empty(),
        "nothing reached the server"
    );
    assert!(
        begun.elapsed() >= SETTING_UP,
        "looked for while it may wait"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hold_with_no_live_generation_acknowledges_without_waiting_for_mirrors() {
    let (server, ended) = echoing().await;
    let serving = serving_listing(vec![ours(server)]).await;
    let at = mirrored_at(&serving, place(server)).await;
    let mut client = TcpStream::connect(at).await.unwrap();
    client.write_all(b"a").await.unwrap();
    let mut heard = [0_u8; 1];
    client.read_exact(&mut heard).await.unwrap();
    let ending = ended.notified();
    serving.fixture.guest.failing.store(usize::MAX, SeqCst);
    serving.fixture.guest.cut();
    tokio::time::timeout(Duration::from_secs(20), ending)
        .await
        .expect("the relay ended with its generation");
    until("no live generation", || {
        serving.fixture.keeper.shared.live().is_none()
    })
    .await;
    let change = maintained(&serving, Phase::Holding);

    let begun = Instant::now();
    let held = one(&serving, &json!({"op": "hold", "change": change})).await;

    assert_eq!(held["ok"], true, "{held}");
    assert!(
        begun.elapsed() < Duration::from_secs(2),
        "{:?}",
        begun.elapsed()
    );
    assert_eq!(said(&serving), Value::Null);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hold_on_a_guest_without_mirrors_acknowledges_without_waiting() {
    let (server, _) = shouting("127.0.0.1:0").await;
    let fixture = fixture();
    *fixture.listed.listeners.lock().unwrap() = vec![ours(server)];
    fixture
        .guest
        .standin
        .undeclared
        .lock()
        .unwrap()
        .push(MIRRORS.to_owned());
    let serving = serving_of(fixture, true).await;
    let change = maintained(&serving, Phase::Holding);

    let begun = Instant::now();
    let held = one(&serving, &json!({"op": "hold", "change": change})).await;

    assert_eq!(held["ok"], true, "{held}");
    assert!(
        begun.elapsed() < Duration::from_secs(2),
        "{:?}",
        begun.elapsed()
    );
    assert!(serving.fixture.guest.standin.mirroring.lists().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_generation_that_ends_during_a_hold_is_drained_and_the_hold_acknowledges() {
    let (server, ended) = echoing().await;
    let serving = serving_listing(vec![ours(server)]).await;
    let at = mirrored_at(&serving, place(server)).await;
    let standin = &serving.fixture.guest.standin;
    let mut client = TcpStream::connect(at).await.unwrap();
    client.write_all(b"a").await.unwrap();
    let mut heard = [0_u8; 1];
    client.read_exact(&mut heard).await.unwrap();
    // The guest takes no list from now on.
    standin.mirroring.losing.store(usize::MAX, SeqCst);
    let change = maintained(&serving, Phase::Holding);
    let socket = serving.socket.clone();
    let begun = Instant::now();
    let holding =
        tokio::spawn(async move { asked(&socket, &json!({"op": "hold", "change": change})).await });
    standin.mirroring.listed(<[Endpoint]>::is_empty).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let waiting = !holding.is_finished();

    let ending = ended.notified();
    serving.fixture.guest.cut();
    let held = holding.await.unwrap().remove(0);
    tokio::time::timeout(Duration::from_secs(20), ending)
        .await
        .expect("the relay ended with its generation");

    assert!(waiting, "the hold waits for the guest");
    assert_eq!(held["ok"], true, "{held}");
    assert_eq!(held["cancelled"], json!({"mirror": 1}), "{held}");
    assert!(begun.elapsed() < CLOSING, "{:?}", begun.elapsed());
    assert_eq!(serving.fixture.keeper.lanes.open_now(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hold_whose_empty_list_the_guest_never_takes_keeps_agents_held_and_says_so() {
    let (server, _) = shouting("127.0.0.1:0").await;
    let serving = serving_listing(vec![ours(server)]).await;
    mirrored_at(&serving, place(server)).await;
    serving
        .fixture
        .guest
        .standin
        .mirroring
        .losing
        .store(usize::MAX, SeqCst);
    let change = maintained(&serving, Phase::Holding);

    let held = one(&serving, &json!({"op": "hold", "change": change})).await;

    assert_eq!(held["code"], "not-ready", "{held}");
    assert_eq!(
        held["error"],
        "work did not say in time that it stopped mirroring this Mac's ports; agents stay held."
    );
    assert!(serving.fixture.keeper.lanes.held().is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_client_that_half_closes_then_goes_while_the_answer_waits_gives_back_its_lane_and_credit()
{
    // A server that reads the request whole, then answers without end.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut request = Vec::new();
                let _ = stream.read_to_end(&mut request).await;
                let chunk = vec![9_u8; CHUNK];
                while stream.write_all(&chunk).await.is_ok() {}
            });
        }
    });
    let serving = serving_listing(vec![ours(server)]).await;
    let at = mirrored_at(&serving, place(server)).await;
    let standin = &serving.fixture.guest.standin;
    let session = serving.fixture.keeper.shared.live().unwrap();
    let credit = session.control.credit().clone();
    let mut client = TcpStream::connect(at).await.unwrap();
    client.write_all(b"GET /").await.unwrap();
    client.shutdown().await.unwrap();
    until("the answer held up on its way", || {
        standin.mirroring.joined() == 1 && credit.left() < BULK as usize
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // The client goes without reading the answer.
    drop(client);
    until("the lane given back", || {
        serving.fixture.keeper.lanes.open_now() == 0
    })
    .await;

    assert_eq!(credit.left(), BULK as usize, "its credit back");
    until("the guest's end gone", || standin.mirroring.joined() == 0).await;
    assert_eq!(
        standin.bulk_credit().unwrap().left(),
        BULK as usize,
        "nothing owed the other way"
    );
}

/// A client of the guest whose connection to the echoing server at
/// `server` is carried now.
async fn carried(serving: &Serving, server: SocketAddr) -> TcpStream {
    let at = mirrored_at(serving, place(server)).await;
    let mut client = TcpStream::connect(at).await.unwrap();
    client.write_all(b"a").await.unwrap();
    let mut heard = [0_u8; 1];
    client.read_exact(&mut heard).await.unwrap();
    assert_eq!(&heard, b"A");
    client
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hold_empties_the_list_at_once_while_a_listing_is_stalled() {
    let (server, ended) = echoing().await;
    let serving = serving_listing(vec![ours(server)]).await;
    let _client = carried(&serving, server).await;
    let ending = ended.notified();
    let (gate, _opening) = Gate::shut();
    *serving.fixture.listed.scanning.lock().unwrap() = Some(gate.clone());
    until("a listing stalled", || gate.waiting() == 1).await;
    let change = maintained(&serving, Phase::Holding);

    let begun = Instant::now();
    let held = one(&serving, &json!({"op": "hold", "change": change})).await;
    let took = begun.elapsed();
    tokio::time::timeout(Duration::from_secs(10), ending)
        .await
        .expect("the relay ended");

    assert_eq!(held["ok"], true, "{held}");
    assert_eq!(held["cancelled"], json!({"mirror": 1}), "{held}");
    assert!(took < Duration::from_secs(5), "{took:?}");
    assert!(said_places(&serving).is_empty());
    assert_eq!(said(&serving)["held"], true);
    assert_eq!(gate.waiting(), 1, "the listing still stalled");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_maintenance_kept_while_a_listing_is_stalled_empties_the_list_and_ends_its_relays() {
    let (server, ended) = echoing().await;
    let serving = serving_listing(vec![ours(server)]).await;
    let mut client = carried(&serving, server).await;
    let ending = ended.notified();
    let (gate, _opening) = Gate::shut();
    *serving.fixture.listed.scanning.lock().unwrap() = Some(gate.clone());
    until("a listing stalled", || gate.waiting() == 1).await;

    maintained(&serving, Phase::Open);
    tokio::time::timeout(Duration::from_secs(10), ending)
        .await
        .expect("the relay ended");
    let mut rest = Vec::new();
    let closed = tokio::time::timeout(Duration::from_secs(10), client.read_to_end(&mut rest)).await;

    assert!(closed.is_ok(), "the guest's end closed");
    until("the empty list", || said_places(&serving).is_empty()).await;
    assert_eq!(said(&serving)["held"], true);
    assert_eq!(gate.waiting(), 1, "the listing still stalled");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn checks_that_stall_close_their_lanes_in_time_and_leave_no_more_reads_waiting() {
    let (server, read) = shouting("127.0.0.1:0").await;
    let serving = serving_listing(vec![ours(server)]).await;
    let at = mirrored_at(&serving, place(server)).await;
    let (gate, opening) = Gate::shut();
    *serving.fixture.listed.holding.lock().unwrap() = Some(gate.clone());

    let begun = Instant::now();
    let stalled: Vec<_> = (0..MIRROR_LANES).map(|_| shout(at, b"stalled")).collect();
    let mut heard = Vec::new();
    for shouted in stalled {
        heard.push(shouted.await.unwrap());
    }
    let took = begun.elapsed();
    until("every lane back", || {
        serving.fixture.keeper.lanes.open_now() == 0
    })
    .await;
    let waiting = gate.waiting();
    let ninth = shout(at, b"ninth").await.unwrap();
    let still = gate.waiting();
    until("counted", || failed(&serving) == 9).await;
    drop(opening);
    until("the reads went", || gate.waiting() == 0).await;
    let after = shout(at, b"after").await.unwrap();

    assert!(heard.iter().all(Vec::is_empty), "{heard:?}");
    assert!(took < Duration::from_secs(10), "{took:?}");
    assert_eq!(waiting, MIRROR_LANES, "one read per check, no more");
    assert!(ninth.is_empty());
    assert_eq!(still, MIRROR_LANES, "none piled up behind them");
    assert_eq!(after, b"AFTER");
    assert_eq!(
        *read.lock().unwrap(),
        b"after",
        "nothing reached the server before"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_connection_waiting_for_a_listener_replaced_meanwhile_is_never_forwarded_to_its_successor()
 {
    let (busy, held) = stalling(false).await;
    let (server, read) = shouting("127.0.0.1:0").await;
    let serving = serving_listing(vec![ours(busy), ours(server)]).await;
    let standin = &serving.fixture.guest.standin;
    let at_busy = mirrored_at(&serving, place(busy)).await;
    let at = mirrored_at(&serving, place(server)).await;
    let (_, first) = standin.mirroring.listed(|listed| listed.len() == 2).await;
    let before = *first
        .iter()
        .find(|endpoint| endpoint.port == server.port())
        .unwrap();
    let mut busy_clients = Vec::new();
    for _ in 0..MIRROR_LANES {
        busy_clients.push(TcpStream::connect(at_busy).await.unwrap());
    }
    until("every lane taken", || {
        standin.mirroring.joined() == MIRROR_LANES
    })
    .await;
    let mut waiting = TcpStream::connect(at).await.unwrap();
    waiting.write_all(b"for the listed one").await.unwrap();
    waiting.shutdown().await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Another socket takes the endpoint's place before the lanes free up.
    let mut successor = ours(server);
    successor.socket = 99;
    serving.fixture.listed.listeners.lock().unwrap()[1] = successor;
    let (_, after) = standin
        .mirroring
        .listed(|listed| {
            listed.iter().any(|endpoint| {
                endpoint.port == server.port() && endpoint.incarnation != before.incarnation
            })
        })
        .await;
    drop(busy_clients);
    held.lock().unwrap().clear();
    until("the lanes free", || standin.mirroring.joined() == 0).await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let reached = read.lock().unwrap().clone();
    let at_now = mirrored_at(&serving, place(server)).await;
    let fresh = shout(at_now, b"for the new one").await.unwrap();

    assert!(reached.is_empty(), "{reached:?}");
    assert_eq!(after.len(), 2);
    assert_ne!(at_now, at, "listened on afresh");
    assert_eq!(fresh, b"FOR THE NEW ONE");
    assert_eq!(*read.lock().unwrap(), b"for the new one");
}
