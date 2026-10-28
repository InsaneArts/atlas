//! Whether an agent's action would land in the app it names, judged inside
//! the helper just before the action is sent: text and keys need the app
//! to have keyboard focus, pointer actions need the app's own window to be
//! the top one at the point.
use super::windows::{self, Bounds, Dock, Screen, Window};
use crate::{
    actions::{Action, Expect, PointerPhase},
    error::{Error, Result},
};
use serde::Serialize;
use std::time::{Duration, Instant};

/// The most time one request spends checking; past it, the rest is refused
/// rather than risk the controller's five-second deadline.
pub const CHECK_BUDGET: Duration = Duration::from_millis(500);

/// The longest one Accessibility question may take.
const FOCUS_TIMEOUT: Duration = Duration::from_millis(250);

/// How every refusal ends: nothing of the refused action was posted.
const NOT_SENT: &str = "so this action and the rest were not sent";

/// What an action needs of its app before its first event.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Need {
    /// Keyboard focus.
    Keys,
    /// The app's own window on top at this point.
    Point(f64, f64),
    /// Nothing: the action sends nothing an app acts on, or continues a
    /// gesture already under way.
    Nothing,
}

impl Need {
    /// Type and Key need keys; Click, Scroll and a pointer press their
    /// point; Drag its start; Move, Wait and a pointer's move or release
    /// nothing.
    pub fn of(action: &Action) -> Self {
        match action {
            Action::Type { .. } | Action::Key { .. } => Self::Keys,
            Action::Click { x, y, .. }
            | Action::Scroll { x, y, .. }
            | Action::Pointer {
                phase: PointerPhase::Down,
                x,
                y,
                ..
            } => Self::Point(*x, *y),
            Action::Drag { from_x, from_y, .. } => Self::Point(*from_x, *from_y),
            Action::Move { .. }
            | Action::Wait { .. }
            | Action::Pointer {
                phase: PointerPhase::Move | PointerPhase::Up,
                ..
            } => Self::Nothing,
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
    /// What is in front could not be read in time.
    Unreadable,
}

/// The app found where the expected one should have been: its process and,
/// when a window of it is listed, its name. Never a window's title, which
/// may be a document's.
#[derive(Debug, Serialize)]
struct Found {
    #[serde(skip_serializing_if = "Option::is_none")]
    app: Option<String>,
    pid: i64,
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
        }
    }
}

/// What the helper shows, read only as far as a check needs it.
trait Look {
    /// The process with keyboard focus; None when no app has it.
    fn focused(&mut self) -> Result<Option<i64>>;
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

    fn windows(&mut self) -> Result<&[Window]> {
        if self.windows.is_none() {
            self.left()?;
            self.windows = Some(windows::windows()?);
        }

        Ok(self.windows.as_deref().unwrap_or_default())
    }

    /// The Dock's tiles as Accessibility reports them, from the Dock that
    /// owns a window in the list.
    fn dock(&mut self) -> Dock {
        if self.dock.is_none() {
            let dock = match self.left() {
                Ok(left) => match self.windows() {
                    Ok(listed) => super::ax::dock(listed, left.min(super::ax::DOCK_TIMEOUT)),
                    Err(_) => Dock::Unread,
                },
                Err(_) => Dock::Unread,
            };
            self.dock = Some(dock);
        }

        self.dock.clone().unwrap_or_default()
    }
}

/// [`require`] against the live helper, taking until `deadline` at most.
pub fn check(expect: Expect, need: Need, display: &Bounds, deadline: Instant) -> Result<()> {
    require(expect, need, display, &mut Live::until(deadline), deadline)
}

/// Refuse with `focus-changed` unless `expect` holds for `need` as `look`
/// sees it now. A look that ends after `deadline` proves nothing: a read
/// that stalled may describe a helper that has moved on since.
fn require(
    expect: Expect,
    need: Need,
    display: &Bounds,
    look: &mut impl Look,
    deadline: Instant,
) -> Result<()> {
    let outcome = match need {
        Need::Nothing => return Ok(()),
        Need::Keys => keys(expect, look),
        Need::Point(x, y) => point(expect, x, y, display, look),
    };
    if outcome.is_ok() && Instant::now() >= deadline {
        return Err(unreadable());
    }

    outcome
}

/// Keys go ahead only while the app has keyboard focus. The window list is
/// read only to name another app that has it.
fn keys(expect: Expect, look: &mut impl Look) -> Result<()> {
    let focused = look.focused().map_err(|_| unreadable())?;
    let Some(pid) = focused else {
        return Err(refused(
            Reason::App,
            format!("No app has keyboard focus, {NOT_SENT}"),
            None,
        ));
    };
    if pid == expect.pid() {
        return Ok(());
    }

    let found = Found::named(pid, look.windows().unwrap_or_default());
    let message = match &found.app {
        Some(name) => format!("{name} has keyboard focus, not the app this was for, {NOT_SENT}"),
        None => format!("Another app has keyboard focus, {NOT_SENT}"),
    };
    Err(refused(Reason::App, message, Some(found)))
}

/// A pointer action goes ahead only where the app's own window, at any
/// layer, is uncovered at the point, the Dock's tiles covering what is
/// under them; on the menu bar, only while the app has keyboard focus,
/// since the menu bar shows that app's titles.
fn point(expect: Expect, x: f64, y: f64, display: &Bounds, look: &mut impl Look) -> Result<()> {
    let screen = Screen::new(display.clone(), look.dock());
    let listed = look.windows().map_err(|_| unreadable())?;
    let top = windows::top_at(listed, x, y, &screen);
    if top.is_some_and(Window::is_menu_bar) {
        return menu_bar(expect, look);
    }
    if windows::reaches(listed, expect.pid(), x, y, &screen) {
        return Ok(());
    }

    let Some(top) = top else {
        return Err(refused(
            Reason::Point,
            format!("No window of the app is at that point, {NOT_SENT}"),
            None,
        ));
    };
    let found = Found {
        app: top.owner.clone(),
        pid: top.pid,
    };
    let message = match &found.app {
        Some(name) => format!("{name}'s window is at that point, not the app's, {NOT_SENT}"),
        None => format!("Another app's window is at that point, {NOT_SENT}"),
    };
    Err(refused(Reason::Point, message, Some(found)))
}

/// The menu bar counts as the app's only while the app has keyboard focus.
fn menu_bar(expect: Expect, look: &mut impl Look) -> Result<()> {
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
        /// Where the Dock takes clicks; no tiles unless a test sets them.
        dock: Option<Dock>,
        focus_reads: usize,
        window_reads: usize,
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
    }

    impl Look for Seen {
        fn focused(&mut self) -> Result<Option<i64>> {
            self.focus_reads += 1;
            match &self.focused {
                Some(Ok(pid)) => Ok(*pid),
                _ => Err(Error::new("native", "unreadable")),
            }
        }

        fn windows(&mut self) -> Result<&[Window]> {
            self.window_reads += 1;
            self.windows
                .as_deref()
                .ok_or_else(|| Error::new("windows", "unreadable"))
        }

        fn dock(&mut self) -> Dock {
            self.dock.clone().unwrap_or(Dock::Tiles(Vec::new()))
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
        require(expect(), Need::Keys, &display(), look, later())
    }

    fn point_with(look: &mut Seen, x: f64, y: f64) -> Result<()> {
        require(expect(), Need::Point(x, y), &display(), look, later())
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

    /// The Dock's full-display window lets clicks through except where it
    /// shows its tiles: there it is on top for every app, even over an
    /// app's own window under it.
    #[test]
    fn the_docks_tiles_refuse_every_app_and_the_points_beside_them_pass() {
        let dock = window(2, "Dock", 20, (0.0, 0.0, 1000.0, 800.0));
        let calculator = window(APP, "Calculator", 0, (150.0, 600.0, 300.0, 200.0));
        let finder = window(OTHER, "Finder", 0, (0.0, 0.0, 1000.0, 800.0));
        let listed = vec![dock, calculator, finder];
        let tiles = Dock::Tiles(vec![Bounds {
            x: 200.0,
            y: 730.0,
            width: 600.0,
            height: 70.0,
        }]);
        let at = |pid: i64, dock: &Dock, x: f64, y: f64| {
            let mut look = Seen::focused(Some(pid))
                .showing(listed.clone())
                .docked(dock.clone());
            let expect = Expect::new(pid).unwrap();
            require(expect, Need::Point(x, y), &display(), &mut look, later())
        };

        for (pid, x, y) in [
            (APP, 300.0, 760.0),
            (OTHER, 300.0, 760.0),
            (OTHER, 790.0, 799.0),
        ] {
            assert_eq!(
                refusal(at(pid, &tiles, x, y)),
                json!({"reason": "point", "found": {"app": "Dock", "pid": 2}}),
                "{pid} at {x},{y}"
            );
        }

        assert!(at(APP, &tiles, 170.0, 760.0).is_ok(), "beside the tiles");
        assert!(at(APP, &tiles, 300.0, 700.0).is_ok(), "above the tiles");
        assert!(at(OTHER, &tiles, 900.0, 760.0).is_ok(), "beside the tiles");

        // A Dock that could not be read covers each edge it may sit on.
        assert_eq!(
            refusal(at(APP, &Dock::Unread, 300.0, 700.0))["found"]["app"],
            "Dock"
        );
        assert_eq!(
            refusal(at(OTHER, &Dock::Unread, 10.0, 300.0))["found"]["app"],
            "Dock"
        );
        assert!(at(APP, &Dock::Unread, 300.0, 620.0).is_ok());
        assert!(at(OTHER, &Dock::Unread, 500.0, 300.0).is_ok());
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
            Need::Point(5.0, 6.0),
            "a drag needs its start, never its end"
        );
        assert_eq!(
            need(json!({"type": "pointer", "phase": "down", "x": 9, "y": 10})),
            Need::Point(9.0, 10.0)
        );
        for nothing in [
            json!({"type": "pointer", "phase": "move", "x": 9, "y": 10}),
            json!({"type": "pointer", "phase": "up", "x": 9, "y": 10}),
            json!({"type": "move", "x": 1, "y": 1}),
            json!({"type": "wait", "ms": 10}),
        ] {
            assert_eq!(need(nothing.clone()), Need::Nothing, "{nothing}");
        }
    }

    #[test]
    fn nothing_needed_is_never_looked_for() {
        let mut look = Seen::default();
        require(
            expect(),
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
            assert!(require(expect(), need, &display(), &mut late, later()).is_ok());
            assert_eq!(
                refusal(require(expect(), need, &display(), &mut late, deadline)),
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
                refusal(require(expect(), need, &display(), &mut spent, now)),
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
}
