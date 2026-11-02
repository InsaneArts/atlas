use super::*;
use crate::{files::Token, host_session::Key, task::Task};
use serde_json::{Value, json};

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
fn running(name: &str, executable: &str, launched: Option<u64>) -> AppList {
    let apps = json!([{"name": name, "pid": 812, "policy": "regular",
        "executable": executable, "launchedAt": launched}]);
    AppList::decode(json!({"ok": true, "apps": apps})).unwrap()
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
