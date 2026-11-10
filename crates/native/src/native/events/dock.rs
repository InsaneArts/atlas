//! Dock taps use AXPress; a viewer's held press is deferred until it is
//! known to be a tap or a drag. Deferred presses have posted nothing.
use super::{Button, Engine, Point, PointerPhase};
use crate::native::{ax, focus::Reason};
use atlas_core::{
    actions::CLICK_GAP,
    error::{Code, Error, Result},
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
            return Err(off_target(
                "A deferred Dock press cannot continue in the app; no input was sent",
            ));
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
        if self.held.is_some() || self.dock_down.is_some() {
            return Err(Error::new(Code::Input, "A mouse button is already held"));
        }
        for click in 1..=count {
            if click > 1 {
                crate::native::service::recheck_event(self)?;
            }
            if !self.dock_click(point, button, modifiers, true).await? {
                self.post_pointer(PointerPhase::Down, point, button, modifiers, click)
                    .await?;
                self.post_pointer(PointerPhase::Up, point, button, modifiers, click)
                    .await?;
            }
            if click < count {
                self.delay(CLICK_GAP).await?;
            }
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
        #[cfg(test)]
        if self.tap.is_some() {
            return Ok(false);
        }

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
        .map_err(|cause| {
            Error::new(Code::Uncertain, "Dock press outcome unknown")
                .uncertain()
                .with_source(cause)
        })??;
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
            match phase {
                PointerPhase::Up => {
                    // Resolve again: the Dock may have moved or magnified since down.
                    if !self.dock_click(point, down.button, modifiers, true).await? {
                        return Err(Error::new(Code::Input, "The Dock moved; no click was sent"));
                    }
                    return Ok(());
                }
                PointerPhase::Down | PointerPhase::Move => {}
            }
            self.post_pointer(
                PointerPhase::Down,
                down.point,
                down.button,
                &down.modifiers,
                down.count,
            )
            .await?;
        } else {
            let on_dock = match phase {
                PointerPhase::Down => {
                    self.held.is_none() && self.dock_click(point, button, modifiers, false).await?
                }
                PointerPhase::Move | PointerPhase::Up => false,
            };
            if on_dock {
                self.dock_down = Some(Down {
                    point,
                    button,
                    modifiers: modifiers.to_vec(),
                    count,
                });
                return Ok(());
            }
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
        return Err(off_target(
            "The Dock is at that point, not the app; no click was sent",
        ));
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

fn off_target(message: &str) -> Error {
    Error::new(Code::FocusChanged, message).with("reason", serde_json::json!(Reason::Point))
}

fn press_result(status: AXError) -> Result<()> {
    if status == AXError::Success {
        Ok(())
    } else {
        Err(
            Error::new(Code::Uncertain, "The Dock did not acknowledge the press")
                .uncertain()
                .with_source(ax::Status(status)),
        )
    }
}

#[cfg(test)]
#[path = "dock_tests.rs"]
mod tests;
