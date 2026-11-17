use super::*;
use serde_json::json;

fn login(uid: u32) -> Desktop {
    Desktop::from(crate::user::User { uid })
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
fn only_a_vm_is_proved_over_its_session_and_only_a_second_login_by_its_own_status() {
    use crate::session::{MachineExpect, key::PeerKey, paired::Paired};
    let vm = Desktop::from(crate::vm::Vm {
        id: DesktopId::parse("0123456789abcdef01234567").unwrap(),
    });
    let station = PeerKey::from_bytes([2; 32]);
    let paired = Paired {
        station: station.fingerprint(),
        guest: PeerKey::from_bytes([1; 32]),
        uid: 501,
        boot: "boot-0".to_owned(),
        change: crate::files::Token::new(),
    };
    let expect = MachineExpect {
        paired: &paired,
        peer: paired.guest,
        station,
        eligible: true,
        hands_off: Some(true),
    };
    let another = Observation::decode(&json!({"ok": true, "user": {"uid": 502}}));

    let machine = vm.prove_machine(&another, &expect).unwrap_err();
    let expected = another.require_machine(&expect).unwrap_err();
    assert_eq!(
        (machine.code, machine.error),
        (expected.code, expected.error)
    );
    assert!(
        vm.prove(&another).is_err(),
        "a VM's own status proves nothing"
    );
    let user = login(502).prove_machine(&another, &expect).unwrap_err();
    assert_eq!(user.code, crate::error::Code::Platform);
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

#[test]
fn a_desktop_name_is_read_back_as_written_and_nothing_else_is_one() {
    let written = Desktop::from(crate::user::User { uid: 502 }).id();

    assert_eq!(DesktopId::parse(written.as_str()), Some(written.clone()));
    for not_one in [
        "",
        "user",
        "ABCDEF0123456789abcdef01",
        "0123456789abcdef012345678",
    ] {
        assert_eq!(DesktopId::parse(not_one), None, "{not_one}");
    }
}

fn machine() -> Desktop {
    Desktop::from(crate::vm::Vm {
        id: DesktopId::parse("0123456789abcdef01234567").unwrap(),
    })
}

#[test]
fn a_vm_desktop_is_named_by_its_own_identifier() {
    assert_eq!(machine().id().as_str(), "0123456789abcdef01234567");
    assert_eq!(
        machine().record_key().as_str(),
        "vm-0123456789abcdef01234567"
    );
    assert_ne!(machine().record_key(), RecordKey::user());
}

#[test]
fn a_vms_record_key_names_it_and_no_other_key_names_a_vm() {
    assert_eq!(machine().record_key().vm_id(), Some(machine().id()));
    assert_eq!(RecordKey::user().vm_id(), None);
    assert_eq!(RecordKey::parse("vm-0123").unwrap().vm_id(), None);
}

#[test]
fn every_answer_of_a_vm_desktop_holds_work_back() {
    let desktop = machine();
    assert_eq!(desktop.user(), None);
    let observation = Observation::decode(&json!({
        "ok": true,
        "user": {"uid": 502},
        "session": {"onConsole": false},
    }));
    assert_eq!(
        desktop.prove(&observation).unwrap_err().code,
        crate::error::Code::Platform
    );
    assert!(!desktop.input_current(&observation, 1));
    assert!(
        desktop.left_may_run(&[], None),
        "its guest's processes are not seen here"
    );
    assert!(desktop.orphans_run().unwrap());
    assert_eq!(desktop.instance(std::process::id()), None);
    let mine = login(502).instance(std::process::id()).unwrap();
    assert!(!desktop.exited(mine), "never seen to end from this Mac");
}
