use super::*;
use crate::actions::{COMMAND, CONTROL, FUNCTION, OPTION, SHIFT};

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
