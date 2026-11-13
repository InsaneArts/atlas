use super::{
    list::{Labels, SHOWN, listing},
    stage::{decode_reply, refused},
    *,
};
use crate::test_support::Helper;
use atlas_core::{
    files::{Lock, Token},
    test_support::UID,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{
    cell::Cell,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
};

pub(crate) struct Sandbox {
    _root: tempfile::TempDir,
    pub(crate) root: PathBuf,
    pub(crate) home: PathBuf,
}

impl Sandbox {
    pub(crate) fn new() -> Self {
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

    pub(crate) fn payload(&self, target: &str, bytes: &[u8]) -> Payload {
        let source = self.root.join(atlas_core::files::token());
        fs::write(&source, bytes).unwrap();
        Payload {
            source,
            target: target.into(),
            sha256: digest(bytes),
            executable: false,
        }
    }

    pub(super) fn preview(&self, files: Vec<Payload>) -> Value {
        worker::preview(
            &self.home,
            nix::unistd::getuid().as_raw(),
            &atlas_core::files::token(),
            files,
            &|| Ok(()),
        )
        .unwrap()
    }

    pub(crate) fn plan(&self, files: Vec<Payload>) -> Plan {
        let value = self.preview(files);
        assert_eq!(value["ok"], true, "{value}");
        serde_json::from_value(value["plan"].clone()).unwrap()
    }

    pub(crate) fn apply(&self, plan: &Plan) -> Value {
        worker::apply_plan(&self.home, plan, &|| Ok(())).unwrap()
    }

    pub(crate) fn rollback(&self, plan: &Plan, apply: bool) -> Value {
        worker::rollback(&self.home, &plan.id, apply, &|| Ok(())).unwrap()
    }
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
fn bounded_extended_attributes_are_preserved_and_compared_exactly() {
    let sandbox = Sandbox::new();
    let path = sandbox.home.join("config");
    atomic_write(&path, b"old", 0o640).unwrap();
    let mut attributes = fingerprint(&path).unwrap().unwrap().xattrs;
    attributes.insert("user.atlas-test".into(), STANDARD.encode([0_u8, 1, 255, 3]));
    restore_attributes(&path, &attributes).unwrap();

    let state = state::Store::open(&sandbox.home, Lock::Exclusive).unwrap();
    state
        .save_ledger(&Ledger {
            version: 1,
            files: BTreeMap::from([(
                PathBuf::from("config"),
                Entry {
                    generation: "earlier".into(),
                    fingerprint: fingerprint(&path).unwrap().unwrap(),
                },
            )]),
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
fn lost_install_acknowledgement_uses_the_prepared_inode_not_just_its_hash() {
    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![sandbox.payload("file", b"one")]);
    assert_eq!(sandbox.apply(&plan)["ok"], true);
    let path = sandbox.home.join("file");
    let installed_inode = fs::metadata(&path).unwrap().ino();

    let store = state::Store::open(&sandbox.home, Lock::Exclusive).unwrap();
    let mut transaction = store.transaction(&plan.id).unwrap().unwrap();
    // This is the on-disk boundary after rename and before ledger acknowledgement.
    transaction.phase = Phase::Applying;
    let prepared = transaction.progress[0].prepared.clone().unwrap();
    transaction.progress[0] = Progress::default();
    transaction.progress[0].prepared = Some(prepared);
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
    let store = state::Store::open(&sandbox.home, Lock::Exclusive).unwrap();
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
fn kept_progress_without_restoration_is_refused_without_rewriting_it() {
    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![sandbox.payload("file", b"one")]);
    assert_eq!(sandbox.apply(&plan)["ok"], true);
    let store = state::Store::open(&sandbox.home, Lock::Exclusive).unwrap();
    let record = store.folder(&plan.id).join("transaction.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&record).unwrap()).unwrap();
    value["phase"] = json!("rollingBack");
    value["progress"][0]["kept"] = json!(true);
    let bytes = serde_json::to_vec_pretty(&value).unwrap();
    atomic_write(&record, &bytes, 0o600).unwrap();
    let ledger = fs::read(store.path.join("files.json")).unwrap();
    let backup = fs::read(store.blob(&plan.id, 0, false)).unwrap();
    drop(store);

    let error = worker::rollback(&sandbox.home, &plan.id, true, &|| Ok(())).unwrap_err();
    assert_eq!(error.error, "Invalid transaction record");
    assert_eq!(fs::read(&record).unwrap(), bytes);
    assert_eq!(fs::read(sandbox.home.join("file")).unwrap(), b"one");
    let store = state::Store::open(&sandbox.home, Lock::Shared).unwrap();
    assert_eq!(fs::read(store.path.join("files.json")).unwrap(), ledger);
    assert_eq!(fs::read(store.blob(&plan.id, 0, false)).unwrap(), backup);
}

#[test]
fn an_untouched_rollback_needs_no_applied_or_reverting_flag() {
    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![sandbox.payload("file", b"one")]);
    let record = sandbox
        .home
        .join(STATE)
        .join(format!("transaction-{}/transaction.json", plan.id));
    assert_eq!(stop_apply(&sandbox, &plan, || record.exists())["ok"], false);
    assert_eq!(sandbox.rollback(&plan, true)["rolledBack"], true);

    let store = state::Store::open(&sandbox.home, Lock::Shared).unwrap();
    let transaction = store.transaction(&plan.id).unwrap().unwrap();
    let progress = &transaction.progress[0];
    assert!(progress.restored);
    assert!(!progress.applied);
    assert!(!progress.reverting);
    assert!(!progress.kept);
    assert!(progress.prepared.is_none());
    assert!(!sandbox.home.join("file").exists());
}

/// Reproduce the durable and filesystem sides of each write-ahead boundary.
/// Returning drops the store lock; recovery must open and validate it anew.
pub(super) fn file_progress_at(sandbox: &Sandbox, operation: Operation, boundary: &str) -> Plan {
    let path = sandbox.home.join("file");
    match operation {
        Operation::Update => {
            let previous = sandbox.plan(vec![sandbox.payload("file", b"before")]);
            assert_eq!(sandbox.apply(&previous)["ok"], true);
        }
        Operation::Permissions => atomic_write(&path, b"after", 0o640).unwrap(),
        Operation::Unchanged => atomic_write(&path, b"after", 0o600).unwrap(),
        Operation::Create => {}
        Operation::Conflict => unreachable!(),
    }
    let plan = sandbox.plan(vec![sandbox.payload("file", b"after")]);
    assert_eq!(plan.files[0].operation, operation);
    let before = snapshot(&path).unwrap();
    let store = state::Store::open(&sandbox.home, Lock::Exclusive).unwrap();
    private_directory(&store.folder(&plan.id)).unwrap();
    atomic_write(&store.blob(&plan.id, 0, false), b"after", 0o600).unwrap();
    if let Some((_, bytes)) = &before {
        atomic_write(&store.blob(&plan.id, 0, true), bytes, 0o600).unwrap();
    }
    let mut transaction = Transaction {
        version: 1,
        plan: plan.clone(),
        phase: Phase::Applying,
        progress: vec![Progress::default()],
        directories: Vec::new(),
        retained_directories: Vec::new(),
    };
    let mut pending_ledger = None;
    let mut active: Option<&str> = None;
    assert!(store.active().unwrap().is_none());
    'cut: {
        assert!(transaction.valid_progress());
        if boundary == "initial" {
            break 'cut;
        }
        assert!(active.is_none_or(|id| id == plan.id.as_str()));
        active = Some(plan.id.as_str());
        let install = sandbox.home.join(format!(".atlas-{}-0-install", plan.id));
        if operation != Operation::Unchanged {
            atomic_install(&install, b"after", 0o600, true).unwrap();
            if boundary == "unrecorded prepared inode" {
                break 'cut;
            }
            transaction.progress[0].prepared = Some(fingerprint(&install).unwrap().unwrap());
            assert!(transaction.valid_progress());
            if boundary == "prepared evidence" {
                break 'cut;
            }
            install_prepared(&install, &path, before.is_none()).unwrap();
        }
        if boundary == "installation" {
            break 'cut;
        }
        let ledger = pending_ledger.insert(store.ledger().unwrap());
        ledger.version = 1;
        ledger.files.insert(
            "file".into(),
            Entry {
                generation: plan.id.clone(),
                fingerprint: fingerprint(&path).unwrap().unwrap(),
            },
        );
        if boundary == "apply ledger" {
            break 'cut;
        }
        transaction.progress[0].applied = true;
        assert!(transaction.valid_progress());
        if boundary == "applied acknowledgement" {
            break 'cut;
        }
        transaction.phase = Phase::Applied;
        assert!(transaction.valid_progress());
        assert_eq!(active, Some(plan.id.as_str()));
        active = None;
        if boundary == "applied phase" {
            break 'cut;
        }
        assert!(active.is_none_or(|id| id == plan.id.as_str()));
        active = Some(plan.id.as_str());
        transaction.phase = Phase::RollingBack;
        assert!(transaction.valid_progress());
        if boundary == "rollback intent" {
            break 'cut;
        }
        if boundary == "kept ledger" || boundary == "kept acknowledgement" {
            atomic_write(&path, b"helper edit", 0o600).unwrap();
        } else if let Some((fingerprint, bytes)) = &before {
            let restore = if operation == Operation::Unchanged {
                fingerprint.clone()
            } else {
                let restore = sandbox.home.join(format!(".atlas-{}-0-restore", plan.id));
                atomic_install(&restore, bytes, fingerprint.mode, true).unwrap();
                if boundary == "unrecorded restore inode" {
                    break 'cut;
                }
                super::fingerprint(&restore).unwrap().unwrap()
            };
            transaction.progress[0].restore = Some(restore);
            if operation != Operation::Unchanged {
                assert!(transaction.valid_progress());
                if boundary == "restore evidence" {
                    break 'cut;
                }
            }
        }
        if boundary != "kept ledger" && boundary != "kept acknowledgement" {
            transaction.progress[0].reverting = true;
            assert!(transaction.valid_progress());
            if boundary == "reverting intent" {
                break 'cut;
            }
            if operation != Operation::Unchanged {
                if before.is_some() {
                    let restore = sandbox.home.join(format!(".atlas-{}-0-restore", plan.id));
                    install_prepared(&restore, &path, false).unwrap();
                } else {
                    unlink(&path, false).unwrap();
                }
            }
            if boundary == "file restoration" {
                break 'cut;
            }
        }
        let previous = plan.files[0].previous.as_ref().map(|entry| Entry {
            generation: entry.generation.clone(),
            fingerprint: transaction.progress[0]
                .restore
                .clone()
                .unwrap_or_else(|| plan.files[0].before.clone().unwrap()),
        });
        if let Some(previous) = previous {
            ledger.files.insert("file".into(), previous);
        } else {
            ledger.files.remove(Path::new("file"));
        }
        if boundary == "restore ledger" || boundary == "kept ledger" {
            break 'cut;
        }
        if boundary == "kept acknowledgement" {
            transaction.progress[0].complete_kept();
        } else {
            transaction.progress[0].restored = true;
        }
        assert!(transaction.valid_progress());
    }
    store.save(&transaction).unwrap();
    if let Some(ledger) = pending_ledger {
        store.save_ledger(&ledger).unwrap();
    }
    if let Some(id) = active {
        store.activate(id).unwrap();
    }
    plan
}

#[test]
fn invalid_and_future_progress_records_remain_inert() {
    let sandbox = Sandbox::new();
    let plan = file_progress_at(&sandbox, Operation::Permissions, "applied phase");
    let store = state::Store::open(&sandbox.home, Lock::Exclusive).unwrap();
    let path = store.folder(&plan.id).join("transaction.json");
    let original: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let ledger = fs::read(store.path.join("files.json")).unwrap();
    let before = fs::read(store.blob(&plan.id, 0, true)).unwrap();
    let after = fs::read(store.blob(&plan.id, 0, false)).unwrap();
    drop(store);

    for (pointer, value) in [
        ("/version", json!(2)),
        ("/phase", json!("future")),
        ("/phase", json!("rolledBack")),
        ("/progress/0/prepared", Value::Null),
        ("/progress/0/applied", json!(false)),
        ("/progress/0/restored", json!(true)),
        ("/progress/0/reverting", json!(true)),
        ("/progress/0/kept", json!(true)),
    ] {
        let mut record = original.clone();
        // The optional kept key is absent in a legacy/default record.
        if pointer == "/progress/0/kept" {
            record["progress"][0]["kept"] = value;
        } else {
            *record.pointer_mut(pointer).unwrap() = value;
        }
        let bytes = serde_json::to_vec_pretty(&record).unwrap();
        atomic_write(&path, &bytes, 0o600).unwrap();
        let store = state::Store::open(&sandbox.home, Lock::Exclusive).unwrap();
        assert!(store.transaction(&plan.id).is_err(), "{pointer}");
        if let Ok(transaction) = decode::<Transaction>(&bytes)
            && !transaction.valid_progress()
        {
            assert_eq!(
                store.save(&transaction).unwrap_err().error,
                "Invalid transaction record"
            );
        }
        drop(store);
        assert!(worker::apply_plan(&sandbox.home, &plan, &|| Ok(())).is_err());
        assert!(worker::rollback(&sandbox.home, &plan.id, true, &|| Ok(())).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::read(sandbox.home.join("file")).unwrap(), b"after");
        let store = state::Store::open(&sandbox.home, Lock::Shared).unwrap();
        assert_eq!(fs::read(store.path.join("files.json")).unwrap(), ledger);
        assert_eq!(fs::read(store.blob(&plan.id, 0, true)).unwrap(), before);
        assert_eq!(fs::read(store.blob(&plan.id, 0, false)).unwrap(), after);
    }

    for extra in ["future", "progress"] {
        let mut record = original.clone();
        if extra == "progress" {
            record["progress"][0]["future"] = json!(false);
        } else {
            record[extra] = json!(false);
        }
        let bytes = serde_json::to_vec_pretty(&record).unwrap();
        atomic_write(&path, &bytes, 0o600).unwrap();
        assert!(worker::rollback(&sandbox.home, &plan.id, true, &|| Ok(())).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn rollback_progress_rejects_only_proven_impossible_evidence_pairs() {
    for operation in [Operation::Create, Operation::Update] {
        let sandbox = Sandbox::new();
        let plan = file_progress_at(&sandbox, operation, "reverting intent");
        let store = state::Store::open(&sandbox.home, Lock::Exclusive).unwrap();
        let mut transaction = store.transaction(&plan.id).unwrap().unwrap();
        if operation == Operation::Create {
            let prepared = transaction.progress[0].prepared.clone();
            transaction.progress[0].restore = prepared;
        } else {
            transaction.progress[0].restore = None;
        }
        let record = store.folder(&plan.id).join("transaction.json");
        let original = fs::read(&record).unwrap();
        assert_eq!(
            store.save(&transaction).unwrap_err().error,
            "Invalid transaction record"
        );
        assert_eq!(fs::read(&record).unwrap(), original);
        let malformed = serde_json::to_vec_pretty(&transaction).unwrap();
        atomic_write(&record, &malformed, 0o600).unwrap();
        assert_eq!(
            store.transaction(&plan.id).unwrap_err().error,
            "Invalid transaction record"
        );
        assert_eq!(fs::read(record).unwrap(), malformed);
    }
}

#[test]
fn reverting_files_can_be_kept_after_drift() {
    for operation in [Operation::Create, Operation::Update, Operation::Unchanged] {
        let sandbox = Sandbox::new();
        let plan = file_progress_at(&sandbox, operation, "reverting intent");
        atomic_write(&sandbox.home.join("file"), b"helper edit", 0o600).unwrap();
        let result = sandbox.rollback(&plan, true);
        assert_eq!(result["rolledBack"], true, "{result}");
        assert_eq!(result["keptTargets"], json!(["file"]));
        let store = state::Store::open(&sandbox.home, Lock::Shared).unwrap();
        let transaction = store.transaction(&plan.id).unwrap().unwrap();
        assert!(transaction.progress[0].reverting);
        assert!(transaction.progress[0].kept);
        assert_eq!(fs::read(sandbox.home.join("file")).unwrap(), b"helper edit");
    }
}

#[test]
fn legacy_progress_keeps_optional_fingerprints_and_kept_defaults() {
    for omitted in [false, true] {
        let sandbox = Sandbox::new();
        let plan = file_progress_at(&sandbox, Operation::Unchanged, "applied phase");
        let store = state::Store::open(&sandbox.home, Lock::Exclusive).unwrap();
        let path = store.folder(&plan.id).join("transaction.json");
        let mut record: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let progress = record["progress"][0].as_object_mut().unwrap();
        assert!(!progress.contains_key("kept"));
        for field in ["prepared", "restore"] {
            assert_eq!(progress[field], Value::Null);
            if omitted {
                progress.remove(field);
            }
        }
        let bytes = serde_json::to_vec_pretty(&record).unwrap();
        atomic_write(&path, &bytes, 0o600).unwrap();
        let transaction = store.transaction(&plan.id).unwrap().unwrap();
        assert!(!transaction.progress[0].kept);
        assert!(transaction.progress[0].prepared.is_none());
        assert!(transaction.progress[0].restore.is_none());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        drop(store);
        assert_eq!(sandbox.rollback(&plan, true)["rolledBack"], true);
        assert_eq!(fs::read(sandbox.home.join("file")).unwrap(), b"after");
    }
}

#[test]
fn recorded_candidate_drift_is_refused_without_losing_recovery_evidence() {
    for (boundary, suffix) in [
        ("prepared evidence", "install"),
        ("restore evidence", "restore"),
    ] {
        for metadata in [false, true] {
            let sandbox = Sandbox::new();
            let plan = file_progress_at(&sandbox, Operation::Update, boundary);
            let candidate = sandbox.home.join(format!(".atlas-{}-0-{suffix}", plan.id));
            if metadata {
                fs::set_permissions(&candidate, fs::Permissions::from_mode(0o640)).unwrap();
            } else {
                let bytes = fs::read(&candidate).unwrap();
                atomic_write(&candidate, &bytes, 0o600).unwrap();
            }
            let record = sandbox
                .home
                .join(STATE)
                .join(format!("transaction-{}/transaction.json", plan.id));
            let bytes = fs::read(&record).unwrap();
            let result = if suffix == "install" {
                sandbox.apply(&plan)
            } else {
                sandbox.rollback(&plan, true)
            };
            assert_eq!(result["ok"], false, "{result}");
            assert_eq!(fs::read(&record).unwrap(), bytes);
            assert!(candidate.exists());
        }
    }
}

#[test]
fn a_ready_worker_can_be_killed_at_a_durable_progress_boundary() {
    use crate::test_support::{Fixture, executor_ready};
    use atlas_core::test_support::rerun;

    const TEST: &str =
        "environment::tests::a_ready_worker_can_be_killed_at_a_durable_progress_boundary";
    if let Some(directory) = atlas_core::test_support::directory() {
        let plan: Plan = decode(&fs::read(directory.join("plan.json")).unwrap()).unwrap();
        let store = state::Store::open(&plan.home, Lock::Exclusive).unwrap();
        let transaction = store.transaction(&plan.id).unwrap().unwrap();
        executor_ready(&directory, || store.save(&transaction).unwrap()).unwrap();
        return;
    }
    for boundary in [
        "prepared evidence",
        "apply ledger",
        "reverting intent",
        "restore ledger",
        "kept acknowledgement",
    ] {
        let sandbox = Sandbox::new();
        let plan = file_progress_at(&sandbox, Operation::Update, boundary);
        fs::write(
            sandbox.root.join("plan.json"),
            serde_json::to_vec(&plan).unwrap(),
        )
        .unwrap();
        let mut fixture = Fixture::spawn(
            &sandbox.root,
            rerun(TEST).env("ATLAS_MUTATION_TEST_DIRECTORY", &sandbox.root),
        )
        .unwrap();
        fixture.ready().unwrap();
        assert!(state::Store::open_with_retry(&sandbox.home, Lock::Exclusive, |_| false).is_err());
        fixture.interrupt().unwrap();
        drop(fixture);
        let result = sandbox.rollback(&plan, true);
        assert_eq!(result["rolledBack"], true, "{boundary}: {result}");
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

    let store = state::Store::open(&sandbox.home, Lock::Exclusive).unwrap();
    fs::write(store.blob(&second.id, 0, true), b"corrupt").unwrap();
    drop(store);

    assert_eq!(sandbox.rollback(&second, false)["ok"], false);
    assert_eq!(sandbox.rollback(&second, true)["ok"], false);
    assert_eq!(fs::read(sandbox.home.join("first")).unwrap(), b"new one");
    assert_eq!(fs::read(sandbox.home.join("second")).unwrap(), b"new two");
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
            &atlas_core::files::token(),
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
fn shipped_manifest_and_profile_examples_parse_as_toml() {
    let root = atlas_core::app_bundle::checkout();
    let manifest = root.join("examples/environment.toml");
    let parsed: Manifest =
        atlas_core::config::parse_toml(&fs::read(&manifest).unwrap(), &manifest).unwrap();
    assert_eq!(parsed.files.len(), 2);
    assert!(parsed.files[0].executable);

    let profile = root.join("examples/profile.toml");
    profile::load(&profile).unwrap();

    let json = br#"{"files": []}"#;
    let refused = atlas_core::config::parse_toml::<Manifest>(json, &manifest);
    assert!(refused.is_err_and(|error| error.code == Code::Config));
}

#[test]
fn a_briefly_held_store_lock_is_waited_for_but_a_held_one_is_refused() {
    let sandbox = Sandbox::new();
    let mut first = Some(state::Store::open(&sandbox.home, Lock::Exclusive).unwrap());
    state::Store::open_with_retry(&sandbox.home, Lock::Exclusive, |_| {
        drop(first.take().expect("the first retry releases the lock"));
        true
    })
    .expect("a lock released on retry is taken");
    assert!(first.is_none(), "the held lock required a retry");

    let held = state::Store::open(&sandbox.home, Lock::Exclusive).unwrap();
    let mut retries = 0;
    let refused = state::Store::open_with_retry(&sandbox.home, Lock::Exclusive, |_| {
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
pub(crate) fn stage_in(shared: &Path, id: &str) -> PathBuf {
    let stage = shared.join(format!("environment-plan-{id}"));
    fs::DirBuilder::new().mode(0o700).create(&stage).unwrap();
    stage
}

pub(crate) fn write_plan(stage: &Path, id: &str, uid: u32) {
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
pub(crate) fn worker_reply(exit: u8, output: &Value) -> Value {
    json!({
        "ok": true,
        "exitCode": exit,
        "timedOut": false,
        "signal": null,
        "events": [{"event": "output", "data": STANDARD.encode(output.to_string())}],
    })
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
            drop(state::Store::open(&sandbox.home, Lock::Exclusive).unwrap());
        }
        Ok(())
    };

    let refused = worker::preview(
        &sandbox.home,
        nix::unistd::getuid().as_raw(),
        &atlas_core::files::token(),
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

/// The second login `client` acts in.
pub(crate) fn user_of(client: &Client) -> atlas_core::user::User {
    match client.desktop().kind() {
        Kind::User(login) => *login,
    }
}

pub(crate) fn rollback_request(request: &Value, id: &Token) -> bool {
    let argv = request["argv"].as_array().unwrap();
    let packet: Packet = decode(&fs::read(argv[2].as_str().unwrap()).unwrap()).unwrap();
    let Request::Rollback { id: asked, apply } = packet.request else {
        panic!("only this plan's rollback is requested");
    };
    assert_eq!(asked, id.as_str());
    assert_eq!(argv.len(), if apply { 4 } else { 3 });
    apply
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

    let preview = state::Store::open(&sandbox.home, Lock::Shared).unwrap();
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

    let store = state::Store::open(&sandbox.home, Lock::Exclusive).unwrap();
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

/// Stop `plan`'s apply at the first guard where `stop` holds.
pub(crate) fn stop_apply(sandbox: &Sandbox, plan: &Plan, stop: impl Fn() -> bool) -> Value {
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
pub(super) fn foreign_provenance() -> BTreeMap<String, String> {
    BTreeMap::from([(
        "com.apple.provenance".to_owned(),
        STANDARD.encode([1_u8, 2, 0, 0x83, 0x32, 0x51, 0x45, 0x74, 0x26, 0x9a, 0xda]),
    )])
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

    let state = state::Store::open(&sandbox.home, Lock::Exclusive).unwrap();
    state
        .save_ledger(&Ledger {
            version: 1,
            files: BTreeMap::from([(
                PathBuf::from("tool"),
                Entry {
                    generation: "earlier".into(),
                    fingerprint: fingerprint(&path).unwrap().unwrap(),
                },
            )]),
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

    let store = state::Store::open(&sandbox.home, Lock::Shared).unwrap();
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
    let store = state::Store::open(&sandbox.home, Lock::Exclusive).unwrap();
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

    let store = state::Store::open(&sandbox.home, Lock::Exclusive).unwrap();
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
    pub(super) fn transactions(&self) -> Value {
        self.transactions_of(false, &[])
    }

    /// The helper's list, with those rolled back when `all`, and each of
    /// `ids` whatever its phase.
    pub(crate) fn transactions_of(&self, all: bool, ids: &[String]) -> Value {
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

/// The helper's list of its transactions `there`, as `list` reads it.
pub(crate) fn as_listed(there: &Value) -> reply::Transactions {
    serde_json::from_value(there.clone()).unwrap()
}

/// The labels of a command with no task, from no records.
pub(crate) fn unlabelled() -> Labels<'static> {
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
    let shown = listing(&[], &as_listed(&listed), &unlabelled(), false);

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
    let store = state::Store::open(&sandbox.home, Lock::Exclusive).unwrap();
    let mut transaction = store.transaction(&plan.id).unwrap().unwrap();
    transaction.phase = Phase::Applying;
    store.save(&transaction).unwrap();
    store.activate(&plan.id).unwrap();
    drop(store);

    let listed = sandbox.transactions();
    let shown = listing(&[], &as_listed(&listed), &unlabelled(), false);

    assert_eq!(listed["transactions"][0]["active"], true);
    assert_eq!(shown["plans"][0]["state"], "applying");
}

#[test]
fn an_invalid_record_is_counted_not_listed() {
    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![sandbox.payload("file", b"copied")]);
    assert_eq!(sandbox.apply(&plan)["ok"], true);
    let store = state::Store::open(&sandbox.home, Lock::Exclusive).unwrap();
    let broken = atlas_core::files::token();
    fs::create_dir(store.folder(&broken)).unwrap();
    atomic_write(
        &store.folder(&broken).join("transaction.json"),
        b"{\"version\": 1}",
        0o600,
    )
    .unwrap();
    let unstarted = atlas_core::files::token();
    fs::create_dir(store.folder(&unstarted)).unwrap();
    drop(store);

    let listed = sandbox.transactions();

    assert_eq!(listed["unreadable"], 1);
    let listed = listed["transactions"].as_array().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["planId"], plan.id);
}

#[test]
fn rolled_back_plans_are_listed_only_when_asked() {
    let sandbox = Sandbox::new();
    let kept = sandbox.plan(vec![sandbox.payload("kept", b"one")]);
    assert_eq!(sandbox.apply(&kept)["ok"], true);
    let undone = sandbox.plan(vec![sandbox.payload("undone", b"two")]);
    assert_eq!(sandbox.apply(&undone)["ok"], true);
    assert_eq!(sandbox.rollback(&undone, true)["rolledBack"], true);

    let shown = |all: bool| {
        listing(
            &[],
            &as_listed(&sandbox.transactions_of(all, &[])),
            &unlabelled(),
            all,
        )
    };
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
    let store = state::Store::open(&sandbox.home, Lock::Exclusive).unwrap();
    let older = std::time::SystemTime::now() - Duration::from_secs(10_000);
    fs::File::open(store.folder(&applied.id).join("transaction.json"))
        .unwrap()
        .set_modified(older)
        .unwrap();
    let mut undone = store.transaction(&applied.id).unwrap().unwrap();
    undone.phase = Phase::RolledBack;
    undone.progress[0].reverting = true;
    undone.progress[0].restored = true;
    for _ in 0..210 {
        undone.plan.id = atlas_core::files::token();
        fs::create_dir(store.folder(&undone.plan.id)).unwrap();
        store.save(&undone).unwrap();
    }
    drop(store);

    let listed = sandbox.transactions();
    let shown = listing(&[], &as_listed(&listed), &unlabelled(), false);

    assert_eq!(listed["hidden"], 210);
    assert_eq!(listed["more"], 0);
    assert_eq!(shown["plans"].as_array().unwrap().len(), 1);
    assert_eq!(shown["plans"][0]["planId"], applied.id);
    assert_eq!(shown["plans"][0]["state"], "applied");
}

#[test]
fn the_list_is_newest_first_and_bounded() {
    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![sandbox.payload("file", b"copied")]);
    assert_eq!(sandbox.apply(&plan)["ok"], true);
    let store = state::Store::open(&sandbox.home, Lock::Exclusive).unwrap();
    let mut transaction = store.transaction(&plan.id).unwrap().unwrap();
    let then = std::time::SystemTime::now() - Duration::from_secs(1000);
    for age in 0..229_u64 {
        transaction.plan.id = atlas_core::files::token();
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
    let shown = listing(&[], &as_listed(&listed), &unlabelled(), false);

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

/// A client of the stand-in helper whose command belongs to a named task
/// of a fresh session, and that task.
pub(crate) fn tasked(root: &Path) -> (Client, Helper, crate::task::Task) {
    let (mut client, helper) = Helper::client(root, UID);
    let session = crate::host_session::Key::of(crate::host_session::Host::Codex, "planning");
    let task = crate::task::Task::of(&session, Some("probe")).unwrap();
    client.task = Some(task.clone());
    (client, helper, task)
}

/// What `client`'s task record claims, with their states.
pub(crate) fn task_claims(
    client: &Client,
    task: &crate::task::Task,
) -> Vec<(crate::task::Owned, crate::task::State)> {
    crate::task::store::Store::at(client.output_dir().unwrap())
        .load(task.key())
        .map(|record| record.claims().collect())
        .unwrap_or_default()
}

/// `reply` as text, with what differs between runs named: each of `plans`
/// by its place, and every time.
fn shaped(reply: &Value, plans: &[&str]) -> String {
    let mut text = serde_json::to_string(reply).unwrap();
    for (place, plan) in plans.iter().enumerate() {
        text = text.replace(plan, &format!("PLAN{place}"));
    }
    let mut out = String::new();
    let mut rest = text.as_str();
    while let Some(at) = rest.find("\"updatedAt\":") {
        let (before, after) = rest.split_at(at + "\"updatedAt\":".len());
        out.push_str(before);
        out.push_str("TIME");
        rest = after.trim_start_matches(|c: char| c.is_ascii_digit());
    }
    out.push_str(rest);
    out
}

#[test]
fn the_transaction_replies_keep_their_wire_shape() {
    let sandbox = Sandbox::new();
    let kept = sandbox.plan(vec![sandbox.payload(".config/probe/a.toml", b"one")]);
    let undone = sandbox.plan(vec![
        sandbox.payload("undone", b"two"),
        sandbox.payload(".local/bin/tool", b"three"),
    ]);
    let plans = [kept.id.as_str(), undone.id.as_str()];
    let mut said = vec![
        shaped(&sandbox.apply(&kept), &plans),
        shaped(&sandbox.apply(&undone), &plans),
    ];
    fs::write(sandbox.home.join("undone"), b"changed in the helper").unwrap();
    said.push(shaped(&sandbox.rollback(&undone, false), &plans));
    said.push(shaped(&sandbox.rollback(&undone, true), &plans));
    said.push(shaped(&sandbox.rollback(&undone, true), &plans));
    let store = state::Store::open(&sandbox.home, Lock::Exclusive).unwrap();
    fs::File::open(store.folder(&kept.id).join("transaction.json"))
        .unwrap()
        .set_modified(std::time::SystemTime::now() - Duration::from_secs(10_000))
        .unwrap();
    drop(store);
    let listed = sandbox.transactions();
    said.push(shaped(&listed, &plans));
    said.push(shaped(
        &listing(&[], &as_listed(&listed), &unlabelled(), false),
        &plans,
    ));
    let every = sandbox.transactions_of(true, &[]);
    said.push(shaped(&every, &plans));
    said.push(shaped(
        &listing(&[], &as_listed(&every), &unlabelled(), true),
        &plans,
    ));
    let odd = json!({"ok": true, "more": 2, "hidden": 1, "unreadable": 1, "transactions": [
        {"planId": kept.id, "phase": "sideways", "updatedAt": 5, "files": []},
        {"planId": "not-a-plan", "phase": "applied"},
        {"planId": undone.id, "phase": "applied", "files": [{"target": "x", "operation": "create", "legacyPrevious": true}]},
    ]});
    said.push(shaped(
        &listing(&[], &as_listed(&odd), &unlabelled(), false),
        &plans,
    ));

    assert_eq!(
        said,
        [
            r#"{"applied":true,"completedTargets":[".config/probe/a.toml"],"files":[{"legacyPrevious":false,"operation":"create","target":".config/probe/a.toml"}],"ok":true,"pendingTargets":[],"phase":"applied","planId":"PLAN0","resumeRequired":false,"retainedDirectories":[],"rolledBack":false}"#,
            r#"{"applied":true,"completedTargets":["undone",".local/bin/tool"],"files":[{"legacyPrevious":false,"operation":"create","target":"undone"},{"legacyPrevious":false,"operation":"create","target":".local/bin/tool"}],"ok":true,"pendingTargets":[],"phase":"applied","planId":"PLAN1","resumeRequired":false,"retainedDirectories":[],"rolledBack":false}"#,
            r#"{"applied":true,"completedTargets":[],"files":[{"legacyPrevious":false,"operation":"create","target":"undone"},{"legacyPrevious":false,"operation":"create","target":".local/bin/tool"}],"keptTargets":["undone"],"ok":true,"pendingTargets":[".local/bin/tool"],"phase":"applied","planId":"PLAN1","preview":true,"resumeRequired":false,"retainedDirectories":[],"rolledBack":false}"#,
            r#"{"applied":false,"completedTargets":[".local/bin/tool"],"files":[{"legacyPrevious":false,"operation":"create","target":"undone"},{"legacyPrevious":false,"operation":"create","target":".local/bin/tool"}],"keptTargets":["undone"],"ok":true,"pendingTargets":[],"phase":"rolledBack","planId":"PLAN1","preview":false,"resumeRequired":false,"retainedDirectories":[],"rolledBack":true}"#,
            r#"{"applied":false,"completedTargets":[".local/bin/tool"],"files":[{"legacyPrevious":false,"operation":"create","target":"undone"},{"legacyPrevious":false,"operation":"create","target":".local/bin/tool"}],"keptTargets":["undone"],"ok":true,"pendingTargets":[],"phase":"rolledBack","planId":"PLAN1","resumeRequired":false,"retainedDirectories":[],"rolledBack":true}"#,
            r#"{"hidden":1,"more":0,"ok":true,"transactions":[{"active":false,"files":[{"legacyPrevious":false,"operation":"create","target":".config/probe/a.toml"}],"phase":"applied","planId":"PLAN0","retainedDirectories":0,"updatedAt":TIME}],"unreadable":0}"#,
            r#"{"hidden":1,"more":0,"ok":true,"plans":[{"files":[{"operation":"create","target":".config/probe/a.toml"}],"planId":"PLAN0","session":null,"state":"applied","thisTask":false,"updatedAt":TIME}],"unreadable":0}"#,
            r#"{"hidden":0,"more":0,"ok":true,"transactions":[{"active":false,"files":[{"legacyPrevious":false,"operation":"create","target":"undone"},{"legacyPrevious":false,"operation":"create","target":".local/bin/tool"}],"phase":"rolledBack","planId":"PLAN1","retainedDirectories":0,"updatedAt":TIME},{"active":false,"files":[{"legacyPrevious":false,"operation":"create","target":".config/probe/a.toml"}],"phase":"applied","planId":"PLAN0","retainedDirectories":0,"updatedAt":TIME}],"unreadable":0}"#,
            r#"{"hidden":0,"more":0,"ok":true,"plans":[{"files":[{"operation":"create","target":"undone"},{"operation":"create","target":".local/bin/tool"}],"planId":"PLAN1","session":null,"state":"rolledBack","thisTask":false,"updatedAt":TIME},{"files":[{"operation":"create","target":".config/probe/a.toml"}],"planId":"PLAN0","session":null,"state":"applied","thisTask":false,"updatedAt":TIME}],"unreadable":0}"#,
            r#"{"hidden":1,"more":2,"ok":true,"plans":[{"files":[{"operation":"create","target":"x"}],"planId":"PLAN1","session":null,"state":"applied","thisTask":false,"updatedAt":TIME}],"unreadable":3}"#,
        ]
    );
}

#[tokio::test]
async fn a_profile_whose_file_or_reference_is_a_store_is_refused_before_the_helper_is_asked() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let store = directory.path().join("cookies.sqlite");
    fs::write(&store, b"").unwrap();
    for (name, body) in [
        (
            "file.toml",
            "version = 2\n[[files]]\nsource = \"cookies.sqlite\"\ntarget = \"cookies.sqlite\"\n"
                .to_owned(),
        ),
        (
            "reference.toml",
            format!(
                "version = 2\n[[references]]\npath = \"{}\"\n",
                store.display()
            ),
        ),
    ] {
        let profile = directory.path().join(name);
        fs::write(&profile, body).unwrap();
        let error = profile::sync(&client, &profile, false).await.unwrap_err();
        assert_eq!(error.code, Code::Credentials, "{name}: {error}");
    }
}
