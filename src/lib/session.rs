//! What the helper's status proves about its session: the display and the
//! processes an action is tied to (its [`Epoch`]), whether the helper is the
//! hidden graphical login it must be, and the doctor report built from it.
mod logins;
#[cfg(test)]
use logins::*;
pub(crate) use logins::{
    Blocker, REUSED_SLACK, account_created, blockers_now, confirm_gone, console_owner,
    desktop_runs, desktop_up, logged_out, login, setup_assistant, stale,
};
pub use logins::{Kind, Sessions, absence, logged_in, sessions};
#[cfg(test)]
pub(crate) use logins::{Record, Window, classify};

use crate::error::{Code, Error, Result};
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
    pub(crate) uid: u32,
    pub(crate) capture_pid: u32,
    pub(crate) display: Display,
    pub(crate) security_session_id: u64,
    pub(crate) display_id: u64,
    pub input_pid: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputEpoch {
    pub(crate) uid: u32,
    pub(crate) security_session_id: u64,
    pub(crate) display_id: u64,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) process_id: u32,
}

impl Epoch {
    pub(crate) fn input_epoch(&self) -> InputEpoch {
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

/// The lowest UID macOS gives an account a person signs in to.
const FIRST_USER_UID: u32 = 501;
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
struct ProofObservation {
    error: Option<String>,
    ok: Option<bool>,
    uid: Option<u64>,
    parent_pid: Option<u64>,
    on_console: Option<bool>,
    graphic_access: Option<bool>,
    security_session_id: Option<u64>,
    display_id: Option<u64>,
    width: Option<f64>,
    height: Option<f64>,
}

impl ProofObservation {
    fn decode(proof: &Value) -> Self {
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

    fn input_epoch(&self, process_id: u32) -> Result<InputEpoch> {
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

/// Permission not asked about, not answered, refused, or granted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PermissionObservation {
    Unqueried,
    Unavailable,
    Denied,
    Allowed,
}

impl PermissionObservation {
    fn decode(value: &Value, queried: bool) -> Self {
        if !queried {
            Self::Unqueried
        } else {
            match value.as_bool() {
                Some(true) => Self::Allowed,
                Some(false) => Self::Denied,
                None => Self::Unavailable,
            }
        }
    }
}

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
pub(crate) struct NativeObservation {
    pub(crate) ok: bool,
    queried: bool,
    protocol_version: Option<u64>,
    pub(crate) pid: Option<u32>,
    pub(crate) ready: Option<bool>,
    pub(crate) accessibility: PermissionObservation,
    epoch: Option<InputEpoch>,
    pinned: PinObservation,
    pub(crate) control_active: Option<bool>,
    pub(crate) held: bool,
    pub(crate) held_input: Option<bool>,
    build: Option<String>,
    replaced: bool,
    capabilities: Vec<String>,
}

impl NativeObservation {
    pub(crate) fn decode(value: &Value) -> Self {
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
            accessibility: PermissionObservation::decode(&value["accessibility"], !value.is_null()),
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
        self.ok
            && self.protocol_version == Some(1)
            && self.accessibility == PermissionObservation::Allowed
    }

    pub(crate) fn has_capability(&self, name: &str) -> bool {
        self.capabilities
            .iter()
            .any(|capability| capability == name)
    }

    pub(crate) fn pinned_to_session(&self, session: &Session) -> bool {
        let epoch = &session.epoch;
        self.healthy()
            && self.pinned.uid == Some(epoch.uid.into())
            && self.pinned.security_session_id == Some(epoch.security_session_id)
            && self.pinned.process_id == Some(epoch.input_pid.into())
    }

    pub(crate) fn repinnable(&self, session: &Session) -> bool {
        self.pinned_to_session(session)
            && self.pinned.width == Some(session.epoch.display.width)
            && self.pinned.height == Some(session.epoch.display.height)
    }
}

/// One status read, including facts that cannot yet prove a complete session.
/// Keeping it never authorizes an effect against a later session.
#[derive(Clone, Debug)]
pub(crate) struct Observation {
    pub(crate) ok: bool,
    pub(crate) code: Option<String>,
    error: Option<String>,
    uid: Option<u64>,
    home: Option<std::path::PathBuf>,
    on_console: Option<bool>,
    aqua: bool,
    pub(crate) capture_pid: Option<u32>,
    display: Option<Display>,
    proof: ProofObservation,
    capture: PermissionObservation,
    pub(crate) accessibility: PermissionObservation,
    daemon_capabilities: Vec<String>,
    pub(crate) native: NativeObservation,
    pub(crate) installed_build: Option<String>,
}

impl Observation {
    pub(crate) fn decode(value: &Value) -> Self {
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
            capture: PermissionObservation::decode(&value["permissions"]["screenCapture"], ok),
            accessibility: PermissionObservation::decode(
                &value["permissions"]["accessibility"],
                true,
            ),
            daemon_capabilities: capability_names(&value["daemon"]),
            native: NativeObservation::decode(&value["inputService"]),
            installed_build: value["installedBuild"].as_str().map(str::to_owned),
        }
    }

    pub(crate) fn display_valid(&self) -> bool {
        self.display.as_ref().is_some_and(Display::valid)
    }

    pub(crate) fn has_capability(&self, name: &str) -> bool {
        self.daemon_capabilities
            .iter()
            .any(|capability| capability == name)
    }

    pub(crate) fn helper_home(&self) -> Result<&std::path::Path> {
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

    pub(crate) fn input_settled(&self, uid: u32, pid: u32) -> bool {
        self.require_hidden(uid)
            .is_ok_and(|session| self.settled_in(&session, pid))
    }

    pub(crate) fn settled_in(&self, session: &Session, pid: u32) -> bool {
        session.input
            && session.epoch.input_pid == pid
            && self.native.control_active == Some(false)
            && self.native.held_input == Some(false)
    }

    pub(crate) fn input_build_current(&self) -> bool {
        self.native.build.is_some()
            && self.native.build == self.installed_build
            && !self.native.replaced
    }

    pub(crate) fn input_current(&self, uid: u32, pid: u32) -> bool {
        self.input_settled(uid, pid) && self.input_build_current()
    }
    pub(crate) fn require_hidden(&self, uid: u32) -> Result<Session> {
        validate_uid(uid)?;

        // The common cases in plain words; anything else is one unverified state.
        if !self.ok {
            // A shell that may not reach the helper says so: it is not down.
            if self.code.as_deref() == Some("blocked") {
                return Err(Error::new(
                    Code::Blocked,
                    self.error
                        .as_deref()
                        .unwrap_or("This shell may not connect to the helper"),
                ));
            }

            // A helper turning callers away while it serves others is up, not
            // silent: the caller may try again.
            if self.code.as_deref() == Some("busy") {
                return Err(Error::new(
                    Code::Busy,
                    self.error
                        .as_deref()
                        .unwrap_or("The helper is serving too many requests; try again"),
                ));
            }

            // A socket served by the wrong account stays an identity violation.
            if self.code.as_deref() == Some("identity") {
                return Err(Error::new(
                    Code::Identity,
                    self.error
                        .as_deref()
                        .unwrap_or("The helper socket belongs to another account"),
                ));
            }

            let cause = self
                .error
                .as_deref()
                .map_or_else(String::new, |cause| format!(": {cause}"));
            return Err(Error::new(
                Code::Transport,
                format!("The helper is not answering{cause}. Run atlas doctor."),
            ));
        }

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

        let proof = &self.proof;
        let (security, display_id) = proof.session_ids().ok_or_else(invalid)?;

        if proof.parent_pid != Some(pid.into())
            || proof.ok != Some(true)
            || proof.uid != Some(uid.into())
            || proof.graphic_access != Some(true)
            || proof.on_console != Some(false)
            || proof.width != Some(display.width)
            || proof.height != Some(display.height)
        {
            return Err(invalid());
        }

        let input_pid = self.native.pid.unwrap_or(0);
        let epoch = Epoch {
            uid,
            capture_pid: pid,
            display,
            security_session_id: security,
            display_id,
            input_pid,
        };
        let native_matches = self.native.epoch.as_ref() == Some(&epoch.input_epoch());

        Ok(Session {
            epoch,
            capture: self.capture == PermissionObservation::Allowed,
            input: self.native.healthy() && self.native.ready == Some(true) && native_matches,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Session {
    pub epoch: Epoch,
    pub capture: bool,
    pub input: bool,
}

pub(crate) fn validate_uid(uid: u32) -> Result<()> {
    validate_target_uid(uid, nix::unistd::getuid().as_raw())
}

pub(crate) fn validate_target_uid(uid: u32, controller_uid: u32) -> Result<()> {
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

/// The concrete recovery action and the words accompanying other advice.
#[derive(Clone, Debug)]
pub(crate) enum Guidance {
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
    pub(crate) fn text(&self) -> &str {
        match self {
            Self::StartInput => START_INPUT,
            Self::Advice(text) => text,
        }
    }
}

/// Null retains the existing uninitialized-report projection. Other
/// non-object roots cannot receive report fields safely.
pub(crate) fn require_report(report: &Value) -> Result<()> {
    if report.is_object() || report.is_null() {
        Ok(())
    } else {
        Err(Error::new(
            Code::State,
            "A doctor report must be an object or null",
        ))
    }
}

pub(crate) struct Diagnosis {
    pub(crate) value: Value,
    pub(crate) observation: Observation,
    pub(crate) ready: bool,
    pub(crate) first_step: Guidance,
}

struct Check {
    name: &'static str,
    /// None when the fact cannot be observed yet; never reported as missing.
    ok: Option<bool>,
    next_step: Guidance,
}

/// Whether `status` shows input service `pid` ready in the hidden helper with
/// no controller and nothing held.
pub fn input_settled(status: &Value, uid: u32, pid: u32) -> bool {
    Observation::decode(status).input_settled(uid, pid)
}

/// Whether the input service runs the installed build from the file still
/// in place (a replaced file leaves even the same build stale).
pub fn input_build_current(status: &Value) -> bool {
    Observation::decode(status).input_build_current()
}

/// The input-service capability to shut down when its controller asks.
pub(crate) const SHUTDOWN: &str = "shutdown";

/// The input-service capability to keep the person's Stop of agents.
pub(crate) const HOLD: &str = "hold";

/// The input-service capability that checks an agent's target app.
pub(crate) const FOCUS_APP: &str = "focus-app";

/// The input-service capability whose point check counts the Dock as
/// covering where it takes clicks. A service without it lets a click on a
/// Dock icon through.
pub(crate) const FOCUS_DOCK: &str = "focus-dock";

/// The input-service capability that checks the window an agent names. A
/// service without it would refuse the expectation, never ignore it.
pub(crate) const FOCUS_WINDOW: &str = "focus-window";

/// The input-service capability that checks the field an agent names.
pub(crate) const FOCUS_FIELD: &str = "focus-field";

/// Whether an input service's hello lists `name` among its capabilities.
pub(crate) fn has_capability(native: &Value, name: &str) -> bool {
    NativeObservation::decode(native).has_capability(name)
}

/// [`input_settled`], and running the installed build.
pub fn input_current(status: &Value, uid: u32, pid: u32) -> bool {
    Observation::decode(status).input_current(uid, pid)
}

/// The doctor report for a status snapshot: each check, what to do about a
/// failed one, and `ready` for transport, identity, permissions and the
/// input service. Whether native Save/Open dialogs work is a separate
/// observation ([`crate::capability`]) that `ready` never implies.
pub fn diagnose(value: Value, uid: u32) -> Value {
    diagnosis(value, uid).value
}

pub(crate) fn diagnosis(value: Value, uid: u32) -> Diagnosis {
    Observation::decode(&value).diagnose(value, uid)
}

impl Observation {
    pub(crate) fn diagnose(self, value: Value, uid: u32) -> Diagnosis {
        let observation = self;
        let session = observation.require_hidden(uid).ok();
        let checks = observation.checks(uid, session.as_ref());

        let mut warnings = Vec::new();

        if observation.native.ok && !observation.input_build_current() {
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
                    "nextStep": (check.ok != Some(true)).then(|| check.next_step.text()),
                })
            })
            .collect::<Vec<_>>();

        let next_steps = checks
            .iter()
            .filter(|check| check.ok == Some(false))
            .map(|check| check.next_step.text())
            .collect::<Vec<_>>();

        let ready = checks.iter().all(|check| check.ok == Some(true));
        let first_step = checks
            .iter()
            .find(|check| check.ok == Some(false))
            .map(|check| check.next_step.clone())
            .unwrap_or_else(|| Guidance::Advice("The helper is not ready".into()));
        let observe = session.as_ref().is_some_and(|s| s.capture);
        let mut report = json!({
            "ok": true,
            "ready": ready,
            "canObserve": observe,
            "capabilities": {
                "observe": observe,
                "input": session.as_ref().is_some_and(|s| s.input || observation.native.repinnable(s)),
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
            observation,
            ready,
            first_step,
        }
    }
}

impl Observation {
    fn checks(&self, uid: u32, session: Option<&Session>) -> [Check; 5] {
        let reachable = self.ok;
        let native_reachable = self.native.ok;
        // The input service is only queried once the capture side proves a hidden
        // helper; an unqueried service is unknown, not failed.
        let queried = reachable && self.native.queried;
        let known = |fact: bool, ok: bool| fact.then_some(ok);

        // A blocked shell says nothing of the helper, so it may well be running.
        let blocked = "Unknown until this shell can reach the helper.";

        let (unknown, not_queried) = if self.code.as_deref() == Some("blocked") {
            (blocked, blocked)
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
                name: if self.code.as_deref() == Some("blocked") {
                    "Helper reachable from here"
                } else {
                    "Helper running"
                },
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
                ok: known(reachable, self.capture == PermissionObservation::Allowed),
                next_step: if reachable {
                    "In the helper's System Settings › Privacy & Security › Screen & System Audio \
                     Recording, turn Atlas on."
                } else {
                    unknown
                }
                .into(),
            },
            Check {
                name: "Accessibility allowed",
                ok: known(
                    native_reachable,
                    self.native.accessibility == PermissionObservation::Allowed,
                ),
                next_step: if native_reachable || self.accessibility == PermissionObservation::Denied {
                    "In the helper's System Settings › Privacy & Security › Accessibility, turn \
                     Atlas on (atlas init walks you through it)."
                } else if queried {
                    START_INPUT
                } else {
                    not_queried
                }
                .into(),
            },
            Check {
                name: "Input service ready",
                ok: known(
                    queried,
                    session
                        .as_ref()
                        .is_some_and(|s| s.input || self.native.repinnable(s)),
                ),
                next_step: if queried && self.accessibility == PermissionObservation::Denied {
                    "Turn on Accessibility for Atlas in the helper first; atlas init walks you through it."
                } else if queried {
                    START_INPUT
                } else if reachable {
                    not_queried
                } else {
                    unknown
                }
                .into(),
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
pub(crate) const START_INPUT: &str = "Run atlas start-input.";

/// The doctor's step for a logged-out helper.
pub(crate) const LOGGED_OUT: &str = "The helper is logged out (after a restart or a log-out). Log in \
     to it once, then switch back: atlas switch locks the screen for that.";

impl Absence {
    /// Its name in a report, as the overview reads it.
    pub(crate) fn as_str(&self) -> &'static str {
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
    status["ok"] != true
        && !["blocked", "identity", "busy"]
            .iter()
            .any(|code| status["code"] == *code)
}

#[cfg(test)]
mod tests {
    use super::{
        Absence, Blocker, FOCUS_APP, Kind, LOGGED_OUT, Observation, PermissionObservation, Process,
        Record, Window, blockers, classify, desktop_up_in, diagnose, gone_twice, has_capability,
        input_build_current, logged_out_in, parse_processes, record_with, require_hidden,
        running_for, silent, stale_in, validate_target_uid,
    };
    use crate::error::Code;
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn discovery_requires_an_absolute_helper_home() {
        for value in [
            json!({}),
            json!(null),
            json!(false),
            json!(12),
            json!(""),
            json!("relative"),
        ] {
            let observation = Observation::decode(&json!({"user": {"home": value}}));
            let error = observation.helper_home().unwrap_err();
            assert_eq!(error.code, "identity");
            assert_eq!(error.error, "The helper home is unknown; run atlas doctor");
        }
        let observation = Observation::decode(&json!({"user": {"home": "/Users/helper"}}));
        assert_eq!(
            observation.helper_home().unwrap(),
            std::path::Path::new("/Users/helper")
        );
    }

    #[test]
    fn an_unqueried_permission_remains_distinct_from_denial() {
        let unknown = json!({"ok": true});
        let unavailable = json!({"ok": true, "inputService": {"ok": false}});
        let denied = json!({"ok": true, "inputService": {"ok": true, "accessibility": false}});
        let allowed = json!({"ok": true, "inputService": {"ok": true, "accessibility": true}});
        for (value, expected, projection) in [
            (unknown, PermissionObservation::Unqueried, json!(null)),
            (unavailable, PermissionObservation::Unavailable, json!(null)),
            (denied, PermissionObservation::Denied, json!(false)),
            (allowed, PermissionObservation::Allowed, json!(true)),
        ] {
            let observation = Observation::decode(&value);
            assert_eq!(observation.native.accessibility, expected);
            let report = diagnose(value, 502);
            let check = report["checks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|check| check["name"] == "Accessibility allowed")
                .unwrap();
            assert_eq!(check["ok"], projection);
        }

        for (value, expected) in [
            (json!({}), PermissionObservation::Unqueried),
            (json!({"ok": true}), PermissionObservation::Unavailable),
            (
                json!({"ok": true, "permissions": {"screenCapture": false}}),
                PermissionObservation::Denied,
            ),
            (
                json!({"ok": true, "permissions": {"screenCapture": true}}),
                PermissionObservation::Allowed,
            ),
        ] {
            assert_eq!(Observation::decode(&value).capture, expected);
        }
    }

    #[test]
    fn a_partial_observation_never_proves_a_hidden_session() {
        for value in [
            json!(null),
            json!(true),
            json!([]),
            json!({}),
            json!({"ok": true}),
            json!({"ok": true, "user": {"uid": 502},
                "session": {"managerName": "Aqua", "onConsole": false}}),
        ] {
            let observation = Observation::decode(&value);
            assert!(observation.require_hidden(502).is_err(), "{value}");
            assert!(!observation.input_current(502, 77), "{value}");
            assert_eq!(diagnose(value, 502)["ready"], false);
        }
    }

    #[test]
    fn nonfinite_or_nonpositive_geometry_never_proves_an_input_epoch() {
        let value = json!({"ok": true, "uid": 502, "onConsole": false,
            "hasGraphicAccess": true, "securitySessionID": 42, "displayID": 7,
            "width": 100, "height": 100});
        for invalid in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            0.0,
            -1.0,
            32769.0,
        ] {
            let mut proof = super::ProofObservation::decode(&value);
            proof.width = Some(invalid);
            assert!(proof.input_epoch(77).is_err());
            proof.width = Some(100.0);
            proof.height = Some(invalid);
            assert!(proof.input_epoch(77).is_err());
            assert!(
                !super::Display {
                    width: invalid,
                    height: 100.0,
                    scale: 1.0
                }
                .valid()
            );
        }
    }

    #[test]
    fn a_service_lists_only_the_capabilities_it_names() {
        let native = json!({"capabilities": ["shutdown", "focus-app"]});
        assert!(has_capability(&native, "shutdown"));
        assert!(has_capability(&native, FOCUS_APP));
        assert!(!has_capability(&native, "hold"));
        assert!(!has_capability(&json!({}), "hold"));
        assert!(!has_capability(&json!({"capabilities": "hold"}), "hold"));
    }

    fn listed_processes(listing: &str) -> Vec<Process> {
        parse_processes(listing).expect("valid process list")
    }

    #[test]
    fn only_a_helper_that_did_not_answer_is_explained() {
        assert!(silent(
            &json!({"ok": false, "code": "io", "error": "No such file"})
        ));
        assert!(silent(&json!({"ok": false, "code": "transport"})));

        for code in ["blocked", "identity", "busy"] {
            assert!(!silent(&json!({"ok": false, "code": code})), "{code}");
        }

        assert!(!silent(&json!({"ok": true})));
    }

    #[test]
    fn the_desktop_is_up_once_setup_assistant_is_gone_and_the_dock_and_finder_run() {
        let core = "/System/Library/CoreServices";
        let listing = |extra: &str| {
            listed_processes(&format!(
                "502 10 00:10 {core}/Dock.app/Contents/MacOS/Dock\n\
                 502 11 00:10 {core}/Finder.app/Contents/MacOS/Finder\n\
                 502 12 00:10 {core}/KeyboardSetupAssistant.app/Contents/MacOS/KeyboardSetupAssistant\n\
                 {extra}"
            ))
        };
        let assistant = |uid: u32| {
            format!("{uid} 13 00:10 {core}/Setup Assistant.app/Contents/MacOS/Setup Assistant\n")
        };
        assert!(desktop_up_in(&listing(""), 502));
        assert!(!desktop_up_in(&listing(&assistant(502)), 502));
        assert!(
            desktop_up_in(&listing(&assistant(503)), 502),
            "another account's Setup Assistant"
        );
        assert!(!desktop_up_in(&listing(""), 503), "no Dock of its own");
    }

    #[test]
    fn processes_are_read_from_ps_lines() {
        let listing = "  505  1158 04-00:04:57 /System/Library/CoreServices/loginwindow.app/Contents/MacOS/loginwindow\n\
                       501   700       05:32 /Applications/Some App.app/Contents/MacOS/Some App\n\
                       -2    40       00:10 /usr/sbin/system-service\n";
        let listed = listed_processes(listing);
        assert_eq!(listed.len(), 2);
        assert_eq!((listed[0].uid, listed[0].pid), (505, 1158));
        assert_eq!(
            listed[0].running,
            std::time::Duration::from_secs(4 * 86_400 + 4 * 60 + 57)
        );
        assert_eq!(
            listed[1].command, "/Applications/Some App.app/Contents/MacOS/Some App",
            "a command keeps its spaces"
        );
    }

    #[test]
    fn process_lists_ignore_negative_ids_but_refuse_unreadable_rows() {
        let listing = "-2 40 00:10 /usr/sbin/system-service\n\
                       1 41 00:10 /usr/sbin/daemon\n\
                       99 42 00:10 /usr/sbin/unknown\n\
                       502 43 00:10 /System/Library/CoreServices/Dock.app/Contents/MacOS/Dock\n";
        let listed = parse_processes(listing).unwrap();
        assert_eq!(
            listed.iter().map(|process| process.uid).collect::<Vec<_>>(),
            [1, 99, 502]
        );
        for line in [
            "garbage",
            "NaN 44 00:10 /Dock",
            "4294967296 44 00:10 /Dock",
            "502 bad 00:10 /Dock",
            "502 44 bad /Dock",
            "502 44 00:10",
        ] {
            let malformed = format!("{listing}{line}\n");
            assert!(parse_processes(&malformed).is_none(), "{line}");
            assert!(!gone_twice(&[502], || parse_processes(&malformed), || {}));
        }
    }

    #[test]
    fn a_logged_out_helper_says_how_to_log_it_in() {
        let mut report = diagnose(json!({"ok": false, "code": "io"}), 501);
        Absence::LoggedOut("atlas".into()).explain(&mut report);
        assert_eq!(report["absence"], "loggedOut");
        assert_eq!(report["account"], "atlas");
        assert_eq!(report["checks"][0]["nextStep"], LOGGED_OUT);
        assert_eq!(report["nextSteps"][0], LOGGED_OUT);

        let mut gone = diagnose(json!({"ok": false, "code": "io"}), 501);
        Absence::Missing.explain(&mut gone);
        assert_eq!(gone["absence"], "missing");
        assert!(gone.get("account").is_none());
    }

    /// A failed connection keeps its step and socket, so an error alone says
    /// which connection failed.
    #[test]
    fn a_helper_that_cannot_be_reached_keeps_the_cause_of_the_failure() {
        let uid = (nix::unistd::getuid().as_raw() + 1).max(501);
        let cause = "Could not connect to socket /tmp/helper.sock: Connection refused";
        let error = require_hidden(&json!({"ok": false, "error": cause}), uid).unwrap_err();
        assert_eq!(
            (error.code, error.error),
            (
                Code::Transport,
                format!("The helper is not answering: {cause}. Run atlas doctor.")
            )
        );

        let error = require_hidden(&json!({"ok": false}), uid).unwrap_err();
        assert_eq!(
            error.error,
            "The helper is not answering. Run atlas doctor."
        );
    }

    /// A helper too busy for one more caller is busy, never "not answering",
    /// so nobody is sent to restart a helper that is up.
    #[test]
    fn a_helper_turning_callers_away_is_busy_not_silent() {
        let status = json!({"ok": false, "code": "busy", "error": "The helper is serving too many requests; try again"});
        let uid = nix::unistd::getuid().as_raw() + 1;
        let error = require_hidden(&status, uid.max(501)).unwrap_err();
        assert_eq!(
            (error.code, error.error.as_str()),
            (
                Code::Busy,
                "The helper is serving too many requests; try again"
            )
        );
    }

    /// A shell that may not reach the helper is told so, never that the
    /// helper stopped or that setup must run again.
    #[test]
    fn a_blocked_shell_is_not_a_stopped_helper() {
        let status = json!({"ok": false, "code": "blocked", "error": "This shell may not connect"});
        let uid = nix::unistd::getuid().as_raw() + 1;
        let error = require_hidden(&status, uid.max(501)).unwrap_err();
        assert_eq!(
            (error.code, error.error.as_str()),
            (Code::Blocked, "This shell may not connect")
        );

        let report = diagnose(status, uid.max(501));
        assert_eq!(report["checks"][0]["name"], "Helper reachable from here");
        assert_eq!(report["nextSteps"][0], "This shell may not connect");

        for check in report["checks"].as_array().unwrap().iter().skip(1) {
            assert_eq!(
                check["nextStep"],
                "Unknown until this shell can reach the helper."
            );
        }

        let stopped = diagnose(
            json!({"ok": false, "code": "io", "error": "No such file"}),
            501,
        );
        assert_eq!(stopped["checks"][0]["name"], "Helper running");
    }

    #[test]
    fn input_pinned_to_an_earlier_same_size_display_counts_as_ready() {
        use super::{Display, Epoch, NativeObservation, Session};

        let session = Session {
            epoch: Epoch {
                uid: 503,
                capture_pid: 10,
                display: Display {
                    width: 1512.0,
                    height: 982.0,
                    scale: 2.0,
                },
                security_session_id: 100135,
                display_id: 15,
                input_pid: 11318,
            },
            capture: true,
            input: false,
        };

        let status = |width: f64| {
            json!({"inputService": {"ok": true, "protocolVersion": 1,
            "accessibility": true, "enforcedEpoch": {"uid": 503, "securitySessionId": 100135,
            "displayId": 9, "processId": 11318, "width": width, "height": 982.0}}})
        };
        assert!(NativeObservation::decode(&status(1512.0)["inputService"]).repinnable(&session));
        assert!(
            !NativeObservation::decode(&status(1440.0)["inputService"]).repinnable(&session),
            "a new size needs a new look"
        );

        let mut other = status(1512.0);
        other["inputService"]["enforcedEpoch"]["securitySessionId"] = json!(1);
        assert!(
            !NativeObservation::decode(&other["inputService"]).repinnable(&session),
            "never across sessions"
        );
    }

    #[test]
    fn elapsed_times_read_like_ps_writes_them() {
        use std::time::Duration;

        assert_eq!(running_for("05:09"), Some(Duration::from_secs(309)));
        assert_eq!(running_for("12:05:09"), Some(Duration::from_secs(43_509)));
        assert_eq!(
            running_for("2-00:00:01"),
            Some(Duration::from_secs(172_801))
        );
        assert_eq!(running_for("soon"), None);
    }

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

    fn window(uid: u32, seconds: u64) -> Window {
        Window {
            uid,
            pid: uid + 1000,
            age: Duration::from_secs(seconds),
        }
    }

    fn account(seconds: Option<u64>) -> Record {
        Record {
            passwd: true,
            directory: true,
            age: seconds.map(Duration::from_secs),
        }
    }

    fn kinds(classified: &[(Window, Kind)]) -> Vec<(u32, Kind)> {
        classified
            .iter()
            .map(|(window, kind)| (window.uid, *kind))
            .collect()
    }

    #[test]
    fn a_login_whose_account_no_directory_has_is_an_orphan() {
        let windows = [window(0, 900), window(501, 900), window(502, 900)];
        let classified = classify(
            &windows,
            |uid| {
                if uid == 502 {
                    Record::default()
                } else {
                    account(Some(5000))
                }
            },
            501,
            Some(503),
            Some(501),
        );
        assert_eq!(
            kinds(&classified),
            [(0, Kind::Picker), (501, Kind::Mine), (502, Kind::Orphan)]
        );
    }

    #[test]
    fn a_reused_user_id_is_a_leftover_only_while_its_login_is_older_than_the_account() {
        let older = classify(
            &[window(503, 900)],
            |_| account(Some(60)),
            501,
            Some(503),
            Some(501),
        );
        assert_eq!(kinds(&older), [(503, Kind::HelperLeftover)]);

        let newer = classify(
            &[window(503, 30)],
            |_| account(Some(60)),
            501,
            Some(503),
            Some(501),
        );
        assert_eq!(kinds(&newer), [(503, Kind::Helper)]);

        let rounding = classify(
            &[window(503, 65)],
            |_| account(Some(60)),
            501,
            Some(503),
            None,
        );
        assert_eq!(kinds(&rounding), [(503, Kind::Helper)], "within the slack");

        let unknown = classify(&[window(503, 900)], |_| account(None), 501, Some(503), None);
        assert_eq!(
            kinds(&unknown),
            [(503, Kind::Helper)],
            "no age, no leftover"
        );
    }

    #[test]
    fn the_login_window_itself_is_the_picker() {
        let classified = classify(
            &[window(0, 10), window(88, 10)],
            |_| Record::default(),
            501,
            None,
            None,
        );
        assert_eq!(kinds(&classified), [(0, Kind::Picker), (88, Kind::Picker)]);
    }

    #[test]
    fn this_account_is_never_acted_on_even_without_a_record() {
        let classified = classify(
            &[window(501, 10)],
            |_| Record::default(),
            501,
            Some(501),
            None,
        );
        assert_eq!(kinds(&classified), [(501, Kind::Mine)]);
    }

    #[test]
    fn the_account_on_screen_is_never_an_orphan() {
        let classified = classify(
            &[window(504, 900)],
            |_| Record::default(),
            501,
            Some(503),
            Some(504),
        );
        assert_eq!(kinds(&classified), [(504, Kind::OtherPerson)]);

        let helper = classify(
            &[window(503, 900)],
            |_| account(Some(60)),
            501,
            Some(503),
            Some(503),
        );
        assert_eq!(
            kinds(&helper),
            [(503, Kind::Helper)],
            "the helper on screen"
        );
    }

    #[test]
    fn an_account_getpwuid_found_is_never_searched_for_again() {
        let found = Ok(Some(crate::setup::Account {
            name: "helper".to_owned(),
            uid: 503,
            home: "/Users/helper".into(),
        }));
        let record = record_with(503, false, found, |_| panic!("searched every directory"));
        assert!(record.exists());

        let failed = Err(crate::error::Error::new(Code::Setup, "The lookup failed"));
        let record = record_with(503, false, failed, |_| panic!("searched every directory"));
        assert!(
            record.exists(),
            "a failed lookup counts as an account found"
        );
    }

    #[test]
    fn an_account_getpwuid_did_not_find_is_searched_for_in_every_directory() {
        let mut asked = None;
        let record = record_with(1201, false, Ok(None), |uid| {
            asked = Some(uid);
            true
        });
        assert_eq!(asked, Some(1201));
        assert!(record.exists(), "a network account");

        let record = record_with(1202, false, Ok(None), |_| false);
        assert!(!record.exists(), "a deleted account");
    }

    #[test]
    fn a_network_account_is_never_an_orphan() {
        let network = Record {
            passwd: false,
            directory: true,
            age: None,
        };
        let classified = classify(&[window(1201, 900)], |_| network, 501, Some(503), Some(501));
        assert_eq!(kinds(&classified), [(1201, Kind::OtherPerson)]);

        let cached = Record {
            passwd: true,
            directory: false,
            age: None,
        };
        let classified = classify(&[window(1202, 900)], |_| cached, 501, None, Some(501));
        assert_eq!(kinds(&classified), [(1202, Kind::OtherPerson)]);
    }

    #[test]
    fn a_login_without_a_dock_for_long_is_stuck_but_setup_assistant_is_not() {
        let core = "/System/Library/CoreServices";
        let classified = [(window(503, 130), Kind::Helper)];

        let bare = listed_processes(&format!(
            "503 10 02:10 {core}/loginwindow.app/Contents/MacOS/loginwindow\n"
        ));
        assert_eq!(
            blockers(&classified, &bare),
            [Blocker::StuckLogin { uid: 503 }]
        );

        let assistant = listed_processes(&format!(
            "503 11 02:00 {core}/Setup Assistant.app/Contents/MacOS/Setup Assistant\n"
        ));
        assert!(blockers(&classified, &assistant).is_empty());

        let docked = listed_processes(&format!(
            "503 12 02:00 {core}/Dock.app/Contents/MacOS/Dock\n"
        ));
        assert!(blockers(&classified, &docked).is_empty());

        let young = [(window(503, 90), Kind::Helper)];
        assert!(blockers(&young, &bare).is_empty(), "still within its time");

        let mine = [(window(501, 900), Kind::Mine)];
        assert!(blockers(&mine, &bare).is_empty(), "never this account");
    }

    #[test]
    fn a_running_account_updater_and_deleted_accounts_block_a_login() {
        let updater =
            listed_processes("0 40 00:05 /System/Library/CoreServices/UserAccountUpdater\n");
        assert_eq!(blockers(&[], &updater), [Blocker::AccountUpdating]);

        let classified = [
            (window(502, 900), Kind::Orphan),
            (window(503, 900), Kind::HelperLeftover),
        ];
        let found = blockers(&classified, &[]);
        assert_eq!(
            found,
            [
                Blocker::Orphan { uid: 502 },
                Blocker::HelperLeftover { uid: 503 }
            ]
        );
        assert_eq!(
            found.iter().map(Blocker::uid).collect::<Vec<_>>(),
            [Some(502), Some(503)]
        );
        assert_eq!(Blocker::AccountUpdating.uid(), None);
        assert_eq!(
            Blocker::StuckLogin { uid: 1 }.words(),
            "another account is still logging in."
        );
    }

    #[test]
    fn a_login_is_over_only_once_its_loginwindow_and_dock_are_both_gone() {
        let core = "/System/Library/CoreServices";
        let both = listed_processes(&format!(
            "502 10 00:10 {core}/loginwindow.app/Contents/MacOS/loginwindow\n\
             502 11 00:10 {core}/Dock.app/Contents/MacOS/Dock\n"
        ));
        assert!(!logged_out_in(&both, 502));
        assert!(!logged_out_in(&both[..1], 502), "a loginwindow alone");
        assert!(!logged_out_in(&both[1..], 502), "a Dock alone");
        assert!(logged_out_in(&[], 502));
        assert!(logged_out_in(&both, 503), "another account's");
    }

    #[test]
    fn closed_logins_are_gone_only_once_seen_without_loginwindow_and_dock_twice() {
        use std::{cell::RefCell, collections::VecDeque};

        let core = "/System/Library/CoreServices";
        let dock = listed_processes(&format!(
            "7001 11 00:10 {core}/Dock.app/Contents/MacOS/Dock\n"
        ));
        let window = listed_processes(&format!(
            "7003 10 00:10 {core}/loginwindow.app/Contents/MacOS/loginwindow\n"
        ));
        let gone = |looks: Vec<Option<Vec<Process>>>| {
            let looks = RefCell::new(looks.into_iter().collect::<VecDeque<_>>());
            gone_twice(
                &[7001, 7003],
                || looks.borrow_mut().pop_front().flatten(),
                || {},
            )
        };

        assert!(gone(vec![Some(vec![]), Some(vec![])]));
        assert!(!gone(vec![Some(dock.clone()), Some(vec![])]), "a Dock left");
        assert!(
            !gone(vec![Some(vec![]), Some(window)]),
            "back by the second look"
        );
        assert!(!gone(vec![None, Some(vec![])]), "a list that failed");
    }

    #[test]
    fn a_login_is_left_behind_only_once_its_desktop_has_been_up_for_minutes() {
        let core = "/System/Library/CoreServices";
        let listing = |age: &str, dock: bool| {
            let mut text =
                format!("503 10 {age} {core}/loginwindow.app/Contents/MacOS/loginwindow\n");
            if dock {
                text.push_str(&format!(
                    "503 11 {age} {core}/Dock.app/Contents/MacOS/Dock\n"
                ));
            }
            listed_processes(&text)
        };
        assert!(stale_in(&listing("04:00", true), 503));
        assert!(!stale_in(&listing("02:00", true), 503), "still starting");
        assert!(!stale_in(&listing("04:00", false), 503), "no desktop yet");
        assert!(!stale_in(&listing("04:00", true), 504), "another account's");
    }

    #[test]
    fn the_desktop_waits_for_the_account_updater_to_end() {
        let core = "/System/Library/CoreServices";
        let listed = listed_processes(&format!(
            "502 10 00:10 {core}/Dock.app/Contents/MacOS/Dock\n\
             502 11 00:10 {core}/Finder.app/Contents/MacOS/Finder\n"
        ));
        assert!(desktop_up_in(&listed, 502));

        let mut updating = listed.clone();
        updating.extend(listed_processes(&format!(
            "0 12 00:01 {core}/UserAccountUpdater\n"
        )));
        assert!(!desktop_up_in(&updating, 502));
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

    #[test]
    fn unsupported_report_roots_return_a_certain_error_instead_of_panicking() {
        let mut outcomes = Vec::new();
        for absence in [Absence::Missing, Absence::LoggedOut("helper".into())] {
            for value in crate::test_support::unsupported_report_roots() {
                outcomes.push(std::panic::catch_unwind(|| {
                    let mut report = value;
                    absence.explain(&mut report);
                    report
                }));
            }
        }
        for outcome in outcomes {
            let report = outcome.expect("An invalid report shape must not panic");
            assert_eq!(
                report,
                json!({"ok": false, "code": "state", "error": "A doctor report must be an object or null", "uncertain": false})
            );
            assert!(!report.to_string().contains("private-report-input-canary"));
        }
    }

    #[test]
    fn a_legacy_null_report_keeps_its_existing_absence_fields() {
        let mut missing = json!(null);
        Absence::Missing.explain(&mut missing);
        assert_eq!(missing, json!({"absence": "missing", "checks": null}));
        let mut logged_out = json!(null);
        Absence::LoggedOut("helper".into()).explain(&mut logged_out);
        assert_eq!(
            logged_out,
            json!({"absence": "loggedOut", "account": "helper", "checks": null})
        );
    }
}
