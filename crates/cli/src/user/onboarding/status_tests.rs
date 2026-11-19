use super::*;

#[test]
fn permissions_without_the_jobs_answer_are_unknown_never_off() {
    assert_eq!(permissions_from(None), Value::Null);
    let hello = json!({"permissions": {"screenCapture": true, "accessibility": false}});
    assert_eq!(
        permissions_from(Some(&hello)),
        json!({"screenCapture": true, "accessibility": false})
    );
}

#[test]
fn onboarding_status_with_only_a_vm_is_configured() {
    let home = tempfile::tempdir().unwrap();
    let settings = home.path().join("atlas.toml");
    atlas_core::config::write::Change::AddVm {
        name: "vm".to_owned(),
        id: atlas_core::desktop::DesktopId::parse("0123456789abcdef01234567").unwrap(),
    }
    .apply(&settings)
    .unwrap();
    assert!(only_vms(&settings), "a settings file listing a VM alone");
    assert!(!only_vms(&home.path().join("absent.toml")));

    let vm = json!({"name": "vm", "kind": "vm", "lifecycle": "created", "ready": true});
    let reply = on_vms(
        json!({"name": "me"}),
        json!({"installed": true}),
        Some(vm.clone()),
        json!({"vm": {"available": true}}),
        false,
    );

    assert_eq!(reply["ok"], true);
    assert_eq!(reply["role"], "controller");
    assert_eq!(reply["configured"], true);
    assert_eq!(reply["finished"], true, "a ready VM finished its setup");
    assert_eq!(reply["candidates"], json!([]));
    assert_eq!(reply["vm"], vm);
    assert!(reply.get("helper").is_none(), "no Second Login to look at");
    let adding = on_vms(json!({}), json!({}), None, Value::Null, false);
    assert_eq!(adding["finished"], false);
}

#[test]
fn the_vm_of_a_first_setup_is_the_one_being_added_else_the_default_one_and_never_beside_a_second_login()
 {
    let listing = |desktops: Value| json!({"ok": true, "desktops": desktops});
    let adding = json!({"name": "vm-2", "kind": "vm", "listed": false, "lifecycle": "adding"});
    let added = json!({"name": "vm", "kind": "vm", "listed": true, "default": true,
        "lifecycle": "created"});
    let removing = json!({"name": "old", "kind": "vm", "listed": false, "lifecycle": "removing"});
    let user = json!({"name": "second-login", "kind": "user", "default": true});

    assert_eq!(
        setting_up(&listing(json!([added, adding]))),
        Some(adding.clone())
    );
    assert_eq!(setting_up(&listing(json!([added]))), Some(added.clone()));
    assert_eq!(setting_up(&listing(json!([removing]))), None);
    assert_eq!(setting_up(&listing(json!([user, adding]))), None);
    assert_eq!(setting_up(&listing(json!([]))), None);
}
