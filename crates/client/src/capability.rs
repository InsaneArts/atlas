//! What doctor reports of a helper's native dialogs and web content, a VM
//! desktop's report from its status, and the summary programs read. The
//! dated check of native Save/Open panels is driven by [`canary`] on every
//! desktop; the user desktop's record of it is in
//! [`crate::user::capability`], a VM's in its own records folder.
pub mod canary;

use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum NativeState {
    Unknown,
    Stale,
    Passed,
    Failed,
    /// No check of them is made on this desktop yet.
    NotChecked,
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

/// What a VM desktop is, as the overview shows it: the person's maintenance
/// of it first, then what its guest said of its Stop, then whether it is
/// ready for agents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VmMode {
    /// The person's maintenance holds it, and why: no Resume is offered, as
    /// it is refused until the maintenance ends.
    Maintained(String),
    /// The guest says agents are stopped there: Resume is offered.
    Stopped,
    /// Ready for agents, as [`crate::client::readiness_of`] judges it.
    Ready,
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

    /// The report of a VM desktop from its `status`, its report beside it:
    /// stopped when its guest says so, through its keeper; ready when
    /// [`crate::client::readiness_of`] says so; otherwise unavailable with
    /// why: the keeper's own when its session is not live, else why it is
    /// not ready, [`vm_first_step`]'s, or `refusal`, the one wording for what
    /// it cannot do.
    pub(crate) fn vm(status: Value, refusal: atlas_core::error::Error) -> Self {
        let ready = crate::client::readiness_of(&status).is_ok();
        let first = status["notReady"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| vm_first_step(&status).map(|step| step.error))
            .unwrap_or_else(|| refusal.error.clone());
        let guest = &status["guest"];
        let stop = if guest["ok"] == true {
            &guest["stop"]["state"]
        } else {
            &status["keeper"]["report"]["stop"]["state"]
        };
        // Said by the station's own record, a status that failed included;
        // the guest's Stop stays in the status as it is.
        let maintained = status["station"]["maintenance"]["state"]
            .as_str()
            .filter(|state| *state != "none")
            .map(|_| {
                let name = status["desktop"]["name"].as_str().unwrap_or("the VM");
                crate::vm::maintenance::holds(name, &status["station"]["maintenance"])
                    .unwrap_or_default()
            });
        let mode = if let Some(why) = maintained {
            VmMode::Maintained(why)
        } else if status["ok"] == true && stop == "stopped" {
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
        } else if ready {
            VmMode::Ready
        } else {
            VmMode::Unavailable(with_work(&first, &status))
        };
        let ready = mode == VmMode::Ready;
        let mut doctor = Self::unreached(with_vm_report(status, ready), refusal);
        doctor.diagnosis.ready = ready;
        doctor.diagnosis.first_step = atlas_core::session::Guidance::Advice(first);
        if ready {
            doctor.absence = None;
        }
        doctor.native = NativeState::NotChecked;
        doctor.vm = Some(mode);
        doctor
    }

    /// This VM report, its mode the handover `unfinished` says is still under
    /// way, when one is: that is its reason, whatever else it says, and it is
    /// not ready.
    #[must_use]
    pub(crate) fn handing_over(mut self, unfinished: Option<String>) -> Self {
        if let Some(why) = unfinished {
            self.diagnosis.ready = false;
            self.diagnosis.value["ready"] = json!(false);
            self.vm = Some(VmMode::Unavailable(why));
        }
        self
    }
}

/// Doctor's report of the VM whose status, an answer, is `status`: what it
/// says, whether it is ready for agents, each check of it in order, what to
/// do, and what is not checked on a VM yet. `refusal` is the one wording
/// for what this build cannot do on a VM.
#[must_use]
pub fn vm_report(status: Value, refusal: atlas_core::error::Error) -> Value {
    Doctor::vm(status, refusal).diagnosis.value
}

/// The VM whose status is `status`, `ready` or not, with doctor's report
/// beside what it says: each check of its readiness, what to do, what
/// agents may do there, and what is not checked on a VM yet, never as
/// passed.
fn with_vm_report(mut status: Value, ready: bool) -> Value {
    if !status.is_object() {
        return status;
    }
    let name = status["desktop"]["name"]
        .as_str()
        .unwrap_or("the VM")
        .to_owned();
    let proven = status["ok"] == true && status["proven"] == true;
    let observe = proven && status["proof"]["capture"] == true;
    let input = proven && status["proof"]["input"] == true;
    let checks = crate::client::readiness_checks(&status);
    let mut next_steps: Vec<Value> = checks
        .iter()
        .filter(|check| check["ok"] == false)
        .map(|check| check["nextStep"].clone())
        .collect();
    // A step two checks fail for, as maintenance makes the proof's, is
    // said once.
    next_steps.dedup();
    let mut warnings = vec![json!(vm_native_warning(&name, NativeState::NotChecked))];
    match status["leftovers"].as_str() {
        Some("running") if proven => warnings.push(json!(format!(
            "Work an agent started is still running on {name}."
        ))),
        Some("unknown") if proven => warnings.push(json!(format!(
            "Whether work an agent started still runs on {name} is not known."
        ))),
        _ => {}
    }
    if let Some(being_made) = crate::client::under_way(&name, &status["station"]["unresolved"]) {
        warnings.push(json!(being_made));
    }
    status["ready"] = json!(ready);
    status["canObserve"] = json!(observe);
    status["capabilities"] = json!({
        "observe": observe,
        "input": input,
        "nativeDialogs": native_unasked(&name, proven),
        "installedApps": installed_unasked(&name, &status),
        "maintenance": maintenance(&name, &status["station"]["maintenance"]),
    });
    status["checks"] = json!(checks);
    status["nextSteps"] = json!(next_steps);
    status["warnings"] = json!(warnings);
    status
}

/// Doctor's word on native dialogs on the VM `name` before its record is
/// read: judged once it is proven again, else not checked yet.
fn native_unasked(name: &str, proven: bool) -> Value {
    let reason = if proven {
        format!(
            "Native Save/Open has not been checked on {name}; use atlas run or atlas open \
             instead, or check it with atlas --desktop {name} verify-native."
        )
    } else {
        format!("Whether native Save/Open works on {name} is judged once it is proven again.")
    };
    json!({"state": NativeState::NotChecked, "reason": reason})
}

/// The warning doctor gives of native dialogs on the VM `name` in `state`.
fn vm_native_warning(name: &str, state: NativeState) -> String {
    match state {
        NativeState::Passed => {
            format!("Native Save/Open passed a dated check in {name}, not a connection check.")
        }
        NativeState::Failed => format!(
            "Native Save/Open failed its last check in {name}; cancel inert panels and use atlas \
             run or atlas open."
        ),
        NativeState::Stale => {
            format!("Native Save/Open's check of {name} is stale; use atlas run or atlas open.")
        }
        NativeState::Unknown => {
            format!("Native Save/Open is unqualified on {name}; use atlas run or atlas open.")
        }
        NativeState::NotChecked => {
            format!("Native Save/Open is not checked on {name}; use atlas run or atlas open.")
        }
    }
}

/// The browsers a sign-in opens, which doctor asks a VM after.
const BROWSERS: [&str; 2] = ["Google Chrome", "Firefox"];

/// Doctor's report of the VM `client` names, whose status is `status`:
/// [`vm_report`], and which browsers its guest says it has, asked once its
/// status shows it proven, serving the read and under no maintenance.
/// "Not checked" when it cannot be asked, never passed.
pub async fn vm_doctor_report(client: &crate::client::Client, status: Value) -> Value {
    let name = client.selected.name.clone();
    let asked = status["ok"] == true
        && status["proven"] == true
        && status["station"]["maintenance"]["state"] == "none"
        && declares_installed(&status);
    let (native, dialogs) = crate::vm::native::native_dialogs(client, &status);
    let mut report = vm_report(status, client.unreachable());
    if report["capabilities"].is_object() {
        report["capabilities"]["nativeDialogs"] = dialogs;
        let unchecked = json!(vm_native_warning(&name, NativeState::NotChecked));
        if let Some(warning) = report["warnings"]
            .as_array_mut()
            .and_then(|warnings| warnings.iter_mut().find(|warning| **warning == unchecked))
        {
            *warning = json!(vm_native_warning(&name, native));
        }
    }
    if asked && report["capabilities"].is_object() {
        report["capabilities"]["installedApps"] =
            match crate::app_selection::installed(client, &BROWSERS).await {
                Ok(apps) => installed_checked(&name, &apps),
                Err(refused) if refused.code == atlas_core::error::Code::Update => {
                    not_checked_apps(older_apps(&name))
                }
                // The guest could not look: in its words, said of the VM.
                Err(refused) if refused.code == atlas_core::error::Code::Apps => {
                    not_checked_apps(refused.error)
                }
                Err(_) => not_checked_apps(unproven_apps(&name)),
            };
    }
    report
}

/// Whether the guest of the VM whose status is `status` declared, where
/// its proof was taken, that it says which apps it has.
fn declares_installed(status: &Value) -> bool {
    status["declared"]["capabilities"]
        .as_array()
        .is_some_and(|declared| {
            declared
                .iter()
                .any(|name| name == atlas_session::compatibility::INSTALLED)
        })
}

/// Doctor's word on which apps the VM `name`, whose status is `status`,
/// has before its guest is asked: not checked, as its Atlas cannot say
/// when its proof shows so, else until it is proven again.
fn installed_unasked(name: &str, status: &Value) -> Value {
    let proven = status["ok"] == true && status["proven"] == true;
    if proven && status["declared"].is_object() && !declares_installed(status) {
        not_checked_apps(older_apps(name))
    } else {
        not_checked_apps(unproven_apps(name))
    }
}

fn not_checked_apps(reason: String) -> Value {
    json!({"state": NativeState::NotChecked, "reason": reason})
}

fn older_apps(name: &str) -> String {
    format!("{name}'s Atlas is from an older build and cannot say which apps it has.")
}

fn unproven_apps(name: &str) -> String {
    format!("Which apps {name} has is asked once it is proven again.")
}

/// Doctor's word on the browsers the VM `name` has, as its guest said
/// `apps`: each by its name, and what that means for a sign-in there.
fn installed_checked(name: &str, apps: &BTreeMap<String, bool>) -> Value {
    let has = |app: &str| apps.get(app).copied().unwrap_or(false);
    let [chrome, firefox] = BROWSERS;
    let reason = match (has(chrome), has(firefox)) {
        (false, false) => format!(
            "Neither {chrome} nor {firefox} is installed in {name}; sign-ins there need one of \
             them."
        ),
        (true, false) => format!("{chrome} is installed in {name}; {firefox} is not."),
        (false, true) => format!("{firefox} is installed in {name}; {chrome} is not."),
        (true, true) => format!("{chrome} and {firefox} are installed in {name}."),
    };
    json!({"state": "checked", "apps": apps, "reason": reason})
}

/// Doctor's word on the person's maintenance of the VM `name`, as its
/// status says it, `said`: its state, and why agents wait while one is
/// kept. A status that does not say is never none, and never passed.
fn maintenance(name: &str, said: &Value) -> Value {
    match said["state"].as_str() {
        Some("none") => json!({"state": "none"}),
        Some(state @ ("on" | "ending")) => json!({"state": state, "reason": said["reason"]}),
        _ => json!({
            "state": "unknown",
            "reason": crate::vm::maintenance::holds(name, said),
        }),
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

/// What comes first for a VM desktop whose status is `status`: its proof's
/// refusal, or, once proven, what its guest still has to be allowed, in its
/// own words, with what the person is asked, or that Atlas in it is part
/// way through an update. None once nothing of these holds it, and for a
/// status that is no answer.
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
    let name = status["desktop"]["name"].as_str().unwrap_or("the VM");
    // Asked of the person in the VM's own window, never the helper's.
    let allow = |setting: &str| json!(allow_in_window(name, setting));
    if guest["permissions"]["screenCapture"] != true {
        return Some(vm_capture_held(guest).with("next", allow("Screen & System Audio Recording")));
    }
    if status["proof"]["input"] != true {
        let held = vm_input_held(guest);
        return Some(if held.code == Code::Permission {
            held.with("next", allow("Accessibility"))
        } else {
            held
        });
    }
    None
}

/// What the person is asked so Atlas in the VM `name` may use `setting`:
/// to allow it in the VM's own window, opened for their maintenance.
pub(crate) fn allow_in_window(name: &str, setting: &str) -> String {
    format!(
        "Ask the person to open {name}'s window with atlas desktop maintain {name} and allow \
         Atlas in Privacy & Security, {setting}, there."
    )
}

/// Why a VM whose guest's status is `guest` gives agents no pictures now, in
/// the guest's own words where it says why.
pub(crate) fn vm_capture_held(guest: &Value) -> atlas_core::error::Error {
    use atlas_core::error::{Code, Error};
    Error::new(
        Code::Permission,
        guest["capture"]["held"].as_str().unwrap_or(
            "In the VM's window, open System Settings › Privacy & Security › Screen & System \
             Audio Recording and turn Atlas on.",
        ),
    )
}

/// Why a VM whose guest's status is `guest` takes no input from agents now,
/// in the guest's own words where it says why: Atlas in it part way through
/// an update, what it still has to be allowed, or its input service not
/// ready.
pub(crate) fn vm_input_held(guest: &Value) -> atlas_core::error::Error {
    use atlas_core::error::{Code, Error};
    if crate::vm::proof::mid_update(guest) {
        return Error::new(Code::Update, crate::vm::proof::MID_UPDATE);
    }
    match guest["input"]["held"].as_str() {
        Some(held) => Error::new(Code::Permission, held),
        None => Error::new(
            Code::Input,
            "The VM's input service is not ready to act; agents wait for it.",
        ),
    }
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
        NativeState::NotChecked => {
            "Native Save/Open is not checked here; use atlas run or atlas open."
        }
    };
    format!("{native} {WEB_CONTENT}")
}

/// Doctor's report for programs. A ready desktop is only that, with any
/// warnings. Otherwise it is the report without the raw status it was
/// judged from: the verdict, each check, what to do, and which helper and
/// display, or which VM. `doctor --verbose` keeps everything for diagnosis.
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

    // A VM's report keeps only its verdict, its checks and which VM, never
    // the status it was judged from.
    if report["desktop"]["kind"] == "vm" {
        let shown = [
            "ok",
            "ready",
            "notReady",
            "desktop",
            "canObserve",
            "capabilities",
            "checks",
            "nextSteps",
            "warnings",
        ];
        if let Some(fields) = report.as_object_mut() {
            fields.retain(|key, _| shown.contains(&key.as_str()));
        }
        return report;
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
