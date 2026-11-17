use super::*;
use crate::vm::{
    lifecycle::{barrier::Barrier, record::State},
    removing::{
        Which,
        tests::{SECOND_LOGIN, SPARE, THIRD, WORK, World, change, id, vm_entry},
    },
};
use std::cell::RefCell;

/// Ask for the uninstall's VM phase, as `atlas uninstall` does; what it
/// said is kept.
fn uninstall(world: &World) -> (Result<Option<Vec<Value>>>, Vec<String>) {
    let (removed, said) = uninstall_holding(world, &|_| {});
    (
        removed.map(|removed| removed.map(|removed| removed.vms)),
        said,
    )
}

/// The VM phase as `uninstall` asks it, `meanwhile` run at each line it
/// says; what it came to is given whole, its exclusion held.
fn uninstall_holding(
    world: &World,
    meanwhile: &dyn Fn(&str),
) -> (Result<Option<Removed>>, Vec<String>) {
    world.removes.set(0);
    let said = RefCell::new(Vec::new());
    let removed = remove_every_with(&world.lifecycle(), world, &world.settings(), &|line| {
        said.borrow_mut().push(line.to_owned());
        meanwhile(line);
    });
    (removed, said.into_inner())
}

fn pending_of(world: &World) -> Pending {
    pending_in(&world.lifecycle(), &world.settings()).unwrap()
}

/// A VM only kept on disk: its folder, as a run leaves it, and nothing
/// else of it.
fn kept_only(world: &World, vm: &str) {
    let folder = world.vms.folder(&id(vm));
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("Disk.asif"), "disk").unwrap();
}

fn barrier(world: &World) -> Option<Barrier> {
    world.lifecycle().barrier().unwrap()
}

fn mixed() -> String {
    format!(
        "default = \"second-login\"\n{SECOND_LOGIN}{}{}",
        vm_entry("work", WORK),
        vm_entry("spare", SPARE)
    )
}

#[test]
fn nothing_is_touched_without_the_persons_grant() {
    let world = World::new();
    world.write_settings(&mixed());
    world.vm("work", WORK, State::Created, true);
    world.allows.set(false);
    let before = world.read_settings();

    let (refused, said) = uninstall(&world);

    assert_eq!(refused.unwrap_err().code, Code::Denied);
    let asked = world.operations.borrow();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].kind, grant::Kind::Uninstall);
    assert_eq!(
        asked[0].settings,
        atlas_core::config::resolve_path(&world.settings()).unwrap()
    );
    assert!(said[0].starts_with("Asking you in Atlas.app"), "{said:?}");
    assert_eq!(barrier(&world), None, "no barrier before the grant");
    assert_eq!(world.removes.get(), 0);
    assert_eq!(world.record(WORK).unwrap().state, State::Created);
    assert!(world.vms.folder(&id(WORK)).join("Disk.asif").exists());
    assert_eq!(world.read_settings(), before);
}

#[test]
fn with_no_vm_the_person_is_not_asked_and_nothing_is_made() {
    for settings in [
        None,
        Some(format!("default = \"second-login\"\n{SECOND_LOGIN}")),
    ] {
        let world = World::new();
        if let Some(settings) = &settings {
            world.write_settings(settings);
        }

        assert_eq!(pending_of(&world), Pending::Nothing);
        let (removed, said) = uninstall(&world);

        assert_eq!(removed.unwrap(), None);
        assert!(said.is_empty(), "{said:?}");
        assert!(world.operations.borrow().is_empty());
        let station = world
            .home
            .path()
            .join("Library/Application Support/Atlas/Station");
        assert!(!station.exists(), "no barrier or lifecycle folder made");
    }
}

#[test]
fn the_inventory_finds_listed_kept_only_and_lifecycle_only_vms_and_removes_each() {
    let world = World::new();
    world.write_settings(&format!(
        "default = \"second-login\"\n{SECOND_LOGIN}{}",
        vm_entry("work", WORK)
    ));
    world.vm("work", WORK, State::Created, true);
    kept_only(&world, SPARE);
    // An add never published: its record and name, nothing else.
    world.vm("third", THIRD, State::Adding, false);
    world.runner_forgets();
    let before = world.read_settings();

    assert_eq!(pending_of(&world), Pending::Ask);
    let (removed, _) = uninstall(&world);

    let asked = world.operations.borrow();
    assert_eq!(
        asked[0].name,
        format!("work, third and {SPARE}"),
        "every VM named"
    );
    assert!(!asked[0].cancel);
    drop(asked);
    let removed = removed.unwrap().unwrap();
    let names: Vec<&str> = removed
        .iter()
        .map(|done| done["removed"]["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["work", "third", SPARE], "{removed:?}");
    for vm in [WORK, SPARE, THIRD] {
        assert!(!world.vms.folder(&id(vm)).exists(), "{vm}");
        assert_eq!(world.record(vm), None, "{vm}");
        let recorded = removal::recorded(&world.vms, &id(vm)).unwrap().unwrap();
        assert!(recorded.done, "{vm}");
        assert_eq!(recorded.change, change(), "under the uninstall's change");
    }
    assert!(world.names().is_empty());
    assert_eq!(world.grants.get(), 1);
    assert_eq!(
        world.read_settings(),
        before,
        "the settings left as they are"
    );
    assert_eq!(
        barrier(&world),
        Some(Barrier {
            change: change(),
            phase: Phase::Committed,
            vms: vec![id(WORK), id(THIRD), id(SPARE)],
        })
    );
    // The settings still list the VM, but nothing of it is left: asked
    // again, there is nothing to remove and no one to ask.
    assert_eq!(pending_of(&world), Pending::Nothing);
    let (again, _) = uninstall(&world);
    assert_eq!(again.unwrap(), None);
    assert_eq!(world.operations.borrow().len(), 1);
}

#[test]
fn the_barrier_refuses_a_removal_during_the_vm_phase_and_blocks_nothing_once_it_commits() {
    let world = World::new();
    world.write_settings(&mixed());
    world.vm("work", WORK, State::Created, true);
    world.vm("spare", SPARE, State::Created, true);
    world.runner_forgets();
    // Killed at its first removal.
    world.kill_at.set(Some(1));

    let (killed, _) = uninstall(&world);

    assert_eq!(killed.unwrap_err().code, Code::Interrupted);
    assert_eq!(
        barrier(&world).unwrap().phase,
        Phase::Vms,
        "the VM phase stays under way"
    );
    let (refused, _) = world.remove_which(Which::Name("spare"));
    assert!(
        refused.unwrap_err().error.contains("being uninstalled"),
        "a removal is refused at the barrier"
    );
    assert!(world.lifecycle().admit().is_err(), "and so is an add");

    world.kill_at.set(None);
    let (removed, _) = uninstall(&world);

    assert_eq!(removed.unwrap().unwrap().len(), 2);
    assert_eq!(barrier(&world).unwrap().phase, Phase::Committed);
    world.lifecycle().admit().unwrap();
}

#[test]
fn a_vm_phase_cut_short_goes_on_as_the_same_uninstall_with_no_new_grant() {
    let world = World::new();
    world.write_settings(&mixed());
    world.vm("work", WORK, State::Created, true);
    world.vm("spare", SPARE, State::Created, true);
    world.runner_forgets();
    // The first VM removed, the command killed at the second's removal.
    world.kill_at.set(Some(2));

    let (killed, _) = uninstall(&world);

    assert_eq!(killed.unwrap_err().code, Code::Interrupted);
    assert_eq!(world.record(WORK), None);
    assert_eq!(world.record(SPARE).unwrap().state, State::Removing);
    assert_eq!(pending_of(&world), Pending::Resume(change()));

    world.kill_at.set(None);
    world.allows.set(false);
    let (removed, said) = uninstall(&world);

    assert_eq!(removed.unwrap().unwrap().len(), 1, "only what was left");
    assert_eq!(world.grants.get(), 1, "no new grant");
    assert_eq!(world.operations.borrow().len(), 1, "the person not asked");
    assert!(said[0].starts_with("Going on uninstalling"), "{said:?}");
    assert_eq!(world.record(SPARE), None);
    let recorded = removal::recorded(&world.vms, &id(SPARE)).unwrap().unwrap();
    assert_eq!(recorded.change, change(), "the same uninstall's change");
    assert_eq!(barrier(&world).unwrap().phase, Phase::Committed);
}

#[test]
fn kept_paths_block_everything_after_until_they_are_moved_aside() {
    let world = World::new();
    world.write_settings(&format!("default = \"work\"\n{}", vm_entry("work", WORK)));
    world.vm("work", WORK, State::Created, true);
    world.runner_forgets();
    // A runner holds the VM at first; while the removal waits, it goes and
    // another folder takes the VM's place.
    let runner = crate::vm::try_lock(&world.vms.lifecycle(&id(WORK)))
        .unwrap()
        .unwrap();
    let runner = RefCell::new(Some(runner));
    *world.meanwhile.borrow_mut() = Some(Box::new(move |world: &World| {
        runner.borrow_mut().take();
        let folder = world.vms.folder(&id(WORK));
        std::fs::remove_dir_all(&folder).unwrap();
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("other"), "other").unwrap();
    }));
    let folder = world.vms.folder(&id(WORK)).to_string_lossy().into_owned();

    let (blocked, said) = uninstall(&world);

    let blocked = blocked.unwrap_err();
    assert_eq!(blocked.code, Code::Uninstall);
    assert_eq!(
        blocked.details["kept"],
        json!([{"vm": "work", "path": folder}]),
        "{blocked:?}"
    );
    assert!(
        said.iter()
            .any(|line| line.contains(&folder) && line.contains("never deletes")),
        "{said:?}"
    );
    assert_eq!(barrier(&world).unwrap().phase, Phase::Vms, "not committed");
    assert!(world.lifecycle().admit().is_err());

    // Asked again with the path still there: still blocked, never deleted.
    let (still, _) = uninstall(&world);
    assert_eq!(still.unwrap_err().code, Code::Uninstall);
    assert!(world.vms.folder(&id(WORK)).join("other").exists());

    let aside = world.home.path().join("moved aside");
    std::fs::rename(world.vms.folder(&id(WORK)), &aside).unwrap();
    let (removed, _) = uninstall(&world);

    assert_eq!(removed.unwrap().unwrap()[0]["kept"], json!([]));
    assert!(aside.join("other").exists(), "never deleted");
    assert_eq!(world.record(WORK), None);
    assert_eq!(barrier(&world).unwrap().phase, Phase::Committed);
    assert_eq!(world.grants.get(), 1);
}

#[test]
fn a_left_password_deleted_by_hand_lets_the_uninstall_go_on() {
    let world = World::new();
    world.write_settings(&format!("default = \"work\"\n{}", vm_entry("work", WORK)));
    world.vm("work", WORK, State::Created, true);
    // The runner cannot run, and Atlas VM's entry fails too.
    *world.fake.entry_fails.lock().unwrap() = true;
    *world.fake.passwords.lock().unwrap() = vec![WORK.to_owned(), format!("{WORK}.admin")];

    let (blocked, said) = uninstall(&world);

    let blocked = blocked.unwrap_err();
    assert_eq!(blocked.code, Code::Uninstall);
    assert_eq!(blocked.details["left"].as_array().unwrap().len(), 2);
    assert!(
        blocked.details["next"]
            .as_str()
            .unwrap()
            .contains("Keychain Access")
    );
    assert!(
        said.iter().any(|line| line.contains(&format!(
            "com.insanearts.atlas.vm {WORK}.admin of the VM \u{201c}work\u{201d}"
        ))),
        "{said:?}"
    );
    assert_eq!(barrier(&world).unwrap().phase, Phase::Vms);

    // Still there: still blocked.
    let (still, _) = uninstall(&world);
    assert_eq!(still.unwrap_err().code, Code::Uninstall);

    // The person deleted both in Keychain Access.
    world.fake.passwords.lock().unwrap().clear();
    let (removed, _) = uninstall(&world);

    assert_eq!(removed.unwrap().unwrap()[0]["left"], json!([]));
    let recorded = removal::recorded(&world.vms, &id(WORK)).unwrap().unwrap();
    assert_eq!(recorded.passwords[WORK], "gone");
    assert_eq!(recorded.passwords[&format!("{WORK}.admin")], "gone");
    assert_eq!(world.record(WORK), None);
    assert_eq!(barrier(&world).unwrap().phase, Phase::Committed);
    assert_eq!(
        world.fake.ran.lock().unwrap().len(),
        1,
        "the entry ran once"
    );
    assert_eq!(world.grants.get(), 1);
}

#[test]
fn mixed_and_vm_only_settings_are_left_exactly_as_they_were() {
    for settings in [
        mixed(),
        format!(
            "# mine\ndefault = \"work\"\n{}{}",
            vm_entry("work", WORK),
            vm_entry("spare", SPARE)
        ),
    ] {
        let world = World::new();
        world.write_settings(&settings);
        world.vm("work", WORK, State::Created, true);
        world.vm("spare", SPARE, State::Created, true);
        world.runner_forgets();

        let (removed, _) = uninstall(&world);

        assert_eq!(removed.unwrap().unwrap().len(), 2);
        assert_eq!(world.read_settings(), settings, "never an empty list");
        assert!(!world.settings().with_extension("toml.before-vm").exists());
    }
}

#[test]
fn a_removal_recorded_under_another_change_than_its_kept_one_stops_the_uninstall() {
    let world = World::new();
    world.write_settings(&format!("default = \"work\"\n{}", vm_entry("work", WORK)));
    world.vm("work", WORK, State::Created, true);
    let mut record = world.record(WORK).unwrap();
    record.removal = Some(Token::new());
    world.lifecycle().keep_record(&record).unwrap();
    crate::vm::removing::tests::begun_by_hand(&world, WORK, &"d".repeat(48));

    let (refused, _) = uninstall(&world);

    assert_eq!(refused.unwrap_err().code, Code::State);
    assert_eq!(world.removes.get(), 0);
    assert_eq!(barrier(&world).unwrap().phase, Phase::Vms);
}

fn cancel(world: &World) -> (Result<Value>, Vec<String>) {
    let said = RefCell::new(Vec::new());
    let cancelled = cancel_with(&world.lifecycle(), world, &world.settings(), &|line| {
        said.borrow_mut().push(line.to_owned());
    });
    (cancelled, said.into_inner())
}

#[test]
fn a_vm_the_person_was_not_told_of_is_never_removed() {
    let world = World::new();
    world.write_settings(&format!(
        "default = \"second-login\"\n{SECOND_LOGIN}{}",
        vm_entry("work", WORK)
    ));
    world.vm("work", WORK, State::Created, true);
    world.runner_forgets();
    // Another VM comes while the person is asked.
    *world.while_asked.borrow_mut() = Some(Box::new(|world: &World| {
        world.vm("spare", SPARE, State::Created, true);
    }));

    let (refused, _) = uninstall(&world);

    let refused = refused.unwrap_err();
    assert!(
        refused.error.contains("\u{201c}spare\u{201d} too")
            && refused.error.contains("nothing was removed"),
        "{refused:?}"
    );
    assert_eq!(world.removes.get(), 0);
    assert_eq!(
        barrier(&world).unwrap().phase,
        Phase::Cancelled,
        "called off"
    );
    world.lifecycle().admit().unwrap();
    for vm in [WORK, SPARE] {
        assert_eq!(world.record(vm).unwrap().state, State::Created);
        assert!(world.vms.folder(&id(vm)).exists());
    }

    // Asked again, the person is told of both.
    let (removed, _) = uninstall(&world);

    assert_eq!(removed.unwrap().unwrap().len(), 2);
    assert_eq!(world.operations.borrow()[1].name, "work and spare");
}

#[test]
fn a_cut_short_uninstall_never_removes_a_vm_it_was_not_allowed_to() {
    let world = World::new();
    world.write_settings(&mixed());
    world.vm("work", WORK, State::Created, true);
    // A VM phase allowed for work alone, cut short, then another VM made
    // past it by hand.
    drop(
        world
            .lifecycle()
            .uninstalling(&change(), &[id(WORK)])
            .unwrap(),
    );
    world.vm("spare", SPARE, State::Created, true);

    let (refused, _) = uninstall(&world);

    let refused = refused.unwrap_err();
    assert!(
        refused.error.contains("atlas uninstall --cancel"),
        "{refused:?}"
    );
    assert_eq!(world.removes.get(), 0);
    assert_eq!(barrier(&world).unwrap().phase, Phase::Vms);
}

#[test]
fn a_blocked_uninstall_called_off_removes_nothing_more_and_lets_adds_and_removals_in() {
    let world = World::new();
    world.write_settings(&format!(
        "default = \"second-login\"\n{SECOND_LOGIN}{}{}",
        vm_entry("work", WORK),
        vm_entry("spare", SPARE)
    ));
    world.vm("work", WORK, State::Created, true);
    world.vm("spare", SPARE, State::Created, true);
    world.runner_forgets();
    // Killed at spare's removal, after work's.
    world.kill_at.set(Some(2));
    uninstall(&world).0.unwrap_err();
    let before = world.read_settings();

    world.allows.set(false);
    let (refused, _) = cancel(&world);
    assert_eq!(refused.unwrap_err().code, Code::Denied);
    assert_eq!(
        barrier(&world).unwrap().phase,
        Phase::Vms,
        "not without the grant"
    );

    world.allows.set(true);
    let (cancelled, said) = cancel(&world);

    let cancelled = cancelled.unwrap();
    assert_eq!(cancelled["cancelled"], true);
    assert_eq!(cancelled["kept"], json!(["spare"]));
    assert!(said[0].contains("calling the uninstall off"), "{said:?}");
    let asked = world.operations.borrow().last().cloned().unwrap();
    assert!(asked.cancel);
    assert_eq!(asked.kind, grant::Kind::Uninstall);
    assert_eq!(asked.name, "spare");
    assert_eq!(barrier(&world).unwrap().phase, Phase::Cancelled);
    assert_eq!(world.record(WORK), None, "removed stays removed");
    assert!(!world.vms.folder(&id(WORK)).exists());
    assert!(
        world.vms.folder(&id(SPARE)).join("Disk.asif").exists(),
        "the rest as it is"
    );
    assert_eq!(world.read_settings(), before);
    world.lifecycle().admit().unwrap();
    assert_eq!(
        pending_of(&world),
        Pending::Ask,
        "another uninstall is asked anew"
    );

    // A removal is let in again, and goes on under its kept change.
    world.kill_at.set(None);
    world.write_settings(&format!(
        "default = \"second-login\"\n{SECOND_LOGIN}{}",
        vm_entry("spare", SPARE)
    ));
    let (removed, _) = world.remove("spare");
    removed.unwrap();
    assert_eq!(world.record(SPARE), None);
}

#[test]
fn with_no_uninstall_under_way_there_is_nothing_to_call_off_and_nobody_is_asked() {
    let world = World::new();
    world.write_settings(&mixed());
    world.vm("work", WORK, State::Created, true);

    let (refused, _) = cancel(&world);

    assert!(
        refused
            .unwrap_err()
            .error
            .contains("No uninstall is under way")
    );
    assert!(world.operations.borrow().is_empty());
    assert_eq!(barrier(&world), None);
}

#[test]
fn no_add_or_removal_starts_between_the_vm_phase_commit_and_the_end_of_the_uninstall() {
    let world = World::new();
    world.write_settings(&mixed());
    world.vm("work", WORK, State::Created, true);
    world.runner_forgets();

    let (removed, _) = uninstall_holding(&world, &|_| {});

    let removed = removed.unwrap().unwrap();
    assert_eq!(barrier(&world).unwrap().phase, Phase::Committed);
    let refused = world.lifecycle().admit().unwrap_err();
    assert!(refused.error.contains("being uninstalled"), "{refused:?}");
    // The shared cleanup deletes Atlas's own folder; its caches' locks stay.
    std::fs::remove_dir_all(world.home.path().join("Library/Application Support/Atlas")).unwrap();
    assert!(world.lifecycle().admit().is_err(), "still out");
    drop(removed);
    world.lifecycle().admit().unwrap();
}

#[test]
fn a_cancel_landing_between_a_resumes_look_and_its_lock_is_never_undone() {
    let world = World::new();
    world.write_settings(&mixed());
    world.vm("work", WORK, State::Created, true);
    world.vm("spare", SPARE, State::Created, true);
    world.runner_forgets();
    world.kill_at.set(Some(1));
    uninstall(&world).0.unwrap_err();
    world.kill_at.set(None);
    let lifecycle = world.lifecycle();

    let (refused, _) = uninstall_holding(&world, &|line| {
        if line.starts_with("Going on") {
            assert!(lifecycle.call_off(&change()).unwrap());
        }
    });

    let refused = refused.unwrap_err();
    assert!(refused.error.contains("called off"), "{refused:?}");
    assert_eq!(world.removes.get(), 0);
    assert_eq!(barrier(&world).unwrap().phase, Phase::Cancelled);
    world.lifecycle().admit().unwrap();
}

#[test]
fn an_uninstall_is_called_off_though_a_lifecycle_record_cannot_be_read_and_never_goes_on() {
    let world = World::new();
    world.write_settings(&mixed());
    world.vm("work", WORK, State::Created, true);
    world.vm("spare", SPARE, State::Created, true);
    world.runner_forgets();
    world.kill_at.set(Some(1));
    uninstall(&world).0.unwrap_err();
    world.kill_at.set(None);
    atlas_core::files::write_private(&world.lifecycle().record_at(&id(THIRD)), b"{").unwrap();

    let (stopped, _) = uninstall(&world);
    assert!(stopped.is_err(), "no removal goes on past it");
    assert_eq!(world.removes.get(), 0);
    let (cancelled, _) = cancel(&world);

    let cancelled = cancelled.unwrap();
    assert_eq!(
        cancelled["kept"],
        json!([WORK, SPARE]),
        "named by identifier"
    );
    assert_eq!(
        world.operations.borrow().last().unwrap().name,
        format!("{WORK} and {SPARE}")
    );
    assert_eq!(barrier(&world).unwrap().phase, Phase::Cancelled);
    world.lifecycle().admit().unwrap();
}
