//! Accessibility elements of one application, read *inside* the helper login.
//! Read-only and bounded: a global messaging timeout and an overall deadline
//! keep an unresponsive app from hanging the probe; depth, count, children
//! fetched and string lengths are capped before anything is copied. A scan
//! that hit a limit or a communication failure reports itself incomplete.
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::{
    ffi::{CStr, c_char, c_void},
    time::{Duration, Instant},
};

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
struct Range {
    location: isize,
    length: isize,
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> CfRef;
    fn AXUIElementCreateSystemWide() -> CfRef;
    fn AXUIElementCopyAttributeValue(element: CfRef, attribute: CfRef, value: *mut CfRef) -> i32;
    fn AXUIElementCopyAttributeValues(
        element: CfRef,
        attribute: CfRef,
        index: isize,
        maximum: isize,
        values: *mut CfRef,
    ) -> i32;
    fn AXUIElementSetMessagingTimeout(element: CfRef, seconds: f32) -> i32;
    fn AXUIElementGetAttributeValueCount(
        element: CfRef,
        attribute: CfRef,
        count: *mut isize,
    ) -> i32;
    fn AXValueGetValue(value: CfRef, kind: u32, out: *mut c_void) -> bool;
    fn AXValueGetTypeID() -> usize;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringCreateWithCString(allocator: CfRef, text: *const c_char, encoding: u32) -> CfRef;
    fn CFStringCreateWithSubstring(allocator: CfRef, string: CfRef, range: Range) -> CfRef;
    fn CFStringGetLength(string: CfRef) -> isize;
    fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
    fn CFStringGetCString(string: CfRef, buffer: *mut c_char, size: isize, encoding: u32) -> bool;
    fn CFArrayGetCount(array: CfRef) -> isize;
    fn CFArrayGetValueAtIndex(array: CfRef, index: isize) -> CfRef;
    fn CFBooleanGetValue(boolean: CfRef) -> bool;
    fn CFNumberGetValue(number: CfRef, kind: isize, value: *mut c_void) -> bool;
    fn CFGetTypeID(value: CfRef) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFArrayGetTypeID() -> usize;
    fn CFBooleanGetTypeID() -> usize;
    fn CFNumberGetTypeID() -> usize;
    fn CFRetain(value: CfRef) -> CfRef;
    fn CFRelease(value: CfRef);
}

const UTF8: u32 = 0x0800_0100;
const POINT: u32 = 1;
const SIZE: u32 = 2;
const FLOAT64: isize = 13;
/// Longest text reported for any one attribute, in UTF-16 units.
const TEXT_LIMIT: isize = 200;
/// AX errors meaning the element could not be read at all: the request could
/// not complete (timeout or unresponsive app), the element vanished, or the
/// API is disabled. Any of these makes the whole scan incomplete.
const INCOMPLETE: [i32; 3] = [-25204, -25202, -25211];
/// AX errors meaning the element simply lacks the attribute: no value,
/// unsupported, not implemented.
const ABSENT: [i32; 3] = [-25212, -25205, -25208];
/// Attributes a selector matches on; an element whose selector attribute
/// fails to read (other than being absent) may be an unseen match.
const SELECTORS: [&CStr; 4] = [c"AXRole", c"AXTitle", c"AXDescription", c"AXIdentifier"];

/// An owned CoreFoundation reference, released once. Never wraps null.
struct Owned(CfRef);
impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: only non-null references from Create/Copy/Retain are wrapped.
        unsafe { CFRelease(self.0) };
    }
}
impl Owned {
    fn new(value: CfRef) -> Option<Self> {
        if value.is_null() {
            None
        } else {
            Some(Self(value))
        }
    }
}

fn cf_string(text: &CStr) -> Option<Owned> {
    // SAFETY: NUL-terminated static text and the default allocator.
    Owned::new(unsafe { CFStringCreateWithCString(std::ptr::null(), text.as_ptr(), UTF8) })
}

/// The first `TEXT_LIMIT` UTF-16 units of a CFString, converted without
/// copying the rest. SAFETY: `value` must be a live CFString.
unsafe fn bounded_string(value: CfRef) -> Option<String> {
    // SAFETY: per contract; the substring is owned and the buffer is sized
    // for its encoding.
    unsafe {
        let length = CFStringGetLength(value).min(TEXT_LIMIT);
        let range = Range {
            location: 0,
            length,
        };
        let part = Owned::new(CFStringCreateWithSubstring(std::ptr::null(), value, range))?;
        let size = CFStringGetMaximumSizeForEncoding(length, UTF8) + 1;
        let mut buffer = vec![0 as c_char; usize::try_from(size).ok()?];
        CFStringGetCString(part.0, buffer.as_mut_ptr(), size, UTF8).then(|| {
            CStr::from_ptr(buffer.as_ptr())
                .to_string_lossy()
                .into_owned()
        })
    }
}

/// One traversal: its deadline, whether anything made it incomplete, the
/// selector attributes the current element failed to read, and how often
/// each attribute returned each AX error.
struct Scan {
    deadline: Instant,
    incomplete: bool,
    unreadable: Vec<String>,
    errors: std::collections::BTreeMap<String, usize>,
}

impl Scan {
    /// Whether another AX request may start; past the deadline the scan is
    /// incomplete and nothing more is asked.
    fn open(&mut self) -> bool {
        if Instant::now() >= self.deadline {
            self.incomplete = true;
        }
        !self.incomplete || Instant::now() < self.deadline
    }
    fn failed(&mut self, status: i32, attribute: &CStr) {
        let key = format!("{}:{status}", attribute.to_string_lossy());
        *self.errors.entry(key).or_default() += 1;
        if INCOMPLETE.contains(&status) {
            self.incomplete = true;
        } else if !ABSENT.contains(&status) && SELECTORS.contains(&attribute) {
            self.unreadable
                .push(attribute.to_string_lossy().into_owned());
        }
    }
    /// Copy one attribute value.
    fn attribute(&mut self, element: CfRef, attribute: &CStr) -> Option<Owned> {
        if !self.open() {
            return None;
        }
        let name = cf_string(attribute)?;
        let mut value = std::ptr::null();
        // SAFETY: live element and attribute; Copy transfers ownership on success.
        let status = unsafe { AXUIElementCopyAttributeValue(element, name.0, &mut value) };
        if status == 0 {
            return Owned::new(value);
        }
        self.failed(status, attribute);
        None
    }
    fn text(&mut self, element: CfRef, name: &CStr) -> Option<String> {
        let value = self.attribute(element, name)?;
        // SAFETY: type-checked before reading.
        unsafe { (CFGetTypeID(value.0) == CFStringGetTypeID()).then(|| bounded_string(value.0)) }
            .flatten()
    }
    fn flag(&mut self, element: CfRef, name: &CStr) -> Option<bool> {
        let value = self.attribute(element, name)?;
        // SAFETY: type-checked before reading.
        unsafe {
            (CFGetTypeID(value.0) == CFBooleanGetTypeID()).then(|| CFBooleanGetValue(value.0))
        }
    }
    /// The element's value as text: strings bounded, numbers and booleans rendered.
    fn value(&mut self, element: CfRef) -> Option<String> {
        let value = self.attribute(element, c"AXValue")?;
        // SAFETY: each branch checks the CF type before reading.
        unsafe {
            let kind = CFGetTypeID(value.0);
            if kind == CFStringGetTypeID() {
                bounded_string(value.0)
            } else if kind == CFNumberGetTypeID() {
                let mut number = 0f64;
                CFNumberGetValue(value.0, FLOAT64, (&mut number as *mut f64).cast())
                    .then(|| number.to_string())
            } else if kind == CFBooleanGetTypeID() {
                Some(CFBooleanGetValue(value.0).to_string())
            } else {
                None
            }
        }
    }
    fn frame(&mut self, element: CfRef) -> Option<Frame> {
        let position = self.attribute(element, c"AXPosition")?;
        let size = self.attribute(element, c"AXSize")?;
        let (mut point, mut extent) = (Point::default(), Size::default());
        // SAFETY: both are type-checked AXValues read into matching structs.
        unsafe {
            (CFGetTypeID(position.0) == AXValueGetTypeID()
                && CFGetTypeID(size.0) == AXValueGetTypeID()
                && AXValueGetValue(position.0, POINT, (&mut point as *mut Point).cast())
                && AXValueGetValue(size.0, SIZE, (&mut extent as *mut Size).cast()))
            .then_some(Frame {
                x: point.x,
                y: point.y,
                width: extent.width,
                height: extent.height,
            })
        }
    }
    /// At most `maximum` element references from an array attribute, each
    /// retained. The value count is read first: zero is a leaf, and a count
    /// beyond what was fetched (or any failure to enumerate a non-empty
    /// array) leaves the scan incomplete instead of silently dropping values.
    fn children(&mut self, element: CfRef, attribute: &CStr, maximum: usize) -> Vec<Owned> {
        if !self.open() {
            return Vec::new();
        }
        let Some(name) = cf_string(attribute) else {
            return Vec::new();
        };
        let mut count = 0isize;
        // SAFETY: live element and attribute with a valid output pointer.
        let status = unsafe { AXUIElementGetAttributeValueCount(element, name.0, &mut count) };
        if status != 0 {
            self.failed(status, attribute);
            if !ABSENT.contains(&status) {
                self.incomplete = true;
            }
            return Vec::new();
        }
        if count <= 0 {
            return Vec::new();
        }
        let wanted = count.min(isize::try_from(maximum).unwrap_or(isize::MAX));
        if wanted < count {
            self.incomplete = true;
        }
        let mut array = std::ptr::null();
        // SAFETY: live element and attribute; the range copy returns an owned
        // array whose elements are retained before it is released.
        unsafe {
            let status = AXUIElementCopyAttributeValues(element, name.0, 0, wanted, &mut array);
            let Some(array) = (status == 0).then(|| Owned::new(array)).flatten() else {
                self.failed(status, attribute);
                self.incomplete = true;
                return Vec::new();
            };
            if CFGetTypeID(array.0) != CFArrayGetTypeID() {
                self.incomplete = true;
                return Vec::new();
            }
            (0..CFArrayGetCount(array.0))
                .filter_map(|index| Owned::new(CFRetain(CFArrayGetValueAtIndex(array.0, index))))
                .collect()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl Frame {
    pub fn center(&self) -> (f64, f64) {
        (
            (self.x + self.width / 2.0).round(),
            (self.y + self.height / 2.0).round(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Element {
    /// Position in depth-first order, stable within one snapshot.
    pub index: usize,
    pub depth: usize,
    /// Frame of the top-level window this element belongs to.
    pub window: Option<Frame>,
    pub role: Option<String>,
    pub subrole: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub value: Option<String>,
    pub identifier: Option<String>,
    pub enabled: Option<bool>,
    pub focused: Option<bool>,
    pub frame: Option<Frame>,
    /// Selector attributes (role, title, description, identifier) that could
    /// not be read, so this element might match a selector unseen.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unreadable: Vec<String>,
}

/// One traversal's result.
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub elements: Vec<Element>,
    /// A limit, the deadline or an unreadable element cut the scan short, so
    /// absence from it proves nothing.
    pub incomplete: bool,
    /// "attribute:AX error code" -> count, for diagnosing incomplete scans.
    pub errors: std::collections::BTreeMap<String, usize>,
}

/// Bounds on one traversal.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub depth: usize,
    pub count: usize,
    pub time: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            depth: 14,
            count: 1500,
            time: Duration::from_secs(4),
        }
    }
}

/// Depth-first elements of `pid`'s windows (the menu bar is excluded).
pub fn elements(pid: i32, limits: Limits) -> Result<Snapshot> {
    // SAFETY: the system-wide element sets this process's global messaging
    // timeout, which covers every element read below.
    if let Some(system) = Owned::new(unsafe { AXUIElementCreateSystemWide() }) {
        unsafe { AXUIElementSetMessagingTimeout(system.0, 1.0) };
    }
    // SAFETY: Create returns an owned element (or null) for any PID.
    let app = Owned::new(unsafe { AXUIElementCreateApplication(pid) })
        .ok_or_else(|| Error::new("elements", "No accessibility element for that process"))?;
    let mut scan = Scan {
        deadline: Instant::now() + limits.time,
        incomplete: false,
        unreadable: Vec::new(),
        errors: Default::default(),
    };
    let windows = scan.children(app.0, c"AXWindows", limits.count);
    // An app that answers nothing at all, not even its role, is unreachable
    // (no Accessibility trust, or not a GUI process); report it, not "empty".
    if windows.is_empty() && scan.text(app.0, c"AXRole").is_none() {
        return Err(Error::new(
            "permission",
            "The app's accessibility tree is unavailable; check Accessibility for the helper",
        ));
    }
    let mut found = Vec::new();
    let mut stack: Vec<(Owned, usize, Option<Frame>)> = Vec::new();
    for window in windows.into_iter().rev() {
        let frame = scan.frame(window.0);
        stack.push((window, 0, frame));
    }
    while let Some((element, depth, window)) = stack.pop() {
        if found.len() >= limits.count || !scan.open() {
            scan.incomplete = true;
            break;
        }
        let item = element.0;
        scan.unreadable.clear();
        let mut element = Element {
            index: found.len(),
            depth,
            window: window.clone(),
            role: scan.text(item, c"AXRole"),
            subrole: scan.text(item, c"AXSubrole"),
            title: scan.text(item, c"AXTitle"),
            description: scan.text(item, c"AXDescription"),
            value: scan.value(item),
            identifier: scan.text(item, c"AXIdentifier"),
            enabled: scan.flag(item, c"AXEnabled"),
            focused: scan.flag(item, c"AXFocused"),
            frame: scan.frame(item),
            unreadable: Vec::new(),
        };
        element.unreadable = std::mem::take(&mut scan.unreadable);
        found.push(element);
        if depth + 1 >= limits.depth {
            scan.incomplete |= !scan.children(item, c"AXChildren", 1).is_empty();
            continue;
        }
        let budget = limits.count.saturating_sub(found.len() + stack.len());
        let children = scan.children(item, c"AXChildren", budget);
        for child in children.into_iter().rev() {
            stack.push((child, depth + 1, window.clone()));
        }
    }
    // A request that started before the deadline may have ended after it.
    scan.open();
    Ok(Snapshot {
        elements: found,
        incomplete: scan.incomplete,
        errors: scan.errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_report_whole_point_centres() {
        let frame = Frame {
            x: 10.0,
            y: 20.0,
            width: 31.0,
            height: 10.0,
        };
        assert_eq!(frame.center(), (26.0, 25.0));
    }

    #[test]
    fn null_references_are_never_owned() {
        assert!(Owned::new(std::ptr::null()).is_none());
    }

    #[test]
    fn strings_are_bounded_before_conversion() {
        let long = std::ffi::CString::new("x".repeat(5000)).unwrap();
        // SAFETY: owned CFString from a NUL-terminated buffer.
        let string =
            Owned::new(unsafe { CFStringCreateWithCString(std::ptr::null(), long.as_ptr(), UTF8) })
                .unwrap();
        let text = unsafe { bounded_string(string.0) }.unwrap();
        assert_eq!(text.len(), TEXT_LIMIT as usize);
    }

    #[test]
    fn an_absent_process_yields_no_elements() {
        // PID 0 is the kernel; it has no accessible windows. The call must
        // fail closed or return nothing, never crash.
        if let Ok(snapshot) = elements(0, Limits::default()) {
            assert!(snapshot.elements.is_empty());
        }
    }
}
