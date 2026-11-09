//! Time-limited commands with owned process groups and complete output.
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
pub const LOOKUP: Duration = Duration::from_secs(3);

/// How long a one-time listing during setup may take, such as every
/// account directory services know, which can be slow on a Mac bound to a
/// network directory. Nothing polls it.
pub const LISTING: Duration = Duration::from_secs(30);

/// How long a system tool may take over one file or folder it does not
/// copy: reading an Info.plist or a folder's access list, or changing one
/// folder's access.
pub const FILE: Duration = Duration::from_secs(10);

/// How long copying an app or a file for the helper may take: long enough
/// for the largest app, short enough that a copy stuck on a disk that
/// stopped answering ends.
pub const COPY: Duration = Duration::from_secs(10 * 60);

/// How often a running command is checked on.
const POLL: Duration = Duration::from_millis(5);

/// How long output may keep arriving after its command ended.
const DRAIN: Duration = Duration::from_millis(500);

/// Run with no input in an isolated process group. Timeouts and I/O
/// failures stop the group and reap the child; a cleanup
/// failure retains both causes and reports which termination is unknown.
///
/// # Errors
///
/// Returns the I/O error of starting or waiting for the command, or a timeout.
pub fn output(command: &mut Command, limit: Duration) -> io::Result<Output> {
    // Complete directory listings have no byte ceiling. A completed large
    // network-directory response must still participate in UID selection.
    output_with_limit(command, limit, usize::MAX)
}

/// Run an owned command with a byte ceiling for each stream. Overflow fails the
/// complete observation; no partial output is returned as a successful result.
pub(crate) fn output_with_limit(
    command: &mut Command,
    limit: Duration,
    cap: usize,
) -> io::Result<Output> {
    let deadline = Instant::now() + limit;
    let mut child = crate::spawn::spawn(
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0),
    )?;
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
        Err(primary) => return Err(stopped_after(&mut child, primary)),
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

/// Stop `child` after the failure `primary`, which is what the run comes to.
fn stopped_after(child: &mut Child, primary: io::Error) -> io::Error {
    match stop(child) {
        Ok(()) => primary,
        // A timeout with failed cleanup is not settled: callers may remove
        // staging on TimedOut, so retain it as an unknown I/O outcome with
        // the original kind still in the private cause.
        Err(cleanup) => {
            let kind = match primary.kind() {
                io::ErrorKind::TimedOut => io::ErrorKind::Other,
                kind => kind,
            };
            io::Error::new(kind, OutputFailure { primary, cleanup })
        }
    }
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
        match read_some(fd, &mut chunk[..count]) {
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

fn read_some(fd: &OwnedFd, buffer: &mut [u8]) -> io::Result<usize> {
    #[cfg(test)]
    faults::check(faults::Operation::Read)?;
    nix::unistd::read(fd, buffer).map_err(Into::into)
}

fn drain(
    child: &Child,
    stdout: &mut Pipe,
    stderr: &mut Pipe,
    deadline: Instant,
    cap: usize,
) -> io::Result<()> {
    drain_by(child, stdout, stderr, deadline, cap, Instant::now)
}

/// [`drain`], telling the time by `clock`.
fn drain_by(
    child: &Child,
    stdout: &mut Pipe,
    stderr: &mut Pipe,
    deadline: Instant,
    cap: usize,
    mut clock: impl FnMut() -> Instant,
) -> io::Result<()> {
    let mut next_observation = clock();
    let mut exited = None;
    let mut stderr_deadline = None;
    loop {
        let now = clock();
        if exited.is_none() && now >= next_observation {
            if observe_exit(child)? {
                exited = Some(now);
            } else if now >= deadline {
                return Err(late());
            }
            next_observation = clock() + POLL;
        }
        let delay = if let Some(ended) = exited {
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
                // What the pipes already hold when the window ends came in
                // time, however late this thread was to look.
                return settled(stdout, stderr, cap);
            }
            until.saturating_duration_since(now).min(POLL)
        } else {
            next_observation
                .min(deadline)
                .saturating_duration_since(now)
        };
        wait(stdout, stderr, delay)?;
        stdout.read(cap)?;
        stderr.read(cap)?;
    }
}

/// The most chunks each pipe gives in [`settled`]: more than a pipe
/// buffers, so a writer that keeps writing cannot hold the read open.
const SETTLING_READS: usize = 128;

/// Read what both pipes hold without waiting, ending well once both are
/// at their end and late otherwise.
fn settled(stdout: &mut Pipe, stderr: &mut Pipe, cap: usize) -> io::Result<()> {
    for pipe in [&mut *stdout, &mut *stderr] {
        for _ in 0..SETTLING_READS {
            let held = pipe.bytes.len();
            pipe.read(cap)?;
            if pipe.fd.is_none() || pipe.bytes.len() == held {
                break;
            }
        }
    }
    if stdout.fd.is_none() && stderr.fd.is_none() {
        Ok(())
    } else {
        Err(late())
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
    match poll_pipes(stdout, stderr, timeout) {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::Interrupted => Ok(()),
        Err(error) => Err(error),
    }
}

fn poll_pipes(
    stdout: Option<PollFd<'_>>,
    stderr: Option<PollFd<'_>>,
    timeout: u16,
) -> io::Result<i32> {
    #[cfg(test)]
    faults::check(faults::Operation::Poll)?;
    match (stdout, stderr) {
        (Some(out), Some(err)) => poll(&mut [out, err], timeout),
        (Some(fd), None) | (None, Some(fd)) => poll(&mut [fd], timeout),
        (None, None) => poll(&mut [], timeout),
    }
    .map_err(Into::into)
}

fn observe_exit(child: &Child) -> io::Result<bool> {
    observe_exit_using(child, |pid| {
        crate::process::child_exited(pid).map(|exited| if exited { pid as i32 } else { 0 })
    })
}

fn observe_exit_using(
    child: &Child,
    mut observe: impl FnMut(u32) -> io::Result<i32>,
) -> io::Result<bool> {
    let pid = command_pid(child.id())?.as_raw();
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
    killpg(command_pid(pid)?, Signal::SIGKILL).map_err(Into::into)
}

fn command_pid(pid: u32) -> io::Result<Pid> {
    i32::try_from(pid)
        .ok()
        .filter(|pid| *pid > 0)
        .map(Pid::from_raw)
        .ok_or_else(|| io::Error::other("Invalid command process identity"))
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
mod tests;

#[cfg(test)]
pub(crate) mod faults {
    use std::{
        cell::{Cell, RefCell},
        collections::VecDeque,
        io,
    };

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum Operation {
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

    pub(crate) fn child() -> u32 {
        CHILD.get()
    }

    pub(super) fn signalling() {
        SIGNALS.set(SIGNALS.get() + 1);
    }

    pub(super) fn signals() -> usize {
        SIGNALS.get()
    }

    pub(crate) fn inject(errors: impl IntoIterator<Item = (Operation, io::Error)>) {
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
