//! Read-only capture timing; never acquires control or sends helper input.
use atlas_client::client::Client;
use atlas_core::config::{self, Config};
use serde_json::json;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new(Config::load(&config::installed_path()?)?)?;
    for _ in 0..3 {
        let began = Instant::now();
        client.identity().await?;
        println!(
            "{}",
            json!({"read": "identity", "ms": began.elapsed().as_secs_f64() * 1000.0})
        );
    }
    for side in [1600, 960, 640, 480] {
        for _ in 0..3 {
            let began = Instant::now();
            let frame = client.frame(Some(side)).await?;
            println!(
                "{}",
                json!({"read": "frame", "side": side, "ms": began.elapsed().as_secs_f64() * 1000.0,
                    "bytes": frame.png.len(), "width": frame.width, "height": frame.height})
            );
        }
    }
    Ok(())
}
