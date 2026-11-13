//! What doctor reports of a helper's native dialogs and web content, and the
//! summary programs read. The user desktop's dated check of native Save/Open
//! panels, and its record, are in [`crate::user::capability`].
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum NativeState {
    Unknown,
    Stale,
    Passed,
    Failed,
}

/// Default location of the compiled fixture (tests/live/DesktopFixture.swift).
pub const FIXTURE: &str = "/Users/Shared/AtlasTests/DesktopFixture";

/// Static guidance, not a test of the page or remote connection.
pub(crate) const WEB_CONTENT: &str = "Safari/WebKit can stay blank without authenticated Screen Sharing. \
                           Use Chrome/Firefox for web pages; do not retry a blank page.";

pub struct Doctor {
    pub diagnosis: atlas_core::session::Diagnosis,
    pub(crate) native: NativeState,
    pub(crate) absence: Option<atlas_core::session::Absence>,
}

/// `report` with what is known of native dialogs (`native`, in `state`) and
/// of web content, and the warning each comes to.
pub(crate) fn with_capabilities(mut report: Value, state: NativeState, native: Value) -> Value {
    report["capabilities"]["nativeDialogs"] = native;

    report["capabilities"]["webContent"] = json!({
        "state": "unknown",
        "reason": WEB_CONTENT,
    });

    let mut warnings = report["warnings"].as_array().cloned().unwrap_or_default();
    warnings.push(json!(compact_warning(state)));
    report["warnings"] = json!(warnings);

    report
}

pub(crate) const NATIVE_DIALOGS_FAILED: &str =
    "Native Save/Open failed its last check; cancel inert panels and use atlas run or atlas open.";

fn compact_warning(state: NativeState) -> String {
    let native = match state {
        NativeState::Passed => "Native Save/Open passed a dated check, not a connection check.",
        NativeState::Failed => NATIVE_DIALOGS_FAILED,
        NativeState::Stale => "Native Save/Open's check is stale; use atlas run or atlas open.",
        NativeState::Unknown => "Native Save/Open is unqualified; use atlas run or atlas open.",
    };
    format!("{native} {WEB_CONTENT}")
}

/// Doctor's report for programs. A ready helper is only that, with any
/// warnings. Otherwise it is the report without the raw helper status it was
/// judged from: the verdict, each check, what to do, and which helper and
/// display. `doctor --verbose` keeps everything for diagnosis.
/// Objects and legacy null reports retain their projection; other roots
/// return a certain state-error report.
pub fn summary(report: Value) -> Value {
    if let Err(error) = atlas_core::session::require_report(&report) {
        return json!(error);
    }
    // Public report ingress; the remaining fields are presentation only.
    let ready = report["ready"] == true;
    summarize(report, ready)
}

fn summarize(mut report: Value, ready: bool) -> Value {
    if ready {
        let warnings = report["warnings"].as_array().cloned().unwrap_or_default();
        return json!({"ok": true, "ready": true, "warnings": warnings});
    }

    let session = report
        .as_object_mut()
        .and_then(|report| report.remove("session"))
        .unwrap_or(Value::Null);

    report["helper"] = json!({
        "name": session["user"]["name"],
        "uid": session["user"]["uid"],
    });

    report["display"] = session["display"].clone();
    report
}

/// A doctor report for tests of what reads one, its Save/Open check in the
/// state `native`.
#[cfg(any(test, feature = "test-support"))]
pub fn sample_report(ready: bool, warnings: Value, native: &str) -> Value {
    with_capabilities(
        json!({
            "ok": true,
            "ready": ready,
            "canObserve": true,
            "capabilities": {
                "observe": true,
                "input": ready,
            },
            "checks": [{"kind": "input", "ok": ready}],
            "nextSteps": if ready { json!([]) } else { json!(["Run atlas start-input."]) },
            "warnings": warnings,
            "absence": "loggedOut",
            "account": "atlas",
            "session": {
                "user": {"name": "atlas", "uid": 502},
                "display": {"width": 1920, "height": 1080},
            },
        }),
        match native {
            "passed" => NativeState::Passed,
            "failed" => NativeState::Failed,
            "stale" => NativeState::Stale,
            _ => NativeState::Unknown,
        },
        json!({"state": native}),
    )
}

#[cfg(test)]
#[path = "capability_tests.rs"]
mod tests;
