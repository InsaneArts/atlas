use super::*;

fn refused(line: &str) -> Value {
    parse(line.as_bytes()).unwrap_err()
}

#[test]
fn requests_parse_into_operations() {
    assert_eq!(parse(br#"{"op":"hello"}"#), Ok(Request::Hello));
    assert_eq!(parse(br#"{"op":"apps"}"#), Ok(Request::Apps { pin: None }));
    assert_eq!(parse(br#"{"op":"restart"}"#), Ok(Request::Restart));
    assert_eq!(
        parse(br#"{"op":"logout"}"#),
        Ok(Request::Logout { force: false })
    );
    assert_eq!(
        parse(br#"{"op":"logout","force":true}"#),
        Ok(Request::Logout { force: true })
    );
    assert_eq!(
        refused(r#"{"op":"logout","force":"yes"}"#)["error"],
        "\"force\" must be true or false"
    );
    assert_eq!(
        parse(br#"{"op":"screenshot","maxDimension":320}"#),
        Ok(Request::Screenshot {
            max_dimension: Some(320),
            per_point: false,
            crop: None
        })
    );
    assert_eq!(
        parse(br#"{"op":"screenshot"}"#),
        Ok(Request::Screenshot {
            max_dimension: None,
            per_point: false,
            crop: None
        })
    );
    assert_eq!(
        parse(br#"{"op":"run","argv":["/bin/echo","hi"],"timeoutMs":3000}"#),
        Ok(Request::Run {
            argv: vec!["/bin/echo".into(), "hi".into()],
            timeout: Duration::from_millis(3000),
            pin: None
        })
    );

    let Ok(Request::Run { timeout, .. }) = parse(br#"{"op":"run","argv":["true"]}"#) else {
        panic!("run without a timeout");
    };

    assert_eq!(timeout, Duration::from_secs(30));
}

/// A run or a listing may name the helper session it was verified
/// in; anything but its two numbers is refused.
#[test]
fn a_read_may_name_the_session_it_is_for() {
    let pin = Some(SessionPin {
        security_session_id: 42,
        display_id: 7,
    });
    assert_eq!(
        parse(br#"{"op":"apps","session":{"securitySessionId":42,"displayId":7}}"#),
        Ok(Request::Apps { pin })
    );
    assert_eq!(
        parse(br#"{"op":"windows","dock":true,"session":{"securitySessionId":42,"displayId":7}}"#),
        Ok(Request::Windows { dock: true, pin })
    );
    let Ok(Request::Run { pin: run, .. }) =
        parse(br#"{"op":"run","argv":["true"],"session":{"securitySessionId":42,"displayId":7}}"#)
    else {
        panic!("a pinned run");
    };
    assert_eq!(run, pin);

    for session in [
        r#"{"securitySessionId":42}"#,
        r#"{"securitySessionId":"42","displayId":7}"#,
        r#"{"securitySessionId":42,"displayId":7,"uid":501}"#,
        r#"42"#,
    ] {
        for op in ["apps", "windows"] {
            let line = format!(r#"{{"op":"{op}","session":{session}}}"#);
            assert_eq!(refused(&line)["code"], "bad-request", "{session}");
        }
    }
}

#[test]
fn a_window_read_asks_for_the_dock_only_when_needed() {
    for dock in ["", ",\"dock\":null", ",\"dock\":false"] {
        let line = format!(r#"{{"op":"windows"{dock}}}"#);
        assert_eq!(
            parse(line.as_bytes()),
            Ok(Request::Windows {
                dock: false,
                pin: None
            })
        );
    }
    for dock in ["1", "\"yes\""] {
        let line = format!(r#"{{"op":"windows","dock":{dock}}}"#);
        assert_eq!(refused(&line)["error"], "\"dock\" must be true or false");
    }
}

/// The wording clients match on.
#[test]
fn refusals_keep_their_wording() {
    assert_eq!(
        refused(r#"{"notAnOp":true}"#),
        json!({"ok": false, "code": "bad-request", "error": "request is missing an \"op\" string", "uncertain": false})
    );
    assert_eq!(
        refused(r#"{"op":"nope"}"#),
        json!({"ok": false, "code": "bad-request", "error": "unknown op 'nope'", "uncertain": false})
    );
    assert_eq!(
        refused(r#"{"op":"run","argv":[],"timeoutMs":3000}"#),
        json!({"ok": false, "code": "bad-request", "error": "run requires a non-empty \"argv\" array", "uncertain": false})
    );
}

#[test]
fn run_and_job_arguments_keep_their_limits_and_error_precedence() {
    for (operation, prefix) in [
        ("run", json!({"op": "run"})),
        (
            "job start",
            json!({"op": "job", "action": "start", "uid": 502, "lifetimeMs": 5000}),
        ),
    ] {
        for (arguments, message) in [
            (json!([]), "requires a non-empty \"argv\" array"),
            (json!([""]), "requires a non-empty \"argv\" array"),
            (json!(["", 7]), "arguments must be strings without NUL"),
            (json!(["x\0"]), "arguments must be strings without NUL"),
            (
                json!(["x".repeat(MAX_ARGUMENT_BYTES + 1)]),
                "arguments must be strings without NUL",
            ),
            (
                json!(vec![Value::Null; MAX_ARGUMENTS + 1]),
                "accepts at most 4096 arguments",
            ),
        ] {
            let mut request = prefix.clone();
            request["argv"] = arguments;
            assert_eq!(
                parse(request.to_string().as_bytes()),
                Err(bad(format!("{operation} {message}")))
            );
        }
        let mut request = prefix;
        request["argv"] = json!(vec!["x"; MAX_ARGUMENTS]);
        assert!(parse(request.to_string().as_bytes()).is_ok());
        request["argv"] = json!(["x".repeat(MAX_ARGUMENT_BYTES)]);
        assert!(parse(request.to_string().as_bytes()).is_ok());
    }
}

#[cfg(target_os = "macos")]
#[test]
fn every_request_deliberately_selects_admission_and_lost_reply_handling() {
    let pin = json!({"securitySessionId": 42, "displayId": 7});
    for (mut request, hidden, pinned, mutation, label) in [
        (json!({"op": "hello"}), false, false, false, "hello"),
        (json!({"op": "apps"}), false, true, false, "apps"),
        (json!({"op": "windows"}), true, true, false, "windows"),
        (
            json!({"op": "screenshot"}),
            true,
            false,
            false,
            "screenshot",
        ),
        (
            json!({"op": "read", "path": "x", "maxSize": 1}),
            true,
            true,
            false,
            "read",
        ),
        (
            json!({"op": "run", "argv": ["true"]}),
            true,
            true,
            true,
            "run",
        ),
        (json!({"op": "restart"}), false, false, true, "restart"),
        (json!({"op": "logout"}), true, false, true, "logout"),
        (
            json!({"op": "job", "action": "start", "argv": ["true"], "uid": 502, "lifetimeMs": 5000}),
            true,
            true,
            true,
            "job-start",
        ),
        (
            json!({"op": "job", "action": "stop", "id": "7c2e91a04b3f", "expect": {"boot": "B", "pid": 4312, "start": [1, 2], "pgid": 4312}}),
            false,
            false,
            true,
            "job-stop",
        ),
        (
            json!({"op": "job", "action": "stop-all", "reason": "person"}),
            false,
            false,
            true,
            "job-stop",
        ),
        (
            json!({"op": "job", "action": "show"}),
            false,
            false,
            false,
            "job",
        ),
        (
            json!({"op": "job", "action": "log", "id": "7c2e91a04b3f", "bytes": 1}),
            false,
            false,
            false,
            "job",
        ),
        (json!({"op": "sound"}), false, false, false, "sound"),
        (
            json!({"op": "sound", "muted": false}),
            false,
            false,
            true,
            "sound",
        ),
    ] {
        request["session"] = pin.clone();
        let parsed = parse(request.to_string().as_bytes());
        assert!(parsed.is_ok(), "{request}");
        if let Ok(parsed) = parsed {
            let policy = parsed.policy();
            assert!(policy.allowed(false).is_none(), "{request}");
            assert_eq!(policy.allowed(true).is_some(), hidden, "{request}");
            assert_eq!(policy.pin.is_some(), pinned, "{request}");
            let phase = if mutation {
                ConnectionPhase::Mutation(label)
            } else {
                ConnectionPhase::Read(label)
            };
            assert_eq!(policy.phase, phase, "{request}");
        }
    }
    let host = Request::HostSession(Vec::new()).policy();
    assert!(host.allowed(true).is_none());
    assert!(host.pin.is_none());
    assert!(matches!(host.phase, ConnectionPhase::Read("host-session")));
}

#[test]
fn screenshots_parse_an_app_or_a_region() {
    let shot = |crop| {
        Ok(Request::Screenshot {
            max_dimension: None,
            per_point: false,
            crop,
        })
    };

    assert_eq!(
        parse(br#"{"op":"screenshot","pid":4242}"#),
        shot(Some(Crop::App { pid: 4242 }))
    );
    assert_eq!(
        parse(br#"{"op":"screenshot","region":{"x":-100,"y":80,"width":800,"height":600}}"#),
        shot(Some(Crop::Region(Region {
            x: -100,
            y: 80,
            width: 800,
            height: 600
        })))
    );
    assert_eq!(
        parse(br#"{"op":"screenshot","pid":null,"region":null}"#),
        shot(None)
    );
    assert_eq!(
        parse(br#"{"op":"screenshot","pid":7,"maxDimension":400}"#),
        Ok(Request::Screenshot {
            max_dimension: Some(400),
            per_point: false,
            crop: Some(Crop::App { pid: 7 })
        })
    );
    assert_eq!(
        refused(r#"{"op":"screenshot","pid":1,"region":{"x":0,"y":0,"width":1,"height":1}}"#)["error"],
        "give \"pid\" or \"region\", not both"
    );
    assert_eq!(
        refused(r#"{"op":"screenshot","pid":0}"#)["error"],
        "\"pid\" must be a process ID"
    );
    assert_eq!(
        refused(r#"{"op":"screenshot","region":{"x":0,"y":0,"width":0,"height":1}}"#)["error"],
        "\"region\" must be whole points: x and y -16384–16384, width and height 1–16384"
    );
}

#[test]
fn a_screenshot_may_ask_for_at_most_one_pixel_per_point() {
    assert_eq!(
        parse(br#"{"op":"screenshot","maxDimension":1456,"perPoint":true}"#),
        Ok(Request::Screenshot {
            max_dimension: Some(1456),
            per_point: true,
            crop: None
        })
    );
    assert_eq!(
        parse(br#"{"op":"screenshot","pid":7,"perPoint":false}"#),
        Ok(Request::Screenshot {
            max_dimension: None,
            per_point: false,
            crop: Some(Crop::App { pid: 7 })
        })
    );
    assert_eq!(
        parse(br#"{"op":"screenshot","perPoint":null}"#),
        Ok(Request::Screenshot {
            max_dimension: None,
            per_point: false,
            crop: None
        })
    );
    for line in [
        r#"{"op":"screenshot","perPoint":"yes"}"#,
        r#"{"op":"screenshot","perPoint":1}"#,
    ] {
        assert_eq!(
            refused(line)["error"],
            "\"perPoint\" must be true or false",
            "{line}"
        );
    }
}

#[test]
fn regions_widen_to_whole_points() {
    assert_eq!(
        Region::from_points(1.5, 2.25, 3.0, 4.0),
        Some(Region {
            x: 1,
            y: 2,
            width: 4,
            height: 5
        })
    );
    assert_eq!(
        Region::from_points(-50.0, -50.0, 200.0, 200.0),
        Some(Region {
            x: -50,
            y: -50,
            width: 200,
            height: 200
        })
    );
    assert_eq!(Region::from_points(0.0, 0.0, 16385.0, 10.0), None);
    assert_eq!(Region::from_points(16385.0, 0.0, 10.0, 10.0), None);
    assert_eq!(Region::from_points(-16384.5, 0.0, 10.0, 10.0), None);
    assert_eq!(Region::from_points(0.0, 0.0, f64::NAN, 10.0), None);
    assert_eq!(Region::from_points(0.0, 0.0, 1e300, 10.0), None);
}

#[test]
fn every_bound_is_enforced() {
    for line in [
        "not json",
        "[1]",
        r#"{"op":7}"#,
        r#"{"op":"run","argv":"/bin/echo"}"#,
        r#"{"op":"run","argv":[""]}"#,
        r#"{"op":"run","argv":[7]}"#,
        "{\"op\":\"run\",\"argv\":[\"a\\u0000b\"]}",
        r#"{"op":"run","argv":["true"],"timeoutMs":0}"#,
        r#"{"op":"run","argv":["true"],"timeoutMs":600001}"#,
        r#"{"op":"run","argv":["true"],"timeoutMs":"5"}"#,
        r#"{"op":"screenshot","maxDimension":0}"#,
        r#"{"op":"screenshot","maxDimension":8193}"#,
        r#"{"op":"screenshot","maxDimension":1.5}"#,
        r#"{"op":"screenshot","pid":0}"#,
        r#"{"op":"screenshot","pid":-1}"#,
        r#"{"op":"screenshot","pid":"5"}"#,
        r#"{"op":"screenshot","pid":2147483648}"#,
        r#"{"op":"screenshot","pid":5.5}"#,
        r#"{"op":"screenshot","region":{"x":0,"y":0,"width":10}}"#,
        r#"{"op":"screenshot","region":{"x":0,"y":0,"width":0,"height":10}}"#,
        r#"{"op":"screenshot","region":{"x":0,"y":0,"width":16385,"height":10}}"#,
        r#"{"op":"screenshot","region":{"x":-16385,"y":0,"width":10,"height":10}}"#,
        r#"{"op":"screenshot","region":{"x":0.5,"y":0,"width":10,"height":10}}"#,
        r#"{"op":"screenshot","region":"0,0,10,10"}"#,
        r#"{"op":"screenshot","region":[0,0,10,10]}"#,
        r#"{"op":"screenshot","pid":5,"region":{"x":0,"y":0,"width":10,"height":10}}"#,
    ] {
        assert_eq!(refused(line)["code"], "bad-request", "{line}");
    }

    let many = format!(
        r#"{{"op":"run","argv":[{}]}}"#,
        vec!["\"x\""; MAX_ARGUMENTS + 1].join(",")
    );
    assert_eq!(refused(&many)["code"], "bad-request");

    let huge = vec![b' '; MAX_REQUEST + 1];
    assert_eq!(parse(&huge).unwrap_err()["error"], "request exceeds 1 MiB");
}

const EXPECT: &str = r#"{"boot":"B","pid":4312,"start":[1,2],"pgid":4312}"#;

#[test]
fn job_requests_parse_within_their_bounds() {
    let start = parse(
        br#"{"op":"job","action":"start","argv":["/bin/sleep","9"],"lifetimeMs":3600000,"uid":502,"name":"web","task":"3f9a1c07b2e4","hostSession":"3f9a1c07b2e4"}"#,
    )
    .unwrap();
    let Request::Job(JobRequest::Start { start, uid, .. }) = start else {
        panic!("a start");
    };
    assert_eq!(start.argv, ["/bin/sleep", "9"]);
    assert_eq!(start.lifetime, Duration::from_secs(3600));
    assert_eq!(start.name.as_deref(), Some("web"));
    assert_eq!(start.task.unwrap().as_str(), "3f9a1c07b2e4");
    assert_eq!(uid, 502);

    let stop = format!(r#"{{"op":"job","action":"stop","id":"7c2e91a04b3f","expect":{EXPECT}}}"#);
    let Ok(Request::Job(JobRequest::Stop { grace, expect, .. })) = parse(stop.as_bytes()) else {
        panic!("a stop");
    };
    assert_eq!(grace, JOB_GRACE);
    assert_eq!(expect.pgid, 4312);

    assert_eq!(
        parse(br#"{"op":"job","action":"stop-all","reason":"on-console"}"#),
        Ok(Request::Job(JobRequest::StopAll {
            reason: StopReason::OnConsole
        }))
    );
    assert_eq!(
        parse(br#"{"op":"job","action":"show"}"#),
        Ok(Request::Job(JobRequest::Show { id: None }))
    );
    assert_eq!(
        parse(br#"{"op":"job","action":"log","id":"7c2e91a04b3f","bytes":16384}"#),
        Ok(Request::Job(JobRequest::Log {
            id: JobId::parse("7c2e91a04b3f").unwrap(),
            bytes: 16384
        }))
    );

    let stop = |rest: &str| {
        format!(r#"{{"op":"job","action":"stop","id":"7c2e91a04b3f","expect":{EXPECT}{rest}}}"#)
    };
    for line in [
        r#"{"op":"job","action":"start","argv":["x"],"lifetimeMs":999,"uid":502}"#.to_owned(),
        r#"{"op":"job","action":"start","argv":["x"],"lifetimeMs":86400001,"uid":502}"#.to_owned(),
        r#"{"op":"job","action":"start","argv":["x"],"lifetimeMs":5000}"#.to_owned(),
        r#"{"op":"job","action":"start","argv":[],"lifetimeMs":5000,"uid":502}"#.to_owned(),
        r#"{"op":"job","action":"start","argv":["x"],"lifetimeMs":5000,"uid":502,"name":""}"#.to_owned(),
        "{\"op\":\"job\",\"action\":\"start\",\"argv\":[\"x\"],\"lifetimeMs\":5000,\"uid\":502,\"name\":\"a\\nb\"}".to_owned(),
        r#"{"op":"job","action":"start","argv":["x"],"lifetimeMs":5000,"uid":502,"task":"ABC"}"#.to_owned(),
        stop(r#","graceMs":999"#),
        stop(r#","graceMs":60001"#),
        r#"{"op":"job","action":"log","id":"7c2e91a04b3f","bytes":0}"#.to_owned(),
        r#"{"op":"job","action":"log","id":"7c2e91a04b3f","bytes":1048577}"#.to_owned(),
        r#"{"op":"job","action":"stop-all","reason":"later"}"#.to_owned(),
        r#"{"op":"job"}"#.to_owned(),
    ] {
        assert_eq!(refused(&line)["code"], "bad-request", "{line}");
    }
}

#[test]
fn a_job_expectation_has_exactly_its_fields() {
    for expect in [
        r#"{"boot":"B","pid":4312,"start":[1,2]}"#,
        r#"{"boot":"B","pid":4312,"start":[1,2],"pgid":4312,"uid":502}"#,
        r#"{"boot":"B","pid":4312,"start":[1],"pgid":4312}"#,
    ] {
        let line =
            format!(r#"{{"op":"job","action":"stop","id":"7c2e91a04b3f","expect":{expect}}}"#);
        assert_eq!(refused(&line)["code"], "bad-request", "{expect}");
    }
}

#[test]
fn job_ids_and_keys_are_exact() {
    assert!(JobId::parse("7c2e91a04b3f").is_some());
    for id in [
        "7c2e91a04b3",
        "7c2e91a04b3f0",
        "7C2E91A04B3F",
        "7c2e91a04b3g",
    ] {
        assert!(JobId::parse(id).is_none(), "{id}");
        let line = format!(r#"{{"op":"job","action":"show","id":"{id}"}}"#);
        assert_eq!(refused(&line)["code"], "bad-request", "{id}");
    }
    assert_eq!(JobId::new().as_str().len(), JobId::LEN);
}

#[test]
fn unknown_job_actions_are_bad_requests() {
    let reply = refused(r#"{"op":"job","action":"stop-everything"}"#);
    assert_eq!(reply["error"], "unknown job action 'stop-everything'");
    assert_eq!(reply["uncertain"], false);
}

#[test]
fn read_requests_parse_within_their_bounds() {
    let Ok(Request::Read(read)) = parse(
        br#"{"op":"read","path":"atlas-work/a.bin","offset":8,"length":8388608,"maxSize":67108864}"#,
    ) else {
        panic!("a read");
    };
    assert_eq!(
        (read.offset, read.length, read.max_size, read.digest),
        (8, READ_RANGE_MAX, 64 << 20, false)
    );
    let Ok(Request::Read(digest)) = parse(
        br#"{"op":"read","path":"a","digest":true,"maxSize":1,"expect":{"dev":1,"ino":2,"size":3,"mtime":[4,5],"ctime":[6,7]}}"#,
    ) else {
        panic!("a digest");
    };
    assert!(digest.digest);
    assert_eq!(digest.expect.unwrap().size, 3);

    let long = format!(
        r#"{{"op":"read","path":"{}","maxSize":1}}"#,
        "a".repeat(1025)
    );
    for line in [
        r#"{"op":"read","maxSize":1}"#.to_owned(),
        r#"{"op":"read","path":"","maxSize":1}"#.to_owned(),
        long,
        r#"{"op":"read","path":"a","length":8388609,"maxSize":1}"#.to_owned(),
        r#"{"op":"read","path":"a","maxSize":0}"#.to_owned(),
        r#"{"op":"read","path":"a","maxSize":1073741825}"#.to_owned(),
        r#"{"op":"read","path":"a","digest":true,"length":5,"maxSize":1}"#.to_owned(),
        r#"{"op":"read","path":"a","maxSize":1,"expect":{"dev":1}}"#.to_owned(),
        "{\"op\":\"read\",\"path\":\"a\\u0000b\",\"maxSize\":1}".to_owned(),
    ] {
        assert_eq!(refused(&line)["code"], "bad-request", "{line}");
    }
}

#[test]
fn protocol_bounds_challenges_before_starting_any_worker() {
    assert_eq!(
        parse(br#"{"op":"session-proof","challenge":"abc-123"}"#).unwrap(),
        Request::SessionProof {
            challenge: "abc-123".into()
        }
    );
    for challenge in [
        json!(""),
        json!("x".repeat(129)),
        json!("a\nb"),
        json!("ა"),
        Value::Null,
    ] {
        assert!(
            parse(
                &serde_json::to_vec(&json!({"op":"session-proof","challenge":challenge})).unwrap()
            )
            .is_err()
        );
    }
}

#[test]
fn a_stream_requires_a_bounded_size_and_explicit_session_pin() {
    for side in [0, 319, 4097, u32::MAX] {
        assert!(parse(json!({"op":"stream","maxDimension":side,"session":{"securitySessionId":42,"displayId":7}}).to_string().as_bytes()).is_err());
    }
    assert!(parse(br#"{"op":"stream","maxDimension":640}"#).is_err());
    assert_eq!(parse(br#"{"op":"stream","maxDimension":640,"session":{"securitySessionId":42,"displayId":7}}"#).unwrap(),Request::Stream {
            max_dimension: 640,
            pin: SessionPin {
                security_session_id: 42,
                display_id: 7,
            },
            backend: CaptureBackend::CoreGraphics,
        });
}

#[test]
fn a_drain_query_names_one_generation_with_a_fresh_query_id() {
    let query = "0123456789abcdef0123456789abcdef";
    assert_eq!(
        parse(
            json!({"op": "stream-drain-status", "daemonPID": 77, "producerGeneration": 3,
                "queryID": query, "stopRequestID": 9})
            .to_string()
            .as_bytes()
        ),
        Ok(Request::StreamDrainStatus {
            daemon_pid: 77,
            producer_generation: 3,
            query_id: query.into(),
            stop_request: Some(9),
        })
    );
    for bad in [
        json!({"op": "stream-drain-status", "daemonPID": 77, "producerGeneration": 3, "queryID": "ABCDEF0123456789abcdef0123456789"}),
        json!({"op": "stream-drain-status", "daemonPID": 77, "producerGeneration": 0, "queryID": query}),
        json!({"op": "stream-drain-status", "daemonPID": 0, "producerGeneration": 3, "queryID": query}),
        json!({"op": "stream-drain-status", "producerGeneration": 3, "queryID": query}),
    ] {
        assert!(parse(bad.to_string().as_bytes()).is_err(), "{bad}");
    }
}

#[test]
fn screenshots_parse_one_window_or_an_apps_part_of_a_region_only_with_the_app() {
    let shot = |crop| {
        Ok(Request::Screenshot {
            max_dimension: None,
            per_point: false,
            crop: Some(crop),
        })
    };
    assert_eq!(
        parse(br#"{"op":"screenshot","pid":7,"window":31}"#),
        shot(Crop::Part {
            pid: 7,
            part: Part::Window(31)
        })
    );
    assert_eq!(
        parse(br#"{"op":"screenshot","pid":7,"within":{"x":0,"y":10,"width":20,"height":30}}"#),
        shot(Crop::Part {
            pid: 7,
            part: Part::Within(Region {
                x: 0,
                y: 10,
                width: 20,
                height: 30
            })
        })
    );
    for (request, says) in [
        (r#"{"op":"screenshot","window":31}"#, "with \"pid\" alone"),
        (
            r#"{"op":"screenshot","pid":7,"window":31,"within":{"x":0,"y":0,"width":1,"height":1}}"#,
            "with \"pid\" alone",
        ),
        (
            r#"{"op":"screenshot","region":{"x":0,"y":0,"width":1,"height":1},"within":{"x":0,"y":0,"width":1,"height":1}}"#,
            "with \"pid\" alone",
        ),
        (
            r#"{"op":"screenshot","pid":7,"window":0}"#,
            "\"window\" must be a window number",
        ),
        (
            r#"{"op":"screenshot","pid":7,"within":{"x":0}}"#,
            "\"within\" must be whole points",
        ),
    ] {
        let error = refused(request)["error"].as_str().unwrap().to_owned();
        assert!(error.contains(says), "{request}: {error}");
    }
}

#[test]
fn a_sound_request_reads_or_sets_whether_the_helper_is_muted() {
    assert_eq!(
        parse(br#"{"op":"sound"}"#),
        Ok(Request::Sound { muted: None })
    );
    assert_eq!(
        parse(br#"{"op":"sound","muted":true}"#),
        Ok(Request::Sound { muted: Some(true) })
    );
    assert_eq!(
        refused(r#"{"op":"sound","muted":"yes"}"#)["error"],
        "\"muted\" must be true or false"
    );
}
