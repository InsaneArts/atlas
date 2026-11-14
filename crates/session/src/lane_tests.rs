use super::*;
use crate::{
    authority::{Hook, Retired},
    clock::Manual,
};
use std::sync::Arc;

fn lanes() -> (Lanes, Authority) {
    let authority = Authority::new(
        Generation(3),
        Arc::new(Manual::default()),
        Box::new(|_, _| {}) as Hook,
    );
    (Lanes::new(authority.clone()), authority)
}

fn header(lane: u64) -> Header {
    Header {
        generation: Generation(3),
        lane: LaneId(lane),
        kind: LaneKind::Read,
    }
}

#[test]
fn a_lane_the_station_opens_is_admitted_once() {
    let (lanes, _) = lanes();

    lanes.admit(&header(1), Opener::Station).unwrap();

    assert_eq!(
        lanes.admit(&header(1), Opener::Station).unwrap_err().code,
        Code::Denied
    );
}

#[test]
fn a_lane_the_guest_opens_must_be_one_the_station_asked_for() {
    let (lanes, _) = lanes();

    assert!(lanes.admit(&header(1), Opener::Guest).is_err());
    lanes.asked(LaneId(2));
    lanes.admit(&header(2), Opener::Guest).unwrap();
}

#[test]
fn a_cancel_that_comes_before_its_lane_still_refuses_it() {
    let (lanes, _) = lanes();
    lanes.asked(LaneId(4));

    lanes.cancel(LaneId(4));

    assert!(lanes.cancelled(LaneId(4)));
    assert!(lanes.admit(&header(4), Opener::Guest).is_err());
    assert!(lanes.admit(&header(4), Opener::Station).is_err());
}

#[test]
fn a_lane_of_another_generation_is_refused() {
    let (lanes, _) = lanes();
    let mut older = header(1);
    older.generation = Generation(2);

    assert!(lanes.admit(&older, Opener::Station).is_err());
}

#[test]
fn no_lane_is_admitted_once_its_generation_retired() {
    let (lanes, authority) = lanes();

    authority.retire(Retired::Replaced);

    assert_eq!(
        lanes.admit(&header(1), Opener::Station).unwrap_err().code,
        Code::SessionChanged
    );
}

#[test]
fn a_header_names_a_change_by_its_kind() {
    let change = Token::new();
    let written = serde_json::to_value(Header {
        generation: Generation(3),
        lane: LaneId(9),
        kind: LaneKind::Change {
            change: change.clone(),
        },
    })
    .unwrap();

    assert_eq!(
        written,
        serde_json::json!({
            "generation": 3,
            "lane": 9,
            "kind": {"change": {"change": change.as_str()}},
        })
    );
    assert_eq!(
        serde_json::from_value::<Header>(written).unwrap().lane,
        LaneId(9)
    );
    let without = serde_json::json!({"generation": 3, "lane": 9, "kind": {"change": {}}});
    assert!(serde_json::from_value::<Header>(without).is_err());
    for unknown in [
        serde_json::json!({"generation": 3, "lane": 9, "kind": "read", "run": "rm"}),
        serde_json::json!({
            "generation": 3,
            "lane": 9,
            "kind": {"stage": {"change": change.as_str(), "run": "rm"}},
        }),
    ] {
        assert!(serde_json::from_value::<Header>(unknown).is_err());
    }
}

#[tokio::test]
async fn a_header_goes_first_and_what_follows_is_left_for_the_lane() {
    use crate::{
        keys::Keypair,
        secure::{Purpose, connect_as_guest, connect_as_station},
    };
    use atlas_core::{desktop::Desktop, user::User};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (guest, station) = (Keypair::generate().unwrap(), Keypair::generate().unwrap());
    let named = Desktop::from(User { uid: 502 }).id();
    let (guest_end, station_end) = tokio::io::duplex(1 << 16);
    let (guest_side, station_side) = tokio::join!(
        connect_as_guest(guest_end, &guest, station.public(), &named, Purpose::Lane),
        connect_as_station(station_end, &station, guest.public(), &named, Purpose::Lane),
    );
    let (mut guest_side, mut station_side) = (guest_side.unwrap(), station_side.unwrap());

    send_header(&mut station_side, &header(5)).await.unwrap();
    station_side.write_all(b"body").await.unwrap();
    station_side.flush().await.unwrap();

    assert_eq!(read_header(&mut guest_side).await.unwrap(), header(5));
    let mut body = [0; 4];
    guest_side.read_exact(&mut body).await.unwrap();
    assert_eq!(&body, b"body");
}

#[test]
fn a_lane_admitted_before_a_retirement_is_one_its_hook_finds() {
    let found: Arc<std::sync::OnceLock<Arc<Lanes>>> = Arc::new(std::sync::OnceLock::new());
    let swept = Arc::new(Mutex::new(Vec::new()));
    let hook = {
        let (found, swept) = (found.clone(), swept.clone());
        Box::new(move |_, _| {
            if let Some(lanes) = found.get() {
                *swept.lock().unwrap() = lanes.admitted();
            }
        }) as Hook
    };
    let authority = Authority::new(Generation(3), Arc::new(Manual::default()), hook);
    let lanes = Arc::new(Lanes::new(authority.clone()));
    let _ = found.set(lanes.clone());

    lanes.admit(&header(1), Opener::Station).unwrap();
    authority.retire(Retired::ControlLost);

    assert_eq!(*swept.lock().unwrap(), [LaneId(1)]);
    assert!(lanes.admit(&header(2), Opener::Station).is_err());
}

/// Both ends of a lane, connected.
async fn lane() -> (
    Secure<tokio::io::DuplexStream>,
    Secure<tokio::io::DuplexStream>,
) {
    use crate::{
        keys::Keypair,
        secure::{Purpose, connect_as_guest, connect_as_station},
    };
    use atlas_core::{desktop::Desktop, user::User};
    let (guest, station) = (Keypair::generate().unwrap(), Keypair::generate().unwrap());
    let named = Desktop::from(User { uid: 502 }).id();
    let (guest_end, station_end) = tokio::io::duplex(1 << 16);
    let (guest_side, station_side) = tokio::join!(
        connect_as_guest(guest_end, &guest, station.public(), &named, Purpose::Lane),
        connect_as_station(station_end, &station, guest.public(), &named, Purpose::Lane),
    );
    (guest_side.unwrap(), station_side.unwrap())
}

#[tokio::test]
async fn a_header_and_its_body_in_one_frame_leave_the_body_whole() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (mut guest_side, mut station_side) = lane().await;
    let mut together = serde_json::to_vec(&header(6)).unwrap();
    together.extend_from_slice(b"\nthe whole body");

    station_side.write_all(&together).await.unwrap();
    station_side.flush().await.unwrap();

    assert_eq!(read_header(&mut guest_side).await.unwrap(), header(6));
    let mut body = [0; 14];
    guest_side.read_exact(&mut body).await.unwrap();
    assert_eq!(&body, b"the whole body");
}

#[tokio::test]
async fn a_header_of_the_longest_length_is_read_and_a_longer_one_refused() {
    use tokio::io::AsyncWriteExt;
    for (length, read) in [(HEADER, true), (HEADER + 1, false)] {
        let (mut guest_side, mut station_side) = lane().await;
        let mut line = serde_json::to_vec(&header(7)).unwrap();
        // JSON allows the spaces after it.
        line.resize(length, b' ');
        line.push(b'\n');

        station_side.write_all(&line).await.unwrap();
        station_side.flush().await.unwrap();

        let header_read = read_header(&mut guest_side).await;
        assert_eq!(header_read.is_ok(), read, "{length} bytes");
    }
}

/// Whether `ended` completes within a moment.
async fn ends(ended: impl std::future::Future<Output = ()>) -> bool {
    tokio::time::timeout(std::time::Duration::from_millis(200), ended)
        .await
        .is_ok()
}

#[tokio::test]
async fn a_lanes_work_ends_when_it_is_cancelled_before_or_while_it_waits() {
    let (lanes, _) = lanes();
    let lanes = Arc::new(lanes);
    lanes.cancel(LaneId(1));
    assert!(ends(lanes.ended(LaneId(1))).await);

    let waiting = tokio::spawn({
        let lanes = lanes.clone();
        async move { lanes.ended(LaneId(2)).await }
    });
    tokio::task::yield_now().await;
    lanes.cancel(LaneId(2));
    tokio::time::timeout(std::time::Duration::from_secs(5), waiting)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn another_lanes_cancel_leaves_a_lanes_work_running() {
    let (lanes, _) = lanes();

    lanes.cancel(LaneId(1));

    assert!(!ends(lanes.ended(LaneId(2))).await);
}

#[tokio::test]
async fn every_lanes_work_ends_when_its_generation_retires() {
    let (lanes, authority) = lanes();

    authority.retire(Retired::ControlLost);

    assert!(ends(lanes.ended(LaneId(5))).await);
}
