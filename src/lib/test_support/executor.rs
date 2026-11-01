//! Unprivileged subprocess stand-ins for interrupted mutation transactions.
use super::{UID, rerun};
use crate::{control::Controller, process::Instance};
use nix::sys::{
    event::{EvFlags, EventFilter, FilterFlag, KEvent, Kqueue},
    signal::{Signal, kill},
    socket::{getsockopt, sockopt::LocalPeerPid},
};
use std::{
    fs,
    io::{self, BufRead, BufReader, Read, Write},
    net::Shutdown,
    os::{
        fd::AsRawFd,
        unix::net::{UnixListener, UnixStream},
    },
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

const LIMIT: Duration = Duration::from_secs(15);
const WORKER: &str = "test_support::executor::tests::an_executor_test_worker";

/// Dispatch through the production executor handshake, then replace the
/// shell with a worker of the same process identity. All effects are temporary.
pub(crate) fn pause_executor(directory: &Path) {
    let script = [
        crate::setup::executor::EXECUTOR,
        "exec \"$1\" --exact \"$2\" --nocapture\n",
    ]
    .concat();
    let _ = crate::setup::executor::tracked_output(
        Command::new("/bin/zsh")
            .args(["-f", "-c", &script, "zsh"])
            .arg(std::env::current_exe().unwrap())
            .arg(WORKER)
            .env("ATLAS_MUTATION_TEST_DIRECTORY", directory),
    );
}

/// Report identity before acting, and wait for an explicit continuation.
/// EOF and watchdog expiry end the worker rather than leaving an orphan.
pub(crate) fn executor_ready(directory: &Path, mutate: impl FnOnce()) -> io::Result<()> {
    let mut channel = UnixStream::connect(directory.join("executor.sock"))?;
    channel.set_read_timeout(Some(LIMIT))?;
    channel.set_write_timeout(Some(LIMIT))?;
    let identity = Instance::current().ok_or_else(|| io::Error::other("no executor identity"))?;
    let identity = serde_json::to_string(&identity)?;
    writeln!(channel, "{identity}")?;
    expect(&mut channel, "observed")?;
    mutate();
    writeln!(channel, "ready {identity}")?;
    expect(&mut channel, "continue")?;
    writeln!(channel, "exited")
}

fn line(channel: &mut UnixStream) -> io::Result<String> {
    let mut bytes = Vec::new();
    BufReader::new(channel.take(256)).read_until(b'\n', &mut bytes)?;
    if bytes.last() != Some(&b'\n') {
        return Err(io::Error::other("incomplete executor message"));
    }
    String::from_utf8(bytes).map_err(io::Error::other)
}

fn expect(channel: &mut UnixStream, expected: &str) -> io::Result<()> {
    if line(channel)? != format!("{expected}\n") {
        return Err(io::Error::other("unexpected executor message"));
    }
    Ok(())
}

fn watch(ident: usize, filter: EventFilter, flags: FilterFlag) -> io::Result<Kqueue> {
    let queue = Kqueue::new()?;
    queue.kevent(
        &[KEvent::new(ident, filter, EvFlags::EV_ADD, flags, 0, 0)],
        &mut [],
        None,
    )?;
    Ok(queue)
}

fn observed(queue: &Kqueue, limit: Duration) -> io::Result<()> {
    let mut events = [KEvent::new(
        0,
        EventFilter::EVFILT_READ,
        EvFlags::empty(),
        FilterFlag::empty(),
        0,
        0,
    )];
    let timeout = *nix::sys::time::TimeSpec::from_duration(limit).as_ref();
    let count = queue.kevent(&[], &mut events, Some(timeout))?;
    if count == 0 {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "executor watchdog expired",
        ));
    }
    if events[0].flags().contains(EvFlags::EV_ERROR) {
        return Err(io::Error::other("executor kernel watch failed"));
    }
    Ok(())
}

/// The kernel watch belongs to the outer test, independently of the caller
/// it kills. A socket's last message alone never proves process exit.
struct Executor {
    identity: Instance,
    exit: Option<Kqueue>,
}

impl Executor {
    fn at(channel: &UnixStream) -> io::Result<Self> {
        let pid = getsockopt(channel, LocalPeerPid)?;
        let identity = Instance::new(pid, crate::process::start(pid).map_err(io::Error::other)?);
        let exit = watch(
            pid as usize,
            EventFilter::EVFILT_PROC,
            FilterFlag::NOTE_EXIT,
        )?;
        if crate::process::start(pid).map_err(io::Error::other)? != identity.start {
            return Err(io::Error::other("executor identity changed"));
        }
        Ok(Self {
            identity,
            exit: Some(exit),
        })
    }

    fn wait(&mut self) -> io::Result<()> {
        if let Some(exit) = &self.exit {
            observed(exit, LIMIT)?;
            self.exit = None;
        }
        Ok(())
    }

    fn finish(&mut self, limit: Duration) -> io::Result<()> {
        self.wait()?;
        // Recovery still excludes an exited executor until its identity is gone.
        let started = Instant::now();
        while crate::process::info(self.identity.pid)
            .map_err(io::Error::other)?
            .is_some_and(|info| info.instance == self.identity)
        {
            if started.elapsed() >= limit {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "executor reaping watchdog expired",
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    fn stop(&mut self) -> io::Result<()> {
        if self.wait().is_ok() {
            return Ok(());
        }
        // Only this captured lifetime may be signalled; a reused PID is foreign.
        if crate::process::info(self.identity.pid).is_ok_and(|info| {
            info.is_some_and(|info| info.instance == self.identity && !info.zombie)
        }) {
            let _ = kill(
                nix::unistd::Pid::from_raw(self.identity.pid),
                Signal::SIGKILL,
            );
        }
        self.wait()
    }
}

pub(crate) struct Fixture {
    pub(crate) caller: Child,
    requester: Option<Child>,
    listener: Option<UnixListener>,
    channel: Option<UnixStream>,
    executor: Option<Executor>,
}

fn wait_child(child: &mut Child) -> io::Result<ExitStatus> {
    if let Some(status) = child.try_wait()? {
        return Ok(status);
    }
    let exit = match watch(
        child.id() as usize,
        EventFilter::EVFILT_PROC,
        FilterFlag::NOTE_EXIT,
    ) {
        Ok(exit) => exit,
        // The child can exit between try_wait and registering the watch.
        Err(error) => return child.try_wait()?.ok_or(error),
    };
    observed(&exit, LIMIT)?;
    child.wait()
}

impl Fixture {
    fn start(directory: &Path, test: &str, mode: &str) -> io::Result<Self> {
        Self::spawn(
            directory,
            rerun(test)
                .arg("--nocapture")
                .env("ATLAS_MUTATION_TEST_DIRECTORY", directory)
                .env("ATLAS_MUTATION_TEST_MODE", mode)
                .stdout(Stdio::null())
                .stderr(fs::File::create(directory.join("caller.stderr"))?),
        )
    }

    pub(crate) fn spawn(directory: &Path, command: &mut Command) -> io::Result<Self> {
        let listener = UnixListener::bind(directory.join("executor.sock"))?;
        let caller = command.spawn()?;
        Ok(Self {
            caller,
            requester: None,
            listener: Some(listener),
            channel: None,
            executor: None,
        })
    }

    pub(crate) fn request(&mut self, command: &mut Command) -> io::Result<()> {
        self.requester = Some(
            command
                .stdin(Stdio::from(self.caller.stdout.take().unwrap()))
                .stdout(Stdio::from(self.caller.stdin.take().unwrap()))
                .spawn()?,
        );
        Ok(())
    }

    pub(crate) fn interrupt(&mut self) -> io::Result<()> {
        let caller = self.requester.as_mut().unwrap_or(&mut self.caller);
        caller.kill()?;
        caller.wait()?;
        Ok(())
    }

    pub(crate) fn wait(&mut self) -> io::Result<()> {
        if !wait_child(&mut self.caller)?.success() {
            return Err(io::Error::other("executor fixture worker failed"));
        }
        if let Some(requester) = &mut self.requester
            && !wait_child(requester)?.success()
        {
            return Err(io::Error::other("executor fixture requester failed"));
        }
        Ok(())
    }

    fn connect(&mut self) -> io::Result<()> {
        let listener = self.listener.take().unwrap();
        let incoming = watch(
            listener.as_raw_fd() as usize,
            EventFilter::EVFILT_READ,
            FilterFlag::empty(),
        )?;
        observed(&incoming, LIMIT)?;
        let (channel, _) = listener.accept()?;
        channel.set_read_timeout(Some(LIMIT))?;
        channel.set_write_timeout(Some(LIMIT))?;
        self.channel = Some(channel);
        self.executor = Some(Executor::at(self.channel.as_ref().unwrap())?);
        Ok(())
    }

    pub(crate) fn ready(&mut self) -> io::Result<()> {
        if self.channel.is_none() {
            self.connect()?;
        }
        let channel = self.channel.as_mut().unwrap();
        let identity = line(channel)?;
        let reported: Instance = serde_json::from_str(&identity)?;
        if reported != self.executor.as_ref().unwrap().identity {
            return Err(io::Error::other("executor identity mismatch"));
        }
        writeln!(channel, "observed")?;
        expect(channel, &format!("ready {}", identity.trim_end()))
    }

    pub(crate) fn finish(&mut self) -> io::Result<()> {
        let channel = self.channel.as_mut().unwrap();
        writeln!(channel, "continue")?;
        expect(channel, "exited")?;
        let executor = self.executor.as_mut().unwrap();
        executor.wait()?;
        // Acceptance workers are our children; their requester cannot reap them.
        if executor.identity.pid == self.caller.id() as i32 {
            wait_child(&mut self.caller)?;
        }
        executor.finish(LIMIT)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(channel) = self.channel.take() {
            // Closing the supervisor's endpoint also releases an abandoned worker.
            let _ = channel.shutdown(Shutdown::Both);
        }
        let stopped = self.executor.as_mut().map(Executor::stop).transpose();
        let requester_reaped = if let Some(requester) = &mut self.requester {
            let _ = requester.kill();
            requester.wait().map(|_| ())
        } else {
            Ok(())
        };
        // The caller is our child even when readiness or an assertion failed.
        let _ = self.caller.kill();
        let reaped = self.caller.wait();
        if let Err(error) = stopped.and(requester_reaped).and(reaped.map(|_| ())) {
            if std::thread::panicking() {
                eprintln!("executor fixture cleanup failed: {error}");
            } else {
                panic!("executor fixture cleanup failed: {error}");
            }
        }
    }
}

/// Kill the actual transaction's caller only after its executor reports
/// readiness. The outer fixture retains the exit watch and cleanup ownership.
pub(crate) fn interrupted_executor(test: &str, mode: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("opener"), b"temporary opener").unwrap();
    let errors = directory.path().join("caller.stderr");
    let mut fixture = Fixture::start(directory.path(), test, mode).unwrap();
    let ready = fixture.ready();
    assert!(
        ready.is_ok(),
        "{mode}: {ready:?}\n{}",
        fs::read_to_string(&errors).unwrap()
    );
    let controller = Controller::at(UID, &directory.path().join("state"));
    let held_during_dispatch = controller.acquire(true).is_err();
    let claimed_during_dispatch = controller.pending().unwrap().len() == 1;
    fixture.caller.kill().unwrap();
    fixture.caller.wait().unwrap();
    let before = controller.pending().unwrap();
    let recovery_blocked = controller.acquire(true).is_err();
    controller.clear().unwrap();
    let after = controller.pending().unwrap();
    let normal_blocked = controller.acquire(false).is_err();
    let finished = fixture.finish();
    assert!(
        finished.is_ok(),
        "{mode}: {finished:?}\n{}",
        fs::read_to_string(&errors).unwrap()
    );
    assert!(
        held_during_dispatch,
        "setup and teardown exclude each other: {mode}"
    );
    assert!(
        claimed_during_dispatch,
        "the mutation is claimed before dispatch: {mode}"
    );
    assert!(
        recovery_blocked && normal_blocked,
        "the surviving executor excludes recovery: {mode}"
    );
    assert_eq!(
        after, before,
        "recovery preserves the executor's claim: {mode}"
    );
    assert!(directory.path().join("changed").exists());
    let held = controller.acquire(true).unwrap();
    controller.clear().unwrap();
    drop(held);
    assert!(controller.acquire(false).is_ok());
    directory
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_executor_test_worker() {
        let Some(directory) = super::super::directory() else {
            return;
        };
        executor_ready(&directory, || {
            fs::remove_file(directory.join("opener")).unwrap();
            fs::write(directory.join("changed"), b"").unwrap();
        })
        .unwrap();
    }

    #[test]
    fn an_executor_reports_readiness_before_its_caller_is_killed() {
        interrupted_executor("init::tests::a_job_installation_test_caller", "install");
    }

    #[test]
    fn a_surviving_executor_keeps_recovery_excluded_until_its_observed_exit() {
        interrupted_executor(
            "onboarding::tests::an_account_setup_test_caller",
            "interactive",
        );
    }

    #[test]
    fn a_lost_readiness_channel_fails_and_reaps_its_fixture() {
        let directory = tempfile::tempdir().unwrap();
        let mut fixture = Fixture::start(directory.path(), WORKER, "").unwrap();
        fixture.connect().unwrap();
        let identity = fixture.executor.as_ref().unwrap().identity;
        fixture
            .channel
            .as_ref()
            .unwrap()
            .shutdown(Shutdown::Both)
            .unwrap();
        assert!(fixture.ready().is_err());
        drop(fixture);
        assert_eq!(crate::process::info(identity.pid).unwrap(), None);
        assert!(!directory.path().join("changed").exists());
    }

    #[test]
    fn a_failed_assertion_still_releases_and_reaps_its_fixture() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("opener"), b"temporary opener").unwrap();
        let mut fixture = Fixture::start(directory.path(), WORKER, "").unwrap();
        fixture.ready().unwrap();
        let identity = fixture.executor.as_ref().unwrap().identity;
        let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _fixture = fixture;
            panic!("the fixture's assertion failed");
        }));
        assert!(failed.is_err());
        assert_eq!(crate::process::info(identity.pid).unwrap(), None);
    }

    #[test]
    fn an_abandoned_continuation_cannot_leave_a_test_child() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("opener"), b"temporary opener").unwrap();
        let mut fixture = Fixture::start(directory.path(), WORKER, "").unwrap();
        fixture.ready().unwrap();
        let identity = fixture.executor.as_ref().unwrap().identity;
        drop(fixture);
        assert_eq!(crate::process::info(identity.pid).unwrap(), None);
        assert!(directory.path().join("changed").exists());
    }

    #[test]
    fn a_lost_exit_message_cannot_confirm_settlement() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("opener"), b"temporary opener").unwrap();
        let mut fixture = Fixture::start(directory.path(), WORKER, "").unwrap();
        fixture.ready().unwrap();
        let identity = fixture.executor.as_ref().unwrap().identity;
        fixture
            .channel
            .as_ref()
            .unwrap()
            .shutdown(Shutdown::Both)
            .unwrap();
        assert!(fixture.finish().is_err());
        drop(fixture);
        assert_eq!(crate::process::info(identity.pid).unwrap(), None);
    }

    #[test]
    fn a_failed_fixture_reaps_both_its_worker_and_its_requester() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("opener"), b"temporary opener").unwrap();
        let mut fixture = Fixture::spawn(
            directory.path(),
            rerun(WORKER)
                .env("ATLAS_MUTATION_TEST_DIRECTORY", directory.path())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null()),
        )
        .unwrap();
        fixture.request(&mut Command::new("/bin/cat")).unwrap();
        fixture.ready().unwrap();
        let worker = fixture.executor.as_ref().unwrap().identity;
        let requester = fixture.requester.as_ref().unwrap().id() as i32;
        drop(fixture);
        assert_eq!(crate::process::info(worker.pid).unwrap(), None);
        assert_eq!(crate::process::info(requester).unwrap(), None);
    }

    #[test]
    fn a_fixture_reaps_its_direct_worker_before_confirming_completion() {
        use std::os::fd::OwnedFd;

        let (channel, worker) = UnixStream::pair().unwrap();
        channel.set_read_timeout(Some(LIMIT)).unwrap();
        channel.set_write_timeout(Some(LIMIT)).unwrap();
        let input: OwnedFd = worker.try_clone().unwrap().into();
        let output: OwnedFd = worker.into();
        let caller = Command::new("/bin/sh")
            .args([
                "-c",
                "IFS= read -r message && [ \"$message\" = continue ] && printf 'exited\\n'",
            ])
            .stdin(Stdio::from(input))
            .stdout(Stdio::from(output))
            .spawn()
            .unwrap();
        let pid = caller.id() as i32;
        let identity = Instance::new(pid, crate::process::start(pid).unwrap());
        let mut fixture = Fixture {
            caller,
            requester: None,
            listener: None,
            channel: Some(channel),
            executor: Some(Executor {
                identity,
                exit: Some(
                    watch(
                        pid as usize,
                        EventFilter::EVFILT_PROC,
                        FilterFlag::NOTE_EXIT,
                    )
                    .unwrap(),
                ),
            }),
        };
        fixture.finish().unwrap();
        fixture.wait().unwrap();
        assert_eq!(crate::process::info(pid).unwrap(), None);
    }

    #[test]
    fn an_exit_watch_requires_the_owned_process_to_end_and_be_reaped() {
        let caller = Command::new("/bin/cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let mut fixture = Fixture {
            caller,
            requester: None,
            listener: None,
            channel: None,
            executor: None,
        };
        let pid = fixture.caller.id() as i32;
        let identity = Instance::new(pid, crate::process::start(pid).unwrap());
        fixture.executor = Some(Executor {
            identity,
            exit: Some(
                watch(
                    pid as usize,
                    EventFilter::EVFILT_PROC,
                    FilterFlag::NOTE_EXIT,
                )
                .unwrap(),
            ),
        });
        assert_eq!(
            observed(
                fixture.executor.as_ref().unwrap().exit.as_ref().unwrap(),
                Duration::ZERO
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::TimedOut
        );
        drop(fixture.caller.stdin.take());
        fixture.executor.as_mut().unwrap().wait().unwrap();
        let started = Instant::now();
        loop {
            let exited = crate::process::info(pid).unwrap().unwrap();
            assert_eq!(exited.instance, identity);
            if exited.zombie {
                break;
            }
            assert!(
                started.elapsed() < LIMIT,
                "executor zombie watchdog expired"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            fixture
                .executor
                .as_mut()
                .unwrap()
                .finish(Duration::from_millis(50))
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut,
            "an observed exit cannot finish while its identity remains unreaped"
        );
        assert!(fixture.caller.wait().unwrap().success());
        fixture.executor.as_mut().unwrap().finish(LIMIT).unwrap();
        assert_eq!(crate::process::info(identity.pid).unwrap(), None);
    }
}
