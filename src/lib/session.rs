use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Display {
    pub width: f64,
    pub height: f64,
    pub scale: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Epoch {
    pub uid: u32,
    pub capture_pid: u32,
    pub display: Display,
    pub security_session_id: u64,
    pub display_id: u64,
    pub input_pid: u32,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputEpoch {
    pub uid: u32,
    pub security_session_id: u64,
    pub display_id: u64,
    pub width: f64,
    pub height: f64,
    pub process_id: u32,
}
impl Epoch {
    pub fn input_epoch(&self) -> InputEpoch {
        InputEpoch {
            uid: self.uid,
            security_session_id: self.security_session_id,
            display_id: self.display_id,
            width: self.display.width,
            height: self.display.height,
            process_id: self.input_pid,
        }
    }
}
impl InputEpoch {
    pub fn from_proof(proof: &Value, process_id: u32) -> Result<Self> {
        let invalid = || {
            Error::new(
                "identity",
                "A hidden graphical session with valid geometry is required",
            )
        };
        let epoch = Self {
            uid: proof["uid"]
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .filter(|v| *v >= 501)
                .ok_or_else(invalid)?,
            security_session_id: proof["securitySessionID"]
                .as_u64()
                .filter(|v| *v > 0 && *v < u32::MAX.into())
                .ok_or_else(invalid)?,
            display_id: proof["displayID"]
                .as_u64()
                .filter(|v| *v > 0 && *v <= u32::MAX.into())
                .ok_or_else(invalid)?,
            width: proof["width"]
                .as_f64()
                .filter(|v| v.is_finite() && *v > 0.0 && *v <= 32768.0)
                .ok_or_else(invalid)?,
            height: proof["height"]
                .as_f64()
                .filter(|v| v.is_finite() && *v > 0.0 && *v <= 32768.0)
                .ok_or_else(invalid)?,
            process_id,
        };
        if proof["ok"] != true
            || proof["onConsole"] != false
            || proof["hasGraphicAccess"] != true
            || process_id == 0
        {
            return Err(invalid());
        }
        Ok(epoch)
    }
}
#[derive(Clone, Debug)]
pub struct Session {
    pub epoch: Epoch,
    pub capture: bool,
    pub input: bool,
}

pub fn validate_uid(uid: u32) -> Result<()> {
    validate_target_uid(uid, nix::unistd::getuid().as_raw())
}

pub(crate) fn validate_target_uid(uid: u32, controller_uid: u32) -> Result<()> {
    if uid < 501 || uid == controller_uid {
        return Err(Error::new(
            "identity",
            "Target must be a distinct helper account with UID >= 501",
        ));
    }
    Ok(())
}

pub fn require_hidden(value: &Value, uid: u32) -> Result<Session> {
    validate_uid(uid)?;
    // The common cases in plain words; anything else is one unverified state.
    if value["ok"] != true {
        // A socket served by the wrong account stays an identity violation.
        if value["code"] == "identity" {
            return Err(Error::new(
                "identity",
                value["error"]
                    .as_str()
                    .unwrap_or("The helper socket belongs to another account"),
            ));
        }
        return Err(Error::new(
            "transport",
            "The helper is not answering. Run understudy doctor.",
        ));
    }
    if value["user"]["uid"]
        .as_u64()
        .is_some_and(|other| other != u64::from(uid))
    {
        return Err(Error::new(
            "identity",
            "A different account answered than the configured helper. Run understudy doctor.",
        ));
    }
    if value["session"]["onConsole"] == true || value["sessionProof"]["onConsole"] == true {
        return Err(Error::new(
            "on-console",
            "The helper is on screen right now. Understudy works only while it is in the \
             background: switch back to your own account.",
        ));
    }
    let invalid = || {
        Error::new(
            "identity",
            "The helper's session could not be verified. Run understudy doctor.",
        )
    };
    let pid = value["daemon"]["pid"]
        .as_u64()
        .filter(|p| *p > 0 && *p <= u32::MAX as u64)
        .ok_or_else(invalid)? as u32;
    let display: Display =
        serde_json::from_value(value["display"].clone()).map_err(|_| invalid())?;
    if value["ok"] != true
        || value["user"]["uid"].as_u64() != Some(uid.into())
        || value["session"]["onConsole"] != false
        || value["session"]["managerName"] != "Aqua"
        || ![display.width, display.height, display.scale]
            .into_iter()
            .zip([32768.0, 32768.0, 8.0])
            .all(|(n, max)| n.is_finite() && n > 0.0 && n <= max)
    {
        return Err(invalid());
    }
    let proof = &value["sessionProof"];
    let security = proof["securitySessionID"]
        .as_u64()
        .filter(|n| *n > 0 && *n < u32::MAX.into())
        .ok_or_else(invalid)?;
    let display_id = proof["displayID"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= u32::MAX.into())
        .ok_or_else(invalid)?;
    if proof["parentPID"].as_u64() != Some(pid.into())
        || proof["ok"] != true
        || proof["uid"].as_u64() != Some(uid.into())
        || proof["hasGraphicAccess"] != true
        || proof["onConsole"] != false
        || proof["width"].as_f64() != Some(display.width)
        || proof["height"].as_f64() != Some(display.height)
    {
        return Err(invalid());
    }
    let input_pid = value["inputService"]["pid"]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .filter(|v| *v > 0)
        .unwrap_or(0);
    let native_epoch = InputEpoch::from_proof(&value["inputService"]["session"], input_pid).ok();
    let epoch = Epoch {
        uid,
        capture_pid: pid,
        display,
        security_session_id: security,
        display_id,
        input_pid,
    };
    let native_matches = native_epoch.as_ref() == Some(&epoch.input_epoch());
    Ok(Session {
        epoch,
        capture: value["permissions"]["screenCapture"] == true,
        input: value["inputService"]["ok"] == true
            && value["inputService"]["protocolVersion"] == 1
            && value["inputService"]["ready"] == true
            && value["inputService"]["accessibility"] == true
            && native_matches,
    })
}

struct Check {
    name: &'static str,
    /// None when the fact cannot be observed yet; never reported as missing.
    ok: Option<bool>,
    next_step: String,
}

/// Separate observation, input and native-dialog capability. `ready` keeps its
/// established meaning (transport, identity, permissions, input service); it
/// never implies that native Save/Open dialogs work.
/// Whether `status` shows input service `pid` ready in the hidden helper with
/// no controller and nothing held.
pub fn input_settled(status: &Value, uid: u32, pid: u32) -> bool {
    let native = &status["inputService"];
    require_hidden(status, uid).is_ok_and(|session| session.input && session.epoch.input_pid == pid)
        && native["controlActive"] == false
        && native["heldInput"] == false
}

/// Whether the input service runs the installed build from the file still
/// in place (a replaced file leaves even the same build stale).
pub fn input_build_current(status: &Value) -> bool {
    let native = &status["inputService"];
    native["build"].is_string()
        && native["build"] == status["installedBuild"]
        && native["replaced"] != true
}

/// [`input_settled`], and running the installed build.
pub fn input_current(status: &Value, uid: u32, pid: u32) -> bool {
    input_settled(status, uid, pid) && input_build_current(status)
}

pub fn diagnose(value: Value, uid: u32) -> Value {
    let session = require_hidden(&value, uid).ok();
    let reachable = value["ok"] == true;
    let native = &value["inputService"];
    let native_reachable = native["ok"] == true;
    // The input service is only queried once the capture side proves a hidden
    // helper; an unqueried service is unknown, not failed.
    let queried = reachable && !native.is_null();
    let not_queried = "Unknown until the helper runs in the background.";
    let known = |fact: bool, ok: bool| fact.then_some(ok);
    let unknown = "Unknown until the helper is running.";
    let connection = if value["code"] == "identity" {
        value["error"]
            .as_str()
            .unwrap_or("The socket belongs to another account")
    } else {
        "Start the helper: run understudy init (safe to run again)."
    };
    let hidden_guidance = if value["user"]["uid"]
        .as_u64()
        .is_some_and(|v| v != u64::from(uid))
    {
        "A different account answered than the configured helper; run understudy init."
    } else if value["session"]["onConsole"] == true || value["sessionProof"]["onConsole"] == true {
        "Switch back to your own account without logging the helper out."
    } else {
        // The session check's own reason, when it gave one, says what to fix.
        value["sessionProof"]["error"].as_str().unwrap_or(
            "The helper could not be checked. Run understudy restart; if that does not help, \
             run understudy init.",
        )
    };
    let checks = [
        Check {
            name: "Helper running",
            ok: Some(reachable),
            next_step: connection.into(),
        },
        Check {
            name: "Helper in the background",
            ok: known(reachable, session.is_some()),
            next_step: if reachable { hidden_guidance } else { unknown }.into(),
        },
        Check {
            name: "Screen Recording allowed",
            ok: known(reachable, value["permissions"]["screenCapture"] == true),
            next_step: if reachable {
                "In the helper's System Settings › Privacy & Security › Screen & System Audio \
                 Recording, turn Understudy on."
            } else {
                unknown
            }
            .into(),
        },
        Check {
            name: "Accessibility allowed",
            ok: known(native_reachable, native["accessibility"] == true),
            next_step: if native_reachable {
                "In the helper's System Settings › Privacy & Security › Accessibility, turn \
                 Understudy on."
            } else if queried {
                "Run understudy start-input."
            } else {
                not_queried
            }
            .into(),
        },
        Check {
            name: "Input service ready",
            ok: known(queried, session.as_ref().is_some_and(|s| s.input)),
            next_step: if queried {
                "Run understudy start-input."
            } else if reachable {
                not_queried
            } else {
                unknown
            }
            .into(),
        },
    ];
    let mut warnings = Vec::new();
    if native_reachable && !input_build_current(&value) {
        warnings.push(
            "The input service is from an older build; understudy start-input replaces it."
                .to_owned(),
        );
    }
    let details = checks
        .iter()
        .map(|check| {
            json!({
                "name": check.name,
                "ok": check.ok,
                "nextStep": (check.ok != Some(true)).then_some(&check.next_step),
            })
        })
        .collect::<Vec<_>>();
    let next_steps = checks
        .iter()
        .filter(|check| check.ok == Some(false))
        .map(|check| &check.next_step)
        .collect::<Vec<_>>();
    let observe = session.as_ref().is_some_and(|s| s.capture);
    json!({
        "ok": true,
        "ready": checks.iter().all(|check| check.ok == Some(true)),
        "canObserve": observe,
        "capabilities": {
            "observe": observe,
            "input": session.as_ref().is_some_and(|s| s.input),
            // Transport, identity and permissions were all green in Phase 1
            // while native Save/Open failed. Only a fresh native check counts.
            "nativeDialogs": {
                "state": "unknown",
                "reason": "No fresh native Save/Open observation. Native dialogs were qualified \
                           only with an authenticated Screen Sharing session into the helper.",
            },
        },
        "checks": details,
        "nextSteps": next_steps,
        "warnings": warnings,
        "session": value,
    })
}

#[cfg(test)]
mod tests {
    use super::{input_build_current, validate_target_uid};
    use serde_json::json;

    /// A service from a replaced file is stale even when its build matches.
    #[test]
    fn a_replaced_input_service_is_not_current() {
        let status = |replaced: bool| {
            json!({
                "installedBuild": "abc",
                "inputService": {"build": "abc", "replaced": replaced},
            })
        };
        assert!(input_build_current(&status(false)));
        assert!(!input_build_current(&status(true)));
        let mut older = status(false);
        older["inputService"]["build"] = json!("old");
        assert!(!input_build_current(&older));
        let mut legacy = status(false);
        legacy["inputService"]
            .as_object_mut()
            .unwrap()
            .remove("replaced");
        assert!(
            input_build_current(&legacy),
            "services that predate the field"
        );
    }

    #[test]
    fn helper_may_be_the_first_normal_account_but_never_the_controller() {
        assert!(validate_target_uid(501, 502).is_ok());
        assert!(validate_target_uid(502, 501).is_ok());
        assert!(validate_target_uid(501, 501).is_err());
        assert!(validate_target_uid(502, 502).is_err());
        assert!(validate_target_uid(500, 502).is_err());
        assert!(validate_target_uid(0, 502).is_err());
    }
}
