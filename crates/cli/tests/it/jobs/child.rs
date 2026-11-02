use atlas_core::process::{self, Instance};
use nix::{
    sys::{
        signal::{Signal, kill, killpg},
        stat::Mode,
        wait::{WaitPidFlag, WaitStatus, waitpid},
    },
    unistd::{Pid, mkfifo},
};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    time::{Duration, Instant},
};
use tokio::{io::AsyncReadExt, net::unix::pipe};

pub(super) struct Child {
    // Reap before closing control pipes or removing their directory.
    owned: Processes,
    folder: tempfile::TempDir,
    events: pipe::Receiver,
    leader: File,
    descendant: File,
    parent: File,
}

impl Child {
    pub(super) fn new() -> Self {
        let folder = tempfile::tempdir().unwrap();
        let fifo = |name: &str| {
            let path = folder.path().join(name);
            mkfifo(&path, Mode::S_IRUSR | Mode::S_IWUSR).unwrap();
            OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(nix::libc::O_NONBLOCK)
                .open(path)
                .unwrap()
        };
        let events = pipe::Receiver::from_file(fifo("events")).unwrap();
        let leader = fifo("leader");
        let descendant = fifo("descendant");
        let parent = fifo("parent");
        std::fs::write(folder.path().join("child.pl"), include_str!("child.pl")).unwrap();
        Self {
            folder,
            events,
            leader,
            descendant,
            parent,
            owned: Processes::default(),
        }
    }

    pub(super) fn argv(&self, mode: &str) -> Vec<String> {
        let mut argv = vec!["/usr/bin/perl".to_owned()];
        for name in ["child.pl", "events", "leader", "descendant", "parent"] {
            argv.push(self.folder.path().join(name).to_str().unwrap().to_owned());
        }
        argv.push(mode.to_owned());
        argv
    }

    pub(super) async fn message(&mut self, expected: &str) -> Instance {
        let message = async {
            let mut bytes = Vec::new();
            loop {
                let byte = self.events.read_u8().await.unwrap();
                if byte == b'\n' {
                    break;
                }
                bytes.push(byte);
            }
            String::from_utf8(bytes).unwrap()
        };
        let line = tokio::time::timeout(Duration::from_secs(20), message)
            .await
            .unwrap();
        let (message, pid) = line.split_once(' ').unwrap();
        assert_eq!(message, expected, "{line}");
        let pid = pid.parse().unwrap();
        if let Some(owned) = self.owned.0.iter().find(|owned| owned.pid == pid) {
            return *owned;
        }
        let info = process::info(pid).unwrap().unwrap();
        assert!(info.owned_by(nix::unistd::getuid().as_raw()));
        self.owned.0.push(info.instance);
        info.instance
    }

    pub(super) fn permit(&mut self, message: &str) {
        writeln!(self.leader, "{message}").unwrap();
    }

    pub(super) fn permit_descendant(&mut self, message: &str) {
        writeln!(self.descendant, "{message}").unwrap();
    }

    pub(super) fn permit_parent(&mut self, message: &str) {
        writeln!(self.parent, "{message}").unwrap();
    }

    fn spawn(&mut self, mode: &str) -> std::process::Child {
        use std::os::unix::process::CommandExt;

        let argv = self.argv(mode);
        let child = std::process::Command::new(&argv[0])
            .args(&argv[1..])
            .process_group(0)
            .spawn()
            .unwrap();
        let instance = process::info(child.id() as i32).unwrap().unwrap().instance;
        self.owned.0.push(instance);
        child
    }
}

#[derive(Default)]
pub(super) struct Processes(Vec<Instance>);

impl Processes {
    pub(super) fn new(leader: Instance) -> Self {
        Self(vec![leader])
    }
}

impl Processes {
    fn cleanup(&mut self) -> Result<(), String> {
        // The leader's exact start time pins its group. Take its members
        // before signalling, so orphaned children are also waited for.
        if let Some(leader) = self.0.first().copied()
            && let Some(info) = process::info(leader.pid).map_err(|error| error.to_string())?
            && info.instance == leader
            && info.pgid == leader.pid
        {
            for member in process::group(leader.pid).map_err(|error| error.to_string())? {
                if !self.0.contains(&member.instance) {
                    self.0.push(member.instance);
                }
            }
            match killpg(Pid::from_raw(leader.pid), Signal::SIGKILL) {
                Ok(()) | Err(nix::errno::Errno::ESRCH) => {}
                Err(error) => return Err(format!("cannot kill fixture group: {error}")),
            }
        }
        // An escaped child is only signalled while its own start time matches.
        for owned in self.0.iter().rev() {
            if let Some(info) = process::info(owned.pid).map_err(|error| error.to_string())?
                && info.instance == *owned
            {
                match kill(Pid::from_raw(owned.pid), Signal::SIGKILL) {
                    Ok(()) | Err(nix::errno::Errno::ESRCH) => {}
                    Err(error) => return Err(format!("cannot kill fixture: {error}")),
                }
            }
        }
        let until = Instant::now() + Duration::from_secs(5);
        for owned in &self.0 {
            loop {
                let info = process::info(owned.pid).map_err(|error| error.to_string())?;
                if info.is_none_or(|info| info.instance != *owned) {
                    break;
                }
                match waitpid(Pid::from_raw(owned.pid), Some(WaitPidFlag::WNOHANG)) {
                    Ok(WaitStatus::StillAlive) | Err(nix::errno::Errno::ECHILD) => {}
                    Ok(_) => break,
                    Err(nix::errno::Errno::EINTR) => continue,
                    Err(error) => return Err(format!("cannot reap fixture: {error}")),
                }
                if Instant::now() >= until {
                    return Err(format!("fixture was not reaped: {owned:?}"));
                }
                // Kernel exit/reparenting has no userspace notification here;
                // poll only for cleanup, under the outer watchdog.
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        Ok(())
    }
}

impl Drop for Processes {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup() {
            if std::thread::panicking() {
                eprintln!("fixture cleanup failed while unwinding: {error}");
            } else {
                panic!("fixture cleanup failed: {error}");
            }
        }
    }
}

#[tokio::test]
async fn a_ready_fixture_observes_term_and_keeps_running() {
    use std::os::unix::process::ExitStatusExt;

    let mut fixture = Child::new();
    let mut child = fixture.spawn("ignore");
    let leader = fixture.message("ready").await;
    fixture.permit("start");
    assert_eq!(process::info(leader.pid).unwrap().unwrap().instance, leader);
    kill(Pid::from_raw(leader.pid), Signal::SIGTERM).unwrap();
    assert_eq!(fixture.message("term").await, leader);
    assert!(child.try_wait().unwrap().is_none());
    kill(Pid::from_raw(leader.pid), Signal::SIGKILL).unwrap();
    assert_eq!(child.wait().unwrap().signal(), Some(Signal::SIGKILL as i32));
}

#[tokio::test]
async fn a_fixture_failure_reaps_its_group_and_escaped_descendant() {
    for stage in ["registered", "permitted", "escaped"] {
        let mut fixture = Child::new();
        let mut child = fixture.spawn("escape");
        let leader = fixture.message("ready").await;
        fixture.permit("start");
        let member = fixture.message("member").await;
        fixture.permit("fork");
        let parent = fixture.message("parent").await;
        fixture.permit_parent("fork");
        let descendant = fixture.message("descendant").await;
        let info = process::info(descendant.pid).unwrap().unwrap();
        assert_eq!(info.ppid, parent.pid);
        assert_eq!(info.pgid, leader.pid);
        if stage != "registered" {
            fixture.permit_descendant("escape");
        }
        if stage == "escaped" {
            assert_eq!(fixture.message("escaped").await, descendant);
            assert_ne!(
                process::info(descendant.pid).unwrap().unwrap().pgid,
                leader.pid
            );
        }

        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _fixture = fixture;
            panic!("fixture assertion failed");
        }));
        assert!(failure.is_err());
        let reaped = child.try_wait();
        if matches!(reaped, Ok(None)) {
            Processes::new(leader).cleanup().unwrap();
        }
        assert_eq!(reaped.unwrap_err().raw_os_error(), Some(nix::libc::ECHILD));
        for owned in [leader, member, parent, descendant] {
            assert!(process::info(owned.pid).unwrap().is_none(), "{owned:?}");
        }
    }
}

#[tokio::test]
async fn a_fixture_failure_before_descendant_registration_reaps_its_group() {
    let mut fixture = Child::new();
    let mut child = fixture.spawn("escape");
    let leader = fixture.message("ready").await;
    fixture.permit("start");
    let member = fixture.message("member").await;
    fixture.permit("fork");
    let parent = fixture.message("parent").await;
    fixture.permit_parent("fork");
    // Refuse the announcement before recording its identity, just as a failed
    // process observation would. The descendant must still belong to the group.
    let failure = tokio::spawn(async move { fixture.message("escaped").await })
        .await
        .unwrap_err();
    assert!(failure.is_panic());
    let reaped = child.try_wait();
    if matches!(reaped, Ok(None)) {
        Processes::new(leader).cleanup().unwrap();
    }
    assert_eq!(reaped.unwrap_err().raw_os_error(), Some(nix::libc::ECHILD));
    assert!(process::group(leader.pid).unwrap().is_empty());
    for owned in [leader, member, parent] {
        assert!(process::info(owned.pid).unwrap().is_none(), "{owned:?}");
    }
}
