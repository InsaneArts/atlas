use super::*;
use atlas_core::buffer::Buffer;
use atlas_native::native::capture::RawShot;
use atlas_session::{
    control::{Outcome, Said},
    credit::{BULK, CHUNK},
    station::Opened,
};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize};
use tokio::io::AsyncReadExt;

/// The session the guest proves, as a pin names it.
fn session() -> Value {
    json!({"securitySessionId": 42, "displayId": 7})
}

/// A pinned frame as large as a VM gives one.
fn frame() -> Value {
    json!({"op": "frame", "maxDimension": 1600, "session": session()})
}

const WIDE: (u32, u32) = (800, 600);

/// The bytes of a frame of [`WIDE`], more than the whole bulk credit.
const PIXELS: usize = (WIDE.0 * WIDE.1 * 3) as usize;

/// A raw frame of [`WIDE`], its bytes numbered so a frame cut short or
/// out of order shows.
fn wide(display: u32, _: Option<u32>) -> atlas_core::error::Result<RawShot> {
    assert_eq!(display, 7);
    let mut rgb = Buffer::zeroed(PIXELS).unwrap();
    for (at, byte) in rgb.iter_mut().enumerate() {
        *byte = (at % 251) as u8;
    }
    Ok(RawShot {
        rgb,
        width: WIDE.0,
        height: WIDE.1,
        scale: 1.0,
    })
}

/// A guest whose screen gives frames of [`WIDE`].
fn widely(daemon: &mut Daemon) {
    daemon.screen.raw_capture = wide;
}

/// The bulk credit of the guest's live generation.
async fn guest_credit(served: &Served) -> Arc<Credit> {
    served
        .machine
        .live
        .lock()
        .await
        .as_ref()
        .expect("a live generation")
        .credit
        .clone()
}

/// Open a picture lane on `desktop` and say `request` on it.
async fn picture(desktop: &Desktop, request: &Value) -> Opened {
    let mut opened = desktop.open(LaneKind::Picture).await.unwrap();
    let mut line = serde_json::to_vec(request).unwrap();
    line.push(b'\n');
    opened.stream.write_all(&line).await.unwrap();
    opened.stream.flush().await.unwrap();
    opened
}

/// The station reading a picture lane: every read given back over
/// `controlling` after `pause`.
struct Reading<'a> {
    opened: Opened,
    controlling: &'a Controlling,
    pause: Duration,
    came: usize,
    repaid: usize,
    /// The most read and not yet given back at once.
    most: usize,
}

impl<'a> Reading<'a> {
    fn of(opened: Opened, controlling: &'a Controlling, pause: Duration) -> Self {
        Self {
            opened,
            controlling,
            pause,
            came: 0,
            repaid: 0,
            most: 0,
        }
    }

    /// Fill `into` whole, giving back each read.
    async fn exactly(&mut self, into: &mut [u8]) {
        let mut at = 0;
        while at < into.len() {
            let read = tokio::time::timeout(
                Duration::from_secs(10),
                self.opened.stream.read(&mut into[at..]),
            )
            .await
            .expect("more of the lane in time")
            .unwrap();
            assert_ne!(read, 0, "the lane ended at {at} of {}", into.len());
            at += read;
            self.came += read;
            self.most = self.most.max(self.came - self.repaid);
            tokio::time::sleep(self.pause).await;
            self.give_back(read);
        }
    }

    fn give_back(&mut self, bytes: usize) {
        self.repaid += bytes;
        self.controlling
            .outbox
            .say(Said::Credit {
                lane: self.opened.lane,
                bytes: bytes as u64,
            })
            .unwrap();
    }

    /// A frame's header, as a live frame is framed.
    async fn framed_header(&mut self) -> Value {
        let mut size = [0; 4];
        self.exactly(&mut size).await;
        let mut header = vec![0; u32::from_be_bytes(size) as usize];
        self.exactly(&mut header).await;
        serde_json::from_slice(&header).unwrap()
    }

    /// A frame's header and its pixels, as a live frame is framed.
    async fn framed(&mut self) -> (Value, Vec<u8>) {
        let header = self.framed_header().await;
        let side = |name: &str| header[name].as_u64().unwrap_or(0);
        let length = side("width") * side("height") * 3;
        let mut pixels = vec![0; length as usize];
        self.exactly(&mut pixels).await;
        (header, pixels)
    }

    /// One line of the lane's reply.
    async fn line(&mut self) -> Value {
        let mut line = Vec::new();
        let mut byte = [0; 1];
        loop {
            self.exactly(&mut byte).await;
            if byte[0] == b'\n' {
                return serde_json::from_slice(&line).unwrap();
            }
            line.push(byte[0]);
        }
    }

    /// Whether the lane ends, with nothing more, within 5 s.
    async fn ends(&mut self) -> bool {
        let mut rest = Vec::new();
        let reading = self.opened.stream.read_to_end(&mut rest);
        let ended = tokio::time::timeout(Duration::from_secs(5), reading).await;
        ended.is_ok() && rest.is_empty()
    }
}

/// Wait until `credit` has `left` to spend, within 10 s.
async fn left_at(credit: &Credit, left: usize) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while credit.left() != left {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{} left, not {left}", credit.left()));
}

/// Numbered bytes of `length`, as [`wide`] gives them.
fn numbered(length: usize) -> Vec<u8> {
    (0..length).map(|at| (at % 251) as u8).collect()
}

#[tokio::test]
async fn a_screenshot_on_a_read_lane_or_unpinned_on_a_picture_lane_is_refused_before_it_runs() {
    static CAPTURES: AtomicUsize = AtomicUsize::new(0);
    let served = Served::seeing(records(), |daemon| {
        daemon.screen.capture = |display, area, limit| {
            CAPTURES.fetch_add(1, SeqCst);
            synthetic(display, area, limit)
        };
    })
    .await;
    let (_controlling, desktop) = served.control().await;

    for (kind, request) in [
        (
            LaneKind::Read,
            json!({"op": "screenshot", "session": session()}),
        ),
        (LaneKind::Picture, json!({"op": "screenshot"})),
        (
            LaneKind::Picture,
            json!({"op": "read", "path": "/private/tmp/notes", "maxSize": 1}),
        ),
        (
            LaneKind::Picture,
            json!({"op": "apps", "session": session()}),
        ),
    ] {
        let replies = ask(&desktop, kind.clone(), &request).await;
        assert_eq!(replies.len(), 1, "{kind:?} {request}");
        assert_eq!(replies[0]["code"], "bad-request", "{kind:?} {request}");
    }
    assert_eq!(CAPTURES.load(SeqCst), 0, "nothing captured");
}

#[tokio::test]
async fn a_pinned_screenshot_on_a_picture_lane_is_answered_no_larger_than_a_vm_gives() {
    static LIMIT: AtomicU32 = AtomicU32::new(0);
    let served = Served::seeing(records(), |daemon| {
        daemon.screen.capture = |display, area, limit| {
            LIMIT.store(limit.unwrap_or(u32::MAX), SeqCst);
            synthetic(display, area, limit)
        };
    })
    .await;
    let (controlling, desktop) = served.control().await;
    let opened = picture(
        &desktop,
        &json!({"op": "screenshot", "maxDimension": 4000, "session": session()}),
    )
    .await;
    let mut reading = Reading::of(opened, &controlling, Duration::ZERO);

    let shot = reading.line().await;

    assert_eq!(shot["ok"], true, "{shot}");
    assert!(shot["png"].is_string(), "{shot}");
    assert_eq!(LIMIT.load(SeqCst), 1600);
    assert!(reading.ends().await);
    left_at(&*guest_credit(&served).await, BULK as usize).await;
}

#[tokio::test]
async fn a_pinned_frame_arrives_whole_with_at_most_two_chunks_on_their_way_to_a_slow_reader() {
    let served = Served::seeing(records(), widely).await;
    let (controlling, desktop) = served.control().await;
    let credit = guest_credit(&served).await;
    let opened = picture(&desktop, &frame()).await;
    let mut reading = Reading::of(opened, &controlling, Duration::from_millis(2));

    let (header, pixels) = reading.framed().await;

    assert_eq!(header["ok"], true, "{header}");
    assert_eq!(
        (header["width"].as_u64(), header["height"].as_u64()),
        (Some(800), Some(600))
    );
    assert!(pixels == numbered(PIXELS), "the frame whole and in order");
    assert!(PIXELS > BULK as usize, "more than all the credit");
    assert!(
        reading.most <= 2 * CHUNK,
        "{} on their way at once",
        reading.most
    );
    assert!(reading.ends().await);
    left_at(&credit, BULK as usize).await;
}

#[tokio::test]
async fn a_cancelled_lane_ends_its_stream_mid_frame_and_owes_nothing() {
    let served = Served::seeing(records(), widely).await;
    let (controlling, desktop) = served.control().await;
    let credit = guest_credit(&served).await;
    let opened = picture(
        &desktop,
        &json!({"op": "stream", "maxDimension": 1600, "session": session()}),
    )
    .await;
    let lane = opened.lane;
    let mut reading = Reading::of(opened, &controlling, Duration::ZERO);
    let started = reading.framed_header().await;
    assert_eq!(started["stream"], "rgb-stream-v1", "{started}");
    reading.opened.stream.write_all(&[1]).await.unwrap();
    reading.opened.stream.flush().await.unwrap();
    let mut part = vec![0; 3 * CHUNK];
    reading.exactly(&mut part).await;

    controlling.outbox.say(Said::Cancel { lane }).unwrap();

    let mut rest = Vec::new();
    tokio::time::timeout(
        Duration::from_secs(5),
        reading.opened.stream.read_to_end(&mut rest),
    )
    .await
    .expect("the lane ended")
    .ok();
    assert!(
        reading.came + rest.len() < PIXELS,
        "ended mid-frame: {}",
        reading.came + rest.len()
    );
    left_at(&credit, BULK as usize).await;
}

/// Open `count` frame lanes on `desktop` whose reader reads nothing and
/// gives nothing back, once all the guest's credit is spent on them.
async fn stalled(served: &Served, desktop: &Desktop, count: usize) -> Vec<Opened> {
    let mut lanes = Vec::new();
    for _ in 0..count {
        lanes.push(picture(desktop, &frame()).await);
    }
    left_at(&*guest_credit(served).await, 0).await;
    lanes
}

#[tokio::test]
async fn control_lost_while_credit_is_spent_ends_every_pump_and_owes_nothing_with_lanes_still_open()
{
    let served = Served::seeing(records(), widely).await;
    let (controlling, desktop, connection) = served.control_cut().await;
    let credit = guest_credit(&served).await;
    let mut lanes = stalled(&served, &desktop, 12).await;

    connection.cut();

    left_at(&credit, BULK as usize).await;
    for lane in &mut lanes {
        let mut rest = Vec::new();
        let read =
            tokio::time::timeout(Duration::from_secs(5), lane.stream.read_to_end(&mut rest)).await;
        assert!(read.is_ok(), "the guest ended its side of the lane");
        assert!(rest.len() <= 2 * CHUNK, "{}", rest.len());
    }
    drop(controlling);
}

#[tokio::test]
async fn a_reader_that_stalls_past_the_deadline_is_closed_and_refunded_and_another_lane_goes_on() {
    let served = Served::seeing(records(), widely).await;
    served
        .machine
        .progressing_within(Duration::from_millis(300));
    let (controlling, desktop) = served.control().await;
    let credit = guest_credit(&served).await;
    let mut stalled = picture(&desktop, &frame()).await;

    let mut sent = Vec::new();
    tokio::time::timeout(
        Duration::from_secs(5),
        stalled.stream.read_to_end(&mut sent),
    )
    .await
    .expect("closed by the guest")
    .ok();
    assert!(sent.len() <= 2 * CHUNK, "{}", sent.len());
    left_at(&credit, BULK as usize).await;
    assert!(served.retired().is_empty(), "control stayed healthy");

    let opened = picture(&desktop, &frame()).await;
    let mut reading = Reading::of(opened, &controlling, Duration::ZERO);
    let (header, pixels) = reading.framed().await;
    assert_eq!(header["ok"], true, "{header}");
    assert_eq!(pixels.len(), PIXELS);
    left_at(&credit, BULK as usize).await;
}

#[tokio::test]
async fn a_stop_on_control_is_answered_while_pictures_spend_all_credit_and_never_holds_them() {
    let served = Served::seeing(records(), widely).await;
    let (controlling, desktop) = served.control().await;
    let credit = guest_credit(&served).await;
    let lanes = stalled(&served, &desktop, 12).await;

    let outcome = answered(&controlling, Said::Halt { seq: 1 }, 1).await;

    assert_eq!(outcome, Outcome::Stopped);
    assert_eq!(credit.left(), 0, "still spent");
    for opened in &lanes {
        controlling
            .outbox
            .say(Said::Cancel { lane: opened.lane })
            .unwrap();
    }
    left_at(&credit, BULK as usize).await;
    let opened = picture(&desktop, &frame()).await;
    let mut reading = Reading::of(opened, &controlling, Duration::ZERO);
    let (header, pixels) = reading.framed().await;
    assert_eq!(header["ok"], true, "a picture is no change: {header}");
    assert_eq!(pixels.len(), PIXELS);
}

#[tokio::test]
async fn a_picture_lane_of_another_generation_is_refused() {
    static CAPTURES: AtomicUsize = AtomicUsize::new(0);
    let served = Served::seeing(records(), |daemon| {
        daemon.screen.raw_capture = |display, side| {
            CAPTURES.fetch_add(1, SeqCst);
            wide(display, side)
        };
    })
    .await;
    let (_older, old) = served.control().await;
    let (_newer, _new) = served.control().await;

    let mut opened = picture(&old, &frame()).await;

    let mut said = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(5), opened.stream.read_to_end(&mut said))
        .await
        .expect("closed");
    assert!(said.is_empty(), "nothing said on it");
    assert_eq!(CAPTURES.load(SeqCst), 0);
}

#[tokio::test]
async fn a_picture_without_screen_recording_is_refused_in_the_guests_words_and_a_read_is_served() {
    static CAPTURES: AtomicUsize = AtomicUsize::new(0);
    let served = Served::seeing(records(), |daemon| {
        daemon.screen.recording = || false;
        daemon.screen.raw_capture = |display, side| {
            CAPTURES.fetch_add(1, SeqCst);
            wide(display, side)
        };
        daemon.screen.capture = |display, area, limit| {
            CAPTURES.fetch_add(1, SeqCst);
            synthetic(display, area, limit)
        };
    })
    .await;
    let (controlling, desktop) = served.control().await;
    let held = "Allow Atlas in Privacy & Security, Screen & System Audio Recording, in the VM's \
                window: agents' pictures wait for it.";

    for request in [
        frame(),
        json!({"op": "stream", "maxDimension": 1600, "session": session()}),
    ] {
        let opened = picture(&desktop, &request).await;
        let mut reading = Reading::of(opened, &controlling, Duration::ZERO);
        let refused = reading.framed_header().await;
        assert_eq!(refused["code"], "permission", "{request} {refused}");
        assert_eq!(refused["error"], held, "{request}");
        assert!(reading.ends().await, "{request}");
    }
    let opened = picture(&desktop, &json!({"op": "screenshot", "session": session()})).await;
    let mut reading = Reading::of(opened, &controlling, Duration::ZERO);
    let refused = reading.line().await;
    assert_eq!(refused["code"], "permission", "{refused}");
    assert_eq!(refused["error"], held);
    assert_eq!(CAPTURES.load(SeqCst), 0, "nothing captured");

    let missing = tempfile::tempdir_in("/private/tmp").unwrap();
    let path = missing.path().join("notes");
    let opened = picture(
        &desktop,
        &json!({"op": "read", "path": path, "maxSize": 1, "session": session()}),
    )
    .await;
    let mut reading = Reading::of(opened, &controlling, Duration::ZERO);
    let read = reading.line().await;
    assert_eq!(read["reason"], "missing", "served, as no picture: {read}");

    let hello = hello_of(&desktop).await;
    assert_eq!(hello["capture"]["held"], held, "{hello}");
    left_at(&*guest_credit(&served).await, BULK as usize).await;
}

#[tokio::test]
async fn an_idle_resumed_guest_restarted_for_screen_recording_stays_stopped_until_resumed() {
    use crate::machine::stop::{Kept, Why};
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    assert_eq!(served.machine.paired.stop.kept().unwrap(), Kept::Clear);
    assert!(!refused_as_stopped(&desktop).await, "resumed and idle");

    assert!(served.machine.restart_for_recording().await);

    let Served {
        records,
        vault,
        _serving: serving,
        ..
    } = served;
    let ended = tokio::time::timeout(Duration::from_secs(5), serving)
        .await
        .expect("serving ended for the restart")
        .unwrap();
    assert_eq!(ended.unwrap_err().code, Code::Restart);
    let changes = Arc::new(Changes::new(records.path(), vault.clone()));
    let restarted = Served::on((records, vault, changes)).await;
    let (controlling, desktop) = restarted.control().await;
    assert_eq!(
        restarted.machine.paired.stop.kept().unwrap(),
        Kept::Stopped {
            why: Why::Recording
        }
    );
    assert!(refused_as_stopped(&desktop).await, "stopped until resumed");
    assert_eq!(
        without_epoch(&hello_of(&desktop).await["stop"]),
        json!({"state": "stopped", "why": "recording"})
    );

    assert!(
        resumed(&controlling, &desktop).await,
        "the person's Resume lifts it"
    );
}

#[tokio::test]
async fn an_unpaid_stream_header_followed_by_silence_ends_its_lane_in_time_with_control_healthy() {
    let served = Served::seeing(records(), widely).await;
    served
        .machine
        .progressing_within(Duration::from_millis(300));
    let (_controlling, desktop) = served.control().await;
    let credit = guest_credit(&served).await;
    let mut opened = picture(
        &desktop,
        &json!({"op": "stream", "maxDimension": 1600, "session": session()}),
    )
    .await;
    let mut size = [0; 4];
    opened.stream.read_exact(&mut size).await.unwrap();
    let started = std::time::Instant::now();

    let mut rest = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), opened.stream.read_to_end(&mut rest))
        .await
        .expect("the lane ended before the stream's own idle time")
        .ok();

    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    left_at(&credit, BULK as usize).await;
    assert!(served.retired().is_empty(), "control stayed healthy");
}

#[test]
fn a_frame_whose_lane_is_cancelled_while_its_capture_waits_never_captures() {
    static CAPTURES: AtomicUsize = AtomicUsize::new(0);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let served = Served::seeing(records(), |daemon| {
            daemon.screen.raw_capture = |display, side| {
                CAPTURES.fetch_add(1, SeqCst);
                wide(display, side)
            };
        })
        .await;
        let (controlling, desktop) = served.control().await;
        // The one thread blocking work runs on, taken: the frame's capture
        // waits behind it.
        let (release, waiting) = std::sync::mpsc::channel::<()>();
        let blocker = tokio::task::spawn_blocking(move || {
            let _ = waiting.recv_timeout(Duration::from_secs(10));
        });
        let mut opened = picture(&desktop, &frame()).await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        controlling
            .outbox
            .say(Said::Cancel { lane: opened.lane })
            .unwrap();

        let mut said = Vec::new();
        tokio::time::timeout(Duration::from_secs(5), opened.stream.read_to_end(&mut said))
            .await
            .expect("the lane ended while its capture waited")
            .ok();
        release.send(()).unwrap();
        blocker.await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(said.is_empty(), "nothing said");
        assert_eq!(CAPTURES.load(SeqCst), 0, "the waiting capture never began");
        left_at(&*guest_credit(&served).await, BULK as usize).await;
    });
}

/// A guest that may not record, asked for it as the machine asks.
#[derive(Default)]
struct Asking {
    requested: tokio::sync::Notify,
    asked: AtomicUsize,
}

impl super::super::recording::Recording for Asking {
    fn permitted(&self) -> bool {
        false
    }

    fn seen(&self) -> impl Future<Output = Option<bool>> + Send {
        std::future::ready(Some(false))
    }

    fn request(&self) {
        self.asked.fetch_add(1, SeqCst);
        self.requested.notify_one();
    }

    fn restart(&self) -> impl Future<Output = bool> + Send {
        std::future::ready(false)
    }
}

/// A guest that may not record, ended the moment macOS asks it for Screen
/// Recording, as its process is ended: its runtime shut down with nothing
/// more run, so nothing it would save on the way out is saved. Then started
/// again over the same records, its screen as `seeing` makes it.
async fn ended_while_asked(seeing: fn(&mut Daemon)) -> (Served, Controlling, Desktop) {
    let (ended, records) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let left = runtime.block_on(asked_until_ended());
        runtime.shutdown_background();
        let _ = ended.send(left);
    });
    let (records, vault) = records.await.expect("the first daemon ran");
    let changes = Arc::new(Changes::new(records.path(), vault.clone()));
    let reopened = Served::built((records, vault, changes), None, true, seeing).await;
    let (controlling, desktop) = reopened.control().await;
    (reopened, controlling, desktop)
}

/// A guest that may not record, asked for Screen Recording by its own
/// watch, up to the moment macOS asks: its records, which outlive it.
async fn asked_until_ended() -> (tempfile::TempDir, Arc<dyn Vault>) {
    use super::super::recording::{Record, watch};
    let served = Served::seeing(records(), |daemon| daemon.screen.recording = || false).await;
    // Idle: no work of its own left for the next daemon to find.
    let (controlling, desktop) = served.control().await;
    let asking = Arc::new(Asking::default());
    let record = Record::at(served.machine.paired.recording.clone());
    tokio::spawn({
        let asking = asking.clone();
        async move { watch(&*asking, &record).await }
    });
    tokio::time::timeout(Duration::from_secs(10), asking.requested.notified())
        .await
        .expect("macOS asked");
    assert_eq!(
        served.machine.paired.stop.kept().unwrap(),
        crate::machine::stop::Kept::Clear,
        "asking stops nothing"
    );
    // macOS's Quit & Reopen ends the daemon here: everything it holds goes
    // with its runtime.
    let Served { records, vault, .. } = served;
    std::mem::forget((controlling, desktop));
    (records, vault)
}

#[tokio::test]
async fn a_guest_ended_while_macos_asks_and_started_able_to_record_stays_stopped_until_resumed() {
    use crate::machine::stop::{Kept as StopKept, Why};
    let (reopened, controlling, desktop) = ended_while_asked(|_| {}).await;

    assert!(
        refused_as_stopped(&desktop).await,
        "stopped before admitting anything"
    );
    assert_eq!(
        reopened.machine.paired.stop.kept().unwrap(),
        StopKept::Stopped {
            why: Why::Recording
        }
    );
    assert_eq!(
        without_epoch(&hello_of(&desktop).await["stop"]),
        json!({"state": "stopped", "why": "recording"})
    );
    let record = super::super::recording::Record::at(reopened.machine.paired.recording.clone());
    assert!(!record.held(), "the hold forgotten once the Stop was saved");
    assert!(
        resumed(&controlling, &desktop).await,
        "the person's Resume lifts it"
    );
}

#[tokio::test]
async fn a_guest_ended_while_macos_asks_and_started_still_denied_stops_nothing() {
    let (reopened, _controlling, desktop) =
        ended_while_asked(|daemon| daemon.screen.recording = || false).await;

    assert!(!refused_as_stopped(&desktop).await);
    assert_eq!(
        reopened.machine.paired.stop.kept().unwrap(),
        crate::machine::stop::Kept::Clear
    );
    let record = super::super::recording::Record::at(reopened.machine.paired.recording.clone());
    assert!(record.held(), "still held, for the grant to come");
}

#[tokio::test]
async fn a_fresh_guest_asked_and_never_granted_is_never_stopped() {
    use super::super::recording::{Record, watch};
    let served = Served::seeing(records(), |daemon| daemon.screen.recording = || false).await;
    let (_controlling, desktop) = served.control().await;
    let asking = Asking::default();
    let record = Record::at(served.machine.paired.recording.clone());

    let _ = tokio::time::timeout(Duration::from_secs(3), watch(&asking, &record)).await;

    assert_eq!(asking.asked.load(SeqCst), 1);
    assert_eq!(
        served.machine.paired.stop.kept().unwrap(),
        crate::machine::stop::Kept::Clear
    );
    assert!(!refused_as_stopped(&desktop).await, "agents go on");
    let hello = hello_of(&desktop).await;
    assert!(hello["capture"]["held"].is_string(), "{hello}");
}

#[tokio::test]
async fn a_guest_that_cannot_save_its_stop_for_a_grant_at_start_admits_nothing_and_keeps_the_hold()
{
    use super::super::recording::{Record, watch};
    let records = records();
    let record = Record::at(recording_at(&records.0));
    let _ = tokio::time::timeout(Duration::from_secs(1), watch(&Asking::default(), &record)).await;
    assert!(record.held());
    // The Stop's file cannot be written: a folder stands in its place.
    std::fs::create_dir(records.0.path().join("stop")).unwrap();

    let served = Served::on(records).await;
    let (controlling, desktop) = served.control().await;

    assert!(refused_as_stopped(&desktop).await, "nothing admitted");
    assert!(record.held(), "the hold kept for the next start");
    assert!(
        !resumed(&controlling, &desktop).await,
        "lifted with no Stop saved"
    );
}

/// A stream on `desktop` whose header came and whose READY went, its
/// capture held in `capture` until the test lets it go.
async fn held_stream(desktop: &Desktop, controlling: &Controlling, repay: bool) -> Opened {
    let mut opened = picture(
        desktop,
        &json!({"op": "stream", "maxDimension": 1600, "session": session()}),
    )
    .await;
    let mut size = [0; 4];
    opened.stream.read_exact(&mut size).await.unwrap();
    let mut header = vec![0; u32::from_be_bytes(size) as usize];
    opened.stream.read_exact(&mut header).await.unwrap();
    if repay {
        controlling
            .outbox
            .say(Said::Credit {
                lane: opened.lane,
                bytes: (4 + header.len()) as u64,
            })
            .unwrap();
    }
    opened.stream.write_all(&[1]).await.unwrap();
    opened.stream.flush().await.unwrap();
    opened
}

#[tokio::test]
async fn a_lane_ended_while_its_capture_is_blocked_closes_and_is_refunded_in_time() {
    static BLOCKED: AtomicBool = AtomicBool::new(true);
    static STARTED: AtomicUsize = AtomicUsize::new(0);
    static ENDED: AtomicUsize = AtomicUsize::new(0);
    let blocking = |daemon: &mut Daemon| {
        daemon.screen.raw_capture = |display, side| {
            STARTED.fetch_add(1, SeqCst);
            while BLOCKED.load(SeqCst) {
                std::thread::sleep(Duration::from_millis(1));
            }
            ENDED.fetch_add(1, SeqCst);
            wide(display, side)
        };
    };
    // Its pump past its time with the header unpaid, then the station's
    // cancel of a lane it paid for.
    for expiring in [true, false] {
        BLOCKED.store(true, SeqCst);
        STARTED.store(0, SeqCst);
        ENDED.store(0, SeqCst);
        let served = Served::seeing(records(), blocking).await;
        if expiring {
            served
                .machine
                .progressing_within(Duration::from_millis(300));
        }
        let (controlling, desktop) = served.control().await;
        let credit = guest_credit(&served).await;
        let mut opened = held_stream(&desktop, &controlling, !expiring).await;
        tokio::time::timeout(Duration::from_secs(5), async {
            while STARTED.load(SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("its capture began");
        if !expiring {
            controlling
                .outbox
                .say(Said::Cancel { lane: opened.lane })
                .unwrap();
        }

        let mut rest = Vec::new();
        let closed =
            tokio::time::timeout(Duration::from_secs(3), opened.stream.read_to_end(&mut rest))
                .await;
        let refunded = tokio::time::timeout(Duration::from_secs(3), async {
            while credit.left() != BULK as usize {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        let still_capturing = ENDED.load(SeqCst) == 0;
        BLOCKED.store(false, SeqCst);

        assert!(closed.is_ok(), "expiring {expiring}: the lane stayed open");
        assert!(refunded.is_ok(), "expiring {expiring}: its credit kept");
        assert!(
            still_capturing,
            "expiring {expiring}: closed only once captured"
        );
        assert!(served.retired().is_empty());
    }
}

#[test]
fn a_queued_streams_capture_never_begins_once_its_station_hangs_up_before_its_cancel() {
    static CAPTURES: AtomicUsize = AtomicUsize::new(0);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let served = Served::seeing(records(), |daemon| {
            daemon.screen.raw_capture = |display, side| {
                CAPTURES.fetch_add(1, SeqCst);
                wide(display, side)
            };
        })
        .await;
        let (controlling, desktop) = served.control().await;
        let opened = picture(
            &desktop,
            &json!({"op": "stream", "maxDimension": 1600, "session": session()}),
        )
        .await;
        let lane = opened.lane;
        let mut reading = Reading::of(opened, &controlling, Duration::ZERO);
        let started = reading.framed_header().await;
        assert_eq!(started["stream"], "rgb-stream-v1", "{started}");
        // The one thread blocking work runs on, taken: the frame's capture
        // waits behind it.
        let (release, waiting) = std::sync::mpsc::channel::<()>();
        let blocker = tokio::task::spawn_blocking(move || {
            let _ = waiting.recv_timeout(Duration::from_secs(10));
        });
        reading.opened.stream.write_all(&[1]).await.unwrap();
        reading.opened.stream.flush().await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;

        // The station hangs up first, then cancels.
        drop(reading);
        tokio::time::sleep(Duration::from_millis(200)).await;
        controlling.outbox.say(Said::Cancel { lane }).unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        release.send(()).unwrap();
        blocker.await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;

        assert_eq!(CAPTURES.load(SeqCst), 0, "the queued capture never began");
        left_at(&*guest_credit(&served).await, BULK as usize).await;
    });
}

/// A hello's Stop without the run and count it was read at.
fn without_epoch(stop: &Value) -> Value {
    let mut stop = stop.clone();
    if let Some(stop) = stop.as_object_mut() {
        assert!(stop.remove("epoch").is_some(), "its epoch is said");
    }
    stop
}
