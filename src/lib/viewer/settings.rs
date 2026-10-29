//! Following the settings file: a new output folder keeps the helper and
//! moves the viewer's record, and another helper is served once control in
//! the previous one has ended.
use super::{MENUBAR_RECORD, Viewer, unpoisoned};
use crate::{client::Client, config::Config, error::Error, files};
use serde_json::Value;
use std::{path::Path, sync::Arc, time::Duration};

impl Viewer {
    /// Follow the settings file. Only a change to the file's text is read.
    /// A new output folder alone keeps the same helper: control carries on
    /// and the record moves there. Once the file names another helper
    /// (Getting Started set one up, say), the viewer serves that one without
    /// a restart. Control held in the previous helper ends there first, in a
    /// task of its own that is waited on for a moment; while control is
    /// still held (or being taken) the move waits for a later read.
    pub(super) async fn follow_settings(self: &Arc<Self>) {
        let Some(path) = self.settings.clone() else {
            return;
        };

        let read = path.clone();
        let Ok(Ok(text)) = tokio::task::spawn_blocking(move || std::fs::read(read)).await else {
            return;
        };

        if unpoisoned(&self.settings_text).as_deref() == Some(text.as_slice()) {
            return;
        }

        let Ok(Ok(config)) = tokio::task::spawn_blocking(move || Config::load(&path)).await else {
            return;
        };

        let current = self.client();
        if config == current.config {
            *unpoisoned(&self.settings_text) = Some(text);
            return;
        }

        if same_helper(&config, &current.config) {
            self.follow_output(&current, config, text).await;
            return;
        }

        let Ok(mut next) = Client::new(config) else {
            return;
        };
        next.actor = current.actor;

        let ending = self.release_in_background();
        let _ = tokio::time::timeout(Duration::from_secs(1), ending).await;

        {
            let mut lease = self.lease.lock().await;
            let mut client = unpoisoned(&self.client);
            if lease.active() || client.config == next.config {
                return;
            }

            *client = Arc::new(next);
            *unpoisoned(&self.settings_text) = Some(text);
        }

        self.frames.lock().await.items.clear();
        *unpoisoned(&self.input_started) = None;
        self.move_record();
    }

    /// The settings name another output folder for the same helper: serve
    /// it with `config` (whose file reads `text`) in place of `current`,
    /// control and all, and move the record there.
    pub(super) async fn follow_output(&self, current: &Arc<Client>, config: Config, text: Vec<u8>) {
        let mut next = Client::clone(current);
        next.config = config;
        let next = Arc::new(next);

        {
            let _lease = self.lease.lock().await;
            let mut client = unpoisoned(&self.client);
            if !Arc::ptr_eq(&client, current) {
                return;
            }

            let mut held = unpoisoned(&self.lease_client);
            if held.as_ref().is_some_and(|held| Arc::ptr_eq(held, current)) {
                *held = Some(next.clone());
            }

            *client = next;
            *unpoisoned(&self.settings_text) = Some(text);
        }

        self.move_record();
    }

    /// Keep this viewer's record in the output folder the settings name
    /// now, so commands reading them still find the menu bar.
    fn move_record(&self) {
        let client = self.client();
        let mut kept = unpoisoned(&self.record);
        let Some((path, record)) = kept.clone() else {
            return;
        };

        let Ok(output) = client.output_dir() else {
            return;
        };

        let moved = output.join(MENUBAR_RECORD);
        if moved == path {
            return;
        }

        let written = serde_json::to_vec_pretty(&record)
            .map_err(Error::from)
            .and_then(|bytes| files::write_private(&moved, &bytes));
        if written.is_ok() {
            remove_record(&path);
            *kept = Some((moved, record));
        }
    }
}

/// Whether two settings name the same helper: everything but where output
/// goes.
fn same_helper(one: &Config, other: &Config) -> bool {
    let moved = Config {
        output_dir: other.output_dir.clone(),
        ..one.clone()
    };

    moved == *other
}

/// Remove the record at `path` when it names this process.
pub(super) fn remove_record(path: &Path) {
    let ours = std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .is_some_and(|record| record["pid"] == std::process::id());
    if ours {
        let _ = std::fs::remove_file(path);
    }
}
