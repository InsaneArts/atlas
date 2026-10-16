//! What a person can do about each kind of failure, in one sentence. The
//! agent-facing table of the same codes is the plugin's troubleshooting.md;
//! a test keeps every code there covered here.

/// The next step for a failure with `code`.
pub fn fix(code: &str) -> Option<&'static str> {
    Some(match code {
        "agents" => "Run atlas agents to see which agents are installed.",
        "apps" | "capture" | "protocol" | "windows" => {
            "Try again. If it keeps failing, run atlas doctor."
        }
        "bad-request" | "usage" => "Add --help to the command to see what it accepts.",
        "blocked" => "Run atlas outside the sandbox that blocks it, such as an agent's.",
        "busy" => "Someone else has control, often you in the viewer. Release it there, or wait.",
        "cancelled" | "terminal" => return None,
        "config" | "setup" => "Run atlas init to finish setting up.",
        "config-invalid" => "Fix that file, or move it aside and run atlas init.",
        "daemon" | "state" => "Run atlas restart.",
        "elements" => {
            "Narrow it with --name or --role, or click the point from a screenshot instead."
        }
        "environment" => "Check what it names, then try again. Nothing is ever overwritten.",
        "identity" | "native" | "restart" => "Run atlas doctor to see what is not ready.",
        "init" => "Run atlas init again. It picks up where it stopped.",
        "input" => "Check the action, take a screenshot, then try again.",
        "install" => "Move Atlas.app to /Applications, then try again.",
        "invalid-json" => "Check the JSON you passed.",
        "port-in-use" => "Pass --port with a free port, or stop what is using it.",
        "io" => "Check the path and its permissions, then try again.",
        "lease-lost" => "Control was taken back. Take a screenshot before trying again.",
        "menubar" => "The rest of Atlas works without the menu bar.",
        "on-console" => {
            "Switch back to your own account. Atlas works while the helper is in the background."
        }
        "onboarding" => "Open Atlas again to carry on setting up, or run atlas init.",
        "permission" => {
            "In the helper account, open System Settings › Privacy & Security and turn Atlas on. atlas init walks you through it."
        }
        "platform" => "Atlas runs on macOS only.",
        "session-changed" => "The helper changed meanwhile. Take a new screenshot and try again.",
        "spawn-failed" => "Check that the program exists in the helper account.",
        "stale-frame" => "Take a new screenshot.",
        "startup" => "Run atlas start-input, then atlas doctor.",
        "switch" => "Use the Apple menu: Lock Screen, then Switch User.",
        "transport" => "Run atlas doctor. If an action was cut off, run atlas restart.",
        "uncertain" => "Take a screenshot to see what happened, then run atlas restart.",
        "uninstall" => "Fix what it names, then run atlas uninstall again.",
        "update" => "Try again later. Atlas never installs a build that fails its checks.",
        "wait-failed" => "Check what the command changed before running it again.",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
