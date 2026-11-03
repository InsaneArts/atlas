//! Whether an agent's action would land in the app, window and field it
//! names, judged inside the helper just before the action is sent: text
//! and keys need the app, and the window and field when named, to have
//! keyboard focus; pointer actions need the app's own window, or the named
//! one, to be the top one at the point. Preparation requests focus first;
//! the final verifier independently proves where input would land.
use super::ax::{self, Focused, Judged, Keyboard, Unread, Within};
use crate::native::windows as native_windows;
use atlas_core::{
    actions::Expect,
    error::{Code, Error, Result},
    screen::{
        elements::Element,
        select::{ElementFilter, shown_name},
        windows::{self, Bounds, Dock, Listed, Screen, Window},
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fmt::Display,
    time::{Duration, Instant},
};

/// The longest one destination check may take before input is posted.
pub(super) const CHECK_BUDGET: Duration = Duration::from_millis(500);

/// The total focus allowance per input batch, including preparation and settling.
pub(crate) const PREPARE_BUDGET: Duration = Duration::from_millis(1250);

/// [`PREPARE_BUDGET`], for the test that holds it to the client's reply timeout.
#[cfg(any(test, feature = "test-support"))]
pub const fn prepare_budget() -> Duration {
    PREPARE_BUDGET
}

/// The longest one Accessibility question may take.
const FOCUS_TIMEOUT: Duration = Duration::from_millis(250);

/// The longest the reads of where an app's keyboard focus is may take in
/// all: its focused window, its focused element and the way between them.
const KEYBOARD_TIMEOUT: Duration = Duration::from_millis(300);

/// The roles of controls that take typed text themselves. Anything else
/// reported as focused (a window or a web page while focus moves between
/// fields or sits in a custom view, a button, a tab group, an unknown
/// element) is no field, whatever its name.
pub(super) const FIELDS: [&str; 5] = [
    "AXTextField",
    "AXTextArea",
    "AXComboBox",
    "AXSearchField",
    "AXSecureTextField",
];

/// Roles that hold other elements, whose names are titles (a document's,
/// a page's) rather than a field's: a refusal never shows them.
const CONTAINERS: [&str; 9] = [
    "AXApplication",
    "AXWindow",
    "AXSheet",
    "AXDrawer",
    "AXWebArea",
    "AXScrollArea",
    "AXSplitGroup",
    "AXGroup",
    "AXLayoutArea",
];

/// How every refusal ends: nothing of the refused action was posted.
const NOT_SENT: &str = "this action and the rest were not sent";

/// The neutral action destination, used here for focus admission.
pub(super) use atlas_core::actions::Target as Need;

/// Where on its way a pointer action is checked.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Spot {
    /// Where it presses, scrolls or moves.
    At,
    /// Where a drag drops what it carries.
    Drop,
}

impl Spot {
    fn words(self) -> &'static str {
        match self {
            Self::At => "that point",
            Self::Drop => "the drop point",
        }
    }
}

/// Why an action was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Reason {
    /// Another app, or none, has keyboard focus.
    App,
    /// Another app's window, or none, is on top at the point.
    Point,
    /// Another window of the app, or none, has keyboard focus or is on top
    /// at the point.
    Window,
    /// A sheet over the window has keyboard focus.
    Sheet,
    /// Another field, or none, has keyboard focus.
    Field,
    /// The field has keyboard focus but is disabled.
    Disabled,
    /// What is in front could not be told; `unread` says why when the app
    /// was.
    Unreadable,
}

impl Reason {
    /// What is wrong, when nothing more particular is known.
    fn fact(self) -> &'static str {
        match self {
            Self::App => "The target app is not in front or has quit",
            Self::Point => "The app's window is not on top at that point",
            Self::Window => "The named window is not in front",
            Self::Sheet => "A sheet or dialog is over the target window",
            Self::Field => "The named field does not have keyboard focus or cannot be told apart",
            Self::Disabled => "That field is disabled",
            Self::Unreadable => "Atlas could not tell what is in front",
        }
    }

    /// What to do about it.
    fn remedy(self) -> &'static str {
        match self {
            Self::App => "bring it to the front",
            Self::Point => "bring it there or pick another point",
            Self::Window => "bring it to the front or look again",
            Self::Sheet => "answer it first",
            Self::Field => "click it or give its exact name",
            Self::Disabled => "enable it or name another field",
            Self::Unreadable => "click where the app shows and try again",
        }
    }

    /// Why `error` refused the action, when it says.
    pub(super) fn of(error: &Error) -> Option<Self> {
        Self::deserialize(error.details.get("reason")?).ok()
    }

    /// Whether a focus request may yet end this refusal by taking effect.
    pub(super) fn may_settle(self) -> bool {
        match self {
            Self::App | Self::Point | Self::Window | Self::Field => true,
            Self::Sheet | Self::Disabled | Self::Unreadable => false,
        }
    }
}

/// The focused element found where the expected field should have been:
/// its role and the name `atlas elements` shows for it, unless it is a
/// container whose name is a title. Never its value, which may be what
/// someone typed.
#[derive(Debug, Serialize)]
struct Field {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

impl Field {
    fn of(element: &Element) -> Self {
        let container = element
            .role
            .as_deref()
            .is_some_and(|role| CONTAINERS.contains(&role));
        Self {
            role: element.role.clone(),
            name: (!container)
                .then(|| shown_name(element).map(str::to_owned))
                .flatten(),
        }
    }
}

/// The app found where the expected one should have been: its process and,
/// when a window of it is listed, its name; with a window or field check,
/// also the window's number and the focused element. Never a window's
/// title, which may be a document's.
#[derive(Debug, Serialize)]
struct Found {
    #[serde(skip_serializing_if = "Option::is_none")]
    app: Option<String>,
    pid: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    window: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    field: Option<Field>,
}

impl Found {
    /// Process `pid`, named by the owner of any of its listed windows.
    fn named(pid: i64, windows: &[Window]) -> Self {
        Self {
            app: windows
                .iter()
                .find(|window| window.pid == pid)
                .and_then(|window| window.owner.clone()),
            pid,
            window: None,
            field: None,
        }
    }

    /// The owner of `window`, and the window itself.
    fn at(window: &Window) -> Self {
        Self {
            app: window.owner.clone(),
            pid: window.pid,
            window: Some(window.number),
            field: None,
        }
    }

    /// Whose the found app is, as a refusal names it.
    fn whose(&self) -> String {
        match &self.app {
            Some(name) => format!("{name}'s"),
            None => "Another app's".to_owned(),
        }
    }
}

/// What the helper shows, read only as far as a check needs it.
trait Look {
    /// The monotonic time of this observation, including time spent reading.
    fn now(&self) -> Instant {
        Instant::now()
    }

    /// The process with keyboard focus; None when no app has it.
    fn focused(&mut self) -> Result<Option<i64>>;
    /// Direct app evidence when the system-wide focus attribute is absent.
    fn app_focused(&mut self, pid: i64) -> Result<bool>;
    /// Where keyboard focus is inside process `pid`, as far as what is
    /// `judged` needs.
    fn keyboard(&mut self, pid: i64, judged: Judged) -> Result<Keyboard>;
    /// On-screen windows, front to back.
    fn windows(&mut self) -> Result<&[Window]>;
    /// Where the Dock takes clicks; [`Dock::Unread`] when that cannot be
    /// read in time.
    fn dock(&mut self) -> Dock;
}

/// The live helper, read until `deadline`, its window list and its Dock at
/// most once.
struct Live {
    deadline: Instant,
    windows: Option<Vec<Window>>,
    dock: Option<Dock>,
}

impl Live {
    /// A look that asks nothing once `deadline` has passed.
    fn until(deadline: Instant) -> Self {
        Self {
            deadline,
            windows: None,
            dock: None,
        }
    }

    /// The time left to look, or an error once it is spent.
    fn left(&self) -> Result<Duration> {
        ax::remaining(self.deadline, Instant::now())
    }
}

impl Look for Live {
    /// Keyboard focus as Accessibility reports it: the one place the
    /// focused app is read from.
    fn focused(&mut self) -> Result<Option<i64>> {
        let left = self.left()?;
        ax::focused_pid(left.min(FOCUS_TIMEOUT))
    }

    fn app_focused(&mut self, pid: i64) -> Result<bool> {
        let left = self.left()?;
        ax::frontmost(pid, left.min(FOCUS_TIMEOUT))
    }

    /// Read from the app element of the process judged to have focus, so
    /// the app, window and field checks look at the same app.
    fn keyboard(&mut self, pid: i64, judged: Judged) -> Result<Keyboard> {
        let left = self.left()?;
        ax::keyboard(pid, left.min(KEYBOARD_TIMEOUT), judged)
    }

    fn windows(&mut self) -> Result<&[Window]> {
        if self.windows.is_none() {
            self.left()?;
            self.windows = Some(native_windows::windows()?);
        }

        Ok(self.windows.as_deref().unwrap_or_default())
    }

    /// The Dock as Accessibility reports it, from the Dock that owns a
    /// window in the list, in what time is left once the list is read.
    fn dock(&mut self) -> Dock {
        if let Some(dock) = &self.dock {
            return dock.clone();
        }

        let deadline = self.deadline;
        let dock = self
            .windows()
            .and_then(|listed| {
                let left = ax::remaining(deadline, Instant::now())?;
                Ok(ax::dock(listed, left.min(ax::DOCK_TIMEOUT)))
            })
            .unwrap_or(Dock::Unread);
        self.dock = Some(dock.clone());
        dock
    }
}

/// [`require`] against the live helper, taking until `deadline` at most.
pub(super) fn check(
    expect: &Expect,
    need: Need,
    display: &Bounds,
    deadline: Instant,
) -> Result<Option<&'static str>> {
    let deadline = deadline.min(Instant::now() + CHECK_BUDGET);
    require(expect, need, display, &mut Live::until(deadline), deadline)
}

/// Prepare once, then poll with fresh observations. Resolution and writes
/// leave time for a final read within the same request's deadline.
pub(super) fn prepare(
    expect: &Expect,
    need: Need,
    display: &Bounds,
    deadline: Instant,
    guard: impl Fn() -> Result<()>,
) -> Result<Option<&'static str>> {
    if need == Need::Nothing {
        return Ok(None);
    }
    ax::bring(expect, need, deadline, &guard, |number, until| {
        let selected = match number {
            Some(number) if expect.window() != Some(number) => expect.clone().in_window(number)?,
            _ => expect.clone(),
        };
        check(&selected, need, display, until)
    })
    .map_err(|error| {
        if error.code == Code::Native {
            unread_by(error)
        } else {
            error
        }
    })
}

/// Refuse with `focus-changed` unless `expect` holds for `need` as `look`
/// sees it now; once it does, the role of the field it names, one of
/// [`FIELDS`], when it names one. A look that ends after `deadline` proves
/// nothing: a read that stalled may describe a helper that has moved on
/// since.
fn require(
    expect: &Expect,
    need: Need,
    display: &Bounds,
    look: &mut impl Look,
    deadline: Instant,
) -> Result<Option<&'static str>> {
    let outcome = match need {
        Need::Nothing => return Ok(None),
        Need::Keys => active(expect, look).and_then(|()| inside(expect, look)),
        Need::Point(x, y) | Need::Pointer(x, y) => active(expect, look)
            .and_then(|()| point(expect, (x, y), Spot::At, display, look))
            .map(|()| None),
        Need::Drag { from, to } => active(expect, look)
            .and_then(|()| point(expect, from, Spot::At, display, look))
            .and_then(|()| point(expect, to, Spot::Drop, display, look))
            .map(|()| None),
    };
    let unread = outcome
        .as_ref()
        .is_err_and(|error| Reason::of(error) == Some(Reason::Unreadable));
    if (outcome.is_ok() || unread) && look.now() >= deadline {
        return Err(refused(Reason::Unreadable).with("unread", json!(Unread::Slow)));
    }

    outcome
}

/// Prove the target app is active, independently of its window focus.
fn active(expect: &Expect, look: &mut impl Look) -> Result<()> {
    let focused = look.focused().map_err(unreadable_from)?;
    if focused.is_none() && look.app_focused(expect.pid()).map_err(unread_by)? {
        return Ok(());
    }
    let Some(pid) = focused else {
        return Err(refused_because(Reason::App, "No app has keyboard focus"));
    };
    if pid == expect.pid() {
        return Ok(());
    }

    let found = Found::named(pid, look.windows().unwrap_or_default());
    let fact = match &found.app {
        Some(name) => format!("{name} has keyboard focus, not the app this was for"),
        None => "Another app has keyboard focus".to_owned(),
    };
    Err(refused_with(Reason::App, fact, found))
}

/// With the app in focus, the window and the field it names, judged from
/// one read of where the app's focus is; once they pass, the named field's
/// role.
fn inside(expect: &Expect, look: &mut impl Look) -> Result<Option<&'static str>> {
    if expect.window().is_none() && expect.field().is_none() {
        return Ok(None);
    }

    let judged = Judged {
        window: expect.window().is_some(),
        field: expect.field().is_some(),
    };
    let keyboard = look.keyboard(expect.pid(), judged).map_err(unread_by)?;
    let verdict = match expect.window() {
        Some(wanted) => {
            let listed = look.windows().map_err(unreadable_from)?;
            in_window(wanted, expect.pid(), &keyboard, listed)
        }
        None => Verdict::Pass,
    }
    .or_else(|| match expect.field() {
        Some(name) => in_field(name, expect.pid(), &keyboard),
        None => Verdict::Pass,
    });

    let reason = match verdict {
        Verdict::Pass if expect.field().is_some() => return Ok(field_role(&keyboard)),
        Verdict::Pass => return Ok(None),
        Verdict::Unknown(why) => return Err(unread_by(why.error())),
        Verdict::Refused(reason) => reason,
    };
    let listed = look.windows().unwrap_or_default();
    let mut found = Found::named(expect.pid(), listed);
    found.window =
        keyboard
            .window
            .as_ref()
            .and_then(|window| match number(listed, expect.pid(), window) {
                Listed::One(number) => Some(number),
                Listed::Missing | Listed::Several => None,
            });
    found.field = keyboard
        .focused
        .as_ref()
        .map(|focused| Field::of(&focused.element));
    let fact = match (reason, found.window, expect.window()) {
        (Reason::Window, Some(number), Some(wanted)) if number != wanted => {
            format!("Window {number} has keyboard focus, not window {wanted}")
        }
        _ => reason.fact().to_owned(),
    };
    Err(refused_with(reason, fact, found))
}

/// The role of the field with keyboard focus, as [`FIELDS`] spells it,
/// when it is one of them.
fn field_role(keyboard: &Keyboard) -> Option<&'static str> {
    let role = keyboard.focused.as_ref()?.element.role.as_deref()?;
    FIELDS.into_iter().find(|field| *field == role)
}

/// What one part of a check found.
#[derive(Debug, PartialEq)]
enum Verdict {
    /// What it checks is in place.
    Pass,
    /// It is not, for this reason.
    Refused(Reason),
    /// Whether it is could not be told, for this reason.
    Unknown(Unread),
}

impl Verdict {
    /// This verdict, or once it passes, the next part's.
    fn or_else(self, next: impl FnOnce() -> Self) -> Self {
        match self {
            Self::Pass => next(),
            other @ (Self::Refused(..) | Self::Unknown(_)) => other,
        }
    }
}

/// Whether window `wanted` of app `pid` has keyboard focus with no sheet
/// over it: the app's focused window is that window, the one listed window
/// it can be ([`number`]); it holds the focused element with no sheet
/// between them; and no shown window of the app's is in front of it at its
/// layer or above. A sheet named by its own number is a window like any
/// other.
fn in_window(wanted: i64, pid: i64, keyboard: &Keyboard, listed: &[Window]) -> Verdict {
    let Some(window) = &keyboard.window else {
        return Verdict::Refused(Reason::Window);
    };
    // The app itself in place of its window: it withholds its windows, as
    // apps do while the Mac's screen is locked, and no list can show one.
    if window.role.as_deref() == Some("AXApplication") {
        return Verdict::Unknown(Unread::Withheld);
    }
    let sheet = window.role.as_deref() == Some("AXSheet") || keyboard.within == Within::Sheet;

    match number(listed, pid, window) {
        Listed::One(number) if number == wanted => {
            if keyboard.within == Within::Sheet {
                return Verdict::Refused(Reason::Sheet);
            }
            if covered(listed, pid, number) {
                return Verdict::Refused(Reason::Window);
            }
            if keyboard.within == Within::Elsewhere {
                return Verdict::Unknown(Unread::Failed);
            }
            Verdict::Pass
        }
        _ if sheet => Verdict::Refused(Reason::Sheet),
        Listed::One(_) => Verdict::Refused(Reason::Window),
        Listed::Missing => Verdict::Unknown(Unread::Unlisted),
        Listed::Several => Verdict::Unknown(Unread::Alike),
    }
}

/// Whether the field named `name` has keyboard focus in app `pid`: the
/// focused element is the app's own, is a control that takes text itself
/// (see [`FIELDS`]), says it is focused, matches the name as `atlas
/// elements --name` would, and is enabled.
fn in_field(name: &str, pid: i64, keyboard: &Keyboard) -> Verdict {
    let Some(Focused {
        element,
        pid: owner,
    }) = &keyboard.focused
    else {
        return Verdict::Refused(Reason::Field);
    };

    let field = element
        .role
        .as_deref()
        .is_some_and(|role| FIELDS.contains(&role));
    if *owner != pid || !field || element.focused == Some(false) {
        return Verdict::Refused(Reason::Field);
    }

    let filter = ElementFilter {
        role: None,
        name: Some(name.to_owned()),
    };
    if filter.match_of(element).is_none() {
        return Verdict::Refused(Reason::Field);
    }
    if element.enabled == Some(false) {
        return Verdict::Refused(Reason::Disabled);
    }

    Verdict::Pass
}

/// Which listed window app `pid`'s focused `window` is, of every window of
/// the app; a window with no frame is not listed.
fn number(listed: &[Window], pid: i64, window: &Element) -> Listed {
    match &window.frame {
        Some(frame) => ax::described(window, frame, pid, listed),
        None => Listed::Missing,
    }
}

/// Whether a shown window of app `pid` is in front of window `number` and
/// overlaps it, at its layer or above: a sheet's own window, a popup, a
/// floating panel or an open menu, any of which may take the keys.
fn covered(listed: &[Window], pid: i64, number: i64) -> bool {
    let Some(index) = listed.iter().position(|window| window.number == number) else {
        return false;
    };
    let window = &listed[index];
    listed[..index].iter().any(|other| {
        other.pid == pid
            && other.layer >= window.layer
            && other.alpha > 0.0
            && other.bounds.intersects(&window.bounds)
    })
}

/// A pointer action goes ahead only where the app's own window, at any
/// layer, is uncovered at the point, the Dock covering what is under it
/// where it takes clicks. The active app owns the menu bar. With a window named,
/// only where that window is uncovered, judged where it presses: a drop
/// lands in the app's window if not the same one. Where the Dock could not
/// be read and is on top, what is in front is unreadable.
fn point(
    expect: &Expect,
    (x, y): (f64, f64),
    spot: Spot,
    display: &Bounds,
    look: &mut impl Look,
) -> Result<()> {
    let screen = Screen::new(display.clone(), look.dock());
    let listed = look.windows().map_err(unreadable_from)?;
    if let (Spot::At, Some(wanted)) = (spot, expect.window()) {
        return at_window(expect.pid(), wanted, (x, y), listed, &screen);
    }
    let top = windows::top_at(listed, x, y, &screen);
    if top.is_some_and(Window::is_menu_bar) || windows::reaches(listed, expect.pid(), x, y, &screen)
    {
        return Ok(());
    }

    let at = spot.words();
    let Some(top) = top else {
        let fact = format!("No window of the app is at {at}");
        return Err(refused_because(Reason::Point, fact));
    };
    readable_dock(top, &screen)?;

    let found = Found {
        window: None,
        ..Found::at(top)
    };
    let fact = format!("{} window is at {at}, not the app's", found.whose());
    Err(refused_with(Reason::Point, fact, found))
}

/// A pointer action at a point goes ahead only where window `wanted`, one
/// of app `pid`'s, is uncovered.
fn at_window(
    pid: i64,
    wanted: i64,
    (x, y): (f64, f64),
    listed: &[Window],
    screen: &Screen,
) -> Result<()> {
    if !listed
        .iter()
        .any(|window| window.number == wanted && window.pid == pid)
    {
        let fact = format!("Window {wanted} of the app is not on screen");
        return Err(refused_because(Reason::Window, fact));
    }
    if windows::unobstructed(listed, wanted, x, y, screen) {
        return Ok(());
    }

    let Some(top) = windows::top_at(listed, x, y, screen) else {
        let fact = format!("Window {wanted} is not at that point");
        return Err(refused_because(Reason::Window, fact));
    };
    readable_dock(top, screen)?;

    let found = Found::at(top);
    let (reason, whose) = if top.pid == pid {
        (Reason::Window, "The app's".to_owned())
    } else {
        (Reason::Point, found.whose())
    };
    let fact = format!(
        "{whose} window {} is at that point, not window {wanted}",
        top.number
    );
    Err(refused_with(reason, fact, found))
}

/// Refuse when the Dock is `top` and where it takes clicks could not be read.
fn readable_dock(top: &Window, screen: &Screen) -> Result<()> {
    if top.is_dock() && *screen.dock() == Dock::Unread {
        let fact = "Atlas could not read where the Dock takes clicks";
        return Err(refused_because(Reason::Unreadable, fact));
    }

    Ok(())
}

fn unreadable_from(error: Error) -> Error {
    refused(Reason::Unreadable).with_source(error)
}

/// The refusal for an app in front whose window or field could not be
/// told, naming why when the read's `error` says.
fn unread_by(error: Error) -> Error {
    let why = Unread::of(&error).unwrap_or(Unread::Failed);
    unreadable_from(error).with("unread", json!(why))
}

/// A `focus-changed` refusal for `reason`.
pub(super) fn refused(reason: Reason) -> Error {
    refused_because(reason, reason.fact())
}

/// [`refused`], saying what is wrong as `fact`.
fn refused_because(reason: Reason, fact: impl Display) -> Error {
    let message = format!("{fact}, so {}; {NOT_SENT}", reason.remedy());
    Error::new(Code::FocusChanged, message).with("reason", json!(reason))
}

/// [`refused_because`], naming what was `found` there.
fn refused_with(reason: Reason, fact: impl Display, found: Found) -> Error {
    refused_because(reason, fact).with("found", json!(found))
}

#[cfg(test)]
pub(super) mod tests;
