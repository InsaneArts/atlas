//! Cursor loads a copied local plugin. Its settings and marketplace cache
//! stay untouched; registration is independent of a running host session.
use super::{ASK, Agent, Presence, Removal, connection_lock, problem};
use crate::{
    error::Result,
    host_session::{Host, HostSession},
};
use serde_json::{Value, json};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

mod store;

/// The variable Cursor's agent shells name their conversation in, when
/// Cursor passes a session start's `env` on to them.
const CONVERSATION: &str = "ATLAS_CURSOR_SESSION";
/// The most a hook's input may hold.
const HOOK_INPUT: u64 = 1 << 20;

/// Cursor's plugin loader uses HOME/.cursor even when its CLI authentication
/// configuration is redirected with CURSOR_CONFIG_DIR.
pub(super) fn home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(|home| PathBuf::from(home).join(".cursor"))
        .ok_or_else(|| problem("Cursor's home folder could not be found"))
}

pub(super) fn presence() -> Presence {
    let person = std::env::var_os("HOME").map(PathBuf::from);
    let applications = [
        Some(PathBuf::from("/Applications")),
        person.map(|p| p.join("Applications")),
    ];
    for directory in applications.into_iter().flatten() {
        let app = directory.join("Cursor.app");
        if app.join("Contents/MacOS/Cursor").is_file() {
            return Presence::Installed("Cursor app".into());
        }
    }
    if super::process::cursor_executable().is_none() {
        return Presence::Missing;
    }
    match super::process::exec(Agent::Cursor, &["--version"], ASK) {
        Ok(output) if output.status.success() => {
            Presence::Installed(String::from_utf8_lossy(&output.stdout).trim().into())
        }
        Err(error) if error.code == "agents" => Presence::Silent(error.error),
        _ => Presence::Missing,
    }
}

pub(super) fn status() -> Value {
    let mut row = match presence() {
        Presence::Installed(version) => {
            json!({"agent":"cursor", "installed":true, "version":version})
        }
        Presence::Silent(error) => json!({"agent":"cursor", "installed":true, "error":error}),
        Presence::Missing => json!({"agent":"cursor", "installed":false}),
    };
    match home().and_then(|root| store::connected(&root)) {
        Ok(connected) => row["connected"] = json!(connected),
        Err(error) => row["error"] = json!(error.error),
    }
    // A registration is not evidence that enterprise policy allowed it,
    // or that a particular running Cursor window loaded the plugin.
    row["verification"] = json!("registration");
    row
}

pub(super) fn recorded() -> bool {
    home().is_ok_and(|root| store::recorded(&root))
}

pub(super) fn connected() -> Result<bool> {
    store::connected(&home()?)
}

pub(super) fn change(marketplace: Option<&Path>) -> Result<Removal> {
    let _lock = connection_lock()?;
    if super::stopping() {
        return Err(problem("Atlas is stopping"));
    }
    store::change(&home()?, marketplace, || {
        if super::stopping() {
            Err(problem(
                "Atlas is stopping; reconnect Cursor to finish recovery",
            ))
        } else {
            Ok(())
        }
    })
}

/// Answer a Cursor command hook with its native JSON shape. The hidden CLI
/// route always requests JSON, so no human output or result envelope is added.
pub async fn hook(config: &Path, event: &str, input: impl Read) -> Value {
    match event {
        "session-start" => {
            let session = conversation(input);
            let readiness = super::brief(config, session.as_ref()).await;
            let mut reply = json!({"additional_context":format!(
                "{readiness}\nFor GUI work, read the atlas skill and automatically open the live view \
                 with show_screen in Cursor IDE before starting. Requests to watch, show the helper \
                 screen or take over mean open that view; do not ask the person to name a tool. \
                 To take over, the person clicks Controls, then Take Control. Cursor Agent CLI \
                 uses atlas monitor instead. Only the person operates the view's Stop, Resume, \
                 Take Control and Give Back; never call those MCP tools yourself."
            )});
            // The conversation, for the agent's shell commands to name as
            // their session; a label only.
            if let Some(session) = session {
                reply["env"] = json!({ CONVERSATION: session.id() });
            }
            reply
        }
        "before-shell" => guard(input),
        _ => denied("Atlas received an unknown Cursor hook event"),
    }
}

/// The conversation a session start names, as a host session, when its
/// `conversation_id` is a plain id. Input that cannot be read is none.
fn conversation(input: impl Read) -> Option<HostSession> {
    let value: Value = serde_json::from_slice(&hook_input(input)?).ok()?;
    HostSession::new(Host::Cursor, value["conversation_id"].as_str()?)
}

/// What Cursor sent a hook, when it is at most [`HOOK_INPUT`] bytes.
fn hook_input(input: impl Read) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    let read = input.take(HOOK_INPUT + 1).read_to_end(&mut bytes);
    (read.is_ok() && bytes.len() as u64 <= HOOK_INPUT).then_some(bytes)
}

fn denied(message: &str) -> Value {
    json!({"permission":"deny", "user_message":message, "agent_message":message})
}

fn guard(input: impl Read) -> Value {
    let Some(bytes) = hook_input(input) else {
        return denied("Atlas could not read Cursor's shell request");
    };
    let value: Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => return denied("Atlas received an invalid Cursor shell request"),
    };
    let Some(command) = value["command"].as_str() else {
        return denied("Atlas received a Cursor shell request without a command");
    };
    if command.contains("ATLAS_ACTOR") {
        return denied(
            "Only the person resumes agents or acts as the person in Atlas. Ask them to use the Atlas menu bar.",
        );
    }
    json!({"permission":"allow"})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_guards_mcp_person_actions_and_keeps_hook_arguments_separate() {
        use std::{fs, os::unix::fs::PermissionsExt, process::Command};

        let directory = tempfile::tempdir().unwrap();
        let atlas = directory.path().join("atlas");
        fs::write(&atlas, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
        fs::set_permissions(&atlas, fs::Permissions::from_mode(0o700)).unwrap();
        let launcher = Path::new(env!("CARGO_MANIFEST_DIR")).join("agents/atlas/cursor/launch");
        for (event, expected) in [
            ("mcp", "mcp\n--guard-app-controls\n"),
            (
                "session-start",
                "--json\nagents\ncursor-hook\nsession-start\n",
            ),
            (
                "before-shell",
                "--json\nagents\ncursor-hook\nbefore-shell\n",
            ),
        ] {
            let result = Command::new("/bin/sh")
                .arg(&launcher)
                .arg(event)
                .env_clear()
                .env("PATH", directory.path())
                .env("HOME", directory.path())
                .output()
                .unwrap();
            assert!(result.status.success(), "{event}");
            assert_eq!(result.stdout, expected.as_bytes(), "{event}");
            assert!(result.stderr.is_empty(), "{event}");
        }
    }

    #[test]
    fn cursor_uses_its_own_hook_schema_and_shared_skills_without_a_node_runtime() {
        use std::os::unix::fs::PermissionsExt;

        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("agents/atlas");
        let read = |name: &str| -> Value {
            serde_json::from_slice(&std::fs::read(root.join(name)).unwrap()).unwrap()
        };
        let manifest = read(".cursor-plugin/plugin.json");
        assert_eq!(manifest["skills"], "./skills/");
        assert_eq!(manifest["hooks"], "./cursor/hooks.json");
        assert_eq!(manifest["mcpServers"], "./cursor/mcp.json");
        let hooks = read("cursor/hooks.json");
        assert_eq!(hooks["version"], 1);
        assert!(hooks.get("modules").is_none());
        assert_eq!(
            hooks["hooks"]["beforeShellExecution"][0]["failClosed"],
            true
        );
        for (event, argument) in [
            ("sessionStart", "session-start"),
            ("beforeShellExecution", "before-shell"),
        ] {
            assert_eq!(
                hooks["hooks"][event][0]["command"],
                format!("\"${{CURSOR_PLUGIN_ROOT}}/cursor/launch\" {argument}")
            );
        }
        assert_eq!(
            read("cursor/mcp.json")["mcpServers"]["atlas"]["command"],
            "/bin/sh"
        );
        assert_ne!(
            std::fs::metadata(root.join("cursor/launch"))
                .unwrap()
                .permissions()
                .mode()
                & 0o111,
            0
        );
    }

    #[test]
    fn cursor_shell_hooks_refuse_actor_overrides_and_invalid_requests() {
        for command in ["ATLAS_ACTOR=viewer atlas resume", "printf '%s' ATLAS_ACTOR"] {
            let input = json!({"command":command}).to_string();
            assert_eq!(guard(input.as_bytes())["permission"], "deny");
        }
        for input in [b"not json".as_slice(), b"{}", b"{\"command\":1}"] {
            assert_eq!(guard(input)["permission"], "deny");
        }
        assert_eq!(
            guard(vec![b'x'; (1 << 20) + 1].as_slice())["permission"],
            "deny"
        );
        assert_eq!(
            guard(br#"{"command":"printf '\\nhello \\\"world\\\"'"}"#.as_slice())["permission"],
            "allow"
        );
    }

    #[tokio::test]
    async fn cursor_session_context_is_json_and_never_needs_a_configured_helper() {
        let directory = tempfile::tempdir().unwrap();
        let reply = hook(
            &directory.path().join("missing.toml"),
            "session-start",
            &b""[..],
        )
        .await;
        let said = reply["additional_context"].as_str().unwrap();
        assert!(said.contains("not set up"), "{said}");
        assert!(
            said.contains("the person clicks Controls, then Take Control"),
            "{said}"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&reply.to_string()).unwrap(),
            reply
        );
        assert!(reply.get("ok").is_none());
        // JSON serialization preserves arbitrary brief characters.
        let context = "A quote: \"; a slash: \\; a newline:\nქართული";
        let encoded = json!({"additional_context":context}).to_string();
        assert_eq!(
            serde_json::from_str::<Value>(&encoded).unwrap()["additional_context"],
            context
        );
    }

    #[tokio::test]
    async fn the_cursor_session_start_passes_the_conversation_to_agent_shells() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("missing.toml");
        let started = |input: Vec<u8>| hook(&config, "session-start", std::io::Cursor::new(input));

        let reply =
            started(br#"{"conversation_id":"c-6f1e","session_id":"c-6f1e"}"#.to_vec()).await;
        assert_eq!(reply["env"], json!({"ATLAS_CURSOR_SESSION": "c-6f1e"}));
        let context = reply["additional_context"].clone();

        let oversized = format!(
            r#"{{"conversation_id":"c-6f1e","pad":"{}"}}"#,
            "x".repeat(1 << 20)
        );
        for input in [
            br#"{"conversation_id":"two words"}"#.to_vec(),
            br#"{"conversation_id":"../up"}"#.to_vec(),
            br#"{"conversation_id":7}"#.to_vec(),
            b"not json".to_vec(),
            oversized.into_bytes(),
        ] {
            let reply = started(input).await;
            assert!(reply.get("env").is_none(), "{reply}");
            assert_eq!(
                reply["additional_context"], context,
                "the context is unchanged"
            );
        }
    }
}
