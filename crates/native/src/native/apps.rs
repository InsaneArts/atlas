//! Applications running in the helper login, as the helper daemon reports
//! them: name, bundle identifier, process and its executable, when it was
//! launched, and whether each is frontmost, hidden or a background agent.
use objc2_app_kit::{NSApplicationActivationPolicy, NSRunningApplication, NSWorkspace};
use serde_json::{Value, json};

#[derive(serde::Serialize)]
pub struct Running {
    pub pid: i32,
    #[serde(flatten)]
    fields: Value,
}

fn policy(policy: NSApplicationActivationPolicy) -> &'static str {
    match policy {
        NSApplicationActivationPolicy::Regular => "regular",
        NSApplicationActivationPolicy::Accessory => "accessory",
        NSApplicationActivationPolicy::Prohibited => "prohibited",
        _ => "unknown",
    }
}

/// The running applications, in the order the system lists them, without
/// those that have no process (macOS may still list one that quit, its
/// process ID -1). Called on worker threads, so each call drains its own
/// autorelease pool.
pub fn running() -> Vec<Running> {
    objc2::rc::autoreleasepool(|_| {
        NSWorkspace::sharedWorkspace()
            .runningApplications()
            .iter()
            .filter(|app| app.processIdentifier() > 0)
            .map(|app| Running {
                pid: app.processIdentifier(),
                fields: json!({
                    "name": app.localizedName().map(|name| name.to_string()),
                    "bundleId": app.bundleIdentifier().map(|id| id.to_string()),
                    "executable": app
                        .executableURL()
                        .and_then(|url| url.path())
                        .map(|path| path.to_string()),
                    "active": app.isActive(),
                    "hidden": app.isHidden(),
                    "policy": policy(app.activationPolicy()),
                    "launchedAt": launched_at(&app),
                }),
            })
            .collect()
    })
}

/// The bundle identifier of the app running as process `pid`, when one
/// does and has one. Called on worker threads, so it drains its own
/// autorelease pool.
pub fn bundle_id(pid: i64) -> Option<String> {
    bundle_id_with(pid, |pid| {
        objc2::rc::autoreleasepool(|_| {
            NSRunningApplication::runningApplicationWithProcessIdentifier(pid)?
                .bundleIdentifier()
                .map(|id| id.to_string())
        })
    })
}

/// The name the app running as process `pid` shows, when one does. Called
/// on worker threads, so it drains its own autorelease pool.
pub fn name(pid: i64) -> Option<String> {
    bundle_id_with(pid, |pid| {
        objc2::rc::autoreleasepool(|_| {
            NSRunningApplication::runningApplicationWithProcessIdentifier(pid)?
                .localizedName()
                .map(|name| name.to_string())
        })
    })
}

fn bundle_id_with(pid: i64, lookup: impl FnOnce(libc::pid_t) -> Option<String>) -> Option<String> {
    let pid = libc::pid_t::try_from(pid).ok().filter(|pid| *pid > 0)?;
    lookup(pid)
}

/// When `app` was launched, in milliseconds since 1970; none for an app
/// LaunchServices did not launch.
fn launched_at(app: &NSRunningApplication) -> Option<u64> {
    let seconds = app.launchDate()?.timeIntervalSince1970();
    (seconds.is_finite() && seconds > 0.0).then_some((seconds * 1000.0) as u64)
}

#[cfg(test)]
mod tests;
