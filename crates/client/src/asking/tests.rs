use super::*;
use crate::sign_ins::{Entry, How, Kind, Level, Listed};

fn github() -> Entry {
    Entry {
        kind: Kind::Site,
        id: "github.com".to_owned(),
        name: "github.com".to_owned(),
        browser: Some("com.google.Chrome".to_owned()),
        added: 1,
        how: How::ByHand,
        level: Level::Ask,
    }
}

#[test]
fn with_no_account_listed_the_request_carries_nothing_new() {
    let dir = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(dir.path());
    assert_eq!(
        policy_by(&client, Listed::Accounts(Vec::new()), &[]).0,
        None
    );
    let listed = policy_by(&client, Listed::Accounts(vec![github()]), &[])
        .0
        .unwrap();
    assert_eq!(listed["accounts"][0]["id"], "github.com");
    assert_eq!(listed["accounts"][0]["level"], "ask");
    assert_eq!(
        policy_by(&client, Listed::Unknown, &[]).0.unwrap()["unknown"],
        true,
        "never read as none"
    );
}

/// A refusal to ask, as the input service gives it.
fn waiting(batch: &str) -> Error {
    Error::new(
        Code::WaitingForPerson,
        "github.com is signed in; Atlas asked the person",
    )
    .with(
        "ask",
        json!({
            "account": "github.com",
            "accountKey": {"kind": "site", "id": "github.com", "browser": "com.google.Chrome"},
            "what": "click",
            "batch": batch,
            "bound": {"name": "Merge"},
            "bind": {"batch": batch, "bound": {"name": "Merge"}},
            "once": true,
        }),
    )
}

/// Answer the one ask up in `client`'s store with `answer`, once one is up.
fn answer_soon(client: &Client, answer: Answer) -> std::thread::JoinHandle<()> {
    let asks = client.controller.asks();
    std::thread::spawn(move || {
        for _ in 0..200 {
            if let Some(ask) = asks.open(now_ms()).into_iter().next() {
                asks.answer(&ask.id, answer, true, now_ms()).unwrap();
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("no ask went up");
    })
}

#[tokio::test]
async fn just_this_once_runs_the_request_again_once_with_its_grant_and_the_ordinary_reply() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(dir.path());
    client.controller = client
        .controller
        .clone()
        .waiting(std::time::Duration::from_secs(5));
    let answering = answer_soon(&client, Answer::Once);
    let mut attempts = Vec::new();
    let reply = after_asking(&client, async || {
        attempts.push(client.granted());
        if attempts.len() == 1 {
            Err(waiting("b1"))
        } else {
            Ok(json!({"ok": true, "performed": 1}))
        }
    })
    .await
    .unwrap();
    answering.join().unwrap();
    assert_eq!(
        reply,
        json!({"ok": true, "performed": 1}),
        "the ordinary reply, byte for byte"
    );
    assert_eq!(attempts.len(), 2);
    assert!(attempts[0].is_empty());
    assert_eq!(attempts[1][0]["scope"], "once");
    assert_eq!(attempts[1][0]["batch"], "b1");
    assert_eq!(
        attempts[1][0]["target"]["name"], "Merge",
        "bound where it was asked about"
    );
    assert_eq!(attempts[1][0]["account"]["id"], "github.com");
    assert!(
        client.granted().is_empty(),
        "a once grant is spent by the attempt it was for"
    );
    assert!(
        client.controller.asks().open(now_ms()).is_empty(),
        "the question came down"
    );
}

#[tokio::test]
async fn deny_is_a_clear_no_and_no_answer_in_time_leaves_the_question_up() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(dir.path());
    client.controller = client
        .controller
        .clone()
        .waiting(std::time::Duration::from_secs(5));
    let answering = answer_soon(&client, Answer::Deny);
    let denied = after_asking(&client, async || Err(waiting("b2")))
        .await
        .unwrap_err();
    answering.join().unwrap();
    assert_eq!(denied.code, Code::Denied);

    client.controller = client
        .controller
        .clone()
        .waiting(std::time::Duration::from_millis(100));
    let unanswered = after_asking(&client, async || Err(waiting("b3")))
        .await
        .unwrap_err();
    assert_eq!(unanswered.code, Code::WaitingForPerson);
    assert_eq!(
        client.controller.asks().open(now_ms()).len(),
        1,
        "it stays up for a rerun"
    );
    // A rerun joins it rather than asking again.
    let unanswered = after_asking(&client, async || Err(waiting("b3")))
        .await
        .unwrap_err();
    assert_eq!(unanswered.code, Code::WaitingForPerson);
    assert_eq!(client.controller.asks().open(now_ms()).len(), 1);
}

#[tokio::test]
async fn a_refusal_that_may_have_sent_something_is_never_asked_about_or_run_again() {
    let dir = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(dir.path());
    let mut attempts = 0;
    let partial = after_asking(&client, async || {
        attempts += 1;
        Err(waiting("b4").uncertain())
    })
    .await
    .unwrap_err();
    assert!(partial.uncertain);
    assert_eq!(attempts, 1);
    assert!(client.controller.asks().open(now_ms()).is_empty());
}

#[tokio::test]
async fn the_person_can_take_control_while_an_agent_waits_and_an_allow_later_runs_with_a_fresh_look()
 {
    let dir = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(dir.path());
    client.controller = client
        .controller
        .clone()
        .waiting(std::time::Duration::from_secs(5));
    let asks = client.controller.asks();
    let controller = client.controller.clone();
    let person = std::thread::spawn(move || {
        let ask = loop {
            if let Some(ask) = asks.open(now_ms()).into_iter().next() {
                break ask;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        // Nothing of the agent's command holds control while it waits.
        let taken = controller.acquire(false).map(drop).is_ok();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let answered = Instant::now();
        asks.answer(&ask.id, Answer::Task, true, now_ms()).unwrap();
        (taken, answered)
    });
    let mut looked = Vec::new();
    let reply = after_asking(&client, async || {
        // Each attempt takes control and looks at its target itself.
        let _ownership = client.controller.acquire(false)?;
        looked.push(Instant::now());
        if looked.len() == 1 {
            Err(waiting("b5"))
        } else {
            Ok(json!({"ok": true}))
        }
    })
    .await
    .unwrap();
    let (taken, answered) = person.join().unwrap();
    assert!(taken, "the person took control during the wait");
    assert_eq!(reply, json!({"ok": true}));
    assert!(
        looked[1] >= answered,
        "the allowed attempt looked again after the answer, however late it came"
    );
}

#[tokio::test]
async fn just_this_once_for_typing_is_bound_to_its_field_and_all_of_its_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(dir.path());
    client.controller = client
        .controller
        .clone()
        .waiting(std::time::Duration::from_secs(5));
    client.set_typing(Some("whole text".to_owned()));
    let policy = policy_by(&client, Listed::Accounts(vec![github()]), &[])
        .0
        .unwrap();
    assert_eq!(policy["typing"], "whole text", "each request carries it");

    let answering = answer_soon(&client, Answer::Once);
    let mut attempts = Vec::new();
    after_asking(&client, async || {
        attempts.push(client.granted());
        if attempts.len() == 1 {
            let mut error = waiting("b6");
            error.details["ask"]["dry"] = json!(true);
            error.details["ask"]["typing"] = json!("whole text");
            Err(error)
        } else {
            Ok(json!({"ok": true}))
        }
    })
    .await
    .unwrap();
    answering.join().unwrap();
    let grant = &attempts[1][0];
    assert_eq!(grant["scope"], "command");
    assert_eq!(
        grant["target"]["name"], "Merge",
        "the field it was asked about"
    );
    assert_eq!(grant["typing"], "whole text", "and all of its text");
    assert!(
        client.granted().is_empty(),
        "spent by that one command's attempt"
    );
}

#[tokio::test]
async fn one_just_this_once_runs_one_of_two_commands_waiting_on_the_same_question() {
    let dir = tempfile::tempdir().unwrap();
    let waiting_for = std::time::Duration::from_millis(1500);
    let mut first = crate::test_support::client(dir.path());
    first.controller = first.controller.clone().waiting(waiting_for);
    let mut second = crate::test_support::client(dir.path());
    second.controller = second.controller.clone().waiting(waiting_for);
    let asks = first.controller.asks();
    let joined = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let person = {
        let joined = joined.clone();
        std::thread::spawn(move || {
            // Both commands are refused and wait on the one question first.
            while joined.load(std::sync::atomic::Ordering::SeqCst) < 2 {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
            let ask = asks.open(now_ms()).into_iter().next().unwrap();
            asks.answer(&ask.id, Answer::Once, true, now_ms()).unwrap();
        })
    };
    let ran = std::sync::atomic::AtomicUsize::new(0);
    let command = async |client: &Client| {
        after_asking(client, async || {
            if client.granted().is_empty() {
                joined.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err(waiting("b7"))
            } else {
                ran.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(json!({"ok": true}))
            }
        })
        .await
    };
    let (one, two) = tokio::join!(command(&first), command(&second));
    person.join().unwrap();
    assert_eq!(
        ran.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "exactly one ran"
    );
    assert_eq!(
        [one.is_ok(), two.is_ok()].iter().filter(|ok| **ok).count(),
        1,
        "the other asks again and is still waiting"
    );
}

#[test]
fn allow_for_this_task_holds_only_in_the_host_process_it_was_given_in_while_its_session_lasts() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(dir.path());
    let running = atlas_core::process::Instance::current().unwrap();
    let session = atlas_core::host::HostSession::new(atlas_core::host::Host::Codex, "one")
        .unwrap()
        .run_by(Some(running));
    client.host_session = Some(session.clone());
    let asks = client.controller.asks();
    let ask = Ask {
        id: String::new(),
        session: session.key().to_string(),
        process: Some(running),
        who: "Codex".to_owned(),
        task: None,
        account: "github.com".to_owned(),
        question: json!({"accountKey": {"kind": "site", "id": "github.com"}, "bind": {"b": 1}}),
        asked: 0,
        ends: 0,
    };
    let id = asks.ask(ask, now_ms()).unwrap();
    asks.answer(&id, Answer::Task, true, now_ms()).unwrap();
    let tasks = |client: &Client| {
        policy_by(client, Listed::Accounts(vec![github()]), &[])
            .0
            .unwrap()["grants"]
            .as_array()
            .unwrap()
            .len()
    };
    assert_eq!(tasks(&client), 1, "the process it was given in");

    let mut resumed = crate::test_support::client(dir.path());
    resumed.host_session = Some(
        session
            .clone()
            .run_by(Some(atlas_core::process::Instance::new(
                running.pid,
                [1, 0],
            ))),
    );
    assert_eq!(
        tasks(&resumed),
        0,
        "the same session key in another process"
    );

    let now = now_ms();
    crate::host_session::store::Store::at(client.output_dir().unwrap())
        .update(&session, now, &client.controller, |record| {
            record.ended = Some(now);
        })
        .unwrap();
    assert_eq!(tasks(&client), 0, "its session ended");
}

#[tokio::test]
async fn just_this_once_answers_gathered_in_one_command_all_hold_until_it_ends() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(dir.path());
    client.controller = client
        .controller
        .clone()
        .waiting(std::time::Duration::from_secs(5));
    let asks = client.controller.asks();
    let person = std::thread::spawn(move || {
        let mut answered = Vec::new();
        while answered.len() < 2 {
            if let Some(ask) = asks
                .open(now_ms())
                .into_iter()
                .find(|ask| !answered.contains(&ask.id))
            {
                asks.answer(&ask.id, Answer::Once, true, now_ms()).unwrap();
                answered.push(ask.id);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    });
    let mut attempts = Vec::new();
    // One request with two places that send: each asks in turn, sending nothing.
    let reply = after_asking(&client, async || {
        attempts.push(client.granted());
        match attempts.len() {
            1 => Err(waiting("c1")),
            2 => Err(waiting("c2")),
            _ => Ok(json!({"ok": true})),
        }
    })
    .await
    .unwrap();
    person.join().unwrap();
    assert_eq!(reply, json!({"ok": true}));
    assert_eq!(attempts[1].len(), 1);
    assert_eq!(
        attempts[2].len(),
        2,
        "the first approval held for the second"
    );
    assert!(client.granted().is_empty(), "the command's end spends them");
}

/// A refusal to ask about sending `preview`, as the input service gives it.
fn waiting_to_send(batch: &str, preview: &str) -> Error {
    let mut error = waiting(batch);
    error.details["ask"]["message"] = json!({"preview": preview, "digest": "d", "length": 7});
    error
}

/// The previews on disk among the asks still up in `client`'s store.
fn previews(client: &Client) -> Vec<Value> {
    client
        .controller
        .asks()
        .open(now_ms())
        .into_iter()
        .map(|ask| ask.question["message"]["preview"].clone())
        .collect()
}

#[tokio::test]
async fn a_message_preview_is_gone_from_disk_once_its_wait_ends_however_it_ends() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client(dir.path());
    client.controller = client
        .controller
        .clone()
        .waiting(std::time::Duration::from_millis(100));

    // Out of time: the question stays up for a rerun, its preview doesn't.
    let unanswered = after_asking(&client, async || Err(waiting_to_send("p1", "canary one")))
        .await
        .unwrap_err();
    assert_eq!(unanswered.code, Code::WaitingForPerson);
    assert_eq!(previews(&client), [Value::Null]);

    // Stopped.
    client.controller.note_stop(true).unwrap();
    let stopped = after_asking(&client, async || Err(waiting_to_send("p2", "canary two")))
        .await
        .unwrap_err();
    assert_eq!(stopped.code, Code::Stopped);
    client.controller.note_stop(false).unwrap();
    assert!(previews(&client).iter().all(Value::is_null));

    // Interrupted: the command is dropped while it waits.
    client.controller = client
        .controller
        .clone()
        .waiting(std::time::Duration::from_secs(1));
    let interrupted = tokio::time::timeout(
        std::time::Duration::from_millis(200),
        after_asking(&client, async || Err(waiting_to_send("p3", "canary three"))),
    )
    .await;
    assert!(interrupted.is_err(), "still waiting when it was dropped");
    assert!(previews(&client).iter().all(Value::is_null));

    // Answered: the question comes down with it.
    let asks = client.controller.asks();
    for ask in asks.open(now_ms()) {
        asks.close(&ask.id);
    }
    let answering = answer_soon(&client, Answer::Deny);
    let denied = after_asking(&client, async || Err(waiting_to_send("p4", "canary four")))
        .await
        .unwrap_err();
    assert_eq!(denied.code, Code::Denied);
    answering.join().unwrap();
    let left = client.controller.asks().open(now_ms());
    assert!(
        !serde_json::to_string(&left).unwrap().contains("canary"),
        "{left:?}"
    );
}

#[tokio::test]
async fn a_typed_step_the_person_allowed_after_a_stop_is_typed_once_without_asking_again() {
    use atlas_core::session::{ASKS, FOCUS_APP, TYPED_FIELDS};
    let dir = tempfile::tempdir().unwrap();
    let (mut client, helper) =
        crate::test_support::Helper::client(dir.path(), atlas_core::test_support::UID);
    client.listed = Some(Listed::Accounts(vec![github()]));
    let helper = helper.serving_input(&client, &[FOCUS_APP, TYPED_FIELDS, ASKS]);
    // The person allowed, just this once, the line break that stopped a request.
    let stopped = atlas_core::actions::Action::Type {
        text: "\n".to_owned(),
    };
    let asks = client.controller.asks();
    asks.permit(
        Permit {
            session: "terminal".to_owned(),
            process: None,
            account: json!({"kind": "site", "id": "github.com", "browser": "com.google.Chrome"}),
            action: stopped.digest(),
            target: json!({"window": 7}),
            value: None,
            ends: u64::MAX,
        },
        now_ms(),
    )
    .unwrap();

    let mut inputs = Vec::new();
    let typed = helper
        .serve(
            crate::agent::type_text(&client, "\n", None),
            |request| match request["op"].as_str() {
                Some("input") => {
                    inputs.push(request["asking"].clone());
                    if request["asking"]["dry"] == true {
                        Some(json!({"ok": true, "performed": 0, "dry": true}))
                    } else {
                        Some(json!({"ok": true, "performed": 1}))
                    }
                }
                _ => Some(json!({"ok": true})),
            },
        )
        .await
        .unwrap();
    assert_eq!(typed["ok"], true, "{typed}");
    assert_eq!(inputs.len(), 2, "the check, then the text");
    for asking in &inputs {
        assert!(
            asking["grants"]
                .as_array()
                .unwrap()
                .iter()
                .any(|grant| grant["scope"] == "permit"),
            "the permit goes with the check and with the text: {asking}"
        );
    }
    assert!(
        asks.take_permits("terminal", None, &[stopped.digest()], now_ms())
            .unwrap()
            .is_empty(),
        "spent by the text it covered"
    );
}
