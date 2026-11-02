use super::*;
use crate::process::{Instance, State};

#[test]
fn this_process_is_described_with_its_parent_and_start() {
    let pid = i32::try_from(std::process::id()).unwrap();
    let facts = process(pid).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    assert_eq!(facts.parent, nix::unistd::getppid().as_raw());
    assert!(facts.start[0] > 0 && facts.start[0] <= now, "{facts:?}");
    assert!(facts.start[1] < 1_000_000);
    assert!(!facts.command.is_empty());
    let this = Instance::current().unwrap();
    assert!(this.running() && this.is_this_one());
    assert!(
        !Instance::new(pid, [1, 0]).running(),
        "another start is another process"
    );
}

#[test]
fn a_suspended_process_is_seen_as_stopped_until_continued() {
    use nix::sys::signal::{Signal, kill};

    let mut child = std::process::Command::new("/bin/sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let pid = i32::try_from(child.id()).unwrap();
    let process = Instance::new(pid, super::process(pid).unwrap().start);
    let seen = |want: State| {
        let started = std::time::Instant::now();
        while process.state() != want {
            assert!(
                started.elapsed() < std::time::Duration::from_secs(10),
                "{want:?}"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    };

    kill(nix::unistd::Pid::from_raw(pid), Signal::SIGSTOP).unwrap();
    seen(State::Stopped);
    assert!(process.running(), "a stopped process still runs");
    kill(nix::unistd::Pid::from_raw(pid), Signal::SIGCONT).unwrap();
    seen(State::Running);
    child.kill().unwrap();
    child.wait().unwrap();
    assert_eq!(process.state(), State::Gone);
}

#[test]
fn a_process_that_does_not_exist_is_none() {
    assert_eq!(process(i32::MAX), None);
    assert_eq!(process(-1), None);
}

#[test]
fn the_walk_ends_at_launchd_and_never_repeats() {
    let walked = ancestors(64);
    assert!(!walked.is_empty());
    assert_eq!(walked[0].pid, nix::unistd::getppid().as_raw());
    for (index, ancestor) in walked.iter().enumerate() {
        assert!(
            !walked[index + 1..]
                .iter()
                .any(|later| later.pid == ancestor.pid),
            "{walked:?}"
        );
    }
    if let Some(launchd) = walked.iter().position(|ancestor| ancestor.pid == 1) {
        assert_eq!(launchd, walked.len() - 1, "nothing after launchd");
    }

    let looping = |pid: i32| {
        Some(Facts {
            uid: nix::unistd::geteuid().as_raw(),
            parent: if pid == 3 { 2 } else { 3 },
            command: "sh".to_owned(),
            start: [1, 0],
        })
    };
    let walked = ancestors_from(2, 64, looping);
    assert_eq!(
        walked
            .iter()
            .map(|ancestor| ancestor.pid)
            .collect::<Vec<_>>(),
        [2, 3]
    );
    assert_eq!(ancestors_from(2, 1, looping).len(), 1, "at most the limit");
}
