//! Persistent raw preview reads. Every readiness byte follows an independent
//! session proof; no picture captured before that checkpoint can be accepted.
use super::Client;
use atlas_core::{
    buffer::Buffer,
    error::{Code, Error, Result},
    frame_pipe::{
        budget::Budget,
        diagnostics::{Backend, Outcome, Reason, Record, Ring, Stage, Timing, nanos},
        pixels::FramePixels,
    },
    protocol::{
        CaptureBackend, RGB_STREAM, RGB_STREAM_HIGH_RES, RGB_STREAM_STOP, SCK_EXPERIMENTAL,
        SESSION_PROOF, SessionPin,
    },
    session::{Epoch, InputEpoch},
    transport::{connect_socket, refusal},
};
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

const HEADER_LIMIT: usize = 4096;
/// How long setup, or one complete frame read, may take.
const DEADLINE: Duration = Duration::from_secs(2);
/// How old a frame may be, from its readiness checkpoint, when it is accepted.
const FRESHNESS: Duration = Duration::from_millis(500);
/// How often a stop that went unanswered is asked about while it settles.
const DRAIN_QUERY: Duration = Duration::from_millis(50);

/// One verified picture of the helper and how it was read, its pixels in
/// `P`: a shared buffer, or pixels charged to a budget.
pub struct LiveFrame<P = Arc<Buffer>> {
    pub rgb: P,
    pub width: u32,
    pub height: u32,
    pub epoch: Epoch,
    pub at: Instant,
    pub metrics: Metrics,
}

/// What the helper's daemon offers this connection.
#[derive(Clone, Copy, Default)]
pub struct Profile {
    pub ceiling: u32,
    pub persistent_proofs: bool,
}

/// Where one frame's time went, for diagnostics only.
#[derive(Clone, Copy, Default)]
pub struct Metrics {
    pub profile: Profile,
    pub before_ms: f64,
    pub capture_wait_ms: f64,
    pub transfer_ms: f64,
    pub after_ms: f64,
    pub total_ms: f64,
    pub backend: &'static str,
    pub native_size: (u32, u32),
    pub native_capture_ms: Option<f64>,
    pub capture_stats: Option<CaptureStats>,
}

/// Optional telemetry, never used as a session proof or frame authority.
#[derive(Clone, Copy, Default)]
pub struct CaptureStats {
    pub native_frames: u64,
    pub fallbacks: u64,
    pub timeouts: u64,
    pub idle_hints: u64,
    pub last_fallback: &'static str,
    pub native_reason: &'static str,
    pub wait_ms: f64,
    pub guard_ms: f64,
    pub copy_ms: f64,
}

impl CaptureStats {
    fn parse(value: &Value) -> Option<Self> {
        value.as_object()?;
        let millis = |key: &str| {
            value[key]
                .as_u64()
                .map(|v| v as f64 / 1000.0)
                .unwrap_or(0.0)
        };
        Some(Self {
            native_frames: value["nativeFrames"].as_u64().unwrap_or(0),
            fallbacks: value["fallbacks"].as_u64().unwrap_or(0),
            timeouts: value["timeouts"].as_u64().unwrap_or(0),
            idle_hints: value["idleHints"].as_u64().unwrap_or(0),
            // Only local literals can enter a footer, never remote text or
            // escape sequences supplied by optional diagnostic metadata.
            last_fallback: match value["lastFallback"].as_str() {
                Some("none") => "none",
                Some("idle") => "idle",
                Some("timeout") => "timeout",
                Some("starting") => "starting",
                Some("queue-full") => "queue-full",
                Some("native-error") => "native-error",
                _ => "unknown",
            },
            native_reason: match value["nativeReason"].as_str() {
                Some("none") => "none",
                Some("no-exact-display") => "no-exact-display",
                Some("shareable-content") => "shareable-content",
                Some("start-error") => "start-error",
                Some("format-layout") => "format-layout",
                Some("metadata") => "metadata",
                Some("guard") => "guard",
                Some("stop-error") => "stop-error",
                _ => "unknown",
            },
            wait_ms: millis("waitUs"),
            guard_ms: millis("guardBeforeUs") + millis("guardAfterUs"),
            copy_ms: millis("copyUs"),
        })
    }
}

/// One persistent raw frame connection to the verified helper daemon.
pub struct LiveStream {
    socket: UnixStream,
    epoch: Epoch,
    sequence: u64,
    limit: u32,
    synchronized: bool,
    profile: Profile,
    backend: CaptureBackend,
    /// The daemon and source generation serving this connection, when the
    /// daemon confirms stops.
    producer: Option<Served>,
    /// How many acquisitions this connection began.
    acquisitions: u64,
}

impl LiveStream {
    /// A raw frame connection, or none for an older helper, which keeps the
    /// PNG path. Failure after advertising this capability is an error,
    /// never a silent downgrade across epochs.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid size, an unverified or changed helper,
    /// a helper without the requested backend, a peer that is not its daemon,
    /// a refused handshake, or setup past its deadline.
    pub async fn connect(
        client: &Client,
        limit: u32,
        backend: CaptureBackend,
    ) -> Result<Option<Self>> {
        let setup = tokio::time::Instant::now() + DEADLINE;
        tokio::time::timeout_at(setup, Self::open(client, limit, backend, setup))
            .await
            .map_err(|_| invalid("Preview setup timed out"))?
    }

    /// Open the stream; a denied connection is tried again only while it fits
    /// before `setup`, the whole setup's deadline.
    async fn open(
        client: &Client,
        limit: u32,
        backend: CaptureBackend,
        setup: tokio::time::Instant,
    ) -> Result<Option<Self>> {
        if !(320..=4096).contains(&limit) {
            return Err(invalid("Invalid preview size"));
        }
        let verified = client.verify().await?;
        let offers = |name: &str| verified.observation.has_capability(name);
        if backend == CaptureBackend::ScreenCaptureKit && !offers(SCK_EXPERIMENTAL) {
            return Err(Error::new(
                Code::Usage,
                "ScreenCaptureKit requires the new signed helper with experimental stream support",
            ));
        }
        if !offers(RGB_STREAM) {
            return Ok(None);
        }
        // A native source is replaced only behind a confirmed stop.
        if backend == CaptureBackend::ScreenCaptureKit && !offers(RGB_STREAM_STOP) {
            return Err(Error::new(
                Code::Usage,
                "ScreenCaptureKit requires a helper that confirms its capture stopped; update it",
            ));
        }
        let high_res = offers(RGB_STREAM_HIGH_RES);
        let limit = if high_res { limit } else { limit.min(1600) };
        let epoch = verified.session.epoch.clone();
        let socket = tokio::time::timeout(DEADLINE, connect_socket(&client.transport.path, setup))
            .await
            .map_err(|_| invalid("Preview connection timed out"))??;
        let peer = socket.peer_cred()?;
        if peer.uid() != client.transport.peer_uid
            || peer.pid().and_then(|pid| u32::try_from(pid).ok()) != Some(epoch.capture_pid)
        {
            return Err(Error::new(
                Code::Identity,
                "The preview peer is not the verified helper daemon",
            ));
        }
        let mut stream = Self {
            socket,
            epoch,
            sequence: 0,
            limit,
            synchronized: true,
            backend,
            profile: Profile {
                ceiling: if high_res { 4096 } else { 1600 },
                persistent_proofs: offers(SESSION_PROOF),
            },
            producer: None,
            acquisitions: 0,
        };
        let mut request = serde_json::to_vec(&request(&stream.epoch, limit, backend))?;
        request.push(b'\n');
        tokio::time::timeout(DEADLINE, stream.socket.write_all(&request))
            .await
            .map_err(|_| invalid("Preview request timed out"))??;
        // The line protocol discards trailing bytes. Wait until the daemon
        // has entered binary mode before any READY can share its request read.
        let accepted = tokio::time::timeout(DEADLINE, read_header(&mut stream.socket))
            .await
            .map_err(|_| invalid("Preview handshake timed out"))??;
        if accepted["stream"] != RGB_STREAM {
            return Err(invalid(
                "The helper did not accept the raw preview protocol",
            ));
        }
        if offers(RGB_STREAM_STOP) {
            let daemon = accepted["daemonPID"]
                .as_u64()
                .and_then(|pid| u32::try_from(pid).ok())
                .filter(|pid| *pid == stream.epoch.capture_pid);
            let generation = accepted["producerGeneration"]
                .as_u64()
                .filter(|generation| *generation > 0);
            let (daemon, generation) = daemon
                .zip(generation)
                .ok_or_else(|| invalid("The helper did not name its live capture source"))?;
            stream.producer = Some(Served {
                daemon,
                generation,
                instance: i32::try_from(daemon)
                    .ok()
                    .and_then(|pid| atlas_core::process::info(pid).ok().flatten())
                    .map(|info| info.instance),
            });
        }
        Ok(Some(stream))
    }

    /// The next frame, bracketed by fresh session proofs. Its outcome,
    /// failed or not, goes into `ring` before it is returned, with the time
    /// each stage took, the failed one included; a failure carries the stage
    /// it ended at and why.
    ///
    /// # Errors
    ///
    /// Returns an error when either proof fails or disagrees, the header or
    /// pixels are invalid, the read passes its deadline, or the complete frame
    /// is older than its freshness limit.
    pub async fn next(&mut self, client: &Client, ring: &mut Ring) -> Result<LiveFrame> {
        let frame = self
            .next_into(client, ring, |_, _, length| Ok(Buffer::zeroed(length)?))
            .await?;
        Ok(LiveFrame {
            rgb: Arc::new(frame.rgb),
            width: frame.width,
            height: frame.height,
            epoch: frame.epoch,
            at: frame.at,
            metrics: frame.metrics,
        })
    }

    /// [`next`](Self::next), its pixels reserved from `budget` before any is
    /// allocated or read.
    ///
    /// # Errors
    ///
    /// As [`next`](Self::next), and `Busy` when the budget cannot hold the
    /// picture, recorded at the allocation stage as an exhausted budget.
    pub async fn next_reserved(
        &mut self,
        client: &Client,
        ring: &mut Ring,
        budget: &Budget,
    ) -> Result<LiveFrame<FramePixels>> {
        self.next_into(client, ring, |width, height, _| {
            FramePixels::reserve(budget, width, height).map_err(|reason| {
                Error::new(Code::Busy, "The frame budget cannot hold this picture")
                    .with("reason", json!(reason))
            })
        })
        .await
    }

    async fn next_into<P: Pixels>(
        &mut self,
        client: &Client,
        ring: &mut Ring,
        allocate: impl FnOnce(u32, u32, usize) -> Result<P>,
    ) -> Result<LiveFrame<P>> {
        self.synchronized = false;
        self.acquisitions += 1;
        let mut progress = Progress::new(Record::new(self.acquisitions));
        progress.record.source_generation = self.producer.map_or(0, |served| served.generation);
        progress.record.requested_backend = Backend::named(self.backend.name());
        progress.record.pipe_reused = self.acquisitions > 1;
        let began = Instant::now();
        let result = tokio::time::timeout(DEADLINE, self.capture(client, &mut progress, allocate))
            .await
            .unwrap_or_else(|_| {
                progress.record.outcome = Outcome::Timeout;
                Err(invalid("Preview verification timed out"))
            });
        let mut record = progress.finish();
        record.time(Timing::Total, began.elapsed());
        record.pipe_synchronized = self.synchronized;
        record.source_sequence = self.sequence;
        let result = match result {
            Ok(frame) => {
                record.age_at_decision_ns = nanos(frame.at.elapsed());
                record.end(Outcome::Accepted, Stage::None, Reason::None);
                Ok(frame)
            }
            Err(error) => {
                let outcome = match (record.outcome, error.code) {
                    (Outcome::Timeout, _) => Outcome::Timeout,
                    (_, Code::Cancelled) => Outcome::Cancelled,
                    _ => Outcome::Rejected,
                };
                let reason = match (record.stage, error.code) {
                    (Stage::SourceAge, _) => Reason::SourceStale,
                    (Stage::Allocation, Code::Busy) => Reason::BudgetExhausted,
                    (Stage::BodyRead, Code::Io) => Reason::Truncated,
                    (Stage::ReadyHeader, Code::Io) => Reason::TransportEnded,
                    (Stage::ReadyHeader, Code::InvalidJson) => Reason::InvalidSchema,
                    (_, code) => Reason::of(code),
                };
                record.end(outcome, record.stage, reason);
                Err(error
                    .with("stage", json!(record.stage))
                    .with("reason", json!(record.reason)))
            }
        };
        ring.push(record);
        result
    }

    /// Give the helper's live source back and wait, two seconds at most,
    /// until the daemon that served it says it is stopped and drained: by a
    /// stop request at a frame boundary, else by asking that daemon about the
    /// source's generation once this end of the connection is closed. A
    /// stream without a stop-confirming helper is a CoreGraphics one, given
    /// back when the connection closes. Asking again after a failure only
    /// asks again; no second stop is sent.
    ///
    /// # Errors
    ///
    /// Returns an error when the daemon does not confirm the source stopped
    /// and drained in time; its source then stays quarantined there,
    /// refusing every capture until it is confirmed.
    pub async fn stop(&mut self, client: &Client) -> Result<()> {
        let until = tokio::time::Instant::now() + DEADLINE;
        let Some(Served {
            daemon,
            generation,
            instance,
        }) = self.producer
        else {
            // A CoreGraphics stream's source is given back as it closes.
            let _ = tokio::time::timeout_at(until, self.socket.shutdown()).await;
            return Ok(());
        };
        if self.synchronized {
            self.synchronized = false;
            let request = stop_request_id();
            let mut stop = vec![0];
            stop.extend_from_slice(&request.to_be_bytes());
            let answered = tokio::time::timeout_at(until, async {
                self.socket.write_all(&stop).await?;
                read_header(&mut self.socket).await
            })
            .await;
            if let Ok(Ok(reply)) = answered
                && reply["ok"] == true
                && reply["drained"] == true
                && reply["daemonPID"] == daemon
                && reply["producerGeneration"] == generation
                && reply["stopRequestID"] == request
            {
                return Ok(());
            }
        }
        // No confirmation came: closing hands the source back, and only a
        // fresh query of that generation, answered by that daemon, can say
        // it is drained.
        let _ = tokio::time::timeout_at(until, self.socket.shutdown()).await;
        loop {
            // A daemon that exited took its native source with it.
            if exited(instance) {
                return Ok(());
            }
            let query = atlas_core::files::token()[..32].to_owned();
            let reply = tokio::time::timeout_at(
                until,
                drain_query(client, daemon, generation, &query, until),
            )
            .await;
            match reply {
                Ok(Ok(reply))
                    if reply["ok"] == true
                        && reply["queryID"] == query.as_str()
                        && reply["daemonPID"] == daemon
                        && reply["producerGeneration"] == generation
                        && reply["state"] == "stopped"
                        && reply["drained"] == true =>
                {
                    return Ok(());
                }
                Ok(Ok(reply)) if reply["state"] == "active" || reply["state"] == "stopping" => {}
                _ => break,
            }
            if tokio::time::Instant::now() + DRAIN_QUERY >= until {
                break;
            }
            tokio::time::sleep(DRAIN_QUERY).await;
        }
        Err(Error::new(
            Code::Capture,
            "The helper did not confirm that its live capture stopped",
        ))
    }

    /// Whether `error` left this connection at a frame boundary, so the next
    /// read may use it again.
    pub fn can_retry(&self, error: &Error) -> bool {
        error.code == Code::StaleFrame && self.synchronized
    }

    async fn capture<P: Pixels>(
        &mut self,
        client: &Client,
        progress: &mut Progress,
        allocate: impl FnOnce(u32, u32, usize) -> Result<P>,
    ) -> Result<LiveFrame<P>> {
        let began = Instant::now();
        progress.enter(Stage::BeforeProof);
        let before = client.identity().await;
        progress.done();
        let before = before?.epoch;
        let before_ms = began.elapsed().as_secs_f64() * 1000.0;
        if before != self.epoch {
            return Err(changed());
        }
        let at = Instant::now();
        let read = tokio::time::timeout(DEADLINE, async {
            progress.enter(Stage::ReadyHeader);
            self.socket.write_all(&[1]).await?;
            let header = read_header(&mut self.socket).await?;
            progress.done();
            let capture_wait_ms = at.elapsed().as_secs_f64() * 1000.0;
            let (width, height, sequence) =
                validate_header(&header, &before, self.limit, self.sequence)?;
            let source = match (self.backend, header["captureBackend"].as_str()) {
                (CaptureBackend::CoreGraphics, None | Some("cg")) => "cg",
                (CaptureBackend::ScreenCaptureKit, Some("sck")) => "sck",
                (CaptureBackend::ScreenCaptureKit, Some("cg-fallback")) => "cg-fallback",
                _ => {
                    return Err(invalid(
                        "Capture backend metadata disagrees with the request",
                    ));
                }
            };
            progress.record.actual_backend = Backend::named(source);
            let length = usize::try_from(width)
                .ok()
                .and_then(|w| {
                    usize::try_from(height)
                        .ok()
                        .and_then(|h| w.checked_mul(h)?.checked_mul(3))
                })
                .ok_or_else(|| invalid("Preview dimensions overflow"))?;
            progress.enter(Stage::Allocation);
            let mut rgb = allocate(width, height, length)?;
            progress.done();
            progress.record.allocated_bytes = length as u64;
            progress.enter(Stage::BodyRead);
            let reading = Instant::now();
            self.socket.read_exact(rgb.bytes()).await?;
            progress.done();
            Ok::<_, Error>((
                Received {
                    width,
                    height,
                    sequence,
                    capture_wait_ms,
                    transfer_ms: reading.elapsed().as_secs_f64() * 1000.0,
                    source,
                    native_capture_ms: header["captureMs"]
                        .as_f64()
                        .filter(|v| v.is_finite() && *v >= 0.0),
                    capture_stats: CaptureStats::parse(&header["captureStats"]),
                },
                rgb,
            ))
        })
        .await;
        let (header, rgb) = match read {
            Ok(read) => read?,
            Err(_) => {
                progress.record.outcome = Outcome::Timeout;
                return Err(invalid("Preview frame timed out"));
            }
        };
        progress.enter(Stage::AfterProof);
        let proving = Instant::now();
        let after = client.identity().await;
        progress.done();
        let after = after?.epoch;
        let after_ms = proving.elapsed().as_secs_f64() * 1000.0;
        if after != before {
            return Err(changed());
        }
        // This complete frame consumed its sequence even if it is too late
        // to present. A retry may use the same pipe, never replay this image.
        self.sequence = header.sequence;
        self.synchronized = true;
        progress.enter(Stage::SourceAge);
        progress.record.age_at_decision_ns = nanos(at.elapsed());
        if at.elapsed() >= FRESHNESS {
            return Err(Error::new(
                Code::StaleFrame,
                "Preview became stale while verifying its session",
            ));
        }
        Ok(LiveFrame {
            rgb,
            width: header.width,
            height: header.height,
            epoch: after,
            at,
            metrics: Metrics {
                profile: self.profile,
                before_ms,
                capture_wait_ms: header.capture_wait_ms,
                transfer_ms: header.transfer_ms,
                after_ms,
                total_ms: began.elapsed().as_secs_f64() * 1000.0,
                backend: header.source,
                native_capture_ms: header.native_capture_ms,
                capture_stats: header.capture_stats,
                native_size: (
                    (before.display.width * before.display.scale).round() as u32,
                    (before.display.height * before.display.scale).round() as u32,
                ),
            },
        })
    }
}

/// Where a frame's pixels are read into, allocated once its size is known.
trait Pixels {
    fn bytes(&mut self) -> &mut [u8];
}

impl Pixels for Buffer {
    fn bytes(&mut self) -> &mut [u8] {
        self
    }
}

impl Pixels for FramePixels {
    fn bytes(&mut self) -> &mut [u8] {
        self.as_mut_slice()
    }
}

/// The daemon serving a stream, as it was when the stream opened, and the
/// generation of its live source.
#[derive(Clone, Copy, Debug)]
struct Served {
    daemon: u32,
    generation: u64,
    instance: Option<atlas_core::process::Instance>,
}

/// Whether the daemon in `instance` has exited: its PID is gone, a zombie,
/// or another process's. Unknown is not exited.
fn exited(instance: Option<atlas_core::process::Instance>) -> bool {
    instance.is_some_and(|instance| match atlas_core::process::info(instance.pid) {
        Ok(None) => true,
        Ok(Some(info)) => info.instance.start != instance.start || info.zombie,
        Err(_) => false,
    })
}

/// One acquisition's record as it goes: the stage it is in and when that
/// stage began, so a stage that fails or times out keeps its elapsed time.
struct Progress {
    record: Record,
    since: Instant,
}

impl Progress {
    fn new(record: Record) -> Self {
        Self {
            record,
            since: Instant::now(),
        }
    }

    fn enter(&mut self, stage: Stage) {
        self.record.stage = stage;
        self.since = Instant::now();
    }

    /// Note the time the current stage took, unless already noted.
    fn done(&mut self) {
        let timing = match self.record.stage {
            Stage::BeforeProof => Timing::BeforeProof,
            Stage::ReadyHeader => Timing::ReadyHeader,
            Stage::Allocation => Timing::Allocation,
            Stage::BodyRead => Timing::BodyRead,
            Stage::AfterProof => Timing::AfterProof,
            _ => return,
        };
        if self.record.timing(timing).is_none() {
            self.record.time(timing, self.since.elapsed());
        }
    }

    /// The record, with the stage it ended in timed.
    fn finish(mut self) -> Record {
        self.done();
        self.record
    }
}

/// What one frame's header and body read said.
struct Received {
    width: u32,
    height: u32,
    sequence: u64,
    capture_wait_ms: f64,
    transfer_ms: f64,
    source: &'static str,
    native_capture_ms: Option<f64>,
    capture_stats: Option<CaptureStats>,
}

/// A fresh, unpredictable ID for one stop request.
fn stop_request_id() -> u64 {
    let token = atlas_core::files::token();
    u64::from_str_radix(&token[..16], 16).unwrap_or_default()
}

fn request(epoch: &Epoch, limit: u32, backend: CaptureBackend) -> Value {
    let mut request = json!({"op":"stream", "maxDimension":limit,
        "session":SessionPin { security_session_id:epoch.security_session_id, display_id:epoch.display_id }});
    if backend == CaptureBackend::ScreenCaptureKit {
        request["backend"] = json!(backend.name());
    }
    request
}

/// Ask the daemon `daemon`, and only it, whether `generation` is drained.
async fn drain_query(
    client: &Client,
    daemon: u32,
    generation: u64,
    query: &str,
    until: tokio::time::Instant,
) -> Result<Value> {
    let mut socket = connect_socket(&client.transport.path, until).await?;
    let peer = socket.peer_cred()?;
    if peer.uid() != client.transport.peer_uid
        || peer.pid().and_then(|pid| u32::try_from(pid).ok()) != Some(daemon)
    {
        return Err(Error::new(
            Code::Identity,
            "The drain answer did not come from the helper daemon that ran the stream",
        ));
    }
    let mut line = serde_json::to_vec(&json!({"op": "stream-drain-status", "daemonPID": daemon,
        "producerGeneration": generation, "queryID": query}))?;
    line.push(b'\n');
    socket.write_all(&line).await?;
    let mut reply = Vec::new();
    BufReader::new(socket)
        .take(HEADER_LIMIT as u64)
        .read_until(b'\n', &mut reply)
        .await?;
    Ok(serde_json::from_slice(&reply)?)
}

pub(super) async fn read_header(reader: &mut (impl tokio::io::AsyncRead + Unpin)) -> Result<Value> {
    let length = reader.read_u32().await? as usize;
    if length == 0 || length > HEADER_LIMIT {
        return Err(invalid("Invalid preview header length"));
    }
    let mut header = vec![0; length];
    reader.read_exact(&mut header).await?;
    let header: Value = serde_json::from_slice(&header)?;
    if header["ok"] != true {
        return Err(refusal(&header));
    }
    Ok(header)
}

fn validate_header(
    header: &Value,
    epoch: &Epoch,
    limit: u32,
    previous: u64,
) -> Result<(u32, u32, u64)> {
    let side = |key: &str| {
        header[key]
            .as_u64()
            .filter(|n| *n > 0 && *n <= u64::from(limit))
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| invalid("Invalid preview dimensions"))
    };
    let (width, height) = (side("width")?, side("height")?);
    let sequence = header["sequence"]
        .as_u64()
        .filter(|n| *n > previous)
        .ok_or_else(|| invalid("Preview sequence did not advance"))?;
    let process_id = header["daemonPID"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|pid| *pid == epoch.capture_pid)
        .ok_or_else(changed)?;
    let proof = InputEpoch::from_proof(&header["session"], process_id)?;
    let expected = InputEpoch {
        process_id: epoch.capture_pid,
        ..epoch.input_epoch()
    };
    if proof != expected || header["scale"].as_f64() != Some(epoch.display.scale) {
        return Err(changed());
    }
    let distortion =
        (f64::from(width) * epoch.display.height - f64::from(height) * epoch.display.width).abs();
    if distortion > epoch.display.width.max(epoch.display.height) * 1.5 {
        return Err(invalid(
            "Preview aspect ratio disagrees with the helper display",
        ));
    }
    Ok((width, height, sequence))
}

fn invalid(message: &str) -> Error {
    Error::new(Code::Capture, message)
}

fn changed() -> Error {
    Error::new(
        Code::SessionChanged,
        "The helper session changed during preview; observe again",
    )
}

#[cfg(test)]
#[path = "live_tests.rs"]
mod tests;
