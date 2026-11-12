use super::*;
use serde_json::json;

fn login(uid: u32) -> Desktop {
    Desktop::from(User { uid })
}

#[test]
fn a_user_desktop_keeps_the_record_name_installs_already_use() {
    assert_eq!(login(502).id().as_str(), "c6a7efd7472129144451d113");
    assert_eq!(login(503).id().as_str(), "ae2892390aeb6c07f55a0eea");
}

#[test]
fn a_desktop_is_named_by_its_account_alone() {
    assert_eq!(login(502).id(), login(502).id());
    assert_ne!(login(502).id(), login(503).id());
}

#[test]
fn a_user_desktop_is_proved_by_the_hidden_session_check() {
    for value in [
        json!({"ok": false}),
        json!({"ok": true, "user": {"uid": 503}}),
        json!({"ok": true, "user": {"uid": 502}, "session": {"onConsole": true}}),
    ] {
        let observation = Observation::decode(&value);
        let refused = login(502).prove(&observation).unwrap_err();
        let expected = observation.require_hidden(502).unwrap_err();
        assert_eq!(refused.code, expected.code, "{value}");
        assert_eq!(refused.error, expected.error, "{value}");
        assert!(!login(502).input_current(&observation, 1), "{value}");
    }
}

#[test]
fn a_full_look_finds_work_whose_group_leader_is_gone() {
    let process = |pid, pgid| Info {
        instance: process::Instance::new(pid, [1, 0]),
        ppid: 1,
        pgid,
        uid: 501,
        ruid: 501,
        svuid: 501,
        zombie: false,
        stopped: false,
        name: "sleep".to_owned(),
    };
    assert!(!orphaned(&[process(10, 10), process(11, 10)]));
    assert!(orphaned(&[process(10, 10), process(12, 99)]));
    assert!(!orphaned(&[]));
}

#[test]
fn a_process_has_exited_only_once_its_instance_is_gone() {
    let desktop = login(nix::unistd::getuid().as_raw());
    let mut child =
        crate::test_support::TestChild::spawn(&mut std::process::Command::new("/bin/cat"));
    let instance = desktop
        .instance(child.process.id())
        .expect("a running process can be looked at");
    assert!(!desktop.exited(instance), "it still runs");

    drop(child.process.stdin.take());
    assert!(child.wait().success());
    assert!(desktop.exited(instance), "it ended and was reaped");
    // A PID that names another process later is not the one that ran.
    let reused = crate::process::Instance::new(i32::try_from(std::process::id()).unwrap(), [1, 0]);
    assert!(desktop.exited(reused));
}

#[test]
fn a_user_desktop_is_asked_about_its_own_account_alone() {
    use std::os::unix::process::CommandExt;

    let uid = nix::unistd::getuid().as_raw();
    // A process that waits on its input, alone in a group of its own.
    let mut child = crate::test_support::TestChild::spawn(
        std::process::Command::new("/bin/cat").process_group(0),
    );
    let group = i32::try_from(child.process.id()).unwrap();

    assert!(
        login(uid).left_may_run(&[], Some(group)),
        "its own account's process runs there"
    );
    assert!(
        !login(uid ^ 1).left_may_run(&[], Some(group)),
        "another account has no process in that group"
    );
    assert!(!login(uid).left_may_run(&[], None), "nothing was left");

    drop(child.process.stdin.take());
    assert!(child.wait().success());
    assert!(!login(uid).left_may_run(&[], Some(group)), "it ended");
}
