//! CoreGraphics input belongs to one private event source in the helper session.
use super::{
    journal::{self, Held, Journal},
    lease::Grant,
};
use crate::{
    actions::{Action, Button, PointerPhase},
    error::{Error, Result},
    native::session_info,
    session::InputEpoch,
};
use objc2_application_services::AXIsProcessTrusted;
use objc2_core_foundation::{CFRetained, CGPoint as Point};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation,
    CGEventType, CGMouseButton, CGScrollEventUnit,
};
use std::{path::PathBuf, time::Duration};
use unicode_segmentation::UnicodeSegmentation;

/// The only place events go: the helper login's own session event tap, never
/// the global HID tap.
const TAP: CGEventTapLocation = CGEventTapLocation::SessionEventTap;

fn created(event: Option<CFRetained<CGEvent>>) -> Result<CFRetained<CGEvent>> {
    event.ok_or_else(|| Error::new("native", "Could not create a native event"))
}

pub struct Engine {
    pub epoch: InputEpoch,
    pub grant: Option<Grant>,
    journal: Option<Journal>,
    journal_dirty: bool,
    source: CFRetained<CGEventSource>,
    held: Option<Button>,
    held_keys: Vec<u16>,
    position: Point,
    click_count: u8,
    mouse_number: i64,
    drag_started: bool,
    flags: u64,
}
// SAFETY: Engine is exclusively accessed under the service mutex; no event or
// source reference escapes it. CoreGraphics posting does not require AppKit's main thread.
#[allow(unsafe_code)]
unsafe impl Send for Engine {}

impl Engine {
    pub fn new(uid: u32) -> Result<Self> {
        let source = CGEventSource::new(CGEventSourceStateID::Private)
            .ok_or_else(|| Error::new("native", "Could not create the event source"))?;
        let engine = Self {
            epoch: InputEpoch::from_proof(&session_info()?, std::process::id())?,
            grant: None,
            journal: None,
            journal_dirty: false,
            source,
            held: None,
            held_keys: Vec::new(),
            position: Point { x: 0.0, y: 0.0 },
            click_count: 1,
            mouse_number: 0,
            drag_started: false,
            flags: 0,
        };
        if engine.epoch.uid != uid {
            return Err(Error::new(
                "identity",
                "Input service UID differs from its configuration",
            ));
        }
        engine.check()?;
        Ok(engine)
    }
    #[allow(unsafe_code)] // FFI: Accessibility trust query.
    pub fn trusted() -> bool {
        // SAFETY: no arguments; only reads this process's consent.
        unsafe { AXIsProcessTrusted() }
    }
    pub fn check(&self) -> Result<()> {
        let current = InputEpoch::from_proof(&session_info()?, std::process::id())?;
        if current != self.epoch {
            return Err(Error::new(
                "identity",
                "Native input requires the configured hidden graphical session",
            ));
        }
        if !Self::trusted() {
            return Err(Error::new(
                "permission",
                "The input process does not have Accessibility permission",
            ));
        }
        // Without it macOS drops posted events silently: refuse instead.
        if !objc2_core_graphics::CGPreflightPostEventAccess() {
            return Err(Error::new(
                "permission",
                "The input process may not post events (Accessibility)",
            ));
        }
        Ok(())
    }
    pub fn recover(&mut self, path: PathBuf) -> Result<()> {
        if let Some(held) = journal::load(&path, &self.epoch)? {
            self.held = held.button;
            self.held_keys = held.keys;
            self.flags = held.flags;
            self.click_count = held.count;
            self.mouse_number = held.mouse_number;
            self.position = Point {
                x: held.x.clamp(0.0, self.epoch.width - 1.0),
                y: held.y.clamp(0.0, self.epoch.height - 1.0),
            };
        }
        self.journal = Some(Journal::new(path));
        self.journal_dirty = true;
        self.release()
    }
    pub fn rebind(&mut self, current: InputEpoch) -> Result<()> {
        if current.uid != self.epoch.uid
            || current.security_session_id != self.epoch.security_session_id
        {
            return Err(Error::new("identity", "Input recovery cannot cross logins"));
        }
        self.epoch = current;
        self.journal_dirty = true;
        self.position.x = self.position.x.clamp(0.0, self.epoch.width - 1.0);
        self.position.y = self.position.y.clamp(0.0, self.epoch.height - 1.0);
        self.release()
    }
    fn persist(&mut self, point: Point, flags: u64) -> Result<()> {
        if let Some(journal) = &mut self.journal {
            journal.write(Held {
                epoch: self.epoch.clone(),
                button: self.held,
                keys: self.held_keys.clone(),
                x: point.x,
                y: point.y,
                count: self.click_count,
                mouse_number: self.mouse_number,
                flags,
            })?;
        }
        Ok(())
    }

    fn commit(&mut self) -> Result<()> {
        self.persist(self.position, self.flags)?;
        self.journal_dirty = false;
        Ok(())
    }

    fn post(&mut self, event: CFRetained<CGEvent>, flags: u64, point: Point) -> Result<()> {
        // Persist conservatively before the final gate. Slow storage must not
        // hold cancellation or widen the gap after the hidden-session check.
        // Until this event is posted, both the previous and requested modifier
        // states may need releasing after a crash.
        self.flags |= flags;
        self.journal_dirty = true;
        self.persist(point, self.flags)?;
        let _authorization = self.grant.as_ref().map(Grant::authorize).transpose()?;
        self.check()?;
        CGEvent::set_flags(Some(&event), CGEventFlags(flags));
        CGEvent::post(TAP, Some(&event));
        self.flags = flags;
        Ok(())
    }
    fn mouse(
        &mut self,
        kind: u32,
        point: Point,
        button: Button,
        count: u8,
        flags: u64,
    ) -> Result<()> {
        let event = created(CGEvent::new_mouse_event(
            Some(&self.source),
            CGEventType(kind),
            point,
            CGMouseButton(button.number()),
        ))?;
        let set = |field, value| CGEvent::set_integer_value_field(Some(&event), field, value);
        // A press, its drag events, and its release share one event number.
        set(CGEventField::MouseEventNumber, self.mouse_number);
        set(CGEventField::MouseEventClickState, count.into());
        set(
            CGEventField::MouseEventDeltaX,
            (point.x.round() - self.position.x.round()) as i64,
        );
        set(
            CGEventField::MouseEventDeltaY,
            (point.y.round() - self.position.y.round()) as i64,
        );
        self.post(event, flags, point)
    }
    pub fn release(&mut self) -> Result<()> {
        self.grant = None;
        if !self.held_input() {
            return if self.journal_dirty {
                self.commit()
            } else {
                Ok(())
            };
        }
        if let Some(button) = self.held {
            self.mouse(button.up(), self.position, button, self.click_count, 0)?;
            self.held = None;
            self.drag_started = false;
        }
        while let Some(&code) = self.held_keys.last() {
            self.key_event(code, false, 0, None)?;
            self.held_keys.pop();
        }
        self.release_modifiers()?;
        self.commit()
    }
    pub fn held_input(&self) -> bool {
        self.held.is_some() || !self.held_keys.is_empty() || self.flags != 0
    }
    async fn pointer(
        &mut self,
        phase: PointerPhase,
        point: Point,
        button: Button,
        modifiers: &[String],
        count: u8,
    ) -> Result<()> {
        let flags = modifier_flags(modifiers);
        match phase {
            PointerPhase::Down => {
                if self.held.is_some() {
                    return Err(Error::new("input", "Release the held mouse button first"));
                }
                self.mouse(5, point, button, count, flags)?;
                self.position = point;
                self.held = Some(button);
                self.mouse_number = self.mouse_number.checked_add(1).unwrap_or(1);
                self.drag_started = false;
                self.click_count = count;
                self.mouse(button.down(), point, button, count, flags)?;
            }
            PointerPhase::Move => {
                let active = self.held.unwrap_or(button);
                if self.held.is_some()
                    && !self.drag_started
                    && (point.x != self.position.x || point.y != self.position.y)
                {
                    // AppKit establishes native window tracking at the first
                    // dragged location. Bound that initial step to one point
                    // so a sparse viewer stream does not lose its first jump.
                    // A stationary click never enters this path.
                    let anchor = Point {
                        x: self.position.x + (point.x - self.position.x).clamp(-1.0, 1.0),
                        y: self.position.y + (point.y - self.position.y).clamp(-1.0, 1.0),
                    };
                    self.mouse(active.dragged(), anchor, active, self.click_count, flags)?;
                    self.position = anchor;
                    self.drag_started = true;
                    self.commit()?;
                    if anchor.x == point.x && anchor.y == point.y {
                        return Ok(());
                    }
                    self.delay(Duration::from_millis(10)).await?;
                }
                self.mouse(
                    if self.held.is_some() {
                        active.dragged()
                    } else {
                        5
                    },
                    point,
                    active,
                    self.click_count,
                    flags,
                )?;
                self.position = point;
            }
            PointerPhase::Up => {
                let held = self
                    .held
                    .ok_or_else(|| Error::new("input", "No mouse button is held"))?;
                if held != button {
                    return Err(Error::new(
                        "input",
                        "Mouse button does not match the active gesture",
                    ));
                }
                self.mouse(button.up(), point, button, self.click_count, flags)?;
                self.held = None;
                self.drag_started = false;
                self.position = point;
                self.release_modifiers()?;
            }
        }
        self.commit()
    }
    #[allow(unsafe_code)] // FFI: key text is passed as a UTF-16 pointer.
    fn key_event(&mut self, code: u16, down: bool, flags: u64, text: Option<&[u16]>) -> Result<()> {
        let event = created(CGEvent::new_keyboard_event(Some(&self.source), code, down))?;
        if let Some(text) = text {
            // SAFETY: the pointer and length describe the live UTF-16 slice.
            unsafe {
                CGEvent::keyboard_set_unicode_string(Some(&event), text.len() as _, text.as_ptr())
            };
        }
        self.post(event, flags, self.position)
    }
    fn release_modifiers(&mut self) -> Result<()> {
        if self.flags != 0 {
            // Finish the chord in CoreGraphics too. Viewer key-up events do
            // not represent remote holds, so a completed gesture must be clean.
            for code in [55, 56, 58, 59, 63] {
                self.key_event(code, false, 0, None)?;
            }
        }
        Ok(())
    }
    fn key(&mut self, code: u16, flags: u64, text: Option<&[u16]>) -> Result<()> {
        self.held_keys.push(code);
        self.key_event(code, true, flags, text)?;
        // A key action is a complete chord. Its modifiers must not remain held
        // while the viewer keeps control and waits for the next action.
        self.key_event(code, false, 0, text)?;
        self.held_keys.pop();
        self.commit()
    }
    async fn delay(&mut self, duration: Duration) -> Result<()> {
        let end = tokio::time::Instant::now() + duration;
        loop {
            self.check()?;
            if let Some(grant) = &self.grant {
                drop(grant.authorize()?);
            }
            let now = tokio::time::Instant::now();
            if now >= end {
                return Ok(());
            }
            tokio::time::sleep((end - now).min(Duration::from_millis(10))).await;
        }
    }
    pub async fn perform(&mut self, action: Action) -> Result<()> {
        self.check()?;
        match action {
            Action::Pointer {
                phase,
                x,
                y,
                button,
                modifiers,
                count,
            } => {
                self.pointer(phase, Point { x, y }, button, &modifiers, count)
                    .await?
            }
            Action::Move { x, y } => {
                self.pointer(PointerPhase::Move, Point { x, y }, Button::Left, &[], 1)
                    .await?
            }
            Action::Click {
                x,
                y,
                button,
                count,
                modifiers,
            } => {
                for click in 1..=count {
                    self.pointer(
                        PointerPhase::Down,
                        Point { x, y },
                        button,
                        &modifiers,
                        click,
                    )
                    .await?;
                    self.pointer(PointerPhase::Up, Point { x, y }, button, &modifiers, click)
                        .await?;
                    if click < count {
                        self.delay(Duration::from_millis(60)).await?;
                    }
                }
            }
            Action::Drag {
                from_x,
                from_y,
                to_x,
                to_y,
                button,
            } => {
                self.pointer(
                    PointerPhase::Down,
                    Point {
                        x: from_x,
                        y: from_y,
                    },
                    button,
                    &[],
                    1,
                )
                .await?;
                self.delay(Duration::from_millis(80)).await?;
                for step in 1..=24 {
                    let t = f64::from(step) / 24.0;
                    self.pointer(
                        PointerPhase::Move,
                        Point {
                            x: from_x + (to_x - from_x) * t,
                            y: from_y + (to_y - from_y) * t,
                        },
                        button,
                        &[],
                        1,
                    )
                    .await?;
                    self.delay(Duration::from_millis(16)).await?;
                }
                self.pointer(PointerPhase::Up, Point { x: to_x, y: to_y }, button, &[], 1)
                    .await?;
            }
            Action::Scroll { x, y, dx, dy } => {
                self.pointer(PointerPhase::Move, Point { x, y }, Button::Left, &[], 1)
                    .await?;
                let event = created(CGEvent::new_scroll_wheel_event2(
                    Some(&self.source),
                    CGScrollEventUnit::Line,
                    2,
                    dy,
                    dx,
                    0,
                ))?;
                self.post(event, 0, self.position)?;
            }
            Action::Type { text } => {
                for character in text.graphemes(true) {
                    let units: Vec<u16> = character.encode_utf16().collect();
                    self.key(0, 0, Some(&units))?;
                    self.delay(Duration::from_millis(2)).await?;
                }
            }
            Action::Key { key } => {
                let lower = key.to_lowercase();
                let mut parts: Vec<_> = lower.split('+').map(str::to_owned).collect();
                let mut key = parts.pop().unwrap_or_default();
                if key.is_empty() {
                    key = "=".into();
                    parts.push("shift".into());
                }
                self.key(
                    keycode(&key).ok_or_else(|| Error::new("input", "Unsupported key"))?,
                    modifier_flags(&parts),
                    None,
                )?;
            }
            Action::Wait { ms } => self.delay(Duration::from_millis(ms.into())).await?,
        }
        Ok(())
    }
}
impl Button {
    fn number(self) -> u32 {
        match self {
            Self::Left => 0,
            Self::Right => 1,
            Self::Middle => 2,
        }
    }
    fn down(self) -> u32 {
        match self {
            Self::Left => 1,
            Self::Right => 3,
            Self::Middle => 25,
        }
    }
    fn up(self) -> u32 {
        match self {
            Self::Left => 2,
            Self::Right => 4,
            Self::Middle => 26,
        }
    }
    fn dragged(self) -> u32 {
        match self {
            Self::Left => 6,
            Self::Right => 7,
            Self::Middle => 27,
        }
    }
}
fn modifier_flags(modifiers: &[String]) -> u64 {
    modifiers.iter().fold(0, |flags, m| {
        flags
            | match m.as_str() {
                "cmd" | "command" => 1 << 20,
                "ctrl" | "control" => 1 << 18,
                "shift" => 1 << 17,
                "alt" | "opt" | "option" => 1 << 19,
                "fn" => 1 << 23,
                _ => 0,
            }
    })
}
fn keycode(key: &str) -> Option<u16> {
    Some(match key {
        "a" => 0,
        "s" => 1,
        "d" => 2,
        "f" => 3,
        "h" => 4,
        "g" => 5,
        "z" => 6,
        "x" => 7,
        "c" => 8,
        "v" => 9,
        "b" => 11,
        "q" => 12,
        "w" => 13,
        "e" => 14,
        "r" => 15,
        "y" => 16,
        "t" => 17,
        "1" => 18,
        "2" => 19,
        "3" => 20,
        "4" => 21,
        "6" => 22,
        "5" => 23,
        "=" => 24,
        "9" => 25,
        "7" => 26,
        "-" => 27,
        "8" => 28,
        "0" => 29,
        "]" => 30,
        "o" => 31,
        "u" => 32,
        "[" => 33,
        "i" => 34,
        "p" => 35,
        "return" | "enter" => 36,
        "l" => 37,
        "j" => 38,
        "'" => 39,
        "k" => 40,
        ";" => 41,
        "\\" => 42,
        "," => 43,
        "/" => 44,
        "n" => 45,
        "m" => 46,
        "." => 47,
        "tab" => 48,
        "space" => 49,
        "`" => 50,
        "backspace" | "delete" => 51,
        "escape" | "esc" => 53,
        "f1" => 122,
        "f2" => 120,
        "f3" => 99,
        "f4" => 118,
        "f5" => 96,
        "f6" => 97,
        "f7" => 98,
        "f8" => 100,
        "f9" => 101,
        "f10" => 109,
        "f11" => 103,
        "f12" => 111,
        "home" => 115,
        "pageup" => 116,
        "forwarddelete" => 117,
        "end" => 119,
        "pagedown" => 121,
        "left" => 123,
        "right" => 124,
        "down" => 125,
        "up" => 126,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
