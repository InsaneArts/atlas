//! A bounded ancestry walk with each process's account and exact start.
use crate::process::{Instance, Own, Start, own};

/// What macOS says of one described process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Facts {
    pub parent: i32,
    pub uid: u32,
    /// Its command name, at most 16 bytes as the kernel keeps it.
    pub command: String,
    pub start: Start,
}

/// One ancestor of this process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Ancestor {
    pub pid: i32,
    pub uid: u32,
    pub instance: Instance,
    pub parent: i32,
    pub command: String,
}

/// The facts of `pid`, when it is a live process of this account.
pub(crate) fn process(pid: i32) -> Option<Facts> {
    match own(pid) {
        Own::Found(info) if info.uid == nix::unistd::geteuid().as_raw() => Some(Facts {
            parent: info.ppid,
            uid: info.uid,
            command: info.name,
            start: info.instance.start,
        }),
        Own::Found(_) | Own::Gone | Own::Unknown => None,
    }
}

/// This process's ancestors, parent first, at most `limit`, ending at
/// launchd or at the first that cannot be read.
pub(crate) fn ancestors(limit: usize) -> Vec<Ancestor> {
    ancestors_from(nix::unistd::getppid().as_raw(), limit, process)
}

/// [`ancestors`] from `first`, described by `describe`.
pub(crate) fn ancestors_from(
    first: i32,
    limit: usize,
    describe: impl Fn(i32) -> Option<Facts>,
) -> Vec<Ancestor> {
    let mut found: Vec<Ancestor> = Vec::new();
    let mut pid = first;

    while found.len() < limit && pid > 0 && !found.iter().any(|seen| seen.pid == pid) {
        let Some(facts) = describe(pid) else {
            break;
        };
        found.push(Ancestor {
            pid,
            uid: facts.uid,
            instance: Instance::new(pid, facts.start),
            parent: facts.parent,
            command: facts.command,
        });
        if pid == 1 {
            break;
        }
        pid = facts.parent;
    }

    found
}

#[cfg(test)]
mod tests {
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
}
