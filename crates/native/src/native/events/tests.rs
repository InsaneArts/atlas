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

/// A drag presses at its source, travels no further than its destination,
/// and releases there, recording what is held before every event.
#[tokio::test(start_paused = true)]
async fn a_complete_drag_releases_at_its_requested_destination() {
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

    for (case, from, to) in cases {
        let directory = tempfile::tempdir().unwrap();
        let (tap, seen) = tap::Tap::new();
        let mut engine = Engine::tapped(epoch(), Some(directory.path().join("held.json")), tap);
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
        let first = posts.first().unwrap();
        assert_eq!(
            (first.kind, at(first)),
            (moved, from),
            "{case}: moves to the source"
        );
        assert_eq!(
            (posts[1].kind, at(&posts[1])),
            (down, from),
            "{case}: presses there"
        );
        let release = posts.iter().position(|posted| posted.kind == up).unwrap();
        assert_eq!(
            at(&posts[release]),
            to,
            "{case}: releases at the destination"
        );
        assert_eq!(
            release,
            posts.len() - 1,
            "{case}: nothing moves after the release"
        );
        let before = &posts[posts.len() - 2];
        assert_eq!(
            (before.kind, at(before)),
            (dragged, to),
            "{case}: drags to it"
        );

        let gesture = &posts[1..];
        let low = (from.0.min(to.0), from.1.min(to.1));
        let high = (from.0.max(to.0), from.1.max(to.1));
        for posted in gesture {
            assert!(
                (low.0..=high.0).contains(&posted.x) && (low.1..=high.1).contains(&posted.y),
                "{case}: {posted:?} overshoots"
            );
            assert_eq!(posted.number, gesture[0].number, "{case}: one gesture");
        }
        assert!(
            gesture[1..gesture.len() - 1]
                .iter()
                .all(|posted| posted.kind == dragged),
            "{case}: only drags between press and release"
        );
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
        let travelled = gesture[1..]
            .iter()
            .fold((0, 0), |(x, y), posted| (x + posted.dx, y + posted.dy));
        assert_eq!(
            travelled,
            (
                (to.0.round() - from.0.round()) as i64,
                (to.1.round() - from.1.round()) as i64,
            ),
            "{case}: travels exactly the requested distance"
        );

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
