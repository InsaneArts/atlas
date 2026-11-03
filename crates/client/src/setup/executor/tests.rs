use super::*;
#[test]
fn quiet_commands_hide_log_lines_and_keep_their_outcome() {
    use atlas_core::test_support::{TestChild, message, rerun};
    use std::process::Stdio;

    const NAME: &str =
        "setup::executor::tests::quiet_commands_hide_log_lines_and_keep_their_outcome";
    const LOG: &str = "2026-10-11 14:28:19.767 sysadminctl[1:2] noise\n";
    const PROMPT: &str = "User password:";
    const OUTPUT: &str = "command output";
    if let Ok(status) = std::env::var("ATLAS_QUIET_COMMAND_FIXTURE") {
        assert!(matches!(status.as_str(), "0" | "3"));
        // The fixture asks no real password. It cannot finish its partial line
        // until the parent observes the actual prompt flush and sends a permit.
        let script = "printf '%s' \"$1\" >&2; printf '%s' \"$2\" >&2; \
                          read -r -t 10 permit && test \"$permit\" = continue || exit 9; \
                          printf '\\n' >&2; printf '%s' \"$3\"; exit \"$4\"";
        let child = Command::new("/bin/sh")
            .args(["-c", script, "quiet-fixture", LOG, PROMPT, OUTPUT, &status])
            .stdin(Stdio::inherit())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        message("quiet command started");
        let result = quietly_attended(child, "it failed");
        if status == "0" {
            assert!(result.is_ok());
        } else {
            assert_eq!(result.unwrap_err().error, "it failed");
        }
        message("quiet command ended");
        return;
    }

    for status in ["0", "3"] {
        let mut child = TestChild::spawn(
            rerun(NAME)
                .arg("--nocapture")
                .env("ATLAS_QUIET_COMMAND_FIXTURE", status),
        );
        let mut receiver = forward(child.process.stderr.take().unwrap());
        child.expect("quiet command started");
        let mut seen = Vec::new();
        while seen.len() < PROMPT.len() {
            match receiver.next(Duration::from_secs(10)) {
                Said::Bytes(bytes) => seen.extend(bytes),
                said => panic!("the partial prompt never arrived: {said:?}"),
            }
        }
        assert_eq!(
            seen,
            PROMPT.as_bytes(),
            "only the partial prompt is flushed"
        );
        child.permit("continue");
        child.expect("quiet command ended");
        assert!(child.wait().success());
        loop {
            match receiver.next(Duration::from_secs(10)) {
                Said::Bytes(bytes) => seen.extend(bytes),
                Said::Ended => break,
                said => panic!("the owned stderr reader did not finish: {said:?}"),
            }
        }
        let expected = if status == "0" {
            format!("{PROMPT}\n")
        } else {
            format!("{PROMPT}\n{LOG}{OUTPUT}")
        };
        assert_eq!(seen, expected.as_bytes(), "status {status}");
    }
}

type Steps = std::collections::VecDeque<std::io::Result<&'static [u8]>>;

/// Say the next of `steps`, or end once there are none.
fn step(steps: &mut Steps, chunk: &mut [u8]) -> std::io::Result<Option<usize>> {
    match steps.pop_front() {
        Some(Ok(bytes)) => {
            chunk[..bytes.len()].copy_from_slice(bytes);
            Ok(Some(bytes.len()))
        }
        Some(Err(error)) => Err(error),
        None => Ok(Some(0)),
    }
}

/// A stream that says each of `steps` in turn, then ends.
fn scripted(steps: Vec<std::io::Result<&'static [u8]>>) -> Forward {
    let mut steps = Steps::from(steps);
    Forward::with(move |chunk| step(&mut steps, chunk))
}

/// Input whose reader has gone, as a step that stopped early leaves it.
struct Gone;

impl Write for Gone {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::ErrorKind::BrokenPipe.into())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Err(std::io::ErrorKind::BrokenPipe.into())
    }
}

#[test]
fn a_read_failure_after_an_executor_marker_is_not_a_completed_reply() {
    const REFUSED: &[u8] = b"@atlas error refused\n";
    let lost = || Err(std::io::Error::other("the channel failed"));
    let interrupted = || Err(std::io::ErrorKind::Interrupted.into());
    let deadline = || Instant::now() + Duration::from_secs(10);

    let mut seen = Vec::new();
    let answered = answer(
        &mut Vec::new(),
        &mut scripted(vec![Ok(REFUSED), lost()]),
        b"",
        0,
        deadline(),
        &mut seen,
    );
    assert_eq!(seen, REFUSED);
    assert!(
        matches!(answered, Err(Unanswered::Unread(_))),
        "a read failure is not the end of output: {answered:?}"
    );

    let (finished, said) = converse(
        Vec::new(),
        scripted(vec![Ok(REFUSED), lost()]),
        Some(b"secret"),
        deadline(),
    );
    assert!(!finished);
    assert!(said.is_err(), "a marker never masks a read failure");

    let (finished, said) = converse(
        Vec::new(),
        scripted(vec![interrupted(), Ok(REFUSED), interrupted()]),
        Some(b"secret"),
        deadline(),
    );
    assert!(finished, "interrupted reads are retried");
    assert_eq!(said.unwrap().as_bytes(), REFUSED);

    // A step that refused before its password went in closed its channel;
    // its own line still says why.
    let (finished, said) = converse(
        Gone,
        scripted(vec![Ok(b"@atlas error refused\nUser password:")]),
        Some(b"secret"),
        deadline(),
    );
    assert!(!finished);
    assert_eq!(said.unwrap(), "@atlas error refused\nUser password:");
}

#[test]
fn an_executor_failure_includes_its_final_error_output() {
    const SAID: &[u8] = b"sudo: a se cret diagnostic\n";
    let endings: [(&str, Steps); 3] = [
        ("a clean end", Steps::new()),
        (
            "interrupted reads",
            Steps::from([
                Err(std::io::ErrorKind::Interrupted.into()),
                Err(std::io::ErrorKind::Interrupted.into()),
            ]),
        ),
        (
            "a failed read",
            Steps::from([Err(std::io::Error::other("the pipe failed"))]),
        ),
    ];
    for (case, mut rest) in endings {
        // The diagnostic is held until the report waits at its receive.
        let (release, held) = std::sync::mpsc::channel();
        let (waiting, waits) = std::sync::mpsc::channel();
        let releaser = std::thread::spawn(move || {
            let waited = waits.recv().is_ok();
            if waited {
                release.send(()).unwrap();
            }
            waited
        });
        rest.push_front(Ok(SAID));
        let mut released = false;
        let mut complaints = Forward::with(move |chunk| {
            if !released {
                if held.recv_timeout(LOOK).is_err() {
                    return Ok(None);
                }
                released = true;
            }
            step(&mut rest, chunk)
        });
        complaints.waiting = Some(waiting);
        let said = summary(
            "",
            &mut complaints,
            b"se cret",
            Instant::now() + Duration::from_secs(10),
        );
        // A report that never waited drops the only way to release it.
        drop(complaints);
        assert!(releaser.join().unwrap(), "the report waits: {case}");
        assert_eq!(said, "sudo: a  diagnostic", "{case}");
    }

    // A program's own last words need no error output.
    let mut silent = Forward::with(|_| Ok(None));
    let said = summary(
        "sysadminctl: refused\n",
        &mut silent,
        b"",
        Instant::now() + Duration::from_secs(10),
    );
    assert_eq!(said, "sysadminctl: refused");

    // Error output that never ends is read only until the deadline.
    let mut endless = Forward::with(|_| Ok(None));
    assert_eq!(summary("", &mut endless, b"", Instant::now()), "");
}

/// Ask for `rights` with `consent`, renewing as worker `id` on each of
/// `ticks` until the renewal is stopped.
fn ticked(
    rights: &mut AdministratorRights,
    consent: std::io::Result<bool>,
    id: usize,
    ticks: std::sync::mpsc::Receiver<()>,
    mut renew: impl FnMut(usize) + Send + 'static,
) -> AtlasResult<()> {
    use std::sync::mpsc::{RecvTimeoutError, TryRecvError};
    rights.request_with(
        || consent,
        move || renew(id),
        move |stop| loop {
            if stop.try_recv() != Err(TryRecvError::Empty) {
                return false;
            }
            match ticks.recv_timeout(Duration::from_millis(10)) {
                Ok(()) => return true,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return false,
            }
        },
    )
}

#[test]
fn administrator_renewal_ends_with_its_attended_setup_scope() {
    use std::sync::mpsc::{TryRecvError, channel};

    // One renewal per tick, by the first worker however often consent is asked.
    let (tick, ticks) = channel();
    let (renewing, renewed) = channel();
    let mut rights = AdministratorRights::default();
    let again = renewing.clone();
    ticked(&mut rights, Ok(true), 1, ticks, move |id| {
        renewing.send(id).unwrap();
    })
    .unwrap();
    let (_unused, never) = channel();
    ticked(&mut rights, Ok(true), 2, never, move |id| {
        again.send(id).unwrap();
    })
    .unwrap();
    for _ in 0..3 {
        tick.send(()).unwrap();
        assert_eq!(renewed.recv().unwrap(), 1);
    }
    drop(rights);
    assert!(tick.send(()).is_err(), "the renewal stopped with its setup");
    assert_eq!(renewed.try_recv(), Err(TryRecvError::Disconnected));

    // Ending the setup waits for a refresh already running.
    let (tick, ticks) = channel();
    let (entering, entered) = channel();
    let (release, held) = channel::<()>();
    let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let marked = std::sync::Arc::clone(&finished);
    let mut rights = AdministratorRights::default();
    ticked(&mut rights, Ok(true), 1, ticks, move |_| {
        entering.send(()).unwrap();
        held.recv().unwrap();
        marked.store(true, std::sync::atomic::Ordering::SeqCst);
    })
    .unwrap();
    tick.send(()).unwrap();
    entered.recv().unwrap();
    let (dropping, dropped) = channel();
    let ending = std::thread::spawn(move || {
        drop(rights);
        dropping.send(()).unwrap();
    });
    assert!(
        dropped.recv_timeout(Duration::from_millis(200)).is_err(),
        "ending the setup waits for the refresh in flight"
    );
    release.send(()).unwrap();
    dropped.recv().unwrap();
    ending.join().unwrap();
    assert!(finished.load(std::sync::atomic::Ordering::SeqCst));

    // A refused or failed consent starts no renewal.
    for consent in [Ok(false), Err(std::io::Error::other("no terminal"))] {
        let (tick, ticks) = channel();
        let mut rights = AdministratorRights::default();
        assert!(ticked(&mut rights, consent, 1, ticks, |_| panic!("renewed")).is_err());
        assert!(rights.renewal.is_none());
        assert!(tick.send(()).is_err());
    }

    // A failed renewal, or a setup that unwinds, still ends it.
    let (tick, ticks) = channel();
    let mut rights = AdministratorRights::default();
    ticked(&mut rights, Ok(true), 1, ticks, |_| {
        panic!("the refresh failed")
    })
    .unwrap();
    tick.send(()).unwrap();
    drop(rights);
    assert!(tick.send(()).is_err());
    let (tick, ticks) = channel();
    let (renewing, renewed) = channel();
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut rights = AdministratorRights::default();
        ticked(&mut rights, Ok(true), 1, ticks, move |id| {
            renewing.send(id).unwrap();
        })
        .unwrap();
        tick.send(()).unwrap();
        renewed.recv().unwrap();
        panic!("setup unwound");
    }));
    assert!(unwound.is_err());
    assert!(tick.send(()).is_err());
    assert_eq!(renewed.try_recv(), Err(TryRecvError::Disconnected));
}
