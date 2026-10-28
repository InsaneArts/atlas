use super::{
    record::{Entry, OWNED, Record, VERSION},
    *,
};
use crate::host_session::{Host, store::Store as Sessions};
use serde_json::json;
use std::{fs, path::Path};

fn session(id: &str) -> Key {
    Key::of(Host::Codex, id)
}

fn task(id: &str, name: Option<&str>) -> Task {
    Task::of(&session(id), name).unwrap()
}

fn copy() -> Owned {
    Owned::Copy { id: Token::new() }
}

fn plan() -> Owned {
    Owned::Plan { id: Token::new() }
}

/// The claims `store` holds for `task`, with their states.
fn claimed(store: &Store, task: &Task) -> Vec<(Owned, State)> {
    store.load(task.key()).unwrap().claims().collect()
}

/// The bytes of `task`'s record file in `output`.
fn bytes(output: &Path, task: &Task) -> Vec<u8> {
    fs::read(output.join("tasks").join(format!("{}.json", task.key()))).unwrap()
}

#[test]
fn a_task_name_is_a_bounded_plain_label() {
    let key = session("one");
    for good in ["settings-sheet", "a.b_c:1", &"n".repeat(NAME_LIMIT)] {
        let task = Task::of(&key, Some(good)).unwrap();
        assert_eq!(task.name().map(Name::as_str), Some(good));
        assert_ne!(task.key(), &key);
    }
    for default in [None, Some("")] {
        let task = Task::of(&key, default).unwrap();
        assert_eq!((task.key(), task.name()), (&key, None), "{default:?}");
    }

    let long = "n".repeat(NAME_LIMIT + 1);
    for bad in [
        long.as_str(),
        "two words",
        "a/b",
        "café",
        "line\nbreak",
        "../x",
    ] {
        let error = Task::of(&key, Some(bad)).unwrap_err();
        assert_eq!(error.code, "usage", "{bad:?}");
    }
}

#[test]
fn the_same_name_in_two_sessions_is_two_tasks() {
    let (left, right) = (task("left", Some("probe")), task("right", Some("probe")));
    assert_ne!(left.key(), right.key());
    assert_eq!(left.key(), task("left", Some("probe")).key(), "stable");
    assert_eq!(
        left.key(),
        &Key::digest(&format!("task:{}:probe", session("left")))
    );
}

#[test]
fn a_command_without_a_host_session_has_no_task_even_with_a_name() {
    assert_eq!(Task::resolve(None, Some("probe")).unwrap(), None);
    assert_eq!(Task::resolve(None, Some("not plain")).unwrap(), None);

    let host = HostSession::new(Host::Named, "release").unwrap();
    let named = Task::resolve(Some(&host), Some("probe")).unwrap().unwrap();
    assert_eq!(named.session(), host.key());
    assert!(Task::resolve(Some(&host), Some("not plain")).is_err());
}

#[test]
fn a_claim_is_written_whole_and_settled_in_place() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let task = task("one", Some("probe"));
    let claims = Claims::in_store(Store::at(directory.path()), task.clone());
    let (first, second) = (copy(), plan());

    claims.claim(&first, State::Intent).unwrap();
    claims.claim(&second, State::Made).unwrap();
    claims.settle(&first, State::Made).unwrap();
    claims.settle(&first, State::Gone).unwrap();

    assert_eq!(
        claimed(&store, &task),
        [(first, State::Gone), (second, State::Made)]
    );
    let record = store.load(task.key()).unwrap();
    assert_eq!(record.version, VERSION);
    assert_eq!(record.session, *task.session());
    assert_eq!(record.name, task.name().cloned());
    let mode = |path: &Path| {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    };
    assert_eq!(mode(&directory.path().join("tasks")), 0o700);
    assert_eq!(
        mode(&directory.path().join(format!("tasks/{}.json", task.key()))),
        0o600
    );
}

#[test]
fn a_session_record_rewritten_by_an_older_build_loses_no_claim() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::transaction_test::client(directory.path());
    let output = client.output_dir().unwrap().to_owned();
    let host = HostSession::new(Host::Codex, "one").unwrap();
    let task = Task::of(host.key(), None).unwrap();
    Claims::in_store(Store::at(&output), task.clone())
        .claim(&copy(), State::Made)
        .unwrap();
    let before = bytes(&output, &task);

    // A build that knows only the session record rewrites it with its own
    // fields, as every command it records does.
    Sessions::at(&output)
        .update(&host, now_ms(), &client.controller, |record| {
            record.copies.clear();
            record.plans.clear();
        })
        .unwrap();

    assert_eq!(bytes(&output, &task), before);
}

#[test]
fn claims_from_two_commands_at_once_both_survive() {
    let directory = tempfile::tempdir().unwrap();
    let task = task("one", None);
    let made: Vec<Owned> = (0..8).map(|_| copy()).collect();

    std::thread::scope(|scope| {
        for owned in &made {
            let (output, task) = (directory.path(), task.clone());
            scope.spawn(move || {
                Claims::in_store(Store::at(output), task)
                    .claim(owned, State::Intent)
                    .unwrap();
            });
        }
    });

    let kept = claimed(&Store::at(directory.path()), &task);
    assert_eq!(kept.len(), made.len());
    assert!(
        made.iter()
            .all(|owned| kept.iter().any(|(kept, _)| kept == owned))
    );
}

#[test]
fn a_full_record_drops_finished_entries_first_and_refuses_a_claim_when_none_are_finished() {
    let task = task("one", None);
    let mut record = Record::new(&task, 1);
    let first = copy();
    record.claim(&first, State::Made, 1).unwrap();
    let launch = Owned::Launch {
        app: "TextEdit".into(),
    };
    record.claim(&launch, State::Made, 2).unwrap();
    let gone = copy();
    record.claim(&gone, State::Gone, 3).unwrap();
    for at in 4..=u64::try_from(OWNED).unwrap() {
        record.claim(&copy(), State::Made, at).unwrap();
    }
    assert_eq!(record.owned.len(), OWNED);

    let held = |record: &Record, owned: &Owned| record.claims().any(|(kept, _)| kept == *owned);
    record.claim(&copy(), State::Intent, 1000).unwrap();
    assert!(!held(&record, &gone), "the finished one goes first");
    record.claim(&copy(), State::Intent, 1001).unwrap();
    assert!(!held(&record, &launch), "then an app only listed");

    let error = record.claim(&copy(), State::Intent, 1002).unwrap_err();
    assert_eq!(error.code, "state");
    assert_eq!(record.owned.len(), OWNED);
    assert!(held(&record, &first), "nothing unfinished is forgotten");

    record.claim(&launch, State::Made, 1003).unwrap_err();
    record.settle(&first, State::Gone, 1004);
    record.claim(&launch, State::Made, 1005).unwrap();
}

#[test]
fn a_record_a_later_build_wrote_keeps_what_this_one_does_not_know() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let task = task("one", Some("probe"));
    let known = copy();
    let Owned::Copy { id } = &known else {
        unreachable!()
    };
    let install = json!({"kind": "install", "id": Token::new(), "app": "Probe",
        "at": 5, "state": "intent"});
    let written = json!({
        "version": VERSION, "key": task.key(), "session": task.session(), "name": "probe",
        "started": 1, "updated": 5,
        "owned": [
            {"kind": "copy", "id": id, "at": 2, "state": "made", "note": "later"},
            install,
            {"kind": "plan", "id": Token::new(), "at": 3, "state": "detached"},
        ],
        "jobs": [{"id": "j1"}], "outputs": [], "adoptedBy": null,
    });
    fs::create_dir(directory.path().join("tasks")).unwrap();
    fs::write(
        directory.path().join(format!("tasks/{}.json", task.key())),
        written.to_string(),
    )
    .unwrap();

    let claims = Claims::in_store(Store::at(directory.path()), task.clone());
    claims.settle(&known, State::Gone).unwrap();
    claims.claim(&plan(), State::Made).unwrap();

    let rewritten: serde_json::Value =
        serde_json::from_slice(&bytes(directory.path(), &task)).unwrap();
    assert_eq!(rewritten["jobs"], written["jobs"]);
    assert_eq!(rewritten["outputs"], json!([]));
    assert!(rewritten.as_object().unwrap().contains_key("adoptedBy"));
    assert_eq!(rewritten["owned"][0]["note"], "later");
    assert_eq!(rewritten["owned"][0]["state"], "gone");
    assert_eq!(rewritten["owned"][1], install);
    assert_eq!(rewritten["owned"][2], written["owned"][2]);
    assert_eq!(rewritten["owned"].as_array().unwrap().len(), 4);

    let record = store.load(task.key()).unwrap();
    assert!(matches!(record.owned[1], Entry::Later(_)));
    assert!(
        record.holds_anything(),
        "what it cannot read is never finished"
    );
}

#[test]
fn a_record_this_build_cannot_rewrite_is_left_as_it_is() {
    let directory = tempfile::tempdir().unwrap();
    let task = task("one", None);
    let folder = directory.path().join("tasks");
    fs::create_dir(&folder).unwrap();
    let path = folder.join(format!("{}.json", task.key()));
    let later = json!({"version": VERSION + 1, "key": task.key(), "session": task.session(),
        "name": null, "started": 1, "updated": 1, "owned": []})
    .to_string();

    for written in [later.as_str(), "{not json"] {
        fs::write(&path, written).unwrap();
        let claims = Claims::in_store(Store::at(directory.path()), task.clone());
        let error = claims.claim(&copy(), State::Intent).unwrap_err();
        assert_eq!(error.code, "state");
        assert_eq!(fs::read_to_string(&path).unwrap(), written);
    }
}

#[test]
fn a_rolled_back_plan_is_settled_in_every_task_that_claimed_it() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::transaction_test::client(directory.path());
    let output = client.output_dir().unwrap().to_owned();
    let (left, right) = (task("left", None), task("right", Some("probe")));
    let (undone, kept) = (Token::new(), Token::new());
    for task in [&left, &right] {
        let claims = Claims::in_store(Store::at(&output), task.clone());
        for id in [&undone, &kept] {
            claims
                .claim(&Owned::Plan { id: id.clone() }, State::Made)
                .unwrap();
        }
    }

    let changes = Kind::Environment { changes: true };
    let rolled_back = |id: &Token, kept: &[&str]| {
        Ok(
            json!({"ok": true, "planId": id, "phase": "rolledBack", "keptTargets": kept,
            "retainedDirectories": []}),
        )
    };
    settle_rolled_back(&client, changes, &rolled_back(&undone, &[])).unwrap();
    settle_rolled_back(&client, changes, &rolled_back(&kept, &[".config/probe"])).unwrap();
    let preview = Ok(json!({"ok": true, "planId": undone, "phase": "rolledBack", "preview": true}));
    settle_rolled_back(&client, changes, &preview).unwrap();

    let store = Store::at(&output);
    for task in [&left, &right] {
        assert_eq!(
            claimed(&store, task),
            [
                (Owned::Plan { id: undone.clone() }, State::Gone),
                (Owned::Plan { id: kept.clone() }, State::Kept),
            ]
        );
    }
}

#[test]
fn a_finished_task_goes_once_its_session_record_has_gone() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let (finished, unfinished) = (task("gone", Some("a")), task("gone", Some("b")));
    let claims = |task: &Task| Claims::in_store(Store::at(directory.path()), task.clone());
    let made = copy();
    claims(&finished).claim(&made, State::Made).unwrap();
    claims(&finished).settle(&made, State::Gone).unwrap();
    claims(&unfinished).claim(&copy(), State::Made).unwrap();

    // A new record is made, and the others are looked over.
    claims(&task("new", None))
        .claim(&copy(), State::Intent)
        .unwrap();

    assert!(store.load(finished.key()).is_none());
    assert!(store.load(unfinished.key()).is_some());
    assert_eq!(store.of_session(&session("gone")).len(), 1);
}
