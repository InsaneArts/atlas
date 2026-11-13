use super::*;
use crate::{
    environment::{
        list::{labelled, listing},
        profile::{load, selected_environment, sync},
        tests::*,
    },
    test_support::{Helper, completed},
};
use atlas_core::{files::Token, test_support::UID};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::os::unix::fs::symlink;
use tokio::sync::Notify;

#[tokio::test]
async fn a_worker_killed_after_replacing_a_file_stays_pending_until_recovery() {
    const TEST: &str = "user::environment::tests::a_worker_killed_after_replacing_a_file_stays_pending_until_recovery";
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
    if !atlas_core::test_support::isolated(TEST) {
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
    let mut client = crate::test_support::client(&sandbox.root);
    client.transport.peer_uid = nix::unistd::getuid().as_raw();
    fs::create_dir_all(client.transport.path.parent().unwrap()).unwrap();
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
                let output = atlas_core::spawn::output_async(
                    Command::from(atlas_core::test_support::rerun(TEST))
                        .arg("--nocapture")
                        .env(
                            "ATLAS_ENVIRONMENT_TEST_PACKET",
                            request["argv"][2].as_str().unwrap(),
                        ),
                )
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

/// A stage is removed by first moving it to a discarded name, so nothing
/// sees it half removed; what such a removal left is cleared later.
#[test]
fn a_discarded_stage_goes_at_once_and_its_leftovers_are_cleared() {
    let sandbox = Sandbox::new();
    let stage = stage_in(&sandbox.root, &atlas_core::files::token());
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
    let client = crate::test_support::client(&sandbox.root);
    let stage = |inside: &[&str]| {
        let id = atlas_core::files::token();
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

    prune(&client, &user_of(&client), "nobody").await;
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
        let stage = stage_in(&sandbox.root, &atlas_core::files::token());
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
    let mut client = crate::test_support::client(&sandbox.root);
    // An account that resolves to a name, as the helper's does.
    client.config.helper_uid = nix::unistd::getuid().as_raw();
    let missing = apply(&client, &atlas_core::files::token())
        .await
        .unwrap_err();
    assert!(missing.error.contains("There is no plan"), "{missing}");
    assert!(!locked.exists(), "swept once its lock is let go");
    assert!(unknown.exists() && planned.exists() && recent.exists());
}

/// A request that cannot be sent leaves no request folder, which would
/// otherwise read as an unknown outcome forever.
#[tokio::test]
async fn a_request_that_was_never_sent_leaves_nothing() {
    let sandbox = Sandbox::new();
    let client = crate::test_support::client_in(&sandbox.root);

    let stage = stage_in(&sandbox.root, &atlas_core::files::token());
    let request = Request::Rollback {
        id: atlas_core::files::token(),
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
    let client = crate::test_support::client(&sandbox.root);
    fs::create_dir_all(client.transport.path.parent().unwrap()).unwrap();
    let daemon = tokio::net::UnixListener::bind(&client.transport.path).unwrap();
    let _held = client.controller.acquire(false).unwrap();

    let stage = stage_in(&sandbox.root, &atlas_core::files::token());
    let request = Request::Rollback {
        id: atlas_core::files::token(),
        apply: true,
    };
    let refused = dispatch_as(
        &client,
        None,
        "nobody",
        &stage,
        request,
        Duration::from_secs(1),
    )
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
pub(super) fn account() -> String {
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
    let id = atlas_core::files::token();
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
    assert!(
        !client
            .config
            .output_dir
            .join(crate::activity_log::FILE)
            .exists()
    );
}

#[tokio::test]
async fn a_lost_preview_reply_leaves_no_marker() {
    let sandbox = Sandbox::new();
    let (client, helper) = Helper::client(&sandbox.root, UID);
    let account = account();
    let id = atlas_core::files::token();
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
    let stage = stage_in(&sandbox.root, &atlas_core::files::token());
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
async fn an_already_rolled_back_preview_is_reused_without_another_mutation() {
    for (fields, expected) in [
        (json!({}), Undone::RolledBack),
        (
            json!({"keptTargets": ["changed", null, 7, "other"],
                "retainedDirectories": ["nested", null]}),
            Undone::Kept {
                targets: vec!["changed".into(), "other".into()],
                directories: 2,
            },
        ),
        (
            json!({"keptTargets": false, "retainedDirectories": {}, "resumeRequired": true}),
            Undone::RolledBack,
        ),
    ] {
        let sandbox = Sandbox::new();
        let (client, helper) = Helper::client(&sandbox.root, UID);
        let account = account();
        let held = client.controller.acquire(false).unwrap();
        let id = Token::new();
        let stage = stage_in(&sandbox.root, id.as_str());
        let _lock = lock_stage(&stage).unwrap();
        let mut preview = json!({"ok": true, "phase": "rolledBack"});
        preview
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        let mut requests = 0;

        let result = helper
            .serve(
                undo_locked(&client, &held, &account, &stage, &id, false),
                |request| {
                    requests += 1;
                    assert!(!rollback_request(request, &id));
                    Some(worker_reply(0, &preview))
                },
            )
            .await
            .unwrap();

        assert_eq!(result, expected);
        assert_eq!(requests, 1);
        assert!(!stage.exists());
        assert!(client.controller.pending().unwrap().is_empty());
    }
}

#[tokio::test]
async fn an_unknown_preview_phase_never_starts_rollback() {
    for fields in [
        json!({"phase": "future"}),
        json!({}),
        json!({"phase": null}),
        json!({"phase": 7}),
        json!({"phase": true}),
        json!({"phase": []}),
        json!({"phase": {}}),
        json!({"phase": {"applying": null}}),
        json!({"phase": {"applied": null}}),
        json!({"phase": {"rollingBack": null}}),
        json!({"phase": {"rolledBack": null}}),
    ] {
        let sandbox = Sandbox::new();
        let (client, helper) = Helper::client(&sandbox.root, UID);
        let account = account();
        let held = client.controller.acquire(false).unwrap();
        let id = Token::new();
        let stage = stage_in(&sandbox.root, id.as_str());
        let _lock = lock_stage(&stage).unwrap();
        write_plan(&stage, id.as_str(), UID);
        let plan = fs::read(stage.join("plan.json")).unwrap();
        let pending = stage.join("request-pending");
        fs::create_dir(&pending).unwrap();
        fs::write(pending.join("packet.json"), b"unacknowledged request").unwrap();
        let inode = fs::metadata(&stage).unwrap().ino();
        let foreign = sandbox.home.join("foreign");
        fs::write(&foreign, b"foreign bytes").unwrap();
        let mut preview = json!({"ok": true});
        preview
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        let mut requests = 0;

        let result = helper
            .serve(
                undo_locked(&client, &held, &account, &stage, &id, false),
                |request| {
                    requests += 1;
                    assert!(!rollback_request(request, &id), "{preview}");
                    Some(worker_reply(0, &preview))
                },
            )
            .await
            .unwrap();

        assert_eq!(
            result,
            Undone::Unfinished(
                "The helper reported an unknown transaction phase; no rollback was sent".into()
            )
        );
        assert_eq!(requests, 1);
        assert_eq!(fs::metadata(&stage).unwrap().ino(), inode);
        assert_eq!(fs::read(stage.join("plan.json")).unwrap(), plan);
        assert_eq!(
            fs::read(pending.join("packet.json")).unwrap(),
            b"unacknowledged request"
        );
        assert_eq!(fs::read(&foreign).unwrap(), b"foreign bytes");
    }
}

#[tokio::test]
async fn a_known_partially_applied_plan_still_rolls_back() {
    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![
        sandbox.payload("first", b"one"),
        sandbox.payload("second", b"two"),
    ]);
    let state = sandbox.home.join(STATE);
    let stopped = stop_apply(&sandbox, &plan, || {
        fs::read_to_string(state.join("files.json")).is_ok_and(|ledger| ledger.contains("first"))
    });
    assert_eq!(stopped["completedTargets"], json!(["first"]));
    assert_eq!(stopped["phase"], "applying");
    fs::write(sandbox.home.join("first"), b"helper edit").unwrap();
    let (client, helper) = Helper::client(&sandbox.root, UID);
    let account = account();
    let held = client.controller.acquire(false).unwrap();
    let id = Token::parse(&plan.id).unwrap();
    let stage = stage_in(&sandbox.root, &plan.id);
    let _lock = lock_stage(&stage).unwrap();
    let mut applied = Vec::new();

    let result = helper
        .serve(
            undo_locked(&client, &held, &account, &stage, &id, false),
            |request| {
                let apply = rollback_request(request, &id);
                applied.push(apply);
                Some(worker_reply(0, &sandbox.rollback(&plan, apply)))
            },
        )
        .await
        .unwrap();

    assert_eq!(applied, [false, true]);
    assert_eq!(
        result,
        Undone::Kept {
            targets: vec!["first".into()],
            directories: 0
        }
    );
    assert_eq!(
        fs::read(sandbox.home.join("first")).unwrap(),
        b"helper edit"
    );
    assert!(!sandbox.home.join("second").exists());
    assert!(!stage.exists());
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn a_real_rollback_requires_explicit_settlement() {
    for fields in [
        json!({"phase": "rolledBack", "resumeRequired": false}),
        json!({"phase": "rolledBack"}),
        json!({"phase": "rolledBack", "resumeRequired": null}),
        json!({"phase": "rolledBack", "resumeRequired": "false"}),
        json!({"phase": "rolledBack", "resumeRequired": true}),
        json!({"phase": "rollingBack", "resumeRequired": false}),
        json!({"phase": "future", "resumeRequired": false}),
        json!({"phase": {"applying": null}, "resumeRequired": false}),
        json!({"phase": {"applied": null}, "resumeRequired": false}),
        json!({"phase": {"rollingBack": null}, "resumeRequired": false}),
        json!({"phase": {"rolledBack": null}, "resumeRequired": false}),
        json!({"resumeRequired": false}),
    ] {
        let sandbox = Sandbox::new();
        let (client, helper) = Helper::client(&sandbox.root, UID);
        let account = account();
        let held = client.controller.acquire(false).unwrap();
        let id = Token::new();
        let stage = stage_in(&sandbox.root, id.as_str());
        let _lock = lock_stage(&stage).unwrap();
        write_plan(&stage, id.as_str(), UID);
        let mut reply = json!({"ok": true});
        reply
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        let mut applied = Vec::new();

        let result = helper
            .serve(
                undo_locked(&client, &held, &account, &stage, &id, false),
                |request| {
                    let apply = rollback_request(request, &id);
                    applied.push(apply);
                    Some(worker_reply(
                        0,
                        &if apply {
                            reply.clone()
                        } else {
                            json!({"ok": true, "phase": "applied"})
                        },
                    ))
                },
            )
            .await
            .unwrap();

        assert_eq!(applied, [false, true]);
        if fields["phase"] == "rolledBack" && fields["resumeRequired"] == false {
            assert_eq!(result, Undone::RolledBack);
            assert!(!stage.exists());
        } else {
            assert_eq!(
                result,
                Undone::Unfinished("The helper did not finish undoing this copy".into())
            );
            assert!(stage.join("plan.json").exists());
        }
    }
}

#[tokio::test]
async fn an_unfinished_rollback_retains_its_request_across_restart() {
    let sandbox = Sandbox::new();
    let id = Token::new();
    let stage = stage_in(&sandbox.root, id.as_str());
    write_plan(&stage, id.as_str(), UID);
    let plan = fs::read(stage.join("plan.json")).unwrap();
    let kept = sandbox.home.join("changed");
    fs::write(&kept, b"helper edit").unwrap();
    let mut pending: Option<(PathBuf, Vec<u8>)> = None;

    for phase in ["applied", "rollingBack"] {
        let (client, helper) = Helper::client(&sandbox.root, UID);
        let account = account();
        let (held, unknown) = client.controller.acquire_for_cleanup().unwrap();
        assert_eq!(unknown, pending.is_some());
        let lock = lock_stage(&stage).unwrap();
        let mut applied = Vec::new();
        let result = helper
            .serve(
                undo_locked(&client, &held, &account, &stage, &id, false),
                |request| {
                    let apply = rollback_request(request, &id);
                    applied.push(apply);
                    if apply && phase == "applied" {
                        let path = PathBuf::from(request["argv"][2].as_str().unwrap());
                        pending = Some((path.clone(), fs::read(&path).unwrap()));
                        return None;
                    }
                    let reply = if apply {
                        json!({"ok": false, "phase": "rollingBack", "resumeRequired": true,
                            "error": "Interrupted rollback", "keptTargets": ["changed"]})
                    } else {
                        json!({"ok": true, "phase": phase})
                    };
                    Some(worker_reply(0, &reply))
                },
            )
            .await;
        drop(lock);
        if phase == "applied" {
            assert!(result.unwrap_err().uncertain);
        } else {
            assert_eq!(
                result.unwrap(),
                Undone::Unfinished("Interrupted rollback".into())
            );
        }
        assert_eq!(applied, [false, true]);
        assert_eq!(fs::read(stage.join("plan.json")).unwrap(), plan);
        let (path, bytes) = pending.as_ref().unwrap();
        assert_eq!(&fs::read(path).unwrap(), bytes);
        assert_eq!(fs::read(&kept).unwrap(), b"helper edit");
        drop(helper);
        fs::remove_file(&client.transport.path).unwrap();
    }
}

#[tokio::test]
async fn an_apply_still_takes_control_and_stays_pending_when_its_reply_is_lost() {
    const TEST: &str = "user::environment::tests::an_apply_still_takes_control_and_stays_pending_when_its_reply_is_lost";
    if !atlas_core::test_support::isolated(TEST) {
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

#[test]
fn a_frozen_plan_the_helper_never_started_is_listed_as_previewed() {
    let sandbox = Sandbox::new();
    let uid = nix::unistd::getuid().as_raw();
    let previewed = atlas_core::files::token();
    write_plan(&stage_in(&sandbox.root, &previewed), &previewed, uid);
    let unknown = atlas_core::files::token();
    fs::create_dir(stage_in(&sandbox.root, &unknown).join("request-ab12")).unwrap();
    let running = atlas_core::files::token();
    let locked = stage_in(&sandbox.root, &running);
    write_plan(&locked, &running, uid);
    let _held = lock_stage(&locked).unwrap();
    let elsewhere = atlas_core::files::token();
    write_plan(&stage_in(&sandbox.root, &elsewhere), &elsewhere, uid + 1);
    stage_in(&sandbox.root, &atlas_core::files::token());

    let here = frozen(&sandbox.root, uid);
    let nothing = json!({"ok": true, "transactions": [], "more": 0, "unreadable": 0});
    let shown = listing(&here, &as_listed(&nothing), &unlabelled(), false);

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
fn a_plan_the_asker_holds_is_listed_whatever_its_phase_or_age() {
    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![sandbox.payload("file", b"copied")]);
    assert_eq!(sandbox.apply(&plan)["ok"], true);
    assert_eq!(sandbox.rollback(&plan, true)["rolledBack"], true);
    let uid = nix::unistd::getuid().as_raw();
    write_plan(&stage_in(&sandbox.root, &plan.id), &plan.id, uid);

    let here = frozen(&sandbox.root, uid);
    let listed = sandbox.transactions_of(false, std::slice::from_ref(&plan.id));
    let shown = listing(&here, &as_listed(&listed), &unlabelled(), false);

    assert_eq!(listed["transactions"][0]["phase"], "rolledBack");
    assert!(
        shown["plans"].as_array().unwrap().is_empty(),
        "never taken for a plan the helper did not start: {shown}"
    );
    assert_eq!(shown["hidden"], 1);
}

#[test]
fn a_plans_lock_looked_at_for_an_instant_is_waited_out() {
    if !atlas_core::test_support::isolated(
        "user::environment::tests::a_plans_lock_looked_at_for_an_instant_is_waited_out",
    ) {
        return;
    }
    let sandbox = Sandbox::new();
    let stage = stage_in(&sandbox.root, &atlas_core::files::token());
    atomic_write(&stage.join("controller.lock"), b"", 0o600).unwrap();
    let looking = fs::File::open(stage.join("controller.lock")).unwrap();
    FileExt::try_lock_shared(&looking).unwrap();

    let locked = std::thread::scope(|scope| {
        let (notice, contended) = std::sync::mpsc::channel();
        let waiting = scope.spawn(|| {
            atlas_core::files::on_next_lock_contention(notice);
            lock_stage(&stage).map(|_| ())
        });
        let contention = contended.recv_timeout(Duration::from_secs(5));
        drop(looking);
        let result = waiting.join().unwrap();
        assert!(contention.is_ok(), "the plan lock was actually contended");
        result
    });
    assert!(locked.is_ok(), "{locked:?}");

    let held = lock_stage(&stage).unwrap();
    let error = lock_stage(&stage).unwrap_err();
    assert!(error.error.contains("already running"), "{error}");
    drop(held);
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
                id: atlas_core::files::Token::parse(&plan.id).unwrap(),
            },
            State::Made,
        )
        .unwrap();
    let before = client.controller.pending().unwrap();

    let (here, there) = helper
        .serve(
            list_as(&client, &user_of(&client), &account(), false),
            previewing(&sandbox.home),
        )
        .await
        .unwrap();
    let listed = labelled(&client, &here, &there, false).unwrap();

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
    let id = atlas_core::files::token();
    let stage = stage_in(&sandbox.root, &id);
    write_plan(&stage, &id, UID);
    atomic_write(&stage.join("controller.lock"), b"", 0o600).unwrap();
    let then = std::time::SystemTime::now() - UNUSED - Duration::from_secs(60);
    fs::File::open(&stage).unwrap().set_modified(then).unwrap();
    let session = crate::host_session::Key::of(crate::host_session::Host::Codex, "pruning");
    let task = Task::of(&session, None).unwrap();
    client.task = Some(task.clone());
    let plan = Owned::Plan {
        id: atlas_core::files::Token::parse(&id).unwrap(),
    };
    let output = client.output_dir().unwrap().to_owned();
    Claims::in_store(Tasks::at(&output), task.clone())
        .claim(&plan, State::Made)
        .unwrap();

    helper
        .serve(prune(&client, &user_of(&client), &account()), |_| {
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
                &user_of(&client),
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

    let id = atlas_core::files::Token::parse(previewed["planId"].as_str().unwrap()).unwrap();
    assert_eq!(
        task_claims(&client, &task),
        [(Owned::Plan { id }, State::Made)]
    );
}

#[tokio::test]
async fn a_manifest_that_selects_a_secret_is_refused_before_the_helper_is_asked_and_nothing_is_staged()
 {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let manifest = directory.path().join("manifest.toml");
    fs::write(&manifest, b"").unwrap();
    let secret = directory.path().join(".env");
    fs::write(&secret, b"TOKEN=1").unwrap();
    let selections = vec![Selection {
        source: ".env".into(),
        target: ".config/probe/.env".into(),
        executable: false,
    }];
    let error = freeze(&client, &user_of(&client), &manifest, selections, false)
        .await
        .unwrap_err();
    assert_eq!(error.code, Code::Credentials, "{error}");
    assert!(
        !directory.path().join("output").exists(),
        "a stage was made"
    );
}

#[tokio::test]
async fn a_manifest_source_that_is_a_link_named_like_a_secret_is_refused_though_it_leads_elsewhere()
{
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let manifest = directory.path().join("manifest.toml");
    fs::write(&manifest, b"").unwrap();
    let settings = directory.path().join("settings");
    fs::write(&settings, b"TOKEN=1").unwrap();
    symlink(&settings, directory.path().join(".npmrc")).unwrap();
    let selections = vec![Selection {
        source: ".npmrc".into(),
        target: ".npmrc".into(),
        executable: false,
    }];
    let error = freeze(&client, &user_of(&client), &manifest, selections, false)
        .await
        .unwrap_err();
    assert_eq!(error.code, Code::Credentials, "{error}");
}

/// The refusal that reading `sandbox`'s `settings.toml` as a selection comes
/// to, once the helper is verified and before it is asked anything.
async fn refused_as_read(sandbox: &Sandbox) -> atlas_core::error::Error {
    let (client, helper, _task) = tasked(&sandbox.root);
    let selections = vec![Selection {
        source: sandbox.root.join("settings.toml"),
        target: ".config/probe/settings.toml".into(),
        executable: false,
    }];
    let manifest = sandbox.root.join("manifest.toml");
    helper
        .serve(
            freeze_as(
                &client,
                &user_of(&client),
                &account(),
                &manifest,
                selections,
                false,
            ),
            |_| panic!("the helper was asked to preview a store"),
        )
        .await
        .unwrap_err()
}

#[tokio::test]
async fn a_source_that_became_a_link_to_a_store_after_the_first_check_is_refused_as_it_is_read() {
    let sandbox = Sandbox::new();
    let store = sandbox.root.join("cookies.sqlite");
    fs::write(&store, b"").unwrap();
    // Passed the first check as a plain file, then turned into a link before it was read.
    symlink(&store, sandbox.root.join("settings.toml")).unwrap();
    let error = refused_as_read(&sandbox).await;
    assert_eq!(error.code, Code::Credentials, "{error}");
}

#[tokio::test]
async fn a_source_that_became_a_link_through_a_store_named_link_is_refused_as_it_is_read() {
    let sandbox = Sandbox::new();
    let plain = sandbox.root.join("plain.txt");
    fs::write(&plain, b"TOKEN=1").unwrap();
    // Two links: the source to a `.npmrc`, which leads on to an ordinary-looking file.
    let npmrc = sandbox.root.join(".npmrc");
    symlink(&plain, &npmrc).unwrap();
    symlink(&npmrc, sandbox.root.join("settings.toml")).unwrap();
    let error = refused_as_read(&sandbox).await;
    assert_eq!(error.code, Code::Credentials, "{error}");
}

/// The account the stand-in helper is, as a helper must be another
/// account than the one running the test.
fn helper_uid() -> u32 {
    nix::unistd::User::from_name("nobody")
        .unwrap()
        .unwrap()
        .uid
        .as_raw()
}

/// A client of a stand-in helper kept in a fresh folder, with its output
/// folder, a helper home and a profile that selects no files.
struct Syncing {
    _temp: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    output: PathBuf,
    profile: PathBuf,
    client: Client,
    helper: Helper,
}

impl Syncing {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let (client, helper) = Helper::staging(&root, helper_uid());
        let output = client.output_dir().unwrap().to_owned();
        fs::create_dir_all(&output).unwrap();
        let home = root.join("home");
        fs::create_dir(&home).unwrap();
        let profile = root.join("profile.toml");
        fs::write(&profile, "version = 2\n").unwrap();
        Self {
            _temp: temp,
            root,
            home,
            output,
            profile,
            client,
            helper,
        }
    }

    /// The generated sources a sync keeps in the output folder now.
    fn generated(&self) -> Vec<PathBuf> {
        fs::read_dir(&self.output)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("environment-profile-")
            })
            .collect()
    }

    /// The helper's answer to a preview, made on its home.
    fn previewed(&self, id: &str, files: Vec<crate::environment::model::Payload>) -> Value {
        let value = worker::preview(&self.home, helper_uid(), id, files, &|| Ok(())).unwrap();
        completed(&value.to_string())
    }

    /// Sync the profile, applying it when `apply`, until `held` is
    /// notified: none when it was cut short then.
    async fn sync_until(&self, apply: bool, held: &Notify) -> Option<Result<Value>> {
        tokio::select! {
            biased;
            () = held.notified() => None,
            result = sync(&self.client, &self.profile, apply) => Some(result),
        }
    }
}

/// What a run of the environment worker asks of it.
fn asked(request: &Value) -> Option<Request> {
    let packet: Packet = decode(&fs::read(request["argv"][2].as_str()?).ok()?).ok()?;
    Some(packet.request)
}

#[tokio::test]
async fn cancelling_profile_synchronization_removes_its_generated_environment_source() {
    let syncing = Syncing::new();
    let held = Notify::new();

    let finished = syncing
        .helper
        .serve(syncing.sync_until(false, &held), |request| {
            let Some(Request::Preview { .. }) = asked(request) else {
                panic!("only a preview is asked for");
            };
            let sources = syncing.generated();
            assert_eq!(sources.len(), 1, "{sources:?}");
            assert!(sources[0].join("environment.json").exists());
            held.notify_one();
            None
        })
        .await;

    assert!(finished.is_none(), "{finished:?}");
    assert_eq!(syncing.generated(), Vec::<PathBuf>::new());
}

#[tokio::test]
async fn cancelling_after_staging_preserves_recovery_without_retaining_the_generated_source() {
    let syncing = Syncing::new();
    let held = Notify::new();

    let finished = syncing
        .helper
        .serve(syncing.sync_until(true, &held), |request| {
            match asked(request)? {
                Request::Preview { id, files } => Some(syncing.previewed(&id, files)),
                Request::Apply { .. } => {
                    held.notify_one();
                    None
                }
                _ => panic!("only a preview and its apply are asked for"),
            }
        })
        .await;

    assert!(finished.is_none(), "{finished:?}");
    assert_eq!(syncing.generated(), Vec::<PathBuf>::new());
    let stages: Vec<PathBuf> = leftovers(&syncing.root)
        .into_iter()
        .filter(|stage| {
            stage
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("environment-plan-")
        })
        .collect();
    assert_eq!(stages.len(), 1, "{stages:?}");
    let plan: Plan = decode(&fs::read(stages[0].join("plan.json")).unwrap()).unwrap();
    assert_eq!(plan.uid, helper_uid());
    let bytes = selected_environment(&load(&syncing.profile).unwrap()).unwrap();
    assert_eq!(fs::read(&plan.files[0].payload.source).unwrap(), bytes);
    assert!(requests(&stages[0]), "the apply's request is kept");
}

#[tokio::test]
async fn a_finished_profile_synchronization_removes_its_generated_source() {
    let syncing = Syncing::new();
    let synced = syncing
        .helper
        .serve(
            sync(&syncing.client, &syncing.profile, false),
            |request| match asked(request)? {
                Request::Preview { id, files } => Some(syncing.previewed(&id, files)),
                _ => panic!("only a preview is asked for"),
            },
        )
        .await
        .unwrap();
    assert_eq!(synced["ok"], true, "{synced}");
    assert!(synced.get("cleanupWarning").is_none(), "{synced}");
    assert_eq!(syncing.generated(), Vec::<PathBuf>::new());

    let syncing = Syncing::new();
    let failed = syncing
        .helper
        .serve(sync(&syncing.client, &syncing.profile, false), |_| None)
        .await
        .unwrap_err();
    assert!(!failed.error.contains("cleanup"), "{failed:?}");
    assert_eq!(syncing.generated(), Vec::<PathBuf>::new());
}

#[tokio::test]
async fn a_generated_source_that_will_not_go_is_reported() {
    // Permissions do not hold root back, so nothing would fail.
    if nix::unistd::geteuid().is_root() {
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    let syncing = Syncing::new();
    let synced = syncing
        .helper
        .serve(
            sync(&syncing.client, &syncing.profile, false),
            |request| match asked(request)? {
                Request::Preview { id, files } => {
                    for source in syncing.generated() {
                        fs::set_permissions(&source, fs::Permissions::from_mode(0o500)).unwrap();
                    }
                    Some(syncing.previewed(&id, files))
                }
                _ => panic!("only a preview is asked for"),
            },
        )
        .await
        .unwrap();
    let sources = syncing.generated();
    for source in &sources {
        fs::set_permissions(source, fs::Permissions::from_mode(0o700)).unwrap();
    }

    assert_eq!(sources.len(), 1, "{sources:?}");
    let warning = synced["cleanupWarning"].as_str().unwrap();
    assert!(
        warning.contains(&sources[0].to_string_lossy().into_owned()),
        "{warning}"
    );
}
