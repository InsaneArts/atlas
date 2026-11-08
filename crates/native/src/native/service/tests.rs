use super::{
    Action, CAPABILITIES, Duration, Error, Expect, Gate, MAX_REQUEST, Perform, Request, Result,
    admitted, hold, page_host, read_request,
};
use crate::native::{
    ax::{Acted, Reached, Resolved},
    events::tap::{Posted, Seen, Tap},
    focus::Need,
};
use atlas_core::{
    actions::{ElementAction, Verification},
    error::Code,
    process::Instance,
    screen::elements::{LOOK_AGAIN, Reference, stale},
    session::Display,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    sync::{Arc, Weak},
    time::Instant,
};
use tokio::{io::AsyncWriteExt, net::UnixStream, sync::Mutex};

/// How long a test's lease lasts: past any delay in running a test on a
/// loaded machine, so only what a test does can end it.
const LASTING: Duration = Duration::from_secs(60 * 60);

/// How long a test waits on work that should finish at once before it
/// calls it stuck: far past any delay in running a test on a loaded machine.
const WATCHDOG: Duration = Duration::from_secs(60);

/// [`super::run`], the role of the field it named, a failure as the reply
/// it sends.
async fn run(
    engine: &mut impl Perform,
    gate: &Gate,
    actions: Vec<Action>,
    expect: Option<&Expect>,
) -> std::result::Result<Option<&'static str>, Value> {
    super::run(engine, gate, actions, expect)
        .await
        .map(|done| done.field)
        .map_err(|failure| failure.reply())
}

fn service(marker: &std::path::Path) -> super::Service {
    let epoch = atlas_core::session::InputEpoch {
        uid: 0,
        security_session_id: 0,
        display_id: 0,
        width: 1.0,
        height: 1.0,
        process_id: std::process::id(),
    };
    let engine = super::Engine::idle(epoch.clone());
    super::Service {
        looks: engine.looks.clone(),
        engine: Arc::new(Mutex::new(engine)),
        operations: super::Operations::default(),
        gate: Arc::new(Gate::default()),
        epoch: Arc::new(super::RwLock::new(epoch)),
        build: None,
        executable: None,
        stopping: tokio::sync::Notify::new(),
        stop_marker: marker.to_owned(),
        activity: marker.with_extension("activity"),
        seen: std::sync::Mutex::new(None),
    }
}

fn input_body(fields: &str) -> String {
    format!(
        r#"{{"op":"input","owner":"{}","epoch":{{"uid":501,"securitySessionId":1,"displayId":2,"width":800,"height":600,"processId":1}},{fields}}}"#,
        "a".repeat(48)
    )
}

const DISPLAY: Display = Display {
    width: 800.0,
    height: 600.0,
    scale: 1.0,
};

#[test]
fn input_requests_take_each_member_from_its_last_value() {
    let key = format!(
        "{:?}",
        Vec::<Action>::deserialize(json!([{"type":"key","key":"a"}])).unwrap()
    );
    for (options, expect, app) in [
        ("", Value::Null, false),
        (r#","app":false"#, Value::Null, false),
        (r#","app":true"#, Value::Null, true),
        (r#","app":null,"app":true"#, Value::Null, true),
        (r#","app":true,"app":null"#, Value::Null, false),
        (r#","app":"true""#, Value::Null, false),
        (r#","app":0"#, Value::Null, false),
        (r#","app":[]"#, Value::Null, false),
        (r#","app":{}"#, Value::Null, false),
        (r#","expect":null"#, Value::Null, false),
        (
            r#","expect":{"pid":812,"window":42,"field":"Search"}"#,
            json!({"pid":812,"window":42,"field":"Search"}),
            false,
        ),
        (
            r#","expect":{"pid":null,"pid":812}"#,
            json!({"pid":812}),
            false,
        ),
        (
            r#","expect":false,"expect":{"pid":812}"#,
            json!({"pid":812}),
            false,
        ),
    ] {
        let body = input_body(&format!(
            r#""actions":null,"actions":[{{"type":"key","key":null,"key":"a"}}]{options},"other":1,"other":2,"person":null,"person":false"#
        ));
        let Request::Input {
            owner,
            epoch,
            actions,
            expect: actual_expect,
            app: actual_app,
        } = Request::decode(body.as_bytes()).unwrap()
        else {
            panic!("expected typed input");
        };
        assert_eq!(owner, "a".repeat(48));
        assert_eq!(epoch.unwrap().width, DISPLAY.width);
        assert_eq!(format!("{:?}", actions.unwrap()), key, "{body}");
        assert_eq!(json!(actual_expect.unwrap()), expect, "{body}");
        assert_eq!(actual_app, app, "{body}");
    }
    let body = r#"{"op":"input","epoch":{"uid":501,"securitySessionId":1,"displayId":2,"width":null,"width":800,"height":600,"processId":1},"actions":[]}"#;
    let Request::Input { epoch, .. } = Request::decode(body.as_bytes()).unwrap() else {
        panic!("expected typed input");
    };
    assert_eq!(epoch.unwrap().width, DISPLAY.width);
    let body = input_body(r#""actions":[{"type":"key","key":"a"}],"op":null"#);
    assert_eq!(
        json!(Request::decode(body.as_bytes()).unwrap_err()),
        json!({"code":"input","error":"Native operation requires a string op","ok":false,"uncertain":false})
    );
}

#[test]
fn input_refusals_stay_on_their_own_field_and_refuse_the_whole_batch() {
    let batch = |action: &str| {
        input_body(&format!(
            r#""actions":[{{"type":"key","key":"a"}},{action}]"#
        ))
    };
    let expect = |expect: &str| {
        input_body(&format!(
            r#""actions":[{{"type":"key","key":"a"}}],"expect":{expect}"#
        ))
    };
    let epoch = |epoch: &str| {
        format!(r#"{{"op":"input","epoch":{epoch},"actions":[{{"type":"key","key":"a"}}]}}"#)
    };
    let refused = "Action 2 was refused and nothing was sent:";
    let refusals = [
        (
            input_body(r#""actions":null"#),
            "actions",
            "invalid-json",
            "invalid type: null, expected a sequence".to_owned(),
        ),
        (
            input_body(r#""actions":[{"type":"key","key":"a"}],"actions":null"#),
            "actions",
            "invalid-json",
            "invalid type: null, expected a sequence".into(),
        ),
        (
            input_body(r#""actions":[]"#),
            "actions",
            "input",
            "Send between 1 and 32 actions".into(),
        ),
        (
            input_body(&format!(
                r#""actions":{}"#,
                json!(vec![json!({"type":"key","key":"a"}); 33])
            )),
            "actions",
            "input",
            "Send between 1 and 32 actions".into(),
        ),
        (
            input_body(&format!(
                r#""actions":{}"#,
                json!(vec![json!({"type":"wait","ms":250}); 9])
            )),
            "actions",
            "input",
            "The batch would take longer than two seconds; send it in parts".into(),
        ),
        (
            batch(r#"{"type":"key","key":"a","key":null}"#),
            "actions",
            "invalid-json",
            "invalid type: null, expected a string".into(),
        ),
        (
            batch(r#"{"type":"move","x":10}"#),
            "actions",
            "invalid-json",
            "missing field `y`".into(),
        ),
        (
            batch(r#"{"type":"move","x":false,"y":20}"#),
            "actions",
            "invalid-json",
            "invalid type: boolean `false`, expected f64".into(),
        ),
        (
            batch(r#"{"type":"click","x":null,"y":20,"count":null}"#),
            "actions",
            "invalid-json",
            "invalid type: null, expected u8".into(),
        ),
        (
            batch(r#"{"type":"click","count":null,"y":20,"x":null}"#),
            "actions",
            "invalid-json",
            "invalid type: null, expected u8".into(),
        ),
        (
            batch(r#"{"type":"unknown"}"#),
            "actions",
            "invalid-json",
            "unknown variant `unknown`, expected one of `pointer`, `move`, `click`, `drag`, `scroll`, `type`, `key`, `wait`, `reference`, `press`, `set-value`, `check`, `uncheck`, `pick`, `expand`, `collapse`, `scroll-into-view`".into(),
        ),
        (
            batch(r#"{"type":"click","x":10,"y":20,"button":"invalid"}"#),
            "actions",
            "invalid-json",
            "unknown variant `invalid`, expected one of `left`, `middle`, `right`".into(),
        ),
        (
            batch(r#"{"type":"click","x":10,"y":20,"count":0}"#),
            "actions",
            "input",
            format!("{refused} a click is 1 to 5 clicks"),
        ),
        (
            batch(r#"{"type":"move","x":800,"y":20}"#),
            "actions",
            "input",
            format!("{refused} its point is off the display"),
        ),
        (
            batch(r#"{"type":"wait","ms":251}"#),
            "actions",
            "input",
            format!("{refused} a wait is at most 250 ms"),
        ),
        (
            batch(r#"{"type":"scroll","x":10,"y":20,"dx":101}"#),
            "actions",
            "input",
            format!("{refused} a scroll is at most 100 lines each way"),
        ),
        (
            expect("false"),
            "expect",
            "input",
            r#""expect" must name a process ID"#.into(),
        ),
        (
            expect(r#"{"pid":812,"pid":null}"#),
            "expect",
            "input",
            r#""expect" must name a process ID"#.into(),
        ),
        (
            expect(r#"{"pid":812,"window":0}"#),
            "expect",
            "input",
            r#""window" must be a window number"#.into(),
        ),
        (
            epoch("null"),
            "epoch",
            "invalid-json",
            "invalid type: null, expected struct InputEpoch".into(),
        ),
        (
            epoch(r#"{"uid":501,"securitySessionId":1,"displayId":2,"width":800,"height":600}"#),
            "epoch",
            "invalid-json",
            "missing field `processId`".into(),
        ),
        (
            epoch(
                r#"{"uid":false,"securitySessionId":1,"displayId":2,"width":800,"height":600,"processId":1}"#,
            ),
            "epoch",
            "invalid-json",
            "invalid type: boolean `false`, expected u32".into(),
        ),
        (
            epoch(
                r#"{"uid":501,"securitySessionId":1,"displayId":2,"width":800,"width":null,"height":600,"processId":1}"#,
            ),
            "epoch",
            "invalid-json",
            "invalid type: null, expected f64".into(),
        ),
    ];
    for (body, field, code, word) in refusals {
        let Request::Input {
            epoch,
            actions,
            expect,
            ..
        } = Request::decode(body.as_bytes()).unwrap()
        else {
            panic!("expected typed input for {body}");
        };
        let actions = actions.and_then(|actions| atlas_core::actions::check(&actions, &DISPLAY));
        let refusals: Vec<_> = [
            ("epoch", epoch.err()),
            ("actions", actions.err()),
            ("expect", expect.err()),
        ]
        .into_iter()
        .filter_map(|(name, error)| Some((name, json!(error?))))
        .collect();
        assert_eq!(
            refusals,
            [(
                field,
                json!({"code":code,"error":word,"ok":false,"uncertain":false})
            )],
            "{body}"
        );
    }
}

#[tokio::test]
async fn input_availability_still_precedes_deferred_field_refusals() {
    let directory = tempfile::tempdir().unwrap();
    let service = service(&directory.path().join("stopped"));
    let owner = "a".repeat(48);
    let grant = service
        .gate
        .acquire_lasting(&owner, super::Who::Agent, LASTING)
        .unwrap();
    service.operations.set_failed(true);
    for fields in [
        r#""actions":null"#,
        r#""actions":[{"type":"click","x":null,"y":20}]"#,
        r#""actions":[{"type":"key","key":"a"}],"expect":false"#,
    ] {
        let mut wire: Value = serde_json::from_str(&input_body(fields)).unwrap();
        for epoch in [wire["epoch"].clone(), Value::Null, json!({"uid":false})] {
            wire["epoch"] = epoch;
            let request = Request::decode(wire.to_string().as_bytes()).unwrap();
            let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
            let error = service.dispatch(request, &phase).await.unwrap_err();
            assert_eq!(json!(error), json!(super::worker_failed()));
            assert!(matches!(
                *observed.borrow(),
                super::ConnectionPhase::Refusal
            ));
            assert!(grant.authorize().is_ok());
            assert_eq!(service.operations.registered(), 0);
            let engine = service.engine.lock().await;
            assert_eq!(engine.posted, 0);
            assert!(!engine.held_input());
        }
    }
}

#[tokio::test]
async fn unknown_operations_keep_availability_and_epoch_refusals()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let service = service(&directory.path().join("stopped"));
    let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
    service.operations.set_failed(true);
    for fields in [
        "",
        r#", "epoch":null"#,
        r#", "epoch":false, "owner":null, "actions":null"#,
        r#", "epoch":{}, "epoch":false"#,
    ] {
        let body = format!(r#"{{"op":"unknown"{fields}}}"#);
        let request = Request::decode(body.as_bytes())?;
        let error = service
            .dispatch(request, &phase)
            .await
            .err()
            .ok_or("expected refusal")?;
        assert_eq!(json!(error), json!(super::worker_failed()), "{body}");
        assert!(matches!(
            *observed.borrow(),
            super::ConnectionPhase::Refusal
        ));
    }
    service.operations.set_failed(false);
    for epoch in [Value::Null, json!(false), json!({})] {
        let body = json!({"op":"unknown","epoch":epoch}).to_string();
        let Request::Unknown { epoch: actual } = Request::decode(body.as_bytes())? else {
            panic!("expected unknown operation");
        };
        let expected = atlas_core::session::InputEpoch::deserialize(&epoch)
            .err()
            .ok_or("expected refusal")?;
        assert_eq!(
            json!(actual.err().ok_or("expected refusal")?),
            json!(Error::from(expected))
        );
    }
    let mut stale = service.epoch()?;
    stale.display_id += 1;
    let error = service.check_epoch(stale).err().ok_or("expected refusal")?;
    assert_eq!(error.code, Code::SessionChanged);
    assert!(!error.uncertain);
    Ok(())
}

#[test]
fn only_a_literal_true_person_flag_selects_personal_control()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let service = service(&directory.path().join("stopped"));
    let epoch = serde_json::to_string(&service.epoch()?)?;
    for person in ["null", r#""true""#, "0", "1", "[]", "{}", "false", "true"] {
        let body = format!(
            r#"{{"op":"acquire","owner":"{}","epoch":{epoch},"person":{person}}}"#,
            "a".repeat(48)
        );
        let Request::Acquire { who, epoch, .. } = Request::decode(body.as_bytes())? else {
            panic!("expected acquire");
        };
        assert_eq!(epoch?, service.epoch()?);
        assert_eq!(
            who,
            if person == "true" {
                super::Who::Person
            } else {
                super::Who::Agent
            }
        );
        assert!(!service.gate.active()?);
    }
    Ok(())
}

#[test]
fn an_absent_person_flag_remains_an_agent() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let service = service(&directory.path().join("stopped"));
    let epoch = serde_json::to_string(&service.epoch()?)?;
    let owner = "a".repeat(48);
    for (person, expected) in [
        ("", super::Who::Agent),
        (r#", "person":false"#, super::Who::Agent),
        (r#", "person":true"#, super::Who::Person),
    ] {
        let body = format!(r#"{{"epoch":{epoch},"owner":"{owner}"{person},"op":"acquire"}}"#);
        let Request::Acquire {
            owner,
            epoch: actual,
            who,
        } = Request::decode(body.as_bytes())?
        else {
            panic!("expected typed acquire");
        };
        assert_eq!(actual?, service.epoch()?);
        assert_eq!(who, expected);
        let gate = Arc::new(Gate::default());
        gate.hold(true)?;
        let grant = gate.acquire_lasting(&owner, who, LASTING);
        if who == super::Who::Agent {
            assert_eq!(
                grant.err().ok_or("expected stopped grant")?.code,
                Code::Stopped
            );
            assert!(!gate.active()?);
        } else {
            assert!(grant?.authorize().is_ok());
            assert_eq!(gate.holder()?, Some(atlas_core::error::Holder::Person));
        }
    }
    Ok(())
}

#[test]
fn acquire_and_reset_ignore_only_unrelated_extensions()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let epoch =
        r#"{"uid":501,"securitySessionId":1,"displayId":2,"width":800,"height":600,"processId":1}"#;
    let body =
        format!(r#"{{"op":"acquire","epoch":{epoch},"other":1,"other":2,"on":null,"on":false}}"#);
    let Request::Acquire { owner, who, .. } = Request::decode(body.as_bytes())? else {
        panic!("expected typed acquire");
    };
    assert_eq!(owner, "");
    assert_eq!(who, super::Who::Agent);
    for owner in ["", "not-an-owner", &"a".repeat(48)] {
        let body = format!(r#"{{"op":"acquire","owner":"{owner}","epoch":{epoch}}}"#);
        let Request::Acquire { owner: actual, .. } = Request::decode(body.as_bytes())? else {
            panic!("the gate checks token identity");
        };
        assert_eq!(actual, owner);
    }
    let body = format!(
        r#"{{"op":"reset","epoch":{epoch},"owner":null,"owner":false,"person":[],"person":{{}},"other":1,"other":2}}"#
    );
    assert!(matches!(
        Request::decode(body.as_bytes())?,
        Request::Reset { .. }
    ));
    // InputEpoch's existing sequence representation remains compatible.
    for op in ["acquire", "reset"] {
        let body = format!(r#"{{"op":"{op}","epoch":[501,1,2,800,600,1]}}"#);
        let (Request::Acquire { epoch: actual, .. } | Request::Reset { epoch: actual }) =
            Request::decode(body.as_bytes())?
        else {
            panic!("expected a typed epoch");
        };
        assert_eq!(
            actual?,
            atlas_core::session::InputEpoch::deserialize(serde_json::from_str::<Value>(epoch)?)?
        );
    }
    Ok(())
}

#[test]
fn epoch_refusals_keep_their_existing_words_and_order() {
    let fixtures = [
        (
            "[501,1,2,800,600,1,0]",
            "invalid length 7, expected fewer elements in array",
        ),
        (
            r#"{"z":0,"a":0}"#,
            "unknown field `a`, expected one of `uid`, `securitySessionId`, `displayId`, `width`, `height`, `processId`",
        ),
        (
            r#"{"uid":null,"displayId":"wrong"}"#,
            "invalid type: string \"wrong\", expected u64",
        ),
        (r#"{"z":0,"uid":null}"#, "invalid type: null, expected u32"),
        ("null", "invalid type: null, expected struct InputEpoch"),
        (
            "[]",
            "invalid length 0, expected struct InputEpoch with 6 elements",
        ),
        ("{}", "missing field `uid`"),
    ];
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for op in ["acquire", "reset"] {
        for (epoch, message) in fixtures {
            let value = serde_json::from_str::<Value>(epoch).unwrap();
            let legacy = atlas_core::session::InputEpoch::deserialize(&value).unwrap_err();
            assert_eq!(legacy.to_string(), message);
            let body = format!(r#"{{"op":"{op}","epoch":{epoch}}}"#);
            let error = match Request::decode(body.as_bytes()) {
                Err(error) => json!(error),
                Ok(
                    Request::Acquire {
                        epoch: Err(error), ..
                    }
                    | Request::Reset { epoch: Err(error) },
                ) => json!(error),
                request => panic!("expected an epoch refusal for {body}: {request:?}"),
            };
            actual.push((body.clone(), error));
            expected.push((
                body,
                json!({
                    "code": "invalid-json",
                    "error": message,
                    "ok": false,
                    "uncertain": false,
                }),
            ));
        }
    }
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn typed_acquire_and_reset_still_require_fresh_authority() {
    let directory = tempfile::tempdir().unwrap();
    let service = service(&directory.path().join("stopped"));
    let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
    let epoch = service.epoch().unwrap();
    for who in [super::Who::Agent, super::Who::Person] {
        let grant = service
            .gate
            .acquire_lasting(&"a".repeat(48), who, LASTING)
            .unwrap();
        for member in [
            "uid",
            "securitySessionId",
            "displayId",
            "width",
            "height",
            "processId",
        ] {
            let mut stale = json!(epoch);
            stale[member] = json!(2);
            let stale = atlas_core::session::InputEpoch::deserialize(stale).unwrap();
            assert_ne!(stale, epoch);
            for request in [
                Request::Acquire {
                    owner: "wrong-owner".into(),
                    epoch: Ok(stale.clone()),
                    who,
                },
                Request::Reset { epoch: Ok(stale) },
            ] {
                let error = service.dispatch(request, &phase).await.unwrap_err();
                assert_eq!(error.code, Code::Identity);
                assert!(!error.uncertain);
                assert!(grant.authorize().is_ok());
                assert_eq!(service.epoch().unwrap(), epoch);
                assert_eq!(service.engine.lock().await.posted, 0);
                assert!(matches!(
                    *observed.borrow(),
                    super::ConnectionPhase::Refusal
                ));
            }
        }
        service.gate.cancel(None).unwrap();
    }
    service.operations.set_failed(true);
    let request = Request::decode(br#"{"op":"acquire","epoch":null}"#).unwrap();
    let error = service.dispatch(request, &phase).await.unwrap_err();
    assert_eq!(json!(error), json!(super::worker_failed()));
}

#[tokio::test]
async fn an_acquire_cannot_pass_cleanup_still_in_progress() {
    for who in [super::Who::Agent, super::Who::Person] {
        let operations = super::Operations::default();
        let gate = Arc::new(Gate::default());
        let old = gate.acquire_lasting(&"a".repeat(48), who, LASTING).unwrap();
        let stand = Stand::locked(&[]);
        let engine = stand.clone().lock_owned().await;
        let (resume, barrier) = tokio::sync::oneshot::channel::<()>();
        let running = operations
            .start(async move {
                barrier.await.unwrap();
                drop(engine);
            })
            .await
            .unwrap();
        gate.cancel(None).unwrap();
        assert!(old.authorize().is_err());
        let cleanup = super::drain_and_release(&operations, &stand, &gate);
        let acquire = async {
            let mut engine = stand.lock().await;
            operations.check()?;
            engine.release()?;
            gate.acquire_lasting(&"b".repeat(48), who, LASTING)
        };
        tokio::pin!(cleanup, acquire);
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(cleanup.as_mut(), cx).is_pending());
            assert!(std::future::Future::poll(acquire.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        assert!(!gate.active().unwrap());
        assert_eq!(operations.registered(), 1);
        resume.send(()).unwrap();
        let (cleaned, grant) = tokio::join!(cleanup, acquire);
        cleaned.unwrap();
        assert!(grant.unwrap().authorize().is_ok());
        running.await.unwrap();
    }
}

fn malformed_acquire_and_reset_requests() -> Vec<String> {
    let epoch = r#"{"uid":501,"securitySessionId":1,"displayId":2,"width":800.0,"height":600.0,"processId":1}"#;
    let owner = "a".repeat(48);
    let mut cases = Vec::new();
    for op in ["acquire", "reset"] {
        let prefix = format!(r#""op":"{op}","owner":"{owner}""#);
        cases.push(format!("{{{prefix}}}"));
        for value in ["null", "false", "0", r#""epoch""#, "[]", "{}"] {
            cases.push(format!(r#"{{{prefix},"epoch":{value}}}"#));
        }
        for member in ["op", "epoch"] {
            let value = if member == "op" {
                format!(r#""{op}""#)
            } else {
                epoch.to_owned()
            };
            for (first, second) in [
                (value.as_str(), value.as_str()),
                ("null", value.as_str()),
                (value.as_str(), "null"),
            ] {
                let fields = if member == "op" {
                    format!(r#""owner":"{owner}","epoch":{epoch}"#)
                } else {
                    prefix.clone()
                };
                cases.push(format!(
                    r#"{{{fields},"{member}":{first},"{member}":{second}}}"#
                ));
            }
        }
        let fields = serde_json::from_str::<serde_json::Map<String, Value>>(epoch).unwrap();
        for (member, valid) in &fields {
            let mut missing = fields.clone();
            missing.remove(member);
            cases.push(format!(r#"{{{prefix},"epoch":{}}}"#, json!(missing)));
            let mut invalid = vec!["null", "false", r#""1""#, "[]", "{}"];
            if member != "width" && member != "height" {
                invalid.extend(["-1", "1.0", "18446744073709551616"]);
            }
            if member == "uid" || member == "processId" {
                invalid.push("4294967296");
            }
            for value in invalid {
                let mut malformed = fields.clone();
                malformed.insert(member.clone(), serde_json::from_str(value).unwrap());
                cases.push(format!(r#"{{{prefix},"epoch":{}}}"#, json!(malformed)));
            }
            for (first, second) in [
                (valid.to_string(), valid.to_string()),
                ("null".into(), valid.to_string()),
                (valid.to_string(), "null".into()),
            ] {
                cases.push(format!(
                    r#"{{{prefix},"epoch":{{"{member}":{first},"{member}":{second},{}}}}}"#,
                    serde_json::to_string(&missing)
                        .unwrap()
                        .trim_start_matches('{')
                        .trim_end_matches('}')
                ));
            }
            let escaped = format!(r#"\u{:04x}{}"#, member.as_bytes()[0], &member[1..]);
            cases.push(format!(
                r#"{{{prefix},"epoch":{{"{escaped}":{valid},{}}}}}"#,
                epoch.trim_start_matches('{').trim_end_matches('}')
            ));
        }
        cases.push(format!(
            r#"{{{prefix},"epoch":{{"extension":1,{}}}}}"#,
            epoch.trim_start_matches('{').trim_end_matches('}')
        ));
    }
    for member in ["owner", "person"] {
        for value in ["null", "0", "1", "[]", "{}"] {
            let other = if member == "person" {
                format!(r#""owner":"{owner}""#)
            } else {
                r#""person":false"#.to_owned()
            };
            cases.push(format!(
                r#"{{"op":"acquire","epoch":{epoch},{other},"{member}":{value}}}"#
            ));
        }
        let values = if member == "owner" {
            vec![format!(r#""{owner}""#), "false".into(), "null".into()]
        } else {
            vec![
                "false".into(),
                "true".into(),
                r#""true""#.into(),
                "null".into(),
            ]
        };
        for first in &values {
            for second in &values {
                let other = if member == "person" {
                    format!(r#""owner":"{owner}","epoch":{epoch}"#)
                } else {
                    format!(r#""epoch":{epoch}"#)
                };
                cases.push(format!(
                    r#"{{"op":"acquire",{other},"{member}":{first},"{member}":{second}}}"#
                ));
            }
        }
        let value = if member == "owner" {
            "false"
        } else {
            r#""false""#
        };
        cases.push(format!(
            r#"{{"op":"acquire","epoch":{epoch},"{member}":{value}}}"#
        ));
        let escaped = if member == "owner" {
            r#"\u006fwner"#
        } else {
            r#"\u0070erson"#
        };
        let value = if member == "owner" {
            format!(r#""{owner}""#)
        } else {
            "true".into()
        };
        cases.push(format!(
            r#"{{"op":"acquire","epoch":{epoch},"{member}":{value},"{escaped}":{value}}}"#
        ));
    }
    cases
}

#[tokio::test]
async fn malformed_acquire_and_reset_fields_leave_ownership_and_recovery_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    for who in [super::Who::Agent, super::Who::Person] {
        for failed in [false, true] {
            let marker = directory.path().join("stopped");
            let service = service(&marker);
            let grant = service
                .gate
                .acquire_lasting(&"a".repeat(48), who, LASTING)
                .unwrap();
            let (resume, barrier) = tokio::sync::oneshot::channel::<()>();
            let running = service.operations.start(barrier).await.unwrap();
            service.operations.set_failed(failed);
            let _recovery = service.operations.hold_recovery().await;
            let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
            for body in malformed_acquire_and_reset_requests() {
                match Request::decode(body.as_bytes()) {
                    Ok(request) => {
                        let error = service.dispatch(request, &phase).await.unwrap_err();
                        let Ok(wire) = serde_json::from_str::<Value>(&body) else {
                            panic!("invalid fixture");
                        };
                        if wire["op"] == "acquire" {
                            assert_eq!(json!(error), json!(service.check().unwrap_err()), "{body}");
                        } else if let Err(expected) =
                            atlas_core::session::InputEpoch::deserialize(&wire["epoch"])
                        {
                            assert_eq!(json!(error), json!(Error::from(expected)), "{body}");
                        }
                    }
                    Err(error) => {
                        assert!(
                            matches!(error.code, Code::InvalidJson | Code::Input),
                            "{body}"
                        );
                        assert!(!error.uncertain, "{body}");
                    }
                }
                assert!(grant.authorize().is_ok(), "{body}");
                assert!(!service.gate.held().unwrap(), "{body}");
                assert!(!marker.exists(), "{body}");
                let engine = service.engine.lock().await;
                assert_eq!(engine.posted, 0, "{body}");
                assert!(!engine.held_input(), "{body}");
                assert!(engine.grant.is_none(), "{body}");
                assert_eq!(engine.epoch, service.epoch().unwrap(), "{body}");
                assert_eq!(service.operations.has_failed(), failed, "{body}");
                assert_eq!(service.operations.registered(), 1, "{body}");
                assert!(service.operations.recovering(), "{body}");
                assert!(
                    matches!(*observed.borrow(), super::ConnectionPhase::Refusal),
                    "{body}"
                );
            }
            resume.send(()).unwrap();
            running.await.unwrap().unwrap();
            service.operations.set_failed(false);
            service.operations.drain().await.unwrap();
        }
    }
}

#[tokio::test]
async fn lifecycle_members_preserve_last_value_refusals_and_effects()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let marker = directory.path().join("stopped");
    let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
    let pid = std::process::id();
    let mut cases = Vec::new();
    for fields in [
        "",
        r#""op":null"#,
        r#""op":false"#,
        r#""op":0"#,
        r#""op":[]"#,
        r#""op":{}"#,
        r#""op":"hold","op":"hello","on":true"#,
        r#""op":"hello","op":"hold","on":true"#,
        r#""op":"hello","op":"hello""#,
        r#""op":null,"op":"hello""#,
        r#""op":"hello","op":null"#,
        r#""op":"hello","\u006fp":"hello""#,
    ] {
        cases.push(format!("{{{fields}}}\n"));
    }
    for fields in [
        "",
        r#", "pid":null"#,
        r#", "pid":false"#,
        r#", "pid":"1""#,
        r#", "pid":-1"#,
        r#", "pid":1.0"#,
        r#", "pid":[]"#,
        r#", "pid":{}"#,
    ] {
        cases.push(format!(r#"{{"op":"shutdown"{fields}}}"#));
    }
    for fields in [
        format!(r#""pid":{pid},"pid":0"#),
        format!(r#""pid":0,"pid":{pid}"#),
        format!(r#""pid":{pid},"pid":{pid}"#),
        format!(r#""pid":{pid},"\u0070id":{pid}"#),
    ] {
        cases.push(format!(r#"{{"op":"shutdown",{fields}}}"#));
    }
    for operation in ["release", "renew"] {
        for fields in [
            r#""owner":"a","owner":"b""#,
            r#""owner":"b","owner":"a""#,
            r#""owner":"a","owner":"a""#,
            r#""owner":null,"owner":"a""#,
            r#""owner":"a","owner":null"#,
            r#""owner":"a","\u006fwner":"a""#,
        ] {
            cases.push(format!(r#"{{"op":"{operation}",{fields}}}"#));
        }
    }
    for stopped in [false, true] {
        for body in &cases {
            let parsed: Value = serde_json::from_str(body)?;
            let canonical = parsed.to_string();
            let reference_marker = directory.path().join("reference");
            let reference = self::service(&reference_marker);
            let actual = self::service(&marker);
            for (instance, path) in [(&reference, &reference_marker), (&actual, &marker)] {
                instance
                    .gate
                    .acquire_lasting(&"a".repeat(48), super::Who::Person, LASTING)?;
                instance.gate.hold(stopped)?;
                if stopped {
                    std::fs::write(path, b"stop")?;
                } else if path.exists() {
                    std::fs::remove_file(path)?;
                }
            }
            let (reference_phase, _) = super::watch::channel(super::ConnectionPhase::Refusal);
            let reference_result = match Request::decode(canonical.as_bytes()) {
                Ok(request) => reference.dispatch(request, &reference_phase).await,
                Err(error) => Err(error),
            };
            let actual_result = match deliver(format!("{body}\n").into_bytes(), 64).await {
                Ok(request) => actual.dispatch(request, &phase).await,
                Err(error) => Err(error),
            };
            let refusal = match parsed["op"].as_str() {
                None => Some((Code::Input, "Native operation requires a string op")),
                Some("shutdown") if parsed["pid"].as_u64() != Some(u64::from(pid)) => Some((
                    Code::Identity,
                    "Shutdown names a different input service process",
                )),
                _ => None,
            };
            if let Some((code, message)) = refusal {
                let Err(error) = &actual_result else {
                    panic!("invalid lifecycle field was accepted: {body}");
                };
                assert_eq!(error.code, code, "{body}");
                assert_eq!(error.error, message, "{body}");
                assert!(!error.uncertain, "{body}");
            }
            assert_eq!(json!(actual_result), json!(reference_result), "{body}");
            assert_eq!(actual.gate.active()?, reference.gate.active()?, "{body}");
            assert_eq!(actual.gate.held()?, reference.gate.held()?, "{body}");
            assert_eq!(marker.exists(), reference_marker.exists(), "{body}");
            let engine = actual.engine.lock().await;
            assert_eq!(engine.posted, 0, "{body}");
            assert!(!engine.held_input(), "{body}");
            if matches!(*observed.borrow(), super::ConnectionPhase::Refusal) {
                assert!(actual.gate.active()?, "{body}");
                assert_eq!(actual.gate.held()?, stopped, "{body}");
            }
            phase.send_replace(super::ConnectionPhase::Refusal);
        }
    }
    Ok(())
}

#[tokio::test]
async fn array_shaped_lifecycle_requests_are_refused_without_effects() {
    let owner = "a".repeat(48);
    let cases = [
        r#"["hello"]"#.to_owned(),
        format!(r#"["shutdown",{}]"#, std::process::id()),
        r#"["hold",true]"#.to_owned(),
        r#"["hold",false]"#.to_owned(),
        format!(r#"["release","{owner}"]"#),
        format!(r#"["renew","{owner}"]"#),
    ];
    for owned in [false, true] {
        for stopped in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let marker = directory.path().join("stopped");
            let service = service(&marker);
            let grant = owned.then(|| {
                service
                    .gate
                    .acquire_lasting(&owner, super::Who::Person, LASTING)
                    .unwrap()
            });
            service.gate.hold(stopped).unwrap();
            if stopped {
                std::fs::write(&marker, b"stop").unwrap();
            }
            let epoch = service.epoch().unwrap();
            let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
            for body in &cases {
                let result = match deliver(format!(" \t{body}\n").into_bytes(), 1).await {
                    Ok(request) => service.dispatch(request, &phase).await,
                    Err(error) => Err(error),
                };
                let error = result.unwrap_err();
                assert_eq!(error.code, Code::Input, "{body}");
                assert_eq!(
                    error.error, "Native operation requires a string op",
                    "{body}"
                );
                assert!(!error.uncertain, "{body}");
                assert!(
                    matches!(*observed.borrow(), super::ConnectionPhase::Refusal),
                    "{body}"
                );
                assert_eq!(service.gate.active().unwrap(), owned, "{body}");
                if let Some(grant) = &grant {
                    assert!(grant.authorize().is_ok(), "{body}");
                }
                assert_eq!(service.gate.held().unwrap(), stopped, "{body}");
                assert_eq!(service.epoch().unwrap(), epoch, "{body}");
                assert_eq!(service.engine.lock().await.posted, 0, "{body}");
                assert!(service.operations.check().is_ok(), "{body}");
                if stopped {
                    assert_eq!(std::fs::read(&marker).unwrap(), b"stop", "{body}");
                } else {
                    assert!(!marker.exists(), "{body}");
                }
            }
        }
    }
}

#[tokio::test]
async fn lifecycle_fields_keep_their_operation_specific_refusals() {
    let directory = tempfile::tempdir().unwrap();
    let service = service(&directory.path().join("stopped"));
    let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
    let grant = service
        .gate
        .acquire_lasting(&"a".repeat(48), super::Who::Agent, LASTING)
        .unwrap();
    for fields in [
        "",
        r#", "owner":null"#,
        r#", "owner":true"#,
        r#", "owner":1"#,
        r#", "owner":[]"#,
        r#", "owner":{}"#,
        r#", "owner":"""#,
        r#", "owner":"not-an-owner""#,
    ] {
        for op in ["release", "renew"] {
            let request =
                Request::decode(format!(r#"{{"op":"{op}"{fields}}}"#).as_bytes()).unwrap();
            let error = service.dispatch(request, &phase).await.unwrap_err();
            if op == "release" {
                assert_eq!(error.code, Code::LeaseLost);
                assert_eq!(error.error, "This token cannot release another controller");
            } else {
                assert_eq!(json!(error), json!(service.check().unwrap_err()));
            }
            assert!(!error.uncertain);
            assert!(grant.authorize().is_ok());
            assert!(matches!(
                *observed.borrow(),
                super::ConnectionPhase::Refusal
            ));
        }
    }
    let request = Request::decode(br#"{"op":"shutdown","pid":0}"#).unwrap();
    let error = service.dispatch(request, &phase).await.unwrap_err();
    assert_eq!(error.code, Code::Identity);
    assert_eq!(
        error.error,
        "Shutdown names a different input service process"
    );
    let request = Request::Shutdown {
        pid: std::process::id().into(),
    };
    let error = service.dispatch(request, &phase).await.unwrap_err();
    assert_eq!(error.code, Code::Busy);
    assert!(grant.authorize().is_ok());
}

#[test]
fn lifecycle_payloads_ignore_only_unrelated_extension_members() {
    assert!(matches!(
        Request::decode(br#"{"op":"hello","on":null,"on":true,"pid":0,"pid":1}"#).unwrap(),
        Request::Hello
    ));
    assert!(matches!(
        Request::decode(br#"{"pid":42,"on":null,"on":false,"op":"shutdown"}"#).unwrap(),
        Request::Shutdown { pid: 42 }
    ));
    assert!(matches!(
        Request::decode(br#"{"on":true,"pid":null,"pid":false,"op":"hold"}"#).unwrap(),
        Request::Hold { on: Ok(true) }
    ));
    for op in ["release", "renew"] {
        let body = format!(
            r#"{{"owner":"unvalidated-token","on":null,"on":false,"op":"{op}","other":1,"other":2}}"#
        );
        let (Request::Release { owner } | Request::Renew { owner }) =
            Request::decode(body.as_bytes()).unwrap()
        else {
            panic!("expected an owner");
        };
        assert_eq!(owner, "unvalidated-token", "the gate owns token identity");
    }
}

#[tokio::test]
async fn stop_and_release_remain_available_when_ordinary_permission_checks_fail() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("stopped");
    let service = service(&marker);
    let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
    assert!(
        service.check().is_err(),
        "the synthetic session is never authorized"
    );
    let owner = "a".repeat(48);
    let grant = service
        .gate
        .acquire_lasting(&owner, super::Who::Agent, LASTING)
        .unwrap();

    let request = Request::decode(br#"{"op":"hold","on":true}"#).unwrap();
    assert_eq!(
        service.dispatch(request, &phase).await.unwrap(),
        json!({"ok": true, "held": true, "kept": true})
    );
    assert!(marker.exists());
    assert!(service.gate.held().unwrap());
    assert!(grant.authorize().is_err());
    assert!(matches!(
        *observed.borrow(),
        super::ConnectionPhase::Mutation("hold")
    ));

    let request = Request::decode(br#"{"op":"hold","on":false}"#).unwrap();
    service.dispatch(request, &phase).await.unwrap();
    let grant = service
        .gate
        .acquire_lasting(&owner, super::Who::Agent, LASTING)
        .unwrap();
    let request =
        Request::decode(format!(r#"{{"op":"release","owner":"{owner}"}}"#).as_bytes()).unwrap();
    assert_eq!(
        service.dispatch(request, &phase).await.unwrap(),
        json!({"ok": true, "released": true})
    );
    assert!(grant.authorize().is_err());
    assert!(matches!(
        *observed.borrow(),
        super::ConnectionPhase::Mutation("release")
    ));

    let error = service
        .dispatch(Request::Renew { owner }, &phase)
        .await
        .unwrap_err();
    assert_eq!(error.code, service.check().unwrap_err().code);
    assert_ne!(
        error.code,
        Code::LeaseLost,
        "renew still checks the session first"
    );
    assert_eq!(service.engine.lock().await.posted, 0);
}

#[tokio::test]
async fn shutdown_rechecks_ownership_after_draining() {
    let directory = tempfile::tempdir().unwrap();
    let service = service(&directory.path().join("stopped"));
    let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
    let (resume, waiting) = tokio::sync::oneshot::channel::<()>();
    let work = service
        .operations
        .start(async move { waiting.await.unwrap() })
        .await
        .unwrap();
    let request =
        Request::decode(format!(r#"{{"op":"shutdown","pid":{}}}"#, std::process::id()).as_bytes())
            .unwrap();
    let dispatch = service.dispatch(request, &phase);
    tokio::pin!(dispatch);
    std::future::poll_fn(|cx| {
        assert!(std::future::Future::poll(dispatch.as_mut(), cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    let grant = service
        .gate
        .acquire_lasting(&"a".repeat(48), super::Who::Agent, LASTING)
        .unwrap();
    resume.send(()).unwrap();
    let error = dispatch.await.unwrap_err();
    work.await.unwrap();
    assert_eq!(error.code, Code::Busy);
    assert_eq!(error.error, "The input service has an active controller");
    assert!(!error.uncertain);
    assert!(matches!(
        *observed.borrow(),
        super::ConnectionPhase::Refusal
    ));
    assert!(grant.authorize().is_ok());
    assert_eq!(service.engine.lock().await.posted, 0);
}

#[tokio::test]
async fn release_cancels_ownership_before_waiting_for_the_engine() {
    let directory = tempfile::tempdir().unwrap();
    let service = service(&directory.path().join("stopped"));
    let (phase, _) = super::watch::channel(super::ConnectionPhase::Refusal);
    let owner = "a".repeat(48);
    let grant = service
        .gate
        .acquire_lasting(&owner, super::Who::Agent, LASTING)
        .unwrap();
    let engine = service.engine.lock().await;
    let request = Request::Release { owner };
    let dispatch = service.dispatch(request, &phase);
    tokio::pin!(dispatch);
    std::future::poll_fn(|cx| {
        assert!(std::future::Future::poll(dispatch.as_mut(), cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    assert!(grant.authorize().is_err());
    assert!(service.engine.try_lock().is_err());
    drop(engine);
    assert_eq!(
        dispatch.await.unwrap(),
        json!({"ok": true, "released": true})
    );
    service
        .gate
        .acquire_lasting(&"b".repeat(48), super::Who::Agent, LASTING)
        .unwrap();
}

#[tokio::test]
async fn a_failed_marker_removal_keeps_agents_stopped() {
    let directory = tempfile::tempdir().unwrap();
    let service = service(directory.path());
    let (phase, _) = super::watch::channel(super::ConnectionPhase::Refusal);
    let request = Request::decode(br#"{"op":"hold","on":true}"#).unwrap();
    assert_eq!(
        service.dispatch(request, &phase).await.unwrap(),
        json!({"ok": true, "held": true, "kept": false})
    );
    let request = Request::decode(br#"{"op":"hold","on":false}"#).unwrap();
    let error = service.dispatch(request, &phase).await.unwrap_err();
    assert_eq!(error.code, Code::Native);
    assert_eq!(
        error.error,
        "Agents could not be resumed: the helper could not clear the stop"
    );
    assert!(!error.uncertain);
    assert!(service.gate.held().unwrap());
    assert!(directory.path().is_dir());
    assert_eq!(service.engine.lock().await.posted, 0);
}

#[tokio::test]
async fn lifecycle_replies_keep_their_exact_line_framing() {
    for (result, expected) in [
            (Ok(json!({"ok":true,"held":true,"kept":true})), b"{\"held\":true,\"kept\":true,\"ok\":true}\n".as_slice()),
            (Request::decode(br#"{"op":"hold","on":null}"#).and_then(|request| match request { Request::Hold { on } => on.map(|_| json!({})), _ => panic!("expected hold") }), b"{\"code\":\"input\",\"error\":\"Hold requires a boolean on\",\"ok\":false,\"uncertain\":false}\n".as_slice()),
        ] {
            let (mut client, mut server) = UnixStream::pair().unwrap();
            super::reply(&mut server, result).await.unwrap();
            drop(server);
            let mut bytes = Vec::new();
            super::AsyncReadExt::read_to_end(&mut client, &mut bytes).await.unwrap();
            assert_eq!(bytes, expected);
        }
}

#[tokio::test]
async fn an_abandoned_read_does_not_degrade_the_service() {
    let operations = super::Operations::default();
    let slots = Arc::new(tokio::sync::Semaphore::new(1));
    for malformed in [false, true] {
        let (mut client, mut server) = UnixStream::pair().unwrap();
        client
            .write_all(if malformed {
                b"{\n"
            } else {
                b"{\"op\":\"hello\"}\n"
            })
            .await
            .unwrap();
        drop(client);
        let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
        let slot = slots.clone().acquire_owned().await.unwrap();
        let mut tasks = super::JoinSet::new();
        let task = tasks.spawn(async move {
            let _slot = slot;
            let result = match read_request(&mut server).await {
                Ok(request) => {
                    assert!(matches!(request, Request::Hello));
                    phase.send_replace(super::ConnectionPhase::Read);
                    Ok(json!({"ok": true}))
                }
                Err(error) => Err(error),
            };
            super::reply(&mut server, result).await
        });
        operations
            .connections
            .lock()
            .unwrap()
            .insert(task.id(), (observed, false));
        let outcome = tasks.join_next_with_id().await.unwrap();
        assert!(matches!(outcome, Ok((_, Err(_)))));
        super::connection_outcome(outcome, &operations, false).unwrap();
        assert!(operations.connections.lock().unwrap().is_empty());
        assert_eq!(slots.available_permits(), 1);
        operations.check().unwrap();
    }
    operations.start(async { 42 }).await.unwrap().await.unwrap();
    operations.drain().await.unwrap();
}

#[tokio::test]
async fn a_failed_refusal_reply_does_not_degrade_the_service()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let marker = directory.path().join("stopped");
    let service = Arc::new(service(&marker));
    let (mut client, mut server) = UnixStream::pair()?;
    client.write_all(b"{\"op\":\"hold\",\"on\":null}\n").await?;
    drop(client);
    let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
    let mut tasks = super::JoinSet::new();
    let serving = service.clone();
    let task = tasks.spawn(async move {
        let request = read_request(&mut server).await?;
        let refusal = serving.dispatch(request, &phase).await;
        let Err(error) = &refusal else {
            panic!("expected a hold refusal");
        };
        assert_eq!(error.code, Code::Input);
        assert_eq!(error.error, "Hold requires a boolean on");
        assert!(!error.uncertain);
        assert!(matches!(*phase.borrow(), super::ConnectionPhase::Refusal));
        super::reply(&mut server, refusal).await
    });
    service
        .operations
        .connections
        .lock()
        .map_err(|_| "connection tracking failed")?
        .insert(task.id(), (observed, false));
    let outcome = tasks
        .join_next_with_id()
        .await
        .ok_or("connection missing")?;
    assert!(matches!(outcome, Ok((_, Err(_)))));
    super::connection_outcome(outcome, &service.operations, false)?;
    service.operations.check()?;
    assert!(!service.gate.held()?);
    assert!(!marker.exists());
    assert_eq!(service.engine.lock().await.posted, 0);
    Ok(())
}

#[tokio::test]
async fn an_abandoned_ownership_refusal_keeps_the_current_controller_healthy() {
    let operations = super::Operations::default();
    let gate = Arc::new(Gate::default());
    let owner = "a".repeat(48);
    let grant = gate
        .acquire_lasting(&owner, super::Who::Agent, LASTING)
        .unwrap();
    for operation in ["release", "renew", "input"] {
        let (mut client, mut server) = UnixStream::pair().unwrap();
        client
            .write_all(
                format!("{}\n", json!({"op": operation, "owner": "b".repeat(48)})).as_bytes(),
            )
            .await
            .unwrap();
        let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
        let refused = gate.clone();
        let (decided, deciding) = tokio::sync::oneshot::channel();
        let (left, leaving) = tokio::sync::oneshot::channel::<()>();
        let mut tasks = super::JoinSet::new();
        let task = tasks.spawn(async move {
            let _phase = phase;
            let request = read_request(&mut server).await?;
            let refusal = match request {
                Request::Renew { owner } => refused.renew(&owner).unwrap_err(),
                Request::Release { owner } => refused.cancel(Some(&owner)).unwrap_err(),
                Request::Input { owner, .. } => refused.cancel(Some(&owner)).unwrap_err(),
                _ => panic!("expected an ownership request"),
            };
            assert_eq!(refusal.code, Code::LeaseLost);
            assert!(!refusal.uncertain);
            // The refusal is answered only once its client has gone.
            decided.send(()).unwrap();
            leaving.await.unwrap();
            super::reply(&mut server, Err(refusal)).await
        });
        operations
            .connections
            .lock()
            .unwrap()
            .insert(task.id(), (observed, false));
        deciding.await.expect("the request was read and refused");
        drop(client);
        left.send(()).unwrap();
        let outcome = tasks.join_next_with_id().await.unwrap();
        assert!(matches!(outcome, Ok((_, Err(_)))));
        super::connection_outcome(outcome, &operations, false).unwrap();
        operations.check().unwrap();
        assert!(gate.active().unwrap());
        drop(grant.authorize().unwrap());
        gate.renew_lasting(&owner, LASTING).unwrap();
    }
    operations.drain().await.unwrap();
}

#[tokio::test]
async fn a_handler_panic_is_observed() {
    for stopping in [false, true] {
        let operations = super::Operations::default();
        let mut tasks = super::JoinSet::<Result<()>>::new();
        let (phase, observed) = super::watch::channel(super::ConnectionPhase::Mutation("input"));
        let task = tasks.spawn(async move {
            let _phase = phase;
            panic!("private owner and typed text");
        });
        operations
            .connections
            .lock()
            .unwrap()
            .insert(task.id(), (observed, false));
        let outcome = tasks.join_next_with_id().await.unwrap();
        assert!(outcome.as_ref().unwrap_err().is_panic());
        let error = super::connection_outcome(outcome, &operations, stopping).unwrap_err();
        assert_eq!(error.code, Code::Uncertain);
        assert!(error.uncertain);
        let sticky = operations.check().unwrap_err();
        assert!(sticky.uncertain);
        assert!(
            std::error::Error::source(&sticky).is_some(),
            "keeps its cause"
        );
        assert!(operations.drain().await.unwrap_err().uncertain);
        assert!(operations.connections.lock().unwrap().is_empty());
        for diagnostic in [json!(error).to_string(), format!("{error:?}")] {
            assert!(!diagnostic.contains("private owner and typed text"));
            assert!(diagnostic.len() < 256);
        }
    }
}

#[tokio::test]
async fn a_lost_mutation_reply_keeps_recovery_pending() {
    let operations = Arc::new(super::Operations::default());
    let gate = Arc::new(Gate::default());
    let owner = "a".repeat(48);
    let grant = gate
        .acquire_lasting(&owner, super::Who::Agent, LASTING)
        .unwrap();
    let engine = Stand::locked(&[]);
    let mut tasks = super::JoinSet::new();
    let (mut client, mut server) = UnixStream::pair().unwrap();
    let (effect, effected) = tokio::sync::oneshot::channel();
    let (deliver, delivery) = tokio::sync::oneshot::channel::<()>();
    let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
    let performed = engine.clone();
    let granted = grant.clone();
    let working = operations.clone();
    let task = tasks.spawn(async move {
        let request = read_request(&mut server).await?;
        assert!(matches!(request, Request::Input { .. }));
        phase.send_replace(super::ConnectionPhase::Mutation("input"));
        let work = working
            .start(async move {
                drop(granted.authorize().unwrap());
                let mut engine = performed.lock().await;
                engine.posted += 1;
                engine.held = true;
            })
            .await?;
        work.await.unwrap();
        effect.send(()).unwrap();
        delivery.await.unwrap();
        super::reply(&mut server, Ok(json!({"ok": true, "performed": 1}))).await
    });
    operations
        .connections
        .lock()
        .unwrap()
        .insert(task.id(), (observed, false));
    assert_eq!(engine.lock().await.posted, 0);
    client.write_all(b"{\"op\":\"input\"}\n").await.unwrap();
    effected.await.unwrap();
    drop(client);
    deliver.send(()).unwrap();
    let error =
        super::connection_outcome(tasks.join_next_with_id().await.unwrap(), &operations, false)
            .unwrap_err();
    assert!(error.uncertain);
    assert!(gate.active().unwrap());
    assert!(grant.authorize().is_ok());
    assert!(operations.start(async {}).await.unwrap_err().uncertain);
    assert_eq!(engine.lock().await.posted, 1);
    gate.cancel(Some(&owner)).unwrap();
    assert!(
        super::drain_and_release(&operations, &engine, &gate)
            .await
            .unwrap_err()
            .uncertain
    );
    assert!(!engine.lock().await.held);
    assert!(operations.check().unwrap_err().uncertain);
    super::reset(&operations, &engine, &gate, Perform::release)
        .await
        .unwrap();
    operations.check().unwrap();
    assert_eq!(
        engine.lock().await.posted,
        1,
        "recovery never replays input"
    );
}

#[tokio::test]
async fn reset_settles_completed_connections_without_hiding_later_failures() {
    for late in [false, true] {
        for panic in [false, true] {
            tokio::time::timeout(WATCHDOG, async {
                let operations = Arc::new(super::Operations::default());
                let gate = Arc::new(Gate::default());
                let old_grant = gate
                    .acquire_lasting(&"a".repeat(48), super::Who::Agent, LASTING)
                    .unwrap();
                let engine = Stand::locked(&[]);
                let (client, mut server) = UnixStream::pair().unwrap();
                let (effect, effected) = tokio::sync::oneshot::channel();
                let (deliver, delivery) = tokio::sync::oneshot::channel();
                let mut deliver = Some(deliver);
                let (phase, observed) =
                    super::watch::channel(super::ConnectionPhase::Mutation("input"));
                let mut completed = observed.clone();
                let working = operations.clone();
                let performed = engine.clone();
                let mut tasks = super::JoinSet::new();
                let task = tasks.spawn(async move {
                    let _phase = phase;
                    working
                        .start(async move {
                            let mut engine = performed.lock().await;
                            engine.posted += 1;
                            engine.held = true;
                        })
                        .await?
                        .await
                        .unwrap();
                    effect.send(()).unwrap();
                    delivery.await.unwrap();
                    assert!(!panic, "private connection failure");
                    super::reply(&mut server, Ok(json!({"ok": true, "performed": 1}))).await
                });
                operations
                    .connections
                    .lock()
                    .unwrap()
                    .insert(task.id(), (observed, false));
                effected.await.unwrap();
                drop(client);
                if !late {
                    deliver.take().unwrap().send(()).unwrap();
                    completed.changed().await.unwrap_err();
                }
                super::reset(&operations, &engine, &gate, Perform::release)
                    .await
                    .unwrap();
                operations.check().unwrap();
                assert!(old_grant.authorize().is_err());
                assert!(!engine.lock().await.held);
                if late {
                    deliver.take().unwrap().send(()).unwrap();
                    completed.changed().await.unwrap_err();
                }
                let outcome = tasks.join_next_with_id().await.unwrap();
                if panic {
                    assert!(outcome.as_ref().unwrap_err().is_panic());
                } else {
                    assert!(matches!(outcome, Ok((_, Err(_)))));
                }
                assert_eq!(
                    super::connection_outcome(outcome, &operations, false).is_err(),
                    late
                );
                assert_eq!(operations.check().is_err(), late);
                assert!(operations.connections.lock().unwrap().is_empty());
                if !late {
                    let grant = gate
                        .acquire_lasting(&"b".repeat(48), super::Who::Agent, LASTING)
                        .unwrap();
                    operations
                        .start(async move {
                            drop(grant.authorize().unwrap());
                        })
                        .await
                        .unwrap()
                        .await
                        .unwrap();
                    operations.drain().await.unwrap();
                }
                assert_eq!(
                    engine.lock().await.posted,
                    1,
                    "recovery never replays input"
                );
            })
            .await
            .expect("connection recovery did not settle");
        }
    }
}

#[tokio::test]
async fn shutdown_classifies_cancelled_handlers_and_joins_owned_work() {
    for lost_reply in [false, true] {
        let operations = super::Operations::default();
        let gate = Arc::new(Gate::default());
        let _grant = gate
            .acquire_lasting(&"a".repeat(48), super::Who::Agent, LASTING)
            .unwrap();
        let engine = Stand::locked(&[]);
        let held = engine.clone().lock_owned().await;
        let (resume, barrier) = tokio::sync::oneshot::channel::<()>();
        let work = operations
            .start(async move {
                let mut held = held;
                barrier.await.unwrap();
                held.held = true;
            })
            .await
            .unwrap();
        let mut tasks = super::JoinSet::new();
        let (phase, observed) = super::watch::channel(super::ConnectionPhase::Mutation("input"));
        let idle = tasks.spawn(async move {
            let _phase = phase;
            std::future::pending::<Result<()>>().await
        });
        operations
            .connections
            .lock()
            .unwrap()
            .insert(idle.id(), (observed, false));
        if lost_reply {
            let (finished, received) = tokio::sync::oneshot::channel();
            let (phase, observed) =
                super::watch::channel(super::ConnectionPhase::Mutation("release"));
            let task = tasks.spawn(async move {
                let _phase = phase;
                finished.send(()).unwrap();
                Err(Error::new(Code::Transport, "private transport cause"))
            });
            operations
                .connections
                .lock()
                .unwrap()
                .insert(task.id(), (observed, false));
            received.await.unwrap();
        }
        let cleanup = super::finish(&gate, &mut tasks, &operations, &engine);
        tokio::pin!(cleanup);
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(cleanup.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        assert!(!gate.active().unwrap());
        assert!(
            gate.acquire_lasting(&"b".repeat(48), super::Who::Agent, LASTING)
                .is_err()
        );
        assert!(engine.try_lock().is_err());
        resume.send(()).unwrap();
        let result = tokio::time::timeout(WATCHDOG, cleanup)
            .await
            .expect("shutdown did not cancel handlers and drain work");
        assert_eq!(result.is_err(), lost_reply);
        assert_eq!(operations.check().is_err(), lost_reply);
        tokio::time::timeout(WATCHDOG, work)
            .await
            .expect("registered work did not report its outcome")
            .unwrap();
        assert_eq!(operations.registered(), 0);
        assert!(!engine.lock().await.held);
        assert_eq!(engine.lock().await.releases, 1);
    }
}

#[tokio::test]
async fn stop_fields_use_the_last_boolean_and_invalid_values_leave_the_stop_unchanged()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let marker = dir.path().join("stopped");
    let gate = Gate::default();
    let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);

    for stopped in [true, false] {
        let reply = hold(&gate, &marker, stopped, &phase)?;
        assert_eq!(reply, json!({"ok": true, "held": stopped, "kept": true}));
        if stopped {
            std::fs::write(&marker, b"stop")?;
        }
        for fields in [
            r#", "on": true, "on": false"#,
            r#", "on": false, "on": true"#,
            r#", "on": true, "on": true"#,
            r#", "on": false, "on": false"#,
            r#", "on": true, "\u006fn": false"#,
            r#", "\u006f\u006e": false, "on": true"#,
            r#", "on": null, "on": true"#,
            r#", "on": false, "on": null"#,
            "",
            r#", "on": null"#,
            r#", "on": "false""#,
            r#", "on": 0"#,
            r#", "on": []"#,
            r#", "on": {}"#,
        ] {
            let body = format!("{{\"op\":\"hold\"{fields}}}\n");
            phase.send_replace(super::ConnectionPhase::Refusal);
            let result = match deliver(body.clone().into_bytes(), 64).await {
                Ok(Request::Hold { on }) => on.and_then(|on| hold(&gate, &marker, on, &phase)),
                Ok(_) => panic!("expected hold"),
                Err(error) => Err(error),
            };
            let wire: Value = serde_json::from_str(&body)?;
            if let Some(on) = wire["on"].as_bool() {
                assert_eq!(result?, json!({"ok":true,"held":on,"kept":true}), "{body}");
                assert_eq!(gate.held()?, on, "{body}");
                assert_eq!(marker.exists(), on, "{body}");
                hold(&gate, &marker, stopped, &phase)?;
                if stopped {
                    std::fs::write(&marker, b"stop")?;
                }
                continue;
            }
            let error = result.err().ok_or("expected refusal")?;
            assert_eq!(error.code, Code::Input, "{body}");
            assert_eq!(error.error, "Hold requires a boolean on");
            assert!(!error.uncertain, "{body}");
            assert!(matches!(
                *observed.borrow(),
                super::ConnectionPhase::Refusal
            ));
            assert_eq!(gate.held()?, stopped, "{body}");
            if stopped {
                assert_eq!(std::fs::read(&marker)?, b"stop", "{body}");
            } else {
                assert!(!marker.exists(), "{body}");
            }
        }
    }

    for on in [true, false] {
        let body = format!("{{\"op\":\"hold\",\"on\":{on},\"other\":1,\"other\":2}}\n");
        let Request::Hold { on } = deliver(body.into_bytes(), 64).await? else {
            panic!("expected hold");
        };
        let on = on?;
        let reply = hold(&gate, &marker, on, &phase)?;
        assert_eq!(reply, json!({"ok": true, "held": on, "kept": true}));
        assert_eq!(gate.held()?, on);
        assert_eq!(marker.exists(), on);
    }
    Ok(())
}

/// What a stand-in engine does at one action.
#[derive(Clone, Copy, PartialEq)]
enum Step {
    Post,
    Refuse,
    Unreadable,
    /// Post one event, then fail.
    Break,
    UnacknowledgedPress,
    DockRefusal,
}

/// An engine that logs each check and post, and does at each action
/// what its script says. It holds a handle to the lock it lives in, to
/// see that the lock is held whenever it is asked anything.
struct Stand {
    script: Vec<Step>,
    log: Vec<String>,
    /// What each check was asked to find.
    asked: Vec<Expect>,
    /// The role each check of a field finds it of, a text field's past
    /// the list.
    roles: Vec<Option<&'static str>>,
    preparations: Vec<bool>,
    budgets: Vec<Duration>,
    check_delay: Duration,
    clock: Option<Instant>,
    held: bool,
    sheet: bool,
    opens_sheet: bool,
    posted: u64,
    releases: usize,
    release_fails: bool,
    /// What a referenced element is found as; none when its look is gone.
    found: Option<Resolved>,
    /// Whether the element changes once its app is brought forward.
    moves: bool,
    /// Where each click it carried out landed.
    clicked: Vec<(f64, f64)>,
    /// What an action on an element comes to; a stale refusal when none.
    acts: Option<std::result::Result<Acted, Code>>,
    /// Each action on an element it carried out, and the window named.
    acted: Vec<(Reference, ElementAction, Option<i64>)>,
    lock: Weak<Mutex<Stand>>,
}

impl Stand {
    fn locked(script: &[Step]) -> Arc<Mutex<Self>> {
        Arc::new_cyclic(|lock| {
            Mutex::new(Self {
                script: script.to_vec(),
                log: Vec::new(),
                asked: Vec::new(),
                roles: Vec::new(),
                preparations: Vec::new(),
                budgets: Vec::new(),
                check_delay: Duration::ZERO,
                clock: None,
                held: false,
                sheet: false,
                opens_sheet: false,
                posted: 0,
                releases: 0,
                release_fails: false,
                found: None,
                moves: false,
                clicked: Vec::new(),
                acts: None,
                acted: Vec::new(),
                lock: lock.clone(),
            })
        })
    }

    fn assert_locked(&self) {
        let lock = self.lock.upgrade().unwrap();
        assert!(lock.try_lock().is_err(), "asked outside the engine lock");
    }

    /// How many lines of the log start with `what`.
    fn logged(&self, what: &str) -> usize {
        self.log
            .iter()
            .filter(|line| line.starts_with(what))
            .count()
    }
}

impl Perform for Stand {
    fn now(&self) -> Instant {
        self.clock.unwrap_or_else(Instant::now)
    }

    async fn require(
        &mut self,
        expect: &Expect,
        _: Need,
        budget: Duration,
        prepare: bool,
    ) -> Result<Option<&'static str>> {
        self.assert_locked();
        self.preparations.push(prepare);
        self.budgets.push(budget);
        if let Some(clock) = &mut self.clock {
            *clock += self.check_delay;
        } else if !self.check_delay.is_zero() {
            tokio::time::sleep(self.check_delay).await;
        }
        if prepare && self.sheet {
            return Err(Error::new(Code::FocusChanged, "A sheet blocks the window")
                .with("reason", json!("sheet")));
        }
        self.asked.push(expect.clone());
        let index = self.logged("check");
        self.log.push(format!("check {index}"));
        if self.script.get(index) == Some(&Step::Refuse) {
            return Err(
                Error::new(Code::FocusChanged, "Calculator has keyboard focus")
                    .with("reason", json!("app"))
                    .with("found", json!({"app": "Calculator", "pid": 812})),
            );
        }
        if self.script.get(index) == Some(&Step::Unreadable) {
            return Err(crate::native::focus::refused(
                crate::native::focus::Reason::Unreadable,
            ));
        }
        Ok(expect.field().and_then(|_| {
            self.roles
                .get(index)
                .copied()
                .unwrap_or(Some("AXTextField"))
        }))
    }

    async fn resolve(&mut self, _: Reference) -> Result<Resolved> {
        self.assert_locked();
        let index = self.logged("resolve");
        self.log.push(format!("resolve {index}"));
        self.found
            .clone()
            .ok_or_else(|| stale("That look is no longer kept"))
    }

    async fn confirm(&mut self, _: Resolved) -> Result<()> {
        self.assert_locked();
        let index = self.logged("confirm");
        self.log.push(format!("confirm {index}"));
        if self.moves {
            return Err(stale("The element moved while its app was brought forward"));
        }
        Ok(())
    }

    async fn act(
        &mut self,
        reference: Reference,
        action: ElementAction,
        window: Option<i64>,
    ) -> Result<Acted> {
        self.assert_locked();
        let index = self.logged("act");
        self.log.push(format!("act {index}"));
        self.acted.push((reference, action, window));
        match self.acts.clone() {
            None => Err(stale("That look is no longer kept")),
            Some(Err(code)) => {
                Err(Error::new(code, "The app did not acknowledge the action").uncertain())
            }
            Some(Ok(acted)) => {
                self.posted += u64::from(acted.changed);
                Ok(acted)
            }
        }
    }

    async fn perform(&mut self, action: Action) -> Result<()> {
        self.assert_locked();
        let index = self.logged("post");
        self.log.push(format!("post {index}"));
        if let Action::Click { x, y, .. } = action {
            self.clicked.push((x, y));
        }
        if self.script.get(index) == Some(&Step::UnacknowledgedPress) {
            return Err(
                Error::new(Code::Uncertain, "The Dock did not acknowledge the press").uncertain(),
            );
        }
        if self.script.get(index) == Some(&Step::DockRefusal) {
            return Err(Error::new(
                Code::Input,
                "No pressable Dock item at that point; no click was sent",
            ));
        }
        self.posted += 1;
        if self.opens_sheet {
            self.sheet = true;
        }
        match self.script.get(index) {
            Some(Step::Break) => Err(Error::new(Code::Identity, "The session changed")),
            _ => Ok(()),
        }
    }

    fn posted(&self) -> u64 {
        self.posted
    }

    fn held_input(&self) -> bool {
        self.held
    }

    fn release(&mut self) -> Result<()> {
        self.releases += 1;
        if self.release_fails {
            return Err(
                Error::new(Code::Uncertain, "Held input could not be released").uncertain(),
            );
        }
        self.held = false;
        Ok(())
    }
}

fn keys(count: usize) -> Vec<Action> {
    (0..count)
        .map(|_| serde_json::from_value(json!({"type": "key", "key": "a"})).unwrap())
        .collect()
}

/// Run `count` key actions on a stand-in following `script`, under
/// its lock, with or without an expectation.
async fn ran(
    script: &[Step],
    count: usize,
    expect: Option<Expect>,
) -> (
    std::result::Result<Option<&'static str>, Value>,
    Arc<Mutex<Stand>>,
) {
    let stand = Stand::locked(script);
    let gate = Arc::new(Gate::default());
    let outcome = {
        let mut engine = stand.lock().await;
        run(&mut *engine, &gate, keys(count), expect.as_ref()).await
    };
    (outcome, stand)
}

fn app() -> Option<Expect> {
    Some(Expect::new(812).unwrap())
}

#[tokio::test]
async fn the_target_is_checked_under_the_engine_lock_just_before_each_action() {
    let (outcome, stand) = ran(&[Step::Post, Step::Post], 2, app()).await;
    assert!(outcome.is_ok());
    assert_eq!(
        stand.lock().await.log,
        ["check 0", "post 0", "check 1", "post 1"]
    );
}

#[tokio::test]
async fn a_mismatch_posts_nothing_and_is_not_uncertain() {
    let (outcome, stand) = ran(&[Step::Refuse], 2, app()).await;
    let reply = outcome.unwrap_err();
    assert_eq!(reply["code"], "focus-changed");
    assert_eq!(reply["reason"], "app");
    assert_eq!(reply["found"], json!({"app": "Calculator", "pid": 812}));
    assert_eq!(reply["performed"], 0);
    assert_eq!(reply["partial"], false);
    assert_eq!(reply["uncertain"], false);

    let stand = stand.lock().await;
    assert_eq!(stand.log, ["check 0"], "nothing was performed");
    assert_eq!(stand.posted, 0);
    assert_eq!(stand.releases, 1, "what was held is released");
}

#[tokio::test]
async fn a_mismatch_after_earlier_actions_says_how_many_were_sent() {
    let (outcome, stand) = ran(&[Step::Post, Step::Post, Step::Refuse], 4, app()).await;
    let reply = outcome.unwrap_err();
    assert_eq!(reply["code"], "focus-changed");
    assert_eq!(reply["performed"], 2);
    assert_eq!(reply["partial"], false);
    assert_eq!(reply["uncertain"], false);
    assert_eq!(stand.lock().await.posted, 2);
}

#[tokio::test]
async fn an_unreadable_first_check_posts_nothing_and_is_not_uncertain() {
    for action in [
        Action::Type {
            text: "hello".to_owned(),
        },
        Action::Key {
            key: "a".to_owned(),
        },
    ] {
        let stand = Stand::locked(&[Step::Unreadable]);
        let gate = Arc::new(Gate::default());
        let grant = gate
            .acquire_lasting(&atlas_core::files::token(), super::Who::Agent, LASTING)
            .unwrap();
        assert!(gate.active().unwrap());

        let mut engine = stand.lock().await;
        assert_eq!(engine.posted, 0);
        let reply = run(&mut *engine, &gate, vec![action], app().as_ref())
            .await
            .unwrap_err();
        assert_eq!(reply["code"], "focus-changed");
        assert_eq!(reply["reason"], "unreadable");
        assert_eq!(reply["found"], Value::Null);
        assert_eq!(reply["performed"], 0);
        assert_eq!(reply["partial"], false);
        assert_eq!(reply["uncertain"], false);
        assert_eq!(engine.preparations, [true]);
        assert_eq!(engine.log, ["check 0"]);
        assert_eq!(engine.posted, 0);
        assert_eq!(engine.releases, 1);
        assert!(!gate.active().unwrap());
        drop(grant);
    }
}

#[tokio::test]
async fn an_unreadable_later_check_does_not_repeat_earlier_input() {
    let stand = Stand::locked(&[Step::Post, Step::Unreadable]);
    let gate = Arc::new(Gate::default());
    let grant = gate
        .acquire_lasting(&atlas_core::files::token(), super::Who::Agent, LASTING)
        .unwrap();
    assert!(gate.active().unwrap());

    let mut engine = stand.lock().await;
    let reply = run(&mut *engine, &gate, keys(2), app().as_ref())
        .await
        .unwrap_err();
    assert_eq!(reply["code"], "focus-changed");
    assert_eq!(reply["reason"], "unreadable");
    assert_eq!(reply["found"], Value::Null);
    assert_eq!(reply["performed"], 1);
    assert_eq!(reply["partial"], false);
    assert_eq!(reply["uncertain"], false);
    assert_eq!(engine.preparations, [true, false]);
    assert_eq!(engine.log, ["check 0", "post 0", "check 1"]);
    assert_eq!(engine.posted, 1);
    assert_eq!(engine.releases, 1);
    assert!(!gate.active().unwrap());
    drop(grant);
}

#[tokio::test]
async fn an_action_that_fails_partway_stays_uncertain() {
    let (outcome, _) = ran(&[Step::Break], 2, app()).await;
    let reply = outcome.unwrap_err();
    assert_eq!(reply["code"], "identity");
    assert_eq!(reply["performed"], 0);
    assert_eq!(reply["partial"], true);
    assert_eq!(reply["uncertain"], true);
}

#[tokio::test]
async fn a_refused_or_unacknowledged_dock_click_reports_no_posted_events() {
    for (step, uncertain, code) in [
        (Step::DockRefusal, false, "input"),
        (Step::UnacknowledgedPress, true, "uncertain"),
    ] {
        let stand = Stand::locked(&[step]);
        let gate = Gate::default();
        let action = serde_json::from_value(json!({"type": "click", "x": 1138, "y": 940})).unwrap();
        let mut engine = stand.lock().await;
        let reply = tokio::time::timeout(
            Duration::from_secs(1),
            run(&mut *engine, &gate, vec![action], None),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["code"], code);
        assert_eq!(reply["performed"], 0);
        assert_eq!(reply["partial"], false);
        assert_eq!(reply["uncertain"], uncertain);
        assert_eq!(engine.posted, 0);
        assert_eq!(engine.releases, 1);
    }
}

/// Run `actions` on a stand-in that posts each, expecting `expect`.
async fn ran_actions(actions: Vec<Value>, expect: &Expect) -> Arc<Mutex<Stand>> {
    let stand = Stand::locked(&[]);
    let gate = Arc::new(Gate::default());
    let actions = actions
        .into_iter()
        .map(|action| serde_json::from_value(action).unwrap())
        .collect();
    {
        let mut engine = stand.lock().await;
        run(&mut *engine, &gate, actions, Some(expect))
            .await
            .unwrap();
    }
    stand
}

/// The window and field each check was asked for.
fn asked_for(stand: &Stand) -> Vec<(Option<i64>, Option<String>)> {
    stand
        .asked
        .iter()
        .map(|expect| (expect.window(), expect.field().map(str::to_owned)))
        .collect()
}

#[tokio::test]
async fn keys_are_checked_once_before_the_first_key() {
    let expect = Expect::new(812)
        .unwrap()
        .in_window(4120)
        .unwrap()
        .in_field("Email")
        .unwrap();
    let key = |key: &str| json!({"type": "key", "key": key});
    let stand = ran_actions(vec![key("tab"), key("a"), key("return")], &expect).await;
    let stand = stand.lock().await;
    assert_eq!(
        asked_for(&stand),
        [
            (Some(4120), Some("Email".to_owned())),
            (None, None),
            (None, None)
        ],
        "the app before every key, the window and field before the first"
    );
    assert!(stand.asked.iter().all(|asked| asked.pid() == 812));
}

#[tokio::test]
async fn later_keys_keep_app_only_checks_while_pointer_and_text_keep_their_own_targets() {
    let expect = Expect::new(812)
        .unwrap()
        .in_window(42)
        .unwrap()
        .in_field("Search")
        .unwrap();
    let stand = ran_actions(
        vec![
            json!({"type":"key","key":"tab"}),
            json!({"type":"pointer","phase":"down","x":10,"y":20}),
            json!({"type":"pointer","phase":"move","x":30,"y":40}),
            json!({"type":"pointer","phase":"up","x":30,"y":40}),
            json!({"type":"wait","ms":0}),
            json!({"type":"key","key":"return"}),
            json!({"type":"type","text":"a"}),
        ],
        &expect,
    )
    .await;
    let engine = stand.lock().await;
    assert_eq!(engine.posted, 7);
    let full = (Some(42), Some("Search".to_owned()));
    assert_eq!(
        asked_for(&engine),
        [
            full.clone(),
            full.clone(),
            full.clone(),
            full.clone(),
            (None, None),
            full
        ]
    );
    assert!(engine.asked.iter().all(|asked| asked.pid() == 812));
}

#[tokio::test]
async fn text_is_checked_whole_before_every_part() {
    let expect = Expect::new(812).unwrap().in_field("Email").unwrap();
    let part = |text: &str| json!({"type": "type", "text": text});
    let stand = ran_actions(vec![part("sam@"), part("example"), part(".com")], &expect).await;
    assert_eq!(
        asked_for(&*stand.lock().await),
        vec![(None, Some("Email".to_owned())); 3]
    );
}

/// Send `actions` on a stand-in whose checks find the field of each of
/// `roles` in turn (none for one they could not tell), expecting the field
/// named Email.
async fn typed_into(actions: &[Value], roles: &[Option<&'static str>]) -> Option<&'static str> {
    let stand = Stand::locked(&[]);
    stand.lock().await.roles = roles.to_vec();
    let gate = Arc::new(Gate::default());
    let expect = Expect::new(812).unwrap().in_field("Email").unwrap();
    let actions = actions
        .iter()
        .map(|action| serde_json::from_value(action.clone()).unwrap())
        .collect();
    let mut engine = stand.lock().await;
    run(&mut *engine, &gate, actions, Some(&expect))
        .await
        .unwrap()
}

#[tokio::test]
async fn typing_into_a_named_field_says_the_role_every_check_found_it_of() {
    let part = |text: &str| json!({"type": "type", "text": text});
    let search = Some("AXSearchField");
    assert_eq!(
        typed_into(&[part("sam@"), part("example")], &[search, search]).await,
        search
    );
    assert_eq!(
        typed_into(
            &[part("sam@"), part("example")],
            &[Some("AXTextField"), Some("AXTextArea")]
        )
        .await,
        None,
        "checks that disagree name no role"
    );

    let (unnamed, _) = ran(&[Step::Post], 1, app()).await;
    assert_eq!(unnamed.unwrap(), None, "a check of no field says none");
    let (refused, _) = ran(
        &[Step::Refuse],
        1,
        Some(Expect::new(812).unwrap().in_field("Email").unwrap()),
    )
    .await;
    assert!(refused.is_err(), "a refusal is a reply of its own");
}

#[tokio::test]
async fn text_any_check_could_not_place_in_a_field_names_no_field() {
    let part = |text: &str| json!({"type": "type", "text": text});
    let tab = json!({"type": "key", "key": "tab"});
    let field = Some("AXTextField");
    assert_eq!(
        typed_into(&[part("sam"), tab, part("hunter2")], &[field, field, None]).await,
        None,
        "one part typed where no field was found keeps the batch from naming one"
    );
    assert_eq!(
        typed_into(&[part("sam"), part("more")], &[None, field]).await,
        None
    );
}

#[tokio::test]
async fn text_any_check_found_in_a_password_field_names_the_password_field() {
    let part = |text: &str| json!({"type": "type", "text": text});
    let tab = json!({"type": "key", "key": "tab"});
    let field = Some("AXTextField");
    assert_eq!(
        typed_into(
            &[part("sam"), tab, part("hunter2")],
            &[field, field, Some("AXSecureTextField")]
        )
        .await,
        Some("AXSecureTextField")
    );
    assert_eq!(
        typed_into(
            &[part("sam"), part("x")],
            &[None, Some("AXSecureTextField")]
        )
        .await,
        Some("AXSecureTextField"),
        "a password field found is said whatever else was not"
    );
}

#[tokio::test]
async fn input_without_an_expectation_is_never_checked() {
    let (outcome, stand) = ran(&[Step::Refuse, Step::Refuse], 2, None).await;
    assert!(outcome.is_ok());
    assert_eq!(stand.lock().await.log, ["post 0", "post 1"]);
}

fn referenced() -> Action {
    serde_json::from_value(json!({"type": "reference", "ref": "8123abc.12"})).unwrap()
}

#[tokio::test]
async fn a_stale_reference_is_refused_before_any_check_or_post() {
    for expect in [None, app()] {
        let stand = Stand::locked(&[Step::Post]);
        let gate = Arc::new(Gate::default());
        let outcome = {
            let mut engine = stand.lock().await;
            run(&mut *engine, &gate, vec![referenced()], expect.as_ref()).await
        };
        let reply = outcome.unwrap_err();
        assert_eq!(reply["code"], "elements");
        assert_eq!(reply["next"], LOOK_AGAIN);
        assert_eq!(reply["performed"], 0);
        assert_eq!(reply["partial"], false);
        assert_eq!(reply["uncertain"], false);

        let stand = stand.lock().await;
        assert_eq!(stand.log, ["resolve 0"], "no focus preparation");
        assert!(stand.asked.is_empty());
        assert_eq!(stand.posted, 0);
        assert_eq!(stand.releases, 1);
    }
}

/// A referenced click is checked against the window its element was found
/// in, not the batch's own expectation, and lands where the element is now.
#[tokio::test]
async fn a_found_reference_is_checked_against_its_own_window_and_clicked_where_it_is() {
    let found = Expect::new(812).unwrap().in_window(42).unwrap();
    for expect in [None, app()] {
        let stand = Stand::locked(&[Step::Post, Step::Post]);
        stand.lock().await.found = Some(Resolved::at(30.0, 40.0, found.clone()));
        let gate = Arc::new(Gate::default());
        let actions = vec![referenced(), keys(1).remove(0)];
        let outcome = {
            let mut engine = stand.lock().await;
            run(&mut *engine, &gate, actions, expect.as_ref()).await
        };
        assert!(outcome.is_ok());

        let stand = stand.lock().await;
        assert_eq!(stand.clicked, [(30.0, 40.0)]);
        assert_eq!(stand.preparations.first(), Some(&true));
        assert_eq!(stand.asked.first(), Some(&found));
        let checked = if expect.is_some() {
            vec![
                "resolve 0",
                "check 0",
                "confirm 0",
                "check 1",
                "post 0",
                "check 2",
                "post 1",
            ]
        } else {
            vec![
                "resolve 0",
                "check 0",
                "confirm 0",
                "check 1",
                "post 0",
                "post 1",
            ]
        };
        assert_eq!(stand.log, checked);
        assert_eq!(
            stand.preparations[..2],
            [true, false],
            "prepared once, then only read again"
        );
        assert_eq!(stand.asked[..2], [found.clone(), found.clone()]);
    }
}

/// A referenced click's batch reports its element as it was found, from
/// the reads its click needs anyway: nothing more is asked.
#[tokio::test]
async fn a_referenced_click_reports_the_element_it_found_without_another_read() {
    let found = Expect::new(812).unwrap().in_window(42).unwrap();
    let stand = Stand::locked(&[Step::Post]);
    stand.lock().await.found = Some(Resolved::at(30.0, 40.0, found));
    let gate = Arc::new(Gate::default());
    let done = {
        let mut engine = stand.lock().await;
        super::run(&mut *engine, &gate, vec![referenced()], None)
            .await
            .unwrap()
    };
    assert_eq!(done.reached, Some(Reached::button(30.0, 40.0)));
    let stand = stand.lock().await;
    assert_eq!(
        stand.log,
        ["resolve 0", "check 0", "confirm 0", "check 1", "post 0"]
    );
}

/// A click at a point reaches no element the batch knows of.
#[tokio::test]
async fn a_click_at_a_point_reports_no_element() {
    let stand = Stand::locked(&[Step::Post]);
    let gate = Arc::new(Gate::default());
    let click = serde_json::from_value(json!({"type": "click", "x": 30, "y": 40})).unwrap();
    let done = {
        let mut engine = stand.lock().await;
        super::run(&mut *engine, &gate, vec![click], app().as_ref())
            .await
            .unwrap()
    };
    assert_eq!(done.reached, None);
}

/// An element that changes once its app is brought forward is refused
/// after that preparation, before anything is posted.
#[tokio::test]
async fn a_reference_that_moves_while_its_app_is_brought_forward_is_never_clicked() {
    let stand = Stand::locked(&[Step::Post]);
    {
        let mut stand = stand.lock().await;
        stand.found = Some(Resolved::at(
            30.0,
            40.0,
            Expect::new(812).unwrap().in_window(42).unwrap(),
        ));
        stand.moves = true;
    }
    let gate = Arc::new(Gate::default());
    let outcome = {
        let mut engine = stand.lock().await;
        run(&mut *engine, &gate, vec![referenced()], None).await
    };
    let reply = outcome.unwrap_err();
    assert_eq!(reply["code"], "elements");
    assert_eq!(reply["next"], LOOK_AGAIN);
    assert_eq!(reply["performed"], 0);
    assert_eq!(reply["uncertain"], false);
    let stand = stand.lock().await;
    assert_eq!(stand.log, ["resolve 0", "check 0", "confirm 0"]);
    assert_eq!(stand.preparations, [true], "prepared once, never again");
    assert_eq!(stand.posted, 0);
}

/// A batch that names a window clicks a referenced element only in it,
/// refusing one found in another before any check or post.
#[tokio::test]
async fn a_reference_found_outside_the_window_named_is_refused() {
    let stand = Stand::locked(&[Step::Post]);
    stand.lock().await.found = Some(Resolved::at(
        30.0,
        40.0,
        Expect::new(812).unwrap().in_window(43).unwrap(),
    ));
    let named = Expect::new(812).unwrap().in_window(42).unwrap();
    let gate = Arc::new(Gate::default());
    let outcome = {
        let mut engine = stand.lock().await;
        run(&mut *engine, &gate, vec![referenced()], Some(&named)).await
    };
    let reply = outcome.unwrap_err();
    assert_eq!(reply["code"], "elements");
    assert_eq!(reply["performed"], 0);
    assert_eq!(reply["uncertain"], false);
    let stand = stand.lock().await;
    assert_eq!(stand.log, ["resolve 0"]);
    assert_eq!(stand.posted, 0);
}

#[test]
fn the_input_service_offers_looks_a_click_can_name() {
    assert!(CAPABILITIES.contains(&atlas_core::session::LOOKS));
    assert_eq!(atlas_core::session::LOOKS, "looks");
}

#[test]
fn the_input_service_offers_actions_on_the_elements_of_a_look() {
    assert!(CAPABILITIES.contains(&atlas_core::session::ELEMENT_ACTIONS));
    assert_eq!(atlas_core::session::ELEMENT_ACTIONS, "element-actions");
}

#[test]
fn the_input_service_says_it_names_a_password_field_by_its_subrole() {
    assert!(CAPABILITIES.contains(&atlas_core::session::TYPED_FIELDS));
}

fn pressed() -> Action {
    serde_json::from_value(json!({"type": "press", "ref": "8123abc.12"})).unwrap()
}

/// An action on an element asks for no focus and posts no event: its own
/// Accessibility request carries it out, in the window the batch names,
/// and the actions after it are checked as ever.
#[tokio::test]
async fn an_element_action_is_carried_out_with_no_focus_check_and_no_event() {
    let named = Expect::new(812).unwrap().in_window(42).unwrap();
    let stand = Stand::locked(&[Step::Post]);
    stand.lock().await.acts = Some(Ok(Acted {
        verification: Verification::Matched,
        instance: Instance::new(812, [1, 0]),
        changed: true,
        reached: None,
    }));
    let gate = Arc::new(Gate::default());
    let done = {
        let mut engine = stand.lock().await;
        let actions = vec![pressed(), keys(1).remove(0)];
        super::run(&mut *engine, &gate, actions, Some(&named))
            .await
            .unwrap()
    };
    assert_eq!(done.performed(2), 2, "both were sent");
    assert_eq!(done.acted.unwrap().verification, Verification::Matched);

    let stand = stand.lock().await;
    assert_eq!(stand.log, ["act 0", "check 0", "post 0"]);
    assert_eq!(
        stand.acted,
        [(
            Reference::parse("8123abc.12").unwrap(),
            ElementAction::Press,
            Some(42)
        )]
    );
    assert_eq!(stand.asked.len(), 1, "only the key is checked");
    assert!(stand.clicked.is_empty());
    assert_eq!(stand.posted, 2);
}

/// An action on an element reports the element as the action met it, and
/// nothing else is asked of the app for it.
#[tokio::test]
async fn an_element_action_reports_the_element_it_met() {
    let stand = Stand::locked(&[]);
    stand.lock().await.acts = Some(Ok(Acted {
        verification: Verification::NotChecked,
        instance: Instance::new(812, [1, 0]),
        changed: true,
        reached: Some(Reached::button(30.0, 40.0)),
    }));
    let gate = Arc::new(Gate::default());
    let done = {
        let mut engine = stand.lock().await;
        super::run(&mut *engine, &gate, vec![pressed()], None)
            .await
            .unwrap()
    };
    assert_eq!(done.reached, Some(Reached::button(30.0, 40.0)));
    assert_eq!(stand.lock().await.log, ["act 0"]);
}

/// An element already as asked is left: the reply says so, and nothing
/// counts as sent.
#[tokio::test]
async fn an_element_already_as_asked_is_matched_with_nothing_sent() {
    let stand = Stand::locked(&[]);
    stand.lock().await.acts = Some(Ok(Acted {
        verification: Verification::Matched,
        instance: Instance::new(812, [1, 0]),
        changed: false,
        reached: None,
    }));
    let gate = Arc::new(Gate::default());
    let mut engine = stand.lock().await;
    let done = super::run(&mut *engine, &gate, vec![pressed()], None)
        .await
        .unwrap();
    assert_eq!(done.performed(1), 0, "nothing was sent");
    let acted = done.acted.unwrap();
    assert_eq!(acted.verification, Verification::Matched);
    assert!(!acted.changed);
    assert_eq!(engine.posted, 0);
}

/// A refusal before the request that acts sent nothing; a request the app
/// did not acknowledge may have been carried out, so its outcome is unknown.
#[tokio::test]
async fn an_element_action_refused_first_is_certain_and_one_unacknowledged_is_not() {
    for (acts, code, uncertain) in [
        (None, "elements", false),
        (Some(Err(Code::Uncertain)), "uncertain", true),
    ] {
        let stand = Stand::locked(&[]);
        stand.lock().await.acts = acts;
        let gate = Arc::new(Gate::default());
        let reply = {
            let mut engine = stand.lock().await;
            run(&mut *engine, &gate, vec![pressed()], app().as_ref())
                .await
                .unwrap_err()
        };
        assert_eq!(reply["code"], code);
        assert_eq!(reply["performed"], 0);
        assert_eq!(reply["partial"], false);
        assert_eq!(reply["uncertain"], uncertain);

        let stand = stand.lock().await;
        assert_eq!(stand.log, ["act 0"]);
        assert!(stand.asked.is_empty());
        assert_eq!(stand.posted, 0);
        assert_eq!(stand.releases, 1);
    }
}

#[test]
fn a_look_request_keeps_its_epoch_and_app_refusals_for_their_guards() {
    let epoch =
        r#"{"uid":501,"securitySessionId":1,"displayId":2,"width":800,"height":600,"processId":1}"#;
    let body =
        format!(r#"{{"op":"elements","epoch":{epoch},"expect":{{"pid":812,"start":[123,456]}}}}"#);
    let Ok(Request::Elements {
        epoch: Ok(_),
        expect: Ok(Some(expect)),
    }) = Request::decode(body.as_bytes())
    else {
        panic!("expected a typed look");
    };
    assert_eq!(expect.pid(), 812);

    for body in [
        r#"{"op":"elements","epoch":null,"expect":{"pid":812}}"#.to_owned(),
        format!(r#"{{"op":"elements","epoch":{epoch},"expect":{{"pid":812,"other":1}}}}"#),
        format!(r#"{{"op":"elements","epoch":{epoch}}}"#),
    ] {
        assert!(
            matches!(
                Request::decode(body.as_bytes()),
                Ok(Request::Elements { epoch: Err(_), .. }
                    | Request::Elements {
                        expect: Err(_) | Ok(None),
                        ..
                    })
            ),
            "{body}"
        );
    }
}

/// A look reads nothing for an epoch this session does not prove now, even
/// while agents are stopped, and that refusal changes nothing.
#[tokio::test]
async fn a_look_for_another_epoch_is_refused_before_anything_is_read() {
    let directory = tempfile::tempdir().unwrap();
    let service = service(&directory.path().join("stopped"));
    service.gate.hold(true).unwrap();
    let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
    let mut stale = json!(service.epoch().unwrap());
    stale["displayId"] = json!(9);
    let request = Request::Elements {
        epoch: Ok(atlas_core::session::InputEpoch::deserialize(stale).unwrap()),
        expect: Ok(Some(Expect::new(812).unwrap())),
    };
    let error = service.dispatch(request, &phase).await.unwrap_err();
    // A test runs in no hidden session, or in one whose epoch is another.
    assert!(
        matches!(error.code, Code::SessionChanged | Code::Identity),
        "{}",
        error.error
    );
    assert!(!error.uncertain);
    assert!(matches!(
        *observed.borrow(),
        super::ConnectionPhase::Refusal
    ));
    assert_eq!(service.engine.lock().await.posted, 0);
}

#[test]
fn the_input_service_offers_the_target_check() {
    assert!(CAPABILITIES.contains(&atlas_core::session::FOCUS_APP));
    assert!(CAPABILITIES.contains(&atlas_core::session::FOCUS_DOCK));
    assert_eq!(atlas_core::session::FOCUS_APP, "focus-app");
    assert_eq!(atlas_core::session::FOCUS_DOCK, "focus-dock");
}

#[test]
fn the_input_service_offers_the_window_and_field_checks() {
    assert!(CAPABILITIES.contains(&atlas_core::session::FOCUS_WINDOW));
    assert!(CAPABILITIES.contains(&atlas_core::session::FOCUS_FIELD));
    assert_eq!(atlas_core::session::FOCUS_WINDOW, "focus-window");
    assert_eq!(atlas_core::session::FOCUS_FIELD, "focus-field");
}

/// Only the controller is answered: a process in the helper's own
/// account, where the apps under test run, is turned away.
#[tokio::test]
async fn only_the_controller_is_admitted() {
    let me = nix::unistd::getuid().as_raw();
    let (_client, server) = UnixStream::pair().unwrap();

    assert!(admitted(&server, me), "the controller");
    assert!(
        !admitted(&server, me.wrapping_add(1)),
        "the helper's own account, or any other"
    );
}

/// Deliver `line` in `chunk`-byte writes and parse it as the service would.
async fn deliver(line: Vec<u8>, chunk: usize) -> Result<Request> {
    let (mut client, mut server) = tokio::io::duplex(chunk);
    let writer = tokio::spawn(async move {
        for part in line.chunks(chunk) {
            if client.write_all(part).await.is_err() {
                break;
            }
        }
    });

    let result = read_request(&mut server).await;
    writer.abort();
    result
}

fn padded(length: usize) -> Vec<u8> {
    let mut line = br#"{"op":"hello"}"#.to_vec();
    line.resize(length, b' ');
    line.push(b'\n');
    line
}

#[tokio::test]
async fn the_line_limit_holds_however_the_request_is_fragmented() {
    for chunk in [1000, 4096, 65536] {
        let reply = deliver(padded(MAX_REQUEST), chunk).await.unwrap();
        assert!(matches!(reply, Request::Hello));
        let error = deliver(padded(MAX_REQUEST + 1), chunk).await.unwrap_err();
        assert_eq!(error.code, "protocol");
    }
}

#[tokio::test]
async fn the_other_routes_still_ignore_stop_fields() {
    let request = deliver(
        b"{\"op\":\"hello\",\"on\":true,\"on\":false}\n".to_vec(),
        64,
    )
    .await
    .unwrap();
    assert!(matches!(request, Request::Hello));
    assert_eq!(
        deliver(b"[]\n".to_vec(), 64).await.unwrap_err().code,
        Code::Input
    );
}

#[tokio::test]
async fn malformed_and_truncated_requests_are_errors() {
    assert_eq!(
        deliver(b"{not json}\n".to_vec(), 64)
            .await
            .unwrap_err()
            .code,
        "invalid-json"
    );
    assert_eq!(
        deliver(b"{\"op\":".to_vec(), 64).await.unwrap_err().code,
        "protocol"
    );
}
#[tokio::test]
async fn a_key_that_opens_a_sheet_keeps_later_keys_as_read_only_continuations() {
    let stand = Stand::locked(&[]);
    stand.lock().await.opens_sheet = true;
    let gate = Arc::new(Gate::default());
    let expect = Expect::new(812).unwrap().in_window(42).unwrap();
    let mut engine = stand.lock().await;
    run(&mut *engine, &gate, keys(2), Some(&expect))
        .await
        .unwrap();
    assert_eq!(engine.preparations, [true, false]);
    assert_eq!(engine.budgets[0], crate::native::focus::PREPARE_BUDGET);
    assert_eq!(engine.budgets[1], crate::native::focus::CHECK_BUDGET);
    assert_eq!(engine.posted, 2);
    let refused = run(&mut *engine, &gate, keys(1), Some(&expect))
        .await
        .unwrap_err();
    assert_eq!(refused["reason"], "sheet");
    assert_eq!(refused["performed"], 0);
    assert_eq!(engine.posted, 2);
}

#[tokio::test]
async fn held_input_and_waits_never_prepare_a_target() {
    let stand = Stand::locked(&[]);
    let gate = Arc::new(Gate::default());
    let expect = Expect::new(812).unwrap();
    let mut engine = stand.lock().await;
    engine.held = true;
    let actions = serde_json::from_value(json!([
        {"type":"move", "x": 20, "y":20},
        {"type":"wait", "ms":1},
        {"type":"pointer", "phase":"up", "x":20, "y":20, "button":"left"}
    ]))
    .unwrap();
    run(&mut *engine, &gate, actions, Some(&expect))
        .await
        .unwrap();
    assert_eq!(engine.preparations, [false, false]);
    assert_eq!(engine.budgets, [crate::native::focus::CHECK_BUDGET; 2]);
    assert_eq!(engine.posted, 3);
    assert!(engine.budgets.windows(2).all(|pair| pair[1] <= pair[0]));
    engine.script = vec![Step::Post, Step::Post, Step::Refuse];
    let reply = run(
        &mut *engine,
        &gate,
        vec![Action::Move { x: 20.0, y: 20.0 }],
        Some(&expect),
    )
    .await
    .unwrap_err();
    assert_eq!(reply["performed"], 0);
    assert_eq!(engine.posted, 3);
    assert_eq!(engine.preparations.last(), Some(&false));
    assert_eq!(engine.releases, 1);
}

#[test]
fn the_persons_input_cannot_name_an_agent_target() {
    let gate = Arc::new(Gate::default());
    let owner = "a".repeat(48);
    let _grant = gate
        .acquire_lasting(&owner, super::Who::Person, LASTING)
        .unwrap();
    let expect = Expect::new(812).unwrap();
    let error = super::agent_target(&gate, Some(&expect), &[]).unwrap_err();
    assert_eq!(error.code, Code::Input);
    assert_eq!(
        error.error,
        "The person's input cannot name an agent target"
    );
    for action in [referenced(), pressed()] {
        let error = super::agent_target(&gate, None, &[action]).unwrap_err();
        assert_eq!(
            error.error,
            "The person's input cannot name an agent target"
        );
    }
    super::agent_target(&gate, None, &keys(1)).unwrap();
}

struct Blocked {
    grant: super::super::lease::Grant,
    started: Option<tokio::sync::oneshot::Sender<()>>,
    resume: Option<std::sync::mpsc::Receiver<()>>,
    writes: Arc<std::sync::atomic::AtomicUsize>,
    posted: u64,
    releases: usize,
}

impl Blocked {
    /// A blocking request that writes once, waits on the barrier, and writes
    /// again only while its grant still holds.
    async fn block(&mut self) -> Result<()> {
        let grant = self.grant.clone();
        let writes = self.writes.clone();
        let started = self.started.take().unwrap();
        let resume = self.resume.take().unwrap();
        let final_grant = grant.clone();
        super::batch::checked_preparation(
            move || {
                drop(grant.authorize()?);
                writes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                started.send(()).unwrap();
                resume.recv_timeout(Duration::from_secs(3)).unwrap();
                drop(grant.authorize()?);
                writes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            },
            || {
                drop(final_grant.authorize()?);
                Ok(())
            },
        )
        .await
    }
}

impl Perform for Blocked {
    async fn require(
        &mut self,
        _: &Expect,
        _: Need,
        _: Duration,
        _: bool,
    ) -> Result<Option<&'static str>> {
        self.block().await.map(|()| None)
    }
    async fn resolve(&mut self, _: Reference) -> Result<Resolved> {
        Err(stale("No look is kept here"))
    }
    async fn confirm(&mut self, _: Resolved) -> Result<()> {
        Err(stale("No look is kept here"))
    }
    async fn act(&mut self, _: Reference, _: ElementAction, _: Option<i64>) -> Result<Acted> {
        self.block().await?;
        self.posted += 1;
        Ok(Acted {
            verification: Verification::NotChecked,
            instance: Instance::new(812, [1, 0]),
            changed: true,
            reached: None,
        })
    }
    async fn perform(&mut self, _: Action) -> Result<()> {
        self.posted += 1;
        Ok(())
    }
    fn posted(&self) -> u64 {
        self.posted
    }
    fn held_input(&self) -> bool {
        false
    }
    fn release(&mut self) -> Result<()> {
        self.releases += 1;
        Ok(())
    }
}

enum Ending {
    Abort,
    Term,
    Stop,
    Cancel,
}

/// A blocked write uses the real grant and operation registry. The
/// request owns no execution guard once the operation has started.
async fn blocked_operation(ending: Ending) {
    blocked_while(ending, keys(1)).await;
}

/// [`blocked_operation`], blocked in carrying out `actions`.
async fn blocked_while(ending: Ending, actions: Vec<Action>) {
    let shutdown = matches!(ending, Ending::Term);
    let gate = Arc::new(Gate::default());
    let owner = "a".repeat(48);
    let grant = gate
        .acquire_lasting(&owner, super::Who::Agent, LASTING)
        .unwrap();
    let operations = Arc::new(super::Operations::default());
    let (started, entered) = tokio::sync::oneshot::channel();
    let (resume, barrier) = std::sync::mpsc::channel();
    let writes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let engine = Arc::new(Mutex::new(Blocked {
        grant,
        started: Some(started),
        resume: Some(barrier),
        writes: writes.clone(),
        posted: 0,
        releases: 0,
    }));
    let observed = Arc::new(std::sync::Mutex::new(None));
    let mut requests = tokio::task::JoinSet::new();
    let request = {
        let engine = engine.clone();
        let gate = gate.clone();
        let operations = operations.clone();
        let observed = observed.clone();
        requests.spawn(async move {
            let mut cancel = super::Cancellation {
                gate: gate.clone(),
                owner: Some(owner),
            };
            let mut engine = engine.lock_owned().await;
            let outcome = operations
                .start(async move { run(&mut *engine, &gate, actions, app().as_ref()).await })
                .await
                .unwrap();
            let reply = outcome.await.unwrap();
            cancel.owner = None;
            *observed.lock().unwrap() = Some(reply);
            Ok(())
        })
    };
    entered.await.unwrap();
    assert!(engine.try_lock().is_err());
    if matches!(ending, Ending::Stop | Ending::Cancel) {
        if matches!(ending, Ending::Stop) {
            gate.hold(true).unwrap();
        } else {
            gate.cancel(None).unwrap();
        }
        assert!(!gate.active().unwrap(), "cancellation did not wait for AX");
    } else if !shutdown {
        request.abort();
        while requests.join_next().await.is_some() {}
        assert!(
            !gate.active().unwrap(),
            "request cancellation did not wait for AX"
        );
        let abandoned = super::drain_and_release(&operations, &engine, &gate);
        assert!(
            tokio::time::timeout(Duration::from_millis(30), abandoned)
                .await
                .is_err()
        );
        assert_eq!(
            operations.registered(),
            1,
            "cancelling cleanup kept the worker registered"
        );
    }
    {
        let cleanup = async {
            if shutdown {
                super::finish(&gate, &mut requests, &operations, &engine).await
            } else {
                super::drain_and_release(&operations, &engine, &gate).await
            }
        };
        tokio::pin!(cleanup);
        assert!(
            tokio::time::timeout(Duration::from_millis(30), &mut cleanup)
                .await
                .is_err(),
            "cleanup returned while a write was in flight"
        );
        assert!(!gate.active().unwrap());
        let competing = async {
            let engine = engine.lock().await;
            assert!(engine.releases >= 1, "cleanup acknowledged release first");
            let who = if matches!(ending, Ending::Stop) {
                super::Who::Person
            } else {
                super::Who::Agent
            };
            gate.acquire_lasting(&"b".repeat(48), who, LASTING)
                .map(drop)
        };
        tokio::pin!(competing);
        assert!(
            tokio::time::timeout(Duration::from_millis(30), &mut competing)
                .await
                .is_err(),
            "a second holder passed the blocked worker"
        );
        resume.send(()).unwrap();
        let (cleaned, _) = tokio::join!(cleanup, competing);
        cleaned.unwrap();
    }
    assert_eq!(
        writes.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "only the bounded in-flight write finished"
    );
    let engine = engine.lock().await;
    assert_eq!(engine.posted, 0);
    assert!(engine.releases >= 1);
    assert_eq!(operations.registered(), 0);
    if matches!(ending, Ending::Stop | Ending::Cancel) {
        requests.join_next().await.unwrap().unwrap().unwrap();
        let reply = observed.lock().unwrap().take().unwrap().unwrap_err();
        let (code, instruction) = if matches!(ending, Ending::Stop) {
            (Code::Stopped, Error::stopped().error)
        } else {
            (Code::Cancelled, "Native control was stopped".to_owned())
        };
        assert_eq!(reply["code"], json!(code));
        assert_eq!(reply["error"], instruction);
        assert_eq!(reply["performed"], 0);
        assert_eq!(reply["uncertain"], false);
    }
}

#[tokio::test]
async fn aborting_a_request_waits_for_its_preparation_before_cleanup() {
    blocked_operation(Ending::Abort).await;
}

#[tokio::test]
async fn aborting_a_request_after_registration_keeps_its_work_until_cleanup() {
    use std::{future::Future, task::Context, task::Poll, task::Waker};

    let operations = super::Operations::default();
    let gate = Arc::new(Gate::default());
    let owner = "a".repeat(48);
    let grant = gate
        .acquire_lasting(&owner, super::Who::Agent, LASTING)
        .unwrap();
    let stand = Stand::locked(&[]);
    let (resume, barrier) = tokio::sync::oneshot::channel::<()>();
    {
        let request = async {
            let _cancel = super::Cancellation {
                gate: gate.clone(),
                owner: Some(owner),
            };
            let engine = stand.clone().lock_owned().await;
            operations
                .start(async move {
                    let _engine = engine;
                    barrier.await.unwrap();
                    grant.authorize().map(drop)
                })
                .await
                .unwrap()
                .await
        };
        tokio::pin!(request);
        let mut context = Context::from_waker(Waker::noop());
        assert!(matches!(request.as_mut().poll(&mut context), Poll::Pending));
        assert_eq!(operations.registered(), 1);
    }
    assert!(!gate.active().unwrap());
    assert!(stand.try_lock().is_err());
    {
        let cleanup = super::drain_and_release(&operations, &stand, &gate);
        tokio::pin!(cleanup);
        let mut context = Context::from_waker(Waker::noop());
        assert!(matches!(cleanup.as_mut().poll(&mut context), Poll::Pending));
        assert_eq!(operations.registered(), 1);
        resume.send(()).unwrap();
        cleanup.await.unwrap();
    }
    assert_eq!(operations.registered(), 0);
    let engine = stand.lock().await;
    assert_eq!(engine.releases, 1);
    assert_eq!(engine.posted, 0);
}

#[tokio::test]
async fn term_shutdown_drains_preparation_before_returning() {
    blocked_operation(Ending::Term).await;
}

#[tokio::test]
async fn repeated_input_under_one_retained_lease_keeps_the_operation_registry_bounded() {
    let gate = Arc::new(Gate::default());
    let owner = "a".repeat(48);
    let grant = gate
        .acquire_lasting(&owner, super::Who::Agent, LASTING)
        .unwrap();
    let operations = super::Operations::default();
    let engine = Stand::locked(&[]);
    for _ in 0..1000 {
        let mut engine = engine.clone().lock_owned().await;
        let grant = grant.clone();
        let gate = gate.clone();
        operations
            .start(async move {
                drop(grant.authorize()?);
                Ok::<_, Error>(run(&mut *engine, &gate, keys(1), None).await)
            })
            .await
            .unwrap()
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(operations.registered() <= 1);
    }
    assert!(gate.active().unwrap());
    drop(grant.authorize().unwrap());
    let engine = engine.lock().await;
    assert_eq!(engine.posted, 1000);
    assert_eq!(engine.releases, 0);
    operations.drain().await.unwrap();
    assert_eq!(operations.registered(), 0);
}

#[tokio::test]
async fn reaping_completed_operations_keeps_running_work_registered() {
    let operations = super::Operations::default();
    let (resume, barrier) = tokio::sync::oneshot::channel::<()>();
    let running = operations.start(barrier).await.unwrap();
    for _ in 0..100 {
        operations.start(async {}).await.unwrap().await.unwrap();
        assert_eq!(operations.registered(), 2);
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(10), operations.drain())
            .await
            .is_err()
    );
    assert_eq!(operations.registered(), 2);
    resume.send(()).unwrap();
    running.await.unwrap().unwrap();
    operations.drain().await.unwrap();
    assert_eq!(operations.registered(), 0);
}

#[tokio::test]
async fn a_reaped_operation_join_failure_remains_uncertain_during_cleanup() {
    use std::error::Error as _;

    let operations = super::Operations::default();
    let outcome = operations
        .start(async { panic!("worker failed") })
        .await
        .unwrap();
    assert!(outcome.await.is_err());
    let error = operations.start(async {}).await.unwrap_err();
    assert_eq!(error.code, Code::Uncertain);
    assert!(error.uncertain);
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<super::operations::Cause>()
            .unwrap()
            .downcast_ref::<tokio::task::JoinError>()
            .unwrap()
            .is_panic()
    );
    assert_eq!(operations.registered(), 0);
    let error = operations.drain().await.unwrap_err();
    assert_eq!(error.code, Code::Uncertain);
    assert!(error.uncertain);
    let stand = Stand::locked(&[]);
    let error = super::drain_and_release(&operations, &stand, &Gate::default())
        .await
        .unwrap_err();
    assert_eq!(error.error, "The input worker did not report its outcome");
    assert!(error.uncertain);
    assert_eq!(stand.lock().await.releases, 1);
    assert!(operations.check().is_err());
}

#[test]
fn successful_cleanup_and_single_failures_keep_their_original_shapes() {
    use std::error::Error as _;

    super::combine_cleanup(Ok(()), Ok(())).unwrap();
    for primary in [false, true] {
        let error = Error::new(Code::Busy, "Original failure")
            .held_by(atlas_core::error::Holder::Agent)
            .with("detail", json!({"count": 3}))
            .uncertain()
            .with_source(std::io::Error::from_raw_os_error(13));
        let expected = serde_json::to_vec(&error).unwrap();
        let error = if primary {
            super::combine_cleanup(Err(error), Ok(()))
        } else {
            super::combine_cleanup(Ok(()), Err(error))
        }
        .unwrap_err();
        assert_eq!(serde_json::to_vec(&error).unwrap(), expected);
        assert_eq!(error.to_string(), "Original failure");
        assert_eq!(
            error
                .source()
                .unwrap()
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .raw_os_error(),
            Some(13)
        );
    }
}

#[test]
fn primary_and_cleanup_failures_are_both_retained() {
    use std::error::Error as _;

    for primary_unknown in [false, true] {
        for cleanup_unknown in [false, true] {
            let mut primary = Error::new(Code::Busy, "Original failure")
                .held_by(atlas_core::error::Holder::Agent)
                .with("count", json!(3))
                .with_source(std::io::Error::from_raw_os_error(13));
            primary.uncertain = primary_unknown;
            let mut cleanup = Error::new(Code::Cleanup, "Original cleanup failure")
                .with("private", json!("cleanup detail"))
                .with_source(std::io::Error::from_raw_os_error(5));
            cleanup.uncertain = cleanup_unknown;
            let primary_bytes = serde_json::to_vec(&primary).unwrap();
            let cleanup_bytes = serde_json::to_vec(&cleanup).unwrap();
            let error = super::combine_cleanup(Err(primary), Err(cleanup)).unwrap_err();
            let pending = primary_unknown || cleanup_unknown;
            assert_eq!(
                json!(error),
                json!({
                    "ok": false,
                    "code": "busy",
                    "error": format!("Original failure; cleanup: cleanup (recovery pending: {pending})"),
                    "uncertain": pending,
                    "holder": "agent",
                    "count": 3,
                })
            );
            let pair = error
                .source()
                .unwrap()
                .downcast_ref::<super::operations::CleanupFailures>()
                .unwrap();
            assert_eq!(serde_json::to_vec(&pair.primary).unwrap(), primary_bytes);
            assert_eq!(serde_json::to_vec(&pair.cleanup).unwrap(), cleanup_bytes);
            assert!(std::ptr::eq(
                pair.source().unwrap().downcast_ref::<Error>().unwrap(),
                &pair.primary
            ));
            for (original, errno) in [(&pair.primary, 13), (&pair.cleanup, 5)] {
                assert_eq!(
                    original
                        .source()
                        .unwrap()
                        .downcast_ref::<std::io::Error>()
                        .unwrap()
                        .raw_os_error(),
                    Some(errno)
                );
            }
        }
    }
}

#[tokio::test]
async fn a_cleanup_failure_cannot_turn_unknown_work_into_success() {
    use std::error::Error as _;

    let operations = super::Operations::default();
    let outcome = operations
        .start(async { panic!("worker failed") })
        .await
        .unwrap();
    assert!(outcome.await.is_err());
    let stand = Stand::locked(&[]);
    {
        let mut engine = stand.lock().await;
        engine.held = true;
        engine.release_fails = true;
    }
    let gate = Gate::default();
    let error = super::drain_and_release(&operations, &stand, &gate)
        .await
        .unwrap_err();
    assert_eq!(error.code, Code::Uncertain);
    assert!(error.uncertain);
    let pair = error
        .source()
        .unwrap()
        .downcast_ref::<super::operations::CleanupFailures>()
        .unwrap();
    assert_eq!(
        pair.primary.error,
        "The input worker did not report its outcome"
    );
    assert_eq!(pair.cleanup.error, "Held input could not be released");
    assert!(stand.lock().await.held);
    assert!(operations.start(async {}).await.unwrap_err().uncertain);
    stand.lock().await.release_fails = false;
    assert!(
        super::drain_and_release(&operations, &stand, &gate)
            .await
            .unwrap_err()
            .uncertain
    );
    assert!(!stand.lock().await.held);
    assert!(operations.start(async {}).await.unwrap_err().uncertain);
    super::reset(&operations, &stand, &gate, Perform::release)
        .await
        .unwrap();
    operations.start(async {}).await.unwrap().await.unwrap();
}

#[tokio::test]
async fn a_gate_failure_does_not_hide_a_drain_failure() {
    use std::error::Error as _;

    let operations = super::Operations::default();
    let outcome = operations
        .start(async { panic!("worker failed") })
        .await
        .unwrap();
    assert!(outcome.await.is_err());
    let gate = Arc::new(Gate::default());
    let grant = gate
        .acquire_lasting(&"a".repeat(48), super::Who::Agent, LASTING)
        .unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = grant.authorize().unwrap();
            panic!("poison ownership");
        }))
        .is_err()
    );
    let stand = Stand::locked(&[]);
    stand.lock().await.held = true;
    let error = super::drain_and_release(&operations, &stand, &gate)
        .await
        .unwrap_err();
    assert_eq!(error.code, Code::Uncertain);
    assert!(error.uncertain);
    let pair = error
        .source()
        .unwrap()
        .downcast_ref::<super::operations::CleanupFailures>()
        .unwrap();
    assert_eq!(pair.primary.code, Code::Uncertain);
    assert_eq!(pair.cleanup.code, Code::Native);
    assert_eq!(pair.cleanup.error, "Input ownership failed");
    let engine = stand.lock().await;
    assert_eq!(engine.releases, 0, "unknown ownership cannot allow release");
    assert!(engine.held);
    assert!(operations.check().unwrap_err().uncertain);
    drop(engine);
    let mut requests = tokio::task::JoinSet::new();
    let error = super::finish(&gate, &mut requests, &operations, &stand)
        .await
        .unwrap_err();
    assert_eq!(error.code, Code::Native, "closing failed first");
    assert!(error.uncertain);
    let pair = error
        .source()
        .unwrap()
        .downcast_ref::<super::operations::CleanupFailures>()
        .unwrap();
    assert_eq!(pair.primary.error, "Input ownership failed");
    assert_eq!(pair.cleanup.code, Code::Uncertain);
    assert!(pair.cleanup.source().is_some());
    assert_eq!(stand.lock().await.releases, 0);
}

#[test]
fn cleanup_diagnostics_do_not_reveal_private_causes() {
    use std::error::Error as _;

    let secret = "private-owner-input-cause".repeat(1024);
    let primary =
        Error::new(Code::Native, "Input failed").with_source(std::io::Error::other(secret.clone()));
    let cleanup = Error::new(Code::Cleanup, secret.clone())
        .with("private", json!(secret))
        .with_source(std::io::Error::other(secret.clone()))
        .uncertain();
    let error = super::combine_cleanup(Err(primary), Err(cleanup)).unwrap_err();
    let pair = error
        .source()
        .unwrap()
        .downcast_ref::<super::operations::CleanupFailures>()
        .unwrap();
    for diagnostic in [
        serde_json::to_string(&error).unwrap(),
        error.to_string(),
        format!("{error:?}"),
        pair.to_string(),
        format!("{pair:?}"),
    ] {
        assert!(!diagnostic.contains("private-owner-input-cause"));
        assert!(diagnostic.len() < 256);
    }
    assert_eq!(pair.cleanup.error, secret);
    assert_eq!(pair.cleanup.details["private"], secret);
    assert_eq!(pair.primary.source().unwrap().to_string(), secret);
    assert_eq!(pair.cleanup.source().unwrap().to_string(), secret);
}

#[tokio::test]
async fn a_successful_reset_recovers_named_and_unnamed_input_after_a_join_failure() {
    let operations = super::Operations::default();
    let gate = Arc::new(Gate::default());
    let owner = "a".repeat(48);
    let old_grant = gate
        .acquire_lasting(&owner, super::Who::Agent, LASTING)
        .unwrap();
    let stand = Stand::locked(&[]);
    stand.lock().await.held = true;
    let outcome = operations
        .start(async { panic!("worker failed") })
        .await
        .unwrap();
    assert!(outcome.await.is_err());
    operations.reap().await.unwrap();
    assert!(operations.check().unwrap_err().uncertain);

    let cancelled = gate.clone();
    super::reset(&operations, &stand, &gate, move |engine| {
        assert!(!cancelled.active()?);
        engine.assert_locked();
        engine.release()
    })
    .await
    .unwrap();
    assert!(old_grant.authorize().is_err());
    operations.check().unwrap();
    assert!(!stand.lock().await.held);
    assert_eq!(stand.lock().await.releases, 1);

    let grant = gate
        .acquire_lasting(&owner, super::Who::Agent, LASTING)
        .unwrap();
    let named = Expect::new(812)
        .unwrap()
        .in_window(42)
        .unwrap()
        .in_field("Email")
        .unwrap();
    for expect in [Some(named), None] {
        let mut engine = stand.clone().lock_owned().await;
        let grant = grant.clone();
        let gate = gate.clone();
        operations
            .start(async move {
                drop(grant.authorize()?);
                Ok::<_, Error>(run(&mut *engine, &gate, keys(1), expect.as_ref()).await)
            })
            .await
            .unwrap()
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
    assert_eq!(stand.lock().await.posted, 2);
    gate.cancel(None).unwrap();
    super::drain_and_release(&operations, &stand, &gate)
        .await
        .unwrap();
}

#[tokio::test]
async fn reset_joins_registered_work_before_cleanup_and_clearing_failure() {
    let operations = super::Operations::default();
    let gate = Arc::new(Gate::default());
    let _grant = gate
        .acquire_lasting(&"a".repeat(48), super::Who::Agent, LASTING)
        .unwrap();
    let stand = Stand::locked(&[]);
    let (resume, barrier) = tokio::sync::oneshot::channel::<()>();
    let running = operations.start(barrier).await.unwrap();
    let outcome = operations
        .start(async { panic!("worker failed") })
        .await
        .unwrap();
    assert!(outcome.await.is_err());
    operations.reap().await.unwrap();
    let recovery = super::reset(&operations, &stand, &gate, Perform::release);
    tokio::pin!(recovery);
    assert!(
        tokio::time::timeout(Duration::from_millis(10), &mut recovery)
            .await
            .is_err()
    );
    assert!(!gate.active().unwrap());
    assert!(operations.check().unwrap_err().uncertain);
    assert_eq!(stand.lock().await.releases, 0);
    assert_eq!(operations.registered(), 1);
    resume.send(()).unwrap();
    recovery.await.unwrap();
    running.await.unwrap().unwrap();
    assert_eq!(stand.lock().await.releases, 1);
    operations.drain().await.unwrap();
    assert_eq!(operations.registered(), 0);
}

#[tokio::test]
async fn cancelling_reset_before_drain_or_rebind_keeps_recovery_pending() {
    use std::{future::Future, task::Context, task::Poll, task::Waker};

    for before_rebind in [false, true] {
        let operations = super::Operations::default();
        let gate = Arc::new(Gate::default());
        let _grant = gate
            .acquire_lasting(&"a".repeat(48), super::Who::Agent, LASTING)
            .unwrap();
        let stand = Stand::locked(&[]);
        let (resume, barrier) = tokio::sync::oneshot::channel::<()>();
        let mut resume = Some(resume);
        let running = operations.start(barrier).await.unwrap();
        let engine_guard = if before_rebind {
            resume.take().unwrap().send(()).unwrap();
            running.await.unwrap().unwrap();
            operations.drain().await.unwrap();
            Some(stand.lock().await)
        } else {
            // Leave the registered operation waiting at its drain barrier.
            None
        };
        {
            let recovery = super::reset(&operations, &stand, &gate, Perform::release);
            tokio::pin!(recovery);
            let mut context = Context::from_waker(Waker::noop());
            assert!(matches!(
                recovery.as_mut().poll(&mut context),
                Poll::Pending
            ));
            assert!(
                !gate.active().unwrap(),
                "cancellation did not need the engine"
            );
            assert!(operations.check().unwrap_err().uncertain);
        }
        assert!(operations.start(async {}).await.unwrap_err().uncertain);
        if !before_rebind {
            assert_eq!(operations.registered(), 1);
        }
        drop(resume);
        drop(engine_guard);
        super::reset(&operations, &stand, &gate, Perform::release)
            .await
            .unwrap();
        assert_eq!(stand.lock().await.releases, 1);
        operations.check().unwrap();
        assert_eq!(operations.registered(), 0);
    }
}

#[tokio::test]
async fn a_failed_reset_cleanup_keeps_input_uncertain_until_a_successful_reset() {
    let operations = super::Operations::default();
    let gate = Arc::new(Gate::default());
    let _grant = gate
        .acquire_lasting(&"a".repeat(48), super::Who::Agent, LASTING)
        .unwrap();
    let stand = Stand::locked(&[]);
    {
        let mut engine = stand.lock().await;
        engine.held = true;
        engine.release_fails = true;
    }
    let error = super::reset(&operations, &stand, &gate, Perform::release)
        .await
        .unwrap_err();
    assert!(error.uncertain);
    assert!(!gate.active().unwrap());
    assert!(operations.check().unwrap_err().uncertain);
    assert!(operations.drain().await.unwrap_err().uncertain);
    assert!(stand.lock().await.held);
    for expect in [app(), None] {
        let mut engine = stand.clone().lock_owned().await;
        let gate = gate.clone();
        assert!(
            operations
                .start(async move { run(&mut *engine, &gate, keys(1), expect.as_ref()).await })
                .await
                .unwrap_err()
                .uncertain
        );
    }
    assert_eq!(stand.lock().await.posted, 0);
    // A timer or release can finish cleanup but cannot acknowledge recovery.
    stand.lock().await.release_fails = false;
    assert!(
        super::drain_and_release(&operations, &stand, &gate)
            .await
            .unwrap_err()
            .uncertain
    );
    assert!(!stand.lock().await.held);
    assert!(operations.check().is_err());
    super::reset(&operations, &stand, &gate, Perform::release)
        .await
        .unwrap();
    operations.check().unwrap();
    operations.drain().await.unwrap();
}

#[tokio::test]
async fn a_preparation_worker_join_failure_keeps_the_outcome_unknown() {
    use std::error::Error as _;

    let error = super::batch::preparation_worker::<()>(|| panic!("worker failed"))
        .await
        .unwrap_err();
    assert_eq!(error.code, Code::Uncertain);
    assert!(error.uncertain);
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<tokio::task::JoinError>()
            .unwrap()
            .is_panic()
    );
    let stand = Stand::locked(&[]);
    let mut engine = stand.lock().await;
    let reply = super::batch::failed(&mut *engine, error, 0, 0).reply();
    assert_eq!(reply["uncertain"], true);
    assert_eq!(reply["partial"], false);
    assert_eq!(reply["performed"], 0);
}

#[tokio::test]
async fn a_panicking_engine_worker_leaves_outcomes_unknown_until_recovery() {
    let operations = super::Operations::default();
    let stand = Stand::locked(&[]);
    let engine = stand.clone().lock_owned().await;
    let Err(error) = operations
        .clean_up::<_, ()>(engine, |_| panic!("engine worker failed"))
        .await
    else {
        panic!("a panicking worker reported an outcome");
    };
    assert_eq!(error.code, Code::Uncertain);
    assert!(error.uncertain);
    assert!(operations.check().unwrap_err().uncertain);
    assert_eq!(operations.registered(), 0);
    assert!(stand.try_lock().is_ok(), "the engine is free again");

    super::reset(&operations, &stand, &Gate::default(), Perform::release)
        .await
        .unwrap();
    operations.check().unwrap();
    assert_eq!(stand.lock().await.releases, 1);
}

#[tokio::test]
async fn stopping_during_preparation_cancels_ownership_without_waiting_for_the_engine() {
    blocked_operation(Ending::Stop).await;
}

#[tokio::test]
async fn cancelling_during_preparation_keeps_the_observed_cancellation_reply() {
    blocked_operation(Ending::Cancel).await;
}

/// Stop and the person's Take Control end an agent's ownership while its
/// action on an element waits on Accessibility, which stays registered
/// until it returns; aborting the request or shutting down waits for it.
#[tokio::test]
async fn an_element_action_blocked_in_accessibility_never_holds_up_stop_or_cancel() {
    for ending in [Ending::Stop, Ending::Cancel, Ending::Abort, Ending::Term] {
        blocked_while(ending, vec![pressed()]).await;
    }
}

#[tokio::test]
async fn all_text_parts_spend_the_same_focus_budget() {
    let stand = Stand::locked(&[]);
    let mut engine = stand.lock().await;
    engine.check_delay = Duration::from_millis(5);
    engine.clock = Some(Instant::now());
    let gate = Gate::default();
    let actions = (0..3)
        .map(|_| Action::Type {
            text: "hello".into(),
        })
        .collect();
    run(&mut *engine, &gate, actions, app().as_ref())
        .await
        .unwrap();
    assert_eq!(
        engine.budgets,
        [
            crate::native::focus::PREPARE_BUDGET,
            crate::native::focus::PREPARE_BUDGET - Duration::from_millis(5),
            crate::native::focus::PREPARE_BUDGET - Duration::from_millis(10),
        ]
    );
    assert_eq!(engine.preparations, [true, true, true]);
}

#[tokio::test]
async fn failed_focus_checks_spend_budget_and_untargeted_actions_spend_none() {
    let stand = Stand::locked(&[Step::Refuse]);
    let mut engine = stand.lock().await;
    engine.clock = Some(Instant::now());
    engine.check_delay = Duration::from_millis(5);
    let mut budget = Duration::from_millis(2);
    let error = super::batch::require_within(
        &mut *engine,
        app().as_ref().unwrap(),
        Need::Keys,
        &mut budget,
        true,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, Code::FocusChanged);
    assert_eq!(error.details["reason"], "app");
    assert_eq!(budget, Duration::ZERO);
    assert_eq!(engine.budgets, [Duration::from_millis(2)]);

    let before = engine.clock;
    let mut untouched = crate::native::focus::PREPARE_BUDGET;
    assert_eq!(
        super::batch::require_within(
            &mut *engine,
            app().as_ref().unwrap(),
            Need::Nothing,
            &mut untouched,
            false,
        )
        .await
        .unwrap(),
        None
    );
    assert_eq!(untouched, crate::native::focus::PREPARE_BUDGET);
    assert_eq!(engine.clock, before);
    assert_eq!(engine.budgets, [Duration::from_millis(2)]);
}

fn display_epoch() -> atlas_core::session::InputEpoch {
    atlas_core::session::InputEpoch {
        uid: 501,
        security_session_id: 1,
        display_id: 2,
        width: 800.0,
        height: 600.0,
        process_id: 1,
    }
}

/// A service whose engine journals into `directory` and posts only to `tap`.
fn tapped_service(directory: &std::path::Path, tap: Tap, gate: Arc<Gate>) -> Arc<super::Service> {
    let epoch = display_epoch();
    let mut engine = super::Engine::tapped(epoch.clone(), Some(directory.join("held.json")), tap);
    engine.activity = Some(directory.join("input.activity"));
    Arc::new(super::Service {
        looks: engine.looks.clone(),
        engine: Arc::new(Mutex::new(engine)),
        operations: super::Operations::default(),
        gate,
        epoch: Arc::new(super::RwLock::new(epoch)),
        build: None,
        executable: None,
        stopping: tokio::sync::Notify::new(),
        stop_marker: directory.join("stopped"),
        activity: directory.join("input.activity"),
        seen: std::sync::Mutex::new(None),
    })
}

fn posts(seen: &std::sync::Mutex<Vec<Seen>>) -> Vec<Posted> {
    seen.lock()
        .unwrap()
        .iter()
        .filter_map(|seen| match seen {
            Seen::Post(posted) => Some(*posted),
            Seen::Write { .. } | Seen::Check => None,
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq)]
enum Opener {
    Test,
    Watchdog,
}

/// Where a journal write waits. The test opens it; when the runtime stalls
/// behind the write so that the test cannot, a watchdog thread opens it
/// instead, and the test can tell.
#[derive(Default)]
struct Hold {
    opened: std::sync::Mutex<Option<Opener>>,
    changed: std::sync::Condvar,
}

impl Hold {
    fn watched() -> Arc<Self> {
        let hold = Arc::new(Self::default());
        let watched = hold.clone();
        std::thread::spawn(move || {
            let opened = watched.opened.lock().unwrap();
            let (mut opened, _) = watched
                .changed
                .wait_timeout_while(opened, WATCHDOG, |opened| opened.is_none())
                .unwrap();
            if opened.is_none() {
                *opened = Some(Opener::Watchdog);
                watched.changed.notify_all();
            }
        });
        hold
    }

    fn wait(&self) {
        let opened = self.opened.lock().unwrap();
        drop(
            self.changed
                .wait_while(opened, |opened| opened.is_none())
                .unwrap(),
        );
    }

    fn stalled(&self) -> bool {
        *self.opened.lock().unwrap() == Some(Opener::Watchdog)
    }

    fn open(&self) {
        let mut opened = self.opened.lock().unwrap();
        opened.get_or_insert(Opener::Test);
        self.changed.notify_all();
    }
}

/// A tap whose first journal write after `armed` is set says so and waits
/// at `hold`.
fn blocking_tap(
    armed: Arc<std::sync::atomic::AtomicBool>,
    hold: Arc<Hold>,
) -> (
    Tap,
    Arc<std::sync::Mutex<Vec<Seen>>>,
    tokio::sync::oneshot::Receiver<()>,
) {
    let (entered, entering) = tokio::sync::oneshot::channel();
    let mut entered = Some(entered);
    let (tap, seen) = Tap::new();
    let tap = tap.writing(Box::new(move |_| {
        if armed.load(std::sync::atomic::Ordering::SeqCst)
            && let Some(entered) = entered.take()
        {
            entered.send(()).unwrap();
            hold.wait();
        }
        Ok(())
    }));
    (tap, seen, entering)
}

fn pending<F: std::future::Future>(future: std::pin::Pin<&mut F>) -> bool {
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    future.poll(&mut context).is_pending()
}

#[tokio::test]
async fn a_blocked_journal_leaves_cancellation_and_the_service_loop_responsive() {
    let directory = tempfile::tempdir().unwrap();
    let hold = Hold::watched();
    let armed = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let (tap, seen, entered) = blocking_tap(armed, hold.clone());
    let service = tapped_service(directory.path(), tap, Arc::default());
    let owner = "a".repeat(48);
    service
        .gate
        .acquire_lasting(&owner, super::Who::Agent, LASTING)
        .unwrap();
    let input = tokio::spawn({
        let service = service.clone();
        let owner = owner.clone();
        async move {
            let (phase, _) = super::watch::channel(super::ConnectionPhase::Refusal);
            let actions =
                vec![serde_json::from_value(json!({"type": "move", "x": 10, "y": 20})).unwrap()];
            service
                .input(
                    owner,
                    Ok(display_epoch()),
                    Ok(actions),
                    Ok(None),
                    false,
                    &phase,
                )
                .await
        }
    });
    entered.await.unwrap();

    let (mut client, server) = UnixStream::pair().unwrap();
    let (phase, _) = super::watch::channel(super::ConnectionPhase::Refusal);
    let hello = tokio::spawn(super::handle(server, service.clone(), phase));
    client.write_all(b"{\"op\":\"hello\"}\n").await.unwrap();
    hello.await.unwrap().unwrap();
    let mut reply = Vec::new();
    tokio::io::AsyncReadExt::read_to_end(&mut client, &mut reply)
        .await
        .unwrap();
    assert!(reply.ends_with(b"\n"), "the service answered");
    assert!(
        !hold.stalled(),
        "the service stalled behind a journal write"
    );

    let (phase, _) = super::watch::channel(super::ConnectionPhase::Refusal);
    let release = service.release(owner, &phase);
    tokio::pin!(release);
    assert!(pending(release.as_mut()), "release waits for the write");
    assert!(!service.gate.active().unwrap(), "ownership ended at once");
    let acquire = service.acquire(
        "b".repeat(48),
        Ok(display_epoch()),
        super::Who::Agent,
        &phase,
    );
    tokio::pin!(acquire);
    assert!(
        pending(acquire.as_mut()),
        "no controller passes the blocked write"
    );
    assert!(!hold.stalled());

    hold.open();
    let reply = input.await.unwrap().unwrap();
    assert_eq!(reply["code"], json!(Code::Cancelled));
    assert_eq!(reply["performed"], 0);
    assert!(posts(&seen).is_empty(), "nothing was posted");
    acquire.await.unwrap();
    release.await.unwrap();
    assert!(service.gate.active().unwrap());
}

#[tokio::test]
async fn an_event_is_posted_only_after_its_conservative_held_state_is_durable() {
    let click = |modifiers: &[&str]| -> Vec<Action> {
        vec![
            serde_json::from_value(
                json!({"type": "click", "x": 30, "y": 40, "modifiers": modifiers}),
            )
            .unwrap(),
        ]
    };
    let owner = "a".repeat(48);

    let directory = tempfile::tempdir().unwrap();
    let (tap, seen) = Tap::new();
    let service = tapped_service(directory.path(), tap, Arc::default());
    service
        .gate
        .acquire_lasting(&owner, super::Who::Agent, LASTING)
        .unwrap();
    let (phase, _) = super::watch::channel(super::ConnectionPhase::Refusal);
    let reply = service
        .input(
            owner.clone(),
            Ok(display_epoch()),
            Ok(click(&["cmd"])),
            Ok(None),
            false,
            &phase,
        )
        .await
        .unwrap();
    assert_eq!(reply["ok"], true);
    let seen = seen.lock().unwrap().clone();
    let mut posted = 0;
    for (index, event) in seen.iter().enumerate() {
        let Seen::Post(event) = event else {
            continue;
        };
        posted += 1;
        assert_eq!(seen[index - 1], Seen::Check, "checked after the write");
        let Seen::Write { button, flags, .. } = &seen[index - 2] else {
            panic!("{event:?} was posted before its state was written");
        };
        assert_eq!(flags & event.flags, event.flags, "{event:?} flags held");
        if [
            objc2_core_graphics::CGEventType::LeftMouseDown.0,
            objc2_core_graphics::CGEventType::LeftMouseUp.0,
        ]
        .contains(&event.kind)
        {
            assert!(button.is_some(), "{event:?} button held until it is up");
        }
        if event.kind == objc2_core_graphics::CGEventType::KeyUp.0 {
            assert_eq!(event.key, 55);
            assert_ne!(
                flags & atlas_core::actions::COMMAND,
                0,
                "command held until it is up"
            );
        }
    }
    assert_eq!(
        posted, 4,
        "move, press, release and the command key's release"
    );
    let held = crate::native::journal::load(&directory.path().join("held.json"), &display_epoch())
        .unwrap()
        .unwrap();
    assert!(held.button.is_none() && held.keys.is_empty() && held.flags == 0);

    for cancel in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let gate = Arc::new(Gate::default());
        let cancelling = gate.clone();
        let mut first = true;
        let (tap, seen) = Tap::new();
        let tap = tap.writing(Box::new(move |_| {
            if std::mem::take(&mut first) {
                if cancel {
                    cancelling.cancel(None)?;
                } else {
                    return Err(Error::new(Code::Native, "The disk is full"));
                }
            }
            Ok(())
        }));
        let service = tapped_service(directory.path(), tap, gate);
        service
            .gate
            .acquire_lasting(&owner, super::Who::Agent, LASTING)
            .unwrap();
        let reply = service
            .input(
                owner.clone(),
                Ok(display_epoch()),
                Ok(click(&[])),
                Ok(None),
                false,
                &phase,
            )
            .await
            .unwrap();
        let code = if cancel {
            Code::Cancelled
        } else {
            Code::Native
        };
        assert_eq!(reply["code"], json!(code));
        assert_eq!(reply["performed"], 0);
        assert_eq!(reply["partial"], false);
        assert!(posts(&seen).is_empty(), "nothing was posted");
    }
}

#[tokio::test]
async fn expiry_cleanup_remains_registered_until_its_journal_finishes() {
    let directory = tempfile::tempdir().unwrap();
    let hold = Hold::watched();
    let armed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (tap, seen, entered) = blocking_tap(armed.clone(), hold.clone());
    let service = tapped_service(directory.path(), tap, Arc::default());
    let down =
        serde_json::from_value(json!({"type": "pointer", "phase": "down", "x": 50, "y": 60}))
            .unwrap();
    service.engine.lock().await.perform(down).await.unwrap();
    assert!(service.engine.lock().await.held_input());
    armed.store(true, std::sync::atomic::Ordering::SeqCst);

    for _ in 0..3 {
        service.expire().await.unwrap();
        assert!(!hold.stalled(), "expiry waited for the journal");
        assert_eq!(
            service.operations.registered(),
            1,
            "one registered cleanup however many ticks pass"
        );
    }
    entered.await.unwrap();
    assert!(
        service.engine.try_lock().is_err(),
        "cleanup owns the engine"
    );

    let mut requests = tokio::task::JoinSet::new();
    let finish = super::finish(
        &service.gate,
        &mut requests,
        &service.operations,
        &service.engine,
    );
    tokio::pin!(finish);
    assert!(pending(finish.as_mut()), "shutdown waits for the cleanup");
    assert!(!hold.stalled());
    hold.open();
    finish.await.unwrap();

    assert_eq!(service.operations.registered(), 0);
    assert!(!service.engine.lock().await.held_input());
    let up = posts(&seen).pop().unwrap();
    assert_eq!(up.kind, objc2_core_graphics::CGEventType::LeftMouseUp.0);
}

#[test]
fn input_that_comes_while_quiet_work_runs_waits_for_it_and_is_then_posted() {
    let (notice, contended) = std::sync::mpsc::channel();
    // The batch takes the activity lock on a blocking thread of this runtime.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .on_thread_start(move || atlas_core::files::on_next_lock_contention(notice.clone()))
        .build()
        .unwrap();
    let owner = "a".repeat(48);
    let directory = tempfile::tempdir().unwrap();
    let (tap, seen) = Tap::new();
    let service = tapped_service(directory.path(), tap, Arc::default());
    service
        .gate
        .acquire_lasting(&owner, super::Who::Agent, LASTING)
        .unwrap();
    let (started, working) = std::sync::mpsc::channel();
    let (finish, finished) = std::sync::mpsc::channel::<()>();
    let work = std::thread::spawn({
        let (activity, seen) = (service.activity.clone(), seen.clone());
        move || {
            crate::native::activity::quiet(&activity, Duration::ZERO, || {
                started.send(()).unwrap();
                finished.recv().unwrap();
                posts(&seen).is_empty()
            })
        }
    });
    working.recv().unwrap();
    let (phase, _) = super::watch::channel(super::ConnectionPhase::Refusal);
    let actions = vec![serde_json::from_value(json!({"type": "move", "x": 10, "y": 20})).unwrap()];
    let (reply, waited) = runtime.block_on(async {
        tokio::join!(
            service.input(
                owner,
                Ok(display_epoch()),
                Ok(actions),
                Ok(None),
                false,
                &phase,
            ),
            async {
                let waited = tokio::task::spawn_blocking(move || {
                    contended.recv_timeout(Duration::from_secs(5)).is_ok()
                })
                .await
                .unwrap();
                finish.send(()).unwrap();
                waited
            },
        )
    });
    let reply = reply.unwrap();
    assert!(waited, "the input waited for the work");
    assert_eq!(
        work.join().unwrap(),
        Some(true),
        "nothing posted under the work"
    );
    assert_eq!(reply["ok"], true, "the input waited rather than failed");
    assert_eq!(reply["performed"], 1);
    assert!(!posts(&seen).is_empty());
    assert_eq!(
        crate::native::activity::quiet(&service.activity, Duration::from_secs(2), || ()),
        None,
        "the input was told to the helper's other processes"
    );
}

#[tokio::test]
async fn input_held_between_requests_keeps_quiet_work_out_until_it_is_let_go() {
    let owner = "a".repeat(48);
    let directory = tempfile::tempdir().unwrap();
    let (tap, _seen) = Tap::new();
    let service = tapped_service(directory.path(), tap, Arc::default());
    service
        .gate
        .acquire_lasting(&owner, super::Who::Agent, LASTING)
        .unwrap();
    let (phase, _) = super::watch::channel(super::ConnectionPhase::Refusal);
    let down = vec![
        serde_json::from_value(json!({"type": "pointer", "phase": "down", "x": 50, "y": 60}))
            .unwrap(),
    ];
    let reply = service
        .input(
            owner.clone(),
            Ok(display_epoch()),
            Ok(down),
            Ok(None),
            false,
            &phase,
        )
        .await
        .unwrap();
    assert_eq!(reply["ok"], true);
    let quiet = || crate::native::activity::quiet(&service.activity, Duration::ZERO, || ());
    assert_eq!(quiet(), None, "the button is still held");
    service.release(owner, &phase).await.unwrap();
    assert_eq!(quiet(), Some(()), "let go");
}

#[test]
fn a_pages_host_is_read_from_its_address_and_nothing_else() {
    assert_eq!(
        page_host("https://GitHub.com/login?x=1#y").as_deref(),
        Some("github.com")
    );
    assert_eq!(
        page_host("http://user:pw@example.org:8080/a").as_deref(),
        Some("example.org")
    );
    assert_eq!(page_host("http://[::1]:3000/").as_deref(), Some("::1"));
    assert_eq!(
        page_host("http://localhost:3000").as_deref(),
        Some("localhost")
    );
    assert_eq!(page_host("file:///Users/someone/a.html"), None);
    assert_eq!(page_host("about:blank"), None);
}

#[tokio::test]
async fn an_agents_typing_is_never_looked_at_for_a_password() {
    let directory = tempfile::tempdir().unwrap();
    let service = service(&directory.path().join("stopped"));
    let owner = atlas_core::files::token();
    let _grant = service.gate.acquire(&owner, super::Who::Agent).unwrap();
    service.note_password(&owner).await;
    assert!(service.seen.lock().unwrap().is_none());
}

#[test]
fn what_the_person_typed_a_password_into_is_given_back_once_to_their_lease_only() {
    let directory = tempfile::tempdir().unwrap();
    let service = service(&directory.path().join("stopped"));
    let seen = serde_json::json!({"app": "com.google.Chrome", "host": "github.com"});
    *service.seen.lock().unwrap() = Some(("person-lease".to_owned(), seen.clone()));

    assert_eq!(service.take_seen("another-lease"), None);
    assert_eq!(service.take_seen("person-lease"), Some(seen));
    assert_eq!(service.take_seen("person-lease"), None);
}
