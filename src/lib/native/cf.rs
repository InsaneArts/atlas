//! Typed access to the dictionaries CoreGraphics returns.
// FFI boundary: documented casts of CoreGraphics dictionaries and arrays.
#![allow(unsafe_code)]
pub use objc2_core_foundation::{
    CFArray, CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType, Type,
};
/// A dictionary CoreGraphics returns: string keys, values of any CF type.
pub type Record = CFDictionary<CFString, CFType>;

/// View a CoreGraphics session or window dictionary as a [`Record`].
pub fn record(dictionary: CFRetained<CFDictionary>) -> CFRetained<Record> {
    // SAFETY: only CoreGraphics session and window-list dictionaries come
    // here; their keys are documented CFStrings and their values CF objects.
    unsafe { CFRetained::cast_unchecked(dictionary) }
}

/// View the window list (an array of dictionaries) as records.
pub fn records(array: CFRetained<CFArray>) -> CFRetained<CFArray<Record>> {
    // SAFETY: CGWindowListCopyWindowInfo documents an array of window
    // dictionaries with CFString keys.
    unsafe { CFRetained::cast_unchecked(array) }
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
    let value = value(record, key)?;
    value
        .downcast_ref::<CFDictionary>()
        .map(|dictionary| self::record(dictionary.retain()))
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
        let record = record(dictionary.as_opaque().retain());
        assert_eq!(boolean(&record, "flag"), Some(true));
        assert_eq!(integer(&record, "count"), Some(42));
        assert_eq!(float(&record, "count"), Some(42.0));
        assert_eq!(text(&record, "name").as_deref(), Some("helper"));
        assert_eq!(boolean(&record, "count"), None, "wrong type");
        assert_eq!(integer(&record, "missing"), None);
    }
}
