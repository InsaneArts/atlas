//! Observed windows, front to back, and portable interpretation of where
//! their visible regions route input. Native readers supply the observations.
pub use super::geometry::Bounds;
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
pub struct HitWindow {
    pub number: i64,
    pub pid: i64,
    pub start: Option<crate::process::Start>,
    pub layer: i64,
    pub bounds: Bounds,
}

/// Visible hit regions, front to back, without window or application names.
pub fn hit_windows(
    windows: &[Window],
    display: &Bounds,
    dock: &Dock,
    starts: &std::collections::BTreeMap<i64, crate::process::Start>,
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
                start: starts.get(&window.pid).copied(),
                layer: window.layer,
                bounds,
            })
        })
        .collect()
}

/// Whether two ordered window lists route points to the same visible windows.
pub fn same_routing(before: &[Window], after: &[Window]) -> bool {
    fn routes(windows: &[Window]) -> impl Iterator<Item = (i64, i64, i64, &Bounds)> {
        windows
            .iter()
            .filter(|window| window.alpha > 0.0 && !window.is_capture_indicator())
            .map(|window| (window.number, window.pid, window.layer, &window.bounds))
    }
    routes(before).eq(routes(after))
}

impl Window {
    /// Whether this is the indicator the Window Server raises above the menu
    /// bar when a capture starts: it comes and goes by itself and moves no
    /// click to another window.
    fn is_capture_indicator(&self) -> bool {
        self.layer > MENU_BAR_LAYER && self.owner.as_deref() == Some("Window Server")
    }

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
    frame: &'a Bounds,
) -> impl Iterator<Item = &'a Window> {
    windows
        .iter()
        .filter(move |window| window.pid == pid && frame.matches(&window.bounds))
}

/// The one listed window a window Accessibility describes is: of every
/// window of process `pid` at its `frame` (see [`framed`]), those at a
/// layer its `subrole` allows and whose title, when both are known, holds
/// the other's (Chrome adds its own name to what Accessibility reports). A
/// floating panel is never at the ordinary layer; any other window, a
/// document, dialog or sheet, may be raised above it, so no layer is ruled
/// out.
pub fn described(
    windows: &[Window],
    pid: i64,
    frame: &Bounds,
    title: Option<&str>,
    subrole: Option<&str>,
) -> Listed {
    let floating = matches!(subrole, Some("AXFloatingWindow" | "AXSystemFloatingWindow"));
    let mut matching = framed(windows, pid, frame).filter(|window| {
        !(floating && window.layer == 0)
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
pub fn owning_window(windows: &[Window], pid: i64, frame: &Bounds) -> Option<i64> {
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

    /// Whether `window` shows at the point: visible there, and not one of
    /// the overlays above `layer` that let the point through.
    fn shows(&self, window: &Window, layer: i64, x: f64, y: f64) -> bool {
        let overlay = window.layer > layer && self.passes(window, x, y);
        window.alpha > 0.0 && !overlay && window.bounds.contains(x, y)
    }
}

/// What the helper daemon reports: the on-screen
/// windows, front to back, and, when asked for, where the Dock takes
/// clicks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shown {
    pub windows: Vec<Window>,
    /// Process identities from this snapshot.
    pub starts: std::collections::BTreeMap<i64, crate::process::Start>,
    /// Unread when it was not asked for.
    #[serde(default)]
    pub dock: Dock,
}

/// What the helper daemon reported; None when it did not answer whole.
pub fn from_reply(value: &serde_json::Value) -> Option<Shown> {
    (value["ok"] == true)
        .then(|| Shown::deserialize(value).ok())
        .flatten()
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
        && !windows[..index]
            .iter()
            .any(|other| screen.shows(other, window.layer, x, y))
}

/// The window shown on top at the point: the frontmost visible one
/// containing it, passing over the Dock's and Notification Center's
/// full-display click-through overlays above the ordinary layer, the same
/// overlays [`unobstructed`] passes over. Where the Dock takes clicks that
/// is the Dock's window. `windows` is front to back.
pub fn top_at<'a>(windows: &'a [Window], x: f64, y: f64, screen: &Screen) -> Option<&'a Window> {
    windows.iter().find(|window| screen.shows(window, 0, x, y))
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
mod tests;
