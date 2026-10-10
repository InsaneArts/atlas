//! On-screen windows of the current graphical session, front to back, read
//! inside the helper login through CGWindowList. Read-only; titles appear only
//! where the session's capture permission allows them.
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::ffi::{c_char, c_void};

type CfRef = *const c_void;

#[repr(C)]
#[derive(Default)]
struct Point {
    x: f64,
    y: f64,
}
#[repr(C)]
#[derive(Default)]
struct Size {
    width: f64,
    height: f64,
}
#[repr(C)]
#[derive(Default)]
struct Rect {
    origin: Point,
    size: Size,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> CfRef;
    fn CGRectMakeWithDictionaryRepresentation(dictionary: CfRef, rect: *mut Rect) -> bool;
    static kCGWindowNumber: CfRef;
    static kCGWindowOwnerPID: CfRef;
    static kCGWindowOwnerName: CfRef;
    static kCGWindowName: CfRef;
    static kCGWindowLayer: CfRef;
    static kCGWindowAlpha: CfRef;
    static kCGWindowBounds: CfRef;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFArrayGetCount(array: CfRef) -> isize;
    fn CFArrayGetValueAtIndex(array: CfRef, index: isize) -> CfRef;
    fn CFDictionaryGetValue(dictionary: CfRef, key: CfRef) -> CfRef;
    fn CFNumberGetValue(number: CfRef, kind: isize, value: *mut c_void) -> bool;
    fn CFStringGetLength(string: CfRef) -> isize;
    fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
    fn CFStringGetCString(string: CfRef, buffer: *mut c_char, size: isize, encoding: u32) -> bool;
    fn CFGetTypeID(value: CfRef) -> usize;
    fn CFNumberGetTypeID() -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFRelease(value: CfRef);
}

const ON_SCREEN_ONLY: u32 = 1 << 0;
const EXCLUDE_DESKTOP: u32 = 1 << 4;
const SINT64: isize = 4;
const FLOAT64: isize = 13;
const UTF8: u32 = 0x0800_0100;

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

/// SAFETY: `dictionary` must be a live CFDictionary and `key` a CFString.
unsafe fn cf_number(dictionary: CfRef, key: CfRef, kind: isize) -> Option<f64> {
    // SAFETY: per the function contract; the value is borrowed and type-checked.
    unsafe {
        let value = CFDictionaryGetValue(dictionary, key);
        if value.is_null() || CFGetTypeID(value) != CFNumberGetTypeID() {
            return None;
        }
        if kind == FLOAT64 {
            let mut out = 0f64;
            CFNumberGetValue(value, FLOAT64, (&mut out as *mut f64).cast()).then_some(out)
        } else {
            let mut out = 0i64;
            CFNumberGetValue(value, SINT64, (&mut out as *mut i64).cast()).then_some(out as f64)
        }
    }
}

/// SAFETY: `dictionary` must be a live CFDictionary and `key` a CFString.
unsafe fn cf_string(dictionary: CfRef, key: CfRef) -> Option<String> {
    // SAFETY: per the function contract; the buffer is sized for the encoding.
    unsafe {
        let value = CFDictionaryGetValue(dictionary, key);
        if value.is_null() || CFGetTypeID(value) != CFStringGetTypeID() {
            return None;
        }
        let size = CFStringGetMaximumSizeForEncoding(CFStringGetLength(value), UTF8) + 1;
        let mut buffer = vec![0 as c_char; usize::try_from(size).ok()?];
        if !CFStringGetCString(value, buffer.as_mut_ptr(), size, UTF8) {
            return None;
        }
        Some(
            std::ffi::CStr::from_ptr(buffer.as_ptr())
                .to_string_lossy()
                .into_owned(),
        )
    }
}

/// On-screen windows in front-to-back order.
pub fn windows() -> Result<Vec<Window>> {
    // SAFETY: Copy returns an owned CFArray (or null), released below; every
    // element is a borrowed CFDictionary valid while the array lives.
    unsafe {
        let array = CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP, 0);
        if array.is_null() {
            return Err(Error::new("windows", "The window list is unavailable"));
        }
        let mut windows = Vec::new();
        for index in 0..CFArrayGetCount(array) {
            let entry = CFArrayGetValueAtIndex(array, index);
            let mut rect = Rect::default();
            let bounds = CFDictionaryGetValue(entry, kCGWindowBounds);
            if bounds.is_null() || !CGRectMakeWithDictionaryRepresentation(bounds, &mut rect) {
                continue;
            }
            let field = |key| cf_number(entry, key, SINT64).map(|value| value as i64);
            let (Some(number), Some(pid), Some(layer)) = (
                field(kCGWindowNumber),
                field(kCGWindowOwnerPID),
                field(kCGWindowLayer),
            ) else {
                continue;
            };
            windows.push(Window {
                number,
                pid,
                owner: cf_string(entry, kCGWindowOwnerName),
                title: cf_string(entry, kCGWindowName),
                layer,
                alpha: cf_number(entry, kCGWindowAlpha, FLOAT64).unwrap_or(1.0),
                bounds: Bounds {
                    x: rect.origin.x,
                    y: rect.origin.y,
                    width: rect.size.width,
                    height: rect.size.height,
                },
            });
        }
        CFRelease(array);
        Ok(windows)
    }
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

    #[test]
    fn the_current_session_lists_windows() {
        // Any logged-in test session has at least the menu bar or Dock.
        let windows = windows().unwrap();
        assert!(windows.iter().all(|w| w.bounds.width >= 0.0 && w.pid > 0));
    }
}
