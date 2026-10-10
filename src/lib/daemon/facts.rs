//! The `hello` reply: who the daemon runs as, in which session, on which
//! display, with which permissions.
// FFI boundary: Accessibility trust queries.
#![allow(unsafe_code)]
use crate::native::{
    capture,
    cf::{self, CFBoolean, CFDictionary, CFRetained, CFString},
};
use objc2_application_services::{AXIsProcessTrusted, AXIsProcessTrustedWithOptions};
use objc2_core_graphics::{
    CGDisplayBounds, CGDisplayCopyDisplayMode, CGDisplayMode, CGMainDisplayID,
    CGSessionCopyCurrentDictionary,
};
use serde_json::{Value, json};

/// Whether this process may use Accessibility.
fn accessibility() -> bool {
    // SAFETY: no arguments; only reads this process's consent.
    unsafe { AXIsProcessTrusted() }
}

/// Ask macOS for whatever consent the daemon still lacks — Screen Recording
/// and Accessibility — so it is listed in System Settings with a prompt in
/// the helper's session. Nothing is granted here; the person decides.
pub fn request_consent() {
    if !capture::permitted() {
        capture::request();
    }
    if accessibility() {
        return;
    }
    let key = CFString::from_static_str("AXTrustedCheckOptionPrompt");
    let options =
        CFDictionary::<CFString, CFBoolean>::from_slices(&[&key], &[CFBoolean::new(true)]);
    // SAFETY: the options dictionary holds the documented string key with a
    // boolean value.
    unsafe { AXIsProcessTrustedWithOptions(Some(options.as_opaque())) };
}

/// This process's account: name, home and UID.
fn user() -> Value {
    let uid = nix::unistd::getuid().as_raw();
    match crate::setup::account_with_uid(uid) {
        Ok(Some(account)) => json!({"uid": uid, "name": account.name, "home": account.home}),
        _ => json!({"uid": uid, "name": null, "home": null}),
    }
}

/// The window server's record of this graphical session.
fn session_record() -> Option<CFRetained<cf::Record>> {
    CGSessionCopyCurrentDictionary().map(cf::record)
}

/// Whether this graphical session is on the console now, if known.
fn console_state() -> Option<bool> {
    let record = session_record()?;
    cf::boolean(&record, "kCGSSessionOnConsoleKey")
        .or_else(|| cf::boolean(&record, "kCGSessionOnConsoleKey"))
}

/// Who owns this graphical session and whether their login has finished,
/// from the window server's own session record.
pub fn session_owner() -> Option<(i64, bool)> {
    let record = session_record()?;
    let uid = cf::integer(&record, "kCGSSessionUserIDKey")?;
    let done = cf::boolean(&record, "kCGSessionLoginDoneKey")?;
    Some((uid, done))
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
    let bounds = CGDisplayBounds(CGMainDisplayID());
    (
        bounds.origin.x,
        bounds.origin.y,
        bounds.size.width,
        bounds.size.height,
    )
}

/// The main display in points, and its backing pixels per point.
fn display() -> Value {
    let display = CGMainDisplayID();
    let bounds = CGDisplayBounds(display);
    let mode = CGDisplayCopyDisplayMode(display);
    let (pixels, points) = (
        CGDisplayMode::pixel_width(mode.as_deref()),
        CGDisplayMode::width(mode.as_deref()),
    );
    let scale = if points > 0 {
        pixels as f64 / points as f64
    } else {
        1.0
    };
    json!({"width": bounds.size.width, "height": bounds.size.height, "scale": scale})
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
    json!({
        "ok": true,
        "op": "hello",
        "user": user(),
        "permissions": {"accessibility": accessibility(), "screenCapture": capture::permitted()},
        "session": session(manager),
        "display": display(),
        "daemon": {"protocol": 1, "pid": std::process::id(), "version": env!("CARGO_PKG_VERSION")},
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The prompt option is passed by name; it is the exported constant.
    #[test]
    #[allow(unsafe_code)]
    fn the_prompt_option_is_the_framework_constant() {
        // SAFETY: reading an immutable framework constant in a test.
        let key = unsafe { objc2_application_services::kAXTrustedCheckOptionPrompt };
        assert_eq!(key.to_string(), "AXTrustedCheckOptionPrompt");
    }

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
