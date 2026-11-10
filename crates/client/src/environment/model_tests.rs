use super::*;
use crate::environment::{
    problem,
    stage::settled,
    tests::{Sandbox, file_progress_at},
};
use atlas_core::files::Lock;
use serde_json::json;
use std::fs;

fn persisted_progress_resumes(operation: Operation) {
    for boundary in [
        "initial",
        "unrecorded prepared inode",
        "prepared evidence",
        "installation",
        "apply ledger",
        "applied acknowledgement",
        "applied phase",
        "rollback intent",
        "unrecorded restore inode",
        "restore evidence",
        "reverting intent",
        "file restoration",
        "restore ledger",
        "restored acknowledgement",
        "kept ledger",
        "kept acknowledgement",
    ] {
        if (operation == Operation::Unchanged
            && ["unrecorded prepared inode", "prepared evidence"].contains(&boundary))
            || (matches!(operation, Operation::Create | Operation::Unchanged)
                && ["unrecorded restore inode", "restore evidence"].contains(&boundary))
        {
            continue;
        }
        let sandbox = Sandbox::new();
        let plan = file_progress_at(&sandbox, operation, boundary);
        let store = state::Store::open(&sandbox.home, Lock::Shared).unwrap();
        let transaction = store.transaction(&plan.id).unwrap().unwrap();
        let bytes = fs::read(store.folder(&plan.id).join("transaction.json")).unwrap();
        assert_eq!(serde_json::to_vec_pretty(&transaction).unwrap(), bytes);
        let resume_apply = matches!(transaction.phase, Phase::Applying | Phase::Applied);
        drop(store);
        if resume_apply {
            assert_eq!(
                sandbox.apply(&plan)["ok"],
                true,
                "{operation:?}: {boundary}"
            );
        }
        let result = sandbox.rollback(&plan, true);
        assert_eq!(
            result["rolledBack"], true,
            "{operation:?}: {boundary}: {result}"
        );
        let path = sandbox.home.join("file");
        if boundary.starts_with("kept") {
            assert_eq!(fs::read(path).unwrap(), b"helper edit");
            assert_eq!(result["keptTargets"], json!(["file"]));
        } else if operation == Operation::Create {
            assert!(!path.exists());
        } else {
            let expected = if operation == Operation::Update {
                b"before".as_slice()
            } else {
                b"after".as_slice()
            };
            assert_eq!(fs::read(&path).unwrap(), expected);
            assert_eq!(
                fs::metadata(path).unwrap().mode() & 0o777,
                plan.files[0].before.as_ref().unwrap().mode
            );
        }
        assert_eq!(sandbox.rollback(&plan, true)["rolledBack"], true);
    }
}

#[test]
fn every_persisted_create_transition_can_resume_after_restart() {
    persisted_progress_resumes(Operation::Create);
}

#[test]
fn every_persisted_update_transition_can_resume_after_restart() {
    persisted_progress_resumes(Operation::Update);
}

#[test]
fn every_persisted_permission_transition_can_resume_after_restart() {
    persisted_progress_resumes(Operation::Permissions);
}

#[test]
fn every_persisted_unchanged_transition_can_resume_after_restart() {
    persisted_progress_resumes(Operation::Unchanged);
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
fn only_a_confirmed_finish_settles_a_plan() {
    let done = json!({"ok": true, "phase": "applied", "resumeRequired": false});
    assert!(settled(&Ok(done.clone()), Phase::Applied));
    assert!(!settled(&Ok(done), Phase::RolledBack));

    for value in [
        json!({"ok": true, "phase": "applied", "resumeRequired": true}),
        json!({"ok": false, "phase": "applied", "resumeRequired": false}),
        json!({"ok": true, "phase": "applied"}),
    ] {
        assert!(!settled(&Ok(value.clone()), Phase::Applied), "{value}");
    }

    assert!(!settled(&Err(problem("lost")), Phase::Applied));
}

#[test]
fn only_an_apply_an_undo_with_apply_and_a_run_change_the_helper() {
    let id = atlas_core::files::token;
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
