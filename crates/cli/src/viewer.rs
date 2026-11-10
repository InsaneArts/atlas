//! The local service Atlas.app talks to: a capability URL on the loopback
//! interface that answers status, pictures of the helper, control and input
//! for one menu bar, and keeps a private record of itself in the output
//! directory so `atlas monitor` and the login item can find it.
mod accounts;
mod app;
mod control;
mod frames;
mod http;
mod local;
mod reads;
mod settings;
mod sign_in;
mod terminal;

pub(crate) use app::{launched_url, open_viewer};
pub use app::{monitor, render_menubar, serve};
pub(crate) use control::LeaseRefusal;
pub(crate) use local::{
    SharedFrame, address, answering, fetch, fetch_frame, frame_service, local, post_local, process,
    read_record, recorded, recorded_port, running,
};
pub(crate) use terminal::{TerminalFrames, TerminalService};

use crate::menubar::Line;
use atlas_client::client::Client;
use atlas_control::SharedLease;
use atlas_core::{
    error::{Code, Error},
    session::Epoch,
};
use serde_json::Value;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::{Mutex, Notify, RwLock, RwLockReadGuard, Semaphore};

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
    /// Every output folder served since the viewer started, the pictures
    /// kept in each pruned while it runs, and the signal that the settings
    /// added one or changed how long pictures are kept.
    outputs: std::sync::Mutex<Vec<PathBuf>>,
    output_moved: tokio::sync::Notify,
    lease: SharedLease,
    lease_epoch: Mutex<Option<Epoch>>,
    /// The helper the lease was taken on. Control ends there, and input
    /// goes there, even once the settings name another helper.
    lease_client: std::sync::Mutex<Option<Arc<Client>>>,
    /// Held from before native input begins until its activity recording
    /// ends. Release cancels ownership first, then waits here to flush.
    input_recording: Mutex<()>,
    /// Shared by each change being made in the helper (taking or ending
    /// control, input, recovery) for as long as it runs. Shutdown takes it
    /// whole, so it waits for those in progress, and sets it to true, so no
    /// more begin.
    closed: RwLock<bool>,
    frames: Mutex<frames::Frames>,
    /// Pictures an observer of the frame service committed, kept as what
    /// input needs of them, under their frame keys.
    committed: std::sync::Mutex<std::collections::VecDeque<(String, Arc<frames::Observation>)>>,
    /// The frame service this viewer runs, when it runs one: what a client
    /// needs to pair with it. A bearer secret, like the URL.
    pipe: std::sync::Mutex<Option<Value>>,
    /// Told each time input was sent, so the frame service reads the
    /// helper again at once rather than at a still screen's slower pace.
    input_sent: Arc<Notify>,
    /// The widest picture a capture asks for, in pixels.
    frame_width: u32,
    /// How recent the last frame must be to stand in for a capture.
    frame_reuse: Duration,
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
    account_name: std::sync::Mutex<Option<(String, atlas_core::account::Title)>>,
    /// The app this viewer serves, when it serves one.
    app: std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<Line>>>,
    /// The helper daemon and display input was last started or re-pinned
    /// for, so each gets one try.
    input_started: std::sync::Mutex<Option<String>>,
    /// Where this viewer's record is kept and what it says, once written.
    record: std::sync::Mutex<Option<(PathBuf, Value)>>,
    /// The home folder whose record of the helper's accounts this viewer
    /// shows and changes: the person's.
    accounts_home: std::sync::Mutex<Option<PathBuf>>,
    /// The sign-in under way, while its lease holds; it ends with the lease.
    sign_in: Mutex<Option<sign_in::SignIn>>,
    /// The app this viewer serves, as macOS names its process once started:
    /// the frame service shows it the screen while agents may not see it.
    app_process: std::sync::Mutex<Option<atlas_core::process::Instance>>,
    /// The secret the app names itself by, given only to the app this
    /// viewer starts, on its standard input; never written anywhere.
    app_secret: String,
    /// The menu bar's hold on the folder its record is kept in, while it
    /// serves: taken before the record moves into another, after `record`.
    alone: std::sync::Mutex<Option<atlas_core::files::LockFile>>,
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
#[path = "viewer_tests.rs"]
mod tests;
