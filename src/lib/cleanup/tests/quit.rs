//! Which apps a cleanup quits: those running from what it removes, chosen
//! by their folder, never by name, and never one opened by name.
use super::*;

#[tokio::test]
async fn apps_opened_by_name_are_listed_and_never_quit() {
    let place = Place::new();
    let key = session("launch");
    let task = default_task(&key);
    place.claim(
        &task,
        Owned::Launch {
            app: "TextEdit".into(),
        },
    );
    let fake = Fake::at(&place.root);
    fake.running(
        812,
        "TextEdit",
        "/System/Applications/TextEdit.app/Contents/MacOS/TextEdit",
    );

    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["steps"], json!([]));
    assert_eq!(
        reply["left"],
        json!([{"app": "TextEdit", "why": "opened by name"}])
    );
    assert!(fake.logged().is_empty());
}

#[tokio::test]
async fn an_app_opened_by_name_that_no_longer_runs_is_not_left() {
    let place = Place::new();
    let key = session("quit-since");
    let task = default_task(&key);
    let install = Token::new();
    for owned in [
        Owned::Launch {
            app: "Probe".into(),
        },
        Owned::Launch {
            app: "TextEdit".into(),
        },
        Owned::Install {
            id: install.clone(),
            app: "Probe".into(),
        },
    ] {
        place.claim(&task, owned);
    }
    let fake = Fake::at(&place.root);
    fake.running(
        812,
        "textedit",
        "/System/Applications/TextEdit.app/Contents/MacOS/TextEdit",
    );
    let left = json!([{"app": "TextEdit", "why": "opened by name"}]);

    let scope = own(&key);
    let preview = preview(&fake, &place.output, &scope, &place.loaded(&scope))
        .await
        .unwrap();
    assert_eq!(preview["left"], left, "Probe is not running");
    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["left"], left);
    let nothing = place.apply(&fake, &own(&key)).await;
    assert_eq!(nothing["steps"], json!([]));
    assert_eq!(nothing["left"], left, "said with nothing left to do too");
}

#[tokio::test]
async fn processes_are_chosen_by_their_folder_never_by_name() {
    let place = Place::new();
    let key = session("folder");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let fake = Fake::at(&place.root);
    let inside = place.folder(&copy).join("Probe.app/Contents/MacOS/Probe");
    fake.running(812, "Probe", &inside.to_string_lossy());
    fake.running(900, "Probe", "/Applications/Probe.app/Contents/MacOS/Probe");
    let beside = format!("{}-other/Probe", place.folder(&copy).display());
    fake.running(901, "Other", &beside);

    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(fake.logged(), ["quit 812"]);
    assert_eq!(states(&reply)[0], "done");
}

#[tokio::test]
async fn nothing_runs_from_a_foreign_copy_or_a_replaced_install_is_quit() {
    let place = Place::new();
    let (key, other) = (session("quitter"), session("neighbour"));
    let task = default_task(&key);
    let theirs = place.copy(Some(&other));
    place.claim(&task, Owned::Copy { id: theirs.clone() });
    let install = Token::new();
    place.claim(
        &task,
        Owned::Install {
            id: install.clone(),
            app: "Probe".into(),
        },
    );
    let fake = Fake::at(&place.root);
    let inside = place
        .folder(&theirs)
        .join("TextEdit.app/Contents/MacOS/TextEdit");
    fake.running(812, "TextEdit", &inside.to_string_lossy());
    // The helper no longer finds its own folder at the install's place:
    // what runs there is someone else's.
    let replaced = "/Users/helper/Applications/Probe.app/Contents/MacOS/Probe";
    fake.running(813, "Probe", replaced);
    fake.installed.borrow_mut().insert(
        install.clone(),
        json!({"installId": install, "phase": "installed", "app": "Probe", "verified": false}),
    );
    fake.removals.borrow_mut().insert(
        install.clone(),
        json!({"ok": true, "outcome": "kept", "reason": "replaced"}),
    );

    let reply = place.apply(&fake, &own(&key)).await;
    assert!(
        fake.logged().iter().all(|call| !call.starts_with("quit")),
        "{:?}",
        fake.logged()
    );
    assert_eq!(reply["steps"][0]["state"], "done");
    assert!(place.folder(&theirs).exists());
    assert_eq!(fake.apps.borrow().len(), 2, "both still run");
}

#[tokio::test]
async fn an_app_running_from_a_copy_macos_cannot_trace_is_left_never_quit() {
    let place = Place::new();
    let key = session("translocated");
    let task = default_task(&key);
    let install = Token::new();
    place.claim(
        &task,
        Owned::Install {
            id: install.clone(),
            app: "Probe".into(),
        },
    );
    let fake = Fake::at(&place.root);
    let mut answer = installed(&install, &[]);
    answer["unproven"] = json!([{"pid": 900, "executable": "/private/var/folders/x/AppTranslocation/1/d/Probe.app/Contents/MacOS/Probe"}]);
    fake.installed.borrow_mut().insert(install.clone(), answer);
    fake.removals
        .borrow_mut()
        .insert(install.clone(), json!({"ok": true, "outcome": "inUse"}));

    let reply = place.apply(&fake, &own(&key)).await;
    assert!(fake.logged().iter().all(|call| !call.starts_with("quit")));
    assert_eq!(
        reply["left"],
        json!([{"app": "Probe", "why": "runs from a copy macOS moved it to"}])
    );
    assert_eq!(reply["steps"][1]["reason"], "in-use");
}

#[tokio::test]
async fn new_work_after_a_failed_quit_waits_for_every_quit() {
    let place = Place::new();
    let key = session("rewire");
    let task = default_task(&key);
    let first = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: first.clone() });
    let plan = plan_claimed(&place, &task);
    let mut slow = Fake::at(&place.root);
    slow.slow = true;
    let running = place
        .folder(&first)
        .join("TextEdit.app/Contents/MacOS/TextEdit");
    slow.running(812, "TextEdit", &running.to_string_lossy());
    let scope = own(&key);
    let earlier = place.apply(&slow, &scope).await;
    assert_eq!(states(&earlier), ["failed", "failed", "failed"]);

    let second = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: second.clone() });
    let mut still_slow = Fake::at(&place.root);
    still_slow.slow = true;
    still_slow.running(812, "TextEdit", &running.to_string_lossy());
    let reply = place.apply(&still_slow, &scope).await;

    let receipt = place.receipt(&reply);
    let quits: Vec<u32> = receipt
        .steps
        .iter()
        .filter(|entry| matches!(entry.step, Step::Quit { .. }))
        .map(|entry| entry.n)
        .collect();
    assert_eq!(quits.len(), 2);
    for entry in receipt
        .steps
        .iter()
        .filter(|entry| !matches!(entry.step, Step::Quit { .. }))
    {
        assert_eq!(entry.after, quits, "{:?} waits for both quits", entry.step);
    }
    assert!(!still_slow.logged().contains(&format!("undo {plan}")));
}
