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
    assert!(query(&state, "fresh").await.is_err());
    assert!(state.lock().await.is_none());
    assert!(nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), None).is_err());
}

#[tokio::test]
async fn a_worker_that_never_answers_is_timed_out_and_reaped() {
    let worker = synthetic("read -r request; read -r never");
    let pid = worker.child.id().unwrap();
    let state = Mutex::new(Some(worker));
    assert!(
        tokio::time::timeout(Duration::from_secs(5), query(&state, "fresh"))
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
        tokio::time::timeout(Duration::from_millis(30), query(&state, "fresh"))
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
