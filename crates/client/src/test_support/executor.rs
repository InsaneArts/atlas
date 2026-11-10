//! Unprivileged subprocess stand-ins for interrupted mutation transactions.
use atlas_control::Controller;
use atlas_core::{
    process::Instance,
    test_support::{UID, directory, rerun},
};
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
/// This crate's worker test; a test binary above it names its own.
pub const WORKER: &str = "test_support::executor::tests::an_executor_test_worker";

/// Dispatch through the production executor handshake, then replace the
/// shell with `worker`, a test of this binary that runs [`executor_worker`],
/// of the same process identity. All effects are temporary.
pub fn pause_executor(directory: &Path, worker: &str) {
    let script = [
        crate::setup::executor::EXECUTOR,
        "exec \"$1\" --exact \"$2\" --nocapture\n",
    ]
    .concat();
    let _ = crate::setup::executor::tracked_output(
        Command::new("/bin/zsh")
            .args(["-f", "-c", &script, "zsh"])
            .arg(std::env::current_exe().unwrap())
            .arg(worker)
            .env("ATLAS_MUTATION_TEST_DIRECTORY", directory),
    );
}

/// What a worker test does: remove the temporary opener once released to,
/// and nothing when no test started it.
pub fn executor_worker() {
    let Some(directory) = directory() else {
        return;
    };
    executor_ready(&directory, || {
        fs::remove_file(directory.join("opener")).unwrap();
        fs::write(directory.join("changed"), b"").unwrap();
    })
    .unwrap();
}

/// Report identity before acting, and wait for an explicit continuation.
/// EOF and watchdog expiry end the worker rather than leaving an orphan.
pub fn executor_ready(directory: &Path, mutate: impl FnOnce()) -> io::Result<()> {
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
        let identity = Instance::new(
            pid,
            atlas_core::process::start(pid).map_err(io::Error::other)?,
        );
        let exit = watch(
            pid as usize,
            EventFilter::EVFILT_PROC,
            FilterFlag::NOTE_EXIT,
        )?;
        if atlas_core::process::start(pid).map_err(io::Error::other)? != identity.start {
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
        while atlas_core::process::info(self.identity.pid)
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
        if atlas_core::process::info(self.identity.pid).is_ok_and(|info| {
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

pub struct Fixture {
    pub caller: Child,
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
        // Darwin can stop accepting a watch before an exiting child is
        // waitable. This unreaped child is still ours; wait for its status.
        Err(error) if error.raw_os_error() == Some(libc::ESRCH) => return child.wait(),
        Err(error) => return Err(error),
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

    pub fn spawn(directory: &Path, command: &mut Command) -> io::Result<Self> {
        let listener = UnixListener::bind(directory.join("executor.sock"))?;
        let caller = atlas_core::spawn::spawn(command)?;
        Ok(Self {
            caller,
            requester: None,
            listener: Some(listener),
            channel: None,
            executor: None,
        })
    }

    pub fn request(&mut self, command: &mut Command) -> io::Result<()> {
        self.requester = Some(atlas_core::spawn::spawn(
            command
                .stdin(Stdio::from(self.caller.stdout.take().unwrap()))
                .stdout(Stdio::from(self.caller.stdin.take().unwrap())),
        )?);
        Ok(())
    }

    pub fn interrupt(&mut self) -> io::Result<()> {
        let caller = self.requester.as_mut().unwrap_or(&mut self.caller);
        caller.kill()?;
        caller.wait()?;
        Ok(())
    }

    pub fn wait(&mut self) -> io::Result<()> {
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

    pub fn ready(&mut self) -> io::Result<()> {
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

    pub fn finish(&mut self) -> io::Result<()> {
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
pub fn interrupted_executor(test: &str, mode: &str) -> tempfile::TempDir {
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
#[path = "executor_tests.rs"]
mod tests;
