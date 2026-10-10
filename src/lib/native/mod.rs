//! macOS boundary executed *inside* the helper login: the read-only session
//! identity probe, plus the input service and its event engine.
use crate::error::{Error, Result};
use serde_json::Value;

#[cfg(target_os = "macos")]
pub fn session_info() -> Result<Value> {
    use serde_json::json;
    use std::ffi::{c_char, c_void};
    type CfRef = *const c_void;
    #[repr(C)]
    struct Point {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    struct Size {
        width: f64,
        height: f64,
    }
    #[repr(C)]
    struct Rect {
        origin: Point,
        size: Size,
    }
    #[link(name = "Security", kind = "framework")]
    unsafe extern "C" {
        fn SessionGetInfo(session: u32, result: *mut u32, attributes: *mut u32) -> i32;
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGSessionCopyCurrentDictionary() -> CfRef;
        fn CGMainDisplayID() -> u32;
        fn CGDisplayBounds(display: u32) -> Rect;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringCreateWithCString(allocator: CfRef, text: *const c_char, encoding: u32)
        -> CfRef;
        fn CFDictionaryGetValue(dictionary: CfRef, key: CfRef) -> CfRef;
        fn CFGetTypeID(value: CfRef) -> usize;
        fn CFBooleanGetTypeID() -> usize;
        fn CFBooleanGetValue(value: CfRef) -> bool;
        fn CFRelease(value: CfRef);
    }
    struct OwnedCf(CfRef);
    impl Drop for OwnedCf {
        fn drop(&mut self) {
            // SAFETY: OwnedCf only stores non-null references returned by Create/Copy.
            unsafe { CFRelease(self.0) };
        }
    }
    // SAFETY: valid output pointers, documented caller-security-session sentinel.
    let mut security_id = 0;
    let mut attributes = 0;
    let status = unsafe { SessionGetInfo(u32::MAX, &mut security_id, &mut attributes) };
    if status != 0 {
        return Err(Error::new(
            "identity",
            format!("SessionGetInfo failed: {status}"),
        ));
    }
    // SAFETY: no arguments; Copy transfers one reference to the caller.
    let dictionary = unsafe { CGSessionCopyCurrentDictionary() };
    if dictionary.is_null() {
        return Err(Error::new("identity", "No graphical session dictionary"));
    }
    let dictionary = OwnedCf(dictionary);
    let mut on_console = None;
    for name in [c"kCGSSessionOnConsoleKey", c"kCGSessionOnConsoleKey"] {
        // SAFETY: static NUL-terminated UTF-8 key, default allocator, valid dictionary.
        let key = unsafe { CFStringCreateWithCString(std::ptr::null(), name.as_ptr(), 0x08000100) };
        if key.is_null() {
            continue;
        }
        let key = OwnedCf(key);
        let value = unsafe { CFDictionaryGetValue(dictionary.0, key.0) };
        // SAFETY: borrowed value lives with dictionary; type checked before reading.
        if !value.is_null() && unsafe { CFGetTypeID(value) == CFBooleanGetTypeID() } {
            on_console = Some(unsafe { CFBooleanGetValue(value) });
            break;
        }
    }
    let on_console =
        on_console.ok_or_else(|| Error::new("identity", "Missing console-session state"))?;
    // SAFETY: functions have no pointer inputs or mutable shared Rust state.
    let display_id = unsafe { CGMainDisplayID() };
    let bounds = unsafe { CGDisplayBounds(display_id) };
    let uid = unsafe { libc::getuid() };
    // SAFETY: getppid has no pointer arguments or preconditions.
    let parent_pid = unsafe { libc::getppid() };
    Ok(json!({
        "ok": true,
        "uid": uid,
        "parentPID": parent_pid,
        "securitySessionID": security_id,
        "hasGraphicAccess": attributes & 0x0010 != 0,
        "onConsole": on_console,
        "displayID": display_id,
        "width": bounds.size.width,
        "height": bounds.size.height,
    }))
}

#[cfg(not(target_os = "macos"))]
pub fn session_info() -> Result<Value> {
    Err(Error::new("platform", "Session identity requires macOS"))
}

#[cfg(target_os = "macos")]
pub mod apps;
#[cfg(target_os = "macos")]
pub mod ax;
#[cfg(target_os = "macos")]
pub mod capture;
#[cfg(target_os = "macos")]
pub mod cf;
#[cfg(target_os = "macos")]
pub(crate) mod endpoint;
#[cfg(target_os = "macos")]
mod events;
#[cfg(target_os = "macos")]
mod journal;
#[cfg(target_os = "macos")]
mod lease;
#[cfg(target_os = "macos")]
pub mod service;
#[cfg(target_os = "macos")]
pub mod windows;
