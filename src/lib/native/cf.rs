//! The CoreFoundation pieces the native code shares: an owned reference that
//! is released exactly once, and string and dictionary access.
use std::ffi::{CStr, c_char, c_void};

pub type CfRef = *const c_void;

const UTF8: u32 = 0x0800_0100;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: CfRef);
    fn CFStringCreateWithCString(allocator: CfRef, text: *const c_char, encoding: u32) -> CfRef;
    fn CFDictionaryGetValue(dictionary: CfRef, key: CfRef) -> CfRef;
    fn CFGetTypeID(value: CfRef) -> usize;
    fn CFBooleanGetTypeID() -> usize;
    fn CFBooleanGetValue(value: CfRef) -> bool;
    fn CFNumberGetTypeID() -> usize;
    fn CFNumberGetValue(number: CfRef, kind: isize, value: *mut c_void) -> bool;
    fn CFDataGetLength(data: CfRef) -> isize;
    fn CFDataGetBytePtr(data: CfRef) -> *const u8;
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

/// A dictionary's value for `key`, borrowed from the dictionary.
///
/// # Safety
/// `dictionary` must be a live CFDictionary.
unsafe fn value(dictionary: CfRef, key: &CStr) -> Option<CfRef> {
    let key = string(key)?;
    // SAFETY: per contract; the returned value is borrowed from the dictionary.
    let value = unsafe { CFDictionaryGetValue(dictionary, key.get()) };
    (!value.is_null()).then_some(value)
}

/// A boolean dictionary value, if present and boolean.
///
/// # Safety
/// `dictionary` must be a live CFDictionary.
pub unsafe fn bool_value(dictionary: CfRef, key: &CStr) -> Option<bool> {
    // SAFETY: per contract; the type is checked before reading.
    unsafe {
        let value = value(dictionary, key)?;
        (CFGetTypeID(value) == CFBooleanGetTypeID()).then(|| CFBooleanGetValue(value))
    }
}

/// An integer dictionary value, if present and numeric.
///
/// # Safety
/// `dictionary` must be a live CFDictionary.
pub unsafe fn i64_value(dictionary: CfRef, key: &CStr) -> Option<i64> {
    const SINT64: isize = 4;
    // SAFETY: per contract; the type is checked and the output sized for SInt64.
    unsafe {
        let value = value(dictionary, key)?;
        let mut number = 0i64;
        (CFGetTypeID(value) == CFNumberGetTypeID()
            && CFNumberGetValue(value, SINT64, (&mut number as *mut i64).cast()))
        .then_some(number)
    }
}

/// The bytes of a CFData.
///
/// # Safety
/// `data` must be a live CFData.
pub unsafe fn bytes(data: CfRef) -> Vec<u8> {
    // SAFETY: per contract; the pointer is valid for the reported length.
    unsafe {
        let length = usize::try_from(CFDataGetLength(data)).unwrap_or(0);
        if length == 0 {
            return Vec::new();
        }
        std::slice::from_raw_parts(CFDataGetBytePtr(data), length).to_vec()
    }
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
}
