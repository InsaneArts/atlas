use super::*;

#[test]
fn each_challenge_reads_a_new_proof_and_echoes_only_its_own_nonce() {
    let input = b"{\"challenge\":\"first\"}\n{\"challenge\":\"second\"}\n";
    let mut output = Vec::new();
    let mut reads = 0;
    serve(input.as_slice(), &mut output, || {
        reads += 1;
        Ok(json!({"ok":true,"sequence":reads}))
    })
    .unwrap();
    let replies: Vec<Value> = output
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect();
    assert_eq!(reads, 2);
    assert_eq!(replies[0]["challenge"], "first");
    assert_eq!(replies[0]["proof"]["sequence"], 1);
    assert_eq!(replies[1]["challenge"], "second");
    assert_eq!(replies[1]["proof"]["sequence"], 2);
}

#[test]
fn malformed_or_oversized_requests_never_query_session_facts() {
    for request in [
        b"{\"challenge\":\"\"}\n".to_vec(),
        b"{\"challenge\":\"two words\"}\n".to_vec(),
        b"{\"challenge\":\"x\",\"extra\":true}\n".to_vec(),
        b"{\"challenge\":\"truncated\"}".to_vec(),
        vec![b'a'; LINE + 20],
        format!("{{\"challenge\":\"{}\"}}\n", "a".repeat(129)).into_bytes(),
    ] {
        let mut output = Vec::new();
        assert!(
            serve(request.as_slice(), &mut output, || panic!(
                "invalid requests must not read session facts"
            ))
            .is_err()
        );
        assert!(output.is_empty());
    }
}

fn synthetic(script: &str) -> Worker {
    Worker::spawn(Command::new("/bin/sh").args(["-c", script])).unwrap()
}

#[tokio::test]
async fn consecutive_proofs_reuse_one_child_but_read_separate_replies() {
    let mut worker = synthetic(
        r#"
read -r first
printf '%s\n' '{"ok":true,"challenge":"first","proof":{"sequence":1}}'
read -r second
printf '%s\n' '{"ok":true,"challenge":"second","proof":{"sequence":2}}'
read -r end
"#,
    );
    let pid = worker.child.id().unwrap();
    assert_eq!(
        worker.exchange("first").await.unwrap()["proof"]["sequence"],
        1
    );
    assert_eq!(
        worker.exchange("second").await.unwrap()["proof"]["sequence"],
        2
    );
    assert_eq!(worker.child.id(), Some(pid));
    worker.stop().await;
    assert!(
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), None).is_err(),
        "the synthetic child was reaped"
    );
}

#[tokio::test]
async fn replayed_replies_discard_and_reap_the_worker_instead_of_reusing_it() {
    let worker = synthetic(
        r#"
read -r request
printf '%s\n' '{"ok":true,"challenge":"old","proof":{"ok":true}}'
read -r end
"#,
    );
    let pid = worker.child.id().unwrap();
    let state = Mutex::new(Some(worker));
    assert!(query(&state, &Starter::Own, "fresh").await.is_err());
    assert!(state.lock().await.is_none());
    assert!(nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), None).is_err());
}

#[tokio::test]
async fn a_worker_that_never_answers_is_timed_out_and_reaped() {
    let worker = synthetic("read -r request; read -r never");
    let pid = worker.child.id().unwrap();
    let state = Mutex::new(Some(worker));
    assert!(
        tokio::time::timeout(
            Duration::from_secs(5),
            query(&state, &Starter::Own, "fresh")
        )
        .await
        .unwrap()
        .is_err()
    );
    assert!(state.lock().await.is_none());
    assert!(nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), None).is_err());
}

#[tokio::test]
async fn cancelling_a_query_cannot_leave_an_old_reply_in_the_cached_worker() {
    let worker = synthetic("read -r request; read -r never");
    let pid = worker.child.id().unwrap();
    let state = Mutex::new(Some(worker));
    assert!(
        tokio::time::timeout(
            Duration::from_millis(30),
            query(&state, &Starter::Own, "fresh")
        )
        .await
        .is_err()
    );
    assert!(state.lock().await.is_none());
    tokio::time::timeout(Duration::from_secs(2), async {
        while nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), None).is_ok() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the dropped synthetic child is killed and reaped");
}

/// The word after `--` that makes this test binary serve as a proof worker.
pub(super) const SERVING: &str = "session-proof";

/// Serve as the proof worker when this test binary was started as one, its
/// replies on descriptor 3 as [`Starter::TheseTests`] gives it: the session
/// facts are this process's own, its parent whoever started it.
#[test]
fn a_proof_worker_serves_when_started_for_these_tests() {
    if std::env::args_os().skip_while(|word| word != "--").nth(1) != Some(SERVING.into()) {
        return;
    }
    let replies = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/fd/3")
        .unwrap();
    let served = serve(
        std::io::stdin().lock(),
        replies,
        atlas_native::native::session_info,
    );
    std::process::exit(i32::from(served.is_err()));
}

#[tokio::test]
async fn a_worker_started_for_these_tests_proves_this_session_with_this_process_its_parent() {
    // Taken as the tests of real input services take it: many of those at
    // once starve the window server the proof asks.
    let _turn = crate::machine::input::tests::ONE_AT_A_TIME.lock().await;
    let state = Mutex::new(None);

    let first = query(&state, &Starter::TheseTests, "first").await.unwrap();
    let second = query(&state, &Starter::TheseTests, "second").await.unwrap();

    assert_eq!(first["challenge"], "first");
    assert_eq!(second["challenge"], "second");
    for reply in [&first, &second] {
        assert_eq!(
            reply["proof"]["parentPID"].as_u64(),
            Some(u64::from(std::process::id())),
            "{reply}"
        );
        assert!(reply["proof"]["scale"].as_f64().is_some(), "{reply}");
    }
    let worker = state.lock().await.take().unwrap();
    worker.stop().await;
}
