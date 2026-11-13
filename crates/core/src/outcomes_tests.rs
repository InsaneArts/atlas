use super::*;

fn outcomes() -> Arc<Outcomes> {
    Arc::new(Outcomes::default())
}

#[test]
fn a_change_is_running_until_its_reply_is_remembered() {
    let outcomes = outcomes();
    let change = Token::new();
    assert_eq!(outcomes.of(&change), Outcome::Unknown);

    let remembering = outcomes.begin(change.clone()).unwrap();
    assert_eq!(outcomes.of(&change), Outcome::Running);

    let reply = json!({"ok": true, "performed": 2});
    remembering.done(&reply);
    assert_eq!(outcomes.of(&change), Outcome::Done { reply });
}

#[test]
fn a_change_asked_for_twice_is_begun_once() {
    let outcomes = outcomes();
    let change = Token::new();

    let remembering = outcomes.begin(change.clone()).unwrap();
    assert!(outcomes.begin(change.clone()).is_none(), "still under way");

    remembering.done(&json!({"ok": true}));
    assert!(outcomes.begin(change).is_none(), "ended, and remembered");
}

#[test]
fn a_change_dropped_without_its_reply_is_forgotten() {
    let outcomes = outcomes();
    let change = Token::new();

    drop(outcomes.begin(change.clone()).unwrap());
    assert_eq!(outcomes.of(&change), Outcome::Unknown);
    assert!(outcomes.begin(change).is_some());
}

#[test]
fn the_oldest_ended_change_is_forgotten_first() {
    let outcomes = outcomes();
    let changes: Vec<Token> = (0..=KEPT).map(|_| Token::new()).collect();
    for change in &changes {
        outcomes
            .begin(change.clone())
            .unwrap()
            .done(&json!({"ok": true}));
    }

    assert_eq!(outcomes.of(&changes[0]), Outcome::Unknown);
    for change in &changes[1..] {
        assert_ne!(outcomes.of(change), Outcome::Unknown);
    }
}

#[test]
fn a_change_under_way_is_never_forgotten_for_newer_ones() {
    let outcomes = outcomes();
    let running = Token::new();
    let _remembering = outcomes.begin(running.clone()).unwrap();
    for _ in 0..=KEPT {
        outcomes
            .begin(Token::new())
            .unwrap()
            .done(&json!({"ok": true}));
    }

    assert_eq!(outcomes.of(&running), Outcome::Running);
}

#[test]
fn a_reply_too_long_to_keep_is_not_remembered() {
    let outcomes = outcomes();
    let change = Token::new();
    outcomes
        .begin(change.clone())
        .unwrap()
        .done(&json!({"ok": true, "output": "x".repeat(REPLY_MAX)}));

    assert_eq!(outcomes.of(&change), Outcome::Unknown);
}

#[test]
fn each_outcome_has_its_exact_answer() {
    for (outcome, wire) in [
        (Outcome::Unknown, r#"{"ok":true,"outcome":"unknown"}"#),
        (Outcome::Running, r#"{"ok":true,"outcome":"running"}"#),
        (
            Outcome::Done {
                reply: json!({"ok": false, "code": "busy"}),
            },
            r#"{"ok":true,"outcome":"done","reply":{"code":"busy","ok":false}}"#,
        ),
    ] {
        assert_eq!(outcome.answer().to_string(), wire);
    }
}

#[test]
fn only_an_ended_change_with_a_final_reply_is_remembered() {
    let reply = json!({"ok": true, "performed": 1});
    let done = Outcome::Done {
        reply: reply.clone(),
    };
    // As a transport hands it over, with how long the answer took.
    let mut answer = done.answer();
    answer["elapsedMs"] = json!(1.5);
    assert_eq!(Outcome::remembered(&answer), Some(reply));

    for answer in [
        Outcome::Unknown.answer(),
        Outcome::Running.answer(),
        json!({"ok": false, "code": "bad-request", "error": "unknown op 'outcome'"}),
        json!({"ok": false, "outcome": "done", "reply": {"ok": true}}),
        json!({"ok": true, "outcome": "done"}),
        json!({"ok": true, "outcome": "done", "reply": "done"}),
        json!({"ok": true, "outcome": "done", "reply": {"performed": 1}}),
        json!({"ok": true, "outcome": "done", "reply": {"ok": "yes"}}),
        json!({"ok": true, "outcome": "done", "reply": {"ok": true, "uncertain": "no"}}),
        json!({"ok": true, "outcome": "finished", "reply": {"ok": true}}),
    ] {
        assert_eq!(Outcome::remembered(&answer), None, "{answer}");
    }
}
