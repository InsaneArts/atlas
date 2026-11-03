//! Readiness-driven RGB frames. There is no frame queue: a capture starts
//! only after the controller has checked its independent session proof and
//! sent READY. The next capture cannot start until delivery has completed.
use super::Daemon;
use atlas_core::{
    error::{Code, Error, Result},
    protocol::{CaptureBackend, SessionPin, refused},
    session::InputEpoch,
};
use atlas_native::native::capture::{self, RawShot};
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
const NATIVE_CADENCE: Duration = Duration::from_nanos(1_000_000_000 / 60);
const MAX_HEADER: usize = 4096;

/// Stops the native source when the connection that owns it ends. A stop
/// that is not confirmed leaves the source quarantined in its worker.
struct StreamStop(CaptureBackend);

impl Drop for StreamStop {
    fn drop(&mut self) {
        if self.0 == CaptureBackend::ScreenCaptureKit {
            let _ = capture::stop_display_stream();
        }
    }
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

fn capture(
    daemon: &Daemon,
    expected: &InputEpoch,
    side: u32,
    backend: CaptureBackend,
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
                capture::stream_display_rgb(expected.display_id as u32, Some(side))?;
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
    let Ok(_slot) = daemon.stream_slot.try_acquire() else {
        return write_header(
            &mut writer,
            &refused(Error::new(Code::Busy, "A live capture is already open")),
        )
        .await;
    };
    // Drop order keeps the producer slot excluded through native cancellation
    // and callback drain, including client departure and capture errors.
    let _stream_stop = StreamStop(backend);
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
    write_header(&mut writer, &json!({"ok":true,"stream":"rgb-stream-v1"})).await?;
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
        if ready != [1] {
            return write_header(
                &mut writer,
                &refused(Error::new(Code::BadRequest, "A live frame requires READY")),
            )
            .await;
        }
        tokio::select! {
            () = daemon.stopped() => return Ok(()),
            _ = tokio::time::sleep_until(next) => {}
        }
        let capture_started = tokio::time::Instant::now();
        let worker_daemon = daemon.clone();
        let worker_expected = expected.clone();
        let mut worker = tokio::task::spawn_blocking(move || {
            capture(&worker_daemon, &worker_expected, side, backend)
        });
        // A blocking framework capture cannot be interrupted safely. On
        // departure or shutdown its result is discarded, and its task is
        // joined while still holding the stream slot: no orphan producer.
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
