//! Finished effects and durable record acknowledgement remain separate.
use super::*;
use crate::host_session::record::Record as SessionRecord;
use crate::record_folder::LISTED;

fn finished_plan(place: &Place, key: &Key, plan: &Token, state: StepState) -> Receipt {
    let mut entry = Entry::new(1, Step::UndoPlan { plan: plan.clone() });
    entry.set(state, None);
    let mut receipt = Receipt::new(key.clone(), None, Actor::Agent);
    receipt.steps.push(entry);
    Receipts::at(&place.output).save(&mut receipt).unwrap();
    receipt
}

fn session_plan(place: &Place, key: &Key, plan: &Token) -> PathBuf {
    let host = HostSession::new(Host::Codex, "settlement").unwrap();
    let mut record = SessionRecord::new(&host, 1, None);
    record.key = key.clone();
    record.planned(plan.clone(), crate::environment::Phase::Applied, 1);
    let folder = place.output.join("sessions");
    fs::DirBuilder::new().mode(0o700).create(&folder).unwrap();
    let path = folder.join(format!("{key}.json"));
    fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
    path
}

fn assert_unsettled(place: &Place, receipt: &Receipt) {
    let saved = Receipts::at(&place.output).load(&receipt.id).unwrap();
    assert_eq!(saved.steps[0].state, receipt.steps[0].state);
    assert!(!saved.steps[0].settled);
}

fn reconcile_place(place: &Place, scope: &Scope) -> Result<()> {
    reconcile(
        &Receipts::at(&place.output),
        &Tasks::at(&place.output),
        &Sessions::at(&place.output),
        &place.loaded(scope),
    )
}

#[test]
fn unreadable_or_future_records_keep_finished_steps_unsettled_until_repaired() {
    for state in [StepState::Done, StepState::Kept] {
        for kind in ["tasks", "sessions"] {
            for future in [false, true] {
                let place = Place::new();
                let key = session("settlement");
                let task = default_task(&key);
                let plan = Token::new();
                place.claim(&task, Owned::Plan { id: plan.clone() });
                let session_path = session_plan(&place, &key, &plan);
                let path = if kind == "tasks" {
                    place.output.join(format!("tasks/{}.json", task.key()))
                } else {
                    session_path
                };
                let original = fs::read(&path).unwrap();
                let bad = if future {
                    let mut value: Value = serde_json::from_slice(&original).unwrap();
                    value["version"] = json!(u32::MAX);
                    value["future"] = json!({"private": "preserve-this-byte-sequence"});
                    serde_json::to_vec(&value).unwrap()
                } else {
                    b"{private broken record".to_vec()
                };
                let receipt = finished_plan(&place, &key, &plan, state);
                fs::write(&path, &bad).unwrap();
                reconcile_place(&place, &own(&key)).unwrap();
                assert_unsettled(&place, &receipt);
                assert_eq!(fs::read(&path).unwrap(), bad, "{kind}, {state:?}");

                fs::write(&path, original).unwrap();
                reconcile_place(&place, &own(&key)).unwrap();
                let saved = Receipts::at(&place.output).load(&receipt.id).unwrap();
                assert!(saved.steps[0].settled, "{kind}, {state:?}");
                let record = Tasks::at(&place.output).load(task.key()).unwrap();
                let expected = if state == StepState::Done {
                    State::Gone
                } else {
                    State::Kept
                };
                assert!(record.claims().all(|(_, found)| found == expected));
                assert!(
                    Sessions::at(&place.output)
                        .read(&key)
                        .unwrap()
                        .plans
                        .is_empty()
                );
            }
        }
    }
}

#[test]
fn failed_record_writes_and_bounded_scans_never_acknowledge_cleanup() {
    for state in [StepState::Done, StepState::Kept] {
        for kind in ["tasks", "sessions"] {
            for failure in ["write", "scan", "lock"] {
                let place = Place::new();
                let key = session("settlement");
                let task = default_task(&key);
                let plan = Token::new();
                place.claim(&task, Owned::Plan { id: plan.clone() });
                session_plan(&place, &key, &plan);
                let receipt = finished_plan(&place, &key, &plan, state);
                let folder = place.output.join(kind);
                let faults: Vec<_> = match failure {
                    "scan" => (0..LISTED)
                        .map(|n| folder.join(format!("unrelated-{n}")))
                        .collect(),
                    "lock" => vec![folder.join(".lock")],
                    _ => vec![folder.join(format!("{key}.json.partial"))],
                };
                for fault in &faults {
                    if failure == "lock" && fault.exists() {
                        fs::remove_file(fault).unwrap();
                    }
                    fs::create_dir(fault).unwrap();
                }
                reconcile_place(&place, &own(&key)).unwrap();
                assert_unsettled(&place, &receipt);
                for fault in faults {
                    fs::remove_dir(fault).unwrap();
                }
                reconcile_place(&place, &own(&key)).unwrap();
                assert!(Receipts::at(&place.output).load(&receipt.id).unwrap().steps[0].settled);
            }
        }
    }
}

#[test]
fn a_record_folder_that_is_a_file_cannot_prove_an_empty_settlement() {
    for kind in ["tasks", "sessions"] {
        let place = Place::new();
        fs::write(place.output.join(kind), b"preserve folder fault").unwrap();
        let plan = Token::new();
        let result = if kind == "tasks" {
            Tasks::at(&place.output).settle_everywhere_checked(
                &Owned::Plan { id: plan },
                State::Gone,
                1,
            )
        } else {
            Sessions::at(&place.output).forget_plan_checked(&plan)
        };
        assert_eq!(result.unwrap_err().code, Code::State);
        assert_eq!(
            fs::read(place.output.join(kind)).unwrap(),
            b"preserve folder fault"
        );
    }
}

#[tokio::test]
async fn failed_receipt_acknowledgement_prevents_new_effects_and_retry_never_repeats_old_effects() {
    let place = Place::new();
    let key = session("settlement");
    let task = default_task(&key);
    let finished = Token::new();
    place.claim(
        &task,
        Owned::Plan {
            id: finished.clone(),
        },
    );
    let receipt = finished_plan(&place, &key, &finished, StepState::Done);
    let later = Token::new();
    place.claim(&task, Owned::Plan { id: later.clone() });
    let scope = own(&key);
    let loaded = place.loaded(&scope);
    let fault = place
        .output
        .join(format!("cleanup/{}.json.partial", receipt.id));
    fs::create_dir(&fault).unwrap();
    let hands = Fake::at(&place.root);
    let result = apply(&hands, &place.output, &scope, &loaded, Actor::Agent, None).await;
    let error = result.unwrap_err();
    let path = place.output.join(format!("cleanup/{}.json", receipt.id));
    let message = format!(
        "Write record {}: {}: {}",
        path.display(),
        fault.display(),
        std::io::Error::from_raw_os_error(libc::EISDIR)
    );
    assert_eq!(
        json!(error),
        json!({"ok":false,"code":"io","error":message,"uncertain":false})
    );
    assert!(
        hands.logged().is_empty(),
        "no further helper effect after failed receipt save"
    );
    assert_unsettled(&place, &receipt);

    fs::remove_dir(fault).unwrap();
    let retry = Fake::at(&place.root);
    let reply = place.apply(&retry, &scope).await;
    assert_eq!(reply["ok"], true);
    assert_eq!(retry.logged(), [format!("undo {later}")]);
    assert!(Receipts::at(&place.output).load(&receipt.id).unwrap().steps[0].settled);
}

#[test]
fn confirmed_missing_record_folders_allow_acknowledgement_without_creating_storage() {
    let place = Place::new();
    let plan = Token::new();
    Tasks::at(&place.output)
        .settle_everywhere_checked(&Owned::Plan { id: plan.clone() }, State::Gone, 1)
        .unwrap();
    Sessions::at(&place.output)
        .forget_plan_checked(&plan)
        .unwrap();
    assert!(!place.output.join("tasks").exists());
    assert!(!place.output.join("sessions").exists());
}

#[tokio::test]
async fn repeated_apply_keeps_unacknowledged_completion_without_repeating_its_effect() {
    for state in [StepState::Done, StepState::Kept] {
        let place = Place::new();
        let key = session("settlement");
        let task = default_task(&key);
        let plan = Token::new();
        place.claim(&task, Owned::Plan { id: plan.clone() });
        let path = session_plan(&place, &key, &plan);
        let original = fs::read(&path).unwrap();
        fs::write(&path, b"{private unfinished record").unwrap();
        let scope = own(&key);
        let hands = Fake::at(&place.root);
        if state == StepState::Kept {
            hands.undo.borrow_mut().insert(
                plan.clone(),
                Ok(Undone::Kept {
                    targets: vec![".config/probe/settings.toml".into()],
                    directories: 0,
                }),
            );
        }
        let first = place.apply(&hands, &scope).await;
        // These are the effect's established semantics. Settlement is separate.
        assert_eq!(
            (first["ok"].clone(), first["state"].clone()),
            (json!(true), json!("complete"))
        );
        assert_eq!(hands.logged(), [format!("undo {plan}")]);
        let receipt = place.receipt(&first);
        assert_eq!(receipt.steps[0].state, state);
        assert_unsettled(&place, &receipt);

        let retry = Fake::at(&place.root);
        let second = place.apply(&retry, &scope).await;
        assert_eq!(second["ok"], true);
        assert!(retry.logged().is_empty());
        assert_unsettled(&place, &receipt);
        assert_eq!(fs::read(&path).unwrap(), b"{private unfinished record");

        fs::write(&path, original).unwrap();
        let repaired = Fake::at(&place.root);
        let third = place.apply(&repaired, &scope).await;
        assert_eq!(third["ok"], true);
        assert!(repaired.logged().is_empty());
        assert!(Receipts::at(&place.output).load(&receipt.id).unwrap().steps[0].settled);
    }
}

#[test]
fn pruning_preserves_finished_unsettled_receipts_and_unknown_bytes() {
    let place = Place::new();
    let key = session("settlement");
    let plan = Token::new();
    let mut receipt = finished_plan(&place, &key, &plan, StepState::Done);
    receipt.updated = 0;
    let path = place.output.join(format!("cleanup/{}.json", receipt.id));
    fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    let unknown = place.output.join(format!("cleanup/{}.json", Token::new()));
    fs::write(&unknown, b"{private unknown receipt").unwrap();
    Receipts::at(&place.output).prune(&Token::new(), |_| false);
    assert!(path.exists());
    assert_eq!(fs::read(&unknown).unwrap(), b"{private unknown receipt");
    receipt.steps[0].settled = true;
    fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    Receipts::at(&place.output).prune(&Token::new(), |_| false);
    assert!(!path.exists());
}

#[test]
fn nested_unknown_ownership_keeps_finished_steps_unsettled_and_preserves_the_record() {
    for state in [StepState::Done, StepState::Kept] {
        for fault in ["future-state", "future-kind", "invalid-state", "missing-id"] {
            let place = Place::new();
            let key = session("nested-unknown");
            let task = default_task(&key);
            let plan = Token::new();
            place.claim(&task, Owned::Plan { id: plan.clone() });
            let path = place.output.join(format!("tasks/{}.json", task.key()));
            let original = fs::read(&path).unwrap();
            let mut record: Value = serde_json::from_slice(&original).unwrap();
            match fault {
                "future-state" => record["owned"][0]["state"] = json!("future-state"),
                "future-kind" => record["owned"][0]["kind"] = json!("future-kind"),
                "invalid-state" => record["owned"][0]["state"] = json!(7),
                _ => {
                    assert!(
                        record["owned"][0]
                            .as_object_mut()
                            .unwrap()
                            .remove("id")
                            .is_some()
                    );
                }
            }
            record["owned"][0]["privateFuture"] = json!("keep exact nested ownership");
            let preserved = serde_json::to_vec_pretty(&record).unwrap();
            fs::write(&path, &preserved).unwrap();
            let receipt = finished_plan(&place, &key, &plan, state);

            reconcile_place(&place, &own(&key)).unwrap();
            assert_unsettled(&place, &receipt);
            assert_eq!(fs::read(&path).unwrap(), preserved, "{fault}, {state:?}");
            assert_eq!(
                Tasks::at(&place.output).checked_all().unwrap_err().code,
                Code::State
            );

            fs::write(&path, original).unwrap();
            reconcile_place(&place, &own(&key)).unwrap();
            assert!(Receipts::at(&place.output).load(&receipt.id).unwrap().steps[0].settled);
        }
    }
}

#[tokio::test]
async fn continuing_an_incomplete_receipt_retains_reconciled_acknowledgement() {
    let place = Place::new();
    let key = session("incomplete-receipt");
    let task = default_task(&key);
    let finished = Token::new();
    let pending = Token::new();
    for plan in [&finished, &pending] {
        place.claim(&task, Owned::Plan { id: plan.clone() });
    }
    let mut receipt = finished_plan(&place, &key, &finished, StepState::Done);
    receipt.steps.push(Entry::new(
        2,
        Step::UndoPlan {
            plan: pending.clone(),
        },
    ));
    Receipts::at(&place.output).save(&mut receipt).unwrap();
    assert_eq!(receipt.state, CleanupState::Incomplete);
    let hands = Fake::at(&place.root);

    let result = place.apply(&hands, &own(&key)).await;
    assert_eq!(result["ok"], true);
    assert_eq!(hands.logged(), [format!("undo {pending}")]);
    let saved = Receipts::at(&place.output).load(&receipt.id).unwrap();
    assert!(
        saved.steps[0].settled,
        "the reconciled first step must not revert to its stale snapshot"
    );
    assert!(saved.steps[1].settled);
    assert!(
        saved
            .steps
            .iter()
            .all(|entry| entry.state == StepState::Done)
    );
}
