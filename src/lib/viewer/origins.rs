//! Which agent's session opened each app open in the helper, and how: it
//! runs from a copy a task made, it is an app a task installed, or a task
//! opened it by name as it launched. Told from the live app list, the
//! tasks' records and the activity log, each read as it stands: nothing is
//! written, and an origin holds a session's key and a time, never a path.
use crate::{
    activity_log::Row,
    doing::bounded,
    handoff,
    helper_info::bundle_of,
    task::{Owned, State, record::Record},
    ui::text::same_text,
};
use serde::Serialize;
use serde_json::Value;
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
    apps: &Value,
    tasks: &[Record],
    rows: &[Row],
    copies: &Path,
) -> HashMap<u64, Origin> {
    apps.as_array()
        .into_iter()
        .flatten()
        .filter_map(|app| {
            let pid = app["pid"].as_u64()?;
            let running = Running {
                name: bounded(app["name"].as_str()?),
                executable: app["executable"].as_str().unwrap_or_default(),
                launched_at: app["launchedAt"].as_u64(),
            };
            running
                .copied(tasks, copies)
                .or_else(|| running.installed(tasks))
                .or_else(|| running.claimed(tasks))
                .or_else(|| running.opened(rows))
                .map(|origin| (pid, origin))
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
                Owned::Copy { id }
                    if handoff::runs_from(self.executable, &handoff::copy_folder(copies, id)) =>
                {
                    Some(self.origin(task, How::Copy, task.claimed_at(&owned)))
                }
                _ => None,
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

        newest(tasks, |owned, state, _| {
            matches!(owned, Owned::Install { app, .. } if state.live() && same_text(app, &self.name))
        })
        .map(|(task, at)| self.origin(task, How::Install, Some(at)))
    }

    /// Opened by name by a task that claimed it near this launch.
    fn claimed(&self, tasks: &[Record]) -> Option<Origin> {
        let launched = self.launched_at?;
        newest(tasks, |owned, state, at| {
            matches!(owned, Owned::Launch { app } if state == State::Made && same_text(app, &self.name))
                && launched.abs_diff(at) <= CLAIM_AROUND_LAUNCH
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
mod tests {
    use super::*;
    use crate::{files::Token, host_session::Key, task::Task};
    use serde_json::json;

    /// When the apps below launched, in milliseconds since 1970.
    const LAUNCHED: u64 = 1_800_000_000_000;

    /// A task of the session `session` names, with `claims`, each made at
    /// its time.
    fn task(session: &str, claims: &[(Owned, State, u64)]) -> Record {
        let task = Task::of(&Key::digest(session), None).unwrap();
        let mut record = Record::new(&task, LAUNCHED);
        for (owned, state, at) in claims {
            record.claim(owned, *state, *at).unwrap();
        }
        record
    }

    /// The helper's app list with one app, `name`, as process 812.
    fn running(name: &str, executable: &str, launched: Option<u64>) -> Value {
        json!([{"name": name, "pid": 812, "policy": "regular", "executable": executable,
            "launchedAt": launched}])
    }

    fn row(value: Value) -> Row {
        Row::parse(&value).unwrap()
    }

    #[test]
    fn an_app_run_from_a_sessions_copy_was_opened_by_that_session() {
        let copies = Path::new("/Users/Shared/Atlas/Open");
        let token = Token::new();
        let executable = format!(
            "{}/{token}/Probe.app/Contents/MacOS/Probe",
            copies.display()
        );
        let made = task(
            "claude:a1",
            &[(Owned::Copy { id: token }, State::Made, LAUNCHED - 500)],
        );
        let other = task(
            "codex:b2",
            &[(Owned::Copy { id: Token::new() }, State::Made, LAUNCHED)],
        );

        let found = origins(
            &running("Probe", &executable, Some(LAUNCHED)),
            &[other, made.clone()],
            &[],
            copies,
        );
        assert_eq!(
            found.get(&812),
            Some(&Origin {
                session: made.session.as_str().to_owned(),
                how: How::Copy,
                at: Some(LAUNCHED),
            })
        );
        let elsewhere = running(
            "Probe",
            "/Applications/Probe.app/Contents/MacOS/Probe",
            None,
        );
        assert!(origins(&elsewhere, &[made], &[], copies).is_empty());
    }

    #[test]
    fn an_app_launched_before_the_open_was_already_open() {
        let launch = |at: u64| {
            task(
                "claude:a1",
                &[(
                    Owned::Launch {
                        app: "Notes".into(),
                    },
                    State::Made,
                    at,
                )],
            )
        };
        let notes = |launched: Option<u64>| {
            running(
                "Notes",
                "/System/Applications/Notes.app/Contents/MacOS/Notes",
                launched,
            )
        };
        let copies = Path::new(handoff::ROOT);

        let opened = origins(
            &notes(Some(LAUNCHED)),
            &[launch(LAUNCHED + 1_500)],
            &[],
            copies,
        );
        assert_eq!(opened[&812].how, How::Launch);
        assert_eq!(opened[&812].at, Some(LAUNCHED));
        for already in [
            origins(
                &notes(Some(LAUNCHED)),
                &[launch(LAUNCHED + 60_000)],
                &[],
                copies,
            ),
            origins(&notes(None), &[launch(LAUNCHED)], &[], copies),
        ] {
            assert!(already.is_empty(), "{already:?}");
        }
        let gone = task(
            "claude:a1",
            &[(
                Owned::Launch {
                    app: "Notes".into(),
                },
                State::Gone,
                LAUNCHED,
            )],
        );
        assert!(origins(&notes(Some(LAUNCHED)), &[gone], &[], copies).is_empty());

        // With no claim, an agent's open as the app launched still tells.
        let open = |at: u64, ok: bool, actor: &str| {
            row(
                json!({"at": (u128::from(at) * 1_000_000).to_string(), "operation": "run",
                "ok": ok, "verb": "open", "app": "notes", "actor": actor,
                "session": "0123456789ab"}),
            )
        };
        let by_row = origins(
            &notes(Some(LAUNCHED)),
            &[],
            &[open(LAUNCHED + 4_000, true, "agent")],
            copies,
        );
        assert_eq!(
            by_row.get(&812),
            Some(&Origin {
                session: "0123456789ab".into(),
                how: How::Launch,
                at: Some(LAUNCHED),
            })
        );
        for (row, why) in [
            (open(LAUNCHED - 30_000, true, "agent"), "long before"),
            (open(LAUNCHED + 30_000, true, "agent"), "long after"),
            (open(LAUNCHED, false, "agent"), "it failed"),
            (open(LAUNCHED, true, "viewer"), "the person's"),
        ] {
            assert!(
                origins(&notes(Some(LAUNCHED)), &[], &[row], copies).is_empty(),
                "{why}"
            );
        }
    }

    #[test]
    fn a_launch_claim_only_names_an_app_launched_near_its_time() {
        let apps = running(
            "Notes",
            "/System/Applications/Notes.app/Contents/MacOS/Notes",
            Some(LAUNCHED),
        );
        for (offset, matches) in [
            (-60_000_i64, false),
            (-2_001, false),
            (-2_000, true),
            (0, true),
            (2_000, true),
            (2_001, false),
        ] {
            let claim = task(
                "claude:a1",
                &[(
                    Owned::Launch {
                        app: "Notes".into(),
                    },
                    State::Made,
                    LAUNCHED.checked_add_signed(offset).unwrap(),
                )],
            );
            assert_eq!(
                !origins(&apps, &[claim], &[], Path::new(handoff::ROOT)).is_empty(),
                matches,
                "claim offset {offset}"
            );
        }
    }

    #[test]
    fn the_newest_claim_within_the_launch_interval_wins_over_later_activations() {
        let apps = running(
            "Notes",
            "/System/Applications/Notes.app/Contents/MacOS/Notes",
            Some(LAUNCHED),
        );
        let launch = Owned::Launch {
            app: "Notes".into(),
        };
        let first = task(
            "claude:a1",
            &[(launch.clone(), State::Made, LAUNCHED - 500)],
        );
        let newest_valid = task("codex:b2", &[(launch.clone(), State::Made, LAUNCHED + 500)]);
        let later = task("codex:c3", &[(launch, State::Made, LAUNCHED + 60_000)]);
        for (tasks, expected) in [
            (vec![first.clone(), later.clone()], &first),
            (vec![later.clone(), first.clone()], &first),
            (
                vec![first.clone(), newest_valid.clone(), later],
                &newest_valid,
            ),
        ] {
            let found = origins(&apps, &tasks, &[], Path::new(handoff::ROOT));
            assert_eq!(
                found.get(&812),
                Some(&Origin {
                    session: expected.session.as_str().to_owned(),
                    how: How::Launch,
                    at: Some(LAUNCHED),
                })
            );
        }
    }

    #[test]
    fn repeated_opens_in_one_task_preserve_the_evidence_of_each_launch() {
        let launch = Owned::Launch {
            app: "Notes".into(),
        };
        let reopened = LAUNCHED + 120_000;
        let record = task(
            "claude:a1",
            &[
                (launch.clone(), State::Made, LAUNCHED + 500),
                (launch.clone(), State::Made, LAUNCHED + 60_000),
                (launch, State::Made, reopened + 500),
            ],
        );
        let saved = serde_json::to_value(&record).unwrap();
        let records = [serde_json::from_value(saved).unwrap()];
        for launched in [LAUNCHED, reopened] {
            let apps = running(
                "Notes",
                "/System/Applications/Notes.app/Contents/MacOS/Notes",
                Some(launched),
            );
            let found = origins(&apps, &records, &[], Path::new(handoff::ROOT));
            assert_eq!(
                found.get(&812),
                Some(&Origin {
                    session: record.session.as_str().to_owned(),
                    how: How::Launch,
                    at: Some(launched),
                })
            );
        }
    }

    #[test]
    fn an_app_installed_by_a_task_says_so() {
        let installed = |state: State| {
            task(
                "claude:a1",
                &[(
                    Owned::Install {
                        id: Token::new(),
                        app: "Probe".into(),
                    },
                    state,
                    LAUNCHED - 60_000,
                )],
            )
        };
        let probe = |executable: &str| running("Probe", executable, Some(LAUNCHED));
        let home = "/Users/helper/Applications/Probe.app/Contents/MacOS/Probe";
        let copies = Path::new(handoff::ROOT);

        let found = origins(&probe(home), &[installed(State::Made)], &[], copies);
        assert_eq!(found[&812].how, How::Install);
        assert_eq!(found[&812].at, Some(LAUNCHED));
        for (executable, tasks) in [
            (
                "/Applications/Probe.app/Contents/MacOS/Probe",
                vec![installed(State::Made)],
            ),
            (
                "/Users/helper/Desktop/Probe.app/Contents/MacOS/Probe",
                vec![installed(State::Made)],
            ),
            (
                "/Users/helper/Applications/Other.app/Contents/MacOS/Probe",
                vec![installed(State::Made)],
            ),
            (home, vec![installed(State::Gone)]),
            (home, Vec::new()),
        ] {
            assert!(
                origins(&probe(executable), &tasks, &[], copies).is_empty(),
                "{executable}"
            );
        }
    }
}
