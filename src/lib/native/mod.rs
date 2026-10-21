//! macOS boundary executed *inside* the helper login: the read-only session
//! identity probe, plus the input service and its event engine.
use crate::error::{Error, Result};
use serde_json::Value;

// FFI: SessionGetInfo reports through out-pointers.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
pub fn session_info() -> Result<Value> {
    use objc2_core_graphics::{CGDisplayBounds, CGMainDisplayID};
    use objc2_security::{SessionAttributeBits, SessionGetInfo, callerSecuritySession};
    use serde_json::json;

    let mut security_id = 0;
    let mut attributes = SessionAttributeBits(0);

    // SAFETY: both outputs are valid for writes; the sentinel asks about this
    // process's own security session.
    let status =
        unsafe { SessionGetInfo(callerSecuritySession, &mut security_id, &mut attributes) };

    if status != 0 {
        return Err(Error::new(
            "identity",
            format!("SessionGetInfo failed: {status}"),
        ));
    }

    let record =
        cf::session().ok_or_else(|| Error::new("identity", "No graphical session dictionary"))?;
    let on_console = cf::boolean(&record, "kCGSSessionOnConsoleKey")
        .or_else(|| cf::boolean(&record, "kCGSessionOnConsoleKey"))
        .ok_or_else(|| Error::new("identity", "Missing console-session state"))?;
    let display_id = CGMainDisplayID();
    let bounds = CGDisplayBounds(display_id);

    Ok(json!({
        "ok": true,
        "uid": nix::unistd::getuid().as_raw(),
        "parentPID": nix::unistd::getppid().as_raw(),
        "securitySessionID": security_id,
        "hasGraphicAccess": attributes.contains(SessionAttributeBits::sessionHasGraphicAccess),
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

/// Whether this process may use Accessibility (macOS has let the person
/// allow it for Atlas).
#[cfg(target_os = "macos")]
#[allow(unsafe_code)] // FFI: Accessibility trust query.
pub fn accessibility_trusted() -> bool {
    // SAFETY: no arguments; only reads this process's consent.
    unsafe { objc2_application_services::AXIsProcessTrusted() }
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
pub mod icons;
#[cfg(target_os = "macos")]
mod journal;
#[cfg(target_os = "macos")]
mod lease;
#[cfg(target_os = "macos")]
pub mod service;
#[cfg(target_os = "macos")]
pub mod windows;
