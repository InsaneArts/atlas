//! Cursor loads a copied local plugin. Its settings and marketplace cache
//! stay untouched; registration is independent of a running host session.
use super::{Agent, Presence, Probe, Removal, connection_lock, problem};
use crate::host_session::{Host, HostSession};
use atlas_core::error::Result;
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
    super::process::presence(Agent::Cursor)
}

pub(super) fn probe() -> Probe {
    Probe {
        presence: presence(),
        connection: Some(home().and_then(|root| store::connected(&root)).map(Some)),
        // A registration is not evidence that enterprise policy allowed it,
        // or that a particular running Cursor window loaded the plugin.
        verification: Some("registration"),
    }
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
    atlas_core::files::read_limited(input, HOOK_INPUT)
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
mod tests;
