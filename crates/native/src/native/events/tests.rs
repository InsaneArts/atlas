use super::*;
use atlas_core::actions::{COMMAND, CONTROL, FUNCTION, OPTION, SHIFT};

/// The typed event kinds, buttons and modifier masks are the raw values
/// input has always sent.
#[test]
fn typed_constants_keep_their_raw_values() {
    let raw = |button: Button| {
        let mouse = Mouse::from(button);
        (mouse.number.0, mouse.down.0, mouse.up.0, mouse.dragged.0)
    };

    assert_eq!(raw(Button::Left), (0, 1, 2, 6));
    assert_eq!(raw(Button::Right), (1, 3, 4, 7));
    assert_eq!(raw(Button::Middle), (2, 25, 26, 27));
    assert_eq!(CGEventType::MouseMoved.0, 5);

    assert_eq!(COMMAND, CGEventFlags::MaskCommand.0);
    assert_eq!(CONTROL, CGEventFlags::MaskControl.0);
    assert_eq!(OPTION, CGEventFlags::MaskAlternate.0);
    assert_eq!(SHIFT, CGEventFlags::MaskShift.0);
    assert_eq!(FUNCTION, CGEventFlags::MaskSecondaryFn.0);
}

/// Each modifier is released with the flags still held after it, and
/// one that fails keeps itself and the rest held for the next try.
#[test]
fn a_failed_modifier_release_is_retried_with_the_rest() {
    let mut held = COMMAND | SHIFT | CONTROL;
    let mut posted = Vec::new();

    let failed = release_each(&mut held, |code, rest| {
        if code == 56 {
            return Err(Error::new(Code::Native, "Could not post"));
        }

        posted.push((code, rest));
        Ok(())
    });

    assert!(failed.is_err());
    assert_eq!(posted, [(55, SHIFT | CONTROL)]);
    assert_eq!(held, SHIFT | CONTROL, "shift and control are still held");

    posted.clear();
    release_each(&mut held, |code, rest| {
        posted.push((code, rest));
        Ok(())
    })
    .unwrap();

    assert_eq!(posted, [(56, CONTROL), (59, 0)]);
    assert_eq!(held, 0);
}

/// Flags no modifier key stands for do not stay held once every
/// modifier key is up.
#[test]
fn nothing_stays_held_once_every_modifier_is_up() {
    let mut held = CGEventFlags::MaskAlphaShift.0;
    let mut posted = 0;

    release_each(&mut held, |_, _| {
        posted += 1;
        Ok(())
    })
    .unwrap();

    assert_eq!((held, posted), (0, 0));
}

/// The named constants are the raw values input has always used; in
/// particular events go to the session tap (1), never the HID tap (0).
#[test]
fn event_constants_keep_their_raw_values() {
    assert_eq!(TAP.0, 1);
    assert_ne!(TAP, CGEventTapLocation::HIDEventTap);
    assert_eq!(CGEventField::MouseEventNumber.0, 0);
    assert_eq!(CGEventField::MouseEventClickState.0, 1);
    assert_eq!(CGEventField::MouseEventDeltaX.0, 4);
    assert_eq!(CGEventField::MouseEventDeltaY.0, 5);
    assert_eq!(CGScrollEventUnit::Line.0, 1);
    assert_eq!(CGEventSourceStateID::Private.0, -1);
}

fn epoch() -> InputEpoch {
    InputEpoch {
        uid: 501,
        security_session_id: 1,
        display_id: 2,
        width: 800.0,
        height: 600.0,
        process_id: 1,
    }
}

/// What a tapped engine posted, in order.
fn posts(seen: &[tap::Seen]) -> Vec<tap::Posted> {
    seen.iter()
        .filter_map(|seen| match seen {
            tap::Seen::Post(posted) => Some(*posted),
            tap::Seen::Write { .. } | tap::Seen::Check => None,
        })
        .collect()
}

/// AppKit anchors a native window move at the first dragged event that
/// moves, one first step from the press. So a drag presses at its source,
/// steps once towards its target, travels to one first step past the target
/// (kept on the display), releases there and then rests on the target,
/// recording what is held before every event.
#[tokio::test(start_paused = true)]
async fn a_drag_releases_one_first_step_past_its_target_and_then_rests_on_it() {
    let (moved, down, dragged, up) = (
        CGEventType::MouseMoved.0,
        CGEventType::LeftMouseDown.0,
        CGEventType::LeftMouseDragged.0,
        CGEventType::LeftMouseUp.0,
    );
    let cases = [
        ("positive diagonal", (100.0, 100.0), (200.0, 150.0)),
        ("negative diagonal", (300.0, 250.0), (200.0, 200.0)),
        ("horizontal", (100.0, 100.0), (160.0, 100.0)),
        ("vertical", (100.0, 100.0), (100.0, 40.0)),
        ("stationary", (120.0, 80.0), (120.0, 80.0)),
        ("fractional", (10.5, 20.25), (110.75, 70.5)),
        ("far display edge", (700.0, 500.0), (799.0, 599.0)),
        ("near display edge", (50.0, 50.0), (0.0, 0.0)),
    ];
    let display = epoch();

    for (case, from, to) in cases {
        let step = |from: f64, to: f64| (to - from).clamp(-DRAG_ANCHOR, DRAG_ANCHOR);
        let first = (step(from.0, to.0), step(from.1, to.1));
        let anchor = (from.0 + first.0, from.1 + first.1);
        let end = (
            (to.0 + first.0).clamp(0.0, display.width - 1.0),
            (to.1 + first.1).clamp(0.0, display.height - 1.0),
        );

        let directory = tempfile::tempdir().unwrap();
        let (tap, seen) = tap::Tap::new();
        let mut engine = Engine::tapped(
            display.clone(),
            Some(directory.path().join("held.json")),
            tap,
        );
        engine
            .perform(Action::Drag {
                from_x: from.0,
                from_y: from.1,
                to_x: to.0,
                to_y: to.1,
                button: Button::Left,
            })
            .await
            .unwrap();

        let seen = seen.lock().unwrap().clone();
        let posts = posts(&seen);
        let at = |posted: &tap::Posted| (posted.x, posted.y);
        let kinds: Vec<u32> = posts.iter().map(|posted| posted.kind).collect();
        let mut expected = vec![moved, down];
        expected.extend(std::iter::repeat_n(dragged, 1 + DRAG_STEPS as usize));
        expected.extend([up, moved]);
        assert_eq!(kinds, expected, "{case}: event kinds");

        assert_eq!(at(&posts[0]), from, "{case}: moves to the source");
        assert_eq!(at(&posts[1]), from, "{case}: presses there");
        assert_eq!(
            (at(&posts[2]), (posts[2].dx, posts[2].dy)),
            (anchor, (first.0 as i64, first.1 as i64)),
            "{case}: the first drag moves one first step"
        );
        let release = posts.len() - 2;
        assert_eq!(
            at(&posts[release - 1]),
            end,
            "{case}: drags past the target"
        );
        assert_eq!(
            at(&posts[release]),
            end,
            "{case}: releases one first step past the target"
        );
        assert_eq!(
            at(&posts[release + 1]),
            to,
            "{case}: then rests on the target"
        );

        let gesture = &posts[1..=release];
        let low = (from.0.min(end.0), from.1.min(end.1));
        let high = (from.0.max(end.0), from.1.max(end.1));
        for posted in gesture {
            assert!(
                (low.0..=high.0).contains(&posted.x) && (low.1..=high.1).contains(&posted.y),
                "{case}: {posted:?} leaves the drag's path"
            );
            assert_eq!(posted.number, gesture[0].number, "{case}: one gesture");
        }
        for pair in posts.windows(2) {
            assert_eq!(
                (pair[1].dx, pair[1].dy),
                (
                    (pair[1].x.round() - pair[0].x.round()) as i64,
                    (pair[1].y.round() - pair[0].y.round()) as i64,
                ),
                "{case}: each delta is the move since the last event"
            );
        }
        let anchored = posts[3..=release]
            .iter()
            .fold((0, 0), |(x, y), posted| (x + posted.dx, y + posted.dy));
        assert_eq!(
            anchored,
            (
                (end.0.round() - anchor.0.round()) as i64,
                (end.1.round() - anchor.1.round()) as i64,
            ),
            "{case}: travels from the anchor to the release"
        );
        if end == (to.0 + first.0, to.1 + first.1) {
            assert_eq!(
                anchored,
                (
                    (to.0.round() - from.0.round()) as i64,
                    (to.1.round() - from.1.round()) as i64,
                ),
                "{case}: what the anchor drags moves exactly as asked"
            );
        }

        for (index, event) in seen.iter().enumerate() {
            if matches!(event, tap::Seen::Post(_)) {
                assert_eq!(seen[index - 1], tap::Seen::Check, "{case}: checked first");
                assert!(
                    matches!(seen[index - 2], tap::Seen::Write { .. }),
                    "{case}: recorded first"
                );
            }
        }
        assert!(!engine.held_input(), "{case}: nothing stays held");
        assert_eq!(
            seen.iter()
                .rev()
                .find(|seen| matches!(seen, tap::Seen::Write { .. })),
            Some(&tap::Seen::Write {
                button: None,
                keys: Vec::new(),
                flags: 0,
            }),
            "{case}: the journal ends with nothing held"
        );
    }
}

/// A look at nothing, for a check that only names the action under way.
struct Nowhere;

impl crate::native::asking::Look for Nowhere {
    fn at_point(&mut self, _: f64, _: f64) -> crate::native::asking::Target {
        crate::native::asking::Target::default()
    }
    fn focused(&mut self) -> crate::native::asking::Target {
        crate::native::asking::Target::default()
    }
    fn referenced(
        &mut self,
        _: &atlas_core::screen::elements::Reference,
    ) -> crate::native::asking::Target {
        crate::native::asking::Target::default()
    }
    fn message(&mut self) -> crate::native::asking::Message {
        crate::native::asking::Message::Unread
    }
    fn unmoved(&mut self) -> bool {
        false
    }
}

#[tokio::test(start_paused = true)]
async fn each_character_after_the_first_is_judged_again_and_one_that_asks_stops_the_rest() {
    let directory = tempfile::tempdir().unwrap();
    let (tap, seen) = tap::Tap::new();
    let mut engine = Engine::tapped(epoch(), Some(directory.path().join("held.json")), tap);
    // Every app asks while the list can't be read, and a line break sends.
    let asking = crate::native::asking::Asking {
        unknown: true,
        ..crate::native::asking::Asking::default()
    };
    let typing = Action::Type {
        text: "a\nb".to_owned(),
    };
    let mut recheck = crate::native::asking::recheck(std::slice::from_ref(&typing), &asking);
    let _ = recheck.check(&typing, &mut Nowhere);
    engine.recheck = Some(recheck);

    let error = engine.perform(typing).await.unwrap_err();
    assert_eq!(error.code, Code::WaitingForPerson);
    let posts = posts(&seen.lock().unwrap().clone());
    assert_eq!(
        posts.len(),
        2,
        "the first character's down and up, nothing after"
    );
}
