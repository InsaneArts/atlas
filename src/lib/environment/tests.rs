use super::{
    list::{Labels, SHOWN, frozen, list_as, listing},
    *,
};
use crate::helper_test::{Helper, UID};
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
        let source = self.root.join(crate::files::token());
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
            &crate::files::token(),
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

    // A file the helper changed since is kept as it is.
    fs::set_permissions(sandbox.home.join("file"), fs::Permissions::from_mode(0o644)).unwrap();
    let result = sandbox.rollback(&first, true);
    assert_eq!(result["rolledBack"], true, "{result}");
    assert_eq!(result["keptTargets"], json!(["file"]));
    assert_eq!(fs::read(sandbox.home.join("file")).unwrap(), b"same");
    assert_eq!(
        fs::metadata(sandbox.home.join("file")).unwrap().mode() & 0o777,
        0o644
    );
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

#[tokio::test]
async fn a_worker_killed_after_replacing_a_file_stays_pending_until_recovery() {
    const TEST: &str =
        "environment::tests::a_worker_killed_after_replacing_a_file_stays_pending_until_recovery";
    if let Some(path) = std::env::var_os("ATLAS_ENVIRONMENT_TEST_PACKET") {
        let packet: Packet = decode(&fs::read(path).unwrap()).unwrap();
        let Request::Apply { plan } = packet.request else {
            panic!("expected an apply request");
        };
        let _ = worker::apply_plan(&plan.home, &plan, &|| {
            if fs::read(plan.home.join("file")).unwrap() == b"replacement" {
                nix::sys::signal::kill(nix::unistd::getpid(), nix::sys::signal::Signal::SIGKILL)
                    .unwrap();
            }
            Ok(())
        });
        panic!("the worker must be killed after replacement");
    }
    if !crate::transaction_test::isolated(TEST) {
        return;
    }

    use std::os::unix::process::ExitStatusExt;
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::UnixListener,
        process::Command,
    };

    let sandbox = Sandbox::new();
    let first = sandbox.plan(vec![sandbox.payload("file", b"original")]);
    assert_eq!(sandbox.apply(&first)["ok"], true);
    let plan = sandbox.plan(vec![sandbox.payload("file", b"replacement")]);
    let mut client = crate::transaction_test::client(&sandbox.root);
    client.transport.peer_uid = nix::unistd::getuid().as_raw();
    let listener = UnixListener::bind(&client.transport.path).unwrap();
    let stage = stage_in(&sandbox.root, &plan.id);
    let helper = helper_name(nix::unistd::getuid().as_raw()).unwrap();
    let daemon = async {
        loop {
            let (peer, _) = listener.accept().await.unwrap();
            let mut peer = BufReader::new(peer);
            let mut line = String::new();
            peer.read_line(&mut line).await.unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            let worker = request["argv"][1] == "environment-worker";
            let reply = if request["op"] == "hello" {
                json!({"ok": true, "user": {"uid": 7001}, "daemon": {"pid": 123},
                    "session": {"managerName": "Aqua", "onConsole": false},
                    "display": {"width": 100, "height": 100, "scale": 1}})
            } else if worker {
                assert_eq!(client.controller.acquire(true).unwrap_err().code, "busy");
                assert!(!client.controller.pending().unwrap().is_empty());
                let output = Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", TEST, "--nocapture"])
                    .env(
                        "ATLAS_ENVIRONMENT_TEST_PACKET",
                        request["argv"][2].as_str().unwrap(),
                    )
                    .output()
                    .await
                    .unwrap();
                assert_eq!(output.status.signal(), Some(libc::SIGKILL));
                assert_eq!(fs::read(sandbox.home.join("file")).unwrap(), b"replacement");
                json!({"ok": true, "exitCode": null, "signal": "SIGKILL",
                    "timedOut": false, "events": []})
            } else {
                assert_eq!(request["op"], "run");
                let proof = json!({"ok": true, "uid": 7001, "hasGraphicAccess": true,
                    "parentPID": 123, "onConsole": false, "securitySessionID": 42,
                    "displayID": 7, "width": 100, "height": 100});
                json!({"ok": true, "exitCode": 0, "timedOut": false,
                    "events": [{"event": "output", "data": STANDARD.encode(proof.to_string())}]})
            };
            peer.get_mut()
                .write_all(format!("{reply}\n").as_bytes())
                .await
                .unwrap();
            if worker {
                break;
            }
        }
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(15), async {
        tokio::join!(
            dispatch(
                &client,
                &helper,
                &stage,
                Request::Apply { plan: plan.clone() }
            ),
            daemon
        )
    })
    .await
    .unwrap();
    assert!(result.unwrap_err().uncertain);
    assert!(requests(&stage));
    assert_eq!(client.controller.pending().unwrap().len(), 1);
    assert_eq!(
        client.controller.acquire(false).unwrap_err().code,
        "uncertain"
    );
    let ownership = client.controller.acquire(true).unwrap();
    assert_eq!(sandbox.apply(&plan)["applied"], true);
    client.controller.clear().unwrap();
    drop(ownership);
    assert!(client.controller.acquire(false).is_ok());
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

    // An uncooperative replacement with identical bytes is still a new
    // inode: the helper's file, which undoing keeps.
    atomic_write(&path, b"one", 0o600).unwrap();
    let result = sandbox.rollback(&plan, true);
    assert_eq!(result["keptTargets"], json!(["file"]), "{result}");
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
            &crate::files::token(),
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
    let mut first = Some(state::Store::open(&sandbox.home, true).unwrap());
    state::Store::open_with_retry(&sandbox.home, true, |_| {
        drop(first.take().expect("the first retry releases the lock"));
        true
    })
    .expect("a lock released on retry is taken");
    assert!(first.is_none(), "the held lock required a retry");

    let held = state::Store::open(&sandbox.home, true).unwrap();
    let mut retries = 0;
    let refused = state::Store::open_with_retry(&sandbox.home, true, |_| {
        retries += 1;
        retries < 2
    })
    .err()
    .unwrap();
    assert_eq!(refused.error, "Another environment operation is running");
    assert_eq!(retries, 2);
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
        "ok": true,
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
    let stage = stage_in(&sandbox.root, &crate::files::token());
    assert!(!requests(&stage));

    fs::write(stage.join("controller.lock"), b"").unwrap();
    assert!(!requests(&stage), "a lock is not a request");

    fs::create_dir(stage.join("read-ab12")).unwrap();
    assert!(
        !requests(&stage),
        "a read left behind has nothing to recover"
    );

    fs::create_dir(stage.join("request-ab12")).unwrap();
    assert!(requests(&stage));
}

/// A stage that cannot be read may hold a plan or a request, so it counts
/// as holding both and is never removed as a preview cut short.
#[test]
fn a_stage_that_cannot_be_read_counts_as_holding_a_plan_and_a_request() {
    let sandbox = Sandbox::new();
    let stage = stage_in(&sandbox.root, &crate::files::token());
    assert!(!planned(&stage) && !requests(&stage));
    assert!(abandoned(&stage));

    fs::set_permissions(&stage, fs::Permissions::from_mode(0o000)).unwrap();
    let (plan, request, cut_short) = (planned(&stage), requests(&stage), abandoned(&stage));
    fs::set_permissions(&stage, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(plan, "a plan that cannot be looked for may be there");
    assert!(request, "a request that cannot be looked for may be there");
    assert!(!cut_short);

    assert!(
        requests(&sandbox.root.join("missing")),
        "a folder that cannot be read"
    );
}

/// A stage is made and locked under a name no sweep looks at, then moved
/// into place: even paused before its lock is taken, long enough to look
/// abandoned, it is never swept.
#[test]
fn a_stage_being_made_is_never_swept_before_it_is_locked() {
    let sandbox = Sandbox::new();
    let account = account();
    let stage = sandbox
        .root
        .join(format!("environment-plan-{}", crate::files::token()));
    let aged = |folder: &Path| {
        let then = std::time::SystemTime::now() - STARTING - Duration::from_secs(60);
        fs::File::open(folder).unwrap().set_modified(then).unwrap();
    };

    let mut paused = None;
    let lock = create_locked_stage_with(&stage, &account, |made| {
        aged(made);
        sweep(&sandbox.root);
        assert!(made.exists(), "swept before its lock was taken");
        assert!(!stage.exists(), "seen under its own name while unlocked");
        paused = Some(made.to_owned());
    })
    .unwrap();

    let made = paused.unwrap();
    assert!(!made.exists(), "moved into place");
    assert!(stage.exists());
    assert!(lock_stage(&stage).is_err(), "its lock is held");
    aged(&stage);
    sweep(&sandbox.root);
    assert!(stage.exists(), "a locked stage is never swept");

    drop(lock);
    sweep(&sandbox.root);
    assert!(!stage.exists(), "swept once its maker let it go");
}

/// A stage is removed by first moving it to a discarded name, so nothing
/// sees it half removed; what such a removal left is cleared later.
#[test]
fn a_discarded_stage_goes_at_once_and_its_leftovers_are_cleared() {
    let sandbox = Sandbox::new();
    let stage = stage_in(&sandbox.root, &crate::files::token());
    fs::write(stage.join("payload-0"), b"copy").unwrap();
    discard(&stage);
    assert!(!stage.exists());
    assert!(
        fs::read_dir(&sandbox.root)
            .unwrap()
            .flatten()
            .all(|entry| !entry.file_name().to_string_lossy().starts_with(DISCARDED)),
        "nothing is left under a discarded name"
    );

    let left = sandbox.root.join(format!("{DISCARDED}ab12"));
    fs::create_dir(&left).unwrap();
    fs::write(left.join("payload-0"), b"copy").unwrap();
    let other = sandbox.root.join("someone-elses");
    fs::create_dir(&other).unwrap();
    clear_discarded(&sandbox.root);
    assert!(!left.exists());
    assert!(other.exists(), "anything else is left alone");
}

/// A week-old preview cut short holds copies but no plan and nothing whose
/// outcome is unknown, so it goes; one with an unknown request, a plan, or
/// that is recent, stays.
#[tokio::test]
async fn a_week_old_preview_cut_short_is_pruned() {
    let sandbox = Sandbox::new();
    let client = crate::transaction_test::client(&sandbox.root);
    let stage = |inside: &[&str]| {
        let id = crate::files::token();
        let stage = stage_in(&sandbox.root, &id);
        fs::write(stage.join("payload-0"), b"copy").unwrap();
        for folder in inside {
            fs::create_dir(stage.join(folder)).unwrap();
        }
        (id, stage)
    };
    let week_old = |stage: &Path| {
        let then = std::time::SystemTime::now() - UNUSED - Duration::from_secs(60);
        fs::File::open(stage).unwrap().set_modified(then).unwrap();
    };

    let (_, cut_short) = stage(&["read-ab12"]);
    let (_, unknown) = stage(&["request-ab12"]);
    let (id, foreign) = stage(&[]);
    write_plan(&foreign, &id, 501);
    let (_, recent) = stage(&[]);
    for old in [&cut_short, &unknown, &foreign] {
        week_old(old);
    }

    prune(&client, "nobody").await;
    assert!(!cut_short.exists());
    assert!(unknown.exists(), "an unknown outcome stays");
    assert!(foreign.exists(), "another helper's plan stays");
    assert!(recent.exists(), "a recent preview stays");
}

/// A preview whose command was killed holds copies but no plan and nothing
/// whose outcome is unknown, so the next environment command removes it
/// once it is older than a preview takes. One being made holds its lock,
/// and one that is recent, has an unknown request or has a plan, stays.
#[tokio::test]
async fn a_preview_cut_short_is_swept_once_no_preview_can_still_be_making_it() {
    let sandbox = Sandbox::new();
    let stage = |inside: &[&str]| {
        let stage = stage_in(&sandbox.root, &crate::files::token());
        fs::write(stage.join("payload-0"), b"copy").unwrap();
        for folder in inside {
            fs::create_dir(stage.join(folder)).unwrap();
        }
        stage
    };
    let aged = |stage: &Path| {
        let then = std::time::SystemTime::now() - STARTING - Duration::from_secs(60);
        fs::File::open(stage).unwrap().set_modified(then).unwrap();
    };

    let cut_short = stage(&["read-ab12"]);
    let bare = stage(&[]);
    let unknown = stage(&["request-ab12"]);
    let planned = stage(&[]);
    let id = planned.file_name().unwrap().to_string_lossy()["environment-plan-".len()..].to_owned();
    write_plan(&planned, &id, 501);
    let recent = stage(&[]);
    let locked = stage(&[]);
    let held = lock_stage(&locked).unwrap();
    for old in [&cut_short, &bare, &unknown, &planned, &locked] {
        aged(old);
    }

    sweep(&sandbox.root);
    assert!(!cut_short.exists(), "a preview cut short mid-read goes");
    assert!(!bare.exists(), "a preview cut short before it read goes");
    assert!(unknown.exists(), "an unknown outcome stays");
    assert!(planned.exists(), "a plan stays");
    assert!(
        !planned.join("controller.lock").exists(),
        "a folder that is kept is not touched"
    );
    assert!(recent.exists(), "a preview just started stays");
    assert!(locked.exists(), "a preview being made stays");

    // Any later environment command sweeps it, here an apply of a plan
    // that does not exist.
    drop(held);
    aged(&locked);
    let mut client = crate::transaction_test::client(&sandbox.root);
    // An account that resolves to a name, as the helper's does.
    client.config.helper_uid = nix::unistd::getuid().as_raw();
    let missing = apply(&client, &crate::files::token()).await.unwrap_err();
    assert!(missing.error.contains("There is no plan"), "{missing}");
    assert!(!locked.exists(), "swept once its lock is let go");
    assert!(unknown.exists() && planned.exists() && recent.exists());
}

#[test]
fn only_an_apply_an_undo_with_apply_and_a_run_change_the_helper() {
    let id = crate::files::token;
    let plan = Plan {
        version: 1,
        id: id(),
        uid: 501,
        home: "/Users/helper".into(),
        files: Vec::new(),
    };

    let preview = Request::Preview {
        id: id(),
        files: Vec::new(),
    };
    assert!(!preview.changes_helper());
    assert!(
        !Request::Rollback {
            id: id(),
            apply: false
        }
        .changes_helper()
    );
    assert!(
        Request::Rollback {
            id: id(),
            apply: true
        }
        .changes_helper()
    );
    assert!(Request::Apply { plan }.changes_helper());
    assert!(
        !Request::Transactions {
            all: true,
            ids: Vec::new()
        }
        .changes_helper()
    );
    assert!(
        Request::Run {
            argv: vec!["cat".into()]
        }
        .changes_helper()
    );
}

/// A preview that began before the helper had any environment state reads
/// without a lock, as there is none to take; an operation that makes the
/// state meanwhile makes it refuse, as one holding the lock would.
#[test]
fn a_preview_refuses_when_the_state_appears_while_it_reads() {
    let sandbox = Sandbox::new();
    let payload = sandbox.payload("file", b"new");
    let guards = Cell::new(0);
    let other_operation = || {
        guards.set(guards.get() + 1);
        if guards.get() == 2 {
            drop(state::Store::open(&sandbox.home, true).unwrap());
        }
        Ok(())
    };

    let refused = worker::preview(
        &sandbox.home,
        nix::unistd::getuid().as_raw(),
        &crate::files::token(),
        vec![payload.clone()],
        &other_operation,
    )
    .unwrap_err();
    assert_eq!(refused.error, "Another environment operation is running");

    let value = sandbox.preview(vec![payload]);
    assert_eq!(
        value["ok"], true,
        "a state that was there all along is read under its lock"
    );
}

/// A rollback's own folder goes once nothing in it is unconfirmed, also
/// after a refusal (a mistyped ID); a frozen plan's folder goes only with
/// a finished undo.
#[test]
fn rollback_removes_its_own_folder_unless_a_request_is_unconfirmed() {
    let sandbox = Sandbox::new();
    let stage = stage_in(&sandbox.root, &crate::files::token());
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

/// A worker's own refusal (an unknown ID, another operation running, a
/// plan that must be finished first) changed nothing, so it is not a
/// transaction to resume; any other failed mutation stays resumable.
#[test]
fn a_refusal_that_changed_nothing_is_not_resumable() {
    let refusal = |error: &str, request: &str| json!({"ok": false, "code": "environment", "error": error, "uncertain": false, "refusal": request});

    for error in [
        worker::MISSING,
        "Another environment operation is running",
        "Transaction 0 must be resumed or rolled back first",
    ] {
        let reply = worker_reply(WORKER_REFUSED, &refusal(error, "request-ab12"));
        let value = decode_reply(&reply, true, "request-ab12").unwrap();
        assert_eq!(value["ok"], false);
        assert!(value["resumeRequired"].is_null(), "{value}");
    }

    let reply = worker_reply(WORKER_REFUSED, &refusal(worker::MISSING, "request-ab12"));
    for (exit, output) in [
        (WORKER_REFUSED, refusal(worker::MISSING, "request-other")),
        (WORKER_REFUSED, json!({"ok": false, "error": "Stopped"})),
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
    let id = crate::files::token();
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
    let id = crate::files::token();
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
    let id = crate::files::token();
    let plan = stage_in(&sandbox.root, &id);
    let run = sandbox.root.join(format!("environment-run-{id}"));
    fs::create_dir(&run).unwrap();
    fs::create_dir(sandbox.root.join("environment-plan-short")).unwrap();
    fs::create_dir(sandbox.root.join(format!("other-{id}"))).unwrap();
    let file = crate::files::token();
    fs::write(sandbox.root.join(format!("environment-plan-{file}")), b"").unwrap();

    let linked = crate::files::token();
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
    let id = crate::files::token();
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
    assert!(!prunable(&stage, &crate::files::token(), 501));

    fs::create_dir(stage.join("request-ab12")).unwrap();
    assert!(!prunable(&stage, &id, 501));
}

/// A command's folder, kept when the command was stopped or its reply was
/// lost, goes once it is older than a week.
#[test]
fn old_command_folders_are_pruned() {
    let sandbox = Sandbox::new();
    let id = crate::files::token();
    let run = sandbox.root.join(format!("environment-run-{id}"));
    fs::create_dir(&run).unwrap();
    stage_in(&sandbox.root, &crate::files::token());
    std::thread::sleep(std::time::Duration::from_millis(20));

    assert_eq!(
        stale_runs(&sandbox.root, std::time::Duration::ZERO),
        vec![run.clone()]
    );
    assert!(stale_runs(&sandbox.root, UNUSED).is_empty());

    prune_runs(&sandbox.root);
    assert!(run.exists(), "a recent command's folder stays");
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

    let stage = stage_in(&sandbox.root, &crate::files::token());
    let request = Request::Rollback {
        id: crate::files::token(),
        apply: true,
    };
    assert!(dispatch(&client, "nobody", &stage, request).await.is_err());
    assert_eq!(fs::read_dir(&stage).unwrap().count(), 0);

    // A folder the helper cannot be given goes too.
    let refused = sandbox.root.join("refused");
    assert!(create_stage(&refused, "atlas-no-such-user-zz").is_err());
    assert!(!refused.exists());
}

/// A request reads which helper it expects only once it has control, so a
/// wait for control never leaves it expecting a helper since restarted.
#[tokio::test]
async fn an_environment_request_reads_the_helper_only_once_it_has_control() {
    let sandbox = Sandbox::new();
    let client = crate::transaction_test::client(&sandbox.root);
    let daemon = tokio::net::UnixListener::bind(&client.transport.path).unwrap();
    let _held = client.controller.acquire(false).unwrap();

    let stage = stage_in(&sandbox.root, &crate::files::token());
    let request = Request::Rollback {
        id: crate::files::token(),
        apply: true,
    };
    let refused = dispatch_within(&client, "nobody", &stage, request, Duration::from_secs(1))
        .await
        .unwrap_err();
    assert_eq!(refused.code, "busy");
    assert!(
        tokio::time::timeout(Duration::from_millis(100), daemon.accept())
            .await
            .is_err(),
        "the helper was not asked"
    );
}

/// The account requests are shared with: the one running the test, which
/// also runs the stand-in helper.
fn account() -> String {
    helper_name(nix::unistd::getuid().as_raw()).unwrap()
}

/// Answer a worker request as the helper would for a preview or an undo
/// preview, by running it on `home` in this process.
fn previewing(home: &Path) -> impl FnMut(&Value) -> Option<Value> + '_ {
    move |request| {
        let argv = request["argv"].as_array().unwrap();
        assert_eq!(argv.len(), 3, "a preview is never sent with --apply");
        let folder = Path::new(argv[2].as_str().unwrap()).parent().unwrap();
        assert!(
            folder
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(READ),
            "a read is never mistaken for a step whose outcome is unknown"
        );
        let packet: Packet = decode(&fs::read(argv[2].as_str().unwrap()).unwrap()).unwrap();
        let value = match packet.request {
            Request::Preview { id, files } => {
                worker::preview(home, nix::unistd::getuid().as_raw(), &id, files, &|| Ok(()))
            }
            Request::Transactions { all, ids } => worker::transactions(home, all, &ids, &|| Ok(())),
            Request::Rollback { id, apply: false } => {
                worker::rollback(home, &id, false, &|| Ok(()))
            }
            _ => panic!("only previews are answered"),
        };

        Some(worker_reply(0, &value.unwrap()))
    }
}

#[tokio::test]
async fn a_preview_works_while_another_controller_holds_the_lock_and_leaves_no_marker() {
    let sandbox = Sandbox::new();
    let (client, helper) = Helper::client(&sandbox.root, UID);
    let account = account();
    let _held = client.controller.acquire(false).unwrap();
    let id = crate::files::token();
    let stage = stage_in(&sandbox.root, &id);
    let request = Request::Preview {
        id: id.clone(),
        files: vec![sandbox.payload("file", b"new")],
    };

    let value = helper
        .serve(
            dispatch(&client, &account, &stage, request),
            previewing(&sandbox.home),
        )
        .await
        .unwrap();
    assert_eq!(value["ok"], true, "{value}");
    assert_eq!(value["planId"], id);
    assert!(!sandbox.home.join("file").exists());
    assert!(client.controller.pending().unwrap().is_empty());
    assert!(!requests(&stage));
    assert!(!client.config.output_dir.join("metadata.jsonl").exists());
}

#[tokio::test]
async fn a_lost_preview_reply_leaves_no_marker() {
    let sandbox = Sandbox::new();
    let (client, helper) = Helper::client(&sandbox.root, UID);
    let account = account();
    let id = crate::files::token();
    let stage = stage_in(&sandbox.root, &id);
    let request = Request::Preview {
        id,
        files: vec![sandbox.payload("file", b"new")],
    };

    let error = helper
        .serve(dispatch(&client, &account, &stage, request), |_| None)
        .await
        .unwrap_err();
    assert!(!error.uncertain, "{error:?}");
    assert!(client.controller.pending().unwrap().is_empty());
    assert!(!requests(&stage), "the request folder goes");
}

#[tokio::test]
async fn an_undo_preview_takes_no_control() {
    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![sandbox.payload("file", b"copied")]);
    assert_eq!(sandbox.apply(&plan)["ok"], true);
    let (client, helper) = Helper::client(&sandbox.root, UID);
    let account = account();
    let _held = client.controller.acquire(false).unwrap();
    let stage = stage_in(&sandbox.root, &crate::files::token());
    let request = Request::Rollback {
        id: plan.id.clone(),
        apply: false,
    };

    let value = helper
        .serve(
            dispatch(&client, &account, &stage, request),
            previewing(&sandbox.home),
        )
        .await
        .unwrap();
    assert_eq!(value["pendingTargets"], json!(["file"]), "{value}");
    assert_eq!(fs::read(sandbox.home.join("file")).unwrap(), b"copied");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn an_apply_still_takes_control_and_stays_pending_when_its_reply_is_lost() {
    const TEST: &str =
        "environment::tests::an_apply_still_takes_control_and_stays_pending_when_its_reply_is_lost";
    if !crate::transaction_test::isolated(TEST) {
        return;
    }

    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![sandbox.payload("file", b"copied")]);
    let (client, helper) = Helper::client(&sandbox.root, UID);
    let account = account();
    let stage = stage_in(&sandbox.root, &plan.id);
    let apply = || Request::Apply { plan: plan.clone() };

    let held = client.controller.acquire(false).unwrap();
    let refused = dispatch(&client, &account, &stage, apply())
        .await
        .unwrap_err();
    assert_eq!(refused.code, "busy");
    drop(held);

    let error = helper
        .serve(dispatch(&client, &account, &stage, apply()), |_| None)
        .await
        .unwrap_err();
    assert!(error.uncertain, "{error:?}");
    assert!(requests(&stage), "its request is kept");
    assert!(!client.controller.pending().unwrap().is_empty());
    assert_eq!(
        client.controller.acquire(false).unwrap_err().code,
        "uncertain"
    );
}

/// A preview holds the helper's store shared while it reads; an apply that
/// comes then is refused before it changes anything, so there is nothing to
/// resume and it can simply be repeated.
#[test]
fn an_apply_refused_by_a_running_preview_is_not_resumable() {
    let sandbox = Sandbox::new();
    let first = sandbox.plan(vec![sandbox.payload("file", b"original")]);
    assert_eq!(sandbox.apply(&first)["ok"], true);
    let plan = sandbox.plan(vec![sandbox.payload("file", b"replacement")]);

    let preview = state::Store::open(&sandbox.home, false).unwrap();
    let error = worker::apply_plan(&sandbox.home, &plan, &|| Ok(())).unwrap_err();
    drop(preview);
    assert_eq!(error.error, "Another environment operation is running");
    assert_eq!(fs::read(sandbox.home.join("file")).unwrap(), b"original");

    let mut refusal = json!(error);
    refusal["refusal"] = json!("request-ab12");
    let reply = worker_reply(WORKER_REFUSED, &refusal);
    let value = decode_reply(&reply, true, "request-ab12").unwrap();
    assert_eq!(value["ok"], false);
    assert!(value["resumeRequired"].is_null(), "{value}");
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

#[test]
fn rollback_keeps_a_helper_edit_to_a_file_the_plan_never_installed() {
    // Stop once the first file is in place: before the second is prepared,
    // and after its copy waits beside it.
    for prepared in [false, true] {
        let sandbox = Sandbox::new();
        let plan = sandbox.plan(vec![
            sandbox.payload("first", b"one"),
            sandbox.payload("second", b"two"),
        ]);
        let waiting = sandbox.home.join(format!(".atlas-{}-1-install", plan.id));

        let stopped = Cell::new(false);
        let result = worker::apply_plan(&sandbox.home, &plan, &|| {
            let ready = if prepared {
                waiting.exists()
            } else {
                sandbox.home.join("first").exists()
            };
            if ready && !stopped.replace(true) {
                return Err(problem("session changed"));
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(result["ok"], false, "{result}");
        assert_eq!(waiting.exists(), prepared);

        fs::write(sandbox.home.join("second"), b"helper edit").unwrap();

        let resumed = sandbox.apply(&plan);
        assert_eq!(resumed["ok"], false);
        assert!(
            resumed["error"]
                .as_str()
                .unwrap()
                .contains("atlas environment rollback"),
            "{resumed}"
        );

        let result = sandbox.rollback(&plan, true);
        assert_eq!(result["ok"], true, "{result}");
        assert_eq!(result["rolledBack"], true);
        assert!(!sandbox.home.join("first").exists());
        assert_eq!(
            fs::read(sandbox.home.join("second")).unwrap(),
            b"helper edit"
        );
        assert!(!waiting.exists());

        let next = sandbox.plan(vec![sandbox.payload("next", b"next")]);
        assert_eq!(sandbox.apply(&next)["ok"], true);
    }
}

#[test]
fn a_target_inside_or_around_a_copied_file_is_refused_before_it_wedges_the_ledger() {
    let sandbox = Sandbox::new();
    let nested = sandbox.plan(vec![sandbox.payload("x/y", b"nested")]);
    let around = sandbox.plan(vec![sandbox.payload("x", b"file")]);
    assert_eq!(sandbox.apply(&nested)["ok"], true);

    fs::remove_dir_all(sandbox.home.join("x")).unwrap();
    let result = worker::preview(
        &sandbox.home,
        nix::unistd::getuid().as_raw(),
        &crate::files::token(),
        vec![sandbox.payload("x", b"file")],
        &|| Ok(()),
    );
    assert!(result.unwrap_err().error.contains("overlaps x/y"));

    assert!(worker::apply_plan(&sandbox.home, &around, &|| Ok(())).is_err());
    assert!(!sandbox.home.join("x").exists());

    let next = sandbox.plan(vec![sandbox.payload("next", b"next")]);
    assert_eq!(sandbox.apply(&next)["ok"], true);
}

#[test]
fn a_finished_rollback_keeps_only_its_record() {
    let sandbox = Sandbox::new();
    let first = sandbox.plan(vec![sandbox.payload("secret", b"old token")]);
    assert_eq!(sandbox.apply(&first)["ok"], true);

    let plan = sandbox.plan(vec![sandbox.payload("secret", b"new token")]);
    assert_eq!(sandbox.apply(&plan)["ok"], true);

    let folder = sandbox
        .home
        .join(STATE)
        .join(format!("transaction-{}", plan.id));
    let names = || {
        let mut names: Vec<_> = fs::read_dir(&folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    };
    assert_eq!(names(), ["after-0", "before-0", "transaction.json"]);

    assert_eq!(sandbox.rollback(&plan, true)["ok"], true);
    assert_eq!(fs::read(sandbox.home.join("secret")).unwrap(), b"old token");
    assert_eq!(names(), ["transaction.json"]);
    assert_eq!(sandbox.rollback(&plan, true)["rolledBack"], true);
}

#[test]
fn retrying_a_finished_rollback_clears_its_active_marker_before_another_profile_applies() {
    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![sandbox.payload("file", b"first")]);
    assert_eq!(sandbox.apply(&plan)["applied"], true);
    assert_eq!(sandbox.rollback(&plan, true)["rolledBack"], true);

    let store = state::Store::open(&sandbox.home, true).unwrap();
    assert_eq!(
        store.transaction(&plan.id).unwrap().unwrap().phase,
        Phase::RolledBack
    );
    store.activate(&plan.id).unwrap();
    atomic_write(&store.blob(&plan.id, 0, false), b"first", 0o600).unwrap();
    drop(store);

    assert_eq!(sandbox.rollback(&plan, false)["rolledBack"], true);
    let active = sandbox.home.join(STATE).join("active.json");
    assert!(active.exists());
    let calls = Cell::new(0);
    let refused = worker::rollback(&sandbox.home, &plan.id, true, &|| {
        calls.set(calls.get() + 1);
        if calls.get() == 2 {
            return Err(problem("session changed"));
        }
        Ok(())
    });
    assert!(refused.is_err());
    assert!(active.exists());
    assert!(
        sandbox
            .home
            .join(STATE)
            .join(format!("transaction-{}/after-0", plan.id))
            .exists()
    );

    assert_eq!(sandbox.rollback(&plan, true)["rolledBack"], true);
    assert!(!active.exists());
    let next = sandbox.plan(vec![sandbox.payload("file", b"second")]);
    assert_eq!(sandbox.apply(&next)["applied"], true);
    assert_eq!(fs::read(sandbox.home.join("file")).unwrap(), b"second");
}

#[test]
fn copies_of_a_preparation_that_stopped_part_way_are_removed() {
    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![
        sandbox.payload("first", b"one"),
        sandbox.payload("second", b"two"),
    ]);
    let folder = sandbox
        .home
        .join(STATE)
        .join(format!("transaction-{}", plan.id));

    let result = worker::apply_plan(&sandbox.home, &plan, &|| {
        if folder.join("after-0").exists() {
            return Err(problem("session changed"));
        }
        Ok(())
    });
    assert!(result.is_err());
    assert!(!folder.exists());

    // A preparation killed outright leaves its folder until the next one.
    let orphan = sandbox
        .home
        .join(STATE)
        .join(format!("transaction-{}", crate::files::token()));
    fs::create_dir(&orphan).unwrap();
    fs::write(orphan.join("before-0"), b"secret").unwrap();

    assert_eq!(sandbox.apply(&plan)["ok"], true);
    assert!(!orphan.exists());
}

/// Stop `plan`'s apply at the first guard where `stop` holds.
fn stop_apply(sandbox: &Sandbox, plan: &Plan, stop: impl Fn() -> bool) -> Value {
    let stopped = Cell::new(false);

    worker::apply_plan(&sandbox.home, plan, &|| {
        if stop() && !stopped.replace(true) {
            return Err(problem("session changed"));
        }
        Ok(())
    })
    .unwrap()
}

/// A provenance value no process of this test makes.
fn foreign_provenance() -> BTreeMap<String, String> {
    BTreeMap::from([(
        "com.apple.provenance".to_owned(),
        STANDARD.encode([1_u8, 2, 0, 0x83, 0x32, 0x51, 0x45, 0x74, 0x26, 0x9a, 0xda]),
    )])
}

/// macOS sets com.apple.provenance itself and ignores attempts to change it,
/// so restores leave it alone and fingerprints do not compare it.
#[test]
fn provenance_is_never_restored_or_compared() {
    let sandbox = Sandbox::new();
    let path = sandbox.home.join("file");
    atomic_write(&path, b"data", 0o600).unwrap();

    restore_attributes(&path, &foreign_provenance()).unwrap();
    restore_attributes(&path, &BTreeMap::new()).unwrap();

    let own = fingerprint(&path).unwrap().unwrap();
    let mut written_by_an_app = own.clone();
    written_by_an_app.xattrs = foreign_provenance();
    assert_eq!(own, written_by_an_app);

    written_by_an_app
        .xattrs
        .insert("user.atlas-test".into(), STANDARD.encode([1_u8]));
    assert_ne!(own, written_by_an_app, "other attributes still count");
}

#[test]
fn fingerprint_comparisons_ignore_only_their_intended_fields() {
    let original = Fingerprint {
        node: Node {
            device: 1,
            inode: 2,
        },
        sha256: digest(b"data"),
        size: 4,
        mode: 0o600,
        uid: 501,
        gid: 20,
        acl: false,
        xattrs: BTreeMap::from([
            ("user.atlas-test".to_owned(), STANDARD.encode(b"owned")),
            (
                "com.apple.provenance".to_owned(),
                STANDARD.encode(b"system"),
            ),
        ]),
        flags: 0,
    };
    assert_eq!(original, original.clone());
    assert!(original.same_contents_and_metadata(&original));

    for field in [
        "device",
        "inode",
        "sha256",
        "size",
        "mode",
        "uid",
        "gid",
        "acl",
        "xattrs",
        "flags",
        "provenance",
    ] {
        let mut changed = original.clone();
        match field {
            "device" => changed.node.device += 1,
            "inode" => changed.node.inode += 1,
            "sha256" => changed.sha256 = digest(b"else"),
            "size" => changed.size += 1,
            "mode" => changed.mode = 0o700,
            "uid" => changed.uid += 1,
            "gid" => changed.gid += 1,
            "acl" => changed.acl = true,
            "flags" => changed.flags = 1,
            "xattrs" | "provenance" => {
                let key = if field == "xattrs" {
                    "user.atlas-test"
                } else {
                    "com.apple.provenance"
                };
                changed
                    .xattrs
                    .insert(key.into(), STANDARD.encode(b"changed"));
            }
            _ => unreachable!(),
        }
        for (left, right) in [(&original, &changed), (&changed, &original)] {
            assert_eq!(left == right, field == "provenance", "{field}");
            assert_eq!(
                left.same_contents_and_metadata(right),
                matches!(field, "device" | "inode" | "provenance"),
                "{field}"
            );
        }
    }
}

/// A file an app in the helper wrote carries that app's provenance. A copy
/// that only fixes its permissions, and undoing it, still finish.
#[test]
fn a_file_an_app_wrote_can_be_copied_over_and_restored() {
    let sandbox = Sandbox::new();
    let path = sandbox.home.join("settings");
    atomic_write(&path, b"same", 0o644).unwrap();

    let mut plan = sandbox.plan(vec![sandbox.payload("settings", b"same")]);
    assert_eq!(plan.files[0].operation, Operation::Permissions);
    plan.files[0].before.as_mut().unwrap().xattrs = foreign_provenance();

    let result = sandbox.apply(&plan);
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);

    let other = sandbox.plan(vec![sandbox.payload("other", b"other")]);
    assert_eq!(sandbox.apply(&other)["ok"], true);
    assert_eq!(sandbox.rollback(&other, true)["rolledBack"], true);

    let result = sandbox.rollback(&plan, true);
    assert_eq!(result["rolledBack"], true, "{result}");
    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o644);
}

/// New contents never take over the download mark of the file they
/// replace, which would stop a copied tool from running; undoing puts the
/// earlier file back with it.
#[test]
fn new_contents_do_not_inherit_the_download_mark() {
    let sandbox = Sandbox::new();
    let path = sandbox.home.join("tool");
    atomic_write(&path, b"old", 0o700).unwrap();
    let mut attributes = fingerprint(&path).unwrap().unwrap().xattrs;
    attributes.insert(
        "com.apple.quarantine".into(),
        STANDARD.encode("0081;00000000;Safari;"),
    );
    restore_attributes(&path, &attributes).unwrap();

    let state = state::Store::open(&sandbox.home, true).unwrap();
    state
        .save_ledger(&Ledger {
            version: 0,
            files: BTreeMap::from([(PathBuf::from("tool"), Entry::Legacy(digest(b"old")))]),
        })
        .unwrap();
    drop(state);

    let mut payload = sandbox.payload("tool", b"new");
    payload.executable = true;
    let plan = sandbox.plan(vec![payload]);
    assert_eq!(plan.files[0].operation, Operation::Update);

    assert_eq!(sandbox.apply(&plan)["ok"], true);
    let copied = fingerprint(&path).unwrap().unwrap().xattrs;
    assert!(!copied.contains_key("com.apple.quarantine"), "{copied:?}");

    assert_eq!(sandbox.rollback(&plan, true)["rolledBack"], true);
    assert_eq!(fs::read(&path).unwrap(), b"old");
    assert!(
        fingerprint(&path)
            .unwrap()
            .unwrap()
            .xattrs
            .contains_key("com.apple.quarantine")
    );
}

/// A file the helper changed while a copy was part way does not block the
/// helper: undoing keeps that file, undoes the rest and releases the plan.
#[test]
fn a_file_changed_part_way_is_kept_and_the_rest_undone() {
    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![
        sandbox.payload("first", b"one"),
        sandbox.payload("second", b"two"),
    ]);

    let state = sandbox.home.join(STATE);
    let result = stop_apply(&sandbox, &plan, || {
        fs::read_to_string(state.join("files.json")).is_ok_and(|ledger| ledger.contains("first"))
    });
    assert_eq!(result["completedTargets"], json!(["first"]), "{result}");

    fs::write(sandbox.home.join("first"), b"app edit").unwrap();

    let resumed = sandbox.apply(&plan);
    assert_eq!(resumed["ok"], false);
    assert_eq!(resumed["rollbackRequired"], true, "{resumed}");
    assert!(
        resumed["error"]
            .as_str()
            .unwrap()
            .contains("changed in the helper after this plan copied it"),
        "{resumed}"
    );

    let preview = sandbox.rollback(&plan, false);
    assert_eq!(preview["ok"], true, "{preview}");
    assert_eq!(preview["keptTargets"], json!(["first"]));
    assert_eq!(preview["pendingTargets"], json!([]));

    let result = sandbox.rollback(&plan, true);
    assert_eq!(result["rolledBack"], true, "{result}");
    assert_eq!(result["keptTargets"], json!(["first"]));
    assert_eq!(result["completedTargets"], json!(["second"]));
    assert_eq!(fs::read(sandbox.home.join("first")).unwrap(), b"app edit");
    assert!(!sandbox.home.join("second").exists());

    // The kept file is the helper's now: a new copy names it a conflict.
    let next = sandbox.preview(vec![sandbox.payload("first", b"one")]);
    assert_eq!(next["files"][0]["operation"], "conflict");
    let next = sandbox.plan(vec![sandbox.payload("next", b"next")]);
    assert_eq!(sandbox.apply(&next)["ok"], true);
}

/// Undoing a finished copy keeps the one file the helper changed since and
/// puts every other one back.
#[test]
fn undoing_a_copy_keeps_only_the_file_the_helper_changed() {
    let sandbox = Sandbox::new();
    fs::write(sandbox.home.join("kept"), b"before").unwrap();
    let earlier = sandbox.plan(vec![sandbox.payload("kept", b"before")]);
    assert_eq!(sandbox.apply(&earlier)["ok"], true);

    let plan = sandbox.plan(vec![
        sandbox.payload("kept", b"copied"),
        sandbox.payload("created", b"created"),
    ]);
    assert_eq!(sandbox.apply(&plan)["ok"], true);
    fs::write(sandbox.home.join("kept"), b"app edit").unwrap();

    let preview = sandbox.rollback(&plan, false);
    assert_eq!(preview["pendingTargets"], json!(["created"]), "{preview}");
    assert_eq!(preview["keptTargets"], json!(["kept"]));

    let result = sandbox.rollback(&plan, true);
    assert_eq!(result["rolledBack"], true, "{result}");
    assert_eq!(fs::read(sandbox.home.join("kept")).unwrap(), b"app edit");
    assert!(!sandbox.home.join("created").exists());

    // The earlier copy's file changed too, so undoing it keeps it as well.
    let result = sandbox.rollback(&earlier, true);
    assert_eq!(result["rolledBack"], true, "{result}");
    assert_eq!(result["keptTargets"], json!(["kept"]));
    assert_eq!(fs::read(sandbox.home.join("kept")).unwrap(), b"app edit");
}

/// A later plan's copy of the same file is that plan's to undo first.
#[test]
fn a_file_a_later_plan_copied_again_still_blocks_the_older_undo() {
    let sandbox = Sandbox::new();
    let first = sandbox.plan(vec![sandbox.payload("file", b"one")]);
    assert_eq!(sandbox.apply(&first)["ok"], true);

    let second = sandbox.plan(vec![sandbox.payload("file", b"two")]);
    assert_eq!(sandbox.apply(&second)["ok"], true);

    let result = sandbox.rollback(&first, true);
    assert_eq!(result["ok"], false, "{result}");
    assert!(
        result["error"]
            .as_str()
            .unwrap()
            .contains("roll that back first")
    );
    assert_eq!(fs::read(sandbox.home.join("file")).unwrap(), b"two");
}

/// A preview of an undo leaves out files it would not restore or remove:
/// ones the copy found already in place, and ones it never put in place.
#[test]
fn an_undo_preview_names_only_files_it_restores_or_removes() {
    let sandbox = Sandbox::new();
    fs::write(sandbox.home.join("there"), b"same").unwrap();
    fs::set_permissions(
        sandbox.home.join("there"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();

    let plan = sandbox.plan(vec![
        sandbox.payload("there", b"same"),
        sandbox.payload("first", b"one"),
        sandbox.payload("second", b"two"),
    ]);
    assert_eq!(plan.files[0].operation, Operation::Unchanged);

    let first = sandbox.home.join("first");
    let result = stop_apply(&sandbox, &plan, || first.exists());
    assert_eq!(result["ok"], false, "{result}");

    let preview = sandbox.rollback(&plan, false);
    assert_eq!(preview["pendingTargets"], json!(["first"]), "{preview}");
    assert_eq!(preview["keptTargets"], json!([]));

    assert_eq!(sandbox.rollback(&plan, true)["rolledBack"], true);
    assert_eq!(fs::read(sandbox.home.join("there")).unwrap(), b"same");
}

/// A folder apply could not put a file into is refused by the preview, and
/// by apply before it starts, never after it has begun.
#[test]
fn a_shared_folder_is_refused_before_anything_starts() {
    let sandbox = Sandbox::new();
    let shared = sandbox.home.join("shared");
    fs::create_dir(&shared).unwrap();
    fs::set_permissions(&shared, fs::Permissions::from_mode(0o775)).unwrap();

    let refused = worker::preview(
        &sandbox.home,
        nix::unistd::getuid().as_raw(),
        &crate::files::token(),
        vec![sandbox.payload("shared/inner/file", b"x")],
        &|| Ok(()),
    )
    .unwrap_err();
    assert!(
        refused
            .error
            .contains("~/shared is not a folder only the helper"),
        "{}",
        refused.error
    );

    fs::set_permissions(&shared, fs::Permissions::from_mode(0o755)).unwrap();
    let plan = sandbox.plan(vec![sandbox.payload("shared/file", b"x")]);
    fs::set_permissions(&shared, fs::Permissions::from_mode(0o775)).unwrap();

    assert!(worker::apply_plan(&sandbox.home, &plan, &|| Ok(())).is_err());
    assert!(!sandbox.home.join(STATE).join("active.json").exists());
    assert!(!shared.join("file").exists());

    let next = sandbox.plan(vec![sandbox.payload("next", b"next")]);
    assert_eq!(sandbox.apply(&next)["ok"], true);
}

/// A plan stopped after it was prepared but before it began is not active,
/// so another plan may copy the same file. The stopped one then never
/// touched it: undoing it releases its copies and keeps the other's file.
#[test]
fn a_plan_stopped_before_it_began_can_be_undone_after_another_copied_its_file() {
    let sandbox = Sandbox::new();
    let stopped = sandbox.plan(vec![sandbox.payload("file", b"stopped")]);

    let folder = sandbox
        .home
        .join(STATE)
        .join(format!("transaction-{}", stopped.id));
    let active = sandbox.home.join(STATE).join("active.json");
    let result = stop_apply(&sandbox, &stopped, || {
        folder.join("transaction.json").exists() && !active.exists()
    });
    assert_eq!(result["ok"], false, "{result}");
    assert!(!active.exists());

    let other = sandbox.plan(vec![sandbox.payload("file", b"other")]);
    assert_eq!(sandbox.apply(&other)["ok"], true);

    let resumed = sandbox.apply(&stopped);
    assert_eq!(resumed["rollbackRequired"], true, "{resumed}");
    assert!(
        !active.exists(),
        "a plan that cannot finish never activates"
    );

    let result = sandbox.rollback(&stopped, true);
    assert_eq!(result["rolledBack"], true, "{result}");
    assert_eq!(fs::read(sandbox.home.join("file")).unwrap(), b"other");

    let mut names: Vec<_> = fs::read_dir(&folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, ["transaction.json"]);

    assert_eq!(sandbox.rollback(&other, true)["rolledBack"], true);
    assert!(!sandbox.home.join("file").exists());
}

/// A write stopped part way leaves its file under a known name, which the
/// next write or an undo removes; a refused exclusive write leaves nothing.
#[test]
fn an_interrupted_write_leaves_nothing_behind() {
    let sandbox = Sandbox::new();
    let path = sandbox.home.join("record");
    let partial = sandbox.home.join(".atlas-partial-record");

    fs::write(&partial, b"half").unwrap();
    atomic_write(&path, b"whole", 0o600).unwrap();
    assert!(!partial.exists());
    assert_eq!(fs::read(&path).unwrap(), b"whole");

    assert!(atomic_install(&path, b"other", 0o600, true).is_err());
    assert_eq!(fs::read_dir(&sandbox.home).unwrap().count(), 1);
    assert_eq!(fs::metadata(&path).unwrap().nlink(), 1);

    let plan = sandbox.plan(vec![sandbox.payload("dir/file", b"one")]);
    let folder = sandbox.home.join("dir");
    let result = stop_apply(&sandbox, &plan, || folder.exists());
    assert_eq!(result["ok"], false, "{result}");

    let waiting = sandbox
        .home
        .join(format!("dir/.atlas-partial-.atlas-{}-0-install", plan.id));
    fs::write(&waiting, b"half").unwrap();

    let result = sandbox.rollback(&plan, true);
    assert_eq!(result["rolledBack"], true, "{result}");
    assert!(!sandbox.home.join("dir").exists());
}

#[test]
fn every_rollback_guard_failure_with_a_kept_file_is_resumable() {
    let applied = || {
        let sandbox = Sandbox::new();
        let plan = sandbox.plan(vec![
            sandbox.payload("nested/kept", b"one"),
            sandbox.payload("nested/undone", b"two"),
        ]);
        assert_eq!(sandbox.apply(&plan)["ok"], true);
        fs::write(sandbox.home.join("nested/kept"), b"app edit").unwrap();
        (sandbox, plan)
    };

    let (baseline, baseline_plan) = applied();
    let calls = Cell::new(0);
    worker::rollback(&baseline.home, &baseline_plan.id, true, &|| {
        calls.set(calls.get() + 1);
        Ok(())
    })
    .unwrap();

    for fail_at in 1..=calls.get() {
        let (sandbox, plan) = applied();
        let current = Cell::new(0);
        let _ = worker::rollback(&sandbox.home, &plan.id, true, &|| {
            current.set(current.get() + 1);
            if current.get() == fail_at {
                Err(problem("session changed"))
            } else {
                Ok(())
            }
        });

        let result = sandbox.rollback(&plan, true);
        assert_eq!(result["rolledBack"], true, "guard {fail_at}: {result}");
        assert_eq!(result["keptTargets"], json!(["nested/kept"]));
        assert_eq!(
            fs::read(sandbox.home.join("nested/kept")).unwrap(),
            b"app edit"
        );
        assert!(!sandbox.home.join("nested/undone").exists());
        assert_eq!(
            fs::read_dir(sandbox.home.join("nested")).unwrap().count(),
            1,
            "guard {fail_at}: no waiting copy is left"
        );
    }
}

/// Keeping a file never needs to change its folder, so a folder the helper
/// shared since does not stop the undo.
#[test]
fn a_kept_file_in_a_folder_shared_since_still_finishes() {
    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![sandbox.payload("dir/file", b"one")]);
    assert_eq!(sandbox.apply(&plan)["ok"], true);

    let folder = sandbox.home.join("dir");
    fs::write(folder.join("file"), b"app edit").unwrap();
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o775)).unwrap();

    let result = sandbox.rollback(&plan, true);
    assert_eq!(result["rolledBack"], true, "{result}");
    assert_eq!(result["keptTargets"], json!(["dir/file"]));
    assert_eq!(result["retainedDirectories"], json!(["dir"]));
}

#[test]
fn an_adopted_file_a_later_undo_put_back_can_still_be_undone() {
    let sandbox = Sandbox::new();
    let path = sandbox.home.join("existing");
    atomic_write(&path, b"same", 0o600).unwrap();
    let adopt = sandbox.plan(vec![sandbox.payload("existing", b"same")]);
    assert_eq!(adopt.files[0].operation, Operation::Unchanged);
    assert_eq!(sandbox.apply(&adopt)["ok"], true);

    let update = sandbox.plan(vec![sandbox.payload("existing", b"changed")]);
    assert_eq!(update.files[0].operation, Operation::Update);
    assert_eq!(sandbox.apply(&update)["ok"], true);

    let undone = sandbox.rollback(&update, true);
    assert_eq!(undone["ok"], true, "{undone}");
    assert_eq!(fs::read(&path).unwrap(), b"same");
    let inode = fs::metadata(&path).unwrap().ino();

    let result = sandbox.rollback(&adopt, true);
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["rolledBack"], true, "{result}");
    assert_eq!(fs::read(&path).unwrap(), b"same");
    assert_eq!(fs::metadata(&path).unwrap().ino(), inode);

    let store = state::Store::open(&sandbox.home, false).unwrap();
    assert!(
        !store
            .ledger()
            .unwrap()
            .files
            .contains_key(Path::new("existing"))
    );
    drop(store);

    sandbox.plan(vec![sandbox.payload("other", b"x")]);
}

#[test]
fn a_restore_copy_written_before_it_was_recorded_goes_when_a_helper_edit_is_kept() {
    let sandbox = Sandbox::new();
    let path = sandbox.home.join("file");
    let first = sandbox.plan(vec![sandbox.payload("file", b"old")]);
    assert_eq!(sandbox.apply(&first)["ok"], true);
    let second = sandbox.plan(vec![sandbox.payload("file", b"new")]);
    assert_eq!(sandbox.apply(&second)["ok"], true);

    // The undo stopped right after writing its restore copy, before
    // recording it.
    let store = state::Store::open(&sandbox.home, true).unwrap();
    let mut transaction = store.transaction(&second.id).unwrap().unwrap();
    transaction.phase = Phase::RollingBack;
    store.save(&transaction).unwrap();
    store.activate(&second.id).unwrap();
    drop(store);

    let restore = sandbox.home.join(format!(".atlas-{}-0-restore", second.id));
    atomic_write(&restore, b"old", 0o600).unwrap();
    fs::write(&path, b"helper edit").unwrap();

    let result = sandbox.rollback(&second, true);
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["keptTargets"], json!(["file"]));
    assert_eq!(fs::read(&path).unwrap(), b"helper edit");
    assert!(!restore.exists());

    sandbox.plan(vec![sandbox.payload("other", b"x")]);
}

#[test]
fn an_unrecorded_restore_copy_with_other_contents_is_kept_for_inspection() {
    let sandbox = Sandbox::new();
    let path = sandbox.home.join("file");
    let first = sandbox.plan(vec![sandbox.payload("file", b"old")]);
    assert_eq!(sandbox.apply(&first)["ok"], true);
    let second = sandbox.plan(vec![sandbox.payload("file", b"new")]);
    assert_eq!(sandbox.apply(&second)["ok"], true);

    let store = state::Store::open(&sandbox.home, true).unwrap();
    let mut transaction = store.transaction(&second.id).unwrap().unwrap();
    transaction.phase = Phase::RollingBack;
    store.save(&transaction).unwrap();
    store.activate(&second.id).unwrap();
    drop(store);

    let restore = sandbox.home.join(format!(".atlas-{}-0-restore", second.id));
    atomic_write(&restore, b"something else", 0o600).unwrap();
    fs::write(&path, b"helper edit").unwrap();

    let result = sandbox.rollback(&second, true);
    assert_eq!(result["ok"], false, "{result}");
    assert_eq!(fs::read(&restore).unwrap(), b"something else");
}

impl Sandbox {
    /// The helper's list of its transactions, as `list` asks for it.
    fn transactions(&self) -> Value {
        self.transactions_of(false, &[])
    }

    /// The helper's list, with those rolled back when `all`, and each of
    /// `ids` whatever its phase.
    fn transactions_of(&self, all: bool, ids: &[String]) -> Value {
        worker::transactions(&self.home, all, ids, &|| Ok(())).unwrap()
    }
}

/// Every path under `root`, so a test can tell nothing was made.
fn tree(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut folders = vec![root.to_owned()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).unwrap().flatten() {
            if entry.file_type().unwrap().is_dir() {
                folders.push(entry.path());
            }
            paths.push(entry.path());
        }
    }
    paths.sort();
    paths
}

/// The labels of a command with no task, from no records.
fn unlabelled() -> Labels<'static> {
    Labels {
        work: &[],
        task: None,
    }
}

#[test]
fn the_transaction_list_carries_no_digest_source_or_contents() {
    let sandbox = Sandbox::new();
    let payload = sandbox.payload(".config/probe/settings.toml", b"SECRET-VALUE");
    let source = payload.source.to_string_lossy().into_owned();
    let plan = sandbox.plan(vec![payload]);
    assert_eq!(sandbox.apply(&plan)["ok"], true);

    let listed = sandbox.transactions();
    let shown = listing(&[], &listed, &unlabelled(), false);

    assert_eq!(listed["transactions"][0]["planId"], plan.id);
    assert_eq!(listed["transactions"][0]["phase"], "applied");
    assert_eq!(
        shown["plans"][0]["files"],
        json!([{"target": ".config/probe/settings.toml", "operation": "create"}])
    );
    for text in [listed.to_string(), shown.to_string()] {
        assert!(!text.contains("SECRET-VALUE"), "{text}");
        assert!(!text.contains(&digest(b"SECRET-VALUE")), "{text}");
        assert!(
            !text.contains("payload-") && !text.contains(&source),
            "{text}"
        );
        assert!(
            !text.contains("before") && !text.contains("sha256"),
            "{text}"
        );
    }
}

#[test]
fn the_list_opens_no_state_that_does_not_exist() {
    let sandbox = Sandbox::new();
    let before = tree(&sandbox.root);

    let listed = sandbox.transactions();

    assert_eq!(
        listed,
        json!({"ok": true, "transactions": [], "hidden": 0, "more": 0, "unreadable": 0})
    );
    assert_eq!(tree(&sandbox.root), before);
}

#[test]
fn a_stopped_apply_is_listed_as_one_to_finish() {
    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![sandbox.payload("file", b"copied")]);
    assert_eq!(sandbox.apply(&plan)["ok"], true);
    let store = state::Store::open(&sandbox.home, true).unwrap();
    let mut transaction = store.transaction(&plan.id).unwrap().unwrap();
    transaction.phase = Phase::Applying;
    store.save(&transaction).unwrap();
    store.activate(&plan.id).unwrap();
    drop(store);

    let listed = sandbox.transactions();
    let shown = listing(&[], &listed, &unlabelled(), false);

    assert_eq!(listed["transactions"][0]["active"], true);
    assert_eq!(shown["plans"][0]["state"], "applying");
}

#[test]
fn an_invalid_record_is_counted_not_listed() {
    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![sandbox.payload("file", b"copied")]);
    assert_eq!(sandbox.apply(&plan)["ok"], true);
    let store = state::Store::open(&sandbox.home, true).unwrap();
    let broken = crate::files::token();
    fs::create_dir(store.folder(&broken)).unwrap();
    atomic_write(
        &store.folder(&broken).join("transaction.json"),
        b"{\"version\": 1}",
        0o600,
    )
    .unwrap();
    let unstarted = crate::files::token();
    fs::create_dir(store.folder(&unstarted)).unwrap();
    drop(store);

    let listed = sandbox.transactions();

    assert_eq!(listed["unreadable"], 1);
    let listed = listed["transactions"].as_array().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["planId"], plan.id);
}

#[test]
fn a_frozen_plan_the_helper_never_started_is_listed_as_previewed() {
    let sandbox = Sandbox::new();
    let uid = nix::unistd::getuid().as_raw();
    let previewed = crate::files::token();
    write_plan(&stage_in(&sandbox.root, &previewed), &previewed, uid);
    let unknown = crate::files::token();
    fs::create_dir(stage_in(&sandbox.root, &unknown).join("request-ab12")).unwrap();
    let running = crate::files::token();
    let locked = stage_in(&sandbox.root, &running);
    write_plan(&locked, &running, uid);
    let _held = lock_stage(&locked).unwrap();
    let elsewhere = crate::files::token();
    write_plan(&stage_in(&sandbox.root, &elsewhere), &elsewhere, uid + 1);
    stage_in(&sandbox.root, &crate::files::token());

    let here = frozen(&sandbox.root, uid);
    let nothing = json!({"ok": true, "transactions": [], "more": 0, "unreadable": 0});
    let shown = listing(&here, &nothing, &unlabelled(), false);

    let mut states: Vec<(String, String)> = shown["plans"]
        .as_array()
        .unwrap()
        .iter()
        .map(|plan| {
            (
                plan["planId"].as_str().unwrap().to_owned(),
                plan["state"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    states.sort();
    let mut expected = vec![
        (previewed, "previewed".to_owned()),
        (unknown, "unknown".to_owned()),
        (running, "running".to_owned()),
    ];
    expected.sort();
    assert_eq!(states, expected);
    assert_eq!(shown["plans"][0]["session"], Value::Null);
}

#[test]
fn rolled_back_plans_are_listed_only_when_asked() {
    let sandbox = Sandbox::new();
    let kept = sandbox.plan(vec![sandbox.payload("kept", b"one")]);
    assert_eq!(sandbox.apply(&kept)["ok"], true);
    let undone = sandbox.plan(vec![sandbox.payload("undone", b"two")]);
    assert_eq!(sandbox.apply(&undone)["ok"], true);
    assert_eq!(sandbox.rollback(&undone, true)["rolledBack"], true);

    let shown = |all: bool| listing(&[], &sandbox.transactions_of(all, &[]), &unlabelled(), all);
    let ids = |all: bool| -> Vec<Value> {
        shown(all)["plans"]
            .as_array()
            .unwrap()
            .iter()
            .map(|plan| json!([plan["planId"], plan["state"]]))
            .collect()
    };

    assert_eq!(ids(false), [json!([kept.id, "applied"])]);
    assert_eq!(shown(false)["hidden"], 1);
    let all = ids(true);
    assert_eq!(all.len(), 2);
    assert!(all.contains(&json!([undone.id, "rolledBack"])));
    assert_eq!(shown(true)["hidden"], 0);
}

#[test]
fn rolled_back_plans_never_push_an_older_applied_one_out_of_the_list() {
    let sandbox = Sandbox::new();
    let applied = sandbox.plan(vec![sandbox.payload("file", b"copied")]);
    assert_eq!(sandbox.apply(&applied)["ok"], true);
    let store = state::Store::open(&sandbox.home, true).unwrap();
    let older = std::time::SystemTime::now() - Duration::from_secs(10_000);
    fs::File::open(store.folder(&applied.id).join("transaction.json"))
        .unwrap()
        .set_modified(older)
        .unwrap();
    let mut undone = store.transaction(&applied.id).unwrap().unwrap();
    undone.phase = Phase::RolledBack;
    for _ in 0..210 {
        undone.plan.id = crate::files::token();
        fs::create_dir(store.folder(&undone.plan.id)).unwrap();
        store.save(&undone).unwrap();
    }
    drop(store);

    let listed = sandbox.transactions();
    let shown = listing(&[], &listed, &unlabelled(), false);

    assert_eq!(listed["hidden"], 210);
    assert_eq!(listed["more"], 0);
    assert_eq!(shown["plans"].as_array().unwrap().len(), 1);
    assert_eq!(shown["plans"][0]["planId"], applied.id);
    assert_eq!(shown["plans"][0]["state"], "applied");
}

#[test]
fn a_plan_the_asker_holds_is_listed_whatever_its_phase_or_age() {
    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![sandbox.payload("file", b"copied")]);
    assert_eq!(sandbox.apply(&plan)["ok"], true);
    assert_eq!(sandbox.rollback(&plan, true)["rolledBack"], true);
    let uid = nix::unistd::getuid().as_raw();
    write_plan(&stage_in(&sandbox.root, &plan.id), &plan.id, uid);

    let here = frozen(&sandbox.root, uid);
    let listed = sandbox.transactions_of(false, std::slice::from_ref(&plan.id));
    let shown = listing(&here, &listed, &unlabelled(), false);

    assert_eq!(listed["transactions"][0]["phase"], "rolledBack");
    assert!(
        shown["plans"].as_array().unwrap().is_empty(),
        "never taken for a plan the helper did not start: {shown}"
    );
    assert_eq!(shown["hidden"], 1);
}

#[test]
fn a_plans_lock_looked_at_for_an_instant_is_waited_out() {
    let sandbox = Sandbox::new();
    let stage = stage_in(&sandbox.root, &crate::files::token());
    atomic_write(&stage.join("controller.lock"), b"", 0o600).unwrap();
    let looking = fs::File::open(stage.join("controller.lock")).unwrap();
    FileExt::try_lock_shared(&looking).unwrap();

    let locked = std::thread::scope(|scope| {
        let waiting = scope.spawn(|| lock_stage(&stage).map(|_| ()));
        std::thread::sleep(Duration::from_millis(50));
        drop(looking);
        waiting.join().unwrap()
    });
    assert!(locked.is_ok(), "{locked:?}");

    let held = lock_stage(&stage).unwrap();
    let error = lock_stage(&stage).unwrap_err();
    assert!(error.error.contains("already running"), "{error}");
    drop(held);
}

#[test]
fn the_list_is_newest_first_and_bounded() {
    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![sandbox.payload("file", b"copied")]);
    assert_eq!(sandbox.apply(&plan)["ok"], true);
    let store = state::Store::open(&sandbox.home, true).unwrap();
    let mut transaction = store.transaction(&plan.id).unwrap().unwrap();
    let then = std::time::SystemTime::now() - Duration::from_secs(1000);
    for age in 0..229_u64 {
        transaction.plan.id = crate::files::token();
        fs::create_dir(store.folder(&transaction.plan.id)).unwrap();
        store.save(&transaction).unwrap();
        let record = store.folder(&transaction.plan.id).join("transaction.json");
        fs::File::open(record)
            .unwrap()
            .set_modified(then - Duration::from_secs(age))
            .unwrap();
    }
    drop(store);

    let listed = sandbox.transactions();
    let shown = listing(&[], &listed, &unlabelled(), false);

    assert_eq!(listed["transactions"].as_array().unwrap().len(), 200);
    assert_eq!(listed["more"], 30);
    assert_eq!(listed["transactions"][0]["planId"], plan.id, "the newest");
    let times: Vec<u64> = listed["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|transaction| transaction["updatedAt"].as_u64().unwrap())
        .collect();
    assert!(times.windows(2).all(|pair| pair[0] >= pair[1]));
    assert_eq!(shown["plans"].as_array().unwrap().len(), SHOWN);
    assert_eq!(shown["more"], 30 + 150);
}

#[tokio::test]
async fn a_listed_plan_names_the_session_and_task_that_made_it() {
    use crate::{
        host_session::{Host, HostSession},
        task::{Claims, Owned, State, Task, store::Store as Tasks},
    };

    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![sandbox.payload("file", b"copied")]);
    assert_eq!(sandbox.apply(&plan)["ok"], true);
    let other = sandbox.plan(vec![sandbox.payload("other", b"copied")]);
    assert_eq!(sandbox.apply(&other)["ok"], true);
    let (mut client, helper) = Helper::client(&sandbox.root, UID);
    let session = HostSession::new(Host::Codex, "listing").unwrap();
    let task = Task::of(session.key(), Some("probe")).unwrap();
    client.task = Some(task.clone());
    Claims::in_store(Tasks::at(client.output_dir().unwrap()), task)
        .claim(
            &Owned::Plan {
                id: crate::files::Token::parse(&plan.id).unwrap(),
            },
            State::Made,
        )
        .unwrap();
    let before = client.controller.pending().unwrap();

    let listed = helper
        .serve(
            list_as(&client, &account(), false),
            previewing(&sandbox.home),
        )
        .await
        .unwrap();

    let plans = listed["plans"].as_array().unwrap();
    let mine = plans
        .iter()
        .find(|entry| entry["planId"] == plan.id)
        .unwrap();
    assert_eq!(mine["session"], session.key().as_str());
    assert_eq!(mine["thisTask"], true);
    let theirs = plans
        .iter()
        .find(|entry| entry["planId"] == other.id)
        .unwrap();
    assert_eq!(
        (&theirs["session"], &theirs["thisTask"]),
        (&Value::Null, &json!(false))
    );
    assert_eq!(
        client.controller.pending().unwrap(),
        before,
        "nothing pending"
    );
    assert!(
        leftovers(&sandbox.root).is_empty(),
        "the request folder goes"
    );
}

#[tokio::test]
async fn a_plan_pruned_because_the_helper_never_started_it_is_settled_gone() {
    use crate::task::{Claims, Owned, State, Task, store::Store as Tasks};

    let sandbox = Sandbox::new();
    let (mut client, helper) = Helper::client(&sandbox.root, UID);
    let id = crate::files::token();
    let stage = stage_in(&sandbox.root, &id);
    write_plan(&stage, &id, UID);
    atomic_write(&stage.join("controller.lock"), b"", 0o600).unwrap();
    let then = std::time::SystemTime::now() - UNUSED - Duration::from_secs(60);
    fs::File::open(&stage).unwrap().set_modified(then).unwrap();
    let session = crate::host_session::Key::of(crate::host_session::Host::Codex, "pruning");
    let task = Task::of(&session, None).unwrap();
    client.task = Some(task.clone());
    let plan = Owned::Plan {
        id: crate::files::Token::parse(&id).unwrap(),
    };
    let output = client.output_dir().unwrap().to_owned();
    Claims::in_store(Tasks::at(&output), task.clone())
        .claim(&plan, State::Made)
        .unwrap();

    helper
        .serve(prune(&client, &account()), |_| {
            Some(worker_reply(
                1,
                &json!({"ok": false, "error": worker::MISSING}),
            ))
        })
        .await;

    assert!(!stage.exists(), "the plan the helper never started went");
    let claims: Vec<_> = Tasks::at(&output)
        .load(task.key())
        .unwrap()
        .claims()
        .collect();
    assert_eq!(claims, [(plan, State::Gone)]);
}

/// A client of the stand-in helper whose command belongs to a named task
/// of a fresh session, and that task.
fn tasked(root: &Path) -> (Client, Helper, crate::task::Task) {
    let (mut client, helper) = Helper::client(root, UID);
    let session = crate::host_session::Key::of(crate::host_session::Host::Codex, "planning");
    let task = crate::task::Task::of(&session, Some("probe")).unwrap();
    client.task = Some(task.clone());
    (client, helper, task)
}

/// What `client`'s task record claims, with their states.
fn task_claims(
    client: &Client,
    task: &crate::task::Task,
) -> Vec<(crate::task::Owned, crate::task::State)> {
    crate::task::store::Store::at(client.output_dir().unwrap())
        .load(task.key())
        .map(|record| record.claims().collect())
        .unwrap_or_default()
}

#[tokio::test]
async fn a_preview_claims_its_plan_for_the_task_once_the_plan_is_written() {
    use crate::task::{Owned, State};

    let sandbox = Sandbox::new();
    let (client, helper, task) = tasked(&sandbox.root);
    let source = sandbox.root.join("settings.toml");
    fs::write(&source, b"copied").unwrap();
    let selections = vec![Selection {
        source,
        target: ".config/probe/settings.toml".into(),
        executable: false,
    }];
    let home = sandbox.home.clone();

    let previewed = helper
        .serve(
            freeze_as(
                &client,
                &account(),
                &sandbox.root.join("manifest.toml"),
                selections,
                false,
            ),
            move |request| {
                let packet: Packet =
                    decode(&fs::read(request["argv"][2].as_str().unwrap()).unwrap()).unwrap();
                let Request::Preview { id, files } = packet.request else {
                    panic!("only a preview is asked for");
                };
                let value = worker::preview(&home, UID, &id, files, &|| Ok(())).unwrap();
                Some(worker_reply(0, &value))
            },
        )
        .await
        .unwrap();

    let id = crate::files::Token::parse(previewed["planId"].as_str().unwrap()).unwrap();
    assert_eq!(
        task_claims(&client, &task),
        [(Owned::Plan { id }, State::Made)]
    );
}

#[tokio::test]
async fn an_apply_claims_its_plan_for_the_task_before_the_helper_is_asked() {
    use crate::task::{Owned, State};

    let sandbox = Sandbox::new();
    let (client, helper, task) = tasked(&sandbox.root);
    let id = crate::files::token();
    write_plan(&stage_in(&sandbox.root, &id), &id, UID);
    let plan = Owned::Plan {
        id: crate::files::Token::parse(&id).unwrap(),
    };

    let mut asked = Vec::new();
    let applied = helper
        .serve(apply_frozen(&client, &account(), &id), |_| {
            asked.push(task_claims(&client, &task));
            Some(worker_reply(
                0,
                &json!({"ok": true, "planId": id, "phase": "applied", "resumeRequired": false}),
            ))
        })
        .await
        .unwrap();

    assert_eq!(applied["phase"], "applied");
    assert_eq!(asked, [vec![(plan.clone(), State::Made)]]);
    assert_eq!(task_claims(&client, &task), [(plan, State::Made)]);
}
