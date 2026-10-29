//! How much of an element's text is read: at most a fixed number of UTF-16
//! units, cut only between characters, and for a long text only its start.
use super::ffi;
use objc2_core_foundation::{CFIndex, CFRange, CFString, CFType};

/// Longest text reported for any one attribute, in UTF-16 units.
pub(super) const TEXT_LIMIT: CFIndex = 200;

/// Units read past `TEXT_LIMIT` from the start of a long text, so the cut
/// can fall between characters.
const PREFIX_SLACK: CFIndex = 32;

/// Roles whose value is the text they count, so a long one can be read
/// from its start. Any other element's value is read whole.
pub(super) const TEXT_ROLES: [&str; 5] = [
    "AXTextArea",
    "AXTextField",
    "AXComboBox",
    "AXSearchField",
    "AXStaticText",
];

/// At most the first `TEXT_LIMIT` UTF-16 units of a CFString, converted
/// without copying the rest and cut only between composed characters, so a
/// surrogate pair, emoji sequence or combining mark is never split. Returns
/// the text and whether it was cut.
pub(super) fn bounded_string(value: &CFString) -> Option<(String, bool)> {
    let full = value.length();
    let truncated = full > TEXT_LIMIT;
    let length = if truncated {
        // The index is inside the string: it is longer than it.
        ffi::composed_start(value, TEXT_LIMIT)?
    } else {
        full
    };

    let range = CFRange {
        location: 0,
        length,
    };
    let part = ffi::substring(value, range)?;

    Some((part.to_string(), truncated))
}

/// The range read from the start of a text of `characters` UTF-16 units:
/// past the limit by a little, so the cut lands where it would in the whole
/// text (where a character starts at the limit depends only on the units
/// before it), but never past the end.
pub(super) fn prefix_range(characters: CFIndex) -> CFRange {
    CFRange {
        location: 0,
        length: characters.min(TEXT_LIMIT + PREFIX_SLACK),
    }
}

/// The start of a long text, cut as [`bounded_string`] cuts it; None when
/// it is not text or not longer than the limit, which the plain value then
/// reports whole.
pub(super) fn prefix_text(value: &CFType) -> Option<String> {
    let (text, cut) = value.downcast_ref::<CFString>().and_then(bounded_string)?;
    cut.then_some(text)
}
