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

/// A command's folder, kept when the command was stopped or its reply was
/// lost, goes once it is older than a week.
#[test]
fn old_command_folders_are_pruned() {
    let sandbox = Sandbox::new();
    let id = crate::control::token();
    let run = sandbox.root.join(format!("environment-run-{id}"));
    fs::create_dir(&run).unwrap();
    stage_in(&sandbox.root, &crate::control::token());
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
        &crate::control::token(),
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
        .join(format!("transaction-{}", crate::control::token()));
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
        &crate::control::token(),
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
