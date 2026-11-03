use super::*;

fn state() -> watch::Sender<State> {
    watch::channel(State {
        revision: 0,
        frame: None,
        controlling: true,
        paused: false,
        phase: ControlPhase::Controlling,
        view: viewport::Viewport::default(),
        diagnostics: 0,
        input_ack_ms: None,
        last_frame_error: None,
        message: String::new(),
    })
    .0
}

fn scroll(dx: i64, dy: i64) -> Value {
    json!({"type":"scroll", "x":10.0, "y":20.0, "dx":dx, "dy":dy})
}

fn enqueue(session: &mut Session, states: &watch::Sender<State>, frame: &str, action: Value) {
    session.command(
        Command::Input {
            frame: frame.into(),
            action,
        },
        states,
    );
}

fn waiting() -> Session {
    let mut session = Session::new("http://127.0.0.1:1/".into(), false);
    session.lease = Some("held".into());
    session.pending = true;
    session
}

#[tokio::test]
async fn wheel_bursts_gather_while_input_is_blocked_instead_of_releasing_control() {
    let states = state();
    let mut session = waiting();
    for _ in 0..64 {
        enqueue(&mut session, &states, "observed", scroll(0, 1));
    }
    assert_eq!(session.queue.len(), 1);
    assert_eq!(session.queue[0].1["dy"], 64);
    assert!(!session.releasing);
    assert!(session.operations.is_empty());
}

#[tokio::test]
async fn scroll_travel_past_one_actions_limit_is_preserved_in_later_actions() {
    let states = state();
    let mut session = waiting();
    for _ in 0..301 {
        enqueue(&mut session, &states, "observed", scroll(-1, 1));
    }
    assert_eq!(session.queue.len(), 4);
    let x: i64 = session
        .queue
        .iter()
        .map(|(_, action)| action["dx"].as_i64().unwrap())
        .sum();
    let y: i64 = session
        .queue
        .iter()
        .map(|(_, action)| action["dy"].as_i64().unwrap())
        .sum();
    assert_eq!((x, y), (-301, 301));
    assert!(!session.releasing);
    for (_, action) in &session.queue {
        atlas_core::actions::check(
            &serde_json::from_value::<Vec<atlas_core::actions::Action>>(json!([action])).unwrap(),
            &atlas_core::session::Display {
                width: 100.0,
                height: 100.0,
                scale: 1.0,
            },
        )
        .unwrap();
    }
}

#[tokio::test]
async fn scrolling_does_not_merge_across_frame_target_direction_or_key_boundaries() {
    let states = state();
    let mut session = waiting();
    enqueue(&mut session, &states, "first", scroll(0, 1));
    enqueue(&mut session, &states, "second", scroll(0, 1));
    let mut elsewhere = scroll(0, 1);
    elsewhere["x"] = json!(30.0);
    enqueue(&mut session, &states, "second", elsewhere.clone());
    elsewhere["dy"] = json!(-1);
    enqueue(&mut session, &states, "second", elsewhere.clone());
    enqueue(
        &mut session,
        &states,
        "second",
        json!({"type":"key","key":"up"}),
    );
    enqueue(&mut session, &states, "second", elsewhere);
    assert_eq!(session.queue.len(), 6);
    assert_eq!(session.queue[0].0, "first");
    assert_eq!(session.queue[1].0, "second");
    assert_eq!(session.queue[4].1["type"], "key");
}

#[test]
fn one_request_contains_at_most_one_native_scroll_settlement() {
    let mut queue = VecDeque::from([
        ("frame".into(), scroll(0, 100)),
        ("frame".into(), scroll(0, 25)),
        ("frame".into(), json!({"type":"type","text":"a"})),
        ("frame".into(), json!({"type":"key","key":"up"})),
        ("frame".into(), scroll(0, -2)),
    ]);
    let mut batches = Vec::new();
    while let Some((_, actions)) = input_batch(&mut queue) {
        assert_eq!(
            actions
                .iter()
                .filter(|action| action["type"] == "scroll")
                .count(),
            1
        );
        batches.push(actions);
    }
    assert_eq!(batches.iter().map(Vec::len).collect::<Vec<_>>(), [1, 3, 1]);
    assert_eq!(batches[0][0]["dy"], 100);
    assert_eq!(batches[1][0]["dy"], 25);
    assert_eq!(batches[1][2]["key"], "up");
    assert_eq!(batches[2][0]["dy"], -2);
}

#[test]
fn plain_text_merging_keeps_key_frame_and_unicode_grapheme_boundaries() {
    let mut queue = VecDeque::from([
        ("first".into(), json!({"type":"type","text":"a"})),
        ("first".into(), json!({"type":"type","text":"b"})),
        ("first".into(), json!({"type":"key","key":"tab"})),
        ("first".into(), json!({"type":"type","text":"e"})),
        ("first".into(), json!({"type":"type","text":"\u{301}"})),
        ("second".into(), json!({"type":"type","text":"c"})),
    ]);
    let (frame, actions) = input_batch(&mut queue).unwrap();
    assert_eq!(frame, "first");
    assert_eq!(
        actions,
        json!([
            {"type":"type","text":"ab"},
            {"type":"key","key":"tab"},
            {"type":"type","text":"e"},
            {"type":"type","text":"\u{301}"},
        ])
        .as_array()
        .unwrap()
        .clone()
    );
    assert_eq!(input_batch(&mut queue).unwrap().0, "second");
}

#[test]
fn text_merging_keeps_the_original_sixteen_event_batch_bound() {
    let mut queue = (0..20)
        .map(|_| ("frame".into(), json!({"type":"type","text":"x"})))
        .collect();
    let (_, actions) = input_batch(&mut queue).unwrap();
    assert_eq!(
        actions,
        json!([{"type":"type","text":"xxxxxxxxxxxxxxxx"}])
            .as_array()
            .unwrap()
            .clone()
    );
    assert_eq!(queue.len(), 4);
}

#[tokio::test]
async fn typing_bursts_compact_before_the_queue_limit_without_growing_native_batches() {
    let states = state();
    let mut session = waiting();
    let text: String = (0..100)
        .map(|index| char::from(b'a' + index % 26))
        .collect();
    for character in text.chars() {
        enqueue(
            &mut session,
            &states,
            "observed",
            json!({"type":"type","text":character.to_string()}),
        );
    }
    assert_eq!(session.queue.len(), 7);
    assert!(!session.releasing, "typing must not exhaust action slots");
    assert!(session.operations.is_empty());
    let mut sent = String::new();
    while let Some((frame, actions)) = input_batch(&mut session.queue) {
        assert_eq!(frame, "observed");
        let batch: String = actions
            .iter()
            .map(|action| action["text"].as_str().unwrap())
            .collect();
        assert!(
            batch.len() <= 16,
            "compaction must not lengthen native work per request"
        );
        sent.push_str(&batch);
    }
    assert_eq!(sent, text);
}

#[tokio::test]
async fn queued_text_compaction_preserves_frame_key_unicode_and_pointer_boundaries() {
    let states = state();
    let mut session = waiting();
    for (frame, action) in [
        ("first", json!({"type":"type","text":"a"})),
        ("first", json!({"type":"type","text":"b"})),
        ("second", json!({"type":"type","text":"c"})),
        ("second", json!({"type":"type","text":"\u{301}"})),
        ("second", json!({"type":"type","text":"d"})),
        ("second", json!({"type":"key","key":"tab"})),
        ("second", json!({"type":"type","text":"e"})),
        (
            "second",
            json!({"type":"pointer","phase":"up","button":"left","x":10,"y":20}),
        ),
        ("second", json!({"type":"type","text":"f"})),
    ] {
        enqueue(&mut session, &states, frame, action);
    }
    assert_eq!(session.queue.len(), 8);
    assert_eq!(
        session.queue[0],
        ("first".into(), json!({"type":"type","text":"ab"}))
    );
    assert_eq!(session.queue[1].0, "second");
    assert_eq!(session.queue[2].1["text"], "\u{301}");
    assert_eq!(session.queue[4].1["key"], "tab");
    assert_eq!(session.queue[6].1["phase"], "up");
    assert!(!session.releasing);
}

#[test]
fn already_compacted_text_spends_the_same_sixteen_character_request_budget() {
    let mut queue = VecDeque::from([
        ("frame".into(), json!({"type":"type","text":"1234567890"})),
        ("frame".into(), json!({"type":"type","text":"abcdefg"})),
        ("frame".into(), json!({"type":"key","key":"return"})),
    ]);
    let (_, first) = input_batch(&mut queue).unwrap();
    assert_eq!(first, [json!({"type":"type","text":"1234567890"})]);
    let (_, second) = input_batch(&mut queue).unwrap();
    assert_eq!(
        second,
        [
            json!({"type":"type","text":"abcdefg"}),
            json!({"type":"key","key":"return"})
        ]
    );
    assert!(queue.is_empty());
}
