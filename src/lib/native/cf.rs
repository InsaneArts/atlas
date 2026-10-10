//! The CoreFoundation pieces the native code shares: typed access to the
//! dictionaries CoreGraphics returns, and (until the accessibility code moves
//! to typed bindings) an owned raw reference.
pub use objc2_core_foundation::{
    CFArray, CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType, Type,
};
use std::ffi::{CStr, c_char, c_void};

pub type CfRef = *const c_void;

const UTF8: u32 = 0x0800_0100;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: CfRef);
    fn CFStringCreateWithCString(allocator: CfRef, text: *const c_char, encoding: u32) -> CfRef;
}

/// An owned CoreFoundation reference, released once. Never wraps null.
#[derive(Debug)]
pub struct Owned(CfRef);

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: only non-null references from Create/Copy/Retain are wrapped.
        unsafe { CFRelease(self.0) };
    }
}

impl Owned {
    /// Take ownership of a reference; None for null.
    ///
    /// # Safety
    /// `value` must be null or a live CoreFoundation reference the caller
    /// owns (from a Create or Copy function, or retained) and hands over: it
    /// is released when the returned value drops.
    pub unsafe fn adopt(value: CfRef) -> Option<Self> {
        (!value.is_null()).then(|| Self(value))
    }
    /// The reference, borrowed for as long as this value lives.
    pub fn get(&self) -> CfRef {
        self.0
    }
}

/// A CFString from static or NUL-terminated text.
pub fn string(text: &CStr) -> Option<Owned> {
    // SAFETY: NUL-terminated text and the default allocator; Create returns
    // an owned string or null.
    unsafe {
        Owned::adopt(CFStringCreateWithCString(
            std::ptr::null(),
            text.as_ptr(),
            UTF8,
        ))
    }
}

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
    fn null_is_never_owned_and_strings_are() {
        // SAFETY: null is never adopted.
        assert!(unsafe { Owned::adopt(std::ptr::null()) }.is_none());
        assert!(string(c"understudy").is_some());
    }

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
