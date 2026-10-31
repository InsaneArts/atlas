//! Dock taps use AXPress; a viewer's held press is deferred until it is
//! known to be a tap or a drag. Deferred presses have posted nothing.
use super::{Button, Engine, Point, PointerPhase};
use crate::{
    error::{Code, Error, Result},
    native::{ax, windows::Bounds},
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
    fn ready(&self, phase: PointerPhase, point: Point, button: Button) -> Result<bool> {
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

pub(super) trait Click {
    async fn dock_click(
        &mut self,
        point: Point,
        button: Button,
        modifiers: &[String],
        press: bool,
    ) -> Result<bool>;

    async fn post_click(
        &mut self,
        point: Point,
        button: Button,
        modifiers: &[String],
        count: u8,
    ) -> Result<()>;

    async fn click(
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
}

impl Click for Engine {
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
}

impl Engine {
    pub(super) async fn pointer(
        &mut self,
        phase: PointerPhase,
        point: Point,
        button: Button,
        modifiers: &[String],
        count: u8,
    ) -> Result<()> {
        if let Some(down) = &self.dock_down {
            if !down.ready(phase, point, button)? {
                return Ok(());
            }
            let down = self.dock_down.take().expect("the deferred down exists");
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
        .with("reason", "point"));
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

    struct Mouse {
        on_dock: bool,
        lookups: usize,
        presses: usize,
        posts: usize,
    }

    impl Click for Mouse {
        async fn dock_click(
            &mut self,
            _: Point,
            button: Button,
            modifiers: &[String],
            press: bool,
        ) -> Result<bool> {
            resolve(
                false,
                button,
                modifiers,
                press,
                || Ok(()),
                || {
                    self.lookups += 1;
                    Ok(self.on_dock.then_some(()))
                },
                |_| {
                    self.presses += 1;
                    AXError::Success
                },
            )
        }

        async fn post_click(&mut self, _: Point, _: Button, _: &[String], _: u8) -> Result<()> {
            self.posts += 2;
            Ok(())
        }
    }

    #[tokio::test]
    async fn a_complete_click_resolves_the_dock_once() {
        for on_dock in [false, true] {
            let mut mouse = Mouse {
                on_dock,
                lookups: 0,
                presses: 0,
                posts: 0,
            };
            mouse
                .click(Point { x: 40.0, y: 50.0 }, Button::Left, &[], 1)
                .await
                .unwrap();
            assert_eq!(mouse.lookups, 1);
            assert_eq!(mouse.presses, usize::from(on_dock));
            assert_eq!(mouse.posts, if on_dock { 0 } else { 2 });
        }
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
    fn a_viewer_dock_press_waits_for_release_and_movement_returns_to_dragging() {
        let down = Down {
            point: Point { x: 40.0, y: 50.0 },
            button: Button::Left,
            modifiers: Vec::new(),
            count: 1,
        };
        assert!(
            !down
                .ready(PointerPhase::Move, down.point, Button::Left)
                .unwrap()
        );
        assert!(
            down.ready(PointerPhase::Up, down.point, Button::Left)
                .unwrap()
        );
        assert!(
            down.ready(PointerPhase::Move, Point { x: 45.0, y: 50.0 }, Button::Left)
                .unwrap()
        );
        for phase in [PointerPhase::Down, PointerPhase::Up] {
            assert!(down.ready(phase, down.point, Button::Right).is_err());
        }
    }
}
