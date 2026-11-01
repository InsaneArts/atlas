//! CoreGraphics input belongs to one private event source in the helper session.
use super::{
    journal::{self, Held, Journal},
    lease::Grant,
};
use crate::{
    actions::{
        Action, Button, CLICK_GAP, COMMAND, CONTROL, DRAG_ANCHOR, DRAG_HOLD, DRAG_START, DRAG_STEP,
        DRAG_STEPS, FUNCTION, OPTION, PointerPhase, SCROLL_SETTLE, SHIFT, TYPE_GAP, modifier_flags,
        parse_key,
    },
    error::{Code, Error, Result},
    native::{accessibility_trusted, session_info},
    session::InputEpoch,
};
use objc2_core_foundation::{CFRetained, CGPoint as Point};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation,
    CGEventType, CGMouseButton, CGScrollEventUnit,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use unicode_segmentation::UnicodeSegmentation;

mod dock;

/// The only place events go: the helper login's own session event tap, never
/// the global HID tap.
const TAP: CGEventTapLocation = CGEventTapLocation::SessionEventTap;

/// How long a confirmed permission to post events holds before macOS is
/// asked again. The question is a round trip to the consent service of a
/// few milliseconds, which asked for every event cost about half of a
/// keystroke's time (typing ran at some 70 characters a second, now 150);
/// consent changes by the person's hand in System Settings, never within
/// a keystroke.
const POST_ACCESS_TTL: Duration = Duration::from_millis(250);

fn created(event: Option<CFRetained<CGEvent>>) -> Result<CFRetained<CGEvent>> {
    event.ok_or_else(|| Error::new(Code::Native, "Could not create a native event"))
}

/// The modifier keys a finished gesture releases, each with its flag:
/// command, shift, option, control and function.
const MODIFIER_KEYS: [(u16, u64); 5] = [
    (55, COMMAND),
    (56, SHIFT),
    (58, OPTION),
    (59, CONTROL),
    (63, FUNCTION),
];

/// Release each held modifier in `held` through `post`, given its key code
/// and the flags still held once it is up. A modifier's flag is cleared only
/// once its own release has posted, so after a failure the ones not yet
/// released stay held and the next release tries them again.
fn release_each(held: &mut u64, mut post: impl FnMut(u16, u64) -> Result<()>) -> Result<()> {
    for (code, mask) in MODIFIER_KEYS {
        if *held & mask == 0 {
            continue;
        }

        post(code, *held & !mask)?;
        *held &= !mask;
    }

    // Every modifier key is up, so nothing the flags still carry is held.
    *held = 0;
    Ok(())
}

/// The engine's private event source, the one field that is not `Send` by
/// itself.
struct Source(CFRetained<CGEventSource>);

#[allow(
    unsafe_code,
    reason = "FFI: CGEventSource is not marked Send by its binding"
)]
// SAFETY: a CGEventSource is a CoreFoundation object: its retain and release
// are thread-safe, and creating events from it is not tied to the thread that
// made it (CoreGraphics posting needs no main thread). The engine holds the
// only reference and is used only under the service's mutex, so the source
// is never used from two threads at once.
unsafe impl Send for Source {}

pub(super) struct Engine {
    pub epoch: InputEpoch,
    pub grant: Option<Grant>,
    /// The process resolved for this request, retained through the last post.
    pub target: Option<crate::process::Instance>,
    journal: Option<Journal>,
    journal_dirty: bool,
    /// Events posted or AX presses acknowledged so far: distinguishes input
    /// that already changed something from input that changed nothing.
    pub(crate) posted: u64,
    source: Source,
    held: Option<Button>,
    dock_down: Option<dock::Down>,
    pub(super) dock_pressed: bool,
    held_keys: Vec<u16>,
    position: Point,
    click_count: u8,
    mouse_number: i64,
    drag_started: bool,
    flags: u64,
    /// When macOS last confirmed that this process may post events; none
    /// until it has, or after it said no.
    post_access: Option<Instant>,
}

impl Engine {
    pub(super) fn new(uid: u32) -> Result<Self> {
        let source = CGEventSource::new(CGEventSourceStateID::Private)
            .map(Source)
            .ok_or_else(|| Error::new(Code::Native, "Could not create the event source"))?;
        let mut engine = Self {
            epoch: InputEpoch::from_proof(&session_info()?, std::process::id())?,
            grant: None,
            target: None,
            journal: None,
            journal_dirty: false,
            posted: 0,
            source,
            held: None,
            dock_down: None,
            dock_pressed: false,
            held_keys: Vec::new(),
            position: Point { x: 0.0, y: 0.0 },
            click_count: 1,
            mouse_number: 0,
            drag_started: false,
            flags: 0,
            post_access: None,
        };

        if engine.epoch.uid != uid {
            return Err(Error::new(
                Code::Identity,
                "Input service UID differs from its configuration",
            ));
        }

        engine.check()?;
        Ok(engine)
    }

    /// Whether this process may use Accessibility.
    pub(super) fn trusted() -> bool {
        accessibility_trusted()
    }

    /// An independent proof for the AX worker. No gate lock survives this
    /// callback, and every call reads the exact session and permission anew.
    pub(super) fn preparation_guard(&self) -> impl Fn() -> Result<()> + Send + 'static {
        let epoch = self.epoch.clone();
        let grant = self.grant.clone();
        let target = self.target;
        move || {
            let grant = grant
                .as_ref()
                .ok_or_else(|| Error::new(Code::LeaseLost, "Input has no controller"))?;
            drop(grant.authorize()?);
            Self::check_epoch(&epoch)?;
            super::ax::check_application(target)?;
            if !objc2_core_graphics::CGPreflightPostEventAccess() {
                return Err(Error::new(
                    Code::Permission,
                    "The input process may not post events (Accessibility)",
                ));
            }
            drop(grant.authorize()?);
            Ok(())
        }
    }

    fn check_epoch(epoch: &InputEpoch) -> Result<()> {
        let current = InputEpoch::from_proof(&session_info()?, std::process::id())?;
        if &current != epoch {
            return Err(Error::new(
                Code::Identity,
                "Native input requires the configured hidden graphical session",
            ));
        }

        if !Self::trusted() {
            return Err(Error::new(
                Code::Permission,
                "The input process does not have Accessibility permission",
            ));
        }

        Ok(())
    }

    pub(super) fn check(&mut self) -> Result<()> {
        Self::check_epoch(&self.epoch)?;

        // Without it macOS drops posted events silently: refuse instead.
        // The answer is trusted for a moment (see `POST_ACCESS_TTL`).
        let confirmed = self
            .post_access
            .is_some_and(|at| at.elapsed() < POST_ACCESS_TTL);
        if !confirmed {
            if !objc2_core_graphics::CGPreflightPostEventAccess() {
                self.post_access = None;
                return Err(Error::new(
                    Code::Permission,
                    "The input process may not post events (Accessibility)",
                ));
            }

            self.post_access = Some(Instant::now());
        }

        Ok(())
    }

    pub(super) fn recover(&mut self, path: PathBuf) -> Result<()> {
        if let Some(held) = journal::load(&path, &self.epoch)? {
            self.held = held.button;
            self.held_keys = held.keys;
            self.flags = held.flags;
            self.click_count = held.count;
            self.mouse_number = held.mouse_number;
            self.position = self.on_display(Point {
                x: held.x,
                y: held.y,
            });
        }

        self.journal = Some(Journal::new(path));
        self.journal_dirty = true;

        self.release()
    }

    pub(super) fn rebind(&mut self, current: InputEpoch) -> Result<()> {
        if current.uid != self.epoch.uid
            || current.security_session_id != self.epoch.security_session_id
        {
            return Err(Error::new(
                Code::Identity,
                "Input recovery cannot cross logins",
            ));
        }

        self.epoch = current;
        self.journal_dirty = true;
        self.position = self.on_display(self.position);

        self.release()
    }

    /// `point`, moved onto the display if it lies past an edge.
    fn on_display(&self, point: Point) -> Point {
        Point {
            x: point.x.clamp(0.0, self.epoch.width - 1.0),
            y: point.y.clamp(0.0, self.epoch.height - 1.0),
        }
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

        self.check()?;
        let _authorization = self.grant.as_ref().map(Grant::authorize).transpose()?;
        super::ax::check_application(self.target)?;
        // Exactly the modifiers asked for: an arrow key's own numeric pad and
        // function flags would leave macOS reading function as held.
        CGEvent::set_flags(Some(&event), CGEventFlags(flags));
        CGEvent::post(TAP, Some(&event));
        self.dock_pressed = false;

        self.posted += 1;
        self.flags = flags;

        Ok(())
    }

    fn mouse(
        &mut self,
        kind: CGEventType,
        point: Point,
        button: Button,
        count: u8,
        flags: u64,
    ) -> Result<()> {
        let event = created(CGEvent::new_mouse_event(
            Some(&self.source.0),
            kind,
            point,
            button.number(),
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

    pub(super) fn release(&mut self) -> Result<()> {
        self.grant = None;
        self.target = None;
        self.dock_down = None;

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

    pub(super) fn held_input(&self) -> bool {
        self.held.is_some()
            || self.dock_down.is_some()
            || !self.held_keys.is_empty()
            || self.flags != 0
    }

    async fn post_pointer(
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
                    return Err(Error::new(
                        Code::Input,
                        "Release the held mouse button first",
                    ));
                }

                self.mouse(CGEventType::MouseMoved, point, button, count, flags)?;
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
                    // dragged location that has moved, and what is dragged
                    // then follows the pointer from there. Bound that initial
                    // step to one point so a sparse viewer stream does not
                    // lose its first jump. A stationary click never enters
                    // this path.
                    let anchor = Point {
                        x: self.position.x
                            + (point.x - self.position.x).clamp(-DRAG_ANCHOR, DRAG_ANCHOR),
                        y: self.position.y
                            + (point.y - self.position.y).clamp(-DRAG_ANCHOR, DRAG_ANCHOR),
                    };
                    self.mouse(active.dragged(), anchor, active, self.click_count, flags)?;
                    self.position = anchor;
                    self.drag_started = true;
                    self.commit()?;

                    if anchor.x == point.x && anchor.y == point.y {
                        return Ok(());
                    }

                    self.delay(DRAG_START).await?;
                }

                self.mouse(
                    if self.held.is_some() {
                        active.dragged()
                    } else {
                        CGEventType::MouseMoved
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
                    .ok_or_else(|| Error::new(Code::Input, "No mouse button is held"))?;
                if held != button {
                    return Err(Error::new(
                        Code::Input,
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

    #[allow(unsafe_code, reason = "FFI: key text is passed as a UTF-16 pointer")]
    fn key_event(&mut self, code: u16, down: bool, flags: u64, text: Option<&[u16]>) -> Result<()> {
        let event = created(CGEvent::new_keyboard_event(
            Some(&self.source.0),
            code,
            down,
        ))?;

        if let Some(text) = text {
            // SAFETY: the pointer and length describe the live UTF-16 slice.
            unsafe {
                CGEvent::keyboard_set_unicode_string(Some(&event), text.len() as _, text.as_ptr())
            };
        }

        self.post(event, flags, self.position)
    }

    fn release_modifiers(&mut self) -> Result<()> {
        if self.flags == 0 {
            return Ok(());
        }

        // Finish the chord in CoreGraphics too. Viewer key-up events do not
        // represent remote holds, so a completed gesture must be clean. A
        // failed release stops here, leaving it and the ones after it held
        // so that the next release tries them again.
        let mut held = self.flags;
        let released = release_each(&mut held, |code, rest| {
            self.key_event(code, false, rest, None)
        });

        self.flags = held;
        self.journal_dirty = true;
        released
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

    pub(super) async fn perform(&mut self, action: Action) -> Result<()> {
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
                if self.held.is_some() || self.dock_down.is_some() {
                    return Err(Error::new(Code::Input, "A mouse button is already held"));
                }
                for click in 1..=count {
                    self.click(Point { x, y }, button, &modifiers, click)
                        .await?;

                    if click < count {
                        self.delay(CLICK_GAP).await?;
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

                self.delay(DRAG_HOLD).await?;
                // What is dragged follows the pointer from the first moved
                // step (see the move phase), so the pointer travels that
                // step past the target, releases there, and then rests on
                // the target: the window or item moves exactly as asked.
                let step = Point {
                    x: (to_x - from_x).clamp(-DRAG_ANCHOR, DRAG_ANCHOR),
                    y: (to_y - from_y).clamp(-DRAG_ANCHOR, DRAG_ANCHOR),
                };
                let anchor = Point {
                    x: from_x + step.x,
                    y: from_y + step.y,
                };
                let end = self.on_display(Point {
                    x: to_x + step.x,
                    y: to_y + step.y,
                });

                self.pointer(PointerPhase::Move, anchor, button, &[], 1)
                    .await?;
                self.delay(DRAG_START).await?;

                for step in 1..=DRAG_STEPS {
                    let t = f64::from(step) / f64::from(DRAG_STEPS);
                    self.pointer(
                        PointerPhase::Move,
                        Point {
                            x: anchor.x + (end.x - anchor.x) * t,
                            y: anchor.y + (end.y - anchor.y) * t,
                        },
                        button,
                        &[],
                        1,
                    )
                    .await?;
                    self.delay(DRAG_STEP).await?;
                }

                self.pointer(PointerPhase::Up, end, button, &[], 1).await?;
                self.pointer(
                    PointerPhase::Move,
                    Point { x: to_x, y: to_y },
                    button,
                    &[],
                    1,
                )
                .await?;
            }
            Action::Scroll { x, y, dx, dy } => {
                self.pointer(PointerPhase::Move, Point { x, y }, Button::Left, &[], 1)
                    .await?;
                // The app learns where the pointer is from that move; the
                // wheel turn reaches the view under it only once it has.
                self.delay(SCROLL_SETTLE).await?;

                // A wheel's positive turn scrolls up and left; the command's
                // positive lines scroll down and right, as documented.
                let event = created(CGEvent::new_scroll_wheel_event2(
                    Some(&self.source.0),
                    CGScrollEventUnit::Line,
                    2,
                    -dy,
                    -dx,
                    0,
                ))?;
                self.post(event, 0, self.position)?;
            }
            Action::Type { text } => {
                for character in text.graphemes(true) {
                    let units: Vec<u16> = character.encode_utf16().collect();
                    self.key(0, 0, Some(&units))?;
                    self.delay(TYPE_GAP).await?;
                }
            }
            Action::Key { key } => {
                let (code, flags) =
                    parse_key(&key).ok_or_else(|| Error::new(Code::Input, "Unsupported key"))?;
                self.key(code, flags, None)?;
            }
            Action::Wait { ms } => self.delay(Duration::from_millis(ms.into())).await?,
        }

        Ok(())
    }
}

impl Button {
    fn number(self) -> CGMouseButton {
        match self {
            Self::Left => CGMouseButton::Left,
            Self::Right => CGMouseButton::Right,
            Self::Middle => CGMouseButton::Center,
        }
    }

    fn down(self) -> CGEventType {
        match self {
            Self::Left => CGEventType::LeftMouseDown,
            Self::Right => CGEventType::RightMouseDown,
            Self::Middle => CGEventType::OtherMouseDown,
        }
    }

    fn up(self) -> CGEventType {
        match self {
            Self::Left => CGEventType::LeftMouseUp,
            Self::Right => CGEventType::RightMouseUp,
            Self::Middle => CGEventType::OtherMouseUp,
        }
    }

    fn dragged(self) -> CGEventType {
        match self {
            Self::Left => CGEventType::LeftMouseDragged,
            Self::Right => CGEventType::RightMouseDragged,
            Self::Middle => CGEventType::OtherMouseDragged,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The typed event kinds, buttons and modifier masks are the raw values
    /// input has always sent.
    #[test]
    fn typed_constants_keep_their_raw_values() {
        let raw = |button: Button| {
            (
                button.number().0,
                button.down().0,
                button.up().0,
                button.dragged().0,
            )
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
}
