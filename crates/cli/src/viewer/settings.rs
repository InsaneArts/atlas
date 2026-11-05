//! Following the settings file: a new output folder keeps the helper and
//! moves the viewer's record, and another helper is served once control in
//! the previous one has ended.
use super::{
    MENUBAR_RECORD, Viewer,
    local::{process, read_record},
    unpoisoned,
};
use atlas_client::client::Client;
use atlas_core::{config::Config, error::Error, files};
use std::{path::Path, sync::Arc};

impl Viewer {
    /// Follow the settings file. Only a change to the file's text is read.
    /// A new output folder alone keeps the same helper: control carries on
    /// and the record moves there. Once the file names another helper
    /// (Getting Started set one up, say), the viewer serves that one without
    /// a restart. Control held in the previous helper ends there first, in a
    /// task of its own that is waited on for a moment; while control is
    /// still held (or being taken) the move waits for a later read. The
    /// record follows the helper served on every read, so a move that failed
    /// is tried again.
    pub(super) async fn follow_settings(self: &Arc<Self>) {
        let (output, keep) = {
            let config = &self.client().config;
            (config.output_dir.clone(), config.activity.keep())
        };
        self.apply_settings().await;
        let (moved, kept) = {
            let config = &self.client().config;
            (config.output_dir.clone(), config.activity.keep())
        };
        if moved != output || kept != keep {
            // Kept before the signal, which coalesces: a folder served only
            // between two prunings is pruned all the same.
            let mut outputs = unpoisoned(&self.outputs);
            if !outputs.contains(&moved) {
                outputs.push(moved);
            }
            drop(outputs);
            self.output_moved.notify_one();
        }
        self.move_record();
    }

    /// Serve what the settings file names, when its text changed.
    async fn apply_settings(self: &Arc<Self>) {
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

        if config.same_helper(&current.config) {
            self.follow_output(&current, config, text).await;
            return;
        }

        let Ok(mut next) = Client::new(config) else {
            return;
        };
        next.actor = current.actor;
        next.person_input = current.person_input.clone();

        self.release_shortly().await;

        {
            let mut lease = self.lease.lock().await;
            let mut client = unpoisoned(&self.client);
            if lease.active() || client.config == next.config {
                return;
            }

            *client = Arc::new(next);
            *unpoisoned(&self.settings_text) = Some(text);
        }

        self.frames.lock().await.clear();
        *unpoisoned(&self.input_started) = None;
    }

    /// The settings name another output folder for the same helper: serve
    /// it with `config` (whose file reads `text`) in place of `current`,
    /// control and all.
    async fn follow_output(&self, current: &Arc<Client>, config: Config, text: Vec<u8>) {
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
    }

    /// Keep this viewer's record in the output folder of the helper served
    /// now, so commands reading the settings still find the menu bar. The
    /// kept record is where it was last written; it changes only once the
    /// new one is written.
    fn move_record(&self) {
        // Moves take the record first and the client inside it, so the last
        // one to run writes where the newest client says.
        let mut kept = unpoisoned(&self.record);
        let Some((path, record)) = kept.clone() else {
            return;
        };

        // Every read comes here: the folder is checked only to move.
        let client = self.client();
        let moved = client.config.output_dir.join(MENUBAR_RECORD);
        if moved == path || client.output_dir().is_err() {
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

/// Remove the record at `path` when it names this process.
pub(super) fn remove_record(path: &Path) {
    if read_record(path).is_some_and(|record| process(&record) == Some(std::process::id())) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests;
