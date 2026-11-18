//! A VM's pictures and file reads through a real keeper, its guest a
//! stand-in that paces each picture lane by its bulk credit as a machine
//! guest does: at most two chunks on their way, its lane's credit waited
//! for whole before it ends, and given back once the lane is cancelled or
//! its generation retires.
use super::*;
use crate::vm::keeper::{
    carries,
    change::tests::{admitted, live, proven},
    clients::tests::{Serving, asked, serving_with},
    tests::until,
};
use atlas_session::{
    credit::{BULK, Credit},
    standin::pictures::Pumped,
};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// The session a command's proof pins, as a picture names it.
pub(super) fn session() -> Value {
    json!({"securitySessionId": 42, "displayId": 7})
}

/// The keeper's request for `request`, a picture or a read, in the
/// generation `generation`.
pub(super) fn picture(request: Value, generation: u64) -> Value {
    json!({"op": "picture", "generation": generation, "request": request})
}

/// A pinned frame, as a command asks for one.
fn frame() -> Value {
    json!({"op": "frame", "maxDimension": 1280, "session": session()})
}

/// Numbered bytes, `length` of them, so any cut or reordering shows.
fn numbered(length: usize) -> Vec<u8> {
    (0..length).map(|at| (at % 251) as u8).collect()
}

/// Make the guest of `serving` send `bytes` on every picture lane, paced
/// by its credit.
fn sending(serving: &Serving, bytes: Vec<u8>) {
    serving
        .fixture
        .guest
        .standin
        .pictures
        .set(Arc::new(move |_| Some(bytes.clone())));
}

/// The guest's bulk credit in the live generation.
fn credit(serving: &Serving) -> Arc<Credit> {
    serving.fixture.guest.standin.bulk_credit().unwrap()
}

/// Every picture lane's pump that ended, in order.
fn pumped(serving: &Serving) -> Vec<Pumped> {
    serving.fixture.guest.standin.pumped()
}

/// The picture lanes the guest admitted.
fn picture_lanes(serving: &Serving) -> usize {
    admitted(serving)
        .iter()
        .filter(|header| header.kind == LaneKind::Picture)
        .count()
}

/// A client of the keeper that says `request` and reads the keeper's own
/// first line, a byte at a time so nothing after it is taken.
pub(super) async fn begun(serving: &Serving, request: &Value) -> (UnixStream, Value) {
    let mut stream = UnixStream::connect(&serving.socket).await.unwrap();
    let mut line = serde_json::to_vec(request).unwrap();
    line.push(b'\n');
    stream.write_all(&line).await.unwrap();
    let mut first = Vec::new();
    let mut byte = [0_u8; 1];
    while byte[0] != b'\n' {
        stream.read_exact(&mut byte).await.unwrap();
        first.push(byte[0]);
    }
    (stream, serde_json::from_slice(&first).unwrap())
}

/// Everything left on `stream` until the keeper closes it, within `within`.
pub(super) async fn rest(stream: &mut UnixStream, within: Duration) -> Vec<u8> {
    let mut rest = Vec::new();
    tokio::time::timeout(within, stream.read_to_end(&mut rest))
        .await
        .expect("the keeper closed in time")
        .unwrap();
    rest
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_picture_comes_whole_with_at_most_two_chunks_on_their_way_and_credit_back_only_once_written()
 {
    let (serving, _saying, _client, _directory) = proven().await;
    let bytes = numbered(1_536 * 1024);
    sending(&serving, bytes.clone());

    let (mut stream, first) = begun(&serving, &picture(frame(), live(&serving))).await;
    assert_eq!(first, json!({"ok": true, "op": "picture"}));
    let credit = credit(&serving);
    // Not read: what the keeper took is not given back.
    until("credit spent", || credit.left() < BULK as usize).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let held = BULK as usize - credit.left();
    assert!(held > 0, "credit came back before the client took it");
    assert!(held <= 2 * CHUNK, "{held} bytes on their way");
    assert!(pumped(&serving).is_empty(), "the guest still waits");

    let mut came = Vec::new();
    let mut part = vec![0_u8; 8 * 1024];
    while came.len() < bytes.len() {
        let read = stream.read(&mut part).await.unwrap();
        assert_ne!(read, 0, "ended at {}", came.len());
        came.extend_from_slice(&part[..read]);
        assert!(BULK as usize - credit.left() <= 2 * CHUNK);
    }
    assert!(rest(&mut stream, Duration::from_secs(10)).await.is_empty());

    assert!(came == bytes, "every byte, in order");
    until("the pump ended", || !pumped(&serving).is_empty()).await;
    let pump = pumped(&serving)[0];
    assert!(pump.whole, "{pump:?}");
    assert_eq!(pump.owed, 0);
    assert_eq!(credit.left(), BULK as usize);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn several_short_replies_read_late_come_whole_and_the_guest_is_owed_nothing() {
    let (serving, _saying, _client, _directory) = proven().await;
    let lines = b"{\"event\":\"data\",\"data\":\"aGk=\"}\n{\"event\":\"data\",\"data\":\"aGk=\"}\n{\"ok\":true,\"length\":4}\n";
    sending(&serving, lines.to_vec());
    let read = json!({"op": "read", "path": "a.bin", "offset": 0, "length": 4,
        "maxSize": 4, "session": session()});

    let (mut stream, first) = begun(&serving, &picture(read, live(&serving))).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let came = rest(&mut stream, Duration::from_secs(10)).await;

    assert_eq!(first["ok"], true, "{first}");
    assert_eq!(came, lines);
    until("the pump ended", || !pumped(&serving).is_empty()).await;
    let pump = pumped(&serving)[0];
    assert!(pump.whole, "{pump:?}");
    assert_eq!(pump.owed, 0);
    assert_eq!(credit(&serving).left(), BULK as usize);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_client_that_hangs_up_has_its_lane_cancelled_and_the_guests_pump_ends_owing_nothing() {
    let (serving, _saying, _client, _directory) = proven().await;
    sending(&serving, numbered(1_536 * 1024));

    let (mut stream, _) = begun(&serving, &picture(frame(), live(&serving))).await;
    let mut part = vec![0_u8; 100 * 1024];
    stream.read_exact(&mut part).await.unwrap();
    drop(stream);

    until("the pump ended", || !pumped(&serving).is_empty()).await;
    let pump = pumped(&serving)[0];
    assert!(!pump.whole, "{pump:?}");
    assert_eq!(credit(&serving).left(), BULK as usize, "given back whole");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_client_that_takes_nothing_for_its_time_is_closed_and_refunded_and_other_pictures_go_on()
{
    let (serving, _saying, _client, _directory) = proven().await;
    *serving.fixture.keeper.progress.lock().unwrap() = Duration::from_millis(300);
    let bytes = numbered(1_536 * 1024);
    sending(&serving, bytes.clone());

    let (mut stalled, _) = begun(&serving, &picture(frame(), live(&serving))).await;
    until("the stalled lane's pump ended", || {
        !pumped(&serving).is_empty()
    })
    .await;
    let pump = pumped(&serving)[0];
    let left = rest(&mut stalled, Duration::from_secs(5)).await;
    let (mut next, first) = begun(&serving, &picture(frame(), live(&serving))).await;
    let came = rest(&mut next, Duration::from_secs(30)).await;

    assert!(!pump.whole, "{pump:?}");
    assert!(left.len() < bytes.len(), "closed part way");
    assert_eq!(first["ok"], true, "{first}");
    assert!(came == bytes, "the next picture came whole");
    until("both pumps ended", || pumped(&serving).len() == 2).await;
    assert_eq!(credit(&serving).left(), BULK as usize);
    assert_eq!(serving.fixture.guest.controls().len(), 1, "control kept");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stop_is_answered_while_stalled_pictures_hold_all_the_guests_credit() {
    let (serving, _saying, _client, _directory) = proven().await;
    sending(&serving, numbered(1_536 * 1024));
    let generation = live(&serving);

    let mut stalled = Vec::new();
    for _ in 0..8 {
        stalled.push(begun(&serving, &picture(frame(), generation)).await.0);
    }
    let credit = credit(&serving);
    until("every credit spent", || credit.left() == 0).await;
    let stop = asked(&serving.socket, &json!({"op": "stop"})).await;

    assert_eq!(stop[0]["outcome"], "stopped", "{stop:?}");
    assert!(pumped(&serving).is_empty(), "the pictures were never held");
    drop(stalled);
    until("every pump ended", || pumped(&serving).len() == 8).await;
    assert_eq!(credit.left(), BULK as usize);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_generations_end_closes_a_pictures_client_in_time_and_its_pump_owes_nothing() {
    let (serving, _saying, _client, _directory) = proven().await;
    sending(&serving, numbered(1_536 * 1024));

    let (mut stalled, _) = begun(&serving, &picture(frame(), live(&serving))).await;
    let credit = credit(&serving);
    until("credit spent", || credit.left() < BULK as usize).await;
    serving.fixture.guest.cut();
    let started = tokio::time::Instant::now();
    let _ = rest(&mut stalled, Duration::from_secs(5)).await;

    assert!(
        started.elapsed() < PROGRESS / 2,
        "closed with the generation"
    );
    until("the pump ended", || !pumped(&serving).is_empty()).await;
    assert!(!pumped(&serving)[0].whole);
    assert_eq!(credit.left(), BULK as usize);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn what_a_client_says_after_the_keepers_first_line_reaches_the_guest() {
    let (serving, _saying, _client, _directory) = proven().await;
    serving
        .fixture
        .guest
        .standin
        .pictures
        .set(Arc::new(|_| None));

    let (mut stream, first) = begun(&serving, &picture(frame(), live(&serving))).await;
    stream.write_all(&[0]).await.unwrap();
    until("the lane admitted", || picture_lanes(&serving) == 1).await;
    let lane = admitted(&serving)
        .into_iter()
        .find(|header| header.kind == LaneKind::Picture)
        .unwrap()
        .lane;
    until("heard", || {
        serving.fixture.guest.standin.heard_on(lane) == [0]
    })
    .await;

    assert_eq!(first["ok"], true, "{first}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_picture_of_another_generation_or_said_too_early_is_refused_unsaid() {
    let (serving, _saying, _client, _directory) = proven().await;
    sending(&serving, numbered(16));
    let generation = live(&serving);

    let (_, other) = begun(&serving, &picture(frame(), generation + 1)).await;
    let mut early = UnixStream::connect(&serving.socket).await.unwrap();
    let mut said = serde_json::to_vec(&picture(frame(), generation)).unwrap();
    said.extend_from_slice(b"\n\0");
    early.write_all(&said).await.unwrap();
    let early = rest(&mut early, Duration::from_secs(10)).await;
    let early: Value = serde_json::from_slice(&early).unwrap();

    assert_eq!(other["code"], "session-changed", "{other}");
    assert_eq!(other["sent"], false);
    assert_eq!(early["code"], "bad-request", "{early}");
    assert_eq!(early["sent"], false);
    assert_eq!(picture_lanes(&serving), 0, "nothing asked of the guest");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_that_gives_no_pictures_is_told_to_update_and_nothing_is_asked() {
    let serving = serving_with(true, true).await;
    let generation = live(&serving);

    let (_, refused) = begun(&serving, &picture(frame(), generation)).await;

    assert_eq!(refused["code"], "update", "{refused}");
    assert_eq!(picture_lanes(&serving), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_a_pinned_picture_or_read_goes_on_a_picture_lane_and_a_read_lane_still_denies_one() {
    let (serving, _saying, _client, _directory) = proven().await;
    let generation = live(&serving);

    let mut refused = Vec::new();
    for request in [
        json!({"op": "screenshot"}),
        json!({"op": "frame", "maxDimension": 1280}),
        json!({"op": "read", "path": "a.bin"}),
        json!({"op": "run", "argv": ["/bin/echo"], "session": session()}),
        json!({"op": "windows", "session": session()}),
    ] {
        refused.push(begun(&serving, &picture(request, generation)).await.1);
    }
    let read_lane = asked(
        &serving.socket,
        &json!({"op": "read", "request": {"op": "screenshot", "session": session()}}),
    )
    .await;

    for refused in refused.iter().chain(&read_lane) {
        assert_eq!(refused["code"], "denied", "{refused}");
    }
    assert_eq!(picture_lanes(&serving), 0);
    assert!(carries_bulk(&json!({"op": "stream", "session": session()})));
    assert!(!carries(
        &LaneKind::Read,
        &json!({"op": "frame", "session": session()})
    ));
}
