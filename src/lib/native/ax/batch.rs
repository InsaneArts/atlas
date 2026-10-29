//! An element's attributes asked for in one request: which ones, what each
//! slot of the reply holds, and where a single attribute is then read from.
use super::{
    ABSENT, ffi,
    text::{TEXT_LIMIT, TEXT_ROLES},
};
use objc2_application_services::{AXError, AXUIElement, AXValue, AXValueType};
use objc2_core_foundation::{CFArray, CFIndex, CFNull, CFNumber, CFRetained, CFType};

/// Attributes read together for each element, in the order their values
/// come back. The value is asked for on its own, so a long document is never
/// copied whole, and so are the children, which are counted before any is
/// copied.
pub(super) const BATCH: [&str; 12] = [
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

/// One attribute's answer in a batched read.
pub(super) enum Slot {
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
pub(super) fn slot(value: CFRetained<CFType>) -> Slot {
    if let Some(answer) = value.downcast_ref::<AXValue>()
        && ffi::kind(answer) == AXValueType::AXError
    {
        let code = ffi::error(answer).filter(|code| *code != AXError::Success);
        return Slot::Failed(code.unwrap_or(AXError::Failure));
    }

    if value.downcast_ref::<CFNull>().is_some() {
        Slot::Empty
    } else {
        Slot::Value(value)
    }
}

/// The attributes in `BATCH` of one element, read in one request.
pub(super) struct Batch {
    pub(super) slots: Vec<Slot>,
    /// The whole request could not complete: the element did not answer,
    /// and asking it anything more would wait as long.
    pub(super) stalled: bool,
}

impl Batch {
    pub(super) fn unread() -> Self {
        Self {
            slots: BATCH.iter().map(|_| Slot::Unread).collect(),
            stalled: false,
        }
    }

    /// Every attribute failed as the whole request did, with an error that
    /// leaves the scan incomplete.
    pub(super) fn failed(status: AXError) -> Self {
        Self {
            slots: BATCH.iter().map(|_| Slot::Failed(status)).collect(),
            stalled: true,
        }
    }

    /// One slot per attribute, or None when the reply holds another number
    /// of values.
    pub(super) fn from_values(values: &CFArray<CFType>) -> Option<Self> {
        (values.len() == BATCH.len()).then(|| Self {
            slots: values.iter().map(slot).collect(),
            stalled: false,
        })
    }

    /// The answer for `attribute`; None for one the batch does not ask for.
    pub(super) fn slot(&self, attribute: &str) -> Option<&Slot> {
        let at = BATCH.iter().position(|name| *name == attribute);
        debug_assert!(at.is_some(), "{attribute} is not read in a batch");
        self.slots.get(at?)
    }

    /// How many UTF-16 units the element's text holds, when it said. Never
    /// recorded as a failure: most elements hold no text.
    pub(super) fn characters(&self) -> Option<CFIndex> {
        match self.slot("AXNumberOfCharacters")? {
            Slot::Value(value) => value.downcast_ref::<CFNumber>()?.as_isize(),
            _ => None,
        }
    }

    /// How many units a text holds that is read only from its start: one
    /// longer than the limit, of an element whose role is a text role and
    /// that is surely not a password field: its subrole is another or
    /// absent, not one that failed to read.
    pub(super) fn long_text(&self, role: Option<&str>, subrole: Option<&str>) -> Option<CFIndex> {
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
pub(super) enum Source<'a> {
    Element(&'a AXUIElement),
    Batch(&'a Batch),
}

impl<'a> Source<'a> {
    /// The batch read from `element`, or the element itself when there is
    /// none to read from.
    pub(super) fn of(element: &'a AXUIElement, batch: Option<&'a Batch>) -> Self {
        batch.map_or(Source::Element(element), Source::Batch)
    }
}
