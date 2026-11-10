//! Typed access to the dictionaries CoreGraphics returns. Records are made
//! only from the documented CoreGraphics collections fetched here (or from
//! typed CF values), and every element's type is checked at run time.
#![allow(
    unsafe_code,
    reason = "FFI: CoreGraphics collection types and session-local CF notifications"
)]
pub(crate) use objc2_core_foundation::{
    CFArray, CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType,
};
use objc2_core_graphics::{
    CGSessionCopyCurrentDictionary, CGWindowListCopyWindowInfo, CGWindowListOption,
};

/// Deliver a distributed preference notification only in this login session.
pub fn notify_preferences(name: &str) -> bool {
    let Some(center) = objc2_core_foundation::CFNotificationCenter::distributed_center() else {
        return false;
    };
    let name = CFString::from_str(name);
    // SAFETY: a live notification name, no object or user info. Immediate
    // delivery does not include the option to post to all login sessions.
    unsafe { center.post_notification(Some(&name), std::ptr::null(), None, true) };
    true
}

/// A dictionary CoreGraphics returns: string keys, values of any CF type.
pub(crate) type Record = CFDictionary<CFString, CFType>;

/// View `dictionary` as a [`Record`].
///
/// # Safety
/// Every value in `dictionary` must be a CF object (as in any property list).
unsafe fn record(dictionary: CFRetained<CFDictionary>) -> CFRetained<Record> {
    // SAFETY: per contract; keys are only compared, values are CF objects.
    unsafe { CFRetained::cast_unchecked(dictionary) }
}

/// The window server's record of this graphical session.
pub fn session() -> Option<CFRetained<Record>> {
    // SAFETY: the session dictionary is a property list of CF values.
    CGSessionCopyCurrentDictionary().map(|dictionary| unsafe { record(dictionary) })
}

/// The window list's entries, front to back, each a window's record.
pub(crate) fn windows(options: CGWindowListOption) -> Option<Vec<CFRetained<Record>>> {
    let list = CGWindowListCopyWindowInfo(options, 0)?;
    // SAFETY: a CoreGraphics array holds CF objects; each is checked to be a
    // dictionary before it is read.
    let list = unsafe { CFRetained::cast_unchecked::<CFArray<CFType>>(list) };

    Some(
        list.iter()
            .filter_map(|entry| entry.downcast::<CFDictionary>().ok())
            // SAFETY: window dictionaries are property lists of CF values.
            .map(|dictionary| unsafe { record(dictionary) })
            .collect(),
    )
}

fn value(record: &Record, key: &'static str) -> Option<CFRetained<CFType>> {
    record.get(&CFString::from_static_str(key))
}

/// A boolean value, if present and boolean.
pub fn boolean(record: &Record, key: &'static str) -> Option<bool> {
    value(record, key)?
        .downcast_ref::<CFBoolean>()
        .map(CFBoolean::as_bool)
}

/// An integer value, if present and representable.
pub fn integer(record: &Record, key: &'static str) -> Option<i64> {
    value(record, key)?.downcast_ref::<CFNumber>()?.as_i64()
}

/// A floating-point value, if present and numeric.
pub(crate) fn float(record: &Record, key: &'static str) -> Option<f64> {
    value(record, key)?.downcast_ref::<CFNumber>()?.as_f64()
}

/// A string value, if present and a string.
pub(crate) fn text(record: &Record, key: &'static str) -> Option<String> {
    value(record, key)?
        .downcast_ref::<CFString>()
        .map(ToString::to_string)
}

/// A nested dictionary value, as a record.
pub(crate) fn nested(record: &Record, key: &'static str) -> Option<CFRetained<Record>> {
    let dictionary = value(record, key)?.downcast::<CFDictionary>().ok()?;
    // SAFETY: records hold property lists (from CoreGraphics, or typed CF
    // values), so a nested dictionary's values are CF objects too.
    Some(unsafe { self::record(dictionary) })
}

#[cfg(test)]
#[path = "cf_tests.rs"]
mod tests;
