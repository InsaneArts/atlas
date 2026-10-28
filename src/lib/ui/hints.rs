//! What a person, or an agent, can do about each kind of failure, in one
//! sentence. The agent-facing table of the same codes is the plugin's
//! troubleshooting.md; a test keeps every code there covered here.
use crate::error::Holder;

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

/// What an agent does about a failure: the uncertain step when it may or
/// may not have happened, else the code's. Waiting for the person (agents
/// stopped, the person cancelled it, holds control or is at the helper's
/// screen) keeps its own step either way, since an agent may not act until
/// the person says.
pub fn step(code: &str, holder: Option<Holder>, uncertain: bool) -> &'static str {
    let waiting_for_person = matches!(
        (code, holder),
        ("stopped" | "cancelled" | "on-console", _) | ("busy", Some(Holder::Person))
    );
    if uncertain && !waiting_for_person {
        UNCERTAIN
    } else {
        next(code, holder)
    }
}

/// What an agent does about a failure with `code`, in one sentence; who
/// holds control changes it for busy. Codes whose step is the same for a
/// person use theirs.
pub fn next(code: &str, holder: Option<Holder>) -> &'static str {
    match (code, holder) {
        ("busy", Some(Holder::Person)) => {
            "Wait until the person gives control back, then look at the helper before trying again. Never take it from them."
        }
        ("busy", Some(Holder::Agent)) => {
            "Another Atlas command is running, often another agent's, and Atlas waited for it unless told not to. Do something else, then try again later."
        }
        ("busy", _) => "Wait, then try again. Never take control from the person.",
        ("stopped", _) => {
            "Don't repeat it, as it may have happened. Stop and ask the person what to do; only they can resume agents."
        }
        ("uncertain", _) => {
            "An earlier action's outcome is unknown. Take a screenshot; run atlas restart only if that action was yours, otherwise ask the person first."
        }
        ("cancelled", _) => {
            "The person stopped it, maybe partway. Look at the helper, then ask them before trying again."
        }
        ("terminal", _) => {
            "Ask the person. Pass the flag it names, such as --yes, only with their agreement."
        }
        ("on-console", _) => {
            "Ask the person to switch back to their own account. Don't try again until they have."
        }
        ("logged-out", _) => {
            "Ask the person to log in to the helper once, then switch back. Don't run atlas init for this."
        }
        ("logout-held", _) => {
            "Take a screenshot and show the person. Use --force only if they agree, since unsaved work is lost."
        }
        ("permission", _) => {
            "Ask the person to allow Atlas in the helper's System Settings › Privacy & Security."
        }
        ("blocked", _) => {
            "Run atlas outside your sandbox, or ask for that permission. Don't run atlas init for this."
        }
        ("config" | "setup", _) => {
            "Atlas is not set up. Use the setup skill, or ask the person to open Atlas."
        }
        ("init", _) => "Ask the person to run atlas init again. It picks up where it stopped.",
        ("config-invalid" | "uninstall", _) => "Show the error to the person.",
        ("onboarding", _) => "Show the error to the person. They open Atlas again to carry on.",
        ("install", _) => "Ask the person to move Atlas.app to /Applications.",
        ("logout", _) => "Take a screenshot. If the helper is still logged in, run atlas restart.",
        ("not-ready", _) => "Tell the person what the error says. Don't try again in a loop.",
        ("switch", _) => "Ask the person to lock the screen from the Apple menu and switch there.",
        ("update", _) => "Show the error to the person. Never bypass the signature checks.",
        ("focus-changed", _) => focus_changed(false, InTheWay::App),
        ("interrupted", _) => {
            "Atlas was stopped partway, and nothing it did is left unknown. Look at the helper before going on."
        }
        ("cleanup", _) => {
            "Read steps: run atlas cleanup --apply again for those that may be retried, and tell the person what was kept or keeps failing."
        }
        _ => fix(code).unwrap_or("Take a screenshot, then run atlas doctor to see what is wrong."),
    }
}

/// What was in the way of an action refused because focus was elsewhere,
/// from the refusal's `reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InTheWay {
    /// Another app had keyboard focus, or its window was at the point.
    App,
    /// Another window of the app had keyboard focus, or was at the point.
    Window,
    /// A sheet over the window had keyboard focus.
    Sheet,
    /// Another field had keyboard focus, or the named one was disabled.
    Field,
    /// Atlas could not tell what had keyboard focus.
    Unknown,
}

impl InTheWay {
    /// What a refusal's `reason` says was in the way; another app for a
    /// reason this build does not know.
    pub fn of(reason: Option<&str>) -> Self {
        match reason {
            Some("window") => Self::Window,
            Some("sheet") => Self::Sheet,
            Some("field" | "disabled") => Self::Field,
            Some("unreadable") => Self::Unknown,
            _ => Self::App,
        }
    }
}

/// What an agent does about an action refused because focus was
/// elsewhere: `sent` says whether any of the command went before it, and
/// `in_the_way` what was there instead.
pub fn focus_changed(sent: bool, in_the_way: InTheWay) -> &'static str {
    match (sent, in_the_way) {
        (false, InTheWay::App) => {
            "Another app was in front, maybe another agent's, and nothing of the command was sent. Click where your app shows, then run it again."
        }
        (false, InTheWay::Window) => {
            "Another window of your app had keyboard focus or was at the point, and nothing of the command was sent. Click in your window, then run it again."
        }
        (false, InTheWay::Sheet) => {
            "A sheet or dialog was open over your window, and nothing of the command was sent. Take a screenshot, deal with the sheet, then run it again."
        }
        (false, InTheWay::Field) => {
            "Another field had keyboard focus, or yours was disabled, and nothing of the command was sent. Click in your field, then run it again. If it keeps happening, the app may not say which field has focus (browsers and custom views often don't): use --window instead."
        }
        (false, InTheWay::Unknown) => {
            "Atlas couldn't tell what was in front, often because the active app has no window, and nothing of the command was sent. Click where your app shows, or open the app if none of it shows, then run it again."
        }
        (true, InTheWay::App) => {
            "Another app was in front, maybe another agent's. Earlier actions were sent: check typed or performed and what landed, click where your app shows, then send only the rest."
        }
        (true, InTheWay::Window) => {
            "Another window of your app took keyboard focus. Earlier actions were sent: check typed or performed and what landed, click in your window, then send only the rest."
        }
        (true, InTheWay::Sheet) => {
            "A sheet or dialog opened over your window. Earlier actions were sent: check typed or performed and what landed, deal with the sheet, then send only the rest."
        }
        (true, InTheWay::Field) => {
            "Focus left your field. Earlier actions were sent: check typed or performed and what landed, click in your field, then send only the rest."
        }
        (true, InTheWay::Unknown) => {
            "Atlas couldn't tell what was in front, often because the active app has no window. Earlier actions were sent: check typed or performed and what landed, click where your app shows or open the app if none of it shows, then send only the rest."
        }
    }
}

/// The next step for a failure with `code`.
pub fn fix(code: &str) -> Option<&'static str> {
    Some(match code {
        "agents" => "Run atlas agents to see which agents are installed.",
        "apps" | "capture" | "protocol" | "windows" => {
            "Try again. If it keeps failing, run atlas doctor."
        }
        "bad-request" | "usage" => "Add --help to the command to see what it accepts.",
        "cleanup" => "Run atlas cleanup --apply again, or see what was kept in its reply.",
        "blocked" => "Run atlas outside the sandbox that blocks it, such as an agent's.",
        "busy" => "Someone else has control, often you in the viewer. Release it there, or wait.",
        "cancelled" | "terminal" => return None,
        "config" | "setup" => "Run atlas init to finish setting up.",
        "config-invalid" => "Fix that file, or move it aside and run atlas init.",
        "daemon" | "state" => "Run atlas restart.",
        "elements" => {
            "Check the app is running with a window (atlas windows), then narrow it with --name or --role, or click the point from a screenshot instead."
        }
        "environment" => "Check what it names, then try again. Nothing is ever overwritten.",
        "focus-changed" => "Bring the app to the front, then try again.",
        "identity" | "native" | "restart" => "Run atlas doctor to see what is not ready.",
        "init" => "Run atlas init again. It picks up where it stopped.",
        "input" => "Check the action, take a screenshot, then try again.",
        "install" => "Move Atlas.app to /Applications, then try again.",
        "interrupted" => "Run it again if you still want it; look at the helper first.",
        "invalid-json" => "Check the JSON you passed.",
        "port-in-use" => "Pass --port with a free port, or stop what is using it.",
        "io" => "Check the path and its permissions, then try again.",
        "lease-lost" => "Control was taken back. Take a screenshot before trying again.",
        "logged-out" => {
            "Log in to it once, then switch back: atlas switch locks the screen for that."
        }
        "logout" => {
            "Look at the helper in the viewer. If it is still logged in, run atlas restart."
        }
        "logout-held" => {
            "Run atlas logout --force to end it anyway (unsaved work is lost), or atlas restart, then answer the app in the viewer."
        }
        "menubar" => "The rest of Atlas works without the menu bar.",
        "no-environment" => "Use atlas run, or set one up with atlas environment profile.",
        "no-window" => "Open a window in the app, or take a full screenshot.",
        "not-ready" => {
            "Run atlas doctor. A deleted account's login closes in Atlas › Settings › Helper or with atlas init; otherwise wait a minute and try again."
        }
        "on-console" => {
            "Switch back to your own account. Atlas works while the helper is in the background."
        }
        "onboarding" => "Open Atlas again to carry on setting up, or run atlas init.",
        "permission" => {
            "In the helper account, open System Settings › Privacy & Security and turn Atlas on. atlas init walks you through it."
        }
        "platform" => "Atlas runs on macOS only.",
        "session-changed" => "The helper changed meanwhile. Take a new screenshot and try again.",
        "spawn-failed" => "Check that the program or app exists in the helper account.",
        "stale-frame" => "Take a new screenshot.",
        "startup" => "Run atlas start-input, then atlas doctor.",
        "stopped" => "Choose Resume Agent in the Atlas menu bar when the agent may go on.",
        "switch" => "Use the Apple menu: Lock Screen, then Switch User.",
        "transport" => "Run atlas doctor. If an action was cut off, run atlas restart.",
        "uncertain" => "Take a screenshot to see what happened, then run atlas restart.",
        "uninstall" => "Fix what it names, then run atlas uninstall again.",
        "update" => "Try again later. Atlas never installs a build that fails its checks.",
        "wait-failed" => {
            "Look at the helper first (atlas windows, a screenshot), then wait or run it again."
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_has_a_next_step_for_an_agent() {
        for code in crate::error::CODES {
            let step = next(code, None);
            assert!(
                step.ends_with('.') && !step.contains('\n'),
                "{code}: {step}"
            );
        }
        assert_eq!(next("elements", None), fix("elements").unwrap());
        assert!(!next("made-up", None).is_empty());
    }

    /// Steps only the person may take (resuming agents, logging the helper
    /// in, setting Atlas up, choosing to lose work) are never an agent's.
    #[test]
    fn an_agent_is_never_told_to_do_what_only_the_person_may() {
        let stopped = next("stopped", None);
        assert!(stopped.contains("ask"), "{stopped}");
        assert!(!stopped.contains("Resume"), "{stopped}");

        let held = next("logout-held", None);
        let force = held.find("--force").unwrap();
        assert!(held[..force].contains("show the person"), "{held}");
        assert!(held.contains("only if they agree"), "{held}");

        for code in crate::error::CODES {
            let step = next(code, None);
            assert!(!step.starts_with("Run atlas init"), "{code}: {step}");
            assert!(!step.contains("in the viewer"), "{code}: {step}");
            assert!(!step.contains("Resume"), "{code}: {step}");
        }

        // Even when it may or may not have happened, an agent waiting for
        // the person is never told to act (atlas restart is not refused
        // while agents are stopped).
        for (code, holder) in [
            ("stopped", None),
            ("cancelled", None),
            ("on-console", None),
            ("busy", Some(Holder::Person)),
        ] {
            let said = step(code, holder, true);
            assert_eq!(said, next(code, holder), "{code}");
            assert!(!said.contains("atlas restart"), "{code}: {said}");
            assert!(
                said.contains("person") || said.contains("ask"),
                "{code}: {said}"
            );
        }
        assert_eq!(step("busy", Some(Holder::Agent), true), UNCERTAIN);
        assert_eq!(step("transport", None, true), UNCERTAIN);
        assert_eq!(step("transport", None, false), next("transport", None));
    }

    #[test]
    fn busy_says_who_to_wait_for() {
        assert!(next("busy", Some(Holder::Person)).contains("the person gives control back"));
        assert!(next("busy", Some(Holder::Agent)).contains("Another Atlas command"));
        let unknown = next("busy", None);
        assert!(unknown.contains("Never take control"), "{unknown}");
        assert_ne!(next("busy", None), fix("busy").unwrap(), "not the person's");
    }

    #[test]
    fn busy_after_waiting_says_atlas_already_waited() {
        assert!(WAITED.ends_with('.'), "{WAITED}");
        assert!(WAITED.contains("already waited"), "{WAITED}");
        assert!(!WAITED.contains("Resume"), "{WAITED}");
        assert!(!WAITED.contains("in the viewer"), "{WAITED}");
    }

    /// An earlier action's unknown outcome, perhaps another agent's, is
    /// recovered without asking only by its owner; an agent's own action
    /// that may or may not have happened is its own to recover.
    #[test]
    fn an_earlier_unknown_outcome_is_recovered_only_by_its_owner() {
        let said = next("uncertain", None);
        assert!(said.contains("only if that action was yours"), "{said}");
        assert!(said.contains("ask the person"), "{said}");
        assert_eq!(step("uncertain", None, false), said);
        assert_eq!(step("uncertain", None, true), UNCERTAIN);
    }

    /// Opening an app again does not bring it forward in the helper, and
    /// takes focus from whoever moved it; a click where the app shows does.
    #[test]
    fn a_focus_refusal_never_sends_an_agent_to_open_the_app() {
        for sent in [false, true] {
            for in_the_way in EVERY_WAY {
                let said = focus_changed(sent, in_the_way);
                assert!(!said.contains("atlas open"), "{said}");
            }
            for in_the_way in [InTheWay::App, InTheWay::Unknown] {
                let said = focus_changed(sent, in_the_way);
                assert!(
                    said.to_lowercase().contains("click where your app shows"),
                    "{said}"
                );
            }
        }
        assert_eq!(
            next("focus-changed", None),
            focus_changed(false, InTheWay::App)
        );
        assert_eq!(
            step("focus-changed", None, false),
            focus_changed(false, InTheWay::App)
        );
    }

    /// Every kind of thing a focus refusal can find in the way.
    const EVERY_WAY: [InTheWay; 5] = [
        InTheWay::App,
        InTheWay::Window,
        InTheWay::Sheet,
        InTheWay::Field,
        InTheWay::Unknown,
    ];

    /// A refusal for the window, a sheet or the field says what was in the
    /// way in its own words, and how to put it right.
    #[test]
    fn a_focus_refusal_says_what_was_in_the_way() {
        for (reason, said) in [
            (Some("app"), "Another app"),
            (Some("point"), "Another app"),
            (Some("window"), "Another window of your app"),
            (Some("sheet"), "A sheet or dialog"),
            (Some("field"), "Another field"),
            (Some("disabled"), "yours was disabled"),
            (Some("unreadable"), "Atlas couldn't tell"),
            (Some("made-up"), "Another app"),
            (None, "Another app"),
        ] {
            let hint = focus_changed(false, InTheWay::of(reason));
            assert!(hint.contains(said), "{reason:?}: {hint}");
        }
        assert!(focus_changed(true, InTheWay::Field).starts_with("Focus left your field"));
    }

    /// Only a refusal after something of the command was sent says so, and
    /// asks for the rest alone; one that sent nothing is run again whole.
    #[test]
    fn a_focus_refusal_says_earlier_actions_were_sent_only_when_they_were() {
        for in_the_way in EVERY_WAY {
            let sent = focus_changed(true, in_the_way);
            assert!(sent.contains("Earlier actions were sent"), "{sent}");
            assert!(sent.contains("send only the rest"), "{sent}");
            assert!(!sent.contains("run it again"), "{sent}");

            let nothing = focus_changed(false, in_the_way);
            assert!(!nothing.contains("Earlier actions"), "{nothing}");
            assert!(
                nothing.contains("nothing of the command was sent"),
                "{nothing}"
            );
            assert!(nothing.contains("run it again"), "{nothing}");
        }
    }

    /// When what is in front cannot be read, often because the active app
    /// has no window, the agent is told to bring its app up first.
    #[test]
    fn an_unreadable_front_asks_for_the_app_to_be_clicked_or_opened_first() {
        for sent in [false, true] {
            let said = focus_changed(sent, InTheWay::Unknown);
            assert!(
                said.starts_with("Atlas couldn't tell what was in front"),
                "{said}"
            );
            assert!(said.contains("open the app if none of it shows"), "{said}");
            let other = focus_changed(sent, InTheWay::App);
            assert!(!other.contains("open the app"));
            assert!(other.starts_with("Another app was in front"), "{other}");
        }
    }

    /// A command a signal stopped while its change was under way left that
    /// change unknown: it is the agent's own, to recover.
    #[test]
    fn an_interrupted_command_that_was_cut_is_told_to_recover_it() {
        assert_eq!(step("interrupted", None, true), UNCERTAIN);
        let said = step("interrupted", None, false);
        assert_eq!(said, next("interrupted", None));
        assert!(said.contains("nothing it did is left unknown"), "{said}");
        assert!(!said.contains("atlas restart"), "{said}");
    }

    /// Every code an agent can meet has a step for a person too.
    #[test]
    fn every_documented_code_has_a_fix() {
        for code in crate::error::CODES {
            assert!(
                fix(code).is_some() || ["cancelled", "terminal"].contains(code),
                "no fix for {code}"
            );
        }
    }
}
