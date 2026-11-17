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

#[derive(Clone)]
pub struct Doctor {
    pub diagnosis: atlas_core::session::Diagnosis,
    pub(crate) native: NativeState,
    pub(crate) absence: Option<atlas_core::session::Absence>,
    /// A VM desktop's mode before its proof, as its guest says: none for
    /// any other desktop.
    pub(crate) vm: Option<VmMode>,
}

/// What a VM desktop is, as the overview shows it before its proof: only
/// what its guest said of its Stop decides it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VmMode {
    /// The guest says agents are stopped there: Resume is offered.
    Stopped,
    /// Anything else, and why.
    Unavailable(String),
}

impl Doctor {
    /// The report of a desktop this build cannot reach: `reply`, never
    /// ready, with `refusal` for its first step and nothing checked.
    pub(crate) fn unreached(reply: Value, refusal: atlas_core::error::Error) -> Self {
        Self {
            diagnosis: atlas_core::session::Diagnosis {
                observation: atlas_core::session::Observation::decode(&reply),
                value: reply,
                ready: false,
                first_step: atlas_core::session::Guidance::Advice(refusal.error.clone()),
            },
            native: NativeState::Unknown,
            absence: Some(atlas_core::session::Absence::Unreachable(refusal.error)),
            vm: None,
        }
    }

    /// The report of a VM desktop from its `status`: never ready, stopped
    /// when its guest says so, through its keeper, otherwise unavailable with
    /// why: the keeper's own when its session is not live, else its first
    /// step, [`vm_first_step`]'s, or `refusal`, the one wording for what it
    /// cannot do yet.
    pub(crate) fn vm(status: Value, refusal: atlas_core::error::Error) -> Self {
        let first = vm_first_step(&status).map_or_else(|| refusal.error.clone(), |step| step.error);
        let guest = &status["guest"];
        let stop = if guest["ok"] == true {
            &guest["stop"]["state"]
        } else {
            &status["keeper"]["report"]["stop"]["state"]
        };
        let mode = if status["ok"] == true && stop == "stopped" {
            VmMode::Stopped
        } else if status["ok"] != true {
            VmMode::Unavailable(
                status["why"]
                    .as_str()
                    .or_else(|| status["error"].as_str())
                    .unwrap_or(&refusal.error)
                    .to_owned(),
            )
        } else if status["keeper"]["live"] != true {
            VmMode::Unavailable(
                status["keeper"]["why"]
                    .as_str()
                    .unwrap_or(&refusal.error)
                    .to_owned(),
            )
        } else {
            VmMode::Unavailable(with_work(&first, &status))
        };
        let mut doctor = Self::unreached(status, refusal);
        doctor.diagnosis.first_step = atlas_core::session::Guidance::Advice(first);
        doctor.vm = Some(mode);
        doctor
    }

    /// This VM report, its mode the handover `unfinished` says is still under
    /// way, when one is: that is its reason, whatever else it says.
    #[must_use]
    pub(crate) fn handing_over(mut self, unfinished: Option<String>) -> Self {
        if let Some(why) = unfinished {
            self.vm = Some(VmMode::Unavailable(why));
        }
        self
    }
}

/// `why` a VM whose status is `status` is not ready yet, and, once it is
/// proven, what its guest says of work an agent started there that is
/// still running or may be.
fn with_work(why: &str, status: &Value) -> String {
    if status["proven"] != true {
        return why.to_owned();
    }
    match status["leftovers"].as_str() {
        Some("running") => format!("{why} Work an agent started is still running there."),
        Some("unknown") => {
            format!("{why} Whether work an agent started still runs there is not known.")
        }
        _ => why.to_owned(),
    }
}

/// What comes first for a VM desktop whose status is `status`, before the
/// one wording for what this build cannot do on a VM: its proof's refusal,
/// or, once proven, what its guest still has to be allowed, in its own
/// words, or that Atlas in it is part way through an update. None once nothing comes before that wording, and for a status
/// that is no answer, whose refusal is that wording.
#[must_use]
pub fn vm_first_step(status: &Value) -> Option<atlas_core::error::Error> {
    use atlas_core::error::{Code, Error};
    if status["ok"] != true {
        return None;
    }
    // A status that says no proof was taken has no proof's refusal to say.
    if status["proven"] != true {
        let proof = &status["proof"];
        let code = proof["code"]
            .as_str()
            .and_then(Code::parse)
            .unwrap_or(Code::Identity);
        return proof["error"]
            .as_str()
            .map(|refused| Error::new(code, refused));
    }
    let guest = &status["guest"];
    if guest["permissions"]["screenCapture"] != true {
        return Some(Error::new(
            Code::Permission,
            "In the VM's window, open System Settings › Privacy & Security › Screen & \
             System Audio Recording and turn Atlas on.",
        ));
    }
    if status["proof"]["input"] != true {
        if crate::vm::proof::mid_update(guest) {
            return Some(Error::new(Code::Update, crate::vm::proof::MID_UPDATE));
        }
        return Some(match guest["input"]["held"].as_str() {
            Some(held) => Error::new(Code::Permission, held),
            None => Error::new(
                Code::Input,
                "The VM's input service is not ready to act; agents wait for it.",
            ),
        });
    }
    None
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
