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

impl Engine {
    async fn dock_click(&mut self, point: Point, press: bool) -> Result<bool> {
        let display = Bounds::display(self.epoch.width, self.epoch.height);
        let guard = self.preparation_guard();
        let on_dock = tokio::task::spawn_blocking(move || -> Result<bool> {
            guard()?;
            let Some(item) = ax::dock_item(display, point.x, point.y)? else {
                return Ok(false);
            };
            if press {
                guard()?;
                press_result(ax::act(&item, &CFString::from_static_str("AXPress")))?;
            }
            Ok(true)
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
                if !self.dock_click(point, true).await? {
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
            && self.dock_click(point, false).await?
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
