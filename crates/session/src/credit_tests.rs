use super::*;
use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::io::{AsyncReadExt, duplex};

const LANE: LaneId = LaneId(1);

fn left(credit: &Credit) -> usize {
    credit.available.available_permits()
}

#[tokio::test]
async fn bulk_waits_once_its_credit_is_spent_and_goes_on_when_it_comes_back() {
    let credit = Arc::new(Credit::default());
    let (mut near, mut far) = duplex(4 << 20);
    let all = vec![1; BULK as usize + CHUNK];

    let sending = {
        let credit = credit.clone();
        tokio::spawn(async move { credit.send(LANE, &mut near, &all).await })
    };
    let mut first = vec![0; BULK as usize];
    far.read_exact(&mut first).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!sending.is_finished(), "more than the credit went");

    credit.returned(LANE, CHUNK as u64);
    tokio::time::timeout(Duration::from_secs(5), sending)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[test]
fn credit_given_back_never_grows_past_what_a_lane_owes() {
    let credit = Credit::default();

    credit.returned(LANE, u64::MAX);
    credit.returned(LaneId(9), u64::MAX);

    assert_eq!(left(&credit), BULK as usize);
}

/// A writer that takes `takes` bytes and then fails.
struct Failing {
    takes: usize,
}

impl AsyncWrite for Failing {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        if self.takes == 0 {
            return Poll::Ready(Err(std::io::Error::other("the lane failed")));
        }
        let taken = data.len().min(self.takes);
        self.takes -= taken;
        Poll::Ready(Ok(taken))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn a_chunk_none_of_which_was_written_is_not_owed() {
    let credit = Credit::default();

    let failed = credit
        .send(LANE, &mut Failing { takes: 0 }, &vec![0; CHUNK])
        .await;

    assert!(failed.is_err());
    assert_eq!(left(&credit), BULK as usize);
}

#[tokio::test]
async fn a_chunk_written_in_part_stays_owed_until_its_lane_closes() {
    let credit = Credit::default();

    let failed = credit
        .send(LANE, &mut Failing { takes: CHUNK / 2 }, &vec![0; CHUNK])
        .await;

    assert!(failed.is_err());
    assert_eq!(
        left(&credit),
        BULK as usize - CHUNK,
        "part may be on its way"
    );
    credit.closed(LANE);
    assert_eq!(left(&credit), BULK as usize);
}

#[tokio::test]
async fn a_send_given_up_part_way_keeps_owing_what_may_be_on_its_way() {
    let credit = Credit::default();
    // Half a chunk fits; the rest waits for a reader that never comes.
    let (mut near, _far) = duplex(CHUNK / 2);

    let gave_up = tokio::time::timeout(
        Duration::from_millis(50),
        credit.send(LANE, &mut near, &vec![0; CHUNK * 3]),
    )
    .await;

    assert!(gave_up.is_err());
    assert_eq!(left(&credit), BULK as usize - CHUNK);
    credit.closed(LANE);
    assert_eq!(left(&credit), BULK as usize);
}

#[tokio::test]
async fn a_send_given_up_before_it_wrote_anything_owes_nothing() {
    let credit = Credit::default();
    let (mut near, _far) = duplex(CHUNK);
    // The first chunk fills the pipe; the second never starts writing.
    credit.send(LANE, &mut near, &vec![0; CHUNK]).await.unwrap();

    let gave_up = tokio::time::timeout(
        Duration::from_millis(50),
        credit.send(LANE, &mut near, &vec![0; CHUNK]),
    )
    .await;

    assert!(gave_up.is_err());
    assert_eq!(left(&credit), BULK as usize - CHUNK);
}

/// A writer that takes everything and whose flush waits until `flushing`
/// is let go.
struct SlowFlush {
    flushing: Arc<tokio::sync::Semaphore>,
}

impl AsyncWrite for SlowFlush {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Poll::Ready(Ok(data.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        if self.flushing.try_acquire().is_ok() {
            return Poll::Ready(Ok(()));
        }
        context.waker().wake_by_ref();
        Poll::Pending
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn credit_given_back_while_its_chunk_still_flushes_is_counted() {
    let credit = Arc::new(Credit::default());
    let flushing = Arc::new(tokio::sync::Semaphore::new(0));
    let sending = {
        let (credit, flushing) = (credit.clone(), flushing.clone());
        tokio::spawn(async move {
            credit
                .send(LANE, &mut SlowFlush { flushing }, &vec![0; CHUNK])
                .await
        })
    };
    tokio::time::sleep(Duration::from_millis(20)).await;

    // The other end read it before the flush finished here.
    credit.returned(LANE, CHUNK as u64);
    flushing.add_permits(1);
    sending.await.unwrap().unwrap();

    assert_eq!(left(&credit), BULK as usize);
}

#[tokio::test]
async fn many_lanes_each_sending_less_than_a_chunk_never_stall_each_other() {
    let credit = Arc::new(Credit::default());
    let lanes = 64;
    let each = BULK as usize / 32;
    let mut sending = Vec::new();
    for lane in 0..lanes {
        let credit = credit.clone();
        let (mut near, mut far) = duplex(each * 2);
        let reading = credit.clone();
        tokio::spawn(async move {
            let mut buffer = vec![0; each];
            // Credit back for every read, however small.
            while let Ok(read) = far.read(&mut buffer).await {
                if read == 0 {
                    return;
                }
                reading.returned(LaneId(lane), read as u64);
            }
        });
        sending.push(tokio::spawn(async move {
            credit.send(LaneId(lane), &mut near, &vec![0; each]).await
        }));
    }

    for sent in sending {
        tokio::time::timeout(Duration::from_secs(5), sent)
            .await
            .expect("a lane stalled")
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn a_wait_for_room_on_a_lane_ends_once_it_owes_no_more_than_asked() {
    let credit = Arc::new(Credit::default());
    let (mut near, _far) = duplex(4 * CHUNK);
    credit
        .send(LANE, &mut near, &vec![0; 2 * CHUNK])
        .await
        .unwrap();
    let waiting = {
        let credit = credit.clone();
        tokio::spawn(async move { credit.owing_at_most(LANE, CHUNK as u64).await })
    };
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!waiting.is_finished(), "two chunks are owed");

    assert_eq!(credit.owing(LANE), 2 * CHUNK as u64);

    credit.returned(LANE, CHUNK as u64);

    tokio::time::timeout(Duration::from_secs(5), waiting)
        .await
        .expect("room once one chunk came back")
        .unwrap();
    assert_eq!(credit.owing(LANE), CHUNK as u64);
    assert_eq!(credit.owing(LaneId(9)), 0);
    credit.owing_at_most(LaneId(9), 0).await;
}

#[tokio::test]
async fn a_lane_its_reader_gave_up_is_repaid_at_once_and_sends_nothing_more_until_it_closes() {
    let credit = Credit::default();
    let (mut near, _far) = duplex(1 << 20);
    credit.send(LANE, &mut near, &[1; CHUNK]).await.unwrap();

    credit.returned(LANE, ENDED);
    tokio::time::timeout(Duration::from_secs(5), credit.repaid(LANE))
        .await
        .expect("repaid at once");
    tokio::time::timeout(Duration::from_secs(5), credit.given_up(LANE))
        .await
        .expect("told");
    let after = credit.send(LANE, &mut near, &[2; CHUNK]).await;
    credit.closed(LANE);

    assert!(after.is_err(), "nothing more goes");
    assert_eq!(left(&credit), BULK as usize, "everything it owed came back");
    assert!(credit.gone().is_empty(), "forgotten once closed");
    assert!(
        credit.send(LaneId(2), &mut near, &[3; 16]).await.is_ok(),
        "another lane goes on"
    );
}
