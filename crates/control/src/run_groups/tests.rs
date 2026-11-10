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
    let controller = Controller::at(nix::unistd::getuid().as_raw(), directory.path());
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
    let controller = Controller::at(nix::unistd::getuid().as_raw(), directory.path());
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
    let controller = Controller::at(nix::unistd::getuid().as_raw(), directory.path());
    fs::create_dir_all(directory.path()).unwrap();
    fs::write(&controller.run_groups, b"[not groups").unwrap();
    let refused = controller
        .require_no_run_left(nix::unistd::getuid().as_raw())
        .unwrap_err();
    assert_eq!(refused.code, "state");
}

#[test]
fn past_the_cap_no_running_group_is_let_go_and_the_record_refuses_until_none_is_left() {
    let directory = tempfile::tempdir().unwrap();
    let helper = nix::unistd::getuid().as_raw();
    let controller = Controller::at(helper, directory.path());
    let groups: Vec<i32> = (0..=KEPT).map(|_| handed_on().0).collect();
    for group in &groups {
        controller.note_run_group(*group, helper).unwrap();
    }
    let record: Record = read(&controller.run_groups).unwrap().unwrap();
    assert_eq!(record.groups, groups[..KEPT]);
    assert!(record.overflow);

    // The groups it kept end; the one past the cap still runs.
    for group in &groups[..KEPT] {
        end(*group);
    }
    let refused = controller.require_no_run_left(helper).unwrap_err();
    assert_eq!(refused.details["running"], "command");
    let record: Record = read(&controller.run_groups).unwrap().unwrap();
    assert!(record.groups.is_empty());
    assert!(
        record.overflow,
        "only a full look that finds none left clears it"
    );
    end(groups[KEPT]);
}

#[test]
fn a_full_look_finds_work_whose_group_leader_is_gone() {
    let process = |pid, pgid| atlas_core::process::Info {
        instance: atlas_core::process::Instance::new(pid, [1, 0]),
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
fn a_command_whose_work_could_not_be_recorded_stays_unknown_until_it_is() {
    let directory = tempfile::tempdir().unwrap();
    let helper = nix::unistd::getuid().as_raw();
    let controller = Controller::at(helper, directory.path());
    let (group, _) = handed_on();
    let claim = controller.quarantine_command().unwrap();
    // The record can't be written: a folder stands where it goes.
    fs::create_dir_all(&controller.run_groups).unwrap();
    assert!(controller.note_run_group(group, helper).is_err());

    controller.keep_unrecorded_run(claim, group);

    let refused = controller.acquire(false).unwrap_err();
    assert_eq!(
        refused.code, "uncertain",
        "the command's outcome is unknown"
    );

    // Once the record can be written, the next look records the work and
    // settles the command; the work still keeps the person out.
    fs::remove_dir(&controller.run_groups).unwrap();
    drop(controller.acquire(false).unwrap());
    assert!(controller.pending().unwrap().is_empty());
    let refused = controller.require_no_run_left(helper).unwrap_err();
    assert_eq!(refused.details["running"], "command");
    end(group);
    controller.require_no_run_left(helper).unwrap();
}
