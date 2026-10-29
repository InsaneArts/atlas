//! Removing installs: each is moved aside in the helper by its path, one
//! that changed is kept, one in use is tried again, and one removed by hand
//! is said to be gone already.
use super::*;

#[tokio::test]
async fn an_install_that_changed_is_kept_and_one_in_use_is_tried_again() {
    let place = Place::new();
    let key = session("installs");
    let task = default_task(&key);
    let (changed, busy) = (Token::new(), Token::new());
    for id in [&changed, &busy] {
        place.claim(
            &task,
            Owned::Install {
                id: id.clone(),
                app: "Probe".into(),
            },
        );
    }
    let fake = Fake::at(&place.root);
    fake.removals.borrow_mut().insert(
        changed.clone(),
        json!({"ok": true, "outcome": "kept", "reason": "changed",
            "sideEffects": {"kept": [{"path": "Library/Preferences/test.atlas.install-probe.plist",
                "existedBefore": false}]}}),
    );
    fake.removals
        .borrow_mut()
        .insert(busy.clone(), json!({"ok": true, "outcome": "inUse"}));

    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(states(&reply), ["done", "kept", "failed"]);
    assert_eq!(reply["steps"][1]["reason"], "changed");
    assert_eq!(
        reply["steps"][1]["sideEffects"]["kept"][0]["existedBefore"],
        false
    );
    assert_eq!(
        (&reply["steps"][2]["reason"], &reply["steps"][2]["retry"]),
        (&json!("in-use"), &json!(true))
    );
    assert_eq!(reply["state"], "incomplete");
    let record = Tasks::at(&place.output).load(task.key()).unwrap();
    let state = |id: &Token| {
        record
            .claims()
            .find(|(owned, _)| {
                owned.same(&Owned::Install {
                    id: id.clone(),
                    app: String::new(),
                })
            })
            .map(|(_, state)| state)
    };
    assert_eq!(
        (state(&changed), state(&busy)),
        (Some(State::Kept), Some(State::Made))
    );
}

#[tokio::test]
async fn an_install_is_moved_aside_in_the_helper_by_its_path_and_a_copy_is_removed() {
    let place = Place::new();
    let key = session("held");
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
    let fake = Fake::at(&place.root);

    let reply = place.apply(&fake, &own(&key)).await;
    let held = held_path(&install);
    assert_eq!(reply["steps"][1]["held"], json!(held));
    assert!(reply["steps"][1].get("alreadyGone").is_none());
    assert_eq!(reply["steps"][2]["state"], "done");
    assert!(reply["steps"][2].get("held").is_none());
    assert!(!place.folder(&copy).exists(), "a copy is removed");
    let receipt = place.receipt(&reply);
    let kept: Vec<Option<&str>> = receipt
        .steps
        .iter()
        .map(|entry| entry.came_to.held.as_deref())
        .collect();
    assert_eq!(kept, [None, Some(held.as_str()), None]);
}

#[tokio::test]
async fn an_install_removed_by_hand_is_reported_already_gone() {
    let place = Place::new();
    let key = session("by-hand");
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
    fake.removals.borrow_mut().insert(
        install.clone(),
        json!({"ok": true, "installId": install, "outcome": "alreadyGone", "app": "Probe"}),
    );

    let reply = place.apply(&fake, &own(&key)).await;
    let step = &reply["steps"][1];
    assert_eq!(step["do"], "remove-install");
    assert_eq!(step["alreadyGone"], true, "{step}");
    assert!(step.get("held").is_none());
    assert_eq!(reply["ok"], true);
}
