//! Where keyboard focus is inside one app, read for the check made just
//! before input is sent. Unlike a listing, which reports what it could and
//! marks the rest, this read is strict: every attribute the check judges is
//! read whole or the whole read fails, an attribute the app lacks is told
//! apart from one that failed or answered something else, and each request
//! is bounded by what is left of one deadline. Only copies are made;
//! nothing is set but each element's own messaging timeout.
use super::{
    ABSENT, Element, Frame, ffi,
    text::{bounded_string, getter_failed},
};
use crate::error::{Code, Error, Result};
use objc2_application_services::{AXError, AXUIElement, AXValue, AXValueType};
use objc2_core_foundation::{CFBoolean, CFNull, CFRetained, CFString, CFType};
use serde::Serialize;
use std::time::{Duration, Instant};

/// Most steps from the focused element up to its window. No app nests a
/// field this deep; the read's deadline ends a long walk well before it.
const PARENTS: usize = 256;

/// The element with keyboard focus in an app and the process that owns
/// it, which for a panel another process draws is not the app's.
#[derive(Debug, Clone)]
pub(crate) struct Focused {
    pub(crate) element: Element,
    pub(crate) pid: i64,
}

/// Where the focused element sits, from its parents up to a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Within {
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
pub(crate) struct Keyboard {
    /// The app's focused window: its role, subrole, title and frame. None
    /// when the app has none, or when no window is judged.
    pub(crate) window: Option<Element>,
    /// The focused element: its role, subrole, names and flags. None when
    /// nothing in the app has focus, or when no field is judged.
    pub(crate) focused: Option<Focused>,
    /// Where the focused element sits; [`Within::Window`] when there is
    /// none.
    pub(crate) within: Within,
}

/// Why Atlas could not tell where keyboard focus is, as a refusal names
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Unread {
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
    /// The error a read that could not tell, for this reason, ends with.
    pub(crate) fn error(self) -> Error {
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
        Error::new(Code::Native, said).with("unread", self)
    }
}

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
pub(crate) struct Judged {
    pub(crate) window: bool,
    pub(crate) field: bool,
}

/// One strict read, until its deadline.
pub(super) struct Reader<A> {
    pub(super) ask: A,
    pub(super) deadline: Instant,
}

impl<A: Ask> Reader<A> {
    /// What is left of the deadline; an error once it is spent.
    pub(super) fn left(&self) -> Result<Duration> {
        let left = self.deadline.saturating_duration_since(self.ask.now());
        if left.is_zero() {
            return Err(Unread::Slow.error());
        }

        Ok(left)
    }

    /// One attribute as the app answered it, its request bounded by what
    /// is left just before it is sent. A request that could not be bounded,
    /// or that ended past the deadline, is an error.
    fn asked(
        &mut self,
        element: &A::Element,
        attribute: &'static str,
    ) -> Result<Answer<A::Element>> {
        let left = self.left()?;
        if !self
            .ask
            .bound(element, left.min(Duration::from_millis(250)))
        {
            return Err(Unread::Failed.error());
        }

        let answer = self.ask.ask(element, attribute);
        self.left()?;
        Ok(answer)
    }

    /// [`Reader::asked`], a failed request an error too.
    fn answer(
        &mut self,
        element: &A::Element,
        attribute: &'static str,
    ) -> Result<Answer<A::Element>> {
        match self.asked(element, attribute)? {
            Answer::Failed(_) => Err(Unread::Failed.error()),
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
        failed: &mut Vec<&'static str>,
    ) -> Result<Option<String>> {
        match self.asked(element, attribute)? {
            Answer::Failed(status) if getter_failed(status, attribute) => {
                failed.push(attribute);
                Ok(None)
            }
            Answer::Failed(_) => Err(Unread::Failed.error()),
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
            _ => Err(Unread::Malformed.error()),
        }
    }

    pub(super) fn reference(
        &mut self,
        element: &A::Element,
        attribute: &'static str,
    ) -> Result<Option<A::Element>> {
        reference(self.answer(element, attribute)?)
    }

    /// The frame from position and size, both there or both absent.
    fn frame(&mut self, element: &A::Element) -> Result<Option<Frame>> {
        let position = self.answer(element, "AXPosition")?;
        let size = self.answer(element, "AXSize")?;
        match (position, size) {
            (Answer::Absent, Answer::Absent) => Ok(None),
            (Answer::Point(x, y), Answer::Size(width, height)) => Ok(Some(Frame {
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
            ..bare()
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
    pub(super) fn field_names(
        &mut self,
        element: &A::Element,
    ) -> Result<(Element, Vec<&'static str>)> {
        let mut failed = Vec::new();
        let field = Element {
            role: self.text(element, "AXRole")?,
            subrole: self.text(element, "AXSubrole")?,
            title: self.name(element, "AXTitle", &mut failed)?,
            description: self.name(element, "AXDescription", &mut failed)?,
            identifier: self.name(element, "AXIdentifier", &mut failed)?,
            enabled: self.flag(element, "AXEnabled")?,
            focused: self.flag(element, "AXFocused")?,
            ..bare()
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
            if let Step::Found(within) = place(at_window, role.as_deref()) {
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

/// An element with nothing read yet.
fn bare() -> Element {
    Element {
        index: 0,
        depth: 0,
        window: None,
        role: None,
        subrole: None,
        title: None,
        description: None,
        value: None,
        identifier: None,
        enabled: None,
        focused: None,
        selected: None,
        frame: None,
        unreadable: Vec::new(),
        truncated: Vec::new(),
        menu: false,
        mark: None,
    }
}

/// The text an answer holds: none when absent, an error when it is cut
/// short or anything but text.
fn text<E>(answer: Answer<E>) -> Result<Option<String>> {
    match answer {
        Answer::Absent => Ok(None),
        Answer::Text(text) => Ok(Some(text)),
        Answer::Cut => Err(Unread::Cut.error()),
        _ => Err(Unread::Malformed.error()),
    }
}

/// The element an answer names: none when absent, an error when it names
/// anything but an element.
fn reference<E>(answer: Answer<E>) -> Result<Option<E>> {
    match answer {
        Answer::Absent => Ok(None),
        Answer::Element(element) => Ok(Some(element)),
        _ => Err(Unread::Malformed.error()),
    }
}

/// What one step of the walk up from the focused element says.
#[derive(Debug, PartialEq)]
enum Step {
    /// Where the focused element sits.
    Found(Within),
    /// Nothing yet: go on to the parent.
    Up,
}

/// The walk at one element: the focused window itself, a sheet before it,
/// another window or the app at the top, or anything else to pass.
fn place(at_window: bool, role: Option<&str>) -> Step {
    if at_window {
        return Step::Found(Within::Window);
    }

    match role {
        Some("AXSheet") => Step::Found(Within::Sheet),
        Some("AXWindow" | "AXApplication") => Step::Found(Within::Elsewhere),
        _ => Step::Up,
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
    let element = reader.reference(app, "AXFocusedUIElement")?;
    if judged.window {
        if let Some(element) = &element
            && let Some(focused) = reader.focused_window(element.clone(), window.as_ref())?
        {
            let owner = reader
                .ask
                .owner(app)
                .ok_or_else(|| Unread::Failed.error())?;
            if reader.ask.owner(&focused) != Some(owner) {
                return Err(Unread::Failed.error());
            }
            window = Some(focused);
        }
        if let Some(window) = &window {
            let owner = reader
                .ask
                .owner(app)
                .ok_or_else(|| Unread::Failed.error())?;
            if reader.ask.owner(window) != Some(owner) {
                return Err(Unread::Failed.error());
            }
            keyboard.window = Some(reader.window(window)?);
        }
    }
    let Some(element) = element else {
        return Ok(keyboard);
    };
    if judged.field {
        let pid = reader
            .ask
            .owner(&element)
            .ok_or_else(|| Unread::Failed.error())?;
        keyboard.focused = Some(Focused {
            element: reader.field(&element)?,
            pid,
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
pub(crate) fn keyboard(pid: i64, timeout: Duration, judged: Judged) -> Result<Keyboard> {
    let deadline = Instant::now() + timeout;
    let pid = libc::pid_t::try_from(pid).map_err(|_| Unread::Failed.error())?;
    let app = ffi::application(pid).ok_or_else(|| Unread::Failed.error())?;

    read(Live, &app, deadline, judged)
}

/// Some apps omit the system-wide focused application. Their own frontmost
/// flag and an owned focus reference must both be present to substitute.
pub(crate) fn frontmost(pid: i64, timeout: Duration) -> Result<bool> {
    let deadline = Instant::now() + timeout;
    let pid = libc::pid_t::try_from(pid).map_err(|_| Unread::Failed.error())?;
    let app = ffi::application(pid).ok_or_else(|| Unread::Failed.error())?;
    app_focus(
        &mut Reader {
            ask: Live,
            deadline,
        },
        &app,
        i64::from(pid),
    )
}

fn app_focus<A: Ask>(reader: &mut Reader<A>, app: &A::Element, pid: i64) -> Result<bool> {
    if reader.ask.owner(app) != Some(pid) || reader.flag(app, "AXFrontmost")? != Some(true) {
        return Ok(false);
    }
    for attribute in ["AXFocusedWindow", "AXFocusedUIElement"] {
        if let Some(element) = reader.reference(app, attribute)? {
            if reader.ask.owner(&element) != Some(pid) {
                return Err(Unread::Failed.error());
            }
            match reader.text(&element, "AXRole")?.as_deref() {
                Some("AXApplication") => return Err(Unread::Withheld.error()),
                Some("AXWindow" | "AXSheet") => {}
                Some(_) if attribute == "AXFocusedUIElement" => {}
                _ => return Err(Unread::Malformed.error()),
            }
            return Ok(true);
        }
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
            Err(status) if ABSENT.contains(&status) => Answer::Absent,
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
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// The app's element in a test app.
    const APP: usize = 0;
    const WINDOW: usize = 1;
    const SHEET: usize = 2;
    const GROUP: usize = 3;
    const FIELD: usize = 4;

    /// A test app: answers by element and attribute, each taking its time
    /// on a clock that moves only as it answers. It keeps every timeout it
    /// was asked to set, and the attributes asked for.
    struct Fake {
        started: Instant,
        elapsed: Duration,
        answers: HashMap<(usize, &'static str), (Answer<usize>, Duration)>,
        bounds: Vec<Duration>,
        asked: Vec<(usize, &'static str)>,
        bounded: bool,
        owners: HashMap<usize, i64>,
    }

    impl Fake {
        /// A window holding a group holding a field named Email, focused.
        fn app() -> Self {
            let mut fake = Self {
                started: Instant::now(),
                elapsed: Duration::ZERO,
                answers: HashMap::new(),
                bounds: Vec::new(),
                asked: Vec::new(),
                bounded: true,
                owners: HashMap::new(),
            };
            let text = |text: &str| Answer::Text(text.to_owned());
            fake.set(APP, "AXFocusedWindow", Answer::Element(WINDOW));
            fake.set(APP, "AXFocusedUIElement", Answer::Element(FIELD));
            fake.set(WINDOW, "AXRole", text("AXWindow"));
            fake.set(WINDOW, "AXSubrole", text("AXStandardWindow"));
            fake.set(WINDOW, "AXTitle", text("Letter.txt"));
            fake.set(WINDOW, "AXPosition", Answer::Point(100.0, 100.0));
            fake.set(WINDOW, "AXSize", Answer::Size(400.0, 300.0));
            fake.set(FIELD, "AXRole", text("AXTextField"));
            fake.set(FIELD, "AXDescription", text("Email"));
            fake.set(FIELD, "AXEnabled", Answer::Flag(true));
            fake.set(FIELD, "AXFocused", Answer::Flag(true));
            fake.set(FIELD, "AXParent", Answer::Element(GROUP));
            fake.set(GROUP, "AXRole", text("AXGroup"));
            fake.set(GROUP, "AXParent", Answer::Element(WINDOW));
            fake
        }

        fn set(&mut self, element: usize, attribute: &'static str, answer: Answer<usize>) {
            self.answers
                .insert((element, attribute), (answer, Duration::from_millis(1)));
        }

        /// Make `attribute` of `element` take `delay` to answer.
        fn slow(&mut self, element: usize, attribute: &'static str, delay: Duration) {
            if let Some(entry) = self.answers.get_mut(&(element, attribute)) {
                entry.1 = delay;
            }
        }
    }

    impl Ask for &mut Fake {
        type Element = usize;

        fn now(&self) -> Instant {
            self.started + self.elapsed
        }

        fn bound(&mut self, _: &usize, timeout: Duration) -> bool {
            self.bounds.push(timeout);
            self.bounded
        }

        fn ask(&mut self, element: &usize, attribute: &'static str) -> Answer<usize> {
            self.asked.push((*element, attribute));
            let (answer, delay) = self
                .answers
                .get(&(*element, attribute))
                .cloned()
                .unwrap_or((Answer::Absent, Duration::from_millis(1)));
            self.elapsed += delay;
            answer
        }

        fn owner(&mut self, element: &usize) -> Option<i64> {
            Some(*self.owners.get(element).unwrap_or(&812))
        }
    }

    /// Both the window and the field judged.
    const BOTH: Judged = Judged {
        window: true,
        field: true,
    };

    /// A read of `fake` with `budget` to spend, judging both.
    fn read_with(fake: &mut Fake, budget: Duration) -> Result<Keyboard> {
        let deadline = fake.started + budget;
        read(&mut *fake, &APP, deadline, BOTH)
    }

    fn unread(outcome: Result<Keyboard>) -> serde_json::Value {
        outcome.unwrap_err().details["unread"].clone()
    }

    fn budget() -> Duration {
        Duration::from_millis(300)
    }

    #[test]
    fn a_focused_field_is_read_whole_with_where_it_sits() {
        let mut fake = Fake::app();
        let keyboard = read_with(&mut fake, budget()).unwrap();
        let window = keyboard.window.unwrap();
        assert_eq!(window.subrole.as_deref(), Some("AXStandardWindow"));
        assert_eq!(
            window.frame,
            Some(Frame {
                x: 100.0,
                y: 100.0,
                width: 400.0,
                height: 300.0
            })
        );
        let focused = keyboard.focused.unwrap();
        assert_eq!(focused.pid, 812);
        assert_eq!(focused.element.description.as_deref(), Some("Email"));
        assert_eq!(focused.element.enabled, Some(true));
        assert_eq!(keyboard.within, Within::Window);
        assert!(
            !fake
                .asked
                .iter()
                .any(|(_, attribute)| *attribute == "AXValue"),
            "no value is ever read"
        );
    }

    #[test]
    fn every_request_is_bounded_by_what_is_left_just_before_it() {
        let mut fake = Fake::app();
        fake.slow(WINDOW, "AXTitle", Duration::from_millis(120));
        read_with(&mut fake, budget()).unwrap();

        assert_eq!(fake.bounds.len(), fake.asked.len(), "one bound per request");
        assert_eq!(fake.bounds[0], Duration::from_millis(250));
        assert!(
            fake.bounds.windows(2).all(|pair| pair[1] <= pair[0]),
            "each bound is what is left, never the first one again: {:?}",
            fake.bounds
        );
        let after_title = fake
            .asked
            .iter()
            .position(|asked| *asked == (WINDOW, "AXTitle"))
            .unwrap();
        assert!(fake.bounds[after_title + 1] < budget() - Duration::from_millis(120));
    }

    #[test]
    fn a_read_that_runs_past_its_deadline_is_refused_as_slow() {
        let mut fake = Fake::app();
        fake.slow(FIELD, "AXDescription", Duration::from_millis(400));
        assert_eq!(unread(read_with(&mut fake, budget())), "slow");
        assert_eq!(
            fake.asked.last(),
            Some(&(FIELD, "AXDescription")),
            "nothing more is asked once the time is spent"
        );
    }

    #[test]
    fn a_timeout_that_cannot_be_set_stops_the_read_at_once() {
        let mut fake = Fake::app();
        fake.bounded = false;
        assert_eq!(unread(read_with(&mut fake, budget())), "failed");
        assert!(fake.asked.is_empty(), "nothing is asked unbounded");
    }

    #[test]
    fn a_failed_read_of_anything_judged_refuses_where_absence_would_not() {
        for (element, attribute) in [
            (WINDOW, "AXRole"),
            (WINDOW, "AXSubrole"),
            (WINDOW, "AXTitle"),
            (WINDOW, "AXSize"),
            (FIELD, "AXRole"),
            (FIELD, "AXTitle"),
            (FIELD, "AXIdentifier"),
            (FIELD, "AXEnabled"),
            (FIELD, "AXFocused"),
            (GROUP, "AXParent"),
        ] {
            let mut failed = Fake::app();
            failed.set(element, attribute, Answer::Failed(AXError::CannotComplete));
            assert_eq!(
                unread(read_with(&mut failed, budget())),
                "failed",
                "{element} {attribute}"
            );
        }

        for attribute in ["AXRole", "AXEnabled", "AXFocused"] {
            let mut failed = Fake::app();
            failed.set(FIELD, attribute, Answer::Failed(AXError::Failure));
            assert_eq!(
                unread(read_with(&mut failed, budget())),
                "failed",
                "{attribute}: a failed getter of anything judged against refuses"
            );
        }

        let mut absent = Fake::app();
        absent.set(FIELD, "AXEnabled", Answer::Absent);
        let keyboard = read_with(&mut absent, budget()).unwrap();
        assert_eq!(
            keyboard.focused.unwrap().element.enabled,
            None,
            "an app that does not say is not taken to say enabled or not"
        );
    }

    /// An AppKit text view answers its description with a plain failure
    /// every time: that is no name, not a refusal, since a name can only
    /// make a field match.
    #[test]
    fn a_name_whose_getter_fails_outright_is_no_name() {
        let mut fake = Fake::app();
        fake.set(FIELD, "AXDescription", Answer::Failed(AXError::Failure));
        fake.set(
            FIELD,
            "AXIdentifier",
            Answer::Text("First Text View".to_owned()),
        );
        let focused = read_with(&mut fake, budget()).unwrap().focused.unwrap();
        assert_eq!(focused.element.description, None);
        assert_eq!(
            focused.element.identifier.as_deref(),
            Some("First Text View")
        );

        let mut stalled = Fake::app();
        stalled.set(
            FIELD,
            "AXDescription",
            Answer::Failed(AXError::CannotComplete),
        );
        assert_eq!(unread(read_with(&mut stalled, budget())), "failed");
    }

    #[test]
    fn a_read_asks_for_what_is_judged_and_nothing_more() {
        let mut window_only = Fake::app();
        window_only.set(
            FIELD,
            "AXDescription",
            Answer::Failed(AXError::CannotComplete),
        );
        let deadline = window_only.started + budget();
        let keyboard = read(
            &mut window_only,
            &APP,
            deadline,
            Judged {
                window: true,
                field: false,
            },
        )
        .unwrap();
        assert!(keyboard.focused.is_none());
        assert_eq!(keyboard.within, Within::Window);
        assert!(
            !window_only.asked.contains(&(FIELD, "AXDescription")),
            "the field's names are not asked for"
        );

        let mut field_only = Fake::app();
        field_only.set(WINDOW, "AXTitle", Answer::Failed(AXError::CannotComplete));
        let deadline = field_only.started + budget();
        let keyboard = read(
            &mut field_only,
            &APP,
            deadline,
            Judged {
                window: false,
                field: true,
            },
        )
        .unwrap();
        assert!(keyboard.window.is_none());
        assert!(keyboard.focused.is_some());
        assert!(
            !field_only
                .asked
                .iter()
                .any(|(element, _)| *element == WINDOW),
            "the window is not asked about"
        );
    }

    #[test]
    fn an_answer_of_the_wrong_kind_or_cut_short_refuses() {
        let mut wrong = Fake::app();
        wrong.set(FIELD, "AXEnabled", Answer::Text("yes".to_owned()));
        assert_eq!(unread(read_with(&mut wrong, budget())), "malformed");

        let mut half = Fake::app();
        half.set(WINDOW, "AXSize", Answer::Absent);
        assert_eq!(unread(read_with(&mut half, budget())), "malformed");

        let mut cut = Fake::app();
        cut.set(WINDOW, "AXTitle", Answer::Cut);
        assert_eq!(unread(read_with(&mut cut, budget())), "cut");
    }

    #[test]
    fn a_sheet_between_the_field_and_its_window_is_found_on_the_way_up() {
        let mut fake = Fake::app();
        fake.set(GROUP, "AXParent", Answer::Element(SHEET));
        fake.set(SHEET, "AXRole", Answer::Text("AXSheet".to_owned()));
        fake.set(SHEET, "AXPosition", Answer::Point(80.0, 120.0));
        fake.set(SHEET, "AXSize", Answer::Size(500.0, 180.0));
        let keyboard = read_with(&mut fake, budget()).unwrap();
        assert_eq!(keyboard.within, Within::Window);
        let window = keyboard.window.unwrap();
        assert_eq!(window.role.as_deref(), Some("AXSheet"));
        assert_eq!(window.frame.unwrap().width, 500.0);

        let mut orphan = Fake::app();
        orphan.set(GROUP, "AXParent", Answer::Absent);
        assert_eq!(
            read_with(&mut orphan, budget()).unwrap().within,
            Within::Elsewhere
        );
    }

    #[test]
    fn a_walk_longer_than_any_app_nests_is_refused_as_too_deep() {
        let mut fake = Fake::app();
        fake.set(GROUP, "AXParent", Answer::Element(GROUP));
        for answer in fake.answers.values_mut() {
            answer.1 = Duration::ZERO;
        }
        assert_eq!(unread(read_with(&mut fake, budget())), "deep");
    }

    #[test]
    fn the_walk_stops_at_its_window_a_sheet_or_another_window() {
        assert_eq!(place(true, Some("AXSheet")), Step::Found(Within::Window));
        assert_eq!(place(false, Some("AXSheet")), Step::Found(Within::Sheet));
        for role in ["AXWindow", "AXApplication"] {
            assert_eq!(place(false, Some(role)), Step::Found(Within::Elsewhere));
        }
        for role in [Some("AXGroup"), Some("AXScrollArea"), None] {
            assert_eq!(place(false, role), Step::Up);
        }
    }

    #[test]
    fn a_reference_is_an_element_or_absent_and_nothing_else() {
        assert_eq!(reference(Answer::Element(7)).unwrap(), Some(7));
        assert_eq!(reference::<usize>(Answer::Absent).unwrap(), None);
        for answer in [
            Answer::Text("x".to_owned()),
            Answer::Flag(true),
            Answer::Other,
        ] {
            let error = reference::<usize>(answer).unwrap_err();
            assert_eq!(error.details["unread"], "malformed");
        }
    }
    #[test]
    fn direct_app_focus_requires_both_frontmost_and_an_owned_focus_reference() {
        for attribute in ["AXFocusedWindow", "AXFocusedUIElement"] {
            let mut fake = Fake::app();
            fake.set(APP, "AXFrontmost", Answer::Flag(true));
            fake.set(APP, "AXFocusedWindow", Answer::Absent);
            fake.set(APP, "AXFocusedUIElement", Answer::Absent);
            fake.set(APP, attribute, Answer::Element(WINDOW));
            let deadline = fake.started + budget();
            assert!(
                app_focus(
                    &mut Reader {
                        ask: &mut fake,
                        deadline
                    },
                    &APP,
                    812
                )
                .unwrap()
            );
            fake.owners.insert(WINDOW, 999);
            assert!(
                app_focus(
                    &mut Reader {
                        ask: &mut fake,
                        deadline
                    },
                    &APP,
                    812
                )
                .is_err()
            );
        }
        for frontmost in [Answer::Absent, Answer::Flag(false), Answer::Flag(true)] {
            let mut fake = Fake::app();
            fake.set(APP, "AXFrontmost", frontmost);
            fake.set(APP, "AXFocusedWindow", Answer::Absent);
            fake.set(APP, "AXFocusedUIElement", Answer::Absent);
            let deadline = fake.started + budget();
            assert!(
                !app_focus(
                    &mut Reader {
                        ask: &mut fake,
                        deadline
                    },
                    &APP,
                    812
                )
                .unwrap()
            );
        }
        let mut fake = Fake::app();
        fake.set(APP, "AXFrontmost", Answer::Failed(AXError::CannotComplete));
        let deadline = fake.started + budget();
        assert!(
            app_focus(
                &mut Reader {
                    ask: &mut fake,
                    deadline
                },
                &APP,
                812
            )
            .is_err()
        );
    }
    #[test]
    fn a_focused_element_proves_its_window_when_the_app_omits_the_window_attribute() {
        let mut fake = Fake::app();
        fake.set(APP, "AXFocusedWindow", Answer::Absent);
        let keyboard = read_with(&mut fake, budget()).unwrap();
        assert_eq!(keyboard.window.unwrap().role.as_deref(), Some("AXWindow"));
        assert_eq!(keyboard.within, Within::Window);
    }
}
