use super::status::permissions_from;
use atlas_client::setup::executor::without;
#[test]
fn settings_are_absent_only_when_the_configuration_is_missing() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    for missing in ["missing.toml", "missing/settings.toml"] {
        assert!(settings(&dir.path().join(missing)).unwrap().is_none());
    }
    let valid = dir.path().join("valid.toml");
    fs::write(&valid, include_str!("../../../examples/config.toml")).unwrap();
    assert_eq!(
        settings(&valid).unwrap().unwrap(),
        Config::load(&valid).unwrap()
    );

    let malformed = dir.path().join("malformed.toml");
    fs::write(&malformed, "helper_uid = [").unwrap();
    let dangling = dir.path().join("dangling.toml");
    symlink(dir.path().join("missing.toml"), &dangling).unwrap();
    let looping = dir.path().join("loop");
    symlink(&looping, &looping).unwrap();
    assert_eq!(fs::read_link(&looping).unwrap(), looping);
    for broken in [malformed, dangling, looping.join("settings.toml")] {
        assert_eq!(settings(&broken).unwrap_err().code, "config-invalid");
    }
}

#[test]
fn choosing_the_configured_helper_again_bypasses_the_replacement_refusal() {
    let config = Config {
        helper_uid: 502,
        socket_path: "/unused/daemon.sock".into(),
        identity_probe: "/unused/atlas".into(),
        input_socket: "/unused/input.sock".into(),
        output_dir: "/unused/runs".into(),
        activity: Default::default(),
    };
    let keep = |_: u32, _: &Config| Err(problem("The configured helper is in use"));
    assert!(check_choice(501, Some(502), &config, keep).is_ok());
    assert!(check_choice(501, Some(503), &config, keep).is_err());
    assert!(check_choice(501, None, &config, keep).is_err());
}

#[test]
fn setup_without_orphans_excludes_teardown_and_keeps_its_executor_after_caller_death() {
    if !atlas_core::test_support::isolated(
        "onboarding::tests::setup_without_orphans_excludes_teardown_and_keeps_its_executor_after_caller_death",
    ) {
        return;
    }
    for mode in ["selected", "allocated"] {
        atlas_client::test_support::interrupted_executor(
            "onboarding::tests::an_account_setup_test_caller",
            mode,
        );
    }
}

#[test]
fn interactive_creation_keeps_its_worker_claim_after_the_caller_dies() {
    if !atlas_core::test_support::isolated(
        "onboarding::tests::interactive_creation_keeps_its_worker_claim_after_the_caller_dies",
    ) {
        return;
    }
    atlas_client::test_support::interrupted_executor(
        "onboarding::tests::an_account_setup_test_caller",
        "interactive",
    );
}

/// A subprocess entry point using only temporary state and a harmless executor.
#[test]
fn an_account_setup_test_caller() {
    let Some(directory) = atlas_core::test_support::directory() else {
        return;
    };
    if std::env::var("ATLAS_MUTATION_TEST_MODE").unwrap() == "interactive" {
        let controller = Controller::at(atlas_core::test_support::UID, &directory.join("state"));
        setup::executor::setup_transaction(&controller, || {
            // Ignore terminal hangup before exec so the worker survives
            // its caller. The worker publishes readiness itself, with
            // no external command still running when the caller dies.
            let executable = std::env::current_exe().unwrap();
            let args = [
                "-f",
                "-c",
                "trap '' HUP; exec \"$@\"",
                "zsh",
                executable.to_str().unwrap(),
                "--exact",
                "onboarding::tests::an_interactive_setup_test_worker",
                "--nocapture",
            ];
            answer_prompts("/bin/zsh", &args, b"unused", Duration::from_secs(30))
        })
        .unwrap();
        return;
    }
    let _ = closing_first_using(
        7001,
        &[],
        |uid| Ok(Controller::at(uid, &directory.join("state"))),
        || {
            atlas_client::test_support::pause_executor(&directory, WORKER);
            Ok(())
        },
        || true,
    );
}

/// A subprocess worker that stays alive until released, even without its terminal.
#[test]
fn an_interactive_setup_test_worker() {
    let Some(directory) = atlas_core::test_support::directory() else {
        return;
    };
    atlas_client::test_support::executor_ready(&directory, || {
        fs::write(directory.join("changed"), b"").unwrap();
    })
    .unwrap();
}

#[test]
fn setup_requires_its_own_acknowledgement_even_when_every_orphan_is_gone() {
    if !atlas_core::test_support::isolated(
        "onboarding::tests::setup_requires_its_own_acknowledgement_even_when_every_orphan_is_gone",
    ) {
        return;
    }
    for outcome in ["complete", "refused", "partial", "unknown"] {
        let directory = tempfile::tempdir().unwrap();
        let controller = Controller::at(7001, directory.path());
        let competing_teardown = controller.acquire(true).unwrap();
        let blocked = closing_first_using(
            7001,
            &[],
            |uid| Ok(Controller::at(uid, directory.path())),
            || panic!("setup cannot dispatch while teardown owns the helper"),
            || true,
        );
        assert!(matches!(blocked, Err(error) if error.code == Code::Busy));
        drop(competing_teardown);
        let directory = tempfile::tempdir().unwrap();
        let controller = Controller::at(7001, directory.path());
        let result = closing_first_using(
            7001,
            &[],
            |uid| Ok(Controller::at(uid, directory.path())),
            || {
                assert!(controller.acquire(true).is_err());
                assert_eq!(controller.pending().unwrap().len(), 1);
                if outcome == "complete" {
                    Ok(())
                } else {
                    Err(as_root::Unclosed {
                        refusal: Refusal::Failed("setup did not complete".into()),
                        exit: match outcome {
                            "refused" => Exit::WithoutEffects,
                            "unknown" => Exit::Unknown,
                            _ => Exit::AfterEffects,
                        },
                    })
                }
            },
            || true,
        )
        .unwrap();
        assert_eq!(result.is_ok(), outcome == "complete");
        assert_eq!(
            controller.pending().unwrap().len(),
            usize::from(matches!(outcome, "partial" | "unknown"))
        );
    }
}
#[test]
fn preparing_an_account_holds_every_orphan_and_records_interrupted_cleanup() {
    if !atlas_core::test_support::isolated(
        "onboarding::tests::preparing_an_account_holds_every_orphan_and_records_interrupted_cleanup",
    ) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let controllers = [
        Controller::at(7001, dir.path()),
        Controller::at(7002, dir.path()),
    ];
    for busy in &controllers {
        let other = busy.acquire(true).unwrap();
        let result = setup_batch(
            &controllers,
            || -> std::result::Result<(), as_root::Unclosed> {
                unreachable!("setup never runs while an orphan is controlled")
            },
            || true,
        );
        assert!(matches!(result, Err(error) if error.code == Code::Busy));
        drop(other);
    }
    let result = setup_batch(
        &controllers,
        || -> std::result::Result<(), as_root::Unclosed> {
            for controller in &controllers {
                assert!(controller.acquire(true).is_err());
                assert_eq!(controller.pending().unwrap().len(), 1);
            }
            Err(as_root::Unclosed {
                refusal: Refusal::Failed("cleanup was interrupted".into()),
                exit: Exit::AfterEffects,
            })
        },
        || false,
    );
    assert!(matches!(result, Ok(Err(Refusal::Failed(_)))));
    for controller in controllers {
        assert_eq!(controller.pending().unwrap().len(), 1);
    }
}

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
fn a_secret_is_one_short_plain_line() {
    assert_eq!(read_secret(&b"hunter2\n"[..]).unwrap(), b"hunter2");
    assert_eq!(read_secret(&b"hunter2"[..]).unwrap(), b"hunter2");
    assert_eq!(
        read_secret("pässwörd\n".as_bytes()).unwrap(),
        "pässwörd".as_bytes()
    );
    assert!(read_secret(&b"\n"[..]).is_err());
    assert!(read_secret(&b"two\nlines\n"[..]).is_err());
    assert!(read_secret(&b"carriage\rreturn"[..]).is_err());
    assert!(read_secret(&b"del\x7fete"[..]).is_err());
    assert!(read_secret(&[b'x'; 300][..]).is_err());
}

/// The secret is typed only once asked, reaches a terminal prompt
/// with echo off, and never appears in what is kept of the output.
#[test]
fn prompts_are_answered_through_a_terminal() {
    let script = "printf 'User password:'; stty -echo; read p; stty echo; echo; \
                      [ \"$p\" = 'se cret' ] && echo matched";
    let text = answer_prompts(
        "/bin/sh",
        &["-c", script],
        b"se cret",
        Duration::from_secs(10),
    )
    .unwrap();
    assert!(text.contains("matched"), "{text}");
    assert!(!text.contains("se cret"), "{text}");

    let wrong = "printf 'Password:'; read p; exit 3";
    let failed = answer_prompts("/bin/sh", &["-c", wrong], b"abc", Duration::from_secs(10));
    assert!(failed.is_err());
}

#[test]
fn the_secret_is_cut_out_byte_for_byte() {
    assert_eq!(without(b"a se cret b se cret", b"se cret"), b"a  b ");
    assert_eq!(without(b"sesecret", b"secret"), b"se");
    assert_eq!(without(b"\xff\xfepass", b"\xfe"), b"\xffpass");
    assert_eq!(without(b"kept", b""), b"kept");
}

fn pairing(controller: u32) -> Pairing {
    Pairing {
        ticket: "a".repeat(atlas_core::files::TOKEN_BYTES * 2),
        controller_uid: controller,
        controller_name: "person".into(),
        helper_uid: controller + 1,
        helper_name: "helper".into(),
        helper_identity: helper_setup::identity::Identity {
            uid: controller + 1,
            name: "helper".into(),
            generated_uid: "6DB4B7AF-BE3C-486C-BD46-1B4558946919".into(),
        },
    }
}

#[test]
fn a_request_is_trusted_only_from_its_controller() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
    let path = dir.path().join("onboarding.json");
    let me = nix::unistd::getuid().as_raw();

    write_request(&path, &pairing(me)).unwrap();
    assert_eq!(read_request(&path), Some(pairing(me)));

    // Without a ticket: ignored.
    let mut unticketed: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    unticketed.as_object_mut().unwrap().remove("ticket");
    atlas_core::files::write_atomic(&path, &serde_json::to_vec(&unticketed).unwrap(), 0o644)
        .unwrap();
    assert_eq!(read_request(&path), None);

    // Written by someone else than the controller it names: ignored.
    write_request(&path, &pairing(me + 7)).unwrap();
    assert_eq!(read_request(&path), None);

    // Writable by others: ignored.
    write_request(&path, &pairing(me)).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
    assert_eq!(read_request(&path), None);

    // In a folder others can change: ignored.
    write_request(&path, &pairing(me)).unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o777)).unwrap();
    assert_eq!(read_request(&path), None);
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();

    // A link to a request: ignored.
    let link = dir.path().join("link.json");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    assert_eq!(read_request(&link), None);
}

#[test]
fn shared_requests_and_helper_text_never_wait_for_a_pipe_writer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("onboarding.json");
    nix::unistd::mkfifo(&path, nix::sys::stat::Mode::S_IRUSR).unwrap();
    assert_eq!(read_request(&path), None);
    assert_eq!(read_text(&path, 64 * 1024), None);
    fs::remove_file(&path).unwrap();
    fs::write(&path, "a".repeat(65)).unwrap();
    assert_eq!(read_text(&path, 64), None);
    fs::write(&path, "a".repeat(64)).unwrap();
    assert_eq!(read_text(&path, 64), Some("a".repeat(64)));
}

/// A helper answers only while something listens at its socket; a
/// socket file left behind, or none, is silent.
#[test]
fn a_helper_answers_only_while_its_socket_listens() {
    if !atlas_core::test_support::isolated(
        "onboarding::tests::a_helper_answers_only_while_its_socket_listens",
    ) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("atlas.sock");
    let config = Config {
        helper_uid: 502,
        socket_path: socket.clone(),
        identity_probe: dir.path().join("atlas"),
        input_socket: dir.path().join("input.sock"),
        output_dir: dir.path().join("runs"),
        activity: Default::default(),
    };
    assert!(!answers(&config), "no socket");

    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    assert!(answers(&config));

    drop(listener);
    assert!(socket.exists());
    assert!(!answers(&config), "a socket nothing listens at");
}

/// A helper whose Atlas folder was prepared for this account may have
/// its job installed, so its settings are not replaced by another's.
#[test]
fn a_helper_prepared_for_this_account_is_recognised() {
    let dir = tempfile::tempdir().unwrap();
    let helper = account::Account {
        name: "helper".into(),
        uid: 502,
        home: dir.path().to_owned(),
    };
    assert!(!prepared(&helper), "nothing prepared");

    let layout = Layout::new(helper.uid, helper.home.clone());
    fs::create_dir_all(layout.root()).unwrap();
    assert!(!prepared(&helper), "only its root");

    fs::create_dir_all(layout.daemon()).unwrap();
    assert!(prepared(&helper));
}

/// Only a request this account made, for a helper that never answered,
/// was never prepared and has no setup recorded, may be taken back.
#[test]
fn only_an_unanswered_request_may_be_taken_back() {
    let mine = Asked {
        requested: true,
        ..Asked::default()
    };
    assert!(mine.may_forget());
    assert!(!Asked::default().may_forget(), "not this account's");
    let recorded = Asked {
        recorded: true,
        ..mine
    };
    assert!(!recorded.may_forget(), "a setup is recorded");
    let answered = Asked {
        answered: true,
        ..mine
    };
    assert!(!answered.may_forget(), "the helper answered");
    let prepared = Asked {
        prepared: true,
        ..mine
    };
    assert!(!prepared.may_forget(), "prepared for this account");
}

fn opie() -> Identity {
    Identity {
        uid: 503,
        name: "opie".into(),
        generated_uid: "6DB4B7AF-BE3C-486C-BD46-1B4558946919".into(),
    }
}

fn sessions(
    logins: &[(u32, u32, atlas_core::user::logins::Kind)],
) -> atlas_core::user::logins::Sessions {
    atlas_core::user::logins::Sessions {
        logins: logins
            .iter()
            .map(|(uid, pid, kind)| {
                let window = atlas_core::user::logins::Window {
                    uid: *uid,
                    pid: *pid,
                    age: Duration::from_secs(600),
                };
                (window, *kind)
            })
            .collect(),
        blockers: Vec::new(),
    }
}

#[test]
fn a_login_under_the_helpers_user_id_is_closed_only_as_the_recorded_helpers() {
    use atlas_core::user::logins::Kind;

    let found = sessions(&[
        (7001, 11, Kind::Orphan),
        (503, 22, Kind::HelperLeftover),
        (501, 33, Kind::Mine),
        (504, 44, Kind::OtherPerson),
    ]);
    assert_eq!(
        leftover_targets(&found, Some(&opie())),
        Ok(vec![
            Target::Orphan(7001),
            Target::Leftover {
                helper: opie(),
                pid: 22
            },
        ])
    );
    assert!(leftover_targets(&found, None).is_err(), "nothing recorded");
    let other = Identity { uid: 505, ..opie() };
    assert!(leftover_targets(&found, Some(&other)).is_err());
}

#[test]
fn every_leftover_login_closes_under_one_approval() {
    if !atlas_core::test_support::isolated(
        "onboarding::tests::every_leftover_login_closes_under_one_approval",
    ) {
        return;
    }
    let targets = [
        Target::Orphan(7001),
        Target::Leftover {
            helper: opie(),
            pid: 22,
        },
        Target::Orphan(7003),
    ];
    let dir = tempfile::tempdir().unwrap();
    let held = controller(dir.path());
    for controllers in [&[][..], std::slice::from_ref(&held)] {
        let asked = std::cell::Cell::new(0);
        let closed = close_batch(
            controllers,
            &targets,
            |all| {
                asked.set(asked.get() + 1);
                assert_eq!(all, targets, "every target in the one run");
                Ok(all.iter().map(Target::uid).collect())
            },
            |uids| uids == [7001, 503, 7003],
        );
        assert_eq!(closed.unwrap(), [7001, 503, 7003]);
        assert_eq!(asked.get(), 1, "one approval");
    }
    assert!(held.pending().unwrap().is_empty());

    let none = close_batch(
        std::slice::from_ref(&held),
        &[],
        |_| unreachable!("nothing to close"),
        |_| true,
    );
    assert_eq!(none.unwrap(), Vec::<u32>::new());

    let still = close_batch(
        &[],
        &targets,
        |all| Ok(all.iter().map(Target::uid).collect()),
        |_| false,
    );
    assert!(
        matches!(still, Err(Held::Refused(Refusal::Failed(why))) if why == "it is still running"),
        "seen gone, not taken on root's word"
    );
}

#[test]
fn closing_leftover_logins_holds_the_helper_and_keeps_an_unknown_outcome() {
    if !atlas_core::test_support::isolated(
        "onboarding::tests::closing_leftover_logins_holds_the_helper_and_keeps_an_unknown_outcome",
    ) {
        return;
    }
    let targets = [Target::Orphan(7001), Target::Orphan(503)];
    let dir = tempfile::tempdir().unwrap();
    let held = controller(dir.path());

    // Another controller holds the helper: root is never asked.
    let other = held.acquire(true).unwrap();
    let busy = close_batch(
        std::slice::from_ref(&held),
        &targets,
        |_| unreachable!("root is not asked"),
        |_| true,
    );
    assert!(matches!(busy, Err(Held::State(error)) if error.code == Code::Busy));
    drop(other);

    // Held throughout, and recorded before root is asked.
    let interrupted = close_batch(
        std::slice::from_ref(&held),
        &targets,
        |_| {
            assert!(controller(dir.path()).acquire(true).is_err(), "taken");
            assert_eq!(controller(dir.path()).pending().unwrap().len(), 1);
            Err(as_root::Unclosed {
                refusal: Refusal::Failed("macOS didn't close the login of user ID 503".into()),
                exit: Exit::AfterEffects,
            })
        },
        |_| false,
    );
    assert!(interrupted.is_err());
    assert_eq!(held.pending().unwrap().len(), 1, "kept for recovery");
}

#[test]
fn creating_without_the_approval_call_waits_while_a_deleted_accounts_login_is_open() {
    let error = without_approval(&[7001], || -> Result<()> {
        unreachable!("no account is made")
    })
    .unwrap_err();
    assert!(
        error
            .error
            .contains("sudo launchctl bootout user/7001, then try again"),
        "{}",
        error.error
    );
    assert_eq!(without_approval(&[], || Ok(7)).unwrap(), 7);
}

/// A controller kept in `dir`, and the generations it has pending.
fn controller(dir: &Path) -> Controller {
    Controller::at(60001, &dir.join("state"))
}

fn untouched(refusal: Refusal) -> as_root::Unclosed {
    as_root::Unclosed {
        refusal,
        exit: Exit::WithoutEffects,
    }
}

#[test]
fn the_helper_is_logged_out_as_root_only_under_control_held_throughout() {
    if !atlas_core::test_support::isolated(
        "onboarding::tests::the_helper_is_logged_out_as_root_only_under_control_held_throughout",
    ) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let held = controller(dir.path());

    // Another controller holds the helper: root is never asked.
    let other = held.acquire(true).unwrap();
    let busy = close_helper_held(
        &held,
        || Ok(opie()),
        |_| unreachable!("root is not asked"),
        || true,
    );
    assert!(matches!(busy, Err(Held::State(error)) if error.code == Code::Busy));
    drop(other);

    // Held while root runs and while the login is looked at.
    let ended = (0..50)
        .find_map(|_| {
            let result = close_helper_held(
                &held,
                || Ok(opie()),
                |helper| {
                    assert!(controller(dir.path()).acquire(true).is_err(), "taken");
                    assert_eq!(helper, opie());
                    Ok(vec![503])
                },
                || {
                    assert!(controller(dir.path()).acquire(true).is_err(), "taken");
                    true
                },
            );
            match result {
                Err(Held::State(error)) if error.code == Code::Busy => {
                    std::thread::sleep(Duration::from_millis(20));
                    None
                }
                result => Some(result),
            }
        })
        .expect("a child that inherited the old lock finishes its exec");
    assert!(ended.is_ok());
    assert!(held.pending().unwrap().is_empty(), "settled once seen gone");
    assert!(held.acquire(false).is_ok(), "let go after");
}

#[test]
fn a_log_out_as_root_stays_pending_until_the_login_is_seen_gone() {
    if !atlas_core::test_support::isolated(
        "onboarding::tests::a_log_out_as_root_stays_pending_until_the_login_is_seen_gone",
    ) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let held = controller(dir.path());

    // Recorded before root is asked, so a run cut short leaves it.
    let ownership = held.acquire(true).unwrap();
    let lost = end_as_root(
        &held,
        &ownership,
        || {
            assert_eq!(controller(dir.path()).pending().unwrap().len(), 1);
            Err(as_root::Unclosed {
                refusal: Refusal::Failed(
                    "Atlas stopped waiting for the approved step, which may still finish".into(),
                ),
                exit: Exit::Unknown,
            })
        },
        || false,
    );
    assert!(matches!(lost, Err(Held::Refused(Refusal::Failed(_)))));
    assert_eq!(
        held.pending().unwrap().len(),
        1,
        "a lost reply stays pending"
    );

    // Root said it closed it, but it is still seen: pending too.
    let stayed = end_as_root(&held, &ownership, || Ok(vec![503]), || false);
    assert!(stayed.is_err());
    assert_eq!(held.pending().unwrap().len(), 2);
    drop(ownership);
    assert_eq!(
        held.acquire(false).unwrap_err().code,
        "uncertain",
        "recovery comes first"
    );
}

#[test]
fn a_root_run_whose_reply_was_lost_stays_pending_though_the_login_looks_gone() {
    if !atlas_core::test_support::isolated(
        "onboarding::tests::a_root_run_whose_reply_was_lost_stays_pending_though_the_login_looks_gone",
    ) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let held = controller(dir.path());
    let ownership = held.acquire(true).unwrap();

    // The desktop is gone for now, but root may still be waiting to act.
    let lost = end_as_root(
        &held,
        &ownership,
        || {
            Err(as_root::Unclosed {
                refusal: Refusal::Failed("The step did not finish".into()),
                exit: Exit::Unknown,
            })
        },
        || unreachable!("how the login looks proves nothing yet"),
    );
    assert!(matches!(lost, Err(Held::Refused(_))));
    assert_eq!(held.pending().unwrap().len(), 1);
}

#[test]
fn a_log_out_root_never_started_leaves_nothing_pending() {
    if !atlas_core::test_support::isolated(
        "onboarding::tests::a_log_out_root_never_started_leaves_nothing_pending",
    ) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let held = controller(dir.path());
    let ownership = held.acquire(true).unwrap();

    for refusal in [
        Refusal::Cancelled,
        Refusal::Unavailable,
        Refusal::Failed("the helper is on screen; switch back to your own account first".into()),
    ] {
        let ended = end_as_root(
            &held,
            &ownership,
            || Err(untouched(refusal.clone())),
            || false,
        );
        assert!(matches!(ended, Err(Held::Refused(said)) if said == refusal));
    }
    assert!(held.pending().unwrap().is_empty());

    // Root stopped after it acted, but the login is gone: settled.
    let late = end_as_root(
        &held,
        &ownership,
        || {
            Err(as_root::Unclosed {
                refusal: Refusal::Failed("macOS didn't end the login of user ID 503".into()),
                exit: Exit::AfterEffects,
            })
        },
        || true,
    );
    assert!(late.is_ok());
    assert!(held.pending().unwrap().is_empty());
}

/// The test that stands in for an executor in this test binary.
const WORKER: &str = "onboarding::tests::an_executor_test_worker";

#[test]
fn an_executor_test_worker() {
    atlas_client::test_support::executor_worker();
}

#[test]
fn a_surviving_executor_keeps_recovery_excluded_until_its_observed_exit() {
    atlas_client::test_support::interrupted_executor(
        "onboarding::tests::an_account_setup_test_caller",
        "interactive",
    );
}
