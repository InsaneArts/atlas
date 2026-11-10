//! What part of the display a cropped screenshot shows: an app's windows
//! with a small margin, or a region cut to the display.
use atlas_core::{
    protocol::{Part, Region},
    screen::windows::{Bounds, Dock, MENU_BAR_LAYER, Window},
};

/// Points of the display shown around an app's windows, so their edges and
/// focus rings are in the picture.
const PADDING: f64 = 8.0;

/// The part of the display showing one app's windows.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct AppCrop {
    pub(super) region: Region,
    /// The owner of the app's frontmost window.
    pub(super) name: Option<String>,
    /// Other apps' windows in front of the app's, inside the crop.
    pub(super) overlapping: Vec<Window>,
}

fn bounds(region: &Region) -> Bounds {
    Bounds {
        x: f64::from(region.x),
        y: f64::from(region.y),
        width: f64::from(region.width),
        height: f64::from(region.height),
    }
}

/// The part of `bounds` inside `area`, empty when they do not meet.
fn clipped(bounds: &Bounds, area: &Bounds) -> Bounds {
    let left = bounds.x.max(area.x);
    let top = bounds.y.max(area.y);
    let right = (bounds.x + bounds.width).min(area.x + area.width);
    let bottom = (bounds.y + bounds.height).min(area.y + area.height);

    Bounds {
        x: left,
        y: top,
        width: (right - left).max(0.0),
        height: (bottom - top).max(0.0),
    }
}

/// `region` cut to the display; None when nothing of it is on the display.
pub(super) fn clamp(region: Region, display: &Region) -> Option<Region> {
    let left = region.x.max(display.x);
    let top = region.y.max(display.y);
    let right = (region.x + region.width).min(display.x + display.width);
    let bottom = (region.y + region.height).min(display.y + display.height);

    (right > left && bottom > top).then_some(Region {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

/// The crop showing every on-screen window of `pid`, or only the `part` of
/// it asked for, and the windows of others in front of them inside it, the
/// parts of the Dock that take clicks among them; None when nothing of the
/// app is there. `windows` is front to back, in display points.
pub(super) fn app(
    windows: &[Window],
    pid: i64,
    part: Option<Part>,
    display: &Region,
    dock: &Dock,
) -> Option<AppCrop> {
    let screen = bounds(display);
    let strips: Vec<&Bounds> = windows
        .iter()
        .filter(|window| window.layer == MENU_BAR_LAYER)
        .map(|window| &window.bounds)
        .collect();
    // Status items and the menu bar's own titles would stretch a crop across
    // the whole top of the display. Only the part on the display counts: the
    // recording indicator reaches past its edge.
    let in_menu_bar = |window: &Window| {
        let shown = clipped(&window.bounds, &screen);
        strips.iter().any(|strip| strip.covers(&shown))
    };

    let included: Vec<(usize, &Window)> = windows
        .iter()
        .enumerate()
        .filter(|(_, window)| {
            window.pid == pid
                && window.alpha > 0.0
                && window.bounds.width >= 1.0
                && window.bounds.height >= 1.0
                && !in_menu_bar(window)
                && window.bounds.intersects(&screen)
                && match part {
                    Some(Part::Window(number)) => window.number == number,
                    Some(Part::Within(within)) => window.bounds.intersects(&bounds(&within)),
                    None => true,
                }
        })
        .collect();
    let ((_, front), (back, _)) = (included.first()?, included.last()?);

    let (mut left, mut top) = (f64::INFINITY, f64::INFINITY);
    let (mut right, mut bottom) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for bounds in included.iter().map(|(_, window)| &window.bounds) {
        left = left.min(bounds.x);
        top = top.min(bounds.y);
        right = right.max(bounds.x + bounds.width);
        bottom = bottom.max(bounds.y + bounds.height);
    }

    let left = (left - PADDING).max(screen.x).floor();
    let top = (top - PADDING).max(screen.y).floor();
    let right = (right + PADDING).min(screen.x + screen.width).ceil();
    let bottom = (bottom + PADDING).min(screen.y + screen.height).ceil();
    if right - left < 1.0 || bottom - top < 1.0 {
        return None;
    }

    let region = Region {
        x: left as i32,
        y: top as i32,
        width: (right - left) as i32,
        height: (bottom - top) as i32,
    };
    let region = match part {
        Some(Part::Within(within)) => clamp(region, &within)?,
        Some(Part::Window(_)) | None => region,
    };
    let area = bounds(&region);
    let overlay = |window: &Window| window.passes_through(&screen);
    let in_front = || {
        // `back` came from enumerating this slice.
        // A window asked for alone is covered by the app's other windows too.
        windows[..*back].iter().filter(|window| {
            let other = match part {
                Some(Part::Window(number)) => window.number != number,
                Some(Part::Within(_)) | None => window.pid != pid,
            };
            other && window.alpha > 0.0
        })
    };
    let mut overlapping: Vec<Window> = in_front()
        .filter(|window| {
            window.layer != MENU_BAR_LAYER
                && !in_menu_bar(window)
                && !overlay(window)
                && window.bounds.intersects(&area)
        })
        .cloned()
        .collect();
    // The Dock's full-display window lets clicks through but for where it
    // takes them: those parts of it are in front of the app.
    for window in in_front().filter(|window| overlay(window) && window.is_dock()) {
        for part in dock.areas(&window.bounds) {
            if part.intersects(&area) {
                overlapping.push(Window {
                    bounds: part,
                    ..window.clone()
                });
            }
        }
    }

    Some(AppCrop {
        region,
        name: front.owner.clone(),
        overlapping,
    })
}

#[cfg(test)]
#[path = "crop_tests.rs"]
mod tests;
