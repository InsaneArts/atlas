use super::*;
use atlas_core::{buffer::Buffer, protocol::ConnectionPhase};
use atlas_native::native::capture::Shot;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::{
    net::UnixStream,
    sync::{Mutex, watch},
    task::JoinHandle,
};

static SERIAL: Mutex<()> = Mutex::const_new(());
static CAPTURES: AtomicUsize = AtomicUsize::new(0);
static MOVED: AtomicBool = AtomicBool::new(false);
static CONSOLE: AtomicBool = AtomicBool::new(false);
static BLOCKED: AtomicBool = AtomicBool::new(false);
static STARTED: AtomicBool = AtomicBool::new(false);

fn session() -> Result<Value> {
    Ok(
        json!({"ok":true,"uid":502,"securitySessionID":42,"displayID":if MOVED.load(Ordering::SeqCst) {8} else {7},"hasGraphicAccess":true,"onConsole":false,"width":64.0,"height":36.0}),
    )
}

fn raw(_: u32, _: Option<u32>) -> Result<RawShot> {
    CAPTURES.fetch_add(1, Ordering::SeqCst);
    Ok(RawShot {
        rgb: Buffer::copy_from(&[11, 22, 33, 44, 55, 66]).unwrap(),
        width: 2,
        height: 1,
        scale: 2.0,
    })
}

fn changed(display: u32, side: Option<u32>) -> Result<RawShot> {
    let shot = raw(display, side)?;
    MOVED.store(true, Ordering::SeqCst);
    Ok(shot)
}

fn console(display: u32, side: Option<u32>) -> Result<RawShot> {
    let shot = raw(display, side)?;
    CONSOLE.store(true, Ordering::SeqCst);
    Ok(shot)
}

fn blocking(display: u32, side: Option<u32>) -> Result<RawShot> {
    STARTED.store(true, Ordering::SeqCst);
    while BLOCKED.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(1));
    }
    raw(display, side)
}

fn large(_: u32, _: Option<u32>) -> Result<RawShot> {
    CAPTURES.fetch_add(1, Ordering::SeqCst);
    Ok(RawShot {
        rgb: Buffer::zeroed(1600 * 1600 * 3).unwrap(),
        width: 1600,
        height: 1600,
        scale: 1.0,
    })
}

fn reset() {
    CAPTURES.store(0, Ordering::SeqCst);
    MOVED.store(false, Ordering::SeqCst);
    CONSOLE.store(false, Ordering::SeqCst);
    BLOCKED.store(false, Ordering::SeqCst);
    STARTED.store(false, Ordering::SeqCst);
}

fn daemon(raw_capture: fn(u32, Option<u32>) -> Result<RawShot>) -> Arc<Daemon> {
    let mut daemon = Daemon::new(
        "Aqua".into(),
        |_, _, _| -> Result<Shot> { panic!("A live capture must never encode PNG") },
        || CONSOLE.load(Ordering::SeqCst),
        session,
    );
    daemon.screen.raw_capture = raw_capture;
    Arc::new(daemon)
}

fn pin() -> SessionPin {
    SessionPin {
        security_session_id: 42,
        display_id: 7,
    }
}

fn connection(daemon: Arc<Daemon>, side: u32) -> (UnixStream, JoinHandle<Result<()>>) {
    let (client, server) = UnixStream::pair().unwrap();
    let (reader, writer) = server.into_split();
    (
        client,
        tokio::spawn(serve(
            daemon,
            reader,
            writer,
            side,
            pin(),
            CaptureBackend::CoreGraphics,
        )),
    )
}

async fn header(client: &mut UnixStream) -> Value {
    let size = client.read_u32().await.unwrap() as usize;
    assert!((1..=MAX_HEADER).contains(&size));
    let mut bytes = vec![0; size];
    client.read_exact(&mut bytes).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn accepted(client: &mut UnixStream) {
    assert_eq!(
        header(client).await,
        json!({"ok":true,"stream":"rgb-stream-v1"})
    );
}

#[tokio::test]
async fn live_frames_wait_for_readiness_and_send_only_pixels_without_a_codec() {
    let _serial = SERIAL.lock().await;
    reset();
    let (mut client, task) = connection(daemon(raw), 640);
    accepted(&mut client).await;
    tokio::time::sleep(Duration::from_millis(15)).await;
    assert_eq!(
        CAPTURES.load(Ordering::SeqCst),
        0,
        "No capture may precede the controller's independent proof"
    );
    for sequence in 1..=2 {
        client.write_all(&[1]).await.unwrap();
        let metadata = header(&mut client).await;
        assert_eq!(metadata["ok"], true);
        assert_eq!(metadata["sequence"], sequence);
        assert_eq!(metadata["daemonPID"], std::process::id());
        assert_eq!(metadata["session"]["displayID"], 7);
        assert_eq!(metadata["width"], 2);
        assert_eq!(metadata["height"], 1);
        let mut rgb = [0; 6];
        client.read_exact(&mut rgb).await.unwrap();
        assert_eq!(rgb, [11, 22, 33, 44, 55, 66]);
        assert_eq!(CAPTURES.load(Ordering::SeqCst), sequence as usize);
    }
    drop(client);
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn a_session_or_console_change_discards_the_entire_captured_picture() {
    let _serial = SERIAL.lock().await;
    for (capture, code) in [
        (
            changed as fn(u32, Option<u32>) -> Result<RawShot>,
            Code::SessionChanged,
        ),
        (console, Code::OnConsole),
    ] {
        reset();
        let (mut client, task) = connection(daemon(capture), 640);
        accepted(&mut client).await;
        client.write_all(&[1]).await.unwrap();
        let refusal = header(&mut client).await;
        assert_eq!(refusal["ok"], false);
        assert_eq!(refusal["code"], serde_json::to_value(code).unwrap());
        assert_eq!(
            client.read(&mut [0]).await.unwrap(),
            0,
            "Rejected pixels must not reach the terminal"
        );
        task.await.unwrap().unwrap();
    }
}

#[tokio::test]
async fn a_second_stream_is_refused_and_bad_readiness_cannot_trigger_capture() {
    let _serial = SERIAL.lock().await;
    reset();
    let daemon = daemon(raw);
    let (mut first, first_task) = connection(daemon.clone(), 640);
    accepted(&mut first).await;
    let (mut second, second_task) = connection(daemon, 640);
    assert_eq!(header(&mut second).await["code"], "busy");
    second_task.await.unwrap().unwrap();
    first.write_all(&[2]).await.unwrap();
    assert_eq!(header(&mut first).await["code"], "bad-request");
    first_task.await.unwrap().unwrap();
    assert_eq!(CAPTURES.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_departed_reader_cannot_leave_an_orphan_capture_or_release_its_slot_early() {
    let _serial = SERIAL.lock().await;
    reset();
    BLOCKED.store(true, Ordering::SeqCst);
    let daemon = daemon(blocking);
    let (mut client, mut task) = connection(daemon.clone(), 640);
    accepted(&mut client).await;
    client.write_all(&[1]).await.unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        while !STARTED.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    drop(client);
    assert!(
        tokio::time::timeout(Duration::from_millis(15), &mut task)
            .await
            .is_err()
    );
    assert_eq!(daemon.stream_slot.available_permits(), 0);
    BLOCKED.store(false, Ordering::SeqCst);
    task.await.unwrap().unwrap();
    assert_eq!(daemon.stream_slot.available_permits(), 1);
}

#[tokio::test]
async fn a_stalled_reader_is_disconnected_without_queuing_the_next_frame() {
    let _serial = SERIAL.lock().await;
    reset();
    let (mut client, task) = connection(daemon(large), 1600);
    accepted(&mut client).await;
    client.write_all(&[1]).await.unwrap();
    let error = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, Code::Transport);
    assert_eq!(CAPTURES.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn the_stream_routes_controller_and_pin_refusals_as_binary_headers() {
    let _serial = SERIAL.lock().await;
    reset();
    for (controller, display, code) in [(false, 7, "permission"), (true, 8, "session-changed")] {
        let (mut client, server) = UnixStream::pair().unwrap();
        let task = tokio::spawn(super::super::handle(
            server,
            daemon(raw),
            controller,
            watch::channel(ConnectionPhase::Refusal).0,
        ));
        let request = json!({"op":"stream","maxDimension":640,"session":{"securitySessionId":42,"displayId":display}});
        client
            .write_all(format!("{request}\n").as_bytes())
            .await
            .unwrap();
        let refusal = header(&mut client).await;
        assert_eq!(refusal["code"], code);
        task.await.unwrap().unwrap();
    }
    assert_eq!(CAPTURES.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn readiness_follows_the_acceptance_handshake_after_the_initial_request_line() {
    let _serial = SERIAL.lock().await;
    reset();
    let (mut client, server) = UnixStream::pair().unwrap();
    let task = tokio::spawn(super::super::handle(
        server,
        daemon(raw),
        true,
        watch::channel(ConnectionPhase::Refusal).0,
    ));
    let request =
        json!({"op":"stream","maxDimension":640,"session":{"securitySessionId":42,"displayId":7}});
    client
        .write_all(format!("{request}\n").as_bytes())
        .await
        .unwrap();
    accepted(&mut client).await;
    assert_eq!(CAPTURES.load(Ordering::SeqCst), 0);
    client.write_all(&[1]).await.unwrap();
    assert_eq!(header(&mut client).await["sequence"], 1);
    let mut rgb = [0; 6];
    client.read_exact(&mut rgb).await.unwrap();
    assert_eq!(rgb, [11, 22, 33, 44, 55, 66]);
    drop(client);
    task.await.unwrap().unwrap();
}
