//! What the helper's status proves about its session: the display and the
//! processes an action is tied to (its [`Epoch`]), whether the helper is the
//! hidden graphical login it must be, and the doctor report built from it.
pub mod diagnosis;
mod epoch;
pub mod key;
pub mod paired;

pub use diagnosis::{Absence, silent};
pub use diagnosis::{Diagnosis, Guidance, LOGGED_OUT, require_report};
#[cfg(any(test, feature = "test-support"))]
pub use diagnosis::{START_INPUT, diagnosis};
pub use epoch::{Console, Display, Epoch, InputEpoch, MachineEpoch};

use crate::account::FIRST_USER_UID;
use crate::error::{Code, Error, Result};
use epoch::ProofObservation;
use serde::Deserialize;
use serde_json::Value;

/// The longest boot identity taken from a status.
const BOOT_LENGTH: usize = 128;

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
    /// The epoch the input service proves when its session is on its own
    /// screen, as a machine desktop's is.
    own_screen_epoch: Option<InputEpoch>,
    /// The backing scale of the display its session is on, which only a
    /// machine desktop's input is held to.
    own_screen_scale: Option<f64>,
    pinned: PinObservation,
    pub control_active: Option<bool>,
    pub held: bool,
    pub held_input: Option<bool>,
    /// Whether input is held or its record not yet written; None when not
    /// answered.
    pub unsettled: Option<bool>,
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
                .input_epoch(pid.unwrap_or(0), Console::Off)
                .ok(),
            own_screen_epoch: ProofObservation::decode(&value["session"])
                .input_epoch(pid.unwrap_or(0), Console::Own)
                .ok(),
            own_screen_scale: value["session"]["scale"].as_f64(),
            pinned: PinObservation::decode(&value["enforcedEpoch"]),
            control_active: value["controlActive"].as_bool(),
            held: value["held"] == true,
            held_input: value["heldInput"].as_bool(),
            unsettled: value["unsettled"].as_bool(),
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
    /// What a machine desktop's daemon says of itself: whether it runs in
    /// machine mode, and the boot it runs in.
    machine_mode: bool,
    boot: Option<String>,
    /// Whether the machine is a virtual one, as its kernel says.
    virtual_machine: Option<bool>,
    /// The digest of the executable a machine desktop's daemon started
    /// from, in the form its input service says its own.
    daemon_build: Option<String>,
    /// The Atlas version a machine desktop's daemon says it is.
    daemon_version: Option<String>,
    /// Why a machine desktop's guest holds agents while it is open to more
    /// than Atlas; none once it says it is closed.
    posture_held: Option<String>,
}

/// Why a machine desktop's guest holds agents, as its hello `value` says;
/// none only once Remote Login is off and its account no administrator.
fn posture_held(value: &Value) -> Option<String> {
    if value["remoteLogin"] == "off" && value["account"]["admin"] == false {
        return None;
    }
    Some(
        value["agents"]["held"]
            .as_str()
            .unwrap_or("Whether the guest is closed to anyone but Atlas cannot be told.")
            .to_owned(),
    )
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
            machine_mode: value["machine"]["mode"] == "machine",
            boot: value["machine"]["boot"]
                .as_str()
                .filter(|boot| !boot.is_empty() && boot.len() <= BOOT_LENGTH)
                .map(str::to_owned),
            virtual_machine: value["machine"]["virtual"].as_bool(),
            daemon_build: value["daemon"]["build"].as_str().map(str::to_owned),
            daemon_version: value["daemon"]["version"].as_str().map(str::to_owned),
            posture_held: posture_held(value),
        }
    }

    /// Whether the machine that answered says it is a virtual one; none when
    /// it did not say.
    pub fn in_virtual_machine(&self) -> Option<bool> {
        self.virtual_machine
    }

    pub fn display_valid(&self) -> bool {
        self.display.as_ref().is_some_and(Display::valid)
    }

    pub fn has_capability(&self, name: &str) -> bool {
        self.daemon_capabilities
            .iter()
            .any(|capability| capability == name)
    }

    /// The capabilities the desktop's daemon declares, in its order.
    pub fn daemon_capabilities(&self) -> &[String] {
        &self.daemon_capabilities
    }

    /// The digest of the executable a machine desktop's daemon started
    /// from; none when it did not say.
    pub fn daemon_build(&self) -> Option<&str> {
        self.daemon_build.as_deref()
    }

    /// The Atlas version a machine desktop's daemon says it is; none when
    /// it did not say.
    pub fn daemon_version(&self) -> Option<&str> {
        self.daemon_version.as_deref()
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

        let proof = self
            .proof
            .input_epoch(pid, Console::Off)
            .map_err(|_| invalid())?;
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

    /// The session a machine desktop's status proves, held to what its
    /// pairing fixed and to what the station knows of it (`expect`). It
    /// keeps every check [`Observation::require_hidden`] makes but two: the
    /// account need not differ from the station's, since it is another
    /// machine's, and its screen is its own, so it must be on its console,
    /// with the person's hands off it instead. Its epoch includes the
    /// display's scale, which its proof must show too, and its input counts
    /// only from the very build its daemon started from.
    ///
    /// # Errors
    ///
    /// Returns a refusal when the answer is not the paired machine as
    /// `expect` says it must be, and when its guest holds agents while it
    /// is open to more than Atlas.
    pub fn require_machine(&self, expect: &MachineExpect<'_>) -> Result<MachineSession> {
        self.require_answer()?;
        let uid = expect.paired.uid;
        let refused = |why: &str| Error::new(Code::Identity, format!("{why}. Run atlas doctor."));
        if uid < FIRST_USER_UID {
            return Err(refused("The desktop's account is not a user account"));
        }
        if self.uid != Some(uid.into()) {
            return Err(refused(
                "A different account answered than the desktop was paired with",
            ));
        }
        if expect.peer != expect.paired.guest || expect.peer == expect.station {
            return Err(refused(
                "The desktop did not prove the key it was paired with",
            ));
        }
        if expect.station.fingerprint() != expect.paired.station {
            return Err(refused("The desktop was paired with another station"));
        }
        if !self.machine_mode {
            return Err(refused(
                "The desktop's Atlas is not running as a machine desktop",
            ));
        }
        if !expect.eligible {
            return Err(refused("The desktop is not one this kind may use"));
        }
        if let Some(why) = &self.posture_held {
            return Err(Error::new(
                Code::Platform,
                format!("{why} Agents wait until the desktop is closed to anyone but Atlas."),
            ));
        }
        match expect.hands_off {
            Some(true) => {}
            Some(false) => {
                return Err(Error::new(
                    Code::OnConsole,
                    "The desktop's window may be taking your input. Close it, or turn its \
                     input off, and try again.",
                ));
            }
            None => {
                return Err(Error::new(
                    Code::OnConsole,
                    "Whether the desktop's window takes your input cannot be told. Restart \
                     the VM, and try again.",
                ));
            }
        }

        let invalid = || refused("The desktop's session could not be verified");
        let pid = self.capture_pid.ok_or_else(invalid)?;
        let display = self.display.clone().ok_or_else(invalid)?;
        let boot = self.boot.clone().ok_or_else(invalid)?;
        if self.on_console != Some(true) || !self.aqua || !display.valid() {
            return Err(invalid());
        }
        let proof = self
            .proof
            .input_epoch(pid, Console::Own)
            .map_err(|_| invalid())?;
        if self.proof.parent_pid != Some(pid.into())
            || proof.uid != uid
            || proof.width != display.width
            || proof.height != display.height
            || self.proof.scale != Some(display.scale)
        {
            return Err(invalid());
        }

        let epoch = MachineEpoch {
            uid,
            boot,
            capture_pid: pid,
            display,
            security_session_id: proof.security_session_id,
            display_id: proof.display_id,
            input_pid: self.native.pid.unwrap_or(0),
        };
        let native_matches = self.native.own_screen_epoch.as_ref() == Some(&epoch.input_epoch())
            && self.native.own_screen_scale == Some(epoch.display.scale);
        let same_build = self.native.build.is_some() && self.native.build == self.daemon_build;
        Ok(MachineSession {
            capture: self.capture == Some(true),
            input: self.native.healthy()
                && self.native.ready == Some(true)
                && native_matches
                && same_build,
            epoch,
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

/// What a machine desktop's proof is held to beyond its status.
#[derive(Clone, Debug)]
pub struct MachineExpect<'a> {
    /// What its pairing fixed.
    pub paired: &'a paired::Paired,
    /// The key the connection this status came over proved.
    pub peer: key::PeerKey,
    /// The station's own key, which a desktop never is.
    pub station: key::PeerKey,
    /// Whether the kind finds this machine one it may use.
    pub eligible: bool,
    /// Whether the person's hands are off it: its window takes no input and
    /// no maintenance runs. Unknown counts as on.
    pub hands_off: Option<bool>,
}

/// The session a machine desktop's status proved.
#[derive(Clone, Debug)]
pub struct MachineSession {
    pub epoch: MachineEpoch,
    pub capture: bool,
    pub input: bool,
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

/// The input-service capability that names the field a batch's text went
/// into, a password field by its subrole too, and none when its typing
/// actions found different fields: what typed text may be kept by.
pub const TYPED_FIELDS: &str = "typed-fields";

/// An input service with this asks the person before an agent sends from
/// an account they listed: it reads the request's `asking` and refuses with
/// `waiting-for-person` or `denied` before anything is sent.
pub const ASKS: &str = "asks";

/// Whether an input service's hello lists `name` among its capabilities.
pub fn has_capability(native: &Value, name: &str) -> bool {
    capability_names(native)
        .iter()
        .any(|capability| capability == name)
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
