//! Dock taps use AXPress; a viewer's held press is deferred until it is
//! known to be a tap or a drag. Deferred presses have posted nothing.
use super::{Button, Engine, Point, PointerPhase};
use crate::{
    error::{Code, Error, Result},
    native::{ax, windows::Bounds},
};

pub(super) struct Down {
    point: Point,
    button: Button,
    modifiers: Vec<String>,
    count: u8,
}

#[derive(Debug, PartialEq)]
enum Continuation {
    Wait,
    Press,
    Drag,
}

impl Down {
    fn continuation(
        &self,
        phase: PointerPhase,
        point: Point,
        button: Button,
    ) -> Result<Continuation> {
        match phase {
            PointerPhase::Down => Err(Error::new(
                Code::Input,
                "Release the held mouse button first",
            )),
            PointerPhase::Up if button != self.button => Err(Error::new(
                Code::Input,
                "Mouse button does not match the active gesture",
            )),
            PointerPhase::Up => Ok(Continuation::Press),
            PointerPhase::Move if point == self.point => Ok(Continuation::Wait),
            PointerPhase::Move => Ok(Continuation::Drag),
        }
    }
}

impl Engine {
    pub(super) async fn dock_click(&mut self, point: Point, press: bool) -> Result<bool> {
        self.check()?;
        let display = Bounds::display(self.epoch.width, self.epoch.height);
        let guard = self.preparation_guard();
        let on_dock = tokio::task::spawn_blocking(move || {
            ax::dock_click(display, point.x, point.y, press, guard)
        })
        .await
        .unwrap_or_else(|_| {
            Err(Error::new(
                Code::Uncertain,
                "The Dock worker did not report its outcome",
            )
            .uncertain())
        })?;
        if on_dock && press {
            self.posted += 1;
        }
        self.check()?;
        drop(
            self.grant
                .as_ref()
                .ok_or_else(|| Error::new(Code::LeaseLost, "Input has no controller"))?
                .authorize()?,
        );
        Ok(on_dock)
    }

    pub(super) async fn dock_pointer(
        &mut self,
        phase: PointerPhase,
        point: Point,
        button: Button,
        modifiers: &[String],
        count: u8,
    ) -> Result<bool> {
        if let Some(down) = &self.dock_down {
            match down.continuation(phase, point, button)? {
                Continuation::Press => {
                    self.dock_down = None;
                    // Resolve again: the Dock may have moved or magnified since down.
                    if !self.dock_click(point, true).await? {
                        return Err(Error::new(Code::Input, "The Dock moved; no click was sent"));
                    }
                    return Ok(true);
                }
                Continuation::Wait => return Ok(true),
                Continuation::Drag => {
                    let down = self.dock_down.take().expect("the deferred press exists");
                    self.pointer(
                        PointerPhase::Down,
                        down.point,
                        down.button,
                        &down.modifiers,
                        down.count,
                    )
                    .await?;
                }
            }
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
            return Ok(true);
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_viewer_dock_press_waits_for_release_and_movement_returns_to_dragging() {
        let down = Down {
            point: Point { x: 40.0, y: 50.0 },
            button: Button::Left,
            modifiers: Vec::new(),
            count: 1,
        };
        assert_eq!(
            down.continuation(PointerPhase::Move, down.point, Button::Left)
                .unwrap(),
            Continuation::Wait
        );
        assert_eq!(
            down.continuation(PointerPhase::Up, down.point, Button::Left)
                .unwrap(),
            Continuation::Press
        );
        assert_eq!(
            down.continuation(PointerPhase::Move, Point { x: 45.0, y: 50.0 }, Button::Left)
                .unwrap(),
            Continuation::Drag
        );
        assert!(
            down.continuation(PointerPhase::Up, down.point, Button::Right)
                .is_err()
        );
        assert!(
            down.continuation(PointerPhase::Down, down.point, Button::Left)
                .is_err()
        );
    }
}
