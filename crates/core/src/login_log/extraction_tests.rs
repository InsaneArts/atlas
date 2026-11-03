use super::*;
use crate::bounded::faults::{self, Operation};
use std::{error::Error as _, fs, io, process::Stdio, sync::mpsc, thread};

const CANARY: &str = "private-os-log-extraction-canary";
const WATCHDOG: Duration = Duration::from_secs(20);
const LINES: &str = concat!(
    "{\"processImagePath\":\"/loginwindow\",\"processID\":7,\"timestamp\":\"2026-10-23 20:50:04.000000+0400\",\"eventMessage\":\"login state: ReturnFromAuthCopy\"}\n",
    "{\"processImagePath\":\"/loginwindow\",\"processID\":7,\"timestamp\":\"2026-10-23 20:50:04.100000+0400\",\"eventMessage\":\"login state: CreateAndSwitch\"}\n",
    "{\"subsystem\":\"com.insanearts.atlas\",\"timestamp\":\"2026-10-23 20:50:05.500000+0400\",\"eventMessage\":\"opener.desktop é🙂 setupAssistant=false\"}\n",
);

// Failure interrupts the test process rather than detaching a blocked query.
// This is an outer safety bound, never a permit or an elapsed-time assertion.
fn watched(run: impl FnOnce()) {
    let (finished, waiting) = mpsc::channel();
    let watchdog = thread::spawn(move || {
        if waiting.recv_timeout(WATCHDOG) == Err(mpsc::RecvTimeoutError::Timeout) {
            std::process::abort();
        }
    });
    run();
    finished.send(()).unwrap();
    watchdog.join().unwrap();
}

fn query(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", script]).stdin(Stdio::null());
    command
}

fn assert_reaped() {
    assert_eq!(
        nix::sys::wait::waitpid(nix::unistd::Pid::from_raw(faults::child() as i32), None),
        Err(nix::errno::Errno::ECHILD),
        "the owned query child must already be reaped",
    );
}

fn assert_private(error: &Error) {
    assert_eq!(error.code, Code::Io);
    assert!(!serde_json::to_string(error).unwrap().contains(CANARY));
    assert!(!error.to_string().contains(CANARY));
    assert!(!format!("{error:?}").contains(CANARY));
    assert!(!error.uncertain, "a log query has no helper input effects");
}

#[test]
fn login_log_extraction_is_bounded_before_parsing() {
    watched(|| {
        assert_eq!(OUTPUT_LIMIT, 8 * 1024 * 1024);
        assert_eq!(QUERY_LIMIT, Duration::from_secs(30));
        let folder = tempfile::tempdir().unwrap();
        let log = folder.path().join("events.ndjson");
        fs::write(&log, LINES).unwrap();
        let expected = summarize(&LINES.lines().filter_map(event).collect::<Vec<_>>());
        assert_eq!(expected["found"], true);
        assert_eq!(expected["steps"]["passwordToDesktop"], 1.5);
        assert!(
            expected["markers"][0]["marker"]
                .as_str()
                .unwrap()
                .contains("é🙂")
        );
        for stream in ["stdout", "stderr"] {
            for size in [OUTPUT_LIMIT - 1, OUTPUT_LIMIT, OUTPUT_LIMIT + 1] {
                let mut bytes = if stream == "stdout" {
                    LINES.as_bytes().to_vec()
                } else {
                    CANARY.as_bytes().to_vec()
                };
                bytes.resize(size, b'\n');
                let path = folder.path().join("stream");
                fs::write(&path, bytes).unwrap();
                let script = if stream == "stdout" {
                    "exec /bin/cat \"$1\""
                } else {
                    "/bin/cat \"$2\"; exec /bin/cat \"$1\" >&2"
                };
                let result = extract(
                    query(script).args([
                        "log-fixture",
                        path.to_str().unwrap(),
                        log.to_str().unwrap(),
                    ]),
                    WATCHDOG / 2,
                    OUTPUT_LIMIT,
                );
                if size <= OUTPUT_LIMIT {
                    assert_eq!(result.unwrap(), expected, "{stream} {size}");
                } else {
                    let error = result.unwrap_err();
                    assert_private(&error);
                    assert_eq!(
                        error
                            .source()
                            .unwrap()
                            .downcast_ref::<io::Error>()
                            .unwrap()
                            .kind(),
                        io::ErrorKind::InvalidData
                    );
                }
                assert_reaped();
            }
        }
    });
}

#[test]
fn failed_log_extraction_never_returns_a_partial_timeline() {
    watched(|| {
        let folder = tempfile::tempdir().unwrap();
        let log = folder.path().join("events.ndjson");
        fs::write(&log, LINES).unwrap();
        let error = extract(
            query("/bin/cat \"$1\"; printf '%s' \"$2\" >&2; exit 3").args([
                "log-fixture",
                log.to_str().unwrap(),
                CANARY,
            ]),
            WATCHDOG / 2,
            OUTPUT_LIMIT,
        )
        .unwrap_err();
        assert_private(&error);
        assert!(
            error
                .source()
                .unwrap()
                .to_string()
                .contains("exit status: 3")
        );
        assert_reaped();

        // The query prints a valid timeline before exceeding its byte cap.
        let error = extract(
            query("/bin/cat \"$1\"; exec /usr/bin/yes \"$2\"").args([
                "log-fixture",
                log.to_str().unwrap(),
                CANARY,
            ]),
            WATCHDOG / 2,
            LINES.len() + 10,
        )
        .unwrap_err();
        assert_private(&error);
        assert_eq!(
            error
                .source()
                .unwrap()
                .downcast_ref::<io::Error>()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert_reaped();
    });
}

#[test]
fn log_query_io_failures_retain_their_cause_after_reaping() {
    watched(|| {
        for operation in [Operation::Configure, Operation::Read, Operation::Poll] {
            faults::inject([(operation, io::Error::other(CANARY))]);
            let error = extract(&mut query("printf '{}'"), WATCHDOG / 2, OUTPUT_LIMIT).unwrap_err();
            assert_private(&error);
            let mut cause = error.source();
            let mut retained = false;
            while let Some(current) = cause {
                retained |= current.to_string() == CANARY;
                cause = current.source();
            }
            assert!(
                retained,
                "retain the private primary cause even if cleanup also fails"
            );
            assert_reaped();
        }
    });
}

#[test]
fn an_expired_log_query_is_stopped_and_reaped() {
    watched(|| {
        // This held real child is a workload, not a sequencing sleep. A zero
        // command budget exercises expiry without waiting thirty wall seconds.
        let error = extract(
            &mut query("exec /bin/sleep 60"),
            Duration::ZERO,
            OUTPUT_LIMIT,
        )
        .unwrap_err();
        assert_private(&error);
        assert_eq!(
            error
                .source()
                .unwrap()
                .downcast_ref::<io::Error>()
                .unwrap()
                .kind(),
            io::ErrorKind::TimedOut
        );
        assert_reaped();
    });
}

#[test]
fn a_descendant_cannot_turn_an_incomplete_log_query_into_success() {
    watched(|| {
        let folder = tempfile::tempdir().unwrap();
        let fifo = folder.path().join("ready");
        nix::unistd::mkfifo(
            &fifo,
            nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
        )
        .unwrap();
        // The child retains both output pipes. Its FIFO message lets the leader
        // exit only after that child is live; sleep is a bounded workload.
        let script = "/bin/sh -c 'printf \"ready\\n\" > \"$1\"; exec /bin/sleep 10' descendant \"$1\" & read -r ready < \"$1\"; test \"$ready\" = ready || exit 3; exit 0";
        let error = extract(
            query(script).args(["log-fixture", fifo.to_str().unwrap()]),
            WATCHDOG / 2,
            OUTPUT_LIMIT,
        )
        .unwrap_err();
        assert_private(&error);
        assert_eq!(
            error
                .source()
                .unwrap()
                .downcast_ref::<io::Error>()
                .unwrap()
                .kind(),
            io::ErrorKind::TimedOut
        );
        assert_reaped();
    });
}

#[test]
fn unconfirmed_log_query_cleanup_keeps_both_failure_causes() {
    watched(|| {
        faults::inject([
            (Operation::Configure, io::Error::other(CANARY)),
            (
                Operation::KillGroup,
                io::Error::from_raw_os_error(libc::EPERM),
            ),
            (Operation::Wait, io::Error::from_raw_os_error(libc::EIO)),
        ]);
        // No descendants: the fallback real kill ends the child, but the injected
        // wait failure deliberately retains its unreaped identity for this test.
        let error =
            extract(&mut query("exec /bin/sleep 60"), WATCHDOG / 2, OUTPUT_LIMIT).unwrap_err();
        let pid = faults::child();
        let recovered = nix::sys::wait::waitpid(nix::unistd::Pid::from_raw(pid as i32), None);
        recovered.unwrap();
        assert_private(&error);
        let source = error.source().unwrap().downcast_ref::<io::Error>().unwrap();
        assert!(source.to_string().contains("unknown"));
        let mut cause = source.source();
        let mut retained = false;
        while let Some(current) = cause {
            retained |= current.to_string() == CANARY;
            cause = current.source();
        }
        assert!(retained, "the primary I/O error survives failed cleanup");
        assert_reaped();
    });
}
