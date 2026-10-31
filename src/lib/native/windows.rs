//! On-screen windows of the current graphical session, front to back, read
//! inside the helper login through CGWindowList. Read-only; titles appear only
//! where the session's capture permission allows them.
use crate::{
    error::{Code, Error, Result},
    native::{ax::Frame, cf},
};
use objc2_core_graphics::CGWindowListOption;
use serde::{Deserialize, Serialize};

/// The layer an open menu's window sits on (the pop-up menu level): menu
/// bar, status item and context menus alike.
pub const MENU_LAYER: i64 = 101;

/// The layer of the menu bar itself, a window of the Window Server's that
/// shows the frontmost app's titles.
pub const MENU_BAR_LAYER: i64 = 24;

/// Layers from the Dock's up hold the Dock, the menu bar and system overlays,
/// never an app's own windows.
pub const DOCK_LAYER: i64 = 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Bounds {
    /// The main display of `width` by `height` points: its corner is the
    /// origin of global points, so a window's bounds are points of it.
    pub fn display(width: f64, height: f64) -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width,
            height,
        }
    }

    /// These bounds grown by `pad` on every side.
    pub fn padded(&self, pad: f64) -> Self {
        Self {
            x: self.x - pad,
            y: self.y - pad,
            width: self.width + 2.0 * pad,
            height: self.height + 2.0 * pad,
        }
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }

    pub fn covers(&self, other: &Bounds) -> bool {
        self.x <= other.x
            && self.y <= other.y
            && self.x + self.width >= other.x + other.width
            && self.y + self.height >= other.y + other.height
    }

    /// Whether the two share an area, not just an edge or a corner.
    pub fn intersects(&self, other: &Bounds) -> bool {
        self.width > 0.0
            && self.height > 0.0
            && other.width > 0.0
            && other.height > 0.0
            && self.x < other.x + other.width
            && other.x < self.x + self.width
            && self.y < other.y + other.height
            && other.y < self.y + self.height
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    pub number: i64,
    pub pid: i64,
    pub owner: Option<String>,
    pub title: Option<String>,
    pub layer: i64,
    pub alpha: f64,
    pub bounds: Bounds,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct HitWindow {
    pub number: i64,
    pub pid: i64,
    pub start: Option<crate::process::Start>,
    pub layer: i64,
    pub bounds: Bounds,
}

/// Visible hit regions, front to back, without window or application names.
pub(crate) fn hit_windows(
    windows: &[Window],
    display: &Bounds,
    dock: &Dock,
    starts: Option<&std::collections::BTreeMap<i64, crate::process::Start>>,
) -> Vec<HitWindow> {
    windows
        .iter()
        .filter(|window| window.alpha > 0.0)
        .flat_map(|window| {
            let bounds = if window.layer > 0 && window.passes_through(display) {
                if window.is_dock() {
                    dock.areas(&window.bounds)
                } else {
                    Vec::new()
                }
            } else {
                vec![window.bounds.clone()]
            };
            bounds.into_iter().map(|bounds| HitWindow {
                number: window.number,
                pid: window.pid,
                start: starts.and_then(|starts| starts.get(&window.pid)).copied(),
                layer: window.layer,
                bounds,
            })
        })
        .collect()
}

/// Whether two ordered window lists route points to the same windows.
pub(crate) fn same_routing(before: &[Window], after: &[Window]) -> bool {
    fn route(window: &Window) -> (i64, i64, i64, &Bounds, bool) {
        (
            window.number,
            window.pid,
            window.layer,
            &window.bounds,
            window.alpha > 0.0,
        )
    }
    before.iter().map(route).eq(after.iter().map(route))
}

impl Window {
    /// Whether this is the menu bar, the Window Server's window that shows
    /// the frontmost app's titles.
    pub fn is_menu_bar(&self) -> bool {
        self.layer == MENU_BAR_LAYER && self.owner.as_deref() == Some("Window Server")
    }

    /// Whether this is one of the Dock's or Notification Center's windows
    /// spanning the whole `display`, which pass pointer events through to
    /// what is under them, except where the Dock shows its tiles (see
    /// [`Screen`]).
    pub fn passes_through(&self, display: &Bounds) -> bool {
        self.bounds.covers(display)
            && matches!(self.owner.as_deref(), Some("Dock" | "Notification Center"))
    }

    /// Whether this is a window of the Dock's.
    pub fn is_dock(&self) -> bool {
        self.owner.as_deref() == Some("Dock")
    }

    /// Whether this is one of an app's own visible windows: an ordinary
    /// one, or a floating panel or alert below the Dock's layer. An open
    /// menu is not one: `windows` and `wait` never mistake it for the window
    /// it opens.
    pub fn is_apps(&self) -> bool {
        self.layer == 0 || ((1..DOCK_LAYER).contains(&self.layer) && self.alpha > 0.0)
    }

    /// [`Window::is_apps`], or a menu the app has open: what an element
    /// command may act on.
    pub fn is_apps_or_menu(&self) -> bool {
        self.is_apps() || (self.layer == MENU_LAYER && self.alpha > 0.0)
    }
}

/// What kind of window Accessibility says a window is, by its subrole,
/// which tells at which layers the window list can show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A floating panel, never at the ordinary layer.
    Floating,
    /// A document window, a dialog, a sheet or anything else: at the
    /// ordinary layer, or above it where its app raised it, so no layer is
    /// ruled out.
    Other,
}

impl Kind {
    /// The kind a window's subrole names.
    pub fn of(subrole: Option<&str>) -> Self {
        match subrole {
            Some("AXFloatingWindow" | "AXSystemFloatingWindow") => Self::Floating,
            _ => Self::Other,
        }
    }

    /// Whether a window of this kind can be listed at `layer`.
    fn admits(self, layer: i64) -> bool {
        match self {
            Self::Floating => layer != 0,
            Self::Other => true,
        }
    }
}

/// Which listed window a window Accessibility describes is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Listed {
    /// This one, and only this one.
    One(i64),
    /// None of them.
    Missing,
    /// More than one, which cannot be told apart.
    Several,
}

/// Every listed window of process `pid` whose bounds are `frame` within a
/// point, at any layer, however faint.
fn framed<'a>(
    windows: &'a [Window],
    pid: i64,
    frame: &'a Frame,
) -> impl Iterator<Item = &'a Window> {
    windows
        .iter()
        .filter(move |window| window.pid == pid && frame.matches(&Frame::from(&window.bounds)))
}

/// The one listed window a window Accessibility describes is: of every
/// window of process `pid` at its `frame` (see [`framed`]), those whose
/// layer its `kind` admits and whose title, when both are known, holds the
/// other's (Chrome adds its own name to what Accessibility reports).
pub fn described(
    windows: &[Window],
    pid: i64,
    frame: &Frame,
    title: Option<&str>,
    kind: Kind,
) -> Listed {
    let mut matching = framed(windows, pid, frame).filter(|window| {
        kind.admits(window.layer)
            && match (window.title.as_deref(), title) {
                (Some(listed), Some(title)) => title.contains(listed) || listed.contains(title),
                _ => true,
            }
    });
    match (matching.next(), matching.next()) {
        (Some(window), None) => Listed::One(window.number),
        (None, _) => Listed::Missing,
        (Some(_), Some(_)) => Listed::Several,
    }
}

/// The window whose bounds are an element's AX window frame, for listing
/// and clicking the element: of the app's own windows and open menus at
/// that frame (see [`framed`]), an ordinary window wins over a same-sized
/// overlay. None when no window, or more than one, is left.
pub fn owning_window(windows: &[Window], pid: i64, frame: &Frame) -> Option<i64> {
    let matching: Vec<&Window> = framed(windows, pid, frame)
        .filter(|window| window.is_apps_or_menu())
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

/// How far around its list of icons the Dock's shelf takes clicks too.
const SHELF: f64 = 12.0;

/// Where the Dock takes clicks inside its full-display window. Only its
/// list of icons is known to leave the rest of the display to what is
/// under it; anything else the Dock shows counts as covering all of it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Dock {
    /// The frames of its lists of icons, as Accessibility reports them, and
    /// nothing else; none when no Dock is on screen.
    Icons(Vec<Bounds>),
    /// It shows something other than its lists of icons, so all of its
    /// window counts as taking clicks.
    Other,
    /// It could not be read, so all of its window counts as taking clicks.
    #[default]
    Unread,
}

impl Dock {
    /// Whether the Dock takes a pointer event at the point: on its icons or
    /// the shelf around them, or anywhere when what it shows is not known.
    fn takes(&self, x: f64, y: f64) -> bool {
        match self {
            Self::Icons(lists) => lists.iter().any(|list| list.padded(SHELF).contains(x, y)),
            Self::Other | Self::Unread => true,
        }
    }

    /// The parts of the Dock's window, at `bounds`, that take clicks.
    pub fn areas(&self, bounds: &Bounds) -> Vec<Bounds> {
        match self {
            Self::Icons(lists) => lists.iter().map(|list| list.padded(SHELF)).collect(),
            Self::Other | Self::Unread => vec![bounds.clone()],
        }
    }
}

/// What pointer events are judged against: the display, and where its
/// Dock takes clicks.
#[derive(Debug, Clone, PartialEq)]
pub struct Screen {
    display: Bounds,
    dock: Dock,
}

impl Screen {
    pub fn new(display: Bounds, dock: Dock) -> Self {
        Self { display, dock }
    }

    /// Where the Dock takes clicks.
    pub fn dock(&self) -> &Dock {
        &self.dock
    }

    /// Whether `window` lets a pointer event at the point through to what
    /// is under it: a full-display click-through overlay, but not where the
    /// Dock takes clicks.
    fn passes(&self, window: &Window, x: f64, y: f64) -> bool {
        window.passes_through(&self.display) && !(window.is_dock() && self.dock.takes(x, y))
    }
}

/// What a `windows-info` probe run in the helper reports: the on-screen
/// windows, front to back, and, when asked for, where the Dock takes
/// clicks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shown {
    pub windows: Vec<Window>,
    /// Process identities from this snapshot; absent only for older probes.
    pub starts: Option<std::collections::BTreeMap<i64, crate::process::Start>>,
    /// Unread when the probe does not say: it was not asked, or is older.
    #[serde(default)]
    pub dock: Dock,
}

/// One window-list entry, if it carries the fields every window has.
fn read_window(entry: &cf::Record) -> Option<Window> {
    // The bounds dictionary holds X, Y, Width and Height as numbers.
    let bounds = cf::nested(entry, "kCGWindowBounds")?;
    let side = |key| cf::float(&bounds, key);

    Some(Window {
        number: cf::integer(entry, "kCGWindowNumber")?,
        pid: cf::integer(entry, "kCGWindowOwnerPID")?,
        owner: cf::text(entry, "kCGWindowOwnerName"),
        title: cf::text(entry, "kCGWindowName"),
        layer: cf::integer(entry, "kCGWindowLayer")?,
        alpha: cf::float(entry, "kCGWindowAlpha").unwrap_or(1.0),
        bounds: Bounds {
            x: side("X")?,
            y: side("Y")?,
            width: side("Width")?,
            height: side("Height")?,
        },
    })
}

/// What a `windows-info` probe run in the helper reported, from the helper
/// daemon's reply for that run; None when it did not answer whole.
pub fn from_probe(reply: &serde_json::Value) -> Option<Shown> {
    let output = crate::transport::run_output(reply, 1 << 20).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&output).ok()?;
    (value["ok"] == true)
        .then(|| Shown::deserialize(&value).ok())
        .flatten()
}

/// On-screen windows in front-to-back order.
pub fn windows() -> Result<Vec<Window>> {
    let options =
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements;
    let list = cf::windows(options)
        .ok_or_else(|| Error::new(Code::Windows, "The window list is unavailable"))?;

    Ok(list.iter().filter_map(|entry| read_window(entry)).collect())
}

/// Whether the point lies inside window `target` with nothing in front of it
/// covering that point. `windows` is front to back. Only the Dock's and
/// Notification Center's higher-layer windows spanning the whole display
/// are click-through overlays, the Dock's only away from where it takes
/// clicks; every other window in front, the menu bar and any unidentified
/// overlay included, occludes.
pub fn unobstructed(windows: &[Window], target: i64, x: f64, y: f64, screen: &Screen) -> bool {
    let Some(index) = windows.iter().position(|window| window.number == target) else {
        return false;
    };

    let window = &windows[index];
    window.bounds.contains(x, y)
        && !windows[..index].iter().any(|other| {
            let overlay = other.layer > window.layer && screen.passes(other, x, y);
            other.alpha > 0.0 && !overlay && other.bounds.contains(x, y)
        })
}

/// The window shown on top at the point: the frontmost visible one
/// containing it, passing over the Dock's and Notification Center's
/// full-display click-through overlays above the ordinary layer, the same
/// overlays [`unobstructed`] passes over. Where the Dock takes clicks that
/// is the Dock's window. `windows` is front to back.
pub fn top_at<'a>(windows: &'a [Window], x: f64, y: f64, screen: &Screen) -> Option<&'a Window> {
    windows.iter().find(|window| {
        let overlay = window.layer > 0 && screen.passes(window, x, y);
        window.alpha > 0.0 && !overlay && window.bounds.contains(x, y)
    })
}

/// Whether a pointer event at the point reaches process `pid`: one of its
/// windows there is [`unobstructed`], the test a named click's element
/// passes. A window of its that shows nothing (alpha 0) counts as it does
/// there, though [`top_at`] names what is shown behind it.
pub fn reaches(windows: &[Window], pid: i64, x: f64, y: f64, screen: &Screen) -> bool {
    windows
        .iter()
        .any(|window| window.pid == pid && unobstructed(windows, window.number, x, y, screen))
}

/// A whole point inside `target` that is [`unobstructed`], preferring points
/// near its centre. The point that is tested is the point that is clicked.
pub fn visible_point(windows: &[Window], target: i64, screen: &Screen) -> Option<(f64, f64)> {
    let bounds = &windows
        .iter()
        .find(|window| window.number == target)?
        .bounds;
    let steps = 24;
    let mut candidates = Vec::new();

    for row in 1..steps {
        for column in 1..steps {
            let x = (bounds.x + bounds.width * f64::from(column) / f64::from(steps)).round();
            let y = (bounds.y + bounds.height * f64::from(row) / f64::from(steps)).round();
            let distance = (x - (bounds.x + bounds.width / 2.0)).powi(2)
                + (y - (bounds.y + bounds.height / 2.0)).powi(2);
            candidates.push((distance, x, y));
        }
    }

    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    candidates
        .into_iter()
        .find(|(_, x, y)| unobstructed(windows, target, *x, *y, screen))
        .map(|(_, x, y)| (x, y))
}

/// The process owning the frontmost ordinary (layer 0) window.
pub fn frontmost_pid(windows: &[Window]) -> Option<i64> {
    windows
        .iter()
        .find(|window| window.layer == 0)
        .map(|window| window.pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_compares_visibility_but_ignores_names_and_visible_alpha_changes() {
        let before = vec![placed(42, 0, None, 1.0), placed(43, 0, None, 1.0)];
        for change in 0..8 {
            let mut after = before.clone();
            match change {
                0 => after[0].alpha = 0.0,
                1 => after[0].alpha = 0.5,
                2 => after[0].title = Some("Renamed".into()),
                3 => after.reverse(),
                4 => after[0].bounds.x += 1.0,
                5 => after[0].pid += 1,
                6 => after[0].layer += 1,
                _ => {
                    after.pop();
                }
            }
            assert_eq!(same_routing(&before, &after), matches!(change, 1 | 2));
            assert_eq!(same_routing(&after, &before), matches!(change, 1 | 2));
        }
    }

    /// A window of process 8 at `layer` with `title`, at one place.
    fn placed(number: i64, layer: i64, title: Option<&str>, alpha: f64) -> Window {
        Window {
            number,
            pid: 8,
            owner: Some("TextEdit".into()),
            title: title.map(str::to_owned),
            layer,
            alpha,
            bounds: Bounds {
                x: 10.0,
                y: 20.0,
                width: 300.0,
                height: 200.0,
            },
        }
    }

    /// Where every window here is placed, as Accessibility reports it.
    fn frame() -> Frame {
        Frame::from(&placed(0, 0, None, 1.0).bounds)
    }

    #[test]
    fn a_described_window_is_the_one_its_frame_kind_and_title_leave() {
        let document = placed(5, 0, Some("Letter.txt"), 1.0);
        let panel = placed(6, 3, Some("Fonts"), 1.0);
        let faint = placed(7, 3, None, 0.0);
        let listed = [panel.clone(), document.clone()];
        let at = |listed: &[Window], title, kind| described(listed, 8, &frame(), title, kind);

        assert_eq!(at(&listed, None, Kind::Floating), Listed::One(6));
        assert_eq!(
            at(&listed, None, Kind::Other),
            Listed::Several,
            "a document window may be raised above the ordinary layer"
        );
        assert_eq!(at(&listed, Some("Fonts"), Kind::Other), Listed::One(6));
        assert_eq!(
            at(&[faint, document.clone()], None, Kind::Other),
            Listed::Several,
            "a faint window at the same place still counts"
        );
        assert_eq!(
            at(std::slice::from_ref(&document), None, Kind::Floating),
            Listed::Missing
        );
        assert_eq!(at(&[document], None, Kind::Other), Listed::One(5));
        assert_eq!(
            described(&listed, 9, &frame(), None, Kind::Other),
            Listed::Missing
        );
    }

    #[test]
    fn kinds_follow_the_subrole_and_rule_out_layers() {
        assert_eq!(Kind::of(Some("AXStandardWindow")), Kind::Other);
        assert_eq!(Kind::of(Some("AXDialog")), Kind::Other);
        assert_eq!(Kind::of(Some("AXFloatingWindow")), Kind::Floating);
        assert_eq!(Kind::of(None), Kind::Other);
        assert!(!Kind::Floating.admits(0) && Kind::Floating.admits(3));
        assert!(Kind::Other.admits(0) && Kind::Other.admits(101));
    }

    /// The literal keys read here are the framework's exported constants.
    #[test]
    #[allow(
        unsafe_code,
        reason = "FFI: the framework's constants are extern statics"
    )]
    fn window_keys_match_the_framework_constants() {
        use objc2_core_graphics::{
            kCGWindowAlpha, kCGWindowBounds, kCGWindowLayer, kCGWindowName, kCGWindowNumber,
            kCGWindowOwnerName, kCGWindowOwnerPID,
        };

        // SAFETY: reading immutable framework constants in a test.
        let constants = unsafe {
            [
                kCGWindowNumber,
                kCGWindowOwnerPID,
                kCGWindowOwnerName,
                kCGWindowName,
                kCGWindowLayer,
                kCGWindowAlpha,
                kCGWindowBounds,
            ]
        };

        let names = [
            "kCGWindowNumber",
            "kCGWindowOwnerPID",
            "kCGWindowOwnerName",
            "kCGWindowName",
            "kCGWindowLayer",
            "kCGWindowAlpha",
            "kCGWindowBounds",
        ];

        for (constant, name) in constants.into_iter().zip(names) {
            assert_eq!(constant.to_string(), name);
        }
    }

    fn window(number: i64, x: f64, y: f64, width: f64, height: f64) -> Window {
        Window {
            number,
            pid: number,
            owner: None,
            title: None,
            layer: 0,
            alpha: 1.0,
            bounds: Bounds {
                x,
                y,
                width,
                height,
            },
        }
    }

    #[test]
    fn bounds_intersect_only_with_area_in_common() {
        let square = window(1, 0.0, 0.0, 100.0, 100.0).bounds;
        let inside = window(2, 10.0, 10.0, 5.0, 5.0).bounds;
        let overlapping = window(3, 90.0, 90.0, 50.0, 50.0).bounds;
        let touching = window(4, 100.0, 0.0, 50.0, 100.0).bounds;
        let corner = window(5, 100.0, 100.0, 10.0, 10.0).bounds;
        let apart = window(6, 300.0, 300.0, 10.0, 10.0).bounds;
        let line = window(7, 50.0, 0.0, 0.0, 100.0).bounds;

        assert!(square.intersects(&inside) && inside.intersects(&square));
        assert!(square.intersects(&overlapping) && overlapping.intersects(&square));
        assert!(!square.intersects(&touching), "an edge in common");
        assert!(!square.intersects(&corner), "a corner in common");
        assert!(!square.intersects(&apart));
        assert!(!square.intersects(&line), "no area of its own");
    }

    fn display() -> Bounds {
        Bounds::display(1000.0, 800.0)
    }

    /// The test display with the Dock's icons along its bottom edge.
    fn docked() -> Screen {
        Screen::new(display(), Dock::Icons(vec![dock_icons()]))
    }

    fn dock_icons() -> Bounds {
        Bounds {
            x: 200.0,
            y: 740.0,
            width: 600.0,
            height: 60.0,
        }
    }

    /// The test display with no Dock icons on it.
    fn undocked() -> Screen {
        Screen::new(display(), Dock::Icons(Vec::new()))
    }

    #[test]
    fn visible_points_avoid_windows_in_front() {
        let display = undocked();
        let target = window(2, 0.0, 0.0, 100.0, 100.0);
        let point = |windows: &[Window]| visible_point(windows, 2, &display);
        assert_eq!(point(std::slice::from_ref(&target)), Some((50.0, 50.0)));

        let cover = window(1, 0.0, 0.0, 80.0, 100.0);
        assert!(point(&[cover, target.clone()]).unwrap().0 >= 80.0);

        let behind = window(3, 0.0, 0.0, 100.0, 100.0);
        assert_eq!(point(&[target.clone(), behind]), Some((50.0, 50.0)));

        let mut overlay = window(1, 0.0, 0.0, 1000.0, 800.0);
        overlay.layer = 20;
        overlay.owner = Some("Dock".into());
        assert_eq!(
            point(&[overlay, target.clone()]),
            Some((50.0, 50.0)),
            "full-display overlays pass clicks through"
        );

        let mut unknown = window(1, 0.0, 0.0, 1000.0, 800.0);
        unknown.layer = 20;
        unknown.owner = Some("Other".into());
        assert_eq!(
            point(&[unknown, target.clone()]),
            None,
            "unidentified overlays fail closed"
        );

        let mut floating = window(1, -10.0, -10.0, 200.0, 200.0);
        floating.layer = 3;
        assert_eq!(
            point(&[floating, target.clone()]),
            None,
            "a floating panel over the target still occludes"
        );

        let mut menu = window(1, 0.0, 0.0, 100.0, 60.0);
        menu.layer = 24;
        assert!(point(&[menu, target.clone()]).unwrap().1 >= 60.0);

        // The tested point is the clicked point: rounding cannot move it under
        // an occluder (unrounded centre 50.5 would round into the cover).
        let odd = window(2, 0.0, 0.0, 101.0, 100.0);
        let right = window(1, 51.0, 0.0, 50.0, 100.0);
        let (x, _) = point(&[right, odd]).unwrap();
        assert!(x < 51.0, "{x}");

        assert_eq!(point(&[]), None);

        let menu_bar = {
            let mut menu = window(9, 0.0, 0.0, 1000.0, 30.0);
            menu.layer = 24;
            menu
        };
        let windows = [
            menu_bar,
            window(2, 0.0, 0.0, 100.0, 100.0),
            window(3, 0.0, 0.0, 9.0, 9.0),
        ];

        assert!(
            !unobstructed(&windows, 2, 10.0, 10.0, &display),
            "under the menu bar"
        );
        assert!(unobstructed(&windows, 2, 10.0, 40.0, &display));
        assert!(
            !unobstructed(&windows, 2, 150.0, 40.0, &display),
            "outside the window"
        );
        assert!(
            !unobstructed(&windows, 7, 10.0, 40.0, &display),
            "unknown window"
        );
        assert_eq!(frontmost_pid(&windows), Some(2));
        assert_eq!(frontmost_pid(&windows[..1]), None);
    }

    /// A named click's element under the Dock's icons or the shelf around
    /// them is covered for every app, so no visible point is found there;
    /// beside the shelf the Dock lets clicks through.
    #[test]
    fn the_docks_icons_cover_every_window_under_them() {
        let mut dock = window(1, 0.0, 0.0, 1000.0, 800.0);
        dock.layer = 20;
        dock.owner = Some("Dock".into());
        let under = window(2, 300.0, 740.0, 100.0, 60.0);
        let beside = window(3, 0.0, 700.0, 150.0, 100.0);
        let behind = window(4, 0.0, 0.0, 1000.0, 800.0);
        let windows = [dock, under, beside, behind];

        for (target, x, y) in [(2, 350.0, 770.0), (4, 600.0, 770.0), (4, 195.0, 770.0)] {
            assert!(
                !unobstructed(&windows, target, x, y, &docked()),
                "window {target} at {x},{y}"
            );
            assert!(
                unobstructed(&windows, target, x, y, &undocked()),
                "window {target} at {x},{y} with no icons"
            );
        }
        assert_eq!(top_at(&windows, 350.0, 770.0, &docked()).unwrap().number, 1);
        assert_eq!(visible_point(&windows, 2, &docked()), None);

        assert!(unobstructed(&windows, 3, 75.0, 750.0, &docked()));
        assert_eq!(visible_point(&windows, 3, &docked()), Some((75.0, 750.0)));
        assert!(unobstructed(&windows, 4, 500.0, 700.0, &docked()));
    }

    /// A Dock that shows more than its icons, or could not be read, covers
    /// every window under it anywhere on the display.
    #[test]
    fn a_dock_that_is_not_only_its_icons_covers_the_whole_display() {
        let mut dock = window(1, 0.0, 0.0, 1000.0, 800.0);
        dock.layer = 20;
        dock.owner = Some("Dock".into());
        let mut menu = window(5, 100.0, 100.0, 100.0, 100.0);
        menu.layer = MENU_LAYER;
        let windows = [menu, dock, window(4, 0.0, 0.0, 1000.0, 800.0)];

        for dock in [Dock::Other, Dock::Unread] {
            let screen = Screen::new(display(), dock);
            for (x, y) in [(500.0, 300.0), (10.0, 10.0), (75.0, 750.0)] {
                assert!(
                    !unobstructed(&windows, 4, x, y, &screen),
                    "{screen:?} {x},{y}"
                );
                assert!(top_at(&windows, x, y, &screen).unwrap().is_dock());
            }
            assert_eq!(visible_point(&windows, 4, &screen), None);
            assert!(
                unobstructed(&windows, 5, 150.0, 150.0, &screen),
                "a menu in front of the Dock"
            );
        }
    }

    #[test]
    fn a_probe_says_where_the_dock_takes_clicks_or_leaves_it_unread() {
        use serde_json::json;

        let reply = |output: serde_json::Value| crate::test_support::completed(&output.to_string());
        let icons = json!({"icons": [{"x": 200.0, "y": 740.0, "width": 600.0, "height": 60.0}]});
        for (said, dock) in [
            (icons, Dock::Icons(vec![dock_icons()])),
            (json!("other"), Dock::Other),
            (json!("unread"), Dock::Unread),
        ] {
            let shown =
                from_probe(&reply(json!({"ok": true, "windows": [], "dock": said}))).unwrap();
            assert_eq!(shown.dock, dock);
            assert_eq!(serde_json::to_value(&dock).unwrap(), said);
        }

        let unasked = from_probe(&reply(json!({"ok": true, "windows": []}))).unwrap();
        assert_eq!(
            unasked.dock,
            Dock::Unread,
            "not asked for, or an older probe"
        );
    }

    #[test]
    fn the_top_window_at_a_point_agrees_with_the_unobstructed_check() {
        let layered = |number, layer, owner: &str, bounds: (f64, f64, f64, f64)| {
            let mut window = window(number, bounds.0, bounds.1, bounds.2, bounds.3);
            window.layer = layer;
            window.owner = Some(owner.to_owned());
            window
        };
        let mut hidden = layered(8, 0, "Ghost", (0.0, 0.0, 1000.0, 800.0));
        hidden.alpha = 0.0;
        let fixtures = [
            vec![
                layered(1, MENU_LAYER, "TextEdit", (100.0, 24.0, 200.0, 300.0)),
                layered(2, MENU_BAR_LAYER, "Window Server", (0.0, 0.0, 1000.0, 24.0)),
                layered(3, 20, "Dock", (0.0, 0.0, 1000.0, 800.0)),
                layered(4, 3, "Notes", (700.0, 100.0, 200.0, 200.0)),
                layered(5, 0, "Calculator", (250.0, 200.0, 300.0, 300.0)),
                layered(6, 0, "TextEdit", (50.0, 50.0, 600.0, 500.0)),
                layered(7, 0, "Finder", (0.0, 0.0, 1000.0, 800.0)),
            ],
            vec![
                hidden,
                layered(9, 20, "Notification Center", (0.0, 0.0, 1000.0, 800.0)),
                layered(10, 20, "Other", (0.0, 0.0, 500.0, 800.0)),
                layered(11, 0, "TextEdit", (100.0, 100.0, 800.0, 600.0)),
            ],
        ];

        let screens = [undocked(), docked(), Screen::new(display(), Dock::Unread)];

        // Each fixture window is its own process.
        for windows in &fixtures {
            for screen in &screens {
                let hits = hit_windows(windows, &display(), screen.dock(), None);
                for x in (0..1000).step_by(25) {
                    for y in (0..800).step_by(25) {
                        let (x, y) = (f64::from(x), f64::from(y));
                        let top = top_at(windows, x, y, screen).map(|window| window.number);
                        assert_eq!(
                            hits.iter()
                                .find(|window| window.bounds.contains(x, y))
                                .map(|window| window.number),
                            top
                        );
                        for ordinary in windows.iter().filter(|window| window.layer == 0) {
                            let clear = unobstructed(windows, ordinary.number, x, y, screen);
                            let at = format!(
                                "window {} at {x},{y} with {:?}",
                                ordinary.number, screen.dock
                            );
                            assert_eq!(reaches(windows, ordinary.pid, x, y, screen), clear, "{at}");
                            if ordinary.alpha > 0.0 {
                                assert_eq!(top == Some(ordinary.number), clear, "{at}");
                            }
                        }
                    }
                }
            }
        }
        assert!(
            reaches(&fixtures[1], 8, 300.0, 300.0, &undocked()),
            "an ordinary window that shows nothing is reached where it is clear"
        );

        let first = &fixtures[0];
        assert_eq!(top_at(first, 150.0, 100.0, &undocked()).unwrap().number, 1);
        assert!(
            top_at(first, 10.0, 10.0, &undocked())
                .unwrap()
                .is_menu_bar()
        );
        assert_eq!(top_at(first, 750.0, 150.0, &undocked()).unwrap().number, 4);
        assert_eq!(
            top_at(&fixtures[1], 300.0, 300.0, &undocked())
                .unwrap()
                .number,
            10
        );
        assert_eq!(top_at(&[], 1.0, 1.0, &undocked()), None);

        // On the Dock's tiles the Dock is on top and no app is reached;
        // beside them the app under the Dock is.
        assert_eq!(top_at(first, 300.0, 775.0, &docked()).unwrap().number, 3);
        assert_eq!(top_at(first, 300.0, 775.0, &undocked()).unwrap().number, 7);
        for pid in [5, 6, 7] {
            assert!(!reaches(first, pid, 300.0, 775.0, &docked()), "{pid}");
        }
        assert!(reaches(first, 7, 100.0, 775.0, &docked()));
    }

    /// Window dictionaries are read through the same keys CoreGraphics uses.
    #[test]
    fn window_entries_read_through_typed_keys() {
        use crate::native::cf::{CFDictionary, CFNumber, CFString, CFType};

        let number = |value: f64| CFNumber::new_f64(value);
        let keys = ["X", "Y", "Width", "Height"].map(CFString::from_static_str);
        let sides = [number(1.0), number(2.0), number(30.0), number(40.0)];
        let bounds = CFDictionary::<CFString, CFNumber>::from_slices(
            &keys.iter().map(|key| &**key).collect::<Vec<_>>(),
            &sides.iter().map(|side| &**side).collect::<Vec<_>>(),
        );

        let names = [
            "kCGWindowNumber",
            "kCGWindowOwnerPID",
            "kCGWindowLayer",
            "kCGWindowOwnerName",
            "kCGWindowBounds",
        ]
        .map(CFString::from_static_str);

        let (seven, pid, layer) = (
            CFNumber::new_i64(7),
            CFNumber::new_i64(99),
            CFNumber::new_i64(0),
        );
        let owner = CFString::from_static_str("Finder");
        let values: [&CFType; 5] = [
            seven.as_ref(),
            pid.as_ref(),
            layer.as_ref(),
            owner.as_ref(),
            bounds.as_opaque().as_ref(),
        ];

        let entry = CFDictionary::<CFString, CFType>::from_slices(
            &names.iter().map(|name| &**name).collect::<Vec<_>>(),
            &values,
        );
        let read = read_window(&entry).unwrap();

        assert_eq!((read.number, read.pid, read.layer), (7, 99, 0));
        assert_eq!(read.owner.as_deref(), Some("Finder"));
        assert_eq!(read.title, None);
        assert_eq!(read.alpha, 1.0);
        assert_eq!(
            read.bounds,
            Bounds {
                x: 1.0,
                y: 2.0,
                width: 30.0,
                height: 40.0
            }
        );
    }

    #[test]
    fn the_current_session_lists_windows() {
        // Any logged-in test session has at least the menu bar or Dock.
        let windows = windows().unwrap();
        assert!(windows.iter().all(|w| w.bounds.width >= 0.0 && w.pid > 0));
    }
}
