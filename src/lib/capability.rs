//! Native dialog capability: an explicit, mutating canary and its dated record.
//!
//! A helper can pass every readiness check while native Save/Open panels
//! fail, so readiness never implies dialogs work. `verify-native` launches
//! a fresh copy of the maintained AppKit fixture, drives its real NSSavePanel
//! and NSOpenPanel with guarded keyboard input under one controller ownership
//! and one display epoch, checks exact file bytes, archives the fixture's
//! evidence, and records the outcome tagged with the session generation it
//! observed. Doctor reports a pass only while that generation is unchanged,
//! the observation is fresh, and both Save and Open passed.
mod canary;
pub use canary::verify;

use crate::{
    config::home,
    error::{Code, Error, Result},
    session::{Display, Epoch, require_hidden},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Default location of the compiled fixture (tests/live/DesktopFixture.swift).
pub const FIXTURE: &str = "/Users/Shared/AtlasTests/DesktopFixture";
/// How long an observation stays meaningful.
const FRESH: Duration = Duration::from_secs(30 * 60);
/// Clock skew tolerated before an observation counts as from the future.
const SKEW: u64 = 60;

/// What must be unchanged for an observation to still describe this session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Generation {
    pub capture_pid: u32,
    pub input_pid: u32,
    pub security_session_id: u64,
    pub display_id: u64,
    pub display: Display,
    pub boot: u64,
}

impl Generation {
    fn of(epoch: &Epoch, boot: u64) -> Self {
        Self {
            capture_pid: epoch.capture_pid,
            input_pid: epoch.input_pid,
            security_session_id: epoch.security_session_id,
            display_id: epoch.display_id,
            display: epoch.display.clone(),
            boot,
        }
    }
}

/// One native operation's outcome and, if it failed, where.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Check {
    pub kind: String,
    pub passed: bool,
    pub stage: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Record {
    pub observed_at: u64,
    pub passed: bool,
    pub checks: Vec<Check>,
    pub generation: Generation,
    pub evidence: Vec<PathBuf>,
}

impl Record {
    /// Exactly one Save and one Open check, and `passed` only if both passed.
    fn consistent(&self) -> bool {
        let outcome = |kind: &str| {
            let mut matches = self.checks.iter().filter(|check| check.kind == kind);
            let first = matches.next().map(|check| check.passed);
            if matches.next().is_some() {
                None
            } else {
                first
            }
        };

        match (self.checks.len(), outcome("save"), outcome("open")) {
            (2, Some(save), Some(open)) => self.passed == (save && open),
            _ => false,
        }
    }
}

fn problem(message: impl Into<String>) -> Error {
    Error::new(Code::Native, message)
}

fn record_path(uid: u32) -> Result<PathBuf> {
    Ok(home()?
        .join("Library/Application Support/Atlas")
        .join(format!("native-{uid}.json")))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Seconds since the epoch at which this machine booted.
#[allow(unsafe_code, reason = "FFI: sysctl kern.boottime has no safe binding")]
fn boot_time() -> Option<u64> {
    let mut boot = libc::timeval {
        tv_sec: 0,
        tv_usec: 0,
    };
    let mut size = std::mem::size_of::<libc::timeval>();
    let mut name = [libc::CTL_KERN, libc::KERN_BOOTTIME];

    // SAFETY: valid MIB, output buffer and size for kern.boottime.
    let status = unsafe {
        libc::sysctl(
            name.as_mut_ptr(),
            2,
            (&mut boot as *mut libc::timeval).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };

    (status == 0).then(|| u64::try_from(boot.tv_sec).unwrap_or_default())
}

/// The generation a status snapshot describes, if it shows a ready helper.
pub fn generation(status: &Value, uid: u32) -> Option<Generation> {
    generation_at(status, uid, boot_time()?)
}

/// The generation a status snapshot describes on a machine booted at `boot`.
fn generation_at(status: &Value, uid: u32, boot: u64) -> Option<Generation> {
    let session = require_hidden(status, uid).ok().filter(|s| s.input)?;
    Some(Generation::of(&session.epoch, boot))
}

/// Doctor's view of native dialogs: unknown, stale, passed or failed.
pub fn assess(record: Option<&Record>, current: Option<&Generation>, at: u64) -> Value {
    assessment(record, current, at).value()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum NativeState {
    Unknown,
    Stale,
    Passed,
    Failed,
}

struct Assessment<'a> {
    state: NativeState,
    reason: &'static str,
    record: Option<&'a Record>,
    at: u64,
}

impl Assessment<'_> {
    fn value(&self) -> Value {
        let mut value = json!({"state": self.state, "reason": self.reason});
        if let Some(record) = self.record {
            value["observedAt"] = json!(record.observed_at);
            value["ageSeconds"] = json!(self.at.saturating_sub(record.observed_at));
            value["checks"] = json!(record.checks);
            value["evidence"] = json!(record.evidence);
        }
        value
    }
}

fn assessment<'a>(
    record: Option<&'a Record>,
    current: Option<&Generation>,
    at: u64,
) -> Assessment<'a> {
    use NativeState::{Failed, Passed, Stale, Unknown};
    let Some(record) = record else {
        return Assessment {
            state: Unknown,
            reason: "Native Save/Open has no recorded check. Panels can be inert without \
                       authenticated Screen Sharing; use atlas run or atlas open instead.",
            record: None,
            at,
        };
    };

    let (state, reason) = if !record.consistent() {
        (
            Unknown,
            "The recorded observation is inconsistent and was ignored",
        )
    } else if record.observed_at > at.saturating_add(SKEW) {
        (Unknown, "The recorded observation is dated in the future")
    } else if current != Some(&record.generation) {
        (
            Stale,
            "The helper session, display, capture or input process changed since it was observed",
        )
    } else if at.saturating_sub(record.observed_at) > FRESH.as_secs() {
        (Stale, "The observation is older than 30 minutes")
    } else if record.passed {
        (
            Passed,
            "Native Save and Open completed with exact bytes at the recorded time; this is not a connection check",
        )
    } else {
        (Failed, "Native Save/Open did not complete in this session")
    };

    Assessment {
        state,
        reason,
        record: Some(record),
        at,
    }
}

/// Static guidance, not a test of the page or remote connection.
const WEB_CONTENT: &str = "Safari/WebKit can stay blank without authenticated Screen Sharing. \
                           Use Chrome/Firefox for web pages; do not retry a blank page.";

/// Doctor's native-dialog assessment for a status snapshot.
pub fn native_dialogs(status: &Value, uid: u32) -> Value {
    let record = read_record(uid);
    assess(record.as_ref(), generation(status, uid).as_ref(), now())
}

fn read_record(uid: u32) -> Option<Record> {
    record_path(uid)
        .ok()
        .and_then(|path| fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<Record>(&bytes).ok())
}

/// Doctor's report with the recorded native-dialog observation in place of
/// the default unknown.
pub fn doctor(status: Value, uid: u32) -> Value {
    doctor_report(status, uid).diagnosis.value
}

pub(crate) struct Doctor {
    pub(crate) diagnosis: crate::session::Diagnosis,
    pub(crate) native: NativeState,
    pub(crate) absence: Option<crate::session::Absence>,
}

pub(crate) fn doctor_report(status: Value, uid: u32) -> Doctor {
    let record = read_record(uid);
    let observation = crate::session::Observation::decode(&status);
    let current = boot_time().and_then(|boot| {
        observation
            .require_hidden(uid)
            .ok()
            .filter(|session| session.input)
            .map(|session| Generation::of(&session.epoch, boot))
    });
    let assessment = assessment(record.as_ref(), current.as_ref(), now());
    let native = assessment.state;
    let projection = assessment.value();
    let mut diagnosis = observation.diagnose(status, uid);
    diagnosis.value = with_capabilities(diagnosis.value, native, projection);
    Doctor {
        diagnosis,
        native,
        absence: None,
    }
}

fn with_capabilities(mut report: Value, state: NativeState, native: Value) -> Value {
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

/// What doctor warns when the last native Save and Open check failed.
pub const NATIVE_DIALOGS_FAILED: &str =
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
pub fn summary(report: Value) -> Value {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn generation() -> Generation {
        Generation {
            capture_pid: 1,
            input_pid: 2,
            security_session_id: 3,
            display_id: 4,
            display: Display {
                width: 2560.0,
                height: 1440.0,
                scale: 1.0,
            },
            boot: 5,
        }
    }

    fn check(kind: &str, passed: bool) -> Check {
        Check {
            kind: kind.into(),
            passed,
            stage: (!passed).then(|| format!("{kind} confirmation enabled")),
            error: None,
        }
    }

    fn record(save: bool, open: bool, observed_at: u64) -> Record {
        Record {
            observed_at,
            passed: save && open,
            checks: vec![check("save", save), check("open", open)],
            generation: generation(),
            evidence: Vec::new(),
        }
    }

    #[test]
    fn observations_count_only_for_the_same_fresh_generation() {
        let current = generation();
        let state = |record: Option<&Record>, current: Option<&Generation>, at| {
            assess(record, current, at)["state"].clone()
        };

        assert_eq!(state(None, Some(&current), 100), "unknown");

        let passed = record(true, true, 100);
        assert_eq!(state(Some(&passed), Some(&current), 160), "passed");
        let failed = record(true, false, 100);
        assert_eq!(state(Some(&failed), Some(&current), 160), "failed");
        assert_eq!(state(Some(&passed), Some(&current), 100 + 31 * 60), "stale");

        let mut resized = generation();
        resized.display.width = 1920.0;
        assert_eq!(state(Some(&passed), Some(&resized), 160), "stale");

        let mut restarted = generation();
        restarted.input_pid = 9;
        assert_eq!(state(Some(&passed), Some(&restarted), 160), "stale");
        assert_eq!(state(Some(&passed), None, 160), "stale");
    }

    #[test]
    fn inconsistent_or_future_records_are_not_trusted() {
        let current = generation();
        let state = |record: &Record| assess(Some(record), Some(&current), 1000)["state"].clone();

        let mut save_only = record(true, true, 900);
        save_only.checks.pop();
        assert_eq!(state(&save_only), "unknown");

        let mut duplicated = record(true, true, 900);
        duplicated.checks[1] = check("save", true);
        assert_eq!(state(&duplicated), "unknown");

        let mut claimed = record(true, false, 900);
        claimed.passed = true;
        assert_eq!(state(&claimed), "unknown");

        let mut launch = record(false, false, 900);
        launch.checks = vec![check("launch", false)];
        assert_eq!(state(&launch), "unknown");

        assert_eq!(state(&record(true, true, 1000 + SKEW + 1)), "unknown");
        assert_eq!(state(&record(true, true, 1000 + SKEW)), "passed");
    }

    fn report(ready: bool, warnings: Value, native: &str) -> Value {
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

    #[test]
    fn a_doctor_warns_about_panels_and_webkit_once_in_each_format() {
        for (state, native) in [
            (
                "unknown",
                "Native Save/Open is unqualified; use atlas run or atlas open.",
            ),
            (
                "stale",
                "Native Save/Open's check is stale; use atlas run or atlas open.",
            ),
            ("failed", NATIVE_DIALOGS_FAILED),
            (
                "passed",
                "Native Save/Open passed a dated check, not a connection check.",
            ),
        ] {
            for ready in [false, true] {
                let full = report(ready, json!([]), state);
                let warning = format!("{native} {WEB_CONTENT}");
                assert_eq!(full["warnings"], json!([warning]));
                for verbose in [false, true] {
                    let rendered = crate::ui::render::render(
                        &crate::ui::render::View::Doctor { verbose },
                        &full,
                    );
                    assert_eq!(
                        rendered.out.matches(&warning).count(),
                        1,
                        "{}",
                        rendered.out
                    );
                    assert_eq!(rendered.out.matches(native).count(), 1, "{}", rendered.out);
                }
                let summed = summary(full);
                assert_eq!(summed["warnings"], json!([warning]));
                if ready {
                    assert_eq!(
                        summed,
                        json!({"ok":true, "ready":true, "warnings":[warning]})
                    );
                    assert!(summed.to_string().len() < 384);
                }
            }
        }
    }

    #[test]
    fn compact_capability_advice_preserves_existing_warnings() {
        let summed = summary(report(true, json!(["The display changed."]), "passed"));
        assert_eq!(summed["warnings"][0], "The display changed.");
        assert_eq!(summed["warnings"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn a_pass_reports_its_date_without_claiming_a_connected_session() {
        let passed = record(true, true, 100);
        let assessment = assess(Some(&passed), Some(&generation()), 160);
        assert_eq!(assessment["observedAt"], 100);
        assert_eq!(assessment["ageSeconds"], 60);
        assert!(
            assessment["reason"]
                .as_str()
                .unwrap()
                .contains("not a connection check")
        );
        assert_eq!(
            assess(Some(&passed), Some(&generation()), u64::MAX)["state"],
            "stale"
        );
    }

    #[test]
    fn a_doctor_that_is_not_ready_keeps_every_check_for_programs() {
        let summed = summary(report(false, json!(["The display changed."]), "failed"));

        assert_eq!(summed["ok"], true);
        assert_eq!(summed["ready"], false);
        assert_eq!(summed["helper"], json!({"name": "atlas", "uid": 502}));
        assert_eq!(summed["display"], json!({"width": 1920, "height": 1080}));
        assert_eq!(summed["checks"], json!([{"kind": "input", "ok": false}]));
        assert_eq!(summed["nextSteps"], json!(["Run atlas start-input."]));
        assert_eq!(
            summed["warnings"],
            json!(["The display changed.", compact_warning(NativeState::Failed)])
        );
        assert_eq!(summed["capabilities"]["nativeDialogs"]["state"], "failed");
        assert_eq!(summed["absence"], "loggedOut");
        assert_eq!(summed["account"], "atlas");
        assert!(summed.get("session").is_none());
    }

    #[test]
    fn a_failed_native_check_keeps_its_exact_warning() {
        let report = with_capabilities(
            json!({"warnings": ["existing"]}),
            NativeState::Failed,
            json!({"state": "passed"}),
        );
        assert_eq!(
            report["warnings"],
            json!(["existing", format!("{NATIVE_DIALOGS_FAILED} {WEB_CONTENT}")])
        );
        assert_eq!(report["capabilities"]["nativeDialogs"]["state"], "passed");
    }

    #[test]
    fn missing_proof_never_upgrades_capability() {
        for status in [
            Value::Null,
            json!({}),
            json!({"ok": true}),
            json!({
                "ok": true, "user": {"uid": 502},
                "session": {"managerName": "Aqua", "onConsole": false},
                "inputService": {"ok": true, "ready": true, "accessibility": true}
            }),
        ] {
            let diagnosis = crate::session::diagnosis(status, 502);
            assert!(!diagnosis.ready);
            assert!(diagnosis.observation.require_hidden(502).is_err());
            assert!(!diagnosis.value["capabilities"]["input"].as_bool().unwrap());
        }
        let native = assessment(None, None, 100);
        assert_eq!(native.state, NativeState::Unknown);
        assert_eq!(
            native.value(),
            json!({
                "state": "unknown",
                "reason": "Native Save/Open has no recorded check. Panels can be inert without authenticated Screen Sharing; use atlas run or atlas open instead."
            })
        );
    }

    #[test]
    fn a_summary_uses_the_readiness_fact_instead_of_the_projection() {
        assert_eq!(
            summarize(json!({"ready": false, "warnings": ["kept"]}), true),
            json!({"ok": true, "ready": true, "warnings": ["kept"]})
        );
    }
}
