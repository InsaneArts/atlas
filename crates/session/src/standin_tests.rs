use super::{
    link::{BUFFERED, Conditions, Connection, Link},
    *,
};
use crate::secure::connect_as_station;
use crate::{
    keys::PairingSecret,
    pairing::{
        Enrolment, GuestFacts, GuestRecords, StationRecords, guest_pairs, station_completes,
        station_pairs,
    },
};
use atlas_core::{
    desktop::Desktop,
    error::{Code, Error},
    files::Token,
    user::User,
};
use std::sync::atomic::AtomicBool;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn desktop() -> DesktopId {
    Desktop::from(User { uid: 502 }).id()
}

/// The stand-in live in a generation over `link`, and the station's end.
async fn connected(
    link: &Link,
    standin: &Arc<StandIn>,
    station: &Keypair,
) -> (Live, Controlling, Connection) {
    let (station_end, desktop_end, connection) = link.connect();
    let (live, controlling) = tokio::join!(
        standin.control(desktop_end, station.public()),
        station_control(
            station_end,
            station,
            standin.keys.public(),
            &standin.desktop
        ),
    );
    (live.unwrap(), controlling.unwrap(), connection)
}

fn slow() -> Conditions {
    Conditions {
        delay: Duration::from_millis(200),
        bytes_per_second: Some(64 * 1024),
        partitioned: false,
    }
}

#[tokio::test(start_paused = true)]
async fn heartbeats_across_a_slow_narrow_link_keep_the_generation_live() {
    let link = Link::new(slow());
    let (standin, station) = (
        StandIn::new(desktop(), Duration::ZERO),
        Keypair::generate().unwrap(),
    );
    let (live, controlling, _connection) = connected(&link, &standin, &station).await;

    tokio::time::sleep(Duration::from_secs(20)).await;

    live.authority.check().unwrap();
    assert_eq!(controlling.heard.silent.load(SeqCst), 0);
    assert_eq!(controlling.generation, live.authority.generation());
}

#[tokio::test(start_paused = true)]
async fn a_guest_clock_far_ahead_of_the_stations_changes_nothing() {
    let link = Link::new(slow());
    let standin = StandIn::new(desktop(), Duration::from_secs(86_400 * 365));
    let station = Keypair::generate().unwrap();
    let (live, controlling, _connection) = connected(&link, &standin, &station).await;

    tokio::time::sleep(Duration::from_secs(20)).await;

    live.authority.check().unwrap();
    assert_eq!(controlling.heard.silent.load(SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn a_silent_partition_holds_the_station_and_lapses_the_guest_for_good() {
    let link = Link::new(Conditions::default());
    let (standin, station) = (
        StandIn::new(desktop(), Duration::ZERO),
        Keypair::generate().unwrap(),
    );
    let (live, controlling, _connection) = connected(&link, &standin, &station).await;
    tokio::time::sleep(Duration::from_secs(2)).await;

    link.set(Conditions {
        partitioned: true,
        ..Conditions::default()
    });
    tokio::time::sleep(Duration::from_secs(6)).await;

    assert_eq!(
        controlling.heard.silent.load(SeqCst),
        1,
        "the station held its agents"
    );
    assert_eq!(live.authority.check().unwrap_err().code, Code::Stopped);
    link.set(Conditions::default());
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(controlling.heard.again.load(SeqCst), 1);
    assert!(
        live.authority.check().is_err(),
        "a lapsed generation came back"
    );
    assert_eq!(
        *standin.retired.lock().unwrap(),
        [(live.authority.generation(), Retired::Lapsed)]
    );
}

#[tokio::test(start_paused = true)]
async fn control_failing_alone_retires_the_generation_while_a_lane_stays_open() {
    let link = Link::new(Conditions::default());
    let (standin, station) = (
        StandIn::new(desktop(), Duration::ZERO),
        Keypair::generate().unwrap(),
    );
    let (live, _controlling, control) = connected(&link, &standin, &station).await;
    let (station_end, desktop_end, _lane) = link.connect();
    let (opened, answered) = tokio::join!(
        connect_as_station(
            station_end,
            &station,
            standin.keys.public(),
            &standin.desktop,
            Purpose::Lane
        ),
        connect_as_guest(
            desktop_end,
            &standin.keys,
            station.public(),
            &standin.desktop,
            Purpose::Lane
        ),
    );
    let (mut station_lane, mut desktop_lane) = (opened.unwrap(), answered.unwrap());

    control.cut();
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert_eq!(live.authority.retired(), Some(Retired::ControlLost));
    let header = crate::lane::Header {
        generation: live.authority.generation(),
        lane: LaneId(1),
        kind: LaneKind::Read,
    };
    assert!(
        live.lanes
            .admit(&header, crate::lane::Opener::Station)
            .is_err()
    );
    // The lane itself still carries bytes: only what it may do changed.
    station_lane.write_all(b"ping").await.unwrap();
    station_lane.flush().await.unwrap();
    let mut ping = [0; 4];
    desktop_lane.read_exact(&mut ping).await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn a_cut_link_ends_control_on_both_ends() {
    let link = Link::new(Conditions::default());
    let (standin, station) = (
        StandIn::new(desktop(), Duration::ZERO),
        Keypair::generate().unwrap(),
    );
    let (live, controlling, _connection) = connected(&link, &standin, &station).await;

    link.cut();

    assert!(controlling.reading.await.unwrap().is_err());
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(live.authority.retired(), Some(Retired::ControlLost));
}

/// Open a lane across `link` between `station` and `standin`.
async fn lane(
    link: &Link,
    standin: &StandIn,
    station: &Keypair,
) -> (
    crate::secure::Secure<tokio::io::DuplexStream>,
    crate::secure::Secure<tokio::io::DuplexStream>,
    Connection,
) {
    let (station_end, desktop_end, connection) = link.connect();
    let (opened, answered) = tokio::join!(
        connect_as_station(
            station_end,
            station,
            standin.keys.public(),
            &standin.desktop,
            Purpose::Lane
        ),
        connect_as_guest(
            desktop_end,
            &standin.keys,
            station.public(),
            &standin.desktop,
            Purpose::Lane
        ),
    );
    (opened.unwrap(), answered.unwrap(), connection)
}

#[tokio::test(start_paused = true)]
async fn stop_release_and_cancel_cross_a_link_full_of_bulk_lanes_within_the_credits_bound() {
    let rate = 1 << 20;
    // A link that would hold far more than the credit: what bounds the
    // wait is the credit, not the link.
    let link = Link::buffering(
        Conditions {
            bytes_per_second: Some(rate),
            ..Conditions::default()
        },
        16 << 20,
        4 << 20,
    );
    let (standin, station) = (
        StandIn::new(desktop(), Duration::ZERO),
        Keypair::generate().unwrap(),
    );
    let (live, controlling, _control) = connected(&link, &standin, &station).await;
    // Pictures, a stage and a mirror at once, from the desktop, each as
    // fast as the shared credit lets it; the station reads them and gives
    // credit back for every read.
    let mut sending = Vec::new();
    let mut lanes = Vec::new();
    for number in 1..=3 {
        let (mut station_lane, mut desktop_lane, connection) =
            lane(&link, &standin, &station).await;
        lanes.push(connection);
        let credit = live.credit.clone();
        sending.push(tokio::spawn(async move {
            credit
                .send(LaneId(number), &mut desktop_lane, &vec![7; 64 << 20])
                .await
        }));
        let outbox = controlling.outbox.clone();
        tokio::spawn(async move {
            let mut read = vec![0; 64 * 1024];
            while let Ok(count) = station_lane.read(&mut read).await {
                if count == 0 {
                    return;
                }
                let _ = outbox.say(Said::Credit {
                    lane: LaneId(number),
                    bytes: count as u64,
                });
            }
        });
    }
    // A dial whose work is stuck on the desktop.
    standin
        .dial_gate
        .forget_permits(tokio::sync::Semaphore::MAX_PERMITS);
    say(
        &controlling,
        Said::Dial {
            lane: LaneId(9),
            kind: LaneKind::Read,
        },
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while standin.dials_begun.load(SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the dial never began");
    tokio::time::sleep(Duration::from_secs(2)).await;
    // The credit is spent and the link full, every sender still sending.
    assert!(
        live.credit.left() < 64 * 1024,
        "{} credit left",
        live.credit.left()
    );
    assert!(link.buffered() > 0);
    assert!(sending.iter().all(|sent| !sent.is_finished()));

    let credit_at_the_link = Duration::from_secs_f64(f64::from(crate::credit::BULK) / rate as f64);
    let bound = credit_at_the_link + Duration::from_millis(500);
    for (said, heard) in [
        (Said::Stop, &standin.stops),
        (Said::Release, &standin.releases),
    ] {
        let before = heard.load(SeqCst);
        // Said as the link holds a backlog of the desktop's bulk.
        tokio::time::timeout(Duration::from_secs(10), async {
            while link.buffered() < crate::credit::BULK as usize / 2 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("no backlog built up");
        let backlog = Duration::from_secs_f64(link.buffered() as f64 / rate as f64);
        let at = tokio::time::Instant::now();
        say(&controlling, said.clone());
        tokio::time::timeout(bound, async {
            while heard.load(SeqCst) == before {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("{said:?} took longer than {bound:?}"));
        // It waited behind the desktop's bulk going the other way: the
        // link is one, shared by every connection and both directions.
        assert!(
            at.elapsed() >= backlog / 2,
            "{said:?} waited {:?} behind a backlog of {backlog:?}",
            at.elapsed()
        );
    }
    say(&controlling, Said::Cancel { lane: LaneId(2) });
    tokio::time::timeout(bound, async {
        while !live.lanes.cancelled(LaneId(2)) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the cancel took too long");
    live.authority.check().unwrap();
    assert!(
        sending.iter().all(|sent| !sent.is_finished()),
        "a sender stopped"
    );
    standin.dial_gate.add_permits(1);
}

#[tokio::test(start_paused = true)]
async fn a_new_generation_retires_the_one_before_it() {
    let link = Link::new(Conditions::default());
    let (standin, station) = (
        StandIn::new(desktop(), Duration::ZERO),
        Keypair::generate().unwrap(),
    );
    let (first, _first_station, _first_connection) = connected(&link, &standin, &station).await;

    let (second, second_station, _second_connection) = connected(&link, &standin, &station).await;

    assert_eq!(first.authority.retired(), Some(Retired::Replaced));
    let header = |live: &Live| crate::lane::Header {
        generation: live.authority.generation(),
        lane: LaneId(1),
        kind: LaneKind::Read,
    };
    assert!(
        first
            .lanes
            .admit(&header(&first), crate::lane::Opener::Station)
            .is_err()
    );
    second
        .lanes
        .admit(&header(&second), crate::lane::Opener::Station)
        .unwrap();
    assert!(second_station.generation > first.authority.generation());
}

#[tokio::test(start_paused = true)]
async fn a_partition_holds_back_what_is_crossing_or_on_its_way() {
    for conditions in [
        // Waiting out its delay when the partition comes.
        Conditions {
            delay: Duration::from_secs(2),
            ..Conditions::default()
        },
        // Still leaving, slowly, when it comes.
        Conditions {
            bytes_per_second: Some(1024),
            ..Conditions::default()
        },
    ] {
        let link = Link::new(conditions);
        let (mut station_end, mut desktop_end, _connection) = link.connect();
        station_end.write_all(&[1; 1024]).await.unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;

        link.set(Conditions {
            partitioned: true,
            ..conditions
        });
        let mut piece = vec![0; 1024];
        let held =
            tokio::time::timeout(Duration::from_secs(10), desktop_end.read_exact(&mut piece)).await;
        assert!(held.is_err(), "it crossed a partition: {conditions:?}");

        link.set(conditions);
        tokio::time::timeout(Duration::from_secs(10), desktop_end.read_exact(&mut piece))
            .await
            .expect("it never crossed once the partition ended")
            .unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn a_writer_nobody_reads_comes_to_wait_rather_than_filling_the_link_without_end() {
    let link = Link::new(Conditions::default());
    let (mut station_end, _desktop_end, _connection) = link.connect();

    let written = tokio::time::timeout(
        Duration::from_secs(5),
        station_end.write_all(&vec![0; 8 << 20]),
    )
    .await;

    assert!(written.is_err(), "eight megabytes went nowhere");
    assert!(
        link.buffered() < BUFFERED,
        "it filled the link for everyone"
    );
}

#[tokio::test(start_paused = true)]
async fn a_link_that_goes_closes_every_connection_across_it() {
    let link = Link::new(Conditions::default());
    let (mut station_end, _desktop_end, _connection) = link.connect();

    drop(link);
    tokio::time::sleep(Duration::from_millis(10)).await;

    let mut byte = [0; 1];
    let read = tokio::time::timeout(Duration::from_secs(1), station_end.read(&mut byte)).await;
    assert!(matches!(read, Ok(Ok(0)) | Ok(Err(_))), "{read:?}");
}

/// A station's record in memory, whose paired write can fail once.
#[derive(Default)]
struct Kept {
    record: Mutex<Option<Enrolment>>,
    goes_before_pairing: AtomicBool,
}

impl StationRecords for Kept {
    fn read(&self) -> Result<Option<Enrolment>> {
        Ok(self.record.lock().unwrap().clone())
    }

    fn replace(&self, expected: Option<&Enrolment>, record: &Enrolment) -> Result<()> {
        if matches!(record, Enrolment::Paired(_)) && self.goes_before_pairing.swap(false, SeqCst) {
            return Err(Error::new(Code::Io, "The station went."));
        }
        let mut kept = self.record.lock().unwrap();
        if kept.as_ref() != expected {
            return Err(Error::new(Code::Busy, "Changed meanwhile."));
        }
        *kept = Some(record.clone());
        Ok(())
    }
}

/// The desktop's side of a pairing, in memory.
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

#[tokio::test(start_paused = true)]
async fn a_station_gone_mid_pairing_across_a_slow_link_completes_it_after() {
    let link = Link::new(slow());
    let (standin, station) = (
        StandIn::new(desktop(), Duration::ZERO),
        Keypair::generate().unwrap(),
    );
    let secret = PairingSecret::generate().unwrap();
    let change = Token::new();
    let facts = GuestFacts {
        uid: 501,
        boot: "boot-1".to_owned(),
    };
    let kept = Arc::new(Kept::default());
    kept.goes_before_pairing.store(true, SeqCst);
    let (records, held): (Arc<dyn StationRecords>, Arc<dyn GuestRecords>) = (
        kept.clone(),
        Arc::new(Held {
            committed: Mutex::new(None),
            secret: Mutex::new(Some(*secret.as_bytes())),
        }),
    );

    let (station_end, desktop_end, _first) = link.connect();
    let (went, committed) = tokio::join!(
        station_pairs(
            station_end,
            &station,
            &secret,
            &standin.desktop,
            &change,
            &records
        ),
        guest_pairs(
            desktop_end,
            &standin.keys,
            &standin.desktop,
            &change,
            &facts,
            &held
        ),
    );
    assert!(went.is_err());
    assert_eq!(committed.unwrap(), station.public());

    let (station_end, desktop_end, _second) = link.connect();
    let (paired, acked) = tokio::join!(
        station_completes(station_end, &station, &standin.desktop, &change, &records),
        guest_pairs(
            desktop_end,
            &standin.keys,
            &standin.desktop,
            &change,
            &facts,
            &held
        ),
    );
    assert_eq!(paired.unwrap().guest, standin.keys.public());
    assert!(acked.is_ok());
    assert!(matches!(
        &*kept.record.lock().unwrap(),
        Some(Enrolment::Paired(_))
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn control_connections_that_overlap_leave_the_newest_generation_live() {
    let link = Link::new(Conditions::default());
    let (standin, station) = (
        StandIn::new(desktop(), Duration::ZERO),
        Keypair::generate().unwrap(),
    );
    let station = Arc::new(station);
    let mut opening = Vec::new();
    for _ in 0..8 {
        let (link, standin, station) = (link.clone(), standin.clone(), station.clone());
        opening.push(tokio::spawn(async move {
            connected(&link, &standin, &station).await
        }));
    }
    let mut opened = Vec::new();
    for open in opening {
        opened.push(open.await.unwrap());
    }

    let live: Vec<_> = opened
        .iter()
        .filter(|(live, ..)| live.authority.check().is_ok())
        .map(|(live, ..)| live.authority.generation())
        .collect();
    let newest = opened
        .iter()
        .map(|(live, ..)| live.authority.generation())
        .max()
        .unwrap();
    assert_eq!(live, [newest], "exactly the newest is live");
    assert!(
        standin
            .retired
            .lock()
            .unwrap()
            .iter()
            .all(|(generation, cause)| *generation < newest && *cause == Retired::Replaced)
    );
}

#[tokio::test(start_paused = true)]
async fn a_partition_holds_back_a_delivery_waiting_on_a_full_reader() {
    let link = Link::new(Conditions::default());
    let (mut station_end, mut desktop_end, _connection) = link.connect();
    let sent: Vec<u8> = (0..48 * 1024_u32).map(|at| (at % 251) as u8).collect();
    station_end.write_all(&sent).await.unwrap();
    // The reader reads nothing: its side fills, and the next delivery
    // waits on it.
    tokio::time::sleep(Duration::from_millis(100)).await;

    link.set(Conditions {
        partitioned: true,
        ..Conditions::default()
    });
    // What was delivered before the partition, a reader's side full.
    let mut got = vec![0; 16 * 1024];
    desktop_end.read_exact(&mut got).await.unwrap();
    let mut more = [0; 1];
    let crossed = tokio::time::timeout(Duration::from_secs(1), desktop_end.read(&mut more)).await;
    assert!(
        crossed.is_err(),
        "a delivery went on through the partition: {crossed:?}"
    );

    link.set(Conditions::default());
    let mut rest = vec![0; sent.len() - got.len()];
    desktop_end.read_exact(&mut rest).await.unwrap();
    got.extend_from_slice(&rest);
    assert_eq!(got, sent, "bytes came out of order or changed");
}

#[tokio::test(start_paused = true)]
async fn a_link_with_buffers_smaller_than_a_write_still_carries_it() {
    let link = Link::buffering(Conditions::default(), 1024, 512);
    let (mut station_end, mut desktop_end, _connection) = link.connect();

    station_end.write_all(&[3; 4096]).await.unwrap();
    let mut got = [0; 4096];
    tokio::time::timeout(Duration::from_secs(5), desktop_end.read_exact(&mut got))
        .await
        .expect("a small buffer stopped the link")
        .unwrap();

    assert_eq!(got, [3; 4096]);
}

#[tokio::test(start_paused = true)]
async fn a_stuck_dial_ends_with_the_stand_in() {
    let link = Link::new(Conditions::default());
    let (standin, station) = (
        StandIn::new(desktop(), Duration::ZERO),
        Keypair::generate().unwrap(),
    );
    let gate = standin.dial_gate.clone();
    gate.forget_permits(tokio::sync::Semaphore::MAX_PERMITS);
    let (live, controlling, connection) = connected(&link, &standin, &station).await;
    say(
        &controlling,
        Said::Dial {
            lane: LaneId(9),
            kind: LaneKind::Read,
        },
    );
    while standin.dials_begun.load(SeqCst) == 0 {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    drop((live, controlling, connection, standin, link));
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert_eq!(
        Arc::strong_count(&gate),
        1,
        "the stuck dial outlived the stand-in"
    );
}

#[tokio::test(start_paused = true)]
async fn a_connection_nobody_reads_never_keeps_another_from_crossing() {
    let link = Link::new(Conditions::default());
    let (mut unread, _never_read, _first) = link.connect();
    let (mut station_end, mut desktop_end, _second) = link.connect();
    let flooding = tokio::spawn(async move {
        let _ = unread.write_all(&vec![0; 8 << 20]).await;
    });
    tokio::time::sleep(Duration::from_secs(1)).await;

    station_end.write_all(b"stop").await.unwrap();
    let mut stop = [0; 4];
    tokio::time::timeout(Duration::from_secs(1), desktop_end.read_exact(&mut stop))
        .await
        .expect("one unread connection kept another from crossing")
        .unwrap();
    assert!(!flooding.is_finished());
}

#[tokio::test(start_paused = true)]
async fn numbered_stops_and_resumes_cross_a_slow_link_and_are_answered_in_order() {
    let link = Link::new(slow());
    let (standin, station) = (
        StandIn::new(desktop(), Duration::ZERO),
        Keypair::generate().unwrap(),
    );
    let (_live, controlling, _connection) = connected(&link, &standin, &station).await;
    assert!(controlling.declared.acknowledges());

    say(&controlling, Said::stop_to(&controlling.declared, 1));
    say(&controlling, Said::resume_to(&controlling.declared, 2));
    say(&controlling, Said::stop_to(&controlling.declared, 3));

    let last = tokio::time::timeout(Duration::from_secs(10), controlling.heard.applied(3))
        .await
        .expect("an answer");
    assert_eq!(last, Outcome::Stopped);
    assert_eq!(
        *controlling.heard.applied.lock().unwrap(),
        [
            (1, Outcome::Stopped),
            (2, Outcome::Resumed),
            (3, Outcome::Stopped)
        ]
    );
    assert_eq!(standin.stops.load(SeqCst), 2);
    assert_eq!(standin.resumes.load(SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn a_replaced_generations_numbered_stop_is_never_answered_on_the_one_after_it() {
    let link = Link::new(Conditions::default());
    let (standin, station) = (
        StandIn::new(desktop(), Duration::ZERO),
        Keypair::generate().unwrap(),
    );
    let (_first, first_station, _first_connection) = connected(&link, &standin, &station).await;
    let (_second, second_station, _second_connection) = connected(&link, &standin, &station).await;

    say(&first_station, Said::Halt { seq: 1 });
    tokio::time::timeout(Duration::from_secs(10), async {
        while standin.stops.load(SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the replaced generation's Stop came");
    say(&second_station, Said::Lift { seq: 1 });

    let answered = tokio::time::timeout(Duration::from_secs(10), second_station.heard.applied(1))
        .await
        .expect("an answer");
    assert_eq!(answered, Outcome::Resumed);
    assert_eq!(
        *second_station.heard.applied.lock().unwrap(),
        [(1, Outcome::Resumed)]
    );
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(first_station.heard.applied.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_change_the_stand_in_carried_out_is_named_at_each_hello_until_the_station_settles_it() {
    let link = Link::new(Conditions::default());
    let (standin, station) = (
        StandIn::new(desktop(), Duration::ZERO),
        Keypair::generate().unwrap(),
    );
    let (done, asked) = (Token::new(), Token::new());
    assert_eq!(standin.ask(&done), Asked::Begun);
    standin.done(&done, &serde_json::json!({"ok": true}));
    assert_eq!(standin.ask(&asked), Asked::Begun);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let hooked = seen.clone();
    standin.on_settled.set(move |changes| {
        hooked.lock().unwrap().extend_from_slice(changes);
    });

    let (_live, controlling, _connection) = connected(&link, &standin, &station).await;
    assert_eq!(controlling.unsettled, std::slice::from_ref(&done));
    assert_eq!(
        standin.ask(&done),
        Asked::Again(Some(serde_json::json!({"ok": true})))
    );
    assert_eq!(
        standin.outcome(&asked).unwrap()["outcome"],
        "unknown",
        "one carried out still is unknown"
    );
    say(
        &controlling,
        Said::Settled {
            changes: vec![done.clone()],
        },
    );
    tokio::time::timeout(Duration::from_secs(10), async {
        while standin.outcome(&done).is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("settled");

    assert_eq!(*seen.lock().unwrap(), [done]);
    assert!(standin.unsettled().is_empty());
}

#[tokio::test]
async fn a_stand_in_declares_what_a_machine_guest_serving_everything_does_native_check_last() {
    let link = Link::new(Conditions::default());
    let (standin, station) = (
        StandIn::new(desktop(), Duration::ZERO),
        Keypair::generate().unwrap(),
    );

    let (_live, controlling, _connection) = connected(&link, &standin, &station).await;

    assert_eq!(
        controlling.declared.capabilities,
        crate::compatibility::MACHINE
    );
    let last: Vec<&str> = controlling.declared.capabilities[13..]
        .iter()
        .map(String::as_str)
        .collect();
    assert_eq!(
        last,
        [
            crate::compatibility::PICTURES,
            crate::compatibility::INSTALLED,
            crate::compatibility::NATIVE_CHECK
        ]
    );
}

#[test]
fn a_stand_ins_installed_read_is_answered_by_a_machine_guests_own_code() {
    let standin = StandIn::new(desktop(), Duration::ZERO);
    let asked = serde_json::json!({"op": "installed", "names": ["Google Chrome", "Firefox"]});

    let fresh = standin.installed(&asked);
    let applications = tempfile::tempdir().unwrap();
    std::fs::create_dir(applications.path().join("Firefox.app")).unwrap();
    *standin.applications.lock().unwrap() = vec![applications.path().to_owned()];
    let firefox = standin.installed(&asked);
    let refused = standin.installed(&serde_json::json!({"op": "installed", "names": [".."]}));

    assert_eq!(
        fresh["installed"],
        serde_json::json!({"Google Chrome": false, "Firefox": false}),
        "{fresh}"
    );
    assert_eq!(
        firefox["installed"],
        serde_json::json!({"Google Chrome": false, "Firefox": true})
    );
    assert_eq!(refused["code"], "bad-request", "{refused}");
}

#[test]
fn a_stand_ins_native_read_is_answered_by_a_machine_guests_own_code() {
    let canary = std::path::PathBuf::from(format!(
        "/private/tmp/atlas-canary.{}",
        &atlas_core::files::token()[..12]
    ));
    std::fs::create_dir(&canary).unwrap();
    std::fs::set_permissions(&canary, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    std::fs::write(canary.join("fixture.log"), b"launched").unwrap();
    let session = serde_json::json!({"securitySessionId": 42, "displayId": 7});

    let read = super::native(&serde_json::json!({"op": "native", "action": "files",
        "directory": canary, "session": session}));
    let refused = super::native(&serde_json::json!({"op": "native", "action": "files",
        "directory": "/private/tmp/elsewhere", "session": session}));
    let _ = std::fs::remove_dir_all(&canary);

    assert_eq!(read["files"]["fixture.log"], "bGF1bmNoZWQ=", "{read}");
    assert_eq!(refused["code"], "bad-request", "{refused}");
}
