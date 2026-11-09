use super::*;
use crate::environment::{
    STATE, problem,
    stage::rolled_back,
    tests::{Sandbox, stage_in, stop_apply},
};
use atlas_core::error::Result;
use serde_json::{Value, json};
use std::{
    cell::Cell,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
};

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

/// A rollback's own folder goes once nothing in it is unconfirmed, also
/// after a refusal (a mistyped ID); a frozen plan's folder goes only with
/// a finished undo.
#[test]
fn rollback_removes_its_own_folder_unless_a_request_is_unconfirmed() {
    let sandbox = Sandbox::new();
    let stage = stage_in(&sandbox.root, &atlas_core::files::token());
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
        &atlas_core::files::token(),
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
        .join(format!("transaction-{}", atlas_core::files::token()));
    fs::create_dir(&orphan).unwrap();
    fs::write(orphan.join("before-0"), b"secret").unwrap();

    assert_eq!(sandbox.apply(&plan)["ok"], true);
    assert!(!orphan.exists());
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
        &atlas_core::files::token(),
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
