//! Where keyboard focus is inside one app, read for the check made just
//! before input is sent. Unlike a listing, which reports what it could and
//! marks the rest, this read is strict: every attribute the check judges is
//! read whole or the whole read fails, an attribute the app lacks is told
//! apart from one that failed or answered something else, and each request
//! is bounded by what is left of one deadline. Only copies are made;
//! nothing is set but each element's own messaging timeout.
use super::{
    absent, ffi,
    text::{bounded_string, getter_failed},
};
use atlas_core::{
    error::{Code, Error, Result},
    screen::{elements::Element, geometry::Bounds},
};
use objc2_application_services::{AXError, AXUIElement, AXValue, AXValueType};
use objc2_core_foundation::{CFBoolean, CFNull, CFRetained, CFString, CFType};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    time::{Duration, Instant},
};

/// Most steps from the focused element up to its window. No app nests a
/// field this deep; the read's deadline ends a long walk well before it.
pub(super) const PARENTS: usize = 256;

/// The longest one request to an app may take.
pub(super) const REQUEST: Duration = Duration::from_millis(250);

/// The pause before asking again an app that could not complete a request:
/// it was busy, as TextEdit is while it lays out a long document.
pub(super) const BUSY_PAUSE: Duration = Duration::from_millis(10);

/// The element with keyboard focus in an app and the process that owns
/// it, which for a panel another process draws is not the app's.
#[derive(Debug, Clone)]
pub(in crate::native) struct Focused {
    pub(in crate::native) element: Element,
    pub(in crate::native) pid: i64,
}

/// Where the focused element sits, from its parents up to a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(in crate::native) enum Within {
    /// In the app's focused window, with no sheet between them.
    #[default]
    Window,
    /// In a sheet, which takes the keys meant for its window.
    Sheet,
    /// Not in the focused window: the walk ended at another window or at
    /// the top.
    Elsewhere,
}

/// Where keyboard focus is inside one app, as Accessibility reports it.
/// Never a field's value: no value is read at all.
#[derive(Debug, Clone, Default)]
pub(in crate::native) struct Keyboard {
    /// The app's focused window: its role, subrole, title and frame. None
    /// when the app has none, or when no window is judged.
    pub(in crate::native) window: Option<Element>,
    /// The focused element: its role, subrole, names and flags. None when
    /// nothing in the app has focus, or when no field is judged.
    pub(in crate::native) focused: Option<Focused>,
    /// Where the focused element sits; [`Within::Window`] when there is
    /// none.
    pub(in crate::native) within: Within,
}

/// Why Atlas could not tell where keyboard focus is, as a refusal names
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(in crate::native) enum Unread {
    /// A read failed.
    Failed,
    /// A read answered something other than what the attribute holds.
    Malformed,
    /// A title or name the check judges is longer than Atlas reads.
    Cut,
    /// The focused element sits deeper than Atlas walks.
    Deep,
    /// The app did not answer in time.
    Slow,
    /// Two listed windows of the app could each be the focused one.
    Alike,
    /// The focused window is not in the window list.
    Unlisted,
    /// The app gave its own element as its focused window, as apps do
    /// while the Mac's screen is locked: it withheld its windows.
    Withheld,
}

impl Unread {
    pub(super) fn reading(self, status: AXError) -> Error {
        self.after(status).error().with_source(Status(status))
    }

    /// [`Unread::reading`], keeping which request `call` failed.
    pub(in crate::native) fn asking(self, call: &'static str, status: AXError) -> Error {
        self.after(status)
            .error()
            .with_source(Asked { call, status })
    }

    /// Why a request that ended with `status` could not tell: an app that
    /// could not complete it is slow, whatever else was wrong.
    fn after(self, status: AXError) -> Self {
        if status == AXError::CannotComplete {
            Self::Slow
        } else {
            self
        }
    }

    /// Whether the request behind `error` failed because the app could not
    /// complete it.
    pub(super) fn busy(error: &Error) -> bool {
        let mut cause = std::error::Error::source(error);
        while let Some(current) = cause {
            if let Some(asked) = current.downcast_ref::<Asked>() {
                return asked.status == AXError::CannotComplete;
            }
            if let Some(Status(status)) = current.downcast_ref::<Status>() {
                return *status == AXError::CannotComplete;
            }
            cause = current.source();
        }
        false
    }

    /// Why `error` says it could not tell, when it says.
    pub(in crate::native) fn of(error: &Error) -> Option<Self> {
        Self::deserialize(error.details.get("unread")?).ok()
    }

    /// The error a read that could not tell, for this reason, ends with.
    pub(in crate::native) fn error(self) -> Error {
        let said = match self {
            Self::Failed | Self::Malformed => "Where the app's focus is could not be read",
            Self::Cut => "A title or name of the focused window or field is too long to read whole",
            Self::Deep => "The focused element sits deeper in the app than Atlas reads",
            Self::Slow => "The app did not say in time where its focus is",
            Self::Alike => "Two windows of the app look the same",
            Self::Unlisted => "The window with keyboard focus is not in the window list",
            Self::Withheld => {
                "The app withheld its windows, as apps do while the Mac's screen is locked"
            }
        };
        Error::new(Code::Native, said).with("unread", serde_json::json!(self))
    }
}

/// The status behind a failed Accessibility request, kept as an internal
/// cause: a refusal states only its category.
#[derive(Debug)]
pub(in crate::native) struct Status(pub(in crate::native) AXError);

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Accessibility error {}", self.0.0)
    }
}

impl std::error::Error for Status {}

/// The Accessibility request that failed and its status, kept as an
/// internal cause: the attribute or call it was, never what it held.
#[derive(Debug)]
pub(in crate::native) struct Asked {
    pub(in crate::native) call: &'static str,
    pub(in crate::native) status: AXError,
}

impl fmt::Display for Asked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} failed with Accessibility error {}",
            self.call, self.status.0
        )
    }
}

impl std::error::Error for Asked {}

/// One attribute's answer, as the read tells answers apart.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Answer<E> {
    /// The element has no such attribute, or no value for it.
    Absent,
    /// The request failed with this error.
    Failed(AXError),
    /// Text, whole.
    Text(String),
    /// Text longer than Atlas reads.
    Cut,
    Flag(bool),
    Point(f64, f64),
    Size(f64, f64),
    Element(E),
    /// Something else.
    Other,
}

/// What the read asks of the app, and when: the live app, or a test's.
pub(super) trait Ask {
    /// An element of the app.
    type Element: Clone + PartialEq;
    fn now(&self) -> Instant;
    /// Wait `duration` before asking again.
    fn pause(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
    /// Bound the next request to `element` by `timeout`; false when that
    /// could not be done.
    fn bound(&mut self, element: &Self::Element, timeout: Duration) -> bool;
    /// One attribute of `element`.
    fn ask(&mut self, element: &Self::Element, attribute: &'static str) -> Answer<Self::Element>;
    /// The process that owns `element`, from the element itself: nothing
    /// is asked of the app.
    fn owner(&mut self, element: &Self::Element) -> Option<i64>;
}

/// What a check judges, so a read asks for that and nothing more: the
/// focused window and where the focused element sits in it, and the
/// focused element itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::native) struct Judged {
    pub(in crate::native) window: bool,
    pub(in crate::native) field: bool,
}

/// Which name namespaces a plain getter failure left unproven.
#[derive(Default)]
pub(super) struct UnknownNames {
    pub(super) labels: bool,
    pub(super) identifier: bool,
}

pub(in crate::native) fn remaining(deadline: Instant, now: Instant) -> Result<Duration> {
    let left = deadline.saturating_duration_since(now);
    if left.is_zero() {
        Err(Unread::Slow.error())
    } else {
        Ok(left)
    }
}

/// One strict read, until its deadline.
pub(super) struct Reader<A> {
    pub(super) ask: A,
    pub(super) deadline: Instant,
}

impl<A: Ask> Reader<A> {
    /// What is left of the deadline; an error once it is spent.
    pub(super) fn left(&self) -> Result<Duration> {
        remaining(self.deadline, self.ask.now())
    }

    pub(super) fn bound(&mut self, element: &A::Element, budget: Duration) -> Result<()> {
        let left = self.left()?;
        if !self.ask.bound(element, left.min(budget)) {
            return Err(Unread::Failed.error());
        }
        Ok(())
    }

    /// The process that owns `element`, as the element itself says.
    fn owner(&mut self, element: &A::Element) -> Result<i64> {
        self.ask
            .owner(element)
            .ok_or_else(|| Unread::Failed.error())
    }

    pub(super) fn owned(&mut self, element: &A::Element, pid: i64) -> Result<()> {
        if self.ask.owner(element) != Some(pid) {
            return Err(Unread::Failed.error());
        }
        Ok(())
    }

    /// One attribute as the app answered it, its request bounded by what
    /// is left just before it is sent. An app too busy to complete it is
    /// asked again, after [`BUSY_PAUSE`], while the read has time for
    /// another. A request that could not be bounded, or that ended past the
    /// deadline, is an error, which keeps the request and its status when
    /// it failed.
    fn asked(
        &mut self,
        element: &A::Element,
        attribute: &'static str,
    ) -> Result<Answer<A::Element>> {
        loop {
            self.bound(element, REQUEST)?;
            let answer = self.ask.ask(element, attribute);
            let left = match (self.left(), &answer) {
                (Ok(left), _) => left,
                (Err(_), Answer::Failed(status)) => {
                    return Err(Unread::Slow.asking(attribute, *status));
                }
                (Err(late), _) => return Err(late),
            };
            if answer != Answer::Failed(AXError::CannotComplete) || left <= BUSY_PAUSE {
                return Ok(answer);
            }
            self.ask.pause(BUSY_PAUSE);
        }
    }

    /// Whether the read has time to ask again a request the app could not
    /// complete, behind `error`, once [`BUSY_PAUSE`] has passed.
    pub(super) fn may_ask_again(&self, error: &Error) -> bool {
        Unread::busy(error) && self.left().is_ok_and(|left| left > BUSY_PAUSE)
    }

    /// [`Reader::asked`], a failed request an error too.
    fn answer(
        &mut self,
        element: &A::Element,
        attribute: &'static str,
    ) -> Result<Answer<A::Element>> {
        match self.asked(element, attribute)? {
            Answer::Failed(status) => Err(Unread::Failed.asking(attribute, status)),
            answer => Ok(answer),
        }
    }

    pub(super) fn text(
        &mut self,
        element: &A::Element,
        attribute: &'static str,
    ) -> Result<Option<String>> {
        text(self.answer(element, attribute)?)
    }

    /// One name, retaining plain getter failures for candidate ranking.
    /// Other failures and malformed or incomplete text refuse immediately.
    fn name(
        &mut self,
        element: &A::Element,
        attribute: &'static str,
        failed: &mut bool,
    ) -> Result<Option<String>> {
        match self.asked(element, attribute)? {
            Answer::Failed(status) if getter_failed(status, attribute) => {
                *failed = true;
                Ok(None)
            }
            Answer::Failed(status) => Err(Unread::Failed.asking(attribute, status)),
            answer => text(answer),
        }
    }

    pub(super) fn flag(
        &mut self,
        element: &A::Element,
        attribute: &'static str,
    ) -> Result<Option<bool>> {
        match self.answer(element, attribute)? {
            Answer::Absent => Ok(None),
            Answer::Flag(flag) => Ok(Some(flag)),
            Answer::Failed(_)
            | Answer::Text(_)
            | Answer::Cut
            | Answer::Point(..)
            | Answer::Size(..)
            | Answer::Element(_)
            | Answer::Other => Err(Unread::Malformed.error()),
        }
    }

    pub(super) fn reference(
        &mut self,
        element: &A::Element,
        attribute: &'static str,
    ) -> Result<Option<A::Element>> {
        reference(self.answer(element, attribute)?)
    }

    /// The app's focused element; none when the element it names no longer
    /// exists, also when asked again. Compose Desktop windows name such an
    /// element whenever nothing inside them is focused; like an app that
    /// names none, the focused window alone is then judged.
    pub(super) fn focused_element(&mut self, app: &A::Element) -> Result<Option<A::Element>> {
        for _ in 0..2 {
            let Some(element) = self.reference(app, "AXFocusedUIElement")? else {
                return Ok(None);
            };
            let answer = self.asked(&element, "AXRole")?;
            if answer != Answer::Failed(AXError::InvalidUIElement) {
                return Ok(Some(element));
            }
        }
        Ok(None)
    }

    /// The frame from position and size, both there or both absent.
    pub(super) fn frame(&mut self, element: &A::Element) -> Result<Option<Bounds>> {
        let position = self.answer(element, "AXPosition")?;
        let size = self.answer(element, "AXSize")?;
        match (position, size) {
            (Answer::Absent, Answer::Absent) => Ok(None),
            (Answer::Point(x, y), Answer::Size(width, height)) => Ok(Some(Bounds {
                x,
                y,
                width,
                height,
            })),
            _ => Err(Unread::Malformed.error()),
        }
    }

    /// A window as the check judges it: role, subrole, title and frame.
    pub(super) fn window(&mut self, element: &A::Element) -> Result<Element> {
        Ok(Element {
            role: self.text(element, "AXRole")?,
            subrole: self.text(element, "AXSubrole")?,
            title: self.text(element, "AXTitle")?,
            frame: self.frame(element)?,
            ..Element::default()
        })
    }

    /// A focused element as the check judges it: its role, subrole and
    /// names, as `atlas elements --name` matches them, and whether it is
    /// enabled and focused. No value is read: a field's value is what was
    /// typed, never its name.
    pub(super) fn field(&mut self, element: &A::Element) -> Result<Element> {
        self.field_names(element).map(|(field, _)| field)
    }

    /// The field and the name attributes whose getters failed plainly.
    /// Each failed read leaves that name's namespace uncertain.
    pub(super) fn field_names(&mut self, element: &A::Element) -> Result<(Element, UnknownNames)> {
        let mut failed = UnknownNames::default();
        let field = Element {
            role: self.text(element, "AXRole")?,
            subrole: self.text(element, "AXSubrole")?,
            title: self.name(element, "AXTitle", &mut failed.labels)?,
            description: self.name(element, "AXDescription", &mut failed.labels)?,
            identifier: self.name(element, "AXIdentifier", &mut failed.identifier)?,
            enabled: self.flag(element, "AXEnabled")?,
            focused: self.flag(element, "AXFocused")?,
            ..Element::default()
        };
        Ok((field, failed))
    }

    /// Where `element` sits, walking its parents until [`place`] tells.
    pub(super) fn within(
        &mut self,
        element: A::Element,
        window: Option<&A::Element>,
    ) -> Result<Within> {
        let mut current = element;
        for _ in 0..PARENTS {
            let at_window = window.is_some_and(|window| *window == current);
            let role = if at_window {
                None
            } else {
                self.text(&current, "AXRole")?
            };
            if let Some(within) = place(at_window, role.as_deref()) {
                return Ok(within);
            }

            match self.reference(&current, "AXParent")? {
                Some(parent) => current = parent,
                None => return Ok(Within::Elsewhere),
            }
        }

        Err(Unread::Deep.error())
    }

    /// A focused window may be the document that holds the focused sheet.
    /// Prefer that sheet's own frame when matching its CG window number.
    fn focused_window(
        &mut self,
        element: A::Element,
        window: Option<&A::Element>,
    ) -> Result<Option<A::Element>> {
        let mut current = element;
        for _ in 0..PARENTS {
            if window == Some(&current) {
                return Ok(None);
            }
            match self.text(&current, "AXRole")?.as_deref() {
                Some("AXSheet") => return Ok(Some(current)),
                Some("AXWindow") if window.is_none() => return Ok(Some(current)),
                Some("AXWindow" | "AXApplication") => return Ok(None),
                _ => {}
            }
            match self.reference(&current, "AXParent")? {
                Some(parent) => current = parent,
                None => return Ok(None),
            }
        }
        Err(Unread::Deep.error())
    }
}

/// The text an answer holds: none when absent, an error when it is cut
/// short or anything but text.
fn text<E>(answer: Answer<E>) -> Result<Option<String>> {
    match answer {
        Answer::Absent => Ok(None),
        Answer::Text(text) => Ok(Some(text)),
        Answer::Cut => Err(Unread::Cut.error()),
        Answer::Failed(_)
        | Answer::Flag(_)
        | Answer::Point(..)
        | Answer::Size(..)
        | Answer::Element(_)
        | Answer::Other => Err(Unread::Malformed.error()),
    }
}

/// The element an answer names: none when absent, an error when it names
/// anything but an element.
fn reference<E>(answer: Answer<E>) -> Result<Option<E>> {
    match answer {
        Answer::Absent => Ok(None),
        Answer::Element(element) => Ok(Some(element)),
        Answer::Failed(_)
        | Answer::Text(_)
        | Answer::Cut
        | Answer::Flag(_)
        | Answer::Point(..)
        | Answer::Size(..)
        | Answer::Other => Err(Unread::Malformed.error()),
    }
}

/// The walk at one element: the focused window itself, a sheet before it,
/// another window or the app at the top, or anything else to pass.
fn place(at_window: bool, role: Option<&str>) -> Option<Within> {
    if at_window {
        return Some(Within::Window);
    }

    match role {
        Some("AXSheet") => Some(Within::Sheet),
        Some("AXWindow" | "AXApplication") => Some(Within::Elsewhere),
        _ => None,
    }
}

/// Where keyboard focus is inside the app whose element is `app`, read
/// through `ask` until `deadline`, as far as what is `judged` needs.
pub(super) fn read<A: Ask>(
    ask: A,
    app: &A::Element,
    deadline: Instant,
    judged: Judged,
) -> Result<Keyboard> {
    let mut reader = Reader { ask, deadline };
    let mut keyboard = Keyboard::default();
    let mut window = if judged.window {
        reader.reference(app, "AXFocusedWindow")?
    } else {
        None
    };
    let element = reader.focused_element(app)?;
    if judged.window {
        if let Some(element) = &element
            && let Some(focused) = reader.focused_window(element.clone(), window.as_ref())?
        {
            let owner = reader.owner(app)?;
            reader.owned(&focused, owner)?;
            window = Some(focused);
        }
        if let Some(window) = &window {
            let owner = reader.owner(app)?;
            reader.owned(window, owner)?;
            keyboard.window = Some(reader.window(window)?);
        }
    }
    let Some(element) = element else {
        return Ok(keyboard);
    };
    if judged.field {
        keyboard.focused = Some(Focused {
            pid: reader.owner(&element)?,
            element: reader.field(&element)?,
        });
    }
    if judged.window {
        keyboard.within = reader.within(element, window.as_ref())?;
    }

    Ok(keyboard)
}

/// Where keyboard focus is inside app `pid`, as far as what is `judged`
/// needs, waiting at most `timeout` in all. Any read that fails, answers
/// something else, is cut short or ends past the deadline makes the whole
/// answer an error: half an answer could pass a check it should not.
pub(in crate::native) fn keyboard(pid: i64, timeout: Duration, judged: Judged) -> Result<Keyboard> {
    let deadline = Instant::now() + timeout;
    read(Live, &app_element(pid)?, deadline, judged)
}

/// Some apps omit the system-wide focused application. Their own frontmost
/// flag and an owned focus reference must both be present to substitute;
/// with `desktop`, for a pointer action, the reference may be an owned
/// focused element beside a focused window that is no window (see
/// [`app_focus`]).
pub(in crate::native) fn frontmost(pid: i64, timeout: Duration, desktop: bool) -> Result<bool> {
    let deadline = Instant::now() + timeout;
    let app = app_element(pid)?;
    app_focus(
        &mut Reader {
            ask: Live,
            deadline,
        },
        &app,
        pid,
        desktop,
    )
}

fn app_element(pid: i64) -> Result<CFRetained<AXUIElement>> {
    let pid = libc::pid_t::try_from(pid).map_err(|_| Unread::Failed.error())?;
    ffi::application(pid).ok_or_else(|| Unread::Failed.error())
}

/// Whether app `pid` says it is frontmost and holds a focus reference of
/// its own. A focused window that is no window, as Finder's desktop is
/// while none of its windows is key, is malformed unless `desktop` allows
/// it; then it proves nothing alone, and only an owned focused element
/// does. That proves the app active, not which window takes the keys.
fn app_focus<A: Ask>(
    reader: &mut Reader<A>,
    app: &A::Element,
    pid: i64,
    desktop: bool,
) -> Result<bool> {
    if reader.ask.owner(app) != Some(pid) || reader.flag(app, "AXFrontmost")? != Some(true) {
        return Ok(false);
    }
    let mut unwindowed = false;
    for attribute in ["AXFocusedWindow", "AXFocusedUIElement"] {
        let element = match attribute {
            "AXFocusedUIElement" => reader.focused_element(app)?,
            _ => reader.reference(app, attribute)?,
        };
        if let Some(element) = element {
            if reader.ask.owner(&element) != Some(pid) {
                return Err(Unread::Failed.error());
            }
            match reader.text(&element, "AXRole")?.as_deref() {
                Some("AXApplication") => return Err(Unread::Withheld.error()),
                Some("AXWindow" | "AXSheet") => {}
                Some(_) if attribute == "AXFocusedUIElement" => {}
                Some(_) if desktop => {
                    unwindowed = true;
                    continue;
                }
                Some(_) | None => return Err(Unread::Malformed.error()),
            }
            return Ok(true);
        }
    }
    if unwindowed {
        return Err(Unread::Malformed.error());
    }
    Ok(false)
}

/// The app itself, asked through Accessibility.
pub(super) struct Live;

impl Ask for Live {
    type Element = CFRetained<AXUIElement>;

    fn now(&self) -> Instant {
        Instant::now()
    }

    fn bound(&mut self, element: &Self::Element, timeout: Duration) -> bool {
        // This one element's messaging timeout bounds the next request to
        // it; nothing else in the process is affected.
        ffi::set_timeout(element, timeout) == AXError::Success
    }

    fn ask(&mut self, element: &Self::Element, attribute: &'static str) -> Answer<Self::Element> {
        let name = CFString::from_static_str(attribute);
        match ffi::copy(element, &name) {
            Ok(Some(value)) => answer(value),
            Ok(None) => Answer::Absent,
            Err(status) if absent(status) => Answer::Absent,
            Err(status) => Answer::Failed(status),
        }
    }

    fn owner(&mut self, element: &Self::Element) -> Option<i64> {
        ffi::owner(element)
            .ok()
            .filter(|pid| *pid > 0)
            .map(i64::from)
    }
}

/// What a copied value is, as the read tells answers apart.
fn answer(value: CFRetained<CFType>) -> Answer<CFRetained<AXUIElement>> {
    if value.downcast_ref::<CFNull>().is_some() {
        return Answer::Absent;
    }
    if let Some(text) = value.downcast_ref::<CFString>() {
        return match bounded_string(text) {
            Some((text, false)) => Answer::Text(text),
            Some((_, true)) => Answer::Cut,
            None => Answer::Other,
        };
    }
    if let Some(flag) = value.downcast_ref::<CFBoolean>() {
        return Answer::Flag(flag.as_bool());
    }
    if let Some(geometry) = value.downcast_ref::<AXValue>() {
        return geometric(geometry);
    }

    value
        .downcast::<AXUIElement>()
        .map_or(Answer::Other, Answer::Element)
}

/// A point or a size, read into the struct its type names.
fn geometric(value: &AXValue) -> Answer<CFRetained<AXUIElement>> {
    match ffi::kind(value) {
        AXValueType::CGPoint => {
            ffi::point(value).map_or(Answer::Other, |point| Answer::Point(point.x, point.y))
        }
        AXValueType::CGSize => {
            ffi::size(value).map_or(Answer::Other, |size| Answer::Size(size.width, size.height))
        }
        _ => Answer::Other,
    }
}

#[cfg(test)]
mod tests;
