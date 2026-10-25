//! What the helper's status proves about its session: the display and the
//! processes an action is tied to (its [`Epoch`]), whether the helper is the
//! hidden graphical login it must be, and the doctor report built from it.
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

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

/// A session proof's security session and main display IDs. Both are 32-bit
/// on macOS. Zero is no session and no display; a security session of
/// `u32::MAX` is `callerSecuritySession`, the sentinel that asks about the
/// caller's own session and never a real ID, so a session lies strictly
/// between the two.
fn session_ids(proof: &Value) -> Option<(u64, u64)> {
    let security = proof["securitySessionID"]
        .as_u64()
        .filter(|n| *n > 0 && *n < u32::MAX.into())?;
    let display = proof["displayID"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= u32::MAX.into())?;

    Some((security, display))
}

impl InputEpoch {
    pub fn from_proof(proof: &Value, process_id: u32) -> Result<Self> {
        let invalid = || {
            Error::new(
                "identity",
                "A hidden graphical session with valid geometry is required",
            )
        };
        let (security_session_id, display_id) = session_ids(proof).ok_or_else(invalid)?;
        let side = |key: &str| {
            proof[key]
                .as_f64()
                .filter(|v| within(*v, MAX_SIDE))
                .ok_or_else(invalid)
        };

        let epoch = Self {
            uid: proof["uid"]
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .filter(|v| *v >= FIRST_USER_UID)
                .ok_or_else(invalid)?,
            security_session_id,
            display_id,
            width: side("width")?,
            height: side("height")?,
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
    if uid < FIRST_USER_UID || uid == controller_uid {
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
        // A shell that may not reach the helper says so: it is not down.
        if value["code"] == "blocked" {
            return Err(Error::new(
                "blocked",
                value["error"]
                    .as_str()
                    .unwrap_or("This shell may not connect to the helper"),
            ));
        }

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
            "The helper is not answering. Run atlas doctor.",
        ));
    }

    if value["user"]["uid"]
        .as_u64()
        .is_some_and(|other| other != u64::from(uid))
    {
        return Err(Error::new(
            "identity",
            "A different account answered than the configured helper. Run atlas doctor.",
        ));
    }

    if value["session"]["onConsole"] == true || value["sessionProof"]["onConsole"] == true {
        return Err(Error::new(
            "on-console",
            "The helper is on screen right now. Atlas works only while it is in the \
             background: switch back to your own account.",
        ));
    }

    let invalid = || {
        Error::new(
            "identity",
            "The helper's session could not be verified. Run atlas doctor.",
        )
    };

    let pid = value["daemon"]["pid"]
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok())
        .filter(|pid| *pid > 0)
        .ok_or_else(invalid)?;
    let display = Display::deserialize(&value["display"]).map_err(|_| invalid())?;

    // `ok` is already known true here.
    if value["user"]["uid"].as_u64() != Some(uid.into())
        || value["session"]["onConsole"] != false
        || value["session"]["managerName"] != "Aqua"
        || !display.valid()
    {
        return Err(invalid());
    }

    let proof = &value["sessionProof"];
    let (security, display_id) = session_ids(proof).ok_or_else(invalid)?;

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

/// Whether the input service in `value` is healthy and the one `session`
/// names, in the same login, whatever display it is pinned to.
pub fn pinned_to_session(value: &Value, session: &Session) -> bool {
    let native = &value["inputService"];
    let pinned = &native["enforcedEpoch"];
    let epoch = &session.epoch;

    native["ok"] == true
        && native["protocolVersion"] == 1
        && native["accessibility"] == true
        && pinned["uid"] == epoch.uid
        && pinned["securitySessionId"] == epoch.security_session_id
        && pinned["processId"] == epoch.input_pid
}

/// Whether the input service in `value` is healthy and in the same helper
/// session as `session`, pinned to an earlier number of the same-size
/// display (switching accounts can renumber it). The next action re-pins it
/// to the current display, so it counts as ready.
pub fn repinnable(value: &Value, session: &Session) -> bool {
    let pinned = &value["inputService"]["enforcedEpoch"];
    let epoch = &session.epoch;

    pinned_to_session(value, session)
        && pinned["width"].as_f64() == Some(epoch.display.width)
        && pinned["height"].as_f64() == Some(epoch.display.height)
}

struct Check {
    name: &'static str,
    /// None when the fact cannot be observed yet; never reported as missing.
    ok: Option<bool>,
    next_step: String,
}

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

/// The doctor report for a status snapshot: each check, what to do about a
/// failed one, and `ready` for transport, identity, permissions and the
/// input service. Whether native Save/Open dialogs work is a separate
/// observation ([`crate::capability`]) that `ready` never implies.
pub fn diagnose(value: Value, uid: u32) -> Value {
    let session = require_hidden(&value, uid).ok();
    let reachable = value["ok"] == true;
    let native = &value["inputService"];
    let native_reachable = native["ok"] == true;
    // The input service is only queried once the capture side proves a hidden
    // helper; an unqueried service is unknown, not failed.
    let queried = reachable && !native.is_null();
    let known = |fact: bool, ok: bool| fact.then_some(ok);

    // A blocked shell says nothing of the helper, so it may well be running.
    let blocked = "Unknown until this shell can reach the helper.";

    let (unknown, not_queried) = if value["code"] == "blocked" {
        (blocked, blocked)
    } else {
        (
            "Unknown until the helper is running.",
            "Unknown until the helper runs in the background.",
        )
    };

    let connection = if value["code"] == "identity" {
        value["error"]
            .as_str()
            .unwrap_or("The socket belongs to another account")
    } else if value["code"] == "blocked" {
        value["error"]
            .as_str()
            .unwrap_or("This shell may not connect to the helper; run atlas outside a sandbox.")
    } else {
        "Start the helper: run atlas init (safe to run again)."
    };

    let hidden_guidance = if value["user"]["uid"]
        .as_u64()
        .is_some_and(|v| v != u64::from(uid))
    {
        "A different account answered than the configured helper; run atlas init."
    } else if value["session"]["onConsole"] == true || value["sessionProof"]["onConsole"] == true {
        "Switch back to your own account without logging the helper out."
    } else {
        // The session check's own reason, when it gave one, says what to fix.
        value["sessionProof"]["error"].as_str().unwrap_or(
            "The helper could not be checked. Run atlas restart; if that does not help, \
             run atlas init.",
        )
    };

    let checks = [
        Check {
            // A blocked shell cannot tell whether the helper runs.
            name: if value["code"] == "blocked" {
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
            ok: known(reachable, value["permissions"]["screenCapture"] == true),
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
            ok: known(native_reachable, native["accessibility"] == true),
            next_step: if native_reachable || value["permissions"]["accessibility"] == false {
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
                    .is_some_and(|s| s.input || repinnable(&value, s)),
            ),
            next_step: if queried && value["permissions"]["accessibility"] == false {
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
    ];

    let mut warnings = Vec::new();

    if native_reachable && !input_build_current(&value) {
        warnings.push(
            "The input service is from an older build; atlas start-input replaces it.".to_owned(),
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
    let mut report = json!({
        "ok": true,
        "ready": checks.iter().all(|check| check.ok == Some(true)),
        "canObserve": observe,
        "capabilities": {
            "observe": observe,
            "input": session.as_ref().is_some_and(|s| s.input || repinnable(&value, s)),
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

    report
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
    pub fn explain(&self, report: &mut Value) {
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
/// explain: not refused by this shell, and not answered by another account.
pub fn silent(status: &Value) -> bool {
    status["ok"] != true && status["code"] != "blocked" && status["code"] != "identity"
}

/// Why the silent helper `uid` is down, if this side can tell. It asks
/// directory services and the process list, so call it off the async
/// runtime.
pub fn absence(uid: u32) -> Option<Absence> {
    match crate::setup::account_with_uid(uid) {
        Ok(None) => Some(Absence::Missing),
        Ok(Some(account)) => {
            (logged_in(uid) == Some(false)).then_some(Absence::LoggedOut(account.name))
        }
        Err(_) => None,
    }
}

/// One process, as `ps` lists it.
#[derive(Debug, Clone, PartialEq)]
struct Process {
    uid: u32,
    pid: u32,
    running: std::time::Duration,
    command: String,
}

/// Every process on this Mac, or `None` when they could not be listed.
fn processes() -> Option<Vec<Process>> {
    let output = std::process::Command::new("/bin/ps")
        .args(["-axo", "uid=,pid=,etime=,comm="])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }
    parse_processes(std::str::from_utf8(&output.stdout).ok()?)
}

/// `ps -o uid=,pid=,etime=,comm=` lines; the command is the rest of a line.
fn parse_processes(listing: &str) -> Option<Vec<Process>> {
    let mut processes = Vec::new();
    for line in listing.lines() {
        let mut words = line.split_whitespace();
        let uid = words.next()?.parse::<i64>().ok()?;
        // Negative system IDs cannot belong to a helper session.
        if uid < 0 {
            continue;
        }
        let uid = u32::try_from(uid).ok()?;
        let pid = words.next()?.parse().ok()?;
        let running = running_for(words.next()?)?;
        let command = words.collect::<Vec<_>>().join(" ");
        if command.is_empty() {
            return None;
        }
        processes.push(Process {
            uid,
            pid,
            running,
            command,
        });
    }
    Some(processes)
}

/// Whether `uid` is logged in to a desktop (its Dock runs); `None` when the
/// process list could not be read, which is never "logged out".
pub fn logged_in(uid: u32) -> Option<bool> {
    let listed = processes()?;
    Some(runs_as(&listed, uid, "/Dock"))
}

/// Whether `uid` is fully logged out: neither its loginwindow nor its Dock
/// runs. A Dock alone is not enough, since launchd restarts it in a login
/// that lives on. `None` when the process list could not be read.
pub fn logged_out(uid: u32) -> Option<bool> {
    Some(logged_out_in(&processes()?, uid))
}

fn logged_out_in(listed: &[Process], uid: u32) -> bool {
    !runs_as(listed, uid, "/loginwindow") && !runs_as(listed, uid, "/Dock")
}

/// Whether `uid`'s desktop login runs: its loginwindow and its Dock both.
/// `None` when the process list could not be read.
pub fn desktop_runs(uid: u32) -> Option<bool> {
    let listed = processes()?;
    Some(runs_as(&listed, uid, "/loginwindow") && runs_as(&listed, uid, "/Dock"))
}

/// Whether a process of `uid` whose command ends with `name` runs.
fn runs_as(listed: &[Process], uid: u32, name: &str) -> bool {
    listed
        .iter()
        .any(|process| process.uid == uid && process.command.ends_with(name))
}

/// Whether macOS is still updating an account for its login: its
/// UserAccountUpdater runs, as root and as the account.
fn updating(listed: &[Process]) -> bool {
    listed
        .iter()
        .any(|process| process.command.ends_with("/UserAccountUpdater"))
}

/// Whether `uid`'s desktop is up: macOS done updating the account, Setup
/// Assistant gone, and the Dock and the Finder running (loginwindow starts
/// the Dock only once Setup Assistant is done). `None` when processes could
/// not be listed.
pub fn desktop_up(uid: u32) -> Option<bool> {
    Some(desktop_up_in(&processes()?, uid))
}

fn desktop_up_in(listed: &[Process], uid: u32) -> bool {
    !updating(listed)
        && !runs_as(listed, uid, "/Setup Assistant")
        && runs_as(listed, uid, "/Dock")
        && runs_as(listed, uid, "/Finder")
}

/// Whether Setup Assistant runs for `uid` now; `None` when processes could
/// not be listed.
pub fn setup_assistant(uid: u32) -> Option<bool> {
    Some(runs_as(&processes()?, uid, "/Setup Assistant"))
}

/// A desktop login: its loginwindow, which starts when the person logs in
/// and lasts until they log out (unlike the Dock, which may restart).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Login {
    pub pid: u32,
    pub age: std::time::Duration,
}

/// The desktop login of `uid`, if it has one.
pub fn login(uid: u32) -> Option<Login> {
    processes()?
        .into_iter()
        .find(|process| process.uid == uid && process.command.ends_with("/loginwindow"))
        .map(|process| Login {
            pid: process.pid,
            age: process.running,
        })
}

/// One desktop login as the process list shows it: its loginwindow, the
/// user ID that runs it, and how long it has run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub uid: u32,
    pub pid: u32,
    pub age: Duration,
}

/// What directory services say of one user ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Record {
    /// Found by `getpwuid`, which asks every directory macOS uses.
    pub passwd: bool,
    /// Found by a search of every directory (`dscl /Search`), network and
    /// mobile accounts included.
    pub directory: bool,
    /// How long ago the account was made, when that is known.
    pub age: Option<Duration>,
}

impl Record {
    /// Whether any directory has an account with this user ID.
    pub fn exists(&self) -> bool {
        self.passwd || self.directory
    }
}

/// Whose a desktop login is, and so whether Atlas may ever close it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// The account asking; never acted on.
    Mine,
    /// The helper's own login.
    Helper,
    /// A deleted account's login still running under a user ID the helper
    /// was given afterwards: older than the helper's account itself.
    HelperLeftover,
    /// The login of an account that no directory has any more.
    Orphan,
    /// Another person's login, or one on screen; never acted on.
    OtherPerson,
    /// The login window itself, run by a system user.
    Picker,
}

/// How much older than its account a login must be to have outlived an
/// earlier account: a login cannot start before its account exists, and a
/// few seconds cover clock rounding.
pub(crate) const REUSED_SLACK: Duration = Duration::from_secs(10);

/// Each login in `windows` with whose it is, given what directory services
/// say of each user ID (`accounts`), this account (`me`), the helper (or
/// the account about to be made it), and who owns the screen (`console`).
pub fn classify(
    windows: &[Window],
    accounts: impl Fn(u32) -> Record,
    me: u32,
    helper: Option<u32>,
    console: Option<u32>,
) -> Vec<(Window, Kind)> {
    windows
        .iter()
        .map(|window| {
            let kind = if window.uid < FIRST_USER_UID {
                Kind::Picker
            } else if window.uid == me {
                Kind::Mine
            } else {
                let record = accounts(window.uid);
                let helper = Some(window.uid) == helper;
                let reused = record
                    .age
                    .is_some_and(|age| window.age >= age + REUSED_SLACK);

                if !record.exists() && Some(window.uid) != console {
                    Kind::Orphan
                } else if helper && reused && Some(window.uid) != console {
                    Kind::HelperLeftover
                } else if helper {
                    Kind::Helper
                } else {
                    Kind::OtherPerson
                }
            };

            (*window, kind)
        })
        .collect()
}

/// Why a new login could hang now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Blocker {
    /// A deleted account is still logged in.
    Orphan { uid: u32 },
    /// A deleted account is still logged in under the helper's user ID.
    HelperLeftover { uid: u32 },
    /// Another account started logging in long ago and never got its
    /// desktop.
    StuckLogin { uid: u32 },
    /// macOS is updating an account for its login.
    AccountUpdating,
}

impl Blocker {
    /// What it is, in the words that follow "Atlas can't switch yet:".
    pub fn words(&self) -> &'static str {
        match self {
            Self::Orphan { .. } | Self::HelperLeftover { .. } => {
                "a deleted account is still logged in."
            }
            Self::StuckLogin { .. } => "another account is still logging in.",
            Self::AccountUpdating => "macOS is still setting up an account.",
        }
    }

    /// The user ID whose login it is, if it is one login's.
    pub fn uid(&self) -> Option<u32> {
        match self {
            Self::Orphan { uid } | Self::HelperLeftover { uid } | Self::StuckLogin { uid } => {
                Some(*uid)
            }
            Self::AccountUpdating => None,
        }
    }

    /// Whether Atlas can close it: a deleted account's login.
    pub fn closable(&self) -> bool {
        matches!(self, Self::Orphan { .. } | Self::HelperLeftover { .. })
    }
}

/// How long a login may go without a desktop before it counts as stuck.
const STUCK_AFTER: Duration = Duration::from_secs(120);

/// What would make a new login hang, from the classified logins and the
/// process list: a deleted account's login, a login of another account
/// with no Dock and no Setup Assistant for too long, or macOS updating an
/// account.
fn blockers(classified: &[(Window, Kind)], listed: &[Process]) -> Vec<Blocker> {
    let mut found: Vec<Blocker> = classified
        .iter()
        .filter_map(|(window, kind)| match kind {
            Kind::Orphan => Some(Blocker::Orphan { uid: window.uid }),
            Kind::HelperLeftover => Some(Blocker::HelperLeftover { uid: window.uid }),
            Kind::Helper | Kind::OtherPerson
                if window.age > STUCK_AFTER
                    && !runs_as(listed, window.uid, "/Dock")
                    && !runs_as(listed, window.uid, "/Setup Assistant") =>
            {
                Some(Blocker::StuckLogin { uid: window.uid })
            }
            _ => None,
        })
        .collect();

    if updating(listed) {
        found.push(Blocker::AccountUpdating);
    }

    found
}

/// The loginwindow processes in a process list.
fn windows_in(listed: &[Process]) -> Vec<Window> {
    listed
        .iter()
        .filter(|process| process.command.ends_with("/loginwindow"))
        .map(|process| Window {
            uid: process.uid,
            pid: process.pid,
            age: process.running,
        })
        .collect()
}

/// The user ID that owns the screen now (0 at the login window), or
/// `None` when it cannot be read.
pub fn console_owner() -> Option<u32> {
    use std::os::unix::fs::MetadataExt;

    std::fs::metadata("/dev/console")
        .ok()
        .map(|metadata| metadata.uid())
}

/// What directory services say of `uid`, with how long ago its account was
/// made when `aged` (only the helper's age is ever compared). A lookup that
/// fails counts as an account found: an unknown is never taken for a
/// deleted account.
pub fn record(uid: u32, aged: bool) -> Record {
    let account = crate::setup::account_with_uid(uid);
    let passwd = !matches!(account, Ok(None));
    let directory = std::process::Command::new("/usr/bin/dscl")
        .args(["/Search", "-search", "/Users", "UniqueID", &uid.to_string()])
        .stdin(std::process::Stdio::null())
        .output()
        .map_or(true, |output| {
            !output.status.success() || !String::from_utf8_lossy(&output.stdout).trim().is_empty()
        });
    let age = account.ok().flatten().filter(|_| aged).and_then(|account| {
        account_age(&account.name).or_else(|| {
            std::fs::metadata(&account.home)
                .and_then(|metadata| metadata.created())
                .ok()
                .and_then(|created| created.elapsed().ok())
        })
    });

    Record {
        passwd,
        directory,
        age,
    }
}

/// Every desktop login on this Mac with whose it is, and what would make a
/// new login hang now. `helper` is the helper (or the account about to be
/// made it). `None` when the process list could not be read.
pub fn sessions(helper: Option<u32>) -> Option<Sessions> {
    let listed = processes()?;
    let me = nix::unistd::getuid().as_raw();
    let accounts = |uid| record(uid, Some(uid) == helper);
    let logins = classify(&windows_in(&listed), accounts, me, helper, console_owner());
    let blockers = blockers(&logins, &listed);

    Some(Sessions { logins, blockers })
}

/// Every desktop login on this Mac, classified, and what would make a new
/// login hang now.
#[derive(Debug, Clone, PartialEq)]
pub struct Sessions {
    pub logins: Vec<(Window, Kind)>,
    pub blockers: Vec<Blocker>,
}

impl Sessions {
    /// The logins Atlas may close: deleted accounts' logins, as
    /// `{"uid", "kind"}`.
    pub fn leftovers(&self) -> Vec<Value> {
        self.logins
            .iter()
            .filter(|(_, kind)| matches!(kind, Kind::Orphan | Kind::HelperLeftover))
            .map(|(window, kind)| json!({"uid": window.uid, "kind": kind}))
            .collect()
    }

    /// The user IDs of the deleted accounts still logged in, by kind.
    pub fn leftover_uids(&self, wanted: Kind) -> Vec<u32> {
        self.logins
            .iter()
            .filter(|(_, kind)| *kind == wanted)
            .map(|(window, _)| window.uid)
            .collect()
    }
}

/// What would make a new login hang now; `None` when the process list could
/// not be read.
pub fn blockers_now(helper: Option<u32>) -> Option<Vec<Blocker>> {
    sessions(helper).map(|sessions| sessions.blockers)
}

/// How long a login with its desktop up may go without Atlas answering
/// before its login counts as left behind rather than still starting.
const STALE_AFTER: Duration = Duration::from_secs(180);

/// Whether `uid`'s login is up and settled: its loginwindow has run for
/// over three minutes and its Dock runs. Called for a helper whose Atlas is
/// silent, it tells a login left behind from one still starting. `None`
/// when processes could not be listed.
pub fn stale(uid: u32) -> Option<bool> {
    let listed = processes()?;
    Some(stale_in(&listed, uid))
}

fn stale_in(listed: &[Process], uid: u32) -> bool {
    windows_in(listed)
        .iter()
        .any(|window| window.uid == uid && window.age > STALE_AFTER)
        && runs_as(listed, uid, "/Dock")
}

/// Whether the logins of `uids` are gone: no loginwindow and no Dock of
/// any of them, seen twice half a second apart, so a login on its way out
/// is not taken for one that ended (a Dock alone can restart in a login
/// that lives on). A process list that cannot be read is not gone.
pub fn confirm_gone(uids: &[u32]) -> bool {
    gone_twice(uids, processes, || {
        std::thread::sleep(Duration::from_millis(500));
    })
}

fn gone_twice(
    uids: &[u32],
    mut listed: impl FnMut() -> Option<Vec<Process>>,
    mut pause: impl FnMut(),
) -> bool {
    (0..2).all(|_| {
        pause();
        listed().is_some_and(|listed| uids.iter().all(|uid| logged_out_in(&listed, *uid)))
    })
}

/// How long ago account `name` was created, from its directory record.
fn account_age(name: &str) -> Option<std::time::Duration> {
    account_created(name)?.elapsed().ok()
}

/// The directory record's creation time; home-folder timestamps are not provenance.
pub(crate) fn account_created(name: &str) -> Option<std::time::SystemTime> {
    let output = std::process::Command::new("/usr/bin/dscl")
        .args([".", "-read", &format!("/Users/{name}"), "accountPolicyData"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let after = text.split("<key>creationTime</key>").nth(1)?;
    let value = after.split("<real>").nth(1)?.split("</real>").next()?;
    std::time::UNIX_EPOCH
        .checked_add(std::time::Duration::try_from_secs_f64(value.trim().parse().ok()?).ok()?)
}

/// `ps` elapsed time (`[[days-]hours:]minutes:seconds`) as a duration.
fn running_for(elapsed: &str) -> Option<std::time::Duration> {
    let (days, clock) = match elapsed.split_once('-') {
        Some((days, clock)) => (days.parse::<u64>().ok()?, clock),
        None => (0, elapsed),
    };

    let parts: Vec<u64> = clock
        .split(':')
        .map(|part| part.parse().ok())
        .collect::<Option<_>>()?;
    let seconds = match parts.as_slice() {
        [minutes, seconds] => minutes * 60 + seconds,
        [hours, minutes, seconds] => hours * 3600 + minutes * 60 + seconds,
        _ => return None,
    };

    Some(std::time::Duration::from_secs(days * 86_400 + seconds))
}

#[cfg(test)]
mod tests {
    use super::{
        Absence, Blocker, Kind, LOGGED_OUT, Process, Record, Window, blockers, classify,
        desktop_up_in, diagnose, gone_twice, input_build_current, logged_out_in, parse_processes,
        require_hidden, running_for, silent, stale_in, validate_target_uid,
    };
    use serde_json::json;
    use std::time::Duration;

    fn listed_processes(listing: &str) -> Vec<Process> {
        parse_processes(listing).expect("valid process list")
    }

    #[test]
    fn only_a_helper_that_did_not_answer_is_explained() {
        assert!(silent(
            &json!({"ok": false, "code": "io", "error": "No such file"})
        ));
        assert!(silent(&json!({"ok": false, "code": "transport"})));

        for code in ["blocked", "identity"] {
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

    /// A shell that may not reach the helper is told so, never that the
    /// helper stopped or that setup must run again.
    #[test]
    fn a_blocked_shell_is_not_a_stopped_helper() {
        let status = json!({"ok": false, "code": "blocked", "error": "This shell may not connect"});
        let uid = nix::unistd::getuid().as_raw() + 1;
        let error = require_hidden(&status, uid.max(501)).unwrap_err();
        assert_eq!(
            (error.code, error.error.as_str()),
            ("blocked", "This shell may not connect")
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
        use super::{Display, Epoch, Session, repinnable};

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
        assert!(repinnable(&status(1512.0), &session));
        assert!(
            !repinnable(&status(1440.0), &session),
            "a new size needs a new look"
        );

        let mut other = status(1512.0);
        other["inputService"]["enforcedEpoch"]["securitySessionId"] = json!(1);
        assert!(!repinnable(&other, &session), "never across sessions");
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
        assert!(found.iter().all(Blocker::closable));
        assert!(!Blocker::AccountUpdating.closable());
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
}
