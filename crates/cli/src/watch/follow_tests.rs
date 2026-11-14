use super::*;
use serde_json::json;

fn event(seq: u64, kind: &str) -> Value {
    json!({"feed": "f", "seq": seq, "at": 0, "desktop": "d", "kind": kind})
}

#[test]
fn a_reset_is_followed_from_the_position_given_and_reads_everything_again() {
    let following = Following::default();

    let (next, pause) = following.take(
        None,
        &json!({"ok": true, "reset": {"feed": "f", "head": 7}}),
    );

    assert_eq!(next, Some(("f".to_owned(), 7)));
    assert_eq!(pause, Duration::ZERO);
    assert!(following.live());
    assert_eq!(following.told(), 1);
}

#[test]
fn only_what_the_status_says_asks_for_a_read_and_the_position_moves_past_every_event() {
    let following = Following::default();
    let at = Some(("f".to_owned(), 1));

    let asks = json!({"ok": true, "feed": "f", "head": 2, "events": [event(2, "asks")]});
    let (next, _) = following.take(at, &asks);
    assert_eq!(next, Some(("f".to_owned(), 2)));
    assert_eq!(
        following.told(),
        0,
        "asks are not in the status a watch says"
    );

    let status = json!({"ok": true, "feed": "f", "head": 4, "events": [event(3, "asks"), event(4, "status")]});
    let (next, _) = following.take(next, &status);
    assert_eq!(next, Some(("f".to_owned(), 4)));
    assert_eq!(following.told(), 1);

    let unknown =
        json!({"ok": true, "feed": "f", "head": 5, "events": [event(5, "something-new")]});
    following.take(next, &unknown);
    assert_eq!(
        following.told(),
        2,
        "a kind this build does not know may say anything"
    );
}

#[test]
fn a_wait_that_ends_with_nothing_keeps_the_position() {
    let following = Following::default();
    let at = Some(("f".to_owned(), 3));

    let (next, pause) = following.take(
        at.clone(),
        &json!({"ok": true, "feed": "f", "head": 3, "events": []}),
    );

    assert_eq!(next, at);
    assert_eq!(pause, Duration::ZERO);
    assert_eq!(following.told(), 0);
}

#[test]
fn a_service_past_its_waits_is_left_a_moment_while_the_log_is_looked_at() {
    let following = Following::default();
    let at = Some(("f".to_owned(), 3));

    let (next, pause) = following.take(
        at.clone(),
        &json!({"ok": true, "feed": "f", "head": 3, "events": [], "poll": true}),
    );

    assert_eq!((next, pause), (at, POLL));
    assert!(!following.live());
}

#[test]
fn an_answer_that_is_not_the_feeds_is_left_a_while_and_the_log_looked_at() {
    let following = Following::default();
    let at = Some(("f".to_owned(), 3));

    let (next, pause) = following.take(at.clone(), &json!({"ok": true}));

    assert_eq!((next, pause), (at, PROBE));
    assert!(!following.live());
    assert_eq!(following.told(), 0);
}
