//! Finished effects and durable record acknowledgement remain separate.
use super::*;
use crate::host_session::record::Record as SessionRecord;
use crate::task::record::Record as TaskRecord;
use atlas_core::record_folder::CLEANUP_ENTRIES;

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
                let loaded = place.loaded(&own(&key));
                let folder = place.output.join(kind);
                let faults: Vec<_> = match failure {
                    "scan" => (0..CLEANUP_ENTRIES)
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
                if kind == "tasks" && matches!(failure, "scan" | "lock") {
                    let error = Loaded::of(&place.output, &own(&key))
                        .err()
                        .expect("Incomplete discovery must refuse before cleanup");
                    assert_eq!(
                        error.code,
                        if failure == "scan" {
                            Code::State
                        } else {
                            Code::Io
                        }
                    );
                    assert!(!error.uncertain);
                }
                // A fault can also arrive after discovery. Keep exercising
                // acknowledgement itself with the original loaded receipt.
                reconcile(
                    &Receipts::at(&place.output),
                    &Tasks::at(&place.output),
                    &Sessions::at(&place.output),
                    &loaded,
                )
                .unwrap();
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
                Tasks::at(&place.output)
                    .settle_everywhere_checked(&Owned::Plan { id: plan.clone() }, State::Gone, 1)
                    .unwrap_err()
                    .code,
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

#[tokio::test]
async fn a_finished_cleanup_in_a_315_entry_task_folder_is_acknowledged_without_repeating_it() {
    for state in [StepState::Done, StepState::Kept] {
        let place = Place::new();
        let key = session("large-settlement-folder");
        let task = default_task(&key);
        let plan = Token::new();
        place.claim(&task, Owned::Plan { id: plan.clone() });
        session_plan(&place, &key, &plan);
        let receipt = finished_plan(&place, &key, &plan, state);
        let mut added = 0;
        for n in 0..314 {
            let other = Key::parse(&format!("{n:012x}")).unwrap();
            if other == *task.key() {
                continue;
            }
            let mut record = TaskRecord::new(&task, 1);
            record.key = other;
            fs::write(
                place.output.join(format!("tasks/{}.json", record.key)),
                serde_json::to_vec(&record).unwrap(),
            )
            .unwrap();
            added += 1;
            if added == 313 {
                break;
            }
        }
        assert_eq!(
            fs::read_dir(place.output.join("tasks")).unwrap().count(),
            315
        );
        let hands = Fake::at(&place.root);
        let reply = place.apply(&hands, &own(&key)).await;
        assert_eq!(reply["ok"], true, "{reply}");
        assert!(
            hands.logged().is_empty(),
            "the finished effect must not run again"
        );
        let saved = Receipts::at(&place.output).load(&receipt.id).unwrap();
        assert_eq!(saved.steps[0].at, receipt.steps[0].at);
        assert_eq!(saved.steps[0].state, state);
        assert!(
            saved.steps[0].settled,
            "{state:?}: complete record proof must acknowledge the finished effect"
        );
        let expected = if state == StepState::Done {
            State::Gone
        } else {
            State::Kept
        };
        assert!(
            Tasks::at(&place.output)
                .load(task.key())
                .unwrap()
                .claims()
                .all(|(_, state)| state == expected)
        );
        assert!(
            Sessions::at(&place.output)
                .read(&key)
                .unwrap()
                .plans
                .is_empty()
        );
    }
}

#[test]
fn cleanup_refreshes_matching_claim_timestamps_but_never_rewrites_nonmatching_records() {
    for state in [State::Gone, State::Kept] {
        let place = Place::new();
        let key = session("settlement-timestamps");
        let task = default_task(&key);
        let plan = Owned::Plan { id: Token::new() };
        let other = Owned::Plan { id: Token::new() };
        let store = Tasks::at(&place.output);
        store
            .update(&task, 80, |record| record.claim(&plan, State::Made, 80))
            .unwrap();
        let neighbour = Task::of(&key, Some("neighbour")).unwrap();
        store
            .update(&neighbour, 90, |record| {
                record.claim(&other, State::Made, 90)
            })
            .unwrap();
        store
            .update(&task, 80, |record| {
                assert!(record.settle(&plan, state, 80));
                Ok(())
            })
            .unwrap();
        let path = place.output.join(format!("tasks/{}.json", neighbour.key()));
        let before = fs::read(&path).unwrap();
        if state == State::Gone {
            store.settle_everywhere_checked(&plan, state, 40).unwrap();
        } else {
            store
                .settle_in_scope_checked(&key, Some(task.key()), &plan, state, 40)
                .unwrap();
        }
        let refreshed = store.load(task.key()).unwrap();
        assert_eq!(
            refreshed.updated, 40,
            "timestamps are assigned, not maximized"
        );
        assert_eq!(refreshed.claimed_at(&plan), Some(40));
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}

#[test]
fn checked_plan_forgetting_reads_a_315_entry_session_folder_completely() {
    let place = Place::new();
    let key = session("many-sessions");
    let plan = Token::new();
    session_plan(&place, &key, &plan);
    let host = HostSession::new(Host::Codex, "many-sessions").unwrap();
    let folder = place.output.join("sessions");
    let mut added = 0;
    for n in 0..314 {
        let other = Key::parse(&format!("{n:012x}")).unwrap();
        if other == key {
            continue;
        }
        let mut record = SessionRecord::new(&host, 1, None);
        record.key = other;
        fs::write(
            folder.join(format!("{}.json", record.key)),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        added += 1;
        if added == 313 {
            break;
        }
    }
    // Lock creation is part of the scan; an ordinary listing still remains bounded.
    Sessions::at(&place.output)
        .forget_plan_checked(&plan)
        .unwrap();
    assert_eq!(fs::read_dir(&folder).unwrap().count(), 315);
    assert!(
        Sessions::at(&place.output)
            .read(&key)
            .unwrap()
            .plans
            .is_empty()
    );
}

#[test]
fn checked_settlement_validates_unknown_ownership_outside_the_selected_scope_before_writing() {
    for state in [State::Gone, State::Kept] {
        let place = Place::new();
        let key = session("complete-ownership-proof");
        let task = default_task(&key);
        let owned = Owned::Plan { id: Token::new() };
        place.claim(&task, owned.clone());
        for n in 0..313 {
            let mut record = TaskRecord::new(&task, 1);
            record.key = Key::parse(&format!("{n:012x}")).unwrap();
            assert_ne!(record.key, *task.key());
            fs::write(
                place.output.join(format!("tasks/{}.json", record.key)),
                serde_json::to_vec(&record).unwrap(),
            )
            .unwrap();
        }
        let mut foreign = Tasks::at(&place.output).load(task.key()).unwrap();
        foreign.key = Key::parse("ffffffffffff").unwrap();
        foreign.session = session("foreign-complete-ownership-proof");
        let path = place.output.join(format!("tasks/{}.json", foreign.key));
        let original = serde_json::to_vec(&foreign).unwrap();
        let mut unknown: Value = serde_json::from_slice(&original).unwrap();
        unknown["owned"][0]["state"] = json!("future-state");
        let preserved = serde_json::to_vec_pretty(&unknown).unwrap();
        fs::write(&path, &preserved).unwrap();
        let own_path = place.output.join(format!("tasks/{}.json", task.key()));
        let before = fs::read(&own_path).unwrap();
        let store = Tasks::at(&place.output);
        let settle = || {
            if state == State::Gone {
                store.settle_everywhere_checked(&owned, state, 2)
            } else {
                store.settle_in_scope_checked(&key, Some(task.key()), &owned, state, 2)
            }
        };
        assert!(settle().is_err());
        assert_eq!(fs::read(&own_path).unwrap(), before);
        assert_eq!(fs::read(&path).unwrap(), preserved);
        fs::write(&path, original).unwrap();
        settle().unwrap();
        assert!(
            store
                .load(task.key())
                .unwrap()
                .claims()
                .all(|(_, found)| found == state)
        );
        let other_state = if state == State::Gone {
            State::Gone
        } else {
            State::Made
        };
        assert!(
            store
                .load(&foreign.key)
                .unwrap()
                .claims()
                .all(|(_, found)| found == other_state)
        );
    }
}

#[test]
fn task_cleanup_writes_newest_matching_records_first_and_keeps_scoped_neighbours() {
    for scope in [None, Some("first")] {
        let place = Place::new();
        let key = session("settlement-write-order");
        let owned = Owned::Plan { id: Token::new() };
        let tasks: Vec<_> = ["first", "second", "third"]
            .into_iter()
            .map(|name| Task::of(&key, Some(name)).unwrap())
            .collect();
        for (index, task) in tasks.iter().enumerate() {
            Tasks::at(&place.output)
                .update(task, 1, |record| record.claim(&owned, State::Made, 1))
                .unwrap();
            let path = place.output.join(format!("tasks/{}.json", task.key()));
            fs::File::open(path)
                .unwrap()
                .set_times(fs::FileTimes::new().set_modified(
                    std::time::SystemTime::UNIX_EPOCH
                        + std::time::Duration::from_secs(300 - index as u64),
                ))
                .unwrap();
        }
        let second = place.output.join(format!("tasks/{}.json", tasks[1].key()));
        let partial = second.with_extension("json.partial");
        fs::create_dir(&partial).unwrap();
        let store = Tasks::at(&place.output);
        if scope.is_none() {
            assert!(
                store
                    .settle_everywhere_checked(&owned, State::Gone, 40)
                    .is_err()
            );
        } else {
            store
                .settle_in_scope_checked(&key, Some(tasks[0].key()), &owned, State::Kept, 40)
                .unwrap();
        }
        let expected = if scope.is_none() {
            State::Gone
        } else {
            State::Kept
        };
        assert!(
            store
                .load(tasks[0].key())
                .unwrap()
                .claims()
                .all(|(_, found)| found == expected)
        );
        for task in &tasks[1..] {
            let untouched = store.load(task.key()).unwrap();
            assert_eq!(untouched.updated, 1);
            assert!(untouched.claims().all(|(_, found)| found == State::Made));
        }
        fs::remove_dir(partial).unwrap();
    }
}

#[test]
fn session_cleanup_preserves_last_seen_then_modified_write_order() {
    for same_seen in [false, true] {
        let place = Place::new();
        let plan = Token::new();
        let host = HostSession::new(Host::Codex, "session-write-order").unwrap();
        let folder = place.output.join("sessions");
        fs::DirBuilder::new().mode(0o700).create(&folder).unwrap();
        let first = Key::parse("000000000001").unwrap();
        let second = Key::parse("000000000002").unwrap();
        for (key, seen, modified) in [
            (&first, 20, 100),
            (&second, if same_seen { 20 } else { 10 }, 300),
        ] {
            let mut record = SessionRecord::new(&host, 1, None);
            record.key = key.clone();
            record.last_seen = seen;
            record.planned(plan.clone(), crate::environment::Phase::Applied, 1);
            let path = folder.join(format!("{key}.json"));
            fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
            fs::File::open(path)
                .unwrap()
                .set_times(fs::FileTimes::new().set_modified(
                    std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(modified),
                ))
                .unwrap();
        }
        let (earlier, later) = if same_seen {
            (&second, &first)
        } else {
            (&first, &second)
        };
        fs::create_dir(folder.join(format!("{later}.json.partial"))).unwrap();
        let store = Sessions::at(&place.output);
        assert!(store.forget_plan_checked(&plan).is_err());
        assert!(store.read(earlier).unwrap().plans.is_empty());
        assert_eq!(store.read(earlier).unwrap().last_seen, 20);
        assert_eq!(store.read(later).unwrap().plans.len(), 1);
    }
}

#[test]
fn checked_settlement_of_missing_folders_creates_no_storage() {
    let place = Place::new();
    let id = Token::new();
    let owned = Owned::Plan { id: id.clone() };
    Tasks::at(&place.output)
        .settle_everywhere_checked(&owned, State::Gone, 1)
        .unwrap();
    Sessions::at(&place.output)
        .forget_plan_checked(&id)
        .unwrap();
    assert!(!place.output.join("tasks").exists());
    assert!(!place.output.join("sessions").exists());
}
