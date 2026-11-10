use super::*;
use std::cell::Cell;

fn deferred_engine() -> Engine {
    let mut engine = Engine::idle(atlas_core::session::InputEpoch {
        uid: 502,
        security_session_id: 12,
        display_id: 34,
        width: 1000.0,
        height: 800.0,
        process_id: 56,
    });
    engine.dock_down = Some(Down {
        point: Point { x: 40.0, y: 50.0 },
        button: Button::Left,
        modifiers: vec!["shift".to_owned()],
        count: 2,
    });
    engine
}

fn assert_original_press(down: &Down) {
    assert_eq!(down.point, Point { x: 40.0, y: 50.0 });
    assert_eq!(down.button, Button::Left);
    assert_eq!(down.modifiers, ["shift"]);
    assert_eq!(down.count, 2);
}

#[tokio::test]
async fn an_unchanged_deferred_move_keeps_its_press_without_posting() {
    let mut engine = deferred_engine();
    engine
        .pointer(
            PointerPhase::Move,
            Point { x: 40.0, y: 50.0 },
            Button::Right,
            &["cmd".to_owned()],
            1,
        )
        .await
        .unwrap();
    assert_original_press(engine.dock_down.as_ref().unwrap());
    assert_eq!(engine.posted, 0);
    assert!(engine.held.is_none());
}

#[tokio::test]
async fn a_refused_deferred_continuation_keeps_its_original_press() {
    let mut engine = deferred_engine();
    for (phase, button, code, message) in [
        (
            PointerPhase::Down,
            Button::Left,
            Code::Input,
            "A mouse button is already held",
        ),
        (
            PointerPhase::Up,
            Button::Right,
            Code::Input,
            "Mouse button mismatch",
        ),
    ] {
        let error = engine
            .pointer(phase, Point { x: 45.0, y: 50.0 }, button, &[], 1)
            .await
            .unwrap_err();
        assert_eq!(error.code, code);
        assert_eq!(error.error, message);
        assert!(error.details.is_empty());
        assert!(!error.uncertain);
        assert_original_press(engine.dock_down.as_ref().unwrap());
        assert_eq!(engine.posted, 0);
        assert!(engine.held.is_none());
    }
}

#[tokio::test]
async fn a_targeted_deferred_continuation_keeps_its_original_press() {
    let mut engine = deferred_engine();
    engine.target = Some(atlas_core::process::Instance::new(123, [4, 5]));
    for phase in [PointerPhase::Down, PointerPhase::Move, PointerPhase::Up] {
        let error = engine
            .pointer(phase, Point { x: 45.0, y: 50.0 }, Button::Right, &[], 1)
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::FocusChanged);
        assert_eq!(
            error.error,
            "A deferred Dock press cannot continue in the app; no input was sent"
        );
        assert_eq!(error.details["reason"], "point");
        assert_eq!(error.details.len(), 1);
        assert!(!error.uncertain);
        assert_original_press(engine.dock_down.as_ref().unwrap());
        assert_eq!(engine.posted, 0);
        assert!(engine.held.is_none());
    }
}

#[test]
fn a_ready_drag_consumes_exactly_the_original_down_for_flushing() {
    let mut engine = deferred_engine();
    let down = Down::take_ready(
        &mut engine.dock_down,
        PointerPhase::Move,
        Point { x: 45.0, y: 50.0 },
        Button::Right,
        false,
    )
    .unwrap()
    .unwrap();
    assert_original_press(&down);
    assert!(engine.dock_down.is_none());
    assert!(
        Down::take_ready(
            &mut engine.dock_down,
            PointerPhase::Move,
            Point { x: 45.0, y: 50.0 },
            Button::Right,
            false,
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(engine.posted, 0);
}

#[test]
fn a_ready_tap_resolves_the_dock_again() {
    for still_on_dock in [false, true] {
        let mut engine = deferred_engine();
        let posted = Cell::new(0);
        let find_count = Cell::new(0);
        let on_dock = Cell::new(true);
        let find = || {
            find_count.set(find_count.get() + 1);
            Ok(on_dock.get().then_some(()))
        };
        let act = |_: &(), action: &str| {
            assert_eq!(action, "AXPress");
            posted.set(posted.get() + 1);
            AXError::Success
        };
        assert!(resolve(false, Button::Left, &[], false, || Ok(()), find, act).unwrap());
        assert_eq!(posted.get(), 0);
        on_dock.set(still_on_dock);
        let down = Down::take_ready(
            &mut engine.dock_down,
            PointerPhase::Up,
            Point { x: 45.0, y: 50.0 },
            Button::Left,
            false,
        )
        .unwrap()
        .unwrap();
        assert_original_press(&down);
        assert_eq!(
            resolve(false, down.button, &[], true, || Ok(()), find, act).unwrap(),
            still_on_dock
        );
        assert_eq!(find_count.get(), 2);
        assert_eq!(posted.get(), u8::from(still_on_dock));
        assert!(engine.dock_down.is_none());
    }
}

#[test]
fn releasing_a_deferred_press_posts_nothing() {
    let mut engine = deferred_engine();
    assert!(engine.held_input());
    engine.release().unwrap();
    assert!(engine.dock_down.is_none());
    assert!(!engine.held_input());
    assert_eq!(engine.posted, 0);
}

#[test]
fn a_dock_that_appears_after_target_verification_is_refused() {
    for press in [false, true] {
        let dock_visible = Cell::new(false);
        assert!(
            !resolve(
                true,
                Button::Left,
                &[],
                press,
                || Ok(()),
                || Ok(dock_visible.get().then_some(())),
                |_, _| panic!("an ordinary point must not press the Dock"),
            )
            .unwrap()
        );
        dock_visible.set(true);
        let error = resolve(
            true,
            Button::Left,
            &[],
            press,
            || Ok(()),
            || Ok(dock_visible.get().then_some(())),
            |_, _| panic!("a targeted request must not press the Dock"),
        )
        .unwrap_err();
        assert_eq!(error.code, Code::FocusChanged);
        assert_eq!(error.details["reason"], "point");
        assert!(!error.uncertain);
    }
}

#[test]
fn a_right_or_control_click_shows_a_dock_items_menu() {
    for (button, modifiers) in [
        (Button::Right, vec![]),
        (Button::Left, vec!["ctrl".to_owned()]),
    ] {
        let acted = Cell::new(None);
        assert!(
            resolve(
                false,
                button,
                &modifiers,
                true,
                || Ok(()),
                || Ok(Some(())),
                |_, action| {
                    acted.set(Some(action));
                    AXError::Success
                },
            )
            .unwrap()
        );
        assert_eq!(acted.get(), Some("AXShowMenu"));
    }
}

#[test]
fn dock_presses_refuse_other_buttons_and_modifiers() {
    for press in [false, true] {
        for (button, modifiers) in [
            (Button::Middle, vec![]),
            (Button::Right, vec!["ctrl".to_owned()]),
            (Button::Left, vec!["cmd".to_owned()]),
            (Button::Left, vec!["shift".to_owned()]),
            (Button::Left, vec!["alt".to_owned()]),
            (Button::Left, vec!["fn".to_owned()]),
            (Button::Left, vec!["ctrl".to_owned(), "shift".to_owned()]),
        ] {
            let error = resolve(
                false,
                button,
                &modifiers,
                press,
                || Ok(()),
                || Ok(Some(())),
                |_, _| panic!("an unsupported gesture must not press the Dock"),
            )
            .unwrap_err();
            assert_eq!(error.code, Code::Input);
            assert!(!error.uncertain);
        }
    }
}

#[test]
fn a_dock_press_checks_the_guard_after_resolving_the_item() {
    let current = Cell::new(true);
    let error = resolve(
        false,
        Button::Left,
        &[],
        true,
        || {
            if current.get() {
                Ok(())
            } else {
                Err(Error::new(Code::LeaseLost, "Control ended"))
            }
        },
        || {
            current.set(false);
            Ok(Some(()))
        },
        |_, _| panic!("a revoked controller must not press the Dock"),
    )
    .unwrap_err();
    assert_eq!(error.code, Code::LeaseLost);
}

#[test]
fn a_failed_dock_press_stays_uncertain() {
    for status in [
        AXError::CannotComplete,
        AXError::Failure,
        AXError::InvalidUIElement,
    ] {
        let error = press_result(status).unwrap_err();
        assert!(error.uncertain);
        assert_eq!(error.code, Code::Uncertain);
    }
    assert!(press_result(AXError::Success).is_ok());
}

#[test]
fn a_later_targeted_request_cannot_resume_a_deferred_dock_press() {
    let down = Down {
        point: Point { x: 40.0, y: 50.0 },
        button: Button::Left,
        modifiers: Vec::new(),
        count: 1,
    };
    assert!(
        !down
            .ready(PointerPhase::Move, down.point, Button::Left, false)
            .unwrap()
    );
    for phase in [PointerPhase::Move, PointerPhase::Up] {
        for point in [Point { x: 400.0, y: 300.0 }, down.point] {
            let error = down.ready(phase, point, Button::Left, true).unwrap_err();
            assert_eq!(error.code, Code::FocusChanged);
            assert_eq!(error.details["reason"], "point");
            assert!(!error.uncertain);
        }
    }
    assert!(
        down.ready(PointerPhase::Up, down.point, Button::Left, false)
            .unwrap()
    );
}

#[test]
fn a_viewer_dock_press_waits_for_release_and_movement_returns_to_dragging() {
    let down = Down {
        point: Point { x: 40.0, y: 50.0 },
        button: Button::Left,
        modifiers: Vec::new(),
        count: 1,
    };
    assert!(
        !down
            .ready(PointerPhase::Move, down.point, Button::Left, false)
            .unwrap()
    );
    assert!(
        down.ready(PointerPhase::Up, down.point, Button::Left, false)
            .unwrap()
    );
    assert!(
        down.ready(
            PointerPhase::Move,
            Point { x: 45.0, y: 50.0 },
            Button::Left,
            false
        )
        .unwrap()
    );
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        assert!(down.ready(phase, down.point, Button::Right, false).is_err());
    }
}
