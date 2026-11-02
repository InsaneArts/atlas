//! A real daemon on a made-up session, for the tests of this crate and those
//! above it.
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    reason = "test fixtures: failing is how they report a broken test"
)]
use crate::{
    daemon::{
        read::{self, Places},
        serve::{Daemon, handle},
    },
    helper_setup::{File, Job, Layout},
};
use atlas_core::{
    error::Result,
    protocol::{ConnectionPhase, ReadRequest},
};
use atlas_native::native::capture;
use objc2_core_foundation::CGRect;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::SystemTime};
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

/// `request` served from the folder `home` for the account `uid`, as the
/// daemon's own read serves it.
pub fn read_at(
    request: &ReadRequest,
    home: PathBuf,
    uid: u32,
    events: &mut dyn FnMut(Value) -> bool,
    now: SystemTime,
) -> Value {
    let places = Places {
        home,
        temporary: None,
    };
    read::read_at(request, &places, uid, &|| Ok(()), events, now)
}

/// The helper's check of a job's retirement, reading the job from `read_job`.
pub fn retirement_with(
    layout: &Layout,
    label: &str,
    sha256: &str,
    missing_ok: bool,
    read_job: impl FnOnce() -> Result<Job>,
) -> Result<(File, Job)> {
    crate::helper_setup::retirement_with(layout, label, sha256, missing_ok, read_job)
}

/// The digest the helper's setup keeps of `text`.
pub fn text_sha256(text: &str) -> String {
    crate::helper_setup::text_sha256(text)
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
