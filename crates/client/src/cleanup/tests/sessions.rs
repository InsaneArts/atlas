//! What a cleanup covers: its session or one task, and never a copy
//! labelled for another session.
use super::*;

#[tokio::test]
async fn a_copy_labelled_for_another_session_is_kept_even_when_this_record_lists_it() {
    let place = Place::new();
    let (key, other) = (session("mine"), session("theirs"));
    let task = default_task(&key);
    let theirs = place.copy(Some(&other));
    place.claim(&task, Owned::Copy { id: theirs.clone() });
    let fake = Fake::at(&place.root);

    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(states(&reply), ["done", "kept"]);
    assert_eq!(reply["steps"][1]["reason"], "another-session");
    assert!(place.folder(&theirs).exists());
    assert_eq!(reply["ok"], true);
}

#[tokio::test]
async fn a_session_key_never_widens_what_is_removed() {
    let place = Place::new();
    let (forger, victim) = (session("forger"), session("victim"));
    let victims = place.copy(Some(&victim));
    let unreadable = place.copy(None);
    fs::write(place.folder(&unreadable).join(".session"), "../x\n").unwrap();
    // A record that names another session's copies and one whose label
    // cannot be read, as a forged one would.
    let task = default_task(&forger);
    place.claim(
        &task,
        Owned::Copy {
            id: victims.clone(),
        },
    );
    place.claim(
        &task,
        Owned::Copy {
            id: unreadable.clone(),
        },
    );
    let fake = Fake::at(&place.root);

    let reply = place.apply(&fake, &own(&forger)).await;
    assert_eq!(states(&reply), ["done", "kept", "kept"]);
    assert!(place.folder(&victims).exists());
    assert!(place.folder(&unreadable).exists());
    assert_eq!(reply["steps"][2]["reason"], "label-unreadable");
}

#[tokio::test]
async fn a_cleanup_of_the_session_covers_every_task_and_what_its_record_lists() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let output = client.output_dir().unwrap().to_owned();
    let root = fs::canonicalize(directory.path()).unwrap().join("copies");
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let host = HostSession::new(Host::Codex, "whole").unwrap();
    let key = host.key().clone();

    // A copy only the session's own record lists, as a build without task
    // records left it.
    let recorded = Token::new();
    crate::host_session::store::note(
        &client,
        &host,
        crate::host_session::store::Kind::Open,
        Some(&crate::doing::Doing::new("open")),
        &Ok(json!({"ok": true, "copied": [{"to": format!("{}/{recorded}/a.rtf", handoff::ROOT)}]})),
    )
    .unwrap();
    let place = Place {
        _directory: tempfile::tempdir().unwrap(),
        output: output.clone(),
        root,
    };
    let named = [
        Task::of(&key, Some("one")).unwrap(),
        Task::of(&key, Some("two")).unwrap(),
    ];
    let default = default_task(&key);
    let plans: Vec<Token> = named
        .iter()
        .chain([&default])
        .map(|task| plan_claimed(&place, task))
        .collect();

    let fake = Fake::at(&place.root);
    let reply = place.apply(&fake, &own(&key)).await;

    let mut undone: Vec<String> = fake.logged();
    undone.sort();
    let mut expected: Vec<String> = plans.iter().map(|plan| format!("undo {plan}")).collect();
    expected.sort();
    assert_eq!(undone, expected, "each plan once");
    let copies: Vec<&Value> = reply["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|step| step["do"] == "remove-copy")
        .collect();
    assert_eq!(copies.len(), 1);
    assert_eq!(copies[0]["alreadyGone"], true);
    assert_eq!(reply["state"], "complete");
}

#[tokio::test]
async fn a_cleanup_of_one_task_leaves_the_others_alone() {
    let place = Place::new();
    let key = session("split");
    let (probe, other) = (
        Task::of(&key, Some("probe")).unwrap(),
        Task::of(&key, Some("other")).unwrap(),
    );
    let mine = plan_claimed(&place, &probe);
    let theirs = plan_claimed(&place, &other);
    let scope = Scope {
        session: key.clone(),
        task: Some(probe.key().clone()),
        adopting: false,
        named: true,
    };
    let fake = Fake::at(&place.root);

    let reply = place.apply(&fake, &scope).await;
    assert_eq!(fake.logged(), [format!("undo {mine}")]);
    assert_eq!(reply["task"]["name"], "probe");
    let untouched = Tasks::at(&place.output).load(other.key()).unwrap();
    assert!(untouched.claims().any(
        |(owned, state)| owned.same(&Owned::Plan { id: theirs.clone() }) && state == State::Made
    ));
    assert!(untouched.cleanups.is_empty());
}

#[test]
fn a_key_that_names_no_record_is_said_and_no_session_is_a_usage_error() {
    let place = Place::new();
    let scope = Scope {
        session: session("nobody"),
        task: None,
        adopting: true,
        named: true,
    };
    assert_eq!(
        Loaded::of(&place.output, &scope)
            .err()
            .map(|error| error.code),
        Some(Code::Input)
    );

    let directory = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(directory.path());
    let error = Scope::of(&client, None, None).unwrap_err();
    assert_eq!(error.code, "usage");
    assert!(error.error.contains("ATLAS_SESSION"), "{error}");

    let host = HostSession::new(Host::Codex, "named").unwrap();
    client.task = Task::resolve(Some(&host), Some(std::ffi::OsStr::new("probe"))).unwrap();
    client.host_session = Some(host.clone());
    let scope = Scope::of(&client, None, None).unwrap();
    assert_eq!(scope.task.as_ref(), client.task.as_ref().map(Task::key));
    assert!(!scope.adopting && !scope.named);
    let elsewhere = Scope::of(&client, Some(&session("other")), None).unwrap();
    assert_eq!((elsewhere.task, elsewhere.adopting), (None, true));
}

#[test]
fn a_task_atlas_task_names_before_it_has_a_record_has_nothing_to_clean_up() {
    let place = Place::new();
    let key = session("fresh");
    let scope = Scope {
        session: key.clone(),
        task: Some(Task::of(&key, Some("probe")).unwrap().key().clone()),
        adopting: false,
        named: false,
    };
    let loaded = Loaded::of(&place.output, &scope).unwrap();
    assert!(loaded.undoable().is_empty());
}
