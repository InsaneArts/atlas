use super::*;
use crate::{
    activity_log,
    doing::Doing,
    host_session::{Host, HostSession},
    test_support::{Helper, Pictures},
};
use atlas_core::{
    session::{FOCUS_APP, TYPED_FIELDS},
    test_support::UID,
};
use serde_json::Value;
use std::{fs, path::Path};

/// An agent's client of a stand-in helper with an input service, keeping
/// pictures before actions, its command doing `verb`.
fn agent(directory: &Path, pictures: Pictures, verb: &'static str) -> (Client, Helper) {
    let (client, helper) = Helper::client(directory, UID);
    let mut client = client.doing(Doing::new(verb));
    client.config.activity.pictures_before_actions = true;
    let helper = helper
        .serving_input(&client, &[FOCUS_APP, TYPED_FIELDS])
        .taking_pictures(pictures);
    (client, helper)
}

/// The input service's answer to `request`. An input waits, when `saved`,
/// until its command's picture is saved, so a test can find its file.
fn input_service(client: &Client, saved: bool, request: &Value) -> Option<Value> {
    match request["op"].as_str() {
        Some("acquire" | "release") => Some(json!({"ok": true})),
        Some("input") => {
            let slot = client.doing.as_ref().unwrap().before();
            let until = Instant::now() + Duration::from_secs(5);
            let output = client.config.output_dir.clone();
            let saved_yet = || {
                slot.name()
                    .is_some_and(|name| before::Listing::of(&output).has(&name))
            };
            while saved && !saved_yet() && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(1));
            }
            let performed = request["actions"].as_array().unwrap().len();
            Some(json!({"ok": true, "performed": performed}))
        }
        other => panic!("nothing else is asked: {other:?}"),
    }
}

/// The rows `client` wrote, oldest first.
fn rows(client: &Client) -> Vec<Value> {
    let log = client.output_dir().unwrap().join(activity_log::FILE);
    fs::read_to_string(log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn pictures(client: &Client) -> Vec<String> {
    let folder = client.output_dir().unwrap().join(before::FOLDER);
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    entries
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| before::time(name).is_some())
        .collect()
}

fn key() -> Value {
    json!([{"type": "key", "key": "a"}])
}

#[tokio::test]
async fn an_agents_input_names_the_picture_taken_before_it_was_sent() {
    let directory = tempfile::tempdir().unwrap();
    let (mut client, helper) = agent(directory.path(), Pictures::Taken, "key");
    client.host_session = Some(HostSession::new(Host::Codex, "one").unwrap());
    let key_of = client.host_session.as_ref().unwrap().key().to_string();
    let reply = helper
        .serve(client.input(&key(), None, None), |request| {
            input_service(&client, true, request)
        })
        .await
        .unwrap();

    assert_eq!(reply["ok"], true);
    assert!(reply.get("before").is_none(), "the agent is never given it");
    let log = helper.log();
    let sent = log.iter().position(|op| op == "frame sent").unwrap();
    let input = log.iter().position(|op| op == "input").unwrap();
    assert!(sent < input, "the picture came before the input: {log:?}");
    let [picture] = <[String; 1]>::try_from(pictures(&client)).unwrap();
    assert_eq!(rows(&client)[0]["before"], picture);
    assert!(
        picture.contains(&key_of),
        "its name says whose action it was"
    );
    let facts = before::facts(&client.config.output_dir, &picture).unwrap();
    assert_eq!(
        (
            &facts["action"],
            &facts["outcome"],
            &facts["display"]["width"]
        ),
        (&json!("key"), &json!("worked"), &json!(100.0)),
        "{facts}"
    );
}

/// The facts kept beside the one picture `client` took.
fn only_facts(client: &Client) -> Value {
    let [picture] = <[String; 1]>::try_from(pictures(client)).unwrap();
    before::facts(&client.config.output_dir, &picture).unwrap()
}

#[tokio::test]
async fn a_named_click_marks_its_element_window_and_task_and_its_reply_stays_as_it_was() {
    use crate::{
        agent::{Target, click_element},
        task::Task,
        test_support::completed,
    };
    use atlas_core::{
        actions::Button,
        screen::select::ElementFilter,
        session::{FOCUS_APP, FOCUS_DOCK, FOCUS_WINDOW},
    };

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let mut client = client.doing(Doing::new("click"));
    client.config.activity.pictures_before_actions = true;
    let session = HostSession::new(Host::Codex, "one").unwrap();
    client.task = Some(Task::of(session.key(), Some("probe")).unwrap());
    client.host_session = Some(session);
    let helper = helper
        .serving_input(&client, &[FOCUS_APP, FOCUS_DOCK, FOCUS_WINDOW])
        .taking_pictures(Pictures::Taken);
    let pid = i64::from(std::process::id());
    let filter = ElementFilter {
        name: Some("Save".into()),
        ..ElementFilter::default()
    };
    let reply = helper
        .serve(
            click_element(&client, &Target::Pid(pid), &filter, Button::Left, 1, &[]),
            |request| match request["op"].as_str() {
                Some("apps") => Some(json!({"ok": true, "apps": [{"pid": pid, "name": "Notes"}],
                    "starts": {pid.to_string(): [123, 456]}})),
                Some("windows") => Some(json!({"ok": true,
                    "starts": {pid.to_string(): [123, 456]}, "windows": [{"number": 42,
                    "pid": pid, "owner": "Notes", "title": "Draft", "layer": 0, "alpha": 1.0,
                    "bounds": {"x": 10, "y": 20, "width": 80, "height": 60}}]})),
                Some("run") => Some(completed(
                    &json!({"ok": true, "truncated": false, "elements": [{"index": 0,
                        "depth": 0, "role": "AXButton", "title": "Save", "enabled": true,
                        "frame": {"x": 20, "y": 30, "width": 20, "height": 20},
                        "window": {"x": 10, "y": 20, "width": 80, "height": 60}}]})
                    .to_string(),
                )),
                Some("input") => input_service(&client, true, request).map(|mut reply| {
                    reply["expect"] = request["expect"].clone();
                    reply
                }),
                _ => input_service(&client, false, request),
            },
        )
        .await
        .unwrap();

    assert_eq!(
        reply["element"],
        json!({"role": "AXButton", "name": "Save", "at": [30, 40]})
    );
    let facts = only_facts(&client);
    assert_eq!(
        (&facts["element"], &facts["task"]),
        (
            &json!({"frame": {"x": 20.0, "y": 30.0, "width": 20.0, "height": 20.0},
                "role": "AXButton", "name": "Save"}),
            &json!("probe")
        ),
        "{facts}"
    );
    assert!(
        !facts.to_string().contains("Draft"),
        "a window's title may name a document, so it is never kept: {facts}"
    );
}

#[tokio::test]
async fn a_referenced_click_or_press_marks_the_element_the_input_service_reached() {
    use atlas_core::session::{ELEMENT_ACTIONS, FOCUS_APP, LOOKS};

    let reached = json!({"frame": {"x": 20, "y": 30, "width": 20, "height": 20},
        "role": "AXButton", "name": "Save"});
    for press in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let mut client = client.doing(Doing::new(if press { "press" } else { "click" }));
        client.config.activity.pictures_before_actions = true;
        let helper = helper
            .serving_input(&client, &[FOCUS_APP, LOOKS, ELEMENT_ACTIONS])
            .taking_pictures(Pictures::Taken);
        let answer = |request: &Value| {
            let mut reply = input_service(&client, true, request)?;
            if request["op"] != "input" {
                return Some(reply);
            }
            reply["element"] = reached.clone();
            if press {
                reply["mechanism"] = json!("accessibility");
                reply["verification"] = json!("matched");
            }
            Some(reply)
        };
        let reply = if press {
            let action = json!({"type": "press", "ref": "8123abc.12"});
            helper
                .serve(crate::agent::element(&client, None, &action), answer)
                .await
        } else {
            helper
                .serve(
                    crate::agent::click_reference(
                        &client,
                        None,
                        "8123abc.12",
                        atlas_core::actions::Button::Left,
                        1,
                        &[],
                    ),
                    answer,
                )
                .await
        }
        .unwrap();
        let mut reply = reply;
        reply.as_object_mut().unwrap().remove("elapsedMs");

        let said = if press {
            json!({"ok": true, "performed": 1, "mechanism": "accessibility",
                "verification": "matched"})
        } else {
            json!({"ok": true, "performed": 1})
        };
        assert_eq!(reply, said, "the agent is never told: {press}");
        let facts = only_facts(&client);
        assert_eq!(
            facts["element"],
            json!({"frame": {"x": 20.0, "y": 30.0, "width": 20.0, "height": 20.0},
                "role": "AXButton", "name": "Save"}),
            "{facts}"
        );
        assert!(facts.get("window").is_none() && facts.get("task").is_none());
    }
}

#[tokio::test]
async fn a_click_at_a_point_marks_only_its_point() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = agent(directory.path(), Pictures::Taken, "click");
    let click = json!([{"type": "click", "x": 10, "y": 20}]);
    helper
        .serve(client.input(&click, None, None), |request| {
            input_service(&client, true, request)
        })
        .await
        .unwrap();

    let facts = only_facts(&client);
    assert_eq!(facts["point"], json!({"x": 10.0, "y": 20.0}));
    for absent in ["element", "window", "task"] {
        assert!(facts.get(absent).is_none(), "{absent}: {facts}");
    }
}

#[tokio::test]
async fn the_picture_after_an_action_is_taken_only_when_no_next_step_serves() {
    for next_within in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = agent(directory.path(), Pictures::Taken, "key");
        let output = client.config.output_dir.clone();
        let at = atlas_core::time::timestamp() - 5_000_000_000;
        let first = before::name(at, None, None);
        let ended = u64::try_from(at / 1_000_000).unwrap() + 100;
        std::os::unix::fs::DirBuilderExt::mode(&mut std::fs::DirBuilder::new(), 0o700)
            .recursive(true)
            .create(output.join(before::FOLDER))
            .unwrap();
        if next_within {
            let next = before::name(at + 500_000_000, None, None);
            fs::write(output.join(before::FOLDER).join(next), b"jpeg").unwrap();
        }
        helper
            .serve(client.picture_after(&first, ended), |request| {
                panic!("nothing else is asked: {request}")
            })
            .await
            .unwrap();

        let after = before::after_name(&first).unwrap();
        let listing = before::Listing::of(&output);
        assert_eq!(
            listing.has(&after),
            !next_within,
            "next within: {next_within}"
        );
        assert_eq!(
            helper.log().contains(&"frame sent".to_owned()),
            !next_within
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_picture_after_the_helper_never_answers_gives_up_in_time() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = agent(directory.path(), Pictures::Late, "key");
    let at = atlas_core::time::timestamp() - 5_000_000_000;
    let first = before::name(at, None, None);
    let ended = u64::try_from(at / 1_000_000).unwrap() + 100;

    let error = helper
        .serve(client.picture_after(&first, ended), |request| {
            panic!("nothing else is asked: {request}")
        })
        .await
        .unwrap_err();

    // Its process ends rather than waiting on with the next action's.
    assert_eq!(error.code, Code::Capture, "{error:?}");
    let after = before::after_name(&first).unwrap();
    assert!(!before::Listing::of(&client.config.output_dir).has(&after));
}

#[tokio::test]
async fn the_persons_input_and_a_setting_turned_off_take_no_picture() {
    for person in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let (mut client, helper) = agent(directory.path(), Pictures::Taken, "key");
        if person {
            client.actor = Actor::Viewer;
        } else {
            client.config.activity.pictures_before_actions = false;
        }
        helper
            .serve(client.input(&key(), None, None), |request| {
                input_service(&client, false, request)
            })
            .await
            .unwrap();

        assert!(!helper.log().contains(&"frame".to_owned()), "{person}");
        assert!(pictures(&client).is_empty());
        assert!(rows(&client).iter().all(|row| row.get("before").is_none()));
    }
}

#[tokio::test]
async fn typing_in_several_batches_takes_one_picture_named_on_its_command() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = agent(directory.path(), Pictures::Taken, "type");
    // Three batches of at most 64 characters.
    let text = "a".repeat(129);
    let report = helper
        .serve(crate::agent::type_text(&client, &text, None), |request| {
            input_service(&client, true, request)
        })
        .await
        .unwrap();

    assert_eq!(report["batches"], 3);
    let log = helper.log();
    assert_eq!(log.iter().filter(|op| *op == "frame").count(), 1, "{log:?}");
    assert_eq!(log.iter().filter(|op| *op == "input").count(), 3);
    let [picture] = <[String; 1]>::try_from(pictures(&client)).unwrap();
    let last = rows(&client).pop().unwrap();
    assert_eq!(last["before"], picture, "the command's row names it");
}

#[tokio::test]
async fn text_typed_in_batches_is_kept_only_when_every_batch_went_into_a_known_field() {
    // Each batch's field, or how its reply went instead.
    let field = json!({"role": "AXTextField"});
    let password = json!({"role": "AXSecureTextField"});
    let (lost, failed) = (json!("lost"), json!("failed"));
    for (into, kept, secure) in [
        ([field.clone(), field.clone(), field.clone()], true, false),
        ([field.clone(), Value::Null, field.clone()], false, false),
        ([field.clone(), password, field.clone()], false, true),
        ([field.clone(), lost, field.clone()], false, false),
        ([field.clone(), failed, field.clone()], false, false),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = agent(directory.path(), Pictures::Taken, "type");
        let text = "a".repeat(129);
        let mut batch = 0;
        helper
            .serve(crate::agent::type_text(&client, &text, None), |request| {
                let mut reply = input_service(&client, true, request)?;
                if request["op"] == "input" {
                    let said = into[batch].clone();
                    batch += 1;
                    if said == "lost" {
                        return None;
                    }
                    if said == "failed" {
                        return Some(json!({"ok": false, "code": "input",
                            "error": "The batch failed", "performed": 1, "partial": true,
                            "uncertain": false}));
                    }
                    reply["field"] = said;
                }
                Some(reply)
            })
            .await
            .unwrap();

        let facts = only_facts(&client);
        assert_eq!(facts.get("text").is_some(), kept, "{into:?}: {facts}");
        assert_eq!(facts.get("length").is_some(), kept, "{into:?}");
        assert_eq!(facts.get("secure").is_some(), secure, "{into:?}");
        if kept {
            assert_eq!(facts["length"], 129);
        }
    }
}

#[tokio::test]
async fn no_text_is_kept_through_an_input_service_that_cannot_tell_a_password_field() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let mut client = client.doing(Doing::new("type"));
    client.config.activity.pictures_before_actions = true;
    // An older service names a password field by its role alone.
    let helper = helper
        .serving_input(&client, &[FOCUS_APP])
        .taking_pictures(Pictures::Taken);
    helper
        .serve(
            crate::agent::type_text(&client, "secret", None),
            |request| {
                let mut reply = input_service(&client, true, request)?;
                if request["op"] == "input" {
                    reply["field"] = json!({"role": "AXTextField"});
                }
                Some(reply)
            },
        )
        .await
        .unwrap();

    let facts = only_facts(&client);
    assert_eq!(facts["action"], "type");
    for absent in ["text", "length"] {
        assert!(facts.get(absent).is_none(), "{absent}: {facts}");
    }
}

#[tokio::test]
async fn a_refused_or_late_picture_never_holds_the_input_back() {
    for pictures_answer in [Pictures::Refused, Pictures::Late, Pictures::Cancelled] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = agent(directory.path(), pictures_answer, "key");
        let reply = helper
            .serve(client.input(&key(), None, None), |request| {
                input_service(&client, false, request)
            })
            .await
            .unwrap();

        assert_eq!(reply["ok"], true, "{pictures_answer:?}");
        let log = helper.log();
        assert!(!log.contains(&"frame sent".to_owned()));
        assert!(pictures(&client).is_empty(), "{pictures_answer:?}");
        assert!(rows(&client)[0].get("before").is_none());
    }
}

/// A picture whose cancel is observed, with its answer held by the test.
fn cancellable_picture() -> (Taking, oneshot::Receiver<()>, oneshot::Sender<()>) {
    let directory = tempfile::tempdir().unwrap();
    let admitted = crate::test_support::client(directory.path())
        .controller
        .screen_visible()
        .unwrap();
    let (cancel, cancelled) = oneshot::channel();
    let (noticed, notice) = oneshot::channel();
    let (answer, answered) = oneshot::channel();
    let task = tokio::spawn(async move {
        cancelled.await.unwrap();
        noticed.send(()).unwrap();
        answered.await.unwrap();
        Err(Error::new(Code::Capture, "The picture was cancelled"))
    });
    (
        Taking {
            task,
            deadline: (tokio::time::Instant::now() + WAIT).into_std(),
            cancel: Some(cancel),
            admitted,
        },
        notice,
        answer,
    )
}

#[tokio::test(start_paused = true)]
async fn a_picture_waits_for_the_cancel_answer_before_its_deadline() {
    let (taking, cancelled, answer) = cancellable_picture();
    let started = tokio::time::Instant::now();
    let waiting = tokio::spawn(taking.taken());
    cancelled.await.unwrap();
    assert_eq!(started.elapsed(), WAIT - CANCEL);
    assert!(!waiting.is_finished());
    answer.send(()).unwrap();
    assert!(waiting.await.unwrap().is_none());
    assert_eq!(started.elapsed(), WAIT - CANCEL);
}

#[tokio::test(start_paused = true)]
async fn a_missing_picture_cancel_answer_stops_delaying_input_at_the_deadline() {
    let (taking, cancelled, _answer) = cancellable_picture();
    let started = tokio::time::Instant::now();
    let waiting = tokio::spawn(taking.taken());
    cancelled.await.unwrap();
    tokio::time::advance(CANCEL - Duration::from_millis(1)).await;
    tokio::task::yield_now().await;
    assert!(!waiting.is_finished());
    assert!(waiting.await.unwrap().is_none());
    assert_eq!(started.elapsed(), WAIT);
}

#[tokio::test]
async fn a_frame_sends_its_cancel_and_reads_the_daemons_answer() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = agent(directory.path(), Pictures::Cancelled, "key");
    let (cancel, cancelled) = oneshot::channel();
    cancel.send(()).unwrap();
    let picture = helper
        .serve(
            frame(
                client.transport.clone(),
                epoch(),
                std::process::id(),
                cancelled,
            ),
            |request| panic!("nothing else is asked: {request}"),
        )
        .await;
    assert!(matches!(picture, Err(error) if error.code == Code::Cancelled));
    assert!(helper.log().contains(&"frame cancelled".to_owned()));
}

#[tokio::test]
async fn a_frame_whose_pixels_come_only_after_the_input_leaves_no_trace() {
    for pictures_answer in [Pictures::AfterInput, Pictures::PixelsAfterInput] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = agent(directory.path(), pictures_answer, "type");
        let reply = helper
            .serve(crate::agent::type_text(&client, "hello", None), |request| {
                let mut reply = input_service(&client, false, request)?;
                if request["op"] == "input" {
                    reply["field"] = json!({"role": "AXTextField"});
                }
                Some(reply)
            })
            .await
            .unwrap();
        // Long enough for a writer that should not exist to have written.
        tokio::time::sleep(Duration::from_millis(200)).await;

        assert_eq!(reply["ok"], true, "{pictures_answer:?}");
        let log = helper.log();
        let input = log.iter().position(|op| op == "input");
        let released = log.iter().position(|op| op == "frame released");
        assert!(
            input
                .zip(released)
                .is_some_and(|(input, released)| input < released),
            "valid pixels came, only after the input: {log:?}"
        );
        let folder = client.config.output_dir.join(before::FOLDER);
        let kept: Vec<_> = fs::read_dir(folder)
            .map(|entries| entries.map(|entry| entry.unwrap().file_name()).collect())
            .unwrap_or_default();
        assert!(kept.is_empty(), "{pictures_answer:?}: {kept:?}");
        assert!(
            rows(&client).iter().all(|row| row.get("before").is_none()),
            "{pictures_answer:?}"
        );
    }
}

#[tokio::test]
async fn a_picture_comes_only_from_the_verified_daemon() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = agent(directory.path(), Pictures::Taken, "key");
    let taking = client
        .picture_before(&epoch(), std::process::id() + 1)
        .unwrap();
    let picture = helper
        .serve(taking.taken(), |request| {
            panic!("nothing else is asked: {request}")
        })
        .await;
    assert!(picture.is_none());
    assert!(
        helper.log().is_empty(),
        "nothing is asked of another process"
    );
}

#[tokio::test]
async fn a_late_picture_is_discarded_and_asked_for_once_per_command() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = agent(directory.path(), Pictures::Late, "key");
    let taking = client.picture_before(&epoch(), std::process::id()).unwrap();
    assert!(
        client
            .picture_before(&epoch(), std::process::id())
            .is_none(),
        "one for each command"
    );
    let picture = helper
        .serve(taking.taken(), |request| {
            panic!("nothing else is asked: {request}")
        })
        .await;
    assert!(picture.is_none());
}

/// The session a picture is asked of.
fn epoch() -> InputEpoch {
    InputEpoch {
        uid: UID,
        security_session_id: 42,
        display_id: 7,
        width: 100.0,
        height: 100.0,
        process_id: 456,
    }
}

/// The pictures `client` saved, once it saved `count`, or as they are in
/// five seconds.
fn saved(client: &Client, count: usize) -> Vec<String> {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let pictures = pictures(client);
        if pictures.len() >= count || Instant::now() >= until {
            return pictures;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[tokio::test]
async fn a_lost_reply_keeps_its_picture_and_only_a_refusal_proven_to_send_nothing_drops_it() {
    // The reply never comes: the input may have been sent.
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = agent(directory.path(), Pictures::Taken, "key");
    let lost = helper
        .serve(client.input(&key(), None, None), |request| {
            if request["op"] == "input" {
                return None;
            }
            input_service(&client, false, request)
        })
        .await;
    assert!(lost.is_err());
    assert_eq!(saved(&client, 1).len(), 1, "its picture is kept");

    // Refused to ask the person before its first event: nothing was sent.
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = agent(directory.path(), Pictures::Taken, "key");
    let _ = helper
        .serve(client.input(&key(), None, None), |request| {
            if request["op"] == "input" {
                return Some(json!({"ok": false, "code": "waiting-for-person",
                    "error": "Atlas asked the person", "performed": 0}));
            }
            input_service(&client, false, request)
        })
        .await;
    // Nothing was kept to be saved, so none can come later either.
    assert!(
        pictures(&client).is_empty(),
        "no picture for what sent nothing"
    );
}

#[tokio::test]
async fn a_question_to_the_person_has_its_own_picture_and_activity_keeps_none_whatever_the_setting()
{
    // A referenced click is brought forward first: its picture is taken
    // afresh, the one before it never reused.
    let referenced = json!([{"type": "reference", "ref": "8123abc.12"}]);
    for (keeping, actions) in [(true, key()), (false, key()), (true, referenced)] {
        let fresh = !keeping || actions != key();
        let directory = tempfile::tempdir().unwrap();
        let (mut client, helper) = agent(directory.path(), Pictures::Taken, "key");
        client.config.activity.pictures_before_actions = keeping;
        client.controller = client
            .controller
            .clone()
            .waiting(Duration::from_millis(100));
        let frame = json!({"x": 10.0, "y": 20.0, "width": 80.0, "height": 24.0});
        let refused = json!({"ok": false, "code": "waiting-for-person",
            "error": "github.com is signed in", "performed": 0, "partial": false,
            "uncertain": false, "ask": {"account": "github.com", "what": "click",
                "accountKey": {"kind": "site", "id": "github.com"}, "action": "a1",
                "bind": {"action": "a1"}, "once": true, "task": true, "frame": frame}});
        let outcome = helper
            .serve(
                crate::asking::after_asking(&client, async || {
                    client.input(&actions, None, None).await
                }),
                |request| match request["op"].as_str() {
                    Some("input") => Some(refused.clone()),
                    _ => input_service(&client, false, request),
                },
            )
            .await;

        if fresh {
            // Taken only once the input was let go of and control with it,
            // so the person taking over never waits for it.
            let log = helper.log();
            let released = log.iter().rposition(|op| op == "release");
            let taken = log.iter().rposition(|op| op == "frame sent");
            assert!(
                released
                    .zip(taken)
                    .is_some_and(|(released, taken)| released < taken),
                "{log:?}"
            );
        }
        let asks = client.controller.asks();
        let open = asks.open(atlas_core::time::now_ms());
        let picture = open[0].picture.clone().expect("the question has a picture");
        assert!(asks.folder().join(&picture).exists(), "keeping {keeping}");
        // Where the element is, in the picture's own pixels: the display
        // here is 100 points across.
        let size = &open[0].question["pictureSize"];
        let across = size["width"].as_f64().unwrap() / 100.0;
        assert!(across > 0.0, "{size}");
        assert_eq!(
            open[0].question["frame"]["x"].as_f64(),
            Some(10.0 * across),
            "where the element is"
        );
        assert!(
            pictures(&client).is_empty(),
            "activity keeps none: {keeping}"
        );
        let told = match &outcome {
            Ok(reply) => reply.to_string(),
            Err(error) => serde_json::to_string(error).unwrap(),
        };
        for private in ["frame", &picture, ".jpg", "\"x\":10"] {
            assert!(!told.contains(private), "{private} in {told}");
        }
    }
}

/// The person's lease on `client`'s helper, hiding its screen for a
/// sign-in to github.com.
fn signing_in(client: &Client) -> atlas_control::Lease {
    let mut lease = atlas_control::Lease::default();
    let token = lease.acquire(&client.controller).unwrap();
    let hold = atlas_control::PrivateHold {
        site: "github.com".to_owned(),
        kind: atlas_control::HoldKind::SignIn,
    };
    lease
        .hide_screen(&client.controller, &token, &hold)
        .unwrap();
    lease
}

#[tokio::test]
async fn no_picture_after_an_action_is_taken_while_the_person_signs_in() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = agent(directory.path(), Pictures::Taken, "key");
    let at = atlas_core::time::timestamp() - 5_000_000_000;
    let first = before::name(at, None, None);
    let ended = u64::try_from(at / 1_000_000).unwrap() + 100;
    let mut lease = signing_in(&client);

    helper
        .serve(client.picture_after(&first, ended), |request| {
            panic!("nothing else is asked: {request}")
        })
        .await
        .unwrap();

    assert!(!helper.log().iter().any(|op| op.starts_with("frame")));
    let after = before::after_name(&first).unwrap();
    assert!(!before::Listing::of(&client.config.output_dir).has(&after));
    client.controller.end_private().unwrap();
    lease.release();
}

#[tokio::test]
async fn no_picture_for_a_question_is_taken_while_the_person_signs_in() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = agent(directory.path(), Pictures::Taken, "key");
    let mut lease = signing_in(&client);

    let taken = helper
        .serve(client.picture_for_ask(), |request| {
            panic!("nothing else is asked: {request}")
        })
        .await;

    assert!(taken.is_none());
    assert!(!helper.log().iter().any(|op| op.starts_with("frame")));
    client.controller.end_private().unwrap();
    lease.release();
    let (_, admitted) = helper
        .serve(client.picture_for_ask(), |request| {
            panic!("nothing else is asked: {request}")
        })
        .await
        .expect("taken once the sign-in ended");
    assert!(client.controller.still_visible(admitted).is_ok());
}

#[tokio::test]
async fn a_question_never_shows_a_picture_taken_before_a_sign_in_began() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = agent(directory.path(), Pictures::Taken, "key");
    let (_, admitted) = helper
        .serve(client.picture_for_ask(), |request| {
            panic!("nothing else is asked: {request}")
        })
        .await
        .unwrap();

    let mut lease = signing_in(&client);
    client.controller.end_private().unwrap();
    lease.release();

    assert_eq!(
        client.controller.still_visible(admitted).unwrap_err().code,
        Code::Busy,
        "a hold began and ended since it was taken"
    );
}

#[tokio::test]
async fn an_agents_screenshot_is_refused_while_the_person_signs_in() {
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = agent(directory.path(), Pictures::Taken, "screenshot");
    let mut lease = signing_in(&client);

    let refused = helper
        .serve(
            client.capture(crate::client::Size::Full, None, false),
            |request| panic!("nothing else is asked: {request}"),
        )
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::Busy);
    assert_eq!(refused.holder, Some(atlas_core::error::Holder::Person));
    assert!(helper.log().is_empty(), "{:?}", helper.log());
    client.controller.end_private().unwrap();
    lease.release();
}
