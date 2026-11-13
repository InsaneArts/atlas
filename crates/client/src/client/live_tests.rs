use super::*;

#[test]
fn optional_capture_telemetry_cannot_inject_terminal_text_or_replace_frame_proofs() {
    assert!(CaptureStats::parse(&Value::Null).is_none());
    let stats = CaptureStats::parse(&json!({
        "nativeFrames": 9, "fallbacks": 3, "timeouts": 2, "idleHints": 1,
        "lastFallback": "\u{1b}[2J", "nativeReason": "\u{1b}]0;injected\u{7}", "waitUs": 1250,
        "guardBeforeUs": 1000, "guardAfterUs": 500, "copyUs": -3,
    }))
    .unwrap();
    assert_eq!(stats.native_frames, 9);
    assert_eq!(stats.last_fallback, "unknown");
    assert_eq!(stats.native_reason, "unknown");
    assert_eq!(stats.wait_ms, 1.25);
    assert_eq!(stats.guard_ms, 1.5);
    assert_eq!(stats.copy_ms, 0.0);
    let mut unverified = header();
    unverified["captureStats"] = json!({"nativeFrames":u64::MAX,"lastFallback":"none"});
    unverified["session"]["onConsole"] = json!(true);
    assert!(validate_header(&unverified, &epoch(), 640, 0).is_err());
}

#[test]
fn the_live_request_is_accepted_by_the_helpers_session_pinned_protocol() {
    for backend in [
        CaptureBackend::CoreGraphics,
        CaptureBackend::ScreenCaptureKit,
    ] {
        let bytes = serde_json::to_vec(&request(&epoch(), 640, backend)).unwrap();
        assert_eq!(
            atlas_core::protocol::parse(&bytes).unwrap().request,
            atlas_core::protocol::Request::Stream {
                max_dimension: 640,
                backend,
                pin: SessionPin {
                    security_session_id: 123,
                    display_id: 9
                }
            }
        );
    }
}

/// An open stream on `socket`, as a helper that confirms no stops serves it.
fn opened(socket: UnixStream, epoch: &Epoch) -> LiveStream {
    LiveStream {
        socket,
        epoch: epoch.clone(),
        sequence: 0,
        limit: 640,
        synchronized: true,
        profile: Profile::default(),
        backend: CaptureBackend::CoreGraphics,
        producer: None,
        acquisitions: 0,
    }
}

fn epoch() -> Epoch {
    Epoch {
        uid: 502,
        capture_pid: 77,
        input_pid: 88,
        security_session_id: 123,
        display_id: 9,
        display: atlas_core::session::Display {
            width: 1600.0,
            height: 1000.0,
            scale: 2.0,
        },
    }
}
fn header() -> Value {
    json!({"ok":true,"width":640,"height":400,"scale":2.0,"sequence":1,"daemonPID":77,
        "session":{"ok":true,"uid":502,"securitySessionID":123,"displayID":9,
            "width":1600.0,"height":1000.0,"hasGraphicAccess":true,"onConsole":false}})
}

#[test]
fn a_live_frame_requires_the_exact_verified_helper_display_and_process() {
    assert_eq!(
        validate_header(&header(), &epoch(), 640, 0).unwrap(),
        (640, 400, 1)
    );
    for (key, value) in [
        ("uid", json!(503)),
        ("securitySessionID", json!(124)),
        ("displayID", json!(10)),
        ("width", json!(1599)),
        ("height", json!(999)),
        ("hasGraphicAccess", json!(false)),
        ("onConsole", json!(true)),
    ] {
        let mut changed = header();
        changed["session"][key] = value;
        assert!(
            validate_header(&changed, &epoch(), 640, 0).is_err(),
            "{key}"
        );
    }
    for (key, value) in [
        ("daemonPID", json!(78)),
        ("scale", json!(1.0)),
        ("scale", Value::Null),
    ] {
        let mut changed = header();
        changed[key] = value;
        assert!(
            validate_header(&changed, &epoch(), 640, 0).is_err(),
            "{key}"
        );
    }
}

#[test]
fn a_live_frame_bounds_allocation_and_rejects_replayed_or_distorted_pixels() {
    for (key, value) in [
        ("width", json!(0)),
        ("height", json!(-1)),
        ("width", json!(641)),
        ("height", json!(u64::MAX)),
        ("sequence", json!(0)),
        ("width", json!(640.5)),
        ("height", json!(10)),
    ] {
        let mut changed = header();
        changed[key] = value;
        assert!(
            validate_header(&changed, &epoch(), 640, 0).is_err(),
            "{key}"
        );
    }
    assert!(validate_header(&header(), &epoch(), 640, 1).is_err());
    let mut advanced = header();
    advanced["sequence"] = json!(3);
    assert!(validate_header(&advanced, &epoch(), 640, 1).is_ok());
}

#[tokio::test]
async fn header_lengths_are_checked_before_allocating_or_reading_payloads() {
    for length in [0, 4097, u32::MAX] {
        let (mut writer, mut reader) = tokio::io::duplex(8);
        writer.write_u32(length).await.unwrap();
        let result =
            tokio::time::timeout(Duration::from_millis(100), read_header(&mut reader)).await;
        assert!(result.unwrap().is_err());
    }
}

#[tokio::test]
async fn binary_refusals_are_returned_without_reading_pixel_bytes() {
    let (mut writer, mut reader) = tokio::io::duplex(1024);
    let bytes =
        serde_json::to_vec(&json!({"ok":false,"code":"on-console","error":"Helper is visible"}))
            .unwrap();
    writer.write_u32(bytes.len() as u32).await.unwrap();
    writer.write_all(&bytes).await.unwrap();
    let error = read_header(&mut reader).await.unwrap_err();
    assert_eq!(error.error, "Helper is visible");
}

#[tokio::test]
async fn a_short_header_never_becomes_a_frame() {
    let (mut writer, mut reader) = tokio::io::duplex(64);
    writer.write_u32(20).await.unwrap();
    writer.write_all(b"{}").await.unwrap();
    drop(writer);
    assert!(read_header(&mut reader).await.is_err());
}

async fn verified_frame(changed_after_capture: bool) -> Result<LiveFrame> {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) =
        crate::test_support::Helper::client(directory.path(), atlas_core::test_support::UID);
    let helper = if changed_after_capture {
        helper.moving_after(2)
    } else {
        helper
    };
    helper.serve(async {
        let epoch = client.identity().await.unwrap().epoch;
        assert_eq!(helper.proofs(),1);
        let (socket,mut producer) = UnixStream::pair().unwrap();
        let mut stream = opened(socket, &epoch);
        let mut ring = Ring::default();
        let write = async {
            let mut ready=[0];
            producer.read_exact(&mut ready).await.unwrap();
            assert_eq!(ready,[1]);
            assert_eq!(helper.proofs(),2,"READY follows the independent before-proof");
            let header=json!({"ok":true,"sequence":1,"daemonPID":epoch.capture_pid,
                "width":2,"height":2,"scale":epoch.display.scale,
                "session":{"ok":true,"uid":epoch.uid,"securitySessionID":epoch.security_session_id,
                    "displayID":epoch.display_id,"width":epoch.display.width,"height":epoch.display.height,
                    "hasGraphicAccess":true,"onConsole":false}});
            let bytes=serde_json::to_vec(&header).unwrap();
            producer.write_u32(bytes.len() as u32).await.unwrap();
            producer.write_all(&bytes).await.unwrap();
            producer.write_all(&[42;12]).await.unwrap();
        };
        let (result,())=tokio::join!(stream.next(&client, &mut ring),write);
        assert_eq!(helper.proofs(),3,"The received pixels require a new independent after-proof");
        result
    }, |_| panic!("Live reads only ask for identity, never screenshots or mutations")).await
}

#[tokio::test]
async fn pixels_are_accepted_only_between_two_fresh_independent_session_proofs() {
    let frame = verified_frame(false).await.unwrap();
    assert_eq!(frame.rgb.as_slice(), &[42; 12]);
    assert_eq!((frame.width, frame.height), (2, 2));
}

#[tokio::test]
async fn valid_pixel_payloads_are_discarded_when_the_independent_after_proof_changes() {
    match verified_frame(true).await {
        Ok(_) => panic!("A changed helper must never produce an accepted frame"),
        Err(error) => assert_eq!(error.code, Code::SessionChanged),
    }
}

/// A slow, completely consumed frame must not desynchronize the next READY
/// and header. The stand-in serves only identity facts; pixels use one private
/// socket pair, with no display capture or native event injection.
async fn after_a_consumed_stale_frame(second_sequence: u64) -> Result<LiveFrame> {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) =
        crate::test_support::Helper::client(directory.path(), atlas_core::test_support::UID);
    helper.serve(async {
        let epoch = client.identity().await.unwrap().epoch;
        let (socket, mut producer) = UnixStream::pair().unwrap();
        let mut stream = opened(socket, &epoch);
        let mut ring = Ring::default();
        let produce = async {
            for (round, sequence) in [1, second_sequence].into_iter().enumerate() {
                let mut ready = [0];
                producer.read_exact(&mut ready).await.unwrap();
                assert_eq!(ready, [1]);
                if round == 0 {
                    // Instant is the real monotonic clock: Tokio's paused
                    // clock would not make the frame's actual age advance.
                    tokio::time::sleep(FRESHNESS + Duration::from_millis(30)).await;
                }
                let header = json!({"ok":true,"sequence":sequence,"daemonPID":epoch.capture_pid,
                    "width":2,"height":2,"scale":epoch.display.scale,
                    "session":{"ok":true,"uid":epoch.uid,"securitySessionID":epoch.security_session_id,
                        "displayID":epoch.display_id,"width":epoch.display.width,"height":epoch.display.height,
                        "hasGraphicAccess":true,"onConsole":false}});
                let bytes = serde_json::to_vec(&header).unwrap();
                producer.write_u32(bytes.len() as u32).await.unwrap();
                producer.write_all(&bytes).await.unwrap();
                // A replayed header must be refused before reading pixels.
                if round == 0 || sequence > 1 {
                    producer.write_all(&[if sequence == 1 { 7 } else { 42 }; 12]).await.unwrap();
                }
            }
        };
        let consume = async {
            let stale = stream.next(&client, &mut ring).await;
            match stale {
                Ok(_) => panic!("the first stale pixels must never be returned"),
                Err(error) => {
                    assert_eq!(error.code, Code::StaleFrame);
                    assert!(stream.can_retry(&error), "complete stale frames leave a synchronized stream");
                    assert_eq!(error.details["stage"], "source-age");
                    assert_eq!(error.details["reason"], "source-stale");
                }
            }
            let record = *ring.last().unwrap();
            assert_eq!((record.outcome, record.stage, record.reason), (Outcome::Rejected, Stage::SourceAge, Reason::SourceStale));
            assert!(record.pipe_synchronized);
            assert_eq!(record.source_sequence, 1);
            for timing in [Timing::BeforeProof, Timing::ReadyHeader, Timing::Allocation, Timing::BodyRead, Timing::AfterProof, Timing::Total] {
                assert!(record.timing(timing).is_some(), "{timing:?} is timed even when the frame fails");
            }
            assert_eq!(helper.proofs(), 3, "stale pixels still require the independent after-proof");
            assert_eq!(stream.sequence, 1, "the consumed stale sequence must not be replayable");
            let next = stream.next(&client, &mut ring).await;
            if let Err(error) = &next {
                assert!(!stream.can_retry(error), "replayed headers cannot leave a retryable stream");
            }
            next
        };
        let (result, ()) = tokio::join!(consume, produce);
        result
    }, |_| panic!("live regression tests never request screenshots or mutations")).await
}

#[tokio::test]
async fn a_consumed_stale_frame_leaves_the_same_stream_ready_for_the_next_fresh_frame() {
    let frame = after_a_consumed_stale_frame(2).await.unwrap();
    assert_eq!(
        frame.rgb.as_slice(),
        &[42; 12],
        "only the new, fresh pixels escape verification"
    );
    assert_eq!((frame.width, frame.height), (2, 2));
}

#[tokio::test]
async fn a_consumed_stale_sequence_cannot_be_replayed_as_a_new_fresh_frame() {
    match after_a_consumed_stale_frame(1).await {
        Ok(_) => panic!("a consumed stale sequence must never become a fresh frame"),
        Err(error) => assert!(error.error.contains("sequence did not advance")),
    }
}

#[tokio::test]
async fn a_partially_received_pixel_body_never_leaves_a_retryable_stream() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) =
        crate::test_support::Helper::client(directory.path(), atlas_core::test_support::UID);
    helper.serve(async {
        let epoch = client.identity().await.unwrap().epoch;
        let (socket, mut producer) = UnixStream::pair().unwrap();
        let mut stream = opened(socket, &epoch);
        let mut ring = Ring::default();
        let produce = async move {
            let mut ready = [0];
            producer.read_exact(&mut ready).await.unwrap();
            assert_eq!(ready, [1]);
            let header = json!({"ok":true,"sequence":1,"daemonPID":epoch.capture_pid,
                "width":2,"height":2,"scale":epoch.display.scale,
                "session":{"ok":true,"uid":epoch.uid,"securitySessionID":epoch.security_session_id,
                    "displayID":epoch.display_id,"width":epoch.display.width,"height":epoch.display.height,
                    "hasGraphicAccess":true,"onConsole":false}});
            let bytes = serde_json::to_vec(&header).unwrap();
            producer.write_u32(bytes.len() as u32).await.unwrap();
            producer.write_all(&bytes).await.unwrap();
            producer.write_all(&[7; 6]).await.unwrap();
            // Drop with only half the promised twelve bytes delivered.
        };
        let (result, ()) = tokio::join!(stream.next(&client, &mut ring), produce);
        let error = match result { Ok(_) => panic!("a partial image must never escape"), Err(error) => error };
        assert!(!stream.can_retry(&error));
        assert!(!stream.can_retry(&Error::new(Code::StaleFrame, "the pipe is no longer synchronized")));
        assert_eq!(stream.sequence, 0, "an incomplete payload cannot consume a sequence");
        let record = ring.last().unwrap();
        assert_eq!((record.outcome, record.stage), (Outcome::Rejected, Stage::BodyRead));
        assert!(!record.pipe_synchronized);
        assert_eq!(record.timing(Timing::AfterProof), None, "no after-proof was taken");
        assert_eq!(helper.proofs(), 2, "there is no valid after-proof for an incomplete frame");
    }, |_| panic!("partial-frame tests never request captures or mutations")).await;
}

/// A stream on `socket` whose helper, this process standing in for its
/// daemon, confirms stops for generation 3.
fn confirming(socket: UnixStream, epoch: &Epoch) -> LiveStream {
    LiveStream {
        producer: Some(Served {
            daemon: std::process::id(),
            generation: 3,
            instance: None,
        }),
        ..opened(socket, epoch)
    }
}

fn hex32(id: &Value) -> bool {
    id.as_str().is_some_and(|id| {
        id.len() == 32
            && id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

#[tokio::test]
async fn a_stop_is_confirmed_by_the_drained_reply_to_its_own_request() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let (socket, mut producer) = UnixStream::pair().unwrap();
    let mut stream = confirming(socket, &epoch());
    let helper = async {
        let mut stop = [0; 9];
        producer.read_exact(&mut stop).await.unwrap();
        assert_eq!(stop[0], 0, "a stop request, never READY");
        let id = u64::from_be_bytes(stop[1..].try_into().unwrap());
        let reply = serde_json::to_vec(&json!({"ok": true, "daemonPID": std::process::id(),
            "producerGeneration": 3, "stopRequestID": id, "drained": true}))
        .unwrap();
        producer.write_u32(reply.len() as u32).await.unwrap();
        producer.write_all(&reply).await.unwrap();
    };
    let (stopped, ()) = tokio::join!(stream.stop(&client), helper);
    stopped.unwrap();
}

/// Stop `stream`, its stop going unanswered, while a stand-in daemon answers
/// each drain query with the next of `states`; the queries it saw.
async fn unanswered_stop(states: &[(&str, bool)], daemon: u32) -> (Result<()>, Vec<Value>) {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) =
        crate::test_support::Helper::client(directory.path(), atlas_core::test_support::UID);
    let (socket, mut producer) = UnixStream::pair().unwrap();
    let mut stream = confirming(socket, &epoch());
    if let Some(served) = &mut stream.producer {
        served.daemon = daemon;
    }
    let mut queries = Vec::new();
    let mut answers = states.iter();
    let stopping = async {
        // The stop arrives in full, and its answer never comes.
        let consume = async {
            let mut stop = [0; 9];
            producer.read_exact(&mut stop).await.unwrap();
            drop(producer);
        };
        let (stopped, ()) = tokio::join!(stream.stop(&client), consume);
        stopped
    };
    let stopped = helper
        .serve(stopping, |request| {
            assert_eq!(request["op"], "stream-drain-status");
            assert_eq!(request["producerGeneration"], 3);
            assert!(
                request.get("stopRequestID").is_none(),
                "no stop is claimed that the helper never answered"
            );
            queries.push(request["queryID"].clone());
            let (state, drained) = answers.next().copied().unwrap_or(("stopping", false));
            Some(
                json!({"ok": true, "daemonPID": daemon, "producerGeneration": 3,
                "queryID": request["queryID"], "state": state, "drained": drained}),
            )
        })
        .await;
    (stopped, queries)
}

#[tokio::test]
async fn a_lost_stop_answer_is_settled_only_by_fresh_queries_until_drained() {
    let (stopped, queries) = unanswered_stop(
        &[("active", false), ("stopping", false), ("stopped", true)],
        std::process::id(),
    )
    .await;
    stopped.unwrap();
    assert_eq!(queries.len(), 3);
    assert!(queries.iter().all(hex32));
    assert_ne!(queries[0], queries[1], "every query is fresh");
}

#[tokio::test]
async fn no_drain_claim_short_of_stopped_and_drained_settles_a_stop() {
    for states in [
        &[("quarantined", false)][..],
        &[("unknown", false)],
        &[("stopping", true)],
    ] {
        let (stopped, queries) = unanswered_stop(states, std::process::id()).await;
        assert!(stopped.is_err(), "{states:?}");
        if states[0].0 != "stopping" {
            assert_eq!(queries.len(), 1, "{states:?} is not asked again");
        }
    }
}

#[tokio::test]
async fn a_stop_still_settling_at_its_deadline_is_unconfirmed() {
    let started = std::time::Instant::now();
    let (stopped, queries) = unanswered_stop(&[], std::process::id()).await;
    assert!(stopped.is_err());
    assert!(
        queries.len() > 1,
        "it kept asking while the source was stopping"
    );
    assert!(
        started.elapsed() < DEADLINE + Duration::from_secs(1),
        "one deadline bounds it"
    );
}

#[tokio::test]
async fn only_the_daemon_that_ran_the_stream_can_confirm_its_drain() {
    let directory = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let mut client = crate::test_support::client(directory.path());
    client.transport.peer_uid = nix::unistd::getuid().as_raw();
    std::fs::create_dir_all(client.transport.path.parent().unwrap()).unwrap();
    // This process answers, but the stream was another daemon's.
    let listener = tokio::net::UnixListener::bind(&client.transport.path).unwrap();
    let (socket, producer) = UnixStream::pair().unwrap();
    drop(producer);
    let mut stream = confirming(socket, &epoch());
    if let Some(served) = &mut stream.producer {
        served.daemon = std::process::id() + 1;
    }
    let asked = async {
        let (mut peer, _) = listener.accept().await.unwrap();
        let mut sent = Vec::new();
        peer.read_to_end(&mut sent).await.unwrap();
        sent
    };
    let (stopped, sent) = tokio::join!(stream.stop(&client), asked);
    assert!(stopped.is_err());
    assert!(sent.is_empty(), "another process is never asked");
}

#[tokio::test]
async fn an_older_helper_has_only_the_closed_connection_to_go_by() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let (socket, mut producer) = UnixStream::pair().unwrap();
    let mut stream = opened(socket, &epoch());
    stream.stop(&client).await.unwrap();
    assert_eq!(
        producer.read(&mut [0]).await.unwrap(),
        0,
        "the connection closes with nothing sent, while the stream still exists"
    );
    drop(stream);
}
