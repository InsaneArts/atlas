use super::*;
use std::os::unix::process::CommandExt;

#[test]
fn this_process_reads_as_the_kernel_reports_it() {
    let me = info(std::process::id() as i32).unwrap().unwrap();
    assert_eq!(me.instance.pid, std::process::id() as i32);
    assert_eq!(me.ppid, nix::unistd::getppid().as_raw());
    assert_eq!(me.pgid, nix::unistd::getpgrp().as_raw());
    assert_eq!(me.uid, nix::unistd::geteuid().as_raw());
    assert_eq!(me.ruid, nix::unistd::getuid().as_raw());
    assert_eq!(me.svuid, nix::unistd::geteuid().as_raw());
    assert!(me.owned_by(me.ruid));
    assert!(!me.zombie);
    let executable = std::env::current_exe().unwrap();
    let file = executable.file_name().unwrap().to_string_lossy();
    assert!(file.starts_with(&me.name), "{file} {}", me.name);
}

#[test]
fn a_process_is_known_by_its_start_time_not_its_pid_alone() {
    let mut child = std::process::Command::new("/bin/sleep")
        .arg("5")
        .spawn()
        .unwrap();
    let pid = child.id() as i32;
    let known = info(pid).unwrap().unwrap();
    assert_eq!(known.instance.start, start(pid).unwrap());
    let other = Instance {
        pid,
        start: [known.instance.start[0], known.instance.start[1] ^ 1],
    };
    assert_ne!(known.instance, other);
    child.kill().unwrap();
    child.wait().unwrap();
}

#[test]
fn an_unreaped_child_is_read_as_a_zombie_with_its_start_time() {
    let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
    let pid = child.id() as i32;
    let started = start(pid).unwrap();
    let mut zombie = None;
    for _ in 0..200 {
        zombie = info(pid).unwrap().filter(|info| info.zombie);
        if zombie.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let zombie = zombie.expect("the child exited");
    assert_eq!(zombie.instance.start, started);
    child.wait().unwrap();
    assert_eq!(info(pid).unwrap(), None);
}

#[test]
fn another_accounts_process_is_read_and_a_reaped_one_is_absent() {
    let launchd = info(1).unwrap().unwrap();
    assert_eq!((launchd.uid, launchd.ruid), (0, 0));
    assert!(!launchd.owned_by(nix::unistd::getuid().as_raw()));

    let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
    let pid = child.id() as i32;
    child.wait().unwrap();
    assert_eq!(info(pid).unwrap(), None);
    assert!(start(pid).is_err());
}

#[test]
fn a_process_group_lists_only_its_members() {
    let mut leader = std::process::Command::new("/bin/sh")
        .args(["-c", "sleep 5 & sleep 5 & wait"])
        .process_group(0)
        .spawn()
        .unwrap();
    let pgid = leader.id() as i32;
    let mut members = Vec::new();
    for _ in 0..200 {
        members = group(pgid).unwrap();
        if members.len() == 3 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(members.len(), 3, "{members:?}");
    assert!(members.iter().all(|member| member.pgid == pgid));
    assert!(
        of_user(nix::unistd::geteuid().as_raw())
            .unwrap()
            .iter()
            .filter(|info| info.pgid == pgid)
            .count()
            == 3
    );
    nix::sys::signal::killpg(
        nix::unistd::Pid::from_raw(pgid),
        nix::sys::signal::Signal::SIGKILL,
    )
    .unwrap();
    leader.wait().unwrap();
}

#[test]
fn a_session_is_read_for_a_live_process() {
    assert_eq!(
        session(std::process::id() as i32),
        nix::unistd::getsid(None).ok().map(nix::unistd::Pid::as_raw)
    );
}

#[test]
fn the_boot_is_a_uuid() {
    let boot = boot().unwrap();
    assert_eq!(boot.len(), 36);
    assert_eq!(boot.matches('-').count(), 4);
}

#[test]
fn only_this_accounts_live_processes_are_described() {
    let this = Instance::current().unwrap();
    assert!(this.is_this_one());
    assert_eq!(this.state(), State::Running);
    assert_eq!(
        Instance::new(this.pid, [this.start[0], this.start[1] ^ 1]).state(),
        State::Gone,
        "another start is another process"
    );

    if nix::unistd::geteuid().is_root() {
        return;
    }
    let launchd = info(1).unwrap().unwrap();
    assert_eq!(
        launchd.instance.state(),
        State::Unknown,
        "another account's"
    );
    assert_eq!(
        Instance::new(0, [1, 0]).state(),
        State::Unknown,
        "the kernel's"
    );

    let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
    let pid = child.id() as i32;
    let exited = Instance::new(pid, start(pid).unwrap());
    let mut zombie = false;
    for _ in 0..200 {
        zombie = info(pid).unwrap().is_some_and(|info| info.zombie);
        if zombie {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(zombie, "the child exited");
    assert_eq!(
        exited.state(),
        State::Gone,
        "an unreaped process runs no more"
    );
    child.wait().unwrap();
    assert_eq!(exited.state(), State::Gone);
}

#[test]
fn a_suspended_process_reads_as_stopped() {
    use nix::sys::signal::{Signal, kill};

    let mut child = std::process::Command::new("/bin/sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let pid = child.id() as i32;
    kill(nix::unistd::Pid::from_raw(pid), Signal::SIGSTOP).unwrap();
    let started = std::time::Instant::now();
    while !info(pid).unwrap().unwrap().stopped {
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    child.kill().unwrap();
    child.wait().unwrap();
}
