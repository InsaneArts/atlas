//! What the helper's status proves about its session: the display and the
//! processes an action is tied to (its [`Epoch`]), whether the helper is the
//! hidden graphical login it must be, and the doctor report built from it.
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
/// The lowest UID macOS gives an account a person signs in to.
const FIRST_USER_UID: u32 = 501;
/// The largest display side, in points, a session may report.
const MAX_SIDE: f64 = 32768.0;
/// The largest backing scale a display may report.
const MAX_SCALE: f64 = 8.0;

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
        || !within(display.width, MAX_SIDE)
        || !within(display.height, MAX_SIDE)
        || !within(display.scale, MAX_SCALE)
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
    let not_queried = "Unknown until the helper runs in the background.";
    let known = |fact: bool, ok: bool| fact.then_some(ok);
    let unknown = "Unknown until the helper is running.";
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
                "Run atlas start-input."
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
                "Run atlas start-input."
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

/// Whether a helper that did not answer is signed out: not refused by this
/// shell or answered by another account, and `absent` (the account exists
/// but no desktop of it runs). `absent` is only asked when the rest holds.
pub fn signed_out(status: &Value, absent: impl FnOnce() -> bool) -> bool {
    status["ok"] != true && status["code"] != "blocked" && status["code"] != "identity" && absent()
}

/// Whether a desktop session running as `uid` is older than the account's
/// home folder: a deleted account's session, still signed in, that macOS
/// now attributes to the account that reused its user ID.
pub fn leftover_session(uid: u32, name: &str, home: &std::path::Path) -> bool {
    // How long ago the account was made: its record's creation time, or
    // failing that its home folder's.
    let Some(age) = account_age(name).or_else(|| {
        std::fs::metadata(home)
            .and_then(|metadata| metadata.created())
            .ok()
            .and_then(|created| created.elapsed().ok())
    }) else {
        return false;
    };
    let Ok(output) = std::process::Command::new("/bin/ps")
        .args(["-axo", "uid=,etime=,comm="])
        .output()
    else {
        return false;
    };
    // A session cannot start before its account exists; a few seconds of
    // slack cover clock rounding.
    let slack = std::time::Duration::from_secs(10);
    String::from_utf8_lossy(&output.stdout).lines().any(|line| {
        let mut words = line.split_whitespace();
        let (Some(owner), Some(elapsed), Some(command)) =
            (words.next(), words.next(), words.next())
        else {
            return false;
        };
        owner == uid.to_string()
            && command.ends_with("/loginwindow")
            && running_for(elapsed).is_some_and(|running| running > age + slack)
    })
}

/// How long ago account `name` was created, from its directory record.
fn account_age(name: &str) -> Option<std::time::Duration> {
    let output = std::process::Command::new("/usr/bin/dscl")
        .args([".", "-read", &format!("/Users/{name}"), "accountPolicyData"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let after = text.split("<key>creationTime</key>").nth(1)?;
    let value = after.split("<real>").nth(1)?.split("</real>").next()?;
    let created =
        std::time::UNIX_EPOCH + std::time::Duration::from_secs_f64(value.trim().parse().ok()?);
    created.elapsed().ok()
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
        diagnose, input_build_current, require_hidden, running_for, signed_out, validate_target_uid,
    };

    #[test]
    fn only_a_silent_helper_with_no_desktop_is_signed_out() {
        let silent = json!({"ok": false, "code": "io", "error": "No such file"});
        assert!(signed_out(&silent, || true));
        assert!(!signed_out(&silent, || false), "its desktop runs");
        for code in ["blocked", "identity"] {
            let other = json!({"ok": false, "code": code});
            assert!(!signed_out(&other, || panic!("not asked")), "{code}");
        }
        assert!(!signed_out(&json!({"ok": true}), || panic!("not asked")));
    }
    use serde_json::json;

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
