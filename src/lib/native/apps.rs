//! Applications running in the helper login, as the helper daemon reports
//! them: name, bundle identifier, process and its executable, when it was
//! launched, and whether each is frontmost, hidden or a background agent.
use objc2_app_kit::{NSApplicationActivationPolicy, NSRunningApplication, NSWorkspace};
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
                    "launchedAt": launched_at(&app),
                })
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
            assert!(app["launchedAt"].is_null() || app["launchedAt"].is_u64());
        }
    }

    #[test]
    fn a_bundle_id_lookup_uses_the_requested_process() {
        assert_eq!(
            bundle_id_with(812, |pid| {
                assert_eq!(pid, 812);
                Some("com.example.Probe".to_owned())
            })
            .as_deref(),
            Some("com.example.Probe")
        );
    }

    #[test]
    fn a_bundle_id_lookup_can_find_no_app_or_bundle() {
        assert_eq!(
            bundle_id_with(812, |pid| {
                assert_eq!(pid, 812);
                None
            }),
            None
        );
    }

    #[test]
    fn invalid_process_ids_never_reach_the_bundle_id_lookup() {
        for pid in [0, -1, i64::MIN, i64::MAX] {
            assert_eq!(bundle_id_with(pid, |_| panic!("looked up {pid}")), None);
        }
    }
}
