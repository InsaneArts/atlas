use super::*;
use crate::{
    environment::{
        problem,
        tests::{Sandbox, stage_in, task_claims, tasked, user_of, worker_reply, write_plan},
    },
    user::environment::tests::account,
};
use atlas_core::test_support::UID;
use serde_json::json;
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
    time::Duration,
};

#[test]
fn a_stage_keeps_requests_whose_outcome_is_unknown() {
    let sandbox = Sandbox::new();
    let stage = stage_in(&sandbox.root, &atlas_core::files::token());
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
    let stage = stage_in(&sandbox.root, &atlas_core::files::token());
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
        .join(format!("environment-plan-{}", atlas_core::files::token()));
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

#[test]
fn results_name_a_retained_plan_only_when_one_exists() {
    let sandbox = Sandbox::new();
    let id = atlas_core::files::token();
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
    let id = atlas_core::files::token();
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
    let id = atlas_core::files::token();
    let plan = stage_in(&sandbox.root, &id);
    let run = sandbox.root.join(format!("environment-run-{id}"));
    fs::create_dir(&run).unwrap();
    fs::create_dir(sandbox.root.join("environment-plan-short")).unwrap();
    fs::create_dir(sandbox.root.join(format!("other-{id}"))).unwrap();
    let file = atlas_core::files::token();
    fs::write(sandbox.root.join(format!("environment-plan-{file}")), b"").unwrap();

    let linked = atlas_core::files::token();
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
    let id = atlas_core::files::token();
    let stage = stage_in(&sandbox.root, &id);

    fs::create_dir(sandbox.root.join(format!("environment-run-{id}"))).unwrap();
    fs::File::open(&stage)
        .unwrap()
        .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(1))
        .unwrap();

    assert_eq!(
        older(
            &sandbox.root,
            "environment-plan-",
            std::time::Duration::ZERO
        ),
        vec![(id.clone(), stage.clone())],
        "a command's folder is never a plan"
    );
    assert!(older(&sandbox.root, "environment-plan-", UNUSED).is_empty());
    assert!(!prunable(&stage, &id, 501), "no plan to ask about");

    write_plan(&stage, &id, 501);
    assert!(prunable(&stage, &id, 501));
    assert!(!prunable(&stage, &id, 502), "another helper's plan");
    assert!(!prunable(&stage, &atlas_core::files::token(), 501));

    fs::create_dir(stage.join("request-ab12")).unwrap();
    assert!(!prunable(&stage, &id, 501));
}

/// A command's folder, kept when the command was stopped or its reply was
/// lost, goes once it is older than a week.
#[test]
fn old_command_folders_are_pruned() {
    let sandbox = Sandbox::new();
    let id = atlas_core::files::token();
    let run = sandbox.root.join(format!("environment-run-{id}"));
    fs::create_dir(&run).unwrap();
    stage_in(&sandbox.root, &atlas_core::files::token());
    fs::File::open(&run)
        .unwrap()
        .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(1))
        .unwrap();

    assert_eq!(
        older(&sandbox.root, "environment-run-", std::time::Duration::ZERO)
            .into_iter()
            .map(|(_, stage)| stage)
            .collect::<Vec<_>>(),
        vec![run.clone()]
    );
    assert!(older(&sandbox.root, "environment-run-", UNUSED).is_empty());

    prune_runs(&sandbox.root);
    assert!(run.exists(), "a recent command's folder stays");
}

#[tokio::test]
async fn an_apply_claims_its_plan_for_the_task_before_the_helper_is_asked() {
    use crate::task::{Owned, State};

    let sandbox = Sandbox::new();
    let (client, helper, task) = tasked(&sandbox.root);
    let id = atlas_core::files::token();
    write_plan(&stage_in(&sandbox.root, &id), &id, UID);
    let plan = Owned::Plan {
        id: atlas_core::files::Token::parse(&id).unwrap(),
    };

    let mut asked = Vec::new();
    let applied = helper
        .serve(
            apply_frozen(&client, &user_of(&client), &account(), &id),
            |_| {
                asked.push(task_claims(&client, &task));
                Some(worker_reply(
                    0,
                    &json!({"ok": true, "planId": id, "phase": "applied", "resumeRequired": false}),
                ))
            },
        )
        .await
        .unwrap();

    assert_eq!(applied["phase"], "applied");
    assert_eq!(asked, [vec![(plan.clone(), State::Made)]]);
    assert_eq!(task_claims(&client, &task), [(plan, State::Made)]);
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
