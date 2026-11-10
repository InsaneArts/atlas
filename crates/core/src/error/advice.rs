//! What a person, or an agent, can do about each kind of failure, in one
//! sentence. The agent-facing table of the same codes is the plugin's
//! troubleshooting.md; a test keeps every code there covered here.
use crate::error::{Code, Holder};

/// The step for your own action that may or may not have happened.
pub const UNCERTAIN: &str = "It may or may not have happened, so don't repeat it. Take a screenshot, then run atlas restart.";

/// The step while every pending change of unknown outcome is the asking
/// session's own and none still runs.
pub const UNKNOWN_OWN: &str = "An earlier action of this session may not have finished. Take a screenshot, then run atlas restart.";

/// [`UNKNOWN_OWN`] while a command an agent started still runs, which
/// recovery waits for.
pub const UNKNOWN_OWN_ONCE_COMMAND_ENDS: &str = "An earlier action of this session may not have finished. Take a screenshot, then run atlas restart once the running command ends.";

/// The step for an agent while a pending change of unknown outcome may be
/// another's.
pub const UNKNOWN_OTHERS: &str = "An earlier action, maybe another agent's, may not have finished. Wait for it to be recovered, or tell the person.";

/// The step for the person while a change of unknown outcome is pending.
pub const UNKNOWN_FOR_PERSON: &str = "An action may not have finished. Look at the helper, then recover it in Atlas or with atlas restart.";

/// [`UNKNOWN_FOR_PERSON`] while a command an agent started still runs,
/// which recovery waits for.
pub const UNKNOWN_FOR_PERSON_ONCE_COMMAND_ENDS: &str = "An action may not have finished. Look at the helper, then recover it in Atlas or with atlas restart once the running command ends.";

/// What an agent does about busy while a command an agent started with
/// `run` still runs.
pub const COMMAND_RUNNING: &str =
    "Wait for the command to end, then try again. Do something else meanwhile.";

/// What an agent does about busy once Atlas has waited for the holder.
pub const WAITED: &str = "Atlas already waited for it. Do something else, then try again, or pass a longer --wait if your tool's timeout allows.";

pub const DISABLED_CONTROL: &str = "Choose an enabled control. If this is Save/Open, cancel with Escape and use atlas run to create files or atlas open to open them; do not retry an inert confirmation.";

/// What to do about a menu bar that is refused because another one already
/// serves the same folder.
pub const MENUBAR_RUNNING: &str =
    "It's already running: use that menu bar, or quit it before starting another.";

/// What an agent does about an app that runs in one account at a time and
/// is already open in another, usually the person's.
pub const OPEN_IN_ANOTHER_ACCOUNT: &str = "It can't open in the helper while it's open there. Tell the person; don't retry it, and never quit their copy.";

/// What an agent does about a failure: the uncertain step when it may or
/// may not have happened, else the code's. Waiting for the person (agents
/// stopped, the person cancelled it, holds control or is at the helper's
/// screen) keeps its own step either way, since an agent may not act until
/// the person says. A code this build does not know has the step for
/// what is not known to be wrong.
pub fn step(code: Option<Code>, holder: Option<Holder>, uncertain: bool) -> &'static str {
    let waiting_for_person = matches!(
        (code, holder),
        (Some(Code::Stopped | Code::Cancelled | Code::OnConsole), _)
            | (Some(Code::Busy), Some(Holder::Person))
    );
    match code {
        _ if uncertain && !waiting_for_person => UNCERTAIN,
        Some(code) => next(code, holder),
        None => UNKNOWN_FAILURE,
    }
}

/// What an agent does about a failure no step is written for.
const UNKNOWN_FAILURE: &str = "Take a screenshot, then run atlas doctor to see what is wrong.";

/// What an agent does about a failure with `code`, in one sentence; who
/// holds control changes it for busy. Codes whose step is the same for a
/// person use theirs.
pub fn next(code: Code, holder: Option<Holder>) -> &'static str {
    match (code, holder) {
        (Code::Busy, Some(Holder::Person)) => {
            "Wait until the person gives control back, then look at the helper before trying again. Never take it from them."
        }
        (Code::Busy, Some(Holder::Agent)) => {
            "Another Atlas command is running, often another agent's, and Atlas waited for it unless told not to. Do something else, then try again later."
        }
        (Code::Busy, _) => "Wait, then try again. Never take control from the person.",
        (Code::Stopped, _) => {
            "Don't repeat it, as it may have happened. Stop and ask the person what to do; only they can resume agents."
        }
        (Code::Uncertain, _) => {
            "An earlier action's outcome is unknown. Take a screenshot; run atlas restart only if that action was yours, otherwise ask the person first."
        }
        (Code::Cancelled, _) => {
            "The person stopped it, maybe partway. Look at the helper, then ask them before trying again."
        }
        (Code::Terminal, _) => {
            "Ask the person. Pass the flag it names, such as --yes, only with their agreement."
        }
        (Code::OnConsole, _) => {
            "Ask the person to switch back to their own account. Don't try again until they have."
        }
        (Code::LoggedOut, _) => {
            "Ask the person to log in to the helper once, then switch back. Don't run atlas init for this."
        }
        (Code::LogoutHeld, _) => {
            "Take a screenshot and show the person. Use --force only if they agree, since unsaved work is lost."
        }
        (Code::Permission, _) => {
            "Ask the person to allow Atlas in the helper's System Settings › Privacy & Security."
        }
        (Code::Blocked, _) => {
            "Run atlas outside your sandbox, or ask for that permission. Don't run atlas init for this."
        }
        (Code::Config | Code::Setup, _) => {
            "Atlas is not set up. Use the setup skill, or ask the person to open Atlas."
        }
        (Code::Init, _) => "Ask the person to run atlas init again. It picks up where it stopped.",
        (Code::ConfigInvalid | Code::Uninstall, _) => "Show the error to the person.",
        (Code::Credentials, _) => {
            "Don't copy it another way. If the task needs an account, ask the person to sign the helper in; copy only what the test needs, without secrets."
        }
        (Code::WaitingForPerson, _) => {
            "Do other work, then run the same command again; if some of it ran, follow next. Never send it another way."
        }
        (Code::Denied, _) => "Don't try another way. Ask the person what they want instead.",
        (Code::Onboarding, _) => "Show the error to the person. They open Atlas again to carry on.",
        (Code::Install, _) => "Ask the person to move Atlas.app to /Applications.",
        (Code::Logout, _) => {
            "Take a screenshot. If the helper is still logged in, run atlas restart."
        }
        (Code::NotReady, _) => "Tell the person what the error says. Don't try again in a loop.",
        (Code::Switch, _) => {
            "Ask the person to lock the screen from the Apple menu and switch there."
        }
        (Code::Update, _) => "Show the error to the person. Never bypass the signature checks.",
        (Code::FocusChanged, _) => FOCUS_CHANGED,
        (Code::Interrupted, _) => {
            "Atlas was stopped partway, and nothing it did is left unknown. Look at the helper before going on."
        }
        (Code::Collect, _) => {
            "Read reason: the file was not copied, and nothing in the helper changed."
        }
        (Code::Job, _) => "Run atlas job list to see the job's state; never stop it another way.",
        (Code::Cleanup, _) => {
            "Read steps: run atlas cleanup --apply again for those that may be retried, and tell the person what was kept or keeps failing."
        }
        (
            Code::Agents
            | Code::Apps
            | Code::BadRequest
            | Code::Capture
            | Code::Daemon
            | Code::Elements
            | Code::Environment
            | Code::Identity
            | Code::Input
            | Code::InvalidJson
            | Code::Io
            | Code::LeaseLost
            | Code::Menubar
            | Code::Native
            | Code::NoEnvironment
            | Code::NoWindow
            | Code::Platform
            | Code::PortInUse
            | Code::Protocol
            | Code::Restart
            | Code::SessionChanged
            | Code::SpawnFailed
            | Code::StaleFrame
            | Code::Startup
            | Code::State
            | Code::Transport
            | Code::Usage
            | Code::WaitFailed
            | Code::Windows,
            _,
        ) => fix(code).unwrap_or(UNKNOWN_FAILURE),
    }
}

/// What an agent does about an action refused because another app was in
/// front, when nothing of it was sent.
const FOCUS_CHANGED: &str = "Another app was in front, maybe another agent's, and nothing of the command was sent. Click where your app shows, then run it again.";

/// What was in the way of an action refused because focus was elsewhere,
/// as the start of a sentence, and what to do about it before going on,
/// in lowercase: by the refusal's `reason` and, when Atlas could not tell,
/// its `unread`. Another app for a reason this build does not know.
fn in_the_way(reason: Option<&str>, unread: Option<&str>) -> (&'static str, &'static str) {
    match (reason, unread) {
        (Some("window"), _) => (
            "Another window of your app had keyboard focus, was in front of yours or was at the point",
            "click in your window",
        ),
        (Some("sheet"), _) => (
            "A sheet or dialog was open over your window",
            "take a screenshot and deal with the sheet",
        ),
        (Some("field"), _) => (
            "Another field had keyboard focus, or none did",
            "click in your field (if this keeps happening, the app does not say which field has focus, as browsers and custom views often don't: use --window instead)",
        ),
        (Some("disabled"), _) => (
            "That field is disabled",
            "wait until it is enabled, or choose another field",
        ),
        (Some("unreadable"), Some("alike")) => (
            "Two windows of your app look the same to Atlas (one place, size and title), so it couldn't tell which has focus",
            "move or close one of them",
        ),
        (Some("unreadable"), Some("unlisted")) => (
            "The window with keyboard focus was not listed yet, as when it is still opening",
            "take a screenshot",
        ),
        (Some("unreadable"), Some("withheld")) => (
            "Your app withheld its windows, as apps do while the Mac's screen is locked",
            "ask the person to unlock the Mac",
        ),
        (Some("unreadable"), Some("cut")) => (
            "A title or name Atlas checks was too long to read whole",
            "check this window or field with --app alone",
        ),
        (Some("unreadable"), Some("deep")) => (
            "The focused element sits deeper in your app than Atlas reads",
            "check this window or field with --app alone",
        ),
        (Some("unreadable"), Some("slow")) => {
            ("Your app did not say where its focus is", "wait a moment")
        }
        (Some("unreadable"), _) => (
            "Atlas couldn't tell what was in front, often because the active app has no window",
            "click where your app shows, or open the app if none of it shows",
        ),
        _ => (
            "Another app was in front, maybe another agent's",
            "click where your app shows",
        ),
    }
}

/// The next step for a failed result that is a focus refusal, its outcome
/// known: by what was in the way and whether some of the command was sent
/// first, in which case only the rest is sent again. None for anything
/// else, which [`step`] or [`fix`] advise on.
pub fn focus_advice(result: &serde_json::Value) -> Option<String> {
    if result["code"] != "focus-changed" || result["uncertain"] == true {
        return None;
    }

    let counted = |field: &str| result[field].as_u64().is_some_and(|count| count > 0);
    let sent = counted("performed") || counted("typed") || result["partial"] == true;
    let (reason, unread) = (result["reason"].as_str(), result["unread"].as_str());
    let (what, remedy) = in_the_way(reason, unread);
    if sent {
        return Some(format!(
            "{what}. Earlier actions were sent: check typed or performed and what landed, {remedy}, then send only the rest."
        ));
    }
    // An app that does not say where its focus is may keep not saying it,
    // so repeating the command is not promised to work.
    if (reason, unread) == (Some("unreadable"), Some("slow")) {
        return Some(format!(
            "{what}, and nothing was sent. Wait a moment and try once more; if it refuses again, \
             tell the person the app is not answering instead of repeating the command."
        ));
    }

    let remedy = crate::name::capitalized(remedy);
    Some(format!(
        "{what}, and nothing of the command was sent. {remedy}, then run it again."
    ))
}

/// The next step for a failure with `code`.
pub fn fix(code: Code) -> Option<&'static str> {
    Some(match code {
        Code::Agents => "Run atlas agents to see which agents are installed.",
        Code::Apps | Code::Capture | Code::Protocol | Code::Windows => {
            "Try again. If it keeps failing, run atlas doctor."
        }
        Code::BadRequest | Code::Usage => "Add --help to the command to see what it accepts.",
        Code::Cleanup => "Run atlas cleanup --apply again, or see what was kept in its reply.",
        Code::Collect => "Check the file in the helper, then collect it again.",
        Code::Job => "Check atlas job list.",
        Code::Blocked => "Run atlas outside the sandbox that blocks it, such as an agent's.",
        Code::Busy => {
            "Someone else has control, often you in the viewer. Release it there, or wait."
        }
        Code::Cancelled | Code::Terminal => return None,
        Code::Config | Code::Setup => "Run atlas init to finish setting up.",
        Code::ConfigInvalid => "Fix that file, or move it aside and run atlas init.",
        Code::Credentials => {
            "Sign the helper in to the account instead, and select only files without secrets."
        }
        Code::WaitingForPerson => "Answer it in the Atlas menu bar, or in the viewer.",
        Code::Denied => {
            "Change the account's level in Atlas › Settings › Accounts if agents should act there."
        }
        Code::Daemon | Code::State => "Run atlas restart.",
        Code::Elements => {
            "Check the app is running with a window (atlas windows), then narrow it with --name or --role, or click the point from a screenshot instead."
        }
        Code::Environment => "Check what it names, then try again. Nothing is ever overwritten.",
        Code::FocusChanged => "Bring the app to the front, then try again.",
        Code::Identity | Code::Native | Code::Restart => {
            "Run atlas doctor to see what is not ready."
        }
        Code::Init => "Run atlas init again. It picks up where it stopped.",
        Code::Input => "Check the action, take a screenshot, then try again.",
        Code::Install => "Move Atlas.app to /Applications, then try again.",
        Code::Interrupted => "Run it again if you still want it; look at the helper first.",
        Code::InvalidJson => "Check the JSON you passed.",
        Code::PortInUse => "Pass --port with a free port, or stop what is using it.",
        Code::Io => "Check the path and its permissions, then try again.",
        Code::LeaseLost => "Control was taken back. Take a screenshot before trying again.",
        Code::LoggedOut => {
            "Log in to it once, then switch back: atlas switch locks the screen for that."
        }
        Code::Logout => {
            "Look at the helper in the viewer. If it is still logged in, run atlas restart."
        }
        Code::LogoutHeld => {
            "Run atlas logout --force to end it anyway (unsaved work is lost), or atlas restart, then answer the app in the viewer."
        }
        Code::Menubar => "The rest of Atlas works without the menu bar.",
        Code::NoEnvironment => "Use atlas run, or set one up with atlas environment profile.",
        Code::NoWindow => "Open a window in the app, or take a full screenshot.",
        Code::NotReady => {
            "Run atlas doctor. A deleted account's login closes in Atlas › Settings › Helper or with atlas init; otherwise wait a minute and try again."
        }
        Code::OnConsole => {
            "Switch back to your own account. Atlas works while the helper is in the background."
        }
        Code::Onboarding => "Open Atlas again to carry on setting up, or run atlas init.",
        Code::Permission => {
            "In the helper account, open System Settings › Privacy & Security and turn Atlas on. atlas init walks you through it."
        }
        Code::Platform => "Atlas runs on macOS only.",
        Code::SessionChanged => {
            "The helper changed meanwhile. Take a new screenshot and try again."
        }
        Code::SpawnFailed => "Check that the program or app exists in the helper account.",
        Code::StaleFrame => "Take a new screenshot.",
        Code::Startup => "Run atlas start-input, then atlas doctor.",
        Code::Stopped => "Choose Resume Agent in the Atlas menu bar when the agent may go on.",
        Code::Switch => "Use the Apple menu: Lock Screen, then Switch User.",
        Code::Transport => "Run atlas doctor. If an action was cut off, run atlas restart.",
        Code::Uncertain => "Take a screenshot to see what happened, then run atlas restart.",
        Code::Uninstall => "Fix what it names, then run atlas uninstall again.",
        Code::Update => "Try again later. Atlas never installs a build that fails its checks.",
        Code::WaitFailed => {
            "Look at the helper first (atlas windows, a screenshot), then wait or run it again."
        }
    })
}

#[cfg(test)]
#[path = "advice_tests.rs"]
mod tests;
