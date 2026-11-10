//! CoreGraphics input belongs to one private event source in the helper session.
use super::{
    activity::{self, Acting},
    ax::Looks,
    journal::{self, Held, Journal},
    lease::Grant,
};
use crate::native::{accessibility_trusted, session_info};
use atlas_core::{
    actions::{
        Action, Button, DRAG_ANCHOR, DRAG_HOLD, DRAG_START, DRAG_STEP, DRAG_STEPS, MODIFIER_KEYS,
        PointerPhase, SCROLL_SETTLE, TYPE_GAP, modifier_flags, parse_key,
    },
    error::{Code, Error, Result},
    session::InputEpoch,
};
use objc2_core_foundation::{CFRetained, CGPoint as Point};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation,
    CGEventType, CGMouseButton, CGScrollEventUnit,
};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use unicode_segmentation::UnicodeSegmentation;

mod dock;
#[cfg(test)]
pub(super) mod tap;
mod text;

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

fn may_not_post() -> Error {
    Error::new(
        Code::Permission,
        "The input process may not post events (Accessibility)",
    )
}

/// The first moved step of a drag from `from` towards `to`: at most
/// [`DRAG_ANCHOR`] on each axis.
fn first_step(from: Point, to: Point) -> Point {
    Point {
        x: (to.x - from.x).clamp(-DRAG_ANCHOR, DRAG_ANCHOR),
        y: (to.y - from.y).clamp(-DRAG_ANCHOR, DRAG_ANCHOR),
    }
}

fn shifted(point: Point, by: Point) -> Point {
    Point {
        x: point.x + by.x,
        y: point.y + by.y,
    }
}

/// Release each held modifier in `held` through `post`, given its key code
/// and the flags still held once it is up. A modifier's flag is cleared only
/// once its own release has posted, so after a failure the ones not yet
/// released stay held and the next release tries them again.
fn release_each(held: &mut u64, mut post: impl FnMut(u16, u64) -> Result<()>) -> Result<()> {
    for &(code, mask) in MODIFIER_KEYS {
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
struct Source(
    #[cfg(not(test))] CFRetained<CGEventSource>,
    #[cfg(test)] Option<CFRetained<CGEventSource>>,
);

impl Source {
    #[cfg(not(test))]
    fn get(&self) -> Option<&CGEventSource> {
        Some(&self.0)
    }

    /// A test engine has none: its events are created without one and
    /// reach only its tap.
    #[cfg(test)]
    fn get(&self) -> Option<&CGEventSource> {
        self.0.as_deref()
    }
}

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
    pub(super) epoch: InputEpoch,
    pub(super) grant: Option<Grant>,
    /// The process resolved for this request, retained through the last post.
    pub(super) target: Option<atlas_core::process::Instance>,
    /// An agent's request the person's accounts passed: each action, and
    /// each character typed or click after its first, is judged again just
    /// before it is sent. A drag is judged whole before its press: let go
    /// partway, it would drop where it is.
    pub(super) recheck: Option<super::asking::Recheck>,
    /// A policy test's synthetic pointed target, independent of the host desktop.
    #[cfg(test)]
    pub(super) asking_point: Option<super::asking::Target>,
    /// The looks the service keeps, where a referenced click finds its element.
    pub(super) looks: Arc<Looks>,
    journal: Option<Journal>,
    journal_dirty: bool,
    /// Events posted or AX presses acknowledged so far: distinguishes input
    /// that already changed something from input that changed nothing.
    pub(super) posted: u64,
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
    /// Where input is told to the helper's other processes, so their work
    /// that must not overlap it waits; none tells no one.
    pub(super) activity: Option<PathBuf>,
    /// Held while input stays held between requests, such as a button
    /// pressed and not yet let go, so no such work starts under it.
    acting: Option<Acting>,
    #[cfg(test)]
    tap: Option<tap::Tap>,
}

impl Engine {
    #[cfg(test)]
    pub(super) fn idle(epoch: InputEpoch) -> Self {
        Self::with_source(epoch, Source(None))
    }

    /// A test engine whose checks pass, which finds no Dock and whose events
    /// reach only `tap`, journaling to `journal` when given one.
    #[cfg(test)]
    pub(super) fn tapped(epoch: InputEpoch, journal: Option<PathBuf>, tap: tap::Tap) -> Self {
        let mut engine = Self::with_source(epoch, Source(None));
        engine.journal = journal.map(Journal::new);
        engine.tap = Some(tap);
        engine
    }

    fn with_source(epoch: InputEpoch, source: Source) -> Self {
        Self {
            epoch,
            grant: None,
            target: None,
            recheck: None,
            #[cfg(test)]
            asking_point: None,
            looks: Arc::default(),
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
            activity: None,
            acting: None,
            #[cfg(test)]
            tap: None,
        }
    }

    pub(super) fn new(uid: u32) -> Result<Self> {
        let source = CGEventSource::new(CGEventSourceStateID::Private)
            .ok_or_else(|| Error::new(Code::Native, "Could not create the event source"))?;
        #[cfg(test)]
        let source = Some(source);
        let source = Source(source);
        let mut engine = Self::with_source(
            InputEpoch::from_proof(&session_info()?, std::process::id())?,
            source,
        );

        if engine.epoch.uid != uid {
            return Err(Error::new(
                Code::Identity,
                "Input service UID differs from its configuration",
            ));
        }

        engine.check()?;
        Ok(engine)
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
                return Err(may_not_post());
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

        if !accessibility_trusted() {
            return Err(Error::new(
                Code::Permission,
                "The input process does not have Accessibility permission",
            ));
        }

        Ok(())
    }

    pub(super) fn check(&mut self) -> Result<()> {
        #[cfg(test)]
        if let Some(tap) = &self.tap {
            tap.check();
            return Ok(());
        }

        Self::check_epoch(&self.epoch)?;

        // Without it macOS drops posted events silently: refuse instead.
        // The answer is trusted for a moment (see `POST_ACCESS_TTL`).
        let confirmed = self
            .post_access
            .is_some_and(|at| at.elapsed() < POST_ACCESS_TTL);
        if !confirmed {
            if !objc2_core_graphics::CGPreflightPostEventAccess() {
                self.post_access = None;
                return Err(may_not_post());
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
            let held = Held {
                epoch: self.epoch.clone(),
                button: self.held,
                keys: self.held_keys.clone(),
                x: point.x,
                y: point.y,
                count: self.click_count,
                mouse_number: self.mouse_number,
                flags,
            };
            #[cfg(test)]
            if let Some(tap) = &mut self.tap {
                tap.before_write(&held)?;
            }
            journal.write(held)?;
            #[cfg(test)]
            if let (Some(tap), Some(held)) = (&self.tap, journal.saved()) {
                tap.written(held);
            }
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
        self.emit(&event);
        self.dock_pressed = false;

        self.posted += 1;
        self.flags = flags;

        Ok(())
    }

    #[cfg(not(test))]
    fn emit(&self, event: &CGEvent) {
        CGEvent::post(TAP, Some(event));
    }

    /// Only a tapped test engine has anywhere to send events.
    #[cfg(test)]
    fn emit(&self, event: &CGEvent) {
        self.tap
            .as_ref()
            .expect("An idle test engine cannot post events")
            .post(event);
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
            self.source.get(),
            kind,
            point,
            Mouse::from(button).number,
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

    /// Input about to be posted, told to the helper's other processes once
    /// work that keeps input out has ended (see [`activity::acting`]); none
    /// when input held from before is told already, or no one is told.
    pub(super) fn acting(&self) -> Option<Acting> {
        if self.acting.is_some() {
            return None;
        }
        self.activity.as_deref().and_then(activity::acting)
    }

    /// After posting under `acting`: input still held stays told until it
    /// is let go; otherwise it has ended now.
    pub(super) fn keep_while_held(&mut self, acting: Option<Acting>) {
        if !self.held_input() {
            self.acting = None;
        } else if self.acting.is_none() {
            self.acting = acting;
        }
    }

    pub(super) fn release(&mut self) -> Result<()> {
        self.grant = None;
        self.target = None;
        self.dock_down = None;

        if !self.held_input() {
            self.acting = None;
            return if self.journal_dirty {
                self.commit()
            } else {
                Ok(())
            };
        }

        let acting = self.acting();
        let released = self.release_held();
        self.keep_while_held(acting);
        released
    }

    /// Let go of every button and key still held, and save that.
    fn release_held(&mut self) -> Result<()> {
        if let Some(button) = self.held {
            self.mouse(
                Mouse::from(button).up,
                self.position,
                button,
                self.click_count,
                0,
            )?;
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

    /// Whether a release has anything to do: input still held, or a
    /// journal that does not yet say what is.
    pub(super) fn unsettled(&self) -> bool {
        self.held_input() || self.journal_dirty
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
                self.mouse(Mouse::from(button).down, point, button, count, flags)?;
            }
            PointerPhase::Move => {
                let active = self.held.unwrap_or(button);

                if self.held.is_some() && !self.drag_started && point != self.position {
                    // AppKit establishes native window tracking at the first
                    // dragged location that has moved, and what is dragged
                    // then follows the pointer from there. Bound that initial
                    // step to one point so a sparse viewer stream does not
                    // lose its first jump. A stationary click never enters
                    // this path.
                    let anchor = shifted(self.position, first_step(self.position, point));
                    self.mouse(
                        Mouse::from(active).dragged,
                        anchor,
                        active,
                        self.click_count,
                        flags,
                    )?;
                    self.position = anchor;
                    self.drag_started = true;
                    self.commit()?;

                    if anchor == point {
                        return Ok(());
                    }

                    self.delay(DRAG_START).await?;
                }

                self.mouse(
                    if self.held.is_some() {
                        Mouse::from(active).dragged
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

                self.mouse(
                    Mouse::from(button).up,
                    point,
                    button,
                    self.click_count,
                    flags,
                )?;
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
        let event = created(CGEvent::new_keyboard_event(self.source.get(), code, down))?;

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

    /// One step of a gesture with no modifiers and a single click.
    async fn step(&mut self, phase: PointerPhase, point: Point, button: Button) -> Result<()> {
        self.pointer(phase, point, button, &[], 1).await
    }

    /// One drag owns its anchor, paced travel, release and final resting point.
    async fn drag(&mut self, from: Point, to: Point, button: Button) -> Result<()> {
        self.step(PointerPhase::Down, from, button).await?;

        self.delay(DRAG_HOLD).await?;
        // What is dragged follows the pointer from the first moved
        // step (see the move phase), so the pointer travels that
        // step past the target, releases there, and then rests on
        // the target: the window or item moves exactly as asked.
        let first = first_step(from, to);
        let anchor = shifted(from, first);
        let end = self.on_display(shifted(to, first));

        self.step(PointerPhase::Move, anchor, button).await?;
        self.delay(DRAG_START).await?;

        for step in 1..=DRAG_STEPS {
            let t = f64::from(step) / f64::from(DRAG_STEPS);
            let point = Point {
                x: anchor.x + (end.x - anchor.x) * t,
                y: anchor.y + (end.y - anchor.y) * t,
            };
            self.step(PointerPhase::Move, point, button).await?;
            self.delay(DRAG_STEP).await?;
        }

        self.step(PointerPhase::Up, end, button).await?;
        self.step(PointerPhase::Move, to, button).await
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
                self.click(Point { x, y }, button, &modifiers, count)
                    .await?
            }
            Action::Drag {
                from_x,
                from_y,
                to_x,
                to_y,
                button,
            } => {
                self.drag(
                    Point {
                        x: from_x,
                        y: from_y,
                    },
                    Point { x: to_x, y: to_y },
                    button,
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
                    self.source.get(),
                    CGScrollEventUnit::Line,
                    2,
                    -dy,
                    -dx,
                    0,
                ))?;
                self.post(event, 0, self.position)?;
            }
            Action::Type { text } => {
                let plan = text::Plan::current(CGEventSource::keyboard_type(self.source.get()));
                for (index, character) in text.graphemes(true).enumerate() {
                    // Its first character goes as the action was judged; each
                    // after it is judged again where it lands.
                    if index > 0 {
                        super::service::recheck_event(self)?;
                    }
                    let units: Vec<u16> = character.encode_utf16().collect();
                    let (code, flags) = plan.key(&units);
                    self.key(code, flags, Some(&units))?;
                    self.delay(TYPE_GAP).await?;
                }
            }
            Action::Key { key } => {
                let (code, flags) =
                    parse_key(&key).ok_or_else(|| Error::new(Code::Input, "Unsupported key"))?;
                self.key(code, flags, None)?;
            }
            Action::Wait { ms } => self.delay(Duration::from_millis(ms.into())).await?,
            // A batch finds a referenced element again and clicks it as a
            // click, or acts on it through Accessibility; one that reaches
            // here was never found, so nothing posts.
            Action::Reference { .. }
            | Action::Press { .. }
            | Action::SetValue { .. }
            | Action::Check { .. }
            | Action::Uncheck { .. }
            | Action::Pick { .. }
            | Action::Expand { .. }
            | Action::Collapse { .. }
            | Action::ScrollIntoView { .. } => {
                return Err(Error::new(
                    Code::Input,
                    "A referenced element must be found again before it is acted on",
                ));
            }
        }

        Ok(())
    }
}

/// A button as macOS names it and its events.
struct Mouse {
    number: CGMouseButton,
    down: CGEventType,
    up: CGEventType,
    dragged: CGEventType,
}

impl From<Button> for Mouse {
    fn from(button: Button) -> Self {
        let (number, down, up, dragged) = match button {
            Button::Left => (
                CGMouseButton::Left,
                CGEventType::LeftMouseDown,
                CGEventType::LeftMouseUp,
                CGEventType::LeftMouseDragged,
            ),
            Button::Right => (
                CGMouseButton::Right,
                CGEventType::RightMouseDown,
                CGEventType::RightMouseUp,
                CGEventType::RightMouseDragged,
            ),
            Button::Middle => (
                CGMouseButton::Center,
                CGEventType::OtherMouseDown,
                CGEventType::OtherMouseUp,
                CGEventType::OtherMouseDragged,
            ),
        };
        Self {
            number,
            down,
            up,
            dragged,
        }
    }
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
