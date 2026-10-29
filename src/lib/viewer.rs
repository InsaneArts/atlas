//! The local service Atlas.app talks to: a capability URL on the loopback
//! interface that answers status, pictures of the helper, control and input
//! for one menu bar, and keeps a private record of itself in the output
//! directory so `atlas monitor` and the login item can find it.
mod app;
mod control;
mod frames;
mod http;
mod local;
mod reads;
mod settings;

pub(crate) use app::open_viewer;
pub use app::{monitor, render_menubar, serve};
pub(crate) use local::{SharedFrame, fetch, fetch_frame, local, post_local, recorded, running};

use crate::{
    client::Client,
    control::SharedLease,
    error::{Code, Error},
    helper_info,
    menubar::Line,
    session::Epoch,
};
use serde_json::Value;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::{Mutex, RwLock, RwLockReadGuard, Semaphore};

/// How long control lasts without a renewal; the app is told the same.
const LEASE_SECONDS: u64 = 10;
/// What the viewer's own address answers, so it is told apart from any
/// other program on its port.
const SERVICE: &str = "atlas-viewer";

/// The record of the running viewer, in the output folder.
pub(crate) const MENUBAR_RECORD: &str = "menubar.json";

/// One running viewer: the helper it serves, the control lease the app
/// holds, and the frames it has shown.
struct Viewer {
    /// The helper this viewer serves. It follows the settings when they
    /// name another one.
    client: std::sync::Mutex<Arc<Client>>,
    /// The settings file the client was made from, when it follows one.
    settings: Option<PathBuf>,
    /// That file's text when last found to match the client.
    settings_text: std::sync::Mutex<Option<Vec<u8>>>,
    lease: SharedLease,
    lease_epoch: Mutex<Option<Epoch>>,
    /// The helper the lease was taken on. Control ends there, and input
    /// goes there, even once the settings name another helper.
    lease_client: std::sync::Mutex<Option<Arc<Client>>>,
    /// Shared by each change being made in the helper (taking or ending
    /// control, input, recovery) for as long as it runs. Shutdown takes it
    /// whole, so it waits for those in progress, and sets it to true, so no
    /// more begin.
    closed: RwLock<bool>,
    frames: Mutex<frames::Frames>,
    capture: std::sync::Mutex<Option<frames::Capture>>,
    requests: Semaphore,
    urgent: Semaphore,
    /// The viewer's private address. Whoever holds it may control the
    /// helper, so it is never printed.
    url: String,
    prefix: String,
    authority: String,
    /// The helper account's short name and what directory services said of
    /// its full name, once they answered.
    account_name: std::sync::Mutex<Option<(String, crate::setup::Title)>>,
    /// The app this viewer serves, when it serves one.
    app: std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<Line>>>,
    /// The helper daemon and display input was last started or re-pinned
    /// for, so each gets one try.
    input_started: std::sync::Mutex<Option<String>>,
    /// Where this viewer's record is kept and what it says, once written.
    record: std::sync::Mutex<Option<(PathBuf, Value)>>,
    /// The helper's app icons, made once.
    icons: helper_info::Icons,
}

impl Viewer {
    /// The helper this viewer serves right now.
    fn client(&self) -> Arc<Client> {
        unpoisoned(&self.client).clone()
    }

    /// The helper control ends in: the one the lease was taken on, or the
    /// current one when none was.
    fn holder(&self) -> Arc<Client> {
        let held = unpoisoned(&self.lease_client).clone();
        held.unwrap_or_else(|| self.client())
    }

    /// Leave to change something in the helper, held until the change ends;
    /// None once shutdown began.
    async fn changing(&self) -> Option<RwLockReadGuard<'_, bool>> {
        let closed = self.closed.read().await;
        (!*closed).then_some(closed)
    }

    /// Wait for every change in progress in the helper to end, however long
    /// that takes, then let no more begin.
    async fn close(&self) {
        *self.closed.write().await = true;
    }
}

/// A lock that a panicked holder left poisoned still guards plain data.
fn unpoisoned<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The local viewer could not be reached, or its reply not read.
fn unanswered() -> Error {
    Error::new(Code::Menubar, "The menu bar's viewer did not answer")
}

#[cfg(test)]
mod tests;
