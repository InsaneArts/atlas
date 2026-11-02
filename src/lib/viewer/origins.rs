//! Which agent's session opened each app open in the helper, and how: it
//! runs from a copy a task made, it is an app a task installed, or a task
//! opened it by name as it launched. Told from the live app list, the
//! tasks' records and the activity log, each read as it stands: nothing is
//! written, and an origin holds a session's key and a time, never a path.
use crate::{
    activity_log::Row,
    client::AppList,
    handoff,
    helper_info::bundle_of,
    name::{bounded, same_text},
    task::{Owned, State, record::Record},
};
use serde::Serialize;
use std::{
    collections::HashMap,
    path::{Component, Path},
};

/// How far from an app's launch a task may claim opening it, in
/// milliseconds: `open` can return just before or after the app launches.
const CLAIM_AROUND_LAUNCH: u64 = 2_000;
/// How long before and after an app launched an agent's row opening it may
/// have been written, in milliseconds.
const OPEN_BEFORE_LAUNCH: u64 = 2_000;
const OPEN_AFTER_LAUNCH: u64 = 10_000;
/// Where a home's own apps are, as `/Users/NAME/Applications`.
const HOMES: &str = "/Users";
const APPLICATIONS: &str = "Applications";

/// How an agent's session came to open an app.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum How {
    /// Opened by name, or from where it is installed.
    Launch,
    /// Run from a copy Atlas made for the helper.
    Copy,
    /// Installed into the helper by one of the session's tasks.
    Install,
}

/// Which session opened an app, how, and when.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(super) struct Origin {
    /// The host session's key; empty for an agent no host named.
    pub(super) session: String,
    pub(super) how: How,
    /// When it was opened, in milliseconds since 1970, when known.
    pub(super) at: Option<u64>,
}

/// Who opened each of `apps` (the helper's app list), by process ID, as
/// `tasks` and `rows` (the activity log, newest first) tell, with copies
/// under `copies`. An app none of them names was already open.
pub(super) fn origins(
    apps: &AppList,
    tasks: &[Record],
    rows: &[Row],
    copies: &Path,
) -> HashMap<u64, Origin> {
    apps.rows()
        .iter()
        .filter_map(|app| {
            let running = Running {
                name: bounded(app.name()?),
                executable: app.executable().unwrap_or_default(),
                launched_at: app.launched_at(),
            };
            running
                .copied(tasks, copies)
                .or_else(|| running.installed(tasks))
                .or_else(|| running.claimed(tasks))
                .or_else(|| running.opened(rows))
                .map(|origin| (app.pid(), origin))
        })
        .collect()
}

/// One app open in the helper, as its origin is told.
struct Running<'a> {
    /// Its name, as a row or claim keeps one.
    name: String,
    executable: &'a str,
    launched_at: Option<u64>,
}

impl Running<'_> {
    /// Run from a copy folder a task's claim names.
    fn copied(&self, tasks: &[Record], copies: &Path) -> Option<Origin> {
        tasks.iter().find_map(|task| {
            task.claims().find_map(|(owned, _)| match &owned {
                Owned::Copy { id } => {
                    handoff::runs_from(self.executable, &handoff::copy_folder(copies, id))
                        .then(|| self.origin(task, How::Copy, task.claimed_at(&owned)))
                }
                Owned::Plan { .. } | Owned::Install { .. } | Owned::Launch { .. } => None,
            })
        })
    }

    /// Run from a home's Applications folder as an app of the name a task
    /// installed and has not removed.
    fn installed(&self, tasks: &[Record]) -> Option<Origin> {
        let bundle = Path::new(bundle_of(self.executable)?);
        let stem = bundle.file_stem()?.to_str()?;
        if !in_home_applications(bundle) || !same_text(&bounded(stem), &self.name) {
            return None;
        }

        newest(tasks, |owned, state, _| match owned {
            Owned::Install { app, .. } => state.live() && same_text(app, &self.name),
            Owned::Copy { .. } | Owned::Plan { .. } | Owned::Launch { .. } => false,
        })
        .map(|(task, at)| self.origin(task, How::Install, Some(at)))
    }

    /// Opened by name by a task that claimed it near this launch.
    fn claimed(&self, tasks: &[Record]) -> Option<Origin> {
        let launched = self.launched_at?;
        newest(tasks, |owned, state, at| match owned {
            Owned::Launch { app } => {
                state == State::Made
                    && same_text(app, &self.name)
                    && launched.abs_diff(at) <= CLAIM_AROUND_LAUNCH
            }
            Owned::Copy { .. } | Owned::Plan { .. } | Owned::Install { .. } => false,
        })
        .map(|(task, _)| self.origin(task, How::Launch, Some(launched)))
    }

    /// Opened by an agent's command that worked as it launched, by the
    /// newest such row.
    fn opened(&self, rows: &[Row]) -> Option<Origin> {
        let launched = self.launched_at?;
        let earliest = launched.saturating_sub(OPEN_BEFORE_LAUNCH);
        let latest = launched.saturating_add(OPEN_AFTER_LAUNCH);
        rows.iter()
            .find(|row| {
                row.verb.as_deref() == Some("open")
                    && row.worked()
                    && row.actor() == "agent"
                    && row
                        .app
                        .as_deref()
                        .is_some_and(|app| same_text(app, &self.name))
                    && row
                        .at_ms()
                        .is_some_and(|at| (earliest..=latest).contains(&at))
            })
            .map(|row| Origin {
                session: row.session.clone().unwrap_or_default(),
                how: How::Launch,
                at: Some(launched),
            })
    }

    /// `task`'s session opening it `how`, at its launch or else `claimed`.
    fn origin(&self, task: &Record, how: How, claimed: Option<u64>) -> Origin {
        Origin {
            session: task.session.as_str().to_owned(),
            how,
            at: self.launched_at.or(claimed),
        }
    }
}

/// The task among `tasks` with the newest claim `wanted` takes, with that
/// claim's time.
fn newest(tasks: &[Record], wanted: impl Fn(&Owned, State, u64) -> bool) -> Option<(&Record, u64)> {
    tasks
        .iter()
        .flat_map(|task| {
            task.timed_claims()
                .filter(|(owned, state, at)| wanted(owned, *state, *at))
                .map(move |(_, _, at)| (task, at))
        })
        .max_by_key(|(_, at)| *at)
}

/// Whether `bundle` is right inside a home's own Applications folder, as
/// `/Users/NAME/Applications/Probe.app`, where Atlas installs into the
/// helper.
fn in_home_applications(bundle: &Path) -> bool {
    let parts = bundle.components().collect::<Vec<_>>();
    matches!(
        parts.as_slice(),
        [Component::RootDir, Component::Normal(homes), Component::Normal(_), Component::Normal(folder), Component::Normal(_)]
            if Path::new("/").join(homes) == Path::new(HOMES) && *folder == APPLICATIONS
    )
}

#[cfg(test)]
mod tests;
