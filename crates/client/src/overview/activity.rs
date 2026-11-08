//! A logged row as every app shows it: what it did in words, who did it,
//! how long ago, and why it failed in plain words.
use crate::activity_log::{Failure, Op, Row};
use atlas_core::error::Code;
use serde::{Deserialize, Serialize};

/// Why a failed action failed, in plain words, from the fixed code its row
/// keeps; nil for an action that worked or a code with no plain reason.
pub(crate) fn reason(row: &Row) -> Option<&'static str> {
    if !row.failed() {
        return None;
    }
    let focus = row.reason.as_deref();
    let code = match row.code.as_ref()? {
        Failure::Code(code) => code,
        Failure::Timeout => return Some("It took too long"),
        Failure::Exit => return Some("The command failed"),
        Failure::Later(_) => return None,
    };
    Some(match code {
        Code::Busy => "Something else had the mouse and keyboard",
        Code::Stopped => "Agents were stopped",
        Code::Elements => "Nothing on screen matched",
        Code::Windows => "The window wasn't there",
        Code::NoWindow => "The app had no window on screen",
        Code::Input => "The mouse and keyboard weren't ready",
        Code::Capture => "The screen couldn't be read",
        Code::Permission => match row.operation {
            Some(Op::Screenshot) => "The helper can't record the screen",
            Some(Op::Input) => "The helper can't use the mouse and keyboard",
            _ => "The helper doesn't have permission",
        },
        Code::SessionChanged => "The helper's screen changed meanwhile",
        Code::FocusChanged if focus == Some("unreadable") => {
            "Atlas couldn't read which app was in front"
        }
        Code::FocusChanged if focus == Some("window") => "Another window was in front",
        Code::FocusChanged if focus == Some("sheet") => "A sheet was open over the window",
        Code::FocusChanged if focus == Some("field") => "Another field had focus",
        Code::FocusChanged if focus == Some("disabled") => "That field is disabled",
        Code::FocusChanged => "Another app was in front",
        Code::LeaseLost => "Control ended before it finished",
        Code::Cancelled => "It was cancelled",
        Code::Uncertain => "It may not have finished",
        Code::Job => "A background job was refused",
        Code::Collect => "A file could not be collected",
        Code::Transport | Code::Identity | Code::Native | Code::Daemon | Code::OnConsole => {
            "Atlas couldn't reach the helper"
        }
        Code::Agents
        | Code::Apps
        | Code::BadRequest
        | Code::Blocked
        | Code::Cleanup
        | Code::Config
        | Code::ConfigInvalid
        | Code::Credentials
        | Code::Environment
        | Code::Init
        | Code::Install
        | Code::Interrupted
        | Code::InvalidJson
        | Code::Io
        | Code::LoggedOut
        | Code::Logout
        | Code::LogoutHeld
        | Code::Menubar
        | Code::NoEnvironment
        | Code::NotReady
        | Code::Onboarding
        | Code::Platform
        | Code::PortInUse
        | Code::Protocol
        | Code::Restart
        | Code::Setup
        | Code::SpawnFailed
        | Code::StaleFrame
        | Code::Startup
        | Code::State
        | Code::Switch
        | Code::Terminal
        | Code::Uninstall
        | Code::Update
        | Code::Usage
        | Code::WaitFailed => return None,
    })
}

/// A logged row as every app shows it: what it did in words, who did it
/// and how long ago.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shown {
    /// The row's own id, the same on every read, so an app can keep a
    /// line's identity as newer ones arrive.
    pub(crate) id: String,
    pub(crate) operation: Option<Op>,
    /// A command under way is not a failure: apps that read only `ok` show
    /// it as going well.
    pub ok: bool,
    pub(crate) action_count: Option<u64>,
    pub started: bool,
    pub(crate) verb: Option<String>,
    pub(crate) app: Option<String>,
    /// The bundle identifier of the app it acted in, as the row keeps it.
    pub(crate) bundle_id: Option<String>,
    pub(crate) label: Option<String>,
    /// The host session's key and the screenshot's file name, as the row
    /// keeps them; apps name the session from their own list.
    pub session: Option<String>,
    pub(crate) capture: Option<String>,
    /// Refused before anything was sent, as when another app was in front:
    /// not a problem, and nothing to recover.
    pub nothing_sent: bool,
    /// Such a refusal after part of its text was typed.
    pub(crate) partial: bool,
    /// Its outcome is unknown until the person recovers.
    pub(crate) uncertain: bool,
    pub says: String,
    pub reason: Option<String>,
    pub actor: String,
    pub age_seconds: Option<u64>,
}

impl Shown {
    /// Safe keycaps from a key row's retained label; never raw input.
    pub fn keys(&self) -> Option<Vec<Vec<String>>> {
        (self.verb.as_deref() == Some("key"))
            .then(|| crate::doing::chords(self.label.as_deref().unwrap_or_default()))
    }
}

/// `row` as every app shows it, at `now`.
pub fn shown(row: &Row, now: u128) -> Shown {
    Shown {
        id: row.id.clone(),
        operation: row.operation.clone(),
        ok: row.worked() || row.started == Some(true),
        action_count: row.action_count,
        started: row.started == Some(true),
        verb: row.verb.clone(),
        app: row.app.clone(),
        bundle_id: row.bundle_id.clone(),
        label: row.label.clone(),
        session: row.session.clone(),
        capture: row.capture.clone(),
        nothing_sent: row.coded(Code::FocusChanged) && row.partial != Some(true),
        partial: row.partial == Some(true),
        uncertain: row.coded(Code::Uncertain),
        says: crate::doing::says(row),
        reason: reason(row).map(str::to_owned),
        actor: row.actor().to_owned(),
        age_seconds: row
            .age(now)
            .map(|nanos| u64::try_from(nanos / 1_000_000_000).unwrap_or(u64::MAX)),
    }
}
