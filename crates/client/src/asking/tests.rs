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
    assert_eq!(policy_by(&client, Listed::Accounts(Vec::new())), None);
    let listed = policy_by(&client, Listed::Accounts(vec![github()])).unwrap();
    assert_eq!(listed["accounts"][0]["id"], "github.com");
    assert_eq!(listed["accounts"][0]["level"], "ask");
    assert_eq!(
        policy_by(&client, Listed::Unknown).unwrap()["unknown"],
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
