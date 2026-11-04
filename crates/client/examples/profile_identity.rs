//! Repeated identity reads in one client. No capture, input, or ownership.
use atlas_client::client::Client;
use atlas_core::config::{self, Config};
use serde_json::json;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new(Config::load(&config::installed_path()?)?)?;
    let mut samples = Vec::new();
    for _ in 0..21 {
        let at = Instant::now();
        client.identity().await?;
        samples.push(at.elapsed().as_secs_f64() * 1000.0);
    }
    let mut warm = samples[1..].to_vec();
    warm.sort_by(f64::total_cmp);
    println!(
        "{}",
        json!({"operation": "identity only; no capture/input", "coldMs": samples[0],
            "warmMedianMs": warm[warm.len() / 2], "warmP95Ms": warm[(warm.len() - 1) * 95 / 100],
            "samplesMs": samples})
    );
    Ok(())
}
