use super::*;
use serde_json::json;

fn asks(folder: &Path) -> Asks {
    Asks::at(
        folder.join("h.asks"),
        folder.join("h.allowed.json"),
        folder.join("h.stopped"),
    )
}

fn question(session: &str, batch: &str) -> Ask {
    Ask {
        id: String::new(),
        session: session.to_owned(),
        who: "Claude Code · release".to_owned(),
        task: Some("post the release notes".to_owned()),
        account: "slack.com".to_owned(),
        question: json!({"what": "send-key", "batch": batch}),
        asked: 0,
        ends: 0,
    }
}

#[test]
fn an_ask_is_private_lives_two_minutes_and_a_rerun_joins_it() {
    let folder = tempfile::tempdir().unwrap();
    let asks = asks(folder.path());
    let id = asks.ask(question("s1", "b1"), 1_000).unwrap();
    assert_eq!(
        asks.ask(question("s1", "b1"), 2_000).unwrap(),
        id,
        "the same request joins"
    );
    assert_ne!(
        asks.ask(question("s2", "b1"), 2_000).unwrap(),
        id,
        "another session asks its own"
    );
    assert_ne!(
        asks.ask(question("s1", "b2"), 2_000).unwrap(),
        id,
        "another request asks its own"
    );

    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(folder.path().join(format!("h.asks/{id}.json")))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);

    assert_eq!(asks.open(1_000 + millis(LIFE) - 1).len(), 3);
    asks.expire(1_000 + millis(LIFE));
    assert!(
        asks.open(1_000 + millis(LIFE))
            .iter()
            .all(|ask| ask.id != id)
    );
}

#[test]
fn only_the_person_answers_and_allow_for_this_task_covers_that_session_in_that_account() {
    let folder = tempfile::tempdir().unwrap();
    let asks = asks(folder.path());
    let id = asks.ask(question("s1", "b1"), 1_000).unwrap();
    assert_eq!(
        asks.answer(&id, Answer::Task, false, 1_000)
            .unwrap_err()
            .code,
        Code::Stopped
    );
    assert_eq!(asks.answered(&id), None);

    asks.answer(&id, Answer::Task, true, 1_000).unwrap();
    assert_eq!(asks.answered(&id), Some(Answer::Task));
    let allowed = asks.allowed(1_000);
    assert_eq!(allowed.len(), 1);
    assert_eq!(
        (allowed[0].session.as_str(), allowed[0].account.as_str()),
        ("s1", "slack.com")
    );
    assert!(
        asks.allowed(1_000 + millis(TASK_LIFE)).is_empty(),
        "an hour at most"
    );

    assert_eq!(
        asks.end(Some("slack.com"), None, false, 1_000)
            .unwrap_err()
            .code,
        Code::Stopped
    );
    asks.end(Some("slack.com"), None, true, 1_000).unwrap();
    assert!(asks.allowed(1_000).is_empty(), "End stops it");
    assert_eq!(
        asks.answer("gone", Answer::Once, true, 1_000)
            .unwrap_err()
            .code,
        Code::Input
    );
}

#[test]
fn a_wait_ends_at_the_answer_at_stop_or_at_its_deadline_and_never_polls() {
    let folder = tempfile::tempdir().unwrap();
    let asks = asks(folder.path());
    let id = asks
        .ask(question("s1", "b1"), atlas_core::time::now_ms())
        .unwrap();

    let started = Instant::now();
    assert_eq!(
        asks.wait(&id, started + Duration::from_millis(150))
            .unwrap(),
        Waited::Unanswered
    );
    assert!(started.elapsed() >= Duration::from_millis(150));

    let answering = {
        let asks = asks.clone();
        let id = id.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            asks.answer(&id, Answer::Once, true, atlas_core::time::now_ms())
                .unwrap();
        })
    };
    let started = Instant::now();
    assert_eq!(
        asks.wait(&id, started + Duration::from_secs(5)).unwrap(),
        Waited::Answered(Answer::Once)
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "woken by the answer, not the deadline"
    );
    answering.join().unwrap();

    let other = asks
        .ask(question("s1", "b9"), atlas_core::time::now_ms())
        .unwrap();
    let stopping = {
        let stop = folder.path().join("h.stopped");
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            fs::write(stop, b"").unwrap();
        })
    };
    let started = Instant::now();
    assert_eq!(
        asks.wait(&other, started + Duration::from_secs(5)).unwrap(),
        Waited::Stopped
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    stopping.join().unwrap();
}
