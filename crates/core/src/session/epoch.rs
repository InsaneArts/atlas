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

/// The display epoch a session is in. It serializes as the flat record a
/// capture keeps: its display's fields beside the others, in camelCase.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Epoch {
    pub uid: u32,
    pub capture_pid: u32,
    #[serde(flatten)]
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

/// Where a proved session's screen must be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Console {
    /// Off the console: the second login, kept behind the person's own.
    Off,
    /// On its own console: a machine of its own, whose screen is nobody
    /// else's.
    Own,
}

/// Where this process's own session keeps its screen, fixed once at start.
static KEPT: std::sync::OnceLock<Console> = std::sync::OnceLock::new();

impl Console {
    /// Whether a session `on_console` is away from where this screen must
    /// be: a hidden login shown, or a machine's own session not shown.
    #[must_use]
    pub const fn away(self, on_console: bool) -> bool {
        match self {
            Self::Off => on_console,
            Self::Own => !on_console,
        }
    }

    /// Where this process's session keeps its screen: off the console,
    /// unless the process said otherwise at start. Once read, it is fixed.
    pub fn kept() -> Self {
        *KEPT.get_or_init(|| Self::Off)
    }

    /// Keep this process's session's screen here for its whole life, before
    /// any proof is read.
    ///
    /// # Errors
    ///
    /// Refuses once a place was kept or assumed, even the same one: a
    /// proof may already have been read against it.
    pub fn keep(self) -> Result<()> {
        KEPT.set(self).map_err(|_| {
            Error::new(
                Code::Identity,
                "This process's screen was already kept elsewhere.",
            )
        })
    }
}

/// The epoch a machine desktop's session is in, compared by exact
/// equality: a new boot, security session, display or process is another
/// epoch, and nothing remembered of the last holds in it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineEpoch {
    pub uid: u32,
    /// The guest's boot, as it reports it.
    pub boot: String,
    pub capture_pid: u32,
    #[serde(flatten)]
    pub display: Display,
    pub security_session_id: u64,
    pub display_id: u64,
    pub input_pid: u32,
}

impl MachineEpoch {
    /// The epoch the guest's input service holds every acquire, input and
    /// look in this session to.
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
    /// The display's backing pixels per point, which a machine desktop's
    /// proof is held to; a second login's is not.
    pub(super) scale: Option<f64>,
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
            scale: proof["scale"].as_f64(),
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

    /// The input epoch the proof shows for `process_id`, its session's
    /// screen where `console` says it must be.
    pub(super) fn input_epoch(&self, process_id: u32, console: Console) -> Result<InputEpoch> {
        let invalid = || {
            Error::new(
                Code::Identity,
                match console {
                    Console::Off => "A hidden graphical session with valid geometry is required",
                    Console::Own => {
                        "A graphical session on its own screen with valid geometry is required"
                    }
                },
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
            || self.on_console != Some(console == Console::Own)
            || self.graphic_access != Some(true)
            || process_id == 0
        {
            return Err(invalid());
        }
        Ok(epoch)
    }
}

impl InputEpoch {
    /// Decode a graphical-session observation into an input epoch, its
    /// screen where this process keeps it.
    ///
    /// # Errors
    ///
    /// Rejects incomplete, visible, non-graphical or invalid identity and geometry facts, and a
    /// zero process ID. A decoded proof is a snapshot, never authority for a later session.
    pub fn from_proof(proof: &Value, process_id: u32) -> Result<Self> {
        Self::from_proof_kept(proof, process_id, Console::kept())
    }

    /// [`Self::from_proof`] for a session whose screen must be where
    /// `console` says, whatever this process keeps.
    ///
    /// # Errors
    ///
    /// As [`Self::from_proof`].
    pub fn from_proof_kept(proof: &Value, process_id: u32, console: Console) -> Result<Self> {
        ProofObservation::decode(proof).input_epoch(process_id, console)
    }
}
