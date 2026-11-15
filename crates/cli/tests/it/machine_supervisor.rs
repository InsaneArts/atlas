//! The hidden command a machine desktop's daemon starts its work under,
//! started as the daemon starts it.
#![cfg(target_os = "macos")]

use atlas_daemon::machine::supervise::{
    RENEW,
    invocation::{self, Invocation, Supervisor},
};
use std::{
    ffi::OsString,
    io::{Read, Write},
    os::{fd::OwnedFd, unix::net::UnixStream, unix::process::CommandExt},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

/// `atlas machine-supervise` running `words`, its output read here, with
/// the daemon's end of its lifeline. Its output is its standard error.
fn supervising(words: &[&str]) -> (Child, UnixStream, std::io::PipeReader) {
    let proofs = tempfile::tempdir().unwrap().keep();
    let invocation = Invocation {
        lease: Duration::from_secs(2),
        proofs,
        argv0: words[0].into(),
        program: words[0].into(),
        arguments: words[1..].iter().map(OsString::from).collect(),
    };
    let supervisor = Supervisor::new(
        env!("CARGO_BIN_EXE_atlas").into(),
        vec!["machine-supervise".into(), "--".into()],
    );
    let (lifeline, theirs) = atlas_core::spawn::socket_pair().unwrap();
    let (output, writing) = atlas_core::spawn::pipe().unwrap();
    let mut command = Command::new(supervisor.program());
    command
        .args(supervisor.arguments(&invocation))
        .stdin(Stdio::from(OwnedFd::from(theirs)))
        .stdout(Stdio::null())
        .stderr(Stdio::from(OwnedFd::from(writing)))
        .env(
            atlas_daemon::worker::TEST_DAEMON,
            std::process::id().to_string(),
        )
        .process_group(0);
    let child = atlas_core::spawn::spawn(&mut command).unwrap();
    (child, lifeline, output)
}

/// The frame the supervisor says the program's end in.
fn frame(lifeline: &mut UnixStream) -> Vec<u8> {
    let mut frame = Vec::new();
    let mut byte = [0; 1];
    while frame.len() < invocation::FRAME && lifeline.read(&mut byte).unwrap() == 1 {
        frame.push(byte[0]);
        if byte[0] == b'\n' {
            break;
        }
    }
    frame
}

#[test]
fn the_supervisor_command_runs_a_program_and_says_how_it_ended() {
    let (mut child, mut lifeline, mut output) = supervising(&["/bin/echo", "said"]);
    lifeline.write_all(&[RENEW]).unwrap();

    assert_eq!(frame(&mut lifeline), invocation::STARTED);
    let told = invocation::heard(&frame(&mut lifeline));

    assert_eq!(told.and_then(|status| status.code()), Some(0));
    let mut said = String::new();
    output.read_to_string(&mut said).unwrap();
    assert_eq!(said, "said\n");
    assert_eq!(child.wait().unwrap().code(), Some(0));
}

#[test]
fn the_supervisor_command_ends_its_work_once_its_lifeline_closes() {
    let (mut child, mut lifeline, _output) = supervising(&["/bin/sleep", "30"]);
    lifeline.write_all(&[RENEW]).unwrap();
    std::thread::sleep(Duration::from_millis(300));

    drop(lifeline);

    let started = Instant::now();
    while child.try_wait().unwrap().is_none() {
        assert!(started.elapsed() < Duration::from_secs(2), "still running");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// How `atlas machine-supervise` with `words` ended when its lifeline is
/// `lifeline`, renewed at once, and whether it names a daemon of the test's:
/// its exit status and what it said on its standard error.
fn supervised(words: &[&str], lifeline: Lifeline, named: bool) -> (Option<i32>, String) {
    let (errors, writing) = atlas_core::spawn::pipe().unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_atlas"));
    command
        .arg("machine-supervise")
        .arg("--")
        .args(words)
        .stdout(Stdio::null())
        .stderr(Stdio::from(OwnedFd::from(writing)));
    if named {
        command.env(
            atlas_daemon::worker::TEST_DAEMON,
            std::process::id().to_string(),
        );
    }
    let mut child = match lifeline {
        Lifeline::Socket => {
            let (mut ours, theirs) = atlas_core::spawn::socket_pair().unwrap();
            command.stdin(Stdio::from(OwnedFd::from(theirs)));
            let child = atlas_core::spawn::spawn(&mut command).unwrap();
            let _ = ours.write_all(&[RENEW]);
            let child = wait_for(child);
            drop(ours);
            child
        }
        Lifeline::Pipe => {
            let (reading, mut ours) = atlas_core::spawn::pipe().unwrap();
            command.stdin(Stdio::from(OwnedFd::from(reading)));
            let child = atlas_core::spawn::spawn(&mut command).unwrap();
            let _ = ours.write_all(&[RENEW]);
            let child = wait_for(child);
            drop(ours);
            child
        }
    };
    drop(command);
    let mut said = String::new();
    let mut errors = errors;
    errors.read_to_string(&mut said).unwrap();
    (child.wait().unwrap().code(), said)
}

/// What a supervisor is given as its standard input.
enum Lifeline {
    /// A socket the test made, as its daemon makes one.
    Socket,
    /// A pipe, which anyone could write.
    Pipe,
}

/// `child` once it has ended, within a few seconds.
fn wait_for(mut child: Child) -> Child {
    let started = Instant::now();
    while child.try_wait().unwrap().is_none() {
        assert!(started.elapsed() < Duration::from_secs(5), "still running");
        std::thread::sleep(Duration::from_millis(20));
    }
    child
}

#[test]
fn the_supervisor_command_starts_nothing_from_words_that_are_not_an_invocation() {
    let (status, _) = supervised(&["0", "touch", "/usr/bin/touch"], Lifeline::Socket, true);

    assert_eq!(status, Some(127));
}

#[test]
fn the_supervisor_command_outside_a_machine_guests_daemon_is_refused_and_runs_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let proofs = directory.path().join("proofs");
    std::fs::create_dir(&proofs).unwrap();
    let marker = directory.path().join("ran");
    let words = [
        "2000",
        proofs.to_str().unwrap(),
        "touch",
        "/usr/bin/touch",
        marker.to_str().unwrap(),
    ];

    for (lifeline, named) in [(Lifeline::Socket, false), (Lifeline::Pipe, true)] {
        let (status, said) = supervised(&words, lifeline, named);
        assert_eq!(status, Some(127), "{said}");
        assert!(said.contains(atlas_daemon::worker::REFUSAL), "{said}");
        assert!(!marker.exists(), "the program ran");
    }
}
