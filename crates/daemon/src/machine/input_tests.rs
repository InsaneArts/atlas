use super::*;
use atlas_session::{authority::Authority, clock::Continuous, generation::Generation as Numbered};

/// The input service these tests start: this test binary again, running
/// only this test, serving the channel on its standard input for the
/// folder after `--`, as `atlas machine-input` does once its parent is
/// proved.
#[test]
fn an_input_service_runs_when_started_for_these_tests() {
    use std::os::fd::AsFd;
    let Some(folder) = std::env::args_os().skip_while(|word| word != "--").nth(1) else {
        return;
    };
    atlas_core::session::Console::Own.keep().unwrap();
    let channel = std::io::stdin().as_fd().try_clone_to_owned().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let served = runtime.block_on(atlas_native::native::service::serve_machine(
        nix::unistd::getuid().as_raw(),
        std::os::unix::net::UnixStream::from(channel),
        Path::new(&folder),
    ));
    std::process::exit(i32::from(served.is_err()));
}

/// The file a scripted service makes once it is asked to look at an app.
pub(crate) const SCANNING: &str = "scanning";
/// The file a test makes for a scripted service's look to be answered.
pub(crate) const ANSWER: &str = "answer";

/// A scripted input service a test of the daemon starts in its place,
/// serving the channel on its standard input for the folder after `--`:
/// ready, holding and releasing as it is told, and answering a look at an
/// app's elements only once the test lets it, so a look can be held part
/// way while the daemon goes on.
#[test]
fn a_scripted_input_service_runs_when_started_for_these_tests() {
    use std::os::fd::AsFd;
    let Some(folder) = std::env::args_os().skip_while(|word| word != "--").nth(1) else {
        return;
    };
    let channel = std::io::stdin().as_fd().try_clone_to_owned().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(scripted(
        std::os::unix::net::UnixStream::from(channel),
        std::path::PathBuf::from(folder),
    ));
    std::process::exit(0);
}

/// Serve `channel` as [`a_scripted_input_service_runs_when_started_for_these_tests`]
/// says, its files in `folder`.
async fn scripted(channel: std::os::unix::net::UnixStream, folder: std::path::PathBuf) {
    use atlas_native::native::service::channel::Frame;
    channel.set_nonblocking(true).unwrap();
    let (mut reading, writing) = tokio::net::UnixStream::from_std(channel)
        .unwrap()
        .into_split();
    let writing = Arc::new(tokio::sync::Mutex::new(writing));
    let say = |frame: Frame| {
        let writing = writing.clone();
        async move {
            let _ = frame.write(&mut *writing.lock().await).await;
        }
    };
    let mut heard: std::collections::HashMap<u32, Vec<u8>> = std::collections::HashMap::new();
    while let Ok(frame) = Frame::read(&mut reading).await {
        match frame {
            Frame::Hold { seq, on } => say(Frame::Held { seq, on }).await,
            Frame::Revoke(generation) => {
                say(Frame::Revoked(generation)).await;
                say(Frame::Settled(generation)).await;
            }
            Frame::Data(connection, bytes) => {
                let line = heard.entry(connection).or_default();
                line.extend(bytes);
                if !line.ends_with(b"\n") {
                    continue;
                }
                let request: serde_json::Value =
                    serde_json::from_slice(&std::mem::take(line)).unwrap_or_default();
                let (folder, writing) = (folder.clone(), writing.clone());
                tokio::spawn(async move {
                    let mut reply = serde_json::to_vec(&scripted_answer(&request, &folder).await)
                        .unwrap_or_default();
                    reply.push(b'\n');
                    let _ = Frame::Data(connection, reply)
                        .write(&mut *writing.lock().await)
                        .await;
                });
            }
            Frame::Open(..)
            | Frame::Close(_)
            | Frame::Renew(..)
            | Frame::Revoked(_)
            | Frame::Settled(_)
            | Frame::Held { .. } => {}
        }
    }
}

/// What the scripted service answers `request` with, its files in
/// `folder`.
async fn scripted_answer(request: &serde_json::Value, folder: &Path) -> serde_json::Value {
    match request["op"].as_str() {
        Some("hello") => serde_json::json!({"ok": true, "protocolVersion": 1,
            "pid": std::process::id(), "ready": true, "accessibility": true,
            "capabilities": []}),
        Some("elements") => {
            let _ = std::fs::write(folder.join(SCANNING), b"");
            for _ in 0..1500 {
                if folder.join(ANSWER).exists() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            serde_json::json!({"ok": true, "look": "8123abc", "elements": [],
                "truncated": false, "reasons": [], "errors": {}})
        }
        _ => serde_json::json!({"ok": false, "code": "bad-request", "error": "Not scripted."}),
    }
}

/// Held by every test that starts a real input service, so they take
/// turns: many at once starve everything else of the window server.
pub(crate) static ONE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A service started for these tests, in a folder of its own.
fn started() -> (Input, tempfile::TempDir) {
    let folder = tempfile::tempdir().unwrap();
    let input = Input::start(
        Starter::of_these_tests(folder.path()),
        folder.path().to_owned(),
    );
    (input, folder)
}

/// Whether `check` holds within `within`.
async fn within(within: Duration, check: impl Fn() -> bool) -> bool {
    let started = std::time::Instant::now();
    loop {
        if check() {
            return true;
        }
        if started.elapsed() >= within {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The service's processes for `folder`, by the words it was started with.
pub(crate) fn services(folder: &Path) -> Vec<i32> {
    let uid = nix::unistd::getuid().as_raw();
    let folder = folder.to_string_lossy();
    atlas_core::process::of_user(uid)
        .unwrap()
        .iter()
        .filter(|process| {
            atlas_core::process::arguments(process.instance.pid).is_ok_and(|arguments| {
                arguments.iter().any(|argument| argument == folder.as_ref())
                    && arguments
                        .iter()
                        .any(|argument| argument.contains("an_input_service_runs"))
            })
        })
        .map(|process| process.instance.pid)
        .collect()
}

fn authority(generation: u64) -> Authority {
    Authority::new(
        Numbered(generation),
        Arc::new(Continuous),
        Box::new(|_, _| {}),
    )
}

#[tokio::test]
async fn an_input_service_the_daemon_starts_says_it_is_ready() {
    let _turn = ONE_AT_A_TIME.lock().await;
    let (input, _folder) = started();

    let standing = input.standing();

    assert!(
        within(Duration::from_secs(20), || *standing.borrow()
            == Standing::Ready)
        .await
    );
    assert!(!input.unsettled());
}

#[tokio::test]
async fn a_hold_is_unsettled_until_the_service_says_it_took_effect() {
    let _turn = ONE_AT_A_TIME.lock().await;
    let (input, _folder) = started();
    let standing = input.standing();
    assert!(
        within(Duration::from_secs(20), || *standing.borrow()
            == Standing::Ready)
        .await
    );

    input.hold(true);

    assert!(within(Duration::from_secs(5), || !input.unsettled()).await);
}

#[tokio::test]
async fn a_generation_replaced_is_revoked_and_settled() {
    let _turn = ONE_AT_A_TIME.lock().await;
    let (input, _folder) = started();
    let standing = input.standing();
    assert!(
        within(Duration::from_secs(20), || *standing.borrow()
            == Standing::Ready)
        .await
    );
    input.live(1, authority(1));

    input.live(2, authority(2));

    assert!(within(Duration::from_secs(5), || !input.unsettled()).await);
}

#[tokio::test]
async fn the_service_ends_with_the_daemon_s_input() {
    let _turn = ONE_AT_A_TIME.lock().await;
    let (input, folder) = started();
    let standing = input.standing();
    assert!(
        within(Duration::from_secs(20), || *standing.borrow()
            == Standing::Ready)
        .await
    );
    assert!(!services(folder.path()).is_empty());

    drop(input);

    assert!(
        within(Duration::from_secs(5), || services(folder.path())
            .is_empty())
        .await
    );
}

#[tokio::test]
async fn a_service_that_ends_is_started_again_and_told_the_hold_first() {
    let _turn = ONE_AT_A_TIME.lock().await;
    let (input, folder) = started();
    let standing = input.standing();
    assert!(
        within(Duration::from_secs(20), || *standing.borrow()
            == Standing::Ready)
        .await
    );
    input.hold(true);
    assert!(within(Duration::from_secs(5), || !input.unsettled()).await);
    let first = services(folder.path());

    for pid in &first {
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(*pid),
            nix::sys::signal::Signal::SIGKILL,
        );
    }

    assert!(
        within(Duration::from_secs(5), || *standing.borrow()
            != Standing::Ready)
        .await
    );
    assert!(
        within(Duration::from_secs(30), || *standing.borrow()
            == Standing::Ready)
        .await
    );
    assert!(
        within(Duration::from_secs(5), || !input.unsettled()).await,
        "the hold was told again"
    );
    let again = services(folder.path());
    assert!(!again.is_empty() && again.iter().all(|pid| !first.contains(pid)));
}

#[tokio::test]
async fn a_service_whose_channel_closes_ends_by_itself() {
    let _turn = ONE_AT_A_TIME.lock().await;
    let folder = tempfile::tempdir().unwrap();
    let starter = Starter::of_these_tests(folder.path());
    let (ours, theirs) = atlas_core::spawn::socket_pair().unwrap();
    let mut command = tokio::process::Command::new(&starter.program);
    command
        .args(&starter.arguments)
        .stdin(Stdio::from(OwnedFd::from(theirs)))
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    let mut child = atlas_core::spawn::spawn_async(&mut command).unwrap();
    drop(command);
    ours.set_nonblocking(true).unwrap();
    let mut ours = tokio::net::UnixStream::from_std(ours).unwrap();
    // As its daemon does first: a hold, a renewal, and a hello.
    Frame::Hold { seq: 1, on: true }
        .write(&mut ours)
        .await
        .unwrap();
    Frame::Renew(1, continuous().unwrap() + nanos(LEASE))
        .write(&mut ours)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    Frame::Open(1, 0).write(&mut ours).await.unwrap();
    Frame::Data(1, b"{\"op\":\"hello\"}\n".to_vec())
        .write(&mut ours)
        .await
        .unwrap();
    let mut heard = Vec::new();
    for _ in 0..6 {
        let frame = tokio::time::timeout(Duration::from_secs(5), Frame::read(&mut ours)).await;
        heard.push(format!("{frame:?}").chars().take(80).collect::<String>());
        if matches!(frame, Ok(Ok(Frame::Close(1)))) {
            break;
        }
    }
    eprintln!("HEARD {heard:#?}");

    drop(ours);

    let ended = tokio::time::timeout(Duration::from_secs(10), child.wait()).await;
    assert!(ended.is_ok(), "still serving with its channel closed");
}

#[tokio::test]
async fn an_earlier_service_still_running_keeps_another_from_starting() {
    let _turn = ONE_AT_A_TIME.lock().await;
    let folder = tempfile::tempdir().unwrap();
    let starter = Starter::of_these_tests(folder.path());
    // An earlier service, its daemon's end of the channel kept open.
    let (_ours, theirs) = atlas_core::spawn::socket_pair().unwrap();
    let mut command = tokio::process::Command::new(&starter.program);
    command
        .args(&starter.arguments)
        .stdin(Stdio::from(OwnedFd::from(theirs)))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let _earlier = atlas_core::spawn::spawn_async(&mut command).unwrap();
    drop(command);
    tokio::time::sleep(Duration::from_millis(500)).await;

    let input = Input::start(starter, folder.path().to_owned());
    let standing = input.standing();

    assert!(
        within(Duration::from_secs(15), || *standing.borrow()
            == Standing::CleanupUnknown)
        .await,
        "started beside an earlier one"
    );
    assert_eq!(services(folder.path()).len(), 1);
}

#[test]
fn a_service_that_failed_to_start_is_read_by_the_last_line_it_wrote() {
    let said = b"starting\n{\"ok\":false,\"code\":\"permission\",\"error\":\"The input process does not have Accessibility permission\"}\n";

    let failure = Failure::of(said).unwrap();

    assert_eq!(failure.code, "permission");
    assert!(failure.error.contains("Accessibility"));
    assert_eq!(Failure::of(b"no json here\n"), None);
}
