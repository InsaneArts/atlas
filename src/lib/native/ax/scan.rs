//! One traversal's reads: each attribute, count and array of children asked
//! of an element, bounded by the traversal's deadline, and the record of
//! what failed, what was cut and why the traversal is incomplete.
use super::{
    ABSENT, Frame, INCOMPLETE, Stop,
    batch::{BATCH, Batch, Slot, Source},
    ffi,
    text::{bounded_string, getter_failed, prefix_range, prefix_text},
};
use objc2_application_services::{AXError, AXUIElement, AXValue};
use objc2_core_foundation::{CFArray, CFBoolean, CFIndex, CFNumber, CFRetained, CFString, CFType};
use std::{collections::BTreeSet, time::Instant};

/// Attributes a selector matches on; an element whose selector attribute
/// fails to read (other than being absent) may be an unseen match.
pub(super) const SELECTORS: [&str; 5] = [
    "AXRole",
    "AXTitle",
    "AXDescription",
    "AXIdentifier",
    "AXValue",
];

/// One traversal: its deadline, whether anything made it incomplete and
/// why, the selector attributes the current element failed to read or
/// reported only in part, and how often each attribute returned each AX
/// error.
pub(super) struct Scan {
    pub(super) deadline: Instant,
    pub(super) incomplete: bool,
    pub(super) stops: BTreeSet<Stop>,
    pub(super) unreadable: Vec<String>,
    pub(super) truncated: Vec<String>,
    pub(super) errors: std::collections::BTreeMap<String, usize>,
    /// The last AX error any read returned.
    pub(super) last_error: Option<AXError>,
    /// The names in `BATCH`, made once.
    pub(super) names: CFRetained<CFArray<CFString>>,
}

impl Scan {
    pub(super) fn new(deadline: Instant) -> Self {
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
    pub(super) fn open(&mut self) -> bool {
        let open = Instant::now() < self.deadline;
        if !open {
            self.stop(Stop::Time);
        }
        open
    }

    /// The scan is incomplete, for this reason.
    pub(super) fn stop(&mut self, why: Stop) {
        self.incomplete = true;
        self.stops.insert(why);
    }

    pub(super) fn failed(&mut self, status: AXError, attribute: &str) {
        self.last_error = Some(status);
        let key = format!("{attribute}:{}", status.0);
        *self.errors.entry(key).or_default() += 1;

        if INCOMPLETE.contains(&status) {
            self.stop(Stop::Unread);
        } else if !ABSENT.contains(&status)
            && !getter_failed(status, attribute)
            && SELECTORS.contains(&attribute)
        {
            self.unreadable.push(attribute.to_owned());
        }
    }

    /// One attribute value: copied from the element, or taken from a batch
    /// read from it, which asks nothing more.
    pub(super) fn attribute(
        &mut self,
        from: Source,
        attribute: &'static str,
    ) -> Option<CFRetained<CFType>> {
        let element = match from {
            Source::Element(element) => element,
            Source::Batch(batch) => return self.take(batch, attribute),
        };
        if !self.open() {
            return None;
        }

        let name = CFString::from_static_str(attribute);
        match ffi::copy(element, &name) {
            Ok(value) => value,
            Err(status) => {
                self.failed(status, attribute);
                None
            }
        }
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
    pub(super) fn batch(&mut self, element: &AXUIElement) -> Option<Batch> {
        if !self.open() {
            return Some(Batch::unread());
        }

        match ffi::copy_many(element, &self.names) {
            Ok(values) => self.received(AXError::Success, values.as_deref()),
            Err(status) => self.received(status, None),
        }
    }

    /// The batch a request returned with `status` and `values`. A request
    /// that could not complete fails every attribute. Any other failed
    /// request (some apps do not take one for many attributes at all) is
    /// recorded once and the element is read one attribute at a time: None.
    /// A missing reply, or one with another number of values, is a failure
    /// that leaves the scan incomplete.
    pub(super) fn received(
        &mut self,
        status: AXError,
        values: Option<&CFArray<CFType>>,
    ) -> Option<Batch> {
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
    pub(super) fn stalled(&mut self, batch: Option<&Batch>) -> bool {
        let stalled = batch.is_some_and(|batch| batch.stalled);
        if stalled {
            self.stop(Stop::Unread);
        }

        stalled
    }

    pub(super) fn text(&mut self, from: Source, name: &'static str) -> Option<String> {
        let value = self.attribute(from, name)?;
        let text = value.downcast_ref::<CFString>().and_then(bounded_string);
        if name == "AXSubrole" && text.is_none() {
            self.failed(AXError::Failure, name);
        }
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

    pub(super) fn flag(&mut self, from: Source, name: &'static str) -> Option<bool> {
        let value = self.attribute(from, name)?;
        value.downcast_ref::<CFBoolean>().map(CFBoolean::as_bool)
    }

    /// The element's value as text: strings bounded, numbers and booleans
    /// rendered. A text the batch counts as longer than the limit is read
    /// only from its start (see [`Batch::long_text`]). Nothing is asked of
    /// an element whose batch stalled, or whose value may be secure.
    pub(super) fn value(
        &mut self,
        element: &AXUIElement,
        batch: Option<&Batch>,
        role: Option<&str>,
        subrole: Option<&str>,
    ) -> Option<String> {
        if self.stalled(batch)
            || role.is_none()
            || [role, subrole].contains(&Some("AXSecureTextField"))
        {
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

        let parameter = ffi::range(prefix_range(characters)).ok_or(None)?;
        let name = CFString::from_static_str("AXStringForRange");
        let value = match ffi::copy_for(element, &name, &parameter) {
            Ok(value) => value.ok_or(None)?,
            Err(status) => {
                // An element that cannot read by range just has its value
                // read whole; that says nothing went wrong.
                if status != AXError::ParameterizedAttributeUnsupported {
                    self.failed(status, "AXStringForRange");
                }
                return Err(Some(status));
            }
        };
        prefix_text(&value).ok_or(None)
    }

    pub(super) fn frame(&mut self, from: Source) -> Option<Frame> {
        let position = self.attribute(from, "AXPosition")?;
        let size = self.attribute(from, "AXSize")?;
        let (position, size) = (
            position.downcast_ref::<AXValue>()?,
            size.downcast_ref::<AXValue>()?,
        );
        let point = ffi::point(position)?;
        let extent = ffi::size(size)?;

        Some(Frame {
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
    pub(super) fn children(
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

        let values = match ffi::copy_values(element, &name, wanted) {
            Ok(Some(values)) => values,
            Ok(None) => {
                self.failed(AXError::Success, attribute);
                self.stop(Stop::Unread);
                return Vec::new();
            }
            Err(status) => {
                self.failed(status, attribute);
                self.stop(Stop::Unread);
                return Vec::new();
            }
        };

        // Each value is checked to be an AXUIElement before use.
        values
            .iter()
            .filter_map(|value| value.downcast::<AXUIElement>().ok())
            .collect()
    }

    /// `windows`, an app's windows, without any that is `app` itself. An
    /// app hands back its own element for every window it will not show an
    /// Accessibility client (AppKit does so while the Mac's screen is
    /// locked); a withheld window might hold an unseen match, so the scan
    /// is incomplete rather than empty.
    pub(super) fn shown(
        &mut self,
        app: &AXUIElement,
        windows: Vec<CFRetained<AXUIElement>>,
    ) -> Vec<CFRetained<AXUIElement>> {
        let listed = windows.len();
        let shown: Vec<_> = windows
            .into_iter()
            .filter(|window| **window != *app)
            .collect();
        if shown.len() < listed {
            self.stop(Stop::Withheld);
        }

        shown
    }

    /// Whether an array attribute holds anything, from its count alone:
    /// nothing is copied.
    pub(super) fn has_children(&mut self, element: &AXUIElement, attribute: &'static str) -> bool {
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
        match ffi::count(element, &name) {
            Ok(count) => count,
            Err(status) => {
                self.failed(status, attribute);
                if !ABSENT.contains(&status) {
                    self.stop(Stop::Unread);
                }

                0
            }
        }
    }
}
