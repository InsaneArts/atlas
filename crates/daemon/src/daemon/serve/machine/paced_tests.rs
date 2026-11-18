use super::*;
use atlas_session::{
    authority::{Authority, Retired},
    clock::Manual,
    credit::BULK,
    generation::Generation,
};
use tokio::io::{AsyncWriteExt, DuplexStream, duplex};

const LANE: LaneId = LaneId(3);

/// The lanes of a generation of their own, and its authority.
fn generation() -> (Arc<Lanes>, Authority) {
    let authority = Authority::new(
        Generation(1),
        Arc::new(Manual::default()),
        Box::new(|_, _| {}),
    );
    (Arc::new(Lanes::new(authority.clone())), authority)
}

/// Write `bytes` on `writer`, then end it.
fn writing(mut writer: DuplexStream, bytes: usize) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let _ = writer.write_all(&vec![7; bytes]).await;
    })
}

/// Read `far` to its end, giving credit back for every read after
/// `pause`: how many bytes came, and the most that were on their way, read
/// or not yet given back, at once.
fn repaying(
    mut far: DuplexStream,
    lane: LaneId,
    credit: Arc<Credit>,
    pause: Duration,
) -> tokio::task::JoinHandle<(usize, usize)> {
    tokio::spawn(async move {
        let (mut came, mut most) = (0, 0);
        let mut buffer = vec![0; CHUNK];
        while let Ok(read) = far.read(&mut buffer).await {
            if read == 0 {
                break;
            }
            came += read;
            most = most.max(BULK as usize - credit.left());
            tokio::time::sleep(pause).await;
            credit.returned(lane, read as u64);
        }
        (came, most)
    })
}

#[tokio::test]
async fn a_lane_has_at_most_two_chunks_on_their_way_however_much_it_writes() {
    let (lanes, _authority) = generation();
    let credit = Arc::new(Credit::default());
    let (to, far) = duplex(4 << 20);
    let (writer, pump) = paced(LANE, lanes, credit.clone(), to, BULK_PROGRESS);
    let all = 3 * BULK as usize / 2;

    writing(writer, all);
    let read = repaying(far, LANE, credit.clone(), Duration::from_millis(1));

    pump.finished().await.unwrap();
    let (came, most) = read.await.unwrap();
    assert_eq!(came, all, "whole");
    assert!(most <= 2 * CHUNK, "{most} on their way at once");
    assert_eq!(credit.left(), BULK as usize);
}

#[tokio::test]
async fn several_short_replies_under_withheld_credit_are_owed_until_they_are_read_back() {
    let (lanes, _authority) = generation();
    let credit = Arc::new(Credit::default());
    let (to, mut far) = duplex(1 << 20);
    let (mut writer, pump) = paced(LANE, lanes, credit.clone(), to, BULK_PROGRESS);
    let replies = [
        "{\"event\":\"data\"}\n",
        "{\"event\":\"data\"}\n",
        "{\"ok\":true}\n",
    ];
    let said: usize = replies.iter().map(|reply| reply.len()).sum();
    for reply in replies {
        writer.write_all(reply.as_bytes()).await.unwrap();
    }
    drop(writer);
    let mut came = vec![0; said];
    far.read_exact(&mut came).await.unwrap();
    let finishing = tokio::spawn(pump.finished());

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!finishing.is_finished(), "released before it was read back");
    assert_eq!(credit.left(), BULK as usize - said);

    credit.returned(LANE, said as u64);
    tokio::time::timeout(Duration::from_secs(5), finishing)
        .await
        .expect("finished once read back")
        .unwrap()
        .unwrap();
    assert_eq!(credit.left(), BULK as usize);
    assert_eq!(
        far.read(&mut [0; 1]).await.unwrap(),
        0,
        "and its lane ended"
    );
}

#[tokio::test(start_paused = true)]
async fn a_reader_that_stalls_past_the_deadline_has_its_lane_ended_its_credit_back_and_others_go_on()
 {
    let (lanes, _authority) = generation();
    let credit = Arc::new(Credit::default());
    let (to, mut stalled) = duplex(4 << 20);
    let (writer, pump) = paced(LANE, lanes.clone(), credit.clone(), to, BULK_PROGRESS);
    writing(writer, BULK as usize);
    let started = tokio::time::Instant::now();

    let ended = pump.finished().await.unwrap_err();

    assert_eq!(ended.code, Code::Transport, "{ended:?}");
    assert!(started.elapsed() >= BULK_PROGRESS);
    assert_eq!(credit.left(), BULK as usize);
    let mut sent = Vec::new();
    stalled.read_to_end(&mut sent).await.unwrap();
    assert_eq!(sent.len(), 2 * CHUNK, "closed after what went");
    assert!(lanes.cancelled(LANE), "and its lane ended with it");

    let other = LaneId(4);
    let (to, far) = duplex(4 << 20);
    let (writer, pump) = paced(other, lanes, credit.clone(), to, BULK_PROGRESS);
    writing(writer, BULK as usize);
    let read = repaying(far, other, credit.clone(), Duration::ZERO);
    pump.finished().await.unwrap();
    assert_eq!(read.await.unwrap().0, BULK as usize);
    assert_eq!(credit.left(), BULK as usize);
}

/// A lane of `lanes` whose reader took a chunk and gave nothing back,
/// ended by `end`: what its pump came to, and what its reader read then.
async fn ended_by(lanes: Arc<Lanes>, end: impl FnOnce()) -> (Result<()>, Arc<Credit>, usize) {
    let credit = Arc::new(Credit::default());
    let (to, mut far) = duplex(4 << 20);
    let (writer, pump) = paced(LANE, lanes, credit.clone(), to, BULK_PROGRESS);
    writing(writer, BULK as usize);
    let mut first = vec![0; CHUNK];
    far.read_exact(&mut first).await.unwrap();

    end();

    let pumped = tokio::time::timeout(Duration::from_secs(5), pump.finished())
        .await
        .expect("the pump ended");
    let mut rest = Vec::new();
    far.read_to_end(&mut rest).await.unwrap();
    (pumped, credit, CHUNK + rest.len())
}

#[tokio::test]
async fn a_cancelled_lane_ends_its_pump_and_owes_nothing() {
    let (lanes, _authority) = generation();

    let (pumped, credit, read) = ended_by(lanes.clone(), || lanes.cancel(LANE)).await;

    assert_eq!(pumped.unwrap_err().code, Code::Cancelled);
    assert_eq!(credit.left(), BULK as usize);
    assert!(read <= 2 * CHUNK, "{read}");
}

#[tokio::test]
async fn a_lane_whose_control_was_lost_ends_its_pump_and_owes_nothing() {
    let (lanes, authority) = generation();

    let (pumped, credit, _) = ended_by(lanes, || {
        authority.retire(Retired::ControlLost);
    })
    .await;

    assert_eq!(pumped.unwrap_err().code, Code::Cancelled);
    assert_eq!(credit.left(), BULK as usize);
}

#[tokio::test]
async fn a_pump_dropped_before_it_finished_is_ended_and_its_credit_comes_back() {
    let (lanes, _authority) = generation();
    let credit = Arc::new(Credit::default());
    let (to, mut far) = duplex(4 << 20);
    let (writer, pump) = paced(LANE, lanes, credit.clone(), to, BULK_PROGRESS);
    writing(writer, BULK as usize);
    let mut first = vec![0; CHUNK];
    far.read_exact(&mut first).await.unwrap();

    drop(pump);

    let mut rest = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), far.read_to_end(&mut rest))
        .await
        .expect("its lane ended")
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while credit.left() != BULK as usize {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("its credit back");
}

#[tokio::test]
async fn an_owner_dropped_while_it_waits_for_its_pump_ends_it_before_its_credit_comes_back() {
    let (lanes, _authority) = generation();
    let credit = Arc::new(Credit::default());
    let (to, mut far) = duplex(4 << 20);
    let (writer, pump) = paced(LANE, lanes, credit.clone(), to, BULK_PROGRESS);
    writing(writer, BULK as usize);
    let mut first = vec![0; CHUNK];
    far.read_exact(&mut first).await.unwrap();
    let finishing = tokio::spawn(pump.finished());
    tokio::time::sleep(Duration::from_millis(20)).await;

    finishing.abort();
    let _ = finishing.await;

    let mut rest = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), far.read_to_end(&mut rest))
        .await
        .expect("its pump ended, and its lane with it")
        .unwrap();
    assert!(
        CHUNK + rest.len() <= 2 * CHUNK,
        "nothing sent once it was dropped"
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while credit.left() != BULK as usize {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("its credit back");
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(credit.left(), BULK as usize, "and no new debt after");
}

#[tokio::test(start_paused = true)]
async fn an_unpaid_header_followed_by_silence_ends_its_lane_once_its_time_is_up() {
    let (lanes, _authority) = generation();
    let credit = Arc::new(Credit::default());
    let (to, mut far) = duplex(1 << 20);
    let (mut writer, pump) = paced(LANE, lanes.clone(), credit.clone(), to, BULK_PROGRESS);
    let header = b"{\"ok\":true,\"stream\":\"rgb-stream-v1\"}";
    writer.write_all(header).await.unwrap();
    let mut came = vec![0; header.len()];
    far.read_exact(&mut came).await.unwrap();
    let started = tokio::time::Instant::now();

    let ended = pump.finished().await.unwrap_err();

    assert_eq!(ended.code, Code::Transport, "{ended:?}");
    assert!(
        started.elapsed() <= BULK_PROGRESS,
        "{:?}",
        started.elapsed()
    );
    assert!(lanes.cancelled(LANE));
    assert_eq!(credit.left(), BULK as usize);
    drop(writer);
}

#[tokio::test(start_paused = true)]
async fn the_end_of_what_is_written_never_gives_an_unpaid_lane_more_time() {
    let (lanes, _authority) = generation();
    let credit = Arc::new(Credit::default());
    let (to, mut far) = duplex(1 << 20);
    let (mut writer, pump) = paced(LANE, lanes, credit.clone(), to, BULK_PROGRESS);
    writer.write_all(b"{\"ok\":true}\n").await.unwrap();
    let mut came = vec![0; 12];
    far.read_exact(&mut came).await.unwrap();
    let started = tokio::time::Instant::now();
    tokio::time::sleep(BULK_PROGRESS / 2).await;

    drop(writer);
    let ended = pump.finished().await.unwrap_err();

    assert_eq!(ended.code, Code::Transport, "{ended:?}");
    assert!(
        started.elapsed() <= BULK_PROGRESS,
        "{:?}",
        started.elapsed()
    );
    assert_eq!(credit.left(), BULK as usize);
}

#[tokio::test(start_paused = true)]
async fn credit_coming_back_keeps_a_slow_lane_going_past_its_time() {
    let (lanes, _authority) = generation();
    let credit = Arc::new(Credit::default());
    let (to, far) = duplex(4 << 20);
    let (writer, pump) = paced(LANE, lanes, credit.clone(), to, BULK_PROGRESS);
    writing(writer, 4 * CHUNK);
    let started = tokio::time::Instant::now();

    let read = repaying(far, LANE, credit.clone(), BULK_PROGRESS / 2);
    pump.finished().await.unwrap();

    assert_eq!(read.await.unwrap().0, 4 * CHUNK);
    assert!(started.elapsed() > BULK_PROGRESS, "slow, never stalled");
}
