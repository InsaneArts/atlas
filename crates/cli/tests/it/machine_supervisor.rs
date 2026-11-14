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

#[test]
fn the_supervisor_command_starts_nothing_from_words_that_are_not_an_invocation() {
    let status = atlas_core::spawn::status(
        Command::new(env!("CARGO_BIN_EXE_atlas"))
            .args(["machine-supervise", "--", "0", "touch", "/usr/bin/touch"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
    )
    .unwrap();

    assert_eq!(status.code(), Some(127));
}
