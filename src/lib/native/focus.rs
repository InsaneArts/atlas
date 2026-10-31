//! Whether an agent's action would land in the app, window and field it
//! names, judged inside the helper just before the action is sent: text
//! and keys need the app, and the window and field when named, to have
//! keyboard focus; pointer actions need the app's own window, or the named
//! one, to be the top one at the point. Preparation requests focus first;
//! the final verifier independently proves where input would land.
use super::{
    ax::{self, Element, Focused, Judged, Keyboard, Unread, Within},
    windows::{self, Bounds, Dock, Kind, Listed, Screen, Window},
};
use crate::{
    actions::{Action, Expect},
    error::{Code, Error, Result},
    select::{ElementFilter, shown_name},
};
use serde::Serialize;
use serde_json::json;
use std::time::{Duration, Instant};

/// The most time one request spends checking; past it, the rest is refused
/// rather than risk the controller's five-second deadline.
pub(super) const CHECK_BUDGET: Duration = Duration::from_millis(500);

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
const NOT_SENT: &str = "so this action and the rest were not sent";

/// What an action needs of its app before its first event.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Need {
    /// Keyboard focus.
    Keys,
    /// The app's own window on top at this point.
    Point(f64, f64),
    /// The app's own window on top where a drag starts and where it drops,
    /// so nothing is dropped on another app or the Dock.
    Drag { from: (f64, f64), to: (f64, f64) },
    /// Nothing: the action sends nothing an app acts on.
    Nothing,
}

impl Need {
    /// Type and Key need keys; Click, Scroll, Move and each pointer phase
    /// their point, since a move may carry what a press holds and a release
    /// drops it; Drag its start and its drop; Wait nothing.
    pub(super) fn of(action: &Action) -> Self {
        match action {
            Action::Type { .. } | Action::Key { .. } => Self::Keys,
            Action::Click { x, y, .. }
            | Action::Scroll { x, y, .. }
            | Action::Move { x, y }
            | Action::Pointer { x, y, .. } => Self::Point(*x, *y),
            Action::Drag {
                from_x,
                from_y,
                to_x,
                to_y,
                ..
            } => Self::Drag {
                from: (*from_x, *from_y),
                to: (*to_x, *to_y),
            },
            Action::Wait { .. } => Self::Nothing,
        }
    }
}

/// Where on its way a pointer action is checked, as a refusal names it.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Reason {
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
}

/// What the helper shows, read only as far as a check needs it.
trait Look {
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
        let left = self.deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(Unread::Slow.error());
        }

        Ok(left)
    }
}

impl Look for Live {
    /// Keyboard focus as Accessibility reports it: the one place the
    /// focused app is read from.
    fn focused(&mut self) -> Result<Option<i64>> {
        let left = self.left()?;
        super::ax::focused_pid(left.min(FOCUS_TIMEOUT))
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
            self.windows = Some(windows::windows()?);
        }

        Ok(self.windows.as_deref().unwrap_or_default())
    }

    /// The Dock as Accessibility reports it, from the Dock that owns a
    /// window in the list, in what time is left once the list is read.
    fn dock(&mut self) -> Dock {
        if self.dock.is_none() {
            let deadline = self.deadline;
            let dock = match self.windows() {
                Ok(listed) => match deadline.checked_duration_since(Instant::now()) {
                    Some(left) if !left.is_zero() => {
                        super::ax::dock(listed, left.min(super::ax::DOCK_TIMEOUT))
                    }
                    _ => Dock::Unread,
                },
                Err(_) => Dock::Unread,
            };
            self.dock = Some(dock);
        }

        self.dock.clone().unwrap_or_default()
    }
}

/// [`require`] against the live helper, taking until `deadline` at most.
pub(super) fn check(
    expect: &Expect,
    need: Need,
    display: &Bounds,
    deadline: Instant,
) -> Result<Option<&'static str>> {
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
        prepared(
            expect,
            need,
            number,
            display,
            &mut Live::until(until),
            until,
        )
    })
    .map_err(|error| {
        if error.code == Code::Native {
            unread_by(error)
        } else {
            error
        }
    })
}

/// A visible control may need a click just to activate its window. Prove
/// activation as well as reachability before skipping writes or settling.
fn prepared(
    expect: &Expect,
    need: Need,
    number: Option<i64>,
    display: &Bounds,
    look: &mut impl Look,
    deadline: Instant,
) -> Result<Option<&'static str>> {
    let selected = match number {
        Some(number) if expect.window() != Some(number) => expect.clone().in_window(number)?,
        _ => expect.clone(),
    };
    if matches!(need, Need::Point(..) | Need::Drag { .. }) {
        require(&selected, Need::Keys, display, look, deadline)?;
    }
    require(&selected, need, display, look, deadline)
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
        Need::Keys => keys(expect, look),
        Need::Point(x, y) => point(expect, (x, y), Spot::At, display, look).map(|()| None),
        Need::Drag { from, to } => point(expect, from, Spot::At, display, look)
            .and_then(|()| point(expect, to, Spot::Drop, display, look))
            .map(|()| None),
    };
    let unread = outcome.as_ref().is_err_and(|error| {
        error
            .details
            .get("reason")
            .is_some_and(|why| why == "unreadable")
    });
    if (outcome.is_ok() || unread) && Instant::now() >= deadline {
        return Err(unreadable().with("unread", Unread::Slow));
    }

    outcome
}

/// Keys go ahead only while the app has keyboard focus and, when named,
/// the window and the field, whose role is then said. The window list is
/// read only to name another app that has it, or to tell which window has
/// it.
fn keys(expect: &Expect, look: &mut impl Look) -> Result<Option<&'static str>> {
    let focused = look.focused().map_err(|_| unreadable())?;
    if focused.is_none() && look.app_focused(expect.pid()).map_err(unread_by)? {
        return inside(expect, look);
    }
    let Some(pid) = focused else {
        return Err(refused(
            Reason::App,
            format!("No app has keyboard focus, {NOT_SENT}"),
            None,
        ));
    };
    if pid == expect.pid() {
        return inside(expect, look);
    }

    let found = Found::named(pid, look.windows().unwrap_or_default());
    let message = match &found.app {
        Some(name) => format!("{name} has keyboard focus, not the app this was for, {NOT_SENT}"),
        None => format!("Another app has keyboard focus, {NOT_SENT}"),
    };
    Err(refused(Reason::App, message, Some(found)))
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
            let listed = look.windows().map_err(|_| unreadable())?;
            in_window(wanted, expect.pid(), &keyboard, listed)
        }
        None => Verdict::Pass,
    }
    .or_else(|| match expect.field() {
        Some(name) => in_field(name, expect.pid(), &keyboard),
        None => Verdict::Pass,
    });

    let (reason, message) = match verdict {
        Verdict::Pass if expect.field().is_some() => return Ok(field_role(&keyboard)),
        Verdict::Pass => return Ok(None),
        Verdict::Unknown(why) => return Err(unread_by(why.error())),
        Verdict::Refused(reason, message) => (reason, message),
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
    Err(refused(
        reason,
        format!("{message}, {NOT_SENT}"),
        Some(found),
    ))
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
    /// It is not, for this reason, said in these words.
    Refused(Reason, String),
    /// Whether it is could not be told, for this reason.
    Unknown(Unread),
}

impl Verdict {
    /// This verdict, or once it passes, the next part's.
    fn or_else(self, next: impl FnOnce() -> Self) -> Self {
        match self {
            Self::Pass => next(),
            other => other,
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
        return Verdict::Refused(
            Reason::Window,
            "No window of the app has keyboard focus".to_owned(),
        );
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
                return over(wanted);
            }
            if covered(listed, pid, number) {
                return Verdict::Refused(
                    Reason::Window,
                    format!("Another window of the app is in front of window {wanted}"),
                );
            }
            if keyboard.within == Within::Elsewhere {
                return Verdict::Unknown(Unread::Failed);
            }
            Verdict::Pass
        }
        _ if sheet => over(wanted),
        Listed::One(number) => Verdict::Refused(
            Reason::Window,
            format!("Window {number} has keyboard focus, not window {wanted}"),
        ),
        Listed::Missing => Verdict::Unknown(Unread::Unlisted),
        Listed::Several => Verdict::Unknown(Unread::Alike),
    }
}

/// The refusal for a sheet over window `wanted` that has keyboard focus.
fn over(wanted: i64) -> Verdict {
    Verdict::Refused(
        Reason::Sheet,
        format!("A sheet is open over window {wanted} and has keyboard focus"),
    )
}

/// Whether the field named `name` has keyboard focus in app `pid`: the
/// focused element is the app's own, is a control that takes text itself
/// (see [`FIELDS`]), says it is focused, matches the name as `atlas
/// elements --name` would, and is enabled.
fn in_field(name: &str, pid: i64, keyboard: &Keyboard) -> Verdict {
    let no_field = || {
        Verdict::Refused(
            Reason::Field,
            "No field of the app has keyboard focus".to_owned(),
        )
    };
    let Some(Focused {
        element,
        pid: owner,
    }) = &keyboard.focused
    else {
        return no_field();
    };
    if *owner != pid {
        return Verdict::Refused(
            Reason::Field,
            "Keyboard focus is in a panel another process shows".to_owned(),
        );
    }

    let field = element
        .role
        .as_deref()
        .is_some_and(|role| FIELDS.contains(&role));
    if !field || element.focused == Some(false) {
        return no_field();
    }

    let filter = ElementFilter {
        role: None,
        name: Some(name.to_owned()),
    };
    if filter.match_of(element).is_none() {
        return Verdict::Refused(
            Reason::Field,
            format!("The field with keyboard focus is not the one named {name}"),
        );
    }
    if element.enabled == Some(false) {
        return Verdict::Refused(Reason::Disabled, "That field is disabled".to_owned());
    }

    Verdict::Pass
}

/// Which listed window app `pid`'s focused `window` is: the one its frame,
/// title and kind (by its subrole) leave, of every window of the app.
fn number(listed: &[Window], pid: i64, window: &Element) -> Listed {
    let Some(frame) = &window.frame else {
        return Listed::Missing;
    };
    windows::described(
        listed,
        pid,
        frame,
        window.title.as_deref(),
        Kind::of(window.subrole.as_deref()),
    )
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
/// where it takes clicks; on the menu bar, only while the app has keyboard
/// focus, since the menu bar shows that app's titles. With a window named,
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
    let listed = look.windows().map_err(|_| unreadable())?;
    if let (Spot::At, Some(wanted)) = (spot, expect.window()) {
        return at_window(expect.pid(), wanted, (x, y), listed, &screen);
    }
    let top = windows::top_at(listed, x, y, &screen);
    if top.is_some_and(Window::is_menu_bar) {
        return menu_bar(expect, look);
    }
    if windows::reaches(listed, expect.pid(), x, y, &screen) {
        return Ok(());
    }

    let at = spot.words();
    let Some(top) = top else {
        return Err(refused(
            Reason::Point,
            format!("No window of the app is at {at}, {NOT_SENT}"),
            None,
        ));
    };
    if top.is_dock() && *screen.dock() == Dock::Unread {
        return Err(refused(
            Reason::Unreadable,
            format!("Atlas could not read where the Dock takes clicks, {NOT_SENT}"),
            None,
        ));
    }

    let found = Found {
        app: top.owner.clone(),
        pid: top.pid,
        window: None,
        field: None,
    };
    let message = match &found.app {
        Some(name) => format!("{name}'s window is at {at}, not the app's, {NOT_SENT}"),
        None => format!("Another app's window is at {at}, {NOT_SENT}"),
    };
    Err(refused(Reason::Point, message, Some(found)))
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
        return Err(refused(
            Reason::Window,
            format!("Window {wanted} of the app is not on screen, {NOT_SENT}"),
            None,
        ));
    }
    if windows::unobstructed(listed, wanted, x, y, screen) {
        return Ok(());
    }

    let Some(top) = windows::top_at(listed, x, y, screen) else {
        return Err(refused(
            Reason::Window,
            format!("Window {wanted} is not at that point, {NOT_SENT}"),
            None,
        ));
    };
    if top.is_dock() && *screen.dock() == Dock::Unread {
        return Err(refused(
            Reason::Unreadable,
            format!("Atlas could not read where the Dock takes clicks, {NOT_SENT}"),
            None,
        ));
    }

    let (reason, whose) = match &top.owner {
        _ if top.pid == pid => (Reason::Window, "The app's".to_owned()),
        Some(owner) => (Reason::Point, format!("{owner}'s")),
        None => (Reason::Point, "Another app's".to_owned()),
    };
    Err(refused(
        reason,
        format!(
            "{whose} window {} is at that point, not window {wanted}, {NOT_SENT}",
            top.number
        ),
        Some(Found::at(top)),
    ))
}

/// The menu bar counts as the app's only while the app has keyboard focus.
fn menu_bar(expect: &Expect, look: &mut impl Look) -> Result<()> {
    let focused = look.focused().map_err(|_| unreadable())?;
    if focused == Some(expect.pid())
        || (focused.is_none() && look.app_focused(expect.pid()).map_err(unread_by)?)
    {
        return Ok(());
    }

    let found = focused.map(|pid| Found::named(pid, look.windows().unwrap_or_default()));
    let message = match found.as_ref().and_then(|found| found.app.as_deref()) {
        Some(name) => format!("The menu bar shows {name}, not the app this was for, {NOT_SENT}"),
        None => format!("The menu bar does not show the app, {NOT_SENT}"),
    };
    Err(refused(Reason::Point, message, found))
}

/// The refusal when what is in front could not be read in time.
pub(super) fn unreadable() -> Error {
    refused(
        Reason::Unreadable,
        format!("Atlas could not tell which app is in front, {NOT_SENT}"),
        None,
    )
}

/// The refusal for an app in front whose window or field could not be
/// told, in the words of the read's `error` and naming why when it says.
fn unread_by(error: Error) -> Error {
    let why = error
        .details
        .get("unread")
        .cloned()
        .unwrap_or_else(|| json!(Unread::Failed));
    refused(
        Reason::Unreadable,
        format!("{}, {NOT_SENT}", error.error),
        None,
    )
    .with("unread", why)
}

/// A `focus-changed` refusal for `reason`, naming what was `found`.
fn refused(reason: Reason, message: String, found: Option<Found>) -> Error {
    let error = Error::new(Code::FocusChanged, message).with("reason", reason);
    match found {
        Some(found) => error.with("found", found),
        None => error,
    }
}

#[cfg(test)]
pub(super) mod tests;
