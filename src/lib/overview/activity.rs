//! A logged row as every app shows it: what it did in words, who did it,
//! how long ago, and why it failed in plain words.
use crate::{
    activity_log::{Failure, Op, Row},
    error::Code,
};
use serde::{Deserialize, Serialize};

/// Why a failed action failed, in plain words, from the fixed code its row
/// keeps; nil for an action that worked or a code with no plain reason.
pub(super) fn reason(row: &Row) -> Option<&'static str> {
    if !row.failed() {
        return None;
    }
    let focus = row.reason.as_deref();
    Some(match row.code.as_ref()? {
        Failure::Code(Code::Busy) => "Something else had the mouse and keyboard",
        Failure::Code(Code::Stopped) => "Agents were stopped",
        Failure::Code(Code::Elements) => "Nothing on screen matched",
        Failure::Code(Code::Windows) => "The window wasn't there",
        Failure::Code(Code::NoWindow) => "The app had no window on screen",
        Failure::Code(Code::Input) => "The mouse and keyboard weren't ready",
        Failure::Code(Code::Capture) => "The screen couldn't be read",
        Failure::Code(Code::Permission) => match row.operation {
            Some(Op::Screenshot) => "The helper can't record the screen",
            Some(Op::Input) => "The helper can't use the mouse and keyboard",
            _ => "The helper doesn't have permission",
        },
        Failure::Code(Code::SessionChanged) => "The helper's screen changed meanwhile",
        Failure::Code(Code::FocusChanged) if focus == Some("unreadable") => {
            "Atlas couldn't tell which app was in front"
        }
        Failure::Code(Code::FocusChanged) if focus == Some("window") => {
            "Another window was in front"
        }
        Failure::Code(Code::FocusChanged) if focus == Some("sheet") => {
            "A sheet was open over the window"
        }
        Failure::Code(Code::FocusChanged) if focus == Some("field") => "Another field had focus",
        Failure::Code(Code::FocusChanged) if focus == Some("disabled") => "That field is disabled",
        Failure::Code(Code::FocusChanged) => "Another app was in front",
        Failure::Code(Code::LeaseLost) => "Control ended before it finished",
        Failure::Code(Code::Cancelled) => "It was cancelled",
        Failure::Code(Code::Uncertain) => "It may not have finished",
        Failure::Timeout => "It took too long",
        Failure::Code(Code::Job) => "A background job was refused",
        Failure::Code(Code::Collect) => "A file could not be collected",
        Failure::Exit => "The command failed",
        Failure::Code(Code::Transport)
        | Failure::Code(Code::Identity)
        | Failure::Code(Code::Native)
        | Failure::Code(Code::Daemon)
        | Failure::Code(Code::OnConsole) => "Atlas couldn't reach the helper",
        _ => return None,
    })
}

/// A logged row as every app shows it: what it did in words, who did it
/// and how long ago.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Shown {
    /// The row's own id, the same on every read, so an app can keep a
    /// line's identity as newer ones arrive.
    pub(crate) id: String,
    pub(crate) operation: Option<Op>,
    /// A command under way is not a failure: apps that read only `ok` show
    /// it as going well.
    pub(crate) ok: bool,
    pub(crate) action_count: Option<u64>,
    pub(crate) started: bool,
    pub(crate) verb: Option<String>,
    pub(crate) app: Option<String>,
    /// The bundle identifier of the app it acted in, as the row keeps it.
    pub(crate) bundle_id: Option<String>,
    pub(crate) label: Option<String>,
    /// The host session's key and the screenshot's file name, as the row
    /// keeps them; apps name the session from their own list.
    pub(crate) session: Option<String>,
    pub(crate) capture: Option<String>,
    /// Refused before anything was sent, as when another app was in front:
    /// not a problem, and nothing to recover.
    pub(crate) nothing_sent: bool,
    /// Such a refusal after part of its text was typed.
    pub(crate) partial: bool,
    /// Its outcome is unknown until the person recovers.
    pub(crate) uncertain: bool,
    pub(crate) says: String,
    pub(crate) reason: Option<String>,
    pub(crate) actor: String,
    pub(crate) age_seconds: Option<u64>,
}

impl Shown {
    /// Safe keycaps from a key row's retained label; never raw input.
    pub(crate) fn keys(&self) -> Option<Vec<Vec<String>>> {
        (self.verb.as_deref() == Some("key"))
            .then(|| crate::doing::chords(self.label.as_deref().unwrap_or_default()))
    }
}

/// `row` as every app shows it, at `now`.
pub(crate) fn shown(row: &Row, now: u128) -> Shown {
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
