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
        handed: false,
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

/// Where a program the supervisor runs for these tests finds the atlas it
/// starts as the environment worker on a socket of its own.
const SELF_MADE: &str = "ATLAS_TEST_WORKER_ON_ITS_OWN_SOCKET";

/// The name a test's packet gives the worker's refusal, as a guest daemon
/// gives one.
const PACKET_NAME: &str = "0123456789abcdef0123456789abcdef";

/// A packet whose session is none this Mac has: a worker that reads it is
/// refused by its identity guard, before it does anything.
fn packet_for(daemon: u32) -> Vec<u8> {
    serde_json::json!({
        "version": 1,
        "expected": {"uid": 501, "capturePid": daemon, "securitySessionId": 1, "displayId": 1,
            "width": 1.0, "height": 1.0},
        "request": {"operation": "transactions", "all": false, "ids": []},
        "refusal": PACKET_NAME,
    })
    .to_string()
    .into_bytes()
}

/// `program` with `arguments` under `atlas machine-supervise`, as a guest
/// daemon of this test's starts it, handed `packet` when there is one and
/// renewed until it ends: how the program ended, and what it said.
fn under_supervisor(
    program: &str,
    arguments: &[&str],
    packet: Option<&[u8]>,
    env: &[(&str, &str)],
) -> (Option<i32>, String) {
    let proofs = tempfile::tempdir().unwrap();
    let invocation = Invocation {
        lease: Duration::from_secs(2),
        proofs: proofs.path().to_owned(),
        argv0: program.into(),
        program: program.into(),
        arguments: arguments.iter().map(OsString::from).collect(),
        handed: packet.is_some(),
    };
    let supervisor = Supervisor::new(
        env!("CARGO_BIN_EXE_atlas").into(),
        vec!["machine-supervise".into(), "--".into()],
    );
    let (mut lifeline, theirs) = atlas_core::spawn::socket_pair().unwrap();
    let (mut output, writing) = atlas_core::spawn::pipe().unwrap();
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
        .envs(env.iter().copied())
        .process_group(0);
    let mut child = atlas_core::spawn::spawn(&mut command).unwrap();
    drop(command);
    if let Some(packet) = packet {
        let mut frame = vec![atlas_daemon::machine::supervise::PACKET];
        frame.extend(u32::try_from(packet.len()).unwrap().to_be_bytes());
        frame.extend(packet);
        lifeline.write_all(&frame).unwrap();
    }
    let mut renewing = lifeline.try_clone().unwrap();
    let renewals = std::thread::spawn(move || {
        while renewing.write_all(&[RENEW]).is_ok() {
            std::thread::sleep(Duration::from_millis(200));
        }
    });
    let reading = std::thread::spawn(move || {
        let mut said = String::new();
        let _ = output.read_to_string(&mut said);
        said
    });

    assert_eq!(frame(&mut lifeline), invocation::STARTED);
    let told = invocation::heard(&frame(&mut lifeline));
    let said = reading.join().unwrap();
    drop(lifeline);
    let _ = child.wait();
    renewals.join().unwrap();
    (told.and_then(|status| status.code()), said)
}

/// The refusal a worker printed, as JSON.
fn refusal(said: &str) -> serde_json::Value {
    serde_json::from_str(said.trim_end()).unwrap_or_else(|_| panic!("{said}"))
}

fn atlas() -> &'static str {
    env!("CARGO_BIN_EXE_atlas")
}

#[test]
fn a_worker_handed_its_packet_by_the_supervisor_passes_its_launch_check_and_stops_at_its_guard() {
    let packet = packet_for(std::process::id());

    let (status, said) = under_supervisor(
        atlas(),
        &["environment-worker", "--machine"],
        Some(&packet),
        &[],
    );

    assert_eq!(
        status,
        Some(i32::from(atlas_daemon::worker::REFUSED)),
        "{said}"
    );
    let refused = refusal(&said);
    assert_ne!(refused["error"], atlas_daemon::worker::REFUSAL, "{said}");
    assert_eq!(
        refused["refusal"], PACKET_NAME,
        "named as its packet said: {said}"
    );
    assert!(
        refused["error"]
            .as_str()
            .is_some_and(|error| error.contains("session or display changed"))
            || refused["code"] == "identity",
        "{said}"
    );
}

#[test]
fn a_worker_handed_a_packet_for_another_daemon_stops_at_its_guard() {
    let packet = packet_for(std::process::id() + 1);

    let (status, said) = under_supervisor(
        atlas(),
        &["environment-worker", "--machine", "--apply"],
        Some(&packet),
        &[],
    );

    assert_eq!(
        status,
        Some(i32::from(atlas_daemon::worker::REFUSED)),
        "{said}"
    );
    assert!(
        refusal(&said)["error"]
            .as_str()
            .is_some_and(|error| error.contains("daemon changed")),
        "{said}"
    );
}

#[test]
fn a_worker_an_agents_command_starts_under_the_supervisor_reads_nothing_and_is_refused() {
    let (status, said) = under_supervisor(
        atlas(),
        &["environment-worker", "--machine", "--apply"],
        None,
        &[],
    );

    assert_eq!(
        status,
        Some(i32::from(atlas_daemon::worker::REFUSED)),
        "{said}"
    );
    assert_eq!(
        refusal(&said)["error"],
        atlas_daemon::worker::REFUSAL,
        "{said}"
    );
}

/// What a program the supervisor runs does when [`SELF_MADE`] names the
/// atlas to start: become that atlas's environment worker, its standard
/// input a socket it made itself, the other end kept open.
#[test]
fn a_program_that_makes_its_own_socket_becomes_the_worker_when_asked() {
    let Some(atlas) = std::env::var_os(SELF_MADE) else {
        return;
    };
    let (kept, theirs) = std::os::unix::net::UnixStream::pair().unwrap();
    nix::fcntl::fcntl(
        &kept,
        nix::fcntl::FcntlArg::F_SETFD(nix::fcntl::FdFlag::empty()),
    )
    .unwrap();
    let error = Command::new(atlas)
        .args(["environment-worker", "--machine", "--apply"])
        .stdin(Stdio::from(OwnedFd::from(theirs)))
        .exec();
    panic!("{error}");
}

#[test]
fn a_worker_on_a_socket_it_made_for_itself_is_refused() {
    let this = std::env::current_exe().unwrap();

    let (status, said) = under_supervisor(
        this.to_str().unwrap(),
        &[
            "--exact",
            "machine_supervisor::a_program_that_makes_its_own_socket_becomes_the_worker_when_asked",
            "--test-threads=1",
            "-q",
        ],
        None,
        &[(SELF_MADE, atlas())],
    );

    assert_eq!(
        status,
        Some(i32::from(atlas_daemon::worker::REFUSED)),
        "{said}"
    );
    assert!(said.contains(atlas_daemon::worker::REFUSAL), "{said}");
}

#[test]
fn a_worker_handed_a_socket_by_a_parent_that_is_not_a_supervisor_is_refused() {
    let (mut ours, theirs) = atlas_core::spawn::socket_pair().unwrap();
    let (mut errors, writing) = atlas_core::spawn::pipe().unwrap();
    let mut command = Command::new(atlas());
    command
        .args(["environment-worker", "--machine", "--apply"])
        .stdin(Stdio::from(OwnedFd::from(theirs)))
        .stdout(Stdio::null())
        .stderr(Stdio::from(OwnedFd::from(writing)))
        .env(
            atlas_daemon::worker::TEST_DAEMON,
            std::os::unix::process::parent_id().to_string(),
        );
    let child = atlas_core::spawn::spawn(&mut command).unwrap();
    drop(command);
    let _ = ours.write_all(&packet_for(std::os::unix::process::parent_id()));
    let _ = ours.shutdown(std::net::Shutdown::Write);
    let mut child = wait_for(child);
    let mut said = String::new();
    errors.read_to_string(&mut said).unwrap();

    assert_eq!(
        child.wait().unwrap().code(),
        Some(i32::from(atlas_daemon::worker::REFUSED))
    );
    assert_eq!(
        refusal(&said)["error"],
        atlas_daemon::worker::REFUSAL,
        "{said}"
    );
    drop(ours);
}

#[test]
fn a_worker_started_by_hand_is_refused() {
    let output = atlas_core::spawn::output(
        Command::new(atlas())
            .args(["environment-worker", "--machine", "--apply"])
            .stdin(Stdio::null()),
    )
    .unwrap();

    assert_eq!(
        output.status.code(),
        Some(i32::from(atlas_daemon::worker::REFUSED))
    );
    let said = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        refusal(&said)["error"],
        atlas_daemon::worker::REFUSAL,
        "{said}"
    );
}
