use super::{OUTPUT_LIMIT, OutputError, Transport, accepted, refused, run_output, with_output};
use crate::error::{Code, Error, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{cell::Cell, error::Error as _, io, sync::Arc, time::Duration};
use tokio::time::{Instant, timeout};

#[test]
fn output_cost_counts_events_only_when_they_are_sent() {
    let fixtures: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../../../tests/fixtures/reply-cost/run.json")).unwrap();
    for fixture in fixtures {
        let reply = &fixture["input"];
        if let Some(bytes) = fixture["decodedBytes"].as_u64() {
            assert_eq!(
                run_output(reply, OUTPUT_LIMIT).unwrap().len(),
                bytes as usize
            );
        } else {
            assert_eq!(run_output(reply, OUTPUT_LIMIT), Err(OutputError::Malformed));
        }
        crate::test_support::reply_cost(&with_output(reply.clone()), &fixture);
    }
}

fn endpoint() -> Transport {
    Transport {
        path: "/tmp/atlas-test.sock".into(),
        timeout: Duration::from_millis(25),
        peer_uid: 502,
    }
}

fn io_cause(error: &Error) -> &io::Error {
    error.source().unwrap().downcast_ref().unwrap()
}

#[tokio::test(start_paused = true)]
async fn a_transient_identity_failure_tries_a_fresh_connection_before_returning_it() {
    let transport = endpoint();
    let started = Instant::now();
    let attempts = Cell::new(0);
    let mut previous = std::sync::Weak::new();
    let verified = timeout(
        Duration::from_secs(1),
        transport.connect_verified(
            started + transport.timeout,
            || {
                assert!(previous.upgrade().is_none(), "the failed peer was dropped");
                attempts.set(attempts.get() + 1);
                let connection = Arc::new(attempts.get());
                previous = Arc::downgrade(&connection);
                std::future::ready(Ok(connection))
            },
            |connection| match **connection {
                1 => Err(io::Error::from_raw_os_error(libc::ENOTCONN)),
                _ => Ok(transport.peer_uid),
            },
        ),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(*verified, 2);
    assert_eq!(attempts.get(), 2);
    assert_eq!(started.elapsed(), super::IDENTITY_RETRY);
}

#[tokio::test(start_paused = true)]
async fn a_wrong_peer_account_is_refused_without_a_retry() {
    let transport = endpoint();
    let started = Instant::now();
    let attempts = Cell::new(0);
    let error = timeout(
        Duration::from_secs(1),
        transport.connect_verified(
            started + transport.timeout,
            || {
                attempts.set(attempts.get() + 1);
                std::future::ready(Ok(()))
            },
            |_| Ok(transport.peer_uid + 1),
        ),
    )
    .await
    .unwrap()
    .unwrap_err();

    assert_eq!(attempts.get(), 1);
    assert_eq!(started.elapsed(), Duration::ZERO);
    assert_eq!(error.code, Code::Identity);
    assert!(!error.uncertain);
    assert!(error.error.contains("verify the peer account"));
    assert!(error.error.contains(transport.path.to_str().unwrap()));
}

#[tokio::test(start_paused = true)]
async fn other_identity_failures_keep_their_context_and_cause_without_retrying() {
    let transport = endpoint();
    for errno in [libc::EACCES, libc::EPERM, libc::EINVAL] {
        let attempts = Cell::new(0);
        let error = timeout(
            Duration::from_secs(1),
            transport.connect_verified(
                Instant::now() + transport.timeout,
                || {
                    attempts.set(attempts.get() + 1);
                    std::future::ready(Ok(()))
                },
                |_| Err(io::Error::from_raw_os_error(errno)),
            ),
        )
        .await
        .unwrap()
        .unwrap_err();

        assert_eq!(attempts.get(), 1);
        assert_eq!(error.code, Code::Io);
        assert!(!error.uncertain);
        assert!(error.error.contains("read the peer account"));
        assert!(error.error.contains(transport.path.to_str().unwrap()));
        assert_eq!(io_cause(&error).raw_os_error(), Some(errno));
        let wire = serde_json::to_value(&error).unwrap();
        assert_eq!(wire.as_object().unwrap().len(), 4);
        assert!(wire.get("source").is_none());
    }
}

#[tokio::test(start_paused = true)]
async fn connection_failures_are_never_retried_as_identity_failures() {
    let transport = endpoint();
    for errno in [libc::ENOTCONN, libc::EACCES, libc::ECONNREFUSED] {
        let attempts = Cell::new(0);
        let error = timeout(
            Duration::from_secs(1),
            transport.connect_verified(
                Instant::now() + transport.timeout,
                || {
                    attempts.set(attempts.get() + 1);
                    std::future::ready(Err::<(), _>(io::Error::from_raw_os_error(errno)))
                },
                |_| panic!("a failed connection has no peer to verify"),
            ),
        )
        .await
        .unwrap()
        .unwrap_err();

        assert_eq!(attempts.get(), 1);
        assert_eq!(error.code, Code::Io);
        assert!(!error.uncertain);
        assert!(error.error.contains("connect to socket"));
        assert!(error.error.contains(transport.path.to_str().unwrap()));
        assert_eq!(io_cause(&error).raw_os_error(), Some(errno));
    }
}

#[tokio::test(start_paused = true)]
async fn repeated_identity_failures_stop_at_the_original_deadline_with_the_cause() {
    let transport = endpoint();
    let started = Instant::now();
    let attempts = Cell::new(0);
    let error = timeout(
        Duration::from_secs(1),
        transport.connect_verified(
            started + transport.timeout,
            || {
                attempts.set(attempts.get() + 1);
                std::future::ready(Ok(()))
            },
            |_| Err(io::Error::from_raw_os_error(libc::ENOTCONN)),
        ),
    )
    .await
    .unwrap()
    .unwrap_err();

    assert_eq!(attempts.get(), 3);
    assert_eq!(started.elapsed(), transport.timeout);
    assert_eq!(error.code, Code::Transport);
    assert!(!error.uncertain);
    assert!(error.error.contains("Timed out reading the peer account"));
    assert!(error.error.contains(transport.path.to_str().unwrap()));
    assert_eq!(io_cause(&error).raw_os_error(), Some(libc::ENOTCONN));
}

#[tokio::test(start_paused = true)]
async fn a_stalled_fresh_connection_uses_only_the_remaining_deadline() {
    let transport = endpoint();
    let started = Instant::now();
    let attempts = Cell::new(0);
    let error = timeout(
        Duration::from_secs(1),
        transport.connect_verified(
            started + transport.timeout,
            || {
                attempts.set(attempts.get() + 1);
                let attempt = attempts.get();
                async move {
                    if attempt > 1 {
                        std::future::pending::<()>().await;
                    }
                    Ok(())
                }
            },
            |_| Err(io::Error::from_raw_os_error(libc::ENOTCONN)),
        ),
    )
    .await
    .unwrap()
    .unwrap_err();

    assert_eq!(attempts.get(), 2);
    assert_eq!(started.elapsed(), transport.timeout);
    assert_eq!(error.code, Code::Transport);
    assert!(!error.uncertain);
    assert!(error.error.contains("Timed out connecting to socket"));
    assert!(error.error.contains(transport.path.to_str().unwrap()));
    assert!(error.source().unwrap().is::<tokio::time::error::Elapsed>());
}

/// A listener on a socket in a new folder (kept while the folder is), and
/// a transport to it.
fn listening() -> (tempfile::TempDir, tokio::net::UnixListener, Transport) {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("t.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let transport = Transport {
        path: socket,
        timeout: std::time::Duration::from_secs(10),
        peer_uid: nix::unistd::getuid().as_raw(),
    };

    (dir, listener, transport)
}

/// A verified peer answers only after reading the complete request line.
async fn reply_line(reply: &str) -> Result<Value> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let (_dir, listener, transport) = listening();
    let reply = reply.to_owned();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut peer = BufReader::new(stream);
        let mut request = String::new();
        assert!(peer.read_line(&mut request).await.unwrap() > 0);
        assert!(request.ends_with('\n'));
        peer.get_mut().write_all(reply.as_bytes()).await.unwrap();
    });
    let result = transport.call(&json!({"op": "status"})).await;
    server.await.unwrap();
    result
}

#[tokio::test]
async fn malformed_uncertainty_in_a_final_reply_is_an_unknown_protocol_outcome() {
    for ok in [false, true] {
        for flag in crate::test_support::invalid_uncertainty_flags() {
            let reply = json!({"ok": ok, "uncertain": flag});
            let error = reply_line(&format!("{reply}\n")).await.unwrap_err();
            assert_eq!(error.code, Code::Protocol);
            assert_eq!(error.error, "Invalid uncertainty flag");
            assert!(error.uncertain, "the complete request was sent");
            assert!(
                !serde_json::to_string(&error)
                    .unwrap()
                    .contains("private-uncertainty-input-canary")
            );
        }
    }
}

#[tokio::test]
async fn valid_reply_uncertainty_keeps_legacy_presence_and_last_member_semantics() {
    for ok in [false, true] {
        for fields in [
            "",
            ",\"uncertain\":false",
            ",\"uncertain\":true",
            ",\"uncertain\":null,\"uncertain\":false",
            ",\"uncertain\":{},\"uncertain\":true",
            ",\"uncertain\":\"opaque\",\"uncertain\":false",
        ] {
            let line = format!("{{\"ok\":{ok},\"extension\":{{\"kept\":1}}{fields}}}");
            let mut actual = reply_line(&format!("{line}\n")).await.unwrap();
            assert!(actual["elapsedMs"].is_number());
            actual.as_object_mut().unwrap().remove("elapsedMs");
            assert_eq!(actual, serde_json::from_str::<Value>(&line).unwrap());
        }
    }

    let event = json!({"event": "future-event", "uncertain": "opaque-extension"});
    let reply = reply_line(&format!("{event}\n{{\"ok\":true}}\n"))
        .await
        .unwrap();
    assert_eq!(reply["events"], json!([event]));
}

/// The client refuses exactly the requests the helper daemon would: a
/// line longer than its limit, not counting the newline.
#[tokio::test]
async fn the_request_limit_is_the_daemons() {
    use super::MAX_REQUEST;

    let (_dir, listener, transport) = listening();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut sink = Vec::new();
                let _ = tokio::io::AsyncReadExt::read_to_end(&mut stream, &mut sink).await;
            });
        }
    });

    // A JSON string is its characters plus two quotes.
    let request = |length: usize| json!("x".repeat(length - 2));
    let fits = transport.connect().await.unwrap();
    assert!(fits.dispatch(&request(MAX_REQUEST)).await.is_ok());

    let over = transport.connect().await.unwrap();
    let refused = over
        .dispatch(&request(MAX_REQUEST + 1))
        .await
        .err()
        .unwrap();

    assert_eq!(
        (refused.code, refused.error.as_str()),
        (Code::Protocol, "Request exceeds 1 MiB")
    );
    assert!(!refused.uncertain, "nothing was sent");
}

/// A peer that refused and hung up before the request was sent is
/// heard: nothing ran, so the outcome is known.
#[tokio::test]
async fn a_refusal_sent_before_the_request_is_heard() {
    let (_dir, listener, transport) = listening();
    let (refuse, ready_to_refuse) = tokio::sync::oneshot::channel();
    let (closed, peer_closed) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        // Keep this exact connection open through peer verification.
        ready_to_refuse.await.unwrap();
        crate::native::endpoint::turn_away(
            stream,
            &crate::protocol::failure(Code::Busy, "Too many requests"),
        );
        closed.send(()).unwrap();
    });

    let pending = transport.connect().await.unwrap();
    refuse.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), peer_closed)
        .await
        .expect("The verified peer must refuse and close before dispatch")
        .unwrap();

    let refused = pending
        .dispatch(&json!({"op": "run", "pad": "x".repeat(512 * 1024)}))
        .await
        .err()
        .unwrap();

    assert_eq!(
        (refused.code, refused.error.as_str()),
        (Code::Busy, "Too many requests")
    );
    assert!(!refused.uncertain, "nothing ran");
}

/// A request the peer never reads whole was never acted on: both servers
/// wait for its newline, which is the last byte sent.
#[tokio::test]
async fn a_request_never_sent_whole_is_certain() {
    use super::DISPATCH;

    let (_dir, listener, transport) = listening();
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            held.push(stream);
        }
    });

    let started = std::time::Instant::now();
    let stalled = transport
        .connect()
        .await
        .unwrap()
        .dispatch(&json!({"op": "run", "pad": "x".repeat(1000 * 1024)}))
        .await
        .err()
        .unwrap();

    assert!(started.elapsed() < DISPATCH * 4);
    assert_eq!(stalled.code, "transport");
    assert!(!stalled.uncertain, "the newline was never sent");
}

/// A peer that reads part of the request and hangs up without a word
/// never saw its newline either.
#[tokio::test]
async fn a_peer_that_hangs_up_mid_request_changed_nothing() {
    use tokio::io::AsyncReadExt;

    let (_dir, listener, transport) = listening();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut start = [0; 16];
            let _ = stream.read_exact(&mut start).await;
            drop(stream);
        }
    });

    let cut = transport
        .connect()
        .await
        .unwrap()
        .dispatch(&json!({"op": "run", "pad": "x".repeat(1000 * 1024)}))
        .await
        .err()
        .unwrap();

    assert_eq!(cut.code, "transport");
    assert!(!cut.uncertain, "the newline was never sent");
}

/// A refusal keeps the helper's code when Atlas knows it, and says
/// whether what was refused may have happened.
#[test]
fn a_refusal_keeps_its_code_and_doubt() {
    let refused = super::refusal(
        &json!({"ok": false, "code": "bad-request", "error": "no", "uncertain": false}),
    );
    assert_eq!((refused.code, refused.uncertain), (Code::BadRequest, false));

    let unknown = super::refusal(
        &json!({"ok": false, "code": "something-new", "error": "?", "uncertain": true}),
    );
    assert_eq!((unknown.code, unknown.uncertain), (Code::Daemon, true));
}

#[test]
fn each_refusal_boundary_keeps_only_its_named_fields() {
    for uncertain in [false, true] {
        let error = super::refusal(&json!({
            "ok": false, "code": "bad-request", "error": "No", "uncertain": uncertain,
            "holder": "person", "reason": "app", "unread": "slow", "found": {"pid": 812},
            "performed": 2, "partial": true, "id": "job", "task": "task", "state": "running",
            "secret": "unrelated-refusal-secret-4b72"
        }));
        assert_eq!(
            serde_json::to_value(error).unwrap(),
            json!({"ok": false, "code": "bad-request", "error": "No", "uncertain": uncertain})
        );
    }
}

#[test]
fn a_command_the_helper_stopped_part_way_stays_uncertain_with_its_code() {
    for code in ["on-console", "restart", "transport"] {
        let reply = json!({
            "ok": false,
            "code": code,
            "error": "stopped",
            "uncertain": true,
        });

        let error = accepted(reply).unwrap_err();
        assert_eq!(error.code, code);
        assert!(error.uncertain, "{code}");
    }
}

#[test]
fn a_command_the_helper_never_started_is_a_certain_refusal() {
    let reply = json!({"ok": false, "code": "spawn-failed", "error": "no such file"});

    let error = accepted(reply).unwrap_err();
    assert_eq!(error.code, "spawn-failed");
    assert!(!error.uncertain);
}

#[test]
fn a_command_that_ran_to_its_end_is_read_for_its_exit() {
    let reply = json!({"ok": true, "exitCode": 1, "timedOut": false});

    assert_eq!(accepted(reply.clone()).unwrap(), reply);
}

#[test]
fn a_refused_app_listing_never_reads_as_no_apps() {
    let refused = accepted(json!({"ok": false, "code": "busy", "error": "Busy"}));
    assert_eq!(refused.map_err(|error| error.code).unwrap_err(), "busy");
    assert!(accepted(json!({"ok": true, "apps": []})).is_ok());
}

#[test]
fn only_explicit_success_is_accepted_and_unknown_refusals_keep_their_details() {
    for reply in [
        json!(null),
        json!({}),
        json!({"ok": null}),
        json!({"ok": 0}),
        json!({"ok": "true"}),
        json!({"ok": []}),
        json!({"ok": {}}),
    ] {
        let error = accepted(reply).unwrap_err();
        assert_eq!(error.code, "daemon");
        assert_eq!(error.error, "The helper refused the request");
        assert!(!error.uncertain);
    }

    let error = accepted(json!({
        "ok": false,
        "code": "future-code",
        "error": "The helper stopped",
        "uncertain": true,
    }))
    .unwrap_err();
    assert_eq!(error.code, "daemon");
    assert_eq!(error.error, "The helper stopped");
    assert!(error.uncertain);
}

/// Only a connection policy forbids is `blocked`; a missing socket or a
/// permission on its folder stays what it is.
#[test]
fn a_forbidden_connection_is_blocked() {
    let transport = endpoint();
    for (errno, code) in [
        (libc::EPERM, Code::Blocked),
        (libc::EACCES, Code::Io),
        (libc::ENOENT, Code::Io),
        (libc::ECONNREFUSED, Code::Io),
    ] {
        let error = refused(&transport.path, io::Error::from_raw_os_error(errno));
        assert_eq!(error.code, code);
        assert!(!error.uncertain);
        assert!(error.error.contains("connect"));
        assert!(error.error.contains(transport.path.to_str().unwrap()));
        assert_eq!(io_cause(&error).raw_os_error(), Some(errno));
    }
}

#[test]
fn run_output_joins_only_output_events_within_the_limit() {
    let reply = json!({"events": [
        {"event": "started", "pid": 4},
        {"event": "output", "data": "aGVs"},
        {"event": "output", "data": "bG8="},
    ]});
    assert_eq!(run_output(&reply, 5).unwrap(), b"hello");
    assert_eq!(run_output(&reply, 4), Err(OutputError::Oversized));
    assert_eq!(run_output(&json!({}), 0).unwrap(), b"");

    for data in [json!(7), json!("not base64!")] {
        let reply = json!({"events": [{"event": "output", "data": data}]});
        assert_eq!(run_output(&reply, 64), Err(OutputError::Malformed));
    }

    let huge = json!({"events": [{"event": "output", "data": "A".repeat(4096)}]});
    assert_eq!(run_output(&huge, 16), Err(OutputError::Oversized));

    let decoded = with_output(reply.clone());
    assert_eq!(decoded["output"], "hello");
    assert_eq!(decoded["pid"], 4, "the process is still named");
    assert!(
        decoded.get("events").is_none(),
        "decoded events are not repeated"
    );

    let undecodable = with_output(json!({"events": [{"event": "output", "data": 7}]}));
    assert!(undecodable.get("output").is_none());
    assert!(
        undecodable.get("events").is_some(),
        "what could not be decoded stays"
    );

    let chunk = STANDARD.encode("x".repeat(600_000));
    let long = with_output(json!({"events": [
        {"event": "started", "pid": 9},
        {"event": "output", "data": chunk},
        {"event": "output", "data": chunk},
    ]}));
    assert_eq!(long["output"].as_str().unwrap().len(), OUTPUT_LIMIT);
    assert_eq!(long["outputTruncated"], true, "the cut is said");
    assert_eq!(long["pid"], 9);
    assert!(long.get("events").is_none());
}

#[test]
fn binary_event_data_is_returned_exactly() {
    let bytes: Vec<u8> = (0..=255).chain([0, 0xff, 0xfe, 0xc0]).collect();
    let half = bytes.len() / 2;
    let reply = json!({"ok": true, "events": [
        {"event": "data", "data": STANDARD.encode(&bytes[..half])},
        {"event": "output", "data": STANDARD.encode(b"not this")},
        {"event": "data", "data": STANDARD.encode(&bytes[half..])},
    ]});
    let data = |limit| -> std::result::Result<Vec<u8>, OutputError> {
        let mut output = Vec::new();
        for event in super::events(&reply, "data") {
            super::append_event(event, &mut output, limit)?;
        }
        Ok(output)
    };
    assert_eq!(data(1024).unwrap(), bytes);
    assert_eq!(data(10), Err(OutputError::Oversized));
    assert_eq!(run_output(&reply, 1024).unwrap(), b"not this");
}

/// Each event is handed on as it arrives and never kept, so a long
/// reply is never held whole; the final reply carries none of them.
#[tokio::test]
async fn events_are_handed_on_as_they_arrive() {
    use tokio::io::AsyncWriteExt;

    let (_dir, listener, transport) = listening();
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut line = Vec::new();
        loop {
            let mut byte = [0_u8; 1];
            tokio::io::AsyncReadExt::read_exact(&mut stream, &mut byte)
                .await
                .unwrap();
            if byte[0] == b'\n' {
                break;
            }
            line.push(byte[0]);
        }
        for n in 0..3 {
            let event = json!({"event": "data", "data": STANDARD.encode([n])});
            stream
                .write_all(format!("{event}\n").as_bytes())
                .await
                .unwrap();
        }
        stream.write_all(b"{\"ok\":true}\n").await.unwrap();
    });

    let mut seen = Vec::new();
    let reply = transport
        .connect()
        .await
        .unwrap()
        .dispatch(&json!({"op": "read"}))
        .await
        .unwrap()
        .finish_each(&mut |event| {
            super::append_event(&event, &mut seen, 16).unwrap();
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(seen, [0, 1, 2]);
    assert_eq!(reply["ok"], true);
    assert!(reply.get("events").is_none());
}
