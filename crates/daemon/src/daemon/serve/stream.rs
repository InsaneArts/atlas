//! Readiness-driven RGB frames. There is no frame queue: a capture starts
//! only after the controller has checked its independent session proof and
//! sent READY. The next capture cannot start until delivery has completed.
use super::Daemon;
use atlas_core::{
    error::{Code, Error, Result},
    protocol::{CaptureBackend, SessionPin, refused},
    session::InputEpoch,
};
use atlas_native::native::capture::{
    self, RawShot,
    source::{Drain, Owner, Source},
};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::unix::{OwnedReadHalf, OwnedWriteHalf},
};

/// A stalled terminal must not hold raw pixels waiting for later playback.
const WRITE: Duration = Duration::from_millis(200);
/// How long a connection may go without asking for a frame.
const IDLE: Duration = Duration::from_secs(10);
/// The shortest gap between CoreGraphics capture starts.
const CADENCE: Duration = Duration::from_nanos(1_000_000_000 / 30);
/// The shortest gap between ScreenCaptureKit capture starts.
const NATIVE_CADENCE: Duration = Duration::from_nanos(1_000_000_000 / 30);
const MAX_HEADER: usize = 4096;

/// The connection's ownership of the live source. Dropping it gives the
/// source back: at once for CoreGraphics, and on a blocking thread for a
/// native source, whose admission stays held until its stop is confirmed
/// and which stays quarantined when it is not.
struct Producer {
    daemon: Arc<Daemon>,
    owner: Option<Owner>,
}

impl Producer {
    fn generation(&self) -> u64 {
        self.owner.as_ref().map_or(0, Owner::generation)
    }

    /// Give the source back now, as `stop_request` asked: Ok only once its
    /// native source is stopped and drained.
    async fn stop(mut self, stop_request: u64) -> Result<()> {
        let Some(owner) = self.owner.take() else {
            return Ok(());
        };
        let daemon = self.daemon.clone();
        tokio::task::spawn_blocking(move || daemon.source.stop(owner, Some(stop_request)))
            .await
            .map_err(|_| {
                Error::new(Code::Capture, "The live capture stop did not finish").uncertain()
            })?
    }
}

impl Drop for Producer {
    fn drop(&mut self) {
        let Some(owner) = self.owner.take() else {
            return;
        };
        let native = owner.backend() == CaptureBackend::ScreenCaptureKit;
        let daemon = self.daemon.clone();
        // A failed stop is kept as the source's quarantine, not lost here.
        let stop = move || {
            let _ = daemon.source.stop(owner, None);
        };
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) if native => drop(runtime.spawn_blocking(stop)),
            _ => stop(),
        }
    }
}

/// What the source knows of `generation` of daemon `daemon_pid`, for a
/// controller settling a stream whose stop it never heard confirmed. A stop
/// request is echoed only when this source recorded that very request.
pub(super) fn drain_status(
    source: &Source,
    daemon_pid: u32,
    generation: u64,
    query_id: &str,
    stop_request: Option<u64>,
) -> Value {
    let drain = if daemon_pid == std::process::id() {
        source.drain(generation)
    } else {
        Drain::Unknown
    };
    let (state, drained, recorded) = match drain {
        Drain::Drained { stop_request } => (json!("stopped"), true, stop_request),
        Drain::Held(lifecycle) => (json!(lifecycle), false, None),
        Drain::Unknown => (json!("unknown"), false, None),
    };
    let mut reply = json!({"ok": true, "daemonPID": daemon_pid, "producerGeneration": generation,
        "queryID": query_id, "state": state, "drained": drained});
    if let Some(recorded) = recorded.filter(|recorded| Some(*recorded) == stop_request) {
        reply["stopRequestID"] = json!(recorded);
    }
    reply
}

fn stopped() -> Error {
    Error::new(Code::Transport, "The live frame reader stopped")
}

/// Header length, JSON header, then exactly width*height*3 bytes for a
/// successful frame; failures have no pixel body. No image codec or base64.
pub(super) async fn write_header(out: &mut (impl AsyncWrite + Unpin), value: &Value) -> Result<()> {
    let header = serde_json::to_vec(value)?;
    if header.len() > MAX_HEADER {
        return Err(Error::new(
            Code::Capture,
            "The live frame header is too large",
        ));
    }
    tokio::time::timeout(WRITE, async {
        out.write_all(&(header.len() as u32).to_be_bytes()).await?;
        out.write_all(&header).await
    })
    .await
    .map_err(|_| stopped())??;
    Ok(())
}

/// Every check reads actual session facts. A changed epoch is never silently
/// adopted by an existing connection, including a display geometry change.
fn guard(daemon: &Daemon, expected: &InputEpoch) -> Result<Value> {
    if (daemon.screen.on_console)() {
        return Err(Error::new(Code::OnConsole, "The helper is on the screen"));
    }
    let proof = (daemon.screen.session)()?;
    let current = InputEpoch::from_proof(&proof, std::process::id())?;
    if current != *expected {
        return Err(Error::new(
            Code::SessionChanged,
            "The helper changed during live capture",
        ));
    }
    Ok(proof)
}

/// One frame, asked for at `asked`: a stream's wait for its picture is
/// counted from then, so a frame held back meanwhile still answers in time.
fn capture(
    daemon: &Daemon,
    expected: &InputEpoch,
    side: u32,
    backend: CaptureBackend,
    asked: std::time::Instant,
) -> Result<(RawShot, Value, &'static str, f64)> {
    guard(daemon, expected)?;
    let capturing = std::time::Instant::now();
    let (shot, source) = match backend {
        CaptureBackend::CoreGraphics => (
            (daemon.screen.raw_capture)(expected.display_id as u32, Some(side))?,
            "cg",
        ),
        CaptureBackend::ScreenCaptureKit => {
            let (shot, streamed) =
                capture::stream_display_rgb(expected.display_id as u32, Some(side), asked)?;
            (shot, if streamed { "sck" } else { "cg-fallback" })
        }
    };
    let capture_ms = capturing.elapsed().as_secs_f64() * 1000.0;
    let proof = guard(daemon, expected)?;
    let length = u64::from(shot.width) * u64::from(shot.height) * 3;
    if shot.width == 0
        || shot.height == 0
        || shot.width > side
        || shot.height > side
        || length != shot.rgb.len() as u64
        || !shot.scale.is_finite()
        || !(0.0..=8.0).contains(&shot.scale)
        || shot.scale == 0.0
    {
        return Err(Error::new(
            Code::Capture,
            "The live frame layout is invalid",
        ));
    }
    Ok((shot, proof, source, capture_ms))
}

pub(super) async fn serve(
    daemon: Arc<Daemon>,
    mut reader: OwnedReadHalf,
    mut writer: OwnedWriteHalf,
    side: u32,
    pin: SessionPin,
    backend: CaptureBackend,
) -> Result<()> {
    // Admission may first confirm an earlier native stop, which blocks. The
    // guard is made there, so an abandoned admission still gives the source
    // back.
    let admitting = daemon.clone();
    let admitted = tokio::task::spawn_blocking(move || {
        admitting.source.admit(backend).map(|owner| Producer {
            daemon: admitting.clone(),
            owner: Some(owner),
        })
    });
    // Each capture worker holds a share too: the source is given back only
    // once no capture of this connection can still be running.
    let producer = match admitted.await {
        Ok(Ok(producer)) => Arc::new(producer),
        Ok(Err(error)) => return write_header(&mut writer, &refused(error)).await,
        Err(_) => {
            let error = Error::new(Code::Capture, "The live capture could not start");
            return write_header(&mut writer, &refused(error)).await;
        }
    };
    let initial = daemon.screen.epoch();
    let expected = match initial {
        Ok(epoch)
            if epoch.security_session_id == pin.security_session_id
                && epoch.display_id == pin.display_id =>
        {
            epoch
        }
        Ok(_) => {
            return write_header(
                &mut writer,
                &refused(Error::new(
                    Code::SessionChanged,
                    "The helper changed before live capture",
                )),
            )
            .await;
        }
        Err(error) => return write_header(&mut writer, &refused(error)).await,
    };
    // The controller waits for this before READY. Its request line and
    // readiness byte therefore cannot be coalesced and consumed together
    // by the protocol's single-line reader.
    write_header(
        &mut writer,
        &json!({"ok": true, "stream": "rgb-stream-v1", "daemonPID": std::process::id(),
            "producerGeneration": producer.generation()}),
    )
    .await?;
    let mut sequence = 0_u64;
    let mut next = tokio::time::Instant::now();
    loop {
        let mut ready = [0_u8; 1];
        tokio::select! {
            () = daemon.stopped() => return Ok(()),
            read = tokio::time::timeout(IDLE, reader.read_exact(&mut ready)) => {
                if !matches!(read, Ok(Ok(_))) { return Ok(()); }
            }
        }
        match ready {
            [1] => {}
            // A stop request: its 64-bit ID follows, and the reply comes
            // only once the source is stopped and drained.
            [0] => {
                let mut id = [0_u8; 8];
                let read = tokio::time::timeout(WRITE, reader.read_exact(&mut id)).await;
                if !matches!(read, Ok(Ok(_))) {
                    return Ok(());
                }
                let stop_request = u64::from_be_bytes(id);
                let generation = producer.generation();
                // No capture is in flight between frames, so this is the
                // connection's only share.
                let stopped = match Arc::try_unwrap(producer) {
                    Ok(producer) => producer.stop(stop_request).await,
                    Err(_) => Err(Error::new(
                        Code::Capture,
                        "The live capture is still in use",
                    )),
                };
                let reply = match stopped {
                    Ok(()) => json!({"ok": true, "daemonPID": std::process::id(),
                        "producerGeneration": generation, "stopRequestID": stop_request,
                        "drained": true}),
                    Err(error) => refused(error),
                };
                return write_header(&mut writer, &reply).await;
            }
            _ => {
                return write_header(
                    &mut writer,
                    &refused(Error::new(Code::BadRequest, "A live frame requires READY")),
                )
                .await;
            }
        }
        tokio::select! {
            () = daemon.stopped() => return Ok(()),
            _ = tokio::time::sleep_until(next) => {}
        }
        let capture_started = tokio::time::Instant::now();
        let worker_daemon = daemon.clone();
        let worker_expected = expected.clone();
        let held = producer.clone();
        let asked = std::time::Instant::now();
        // The frame holds the source's fence, so nothing that must run
        // between captures starts under it.
        let mut worker = tokio::task::spawn_blocking(move || {
            let _held = held;
            worker_daemon
                .source
                .frame(|| capture(&worker_daemon, &worker_expected, side, backend, asked))
                .and_then(std::convert::identity)
        });
        // A blocking framework capture cannot be interrupted safely. On
        // departure or shutdown its result is discarded, and its task is
        // joined while still holding the source: no orphan producer.
        let mut extra = [0_u8; 1];
        let captured = tokio::select! {
            () = daemon.stopped() => { let _ = worker.await; return Ok(()); }
            _ = reader.read(&mut extra) => { let _ = worker.await; return Ok(()); }
            result = &mut worker => result.map_err(|_| Error::new(Code::Capture, "The live capture worker stopped"))?,
        };
        let (shot, _, source, capture_ms) = match captured {
            Ok(captured) => captured,
            Err(error) => return write_header(&mut writer, &refused(error)).await,
        };
        let proof = match guard(&daemon, &expected) {
            Ok(proof) => proof,
            Err(error) => return write_header(&mut writer, &refused(error)).await,
        };
        sequence = sequence
            .checked_add(1)
            .ok_or_else(|| Error::new(Code::Capture, "Live frame sequence exhausted"))?;
        let mut header = json!({"ok":true,"sequence":sequence,"width":shot.width,"height":shot.height,"scale":shot.scale,"session":proof,"daemonPID":std::process::id(),"captureBackend":source,"captureMs":capture_ms});
        if backend == CaptureBackend::ScreenCaptureKit
            && let Some(stats) = capture::stream_diagnostics()
        {
            header["captureStats"] = stats;
        }
        // One deadline covers both header and pixels. The next READY is
        // required before the producer creates another frame.
        let delivered = tokio::time::timeout(WRITE, async {
            write_header(&mut writer, &header).await?;
            writer.write_all(&shot.rgb).await?;
            writer.flush().await?;
            Ok::<_, Error>(())
        });
        tokio::select! {
            () = daemon.stopped() => return Ok(()),
            delivery = delivered => { delivery.map_err(|_| stopped())??; }
        }
        next = capture_started
            + if backend == CaptureBackend::ScreenCaptureKit {
                NATIVE_CADENCE
            } else {
                CADENCE
            };
    }
}

#[cfg(test)]
mod tests;
