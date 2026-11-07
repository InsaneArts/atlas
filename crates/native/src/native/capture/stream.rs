//! The helper's live picture from ScreenCaptureKit: one stream, kept open
//! while its owner wants pictures. Objective-C state stays on one native
//! worker run loop. Callbacks send only checked, owned RGB bytes to callers.
//!
//! macOS's screen recorder keeps part of every picture a one-shot capture
//! asks it for, but not of the samples one open stream delivers. A stream
//! sends a sample within a refresh of any change, unchanged samples for a
//! few seconds after, and then nothing until the screen changes again. So
//! while it says the screen is unchanged, or says nothing, its newest
//! picture is served again rather than captured anew.
#![allow(
    unsafe_code,
    reason = "FFI: ScreenCaptureKit callbacks, CoreMedia clocks and locked pixel buffers"
)]
use super::{RawShot, fit, permitted, raw::pack};
use crate::native;
use atlas_core::{
    buffer::Buffer,
    error::{Code, Error, Result},
    session::InputEpoch,
};
use block2::RcBlock;
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::{
    AnyThread, DefinedClass, define_class, msg_send,
    rc::Retained,
    runtime::{NSObject, NSObjectProtocol, ProtocolObject},
    sel,
};
use objc2_core_foundation::{
    CFArray, CFDictionary, CFNumber, CFRetained, CFRunLoop, CFString, CFType, kCFRunLoopDefaultMode,
};
use objc2_core_graphics::{CGDisplayCopyDisplayMode, CGDisplayMode};
use objc2_core_media::{CMClock, CMSampleBuffer, CMTime};
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow,
    CVPixelBufferGetDataSize, CVPixelBufferGetHeight, CVPixelBufferGetPixelFormatType,
    CVPixelBufferGetWidth, CVPixelBufferIsPlanar, CVPixelBufferLockBaseAddress,
    CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress, kCVPixelFormatType_32BGRA,
};
use objc2_foundation::{NSArray, NSError};
use objc2_screen_capture_kit::{
    SCCaptureDynamicRange, SCContentFilter, SCDisplay, SCFrameStatus, SCShareableContent, SCStream,
    SCStreamConfiguration, SCStreamDelegate, SCStreamFrameInfoDisplayTime, SCStreamFrameInfoStatus,
    SCStreamOutput, SCStreamOutputType, SCWindow,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};

/// How long a request waits for its answer. A new stream sends its first
/// picture in about 120 ms, and the caller refuses a frame half a second old
/// anyway.
const WAIT: Duration = Duration::from_millis(250);
/// How long the first request after macOS's screen recorder was restarted
/// waits: a stream on a new recorder sends its first picture in 200–350 ms.
/// A picture still has to reach the caller within its half second, even a
/// CoreGraphics one taken after this wait.
const RESTARTED_WAIT: Duration = Duration::from_millis(400);
/// How long a request waits before a silent stream's newest picture answers
/// it. A stream sends a sample within a refresh of any change, unchanged
/// samples for a few seconds after, and then nothing until the next change.
const SILENCE: Duration = Duration::from_millis(100);
/// How often a request still waiting after [`SILENCE`] asks again.
const SILENCE_CHECK: Duration = Duration::from_millis(20);
/// How long the session check before a CoreGraphics capture may take before
/// it says WindowServer stalled the whole session; it takes well under a
/// millisecond otherwise.
const STALLED: Duration = Duration::from_millis(20);
/// How long a stream with no picture to serve again (none yet, or its last
/// sample proved nothing) may go without a valid sample before it is
/// replaced.
const RENEW: Duration = Duration::from_secs(10);
/// How long a stream that still has a picture to serve again may go without
/// a valid sample before it is replaced. macOS sends nothing while the
/// screen stays still, often for longer than [`RENEW`], so this alone bounds
/// how long a stream that stalled without saying so shows an old picture.
const SILENT_RENEW: Duration = Duration::from_secs(60);
/// How long a stop waits for ScreenCaptureKit to confirm it, which takes
/// 170–200 ms here. A stop not confirmed in time leaves the source stopping,
/// and the owner's frames are refused until its own stop settles it.
const STOP_WAIT: Duration = Duration::from_millis(500);
/// The most samples a second the stream makes. Each one is a composite the
/// window server makes for it, and no viewer reads more than this.
const PICTURES_A_SECOND: i32 = 30;
/// How long a one-shot picture waits for the open stream to prove its
/// picture current: a few refreshes, as a screen that changes, or changed in
/// the last few seconds, sends a sample at each. A still screen sends none,
/// and its picture is taken another way.
const PICTURE_WAIT: Duration = Duration::from_millis(50);
/// How often a one-shot picture asked of an open stream looks again.
const PICTURE_CHECK: Duration = Duration::from_millis(5);
/// How far ahead of its callback a sample may be stamped. Its display time
/// is the refresh that shows it, a few milliseconds away at most; a clock in
/// other units would be off by far more.
const LEAD: Duration = Duration::from_millis(20);

#[derive(Default)]
struct Diagnostics {
    requests: AtomicU64,
    native_frames: AtomicU64,
    reused_frames: AtomicU64,
    renewals: AtomicU64,
    fallbacks: AtomicU64,
    idle_hints: AtomicU64,
    timeouts: AtomicU64,
    /// Requests whose wait ran into a WindowServer stall.
    stalls: AtomicU64,
    callbacks: AtomicU64,
    no_request: AtomicU64,
    missing_metadata: AtomicU64,
    old_samples: AtomicU64,
    future_samples: AtomicU64,
    layout_errors: AtomicU64,
    setup_starts: AtomicU64,
    setup_successes: AtomicU64,
    setup_errors: AtomicU64,
    guard_errors: AtomicU64,
    stop_errors: AtomicU64,
    delegate_stop_errors: AtomicU64,
    wait_us: AtomicU64,
    guard_before_us: AtomicU64,
    guard_after_us: AtomicU64,
    copy_us: AtomicU64,
    last_fallback: Mutex<Option<&'static str>>,
    last_error: Mutex<Option<(&'static str, Code)>>,
}
impl Diagnostics {
    fn error(&self, stage: &'static str, error: &Error) {
        let reason = match error.details.get("nativeReason").and_then(Value::as_str) {
            Some("no-exact-display") => "no-exact-display",
            Some("shareable-content") => "shareable-content",
            Some("start-error") => "start-error",
            Some("format-layout") => "format-layout",
            Some("metadata") => "metadata",
            Some("guard") => "guard",
            Some("stop-error") => "stop-error",
            _ => stage,
        };
        *self.last_error.lock().unwrap_or_else(|p| p.into_inner()) = Some((reason, error.code));
    }
    /// A request answered by a CoreGraphics capture instead, and why.
    fn fell_back(&self, reason: &'static str) {
        *self.last_fallback.lock().unwrap_or_else(|p| p.into_inner()) = Some(reason);
        self.fallbacks.fetch_add(1, Ordering::Relaxed);
    }
    fn snapshot(&self) -> Value {
        let value = |counter: &AtomicU64| counter.load(Ordering::Relaxed);
        let last_error = *self.last_error.lock().unwrap_or_else(|p| p.into_inner());
        json!({"requests":value(&self.requests),"nativeFrames":value(&self.native_frames),
            "reusedFrames":value(&self.reused_frames),"renewals":value(&self.renewals),
            "fallbacks":value(&self.fallbacks),"idleHints":value(&self.idle_hints),
            "timeouts":value(&self.timeouts),"stalls":value(&self.stalls),
            "callbacks":value(&self.callbacks),
            "noRequest":value(&self.no_request),"missingMetadata":value(&self.missing_metadata),
            "oldSamples":value(&self.old_samples),"futureSamples":value(&self.future_samples),
            "layoutErrors":value(&self.layout_errors),"setupStarts":value(&self.setup_starts),
            "setupSuccesses":value(&self.setup_successes),"setupErrors":value(&self.setup_errors),
            "guardErrors":value(&self.guard_errors),"stopErrors":value(&self.stop_errors),
            "delegateStopErrors":value(&self.delegate_stop_errors),
            "waitUs":value(&self.wait_us),"guardBeforeUs":value(&self.guard_before_us),
            "guardAfterUs":value(&self.guard_after_us),"copyUs":value(&self.copy_us),
            "lastFallback":self.last_fallback.lock().unwrap_or_else(|p| p.into_inner()).unwrap_or("none"),
            "nativeReason":last_error.map_or("none", |(reason, _)| reason),
            "nativeErrorCode":last_error.map(|(_, code)| code.as_str())})
    }
}

fn elapsed_us(started: Instant) -> u64 {
    started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
}

fn capture_error(message: &str) -> Error {
    Error::new(Code::Capture, message)
}
fn changed() -> Error {
    Error::new(
        Code::SessionChanged,
        "The native stream's helper session changed",
    )
}
fn cancelled() -> Error {
    Error::new(Code::Cancelled, "The native stream request was cancelled")
}

fn guard(expected: &InputEpoch) -> Result<()> {
    let actual = InputEpoch::from_proof(&native::session_info()?, std::process::id())?;
    validate_epoch(&actual, expected)?;
    if !permitted() {
        return Err(Error::new(
            Code::Permission,
            "Screen Recording is not allowed for Atlas in the helper account",
        ));
    }
    Ok(())
}
fn validate_epoch(actual: &InputEpoch, expected: &InputEpoch) -> Result<()> {
    if actual != expected {
        Err(changed())
    } else {
        Ok(())
    }
}

/// Apple SCStream.h defines displayTime as mach absolute ticks. CoreMedia's
/// host clock and this conversion use precisely those units, not nanoseconds.
fn checkpoint() -> u64 {
    // SAFETY: the immutable system host clock is valid process-wide; the
    // conversion receives its own current CMTime, never a different clock.
    unsafe { CMClock::convert_host_time_to_system_units(CMClock::host_time_clock().time()) }
}

/// `span` in mach ticks.
fn ticks(span: Duration) -> u64 {
    let micros = i64::try_from(span.as_micros()).unwrap_or(i64::MAX);
    // SAFETY: a span in CoreMedia's host-time units; the conversion is
    // linear, so it yields the same span in mach ticks.
    unsafe { CMClock::convert_host_time_to_system_units(CMTime::new(micros, 1_000_000)) }
}

/// The latest display time a sample seen now may carry, in mach ticks.
fn latest() -> u64 {
    checkpoint().saturating_add(ticks(LEAD))
}

#[derive(Clone, PartialEq)]
struct Key {
    epoch: InputEpoch,
    width: usize,
    height: usize,
    scale: f64,
}
struct Pending {
    made: Instant,
    /// When the stream's silence was last asked to answer it.
    hushed: Option<Instant>,
    id: u64,
    checkpoint: u64,
    key: Key,
    alive: Arc<AtomicBool>,
    reply: SyncSender<Result<RawShot>>,
}
#[derive(Default)]
struct Slot {
    pending: Mutex<Option<Pending>>,
    generation: AtomicU64,
    stopping: AtomicBool,
    preparing: AtomicBool,
    /// Whether the screen recorder was restarted since the last request.
    restarted: AtomicBool,
    /// The open stream's state, for one-shot pictures to read without a
    /// request of their own.
    serving: Mutex<Option<Arc<Callback>>>,
    failure: Mutex<Option<(Code, String)>>,
    diagnostics: Diagnostics,
}
impl Slot {
    /// How long the next request, asked for at `asked`, still waits for its
    /// picture: longer once, for the first one after the screen recorder was
    /// restarted, and counted from `asked`.
    fn wait(&self, asked: Instant) -> Duration {
        let wait = if self.restarted.swap(false, Ordering::AcqRel) {
            RESTARTED_WAIT
        } else {
            WAIT
        };
        wait.saturating_sub(asked.elapsed())
    }
    fn quiescent(&self, running: bool) -> bool {
        !running
            && !self.preparing.load(Ordering::Acquire)
            && !self.stopping.load(Ordering::Acquire)
            && self
                .pending
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .is_none()
    }

    fn stop(&self) {
        self.stopping.store(true, Ordering::Release);
        self.invalidate();
        self.cancel();
    }
    /// End the current stream's right to answer. The pending request's lock
    /// orders this against [`Slot::settle`]: a stream's answer lands either
    /// before its invalidation or not at all.
    fn invalidate(&self) {
        let _pending = self.pending.lock().unwrap_or_else(|p| p.into_inner());
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
    fn failure(&self) -> Option<Error> {
        self.failure
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .map(|(code, message)| Error::new(*code, message))
    }
    fn fail(&self, error: Error) {
        let mut pending = self.pending.lock().unwrap_or_else(|p| p.into_inner());
        self.failed(&mut pending, error);
    }
    /// [`Slot::fail`], with the pending request's lock already held.
    fn failed(&self, pending: &mut Option<Pending>, error: Error) {
        *self.failure.lock().unwrap_or_else(|p| p.into_inner()) =
            Some((error.code, error.error.clone()));
        self.stopping.store(true, Ordering::Release);
        self.generation.fetch_add(1, Ordering::AcqRel);
        if let Some(request) = pending.take() {
            request.alive.store(false, Ordering::Release);
            let _ = request.reply.try_send(Err(error));
        }
    }
    /// [`Slot::fail`] for the stream of `generation`, unless that stream was
    /// invalidated meanwhile; a newer stream answers the request then. False
    /// says it was.
    fn fail_from(&self, generation: u64, error: Error) -> bool {
        let mut pending = self.pending.lock().unwrap_or_else(|p| p.into_inner());
        if self.generation.load(Ordering::Acquire) != generation {
            return false;
        }
        self.failed(&mut pending, error);
        true
    }
    /// Hand on the answer of the stream of `generation`, unless that stream
    /// was invalidated meanwhile: a newer stream answers the request then.
    /// False says it was. A failure ends the stream and its request.
    fn settle(&self, generation: u64, answer: Result<Option<(u64, RawShot)>>) -> bool {
        let mut pending = self.pending.lock().unwrap_or_else(|p| p.into_inner());
        if self.generation.load(Ordering::Acquire) != generation {
            return false;
        }
        match answer {
            Ok(Some((id, shot))) => Self::deliver(&mut pending, id, Ok(shot)),
            Ok(None) => {}
            Err(error) => {
                drop(pending);
                self.diagnostics.error("metadata", &error);
                return self.fail_from(generation, error);
            }
        }
        true
    }
    /// The pending request still unanswered after [`SILENCE`], at most
    /// once every [`SILENCE_CHECK`].
    fn overdue(&self) -> Option<u64> {
        let mut pending = self.pending.lock().unwrap_or_else(|p| p.into_inner());
        let request = pending.as_mut().filter(|request| {
            request.alive.load(Ordering::Acquire)
                && request.made.elapsed() >= SILENCE
                && request
                    .hushed
                    .is_none_or(|asked| asked.elapsed() >= SILENCE_CHECK)
        })?;
        request.hushed = Some(Instant::now());
        Some(request.id)
    }
    fn begin(&self, request: Pending) {
        let mut pending = self.pending.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(previous) = pending.replace(request) {
            previous.alive.store(false, Ordering::Release);
            let _ = previous.reply.try_send(Err(cancelled()));
        }
    }
    fn cancel(&self) {
        if let Some(request) = self
            .pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
        {
            request.alive.store(false, Ordering::Release);
            let _ = request.reply.try_send(Err(cancelled()));
        }
    }
    fn finish(&self, id: u64, result: Result<RawShot>) {
        let mut pending = self.pending.lock().unwrap_or_else(|p| p.into_inner());
        Self::deliver(&mut pending, id, result);
    }
    /// End `pending`, the locked request, with `result` when it is request
    /// `id`; one cancelled meanwhile is told nothing.
    fn deliver(pending: &mut Option<Pending>, id: u64, result: Result<RawShot>) {
        if let Some(request) = pending.take_if(|request| request.id == id)
            && request.alive.load(Ordering::Acquire)
        {
            let _ = request.reply.try_send(result);
        }
    }
}
/// Why an open stream did not give a one-shot picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Unserved {
    /// No stream is open.
    Closed,
    /// The open stream shows another session or display.
    Elsewhere,
    /// The open stream's pictures have fewer pixels than asked for.
    Smaller,
    /// The stream proved no picture current in time: the screen is still,
    /// or the stream has no picture.
    Unproven,
}

impl Unserved {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Closed => "no-stream",
            Self::Elsewhere => "other-display",
            Self::Smaller => "smaller",
            Self::Unproven => "unproven",
        }
    }
}

enum Message {
    Capture(Pending),
    Stop(SyncSender<Result<()>>),
}
#[derive(Clone)]
struct Worker {
    messages: SyncSender<Message>,
    slot: Arc<Slot>,
}
static WORKER: OnceLock<Mutex<Option<Worker>>> = OnceLock::new();
thread_local! { static RUNNING: RefCell<Option<Running>> = const { RefCell::new(None) }; }

fn worker() -> Result<Worker> {
    let mut cached = WORKER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| capture_error("Native stream worker state failed"))?;
    if let Some(worker) = &*cached {
        return Ok(worker.clone());
    }
    let (messages, incoming) = mpsc::sync_channel(1);
    let slot = Arc::new(Slot::default());
    let shared = slot.clone();
    std::thread::Builder::new()
        .name("atlas-native-preview".into())
        .spawn(move || run(incoming, shared))?;
    let worker = Worker { messages, slot };
    *cached = Some(worker.clone());
    Ok(worker)
}

/// Reading counters never initializes a worker or starts native capture.
pub fn stream_diagnostics() -> Option<Value> {
    let cached = WORKER.get()?.lock().ok()?;
    Some(cached.as_ref()?.slot.diagnostics.snapshot())
}

/// The display as the open stream shows it now, for a one-shot picture of
/// at most `limit` pixels on its longer side, without a request of its own,
/// so the stream's own requests go on as they were: the stream's newest
/// picture, once a valid sample stamped after this call began says it is
/// current. The copy is bracketed by session, display and consent checks.
/// This never opens a stream, waits up to [`PICTURE_WAIT`], and never waits
/// on the stream's sample queue.
///
/// # Errors
///
/// Returns why the stream gave no picture; the caller then captures it
/// another way.
pub(super) fn streamed_display_rgb(
    display: u32,
    limit: Option<u32>,
) -> std::result::Result<RawShot, Unserved> {
    let state = WORKER
        .get()
        .and_then(|cached| {
            cached
                .lock()
                .ok()?
                .as_ref()
                .map(|worker| worker.slot.clone())
        })
        .and_then(|slot| {
            slot.serving
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone()
        })
        .filter(|state| state.current())
        .ok_or(Unserved::Closed)?;
    let epoch = native::session_info()
        .and_then(|proof| InputEpoch::from_proof(&proof, std::process::id()))
        .map_err(|_| Unserved::Elsewhere)?;
    let mode = CGDisplayCopyDisplayMode(display).ok_or(Unserved::Elsewhere)?;
    let pixels = (
        CGDisplayMode::pixel_width(Some(&mode)),
        CGDisplayMode::pixel_height(Some(&mode)),
    );
    // The display's backing scale is part of what the stream shows: a mode
    // change of the same display and points makes its pictures obsolete.
    if epoch != state.key.epoch
        || epoch.display_id != u64::from(display)
        || pixels.0 as f64 / epoch.width != state.key.scale
    {
        return Err(Unserved::Elsewhere);
    }
    let (width, height) = fit(pixels.0, pixels.1, limit);
    if width > state.key.width || height > state.key.height {
        return Err(Unserved::Smaller);
    }
    let asked = checkpoint();
    let started = Instant::now();
    loop {
        match state.picture_after(asked) {
            Proof::Proven(picture) => return picture.map_err(|_| Unserved::Unproven),
            Proof::Never => return Err(Unserved::Unproven),
            Proof::Waiting if started.elapsed() >= PICTURE_WAIT => {
                return Err(Unserved::Unproven);
            }
            Proof::Waiting => std::thread::sleep(PICTURE_CHECK),
        }
    }
}

/// What an open stream says of a one-shot picture asked for at a moment.
enum Proof {
    /// A valid sample stamped after it says the newest picture is current;
    /// here is its copy.
    Proven(Result<RawShot>),
    /// None has yet, or the stream's sample queue holds the picture now.
    Waiting,
    /// The stream was replaced or stopped, or is due for renewal.
    Never,
}

/// The display as the stream shows it after this call began: a picture
/// delivered after it, or the stream's newest picture when a sample after it
/// says the screen is unchanged or the stream stays silent. A new stream's
/// first picture is waited for, the wait counted from `asked`, when the
/// caller was asked for it. True means the pixels came from the stream.
/// False reports an explicitly revalidated synchronous CoreGraphics capture,
/// taken only when the stream cannot start, failed, or had no picture in
/// time; a wait that ran into a WindowServer stall lasts until a moment after
/// the stall, which held up both alike.
///
/// # Errors
///
/// Returns an error when the session or display changed, consent is missing,
/// the login is not graphical, the source is draining or failed, or neither
/// the native sample nor the guarded fallback could be captured.
pub fn stream_display_rgb(
    display: u32,
    limit: Option<u32>,
    asked: Instant,
) -> Result<(RawShot, bool)> {
    let epoch = InputEpoch::from_proof(&native::session_info()?, std::process::id())?;
    if epoch.display_id != u64::from(display) {
        return Err(changed());
    }
    guard(&epoch)?;
    let tick = checkpoint();
    let mode = CGDisplayCopyDisplayMode(display)
        .ok_or_else(|| capture_error("The stream display has no mode"))?;
    let (source_width, source_height) = (
        CGDisplayMode::pixel_width(Some(&mode)),
        CGDisplayMode::pixel_height(Some(&mode)),
    );
    let (width, height) = fit(source_width, source_height, limit);
    if width == 0 || height == 0 || width.max(height) > 4096 {
        return Err(capture_error("Invalid native stream dimensions"));
    }
    let key = Key {
        epoch: epoch.clone(),
        width,
        height,
        scale: source_width as f64 / epoch.width,
    };
    let worker = worker()?;
    worker
        .slot
        .diagnostics
        .requests
        .fetch_add(1, Ordering::Relaxed);
    if worker.slot.stopping.load(Ordering::Acquire) {
        return Err(worker.slot.failure().unwrap_or_else(cancelled));
    }
    let alive = Arc::new(AtomicBool::new(true));
    let (send, receive) = mpsc::sync_channel(1);
    static SEQUENCE: AtomicU64 = AtomicU64::new(1);
    let request = Pending {
        made: Instant::now(),
        hushed: None,
        id: SEQUENCE.fetch_add(1, Ordering::Relaxed),
        checkpoint: tick,
        key,
        alive: alive.clone(),
        reply: send,
    };
    let sent = !worker.slot.stopping.load(Ordering::Acquire)
        && worker.messages.try_send(Message::Capture(request)).is_ok();
    let waiting = Instant::now();
    let wait = worker.slot.wait(asked);
    let mut outcome = sent
        .then(|| receive.recv_timeout(wait))
        .and_then(std::result::Result::ok);
    if sent && outcome.is_none() && !worker.slot.stopping.load(Ordering::Acquire) {
        // A failed check earns no wait; the fallback's own check refuses.
        let check = || {
            let checking = Instant::now();
            guard(&epoch).is_ok() && checking.elapsed() >= STALLED
        };
        outcome = after_stall(&receive, SILENCE, check, &worker.slot.diagnostics);
    }
    worker
        .slot
        .diagnostics
        .wait_us
        .store(elapsed_us(waiting), Ordering::Relaxed);
    if sent && outcome.is_none() {
        worker
            .slot
            .diagnostics
            .timeouts
            .fetch_add(1, Ordering::Relaxed);
    }
    alive.store(false, Ordering::Release);
    match outcome {
        Some(Ok(shot)) => {
            guard(&epoch)?;
            worker
                .slot
                .diagnostics
                .native_frames
                .fetch_add(1, Ordering::Relaxed);
            Ok((shot, true))
        }
        Some(Err(error)) if !fallback_allowed(&error) => Err(error),
        outcome => {
            if worker.slot.stopping.load(Ordering::Acquire) {
                return Err(worker.slot.failure().unwrap_or_else(cancelled));
            }
            guard(&epoch)?;
            let shot = super::core_graphics_rgb(display, limit)?;
            guard(&epoch)?;
            let reason = if !sent {
                "queue-full"
            } else if outcome.is_some() {
                "native-error"
            } else if worker.slot.preparing.load(Ordering::Acquire) {
                "starting"
            } else {
                "timeout"
            };
            worker.slot.diagnostics.fell_back(reason);
            Ok((shot, false))
        }
    }
}
/// The answer to a request whose wait ran out, once `check`, the session
/// check a CoreGraphics capture needs first, has said whether it passed and
/// was held up.
/// One held up says WindowServer stalled the whole session: the stream could
/// send nothing meanwhile, and a CoreGraphics capture would have waited out
/// the same stall. So the stream then has up to `grace` more, from the end
/// of the stall, to answer. An answer that came during the check is taken
/// either way.
fn after_stall(
    receive: &Receiver<Result<RawShot>>,
    grace: Duration,
    check: impl FnOnce() -> bool,
    diagnostics: &Diagnostics,
) -> Option<Result<RawShot>> {
    if check() {
        diagnostics.stalls.fetch_add(1, Ordering::Relaxed);
        return receive.recv_timeout(grace).ok();
    }
    receive.try_recv().ok()
}

fn fallback_allowed(error: &Error) -> bool {
    error.code == Code::Capture
}

/// Note that macOS's screen recorder was restarted, so the next request,
/// which opens a stream on the new recorder, waits longer for its first
/// picture. This never starts the stream worker.
pub fn recorder_restarted() {
    let worker = WORKER.get().and_then(|cached| {
        cached
            .lock()
            .ok()?
            .as_ref()
            .map(|worker| worker.slot.clone())
    });
    if let Some(slot) = worker {
        slot.restarted.store(true, Ordering::Release);
    }
}

/// Invalidate delivery before waiting for the native stop and callback queue.
/// A timed-out stop leaves the one native source quarantined, never reopened.
///
/// # Errors
///
/// Returns an error when another stop is pending or the native stop and
/// callback drain were not confirmed in time.
pub fn stop_display_stream() -> Result<()> {
    let worker = worker()?;
    worker.slot.stop();
    let (send, receive) = mpsc::sync_channel(1);
    worker
        .messages
        .try_send(Message::Stop(send))
        .map_err(|_| capture_error("Native stream stop is already pending"))?;
    receive
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| capture_error("Native stream callback drain is still pending"))?
}

struct Watchdog(Instant);
impl Watchdog {
    fn poll(&mut self, now: Instant, slot: &Slot, check: impl FnOnce() -> Result<()>) {
        if now.duration_since(self.0) < Duration::from_millis(20)
            || slot.stopping.load(Ordering::Acquire)
        {
            return;
        }
        self.0 = now;
        if let Err(error) = check() {
            slot.diagnostics
                .guard_errors
                .fetch_add(1, Ordering::Relaxed);
            slot.diagnostics.error("guard", &error);
            slot.fail(error);
        }
    }
}

fn receive(
    incoming: &Receiver<Message>,
    slot: &Slot,
    running: bool,
) -> std::result::Result<Message, mpsc::RecvTimeoutError> {
    if slot.quiescent(running) {
        // No native source, preparation callback, stop, or request remains.
        // Capture, Stop, and sender departure all wake this blocking receive.
        incoming
            .recv()
            .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
    } else {
        incoming.recv_timeout(Duration::from_millis(4))
    }
}

fn run(incoming: Receiver<Message>, slot: Arc<Slot>) {
    // launchd's manager type is fixed for this process. Session/UID/display
    // facts and consent are still queried afresh before and after every copy.
    let aqua = native::manager_name() == "Aqua";
    let Some(run_loop) = CFRunLoop::current() else {
        return;
    };
    let mut preparing: Option<Key> = None;
    let mut watchdog = Watchdog(Instant::now());
    loop {
        // SAFETY: the default mode constant is a valid, immutable CFString.
        let Some(mode) = (unsafe { kCFRunLoopDefaultMode }) else {
            return;
        };
        objc2::rc::autoreleasepool(|_| CFRunLoop::run_in_mode(Some(mode), 0.001, true));
        watchdog.poll(Instant::now(), &slot, || {
            let expected = RUNNING.with(|running| {
                running
                    .borrow()
                    .as_ref()
                    .map(|source| source.key.epoch.clone())
            });
            expected.as_ref().map_or(Ok(()), guard)
        });
        // A stream due for renewal is replaced before a request needs it,
        // so its picture is proven again by the new stream's first.
        let silent = RUNNING.with(|running| {
            running
                .borrow()
                .as_ref()
                .filter(|source| source.state.due())
                .map(|source| source.key.clone())
        });
        if let Some(key) = silent
            && !slot.stopping.load(Ordering::Acquire)
        {
            slot.diagnostics.renewals.fetch_add(1, Ordering::Relaxed);
            if replace(&key, &slot, &run_loop) {
                preparing = Some(key);
            }
        }
        if let Some(id) = slot.overdue() {
            RUNNING.with(|running| {
                if let Some(source) = running.borrow().as_ref() {
                    let state = source.state.clone();
                    // On the stream's own queue, behind every sample it has
                    // delivered, so none newer is still waiting there.
                    source.queue.exec_async(move || state.silent(id));
                }
            });
        }
        if slot.stopping.load(Ordering::Acquire) && stop_native(&slot, false).is_ok_and(|done| done)
        {
            slot.stopping.store(false, Ordering::Release);
            preparing = None;
        }
        match receive(&incoming, &slot, RUNNING.with(|r| r.borrow().is_some())) {
            Ok(Message::Stop(reply)) => {
                let result = stop_native(&slot, true).and_then(|done| {
                    if done {
                        Ok(())
                    } else {
                        Err(capture_error("Native stream stop is still pending"))
                    }
                });
                if result.is_ok() {
                    slot.stopping.store(false, Ordering::Release);
                    preparing = None;
                }
                let _ = reply.try_send(result);
            }
            Ok(Message::Capture(request)) => {
                if !request.alive.load(Ordering::Acquire) {
                    continue;
                }
                if !aqua {
                    let _ = request.reply.try_send(Err(Error::new(
                        Code::Identity,
                        "The native stream requires an Aqua helper login",
                    )));
                    continue;
                }
                if slot.stopping.load(Ordering::Acquire) {
                    let _ = request
                        .reply
                        .try_send(Err(capture_error("The previous native stream is draining")));
                    continue;
                }
                if let Err(error) = guard(&request.key.epoch) {
                    slot.diagnostics
                        .guard_errors
                        .fetch_add(1, Ordering::Relaxed);
                    slot.diagnostics.error("guard", &error);
                    let _ = request.reply.try_send(Err(error));
                    continue;
                }
                let matches = RUNNING.with(|running| {
                    running
                        .borrow()
                        .as_ref()
                        .is_some_and(|source| source.key == request.key)
                });
                if !matches
                    && (preparing.as_ref() != Some(&request.key)
                        || !slot.preparing.load(Ordering::Acquire))
                {
                    if !replace(&request.key, &slot, &run_loop) {
                        let _ = request
                            .reply
                            .try_send(Err(capture_error("The previous native stream is draining")));
                        continue;
                    }
                    preparing = Some(request.key.clone());
                }
                slot.begin(request);
            }
            // The binary connection owns this source through StreamStop.
            // Request gaps are backpressure, not ownership cancellation. The
            // daemon bounds idle connections; the watchdog above still checks
            // session identity and consent while no frames are requested.
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = stop_native(&slot, true);
                return;
            }
        }
    }
}

/// The newest complete picture of one stream, served again while the stream
/// reports the screen unchanged.
struct Newest(CFRetained<CVPixelBuffer>);

// SAFETY: CoreVideo buffers are reference counted atomically. This one is
// only read under a read-only base-address lock while its mutex is held, and
// ScreenCaptureKit's buffer pool reuses no buffer while it is retained.
unsafe impl Send for Newest {}

/// Stop any stream and its callbacks, then start one for `key`. False while
/// the old stream's stop is still unconfirmed; it then stays draining.
fn replace(key: &Key, slot: &Arc<Slot>, run_loop: &CFRetained<CFRunLoop>) -> bool {
    slot.invalidate();
    if !stop_native(slot, true).is_ok_and(|done| done) {
        slot.stopping.store(true, Ordering::Release);
        return false;
    }
    *slot.failure.lock().unwrap_or_else(|p| p.into_inner()) = None;
    prepare(key.clone(), slot.clone(), run_loop.clone());
    true
}

struct Callback {
    key: Key,
    slot: Arc<Slot>,
    generation: u64,
    active: AtomicBool,
    /// Only samples of this stream reach it, in the order they were made, so
    /// a new stream starts with none.
    newest: Mutex<Option<Newest>>,
    /// When this stream last delivered a sample of any kind, or started, in
    /// mach ticks.
    heard: AtomicU64,
    /// The display time of its last valid sample, or when it started: a
    /// complete or unchanged one stamped no earlier than the one before and
    /// not ahead of [`LEAD`].
    stamped: AtomicU64,
    /// When that sample's callback began, in mach ticks, before it waited for
    /// anything; 0 before the first. Written under `newest`, with `stamped`.
    received: AtomicU64,
    /// The session, display and consent check around every copy.
    guard: fn(&InputEpoch) -> Result<()>,
}
impl Callback {
    fn new(key: Key, slot: Arc<Slot>, generation: u64) -> Self {
        Self {
            key,
            slot,
            generation,
            active: AtomicBool::new(true),
            newest: Mutex::new(None),
            heard: AtomicU64::new(checkpoint()),
            stamped: AtomicU64::new(checkpoint()),
            received: AtomicU64::new(0),
            guard,
        }
    }

    fn current(&self) -> bool {
        self.active.load(Ordering::Acquire)
            && self.slot.generation.load(Ordering::Acquire) == self.generation
    }

    /// Whether this stream has delivered nothing at all for `span`.
    fn quiet(&self, span: Duration) -> bool {
        checkpoint().saturating_sub(self.heard.load(Ordering::Acquire)) > ticks(span)
    }

    /// Whether this stream has delivered no valid sample for `span`.
    fn stale(&self, span: Duration) -> bool {
        checkpoint().saturating_sub(self.stamped.load(Ordering::Acquire)) > ticks(span)
    }

    /// Whether this stream is due for replacement: it has delivered no valid
    /// sample for [`SILENT_RENEW`], or for [`RENEW`] while it has no picture
    /// to serve again. A still screen leaves a stream silent, with its
    /// newest picture still the screen as it is.
    fn due(&self) -> bool {
        self.stale(SILENT_RENEW)
            || (self.stale(RENEW)
                && self
                    .newest
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .is_none())
    }

    /// The newest picture, copied, once a valid sample stamped and received
    /// after `asked` says it is current. Never blocks on the sample queue: while it
    /// holds the picture, this is still waiting.
    fn picture_after(&self, asked: u64) -> Proof {
        if !self.current() || self.stale(SILENT_RENEW) {
            return Proof::Never;
        }
        let newest = match self.newest.try_lock() {
            Ok(newest) => newest,
            Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return Proof::Waiting,
        };
        // Under the lock, the newest picture is at least as new as the
        // stamp. A sample may be stamped a moment ahead, so one received
        // before the ask proves nothing of the screen after it.
        if self.stamped.load(Ordering::Acquire) <= asked
            || self.received.load(Ordering::Acquire) <= asked
        {
            return Proof::Waiting;
        }
        match newest.as_ref() {
            Some(Newest(buffer)) => Proof::Proven(self.copy(buffer, false)),
            // The last sample proved nothing, or no picture came yet.
            None => Proof::Waiting,
        }
    }

    fn stopped(&self, check: impl FnOnce() -> Result<()>) {
        if !self.active.swap(false, Ordering::AcqRel)
            || self.slot.generation.load(Ordering::Acquire) != self.generation
        {
            return;
        }
        let error = match check() {
            Ok(()) => {
                let error = capture_error("ScreenCaptureKit stopped producing samples");
                self.slot.diagnostics.error("stop-error", &error);
                error
            }
            Err(error) => {
                self.slot
                    .diagnostics
                    .guard_errors
                    .fetch_add(1, Ordering::Relaxed);
                self.slot.diagnostics.error("guard", &error);
                error
            }
        };
        self.slot
            .diagnostics
            .delegate_stop_errors
            .fetch_add(1, Ordering::Relaxed);
        self.slot.fail_from(self.generation, error);
    }

    fn sample(&self, sample: &CMSampleBuffer) {
        self.slot
            .diagnostics
            .callbacks
            .fetch_add(1, Ordering::Relaxed);
        if !self.current() {
            return;
        }
        let arrived = checkpoint();
        self.heard.store(arrived, Ordering::Release);
        let answer = self.answer(sample, arrived);
        self.settle(answer);
    }

    /// Answer request `id`, still unanswered after [`SILENCE`], with the
    /// newest picture once the stream has delivered nothing at all for
    /// [`SILENCE`]. This runs on the stream's sample queue, after every
    /// sample it delivered: the stream saw no change since that picture,
    /// as it sends a sample within a refresh of any. A stream without a
    /// picture yet leaves the request waiting for its first, and one due for
    /// renewal leaves it to the replacement.
    fn silent(&self, id: u64) {
        if !self.current() || !self.quiet(SILENCE) || self.due() {
            return;
        }
        let newest = self.newest.lock().unwrap_or_else(|p| p.into_inner());
        let Some(Newest(buffer)) = newest.as_ref() else {
            return;
        };
        if self.asking().is_none_or(|(asking, _)| asking != id) {
            return;
        }
        let answer = self.copy(buffer, true).map(|shot| Some((id, shot)));
        self.settle(answer);
    }

    /// Hand `answer` on, unless this stream was replaced or stopped
    /// meanwhile: whoever did that owns the pending request now, and a new
    /// stream answers it.
    fn settle(&self, answer: Result<Option<(u64, RawShot)>>) {
        if !self.active.load(Ordering::Acquire) {
            return;
        }
        let failed = answer.is_err();
        if self.slot.settle(self.generation, answer) && failed {
            self.active.store(false, Ordering::Release);
        }
    }

    /// The pending request for this stream, and its checkpoint.
    fn asking(&self) -> Option<(u64, u64)> {
        let pending = self.slot.pending.lock().unwrap_or_else(|p| p.into_inner());
        pending
            .as_ref()
            .filter(|r| r.key == self.key && r.alive.load(Ordering::Acquire))
            .map(|request| (request.id, request.checkpoint))
    }

    /// Note what `sample` says of the screen, then the picture it answers
    /// the pending request with, if it answers it.
    fn answer(&self, sample: &CMSampleBuffer, arrived: u64) -> Result<Option<(u64, RawShot)>> {
        let metadata = metadata(sample)?;
        // Only this stream's sample queue holds this lock for longer than a
        // look, and takes the pending request's only briefly while holding
        // it; the worker looks at it holding no other lock.
        let mut newest = self.newest.lock().unwrap_or_else(|p| p.into_inner());
        // Every sample, asked for or not, says what the screen shows now: a
        // valid complete one is the newest picture, a valid unchanged one
        // keeps it, and anything else (blank, suspended, stopped, unreadable,
        // unstamped, out of order or stamped too far ahead) leaves no picture
        // that may be served again.
        let previous = self.stamped.load(Ordering::Acquire);
        let valid = metadata.as_ref().and_then(|metadata| {
            metadata
                .time
                .filter(|time| (previous..=latest()).contains(time))
                .map(|time| (metadata.status, time))
        });
        match valid {
            Some((status, time)) if status == SCFrameStatus::Complete.0 as i64 => {
                // SAFETY: a live SCK screen sample; its image buffer is retained.
                *newest = unsafe { sample.image_buffer() }.map(Newest);
                self.stamped.store(time, Ordering::Release);
                self.received.store(arrived, Ordering::Release);
            }
            Some((status, time)) if status == SCFrameStatus::Idle.0 as i64 => {
                self.stamped.store(time, Ordering::Release);
                self.received.store(arrived, Ordering::Release);
            }
            _ => *newest = None,
        }
        let Some((id, after)) = self.asking() else {
            self.slot
                .diagnostics
                .no_request
                .fetch_add(1, Ordering::Relaxed);
            return Ok(None);
        };
        let Some(metadata) = metadata else {
            self.slot
                .diagnostics
                .missing_metadata
                .fetch_add(1, Ordering::Relaxed);
            return Ok(None);
        };
        let reused = match eligibility(metadata, after, latest()) {
            Eligibility::Frame => false,
            Eligibility::Unchanged => {
                self.slot
                    .diagnostics
                    .idle_hints
                    .fetch_add(1, Ordering::Relaxed);
                // Without a picture yet, the request waits for the first.
                if newest.is_none() {
                    return Ok(None);
                }
                true
            }
            Eligibility::Old => {
                self.slot
                    .diagnostics
                    .old_samples
                    .fetch_add(1, Ordering::Relaxed);
                return Ok(None);
            }
            Eligibility::Future => {
                self.slot
                    .diagnostics
                    .future_samples
                    .fetch_add(1, Ordering::Relaxed);
                return Ok(None);
            }
            Eligibility::Incomplete => {
                self.slot
                    .diagnostics
                    .missing_metadata
                    .fetch_add(1, Ordering::Relaxed);
                return Ok(None);
            }
            Eligibility::OtherStatus => return Ok(None),
        };
        let Some(Newest(buffer)) = newest.as_ref() else {
            self.slot
                .diagnostics
                .layout_errors
                .fetch_add(1, Ordering::Relaxed);
            return Err(capture_error("Native frame has no pixels")
                .with("nativeReason", "format-layout".into()));
        };
        self.copy(buffer, reused).map(|shot| Some((id, shot)))
    }

    /// `buffer`'s pixels, bracketed by fresh session, display and consent
    /// checks, while this stream is still the current one. `reused` says it
    /// is the newest picture served again.
    fn copy(&self, buffer: &CVPixelBuffer, reused: bool) -> Result<RawShot> {
        let before = Instant::now();
        let guarded = (self.guard)(&self.key.epoch);
        self.slot
            .diagnostics
            .guard_before_us
            .store(elapsed_us(before), Ordering::Relaxed);
        if let Err(error) = guarded {
            self.slot
                .diagnostics
                .guard_errors
                .fetch_add(1, Ordering::Relaxed);
            return Err(error.with("nativeReason", "guard".into()));
        }
        let copying = Instant::now();
        let rgb = pixels(buffer, self.key.width, self.key.height);
        self.slot
            .diagnostics
            .copy_us
            .store(elapsed_us(copying), Ordering::Relaxed);
        let rgb = rgb.map_err(|error| {
            self.slot
                .diagnostics
                .layout_errors
                .fetch_add(1, Ordering::Relaxed);
            error.with("nativeReason", "format-layout".into())
        })?;
        let after = Instant::now();
        let guarded = (self.guard)(&self.key.epoch);
        self.slot
            .diagnostics
            .guard_after_us
            .store(elapsed_us(after), Ordering::Relaxed);
        if let Err(error) = guarded {
            self.slot
                .diagnostics
                .guard_errors
                .fetch_add(1, Ordering::Relaxed);
            return Err(error.with("nativeReason", "guard".into()));
        }
        if !self.current() {
            return Err(cancelled());
        }
        if reused {
            self.slot
                .diagnostics
                .reused_frames
                .fetch_add(1, Ordering::Relaxed);
        }
        Ok(RawShot {
            rgb,
            width: self.key.width as u32,
            height: self.key.height as u32,
            scale: self.key.scale,
        })
    }
}

// SAFETY: NSObject has no subclassing constraints; ivars are thread-safe Rust
// state. Only callback methods touch them, and no native object is sent via Rust.
define_class!(
    #[unsafe(super(NSObject))]
    #[name = "AtlasExperimentalStreamOutput"]
    #[ivars = Arc<Callback>]
    struct Output;
    // SAFETY: NSObjectProtocol has no additional requirements.
    unsafe impl NSObjectProtocol for Output {}
    // SAFETY: the callback signature exactly matches ScreenCaptureKit's protocol.
    unsafe impl SCStreamOutput for Output {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn output(&self, _stream: &SCStream, sample: &CMSampleBuffer, kind: SCStreamOutputType) {
            if kind == SCStreamOutputType::Screen { self.ivars().sample(sample); }
        }
    }
    // SAFETY: the error callback signature matches ScreenCaptureKit's protocol.
    unsafe impl SCStreamDelegate for Output {
        #[unsafe(method(stream:didStopWithError:))]
        fn stopped(&self, _stream: &SCStream, _error: &NSError) {
            self.ivars().stopped(|| guard(&self.ivars().key.epoch));
        }
    }
);

struct Running {
    key: Key,
    stream: Retained<SCStream>,
    _output: Retained<Output>,
    _filter: Retained<SCContentFilter>,
    _configuration: Retained<SCStreamConfiguration>,
    _color_space: CFRetained<CFString>,
    queue: DispatchRetained<DispatchQueue>,
    state: Arc<Callback>,
    stopped: Option<Receiver<Result<()>>>,
    stop_error: Option<String>,
}
impl Running {
    fn start(
        content: &SCShareableContent,
        key: Key,
        slot: Arc<Slot>,
        generation: u64,
    ) -> Result<Self> {
        guard(&key.epoch)?;
        // SAFETY: immutable shareable content from SCK's completion, now solely
        // accessed on the native owner thread. Only the exact pinned ID is used.
        let displays = unsafe { content.displays() };
        let display = displays
            .iter()
            .find(|display| {
                // SAFETY: live SCDisplay returned by ScreenCaptureKit.
                unsafe { display.displayID() == key.epoch.display_id as u32 }
            })
            .ok_or_else(|| {
                capture_error("ScreenCaptureKit did not expose the exact helper display")
                    .with("nativeReason", "no-exact-display".into())
            })?;
        let _display: &SCDisplay = &display;
        let excluded = NSArray::<SCWindow>::new();
        let state = Arc::new(Callback::new(key.clone(), slot, generation));
        let this = Output::alloc().set_ivars(state.clone());
        // SAFETY: NSObject initialization after all declared Rust ivars were set.
        let output: Retained<Output> = unsafe { msg_send![super(this), init] };
        let queue = DispatchQueue::new("dev.atlas.preview.samples", None);
        // SCStreamConfiguration.colorSpaceName is assign, not retain: keep
        // its owned CFString alive through stop and callback-queue drain.
        let color_space = CFString::from_static_str("kCGColorSpaceSRGB");
        // SAFETY: the display, filter, configuration and delegate are retained
        // for the full stream lifetime. All options use documented SDR values.
        let (stream, filter, configuration) = unsafe {
            let filter = SCContentFilter::initWithDisplay_excludingWindows(
                SCContentFilter::alloc(),
                &display,
                &excluded,
            );
            let configuration = SCStreamConfiguration::new();
            configuration.setWidth(key.width);
            configuration.setHeight(key.height);
            configuration.setPixelFormat(kCVPixelFormatType_32BGRA);
            configuration.setColorSpaceName(&color_space);
            configuration.setMinimumFrameInterval(CMTime::new(1, PICTURES_A_SECOND));
            configuration.setQueueDepth(3);
            configuration.setShowsCursor(false);
            if configuration.respondsToSelector(sel!(setPreservesAspectRatio:)) {
                configuration.setPreservesAspectRatio(false);
            }
            if configuration.respondsToSelector(sel!(setCaptureDynamicRange:)) {
                configuration.setCaptureDynamicRange(SCCaptureDynamicRange::SDR);
            }
            if configuration.respondsToSelector(sel!(setCapturesAudio:)) {
                configuration.setCapturesAudio(false);
            }
            if configuration.respondsToSelector(sel!(setShouldBeOpaque:)) {
                configuration.setShouldBeOpaque(true);
            }
            let stream = SCStream::initWithFilter_configuration_delegate(
                SCStream::alloc(),
                &filter,
                &configuration,
                Some(ProtocolObject::from_ref(&*output)),
            );
            stream
                .addStreamOutput_type_sampleHandlerQueue_error(
                    ProtocolObject::from_ref(&*output),
                    SCStreamOutputType::Screen,
                    Some(&queue),
                )
                .map_err(|_| capture_error("ScreenCaptureKit rejected its output"))?;
            let started_state = state.clone();
            let started = RcBlock::new(move |error: *mut NSError| {
                if !error.is_null()
                    && started_state.slot.generation.load(Ordering::Acquire)
                        == started_state.generation
                {
                    started_state.active.store(false, Ordering::Release);
                    let error = guard(&started_state.key.epoch)
                        .err()
                        .unwrap_or_else(|| capture_error("ScreenCaptureKit could not start"));
                    started_state
                        .slot
                        .diagnostics
                        .setup_errors
                        .fetch_add(1, Ordering::Relaxed);
                    started_state.slot.diagnostics.error("start-error", &error);
                    started_state
                        .slot
                        .fail_from(started_state.generation, error);
                } else if error.is_null() {
                    started_state
                        .slot
                        .diagnostics
                        .setup_successes
                        .fetch_add(1, Ordering::Relaxed);
                }
            });
            stream.startCaptureWithCompletionHandler(Some(&started));
            (stream, filter, configuration)
        };
        Ok(Self {
            key,
            stream,
            _output: output,
            _filter: filter,
            _configuration: configuration,
            _color_space: color_space,
            queue,
            state,
            stopped: None,
            stop_error: None,
        })
    }
}

fn prepare(key: Key, slot: Arc<Slot>, run_loop: CFRetained<CFRunLoop>) {
    if slot.preparing.swap(true, Ordering::AcqRel) {
        return;
    }
    let generation = slot.generation.load(Ordering::Acquire);
    slot.diagnostics
        .setup_starts
        .fetch_add(1, Ordering::Relaxed);
    let block = RcBlock::new(
        move |content: *mut SCShareableContent, error: *mut NSError| {
            if slot.generation.load(Ordering::Acquire) != generation {
                slot.preparing.store(false, Ordering::Release);
                return;
            }
            // SAFETY: SCK's completion supplies a live content object or error;
            // retain ownership before the callback returns, without inspecting it.
            let content = if error.is_null() {
                // SAFETY: successful SCK completion supplied this live object.
                unsafe { Retained::retain(content) }
            } else {
                None
            };
            let slot = slot.clone();
            let key = key.clone();
            let scheduled = RcBlock::new(move || {
                slot.preparing.store(false, Ordering::Release);
                if slot.generation.load(Ordering::Acquire) != generation {
                    return;
                }
                let result = content
                    .as_deref()
                    .ok_or_else(|| {
                        capture_error("ScreenCaptureKit shareable content is unavailable")
                            .with("nativeReason", "shareable-content".into())
                    })
                    .and_then(|content| {
                        Running::start(content, key.clone(), slot.clone(), generation)
                    });
                match result {
                    Ok(source) => {
                        *slot.serving.lock().unwrap_or_else(|p| p.into_inner()) =
                            Some(source.state.clone());
                        RUNNING.with(|running| *running.borrow_mut() = Some(source));
                    }
                    Err(error) => {
                        slot.diagnostics
                            .setup_errors
                            .fetch_add(1, Ordering::Relaxed);
                        slot.diagnostics.error("start-error", &error);
                        let id = slot
                            .pending
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .as_ref()
                            .map(|r| r.id);
                        if let Some(id) = id {
                            slot.finish(id, Err(error));
                        }
                    }
                }
            });
            // SAFETY: CFRunLoop copies this block and runs it only on its owner.
            // The retained immutable SCK content is handed off once to that worker;
            // SCStream/display/filter access and ownership never enter Rust channels.
            unsafe {
                if let Some(mode) = kCFRunLoopDefaultMode {
                    run_loop.perform_block(Some(mode.as_ref()), Some(&scheduled));
                }
            }
            run_loop.wake_up();
        },
    );
    // SAFETY: the retained completion copies its captures, and marshals native
    // content back to the sole worker before any native state is inspected.
    unsafe {
        SCShareableContent::getShareableContentWithCompletionHandler(&block);
    }
}

fn stop_native(slot: &Slot, wait: bool) -> Result<bool> {
    let stopped = RUNNING.with(|running| -> Result<bool> {
        let mut running = running.borrow_mut();
        let Some(source) = running.as_mut() else {
            return Ok(true);
        };
        source.state.active.store(false, Ordering::Release);
        if source.stopped.is_none() {
            let (send, receive) = mpsc::sync_channel(1);
            let completion = RcBlock::new(move |error: *mut NSError| {
                let result = if error.is_null() {
                    Ok(())
                } else {
                    Err(capture_error(
                        "ScreenCaptureKit did not confirm stopping; its source remains quarantined",
                    ))
                };
                let _ = send.try_send(result);
            });
            // SAFETY: a live stream; completion contains only owned Rust state.
            unsafe {
                source
                    .stream
                    .stopCaptureWithCompletionHandler(Some(&completion));
            }
            source.stopped = Some(receive);
        }
        let stopped = source.stopped.as_ref().expect("stop receiver");
        let confirmed = confirmed_stop(stopped, &mut source.stop_error, wait);
        if let Err(error) = &confirmed {
            slot.diagnostics.stop_errors.fetch_add(1, Ordering::Relaxed);
            slot.diagnostics.error("stop-error", error);
        }
        if !confirmed? {
            return Ok(false);
        }
        // The delegate cannot access its Rust state after this drain. If a
        // callback is slow, its one source stays quarantined on this worker.
        source.queue.exec_sync(|| {});
        *slot.serving.lock().unwrap_or_else(|p| p.into_inner()) = None;
        *running = None;
        Ok(true)
    })?;
    Ok(stopped && !slot.preparing.load(Ordering::Acquire))
}

fn confirmed_stop(
    receiver: &Receiver<Result<()>>,
    failed: &mut Option<String>,
    wait: bool,
) -> Result<bool> {
    if let Some(error) = failed {
        return Err(capture_error(error));
    }
    let outcome = if wait {
        match receiver.recv_timeout(STOP_WAIT) {
            Ok(result) => Some(result),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Some(Err(capture_error("Native stop acknowledgement was lost")))
            }
        }
    } else {
        match receiver.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err(capture_error("Native stop acknowledgement was lost")))
            }
        }
    };
    match outcome {
        Some(Ok(())) => Ok(true),
        Some(Err(error)) => {
            *failed = Some(error.error.clone());
            Err(error)
        }
        None => Ok(false),
    }
}

#[derive(Debug, PartialEq)]
struct Metadata {
    status: i64,
    time: Option<u64>,
}
/// What one sample says to a request made at a mach-clock checkpoint.
#[derive(Debug, PartialEq)]
enum Eligibility {
    /// A new picture, displayed after the checkpoint.
    Frame,
    /// The screen, after the checkpoint, unchanged since the stream's newest
    /// picture.
    Unchanged,
    Old,
    Future,
    Incomplete,
    OtherStatus,
}
/// Only a complete or unchanged sample stamped strictly after `checkpoint`,
/// and not after `latest`, answers a request.
fn eligibility(metadata: Metadata, checkpoint: u64, latest: u64) -> Eligibility {
    let unchanged = metadata.status == SCFrameStatus::Idle.0 as i64;
    if !unchanged && metadata.status != SCFrameStatus::Complete.0 as i64 {
        return Eligibility::OtherStatus;
    }
    let Some(time) = metadata.time else {
        return Eligibility::Incomplete;
    };
    if checkpoint == 0 {
        return Eligibility::Incomplete;
    }
    if time <= checkpoint {
        return Eligibility::Old;
    }
    if time > latest {
        return Eligibility::Future;
    }
    if unchanged {
        Eligibility::Unchanged
    } else {
        Eligibility::Frame
    }
}

fn metadata(sample: &CMSampleBuffer) -> Result<Option<Metadata>> {
    // SAFETY: a live SCK video sample; attachments are documented CF objects.
    let Some(attachments) = (unsafe { sample.sample_attachments_array(false) }) else {
        return Ok(None);
    };
    // SAFETY: sample attachments contain CF objects, checked before each cast.
    let attachments = unsafe { CFRetained::cast_unchecked::<CFArray<CFType>>(attachments) };
    let Some(first) = attachments.get(0) else {
        return Ok(None);
    };
    let dictionary = first
        .downcast::<CFDictionary>()
        .map_err(|_| capture_error("Malformed native frame metadata"))?;
    // SAFETY: SCK's metadata dictionary uses string keys and CF object values.
    let dictionary =
        unsafe { CFRetained::cast_unchecked::<CFDictionary<CFString, CFType>>(dictionary) };
    // SAFETY: SCK exports these immutable string constants. NSString and
    // CFString keys compare by value through their documented toll-free bridge.
    let (status_key, time_key) = unsafe { (SCStreamFrameInfoStatus, SCStreamFrameInfoDisplayTime) };
    let number = |key: &str| {
        dictionary
            .get(&CFString::from_str(key))
            .and_then(|value| value.downcast::<CFNumber>().ok())
            .and_then(|value| value.as_i64())
    };
    let Some(status) = number(&status_key.to_string()) else {
        return Ok(None);
    };
    let time = number(&time_key.to_string()).and_then(|time| u64::try_from(time).ok());
    Ok(Some(Metadata { status, time }))
}

fn pixels(buffer: &CVPixelBuffer, width: usize, height: usize) -> Result<Buffer> {
    if CVPixelBufferGetPixelFormatType(buffer) != kCVPixelFormatType_32BGRA
        || CVPixelBufferIsPlanar(buffer)
        || CVPixelBufferGetWidth(buffer) != width
        || CVPixelBufferGetHeight(buffer) != height
    {
        return Err(capture_error("Unexpected native pixel layout"));
    }
    // SAFETY: lock and unlock use the same read-only flag; the retained buffer
    // outlives the lock and all reads. No mutable access is exposed.
    if unsafe { CVPixelBufferLockBaseAddress(buffer, CVPixelBufferLockFlags::ReadOnly) } != 0 {
        return Err(capture_error("Native pixels could not be locked"));
    }
    struct Unlock<'a>(&'a CVPixelBuffer);
    impl Drop for Unlock<'_> {
        fn drop(&mut self) {
            // SAFETY: paired with the successful read-only lock above.
            let _ =
                unsafe { CVPixelBufferUnlockBaseAddress(self.0, CVPixelBufferLockFlags::ReadOnly) };
        }
    }
    let _unlock = Unlock(buffer);
    let stride = CVPixelBufferGetBytesPerRow(buffer);
    let length = stride
        .checked_mul(height)
        .filter(|length| {
            *length <= isize::MAX as usize && *length <= CVPixelBufferGetDataSize(buffer)
        })
        .ok_or_else(|| capture_error("Native pixels exceed their allocation"))?;
    let data = CVPixelBufferGetBaseAddress(buffer).cast::<u8>();
    if data.is_null() {
        return Err(capture_error("Native pixels have no base address"));
    }
    // SAFETY: read-only locked, retained buffer; checked allocation and slice
    // bounds. This borrow ends before Unlock releases the buffer lock.
    let bytes = unsafe { std::slice::from_raw_parts(data, length) };
    pack(bytes, width, height, stride, "Invalid native bitmap stride")
}

#[cfg(test)]
mod tests;
