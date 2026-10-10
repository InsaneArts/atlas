//! The `hello` reply: who the daemon runs as, in which session, on which
//! display, with which permissions.
use crate::native::{
    capture,
    cf::{self, CfRef, Owned},
};
use serde_json::{Value, json};

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

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGSessionCopyCurrentDictionary() -> CfRef;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayBounds(display: u32) -> Rect;
    fn CGDisplayCopyDisplayMode(display: u32) -> CfRef;
    fn CGDisplayModeGetPixelWidth(mode: CfRef) -> usize;
    fn CGDisplayModeGetWidth(mode: CfRef) -> usize;
}
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CfRef) -> bool;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFDictionaryCreate(
        allocator: CfRef,
        keys: *const CfRef,
        values: *const CfRef,
        count: isize,
        key_callbacks: *const std::ffi::c_void,
        value_callbacks: *const std::ffi::c_void,
    ) -> CfRef;
    static kCFBooleanTrue: CfRef;
    static kCFTypeDictionaryKeyCallBacks: std::ffi::c_void;
    static kCFTypeDictionaryValueCallBacks: std::ffi::c_void;
}

/// Ask macOS for whatever consent the daemon still lacks — Screen Recording
/// and Accessibility — so it is listed in System Settings with a prompt in
/// the helper's session. Nothing is granted here; the person decides.
pub fn request_consent() {
    if !capture::permitted() {
        capture::request();
    }
    // SAFETY: AXIsProcessTrusted has no arguments; the options dictionary is
    // owned and holds one static key and value.
    unsafe {
        if AXIsProcessTrusted() {
            return;
        }
        let Some(key) = cf::string(c"AXTrustedCheckOptionPrompt") else {
            return;
        };
        let value = kCFBooleanTrue;
        if let Some(options) = Owned::adopt(CFDictionaryCreate(
            std::ptr::null(),
            &key.get(),
            &value,
            1,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        )) {
            AXIsProcessTrustedWithOptions(options.get());
        }
    }
}

/// This process's account: name, home and UID.
fn user() -> Value {
    let uid = nix::unistd::getuid().as_raw();
    match crate::setup::account_with_uid(uid) {
        Ok(Some(account)) => json!({"uid": uid, "name": account.name, "home": account.home}),
        _ => json!({"uid": uid, "name": null, "home": null}),
    }
}

/// Whether this graphical session is on the console now, if known.
fn console_state() -> Option<bool> {
    // SAFETY: Copy returns an owned dictionary or null; values are read
    // through type-checked helpers while it is alive.
    unsafe {
        Owned::adopt(CGSessionCopyCurrentDictionary()).and_then(|dictionary| {
            cf::bool_value(dictionary.get(), c"kCGSSessionOnConsoleKey")
                .or_else(|| cf::bool_value(dictionary.get(), c"kCGSessionOnConsoleKey"))
        })
    }
}

/// Who owns this graphical session and whether their login has finished,
/// from the window server's own session record.
pub fn session_owner() -> Option<(i64, bool)> {
    // SAFETY: Copy returns an owned dictionary or null; values are read
    // through type-checked helpers while it is alive.
    unsafe {
        let dictionary = Owned::adopt(CGSessionCopyCurrentDictionary())?;
        let uid = cf::i64_value(dictionary.get(), c"kCGSSessionUserIDKey")?;
        let done = cf::bool_value(dictionary.get(), c"kCGSessionLoginDoneKey")?;
        Some((uid, done))
    }
}

/// Whether the session is on the console. Unknown counts as on the console:
/// nothing acts unless the session is known to be hidden.
pub fn on_console() -> bool {
    console_state() != Some(false)
}

/// The graphical session: whether it is on the console, and launchd's name
/// for the session type (`Aqua` for a logged-in desktop).
fn session(manager: &str) -> Value {
    json!({"managerName": manager, "onConsole": console_state()})
}

/// The main display's bounds in points: x, y, width, height.
pub fn display_bounds() -> (f64, f64, f64, f64) {
    // SAFETY: no pointer arguments; the display ID comes from the system.
    let bounds = unsafe { CGDisplayBounds(CGMainDisplayID()) };
    (
        bounds.origin.x,
        bounds.origin.y,
        bounds.size.width,
        bounds.size.height,
    )
}

/// The main display in points, and its backing pixels per point.
fn display() -> Value {
    // SAFETY: the display ID comes from the system; the mode is owned.
    unsafe {
        let display = CGMainDisplayID();
        let bounds = CGDisplayBounds(display);
        let scale = Owned::adopt(CGDisplayCopyDisplayMode(display))
            .map(|mode| {
                let (pixels, points) = (
                    CGDisplayModeGetPixelWidth(mode.get()),
                    CGDisplayModeGetWidth(mode.get()),
                );
                if points > 0 {
                    pixels as f64 / points as f64
                } else {
                    1.0
                }
            })
            .unwrap_or(1.0);
        json!({"width": bounds.size.width, "height": bounds.size.height, "scale": scale})
    }
}

/// launchd's session type for this process (`launchctl managername`).
pub fn manager_name() -> String {
    std::process::Command::new("/bin/launchctl")
        .arg("managername")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Unknown".into())
}

/// The full `hello` reply.
pub fn hello(manager: &str) -> Value {
    // SAFETY: AXIsProcessTrusted has no arguments and reads this process's consent.
    let accessibility = unsafe { AXIsProcessTrusted() };
    json!({
        "ok": true,
        "op": "hello",
        "user": user(),
        "permissions": {"accessibility": accessibility, "screenCapture": capture::permitted()},
        "session": session(manager),
        "display": display(),
        "daemon": {"protocol": 1, "pid": std::process::id(), "version": env!("CARGO_PKG_VERSION")},
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_describes_this_process_in_its_session() {
        let reply = hello(&manager_name());
        assert_eq!(reply["user"]["uid"], nix::unistd::getuid().as_raw());
        assert!(reply["user"]["name"].is_string());
        assert!(
            reply["user"]["home"]
                .as_str()
                .is_some_and(|home| home.starts_with('/'))
        );
        assert_eq!(reply["daemon"]["pid"], std::process::id());
        assert_eq!(reply["daemon"]["protocol"], 1);
        assert!(reply["permissions"]["screenCapture"].is_boolean());
        assert!(
            reply["display"]["scale"]
                .as_f64()
                .is_some_and(|scale| scale >= 1.0)
        );
    }
}
