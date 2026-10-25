//! Applications running in the helper login, as the helper daemon reports
//! them: name, bundle identifier, process and its executable, and whether
//! each is frontmost, hidden or a background agent.
use objc2_app_kit::{NSApplicationActivationPolicy, NSWorkspace};
use serde_json::{Value, json};

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
pub fn running() -> Vec<Value> {
    objc2::rc::autoreleasepool(|_| {
        NSWorkspace::sharedWorkspace()
            .runningApplications()
            .iter()
            .filter(|app| app.processIdentifier() > 0)
            .map(|app| {
                json!({
                    "name": app.localizedName().map(|name| name.to_string()),
                    "bundleId": app.bundleIdentifier().map(|id| id.to_string()),
                    "pid": app.processIdentifier(),
                    "executable": app
                        .executableURL()
                        .and_then(|url| url.path())
                        .map(|path| path.to_string()),
                    "active": app.isActive(),
                    "hidden": app.isHidden(),
                    "policy": policy(app.activationPolicy()),
                })
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_session_lists_running_applications() {
        let apps = running();
        assert!(!apps.is_empty(), "a logged-in session always runs apps");

        for app in &apps {
            assert!(app["pid"].as_i64().is_some_and(|pid| pid > 0));
            assert!(
                ["regular", "accessory", "prohibited", "unknown"]
                    .contains(&app["policy"].as_str().unwrap())
            );
        }
    }
}
