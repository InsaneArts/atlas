//! Experimental ScreenCaptureKit source. Objective-C state stays on one native
//! worker run loop. Callbacks send only checked, owned RGB bytes to callers.
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

const WAIT: Duration = Duration::from_millis(35);
const STOP_WAIT: Duration = Duration::from_millis(200);

#[derive(Default)]
struct Diagnostics {
    requests: AtomicU64,
    native_frames: AtomicU64,
    fallbacks: AtomicU64,
    idle_hints: AtomicU64,
    timeouts: AtomicU64,
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
    fn snapshot(&self) -> Value {
        let value = |counter: &AtomicU64| counter.load(Ordering::Relaxed);
        let last_error = *self.last_error.lock().unwrap_or_else(|p| p.into_inner());
        json!({"requests":value(&self.requests),"nativeFrames":value(&self.native_frames),
            "fallbacks":value(&self.fallbacks),"idleHints":value(&self.idle_hints),
            "timeouts":value(&self.timeouts),"callbacks":value(&self.callbacks),
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

#[derive(Clone, PartialEq)]
struct Key {
    epoch: InputEpoch,
    width: usize,
    height: usize,
    scale: f64,
}
struct Pending {
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
    failure: Mutex<Option<(Code, String)>>,
    diagnostics: Diagnostics,
}
impl Slot {
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
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.cancel();
    }
    fn failure(&self) -> Option<Error> {
        self.failure
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .map(|(code, message)| Error::new(*code, message))
    }
    fn fail(&self, error: Error) {
        *self.failure.lock().unwrap_or_else(|p| p.into_inner()) =
            Some((error.code, error.error.clone()));
        self.stopping.store(true, Ordering::Release);
        self.generation.fetch_add(1, Ordering::AcqRel);
        if let Some(request) = self
            .pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
        {
            request.alive.store(false, Ordering::Release);
            let _ = request.reply.try_send(Err(error));
        }
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
        let mut slot = self.pending.lock().unwrap_or_else(|p| p.into_inner());
        if slot.as_ref().is_some_and(|request| request.id == id) {
            let request = slot.take().expect("matched request");
            if request.alive.load(Ordering::Acquire) {
                let _ = request.reply.try_send(result);
            }
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

/// True means pixels came from a new SCK sample. False reports an explicitly
/// revalidated synchronous CG fallback; no idle buffer receives a fresh key.
///
/// # Errors
///
/// Returns an error when the session or display changed, consent is missing,
/// the login is not graphical, the source is draining or failed, or neither
/// the native sample nor the guarded fallback could be captured.
pub fn stream_display_rgb(display: u32, limit: Option<u32>) -> Result<(RawShot, bool)> {
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
        id: SEQUENCE.fetch_add(1, Ordering::Relaxed),
        checkpoint: tick,
        key,
        alive: alive.clone(),
        reply: send,
    };
    let sent = !worker.slot.stopping.load(Ordering::Acquire)
        && worker.messages.try_send(Message::Capture(request)).is_ok();
    let waiting = Instant::now();
    let outcome = sent
        .then(|| receive.recv_timeout(WAIT))
        .and_then(std::result::Result::ok);
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
            let shot = super::main_display_rgb(display, limit)?;
            guard(&epoch)?;
            let reason = if !sent {
                "queue-full"
            } else if outcome.as_ref().is_some_and(|result| {
                result.as_ref().err().is_some_and(|error| {
                    error
                        .details
                        .get("nativeFallback")
                        .is_some_and(|value| value == "idle")
                })
            }) {
                "idle"
            } else if outcome.is_some() {
                "native-error"
            } else if worker.slot.preparing.load(Ordering::Acquire) {
                "starting"
            } else {
                "timeout"
            };
            *worker
                .slot
                .diagnostics
                .last_fallback
                .lock()
                .unwrap_or_else(|p| p.into_inner()) = Some(reason);
            worker
                .slot
                .diagnostics
                .fallbacks
                .fetch_add(1, Ordering::Relaxed);
            Ok((shot, false))
        }
    }
}
fn fallback_allowed(error: &Error) -> bool {
    error.code == Code::Capture
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
                    slot.generation.fetch_add(1, Ordering::AcqRel);
                    if !stop_native(&slot, true).is_ok_and(|done| done) {
                        slot.stopping.store(true, Ordering::Release);
                        let _ = request
                            .reply
                            .try_send(Err(capture_error("The previous native stream is draining")));
                        continue;
                    }
                    preparing = Some(request.key.clone());
                    *slot.failure.lock().unwrap_or_else(|p| p.into_inner()) = None;
                    prepare(request.key.clone(), slot.clone(), run_loop.clone());
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

struct Callback {
    key: Key,
    slot: Arc<Slot>,
    generation: u64,
    active: AtomicBool,
}
impl Callback {
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
        self.slot.fail(error);
    }

    fn sample(&self, sample: &CMSampleBuffer) {
        self.slot
            .diagnostics
            .callbacks
            .fetch_add(1, Ordering::Relaxed);
        if !self.active.load(Ordering::Acquire)
            || self.slot.generation.load(Ordering::Acquire) != self.generation
        {
            return;
        }
        let pending = self.slot.pending.lock().unwrap_or_else(|p| p.into_inner());
        let Some(request) = pending
            .as_ref()
            .filter(|r| r.key == self.key && r.alive.load(Ordering::Acquire))
        else {
            self.slot
                .diagnostics
                .no_request
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
        let (id, after) = (request.id, request.checkpoint);
        drop(pending);
        let result = (|| {
            let Some(metadata) = metadata(sample)? else {
                self.slot
                    .diagnostics
                    .missing_metadata
                    .fetch_add(1, Ordering::Relaxed);
                return Ok(None);
            };
            match eligibility(metadata, after, checkpoint()) {
                Eligibility::Idle => {
                    self.slot
                        .diagnostics
                        .idle_hints
                        .fetch_add(1, Ordering::Relaxed);
                    self.slot.finish(
                        id,
                        Err(capture_error(
                            "The native display is idle; capture a fresh synchronous frame",
                        )
                        .with("nativeFallback", "idle".into())),
                    );
                    return Ok(None);
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
                Eligibility::Frame => {}
            }
            // Only now consider image data. Rejected metadata never bypasses
            // a pixel guard; both fresh epoch/consent checks still bracket copy.
            let before = Instant::now();
            let guarded = guard(&self.key.epoch);
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
            let rgb = pixels(sample, self.key.width, self.key.height);
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
            let guarded = guard(&self.key.epoch);
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
            if !self.active.load(Ordering::Acquire)
                || self.slot.generation.load(Ordering::Acquire) != self.generation
            {
                return Err(cancelled());
            }
            Ok(Some(RawShot {
                rgb,
                width: self.key.width as u32,
                height: self.key.height as u32,
                scale: self.key.scale,
            }))
        })();
        match result {
            Ok(Some(shot)) => self.slot.finish(id, Ok(shot)),
            Ok(None) => {}
            Err(error) => {
                self.slot.diagnostics.error("metadata", &error);
                self.active.store(false, Ordering::Release);
                self.slot.fail(error);
            }
        }
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
        let state = Arc::new(Callback {
            key: key.clone(),
            slot,
            generation,
            active: AtomicBool::new(true),
        });
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
            configuration.setMinimumFrameInterval(CMTime::new(1, 60));
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
                    started_state.slot.fail(error);
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
                    Ok(source) => RUNNING.with(|running| *running.borrow_mut() = Some(source)),
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

fn accepts(status: i64, time: u64, checkpoint: u64, now: u64) -> bool {
    checkpoint > 0 && status == SCFrameStatus::Complete.0 as i64 && time > checkpoint && time <= now
}

#[derive(Debug, PartialEq)]
struct Metadata {
    status: i64,
    time: Option<u64>,
}
#[derive(Debug, PartialEq)]
enum Eligibility {
    Frame,
    Idle,
    Old,
    Future,
    Incomplete,
    OtherStatus,
}
fn eligibility(metadata: Metadata, checkpoint: u64, now: u64) -> Eligibility {
    if metadata.status == SCFrameStatus::Idle.0 as i64 {
        return Eligibility::Idle;
    }
    if metadata.status != SCFrameStatus::Complete.0 as i64 {
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
    if time > now {
        return Eligibility::Future;
    }
    if accepts(metadata.status, time, checkpoint, now) {
        Eligibility::Frame
    } else {
        Eligibility::Incomplete
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

fn pixels(sample: &CMSampleBuffer, width: usize, height: usize) -> Result<Buffer> {
    // SAFETY: SCK's screen sample contains a retained CoreVideo image buffer.
    let buffer = unsafe { sample.image_buffer() }
        .ok_or_else(|| capture_error("Native frame has no pixels"))?;
    if CVPixelBufferGetPixelFormatType(&buffer) != kCVPixelFormatType_32BGRA
        || CVPixelBufferIsPlanar(&buffer)
        || CVPixelBufferGetWidth(&buffer) != width
        || CVPixelBufferGetHeight(&buffer) != height
    {
        return Err(capture_error("Unexpected native pixel layout"));
    }
    // SAFETY: lock and unlock use the same read-only flag; the retained buffer
    // outlives the lock and all reads. No mutable access is exposed.
    if unsafe { CVPixelBufferLockBaseAddress(&buffer, CVPixelBufferLockFlags::ReadOnly) } != 0 {
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
    let _unlock = Unlock(&buffer);
    let stride = CVPixelBufferGetBytesPerRow(&buffer);
    let length = stride
        .checked_mul(height)
        .filter(|length| {
            *length <= isize::MAX as usize && *length <= CVPixelBufferGetDataSize(&buffer)
        })
        .ok_or_else(|| capture_error("Native pixels exceed their allocation"))?;
    let data = CVPixelBufferGetBaseAddress(&buffer).cast::<u8>();
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
