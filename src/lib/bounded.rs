//! Time-limited commands with owned process groups and bounded output.
use nix::{
    fcntl::{FcntlArg, OFlag, fcntl},
    poll::{PollFd, PollFlags, poll},
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use std::{
    error::Error,
    fmt, io,
    os::{
        fd::{AsFd, OwnedFd},
        unix::process::CommandExt,
    },
    process::{Child, Command, Output, Stdio},
    time::{Duration, Instant},
};

/// How long one look at directory services or the process list may take.
pub(crate) const LOOKUP: Duration = Duration::from_secs(3);

/// How long a one-time listing during setup may take, such as every
/// account directory services know, which can be slow on a Mac bound to a
/// network directory. Nothing polls it.
pub(crate) const LISTING: Duration = Duration::from_secs(30);

/// How long a system tool may take over one file or folder it does not
/// copy: reading an Info.plist or a folder's access list, or changing one
/// folder's access.
pub(crate) const FILE: Duration = Duration::from_secs(10);

/// How long copying an app or a file for the helper may take: long enough
/// for the largest app, short enough that a copy stuck on a disk that
/// stopped answering ends.
pub(crate) const COPY: Duration = Duration::from_secs(10 * 60);

/// How often a running command is checked on.
const POLL: Duration = Duration::from_millis(5);

/// How long output may keep arriving after its command ended.
const DRAIN: Duration = Duration::from_millis(500);

/// Each stream retains at most 8 MiB, for at most 16 MiB combined.
const STREAM_LIMIT: usize = 8 * 1024 * 1024;

/// Run with no input in an isolated process group. Timeouts, oversized
/// output and I/O failures stop the group and reap the child; a cleanup
/// failure retains both causes and reports which termination is unknown.
pub(crate) fn output(command: &mut Command, limit: Duration) -> io::Result<Output> {
    output_with_limit(command, limit, STREAM_LIMIT)
}

fn output_with_limit(command: &mut Command, limit: Duration, cap: usize) -> io::Result<Output> {
    let deadline = Instant::now() + limit;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()?;
    #[cfg(test)]
    faults::spawned(child.id());
    let mut stdout = Pipe::new(child.stdout.take().map(OwnedFd::from));
    let mut stderr = Pipe::new(child.stderr.take().map(OwnedFd::from));
    let result = stdout
        .configure()
        .and_then(|()| stderr.configure())
        .and_then(|()| drain(&child, &mut stdout, &mut stderr, deadline, cap));
    // Close both readers before stopping writers, including on setup failure.
    drop(stdout.fd.take());
    drop(stderr.fd.take());
    let status = match result {
        // A failed wait cannot prove the identity is still reserved.
        // Never signal the group after attempting the final reap.
        Ok(()) => reap(&mut child),
        Err(primary) if primary.raw_os_error() == Some(libc::ECHILD) => Err(primary),
        Err(primary) => {
            return Err(match stop(&mut child) {
                Ok(()) => primary,
                // A timeout with failed cleanup is not settled: callers may
                // remove staging on TimedOut, so retain it as an unknown I/O
                // outcome with the original kind still in the private cause.
                Err(cleanup) => {
                    let kind = match primary.kind() {
                        io::ErrorKind::TimedOut => io::ErrorKind::Other,
                        kind => kind,
                    };
                    io::Error::new(kind, OutputFailure { primary, cleanup })
                }
            });
        }
    }
    .map_err(|error| {
        io::Error::new(
            error.kind(),
            CleanupFailure {
                pid: child.id(),
                group: None,
                child: Some(error),
            },
        )
    })?;
    Ok(Output {
        status,
        stdout: stdout.bytes,
        stderr: stderr.bytes,
    })
}

struct Pipe {
    fd: Option<OwnedFd>,
    bytes: Vec<u8>,
}

impl Pipe {
    fn new(fd: Option<OwnedFd>) -> Self {
        Self {
            fd,
            bytes: Vec::new(),
        }
    }

    fn configure(&self) -> io::Result<()> {
        #[cfg(test)]
        faults::check(faults::Operation::Configure)?;
        if let Some(fd) = &self.fd {
            let flags = OFlag::from_bits_truncate(fcntl(fd, FcntlArg::F_GETFL)?);
            fcntl(fd, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK))?;
        }
        Ok(())
    }

    fn read(&mut self, cap: usize) -> io::Result<()> {
        let Some(fd) = &self.fd else {
            return Ok(());
        };
        let mut chunk = [0; 8192];
        let remaining = cap - self.bytes.len();
        // One extra byte detects overflow without retaining it. Both slice
        // bounds are at most the fixed chunk length; bytes never exceeds cap.
        let count = chunk.len().min(remaining.saturating_add(1));
        let mut read = || nix::unistd::read(fd, &mut chunk[..count]).map_err(io::Error::from);
        #[cfg(test)]
        let result = faults::check(faults::Operation::Read).and_then(|()| read());
        #[cfg(not(test))]
        let result = read();
        match result {
            Ok(0) => self.fd = None,
            Ok(size) if size > remaining => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "The command wrote too much output",
                ));
            }
            Ok(size) => {
                let needed = self.bytes.len() + size;
                if needed > self.bytes.capacity() {
                    self.bytes
                        .reserve_exact(needed.next_power_of_two().min(cap) - self.bytes.len());
                }
                self.bytes.extend_from_slice(&chunk[..size]);
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error),
        }
        Ok(())
    }
}

fn drain(
    child: &Child,
    stdout: &mut Pipe,
    stderr: &mut Pipe,
    deadline: Instant,
    cap: usize,
) -> io::Result<()> {
    let mut next_observation = Instant::now();
    let mut exited = None;
    let mut stderr_deadline = None;
    loop {
        let now = Instant::now();
        if exited.is_none() && now >= next_observation {
            if observe_exit(child)? {
                exited = Some(now);
            } else if now >= deadline {
                return Err(late());
            }
            next_observation = Instant::now() + POLL;
        }
        if let Some(ended) = exited {
            if stdout.fd.is_none() && stderr.fd.is_none() {
                return Ok(());
            }
            // Preserve the existing sequential stdout/stderr drain budgets,
            // while reading both pipes concurrently to prevent backpressure.
            let until = if stdout.fd.is_some() {
                ended + DRAIN
            } else {
                *stderr_deadline.get_or_insert(now + DRAIN)
            };
            if now >= until {
                return Err(late());
            }
            wait(
                stdout,
                stderr,
                until.saturating_duration_since(now).min(POLL),
            )?;
        } else {
            wait(
                stdout,
                stderr,
                next_observation
                    .min(deadline)
                    .saturating_duration_since(now),
            )?;
        }
        stdout.read(cap)?;
        stderr.read(cap)?;
    }
}

fn wait(stdout: &Pipe, stderr: &Pipe, delay: Duration) -> io::Result<()> {
    let stdout = stdout
        .fd
        .as_ref()
        .map(|fd| PollFd::new(fd.as_fd(), PollFlags::POLLIN));
    let stderr = stderr
        .fd
        .as_ref()
        .map(|fd| PollFd::new(fd.as_fd(), PollFlags::POLLIN));
    // Round up sub-millisecond waits rather than busy-spinning near a poll.
    let timeout = delay.as_millis().min(u16::MAX as u128) as u16;
    let timeout =
        timeout.saturating_add(u16::from(!delay.subsec_nanos().is_multiple_of(1_000_000)));
    let wait = || match (stdout, stderr) {
        (Some(out), Some(err)) => poll(&mut [out, err], timeout),
        (Some(fd), None) | (None, Some(fd)) => poll(&mut [fd], timeout),
        (None, None) => poll(&mut [], timeout),
    };
    #[cfg(test)]
    let result =
        faults::check(faults::Operation::Poll).and_then(|()| wait().map_err(io::Error::from));
    #[cfg(not(test))]
    let result = wait().map_err(io::Error::from);
    match result {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::Interrupted => Ok(()),
        Err(error) => Err(error),
    }
}

#[allow(unsafe_code, reason = "nix does not expose waitid on macOS")]
fn observe_exit(child: &Child) -> io::Result<bool> {
    observe_exit_using(child, |pid| {
        // SAFETY: an all-zero siginfo_t is valid initialized stack storage.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        // SAFETY: pid is the owned, positive, unreaped child's PID. info is
        // initialized, correctly aligned and live throughout waitid. WNOWAIT
        // preserves child/group identity; the return and si_pid are checked.
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                pid,
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        if result == -1 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: waitid succeeded and info is initialized siginfo_t storage.
        Ok(unsafe { info.si_pid() })
    })
}

fn observe_exit_using(
    child: &Child,
    mut observe: impl FnMut(u32) -> io::Result<i32>,
) -> io::Result<bool> {
    let pid = i32::try_from(child.id())
        .ok()
        .filter(|pid| *pid > 0)
        .ok_or_else(|| io::Error::other("Invalid command process identity"))?;
    loop {
        match observe(pid as u32) {
            Ok(0) => return Ok(false),
            Ok(observed) if observed == pid => return Ok(true),
            Ok(_) => return Err(io::Error::other("Unexpected command process identity")),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

fn reap(child: &mut Child) -> io::Result<std::process::ExitStatus> {
    #[cfg(test)]
    faults::check(faults::Operation::Wait)?;
    child.wait()
}

fn stop(child: &mut Child) -> Result<(), CleanupFailure> {
    let pid = child.id();
    let grouped = stop_group(pid);
    // A failed group signal can leave descendants running even if the
    // leader can be killed. Retain that failure after reaping the leader.
    let killed = if grouped.is_ok() {
        Ok(())
    } else {
        kill_child(child)
    };
    let grouped = match grouped {
        // The unreaped leader reserves the identity, so ESRCH proves no
        // group remains. Still kill/reap the leader if it left that group.
        Err(error) if error.raw_os_error() == Some(libc::ESRCH) => Ok(()),
        result => result,
    };
    let reaped = if killed.is_ok() {
        reap(child).map(|_| ())
    } else {
        observe_exit(child).and_then(|ended| {
            if ended {
                reap(child).map(|_| ())
            } else {
                killed
            }
        })
    };
    match (grouped, reaped) {
        (Ok(()), Ok(())) => Ok(()),
        (group, child) => Err(CleanupFailure {
            pid,
            group: group.err(),
            child: child.err(),
        }),
    }
}

fn stop_group(pid: u32) -> io::Result<()> {
    #[cfg(test)]
    {
        faults::signalling();
        faults::check(faults::Operation::KillGroup)?;
    }
    let pid = i32::try_from(pid)
        .ok()
        .filter(|pid| *pid > 0)
        .ok_or_else(|| io::Error::other("Invalid command process identity"))?;
    killpg(Pid::from_raw(pid), Signal::SIGKILL).map_err(Into::into)
}

fn kill_child(child: &mut Child) -> io::Result<()> {
    #[cfg(test)]
    faults::check(faults::Operation::KillChild)?;
    child.kill()
}

#[derive(Debug)]
struct CleanupFailure {
    pid: u32,
    group: Option<io::Error>,
    child: Option<io::Error>,
}

impl fmt::Display for CleanupFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.child.is_some() {
            write!(
                f,
                "Termination of command process {} and its group is unknown",
                self.pid
            )
        } else {
            write!(
                f,
                "Command process {} was reaped; termination of its group is unknown",
                self.pid
            )
        }
    }
}

impl Error for CleanupFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.group
            .as_ref()
            .or(self.child.as_ref())
            .map(|error| error as _)
    }
}

#[derive(Debug)]
struct OutputFailure {
    primary: io::Error,
    cleanup: CleanupFailure,
}

impl fmt::Display for OutputFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Only operational identity is public; neither output nor arbitrary
        // nested error text is included in the combined diagnostic.
        let message = if self.primary.kind() == io::ErrorKind::InvalidData {
            "The command wrote too much output"
        } else {
            "The command failed"
        };
        write!(f, "{message}; {}", self.cleanup)
    }
}

impl Error for OutputFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.primary)
    }
}

fn late() -> io::Error {
    io::Error::new(
        io::ErrorKind::TimedOut,
        "The command did not finish in time",
    )
}

#[cfg(test)]
mod tests {
    use super::{
        LISTING, LOOKUP, OutputFailure, Pipe, drain, faults, observe_exit, observe_exit_using,
        output, output_with_limit, stop,
    };
    use faults::Operation;
    use std::{
        error::Error,
        io::{self, ErrorKind, Read, Write},
        os::{fd::OwnedFd, unix::process::CommandExt},
        process::{Child, Command, Stdio},
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };

    const WATCHDOG: Duration = Duration::from_secs(20);

    fn assert_output_reaped() {
        assert_eq!(
            nix::sys::wait::waitpid(nix::unistd::Pid::from_raw(faults::child() as i32), None),
            Err(nix::errno::Errno::ECHILD),
        );
    }

    // A watchdog fails the test process instead of leaving a hung drain or
    // a detached supervisor. It never signals a potentially reused PID.
    fn watched<T>(run: impl FnOnce() -> T) -> T {
        let (done, receiver) = mpsc::channel();
        let watchdog = thread::spawn(move || {
            if receiver.recv_timeout(WATCHDOG) == Err(mpsc::RecvTimeoutError::Timeout) {
                std::process::abort();
            }
        });
        let result = run();
        done.send(()).unwrap();
        watchdog.join().unwrap();
        result
    }

    struct Fixture {
        child: Child,
        stdout: Pipe,
        stderr: Pipe,
        reaped: bool,
    }

    impl Fixture {
        fn ready(script: &str) -> Self {
            let mut child = Command::new("/bin/sh")
                .args(["-c", script])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .process_group(0)
                .spawn()
                .unwrap();
            let mut stderr = child.stderr.take().unwrap();
            let mut ready = [0];
            stderr.read_exact(&mut ready).unwrap();
            assert_eq!(&ready, b"r");
            let stdout = Pipe::new(child.stdout.take().map(OwnedFd::from));
            let stderr = Pipe::new(Some(stderr.into()));
            stdout.configure().unwrap();
            stderr.configure().unwrap();
            Self {
                child,
                stdout,
                stderr,
                reaped: false,
            }
        }

        fn proceed(&mut self) {
            self.child.stdin.take().unwrap().write_all(b"go\n").unwrap();
        }

        fn drain(&mut self, cap: usize) -> io::Result<()> {
            drain(
                &self.child,
                &mut self.stdout,
                &mut self.stderr,
                Instant::now() + WATCHDOG / 2,
                cap,
            )
        }

        fn assert_reaped(&mut self) {
            assert!(self.child.wait().unwrap().success());
            self.reaped = true;
            assert_eq!(
                nix::sys::wait::waitpid(nix::unistd::Pid::from_raw(self.child.id() as i32), None),
                Err(nix::errno::Errno::ECHILD),
            );
        }

        fn stop(&mut self) {
            stop(&mut self.child).unwrap();
            self.reaped = true;
        }

        fn assert_closed(&mut self) {
            while self.stdout.fd.is_some() || self.stderr.fd.is_some() {
                super::wait(&self.stdout, &self.stderr, super::POLL).unwrap();
                self.stdout.read(32).unwrap();
                self.stderr.read(32).unwrap();
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            // Never release the group identity before its last signal.
            if !self.reaped {
                self.stop();
            }
        }
    }

    #[test]
    fn a_command_that_ends_in_time_gives_its_output_and_status() {
        watched(|| {
            let output = output(
                Command::new("/bin/sh").args(["-c", "echo found; echo why >&2; exit 3"]),
                WATCHDOG / 2,
            )
            .unwrap();
            assert_eq!(output.stdout, b"found\n");
            assert_eq!(output.stderr, b"why\n");
            assert_eq!(output.status.code(), Some(3));
        });
    }

    #[test]
    fn a_command_gets_no_input() {
        watched(|| {
            let output = output(&mut Command::new("/bin/cat"), WATCHDOG / 2).unwrap();
            assert!(output.status.success());
            assert!(output.stdout.is_empty());
        });
    }

    #[test]
    fn a_command_still_running_at_the_deadline_is_killed_and_reaped() {
        watched(|| {
            let mut fixture = Fixture::ready("printf r >&2; read gate; exec /bin/sleep 60");
            let pid = nix::unistd::Pid::from_raw(fixture.child.id() as i32);
            let error = drain(
                &fixture.child,
                &mut fixture.stdout,
                &mut fixture.stderr,
                Instant::now(),
                32,
            )
            .unwrap_err();
            assert_eq!(error.kind(), ErrorKind::TimedOut);
            fixture.stop();
            assert_eq!(
                nix::sys::wait::waitpid(pid, None),
                Err(nix::errno::Errno::ECHILD)
            );
        });
    }

    #[test]
    fn a_command_still_running_at_the_deadline_is_killed_with_what_it_started() {
        watched(|| {
            // The descendant confirms its own readiness before the leader
            // announces readiness; both then wait on the continuation pipe.
            let mut fixture =
                Fixture::ready("/bin/sh -c 'printf r >&2; exec /bin/sleep 60' & wait");
            let error = drain(
                &fixture.child,
                &mut fixture.stdout,
                &mut fixture.stderr,
                Instant::now(),
                32,
            )
            .unwrap_err();
            assert_eq!(error.kind(), ErrorKind::TimedOut);
            fixture.stop();
            assert_eq!(
                nix::sys::wait::waitpid(
                    nix::unistd::Pid::from_raw(fixture.child.id() as i32),
                    None
                ),
                Err(nix::errno::Errno::ECHILD)
            );
            fixture.assert_closed();
        });
    }

    #[test]
    fn output_is_bounded_before_it_is_retained() {
        watched(|| {
            for redirect in ["", " >&2"] {
                for length in [30, 31, 32] {
                    let script = format!(
                        "printf r >&2; read gate; /usr/bin/printf '%0{length}d'{redirect} 0"
                    );
                    let mut fixture = Fixture::ready(&script);
                    fixture.proceed();
                    let result = fixture.drain(31);
                    let bytes = if redirect.is_empty() {
                        &fixture.stdout.bytes
                    } else {
                        &fixture.stderr.bytes
                    };
                    assert!(bytes.len() <= 31);
                    assert!(bytes.capacity() <= 31);
                    if length <= 31 {
                        result.unwrap();
                        assert_eq!(bytes.len(), length);
                        fixture.assert_reaped();
                    } else {
                        let error = result.unwrap_err();
                        assert_eq!(error.kind(), ErrorKind::InvalidData);
                        assert_eq!(error.to_string(), "The command wrote too much output");
                        fixture.stop();
                        assert_eq!(
                            nix::sys::wait::waitpid(
                                nix::unistd::Pid::from_raw(fixture.child.id() as i32),
                                None
                            ),
                            Err(nix::errno::Errno::ECHILD)
                        );
                    }
                }
            }
            let result = output_with_limit(
                Command::new("/bin/sh").args(["-c", "printf 12345678"]),
                WATCHDOG / 2,
                7,
            );
            assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
            assert_output_reaped();
        });
    }

    #[test]
    fn binary_and_multibyte_output_are_counted_as_bytes() {
        watched(|| {
            for cap in [6, 7, 8] {
                let result = output_with_limit(
                    Command::new("/bin/sh")
                        .args(["-c", "printf '\\000\\377é🙂'; printf '\\000\\377é🙂' >&2"]),
                    WATCHDOG / 2,
                    cap,
                );
                if cap < 8 {
                    let error = result.unwrap_err();
                    assert_eq!(error.kind(), ErrorKind::InvalidData, "{error:?}");
                    assert_output_reaped();
                } else {
                    let output = result.unwrap();
                    assert_eq!(
                        output.stdout,
                        [b"\0\xff".as_slice(), "é🙂".as_bytes()].concat()
                    );
                    assert_eq!(output.stdout, output.stderr);
                }
            }
            let output =
                output_with_limit(&mut Command::new("/usr/bin/true"), WATCHDOG / 2, 0).unwrap();
            assert!(output.stdout.is_empty() && output.stderr.is_empty());
        });
    }

    #[test]
    fn both_streams_are_drained_without_deadlock() {
        watched(|| {
            let script = "printf r >&2; read gate; /bin/dd if=/dev/zero bs=8192 count=32 2>/dev/null & (/bin/dd if=/dev/zero bs=8192 count=32 2>/dev/null) >&2; wait";
            let mut fixture = Fixture::ready(script);
            fixture.proceed();
            fixture.drain(262144).unwrap();
            assert_eq!(fixture.stdout.bytes, vec![0; 262144]);
            assert_eq!(fixture.stderr.bytes, vec![0; 262144]);
            assert!(fixture.stdout.bytes.capacity() <= 262144);
            assert!(fixture.stderr.bytes.capacity() <= 262144);
            fixture.assert_reaped();
        });
    }

    #[test]
    fn a_continuous_writer_is_stopped_at_the_byte_cap() {
        watched(|| {
            let mut fixture = Fixture::ready("printf r >&2; read gate; exec /usr/bin/yes");
            fixture.proceed();
            assert_eq!(
                fixture.drain(16384).unwrap_err().kind(),
                ErrorKind::InvalidData
            );
            assert!(fixture.stdout.bytes.len() <= 16384);
            assert!(fixture.stdout.bytes.capacity() <= 16384);
            fixture.stop();
            assert_eq!(
                nix::sys::wait::waitpid(
                    nix::unistd::Pid::from_raw(fixture.child.id() as i32),
                    None
                ),
                Err(nix::errno::Errno::ECHILD)
            );
        });
    }

    #[test]
    fn read_failure_keeps_its_original_kind() {
        // Reading a real directory descriptor fails in the kernel rather
        // than pretending a sender vanished or waiting for a timeout.
        let mut pipe = Pipe::new(Some(std::fs::File::open(".").unwrap().into()));
        pipe.configure().unwrap();
        let error = pipe.read(32).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(libc::EISDIR));
        assert_eq!(
            error.kind(),
            io::Error::from_raw_os_error(libc::EISDIR).kind()
        );
        assert!(pipe.bytes.is_empty());
    }

    #[test]
    fn pipe_setup_read_and_poll_errors_are_returned_after_cleanup() {
        watched(|| {
            for operation in [Operation::Configure, Operation::Read, Operation::Poll] {
                faults::inject([(operation, io::Error::from_raw_os_error(libc::EIO))]);
                let error = output(
                    Command::new("/bin/sh").args(["-c", "printf ready"]),
                    WATCHDOG / 2,
                )
                .unwrap_err();
                let primary = error
                    .get_ref()
                    .and_then(|source| source.downcast_ref::<OutputFailure>())
                    .map_or(&error, |failure| {
                        assert!(failure.cleanup.child.is_none(), "the child was reaped");
                        &failure.primary
                    });
                assert_eq!(primary.raw_os_error(), Some(libc::EIO), "{operation:?}");
                assert_eq!(error.kind(), primary.kind());
                assert_output_reaped();
            }
        });
    }

    #[test]
    fn interrupted_poll_and_reads_resume_without_losing_output() {
        watched(|| {
            faults::inject([
                (Operation::Poll, io::Error::from_raw_os_error(libc::EINTR)),
                (Operation::Read, io::Error::from_raw_os_error(libc::EAGAIN)),
                (Operation::Read, io::Error::from_raw_os_error(libc::EINTR)),
            ]);
            let output = output(
                Command::new("/bin/sh").args(["-c", "printf out; printf err >&2"]),
                WATCHDOG / 2,
            )
            .unwrap();
            assert_eq!(output.stdout, b"out");
            assert_eq!(output.stderr, b"err");
            assert_output_reaped();
        });
    }

    #[test]
    fn observing_exit_does_not_reap_the_child() {
        watched(|| {
            let mut fixture = Fixture::ready("printf r >&2; read gate; exit 0");
            assert!(!observe_exit(&fixture.child).unwrap());
            fixture.proceed();
            fixture.drain(32).unwrap();
            assert!(observe_exit(&fixture.child).unwrap());
            assert!(observe_exit(&fixture.child).unwrap());
            fixture.assert_reaped();
            assert_eq!(
                observe_exit(&fixture.child).unwrap_err().raw_os_error(),
                Some(libc::ECHILD)
            );
        });
    }

    #[test]
    fn observing_exit_retries_interruptions_and_checks_errors_and_identity() {
        watched(|| {
            let fixture = Fixture::ready("printf r >&2; read gate");
            let mut calls = 0;
            assert!(
                observe_exit_using(&fixture.child, |pid| {
                    calls += 1;
                    if calls == 1 {
                        Err(io::Error::from_raw_os_error(libc::EINTR))
                    } else {
                        Ok(pid as i32)
                    }
                })
                .unwrap()
            );
            assert_eq!(calls, 2);
            assert!(!observe_exit_using(&fixture.child, |_| Ok(0)).unwrap());
            assert_eq!(
                observe_exit_using(&fixture.child, |_| Err(io::Error::from_raw_os_error(
                    libc::ECHILD
                )))
                .unwrap_err()
                .raw_os_error(),
                Some(libc::ECHILD)
            );
            assert_eq!(
                observe_exit_using(&fixture.child, |_| Ok(-1))
                    .unwrap_err()
                    .kind(),
                ErrorKind::Other
            );
        });
    }

    #[test]
    fn a_descendant_cannot_keep_output_readers_alive() {
        watched(|| {
            // Background sh inherits stdout but gets its own stdin from a
            // FIFO. Its readiness is sent before the leader exits.
            let directory = tempfile::tempdir().unwrap();
            let fifo = directory.path().join("continue");
            nix::unistd::mkfifo(
                &fifo,
                nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
            )
            .unwrap();
            let script = format!(
                r#"/bin/sh -c 'printf r >&2; read gate < "$0"' '{}' & exit 0"#,
                fifo.display()
            );
            let mut fixture = Fixture::ready(&script);
            let error = fixture.drain(32).unwrap_err();
            assert_eq!(error.kind(), ErrorKind::TimedOut);
            assert!(
                observe_exit(&fixture.child).unwrap(),
                "leader exit was observed without reaping"
            );
            fixture.stop();
            fixture.assert_closed();
            assert_eq!(
                nix::sys::wait::waitpid(
                    nix::unistd::Pid::from_raw(fixture.child.id() as i32),
                    None
                ),
                Err(nix::errno::Errno::ECHILD)
            );
        });
    }

    #[test]
    fn failed_group_cleanup_never_claims_the_child_ended() {
        watched(|| {
            let mut fixture = Fixture::ready("printf r >&2; read gate");
            faults::inject([
                (
                    Operation::KillGroup,
                    io::Error::from_raw_os_error(libc::EPERM),
                ),
                (
                    Operation::KillChild,
                    io::Error::from_raw_os_error(libc::EPERM),
                ),
            ]);
            let cleanup = stop(&mut fixture.child).unwrap_err();
            assert_eq!(cleanup.pid, fixture.child.id());
            assert!(cleanup.group.is_some() && cleanup.child.is_some());
            assert!(!observe_exit(&fixture.child).unwrap());
            assert!(cleanup.to_string().contains("unknown"));
            assert!(!cleanup.to_string().contains("reaped"));
            fixture.stop();
        });
    }

    #[test]
    fn a_missing_group_still_requires_the_owned_leader_to_be_stopped() {
        watched(|| {
            let mut fixture = Fixture::ready("printf r >&2; read gate");
            faults::inject([(
                Operation::KillGroup,
                io::Error::from_raw_os_error(libc::ESRCH),
            )]);
            fixture.stop();
            assert_eq!(
                nix::sys::wait::waitpid(
                    nix::unistd::Pid::from_raw(fixture.child.id() as i32),
                    None
                ),
                Err(nix::errno::Errno::ECHILD),
            );
        });
    }

    #[test]
    fn failed_wait_retains_the_unreaped_child_identity() {
        watched(|| {
            let mut fixture = Fixture::ready("printf r >&2; read gate");
            faults::inject([(Operation::Wait, io::Error::from_raw_os_error(libc::EIO))]);
            let error = stop(&mut fixture.child).unwrap_err();
            assert_eq!(error.pid, fixture.child.id());
            assert_eq!(
                error.child.as_ref().unwrap().raw_os_error(),
                Some(libc::EIO)
            );
            assert!(error.to_string().contains("unknown"));
            fixture.child.wait().unwrap();
            fixture.reaped = true;
            assert_eq!(
                nix::sys::wait::waitpid(nix::unistd::Pid::from_raw(error.pid as i32), None),
                Err(nix::errno::Errno::ECHILD)
            );
        });
    }

    #[test]
    fn a_failed_final_wait_never_signals_a_possibly_reaped_group() {
        watched(|| {
            let signals = faults::signals();
            faults::inject([(Operation::Wait, io::Error::from_raw_os_error(libc::ECHILD))]);
            let error = output(&mut Command::new("/usr/bin/true"), WATCHDOG / 2).unwrap_err();
            assert_eq!(faults::signals(), signals);
            let failure = error
                .get_ref()
                .unwrap()
                .downcast_ref::<super::CleanupFailure>()
                .unwrap();
            assert_eq!(
                failure.child.as_ref().unwrap().raw_os_error(),
                Some(libc::ECHILD)
            );
            assert!(error.to_string().contains("unknown"));
            nix::sys::wait::waitpid(nix::unistd::Pid::from_raw(failure.pid as i32), None).unwrap();
            assert_output_reaped();
        });
    }

    #[test]
    fn primary_and_cleanup_errors_are_retained_with_safe_display() {
        watched(|| {
            faults::inject([
                (
                    Operation::Configure,
                    io::Error::new(ErrorKind::PermissionDenied, "private output canary"),
                ),
                (
                    Operation::KillGroup,
                    io::Error::new(ErrorKind::PermissionDenied, "private group canary"),
                ),
            ]);
            let error = output(
                Command::new("/bin/sh").args(["-c", "exec /bin/sleep 60"]),
                WATCHDOG / 2,
            )
            .unwrap_err();
            assert_eq!(error.kind(), ErrorKind::PermissionDenied);
            assert!(!error.to_string().contains("canary"));
            let failure = error
                .get_ref()
                .unwrap()
                .downcast_ref::<OutputFailure>()
                .unwrap();
            assert_eq!(failure.primary.kind(), ErrorKind::PermissionDenied);
            assert_eq!(
                failure.source().unwrap().to_string(),
                "private output canary"
            );
            assert_eq!(
                failure.cleanup.source().unwrap().to_string(),
                "private group canary"
            );
            assert!(failure.cleanup.child.is_none());
            assert_eq!(
                nix::sys::wait::waitpid(
                    nix::unistd::Pid::from_raw(failure.cleanup.pid as i32),
                    None
                ),
                Err(nix::errno::Errno::ECHILD)
            );
        });
    }

    #[test]
    fn a_timeout_with_failed_cleanup_is_not_a_settled_timeout() {
        watched(|| {
            faults::inject([
                (Operation::Configure, super::late()),
                (
                    Operation::KillGroup,
                    io::Error::from_raw_os_error(libc::EPERM),
                ),
                (Operation::Wait, io::Error::from_raw_os_error(libc::EIO)),
            ]);
            let error = output(
                Command::new("/bin/sh").args(["-c", "exec /bin/sleep 60"]),
                WATCHDOG / 2,
            )
            .unwrap_err();
            assert_eq!(error.kind(), ErrorKind::Other);
            let failure = error
                .get_ref()
                .unwrap()
                .downcast_ref::<OutputFailure>()
                .unwrap();
            assert_eq!(failure.primary.kind(), ErrorKind::TimedOut);
            assert_eq!(
                failure.cleanup.group.as_ref().unwrap().raw_os_error(),
                Some(libc::EPERM)
            );
            assert_eq!(
                failure.cleanup.child.as_ref().unwrap().raw_os_error(),
                Some(libc::EIO)
            );
            assert!(error.to_string().contains("unknown"));
            assert!(!error.to_string().contains("reaped"));
            // The injected wait failed after a real successful child kill;
            // recover that exact still-owned child before the test ends.
            nix::sys::wait::waitpid(nix::unistd::Pid::from_raw(failure.cleanup.pid as i32), None)
                .unwrap();
            assert_output_reaped();
        });
    }

    #[test]
    fn abandoning_output_still_settles_the_owned_child() {
        watched(|| {
            let (sender, receiver) = mpsc::channel();
            drop(receiver);
            let writer = thread::spawn(move || {
                let result = output_with_limit(
                    Command::new("/bin/sh").args(["-c", "exec /usr/bin/yes"]),
                    WATCHDOG / 2,
                    32,
                );
                assert_eq!(result.as_ref().unwrap_err().kind(), ErrorKind::InvalidData);
                assert_output_reaped();
                assert!(sender.send(result).is_err());
            });
            writer.join().unwrap();
        });
    }

    #[test]
    fn a_one_time_listing_may_take_longer_than_a_polled_lookup() {
        assert!(LISTING > LOOKUP * 5);
    }
}

#[cfg(test)]
mod faults {
    use std::{
        cell::{Cell, RefCell},
        collections::VecDeque,
        io,
    };

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) enum Operation {
        Configure,
        Read,
        Poll,
        KillGroup,
        KillChild,
        Wait,
    }

    thread_local! {
        static PENDING: RefCell<VecDeque<(Operation, io::Error)>> = const { RefCell::new(VecDeque::new()) };
        static CHILD: Cell<u32> = const { Cell::new(0) };
        static SIGNALS: Cell<usize> = const { Cell::new(0) };
    }

    pub(super) fn spawned(pid: u32) {
        CHILD.set(pid);
    }

    pub(super) fn child() -> u32 {
        CHILD.get()
    }

    pub(super) fn signalling() {
        SIGNALS.set(SIGNALS.get() + 1);
    }

    pub(super) fn signals() -> usize {
        SIGNALS.get()
    }

    pub(super) fn inject(errors: impl IntoIterator<Item = (Operation, io::Error)>) {
        PENDING.with(|pending| {
            let mut pending = pending.borrow_mut();
            assert!(pending.is_empty(), "all injected failures were exercised");
            pending.extend(errors);
        });
    }

    pub(super) fn check(operation: Operation) -> io::Result<()> {
        PENDING.with(|pending| {
            let mut pending = pending.borrow_mut();
            if pending.front().is_some_and(|(next, _)| *next == operation) {
                Err(pending.pop_front().unwrap().1)
            } else {
                Ok(())
            }
        })
    }
}
