//! The order a cleanup runs its steps in, and how a run that was cut,
//! stopped or failed partway is resumed, reported and settled.
use super::*;

#[tokio::test]
async fn steps_run_quit_then_plans_newest_first_then_installs_then_copies() {
    let place = Place::new();
    let key = session("order");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let install = Token::new();
    place.claim(
        &task,
        Owned::Install {
            id: install.clone(),
            app: "Probe".into(),
        },
    );
    let older = plan_claimed(&place, &task);
    let newer = plan_claimed(&place, &task);
    let fake = Fake::at(&place.root);
    let executable = place
        .folder(&copy)
        .join("TextEdit.app/Contents/MacOS/TextEdit");
    fake.running(812, "TextEdit", &executable.to_string_lossy());
    fake.installed.borrow_mut().insert(
        install.clone(),
        installed(
            &install,
            &[(
                813,
                "/Users/helper/Applications/Probe.app/Contents/MacOS/Probe",
            )],
        ),
    );
    fake.running(
        813,
        "Probe",
        "/Users/helper/Applications/Probe.app/Contents/MacOS/Probe",
    );

    let reply = place.apply(&fake, &own(&key)).await;

    assert_eq!(
        kinds(&reply),
        [
            "quit",
            "undo-plan",
            "undo-plan",
            "remove-install",
            "remove-copy"
        ]
    );
    assert_eq!(reply["steps"][1]["planId"], newer.as_str());
    assert_eq!(reply["steps"][2]["planId"], older.as_str());
    assert_eq!(reply["steps"][3]["installId"], install.as_str());
    assert_eq!(reply["steps"][3]["app"], "Probe");
    let receipt = place.receipt(&reply);
    assert_eq!(
        receipt.steps[3].after,
        [1, 2, 3],
        "after the quit and both plans"
    );
    assert_eq!(
        fake.logged(),
        [
            "quit 813 812".to_owned(),
            format!("undo {newer}"),
            format!("undo {older}"),
            format!("remove {install}"),
        ]
    );
    assert_eq!(reply["state"], "complete");
    assert_eq!(reply["ok"], true);
    assert!(!place.folder(&copy).exists());
    let record = Tasks::at(&place.output).load(task.key()).unwrap();
    assert!(record.claims().all(|(_, state)| state == State::Gone));
    assert!(!record.holds_anything());
}

#[tokio::test]
async fn a_cleanup_cut_between_steps_resumes_from_its_receipt_without_listing_any_folder() {
    let place = Place::new();
    let key = session("cut");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let plan = plan_claimed(&place, &task);
    // Folders the records do not name are never looked at, let alone
    // removed.
    let stranger = place.copy(Some(&key));

    let mut first = Fake::at(&place.root);
    first.hangs = Some(plan.clone());
    let executable = place
        .folder(&copy)
        .join("TextEdit.app/Contents/MacOS/TextEdit");
    first.running(812, "TextEdit", &executable.to_string_lossy());
    let scope = own(&key);
    let cut = tokio::time::timeout(
        std::time::Duration::from_millis(200),
        place.apply(&first, &scope),
    )
    .await;
    assert!(cut.is_err(), "the run was dropped while it waited");

    let note = Tasks::at(&place.output)
        .load(task.key())
        .unwrap()
        .unfinished_cleanup()
        .cloned()
        .unwrap();
    let written = Receipts::at(&place.output).load(&note.id).unwrap();
    assert_eq!(written.steps[0].state, StepState::Done);
    assert_eq!(written.steps[1].state, StepState::Started);

    let second = Fake::at(&place.root);
    let reply = place.apply(&second, &scope).await;
    assert_eq!(reply["receipt"], note.id.as_str(), "the same receipt");
    assert_eq!(second.logged(), [format!("undo {plan}")]);
    assert_eq!(reply["state"], "complete");
    assert!(place.folder(&stranger).exists());
}

#[tokio::test]
async fn a_removal_that_finished_before_the_crash_counts_as_done_not_as_a_failure() {
    let place = Place::new();
    let key = session("finished");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let fake = Fake::at(&place.root);

    // The step was written started, then its folder went, then the run
    // died before it wrote what happened.
    let scope = own(&key);
    let loaded = place.loaded(&scope);
    let mut receipt = Receipt::new(key.clone(), None, Actor::Agent);
    plan::merge(&mut receipt.steps, &loaded.undoable());
    let removal = receipt.steps.len() - 1;
    receipt.steps[removal].set(StepState::Started, None);
    for step in &mut receipt.steps[..removal] {
        step.set(StepState::Done, None);
    }
    Receipts::at(&place.output).save(&mut receipt).unwrap();
    note(
        &Tasks::at(&place.output),
        &loaded.covered(&scope),
        &receipt,
        None,
    )
    .unwrap();
    fs::remove_dir_all(place.folder(&copy)).unwrap();

    let reply = place.apply(&fake, &scope).await;
    let step = &reply["steps"][removal];
    assert_eq!(step["state"], "done");
    assert_eq!(step["alreadyGone"], true);
    assert_eq!(reply["ok"], true);
}

#[tokio::test]
async fn a_step_waiting_on_a_failed_step_is_not_run() {
    let place = Place::new();
    let key = session("waiting");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let plan = plan_claimed(&place, &task);
    let mut fake = Fake::at(&place.root);
    fake.slow = true;
    let executable = place
        .folder(&copy)
        .join("TextEdit.app/Contents/MacOS/TextEdit");
    fake.running(812, "TextEdit", &executable.to_string_lossy());

    let reply = place.apply(&fake, &own(&key)).await;

    assert_eq!(states(&reply), ["failed", "failed", "failed"]);
    assert_eq!(reply["steps"][0]["reason"], "timeout");
    assert_eq!(reply["steps"][1]["reason"], "needs-step");
    assert_eq!(reply["steps"][2]["reason"], "needs-step");
    assert_eq!(reply["steps"][1]["retry"], true);
    assert!(!fake.logged().contains(&format!("undo {plan}")));
    assert!(place.folder(&copy).exists());
    assert_eq!(
        (&reply["ok"], &reply["code"]),
        (&json!(false), &json!("cleanup"))
    );
    assert_eq!(reply["uncertain"], false);
}

#[tokio::test]
async fn an_uncertain_step_ends_the_run_and_is_never_taken_for_a_failure() {
    let place = Place::new();
    let key = session("uncertain");
    let task = default_task(&key);
    let older = plan_claimed(&place, &task);
    let newer = plan_claimed(&place, &task);
    let fake = Fake::at(&place.root);
    fake.undo.borrow_mut().insert(
        newer.clone(),
        Err(Error::new(Code::Transport, "The reply was lost").uncertain()),
    );

    let reply = place.apply(&fake, &own(&key)).await;

    assert_eq!(states(&reply), ["uncertain", "pending"]);
    assert_eq!(fake.logged(), [format!("undo {newer}")]);
    assert_eq!(
        (&reply["state"], &reply["uncertain"]),
        (&json!("uncertain"), &json!(true))
    );
    let record = Tasks::at(&place.output).load(task.key()).unwrap();
    assert!(record.claims().any(
        |(owned, state)| owned.same(&Owned::Plan { id: older.clone() }) && state == State::Made
    ));
    assert_eq!(
        record.unfinished_cleanup().map(|note| note.state),
        Some(CleanupState::Uncertain)
    );
}

#[tokio::test]
async fn a_stop_between_steps_blocks_the_rest() {
    let place = Place::new();
    let key = session("stopped");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    plan_claimed(&place, &task);
    let mut fake = Fake::at(&place.root);
    fake.stop_after_undo = true;

    let reply = place.apply(&fake, &own(&key)).await;

    assert_eq!(states(&reply), ["done", "done", "blocked"]);
    assert_eq!(reply["steps"][2]["reason"], "stopped");
    assert_eq!(reply["state"], "blocked");
    assert!(place.folder(&copy).exists());
}

#[tokio::test]
async fn kept_and_failed_are_reported_apart() {
    let place = Place::new();
    let key = session("kept");
    let task = default_task(&key);
    let plan = plan_claimed(&place, &task);
    let fake = Fake::at(&place.root);
    fake.undo.borrow_mut().insert(
        plan.clone(),
        Ok(Undone::Kept {
            targets: vec![".config/probe/settings.toml".into()],
            directories: 0,
        }),
    );

    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(states(&reply), ["kept"]);
    assert_eq!(reply["steps"][0]["reason"], "kept-targets");
    assert_eq!(
        reply["steps"][0]["targets"],
        json!([".config/probe/settings.toml"])
    );
    assert_eq!(
        (&reply["ok"], &reply["state"]),
        (&json!(true), &json!("complete"))
    );
    let record = Tasks::at(&place.output).load(task.key()).unwrap();
    assert!(record.claims().all(|(_, state)| state == State::Kept));

    let other = session("failed");
    let task = default_task(&other);
    let copy = place.copy(Some(&other));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let fake = Fake::at(&place.root);
    // An app that runs by the copy's name keeps it in use.
    fake.running(
        9,
        "TextEdit",
        "/System/Applications/TextEdit.app/Contents/MacOS/TextEdit",
    );
    let reply = place.apply(&fake, &own(&other)).await;
    assert_eq!(states(&reply), ["done", "failed"]);
    assert_eq!(reply["steps"][1]["reason"], "in-use");
    assert_eq!(reply["state"], "incomplete");
    assert_eq!(fake.logged(), Vec::<String>::new(), "nothing ran from it");
}

#[tokio::test]
async fn new_work_after_an_unfinished_cleanup_is_added_to_the_same_receipt() {
    let place = Place::new();
    let key = session("more");
    let task = default_task(&key);
    let first = plan_claimed(&place, &task);
    let fake = Fake::at(&place.root);
    fake.undo
        .borrow_mut()
        .insert(first.clone(), Ok(Undone::Unfinished("Busy".into())));
    let scope = own(&key);
    let earlier = place.apply(&fake, &scope).await;
    assert_eq!(earlier["state"], "incomplete");

    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let later = plan_claimed(&place, &task);
    let fake = Fake::at(&place.root);
    let reply = place.apply(&fake, &scope).await;

    assert_eq!(reply["receipt"], earlier["receipt"]);
    assert_eq!(
        kinds(&reply),
        ["quit", "undo-plan", "undo-plan", "remove-copy"]
    );
    assert_eq!(reply["steps"][1]["planId"], later.as_str(), "newest first");
    let receipt = place.receipt(&reply);
    let numbers: Vec<u32> = receipt.steps.iter().map(|step| step.n).collect();
    assert_eq!(numbers, [2, 3, 1, 4], "the first step keeps its number");
    assert_eq!(reply["state"], "complete");
}

#[tokio::test]
async fn a_preview_changes_nothing_and_takes_no_lock() {
    let place = Place::new();
    let key = session("preview");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let plan = plan_claimed(&place, &task);
    let before = fs::read(place.output.join(format!("tasks/{}.json", task.key()))).unwrap();
    let fake = Fake::at(&place.root);
    let executable = place
        .folder(&copy)
        .join("TextEdit.app/Contents/MacOS/TextEdit");
    fake.running(812, "TextEdit", &executable.to_string_lossy());

    let scope = own(&key);
    let reply = preview(&fake, &place.output, &scope, &place.loaded(&scope))
        .await
        .unwrap();

    assert_eq!(reply["preview"], true);
    assert_eq!(kinds(&reply), ["quit", "undo-plan", "remove-copy"]);
    assert_eq!(
        reply["steps"][0]["apps"],
        json!([{"app": "TextEdit", "pid": 812}])
    );
    assert_eq!(reply["steps"][1]["planId"], plan.as_str());
    assert_eq!(reply["steps"][1]["planState"], "applied");
    assert_eq!(reply["steps"][2]["present"], true);
    assert!(reply["steps"][0].get("state").is_none(), "nothing ran yet");
    assert_eq!(fake.logged(), ["plan-states"], "nothing was quit or undone");
    assert!(place.folder(&copy).exists());
    assert!(!place.output.join("cleanup").exists(), "no receipt");
    assert_eq!(
        fs::read(place.output.join(format!("tasks/{}.json", task.key()))).unwrap(),
        before
    );
}

#[tokio::test]
async fn a_receipt_names_no_file_on_this_mac() {
    let place = Place::new();
    let key = session("names");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let plan = plan_claimed(&place, &task);
    let fake = Fake::at(&place.root);
    fake.undo.borrow_mut().insert(
        plan,
        Ok(Undone::Kept {
            targets: vec![".config/probe/settings.toml".into()],
            directories: 1,
        }),
    );

    let reply = place.apply(&fake, &own(&key)).await;
    let id = reply["receipt"].as_str().unwrap();
    let bytes = fs::read_to_string(place.output.join(format!("cleanup/{id}.json"))).unwrap();
    // Paths inside the helper's home are kept; nothing of this Mac is.
    assert!(bytes.contains(".config/probe/settings.toml"));
    for named in ["Secret", "rtf", "/copies", "/Users/"] {
        assert!(!bytes.contains(named), "{named} in {bytes}");
    }
}

#[tokio::test]
async fn a_kept_step_is_settled_in_the_records_of_its_own_receipts_task() {
    let place = Place::new();
    let key = session("two-tasks");
    let (first, second) = (
        Task::of(&key, Some("first")).unwrap(),
        Task::of(&key, Some("second")).unwrap(),
    );
    let theirs = place.copy(Some(&session("elsewhere")));
    place.claim(&first, Owned::Copy { id: theirs.clone() });
    let scope_of = |task: &Task| Scope {
        session: key.clone(),
        task: Some(task.key().clone()),
        adopting: false,
        named: true,
    };
    let fake = Fake::at(&place.root);
    let kept = place.apply(&fake, &scope_of(&first)).await;
    assert_eq!(kept["steps"][1]["reason"], "another-session");

    // As a crash between the kept outcome and settling it leaves things.
    let mut receipt = place.receipt(&kept);
    for entry in &mut receipt.steps {
        entry.settled = false;
    }
    Receipts::at(&place.output).save(&mut receipt).unwrap();
    place.claim(&first, Owned::Copy { id: theirs.clone() });
    let later = plan_claimed(&place, &second);

    let fake = Fake::at(&place.root);
    place.apply(&fake, &scope_of(&second)).await;
    let first_record = Tasks::at(&place.output).load(first.key()).unwrap();
    assert!(
        first_record.claims().any(
            |(owned, state)| owned.same(&Owned::Copy { id: theirs.clone() })
                && state == State::Kept
        ),
        "settled where it was claimed"
    );
    let second_record = Tasks::at(&place.output).load(second.key()).unwrap();
    assert!(
        !second_record
            .claims()
            .any(|(owned, _)| owned.same(&Owned::Copy { id: theirs.clone() }))
    );
    assert!(second_record.claims().any(|(owned, state)| {
        owned.same(&Owned::Plan { id: later.clone() }) && state == State::Gone
    }));
    let settled = Receipts::at(&place.output).load(&receipt.id).unwrap();
    assert!(settled.steps.iter().all(|entry| entry.settled));
}

#[tokio::test]
async fn a_step_finished_before_its_claim_was_settled_is_settled_not_planned_again() {
    let place = Place::new();
    let key = session("settle");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let scope = own(&key);
    let fake = Fake::at(&place.root);
    let first = place.apply(&fake, &scope).await;
    assert_eq!(first["state"], "complete");

    // As a crash between the receipt's last write and settling leaves it:
    // the receipt says done, the claim still says made.
    let mut receipt = place.receipt(&first);
    for entry in &mut receipt.steps {
        entry.settled = false;
    }
    Receipts::at(&place.output).save(&mut receipt).unwrap();
    place.claim(&task, Owned::Copy { id: copy.clone() });

    let fake = Fake::at(&place.root);
    let second = place.apply(&fake, &scope).await;
    assert_eq!(second["steps"], json!([]), "nothing planned again");
    assert!(fake.logged().is_empty());
    let record = Tasks::at(&place.output).load(task.key()).unwrap();
    assert!(record.claims().all(|(_, state)| state == State::Gone));
    let settled = Receipts::at(&place.output).load(&receipt.id).unwrap();
    assert!(settled.steps.iter().all(|entry| entry.settled));
}

#[tokio::test]
async fn what_a_finished_step_came_to_survives_a_later_run() {
    let place = Place::new();
    let key = session("kept-later");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let plan = plan_claimed(&place, &task);
    let fake = Fake::at(&place.root);
    fake.undo.borrow_mut().insert(
        plan,
        Ok(Undone::Kept {
            targets: vec![".config/probe/settings.toml".into()],
            directories: 0,
        }),
    );
    fake.running(
        9,
        "TextEdit",
        "/System/Applications/TextEdit.app/Contents/MacOS/TextEdit",
    );
    let scope = own(&key);
    let first = place.apply(&fake, &scope).await;
    assert_eq!(first["state"], "incomplete");

    let later = Fake::at(&place.root);
    let second = place.apply(&later, &scope).await;
    assert_eq!(second["state"], "complete");
    assert_eq!(
        second["steps"][1]["targets"],
        json!([".config/probe/settings.toml"]),
        "{second}"
    );
}
