use super::*;
use crate::native::{events::Engine, lease::Gate};
use atlas_core::session::InputEpoch;
use std::sync::RwLock;
use tokio::{net::unix::OwnedReadHalf as Reading, sync::Mutex};

/// A machine desktop's service on an idle engine, its files in `folder`.
fn service(folder: &std::path::Path) -> Arc<Service> {
    let epoch = InputEpoch {
        uid: 0,
        security_session_id: 0,
        display_id: 0,
        width: 1.0,
        height: 1.0,
        process_id: std::process::id(),
    };
    let engine = Engine::idle(epoch.clone());
    Arc::new(Service {
        looks: engine.looks.clone(),
        engine: Arc::new(Mutex::new(engine)),
        operations: super::super::Operations::default(),
        gate: Arc::new(Gate::machine()),
        epoch: Arc::new(RwLock::new(epoch)),
        build: None,
        executable: None,
        stopping: tokio::sync::Notify::new(),
        stop_marker: folder.join("input.stopped"),
        activity: folder.join("input.activity"),
        seen: Arc::default(),
        look: Arc::new(super::super::LiveLook),
        outcomes: Arc::default(),
    })
}

/// The service serving over a channel, and the daemon's ends of it.
struct Served {
    service: Arc<Service>,
    reading: Reading,
    writing: tokio::net::unix::OwnedWriteHalf,
    serving: tokio::task::JoinHandle<atlas_core::error::Result<()>>,
    _folder: tempfile::TempDir,
}

fn serve() -> Served {
    let folder = tempfile::tempdir().unwrap();
    let service = service(folder.path());
    let (daemon, theirs) = UnixStream::pair().unwrap();
    let channel = Channel::open(theirs, service.clone());
    let settler = channel.settler();
    let serving = tokio::spawn({
        let service = service.clone();
        async move {
            service
                .listen(super::super::Source::Channel(channel), Some(settler))
                .await
        }
    });
    let (reading, writing) = daemon.into_split();
    Served {
        service,
        reading,
        writing,
        serving,
        _folder: folder,
    }
}

/// The next frame the service says, within 5 s.
async fn heard(reading: &mut Reading) -> Frame {
    tokio::time::timeout(Duration::from_secs(5), Frame::read(reading))
        .await
        .expect("the service said nothing")
        .expect("a frame")
}

/// A deadline `ahead` from now on the continuous clock.
fn due(ahead: Duration) -> u64 {
    let now = nix::time::clock_gettime(nix::time::ClockId::CLOCK_MONOTONIC).unwrap();
    u64::try_from(now.tv_sec()).unwrap() * 1_000_000_000
        + u64::try_from(now.tv_nsec()).unwrap()
        + u64::try_from(ahead.as_nanos()).unwrap()
}

#[tokio::test]
async fn a_connection_its_daemon_opens_is_served_as_on_the_helper_login() {
    let mut served = serve();

    Frame::Open(1, 1).write(&mut served.writing).await.unwrap();
    Frame::Data(1, b"{\"op\":\"hello\"}\n".to_vec())
        .write(&mut served.writing)
        .await
        .unwrap();

    let mut said = Vec::new();
    loop {
        match heard(&mut served.reading).await {
            Frame::Data(1, bytes) => said.extend(bytes),
            Frame::Close(1) => break,
            other => panic!("unexpected {other:?}"),
        }
        if said.ends_with(b"\n") {
            break;
        }
    }
    let reply: serde_json::Value = serde_json::from_slice(&said).unwrap();
    assert!(reply.get("ok").is_some(), "{reply}");
}

#[tokio::test]
async fn what_is_not_a_frame_ends_the_service() {
    let mut served = serve();

    served.writing.write_all(&[99; 17]).await.unwrap();

    let ended = tokio::time::timeout(Duration::from_secs(5), served.serving).await;
    assert!(matches!(ended, Ok(Ok(Ok(())))), "{ended:?}");
}

#[tokio::test]
async fn the_channel_ending_ends_the_service() {
    let served = serve();

    drop(served.writing);

    let ended = tokio::time::timeout(Duration::from_secs(5), served.serving).await;
    assert!(matches!(ended, Ok(Ok(Ok(())))), "{ended:?}");
}

#[tokio::test]
async fn a_revocation_is_said_at_once_and_settled_once_its_work_is_over() {
    let mut served = serve();
    Frame::Renew(1, due(Duration::from_secs(5)))
        .write(&mut served.writing)
        .await
        .unwrap();

    Frame::Revoke(1).write(&mut served.writing).await.unwrap();

    assert_eq!(heard(&mut served.reading).await, Frame::Revoked(1));
    assert_eq!(heard(&mut served.reading).await, Frame::Settled(1));
    assert!(!served.service.gate.owns_through(1).unwrap());
}

#[tokio::test]
async fn a_hold_is_said_once_it_took_effect_and_an_older_word_is_dropped() {
    let mut served = serve();

    Frame::Hold { seq: 2, on: true }
        .write(&mut served.writing)
        .await
        .unwrap();
    assert_eq!(
        heard(&mut served.reading).await,
        Frame::Held { seq: 2, on: true }
    );
    Frame::Hold { seq: 1, on: false }
        .write(&mut served.writing)
        .await
        .unwrap();
    Frame::Hold { seq: 3, on: true }
        .write(&mut served.writing)
        .await
        .unwrap();

    assert_eq!(
        heard(&mut served.reading).await,
        Frame::Held { seq: 3, on: true }
    );
    assert!(served.service.gate.held().unwrap());
}

#[tokio::test]
async fn a_frame_the_daemon_never_sends_ends_the_service() {
    let mut served = serve();

    Frame::Settled(1).write(&mut served.writing).await.unwrap();

    let ended = tokio::time::timeout(Duration::from_secs(5), served.serving).await;
    assert!(matches!(ended, Ok(Ok(Ok(())))), "{ended:?}");
}

#[tokio::test]
async fn data_for_a_connection_never_opened_ends_the_service() {
    let mut served = serve();

    Frame::Data(5, b"x".to_vec())
        .write(&mut served.writing)
        .await
        .unwrap();

    let ended = tokio::time::timeout(Duration::from_secs(5), served.serving).await;
    assert!(matches!(ended, Ok(Ok(Ok(())))), "{ended:?}");
}

#[tokio::test]
async fn a_connection_number_used_before_ends_the_service() {
    let mut served = serve();
    Frame::Open(1, 1).write(&mut served.writing).await.unwrap();

    Frame::Open(1, 1).write(&mut served.writing).await.unwrap();

    let ended = tokio::time::timeout(Duration::from_secs(5), served.serving).await;
    assert!(matches!(ended, Ok(Ok(Ok(())))), "{ended:?}");
}

#[tokio::test]
async fn a_connection_numbered_out_of_turn_ends_the_service() {
    let mut served = serve();

    Frame::Open(2, 1).write(&mut served.writing).await.unwrap();

    let ended = tokio::time::timeout(Duration::from_secs(5), served.serving).await;
    assert!(matches!(ended, Ok(Ok(Ok(())))), "{ended:?}");
}

#[tokio::test]
async fn closing_a_connection_never_opened_ends_the_service() {
    let mut served = serve();
    Frame::Open(1, 1).write(&mut served.writing).await.unwrap();

    Frame::Close(0).write(&mut served.writing).await.unwrap();

    let ended = tokio::time::timeout(Duration::from_secs(5), served.serving).await;
    assert!(matches!(ended, Ok(Ok(Ok(())))), "{ended:?}");
}
