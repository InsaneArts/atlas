//! The `hello` reply: who the daemon runs as, in which session, on which
//! display, with which permissions.
use crate::native::{
    capture,
    cf::{self, CFBoolean, CFDictionary, CFRetained, CFString},
};
use objc2_application_services::{AXIsProcessTrusted, AXIsProcessTrustedWithOptions};
use objc2_core_graphics::{
    CGDisplayBounds, CGDisplayCopyDisplayMode, CGDisplayMode, CGMainDisplayID,
};
use serde_json::{Value, json};
use std::time::Duration;

/// Whether this process may use Accessibility.
#[allow(unsafe_code)] // FFI: Accessibility trust query.
fn accessibility() -> bool {
    // SAFETY: no arguments; only reads this process's consent.
    unsafe { AXIsProcessTrusted() }
}

/// Ask for Accessibility with the system prompt, unless it is allowed.
#[allow(unsafe_code)] // FFI: the consent prompt takes an options dictionary.
fn request_accessibility() {
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

/// Ask for missing consent whenever the helper comes on screen (or starts
/// while on screen), never in the background, where prompts are lost.
/// macOS shows one prompt at a time and the Screen Recording prompt only
/// once per app, so the requests come one by one: Accessibility first,
/// waiting until it is on (macOS reports that at once), then Screen
/// Recording, with its System Settings page opened so its switch is there
/// even when macOS shows no prompt. Nothing is granted here; the person
/// decides, in the normal way.
pub async fn request_consent_on_arrival() {
    let shown = || console_state() == Some(true);
    let mut present = false;
    loop {
        let now = shown();
        if now && !present {
            ask(&shown).await;
        }
        // Leaving while being asked counts as never having arrived.
        present = now && shown();
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// One arrival's requests, given up as soon as the helper leaves the screen.
/// When the person has turned on what was asked for, Atlas says so here, so
/// they know to switch back.
async fn ask(shown: &impl Fn() -> bool) {
    // Let the desktop settle after the switch, and let macOS finish a new
    // account's setup screens: a prompt raised behind them is never seen.
    tokio::time::sleep(Duration::from_secs(2)).await;
    while shown() && setting_up() {
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    if !shown() {
        return;
    }
    // Asked on an earlier visit, or before macOS's Quit & Reopen restarted
    // this process, and everything is on now: say so.
    if accessibility() && capture::permitted() {
        if asked(false) {
            done_notice();
        }
        return;
    }
    asked(true);
    if !accessibility() {
        request_accessibility();
        // As long as the person stays, however long they take. Should the
        // prompt not appear, its System Settings page opens instead.
        let mut second = 0u32;
        while !accessibility() {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if !shown() {
                return;
            }
            second += 1;
            if second == 15 {
                open_settings("Privacy_Accessibility");
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    if shown() && !capture::permitted() {
        capture::request();
        tokio::time::sleep(Duration::from_secs(1)).await;
        if !shown() {
            return;
        }
        open_settings("Privacy_ScreenCapture");
        // This process sees a new Screen Recording grant only after it
        // restarts; a fresh process sees it at once.
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            if !shown() {
                return;
            }
            if screen_recording_now() {
                break;
            }
        }
    }
    if shown() {
        asked(false);
        done_notice();
    }
}

/// Whether Screen Recording is allowed now, as a fresh Atlas process sees it.
fn screen_recording_now() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|executable| {
            std::process::Command::new(executable)
                .arg("permissions-info")
                .stdin(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .output()
                .ok()
        })
        .and_then(|output| serde_json::from_slice::<Value>(&output.stdout).ok())
        .is_some_and(|reply| reply["screenCapture"] == true)
}

/// This process's permissions now (`atlas permissions-info`).
pub fn permissions_now() -> Value {
    json!({
        "ok": true,
        "screenCapture": capture::permitted(),
        "accessibility": accessibility(),
    })
}

/// Remember (`true`) that Atlas asked the person for a permission, or forget
/// it (`false`), returning whether it was remembered. It outlives this
/// process, which macOS's Quit & Reopen restarts mid-way.
fn asked(remember: bool) -> bool {
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        return false;
    };
    let path = home.join(".atlas/consent-asked");
    let was = path.exists();
    if remember {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, b"asked\n");
    } else {
        let _ = std::fs::remove_file(&path);
    }
    was
}

/// Tell the person, in the helper, that it is set up and they can go back.
fn done_notice() {
    let Ok(companion) = crate::menubar::executable() else {
        return;
    };
    let _ = std::process::Command::new(companion)
        .args([
            "--notice",
            "Atlas is set up here",
            "Screen Recording and Accessibility are on. Switch back to your own account; \
             Atlas carries on there by itself.",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// Whether macOS's setup screens for a new account are open in this session.
fn setting_up() -> bool {
    let uid = nix::unistd::getuid().as_raw().to_string();
    std::process::Command::new("/bin/ps")
        .args(["-axo", "uid=,comm="])
        .output()
        .is_ok_and(|output| {
            String::from_utf8_lossy(&output.stdout).lines().any(|line| {
                let line = line.trim_start();
                line.strip_prefix(&uid).is_some_and(|rest| {
                    rest.starts_with(char::is_whitespace)
                        && rest
                            .trim()
                            .ends_with("/Setup Assistant.app/Contents/MacOS/Setup Assistant")
                })
            })
        })
}

/// Typing in the helper arrives exactly as sent: macOS's autocorrect,
/// automatic capitalisation, smart punctuation and predictions are turned
/// off in the helper's own preferences (apps read them when they start).
/// The person's account is never touched.
pub fn type_exactly() {
    for key in [
        "NSAutomaticSpellingCorrectionEnabled",
        "NSAutomaticCapitalizationEnabled",
        "NSAutomaticPeriodSubstitutionEnabled",
        "NSAutomaticDashSubstitutionEnabled",
        "NSAutomaticQuoteSubstitutionEnabled",
        "NSAutomaticTextReplacementEnabled",
        "NSAutomaticTextCompletionEnabled",
        "NSAutomaticInlinePredictionEnabled",
        "WebAutomaticSpellingCorrectionEnabled",
    ] {
        let _ = std::process::Command::new("/usr/bin/defaults")
            .args(["write", "-g", key, "-bool", "false"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

/// Open a Privacy & Security page of System Settings in this session.
fn open_settings(page: &str) {
    let _ = std::process::Command::new("/usr/bin/open")
        .arg(format!(
            "x-apple.systempreferences:com.apple.preference.security?{page}"
        ))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
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
    cf::session()
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
        "permissions": {
            "accessibility": accessibility(),
            "postEvents": objc2_core_graphics::CGPreflightPostEventAccess(),
            "screenCapture": capture::permitted(),
        },
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
