//! Accessibility elements of one application, read *inside* the helper login:
//! its windows and the menus it has open. Read-only and bounded: a global
//! messaging timeout and an overall deadline keep an unresponsive app from
//! hanging the probe; depth, count, children fetched and string lengths are
//! capped before anything is copied, and a long text is read only from its
//! start. An element's attributes are asked for in one request. A scan that
//! hit a limit or a communication failure reports itself incomplete.
// FFI boundary: Accessibility reads return values through out-pointers.
#![allow(unsafe_code)]
use crate::{
    error::{Error, Result},
    native::windows::{self, Dock, MENU_LAYER},
};
use objc2_application_services::{
    AXCopyMultipleAttributeOptions, AXError, AXUIElement, AXValue, AXValueType,
};
use objc2_core_foundation::{
    CFArray, CFBoolean, CFIndex, CFNull, CFNumber, CFRange, CFRetained, CFString, CFType, CGPoint,
    CGSize,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashSet},
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

/// Units read past `TEXT_LIMIT` from the start of a long text, so the cut
/// can fall between characters.
const PREFIX_SLACK: CFIndex = 32;

/// Attributes read together for each element, in the order their values
/// come back. The value is asked for on its own, so a long document is never
/// copied whole, and so are the children, which are counted before any is
/// copied.
const BATCH: [&str; 12] = [
    "AXRole",
    "AXSubrole",
    "AXTitle",
    "AXDescription",
    "AXIdentifier",
    "AXEnabled",
    "AXFocused",
    "AXSelected",
    "AXPosition",
    "AXSize",
    "AXMenuItemMarkChar",
    "AXNumberOfCharacters",
];

/// Roles whose value is the text they count, so a long one can be read
/// from its start. Any other element's value is read whole.
const TEXT_ROLES: [&str; 5] = [
    "AXTextArea",
    "AXTextField",
    "AXComboBox",
    "AXSearchField",
    "AXStaticText",
];

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

/// The range read from the start of a text of `characters` UTF-16 units:
/// past the limit by a little, so the cut lands where it would in the whole
/// text (where a character starts at the limit depends only on the units
/// before it), but never past the end.
fn prefix_range(characters: CFIndex) -> CFRange {
    CFRange {
        location: 0,
        length: characters.min(TEXT_LIMIT + PREFIX_SLACK),
    }
}

/// The start of a long text, cut as [`bounded_string`] cuts it; None when
/// it is not text or not longer than the limit, which the plain value then
/// reports whole.
fn prefix_text(value: &CFType) -> Option<String> {
    let (text, cut) = value.downcast_ref::<CFString>().and_then(bounded_string)?;
    cut.then_some(text)
}

/// One attribute's answer in a batched read.
enum Slot {
    Value(CFRetained<CFType>),
    /// The read failed with this error.
    Failed(AXError),
    /// The element has no value for it.
    Empty,
    /// Not asked: the scan had closed, or the reply could not be read.
    Unread,
}

/// What a batched read returned in one slot: an error value is the error
/// it holds (one that cannot be read, or that claims success, is taken as
/// a failure), a null is no value, and anything else is the value.
fn slot(value: CFRetained<CFType>) -> Slot {
    if let Some(answer) = value.downcast_ref::<AXValue>() {
        // SAFETY: a live AXValue.
        if unsafe { answer.r#type() } == AXValueType::AXError {
            let mut code = AXError::Failure;
            // SAFETY: an error value read into the 32-bit code it holds.
            let read =
                unsafe { answer.value(AXValueType::AXError, NonNull::from(&mut code).cast()) };
            let failed = read && code != AXError::Success;
            return Slot::Failed(if failed { code } else { AXError::Failure });
        }
    }

    if value.downcast_ref::<CFNull>().is_some() {
        Slot::Empty
    } else {
        Slot::Value(value)
    }
}

/// The attributes in `BATCH` of one element, read in one request.
struct Batch {
    slots: Vec<Slot>,
    /// The whole request could not complete: the element did not answer,
    /// and asking it anything more would wait as long.
    stalled: bool,
}

impl Batch {
    fn unread() -> Self {
        Self {
            slots: BATCH.iter().map(|_| Slot::Unread).collect(),
            stalled: false,
        }
    }

    /// Every attribute failed as the whole request did, with an error that
    /// leaves the scan incomplete.
    fn failed(status: AXError) -> Self {
        Self {
            slots: BATCH.iter().map(|_| Slot::Failed(status)).collect(),
            stalled: true,
        }
    }

    /// One slot per attribute, or None when the reply holds another number
    /// of values.
    fn from_values(values: &CFArray<CFType>) -> Option<Self> {
        (values.len() == BATCH.len()).then(|| Self {
            slots: values.iter().map(slot).collect(),
            stalled: false,
        })
    }

    /// The answer for `attribute`; None for one the batch does not ask for.
    fn slot(&self, attribute: &str) -> Option<&Slot> {
        let at = BATCH.iter().position(|name| *name == attribute);
        debug_assert!(at.is_some(), "{attribute} is not read in a batch");
        self.slots.get(at?)
    }

    /// How many UTF-16 units the element's text holds, when it said. Never
    /// recorded as a failure: most elements hold no text.
    fn characters(&self) -> Option<CFIndex> {
        match self.slot("AXNumberOfCharacters")? {
            Slot::Value(value) => value.downcast_ref::<CFNumber>()?.as_isize(),
            _ => None,
        }
    }

    /// How many units a text holds that is read only from its start: one
    /// longer than the limit, of an element whose role is a text role and
    /// that is surely not a password field: its subrole is another or
    /// absent, not one that failed to read.
    fn long_text(&self, role: Option<&str>, subrole: Option<&str>) -> Option<CFIndex> {
        let text = role.is_some_and(|role| TEXT_ROLES.contains(&role));
        let secure = subrole == Some("AXSecureTextField")
            || matches!(
                self.slot("AXSubrole"),
                Some(Slot::Failed(status)) if !ABSENT.contains(status)
            );
        if !text || secure {
            return None;
        }

        self.characters()
            .filter(|characters| *characters > TEXT_LIMIT)
    }
}

/// Where an attribute is read from: the element itself, one request each,
/// or a batch already read from it.
#[derive(Clone, Copy)]
enum Source<'a> {
    Element(&'a AXUIElement),
    Batch(&'a Batch),
}

impl<'a> Source<'a> {
    /// The batch read from `element`, or the element itself when there is
    /// none to read from.
    fn of(element: &'a AXUIElement, batch: Option<&'a Batch>) -> Self {
        batch.map_or(Source::Element(element), Source::Batch)
    }
}

/// Why a scan is incomplete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stop {
    /// Some elements are nested deeper than the scan reads.
    Depth,
    /// The app has more elements than the scan reads.
    Count,
    /// The scan ran out of time.
    Time,
    /// Part of the app could not be read.
    Unread,
}

impl Stop {
    const ALL: [Stop; 4] = [Stop::Depth, Stop::Count, Stop::Time, Stop::Unread];

    /// The word a reply names it by.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Stop::Depth => "depth",
            Stop::Count => "count",
            Stop::Time => "time",
            Stop::Unread => "unread",
        }
    }

    /// The reason a reply names by `word`, if it is one of these.
    pub(crate) fn parse(word: &str) -> Option<Stop> {
        Stop::ALL.into_iter().find(|stop| stop.word() == word)
    }

    /// Why the scan stopped, in plain words.
    pub(crate) fn said(self) -> &'static str {
        match self {
            Stop::Time => "it ran out of time",
            Stop::Count => "the app has more elements than Atlas reads",
            Stop::Depth => "some elements are nested too deep",
            Stop::Unread => "part of the app could not be read",
        }
    }
}

/// Why a scan stopped, in plain words joined into one phrase; None when
/// there is no reason.
pub(crate) fn why(stops: impl IntoIterator<Item = Stop>) -> Option<String> {
    let said: Vec<&str> = stops.into_iter().map(Stop::said).collect();

    match said.as_slice() {
        [] => None,
        [one] => Some((*one).to_owned()),
        [rest @ .., last] => Some(format!("{} and {last}", rest.join(", "))),
    }
}

/// Whether an `"attribute:code"` error key says something went wrong,
/// rather than that the element simply lacks the attribute.
pub(crate) fn telling(key: &str) -> bool {
    let code = key
        .rsplit_once(':')
        .and_then(|(_, code)| code.parse::<i32>().ok());

    code.is_none_or(|code| !ABSENT.iter().any(|absent| absent.0 == code))
}

/// Take over the reference a successful Copy wrote to `slot`, if any.
///
/// # Safety
/// `slot` must hold null or a reference the caller owns (from a Copy).
unsafe fn adopt<T: objc2_core_foundation::Type>(slot: *const T) -> Option<CFRetained<T>> {
    // SAFETY: per contract; ownership of the one reference moves here.
    NonNull::new(slot.cast_mut()).map(|value| unsafe { CFRetained::from_raw(value) })
}

/// One traversal: its deadline, whether anything made it incomplete and
/// why, the selector attributes the current element failed to read or
/// reported only in part, and how often each attribute returned each AX
/// error.
struct Scan {
    deadline: Instant,
    incomplete: bool,
    stops: BTreeSet<Stop>,
    unreadable: Vec<String>,
    truncated: Vec<String>,
    errors: std::collections::BTreeMap<String, usize>,
    /// The last AX error any read returned.
    last_error: Option<AXError>,
    /// The names in `BATCH`, made once.
    names: CFRetained<CFArray<CFString>>,
}

impl Scan {
    fn new(deadline: Instant) -> Self {
        Self {
            deadline,
            incomplete: false,
            stops: BTreeSet::new(),
            unreadable: Vec::new(),
            truncated: Vec::new(),
            errors: Default::default(),
            last_error: None,
            names: CFArray::from_retained_objects(&BATCH.map(CFString::from_static_str)),
        }
    }

    /// Whether another AX request may start; past the deadline the scan is
    /// incomplete and nothing more is asked.
    fn open(&mut self) -> bool {
        let open = Instant::now() < self.deadline;
        if !open {
            self.stop(Stop::Time);
        }
        open
    }

    /// The scan is incomplete, for this reason.
    fn stop(&mut self, why: Stop) {
        self.incomplete = true;
        self.stops.insert(why);
    }

    fn failed(&mut self, status: AXError, attribute: &str) {
        self.last_error = Some(status);
        let key = format!("{attribute}:{}", status.0);
        *self.errors.entry(key).or_default() += 1;

        let getter_failed = status == AXError::Failure && TEXT_SELECTORS.contains(&attribute);

        if INCOMPLETE.contains(&status) {
            self.stop(Stop::Unread);
        } else if !ABSENT.contains(&status) && !getter_failed && SELECTORS.contains(&attribute) {
            self.unreadable.push(attribute.to_owned());
        }
    }

    /// One attribute value: copied from the element, or taken from a batch
    /// read from it, which asks nothing more.
    fn attribute(&mut self, from: Source, attribute: &'static str) -> Option<CFRetained<CFType>> {
        let element = match from {
            Source::Element(element) => element,
            Source::Batch(batch) => return self.take(batch, attribute),
        };
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

    /// An attribute from a batch, its failure recorded as a single read's
    /// would be.
    fn take(&mut self, batch: &Batch, attribute: &str) -> Option<CFRetained<CFType>> {
        match batch.slot(attribute) {
            Some(Slot::Value(value)) => Some(value.clone()),
            Some(Slot::Failed(status)) => {
                self.failed(*status, attribute);
                None
            }
            Some(Slot::Empty | Slot::Unread) => None,
            None => {
                self.stop(Stop::Unread);
                None
            }
        }
    }

    /// Read the attributes in `BATCH` of `element` in one request. Nothing
    /// is recorded until each is taken, so an attribute the walk does not
    /// read leaves no trace. None when the element must be read one
    /// attribute at a time instead.
    fn batch(&mut self, element: &AXUIElement) -> Option<Batch> {
        if !self.open() {
            return Some(Batch::unread());
        }

        let mut values: *const CFArray = std::ptr::null();
        // SAFETY: the names are an array of CFStrings as required and the
        // output slot is valid; on success the Copy hands over one owned
        // array, adopted below.
        let status = unsafe {
            element.copy_multiple_attribute_values(
                self.names.as_opaque(),
                AXCopyMultipleAttributeOptions::empty(),
                NonNull::from(&mut values),
            )
        };
        // SAFETY: adopted only after a successful Copy, which wrote an owned
        // array (or null).
        let values = (status == AXError::Success)
            .then(|| unsafe { adopt(values) })
            .flatten()
            .map(|values| {
                // SAFETY: every element of a CFArray is a CF object, each
                // checked before use.
                unsafe { CFRetained::cast_unchecked::<CFArray<CFType>>(values) }
            });
        self.received(status, values.as_deref())
    }

    /// The batch a request returned with `status` and `values`. A request
    /// that could not complete fails every attribute. Any other failed
    /// request (some apps do not take one for many attributes at all) is
    /// recorded once and the element is read one attribute at a time: None.
    /// A missing reply, or one with another number of values, is a failure
    /// that leaves the scan incomplete.
    fn received(&mut self, status: AXError, values: Option<&CFArray<CFType>>) -> Option<Batch> {
        if INCOMPLETE.contains(&status) {
            return Some(Batch::failed(status));
        }

        if status != AXError::Success {
            self.failed(status, "AXAttributes");
            return None;
        }

        if let Some(batch) = values.and_then(Batch::from_values) {
            return Some(batch);
        }

        self.failed(AXError::Failure, "AXAttributes");
        self.stop(Stop::Unread);
        Some(Batch::unread())
    }

    /// Whether `batch` came back stalled, so nothing more is asked of its
    /// element; the scan is then incomplete.
    fn stalled(&mut self, batch: Option<&Batch>) -> bool {
        let stalled = batch.is_some_and(|batch| batch.stalled);
        if stalled {
            self.stop(Stop::Unread);
        }

        stalled
    }

    fn text(&mut self, from: Source, name: &'static str) -> Option<String> {
        let value = self.attribute(from, name)?;
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

    fn flag(&mut self, from: Source, name: &'static str) -> Option<bool> {
        let value = self.attribute(from, name)?;
        value.downcast_ref::<CFBoolean>().map(CFBoolean::as_bool)
    }

    /// The element's value as text: strings bounded, numbers and booleans
    /// rendered. A text the batch counts as longer than the limit is read
    /// only from its start (see [`Batch::long_text`]). Nothing is asked of
    /// an element whose batch stalled.
    fn value(
        &mut self,
        element: &AXUIElement,
        batch: Option<&Batch>,
        role: Option<&str>,
        subrole: Option<&str>,
    ) -> Option<String> {
        if self.stalled(batch) {
            return None;
        }

        if let Some(characters) = batch.and_then(|batch| batch.long_text(role, subrole)) {
            match self.prefix(element, characters) {
                Ok(text) => {
                    self.truncated.push("AXValue".to_owned());
                    return Some(text);
                }
                // Asking again would wait as long for the same answer.
                Err(Some(status)) if INCOMPLETE.contains(&status) => return None,
                Err(_) => {}
            }
        }

        let value = self.attribute(Source::Element(element), "AXValue")?;
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

    /// The start of a text `characters` units long, cut at the limit; an
    /// error when it cannot be read so (with the AX error, if one came back)
    /// or is not longer than the limit after all.
    fn prefix(
        &mut self,
        element: &AXUIElement,
        characters: CFIndex,
    ) -> std::result::Result<String, Option<AXError>> {
        if !self.open() {
            return Err(None);
        }

        let range = prefix_range(characters);
        // SAFETY: a pointer to a live CFRange, the type the value is made as.
        let parameter = unsafe { AXValue::new(AXValueType::CFRange, NonNull::from(&range).cast()) }
            .ok_or(None)?;
        let name = CFString::from_static_str("AXStringForRange");
        let mut value: *const CFType = std::ptr::null();
        // SAFETY: it takes a range value and the output slot is valid; on
        // success the Copy hands over one owned reference, adopted below.
        let status = unsafe {
            element.copy_parameterized_attribute_value(&name, &parameter, NonNull::from(&mut value))
        };
        if status != AXError::Success {
            // An element that cannot read by range just has its value read
            // whole; that says nothing went wrong.
            if status != AXError::ParameterizedAttributeUnsupported {
                self.failed(status, "AXStringForRange");
            }
            return Err(Some(status));
        }

        // SAFETY: a successful Copy wrote an owned reference (or null).
        let value = unsafe { adopt(value) }.ok_or(None)?;
        prefix_text(&value).ok_or(None)
    }

    fn frame(&mut self, from: Source) -> Option<Frame> {
        let position = self.attribute(from, "AXPosition")?;
        let size = self.attribute(from, "AXSize")?;
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
        let count = self.count(element, attribute);
        if count <= 0 {
            return Vec::new();
        }

        let name = CFString::from_static_str(attribute);
        let wanted = count.min(CFIndex::try_from(maximum).unwrap_or(CFIndex::MAX));
        if wanted < count {
            self.stop(Stop::Count);
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
            self.stop(Stop::Unread);
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

    /// Whether an array attribute holds anything, from its count alone:
    /// nothing is copied.
    fn has_children(&mut self, element: &AXUIElement, attribute: &'static str) -> bool {
        self.count(element, attribute) > 0
    }

    /// How many values an array attribute holds; 0 once the deadline has
    /// passed or when the count fails to read. A failure other than the
    /// attribute being absent leaves the scan incomplete.
    fn count(&mut self, element: &AXUIElement, attribute: &'static str) -> CFIndex {
        if !self.open() {
            return 0;
        }

        let name = CFString::from_static_str(attribute);
        let mut count: CFIndex = 0;
        // SAFETY: a valid output slot for the count.
        let status = unsafe { element.attribute_value_count(&name, NonNull::from(&mut count)) };
        if status != AXError::Success {
            self.failed(status, attribute);
            if !ABSENT.contains(&status) {
                self.stop(Stop::Unread);
            }

            return 0;
        }

        count
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

                let frame = self.frame(Source::Element(&current));
                let menu =
                    self.text(Source::Element(&current), "AXRole").as_deref() == Some("AXMenu");
                let parent = self
                    .attribute(Source::Element(&current), "AXParent")
                    .and_then(|parent| parent.downcast::<AXUIElement>().ok());
                let top = parent.as_ref().is_some_and(|parent| {
                    self.text(Source::Element(parent), "AXRole").as_deref() == Some("AXApplication")
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
    /// Whether a row or cell is selected; not read for other elements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
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
    /// Why it is incomplete.
    pub stops: BTreeSet<Stop>,
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

/// The process of the application with keyboard focus in this login, as
/// Accessibility reports it; None when no application has it. Waits at most
/// `timeout`. This sets the calling process's messaging timeout, which the
/// input service uses for nothing else.
pub fn focused_pid(timeout: Duration) -> Result<Option<i64>> {
    let unreadable = || Error::new("native", "The focused app could not be read");

    // SAFETY: a plain constructor returning an owned element or null.
    let system =
        unsafe { AXUIElementCreateSystemWide().map(|system| CFRetained::from_raw(system)) }
            .ok_or_else(unreadable)?;
    // SAFETY: the system-wide element sets this process's global messaging
    // timeout, which covers the read below. A read without it could wait
    // on an unresponsive app for far longer.
    if unsafe { system.set_messaging_timeout(timeout.as_secs_f32()) } != AXError::Success {
        return Err(unreadable());
    }

    let name = CFString::from_static_str("AXFocusedApplication");
    let mut value: *const CFType = std::ptr::null();
    // SAFETY: a valid output slot; on success the Copy hands over one owned
    // reference, adopted below.
    let status = unsafe { system.copy_attribute_value(&name, NonNull::from(&mut value)) };
    match status {
        AXError::Success => {}
        AXError::NoValue => return Ok(None),
        _ => return Err(unreadable()),
    }

    // SAFETY: a successful Copy wrote an owned reference (or null).
    let focused = unsafe { adopt(value) }.ok_or_else(unreadable)?;
    let app = focused
        .downcast_ref::<AXUIElement>()
        .ok_or_else(unreadable)?;
    let mut pid: libc::pid_t = 0;
    // SAFETY: a valid `pid_t` slot for the element's process.
    let status = unsafe { app.pid(NonNull::from(&mut pid)) };
    if status != AXError::Success || pid <= 0 {
        return Err(unreadable());
    }

    Ok(Some(pid.into()))
}

/// The longest a read of where the Dock takes clicks may take.
pub const DOCK_TIMEOUT: Duration = Duration::from_millis(250);

/// Most children of the Dock's application element read; it has one list.
const DOCK_CHILDREN: usize = 16;

/// Where the Dock takes clicks: the frames of its lists of icons, read
/// from the process owning the Dock's window in `windows`, waiting at most
/// `timeout`. With no Dock window there is nothing of the Dock's to click.
/// A Dock that shows anything but lists of icons is [`Dock::Other`], and
/// one that cannot all be read in time is [`Dock::Unread`]; either covers
/// all of its window.
pub fn dock(windows: &[windows::Window], timeout: Duration) -> Dock {
    let Some(pid) = windows
        .iter()
        .find(|window| window.is_dock())
        .map(|window| window.pid)
    else {
        return Dock::Icons(Vec::new());
    };
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return Dock::Unread;
    };

    // SAFETY: a plain constructor returning an owned element or null.
    let Some(app) = (unsafe { AXUIElementCreateApplication(pid) })
        // SAFETY: the constructor handed over one owned reference.
        .map(|app| unsafe { CFRetained::from_raw(app) })
    else {
        return Dock::Unread;
    };
    // SAFETY: sets this one element's messaging timeout, which bounds each
    // read below; nothing else in the process is affected.
    if unsafe { app.set_messaging_timeout(timeout.as_secs_f32()) } != AXError::Success {
        return Dock::Unread;
    }

    let mut scan = Scan::new(Instant::now() + timeout);
    let mut lists = Vec::new();
    let mut other = false;
    for child in scan.children(&app, "AXChildren", DOCK_CHILDREN) {
        let Some(role) = scan.text(Source::Element(&child), "AXRole") else {
            return Dock::Unread;
        };
        if role != "AXList" {
            // Mission Control's spaces bar and the like take clicks too.
            other = true;
            continue;
        }

        let Some(frame) = scan.frame(Source::Element(&child)) else {
            return Dock::Unread;
        };
        lists.push(windows::Bounds {
            x: frame.x,
            y: frame.y,
            width: frame.width,
            height: frame.height,
        });
    }

    if scan.incomplete {
        return Dock::Unread;
    }
    if other || lists.is_empty() {
        return Dock::Other;
    }

    Dock::Icons(lists)
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
    if windows.is_empty() && scan.text(Source::Element(&app), "AXRole").is_none() {
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
            let frame = scan.frame(Source::Element(&window));
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
    let frontmost = scan.flag(Source::Element(&app), "AXFrontmost");
    if frontmost.is_none() && scan.last_error.is_some_and(|code| !ABSENT.contains(&code)) {
        scan.stop(Stop::Unread);
    }

    if frontmost == Some(true) {
        scan.last_error = None;
        let bar = scan
            .attribute(Source::Element(&app), "AXMenuBar")
            .and_then(|bar| bar.downcast::<AXUIElement>().ok());
        match bar {
            Some(bar) => {
                let frame = scan.frame(Source::Element(&bar));
                for item in scan.children(&bar, "AXChildren", MENU_BAR_ITEMS) {
                    roots.push((item, frame.clone(), false));
                }
            }
            None => {
                if scan.last_error.is_some_and(|code| !ABSENT.contains(&code)) {
                    scan.stop(Stop::Unread);
                }
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
                    None => scan.stop(Stop::Unread),
                }
            }
        }
        Err(_) => scan.stop(Stop::Unread),
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
        if found.len() >= limits.count {
            scan.stop(Stop::Count);
            break;
        }

        if !scan.open() {
            break;
        }

        if !seen.insert(element.clone()) {
            continue;
        }

        let item = &*element;
        unvisited.retain(|hit| **hit != *item);
        scan.unreadable.clear();
        scan.truncated.clear();

        let values = scan.batch(item);
        let from = Source::of(item, values.as_ref());
        let role = scan.text(from, "AXRole");
        scan.last_error = None;
        let frame = scan.frame(from);

        if depth > 0 && role.as_deref() == Some("AXMenu") {
            // A frame that failed to read (rather than being absent) leaves
            // an open menu possible: it might hold an unseen match.
            if frame.is_none() && scan.last_error.is_some_and(|code| !ABSENT.contains(&code)) {
                scan.stop(Stop::Unread);
                continue;
            }

            if skip_nested_menu(frame.as_ref(), &open_menus) {
                continue;
            }
        }

        let window = sheet_window(role.as_deref(), frame.as_ref(), window);
        let selectable = matches!(role.as_deref(), Some("AXRow" | "AXCell"));
        let subrole = scan.text(from, "AXSubrole");
        let title = scan.text(from, "AXTitle");
        let description = scan.text(from, "AXDescription");
        let value = scan.value(item, values.as_ref(), role.as_deref(), subrole.as_deref());
        let mut element = Element {
            index: found.len(),
            depth,
            window: window.clone(),
            role,
            title,
            description,
            value,
            subrole,
            identifier: scan.text(from, "AXIdentifier"),
            enabled: scan.flag(from, "AXEnabled"),
            focused: scan.flag(from, "AXFocused"),
            selected: selectable.then(|| scan.flag(from, "AXSelected")).flatten(),
            frame,
            unreadable: Vec::new(),
            truncated: Vec::new(),
            menu,
            mark: None,
        };
        if element.role.as_deref() == Some("AXMenuItem") {
            element.mark = scan
                .text(from, "AXMenuItemMarkChar")
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

        // An element that did not answer is not asked for its children.
        if scan.stalled(values.as_ref()) {
            continue;
        }

        if depth + 1 >= limits.depth {
            if scan.has_children(item, "AXChildren") {
                scan.stop(Stop::Depth);
            }
            continue;
        }

        let budget = limits.count.saturating_sub(found.len() + stack.len());
        let children = scan.children(item, "AXChildren", budget);
        for child in children.into_iter().rev() {
            stack.push((child, depth + 1, window.clone(), menu));
        }
    }

    if !unvisited.is_empty() {
        scan.stop(Stop::Unread);
    }

    // A request that started before the deadline may have ended after it.
    scan.open();

    Ok(Snapshot {
        elements: found,
        incomplete: scan.incomplete,
        stops: scan.stops,
        errors: scan.errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_core_foundation::{Type, kCFNull};

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
        assert_eq!(scan.stops, BTreeSet::from([Stop::Time]), "and says why");
    }

    #[test]
    fn scan_stops_are_said_in_plain_words() {
        for stop in Stop::ALL {
            assert_eq!(Stop::parse(stop.word()), Some(stop));
            assert_eq!(
                serde_json::to_value(stop).unwrap(),
                serde_json::json!(stop.word())
            );
        }

        assert_eq!(why([Stop::Time]).as_deref(), Some("it ran out of time"));
        assert_eq!(
            why([Stop::Count, Stop::Unread]).as_deref(),
            Some(
                "the app has more elements than Atlas reads and part of the app could not be read"
            )
        );
        assert_eq!(
            why([Stop::Depth, Stop::Count, Stop::Time]).as_deref(),
            Some(
                "some elements are nested too deep, the app has more elements than Atlas reads \
                 and it ran out of time"
            )
        );
        assert_eq!(Stop::parse("later"), None, "an unknown reason is not one");
        assert_eq!(why([]), None);
    }

    #[test]
    fn only_failures_other_than_a_missing_attribute_are_telling() {
        for absent in ABSENT {
            assert!(!telling(&format!("AXTitle:{}", absent.0)));
        }
        for code in [AXError::Failure, AXError::CannotComplete] {
            assert!(telling(&format!("AXChildren:{}", code.0)));
        }
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
            selected: None,
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

    /// An AX error as a batched read returns it in a slot.
    fn error_value(code: AXError) -> CFRetained<CFType> {
        // SAFETY: a pointer to a live 32-bit code, the type the value is made as.
        unsafe { AXValue::new(AXValueType::AXError, NonNull::from(&code).cast()) }
            .unwrap()
            .into()
    }

    /// The null a batched read returns for an attribute with no value.
    fn null() -> CFRetained<CFType> {
        // SAFETY: a constant CoreFoundation defines for the life of the process.
        unsafe { kCFNull }.unwrap().retain().into()
    }

    fn point(x: f64, y: f64) -> CFRetained<CFType> {
        let point = CGPoint { x, y };
        // SAFETY: a pointer to a live CGPoint, the type the value is made as.
        unsafe { AXValue::new(AXValueType::CGPoint, NonNull::from(&point).cast()) }
            .unwrap()
            .into()
    }

    fn size(width: f64, height: f64) -> CFRetained<CFType> {
        let size = CGSize { width, height };
        // SAFETY: a pointer to a live CGSize, the type the value is made as.
        unsafe { AXValue::new(AXValueType::CGSize, NonNull::from(&size).cast()) }
            .unwrap()
            .into()
    }

    fn string(text: &str) -> CFRetained<CFType> {
        CFString::from_str(text).into()
    }

    /// A batch read as an app would answer it: these attributes as given,
    /// every other one with no value.
    fn answered(answers: Vec<(&str, CFRetained<CFType>)>) -> Batch {
        let values: Vec<CFRetained<CFType>> = BATCH
            .iter()
            .map(|name| {
                answers
                    .iter()
                    .find(|(answered, _)| answered == name)
                    .map_or_else(null, |(_, value)| value.clone())
            })
            .collect();
        Batch::from_values(&CFArray::from_retained_objects(&values)).unwrap()
    }

    fn open_scan() -> Scan {
        Scan::new(Instant::now() + Duration::from_secs(60))
    }

    /// What a scan has recorded, for comparing two scans.
    fn recorded(scan: &Scan) -> impl PartialEq + std::fmt::Debug + use<> {
        (
            scan.errors.clone(),
            scan.unreadable.clone(),
            scan.truncated.clone(),
            scan.incomplete,
            scan.stops.clone(),
            scan.last_error.map(|error| error.0),
        )
    }

    /// What one slot holds, in words.
    fn held(slot: &Slot) -> String {
        match slot {
            Slot::Value(_) => "value".into(),
            Slot::Failed(status) => format!("failed {}", status.0),
            Slot::Empty => "empty".into(),
            Slot::Unread => "unread".into(),
        }
    }

    #[test]
    fn an_error_value_reads_as_the_ax_error_it_holds() {
        assert_eq!(
            held(&slot(error_value(AXError::CannotComplete))),
            "failed -25204"
        );
        assert_eq!(held(&slot(error_value(AXError::NoValue))), "failed -25212");
        assert_eq!(
            held(&slot(error_value(AXError::Success))),
            "failed -25200",
            "an error that claims success is taken as a failure"
        );
        assert_eq!(held(&slot(null())), "empty");
        assert_eq!(held(&slot(string("Save"))), "value");
        assert_eq!(
            held(&slot(point(1.0, 2.0))),
            "value",
            "a point is not an error"
        );
    }

    #[test]
    fn a_batched_read_keeps_the_bookkeeping_of_single_reads() {
        let batch = answered(vec![
            ("AXRole", string("AXTextField")),
            ("AXSubrole", error_value(AXError::AttributeUnsupported)),
            ("AXTitle", error_value(AXError::Failure)),
            ("AXDescription", error_value(AXError::IllegalArgument)),
            ("AXIdentifier", error_value(AXError::CannotComplete)),
            ("AXEnabled", CFBoolean::new(true).retain().into()),
            ("AXPosition", point(10.0, 20.0)),
            ("AXSize", size(30.0, 40.0)),
        ]);
        let from = Source::Batch(&batch);
        let mut scan = open_scan();

        assert_eq!(scan.text(from, "AXRole").as_deref(), Some("AXTextField"));
        assert_eq!(
            scan.frame(from),
            Some(Frame {
                x: 10.0,
                y: 20.0,
                width: 30.0,
                height: 40.0,
            })
        );
        assert_eq!(scan.text(from, "AXSubrole"), None);
        assert_eq!(scan.text(from, "AXTitle"), None);
        assert_eq!(scan.text(from, "AXDescription"), None);
        assert_eq!(scan.text(from, "AXIdentifier"), None);
        assert_eq!(scan.flag(from, "AXEnabled"), Some(true));
        assert_eq!(scan.flag(from, "AXFocused"), None);

        let mut single = open_scan();
        single.failed(AXError::AttributeUnsupported, "AXSubrole");
        single.failed(AXError::Failure, "AXTitle");
        single.failed(AXError::IllegalArgument, "AXDescription");
        single.failed(AXError::CannotComplete, "AXIdentifier");

        assert_eq!(recorded(&scan), recorded(&single));
        assert_eq!(scan.unreadable, ["AXDescription"]);
        assert!(scan.incomplete);
    }

    #[test]
    fn a_failed_batch_fails_every_attribute_taken_from_it() {
        let batch = Batch::failed(AXError::CannotComplete);
        let from = Source::Batch(&batch);
        let mut scan = open_scan();
        scan.text(from, "AXRole");
        scan.frame(from);
        for name in ["AXSubrole", "AXTitle", "AXDescription", "AXIdentifier"] {
            scan.text(from, name);
        }
        for name in ["AXEnabled", "AXFocused"] {
            scan.flag(from, name);
        }

        let mut single = open_scan();
        for name in [
            "AXRole",
            "AXPosition",
            "AXSubrole",
            "AXTitle",
            "AXDescription",
            "AXIdentifier",
            "AXEnabled",
            "AXFocused",
        ] {
            single.failed(AXError::CannotComplete, name);
        }

        assert_eq!(recorded(&scan), recorded(&single));
    }

    #[test]
    fn a_size_is_not_read_when_the_position_failed() {
        let batch = answered(vec![
            ("AXPosition", error_value(AXError::Failure)),
            ("AXSize", error_value(AXError::CannotComplete)),
        ]);
        let mut scan = open_scan();
        assert_eq!(scan.frame(Source::Batch(&batch)), None);

        let mut single = open_scan();
        single.failed(AXError::Failure, "AXPosition");
        assert_eq!(recorded(&scan), recorded(&single));
        assert!(!scan.incomplete);
    }

    #[test]
    fn slots_the_walk_does_not_read_leave_no_trace() {
        let batch = answered(vec![
            ("AXRole", string("AXButton")),
            ("AXSelected", error_value(AXError::CannotComplete)),
            ("AXMenuItemMarkChar", error_value(AXError::Failure)),
            ("AXNumberOfCharacters", error_value(AXError::CannotComplete)),
            ("AXSize", error_value(AXError::CannotComplete)),
        ]);
        let from = Source::Batch(&batch);
        let mut scan = open_scan();
        scan.text(from, "AXRole");
        assert_eq!(scan.frame(from), None, "no position, so no size is read");
        assert_eq!(batch.characters(), None);

        assert_eq!(recorded(&scan), recorded(&open_scan()));
    }

    #[test]
    fn a_closed_scan_sends_no_batch_and_takes_nothing() {
        let system = system_wide();
        let mut scan = open_scan();
        scan.deadline = Instant::now();

        let batch = scan.batch(&system).unwrap();
        assert!(batch.slots.iter().all(|slot| matches!(slot, Slot::Unread)));
        let from = Source::Batch(&batch);
        assert_eq!(scan.text(from, "AXRole"), None);
        assert_eq!(scan.frame(from), None);
        assert_eq!(scan.value(&system, Some(&batch), None, None), None);

        assert!(scan.errors.is_empty() && scan.unreadable.is_empty());
        assert_eq!(scan.stops, BTreeSet::from([Stop::Time]));
    }

    #[test]
    fn a_batch_of_the_wrong_length_leaves_the_scan_incomplete() {
        let short = CFArray::from_retained_objects(&[string("AXButton"), null()]);
        for reply in [Some(&*short), None] {
            let mut scan = open_scan();
            let batch = scan.received(AXError::Success, reply).unwrap();
            assert!(batch.slots.iter().all(|slot| matches!(slot, Slot::Unread)));
            assert_eq!(scan.text(Source::Batch(&batch), "AXRole"), None);

            assert_eq!(
                scan.errors,
                std::collections::BTreeMap::from([("AXAttributes:-25200".to_owned(), 1)])
            );
            assert_eq!(scan.stops, BTreeSet::from([Stop::Unread]));
        }
    }

    #[test]
    fn the_batch_asks_for_every_attribute_an_element_reports() {
        let asked: HashSet<&str> = BATCH.into_iter().collect();
        assert_eq!(asked.len(), BATCH.len(), "no attribute is asked twice");
        assert_eq!(open_scan().names.len(), BATCH.len());

        for selector in SELECTORS.into_iter().filter(|name| *name != "AXValue") {
            assert!(asked.contains(selector), "{selector}");
        }
        for name in [
            "AXSubrole",
            "AXEnabled",
            "AXFocused",
            "AXSelected",
            "AXPosition",
            "AXSize",
            "AXMenuItemMarkChar",
            "AXNumberOfCharacters",
        ] {
            assert!(asked.contains(name), "{name}");
        }
        assert!(
            !asked.contains("AXValue"),
            "a long value is never copied in a batch"
        );
    }

    fn system_wide() -> CFRetained<AXUIElement> {
        // SAFETY: a plain constructor returning an owned element or null,
        // adopted here.
        unsafe { AXUIElementCreateSystemWide().map(|system| CFRetained::from_raw(system)) }.unwrap()
    }

    #[test]
    fn an_element_that_refuses_the_batch_is_read_one_attribute_at_a_time() {
        for status in [
            AXError::NotImplemented,
            AXError::AttributeUnsupported,
            AXError::Failure,
            AXError::IllegalArgument,
        ] {
            let mut scan = open_scan();
            assert!(scan.received(status, None).is_none(), "{}", status.0);
            assert_eq!(
                scan.errors,
                std::collections::BTreeMap::from([(format!("AXAttributes:{}", status.0), 1)])
            );
            assert!(scan.unreadable.is_empty());
            assert!(!scan.incomplete);
        }

        let system = system_wide();
        assert!(matches!(
            Source::of(&system, None),
            Source::Element(element) if *element == *system
        ));
        let batch = Batch::unread();
        assert!(matches!(
            Source::of(&system, Some(&batch)),
            Source::Batch(_)
        ));
    }

    #[test]
    fn a_batch_that_could_not_complete_stalls_its_element() {
        for status in INCOMPLETE {
            let mut scan = open_scan();
            let batch = scan.received(status, None).unwrap();
            assert!(batch.stalled);
            assert!(
                batch
                    .slots
                    .iter()
                    .all(|slot| matches!(slot, Slot::Failed(failed) if *failed == status))
            );
            assert!(scan.errors.is_empty(), "nothing is recorded until taken");
        }
    }

    #[test]
    fn an_element_whose_batch_stalled_is_not_asked_for_its_value() {
        let system = system_wide();
        let batch = Batch::failed(AXError::CannotComplete);
        let mut scan = open_scan();

        assert_eq!(
            scan.value(&system, Some(&batch), Some("AXTextArea"), None),
            None
        );
        assert!(scan.errors.is_empty(), "no request was sent");
        assert_eq!(scan.stops, BTreeSet::from([Stop::Unread]));

        let mut answered = open_scan();
        assert!(!answered.stalled(Some(&Batch::unread())));
        assert!(!answered.stalled(None));
        assert!(!answered.incomplete);
    }

    #[test]
    fn only_a_long_text_role_that_is_surely_no_password_is_read_from_its_start() {
        let long = || CFNumber::new_isize(5000).into();
        let batch = answered(vec![("AXNumberOfCharacters", long())]);
        for role in TEXT_ROLES {
            assert_eq!(batch.long_text(Some(role), None), Some(5000), "{role}");
        }
        for role in [None, Some("AXGroup"), Some("AXWebArea"), Some("AXButton")] {
            assert_eq!(batch.long_text(role, None), None, "{role:?}");
        }
        assert_eq!(
            batch.long_text(Some("AXTextField"), Some("AXSecureTextField")),
            None
        );

        let unread_subrole = answered(vec![
            ("AXNumberOfCharacters", long()),
            ("AXSubrole", error_value(AXError::Failure)),
        ]);
        assert_eq!(unread_subrole.long_text(Some("AXTextField"), None), None);

        let no_subrole = answered(vec![
            ("AXNumberOfCharacters", long()),
            ("AXSubrole", error_value(AXError::AttributeUnsupported)),
        ]);
        assert_eq!(no_subrole.long_text(Some("AXTextField"), None), Some(5000));
        assert_eq!(
            batch.long_text(Some("AXTextField"), Some("AXSearchField")),
            Some(5000)
        );

        let short = answered(vec![(
            "AXNumberOfCharacters",
            CFNumber::new_isize(TEXT_LIMIT).into(),
        )]);
        assert_eq!(short.long_text(Some("AXTextArea"), None), None);
    }

    #[test]
    fn a_long_text_prefix_is_cut_at_the_limit() {
        let prefix = string(&"x".repeat(232));
        assert_eq!(prefix_text(&prefix), Some("x".repeat(200)));
    }

    #[test]
    fn a_prefix_no_longer_than_the_limit_reads_the_whole_value_instead() {
        assert_eq!(prefix_text(&string(&"y".repeat(200))), None);
        assert_eq!(prefix_text(&string("short")), None);
        let number: CFRetained<CFType> = CFNumber::new_i32(7).into();
        assert_eq!(prefix_text(&number), None);
    }

    /// The first units of `text` that a prefix read asks for.
    fn asked_prefix(text: &str) -> CFRetained<CFType> {
        let units: Vec<u16> = text.encode_utf16().collect();
        let range = prefix_range(units.len() as CFIndex);
        string(&String::from_utf16(&units[..range.length as usize]).unwrap())
    }

    #[test]
    fn a_prefix_is_cut_between_characters() {
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
        let tail = "z".repeat(5000);
        for text in [
            format!("{}{family}{tail}", "a".repeat(199)),
            format!("{}e\u{301}{tail}", "a".repeat(199)),
            format!("{}e\u{301}{tail}", "a".repeat(198)),
        ] {
            let (whole, cut) = bound(&text);
            assert!(cut);
            assert_eq!(
                prefix_text(&asked_prefix(&text)),
                Some(whole),
                "the prefix is cut where the whole text is"
            );
        }
    }

    #[test]
    fn the_prefix_range_covers_the_limit_and_a_little_more() {
        for (characters, length) in [(5_000_000, 232), (233, 232), (232, 232), (201, 201)] {
            let range = prefix_range(characters);
            assert_eq!((range.location, range.length), (0, length));
        }
    }
}
