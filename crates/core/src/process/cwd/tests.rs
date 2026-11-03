use super::*;

#[test]
fn a_hosts_working_directory_is_read_only_for_the_same_process() {
    let instance = Instance::current().unwrap();
    assert_eq!(cwd(instance), std::env::current_dir().ok());
    assert_eq!(cwd(Instance::new(instance.pid, [1, 0])), None);
}

#[test]
fn a_partial_or_unterminated_working_directory_is_unknown() {
    let instance = Instance::current().unwrap();
    for bytes in [
        None,
        Some(b"/unterminated".to_vec()),
        Some(b"relative\0".to_vec()),
    ] {
        assert_eq!(read(instance, own, |_| bytes), None);
    }
}

#[test]
fn a_reused_or_foreign_pid_has_no_working_directory() {
    let instance = Instance::current().unwrap();
    assert_eq!(
        read(instance, |_| Own::Unknown, |_| panic!("must not read")),
        None
    );
    let calls = std::cell::Cell::new(0);
    assert_eq!(
        read(
            instance,
            |pid| {
                calls.set(calls.get() + 1);
                if calls.get() == 1 {
                    own(pid)
                } else {
                    Own::Gone
                }
            },
            |_| Some(b"/tmp\0".to_vec())
        ),
        None
    );
}
