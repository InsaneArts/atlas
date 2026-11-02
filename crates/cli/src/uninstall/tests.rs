#[test]
fn removing_a_secondary_opener_keeps_control_and_pending_work_after_interruption() {
    if !atlas_core::test_support::isolated(
        "uninstall::tests::removing_a_secondary_opener_keeps_control_and_pending_work_after_interruption",
    ) {
        return;
    }
    let directory = atlas_client::test_support::interrupted_executor(
        "uninstall::tests::an_opener_removal_test_caller",
        "opener",
    );
    assert!(!directory.path().join("opener").exists());

    let controller = Controller::at(7001, &directory.path().join("state"));
    let teardown = controller.acquire(true).unwrap();
    assert!(
        setup::executor::setup_transaction(&controller, || -> Result<Value> {
            panic!("a competing transaction cannot dispatch")
        })
        .is_err()
    );
    drop(teardown);
    let interrupted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        setup::executor::setup_transaction(&controller, || -> Result<Value> {
            fs::write(directory.path().join("removed"), b"").unwrap();
            panic!("the removal acknowledgement was lost");
        })
    }));
    assert!(interrupted.is_err());
    assert_eq!(controller.unresolved().unwrap().len(), 1);
    setup::executor::setup_transaction(&controller, || Ok(json!({"ok": true, "removed": true})))
        .unwrap();
    assert_eq!(
        controller.pending().unwrap().len(),
        1,
        "only acknowledged work is settled"
    );
}

/// A separate caller with an executor that removes only a temporary opener.
#[test]
fn an_opener_removal_test_caller() {
    let Some(directory) = atlas_core::test_support::directory() else {
        return;
    };
    let controller = Controller::at(7001, &directory.join("state"));
    let _ = setup::executor::setup_transaction(&controller, || {
        atlas_client::test_support::pause_executor(
            &directory,
            "onboarding::tests::an_executor_test_worker",
        );
        Ok(json!({"ok": true, "removed": true}))
    });
}

#[test]
fn input_shutdown_stays_pending_when_its_caller_dies_after_dispatch() {
    use std::{process::Stdio, time::Duration};
    let directory = tempfile::tempdir().unwrap();
    let mut caller =
        atlas_core::test_support::rerun("uninstall::tests::an_input_shutdown_test_caller")
            .arg("--nocapture")
            .env("ATLAS_MUTATION_TEST_DIRECTORY", directory.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
    for _ in 0..500 {
        if directory.path().join("dispatched").exists() || caller.try_wait().unwrap().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let controller = Controller::at(7001, &directory.path().join("state"));
    let excluded = controller.acquire(true).is_err();
    let before = controller.pending().unwrap();
    let _ = caller.kill();
    let _ = caller.wait();
    assert!(directory.path().join("dispatched").exists());
    assert!(directory.path().join("claimed-before-dispatch").exists());
    assert!(excluded);
    assert_eq!(before.len(), 1);
    assert_eq!(controller.unresolved().unwrap(), before);
    assert!(controller.acquire(false).is_err());
}

/// A shutdown dispatch stand-in that deliberately never acknowledges completion.
#[tokio::test]
async fn an_input_shutdown_test_caller() {
    let Some(directory) = atlas_core::test_support::directory() else {
        return;
    };
    let controller = Controller::at(7001, &directory.join("state"));
    let _ownership = controller.acquire(true).unwrap();
    let _ = stop_input_claimed(&controller, async {
        if controller.pending().unwrap().len() == 1 {
            fs::write(directory.join("claimed-before-dispatch"), b"").unwrap();
        }
        fs::write(directory.join("dispatched"), b"shutdown sent").unwrap();
        std::future::pending::<Result<()>>().await
    })
    .await;
}

#[tokio::test]
async fn input_shutdown_settles_completion_and_definite_refusal_but_preserves_uncertainty() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(7001, directory.path());
    let _ownership = controller.acquire(true).unwrap();
    let other = controller.quarantine().unwrap();
    for outcome in [
        Ok(()),
        Err(problem("refused")),
        Err(problem("reply lost").uncertain()),
    ] {
        let uncertain = outcome.as_ref().is_err_and(|error| error.uncertain);
        let succeeded = outcome.is_ok();
        let before = controller.pending().unwrap().len();
        let result = stop_input_claimed(&controller, async {
            assert_eq!(controller.pending().unwrap().len(), before + 1);
            assert!(controller.acquire(true).is_err());
            outcome
        })
        .await;
        assert_eq!(result.is_ok(), succeeded);
        assert_eq!(
            controller.pending().unwrap().len(),
            before + usize::from(uncertain)
        );
    }
    drop(other);
}
#[test]
fn interrupted_retirement_keeps_every_unacknowledged_mutation_pending() {
    if !atlas_core::test_support::isolated(
        "uninstall::tests::interrupted_retirement_keeps_every_unacknowledged_mutation_pending",
    ) {
        return;
    }
    let requests = [
        Request::ForgetOpener { uid: 7001 },
        Request::Retire {
            uid: 7001,
            label: LABEL.into(),
            sha256: "a".repeat(64),
            missing_ok: true,
        },
    ];
    for request in requests {
        let dir = tempfile::tempdir().unwrap();
        let controller = Controller::at(7001, &dir.path().join("state"));
        let held = controller.acquire(true).unwrap();
        let claimed_before_dispatch = std::cell::Cell::new(false);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            retirement_request(&controller, &request, || {
                claimed_before_dispatch.set(controller.pending().unwrap().len() == 1);
                fs::write(dir.path().join("changed"), b"retired").unwrap();
                panic!("the worker changed a file but its caller received no acknowledgement");
            })
        }));
        assert!(outcome.is_err());
        assert!(dir.path().join("changed").exists());
        assert_eq!(controller.unresolved().unwrap().len(), 1);
        assert!(claimed_before_dispatch.get());
        drop(held);
        assert!(controller.acquire(false).is_err());
        let held = controller.acquire(true).unwrap();
        controller.clear().unwrap();
        retirement_request(&controller, &request, || Ok(json!({"ok": true}))).unwrap();
        assert!(
            controller.pending().unwrap().is_empty(),
            "acknowledged retirement settles its own claim"
        );
        drop(held);
        assert!(controller.acquire(false).is_ok());
    }
}

#[test]
fn deletion_without_a_complete_acknowledgement_stays_pending() {
    let dir = tempfile::tempdir().unwrap();
    let controller = Controller::at(7001, &dir.path().join("state"));
    let held = controller.acquire(true).unwrap();
    let stand_in = dir.path().join("account");
    fs::write(&stand_in, b"temporary stand-in").unwrap();
    let claimed_before_dispatch = std::cell::Cell::new(false);
    let outcome: Result<()> = setup::executor::setup_transaction_held(&controller, || {
        claimed_before_dispatch.set(controller.pending().unwrap().len() == 1);
        fs::remove_file(&stand_in).unwrap();
        Err(problem(
            "deletion completed but cleanup or its acknowledgement was lost",
        ))
    });
    assert!(outcome.is_err());
    assert!(!stand_in.exists());
    assert_eq!(controller.unresolved().unwrap().len(), 1);
    assert!(claimed_before_dispatch.get());
    let other = controller.quarantine().unwrap();
    setup::executor::setup_transaction_held(&controller, || Ok(())).unwrap();
    assert_eq!(
        controller.pending().unwrap().len(),
        2,
        "success settles only its own generation"
    );
    drop(other);
    drop(held);
    assert!(controller.acquire(false).is_err());
}

#[tokio::test]
async fn an_unrecorded_helper_never_uses_its_daemons_word_to_adopt_an_account() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = Client::new(Config {
        helper_uid: 7001,
        socket_path: dir.path().join("no-daemon.sock"),
        input_socket: dir.path().join("no-input.sock"),
        identity_probe: dir.path().join("atlas"),
        output_dir: dir.path().join("runs"),
    })
    .unwrap();
    client.controller = Controller::at(7001, &dir.path().join("control"));
    let pending = client.controller.quarantine().unwrap();
    let generation = pending.generation().to_owned();
    drop(pending);
    // A surviving daemon only proves its numeric user ID, even when the
    // current directory entry has the same name as the deleted helper.
    let found = Found {
        app: dir.path().join("Atlas.app"),
        executable: dir.path().join("atlas"),
        client: Some(client),
        unloaded: None,
        helper: Some("helper".into()),
        known: Some(Standing::Unrecorded(Identity {
            uid: 7001,
            name: "helper".into(),
            generated_uid: "11111111-2222-3333-4444-555555555555".into(),
        })),
        data: dir.path().join("data"),
        leftovers: vec![],
    };
    let mut live = Live {
        found: &found,
        config_path: &dir.path().join("settings"),
        keep_config: false,
        yes: true,
        removed: vec![],
        warnings: vec![],
        hint: None,
        deleted: false,
        exclusion: None,
        progress: &silent,
        ask: &agree,
    };
    live.settle().await.unwrap();
    assert!(live.exclusion.is_some());
    let controller = &live.found.client.as_ref().unwrap().controller;
    assert_eq!(controller.pending().unwrap(), [generation]);
    assert_eq!(controller.acquire(true).unwrap_err().code, "busy");
    assert!(retiring(live.found.known.as_ref()).is_none());
    assert!(live.removed.is_empty());
    live.hint();
    let hint = live.hint.unwrap();
    assert!(hint.contains("no recorded identity"));
    assert!(hint.contains("System Settings > Users & Groups"));
}

#[test]
fn a_linked_cache_is_refused_without_unlinking_control_state() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("state");
    let controller = atlas_client::control::Controller::at(7001, &target);
    let held = controller.acquire(true).unwrap();
    let claim = controller.quarantine().unwrap();
    let caches = dir.path().join("Atlas");
    std::os::unix::fs::symlink(&target, &caches).unwrap();
    let error = remove_caches(&caches, &mut Vec::new(), &silent).unwrap_err();
    assert!(error.error.contains("symbolic link"));
    assert!(
        fs::symlink_metadata(&caches)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(controller.pending().unwrap(), [claim.generation()]);
    assert!(
        atlas_client::control::Controller::at(7001, &caches)
            .acquire(true)
            .is_err()
    );
    assert!(controller.acquire(true).is_err());
    drop(claim);
    drop(held);
}

use super::log_out::{Came, Next, Way, after, strictly};
use super::*;
fn silent(_: Progress<'_>) {}
fn agree(_: Question) -> Result<bool> {
    Ok(true)
}

#[test]
fn only_links_to_the_app_are_removed() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let executable = root.join("Atlas.app/Contents/MacOS/atlas");
    fs::create_dir_all(executable.parent().unwrap()).unwrap();
    fs::write(&executable, b"").unwrap();

    let ours = root.join("ours");
    std::os::unix::fs::symlink(&executable, &ours).unwrap();
    assert!(remove_link_to(&ours, &executable).unwrap());
    assert!(!ours.exists());

    let theirs = root.join("theirs");
    fs::write(&theirs, b"someone's file").unwrap();
    assert!(!remove_link_to(&theirs, &executable).unwrap());
    assert!(theirs.exists(), "a regular file is left alone");

    let elsewhere = root.join("elsewhere");
    std::os::unix::fs::symlink(&theirs, &elsewhere).unwrap();
    assert!(!remove_link_to(&elsewhere, &executable).unwrap());
    assert!(!remove_link_to(&root.join("missing"), &executable).unwrap());
}

/// The shared folder goes only when nothing but Finder's .DS_Store is
/// left in it.
#[test]
fn only_an_empty_folder_of_this_accounts_is_removed() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("Atlas");

    assert!(!remove_own_empty_folder(&folder), "nothing there");

    fs::create_dir(&folder).unwrap();
    assert!(remove_own_empty_folder(&folder));
    assert!(!folder.exists());

    fs::create_dir(&folder).unwrap();
    fs::write(folder.join(".DS_Store"), b"").unwrap();
    assert!(remove_own_empty_folder(&folder));
    assert!(!folder.exists());

    fs::create_dir(&folder).unwrap();
    fs::write(folder.join(".DS_Store"), b"").unwrap();
    fs::write(folder.join("notes.txt"), b"someone's").unwrap();
    assert!(!remove_own_empty_folder(&folder));
    assert!(folder.join("notes.txt").exists() && folder.join(".DS_Store").exists());

    fs::remove_file(folder.join("notes.txt")).unwrap();
    fs::create_dir(folder.join("Open")).unwrap();
    assert!(!remove_own_empty_folder(&folder), "a folder inside stays");

    let link = dir.path().join("link");
    std::os::unix::fs::symlink(dir.path().join("empty"), &link).unwrap();
    fs::create_dir(dir.path().join("empty")).unwrap();
    assert!(!remove_own_empty_folder(&link), "a link is never followed");
    assert!(dir.path().join("empty").exists());

    // A folder another account (here root) owns stays.
    assert!(!remove_own_empty_folder(Path::new("/var/empty")));
}

#[test]
fn caches_go_and_a_link_there_is_not_followed() {
    let dir = tempfile::tempdir().unwrap();
    let caches = dir.path().join("Caches/Atlas");
    let mut removed = Vec::new();

    remove_caches(&caches, &mut removed, &silent).unwrap();
    assert!(removed.is_empty(), "nothing there");

    fs::create_dir_all(caches.join("update")).unwrap();
    fs::write(caches.join("scratch"), b"").unwrap();
    assert!(!remove_caches(&caches, &mut removed, &silent).unwrap());
    assert!(!caches.exists());
    assert_eq!(removed, vec![json!(caches)]);

    let target = dir.path().join("elsewhere");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("keep"), b"").unwrap();
    std::os::unix::fs::symlink(&target, &caches).unwrap();
    assert!(remove_caches(&caches, &mut removed, &silent).is_err());
    assert!(target.join("keep").exists());
}

#[test]
fn a_helper_held_while_its_caches_go_stays_held() {
    if !atlas_core::test_support::isolated(
        "uninstall::tests::a_helper_held_while_its_caches_go_stays_held",
    ) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let caches = dir.path().join("Caches/Atlas");
    let controller = atlas_client::control::Controller::at(60001, &caches);
    let held = controller.acquire(true).unwrap();
    fs::create_dir_all(caches.join("update/staging")).unwrap();
    fs::write(caches.join("update/staging/Atlas.zip"), b"").unwrap();

    assert!(!remove_caches(&caches, &mut Vec::new(), &silent).unwrap());
    assert!(!caches.join("update").exists(), "the scratch goes");
    let other = atlas_client::control::Controller::at(60001, &caches);
    assert_eq!(
        other.acquire(true).unwrap_err().code,
        "busy",
        "another controller finds the same lock, still held"
    );
    drop(held);
    assert!(other.acquire(true).is_ok());
}

#[test]
fn an_action_of_unknown_outcome_keeps_its_record_through_uninstall() {
    let dir = tempfile::tempdir().unwrap();
    let caches = dir.path().join("Caches/Atlas");
    let controller = atlas_client::control::Controller::at(60001, &caches);
    // One cut short, and one still being carried out.
    let lost = controller.quarantine().unwrap();
    let generation = lost.generation().to_owned();
    drop(lost);
    let running = controller.quarantine().unwrap();

    assert!(remove_caches(&caches, &mut Vec::new(), &silent).unwrap());
    let pending = controller.pending().unwrap();
    assert!(pending.contains(&generation), "{pending:?}");
    assert!(pending.contains(&running.generation().to_owned()));
    assert_eq!(
        controller.unresolved().unwrap(),
        [generation],
        "the claim of the one still running stays too"
    );
}

#[test]
fn only_this_accounts_own_files_are_removed() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("onboarding.json");
    fs::write(&file, b"{}").unwrap();
    assert!(remove_own_file(&file).unwrap());
    assert!(!file.exists());
    assert!(!remove_own_file(&file).unwrap(), "nothing there");

    let target = dir.path().join("target.json");
    fs::write(&target, b"{}").unwrap();
    std::os::unix::fs::symlink(&target, &file).unwrap();
    assert!(!remove_own_file(&file).unwrap(), "a link is left alone");
    assert!(target.exists());

    assert!(!remove_own_file(dir.path()).unwrap(), "only a file");
    // A file another account owns (here root's) stays.
    assert!(!remove_own_file(Path::new("/etc/hosts")).unwrap());
}

/// An unknown outcome blocks uninstall while the helper could settle
/// it: its account exists and it is not known to be logged out, even
/// when Atlas does not answer there now.
#[test]
fn a_helper_that_could_answer_again_must_settle_an_unknown_outcome() {
    assert_eq!(cannot_settle(true, Some(true)), None);
    assert_eq!(cannot_settle(true, None), None, "unknown is not logged out");
    assert_eq!(
        cannot_settle(true, Some(false)),
        Some("the helper is logged out")
    );
    assert_eq!(
        cannot_settle(false, Some(true)),
        Some("the helper account no longer exists")
    );
}

fn facts() -> Facts {
    Facts {
        settings: true,
        account: true,
        other: false,
        logged_in: true,
        on_screen: false,
        remove_helper: false,
    }
}

fn at(steps: &[Step], step: Step) -> usize {
    steps
        .iter()
        .position(|found| *found == step)
        .unwrap_or_else(|| panic!("no {step:?} in {steps:?}"))
}

#[test]
fn the_helper_is_logged_out_before_its_job_is_retired() {
    let steps = plan(&facts()).unwrap();
    assert_eq!(
        steps,
        [
            Step::Settle,
            Step::LogOut,
            Step::Retire,
            Step::Rest,
            Step::Hint,
            Step::Caches
        ]
    );

    // Logged out when the plan was made: the log-out is still planned,
    // since whether it is logged in is read again when it runs.
    let logged_out = plan(&Facts {
        logged_in: false,
        ..facts()
    })
    .unwrap();
    assert_eq!(logged_out, steps);
}

#[test]
fn nothing_is_retired_or_deleted_while_the_helper_is_logged_in() {
    for remove_helper in [false, true] {
        let steps = plan(&Facts {
            remove_helper,
            ..facts()
        })
        .unwrap();
        let log_out = at(&steps, Step::LogOut);
        for later in [Step::Retire, Step::Rest] {
            assert!(log_out < at(&steps, later), "{later:?}");
        }
    }

    // Every way failed: stop, never go on to remove anything.
    assert_eq!(after(Way::Root, Came::Stayed, true), Next::Stop);
    assert_eq!(after(Way::Root, Came::Held, true), Next::Stop);
}

#[test]
fn root_ends_the_login_only_after_the_helpers_own_atlas_could_not() {
    assert_eq!(after(Way::Ask, Came::Gone, false), Next::Done);
    assert_eq!(after(Way::Ask, Came::Stayed, false), Next::Try(Way::Root));
    assert_eq!(after(Way::Ask, Came::Held, true), Next::Try(Way::Force));
    assert_eq!(after(Way::Ask, Came::Held, false), Next::AskToForce);
    assert_eq!(after(Way::Force, Came::Gone, false), Next::Done);
    assert_eq!(after(Way::Force, Came::Stayed, false), Next::Try(Way::Root));
    assert_eq!(after(Way::Force, Came::Held, true), Next::Try(Way::Root));
    assert_eq!(after(Way::Root, Came::Gone, false), Next::Done);
}

#[test]
fn the_way_to_delete_the_helper_is_shown_only_after_it_is_logged_out() {
    let steps = plan(&facts()).unwrap();
    assert!(at(&steps, Step::LogOut) < at(&steps, Step::Hint));
    assert_eq!(steps[steps.len() - 2..], [Step::Hint, Step::Caches]);

    let gone = plan(&Facts {
        account: false,
        ..facts()
    })
    .unwrap();
    assert!(!gone.contains(&Step::Hint), "no account to delete");
}

#[test]
fn the_helper_account_is_deleted_only_after_it_is_logged_out() {
    let steps = plan(&Facts {
        remove_helper: true,
        ..facts()
    })
    .unwrap();
    assert!(at(&steps, Step::LogOut) < at(&steps, Step::RemoveHelper));
    assert!(at(&steps, Step::Retire) < at(&steps, Step::RemoveHelper));
    assert!(!steps.contains(&Step::Hint));
}

#[test]
fn nothing_happens_while_the_helper_is_on_screen() {
    let error = plan(&Facts {
        on_screen: true,
        remove_helper: true,
        ..facts()
    })
    .unwrap_err();
    assert_eq!(error.code, Code::Uninstall);
    assert_eq!(
        error.error,
        "Switch back to your own account first, then run atlas uninstall again."
    );
}

#[test]
fn without_settings_only_this_accounts_own_files_go() {
    let steps = plan(&Facts {
        settings: false,
        account: false,
        other: false,
        logged_in: false,
        on_screen: true,
        remove_helper: true,
    })
    .unwrap();
    assert_eq!(steps, [Step::Rest, Step::Caches]);
}

#[test]
fn another_account_given_the_helpers_user_id_is_left_alone() {
    let steps = plan(&Facts {
        account: false,
        other: true,
        on_screen: true,
        remove_helper: true,
        ..facts()
    })
    .unwrap();
    assert_eq!(
        steps,
        [Step::Settle, Step::Retire, Step::Rest, Step::Caches]
    );
}

#[test]
fn the_helper_is_the_account_recorded_only_while_it_still_is() {
    let opie = Identity {
        uid: 502,
        name: "opie".into(),
        generated_uid: "6DB4B7AF-BE3C-486C-BD46-1B4558946919".into(),
    };
    let reused = Identity {
        generated_uid: "11111111-2222-3333-4444-555555555555".into(),
        ..opie.clone()
    };
    let same = Standing::Same(opie.clone());
    assert_eq!(retiring(Some(&same)), Some(&opie));
    assert_eq!(retiring(Some(&Standing::Replaced)), None);
    assert_eq!(retiring(Some(&Standing::Unknown)), None, "unreadable");
    assert_eq!(retiring(Some(&Standing::Unrecorded(reused))), None);
}

#[test]
fn only_the_recorded_helpers_job_is_retired() {
    let opie = Identity {
        uid: 502,
        name: "opie".into(),
        generated_uid: "6DB4B7AF-BE3C-486C-BD46-1B4558946919".into(),
    };
    let verified = Standing::Same(opie.clone());
    assert_eq!(retiring(Some(&verified)), Some(&opie));
    assert_eq!(retiring(Some(&Standing::Replaced)), None, "left alone");
    assert_eq!(retiring(None), None, "gone");
    assert_eq!(
        retiring(Some(&Standing::Unrecorded(opie))),
        None,
        "never taken for the helper unrecorded"
    );
}

#[test]
fn only_the_strict_look_says_the_helper_is_gone() {
    assert_eq!(strictly(Came::Gone, false), Came::Stayed, "root said so");
    assert_eq!(strictly(Came::Stayed, true), Came::Gone);
    assert_eq!(strictly(Came::Held, false), Came::Held);
    assert_eq!(strictly(Came::Gone, true), Came::Gone);
}

/// Uninstall with stand-ins for every step: each look at whether the
/// helper is logged out answers from `looks` (logged out once they run
/// out), and each step notes whether another controller could take
/// control of the helper while it ran.
struct Script {
    looks: std::collections::VecDeque<bool>,
    steps: Vec<String>,
    controller: atlas_client::control::Controller,
    state: PathBuf,
    held: Option<Ownership>,
    shutdown: Option<Result<()>>,
    warnings: Vec<String>,
}

impl Script {
    fn new(dir: &Path, looks: &[bool]) -> Self {
        let state = dir.join("state");
        Self {
            looks: looks.iter().copied().collect(),
            steps: Vec::new(),
            controller: atlas_client::control::Controller::at(60001, &state),
            state,
            held: None,
            shutdown: None,
            warnings: Vec::new(),
        }
    }

    /// Note `step`, and whether another controller found the helper
    /// taken while it ran.
    fn note(&mut self, step: &str) {
        let other = atlas_client::control::Controller::at(60001, &self.state);
        let taken = other.acquire(true).is_err();
        self.steps
            .push(format!("{step}{}", if taken { "" } else { " (free)" }));
    }
}

impl Teardown for Script {
    async fn settle(&mut self) -> Result<()> {
        self.held = Some(if let Some(outcome) = self.shutdown.take() {
            settle_with(
                &self.controller,
                &mut self.warnings,
                async { Ok((true, Some(true), Some(4242))) },
                |_| async { outcome },
                &silent,
            )
            .await?
        } else {
            self.controller.acquire(true)?
        });
        self.note("settle");
        Ok(())
    }

    fn logged_out(&mut self) -> bool {
        let look = self.looks.pop_front().unwrap_or(true);
        self.steps.push(format!("look {look}"));
        look
    }

    async fn log_out(&mut self) -> Result<()> {
        self.note("log out");
        Ok(())
    }

    fn retire(&mut self) -> Result<()> {
        self.note("retire");
        Ok(())
    }

    fn rest(&mut self) -> Result<()> {
        self.note("rest");
        Ok(())
    }

    fn remove_helper(&mut self) {
        self.note("remove helper");
    }

    fn keep_helper(&mut self) {
        self.note("keep helper");
    }

    fn hint(&mut self) {
        self.note("hint");
    }

    fn caches(&mut self) {
        self.note("caches");
        remove_caches(&self.state, &mut Vec::new(), &silent).unwrap();
    }

    fn release(&mut self) {
        self.held = None;
    }
}

#[tokio::test]
async fn an_unrecorded_account_is_kept_while_atlas_and_its_caches_are_removed() {
    let dir = tempfile::tempdir().unwrap();
    let facts = Facts {
        other: true,
        on_screen: true,
        remove_helper: true,
        ..facts()
    };
    let steps = plan(&facts).unwrap();
    assert_eq!(
        steps,
        [
            Step::Settle,
            Step::Retire,
            Step::Rest,
            Step::Hint,
            Step::Caches
        ]
    );
    let mut script = Script::new(dir.path(), &[false]);
    let claim = script.controller.quarantine().unwrap();
    let generation = claim.generation().to_owned();
    drop(claim);
    execute(&steps, &facts, &mut script).await.unwrap();
    assert_eq!(script.steps, ["settle", "retire", "rest", "hint", "caches"]);
    assert_eq!(script.controller.pending().unwrap(), [generation]);
    assert!(script.controller.acquire(true).is_ok());
}

#[tokio::test]
async fn unsupported_or_vanished_input_still_reaches_verified_logout_and_teardown() {
    if !atlas_core::test_support::isolated(
        "uninstall::tests::unsupported_or_vanished_input_still_reaches_verified_logout_and_teardown",
    ) {
        return;
    }
    for outcome in [
        Err(atlas_core::error::Error::new(
            Code::Input,
            "Unknown native operation",
        )),
        Err(atlas_core::error::Error::new(
            Code::Io,
            "No such file or directory",
        )),
        Err(atlas_core::error::Error::new(Code::Uncertain, "Shutdown reply lost").uncertain()),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let uncertain = outcome.as_ref().is_err_and(|error| error.uncertain);
        let mut script = Script::new(dir.path(), &[false, true, true]);
        script.shutdown = Some(outcome);
        fs::create_dir_all(script.state.join("update")).unwrap();
        fs::write(script.state.join("update/scratch"), b"temporary").unwrap();
        execute(&plan(&facts()).unwrap(), &facts(), &mut script)
            .await
            .unwrap();
        assert_eq!(
            script.steps,
            [
                "settle",
                "look false",
                "log out",
                "look true",
                "retire",
                "rest",
                "look true",
                "hint",
                "caches"
            ]
        );
        assert_eq!(
            script.controller.pending().unwrap().len(),
            usize::from(uncertain)
        );
        assert!(!script.state.join("update").exists());
        assert_eq!(script.warnings.len(), 1);
        assert!(script.controller.acquire(true).is_ok());
    }
    let dir = tempfile::tempdir().unwrap();
    let mut script = Script::new(dir.path(), &[false, false]);
    script.shutdown = Some(Err(atlas_core::error::Error::new(
        Code::Input,
        "Unknown native operation",
    )));
    assert_eq!(
        execute(&plan(&facts()).unwrap(), &facts(), &mut script)
            .await
            .unwrap_err()
            .code,
        "logout"
    );
    assert_eq!(
        script.steps,
        ["settle", "look false", "log out", "look false"]
    );
}

#[tokio::test]
async fn a_login_that_starts_after_the_plan_is_logged_out_before_the_job_is_retired() {
    let dir = tempfile::tempdir().unwrap();
    let facts = Facts {
        logged_in: false,
        ..facts()
    };
    // Logged out when planned, logged in once control is taken.
    let mut script = Script::new(dir.path(), &[false, true, true]);
    execute(&plan(&facts).unwrap(), &facts, &mut script)
        .await
        .unwrap();
    assert_eq!(
        script.steps,
        [
            "settle",
            "look false",
            "log out",
            "look true",
            "retire",
            "rest",
            "look true",
            "hint",
            "caches"
        ]
    );
}

#[tokio::test]
async fn a_helper_still_logged_in_when_its_job_would_go_stops_everything() {
    let dir = tempfile::tempdir().unwrap();
    let mut script = Script::new(dir.path(), &[false, false]);
    let error = execute(&plan(&facts()).unwrap(), &facts(), &mut script)
        .await
        .unwrap_err();
    assert_eq!(error.code, "logout");
    assert_eq!(
        script.steps,
        ["settle", "look false", "log out", "look false"]
    );
}

#[tokio::test]
async fn a_helper_logged_in_again_before_its_deletion_keeps_its_account() {
    let dir = tempfile::tempdir().unwrap();
    let facts = Facts {
        remove_helper: true,
        ..facts()
    };
    let mut script = Script::new(dir.path(), &[true, true, false]);
    execute(&plan(&facts).unwrap(), &facts, &mut script)
        .await
        .unwrap();
    assert!(script.steps.contains(&"keep helper".to_owned()));
    assert!(!script.steps.contains(&"remove helper".to_owned()));
    assert_eq!(script.steps.last().map(String::as_str), Some("caches"));
}

#[tokio::test]
async fn no_other_controller_takes_the_helper_until_the_caches_are_gone() {
    if !atlas_core::test_support::isolated(
        "uninstall::tests::no_other_controller_takes_the_helper_until_the_caches_are_gone",
    ) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    for remove_helper in [false, true] {
        let facts = Facts {
            remove_helper,
            ..facts()
        };
        let mut script = Script::new(dir.path(), &[]);
        execute(&plan(&facts).unwrap(), &facts, &mut script)
            .await
            .unwrap();
        let free: Vec<&String> = script
            .steps
            .iter()
            .filter(|step| step.ends_with("(free)"))
            .collect();
        assert!(free.is_empty(), "{:?}", script.steps);
        assert_eq!(script.steps.last().map(String::as_str), Some("caches"));
        assert!(
            script.controller.acquire(true).is_ok(),
            "let go once every step has run"
        );
    }
}

/// Agents whose marketplace named atlas is someone else's are named.
#[test]
fn agents_left_alone_are_named_with_their_marketplace() {
    let reply = json!({"ok": true, "agents": [
        {"agent": "claude", "ok": true, "changed": true},
        {"agent": "codex", "ok": true, "changed": false, "kept": "someone/atlas"},
    ]});
    assert_eq!(
        kept_agents(&reply),
        vec![("Codex".to_owned(), "someone/atlas".to_owned())]
    );
    assert!(kept_agents(&json!({"ok": true, "agents": []})).is_empty());
}

/// Uninstall's own "off" is never kept as the person's choice; one
/// they made to turn it off is.
#[test]
fn only_a_choice_to_keep_it_off_survives_uninstall() {
    assert!(keeps_login_choice(true, Some(false)));
    assert!(!keeps_login_choice(true, Some(true)), "it was on");
    assert!(!keeps_login_choice(false, Some(false)), "never decided");
    assert!(!keeps_login_choice(false, None));
    assert!(
        !keeps_login_choice(true, None),
        "unknown: setup decides again"
    );
}
