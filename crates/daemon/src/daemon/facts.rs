//! The `hello` reply: who the daemon runs as, in which session, on which
//! display, with which permissions.
use atlas_core::protocol::Census;
use atlas_native::native::{accessibility_trusted as accessibility, capture, cf};
use objc2_core_graphics::{
    CGDisplayBounds, CGDisplayCopyDisplayMode, CGDisplayMode, CGMainDisplayID,
};
use serde_json::{Value, json};

/// This process's permissions now (`atlas permissions-info`).
pub fn permissions_now() -> Value {
    json!({
        "ok": true,
        "screenCapture": capture::permitted(),
        "accessibility": accessibility(),
    })
}

/// This process's account: name, home and UID.
fn user() -> Value {
    let uid = nix::unistd::getuid().as_raw();
    match atlas_core::account::account_with_uid(uid) {
        Ok(Some(account)) => json!({"uid": uid, "name": account.name, "home": account.home}),
        _ => json!({"uid": uid, "name": null, "home": null}),
    }
}

/// Whether this graphical session is on the console now, if known, from
/// the window server's own session record.
pub(super) fn console_state() -> Option<bool> {
    let record = cf::session()?;
    cf::boolean(&record, "kCGSSessionOnConsoleKey")
        .or_else(|| cf::boolean(&record, "kCGSessionOnConsoleKey"))
}

/// Who owns this graphical session and whether their login has finished,
/// from the window server's own session record.
pub(super) fn session_owner() -> Option<(i64, bool)> {
    let record = cf::session()?;
    let uid = cf::integer(&record, "kCGSSessionUserIDKey")?;
    let done = cf::boolean(&record, "kCGSessionLoginDoneKey")?;
    Some((uid, done))
}

/// Whether the session is on the console. Unknown counts as on the console:
/// nothing acts unless the session is known to be hidden.
pub(super) fn on_console() -> bool {
    console_state() != Some(false)
}

/// The main display's bounds in points: x, y, width, height.
pub(super) fn display_bounds() -> (f64, f64, f64, f64) {
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
pub(super) fn manager_name() -> String {
    std::process::Command::new("/bin/launchctl")
        .arg("managername")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Unknown".into())
}

/// The daemon's hello: who and where the helper is, what it may do, and,
/// when it keeps background jobs, how many may still run, are launching,
/// or have records it cannot read, and whether it could not read them all
/// (`jobs`).
pub(super) fn hello(manager: &str, jobs: Option<Census>) -> Value {
    use atlas_core::protocol::{JOBS, READ, SESSION_PIN};

    let mut capabilities = vec![SESSION_PIN, READ];
    if jobs.is_some() {
        capabilities.push(JOBS);
    }
    let mut hello = json!({
        "ok": true,
        "op": "hello",
        "user": user(),
        "permissions": {
            "accessibility": accessibility(),
            "postEvents": objc2_core_graphics::CGPreflightPostEventAccess(),
            "screenCapture": capture::permitted(),
        },
        "session": {"managerName": manager, "onConsole": console_state()},
        "display": display(),
        "daemon": {"protocol": 1, "pid": std::process::id(), "version": env!("CARGO_PKG_VERSION"),
            "capabilities": capabilities},
    });
    if let Some(jobs) = jobs {
        hello["daemon"]["jobs"] = json!(jobs);
    }
    hello
}

#[cfg(test)]
mod tests;
