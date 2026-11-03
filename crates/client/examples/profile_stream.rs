//! Read-only live stream timing against the installed helper: frames read,
//! their rate and stage timings, every acquisition record, and whether the
//! helper confirms the stream's stop. Never acquires control or sends input.
//!
//! cargo run --release --package atlas-client --example profile_stream -- [SIDE] [FRAMES] [cg|sck]
use atlas_client::client::{Client, live::LiveStream};
use atlas_core::{
    config::{self, Config},
    frame_pipe::diagnostics::Ring,
    protocol::CaptureBackend,
};
use serde_json::json;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let side: u32 = args.next().map_or(Ok(1600), |side| side.parse())?;
    let frames: usize = args.next().map_or(Ok(150), |frames| frames.parse())?;
    let backend = match args.next().as_deref() {
        Some("sck") => CaptureBackend::ScreenCaptureKit,
        _ => CaptureBackend::CoreGraphics,
    };
    let client = Client::new(Config::load(&config::installed_path()?)?)?;
    let mut stream = LiveStream::connect(&client, side, backend)
        .await?
        .ok_or("The helper offers no raw stream")?;
    let mut ring = Ring::default();
    let began = Instant::now();
    let (mut accepted, mut totals) = (0_usize, Vec::new());
    for _ in 0..frames {
        match stream.next(&client, &mut ring).await {
            Ok(frame) => {
                accepted += 1;
                totals.push(frame.metrics.total_ms);
            }
            Err(error) if stream.can_retry(&error) => {}
            Err(error) => {
                println!(
                    "{}",
                    json!({"stopped": error.error, "code": error.code.as_str()})
                );
                break;
            }
        }
    }
    let elapsed = began.elapsed().as_secs_f64();
    totals.sort_by(f64::total_cmp);
    let percentile = |p: usize| {
        totals
            .get((totals.len().saturating_sub(1)) * p / 100)
            .copied()
    };
    let records = ring.page(0, 15);
    let failed: Vec<_> = (0..)
        .map_while(|cursor| {
            let page = ring.page(cursor * 15, 15);
            (!page.records.is_empty()).then_some(page.records)
        })
        .flatten()
        .filter(|record| record.outcome != atlas_core::frame_pipe::diagnostics::Outcome::Accepted)
        .map(|record| json!({"stage": record.stage, "reason": record.reason, "outcome": record.outcome}))
        .collect();
    let stopping = Instant::now();
    let stopped = stream.stop(&client).await;
    println!(
        "{}",
        json!({
            "side": side, "backend": backend.name(), "accepted": accepted, "seconds": elapsed,
            "fps": accepted as f64 / elapsed,
            "acquisitionMs": {"p50": percentile(50), "p95": percentile(95), "p99": percentile(99)},
            "firstRecord": records.records.first(), "failed": failed,
            "stop": {"confirmed": stopped.is_ok(), "ms": stopping.elapsed().as_secs_f64() * 1000.0,
                "error": stopped.err().map(|error| error.error)},
        })
    );
    Ok(())
}
