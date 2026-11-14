use super::*;
use atlas_core::{desktop::Desktop, user::User};
use std::time::Duration;
use tokio::io::{DuplexStream, duplex};

fn desktop(uid: u32) -> DesktopId {
    Desktop::from(User { uid }).id()
}

fn keys() -> Keypair {
    Keypair::generate().unwrap()
}

/// What the wire between the two ends does to the guest's frames.
#[derive(Clone, Copy)]
enum Wire {
    Faithful,
    /// Its first frame after the handshake arrives twice.
    Replays,
    /// Its first two frames after the handshake arrive the other way round.
    Reorders,
}

/// Carry the guest's frames (`left`) to the station (`right`) the way
/// `wire` does, and everything back as it is. KK's handshake is one frame
/// each way, so the guest's second frame is its first after it.
fn carry(left: DuplexStream, right: DuplexStream, wire: Wire) {
    let (mut left_read, mut left_write) = tokio::io::split(left);
    let (mut right_read, mut right_write) = tokio::io::split(right);
    tokio::spawn(async move {
        let _ = tokio::io::copy(&mut right_read, &mut left_write).await;
    });
    tokio::spawn(async move {
        let mut buffer = vec![0; FRAME];
        let mut held = None;
        for index in 0.. {
            let Ok(length) = read_frame(&mut left_read, &mut buffer).await else {
                // The guest's end went: so does the station's view of it.
                let _ = right_write.shutdown().await;
                return;
            };
            let frame = buffer[..length].to_vec();
            let order = match (wire, index) {
                (Wire::Replays, 1) => vec![frame.clone(), frame],
                (Wire::Reorders, 1) => {
                    held = Some(frame);
                    vec![]
                }
                (Wire::Reorders, 2) => vec![frame, held.take().unwrap()],
                _ => vec![frame],
            };
            for frame in order {
                if write_frame(&mut right_write, &frame).await.is_err() {
                    return;
                }
            }
        }
    });
}

/// A guest and a station connected for control with each other's keys
/// pinned, over `wire`.
async fn connected(wire: Wire) -> (Secure<DuplexStream>, Secure<DuplexStream>) {
    let (guest, station, named) = (keys(), keys(), desktop(502));
    let (guest_end, left) = duplex(1 << 18);
    let (right, station_end) = duplex(1 << 18);
    carry(left, right, wire);
    let (guest_side, station_side) = tokio::join!(
        connect_as_guest(
            guest_end,
            &guest,
            station.public(),
            &named,
            Purpose::Control
        ),
        connect_as_station(
            station_end,
            &station,
            guest.public(),
            &named,
            Purpose::Control
        ),
    );
    (guest_side.unwrap(), station_side.unwrap())
}

/// One frame, written straight through: data and a flush.
async fn send(end: &mut Secure<DuplexStream>, data: &[u8]) {
    end.write_all(data).await.unwrap();
    end.flush().await.unwrap();
}

async fn received(end: &mut Secure<DuplexStream>, length: usize) -> io::Result<Vec<u8>> {
    let mut data = vec![0; length];
    end.read_exact(&mut data).await.map(|_| data)
}

#[tokio::test]
async fn data_arrives_whole_either_way_however_many_frames_it_takes() {
    let (mut guest, mut station) = connected(Wire::Faithful).await;
    let large: Vec<u8> = (0..1_000_000_u32).map(|at| at as u8).collect();

    let sending = large.clone();
    let (_, back) = tokio::join!(
        async {
            guest.write_all(&sending).await.unwrap();
            guest.flush().await.unwrap();
        },
        received(&mut station, large.len()),
    );
    send(&mut station, b"back").await;

    assert_eq!(back.unwrap(), large);
    assert_eq!(received(&mut guest, 4).await.unwrap(), b"back");
}

#[tokio::test]
async fn each_end_knows_the_other_by_its_pinned_key() {
    let (guest, station, named) = (keys(), keys(), desktop(502));
    let (guest_end, station_end) = duplex(1 << 18);

    let (guest_side, station_side) = tokio::join!(
        connect_as_guest(guest_end, &guest, station.public(), &named, Purpose::Lane),
        connect_as_station(station_end, &station, guest.public(), &named, Purpose::Lane),
    );

    assert_eq!(guest_side.unwrap().peer(), station.public());
    assert_eq!(station_side.unwrap().peer(), guest.public());
}

#[tokio::test]
async fn the_station_waits_for_the_guest_to_begin_whoever_opened_the_stream() {
    let (guest, station, named) = (keys(), keys(), desktop(502));
    let (mut silent_guest, station_end) = duplex(1 << 18);

    let answering = tokio::spawn(async move {
        connect_as_station(
            station_end,
            &station,
            guest.public(),
            &named,
            Purpose::Control,
        )
        .await
    });
    let mut first = [0; 1];
    let wrote =
        tokio::time::timeout(Duration::from_millis(200), silent_guest.read(&mut first)).await;

    assert!(wrote.is_err(), "the station wrote first");
    answering.abort();
}

#[tokio::test]
async fn a_guest_the_station_did_not_pin_is_refused_before_any_data() {
    let (guest, station, stranger, named) = (keys(), keys(), keys(), desktop(502));
    let (guest_end, station_end) = duplex(1 << 18);

    let (guest_side, station_side) = tokio::join!(
        connect_as_guest(
            guest_end,
            &stranger,
            station.public(),
            &named,
            Purpose::Control
        ),
        connect_as_station(
            station_end,
            &station,
            guest.public(),
            &named,
            Purpose::Control
        ),
    );

    assert_eq!(station_side.unwrap_err().code, Code::Identity);
    assert!(guest_side.is_err());
}

#[tokio::test]
async fn a_station_without_the_pinned_key_is_refused_before_any_data() {
    let (guest, station, impostor, named) = (keys(), keys(), keys(), desktop(502));
    let (guest_end, station_end) = duplex(1 << 18);

    let (guest_side, station_side) = tokio::join!(
        connect_as_guest(
            guest_end,
            &guest,
            station.public(),
            &named,
            Purpose::Control
        ),
        connect_as_station(
            station_end,
            &impostor,
            guest.public(),
            &named,
            Purpose::Control
        ),
    );

    assert!(station_side.is_err());
    assert!(guest_side.is_err());
}

#[tokio::test]
async fn ends_that_disagree_on_the_desktop_or_the_purpose_never_connect() {
    let (guest, station, named) = (keys(), keys(), desktop(502));
    for (guest_named, guest_purpose) in [
        (desktop(503), Purpose::Control),
        (desktop(502), Purpose::Lane),
    ] {
        let (guest_end, station_end) = duplex(1 << 18);
        let (guest_side, station_side) = tokio::join!(
            connect_as_guest(
                guest_end,
                &guest,
                station.public(),
                &guest_named,
                guest_purpose
            ),
            connect_as_station(
                station_end,
                &station,
                guest.public(),
                &named,
                Purpose::Control
            ),
        );
        assert_eq!(station_side.unwrap_err().code, Code::Identity);
        assert!(guest_side.is_err());
    }
}

#[tokio::test]
async fn an_end_that_reached_itself_is_refused() {
    let (own, named) = (keys(), desktop(502));
    let same = Keypair::from_secret(crate::keys::SecretKey::from_bytes(*own.secret())).unwrap();
    let (guest_end, station_end) = duplex(1 << 18);

    let (guest_side, station_side) = tokio::join!(
        connect_as_guest(guest_end, &own, own.public(), &named, Purpose::Control),
        connect_as_station(station_end, &same, own.public(), &named, Purpose::Control),
    );

    assert_eq!(station_side.unwrap_err().code, Code::Identity);
    assert!(guest_side.is_err());
}

#[tokio::test]
async fn a_handshake_that_carries_data_is_refused() {
    let (guest, station, named) = (keys(), keys(), desktop(502));
    let (mut guest_end, station_end) = duplex(1 << 18);
    let prologue = prologue(&named, Purpose::Control);
    let mut first = pinned(&guest, &station.public(), &prologue)
        .unwrap()
        .build_initiator()
        .unwrap();

    let answering = tokio::spawn(async move {
        connect_as_station(
            station_end,
            &station,
            guest.public(),
            &named,
            Purpose::Control,
        )
        .await
    });
    let mut message = vec![0; FRAME];
    let length = first.write_message(b"run this", &mut message).unwrap();
    write_frame(&mut guest_end, &message[..length])
        .await
        .unwrap();

    assert_eq!(answering.await.unwrap().unwrap_err().code, Code::Identity);
}

#[tokio::test]
async fn a_recorded_connection_replayed_to_the_station_opens_nothing() {
    let (guest, station, named) = (keys(), keys(), desktop(502));
    let (guest_public, station_public) = (guest.public(), station.public());
    // The first connection, the guest's frames recorded on the way: its
    // handshake message and its first message after it.
    let (guest_end, mut left) = duplex(1 << 18);
    let (mut right, station_end) = duplex(1 << 18);
    let (mut handshake, mut first) = (vec![0; FRAME], vec![0; FRAME]);
    let (guest_side, station_side, (handshake_length, first_length)) = tokio::join!(
        async {
            let mut opened =
                connect_as_guest(guest_end, &guest, station_public, &named, Purpose::Control)
                    .await
                    .unwrap();
            send(&mut opened, b"hello").await;
            opened
        },
        async {
            let mut opened = connect_as_station(
                station_end,
                &station,
                guest_public,
                &named,
                Purpose::Control,
            )
            .await
            .unwrap();
            assert_eq!(received(&mut opened, 5).await.unwrap(), b"hello");
            opened
        },
        async {
            let handshake_length = read_frame(&mut left, &mut handshake).await.unwrap();
            write_frame(&mut right, &handshake[..handshake_length])
                .await
                .unwrap();
            let mut back = vec![0; FRAME];
            let answer = read_frame(&mut right, &mut back).await.unwrap();
            write_frame(&mut left, &back[..answer]).await.unwrap();
            let first_length = read_frame(&mut left, &mut first).await.unwrap();
            write_frame(&mut right, &first[..first_length])
                .await
                .unwrap();
            (handshake_length, first_length)
        },
    );
    drop((guest_side, station_side));

    // Both, replayed to the station later. KK's answering end finishes on
    // the handshake message, but its fresh ephemeral key makes keys the
    // recorded message was never sealed with.
    let (mut replaying, station_end) = duplex(1 << 18);
    let answering = tokio::spawn(async move {
        let mut opened = connect_as_station(
            station_end,
            &station,
            guest_public,
            &named,
            Purpose::Control,
        )
        .await
        .unwrap();
        let mut header = [0; 5];
        let replayed = opened.read_exact(&mut header).await.unwrap_err();
        let after = opened.read_exact(&mut header).await.is_err();
        (replayed.kind(), after)
    });
    write_frame(&mut replaying, &handshake[..handshake_length])
        .await
        .unwrap();
    let mut answer = vec![0; FRAME];
    read_frame(&mut replaying, &mut answer).await.unwrap();
    write_frame(&mut replaying, &first[..first_length])
        .await
        .unwrap();

    let (replayed, after) = answering.await.unwrap();
    assert_eq!(replayed, io::ErrorKind::InvalidData, "the replay was read");
    assert!(after, "the connection went on after a replay");
}

#[tokio::test]
async fn a_replayed_frame_breaks_the_connection_for_good() {
    let (mut guest, mut station) = connected(Wire::Replays).await;

    send(&mut guest, b"once").await;
    send(&mut guest, b"more").await;

    assert_eq!(received(&mut station, 4).await.unwrap(), b"once");
    let replayed = received(&mut station, 4).await.unwrap_err();
    assert_eq!(replayed.kind(), io::ErrorKind::InvalidData);
    assert!(
        received(&mut station, 4).await.is_err(),
        "nothing is read after a bad frame"
    );
    assert!(station.write_all(b"reply").await.is_err(), "nor written");
}

#[tokio::test]
async fn frames_out_of_order_break_the_connection() {
    let (mut guest, mut station) = connected(Wire::Reorders).await;

    send(&mut guest, b"first").await;
    send(&mut guest, b"second").await;

    let reordered = received(&mut station, 6).await.unwrap_err();
    assert_eq!(reordered.kind(), io::ErrorKind::InvalidData);
}

#[tokio::test]
async fn an_end_that_finishes_is_read_to_its_end() {
    let (mut guest, mut station) = connected(Wire::Faithful).await;

    guest.write_all(b"last").await.unwrap();
    guest.shutdown().await.unwrap();
    let mut everything = Vec::new();
    station.read_to_end(&mut everything).await.unwrap();

    assert_eq!(everything, b"last");
}

#[tokio::test]
async fn a_connection_cut_without_finishing_is_a_failure_not_an_end() {
    let (guest_end, station_end) = duplex(1 << 18);
    let (guest, station, named) = (keys(), keys(), desktop(502));
    let (guest_side, station_side) = tokio::join!(
        connect_as_guest(guest_end, &guest, station.public(), &named, Purpose::Lane),
        connect_as_station(station_end, &station, guest.public(), &named, Purpose::Lane),
    );
    let (mut guest_side, mut station_side) = (guest_side.unwrap(), station_side.unwrap());

    send(&mut guest_side, b"part").await;
    drop(guest_side);
    let mut everything = Vec::new();
    let cut = station_side.read_to_end(&mut everything).await.unwrap_err();

    assert_eq!(cut.kind(), io::ErrorKind::UnexpectedEof);
    assert_eq!(everything, b"part");
}

#[tokio::test]
async fn a_pairing_proves_the_secret_and_tells_each_end_the_others_key() {
    let (guest, station, named) = (keys(), keys(), desktop(502));
    let secret = PairingSecret::generate().unwrap();
    let same = PairingSecret::from_bytes(*secret.as_bytes());
    let (guest_end, station_end) = duplex(1 << 18);

    let (guest_side, station_side) = tokio::join!(
        pair_as_guest(guest_end, &guest, &secret, &named),
        pair_as_station(station_end, &station, &same, &named),
    );

    assert_eq!(guest_side.unwrap().peer(), station.public());
    assert_eq!(station_side.unwrap().peer(), guest.public());
}

#[tokio::test]
async fn a_guest_without_the_secret_is_refused_by_the_station() {
    let (guest, station, named) = (keys(), keys(), desktop(502));
    let (made, other) = (
        PairingSecret::generate().unwrap(),
        PairingSecret::generate().unwrap(),
    );
    let (guest_end, station_end) = duplex(1 << 18);

    let (guest_side, station_side) = tokio::join!(
        pair_as_guest(guest_end, &guest, &other, &named),
        pair_as_station(station_end, &station, &made, &named),
    );

    assert_eq!(station_side.unwrap_err().code, Code::Identity);
    // The guest's handshake ended at its own last message; it has no proof
    // of the station until a frame opens, and none ever comes.
    let mut guest_side = guest_side.unwrap();
    assert!(received(&mut guest_side, 1).await.is_err());
}

/// Run a pairing's three messages between `guest` and `station`, which
/// carry nothing: whether the station took the last one.
fn paired(guest: &mut HandshakeState, station: &mut HandshakeState) -> bool {
    let (mut message, mut payload) = (vec![0; FRAME], vec![0; FRAME]);
    let length = guest.write_message(&[], &mut message).unwrap();
    station
        .read_message(&message[..length], &mut payload)
        .unwrap();
    let length = station.write_message(&[], &mut message).unwrap();
    guest
        .read_message(&message[..length], &mut payload)
        .unwrap();
    let length = guest.write_message(&[], &mut message).unwrap();
    station
        .read_message(&message[..length], &mut payload)
        .is_ok()
}

#[tokio::test]
async fn what_a_station_with_another_secret_seals_never_opens_at_the_guest() {
    let (guest, station, named) = (keys(), keys(), desktop(502));
    let (made, other) = (
        PairingSecret::generate().unwrap(),
        PairingSecret::generate().unwrap(),
    );
    let prologue = prologue(&named, Purpose::Pairing);
    // Ephemeral keys held fixed, so a second run makes the same messages up
    // to the secret: the station with the other secret completes it with a
    // twin of the guest, and so holds genuine keys for that secret.
    let (guest_ephemeral, station_ephemeral) = (*keys().secret(), *keys().secret());
    let built = |secret, own, ephemeral: &[u8; 32], initiator: bool| {
        let builder = pairing(own, secret, &prologue)
            .unwrap()
            .fixed_ephemeral_key_for_testing_only(ephemeral);
        if initiator {
            builder.build_initiator().unwrap()
        } else {
            builder.build_responder().unwrap()
        }
    };
    let mut real_guest = built(&made, &guest, &guest_ephemeral, true);
    let mut refusing = built(&other, &station, &station_ephemeral, false);
    assert!(
        !paired(&mut real_guest, &mut refusing),
        "the other secret was taken"
    );
    let mut twin = built(&other, &guest, &guest_ephemeral, true);
    let mut wrong_station = built(&other, &station, &station_ephemeral, false);
    assert!(paired(&mut twin, &mut wrong_station));
    let mut wrong_station = wrong_station.into_transport_mode().unwrap();
    let mut sealed = vec![0; FRAME];
    let length = wrong_station
        .write_message(&[DATA, b'x'], &mut sealed)
        .unwrap();

    let (near, mut far) = duplex(1 << 18);
    let mut guest_side = Secure::new(
        near,
        real_guest.into_transport_mode().unwrap(),
        station.public(),
    );
    write_frame(&mut far, &sealed[..length]).await.unwrap();

    let refused = received(&mut guest_side, 1).await.unwrap_err();
    assert_eq!(refused.kind(), io::ErrorKind::InvalidData);
    assert!(received(&mut guest_side, 1).await.is_err());
    assert!(guest_side.write_all(b"commit").await.is_err());
}

#[tokio::test]
async fn a_request_and_its_reply_go_over_a_lane_as_over_a_socket() {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let (guest, station) = connected(Wire::Faithful).await;
    let answering = tokio::spawn(async move {
        let mut guest = BufReader::new(guest);
        let mut request = String::new();
        guest.read_line(&mut request).await.unwrap();
        guest
            .write_all(b"{\"event\":\"step\"}\n{\"ok\":true,\"value\":7}\n")
            .await
            .unwrap();
        guest.shutdown().await.unwrap();
        request
    });

    let reply =
        atlas_core::transport::Pending::over_authenticated(station, Duration::from_secs(10))
            .dispatch(&serde_json::json!({"op": "hello"}))
            .await
            .unwrap()
            .finish()
            .await
            .unwrap();

    assert_eq!(answering.await.unwrap(), "{\"op\":\"hello\"}\n");
    assert_eq!(reply["value"], 7);
}

#[tokio::test]
async fn a_reply_cut_short_on_a_lane_is_an_unknown_outcome() {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let (guest, station) = connected(Wire::Faithful).await;
    tokio::spawn(async move {
        let mut guest = BufReader::new(guest);
        let mut request = String::new();
        guest.read_line(&mut request).await.unwrap();
        guest.write_all(b"{\"event\":\"step\"}\n").await.unwrap();
        guest.flush().await.unwrap();
        // Gone without finishing.
    });

    let failed =
        atlas_core::transport::Pending::over_authenticated(station, Duration::from_secs(10))
            .dispatch(&serde_json::json!({"op": "run"}))
            .await
            .unwrap()
            .finish()
            .await
            .unwrap_err();

    assert!(failed.uncertain);
}

/// A stream that fails its flushes once `failing` is set, and its
/// shutdown once `ending_fails` is.
struct Failing {
    inner: DuplexStream,
    failing: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ending_fails: bool,
}

impl AsyncRead for Failing {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(context, buffer)
    }
}

impl AsyncWrite for Failing {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(context, data)
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.failing.load(std::sync::atomic::Ordering::SeqCst) {
            return Poll::Ready(Err(io::Error::other("the flush failed")));
        }
        Pin::new(&mut self.inner).poll_flush(context)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.ending_fails {
            return Poll::Ready(Err(io::Error::other("the shutdown failed")));
        }
        Pin::new(&mut self.inner).poll_shutdown(context)
    }
}

/// A guest end over `Failing`, connected to a station.
async fn failing_guest(
    ending_fails: bool,
) -> (
    Secure<Failing>,
    Secure<DuplexStream>,
    std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    let (guest, station, named) = (keys(), keys(), desktop(502));
    let (guest_end, station_end) = duplex(1 << 18);
    let failing = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let guest_end = Failing {
        inner: guest_end,
        failing: failing.clone(),
        ending_fails,
    };
    let (guest_side, station_side) = tokio::join!(
        connect_as_guest(guest_end, &guest, station.public(), &named, Purpose::Lane),
        connect_as_station(station_end, &station, guest.public(), &named, Purpose::Lane),
    );
    (guest_side.unwrap(), station_side.unwrap(), failing)
}

#[tokio::test]
async fn a_stream_that_fails_to_shut_down_breaks_the_connection_for_good() {
    let (mut guest_side, _station_side, _) = failing_guest(true).await;

    assert!(guest_side.shutdown().await.is_err());

    let mut byte = [0; 1];
    assert!(guest_side.read(&mut byte).await.is_err());
    assert!(guest_side.shutdown().await.is_err());
}

#[tokio::test]
async fn a_stream_that_fails_to_flush_breaks_the_connection_for_good() {
    let (mut guest_side, _station_side, failing) = failing_guest(false).await;

    failing.store(true, std::sync::atomic::Ordering::SeqCst);
    guest_side.write_all(b"data").await.unwrap();
    assert!(guest_side.flush().await.is_err());

    assert!(guest_side.write_all(b"more").await.is_err());
    let mut byte = [0; 1];
    assert!(guest_side.read(&mut byte).await.is_err());
    assert!(guest_side.shutdown().await.is_err());
}

#[tokio::test(start_paused = true)]
async fn a_handshake_with_a_silent_peer_ends_at_the_callers_deadline() {
    let (guest, station, named) = (keys(), keys(), desktop(502));
    let (_silent, station_end) = duplex(1 << 18);

    let answered = tokio::time::timeout(
        Duration::from_secs(5),
        connect_as_station(
            station_end,
            &station,
            guest.public(),
            &named,
            Purpose::Control,
        ),
    )
    .await;

    assert!(answered.is_err(), "the handshake went on past its deadline");
}

#[tokio::test]
async fn a_frame_too_large_is_refused_before_any_of_it_is_written() {
    let (mut near, mut far) = duplex(1 << 18);

    let refused = write_frame(&mut near, &vec![0; FRAME + 1])
        .await
        .unwrap_err();
    drop(near);
    let mut written = Vec::new();
    far.read_to_end(&mut written).await.unwrap();

    assert_eq!(refused.code, Code::BadRequest);
    assert!(written.is_empty());
    let (mut near, mut far) = duplex(1 << 18);
    write_frame(&mut near, &vec![7; FRAME]).await.unwrap();
    let mut largest = vec![0; FRAME];
    assert_eq!(read_frame(&mut far, &mut largest).await.unwrap(), FRAME);
}

#[tokio::test]
async fn a_half_that_fails_wakes_the_other_half_waiting() {
    let (guest_side, _station_side, failing) = failing_guest(false).await;
    let (mut reading, mut writing) = tokio::io::split(guest_side);
    // The station says nothing, so the read waits.
    let reader = tokio::spawn(async move {
        let mut byte = [0; 1];
        reading.read(&mut byte).await
    });
    tokio::task::yield_now().await;

    failing.store(true, std::sync::atomic::Ordering::SeqCst);
    writing.write_all(b"data").await.unwrap();
    assert!(writing.flush().await.is_err());

    let woken = tokio::time::timeout(Duration::from_secs(5), reader).await;
    assert!(
        woken
            .expect("the waiting read was never woken")
            .unwrap()
            .is_err()
    );
}
