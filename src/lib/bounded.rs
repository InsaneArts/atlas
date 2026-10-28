//! Commands that may take a set time at most. One still running then is
//! killed and reaped with every process it started, so a lookup that hangs
//! never keeps its process, or a thread waiting on it, alive.
use nix::{
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use std::{
    io::{self, Read},
    os::unix::process::CommandExt,
    process::{Child, Command, Output, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

/// How long one look at directory services or the process list may take.
pub(crate) const LOOKUP: Duration = Duration::from_secs(3);

/// How long a one-time listing during setup may take, such as every
/// account directory services know, which can be slow on a Mac bound to a
/// network directory. Nothing polls it.
pub(crate) const LISTING: Duration = Duration::from_secs(30);

/// How often a running command is checked on.
const POLL: Duration = Duration::from_millis(5);

/// How long output may keep arriving after its command ended.
const DRAIN: Duration = Duration::from_millis(500);

/// Run `command` with no input until it ends, for `limit` at most, with
/// what it wrote. It runs in a process group of its own; one still running
/// at the deadline is killed with that group and reaped, and that is a
/// `TimedOut` error, as is output that never finished.
pub(crate) fn output(command: &mut Command, limit: Duration) -> io::Result<Output> {
    let deadline = Instant::now() + limit;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()?;

    let readers = read(child.stdout.take())
        .and_then(|stdout| read(child.stderr.take()).map(|stderr| (stdout, stderr)));
    let (stdout, stderr) = match readers {
        Ok(readers) => readers,
        Err(error) => {
            stop(&mut child);
            return Err(error);
        }
    };

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL),
            Ok(None) => {
                stop(&mut child);
                return Err(late());
            }
            Err(error) => {
                stop(&mut child);
                return Err(error);
            }
        }
    };

    Ok(Output {
        status,
        stdout: stdout.recv_timeout(DRAIN).map_err(|_| late())?,
        stderr: stderr.recv_timeout(DRAIN).map_err(|_| late())?,
    })
}

/// Read `stream` to its end on a thread of its own; the bytes arrive once
/// it ends.
fn read(stream: Option<impl Read + Send + 'static>) -> io::Result<mpsc::Receiver<Vec<u8>>> {
    let (sender, receiver) = mpsc::channel();
    let Some(mut stream) = stream else {
        let _ = sender.send(Vec::new());
        return Ok(receiver);
    };

    std::thread::Builder::new()
        .name("atlas-output".to_owned())
        .spawn(move || {
            let mut bytes = Vec::new();
            if stream.read_to_end(&mut bytes).is_ok() {
                let _ = sender.send(bytes);
            }
        })?;
    Ok(receiver)
}

/// Kill `child` with its process group and reap it. A child that could not
/// be killed is reaped only if it already ended, so this never waits on it.
fn stop(child: &mut Child) {
    let group = i32::try_from(child.id()).ok().map(Pid::from_raw);
    let grouped = group.is_some_and(|group| killpg(group, Signal::SIGKILL).is_ok());
    if grouped || child.kill().is_ok() {
        let _ = child.wait();
    } else {
        let _ = child.try_wait();
    }
}

/// The error for a command, or its output, that did not finish in time.
fn late() -> io::Error {
    io::Error::new(
        io::ErrorKind::TimedOut,
        "The command did not finish in time",
    )
}

#[cfg(test)]
mod tests {
    use super::{LISTING, LOOKUP, output};
    use std::{
        io::ErrorKind,
        process::Command,
        time::{Duration, Instant},
    };

    #[test]
    fn a_command_that_ends_in_time_gives_its_output_and_status() {
        let output = output(
            Command::new("/bin/sh").args(["-c", "echo found; echo why >&2; exit 3"]),
            Duration::from_secs(5),
        )
        .unwrap();

        assert_eq!(output.stdout, b"found\n");
        assert_eq!(output.stderr, b"why\n");
        assert_eq!(output.status.code(), Some(3));
    }

    #[test]
    fn a_command_still_running_at_the_deadline_is_killed_and_reaped() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("pid");
        let started = Instant::now();

        let error = output(
            Command::new("/bin/sh")
                .arg("-c")
                .arg("echo $$ > \"$0\"; exec /bin/sleep 60")
                .arg(&marker),
            Duration::from_millis(200),
        )
        .unwrap_err();

        assert_eq!(error.kind(), ErrorKind::TimedOut);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "it gave up at the deadline"
        );
        let pid: i32 = std::fs::read_to_string(&marker)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_eq!(
            nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None),
            Err(nix::errno::Errno::ESRCH),
            "the sleep is gone"
        );
    }

    #[test]
    fn a_command_gets_no_input() {
        let output = output(&mut Command::new("/bin/cat"), Duration::from_secs(5)).unwrap();

        assert!(output.status.success());
        assert!(output.stdout.is_empty());
    }

    #[test]
    fn a_command_still_running_at_the_deadline_is_killed_with_what_it_started() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("pid");

        let error = output(
            Command::new("/bin/sh")
                .arg("-c")
                .arg("/bin/sleep 60 & echo $! > \"$0\"; wait")
                .arg(&marker),
            Duration::from_millis(500),
        )
        .unwrap_err();

        assert_eq!(error.kind(), ErrorKind::TimedOut);
        let pid: i32 = std::fs::read_to_string(&marker)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let pid = nix::unistd::Pid::from_raw(pid);
        let given_up = Instant::now() + Duration::from_secs(5);
        while nix::sys::signal::kill(pid, None) != Err(nix::errno::Errno::ESRCH) {
            assert!(Instant::now() < given_up, "the sleep it started is gone");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_one_time_listing_may_take_longer_than_a_polled_lookup() {
        assert!(LISTING > LOOKUP * 5);
    }
}
