use super::*;
use crate::{authority::Hook, clock::Manual, credit::Credit, lane::LaneKind};
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncWriteExt, duplex};

fn dial(lane: u64) -> Said {
    Said::Dial {
        lane: LaneId(lane),
        kind: LaneKind::Read,
    }
}

#[tokio::test]
async fn a_stop_goes_out_ahead_of_dial_requests_queued_before_it() {
    let outbox = Outbox::default();
    outbox.say(dial(1)).unwrap();
    outbox.say(dial(2)).unwrap();

    outbox.say(Said::Stop).unwrap();
    outbox.say(Said::Cancel { lane: LaneId(1) }).unwrap();

    assert_eq!(outbox.next().await, Some(Said::Stop));
    assert_eq!(outbox.next().await, Some(Said::Cancel { lane: LaneId(1) }));
    assert_eq!(outbox.next().await, Some(dial(1)));
}

#[tokio::test]
async fn dial_requests_past_the_bound_are_refused_and_a_stop_still_goes() {
    let outbox = Outbox::default();
    for lane in 0..DIALS as u64 {
        outbox.say(dial(lane)).unwrap();
    }

    let refused = outbox.say(dial(99)).unwrap_err();
    outbox.say(Said::Stop).unwrap();

    assert_eq!(refused.code, Code::Busy);
    assert_eq!(outbox.next().await, Some(Said::Stop));
}

#[tokio::test(start_paused = true)]
async fn what_is_said_goes_out_as_lines_with_a_heartbeat_between() {
    let (near, far) = duplex(1 << 16);
    let outbox = Arc::new(Outbox::default());
    outbox.say(Said::Release).unwrap();
    let sending = tokio::spawn(send(near, outbox.clone()));
    let mut far = BufReader::new(far);

    let mut heard = Vec::new();
    for _ in 0..3 {
        heard.push(read(&mut far).await.unwrap());
    }
    outbox.close();

    assert!(heard.contains(&Said::Release));
    assert!(heard.iter().filter(|said| **said == Said::Beat).count() >= 2);
    sending.await.unwrap().unwrap();
}

#[tokio::test]
async fn a_message_too_long_or_not_understood_is_refused() {
    for line in [vec![b'x'; LINE + 2], b"{\"run\":\"rm\"}\n".to_vec()] {
        // Room for the whole line, written before anything reads it.
        let (mut near, far) = duplex(2 * LINE);
        near.write_all(&line).await.unwrap();
        drop(near);

        assert_eq!(
            read(&mut BufReader::new(far)).await.unwrap_err().code,
            Code::Protocol
        );
    }
}

#[derive(Default)]
struct Told {
    stops: AtomicUsize,
    releases: AtomicUsize,
    resumes: AtomicUsize,
    dials: Mutex<Vec<LaneId>>,
    settled: Mutex<Vec<atlas_core::files::Token>>,
}

impl Guest for Told {
    fn stop(&self) {
        self.stops.fetch_add(1, SeqCst);
    }

    fn release(&self) {
        self.releases.fetch_add(1, SeqCst);
    }

    fn resume(&self) {
        self.resumes.fetch_add(1, SeqCst);
    }

    fn dial(&self, lane: LaneId, _: LaneKind) {
        self.dials.lock().unwrap().push(lane);
    }

    fn settled(&self, changes: Vec<atlas_core::files::Token>) {
        self.settled.lock().unwrap().extend(changes);
    }
}

fn generation() -> (Authority, Lanes, Arc<Manual>) {
    let clock = Arc::new(Manual::default());
    let authority = Authority::new(Generation(5), clock.clone(), Box::new(|_, _| {}) as Hook);
    (authority.clone(), Lanes::new(authority), clock)
}

#[tokio::test]
async fn the_guest_hands_on_what_the_station_says_and_retires_when_control_ends() {
    let (authority, lanes, clock) = generation();
    let (told, credit) = (Told::default(), Credit::default());
    let (mut near, far) = duplex(1 << 16);
    for said in [
        Said::Beat,
        Said::Stop,
        Said::Release,
        dial(3),
        Said::Cancel { lane: LaneId(4) },
    ] {
        let mut line = serde_json::to_vec(&said).unwrap();
        line.push(b'\n');
        near.write_all(&line).await.unwrap();
    }
    drop(near);
    clock.advance(Duration::from_secs(1));

    guest_reads(far, &authority, &lanes, &credit, &told).await;

    assert_eq!(told.stops.load(SeqCst), 1);
    assert_eq!(told.releases.load(SeqCst), 1);
    assert_eq!(*told.dials.lock().unwrap(), [LaneId(3)]);
    assert!(lanes.cancelled(LaneId(4)));
    assert_eq!(authority.retired(), Some(Retired::ControlLost));
}

#[tokio::test]
async fn a_station_takes_nothing_before_the_guests_hello() {
    let (mut near, far) = duplex(1 << 16);
    near.write_all(b"\"beat\"\n").await.unwrap();

    assert_eq!(
        hello(&mut BufReader::new(far)).await.unwrap_err().code,
        Code::Protocol
    );
}

#[derive(Default)]
struct Watching {
    silent: AtomicUsize,
    again: AtomicUsize,
}

impl Station for Watching {
    fn silent(&self) {
        self.silent.fetch_add(1, SeqCst);
    }

    fn heard_again(&self) {
        self.again.fetch_add(1, SeqCst);
    }
}

#[tokio::test(start_paused = true)]
async fn a_guest_that_falls_silent_holds_its_agents_until_it_is_heard_again() {
    let (mut near, far) = duplex(1 << 16);
    let (watching, credit) = (Arc::new(Watching::default()), Arc::new(Credit::default()));
    let reading = {
        let (watching, credit) = (watching.clone(), credit.clone());
        tokio::spawn(async move { station_reads(BufReader::new(far), &credit, &*watching).await })
    };

    tokio::time::sleep(SILENCE + Duration::from_millis(500)).await;
    assert_eq!(watching.silent.load(SeqCst), 1);
    tokio::time::sleep(SILENCE).await;
    assert_eq!(watching.silent.load(SeqCst), 1, "told twice");

    near.write_all(b"\"beat\"\n").await.unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(watching.again.load(SeqCst), 1);

    drop(near);
    assert!(reading.await.unwrap().is_err());
}

#[tokio::test(start_paused = true)]
async fn half_a_message_when_the_silence_ends_is_not_lost() {
    let (mut near, far) = duplex(1 << 16);
    let (watching, credit) = (Arc::new(Watching::default()), Arc::new(Credit::default()));
    let reading = {
        let (watching, credit) = (watching.clone(), credit.clone());
        tokio::spawn(async move { station_reads(BufReader::new(far), &credit, &*watching).await })
    };

    near.write_all(b"{\"credit\":").await.unwrap();
    tokio::time::sleep(SILENCE + Duration::from_millis(500)).await;
    near.write_all(b"{\"lane\":1,\"bytes\":7}}\n")
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;
    drop(near);

    assert!(reading.await.unwrap().unwrap_err().code == Code::Transport);
    assert_eq!(watching.again.load(SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn the_guests_hello_goes_before_its_first_heartbeat() {
    let (near, far) = duplex(1 << 16);
    let outbox = Arc::new(Outbox::hello(
        Generation(8),
        Vec::new(),
        crate::compatibility::Declared::ours(Vec::new()),
    ));
    let sending = tokio::spawn(send(near, outbox.clone()));

    let mut far = BufReader::new(far);
    assert_eq!(hello(&mut far).await.unwrap().generation, Generation(8));
    assert_eq!(read(&mut far).await.unwrap(), Said::Beat);

    outbox.close();
    sending.await.unwrap().unwrap();
}

/// What a slow writer was given, with when.
type Written = Arc<Mutex<Vec<(tokio::time::Instant, Vec<u8>)>>>;

/// A writer that takes `pace` for each write, `beat_pace` for a
/// heartbeat's, and keeps what it was given, with when.
struct Slow {
    pace: Duration,
    beat_pace: Duration,
    waiting: Option<Pin<Box<tokio::time::Sleep>>>,
    written: Written,
}

impl Slow {
    fn new(pace: Duration, beat_pace: Duration) -> (Self, Written) {
        let written = Arc::new(Mutex::new(Vec::new()));
        let slow = Self {
            pace,
            beat_pace,
            waiting: None,
            written: written.clone(),
        };
        (slow, written)
    }
}

impl AsyncWrite for Slow {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let pace = if data.starts_with(b"\"beat\"") {
            self.beat_pace
        } else {
            self.pace
        };
        let waiting = self
            .waiting
            .get_or_insert_with(|| Box::pin(tokio::time::sleep(pace)));
        std::task::ready!(waiting.as_mut().poll(context));
        self.waiting = None;
        self.written
            .lock()
            .unwrap()
            .push((tokio::time::Instant::now(), data.to_vec()));
        Poll::Ready(Ok(data.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

fn beats_in(written: &[(tokio::time::Instant, Vec<u8>)]) -> Vec<tokio::time::Instant> {
    written
        .iter()
        .filter(|(_, line)| line.starts_with(b"\"beat\""))
        .map(|(at, _)| *at)
        .collect()
}

#[tokio::test(start_paused = true)]
async fn heartbeats_keep_their_pace_while_the_outbox_never_empties() {
    // Each message takes a tenth of a second to write, and far more wait
    // than five seconds can send: the outbox is never empty.
    let (slow, written) = Slow::new(Duration::from_millis(100), Duration::from_millis(100));
    let outbox = Arc::new(Outbox::default());
    for _ in 0..200 {
        outbox.say(Said::Release).unwrap();
    }
    let sending = tokio::spawn(send(slow, outbox.clone()));

    tokio::time::sleep(Duration::from_secs(5)).await;

    assert!(!outbox.queues().urgent.is_empty(), "the outbox emptied");
    let beats = beats_in(&written.lock().unwrap());
    assert!(beats.len() >= 4, "{} heartbeats", beats.len());
    assert!(
        beats
            .windows(2)
            .all(|pair| pair[1] - pair[0] <= HEARTBEAT + Duration::from_millis(200))
    );
    sending.abort();
}

#[tokio::test(start_paused = true)]
async fn heartbeats_slower_to_write_than_their_pace_never_keep_a_stop_waiting() {
    let (slow, written) = Slow::new(Duration::from_millis(10), HEARTBEAT * 2);
    let outbox = Arc::new(Outbox::default());
    let sending = tokio::spawn(send(slow, outbox.clone()));
    tokio::time::sleep(HEARTBEAT * 3).await;

    outbox.say(Said::Stop).unwrap();
    tokio::time::sleep(HEARTBEAT * 5).await;

    let written = written.lock().unwrap();
    assert!(
        written
            .iter()
            .any(|(_, line)| line.starts_with(b"\"stop\"")),
        "the Stop never went"
    );
    sending.abort();
}

#[tokio::test]
async fn once_sending_failed_a_stop_is_refused_rather_than_queued_for_nobody() {
    let (near, far) = duplex(1 << 16);
    drop(far);
    let outbox = Arc::new(Outbox::default());
    outbox.say(Said::Release).unwrap();

    assert!(send(near, outbox.clone()).await.is_err());

    assert_eq!(outbox.say(Said::Stop).unwrap_err().code, Code::Transport);
}

#[tokio::test(start_paused = true)]
async fn a_message_only_a_station_says_ends_a_silence_without_counting_as_heard() {
    let (mut near, far) = duplex(1 << 16);
    let (watching, credit) = (Arc::new(Watching::default()), Arc::new(Credit::default()));
    let reading = {
        let (watching, credit) = (watching.clone(), credit.clone());
        tokio::spawn(async move { station_reads(BufReader::new(far), &credit, &*watching).await })
    };
    tokio::time::sleep(SILENCE + Duration::from_millis(500)).await;

    near.write_all(b"\"stop\"\n").await.unwrap();

    assert_eq!(reading.await.unwrap().unwrap_err().code, Code::Protocol);
    assert_eq!(watching.silent.load(SeqCst), 1);
    assert_eq!(watching.again.load(SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn something_that_came_as_the_silence_ended_is_not_silence() {
    let (watching, credit) = (Watching::default(), Credit::default());
    let (mut near, far) = duplex(1 << 16);
    let reading = station_reads(BufReader::new(far), &credit, &watching);
    tokio::pin!(reading);
    // Waiting, its silence timed from now.
    assert!(
        tokio::time::timeout(Duration::ZERO, &mut reading)
            .await
            .is_err()
    );

    // A heartbeat comes before the silence ends, but the reading runs
    // again only after it.
    near.write_all(b"\"beat\"\n").await.unwrap();
    tokio::time::advance(SILENCE + Duration::from_secs(1)).await;
    assert!(
        tokio::time::timeout(Duration::ZERO, &mut reading)
            .await
            .is_err()
    );

    assert_eq!(watching.silent.load(SeqCst), 0);
    assert_eq!(watching.again.load(SeqCst), 0);
}

#[tokio::test]
async fn a_guest_reader_dropped_mid_read_retires_its_generation() {
    let (authority, lanes, _clock) = generation();
    let (told, credit) = (Told::default(), Credit::default());
    let (_near, far) = duplex(1 << 16);
    {
        let reading = guest_reads(far, &authority, &lanes, &credit, &told);
        tokio::pin!(reading);
        assert!(
            tokio::time::timeout(Duration::ZERO, &mut reading)
                .await
                .is_err()
        );
    }

    assert_eq!(authority.retired(), Some(Retired::ControlLost));
    assert!(authority.check().is_err());
}

#[tokio::test]
async fn credit_however_much_never_queues_ahead_of_a_stop_and_gathers_by_lane() {
    let outbox = Outbox::default();
    for read in 0..10_000_u64 {
        outbox
            .say(Said::Credit {
                lane: LaneId(read % 4),
                bytes: 1,
            })
            .unwrap();
    }
    outbox.say(Said::Stop).unwrap();
    outbox.say(Said::Cancel { lane: LaneId(2) }).unwrap();

    assert_eq!(outbox.next().await, Some(Said::Stop));
    assert_eq!(outbox.next().await, Some(Said::Cancel { lane: LaneId(2) }));
    let mut credits = Vec::new();
    while let Some(said) = outbox.try_next() {
        credits.push(said);
    }
    assert_eq!(credits.len(), 4);
    assert!(credits.contains(&Said::Credit {
        lane: LaneId(0),
        bytes: 2500
    }));
}

#[tokio::test(start_paused = true)]
async fn the_guests_hello_goes_first_however_late_sending_starts() {
    let (near, far) = duplex(1 << 16);
    let outbox = Arc::new(Outbox::hello(
        Generation(9),
        Vec::new(),
        crate::compatibility::Declared::ours(Vec::new()),
    ));
    let sending = send(near, outbox.clone());
    tokio::pin!(sending);

    // A long pause before sending first runs: a heartbeat is overdue.
    tokio::time::advance(HEARTBEAT * 3).await;
    let _ = tokio::time::timeout(Duration::from_millis(1), &mut sending).await;

    assert_eq!(
        hello(&mut BufReader::new(far)).await.unwrap().generation,
        Generation(9)
    );
}

#[tokio::test]
async fn a_hello_names_the_changes_the_guest_keeps_unsettled() {
    let (near, far) = duplex(1 << 16);
    let change = atlas_core::files::Token::new();
    let outbox = Arc::new(Outbox::hello(
        Generation(3),
        vec![change.clone()],
        crate::compatibility::Declared::ours(Vec::new()),
    ));
    let sending = tokio::spawn(send(near, outbox.clone()));

    let heard = hello(&mut BufReader::new(far)).await.unwrap();

    assert_eq!(heard.generation, Generation(3));
    assert_eq!(heard.unsettled, [change]);
    outbox.close();
    let _ = sending.await;
}

#[test]
fn the_station_settles_every_change_it_holds_no_claim_on() {
    let (claimed, settled) = (
        atlas_core::files::Token::new(),
        atlas_core::files::Token::new(),
    );

    let settling = settling(&[claimed.clone(), settled.clone()], |change| {
        *change == claimed
    });

    assert_eq!(settling, [settled]);
}

#[tokio::test]
async fn a_settlement_reaches_the_guest_without_waiting() {
    let (authority, lanes, _clock) = generation();
    let told = Told::default();
    let change = atlas_core::files::Token::new();
    let (mut station, guest) = duplex(1 << 16);
    let credit = Credit::default();
    let reading = guest_reads(guest, &authority, &lanes, &credit, &told);
    let saying = async {
        let mut line = serde_json::to_vec(&Said::Settled {
            changes: vec![change.clone()],
        })
        .unwrap();
        line.push(b'\n');
        station.write_all(&line).await.unwrap();
        drop(station);
    };

    tokio::join!(reading, saying);

    assert_eq!(*told.settled.lock().unwrap(), [change]);
}

#[tokio::test]
async fn a_hello_and_a_settlement_naming_every_change_a_guest_keeps_are_read_whole() {
    let named: Vec<atlas_core::files::Token> = (0..NAMED)
        .map(|_| atlas_core::files::Token::new())
        .collect();
    let (near, far) = duplex(1 << 20);
    let outbox = Arc::new(Outbox::hello(
        Generation(2),
        named.clone(),
        crate::compatibility::Declared::ours(Vec::new()),
    ));
    outbox
        .say(Said::Settled {
            changes: named.clone(),
        })
        .unwrap();
    let sending = tokio::spawn(send(near, outbox.clone()));
    let mut far = BufReader::new(far);

    assert_eq!(hello(&mut far).await.unwrap().unsettled, named);
    assert_eq!(
        read(&mut far).await.unwrap(),
        Said::Settled { changes: named }
    );
    outbox.close();
    let _ = sending.await;
}

#[test]
fn a_hello_reads_with_or_without_the_changes_it_names() {
    let change = atlas_core::files::Token::new();
    let declared = r#""protocol":1,"capabilities":[],"version":"0.1.3""#;
    let without: Said =
        serde_json::from_str(&format!(r#"{{"hello":{{"generation":4,{declared}}}}}"#)).unwrap();
    let with: Said = serde_json::from_value(serde_json::json!({"hello": {
        "generation": 4, "unsettled": [change], "protocol": 1, "capabilities": [], "version": "0.1.3",
    }}))
    .unwrap();

    let hello = |unsettled| Said::Hello {
        generation: Generation(4),
        unsettled,
        protocol: 1,
        capabilities: Vec::new(),
        version: "0.1.3".into(),
    };
    assert_eq!(without, hello(Vec::new()));
    assert_eq!(with, hello(vec![change]));
}

#[tokio::test]
async fn a_dial_naming_a_lane_kind_this_guest_does_not_know_ends_control_and_retires_it() {
    let (authority, lanes, _clock) = generation();
    let (told, credit) = (Told::default(), Credit::default());
    let (mut near, far) = duplex(1 << 16);
    near.write_all(b"{\"dial\":{\"lane\":3,\"kind\":{\"teleport\":{}}}}\n")
        .await
        .unwrap();
    // Control stays open: only the message ends it.

    guest_reads(far, &authority, &lanes, &credit, &told).await;

    assert!(told.dials.lock().unwrap().is_empty());
    assert_eq!(authority.retired(), Some(Retired::ControlLost));
    drop(near);
}
