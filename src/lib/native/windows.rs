//! On-screen windows of the current graphical session, front to back, read
//! inside the helper login through CGWindowList. Read-only; titles appear only
//! where the session's capture permission allows them.
use crate::{
    error::{Error, Result},
    native::cf,
};
use objc2_core_graphics::CGWindowListOption;
use serde::{Deserialize, Serialize};

/// The layer an open menu's window sits on (the pop-up menu level): menu
/// bar, status item and context menus alike.
pub const MENU_LAYER: i64 = 101;

/// The layer of the menu bar itself, a window of the Window Server's that
/// shows the frontmost app's titles.
pub const MENU_BAR_LAYER: i64 = 24;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Bounds {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }

    pub fn covers(&self, other: &Bounds) -> bool {
        self.x <= other.x
            && self.y <= other.y
            && self.x + self.width >= other.x + other.width
            && self.y + self.height >= other.y + other.height
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

/// The windows a `windows-info` probe run in the helper reported, from the
/// helper daemon's reply for that run; None when it did not answer whole.
pub fn from_probe(reply: &serde_json::Value) -> Option<Vec<Window>> {
    let output = crate::transport::run_output(reply, 1 << 20).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&output).ok()?;
    (value["ok"] == true)
        .then(|| Vec::<Window>::deserialize(&value["windows"]).ok())
        .flatten()
}

/// On-screen windows in front-to-back order.
pub fn windows() -> Result<Vec<Window>> {
    let options =
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements;
    let list = cf::windows(options)
        .ok_or_else(|| Error::new("windows", "The window list is unavailable"))?;
    Ok(list.iter().filter_map(|entry| read_window(entry)).collect())
}

/// Whether the point lies inside window `target` with nothing in front of it
/// covering that point. `windows` is front to back. Only the Dock's and
/// Notification Center's higher-layer windows spanning the whole `display`
/// are click-through overlays; every other window in front, the menu bar and
/// any unidentified overlay included, occludes.
pub fn unobstructed(windows: &[Window], target: i64, x: f64, y: f64, display: &Bounds) -> bool {
    let Some(index) = windows.iter().position(|window| window.number == target) else {
        return false;
    };
    let window = &windows[index];
    window.bounds.contains(x, y)
        && !windows[..index].iter().any(|other| {
            let overlay = other.layer > window.layer
                && other.bounds.covers(display)
                && matches!(other.owner.as_deref(), Some("Dock" | "Notification Center"));
            other.alpha > 0.0 && !overlay && other.bounds.contains(x, y)
        })
}

/// A whole point inside `target` that is [`unobstructed`], preferring points
/// near its centre. The point that is tested is the point that is clicked.
pub fn visible_point(windows: &[Window], target: i64, display: &Bounds) -> Option<(f64, f64)> {
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
        .find(|(_, x, y)| unobstructed(windows, target, *x, *y, display))
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

    /// The literal keys read here are the framework's exported constants.
    #[test]
    #[allow(unsafe_code)]
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
    fn visible_points_avoid_windows_in_front() {
        let display = Bounds {
            x: 0.0,
            y: 0.0,
            width: 1000.0,
            height: 800.0,
        };
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
