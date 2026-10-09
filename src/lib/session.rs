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
    // SAFETY: getuid has no pointer arguments or preconditions.
    validate_target_uid(uid, unsafe { libc::getuid() })
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
        input: value["permissions"]["accessibility"] == true
            && value["inputService"]["ok"] == true
            && value["inputService"]["protocolVersion"] == 1
            && value["inputService"]["ready"] == true
            && value["inputService"]["accessibility"] == true
            && native_matches,
    })
}

pub fn diagnose(value: Value, uid: u32) -> Value {
    let session = require_hidden(&value, uid).ok();
    let reachable = value["ok"] == true;
    let identity_guidance = value["sessionProof"]["error"].as_str().unwrap_or(
        "The graphical-session proof must match this hidden helper. \
         Switch back without logging it out, then refresh.",
    );
    let checks = [
        (
            "Helper connection",
            reachable,
            "Start the configured helper in its logged-in account.",
        ),
        ("Hidden desktop", session.is_some(), identity_guidance),
        (
            "Screen recording",
            reachable && value["permissions"]["screenCapture"] == true,
            "Enable Screen & System Audio Recording for the installed helper in its account.",
        ),
        (
            "Mouse and keyboard",
            reachable && value["permissions"]["accessibility"] == true,
            "Enable Accessibility for the installed helper in its account.",
        ),
        (
            "Native input service",
            session.as_ref().is_some_and(|session| session.input),
            "Run understudy start-input to start the Rust input process inside the prepared helper.",
        ),
    ];
    let details = checks
        .iter()
        .map(|(name, ok, fix)| {
            json!({
                "name": name,
                "ok": ok,
                "nextStep": if *ok { Value::Null } else { json!(fix) },
            })
        })
        .collect::<Vec<_>>();
    let next_steps = checks
        .iter()
        .filter(|(_, ok, _)| !ok)
        .map(|(_, _, fix)| fix)
        .collect::<Vec<_>>();
    json!({
        "ok": true,
        "ready": checks.iter().all(|(_, ok, _)| *ok),
        "canObserve": session.is_some_and(|s| s.capture),
        "checks": details,
        "nextSteps": next_steps,
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
