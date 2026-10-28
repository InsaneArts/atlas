//! What macOS says of this account's processes: a process's parent, command
//! name and exact start, and this process's ancestors.
use serde::{Deserialize, Serialize};

/// A process and the exact time it started, so a reused pid is never it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Process {
    pub pid: i32,
    /// Seconds and microseconds since 1970.
    pub start: [u64; 2],
}

impl Process {
    pub fn new(pid: i32, start: [u64; 2]) -> Self {
        Self { pid, start }
    }

    /// This process, when macOS describes it.
    pub fn current() -> Option<Self> {
        let pid = i32::try_from(std::process::id()).ok()?;
        process(pid).map(|facts| Self::new(pid, facts.start))
    }

    /// How this very process is now.
    pub fn state(&self) -> State {
        match super::sys::process(self.pid) {
            Lookup::Found(facts) if facts.start != self.start => State::Gone,
            Lookup::Found(facts) if facts.stopped => State::Stopped,
            Lookup::Found(_) => State::Running,
            Lookup::Gone => State::Gone,
            Lookup::Unknown => State::Unknown,
        }
    }

    /// Whether this very process still runs, stopped or not: its pid lives
    /// with the same start time.
    pub fn running(&self) -> bool {
        matches!(self.state(), State::Running | State::Stopped)
    }

    /// When it started, in milliseconds since 1970.
    pub fn started_ms(&self) -> u64 {
        self.start[0]
            .saturating_mul(1000)
            .saturating_add(self.start[1] / 1000)
    }

    /// Whether this is the process asking.
    pub fn is_this_one(&self) -> bool {
        u32::try_from(self.pid).is_ok_and(|pid| pid == std::process::id())
    }
}

/// How a [`Process`] is now, as macOS says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Running,
    /// Suspended, as by SIGSTOP: it runs again only once continued.
    Stopped,
    /// No such process runs: its pid is free, or another process's.
    Gone,
    /// macOS would not say, as for another account's process.
    Unknown,
}

/// What macOS says of one of this account's processes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Facts {
    pub parent: i32,
    /// Its command name, at most 16 bytes as the kernel keeps it.
    pub command: String,
    pub start: [u64; 2],
    /// Whether it is suspended.
    pub stopped: bool,
}

/// One ancestor of this process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ancestor {
    pub pid: i32,
    pub command: String,
}

/// What looking a pid up found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Lookup {
    Found(Facts),
    /// No process has that pid.
    Gone,
    /// macOS would not say.
    Unknown,
}

/// The facts of `pid`, when it is a live process of this account.
pub fn process(pid: i32) -> Option<Facts> {
    match super::sys::process(pid) {
        Lookup::Found(facts) => Some(facts),
        Lookup::Gone | Lookup::Unknown => None,
    }
}

/// This process's ancestors, parent first, at most `limit`, ending at
/// launchd or at the first that cannot be read.
pub fn ancestors(limit: usize) -> Vec<Ancestor> {
    ancestors_from(nix::unistd::getppid().as_raw(), limit, process)
}

/// [`ancestors`] from `first`, described by `describe`.
fn ancestors_from(
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
        let this = Process::current().unwrap();
        assert!(this.running() && this.is_this_one());
        assert!(
            !Process::new(pid, [1, 0]).running(),
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
        let process = Process::new(pid, super::process(pid).unwrap().start);
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
                parent: if pid == 3 { 2 } else { 3 },
                command: "sh".to_owned(),
                start: [1, 0],
                stopped: false,
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
