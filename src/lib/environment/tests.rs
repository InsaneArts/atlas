use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{
    cell::Cell,
    os::unix::fs::{PermissionsExt, symlink},
};

struct Sandbox {
    _root: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let home = root.join("home");
        fs::create_dir(&home).unwrap();

        Self {
            _root: temp,
            root,
            home,
        }
    }

    fn payload(&self, target: &str, bytes: &[u8]) -> Payload {
        let source = self.root.join(crate::control::token());
        fs::write(&source, bytes).unwrap();
        Payload {
            source,
            target: target.into(),
            sha256: digest(bytes),
            executable: false,
        }
    }

    fn preview(&self, files: Vec<Payload>) -> Value {
        worker::preview(
            &self.home,
            nix::unistd::getuid().as_raw(),
            &crate::control::token(),
            files,
            &|| Ok(()),
        )
        .unwrap()
    }

    fn plan(&self, files: Vec<Payload>) -> Plan {
        let value = self.preview(files);
        assert_eq!(value["ok"], true, "{value}");
        serde_json::from_value(value["plan"].clone()).unwrap()
    }

    fn apply(&self, plan: &Plan) -> Value {
        worker::apply_plan(&self.home, plan, &|| Ok(())).unwrap()
    }

    fn rollback(&self, plan: &Plan, apply: bool) -> Value {
        worker::rollback(&self.home, &plan.id, apply, &|| Ok(())).unwrap()
    }
}

#[test]
fn preview_is_read_only_and_create_apply_rollback_is_idempotent() {
    let sandbox = Sandbox::new();
    let mut payload = sandbox.payload(".local/bin/proof", b"frozen executable");
    payload.executable = true;
    let plan = sandbox.plan(vec![payload]);
    assert!(!sandbox.home.join(STATE).exists());
    assert!(!sandbox.home.join(".local").exists());

    assert_eq!(sandbox.apply(&plan)["ok"], true);
    let path = sandbox.home.join(".local/bin/proof");
    assert_eq!(fs::read(&path).unwrap(), b"frozen executable");
    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o700);

    let inode = fs::metadata(&path).unwrap().ino();
    assert_eq!(sandbox.apply(&plan)["ok"], true);
    assert_eq!(fs::metadata(&path).unwrap().ino(), inode);

    let preview = sandbox.rollback(&plan, false);
    assert_eq!(preview["ok"], true, "{preview}");
    assert!(path.exists());

    let result = sandbox.rollback(&plan, true);
    assert_eq!(result["ok"], true, "{result}");
    assert!(!sandbox.home.join(".local").exists());
    assert_eq!(sandbox.rollback(&plan, true)["rolledBack"], true);
}

#[test]
fn rollback_restores_observed_bytes_and_mode_without_inventing_legacy_metadata() {
    let sandbox = Sandbox::new();
    let path = sandbox.home.join("config");
    fs::write(&path, b"old").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();

    let state = state::Store::open(&sandbox.home, true).unwrap();
    let legacy = json!({"files": {"config": digest(b"old")}});
    atomic_write(
        &state.path.join("files.json"),
        &serde_json::to_vec(&legacy).unwrap(),
        0o600,
    )
    .unwrap();
    drop(state);

    let plan = sandbox.plan(vec![sandbox.payload("config", b"new")]);
    assert!(matches!(plan.files[0].previous, Some(Entry::Legacy(_))));
    assert_eq!(sandbox.apply(&plan)["ok"], true);
    assert_eq!(fs::read(&path).unwrap(), b"new");

    let result = sandbox.rollback(&plan, true);
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(fs::read(&path).unwrap(), b"old");
    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o640);

    let state = state::Store::open(&sandbox.home, false).unwrap();
    assert!(matches!(
        state.ledger().unwrap().files[Path::new("config")],
        Entry::Legacy(_)
    ));
}

#[test]
fn adopting_an_existing_file_never_makes_rollback_delete_it() {
    let sandbox = Sandbox::new();
    let path = sandbox.home.join("existing");
    atomic_write(&path, b"same", 0o600).unwrap();
    let inode = fs::metadata(&path).unwrap().ino();
    let plan = sandbox.plan(vec![sandbox.payload("existing", b"same")]);
    assert_eq!(plan.files[0].operation, Operation::Unchanged);

    assert_eq!(sandbox.apply(&plan)["ok"], true);
    assert_eq!(sandbox.rollback(&plan, true)["ok"], true);
    assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
}

#[test]
fn every_destination_is_preflighted_and_helper_edits_are_preserved() {
    let sandbox = Sandbox::new();
    fs::write(sandbox.home.join("existing"), b"mine").unwrap();

    let result = sandbox.preview(vec![
        sandbox.payload("new", b"incoming"),
        sandbox.payload("existing", b"incoming"),
    ]);
    assert_eq!(result["ok"], false);
    assert!(!sandbox.home.join("new").exists());
    assert!(!sandbox.home.join(STATE).exists());

    let plan = sandbox.plan(vec![sandbox.payload("new", b"incoming")]);
    fs::write(sandbox.home.join("new"), b"new helper edit").unwrap();
    assert!(worker::apply_plan(&sandbox.home, &plan, &|| Ok(())).is_err());
    assert_eq!(
        fs::read(sandbox.home.join("new")).unwrap(),
        b"new helper edit"
    );
    assert!(!sandbox.home.join(STATE).exists());
}

#[test]
fn metadata_drift_and_later_same_content_generations_block_older_rollback() {
    let sandbox = Sandbox::new();
    let first = sandbox.plan(vec![sandbox.payload("file", b"same")]);
    assert_eq!(sandbox.apply(&first)["ok"], true);

    let second = sandbox.plan(vec![sandbox.payload("file", b"same")]);
    assert_eq!(sandbox.apply(&second)["ok"], true);

    assert_eq!(sandbox.rollback(&first, false)["ok"], false);
    assert_eq!(sandbox.rollback(&second, true)["ok"], true);

    fs::set_permissions(sandbox.home.join("file"), fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(sandbox.rollback(&first, true)["ok"], false);
    assert_eq!(fs::read(sandbox.home.join("file")).unwrap(), b"same");
}

#[test]
fn interrupted_batch_resumes_frozen_helper_payloads() {
    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![
        sandbox.payload("first", b"one"),
        sandbox.payload("second", b"two"),
    ]);

    let interrupted = Cell::new(false);
    let result = worker::apply_plan(&sandbox.home, &plan, &|| {
        if sandbox.home.join("first").exists() && !interrupted.replace(true) {
            return Err(problem("session changed"));
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(result["ok"], false);
    assert_eq!(result["resumeRequired"], true);
    assert!(!sandbox.home.join("second").exists());

    // Resume uses the helper's durable snapshot, not even the controller stage.
    for file in &plan.files {
        fs::remove_file(&file.payload.source).unwrap();
    }

    assert_eq!(sandbox.apply(&plan)["ok"], true);
    assert_eq!(fs::read(sandbox.home.join("first")).unwrap(), b"one");
    assert_eq!(fs::read(sandbox.home.join("second")).unwrap(), b"two");
    assert_eq!(sandbox.rollback(&plan, true)["ok"], true);
}

#[test]
fn unsupported_metadata_and_links_are_refused_without_modification() {
    let sandbox = Sandbox::new();
    let path = sandbox.home.join("file");
    fs::write(&path, b"same").unwrap();
    fs::hard_link(&path, sandbox.home.join("alias")).unwrap();
    assert!(snapshot(&path).is_err());

    fs::remove_file(sandbox.home.join("alias")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o4600)).unwrap();
    assert!(snapshot(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"same");
}

#[test]
fn aliases_reserved_state_and_special_files_are_rejected() {
    for target in [
        ".config//atlas/provisioning/lock",
        ".CONFIG/atlas/./PROVISIONING/files.json",
        "../escape",
    ] {
        assert!(unique_targets([Path::new(target)].into_iter()).is_err());
    }

    assert!(
        unique_targets([Path::new(".config/Tool"), Path::new(".CONFIG/tool")].into_iter()).is_err()
    );
    assert!(unique_targets([Path::new("tool"), Path::new("tool/child")].into_iter()).is_err());

    let sandbox = Sandbox::new();
    let path = sandbox.root.join("data");
    fs::write(&path, b"too long").unwrap();
    assert!(read_limited(&path, 2).is_err());

    symlink(&path, sandbox.root.join("link")).unwrap();
    assert!(read_limited(&sandbox.root.join("link"), MAX_FILE).is_err());

    symlink(&sandbox.root, sandbox.home.join("escape")).unwrap();
    assert!(atomic_install(&sandbox.home.join("escape/file"), b"data", 0o600, true).is_err());
    assert!(atomic_install(&path, b"replacement", 0o600, true).is_err());
    assert_eq!(fs::read(path).unwrap(), b"too long");
}

#[test]
fn malformed_or_interrupted_mutation_replies_remain_uncertain() {
    for reply in [
        json!({"timedOut": true, "signal": null}),
        json!({"timedOut": false, "signal": "SIGTERM"}),
        json!({"timedOut": false, "signal": null, "exitCode": 0, "events": []}),
        json!({"timedOut": false, "signal": null, "exitCode": 0, "events": [{"event": "output", "data": "invalid!"}]}),
    ] {
        assert!(
            decode_reply(&reply, true, "request-ab12")
                .unwrap_err()
                .uncertain
        );
    }
}

#[test]
fn rollback_retains_a_created_directory_it_cannot_inspect() {
    let applied = || {
        let sandbox = Sandbox::new();
        let plan = sandbox.plan(vec![sandbox.payload("a/b/file", b"bytes")]);
        assert_eq!(sandbox.apply(&plan)["ok"], true);
        (sandbox, plan)
    };

    // On an identical rollback, the last guard call made while a/b still exists
    // is the one authorizing its removal.
    let (twin, twin_plan) = applied();
    let (calls, swap_at) = (Cell::new(0), Cell::new(0));
    worker::rollback(&twin.home, &twin_plan.id, true, &|| {
        calls.set(calls.get() + 1);
        if twin.home.join("a/b").is_dir() {
            swap_at.set(calls.get());
        }
        Ok(())
    })
    .unwrap();
    assert!(!twin.home.join("a").exists());

    let swap_at = swap_at.get();
    let (sandbox, plan) = applied();
    let (home, seen) = (sandbox.home.clone(), Cell::new(0));
    // Before removing a/b, replace a with a symlink. Removing a/b through it
    // and inspecting a then fail (O_NOFOLLOW) instead of proving ownership.
    let guard = || {
        seen.set(seen.get() + 1);
        if seen.get() == swap_at {
            fs::rename(home.join("a"), home.join("moved")).unwrap();
            symlink(home.join("moved"), home.join("a")).unwrap();
        }
        Ok(())
    };

    let result = worker::rollback(&sandbox.home, &plan.id, true, &guard).unwrap();
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["rolledBack"], true);
    assert_eq!(result["retainedDirectories"], json!(["a/b", "a"]));
    assert!(sandbox.home.join("moved/b").is_dir());

    let next = sandbox.plan(vec![sandbox.payload("next", b"next")]);
    assert_eq!(sandbox.apply(&next)["ok"], true);
}

#[test]
fn rollback_preserves_generated_outputs_and_unblocks_later_operations() {
    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![sandbox.payload("project/input.md", b"input")]);
    assert_eq!(sandbox.apply(&plan)["ok"], true);

    fs::write(
        sandbox.home.join("project/output.pdf"),
        b"legitimate output",
    )
    .unwrap();

    let result = sandbox.rollback(&plan, true);
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["rolledBack"], true);
    assert_eq!(result["retainedDirectories"], json!(["project"]));
    assert!(!sandbox.home.join("project/input.md").exists());
    assert_eq!(
        fs::read(sandbox.home.join("project/output.pdf")).unwrap(),
        b"legitimate output"
    );

    let next = sandbox.plan(vec![sandbox.payload("next", b"next")]);
    assert_eq!(sandbox.apply(&next)["ok"], true);
}

#[test]
fn bounded_extended_attributes_are_preserved_and_compared_exactly() {
    let sandbox = Sandbox::new();
    let path = sandbox.home.join("config");
    atomic_write(&path, b"old", 0o640).unwrap();
    let mut attributes = fingerprint(&path).unwrap().unwrap().xattrs;
    attributes.insert("user.atlas-test".into(), STANDARD.encode([0_u8, 1, 255, 3]));
    restore_attributes(&path, &attributes).unwrap();

    let state = state::Store::open(&sandbox.home, true).unwrap();
    state
        .save_ledger(&Ledger {
            version: 0,
            files: BTreeMap::from([(PathBuf::from("config"), Entry::Legacy(digest(b"old")))]),
        })
        .unwrap();
    drop(state);

    let plan = sandbox.plan(vec![sandbox.payload("config", b"new")]);
    let result = sandbox.apply(&plan);
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(fingerprint(&path).unwrap().unwrap().xattrs, attributes);

    let result = sandbox.rollback(&plan, true);
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(fingerprint(&path).unwrap().unwrap().xattrs, attributes);
    assert_eq!(fs::read(&path).unwrap(), b"old");
    assert_eq!(fs::metadata(path).unwrap().mode() & 0o777, 0o640);
}

/// Values larger than a page are read whole, never as a prefix, and the
/// 16 KiB budget is enforced.
#[test]
fn large_extended_attributes_are_read_whole_and_bounded() {
    let sandbox = Sandbox::new();
    let path = sandbox.home.join("large");
    atomic_write(&path, b"x", 0o600).unwrap();
    let mut attributes = fingerprint(&path).unwrap().unwrap().xattrs;
    let value: Vec<u8> = (0..10_000).map(|i| (i % 251) as u8).collect();
    attributes.insert("user.atlas-large".into(), STANDARD.encode(&value));
    restore_attributes(&path, &attributes).unwrap();
    let read = fingerprint(&path).unwrap().unwrap().xattrs;
    assert_eq!(STANDARD.decode(&read["user.atlas-large"]).unwrap(), value);

    attributes.insert(
        "user.atlas-larger".into(),
        STANDARD.encode(vec![7_u8; 8_000]),
    );
    assert!(restore_attributes(&path, &attributes).is_err());
}

#[test]
fn every_apply_guard_failure_can_resume_then_restore_the_original_files() {
    // Each guard is a possible console/session transition. The assertions cover
    // both early refusal and partial durable writes, not a particular call count.
    let baseline = Sandbox::new();
    let baseline_plan = baseline.plan(vec![
        baseline.payload("nested/first", b"one"),
        baseline.payload("second", b"two"),
    ]);
    let calls = Cell::new(0);
    assert_eq!(
        worker::apply_plan(&baseline.home, &baseline_plan, &|| {
            calls.set(calls.get() + 1);
            Ok(())
        })
        .unwrap()["ok"],
        true
    );

    for fail_at in 1..=calls.get() {
        let sandbox = Sandbox::new();
        let plan = sandbox.plan(vec![
            sandbox.payload("nested/first", b"one"),
            sandbox.payload("second", b"two"),
        ]);

        let current = Cell::new(0);
        let result = worker::apply_plan(&sandbox.home, &plan, &|| {
            current.set(current.get() + 1);
            if current.get() == fail_at {
                Err(problem("session changed"))
            } else {
                Ok(())
            }
        });
        assert!(
            result.is_err() || result.unwrap()["ok"] == false,
            "guard {fail_at}"
        );

        let result = sandbox.apply(&plan);
        assert_eq!(result["ok"], true, "guard {fail_at}: {result}");
        assert_eq!(fs::read(sandbox.home.join("nested/first")).unwrap(), b"one");
        assert_eq!(fs::read(sandbox.home.join("second")).unwrap(), b"two");

        let result = sandbox.rollback(&plan, true);
        assert_eq!(result["ok"], true, "guard {fail_at}: {result}");
        assert!(!sandbox.home.join("nested/first").exists());
        assert!(!sandbox.home.join("second").exists());
    }
}

#[test]
fn every_rollback_guard_failure_is_resumable() {
    let baseline = Sandbox::new();
    let baseline_plan = baseline.plan(vec![baseline.payload("nested/file", b"one")]);
    assert_eq!(baseline.apply(&baseline_plan)["ok"], true);
    let calls = Cell::new(0);
    assert_eq!(
        worker::rollback(&baseline.home, &baseline_plan.id, true, &|| {
            calls.set(calls.get() + 1);
            Ok(())
        })
        .unwrap()["ok"],
        true
    );

    for fail_at in 1..=calls.get() {
        let sandbox = Sandbox::new();
        let plan = sandbox.plan(vec![sandbox.payload("nested/file", b"one")]);
        assert_eq!(sandbox.apply(&plan)["ok"], true);

        let current = Cell::new(0);
        let result = worker::rollback(&sandbox.home, &plan.id, true, &|| {
            current.set(current.get() + 1);
            if current.get() == fail_at {
                Err(problem("session changed"))
            } else {
                Ok(())
            }
        });
        assert!(
            result.is_err() || result.unwrap()["ok"] == false,
            "guard {fail_at}"
        );

        let result = sandbox.rollback(&plan, true);
        assert_eq!(result["ok"], true, "guard {fail_at}: {result}");
        assert!(!sandbox.home.join("nested").exists());
    }
}

#[test]
fn lost_install_acknowledgement_uses_the_prepared_inode_not_just_its_hash() {
    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![sandbox.payload("file", b"one")]);
    assert_eq!(sandbox.apply(&plan)["ok"], true);
    let path = sandbox.home.join("file");
    let installed_inode = fs::metadata(&path).unwrap().ino();

    let store = state::Store::open(&sandbox.home, true).unwrap();
    let mut transaction = store.transaction(&plan.id).unwrap().unwrap();
    // This is the on-disk boundary after rename and before ledger acknowledgement.
    transaction.phase = Phase::Applying;
    transaction.progress[0].applied = false;
    store.save(&transaction).unwrap();
    store.save_ledger(&Ledger::default()).unwrap();
    store.activate(&plan.id).unwrap();
    drop(store);

    assert_eq!(sandbox.apply(&plan)["ok"], true);
    assert_eq!(fs::metadata(&path).unwrap().ino(), installed_inode);

    // An uncooperative replacement with identical bytes is still a new inode.
    atomic_write(&path, b"one", 0o600).unwrap();
    assert_eq!(sandbox.rollback(&plan, true)["ok"], false);
    assert_eq!(fs::read(path).unwrap(), b"one");
}

#[test]
fn rollback_of_an_unacknowledged_prepared_file_leaves_no_owned_candidate() {
    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![sandbox.payload("file", b"one")]);
    assert_eq!(sandbox.apply(&plan)["ok"], true);

    let candidate = sandbox.home.join(format!(".atlas-{}-0-install", plan.id));
    fs::rename(sandbox.home.join("file"), &candidate).unwrap();
    let store = state::Store::open(&sandbox.home, true).unwrap();
    let mut transaction = store.transaction(&plan.id).unwrap().unwrap();
    transaction.phase = Phase::Applying;
    transaction.progress[0] = Progress::default();
    store.save(&transaction).unwrap();
    store.save_ledger(&Ledger::default()).unwrap();
    store.activate(&plan.id).unwrap();
    drop(store);

    let result = sandbox.rollback(&plan, true);
    assert_eq!(result["ok"], true, "{result}");
    assert!(!candidate.exists());
    assert!(!sandbox.home.join("file").exists());
}

#[test]
fn exact_worker_identity_includes_capture_parent_and_display_geometry() {
    let proof = json!({"ok": true, "uid": 502, "parentPID": 321, "securitySessionID": 456,
        "displayID": 50, "width": 1440, "height": 900, "onConsole": false, "hasGraphicAccess": true});
    let expected = Identity::from_proof(&proof).unwrap();

    for (key, value) in [
        ("uid", json!(503)),
        ("parentPID", json!(322)),
        ("securitySessionID", json!(457)),
        ("displayID", json!(51)),
        ("width", json!(1441)),
        ("height", json!(901)),
    ] {
        let mut changed = proof.clone();
        changed[key] = value;
        assert_ne!(Identity::from_proof(&changed).unwrap(), expected, "{key}");
    }

    for (key, value) in [
        ("parentPID", json!(0)),
        ("onConsole", json!(true)),
        ("hasGraphicAccess", json!(false)),
    ] {
        let mut changed = proof.clone();
        changed[key] = value;
        assert!(Identity::from_proof(&changed).is_err(), "{key}");
    }
}

#[test]
fn rollback_checks_every_backup_before_restoring_any_selected_file() {
    let sandbox = Sandbox::new();
    let first = sandbox.plan(vec![
        sandbox.payload("first", b"old one"),
        sandbox.payload("second", b"old two"),
    ]);
    assert_eq!(sandbox.apply(&first)["ok"], true);

    let second = sandbox.plan(vec![
        sandbox.payload("first", b"new one"),
        sandbox.payload("second", b"new two"),
    ]);
    assert_eq!(sandbox.apply(&second)["ok"], true);

    let store = state::Store::open(&sandbox.home, true).unwrap();
    fs::write(store.blob(&second.id, 0, true), b"corrupt").unwrap();
    drop(store);

    assert_eq!(sandbox.rollback(&second, false)["ok"], false);
    assert_eq!(sandbox.rollback(&second, true)["ok"], false);
    assert_eq!(fs::read(sandbox.home.join("first")).unwrap(), b"new one");
    assert_eq!(fs::read(sandbox.home.join("second")).unwrap(), b"new two");
}

#[test]
fn nested_rollbacks_restore_predecessor_provenance_with_the_new_inode() {
    let sandbox = Sandbox::new();
    let first = sandbox.plan(vec![sandbox.payload("file", b"first")]);
    assert_eq!(sandbox.apply(&first)["ok"], true);

    let second = sandbox.plan(vec![sandbox.payload("file", b"second")]);
    assert_eq!(sandbox.apply(&second)["ok"], true);

    assert_eq!(sandbox.rollback(&second, true)["ok"], true);
    assert_eq!(fs::read(sandbox.home.join("file")).unwrap(), b"first");

    assert_eq!(sandbox.rollback(&first, true)["ok"], true);
    assert!(!sandbox.home.join("file").exists());
}

#[test]
fn aliases_of_existing_managed_targets_are_rejected() {
    let sandbox = Sandbox::new();
    let first = sandbox.plan(vec![sandbox.payload("Config", b"data")]);
    assert_eq!(sandbox.apply(&first)["ok"], true);
    assert!(
        worker::preview(
            &sandbox.home,
            nix::unistd::getuid().as_raw(),
            &crate::control::token(),
            vec![sandbox.payload("config", b"data")],
            &|| Ok(())
        )
        .is_err()
    );

    for path in [".config", ".config/atlas"] {
        assert!(unique_targets([Path::new(path)].into_iter()).is_err());
    }
}

#[test]
#[cfg(target_os = "macos")]
fn an_existing_acl_is_refused_without_removing_its_entries() {
    let sandbox = Sandbox::new();
    let path = sandbox.home.join("acl-file");
    atomic_write(&path, b"owned test data", 0o600).unwrap();

    assert!(
        std::process::Command::new("/bin/chmod")
            .args(["+a", "everyone allow read"])
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    assert!(has_acl(&path).unwrap());
    assert!(snapshot(&path).is_err());
    assert!(has_acl(&path).unwrap());
    assert_eq!(fs::read(path).unwrap(), b"owned test data");
    assert!(!sandbox.home.join(STATE).exists());
}

#[test]
fn selected_files_under_config_atlas_share_ancestors_with_private_state() {
    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![
        sandbox.payload(
            ".config/atlas/environment.json",
            b"{\"version\":1,\"variables\":{}}",
        ),
        sandbox.payload(".config/atlas/proof/subfolder/input", b"selected"),
    ]);

    assert!(!sandbox.home.join(".config").exists());

    let result = sandbox.apply(&plan);
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(sandbox.apply(&plan)["ok"], true);

    let result = sandbox.rollback(&plan, true);
    assert_eq!(result["ok"], true, "{result}");
    assert!(!sandbox.home.join(".config/atlas/environment.json").exists());
    assert!(!sandbox.home.join(".config/atlas/proof").exists());
    assert!(sandbox.home.join(STATE).is_dir());
}

#[test]
fn shipped_manifest_and_profile_examples_parse_as_toml() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = root.join("examples/environment.toml");
    let parsed: Manifest =
        crate::config::parse_toml(&fs::read(&manifest).unwrap(), &manifest).unwrap();
    assert_eq!(parsed.files.len(), 2);
    assert!(parsed.files[0].executable);

    let profile = root.join("examples/profile.toml");
    profile::load(&profile).unwrap();

    let json = br#"{"files": []}"#;
    let refused = crate::config::parse_toml::<Manifest>(json, &manifest);
    assert!(refused.is_err_and(|error| error.code == "config"));
}

#[test]
fn a_briefly_held_store_lock_is_waited_for_but_a_held_one_is_refused() {
    let sandbox = Sandbox::new();
    let first = state::Store::open(&sandbox.home, true).unwrap();
    let release = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(10));
        drop(first);
    });

    state::Store::open(&sandbox.home, true).expect("a lock released within the patience is taken");
    release.join().unwrap();

    let held = state::Store::open(&sandbox.home, true).unwrap();
    let started = std::time::Instant::now();
    let refused = state::Store::open(&sandbox.home, true).err().unwrap();
    assert_eq!(refused.error, "Another environment operation is running");
    assert!(started.elapsed() >= std::time::Duration::from_millis(200));
    drop(held);
}

/// Only a worker's own refusal becomes an error; a command's output, even
/// an error of the same shape with the same exit status, stays the command's.
#[test]
fn a_worker_refusal_is_told_from_the_command_output() {
    let reply = |exit: u8, text: &str| {
        json!({
            "exitCode": exit,
            "timedOut": false,
            "signal": null,
            "events": [{"event": "output", "data": STANDARD.encode(text)}],
        })
    };

    let refusal = r#"{"ok":false,"code":"environment","error":"No helper environment profile is installed","uncertain":false,"refusal":"request-ab12"}"#;
    let error = refused(reply(WORKER_REFUSED, refusal), "request-ab12").unwrap_err();
    assert!(error.error.contains("No helper environment profile"));
    assert!(
        refused(reply(1, refusal), "request-ab12").is_ok(),
        "exit 1 is the command's"
    );

    let lookalike = r#"{"ok":false,"error":"No helper environment profile is installed"}"#;
    assert!(refused(reply(WORKER_REFUSED, lookalike), "request-ab12").is_ok());
    assert!(refused(reply(WORKER_REFUSED, refusal), "request-other").is_ok());
    assert!(refused(reply(WORKER_REFUSED, "plain text"), "request-ab12").is_ok());

    // A missing profile keeps its own code across the process boundary, so
    // readiness can fall back to the helper's PATH on it.
    let missing = format!(
        r#"{{"ok":false,"code":"{NO_PROFILE}","error":"No helper environment profile is installed","refusal":"request-ab12"}}"#
    );
    let error = refused(reply(WORKER_REFUSED, &missing), "request-ab12").unwrap_err();
    assert_eq!(error.code, NO_PROFILE);
}

/// A frozen-plan folder as freeze leaves it, in a private shared folder.
fn stage_in(shared: &Path, id: &str) -> PathBuf {
    let stage = shared.join(format!("environment-plan-{id}"));
    fs::DirBuilder::new().mode(0o700).create(&stage).unwrap();
    stage
}

fn write_plan(stage: &Path, id: &str, uid: u32) {
    let plan = Plan {
        version: 1,
        id: id.into(),
        uid,
        home: "/Users/helper".into(),
        files: Vec::new(),
    };
    fs::write(stage.join("plan.json"), serde_json::to_vec(&plan).unwrap()).unwrap();
}

/// A worker's reply as the helper's `run` returns it.
fn worker_reply(exit: u8, output: &Value) -> Value {
    json!({
        "exitCode": exit,
        "timedOut": false,
        "signal": null,
        "events": [{"event": "output", "data": STANDARD.encode(output.to_string())}],
    })
}

#[test]
fn only_a_confirmed_finish_settles_a_plan() {
    let done = json!({"ok": true, "phase": "applied", "resumeRequired": false});
    assert!(settled(&Ok(done.clone()), "applied"));
    assert!(!settled(&Ok(done), "rolledBack"));

    for value in [
        json!({"ok": true, "phase": "applied", "resumeRequired": true}),
        json!({"ok": false, "phase": "applied", "resumeRequired": false}),
        json!({"ok": true, "phase": "applied"}),
    ] {
        assert!(!settled(&Ok(value.clone()), "applied"), "{value}");
    }

    assert!(!settled(&Err(problem("lost")), "applied"));
}

#[test]
fn a_stage_keeps_requests_whose_outcome_is_unknown() {
    let sandbox = Sandbox::new();
    let stage = stage_in(&sandbox.root, &crate::control::token());
    assert!(!requests(&stage));

    fs::write(stage.join("controller.lock"), b"").unwrap();
    assert!(!requests(&stage), "a lock is not a request");

    fs::create_dir(stage.join("request-ab12")).unwrap();
    assert!(requests(&stage));
    assert!(!requests(&sandbox.root.join("missing")));
}

/// A rollback's own folder goes once nothing in it is unconfirmed, also
/// after a refusal (a mistyped ID); a frozen plan's folder goes only with
/// a finished undo.
#[test]
fn rollback_removes_its_own_folder_unless_a_request_is_unconfirmed() {
    let sandbox = Sandbox::new();
    let stage = stage_in(&sandbox.root, &crate::control::token());
    fs::write(stage.join("controller.lock"), b"").unwrap();
    let refused: Result<Value> = Ok(json!({"ok": false, "error": worker::MISSING}));
    let finished: Result<Value> =
        Ok(json!({"ok": true, "phase": "rolledBack", "resumeRequired": false}));
    let stopped: Result<Value> =
        Ok(json!({"ok": false, "phase": "rollingBack", "resumeRequired": true}));

    for apply in [false, true] {
        assert!(rolled_back(&refused, apply, true, &stage), "apply {apply}");
        assert!(
            !rolled_back(&refused, apply, false, &stage),
            "apply {apply}"
        );
    }

    assert!(rolled_back(&Err(problem("refused")), true, true, &stage));
    assert!(rolled_back(&finished, true, false, &stage));
    assert!(!rolled_back(&finished, false, false, &stage));
    assert!(!rolled_back(&stopped, true, false, &stage));

    fs::create_dir(stage.join("request-ab12")).unwrap();
    assert!(!rolled_back(&refused, true, true, &stage));
    assert!(!rolled_back(
        &Err(problem("lost").uncertain()),
        true,
        true,
        &stage
    ));
}

/// An undo of an ID the helper never applied is a refusal, not a
/// transaction to resume; any other failed mutation stays resumable.
#[test]
fn a_rollback_of_an_unknown_id_is_not_resumable() {
    let refusal = |error: &str, request: &str| json!({"ok": false, "code": "environment", "error": error, "uncertain": false, "refusal": request});
    let reply = worker_reply(WORKER_REFUSED, &refusal(worker::MISSING, "request-ab12"));
    let value = decode_reply(&reply, true, "request-ab12").unwrap();
    assert_eq!(value["ok"], false);
    assert!(value["resumeRequired"].is_null(), "{value}");

    for (exit, output) in [
        (WORKER_REFUSED, refusal(worker::MISSING, "request-other")),
        (
            WORKER_REFUSED,
            refusal("Transaction is busy", "request-ab12"),
        ),
        (1, json!({"ok": false, "error": worker::MISSING})),
    ] {
        let value = decode_reply(&worker_reply(exit, &output), true, "request-ab12").unwrap();
        assert_eq!(value["resumeRequired"], true, "{output}");
    }

    // A preview of an unknown ID is an answer, never a resume.
    let value = decode_reply(&reply, false, "request-ab12").unwrap();
    assert!(value["resumeRequired"].is_null());
}

#[test]
fn results_name_a_retained_plan_only_when_one_exists() {
    let sandbox = Sandbox::new();
    let id = crate::control::token();
    let stage = stage_in(&sandbox.root, &id);
    write_plan(&stage, &id, 501);
    let kept = retained(Ok(json!({"ok": true})), &id, &stage).unwrap();
    assert_eq!(kept["retainedPlan"], json!(stage));
    assert!(kept["retainedRequest"].is_null());
    assert_eq!(kept["planId"], id.as_str());

    let error = retained(Err(problem("Stopped.")), &id, &stage).unwrap_err();
    let suffix = format!(" Frozen plan {id} retained at {}.", stage.display());
    assert!(error.error.ends_with(&suffix), "{}", error.error);

    // Only requests left: evidence, not a plan to apply.
    fs::remove_file(stage.join("plan.json")).unwrap();
    fs::create_dir(stage.join("request-ab12")).unwrap();
    let request = retained(Ok(json!({"ok": false})), &id, &stage).unwrap();
    assert!(request["retainedPlan"].is_null());
    assert_eq!(request["retainedRequest"], json!(stage));
    let error = retained(Err(problem("Stopped.")), &id, &stage).unwrap_err();
    assert!(!error.error.contains("Frozen plan"), "{}", error.error);
    assert!(error.error.contains("Retained request"), "{}", error.error);

    fs::remove_dir_all(&stage).unwrap();
    let gone = retained(Ok(json!({"ok": true})), &id, &stage).unwrap();
    assert!(gone["retainedPlan"].is_null() && gone["retainedRequest"].is_null());
    let error = retained(Err(problem("Stopped.")), &id, &stage).unwrap_err();
    assert_eq!(error.error, "Stopped.");
}

/// A preview that wrote no plan (a refusal, or an error on the way) leaves
/// none of the selected files' copies behind.
#[test]
fn a_preview_without_a_plan_discards_its_copies() {
    let sandbox = Sandbox::new();
    let id = crate::control::token();
    let stage = stage_in(&sandbox.root, &id);
    fs::write(stage.join("payload-0"), b"secret").unwrap();
    let refused = previewed(Ok(json!({"ok": false, "plan": null})), &id, &stage).unwrap();
    assert!(!stage.exists());
    assert!(refused["retainedPlan"].is_null());

    let stage = stage_in(&sandbox.root, &id);
    fs::write(stage.join("payload-0"), b"secret").unwrap();
    let error = previewed(Err(problem("Unreadable source.")), &id, &stage).unwrap_err();
    assert!(!stage.exists());
    assert_eq!(error.error, "Unreadable source.");

    let stage = stage_in(&sandbox.root, &id);
    write_plan(&stage, &id, 501);
    let planned = previewed(Ok(json!({"ok": true})), &id, &stage).unwrap();
    assert!(stage.join("plan.json").exists());
    assert_eq!(planned["retainedPlan"], json!(stage));
}

#[test]
fn leftovers_are_only_atlas_folders_with_valid_ids() {
    let sandbox = Sandbox::new();
    let id = crate::control::token();
    let plan = stage_in(&sandbox.root, &id);
    let run = sandbox.root.join(format!("environment-run-{id}"));
    fs::create_dir(&run).unwrap();
    fs::create_dir(sandbox.root.join("environment-plan-short")).unwrap();
    fs::create_dir(sandbox.root.join(format!("other-{id}"))).unwrap();
    let file = crate::control::token();
    fs::write(sandbox.root.join(format!("environment-plan-{file}")), b"").unwrap();

    let linked = crate::control::token();
    symlink(
        &plan,
        sandbox.root.join(format!("environment-plan-{linked}")),
    )
    .unwrap();

    let mut found = leftovers(&sandbox.root);
    found.sort();
    let mut expected = vec![plan, run];
    expected.sort();
    assert_eq!(found, expected);
    assert!(leftovers(&sandbox.root.join("missing")).is_empty());
}

#[test]
fn only_old_readable_plans_without_unknown_requests_are_pruned() {
    let sandbox = Sandbox::new();
    let id = crate::control::token();
    let stage = stage_in(&sandbox.root, &id);

    fs::create_dir(sandbox.root.join(format!("environment-run-{id}"))).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));

    assert_eq!(
        stale(&sandbox.root, std::time::Duration::ZERO),
        vec![(id.clone(), stage.clone())],
        "a command's folder is never a plan"
    );
    assert!(stale(&sandbox.root, UNUSED).is_empty());
    assert!(!prunable(&stage, &id, 501), "no plan to ask about");

    write_plan(&stage, &id, 501);
    assert!(prunable(&stage, &id, 501));
    assert!(!prunable(&stage, &id, 502), "another helper's plan");
    assert!(!prunable(&stage, &crate::control::token(), 501));

    fs::create_dir(stage.join("request-ab12")).unwrap();
    assert!(!prunable(&stage, &id, 501));
}

/// A request that cannot be sent leaves no request folder, which would
/// otherwise read as an unknown outcome forever.
#[tokio::test]
async fn a_request_that_was_never_sent_leaves_nothing() {
    let sandbox = Sandbox::new();
    let text = "helper_uid = 60001\nsocket_path = \"c.sock\"\noutput_dir = \"out\"\n\
                identity_probe = \"probe\"\ninput_socket = \"i.sock\"\n";
    let config =
        crate::config::Config::parse(text.as_bytes(), &sandbox.root.join("u.toml")).unwrap();
    let mut client = Client::new(config).unwrap();
    client.controller = crate::control::Controller::at(60001, &sandbox.root.join("state"));

    let stage = stage_in(&sandbox.root, &crate::control::token());
    let request = Request::Rollback {
        id: crate::control::token(),
        apply: true,
    };
    assert!(dispatch(&client, "nobody", &stage, request).await.is_err());
    assert_eq!(fs::read_dir(&stage).unwrap().count(), 0);

    // A folder the helper cannot be given goes too.
    let refused = sandbox.root.join("refused");
    assert!(create_stage(&refused, "atlas-no-such-user-zz").is_err());
    assert!(!refused.exists());
}

/// Plans already on disk store each operation by its lowercase name.
#[test]
fn operations_keep_their_stored_names() {
    for (operation, name) in [
        (Operation::Create, "create"),
        (Operation::Update, "update"),
        (Operation::Permissions, "permissions"),
        (Operation::Unchanged, "unchanged"),
        (Operation::Conflict, "conflict"),
    ] {
        assert_eq!(serde_json::to_value(operation).unwrap(), json!(name));
        assert_eq!(
            serde_json::from_value::<Operation>(json!(name)).unwrap(),
            operation
        );
    }

    assert!(serde_json::from_value::<Operation>(json!("Create")).is_err());

    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![sandbox.payload("file", b"data")]);
    let stored = serde_json::to_value(&plan).unwrap();
    assert_eq!(stored["files"][0]["operation"], "create");
    assert_eq!(serde_json::from_value::<Plan>(stored).unwrap(), plan);
}

#[test]
#[cfg(target_os = "macos")]
fn an_inherited_folder_acl_is_cleared() {
    let sandbox = Sandbox::new();
    let folder = sandbox.root.join("stage");
    fs::create_dir(&folder).unwrap();

    assert!(
        std::process::Command::new("/bin/chmod")
            .args(["+a", "everyone allow read"])
            .arg(&folder)
            .status()
            .unwrap()
            .success()
    );
    assert!(has_acl(&folder).unwrap());
    remove_inherited_acl(&folder).unwrap();
    assert!(!has_acl(&folder).unwrap());
    assert!(remove_inherited_acl(&sandbox.root.join("missing")).is_err());
}
