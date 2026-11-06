use super::*;
use atlas_core::{buffer::Buffer, frame_pipe::diagnostics::Lifecycle, protocol::ConnectionPhase};
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
    connection_from(daemon, side, CaptureBackend::CoreGraphics)
}

fn connection_from(
    daemon: Arc<Daemon>,
    side: u32,
    backend: CaptureBackend,
) -> (UnixStream, JoinHandle<Result<()>>) {
    let (client, server) = UnixStream::pair().unwrap();
    let (reader, writer) = server.into_split();
    (
        client,
        tokio::spawn(serve(daemon, reader, writer, side, pin(), backend)),
    )
}

/// A daemon whose native source stops as `stop` says.
fn stopping(stop: fn() -> Result<()>) -> Arc<Daemon> {
    let mut daemon = Arc::try_unwrap(daemon(raw)).ok().unwrap();
    daemon.source = Arc::new(Source::new(stop));
    Arc::new(daemon)
}

fn never_stops() -> Result<()> {
    Err(Error::new(
        Code::Capture,
        "Native stream stop is still pending",
    ))
}

async fn ask_stop(client: &mut UnixStream, id: u64) {
    client.write_all(&[0]).await.unwrap();
    client.write_all(&id.to_be_bytes()).await.unwrap();
}

async fn header(client: &mut UnixStream) -> Value {
    let size = client.read_u32().await.unwrap() as usize;
    assert!((1..=MAX_HEADER).contains(&size));
    let mut bytes = vec![0; size];
    client.read_exact(&mut bytes).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn accepted(client: &mut UnixStream) -> u64 {
    let header = header(client).await;
    assert_eq!(header["ok"], true);
    assert_eq!(header["stream"], "rgb-stream-v1");
    assert_eq!(header["daemonPID"], std::process::id());
    header["producerGeneration"].as_u64().unwrap()
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
    assert_eq!(daemon.source.lifecycle(), Lifecycle::Active);
    BLOCKED.store(false, Ordering::SeqCst);
    task.await.unwrap().unwrap();
    assert_eq!(daemon.source.lifecycle(), Lifecycle::Stopped);
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

#[tokio::test]
async fn a_stop_request_answers_only_once_the_source_is_drained() {
    let _serial = SERIAL.lock().await;
    reset();
    let daemon = daemon(raw);
    let (mut client, task) = connection(daemon.clone(), 640);
    let generation = accepted(&mut client).await;
    client.write_all(&[1]).await.unwrap();
    assert_eq!(header(&mut client).await["sequence"], 1);
    client.read_exact(&mut [0; 6]).await.unwrap();
    ask_stop(&mut client, 41).await;
    assert_eq!(
        header(&mut client).await,
        json!({"ok": true, "daemonPID": std::process::id(), "producerGeneration": generation,
            "stopRequestID": 41, "drained": true})
    );
    task.await.unwrap().unwrap();
    assert_eq!(daemon.source.lifecycle(), Lifecycle::Stopped);
    let query = "0123456789abcdef0123456789abcdef";
    let status = drain_status(
        &daemon.source,
        std::process::id(),
        generation,
        query,
        Some(41),
    );
    assert_eq!(status["state"], "stopped");
    assert_eq!(status["drained"], true);
    assert_eq!(status["stopRequestID"], 41);
    assert_eq!(status["queryID"], query);
    let other = drain_status(
        &daemon.source,
        std::process::id(),
        generation,
        query,
        Some(7),
    );
    assert!(
        other.get("stopRequestID").is_none(),
        "an unknown stop ID is never echoed"
    );
    let elsewhere = drain_status(
        &daemon.source,
        std::process::id() + 1,
        generation,
        query,
        None,
    );
    assert_eq!(
        (elsewhere["state"].clone(), elsewhere["drained"].clone()),
        (json!("unknown"), json!(false))
    );
}

#[tokio::test]
async fn an_unconfirmed_native_stop_keeps_every_capture_out_until_it_is_confirmed() {
    let _serial = SERIAL.lock().await;
    reset();
    let daemon = stopping(never_stops);
    let (mut client, task) = connection_from(daemon.clone(), 640, CaptureBackend::ScreenCaptureKit);
    let generation = accepted(&mut client).await;
    ask_stop(&mut client, 5).await;
    assert_eq!(header(&mut client).await["code"], "capture");
    task.await.unwrap().unwrap();
    assert_eq!(daemon.source.lifecycle(), Lifecycle::Quarantined);
    let status = drain_status(
        &daemon.source,
        std::process::id(),
        generation,
        "0123456789abcdef0123456789abcdef",
        Some(5),
    );
    assert_eq!(
        (status["state"].clone(), status["drained"].clone()),
        (json!("quarantined"), json!(false))
    );

    let (mut next, next_task) = connection(daemon.clone(), 640);
    assert_eq!(
        header(&mut next).await["code"],
        "busy",
        "a CoreGraphics stream cannot bypass it"
    );
    next_task.await.unwrap().unwrap();
    assert_eq!(CAPTURES.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_truncated_stop_request_still_gives_the_source_back() {
    let _serial = SERIAL.lock().await;
    reset();
    let daemon = daemon(raw);
    let (mut client, task) = connection(daemon.clone(), 640);
    accepted(&mut client).await;
    client.write_all(&[0, 1, 2]).await.unwrap();
    drop(client);
    task.await.unwrap().unwrap();
    assert_eq!(daemon.source.lifecycle(), Lifecycle::Stopped);
}

fn refused_consent(_: u32, _: Option<u32>) -> Result<RawShot> {
    Err(Error::new(
        Code::Permission,
        "Screen Recording is not allowed for Atlas in the helper account",
    ))
}

/// A connection that asked `daemon` for one frame pinned to display
/// `display`, and the daemon's side of it.
async fn asked(daemon: Arc<Daemon>, display: u64) -> (UnixStream, JoinHandle<Result<()>>) {
    let (mut client, server) = UnixStream::pair().unwrap();
    let task = tokio::spawn(super::super::handle(
        server,
        daemon,
        true,
        watch::channel(ConnectionPhase::Refusal).0,
    ));
    let request = json!({"op":"frame","maxDimension":1280,"session":{"securitySessionId":42,"displayId":display}});
    client
        .write_all(format!("{request}\n").as_bytes())
        .await
        .unwrap();
    (client, task)
}

/// One frame asked of `daemon` pinned to display `display`: its header,
/// and every byte after it.
async fn one_frame(daemon: Arc<Daemon>, display: u64) -> (Value, Vec<u8>) {
    let (mut client, task) = asked(daemon, display).await;
    let header = header(&mut client).await;
    let mut rest = Vec::new();
    client.read_to_end(&mut rest).await.unwrap();
    task.await.unwrap().unwrap();
    (header, rest)
}

#[tokio::test]
async fn one_frame_comes_as_raw_pixels_after_a_header_that_proves_its_session() {
    let _serial = SERIAL.lock().await;
    reset();
    let daemon = daemon(raw);
    let (header, pixels) = one_frame(daemon.clone(), 7).await;
    assert_eq!(header["ok"], true);
    assert_eq!(
        (&header["width"], &header["height"]),
        (&json!(2), &json!(1))
    );
    assert_eq!(header["session"]["displayID"], 7);
    assert_eq!(header["daemonPID"], std::process::id());
    assert_eq!(pixels, [11, 22, 33, 44, 55, 66]);
    assert_eq!(CAPTURES.load(Ordering::SeqCst), 1);
    assert_eq!(
        daemon.source.lifecycle(),
        Lifecycle::Stopped,
        "it needs no live source"
    );
}

#[tokio::test]
async fn one_frame_is_refused_on_screen_in_another_session_and_without_consent() {
    let _serial = SERIAL.lock().await;
    // The capture a refusal comes after, if any: one the helper left mid-way.
    for (capture, on_screen, display, code, captures) in [
        (
            raw as fn(u32, Option<u32>) -> Result<RawShot>,
            true,
            7,
            "on-console",
            0,
        ),
        (raw, false, 8, "session-changed", 0),
        (changed, false, 7, "session-changed", 1),
        (console, false, 7, "on-console", 1),
        (refused_consent, false, 7, "permission", 0),
    ] {
        reset();
        CONSOLE.store(on_screen, Ordering::SeqCst);
        let (header, pixels) = one_frame(daemon(capture), display).await;
        assert_eq!(header["ok"], false, "{code}");
        assert_eq!(header["code"], code);
        assert!(pixels.is_empty(), "no pixel leaves the daemon: {code}");
        assert_eq!(CAPTURES.load(Ordering::SeqCst), captures, "{code}");
    }
}

#[tokio::test]
async fn one_frame_waits_out_no_unconfirmed_live_source() {
    let _serial = SERIAL.lock().await;
    reset();
    let daemon = stopping(never_stops);
    let (mut client, task) = connection_from(daemon.clone(), 640, CaptureBackend::ScreenCaptureKit);
    accepted(&mut client).await;
    ask_stop(&mut client, 5).await;
    header(&mut client).await;
    task.await.unwrap().unwrap();

    let (header, pixels) = one_frame(daemon, 7).await;
    assert_eq!(header["code"], "busy");
    assert!(pixels.is_empty());
    assert_eq!(CAPTURES.load(Ordering::SeqCst), 0);
}

static READS: AtomicUsize = AtomicUsize::new(0);
static HELD: AtomicBool = AtomicBool::new(false);
static WAITING: AtomicBool = AtomicBool::new(false);

/// The helper's session, its third reading, a capture's first check, held
/// while [`HELD`] is: after admission and the epoch, before the capture
/// can begin.
fn held_before_capture() -> Result<Value> {
    if READS.fetch_add(1, Ordering::SeqCst) == 2 {
        WAITING.store(true, Ordering::SeqCst);
        while HELD.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    session()
}

async fn until(flag: &AtomicBool) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while !flag.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn a_cancelled_or_abandoned_frame_never_begins_its_capture() {
    let _serial = SERIAL.lock().await;
    for leaves in [false, true] {
        reset();
        READS.store(0, Ordering::SeqCst);
        HELD.store(true, Ordering::SeqCst);
        WAITING.store(false, Ordering::SeqCst);
        let mut daemon = Daemon::new(
            "Aqua".into(),
            |_, _, _| -> Result<Shot> { panic!("A frame must never encode PNG") },
            || false,
            held_before_capture,
        );
        daemon.screen.raw_capture = raw;
        let (mut client, task) = asked(Arc::new(daemon), 7).await;
        until(&WAITING).await;
        if leaves {
            drop(client);
            tokio::time::sleep(Duration::from_millis(15)).await;
            HELD.store(false, Ordering::SeqCst);
        } else {
            client.write_all(&[0]).await.unwrap();
            let answered = header(&mut client).await;
            assert_eq!(
                answered["code"], "cancelled",
                "answered before it could begin"
            );
            HELD.store(false, Ordering::SeqCst);
        }
        task.await.unwrap().unwrap();
        // The worker is joined by the source before the next test runs.
        tokio::time::sleep(Duration::from_millis(15)).await;
        assert_eq!(CAPTURES.load(Ordering::SeqCst), 0, "leaves: {leaves}");
    }
}

#[tokio::test]
async fn a_cancel_is_answered_only_once_a_begun_capture_has_ended() {
    let _serial = SERIAL.lock().await;
    reset();
    BLOCKED.store(true, Ordering::SeqCst);
    let (mut client, task) = asked(daemon(blocking), 7).await;
    until(&STARTED).await;
    client.write_all(&[0]).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(15), header(&mut client))
            .await
            .is_err(),
        "not while the capture runs"
    );
    BLOCKED.store(false, Ordering::SeqCst);
    let answered = header(&mut client).await;
    assert_eq!(answered["code"], "cancelled");
    let mut rest = Vec::new();
    client.read_to_end(&mut rest).await.unwrap();
    assert!(rest.is_empty(), "its pixels never leave the daemon");
    task.await.unwrap().unwrap();
    assert_eq!(CAPTURES.load(Ordering::SeqCst), 1);
}

/// The helper's session, on another display from the fifth reading on: after
/// admission, the epoch and both of a capture's checks.
fn moves_once_captured() -> Result<Value> {
    let mut proof = session()?;
    if READS.fetch_add(1, Ordering::SeqCst) >= 4 {
        proof["displayID"] = json!(8);
    }
    Ok(proof)
}

#[tokio::test]
async fn one_frame_is_checked_again_once_its_worker_is_back() {
    let _serial = SERIAL.lock().await;
    reset();
    READS.store(0, Ordering::SeqCst);
    let mut daemon = Daemon::new(
        "Aqua".into(),
        |_, _, _| -> Result<Shot> { panic!("A frame must never encode PNG") },
        || CONSOLE.load(Ordering::SeqCst),
        moves_once_captured,
    );
    daemon.screen.raw_capture = raw;
    let (header, pixels) = one_frame(Arc::new(daemon), 7).await;
    assert_eq!(header["code"], "session-changed");
    assert!(pixels.is_empty(), "the picture never leaves the daemon");
    assert_eq!(CAPTURES.load(Ordering::SeqCst), 1);
}
