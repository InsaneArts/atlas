use super::*;
use crate::{
    activity_log,
    client::tests::{
        GRACE, SOON, another, answer, command, dispatched, native_client,
        recovery_settles_the_abandoned_claim, run_of, running, waits,
    },
    doing::Doing,
    test_support::client_in,
};
use atlas_core::host::Key;
use atlas_core::{actions, error::Holder};
use nix::sys::signal::Signal;
use std::fs;
use tokio::io::AsyncBufReadExt;

fn run() -> Mutation<'static> {
    Mutation::Run {
        argv: vec!["true".to_owned()],
        timeout: Duration::from_secs(1),
        pin: None,
    }
}

fn epoch() -> InputEpoch {
    InputEpoch {
        uid: 502,
        security_session_id: 7,
        display_id: 3,
        width: 1512.0,
        height: 982.0,
        process_id: 41,
    }
}

#[test]
fn signal_timeout_and_exit_are_distinct_completed_outcomes() {
    for reply in [
        json!({"ok":true,"exitCode":0,"timedOut":false,"signal":null}),
        json!({"ok":true,"exitCode":null,"timedOut":false,"signal":"SIGTERM"}),
        json!({"ok":true,"exitCode":null,"timedOut":true,"signal":"SIGKILL"}),
    ] {
        run().validate(&reply).unwrap();
    }

    for reply in [
        json!({"ok":true}),
        json!({"ok":true,"exitCode":null,"timedOut":false,"signal":15}),
    ] {
        assert!(run().validate(&reply).unwrap_err().uncertain);
    }

    let actions = json!([{"type": "key", "key": "a"}]);
    assert!(
        Mutation::Input {
            actions: Batch::decode(&actions).unwrap(),
            owner: "owner",
            epoch: epoch(),
            picture: None,
            app: false,
            expect: None,
            asking: None,
        }
        .validate(&json!({"ok":true,"performed":0}))
        .unwrap_err()
        .uncertain
    );
}

/// A reply to an action on an element proves it only by saying how it went:
/// one without a verification may come from a service that did not act.
#[test]
fn an_element_action_is_confirmed_only_with_its_verification() {
    let actions = json!([{"type": "check", "ref": "8123abc.12"}]);
    let input = || Mutation::Input {
        actions: Batch::decode(&actions).unwrap(),
        owner: "owner",
        epoch: epoch(),
        picture: None,
        app: false,
        expect: None,
        asking: None,
    };
    input()
        .validate(&json!({"ok": true, "performed": 1, "mechanism": "accessibility", "verification": "matched"}))
        .unwrap();
    // An element already as asked was left as it was: nothing was sent.
    input()
        .validate(&json!({"ok": true, "performed": 0, "mechanism": "accessibility", "verification": "matched"}))
        .unwrap();
    for reply in [
        json!({"ok": true, "performed": 0, "mechanism": "accessibility", "verification": "mismatch"}),
        json!({"ok": true, "performed": 0, "mechanism": "accessibility", "verification": "not-checked"}),
        json!({"ok": true, "performed": 1}),
        json!({"ok": true, "performed": 1, "mechanism": "accessibility"}),
        json!({"ok": true, "performed": 1, "mechanism": "accessibility", "verification": "done"}),
        json!({"ok": true, "performed": 1, "mechanism": "events", "verification": "matched"}),
    ] {
        assert!(input().validate(&reply).unwrap_err().uncertain, "{reply}");
    }
}

/// The client's own transport reads this daemon's replies.
#[tokio::test]
async fn the_client_transport_understands_the_daemon() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("d.sock");
    atlas_daemon::test_support::serve(tokio::net::UnixListener::bind(&socket).unwrap());

    let transport = atlas_core::transport::Transport {
        path: socket,
        timeout: std::time::Duration::from_secs(10),
        peer_uid: nix::unistd::getuid().as_raw(),
    };

    let reply = transport
        .call(&json!({"op": "run", "argv": ["/bin/sh", "-c", "printf ok; exit 4"], "timeoutMs": 3000}))
        .await
        .unwrap();
    assert_eq!(
        atlas_core::transport::run_output(&reply, 64).unwrap(),
        b"ok"
    );
    assert_eq!(
        (&reply["exitCode"], &reply["timedOut"]),
        (&json!(4), &json!(false))
    );
    run().validate(&reply).unwrap();

    let hello = transport.call(&json!({"op": "hello"})).await.unwrap();
    assert_eq!(hello["daemon"]["pid"], std::process::id());
}

/// Every byte other Atlas processes read, independent of how `request` builds it.
#[test]
fn each_mutation_sends_its_exact_request() {
    let actions = json!([{"type": "key", "key": "a"}]);
    let expect = Expect::new(812).unwrap();
    let id = JobId::parse("0123456789ab").unwrap();
    let expected = Expected {
        boot: "boot".to_owned(),
        pid: 9,
        start: [5, 6],
        pgid: 9,
    };
    let pin = SessionPin {
        security_session_id: 7,
        display_id: 3,
    };
    let start = |name: Option<&str>, task: bool| JobStart {
        argv: vec!["/bin/sleep".to_owned(), "9".to_owned()],
        lifetime: Duration::from_secs(2),
        name: name.map(str::to_owned),
        task: task.then(|| Key::parse("aaaaaaaaaaaa").unwrap()),
        session: task.then(|| Key::parse("bbbbbbbbbbbb").unwrap()),
    };
    let input = |expect| Mutation::Input {
        actions: Batch::decode(&actions).unwrap(),
        owner: "owner",
        epoch: epoch(),
        picture: None,
        app: true,
        expect,
        asking: None,
    };
    let epoch_wire = r#"{"displayId":3,"height":982.0,"processId":41,"securitySessionId":7,"uid":502,"width":1512.0}"#;
    let cases = [
        (
            input(None),
            format!(
                r#"{{"actions":[{{"key":"a","type":"key"}}],"app":true,"epoch":{epoch_wire},"op":"input","owner":"owner"}}"#
            ),
        ),
        (
            input(Some(&expect)),
            format!(
                r#"{{"actions":[{{"key":"a","type":"key"}}],"app":true,"epoch":{epoch_wire},"expect":{{"pid":812}},"op":"input","owner":"owner"}}"#
            ),
        ),
        (
            Mutation::Run {
                argv: vec!["true".to_owned()],
                timeout: Duration::from_millis(1500),
                pin: None,
            },
            r#"{"argv":["true"],"op":"run","timeoutMs":1500}"#.to_owned(),
        ),
        (
            Mutation::Run {
                argv: vec!["true".to_owned()],
                timeout: Duration::from_secs(1),
                pin: Some(pin),
            },
            r#"{"argv":["true"],"op":"run","session":{"displayId":3,"securitySessionId":7},"timeoutMs":1000}"#.to_owned(),
        ),
        (
            Mutation::JobStart {
                start: start(None, false),
                uid: 502,
                pin,
            },
            r#"{"action":"start","argv":["/bin/sleep","9"],"lifetimeMs":2000,"op":"job","session":{"displayId":3,"securitySessionId":7},"uid":502}"#.to_owned(),
        ),
        (
            Mutation::JobStart {
                start: start(Some("name"), true),
                uid: 502,
                pin,
            },
            r#"{"action":"start","argv":["/bin/sleep","9"],"hostSession":"bbbbbbbbbbbb","lifetimeMs":2000,"name":"name","op":"job","session":{"displayId":3,"securitySessionId":7},"task":"aaaaaaaaaaaa","uid":502}"#.to_owned(),
        ),
        (
            Mutation::JobStop {
                id: &id,
                expected: &expected,
                grace: Duration::from_secs(5),
            },
            r#"{"action":"stop","expect":{"boot":"boot","pgid":9,"pid":9,"start":[5,6]},"graceMs":5000,"id":"0123456789ab","op":"job"}"#.to_owned(),
        ),
        (Mutation::Restart, r#"{"op":"restart"}"#.to_owned()),
        (
            Mutation::Logout { force: true },
            r#"{"force":true,"op":"logout"}"#.to_owned(),
        ),
    ];
    for (mutation, wire) in cases {
        assert_eq!(mutation.request().to_string(), wire);
    }
}

#[test]
fn a_checked_input_is_confirmed_only_with_its_expectation_echoed() {
    let app = Expect::new(812).unwrap();
    let checked = Some(&app);
    let actions = json!([{"type": "key", "key": "a"}, {"type": "key", "key": "b"}]);
    let epoch = InputEpoch {
        uid: 502,
        security_session_id: 42,
        display_id: 7,
        width: 100.0,
        height: 100.0,
        process_id: 456,
    };

    for (expect, reply, confirmed) in [
        (None, json!({"ok": true, "performed": 2}), true),
        (None, json!({"ok": true, "performed": 1}), false),
        (
            checked,
            json!({"ok": true, "performed": 2, "expect": {"pid": 812}}),
            true,
        ),
        (checked, json!({"ok": true, "performed": 2}), false),
        (
            checked,
            json!({"ok": true, "performed": 2, "expect": {"pid": 9}}),
            false,
        ),
        (checked, json!({"ok": false, "code": "focus-changed"}), true),
    ] {
        let outcome = Mutation::Input {
            actions: actions::Batch::decode(&actions).unwrap(),
            owner: "owner",
            epoch: epoch.clone(),
            picture: None,
            app: false,
            expect,
            asking: None,
        }
        .validate(&reply);
        assert_eq!(outcome.is_ok(), confirmed, "{expect:?} {reply}");
        if let Err(error) = outcome {
            assert_eq!(error.code, "protocol");
            assert!(error.uncertain);
        }
    }
}

#[tokio::test]
async fn a_run_that_ends_within_the_grace_is_settled() {
    let (dir, client, input, _session) = native_client();
    drop(input);
    let (acting, helper, interrupt) = running(dir.path(), client);
    let _ownership = acting.controller.acquire(false).unwrap();
    let run = || run_of(&["true"], 1000);
    let mut caller = Box::pin(acting.mutate(run(), None, None, None));
    let mut peer = dispatched(&mut caller, &helper, "run").await;
    tokio::time::pause();
    interrupt.ask_for_test(Signal::SIGTERM);
    tokio::time::advance(SOON).await;

    answer(
        &mut peer,
        json!({"ok": true, "exitCode": 0, "timedOut": false}),
    )
    .await;
    assert_eq!(caller.await.unwrap()["exitCode"], 0);
    assert!(acting.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn a_run_still_going_after_the_grace_is_cut_and_stays_pending() {
    if !atlas_core::test_support::isolated(
        "client::mutate::tests::a_run_still_going_after_the_grace_is_cut_and_stays_pending",
    ) {
        return;
    }
    let (dir, client, input, _session) = native_client();
    drop(input);
    let (acting, helper, interrupt) = running(dir.path(), client);
    let ownership = acting.controller.acquire(false).unwrap();
    let run = || run_of(&["sleep", "30"], 60_000);
    let mut caller = Box::pin(acting.mutate(run(), None, None, None));
    let mut peer = dispatched(&mut caller, &helper, "run").await;
    interrupt.ask_for_test(Signal::SIGTERM);

    let error = tokio::time::timeout(GRACE * 5, caller)
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, "interrupted");
    assert!(error.uncertain);
    let mut line = String::new();
    assert_eq!(
        peer.read_line(&mut line).await.unwrap(),
        0,
        "the helper sees the connection close, which ends the command"
    );
    let log = fs::read_to_string(acting.output_dir().unwrap().join(activity_log::FILE)).unwrap();
    let ended: Value = serde_json::from_str(log.lines().last().unwrap()).unwrap();
    assert_eq!(ended["ok"], false);
    assert_eq!(ended["code"], "uncertain", "never recorded as not done");
    drop(ownership);
    recovery_settles_the_abandoned_claim(&another(&acting, dir.path()));
}

#[tokio::test]
async fn malformed_uncertainty_after_a_sent_mutation_keeps_its_original_claim() {
    let id = JobId::new();
    let expected = Expected {
        boot: "boot".to_owned(),
        pid: 2,
        start: [1, 0],
        pgid: 2,
    };
    let pin = SessionPin {
        security_session_id: 1,
        display_id: 1,
    };
    for flag in atlas_core::test_support::invalid_uncertainty_flags() {
        for (mutation, op) in [
            (run_of(&["true"], 1000), "run"),
            (
                Mutation::JobStart {
                    start: JobStart {
                        argv: vec!["true".to_owned()],
                        lifetime: Duration::from_secs(1),
                        name: None,
                        task: None,
                        session: None,
                    },
                    uid: 0,
                    pin,
                },
                "job",
            ),
            (
                Mutation::JobStop {
                    id: &id,
                    expected: &expected,
                    grace: Duration::from_secs(1),
                },
                "job",
            ),
        ] {
            let is_run = matches!(mutation, Mutation::Run { .. });
            let (dir, client, input, _session) = native_client();
            drop(input);
            let (acting, helper, _interrupt) = running(dir.path(), client);
            let ownership = acting.controller.acquire_beside_commands().unwrap();
            let (held, release) = if is_run {
                (None, Some(ownership))
            } else {
                (Some(ownership), None)
            };
            let mut caller = Box::pin(acting.mutate(mutation, None, None, release));
            let mut peer = dispatched(&mut caller, &helper, op).await;
            let original = acting.controller.pending().unwrap();
            assert_eq!(original.len(), 1);
            answer(
                &mut peer,
                json!({"ok": false, "code": "job", "error": "unknown", "uncertain": flag}),
            )
            .await;
            let result = caller.await;
            assert_eq!(acting.controller.pending().unwrap(), original);
            let error = result.unwrap_err();
            assert_eq!(error.code, Code::Protocol);
            assert!(error.uncertain);
            if is_run {
                let log = fs::read_to_string(acting.output_dir().unwrap().join(activity_log::FILE))
                    .unwrap();
                let ended: Value = serde_json::from_str(log.lines().last().unwrap()).unwrap();
                assert_eq!(ended["code"], "uncertain");
            }
            drop(held);
            recovery_settles_the_abandoned_claim(&acting);
        }
    }
}

#[tokio::test]
async fn a_run_lets_go_of_control_once_its_request_is_sent() {
    if !atlas_core::test_support::isolated(
        "client::mutate::tests::a_run_lets_go_of_control_once_its_request_is_sent",
    ) {
        return;
    }
    let (dir, client, input, _session) = native_client();
    drop(input);
    let (acting, helper, _interrupt) = running(dir.path(), client);
    let other = another(&acting, dir.path());
    let ownership = acting.controller.acquire_beside_commands().unwrap();
    let mut caller = Box::pin(acting.mutate(command(), None, None, Some(ownership)));
    let mut peer = dispatched(&mut caller, &helper, "run").await;
    waits(&mut caller).await;

    drop(other.controller.acquire_beside_commands().unwrap());
    drop(other.controller.acquire_for_person_for_test().unwrap());
    let refused = other.controller.acquire(true).unwrap_err();
    assert_eq!(refused.code, "busy");
    assert_eq!(refused.holder, Some(Holder::Agent));
    assert_eq!(other.controller.acquire(false).unwrap_err().code, "busy");
    assert!(other.controller.pending_causes().unwrap()[0].command);

    answer(
        &mut peer,
        json!({"ok": true, "exitCode": 0, "timedOut": false}),
    )
    .await;
    assert_eq!(caller.await.unwrap()["exitCode"], 0);
    assert!(other.controller.pending().unwrap().is_empty());
    assert!(other.controller.acquire(false).is_ok());
}

#[tokio::test]
async fn a_lost_run_reply_blocks_every_agent() {
    if !atlas_core::test_support::isolated(
        "client::mutate::tests::a_lost_run_reply_blocks_every_agent",
    ) {
        return;
    }
    let (dir, client, input, _session) = native_client();
    drop(input);
    let (acting, helper, _interrupt) = running(dir.path(), client);
    let other = another(&acting, dir.path());
    let ownership = acting.controller.acquire_beside_commands().unwrap();
    let mut caller = Box::pin(acting.mutate(command(), None, None, Some(ownership)));
    let peer = dispatched(&mut caller, &helper, "run").await;
    waits(&mut caller).await;

    drop(peer);
    let error = caller.await.unwrap_err();
    assert!(error.uncertain, "{error:?}");
    for refused in [
        other.controller.acquire_beside_commands(),
        other.controller.acquire_for_person_for_test(),
        other.controller.acquire(false),
    ] {
        assert_eq!(refused.unwrap_err().code, "uncertain");
    }
    recovery_settles_the_abandoned_claim(&other);
}

#[tokio::test]
async fn a_run_that_let_go_and_is_cut_after_the_grace_stays_pending_for_everyone() {
    if !atlas_core::test_support::isolated(
        "client::mutate::tests::a_run_that_let_go_and_is_cut_after_the_grace_stays_pending_for_everyone",
    ) {
        return;
    }
    let (dir, client, input, _session) = native_client();
    drop(input);
    let (acting, helper, interrupt) = running(dir.path(), client);
    let other = another(&acting, dir.path());
    let ownership = acting.controller.acquire_beside_commands().unwrap();
    let mut caller = Box::pin(acting.mutate(command(), None, None, Some(ownership)));
    let mut peer = dispatched(&mut caller, &helper, "run").await;
    waits(&mut caller).await;
    interrupt.ask_for_test(Signal::SIGTERM);

    let error = tokio::time::timeout(GRACE * 5, caller)
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, "interrupted");
    assert!(error.uncertain);
    let mut line = String::new();
    assert_eq!(
        peer.read_line(&mut line).await.unwrap(),
        0,
        "the helper sees the connection close, which ends the command"
    );
    for refused in [
        other.controller.acquire_beside_commands(),
        other.controller.acquire_for_person_for_test(),
    ] {
        assert_eq!(refused.unwrap_err().code, "uncertain");
    }
    recovery_settles_the_abandoned_claim(&other);
}

#[tokio::test]
async fn two_runs_go_on_beside_each_other() {
    if !atlas_core::test_support::isolated(
        "client::mutate::tests::two_runs_go_on_beside_each_other",
    ) {
        return;
    }
    let (dir, client, input, _session) = native_client();
    drop(input);
    let (acting, helper, _interrupt) = running(dir.path(), client);
    let second = another(&acting, dir.path());
    let other = another(&acting, dir.path());

    let first_control = acting.controller.acquire_beside_commands().unwrap();
    let mut first = Box::pin(acting.mutate(command(), None, None, Some(first_control)));
    let mut first_peer = dispatched(&mut first, &helper, "run").await;
    waits(&mut first).await;
    let second_control = second.controller.acquire_beside_commands().unwrap();
    let mut beside = Box::pin(second.mutate(command(), None, None, Some(second_control)));
    let mut second_peer = dispatched(&mut beside, &helper, "run").await;
    waits(&mut beside).await;

    let causes = other.controller.pending_causes().unwrap();
    assert_eq!(causes.len(), 2);
    assert!(causes.iter().all(|cause| cause.command));
    drop(other.controller.acquire_beside_commands().unwrap());
    assert_eq!(other.controller.acquire(true).unwrap_err().code, "busy");

    let ended = json!({"ok": true, "exitCode": 0, "timedOut": false});
    answer(&mut second_peer, ended.clone()).await;
    beside.await.unwrap();
    assert_eq!(other.controller.pending().unwrap().len(), 1);
    answer(&mut first_peer, ended).await;
    first.await.unwrap();
    assert!(other.controller.pending().unwrap().is_empty());
    assert!(other.controller.acquire(false).is_ok());
}

#[tokio::test]
async fn other_commands_keep_control_for_their_whole_run() {
    let (dir, client, input, _session) = native_client();
    drop(input);
    let (acting, helper, _interrupt) = running(dir.path(), client);
    let other = another(&acting, dir.path());
    let ownership = acting.controller.acquire_beside_commands().unwrap();
    let mut caller = Box::pin(acting.mutate(command(), None, None, None));
    let mut peer = dispatched(&mut caller, &helper, "run").await;
    waits(&mut caller).await;

    assert_eq!(
        other.controller.acquire_beside_commands().unwrap_err().code,
        "busy",
        "the caller keeps control"
    );
    assert!(!other.controller.pending_causes().unwrap()[0].command);

    answer(
        &mut peer,
        json!({"ok": true, "exitCode": 0, "timedOut": false}),
    )
    .await;
    caller.await.unwrap();
    drop(ownership);
    assert!(other.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn a_stopped_agents_command_is_refused_before_it_starts_and_a_read_is_not() {
    let dir = tempfile::tempdir().unwrap();
    let client = client_in(dir.path());
    std::fs::write(client.config.input_socket.with_extension("stopped"), b"").unwrap();
    let run = || run_of(&["true"], 1000);

    let acting = client.clone().doing(Doing::new("run"));
    let refused = acting.mutate(run(), None, None, None).await.unwrap_err();
    assert_eq!(refused.code, "stopped");
    assert!(!refused.uncertain, "nothing started");

    let reading = client.mutate(run(), None, None, None).await.unwrap_err();
    assert_ne!(
        reading.code, "stopped",
        "a read goes on (and finds no helper here)"
    );
}

#[test]
fn a_check_before_typing_is_confirmed_only_as_judged_with_nothing_performed() {
    let actions = json!([{"type": "type", "text": "x"}]);
    let checking = Mutation::Input {
        actions: Batch::decode(&actions).unwrap(),
        owner: "owner",
        epoch: epoch(),
        picture: None,
        app: false,
        expect: None,
        asking: Some(json!({"accounts": [], "dry": true})),
    };
    checking
        .validate(&json!({"ok": true, "performed": 0, "dry": true}))
        .unwrap();
    assert!(
        checking
            .validate(&json!({"ok": true, "performed": 1}))
            .is_err(),
        "typed instead of checked"
    );
    assert!(
        checking
            .validate(&json!({"ok": true, "performed": 0}))
            .is_err(),
        "never said it only checked"
    );
}

#[test]
fn only_a_refusal_proven_to_have_sent_nothing_drops_its_picture() {
    let refusal = |extra: Value| {
        let mut reply = json!({"ok": false, "code": "waiting-for-person"});
        for (key, value) in extra.as_object().unwrap() {
            reply[key] = value.clone();
        }
        reply
    };
    assert!(gated(&refusal(json!({}))), "refused before its first event");
    assert!(gated(&json!({"ok": false, "code": "denied"})));
    assert!(gated(&json!({"ok": true, "performed": 0, "dry": true})));
    assert!(gated(&refusal(
        json!({"performed": 0, "partial": false, "uncertain": false})
    )));
    assert!(
        !gated(&refusal(
            json!({"performed": 0, "partial": true, "uncertain": true})
        )),
        "stopped by the check before its second typed character"
    );
    assert!(
        !gated(&refusal(
            json!({"performed": 1, "partial": false, "uncertain": false})
        )),
        "stopped after one action ran"
    );
    assert!(!gated(&refusal(json!({"uncertain": true}))));
}

#[tokio::test]
async fn a_finished_run_that_left_work_running_keeps_its_group_until_it_ends() {
    use std::os::unix::process::CommandExt;
    // `atlas run sh -c '(sleep 30; open …) &'`: the shell is gone once it
    // answers, and the work it handed on runs in its group.
    let mut shell = atlas_core::spawn::spawn(
        std::process::Command::new("/bin/sh")
            .args([
                "-c",
                "(/bin/sleep 30; /usr/bin/open https://example.com) >/dev/null 2>&1 &",
            ])
            .process_group(0),
    )
    .unwrap();
    let group = i32::try_from(shell.id()).unwrap();
    shell.wait().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client_in(dir.path());
    let me = nix::unistd::getuid().as_raw();
    client.config.helper_uid = me;
    let reply = json!({"ok": true, "exitCode": 0, "timedOut": false,
        "events": [{"event": "started", "pid": group}]});
    assert_eq!(started_group(&reply), Some(group));

    client
        .settle(&run().request(), reply, true, None, Instant::now())
        .await
        .unwrap();

    let refused = client.controller.require_no_run_left(me).unwrap_err();
    assert_eq!(refused.details["running"], "command");
    let _ = nix::sys::signal::killpg(
        nix::unistd::Pid::from_raw(group),
        nix::sys::signal::Signal::SIGKILL,
    );
    for _ in 0..200 {
        if client.controller.require_no_run_left(me).is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the group ended, and still refuses");
}

#[tokio::test]
async fn a_run_whose_leftover_work_cant_be_noted_keeps_the_person_out_until_it_is() {
    use std::os::unix::process::CommandExt;
    let mut shell = atlas_core::spawn::spawn(
        std::process::Command::new("/bin/sh")
            .args([
                "-c",
                "(/bin/sleep 30; /usr/bin/open https://example.com) >/dev/null 2>&1 &",
            ])
            .process_group(0),
    )
    .unwrap();
    let group = i32::try_from(shell.id()).unwrap();
    shell.wait().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client_in(dir.path());
    let me = nix::unistd::getuid().as_raw();
    client.config.helper_uid = me;
    client.controller = atlas_control::Controller::at(me, &dir.path().join("state"));
    let claim = client.controller.quarantine_command().unwrap();
    // The record of leftover work can't be written: a folder stands there.
    std::fs::create_dir_all(client.controller.run_groups_path()).unwrap();
    let reply = json!({"ok": true, "exitCode": 0, "timedOut": false,
        "events": [{"event": "started", "pid": group}]});

    let settled = client
        .settle(&run().request(), reply, true, Some(claim), Instant::now())
        .await
        .unwrap();

    assert!(
        settled["warnings"]
            .to_string()
            .contains("could not be noted")
    );
    let mut lease = atlas_control::Lease::default();
    let refused = lease.acquire(&client.controller).unwrap_err();
    assert_eq!(
        refused.code,
        Code::Uncertain,
        "the person can't sign in beside it"
    );

    std::fs::remove_dir(client.controller.run_groups_path()).unwrap();
    let token = lease.acquire(&client.controller).unwrap();
    assert_eq!(
        client
            .controller
            .require_no_run_left(me)
            .unwrap_err()
            .details["running"],
        "command",
        "noted at last, the work still keeps a sign-in out"
    );
    drop(token);
    lease.release();
    let _ = nix::sys::signal::killpg(
        nix::unistd::Pid::from_raw(group),
        nix::sys::signal::Signal::SIGKILL,
    );
}

#[tokio::test]
async fn an_environment_runs_leftover_work_keeps_the_person_from_signing_in() {
    use std::os::unix::process::CommandExt;
    // `atlas environment run -- sh -c '(sleep 30; open …) &'`: its worker
    // execs the command, a run under control the environment holds rather
    // than an agent's own, and the work the shell handed on runs in its
    // group once the shell is gone.
    let mut shell = atlas_core::spawn::spawn(
        std::process::Command::new("/bin/sh")
            .args([
                "-c",
                "(/bin/sleep 30; /usr/bin/open https://example.com) >/dev/null 2>&1 &",
            ])
            .process_group(0),
    )
    .unwrap();
    let group = i32::try_from(shell.id()).unwrap();
    shell.wait().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client_in(dir.path());
    let me = nix::unistd::getuid().as_raw();
    client.config.helper_uid = me;
    client.controller = atlas_control::Controller::at(me, &dir.path().join("state"));
    let claim = client.controller.quarantine().unwrap();
    let reply = json!({"ok": true, "exitCode": 0, "timedOut": false,
        "events": [{"event": "started", "pid": group}]});

    client
        .settle(&run().request(), reply, false, Some(claim), Instant::now())
        .await
        .unwrap();

    assert!(client.controller.pending().unwrap().is_empty());
    let refused = client.controller.require_no_run_left(me).unwrap_err();
    assert_eq!(refused.details["running"], "command");

    // When it can't be noted, the environment's claim stays unknown.
    let claim = client.controller.quarantine().unwrap();
    std::fs::remove_file(client.controller.run_groups_path()).unwrap();
    std::fs::create_dir_all(client.controller.run_groups_path()).unwrap();
    let reply = json!({"ok": true, "exitCode": 0, "timedOut": false,
        "events": [{"event": "started", "pid": group}]});
    client
        .settle(&run().request(), reply, false, Some(claim), Instant::now())
        .await
        .unwrap();
    let mut lease = atlas_control::Lease::default();
    assert_eq!(
        lease.acquire(&client.controller).unwrap_err().code,
        Code::Uncertain
    );
    std::fs::remove_dir(client.controller.run_groups_path()).unwrap();
    let _ = nix::sys::signal::killpg(
        nix::unistd::Pid::from_raw(group),
        nix::sys::signal::Signal::SIGKILL,
    );
}

#[test]
fn an_agents_input_reply_never_carries_the_account_it_acted_in() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client_in(dir.path());
    let doing = crate::doing::Doing::new("type");
    client.doing = Some(doing.clone());
    let account = json!({"name": "slack.com", "key": {"kind": "site", "id": "slack.com"}});
    let mut reply = json!({"ok": true, "performed": 1, "account": account, "allowed": "task",
        "element": {"role": "AXButton"}});

    client.keep_from_reply(&mut reply);

    assert_eq!(reply, json!({"ok": true, "performed": 1}));
    assert_eq!(
        doing.noted_account(),
        Some((account.clone(), Some(json!("task"))))
    );
    // A later batch allowed once says so; a weaker one never lowers it.
    doing.note_account(account.clone(), Some(json!("once")));
    doing.note_account(account.clone(), Some(json!("trusted")));
    assert_eq!(doing.noted_account(), Some((account, Some(json!("once")))));
}

#[test]
fn a_command_across_accounts_keeps_each_accounts_own_allowance() {
    let doing = crate::doing::Doing::new("type");
    let slack = json!({"name": "slack.com", "key": {"kind": "site", "id": "slack.com"}});
    let github = json!({"name": "github.com", "key": {"kind": "site", "id": "github.com"}});
    doing.note_account(slack.clone(), None);
    doing.note_account(github.clone(), Some(json!("trusted")));
    assert_eq!(
        doing.noted_account(),
        Some((github.clone(), Some(json!("trusted")))),
        "never slack.com as Trusted"
    );
    doing.note_account(slack.clone(), Some(json!("trusted")));
    assert_eq!(
        doing.noted_account().unwrap().0,
        github,
        "a tie keeps the first"
    );
    doing.note_account(slack.clone(), Some(json!("task")));
    assert_eq!(doing.noted_account(), Some((slack, Some(json!("task")))));
}

#[test]
fn a_refused_inputs_row_still_names_the_account_of_what_ran_and_its_reply_never_does() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client_in(dir.path());
    let doing = crate::doing::Doing::new("key");
    client.doing = Some(doing.clone());
    let account = json!({"name": "slack.com", "key": {"kind": "site", "id": "slack.com"}});
    // A send allowed for the task ran; the action after it was refused.
    let mut reply = json!({"ok": false, "code": "denied", "performed": 1,
        "account": account, "allowed": "task"});
    let row = client.row(
        &json!({"op": "input", "actions": [{"type": "key"}, {"type": "type"}]}),
        &reply,
        Duration::ZERO,
        crate::activity_log::Step::End,
    );
    assert_eq!(row.ok, Some(false));
    assert_eq!(row.account.unwrap().name, "slack.com");
    assert_eq!(row.allowed, Some(crate::activity_log::Allowed::Task));

    client.keep_from_reply(&mut reply);
    assert!(reply.get("account").is_none() && reply.get("allowed").is_none());
    assert_eq!(reply["code"], "denied");
}
