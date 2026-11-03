//! What the helper's status proves about its session: the display and the
//! processes an action is tied to (its [`Epoch`]), whether the helper is the
//! hidden graphical login it must be, and the doctor report built from it.
pub mod diagnosis;
mod epoch;
mod logins;

pub use diagnosis::{Absence, silent};
pub use diagnosis::{Diagnosis, Guidance, LOGGED_OUT, require_report};
#[cfg(any(test, feature = "test-support"))]
pub use diagnosis::{START_INPUT, diagnosis};
pub use epoch::{Display, Epoch, InputEpoch};
#[cfg(any(test, feature = "test-support"))]
pub use logins::Window;
pub use logins::{
    Blocker, blockers_now, confirm_gone, console_owner, desktop_runs, desktop_up, logged_out,
    login, setup_assistant, stale,
};
pub use logins::{Kind, Sessions, absence, logged_in, sessions};
#[cfg(test)]
pub(crate) use logins::{Record, classify};

use crate::account::FIRST_USER_UID;
use crate::error::{Code, Error, Result};
use epoch::ProofObservation;
use serde::Deserialize;
use serde_json::Value;

/// A pin may be incomplete; readiness never fills in its missing fields.
#[derive(Clone, Debug)]
struct PinObservation {
    uid: Option<u64>,
    security_session_id: Option<u64>,
    process_id: Option<u64>,
    width: Option<f64>,
    height: Option<f64>,
}

impl PinObservation {
    fn decode(value: &Value) -> Self {
        Self {
            uid: value["uid"].as_u64(),
            security_session_id: value["securitySessionId"].as_u64(),
            process_id: value["processId"].as_u64(),
            width: value["width"].as_f64(),
            height: value["height"].as_f64(),
        }
    }
}

fn capability_names(value: &Value) -> Vec<String> {
    value["capabilities"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

#[derive(Clone, Debug)]
pub struct NativeObservation {
    pub ok: bool,
    queried: bool,
    protocol_version: Option<u64>,
    pub pid: Option<u32>,
    pub ready: Option<bool>,
    /// Whether Accessibility is allowed; None when not asked or not answered.
    pub accessibility: Option<bool>,
    epoch: Option<InputEpoch>,
    pinned: PinObservation,
    pub control_active: Option<bool>,
    pub held: bool,
    pub held_input: Option<bool>,
    build: Option<String>,
    replaced: bool,
    capabilities: Vec<String>,
}

impl NativeObservation {
    pub fn decode(value: &Value) -> Self {
        let pid = value["pid"]
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .filter(|v| *v > 0);
        Self {
            ok: value["ok"] == true,
            queried: !value.is_null(),
            protocol_version: value["protocolVersion"].as_u64(),
            pid,
            ready: value["ready"].as_bool(),
            accessibility: value["accessibility"].as_bool(),
            epoch: ProofObservation::decode(&value["session"])
                .input_epoch(pid.unwrap_or(0))
                .ok(),
            pinned: PinObservation::decode(&value["enforcedEpoch"]),
            control_active: value["controlActive"].as_bool(),
            held: value["held"] == true,
            held_input: value["heldInput"].as_bool(),
            build: value["build"].as_str().map(str::to_owned),
            replaced: value["replaced"] == true,
            capabilities: capability_names(value),
        }
    }

    fn healthy(&self) -> bool {
        self.ok && self.protocol_version == Some(1) && self.accessibility == Some(true)
    }

    pub fn has_capability(&self, name: &str) -> bool {
        self.capabilities
            .iter()
            .any(|capability| capability == name)
    }

    pub fn pinned_to_session(&self, session: &Session) -> bool {
        let epoch = &session.epoch;
        self.healthy()
            && self.pinned.uid == Some(epoch.uid.into())
            && self.pinned.security_session_id == Some(epoch.security_session_id)
            && self.pinned.process_id == Some(epoch.input_pid.into())
    }

    pub fn repinnable(&self, session: &Session) -> bool {
        self.pinned_to_session(session)
            && self.pinned.width == Some(session.epoch.display.width)
            && self.pinned.height == Some(session.epoch.display.height)
    }
}

/// One status read, including facts that cannot yet prove a complete session.
/// Keeping it never authorizes an effect against a later session.
#[derive(Clone, Debug)]
pub struct Observation {
    pub ok: bool,
    pub code: Option<String>,
    error: Option<String>,
    uid: Option<u64>,
    home: Option<std::path::PathBuf>,
    on_console: Option<bool>,
    aqua: bool,
    pub capture_pid: Option<u32>,
    display: Option<Display>,
    proof: ProofObservation,
    /// Whether Screen Recording is allowed; None when the read failed or did not say.
    capture: Option<bool>,
    /// Whether Accessibility is allowed; None when the read did not say.
    pub accessibility: Option<bool>,
    daemon_capabilities: Vec<String>,
    pub native: NativeObservation,
    pub installed_build: Option<String>,
}

impl Observation {
    pub fn decode(value: &Value) -> Self {
        let ok = value["ok"] == true;
        Self {
            ok,
            code: value["code"].as_str().map(str::to_owned),
            error: value["error"].as_str().map(str::to_owned),
            uid: value["user"]["uid"].as_u64(),
            home: value["user"]["home"].as_str().map(Into::into),
            on_console: value["session"]["onConsole"].as_bool(),
            aqua: value["session"]["managerName"] == "Aqua",
            capture_pid: value["daemon"]["pid"]
                .as_u64()
                .and_then(|pid| u32::try_from(pid).ok())
                .filter(|pid| *pid > 0),
            display: Display::deserialize(&value["display"]).ok(),
            proof: ProofObservation::decode(&value["sessionProof"]),
            capture: value["permissions"]["screenCapture"]
                .as_bool()
                .filter(|_| ok),
            accessibility: value["permissions"]["accessibility"].as_bool(),
            daemon_capabilities: capability_names(&value["daemon"]),
            native: NativeObservation::decode(&value["inputService"]),
            installed_build: value["installedBuild"].as_str().map(str::to_owned),
        }
    }

    pub fn display_valid(&self) -> bool {
        self.display.as_ref().is_some_and(Display::valid)
    }

    pub fn has_capability(&self, name: &str) -> bool {
        self.daemon_capabilities
            .iter()
            .any(|capability| capability == name)
    }

    /// # Errors
    ///
    /// Returns an error when the helper's home is unknown.
    pub fn helper_home(&self) -> Result<&std::path::Path> {
        self.home
            .as_deref()
            .filter(|home| home.is_absolute())
            .ok_or_else(|| {
                Error::new(
                    Code::Identity,
                    "The helper home is unknown; run atlas doctor",
                )
            })
    }

    pub fn settled_in(&self, session: &Session, pid: u32) -> bool {
        session.input
            && session.epoch.input_pid == pid
            && self.native.control_active == Some(false)
            && self.native.held_input == Some(false)
    }

    pub fn input_build_current(&self) -> bool {
        self.native.build.is_some()
            && self.native.build == self.installed_build
            && !self.native.replaced
    }

    pub(crate) fn input_current(&self, uid: u32, pid: u32) -> bool {
        self.require_hidden(uid)
            .is_ok_and(|session| self.settled_in(&session, pid))
            && self.input_build_current()
    }

    /// # Errors
    ///
    /// Returns a refusal when the answer is not the hidden helper `uid`.
    pub fn require_hidden(&self, uid: u32) -> Result<Session> {
        validate_uid(uid)?;
        self.require_answer()?;

        if self.uid.is_some_and(|other| other != u64::from(uid)) {
            return Err(Error::new(
                Code::Identity,
                "A different account answered than the configured helper. Run atlas doctor.",
            ));
        }

        if self.on_console == Some(true) || self.proof.on_console == Some(true) {
            return Err(Error::new(
                Code::OnConsole,
                "The helper is on screen right now. Atlas works only while it is in the \
             background: switch back to your own account.",
            ));
        }

        let invalid = || {
            Error::new(
                Code::Identity,
                "The helper's session could not be verified. Run atlas doctor.",
            )
        };

        let pid = self.capture_pid.ok_or_else(invalid)?;
        let display = self.display.clone().ok_or_else(invalid)?;

        // `ok` is already known true here.
        if self.uid != Some(uid.into())
            || self.on_console != Some(false)
            || !self.aqua
            || !display.valid()
        {
            return Err(invalid());
        }

        let proof = self.proof.input_epoch(pid).map_err(|_| invalid())?;
        if self.proof.parent_pid != Some(pid.into())
            || proof.uid != uid
            || proof.width != display.width
            || proof.height != display.height
        {
            return Err(invalid());
        }

        let input_pid = self.native.pid.unwrap_or(0);
        let epoch = Epoch {
            uid,
            capture_pid: pid,
            display,
            security_session_id: proof.security_session_id,
            display_id: proof.display_id,
            input_pid,
        };
        let native_matches = self.native.epoch.as_ref() == Some(&epoch.input_epoch());

        Ok(Session {
            epoch,
            capture: self.capture == Some(true),
            input: self.native.healthy() && self.native.ready == Some(true) && native_matches,
        })
    }

    /// Refuse a status that is no answer, the common cases in plain words.
    fn require_answer(&self) -> Result<()> {
        if self.ok {
            return Ok(());
        }
        if let Some((code, fallback)) = connection_refusal(self.code.as_deref()) {
            return Err(Error::new(code, self.error.as_deref().unwrap_or(fallback)));
        }

        let cause = self
            .error
            .as_deref()
            .map_or_else(String::new, |cause| format!(": {cause}"));
        Err(Error::new(
            Code::Transport,
            format!("The helper is not answering{cause}. Run atlas doctor."),
        ))
    }
}

/// These refusals prove neither silence nor a stopped helper.
fn connection_refusal(code: Option<&str>) -> Option<(Code, &'static str)> {
    Some(match code? {
        "blocked" => (Code::Blocked, "This shell may not connect to the helper"),
        "busy" => (
            Code::Busy,
            "The helper is serving too many requests; try again",
        ),
        "identity" => (
            Code::Identity,
            "The helper socket belongs to another account",
        ),
        _ => return None,
    })
}

#[derive(Clone, Debug)]
pub struct Session {
    pub epoch: Epoch,
    pub capture: bool,
    pub input: bool,
}

/// # Errors
///
/// Returns an error when `uid` can't be a helper account.
pub fn validate_uid(uid: u32) -> Result<()> {
    validate_target_uid(uid, nix::unistd::getuid().as_raw())
}

/// # Errors
///
/// Returns an error when `uid` can't be a helper account or is
/// `controller_uid`.
pub fn validate_target_uid(uid: u32, controller_uid: u32) -> Result<()> {
    if uid < FIRST_USER_UID || uid == controller_uid {
        return Err(Error::new(
            Code::Identity,
            "Target must be a distinct helper account with UID >= 501",
        ));
    }

    Ok(())
}

/// Verify that a status observation describes the configured hidden graphical login.
///
/// # Errors
///
/// Rejects an invalid or non-distinct helper UID, unavailable status, a visible helper or
/// incomplete graphical-session proof. Input availability is reported separately on the
/// returned session.
pub fn require_hidden(value: &Value, uid: u32) -> Result<Session> {
    Observation::decode(value).require_hidden(uid)
}

/// Whether the input service runs the installed build from the file still
/// in place (a replaced file leaves even the same build stale).
pub fn input_build_current(status: &Value) -> bool {
    Observation::decode(status).input_build_current()
}

/// How long the input service's grant of control lasts unless its holder
/// renews it, so how long one can outlive a holder that was killed.
pub const GRANT_TTL: std::time::Duration = std::time::Duration::from_secs(6);

/// The input-service capability to shut down when its controller asks.
pub const SHUTDOWN: &str = "shutdown";

/// The input-service capability to keep the person's Stop of agents.
pub const HOLD: &str = "hold";

/// The input-service capability that checks an agent's target app.
pub const FOCUS_APP: &str = "focus-app";

/// The input-service capability whose point check counts the Dock as
/// covering where it takes clicks. A service without it lets a click on a
/// Dock icon through.
pub const FOCUS_DOCK: &str = "focus-dock";

/// The input-service capability that checks the window an agent names. A
/// service without it would refuse the expectation, never ignore it.
pub const FOCUS_WINDOW: &str = "focus-window";

/// The input-service capability that checks the field an agent names.
pub const FOCUS_FIELD: &str = "focus-field";

/// The input-service capability that reads an app's elements and keeps
/// each look, so a click can name one element of it by reference.
pub const LOOKS: &str = "looks";

/// The input-service capability that acts on an element of a look through
/// Accessibility alone, and says whether it reads as asked afterwards.
pub const ELEMENT_ACTIONS: &str = "element-actions";

/// Whether an input service's hello lists `name` among its capabilities.
pub fn has_capability(native: &Value, name: &str) -> bool {
    capability_names(native)
        .iter()
        .any(|capability| capability == name)
}

/// Whether `status` shows input service `pid` ready in the hidden helper with
/// no controller and nothing held, running the installed build.
pub fn input_current(status: &Value, uid: u32, pid: u32) -> bool {
    Observation::decode(status).input_current(uid, pid)
}

#[cfg(test)]
mod tests;
