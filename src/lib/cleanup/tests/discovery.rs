use super::*;
use crate::task::record::Record as TaskRecord;
use std::{collections::HashSet, io::Write};

fn write_record(path: &std::path::Path, record: &impl serde::Serialize) {
    let bytes = serde_json::to_vec(record).unwrap();
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .unwrap();
    file.write_all(&bytes).unwrap();
}

#[tokio::test]
async fn cleanup_discovers_and_removes_a_default_tasks_install_beyond_the_listing_prefix() {
    let place = Place::new();
    let directory = place.output.join("tasks");
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    let mut records = Vec::new();
    for index in 0..318 {
        let key = session(&format!("discovery-{index}"));
        let record = TaskRecord::new(&default_task(&key), 1);
        write_record(&directory.join(format!("{}.json", record.key)), &record);
        records.push(record);
    }
    let store = Tasks::at(&place.output);
    let listed: HashSet<_> = store.all().into_iter().map(|record| record.key).collect();
    let mut omitted = records
        .into_iter()
        .find(|record| !listed.contains(&record.key))
        .expect("The fixture exceeds the ordinary listing prefix");
    let install_id = Token::new();
    let install = Owned::Install {
        id: install_id.clone(),
        app: "Owned discovery fixture".into(),
    };
    omitted.claim(&install, State::Made, 2).unwrap();
    // Rewrite the existing inode; choosing a missing key must not depend on
    // this filesystem's directory order or move it into the original prefix.
    write_record(&directory.join(format!("{}.json", omitted.key)), &omitted);
    assert!(store.of_session(&omitted.session).is_empty());

    let loaded = place.loaded(&own(&omitted.session));
    assert_eq!(loaded.tasks, [omitted.clone()]);
    assert!(loaded.work.iter().any(|work| work.owned.same(&install)));
    let fake = Fake::at(&place.root);
    let scope = own(&omitted.session);
    let preview = preview(&fake, &place.output, &scope, &loaded)
        .await
        .unwrap();
    assert!(
        preview["steps"].as_array().unwrap().iter().any(|step| {
            step["do"] == "remove-install" && step["installId"] == json!(install_id)
        })
    );
    let reply = place.apply(&fake, &scope).await;
    assert_eq!(reply["state"], "complete");
    assert_eq!(
        fake.logged()
            .iter()
            .filter(|call| **call == format!("remove {install_id}"))
            .count(),
        1
    );
    assert!(place.receipt(&reply).steps.iter().all(|step| step.settled));
    let saved = store.load(&omitted.key).unwrap();
    assert!(
        saved
            .claims()
            .any(|(owned, state)| owned.same(&install) && state == State::Gone)
    );
    let retry = Fake::at(&place.root);
    assert_eq!(place.apply(&retry, &scope).await["state"], "complete");
    assert!(retry.logged().is_empty());
}

#[tokio::test]
async fn a_finished_receipt_beyond_the_listing_prefix_prevents_repeating_its_effect() {
    let place = Place::new();
    let directory = place.output.join("cleanup");
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    let mut receipts = Vec::new();
    for index in 0..318 {
        let receipt = Receipt::new(
            session(&format!("receipt-discovery-{index}")),
            None,
            Actor::Agent,
        );
        write_record(&directory.join(format!("{}.json", receipt.id)), &receipt);
        receipts.push(receipt);
    }
    let listed: HashSet<_> = fs::read_dir(&directory)
        .unwrap()
        .take(crate::record_folder::LISTED)
        .map(|entry| entry.unwrap().file_name())
        .collect();
    let mut omitted = receipts
        .into_iter()
        .find(|receipt| !listed.contains(&std::ffi::OsString::from(format!("{}.json", receipt.id))))
        .expect("The fixture exceeds the ordinary listing prefix");
    let plan = Token::new();
    place.claim(
        &default_task(&omitted.session),
        Owned::Plan { id: plan.clone() },
    );
    let mut step = Entry::new(1, Step::UndoPlan { plan });
    step.set(StepState::Done, None);
    step.at = 1;
    omitted.steps.push(step);
    omitted.state = CleanupState::Complete;
    write_record(&directory.join(format!("{}.json", omitted.id)), &omitted);

    let loaded = place.loaded(&own(&omitted.session));
    assert!(
        loaded
            .receipts
            .iter()
            .any(|receipt| receipt.id == omitted.id)
    );
    assert!(loaded.undoable().is_empty());
    let fake = Fake::at(&place.root);
    let reply = place.apply(&fake, &own(&omitted.session)).await;
    assert_eq!(reply["state"], "complete");
    assert!(fake.logged().is_empty());
    let saved = Receipts::at(&place.output).load(&omitted.id).unwrap();
    assert_eq!(saved.steps.len(), 1);
    assert_eq!(saved.steps[0].at, 1);
    assert!(saved.steps[0].settled);
}

#[test]
fn an_omitted_active_named_task_prevents_adoption_even_when_another_task_is_selected() {
    let place = Place::new();
    let key = session("named-discovery");
    let directory = place.output.join("tasks");
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    let mut records = Vec::new();
    for index in 0..318 {
        let task = Task::of(&key, Some(&format!("work-{index}"))).unwrap();
        let record = TaskRecord::new(&task, 1);
        write_record(&directory.join(format!("{}.json", record.key)), &record);
        records.push(record);
    }
    let store = Tasks::at(&place.output);
    let listed: HashSet<_> = store.all().into_iter().map(|record| record.key).collect();
    let selected = records
        .iter()
        .find(|record| listed.contains(&record.key))
        .unwrap()
        .key
        .clone();
    let mut omitted = records
        .into_iter()
        .find(|record| !listed.contains(&record.key))
        .unwrap();
    omitted.updated = now_ms();
    omitted
        .claim(
            &Owned::Plan { id: Token::new() },
            State::Made,
            omitted.updated,
        )
        .unwrap();
    write_record(&directory.join(format!("{}.json", omitted.key)), &omitted);
    assert!(
        !store
            .of_session(&key)
            .iter()
            .any(|record| record.key == omitted.key)
    );
    let scope = Scope {
        session: key,
        task: Some(selected),
        adopting: true,
        named: true,
    };
    let loaded = place.loaded(&scope);
    assert_eq!(loaded.tasks.len(), 318);
    assert!(loaded.tasks.iter().any(|record| record.key == omitted.key));
    let error = scope::may_adopt(&loaded.record, &loaded.tasks).unwrap_err();
    assert_eq!(error.code, Code::Input);
    assert!(!error.uncertain);
    assert!(error.error.contains("may still be working"));
}

#[test]
fn discovery_preserves_and_omits_opaque_task_and_receipt_formats() {
    let place = Place::new();
    let key = session("opaque-discovery");
    let task = TaskRecord::new(&default_task(&key), 1);
    let receipt = Receipt::new(key.clone(), None, Actor::Agent);
    let task_path = place.output.join(format!("tasks/{}.json", task.key));
    let receipt_path = place.output.join(format!("cleanup/{}.json", receipt.id));
    for directory in ["tasks", "cleanup"] {
        fs::DirBuilder::new()
            .mode(0o700)
            .create(place.output.join(directory))
            .unwrap();
    }
    for fault in ["malformed", "future", "mismatched"] {
        let mut task_value = serde_json::to_value(&task).unwrap();
        let mut receipt_value = serde_json::to_value(&receipt).unwrap();
        if fault == "future" {
            task_value["version"] = json!(u32::MAX);
            receipt_value["version"] = json!(u32::MAX);
        } else {
            task_value["key"] = json!(session("other-key"));
            receipt_value["id"] = json!(Token::new());
        }
        let bytes = |value: &Value| {
            if fault == "malformed" {
                b"{opaque private canary".to_vec()
            } else {
                serde_json::to_vec(value).unwrap()
            }
        };
        let task_bytes = bytes(&task_value);
        let receipt_bytes = bytes(&receipt_value);
        fs::write(&task_path, &task_bytes).unwrap();
        fs::write(&receipt_path, &receipt_bytes).unwrap();
        let loaded = place.loaded(&own(&key));
        assert!(loaded.tasks.is_empty());
        assert!(loaded.receipts.is_empty());
        assert!(loaded.work.is_empty());
        assert_eq!(fs::read(&task_path).unwrap(), task_bytes);
        assert_eq!(fs::read(&receipt_path).unwrap(), receipt_bytes);
    }
}

#[test]
fn discovery_of_missing_stores_creates_no_record_folders() {
    let place = Place::new();
    let loaded = place.loaded(&own(&session("absent-discovery")));
    assert!(loaded.tasks.is_empty());
    assert!(loaded.receipts.is_empty());
    for directory in ["tasks", "cleanup"] {
        assert!(!place.output.join(directory).exists());
    }
}

#[test]
fn an_unreadable_record_cannot_turn_complete_discovery_into_an_empty_scope() {
    for (directory, name, limit) in [
        (
            "tasks",
            session("oversized-discovery").to_string(),
            256 * 1024,
        ),
        ("cleanup", Token::new().to_string(), 512 * 1024),
    ] {
        let place = Place::new();
        fs::DirBuilder::new()
            .mode(0o700)
            .create(place.output.join(directory))
            .unwrap();
        let path = place.output.join(directory).join(format!("{name}.json"));
        let bytes = vec![b'x'; limit + 1];
        fs::write(&path, &bytes).unwrap();
        let error = Loaded::of(&place.output, &own(&session("oversized-discovery")))
            .err()
            .expect("An incomplete physical read is not absence");
        assert_eq!(error.code, Code::State);
        assert!(!error.uncertain);
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}
