//! Following the settings file: every desktop it lists is served, a new
//! output folder keeps the helper and moves the viewer's record, and
//! another desktop is viewed once control in the previous one has ended.
use super::{
    MENUBAR_RECORD, Viewer,
    local::{process, read_record},
    unpoisoned,
    viewed::viewed_in,
};
use atlas_client::client::Client;
use atlas_core::{
    config::desktops::{Chosen, Selected, Settings},
    desktop::RecordKey,
    error::Error,
    files,
};
use serde_json::json;
use std::{path::Path, sync::Arc};

impl Viewer {
    /// Follow the settings file. Only a change to the file's text, or to the
    /// desktop the person chose to view, is read. A new output folder alone
    /// keeps the same helper: control carries on and the record moves
    /// there. Once the file names another helper (Getting Started set one
    /// up, say), or the person views another desktop, the viewer serves
    /// that one without a restart. Control held in the previous helper ends
    /// there first, in a task of its own that is waited on for a moment;
    /// while control is still held (or being taken) the move waits for a
    /// later read. The record follows the default desktop on every read, so
    /// a move that failed is tried again.
    pub(super) async fn follow_settings(self: &Arc<Self>) {
        let keep = || self.client().selected.desktop.activity().keep();
        let before = keep();
        self.apply_settings().await;
        let mut moved = keep() != before;
        // Kept before the signal, which coalesces: a folder served only
        // between two prunings is pruned all the same.
        let mut outputs = unpoisoned(&self.outputs);
        for desk in self.desks() {
            let folder = desk.client.records().to_path_buf();
            if !outputs.contains(&folder) {
                outputs.push(folder);
                moved = true;
            }
        }
        drop(outputs);
        if moved {
            self.output_moved.notify_one();
        }
        self.move_record();
    }

    /// Serve what the settings file names, when its text or the desktop the
    /// person chose to view changed: every desktop it lists, the one viewed
    /// among them.
    async fn apply_settings(self: &Arc<Self>) {
        let Some(path) = self.settings.clone() else {
            return;
        };

        let read = path.clone();
        let Ok(Ok(text)) = tokio::task::spawn_blocking(move || std::fs::read(read)).await else {
            return;
        };
        let seen = (text, self.chosen());

        if unpoisoned(&self.settings_text).as_ref() == Some(&seen) {
            return;
        }

        let Ok(Ok(settings)) = tokio::task::spawn_blocking(move || Settings::load(&path)).await
        else {
            return;
        };
        self.list_desks(&settings).await;
        let selected = settings.selected(viewed_in(&settings, seen.1.as_ref()));

        let current = self.client();
        if selected == current.selected {
            *unpoisoned(&self.settings_text) = Some(seen);
            return;
        }

        if same_helper(&selected, &current.selected) {
            self.follow_output(&current, selected, seen).await;
            return;
        }

        let listed = self
            .desks()
            .into_iter()
            .find(|desk| desk.client.selected == selected);
        let Some(next) = listed.map(|desk| desk.client) else {
            return;
        };
        self.switch_to(next, seen).await;
    }

    /// Serve `next`, another desktop, in place of the one served now, once
    /// control held there has ended, and keep `seen` as what the settings
    /// said: control held there ends first, in a task of its own that is
    /// waited on for a moment; while control is still held (or being
    /// taken), or a sign-in an earlier service died holding is still the
    /// person's to finish there, nothing changes. Its frames go with it.
    async fn switch_to(self: &Arc<Self>, next: Arc<Client>, seen: (Vec<u8>, Option<RecordKey>)) {
        self.release_shortly().await;

        {
            // Never under a request that holds the helper it named.
            let Ok(_switching) = self.pinned.try_write() else {
                return;
            };
            let mut lease = self.lease.lock().await;
            // A sign-in taken over stays this viewer's helper until the
            // person finishes or cancels it: that helper's hold and
            // exclusion are this viewer's to keep.
            let mut interrupted = unpoisoned(&self.interrupted);
            let mut client = unpoisoned(&self.client);
            if lease.active() || interrupted.is_some() || client.selected == next.selected {
                return;
            }
            // The next helper's own interrupted sign-in is taken over before
            // it is served; one that can't be stays on the current helper.
            match self.take_over_for(&next) {
                Ok(taken) => *interrupted = taken,
                Err(_) => return,
            }
            drop(interrupted);

            *client = next;
            *unpoisoned(&self.settings_text) = Some(seen);
        }

        self.frames.lock().await.clear();
        *unpoisoned(&self.input_started) = None;
    }

    /// The settings name another output folder for the same helper: serve
    /// it with `selected` in place of `current`, control and all, and keep
    /// `seen` as what the settings said.
    async fn follow_output(
        &self,
        current: &Arc<Client>,
        selected: Selected,
        seen: (Vec<u8>, Option<RecordKey>),
    ) {
        let mut next = Client::clone(current);
        next.selected = selected;
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
            *unpoisoned(&self.settings_text) = Some(seen);
        }
    }

    /// Keep this viewer's record in the output folder of the default
    /// desktop, so commands of earlier builds reading the settings still
    /// find the menu bar, and its station-wide record naming every
    /// desktop's folder, which every command finds it by. The kept record
    /// is where it was last written; it changes only once the new one is
    /// written.
    fn move_record(&self) {
        self.keep_discovery();
        // Moves take the record first and the client inside it, so the last
        // one to run writes where the newest client says.
        let mut kept = unpoisoned(&self.record);
        let Some((path, record)) = kept.clone() else {
            return;
        };

        // Every read comes here: the folder is checked only to move.
        let client = self
            .desks()
            .into_iter()
            .find(|desk| desk.default)
            .map_or_else(|| self.client(), |desk| desk.client);
        let moved = client.records().join(MENUBAR_RECORD);
        if moved == path || client.output_dir().is_err() {
            return;
        }
        // A folder another menu bar keeps its record in stays its own.
        let Ok(alone) = crate::menubar::alone(client.records()) else {
            return;
        };

        let written = serde_json::to_vec_pretty(&record)
            .map_err(Error::from)
            .and_then(|bytes| files::write_private(&moved, &bytes));
        if written.is_ok() {
            remove_record(&path);
            *kept = Some((moved, record));
            *unpoisoned(&self.alone) = Some(alone);
        }
    }

    /// Keep the station-wide record naming the records folder of every
    /// desktop served, once this service keeps one: written again only when
    /// they changed.
    fn keep_discovery(&self) {
        let mut kept = unpoisoned(&self.discovery);
        let Some((path, record)) = kept.as_mut() else {
            return;
        };
        let folders = json!(
            self.desks()
                .iter()
                .map(|desk| desk.client.records().to_path_buf())
                .collect::<Vec<_>>()
        );
        if record["records"] == folders {
            return;
        }
        let mut next = record.clone();
        next["records"] = folders;
        let written = serde_json::to_vec_pretty(&next)
            .map_err(Error::from)
            .and_then(|bytes| files::write_private(path, &bytes));
        if written.is_ok() {
            *record = next;
        }
    }
}

/// Whether `next` names the Second Login `current` does, its output folder
/// or what the activity keeps aside, so control carries on in it. A VM
/// desktop's client is made anew, its records folder with it.
fn same_helper(next: &Selected, current: &Selected) -> bool {
    match (&next.desktop, &current.desktop) {
        (Chosen::User(next), Chosen::User(current)) => next.same_helper(current),
        _ => false,
    }
}

/// Remove the record at `path` when it names this process.
pub(super) fn remove_record(path: &Path) {
    if read_record(path).is_some_and(|record| process(&record) == Some(std::process::id())) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
