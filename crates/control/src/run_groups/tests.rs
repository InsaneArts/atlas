use super::*;
use std::os::unix::process::CommandExt;

/// A shell in a group of its own that hands `sleep` to the background and
/// exits, as `atlas run sh -c '(sleep 30; open …) &'` does, and the
/// background work's PID.
fn handed_on() -> (i32, i32) {
    let mut shell = atlas_core::spawn::spawn(
        std::process::Command::new("/bin/sh")
            .args([
                "-c",
                "(/bin/sleep 30; /usr/bin/true) >/dev/null 2>&1 & echo $!",
            ])
            .stdout(std::process::Stdio::piped())
            .process_group(0),
    )
    .unwrap();
    let group = i32::try_from(shell.id()).unwrap();
    let mut said = String::new();
    std::io::Read::read_to_string(shell.stdout.as_mut().unwrap(), &mut said).unwrap();
    shell.wait().unwrap();
    (group, said.trim().parse().unwrap())
}

fn end(pid: i32) {
    let _ = nix::sys::signal::killpg(
        nix::unistd::Pid::from_raw(pid),
        nix::sys::signal::Signal::SIGKILL,
    );
    for _ in 0..200 {
        if atlas_core::process::group(pid)
            .unwrap()
            .iter()
            .all(|process| process.zombie)
        {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn a_group_a_finished_command_left_running_refuses_until_it_ends() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let helper = nix::unistd::getuid().as_raw();
    controller.require_no_run_left(helper).unwrap();
    let (group, _) = handed_on();

    controller.note_run_group(group, helper).unwrap();

    let refused = controller.require_no_run_left(helper).unwrap_err();
    assert_eq!(refused.code, "busy");
    assert_eq!(refused.holder, Some(atlas_core::error::Holder::Agent));
    assert_eq!(refused.details["running"], "command");

    end(group);
    controller.require_no_run_left(helper).unwrap();
    assert!(
        !controller.run_groups.exists(),
        "an ended group is forgotten"
    );
}

#[test]
fn a_group_with_nothing_left_is_never_kept() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    let helper = nix::unistd::getuid().as_raw();
    let mut done =
        atlas_core::spawn::spawn(std::process::Command::new("/usr/bin/true").process_group(0))
            .unwrap();
    let group = i32::try_from(done.id()).unwrap();
    done.wait().unwrap();

    controller.note_run_group(group, helper).unwrap();

    assert!(!controller.run_groups.exists());
    controller.require_no_run_left(helper).unwrap();
}

#[test]
fn a_record_that_cant_be_read_refuses() {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(505, directory.path());
    fs::create_dir_all(directory.path()).unwrap();
    fs::write(&controller.run_groups, b"[not groups").unwrap();
    let refused = controller
        .require_no_run_left(nix::unistd::getuid().as_raw())
        .unwrap_err();
    assert_eq!(refused.code, "state");
}
