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
    let record = Sessions::at(output).read(host.key()).unwrap();

    let error = may_adopt(Some(&record), &[]).unwrap_err();
    assert_eq!(error.code, "input");
    assert!(error.error.contains("may still be working"), "{error}");

    let mut ended = record.clone();
    ended.ended = Some(ended.last_seen);
    assert!(may_adopt(Some(&ended), &[]).is_ok());
    assert!(may_adopt(None, &[]).is_ok());

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
    let error = may_adopt(None, std::slice::from_ref(&record)).unwrap_err();
    assert_eq!(error.code, "input");

    record.updated = now_ms() - 20 * 60 * 1000;
    assert!(may_adopt(None, std::slice::from_ref(&record)).is_ok());
    record.updated = now_ms();
    record.adopted_by = Some(Adoption {
        key: None,
        at: now_ms(),
    });
    assert!(
        may_adopt(None, std::slice::from_ref(&record)).is_ok(),
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
async fn a_session_that_acts_while_adoption_waits_for_control_is_not_adopted() {
    use crate::host_session::store::{Kind, note};

    let directory = tempfile::tempdir().unwrap();
    let (client, neighbour) = ended_neighbour(directory.path());
    let scope = Scope::of(&client, Some(neighbour.key()), None).unwrap();
    let held = client.controller.acquire(false).unwrap();
    let acting = client.clone();
    let waker = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
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
        Receipts::at(output).of_session(&scope.session).is_empty(),
        "nothing recorded"
    );
}
