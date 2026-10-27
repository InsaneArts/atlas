//! Accessibility elements of one application, read *inside* the helper login:
//! its windows and the menus it has open. Read-only and bounded: a global
//! messaging timeout and an overall deadline keep an unresponsive app from
//! hanging the probe; depth, count, children fetched and string lengths are
//! capped before anything is copied. A scan that hit a limit or a
//! communication failure reports itself incomplete.
// FFI boundary: Accessibility reads return values through out-pointers.
#![allow(unsafe_code)]
use crate::{
    error::{Error, Result},
    native::windows::{self, MENU_LAYER},
};
use objc2_application_services::{AXError, AXUIElement, AXValue, AXValueType};
use objc2_core_foundation::{
    CFArray, CFBoolean, CFIndex, CFNumber, CFRange, CFRetained, CFString, CFType, CGPoint, CGSize,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    ptr::NonNull,
    time::{Duration, Instant},
};

// The bindings' constructors panic on a null element; these report it.
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C-unwind" {
    fn AXUIElementCreateApplication(pid: libc::pid_t) -> Option<NonNull<AXUIElement>>;
    fn AXUIElementCreateSystemWide() -> Option<NonNull<AXUIElement>>;
}

/// Longest text reported for any one attribute, in UTF-16 units.
const TEXT_LIMIT: CFIndex = 200;

/// Most menu bar titles read for an app.
const MENU_BAR_ITEMS: usize = 64;

/// AX errors meaning the element could not be read at all: the request could
/// not complete (timeout or unresponsive app), the element vanished, or the
/// API is disabled. Any of these makes the whole scan incomplete.
const INCOMPLETE: [AXError; 3] = [
    AXError::CannotComplete,
    AXError::InvalidUIElement,
    AXError::APIDisabled,
];

/// AX errors meaning the element simply lacks the attribute: no value,
/// unsupported, not implemented.
const ABSENT: [AXError; 3] = [
    AXError::NoValue,
    AXError::AttributeUnsupported,
    AXError::NotImplemented,
];

/// Text attributes an app's own getter may fail for (an AppKit text view
/// answers Failure for its description every time); the text is then as
/// good as absent. A tree read that fails so still leaves the scan
/// incomplete.
const TEXT_SELECTORS: [&str; 4] = ["AXTitle", "AXDescription", "AXValue", "AXIdentifier"];

/// Attributes a selector matches on; an element whose selector attribute
/// fails to read (other than being absent) may be an unseen match.
const SELECTORS: [&str; 5] = [
    "AXRole",
    "AXTitle",
    "AXDescription",
    "AXIdentifier",
    "AXValue",
];

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
    /// The last AX error any read returned.
    last_error: Option<AXError>,
}

impl Scan {
    fn new(deadline: Instant) -> Self {
        Self {
            deadline,
            incomplete: false,
            unreadable: Vec::new(),
            truncated: Vec::new(),
            errors: Default::default(),
            last_error: None,
        }
    }

    /// Whether another AX request may start; past the deadline the scan is
    /// incomplete and nothing more is asked.
    fn open(&mut self) -> bool {
        let open = Instant::now() < self.deadline;
        self.incomplete |= !open;
        open
    }

    fn failed(&mut self, status: AXError, attribute: &str) {
        self.last_error = Some(status);
        let key = format!("{attribute}:{}", status.0);
        *self.errors.entry(key).or_default() += 1;

        let getter_failed = status == AXError::Failure && TEXT_SELECTORS.contains(&attribute);

        if INCOMPLETE.contains(&status) {
            self.incomplete = true;
        } else if !ABSENT.contains(&status) && !getter_failed && SELECTORS.contains(&attribute) {
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
            if !ABSENT.contains(&status) {
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

    /// What the app draws in `bounds`, one of its windows at the menu layer:
    /// the element above a point inside it whose frame is the window's (an
    /// open menu, a popover, a combo box's list, a panel raised to the menu
    /// layer). A window whose points all reach a window in `read` instead
    /// shows nothing of its own (a tooltip, a click-through overlay, a menu
    /// fading out); the elements hit there must turn up in that window's
    /// scan. Menus are found this way because a context menu hangs below the
    /// view it opened from, and a menu bar or status item menu below its
    /// item, never among the app's windows.
    fn drawn_at(&mut self, app: &AXUIElement, bounds: &Frame, read: &[Frame]) -> Option<Drawn> {
        let points = [0.5, 0.25, 0.75];
        let mut hits = Vec::new();

        // A point can fall on a separator; a menu has items around it.
        for fraction in points {
            if !self.open() {
                return None;
            }

            let (x, y) = (
                bounds.x + bounds.width / 2.0,
                bounds.y + bounds.height * fraction,
            );
            let mut hit: *const AXUIElement = std::ptr::null();

            // SAFETY: a valid output slot; on success the Copy hands over one
            // owned element, adopted below.
            let status = unsafe {
                app.copy_element_at_position(x as f32, y as f32, NonNull::from(&mut hit))
            };

            if status != AXError::Success {
                self.failed(status, "AXElementAtPosition");
                continue;
            }

            // SAFETY: a successful Copy wrote an owned reference (or null).
            let Some(leaf) = (unsafe { adopt(hit) }) else {
                continue;
            };

            let mut element = Some(leaf.clone());

            // An item sits a few levels below its menu or window.
            for _ in 0..12 {
                let Some(current) = element else {
                    break;
                };

                let frame = self.frame(&current);
                let menu = self.text(&current, "AXRole").as_deref() == Some("AXMenu");
                let parent = self
                    .attribute(&current, "AXParent")
                    .and_then(|parent| parent.downcast::<AXUIElement>().ok());
                let top = parent.as_ref().is_some_and(|parent| {
                    self.text(parent, "AXRole").as_deref() == Some("AXApplication")
                });
                match judge(menu, frame.as_ref(), top, bounds, read) {
                    Step::Found { menu: true } => return Some(Drawn::Menu(current)),
                    Step::Found { menu: false } => return Some(Drawn::Window(current)),
                    Step::Through => {
                        hits.push(leaf);
                        break;
                    }
                    Step::Stop => break,
                    Step::Up => element = parent,
                }
            }
        }

        (hits.len() == points.len()).then_some(Drawn::Through(hits))
    }
}

/// What an app draws at the menu layer (see [`Scan::drawn_at`]).
enum Drawn {
    Menu(CFRetained<AXUIElement>),
    Window(CFRetained<AXUIElement>),
    /// Nothing of its own: every point reached a window read already, at
    /// these elements, which that window's scan must contain.
    Through(Vec<CFRetained<AXUIElement>>),
}

/// One step up from the element hit inside a menu-layer window.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    /// This element has the window's frame: what it draws (a menu or not).
    Found { menu: bool },
    /// The top of another window already read: the point fell through.
    Through,
    /// The top of something else: the window cannot be placed.
    Stop,
    /// Keep climbing.
    Up,
}

/// Judge one element on the way up from a hit: whether it is `menu`, its
/// `frame`, and whether it is `top` (its parent is the application).
fn judge(menu: bool, frame: Option<&Frame>, top: bool, bounds: &Frame, read: &[Frame]) -> Step {
    if frame.is_some_and(|frame| frame.matches(bounds)) {
        return Step::Found { menu };
    }

    if !top {
        return Step::Up;
    }

    if frame.is_some_and(|frame| read.iter().any(|other| other.matches(frame))) {
        Step::Through
    } else {
        Step::Stop
    }
}

/// Whether a menu met inside a window or another menu is left out of the
/// scan: closed (no frame, or no size), or open and read as a menu of its
/// own. Any other menu, such as one a view draws itself, is read.
fn skip_nested_menu(frame: Option<&Frame>, open: &[Frame]) -> bool {
    frame.is_none_or(|frame| {
        frame.width <= 0.0 || frame.height <= 0.0 || open.iter().any(|menu| menu.matches(frame))
    })
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

    /// Whether two frames are the same within a point.
    pub fn matches(&self, other: &Frame) -> bool {
        let near = |a: f64, b: f64| (a - b).abs() <= 1.0;
        near(self.x, other.x)
            && near(self.y, other.y)
            && near(self.width, other.width)
            && near(self.height, other.height)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Element {
    /// Position in depth-first order, stable within one snapshot.
    pub index: usize,
    pub depth: usize,
    /// Frame of the top-level window (or open menu) this element belongs to.
    /// Like the attributes below, it is left out when the scan did not read it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<Frame>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subrole: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<Frame>,
    /// Selector attributes (role, title, description, value, identifier)
    /// that could not be read, so this element might match a selector unseen.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unreadable: Vec<String>,
    /// Text attributes reported only in part (their first 200 UTF-16 units,
    /// cut between characters); the element holds more than is shown.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub truncated: Vec<String>,
    /// In a menu the app has open (its menu bar, a status item or a
    /// context menu), where a click goes while the menu is open.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub menu: bool,
    /// A menu item's check mark (such as "✓"), when it shows one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark: Option<String>,
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

/// The window frame an element is judged visible against: a sheet's own
/// frame for the sheet and all it holds, since a sheet is a window of its own
/// in front of the one it hangs from; otherwise the frame it inherited.
fn sheet_window(
    role: Option<&str>,
    frame: Option<&Frame>,
    inherited: Option<Frame>,
) -> Option<Frame> {
    match (role, frame) {
        (Some("AXSheet"), Some(frame)) => Some(frame.clone()),
        _ => inherited,
    }
}

/// Why an app answered nothing, from the last AX error its reads returned,
/// whether this process may use Accessibility, and whether the process
/// exists. Only a missing permission is reported as one.
fn unreachable(error: Option<AXError>, trusted: bool, exists: bool) -> Error {
    if !trusted || error == Some(AXError::APIDisabled) {
        return Error::new(
            "permission",
            "The app's accessibility tree is unavailable; check Accessibility for the helper",
        );
    }

    if !exists || error == Some(AXError::InvalidUIElement) {
        return Error::new("elements", "No app is running with that process ID");
    }

    if error == Some(AXError::CannotComplete) {
        return Error::new(
            "elements",
            "The app is not responding to Accessibility (it may be busy, or not an app with \
             windows); try again",
        );
    }

    match error {
        Some(error) => Error::new(
            "elements",
            format!(
                "The app's accessibility tree could not be read (error {})",
                error.0
            ),
        ),
        None => Error::new("elements", "The app shows no accessibility tree"),
    }
}

/// Depth-first elements of `pid`'s windows, its menu bar titles when it is
/// the active app, then the menus it has open (a menu bar, status item or
/// context menu). Closed menus are left out: nothing in them is on screen.
pub fn elements(pid: i32, limits: Limits) -> Result<Snapshot> {
    // SAFETY: plain constructors returning an owned element or null; the
    // system-wide element sets this process's global messaging timeout,
    // which covers every read below.
    let app = unsafe {
        if let Some(system) = AXUIElementCreateSystemWide() {
            CFRetained::from_raw(system).set_messaging_timeout(1.0);
        }
        AXUIElementCreateApplication(pid).map(|app| CFRetained::from_raw(app))
    }
    .ok_or_else(|| Error::new("elements", "No accessibility element for that process"))?;

    let mut scan = Scan::new(Instant::now() + limits.time);
    let windows = scan.children(&app, "AXWindows", limits.count);
    let windows_error = scan.last_error.take();

    // An app that answers nothing at all, not even its role, is unreachable;
    // report why, not "empty".
    if windows.is_empty() && scan.text(&app, "AXRole").is_none() {
        let error = scan.last_error.or(windows_error);
        let exists = pid > 0
            && !matches!(
                nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None),
                Err(nix::errno::Errno::ESRCH)
            );

        return Err(unreachable(error, super::accessibility_trusted(), exists));
    }

    let mut roots: Vec<(CFRetained<AXUIElement>, Option<Frame>, bool)> = windows
        .into_iter()
        .map(|window| {
            let frame = scan.frame(&window);
            (window, frame, false)
        })
        .collect();
    // Elements hit through a click-through window, which the scan below
    // must reach; any it does not might hold an unseen match.
    let mut unvisited: Vec<CFRetained<AXUIElement>> = Vec::new();

    // The menu bar shows the active app's titles, which are on screen and
    // take clicks; the closed menus under them are left out below. A bar,
    // or the flag that says whether it is this app's, that fails to read
    // might hide an unseen match.
    scan.last_error = None;
    let frontmost = scan.flag(&app, "AXFrontmost");
    if frontmost.is_none() {
        scan.incomplete |= scan.last_error.is_some_and(|code| !ABSENT.contains(&code));
    }

    if frontmost == Some(true) {
        scan.last_error = None;
        let bar = scan
            .attribute(&app, "AXMenuBar")
            .and_then(|bar| bar.downcast::<AXUIElement>().ok());
        match bar {
            Some(bar) => {
                let frame = scan.frame(&bar);
                for item in scan.children(&bar, "AXChildren", MENU_BAR_ITEMS) {
                    roots.push((item, frame.clone(), false));
                }
            }
            None => {
                scan.incomplete |= scan.last_error.is_some_and(|code| !ABSENT.contains(&code));
            }
        }
    }

    // An open menu is a window of its own at the menu layer. A window there
    // that cannot be read might hold an unseen match, so it leaves the scan
    // incomplete.
    match windows::windows() {
        Ok(list) => {
            let menus = list.into_iter().filter(|window| {
                window.pid == i64::from(pid) && window.layer == MENU_LAYER && window.alpha > 0.0
            });
            for menu in menus {
                let bounds = Frame {
                    x: menu.bounds.x,
                    y: menu.bounds.y,
                    width: menu.bounds.width,
                    height: menu.bounds.height,
                };

                // A window the app lists among its windows is read already.
                if roots
                    .iter()
                    .any(|(_, frame, _)| frame.as_ref().is_some_and(|frame| frame.matches(&bounds)))
                {
                    continue;
                }

                let read: Vec<Frame> = roots
                    .iter()
                    .filter_map(|(_, frame, _)| frame.clone())
                    .collect();
                match scan.drawn_at(&app, &bounds, &read) {
                    Some(Drawn::Menu(element)) => roots.push((element, Some(bounds), true)),
                    Some(Drawn::Window(element)) => roots.push((element, Some(bounds), false)),
                    Some(Drawn::Through(hits)) => unvisited.extend(hits),
                    None => scan.incomplete = true,
                }
            }
        }
        Err(_) => scan.incomplete = true,
    }

    let open_menus: Vec<Frame> = roots
        .iter()
        .filter(|(.., menu)| *menu)
        .filter_map(|(_, frame, _)| frame.clone())
        .collect();
    let mut found = Vec::new();
    let mut stack: Vec<(CFRetained<AXUIElement>, usize, Option<Frame>, bool)> = roots
        .into_iter()
        .rev()
        .map(|(root, frame, menu)| (root, 0, frame, menu))
        .collect();
    // Each element is read once: an app can list an element under itself
    // (TextEdit lists its own application element as a child), and without
    // this the scan would walk that loop until its budget ran out.
    let mut seen: HashSet<CFRetained<AXUIElement>> = HashSet::new();

    while let Some((element, depth, window, menu)) = stack.pop() {
        if found.len() >= limits.count || !scan.open() {
            scan.incomplete = true;
            break;
        }

        if !seen.insert(element.clone()) {
            continue;
        }

        let item = &*element;
        unvisited.retain(|hit| **hit != *item);
        scan.unreadable.clear();
        scan.truncated.clear();

        let role = scan.text(item, "AXRole");
        scan.last_error = None;
        let frame = scan.frame(item);

        if depth > 0 && role.as_deref() == Some("AXMenu") {
            // A frame that failed to read (rather than being absent) leaves
            // an open menu possible: it might hold an unseen match.
            if frame.is_none() && scan.last_error.is_some_and(|code| !ABSENT.contains(&code)) {
                scan.incomplete = true;
                continue;
            }

            if skip_nested_menu(frame.as_ref(), &open_menus) {
                continue;
            }
        }

        let window = sheet_window(role.as_deref(), frame.as_ref(), window);
        let mut element = Element {
            index: found.len(),
            depth,
            window: window.clone(),
            role,
            subrole: scan.text(item, "AXSubrole"),
            title: scan.text(item, "AXTitle"),
            description: scan.text(item, "AXDescription"),
            value: scan.value(item),
            identifier: scan.text(item, "AXIdentifier"),
            enabled: scan.flag(item, "AXEnabled"),
            focused: scan.flag(item, "AXFocused"),
            frame,
            unreadable: Vec::new(),
            truncated: Vec::new(),
            menu,
            mark: None,
        };
        if element.role.as_deref() == Some("AXMenuItem") {
            element.mark = scan
                .text(item, "AXMenuItemMarkChar")
                .filter(|mark| !mark.is_empty());
        }

        element.unreadable = std::mem::take(&mut scan.unreadable);
        element.truncated = std::mem::take(&mut scan.truncated);
        let bar_title = element.role.as_deref() == Some("AXMenuBarItem");
        found.push(element);

        // The menu under a menu bar title is closed, or read as a window of
        // its own above: nothing in it is on screen here.
        if bar_title {
            continue;
        }

        if depth + 1 >= limits.depth {
            scan.incomplete |= !scan.children(item, "AXChildren", 1).is_empty();
            continue;
        }

        let budget = limits.count.saturating_sub(found.len() + stack.len());
        let children = scan.children(item, "AXChildren", budget);
        for child in children.into_iter().rev() {
            stack.push((child, depth + 1, window.clone(), menu));
        }
    }

    if !unvisited.is_empty() {
        scan.incomplete = true;
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

    /// What a sheet holds is judged against the sheet, not the window it
    /// hangs from; anything else keeps its window.
    #[test]
    fn a_sheet_is_the_window_of_what_it_holds() {
        let window = Frame {
            x: 0.0,
            y: 0.0,
            width: 600.0,
            height: 400.0,
        };
        let sheet = Frame {
            x: 100.0,
            y: 50.0,
            width: 400.0,
            height: 300.0,
        };

        assert_eq!(
            sheet_window(Some("AXSheet"), Some(&sheet), Some(window.clone())),
            Some(sheet.clone())
        );
        assert_eq!(
            sheet_window(Some("AXButton"), Some(&sheet), Some(window.clone())),
            Some(window.clone())
        );
        assert_eq!(
            sheet_window(Some("AXSheet"), None, Some(window.clone())),
            Some(window)
        );
    }

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
    fn a_hit_is_placed_by_the_frames_above_it() {
        let frame = |x: f64, width: f64| Frame {
            x,
            y: 20.0,
            width,
            height: 100.0,
        };
        let menu = frame(10.0, 200.0);
        let window = frame(300.0, 600.0);
        let read = [window.clone()];
        let step = |is_menu, at: Option<&Frame>, top| judge(is_menu, at, top, &menu, &read);

        assert_eq!(step(true, Some(&menu), false), Step::Found { menu: true });
        assert_eq!(
            step(false, Some(&menu), false),
            Step::Found { menu: false },
            "a popover or list with the window's frame"
        );
        assert_eq!(
            step(true, Some(&frame(50.0, 90.0)), false),
            Step::Up,
            "a submenu over its parent is passed"
        );
        assert_eq!(step(false, None, false), Step::Up);
        assert_eq!(step(false, Some(&window), true), Step::Through);
        assert_eq!(
            step(false, Some(&frame(900.0, 40.0)), true),
            Step::Stop,
            "the top of something not read"
        );
        assert_eq!(step(false, None, true), Step::Stop);
    }

    #[test]
    fn only_closed_or_already_read_nested_menus_are_skipped() {
        let frame = |x: f64, width: f64| Frame {
            x,
            y: 20.0,
            width,
            height: 100.0,
        };
        let open = [frame(10.0, 200.0)];

        assert!(skip_nested_menu(None, &open), "no frame: closed");
        assert!(
            skip_nested_menu(Some(&frame(0.0, 0.0)), &open),
            "no size: closed"
        );
        assert!(
            skip_nested_menu(Some(&frame(10.5, 200.0)), &open),
            "read as its own"
        );
        assert!(
            !skip_nested_menu(Some(&frame(400.0, 200.0)), &open),
            "a menu a view draws is read"
        );
    }

    #[test]
    fn frames_match_within_a_point() {
        let frame = Frame {
            x: 10.0,
            y: 20.0,
            width: 31.0,
            height: 10.0,
        };
        let mut near = frame.clone();
        near.x += 0.5;
        near.height -= 1.0;
        assert!(frame.matches(&near));

        near.width += 2.0;
        assert!(!frame.matches(&near));
    }

    #[test]
    fn null_references_are_never_owned() {
        // SAFETY: null is never adopted.
        assert!(unsafe { adopt::<CFType>(std::ptr::null()) }.is_none());
    }

    /// The named AX errors are the raw codes the scan has always sorted on.
    #[test]
    fn ax_error_lists_keep_their_codes() {
        assert_eq!(INCOMPLETE.map(|e| e.0), [-25204, -25202, -25211]);
        assert_eq!(ABSENT.map(|e| e.0), [-25212, -25205, -25208]);
        assert_eq!(AXError::Failure.0, -25200);
    }

    /// Past the deadline nothing more is asked and the scan is incomplete.
    #[test]
    fn a_scan_closes_at_its_deadline() {
        let mut scan = Scan::new(Instant::now() + Duration::from_secs(60));
        assert!(scan.open() && !scan.incomplete);

        scan.incomplete = true;
        assert!(
            scan.open(),
            "an incomplete scan may still ask until its deadline"
        );

        scan.deadline = Instant::now();
        scan.incomplete = false;
        assert!(!scan.open() && scan.incomplete);
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

    /// Only a missing permission is called one: a busy app, or a process ID
    /// with no app, is named as such.
    #[test]
    fn an_unreachable_app_is_reported_for_what_it_is() {
        let failed = |error, trusted, exists| {
            let error = unreachable(error, trusted, exists);
            (error.code, error.error)
        };

        assert_eq!(
            failed(Some(AXError::APIDisabled), true, true).0,
            "permission"
        );
        assert_eq!(
            failed(Some(AXError::CannotComplete), false, true).0,
            "permission"
        );

        let (code, message) = failed(Some(AXError::CannotComplete), true, true);
        assert_eq!(code, "elements");
        assert!(message.contains("not responding"));

        let (code, message) = failed(Some(AXError::InvalidUIElement), true, true);
        assert_eq!(code, "elements");
        assert!(message.contains("No app"));
        assert!(
            failed(Some(AXError::CannotComplete), true, false)
                .1
                .contains("No app")
        );

        let (code, message) = failed(Some(AXError::Failure), true, true);
        assert_eq!(code, "elements");
        assert!(message.contains("-25200"));
    }

    fn button() -> Element {
        Element {
            index: 4,
            depth: 2,
            window: None,
            role: Some("AXButton".into()),
            subrole: None,
            title: None,
            description: None,
            value: None,
            identifier: None,
            enabled: None,
            focused: None,
            frame: None,
            unreadable: Vec::new(),
            truncated: Vec::new(),
            menu: false,
            mark: None,
        }
    }

    #[test]
    fn an_element_leaves_out_what_the_scan_did_not_read() {
        assert_eq!(
            serde_json::to_value(button()).unwrap(),
            serde_json::json!({"index": 4, "depth": 2, "role": "AXButton"})
        );
    }

    #[test]
    fn an_element_reads_back_without_its_absent_fields() {
        let element = button();
        let written = serde_json::to_string(&element).unwrap();
        assert_eq!(serde_json::from_str::<Element>(&written).unwrap(), element);

        let older = serde_json::json!({
            "index": 4,
            "depth": 2,
            "window": null,
            "role": "AXButton",
            "subrole": null,
            "title": null,
            "description": null,
            "value": null,
            "identifier": null,
            "enabled": null,
            "focused": null,
            "frame": null,
        });
        assert_eq!(serde_json::from_value::<Element>(older).unwrap(), element);

        let full = Element {
            window: Some(Frame {
                x: 0.0,
                y: 0.0,
                width: 600.0,
                height: 400.0,
            }),
            title: Some("Save".into()),
            enabled: Some(true),
            focused: Some(false),
            frame: Some(Frame {
                x: 10.0,
                y: 20.0,
                width: 30.0,
                height: 10.0,
            }),
            ..button()
        };
        let written = serde_json::to_string(&full).unwrap();
        assert_eq!(serde_json::from_str::<Element>(&written).unwrap(), full);
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
