use super::*;

#[test]
fn cursor_guards_mcp_person_actions_and_keeps_hook_arguments_separate() {
    use std::{fs, os::unix::fs::PermissionsExt, process::Command};

    let directory = tempfile::tempdir().unwrap();
    let atlas = directory.path().join("atlas");
    fs::write(&atlas, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
    fs::set_permissions(&atlas, fs::Permissions::from_mode(0o700)).unwrap();
    let launcher = atlas_core::app_bundle::checkout().join("agents/atlas/cursor/launch");
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
        let result = atlas_core::spawn::output(
            Command::new("/bin/sh")
                .arg(&launcher)
                .arg(event)
                .env_clear()
                .env("PATH", directory.path())
                .env("HOME", directory.path()),
        )
        .unwrap();
        assert!(result.status.success(), "{event}");
        assert_eq!(result.stdout, expected.as_bytes(), "{event}");
        assert!(result.stderr.is_empty(), "{event}");
    }
}

#[test]
fn cursor_uses_its_own_hook_schema_and_shared_skills_without_a_node_runtime() {
    use std::os::unix::fs::PermissionsExt;

    let root = atlas_core::app_bundle::checkout().join("agents/atlas");
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

    let reply = started(br#"{"conversation_id":"c-6f1e","session_id":"c-6f1e"}"#.to_vec()).await;
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
