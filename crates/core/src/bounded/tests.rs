use super::{
    LISTING, LOOKUP, OutputFailure, Pipe, drain, faults, observe_exit, observe_exit_using, output,
    output_with_limit, stop,
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

#[test]
fn complete_output_larger_than_eight_mebibytes_is_still_accepted() -> io::Result<()> {
    watched(|| {
        let output = output(
            Command::new("/bin/dd").args(["if=/dev/zero", "bs=1048576", "count=9"]),
            WATCHDOG / 2,
        )?;
        assert!(output.status.success());
        assert_eq!(output.stdout.len(), 9 * 1024 * 1024);
        assert!(output.stdout.iter().all(|byte| *byte == 0));
        assert_output_reaped();
        Ok(())
    })
}

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
        let mut child = crate::spawn::spawn(
            Command::new("/bin/sh")
                .args(["-c", script])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .process_group(0),
        )
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

    /// Stop the group and reap the leader. macOS will not signal a group
    /// whose one member is its exited, unreaped leader, so a leader that
    /// ended first leaves that refusal with the leader itself reaped.
    fn stop(&mut self) {
        match stop(&mut self.child) {
            Ok(()) => {}
            Err(cleanup)
                if cleanup.child.is_none()
                    && cleanup.group.as_ref().and_then(io::Error::raw_os_error)
                        == Some(libc::EPERM) => {}
            Err(cleanup) => panic!("the group was not stopped: {cleanup:?}"),
        }
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
        let mut fixture = Fixture::ready("/bin/sh -c 'printf r >&2; exec /bin/sleep 60' & wait");
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
            nix::sys::wait::waitpid(nix::unistd::Pid::from_raw(fixture.child.id() as i32), None),
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
                let script =
                    format!("printf r >&2; read gate; /usr/bin/printf '%0{length}d'{redirect} 0");
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

/// A stream that has all of `bytes` and then its end, read without
/// waiting. A file, not a pipe: another test's child can inherit a new
/// pipe's writer before it is marked to close on exec, and hold it open.
fn ended(bytes: &[u8]) -> Pipe {
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(bytes).unwrap();
    std::io::Seek::rewind(&mut file).unwrap();
    let stream = Pipe::new(Some(OwnedFd::from(file)));
    stream.configure().unwrap();
    stream
}

/// A pipe holding `bytes` whose writer stays open while it is kept.
fn open(bytes: &[u8]) -> (Pipe, io::PipeWriter) {
    let (reader, mut writer) = crate::spawn::pipe().unwrap();
    writer.write_all(bytes).unwrap();
    let pipe = Pipe::new(Some(OwnedFd::from(reader)));
    pipe.configure().unwrap();
    (pipe, writer)
}

#[test]
fn a_drain_that_looks_again_only_after_its_window_ended_still_takes_complete_output() {
    watched(|| {
        let mut fixture = Fixture::ready("printf r >&2; read gate");
        fixture.proceed();
        // Waits on the child's exit itself; the watchdog bounds it.
        while !observe_exit(&fixture.child).unwrap() {
            thread::yield_now();
        }
        // The output is complete before the drain starts: more than one
        // read's chunk, then the end. Files, since another test's child
        // can hold a new pipe's writer open (see `ended`).
        let (mut stdout, mut stderr) = (ended(&[0; 12_000]), ended(b""));
        // The exit is seen at the first look and one chunk read; every
        // later look is past the window, as for a thread descheduled then.
        let started = Instant::now();
        let mut looks = 0;
        let clock = || {
            looks += 1;
            if looks <= 2 {
                started
            } else {
                started + super::DRAIN + Duration::from_millis(1)
            }
        };
        super::drain_by(
            &fixture.child,
            &mut stdout,
            &mut stderr,
            started + WATCHDOG,
            usize::MAX,
            clock,
        )
        .unwrap();
        assert_eq!(stdout.bytes, vec![0; 12_000]);
        fixture.assert_reaped();
    });
}

#[test]
fn output_already_complete_when_the_drain_window_ends_is_still_accepted() {
    let (mut stdout, mut stderr) = (ended(&[7; 40_000]), ended(b"warning"));
    super::settled(&mut stdout, &mut stderr, usize::MAX).unwrap();
    assert_eq!(stdout.bytes, vec![7; 40_000]);
    assert_eq!(stderr.bytes, b"warning");
    assert!(stdout.fd.is_none() && stderr.fd.is_none());
}

#[test]
fn a_stream_still_open_when_the_drain_window_ends_is_late() {
    let (mut stdout, _writer) = open(b"done");
    let error = super::settled(&mut stdout, &mut ended(b""), usize::MAX).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::TimedOut);
    assert_eq!(stdout.bytes, b"done");

    let (mut stderr, _writer) = open(b"");
    let mut stdout = ended(b"done");
    let error = super::settled(&mut stdout, &mut stderr, usize::MAX).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::TimedOut);
    assert_eq!(stdout.bytes, b"done");
}

#[test]
fn a_writer_that_keeps_writing_cannot_hold_the_last_read_open() {
    watched(|| {
        let mut fixture = Fixture::ready("printf r >&2; read gate; exec /usr/bin/yes");
        fixture.proceed();
        let error =
            super::settled(&mut fixture.stdout, &mut fixture.stderr, usize::MAX).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::TimedOut);
        assert!(fixture.stdout.bytes.len() <= super::SETTLING_READS * 8192);
    });
}

#[test]
fn the_last_read_still_refuses_output_over_the_byte_cap() {
    let error = super::settled(&mut ended(&[1; 64]), &mut ended(b""), 16).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidData);
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
            nix::sys::wait::waitpid(nix::unistd::Pid::from_raw(fixture.child.id() as i32), None),
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
            nix::sys::wait::waitpid(nix::unistd::Pid::from_raw(fixture.child.id() as i32), None),
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
            nix::sys::wait::waitpid(nix::unistd::Pid::from_raw(fixture.child.id() as i32), None),
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
            nix::sys::wait::waitpid(nix::unistd::Pid::from_raw(failure.cleanup.pid as i32), None),
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
