//! A logged row as every app shows it: what it did in words, who did it,
//! how long ago, and why it failed in plain words.
use crate::activity_log::{Failure, Op, Row};
use atlas_core::error::Code;
use serde::{Deserialize, Serialize};

/// What an activity's reasons call the desktop it was done on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Called<'a> {
    /// Second Login's: the helper.
    Helper,
    /// A VM, by its name.
    Vm(&'a str),
    /// A VM whose name is not known here.
    UnnamedVm,
}

impl Called<'_> {
    /// The desktop at the start of a sentence.
    fn subject(self) -> String {
        match self {
            Self::Helper => "The helper".to_owned(),
            Self::Vm(name) => name.to_owned(),
            Self::UnnamedVm => "The VM".to_owned(),
        }
    }

    /// The desktop within a sentence.
    fn object(self) -> String {
        match self {
            Self::Helper => "the helper".to_owned(),
            Self::Vm(name) => name.to_owned(),
            Self::UnnamedVm => "the VM".to_owned(),
        }
    }
}

/// Why a failed action failed, in plain words, from the fixed code its row
/// keeps, naming the desktop as `called` does; nil for an action that
/// worked or a code with no plain reason.
pub(crate) fn reason(row: &Row, called: Called<'_>) -> Option<String> {
    if !row.failed() {
        return None;
    }
    let focus = row.reason.as_deref();
    let code = match row.code.as_ref()? {
        Failure::Code(code) => code,
        Failure::Timeout => return Some("It took too long".to_owned()),
        Failure::Exit => return Some("The command failed".to_owned()),
        Failure::Later(_) => return None,
    };
    let on_desktop = match code {
        Code::Permission => Some(match row.operation {
            Some(Op::Screenshot) => format!("{} can't record the screen", called.subject()),
            Some(Op::Input) => format!("{} can't use the mouse and keyboard", called.subject()),
            _ => format!("{} doesn't have permission", called.subject()),
        }),
        Code::SessionChanged => Some(format!("{}'s screen changed meanwhile", called.subject())),
        Code::Transport | Code::Identity | Code::Native | Code::Daemon | Code::OnConsole => {
            Some(format!("Atlas couldn't reach {}", called.object()))
        }
        _ => None,
    };
    if on_desktop.is_some() {
        return on_desktop;
    }
    Some(
        match code {
            Code::Busy => "Something else had the mouse and keyboard",
            Code::Stopped => "Agents were stopped",
            Code::Elements => "Nothing on screen matched",
            Code::Windows => "The window wasn't there",
            Code::NoWindow => "The app had no window on screen",
            Code::Input => "The mouse and keyboard weren't ready",
            Code::Capture => "The screen couldn't be read",
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
            Code::WaitingForPerson => "Atlas asked you before sending",
            Code::Denied => "You didn't let it send",
            Code::Permission
            | Code::SessionChanged
            | Code::Transport
            | Code::Identity
            | Code::Native
            | Code::Daemon
            | Code::OnConsole
            | Code::Agents
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
        }
        .to_owned(),
    )
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
    /// The listed account it acted in, and what let it act there: in the
    /// person's own app alone ([`shown_to_person`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) account: Option<crate::activity_log::Account>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) allowed: Option<crate::activity_log::Allowed>,
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

/// `row` as every app shows it, at `now`, its reason naming the desktop
/// as `called` does.
pub fn shown(row: &Row, now: u128, called: Called<'_>) -> Shown {
    Shown {
        reason: reason(row, called),
        ..listed(row, now)
    }
}

/// [`shown`], with no reason given.
pub(crate) fn listed(row: &Row, now: u128) -> Shown {
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
        account: None,
        allowed: None,
        session: row.session.clone(),
        capture: row.capture.clone(),
        nothing_sent: row.coded(Code::FocusChanged) && row.partial != Some(true),
        partial: row.partial == Some(true),
        uncertain: row.coded(Code::Uncertain),
        says: crate::doing::says(row),
        reason: None,
        actor: row.actor().to_owned(),
        age_seconds: row
            .age(now)
            .map(|nanos| u64::try_from(nanos / 1_000_000_000).unwrap_or(u64::MAX)),
    }
}

/// [`shown`] as the person's own app shows it: with the listed account it
/// acted in and what let it act there, and a browser's action said on its
/// site. No agent is shown either.
pub fn shown_to_person(row: &Row, now: u128, called: Called<'_>) -> Shown {
    Shown {
        account: row.account.clone(),
        allowed: row.account.as_ref().and(row.allowed),
        says: crate::doing::says_to_person(row),
        ..shown(row, now, called)
    }
}
