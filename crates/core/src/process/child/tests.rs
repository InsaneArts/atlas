use super::*;

#[test]
fn observing_a_childs_exit_keeps_its_status_and_pid_reserved_until_wait() {
    let mut command = std::process::Command::new("/bin/sh");
    command.args(["-c", "exit 7"]);
    let mut child = crate::spawn::spawn(&mut command).unwrap();
    let pid = child.id();
    let due = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !exited(pid).unwrap() {
        assert!(
            std::time::Instant::now() < due,
            "the isolated child never exited"
        );
        std::thread::yield_now();
    }
    assert!(
        exited(pid).unwrap(),
        "observing does not consume the status"
    );
    assert!(
        crate::process::info(i32::try_from(pid).unwrap())
            .unwrap()
            .unwrap()
            .zombie
    );
    assert_eq!(child.wait().unwrap().code(), Some(7));
    assert_eq!(exited(pid).unwrap_err().raw_os_error(), Some(libc::ECHILD));
}

#[test]
fn an_invalid_pid_never_observes_an_arbitrary_child() {
    for pid in [0, u32::MAX] {
        assert_eq!(exited(pid).unwrap_err().kind(), io::ErrorKind::InvalidInput);
    }
}
