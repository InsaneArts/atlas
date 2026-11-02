//! The facts an action is tied to, free of how they were observed: the
//! display, the session's [`Epoch`], and the proof a session gives of them.
use crate::{
    account::FIRST_USER_UID,
    error::{Code, Error, Result},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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

/// The largest display side, in points, a session may report.
const MAX_SIDE: f64 = 32768.0;
/// The largest backing scale a display may report.
const MAX_SCALE: f64 = 8.0;

impl Display {
    pub(crate) fn valid(&self) -> bool {
        within(self.width, MAX_SIDE)
            && within(self.height, MAX_SIDE)
            && within(self.scale, MAX_SCALE)
    }
}

/// Whether `value` is a finite measure above zero and at most `max`.
fn within(value: f64, max: f64) -> bool {
    value.is_finite() && value > 0.0 && value <= max
}

/// The facts a proof supplied, before any caller's identity is trusted.
#[derive(Clone, Debug)]
pub(super) struct ProofObservation {
    pub(super) error: Option<String>,
    ok: Option<bool>,
    uid: Option<u64>,
    pub(super) parent_pid: Option<u64>,
    pub(super) on_console: Option<bool>,
    graphic_access: Option<bool>,
    security_session_id: Option<u64>,
    display_id: Option<u64>,
    pub(super) width: Option<f64>,
    pub(super) height: Option<f64>,
}

impl ProofObservation {
    pub(super) fn decode(proof: &Value) -> Self {
        Self {
            error: proof["error"].as_str().map(str::to_owned),
            ok: proof["ok"].as_bool(),
            uid: proof["uid"].as_u64(),
            parent_pid: proof["parentPID"].as_u64(),
            on_console: proof["onConsole"].as_bool(),
            graphic_access: proof["hasGraphicAccess"].as_bool(),
            security_session_id: proof["securitySessionID"].as_u64(),
            display_id: proof["displayID"].as_u64(),
            width: proof["width"].as_f64(),
            height: proof["height"].as_f64(),
        }
    }

    /// macOS IDs are 32-bit; zero and callerSecuritySession are not sessions.
    fn session_ids(&self) -> Option<(u64, u64)> {
        let security = self
            .security_session_id
            .filter(|n| *n > 0 && *n < u32::MAX.into())?;
        let display = self
            .display_id
            .filter(|n| *n > 0 && *n <= u32::MAX.into())?;
        Some((security, display))
    }

    pub(super) fn input_epoch(&self, process_id: u32) -> Result<InputEpoch> {
        let invalid = || {
            Error::new(
                Code::Identity,
                "A hidden graphical session with valid geometry is required",
            )
        };
        let (security_session_id, display_id) = self.session_ids().ok_or_else(invalid)?;
        let epoch = InputEpoch {
            uid: self
                .uid
                .and_then(|v| u32::try_from(v).ok())
                .filter(|v| *v >= FIRST_USER_UID)
                .ok_or_else(invalid)?,
            security_session_id,
            display_id,
            width: self
                .width
                .filter(|v| within(*v, MAX_SIDE))
                .ok_or_else(invalid)?,
            height: self
                .height
                .filter(|v| within(*v, MAX_SIDE))
                .ok_or_else(invalid)?,
            process_id,
        };
        if self.ok != Some(true)
            || self.on_console != Some(false)
            || self.graphic_access != Some(true)
            || process_id == 0
        {
            return Err(invalid());
        }
        Ok(epoch)
    }
}

impl InputEpoch {
    /// Decode a hidden graphical-session observation into an input epoch.
    ///
    /// # Errors
    ///
    /// Rejects incomplete, visible, non-graphical or invalid identity and geometry facts, and a
    /// zero process ID. A decoded proof is a snapshot, never authority for a later session.
    pub fn from_proof(proof: &Value, process_id: u32) -> Result<Self> {
        ProofObservation::decode(proof).input_epoch(process_id)
    }
}
