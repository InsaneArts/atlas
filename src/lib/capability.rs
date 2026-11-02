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
    session::{Display, Epoch, Observation},
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
const FRESH: Duration = Duration::from_secs(30 * 60);
/// Clock skew tolerated before an observation counts as from the future.
const SKEW: u64 = 60;

/// What must be unchanged for an observation to still describe this session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Generation {
    capture_pid: u32,
    input_pid: u32,
    security_session_id: u64,
    display_id: u64,
    display: Display,
    boot: u64,
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

/// One native panel a check drives.
#[derive(Clone, Copy)]
enum Panel {
    Save,
    Open,
}

impl Panel {
    /// Every panel a passing record covers, in the order they run.
    const ALL: [Self; 2] = [Self::Save, Self::Open];

    /// The name the fixture and the record know this panel by.
    fn kind(self) -> &'static str {
        match self {
            Self::Save => "save",
            Self::Open => "open",
        }
    }
}

/// One native operation's outcome and, if it failed, where.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Check {
    kind: String,
    passed: bool,
    stage: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    observed_at: u64,
    passed: bool,
    checks: Vec<Check>,
    generation: Generation,
    evidence: Vec<PathBuf>,
}

impl Record {
    /// Exactly one check of every panel, and `passed` only if all passed.
    fn consistent(&self) -> bool {
        let covered = |panel: Panel| self.checks.iter().any(|check| check.kind == panel.kind());

        self.checks.len() == Panel::ALL.len()
            && Panel::ALL.into_iter().all(covered)
            && self.passed == self.checks.iter().all(|check| check.passed)
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

fn ready_epoch(observation: &Observation, uid: u32) -> Option<Epoch> {
    let session = observation.require_hidden(uid).ok()?;
    session.input.then_some(session.epoch)
}

/// The generation a status snapshot describes on a machine booted at `boot`.
fn generation_at(observation: &Observation, uid: u32, boot: u64) -> Option<Generation> {
    Some(Generation::of(&ready_epoch(observation, uid)?, boot))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum NativeState {
    Unknown,
    Stale,
    Passed,
    Failed,
}

/// Doctor's view of native dialogs: unknown, stale, passed or failed, and
/// its report.
fn assessment(
    record: Option<&Record>,
    current: Option<&Generation>,
    at: u64,
) -> (NativeState, Value) {
    use NativeState::{Failed, Passed, Stale, Unknown};
    let Some(record) = record else {
        let reason = "Native Save/Open has no recorded check. Panels can be inert without \
                      authenticated Screen Sharing; use atlas run or atlas open instead.";
        return (Unknown, json!({"state": Unknown, "reason": reason}));
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

    let report = json!({
        "state": state,
        "reason": reason,
        "observedAt": record.observed_at,
        "ageSeconds": at.saturating_sub(record.observed_at),
        "checks": record.checks,
        "evidence": record.evidence,
    });
    (state, report)
}

/// Static guidance, not a test of the page or remote connection.
const WEB_CONTENT: &str = "Safari/WebKit can stay blank without authenticated Screen Sharing. \
                           Use Chrome/Firefox for web pages; do not retry a blank page.";

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
    let observation = Observation::decode(&status);
    let current = boot_time().and_then(|boot| generation_at(&observation, uid, boot));
    let (native, projection) = assessment(record.as_ref(), current.as_ref(), now());
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

const NATIVE_DIALOGS_FAILED: &str =
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
    if let Err(error) = crate::session::require_report(&report) {
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

#[cfg(test)]
mod tests;
