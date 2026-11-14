//! What writes the feed: one task in the service, the feed's only writer.
//! It looks at the marks of the stores other processes write (the activity
//! log and its pictures, the session and task records, the asks and grants,
//! the control files, the accounts, the settings) and writes an event for
//! each kind whose mark moved. What changes no file (the helper's state,
//! a session's liveness, a grant that expired) it learns from the service's
//! own status reads, which the upkeep loop makes when nobody else does, and
//! from a slow look of its own.
//!
//! A mark is taken as seen only once its event is written, so a change the
//! feed could not keep is written on a later look. A feed opened anew finds
//! every mark unseen, so every kind is written once and every reader
//! reloads. Every write happens off the runtime.
use super::{Viewer, events::Kind};
use atlas_client::client::Client;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::time::Instant;

/// How often the stores' marks are looked at.
const TICK: Duration = Duration::from_millis(300);
/// How often what no file shows is looked at, and a feed that could not be
/// written is begun again.
const SLOW: Duration = Duration::from_secs(5);
/// The most entries a folder's mark lists; past them the folder is marked
/// by its own time, which an entry made or removed moves.
const ENTRIES: usize = 4096;
/// Where the feed is kept, in the service's output folder.
const FOLDER: &str = "events";

/// One status read's view of the helper, as the producer compares it: the
/// look it came from, and what it said with the ages that tick left out.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Observed {
    /// When the look at the helper it came from was taken.
    at: Instant,
    status: Value,
    control: Value,
}

impl Viewer {
    /// Hand the producer what a status read of `client` said, from the look
    /// at the helper taken `at`: as any caller is shown it, so the person's
    /// reads and an agent's compare alike, its ticking ages and the activity
    /// rows (which have marks of their own) left out. `active` is whether
    /// this service holds control. A read of a helper the settings replaced
    /// meanwhile, or from an older look than one handed already, says
    /// nothing.
    pub(super) fn tell_producer(
        &self,
        client: &Arc<Client>,
        at: Instant,
        status: &Value,
        active: bool,
    ) {
        if !Arc::ptr_eq(client, &self.client()) {
            return;
        }
        let mut shown = status.clone();
        super::reads::keep_idle_time_for(&mut shown, false);
        if let Some(fields) = shown.as_object_mut() {
            fields.remove("consoleName");
        }
        // How old the last native dialog check is counts up by itself.
        if let Some(dialogs) = shown["capabilities"]["nativeDialogs"].as_object_mut() {
            dialogs.remove("ageSeconds");
        }
        let overview = &mut shown["overview"];
        let control = json!({
            "active": active,
            "holder": overview["holder"],
            "signingIn": overview["signingIn"],
            "signInInterrupted": overview["signInInterrupted"],
        });
        if let Some(fields) = overview.as_object_mut() {
            for person_only in ["activity", "accounts", "accountNote"] {
                fields.remove(person_only);
            }
        }
        if let Some(sessions) = overview["hostSessions"].as_array_mut() {
            sessions
                .iter_mut()
                .for_each(crate::watch::without_ticking_ages);
        }
        let reading = Observed {
            at,
            status: shown,
            control,
        };
        self.observed.send_if_modified(|newest| {
            if newest.as_ref().is_some_and(|newest| newest.at > reading.at) {
                return false;
            }
            *newest = Some(reading);
            true
        });
    }
}

/// What a file or folder looked like: each entry's name, length and time.
type Mark = Vec<(String, u64, Option<SystemTime>)>;

/// Write the feed for `viewer` until the task ends.
pub(super) async fn produce(viewer: Arc<Viewer>) {
    let mut producer = Producer::default();
    let mut observed = viewer.observed.subscribe();
    let mut ticks = tokio::time::interval(TICK);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = ticks.tick() => producer.tick(&viewer).await,
            changed = observed.changed() => {
                if changed.is_err() {
                    return;
                }
                let reading = observed.borrow_and_update().clone();
                if let Some(reading) = reading {
                    producer.observe(&viewer, reading).await;
                }
            }
        }
    }
}

/// What the producer last wrote events for.
#[derive(Default)]
struct Producer {
    /// The output folder the feed is kept in, once open.
    folder: Option<PathBuf>,
    /// The desktop the marks are of.
    desktop: Option<String>,
    marks: BTreeMap<Kind, Mark>,
    /// What the newest status read compared said.
    observed: Option<Observed>,
    /// What the slow look last said of sessions and asks.
    slow: BTreeMap<Kind, Value>,
    slow_due: Option<Instant>,
}

impl Producer {
    /// Look at every store's mark, and every [`SLOW`] at what no file shows.
    async fn tick(&mut self, viewer: &Arc<Viewer>) {
        let client = viewer.client();
        let Ok(output) = client.output_dir().map(Path::to_path_buf) else {
            return;
        };
        let desktop = client.desktop().id().as_str().to_owned();
        let slow = self.slow_due.is_none_or(|due| due <= Instant::now());
        if slow {
            self.slow_due = Some(Instant::now() + SLOW);
        }

        // Another folder or another desktop: everything is read again; so
        // it is once a feed that could not be written is begun again.
        let moved = self.folder.as_deref() != Some(output.as_path())
            || self.desktop.as_deref() != Some(desktop.as_str());
        let fresh = moved || (slow && !viewer.feed.available());
        if fresh {
            let folder = output.join(FOLDER);
            let feed = viewer.clone();
            let opened = tokio::task::spawn_blocking(move || {
                if moved {
                    feed.feed.open(&folder)
                } else {
                    feed.feed.begin(&folder)
                }
            })
            .await;
            self.folder = Some(output.clone());
            self.desktop = Some(desktop.clone());
            // One that could not be opened is begun again at the next slow
            // look, not at every tick.
            if !matches!(opened, Ok(Ok(()))) {
                return;
            }
            self.marks.clear();
            self.slow.clear();
            self.observed = None;
        }

        let places = Places::of(viewer, &output);
        let (marks, slow_values) = tokio::task::spawn_blocking({
            let client = client.clone();
            move || (places.marks(), slow.then(|| slow_values(&client)))
        })
        .await
        .unwrap_or_default();

        let moved_marks: Vec<_> = marks
            .into_iter()
            .filter(|(kind, mark)| self.marks.get(kind) != Some(mark))
            .collect();
        let written = write(viewer, &desktop, moved_marks.iter().map(|(kind, _)| *kind)).await;
        for ((kind, mark), kept) in moved_marks.into_iter().zip(written) {
            if kept {
                self.marks.insert(kind, mark);
            }
        }

        // A fresh feed told every kind by its marks already.
        let changed: Vec<_> = slow_values
            .unwrap_or_default()
            .into_iter()
            .filter(|(kind, value)| self.slow.get(kind) != Some(value))
            .collect();
        let written = if fresh {
            vec![true; changed.len()]
        } else {
            write(viewer, &desktop, changed.iter().map(|(kind, _)| *kind)).await
        };
        for ((kind, value), kept) in changed.into_iter().zip(written) {
            if kept {
                self.slow.insert(kind, value);
            }
        }
    }

    /// Compare a status read with the newest one compared before it: an
    /// older look than one already compared says nothing new. A change the
    /// feed could not keep is told once a feed is begun again, which tells
    /// every kind.
    async fn observe(&mut self, viewer: &Arc<Viewer>, reading: Observed) {
        let Some(desktop) = self.desktop.clone() else {
            return;
        };
        if self
            .observed
            .as_ref()
            .is_some_and(|before| before.at > reading.at)
        {
            return;
        }
        let before = self.observed.replace(reading.clone());
        let changed = [
            (
                Kind::Status,
                before.as_ref().map(|before| &before.status),
                &reading.status,
            ),
            (
                Kind::Control,
                before.as_ref().map(|before| &before.control),
                &reading.control,
            ),
        ]
        .into_iter()
        .filter(|(_, before, now)| *before != Some(*now))
        .map(|(kind, ..)| kind);
        write(viewer, &desktop, changed).await;
    }
}

/// Write an event of each of `kinds` for `desktop`, off the runtime:
/// whether the feed kept each.
async fn write(
    viewer: &Arc<Viewer>,
    desktop: &str,
    kinds: impl Iterator<Item = Kind>,
) -> Vec<bool> {
    let kinds: Vec<_> = kinds.collect();
    if kinds.is_empty() {
        return Vec::new();
    }
    let count = kinds.len();
    let (viewer, desktop) = (viewer.clone(), desktop.to_owned());
    tokio::task::spawn_blocking(move || {
        kinds
            .into_iter()
            .map(|kind| viewer.feed.append(&desktop, kind).is_ok())
            .collect()
    })
    .await
    .unwrap_or_else(|_| vec![false; count])
}

/// The files and folders the marks are of.
struct Places {
    activity: Vec<PathBuf>,
    sessions: Vec<PathBuf>,
    asks: Vec<PathBuf>,
    /// The control folder, and the prefix of this desktop's files in it.
    control: Option<(PathBuf, String)>,
    status: Vec<PathBuf>,
    desktops: Vec<PathBuf>,
}

impl Places {
    fn of(viewer: &Viewer, output: &Path) -> Self {
        let client = viewer.client();
        let controller = &client.controller;
        let asks = controller.asks();
        let name = client.desktop().id().as_str().to_owned();
        let home = viewer.accounts_home();
        Self {
            activity: vec![
                output.join(atlas_client::activity_log::FILE),
                output.join(atlas_client::before::FOLDER),
            ],
            sessions: vec![output.join("sessions"), output.join("tasks")],
            asks: vec![asks.folder().to_owned(), asks.allowed_path().to_owned()],
            control: controller
                .holder_path()
                .parent()
                .map(|folder| (folder.to_owned(), format!("{name}."))),
            status: home
                .iter()
                .map(|home| atlas_client::sign_ins::record_in(home))
                .collect(),
            desktops: viewer.settings.iter().cloned().collect(),
        }
    }

    /// Each kind's mark now.
    fn marks(&self) -> Vec<(Kind, Mark)> {
        let of = |paths: &[PathBuf]| paths.iter().flat_map(|path| mark(path, 2)).collect();
        let control = self
            .control
            .as_ref()
            .map_or_else(Vec::new, |(folder, prefix)| {
                mark(folder, 1)
                    .into_iter()
                    .filter(|(name, ..)| {
                        let name = Path::new(name)
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        // A folder too big to list marks as a whole.
                        name == "…"
                            || name.starts_with(prefix.as_str())
                                && !name.ends_with(".lock")
                                && !name.contains(".asks")
                                && !name.ends_with(".allowed.json")
                    })
                    .collect()
            });
        vec![
            (Kind::Activity, of(&self.activity)),
            (Kind::Sessions, of(&self.sessions)),
            (Kind::Asks, of(&self.asks)),
            (Kind::Control, control),
            (Kind::Status, of(&self.status)),
            (Kind::Desktops, of(&self.desktops)),
        ]
    }
}

/// What no file shows: the live sessions as the sessions route lists them,
/// ages left out, and the asks and grants in force now.
fn slow_values(client: &Client) -> Vec<(Kind, Value)> {
    let now = atlas_core::time::timestamp();
    let facts = atlas_client::host_session::reading::SessionFacts::local(&client.controller, now);
    let mut live = atlas_client::host_session::reading::SessionReading::collect(
        client.output_dir().ok(),
        &client.controller,
    )
    .map_or_else(Vec::new, |reading| reading.entries(facts));
    live.iter_mut().for_each(crate::watch::without_ticking_ages);
    let (asks, allowed) = super::accounts::asks_for_status(client);
    vec![
        (Kind::Sessions, Value::Array(live)),
        (Kind::Asks, json!([asks, allowed])),
    ]
}

/// The mark of `path`: the file itself, or a folder's entries `depth`
/// levels down, sorted, at most [`ENTRIES`]. Nothing there marks as empty.
fn mark(path: &Path, depth: usize) -> Mark {
    let mut found = Vec::new();
    if !walk(path, path, depth, &mut found) {
        // Too many to list: the folder's own time moves with what is made
        // or removed in it.
        let time = fs::symlink_metadata(path).and_then(|metadata| metadata.modified());
        found = vec![(format!("{}/…", path.display()), 0, time.ok())];
    }
    found.sort();
    found
}

/// Add `path`, or its entries `depth` levels down, to `found`; false once
/// there are more than [`ENTRIES`].
fn walk(root: &Path, path: &Path, depth: usize, found: &mut Mark) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return true;
    };
    let name = path
        .strip_prefix(root.parent().unwrap_or(root))
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();
    if !metadata.is_dir() || depth == 0 {
        found.push((name, metadata.len(), metadata.modified().ok()));
        return found.len() <= ENTRIES;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return true;
    };
    entries
        .flatten()
        .all(|entry| walk(root, &entry.path(), depth - 1, found))
}

#[cfg(test)]
#[path = "produce_tests.rs"]
mod tests;
