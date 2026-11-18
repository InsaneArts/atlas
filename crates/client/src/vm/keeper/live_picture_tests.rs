//! A VM's live picture end to end: a real client's live stream through a
//! real keeper, its guest a stand-in that frames a live picture as a guest's
//! daemon does, paced by its bulk credit, and Atlas VM's picture fence as
//! the test moves it.
use crate::{
    client::{
        Client,
        live::{LiveFrame, LiveStream},
    },
    vm::keeper::{
        change::tests::{admitted, live, proven, restarted},
        clients::tests::Serving,
        picture::tests::{begun, picture, rest, session},
        tests::{Guest, Saying, atlas_vm_state, screen, until},
    },
};
use atlas_core::{error::Code, frame_pipe::diagnostics::Ring, protocol::CaptureBackend};
use atlas_session::{
    credit::{BULK, CHUNK},
    lane::{Header, LaneId, LaneKind},
    standin::pictures::{Part, Told},
};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst},
    },
    time::Duration,
};

/// The guest's daemon's process, which its proofs name.
const DAEMON: u64 = 123;
/// The guest's capture source's generation serving the live picture.
const SOURCE: u64 = 5;
/// What the guest's daemon says Screen Recording waits for, in its words.
const CAPTURE_HELD: &str = "Allow Atlas in Privacy & Security, Screen & System Audio Recording, \
                            in the VM's window: agents' pictures wait for it.";

/// `header` framed as the guest's daemon frames one: its length, then it.
fn said(header: &Value) -> Vec<u8> {
    let header = serde_json::to_vec(header).unwrap();
    let mut bytes = u32::try_from(header.len()).unwrap().to_be_bytes().to_vec();
    bytes.extend(header);
    bytes
}

/// The guest's daemon's word that it took a live picture.
fn accepted() -> Vec<u8> {
    said(
        &json!({"ok": true, "stream": "rgb-stream-v1", "daemonPID": DAEMON,
        "producerGeneration": SOURCE}),
    )
}

/// Live frame `sequence`, `width` × `height` pixels of `fill`, its header's
/// proof `proof`, as the guest's daemon frames one.
fn frame(sequence: u64, (width, height): (usize, usize), fill: u8, proof: Value) -> Vec<u8> {
    let mut bytes = said(&json!({"ok": true, "sequence": sequence, "width": width,
        "height": height, "scale": 2.0, "session": proof, "daemonPID": DAEMON,
        "captureBackend": "cg", "captureMs": 1.5}));
    bytes.extend(vec![fill; width * height * 3]);
    bytes
}

/// The guest's daemon's answer to the stop `id`: its source stopped and
/// drained.
fn stopped(id: u64) -> Vec<u8> {
    said(
        &json!({"ok": true, "daemonPID": DAEMON, "producerGeneration": SOURCE,
        "stopRequestID": id, "drained": true}),
    )
}

/// What a test's guest does as its live picture is told: the frames it
/// sends, and whatever it changes as it is told.
type Answer = Arc<dyn Fn(Told) -> Option<(Vec<Part>, bool)> + Send + Sync>;

/// A guest that answers each readiness with a 64 × 40 frame of its number
/// and its stop as drained.
fn plain(told: Told) -> Option<(Vec<Part>, bool)> {
    Some(match told {
        Told::Asked => (sent(accepted()), false),
        Told::Ready(n) => (sent(frame(n, (64, 40), fill(n), screen())), false),
        Told::Stop(id) => (sent(stopped(id)), true),
    })
}

/// `bytes`, sent whole.
fn sent(bytes: Vec<u8>) -> Vec<Part> {
    vec![Part::Bytes(bytes)]
}

/// The pixels of frame `n`.
fn fill(n: u64) -> u8 {
    u8::try_from(n % 250).unwrap()
}

/// A VM a test watches live: its keeper, its guest's saying, an agent's
/// client of it, and every live picture its guest was asked for.
struct Watching {
    serving: Serving,
    saying: Arc<Saying>,
    client: Client,
    _directory: tempfile::TempDir,
    asked: Arc<Mutex<Vec<Value>>>,
}

async fn watching(answer: Answer) -> Watching {
    let (serving, saying, client, directory) = proven().await;
    let asked = Arc::new(Mutex::new(Vec::new()));
    let kept = asked.clone();
    serving
        .fixture
        .guest
        .standin
        .pictures
        .stream(Arc::new(move |request, told| {
            if told == Told::Asked {
                kept.lock().unwrap().push(request.clone());
            }
            answer(told)
        }));
    Watching {
        serving,
        saying,
        client,
        _directory: directory,
        asked,
    }
}

impl Watching {
    fn guest(&self) -> Arc<Guest> {
        self.serving.fixture.guest.clone()
    }

    /// How many times the VM was proved.
    fn proofs(&self) -> usize {
        self.saying.challenges.lock().unwrap().len()
    }

    /// The picture lanes the guest admitted.
    fn picture_lanes(&self) -> Vec<Header> {
        admitted(&self.serving)
            .into_iter()
            .filter(|header| header.kind == LaneKind::Picture)
            .collect()
    }

    /// The live picture's one lane.
    fn lane(&self) -> LaneId {
        let lanes = self.picture_lanes();
        assert_eq!(lanes.len(), 1, "{lanes:?}");
        lanes[0].lane
    }

    /// Wait until the guest's pump of `lane` ended, then say whether it
    /// sent everything it had.
    async fn pumped(&self, lane: LaneId) -> bool {
        let standin = &self.serving.fixture.guest.standin;
        until("the lane's pump ended", || {
            standin.pumped().iter().any(|pumped| pumped.lane == lane)
        })
        .await;
        standin
            .pumped()
            .into_iter()
            .find(|pumped| pumped.lane == lane)
            .unwrap()
            .whole
    }

    /// Wait until the guest owes no credit on any lane.
    async fn refunded(&self) {
        let credit = self.serving.fixture.guest.standin.bulk_credit().unwrap();
        until("every credit back", || credit.left() == BULK as usize).await;
    }

    async fn open(&self) -> LiveStream {
        LiveStream::connect(&self.client, 640, CaptureBackend::ScreenCaptureKit)
            .await
            .unwrap()
            .expect("a VM's live picture")
    }
}

/// Make Atlas VM say its window is `window` and its fence `fence`.
fn fence(guest: &Guest, window: &str, fence: u64) {
    *guest.runner.lock().unwrap() = atlas_vm_state(window, fence);
}

async fn next(stream: &mut LiveStream, client: &Client) -> atlas_core::error::Result<LiveFrame> {
    stream.next(client, &mut Ring::default()).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_live_picture_comes_frame_by_frame_on_one_pinned_picture_lane_of_its_generation() {
    let watching = watching(Arc::new(plain)).await;
    let generation = live(&watching.serving);
    let proved = watching.proofs();

    let mut stream = watching.open().await;
    let mut frames = Vec::new();
    for _ in 0..3 {
        frames.push(next(&mut stream, &watching.client).await.unwrap());
    }

    for (n, frame) in (1..).zip(&frames) {
        assert_eq!((frame.width, frame.height), (64, 40));
        assert!(frame.rgb.iter().all(|pixel| *pixel == fill(n)), "frame {n}");
        assert_eq!(frame.epoch.capture_pid, 123);
    }
    let lanes = watching.picture_lanes();
    assert_eq!(lanes.len(), 1, "{lanes:?}");
    assert_eq!(lanes[0].generation.0, generation);
    let asked = watching.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0]["op"], "stream");
    assert_eq!(asked[0]["maxDimension"], 640);
    assert_eq!(
        asked[0]["session"],
        json!({"securitySessionId": 42, "displayId": 7})
    );
    assert!(asked[0]["backend"].is_null(), "CoreGraphics on a VM");
    assert_eq!(
        watching.proofs(),
        proved + 1,
        "proved to open, then not again"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_frame_whose_header_proves_another_session_is_refused_as_the_session_changed() {
    let watching = watching(Arc::new(|told| match told {
        Told::Ready(n) => {
            let mut proof = screen();
            proof["securitySessionID"] = json!(43);
            Some((sent(frame(n, (64, 40), 1, proof)), false))
        }
        told => plain(told),
    }))
    .await;

    let mut stream = watching.open().await;
    let refused = next(&mut stream, &watching.client).await.err().unwrap();

    assert_eq!(refused.code, Code::SessionChanged, "{refused:?}");
    assert_eq!(
        refused.error,
        "work changed, or its window was opened, since Atlas checked it; look again"
    );
    assert!(!stream.can_retry(&refused));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_live_picture_whose_generation_ended_fails_as_its_transport() {
    let watching = watching(Arc::new(plain)).await;
    let mut stream = watching.open().await;
    next(&mut stream, &watching.client).await.unwrap();
    let lane = watching.lane();

    restarted(&watching.serving).await;
    assert!(!watching.pumped(lane).await, "cut short");
    // Read before its next full proof is due, which would find the new
    // generation.
    stream.proved_ago(Duration::ZERO);
    let ended = next(&mut stream, &watching.client).await.err().unwrap();

    assert_eq!(ended.code, Code::Transport, "{ended:?}");
    assert_eq!(
        ended.error,
        "work's picture stopped as its session ended; open it again"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_live_picture_proved_again_after_its_generation_ended_is_refused_as_changed() {
    let watching = watching(Arc::new(plain)).await;
    let mut stream = watching.open().await;
    next(&mut stream, &watching.client).await.unwrap();

    restarted(&watching.serving).await;
    stream.proved_ago(Duration::from_secs(1));
    let refused = next(&mut stream, &watching.client).await.err().unwrap();

    assert_eq!(refused.code, Code::SessionChanged, "{refused:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_window_opened_between_two_frames_withholds_the_second_before_any_new_proof() {
    let watching = watching(Arc::new(plain)).await;
    let guest = watching.guest();
    let mut stream = watching.open().await;
    next(&mut stream, &watching.client).await.unwrap();
    let proved = watching.proofs();
    let read = guest.runner_asked.load(SeqCst);

    // Opened with input and closed again: only the fence shows it.
    fence(&guest, "closed", 1);
    let withheld = next(&mut stream, &watching.client).await.err().unwrap();

    assert_eq!(withheld.code, Code::OnConsole, "{withheld:?}");
    assert_eq!(
        withheld.error,
        "work's window took the person's input, so its picture stopped"
    );
    assert_eq!(
        watching.proofs(),
        proved,
        "the keeper's fence held it, not a proof"
    );
    assert!(
        guest.runner_asked.load(SeqCst) > read,
        "the fence was read again"
    );
    assert!(
        !watching.pumped(watching.lane()).await,
        "the lane was cancelled"
    );
    watching.refunded().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_frame_taken_while_the_window_opened_and_closed_again_before_it_went_is_withheld() {
    let moved = Arc::new(Mutex::new(None::<Arc<Guest>>));
    let moving = moved.clone();
    let watching = watching(Arc::new(move |told| {
        if let Told::Ready(_) = told
            && let Some(guest) = moving.lock().unwrap().as_ref()
        {
            // Opened, then closed, while this frame was captured.
            fence(guest, "open", 1);
            fence(guest, "closed", 1);
        }
        plain(told)
    }))
    .await;
    *moved.lock().unwrap() = Some(watching.guest());

    let mut stream = watching.open().await;
    let withheld = next(&mut stream, &watching.client).await.err().unwrap();

    assert_eq!(withheld.code, Code::OnConsole, "{withheld:?}");
    assert!(!watching.pumped(watching.lane()).await);
    watching.refunded().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_window_opened_after_the_picture_was_admitted_withholds_its_first_frame() {
    let moved = Arc::new(Mutex::new(None::<Arc<Guest>>));
    let moving = moved.clone();
    let watching = watching(Arc::new(move |told| {
        if told == Told::Asked
            && let Some(guest) = moving.lock().unwrap().as_ref()
        {
            fence(guest, "open", 1);
        }
        plain(told)
    }))
    .await;
    *moved.lock().unwrap() = Some(watching.guest());

    let mut stream = watching.open().await;
    let withheld = next(&mut stream, &watching.client).await.err().unwrap();

    assert_eq!(withheld.code, Code::OnConsole, "{withheld:?}");
    assert_eq!(
        withheld.error,
        "work's window took the person's input, so its picture stopped"
    );
    assert!(!watching.pumped(watching.lane()).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_live_picture_proves_its_vm_again_once_its_last_proof_is_a_second_old() {
    let watching = watching(Arc::new(plain)).await;
    let guest = watching.guest();
    let mut stream = watching.open().await;
    next(&mut stream, &watching.client).await.unwrap();
    let proved = watching.proofs();

    stream.proved_ago(Duration::from_secs(1));
    next(&mut stream, &watching.client).await.unwrap();
    next(&mut stream, &watching.client).await.unwrap();
    assert_eq!(watching.proofs(), proved + 1, "once, then not again");

    // A proof that finds the window opened since stops the picture.
    fence(&guest, "closed", 1);
    stream.proved_ago(Duration::from_secs(1));
    let refused = next(&mut stream, &watching.client).await.err().unwrap();
    assert_eq!(refused.code, Code::SessionChanged, "{refused:?}");
    assert_eq!(watching.proofs(), proved + 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_live_pictures_stop_goes_on_its_lane_and_is_confirmed_drained() {
    let watching = watching(Arc::new(plain)).await;
    let mut stream = watching.open().await;
    next(&mut stream, &watching.client).await.unwrap();
    let lane = watching.lane();

    stream.stop(&watching.client).await.unwrap();

    let heard = watching.serving.fixture.guest.standin.heard_on(lane);
    assert_eq!(heard.len(), 1 + 1 + 8, "{heard:?}");
    assert_eq!(heard[..2], [1, 0]);
    assert!(watching.pumped(lane).await, "the guest ended it whole");
    watching.refunded().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_live_picture_left_mid_frame_asks_on_a_read_lane_of_its_generation_whether_it_drained() {
    let watching = watching(Arc::new(|told| match told {
        Told::Ready(n) => {
            let mut proof = screen();
            proof["displayID"] = json!(8);
            Some((sent(frame(n, (64, 40), 1, proof)), false))
        }
        told => plain(told),
    }))
    .await;
    let queried = Arc::new(Mutex::new(Vec::new()));
    let kept = queried.clone();
    *watching.saying.otherwise.lock().unwrap() = Some(Arc::new(move |header, request| {
        if header.kind == LaneKind::Read && request["op"] == "stream-drain-status" {
            kept.lock()
                .unwrap()
                .push((header.generation.0, request.clone()));
            return json!({"ok": true, "daemonPID": request["daemonPID"],
                "producerGeneration": request["producerGeneration"],
                "queryID": request["queryID"], "state": "stopped", "drained": true});
        }
        json!({"ok": true, "echo": request})
    }));
    let generation = live(&watching.serving);
    let mut stream = watching.open().await;
    let refused = next(&mut stream, &watching.client).await.err().unwrap();
    assert!(!stream.can_retry(&refused));

    stream.stop(&watching.client).await.unwrap();

    let queried = queried.lock().unwrap().clone();
    assert_eq!(queried.len(), 1, "{queried:?}");
    let (asked_in, query) = &queried[0];
    assert_eq!(*asked_in, generation);
    assert_eq!(query["daemonPID"], DAEMON);
    assert_eq!(query["producerGeneration"], SOURCE);
    assert_eq!(
        query["session"],
        json!({"securitySessionId": 42, "displayId": 7})
    );
    let heard = watching
        .serving
        .fixture
        .guest
        .standin
        .heard_on(watching.lane());
    assert_eq!(heard, [1], "no stop said mid-frame");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sixty_frames_of_three_chunks_each_come_whole_with_at_most_two_chunks_held_at_once() {
    // 320 × 200 pixels: 192 000 bytes a frame, three chunks.
    let watching = watching(Arc::new(|told| match told {
        Told::Ready(n) => Some((sent(frame(n, (320, 200), fill(n), screen())), false)),
        told => plain(told),
    }))
    .await;
    let credit = watching
        .serving
        .fixture
        .guest
        .standin
        .bulk_credit()
        .unwrap();
    let watched = Arc::new(AtomicBool::new(true));
    let most = Arc::new(AtomicUsize::new(0));
    let sampling = tokio::spawn({
        let (credit, watched, most) = (credit.clone(), watched.clone(), most.clone());
        async move {
            while watched.load(SeqCst) {
                most.fetch_max(BULK as usize - credit.left(), SeqCst);
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }
    });

    let mut stream = watching.open().await;
    let mut taken = 0_u64;
    while taken < 60 {
        match next(&mut stream, &watching.client).await {
            Ok(frame) => {
                taken += 1;
                assert_eq!((frame.width, frame.height), (320, 200));
                assert!(frame.rgb.iter().all(|pixel| *pixel == frame.rgb[0]));
            }
            // A frame late under load is retried on the same lane.
            Err(error) if stream.can_retry(&error) => {}
            Err(error) => panic!("frame {taken}: {error:?}"),
        }
    }
    watched.store(false, SeqCst);
    sampling.await.unwrap();
    stream.stop(&watching.client).await.unwrap();

    let held = most.load(SeqCst);
    assert!(held > 0, "the guest sent through its credit");
    assert!(held <= 2 * CHUNK, "{held} bytes held at once");
    assert_eq!(
        watching.picture_lanes().len(),
        1,
        "one lane for every frame"
    );
    watching.refunded().await;
}

/// A live picture, as a command asks for one.
fn stream_request() -> Value {
    json!({"op": "stream", "maxDimension": 640, "session": session()})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_live_picture_is_refused_unsaid_while_the_window_is_open_or_its_fence_cannot_be_told() {
    let watching = watching(Arc::new(plain)).await;
    let guest = watching.guest();
    let generation = live(&watching.serving);
    let mut refusals = Vec::new();

    fence(&guest, "open", 1);
    refusals.push(
        begun(&watching.serving, &picture(stream_request(), generation))
            .await
            .1,
    );
    let mut unfenced = atlas_vm_state("closed", 0);
    unfenced.as_object_mut().unwrap().remove("fence");
    *guest.runner.lock().unwrap() = unfenced;
    refusals.push(
        begun(&watching.serving, &picture(stream_request(), generation))
            .await
            .1,
    );
    *guest.runner.lock().unwrap() = json!({"ok": false, "error": "Atlas VM is busy."});
    refusals.push(
        begun(&watching.serving, &picture(stream_request(), generation))
            .await
            .1,
    );

    let codes: Vec<_> = refusals
        .iter()
        .map(|refused| refused["code"].clone())
        .collect();
    assert_eq!(
        codes,
        [json!("on-console"), json!("update"), json!("transport")]
    );
    for refused in &refusals {
        assert_eq!(refused["sent"], false, "{refused}");
        assert!(
            !refused["error"].as_str().unwrap().contains("helper"),
            "{refused}"
        );
    }
    assert!(
        watching.picture_lanes().is_empty(),
        "nothing was asked of the guest"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_withheld_frame_is_replaced_by_the_keepers_word_and_none_of_its_pixels_go() {
    let watching = watching(Arc::new(plain)).await;
    let guest = watching.guest();
    let (mut stream, first) = begun(
        &watching.serving,
        &picture(stream_request(), live(&watching.serving)),
    )
    .await;
    assert_eq!(first, json!({"ok": true, "op": "picture"}));
    let mut word = vec![0_u8; accepted().len()];
    tokio::io::AsyncReadExt::read_exact(&mut stream, &mut word)
        .await
        .unwrap();
    assert_eq!(word, accepted(), "the guest's first word passes as it is");

    fence(&guest, "closed", 1);
    tokio::io::AsyncWriteExt::write_all(&mut stream, &[1])
        .await
        .unwrap();
    let left = rest(&mut stream, Duration::from_secs(10)).await;

    let length = usize::try_from(u32::from_be_bytes(left[..4].try_into().unwrap())).unwrap();
    assert_eq!(
        left.len(),
        4 + length,
        "the keeper's word, and nothing after it"
    );
    let refused: Value = serde_json::from_slice(&left[4..]).unwrap();
    assert_eq!(refused["ok"], false);
    assert_eq!(refused["code"], "on-console");
    assert_eq!(
        refused["error"],
        "The VM's window took the person's input, so its picture stopped."
    );
    assert!(
        !watching.pumped(watching.lane()).await,
        "its lane was cancelled"
    );
    watching.refunded().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_live_pictures_reader_that_takes_nothing_for_its_time_is_closed_and_refunded() {
    // 800 × 500 pixels: more than the guest's whole credit in one frame.
    let watching = watching(Arc::new(|told| match told {
        Told::Ready(n) => Some((sent(frame(n, (800, 500), 7, screen())), false)),
        told => plain(told),
    }))
    .await;
    *watching.serving.fixture.keeper.progress.lock().unwrap() = Duration::from_millis(300);
    let (mut stalled, _) = begun(
        &watching.serving,
        &picture(stream_request(), live(&watching.serving)),
    )
    .await;

    tokio::io::AsyncWriteExt::write_all(&mut stalled, &[1])
        .await
        .unwrap();
    let lane = {
        until("its lane opened", || watching.picture_lanes().len() == 1).await;
        watching.lane()
    };
    let whole = watching.pumped(lane).await;
    let left = rest(&mut stalled, Duration::from_secs(10)).await;

    assert!(!whole, "closed part way");
    assert!(left.len() < 800 * 500 * 3, "{} bytes came", left.len());
    watching.refunded().await;
    assert_eq!(
        watching.serving.fixture.guest.controls().len(),
        1,
        "control kept"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_live_picture_waits_for_screen_recording_in_its_guests_words_and_asks_nothing() {
    let watching = watching(Arc::new(plain)).await;
    *watching.saying.hello.lock().unwrap() = Some(|hello| {
        hello["permissions"]["screenCapture"] = json!(false);
        hello["capture"] = json!({"held": CAPTURE_HELD});
    });

    let refused = LiveStream::connect(&watching.client, 640, CaptureBackend::CoreGraphics)
        .await
        .err()
        .unwrap();
    let looked = watching.client.looking_identity().await.err().unwrap();

    let next = "Ask the person to allow Atlas in Privacy & Security, Screen & System Audio \
                Recording, in work's window.";
    for refused in [refused, looked] {
        assert_eq!(refused.code, Code::Permission, "{refused:?}");
        assert_eq!(refused.error, CAPTURE_HELD);
        assert_eq!(refused.details["next"], next);
        let said = serde_json::to_string(&refused).unwrap();
        assert!(
            !said.contains("helper") && !said.contains("macOS only"),
            "{said}"
        );
    }
    assert!(
        watching.picture_lanes().is_empty(),
        "no picture lane opened"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_live_picture_is_at_most_as_large_as_a_vm_gives() {
    let watching = watching(Arc::new(plain)).await;

    LiveStream::connect(&watching.client, 4096, CaptureBackend::ScreenCaptureKit)
        .await
        .unwrap()
        .unwrap();

    let asked = watching.asked.lock().unwrap().clone();
    assert_eq!(asked[0]["maxDimension"], 1600, "{asked:?}");
}

/// What happens to Atlas VM part way through a frame, as a test makes it.
type MidFrame = fn(&Guest);

/// The header of frame 1 saying it is `claimed` pixels, and the first chunk
/// of its pixels, as its guest first sends them.
fn first_part(claimed: (usize, usize)) -> Vec<u8> {
    let mut bytes = said(&json!({"ok": true, "sequence": 1, "width": claimed.0,
        "height": claimed.1, "scale": 2.0, "session": screen(), "daemonPID": DAEMON,
        "captureBackend": "cg", "captureMs": 1.5}));
    bytes.extend(vec![1_u8; CHUNK]);
    bytes
}

/// A VM whose frame 1 says it is `claimed` pixels, of which its guest sends
/// the first chunk, then, once the keeper passed that on and `mid_frame`
/// changed Atlas VM, `after` more bytes, all 2; and what a client of its
/// keeper took after the picture's first word, until the keeper closed it.
async fn split_frame(
    claimed: (usize, usize),
    after: usize,
    mid_frame: MidFrame,
) -> (Watching, Vec<u8>) {
    let moved = Arc::new(Mutex::new(None::<Arc<Guest>>));
    let moving = moved.clone();
    let watching = watching(Arc::new(move |told| match told {
        Told::Ready(1) => {
            let guest = moving.lock().unwrap().clone();
            let between: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                if let Some(guest) = &guest {
                    mid_frame(guest);
                }
            });
            Some((
                vec![
                    Part::Bytes(first_part(claimed)),
                    Part::Then(between),
                    Part::Bytes(vec![2; after]),
                ],
                false,
            ))
        }
        told => plain(told),
    }))
    .await;
    *moved.lock().unwrap() = Some(watching.guest());
    let (mut stream, first) = begun(
        &watching.serving,
        &picture(stream_request(), live(&watching.serving)),
    )
    .await;
    assert_eq!(first, json!({"ok": true, "op": "picture"}));
    let mut word = vec![0_u8; accepted().len()];
    tokio::io::AsyncReadExt::read_exact(&mut stream, &mut word)
        .await
        .unwrap();
    tokio::io::AsyncWriteExt::write_all(&mut stream, &[1])
        .await
        .unwrap();
    let came = rest(&mut stream, Duration::from_secs(10)).await;
    (watching, came)
}

/// Assert that what `came` is exactly the frame's first part, its header
/// whole and its first chunk of pixels, and nothing sent after the change.
async fn cut_after_first_chunk(watching: &Watching, came: &[u8], claimed: (usize, usize)) {
    assert_eq!(
        came.len(),
        first_part(claimed).len(),
        "nothing after the change"
    );
    assert!(came == first_part(claimed).as_slice());
    assert!(!came.contains(&2));
    assert!(
        !watching.pumped(watching.lane()).await,
        "its lane was cancelled"
    );
    watching.refunded().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_window_opened_between_two_chunks_of_one_frame_ends_it_with_nothing_more_passed_on() {
    // 256 × 160 pixels: one whole chunk, then the rest.
    let (watching, came) = split_frame((256, 160), 256 * 160 * 3 - CHUNK, |guest| {
        fence(guest, "open", 1);
    })
    .await;

    cut_after_first_chunk(&watching, &came, (256, 160)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_frame_whose_length_overstates_it_carries_none_of_what_was_captured_later() {
    // Said to be twice its 256 × 160 pixels: the rest of it, then a later
    // capture, taken once the window was opened and closed again.
    let (watching, came) = split_frame((256, 320), 256 * 320 * 3 - CHUNK, |guest| {
        fence(guest, "closed", 1);
    })
    .await;

    cut_after_first_chunk(&watching, &came, (256, 320)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_frame_whose_fence_cannot_be_read_part_way_ends_with_nothing_more_passed_on() {
    let (watching, came) = split_frame((256, 160), 256 * 160 * 3 - CHUNK, |guest| {
        *guest.runner.lock().unwrap() = json!({"ok": false, "error": "Atlas VM is busy."});
    })
    .await;

    cut_after_first_chunk(&watching, &came, (256, 160)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_frame_during_which_atlas_vm_restarted_ends_with_nothing_more_passed_on() {
    let (watching, came) = split_frame((256, 160), 256 * 160 * 3 - CHUNK, |guest| {
        let mut restarted = atlas_vm_state("closed", 0);
        restarted["incarnation"]["id"] = json!("9a2e44b1-run");
        restarted["incarnation"]["pid"] = json!(4343);
        *guest.runner.lock().unwrap() = restarted;
    })
    .await;

    cut_after_first_chunk(&watching, &came, (256, 160)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_live_picture_cut_part_way_through_a_frame_stops_in_the_vms_words() {
    let moved = Arc::new(Mutex::new(None::<Arc<Guest>>));
    let moving = moved.clone();
    let watching = watching(Arc::new(move |told| match told {
        Told::Ready(1) => {
            let guest = moving.lock().unwrap().clone();
            Some((
                vec![
                    Part::Bytes(first_part((256, 160))),
                    Part::Then(Arc::new(move || {
                        if let Some(guest) = &guest {
                            fence(guest, "open", 1);
                        }
                    })),
                    Part::Bytes(vec![2; 256 * 160 * 3 - CHUNK]),
                ],
                false,
            ))
        }
        told => plain(told),
    }))
    .await;
    *moved.lock().unwrap() = Some(watching.guest());

    let mut stream = watching.open().await;
    let stopped = next(&mut stream, &watching.client).await.err().unwrap();

    assert_eq!(stopped.code, Code::OnConsole, "{stopped:?}");
    assert_eq!(
        stopped.error,
        "work's window took the person's input, so its picture stopped"
    );
    assert!(!stream.can_retry(&stopped));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_live_picture_is_cancelled_at_a_hold_for_maintenance_and_its_credit_comes_back() {
    let watching = watching(Arc::new(plain)).await;
    let serving = &watching.serving;
    let (mut stream, first) = begun(serving, &picture(stream_request(), live(serving))).await;
    until("its lane opened", || watching.picture_lanes().len() == 1).await;
    let lane = watching.lane();
    let keeper = &serving.fixture.keeper;
    let change = crate::test_support::under_maintenance(
        keeper.lifecycle.home(),
        &keeper.vm,
        crate::vm::lifecycle::record::Phase::Holding,
    );

    let held = crate::vm::keeper::clients::tests::asked(
        &serving.socket,
        &json!({"op": "hold", "change": change}),
    )
    .await;

    assert_eq!(first["ok"], true, "{first}");
    assert_eq!(held[0]["cancelled"], json!({"picture": 1}), "{held:?}");
    assert!(!watching.pumped(lane).await, "its lane was cancelled");
    rest(&mut stream, Duration::from_secs(10)).await;
    watching.refunded().await;
}
