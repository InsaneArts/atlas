use super::*;
use serde_json::json;

fn slack() -> Value {
    json!({"kind": "site", "id": "slack.com", "browser": "com.google.Chrome"})
}

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
        process: None,
        who: "Claude Code · release".to_owned(),
        task: Some("post the release notes".to_owned()),
        account: "slack.com".to_owned(),
        question: json!({
            "what": "send-key",
            "batch": batch,
            "accountKey": slack(),
            "bind": {"accountKey": slack(), "batch": batch, "value": "m1"},
        }),
        asked: 0,
        ends: 0,
        picture: None,
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
    assert_eq!(allowed[0].session, "s1");
    assert_eq!(allowed[0].account, slack());
    assert_eq!(allowed[0].name, "slack.com");
    assert!(
        asks.allowed(1_000 + millis(TASK_LIFE)).is_empty(),
        "an hour at most"
    );

    assert_eq!(
        asks.end(Some(&slack()), None, false, 1_000)
            .unwrap_err()
            .code,
        Code::Stopped
    );
    asks.end(Some(&slack()), None, true, 1_000).unwrap();
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

#[test]
fn a_changed_message_is_a_question_of_its_own() {
    let folder = tempfile::tempdir().unwrap();
    let asks = asks(folder.path());
    let first = asks.ask(question("s1", "b1"), 1_000).unwrap();
    let mut changed = question("s1", "b1");
    changed.question["bind"]["value"] = json!("m2");
    assert_ne!(asks.ask(changed, 1_000).unwrap(), first);
}

#[test]
fn grants_and_ends_at_once_never_undo_one_another() {
    let folder = tempfile::tempdir().unwrap();
    let asks = asks(folder.path());
    let ids: Vec<String> = (0..8)
        .map(|index| {
            let mut ask = question(&format!("s{index}"), "b1");
            ask.question["accountKey"] =
                json!({"kind": "site", "id": format!("site{index}.example")});
            asks.ask(ask, 1_000).unwrap()
        })
        .collect();
    let answering: Vec<_> = ids
        .into_iter()
        .map(|id| {
            let asks = asks.clone();
            std::thread::spawn(move || asks.answer(&id, Answer::Task, true, 1_000).unwrap())
        })
        .collect();
    for thread in answering {
        thread.join().unwrap();
    }
    assert_eq!(asks.allowed(1_000).len(), 8, "every grant kept");
    let ending: Vec<_> = (0..8)
        .map(|index| {
            let asks = asks.clone();
            std::thread::spawn(move || {
                let account = json!({"kind": "site", "id": format!("site{index}.example")});
                asks.end(Some(&account), None, true, 1_000).unwrap();
            })
        })
        .collect();
    for thread in ending {
        thread.join().unwrap();
    }
    assert!(asks.allowed(1_000).is_empty(), "every End kept");
}

#[test]
fn just_this_once_goes_to_the_first_waiter_that_claims_it_and_to_no_other() {
    let folder = tempfile::tempdir().unwrap();
    let asks = asks(folder.path());
    let id = asks.ask(question("s1", "b1"), 1_000).unwrap();
    assert_eq!(
        asks.claim_once(&id, 2_000).unwrap(),
        None,
        "not answered yet"
    );
    asks.answer(&id, Answer::Once, true, 2_000).unwrap();
    let claims: Vec<bool> = std::thread::scope(|scope| {
        let waiters: Vec<_> = (0..2)
            .map(|_| scope.spawn(|| asks.claim_once(&id, 3_000).unwrap().is_some()))
            .collect();
        waiters
            .into_iter()
            .map(|waiter| waiter.join().unwrap())
            .collect()
    });
    assert_eq!(claims.iter().filter(|claimed| **claimed).count(), 1);
    assert!(
        asks.open(3_000).is_empty(),
        "the question came down with it"
    );
    assert_eq!(asks.answered(&id), None);
}

#[test]
fn allow_for_this_task_is_kept_for_the_host_process_that_asked_and_ends_when_it_does() {
    let folder = tempfile::tempdir().unwrap();
    let asks = asks(folder.path());
    let process = atlas_core::process::Instance::new(812, [1, 0]);
    let id = asks
        .ask(
            Ask {
                process: Some(process),
                ..question("s1", "b1")
            },
            1_000,
        )
        .unwrap();
    asks.answer(&id, Answer::Task, true, 2_000).unwrap();
    assert_eq!(asks.allowed(3_000)[0].process, Some(process));
    asks.end_unless(3_000, |grant| grant.process != Some(process))
        .unwrap();
    assert!(asks.allowed(3_000).is_empty());
}

#[test]
fn a_choice_a_question_doesnt_offer_cant_be_given() {
    let folder = tempfile::tempdir().unwrap();
    let asks = asks(folder.path());
    let mut ask = question("s1", "b1");
    ask.question["task"] = json!(false);
    ask.question["once"] = json!(false);
    let id = asks.ask(ask, 1_000).unwrap();
    for answer in [Answer::Task, Answer::Once] {
        assert_eq!(
            asks.answer(&id, answer, true, 2_000).unwrap_err().code,
            Code::Input,
            "{answer:?}"
        );
    }
    assert!(asks.allowed(2_000).is_empty());
    asks.answer(&id, Answer::Deny, true, 2_000).unwrap();
}

#[test]
fn an_ask_that_ran_out_is_gone_from_disk_at_the_next_read_whoever_reads() {
    let folder = tempfile::tempdir().unwrap();
    let asks = asks(folder.path());
    let files = |id: &str| {
        ["json", "answer"]
            .map(|ending| folder.path().join(format!("h.asks/{id}.{ending}")).exists())
    };
    let ended = 1_000 + millis(LIFE);

    let read = asks.ask(question("s1", "b1"), 1_000).unwrap();
    assert_eq!(files(&read), [true, false]);
    assert!(asks.open(ended).is_empty());
    assert_eq!(files(&read), [false, false], "a read of the open asks");

    let answered = asks.ask(question("s1", "b2"), 1_000).unwrap();
    assert_eq!(
        asks.answer(&answered, Answer::Deny, true, ended)
            .unwrap_err()
            .code,
        Code::Input
    );
    assert_eq!(files(&answered), [false, false], "an answer that came late");
}

#[test]
fn a_preview_taken_off_when_its_wait_ends_shows_again_when_a_rerun_joins() {
    let folder = tempfile::tempdir().unwrap();
    let asks = asks(folder.path());
    let waiting = || {
        let mut ask = question("s1", "b1");
        ask.question["message"] = json!({"preview": "canary", "digest": "m1"});
        ask
    };
    let id = asks.ask(waiting(), 1_000).unwrap();
    asks.forget_preview(&id).unwrap();
    let on_disk = fs::read_to_string(folder.path().join(format!("h.asks/{id}.json"))).unwrap();
    assert!(!on_disk.contains("canary"), "{on_disk}");
    assert_eq!(asks.ask(waiting(), 2_000).unwrap(), id);
    assert_eq!(asks.open(2_000)[0].question["message"]["preview"], "canary");

    asks.close(&id);
    asks.forget_preview(&id).unwrap();
    assert!(asks.open(2_000).is_empty(), "never written back once down");
}

#[test]
fn a_permit_is_taken_once_by_its_own_session_for_its_own_action() {
    let folder = tempfile::tempdir().unwrap();
    let asks = asks(folder.path());
    let process = atlas_core::process::Instance::new(812, [1, 0]);
    let permit = Permit {
        session: "s1".to_owned(),
        process: Some(process),
        account: slack(),
        action: "return".to_owned(),
        target: json!({"window": 7}),
        value: Some("m1".to_owned()),
        ends: u64::MAX,
    };
    asks.permit(permit.clone(), 1_000).unwrap();
    let take = |session: &str, process, action: &str, now| {
        asks.take_permits(session, process, &[action.to_owned()], now)
            .unwrap()
    };
    assert!(
        take("s2", Some(process), "return", 2_000).is_empty(),
        "another session"
    );
    assert!(
        take("s1", None, "return", 2_000).is_empty(),
        "another process"
    );
    assert!(
        take("s1", Some(process), "click", 2_000).is_empty(),
        "another action"
    );
    let taken = take("s1", Some(process), "return", 2_000);
    assert_eq!(taken.len(), 1);
    assert!(
        take("s1", Some(process), "return", 2_000).is_empty(),
        "only once"
    );

    asks.return_permits(taken, 3_000).unwrap();
    assert_eq!(
        take("s1", Some(process), "return", 3_000).len(),
        1,
        "given back"
    );
    asks.permit(permit, 1_000).unwrap();
    assert!(
        take("s1", Some(process), "return", 1_000 + millis(LIFE)).is_empty(),
        "two minutes at most"
    );
}

/// A frame, as the input service reads one.
fn frame(x: f64) -> Value {
    json!({"x": x, "y": 20.0, "width": 80.0, "height": 24.0})
}

/// Every picture file of ask `id` in `folder`.
fn pictures_of(folder: &Path, id: &str) -> Vec<String> {
    fs::read_dir(folder.join("h.asks"))
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| entry.file_name().into_string().ok())
                .filter(|name| name.starts_with(id) && name.ends_with(".jpg"))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn an_asks_picture_is_private_and_goes_with_it_however_it_goes() {
    let folder = tempfile::tempdir().unwrap();
    let asks = asks(folder.path());
    let now = atlas_core::time::now_ms();
    let pictured = |batch: &str, process| {
        let id = asks
            .ask(
                Ask {
                    process,
                    ..question("s1", batch)
                },
                now,
            )
            .unwrap();
        // A rerun's newer one too.
        asks.keep_picture(&id, b"jpeg", &frame(10.0), (1280, 800))
            .unwrap();
        asks.keep_picture(&id, b"jpeg", &frame(20.0), (1280, 800))
            .unwrap();
        let named = asks
            .open(now)
            .into_iter()
            .find(|ask| ask.id == id)
            .unwrap()
            .picture;
        assert_eq!(
            pictures_of(folder.path(), &id),
            [named.unwrap()],
            "only the one it names, while it is up"
        );
        id
    };

    let mut answered = Vec::new();
    for answer in [Answer::Once, Answer::Task, Answer::Deny] {
        let id = pictured(&format!("{answer:?}"), None);
        let name = pictures_of(folder.path(), &id).remove(0);
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(folder.path().join("h.asks").join(name))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        // Answered through the person's own route, with no command left to
        // take the question down.
        asks.answer(&id, answer, true, now).unwrap();
        assert!(pictures_of(folder.path(), &id).is_empty(), "{answer:?}");
        answered.push(id);
    }
    assert!(
        asks.open(now).iter().all(|ask| ask.picture.is_none()),
        "no card names it"
    );
    asks.keep_picture(&answered[0], b"jpeg", &frame(10.0), (1280, 800))
        .unwrap();
    assert!(
        pictures_of(folder.path(), &answered[0]).is_empty(),
        "none for one answered"
    );

    let expired = pictured("b3", None);
    assert!(asks.open(now + millis(LIFE)).is_empty());
    assert!(pictures_of(folder.path(), &expired).is_empty(), "ran out");

    let gone = atlas_core::process::Instance::new(812, [1, 0]);
    let ended = pictured("b4", Some(gone));
    asks.close_unless(now, |ask| ask.process != Some(gone));
    assert!(
        pictures_of(folder.path(), &ended).is_empty(),
        "its session ended"
    );
    asks.keep_picture(&ended, b"jpeg", &frame(10.0), (1280, 800))
        .unwrap();
    assert!(
        pictures_of(folder.path(), &ended).is_empty(),
        "never kept for one that is down"
    );
}

#[test]
fn a_reader_never_finds_a_picture_beside_a_frame_that_isnt_its_own() {
    let folder = tempfile::tempdir().unwrap();
    let asks = asks(folder.path());
    let now = atlas_core::time::now_ms();
    let id = asks.ask(question("s1", "b1"), now).unwrap();
    let done = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for n in 0..200 {
                let x = f64::from(n);
                asks.keep_picture(&id, x.to_string().as_bytes(), &frame(x), (1280, 800))
                    .unwrap();
            }
            done.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        let mut seen = 0;
        while !done.load(std::sync::atomic::Ordering::SeqCst) {
            let Some(ask) = asks.open(now).into_iter().next() else {
                continue;
            };
            let Some(name) = ask.picture else {
                continue;
            };
            // Gone already, once a newer one is named: never a mismatch.
            let Ok(bytes) = fs::read(asks.folder().join(&name)) else {
                continue;
            };
            let x = ask.question["frame"]["x"].as_f64().unwrap();
            assert_eq!(String::from_utf8(bytes).unwrap(), x.to_string(), "{name}");
            seen += 1;
        }
        assert!(seen > 0, "a reader saw some");
    });
    assert_eq!(
        pictures_of(folder.path(), &id).len(),
        1,
        "only the last left"
    );
}

#[test]
fn a_rerun_that_joins_replaces_the_picture_and_its_frame_together() {
    let folder = tempfile::tempdir().unwrap();
    let asks = asks(folder.path());
    let now = atlas_core::time::now_ms();
    let asked = |x| {
        let mut ask = question("s1", "b1");
        ask.question["frame"] = frame(x);
        asks.ask(ask, now).unwrap()
    };
    let id = asked(10.0);
    asks.keep_picture(&id, b"first", &frame(10.0), (1280, 800))
        .unwrap();
    // The element moved; the rerun that joins brings both anew.
    assert_eq!(asked(300.0), id);
    assert_eq!(
        asks.open(now)[0].question["frame"],
        frame(10.0),
        "not one alone"
    );
    asks.keep_picture(&id, b"second", &frame(300.0), (1280, 800))
        .unwrap();
    assert_eq!(asks.open(now)[0].question["frame"], frame(300.0));
    let name = asks.open(now)[0].picture.clone().unwrap();
    assert_eq!(fs::read(asks.folder().join(name)).unwrap(), b"second");
    assert_eq!(
        pictures_of(folder.path(), &id).len(),
        1,
        "the first is gone"
    );
}

#[test]
fn a_question_that_runs_out_while_its_picture_waits_for_the_lock_goes_and_gets_none() {
    let folder = tempfile::tempdir().unwrap();
    let asks = asks(folder.path());
    // It ends a moment from now.
    let id = asks
        .ask(
            question("s1", "b1"),
            atlas_core::time::now_ms() + 150 - millis(LIFE),
        )
        .unwrap();
    let held = asks.lock().unwrap();
    let publishing = std::thread::spawn({
        let asks = asks.clone();
        let id = id.clone();
        move || {
            asks.keep_picture(&id, b"jpeg", &frame(10.0), (1280, 800))
                .unwrap()
        }
    });
    // It runs out while the publication waits.
    std::thread::sleep(std::time::Duration::from_millis(300));
    drop(held);
    publishing.join().unwrap();
    assert!(pictures_of(folder.path(), &id).is_empty());
    assert!(
        !folder.path().join(format!("h.asks/{id}.json")).exists(),
        "and the question itself is gone"
    );
}

#[test]
fn a_picture_write_cut_short_is_gone_with_its_ask() {
    let folder = tempfile::tempdir().unwrap();
    let asks = asks(folder.path());
    let now = atlas_core::time::now_ms();
    let id = asks.ask(question("s1", "b1"), now).unwrap();
    asks.keep_picture(&id, b"jpeg", &frame(10.0), (1280, 800))
        .unwrap();
    let shown = &asks.open(now)[0].question;
    assert_eq!(shown["pictureSize"], json!({"width": 1280, "height": 800}));
    // A writer killed between writing and placing its picture.
    let cut = asks.folder().join(format!(".{id}-0123abcd.jpg.4567ef"));
    fs::write(&cut, b"private").unwrap();
    asks.answer(&id, Answer::Deny, true, now).unwrap();
    assert!(!cut.exists(), "taken with the question's own");
    assert!(pictures_of(folder.path(), &id).is_empty());
}
