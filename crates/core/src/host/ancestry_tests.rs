use super::*;
use crate::process::{Instance, State};

#[test]
fn this_process_is_described_with_its_parent_and_start() {
    let pid = i32::try_from(std::process::id()).unwrap();
    let info = process(pid).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    assert_eq!(info.instance.pid, pid);
    assert_eq!(info.ppid, nix::unistd::getppid().as_raw());
    let start = info.instance.start;
    assert!(start[0] > 0 && start[0] <= now, "{info:?}");
    assert!(start[1] < 1_000_000);
    assert!(!info.name.is_empty());
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

    let mut child =
        crate::spawn::spawn(std::process::Command::new("/bin/sleep").arg("30")).unwrap();
    let pid = i32::try_from(child.id()).unwrap();
    let process = super::process(pid).unwrap().instance;
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
    assert_eq!(walked[0].instance.pid, nix::unistd::getppid().as_raw());
    for (index, ancestor) in walked.iter().enumerate() {
        assert!(
            !walked[index + 1..]
                .iter()
                .any(|later| later.instance.pid == ancestor.instance.pid),
            "{walked:?}"
        );
    }
    if let Some(launchd) = walked
        .iter()
        .position(|ancestor| ancestor.instance.pid == 1)
    {
        assert_eq!(launchd, walked.len() - 1, "nothing after launchd");
    }

    let looping = |pid: i32| {
        let uid = nix::unistd::geteuid().as_raw();
        Some(Info {
            instance: Instance::new(pid, [1, 0]),
            ppid: if pid == 3 { 2 } else { 3 },
            pgid: pid,
            uid,
            ruid: uid,
            svuid: uid,
            zombie: false,
            stopped: false,
            name: "sh".to_owned(),
        })
    };
    let walked = ancestors_from(2, 64, looping);
    assert_eq!(
        walked
            .iter()
            .map(|ancestor| ancestor.instance.pid)
            .collect::<Vec<_>>(),
        [2, 3]
    );
    assert_eq!(ancestors_from(2, 1, looping).len(), 1, "at most the limit");
}
