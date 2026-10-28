//! Whether an agent's action would land in the app, window and field it
//! names, judged inside the helper just before the action is sent: text
//! and keys need the app, and the window and field when named, to have
//! keyboard focus; pointer actions need the app's own window, or the named
//! one, to be the top one at the point. Nothing here activates, raises or
//! focuses anything: what is not in place is refused, never moved.
use super::{
    ax::{self, Element, Frame, Keyboard, Within},
    windows::{self, Bounds, Dock, Screen, Window},
};
use crate::{
    actions::{Action, Expect},
    error::{Error, Result},
    select::{ElementFilter, shown_name},
};
use serde::Serialize;
use std::time::{Duration, Instant};

/// The most time one request spends checking; past it, the rest is refused
/// rather than risk the controller's five-second deadline.
pub const CHECK_BUDGET: Duration = Duration::from_millis(500);

/// The longest one Accessibility question may take.
const FOCUS_TIMEOUT: Duration = Duration::from_millis(250);

/// The longest the reads of where an app's keyboard focus is may take in
/// all: its focused window, its focused element and the way between them.
const KEYBOARD_TIMEOUT: Duration = Duration::from_millis(300);

/// Layers from the Dock's up never hold a window an app has keyboard focus
/// in.
const DOCK_LAYER: i64 = 20;

/// Roles that hold fields rather than take text themselves. An app that
/// reports one of these as focused, its window or a web page, says no field
/// has keyboard focus: focus is on its way between fields, or in a custom
/// view without accessibility. Its name proves nothing about where keys
/// land.
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
pub enum Need {
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
    pub fn of(action: &Action) -> Self {
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
    /// What is in front could not be read in time.
    Unreadable,
}

/// The focused element found where the expected field should have been:
/// its role and the name `atlas elements` shows for it. Never its value,
/// which may be what someone typed.
#[derive(Debug, Serialize)]
struct Field {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

impl Field {
    fn of(element: &Element) -> Self {
        Self {
            role: element.role.clone(),
            name: shown_name(element).map(str::to_owned),
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
    /// Where keyboard focus is inside process `pid`.
    fn keyboard(&mut self, pid: i64) -> Result<Keyboard>;
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
            return Err(Error::new(
                "native",
                "The time to check which app is in front ran out",
            ));
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

    /// Read from the app element of the process judged to have focus, so
    /// the app, window and field checks look at the same app.
    fn keyboard(&mut self, pid: i64) -> Result<Keyboard> {
        let left = self.left()?;
        ax::keyboard(pid, left.min(KEYBOARD_TIMEOUT))
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
pub fn check(expect: &Expect, need: Need, display: &Bounds, deadline: Instant) -> Result<()> {
    require(expect, need, display, &mut Live::until(deadline), deadline)
}

/// Refuse with `focus-changed` unless `expect` holds for `need` as `look`
/// sees it now. A look that ends after `deadline` proves nothing: a read
/// that stalled may describe a helper that has moved on since.
fn require(
    expect: &Expect,
    need: Need,
    display: &Bounds,
    look: &mut impl Look,
    deadline: Instant,
) -> Result<()> {
    let outcome = match need {
        Need::Nothing => return Ok(()),
        Need::Keys => keys(expect, look),
        Need::Point(x, y) => point(expect, (x, y), Spot::At, display, look),
        Need::Drag { from, to } => point(expect, from, Spot::At, display, look)
            .and_then(|()| point(expect, to, Spot::Drop, display, look)),
    };
    if outcome.is_ok() && Instant::now() >= deadline {
        return Err(unreadable());
    }

    outcome
}

/// Keys go ahead only while the app has keyboard focus and, when named,
/// the window and the field. The window list is read only to name another
/// app that has it, or to tell which window has it.
fn keys(expect: &Expect, look: &mut impl Look) -> Result<()> {
    let focused = look.focused().map_err(|_| unreadable())?;
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
/// one read of where the app's focus is.
fn inside(expect: &Expect, look: &mut impl Look) -> Result<()> {
    if expect.window().is_none() && expect.field().is_none() {
        return Ok(());
    }

    let keyboard = look
        .keyboard(expect.pid())
        .map_err(|_| unreadable_inside())?;
    let verdict = match expect.window() {
        Some(wanted) => {
            let listed = look.windows().map_err(|_| unreadable_inside())?;
            in_window(wanted, expect.pid(), &keyboard, listed)
        }
        None => Verdict::Pass,
    }
    .or_else(|| match expect.field() {
        Some(name) => in_field(name, expect.pid(), &keyboard),
        None => Verdict::Pass,
    });

    let (reason, message) = match verdict {
        Verdict::Pass => return Ok(()),
        Verdict::Unknown => return Err(unreadable_inside()),
        Verdict::Refused(reason, message) => (reason, message),
    };
    let listed = look.windows().unwrap_or_default();
    let mut found = Found::named(expect.pid(), listed);
    found.window = keyboard
        .window
        .as_ref()
        .and_then(|window| number(listed, expect.pid(), window));
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

/// What one part of a check found.
#[derive(Debug, PartialEq)]
enum Verdict {
    /// What it checks is in place.
    Pass,
    /// It is not, for this reason, said in these words.
    Refused(Reason, String),
    /// Whether it is could not be told.
    Unknown,
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
/// over it: the app's focused window is that window by its number, holds
/// the focused element with no sheet between them, and no window of the
/// app's is in front of it at its layer, where a sheet shows. A sheet
/// named by its own number is a window like any other.
fn in_window(wanted: i64, pid: i64, keyboard: &Keyboard, listed: &[Window]) -> Verdict {
    let Some(window) = &keyboard.window else {
        return Verdict::Refused(
            Reason::Window,
            "No window of the app has keyboard focus".to_owned(),
        );
    };
    let sheet = Verdict::Refused(
        Reason::Sheet,
        format!("A sheet is open over window {wanted} and has keyboard focus"),
    );

    match number(listed, pid, window) {
        Some(number) if number == wanted => {
            if keyboard.within == Within::Sheet || covered(listed, pid, number) {
                return sheet;
            }
            if keyboard.within == Within::Elsewhere {
                return Verdict::Unknown;
            }
            Verdict::Pass
        }
        _ if window.role.as_deref() == Some("AXSheet") || keyboard.within == Within::Sheet => sheet,
        Some(number) => Verdict::Refused(
            Reason::Window,
            format!("Window {number} has keyboard focus, not window {wanted}"),
        ),
        None => Verdict::Unknown,
    }
}

/// Whether the field named `name` has keyboard focus in app `pid`: the
/// focused element is the app's own, is no container (see
/// [`CONTAINERS`]), matches the name as `atlas elements --name` would,
/// every name of it was read whole, and it is enabled.
fn in_field(name: &str, pid: i64, keyboard: &Keyboard) -> Verdict {
    let Some(focused) = &keyboard.focused else {
        return Verdict::Refused(
            Reason::Field,
            "Nothing in the app has keyboard focus".to_owned(),
        );
    };
    if focused.pid != pid {
        return Verdict::Refused(
            Reason::Field,
            "Keyboard focus is in a panel another process shows".to_owned(),
        );
    }

    let element = &focused.element;
    let Some(role) = element.role.as_deref() else {
        return Verdict::Unknown;
    };
    if CONTAINERS.contains(&role) {
        return Verdict::Refused(
            Reason::Field,
            "No field of the app has keyboard focus".to_owned(),
        );
    }
    if !element.unreadable.is_empty() || !element.truncated.is_empty() {
        return Verdict::Unknown;
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
        return Verdict::Refused(
            Reason::Disabled,
            format!("The field named {name} has keyboard focus but is disabled"),
        );
    }

    Verdict::Pass
}

/// The number of the listed window that is app `pid`'s focused `window`:
/// the one window of the app below the Dock's layer whose bounds are the
/// window's frame within a point and whose title, when both are known,
/// holds the window's or is held in it (Chrome adds its own name to what
/// Accessibility reports). An ordinary window wins over a same-sized
/// panel. None when no window, or more than one, is it.
fn number(listed: &[Window], pid: i64, window: &Element) -> Option<i64> {
    let frame = window.frame.as_ref()?;
    let matching: Vec<&Window> = listed
        .iter()
        .filter(|listed| {
            listed.pid == pid
                && (0..DOCK_LAYER).contains(&listed.layer)
                && listed.alpha > 0.0
                && frame.matches(&frame_of(&listed.bounds))
                && match (&listed.title, &window.title) {
                    (Some(listed), Some(title)) => title.contains(listed) || listed.contains(title),
                    _ => true,
                }
        })
        .collect();
    let unique = |windows: Vec<&Window>| match windows.as_slice() {
        [window] => Some(window.number),
        _ => None,
    };

    unique(
        matching
            .iter()
            .copied()
            .filter(|window| window.layer == 0)
            .collect(),
    )
    .or_else(|| unique(matching))
}

/// Bounds as a frame, to compare with what Accessibility reports.
fn frame_of(bounds: &Bounds) -> Frame {
    Frame {
        x: bounds.x,
        y: bounds.y,
        width: bounds.width,
        height: bounds.height,
    }
}

/// Whether a shown window of app `pid` at window `number`'s layer is in
/// front of it and overlaps it, as a sheet does.
fn covered(listed: &[Window], pid: i64, number: i64) -> bool {
    let Some(index) = listed.iter().position(|window| window.number == number) else {
        return false;
    };
    let window = &listed[index];
    listed[..index].iter().any(|other| {
        other.pid == pid
            && other.layer == window.layer
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
    if focused == Some(expect.pid()) {
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
pub fn unreadable() -> Error {
    refused(
        Reason::Unreadable,
        format!("Atlas could not tell which app is in front, {NOT_SENT}"),
        None,
    )
}

/// The refusal when the app is in front but where its focus is inside it
/// could not be read whole in time.
fn unreadable_inside() -> Error {
    refused(
        Reason::Unreadable,
        format!(
            "Atlas could not tell which window or field of the app has keyboard focus, {NOT_SENT}"
        ),
        None,
    )
}

/// A `focus-changed` refusal for `reason`, naming what was `found`.
fn refused(reason: Reason, message: String, found: Option<Found>) -> Error {
    let error = Error::new("focus-changed", message).with("reason", reason);
    match found {
        Some(found) => error.with("found", found),
        None => error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const APP: i64 = 812;
    const OTHER: i64 = 900;

    fn display() -> Bounds {
        Bounds {
            x: 0.0,
            y: 0.0,
            width: 1000.0,
            height: 800.0,
        }
    }

    fn window(pid: i64, owner: &str, layer: i64, bounds: (f64, f64, f64, f64)) -> Window {
        // Each window its own number, as the window list gives them.
        static NUMBER: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(1);
        Window {
            number: NUMBER.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            pid,
            owner: Some(owner.to_owned()),
            title: Some("Secret plans.txt".to_owned()),
            layer,
            alpha: 1.0,
            bounds: Bounds {
                x: bounds.0,
                y: bounds.1,
                width: bounds.2,
                height: bounds.3,
            },
        }
    }

    fn menu_bar() -> Window {
        window(
            1,
            "Window Server",
            windows::MENU_BAR_LAYER,
            (0.0, 0.0, 1000.0, 24.0),
        )
    }

    /// A helper as a test sets it, counting each read.
    #[derive(Default)]
    struct Seen {
        focused: Option<Result<Option<i64>>>,
        windows: Option<Vec<Window>>,
        /// Where the Dock takes clicks; no icons unless a test sets them.
        dock: Option<Dock>,
        /// Where focus is inside the app; unreadable unless a test sets it.
        keyboard: Option<Keyboard>,
        focus_reads: usize,
        window_reads: usize,
        keyboard_reads: usize,
    }

    impl Seen {
        fn focused(pid: Option<i64>) -> Self {
            Self {
                focused: Some(Ok(pid)),
                ..Self::default()
            }
        }

        fn showing(mut self, windows: Vec<Window>) -> Self {
            self.windows = Some(windows);
            self
        }

        fn docked(mut self, dock: Dock) -> Self {
            self.dock = Some(dock);
            self
        }

        fn typing_in(mut self, keyboard: Keyboard) -> Self {
            self.keyboard = Some(keyboard);
            self
        }
    }

    impl Look for Seen {
        fn focused(&mut self) -> Result<Option<i64>> {
            self.focus_reads += 1;
            match &self.focused {
                Some(Ok(pid)) => Ok(*pid),
                _ => Err(Error::new("native", "unreadable")),
            }
        }

        fn keyboard(&mut self, _: i64) -> Result<Keyboard> {
            self.keyboard_reads += 1;
            self.keyboard
                .clone()
                .ok_or_else(|| Error::new("native", "unreadable"))
        }

        fn windows(&mut self) -> Result<&[Window]> {
            self.window_reads += 1;
            self.windows
                .as_deref()
                .ok_or_else(|| Error::new("windows", "unreadable"))
        }

        fn dock(&mut self) -> Dock {
            self.dock.clone().unwrap_or(Dock::Icons(Vec::new()))
        }
    }

    fn expect() -> Expect {
        Expect::new(APP).unwrap()
    }

    /// The refusal's code, reason and what it found, as an agent reads them.
    fn refusal(outcome: Result<()>) -> serde_json::Value {
        let error = outcome.unwrap_err();
        assert_eq!(error.code, "focus-changed");
        assert!(!error.uncertain);
        assert!(error.error.ends_with(NOT_SENT), "{}", error.error);
        json!({"reason": error.details["reason"], "found": error.details.get("found")})
    }

    fn keys_with(look: &mut Seen) -> Result<()> {
        require(&expect(), Need::Keys, &display(), look, later())
    }

    fn point_with(look: &mut Seen, x: f64, y: f64) -> Result<()> {
        require(&expect(), Need::Point(x, y), &display(), look, later())
    }

    #[test]
    fn keys_go_ahead_only_while_the_app_has_keyboard_focus() {
        assert!(keys_with(&mut Seen::focused(Some(APP))).is_ok());

        let calculator = window(OTHER, "Calculator", 0, (0.0, 0.0, 200.0, 300.0));
        let mut other = Seen::focused(Some(OTHER)).showing(vec![calculator]);
        assert_eq!(
            refusal(keys_with(&mut other)),
            json!({"reason": "app", "found": {"app": "Calculator", "pid": OTHER}})
        );

        let mut unnamed = Seen::focused(Some(OTHER)).showing(Vec::new());
        assert_eq!(
            refusal(keys_with(&mut unnamed)),
            json!({"reason": "app", "found": {"pid": OTHER}})
        );

        let mut nobody = Seen::focused(None);
        assert_eq!(
            refusal(keys_with(&mut nobody)),
            json!({"reason": "app", "found": null})
        );
    }

    #[test]
    fn keys_ask_for_no_window_list_unless_refused() {
        let mut look = Seen::focused(Some(APP)).showing(Vec::new());
        keys_with(&mut look).unwrap();
        assert_eq!((look.focus_reads, look.window_reads), (1, 0));
    }

    #[test]
    fn a_point_goes_ahead_only_where_the_apps_own_window_is_on_top() {
        let document = window(APP, "TextEdit", 0, (100.0, 100.0, 400.0, 300.0));
        let sheet = window(APP, "TextEdit", 0, (150.0, 100.0, 300.0, 150.0));
        let menu = window(
            APP,
            "TextEdit",
            windows::MENU_LAYER,
            (120.0, 24.0, 150.0, 200.0),
        );
        let panel = window(APP, "TextEdit", 3, (600.0, 100.0, 200.0, 200.0));
        let calculator = window(OTHER, "Calculator", 0, (400.0, 300.0, 200.0, 300.0));
        let dock = window(2, "Dock", 20, (0.0, 760.0, 1000.0, 40.0));
        let status = window(3, "Control Center", 25, (900.0, 0.0, 30.0, 24.0));
        let listed = vec![
            menu,
            status,
            dock,
            panel,
            sheet,
            calculator,
            document,
            menu_bar(),
        ];
        let look = || Seen::focused(Some(APP)).showing(listed.clone());

        for (x, y, what) in [
            (110.0, 350.0, "its document"),
            (200.0, 150.0, "its sheet"),
            (150.0, 100.0, "its open menu"),
            (650.0, 150.0, "its floating panel"),
        ] {
            assert!(point_with(&mut look(), x, y).is_ok(), "{what}");
        }

        assert_eq!(
            refusal(point_with(&mut look(), 450.0, 350.0)),
            json!({"reason": "point", "found": {"app": "Calculator", "pid": OTHER}}),
            "another app's window in front"
        );
        assert_eq!(
            refusal(point_with(&mut look(), 300.0, 780.0))["found"]["app"],
            "Dock"
        );
        assert_eq!(
            refusal(point_with(&mut look(), 910.0, 10.0))["found"]["app"],
            "Control Center"
        );
        assert_eq!(
            refusal(point_with(&mut look(), 50.0, 600.0)),
            json!({"reason": "point", "found": null}),
            "nothing there"
        );
    }

    #[test]
    fn the_menu_bar_counts_as_the_apps_only_while_it_has_keyboard_focus() {
        let document = window(APP, "TextEdit", 0, (100.0, 100.0, 400.0, 300.0));
        let calculator = window(OTHER, "Calculator", 0, (400.0, 300.0, 200.0, 300.0));
        let listed = vec![menu_bar(), calculator, document];

        let mut front = Seen::focused(Some(APP)).showing(listed.clone());
        assert!(point_with(&mut front, 200.0, 10.0).is_ok());

        let mut behind = Seen::focused(Some(OTHER)).showing(listed.clone());
        assert_eq!(
            refusal(point_with(&mut behind, 200.0, 10.0)),
            json!({"reason": "point", "found": {"app": "Calculator", "pid": OTHER}})
        );

        let mut nobody = Seen::focused(None).showing(listed);
        assert_eq!(
            refusal(point_with(&mut nobody, 200.0, 10.0)),
            json!({"reason": "point", "found": null})
        );
    }

    #[test]
    fn click_through_overlays_do_not_hide_the_window_under_them() {
        let document = window(APP, "TextEdit", 0, (100.0, 100.0, 400.0, 300.0));
        for owner in ["Dock", "Notification Center"] {
            let overlay = window(2, owner, 20, (0.0, 0.0, 1000.0, 800.0));
            let mut look = Seen::focused(Some(APP)).showing(vec![overlay, document.clone()]);
            assert!(point_with(&mut look, 200.0, 200.0).is_ok(), "{owner}");
        }

        let other = window(2, "Shade", 20, (0.0, 0.0, 1000.0, 800.0));
        let mut look = Seen::focused(Some(APP)).showing(vec![other, document]);
        assert_eq!(
            refusal(point_with(&mut look, 200.0, 200.0))["found"]["app"],
            "Shade"
        );
    }

    /// The test display's Dock, its icons along the bottom edge.
    fn icons() -> Dock {
        Dock::Icons(vec![Bounds {
            x: 200.0,
            y: 740.0,
            width: 600.0,
            height: 60.0,
        }])
    }

    /// The Dock's window, an app's window partly under its icons, and
    /// another app's window behind both.
    fn under_the_dock() -> Vec<Window> {
        vec![
            window(2, "Dock", 20, (0.0, 0.0, 1000.0, 800.0)),
            window(APP, "Calculator", 0, (150.0, 600.0, 300.0, 200.0)),
            window(OTHER, "Finder", 0, (0.0, 0.0, 1000.0, 800.0)),
        ]
    }

    /// A check of `need` for `pid` with the Dock as `dock`.
    fn docked_need(pid: i64, dock: &Dock, need: Need) -> Result<()> {
        let mut look = Seen::focused(Some(pid))
            .showing(under_the_dock())
            .docked(dock.clone());
        require(
            &Expect::new(pid).unwrap(),
            need,
            &display(),
            &mut look,
            later(),
        )
    }

    /// The Dock's full-display window lets clicks through except on its
    /// icons and the shelf around them: there it is on top for every app,
    /// even over an app's own window under it.
    #[test]
    fn the_docks_icons_refuse_every_app_and_the_points_beside_them_pass() {
        let at = |pid, x, y| docked_need(pid, &icons(), Need::Point(x, y));
        let dock = json!({"reason": "point", "found": {"app": "Dock", "pid": 2}});
        for (pid, x, y, what) in [
            (APP, 300.0, 770.0, "an icon"),
            (OTHER, 300.0, 770.0, "an icon"),
            (OTHER, 790.0, 799.0, "an icon at the edge"),
            (APP, 195.0, 770.0, "the shelf beside the icons"),
            (APP, 300.0, 732.0, "the shelf above the icons"),
        ] {
            assert_eq!(refusal(at(pid, x, y)), dock, "{pid}: {what}");
        }

        assert!(at(APP, 170.0, 770.0).is_ok(), "beside the shelf");
        assert!(at(APP, 300.0, 700.0).is_ok(), "above the shelf");
        assert!(at(OTHER, 900.0, 770.0).is_ok(), "beside the shelf");
    }

    /// A Dock that shows more than its icons, or could not be read, covers
    /// all of its window: what else it shows takes clicks too.
    #[test]
    fn a_dock_that_is_not_only_its_icons_covers_the_whole_display() {
        for (x, y) in [(300.0, 700.0), (500.0, 300.0), (10.0, 10.0)] {
            assert_eq!(
                refusal(docked_need(OTHER, &Dock::Other, Need::Point(x, y))),
                json!({"reason": "point", "found": {"app": "Dock", "pid": 2}}),
                "{x},{y}"
            );
            assert_eq!(
                refusal(docked_need(OTHER, &Dock::Unread, Need::Point(x, y))),
                json!({"reason": "unreadable", "found": null}),
                "{x},{y}"
            );
        }
    }

    /// A drag needs the app's window on top where it drops as well as
    /// where it starts, so nothing is dropped on a Dock icon or the Trash.
    #[test]
    fn a_drag_is_refused_when_it_would_drop_on_the_dock() {
        let drag = |to| Need::Drag {
            from: (300.0, 650.0),
            to,
        };
        assert!(docked_need(APP, &icons(), drag((400.0, 700.0))).is_ok());

        let onto_icon = refusal(docked_need(APP, &icons(), drag((400.0, 770.0))));
        assert_eq!(
            onto_icon,
            json!({"reason": "point", "found": {"app": "Dock", "pid": 2}})
        );
        let error = docked_need(APP, &icons(), drag((400.0, 770.0))).unwrap_err();
        assert!(error.error.contains("the drop point"), "{}", error.error);

        assert_eq!(
            refusal(docked_need(APP, &icons(), drag((900.0, 300.0))))["found"]["app"],
            "Finder",
            "a drop on another app"
        );
        let from_icon = Need::Drag {
            from: (300.0, 770.0),
            to: (400.0, 700.0),
        };
        assert_eq!(
            refusal(docked_need(APP, &icons(), from_icon))["found"]["app"],
            "Dock"
        );
    }

    #[test]
    fn each_action_needs_what_it_touches_first() {
        let need = |action: serde_json::Value| {
            Need::of(&serde_json::from_value::<Action>(action).unwrap())
        };

        assert_eq!(need(json!({"type": "type", "text": "hi"})), Need::Keys);
        assert_eq!(need(json!({"type": "key", "key": "cmd+s"})), Need::Keys);
        assert_eq!(
            need(json!({"type": "click", "x": 1, "y": 2})),
            Need::Point(1.0, 2.0)
        );
        assert_eq!(
            need(json!({"type": "scroll", "x": 3, "y": 4, "dy": 1})),
            Need::Point(3.0, 4.0)
        );
        assert_eq!(
            need(json!({"type": "drag", "fromX": 5, "fromY": 6, "toX": 7, "toY": 8})),
            Need::Drag {
                from: (5.0, 6.0),
                to: (7.0, 8.0)
            },
            "a drag needs its start and its drop"
        );
        for phase in ["down", "move", "up"] {
            assert_eq!(
                need(json!({"type": "pointer", "phase": phase, "x": 9, "y": 10})),
                Need::Point(9.0, 10.0),
                "a pointer's {phase} may press, carry or drop"
            );
        }
        assert_eq!(
            need(json!({"type": "move", "x": 1, "y": 2})),
            Need::Point(1.0, 2.0),
            "a move may carry what a press holds"
        );
        assert_eq!(need(json!({"type": "wait", "ms": 10})), Need::Nothing);
    }

    #[test]
    fn nothing_needed_is_never_looked_for() {
        let mut look = Seen::default();
        require(
            &expect(),
            Need::Nothing,
            &display(),
            &mut look,
            Instant::now(),
        )
        .unwrap();
        assert_eq!((look.focus_reads, look.window_reads), (0, 0));
    }

    #[test]
    fn a_focus_that_cannot_be_read_refuses() {
        let unreadable = json!({"reason": "unreadable", "found": null});
        assert_eq!(refusal(keys_with(&mut Seen::default())), unreadable);

        let document = window(APP, "TextEdit", 0, (100.0, 100.0, 400.0, 300.0));
        let mut no_list = Seen::focused(Some(APP));
        assert_eq!(refusal(point_with(&mut no_list, 200.0, 200.0)), unreadable);

        let mut no_focus = Seen::default().showing(vec![menu_bar(), document]);
        assert_eq!(refusal(point_with(&mut no_focus, 200.0, 10.0)), unreadable);
    }

    /// A deadline well past any test's own reads.
    fn later() -> Instant {
        Instant::now() + Duration::from_secs(60)
    }

    #[test]
    fn a_look_that_ends_after_its_deadline_refuses() {
        let document = window(APP, "TextEdit", 0, (100.0, 100.0, 400.0, 300.0));
        for need in [Need::Keys, Need::Point(200.0, 200.0)] {
            // The look answers as if in time; the deadline has passed by
            // the time it ends.
            let mut late = Seen::focused(Some(APP)).showing(vec![document.clone()]);
            let deadline = Instant::now();
            assert!(require(&expect(), need, &display(), &mut late, later()).is_ok());
            assert_eq!(
                refusal(require(&expect(), need, &display(), &mut late, deadline)),
                json!({"reason": "unreadable", "found": null}),
                "{need:?}"
            );
        }
    }

    #[test]
    fn checks_stop_once_their_time_is_spent() {
        let now = Instant::now();
        let mut spent = Live::until(now);
        for need in [Need::Keys, Need::Point(10.0, 10.0)] {
            assert_eq!(
                refusal(require(&expect(), need, &display(), &mut spent, now)),
                json!({"reason": "unreadable", "found": null}),
                "{need:?}"
            );
        }
        assert!(spent.windows.is_none(), "nothing was read");
    }

    #[test]
    fn a_refusal_names_what_was_there_but_no_title() {
        let calculator = window(OTHER, "Calculator", 0, (0.0, 0.0, 1000.0, 800.0));
        for outcome in [
            keys_with(&mut Seen::focused(Some(OTHER)).showing(vec![calculator.clone()])),
            point_with(
                &mut Seen::focused(Some(APP)).showing(vec![calculator]),
                5.0,
                5.0,
            ),
        ] {
            let error = outcome.unwrap_err();
            let said = serde_json::to_string(&error).unwrap();
            assert!(!said.contains("Secret plans"), "{said}");
            assert!(error.error.starts_with("Calculator"), "{}", error.error);
        }
    }

    /// The document window the window and field tests type into.
    const DOCUMENT: i64 = 4120;

    /// A listed window with the number `number`.
    fn numbered(
        number: i64,
        pid: i64,
        title: Option<&str>,
        bounds: (f64, f64, f64, f64),
    ) -> Window {
        Window {
            number,
            pid,
            owner: Some(if pid == APP { "TextEdit" } else { "Calculator" }.to_owned()),
            title: title.map(str::to_owned),
            layer: 0,
            alpha: 1.0,
            bounds: Bounds {
                x: bounds.0,
                y: bounds.1,
                width: bounds.2,
                height: bounds.3,
            },
        }
    }

    /// The document as the window list shows it.
    fn document() -> Window {
        numbered(
            DOCUMENT,
            APP,
            Some("Letter.txt"),
            (100.0, 100.0, 400.0, 300.0),
        )
    }

    /// An element as a focus read reports it: a role, a frame and the
    /// names given.
    fn element(role: &str, frame: (f64, f64, f64, f64)) -> Element {
        Element {
            index: 0,
            depth: 0,
            window: None,
            role: Some(role.to_owned()),
            subrole: None,
            title: None,
            description: None,
            value: None,
            identifier: None,
            enabled: Some(true),
            focused: Some(true),
            selected: None,
            frame: Some(Frame {
                x: frame.0,
                y: frame.1,
                width: frame.2,
                height: frame.3,
            }),
            unreadable: Vec::new(),
            truncated: Vec::new(),
            menu: false,
            mark: None,
        }
    }

    /// The document as Accessibility reports it when it is the focused
    /// window.
    fn document_window() -> Element {
        Element {
            title: Some("Letter.txt".to_owned()),
            ..element("AXWindow", (100.0, 100.0, 400.0, 300.0))
        }
    }

    /// A text field named `name` by its description, as `elements` would
    /// list it.
    fn field(name: &str) -> Element {
        Element {
            description: Some(name.to_owned()),
            value: Some("what someone typed".to_owned()),
            ..element("AXTextField", (150.0, 150.0, 200.0, 24.0))
        }
    }

    /// Focus in `focused`, owned by the app, inside the document window.
    fn typing(focused: Element) -> Keyboard {
        Keyboard {
            window: Some(document_window()),
            focused: Some(ax::Focused {
                element: focused,
                pid: APP,
            }),
            within: Within::Window,
        }
    }

    /// The app in front, typing as `keyboard` says, with `listed` on screen.
    fn inside_with(keyboard: Keyboard, listed: Vec<Window>) -> Seen {
        Seen::focused(Some(APP)).showing(listed).typing_in(keyboard)
    }

    /// What `expect` says of keys sent with `look`.
    fn expecting(expect: &Expect, look: &mut Seen) -> Result<()> {
        require(expect, Need::Keys, &display(), look, later())
    }

    fn in_document() -> Expect {
        expect().in_window(DOCUMENT).unwrap()
    }

    fn in_field(name: &str) -> Expect {
        expect().in_field(name).unwrap()
    }

    #[test]
    fn keys_ask_for_nothing_inside_the_app_without_a_window_or_field() {
        let mut look = inside_with(typing(field("Email")), vec![document()]);
        keys_with(&mut look).unwrap();
        assert_eq!(look.keyboard_reads, 0);
    }

    #[test]
    fn a_focused_window_that_is_the_named_one_passes() {
        let mut look = inside_with(typing(field("Email")), vec![document()]);
        assert!(expecting(&in_document(), &mut look).is_ok());
        assert_eq!(look.keyboard_reads, 1, "one read for the window and field");
    }

    #[test]
    fn another_focused_window_of_the_app_is_refused_and_named() {
        let other = numbered(4121, APP, Some("Untitled"), (140.0, 140.0, 400.0, 300.0));
        let keyboard = Keyboard {
            window: Some(Element {
                title: Some("Untitled".to_owned()),
                ..element("AXWindow", (140.0, 140.0, 400.0, 300.0))
            }),
            ..typing(field("Email"))
        };
        let mut look = inside_with(keyboard, vec![other, document()]);
        let refused = refusal(expecting(&in_document(), &mut look));
        assert_eq!(refused["reason"], "window");
        assert_eq!(refused["found"]["window"], 4121);
        assert_eq!(refused["found"]["app"], "TextEdit");

        let mut none = inside_with(
            Keyboard {
                window: None,
                ..Keyboard::default()
            },
            vec![document()],
        );
        assert_eq!(
            refusal(expecting(&in_document(), &mut none))["reason"],
            "window"
        );
    }

    #[test]
    fn a_focused_window_of_another_app_is_refused_as_the_app() {
        let calculator = numbered(339, OTHER, Some("Calculator"), (0.0, 0.0, 200.0, 300.0));
        let mut look = Seen::focused(Some(OTHER))
            .showing(vec![calculator, document()])
            .typing_in(typing(field("Email")));
        for expect in [in_document(), in_field("Email")] {
            assert_eq!(
                refusal(expecting(&expect, &mut look)),
                json!({"reason": "app", "found": {"app": "Calculator", "pid": OTHER}})
            );
        }
        assert_eq!(look.keyboard_reads, 0, "the app is judged first");
    }

    #[test]
    fn a_sheet_over_the_window_is_refused_even_when_the_field_inside_matches() {
        let sheet = numbered(4122, APP, None, (150.0, 100.0, 300.0, 150.0));
        let named = in_document().in_field("Email").unwrap();

        // Focus in a field of a sheet the focused window holds.
        let mut held = inside_with(
            Keyboard {
                within: Within::Sheet,
                ..typing(field("Email"))
            },
            vec![sheet.clone(), document()],
        );
        let refused = refusal(expecting(&named, &mut held));
        assert_eq!(refused["reason"], "sheet");
        assert_eq!(refused["found"]["field"]["name"], "Email");

        // The sheet reported as the focused window itself.
        let mut focused = inside_with(
            Keyboard {
                window: Some(element("AXSheet", (150.0, 100.0, 300.0, 150.0))),
                ..typing(element("AXSheet", (150.0, 100.0, 300.0, 150.0)))
            },
            vec![sheet.clone(), document()],
        );
        let refused = refusal(expecting(&named, &mut focused));
        assert_eq!(refused["reason"], "sheet");
        assert_eq!(refused["found"]["window"], 4122);

        // A window of the app's in front of the document at its layer, as
        // a sheet's own window shows, even when the reads miss the sheet.
        let mut covered = inside_with(typing(field("Email")), vec![sheet, document()]);
        assert_eq!(
            refusal(expecting(&in_document(), &mut covered))["reason"],
            "sheet"
        );
    }

    #[test]
    fn a_sheet_named_by_its_own_number_is_the_window_with_focus() {
        let sheet = numbered(4122, APP, None, (150.0, 100.0, 300.0, 150.0));
        let name = Element {
            identifier: Some("saveAsNameTextField".to_owned()),
            ..element("AXTextField", (200.0, 120.0, 200.0, 24.0))
        };
        let mut look = inside_with(
            Keyboard {
                window: Some(element("AXSheet", (150.0, 100.0, 300.0, 150.0))),
                ..typing(name)
            },
            vec![sheet, document()],
        );
        let expect = expect()
            .in_window(4122)
            .unwrap()
            .in_field("saveAsNameTextField")
            .unwrap();
        assert!(expecting(&expect, &mut look).is_ok());
    }

    #[test]
    fn a_floating_panel_of_the_app_in_front_does_not_cover_the_window() {
        let panel = Window {
            layer: 3,
            ..numbered(4130, APP, Some("Fonts"), (150.0, 150.0, 200.0, 200.0))
        };
        let mut look = inside_with(typing(field("Email")), vec![panel, document()]);
        assert!(expecting(&in_document(), &mut look).is_ok());
    }

    #[test]
    fn a_window_is_known_only_by_a_unique_frame_match_in_the_same_process() {
        let twin_elsewhere = numbered(
            5000,
            OTHER,
            Some("Letter.txt"),
            (100.0, 100.0, 400.0, 300.0),
        );
        let offset = numbered(4121, APP, Some("Letter.txt"), (103.0, 100.0, 400.0, 300.0));
        let window = document_window();
        assert_eq!(
            number(&[twin_elsewhere, offset.clone(), document()], APP, &window),
            Some(DOCUMENT)
        );
        assert_eq!(number(&[offset], APP, &window), None, "off by three points");

        let renamed = numbered(
            DOCUMENT,
            APP,
            Some("Other.txt"),
            (100.0, 100.0, 400.0, 300.0),
        );
        assert_eq!(number(&[renamed], APP, &window), None, "titles differ");
        let browser = Element {
            title: Some("Letter.txt - Google Chrome".to_owned()),
            ..document_window()
        };
        assert_eq!(
            number(&[document()], APP, &browser),
            Some(DOCUMENT),
            "the app's name added to the title"
        );
        let numbered_twin = numbered(
            4121,
            APP,
            Some("Letter.txt 2"),
            (100.0, 100.0, 400.0, 300.0),
        );
        assert_eq!(
            number(&[numbered_twin, document()], APP, &window),
            None,
            "titles that both hold the window's leave it unproven"
        );
        let untitled = numbered(DOCUMENT, APP, None, (100.0, 100.0, 400.0, 300.0));
        assert_eq!(
            number(&[untitled], APP, &window),
            Some(DOCUMENT),
            "a title the list does not show proves nothing either way"
        );
    }

    #[test]
    fn two_windows_of_one_app_with_the_same_frame_and_title_leave_the_window_unproven() {
        let twin = numbered(4121, APP, Some("Letter.txt"), (100.0, 100.0, 400.0, 300.0));
        assert_eq!(
            number(&[twin.clone(), document()], APP, &document_window()),
            None
        );

        let mut look = inside_with(typing(field("Email")), vec![twin, document()]);
        assert_eq!(
            refusal(expecting(&in_document(), &mut look)),
            json!({"reason": "unreadable", "found": null})
        );
    }

    #[test]
    fn a_field_reached_from_outside_the_window_leaves_the_window_unproven() {
        let keyboard = Keyboard {
            within: Within::Elsewhere,
            ..typing(field("Email"))
        };
        let mut look = inside_with(keyboard, vec![document()]);
        assert_eq!(
            refusal(expecting(&in_document(), &mut look))["reason"],
            "unreadable"
        );
    }

    #[test]
    fn a_focused_field_that_elements_would_list_by_the_name_passes() {
        for (focused, name) in [
            (field("Email"), "email"),
            (
                Element {
                    identifier: Some("First Text View".to_owned()),
                    ..element("AXTextArea", (100.0, 132.0, 400.0, 268.0))
                },
                "First Text View",
            ),
            (
                Element {
                    title: Some("Search".to_owned()),
                    ..element("AXSearchField", (120.0, 110.0, 200.0, 22.0))
                },
                "Search",
            ),
        ] {
            let filter = ElementFilter {
                role: None,
                name: Some(name.to_owned()),
            };
            assert!(filter.matches(&focused), "elements lists it as {name}");
            let mut look = inside_with(typing(focused), vec![document()]);
            assert!(expecting(&in_field(name), &mut look).is_ok(), "{name}");
        }
    }

    #[test]
    fn a_field_matched_only_by_the_first_part_of_its_label_passes_as_elements_would() {
        let mut look = inside_with(typing(field("Email, required")), vec![document()]);
        assert!(expecting(&in_field("Email"), &mut look).is_ok());
    }

    #[test]
    fn a_field_is_never_named_by_what_it_holds() {
        let mut look = inside_with(typing(field("Email")), vec![document()]);
        assert_eq!(
            refusal(expecting(&in_field("what someone typed"), &mut look))["reason"],
            "field"
        );
    }

    #[test]
    fn a_focused_element_with_another_name_is_refused_as_another_field() {
        let mut look = inside_with(typing(field("Password")), vec![document()]);
        let refused = refusal(expecting(&in_field("Email"), &mut look));
        assert_eq!(
            refused,
            json!({"reason": "field", "found": {"app": "TextEdit", "pid": APP,
                "window": DOCUMENT, "field": {"role": "AXTextField", "name": "Password"}}})
        );

        let mut nothing = inside_with(
            Keyboard {
                focused: None,
                ..typing(field("Email"))
            },
            vec![document()],
        );
        assert_eq!(
            refusal(expecting(&in_field("Email"), &mut nothing))["reason"],
            "field"
        );
    }

    /// A custom view without accessibility leaves its window reported as
    /// focused, and so does an app while focus moves between fields; a
    /// browser may report its page. None is a field, whatever its title.
    #[test]
    fn a_window_or_page_reported_as_focused_is_never_the_field() {
        for role in ["AXWindow", "AXSheet", "AXWebArea", "AXGroup"] {
            let container = Element {
                title: Some("Email".to_owned()),
                ..element(role, (100.0, 100.0, 400.0, 300.0))
            };
            let mut look = inside_with(typing(container), vec![document()]);
            assert_eq!(
                refusal(expecting(&in_field("Email"), &mut look))["reason"],
                "field",
                "{role}"
            );
        }

        let mut unknown = inside_with(
            typing(Element {
                role: None,
                ..field("Email")
            }),
            vec![document()],
        );
        assert_eq!(
            refusal(expecting(&in_field("Email"), &mut unknown))["reason"],
            "unreadable",
            "a role that could not be read"
        );
    }

    #[test]
    fn a_disabled_focused_field_is_refused() {
        let disabled = Element {
            enabled: Some(false),
            ..field("Email")
        };
        let mut look = inside_with(typing(disabled), vec![document()]);
        assert_eq!(
            refusal(expecting(&in_field("Email"), &mut look))["reason"],
            "disabled"
        );
    }

    #[test]
    fn a_field_whose_name_could_not_be_read_whole_is_refused_not_passed() {
        for (unreadable, truncated) in [(vec!["AXDescription"], vec![]), (vec![], vec!["AXTitle"])]
        {
            let unread = Element {
                unreadable: unreadable.iter().map(|a| (*a).to_owned()).collect(),
                truncated: truncated.iter().map(|a| (*a).to_owned()).collect(),
                ..field("Email")
            };
            let mut look = inside_with(typing(unread), vec![document()]);
            assert_eq!(
                refusal(expecting(&in_field("Email"), &mut look))["reason"],
                "unreadable",
                "{unreadable:?} {truncated:?}"
            );
        }
    }

    #[test]
    fn a_focused_element_owned_by_another_process_is_refused() {
        let mut keyboard = typing(field("Email"));
        if let Some(focused) = keyboard.focused.as_mut() {
            focused.pid = OTHER;
        }
        let mut look = inside_with(keyboard, vec![document()]);
        assert_eq!(
            refusal(expecting(&in_field("Email"), &mut look))["reason"],
            "field"
        );
    }

    #[test]
    fn a_check_that_ran_out_of_time_is_refused_and_says_so() {
        let mut look = Seen::focused(Some(APP)).showing(vec![document()]);
        for expect in [in_document(), in_field("Email")] {
            let error = expecting(&expect, &mut look).unwrap_err();
            assert_eq!(error.details["reason"], "unreadable");
            assert!(error.error.contains("could not tell"), "{}", error.error);
            assert!(!error.uncertain);
        }
    }

    #[test]
    fn the_refusal_names_the_focused_field_without_its_value() {
        let secret = Element {
            value: Some("hunter2".to_owned()),
            ..field("Password")
        };
        let mut look = inside_with(typing(secret), vec![document()]);
        let error = expecting(&in_field("Email"), &mut look).unwrap_err();
        let said = serde_json::to_string(&error).unwrap();
        assert!(!said.contains("hunter2"), "{said}");
        assert!(!said.contains("Letter.txt"), "{said}");
        assert!(said.contains("Password"), "{said}");
    }

    #[test]
    fn a_point_outside_the_named_window_is_refused_before_a_click() {
        let other = numbered(4121, APP, Some("Untitled"), (450.0, 100.0, 300.0, 300.0));
        let calculator = numbered(339, OTHER, Some("Calculator"), (100.0, 500.0, 200.0, 200.0));
        let listed = vec![other, calculator, document()];
        let at = |x, y| {
            require(
                &in_document(),
                Need::Point(x, y),
                &display(),
                &mut Seen::focused(Some(APP)).showing(listed.clone()),
                later(),
            )
        };

        assert!(at(200.0, 200.0).is_ok(), "inside it");
        assert_eq!(
            refusal(at(470.0, 200.0)),
            json!({"reason": "window", "found": {"app": "TextEdit", "pid": APP, "window": 4121}}),
            "the app's other window on top there"
        );
        assert_eq!(
            refusal(at(150.0, 550.0))["reason"],
            "point",
            "another app's window there"
        );
        assert_eq!(
            refusal(at(900.0, 700.0))["reason"],
            "window",
            "nothing there"
        );

        let gone = expect().in_window(9999).unwrap();
        let mut look = Seen::focused(Some(APP)).showing(listed.clone());
        assert_eq!(
            refusal(require(
                &gone,
                Need::Point(200.0, 200.0),
                &display(),
                &mut look,
                later()
            ))["reason"],
            "window",
            "a window that is not on screen"
        );
        let elsewhere = Expect::new(OTHER).unwrap().in_window(DOCUMENT).unwrap();
        let mut look = Seen::focused(Some(APP)).showing(listed);
        assert_eq!(
            refusal(require(
                &elsewhere,
                Need::Point(200.0, 200.0),
                &display(),
                &mut look,
                later()
            ))["reason"],
            "window",
            "a window of another app"
        );
    }

    #[test]
    fn a_drag_is_checked_at_its_press_point_only() {
        let other = numbered(4121, APP, Some("Untitled"), (450.0, 100.0, 300.0, 300.0));
        let listed = vec![other, document()];
        let drag = |from, to| {
            require(
                &in_document(),
                Need::Drag { from, to },
                &display(),
                &mut Seen::focused(Some(APP)).showing(listed.clone()),
                later(),
            )
        };

        assert!(
            drag((200.0, 200.0), (600.0, 200.0)).is_ok(),
            "a drop on the app's other window"
        );
        assert_eq!(
            refusal(drag((600.0, 200.0), (200.0, 200.0)))["reason"],
            "window"
        );
        assert_eq!(
            refusal(drag((200.0, 200.0), (900.0, 700.0)))["reason"],
            "point",
            "a drop where no window of the app is"
        );
    }

    #[test]
    fn the_check_never_activates_raises_or_focuses_anything() {
        // What the check reads with: this module and the Accessibility
        // reads, which make copies and set only messaging timeouts.
        for source in [include_str!("focus.rs"), include_str!("ax.rs")] {
            let code = source.split("#[cfg(test)]").next().unwrap_or_default();
            for setter in [
                "set_attribute_value",
                "AXUIElementSetAttributeValue",
                "perform_action",
                "AXUIElementPerformAction",
                "activateWithOptions",
                "activate(",
                "AXRaise",
                "makeKeyAndOrderFront",
            ] {
                assert!(!code.contains(setter), "{setter}");
            }
        }
    }
}
