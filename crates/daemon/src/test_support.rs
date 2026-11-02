//! A real daemon on a made-up session, for the tests of this crate and those
//! above it.
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    reason = "test fixtures: failing is how they report a broken test"
)]
use crate::daemon::serve::{Daemon, handle};
use atlas_core::{error::Result, protocol::ConnectionPhase};
use atlas_native::native::capture;
use objc2_core_foundation::CGRect;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::{net::UnixListener, sync::watch};

/// A screenshot of the made-up display.
pub fn synthetic(display: u32, _: Option<CGRect>, _: Option<u32>) -> Result<capture::Shot> {
    assert_eq!(display, 7);
    Ok(capture::Shot {
        png: b"\x89PNG synthetic".to_vec(),
        width: 64,
        height: 36,
        scale: 1.0,
    })
}

/// The made-up helper session: off the console, on display 7.
pub fn session() -> Result<Value> {
    Ok(json!({
        "ok": true,
        "uid": 502,
        "securitySessionID": 42,
        "hasGraphicAccess": true,
        "onConsole": false,
        "displayID": 7,
        "width": 64.0,
        "height": 36.0,
    }))
}

/// Answer every connection to `listener` as a real daemon does.
pub fn serve(listener: UnixListener) {
    let daemon = Arc::new(Daemon::new("Aqua".into(), synthetic, || false, session));
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(handle(
                stream,
                daemon.clone(),
                true,
                watch::channel(ConnectionPhase::Refusal).0,
            ));
        }
    });
}
