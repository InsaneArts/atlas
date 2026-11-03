//! Doctor checks and their public guidance, derived from one observation.
use super::{Observation, PermissionObservation, Session, connection_refusal};
use crate::error::{Code, Error, Result};
use serde_json::{Value, json};

/// The concrete recovery action and the words accompanying other advice.
#[derive(Clone, Debug)]
pub enum Guidance {
    StartInput,
    Advice(String),
}

impl From<&str> for Guidance {
    fn from(text: &str) -> Self {
        // External error guidance can use the same words as the recovery action.
        if text == START_INPUT {
            Self::StartInput
        } else {
            Self::Advice(text.to_owned())
        }
    }
}

impl Guidance {
    pub fn text(&self) -> &str {
        match self {
            Self::StartInput => START_INPUT,
            Self::Advice(text) => text,
        }
    }
}

/// Null retains the existing uninitialized-report projection. Other
/// non-object roots cannot receive report fields safely.
///
/// # Errors
///
/// Returns an error when `report` is neither an object nor null.
pub fn require_report(report: &Value) -> Result<()> {
    if report.is_object() || report.is_null() {
        Ok(())
    } else {
        Err(Error::new(
            Code::State,
            "A doctor report must be an object or null",
        ))
    }
}

pub struct Diagnosis {
    pub value: Value,
    pub observation: Observation,
    pub ready: bool,
    pub first_step: Guidance,
}

struct Check<'a> {
    name: &'static str,
    /// None when the fact cannot be observed yet; never reported as missing.
    ok: Option<bool>,
    next_step: &'a str,
}

#[cfg(any(test, feature = "test-support"))]
pub fn diagnosis(value: Value, uid: u32) -> Diagnosis {
    Observation::decode(&value).diagnose(value, uid)
}

impl Observation {
    /// The doctor report for a status snapshot: each check, what to do about a
    /// failed one, and `ready` for transport, identity, permissions and the
    /// input service. Whether native Save/Open dialogs work is a separate
    /// observation (the native capability check) that `ready` never implies.
    pub fn diagnose(self, value: Value, uid: u32) -> Diagnosis {
        let session = self.require_hidden(uid).ok();
        let checks = self.checks(uid, session.as_ref());

        let mut warnings = Vec::new();

        if self.native.ok && !self.input_build_current() {
            warnings.push(
                "The input service is from an older build; atlas start-input replaces it."
                    .to_owned(),
            );
        }

        let details = checks
            .iter()
            .map(|check| {
                json!({
                    "name": check.name,
                    "ok": check.ok,
                    "nextStep": (check.ok != Some(true)).then_some(check.next_step),
                })
            })
            .collect::<Vec<_>>();

        let next_steps = checks
            .iter()
            .filter(|check| check.ok == Some(false))
            .map(|check| check.next_step)
            .collect::<Vec<_>>();

        let ready = checks.iter().all(|check| check.ok == Some(true));
        let first_step = checks
            .iter()
            .find(|check| check.ok == Some(false))
            .map(|check| Guidance::from(check.next_step))
            .unwrap_or_else(|| Guidance::Advice("The helper is not ready".into()));
        let observe = session.as_ref().is_some_and(|s| s.capture);
        let mut report = json!({
            "ok": true,
            "ready": ready,
            "canObserve": observe,
            "capabilities": {
                "observe": observe,
                "input": session.as_ref().is_some_and(|s| s.input || self.native.repinnable(s)),
                // Only a fresh native check counts; capability::doctor puts the
                // recorded observation here.
                "nativeDialogs": {
                    "state": "unknown",
                    "reason": "No fresh native Save/Open observation. Native dialogs were qualified \
                               only with an authenticated Screen Sharing session into the helper.",
                },
            },
            "checks": details,
            "nextSteps": next_steps,
            "warnings": warnings,
        });

        // The status itself, moved in rather than copied.
        report["session"] = value;

        Diagnosis {
            value: report,
            observation: self,
            ready,
            first_step,
        }
    }

    fn checks(&self, uid: u32, session: Option<&Session>) -> [Check<'_>; 5] {
        let reachable = self.ok;
        let native_reachable = self.native.ok;
        // The input service is only queried once the capture side proves a hidden
        // helper; an unqueried service is unknown, not failed.
        let queried = reachable && self.native.queried;

        // A blocked shell says nothing of the helper, so it may well be running.
        let blocked = self.code.as_deref() == Some("blocked");
        let (unknown, not_queried) = if blocked {
            let unreached = "Unknown until this shell can reach the helper.";
            (unreached, unreached)
        } else {
            (
                "Unknown until the helper is running.",
                "Unknown until the helper runs in the background.",
            )
        };

        let connection = self.connection_guidance();
        let hidden_guidance = self.hidden_guidance(uid);

        [
            Check {
                // A blocked shell cannot tell whether the helper runs.
                name: if blocked {
                    "Helper reachable from here"
                } else {
                    "Helper running"
                },
                ok: Some(reachable),
                next_step: connection,
            },
            Check {
                name: "Helper in the background",
                ok: reachable.then_some(session.is_some()),
                next_step: if reachable { hidden_guidance } else { unknown },
            },
            Check {
                name: "Screen Recording allowed",
                ok: reachable.then_some(self.capture == PermissionObservation::Allowed),
                next_step: if reachable {
                    "In the helper's System Settings › Privacy & Security › Screen & System Audio \
                     Recording, turn Atlas on."
                } else {
                    unknown
                },
            },
            Check {
                name: "Accessibility allowed",
                ok: native_reachable
                    .then_some(self.native.accessibility == PermissionObservation::Allowed),
                next_step: if native_reachable
                    || self.accessibility == PermissionObservation::Denied
                {
                    "In the helper's System Settings › Privacy & Security › Accessibility, turn \
                     Atlas on (atlas init walks you through it)."
                } else if queried {
                    START_INPUT
                } else {
                    not_queried
                },
            },
            Check {
                name: "Input service ready",
                ok: queried
                    .then_some(session.is_some_and(|s| s.input || self.native.repinnable(s))),
                next_step: if queried && self.accessibility == PermissionObservation::Denied {
                    "Turn on Accessibility for Atlas in the helper first; atlas init walks you through it."
                } else if queried {
                    START_INPUT
                } else if reachable {
                    not_queried
                } else {
                    unknown
                },
            },
        ]
    }

    fn connection_guidance(&self) -> &str {
        if self.code.as_deref() == Some("identity") {
            self.error
                .as_deref()
                .unwrap_or("The socket belongs to another account")
        } else if self.code.as_deref() == Some("blocked") {
            self.error
                .as_deref()
                .unwrap_or("This shell may not connect to the helper; run atlas outside a sandbox.")
        } else {
            "Start the helper: run atlas init (safe to run again)."
        }
    }

    fn hidden_guidance(&self, uid: u32) -> &str {
        if self.uid.is_some_and(|v| v != u64::from(uid)) {
            "A different account answered than the configured helper; run atlas init."
        } else if self.on_console == Some(true) || self.proof.on_console == Some(true) {
            "Switch back to your own account without logging the helper out."
        } else {
            // The session check's own reason, when it gave one, says what to fix.
            self.proof.error.as_deref().unwrap_or(
                "The helper could not be checked. Run atlas restart; if that does not help, \
             run atlas init.",
            )
        }
    }
}

/// Why a helper that did not answer is down, when this side can tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Absence {
    /// The configured account no longer exists.
    Missing,
    /// The account (named here) exists but no desktop of it runs: it was
    /// logged out, or the Mac restarted.
    LoggedOut(String),
}

/// The doctor's step when input is down or pinned to a display that is
/// gone, which Recover also takes.
pub const START_INPUT: &str = "Run atlas start-input.";

/// The doctor's step for a logged-out helper.
pub const LOGGED_OUT: &str = "The helper is logged out (after a restart or a log-out). Log in \
     to it once, then switch back: atlas switch locks the screen for that.";

impl Absence {
    /// Its name in a report, as the overview reads it.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::LoggedOut(_) => "loggedOut",
        }
    }

    /// Put this in a doctor report: `absence` and `account` for the
    /// overview, and the running check's step (in its place among the
    /// next steps) in words for the command line.
    ///
    /// Objects and legacy null reports are enriched. Other roots are
    /// replaced with a certain state-error report.
    pub fn explain(&self, report: &mut Value) {
        if let Err(error) = require_report(report) {
            *report = json!(error);
            return;
        }
        report["absence"] = json!(self.as_str());

        let step = match self {
            Self::Missing => {
                "The helper account no longer exists. Run atlas init to set up a new one."
            }
            Self::LoggedOut(name) => {
                report["account"] = json!(name);
                LOGGED_OUT
            }
        };

        let running = report["checks"].as_array_mut().and_then(|checks| {
            checks
                .iter_mut()
                .find(|check| check["name"] == "Helper running")
        });
        if let Some(check) = running {
            let old = std::mem::replace(&mut check["nextStep"], json!(step));
            if let Some(listed) = report["nextSteps"]
                .as_array_mut()
                .and_then(|steps| steps.iter_mut().find(|listed| **listed == old))
            {
                *listed = json!(step);
            }
        }
    }
}

/// Whether `status` is a helper that did not answer, which this side may
/// explain: not refused by this shell, not answered by another account, and
/// not a helper too busy to serve one more caller.
pub fn silent(status: &Value) -> bool {
    status["ok"] != true && connection_refusal(status["code"].as_str()).is_none()
}
