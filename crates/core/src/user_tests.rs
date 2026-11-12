use super::*;

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
