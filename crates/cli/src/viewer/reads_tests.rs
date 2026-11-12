use super::*;
use crate::viewer::http::configured;
use atlas_client::test_support::Helper;
use atlas_core::{session::Observation, test_support::UID};
use std::future::{Future, poll_fn};

/// A stand-in for the helper's input service at `socket`, running build
/// `build` but not ready. It reports each request's operation and answers
/// a reset only once the returned gate opens.
fn input_service(
    socket: &Path,
    build: String,
) -> (
    tokio::sync::mpsc::UnboundedReceiver<String>,
    Arc<tokio::sync::Notify>,
) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let listener = tokio::net::UnixListener::bind(socket).unwrap();
    let (asked, operations) = tokio::sync::mpsc::unbounded_channel();
    let gate = Arc::new(tokio::sync::Notify::new());
    let opens = gate.clone();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let (asked, opens, build) = (asked.clone(), opens.clone(), build.clone());
            tokio::spawn(async move {
                let mut stream = BufReader::new(stream);
                let mut line = String::new();
                stream.read_line(&mut line).await.unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                let operation = request["op"].as_str().unwrap().to_owned();
                let _ = asked.send(operation.clone());

                let reply = match operation.as_str() {
                    "hello" => json!({"ok": true, "protocolVersion": 1, "pid": 456,
                        "ready": false, "accessibility": true, "build": build,
                        "controlActive": false, "heldInput": false}),
                    "reset" => {
                        opens.notified().await;
                        json!({"ok": true})
                    }
                    _ => json!({"ok": true}),
                };
                let line = format!("{reply}\n");
                let _ = stream.get_mut().write_all(line.as_bytes()).await;
            });
        }
    });
    (operations, gate)
}

/// A viewer of a verified stand-in helper, that stand-in, and the build of
/// the input service installed for it.
fn verified(dir: &Path) -> (Arc<Viewer>, Helper, String) {
    let (mut client, helper) = Helper::client(dir, UID);
    std::fs::write(&client.config.identity_probe, b"installed input").unwrap();
    let build = client.installed_build().unwrap();
    client.config.input_socket = dir.join("input.sock");
    (
        configured(client, "127.0.0.1:1".into(), None),
        helper,
        build,
    )
}

/// What the status read says of the helper: in the background, with
/// Accessibility on and input not running.
async fn observed(client: &Client) -> Observation {
    let mut status = client.status().await;
    status["permissions"]["accessibility"] = json!(true);
    Observation::decode(&status)
}

#[tokio::test]
async fn shutdown_waits_for_automatic_input_startup_already_in_progress() {
    let dir = tempfile::tempdir().unwrap();
    let (viewer, helper, build) = verified(dir.path());
    let client = viewer.client();
    let (mut asked, answer) = input_service(&client.config.input_socket, build);

    helper
        .serve(
            async {
                let observation = observed(&client).await;
                let starting = viewer
                    .start_input_after_login(&client, &observation, true)
                    .expect("the first status read starts input");
                while asked.recv().await.as_deref() != Some("reset") {}
                assert_eq!(client.controller.acquire(false).unwrap_err().code, "busy");

                let closing = viewer.close();
                tokio::pin!(closing);
                poll_fn(|cx| {
                    assert!(
                        closing.as_mut().poll(cx).is_pending(),
                        "shutdown waits for the reset in progress"
                    );
                    std::task::Poll::Ready(())
                })
                .await;
                assert_eq!(client.controller.acquire(false).unwrap_err().code, "busy");
                assert!(!starting.is_finished());

                answer.notify_one();
                closing.await;
                assert!(starting.is_finished());
                starting.await.unwrap();
                assert!(client.controller.pending().unwrap().is_empty());
                assert!(client.controller.acquire(false).is_ok());

                let after = helper.requests();
                assert!(
                    viewer
                        .start_input_after_login(&client, &observation, true)
                        .is_none(),
                    "one try per daemon and display"
                );
                while let Ok(operation) = asked.try_recv() {
                    assert_eq!(operation, "hello", "nothing changes after shutdown");
                }
                assert_eq!(helper.requests(), after);
            },
            |_| None,
        )
        .await;
}

#[tokio::test]
async fn automatic_input_startup_queued_after_shutdown_sends_no_request() {
    let dir = tempfile::tempdir().unwrap();
    let (viewer, helper, build) = verified(dir.path());
    let client = viewer.client();
    let (mut asked, _answer) = input_service(&client.config.input_socket, build);

    helper
        .serve(
            async {
                let observation = observed(&client).await;
                while asked.try_recv().is_ok() {}
                let before = helper.requests();

                viewer.close().await;
                viewer
                    .start_input_after_login(&client, &observation, true)
                    .expect("the attempt is scheduled")
                    .await
                    .unwrap();
                assert_eq!(helper.requests(), before, "the daemon is asked nothing");
                assert!(asked.try_recv().is_err(), "input is asked nothing");
                assert!(client.controller.pending().unwrap().is_empty());
            },
            |_| None,
        )
        .await;
}

#[tokio::test]
async fn an_account_title_is_asked_again_after_no_answer_and_kept_once_answered() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static ASKED: AtomicUsize = AtomicUsize::new(0);
    fn ask(_: &str) -> Title {
        match ASKED.fetch_add(1, Ordering::SeqCst) {
            0 => Title::Unknown,
            1 => Title::Titled("Atlas".to_owned()),
            _ => Title::Titled("Not asked".to_owned()),
        }
    }
    let known = std::sync::Mutex::new(None);

    assert_eq!(account_title(&known, "atlas", ask).await, None);
    assert_eq!(
        account_title(&known, "atlas", ask).await.as_deref(),
        Some("Atlas")
    );
    assert_eq!(
        account_title(&known, "atlas", ask).await.as_deref(),
        Some("Atlas")
    );
    assert_eq!(ASKED.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn an_account_with_no_full_name_is_asked_once() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static ASKED: AtomicUsize = AtomicUsize::new(0);
    fn ask(_: &str) -> Title {
        ASKED.fetch_add(1, Ordering::SeqCst);
        Title::Untitled
    }
    let known = std::sync::Mutex::new(None);

    assert_eq!(account_title(&known, "atlas", ask).await, None);
    assert_eq!(account_title(&known, "atlas", ask).await, None);
    assert_eq!(ASKED.load(Ordering::SeqCst), 1);
}

#[test]
fn only_the_persons_app_is_told_which_app_keeps_their_idle_time_stopped() {
    let held = json!({"ready": true, "idleTime": {"state": "held", "app": "Terminal"}});

    let mut person = held.clone();
    keep_idle_time_for(&mut person, true);
    assert_eq!(person, held);

    let mut agent = held;
    keep_idle_time_for(&mut agent, false);
    assert_eq!(agent, json!({"ready": true}));
}
