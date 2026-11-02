//! Cleaning up after another session: allowed once it has ended or for
//! the person, refused while it may still be working, and noted in the
//! records of the session adopted.
use super::*;

#[test]
fn adoption_is_refused_while_the_session_is_active_and_allowed_for_the_person() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let host = HostSession::new(Host::Codex, "neighbour").unwrap();
    crate::host_session::store::note(
        &client,
        &host,
        crate::host_session::store::Kind::Other,
        Some(&crate::doing::Doing::new("click")),
        &Ok(json!({"ok": true})),
    )
    .unwrap();
    let output = client.output_dir().unwrap();
    let record = Sessions::at(output).read_file(host.key());

    let error = may_adopt(&record, &[]).unwrap_err();
    assert_eq!(error.code, "input");
    assert!(error.error.contains("may still be working"), "{error}");

    let mut ended = record.as_ref().unwrap().clone();
    ended.ended = Some(ended.last_seen);
    assert!(may_adopt(&SessionRead::Record(Box::new(ended)), &[]).is_ok());
    assert!(may_adopt(&SessionRead::Missing, &[]).is_ok());

    let person = Scope::for_person(host.key());
    assert!(person.adopting && person.named && person.task.is_none());
}

#[tokio::test]
async fn an_adopted_session_is_noted_in_its_records_and_its_labels_are_checked_against_it() {
    let place = Place::new();
    let (old, new) = (session("ended"), session("adopter"));
    let task = default_task(&old);
    let copy = place.copy(Some(&old));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let scope = Scope {
        session: old.clone(),
        task: None,
        adopting: true,
        named: true,
    };
    let fake = Fake::at(&place.root);

    let reply = apply(
        &fake,
        &place.output,
        &scope,
        &place.loaded(&scope),
        Actor::Agent,
        Some(Adopter::Session(new.clone())),
    )
    .await
    .unwrap();

    assert_eq!(reply["ok"], true);
    assert!(!place.folder(&copy).exists());
    let record = Tasks::at(&place.output).load(task.key()).unwrap();
    assert_eq!(
        record.adopted_by.map(|adoption| adoption.key),
        Some(Some(new.clone()))
    );
    assert_eq!(place.receipt(&reply).adopted_by, Some(new));
    assert!(
        Tasks::at(&place.output)
            .of_session(&session("adopter"))
            .is_empty(),
        "nothing moves into the adopting session"
    );
}

#[test]
fn fresh_task_records_keep_a_session_without_its_own_record_from_being_adopted() {
    let task = default_task(&session("records-only"));
    let mut record = crate::task::record::Record::new(&task, now_ms());
    let error = may_adopt(&SessionRead::Missing, std::slice::from_ref(&record)).unwrap_err();
    assert_eq!(error.code, "input");

    record.updated = now_ms() - 20 * 60 * 1000;
    assert!(may_adopt(&SessionRead::Missing, std::slice::from_ref(&record)).is_ok());
    record.updated = now_ms();
    record.adopted_by = Some(Adoption {
        key: None,
        at: now_ms(),
    });
    assert!(
        may_adopt(&SessionRead::Missing, std::slice::from_ref(&record)).is_ok(),
        "a cleanup's own notes never count"
    );
}

/// A client whose controller waits for control, and an agent session that
/// ended a while ago and left an app opened by name.
fn ended_neighbour(directory: &Path) -> (Client, HostSession) {
    use crate::host_session::store::{Kind, note};

    let mut client = crate::test_support::client(directory);
    client.controller = client
        .controller
        .waiting(std::time::Duration::from_secs(10));
    client.host_session = Some(HostSession::new(Host::Codex, "adopter").unwrap());
    let neighbour = HostSession::new(Host::Codex, "neighbour").unwrap();
    note(
        &client,
        &neighbour,
        Kind::Other,
        Some(&crate::doing::Doing::new("click")),
        &Ok(json!({"ok": true})),
    )
    .unwrap();
    let output = client.output_dir().unwrap().to_owned();
    let long_ago = now_ms() - 30 * 60 * 1000;
    Sessions::at(&output)
        .update(&neighbour, long_ago, &client.controller, |record| {
            record.last_seen = long_ago;
            record.ended = Some(long_ago);
        })
        .unwrap();
    Tasks::at(&output)
        .update(&default_task(neighbour.key()), long_ago, |record| {
            record.claim(
                &Owned::Launch {
                    app: "TextEdit".into(),
                },
                State::Made,
                long_ago,
            )
        })
        .unwrap();
    (client, neighbour)
}

#[tokio::test]
async fn an_agent_adopts_a_session_that_has_ended() {
    let directory = tempfile::tempdir().unwrap();
    let (client, neighbour) = ended_neighbour(directory.path());
    let scope = Scope::of(&client, Some(neighbour.key()), None).unwrap();

    let reply = apply_scope(&client, &scope).await.unwrap();
    assert_eq!((&reply["ok"], &reply["steps"]), (&json!(true), &json!([])));
    assert_eq!(reply["left"][0]["app"], "TextEdit");
}

#[tokio::test]
async fn a_preserved_session_with_stale_tasks_cannot_be_adopted() {
    let directory = tempfile::tempdir().unwrap();
    let (client, neighbour) = ended_neighbour(directory.path());
    let output = client.output_dir().unwrap();
    let sessions = Sessions::at(output);
    let process = crate::host_session::Instance::current().unwrap();
    assert!(process.running());
    let long_ago = sessions.read(neighbour.key()).unwrap().last_seen;
    sessions
        .update(&neighbour, long_ago, &client.controller, |record| {
            record.host_process = Some(process);
            record.ended = None;
        })
        .unwrap();
    let record = sessions.read(neighbour.key()).unwrap();
    assert!(scope::alive(&record));
    let mut future = serde_json::to_value(&record).unwrap();
    future["version"] = json!(record.version + 1);
    future["futureField"] = json!("private durable canary");
    let mut mismatched = serde_json::to_value(&record).unwrap();
    mismatched["key"] = json!(session("different"));
    let path = output.join(format!("sessions/{}.json", neighbour.key()));
    let task = default_task(neighbour.key());
    let task_path = output.join(format!("tasks/{}.json", task.key()));
    let task_bytes = fs::read(&task_path).unwrap();
    let tasks = Tasks::at(output).of_session(neighbour.key());
    assert_eq!(tasks.len(), 1);
    assert!(tasks.iter().all(|task| task.updated == long_ago));
    let scope = Scope::of(&client, Some(neighbour.key()), None).unwrap();

    for bytes in [
        serde_json::to_vec(&future).unwrap(),
        b"{malformed private durable canary".to_vec(),
        serde_json::to_vec(&mismatched).unwrap(),
    ] {
        fs::write(&path, &bytes).unwrap();
        let error = apply_scope(&client, &scope).await.unwrap_err();
        assert_eq!(error.code, "input");
        assert_eq!(
            error.error,
            "That session's record cannot be read by this version of Atlas and may still be working; ask the person."
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::read(&task_path).unwrap(), task_bytes);
        assert!(
            Receipts::at(output)
                .of_session(neighbour.key())
                .unwrap()
                .is_empty()
        );
    }
}

#[tokio::test]
async fn a_recent_read_protects_an_unreadable_neighbour_with_old_tasks_from_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    let (client, neighbour) = ended_neighbour(directory.path());
    let output = client.output_dir().unwrap();
    let sessions = Sessions::at(output);
    let unreadable = crate::host_session::Instance::new(1, [1, 0]);
    assert_eq!(unreadable.state(), atlas_core::process::State::Unknown);
    let long_ago = sessions.read(neighbour.key()).unwrap().last_seen;
    sessions
        .update(&neighbour, long_ago, &client.controller, |record| {
            record.host_process = Some(unreadable);
            record.ended = None;
        })
        .unwrap();
    let tasks = Tasks::at(output).of_session(neighbour.key());
    assert_eq!(tasks.len(), 1);
    assert!(tasks.iter().all(|task| task.updated == long_ago));

    crate::host_session::store::touch(&client, &neighbour).unwrap();
    let record = sessions.read(neighbour.key()).unwrap();
    assert!(record.last_seen > long_ago);
    assert_eq!(record.ended_at(), None);
    assert!(scope::alive(&record));
    assert!(may_adopt(&sessions.read_file(neighbour.key()), &tasks).is_err());

    let scope = Scope::of(&client, Some(neighbour.key()), None).unwrap();
    let error = apply_scope(&client, &scope).await.unwrap_err();
    assert_eq!(error.code, "input");
    assert!(error.error.contains("may still be working"), "{error}");
    assert_eq!(sessions.read(neighbour.key()).unwrap(), record);
    assert_eq!(Tasks::at(output).of_session(neighbour.key()), tasks);
    assert!(
        Receipts::at(output)
            .of_session(neighbour.key())
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_session_that_acts_while_adoption_waits_for_control_is_not_adopted() {
    use crate::host_session::store::{Kind, note};
    use atlas_control::WaitEvent;

    let directory = tempfile::tempdir().unwrap();
    let (mut client, neighbour) = ended_neighbour(directory.path());
    let (waiting, began) = std::sync::mpsc::channel();
    client.controller = client.controller.on_wait(move |event| {
        if event == WaitEvent::Began {
            waiting.send(()).unwrap();
        }
    });
    let scope = Scope::of(&client, Some(neighbour.key()), None).unwrap();
    let held = client.controller.acquire(false).unwrap();
    let acting = client.clone();
    let waker = std::thread::spawn(move || {
        began
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("adoption reached its held controller lock");
        note(
            &acting,
            &neighbour,
            Kind::Other,
            Some(&crate::doing::Doing::new("click")),
            &Ok(json!({"ok": true})),
        )
        .unwrap();
        drop(held);
    });

    let error = apply_scope(&client, &scope).await.unwrap_err();
    waker.join().unwrap();
    assert_eq!(error.code, "input");
    assert!(error.error.contains("may still be working"), "{error}");
    let output = client.output_dir().unwrap();
    assert!(
        Receipts::at(output)
            .of_session(&scope.session)
            .unwrap()
            .is_empty(),
        "nothing recorded"
    );
}
