use super::*;

#[test]
fn a_spawned_child_runs_with_its_streams_as_set() {
    let mut child = spawn(
        Command::new("/bin/sh")
            .args(["-c", "printf out"])
            .stdout(Stdio::piped()),
    )
    .unwrap();
    let mut said = String::new();
    io::Read::read_to_string(&mut child.stdout.take().unwrap(), &mut said).unwrap();
    assert!(child.wait().unwrap().success());
    assert_eq!(said, "out");
}

#[test]
fn output_captures_both_streams_and_gives_no_input() {
    let output =
        output(Command::new("/bin/sh").args(["-c", "cat; printf out; printf err >&2; exit 3"]))
            .unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(output.stdout, b"out", "cat read nothing: no input");
    assert_eq!(output.stderr, b"err");
}

#[test]
fn status_waits_for_the_end() {
    let status = status(
        Command::new("/bin/sh")
            .args(["-c", "exit 4"])
            .stdout(Stdio::null()),
    )
    .unwrap();
    assert_eq!(status.code(), Some(4));
}

#[test]
fn a_failed_start_is_its_error() {
    let error = spawn(&mut Command::new("/nonexistent/program")).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
}

#[tokio::test]
async fn the_async_forms_start_at_once_and_only_wait_in_their_futures() {
    let output = output_async(tokio::process::Command::new("/bin/sh").args(["-c", "printf out"]));
    // The lock is free again before the future is awaited.
    drop(alone());
    let output = output.await.unwrap();
    assert_eq!(output.stdout, b"out");
    let status = status_async(
        tokio::process::Command::new("/bin/sh")
            .args(["-c", "exit 5"])
            .stdout(Stdio::null()),
    )
    .await
    .unwrap();
    assert_eq!(status.code(), Some(5));
    let child = spawn_async(&mut tokio::process::Command::new("/usr/bin/true")).unwrap();
    assert!(child.wait_with_output().await.unwrap().status.success());
}

/// A long-lived child started beside many short commands must never hold
/// their pipes: each short command's output still reaches its end at once.
#[test]
fn a_child_started_beside_a_command_never_holds_its_pipe_open() {
    let starter = std::thread::spawn(|| {
        let mut children = Vec::new();
        for _ in 0..200 {
            children.push(
                spawn(
                    Command::new("/bin/sleep")
                        .arg("30")
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null()),
                )
                .unwrap(),
            );
        }
        children
    });
    for _ in 0..200 {
        let (mut reader, writer) = pipe().unwrap();
        drop(writer);
        let mut end = Vec::new();
        // With the writer dropped here, only an inherited copy could keep
        // the pipe open, until its child ends thirty seconds later.
        let started = std::time::Instant::now();
        io::Read::read_to_end(&mut reader, &mut end).unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
        let output = output(Command::new("/bin/echo").arg("done")).unwrap();
        assert_eq!(output.stdout, b"done\n");
    }
    for mut child in starter.join().unwrap() {
        child.kill().unwrap();
        child.wait().unwrap();
    }
}
