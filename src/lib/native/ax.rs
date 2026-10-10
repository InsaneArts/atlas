//! Accessibility elements of one application, read *inside* the helper login.
//! Read-only and bounded: a global messaging timeout and an overall deadline
//! keep an unresponsive app from hanging the probe; depth, count, children
//! fetched and string lengths are capped before anything is copied. A scan
//! that hit a limit or a communication failure reports itself incomplete.
use crate::error::{Error, Result};
use objc2_application_services::{AXError, AXUIElement, AXValue, AXValueType};
use objc2_core_foundation::{
    CFArray, CFBoolean, CFIndex, CFNumber, CFRange, CFRetained, CFString, CFType, CGPoint, CGSize,
};
use serde::{Deserialize, Serialize};
use std::{
    ptr::NonNull,
    time::{Duration, Instant},
};

/// Longest text reported for any one attribute, in UTF-16 units.
const TEXT_LIMIT: CFIndex = 200;
/// AX errors meaning the element could not be read at all: the request could
/// not complete (timeout or unresponsive app), the element vanished, or the
/// API is disabled. Any of these makes the whole scan incomplete.
const INCOMPLETE: [i32; 3] = [-25204, -25202, -25211];
/// AX errors meaning the element simply lacks the attribute: no value,
/// unsupported, not implemented.
const ABSENT: [i32; 3] = [-25212, -25205, -25208];
/// Attributes a selector matches on; an element whose selector attribute
/// fails to read (other than being absent) may be an unseen match.
const SELECTORS: [&str; 4] = ["AXRole", "AXTitle", "AXDescription", "AXIdentifier"];

/// At most the first `TEXT_LIMIT` UTF-16 units of a CFString, converted
/// without copying the rest and cut only between composed characters, so a
/// surrogate pair, emoji sequence or combining mark is never split. Returns
/// the text and whether it was cut.
fn bounded_string(value: &CFString) -> Option<(String, bool)> {
    let full = value.length();
    let truncated = full > TEXT_LIMIT;
    let length = if truncated {
        // SAFETY: the index is inside the string (it is longer than it).
        unsafe { value.range_of_composed_characters_at_index(TEXT_LIMIT) }.location
    } else {
        full
    };
    let range = CFRange {
        location: 0,
        length,
    };
    // SAFETY: a live string, the default allocator and a range within it.
    let part = unsafe { CFString::with_substring(None, Some(value), range) }?;
    Some((part.to_string(), truncated))
}

/// Take over the reference a successful Copy wrote to `slot`, if any.
///
/// # Safety
/// `slot` must hold null or a reference the caller owns (from a Copy).
unsafe fn adopt<T: objc2_core_foundation::Type>(slot: *const T) -> Option<CFRetained<T>> {
    // SAFETY: per contract; ownership of the one reference moves here.
    NonNull::new(slot.cast_mut()).map(|value| unsafe { CFRetained::from_raw(value) })
}

/// One traversal: its deadline, whether anything made it incomplete, the
/// selector attributes the current element failed to read or reported only
/// in part, and how often each attribute returned each AX error.
struct Scan {
    deadline: Instant,
    incomplete: bool,
    unreadable: Vec<String>,
    truncated: Vec<String>,
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
    fn failed(&mut self, status: AXError, attribute: &str) {
        let key = format!("{attribute}:{}", status.0);
        *self.errors.entry(key).or_default() += 1;
        if INCOMPLETE.contains(&status.0) {
            self.incomplete = true;
        } else if !ABSENT.contains(&status.0) && SELECTORS.contains(&attribute) {
            self.unreadable.push(attribute.to_owned());
        }
    }
    /// Copy one attribute value.
    fn attribute(
        &mut self,
        element: &AXUIElement,
        attribute: &'static str,
    ) -> Option<CFRetained<CFType>> {
        if !self.open() {
            return None;
        }
        let name = CFString::from_static_str(attribute);
        let mut value: *const CFType = std::ptr::null();
        // SAFETY: a valid output slot; on success the Copy hands over one
        // owned reference, adopted below.
        let status = unsafe { element.copy_attribute_value(&name, NonNull::from(&mut value)) };
        if status == AXError::Success {
            // SAFETY: a successful Copy wrote an owned reference (or null).
            return unsafe { adopt(value) };
        }
        self.failed(status, attribute);
        None
    }
    fn text(&mut self, element: &AXUIElement, name: &'static str) -> Option<String> {
        let value = self.attribute(element, name)?;
        let text = value.downcast_ref::<CFString>().and_then(bounded_string);
        self.bounded(text, name)
    }
    /// Keep bounded text, noting the attribute when it was cut.
    fn bounded(&mut self, text: Option<(String, bool)>, name: &str) -> Option<String> {
        let (text, truncated) = text?;
        if truncated {
            self.truncated.push(name.to_owned());
        }
        Some(text)
    }
    fn flag(&mut self, element: &AXUIElement, name: &'static str) -> Option<bool> {
        let value = self.attribute(element, name)?;
        value.downcast_ref::<CFBoolean>().map(CFBoolean::as_bool)
    }
    /// The element's value as text: strings bounded, numbers and booleans rendered.
    fn value(&mut self, element: &AXUIElement) -> Option<String> {
        let value = self.attribute(element, "AXValue")?;
        if let Some(text) = value.downcast_ref::<CFString>() {
            let text = bounded_string(text);
            self.bounded(text, "AXValue")
        } else if let Some(number) = value.downcast_ref::<CFNumber>() {
            number.as_f64().map(|number| number.to_string())
        } else {
            value
                .downcast_ref::<CFBoolean>()
                .map(|flag| flag.as_bool().to_string())
        }
    }
    fn frame(&mut self, element: &AXUIElement) -> Option<Frame> {
        let position = self.attribute(element, "AXPosition")?;
        let size = self.attribute(element, "AXSize")?;
        let (position, size) = (
            position.downcast_ref::<AXValue>()?,
            size.downcast_ref::<AXValue>()?,
        );
        let (mut point, mut extent) = (CGPoint::ZERO, CGSize::ZERO);
        // SAFETY: each AXValue is read into the struct its type names.
        let read = unsafe {
            position.value(AXValueType::CGPoint, NonNull::from(&mut point).cast())
                && size.value(AXValueType::CGSize, NonNull::from(&mut extent).cast())
        };
        read.then_some(Frame {
            x: point.x,
            y: point.y,
            width: extent.width,
            height: extent.height,
        })
    }
    /// At most `maximum` elements from an array attribute. The value count
    /// is read first: zero is a leaf, and a count beyond what was fetched (or
    /// any failure to enumerate a non-empty array) leaves the scan incomplete
    /// instead of silently dropping values.
    fn children(
        &mut self,
        element: &AXUIElement,
        attribute: &'static str,
        maximum: usize,
    ) -> Vec<CFRetained<AXUIElement>> {
        if !self.open() {
            return Vec::new();
        }
        let name = CFString::from_static_str(attribute);
        let mut count: CFIndex = 0;
        // SAFETY: a valid output slot for the count.
        let status = unsafe { element.attribute_value_count(&name, NonNull::from(&mut count)) };
        if status != AXError::Success {
            self.failed(status, attribute);
            if !ABSENT.contains(&status.0) {
                self.incomplete = true;
            }
            return Vec::new();
        }
        if count <= 0 {
            return Vec::new();
        }
        let wanted = count.min(CFIndex::try_from(maximum).unwrap_or(CFIndex::MAX));
        if wanted < count {
            self.incomplete = true;
        }
        let mut array: *const CFArray = std::ptr::null();
        // SAFETY: a valid output slot; on success the Copy hands over one
        // owned array, adopted below.
        let status =
            unsafe { element.copy_attribute_values(&name, 0, wanted, NonNull::from(&mut array)) };
        // SAFETY: a successful Copy wrote an owned array (or null).
        let Some(array) = (status == AXError::Success)
            .then(|| unsafe { adopt(array) })
            .flatten()
        else {
            self.failed(status, attribute);
            self.incomplete = true;
            return Vec::new();
        };
        // SAFETY: every element of a CFArray is a CF object; each is checked
        // to be an AXUIElement before use.
        let values = unsafe { CFRetained::cast_unchecked::<CFArray<CFType>>(array) };
        values
            .iter()
            .filter_map(|value| value.downcast::<AXUIElement>().ok())
            .collect()
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
    /// Text attributes reported only in part (their first 200 UTF-16 units,
    /// cut between characters); the element holds more than is shown.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub truncated: Vec<String>,
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
    // SAFETY: plain constructors; the system-wide element sets this
    // process's global messaging timeout, which covers every read below.
    let app = unsafe {
        AXUIElement::new_system_wide().set_messaging_timeout(1.0);
        AXUIElement::new_application(pid)
    };
    let mut scan = Scan {
        deadline: Instant::now() + limits.time,
        incomplete: false,
        unreadable: Vec::new(),
        truncated: Vec::new(),
        errors: Default::default(),
    };
    let windows = scan.children(&app, "AXWindows", limits.count);
    // An app that answers nothing at all, not even its role, is unreachable
    // (no Accessibility trust, or not a GUI process); report it, not "empty".
    if windows.is_empty() && scan.text(&app, "AXRole").is_none() {
        return Err(Error::new(
            "permission",
            "The app's accessibility tree is unavailable; check Accessibility for the helper",
        ));
    }
    let mut found = Vec::new();
    let mut stack: Vec<(CFRetained<AXUIElement>, usize, Option<Frame>)> = Vec::new();
    for window in windows.into_iter().rev() {
        let frame = scan.frame(&window);
        stack.push((window, 0, frame));
    }
    while let Some((element, depth, window)) = stack.pop() {
        if found.len() >= limits.count || !scan.open() {
            scan.incomplete = true;
            break;
        }
        let item = &*element;
        scan.unreadable.clear();
        scan.truncated.clear();
        let mut element = Element {
            index: found.len(),
            depth,
            window: window.clone(),
            role: scan.text(item, "AXRole"),
            subrole: scan.text(item, "AXSubrole"),
            title: scan.text(item, "AXTitle"),
            description: scan.text(item, "AXDescription"),
            value: scan.value(item),
            identifier: scan.text(item, "AXIdentifier"),
            enabled: scan.flag(item, "AXEnabled"),
            focused: scan.flag(item, "AXFocused"),
            frame: scan.frame(item),
            unreadable: Vec::new(),
            truncated: Vec::new(),
        };
        element.unreadable = std::mem::take(&mut scan.unreadable);
        element.truncated = std::mem::take(&mut scan.truncated);
        found.push(element);
        if depth + 1 >= limits.depth {
            scan.incomplete |= !scan.children(item, "AXChildren", 1).is_empty();
            continue;
        }
        let budget = limits.count.saturating_sub(found.len() + stack.len());
        let children = scan.children(item, "AXChildren", budget);
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
        // SAFETY: null is never adopted.
        assert!(unsafe { adopt::<CFType>(std::ptr::null()) }.is_none());
    }

    /// Bound `text` as a CFString would be.
    fn bound(text: &str) -> (String, bool) {
        bounded_string(&CFString::from_str(text)).unwrap()
    }

    #[test]
    fn strings_are_bounded_before_conversion() {
        let (text, truncated) = bound(&"x".repeat(5000));
        assert_eq!((text.len(), truncated), (TEXT_LIMIT as usize, true));
        assert_eq!(bound("short"), ("short".into(), false));
        let exact = "y".repeat(TEXT_LIMIT as usize);
        assert_eq!(
            bound(&exact),
            (exact.clone(), false),
            "at the limit is whole"
        );
    }

    #[test]
    fn strings_are_cut_between_characters() {
        // A surrogate pair straddles unit 200: 199 units, then 2 for the emoji.
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
        let (text, truncated) = bound(&format!("{}{family}tail", "a".repeat(199)));
        assert!(truncated);
        assert_eq!(text, "a".repeat(199), "the whole emoji sequence is dropped");
        // A combining mark at unit 200 stays with its base letter.
        let (text, _) = bound(&format!("{}e\u{301}tail", "a".repeat(199)));
        assert_eq!(text, "a".repeat(199));
        let (text, _) = bound(&format!("{}e\u{301}tail", "a".repeat(198)));
        assert_eq!(text, format!("{}e\u{301}", "a".repeat(198)));
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
