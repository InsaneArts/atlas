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
    let invalid = || {
        Error::new(
            "identity",
            "Expected the configured helper in a hidden graphical session with a valid display",
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

/// [`input_settled`], and running the installed build.
pub fn input_current(status: &Value, uid: u32, pid: u32) -> bool {
    let native = &status["inputService"];
    input_settled(status, uid, pid)
        && native["build"].is_string()
        && native["build"] == status["installedBuild"]
}

pub fn diagnose(value: Value, uid: u32) -> Value {
    let session = require_hidden(&value, uid).ok();
    let reachable = value["ok"] == true;
    let native = &value["inputService"];
    let native_reachable = native["ok"] == true;
    // The input service is only queried once the capture side proves a hidden
    // helper; an unqueried service is unknown, not failed.
    let queried = reachable && !native.is_null();
    let not_queried = "Unknown until the helper is hidden and its input service can be queried.";
    let known = |fact: bool, ok: bool| fact.then_some(ok);
    let unknown = "Unknown until the helper connection works.";
    let connection = if value["code"] == "identity" {
        value["error"]
            .as_str()
            .unwrap_or("The socket belongs to another account")
    } else {
        "Start the configured helper in its logged-in account."
    };
    let hidden_guidance = if value["user"]["uid"]
        .as_u64()
        .is_some_and(|v| v != u64::from(uid))
    {
        "The capture service belongs to a different account than the configured helper."
    } else if value["session"]["onConsole"] == true {
        "The helper is on the console. Switch back to your account without logging it out."
    } else {
        value["sessionProof"]["error"].as_str().unwrap_or(
            "The graphical-session proof must match this hidden helper. \
             Switch back without logging it out, then refresh.",
        )
    };
    let checks = [
        Check {
            name: "Helper connection",
            ok: Some(reachable),
            next_step: connection.into(),
        },
        Check {
            name: "Hidden desktop",
            ok: known(reachable, session.is_some()),
            next_step: if reachable { hidden_guidance } else { unknown }.into(),
        },
        Check {
            name: "Screen recording",
            ok: known(reachable, value["permissions"]["screenCapture"] == true),
            next_step: if reachable {
                "Enable Screen & System Audio Recording for the capture helper in its account."
            } else {
                unknown
            }
            .into(),
        },
        Check {
            name: "Input permission",
            ok: known(native_reachable, native["accessibility"] == true),
            next_step: if native_reachable {
                "Enable Accessibility for the input service in the helper account."
            } else if queried {
                "Unknown until the input service answers; run understudy start-input."
            } else {
                not_queried
            }
            .into(),
        },
        Check {
            name: "Native input service",
            ok: known(queried, session.as_ref().is_some_and(|s| s.input)),
            next_step: if queried {
                "Run understudy start-input to start or reset the input service inside the helper."
            } else if reachable {
                not_queried
            } else {
                unknown
            }
            .into(),
        },
    ];
    let mut warnings = Vec::new();
    if native_reachable
        && (!native["build"].is_string() || native["build"] != value["installedBuild"])
    {
        warnings.push(
            "The input service runs a different build than the installed executable; \
             run understudy start-input to replace it."
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
    use super::validate_target_uid;

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
