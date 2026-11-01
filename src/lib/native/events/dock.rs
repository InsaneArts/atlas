//! Dock taps use AXPress; a viewer's held press is deferred until it is
//! known to be a tap or a drag. Deferred presses have posted nothing.
use super::{Button, Engine, Point, PointerPhase};
use crate::{
    error::{Code, Error, Result},
    native::ax,
    screen::geometry::Bounds,
};
use objc2_application_services::AXError;
use objc2_core_foundation::CFString;

pub(super) struct Down {
    point: Point,
    button: Button,
    modifiers: Vec<String>,
    count: u8,
}

impl Down {
    fn take_ready(
        down: &mut Option<Self>,
        phase: PointerPhase,
        point: Point,
        button: Button,
        targeted: bool,
    ) -> Result<Option<Self>> {
        // Readiness only borrows the press. Nothing can change the slot before
        // the ready arm takes it; refusals and unchanged moves leave it intact.
        match down {
            Some(saved) if saved.ready(phase, point, button, targeted)? => Ok(down.take()),
            _ => Ok(None),
        }
    }

    fn ready(
        &self,
        phase: PointerPhase,
        point: Point,
        button: Button,
        targeted: bool,
    ) -> Result<bool> {
        if targeted {
            return Err(Error::new(
                Code::FocusChanged,
                "A deferred Dock press cannot continue in the app; no input was sent",
            )
            .with("reason", serde_json::json!("point")));
        }
        match phase {
            PointerPhase::Down => Err(Error::new(Code::Input, "A mouse button is already held")),
            PointerPhase::Up if button != self.button => {
                Err(Error::new(Code::Input, "Mouse button mismatch"))
            }
            PointerPhase::Move => Ok(point != self.point),
            PointerPhase::Up => Ok(true),
        }
    }
}

impl Engine {
    pub(super) async fn click(
        &mut self,
        point: Point,
        button: Button,
        modifiers: &[String],
        count: u8,
    ) -> Result<()> {
        if !self.dock_click(point, button, modifiers, true).await? {
            self.post_click(point, button, modifiers, count).await?;
        }
        Ok(())
    }

    async fn dock_click(
        &mut self,
        point: Point,
        button: Button,
        modifiers: &[String],
        press: bool,
    ) -> Result<bool> {
        let display = Bounds::display(self.epoch.width, self.epoch.height);
        let guard = self.preparation_guard();
        let targeted = self.target.is_some();
        let modifiers = modifiers.to_vec();
        let on_dock = tokio::task::spawn_blocking(move || -> Result<bool> {
            resolve(
                targeted,
                button,
                &modifiers,
                press,
                guard,
                || ax::dock_item(display, point.x, point.y),
                |item| ax::act(item, &CFString::from_static_str("AXPress")),
            )
        })
        .await
        .map_err(|_| Error::new(Code::Uncertain, "Dock press outcome unknown").uncertain())??;
        if on_dock && press {
            self.posted += 1;
            self.dock_pressed = true;
        }
        self.preparation_guard()()?;
        Ok(on_dock)
    }

    async fn post_click(
        &mut self,
        point: Point,
        button: Button,
        modifiers: &[String],
        count: u8,
    ) -> Result<()> {
        self.post_pointer(PointerPhase::Down, point, button, modifiers, count)
            .await?;
        self.post_pointer(PointerPhase::Up, point, button, modifiers, count)
            .await
    }

    pub(super) async fn pointer(
        &mut self,
        phase: PointerPhase,
        point: Point,
        button: Button,
        modifiers: &[String],
        count: u8,
    ) -> Result<()> {
        if self.dock_down.is_some() {
            let Some(down) = Down::take_ready(
                &mut self.dock_down,
                phase,
                point,
                button,
                self.target.is_some(),
            )?
            else {
                return Ok(());
            };
            if matches!(phase, PointerPhase::Up) {
                // Resolve again: the Dock may have moved or magnified since down.
                if !self.dock_click(point, down.button, modifiers, true).await? {
                    return Err(Error::new(Code::Input, "The Dock moved; no click was sent"));
                }
                return Ok(());
            }
            self.post_pointer(
                PointerPhase::Down,
                down.point,
                down.button,
                &down.modifiers,
                down.count,
            )
            .await?;
        } else if matches!(phase, PointerPhase::Down)
            && self.held.is_none()
            && self.dock_click(point, button, modifiers, false).await?
        {
            self.dock_down = Some(Down {
                point,
                button,
                modifiers: modifiers.to_vec(),
                count,
            });
            return Ok(());
        }
        self.post_pointer(phase, point, button, modifiers, count)
            .await
    }
}

fn resolve<T>(
    targeted: bool,
    button: Button,
    modifiers: &[String],
    press: bool,
    guard: impl Fn() -> Result<()>,
    find: impl FnOnce() -> Result<Option<T>>,
    act: impl FnOnce(&T) -> AXError,
) -> Result<bool> {
    guard()?;
    let Some(item) = find()? else {
        return Ok(false);
    };
    if targeted {
        return Err(Error::new(
            Code::FocusChanged,
            "The Dock is at that point, not the app; no click was sent",
        )
        .with("reason", serde_json::json!("point")));
    }
    if button != Button::Left || !modifiers.is_empty() {
        return Err(Error::new(
            Code::Input,
            "Dock input supports only unmodified left clicks; no click was sent",
        ));
    }
    if press {
        guard()?;
        press_result(act(&item))?;
    }
    Ok(true)
}

fn press_result(status: AXError) -> Result<()> {
    if status == AXError::Success {
        Ok(())
    } else {
        Err(Error::new(Code::Uncertain, "The Dock did not acknowledge the press").uncertain())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn deferred_engine() -> Engine {
        let mut engine = Engine::idle(crate::session::InputEpoch {
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
        engine.target = Some(crate::process::Instance::new(123, [4, 5]));
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
            let act = |_: &()| {
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
                    |_| panic!("an ordinary point must not press the Dock"),
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
                |_| panic!("a targeted request must not press the Dock"),
            )
            .unwrap_err();
            assert_eq!(error.code, Code::FocusChanged);
            assert_eq!(error.details["reason"], "point");
            assert!(!error.uncertain);
        }
    }

    #[test]
    fn dock_presses_refuse_other_buttons_and_modifiers() {
        for press in [false, true] {
            for (button, modifiers) in [
                (Button::Right, vec![]),
                (Button::Middle, vec![]),
                (Button::Left, vec!["ctrl".to_owned()]),
                (Button::Left, vec!["cmd".to_owned()]),
                (Button::Left, vec!["shift".to_owned()]),
                (Button::Left, vec!["alt".to_owned()]),
                (Button::Left, vec!["fn".to_owned()]),
            ] {
                let error = resolve(
                    false,
                    button,
                    &modifiers,
                    press,
                    || Ok(()),
                    || Ok(Some(())),
                    |_| panic!("an unsupported gesture must not press the Dock"),
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
            |_| panic!("a revoked controller must not press the Dock"),
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
}
