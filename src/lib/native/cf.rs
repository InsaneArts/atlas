//! Typed access to the dictionaries CoreGraphics returns. Records are made
//! only from the documented CoreGraphics collections fetched here (or from
//! typed CF values), and every element's type is checked at run time.
#![allow(
    unsafe_code,
    reason = "FFI: casts resting on CoreGraphics' documented collection types"
)]
pub use objc2_core_foundation::{
    CFArray, CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType, Type,
};
use objc2_core_graphics::{
    CGSessionCopyCurrentDictionary, CGWindowListCopyWindowInfo, CGWindowListOption,
};

/// A dictionary CoreGraphics returns: string keys, values of any CF type.
pub type Record = CFDictionary<CFString, CFType>;

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
pub fn windows(options: CGWindowListOption) -> Option<Vec<CFRetained<Record>>> {
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
pub fn float(record: &Record, key: &'static str) -> Option<f64> {
    value(record, key)?.downcast_ref::<CFNumber>()?.as_f64()
}

/// A string value, if present and a string.
pub fn text(record: &Record, key: &'static str) -> Option<String> {
    value(record, key)?
        .downcast_ref::<CFString>()
        .map(ToString::to_string)
}

/// A nested dictionary value, as a record.
pub fn nested(record: &Record, key: &'static str) -> Option<CFRetained<Record>> {
    let dictionary = value(record, key)?.downcast::<CFDictionary>().ok()?;
    // SAFETY: records hold property lists (from CoreGraphics, or typed CF
    // values), so a nested dictionary's values are CF objects too.
    Some(unsafe { self::record(dictionary) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_read_typed_values_and_refuse_other_types() {
        let keys = [
            CFString::from_static_str("flag"),
            CFString::from_static_str("count"),
            CFString::from_static_str("name"),
        ];

        let flag = CFBoolean::new(true);
        let count = CFNumber::new_i64(42);
        let name = CFString::from_static_str("helper");
        let values: [&CFType; 3] = [flag.as_ref(), count.as_ref(), name.as_ref()];
        let dictionary = CFDictionary::<CFString, CFType>::from_slices(
            &keys.iter().map(|key| &**key).collect::<Vec<_>>(),
            &values,
        );
        // SAFETY: built from typed CF values.
        let record = unsafe { record(dictionary.as_opaque().retain()) };

        assert_eq!(boolean(&record, "flag"), Some(true));
        assert_eq!(integer(&record, "count"), Some(42));
        assert_eq!(float(&record, "count"), Some(42.0));
        assert_eq!(text(&record, "name").as_deref(), Some("helper"));
        assert_eq!(boolean(&record, "count"), None, "wrong type");
        assert_eq!(integer(&record, "missing"), None);
    }
}
